use crate::injector::ChaosInjector;
use crate::plans::Plan;
use anyhow::{anyhow, bail, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

fn major_minor(device_path: &str) -> Result<String> {
    let path = Path::new(device_path);

    if !path.exists() {
        bail!("Device path not real: {}", device_path);
    }

    let metadata = fs::metadata(path)?;
    let rdev = metadata.rdev();
    let major = libc::major(rdev);
    let minor = libc::minor(rdev);

    Ok(format!("{}:{}", major, minor))
}

fn io_enabled() -> Result<()> {
    let root_subtree = Path::new("/sys/fs/cgroup/cgroup.subtree_control");
    let subtree = fs::read_to_string(root_subtree)?;

    if !subtree.contains("io") {
        warn!(
            subtree_path = %root_subtree.display(),
            "IO controller is not enabled; run: sudo sh -c 'echo +io > {}'",
            root_subtree.display()
        );

        bail!("IO controller not enabled");
    }

    Ok(())
}

fn run_disk_test(label: &str) -> Result<f64> {
    info!(label, "starting disk test");

    let start = Instant::now();

    let output = Command::new("dd").args(["if=/dev/zero", "of=testfile", "bs=1M", "count=200", "oflag=direct",]).output()?;

    let duration = start.elapsed().as_secs_f64();

    if !output.status.success() {
        bail!("dd command failed");
    }

    let mb_written = 200.0;
    let mbps = mb_written / duration;

    info!(
        label,
        duration_secs = duration,
        throughput_mbps = mbps,
        "disk test complete"
    );

    let _ = fs::remove_file("testfile");

    Ok(mbps)
}

#[derive(Default)]
pub struct BlockDelayInjector {
    applied: bool,
    io_max_path: Option<PathBuf>,
    saved_io_max: Option<String>,
    control_speed_mbps: Option<f64>,
}

impl ChaosInjector for BlockDelayInjector {
    fn name(&self) -> &'static str {
        "block_delay"
    }

    fn apply(&mut self, plan: &Plan) -> Result<()> {
        let cfg = &plan.injectors.block_config;

        if !cfg.enabled {
            debug!("block delay injector not enabled; skipping");
            return Ok(());
        }

        io_enabled()?;

        let device = cfg.device.as_ref().expect("Device must be specified");
        let major_minor = major_minor(device)?;
        info!(
            device,
            major_minor,
            "resolved block device"
        );

        let base_path = Path::new("/sys/fs/cgroup/chaosfilter");
        let cgroup_path = base_path.join(&plan.name);
        info!(
            cgroup = ?cgroup_path,
            "creating cgroup"
        );

        if !base_path.exists() {
            fs::create_dir(base_path)?;
            info!("created base chaosfilter cgroup directory");
        }

        debug!(
            base = ?base_path,
            "checking io controller availability"
        );

        let chaos_subtree = base_path.join("cgroup.subtree_control");

        if chaos_subtree.exists() {
            let content = fs::read_to_string(&chaos_subtree)?;

            if !content.contains("io") {
                info!("enabling io controller in chaosfilter subtree");

                let mut file = fs::OpenOptions::new().write(true).open(&chaos_subtree)?;
                file.write_all(b"+io")?;
            }
        } else {
            warn!(
                path = ?chaos_subtree,
                "chaosfilter subtree_control not available yet"
            );
        }

        if !cgroup_path.exists() {
            fs::create_dir(&cgroup_path)?;
            info!(
                cgroup = ?cgroup_path,
                "created plan cgroup directory"
            );
        } else {
            debug!(
                cgroup = ?cgroup_path,
                "plan cgroup already exists"
            );
        }

        let io_max = cgroup_path.join("io.max");
        if !io_max.exists() {
            warn!(
                path = ?io_max,
                "io.max not found; the io controller may not be enabled"
            );

            return Ok(());
        }

        let control_speed = run_disk_test("CONTROL")?;

        let self_pid = std::process::id();
        let procs_path = cgroup_path.join("cgroup.procs");
        fs::write(&procs_path, self_pid.to_string())?;
        info!(
            pid = self_pid,
            cgroup = ?cgroup_path,
            "moved current process into cgroup"
        );

        let current = fs::read_to_string(&io_max)?;
        debug!(
            contents = current.trim(),
            "current io.max"
        );

        let mut rule = format!("{}", major_minor);

        if let Some(rbps) = cfg.rbps {
            rule.push_str(&format!(" rbps={}", rbps));
        }
        if let Some(wbps) = cfg.wbps {
            rule.push_str(&format!(" wbps={}", wbps));
        }
        if let Some(riops) = cfg.riops {
            rule.push_str(&format!(" riops={}", riops));
        }
        if let Some(wiops) = cfg.wiops {
            rule.push_str(&format!(" wiops={}", wiops));
        }

        debug!(
            rule,
            "applying throttle rule"
        );

        {
            let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
            file.write_all(rule.as_bytes())?;
        }

        self.applied = true;
        self.io_max_path = Some(io_max);
        self.saved_io_max = Some(current);
        self.control_speed_mbps = Some(control_speed);

        thread::sleep(Duration::from_secs(1));

        Ok(())
    }

    fn revert(&mut self) -> Result<()> {
        if !self.applied {
            return Ok(());
        }

        let control_speed = self
            .control_speed_mbps
            .ok_or_else(|| anyhow!("internal error: block_delay control speed missing"))?;

        let experimental_speed = run_disk_test("EXPERIMENTAL")?;

        let io_max = self
            .io_max_path
            .as_ref()
            .ok_or_else(|| anyhow!("internal error: block_delay io_max path missing"))?;
        let saved = self
            .saved_io_max
            .as_ref()
            .ok_or_else(|| anyhow!("internal error: block_delay saved io.max missing"))?;

        info!("restoring original io.max");
        {
            let mut file = fs::OpenOptions::new().write(true).open(io_max)?;
            file.write_all(saved.as_bytes())?;
        }

        info!("block throttling applied and reverted");

        let drop = control_speed - experimental_speed;
        let percent = (drop / control_speed) * 100.0;

        let results = format!(
            "\n========== RESULTS ==========\n\
            Control Speed:      {:.2} MB/s\n\
            Experimental Speed: {:.2} MB/s\n\
            Performance Drop:   {:.2}% slower\n\
            =============================\n",
            control_speed, experimental_speed, percent
        );

        println!("{}", results);

        self.applied = false;
        self.io_max_path = None;
        self.saved_io_max = None;
        self.control_speed_mbps = None;

        Ok(())
    }
}
