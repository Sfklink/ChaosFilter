//! tc helpers.
//!
//! Thin wrappers around `tc qdisc` commands used by ChaosFilter.
//! These helpers are intentionally low-level and make no attempt
//! to preserve or restore previous qdisc state.

use anyhow::{anyhow, Context, Result};
use std::process::Command;

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
pub fn del_root_qdisc(iface: &str) -> Result<()> {
    let status = Command::new("tc")
        .args(["qdisc", "del", "dev", iface, "root"])
        .status()
        .context("failed to execute tc (delete)")?;

    // Best effort: do not hard-fail on cleanup
    if !status.success() {
        eprintln!(
            "WARNING: failed to delete root qdisc on {} (may not exist)",
            iface
        );
    }

    Ok(())
}
