mod config;
#[allow(dead_code)]
mod edge;
mod scrcpy;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::scrcpy::ScrcpyBackend;

#[derive(Debug, Parser)]
#[command(version, about = "USB Android software KVM, backed by scrcpy")]
struct Cli {
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the configured scrcpy backend.
    Run {
        /// Print the scrcpy command without executing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print the resolved configuration.
    PrintConfig,
    /// Validate dependencies and configuration.
    Check,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load(cli.config.as_deref())?;

    match cli.command {
        Command::Run { dry_run } => {
            let backend = ScrcpyBackend::new(config.scrcpy.clone());
            if dry_run {
                println!("{}", backend.command_preview());
                return Ok(());
            }

            backend.run().context("failed to run scrcpy backend")
        }
        Command::PrintConfig => {
            println!("{}", toml::to_string_pretty(&config)?);
            Ok(())
        }
        Command::Check => {
            ScrcpyBackend::new(config.scrcpy).check()?;
            println!("ok");
            Ok(())
        }
    }
}
