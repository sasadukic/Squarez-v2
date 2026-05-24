// src/color/mod.rs
pub mod hsv;
pub mod oklab;
pub use hsv::{rgba_to_hsv, hsv_to_rgba};
pub use oklab::{rgba_to_oklab, oklab_to_rgba, rgba_to_oklch, oklch_to_rgba, safe_oklch_to_rgba};
use crate::project::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PickerMode { Hsv, OkLab, Rgb }

/// All color picker state for the right panel
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ColorState {
    pub foreground: Rgba,
    pub background: Rgba,
    pub active_picker: PickerMode,
    // Target caches for each mode (source of truth, active mode cache is not overwritten by fg)
    pub oklch_l: f32,
    pub oklch_c: f32,
    pub oklch_h: f32,
    pub hsv_h: f32,
    pub hsv_s: f32,
    pub hsv_v: f32,
    pub rgb_r: f32,
    pub rgb_g: f32,
    pub rgb_b: f32,
    // Displayed values (smoothly follow targets for tab-switch tween)
    pub display_oklch_l: f32,
    pub display_oklch_c: f32,
    pub display_oklch_h: f32,
    pub display_hsv_h: f32,
    pub display_hsv_s: f32,
    pub display_hsv_v: f32,
    pub display_rgb_r: f32,
    pub display_rgb_g: f32,
    pub display_rgb_b: f32,
}

impl Default for ColorState {
    fn default() -> Self {
        Self {
            foreground: [0x11, 0xad, 0xc1, 255],
            background: [255, 255, 255, 255],
            active_picker: PickerMode::OkLab,
            oklch_l: 0.7,
            oklch_c: 0.15,
            oklch_h: 210.0,
            hsv_h: 187.0,
            hsv_s: 0.85,
            hsv_v: 0.76,
            rgb_r: 17.0,
            rgb_g: 173.0,
            rgb_b: 193.0,
            display_oklch_l: 0.7,
            display_oklch_c: 0.15,
            display_oklch_h: 210.0,
            display_hsv_h: 187.0,
            display_hsv_s: 0.85,
            display_hsv_v: 0.76,
            display_rgb_r: 17.0,
            display_rgb_g: 173.0,
            display_rgb_b: 193.0,
        }
    }
}
