//Handles all the user menu prompts and input parsing

//menu will display the menu options, read the user input,
//and return a normalized selection

//There is no logic handling, validation beyond parsing, or module dispatching

use std::io::{self, Write};

//Shows the main menu and returns the users selection

//Return options:
// - A # relating to the users selection
// - A 0 if parsing failed

//The simple design is resilient to invalid user input

pub fn show_main_menu() -> u32 {
	println!();
	println!("What system would you like to test?");
	println!("1) Network Stack");
	println!("2) Disk I/O");
	println!("3) CPU / Scheduling");
	println!("4) Exit");

	println!(">");
	io::stdout().flush().unwrap();

	let mut input = String::new();
	io::stdin().read_line(&mut input).unwrap();

	input.trim().parse::<u32>().unwrap_or(0)
}
