//! Shared plan/config types.
//!
//! Defines [`Plan`] and related types used across the CLI and controller.
//! Plans are typically loaded from TOML via [`Plan::load_from_toml_file`].

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::{Stdio, Command}};

pub mod controller;
pub mod cli;
pub mod dispatcher;
/// Injector configuration block.
///
/// Each field represents configuration for a specific chaos mechanism.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    #[serde(default)]
    pub network_config: NetworkConfig,
    #[serde(default)]
    pub memory_config: MemoryConfig,
    //pub cgroup_memedit: CgroupMemEdit,
}


/// Configuration for the `controller/qdisc.rs` injector.
/// THIS IS MISSING QUITE A BIT, WHAT'S THE INTERFACE THAT WE'RE CONNECTING TO
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    /// Master enable flag for this injector.
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub iface: Option<String>,

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
    pub pid: Option<i32>,

    /// If true, move PID into the target cgroup before writing knobs.
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

/*
* Issue here with Having Debug, Clone, and Args no preceding an actual struct declaration.
I know there's a better way to do this.  Consider this for cleanup, later.
#[derive(Parser, Debug, Clone)]
#[command(
    group(
        ArgGroup::new("input")
            .required(true)
            .args(&["config", "iface"])
    )
)]
*/

#[derive(Debug, Clone, Args)]
pub struct RunConfigArgs {
    /// Path to TOML config
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

    /// Optional feature toggles.
    #[serde(default)]
    pub features: Features,

    /// Injector configuration (netem, etc.).
    #[serde(default)]
    pub injectors: Injectors,
}
/// Optional feature toggles that modify plan execution behavior.
/// Right now, unused. :'(
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Features {
    /// Whether to load the eBPF object during execution.
    #[serde(default)]
    pub load_ebpf: bool,
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

/// Validates a plan against the current host environment.
///
/// Performs lightweight pre-flight checks to catch obvious configuration errors
/// before any chaos is applied.
///
/// # Arguments
/// * `plan` - Chaos plan to validate.
///
/// # Returns
/// Returns `Ok(())` if the environment appears compatible with the plan.
///
/// # Side Effects
/// Executes read-only system checks (e.g. `ip link show`) and filesystem existence checks.
///
/// # Errors
/// Returns an error if:
/// - the referenced cgroup path does not exist, or
/// - the referenced network interface does not exist, or
/// - required system commands fail to execute.
pub fn validate_plan(plan: &Plan) -> Result<()> {
    println!("\nValidating chaos plan...\n");

    // Check cgroup exists
    // redefine as separate function from validate_plan
    // can confine to pid_cgroup.rs
    // this is domain specific, so why are we doing it here?
    // We can do this in pid_cgroup.rs and qdiscs.rs individually instead of having if statements
    // this is just going to bloat out of control.
    // DELETE ME AFTER MOVING.
    if let Some(cg) = &plan.targets.cgroup {
        let cgroup_path = format!("/sys/fs/cgroup/{}", cg);
        if !Path::new(&cgroup_path).exists() {
            bail!("cgroup does not exist: {}", cgroup_path);
        }
    }
    // same thing here, this is domain specific
    if let Some(iface) = &plan.targets.iface.as_deref() {
        validate_iface_exists(iface)?;
    }

    println!("\nConfig OK\n");

    Ok(())
}

/// Validates that a network interface exists on the host.
///
/// This function performs a lightweight check using
/// `ip link show <iface>` to verify that the interface
/// is present and accessible.
///
/// # Arguments
/// * `iface` - Name of the network interface to validate.
///
/// # Returns
/// Returns `Ok(())` if the interface exists.
///
/// # Side Effects
/// Executes the system command:
/// - `ip link show <iface>`
///
/// # Errors
/// Returns an error if:
/// - The `ip` command fails to execute, or
/// - The interface does not exist.
///
/// # Requires
/// The `ip` command must be available on the system.
///

pub fn validate_iface_exists(iface: &str) -> Result<()> {

    // Check network interface exists (if provided)
    // this needs to be moved to qdiscs.rs
    // This was always printing to standard output.  I hated it.
    let status = Command::new("ip")
        .args(["link", "show", iface])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;    if !status.success() {
        return Err(anyhow!("network interface not found: {}", iface));
    }

    Ok(())
}



//  // Oh look a good enumeration of a config file.
// #[derive(Debug, Clone, Args)]
// pub struct RunInlineArgs {
//     /// Enable or disable the netem subsystem.
//     #[arg(long, default_value_t = false)]
//     pub netem_enabled: bool,
//

//     /// Network interface (required for inline mode)
//     #[arg(long)]
//     pub iface: String,
//
//     /// Duration in seconds (inline mode)
//     #[arg(long, default_value_t = 5)]
//     pub duration_s: u64,
//
//     /// Optional cgroup relative to /sys/fs/cgroup (inline mode)
//     #[arg(long)]
//     pub cgroup: Option<String>,
//
//     /// Netem delay in ms (inline mode)
//     #[arg(long, default_value_t = 0)]
//     pub netem_delay_ms: u32,
//
//     /// Netem loss percent (inline mode)
//     #[arg(long, default_value_t = 0.0)]
//     pub netem_loss_percent: f32,
//
//     /// Enable eBPF load (inline mode)
//     #[arg(long, default_value_t = false)]
//     pub load_ebpf: bool,
//
//     // --- cgroup knobs injector flags (inline mode) ---
//     #[arg(long, default_value_t = false)]
//     pub cgroup_knobs_enabled: bool,
//
//     #[arg(long)]
//     pub cgroup_pid: Option<i32>,
//
//     #[arg(long, default_value_t = true)]
//     pub cgroup_move_pid: bool,
//
//     /// Repeatable: --cgroup-enable cpu --cgroup-enable memory
//     #[arg(long = "cgroup-enable")]
//     pub cgroup_enable: Vec<String>,
//
//     #[arg(long)]
//     pub cgroup_cpu_max: Option<String>,
//     #[arg(long)]
//     pub cgroup_cpu_weight: Option<u32>,
//     #[arg(long)]
//     pub cgroup_mem_max: Option<String>,
//     #[arg(long)]
//     pub cgroup_mem_high: Option<String>,
//     #[arg(long)]
//     pub cgroup_swap_max: Option<String>,
// }

