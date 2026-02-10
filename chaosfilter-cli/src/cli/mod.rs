//! CLI Runner
//! 
//! Runs the menu loop, reading selections from [`menu::show_main_menu`]
//! and routing them via [`router::route`].

pub mod menu;
pub mod router;

/// Runs the actual interactive CLI menu loop.
pub fn run() {
	loop {
		let choice = menu::show_main_menu();

		if !router::route(choice) {
			break;
		}
	}

	println!("Exiting ChaosFilter.");
}
