//! Router
//! 
//! Central dispatcher that maps menu options to module entry points.

use crate::modules;

/// Routes the user to the specified module.
/// 
/// # Arguments
/// * `choice` - Numeric menu option inputted by the user.
/// 
/// # Returns
/// Returns `true` if execution can continue, or `false` if the application should terminate.
/// 
/// # Side Effects
/// Prints status messages to standard output and invokes one of:
/// - [`crate::modules::network::run`]
/// - [`crate::modules::disk::run`]
/// - [`crate::modules::cpu::run`]
pub fn route(choice: u32) -> bool {
	match choice {
		1 => {
			println!("Loading Network Stack module...");
			modules::network::run();
		}
		2 => {
			println!("Loading Disk I/O module...");
			modules::disk::run();
                }
		3 => {
			println!("Loading CPU module...");
			modules::cpu::run();
                }
		4 => return false,
		_ => println!("Invalid selection"),
	}

	true
}
