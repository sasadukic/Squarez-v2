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
