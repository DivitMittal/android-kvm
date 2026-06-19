use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
  Left,
  Right,
  Top,
  Bottom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
  Host,
  Android,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenBounds {
  pub width: i32,
  pub height: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pointer {
  pub x: i32,
  pub y: i32,
}

#[derive(Clone, Debug)]
pub struct EdgeSwitch {
  edge: Edge,
  activation_pixels: i32,
  release_pixels: i32,
  focus: Focus,
}

impl EdgeSwitch {
  pub fn new(edge: Edge, activation_pixels: u32, release_pixels: u32) -> Self {
    Self {
      edge,
      activation_pixels: activation_pixels as i32,
      release_pixels: release_pixels as i32,
      focus: Focus::Host,
    }
  }

  pub fn update(&mut self, bounds: ScreenBounds, pointer: Pointer) -> Focus {
    self.focus = match self.focus {
      Focus::Host if self.at_activation_edge(bounds, pointer) => Focus::Android,
      Focus::Android if self.at_release_edge(bounds, pointer) => Focus::Host,
      focus => focus,
    };

    self.focus
  }

  fn at_activation_edge(&self, bounds: ScreenBounds, pointer: Pointer) -> bool {
    match self.edge {
      Edge::Left => pointer.x <= self.activation_pixels,
      Edge::Right => pointer.x >= bounds.width - self.activation_pixels,
      Edge::Top => pointer.y <= self.activation_pixels,
      Edge::Bottom => pointer.y >= bounds.height - self.activation_pixels,
    }
  }

  fn at_release_edge(&self, bounds: ScreenBounds, pointer: Pointer) -> bool {
    match self.edge {
      Edge::Left => pointer.x >= self.release_pixels,
      Edge::Right => pointer.x <= bounds.width - self.release_pixels,
      Edge::Top => pointer.y >= self.release_pixels,
      Edge::Bottom => pointer.y <= bounds.height - self.release_pixels,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn switches_to_android_at_right_edge() {
    let mut switch = EdgeSwitch::new(Edge::Right, 1, 4);

    assert_eq!(
      switch.update(
        ScreenBounds {
          width: 1920,
          height: 1080,
        },
        Pointer { x: 1919, y: 500 },
      ),
      Focus::Android,
    );
  }

  #[test]
  fn releases_host_focus_after_moving_back_from_right_edge() {
    let mut switch = EdgeSwitch::new(Edge::Right, 1, 4);
    let bounds = ScreenBounds {
      width: 1920,
      height: 1080,
    };

    switch.update(bounds, Pointer { x: 1919, y: 500 });

    assert_eq!(
      switch.update(bounds, Pointer { x: 1915, y: 500 }),
      Focus::Host,
    );
  }
}
