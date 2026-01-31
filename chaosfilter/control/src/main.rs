use anyhow::Context;
use aya::Bpf;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	//initializes the logging
	env_logger::init();

	//load eBPF object bytes (copied by build.rs into OUT_DIR)
	let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/chaosfilter-ebpf.o"));
	let _bpf = Bpf::load(bytes).context("failed to load eBPF object")?;

	println!("Loaded eBPF object successfully (not attached).");
	Ok(())
}
