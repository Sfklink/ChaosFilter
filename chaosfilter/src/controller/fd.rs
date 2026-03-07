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
use anyhow::{anyhow, Result};
use std::{path::{Path, PathBuf}};

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

pub fn run_plan(plan: &Plan) -> Result<()> {
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
