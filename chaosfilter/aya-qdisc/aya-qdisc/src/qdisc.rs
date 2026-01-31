// Simple helpers for managing Linux traffic control (tc) qdiscs
// from Rust user space

use anyhow::{Result, bail, Context};
use std::process::Command;

// Protects the control qdisc from being changed to ensure a good baseline
fn ensure_not_control(dev: &str) -> Result<()> {
    if dev == "control" {
        bail!("the 'control' interface is read-only and cannot be modified");
    }
    Ok(())
}

/// Maps user-facing interface names to the *actual* interface where
/// packets egress and should be shaped.
///
/// Returns:
///   (real_interface_name, optional_namespace)
fn resolve_tc_target<'a>(dev: &'a str) -> (&'a str, Option<&'static str>) {
    match dev {
        // User says "vethA", but packets egress from vethB inside the chaos netns
        "vethA" => ("vethB", Some("chaos")),

        // Baseline stays on the host, untouched
        "control" => ("control", None),

        // Default: assume host interface
        other => (other, None),
    }
}

/// Show qdiscs (host namespace only; useful for debugging)
pub fn show_qdiscs(iface: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("tc");
    cmd.arg("qdisc").arg("show");

    if let Some(iface) = iface {
        cmd.arg("dev").arg(iface);
    }

    let output = cmd
        .output()
        .context("failed to execute 'tc qdisc show'")?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Attach a netem qdisc to the correct egress interface
pub fn add_netem(dev: &str, delay_ms: u32, loss_pct: f32) -> Result<()> {
    ensure_not_control(dev)?;

    let delay = format!("{delay_ms}ms");
    let loss = format!("{loss_pct}%");

    let (real_dev, netns) = resolve_tc_target(dev);

    if let Some(ns) = netns {
        let status = Command::new("ip")
            .args([
                "netns", "exec", ns,
                "tc", "qdisc", "replace",
                "dev", real_dev,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to apply netem in namespace")?;

        if !status.success() {
            bail!("tc netem failed inside namespace");
        }
    } else {
        let status = Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", real_dev,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to apply netem")?;

        if !status.success() {
            bail!("tc netem failed");
        }
    }

    Ok(())
}

/// Modify an existing netem qdisc
pub fn change_netem(dev: &str, delay_ms: u32, loss_pct: f32) -> Result<()> {
    ensure_not_control(dev)?;

    let delay = format!("{delay_ms}ms");
    let loss = format!("{loss_pct}%");

    let (real_dev, netns) = resolve_tc_target(dev);

    if let Some(ns) = netns {
        let status = Command::new("ip")
            .args([
                "netns", "exec", ns,
                "tc", "qdisc", "replace",
                "dev", real_dev,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to modify netem in namespace")?;

        if !status.success() {
            bail!("tc netem modify failed inside namespace");
        }
    } else {
        let status = Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", real_dev,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to modify netem")?;

        if !status.success() {
            bail!("tc netem modify failed");
        }
    }

    Ok(())
}

/// Remove the root qdisc from the correct interface
pub fn del_root_qdisc(dev: &str) -> Result<()> {
    ensure_not_control(dev)?;

    let (real_dev, netns) = resolve_tc_target(dev);

    if let Some(ns) = netns {
        // Ignore failure here (qdisc may not exist)
        let _ = Command::new("ip")
            .args([
                "netns", "exec", ns,
                "tc", "qdisc", "del",
                "dev", real_dev,
                "root",
            ])
            .status();
    } else {
        let _ = Command::new("tc")
            .args([
                "qdisc", "del",
                "dev", real_dev,
                "root",
            ])
            .status();
    }

    Ok(())
}
