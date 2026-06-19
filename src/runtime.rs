use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use input_capture::{Backend, CaptureEvent, InputCapture, Position};
use input_event::{
  BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, Event, KeyboardEvent, PointerEvent,
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
    let mut pending_activation = None;
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
            CaptureEvent::Begin if active.is_none() && pending_activation.is_none() => {
              pending_activation = Some(SwipeActivation::new(
                self.config.android_edge,
                self.config.activation_pixels,
              ));
              println!(
                "android edge armed; swipe {:?} through the edge to activate",
                self.config.android_edge,
              );
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
              } else if let Some(activation) = pending_activation.as_mut() {
                match activation.update(pointer_event) {
                  SwipeActivationDecision::Activate => {
                    pending_activation = None;
                    active = Some(self.start_android_focus()?);
                    println!("android focus active");
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
                  SwipeActivationDecision::Release => {
                    pending_activation = None;
                    capture
                      .release()
                      .await
                      .context("failed to release capture")?;
                    println!("host focus active");
                  }
                  SwipeActivationDecision::Wait => {}
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
        _ = idle.tick(), if active.is_none() && pending_activation.is_none() => {
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

    Ok(ActiveSession {
      control,
      pointer: VirtualAndroidPointer::new(&self.config),
    })
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
        if session.pointer.update(dx, dy) {
          return Ok(true);
        }
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
    session: &mut ActiveSession,
    keyboard_event: KeyboardEvent,
  ) -> Result<bool> {
    if let KeyboardEvent::Key { key, state, .. } = keyboard_event {
      session.control.set_key(key, state != 0)?;
    }

    Ok(false)
  }
}

struct ActiveSession {
  control: ScrcpyServerControl,
  pointer: VirtualAndroidPointer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SwipeActivationDecision {
  Wait,
  Activate,
  Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SwipeActivation {
  edge: Edge,
  threshold: i32,
  outward_pixels: i32,
  inward_pixels: i32,
}

impl SwipeActivation {
  fn new(edge: Edge, activation_pixels: u32) -> Self {
    Self {
      edge,
      threshold: (activation_pixels as i32).max(1),
      outward_pixels: 0,
      inward_pixels: 0,
    }
  }

  fn update(&mut self, pointer_event: PointerEvent) -> SwipeActivationDecision {
    let PointerEvent::Motion { dx, dy, .. } = pointer_event else {
      return SwipeActivationDecision::Wait;
    };

    let motion = outward_motion(self.edge, dx, dy);
    if motion > 0 {
      self.outward_pixels += motion;
      self.inward_pixels = 0;
    } else if motion < 0 {
      self.inward_pixels += -motion;
      self.outward_pixels = 0;
    }

    if self.outward_pixels >= self.threshold {
      SwipeActivationDecision::Activate
    } else if self.inward_pixels >= self.threshold {
      SwipeActivationDecision::Release
    } else {
      SwipeActivationDecision::Wait
    }
  }
}

fn outward_motion(edge: Edge, dx: f64, dy: f64) -> i32 {
  let motion = match edge {
    Edge::Left => -dx,
    Edge::Right => dx,
    Edge::Top => -dy,
    Edge::Bottom => dy,
  };

  motion.round() as i32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VirtualAndroidBounds {
  width: i32,
  height: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VirtualAndroidPointer {
  edge: Edge,
  bounds: VirtualAndroidBounds,
  release_pixels: i32,
  x: i32,
  y: i32,
  release_armed: bool,
}

impl VirtualAndroidPointer {
  fn new(config: &Config) -> Self {
    let bounds = VirtualAndroidBounds {
      width: config.android_width.unwrap_or(1080).max(1),
      height: config.android_height.unwrap_or(2400).max(1),
    };
    let release_pixels = (config.release_pixels as i32).max(1);
    let (x, y) = entry_position(config.android_edge, bounds, release_pixels);

    Self {
      edge: config.android_edge,
      bounds,
      release_pixels,
      x,
      y,
      release_armed: false,
    }
  }

  fn update(&mut self, dx: i32, dy: i32) -> bool {
    self.x = (self.x + dx).clamp(0, self.bounds.width - 1);
    self.y = (self.y + dy).clamp(0, self.bounds.height - 1);

    if self.is_interior() {
      self.release_armed = true;
    }

    self.release_armed && self.is_at_host_edge()
  }

  fn is_interior(&self) -> bool {
    match self.edge {
      Edge::Left => self.x < self.bounds.width - self.release_pixels,
      Edge::Right => self.x >= self.release_pixels,
      Edge::Top => self.y < self.bounds.height - self.release_pixels,
      Edge::Bottom => self.y >= self.release_pixels,
    }
  }

  fn is_at_host_edge(&self) -> bool {
    match self.edge {
      Edge::Left => self.x >= self.bounds.width - self.release_pixels,
      Edge::Right => self.x < self.release_pixels,
      Edge::Top => self.y >= self.bounds.height - self.release_pixels,
      Edge::Bottom => self.y < self.release_pixels,
    }
  }
}

fn entry_position(edge: Edge, bounds: VirtualAndroidBounds, release_pixels: i32) -> (i32, i32) {
  match edge {
    Edge::Left => (bounds.width - release_pixels, bounds.height / 2),
    Edge::Right => (release_pixels - 1, bounds.height / 2),
    Edge::Top => (bounds.width / 2, bounds.height - release_pixels),
    Edge::Bottom => (bounds.width / 2, release_pixels - 1),
  }
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

  #[test]
  fn swipe_activation_waits_until_outward_threshold() {
    let mut activation = SwipeActivation::new(Edge::Right, 24);

    assert_eq!(
      activation.update(PointerEvent::Motion {
        time: 0,
        dx: 10.0,
        dy: 0.0,
      }),
      SwipeActivationDecision::Wait,
    );
    assert_eq!(
      activation.update(PointerEvent::Motion {
        time: 0,
        dx: 13.0,
        dy: 0.0,
      }),
      SwipeActivationDecision::Wait,
    );
    assert_eq!(
      activation.update(PointerEvent::Motion {
        time: 0,
        dx: 1.0,
        dy: 0.0,
      }),
      SwipeActivationDecision::Activate,
    );
  }

  #[test]
  fn swipe_activation_releases_when_user_moves_back_inward() {
    let mut activation = SwipeActivation::new(Edge::Right, 24);

    assert_eq!(
      activation.update(PointerEvent::Motion {
        time: 0,
        dx: 8.0,
        dy: 0.0,
      }),
      SwipeActivationDecision::Wait,
    );
    assert_eq!(
      activation.update(PointerEvent::Motion {
        time: 0,
        dx: -24.0,
        dy: 0.0,
      }),
      SwipeActivationDecision::Release,
    );
  }

  #[test]
  fn swipe_activation_uses_edge_direction() {
    let mut left = SwipeActivation::new(Edge::Left, 4);
    let mut top = SwipeActivation::new(Edge::Top, 4);
    let mut bottom = SwipeActivation::new(Edge::Bottom, 4);

    assert_eq!(
      left.update(PointerEvent::Motion {
        time: 0,
        dx: -4.0,
        dy: 0.0,
      }),
      SwipeActivationDecision::Activate,
    );
    assert_eq!(
      top.update(PointerEvent::Motion {
        time: 0,
        dx: 0.0,
        dy: -4.0,
      }),
      SwipeActivationDecision::Activate,
    );
    assert_eq!(
      bottom.update(PointerEvent::Motion {
        time: 0,
        dx: 0.0,
        dy: 4.0,
      }),
      SwipeActivationDecision::Activate,
    );
  }

  #[test]
  fn enters_android_near_opposite_side_of_host_edge() {
    let bounds = VirtualAndroidBounds {
      width: 100,
      height: 200,
    };

    assert_eq!(entry_position(Edge::Right, bounds, 4), (3, 100));
    assert_eq!(entry_position(Edge::Left, bounds, 4), (96, 100));
    assert_eq!(entry_position(Edge::Bottom, bounds, 4), (50, 3));
    assert_eq!(entry_position(Edge::Top, bounds, 4), (50, 196));
  }

  #[test]
  fn releases_from_android_right_edge_after_crossing_back_left() {
    let config = Config {
      android_edge: Edge::Right,
      android_width: Some(100),
      android_height: Some(200),
      release_pixels: 4,
      ..Config::default()
    };
    let mut pointer = VirtualAndroidPointer::new(&config);

    assert!(!pointer.update(20, 0));
    assert!(!pointer.update(-19, 0));
    assert!(pointer.update(-1, 0));
  }

  #[test]
  fn releases_from_android_left_edge_after_crossing_back_right() {
    let config = Config {
      android_edge: Edge::Left,
      android_width: Some(100),
      android_height: Some(200),
      release_pixels: 4,
      ..Config::default()
    };
    let mut pointer = VirtualAndroidPointer::new(&config);

    assert!(!pointer.update(-20, 0));
    assert!(!pointer.update(19, 0));
    assert!(pointer.update(1, 0));
  }

  #[test]
  fn releases_from_android_bottom_edge_after_crossing_back_up() {
    let config = Config {
      android_edge: Edge::Bottom,
      android_width: Some(100),
      android_height: Some(200),
      release_pixels: 4,
      ..Config::default()
    };
    let mut pointer = VirtualAndroidPointer::new(&config);

    assert!(!pointer.update(0, 20));
    assert!(!pointer.update(0, -19));
    assert!(pointer.update(0, -1));
  }

  #[test]
  fn releases_from_android_top_edge_after_crossing_back_down() {
    let config = Config {
      android_edge: Edge::Top,
      android_width: Some(100),
      android_height: Some(200),
      release_pixels: 4,
      ..Config::default()
    };
    let mut pointer = VirtualAndroidPointer::new(&config);

    assert!(!pointer.update(0, -20));
    assert!(!pointer.update(0, 19));
    assert!(pointer.update(0, 1));
  }

  #[test]
  fn does_not_release_before_pointer_enters_android_interior() {
    let config = Config {
      android_edge: Edge::Right,
      android_width: Some(100),
      android_height: Some(200),
      release_pixels: 4,
      ..Config::default()
    };
    let mut pointer = VirtualAndroidPointer::new(&config);

    assert!(!pointer.update(-10, 0));
    assert!(!pointer.update(0, 0));
  }
}
