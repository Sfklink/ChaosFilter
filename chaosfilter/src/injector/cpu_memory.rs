//! PID → cgroup v2 injector.
//!
//! Creates/uses a target cgroup, optionally moves a PID into it, writes cpu/memory knobs,
//! and can revert by restoring previous knob values (best effort).

use crate::plans::Plan;
use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tracing::{debug, info, warn};

#[derive(Default)]
pub struct MemoryConfig {
    applied: bool,

    // what we operated on
    pid: Option<u32>,
    target_cg: Option<PathBuf>,

    // revert state
    original_pid_cg: Option<PathBuf>,
    prev_cpu_max: Option<String>,
    prev_cpu_weight: Option<String>,
    prev_mem_max: Option<String>,
    prev_mem_high: Option<String>,
    prev_swap_max: Option<String>,
}

/*
TODO:
    Create validate.rs, and move validation functions there.
 */
pub fn validate_memory_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.memory_config.enabled {
        return Ok(());
    }

    let pid = plan.injectors.memory_config.target_pid.ok_or_else(|| {
        anyhow!("memory_config.enabled=true requires injectors.memory_config.pid")
    })?;

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
        .ok_or_else(|| anyhow!("memory_config.enabled=true requires targets.cgroup"))?;

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

impl MemoryConfig {
    //ugly debuggers dont even look at it

    fn assert_domain_cgroup(cg: &Path) -> Result<()> {
        let ty = fs::read_to_string(cg.join("cgroup.type")).unwrap_or_default();
        if ty.contains("threaded") {
            return Err(anyhow!(
                "target cgroup {} is threaded ('{}'); cannot move PID via cgroup.procs (need cgroup.threads / TIDs)",
                cg.display(),
                ty.trim()
            ));
        }
        Ok(())
    }
    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        if !plan.injectors.memory_config.enabled {
            return Ok(());
        }

        validate_memory_config(plan)?;

        let pid = plan.injectors.memory_config.target_pid.unwrap();
        let cg_rel = plan.targets.cgroup.as_deref().unwrap();
        let cg = resolve_cgroup_path(cg_rel);

        info!(
            cgroup = %cg.display(),
            pid,
            "applying cgroup knobs"
        );

        ensure_cgroup_dir_exists(&cg).context("failed to create/ensure cgroup directory")?;

        // enable controllers on parent (best effort; fail is real because writing new cgroup vals may fail
        if !plan.injectors.memory_config.enable.is_empty() {
            enable_controllers_on_parent(&cg, &plan.injectors.memory_config.enable)
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
        debug!("revert state captured");

        // more troubleshooting
        Self::assert_domain_cgroup(&cg)?;

        // optionally move pid (idempotent)
        // that means it only does one thing one time instead of  repeating itself
        if plan.injectors.memory_config.move_pid {
            match read_pid_cgroup_v2(pid) {
                Some(cur) if cur == cg => {
                    debug!(
                        pid,
                        cgroup = %cg.display(),
                        "PID already in target cgroup, skipping move"
                    );
                }
                _ => {
                    move_pid_into_cgroup(&cg, pid).context("failed to move PID into cgroup")?;
                }
            }
        }

        // checking flag characters because it keeps throwing an error for bad input to syscalls
        // IT WAS NEWLINES
        // DAMMIT NEWLINES
        // CURSE YOU NEWLINES
        if let Some(v) = plan.injectors.memory_config.cpu_max.as_deref() {
            debug!(
                raw = ?v,
                bytes = ?v.as_bytes(),
                "cpu.max raw value"
            );

            write_line(cg.join("cpu.max"), v)
                .with_context(|| format!("failed writing cpu.max='{}' at {}", v, cg.display()))?;
        }

        // write knobs (only if present)
        debug!("writing cpu.weight");
        if let Some(v) = plan.injectors.memory_config.cpu_max.as_deref() {
            write_line(cg.join("cpu.max"), v)
                .with_context(|| format!("failed writing cpu.max='{}' at {}", v, cg.display()))?;
        }

        debug!("writing cpu.weight");
        if let Some(w) = plan.injectors.memory_config.cpu_weight {
            write_line(cg.join("cpu.weight"), &w.to_string())?;
        }

        debug!("writing memory.max");
        if let Some(v) = plan.injectors.memory_config.mem_max.as_deref() {
            write_line(cg.join("memory.max"), v)?;
        }

        debug!("writing memory.high");
        if let Some(v) = plan.injectors.memory_config.mem_high.as_deref() {
            write_line(cg.join("memory.high"), v)?;
        }

        debug!("writing memory.swap.max");
        if let Some(v) = plan.injectors.memory_config.swap_max.as_deref() {
            write_line(cg.join("memory.swap.max"), v)?;
        }

        self.applied = true;

        debug!(
            cpu_max     = ?read_trimmed_opt(cg.join("cpu.max")),
            cpu_weight  = ?read_trimmed_opt(cg.join("cpu.weight")),
            mem_max     = ?read_trimmed_opt(cg.join("memory.max")),
            mem_high    = ?read_trimmed_opt(cg.join("memory.high")),
            swap_max    = ?read_trimmed_opt(cg.join("memory.swap.max")),
            "post-apply cgroup state"
        );

        let dur = plan.schedule.duration_s;
        info!(duration_s = dur, "holding chaos");
        std::thread::sleep(std::time::Duration::from_secs(dur));
        info!("duration elapsed; reverting");

        Ok(())
    }

