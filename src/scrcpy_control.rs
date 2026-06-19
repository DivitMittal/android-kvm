use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use keycode::{KeyMap, KeyMapping, KeyState, KeyboardState};

use crate::scrcpy::shell_words;

const TYPE_UHID_CREATE: u8 = 12;
const TYPE_UHID_INPUT: u8 = 13;
const TYPE_UHID_DESTROY: u8 = 14;
const HID_ID_KEYBOARD: u16 = 1;
const HID_ID_MOUSE: u16 = 2;
const KEYBOARD_ROLLOVER: usize = 6;
const SERVER_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_ACCEPT_POLL: Duration = Duration::from_millis(25);

const MOUSE_REPORT_DESC: &[u8] = &[
  0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x05,
  0x15, 0x00, 0x25, 0x01, 0x95, 0x05, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x03, 0x81, 0x01,
  0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x03,
  0x81, 0x06, 0x05, 0x0C, 0x0A, 0x38, 0x02, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x01, 0x81,
  0x06, 0xC0, 0xC0,
];

const KEYBOARD_REPORT_DESC: &[u8] = &[
  0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01,
  0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x75, 0x08, 0x95, 0x01, 0x81, 0x01, 0x05, 0x08, 0x19, 0x01,
  0x29, 0x05, 0x75, 0x01, 0x95, 0x05, 0x91, 0x02, 0x75, 0x03, 0x95, 0x01, 0x91, 0x01, 0x05, 0x07,
  0x19, 0x00, 0x29, 0x65, 0x15, 0x00, 0x25, 0x65, 0x75, 0x08, 0x95, 0x06, 0x81, 0x00, 0xC0,
];

pub struct ScrcpyControl<W> {
  writer: W,
  buttons: u8,
  keyboard: KeyboardState,
}

pub struct ScrcpyServerControl {
  process: Child,
  control: ScrcpyControl<TcpStream>,
  adb: String,
  serial: Option<String>,
  socket_name: String,
}

impl ScrcpyServerControl {
  pub fn command_preview(
    adb: &str,
    serial: Option<&str>,
    scrcpy_binary: &str,
    server_path: Option<&str>,
    port: u16,
  ) -> String {
    let device_server_path = "/data/local/tmp/scrcpy-server.jar";
    let host_server_path = server_path.unwrap_or("<scrcpy-server-from-scrcpy>");
    let control_port = if port == 0 {
      "<allocated-control-port>".to_string()
    } else {
      port.to_string()
    };
    let socket_name = "scrcpy_<random-scid>";

    [
      shell_words(adb_words(adb, serial).chain(["push", host_server_path, device_server_path])),
      shell_words(adb_words(adb, serial).chain([
        "reverse",
        &format!("localabstract:{socket_name}"),
        &format!("tcp:{control_port}"),
      ])),
      shell_words(adb_words(adb, serial).chain([
        "shell",
        &format!("CLASSPATH={device_server_path}"),
        "app_process",
        "/",
        "com.genymobile.scrcpy.Server",
        "<scrcpy-version>",
        "scid=<random-scid>",
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
      ])),
      format!("# scrcpy binary used for server discovery: {scrcpy_binary}"),
    ]
    .join("\n")
  }

