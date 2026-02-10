//cli/mod defines the top-level control flow for the CLI

//Responsible for
//	-Own the main CLI loop
//	-Display the menus
//	-Dispatch the user sections to the correct module

//It knows *what* modules exist
//but not *how* the modules work

pub mod menu;
pub mod router;

//Starts the CLI and keeps it going until the user exits

//It will display the main menu, read the user input, then dispatch control to the
//selected module and loop this process until the user selects a module or exits
pub fn run() {
	loop {
		let choice = menu::show_main_menu();

		//Dispatch returns false when the user exits
		if !router::route(choice) {
			break;
		}
	}

	println!("Exiting ChaosFilter.");
}
