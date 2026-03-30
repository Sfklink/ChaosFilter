//! # ChaosFilter Build Script
//!
//! This build script is responsible for compiling the eBPF programs located in the
//! `chaosfilter-ebpf` crate. It uses `aya-build` to coordinate the BPF compilation
//! and ensures that the resulting BPF objects are available to the userspace
//! controller at runtime.

use anyhow::{Context as _, anyhow};
use aya_build::Toolchain;

fn main() -> anyhow::Result<()> {
    let cargo_metadata::Metadata { packages, .. } = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("MetadataCommand::exec")?;

    let ebpf_package = packages
        .into_iter()
        .find(|cargo_metadata::Package { name, .. }| name.as_str() == "chaosfilter-ebpf")
        .ok_or_else(|| anyhow!("chaosfilter-ebpf package not found"))?;

    let cargo_metadata::Package {
        name,
        manifest_path,
        ..
    } = ebpf_package;

    let ebpf_package = aya_build::Package {
        name: name.as_str(),
        root_dir: manifest_path
            .parent()
            .ok_or_else(|| anyhow!("no parent for {manifest_path}"))?
            .as_str(),
        ..Default::default()
    };

    // Trigger the eBPF build using the default toolchain.
    aya_build::build_ebpf([ebpf_package], Toolchain::default())
}
