//! Network stack module
//!
//! Provides the interactive CLI interface for network-related chaos testing.
//!
//! This module is responsible for:
//! - Prompting the user for inputs
//! - Validating network interface existence
//! - Running baseline vs. during-chaos comparisons
//! - Delegating execution to the controller layer
//!
//! **Important:** This module does not implement `tc`/qdisc logic directly.
//! All execution is delegated to the controller layer (e.g. [`chaosfilter_controller::qdiscs`])
//! to maintain separation between UI and system-level operations.

use anyhow::{Context, Ok, Result};
use chaosfilter_common::{RunLikeArgs, prompt};
use chaosfilter_controller::qdiscs::{QdiscNetem, run_plan};
use std::io::{self, Write};
use std::process::Command;


/// Runs the interactive network stack module.
///
/// This function:
/// 1. Prompts the user for a valid network interface.
/// 2. Validates the interface using the `ip link show` command.
/// 3. Displays a network-specific submenu.
/// 4. Routes selections to the appropriate qdisc operations.
/// 5. Continues looping until the user returns to the main menu.
///
/// # Returns
/// This function returns `()` when the user exits the network module.
///
/// # Behavior
/// The following submenu options are supported:
/// - `1` → [`QdiscNetem::show_qdisc_state`]
/// - `2` → [`QdiscNetem::create_restore_root`]
/// - `3` → Runs an interactive chaos plan via [`run_plan_inputs`]
/// - `4` → [`QdiscNetem::delete_root_qdisc`]
/// - `5` → Returns to the main menu
///
/// # Side Effects
/// - Reads user input from standard input.
/// - Writes output to standard output.
/// - Executes system commands (`ip`, `tc`) through controller helpers.
/// - May apply or revert network chaos.
///
/// # Requires
/// - A valid Linux network interface.
/// - CAP_NET_ADMIN privileges for qdisc operations (typically via `sudo`).
///
/// # Errors
/// This function does not return a [`Result`].  
/// Errors during chaos execution are printed to stderr.
///
/// # Panics
/// Panics if stdin/stdout operations fail due to internal `unwrap()` usage.
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

/// Collects interactive inputs and executes a network chaos plan.
///
/// This helper function gathers delay, packet loss, and duration values
/// from the user, constructs a [`RunLikeArgs`] instance, builds a plan,
/// and executes it via [`run_plan`].
///
/// # Arguments
/// * `iface` - The validated network interface to apply chaos against.
///
/// # Returns
/// Returns `Ok(())` if the chaos plan executes successfully.
///
/// # Side Effects
/// - Prompts the user for delay, loss, and duration values.
/// - Constructs a [`RunLikeArgs`] configuration.
/// - Builds a plan via [`RunLikeArgs::plan_from_args`].
/// - Executes the plan via [`run_plan`].
///
/// # Errors
/// Returns an error if:
/// - Delay, loss, or duration cannot be parsed.
/// - Plan construction fails.
/// - Chaos execution fails.
///
/// # Requires
/// CAP_NET_ADMIN privileges when applying qdisc modifications.
pub fn run_plan_inputs(iface: &str) -> Result<()> {
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
