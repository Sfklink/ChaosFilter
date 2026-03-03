//! ChaosFilter CLI (argument parsing + routing)
//!
//! This crate is responsible for:
//! - Parsing CLI arguments via [`clap`]
//! - Building a [`chaosfilter_common::Plan`] from a config file or inline flags
//! - Dispatching to controller operations
//!  WRONG.  But good idea, but WRONG.
//!     - [`chaosfilter_common::validate_plan`]
//!     - [`chaosfilter_controller::qdiscs::run_plan`]
//!     - [`cli::run`]

// Writing this here so I don't lose the thought, argument intake is handled here in cli.rs,
// then we CAN send those off to a dispatcher function.  All we do here is intake arguments.
// We don't validate them to see if they play nice.  This is EXCLUSIVELY intake and plan generation.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level CLI argument structure.
///
/// This struct represents the root of the CLI command tree.
/// It is parsed using [`clap::Parser`] and contains the selected
/// subcommand.
///
/// # Behavior
/// Delegates execution to one of the variants in [`Commands`].
#[derive(Parser, Debug)]
#[command(name = "chaosfilter-cli", version, about = "ChaosFilter CLI & UI")]
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
/// - [`Commands::Schema`] → Print a sample config file with variable descriptions.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan (from config or inline flags)
    Validate(RunConfigArgs),

    /// Run the chaos plan (apply -> hold -> revert)
    Chaos(RunConfigArgs),

    /// Print a sample config file with variable descriptions.
    Schema,
}

/// Injector configuration block.
///
/// Each field represents configuration for a specific chaos mechanism.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    #[serde(default)]
    pub network_config: NetworkConfig,
    #[serde(default)]
    pub memory_config: MemoryConfig,
}

/// Configuration for the `controller/qdisc.rs` injector.
/// THIS IS MISSING QUITE A BIT, WHAT'S THE INTERFACE THAT WE'RE CONNECTING TO
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    /// Master enable flag for this injector.
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub target_iface: Option<String>,

    /// Packet delay in milliseconds.
    #[serde(default)]
    pub delay_ms: u32,

    /// Packet loss percentage (`0.0`–`100.0`).
    #[serde(default)]
    pub loss_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    /// Master enable flag for this injector.
    #[serde(default)]
    pub enabled: bool,

    /// PID to move / apply limits to.
    pub target_pid: Option<u32>,

    /// If true, move PID into the target cgroup before writing new config.
    #[serde(default)]
    pub move_pid: bool,

    /// Controllers to enable on the *parent* subtree_control (v2).
    /// Example: ["cpu", "memory"]
    #[serde(default)]
    pub enable: Vec<String>,

    /// cpu.max value, stored in cgroup v2 format: "max 100000" or "50000 100000"
    pub cpu_max: Option<String>,

    pub cpu_weight: Option<u32>,

    pub mem_max: Option<String>,
    pub mem_high: Option<String>,
    pub swap_max: Option<String>,
}

/// Target selection for chaos execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Targets {
    /// Optional cgroup path relative to `/sys/fs/cgroup`.
    #[serde(default)]
    pub cgroup: Option<String>,

    /// Network interface name (e.g. `enp5s0`).
    pub iface: Option<String>,
}

/// Execution timing parameters for a chaos plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Duration to hold chaos in milliseconds.
    pub duration_s: u64,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CommonCommand {
    Validate(RunConfigArgs),
    Chaos(RunConfigArgs),

}

#[derive(Debug, Clone, Args)]
pub struct RunConfigArgs {
    /// Path to TOML config
    /// Used in chaosfilter --config <filename_here>.toml
    #[arg(short, long)]
    pub config: String,
}

