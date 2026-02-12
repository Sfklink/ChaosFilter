//! Network stack module
//!
//! This module provides an interactive menu for ChaosFilter.
//! It is responsible for:
//! - Prompting the user
//! - Parsing user inputs
//! - Running baseline vs during-chaos ping comparisons
//! - Calling [`chaosfilter_controller`] to apply/revert chaos (tc/qdisc/etc.)
//!
//! **Important:** This module does not implement tc/qdisc logic in and of itself.
//! It delegates all execution to the controller layer (e.g. [`chaosfilter_controller::apply_plan`])
//! to keep the UI layer safer and more human-readable.

use anyhow::{Context, Ok, Result};
use chaosfilter_common::{RunLikeArgs, prompt};
use chaosfilter_controller::qdiscs::{QdiscNetem, run_plan};
use std::io::{self, Write};
use std::process::Command;


/// Runs the network stack interactive menu.
///
/// The menu remains active until the user returns to the main menu.
///
/// # Returns
/// Returns `()` when the user exits the network module or when the environment is not initialized.
///
/// # Side Effects
/// - Reads from stdin and prints to stdout.
/// - Runs system commands (`ip`, `tc`, `ping`) via helper functions.
/// - May apply/revert chaos through [`chaosfilter_controller::apply_plan`] and
///   [`chaosfilter_controller::revert_plan`].
///
/// # Requires
/// The chaos network environment must be initialized (via setup scripts).
///
/// # Panics
/// Panics if stdin/stdout operations fail (uses `unwrap()`).
pub fn run() {
	let mut iface;

	loop {
		iface = prompt("Interface name (e.g., enp5s0, wlo1)");

		let exists = Command::new("ip")
			.args(["link", "show", "dev", &iface])
			.status()
			.map(|s| s.success())
			.unwrap_or(false);

		if !iface.is_empty() && exists {
			break;
		}

		println!("ERROR: interface name is either empty or doesn't exist");
	}
	
	loop {
		println!();
		println!("Network Stack options:");
		println!("1) Show current qdisc state");
		println!("2) Create root qdisc");
		println!("3) Apply netem (delay + loss)");
		println!("4) Delete root qdisc");
		println!("5) Return to main menu");

		print!(">");
		io::stdout().flush().unwrap();

		let mut input = String::new();
		io::stdin().read_line(&mut input).unwrap();

		match input.trim() {
			"1" => QdiscNetem::show_qdisc_state(&iface),
			"2" => QdiscNetem::create_restore_root(&iface),
			"3" => {
				if let Err(e) = run_plan_inputs(&iface) {
					eprintln!("ERROR: {:#}", e);
				}
			},
			"4" => QdiscNetem::delete_root_qdisc(&iface),
			"5" => break,
			_ => println!("Invalid selection"),
		}
	}
}

fn run_plan_inputs(iface: &str) -> Result<()> {
	let netem_delay_ms = prompt("Delay (ms)")
		.parse().context("Invalid delay value.")?;
	
	let netem_loss_percent = prompt("Packet loss (%)")
		.parse().context("Invalid loss value.")?;
	
	let duration_s = prompt("Duration (seconds)")
		.parse().context("Invalid duration value.")?;

	let chaos_args = RunLikeArgs {
		config: None,
		iface: Some(iface.to_string()),
		duration_s,
		cgroup: None,
		netem_delay_ms,
		netem_loss_percent,
		load_ebpf: false
	};

	let plan = chaos_args.plan_from_args()?;
	run_plan(&plan)?;

	Ok(())
}
