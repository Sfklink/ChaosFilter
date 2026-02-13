//! PID → cgroup v2 injector.
//!
//! Creates/uses a target cgroup, optionally moves a PID into it, writes cpu/memory knobs,
//! and can revert by restoring previous knob values (best effort).

use anyhow::{anyhow, Context, Result};
use chaosfilter_common::{validate_plan, Plan};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct PidCgroupKnobs {
    applied: bool,

    // what we operated on
    pid: Option<i32>,
    target_cg: Option<PathBuf>,

    // revert state
    original_pid_cg: Option<PathBuf>,
    prev_cpu_max: Option<String>,
    prev_cpu_weight: Option<String>,
    prev_mem_max: Option<String>,
    prev_mem_high: Option<String>,
    prev_swap_max: Option<String>,
}

pub fn run_plan(plan: &Plan) -> Result<()> {
    validate_plan(plan)?;

    if !plan.injectors.cgroup_knobs.enabled {
        return Err(anyhow!("injectors.cgroup_knobs.enabled is false; nothing to do"));
    }

    let mut cg = PidCgroupKnobs::default();
    cg.apply(plan)?;

    std::thread::sleep(std::time::Duration::from_secs(plan.schedule.duration_s));

    cg.revert()?;
    Ok(())
}

impl PidCgroupKnobs {
    pub fn validate(plan: &Plan) -> Result<()> {
        if !plan.injectors.cgroup_knobs.enabled {
            return Ok(());
        }

        let pid = plan
            .injectors
            .cgroup_knobs
            .pid
            .ok_or_else(|| anyhow!("cgroup_knobs.enabled=true requires injectors.cgroup_knobs.pid"))?;

        // quick pid existence check
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return Err(anyhow!("PID does not exist: {pid}"));
        }

        // quick cgroup v2 check
        if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
            return Err(anyhow!(
                "cgroup v2 not detected: /sys/fs/cgroup/cgroup.controllers missing"
            ));
        }

        // target cgroup required if enabled
        let cg_rel = plan
            .targets
            .cgroup
            .as_deref()
            .ok_or_else(|| anyhow!("cgroup_knobs.enabled=true requires targets.cgroup"))?;

        let cg = resolve_cgroup_path(cg_rel);

        // directory may not exist yet; that's fine (apply creates it)
        // but parent must exist
        let parent = cg
            .parent()
            .ok_or_else(|| anyhow!("invalid cgroup path (no parent): {}", cg.display()))?;
        if !parent.exists() {
            return Err(anyhow!(
                "parent cgroup directory does not exist: {}",
                parent.display()
            ));
        }

        Ok(())
    }

    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        if !plan.injectors.cgroup_knobs.enabled {
            return Ok(());
        }

        Self::validate(plan)?;

        let pid = plan.injectors.cgroup_knobs.pid.unwrap();
        let cg_rel = plan.targets.cgroup.as_deref().unwrap();
        let cg = resolve_cgroup_path(cg_rel);

        println!("[cgroup] applying knobs to {} for PID {}", cg.display(), pid);

        ensure_cgroup_dir_exists(&cg).context("failed to create/ensure cgroup directory")?;

        // enable controllers on parent (best effort; fail is real because writing knobs may fail anyway)
        if !plan.injectors.cgroup_knobs.enable.is_empty() {
            enable_controllers_on_parent(&cg, &plan.injectors.cgroup_knobs.enable)
                .context("failed enabling controllers on parent cgroup.subtree_control")?;
        }

        // snapshot revert state
        self.prev_cpu_max = read_trimmed_opt(cg.join("cpu.max"));
        self.prev_cpu_weight = read_trimmed_opt(cg.join("cpu.weight"));
        self.prev_mem_max = read_trimmed_opt(cg.join("memory.max"));
        self.prev_mem_high = read_trimmed_opt(cg.join("memory.high"));
        self.prev_swap_max = read_trimmed_opt(cg.join("memory.swap.max"));

        self.original_pid_cg = read_pid_cgroup_v2(pid);
        self.pid = Some(pid);
        self.target_cg = Some(cg.clone());
        println!("[DEBUG] Revert state set");



        // optionally move pid (idempotent)
        // that means it only does one thing one time instead of  repeating itself
        if plan.injectors.cgroup_knobs.move_pid {
            match read_pid_cgroup_v2(pid) {
                Some(cur) if cur == cg => {
                    println!(
                        "[cgroup] PID {} already in {}, skipping move",
                        pid,
                        cg.display()
                    );
                }
                _ => {
                    move_pid_into_cgroup(&cg, pid).context("failed to move PID into cgroup")?;
                }
            }
        }


        // write knobs (only if present)
        println!("[DEBUG] [cgroup] writing cpu.max...");
        if let Some(v) = plan.injectors.cgroup_knobs.cpu_max.as_deref() {
            write_line(cg.join("cpu.max"), v)
                .with_context(|| format!("failed writing cpu.max='{}' at {}", v, cg.display()))?;
        }

        println!("[DEBUG] [cgroup] writing cpu.weight...");
        if let Some(w) = plan.injectors.cgroup_knobs.cpu_weight {
            write_line(cg.join("cpu.weight"), &w.to_string())?;
        }

        println!("[DEBUG] [cgroup] writing memory.max...");
        if let Some(v) = plan.injectors.cgroup_knobs.mem_max.as_deref() {
            write_line(cg.join("memory.max"), v)?;
        }
        println!("[DEBUG] [cgroup] writing memory.high...");

        if let Some(v) = plan.injectors.cgroup_knobs.mem_high.as_deref() {
            write_line(cg.join("memory.high"), v)?;
        }
        println!("[DEBUG] [cgroup] writing memory.swap.max...");

        if let Some(v) = plan.injectors.cgroup_knobs.swap_max.as_deref() {
            write_line(cg.join("memory.swap.max"), v)?;
        }

        self.applied = true;

        // tiny verification print (like qdisc does)
        println!("[cgroup] post-state:");
        maybe_print(&cg.join("cpu.max"), "  cpu.max");
        maybe_print(&cg.join("cpu.weight"), "  cpu.weight");
        maybe_print(&cg.join("memory.max"), "  memory.max");
        maybe_print(&cg.join("memory.high"), "  memory.high");
        maybe_print(&cg.join("memory.swap.max"), "  memory.swap.max");

        Ok(())
    }

    pub fn revert(&mut self) -> Result<()> {
        if !self.applied {
            println!("[cgroup] nothing applied; skipping revert");
            return Ok(());
        }

        let pid = self.pid.ok_or_else(|| anyhow!("internal error: pid missing"))?;
        let cg = self
            .target_cg
            .clone()
            .ok_or_else(|| anyhow!("internal error: target_cg missing"))?;

        println!("[cgroup] reverting knobs on {} for PID {}", cg.display(), pid);

        // restore knobs (best effort-ish: if file exists, try write)
        restore_opt(cg.join("cpu.max"), self.prev_cpu_max.as_deref())?;
        restore_opt(cg.join("cpu.weight"), self.prev_cpu_weight.as_deref())?;
        restore_opt(cg.join("memory.max"), self.prev_mem_max.as_deref())?;
        restore_opt(cg.join("memory.high"), self.prev_mem_high.as_deref())?;
        restore_opt(cg.join("memory.swap.max"), self.prev_swap_max.as_deref())?;

        // move pid back to its original cgroup if captured it
        if let Some(orig) = self.original_pid_cg.as_ref() {
            // writing PID to cgroup.procs moves it
            if orig.exists() {
                if let Err(e) = move_pid_into_cgroup(orig, pid) {
                    eprintln!(
                        "[cgroup] warning: failed moving PID {} back to {}: {}",
                        pid,
                        orig.display(),
                        e
                    );
                }
            }
        }

        self.applied = false;
        Ok(())
    }
}

