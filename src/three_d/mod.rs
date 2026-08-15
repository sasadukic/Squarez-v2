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
    /// Uniform object scale; operates on `verts` instead of `face`.
    Scale,
}

/// An in-flight extrude/inset/scale drag: pristine mesh + layer snapshots
/// taken at press time so every preview amount recomputes from clean state.
#[derive(Debug, Clone)]
pub struct OpDrag {
    pub kind: OpKind,
    pub face: u32,
    /// Vertex set for Scale (the clicked object's vertices).
    pub verts: Vec<u32>,
    pub start_mesh: mesh::Mesh,
    pub start_layer: Layer,
    pub raw: f32,
    pub applied: i32,
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
    /// Atlas texel under the cursor (paint tools) — drives the on-model
    /// brush preview.
    pub hover_texel: Option<(u32, u32)>,
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
            hover_texel: None,
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
    /// Abandon any in-flight move/extrude/inset/scale gesture without
    /// applying its snapshots. Used when history moves under the gesture
    /// (undo/redo), where replaying a stale snapshot would revert work.
    pub fn cancel_gesture(&mut self) {
        self.drag = None;
        self.op_drag = None;
    }

    /// Drop transient gesture state (selection, stroke) but keep the camera.
    pub fn clear_transient(&mut self) {
        self.sel_verts.clear();
        self.sel_edges.clear();
        self.sel_faces.clear();
        self.hover_face = None;
        self.hover_texel = None;
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

/// Dilate every island's border colors one texel outward into its gutter
/// ring. Run on the composited atlas right before GPU upload: fragments
/// that sample just past an island boundary then read the face's own edge
/// color instead of empty gutter — no seams at face boundaries.
pub fn pad_island_gutters(pixels: &mut [u8], atlas_w: u32, atlas_h: u32, mesh: &mesh::Mesh) {
    let idx = |x: i64, y: i64| -> Option<usize> {
        if x < 0 || y < 0 || x >= atlas_w as i64 || y >= atlas_h as i64 {
            None
        } else {
            Some(((y as u32 * atlas_w + x as u32) * 4) as usize)
        }
    };
    let copy = |sx: i64, sy: i64, dx: i64, dy: i64, px: &mut [u8]| {
        if let (Some(si), Some(di)) = (idx(sx, sy), idx(dx, dy)) {
            let (a, b) = if si < di {
                let (lo, hi) = px.split_at_mut(di);
                (&lo[si..si + 4], &mut hi[..4])
            } else if di < si {
                let (lo, hi) = px.split_at_mut(si);
                (&hi[..4], &mut lo[di..di + 4])
            } else {
                return;
            };
            b.copy_from_slice(a);
        }
    };
    for face in &mesh.faces {
        let isl = face.island;
        if isl.w == 0 || isl.h == 0 {
            continue;
        }
        let (x0, y0) = (isl.x as i64, isl.y as i64);
        let (x1, y1) = (x0 + isl.w as i64 - 1, y0 + isl.h as i64 - 1);
        for x in x0..=x1 {
            copy(x, y0, x, y0 - 1, pixels);
            copy(x, y1, x, y1 + 1, pixels);
        }
        for y in y0..=y1 {
            copy(x0, y, x0 - 1, y, pixels);
            copy(x1, y, x1 + 1, y, pixels);
        }
        // Corners
        copy(x0, y0, x0 - 1, y0 - 1, pixels);
        copy(x1, y0, x1 + 1, y0 - 1, pixels);
        copy(x0, y1, x0 - 1, y1 + 1, pixels);
        copy(x1, y1, x1 + 1, y1 + 1, pixels);
    }
}

/// Heal historical island damage: older builds filled newly grown island
/// rows/columns with checker, baking light strips into painted faces.
/// For every island whose outermost ring consists solely of the two
/// default checker tones while the ring inside it holds real paint,
/// overwrite the rim by clamp-extending the inner colors (up to 2 rings).
/// Fully-checker (unpainted) islands are left untouched.
/// Returns true if anything changed.
pub fn heal_checker_rims(layer: &mut Layer, mesh: &mesh::Mesh) -> bool {
    let is_default = |c: Rgba| c == DEFAULT_FACE_A || c == DEFAULT_FACE_B;
    let mut changed = false;
    for face in &mesh.faces {
        let isl = face.island;
        for pass in 0..2u16 {
            if isl.w <= 2 * (pass + 1) || isl.h <= 2 * (pass + 1) {
                break;
            }
            let (x0, y0) = ((isl.x + pass) as u32, (isl.y + pass) as u32);
            let (x1, y1) = (
                (isl.x + isl.w - 1 - pass) as u32,
                (isl.y + isl.h - 1 - pass) as u32,
            );
            let on_ring = |x: u32, y: u32| x == x0 || x == x1 || y == y0 || y == y1;
            let mut ring_all_default = true;
            let mut inner_has_paint = false;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let c = layer.get_pixel(x, y);
                    if on_ring(x, y) {
                        if !is_default(c) {
                            ring_all_default = false;
                        }
                    } else if !is_default(c) {
                        inner_has_paint = true;
                    }
                }
            }
            if !(ring_all_default && inner_has_paint) {
                break;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if on_ring(x, y) {
                        let sx = x.clamp(x0 + 1, x1 - 1);
                        let sy = y.clamp(y0 + 1, y1 - 1);
                        let c = layer.get_pixel(sx, sy);
                        layer.set_pixel(x, y, c);
                    }
                }
            }
            changed = true;
        }
    }
    changed
}

/// Load-time migration: files saved with older, tighter island packing
/// can't be seam-padded (islands share gutter texels). Repack their atlas
/// with current gutters (growing it if needed) and heal checker rims baked
/// into painted faces by old island-growth behavior. No-op for healthy
/// files; returns true if the project was modified.
pub fn migrate_gutters(project: &mut crate::project::Project) -> bool {
    if !project.mode.is_three_d() {
        return false;
    }
    let Some(mesh) = project.mesh3d.clone() else { return false };
    if !edit::islands_need_repack(&mesh) {
        // Healthy packing — still heal any checker rims baked into painted
        // faces by older builds (conservative + idempotent).
        let frame = &mut project.animations[0].frames[0];
        if let Some(layer) = frame.layers.first_mut() {
            if heal_checker_rims(layer, &mesh) {
                frame.dirty = true;
                return true;
            }
        }
        return false;
    }
    loop {
        let atlas = (project.canvas_width, project.canvas_height);
        let Some(layer) = project.animations[0].frames[0].layers.first() else { return false };
        match edit::repack_islands(&mesh, layer, atlas) {
            Ok(outcome) => {
                let repacked = outcome.mesh;
                let frame = &mut project.animations[0].frames[0];
                if let Some(layer) = frame.layers.first_mut() {
                    for &(x, y, _, new) in &outcome.pixel_edits {
                        layer.set_pixel(x, y, new);
                    }
                    heal_checker_rims(layer, &repacked);
                }
                frame.dirty = true;
                project.mesh3d = Some(repacked);
                return true;
            }
            Err(mesh::AtlasFull) => {
                if project.canvas_height >= 4096 {
                    return false;
                }
                let w = project.canvas_width;
                let new_h = project.canvas_height * 2;
                for anim in &mut project.animations {
                    for frame in &mut anim.frames {
                        frame.resize_canvas(w, new_h);
                    }
                }
                project.canvas_height = new_h;
            }
        }
    }
}
