use anyhow::{anyhow, Context, Result};
use chaosfilter_common::Plan;
use std::process::Command;

// use crate::injector::Injector;

#[derive(Default)]
pub struct QdiscNetem {
    applied: bool,
    iface: Option<String>,
}

impl QdiscNetem {
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