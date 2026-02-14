//! tc netem injector.
//!
//! Applies a root `netem` qdisc to the configured network interface and restores
//! a known-good baseline on revert.

use anyhow::{anyhow, Context, Result};
use chaosfilter_common::{Plan, RunLikeArgs, print_comparison, run_ping_test, RunInlineArgs, RunMode};
use std::process::{Stdio, Command};

/// tc netem injector state.
///
/// Tracks whether chaos was applied so `revert` can be idempotent.
#[derive(Default)]
pub struct QdiscNetem {
    applied: bool,
    iface: Option<String>,
}

impl QdiscNetem {
    /// Prints the current qdisc state for `iface` (best effort).
    ///
    /// This helper is intentionally non-fatal: failures are logged as warnings
    /// rather than returned to the caller.
    ///
    /// # Arguments
    /// * `iface` - Network interface to inspect.
    ///
    /// # Returns
    /// This function returns `()`.
    ///
    /// # Side Effects
    /// - Writes human-readable output to stdout/stderr.
    /// - Executes `tc qdisc show dev <iface>`.
    ///
    /// # Errors
    /// This function does not return a [`Result`].  
    /// If `tc` fails or exits non-zero, a warning is printed.
    ///
    /// # Panics
    /// This function does not explicitly panic.
    ///



