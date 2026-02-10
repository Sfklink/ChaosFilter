//! Main Menu Function
//! 
//! Shows the main menu, parses the input from stdin, 
//! and returns the parsed selection.

use std::io::{self, Write};

/// Displays the menu of ChaosFilter and parses the user's input.
/// 
/// This function will print all of the available subsystems that can 
/// be tested with ChaosFilter and blocks until input is received.
/// 
/// # Returns
/// A numeric selection:
/// - `1` → Network Stack
/// - `2` → Disk I/O
/// - `3` → CPU / Scheduling
/// - `4` → Exit
/// 
/// Returns '0' if the input is invalid or cannot be parsed.
/// 
/// # Panics
/// Panics if reading from standard input or flushing stdout fails.
pub fn show_main_menu() -> u32 {
	println!();
	println!("What system would you like to test?");
	println!("1) Network Stack");
	println!("2) Disk I/O");
	println!("3) CPU / Scheduling");
	println!("4) Exit");

	print!(">");
	io::stdout().flush().unwrap();

	let mut input = String::new();
	io::stdin().read_line(&mut input).unwrap();

	input.trim().parse::<u32>().unwrap_or(0)
}
