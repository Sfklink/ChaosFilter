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

use std::io::{self, Write};
use std::process::Command;
use std::os::unix::fs::PermissionsExt;
use std::fs;

/// Summary statistics parsed from `ping` output.
///
/// # Notes:
/// RTT values are in milliseconds. Packet loss is a percentage in the range `0.0..=100.0`.
#[derive(Debug)]
struct PingStats {
	transmitted: u32,
	received: u32,
	loss_pct: f32,
	rtt_min: f32,
	rtt_avg: f32,
	rtt_max: f32,
}

/// Detects whether `iface` lives in a non-default network namespace.
///
/// # Arguments
/// * `iface` - Interface name to check.
///
/// # Returns
/// Returns `Some(ns_name)` if the interface output contains `link-netns <name>`.
/// Returns `None` if the interface appears to be in the host namespace or cannot be queried.
///
/// # Side Effects
/// Executes `ip link show <iface>`.
fn get_iface_netns(iface: &str) -> Option<String> {
	let output = Command::new("ip")
		.args(["link", "show", iface])
		.output()
		.ok()?;

	let stdout = String::from_utf8_lossy(&output.stdout);

	if let Some(pos) = stdout.find("link-netns") {
		let ns = stdout[pos + "link-netns".len()..]
			.trim()
			.split_whitespace()
			.next()?;
		return Some(ns.to_string());
	}

	None
}

