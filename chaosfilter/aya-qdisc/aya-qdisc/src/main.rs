use anyhow::{Result, bail};
use std::env;

//import the qdisc helper functions and the interactive functiodns and the ping + compare
mod qdisc;
mod interactive;
mod ping;
mod compare;

//entry point for the userspace biary
//acts as a small CLI wrapper around Linux 'tc' operations for netem-based chaos
//usage:
//	aya-qdisc add <iface> <delay_ms> <loss_pct>
//	aya-qdisc mod <iface> <delay_ms> <loss_pct>
//	aya-qdisc del <iface>
fn main() -> Result<()> {
	//collects command line args
	let args: Vec<String> = env::args().collect();

	//shows current qdisc state first
	println!("Current qdisc state:");
	println!("----------------------");
	let state = qdisc::show_qdiscs(None)?;
	if state.trim().is_empty() {
		println!("(no qdiscs found)");
	} else {
		println!("{state}");
	}

	// === INTERACTIVE MODE ===
	if args.len() == 1 {
		return interactive::run();
	}

	// === DIRECT COMMAND MODE ===
	let command = &args[1];
	let iface = args.get(2).ok_or_else(|| anyhow::anyhow!("Missing <iface> argument"))?;

	match command.as_str() {
		"add" => {
			let delay_ms: u32 = args.get(3).unwrap_or(&"0".into()).parse()?;
			let loss_pct: f32 = args.get(4).unwrap_or(&"0".into()).parse()?;
			qdisc::add_netem(iface, delay_ms, loss_pct)?;
		}
		"mod" => {
			let delay_ms: u32 = args.get(3)
				.ok_or_else(|| anyhow::anyhow!("mod requires delay_ms"))?
				.parse()?;
			let loss_pct: f32 = args.get(4)
				.ok_or_else(|| anyhow::anyhow!("mod requires loss_pct"))?
				.parse()?;
			qdisc::change_netem(iface, delay_ms, loss_pct)?;
		}
		"del" => {
			qdisc::del_root_qdisc(iface)?;
		}
		_=> {
			bail!("Unknown command {}", command);
		}
	}

	Ok(())
}

