use anyhow::{Result, anyhow};
use std::io::{self, Write};

use crate::qdisc;

//runs a single interactive command
//	shows avaliable commands
//	reads one line from stdin
//	applies the change
//	shows the updated qdisc state
pub fn run() -> Result<()> {
	println!();
	println!("Available commands:");
	println!("  add <iface> <delay_ms> <loss_pct>");
	println!("  mod <iface> <delay_ms> <loss_pct>");
	println!("  del <iface>");
	println!("  quit");
	println!();

	loop {
		println!("Enter command> ");
		io::stdout().flush()?; //ensure prompt is shown

		let mut input = String::new();
		io::stdin().read_line(&mut input)?;

		let input = input.trim();

		if input.is_empty() {
			continue;
		}

		if input == "quit" || input == "exit" {
			println!("Exiting");
			break;
		}

		let parts: Vec<&str> = input.split_whitespace().collect();

		let result: Result<()> = match parts[0] {
			"add" => {
				if parts.len() != 4 {
					Err(anyhow!("Usage: add <iface> <delay_ms> <loss_pct>"))
				} else {
					(|| -> Result<()> {
						let delay_ms: u32 = parts[2].parse()?;
						let loss_pct: f32 = parts[3].parse()?;
						qdisc::add_netem(parts[1], delay_ms, loss_pct)?;
						crate::compare::compare_with_control(parts[1])?;
						Ok(())
					})()
				}
			}
			"mod" => {
				if parts.len() != 4 {
					Err(anyhow!("Usage: mod <iface> <delay_ms>, <loss_pct>"))
				} else {
					(|| -> Result<()> {
						let delay_ms: u32 = parts[2].parse()?;
						let loss_pct: f32 = parts[3].parse()?;
						qdisc::change_netem(parts[1], delay_ms, loss_pct)?;
						crate::compare::compare_with_control(parts[1])?;
						Ok(())
					})()
				}
			}
			"del" => {
				if parts.len() != 2 {
					Err(anyhow!("Useage: del <iface>"))
				} else {
					qdisc::del_root_qdisc(parts[1])?;
					Ok(())
				}
			}
			other => {
				Err(anyhow!("Unknown command: {}", other))
			}
		};

		if let Err(e) = result {
			eprintln!("Error: {e}");
			continue;
		}

		println!();
		println!("Updated qdisc state:");
		println!("--------------------");
		let state = qdisc::show_qdiscs(None)?;
		if state.trim().is_empty() {
			println!("(no qdiscs found)");
		} else {
			println!("{state}");
		}
	}
	Ok(())
}
