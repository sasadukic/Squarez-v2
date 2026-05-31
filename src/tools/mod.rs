// src/tools/mod.rs
pub mod pencil;
pub mod fill;
pub mod eyedropper;
pub mod shapes;
pub mod select;

pub use pencil::{apply_pencil, apply_eraser, bresenham_line, bresenham_positions};
pub use fill::apply_fill;
pub use eyedropper::apply_eyedropper;
pub use shapes::{apply_rect, apply_ellipse, apply_line};
pub use select::{SelectState, SelectInteraction, Handle, FloatBuffer, DragAnchor, sample_transformed};

use crate::project::Rgba;

pub type PixelEdit = (u32, u32, Rgba, Rgba); // (x, y, old, new)

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveTool {
    Pencil,
    Eraser,
    Fill,
    Eyedropper,
    Rectangle { filled: bool },
    Ellipse   { filled: bool },
    Line,
    RectSelect,
    Move,
    Zoom,
}

#[derive(Debug, Clone)]
pub struct ToolInput {
    pub canvas_x: u32,
    pub canvas_y: u32,
    pub color: Rgba,
}