  pub fn start(
    adb: &str,
    serial: Option<&str>,
    scrcpy_binary: &str,
    server_path: Option<&str>,
    port: u16,
  ) -> Result<Self> {
    let server_path = resolve_server_path(scrcpy_binary, server_path)?;
    let version = scrcpy_version(scrcpy_binary)?;
    let scid = random_scid()?;
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
    let control_port = listener
      .local_addr()
      .context("failed to read control listener address")?
      .port();
    listener
      .set_nonblocking(true)
      .context("failed to configure control listener")?;

    ensure_success(
      adb_command(adb, serial)
        .args([
          "reverse",
          &format!("localabstract:{socket_name}"),
          &format!("tcp:{control_port}"),
        ])
        .status()
        .with_context(|| format!("failed to create adb reverse tunnel with {adb}"))?,
      "adb reverse scrcpy control socket",
    )?;

    let mut process = adb_command(adb, serial)
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
      .stderr(Stdio::piped())
      .spawn()
      .with_context(|| format!("failed to start scrcpy server with {adb}"))?;

    let stream = accept_control_connection(&listener, &mut process)
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
    control
      .create_keyboard()
      .context("failed to create UHID keyboard")?;

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

  pub fn set_mouse_button(&mut self, button: MouseButton, pressed: bool) -> Result<()> {
    self
      .control
      .set_mouse_button(button, pressed)
      .context("failed to send UHID mouse button")
  }

  pub fn scroll_mouse(&mut self, hscroll: i32, vscroll: i32) -> Result<()> {
    self
      .control
      .scroll_mouse(hscroll, vscroll)
      .context("failed to send UHID mouse scroll")
  }

  pub fn set_key(&mut self, linux_key: u32, pressed: bool) -> Result<()> {
    self
      .control
      .set_key(linux_key, pressed)
      .context("failed to send UHID keyboard input")
  }

  pub fn stop(&mut self) -> Result<()> {
    self.control.destroy_keyboard().ok();
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
    Self {
      writer,
      buttons: 0,
      keyboard: KeyboardState::new(Some(KEYBOARD_ROLLOVER)),
    }
  }

  pub fn create_keyboard(&mut self) -> io::Result<()> {
    self.create_hid(HID_ID_KEYBOARD, KEYBOARD_REPORT_DESC)
  }

  pub fn create_mouse(&mut self) -> io::Result<()> {
    self.create_hid(HID_ID_MOUSE, MOUSE_REPORT_DESC)
  }

  fn create_hid(&mut self, id: u16, report_desc: &[u8]) -> io::Result<()> {
    let mut msg = Vec::with_capacity(1 + 2 + 2 + 2 + 1 + 2 + MOUSE_REPORT_DESC.len());
    msg.push(TYPE_UHID_CREATE);
    write_u16(&mut msg, id);
    write_u16(&mut msg, 0);
    write_u16(&mut msg, 0);
    msg.push(0);
    write_u16(&mut msg, report_desc.len() as u16);
    msg.extend_from_slice(report_desc);
    self.writer.write_all(&msg)
  }

  pub fn move_mouse(&mut self, dx: i32, dy: i32) -> io::Result<()> {
    for (dx, dy) in split_motion(dx, dy) {
      self.input([self.buttons, dx as u8, dy as u8, 0, 0])?;
    }
    Ok(())
  }

  pub fn set_mouse_button(&mut self, button: MouseButton, pressed: bool) -> io::Result<()> {
    if pressed {
      self.buttons |= button.bit();
    } else {
      self.buttons &= !button.bit();
    }

    self.input([self.buttons, 0, 0, 0, 0])
  }

  pub fn scroll_mouse(&mut self, hscroll: i32, vscroll: i32) -> io::Result<()> {
    for (hscroll, vscroll) in split_motion(hscroll, vscroll) {
      self.input([self.buttons, 0, 0, vscroll as u8, hscroll as u8])?;
    }
    Ok(())
  }

  pub fn destroy_mouse(&mut self) -> io::Result<()> {
    self.destroy_hid(HID_ID_MOUSE)
  }

  pub fn destroy_keyboard(&mut self) -> io::Result<()> {
    self.destroy_hid(HID_ID_KEYBOARD)
  }

  fn destroy_hid(&mut self, id: u16) -> io::Result<()> {
    let mut msg = Vec::with_capacity(3);
    msg.push(TYPE_UHID_DESTROY);
    write_u16(&mut msg, id);
    self.writer.write_all(&msg)
  }

  pub fn set_key(&mut self, linux_key: u32, pressed: bool) -> io::Result<()> {
    let Some(key) = keyboard_key(linux_key) else {
      return Ok(());
    };

    self.keyboard.update_key(
      key,
      if pressed {
        KeyState::Pressed
      } else {
        KeyState::Released
      },
    );

    self.keyboard_input()
  }

  fn keyboard_input(&mut self) -> io::Result<()> {
    let data = self.keyboard.usb_input_report().to_vec();
    self.hid_input(HID_ID_KEYBOARD, &data)
  }

  fn input(&mut self, data: [u8; 5]) -> io::Result<()> {
    self.hid_input(HID_ID_MOUSE, &data)
  }

  fn hid_input(&mut self, id: u16, data: &[u8]) -> io::Result<()> {
    let mut msg = Vec::with_capacity(10);
    msg.push(TYPE_UHID_INPUT);
    write_u16(&mut msg, id);
    write_u16(&mut msg, data.len() as u16);
    msg.extend_from_slice(data);
    self.writer.write_all(&msg)
  }
}

fn keyboard_key(linux_key: u32) -> Option<KeyMap> {
  let linux_key = u16::try_from(linux_key).ok()?;
  KeyMap::from_key_mapping(KeyMapping::Evdev(linux_key)).ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
  Left,
  Right,
  Middle,
  Back,
  Forward,
}

impl MouseButton {
  fn bit(self) -> u8 {
    match self {
      Self::Left => 1 << 0,
      Self::Right => 1 << 1,
      Self::Middle => 1 << 2,
      Self::Back => 1 << 3,
      Self::Forward => 1 << 4,
    }
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

fn adb_words<'a>(adb: &'a str, serial: Option<&'a str>) -> impl Iterator<Item = &'a str> {
  std::iter::once(adb).chain(serial.into_iter().flat_map(|serial| ["-s", serial]))
}

fn accept_control_connection(listener: &TcpListener, process: &mut Child) -> Result<TcpStream> {
  let deadline = Instant::now() + SERVER_CONNECT_TIMEOUT;

  loop {
    match listener.accept() {
      Ok((stream, _)) => return Ok(stream),
      Err(error) if error.kind() == ErrorKind::WouldBlock => {}
      Err(error) => return Err(error).context("control listener accept failed"),
    }

    if let Some(status) = process
      .try_wait()
      .context("failed to inspect scrcpy server process")?
    {
      let stderr = read_server_stderr(process);
      bail!("scrcpy server exited before opening control socket: {status}{stderr}");
    }

    if Instant::now() >= deadline {
      process.kill().ok();
      let status = process.wait().ok();
      let stderr = read_server_stderr(process);
      bail!(
        "timed out after {}s waiting for scrcpy control socket; server status: {}{stderr}",
        SERVER_CONNECT_TIMEOUT.as_secs(),
        format_server_status(status),
      );
    }

    std::thread::sleep(SERVER_ACCEPT_POLL);
  }
}

fn read_server_stderr(process: &mut Child) -> String {
  let Some(mut stderr) = process.stderr.take() else {
    return String::new();
  };
  let mut output = String::new();
  match stderr.read_to_string(&mut output) {
    Ok(_) if output.trim().is_empty() => String::new(),
    Ok(_) => format!("; stderr: {}", output.trim()),
    Err(error) => format!("; failed to read stderr: {error}"),
  }
}

fn format_server_status(status: Option<ExitStatus>) -> String {
  status.map_or_else(
    || "killed after timeout".to_string(),
    |status| status.to_string(),
  )
}

fn random_scid() -> Result<u32> {
  let mut bytes = [0u8; 4];
  getrandom::fill(&mut bytes)
    .map_err(|error| anyhow::anyhow!("failed to generate scrcpy scid: {error}"))?;
  let scid = u32::from_ne_bytes(bytes);
  Ok(scid.max(1))
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
  fn serializes_mouse_button_message() {
    let mut out = Vec::new();
    ScrcpyControl::new(&mut out)
      .set_mouse_button(MouseButton::Left, true)
      .unwrap();

    assert_eq!(
      out,
      vec![TYPE_UHID_INPUT, 0, HID_ID_MOUSE as u8, 0, 5, 1, 0, 0, 0, 0]
    );
  }

  #[test]
  fn serializes_keyboard_create_message() {
    let mut out = Vec::new();
    ScrcpyControl::new(&mut out).create_keyboard().unwrap();

    assert_eq!(out[0], TYPE_UHID_CREATE);
    assert_eq!(&out[1..3], &HID_ID_KEYBOARD.to_be_bytes());
    assert_eq!(
      &out[8..10],
      &(KEYBOARD_REPORT_DESC.len() as u16).to_be_bytes()
    );
    assert_eq!(&out[10..], KEYBOARD_REPORT_DESC);
  }

  #[test]
  fn serializes_keyboard_key_message() {
    let mut out = Vec::new();
    ScrcpyControl::new(&mut out).set_key(30, true).unwrap();

    assert_eq!(
      out,
      vec![
        TYPE_UHID_INPUT,
        0,
        HID_ID_KEYBOARD as u8,
        0,
        8,
        0,
        0,
        4,
        0,
        0,
        0,
        0,
        0
      ]
    );
  }

  #[test]
  fn maps_evdev_number_and_symbol_keys_to_usb_hid_usages() {
    let mut out = Vec::new();
    let mut control = ScrcpyControl::new(&mut out);

    control.set_key(2, true).unwrap();
    control.set_key(2, false).unwrap();
    control.set_key(12, true).unwrap();

    assert_eq!(keyboard_report_at(&out, 0), [0, 0, 0x1e, 0, 0, 0, 0, 0]);
    assert_eq!(keyboard_report_at(&out, 1), [0; 8]);
    assert_eq!(keyboard_report_at(&out, 2), [0, 0, 0x2d, 0, 0, 0, 0, 0]);
  }

  #[test]
  fn maps_evdev_modifiers_to_usb_hid_modifier_byte() {
    let mut out = Vec::new();
    let mut control = ScrcpyControl::new(&mut out);

    control.set_key(42, true).unwrap();
    control.set_key(30, true).unwrap();

    assert_eq!(keyboard_report_at(&out, 0), [0x02, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(keyboard_report_at(&out, 1), [0x02, 0, 0x04, 0, 0, 0, 0, 0]);
  }

  #[test]
  fn maps_esc_to_usb_hid_instead_of_release_shortcut() {
    let mut out = Vec::new();
    ScrcpyControl::new(&mut out).set_key(1, true).unwrap();

    assert_eq!(keyboard_report_at(&out, 0), [0, 0, 0x29, 0, 0, 0, 0, 0]);
  }

  #[test]
  fn splits_large_motion() {
    let parts = split_motion(300, -300).collect::<Vec<_>>();

    assert!(parts.len() > 1);
    assert_eq!(parts.iter().map(|(x, _)| *x as i32).sum::<i32>(), 300);
    assert_eq!(parts.iter().map(|(_, y)| *y as i32).sum::<i32>(), -300);
  }

  #[test]
  fn split_motion_keeps_exact_i8_boundaries_in_one_report() {
    assert_eq!(
      split_motion(127, -127).collect::<Vec<_>>(),
      vec![(127, -127)]
    );
  }

  #[test]
  fn split_motion_splits_just_past_i8_boundaries() {
    let positive = split_motion(128, 0).collect::<Vec<_>>();
    let negative = split_motion(-128, 0).collect::<Vec<_>>();

    assert_eq!(positive, vec![(64, 0), (64, 0)]);
    assert_eq!(positive.iter().map(|(x, _)| *x as i32).sum::<i32>(), 128);
    assert_eq!(negative, vec![(-64, 0), (-64, 0)]);
    assert_eq!(negative.iter().map(|(x, _)| *x as i32).sum::<i32>(), -128);
  }

  #[test]
  fn server_command_preview_uses_direct_app_process_control_path() {
    let preview = ScrcpyServerControl::command_preview(
      "adb-test",
      Some("device-1"),
      "scrcpy-test",
      Some("/tmp/scrcpy-server"),
      27183,
    );

    assert!(preview.contains("adb-test -s device-1 push /tmp/scrcpy-server"));
    assert!(
      preview
        .contains("adb-test -s device-1 reverse 'localabstract:scrcpy_<random-scid>' tcp:27183")
    );
    assert!(preview.contains("adb-test -s device-1 shell CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server"));
    assert!(preview.contains("# scrcpy binary used for server discovery: scrcpy-test"));
    assert!(!preview.contains("scrcpy --no-video"));
  }

  #[cfg(unix)]
  #[test]
  fn accept_control_connection_reports_server_exit_stderr() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut process = Command::new("sh")
      .args(["-c", "printf server-failed >&2; exit 7"])
      .stderr(Stdio::piped())
      .spawn()
      .unwrap();

    let error = accept_control_connection(&listener, &mut process).unwrap_err();
    let message = error.to_string();

    assert!(message.contains("scrcpy server exited before opening control socket"));
    assert!(message.contains("server-failed"));
  }

  fn keyboard_report_at(out: &[u8], index: usize) -> [u8; 8] {
    let start = index * 13;
    assert_eq!(out[start], TYPE_UHID_INPUT);
    assert_eq!(&out[start + 1..start + 3], &HID_ID_KEYBOARD.to_be_bytes());
    assert_eq!(&out[start + 3..start + 5], &(8u16).to_be_bytes());
    out[start + 5..start + 13].try_into().unwrap()
  }
}
