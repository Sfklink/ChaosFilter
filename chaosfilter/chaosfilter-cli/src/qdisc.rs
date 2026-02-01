use anyhow::{Context, bail, Result};
use std::process::Command;

/* ================= IFB HELPERS ================= */

fn ensure_ifb() -> Result<()> {
    // Load IFB kernel module (safe if already loaded)
    let _ = Command::new("modprobe").arg("ifb").status();

    // Check if ifb0 exists inside chaos namespace
    let exists = Command::new("ip")
        .args(["netns", "exec", "chaos", "ip", "link", "show", "ifb0"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !exists {
        Command::new("ip")
            .args([
                "netns", "exec", "chaos",
                "ip", "link", "add", "ifb0", "type", "ifb",
            ])
            .status()
            .context("failed to create ifb0 in chaos namespace")?;
    }

    Command::new("ip")
        .args([
            "netns", "exec", "chaos",
            "ip", "link", "set", "ifb0", "up",
        ])
        .status()
        .context("failed to bring up ifb0")?;

    Ok(())
}

fn attach_ingress_redirect() -> Result<()> {
    // Ensure clsact exists on vethB
    let _ = Command::new("ip")
        .args([
            "netns", "exec", "chaos",
            "tc", "qdisc", "add",
            "dev", "vethB",
            "clsact",
        ])
        .status();

    // Redirect ingress traffic → ifb0
    let _ = Command::new("ip")
        .args([
            "netns", "exec", "chaos",
            "tc", "filter", "add",
            "dev", "vethB",
            "ingress",
            "matchall",
            "action", "mirred",
            "egress", "redirect",
            "dev", "ifb0",
        ])
        .status();

    Ok(())
}

fn apply_ingress_netem(delay: &str, loss: &str) -> Result<()> {
    Command::new("ip")
        .args([
            "netns", "exec", "chaos",
            "tc", "qdisc", "replace",
            "dev", "ifb0",
            "root",
            "netem",
            "delay", delay,
            "loss", loss,
        ])
        .status()
        .context("failed to apply ingress netem")?;

    Ok(())
}

/* ================= UI HELPERS ================= */

fn alias_qdisc_output(s: &str) -> String {
    s.replace("dev vethB", "dev vethA")
     .replace("dev control-peer", "dev control")
}

/* ================= CORE HELPERS ================= */

fn ensure_not_control(dev: &str) -> Result<()> {
    if dev == "control" {
        bail!("the 'control' interface is read-only and cannot be modified");
    }
    Ok(())
}

fn resolve_tc_target<'a>(dev: &'a str) -> (&'a str, Option<&'static str>) {
    match dev {
        "vethA" => ("vethB", Some("chaos")),
        "control" => ("control", None),
        other => (other, None),
    }
}

/* ================= SHOW ================= */

pub fn show_qdiscs(iface: Option<&str>) -> Result<String> {
    let mut output = String::new();

    // HOST namespace
    output.push_str("HOST namespace:\n");
    output.push_str("----------------\n");

    let mut host_cmd = Command::new("tc");
    host_cmd.args(["qdisc", "show"]);

    if let Some(iface_name) = iface {
        if iface_name != "vethA" {
            host_cmd.args(["dev", iface_name]);
        }
    }

    let host_out = host_cmd.output()?;
    if host_out.status.success() {
        output.push_str(&String::from_utf8_lossy(&host_out.stdout));
    }

    // CHAOS namespace
    output.push_str("\nCHAOS namespace:\n");
    output.push_str("----------------\n");

    let mut chaos_cmd = Command::new("ip");
    chaos_cmd.args(["netns", "exec", "chaos", "tc", "qdisc", "show"]);

    if let Some(iface_name) = iface {
        let mapped = match iface_name {
            "vethA" => "vethB",
            _ => iface_name,
        };
        chaos_cmd.args(["dev", mapped]);
    }

    let chaos_out = chaos_cmd.output()?;
    if chaos_out.status.success() {
        let raw = String::from_utf8_lossy(&chaos_out.stdout);
        output.push_str(&alias_qdisc_output(&raw));
    }

    Ok(output)
}

/* ================= NETEM ================= */

pub fn add_netem(dev: &str, delay_ms: u32, loss_pct: f32) -> Result<()> {
    ensure_not_control(dev)?;

    let delay = format!("{delay_ms}ms");
    let loss = format!("{loss_pct}%");

    let (real_dev, netns) = resolve_tc_target(dev);

    if let Some(ns) = netns {
        Command::new("ip")
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
            .context("failed to apply egress netem")?;
    } else {
        Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", real_dev,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to apply egress netem on host")?;
    }

    if dev == "vethA" {
        ensure_ifb()?;
        attach_ingress_redirect()?;
        apply_ingress_netem(&delay, &loss)?;
    }

    Ok(())
}

pub fn change_netem(dev: &str, delay_ms: u32, loss_pct: f32) -> Result<()> {
    ensure_not_control(dev)?;

    let delay = format!("{delay_ms}ms");
    let loss = format!("{loss_pct}%");

    let (real_dev, netns) = resolve_tc_target(dev);

    if let Some(ns) = netns {
        Command::new("ip")
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
            .context("failed to modify egress netem")?;
    } else {
        Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", real_dev,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to apply egress netem on host")?;
    }

    if dev == "vethA" {
        ensure_ifb()?;
        apply_ingress_netem(&delay, &loss)?;
    }

    Ok(())
}

pub fn del_root_qdisc(dev: &str) -> Result<()> {
    ensure_not_control(dev)?;

    let (real_dev, netns) = resolve_tc_target(dev);

    if let Some(ns) = netns {
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
            .status()
            .context("failed to apply egress netem on host")?;
    }

    if dev == "vethA" {
        let _ = Command::new("ip")
            .args([
                "netns", "exec", "chaos",
                "tc", "qdisc", "del",
                "dev", "ifb0",
                "root",
            ])
            .status();
    }

    Ok(())
}
