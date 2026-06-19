use std::process::Child;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::android::{AndroidBounds, AndroidInput};
use crate::config::Config;
use crate::edge::{Edge, EdgeSwitch, Focus, Pointer, ScreenBounds};
use crate::host::HostPointer;
use crate::scrcpy::ScrcpyBackend;

pub struct Runtime {
    config: Config,
    host: Box<dyn HostPointer>,
    backend: ScrcpyBackend,
}

impl Runtime {
    pub fn new(config: Config, host: Box<dyn HostPointer>) -> Self {
        let backend = ScrcpyBackend::new(config.scrcpy.clone());

        Self {
            config,
            host,
            backend,
        }
    }

    pub fn run(&self) -> Result<()> {
        let host_bounds = self.host.screen_bounds()?;
        let anchor = Pointer {
            x: host_bounds.width / 2,
            y: host_bounds.height / 2,
        };
        let interval = Duration::from_millis(self.config.poll_interval_ms.max(1));
        let mut switch = EdgeSwitch::new(
            self.config.android_edge,
            self.config.activation_pixels,
            self.config.release_pixels,
        );
        let mut active = None;
        let mut previous_focus = Focus::Host;

        loop {
            let focus = match active.as_mut() {
                Some(session) => self.update_android_focus(session, anchor)?,
                None => switch.update(host_bounds, self.host.pointer()?),
            };

            if focus != previous_focus {
                match focus {
                    Focus::Android => {
                        active = Some(self.start_android_focus(host_bounds, anchor)?);
                        println!("android focus active");
                    }
                    Focus::Host => {
                        self.stop_android_focus(&mut active, host_bounds)?;
                        println!("host focus active");
                    }
                }
                previous_focus = focus;
            }

            if child_exited(&mut active)? {
                self.host.end_capture()?;
                previous_focus = Focus::Host;
            }

            thread::sleep(interval);
        }
    }

    fn start_android_focus(
        &self,
        host_bounds: ScreenBounds,
        anchor: Pointer,
    ) -> Result<ActiveSession> {
        let child = self
            .backend
            .spawn()
            .context("failed to start scrcpy focus backend")?;
        let android = self.android_input()?;
        let pointer =
            android_entry_pointer(self.config.android_edge, android.bounds(), host_bounds);

        self.host.begin_capture(anchor)?;
        android.move_pointer(pointer)?;

        Ok(ActiveSession {
            child,
            android,
            pointer,
        })
    }

    fn stop_android_focus(
        &self,
        active: &mut Option<ActiveSession>,
        host_bounds: ScreenBounds,
    ) -> Result<()> {
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
        self.host.end_capture()?;
        self.host
            .warp_pointer(host_return_pointer(self.config.android_edge, host_bounds))?;
        Ok(())
    }

    fn update_android_focus(&self, session: &mut ActiveSession, anchor: Pointer) -> Result<Focus> {
        let pointer = self.host.pointer()?;
        let dx = ((pointer.x - anchor.x) as f32 * self.config.pointer_scale).round() as i32;
        let dy = ((pointer.y - anchor.y) as f32 * self.config.pointer_scale).round() as i32;

        if dx == 0 && dy == 0 {
            return Ok(Focus::Android);
        }

        self.host.warp_pointer(anchor)?;
        session.pointer.x += dx;
        session.pointer.y += dy;

        if should_release(
            self.config.android_edge,
            session.pointer,
            session.android.bounds(),
        ) {
            return Ok(Focus::Host);
        }

        session.android.move_pointer(session.pointer)?;
        Ok(Focus::Android)
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
    child: Child,
    android: AndroidInput,
    pointer: Pointer,
}

fn child_exited(active: &mut Option<ActiveSession>) -> Result<bool> {
    let Some(session) = active else {
        return Ok(false);
    };

    if session
        .child
        .try_wait()
        .context("failed to poll scrcpy focus backend")?
        .is_some()
    {
        *active = None;
        return Ok(true);
    }

    Ok(false)
}

fn android_entry_pointer(edge: Edge, android: AndroidBounds, _host: ScreenBounds) -> Pointer {
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

fn host_return_pointer(edge: Edge, host: ScreenBounds) -> Pointer {
    match edge {
        Edge::Left => Pointer {
            x: 1,
            y: host.height / 2,
        },
        Edge::Right => Pointer {
            x: host.width.saturating_sub(2),
            y: host.height / 2,
        },
        Edge::Top => Pointer {
            x: host.width / 2,
            y: 1,
        },
        Edge::Bottom => Pointer {
            x: host.width / 2,
            y: host.height.saturating_sub(2),
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
    fn right_edge_starts_on_android_left_side() {
        assert_eq!(
            android_entry_pointer(
                Edge::Right,
                AndroidBounds {
                    width: 1080,
                    height: 2400,
                },
                ScreenBounds {
                    width: 1920,
                    height: 1080,
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
