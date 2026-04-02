//! # Filesystem Resource Injector
//!
//! This module implements filesystem-related chaos, primarily focusing on
//! **file descriptor exhaustion**. It works by lowering the `RLIMIT_NOFILE`
//! (maximum number of open file descriptors) for all processes within a
//! target cgroup using the `prlimit64(2)` system call.
//!
//! # Kernel Background
//!
//! Every open file in the kernel is represented by an integer file descriptor.
//! The kernel enforces a per-process limit via `RLIMIT_NOFILE`. When a process
//! attempts to open a file and its current count equals the soft limit, the
//! syscall returns `EMFILE`. While the kernel does not kill the process,
//! many applications will crash or malfunction if they cannot open new files.

use crate::{injector::ChaosInjector, plans::Plan, validate::{resolve_cgroup_path, validate_filesystem_config}};
use anyhow::{anyhow, Result, Context};
use libc::{self, rlimit64, RLIMIT_NOFILE};
use std::{fs, path::{Path, PathBuf}};
use tracing::{info, warn, debug};

/// Snapshot of a process's original file descriptor limits.
struct SavedLimitConfig {
    /// The process ID.
    pid: u32,
    /// The original soft limit.
    soft: u64,
    /// The original hard limit.
    hard: u64,
}

/// The filesystem fault injector implementation.
#[derive(Default)]
pub struct FilesystemInjector {
    /// Indicates whether chaos has been applied.
    applied: bool,
    /// A collection of original limits for all modified processes.
    saved: Vec<SavedLimitConfig>
}

impl ChaosInjector for FilesystemInjector {
    fn name(&self) -> &'static str {
        "filesystem"
    }

    /// Applies reduced `RLIMIT_NOFILE` limits to every PID in the target cgroup.
    ///
    /// # Arguments
    ///
    /// * `plan` - The chaos plan containing filesystem configuration.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the limits were successfully applied to the cgroup's processes.
    ///
    /// # Behavior
    ///
    /// 1. Validates the configuration.
    /// 2. Resolves the target cgroup path.
    /// 3. Reads all PIDs currently in the cgroup.
    /// 4. For each PID, snapshots the current `RLIMIT_NOFILE` and applies the new limits.
    ///
    /// # Side Effects
    ///
    /// - Modifies the resource limits of external processes.
    ///
    /// # Errors
    ///
    /// Returns an error if the cgroup cannot be read or if internal validation fails.
    /// Individual `prlimit64` failures are logged as warnings.
    fn apply(&mut self, plan: &Plan) -> Result<()> {
        if !plan.injectors.filesystem_config.enabled {
            debug!("filesystem injector not enabled; skipping");
            return Ok(());
        }

        validate_filesystem_config(plan)?;

        let cg_rel = plan
            .injectors
            .filesystem_config
            .target_pid
            .unwrap()
            .to_string();


        let cgroup = resolve_cgroup_path(&cg_rel);
        let config = &plan.injectors.filesystem_config;

        let pids = read_cgroup_pids(&cgroup)
            .with_context(|| format!("failed to read cgroup pids at {}", cgroup.display()))?;

        if pids.is_empty() {
            warn!(cgroup = %cgroup.display(), "cgroup has no PIDs, nothing to strain");
        }

        info!(
            soft_limit = config.soft_limit,
            hard_limit = config.hard_limit,
            pid_count = pids.len(),
            "lowering RLIMIT_NOFILE"
        );

        for &pid in &pids {
            match get_rlimit_nofile(pid) {
                Ok((old_soft_limit, old_hard_limit)) => {
                    match set_rlimit_nofile(pid, config.soft_limit, config.hard_limit) {
                        Ok(()) => {
                            info!(
                                pid,
                                old_soft_limit,
                                old_hard_limit,
                                new_soft = config.soft_limit,
                                new_hard = config.hard_limit,
                                "applied fd limit"
                            );

                            self.saved.push(SavedLimitConfig {
                                pid,
                                soft: old_soft_limit,
                                hard: old_hard_limit
                            });
                        }
                        Err(e) => {
                            warn!(pid, error = %e, "prlimit64 set failed; skipping");
                        }
                    }
                }

                Err(e) => {
                    warn!(pid, error = %e, "prlimit64 get failed; skipping");
                }
            }
        }

        self.applied = true;
        Ok(())
    }

    /// Restores the original `RLIMIT_NOFILE` for every process that was modified.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Behavior
    ///
    /// Iterates through the snapshotted limits and reapplies them to each process.
    ///
    /// # Side Effects
    ///
    /// - Restores the resource limits of external processes.
    fn revert(&mut self) -> Result<()> {
        if !self.applied {
            return Ok(());
        }

        info!(pid_count = self.saved.len(), "restoring RLIMIT_NOFILE");

        for entry in &self.saved {
            match set_rlimit_nofile(entry.pid, entry.soft, entry.hard) {
                Ok(()) => {
                    info!(
                        pid = entry.pid,
                        soft = entry.soft,
                        hard = entry.hard,
                        "restored fd limit"
                    );
                }
                Err(e) => {
                    warn!(
                        pid = entry.pid,
                        error = %e,
                        "restore failed; process may have exited"
                    );
                }
            }
        }

        self.applied = false;
        self.saved.clear();
        Ok(())
    }
}

