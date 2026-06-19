mod config;
#[allow(dead_code)]
mod edge;
mod runtime;
mod scrcpy;
mod scrcpy_control;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::Config;
use crate::edge::Edge;
use crate::runtime::Runtime;
use crate::scrcpy::ScrcpyBackend;

#[derive(Debug, Parser)]
#[command(version, about = "USB Android software KVM, backed by scrcpy")]
struct Cli {
  #[arg(short, long, value_name = "PATH")]
  config: Option<PathBuf>,

  /// Override which host edge leads to the Android device.
  #[arg(long, value_name = "EDGE")]
  android_edge: Option<Edge>,

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
  init_logging();

  let cli = Cli::parse();
  let mut config = Config::load(cli.config.as_deref())?;
  if let Some(android_edge) = cli.android_edge {
    config.android_edge = android_edge;
  }
  config.validate()?;

  match cli.command {
    Command::Run { dry_run } => {
      let backend = ScrcpyBackend::new(config.scrcpy.clone());
      if dry_run {
        println!("{}", backend.command_preview());
        return Ok(());
      }

      Runtime::new(config)
        .run()
        .context("android-kvm runtime failed")
    }
    Command::PrintConfig => {
      println!("{}", toml::to_string_pretty(&config)?);
      Ok(())
    }
    Command::Check => {
      ScrcpyBackend::new(config.scrcpy).check()?;
      info!("dependency check passed");
      Ok(())
    }
  }
}

fn init_logging() {
  let filter =
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("android_kvm=info"));

  fmt()
    .compact()
    .with_env_filter(filter)
    .with_target(false)
    .init();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_android_edge_override() {
    let cli = Cli::try_parse_from(["android-kvm", "--android-edge", "left", "run"]).unwrap();

    assert_eq!(cli.android_edge, Some(Edge::Left));
  }
}
