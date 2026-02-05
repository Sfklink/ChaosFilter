use anyhow::{anyhow, Result};
use clap::{ArgGroup, Parser, Subcommand};
use chaosfilter_common::{Features, Injectors, Plan, QdiscNetem, Schedule, Targets};

#[derive(Parser, Debug)]
#[command(name = "chaosfilter-cli", version, about = "ChaosFilter control CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate a chaos plan (from config or inline flags)
    Validate(RunLikeArgs),

    /// Run the chaos plan (apply -> hold -> revert)
    Run(RunLikeArgs),
}

#[derive(Parser, Debug, Clone)]
#[command(
    group(
        ArgGroup::new("input")
            .required(true)
            .args(&["config", "iface"])
    )
)]
pub struct RunLikeArgs {
    /// Path to TOML config (mode A)
    #[arg(short, long)]
    pub config: Option<String>,

    // --- Inline mode (mode B) ---
    /// Network interface (required for inline mode)
    #[arg(long)]
    pub iface: Option<String>,

    /// Duration in ms (inline mode)
    #[arg(long, default_value_t = 5000)]
    pub duration_ms: u64,

    /// Optional cgroup relative to /sys/fs/cgroup (inline mode)
    #[arg(long)]
    pub cgroup: Option<String>,

    /// Enable qdisc netem injector (inline mode)
    #[arg(long, default_value_t = true)]
    pub netem_enabled: bool,

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

pub fn entry<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::Validate(args) => {
            let plan = plan_from_args(args)?;
            chaosfilter_control::validate_plan(&plan)?;
            println!("Config OK: {:?}", plan);
        }
        Commands::Run(args) => {
            let plan = plan_from_args(args)?;
            chaosfilter_control::run_plan(&plan)?;
            println!("Run complete.");
        }
    }

    Ok(())
}

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
                enabled: args.netem_enabled,
                delay_ms: args.netem_delay_ms,
                loss_percent: args.netem_loss_percent,
            },
        },
    })
}
