// src/color/oklab.rs
use crate::project::Rgba;
use crate::color::RampAnchor;

/// Returns OKLab (L: 0-1, a: -0.5..0.5, b: -0.5..0.5)
pub fn rgba_to_oklab(color: Rgba) -> (f32, f32, f32) {
    let r = srgb_to_linear(color[0]);
    let g = srgb_to_linear(color[1]);
    let b = srgb_to_linear(color[2]);
    // sRGB → OKLab
    let lc = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let mc = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let sc = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let l = 0.2104542553 * lc + 0.7936177850 * mc - 0.0040720468 * sc;
    let a = 1.9779984951 * lc - 2.4285922050 * mc + 0.4505937099 * sc;
    let b_out = 0.0259040371 * lc + 0.7827717662 * mc - 0.8086757660 * sc;
    (l, a, b_out)
}

/// L: 0-1, a/b as above, alpha: 0-255 → RGBA
pub fn oklab_to_rgba(l: f32, a: f32, b: f32, alpha: u8) -> Rgba {
    let lc = l + 0.3963377774 * a + 0.2158037573 * b;
    let mc = l - 0.1055613458 * a - 0.0638541728 * b;
    let sc = l - 0.0894841775 * a - 1.2914855480 * b;
    let lc3 = lc * lc * lc;
    let mc3 = mc * mc * mc;
    let sc3 = sc * sc * sc;
    let r =  4.0767416621 * lc3 - 3.3077115913 * mc3 + 0.2309699292 * sc3;
    let g = -1.2684380046 * lc3 + 2.6097574011 * mc3 - 0.3413193965 * sc3;
    let b_out = -0.0041960863 * lc3 - 0.7034186147 * mc3 + 1.7076147010 * sc3;
    [
        linear_to_srgb(r),
        linear_to_srgb(g),
        linear_to_srgb(b_out),
        alpha,
    ]
}

/// Returns OKLCh (L: 0-1, C: 0-~0.4, H: 0-360 degrees) from RGBA.
/// H is undefined when C is ~0 (achromatic); caller should preserve prior H.
pub fn rgba_to_oklch(color: Rgba) -> (f32, f32, f32) {
    let (l, a, b) = rgba_to_oklab(color);
    let c = (a * a + b * b).sqrt();
    let mut h_deg = b.atan2(a).to_degrees();
    if h_deg < 0.0 { h_deg += 360.0; }
    (l, c, h_deg)
}

/// OKLCh (L: 0-1, C: 0-~0.4, H: 0-360) + alpha → RGBA.
pub fn oklch_to_rgba(l: f32, c: f32, h_deg: f32, alpha: u8) -> Rgba {
    let h_rad = h_deg.to_radians();
    let a = c * h_rad.cos();
    let b = c * h_rad.sin();
    oklab_to_rgba(l, a, b, alpha)
}

/// Generate an N-step OKLCh ramp anchored at the user's current color.
///
/// L spans [0.15, 0.90] linearly across the ramp.
/// C is bell-shaped — peaks at the middle, dips ~30% at endpoints.
/// H drifts ±8° across the ramp (cooler shadows, warmer highlights — sign of drift depends on anchor).
///
/// The "anchor" is the index of the ramp slot where the user's input color lives.
/// We then scale C and offset H so that at the anchor index the ramp matches input exactly.
pub fn generate_ramp(base_l: f32, base_c: f32, base_h: f32, n: usize, anchor: RampAnchor, hue_shift_deg: f32, sat_curve_depth: f32) -> Vec<(f32, f32, f32)> {
    if n == 0 { return Vec::new(); }
    let anchor_idx = match anchor {
        RampAnchor::Middle    => n / 2,
        RampAnchor::BaseStep3 => 2.min(n.saturating_sub(1)),
        RampAnchor::Endpoints => 0,
    };

    const L_MIN: f32 = 0.15;
    const L_MAX: f32 = 0.90;

    let denom = (n.saturating_sub(1)).max(1) as f32;
    let l_at = |i: usize| L_MIN + (L_MAX - L_MIN) * (i as f32 / denom);
    let c_curve = |i: usize| {
        let t = i as f32 / denom;
        1.0 - ((2.0 * t - 1.0).powi(2)) * sat_curve_depth
    };
    let h_at = |i: usize| {
        let t = (i as f32 - anchor_idx as f32) / denom;
        base_h + t * hue_shift_deg
    };

    let c_scale = if c_curve(anchor_idx).abs() < 1e-6 { 0.0 } else { base_c / c_curve(anchor_idx) };
    let l_shift = base_l - l_at(anchor_idx);

    (0..n).map(|i| {
        let l = (l_at(i) + l_shift).clamp(0.0, 1.0);
        let c = (c_curve(i) * c_scale).max(0.0);
        let h = h_at(i).rem_euclid(360.0);
        (l, c, h)
    }).collect()
}

/// Generate an N-step HSV ramp anchored at the user's current color.
///
/// V spans [0.20, 0.95] linearly across the ramp.
/// S follows a bell curve (peak in middle, dips at endpoints).
/// H uses **directional** pixel-art hue-shift: indices below anchor (shadows) get +H
/// (toward blue), indices above anchor (highlights) get -H (toward yellow).
pub fn generate_ramp_hsv(base_h: f32, base_s: f32, base_v: f32, n: usize, anchor: RampAnchor, hue_shift_deg: f32, sat_curve_depth: f32) -> Vec<(f32, f32, f32)> {
    if n == 0 { return Vec::new(); }
    let anchor_idx = match anchor {
        RampAnchor::Middle    => n / 2,
        RampAnchor::BaseStep3 => 2.min(n.saturating_sub(1)),
        RampAnchor::Endpoints => 0,
    };

    const V_MIN: f32 = 0.20;
    const V_MAX: f32 = 0.95;

    let denom = (n.saturating_sub(1)).max(1) as f32;
    let v_at = |i: usize| V_MIN + (V_MAX - V_MIN) * (i as f32 / denom);
    let s_curve = |i: usize| {
        let t = i as f32 / denom;
        1.0 - ((2.0 * t - 1.0).powi(2)) * sat_curve_depth
    };
    // Pixel-art directional hue-shift: shadows +H (toward blue), highlights -H (toward yellow).
    // Index below anchor is "shadow", index above is "highlight".
    let h_at = |i: usize| {
        let t = (i as f32 - anchor_idx as f32) / denom; // negative = shadow, positive = highlight
        base_h + (-t) * hue_shift_deg
    };

    let s_scale = if s_curve(anchor_idx).abs() < 1e-6 { 0.0 } else { base_s / s_curve(anchor_idx) };
    let v_shift = base_v - v_at(anchor_idx);

    (0..n).map(|i| {
        let v = (v_at(i) + v_shift).clamp(0.0, 1.0);
        let s = (s_curve(i) * s_scale).clamp(0.0, 1.0);
        let h = h_at(i).rem_euclid(360.0);
        (h, s, v)
    }).collect()
}

fn srgb_to_linear(c: u8) -> f32 {
    let f = c as f32 / 255.0;
    if f <= 0.04045 { f / 12.92 } else { ((f + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(f: f32) -> u8 {
    let f = f.clamp(0.0, 1.0);
    let out = if f <= 0.0031308 { f * 12.92 } else { 1.055 * f.powf(1.0 / 2.4) - 0.055 };
    (out * 255.0).round() as u8
}
