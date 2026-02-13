//! ChaosFilter CLI (argument parsing + routing)
//!
//! This crate is responsible for:
//! - Parsing CLI arguments via [`clap`]
//! - Building a [`chaosfilter_common::Plan`] from a config file or inline flags
//! - Dispatching to controller operations
//!     - [`chaosfilter_common::validate_plan`]
//!     - [`chaosfilter_controller::qdiscs::run_plan`]
//!     - [`cli::run`]

use anyhow::Result;
use chaosfilter_common::RunLikeArgs;
use clap::{Parser, Subcommand};

// pub mod cli;
// pub mod modules;

/// Top-level CLI argument structure.
///
/// This struct represents the root of the CLI command tree.
/// It is parsed using [`clap::Parser`] and contains the selected
/// subcommand.
///
/// # Behavior
/// Delegates execution to one of the variants in [`Commands`].
#[derive(Parser, Debug)]
#[command(
    name = "chaosfilter-cli", 
    version, 
    about = "ChaosFilter CLI & UI"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI subcommands.
///
/// Each variant corresponds to a distinct execution path
/// within ChaosFilter.
///
/// # Variants
/// - [`Commands::Validate`] → Validates a chaos plan.
/// - [`Commands::Chaos`] → Executes a chaos plan (apply → hold → revert).
/// - [`Commands::Menu`] → Launches the interactive CLI UI.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan (from config or inline flags)
    Validate(RunLikeArgs),

    /// Run the chaos plan (apply -> hold -> revert)
    Chaos(RunLikeArgs)

}

/// CLI entrypoint used by `main`.
///
/// Parses CLI arguments into [`Cli`] and dispatches to the selected
/// subcommand.
///
/// # Arguments
/// * `args` - Iterator of command-line arguments (typically from
///   [`std::env::args_os`]).
///
/// # Returns
/// Returns `Ok(())` if the selected command completes successfully.
///
/// # Behavior
/// - For [`Commands::Validate`]:
///     - Builds a plan via [`RunLikeArgs::plan_from_args`].
///     - Validates the plan using [`chaosfilter_common::validate_plan`].
///
/// - For [`Commands::Chaos`]:
///     - Builds a plan via [`RunLikeArgs::plan_from_args`].
///     - Executes the plan via [`chaosfilter_controller::qdiscs::run_plan`].
///
/// - For [`Commands::Menu`]:
///     - Launches the interactive CLI loop via [`cli::run`].
///
/// # Side Effects
/// - Prints status messages to standard output.
/// - May modify system state via controller operations (e.g., `tc`, qdisc).
/// - May launch an interactive stdin/stdout loop.
///
/// # Errors
/// Returns an error if:
/// - Argument parsing fails.
/// - Plan construction fails (invalid config or missing inline flags).
/// - Validation fails.
/// - Chaos execution fails.
/// - Any downstream controller operation fails.
///
/// # Panics
/// This function does not explicitly panic.  
/// Panics may propagate from lower-level modules if not handled.
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
    }

    Ok(())
}
