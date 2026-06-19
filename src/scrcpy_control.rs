use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;

use anyhow::{Context, Result, bail};

const TYPE_UHID_CREATE: u8 = 12;
const TYPE_UHID_INPUT: u8 = 13;
const TYPE_UHID_DESTROY: u8 = 14;
const HID_ID_MOUSE: u16 = 2;

const MOUSE_REPORT_DESC: &[u8] = &[
  0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x05,
  0x15, 0x00, 0x25, 0x01, 0x95, 0x05, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x03, 0x81, 0x01,
  0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x03,
  0x81, 0x06, 0x05, 0x0C, 0x0A, 0x38, 0x02, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x01, 0x81,
  0x06, 0xC0, 0xC0,
];

pub struct ScrcpyControl<W> {
  writer: W,
  buttons: u8,
}

pub struct ScrcpyServerControl {
  process: Child,
  control: ScrcpyControl<TcpStream>,
  adb: String,
  serial: Option<String>,
  socket_name: String,
}

impl ScrcpyServerControl {
  pub fn start(
    adb: &str,
    serial: Option<&str>,
    scrcpy_binary: &str,
    server_path: Option<&str>,
    port: u16,
  ) -> Result<Self> {
    let server_path = resolve_server_path(scrcpy_binary, server_path)?;
    let version = scrcpy_version(scrcpy_binary)?;
    let scid = 0x4b564d31u32;
    let socket_name = format!("scrcpy_{scid:08x}");
    let device_server_path = "/data/local/tmp/scrcpy-server.jar";

    ensure_success(
      adb_command(adb, serial)
        .args([
          "push",
          server_path
            .to_str()
            .context("non-utf8 scrcpy server path")?,
          device_server_path,
        ])
        .status()
        .with_context(|| format!("failed to push scrcpy server with {adb}"))?,
      "adb push scrcpy-server",
    )?;

    let listener = TcpListener::bind(("127.0.0.1", port))
      .with_context(|| format!("failed to listen on 127.0.0.1:{port}"))?;
    listener
      .set_nonblocking(false)
      .context("failed to configure control listener")?;

    ensure_success(
      adb_command(adb, serial)
        .args([
          "reverse",
          &format!("localabstract:{socket_name}"),
          &format!("tcp:{port}"),
        ])
        .status()
        .with_context(|| format!("failed to create adb reverse tunnel with {adb}"))?,
      "adb reverse scrcpy control socket",
    )?;

    let process = adb_command(adb, serial)
      .args([
        "shell",
        &format!("CLASSPATH={device_server_path}"),
        "app_process",
        "/",
        "com.genymobile.scrcpy.Server",
        &version,
        &format!("scid={scid:08x}"),
        "log_level=info",
        "video=false",
        "audio=false",
        "control=true",
        "send_dummy_byte=false",
        "send_device_meta=false",
        "send_frame_meta=false",
        "clipboard_autosync=false",
        "cleanup=false",
        "power_on=false",
      ])
      .spawn()
      .with_context(|| format!("failed to start scrcpy server with {adb}"))?;

    let (stream, _) = listener
      .accept()
      .context("failed to accept scrcpy control connection")?;
    stream
      .set_nodelay(true)
      .context("failed to set TCP_NODELAY on control socket")?;
    stream
      .set_write_timeout(Some(Duration::from_secs(2)))
      .context("failed to configure control socket timeout")?;

    let mut control = ScrcpyControl::new(stream);
    control
      .create_mouse()
      .context("failed to create UHID mouse")?;

    Ok(Self {
      process,
      control,
      adb: adb.to_string(),
      serial: serial.map(str::to_string),
      socket_name,
    })
  }

  pub fn move_mouse(&mut self, dx: i32, dy: i32) -> Result<()> {
    self
      .control
      .move_mouse(dx, dy)
      .context("failed to send UHID mouse motion")
  }

  pub fn stop(&mut self) -> Result<()> {
    self.control.destroy_mouse().ok();
    self.process.kill().ok();
    self.process.wait().ok();
    adb_command(&self.adb, self.serial.as_deref())
      .args([
        "reverse",
        "--remove",
        &format!("localabstract:{}", self.socket_name),
      ])
      .status()
      .ok();
    Ok(())
  }
}

impl Drop for ScrcpyServerControl {
  fn drop(&mut self) {
    let _ = self.stop();
  }
}

impl<W: Write> ScrcpyControl<W> {
  pub fn new(writer: W) -> Self {
    Self { writer, buttons: 0 }
  }

  pub fn create_mouse(&mut self) -> io::Result<()> {
    let mut msg = Vec::with_capacity(1 + 2 + 2 + 2 + 1 + 2 + MOUSE_REPORT_DESC.len());
    msg.push(TYPE_UHID_CREATE);
    write_u16(&mut msg, HID_ID_MOUSE);
    write_u16(&mut msg, 0);
    write_u16(&mut msg, 0);
    msg.push(0);
    write_u16(&mut msg, MOUSE_REPORT_DESC.len() as u16);
    msg.extend_from_slice(MOUSE_REPORT_DESC);
    self.writer.write_all(&msg)
  }

