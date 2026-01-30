use anyhow::{Result, bail};
use std::env;

//import the qdisc helper functions 
mod qdisc;

//entry point for the userspace biary
//acts as a small CLI wrapper around Linux 'tc' operations for netem-based chaos
//usage:
//	aya-qdisc add <iface> <delay_ms> <loss_pct>
//	aya-qdisc mod <iface> <delay_ms> <loss_pct>
//	aya-qdisc del <iface>
fn main() -> Result<()> {
	//collects command line args
	let args: Vec<String> = env::args().collect();

	//allows a 'dry run' for testing
	let dry_run = args.iter().any(|a| a == "--dry-run");

	//simple arg validation
	if args.len() < 3 {
		bail!("Usage: aya-qdisc <add|mod|del> <iface> [delay_ms] [loss_pct]");
	}

	let command = &args[1];
	let iface = &args[2];

	match command.as_str() {
		//add new netem qdisc
		"add" => {
			let delay_ms: u32 = args.get(3).unwrap_or(&"0".into()).parse()?;
			let loss_pct: f32 = args.get(4).unwrap_or(&"0".into()).parse()?;

			if dry_run {
				println!(
					"[dry-run] would execute:\n tc qdisc add dev {} root netem delay {}ms loss {}%",
					iface, delay_ms, loss_pct
				);
				return Ok(());
			}

			qdisc::add_netem(iface, delay_ms, loss_pct)?;
		}
		//mod an existing one
		"mod" => {
			if args.len() < 5 {
				bail!("mod requires delay_ms and loss_pct");
			}
			let delay_ms: u32 = args[3].parse()?;
			let loss_pct: f32 = args[4].parse()?;
			qdisc::change_netem(iface, delay_ms, loss_pct)?;
		}
		//delete the root qdisc
		"del" => {
			qdisc::del_root_qdisc(iface)?;
		}
		//unknown command
		_=> bail!("Unknown command: {}", command),
	}

	Ok(())
}
