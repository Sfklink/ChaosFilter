//! ChaosFilter CLI (argument parsing + routing)

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::controller::{pid_cgroup, qdiscs};
use crate::controller::pid_cgroup::validate_memory_config;
use crate::{Plan, RunConfigArgs};

/// Top-level CLI argument structure.
#[derive(Parser, Debug)]
#[command(
    name = "chaosfilter",
    version,
    about = "ChaosFilter CLI and controller interface",
    long_about = "\
ChaosFilter loads chaos plans from TOML files and dispatches them to controller modules.

Typical workflow:
  validate  - perform safe checks
  chaos     - execute plan and apply injectors

Use 'chaosfilter <command> --help' for detailed information.",
    after_help = "\
EXAMPLES:
  chaosfilter validate --config <config_file>.toml
  chaosfilter chaos --config <config_file>.toml

DISCOVERABILITY:
  chaosfilter --help
  chaosfilter validate --help
  chaosfilter chaos --help
"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan safely
    #[command(
        about = "Validate a chaos plan safely",
        long_about = "\
Loads the plan from TOML and runs lightweight validation checks.

Safe operation:
  - does not apply tc/qdisc changes
  - does not change cgroup knobs
"
    )]
    Validate(RunConfigArgs),

    /// Execute a chaos plan
    #[command(
        about = "Execute a chaos plan",
        long_about = "\
Loads the plan from TOML and executes enabled injectors.

Warning:
  - may modify system state (tc/qdisc, cgroups)
  - may require root privileges
"
    )]
    Chaos(RunConfigArgs),
}

/// Run a loaded chaos plan by dispatching to each injector.
///
/// Each module should early-return Ok(()) when its injector is disabled.
pub fn run_plan(plan: &Plan) -> Result<()> {
    pid_cgroup::run_plan(plan)?;
    qdiscs::run_plan(plan)?;
    Ok(())
}

/// CLI entrypoint used by `main`.
pub fn entry<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Validate(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            validate_memory_config(&plan)?;
            crate::validate_plan(&plan)?;
            Ok(())
        }
        Commands::Chaos(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            validate_memory_config(&plan)?;
            crate::validate_plan(&plan)?;
            run_plan(&plan)?;
            Ok(())
        }
    }
}
