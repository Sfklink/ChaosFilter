//! ChaosFilter CLI
//!
//! Delegates execution to [`chaosfilter_cli::entry`], which
//! parses command-line arguments and routes execution accordingly.

use std::process;
use chaosfilter_cli::entry;

/// Program entrypoint for ChaosFilter.
///
/// This function forwards terminal arguments to
/// [`chaosfilter_cli::entry`] and handles top-level error reporting.
///
/// # Behavior
/// - Collects command-line arguments via [`std::env::args_os`].
/// - Passes them to [`chaosfilter_cli::entry`].
/// - Prints formatted errors to standard error if execution fails.
/// - Exits the process with status code `1` on failure.
///
/// # Returns
/// This function does not return.
///
/// # Side Effects
/// - Writes errors to standard error.
/// - Terminates the process with a non-zero exit code on failure.
///
/// # Errors
/// Any error returned by [`chaosfilter_cli::entry`] will:
/// - Be printed to stderr, and
/// - Cause the process to exit with status code `1`.
///
/// # Panics
/// This function does not explicitly panic.
fn main() {
    if let Err(e) = entry(std::env::args_os()) {
        eprintln!("{:#}", e);
        process::exit(1);
    }
}
