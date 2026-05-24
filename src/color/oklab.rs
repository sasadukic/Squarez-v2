// src/color/oklab.rs
use crate::project::Rgba;

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

/// Convert OKLCh -> linear RGB (no sRGB transfer or clamping).
/// Returns linear RGB floats (may be outside 0.0..1.0 for out-of-gamut colors).
pub fn oklch_to_linear_rgb(l: f32, c: f32, h_deg: f32) -> (f32, f32, f32) {
    let h_rad = h_deg.to_radians();
    let a = c * h_rad.cos();
    let b = c * h_rad.sin();
    // replicate oklab_to_rgba internals up to linear RGB
    let lc = l + 0.3963377774 * a + 0.2158037573 * b;
    let mc = l - 0.1055613458 * a - 0.0638541728 * b;
    let sc = l - 0.0894841775 * a - 1.2914855480 * b;
    let lc3 = lc * lc * lc;
    let mc3 = mc * mc * mc;
    let sc3 = sc * sc * sc;
    let r =  4.0767416621 * lc3 - 3.3077115913 * mc3 + 0.2309699292 * sc3;
    let g = -1.2684380046 * lc3 + 2.6097574011 * mc3 - 0.3413193965 * sc3;
    let b_out = -0.0041960863 * lc3 - 0.7034186147 * mc3 + 1.7076147010 * sc3;
    (r, g, b_out)
}

/// Check whether an OKLCh color is in-gamut for sRGB (all channels inside 0.0..=1.0)
pub fn oklch_in_gamut(l: f32, c: f32, h_deg: f32) -> bool {
    let (r, g, b) = oklch_to_linear_rgb(l, c, h_deg);
    // Convert linear RGB to sRGB (no clamping) to check final gamut
    let sr = linear_to_srgb_f32(r);
    let sg = linear_to_srgb_f32(g);
    let sb = linear_to_srgb_f32(b);
    sr >= 0.0 && sr <= 1.0 && sg >= 0.0 && sg <= 1.0 && sb >= 0.0 && sb <= 1.0
}

fn linear_to_srgb_f32(f: f32) -> f32 {
    // Same as linear_to_srgb but returns float in 0..1 (not clamped here)
    if f <= 0.0031308 { f * 12.92 } else { 1.055 * f.powf(1.0 / 2.4) - 0.055 }
}

/// Find maximum chroma C for given L and H that still maps to in-gamut sRGB.
/// Uses binary search between 0 and C_hi (default 0.6) with tol on C.
pub fn find_max_chroma_for_lh(l: f32, h_deg: f32, c_hi: f32, tol: f32) -> f32 {
    let mut lo = 0.0_f32;
    let mut hi = c_hi.max(0.0);
    // quick exit: if hi is already in gamut, return hi
    if oklch_in_gamut(l, hi, h_deg) {
        return hi;
    }
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        if (hi - lo) < tol { break; }
        if oklch_in_gamut(l, mid, h_deg) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Adjust L to try to match a target perceived L after converting to sRGB and back.
/// We search L' in [L - delta, L + delta] for a color whose oklab L after round-trip
/// conversion is within tol of target L. Returns Some(L') or None if not found.
pub fn solve_for_roundtrip_lightness(target_l: f32, c: f32, h_deg: f32, delta: f32, tol: f32) -> Option<f32> {
    let mut lo = (target_l - delta).clamp(0.0, 1.0);
    let mut hi = (target_l + delta).clamp(0.0, 1.0);
    // If current target_l is already close after round-trip, return it
    if (roundtrip_oklab_l(target_l, c, h_deg) - target_l).abs() <= tol { return Some(target_l); }
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        let mid_l = roundtrip_oklab_l(mid, c, h_deg);
        let err = mid_l - target_l;
        if err.abs() <= tol { return Some(mid); }
        if err > 0.0 {
            // mid maps too light -> reduce source L
            hi = mid;
        } else {
            lo = mid;
        }
        if (hi - lo) < 1e-5 { break; }
    }
    None
}

fn roundtrip_oklab_l(source_l: f32, c: f32, h_deg: f32) -> f32 {
    let (r_lin, g_lin, b_lin) = oklch_to_linear_rgb(source_l, c, h_deg);
    // convert linear RGB -> srgb -> bytes -> back to oklab
    let sr = linear_to_srgb_f32(r_lin).clamp(0.0, 1.0);
    let sg = linear_to_srgb_f32(g_lin).clamp(0.0, 1.0);
    let sb = linear_to_srgb_f32(b_lin).clamp(0.0, 1.0);
    // convert srgb floats to u8 and back through existing helpers
    let rgba = [
        (sr * 255.0).round() as u8,
        (sg * 255.0).round() as u8,
        (sb * 255.0).round() as u8,
        255u8,
    ];
    let (l, _a, _b) = rgba_to_oklab(rgba);
    l
}

/// Safe conversion: adjust chroma (and optionally L) so the resulting sRGB color
/// stays in-gamut. If `preserve_l` is true, attempt a round-trip lightness correction
/// after lowering chroma to better preserve perceived L.
pub fn safe_oklch_to_rgba(l: f32, c: f32, h_deg: f32, alpha: u8, preserve_l: bool) -> Rgba {
    // Fast path
    if oklch_in_gamut(l, c, h_deg) {
        return oklch_to_rgba(l, c, h_deg, alpha);
    }
    // Find maximum chroma that stays in gamut
    let c_max = find_max_chroma_for_lh(l, h_deg, 0.6, 1e-4);
    if c_max <= 1e-5 {
        // Essentially achromatic at this L/H — drop chroma
        return oklch_to_rgba(l, 0.0, h_deg, alpha);
    }
    let c_adj = c.min(c_max);
    let mut l_adj = l;
    if preserve_l {
        if let Some(sol) = solve_for_roundtrip_lightness(l, c_adj, h_deg, 0.08, 0.003) {
            l_adj = sol;
        }
    }
    oklch_to_rgba(l_adj, c_adj, h_deg, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_gamut_simple() {
        // neutral gray must be in gamut
        assert!(oklch_in_gamut(0.5, 0.0, 0.0));
    }

    #[test]
    fn test_find_max_chroma_basic() {
        let c = find_max_chroma_for_lh(0.5, 120.0, 0.6, 1e-4);
        // should be non-zero for mid-lightness
        assert!(c > 0.05);
    }

    #[test]
    fn test_roundtrip_lightness() {
        // choose vivid green where chroma must be reduced
        let l = 0.6;
        let h = 140.0;
        let c_hi = 0.6;
        let c = find_max_chroma_for_lh(l, h, c_hi, 1e-4);
        let solved = solve_for_roundtrip_lightness(l, c, h, 0.08, 0.005);
        assert!(solved.is_some());
    }
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
