//simple helpers for managing Linux traffic control (tc) qdiscs
//from Rust user space

use anyhow::{Result, bail, Context};
use std:: process::Command;

//protects the control qdisc from being changed to ensure a good baseline to comapre results to
fn ensure_not_control(dev: &str) -> Result<()> {
	if dev == "control" {
		bail!("the 'control' interface is read-only and cannot be modified");
	}
	Ok(())
}

pub fn show_qdiscs(iface: Option<&str>) -> Result<String> {
	//build: tc qdisc show [dev <iface>]
	let mut cmd = Command::new("tc");
	cmd.arg("qdisc").arg("show");

	if let Some(iface) = iface {
		cmd.arg("dev").arg(iface);
	}

	let output = cmd
		.output()
		.context("failed to execute 'tc qdisc show'")?;

	//tc sometimes write useful info to stderr but we only treat *command failure as an error
	if !output.status.success() {
		return Ok(String::new());
	}

	Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

//helper function that runs the 'tc' command with the provided args
//returns an error if failure occurs instead of silently continuing
fn run_tc(args: &[&str]) -> Result<()> {
	let status = Command::new ("tc").args(args).status()?;
	if !status.success() {
		bail!("tc failed: {:?}", args);
	}
	Ok(())
}

//attaches a netem qdisc to the root of a network interface
//params are:
//	-dev: network interface name
//	-delay_ms: fixed latency in millisecs
//	-loss_pct: packet loss percentage
pub fn add_netem(dev: &str, delay_ms: u32, loss_pct: f32) -> Result<()> {
	ensure_not_control(dev)?;

	let delay = format!("{delay_ms}ms");
	let loss = format!("{loss_pct}%");

	run_tc(&[
		"qdisc", "replace",
		"dev", dev,
		"root",
		"netem",
		"delay", &delay,
		"loss", &loss,
	])
}

//mods an existing root netem qdisc
// will fail if none exist
//call add_netem first
pub fn change_netem(dev: &str, delay_ms: u32, loss_pct: f32) -> Result<()> {
	ensure_not_control(dev)?;

	let delay = format!("{delay_ms}ms");
	let loss = format!("{loss_pct}%");

	run_tc(&[
		"qdisc", "replace",
		"dev", dev,
		"root",
		"netem",
		"delay", &delay,
		"loss", &loss,
	])
}

//removes the root qdisc from a network interface
pub fn del_root_qdisc(dev:&str) -> Result<()> {
	ensure_not_control(dev)?;

	run_tc(&[
		"qdisc", "del",
		"dev", dev,
		"root",
	])
}

