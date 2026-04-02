//! # Network Fault Injector
//!
//! This module implements network-level fault injection using Linux Traffic Control (`tc`)
//! and the `netem` (Network Emulator) queuing discipline (qdisc). It can simulate
//! packet loss, delay, and jitter on specific network interfaces.
//!
//! Additionally, it supports targeted injection by using **eBPF** classifiers to mark
//! packets originating from specific cgroups, allowing chaos to be isolated to
//! particular processes rather than affecting the entire interface.

use crate::{injector::ChaosInjector, plans::Plan};
use anyhow::{Context, Result, anyhow};
use std::process::{Command, Stdio};
use tracing::{debug, error, info, warn};
use aya::{
    maps::HashMap,
    programs::{SchedClassifier, TcAttachType},
    Ebpf,
};
use aya_log::EbpfLogger;

/// A handle to the loaded eBPF program and its resources.
///
/// This struct ensures that the eBPF program remains loaded for the duration
/// of the chaos experiment.
pub struct EbpfHandle {
    /// The underlying Aya eBPF context.
    pub(crate) _ebpf: Ebpf,
}

/// The network fault injector implementation.
///
/// This struct maintains the state of a network chaos experiment, including
/// the target interface and any loaded eBPF programs.
#[derive(Default)]
pub struct NetworkInjector {
    /// Indicates whether chaos has been applied.
    applied: bool,
    /// The network interface being targeted.
    iface: Option<String>,
    /// Duration of the experiment (not currently used by the injector itself).
    pub duration_s: u64,
    /// The amount of delay injected in milliseconds.
    pub netem_delay_ms: i32,
    /// The percentage of packet loss injected.
    pub netem_loss_percent: f32,
    /// Handle to the optional eBPF classifier.
    pub ebpf_handle: Option<EbpfHandle>,
}

impl NetworkInjector {
    /// Marks the injector as successfully applied.
    fn mark_apply_success(&mut self, iface: &str) {
        self.applied = true;
        self.iface = Some(iface.to_string());
    }
}

/// Traffic steering strategies for network degradation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterMode {
    /// Route all traffic into the `netem` band using a match-all filter.
    MatchAll,
    /// Route only traffic marked by eBPF into the `netem` band.
    EbpfMarked,
}

impl ChaosInjector for NetworkInjector {
    fn name(&self) -> &'static str {
        "network"
    }

    /// Applies the configured network chaos plan to the target interface.
    ///
    /// This method orchestrates:
    /// 1. Root `prio` qdisc creation.
    /// 2. Child `netem` qdisc attachment at band 1:2.
    /// 3. Filter installation (match-all or eBPF-based).
    ///
    /// # Arguments
    ///
    /// * `plan` - The validated chaos plan.
    /// * `iface` - The network interface to target.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the chaos configuration is applied successfully.
    ///
    /// # Side Effects
    ///
    /// - Modifies the `tc` configuration of the specified interface.
    /// - Loads and attaches an eBPF program if cgroup targeting is used.
    ///
    /// # Errors
    ///
    /// Returns an error if any `tc` command fails or if the eBPF program cannot be loaded.
    fn apply(&mut self, plan: Plan) -> Result<()> {
        let iface_owned = plan.injectors.network_config.target_iface
            .ok_or_else(|| anyhow!("network target interface not specified"))?;
        let iface = iface_owned.as_str();
        let delay_ms = plan.injectors.network_config.delay_ms;
        let loss_percent = plan.injectors.network_config.loss_percent;
        let network_cgroup_target = &plan.injectors.network_config.network_ebpf_cgroup;

        info!(
            %iface,
            delay_ms,
            loss_percent,
            "applying netem qdisc"
            );
        ensure_root_prio_qdisc(iface)?;
        ensure_child_netem_qdisc(iface, delay_ms, loss_percent)?;
        show_qdisc_state(iface);

        match select_filter_mode(network_cgroup_target) {
            FilterMode::MatchAll => {
                install_match_all_filter(iface)?;
            }
            FilterMode::EbpfMarked => {
                install_fw_filter(iface)?;
                let handle = attach_classifier(iface, network_cgroup_target)?;
                self.ebpf_handle = Some(handle);
            }
        }

        self.duration_s = plan.schedule.duration_s;
        self.netem_delay_ms = delay_ms as i32;
        self.netem_loss_percent = loss_percent;
        self.mark_apply_success(iface);

        debug!("apply() complete");
        Ok(())
    }

    /// Reverts the network chaos by restoring the baseline qdisc.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Behavior
    ///
    /// - If not applied, it does nothing.
    /// - Otherwise, calls [`create_restore_root`].
    ///
    /// # Errors
    ///
    /// Returns an error if any internal state is missing.
    fn revert(&mut self) -> Result<()> {
        if !self.applied {
            debug!("nothing applied; skipping revert");
            return Ok(());
        }

        let iface = self.iface.as_deref().ok_or_else(|| anyhow!("internal error: iface missing during revert"))?;

        create_restore_root(iface);
        show_qdisc_state(iface);

        self.ebpf_handle = None;
        self.applied = false;
        self.iface = None;
        self.duration_s = 0;
        self.netem_delay_ms = 0;
        self.netem_loss_percent = 0.0;

        Ok(())
    }
}

