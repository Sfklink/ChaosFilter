//! tc netem injector.
//!
//! Applies a root `netem` qdisc to the configured network interface and restores
//! a known-good baseline on revert.

use anyhow::{anyhow, Context, Result};
use std::process::{Command, Stdio};
use crate::cli::Plan;

/// tc netem injector state.
///
/// Tracks whether chaos was applied so `revert` can be idempotent.
///
// THIS IS A PROBLEM.
// Here, we are making the mistake of supplying domain logic to itself internally, we don't like that.
// It takes in arguments, it does the thing.  Right now, this stinks, and is not testable.
#[derive(Default)]
pub struct NetworkConfig {
    applied: bool,
    iface: Option<String>,
    pub duration_s: u64,
    pub netem_delay_ms: i32,
    pub netem_loss_percent: f64,
}

/// Summary statistics parsed from `ping` output.
///
/// # Notes:
/// RTT values are in milliseconds. Packet loss is a percentage in the range `0.0..=100.0`.
#[derive(Debug)]
pub struct PingStats {
    pub transmitted: u32,
    pub received: u32,
    pub loss_pct: f32,
    pub rtt_min: f32,
    pub rtt_avg: f32,
    pub rtt_max: f32,
}

impl NetworkConfig {
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
    /// This method updates internal injector state so that [`NetworkConfig::revert`]
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
        let delay_ms = plan.injectors.network_config.delay_ms;
        let loss_percent = plan.injectors.network_config.loss_percent;

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
    /// `fq_codel` is used as a known baseline so [`NetworkConfig::revert`] can be deterministic.
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
    /// - Restores a baseline root qdisc via [`NetworkConfig::create_restore_root`].
    /// - Prints verification output via [`NetworkConfig::show_qdisc_state`].
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
/// - Applies and reverts system-level chaos via [`NetworkConfig`].
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


    //
    //  Hardcoded ping IP is unacceptable, needs to be moved to args & config
    //

    let ping_target = "8.8.8.8";

    let iface = plan.targets.iface.as_deref()
        .ok_or_else(|| anyhow!("targets.iface required for ping report"))?;
    //
    // let base_args = NetworkConfig {
    //     applied: false,
    //     iface: Option::from(plan.targets.iface.clone().unwrap_or_default()), // see note below
    //
    //     // schedule
    //     duration_s: plan.schedule.duration_s,
    //
    //     // targets
    //
    //     // netem baseline
    //     netem_delay_ms: 0,
    //     netem_loss_percent: 0.0,
    //
    //     };

    // 2) Apply qdisc
    let mut qdisc = NetworkConfig::default();
    qdisc.apply(plan)?;

    // 3) Run Chaos
    println!("Holding chaos for {} seconds.", plan.schedule.duration_s);
    let chaos_stats = run_ping_test(&plan, ping_target)
        .context("Failed to collect chaos ping stats")?;

    // 4) Remove qdisc
    qdisc.revert()?;

    println!("\nRun complete.\n");

    print_comparison(iface, plan.schedule.duration_s, &chaos_stats);
    Ok(())
}


/// Validates that a network interface exists on the host.
///
/// This function performs a lightweight check using
/// `ip link show <iface>` to verify that the interface
/// is present and accessible.
///
/// # Arguments
/// * `iface` - Name of the network interface to validate.
///
/// # Returns
/// Returns `Ok(())` if the interface exists.
///
/// # Side Effects
/// Executes the system command:
/// - `ip link show <iface>`
///
/// # Errors
/// Returns an error if:
/// - The `ip` command fails to execute, or
/// - The interface does not exist.
///
/// # Requires
/// The `ip` command must be available on the system.
///


pub fn validate_iface_exists(iface: Option<&str>) -> Result<()> {
    let Some(iface) = iface else {
        // iface not specified => nothing to validate here
        return Ok(());
    };

    let status = Command::new("ip")
        .args(["link", "show", iface])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow!("network interface not found: {}", iface));
    }

    Ok(())
}

/// Executes a ping test and parses packet statistics.
///
/// This function runs a timed ping using the interface defined
/// in the provided [`Plan`] and extracts transmission, loss,
/// and RTT metrics.
///
/// # Arguments
/// * `plan` - Chaos plan containing target interface and duration.
/// * `target` - Destination host or IP address to ping.
///
/// # Returns
/// Returns `Some(PingStats)` if:
/// - The ping command executes successfully, and
/// - Output can be parsed correctly.
///
/// Returns `None` if:
/// - The interface is not set in the plan,
/// - The command fails,
/// - Or parsing fails.
///
/// # Side Effects
/// Executes:
/// - `ping -I <iface> -w <duration> <target>`
///
/// # Notes
/// RTT values are reported in milliseconds.
/// Packet loss is a percentage in the range `0.0..=100.0`.
pub fn run_ping_test(plan: &Plan, target: &str) -> Option<PingStats> {

    let iface = plan.targets.iface
        .as_deref()?;

    let output = Command::new("ping")
        .args([
            "-I", iface,
            "-w", &plan.schedule.duration_s.to_string(),
            target
        ])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut transmitted = 0;
    let mut received = 0;
    let mut loss_pct = 0.0;
    let mut rtt_min = 0.0;
    let mut rtt_avg = 0.0;
    let mut rtt_max = 0.0;

    for line in stdout.lines() {
        if line.contains("packets transmitted") {
            let parts: Vec<&str> = line.split(',').collect();
            transmitted = parts.get(0)?.trim().split(' ').next()?.parse().ok()?;
            received = parts.get(1)?.trim().split(' ').next()?.parse().ok()?;
            loss_pct = parts.get(2)?.trim().split('%').next()?.parse().ok()?;
        }

        if line.contains("rtt min/avg/max") {
            let stats = line.split('=').nth(1)?.trim();
            let nums: Vec<&str> = stats.split('/').collect();
            rtt_min = nums.get(0)?.parse().ok()?;
            rtt_avg = nums.get(1)?.parse().ok()?;
            rtt_max = nums.get(2)?.parse().ok()?;
        }
    }

    Some(PingStats {
        transmitted,
        received,
        loss_pct,
        rtt_min,
        rtt_avg,
        rtt_max,
    })
}
pub fn print_comparison(iface: &str,
                        duration: u64,
                        //control: &PingStats,
                        modified: &PingStats) -> String {
    let output = format!(
        "\n\n=== Network Comparison (Duration: {duration} seconds) ===

DURING CHAOS ({iface}):
  transmitted : {md_tx}
  received    : {md_rx}
  loss %      : {md_loss}
  rtt (ms)    : min {md_min} | avg {md_avg} | max {md_max}
// ",
// Just hiding this little guy down here because this was implemented nasty as hell and I hate it
        // this is what happens when you make no-value-added updates to the code and then merge them into main
// BEFORE CHAOS (baseline of {iface}):
//   transmitted : {ct_tx}
//   received    : {ct_rx}
//   loss %      : {ct_loss}
//   rtt (ms)    : min {ct_min} | avg {ct_avg} | max {ct_max}

//         ct_tx = control.transmitted,
//         ct_rx = control.received,
//         ct_loss = control.loss_pct,
//         ct_min = control.rtt_min,
//         ct_avg = control.rtt_avg,
//         ct_max = control.rtt_max,
        md_tx = modified.transmitted,
        md_rx = modified.received,
        md_loss = modified.loss_pct,
        md_min = modified.rtt_min,
        md_avg = modified.rtt_avg,
        md_max = modified.rtt_max,
    );

    println!("{}", output);
    output
}