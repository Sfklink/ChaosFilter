//! ChaosFilter CLI
//! 
//! Deletegates execution to [`chaosfilter_cli::entry`], which
//! parses arguments and routes the user.

use std::process;
use chaosfilter_cli::entry;

/// ChaosFilter program entrypoint
/// 
/// calls [`chaosfilter_cli::entry`] with the terminal arguments
/// and exits with a non-zero status code on error.
fn main() {
    if let Err(e) = entry(std::env::args_os()) {
        eprintln!("{:#}", e);
        process::exit(1);
    }
}
