//! # Chaos Plan Schema
//!
//! This module defines the data structures used to represent a chaos experiment.
//! These structures are designed to be deserialized from TOML configuration files,
//! providing a declarative way to specify fault injection parameters.

use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level chaos plan configuration.
///
/// A `Plan` fully describes *what* chaos to run, *where* to run it,
/// and *for how long*. It is consumed by the injector layer and
/// should be treated as immutable once execution begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Human-readable plan name (for logs/UI).
    pub name: String,

    /// Timing parameters for the experiment.
    pub schedule: Schedule,

    /// Configuration for individual fault injectors.
    #[serde(default)]
    pub injectors: Injectors,
}

impl Plan {
    /// Loads a [`Plan`] from a TOML configuration file.
    ///
    /// This method reads a TOML file from disk, parses it into a [`Plan`] struct,
    /// and performs post-processing such as resolving the "default" network interface.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to the TOML configuration file.
    ///
    /// # Returns
    ///
    /// Returns a fully-deserialized and processed [`Plan`] on success.
    ///
    /// # Side Effects
    ///
    /// - Reads from the filesystem.
    /// - Executes `ip route get` to resolve the default network interface if specified.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read.
    /// - The TOML syntax is invalid.
    /// - The "default" interface cannot be determined when requested.
    pub fn load_from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let s = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        let mut plan: Plan = toml::from_str(&s)
            .with_context(|| format!("failed to parse TOML in: {}", path.display()))?;

        /// Internal helper to determine the default network interface.
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

        if let Some(iface) = plan.injectors.network_config.target_iface.as_deref() {
            if iface == "default" {
                plan.injectors.network_config.target_iface = Some(
                    get_default_iface()
                        .ok_or_else(|| anyhow!("could not determine default interface via `ip route get`"))?
                );
            }
        }

        Ok(plan)
    }
}

/// Collection of all available injectors.
///
/// This struct aggregates the configuration for different types of fault injection.
/// Each field corresponds to a specific [`crate::injector::ChaosInjector`] implementation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    /// Network-level fault injection (loss, delay).
    #[serde(default)]
    pub network_config: NetworkConfig,

    /// CPU and Memory resource constraints.
    #[serde(default)]
    pub memory_config: MemoryConfig,

    /// Filesystem resource limits (e.g., file descriptors).
    #[serde(default)]
    pub filesystem_config: FileSystemConfig,
}

/// Configuration for network fault injection.
///
/// Utilizes `tc` and `netem` to manipulate outgoing traffic.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    /// Master enable flag for this injector.
    #[serde(default)]
    pub enabled: bool,

    /// The network interface to target (e.g., "eth0"). Use "default" to auto-detect.
    #[serde(default)]
    pub target_iface: Option<String>,

    /// Packet delay in milliseconds.
    #[serde(default)]
    pub delay_ms: u32,

    /// Packet loss percentage (0.0–100.0).
    #[serde(default)]
    pub loss_percent: f32,

    /// Optional cgroup IDs for eBPF-based network filtering.
    #[serde(default)]
    pub network_ebpf_cgroup: Vec<u64>,
}

/// Configuration for CPU and Memory constraints.
///
/// Leverages Linux cgroups v2 to enforce resource limits on specific processes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    /// Master enable flag for this injector.
    #[serde(default)]
    pub enabled: bool,

    /// The PID of the process to constrain.
    pub target_pid: Option<u32>,

    /// If true, moves the process into a dedicated cgroup before applying limits.
    #[serde(default)]
    pub move_pid: bool,

    /// Cgroup v2 controllers to enable (e.g., ["cpu", "memory"]).
    #[serde(default)]
    pub enable: Vec<String>,

    /// CPU limit in cgroup v2 format (e.g., "max 100000" or "50000 100000").
    pub cpu_max: Option<String>,

    /// CPU weight for proportional sharing.
    pub cpu_weight: Option<u32>,

    /// Maximum memory limit (e.g., "1G").
    pub mem_max: Option<String>,

    /// High memory water mark (soft limit).
    pub mem_high: Option<String>,

    /// Maximum swap usage.
    pub swap_max: Option<String>,
}

/// Configuration for filesystem-related limits.
///
/// Primarily focuses on file descriptor exhaustion by manipulating `RLIMIT_NOFILE`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSystemConfig {
    /// Master enable flag for this injector.
    #[serde(default)]
    pub enabled: bool,

    /// The PID of the target process.
    pub target_pid: Option<u32>,

    /// New soft limit for open file descriptors.
    #[serde(default)]
    pub soft_limit: u64,

    /// New hard limit for open file descriptors.
    #[serde(default)]
    pub hard_limit: u64,
}

/// Execution timing parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Duration to maintain the chaos state in seconds.
    pub duration_s: u64,
}

/// Shared subcommands between CLI entry points.
#[derive(Subcommand, Debug, Clone)]
pub enum CommonCommand {
    /// Validate a chaos plan.
    Validate(RunConfigArgs),

    /// Execute a chaos plan.
    Chaos(RunConfigArgs),

    /// Initialize a new configuration file.
    Init,
}

/// Arguments for commands that require a configuration file.
#[derive(Debug, Clone, Args)]
pub struct RunConfigArgs {
    /// Path to the TOML configuration file.
    #[arg(short, long)]
    pub config: String,
}
