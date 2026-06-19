use anyhow::{Context, Result};
use futures_util::StreamExt;
use input_capture::{Backend, CaptureEvent, InputCapture, Position};
use input_event::{Event, PointerEvent};

use crate::android::{AndroidBounds, AndroidInput};
use crate::config::Config;
use crate::edge::{Edge, Pointer};
use crate::scrcpy::ScrcpyBackend;

const ANDROID_CAPTURE_HANDLE: u64 = 1;

pub struct Runtime {
    config: Config,
    backend: ScrcpyBackend,
}

impl Runtime {
    pub fn new(config: Config) -> Self {
        let backend = ScrcpyBackend::new(config.scrcpy.clone());

        Self { config, backend }
    }

    pub fn run(&self) -> Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to create runtime")?
            .block_on(self.run_async())
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

        while let Some(event) = capture.next().await {
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
                CaptureEvent::Input(Event::Keyboard(_)) => {
                    // Keyboard forwarding needs a scrcpy-control or ADB key map backend.
                }
                CaptureEvent::Begin => {}
            }
        }

        Ok(())
    }

    fn start_android_focus(&self) -> Result<ActiveSession> {
        let child = self
            .backend
            .spawn()
            .context("failed to start scrcpy focus backend")?;
        let android = self.android_input()?;
        let pointer = android_entry_pointer(self.config.android_edge, android.bounds());

        android.move_pointer(pointer)?;

        Ok(ActiveSession {
            child,
            android,
            pointer,
        })
    }

    fn stop_android_focus(&self, active: &mut Option<ActiveSession>) -> Result<()> {
        let Some(mut session) = active.take() else {
            return Ok(());
        };

        session
            .child
            .kill()
            .context("failed to stop scrcpy focus backend")?;
        session
            .child
            .wait()
            .context("failed to wait for scrcpy focus backend")?;
        Ok(())
    }

    fn handle_pointer_event(
        &self,
        session: &mut ActiveSession,
        pointer_event: PointerEvent,
    ) -> Result<bool> {
        match pointer_event {
            PointerEvent::Motion { dx, dy, .. } => {
                session.pointer.x += (dx as f32 * self.config.pointer_scale).round() as i32;
                session.pointer.y += (dy as f32 * self.config.pointer_scale).round() as i32;

                if should_release(
                    self.config.android_edge,
                    session.pointer,
                    session.android.bounds(),
                ) {
                    return Ok(true);
                }

                session.android.move_pointer(session.pointer)?;
            }
            PointerEvent::Button { .. }
            | PointerEvent::Axis { .. }
            | PointerEvent::AxisDiscrete120 { .. } => {
                // TODO: forward buttons/scroll through the Android input backend.
            }
        }

        Ok(false)
    }

    fn android_input(&self) -> Result<AndroidInput> {
        let bounds = match (self.config.android_width, self.config.android_height) {
            (Some(width), Some(height)) => AndroidBounds { width, height },
            _ => AndroidInput::detect_bounds(
                &self.config.adb_binary,
                self.config.scrcpy.serial.as_deref(),
            )?,
        };

        Ok(AndroidInput::new(
            self.config.adb_binary.clone(),
            self.config.scrcpy.serial.clone(),
            bounds,
        ))
    }
}

struct ActiveSession {
    child: std::process::Child,
    android: AndroidInput,
    pointer: Pointer,
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

fn android_entry_pointer(edge: Edge, android: AndroidBounds) -> Pointer {
    match edge {
        Edge::Left => Pointer {
            x: android.width.saturating_sub(1),
            y: android.height / 2,
        },
        Edge::Right => Pointer {
            x: 0,
            y: android.height / 2,
        },
        Edge::Top => Pointer {
            x: android.width / 2,
            y: android.height.saturating_sub(1),
        },
        Edge::Bottom => Pointer {
            x: android.width / 2,
            y: 0,
        },
    }
}

fn should_release(edge: Edge, pointer: Pointer, android: AndroidBounds) -> bool {
    match edge {
        Edge::Left => pointer.x >= android.width,
        Edge::Right => pointer.x < 0,
        Edge::Top => pointer.y >= android.height,
        Edge::Bottom => pointer.y < 0,
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
    fn right_edge_starts_on_android_left_side() {
        assert_eq!(
            android_entry_pointer(
                Edge::Right,
                AndroidBounds {
                    width: 1080,
                    height: 2400,
                },
            ),
            Pointer { x: 0, y: 1200 },
        );
    }

    #[test]
    fn right_edge_releases_when_moving_past_android_left_side() {
        assert!(should_release(
            Edge::Right,
            Pointer { x: -1, y: 1200 },
            AndroidBounds {
                width: 1080,
                height: 2400,
            },
        ));
    }
}
