use crate::plans::Plan;
//use crate::controller;
use anyhow::{Result, bail};
use libc;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::Instant;

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
        let msg = format!(
            "WARNING: IO controller is not enabled.\n\n\
             Run this once after boot:\n\
             sudo sh -c 'echo +io > {}'\n\n\
             Then rerun ChaosFilter.",
            root_subtree.display()
        );

        println!("{}", msg);
        bail!("IO controller not enabled");
    }

    Ok(())
}

// simple disk speed test (control vs experimental)
fn run_disk_test(label: &str) -> Result<f64> {
    println!("Running {} test...", label);

    let start = Instant::now();

    let output = Command::new("dd")
        .args([
            "if=/dev/zero",
            "of=testfile",
            "bs=1M",
            "count=200",
            "oflag=direct",
        ])
        .output()?;

    let duration = start.elapsed().as_secs_f64();

    if !output.status.success() {
        bail!("dd command failed");
    }

    let mb_written = 200.0;
    let mbps = mb_written / duration;

    println!(
        "{} test completed in {:.2} sec ({:.2} MB/s)",
        label, duration, mbps
    );

    // cleanup test file
    let _ = fs::remove_file("testfile");

    Ok(mbps)
}

pub fn run(plan: &Plan) -> Result<()> {
    // bunch of boring verification so you dont brick your system
    let cfg = &plan.injectors.block_config;

    if !cfg.enabled {
        println!("Block delay injector not enabled in config.");
        return Ok(());
    }

    io_enabled()?;

    let device = cfg.device.as_ref().expect("Device must be specified");
    let major_minor = major_minor(device)?;
    println!("Resolved device {} --> {}", device, major_minor);

    let base_path = Path::new("/sys/fs/cgroup/chaosfilter");
    let cgroup_path = base_path.join(&plan.name);
    println!("Creating cgroup at {:?}", cgroup_path);

    if !base_path.exists() {
        fs::create_dir(base_path)?;
        println!("Created base chaosfilter cgroup directory");
    }

    println!("Checking controller availability in {:?}", base_path);

    let chaos_subtree = base_path.join("cgroup.subtree_control");

    if chaos_subtree.exists() {
        let content = fs::read_to_string(&chaos_subtree)?;

        if !content.contains("io") {
            println!("Enabling io controller in chaosfilter subtree...");

            let mut file = fs::OpenOptions::new().write(true).open(&chaos_subtree)?;

            file.write_all(b"+io")?;
        }
    } else {
        println!("WARNING: chaosfilter subtree_control not available yet");
    }

    if !cgroup_path.exists() {
        fs::create_dir(&cgroup_path)?;
        println!("Created plan cgroup directory");
    } else {
        println!("Plan cgroup already exists");
    }

    let io_max = cgroup_path.join("io.max");
    if !io_max.exists() {
        println!("WARNING: io.max not found at {:?}", io_max);
        println!("The io controller may not be enabled.");
        return Ok(());
    }

    // CONTROL RUN
    let control_speed = run_disk_test("CONTROL")?;

    // MOVE SELF INTO CGROUP
    let self_pid = std::process::id();
    let procs_path = cgroup_path.join("cgroup.procs");
    fs::write(&procs_path, self_pid.to_string())?;
    println!("Moved current process (PID {}) into cgroup", self_pid);

    let current = fs::read_to_string(&io_max)?;
    println!("Current io.max contents:");
    println!("{}", current);

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

    println!("Throttle rule stats:");
    println!("{}", rule);

    // throttle application hell yeah
    {
        let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
        file.write_all(rule.as_bytes())?;
    }

    println!(
        "Throttling delay for {} secs. Please hold...",
        plan.schedule.duration_s
    );
    thread::sleep(Duration::from_secs(1)); // short settle time

    // EXPERIMENTAL RUN
    let experimental_speed = run_disk_test("EXPERIMENTAL")?;

    // take it back now yall
    println!("Restoring original io.max. Please hold...");
    {
        let mut file = fs::OpenOptions::new().write(true).open(&io_max)?;
        file.write_all(current.as_bytes())?;
    }

    println!("Cgroup throttling all done and undone.");

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
