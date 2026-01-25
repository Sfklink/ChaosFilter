use anyhow::Result;
use aya_build::{build_ebpf, Package, Toolchain};

fn main() -> Result<()> {
    build_ebpf(
        [Package {
            name: "chaosfilter-ebpf",
            root_dir: "ebpf",
            features: &[],
            no_default_features: false,
        }],
        Toolchain::Nightly,
    )?;
    Ok(())
}
