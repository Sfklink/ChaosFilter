//! Centralized validation section for a [`Plan`].
//! 
//! Every injector that can be ran has validation that can be used here.
//! These functions are ran for both `validate` and `chaos`
//! subcommands.
use crate::plans::Plan;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::debug;

// Master validator

/// Run every enabled injector's validator against a `plan`.
///
/// # Errors
/// Returns all validation errors that were encountered.
pub fn validate_plan(plan: &Plan) -> Result<()> {

    let errors: Vec<String> = [
        validate_iface_exists(plan.injectors.network_config.target_iface.as_deref()),
        validate_memory_config(plan),
        validate_filesystem_config(plan),
        validate_block_config(plan)
    ]
    .into_iter()
    .filter_map(|r| r.err())
    .map(|e| format!(" - {e}"))
    .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "{} validation error(s):\n{}",
            errors.len(),
            errors.join("\n")
        ))
    }    
}

// Per-injector validators

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
        debug!("network_config not enabled; skipping...");
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


/// Validates the `memory_config` section of a [`Plan`].
///  :3
/// Checks the following only runs when `memory_config.enabled = true`:
/// - `target_pid` is present.
/// - The PID exists in `/proc`.
/// - cgroup v2 is available.
/// - `targets.cgroup` is present and its *parent* directory exists.
///
/// # Errors
/// Returns a descriptive error for each condition above.
pub fn validate_memory_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.memory_config.enabled {
        debug!("memory_config not enabled; skipping...");
        return Ok(());
    }

    let pid = plan.injectors.memory_config.target_pid.ok_or_else(|| {
        anyhow!("memory_config.enabled=true requires injectors.memory_config.pid")
    })?;

    // quick pid existence check
    if !Path::new(&format!("/proc/{pid}")).exists() {
        return Err(anyhow!("PID does not exist: {pid}"));
    }

    // quick cgroup v2 check
    if !Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        return Err(anyhow!(
            "cgroup v2 not detected: /sys/fs/cgroup/cgroup.controllers missing"
        ));
    }

    // target cgroup required if enabled
    let cg_rel = plan
        .injectors
        .memory_config
        .target_pid
        .unwrap()
        .to_string();    

    let cg = resolve_cgroup_path(cg_rel.as_str());

    // directory may not exist yet; that's fine (apply creates it)
    // but parent must exist
    let parent = cg
        .parent()
        .ok_or_else(|| anyhow!("invalid cgroup path (no parent): {}", cg.display()))?;
    if !parent.exists() {
        return Err(anyhow!(
            "parent cgroup directory does not exist: {}",
            parent.display()
        ));
    }

    Ok(())
}

/// Validates the [`FilesystemConfig`][crate::plans::FilesystemConfig] section of a [`Plan`].
///
/// It is recommmedn that you run this before `apply` so you may get a clear 
/// error message instead of a mid-run failure.
/// # Errors
/// - `filesystem_config.enabled = true` but `targets.cgroup` is absent.
/// - The resolved cgroup directory doesn't exist.
/// - `soft_limit > hard_limit` (kernel would reject this anyway)
pub fn validate_filesystem_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.filesystem_config.enabled {
        return Ok(());
    }

    let cgroup_rel = plan
        .targets
        .cgroup
        .as_deref()
        .ok_or_else(|| anyhow!("filesystem_config.enabled=true requires targets.cgroup"))?;

    let cgroup = resolve_cgroup_path(cgroup_rel);

    if !cgroup.exists() {
        return Err(anyhow!(
            "target cgroup directory does not exist: {}",
            cgroup.display()
        ));
    }

    let config = &plan.injectors.filesystem_config;

    if config.soft_limit > config.hard_limit {
        return Err(anyhow!(
            "filesystem_config.soft_limit ({}) must be <= filesystem_config.hard_limit ({})",
            config.soft_limit, config.hard_limit
        ));
    }

    Ok(())
}

/// Validates the `block_config` section of a [`Plan`].
///
/// Checks (only when `block_config.enabled = true`):
/// - `block_config.device` is present.
/// - The device path exists on the filesystem.
/// - The cgroup v2 IO controller is enabled at the root.
///
/// # Errors
/// Returns a descriptive error for each condition above.
pub fn validate_block_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.block_config.enabled {
        debug!("block_config not enabled; skipping block validation");
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
 
    // Ensure the cgroup v2 IO controller is active at the root subtree.
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
            "cgroup v2 IO controller not enabled; run: \
            sudo sh -c 'echo +io > {}'",
            subtree_path.display()
        ));
    }
 
    Ok(())
}

// Helper methods

