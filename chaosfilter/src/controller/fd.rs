//! File-descriptor exhaustion injector.
//!
//! Reads every PID from the specified target cgroup, records each process's
//! current `RLIMIT_NOFILE` via `prlimit64(2)`, then lowers both the soft
//! and hard limits to the values supplied in [`FdConfig`].
//!
//! After [`crate::plans::Schedule::duration_s`] seconds, the original limits
//! are restored.
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

/// Snapshot of each RLIMIT_NOFILE per process
struct SavedLimitConfig {
    pid: u32,
    soft: u64,
    hard: u64,
}

/// Tracker for the cgroup so what was applied can be undone with `revert`
#[derive(Default)]
pub struct FdExhaustConfig {
    applied: bool,
    saved: Vec<SavedLimitConfig>
}

/// Validates the [`FdConfig`][crate::plans::FdConfig] section of a [`Plan`].
///
/// It is recommmedn that you run this before `apply` so you may get a clear 
/// error message instead of a mid-run failure.
/// # Errors
/// - `fd_config.enabled = true` but `targets.cgroup` is absent.
/// - The resolved cgroup directory doesn't exist.
/// - `soft_limit > hard_limit` (kernel would reject this anyway)
pub fn validate_fd_config(plan: &Plan) -> Result<()> {
    if !plan.injectors.fd_config.enabled {
        return Ok(());
    }

    let cgroup_rel = plan
        .targets
        .cgroup
        .as_deref()
        .ok_or_else(|| anyhow!("fd_config.enabled=true requires targets.cgroup"))?;

    let cgroup = resolve_cgroup_path(cgroup_rel);

    if !cgroup.exists() {
        return Err(anyhow!(
            "target cgroup directory does not exist: {}",
            cgroup.display()
        ));
    }

    let config = &plan.injectors.fd_config;

    if config.soft_limit > config.hard_limit {
        return Err(anyhow!(
            "fd_config.soft_limit ({}) must be <= fd_config.hard_limit ({})",
            config.soft_limit, config.hard_limit
        ));
    }

    Ok(())
}

fn resolve_cgroup_path(arg: &str) -> PathBuf {
    let path = PathBuf::from(arg);
    if path.is_absolute() {
        return path;
    } else {
        return Path::new("/sys/fs/cgroup").join(path);
    }
}

impl FdExhaustConfig {
    /// Apply reduced `RLIMIT_NOFILE` to every PID in the target cgroup
    ///
    /// # Errors
    /// - Returns an error if the cgroup cannot be read or if `prlimit64` fails
    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        if !plan.injectors.fd_config.enabled {
            return Ok(());
        }

        validate_fd_config(plan)?;

        let cgroup = resolve_cgroup_path(plan.targets.cgroup.as_deref().unwrap());
        let config = &plan.injectors.fd_config;

        let pids = read_cgroup_pids(&cgroup)
            .with_context(|| format!("failed to read cgroup pids at {}", cgroup.display()))?;

        if pids.is_empty() {
            println!("cgroup {} has no PIDs, nothing to strain", cgroup.display());
        }

        println!("Lowering RLIMIT_NOFILE to soft_limit {} and hard_limit {} for {} PID(s)\n",
            config.soft_limit,
            config.hard_limit,
            pids.len()
        );

        for &pid in &pids {
            match get_rlimit_nofile(pid) {
                Ok((old_soft_limit, old_hard_limit)) => {
                    match set_rlimit_nofile(pid, config.soft_limit, config.hard_limit) {
                        Ok(()) => {
                            println!(
                                "PID: {}: Soft Limit: {} -> {}, Hard Limit: {} -> {}",
                                pid, old_soft_limit, config.soft_limit, old_hard_limit, config.hard_limit
                            );

                            self.saved.push(SavedLimitConfig {
                                pid,
                                soft: old_soft_limit,
                                hard: old_hard_limit
                            });
                        }
                        Err(e) => {
                            eprintln!("PID {}: prlimit64 set failed ({}); skipping", pid, e);
                        }
                    }
                }

                Err(e) => {
                    eprintln!("PID {}: prlimit64 get failed ({}); skipping", pid, e);
                }
            }
        }

        self.applied = true;

        let duration = plan.schedule.duration_s;
        println!("\nHolding reduced limits for {} seconds...", duration);
        std::thread::sleep(std::time::Duration::from_secs(duration));
        println!("File exhaustion complete! Reverting all limits...\n");

        Ok(())
    }

    /// Restores the original `RLIMIT_NOFILE` for every PID that was modified.
    pub fn revert(&mut self) -> Result<()> {
        if !self.applied {
            println!("Nothing limits were applied, skipping revert...");
            return Ok(());
        }

        println!("Restoring RLIMIT_NOFILE for {} PID(s)...", self.saved.len());

        for entry in &self.saved {
            match set_rlimit_nofile(entry.pid, entry.soft, entry.hard) {
                Ok(()) => {
                    println!(
                        "PID {}: restored soft limit {} hard limit {}",
                        entry.pid, entry.soft, entry.hard
                    );
                }
                Err(e) => {
                    eprintln!(
                        "PID {}: restore failed ({}). Process may have been exited early.",
                        entry.pid, e
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
    if !plan.injectors.fd_config.enabled {
        return Ok(());
    }

    validate_fd_config(plan)?;

    let mut injector = FdExhaustConfig::default();
    injector.apply(plan)?;
    injector.revert()?;

    Ok(())
}

// Helper methods
//
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
