use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use input_capture::{Backend, CaptureEvent, InputCapture, Position};
use input_event::{
  BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Event, KeyboardEvent, PointerEvent,
  scancode,
};

use crate::config::Config;
use crate::edge::Edge;
use crate::scrcpy_control::{MouseButton, ScrcpyServerControl};

const ANDROID_CAPTURE_HANDLE: u64 = 1;

pub struct Runtime {
  config: Config,
}

impl Runtime {
  pub fn new(config: Config) -> Self {
    Self { config }
  }

  pub fn run(&self) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .context("failed to create runtime")?;
    let local = tokio::task::LocalSet::new();

    local.block_on(&runtime, self.run_async())
  }

  async fn run_async(&self) -> Result<()> {
    let mut capture = InputCapture::new(default_capture_backend())
      .await
      .context("failed to create lan-mouse input capture backend")?;
    capture
      .create(
        ANDROID_CAPTURE_HANDLE,
        to_capture_position(self.config.android_edge),
      )
      .await
      .context("failed to create Android edge capture")?;

    println!("watching {:?} edge for Android", self.config.android_edge);

    let mut active = None;
    let mut idle = tokio::time::interval(Duration::from_secs(5));

    loop {
      tokio::select! {
        event = capture.next() => {
          let Some(event) = event else {
            break;
          };
          let (handle, event) = event.context("input capture failed")?;
          if handle != ANDROID_CAPTURE_HANDLE {
            continue;
          }

          match event {
            CaptureEvent::Begin if active.is_none() => {
              active = Some(self.start_android_focus()?);
              println!("android focus active");
            }
            CaptureEvent::Input(Event::Pointer(pointer_event)) => {
              if let Some(session) = active.as_mut() {
                if self.handle_pointer_event(session, pointer_event)? {
                  self.stop_android_focus(&mut active)?;
                  capture
                    .release()
                    .await
                    .context("failed to release capture")?;
                  println!("host focus active");
                }
              }
            }
            CaptureEvent::Input(Event::Keyboard(keyboard_event)) => {
              if let Some(session) = active.as_mut() {
                if self.handle_keyboard_event(session, keyboard_event)? {
                  self.stop_android_focus(&mut active)?;
                  capture
                    .release()
                    .await
                    .context("failed to release capture")?;
                  println!("host focus active");
                }
              }
            }
            CaptureEvent::Begin => {}
          }
        }
        _ = idle.tick(), if active.is_none() => {
          println!(
            "waiting for {:?} edge capture; if crossing this edge does nothing, check macOS Accessibility/Input Monitoring permissions and lan-mouse edge conflicts",
            self.config.android_edge,
          );
        }
      }
    }

    Ok(())
  }

  fn start_android_focus(&self) -> Result<ActiveSession> {
    let control = ScrcpyServerControl::start(
      &self.config.adb_binary,
      self.config.scrcpy.serial.as_deref(),
      &self.config.scrcpy.binary,
      self.config.scrcpy_server_path.as_deref(),
      self.config.control_port,
    )?;

    Ok(ActiveSession { control })
  }

  fn stop_android_focus(&self, active: &mut Option<ActiveSession>) -> Result<()> {
    let Some(mut session) = active.take() else {
      return Ok(());
    };

    session.control.stop()?;
    Ok(())
  }

  fn handle_pointer_event(
    &self,
    session: &mut ActiveSession,
    pointer_event: PointerEvent,
  ) -> Result<bool> {
    match pointer_event {
      PointerEvent::Motion { dx, dy, .. } => {
        let dx = (dx as f32 * self.config.pointer_scale).round() as i32;
        let dy = (dy as f32 * self.config.pointer_scale).round() as i32;
        session.control.move_mouse(dx, dy)?;
      }
      PointerEvent::Button { button, state, .. } => {
        if let Some(button) = to_mouse_button(button) {
          session.control.set_mouse_button(button, state != 0)?;
        }
      }
      PointerEvent::Axis { axis, value, .. } => {
        let amount = value.round() as i32;
        match axis {
          0 => session.control.scroll_mouse(0, amount)?,
          1 => session.control.scroll_mouse(amount, 0)?,
          _ => {}
        }
      }
      PointerEvent::AxisDiscrete120 { axis, value } => {
        let amount = value / 120;
        match axis {
          0 => session.control.scroll_mouse(0, amount)?,
          1 => session.control.scroll_mouse(amount, 0)?,
          _ => {}
        }
      }
    }

    Ok(false)
  }

  fn handle_keyboard_event(
    &self,
    _session: &mut ActiveSession,
    keyboard_event: KeyboardEvent,
  ) -> Result<bool> {
    if let KeyboardEvent::Key { key, state, .. } = keyboard_event {
      if state != 0 && key == scancode::Linux::KeyEsc as u32 {
        return Ok(true);
      }

      _session.control.set_key(key, state != 0)?;
    }

    Ok(false)
  }
}

struct ActiveSession {
  control: ScrcpyServerControl,
}

fn default_capture_backend() -> Option<Backend> {
  #[cfg(target_os = "macos")]
  {
    Some(Backend::MacOs)
  }

  #[cfg(windows)]
  {
    Some(Backend::Windows)
  }

  #[cfg(all(unix, not(target_os = "macos")))]
  {
    None
  }
}

fn to_capture_position(edge: Edge) -> Position {
  match edge {
    Edge::Left => Position::Left,
    Edge::Right => Position::Right,
    Edge::Top => Position::Top,
    Edge::Bottom => Position::Bottom,
  }
}

fn to_mouse_button(button: u32) -> Option<MouseButton> {
  match button {
    BTN_LEFT => Some(MouseButton::Left),
    BTN_RIGHT => Some(MouseButton::Right),
    BTN_MIDDLE => Some(MouseButton::Middle),
    BTN_BACK => Some(MouseButton::Back),
    BTN_FORWARD => Some(MouseButton::Forward),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn maps_edges_to_capture_positions() {
    assert_eq!(to_capture_position(Edge::Left), Position::Left);
    assert_eq!(to_capture_position(Edge::Right), Position::Right);
    assert_eq!(to_capture_position(Edge::Top), Position::Top);
    assert_eq!(to_capture_position(Edge::Bottom), Position::Bottom);
  }

  #[test]
  fn maps_pointer_buttons_to_hid_buttons() {
    assert_eq!(to_mouse_button(BTN_LEFT), Some(MouseButton::Left));
    assert_eq!(to_mouse_button(BTN_RIGHT), Some(MouseButton::Right));
    assert_eq!(to_mouse_button(BTN_MIDDLE), Some(MouseButton::Middle));
  }
}
