use anyhow::Result;

use crate::edge::{Pointer, ScreenBounds};

pub trait HostPointer {
    fn screen_bounds(&self) -> Result<ScreenBounds>;
    fn pointer(&self) -> Result<Pointer>;
    fn begin_capture(&self, anchor: Pointer) -> Result<()>;
    fn end_capture(&self) -> Result<()>;
    fn warp_pointer(&self, pointer: Pointer) -> Result<()>;
}

pub fn default_host_pointer() -> Result<Box<dyn HostPointer>> {
    platform::default_host_pointer()
}

#[cfg(target_os = "macos")]
mod platform {
    use anyhow::{Result, anyhow};
    use core_graphics::display::CGDisplay;
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    use super::HostPointer;
    use crate::edge::{Pointer, ScreenBounds};

    pub struct MacHostPointer;

    pub fn default_host_pointer() -> Result<Box<dyn HostPointer>> {
        Ok(Box::new(MacHostPointer))
    }

    impl HostPointer for MacHostPointer {
        fn screen_bounds(&self) -> Result<ScreenBounds> {
            let bounds = CGDisplay::main().bounds();
            Ok(ScreenBounds {
                width: bounds.size.width.round() as i32,
                height: bounds.size.height.round() as i32,
            })
        }

        fn pointer(&self) -> Result<Pointer> {
            let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                .map_err(|()| anyhow!("failed to create CoreGraphics event source"))?;
            let event =
                CGEvent::new(source).map_err(|()| anyhow!("failed to read pointer location"))?;
            let location = event.location();

            Ok(Pointer {
                x: location.x.round() as i32,
                y: location.y.round() as i32,
            })
        }

        fn begin_capture(&self, anchor: Pointer) -> Result<()> {
            CGDisplay::main()
                .hide_cursor()
                .map_err(|err| anyhow!("failed to hide macOS cursor: {err}"))?;
            self.warp_pointer(anchor)
        }

        fn end_capture(&self) -> Result<()> {
            CGDisplay::main()
                .show_cursor()
                .map_err(|err| anyhow!("failed to show macOS cursor: {err}"))
        }

        fn warp_pointer(&self, pointer: Pointer) -> Result<()> {
            CGDisplay::warp_mouse_cursor_position(CGPoint::new(pointer.x as f64, pointer.y as f64))
                .map_err(|err| anyhow!("failed to warp macOS cursor: {err}"))
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use anyhow::{Result, bail};

    use super::HostPointer;

    pub fn default_host_pointer() -> Result<Box<dyn HostPointer>> {
        bail!("host pointer backend is currently implemented only for macOS")
    }
}