/// Prints the current qdisc state for the specified interface to the logs.
///
/// # Arguments
///
/// * `iface` - The network interface to inspect.
pub fn show_qdisc_state(iface: &str) {
    match Command::new("tc")
        .args(["qdisc", "show", "dev", iface])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => warn!(iface, exit_code = %status, "tc qdisc show exited non-zero"),
        Err(e) => warn!(iface, error = %e, "failed to run tc qdisc show"),
    }
}

/// Ensures that a root `prio` qdisc exists on the target interface.
///
/// # Arguments
///
/// * `iface` - The network interface to modify.
///
/// # Errors
///
/// Returns an error if the `tc` command fails.
fn ensure_root_prio_qdisc(iface: &str) -> Result<()> {
    debug!("creating root prio qdisc");

    let root = Command::new("tc")
        .args([
            "qdisc", "replace", "dev", iface,
            "root", "handle", "1:",
            "prio", "bands", "2",
            "priomap",
            "0","0","0","0",
            "0","0","0","0",
            "0","0","0","0",
            "0","0","0","0",
        ])
        .output()
        .context("failed to execute tc (root prio)")?;

    if !root.status.success() {
        error!(
        stderr = %String::from_utf8_lossy(&root.stderr),
        "root prio stderr"
    );

        if !root_prio_exists(iface)? {
            return Err(anyhow!("tc failed creating prio root qdisc on {}", iface));
        }

        debug!("root prio already exists, continuing");
    }

    Ok(())
}

/// Attaches a child `netem` qdisc to band 1:2 of the root `prio` qdisc.
///
/// # Arguments
///
/// * `iface` - The network interface to modify.
/// * `delay_ms` - Delay in milliseconds.
/// * `loss_percent` - Loss percentage.
///
/// # Errors
///
/// Returns an error if the `tc` command fails.
fn ensure_child_netem_qdisc(iface: &str, delay_ms: u32, loss_percent: f32) -> Result<()> {
    let delay = format!("{delay_ms}ms");
    let loss = format!("{loss_percent}%");

    debug!(
    "attaching netem; parent=1:2 delay={} loss={}",
    delay, loss
);

    let netem = Command::new("tc")
        .args([
            "qdisc", "replace", "dev", iface,
            "parent", "1:2",
            "handle", "20:",
            "netem",
            "delay", &delay,
            "loss", &loss,
        ])
        .output()
        .context("failed to execute tc (netem child)")?;

    if !netem.status.success() {
        error!(
        stderr = %String::from_utf8_lossy(&netem.stderr),
        "netem stderr"
    );
        return Err(anyhow!("tc failed applying child netem on {}", iface));
    }

    Ok(())
}

/// Selects the filter mode based on whether cgroup targets are provided.
fn select_filter_mode(network_cgroup_target: &Vec<u64>) -> FilterMode {
    if network_cgroup_target.is_empty() {
        FilterMode::MatchAll
    } else {
        FilterMode::EbpfMarked
    }
}

/// Installs a match-all filter to redirect all traffic to the `netem` band.
///
/// # Arguments
///
/// * `iface` - The network interface to modify.
///
/// # Errors
///
/// Returns an error if the `tc` command fails.
fn install_match_all_filter(iface: &str) -> Result<()> {
    debug!("no cgroup targets; installing match-all filter to netem band");

    let filter = Command::new("tc")
        .args([
            "filter", "add", "dev", iface,
            "parent", "1:", "protocol", "all",
            "u32", "match", "u32", "0", "0",
            "flowid", "1:2",
        ])
        .output()
        .context("failed to execute tc (match-all filter)")?;

    if !filter.status.success() {
        error!(stderr = %String::from_utf8_lossy(&filter.stderr), "filter stderr");
        return Err(anyhow!(
        "tc failed installing match-all filter on {}: {}",
        iface,
        String::from_utf8_lossy(&filter.stderr)
    ));
    }

    Ok(())
}

