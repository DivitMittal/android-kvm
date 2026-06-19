use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::edge::Edge;
use crate::scrcpy::ScrcpyConfig;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
  pub android_edge: Edge,
  pub activation_pixels: u32,
  pub release_pixels: u32,
  pub poll_interval_ms: u64,
  pub pointer_scale: f32,
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
      activation_pixels: 1,
      release_pixels: 4,
      poll_interval_ms: 16,
      pointer_scale: 1.0,
      adb_binary: "adb".to_string(),
      android_width: None,
      android_height: None,
      scrcpy_server_path: None,
      control_port: 27183,
      scrcpy: ScrcpyConfig::default(),
    }
  }
}

impl Config {
  pub fn load(path: Option<&Path>) -> Result<Self> {
    let Some(path) = path.map(PathBuf::from).or_else(default_config_path) else {
      return Ok(Self::default());
    };

    if !path.exists() {
      return Ok(Self::default());
    }

    let raw = fs::read_to_string(&path)
      .with_context(|| format!("failed to read config at {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse config at {}", path.display()))
  }
}

fn default_config_path() -> Option<PathBuf> {
  dirs::config_dir().map(|dir| dir.join("android-kvm/config.toml"))
}
