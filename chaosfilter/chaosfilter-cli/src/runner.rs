use anyhow::{bail, Context, Result};
use std::time::Duration;

use crate::cli::{Cli, Command};
use crate::validate;
use crate::qdisc;

pub fn run(cli: &Cli) -> Result<()> {
    match &cli.cmd {
        Command::Status { iface } => {
            validate::validate_iface(iface)?;
            let out = qdisc::show_qdiscs(Some(iface))?;
            print!("{out}");
            Ok(())
        }

        Command::Reset { iface } => {
            validate::validate_iface(iface)?;
            qdisc::del_root_qdisc(iface).context("failed to delete root qdisc")?;
            println!("Reset complete for {iface}");
            Ok(())
        }

        Command::Net { cgroup, iface, latency, loss, duration } => {
            validate::validate_cgroup(cgroup)?;

            let dev = iface
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--iface is required for `net`"))?;
            validate::validate_iface(dev)?;

            // Parse durations like "30s" or "1500ms"
            let dur = parse_duration(duration)?;

            // Parse latency "200ms" becomes 200 ms, else default which is also 200
            let delay_ms = latency
                .as_deref()
                .map(parse_latency_ms)
                .transpose()?
                .unwrap_or(200);

            // Parse loss "5%" becomes 5.0, else default which is 0
            let loss_pct = loss
                .as_deref()
                .map(parse_loss_pct)
                .transpose()?
                .unwrap_or(0.0);

            // The following things are a 4 step process
            // 1) Apply netem
            qdisc::add_netem(dev, delay_ms, loss_pct)
                .context("failed to apply netem")?;

            println!(
                "Chaos running on dev={dev} for {duration} (delay={}ms, loss={}%, cgroup={cgroup}). Ctrl+C to stop.",
                delay_ms, loss_pct
            );

            // 2) Ensure cleanup when Ctrl+C is pressed
            let dev_owned = dev.to_string();
            ctrlc::set_handler(move || {
                let _ = qdisc::del_root_qdisc(&dev_owned);
                std::process::exit(130);
            })
            .context("failed to set Ctrl+C handler")?;

            // 3) Stop/sleep for requested duration
            std::thread::sleep(dur);

            // 4) Cleanup after duration
            qdisc::del_root_qdisc(dev).context("failed to clean up qdisc")?;

            println!("Done. Restored dev={dev}");
            Ok(())
        }
    }
}

fn parse_duration(s: &str) -> Result<Duration> {
    if let Some(ms) = s.strip_suffix("ms") {
        return Ok(Duration::from_millis(ms.parse()?));
    }
    if let Some(sec) = s.strip_suffix('s') {
        return Ok(Duration::from_secs(sec.parse()?));
    }

    bail!("invalid duration '{s}'. Use e.g. 30s or 1500ms");
}

/// Accepts formats like "200ms" or "200" (assumed ms).
fn parse_latency_ms(s: &str) -> Result<u32> {
    if let Some(ms) = s.strip_suffix("ms") {
        return Ok(ms.parse()?);
    }
    
    Ok(s.parse()?)
}

/// Accepts "5%" or "5" as percentage.
fn parse_loss_pct(s: &str) -> Result<f32> {
    if let Some(p) = s.strip_suffix('%') {
        return Ok(p.parse()?);
    }

    Ok(s.parse()?)
}