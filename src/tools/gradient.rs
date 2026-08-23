// src/tools/gradient.rs
//
// Linear gradient over an arbitrary masked region of the layer. Pure — the
// caller decides the region (a face's island, the whole canvas) and the mask
// (face-outline ownership, selection); this only does the axis math and the
// per-style color choice.

use super::PixelEdit;
use crate::color::oklab::{oklab_to_rgba, rgba_to_oklab};
use crate::project::{Layer, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientStyle {
    /// Foreground → background through an ordered 2×2 checker dither.
    /// Exactly two output colors — the classic pixel-art blend.
    Dithered,
    /// Steps through the resolved ramp colors (banded, palette-only).
    PaletteRamp,
    /// Continuous OkLab interpolation foreground → background.
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
            GradientStyle::PaletteRamp => "Palette ramp",
            GradientStyle::Smooth => "Smooth",
        }
    }
}

/// Ordered 2×2 Bayer thresholds, indexed [y % 2][x % 2] in ABSOLUTE atlas
/// coords so the pattern stays phase-stable across faces and islands.
const BAYER2: [[f32; 2]; 2] = [[0.125, 0.625], [0.875, 0.375]];

fn smooth_at(fg: Rgba, bg: Rgba, t: f32) -> Rgba {
    let (l0, a0, b0) = rgba_to_oklab(fg);
    let (l1, a1, b1) = rgba_to_oklab(bg);
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let alpha = (fg[3] as f32 + (bg[3] as f32 - fg[3] as f32) * t).round() as u8;
    oklab_to_rgba(lerp(l0, l1), lerp(a0, a1), lerp(b0, b1), alpha)
}

/// Compute a linear gradient's pixel edits over `rect` (x, y, w, h), keeping
/// only texels the mask owns and whose color actually changes. `start`/`end`
/// are absolute atlas texel coordinates (float; a texel's center is +0.5).
/// A zero-length axis paints solid t = 0.
#[allow(clippy::too_many_arguments)]
pub fn apply_gradient(
    layer: &Layer,
    rect: (u32, u32, u32, u32),
    mask: impl Fn(u32, u32) -> bool,
    start: (f32, f32),
    end: (f32, f32),
    style: GradientStyle,
    fg: Rgba,
    bg: Rgba,
    ramp: Option<&[Rgba]>,
) -> Vec<PixelEdit> {
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
                    if t > BAYER2[(gy % 2) as usize][(gx % 2) as usize] {
                        bg
                    } else {
                        fg
                    }
                }
                GradientStyle::PaletteRamp => match ramp {
                    Some(colors) if !colors.is_empty() => {
                        let n = colors.len();
                        colors[((t * n as f32) as usize).min(n - 1)]
                    }
                    _ => smooth_at(fg, bg, t),
                },
                GradientStyle::Smooth => smooth_at(fg, bg, t),
            };
            let old = layer.get_pixel(gx, gy);
            if old != new {
                edits.push((gx, gy, old, new));
            }
        }
    }
    edits
}
