use anyhow::{Result, anyhow};
use std::process::Command;
use std::fmt;

#[derive(Debug)]
pub struct PingStats {
    pub transmitted: u32,
    pub received: u32,
    pub loss_pct: f32,
    pub min_ms: f32,
    pub avg_ms: f32,
    pub max_ms: f32,
}

impl fmt::Display for PingStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tx={} rx={} loss={:.1}% rtt(min/avg/max)={:.2}/{:.2}/{:.2} ms",
            self.transmitted,
            self.received,
            self.loss_pct,
            self.min_ms,
            self.avg_ms,
            self.max_ms
        )
    }
}

/// Ping using explicit source and destination IPs.
/// This guarantees packets egress the intended interface.
pub fn ping_path(iface: &str, dest_ip: &str, count: u32) -> Result<PingStats> {
    let output = Command::new("ping")
        .args([
            "-c", &count.to_string(),
            "-I", iface,
            dest_ip,
        ])
        .output()
        .map_err(|e| anyhow!("failed to execute ping: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ping_output(&stdout)
}

fn parse_ping_output(output: &str) -> Result<PingStats> {
    let mut transmitted = None;
    let mut received = None;
    let mut loss_pct = None;
    let mut min_ms = None;
    let mut avg_ms = None;
    let mut max_ms = None;

    for line in output.lines() {
        if line.contains("packets transmitted") {
            let parts: Vec<&str> = line.split(',').collect();

            transmitted = Some(parts[0].trim().split_whitespace().next().unwrap().parse()?);
            received = Some(parts[1].trim().split_whitespace().next().unwrap().parse()?);

            let loss_str = parts[2]
                .trim()
                .split_whitespace()
                .next()
                .unwrap()
                .trim_end_matches('%');

            loss_pct = Some(loss_str.parse()?);
        }

        if line.contains("min/avg/max") {
            let stats = line.split('=').nth(1).unwrap().trim().split_whitespace().next().unwrap();
            let nums: Vec<&str> = stats.split('/').collect();

            min_ms = Some(nums[0].parse()?);
            avg_ms = Some(nums[1].parse()?);
            max_ms = Some(nums[2].parse()?);
        }
    }

    Ok(PingStats {
        transmitted: transmitted.ok_or_else(|| anyhow!("missing transmitted"))?,
        received: received.ok_or_else(|| anyhow!("missing received"))?,
        loss_pct: loss_pct.ok_or_else(|| anyhow!("missing loss"))?,
        min_ms: min_ms.unwrap_or(0.0),
        avg_ms: avg_ms.unwrap_or(0.0),
        max_ms: max_ms.unwrap_or(0.0),
    })
}