/// Reads all PIDs currently belonging to the specified cgroup.
fn read_cgroup_pids(cg: &Path) -> Result<Vec<u32>> {
    let procs_path = cg.join("cgroup.procs");
    let contents = fs::read_to_string(&procs_path)
        .with_context(|| format!("cannot read {}", procs_path.display()))?;

    let pids = contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            l.trim()
                .parse::<u32>()
                .with_context(|| format!("non-integer PID in cgroup.procs: '{}'", l))
        })
        .collect::<Result<Vec<u32>>>()?;

    Ok(pids)
}

/// Retrieves the current `RLIMIT_NOFILE` for a specific process.
///
/// # Arguments
///
/// * `pid` - The target process ID (0 for the calling process).
///
/// # Returns
///
/// Returns a tuple of `(soft_limit, hard_limit)` on success.
fn get_rlimit_nofile(pid: u32) -> Result<(u64, u64)> {
    let mut old = rlimit64 {
        rlim_cur: 0,
        rlim_max: 0,
    };

    let rc = unsafe {
        libc::prlimit64(
            pid as libc::pid_t,
            RLIMIT_NOFILE,
            std::ptr::null(),
            &mut old as *mut rlimit64,
        )
    };

    if rc != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(anyhow!("prlimit64(get) for PID {}: {}", pid, errno));
    }

    Ok((old.rlim_cur, old.rlim_max))
}

/// Sets the `RLIMIT_NOFILE` for a specific process.
///
/// # Arguments
///
/// * `pid` - The target process ID (0 for the calling process).
/// * `soft` - The new soft limit.
/// * `hard` - The new hard limit.
fn set_rlimit_nofile(pid: u32, soft: u64, hard: u64) -> Result<()> {
    let new_lim = rlimit64 {
        rlim_cur: soft,
        rlim_max: hard,
    };

    let rc = unsafe {
        libc::prlimit64(
            pid as libc::pid_t,
            RLIMIT_NOFILE,
            &new_lim as *const rlimit64,
            std::ptr::null_mut(),
        )
    };

    if rc != 0 {
        let errno = std::io::Error::last_os_error();
        return Err(anyhow!("prlimit64(set) for PID {}: {}", pid, errno));
    }

    Ok(())
}

// tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plans::{
        BlockConfig, FileSystemConfig, Injectors, MemoryConfig as CliMemCfg,
        NetworkConfig as CliNetCfg, Plan, Schedule
    };
    use std::fs;
    use tempfile::TempDir;

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
    fn read_cgroup_pids_parse() {
        let directory = TempDir::new().unwrap();
        let procs = directory.path().join("cgroup.procs");
        fs::write(&procs, "123\n456\n789\n").unwrap();

        let pids = read_cgroup_pids(directory.path()).unwrap();
        assert_eq!(pids, vec![123, 456, 789]);
    }

    #[test]
    fn read_cgroup_pids_empty() {
        let directory = TempDir::new().unwrap();
        fs::write(directory.path().join("cgroup.procs"), "").unwrap();

        let pids = read_cgroup_pids(directory.path()).unwrap();
        assert!(pids.is_empty());
    }

    #[test]
    fn read_cgroup_pids_missing() {
        let directory = TempDir::new().unwrap();

        let err = read_cgroup_pids(directory.path()).unwrap_err().to_string();
        assert!(err.contains("cannot read"));
    }

    #[test]
    fn get_rlimit_nofile_current_process() {
        let (soft, hard) = get_rlimit_nofile(0).unwrap();

        assert!(soft > 0,  "soft limit should be > 0");
        assert!(hard >= soft, "hard must be >= soft");
    }

    #[test]
    fn set_and_restore_rlimit_nofile() {
        let self_pid = 0u32;
        let (original_soft, original_hard) = get_rlimit_nofile(self_pid).unwrap();

        if original_hard < 512 {
            return;
        }

        set_rlimit_nofile(self_pid, 512, 512).unwrap();

        let (new_soft, new_hard) = get_rlimit_nofile(self_pid).unwrap();
        assert_eq!(new_soft, 512);
        assert_eq!(new_hard, 512);

        set_rlimit_nofile(self_pid, original_soft, original_hard).unwrap();
        let (restored_soft, restored_hard) = get_rlimit_nofile(self_pid).unwrap();
        assert_eq!(restored_soft, original_soft);
        assert_eq!(restored_hard, original_hard);
    }

    #[test]
    fn not_applied_when_disabled() {
        let mut injector = FilesystemInjector::default();
        injector.apply(base_plan()).unwrap();
        assert!(!injector.applied);
    }

    #[test]
    fn no_reverted_when_not_applied() {
        let mut injector = FilesystemInjector::default();
        injector.revert().unwrap();
    }
}
