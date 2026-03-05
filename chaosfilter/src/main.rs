use chaosfilter::cli::{Cli, Commands, Plan};
use chaosfilter::controller::pid_cgroup::validate_memory_config;
use chaosfilter::controller::qdiscs::validate_iface_exists;
use chaosfilter::controller::{pid_cgroup, qdiscs};
use clap::Parser;
use std::{process,fs, path::{Path, PathBuf}};
use anyhow::{Context, Result};

const CONFIG_TEMPLATE: &str =
    include_str!("../assets/init_config.toml");

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
/// - For ['Commands::Init']
///     - Outputs a .toml config file to CWD.
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

            println!("Config OK.");
            Ok(())
        }

        Commands::Chaos(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            run_plan(&plan)?;

            println!("Chaos Plan Complete.");
            Ok(())
        }
        /*
        This guy right here.
        We're going to include an initcommand that points to a .toml file so we can spawn one for the user
        and reads it out.
         */
        Commands::Init { force } => {
            let path = Path::new("cf-config.toml");
            let written_to = output_config(path, force)?;
            println!("Config written to: {}", written_to.display());
            Ok(())
        }

        Commands::Delay(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            chaosfilter::controller::block_delay::run(&plan)?;
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


pub fn output_config(path: &Path, force: bool) -> Result<PathBuf> {

    // Turn whatever the user provided into an absolute-ish path for display
    // If it's relative, make it relative to the current working directory.
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to read current working directory")?
            .join(path)
    };

    if abs_path.exists() && !force {
        anyhow::bail!(
            "{} already exists. Re-run with --force to overwrite.",
            abs_path.display()
        );
    }

    fs::write(&abs_path, CONFIG_TEMPLATE)
        .with_context(|| format!("failed to write config to {}", abs_path.display()))?;

    Ok(abs_path)
}