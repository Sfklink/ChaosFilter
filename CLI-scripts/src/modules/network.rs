//Entry point for network stack testing

//network defines the interactive CLI interface for all
//network stack related chaos

//It contains
//	-User prompts and menus
//	-Input parsing and validation
//	-Calls into the control crate made by Spencer

//IMPORTANT NOTE:
//This section is only calling all the tc/qdisc logic
//and does not handle/implement it itself
//That is all done by the code Spencer wrote

//This seperation makes the CLI safer, more predictable,and easier to evolve when needed

use std::io::{self, Write};
use chaosfilter_control::tc;
use std::process::Command;
use chaosfilter_common::Plan;
use chaosfilter_control::run_plan;

use std::fs;

//This struct is how we do the ping comare between the control and the modified qdisc
//Important for none coder understanding
#[derive(Debug)]
struct PingStats {
	transmitted: u32,
	received: u32,
	loss_pct: f32,
	rtt_min: f32,
	rtt_avg: f32,
	rtt_max: f32,
}

//This function is what does the 'ping'ing
//Important without this no test results and thats bad

fn run_ping_test(iface: &str, count: u32, target: &str) -> Option<PingStats> {
	let output = std::process::Command::new("ping")
		.args([
			"-I", iface,
			"-c", &count.to_string(),
			target,
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

//This makes the results look real pretty
fn print_comparison(iface: &str, count:u32, control: &PingStats, modified: &PingStats) -> String {
	let output = format!(
"=== Network Comparison ({count} pings) ===

CONTROL (baseline):
  transmitted : {ct_tx}
  received    : {ct_rx}
  loss %      : {ct_loss}
  rtt (ms)    : min {ct_min} | avg {ct_avg} | max {ct_max}

MODIFIED ({iface}):
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

//This function writes the reuslts to a text file
//It provides a more permanent solution for the results than the terminal
fn write_results_file(iface: &str, contents: &str) {
	let ts = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
	let path = format!("results/netem_{}_{}.txt", iface, ts);

	if let Err(e) = std::fs::create_dir_all("results") {
		println!("Warning: could not create results directory: {}", e);
		return;
	}

	if let Err(e) = std::fs::write(&path, contents) {
		println!("Warning: could not write results file: {}", e);
	} else {
		println!("Results written to {}", path);
	}
}

//This function verifies that the shell scripts have been run
//Very impprtant since the code can not function without them being ran
fn verify_network_environment() -> bool {
	let status = std::process::Command::new("ip")
		.args(["link", "show", "control"])
		.status();

	match status {
		Ok(s) if s.success() => true,
		_ => false,
	}
}

//This function yells at the user if the setup has not been ran
//Also important because the user need to know there place
fn print_setup_required_message() {
	println!("ERROR: Chaos network environment is not initialized.\n");
	println!("Please run the following commands from the project root:\n");
	println!("  sudo ./scripts/chaos-net-cleanup.sh   (optional, recommended)");
	println!("  sudo ./scripts/chaos-net-setup.sh\n");
	println!("Then re-run:");
	println!("  sudo -E cargo run -p chaosfilter-cli\n");
}

//Entry point called by the CLI dispatcher

//This function displays the network stack options menu which will
//reamin active until the user ends the program

//returning to the main menu marks leaving network stack testing

pub fn run() {
	if !verify_network_environment() {
		print_setup_required_message();
		return;
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
			"1" => show_qdisc_state(),
			"2" => create_root_qdisc(),
			"3" => apply_netem(),
			"4" => delete_root_qdisc(),
			"5" => break,
			_ => println!("Invalid selection"),
		}
	}
}

//Displays the current qdisc state for the host namespace
//I called it something else because I wanted to 

//This runs 'tc qdisc show' and prints the results
//It is a read only operation made to help the user see what can be modified

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

//Currently a place holder 

fn create_root_qdisc() {
	println!();
	println!("Create root qdisc:")
	//Not yet implemented will update this when possible
}

//Applies temporary netem confiurations to an interface

//Behavior:
//	Applies netem to the qdisc
//	Runs the tests
//	Automatically removes netem from qdisc afterwards

//There is no persistant state
//You can safely call repeatedly and is scoped to a single test run

fn apply_netem() {
	let ping_count = 10;
	let ping_target = "8.8.8.8";

	let iface = prompt("Interface (e.g. eth0)");

	if iface == "control" {
		println!("Error: 'control' is a protected interface and cannot be modified");
		return;
	}

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
	plan.injectors.qdisc_netem.enabled = true;
	plan.injectors.qdisc_netem.delay_ms = delay_ms;
	plan.injectors.qdisc_netem.loss_percent = loss_pct;

	println!("Running baseline ping test (control)...");
	let control_stats = match run_ping_test("control", ping_count, ping_target) {
		Some(s) => s,
		None => {
			println!("Failed to collect baseline ping stats");
			return;
		}
	};

	match run_plan(&plan) {
		Ok(_) => {
			println!("Netem applied, tested, and reset successfully");

			println!("Running modified ping test ({})...", iface);
			let modified_stats = match run_ping_test(&iface, ping_count, ping_target) {
				Some(s) => s,
				None => {
					println!("Failed to collect modified ping stats");
					return;
				}
			};

			let report = print_comparison(
				&iface,
				ping_count,
				&control_stats,
				&modified_stats
			);

			write_results_file(&iface, &report);
		}
		Err(e) => {
			println!("Error running chaos plan: {:#}", e);
		}
	}
}

//Deltes the root qdisc on a given interface

//This is destructive
//It removes the root qdisc entirely
//The kernel will fall back to its default qdisc behavior

//This is NOT a state reset
//It will not restore a previously existing qdisc configuration
//It is intended as a cleanup/recovery mechanism and a way to force a known baseline

fn delete_root_qdisc() {
	let iface = prompt("Interface (e.g. eth0)");

	if iface == "control" {
		println!("Error: control is a protected interface and cannot be modified");
		return;
	}


	println!("Deleting root qdisc on interface {}", iface);

	match tc::del_root_qdisc(&iface) {
		Ok(_) => println!("Root qdisc deleted"),
		Err(e) => println!("Error deleting root qdisc: {:#}",e),
	}
}

//Prompts the user for input and returns a trimmed response

//This helper is used for all interaction within the network stack
//module to keep I/O behavior consistant and clean

fn prompt(label: &str) -> String {
	print!("{}: ", label);
	io::stdout().flush().unwrap();

	let mut input = String::new();
	io::stdin().read_line(&mut input).unwrap();

	input.trim().to_string()
}
