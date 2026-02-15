//! Shared plan/config types.
//!
//! Defines [`Plan`] and related types used across the CLI and controller.
//! Plans are typically loaded from TOML via [`Plan::load_from_toml_file`].

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand, Parser};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::{Stdio, Command}};


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

/// Injector configuration block.
///
/// Each field represents configuration for a specific chaos mechanism.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    /// Traffic control (tc) netem injector configuration.
    #[serde(default)]
    pub qdisc_netem: QdiscNetem,
    #[serde(default)]
    pub cgroup_knobs: CgroupKnobs
    //pub cgroup_memedit: CgroupMemEdit,
}

/// Configuration for the `tc netem` injector.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QdiscNetem {
    /// Mode select, True is on, False is off.
    #[serde(default)]
    pub enabled: bool,

    /// Packet delay in milliseconds.
    #[serde(default)]
    pub delay_ms: u32,

    /// Packet loss percentage (`0.0`–`100.0`).
    #[serde(default)]
    pub loss_percent: f32,

    /// Duration in milliseconds.
    #[serde(default)]
    pub duration: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CgroupKnobs {
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

/// This supports two different modes:
///     - **Config Mode:** Provide a config through `--config <path>`
///     - **Inline Mode:** Provide various arguments such as `--iface <name> --duration-ms <time in ms>`
///
/// The clap `ArgGroup` enforces that at least one mode (`--config` or `--iface`) is provided
///
/// Experimental parser wrapper (keeps Parser derive local to this crate without
/// accidentally applying it to RunLikeArgs).
#[derive(Parser, Debug, Clone)]
pub struct CommonParserShim {
    #[command(subcommand)]
    pub command: CommonCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CommonCommand {
    Validate(RunLikeArgs),
    Chaos(RunLikeArgs),
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
pub struct RunLikeArgs {
    #[command(subcommand)]
    pub mode: RunMode,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RunMode {
    /// Use a TOML plan file
    Config(RunConfigArgs),

    /// Use inline flags
    Inline(RunInlineArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RunConfigArgs {
    /// Path to TOML config
    #[arg(short, long)]
    pub config: String,
}

#[derive(Debug, Clone, Args)]
pub struct RunInlineArgs {
    /// Enable or disable the netem subsystem.
    #[arg(long, default_value_t = false)]
    pub netem_enabled: bool,

    /// Network interface (required for inline mode)
    #[arg(long)]
    pub iface: String,

    /// Duration in seconds (inline mode)
    #[arg(long, default_value_t = 5)]
    pub duration_s: u64,

    /// Optional cgroup relative to /sys/fs/cgroup (inline mode)
    #[arg(long)]
    pub cgroup: Option<String>,

    /// Netem delay in ms (inline mode)
    #[arg(long, default_value_t = 0)]
    pub netem_delay_ms: u32,

    /// Netem loss percent (inline mode)
    #[arg(long, default_value_t = 0.0)]
    pub netem_loss_percent: f32,

    /// Enable eBPF load (inline mode)
    #[arg(long, default_value_t = false)]
    pub load_ebpf: bool,

    // --- cgroup knobs injector flags (inline mode) ---
    #[arg(long, default_value_t = false)]
    pub cgroup_knobs_enabled: bool,

    #[arg(long)]
    pub cgroup_pid: Option<i32>,

    #[arg(long, default_value_t = true)]
    pub cgroup_move_pid: bool,

    /// Repeatable: --cgroup-enable cpu --cgroup-enable memory
    #[arg(long = "cgroup-enable")]
    pub cgroup_enable: Vec<String>,

    #[arg(long)]
    pub cgroup_cpu_max: Option<String>,
    #[arg(long)]
    pub cgroup_cpu_weight: Option<u32>,
    #[arg(long)]
    pub cgroup_mem_max: Option<String>,
    #[arg(long)]
    pub cgroup_mem_high: Option<String>,
    #[arg(long)]
    pub cgroup_swap_max: Option<String>,
}



/// Builds a [`Plan`] from either a TOML config file or inline flags.
///
/// This supports two modes:
/// - **Config mode:** `--config <path>`
/// - **Inline mode:** `--iface <name>` plus optional inline flags

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
impl RunLikeArgs {

    pub fn plan_from_args(self) -> Result<Plan> {
        match self.mode {
            RunMode::Config(a) => Plan::load_from_toml_file(a.config),
            RunMode::Inline(a) => a.plan_from_inline(),
        }
    }
}


impl RunInlineArgs {
    fn plan_from_inline(self) -> Result<Plan> {
        // inline-mode invariants (keep these here; controller shouldn’t care)
        if self.cgroup_knobs_enabled && self.cgroup_pid.is_none() {
            return Err(anyhow!("--cgroup-pid is required when --cgroup-knobs-enabled is set"));
        }

        Ok(Plan {
            name: "inline".to_string(),
            targets: Targets {
                cgroup: self.cgroup,
                iface: Some(self.iface),
            },
            schedule: Schedule {
                duration_s: self.duration_s,
            },
            features: Features {
                load_ebpf: self.load_ebpf,
            },
            injectors: Injectors {
                qdisc_netem: QdiscNetem {
                    enabled: self.cgroup_knobs_enabled,
                    delay_ms: self.netem_delay_ms,
                    loss_percent: self.netem_loss_percent,
                    duration: self.duration_s,
                },
                cgroup_knobs: CgroupKnobs {
                    enabled: self.cgroup_knobs_enabled,
                    pid: self.cgroup_pid,
                    move_pid: self.cgroup_move_pid,
                    enable: self.cgroup_enable,
                    cpu_max: self.cgroup_cpu_max,
                    cpu_weight: self.cgroup_cpu_weight,
                    mem_max: self.cgroup_mem_max,
                    mem_high: self.cgroup_mem_high,
                    swap_max: self.cgroup_swap_max,
                },
            },
        })
    }
}




/// Summary statistics parsed from `ping` output.
///
/// # Notes:
/// RTT values are in milliseconds. Packet loss is a percentage in the range `0.0..=100.0`.
#[derive(Debug)]
pub struct PingStats {
    pub transmitted: u32,
    pub received: u32,
    pub loss_pct: f32,
    pub rtt_min: f32,
    pub rtt_avg: f32,
    pub rtt_max: f32,
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
    if let Some(cg) = &plan.targets.cgroup {
        let cgroup_path = format!("/sys/fs/cgroup/{}", cg);
        if !Path::new(&cgroup_path).exists() {
            bail!("cgroup does not exist: {}", cgroup_path);
        }
    }

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

/// Executes a ping test and parses packet statistics.
///
/// This function runs a timed ping using the interface defined
/// in the provided [`Plan`] and extracts transmission, loss,
/// and RTT metrics.
///
/// # Arguments
/// * `plan` - Chaos plan containing target interface and duration.
/// * `target` - Destination host or IP address to ping.
///
/// # Returns
/// Returns `Some(PingStats)` if:
/// - The ping command executes successfully, and
/// - Output can be parsed correctly.
///
/// Returns `None` if:
/// - The interface is not set in the plan,
/// - The command fails,
/// - Or parsing fails.
///
/// # Side Effects
/// Executes:
/// - `ping -I <iface> -w <duration> <target>`
///
/// # Notes
/// RTT values are reported in milliseconds.
/// Packet loss is a percentage in the range `0.0..=100.0`.
pub fn run_ping_test(plan: &Plan, target: &str) -> Option<PingStats> {

    let iface = plan.targets.iface
        .as_deref()?;

    let output = Command::new("ping")
        .args([
            "-I", iface,
            "-w", &plan.schedule.duration_s.to_string(),
            target
        ])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut transmitted = 0;
    let mut received = 0;
    let mut loss_pct = 0.0;
    let mut rtt_min = 0.0;
    let mut rtt_avg = 0.0;
    let mut rtt_max = 0.0;

    for line in stdout.lines() {
        if line.contains("packets transmitted") {
            let parts: Vec<&str> = line.split(',').collect();
            transmitted = parts.get(0)?.trim().split(' ').next()?.parse().ok()?;
            received = parts.get(1)?.trim().split(' ').next()?.parse().ok()?;
            loss_pct = parts.get(2)?.trim().split('%').next()?.parse().ok()?;
        }

        if line.contains("rtt min/avg/max") {
            let stats = line.split('=').nth(1)?.trim();
            let nums: Vec<&str> = stats.split('/').collect();
            rtt_min = nums.get(0)?.parse().ok()?;
            rtt_avg = nums.get(1)?.parse().ok()?;
            rtt_max = nums.get(2)?.parse().ok()?;
        }
    }

    Some(PingStats {
        transmitted,
        received,
        loss_pct,
        rtt_min,
        rtt_avg,
        rtt_max,
    })
}

/// Formats and prints a baseline vs. during-chaos comparison report.
///
/// This function produces a human-readable comparison of ping
/// statistics collected before and during chaos execution.
///
/// # Arguments
/// * `iface` - Network interface under test.
/// * `duration` - Duration of the chaos run (in seconds).
/// * `control` - Baseline [`PingStats`] collected before chaos.
/// * `modified` - [`PingStats`] collected during chaos.
///
/// # Returns
/// Returns the formatted report string.
///
/// # Side Effects
/// - Prints the formatted report to standard output.
///
/// # Notes
/// This function does not perform validation. It assumes both
/// `control` and `modified` statistics are valid and comparable.
pub fn print_comparison(iface: &str, duration: u64, control: &PingStats, modified: &PingStats) -> String {
    let output = format!(
        "\n\n=== Network Comparison (Duration: {duration} seconds) ===

BEFORE CHAOS (baseline of {iface}):
  transmitted : {ct_tx}
  received    : {ct_rx}
  loss %      : {ct_loss}
  rtt (ms)    : min {ct_min} | avg {ct_avg} | max {ct_max}

DURING CHAOS ({iface}):
  transmitted : {md_tx}
  received    : {md_rx}
  loss %      : {md_loss}
  rtt (ms)    : min {md_min} | avg {md_avg} | max {md_max}
",
        ct_tx = control.transmitted,
        ct_rx = control.received,
        ct_loss = control.loss_pct,
        ct_min = control.rtt_min,
        ct_avg = control.rtt_avg,
        ct_max = control.rtt_max,
        md_tx = modified.transmitted,
        md_rx = modified.received,
        md_loss = modified.loss_pct,
        md_min = modified.rtt_min,
        md_avg = modified.rtt_avg,
        md_max = modified.rtt_max,
    );

    println!("{}", output);
    output
}