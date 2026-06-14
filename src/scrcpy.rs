use std::ffi::OsString;
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ScrcpyConfig {
    pub binary: String,
    pub serial: Option<String>,
    pub no_video: bool,
    pub audio_enabled: bool,
    pub audio_buffer_ms: u16,
    pub keyboard: HidMode,
    pub mouse: HidMode,
    pub mouse_bind: String,
    pub shortcut_mod: String,
    pub extra_args: Vec<String>,
}

impl Default for ScrcpyConfig {
    fn default() -> Self {
        Self {
            binary: "scrcpy".to_string(),
            serial: None,
            no_video: true,
            audio_enabled: true,
            audio_buffer_ms: 200,
            keyboard: HidMode::Uhid,
            mouse: HidMode::Uhid,
            mouse_bind: "bhsn".to_string(),
            shortcut_mod: "rctrl".to_string(),
            extra_args: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HidMode {
    Disabled,
    Sdk,
    Uhid,
    Aoa,
}

impl HidMode {
    fn as_scrcpy_value(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Sdk => "sdk",
            Self::Uhid => "uhid",
            Self::Aoa => "aoa",
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

    pub fn run(&self) -> Result<()> {
        let status = self
            .command()
            .status()
            .with_context(|| format!("failed to execute {}", self.config.binary))?;

        ensure_success(status, "scrcpy backend")
    }

    pub fn command_preview(&self) -> String {
        shell_words(std::iter::once(OsString::from(&self.config.binary)).chain(self.args()))
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.config.binary);
        command.args(self.args());
        command
    }

    fn args(&self) -> Vec<OsString> {
        let mut args = Vec::new();

        if let Some(serial) = &self.config.serial {
            args.push("--serial".into());
            args.push(serial.into());
        }

        if self.config.no_video {
            args.push("--no-video".into());
        }

        if self.config.audio_enabled {
            args.push(format!("--audio-buffer={}", self.config.audio_buffer_ms).into());
        } else {
            args.push("--no-audio".into());
        }

        args.push(format!("--keyboard={}", self.config.keyboard.as_scrcpy_value()).into());
        args.push(format!("--mouse={}", self.config.mouse.as_scrcpy_value()).into());
        args.push(format!("--mouse-bind={}", self.config.mouse_bind).into());
        args.push(format!("--shortcut-mod={}", self.config.shortcut_mod).into());
        args.extend(self.config.extra_args.iter().map(OsString::from));

        args
    }
}

fn ensure_success(status: ExitStatus, label: &str) -> Result<()> {
    if status.success() {
        return Ok(());
    }

    bail!("{label} exited with {status}")
}

fn shell_words(words: impl IntoIterator<Item = OsString>) -> String {
    words
        .into_iter()
        .map(|word| shell_quote(&word.to_string_lossy()))
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
