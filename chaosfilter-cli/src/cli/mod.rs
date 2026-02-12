//! CLI Runner
//! 
//! Runs the menu loop, reading selections from [`menu::show_main_menu`]
//! and routing them via [`router::route`].

pub mod menu;
pub mod router;

/// Runs the interactive ChaosFilter CLI loop.
///
/// This function continuously:
/// 1. Displays the main menu using [`menu::show_main_menu`].
/// 2. Routes the selected option via [`router::route`].
/// 3. Repeats until routing indicates termination.
///
/// The loop exits when [`router::route`] returns `false`.
///
/// # Returns
/// This function does not return a value.  
/// Execution continues until the user selects the exit option.
///
/// # Side Effects
/// - Writes menu output to standard output.
/// - Delegates execution to subsystem handlers through [`router::route`].
/// - Prints a termination message when exiting.
///
/// # Errors
/// This function does not return a [`Result`].  
/// Any errors occurring within menu display or routing are handled
/// by the respective called functions.
///
/// # Panics
/// This function itself does not explicitly panic.  
/// However, it may propagate panics from:
/// - [`menu::show_main_menu`]
/// - [`router::route`]
pub fn run() {
	loop {
		let choice = menu::show_main_menu();

		if !router::route(choice) {
			break;
		}
	}

	println!("Exiting ChaosFilter.");
}
