//! ChaosFilter CLI (argument parsing + routing)
//! 
//! This crate's job is:
//! - Parsing CLI arguments via [`clap`]
//! - Building a [`Plan`] from a config file or flags/tags
//! - Dispatching to controller operations
//!     - [`chaosfilter_controller::validate_plan`]
//!     - [`chaosfilter_controller::run_plan`]
//!     - [`cli::run`]

use anyhow::{anyhow, Result};
use clap::{ArgGroup, Parser, Subcommand};
use chaosfilter_common::{Features, Injectors, Plan, QdiscNetem, Schedule, Targets};

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
    Run(RunLikeArgs),

    /// Launch the interactive menu UI.
    Menu,
}

#[derive(Parser, Debug, Clone)]
#[command(
    group(
        ArgGroup::new("input")
            .required(true)
            .args(&["config", "iface"])
    )
)]

/// Arguments shared by [`chaosfilter_controller::validate_plan`] and [`chaosfilter_controller::run_plan`]
/// 
/// This supports two different modes:
///     - **Config Mode:** Provide a config through `--config <path>`
///     - **Inline Mode:** Provide various arguments such as `--iface <name> --duration-ms <time in ms>`
/// 
/// The clap `ArgGroup` enforces that at least one mode (`--config` or `--iface`) is provided
pub struct RunLikeArgs {
    /// Path to TOML config (required for config mode)
    #[arg(short, long)]
    pub config: Option<String>,

    /// Network interface (required for inline mode)
    #[arg(long)]
    pub iface: Option<String>,

    /// Duration in ms (inline mode)
    #[arg(long, default_value_t = 5000)]
    pub duration_ms: u64,

    /// Optional cgroup relative to /sys/fs/cgroup (inline mode)
    #[arg(long)]
    pub cgroup: Option<String>,

    /// Netem delay in ms (inline mode)
    #[arg(long, default_value_t = 50)]
    pub netem_delay_ms: u32,

    /// Netem loss percent (inline mode)
    #[arg(long, default_value_t = 0.0)]
    pub netem_loss_percent: f32,

    /// Enable eBPF load (inline mode)
    #[arg(long, default_value_t = false)]
    pub load_ebpf: bool,
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
            let plan = plan_from_args(args)?;
            chaosfilter_controller::validate_plan(&plan)?;
            println!("Config OK: {:?}", plan);
        }
        Commands::Run(args) => {
            let plan = plan_from_args(args)?;
            chaosfilter_controller::run_plan(&plan)?;
            println!("Run complete.");
        }
        Commands::Menu => {
            cli::run();
        }
    }

    Ok(())
}

/// Builds a [`Plan`] from either a TOML config file or inline flags.
///
/// This supports two modes:
/// - **Config mode:** `--config <path>`
/// - **Inline mode:** `--iface <name>` plus optional inline flags
///
/// # Arguments
/// * `args` - Parsed CLI arguments used to construct the plan.
///
/// # Returns
/// Returns a fully-populated [`Plan`] suitable for validation and execution.
///
/// # Side Effects
/// Reads a config file from disk when `--config` is provided.
///
/// # Errors
/// Returns an error if:
/// - `--config` is provided but the file cannot be read or parsed as TOML, or
/// - inline mode is selected and `--iface` is missing.
fn plan_from_args(args: RunLikeArgs) -> Result<Plan> {
    // Mode A: config file
    if let Some(path) = args.config {
        return Plan::load_from_toml_file(path);
    }

    // Mode B: inline flags
    let iface = args
        .iface
        .ok_or_else(|| anyhow!("--iface is required when --config is not provided"))?;

    Ok(Plan {
        name: "inline".to_string(),
        targets: Targets {
            cgroup: args.cgroup,
            iface: Some(iface),
        },
        schedule: Schedule {
            duration_ms: args.duration_ms,
        },
        features: Features {
            load_ebpf: args.load_ebpf,
        },
        injectors: Injectors {
            qdisc_netem: QdiscNetem {
                delay_ms: args.netem_delay_ms,
                loss_percent: args.netem_loss_percent,
            },
        },
    })
}
