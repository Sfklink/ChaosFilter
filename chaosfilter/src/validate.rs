//! Centralized validation for a [`Plan`].
//!
//! All injector validators live here. The entry-point to this is
//! [`validate_plan`], called by `main` for `validate`.

use crate::plans::Plan;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use tracing::debug;

/// Run every enabled injector validator against `plan`.
///
/// Cheap (no kernel mutations), fails fast on the first error.
/// Safe to call before any chaos is applied.
pub fn validate_plan(plan: &Plan) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
 
    let validators: &[(&str, Result<()>)] = &[
        ("network",     validate_network_config(plan)),
        ("memory",      validate_memory_config(plan)),
        ("filesystem",  validate_filesystem_config(plan)),
        ("block",       validate_block_config(plan)),
    ];
 
    for (name, result) in validators {
        if let Err(e) = result {
            errors.push(format!("  • {name}: {e}"));
        }
    }
 
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("plan validation failed:\n{}", errors.join("\n")))
    }
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
pub fn validate_iface_exists(iface: Option<&str>) -> Result<()> {
    let Some(iface) = iface else {
        // iface not specified => nothing to validate here
        return Ok(());
    };

    let status = Command::new("ip")
        .args(["link", "show", iface])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(anyhow!("network interface not found: {}", iface));
    }

    Ok(())
}

/// Validates the `network_config` section of a [`Plan`].
///
/// Only runs when `network_config.enabled = true`. Checks:
/// - `target_iface` is present.
/// - The interface exists on the host (via [`validate_iface_exists`]).
/// - `loss_percent` is in the range `0.0..=100.0`.
///
/// # Errors
/// Returns a descriptive error for each condition above.
pub fn validate_network_config(plan: &Plan) -> Result<()> {
    let config = &plan.injectors.network_config;
 
    if !config.enabled {
        debug!("network_config not enabled; skipping");
        return Ok(());
    }
 
    let iface = config
        .target_iface
        .as_deref()
        .ok_or_else(|| anyhow!("network_config.enabled=true requires network_config.target_iface"))?;
 
    validate_iface_exists(Some(iface))?;
 
    if !(0.0..=100.0).contains(&config.loss_percent) {
        return Err(anyhow!(
            "network_config.loss_percent ({}) must be between 0.0 and 100.0",
            config.loss_percent
        ));
    }
 
    Ok(())
}

/// Validates the [`FdConfig`][crate::plans::FdConfig] section of a [`Plan`].
///
/// It is recommmedn that you run this before `apply` so you may get a clear 
/// error message instead of a mid-run failure.
/// # Errors
/// - `fd_config.enabled = true` but `targets.cgroup` is absent.
/// - The resolved cgroup directory doesn't exist.
/// - `soft_limit > hard_limit` (kernel would reject this anyway)
pub fn validate_filesystem_config(plan: &Plan) -> Result<()> {
    let config = &plan.injectors.filesystem_config;

    if !config.enabled {
        return Ok(());
    }

    let pid = config.target_pid.ok_or_else(|| {
        anyhow!("filesystem_config.enabled=true requires injectors.filesystem_config.target_pid")
    })?;

    if !Path::new(&format!("/proc/{pid}")).exists() {
        return Err(anyhow!("PID does not exist: {pid}"));
    }

    if config.soft_limit > config.hard_limit {
        return Err(anyhow!(
            "filesystem_config.soft_limit ({}) must be <= filesystem_config.hard_limit ({})",
            config.soft_limit,
            config.hard_limit
        ));
    }

    Ok(())
}

/// Validates the `filesystem_config` (File Descriptor exhaustion) section of a [`Plan`].
///
/// Only runs when `filesystem_config.enabled = true`. Checks:
/// - `target_pid` is present.
/// - The PID exists in `/proc`.
/// - `soft_limit <= hard_limit`.
///
/// # Errors
/// Returns a descriptive error for each condition above.
pub fn validate_memory_config(plan: &Plan) -> Result<()> {
    let config = &plan.injectors.memory_config;

    if !config.enabled {
        return Ok(());
    }

    let pid = config.target_pid.ok_or_else(|| {
        anyhow!("memory_config.enabled=true requires injectors.memory_config.target_pid")
    })?;

    if !Path::new(&format!("/proc/{pid}")).exists() {
        return Err(anyhow!("PID does not exist: {pid}"));
    }

    if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        return Err(anyhow!(
            "cgroup v2 not detected: /sys/fs/cgroup/cgroup.controllers missing"
        ));
    }

    Ok(())
}

