use anyhow::{anyhow, Context};
use aya_build::Toolchain;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
	//tell Cargo when this build script should be re-run
	//if these change, Cargo will re-execute build.rs
	println!("cargo:rerun-if-env-changed=AYA_BUILD_SKIP");
	println!("cargo:rerun-if-changed=../aya-qdisc-ebpf");

	//ask Cargo for metadata about the current workspace
	//this lets us discover where the eBPF crate lives
	let metadata = cargo_metadata::MetadataCommand::new()
		.no_deps()
		.exec()
		.context("failed to read cargo metadata")?;

	//find the eBPF crate by name inside the workspace
	//this must match the crate name in aya-qdisc-ebpf/Cargo.toml
	let ebpf_pkg = metadata
		.packages
		.iter()
		.find(|p| p.name == "aya-qdisc-ebpf")
		.ok_or_else(|| anyhow!("aya-qdisc-ebpf package not found"))?;

	//get the directory that contains the eBPF crate's Cargo.toml
	let ebpf_dir: PathBuf = ebpf_pkg
		.manifest_path
		.parent()
		 .ok_or_else(|| anyhow!("invalid ebpf manifest path"))?
		.into();

	//convert the path to an absolute, canonical form
	//this avoids issues with relative paths later
	let ebpf_dir = ebpf_dir
		.canonicalize()
		.context("canonicalize ebpf dir")?;

	//describe the eBPF package for aya-build
	//this tells Aya which crate to build and where it lives
	let ebpf_package = aya_build::Package {
		name: "aya-qdisc-ebpf",
		root_dir: ebpf_dir.to_str().unwrap(),
		features: &[],
		no_default_features: false,
	};

	//build the eBPF program using Aya's eBPF toolchain
	//this is REQUIRED to avoid mixing eBPG core with the userspace core 
	std::env::set_var("CARGO_TARGET_DIR", "target-ebpf");

	aya_build::build_ebpf(
		[ebpf_package],
		Toolchain::default(),
	)
}
