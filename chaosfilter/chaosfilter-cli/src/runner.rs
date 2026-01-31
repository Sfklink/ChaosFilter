use anyhow::Result;
use crate::cli::Cli;

pub fn run(args: &Cli) -> Result<()> {
    println!("Running chaos test:");
    println!("  cgroup: {}", args.cgroup);
    println!("  iface: {:?}", args.iface);
    println!("  latency: {:?}", args.latency);
    println!("  loss: {:?}", args.loss);
    println!("  duration: {}", args.duration);

    // Call teammate's code here

    Ok(())
}