use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::edge::Pointer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AndroidBounds {
  pub width: i32,
  pub height: i32,
}

pub struct AndroidInput {
  bounds: AndroidBounds,
  shell: Child,
  stdin: ChildStdin,
}

impl AndroidInput {
  pub fn new(adb: String, serial: Option<String>, bounds: AndroidBounds) -> Result<Self> {
    let mut shell = adb_command(&adb, serial.as_deref())
      .arg("shell")
      .stdin(Stdio::piped())
      .spawn()
      .with_context(|| format!("failed to start persistent {adb} shell"))?;
    let stdin = shell
      .stdin
      .take()
      .context("failed to open persistent adb shell stdin")?;

    Ok(Self {
      bounds,
      shell,
      stdin,
    })
  }

  pub fn bounds(&self) -> AndroidBounds {
    self.bounds
  }

  pub fn move_pointer(&mut self, pointer: Pointer) -> Result<()> {
    let pointer = Pointer {
      x: pointer.x.clamp(0, self.bounds.width.saturating_sub(1)),
      y: pointer.y.clamp(0, self.bounds.height.saturating_sub(1)),
    };
    let x = pointer.x.to_string();
    let y = pointer.y.to_string();

    writeln!(self.stdin, "input mouse motionevent MOVE {x} {y}")
      .context("failed to write Android pointer movement to adb shell")?;
    self
      .stdin
      .flush()
      .context("failed to flush Android pointer movement to adb shell")
  }

  pub fn detect_bounds(adb: &str, serial: Option<&str>) -> Result<AndroidBounds> {
    let output = adb_command(adb, serial)
      .args(["shell", "wm", "size"])
      .output()
      .with_context(|| format!("failed to execute {adb}"))?;

    if !output.status.success() {
      bail!("adb wm size exited with {}", output.status);
    }

    parse_wm_size(&String::from_utf8_lossy(&output.stdout))
      .context("failed to parse Android display size from `adb shell wm size`")
  }

  pub fn stop(&mut self) -> Result<()> {
    writeln!(self.stdin, "exit").ok();
    self.stdin.flush().ok();
    self.shell.kill().ok();
    self
      .shell
      .wait()
      .context("failed to wait for persistent adb shell")?;
    Ok(())
  }
}

impl Drop for AndroidInput {
  fn drop(&mut self) {
    let _ = self.stop();
  }
}

fn adb_command(adb: &str, serial: Option<&str>) -> Command {
  let mut command = Command::new(adb);
  if let Some(serial) = serial {
    command.args(["-s", serial]);
  }
  command
}

fn parse_wm_size(output: &str) -> Option<AndroidBounds> {
  output.lines().find_map(|line| {
    let size = line
      .strip_prefix("Physical size:")
      .or_else(|| line.strip_prefix("Override size:"))?
      .trim();
    let (width, height) = size.split_once('x')?;

    Some(AndroidBounds {
      width: width.trim().parse().ok()?,
      height: height.trim().parse().ok()?,
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_physical_wm_size() {
    assert_eq!(
      parse_wm_size("Physical size: 1080x2400\n"),
      Some(AndroidBounds {
        width: 1080,
        height: 2400,
      }),
    );
  }
}
