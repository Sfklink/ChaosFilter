use crate::plans::Plan;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::Instant;
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

pub fn run(plan: &Plan) -> Result<()> {
    // bunch of boring verification so you dont brick your system
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

    // CONTROL RUN
    let control_speed = run_disk_test("CONTROL")?;

    // MOVE SELF INTO CGROUP
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

    debug!(
        rule,
        "applying throttle rule"
    );

    // throttle application hell yeah
    {
        let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
        file.write_all(rule.as_bytes())?;
    }

    info!(
        duration_s = plan.schedule.duration_s,
        "throttling; holding"
    );
    thread::sleep(Duration::from_secs(1)); // short settle time

    // EXPERIMENTAL RUN
    let experimental_speed = run_disk_test("EXPERIMENTAL")?;

    // take it back now yall
    info!("restoring original io.max");
    {
        let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
        file.write_all(current.as_bytes())?;
    }

    info!("block throttling applied and reverted");

    // RESULTS
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

    Ok(())
}