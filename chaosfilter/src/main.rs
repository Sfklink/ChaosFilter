//! # ChaosFilter CLI & Orchestrator
//!
//! This module serves as the primary entry point for the `chaosfilter` application.
//! It handles CLI argument parsing using [`clap`], initializes logging, and
//! orchestrates the lifecycle of a chaos experiment (apply → hold → revert).

use anyhow::Context;
use chaosfilter::plans::{Plan, RunConfigArgs};
use chaosfilter::injector::cpu_memory::MemoryInjector;
use chaosfilter::injector::filesystem::FilesystemInjector;
use chaosfilter::injector::network::NetworkInjector;
use chaosfilter::injector::ChaosInjector;

use chaosfilter::validate::validate_plan;
use clap::{Parser, Subcommand};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, path::{Path, PathBuf}};
use toml_edit::{value, DocumentMut};
use tracing::{debug, info};

/// The configuration template used by [`Commands::Init`].
const CONFIG_TEMPLATE: &str =
    include_str!("../assets/schema_config.toml");

fn main() {
    if let Err(e) = entry(std::env::args_os()) {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}

/// CLI entrypoint used by [`main`].
///
/// This function is responsible for:
/// 1. Parsing command-line arguments into the [`Cli`] struct.
/// 2. Initializing the global tracing subscriber based on verbosity levels.
/// 3. Dispatching execution to the appropriate subcommand handler.
///
/// # Arguments
///
/// * `args` - An iterator of command-line arguments, typically provided by [`std::env::args_os`].
///
/// # Returns
///
/// Returns `Ok(())` if the selected command completes successfully, otherwise an [`anyhow::Result`].
///
/// # Behavior
///
/// - **[`Commands::Validate`]**: Loads a [`Plan`] from a TOML file and performs dry-run validation.
/// - **[`Commands::Chaos`]**: Executes a full chaos experiment by calling [`run_plan`].
/// - **[`Commands::Init`]**: Generates a skeleton TOML configuration file in the current directory.
///
/// # Side Effects
///
/// - Initializes global logging (via `tracing-subscriber`).
/// - Writes to the filesystem when running `init`.
/// - Modifies system state (network, cgroups, etc.) when running `chaos`.
///
/// # Errors
///
/// Returns an error if:
/// - CLI arguments are invalid.
/// - The configuration file cannot be found or parsed.
/// - Validation of the chaos plan fails.
/// - Any injector fails during the apply or revert phases.
pub fn entry<I, T>(args: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    let level: &str = match cli.verbose {
        0 => "off",
        1 => "info",
        _ => "debug",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .init();

    match cli.command {
        Commands::Validate(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            validate_plan(&plan)?;

            println!("\nConfig OK.");
            Ok(())
        }

        Commands::Chaos(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            run_plan(&plan)?;

            println!("\nChaos Plan Complete.");
            Ok(())
        }

        Commands::Init {
            force,
            pid,
            iface,
            pid_pos,
            iface_pos,
        } => {
            let pid = pid.or(pid_pos);
            let iface = iface.or(iface_pos);

            let path = Path::new("cf-config.toml");
            let written_to = output_config(path, force, pid, iface.as_deref())?;
            println!("Config written to: {}", written_to.display());

            if pid.is_none() && iface.is_none() {
                println!("No targets supplied, no injectors enabled.");
            }

            Ok(())
        }
    }
}

/// Orchestrates the execution of a chaos [`Plan`].
///
/// This function identifies which injectors are enabled in the plan,
/// applies them in sequence, waits for the configured duration (or a SIGINT),
/// and then reverts all changes.
///
/// # Arguments
///
/// * `plan` - The validated chaos [`Plan`] to execute.
///
/// # Returns
///
/// Returns `Ok(())` if all injectors were successfully applied and reverted.
///
/// # Behavior
///
/// 1. **Initialization**: Instantiates [`ChaosInjector`] implementations based on the plan.
/// 2. **Application**: Calls [`ChaosInjector::apply`] for each injector.
/// 3. **Holding**: Blocks for `plan.schedule.duration_s`. Can be interrupted by `Ctrl+C`.
/// 4. **Reversion**: Calls [`ChaosInjector::revert`] for each injector in the same order they were applied.
///
/// # Side Effects
///
/// - Modifies system state via the enabled injectors.
/// - Installs a global signal handler for `Ctrl+C`.
///
/// # Errors
///
/// Returns an error if any injector fails to apply or revert, or if the stop signal handler fails.
pub fn run_plan(plan: &Plan) -> anyhow::Result<()> {
    let mut injectors: Vec<Box<dyn ChaosInjector>> = Vec::new();

    if plan.injectors.memory_config.enabled {
        injectors.push(Box::new(MemoryInjector::default()));
    }
    if plan.injectors.network_config.enabled {
        injectors.push(Box::new(NetworkInjector::default()));
    }
    if plan.injectors.filesystem_config.enabled {
        injectors.push(Box::new(FilesystemInjector::default()));
    }

    for injector in injectors.iter_mut() {
        let n = ChaosInjector::name(injector.as_ref());
        injector
            .apply(plan.clone())
            .with_context(|| format!("apply failed ({n})"))?;
    }

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = stop_tx.send(());
    })?;

    let hold = Duration::from_secs(plan.schedule.duration_s);
    match stop_rx.recv_timeout(hold) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        Err(e) => return Err(anyhow::anyhow!("failed while waiting for stop signal: {e}")),
    }

    for injector in injectors.iter_mut() {
        let n = ChaosInjector::name(injector.as_ref());
        injector
            .revert()
            .with_context(|| format!("revert failed ({n})"))?;
    }

    Ok(())
}

