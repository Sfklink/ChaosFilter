//! tc netem injector.
//!
//! Applies a root `netem` qdisc to the configured network interface and restores
//! a known-good baseline on revert.

use anyhow::{anyhow, Context, Result};
use chaosfilter_common::Plan;
use std::process::Command;

/// tc netem injector state.
///
/// Tracks whether chaos was applied so `revert` can be idempotent.
#[derive(Default)]
pub struct QdiscNetem {
    applied: bool,
    iface: Option<String>,
}

impl QdiscNetem {
    /// Validates that the plan contains a usable network interface.
    ///
    /// # Arguments
    /// * `plan` - Chaos plan containing `targets.iface`.
    ///
    /// # Returns
    /// Returns `Ok(())` if `targets.iface` is present and the interface exists.
    ///
    /// # Side Effects
    /// Executes `ip link show <iface>` (read-only check).
    ///
    /// # Errors
    /// Returns an error if:
    /// - `targets.iface` is missing, or
    /// - the interface does not exist, or
    /// - the `ip` command fails to execute.
    pub fn validate(plan: &Plan) -> Result<()> {
        let iface = plan
            .targets
            .iface
            .as_deref()
            .ok_or_else(|| anyhow!("qdisc_netem requires targets.iface"))?;

        // keep verbose: show interface
        let status = Command::new("ip").args(["link", "show", iface]).status()?;
        if !status.success() {
            return Err(anyhow!("network interface not found: {}", iface));
        }

        Ok(())
    }

    /// Prints the current qdisc state for `iface` (best effort).
    ///
    /// # Arguments
    /// * `iface` - Interface to inspect.
    ///
    /// # Side Effects
    /// Executes `tc qdisc show dev <iface>` and may print warnings to stderr.
    ///
    /// # Notes
    /// This helper is intentionally non-fatal: failures are logged instead of returned.
    fn show_qdisc(iface: &str) {
        match Command::new("tc")
            .args(["qdisc", "show", "dev", iface])
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!("[qdisc] warning: tc qdisc show exited {}", status),
            Err(e) => eprintln!("[qdisc] warning: failed to run tc qdisc show: {}", e),
        }
    }

    /// Applies `tc netem` according to `plan.injectors.qdisc_netem`.
    ///
    /// # Arguments
    /// * `plan` - Chaos plan containing `targets.iface` and netem parameters.
    ///
    /// # Returns
    /// Returns `Ok(())` if the qdisc was applied successfully.
    ///
    /// # Side Effects
    /// Executes `tc qdisc replace ... netem delay <delay> loss <loss>` and updates internal state.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if:
    /// - the `tc` command fails to execute, or
    /// - `tc` exits non-zero (often due to missing privileges).
    ///
    /// # Panics
    /// May panic if `plan.targets.iface` is `None` (uses `unwrap()`); callers should
    /// call [`QdiscNetem::validate`] first or ensure the plan is valid.
    pub fn apply(&mut self, plan: &Plan) -> Result<()> {
        let iface = plan.targets.iface.as_deref().unwrap();
        let delay_ms = plan.injectors.qdisc_netem.delay_ms;
        let loss_percent = plan.injectors.qdisc_netem.loss_percent;

        println!(
            "[qdisc] applying netem to {} (delay={}ms loss={}%)",
            iface, delay_ms, loss_percent
        );

        let delay = format!("{delay_ms}ms");
        let loss = format!("{loss_percent}%");

        let status = Command::new("tc")
            .args([
                "qdisc", "replace",
                "dev", iface,
                "root",
                "netem",
                "delay", &delay,
                "loss", &loss,
            ])
            .status()
            .context("failed to execute tc (apply)")?;

        if !status.success() {
            return Err(anyhow!("tc failed applying netem on {} (need sudo)", iface));
        }
        Self::show_qdisc(iface);

        self.applied = true;
        self.iface = Some(iface.to_string());
        Ok(())
    }

    /// Restores a deterministic baseline root qdisc on `iface`.
    ///
    /// This replaces the current root qdisc with `fq_codel`.
    ///
    /// # Arguments
    /// * `iface` - Interface to restore.
    ///
    /// # Side Effects
    /// Executes `tc qdisc replace dev <iface> root fq_codel`.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`).
    ///
    /// # Errors
    /// Returns an error if `tc` fails to execute or exits non-zero.
    ///
    /// # Notes
    /// - This is a destructive operation: it does not attempt to restore a previously
    ///   existing qdisc configuration.
    /// - `fq_codel` is used as a known baseline to make [`QdiscNetem::revert`] deterministic.
    fn restore_default(iface: &str) -> Result<()> {
        let status = Command::new("tc")
            .args(["qdisc", "replace", "dev", iface, "root", "fq_codel"])
            .status()
            .context("failed to execute tc (restore fq_codel)")?;

        if !status.success() {
            return Err(anyhow!("tc failed restoring fq_codel on {}", iface));
        }
        Ok(())
    }

    /// Reverts any applied qdisc changes.
    ///
    /// # Returns
    /// Returns `Ok(())` if nothing was applied or if revert completed successfully.
    ///
    /// # Side Effects
    /// If chaos was applied, restores the baseline root qdisc and prints verification output.
    ///
    /// # Requires
    /// CAP_NET_ADMIN (typically `sudo`), when a revert is actually performed.
    ///
    /// # Errors
    /// Returns an error if restoring the baseline qdisc fails.
    ///
    /// # Notes
    /// This method is intended to be idempotent: calling it multiple times should be safe.
    pub fn revert(&mut self) -> Result<()> {
        if !self.applied {
            println!("[qdisc] nothing applied; skipping revert");
            return Ok(());
        }

        let iface = self.iface.as_deref().unwrap();

        println!("[qdisc] reverting qdisc on {}", iface);

        // Deterministic revert: restore the known-good root qdisc.
        Self::restore_default(iface)?;

        // Verbose verification
        Self::show_qdisc(iface);

        self.applied = false;
        self.iface = None;

        Ok(())
    }
}