// src/tools/mod.rs
pub mod pencil;
pub mod fill;
pub mod eyedropper;
pub mod shapes;
pub mod select;
pub mod gradient;

pub use pencil::{apply_pencil, apply_eraser, bresenham_line, bresenham_positions};
pub use fill::{apply_fill, fill_enclosed_region};
pub use eyedropper::apply_eyedropper;
pub use shapes::{apply_rect, apply_ellipse, apply_line, iso_box_preview, iso_box_pixels, iso_cylinder_preview, iso_cylinder_pixels};
pub use gradient::{apply_gradient, snap_axis_8, GradientStyle};
pub use select::{SelectState, SelectInteraction, Handle, FloatBuffer, DragAnchor, sample_transformed, SelectionMask, SelectionMode, magic_wand_select};

use crate::project::Rgba;

pub type PixelEdit = (u32, u32, Rgba, Rgba); // (x, y, old, new)

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveTool {
    Pencil,
    Eraser,
    Fill,
    Eyedropper,
    /// Linear gradient, clipped to the face it lands on in 3D projects.
    Gradient,
    Rectangle { filled: bool },
    Ellipse   { filled: bool },
    Line,
    RectSelect,
    MagicWand,
    Move,
    Zoom,
    // 3D mode tools
    /// Smart select: picks vertices, edges, or faces by what you click.
    Select3D,
    Extrude,
    Inset,
    LoopCut,
    MoveObject,
    ScaleObject,
    /// Quarter-turn object rotation (90-degree steps about the view axes).
    RotateObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoMode {
    Off,
    Isometric,
    IsometricHidden,
    IsometricFill,
    TopDown,
    TopDownFill,
}


