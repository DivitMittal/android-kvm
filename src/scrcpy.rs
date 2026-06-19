use std::process::{Command, ExitStatus};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ScrcpyConfig {
  pub binary: String,
  pub serial: Option<String>,
  pub audio_enabled: bool,
  pub audio_buffer_ms: u16,
  pub extra_args: Vec<String>,
}

impl Default for ScrcpyConfig {
  fn default() -> Self {
    Self {
      binary: "scrcpy".to_string(),
      serial: None,
      audio_enabled: true,
      audio_buffer_ms: 200,
      extra_args: Vec::new(),
    }
  }
}

#[derive(Clone, Debug)]
pub struct ScrcpyBackend {
  config: ScrcpyConfig,
}

impl ScrcpyBackend {
  pub fn new(config: ScrcpyConfig) -> Self {
    Self { config }
  }

  pub fn check(&self) -> Result<()> {
    let status = Command::new(&self.config.binary)
      .arg("--version")
      .status()
      .with_context(|| format!("failed to execute {}", self.config.binary))?;

    ensure_success(status, "scrcpy --version")
  }
}

fn ensure_success(status: ExitStatus, label: &str) -> Result<()> {
  if status.success() {
    return Ok(());
  }

  bail!("{label} exited with {status}")
}

pub(crate) fn shell_words<'a>(words: impl IntoIterator<Item = &'a str>) -> String {
  words
    .into_iter()
    .map(shell_quote)
    .collect::<Vec<_>>()
    .join(" ")
}

fn shell_quote(value: &str) -> String {
  if value
    .chars()
    .all(|ch| ch.is_ascii_alphanumeric() || "_+-=./:".contains(ch))
  {
    return value.to_string();
  }

  format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn shell_words_quotes_arguments_with_spaces() {
    assert_eq!(
      shell_words(["scrcpy", "--serial", "device one"]),
      "scrcpy --serial 'device one'",
    );
  }
}