/// Generates a skeleton TOML configuration file.
///
/// # Arguments
///
/// * `path` - The destination path for the config file.
/// * `force` - If `true`, overwrites the file if it already exists.
/// * `pid` - Optional PID to pre-configure in the memory injector.
/// * `interface` - Optional network interface to pre-configure in the network injector.
///
/// # Returns
///
/// Returns the absolute [`PathBuf`] to the written file.
///
/// # Behavior
///
/// - Loads [`CONFIG_TEMPLATE`].
/// - If `pid` or `interface` are provided, it automatically enables the corresponding
///   injectors and populates the target fields.
/// - Writes the resulting TOML to disk.
///
/// # Side Effects
///
/// - Writes a file to the filesystem.
///
/// # Errors
///
/// Returns an error if:
/// - The output path exists and `force` is `false`.
/// - The internal TOML template is invalid.
/// - Disk I/O fails.
pub fn output_config(
    path: &Path,
    force: bool,
    pid: Option<u32>,
    interface: Option<&str>,
) -> anyhow::Result<PathBuf> {
    debug!(pid = ?pid, iface = ?interface, force, "init args");
    
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

    let mut doc: DocumentMut = CONFIG_TEMPLATE
        .parse::<DocumentMut>()
        .context("embedded config template is invalid TOML")?;

    if let Some(iface) = interface {
        doc["injectors"]["network_config"]["enabled"] = value(true);
        doc["injectors"]["network_config"]["target_iface"] = value(iface);
        info!(iface, "network injector enabled");
    } else {
        doc["injectors"]["network_config"]["enabled"] = value(false);
    }

    if let Some(p) = pid {
        doc["targets"]["cgroup"] = value(p.to_string());
        doc["injectors"]["memory_config"]["target_pid"] = value(p.to_string());
        doc["injectors"]["memory_config"]["enabled"] = value(true);
        info!(pid = p, "memory injector enabled");
    } else {
        doc["injectors"]["memory_config"]["enabled"] = value(false);
    }

    fs::write(&abs_path, doc.to_string())
        .with_context(|| format!("failed to write config to {}", abs_path.display()))?;

    Ok(abs_path)
}

/// Top-level CLI argument structure.
///
/// This struct represents the root of the CLI command tree.
/// It is parsed using [`clap::Parser`] and contains the selected
/// subcommand.
///
/// # Behavior
///
/// Delegates execution to one of the variants in [`Commands`].
#[derive(Parser, Debug)]
#[command(name = "chaosfilter-cli", version, about = "ChaosFilter CLI & UI")]
pub struct Cli {
    /// Enable verbose logging output (use -v for info and -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI subcommands.
///
/// Each variant corresponds to a distinct execution path
/// within ChaosFilter.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan from config.
    Validate(RunConfigArgs),

    /// Run the chaos plan (apply -> hold -> revert)
    Chaos(RunConfigArgs),

    /// Generate a sample config file in the current directory.
    Init {
        /// Overwrite the file if it already exists
        #[arg(long)]
        force: bool,

        /// PID to constrain (enables memory injector)
        #[arg(long = "pid", value_name = "PID")]
        pid: Option<u32>,

        /// Network interface (enables network injector)
        #[arg(long = "iface", value_name = "IFACE")]
        iface: Option<String>,

        /// PID to constrain (positional form)
        #[arg(value_name = "PID")]
        pid_pos: Option<u32>,

        /// Network interface (positional form)
        #[arg(value_name = "IFACE")]
        iface_pos: Option<String>,
    },
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

            [injectors]

            [injectors.memory_config]
            target_pid = 123

            [injectors.network_config]
            target_iface = "enp5s0"

            [schedule]
            duration_s = 1
            "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{toml}").unwrap();

        let plan = Plan::load_from_toml_file(file.path()).unwrap();

        assert_eq!(plan.name, "test-config");
        assert_eq!(plan.injectors.memory_config.target_pid, Some(123));
        assert_eq!(plan.injectors.network_config.target_iface.as_deref(), Some("enp5s0"));
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
            target_iface = "enp5s0"

            [injectors.memory_config]
            enabled = true
            target_pid = 123

            "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{toml}").unwrap();

        let plan = Plan::load_from_toml_file(file.path()).unwrap();

        assert_eq!(plan.name, "test-config");
        assert_eq!(plan.injectors.network_config.target_iface.as_deref(), Some("enp5s0"));
        assert_eq!(plan.injectors.memory_config.target_pid, Some(123));
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

    #[test]
    fn test_output_config_basic() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_path = tempdir.path().join("cf-config.toml");
        
        output_config(&config_path, false, None, None).unwrap();
        
        assert!(config_path.exists());
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("name = \"example-plan\""));
    }

    #[test]
    fn test_output_config_with_pid_and_iface() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_path = tempdir.path().join("cf-config.toml");
        
        output_config(&config_path, false, Some(1234), Some("eth0")).unwrap();
        
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("target_iface = \"eth0\""));
        assert!(content.contains("target_pid = \"1234\""));
        assert!(content.contains("enabled = true"));
    }

    #[test]
    fn test_output_config_force_overwrite() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_path = tempdir.path().join("cf-config.toml");
        
        fs::write(&config_path, "original content").unwrap();
        
        let err = output_config(&config_path, false, None, None).unwrap_err().to_string();
        assert!(err.contains("already exists"));
        
        output_config(&config_path, true, None, None).unwrap();
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("name = \"example-plan\""));
    }
}
