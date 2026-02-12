//! ChaosFilter CLI (argument parsing + routing)
//! 
//! This crate's job is:
//! - Parsing CLI arguments via [`clap`]
//! - Building a [`Plan`] from a config file or flags/tags
//! - Dispatching to controller operations
//!     - [`chaosfilter_controller::validate_plan`]
//!     - [`chaosfilter_controller::run_plan`]
//!     - [`cli::run`]

use anyhow::Result;
use chaosfilter_common::RunLikeArgs;
use clap::{Parser, Subcommand};

pub mod cli;
pub mod modules;

#[derive(Parser, Debug)]
#[command(
    name = "chaosfilter-cli", 
    version, 
    about = "ChaosFilter CLI & UI"
)]

/// Top-level CLI arguments.
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan (from config or inline flags)
    Validate(RunLikeArgs),

    /// Run the chaos plan (apply -> hold -> revert)
    Chaos(RunLikeArgs),

    /// Launch the interactive menu UI.
    Menu,
}

/// CLI entrypoint used by `main`.
///
/// Parses CLI arguments into [`Cli`] and dispatches to the selected subcommand.
///
/// # Arguments
/// * `args` - Iterator of command-line arguments (typically from `std::env::args_os()`).
///
/// # Returns
/// Returns `Ok(())` if the selected command completes successfully.
///
/// # Side Effects
/// - Prints status messages to standard output.
/// - For `run` / `validate`, may modify system state via controller/injectors (e.g. tc/qdisc).
/// - For `menu`, starts an interactive loop that reads from stdin and prints to stdout.
///
/// # Errors
/// Returns an error if:
/// - argument parsing fails,
/// - plan construction fails (config parsing or missing required inline flags),
/// - validation fails,
/// - running the plan fails,
/// - or any downstream controller operation fails.
pub fn entry<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Validate(args) => {
            let plan = args.plan_from_args()?;
            chaosfilter_common::validate_plan(&plan)?;
        }
        Commands::Chaos(args) => {
            let plan = args.plan_from_args()?;
            chaosfilter_controller::qdiscs::run_plan(&plan)?;
        }
        Commands::Menu => cli::run(),
    }

    Ok(())
}
