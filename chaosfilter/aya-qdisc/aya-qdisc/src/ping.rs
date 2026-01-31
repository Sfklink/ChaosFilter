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

/// Public API expected by compare.rs
pub fn ping_path(iface: &str, target: &str, count: u32) -> Result<PingStats> {
    ensure_namespace_exists()?;

    // Map logical interfaces → namespace interfaces
    let ns_iface = match iface {
        "control" => "control-peer",
        "vethA" => "vethB",
        other => {
            return Err(anyhow!(
                "ping_path: unsupported interface '{other}'"
            ));
        }
    };

    let output = Command::new("ip")
        .args([
            "netns", "exec", "chaos",
            "ping",
            "-I", ns_iface,
            "-c", &count.to_string(),
            target,
        ])
        .output()
        .map_err(|e| anyhow!("failed to execute ping: {e}"))?;

    if !output.status.success() {
        return Err(anyhow!(
            "ping failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ping_output(&stdout)
}

fn parse_ping_output(output: &str) -> Result<PingStats> {
    let mut transmitted = 0;
    let mut received = 0;
    let mut loss_pct = 0.0;
    let mut min_ms = 0.0;
    let mut avg_ms = 0.0;
    let mut max_ms = 0.0;

    for line in output.lines() {
        if line.contains("packets transmitted") {
            // Example:
            // "10 packets transmitted, 8 received, 20% packet loss, time 9009ms"
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 3 {
                transmitted = parts[0]
                    .trim()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse()?;

                received = parts[1]
                    .trim()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse()?;

                loss_pct = parts[2]
                    .trim()
                    .split('%')
                    .next()
                    .unwrap()
                    .parse()?;
            }
        }

        if line.contains("min/avg/max") {
            // Example:
            // "rtt min/avg/max/mdev = 12.3/45.6/78.9/1.2 ms"
            let stats = line.split('=').nth(1).unwrap().trim();
            let values: Vec<&str> = stats.split('/').collect();
            if values.len() >= 3 {
                min_ms = values[0].parse()?;
                avg_ms = values[1].parse()?;
                max_ms = values[2].parse()?;
            }
        }
    }

    Ok(PingStats {
        transmitted,
        received,
        loss_pct,
        min_ms,
        avg_ms,
        max_ms,
    })
}

fn ensure_namespace_exists() -> Result<()> {
    let status = Command::new("ip")
        .args(["netns", "list"])
        .output()?;

    let namespaces = String::from_utf8_lossy(&status.stdout);
    if namespaces.lines().any(|l| l.starts_with("chaos")) {
        return Ok(());
    }

    // Create namespace
    Command::new("ip")
        .args(["netns", "add", "chaos"])
        .status()?;

    Ok(())
}
