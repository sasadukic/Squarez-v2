// src/tools/gradient.rs
//
// Linear gradient over an arbitrary masked region of the layer. Pure — the
// caller decides the region (a face's island, the whole canvas), the mask
// (face-outline ownership, selection), and the colors (the user's palette
// selection, at least two); this does the axis math and the per-style blend.

use super::PixelEdit;
use crate::color::oklab::{oklab_to_rgba, rgba_to_oklab};
use crate::project::{Layer, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientStyle {
    /// Adjacent selected colors blended through an ordered 2x2 checker
    /// dither — the classic pixel-art gradient. Only selected colors appear.
    Dithered,
    /// Hard bands, one per selected color.
    PaletteRamp,
    /// Continuous OkLab interpolation through the selected colors.
    Smooth,
}

impl GradientStyle {
    pub fn next(self) -> Self {
        match self {
            GradientStyle::Dithered => GradientStyle::PaletteRamp,
            GradientStyle::PaletteRamp => GradientStyle::Smooth,
            GradientStyle::Smooth => GradientStyle::Dithered,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GradientStyle::Dithered => "Dithered",
            GradientStyle::PaletteRamp => "Banded",
            GradientStyle::Smooth => "Smooth",
        }
    }
}

/// Ordered 2x2 Bayer thresholds, indexed [y % 2][x % 2] in ABSOLUTE atlas
/// coords so the pattern stays phase-stable across faces and islands.
const BAYER2: [[f32; 2]; 2] = [[0.125, 0.625], [0.875, 0.375]];

fn lerp_oklab(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let (l0, a0, b0) = rgba_to_oklab(a);
    let (l1, a1, b1) = rgba_to_oklab(b);
    let lp = |x: f32, y: f32| x + (y - x) * t;
    let alpha = (a[3] as f32 + (b[3] as f32 - a[3] as f32) * t).round() as u8;
    oklab_to_rgba(lp(l0, l1), lp(a0, a1), lp(b0, b1), alpha)
}

/// Snap the drag axis to the nearest of the 8 pixel-art directions
/// (horizontal, vertical, 45 degrees), preserving the dragged length along
/// the snapped direction. A free-angle gradient looks jagged on a texel
/// grid, so the tool only ever paints along these.
pub fn snap_axis_8(start: (f32, f32), end: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    if dx == 0.0 && dy == 0.0 {
        return end;
    }
    let sector = (dy.atan2(dx) / std::f32::consts::FRAC_PI_4).round();
    let angle = sector * std::f32::consts::FRAC_PI_4;
    let (ux, uy) = (angle.cos(), angle.sin());
    let len = dx * ux + dy * uy;
    (start.0 + ux * len, start.1 + uy * len)
}

/// Compute a linear gradient's pixel edits over `rect` (x, y, w, h), keeping
/// only texels the mask owns and whose color actually changes. `start`/`end`
/// are absolute atlas texel coordinates (float; a texel's center is +0.5).
/// Fewer than two colors paints nothing — a gradient needs a selection of at
/// least two palette colors. A zero-length axis paints solid colors[0].
pub fn apply_gradient(
    layer: &Layer,
    rect: (u32, u32, u32, u32),
    mask: impl Fn(u32, u32) -> bool,
    start: (f32, f32),
    end: (f32, f32),
    style: GradientStyle,
    colors: &[Rgba],
) -> Vec<PixelEdit> {
    let n = colors.len();
    if n < 2 {
        return Vec::new();
    }
    let (rx, ry, rw, rh) = rect;
    let d = (end.0 - start.0, end.1 - start.1);
    let len2 = d.0 * d.0 + d.1 * d.1;
    let mut edits = Vec::new();
    for gy in ry..ry.saturating_add(rh) {
        for gx in rx..rx.saturating_add(rw) {
            if !mask(gx, gy) {
                continue;
            }
            let c = (gx as f32 + 0.5, gy as f32 + 0.5);
            let t = if len2 < 1e-6 {
                0.0
            } else {
                (((c.0 - start.0) * d.0 + (c.1 - start.1) * d.1) / len2).clamp(0.0, 1.0)
            };
            let new = match style {
                GradientStyle::Dithered => {
                    // Position within the color list; dither the fraction
                    // between each adjacent pair.
                    let tp = t * (n - 1) as f32;
                    let i = (tp as usize).min(n - 2);
                    let f = tp - i as f32;
                    if f > BAYER2[(gy % 2) as usize][(gx % 2) as usize] {
                        colors[i + 1]
                    } else {
                        colors[i]
                    }
                }
                GradientStyle::PaletteRamp => colors[((t * n as f32) as usize).min(n - 1)],
                GradientStyle::Smooth => {
                    let tp = t * (n - 1) as f32;
                    let i = (tp as usize).min(n - 2);
                    lerp_oklab(colors[i], colors[i + 1], tp - i as f32)
                }
            };
            let old = layer.get_pixel(gx, gy);
            if old != new {
                edits.push((gx, gy, old, new));
            }
        }
    }
    edits
}
