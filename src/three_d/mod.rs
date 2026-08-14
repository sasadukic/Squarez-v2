// src/three_d/mod.rs
//
// The 3D modeling + texturing mode. All 3D-specific logic lives in this
// module tree; src/app.rs only dispatches into it.

pub mod camera;
pub mod mesh;
pub mod render;
pub mod workspace;

use crate::project::{Layer, Rgba};

/// Per-tab UI state for the 3D workspace: camera, selection, and in-flight
/// gesture data. Parked/swapped alongside the tab like the undo stack.
#[derive(Debug, Clone)]
pub struct ThreeDState {
    pub camera: camera::Camera3D,
    pub sel_verts: Vec<u32>,
    pub sel_faces: Vec<u32>,
    pub hover_face: Option<u32>,
    /// Pixel edits accumulated during the current paint stroke.
    pub stroke_edits: Vec<(u32, u32, Rgba, Rgba)>,
    /// (face index, island texel) of the previous paint event, for
    /// stroke continuity within one face.
    pub last_paint: Option<(u32, (i64, i64))>,
    /// Wheel/trackpad disambiguation timestamp (same trick as CanvasState).
    pub last_mouse_wheel_time: f64,
}

impl Default for ThreeDState {
    fn default() -> Self {
        Self {
            camera: camera::Camera3D::default(),
            sel_verts: Vec::new(),
            sel_faces: Vec::new(),
            hover_face: None,
            stroke_edits: Vec::new(),
            last_paint: None,
            last_mouse_wheel_time: 0.0,
        }
    }
}

impl ThreeDState {
    /// Drop transient gesture state (selection, stroke) but keep the camera.
    pub fn clear_transient(&mut self) {
        self.sel_verts.clear();
        self.sel_faces.clear();
        self.hover_face = None;
        self.stroke_edits.clear();
        self.last_paint = None;
    }
}

/// Fill every allocated island rect on `layer` with `color`. Used at project
/// creation so new faces are immediately visible on the model.
pub fn paint_islands(layer: &mut Layer, mesh: &mesh::Mesh, color: Rgba) {
    for face in &mesh.faces {
        let isl = face.island;
        for ty in isl.y..isl.y + isl.h {
            for tx in isl.x..isl.x + isl.w {
                layer.set_pixel(tx as u32, ty as u32, color);
            }
        }
    }
}