    pub fn show_qdisc_state(iface: &str) {
        match Command::new("tc")
            .args(["qdisc", "show", "dev", iface])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("[qdisc] warning: tc qdisc show exited {}", status),
            Err(e) => eprintln!("[qdisc] warning: failed to run tc qdisc show: {}", e),
        }
    }


    /// Applies `tc netem` according to `plan.injectors.qdisc_netem`.
    ///
    /// This method updates internal injector state so that [`QdiscNetem::revert`]
    /// can undo changes later.
    ///
    /// # Arguments
    /// * `plan` - Chaos plan containing `targets.iface` and netem parameters.
    ///
    /// # Returns
    /// Returns `Ok(())` if the qdisc is applied successfully.
    ///
    /// # Side Effects
    /// - Executes `tc qdisc replace dev <iface> root netem delay <delay> loss <loss>`.
    /// - Prints status output to stdout/stderr.
    /// - Updates internal state (`applied`, `iface`).
    ///
    /// # Requires
    /// CAP_NET_ADMIN privileges (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if:
    /// - The `tc` command fails to execute, or
    /// - `tc` exits non-zero (commonly due to insufficient privileges).
    ///
    /// # Panics
    /// May panic if `plan.targets.iface` is `None` (uses `unwrap()`).
    /// Callers should ensure the plan is valid (e.g., via [`validate_plan`]).
    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        let iface = plan.targets.iface.as_deref().unwrap();
        let delay_ms = plan.injectors.qdisc_netem.delay_ms;
        let loss_percent = plan.injectors.qdisc_netem.loss_percent;

        println!(
            "[qdisc] applying netem to {} (delay={}ms loss={}%)",
            iface, delay_ms, loss_percent
        );

        let delay = format!("{delay_ms}ms");
        let loss = format!("{loss_percent}%");

        let status = Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", iface,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to execute tc (apply)")?;

        if !status.success() {
            return Err(anyhow!("tc failed applying netem on {} (need sudo)", iface));
        }


        self.applied = true;
        self.iface = Some(iface.to_string());
        Ok(())
    }

    /// Restores a deterministic baseline root qdisc on `iface`.
    ///
    /// This replaces the current root qdisc with `fq_codel`. It does **not**
    /// attempt to preserve or restore any previously existing qdisc configuration.
    ///
    /// # Arguments
    /// * `iface` - Network interface to restore.
    ///
    /// # Returns
    /// This function returns `()`.
    ///
    /// # Side Effects
    /// - Executes `tc qdisc replace dev <iface> root fq_codel`.
    /// - Prints a success/failure message to stdout.
    ///
    /// # Requires
    /// CAP_NET_ADMIN privileges (typically `sudo`).
    ///
    /// # Errors
    /// This function does not return a [`Result`].  
    /// Failures are reported via printed messages.
    ///
    /// # Notes
    /// `fq_codel` is used as a known baseline so [`QdiscNetem::revert`] can be deterministic.
    pub fn create_restore_root(iface: &str) {
        let status = Command::new("tc")
            .args(["qdisc", "replace", "dev", iface, "root", "fq_codel"])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("Root qdisc applied successfully to '{}'", iface);
            }
            Ok(_) => {
                println!("ERROR: failed to apply root qdisc on '{}'", iface);
            }
            Err(e) => {
                println!("ERROR: failed to execute tc: {}", e);
            }
        }
    }

    /// Applies a root `netem` qdisc to an interface.
    ///
    /// This is a convenience helper for applying netem directly without using a full [`Plan`].
    ///
    /// # Arguments
    /// * `iface` - Network interface to modify.
    /// * `delay_ms` - Packet delay in milliseconds.
    /// * `loss_percent` - Packet loss percentage (`0.0`–`100.0`).
    ///
    /// # Returns
    /// Returns `Ok(())` if the qdisc was applied successfully.
    ///
    /// # Side Effects
    /// Executes:
    /// - `tc qdisc replace dev <iface> root netem delay <delay> loss <loss>`
    ///
    /// # Requires
    /// CAP_NET_ADMIN privileges (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if:
    /// - The `tc` command fails to execute, or
    /// - `tc` exits non-zero (often due to insufficient privileges).
    pub fn apply_netem(iface: &str, delay_ms: u32, loss_percent: f32) -> Result<()> {
        let delay = format!("{delay_ms}ms");
        let loss = format!("{loss_percent}%");

        let status = Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", iface,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to execute tc (apply)")?;

        if !status.success() {
            return Err(anyhow!(
                "tc failed applying netem on {} (are you running as root?)",
                iface
            ));
        }

        Ok(())
    }

    /// Reverts any applied qdisc changes (best effort).
    ///
    /// This method is intended to be idempotent: if no chaos was applied,
    /// it prints a message and returns success without doing anything.
    ///
    /// # Arguments
    /// This function takes no arguments.
    ///
    /// # Returns
    /// Returns `Ok(())` if:
    /// - No qdisc changes were applied, or
    /// - Revert completed successfully.
    ///
    /// # Side Effects
    /// If chaos was applied:
    /// - Restores a baseline root qdisc via [`QdiscNetem::create_restore_root`].
    /// - Prints verification output via [`QdiscNetem::show_qdisc_state`].
    /// - Clears internal state (`applied`, `iface`).
    ///
    /// # Requires
    /// CAP_NET_ADMIN privileges (typically `sudo`) when a revert is performed.
    ///
    /// # Errors
    /// Returns an error only if internal assumptions are broken in a way that
    /// causes downstream operations to fail unexpectedly.
    ///
    /// # Panics
    /// May panic if internal state is inconsistent (uses `unwrap()` on `self.iface`).
    pub fn revert(&mut self) -> Result<()> {
        if !self.applied {
            println!("[qdisc] nothing applied; skipping revert");
            return Ok(());
        }

        let iface = self.iface.as_deref().unwrap();

        println!("[qdisc] reverting qdisc on {}", iface);

        // Deterministic revert: restore the known-good root qdisc.
        Self::create_restore_root(iface);

        // Verbose verification
        // DEBUG
        // Self::show_qdisc_state(iface);

        self.applied = false;
        self.iface = None;

        Ok(())
    }

    /// Deletes the root qdisc from an interface (best effort).
    ///
    /// This is intended for cleanup or recovery and does **not** attempt to
    /// restore any previously existing qdisc configuration.
    ///
    /// # Arguments
    /// * `iface` - Network interface to modify.
    ///
    /// # Returns
    /// This function returns `()`.
    ///
    /// # Side Effects
    /// - Attempts to remove the interface's root qdisc via:
    ///   `sudo tc qdisc del dev <iface> root`
    /// - Prints status output to stdout.
    ///
    /// # Requires
    /// CAP_NET_ADMIN privileges (typically `sudo`).
    ///
    /// # Errors
    /// This function does not return a [`Result`].  
    /// Failures are reported via printed messages (including the case where no root qdisc exists).
    ///
    /// # Panics
    /// This function does not explicitly panic.
    pub fn delete_root_qdisc(iface: &str) {
        let status = Command::new("sudo")
            .args(["tc", "qdisc", "del", "dev", iface, "root"])
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("Root qdisc deleted successfully from '{}'", iface);
            }
            Ok(_) => {
                println!("ERROR: failed to delete root qdisc from '{}' (there may not be one)", iface);
            }
            Err(e) => {
                println!("ERROR: failed to execute tc: {}", e);
            }
        }
    }
}

