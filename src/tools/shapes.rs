// src/tools/shapes.rs
use crate::project::{Layer, Rgba};
use super::{PixelEdit, apply_pencil};

/// Draw a filled or outlined rectangle.  All coordinates are unconstrained i32 —
/// pixels that land outside the canvas are discarded by apply_pencil's bounds check.
pub fn apply_rect(layer: &Layer, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let (lx, rx) = (x0.min(x1), x0.max(x1));
    let (ly, ry) = (y0.min(y1), y0.max(y1));
    for y in ly..=ry {
        for x in lx..=rx {
            let on_border = x == lx || x == rx || y == ly || y == ry;
            if filled || on_border {
                if x >= 0 && y >= 0 {
                    edits.extend(apply_pencil(layer, x as u32, y as u32, color));
                }
            }
        }
    }
    edits
}

/// Draw a filled or outlined ellipse.  All coordinates are unconstrained i32.
pub fn apply_ellipse(layer: &Layer, cx: i32, cy: i32, rx: i32, ry: i32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    if rx == 0 && ry == 0 {
        if cx >= 0 && cy >= 0 {
            edits.extend(apply_pencil(layer, cx as u32, cy as u32, color));
        }
        return edits;
    }
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let inside = (dx * dx) as f32 / (rx * rx) as f32 + (dy * dy) as f32 / (ry * ry) as f32 <= 1.0;
            let on_border = {
                let outer = inside;
                let inner_x = (rx - 1).max(0);
                let inner_y = (ry - 1).max(0);
                let inner = if inner_x == 0 || inner_y == 0 { false }
                    else { (dx * dx) as f32 / (inner_x * inner_x) as f32 + (dy * dy) as f32 / (inner_y * inner_y) as f32 <= 1.0 };
                outer && !inner
            };
            if (filled && inside) || (!filled && on_border) {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && py >= 0 {
                    edits.extend(apply_pencil(layer, px as u32, py as u32, color));
                }
            }
        }
    }
    edits
}

/// Draw a line between two unconstrained i32 canvas coordinates.
pub fn apply_line(layer: &Layer, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) -> Vec<PixelEdit> {
    super::bresenham_line(layer, x0, y0, x1, y1, color)
}
