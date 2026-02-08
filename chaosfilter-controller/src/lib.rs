use injector::Injector;
use qdiscs::QdiscNetem;
use chaosfilter_common::Plan;

use anyhow::{Context, bail, Result};
use std::{env, fs, thread, time::Duration, path::{Path, PathBuf}, process::Command};
use aya::Ebpf;

pub mod qdiscs;
pub mod injector;
pub mod tc;

pub fn validate_plan(plan: &Plan) -> Result<()> {
    // Check cgroup exists
    if let Some(cg) = &plan.targets.cgroup {
        let cgroup_path = format!("/sys/fs/cgroup/{}", cg);
        if !Path::new(&cgroup_path).exists() {
            bail!("cgroup does not exist: {}", cgroup_path);
        }
    }


    // Check network interface exists (if provided)
    if let Some(iface) = &plan.targets.iface {
        let status = Command::new("ip")
            .args(["link", "show", iface])
            .status()?;

        if !status.success() {
            bail!("network interface not found: {}", iface);
        }
    }

    Ok(())
}


pub fn load_ebpf_object() -> Result<Ebpf> {
    // 1) Prefer explicit path if provided
    if let Ok(p) = env::var("CHAOSFILTER_EBPF_OBJ") {
        let bytes = fs::read(&p)
            .with_context(|| format!("failed to read ebpf object: {}", p))?;
        return Ebpf::load(&bytes).context("failed to load eBPF object");
    }

    // 2) Fallback to OUT_DIR/chaosfilter-ebpf.o (will work later after build.rs/xtask is wired)
    let out_dir = env::var("OUT_DIR")
        .context("OUT_DIR not set; are you running via cargo?")?;

    let mut path = PathBuf::from(out_dir);
    path.push("chaosfilter-ebpf.o");

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read ebpf object: {}", path.display()))?;

    Ebpf::load(&bytes).context("failed to load eBPF object")
}

//For the CLI to work I needed to redo this section. Nothing is being changed
//The CLI just needs to seperate the running of the tests and the reset so 
//it can get the data from the tests

//This function starts the plan
pub fn apply_plan(plan: &Plan) -> Result<QdiscNetem> {
    validate_plan(plan)?;

    //qdisc injector
    QdiscNetem::validate(plan)?;
    let mut qdisc = QdiscNetem::default();
    qdisc.apply(plan)?;

    Ok(qdisc)
}

//This function reverts the plan
pub fn revert_plan(mut qdisc: QdiscNetem, plan: &Plan) -> Result<()> {
    qdisc.revert()?; //best effort clean up

    if plan.features.load_ebpf {
        let _bpf = load_ebpf_object()?;
        println!("[control] loaded ebpf object");
    } else {
        println!("[control] skipping ebpf load (features.load_ebpf=false)");
    }

    Ok(())
}

//Rewritting this to use the new fucntions
//Base line the CLI needed to seperate the running of the test and
//the reverting of the params to get the data
pub fn run_plan(plan: &Plan) -> Result<()> {
    let qdisc = apply_plan(plan)?;

    println!(
        "[control] holding chaos for {} ms",
        plan.schedule.duration_ms
    );
    thread::sleep(Duration::from_millis(plan.schedule.duration_ms));

    revert_plan(qdisc, plan)?;

    Ok(())
}