/// Runs the plan end-to-end (baseline → apply → hold → revert).
///
/// This is the one-shot entrypoint used by the CLI to execute network chaos
/// and produce a basic “before vs during” ping comparison report.
///
/// # Arguments
/// * `plan` - Chaos plan to run.
///
/// # Returns
/// Returns `Ok(())` after:
/// - Baseline ping stats are collected,
/// - Chaos is applied and measured,
/// - And cleanup/revert succeeds.
///
/// # Side Effects
/// - Executes `ping` to collect baseline and chaos metrics.
/// - Applies and reverts system-level chaos via [`QdiscNetem`].
/// - Prints progress and a comparison report to stdout.
///
/// # Requires
/// - A valid [`Plan`] (validated by [`validate_plan`]).
/// - CAP_NET_ADMIN privileges (typically `sudo`) to modify qdiscs.
/// - Network connectivity to the ping target (currently `8.8.8.8`).
///
/// # Errors
/// Returns an error if:
/// - Plan validation fails.
/// - `targets.iface` is missing.
/// - Baseline or chaos ping stats cannot be collected.
/// - Applying or reverting the qdisc fails.
pub fn run_plan(plan: &Plan) -> Result<()> {
    let netem = &plan.injectors.qdisc_netem;
    let netem_is_noop = netem.delay_ms == 0 && netem.loss_percent == 0.0;
    if netem_is_noop {
        return Ok(());
    }
    if !plan.injectors.qdisc_netem.enabled {
        return Ok(());
    }

    //
    //  Hardcoded ping IP is unacceptable, needs to be moved to args & config
    //

    let ping_target = "8.8.8.8";

    let iface = plan.targets.iface.as_deref()
        .ok_or_else(|| anyhow!("targets.iface required for ping report"))?;

    let base_args = RunLikeArgs {
        mode: RunMode::Inline(RunInlineArgs {
            // required in inline mode
            netem_enabled: true,
            iface: plan.targets.iface.clone().unwrap_or_default(), // see note below

            // schedule
            duration_s: plan.schedule.duration_s,

            // targets
            cgroup: plan.targets.cgroup.clone(),

            // netem baseline
            netem_delay_ms: 0,
            netem_loss_percent: 0.0,

            // features
            load_ebpf: false,

            // cgroup injector baseline (disabled)
            cgroup_knobs_enabled: false,
            cgroup_pid: None,
            cgroup_move_pid: true,
            cgroup_enable: vec![],

            cgroup_cpu_max: None,
            cgroup_cpu_weight: None,
            cgroup_mem_max: None,
            cgroup_mem_high: None,
            cgroup_swap_max: None,
        }),
    };
    let base_plan = base_args.plan_from_args()?;

    // 1) Run Baseline (Control)
    println!("Running baseline ping test (before chaos) for {} seconds...", base_plan.schedule.duration_s);
	let control_stats = run_ping_test(&base_plan, ping_target)
		.context("Failed to collect baseline ping stats")?;
    
    // 2) Apply qdisc
    let mut qdisc = QdiscNetem::default();
    qdisc.apply(plan)?;

    // 3) Run Chaos
    println!("Holding chaos for {} seconds.", plan.schedule.duration_s);
    let chaos_stats = run_ping_test(&plan, ping_target)
        .context("Failed to collect chaos ping stats")?;

    // 4) Remove qdisc
    qdisc.revert()?;

    println!("\nRun complete.\n");

    print_comparison(iface, plan.schedule.duration_s, &control_stats, &chaos_stats);
    Ok(())
}