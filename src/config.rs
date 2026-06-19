use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::edge::Edge;
use crate::scrcpy::ScrcpyConfig;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
  pub android_edge: Edge,
  pub activation_pixels: u32,
  pub release_pixels: u32,
  pub poll_interval_ms: u64,
  pub pointer_scale: f32,
  pub audio_always_on: bool,
  pub adb_binary: String,
  pub android_width: Option<i32>,
  pub android_height: Option<i32>,
  pub scrcpy_server_path: Option<String>,
  pub control_port: u16,
  pub scrcpy: ScrcpyConfig,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      android_edge: Edge::Right,
      activation_pixels: 24,
      release_pixels: 4,
      poll_interval_ms: 16,
      pointer_scale: 1.0,
      audio_always_on: true,
      adb_binary: "adb".to_string(),
      android_width: None,
      android_height: None,
      scrcpy_server_path: None,
      control_port: 0,
      scrcpy: ScrcpyConfig::default(),
    }
  }
}

impl Config {
  pub fn load(path: Option<&Path>) -> Result<Self> {
    let Some(path) = path.map(PathBuf::from).or_else(default_config_path) else {
      debug!("no config path found, using defaults");
      return Ok(Self::default());
    };

    if !path.exists() {
      debug!(path = %path.display(), "config file not found, using defaults");
      return Ok(Self::default());
    }

    let raw = fs::read_to_string(&path)
      .with_context(|| format!("failed to read config at {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse config at {}", path.display()))
  }

  pub fn validate(&self) -> Result<()> {
    if self.activation_pixels == 0 {
      bail!("activation-pixels must be greater than 0");
    }
    if self.release_pixels == 0 {
      bail!("release-pixels must be greater than 0");
    }
    if !self.pointer_scale.is_finite() || self.pointer_scale <= 0.0 {
      bail!("pointer-scale must be a finite number greater than 0");
    }
    if self.poll_interval_ms == 0 {
      bail!("poll-interval-ms must be greater than 0");
    }
    if self.adb_binary.is_empty() {
      bail!("adb-binary must not be empty");
    }
    if self.scrcpy.binary.is_empty() {
      bail!("scrcpy.binary must not be empty");
    }
    if let Some(width) = self.android_width
      && width <= 0
    {
      bail!("android-width must be greater than 0 when set");
    }
    if let Some(height) = self.android_height
      && height <= 0
    {
      bail!("android-height must be greater than 0 when set");
    }
    if self.android_width.is_some() != self.android_height.is_some() {
      bail!("android-width and android-height must be set together");
    }
    if let (Some(width), Some(height)) = (self.android_width, self.android_height) {
      let release_pixels = i32::try_from(self.release_pixels)
        .context("release-pixels is too large for Android geometry")?;
      if release_pixels >= width || release_pixels >= height {
        bail!("release-pixels must be smaller than android-width and android-height");
      }
    }

    Ok(())
  }
}

fn default_config_path() -> Option<PathBuf> {
  dirs::config_dir().map(|dir| dir.join("android-kvm/config.toml"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_kebab_case_audio_always_on() {
    let config: Config = toml::from_str(
      r#"
android-edge = "left"
audio-always-on = false
"#,
    )
    .unwrap();

    assert_eq!(config.android_edge, Edge::Left);
    assert!(!config.audio_always_on);
  }

  #[test]
  fn defaults_audio_to_always_on() {
    assert!(Config::default().audio_always_on);
  }

  #[test]
  fn rejects_zero_activation_pixels() {
    let config = Config {
      activation_pixels: 0,
      ..Config::default()
    };

    assert!(config.validate().is_err());
  }

  #[test]
  fn rejects_non_positive_pointer_scale() {
    let config = Config {
      pointer_scale: 0.0,
      ..Config::default()
    };

    assert!(config.validate().is_err());
  }

  #[test]
  fn rejects_release_pixels_outside_configured_device_bounds() {
    let config = Config {
      android_width: Some(100),
      android_height: Some(200),
      release_pixels: 100,
      ..Config::default()
    };

    assert!(config.validate().is_err());
  }
}
