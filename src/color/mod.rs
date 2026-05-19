// src/color/mod.rs
pub mod hsv;
pub mod oklab;
pub use hsv::{rgba_to_hsv, hsv_to_rgba};
pub use oklab::{rgba_to_oklab, oklab_to_rgba};
use crate::project::Rgba;

/// All color picker state for the right panel
#[derive(Debug, Clone)]
pub struct ColorState {
    pub foreground: Rgba,
    pub background: Rgba,
    pub active_picker: PickerMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerMode { Hsv, OkLab }

impl Default for ColorState {
    fn default() -> Self {
        Self {
            foreground: [0, 0, 0, 255],
            background: [255, 255, 255, 255],
            active_picker: PickerMode::Hsv,
        }
    }
}
