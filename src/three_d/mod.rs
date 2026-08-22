// src/three_d/mod.rs
//
// The 3D modeling + texturing mode. All 3D-specific logic lives in this
// module tree; src/app.rs only dispatches into it.

pub mod camera;
pub mod edit;
pub mod gizmo;
pub mod layout;
pub mod mesh;
pub mod paint;
pub mod render;
pub mod workspace;

use crate::project::{Layer, Project, Rgba};

/// Hard cap on atlas growth, per axis, in texels.
pub const MAX_ATLAS_SIDE: u32 = 4096;

/// Grow the atlas — which is the canvas — toward `need`, doubling each axis
/// that falls short, capped at `MAX_ATLAS_SIDE`. Layer content is preserved
/// (top-left anchored) by `Frame::resize_canvas`.
///
/// Returns false when neither axis could grow; the caller must then degrade
/// rather than retry, or it spins.
pub fn grow_atlas(project: &mut Project, need_w: u32, need_h: u32) -> bool {
    let grow = |mut side: u32, need: u32| {
        while side < need && side < MAX_ATLAS_SIDE {
            side = (side.max(1) * 2).min(MAX_ATLAS_SIDE);
        }
        side
    };
    let w = grow(project.canvas_width, need_w);
    let h = grow(project.canvas_height, need_h);
    if w == project.canvas_width && h == project.canvas_height {
        return false;
    }
    for anim in &mut project.animations {
        for frame in &mut anim.frames {
            frame.resize_canvas(w, h);
        }
    }
    project.canvas_width = w;
    project.canvas_height = h;
    true
}

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
    /// How faces are lit in the viewport. Snapped views always render flat
    /// so texel colors read true regardless of this setting.
    pub shading: Shading,
    /// Main window shows the texture for direct 2D painting; the model moves
    /// to the preview panel. Toggled with Tab.
    pub texture_view: bool,
    /// The one-time atlas size prompt was already offered for this tab.
    pub atlas_prompted: bool,
    /// In-flight island drag in the texture view: (start mouse texel,
    /// current integer delta).
    pub island_drag: Option<((i32, i32), (i32, i32))>,
}

/// Viewport lighting style, cycled by the workspace's shading button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shading {
    /// Smooth per-face warm/cool tint (Blender-solid-like).
    Soft,
    /// Flat texel colors with quantized shadows as screen-space dither
    /// patterns — the picoCAD look.
    Dither,
    /// Raw texel colors, no lighting, no wireframe: a clean preview.
    Off,
}

impl Shading {
    pub fn label(self) -> &'static str {
        match self {
            Shading::Soft => "Shading: Soft",
            Shading::Dither => "Shading: Dither",
            Shading::Off => "Shading: Off",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Shading::Soft => Shading::Dither,
            Shading::Dither => Shading::Off,
            Shading::Off => Shading::Soft,
        }
    }
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
            shading: Shading::Dither,
            texture_view: false,
            atlas_prompted: false,
            island_drag: None,
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

/// Fill every allocated island with the default 1-texel checkerboard.
/// Parity is atlas-global (see `edit::default_texel`), so the pattern runs
/// continuously across faces that abut in a projected layout.
pub fn paint_islands_checker(layer: &mut Layer, mesh: &mesh::Mesh) {
    for face in &mesh.faces {
        let isl = face.island;
        for ty in 0..isl.h {
            for tx in 0..isl.w {
                let (x, y) = ((isl.x + tx) as u32, (isl.y + ty) as u32);
                layer.set_pixel(x, y, edit::default_texel(x, y));
            }
        }
    }
}

/// Dilate every island's border colors one texel outward into its gutter
/// ring. Run on the composited atlas right before GPU upload: fragments
/// that sample just past an island boundary then read the face's own edge
/// color instead of empty gutter — no seams at face boundaries.
///
/// Only unclaimed texels are written. In a projected layout islands sit flush
/// against each other, and dilating into a neighbour would overwrite its real
/// paint with a stranger's edge color. Those shared boundaries need no
/// dilation anyway: `mesh::UV_INSET` already keeps every fragment inside its
/// own island, so nothing ever samples across them.
pub fn pad_island_gutters(pixels: &mut [u8], atlas_w: u32, atlas_h: u32, mesh: &mesh::Mesh) {
    let idx = |x: i64, y: i64| -> Option<usize> {
        if x < 0 || y < 0 || x >= atlas_w as i64 || y >= atlas_h as i64 {
            None
        } else {
            Some(((y as u32 * atlas_w + x as u32) * 4) as usize)
        }
    };

    // Occupancy as a bitset, not a Vec<bool>: this runs on every canvas
    // rebuild, and at a 4096x4096 atlas a byte per texel would be 16 MB of
    // allocation per frame while painting.
    let texels = (atlas_w as usize) * (atlas_h as usize);
    let mut claimed = vec![0u64; texels.div_ceil(64)];
    for face in &mesh.faces {
        let isl = face.island;
        for y in isl.y as u32..(isl.y as u32 + isl.h as u32).min(atlas_h) {
            for x in isl.x as u32..(isl.x as u32 + isl.w as u32).min(atlas_w) {
                let bit = (y as usize) * (atlas_w as usize) + x as usize;
                claimed[bit / 64] |= 1u64 << (bit % 64);
            }
        }
    }
    let is_claimed = |x: i64, y: i64| -> bool {
        if x < 0 || y < 0 || x >= atlas_w as i64 || y >= atlas_h as i64 {
            return true; // off-atlas: nothing to write there either
        }
        let bit = (y as usize) * (atlas_w as usize) + x as usize;
        claimed[bit / 64] & (1u64 << (bit % 64)) != 0
    };

    let copy = |sx: i64, sy: i64, dx: i64, dy: i64, px: &mut [u8]| {
        if is_claimed(dx, dy) {
            return;
        }
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

/// Load-time migration: move a model laid out by the old shelf packer onto
/// the projected layout, carrying its paint. No-op for files already laid out
/// that way; returns true if the project was modified.
///
/// The move is lossless. Island *sizes* are a pure function of the mesh and
/// are unchanged by this layout, so it is an exact permutation of
/// identically-sized rects, and the v-flip is an exact row mirror.
///
/// It always mirrors, because any file that still needs migrating was written
/// before the flip — a file already in the projected layout is detected as
/// canonical above and left alone, which also makes this idempotent.
pub fn migrate_layout(project: &mut crate::project::Project) -> bool {
    if !project.mode.is_three_d() {
        return false;
    }
    let Some(mesh) = project.mesh3d.clone() else { return false };
    if mesh.manual_layout {
        // Hand-packed by the user — never "migrate" that back to the
        // automatic arrangement.
        return false;
    }
    let atlas = (project.canvas_width, project.canvas_height);
    if !edit::islands_need_repack(&mesh, atlas) {
        // Already laid out — still heal any checker rims baked into painted
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
        match edit::relayout_existing(&mesh, layer, atlas, true) {
            Ok(outcome) => {
                let moved = outcome.mesh;
                let frame = &mut project.animations[0].frames[0];
                if let Some(layer) = frame.layers.first_mut() {
                    for &(x, y, _, new) in &outcome.pixel_edits {
                        layer.set_pixel(x, y, new);
                    }
                    heal_checker_rims(layer, &moved);
                }
                frame.dirty = true;
                project.mesh3d = Some(moved);
                return true;
            }
            Err(need) => {
                if !grow_atlas(project, need.need_w, need.need_h) {
                    return false;
                }
            }
        }
    }
}
