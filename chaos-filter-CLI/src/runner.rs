use crate::cli::Cli;
use anyhow::Result;

pub fn run(args: &Cli) -> Result<()> {
    println!("Running chaos test:");
    println!("  cgroup: {}", args.cgroup);
    println!("  iface: {:?}", args.iface);
    println!("  latency: {:?}", args.latency);
    println!("  loss: {:?}", args.loss);
    println!("  duration: {}", args.duration);

    // TODO:
    // Call teammate's code here (library or binary)

    Ok(())
}
