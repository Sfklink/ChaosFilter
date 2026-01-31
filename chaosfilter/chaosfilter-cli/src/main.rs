mod cli;
mod validate;
mod runner;

use clap::Parser;
use anyhow::Result;
use cli::Cli;

fn main() -> Result<()> {
    let args = Cli::parse();

    validate::validate(&args)?;
    runner::run(&args)?;

    Ok(())
}