  pub fn move_mouse(&mut self, dx: i32, dy: i32) -> io::Result<()> {
    for (dx, dy) in split_motion(dx, dy) {
      self.input([self.buttons, dx as u8, dy as u8, 0, 0])?;
    }
    Ok(())
  }

  pub fn destroy_mouse(&mut self) -> io::Result<()> {
    let mut msg = Vec::with_capacity(3);
    msg.push(TYPE_UHID_DESTROY);
    write_u16(&mut msg, HID_ID_MOUSE);
    self.writer.write_all(&msg)
  }

  fn input(&mut self, data: [u8; 5]) -> io::Result<()> {
    let mut msg = Vec::with_capacity(10);
    msg.push(TYPE_UHID_INPUT);
    write_u16(&mut msg, HID_ID_MOUSE);
    write_u16(&mut msg, data.len() as u16);
    msg.extend_from_slice(&data);
    self.writer.write_all(&msg)
  }
}

fn split_motion(dx: i32, dy: i32) -> impl Iterator<Item = (i8, i8)> {
  let steps = ((dx.abs().max(dy.abs()) + 126) / 127).max(1);
  let mut emitted = 0;

  std::iter::from_fn(move || {
    if emitted >= steps {
      return None;
    }

    let remaining = steps - emitted;
    let x = (dx - emitted * dx / steps) / remaining;
    let y = (dy - emitted * dy / steps) / remaining;
    emitted += 1;
    Some((x.clamp(-127, 127) as i8, y.clamp(-127, 127) as i8))
  })
}

fn write_u16(buf: &mut Vec<u8>, value: u16) {
  buf.extend_from_slice(&value.to_be_bytes());
}

fn adb_command(adb: &str, serial: Option<&str>) -> Command {
  let mut command = Command::new(adb);
  if let Some(serial) = serial {
    command.args(["-s", serial]);
  }
  command
}

fn ensure_success(status: std::process::ExitStatus, label: &str) -> Result<()> {
  if status.success() {
    return Ok(());
  }

  bail!("{label} exited with {status}")
}

fn resolve_server_path(scrcpy_binary: &str, configured: Option<&str>) -> Result<PathBuf> {
  if let Some(path) = configured {
    return Ok(PathBuf::from(path));
  }

  if let Ok(path) = std::env::var("SCRCPY_SERVER_PATH") {
    return Ok(PathBuf::from(path));
  }

  let binary = which(scrcpy_binary)?;
  let Some(prefix) = binary.parent().and_then(|bin| bin.parent()) else {
    bail!("failed to infer scrcpy prefix from {}", binary.display());
  };

  Ok(prefix.join("share/scrcpy/scrcpy-server"))
}

fn scrcpy_version(scrcpy_binary: &str) -> Result<String> {
  let output = Command::new(scrcpy_binary)
    .arg("--version")
    .output()
    .with_context(|| format!("failed to execute {scrcpy_binary} --version"))?;
  if !output.status.success() {
    bail!("{scrcpy_binary} --version exited with {}", output.status);
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let first = stdout
    .lines()
    .next()
    .context("scrcpy --version printed no output")?;
  first
    .split_whitespace()
    .nth(1)
    .map(str::to_string)
    .context("failed to parse scrcpy version")
}

fn which(binary: &str) -> Result<PathBuf> {
  if binary.contains('/') {
    return Ok(PathBuf::from(binary));
  }
  let path = std::env::var_os("PATH").context("PATH is not set")?;
  for dir in std::env::split_paths(&path) {
    let candidate = dir.join(binary);
    if candidate.is_file() {
      return Ok(candidate);
    }
  }
  bail!("failed to find {binary} in PATH")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn serializes_mouse_create_message() {
    let mut out = Vec::new();
    ScrcpyControl::new(&mut out).create_mouse().unwrap();

    assert_eq!(out[0], TYPE_UHID_CREATE);
    assert_eq!(&out[1..3], &HID_ID_MOUSE.to_be_bytes());
    assert_eq!(&out[8..10], &(MOUSE_REPORT_DESC.len() as u16).to_be_bytes());
    assert_eq!(&out[10..], MOUSE_REPORT_DESC);
  }

  #[test]
  fn serializes_mouse_motion_input_message() {
    let mut out = Vec::new();
    ScrcpyControl::new(&mut out).move_mouse(5, -4).unwrap();

    assert_eq!(
      out,
      vec![
        TYPE_UHID_INPUT,
        0,
        HID_ID_MOUSE as u8,
        0,
        5,
        0,
        5,
        252,
        0,
        0
      ]
    );
  }

  #[test]
  fn splits_large_motion() {
    let parts = split_motion(300, -300).collect::<Vec<_>>();

    assert!(parts.len() > 1);
    assert_eq!(parts.iter().map(|(x, _)| *x as i32).sum::<i32>(), 300);
    assert_eq!(parts.iter().map(|(_, y)| *y as i32).sum::<i32>(), -300);
  }
}