    pub fn revert(&mut self) -> Result<()> {
        if !self.applied {
            debug!("nothing applied; skipping revert");
            return Ok(());
        }

        let pid = self
            .pid
            .ok_or_else(|| anyhow!("internal error: pid missing"))?;
        let cg = self
            .target_cg
            .clone()
            .ok_or_else(|| anyhow!("internal error: target_cg missing"))?;

        info!(
            cgroup = %cg.display(),
            pid,
            "reverting cgroup knobs"
        );

        // restore original (best effort-ish: if file exists, try write)
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
                    warn!(
                        pid,
                        original_cgroup = %orig.display(),
                        error = %e,
                        "failed moving PID back to original cgroup"
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

//new write_line that debugs EVEN MORE BETTER
// so the last failure was due to appending newlines on to it which makes cgroups SUPER TEMPERAMENTAL
fn write_line(path: impl AsRef<Path>, value: &str) -> std::io::Result<()> {
    use std::io::Write;

    let path = path.as_ref();
    let mut f = fs::OpenOptions::new().write(true).open(path)?;

    // cgroup expects exact tokens, no CRLF, no surrounding whitespace.
    let v = value.trim();

    // One single write to avoid cgroupfs rejecting split writes.
    let mut buf = Vec::with_capacity(v.len() + 1);
    buf.extend_from_slice(v.as_bytes());
    buf.push(b'\n');

    f.write_all(&buf)?;
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
        write_line(path, val)?; // converts std::io::Error -> anyhow::Error
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

fn move_pid_into_cgroup(cg: &Path, pid: u32) -> std::io::Result<()> {
    let path = cg.join("cgroup.procs");
    let mut f = fs::OpenOptions::new().write(true).open(path)?;

    let s = format!("{pid}\n");
    f.write_all(s.as_bytes())?;
    f.flush()?;
    Ok(())
}

/// Reads the cgroup v2 path for a PID and returns the absolute cgroup directory.
/// For v2, /proc/<pid>/cgroup has a line like: `0::/some/path`
fn read_pid_cgroup_v2(pid: u32) -> Option<PathBuf> {
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

// apply and revert
// called from main
pub fn run_plan(plan: &Plan) -> Result<()> {
    if !plan.injectors.memory_config.enabled {
        debug!("memory injector not enabled; skipping");
        return Ok(());
    }
    validate_memory_config(plan)?;

    let mut cg = MemoryConfig::default();
    cg.apply(plan)?;

    std::thread::sleep(std::time::Duration::from_secs(plan.schedule.duration_s));

    cg.revert()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::plans::{Injectors, MemoryConfig as MemCfg, NetworkConfig as NetCfg, Schedule, Targets};

    fn base_plan() -> Plan {
        Plan {
            name: "test".to_string(),
            targets: Targets {
                cgroup: None,
                iface: None,
            },
            schedule: Schedule { duration_s: 0 },
            injectors: Injectors {
                network_config: NetCfg::default(),
                memory_config: MemCfg::default(),
                block_config: Default::default(),
                filesystem_config: Default::default(),
            },
        }
    }

    #[test]
    fn resolve_cgroup_path_absolute() {
        let cgroup_path = resolve_cgroup_path("/tmp/test");
        assert_eq!(cgroup_path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn resolve_cgroup_path_relative() {
        let p = resolve_cgroup_path("test");
        assert_eq!(p, Path::new("/sys/fs/cgroup").join("test"));
    }

    #[test]
    fn ensure_cgroup_dir_exists_test_create_if_missing() {
        let tempdir = TempDir::new().unwrap();
        let cgroup = tempdir.path().join("test");

        assert!(!cgroup.exists());

        ensure_cgroup_dir_exists(&cgroup).unwrap();
        assert!(cgroup.exists());
        assert!(cgroup.is_dir());
    }

    #[test]
    fn write_line_test() {
        let tempdir = TempDir::new().unwrap();
        let filepath = tempdir.path().join("cpu.max");
        fs::write(&filepath, "").unwrap();

        write_line(&filepath, "  max 100000  \n").unwrap();

        let read = fs::read_to_string(&filepath).unwrap();
        assert_eq!(read, "max 100000\n");
    }

    #[test]
    fn write_line_fails_if_file_missing() {
        let tempdir = TempDir::new().unwrap();
        let filepath = tempdir.path().join("does_not_exist");

        let err = write_line(&filepath, "x").unwrap_err();
        assert!(matches!(err.kind(), std::io::ErrorKind::NotFound));
    }

    #[test]
    fn read_trimmed_opt_test() {
        let tempdir = TempDir::new().unwrap();
        let filepath = tempdir.path().join("does_not_exist");
        assert!(read_trimmed_opt(filepath).is_none());
    }

    #[test]
    fn restore_opt_noop_if_target_file_missing() {
        let tempdir: TempDir = TempDir::new().unwrap();
        let filepath = tempdir.path().join("does_not_exist");

        restore_opt(filepath, Some("test")).unwrap();
    }

    #[test]
    fn restore_opt_writes_when_present_and_value_some() {
        let tempdir = TempDir::new().unwrap();
        let filepath = tempdir.path().join("present");
        fs::write(&filepath, "old\n").unwrap();

        restore_opt(filepath.clone(), Some("new")).unwrap();
        assert_eq!(fs::read_to_string(&filepath).unwrap(), "new\n");
    }

    #[test]
    fn restore_opt_noop_when_value_none() {
        let tempdir = TempDir::new().unwrap();
        let filepath = tempdir.path().join("present");
        fs::write(&filepath, "old\n").unwrap();

        restore_opt(filepath.clone(), None).unwrap();
        assert_eq!(fs::read_to_string(&filepath).unwrap(), "old\n");
    }

    #[test]
    fn enable_controllers_on_parent_errors_when_no_parent() {
        let err = enable_controllers_on_parent(
            Path::new("/"),
            &["cpu".to_string()]
        )
        .unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn enable_controllers_on_parent_writes_plus_controller_lines() {
        let tempdir = TempDir::new().unwrap();
        let parent = tempdir.path().join("parent");
        let cgroup = parent.join("child");
        fs::create_dir_all(&cgroup).unwrap();

        let subtree = parent.join("cgroup.subtree_control");
        fs::write(&subtree, "").unwrap();

        enable_controllers_on_parent(&cgroup, &["cpu".to_string(), "memory".to_string()]).unwrap();

        let contents = fs::read_to_string(&subtree).unwrap();
        assert!(contents.contains("+memory"));
    }

    #[test]
    fn move_pid_into_cgroup_writes_pid_to_cgroup_procs_file() {
        let tempdir = TempDir::new().unwrap();
        let cgroup = tempdir.path().join("cg");
        fs::create_dir_all(&cgroup).unwrap();

        let procs = cgroup.join("cgroup.procs");
        fs::write(&procs, "").unwrap();

        move_pid_into_cgroup(&cgroup, 1234).unwrap();
        assert_eq!(fs::read_to_string(&procs).unwrap(), "1234\n");
    }

    #[test]
    fn read_pid_cgroup_v2_returns_some_for_current_pid_on_v2_hosts() {
        let pid = std::process::id();
        let cgroup = read_pid_cgroup_v2(pid);
        assert!(cgroup.is_some());
    }

    #[test]
    fn read_pid_cgroup_v2_returns_none_for_nonexistent_pid() {
        let cgroup = read_pid_cgroup_v2(4_000_000_000u32);
        assert!(cgroup.is_none());
    }

    #[test]
    fn assert_domain_cgroup_ok_when_type_not_threaded() {
        let tempdir = TempDir::new().unwrap();
        let cgroup: PathBuf = tempdir.path().join("cg");
        fs::create_dir_all(&cgroup).unwrap();
        fs::write(cgroup.join("cgroup.type"), "domain\n").unwrap();

        MemoryConfig::assert_domain_cgroup(&cgroup).unwrap();
    }

    #[test]
    fn assert_domain_cgroup_errors_when_threaded() {
        let tempdir = TempDir::new().unwrap();
        let cgroup = tempdir.path().join("cg");
        fs::create_dir_all(&cgroup).unwrap();
        fs::write(cgroup.join("cgroup.type"), "threaded\n").unwrap();

        let err = MemoryConfig::assert_domain_cgroup(&cgroup).unwrap_err().to_string();
        assert!(err.contains("threaded"));
        assert!(err.contains("cannot move PID"));
    }

    #[test]
    fn validate_memory_config_ok_when_disabled() {
        let plan = base_plan();
        validate_memory_config(&plan).unwrap();
    }

    #[test]
    fn validate_memory_config_errors_when_enabled_missing_pid() {
        let mut plan = base_plan();
        plan.injectors.memory_config.enabled = true;
        plan.targets.cgroup = Some("testcg".to_string());

        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires injectors.memory_config.pid"));
    }

    #[test]
    fn validate_memory_config_errors_when_enabled_pid_missing_in_proc() {
        let mut plan = base_plan();
        plan.injectors.memory_config.enabled = true;
        plan.injectors.memory_config.target_pid = Some(4_000_000_000u32);
        plan.targets.cgroup = Some("testcg".to_string());

        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("PID does not exist"));
    }

    #[test]
    fn validate_memory_config_errors_when_enabled_missing_targets_cgroup() {
        let mut plan = base_plan();
        plan.injectors.memory_config.enabled = true;
        plan.injectors.memory_config.target_pid = Some(std::process::id());

        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires targets.cgroup"));
    }

    #[test]
    fn validate_memory_config_errors_when_parent_cgroup_missing() {
        let mut plan = base_plan();
        plan.injectors.memory_config.enabled = true;
        plan.injectors.memory_config.target_pid = Some(std::process::id());

        plan.targets.cgroup = Some("does_not_exist/child".to_string());

        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("parent cgroup directory does not exist"));
    }
}
