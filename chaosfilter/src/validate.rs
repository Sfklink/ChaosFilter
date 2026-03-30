//! # Plan Validation
//!
//! This module provides logic to ensure that a [`Plan`] is valid and can be safely
//! executed on the current system. It checks for the existence of required resources
//! (like network interfaces and PIDs) and ensures that configuration values are
//! within acceptable ranges.

use crate::plans::Plan;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use tracing::debug;

/// Run all enabled injector validators against the provided [`Plan`].
///
/// This is the primary entry point for plan validation. It iterates through all
/// configured injectors and calls their respective validation functions if they are enabled.
///
/// # Arguments
///
/// * `plan` - The chaos [`Plan`] to validate.
///
/// # Returns
///
/// Returns `Ok(())` if all enabled injectors pass validation.
///
/// # Errors
///
/// Returns a consolidated error message if any validation checks fail.
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

/// Validates the [`crate::plans::NetworkConfig`] section of a [`Plan`].
///
/// # Arguments
///
/// * `plan` - The plan containing the network configuration.
///
/// # Returns
///
/// Returns `Ok(())` if the configuration is valid or disabled.
///
/// # Behavior
///
/// If enabled, checks:
/// 1. `target_iface` is present.
/// 2. The interface exists on the host (via `ip link show`).
/// 3. `loss_percent` is within the range [0.0, 100.0].
///
/// # Errors
///
/// Returns an error if any of the above checks fail.
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

/// Validates the [`crate::plans::FileSystemConfig`] section of a [`Plan`].
///
/// # Arguments
///
/// * `plan` - The plan containing the filesystem configuration.
///
/// # Returns
///
/// Returns `Ok(())` if the configuration is valid or disabled.
///
/// # Behavior
///
/// If enabled, checks:
/// 1. `target_pid` is present.
/// 2. The target PID exists in `/proc`.
/// 3. `soft_limit` is less than or equal to `hard_limit`.
///
/// # Errors
///
/// Returns an error if any of the above checks fail.
pub fn validate_filesystem_config(plan: &Plan) -> Result<()> {
    let config = &plan.injectors.filesystem_config;

    if !config.enabled {
        debug!("filesystem_config not enabled; skipping");
        return Ok(());
    }

    let pid = config.target_pid.ok_or_else(|| {
        anyhow!("filesystem_config.enabled=true requires injectors.filesystem_config.target_pid")
    })?;

    validate_pid_exists(pid)?;

    if config.soft_limit > config.hard_limit {
        return Err(anyhow!(
            "filesystem_config.soft_limit ({}) must be <= filesystem_config.hard_limit ({})",
            config.soft_limit,
            config.hard_limit
        ));
    }

    Ok(())
}

/// Validates the [`crate::plans::MemoryConfig`] section of a [`Plan`].
///
/// # Arguments
///
/// * `plan` - The plan containing the memory configuration.
///
/// # Returns
///
/// Returns `Ok(())` if the configuration is valid or disabled.
///
/// # Behavior
///
/// If enabled, checks:
/// 1. `target_pid` is present.
/// 2. The target PID exists in `/proc`.
/// 3. Cgroup v2 is supported by the kernel and mounted at `/sys/fs/cgroup`.
///
/// # Errors
///
/// Returns an error if any of the above checks fail.
pub fn validate_memory_config(plan: &Plan) -> Result<()> {
    let config = &plan.injectors.memory_config;

    if !config.enabled {
        debug!("memory_config not enabled; skipping");
        return Ok(());
    }

    let pid = config.target_pid.ok_or_else(|| {
        anyhow!("memory_config.enabled=true requires injectors.memory_config.target_pid")
    })?;

    validate_pid_exists(pid)?;
    validate_cgroup_v2()?;

    Ok(())
}

/// Validates the [`crate::plans::BlockConfig`] section of a [`Plan`].
///
/// # Arguments
///
/// * `plan` - The plan containing the block I/O configuration.
///
/// # Returns
///
/// Returns `Ok(())` if the configuration is valid or disabled.
///
/// # Behavior
///
/// If enabled, checks:
/// 1. `device` is present and the path exists.
/// 2. The `io` controller is enabled in the root cgroup's `subtree_control`.
///
/// # Errors
///
/// Returns an error if any of the above checks fail.
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

/// Helper to check if a network interface exists.
fn validate_iface_exists(iface: Option<&str>) -> Result<()> {
    let Some(iface) = iface else {
        debug!("iface not provided; skipping");
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

/// Helper to check if a process ID exists in `/proc`.
fn validate_pid_exists(pid: u32) -> Result<()> {
    if !Path::new(&format!("/proc/{pid}")).exists() {
        return Err(anyhow!("PID does not exist: {pid}"));
    }
    Ok(())
}

/// Helper to check if cgroup v2 is enabled and mounted.
fn validate_cgroup_v2() -> Result<()> {
    if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        return Err(anyhow!(
            "cgroup v2 not detected: /sys/fs/cgroup/cgroup.controllers missing"
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