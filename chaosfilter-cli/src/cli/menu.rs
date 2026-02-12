//! Main Menu Function
//! 
//! Shows the main menu, parses the input from stdin, 
//! and returns the parsed selection.

use std::io::{self, Write};

/// Displays the main ChaosFilter menu and parses the user's selection.
///
/// This function prints the available subsystems that can be tested
/// and blocks until input is received from standard input.
///
/// # Returns
/// Returns a numeric selection corresponding to the chosen subsystem:
/// - `1` → Network Stack
/// - `2` → Disk I/O
/// - `3` → CPU / Scheduling
/// - `4` → Exit
///
/// Returns `0` if the input is invalid or cannot be parsed into a `u32`.
///
/// # Side Effects
/// - Writes menu text to standard output.
/// - Flushes [`std::io::Stdout`] to ensure the prompt appears immediately.
/// - Blocks while waiting for input from [`std::io::stdin`].
///
/// # Errors
/// This function does not return a [`Result`].  
/// Invalid numeric input is handled gracefully by returning `0`.
///
/// # Panics
/// Panics if:
/// - Flushing stdout fails, or
/// - Reading from standard input fails.
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
