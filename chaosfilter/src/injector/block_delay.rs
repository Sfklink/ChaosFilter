use crate::injector::ChaosInjector;
use crate::plans::Plan;
use crate::validate::validate_block_config;
use anyhow::{bail, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
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

// simple disk speed test (control vs experimental)
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

    // cleanup test file
    let _ = fs::remove_file("testfile");

    Ok(mbps)
}

#[derive(Default)]
pub struct BlockDelayInjector {
    applied: bool,
    io_max_path: Option<PathBuf>,
    saved_io_max: Option<String>,
}

impl ChaosInjector for BlockDelayInjector {
    fn name(&self) -> &'static str {
        "block_delay"
    }

    fn apply(&mut self, plan: Plan) -> Result<()> {
        let cfg = &plan.injectors.block_config;

        validate_block_config(&plan)?;

        let device = cfg.device.as_ref().expect("Device must be specified");
        let mm = major_minor(device)?;
        info!(device, major_minor = %mm, "resolved block device");

        let base_path = Path::new("/sys/fs/cgroup/chaosfilter");
        let cgroup_path = base_path.join(&plan.name);
        info!(cgroup = ?cgroup_path, "creating cgroup");

        if !base_path.exists() {
            fs::create_dir(base_path)?;
            info!("created base chaosfilter cgroup directory");
        }

        debug!(base = ?base_path, "checking io controller availability");

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
            info!(cgroup = ?cgroup_path, "created plan cgroup directory");
        } else {
            debug!(cgroup = ?cgroup_path, "plan cgroup already exists");
        }

        let io_max = cgroup_path.join("io.max");
        if !io_max.exists() {
            warn!(
                path = ?io_max,
                "io.max not found; the io controller may not be enabled"
            );
            return Ok(());
        }

        run_disk_test("CONTROL")?;

        let self_pid = std::process::id();
        let procs_path = cgroup_path.join("cgroup.procs");
        fs::write(&procs_path, self_pid.to_string())?;
        info!(
            pid = self_pid,
            cgroup = ?cgroup_path,
            "moved current process into cgroup"
        );

        let current = fs::read_to_string(&io_max)?;
        debug!(contents = current.trim(), "current io.max");

        let mut rule = mm;

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

        debug!(rule, "applying throttle rule");

        {
            let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
            file.write_all(rule.as_bytes())?;
        }

        self.io_max_path = Some(io_max);
        self.saved_io_max = Some(current);
        self.applied = true;

        Ok(())
    }

    fn revert(&mut self) -> Result<()> {
        if !self.applied {
            debug!("block delay injector not applied; skipping revert");
            return Ok(());
        }

        let path = self
            .io_max_path
            .as_ref()
            .expect("io_max_path set when applied");
        let saved = self
            .saved_io_max
            .as_deref()
            .expect("saved_io_max set when applied");

        info!("restoring original io.max");
        let mut file = fs::OpenOptions::new().write(true).open(path)?;
        file.write_all(saved.as_bytes())?;

        self.applied = false;
        self.io_max_path = None;
        self.saved_io_max = None;

        Ok(())
    }
}