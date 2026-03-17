use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
/*
Most likely going to add scheduler in here so we can fire sequentially.  Just has to deal with
sequencing and variable intake.
 */

/// Top-level chaos plan configuration.
///
/// A `Plan` fully describes *what* chaos to run, *where* to run it,
/// and *for how long*. It is consumed by the injector layer and
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

        let mut plan: Plan = toml::from_str(&s)
            .with_context(|| format!("failed to parse TOML in: {}", path.display()))?;

        fn get_default_iface() -> Option<String> {
            let output = std::process::Command::new("ip")
                .args(["route", "get", "8.8.8.8"])
                .output()
                .ok()?;

            if !output.status.success() {
                return None;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);

            stdout
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .find(|w| w[0] == "dev")
                .map(|w| w[1].to_string())
        }

        if let Some(iface) = plan.targets.iface.as_deref() {
            if iface == "default" {
                plan.targets.iface = Some(
                    get_default_iface()
                        .ok_or_else(|| anyhow!("could not determine default interface via `ip route get`"))?
                );
            }
        }

        Ok(plan)
    }

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
    #[serde(default)]
    pub block_config: BlockConfig,
    #[serde(default)]
    pub filesystem_config: FileSystemConfig,
    #[serde(default)]
    pub ebpf_config: EbpfConfig,
}

/// Configuration for the `injector/qdisc.rs` injector.
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

    pub network_ebpf_cgroup: Vec<u64>,
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

/// Configuration for block io stuff
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlockConfig {
    /// the master enable flag cause it seemed important
    #[serde(default)]
    pub enabled: bool,

    /// what it is delaying
    #[serde(default)]
    pub device: Option<String>,

    /// specific values that are being affected
    pub rbps: Option<u64>,
    pub wbps: Option<u64>,
    pub riops: Option<u64>,
    pub wiops: Option<u64>,
}

/// Configuration for File Descriptor Exhaustion
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSystemConfig {
    /// master enable flag
    #[serde(default)]
    pub enabled: bool,

    /// New soft limit for RLIMIT_NOFILE applied to each PID in the cgroup. (e.g., 32, 64)
    /// Must be < hard_limit or it will cause issues.
    #[serde(default)]
    pub soft_limit: u64,

    /// New hard limit for RLIMIT_NOFILE applied to each PID in the cgroup. (e.g., 128, 256)
    /// Must be > soft_limit or it will cause issues.
    #[serde(default)]
    pub hard_limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EbpfConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub tc_probe: bool,

    #[serde(default)]
    pub target_iface: Option<String>,
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
    Init,
}

#[derive(Debug, Clone, Args)]
pub struct RunConfigArgs {
    /// Path to TOML config
    /// Used in chaosfilter --config <filename_here>.toml
    #[arg(short, long)]
    pub config: String,
}
