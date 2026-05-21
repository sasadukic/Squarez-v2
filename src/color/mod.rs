// src/color/mod.rs
pub mod hsv;
pub mod oklab;
pub use hsv::{rgba_to_hsv, hsv_to_rgba};
pub use oklab::{rgba_to_oklab, oklab_to_rgba, rgba_to_oklch, oklch_to_rgba, generate_ramp, generate_ramp_hsv};
use crate::project::Rgba;

/// All color picker state for the right panel
#[derive(Debug, Clone)]
pub struct ColorState {
    pub foreground: Rgba,
    pub background: Rgba,
    pub active_picker: PickerMode,
    /// Number of color stops in a ramp (3..=9). User sets via right-click on Color section header.
    pub ramp_size: usize,
    /// Whether quantized snapping is on for each OKLCh channel.
    pub snap_oklch_l: bool,
    pub snap_oklch_c: bool,
    pub snap_oklch_h: bool,
    /// Last non-zero hue, kept so the H slider doesn't jump to 0 when C drops to 0 (achromatic).
    pub last_oklch_h: f32,
    /// Which step the current FG color anchors in the generated ramp.
    pub ramp_anchor: RampAnchor,
    /// How many degrees the H drifts across the ramp (total span). Default 16°.
    pub hue_shift_deg: f32,
    /// Bell curve depth for saturation/chroma — 0 = flat, 0.5 = strong dip at endpoints. Default 0.30.
    pub sat_curve_depth: f32,
    /// Snap toggles for HSV channels.
    pub snap_hsv_h: bool,
    pub snap_hsv_s: bool,
    pub snap_hsv_v: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RampAnchor {
    /// FG sits at the middle index of the ramp.
    Middle,
    /// FG sits at index 2 (classic pixel-art "base/skin" step).
    BaseStep3,
    /// FG sits at index 0 (darkest endpoint). M3 will add a 2nd thumb for the light endpoint.
    Endpoints,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerMode { Hsv, OkLab, Rgb }

impl Default for ColorState {
    fn default() -> Self {
        Self {
            foreground: [0, 0, 0, 255],
            background: [255, 255, 255, 255],
            active_picker: PickerMode::Hsv,
            ramp_size: 5,
            snap_oklch_l: false,
            snap_oklch_c: false,
            snap_oklch_h: false,
            last_oklch_h: 0.0,
            ramp_anchor: RampAnchor::Middle,
            hue_shift_deg: 16.0,
            sat_curve_depth: 0.30,
            snap_hsv_h: false,
            snap_hsv_s: false,
            snap_hsv_v: false,
        }
    }
}
