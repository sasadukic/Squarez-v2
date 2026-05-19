// src/tools/shapes.rs
use crate::project::{Layer, Rgba};
use super::{PixelEdit, apply_pencil};

pub fn apply_rect(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let (lx, rx) = (x0.min(x1), x0.max(x1));
    let (ly, ry) = (y0.min(y1), y0.max(y1));
    for y in ly..=ry {
        for x in lx..=rx {
            let on_border = x == lx || x == rx || y == ly || y == ry;
            if filled || on_border {
                edits.extend(apply_pencil(layer, x, y, color));
            }
        }
    }
    edits
}

pub fn apply_ellipse(layer: &Layer, cx: u32, cy: u32, rx: u32, ry: u32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let (cx, cy, rx, ry) = (cx as i32, cy as i32, rx as i32, ry as i32);
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

pub fn apply_line(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba) -> Vec<PixelEdit> {
    super::bresenham_line(layer, x0, y0, x1, y1, color)
}
