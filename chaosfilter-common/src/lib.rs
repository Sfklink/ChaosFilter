//! Shared plan/config types.
//!
//! Defines [`Plan`] and related types used across the CLI and controller.
//! Plans are typically loaded from TOML via [`Plan::load_from_toml_file`].

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

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
    
    /// Optional feature toggles.
    #[serde(default)]
    pub features: Features,
    
    /// Injector configuration (netem, etc.).
    #[serde(default)]
    pub injectors: Injectors,
}

/// Optional feature toggles that modify plan execution behavior.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Features {
    /// Whether to load the eBPF object during execution.
    #[serde(default)]
    pub load_ebpf: bool,
}

/// Injector configuration block.
///
/// Each field represents configuration for a specific chaos mechanism.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    /// Traffic control (tc) netem injector configuration.
    #[serde(default)]
    pub qdisc_netem: QdiscNetem,
}

/// Configuration for the `tc netem` injector.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QdiscNetem {
    /// Packet delay in milliseconds.
    #[serde(default)]
    pub delay_ms: u32,

    /// Packet loss percentage (`0.0`–`100.0`).
    #[serde(default)]
    pub loss_percent: f32,
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
    pub duration_ms: u64,
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