/// Top-level chaos plan configuration.
///
/// A `Plan` fully describes *what* chaos to run, *where* to run it,
/// and *for how long*. It is consumed by the controller layer and
/// should be treated as immutable once execution begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Human-readable plan name (for logs/UI).
    pub name: String,

    /// Where chaos is applied (iface/cgroup).
    pub targets: Targets,

    /// How long chaos should run.
    pub schedule: Schedule,

    /// Injector configuration (netem, etc.).
    #[serde(default)]
    pub injectors: Injectors,
}

impl Plan {
    /// Loads a [`Plan`] from a TOML configuration file.
    ///
    /// # Arguments
    /// * `path` - Path to a TOML file describing a chaos plan.
    ///
    /// # Returns
    /// Returns a fully-deserialized [`Plan`] on success.
    ///
    /// # Side Effects
    /// Reads the configuration file from disk.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the TOML cannot be parsed.
    pub fn load_from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let s = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let plan: Plan = toml::from_str(&s)
            .with_context(|| format!("failed to parse TOML in: {}", path.display()))?;
        Ok(plan)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use clap::Parser;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn clap_parses_config() {
        let toml = r#"
            name = "test-config"

            [targets]
            iface = "enp5s0"
            cgroup = "test-cgroup"

            [schedule]
            duration_s = 1

            [injectors.network_config]
            enabled = false

            [injectors.memory_config]
            enabled = false
            "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{toml}").unwrap();
        let filepath = file.path().to_str().unwrap();
        
        let cli = Cli::try_parse_from([
            "chaosfilter",
            "validate",
            "--config",
            filepath,
        ]).unwrap();

        match cli.command {
            Commands::Validate(args) => assert_eq!(args.config, filepath),
            _ => panic!("expected Validate command"),
        }
    }

    #[test]
    fn clap_rejects_config_missing_args() {
        let err = Cli::try_parse_from([
            "chaosfilter", "validate"
        ]).unwrap_err().to_string();

        assert!(err.contains("a value is required for") || err.contains("--config"))
    }

    #[test]
    fn load_from_toml_ok_with_defaults() {
        let toml = r#"
            name = "test-config"

            [targets]
            iface = "enp5s0"
            cgroup = "test-cgroup"

            [schedule]
            duration_s = 1
            "#;

        // Injectors were left out here to verify #[serde(default)]

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{toml}").unwrap();

        let plan = Plan::load_from_toml_file(file.path()).unwrap();

        assert_eq!(plan.name, "test-config");
        assert_eq!(plan.targets.cgroup.as_deref(), Some("test-cgroup"));
        assert_eq!(plan.targets.iface.as_deref(), Some("enp5s0"));
        assert_eq!(plan.schedule.duration_s, 1);

        // Verify defaults
        assert!(!plan.injectors.network_config.enabled);
        assert!(!plan.injectors.memory_config.enabled);
        assert_eq!(plan.injectors.memory_config.enable.len(), 0);
    }

    #[test]
    fn load_from_toml_ok() {
        let toml = r#"
            name = "test-config"

            [targets]
            iface = "enp5s0"
            cgroup = "test-cgroup"

            [schedule]
            duration_s = 1
            
            [injectors.network_config]
            enabled = true

            [injectors.memory_config]
            enabled = true
            "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{toml}").unwrap();

        let plan = Plan::load_from_toml_file(file.path()).unwrap();

        assert_eq!(plan.name, "test-config");
        assert_eq!(plan.targets.cgroup.as_deref(), Some("test-cgroup"));
        assert_eq!(plan.targets.iface.as_deref(), Some("enp5s0"));
        assert_eq!(plan.schedule.duration_s, 1);
        assert_eq!(plan.injectors.network_config.enabled, true);
        assert_eq!(plan.injectors.memory_config.enabled, true);
        assert_eq!(plan.injectors.memory_config.enable.len(), 0);
    }

    #[test]
    fn load_from_toml_file_missing_file() {
        let err = Plan::load_from_toml_file("/non-existent-folder")
            .unwrap_err()
            .to_string();

        assert!(err.contains("failed to read config file:"))
    }
}