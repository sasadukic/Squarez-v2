// src/color/mod.rs
pub mod hsv;
pub mod oklab;
pub use hsv::{rgba_to_hsv, hsv_to_rgba};
pub use oklab::{
    rgba_to_oklab,
    oklab_to_rgba,
    rgba_to_oklch,
    oklch_to_rgba,
    safe_oklch_to_rgba,
    generate_ramp,
    generate_ramp_hsv,
    generate_ramp_endpoints,
    generate_ramp_hsv_endpoints,
};
use crate::project::Rgba;

/// All color picker state for the right panel
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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
    /// Endpoints mode: the L value (OKLCh) of the *light* end of the ramp.
    /// FG anchors the dark end; this anchors the light end.
    pub light_end_l: f32,
    /// Endpoints mode (HSV): the V value of the *light* end of the ramp.
    pub light_end_v: f32,
    /// Whether non-endpoint ramps push dark/light ends toward near black/white.
    pub ramp_end_extremes: bool,
    /// Ramp Lab fixed 3-point curves (start/mid/end handle y in 0..1).
    pub ramp_curve_start_luma: f32,
    pub ramp_curve_mid_luma: f32,
    pub ramp_curve_end_luma: f32,
    pub ramp_curve_start_sat: f32,
    pub ramp_curve_mid_sat: f32,
    pub ramp_curve_end_sat: f32,
    pub ramp_curve_start_hue: f32,
    pub ramp_curve_mid_hue: f32,
    pub ramp_curve_end_hue: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RampAnchor {
    /// FG sits at the middle index of the ramp.
    Middle,
    /// FG sits at index 2 (classic pixel-art "base/skin" step).
    BaseStep3,
    /// FG sits at index 0 (darkest endpoint). M3 will add a 2nd thumb for the light endpoint.
    Endpoints,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PickerMode { Hsv, OkLab, Rgb }

impl Default for ColorState {
    fn default() -> Self {
        Self {
            // Use a visible non-gray default for easier debugging & UX
            foreground: [0x11, 0xad, 0xc1, 255],
            background: [255, 255, 255, 255],
            active_picker: PickerMode::OkLab,
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
            light_end_l: 0.90,
            light_end_v: 0.95,
            ramp_end_extremes: false,
            ramp_curve_start_luma: 0.00,
            ramp_curve_mid_luma: 0.50,
            ramp_curve_end_luma: 1.00,
            ramp_curve_start_sat: 0.35,
            ramp_curve_mid_sat: 0.50,
            ramp_curve_end_sat: 0.80,
            ramp_curve_start_hue: 0.50,
            ramp_curve_mid_hue: 0.50,
            ramp_curve_end_hue: 0.50,
        }
    }
}
