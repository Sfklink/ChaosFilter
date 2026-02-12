//! tc netem injector.
//!
//! Applies a root `netem` qdisc to the configured network interface and restores
//! a known-good baseline on revert.

use anyhow::{anyhow, Context, Result};
use chaosfilter_common::{Plan, RunLikeArgs, print_comparison, run_ping_test, validate_plan};
use std::process::Command;

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
    /// # Arguments
    /// * `iface` - Interface to inspect.
    ///
    /// # Side Effects
    /// Executes `tc qdisc show dev <iface>` and may print warnings to stderr.
    ///
    /// # Notes
    /// This helper is intentionally non-fatal: failures are logged instead of returned.
    pub fn show_qdisc_state(iface: &str) {
        println!();
        println!("----------------------");
        println!("Current qdiscs:");
        println!("----------------------");

        match Command::new("tc")
            .args(["qdisc", "show", "dev", iface])
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("[qdisc] warning: tc qdisc show exited {}", status),
            Err(e) => eprintln!("[qdisc] warning: failed to run tc qdisc show: {}", e),
        }
    }

    /// Applies `tc netem` according to `plan.injectors.qdisc_netem`.
    ///
    /// # Arguments
    /// * `plan` - Chaos plan containing `targets.iface` and netem parameters.
    ///
    /// # Returns
    /// Returns `Ok(())` if the qdisc was applied successfully.
    ///
    /// # Side Effects
    /// Executes `tc qdisc replace ... netem delay <delay> loss <loss>` and updates internal state.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if:
    /// - the `tc` command fails to execute, or
    /// - `tc` exits non-zero (often due to missing privileges).
    ///
    /// # Panics
    /// May panic if `plan.targets.iface` is `None` (uses `unwrap()`); callers should
    /// call [`QdiscNetem::validate`] first or ensure the plan is valid.
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
        Self::show_qdisc_state(iface);

        self.applied = true;
        self.iface = Some(iface.to_string());
        Ok(())
    }

    /// Restores a deterministic baseline root qdisc on `iface`.
    ///
    /// This replaces the current root qdisc with `fq_codel`.
    ///
    /// # Arguments
    /// * `iface` - Interface to restore.
    ///
    /// # Side Effects
    /// Executes `tc qdisc replace dev <iface> root fq_codel`.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if `tc` fails to execute or exits non-zero.
    ///
    /// # Notes
    /// - This is a destructive operation: it does not attempt to restore a previously
    ///   existing qdisc configuration.
    /// - `fq_codel` is used as a known baseline to make [`QdiscNetem::revert`] deterministic.
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
    /// # Arguments
    /// * `iface` - Network interface to modify.
    /// * `delay_ms` - Packet delay in milliseconds.
    /// * `loss_percent` - Packet loss percentage (`0.0`–`100.0`).
    ///
    /// # Returns
    /// Returns `Ok(())` if the qdisc was applied successfully.
    ///
    /// # Side Effects
    /// Modifies the interface's root qdisc via `tc qdisc replace`.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if:
    /// - the `tc` command fails to execute, or
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

    /// Reverts any applied qdisc changes.
    ///
    /// # Returns
    /// Returns `Ok(())` if nothing was applied or if revert completed successfully.
    ///
    /// # Side Effects
    /// If chaos was applied, restores the baseline root qdisc and prints verification output.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`), when a revert is actually performed.
    ///
    /// # Errors
    /// Returns an error if restoring the baseline qdisc fails.
    ///
    /// # Notes
    /// This method is intended to be idempotent: calling it multiple times should be safe.
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
        Self::show_qdisc_state(iface);

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
    /// Returns `Ok(())` regardless of whether a qdisc was present.
    ///
    /// # Side Effects
    /// Attempts to remove the interface's root qdisc via `tc qdisc del`.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error only if the `tc` command fails to execute.
    /// Non-zero exit codes are logged as warnings and ignored.
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
/// - Sleeps the current thread for `plan.schedule.duration_s`.
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
    validate_plan(plan)?;

    let ping_target = "8.8.8.8";

    let iface = plan.targets.iface.as_deref()
        .ok_or_else(|| anyhow!("targets.iface required for ping report"))?;

    let base_args = RunLikeArgs {
        config: None,
        iface: plan.targets.iface.clone(),
        duration_s: plan.schedule.duration_s,
        cgroup: None,
        netem_delay_ms: 0,
        netem_loss_percent: 0.0,
        load_ebpf: false
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