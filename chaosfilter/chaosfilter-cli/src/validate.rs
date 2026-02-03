use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;

pub fn validate_cgroup(cgroup: &str) -> Result<()> {
    // Check cgroup exists
    let cgroup_path = format!("/sys/fs/cgroup/{}", cgroup);
    if !Path::new(&cgroup_path).exists() {
        bail!("cgroup does not exist: {}", cgroup_path);
    }

    Ok(())
}

pub fn validate_iface(iface: &str) -> Result<()> {
    // Check network interface exists (if provided)
    let status = Command::new("ip")
        .args(["link", "show", iface])
        .status()?;

    if !status.success() {
        bail!("network interface not found: {}", iface);
    }

    Ok(())
}