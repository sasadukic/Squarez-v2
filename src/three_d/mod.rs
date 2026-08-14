// src/three_d/mod.rs
//
// The 3D modeling + texturing mode. All 3D-specific logic lives in this
// module tree; src/app.rs only dispatches into it.

pub mod camera;
pub mod edit;
pub mod gizmo;
pub mod mesh;
pub mod paint;
pub mod render;
pub mod workspace;

use crate::project::{Layer, Rgba};

/// An in-flight move gesture (vertex/edge/face drag): pristine mesh + layer
/// snapshots at drag start, the vertices being moved, and the accumulated
/// raw plus currently-applied (grid-snapped) world deltas. Previews replay
/// the full move (islands included) from the snapshots on every step.
#[derive(Debug, Clone)]
pub struct VertexDrag {
    pub start_mesh: mesh::Mesh,
    pub start_layer: Layer,
    pub verts: Vec<u32>,
    pub raw: [f32; 3],
    pub applied: [i32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Extrude,
    Inset,
}

/// An in-flight extrude/inset drag: pristine mesh + layer snapshots taken at
/// press time so every preview amount recomputes from clean state.
#[derive(Debug, Clone)]
pub struct OpDrag {
    pub kind: OpKind,
    pub face: u32,
    pub start_mesh: mesh::Mesh,
    pub start_layer: Layer,
    pub raw: f32,
    pub applied: u32,
}

/// Per-tab UI state for the 3D workspace: camera, selection, and in-flight
/// gesture data. Parked/swapped alongside the tab like the undo stack.
#[derive(Debug, Clone)]
pub struct ThreeDState {
    pub camera: camera::Camera3D,
    pub sel_verts: Vec<u32>,
    /// Selected edges as sorted vertex-index pairs.
    pub sel_edges: Vec<(u32, u32)>,
    pub sel_faces: Vec<u32>,
    pub hover_face: Option<u32>,
    pub drag: Option<VertexDrag>,
    pub op_drag: Option<OpDrag>,
    /// Pixel edits accumulated during the current paint stroke.
    pub stroke_edits: Vec<(u32, u32, Rgba, Rgba)>,
    /// Texels already recorded this stroke (one undo entry per texel).
    pub stroke_painted: std::collections::HashSet<(u32, u32)>,
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
            sel_edges: Vec::new(),
            sel_faces: Vec::new(),
            hover_face: None,
            drag: None,
            op_drag: None,
            stroke_edits: Vec::new(),
            stroke_painted: std::collections::HashSet::new(),
            last_paint: None,
            last_mouse_wheel_time: 0.0,
        }
    }
}

impl ThreeDState {
    /// Drop transient gesture state (selection, stroke) but keep the camera.
    pub fn clear_transient(&mut self) {
        self.sel_verts.clear();
        self.sel_edges.clear();
        self.sel_faces.clear();
        self.hover_face = None;
        self.drag = None;
        self.op_drag = None;
        self.stroke_edits.clear();
        self.stroke_painted.clear();
        self.last_paint = None;
    }
}

/// Default material tones for fresh faces: the theme's canvas checkerboard
/// pair (Theme::default().checker_light / checker_dark), so an untextured
/// model looks like the familiar empty-canvas checker.
pub const DEFAULT_FACE_A: Rgba = [197, 204, 218, 255]; // #C5CCDA
pub const DEFAULT_FACE_B: Rgba = [170, 178, 194, 255]; // #AAB2C2

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

/// Fill every allocated island with the default 1-texel checkerboard
/// (island-local parity, so every face's pattern starts the same).
pub fn paint_islands_checker(layer: &mut Layer, mesh: &mesh::Mesh) {
    for face in &mesh.faces {
        let isl = face.island;
        for ty in 0..isl.h {
            for tx in 0..isl.w {
                let c = if (tx + ty) % 2 == 0 { DEFAULT_FACE_A } else { DEFAULT_FACE_B };
                layer.set_pixel((isl.x + tx) as u32, (isl.y + ty) as u32, c);
            }
        }
    }
}
