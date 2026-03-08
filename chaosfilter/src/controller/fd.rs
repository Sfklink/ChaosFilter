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
use std::{fs, path::{Path, PathBuf}, io::{BufRead, BufReader}, process::{Child, Command, Stdio}, collections::HashMap};

/// Snapshot of each RLIMIT_NOFILE per process
struct SavedLimitConfig {
    pid: u32,
    soft: u64,
    hard: u64,
}

/// A denied syscall captured by strace
#[derive(Debug)]
pub struct DeniedCall {
    pub pid: u32,
    pub syscall: String,
    pub raw_line: String
}

/// Tracker for the cgroup so what was applied can be undone with `revert`
#[derive(Default)]
pub struct FdExhaustConfig {
    applied: bool,
    saved: Vec<SavedLimitConfig>,
    pub denials: Vec<DeniedCall>
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
        println!("\nHolding reduced limits for {} seconds...\n", duration);

        self.denials = collect_denials(&pids, duration);

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

    print_results(plan.clone(), &injector.denials);

    Ok(())
}

// Helper methods
fn print_results(plan: Plan, denials: &[DeniedCall]) {
    let duration_s = plan.schedule.duration_s;
    let cgroup = plan.targets.cgroup.as_deref().unwrap_or("unknown");
    
    println!("");
    println!("===== FD Exhaustion (Duration: {duration_s}s CGroup: {cgroup}) =====");

    if denials.is_empty() {
        println!("No EMFILE denials captured.");
    } else {
        println!("Total EMFILE denials: {}\n", denials.len());

        let mut by_syscall: HashMap<&str, usize> = HashMap::new();

        for d in denials {
            *by_syscall.entry(d.syscall.as_str()).or_insert(0) += 1;
        }

        let mut counts: Vec<_> = by_syscall.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1));

        println!("Denials by syscall:");
        for (syscall, count) in &counts {
            println!("\t{} {}", syscall, count);
        }

        println!("");
        println!("First 5 raw denials:");
        for d in denials.iter().take(5) {
            println!("\t{}", d.raw_line);
        }
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

/// Spawns `strace` attached to all `pids`, runs it for `duration_s` seconds,
/// kills it, and parses every line that contains `EMFILE`.
///
/// If `strace` is not installed or fails to attach, returns an empty vector
/// and a warning that the chaos plan did run, was completed, and reverts
/// normally, but you cannot see the results without `strace`.
fn collect_denials(pids: &[u32], duration_s: u64) -> Vec<DeniedCall> {
    let mut cmd = Command::new("strace");
    cmd.args([
        "-f",
        "-e", "trace=open,openat,socket,accept4,pipe,pipe2",
        "-e", "status=failed",
    ]);
    
    for &pid in pids {
        cmd.args(["-p", &pid.to_string()]);
    }

    // Because strace throws its output to stderr by default, throw out stdout
    // and pipe stderr into the program instead.
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());

    let child: Option<Child> = match cmd.spawn() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("Warning: Could not spawn strace({}); skipping denial collecting", e);
            eprintln!("Please verify that strace is installed (sudo apt/dnf/pacman install strace)\n");
            None
        }
    };

    std::thread::sleep(std::time::Duration::from_secs(duration_s));

    // If strace isn't available, do not parse and return empty vector
    let mut child = match child {
        Some(c) => c,
        None => return vec![]
    };

    let _ = child.kill();
    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Warning: strace wait failed: {}", e);
            return vec![];
        }
    };

    parse_emfile_results(&output.stderr)
}

/// Parses raw strace stderr output and returns one [`DeniedCall`] per EMFILE line.
///
/// strace output format with -f:
///   [pid 12345] openat(AT_FDCWD, "/dev/null", O_RDONLY) = -1 EMFILE (Too many open files)
/// without -f single pid):
///   openat(AT_FDCWD, "/dev/null", O_RDONLY) = -1 EMFILE (Too many open files)
fn parse_emfile_results(raw: &[u8]) -> Vec<DeniedCall> {
    BufReader::new(raw)
        .lines()
        .map_while(Result::ok)
        .filter(|line| line.contains("EMFILE"))
        .map(|line| {
            // pid from "[pid 12345]" prefix
            let pid = if line.starts_with("[pid ") {
                line.split_whitespace()
                    .nth(1)
                    .and_then(|s| s.trim_end_matches(']').parse::<u32>().ok())
                    .unwrap_or(0)
            } else {
                0
            };

            // syscall name = last word before '('
            let syscall = line
                .split('(')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .last()
                .unwrap_or("unknown")
                .to_string();

            DeniedCall {
                pid,
                syscall,
                raw_line: line,
            }
        })
        .collect()
}

// tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plans::{
        BlockConfig, FdConfig, Injectors, MemoryConfig as CliMemCfg,
        NetworkConfig as CliNetCfg, Plan, Schedule, Targets
    };
    use std::fs;
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
                network_config: CliNetCfg::default(),
                memory_config: CliMemCfg::default(),
                block_config: BlockConfig::default(),
                fd_config: FdConfig::default(),
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
            println!("Skipping: original hard limit ({}) < 512", original_hard);
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

        plan.injectors.fd_config.enabled = true;
        plan.injectors.fd_config.soft_limit = 64;
        plan.injectors.fd_config.hard_limit = 64;

        let err = validate_fd_config(&plan).unwrap_err().to_string();
        assert!(err.contains("requires targets.cgroup"));
    }

    #[test]
    fn validate_fd_config_error_cgroup_missing() {
        let mut plan = base_plan();

        plan.injectors.fd_config.enabled = true;
        plan.injectors.fd_config.soft_limit = 64;
        plan.injectors.fd_config.hard_limit = 64;
        plan.targets.cgroup = Some("/nonexistent/cgroup/path".to_string());

        let err = validate_fd_config(&plan).unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn validate_fd_config_error_soft_greater_than_hard() {
        let dir = TempDir::new().unwrap();
        let mut plan = base_plan();

        plan.injectors.fd_config.enabled = true;
        plan.injectors.fd_config.soft_limit = 200;
        plan.injectors.fd_config.hard_limit = 100;
        plan.targets.cgroup = Some(dir.path().to_str().unwrap().to_string());

        let err = validate_fd_config(&plan).unwrap_err().to_string();
        assert!(err.contains("soft_limit") && err.contains("hard_limit"));
    }

    #[test]
    fn validate_fd_config_ok() {
        let dir = TempDir::new().unwrap();
        let mut plan = base_plan();

        plan.injectors.fd_config.enabled = true;
        plan.injectors.fd_config.soft_limit = 64;
        plan.injectors.fd_config.hard_limit = 64;
        plan.targets.cgroup = Some(dir.path().to_str().unwrap().to_string());

        validate_fd_config(&plan).unwrap();
    }
}