/// Installs a firewall filter that redirects packets with mark 1 to the `netem` band.
///
/// # Arguments
///
/// * `iface` - The network interface to modify.
///
/// # Errors
///
/// Returns an error if the `tc` command fails.
fn install_fw_filter(iface: &str) -> Result<()> {
    debug!("installing fw filter (mark=1 → 1:2)");

    let filter = Command::new("tc")
        .args([
            "filter", "replace", "dev", iface,
            "parent", "1:",
            "protocol", "all",
            "prio", "1",
            "handle", "1",
            "fw",
            "flowid", "1:2",
        ])
        .output()
        .context("failed to execute tc (fw filter)")?;

    if !filter.status.success() {
        error!(stderr = %String::from_utf8_lossy(&filter.stderr), "filter stderr");
        return Err(anyhow!("tc failed installing fw filter on {}", iface));
    }

    Ok(())
}

/// Loads and attaches the eBPF classifier to the target interface.
///
/// # Arguments
///
/// * `iface` - The network interface to attach to.
/// * `cgroups` - A slice of cgroup IDs to target.
///
/// # Returns
///
/// Returns an [`EbpfHandle`] on success.
///
/// # Errors
///
/// Returns an error if the eBPF program cannot be loaded or attached.
pub fn attach_classifier(iface: &str, cgroups: &[u64]) -> Result<EbpfHandle> {
    let mut ebpf = Ebpf::load(aya::include_bytes_aligned!(concat!(
    env!("OUT_DIR"),
    "/chaosfilter-ebpf"
)))
        .context("failed to load embedded eBPF object")?;

    let _logger = match EbpfLogger::init(&mut ebpf) {
        Ok(logger) => {
            log::debug!("[ebpf] logger initialized");
            Some(logger)
        }
        Err(e) => {
            log::warn!("failed to initialize eBPF logger: {e}");
            None
        }
    };

    {
        let map = ebpf
            .map_mut("TARGET_CGROUPS")
            .context("TARGET_CGROUPS map not found")?;

        let mut targets: HashMap<_, u64, u8> =
            HashMap::try_from(map).context("failed to open TARGET_CGROUPS")?;

        for id in cgroups {
            targets.insert(*id, 1, 0)?;
        }
    }

    let program: &mut SchedClassifier = ebpf
        .program_mut("chaosfilter")
        .context("failed to find eBPF program named `chaosfilter`")?
        .try_into()
        .context("failed to cast program to SchedClassifier")?;

    program.load().context("failed to load classifier")?;
    program.attach(iface, TcAttachType::Egress)?;

    Ok(EbpfHandle {
        _ebpf: ebpf,
    })
}

/// Restores the root qdisc to a baseline state (`fq_codel`).
///
/// # Arguments
///
/// * `iface` - The network interface to restore.
pub fn create_restore_root(iface: &str) {
    let status = Command::new("tc")
        .args(["qdisc", "replace", "dev", iface, "root", "fq_codel"])
        .status();

    match status {
        Ok(s) if s.success() => {
            info!(iface, "root qdisc restored to fq_codel");
        }
        Ok(_) => {
            error!(iface, "failed to apply root qdisc");
        }
        Err(e) => {
            error!(error = %e, "failed to execute tc");
        }
    }
}

/// Directly applies `netem` to the root of an interface.
///
/// # Arguments
///
/// * `iface` - The network interface to modify.
/// * `delay_ms` - Delay in milliseconds.
/// * `loss_percent` - Loss percentage.
///
/// # Errors
///
/// Returns an error if the `tc` command fails.
pub fn apply_netem(iface: &str, delay_ms: u32, loss_percent: f32) -> Result<()> {
    let delay = format!("{delay_ms}ms");
    let loss = format!("{loss_percent}%");

    let status = Command::new("tc")
        .args([
            "qdisc", "replace", "dev", iface, "root", "netem", "delay", &delay, "loss", &loss,
        ])
        .status()
        .context("failed to execute tc (apply)")?;

    if !status.success() {
        return Err(anyhow!(
            "tc failed applying netem on {} (are you running as root?)",
            iface
        ));
    }

    Ok(())
}

/// Deletes the root qdisc from an interface entirely.
///
/// # Arguments
///
/// * `iface` - The network interface to modify.
pub fn delete_root_qdisc(iface: &str) {
    let status = Command::new("sudo")
        .args(["tc", "qdisc", "del", "dev", iface, "root"])
        .status();

    match status {
        Ok(s) if s.success() => {
            info!(iface, "root qdisc deleted");
        }
        Ok(_) => {
            error!(iface, "failed to delete root qdisc (there may not be one)");
        }
        Err(e) => {
            error!(error = %e, "failed to execute tc");
        }
    }
}

/// Internal helper to check if a root `prio` qdisc exists.
fn root_prio_exists(iface: &str) -> Result<bool> {
    let output = Command::new("tc")
        .args(["qdisc", "show", "dev", iface])
        .output()
        .context("failed to execute tc qdisc show")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .any(|line| line.contains("qdisc prio") && line.contains("root") && line.contains("1:")))
}