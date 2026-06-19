use std::process::Child;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::edge::{EdgeSwitch, Focus};
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
        let bounds = self.host.screen_bounds()?;
        let interval = Duration::from_millis(self.config.poll_interval_ms.max(1));
        let mut switch = EdgeSwitch::new(
            self.config.android_edge,
            self.config.activation_pixels,
            self.config.release_pixels,
        );
        let mut active = None;
        let mut previous_focus = Focus::Host;

        loop {
            let pointer = self.host.pointer()?;
            let focus = switch.update(bounds, pointer);

            if focus != previous_focus {
                match focus {
                    Focus::Android => {
                        active = Some(self.start_android_focus()?);
                        println!("android focus active");
                    }
                    Focus::Host => {
                        stop_android_focus(&mut active)?;
                        println!("host focus active");
                    }
                }
                previous_focus = focus;
            }

            child_exited(&mut active)?;

            thread::sleep(interval);
        }
    }

    fn start_android_focus(&self) -> Result<Child> {
        self.backend
            .spawn()
            .context("failed to start scrcpy focus backend")
    }
}

fn stop_android_focus(active: &mut Option<Child>) -> Result<()> {
    let Some(mut child) = active.take() else {
        return Ok(());
    };

    child
        .kill()
        .context("failed to stop scrcpy focus backend")?;
    child
        .wait()
        .context("failed to wait for scrcpy focus backend")?;
    Ok(())
}

fn child_exited(active: &mut Option<Child>) -> Result<bool> {
    let Some(child) = active else {
        return Ok(false);
    };

    if child
        .try_wait()
        .context("failed to poll scrcpy focus backend")?
        .is_some()
    {
        *active = None;
        return Ok(true);
    }

    Ok(false)
}
