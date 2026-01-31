use crate::cli::Cli;
use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;

pub fn validate(args: &Cli) -> Result<()> {
    // Check cgroup exists
    let cgroup_path = format!("/sys/fs/cgroup/{}", args.cgroup);
    if !Path::new(&cgroup_path).exists() {
        bail!("cgroup does not exist: {}", cgroup_path);
    }

    // Check network interface exists (if provided)
    if let Some(iface) = &args.iface {
        let status = Command::new("ip")
            .args(["link", "show", iface])
            .status()?;

        if !status.success() {
            bail!("network interface not found: {}", iface);
        }
    }

    Ok(())
}