/// Runs a ping test and parses packet loss + RTT stats.
///
/// If `iface` is in another network namespace, this uses `ip netns exec <ns> ping`.
///
/// # Arguments
/// * `iface` - Interface name to test.
/// * `count` - Number of ping packets to send.
/// * `target` - Intended target host/IP (currently unused; the function pings the interface IP).
///
/// # Returns
/// Returns `Some(PingStats)` if ping output can be collected and parsed.
/// Returns `None` if `ping` fails or output cannot be collected or parsed.
///
/// # Side Effects
/// Executes either:
/// - `ping -c <count> <iface_ip>` (host namespace), or
/// - `ip netns exec <ns> ping -c <count> <iface_ip>`.
fn run_ping_test(iface: &str, count: u32, target: &str) -> Option<PingStats> {
	let netns = get_iface_netns(iface);

	let mut cmd = if let Some(ns) = netns {
		let mut c = Command::new("ip");
		c.args(["netns", "exec", &ns, "ping"]);
		c
	} else {
		Command::new("ping")
	};

	let output = cmd
		.args([
			"-c", &count.to_string(),
			"-I", iface, target
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

/// Formats and prints a baseline vs during-chaos comparison report.
///
/// # Arguments
/// * `iface` - Interface name under test.
/// * `count` - Number of pings used to compute stats.
/// * `control` - Baseline stats collected before chaos.
/// * `modified` - Stats collected during chaos.
///
/// # Returns
/// Returns the formatted report string and prints the formatted String.
///
/// # Side Effects
/// Prints the report to standard output.
fn print_comparison(iface: &str, count:u32, control: &PingStats, modified: &PingStats) -> String {
	let output = format!(
"\n\n=== Network Comparison ({count} pings) ===

BEFORE CHAOS (baseline of {iface}):
  transmitted : {ct_tx}
  received    : {ct_rx}
  loss %      : {ct_loss}
  rtt (ms)    : min {ct_min} | avg {ct_avg} | max {ct_max}

DURING CHAOS ({iface}):
  transmitted : {md_tx}
  received    : {md_rx}
  loss %      : {md_loss}
  rtt (ms)    : min {md_min} | avg {md_avg} | max {md_max}
",
		ct_tx = control.transmitted,
		ct_rx = control.received,
		ct_loss = control.loss_pct,
		ct_min = control.rtt_min,
		ct_avg = control.rtt_avg,
		ct_max = control.rtt_max,
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

/// Writes a results report to `results/netem_<iface>_<timestamp>.txt`.
///
/// This is best-effort: failures are logged as warnings instead of returning errors.
///
/// # Arguments
/// * `iface` - Interface name used for the output filename.
/// * `contents` - Report contents to write.
///
/// # Side Effects
/// - Creates the `results/` directory if needed.
/// - Writes a timestamped file to disk.
/// - Attempts to set file permissions to `0644`.
fn write_results_file(iface: &str, contents: &str) {
	let ts = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
	let path = format!("results/netem_{}_{}.txt", iface, ts);

	if let Err(e) = std::fs::create_dir_all("results") {
		println!("Warning: could not create results directory: {}", e);
		return;
	}

	if let Err(e) = std::fs::write(&path, contents) {
		println!("Warning: could not write results file: {}", e);
	}

	if let Err(e) = std::fs::set_permissions(
		&path,
		std::fs::Permissions::from_mode(0o644),
		) {
		println!("Warning: could not set permissions on results file: {}", e);
	}

	println!("Results written to {}", path);
}

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
			"1" => show_qdisc_state(),
			"2" => create_root_qdisc(),
			"3" => apply_netem(),
			"4" => delete_root_qdisc(),
			"5" => break,
			_ => println!("Invalid selection"),
		}
	}
}

/// Displays the current qdisc state for the host namespace.
///
/// # Side Effects
/// Executes `tc qdisc show` and prints stdout/stderr.
fn show_qdisc_state() {
	println!();
	println!("----------------------");
	println!("Current qdiscs:");
	println!("----------------------");

	let output = Command::new("tc")
		.args(["qdisc", "show"])
		.output();

	match output {
		Ok(out) => {
			if !out.stdout.is_empty() {
				print!("{}", String::from_utf8_lossy(&out.stdout));
			}
			if !out.stderr.is_empty() {
				eprintln!("{}", String::from_utf8_lossy(&out.stderr));
			}
		}
		Err(e) => {
			println!("Failed to execute 'tc qdisc show': {}", e);
		}
	}
}

/// Applies or replaces a root qdisc on an existing network interface.
///
/// Prompts the user for an interface name (e.g., `enp5s0`, `wlo1`) and applies
/// a baseline root qdisc using `tc qdisc replace`.
///
/// # Side Effects
/// - Reads from stdin and prints to stdout.
/// - Executes `sudo tc qdisc replace dev <iface> root <qdisc>`.
/// - Modifies the kernel networking stack for the selected interface.
///
/// # Requires
/// - Root privileges (`sudo`) or sufficient capabilities to manage qdiscs.
/// - The `tc` and `ip` utilities available in `PATH`.
///
/// # Panics
/// Panics if stdin read or stdout flush fails (uses `unwrap()`).
fn create_root_qdisc() {
	println!();
	println!("Create/replace root qdisc:");
	
	let iface = prompt("Interface name (e.g., enp5s0, wlo1)");

	if iface.is_empty() {
		println!("ERROR: interface name cannot be empty");
		return;
	}

	let exists = Command::new("ip")
		.args(["link", "show", "dev", &iface])
		.status()
		.map(|s| s.success())
		.unwrap_or(false);

	if !exists {
        println!("ERROR: interface '{}' not found. Run `ip link` to list interfaces.", iface);
        return;
    }

	let status = Command::new("sudo")
        .args([
            "tc", "qdisc", "replace",
            "dev", &iface,
            "root",
            "fq_codel",
        ])
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

/// Applies netem parameters to an interface for a single interactive test run.
///
/// Behavior:
/// - Prompts for interface + delay + loss
/// - Loads `chaosfilter.toml` as a base plan
/// - Runs a baseline ping test (before chaos)
/// - Applies chaos via [`chaosfilter_controller::apply_plan`]
/// - Runs a ping test during chaos
/// - Reverts changes via [`chaosfilter_controller::revert_plan`]
/// - Writes a comparison report to disk
///
/// # Side Effects
/// - Reads from stdin and prints to stdout.
/// - Reads `chaosfilter.toml` from disk.
/// - Executes `ip`, `ping`, and controller operations (which may call `tc`).
/// - Writes results to `results/`.
///
/// # Requires
/// Applying chaos typically requires CAP_NET_ADMIN (often `sudo`) depending on how the controller runs.
///
/// # Panics
/// Panics if stdin/stdout operations fail (uses `unwrap()`).
fn apply_netem() {
	let ping_count = 10;
	let ping_target = "8.8.8.8";

	let iface = prompt("Interface (e.g., enp5s0, wlo1)");

	let delay = prompt("Delay (ms)");
	let loss = prompt("Packet loss (%)");

	let delay_ms: u32 = match delay.parse() {
		Ok(v) => v,
		Err(_) => {
			println!("Invalid delay value");
			return;
		}
	};

	let loss_pct: f32 = match loss.parse() {
		Ok(v) => v,
		Err(_) => {
			println!("Invalid loss value");
			return;
		}
	};
	println!();

	let config_contents = match fs::read_to_string("chaosfilter.toml") {
		Ok(c) => c,
		Err(e) => {
			println!("Failed to read chaosfilter.toml: {}", e);
			return;
		}
	};

	let mut plan: chaosfilter_common::Plan = match toml::from_str(&config_contents) {
		Ok(p) => p,
		Err(e) => {
			println!("Failed to parse chaosfilter.toml: {}", e);
			return;
		}
	};

	plan.targets.iface = Some(iface.clone());
	plan.injectors.qdisc_netem.delay_ms = delay_ms;
	plan.injectors.qdisc_netem.loss_percent = loss_pct;

	println!("Running baseline ping test (before chaos)...");
	let control_stats = match run_ping_test(&iface, ping_count, ping_target) {
		Some(s) => s,
		None => {
			println!("Failed to collect baseline ping stats");
			return;
		}
	};

	let qdisc = match chaosfilter_controller::apply_plan(&plan) {
		Ok(q) => q,
		Err(e) => {
			println!("Error applying chaos plan: {:#}", e);
			return;
		}
	};

	println!(
		"Running ping during chaos ({} ms window)...",
		plan.schedule.duration_ms
	);

	let modified_stats = match run_ping_test(&iface, ping_count, ping_target) {
		Some(s) => s,
		None => {
			println!("Failed to collect chaos ping stats");
			let _ = chaosfilter_controller::revert_plan(qdisc, &plan);
			return;
		}
	};

	std::thread::sleep(std::time::Duration::from_millis(plan.schedule.duration_ms));

	if let Err(e) = chaosfilter_controller::revert_plan(qdisc, &plan) {
		println!("Warning: failed to fully revert chaos: {:#}", e);
	}

	println!("Netem applied, observed, and reset successfully");

	let report = print_comparison(
		&iface,
		ping_count,
		&control_stats,
		&modified_stats,
	);

	write_results_file(&iface, &report);
}

/// Deletes the root qdisc from an existing network interface.
///
/// Prompts the user for an interface name (e.g., `enp5s0`, `wlo1`) and deletes the
/// root qdisc using `tc qdisc del dev <iface> root`.
///
/// # Notes
/// This is a destructive operation in the sense that it removes traffic control configuration
/// from the interface, and may affect current network behavior.
///
/// # Side Effects
/// - Reads from stdin and prints to stdout.
/// - Executes `sudo tc qdisc del dev <iface> root`.
///
/// # Requires
/// - Root privileges (`sudo`) or sufficient capabilities to manage qdiscs.
/// - The `tc` and `ip` utilities available in `PATH`.
///
/// # Panics
/// Panics if stdin read or stdout flush fails (uses `unwrap()`).
fn delete_root_qdisc() {
	println!();
	println!("Delete interface:");

	let iface = prompt("Interface name (e.g., enp5s0, wlo1)");

	if iface.is_empty() {
		println!("ERROR: interface name cannot be empty");
		return;
	}

	let exists = Command::new("ip")
		.args(["link", "show", "dev", &iface])
		.status()
		.map(|s| s.success())
		.unwrap_or(false);

	if !exists {
        println!("ERROR: interface '{}' not found. Run `ip link` to list interfaces.", iface);
        return;
    }

	let status = Command::new("sudo")
		.args([
			"tc", "qdisc", "del", 
			"dev", &iface, "root"
		])
		.status();

	match status {
		Ok(s) if s.success() => {
			println!("Root qdisc deleted successfully from '{}'", iface);
		}
		Ok(_) => {
			println!("ERROR: failed to delete root qdisc from '{}'", iface);
		}
		Err(e) => {
			println!("ERROR: failed to execute tc: {}", e);
		}
	}
}

/// Prompts the user for input and returns a trimmed response.
///
/// # Arguments
/// * `label` - Prompt label shown to the user.
///
/// # Returns
/// The trimmed user input.
///
/// # Side Effects
/// Prints to stdout and reads a line from stdin.
///
/// # Panics
/// Panics if stdout flush or stdin read fails (uses `unwrap()`).
fn prompt(label: &str) -> String {
	print!("{}: ", label);
	io::stdout().flush().unwrap();

	let mut input = String::new();
	io::stdin().read_line(&mut input).unwrap();

	input.trim().to_string()
}