/// Validates the `block_config` section of a [`Plan`].
///
/// Only runs when `block_config.enabled = true`. Checks:
/// - `device` is present and exists on the filesystem.
/// - The cgroup v2 IO controller is enabled at the root subtree.
///
/// # Errors
/// Returns a descriptive error for each condition above.
pub fn validate_block_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.block_config.enabled {
        debug!("block_config not enabled; skipping");
        return Ok(());
    }
 
    let device = plan
        .injectors
        .block_config
        .device
        .as_deref()
        .ok_or_else(|| anyhow!("block_config.enabled=true requires block_config.device"))?;
 
    if !Path::new(device).exists() {
        return Err(anyhow!("block device not found: {}", device));
    }
 
    let subtree_path = Path::new("/sys/fs/cgroup/cgroup.subtree_control");
    let subtree = std::fs::read_to_string(subtree_path).map_err(|e| {
        anyhow!(
            "could not read {}: {} — is cgroup v2 mounted?",
            subtree_path.display(),
            e
        )
    })?;
 
    if !subtree.contains("io") {
        return Err(anyhow!(
            "cgroup v2 IO controller not enabled; run: sudo sh -c 'echo +io > {}'",
            subtree_path.display()
        ));
    }
 
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::plans::{
        BlockConfig, FileSystemConfig, Injectors, MemoryConfig as CliMemCfg,
        NetworkConfig as CliNetCfg, Plan, Schedule
    };

    fn base_plan() -> Plan {
        Plan {
            name: "test".to_string(),
            schedule: Schedule { duration_s: 0 },
            injectors: Injectors {
                network_config: CliNetCfg::default(),
                memory_config: CliMemCfg::default(),
                block_config: BlockConfig::default(),
                filesystem_config: FileSystemConfig::default(),
            },
        }
    }

    #[test]
    fn validate_filesystem_config_ok_disabled() {
        let plan = base_plan();
        validate_filesystem_config(&plan).unwrap();
    }

    #[test]
    fn validate_filesystem_config_error_enabled_no_pid() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.target_pid = None;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;

        let err = validate_filesystem_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires injectors.filesystem_config.target_pid")
            || err.contains("requires")
            || err.contains("target_pid"));
    }

    #[test]
    fn validate_filesystem_config_error_pid_missing() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.target_pid = Some(4_000_000_000u32);
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;

        let err = validate_filesystem_config(&plan).unwrap_err().to_string();
        assert!(err.contains("PID does not exist") || err.contains("does not exist"));
    }

    #[test]
    fn validate_filesystem_config_error_soft_greater_than_hard() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.target_pid = Some(std::process::id());
        plan.injectors.filesystem_config.soft_limit = 200;
        plan.injectors.filesystem_config.hard_limit = 100;

        let err = validate_filesystem_config(&plan).unwrap_err().to_string();
        assert!(err.contains("soft_limit") && err.contains("hard_limit"));
    }
    #[test]
    fn validate_filesystem_config_ok() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.target_pid = Some(std::process::id());
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;

        validate_filesystem_config(&plan).unwrap();
    }

    #[test]
    fn validate_memory_config_ok_when_disabled() {
        let plan = base_plan();
        validate_memory_config(&plan).unwrap();
    }

    #[test]
    fn validate_memory_config_errors_when_enabled_missing_pid() {
        let mut plan = base_plan();
        plan.injectors.memory_config.enabled = true;
        plan.injectors.memory_config.target_pid = None;

        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires injectors.memory_config.target_pid")
            || err.contains("requires injectors.memory_config.pid"));
    }

    #[test]
    fn validate_memory_config_errors_when_enabled_pid_missing_in_proc() {
        let mut plan = base_plan();
        plan.injectors.memory_config.enabled = true;
        plan.injectors.memory_config.target_pid = Some(4_000_000_000u32);

        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("PID does not exist") || err.contains("does not exist"));
    }
}