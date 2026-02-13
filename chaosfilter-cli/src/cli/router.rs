//! Router
//! 
//! Central dispatcher that maps menu options to module entry points.


/// Routes a user menu selection to the appropriate subsystem module.
///
/// This function acts as the central dispatcher for the CLI,
/// invoking the corresponding subsystem entry point based on
/// the numeric menu selection.
///
/// # Arguments
/// * `choice` - Numeric menu option selected by the user.
///
/// # Returns
/// Returns:
/// - `true` if the application should continue running.
/// - `false` if the application should terminate (Exit selected).
///
/// # Behavior
/// The following mappings are performed:
/// - `1` → [`crate::modules::network::run`]
/// - `2` → [`crate::modules::disk::run`]
/// - `3` → [`crate::modules::cpu::run`]
/// - `4` → Signals termination by returning `false`.
/// - Any other value prints an "Invalid selection" message.
///
/// # Side Effects
/// - Prints status messages to standard output.
/// - Invokes subsystem module `run` functions.
///
/// # Errors
/// This function does not return a [`Result`].  
/// Any errors occurring inside subsystem modules must be handled
/// by those modules themselves.
///
/// # Panics
/// This function does not explicitly panic.  
/// It may propagate panics from invoked subsystem `run` functions.
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
