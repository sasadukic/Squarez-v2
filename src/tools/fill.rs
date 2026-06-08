// src/tools/fill.rs
use crate::project::{Layer, Rgba};
use super::PixelEdit;
use std::collections::VecDeque;

pub fn apply_fill(layer: &Layer, x: u32, y: u32, target: Rgba, replacement: Rgba) -> Vec<PixelEdit> {
    if target == replacement { return vec![]; }
    let mut edits: Vec<PixelEdit> = Vec::new();
    let mut visited = vec![false; (layer.width * layer.height) as usize];
    let mut queue = VecDeque::new();
    queue.push_back((x, y));
    while let Some((cx, cy)) = queue.pop_front() {
        if cx >= layer.width || cy >= layer.height { continue; }
        let idx = (cy * layer.width + cx) as usize;
        if visited[idx] { continue; }
        if layer.get_pixel(cx, cy) != target { continue; }
        visited[idx] = true;
        edits.push((cx, cy, target, replacement));
        if cx > 0               { queue.push_back((cx - 1, cy)); }
        if cx + 1 < layer.width { queue.push_back((cx + 1, cy)); }
        if cy > 0               { queue.push_back((cx, cy - 1)); }
        if cy + 1 < layer.height { queue.push_back((cx, cy + 1)); }
    }
    edits
}

/// Fill every transparent pixel that is fully enclosed by painted stroke pixels.
///
/// Algorithm (border flood-fill):
///   1. Flood-fill transparent pixels reachable from the 4 canvas borders → "outside".
///   2. Any transparent pixel inside the stroke bounding box NOT reached = enclosed → fill.
///
/// `layer` is the layer **after** the stroke has already been painted onto it.
pub fn fill_enclosed_region(
    layer: &Layer,
    stroke_pixels: &[(u32, u32)],
    color: Rgba,
) -> Vec<PixelEdit> {
    if stroke_pixels.is_empty() || color[3] == 0 { return vec![]; }

    let w = layer.width;
    let h = layer.height;

    // passable = fully transparent (no ink) — these are the pixels the outside-fill can flow through
    let passable = |x: u32, y: u32| -> bool {
        x < w && y < h && layer.get_pixel(x, y)[3] == 0
    };

    // Step 1: flood-fill "outside" from all 4 borders
    let mut outside = vec![false; (w * h) as usize];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();

    let mut seed = |queue: &mut VecDeque<(u32, u32)>, outside: &mut Vec<bool>, x: u32, y: u32| {
        if passable(x, y) {
            let idx = (y * w + x) as usize;
            if !outside[idx] {
                outside[idx] = true;
                queue.push_back((x, y));
            }
        }
    };

    for x in 0..w {
        seed(&mut queue, &mut outside, x, 0);
        seed(&mut queue, &mut outside, x, h - 1);
    }
    for y in 0..h {
        seed(&mut queue, &mut outside, 0, y);
        seed(&mut queue, &mut outside, w - 1, y);
    }

    while let Some((cx, cy)) = queue.pop_front() {
        for (nx, ny) in [
            (cx.wrapping_sub(1), cy),
            (cx + 1, cy),
            (cx, cy.wrapping_sub(1)),
            (cx, cy + 1),
        ] {
            if passable(nx, ny) {
                let idx = (ny * w + nx) as usize;
                if !outside[idx] {
                    outside[idx] = true;
                    queue.push_back((nx, ny));
                }
            }
        }
    }

    // Step 2: bounding box of the stroke
    let min_x = stroke_pixels.iter().map(|&(x, _)| x).min().unwrap_or(0);
    let max_x = stroke_pixels.iter().map(|&(x, _)| x).max().unwrap_or(0);
    let min_y = stroke_pixels.iter().map(|&(_, y)| y).min().unwrap_or(0);
    let max_y = stroke_pixels.iter().map(|&(_, y)| y).max().unwrap_or(0);

    // Step 3: fill enclosed interior pixels
    let mut edits = Vec::new();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if !passable(x, y) { continue; }          // already opaque
            let idx = (y * w + x) as usize;
            if outside[idx] { continue; }             // reachable from border = outside
            let old = layer.get_pixel(x, y);
            edits.push((x, y, old, color));
        }
    }
    edits
}
