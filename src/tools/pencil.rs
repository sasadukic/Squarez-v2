// src/tools/pencil.rs
use crate::project::{Layer, Rgba};
use super::PixelEdit;
pub fn apply_pencil(layer: &Layer, x: u32, y: u32, color: Rgba) -> Vec<PixelEdit> {
    let old = layer.get_pixel(x, y);
    if old == color { return vec![]; }
    vec![(x, y, old, color)]
}

pub fn apply_eraser(layer: &Layer, x: u32, y: u32) -> Vec<PixelEdit> {
    let old = layer.get_pixel(x, y);
    let transparent = [0u8, 0, 0, 0];
    if old == transparent { return vec![]; }
    vec![(x, y, old, transparent)]
}

/// Returns pixels along a Bresenham line between two points.
/// Accepts unconstrained i32 coordinates; pixels outside the canvas are skipped.
pub fn bresenham_line(layer: &Layer, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let mut x0 = x0;
    let mut y0 = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 {
            let edit = apply_pencil(layer, x0 as u32, y0 as u32, color);
            edits.extend(edit);
        }
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
    edits
}

/// Returns all pixel positions along a Bresenham line (no dedup, no layer access).
pub fn bresenham_positions(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(u32, u32)> {
    let mut positions = Vec::new();
    let mut x0 = x0;
    let mut y0 = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 {
            positions.push((x0 as u32, y0 as u32));
        }
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
    positions
}