/* ------------------------- helpers ------------------------- */

fn resolve_cgroup_path(arg: &str) -> PathBuf {
    let p = PathBuf::from(arg);
    if p.is_absolute() {
        p
    } else {
        Path::new("/sys/fs/cgroup").join(p)
    }
}

fn ensure_cgroup_dir_exists(cg: &Path) -> std::io::Result<()> {
    if !cg.exists() {
        fs::create_dir_all(cg)?;
    }
    Ok(())
}

//new write_line that debugs better
fn write_line(path: impl AsRef<Path>, value: &str) -> std::io::Result<()> {
    use std::io::Write;

    let path = path.as_ref();

    // cgroup fs can be picky: avoid append/truncate/create; write once with LF newline.
    let mut f = fs::OpenOptions::new().write(true).open(path)?;

    let v = value.trim_end_matches('\r'); // prevent CRLF issues
    write!(f, "{}\n", v)?;

    // Ensure the write is pushed immediately
    f.flush()?;

    Ok(())
}


fn read_trimmed_opt(path: PathBuf) -> Option<String> {
    let mut s = String::new();
    let mut f = fs::OpenOptions::new().read(true).open(&path).ok()?;
    f.read_to_string(&mut s).ok()?;
    Some(s.trim().to_string())
}

fn restore_opt(path: PathBuf, v: Option<&str>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if let Some(val) = v {
        write_line(path, val)?;   // converts std::io::Error -> anyhow::Error
    }
    Ok(())
}

fn enable_controllers_on_parent(cg: &Path, controllers: &[String]) -> std::io::Result<()> {
    let parent = cg.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "cgroup has no parent")
    })?;

    let subtree = parent.join("cgroup.subtree_control");

    // one per write: robust under various kernels/setups
    for c in controllers {
        let line = format!("+{c}\n");
        let mut f = fs::OpenOptions::new().write(true).open(&subtree)?;
        f.write_all(line.as_bytes())?;
    }
    Ok(())
}

fn move_pid_into_cgroup(cg: &Path, pid: i32) -> std::io::Result<()> {
    let path = cg.join("cgroup.procs");
    let mut f = fs::OpenOptions::new().write(true).open(path)?;
    write!(f, "{pid}\n")?;
    Ok(())
}

/// Reads the cgroup v2 path for a PID and returns the absolute cgroup directory.
/// For v2, /proc/<pid>/cgroup has a line like: `0::/some/path`
fn read_pid_cgroup_v2(pid: i32) -> Option<PathBuf> {
    let p = format!("/proc/{pid}/cgroup");
    let contents = fs::read_to_string(p).ok()?;
    for line in contents.lines() {
        // v2 unified hierarchy
        if let Some(rest) = line.strip_prefix("0::") {
            let rel = rest.trim();
            return Some(Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/')));
        }
    }
    None
}

fn maybe_print(path: &Path, name: &str) {
    match fs::read_to_string(path) {
        Ok(v) => println!("{name}: {}", v.trim()),
        Err(e) => println!("{name}: <unreadable: {e}>"),
    }
}
