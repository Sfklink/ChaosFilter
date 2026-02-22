use std::process;
use clap::Parser;
use chaosfilter::cli::{Cli, Commands, Plan};
use chaosfilter::controller::{pid_cgroup, qdiscs};
use chaosfilter::controller::pid_cgroup::validate_memory_config;
use chaosfilter::controller::qdiscs::validate_iface_exists;

fn main() {
    if let Err(e) = entry(std::env::args_os()) {
        eprintln!("{:#}", e);
        process::exit(1);
    }
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
pub fn entry<I, T>(args: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Validate(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            validate_memory_config(&plan)?;
            validate_iface_exists(plan.targets.iface.as_deref())?;
            Ok(())
        }

        Commands::Chaos(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            run_plan(&plan)?;
            Ok(())
        }
    }
}



pub fn run_plan(plan: &Plan) -> anyhow::Result<()> {
    // Each module should early-return Ok(()) when its injector is disabled.
    pid_cgroup::run_plan(plan)?;
    qdiscs::run_plan(plan)?;

    Ok(())
}