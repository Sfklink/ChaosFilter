mod cli;
mod validate;
mod runner;
mod qdisc;

use clap::Parser;
use anyhow::Result;
use cli::Cli;

fn main() -> Result<()> {
    let args = Cli::parse();

    runner::run(&args)?;

    Ok(())
}