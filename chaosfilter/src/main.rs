use anyhow::Context;

use chaosfilter::plans::{Plan, RunConfigArgs};
use chaosfilter::injector::cpu_memory::validate_memory_config;
use chaosfilter::injector::network::validate_iface_exists;
use chaosfilter::injector::{block_delay, cpu_memory, network, ChaosInjector};
use chaosfilter::injector::filesystem::{validate_fd_config, FilesystemInjector};

use clap::{Parser, Subcommand};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, path::{Path, PathBuf}};
use toml_edit::{value, DocumentMut};
use tracing::{debug, info};

const CONFIG_TEMPLATE: &str =
    include_str!("../assets/schema_config.toml");

fn main() {

    if let Err(e) = entry(std::env::args_os()) {
        eprintln!("{e:#}");
        std::process::exit(1);
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
///     - Outputs a .toml config file to eprintlnCWD.
///
/// # Side Effects
/// - Prints status messages to standard output.
/// - May modify system state via injector operations (e.g., `tc`, qdisc).
/// - May launch an interactive stdin/stdout loop.
///
/// # Errors
/// Returns an error if:
/// - Argument parsing fails.
/// - Plan construction fails (invalid config or missing inline flags).
/// - Validation fails.
/// - Chaos execution fails.
/// - Any downstream injector operation fails.
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

    // Subscriber is initialized here based on what level of verbose you want:
    //   (no flag)  → logging off entirely
    //   -v         → INFO
    //   -vv        → DEBUG
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
            validate_memory_config(&plan)?;
            validate_iface_exists(plan.targets.iface.as_deref())?;
            validate_fd_config(&plan)?;

            println!("\nConfig OK.");
            Ok(())
        }

        Commands::Chaos(args) => {
            let plan = Plan::load_from_toml_file(&args.config)?;
            run_plan(&plan)?;

            println!("\nChaos Plan Complete.");
            Ok(())
        }
        /*
        This guy right here.
        We're going to include an init command that points to a .toml file so we can spawn one for the user
        and reads it out.  can include pid and interface
         */
        Commands::Init {
            force,
            pid,
            iface,
            pid_pos,
            iface_pos,
        } => {
            // explicit over positional but should still work
            // i hope
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

pub fn run_plan(plan: &Plan) -> anyhow::Result<()> {
    let injectors: Vec<Box<dyn ChaosInjector>> = vec![
        Box::new(cpu_memory::MemoryInjector::default()),
        Box::new(network::NetworkInjector::default()),
        Box::new(block_delay::BlockDelayInjector::default()),
        Box::new(FilesystemInjector::default()),
    ];

    let mut active_injectors: Vec<Box<dyn ChaosInjector>> = Vec::new();

    for mut injector in injectors {
        let enabled = match injector.name() {
            "memory" => plan.injectors.memory_config.enabled,
            "network" => plan.injectors.network_config.enabled,
            "block_delay" => plan.injectors.block_config.enabled,
            "filesystem" => plan.injectors.filesystem_config.enabled,
            _ => false,
        };

        if enabled {
            injector
                .apply(plan)
                .with_context(|| format!("apply failed ({})", injector.name()))?;
            active_injectors.push(injector);
        }
    }

    let (ctrlc_tx, ctrlc_rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = ctrlc_tx.send(());
    })?;

    let sleep_deadline = Duration::from_secs(plan.schedule.duration_s);
    let _ = ctrlc_rx.recv_timeout(sleep_deadline);

    for mut injector in active_injectors {
        injector
            .revert()
            .with_context(|| format!("revert failed ({})", injector.name()))?;
    }

    Ok(())
}

pub fn output_config(
                    path: &Path,
                    force: bool,
                    pid: Option<u32>,
                    interface: Option<&str>,
                    ) -> anyhow::Result<PathBuf> {
    debug!(pid = ?pid, iface = ?interface, force, "init args");
    // check current directory path because relative sucks and is difficult, but I think this may
    // not be absolutely necessary, just dont run init as root.
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

// yay
    let mut doc: DocumentMut = CONFIG_TEMPLATE
        .parse::<DocumentMut>()
        .context("embedded config template is invalid TOML")?;

    // If interface provided: set target_iface + enable network injector
    // i dont like that targets exists
    // it annoys me, but do I  want to correct that?
    // yeah i do because who else will do it
    if let Some(iface) = interface {
        doc["injectors"]["network_config"]["enabled"] = value(true);
        doc["injectors"]["network_config"]["target_iface"] = value(iface);
        info!(iface, "network injector enabled");
        // set iface
        if doc["targets"]["iface"].is_none() {
            doc["targets"]["iface"] = value(iface);
        } else {
            doc["targets"]["iface"] = value(iface);
        }
    }else{
        doc["injectors"]["network_config"]["enabled"] = value(false);
        doc["injectors"]["network_config"]["target_iface"] = value("default");

    }
    // If pid provided: set target_pid + enable memory injector
    if let Some(p) = pid {
        doc["targets"]["cgroup"] = value(p.to_string());
        doc["injectors"]["memory_config"]["target_pid"] = value(p.to_string());
        doc["injectors"]["memory_config"]["enabled"] = value(true);
        info!(pid = p, "memory injector enabled");
    }else {
        doc["injectors"]["memory_config"]["target_pid"] = value(0);
        doc["targets"]["cgroup"] = value(0);

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
///
/// # Variants
/// - [`Commands::Validate`] → Validates a chaos plan.
/// - [`Commands::Chaos`] → Executes a chaos plan (apply → hold → revert).
/// - [`Commands::Init`] → Output a sample config file to CWD.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan from config. or inline flags)
    Validate(RunConfigArgs),

    /// Run the chaos plan (apply -> hold -> revert)
    Chaos(RunConfigArgs),

    /// Output a config file to local directory.  Include process_id and network_interface
    /// for auto-enable on network_config and memory_config
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
