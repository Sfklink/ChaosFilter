//simple helpers for managing Linux traffic control (tc) qdiscs
//from Rust user space

use anyhow::{Result, bail};
use std:: process::Command;

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
	let delay = format!("{delay_ms}ms");
	let loss = format!("{loss_pct}%");

	run_tc(&[
		"qdisc", "add",
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
	let delay = format!("{delay_ms}ms");
	let loss = format!("{loss_pct}%");

	run_tc(&[
		"qdisc", "change",
		"dev", dev,
		"root",
		"netem",
		"delay", &delay,
		"loss", &loss,
	])
}

//removes the root qdisc from a network interface
pub fn del_root_qdisc(dev:&str) -> Result<()> {
	run_tc(&[
		"qdisc", "del",
		"dev", dev,
		"root",
	])
}

