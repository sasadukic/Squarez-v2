// src/tools/eyedropper.rs
use crate::project::{Layer, Rgba};

pub fn apply_eyedropper(layer: &Layer, x: u32, y: u32) -> Rgba {
    layer.get_pixel(x, y)
}
