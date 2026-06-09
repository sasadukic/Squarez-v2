// src/tools/select.rs
// Selection tool — supports rect select, move, resize (nearest-neighbor scale),
// and rotate (Shift snaps to 90°).
//
// Lifecycle:
//   1. User drags with RectSelect tool → builds a `rect` (no float yet).
//   2. On drag release with a non-empty rect → `float_pixels` is filled by
//      lifting pixels from the active layer; those layer cells are cleared.
//   3. While `float_pixels` is Some, the selection can be moved / resized /
//      rotated. The transformed bitmap is rendered every frame as a preview.
//   4. Commit happens on Escape, tool change, or starting a new selection.
//
// `origin` = original lift rect in canvas pixel coords (the source rectangle).
// `transform` = (offset_x, offset_y, scale_x, scale_y, rotation_radians)
//   - offset is the top-left of the transformed AABB
//   - scale 1.0 = unchanged size; negative = mirrored
//   - rotation is around the center of the (scaled) rect

use crate::project::{Rgba, Layer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionMask {
    pub width: u32,
    pub height: u32,
    pub mask: Vec<bool>, // row-major
}

impl SelectionMask {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mask: vec![false; (width * height) as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.mask[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, val: bool) {
        if x < self.width && y < self.height {
            self.mask[(y * self.width + x) as usize] = val;
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.mask.iter().any(|&b| b)
    }

    pub fn bounding_box(&self) -> Option<(u32, u32, u32, u32)> {
        let mut min_x = self.width;
        let mut max_x = 0;
        let mut min_y = self.height;
        let mut max_y = 0;
        let mut found = false;

        for y in 0..self.height {
            for x in 0..self.width {
                if self.mask[(y * self.width + x) as usize] {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                    found = true;
                }
            }
        }

        if found {
            Some((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
        } else {
            None
        }
    }

    pub fn combine(&self, other: &Self, mode: SelectionMode) -> Self {
        if self.width != other.width || self.height != other.height {
            return self.clone();
        }
        let mut result = self.clone();
        for i in 0..self.mask.len() {
            match mode {
                SelectionMode::Replace => {
                    result.mask[i] = other.mask[i];
                }
                SelectionMode::Add => {
                    result.mask[i] = self.mask[i] || other.mask[i];
                }
                SelectionMode::Subtract => {
                    result.mask[i] = self.mask[i] && !other.mask[i];
                }
                SelectionMode::Intersect => {
                    result.mask[i] = self.mask[i] && other.mask[i];
                }
            }
        }
        result
    }
}

pub fn magic_wand_select(layer: &Layer, start_x: u32, start_y: u32, eight_way: bool) -> SelectionMask {
    let w = layer.width;
    let h = layer.height;
    let mut selected = vec![false; (w * h) as usize];
    let mut visited = vec![false; (w * h) as usize];

    let target = layer.get_pixel(start_x, start_y);
    let mut stack = Vec::new();
    stack.push((start_x, start_y));

    while let Some((x, y)) = stack.pop() {
        if x >= w || y >= h {
            continue;
        }
        let idx = (y * w + x) as usize;
        if visited[idx] {
            continue;
        }
        visited[idx] = true;

        let current = layer.get_pixel(x, y);
        if current[0] == target[0]
            && current[1] == target[1]
            && current[2] == target[2]
            && current[3] == target[3]
        {
            selected[idx] = true;

            // 4-way neighbors
            if x > 0 { stack.push((x - 1, y)); }
            if x + 1 < w { stack.push((x + 1, y)); }
            if y > 0 { stack.push((x, y - 1)); }
            if y + 1 < h { stack.push((x, y + 1)); }

            if eight_way {
                // Diagonal neighbors
                if x > 0 && y > 0 { stack.push((x - 1, y - 1)); }
                if x > 0 && y + 1 < h { stack.push((x - 1, y + 1)); }
                if x + 1 < w && y > 0 { stack.push((x + 1, y - 1)); }
                if x + 1 < w && y + 1 < h { stack.push((x + 1, y + 1)); }
            }
        }
    }

    SelectionMask {
        width: w,
        height: h,
        mask: selected,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Handle {
    NW, N, NE,
    W,      E,
    SW, S, SE,
    Rotate,
    FlipH, // click-only, left of W handle  → mirrors horizontally
    FlipV, // click-only, below S handle    → mirrors vertically
    Inside,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SelectInteraction {
    #[default]
    None,
    Moving,
    Resizing(Handle),
    Rotating,
}

#[derive(Debug, Clone)]
pub struct FloatBuffer {
    pub w: u32,
    pub h: u32,
    pub pixels: Vec<Rgba>, // row-major, length = w*h
}

impl FloatBuffer {
    pub fn sample(&self, sx: i32, sy: i32) -> Option<Rgba> {
        if sx < 0 || sy < 0 || sx >= self.w as i32 || sy >= self.h as i32 {
            return None;
        }
        Some(self.pixels[(sy as u32 * self.w + sx as u32) as usize])
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectState {
    /// Marquee rect (during initial drag, before lift).
    pub rect: Option<(u32, u32, u32, u32)>, // (x, y, w, h)

    /// Magic Wand selection mask (arbitrary boolean mask)
    pub mask: Option<SelectionMask>,
    pub wand_mode: SelectionMode,
    pub wand_eight_way: bool,

    /// Lifted pixel buffer (Some after first drag release on a non-empty rect).
    pub float_pixels: Option<FloatBuffer>,
    /// Original lift rect (immutable reference for the source).
    pub origin: Option<(u32, u32, u32, u32)>,

    /// Current transform of the float buffer.
    /// offset = top-left of transformed bounding box (canvas-space, pixels, f32 for sub-pixel drag).
    pub offset: (f32, f32),
    /// Current scale (post-resize). 1.0 = original.  Can be negative (mirror).
    pub scale: (f32, f32),
    /// Rotation in radians around the center of the rect.
    pub rotation: f32,

    /// Active interaction (drag-in-progress).
    pub interaction: SelectInteraction,
    /// Cached drag start info: (mouse_canvas_x, mouse_canvas_y, snapshot of offset/scale/rotation).
    pub drag_anchor: Option<DragAnchor>,

    /// Legacy clipboard (kept for future Ctrl+C/V).
    pub clipboard: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
pub struct DragAnchor {
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub offset: (f32, f32),
    pub scale: (f32, f32),
    pub rotation: f32,
}

impl SelectState {
    pub fn has_float(&self) -> bool { self.float_pixels.is_some() }

    /// True when a float exists and the user can move/resize/rotate it.
    pub fn is_active(&self) -> bool { self.has_float() }

    /// Width/height of the float buffer in source pixels.
    pub fn float_size(&self) -> Option<(u32, u32)> {
        self.float_pixels.as_ref().map(|f| (f.w, f.h))
    }

    /// Reset everything (no float, no rect).
    pub fn clear(&mut self) {
        *self = SelectState::default();
    }

    /// Begin a float from a freshly lifted buffer and origin rect.
    pub fn begin_float(&mut self, buf: FloatBuffer, origin: (u32, u32, u32, u32)) {
        self.float_pixels = Some(buf);
        self.origin = Some(origin);
        self.offset = (origin.0 as f32, origin.1 as f32);
        self.scale = (1.0, 1.0);
        self.rotation = 0.0;
        self.interaction = SelectInteraction::None;
        self.drag_anchor = None;
        self.rect = None;
    }

    /// Current transformed AABB in canvas pixel space (x, y, w, h) as f32.
    /// Rotation is around the (scaled) rect center, so this returns the
    /// axis-aligned bounding box of the rotated rect.
    pub fn transformed_aabb(&self) -> Option<(f32, f32, f32, f32)> {
        let (w0, h0) = self.float_size()?;
        let sw = w0 as f32 * self.scale.0.abs();
        let sh = h0 as f32 * self.scale.1.abs();
        let (ox, oy) = self.offset;
        if self.rotation == 0.0 {
            return Some((ox, oy, sw, sh));
        }
        let cx = ox + sw * 0.5;
        let cy = oy + sh * 0.5;
        let (s, c) = self.rotation.sin_cos();
        let corners = [
            (ox, oy),
            (ox + sw, oy),
            (ox + sw, oy + sh),
            (ox, oy + sh),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (x, y) in corners {
            let dx = x - cx;
            let dy = y - cy;
            let rx = cx + dx * c - dy * s;
            let ry = cy + dx * s + dy * c;
            min_x = min_x.min(rx);
            min_y = min_y.min(ry);
            max_x = max_x.max(rx);
            max_y = max_y.max(ry);
        }
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// Returns the 4 corners of the (rotated, scaled) selection rect in canvas
    /// pixel space, in the order [NW, NE, SE, SW].
    pub fn rotated_corners(&self) -> Option<[(f32, f32); 4]> {
        let (w0, h0) = self.float_size()?;
        let sw = w0 as f32 * self.scale.0.abs();
        let sh = h0 as f32 * self.scale.1.abs();
        let (ox, oy) = self.offset;
        let cx = ox + sw * 0.5;
        let cy = oy + sh * 0.5;
        let (s, c) = self.rotation.sin_cos();
        let local = [
            (ox, oy),
            (ox + sw, oy),
            (ox + sw, oy + sh),
            (ox, oy + sh),
        ];
        let mut out = [(0.0, 0.0); 4];
        for (i, (x, y)) in local.into_iter().enumerate() {
            let dx = x - cx;
            let dy = y - cy;
            out[i] = (cx + dx * c - dy * s, cy + dx * s + dy * c);
        }
        Some(out)
    }
}

/// Sample the original float buffer at a target canvas pixel `(cx, cy)`,
/// inverting the current transform (translate → rotate → scale).
/// Returns None if outside the source bitmap or fully transparent.
pub fn sample_transformed(state: &SelectState, cx: i32, cy: i32) -> Option<Rgba> {
    let float = state.float_pixels.as_ref()?;
    let (w0, h0) = (float.w as f32, float.h as f32);
    let sw = w0 * state.scale.0.abs();
    let sh = h0 * state.scale.1.abs();
    let (ox, oy) = state.offset;
    let center_x = ox + sw * 0.5;
    let center_y = oy + sh * 0.5;

    // Inverse rotate around center
    let (s, c) = (-state.rotation).sin_cos();
    let dx = cx as f32 + 0.5 - center_x;
    let dy = cy as f32 + 0.5 - center_y;
    let lx = center_x + dx * c - dy * s;
    let ly = center_y + dx * s + dy * c;

    // Back into source space (handle scale + mirror).
    let rel_x = (lx - ox) / sw * w0;
    let rel_y = (ly - oy) / sh * h0;
    let sx = if state.scale.0 < 0.0 { (w0 - rel_x).floor() as i32 } else { rel_x.floor() as i32 };
    let sy = if state.scale.1 < 0.0 { (h0 - rel_y).floor() as i32 } else { rel_y.floor() as i32 };
    let sample = float.sample(sx, sy)?;
    if sample[3] == 0 { return None; }
    Some(sample)
}
