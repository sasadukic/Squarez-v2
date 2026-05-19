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

fn srgb_to_linear(c: u8) -> f32 {
    let f = c as f32 / 255.0;
    if f <= 0.04045 { f / 12.92 } else { ((f + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(f: f32) -> u8 {
    let f = f.clamp(0.0, 1.0);
    let out = if f <= 0.0031308 { f * 12.92 } else { 1.055 * f.powf(1.0 / 2.4) - 0.055 };
    (out * 255.0).round() as u8
}
