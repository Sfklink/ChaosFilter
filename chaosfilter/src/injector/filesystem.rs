//! File-descriptor exhaustion injector.
//!
//! Reads every PID from the specified target cgroup, records each process's
//! current `RLIMIT_NOFILE` via `prlimit64(2)`, then lowers both the soft
//! and hard limits to the values supplied in [`FdConfig`].
//!
//! Calls [`FdExhaustConfig::aply`] to impose the limits and [`FdExhaustConfig::revert`]
//! to restore them to the original. The caller/user is responsible for all scheduling,
//! or in other words how long to hold chaos and when to revert.
//!
//! # Kernel background
//! Every open file in the kernel is represented by an integer file descriptor
//! (fd). The kernel enforces a per-process limit via `RLIMIT_NOFILE`.
//! When a process calss `open(2)` / `openat(2)` and the number of open fd(s)
//! already equals the soft limit, the syscall returns `EMFILE`. It is worth
//! nothing that the kernel *doesn't* automatically kill the process, but any
//! code that doesn't handle `EMFILE` will crash or malfunction.

use crate::plans::Plan;
use anyhow::{anyhow, Result, Context};
use libc::{self, rlimit64, RLIMIT_NOFILE};
use std::{fs, path::{Path, PathBuf}};
use tracing::{info, warn, debug};

/// Snapshot of each RLIMIT_NOFILE per process
struct SavedLimitConfig {
    pid: u32,
    soft: u64,
    hard: u64,
}

/// Tracker for the cgroup so what was applied can be undone with `revert`
#[derive(Default)]
pub struct FilesystemInjector {
    applied: bool,
    saved: Vec<SavedLimitConfig>
}

/*
TODO:
    Create validate.rs, and move validation functions there. 
 */
/// Validates the [`FdConfig`][crate::plans::FdConfig] section of a [`Plan`].
///
/// It is recommmedn that you run this before `apply` so you may get a clear 
/// error message instead of a mid-run failure.
/// # Errors
/// - `fd_config.enabled = true` but `targets.cgroup` is absent.
/// - The resolved cgroup directory doesn't exist.
/// - `soft_limit > hard_limit` (kernel would reject this anyway)
pub fn validate_fd_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.filesystem_config.enabled {
        return Ok(());
    }

    let cgroup_rel = plan
        .injectors
        .filesystem_config
        .target_pid
        .unwrap()
        .to_string();
    
    let cgroup = resolve_cgroup_path(&*cgroup_rel);

    if !cgroup.exists() {
        return Err(anyhow!(
            "target cgroup directory does not exist: {}",
            cgroup.display()
        ));
    }

    let config = &plan.injectors.filesystem_config;

    if config.soft_limit > config.hard_limit {
        return Err(anyhow!(
            "fd_config.soft_limit ({}) must be <= fd_config.hard_limit ({})",
            config.soft_limit, config.hard_limit
        ));
    }

    Ok(())
}

impl FilesystemInjector {
    /// Apply reduced `RLIMIT_NOFILE` to every PID in the target cgroup
    ///
    /// # Errors
    /// - Returns an error if the cgroup cannot be read or if `prlimit64` fails
    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        if !plan.injectors.filesystem_config.enabled {
            debug!("filesystem injector not enabled; skipping");
            return Ok(());
        }

        validate_fd_config(plan)?;


        // target cgroup required if enabled
        let cg_rel = plan
            .injectors
            .filesystem_config
            .target_pid
            .unwrap()
            .to_string();

        
        let cgroup = resolve_cgroup_path(&*cg_rel);
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

    /// Restores the original `RLIMIT_NOFILE` for every PID that was modified.
    pub fn revert(&mut self) -> Result<()> {
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

/// Called from `main::run_plan`. Validates, applies chaos, holds, reverts, the whole shaboo
/// shabang.
pub fn run_plan(plan: &Plan) -> Result<()> {
    if !plan.injectors.filesystem_config.enabled {
        return Ok(());
    }

    validate_fd_config(plan)?;

    let mut injector = FilesystemInjector::default();
    injector.apply(plan)?;
    injector.revert()?;

    Ok(())
}

// Helper methods
fn resolve_cgroup_path(arg: &str) -> PathBuf {
    let path = PathBuf::from(arg);
    if path.is_absolute() {
        path
    } else {
        Path::new("/sys/fs/cgroup").join(path)
    }
}

/// Returns all PIDs listen in <targets.cgroup>/**
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

/// Returns `(soft, hard)` RLIMIT_NOFILE for `pid` via `prlimit64(2)`.
/// If `pid = 0` is passed in, it will target the calling process instead
fn get_rlimit_nofile(pid: u32) -> Result<(u64, u64)> {
    let mut old = rlimit64 {
        rlim_cur: 0,
        rlim_max: 0,
    };

    // prlimit64(pid, RLIMIT_NOFILE, NULL, &old) → just read, do not set.
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

/// Sets `RLIMIT_NOFILE` for the `pid` to `(soft, hard)` via `prlimit64(2)`.
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
    fn resolve_cgroup_path_absolute() {
        let path = resolve_cgroup_path("/tmp/test");
        assert_eq!(path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn resolve_cgroup_path_relative() {
        let path = resolve_cgroup_path("test");
        assert_eq!(path, Path::new("/sys/fs/cgroup").join("test"));
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
    fn validate_fd_config_ok_disabled() {
        let plan = base_plan();
        validate_fd_config(&plan).unwrap();
    }

    #[test]
    fn validate_fd_config_error_enabled_no_cgroup() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;

        let err = validate_fd_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires targets.cgroup"));
    }

    #[test]
    fn validate_fd_config_error_cgroup_missing() {
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;
        plan.injectors.filesystem_config.target_pid = Some("/nonexistent/cgroup/path".to_string());

        let err = validate_fd_config(&plan).unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn validate_fd_config_error_soft_greater_than_hard() {
        let dir = TempDir::new().unwrap();
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 200;
        plan.injectors.filesystem_config.hard_limit = 100;
        plan.injectors.filesystem_config.target_pid = Some(dir.path().to_str().unwrap().to_string());

        let err = validate_fd_config(&plan).unwrap_err().to_string();
        assert!(err.contains("soft_limit") && err.contains("hard_limit"));
    }

    #[test]
    fn validate_fd_config_ok() {
        let dir = TempDir::new().unwrap();
        let mut plan = base_plan();

        plan.injectors.filesystem_config.enabled = true;
        plan.injectors.filesystem_config.soft_limit = 64;
        plan.injectors.filesystem_config.hard_limit = 64;
        plan.injectors.filesystem_config.target_pid = Some(dir.path().to_str());

        validate_fd_config(&plan).unwrap();
    }

    #[test]
    fn not_applied_when_disabled() {
        let mut injector = FilesystemInjector::default();
        injector.apply(&base_plan()).unwrap();
        assert!(!injector.applied);
    }

    #[test]
    fn no_reverted_when_not_applied() {
        let mut injector = FilesystemInjector::default();
        injector.revert().unwrap();
    }
}
