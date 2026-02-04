use anyhow::Result;
use clap::{Parser, Subcommand};
use chaosfilter_common::Plan;

#[derive(Parser, Debug)]
#[command(name = "chaosfilter-cli", version, about = "ChaosFilter control CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Validate a chaos config file
    Validate {
        /// Path to TOML config
        #[arg(short, long)]
        config: String,
    },

    /// Run the chaos plan (apply -> hold -> revert)
    Run {
        /// Path to TOML config
        #[arg(short, long)]
        config: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { config } => {
            let plan = Plan::load_from_toml_file(&config)?;
            chaosfilter_control::validate_plan(&plan)?;
            println!("Config OK: {:?}", plan);
        }
        Commands::Run { config } => {
            let plan = Plan::load_from_toml_file(&config)?;
            chaosfilter_control::run_plan(&plan)?;
            println!("Run complete.");
        }
        
    }

    Ok(())
}
