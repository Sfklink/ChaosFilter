use crate::plans::Plan;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use crate::injector::ChaosInjector;
use anyhow::{Result, bail};
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

// make sure stuff is enabled so you dont have to redo the manual setup after a reboot
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
    // Track whether we actually changed system state so `revert()` can be idempotent.
    applied: bool,

    // Captured state needed to restore `io.max` on revert.
    control_speed: Option<f64>,
    io_max: Option<std::path::PathBuf>,
    original_io_max: Option<String>,
    major_minor: Option<String>,
    device: Option<String>,
}

impl ChaosInjector for BlockDelayInjector {
    fn name(&self) -> &'static str {
        "block_delay"
    }

    fn apply(&mut self, plan: &Plan) -> Result<()> {
        // Apply is the "before/hold" phase: set up cgroup + apply the throttle rule.
        // Any external hold (Ctrl-C or schedule timeout) is owned by `main.rs`.

        // bunch of boring verification so you dont brick your system
        let cfg = &plan.injectors.block_config;

        if !cfg.enabled {
            debug!("block delay injector not enabled; skipping");
            return Ok(());
        }

        io_enabled()?;

        let device = cfg
            .device
            .as_ref()
            .expect("Device must be specified")
            .to_string();
        let major_minor = major_minor(&device)?;
        info!(device, major_minor, "resolved block device");

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

        // CONTROL RUN
        let control_speed = run_disk_test("CONTROL")?;

        // MOVE SELF INTO CGROUP
        let self_pid = std::process::id();
        let procs_path = cgroup_path.join("cgroup.procs");
        fs::write(&procs_path, self_pid.to_string())?;
        info!(pid = self_pid, cgroup = ?cgroup_path, "moved current process into cgroup");

        let current = fs::read_to_string(&io_max)?;
        debug!(contents = current.trim(), "current io.max");

        let mut rule = format!("{}", major_minor);

        // preping the throttling rules/plan whatever word you want to use IDC
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

        // The "hold" is controlled by the caller; we just capture state and return.
        info!("block throttling applied; waiting for external hold");
        thread::sleep(Duration::from_secs(1)); // short settle time

        // Snapshot everything revert needs, then mark as applied.
        self.applied = true;
        self.control_speed = Some(control_speed);
        self.io_max = Some(io_max);
        self.original_io_max = Some(current);
        self.major_minor = Some(major_minor);
        self.device = Some(device);

        Ok(())
    }

    fn revert(&mut self) -> Result<()> {
        // Revert is the "after" phase: run the experimental test and restore `io.max`.
        // It is best-effort and designed to be safe to call even if apply never ran.
        if !self.applied {
            debug!("nothing applied; skipping block_delay revert");
            return Ok(());
        }

        let Some(io_max) = self.io_max.take() else {
            return Ok(());
        };
        let Some(original_io_max) = self.original_io_max.take() else {
            return Ok(());
        };

        // Run the experimental test and compute results, but still restore `io.max`
        // even if the dd/experimental run fails.
        let experimental_speed_res = run_disk_test("EXPERIMENTAL");

        info!("restoring original io.max");
        let restore_res = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
            file.write_all(original_io_max.as_bytes())?;
            Ok(())
        })();

        match experimental_speed_res {
            Ok(experimental_speed) => {
                let control_speed = self
                    .control_speed
                    .take()
                    .unwrap_or(experimental_speed);

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
            }
            Err(e) => {
                warn!(error = %e, "experimental disk test failed");
            }
        }

        self.applied = false;
        self.control_speed = None;
        self.major_minor = None;
        self.device = None;

        restore_res
    }
}

// Backwards-compatible one-shot entrypoint (apply -> hold -> revert)
pub fn run(plan: &Plan) -> Result<()> {
    let mut injector = BlockDelayInjector::default();
    injector.apply(plan)?;
    thread::sleep(Duration::from_secs(plan.schedule.duration_s));
    injector.revert()?;
    Ok(())
}