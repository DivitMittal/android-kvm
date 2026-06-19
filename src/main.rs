mod config;
mod edge;
mod host;
mod runtime;
mod scrcpy;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::host::default_host_pointer;
use crate::runtime::Runtime;
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

            Runtime::new(config, default_host_pointer()?)
                .run()
                .context("android-kvm runtime failed")
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