fn resolve_cgroup_path(arg: &str) -> PathBuf {
    let p = PathBuf::from(arg);
    if p.is_absolute() {
        p
    } else {
        Path::new("/sys/fs/cgroup").join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plans::{
        BlockConfig, FileSystemConfig, Injectors, MemoryConfig, NetworkConfig, Plan, Schedule,
        Targets,
    };
    use tempfile::TempDir;
 
    fn base_plan() -> Plan {
        Plan {
            name: "test".to_string(),
            targets: Targets {
                cgroup: None,
                iface: None,
            },
            schedule: Schedule { duration_s: 0 },
            injectors: Injectors {
                network_config: NetworkConfig::default(),
                memory_config: MemoryConfig::default(),
                block_config: BlockConfig::default(),
                filesystem_config: FileSystemConfig::default(),
            },
        }
    }
  
    #[test]
    fn validate_iface_exists_empty() {
        validate_iface_exists(None).unwrap();
    }
 
    #[test]
    fn validate_iface_exists_invalid() {
        let err = validate_iface_exists(Some("test"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("network interface not found"));
    }
 
    #[test]
    #[cfg(target_os = "linux")]
    fn iface_real_passes() {
        use crate::injector::network::get_default_iface;
        
        // there does exist the change of ip not being available, which can happen
        // in the case that we are not running as root
        let ip_exists = std::process::Command::new("sh")
            .args(["-c", "command -v ip >/dev/null 2>&1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !ip_exists {
            eprintln!("Skipping test: `ip` not installed");
            return;
        }
 
        if let Some(iface) = get_default_iface() {
            validate_iface_exists(Some(&iface)).unwrap();
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn validate_iface_exists_failure() {
        std::process::Command::new("sh")
            .args(["-c", "command -v ip >/dev/null 2>&1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let err = validate_iface_exists(Some("test")).unwrap_err().to_string();
        assert!(err.contains("network interface not found"))
    }
  
    #[test]
    fn validate_memory_config_ok_when_disabled() {
        validate_memory_config(&base_plan()).unwrap();
    }
 
    #[test]
    fn validate_memory_config_errors_when_enabled_missing_pid() {
        let mut plan = base_plan();

        plan.injectors.memory_config.enabled = true;
        plan.targets.cgroup = Some("testcg".to_string());
 
        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires injectors.memory_config.pid"));
    }
 
    #[test]
    fn validate_memory_config_errors_when_enabled_pid_missing_in_proc() {
        let mut plan = base_plan();

        plan.injectors.memory_config.enabled = true;
        plan.injectors.memory_config.target_pid = Some(4_000_000_000u32);
        plan.targets.cgroup = Some("testcg".to_string());
 
        let err = validate_memory_config(&plan).unwrap_err().to_string();
        assert!(err.contains("PID does not exist"));
    }
  
    #[test]
    fn validate_filesystem_config_ok_disabled() {
        validate_filesystem_config(&base_plan()).unwrap();
    }
 
    #[test]
    fn validate_filesystem_config_error_enabled_no_cgroup() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;
 
        let err = validate_filesystem_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires targets.cgroup"));
    }
 
    #[test]
    fn validate_filesystem_config_error_cgroup_missing() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;
        plan.targets.cgroup = Some("/nonexistent/cgroup/path".to_string());
 
        let err = validate_filesystem_config(&plan).unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }
 
    #[test]
    fn validate_filesystem_config_error_soft_greater_than_hard() {
        let dir = TempDir::new().unwrap();
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 200;
        plan.injectors.filesystem_config.hard_limit = 100;
        plan.targets.cgroup = Some(dir.path().to_str().unwrap().to_string());
 
        let err = validate_filesystem_config(&plan).unwrap_err().to_string();
        assert!(err.contains("soft_limit") && err.contains("hard_limit"));
    }
 
    #[test]
    fn validate_filesystem_config_ok() {
        let dir = TempDir::new().unwrap();
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 128;
        plan.targets.cgroup = Some(dir.path().to_str().unwrap().to_string());
 
        validate_filesystem_config(&plan).unwrap();
    }
  
    #[test]
    fn validate_block_disabled_passes() {
        validate_block_config(&base_plan()).unwrap();
    }
 
    #[test]
    fn validate_block_enabled_missing_device_fails() {
        let mut plan = base_plan();

        plan.injectors.block_config.enabled = true;
 
        let err = validate_block_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires block_config.device"));
    }
 
    #[test]
    fn validate_block_enabled_nonexistent_device_fails() {
        let mut plan = base_plan();

        plan.injectors.block_config.enabled = true;
        plan.injectors.block_config.device = Some("/dev/cf_bogus_device99".to_string());
 
        let err = validate_block_config(&plan).unwrap_err().to_string();
        assert!(err.contains("block device not found"));
    }

    #[test]
    fn resolve_absolute_path() {
        assert_eq!(
            resolve_cgroup_path("/tmp/test"),
            PathBuf::from("/tmp/test")
        );
    }
 
    #[test]
    fn resolve_relative_path() {
        assert_eq!(
            resolve_cgroup_path("mygroup"),
            PathBuf::from("/sys/fs/cgroup/mygroup")
        );
    }
}