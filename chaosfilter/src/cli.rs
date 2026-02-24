//! ChaosFilter CLI (argument parsing + routing)
<<<<<<< HEAD
=======
//!
//! This crate is responsible for:
//! - Parsing CLI arguments via [`clap`]
//! - Building a [`Plan`] from a config file
//! - Dispatching to controller operations

// Writing this here so I don't lose the thought, argument intake is handled here in cli.rs,
// then we send those off to a dispatcher function.  All we do here is intake arguments.
// We don't validate them to see if they play nice.  This is EXCLUSIVELY intake and plan generation.
>>>>>>> 2450bb8 (Add memory and network discoverability sections to CLI)

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::controller::{pid_cgroup, qdiscs};
use crate::controller::pid_cgroup::validate_memory_config;
use crate::{Plan, RunConfigArgs};

/// Top-level CLI argument structure.
<<<<<<< HEAD
=======
///
/// Root layout:
///   chaosfilter <COMMAND>
///
/// Sections / modules:
///   network
///   memory
///
/// We keep the legacy top-level commands too:
///   validate
///   chaos
>>>>>>> 2450bb8 (Add memory and network discoverability sections to CLI)
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

Sections:
  network   - network chaos operations (tc/qdisc)
  memory    - memory/cgroup chaos operations

Use 'chaosfilter <command> --help' for detailed information.",
    after_help = "\
Usage: chaosfilter <COMMAND>

EXAMPLES:
  chaosfilter validate --config <config_file>.toml
  chaosfilter chaos --config <config_file>.toml

  chaosfilter network --help
  chaosfilter memory --help

  chaosfilter network apply --config <config_file>.toml
  chaosfilter memory apply --config <config_file>.toml

DISCOVERABILITY:
  chaosfilter --help
  chaosfilter validate --help
  chaosfilter chaos --help
  chaosfilter network --help
  chaosfilter memory --help
"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

<<<<<<< HEAD
/// Available CLI subcommands.
=======
/// Top-level commands.
///
/// Important clap detail:
/// For nested subcommands, use struct variants with `#[command(subcommand)]` fields.
/// This prevents clap from trying to treat nested enums as `Args`.
>>>>>>> 2450bb8 (Add memory and network discoverability sections to CLI)
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan safely
    Validate(RunConfigArgs),

    /// Execute a chaos plan
    Chaos(RunConfigArgs),

    /// Network section
    Network {
        #[command(subcommand)]
        cmd: NetworkCmd,
    },

    /// Memory section
    Memory {
        #[command(subcommand)]
        cmd: MemoryCmd,
    },
}

/// Subcommands under `chaosfilter network`
#[derive(Subcommand, Debug)]
pub enum NetworkCmd {
    /// Validate network portion of a plan
    Validate(RunConfigArgs),

    /// Apply network injector (tc/qdisc)
    Apply(RunConfigArgs),
}

/// Subcommands under `chaosfilter memory`
#[derive(Subcommand, Debug)]
pub enum MemoryCmd {
    /// Validate memory portion of a plan
    Validate(RunConfigArgs),

    /// Apply memory injector (cgroup knobs)
    Apply(RunConfigArgs),
}

/// Run the full plan (legacy path).
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
        // Legacy top-level validate: validate everything we can safely validate.
        Commands::Validate(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            validate_memory_config(&plan)?;
            crate::validate_plan(&plan)?;
            Ok(())
        }
<<<<<<< HEAD
=======

        // Legacy top-level chaos: run both injectors (when enabled)
>>>>>>> 2450bb8 (Add memory and network discoverability sections to CLI)
        Commands::Chaos(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            validate_memory_config(&plan)?;
            crate::validate_plan(&plan)?;
            run_plan(&plan)?;
            Ok(())
        }

        // New section: network
        Commands::Network { cmd } => match cmd {
            NetworkCmd::Validate(args) => {
                let plan = Plan::load_from_toml_file(&args.config)?;
                // Reuse existing environment validation (iface existence, etc.)
                crate::validate_plan(&plan)?;
                Ok(())
            }
            NetworkCmd::Apply(args) => {
                let plan = Plan::load_from_toml_file(&args.config)?;
                // Only run network injector
                qdiscs::run_plan(&plan)?;
                Ok(())
            }
        },

        // New section: memory
        Commands::Memory { cmd } => match cmd {
            MemoryCmd::Validate(args) => {
                let plan = Plan::load_from_toml_file(&args.config)?;
                validate_memory_config(&plan)?;
                Ok(())
            }
            MemoryCmd::Apply(args) => {
                let plan = Plan::load_from_toml_file(&args.config)?;
                validate_memory_config(&plan)?;
                // Only run memory injector
                pid_cgroup::run_plan(&plan)?;
                Ok(())
            }
        },
    }
}
<<<<<<< HEAD
=======


>>>>>>> 2450bb8 (Add memory and network discoverability sections to CLI)
