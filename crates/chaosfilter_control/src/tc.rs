use anyhow::{anyhow, Context, Result};
use std::process::Command;

/// Apply a root netem qdisc to an interface.
/// Requires CAP_NET_ADMIN (sudo).
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

/// Remove the root qdisc (best effort).
pub fn del_root_qdisc(iface: &str) -> Result<()> {
    let status = Command::new("tc")
        .args(["qdisc", "del", "dev", iface, "root"])
        .status()
        .context("failed to execute tc (delete)")?;

    // Best effort: do not hard-fail on cleanup
    if !status.success() {
        eprintln!(
            "[control] warning: failed to delete root qdisc on {} (may not exist)",
            iface
        );
    }

    Ok(())
}
