//! tc netem injector.
//!
//! Applies a root `netem` qdisc to the configured network interface and restores
//! a known-good baseline on revert.

use crate::plans::Plan;
use anyhow::{Context, Result, anyhow};
use std::process::{Command, Stdio};
use crate::injector::ebpf::{attach_classifier, EbpfHandle};
use tracing::{debug, error, info, warn};

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
    pub ebpf_handle: Option<EbpfHandle>,
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
            Ok(status) => warn!(iface, exit_code = %status, "tc qdisc show exited non-zero"),
            Err(e) => warn!(iface, error = %e, "failed to run tc qdisc show"),
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
     pub fn apply(&mut self, plan: &Plan, iface: &str) -> Result<()> {
        let delay_ms = plan.injectors.network_config.delay_ms;
        let loss_percent = plan.injectors.network_config.loss_percent;

        // This is new
        let network_cgroup_target = &plan.injectors.network_config.network_ebpf_cgroup;

        info!(
            iface,
            delay_ms,
            loss_percent,
            "applying netem qdisc"
            );

        let delay = format!("{delay_ms}ms");
        let loss = format!("{loss_percent}%");

        debug!("[network] tc: creating root prio qdisc");

        let root = Command::new("tc")
            .args([
                "qdisc", "replace", "dev", iface,
                "root", "handle", "1:",
                "prio", "bands", "2",
                "priomap",
                "0","0","0","0",
                "0","0","0","0",
                "0","0","0","0",
                "0","0","0","0",
            ])
            .output()
            .context("failed to execute tc (root prio)")?;

        debug!("[network] root prio status: {}", root.status);

        if !root.status.success() {
            eprintln!(
                "[network][ERR] root prio stderr:\n{}",
                String::from_utf8_lossy(&root.stderr)
            );

            if !root_prio_exists(iface)? {
                return Err(anyhow!("tc failed creating prio root qdisc on {}", iface));
            } else {
                debug!("[network] root prio already exists, continuing");
            }
        }

        debug!(
            "[network] tc: attaching netem; parent=1:2 delay={} loss={}",
            delay, loss
        );

        let netem = Command::new("tc")
            .args([
                "qdisc", "replace", "dev", iface,
                "parent", "1:2",
                "handle", "20:",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .output()
            .context("failed to execute tc (netem child)")?;

        debug!("[network] netem status: {}", netem.status);

        if !netem.status.success() {
            eprintln!(
                "[network][ERR] netem stderr:\n{}",
                String::from_utf8_lossy(&netem.stderr)
            );
            return Err(anyhow!("tc failed applying child netem on {}", iface));
        }


        debug!("[network] tc: installing fw filter (mark=1 1:2)");

        let filter = Command::new("tc")
            .args([
                "filter", "replace", "dev", iface,
                "parent", "1:",
                "protocol", "all",
                "prio", "1",
                "handle", "1",
                "fw",
                "flowid", "1:2",
            ])
            .output()
            .context("failed to execute tc (fw filter)")?;

        debug!("[network] filter status: {}", filter.status);

        if !filter.status.success() {
            eprintln!(
                "[network][ERR] filter stderr:\n{}",
                String::from_utf8_lossy(&filter.stderr)
            );
            return Err(anyhow!("tc failed installing fw filter on {}", iface));
        }

        Self::show_qdisc_state(iface);


        // this is also new down here
        debug!("[network] calling attach_classifier, attempting to attach ebpf program.");
        let handle = attach_classifier(iface, network_cgroup_target)
            .context("failed to attach eBPF classifier")?;
        self.ebpf_handle = Some(handle);
        self.applied = true;
        self.iface = Some(iface.to_string());

        debug!("[network] apply() complete");

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
                info!(iface, "root qdisc restored to fq_codel");
            }
            Ok(_) => {
                error!(iface, "failed to apply root qdisc");
            }
            Err(e) => {
                error!(error = %e, "failed to execute tc");
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
                "qdisc", "replace", "dev", iface, "root", "netem", "delay", &delay, "loss", &loss,
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
            debug!("nothing applied; skipping revert");
            return Ok(());
        }

        let iface = self.iface.as_deref().unwrap();

        // Deterministic revert: restore the known-good root qdisc.
        Self::create_restore_root(iface);

        // Verbose verification
        Self::show_qdisc_state(iface);
        self.ebpf_handle = None;
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
                info!(iface, "root qdisc deleted");
            }
            Ok(_) => {
                error!(iface, "failed to delete root qdisc (there may not be one)");
            }
            Err(e) => {
                error!(error = %e, "failed to execute tc");
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
    if !plan.injectors.network_config.enabled {
        debug!("network injector not enabled; skipping");
        return Ok(());
    }

    // need to resolve iface ONE time here, because we were getting it in multiple spots and
    // this was causing a grotesque error where we couldn't declare things publically
    // and except them to cooperate between functions

    let mut iface = plan
        .targets
        .iface
        .as_deref()
        .ok_or_else(|| anyhow!("targets.iface required for ping report"))?
        .to_string();

    if iface == "default" {
        iface = get_default_iface()
            .ok_or_else(|| anyhow!("could not determine default interface via `ip route get`"))?;
    }

    let iface_str: &str = &iface;
    debug!("[network] run_plan({})", iface_str);
    // apply the mutators
    let mut qdisc = NetworkConfig::default();
    qdisc.apply(plan, iface_str)?;

    // Maintain our ctrl-c functionality
    let dev = iface.clone();
    debug!("[network] setting ctrl-c handler for safe quit");

    ctrlc::set_handler(move || {
        warn!(iface = %dev, "Ctrl-C received; removing qdisc and exiting");
        NetworkConfig::delete_root_qdisc(&dev);
        std::process::exit(130);
    })?;
    
    println!("[network] Freezing prgorgam at network for {} seconds", plan.schedule.duration_s);
    std::thread::sleep(std::time::Duration::from_secs(plan.schedule.duration_s));
    // ugh
    // Revert
    //qdisc.revert()?;

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

fn root_prio_exists(iface: &str) -> Result<bool> {
    let output = Command::new("tc")
        .args(["qdisc", "show", "dev", iface])
        .output()
        .context("failed to execute tc qdisc show")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .any(|line| line.contains("qdisc prio") && line.contains("root") && line.contains("1:")))
}

pub fn get_default_iface() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "get", "8.8.8.8"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for: "dev <iface>"
    stdout
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "dev")
        .map(|w| w[1].to_string())
}

#[cfg(test)]
mod tests {
    //wew

    use super::*;

    #[test]
    fn validate_iface_exists_empty() {
        validate_iface_exists(None).unwrap();
    }

    #[test]
    fn validate_iface_exists_invalid() {
        let err = validate_iface_exists(Some("test"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("network interface not found"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn validate_iface_exists_success() {
        // there does exist the change of ip not being available, which can happen
        // in the case that we are not running as root

        let ip_exists = std::process::Command::new("sh")
            .args(["-c", "command -v ip >/dev/null 2>&1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !ip_exists {
            eprintln!("Skipping test: `ip` not installed");
            return;
        }

        let iface = get_default_iface().expect("Could not determine default interface");

        validate_iface_exists(Some(&iface)).unwrap();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn validate_iface_exists_failure() {
        std::process::Command::new("sh")
            .args(["-c", "command -v ip >/dev/null 2>&1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let err = validate_iface_exists(Some("test")).unwrap_err().to_string();
        assert!(err.contains("network interface not found"))
    }
}