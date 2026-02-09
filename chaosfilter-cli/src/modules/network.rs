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

//HEY PAY ATTENTION TO THIS
//I started with 2 qdisc control and the one to modify I changed to a before
//and while the chaos is active. The names of anything that could be called
//at a later point were not changed cause I am lazy. Just know if it mentions
//'control' that is now 'before' and 'modified' became 'during'. The output did
//change to reflect this update

use std::io::{self, Write};
use std::process::Command;
use std::os::unix::fs::PermissionsExt;
use std::fs;
use std::path::PathBuf;

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

//This finds the IP of the interface the user selected
//allowing for the use of more than just using a qdics made by our
//shell script
fn get_iface_ip(iface: &str) -> Option<String> {
	let output = Command::new("ip")
		.args(["-4", "addr", "show", "dev", iface])
		.output()
		.ok()?;

	let stdout = String::from_utf8_lossy(&output.stdout);

	for line in stdout.lines() {
		let line = line.trim();
		if line.starts_with("inet ") {
			let ip = line
				.split_whitespace()
				.nth(1)?
				.split('/')
				.next()?;
			return Some(ip.to_string());
		}
	}

	None
}

//This function checks to see if the user selected interface is in the host namespace
//and if not where is it
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

//This function is what does the 'ping'ing
//Important without this no test results and thats bad
fn run_ping_test(iface: &str, count: u32, _target: &str) -> Option<PingStats> {
	let iface_ip = get_iface_ip(iface)?;
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
			&iface_ip,
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
	}

	if let Err(e) = std::fs::set_permissions(
		&path,
		std::fs::Permissions::from_mode(0o644),
		) {
		println!("Warning: could not set permissions on results file: {}", e);
	}

	println!("Results written to {}", path);
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

//This function allows the user to create a new paied veth interface
//called what ever that would like. It runs a shell script
//For more detail look at comments in /CLI-scripts/scripts/chaos-net-add-iface.sh 
fn create_root_qdisc() {
	println!();
	println!("Create root qdisc:");
	
	let name = prompt("Interface name");

	if name.is_empty() {
		println!("ERROR: interface name cannot be empty");
		return;
	}

	if name.ends_with("-peer") {
		println!("ERROR: interface name must not end with '-peer'");
		return;
	}

	let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.parent().unwrap().parent().unwrap()
		.join("scripts/chaos-net-add-iface.sh");

	let script_path = match script_path.canonicalize() {
		Ok(p) => p,
		Err(e) => {
			println!("ERROR: could not resolve script path: {}", e);
			return;
		}
	};

	println!("Creating interface pair '{} <-> {}-peer'...", name, name);

	let status = Command::new("sudo")
		.args([
			"bash", 
			script_path.to_str().unwrap(),
			&name,
		])
		.status();

	match status {
		Ok(s) if s.success() => {
			println!("Interface '{}' created successfully", name);
		}
		Ok(_) => {
			println!("ERROR: interface creation failed");
		}
		Err(e) => {
			println!("ERROR: failed to execute add-iface script: {}", e);
		}
	}
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

//Deltes the root qdisc on a given interface

//This is destructive
//It removes the root qdisc entirely
//The kernel will fall back to its default qdisc behavior

//This is NOT a state reset
//It will not restore a previously existing qdisc configuration
//It is intended as a cleanup/recovery mechanism and a way to force a known baseline
fn delete_root_qdisc() {
	println!();
	println!("Delete interface:");

	let name = prompt("Interface name");

	if name.is_empty() {
		println!("ERROR: interface name cannot be empty");
		return;
	}

	if name.ends_with("-peer") {
		println!("ERROR: specify base interface name(not -peer)");
		return;
	}

	let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.parent().unwrap().parent().unwrap()
		.join("scripts/chaos-net-del-iface.sh");

	let script_path = match script_path.canonicalize() {
		Ok(p) => p,
		Err(e) => {
			println!("ERROR: coulf not resolve delete script path: {}", e);
			return;
		}
	};

	println!("Deleting interface pair '{} <-> {}-peer'...", name, name);

	let status = Command::new("sudo")
		.args([
			"bash",
			script_path.to_str().unwrap(),
			&name,
		])
		.status();

	match status {
		Ok(s) if s.success() => {
			println!("Interface '{}' deleted successfully", name);
		}
		Ok(_) => {
			println!("ERROR: interface deletion failed");
		}
		Err(e) => {
			println!("ERROR: failed to execute delete script: {}", e);
		}
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
