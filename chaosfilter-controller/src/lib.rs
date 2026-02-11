//! Controller orchestration.
//!
//! High-level operations for validating and running a [`Plan`].
//! This layer coordinates injectors (tc/qdisc, eBPF load, etc.) and ensures
//! best-effort cleanup.

use qdiscs::QdiscNetem;
use chaosfilter_common::Plan;

use anyhow::{Context, bail, Result};
use std::{env, fs, thread, time::Duration, path::{Path, PathBuf}, process::Command};
use aya::Ebpf;

pub mod qdiscs;
pub mod tc;

/// Validates a plan against the current host environment.
///
/// Performs lightweight pre-flight checks to catch obvious configuration errors
/// before any chaos is applied.
///
/// # Arguments
/// * `plan` - Chaos plan to validate.
///
/// # Returns
/// Returns `Ok(())` if the environment appears compatible with the plan.
///
/// # Side Effects
/// Executes read-only system checks (e.g. `ip link show`) and filesystem existence checks.
///
/// # Errors
/// Returns an error if:
/// - the referenced cgroup path does not exist, or
/// - the referenced network interface does not exist, or
/// - required system commands fail to execute.
pub fn validate_plan(plan: &Plan) -> Result<()> {
    println!("\nValidating chaos plan...\n");

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

    println!("\nConfig OK\n");

    Ok(())
}

/// Loads the eBPF object used by ChaosFilter.
///
/// Resolution order:
/// 1) `CHAOSFILTER_EBPF_OBJ` environment variable (explicit path)
/// 2) `$OUT_DIR/chaosfilter-ebpf.o` (build output)
///
/// # Returns
/// Returns an [`aya::Ebpf`] object loaded from the chosen object file.
///
/// # Side Effects
/// Reads an object file from disk and consults environment variables.
///
/// # Errors
/// Returns an error if:
/// - the object file cannot be read,
/// - `OUT_DIR` is missing when needed,
/// - or the object cannot be parsed/loaded by Aya.
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

/// Applies the plan and returns an injector handle used for revert.
///
/// This function is intended for interactive workflows where you need to:
/// 1) apply chaos,
/// 2) run external measurements while chaos is active,
/// 3) revert chaos afterward (using the returned handle).
///
/// # Arguments
/// * `plan` - Chaos plan to apply.
///
/// # Returns
/// Returns an injector handle (currently [`QdiscNetem`]) that can be passed to
/// [`revert_plan`] to undo the applied changes.
///
/// # Side Effects
/// Modifies system state by applying configured injectors (e.g. tc/qdisc netem).
///
/// # Requires
/// Applying network chaos typically requires CAP_NET_ADMIN (often `sudo`), depending on the injector.
///
/// # Errors
/// Returns an error if:
/// - [`validate_plan`] fails,
/// - injector validation fails,
/// - or applying chaos fails.
pub fn apply_plan(plan: &Plan) -> Result<QdiscNetem> {
    validate_plan(plan)?;

    //qdisc injector
    QdiscNetem::validate(plan)?;
    let mut qdisc = QdiscNetem::default();
    qdisc.apply(plan)?;

    Ok(qdisc)
}

/// Reverts applied chaos and performs optional feature actions.
///
/// Currently:
/// - reverts the qdisc injector (best effort cleanup),
/// - optionally loads the eBPF object when `plan.features.load_ebpf` is `true`.
///
/// # Arguments
/// * `qdisc` - Injector handle returned by [`apply_plan`].
/// * `plan` - Original plan used to configure optional feature behavior.
///
/// # Returns
/// Returns `Ok(())` if cleanup and optional steps complete successfully.
///
/// # Side Effects
/// - Modifies system state by reverting chaos.
/// - May load an eBPF object from disk.
/// - Prints status messages to stdout.
///
/// # Requires
/// Reverting network chaos typically requires CAP_NET_ADMIN (often `sudo`), depending on the injector.
///
/// # Errors
/// Returns an error if revert fails or if eBPF loading is enabled and fails.
pub fn revert_plan(mut qdisc: QdiscNetem, plan: &Plan) -> Result<()> {
    qdisc.revert()?; //best effort clean up

    if plan.features.load_ebpf {
        let _bpf = load_ebpf_object()?;
        println!("Loaded ebpf object.");
    } else {
        println!("Skipping ebpf load (features.load_ebpf=false).");
    }

    Ok(())
}


/// Runs the plan end-to-end (apply → hold → revert).
///
/// This is the one-shot “just run it” entrypoint used by the CLI.
///
/// # Arguments
/// * `plan` - Chaos plan to run.
///
/// # Returns
/// Returns `Ok(())` after chaos has been applied, held for the configured duration,
/// and reverted successfully.
///
/// # Side Effects
/// - Applies and reverts system-level chaos (e.g. tc/qdisc).
/// - Sleeps the current thread for `plan.schedule.duration_ms`.
/// - Installs a Ctrl+C handler that attempts cleanup and exits the process.
///
/// # Requires
/// Running network chaos typically requires CAP_NET_ADMIN (often `sudo`), depending on the injector.
///
/// # Errors
/// Returns an error if:
/// - applying the plan fails,
/// - the Ctrl+C handler cannot be installed,
/// - sleeping is interrupted by process exit,
/// - or reverting the plan fails.
pub fn run_plan(plan: &Plan) -> Result<()> {
    println!("\nRunning chaos plan...\n");

    let qdisc = apply_plan(plan)?;

    // If Ctrl+C is pressed, cleanup
    if let Some(iface) = plan.targets.iface.as_deref() {
        let dev_owned = iface.to_string();
        ctrlc::set_handler(move || {
            let _ = crate::tc::del_root_qdisc(&dev_owned);
            std::process::exit(130); // 130 = interrupted (Ctrl + C)
        })?;
    }

    println!(
        "Holding chaos for {} ms.",
        plan.schedule.duration_ms
    );
    thread::sleep(Duration::from_millis(plan.schedule.duration_ms));

    revert_plan(qdisc, plan)?;

    println!("\nRun complete.\n");

    Ok(())
}

