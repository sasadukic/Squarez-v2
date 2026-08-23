// src/three_d/paint.rs
//
// Painting directly on the model: cursor → front-most triangle → barycentric
// → atlas texel → the existing pure tool functions against the active layer.
// One Command::PaintPixels per stroke, exactly like the 2D canvas.

use egui::Pos2;

use super::render::Scene;
use super::ThreeDState;
use crate::color::ColorState;
use crate::history::{Command, UndoStack};
use crate::project::{Layer, Project, Rgba};
use crate::tools::{apply_eraser, apply_pencil, bresenham_positions, ActiveTool};

/// Result of one frame of paint handling.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaintResult {
    pub canvas_dirty: bool,
    pub modified: bool,
}

/// A picked point on the model: face index plus the atlas texel under the
/// cursor (clamped into the face's island).
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub face: u32,
    pub texel: (i64, i64),
}

/// Barycentric coordinates of `p` in triangle `t`, or None if outside
/// (or the triangle is degenerate).
fn barycentric(p: Pos2, t: [Pos2; 3]) -> Option<(f32, f32, f32)> {
    let v0 = t[1] - t[0];
    let v1 = t[2] - t[0];
    let v2 = p - t[0];
    let d00 = v0.dot(v0);
    let d01 = v0.dot(v1);
    let d11 = v1.dot(v1);
    let d20 = v2.dot(v0);
    let d21 = v2.dot(v1);
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-6 {
        return None;
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    const EPS: f32 = 1e-4;
    if u >= -EPS && v >= -EPS && w >= -EPS {
        Some((u, v, w))
    } else {
        None
    }
}

/// Find the front-most face + texel under the cursor.
pub fn pick(scene: &Scene, pos: Pos2, mesh: &super::mesh::Mesh, atlas: (u32, u32)) -> Option<Hit> {
    // tris are sorted far → near; walk them near → far.
    for tri in scene.tris.iter().rev() {
        if let Some((u, v, w)) = barycentric(pos, tri.pts) {
            let uv_x = tri.uvs[0].x * u + tri.uvs[1].x * v + tri.uvs[2].x * w;
            let uv_y = tri.uvs[0].y * u + tri.uvs[1].y * v + tri.uvs[2].y * w;
            let isl = mesh.faces[tri.face as usize].island;
            // A zero-size island would make the clamp below have min > max,
            // which panics. The layout guarantees w,h >= 1, but a face can
            // legitimately be unallocated (e.g. mid-import), so skip it rather
            // than take the whole app down.
            if isl.w == 0 || isl.h == 0 {
                continue;
            }
            let tx = (uv_x * atlas.0 as f32).floor() as i64;
            let ty = (uv_y * atlas.1 as f32).floor() as i64;
            let tx = tx.clamp(isl.x as i64, (isl.x + isl.w) as i64 - 1);
            let ty = ty.clamp(isl.y as i64, (isl.y + isl.h) as i64 - 1);
            return Some(Hit { face: tri.face, texel: (tx, ty) });
        }
    }
    None
}

/// Whether this tool paints on the model.
pub fn is_paint_tool(tool: &ActiveTool) -> bool {
    matches!(
        tool,
        ActiveTool::Pencil
            | ActiveTool::Eraser
            | ActiveTool::Fill
            | ActiveTool::Eyedropper
            | ActiveTool::Gradient
    )
}

/// Project a screen position onto `face`'s plane, returning UNCLAMPED float
/// atlas texel coordinates. Unlike `pick`, this evaluates barycentric coords
/// without the inside test — the affine map extrapolates past the triangle —
/// so a gradient axis endpoint can land beyond the face's edge. Returns None
/// only when every screen triangle of the face is degenerate (edge-on view).
pub fn pick_on_face_plane(
    scene: &Scene,
    face: u32,
    pos: Pos2,
    atlas: (u32, u32),
) -> Option<(f32, f32)> {
    for tri in scene.tris.iter() {
        if tri.face != face {
            continue;
        }
        let t = tri.pts;
        let v0 = t[1] - t[0];
        let v1 = t[2] - t[0];
        let v2 = pos - t[0];
        let d00 = v0.dot(v0);
        let d01 = v0.dot(v1);
        let d11 = v1.dot(v1);
        let d20 = v2.dot(v0);
        let d21 = v2.dot(v1);
        let denom = d00 * d11 - d01 * d01;
        if denom.abs() < 1e-6 {
            continue;
        }
        let v = (d11 * d20 - d01 * d21) / denom;
        let w = (d00 * d21 - d01 * d20) / denom;
        let u = 1.0 - v - w;
        let uv_x = tri.uvs[0].x * u + tri.uvs[1].x * v + tri.uvs[2].x * w;
        let uv_y = tri.uvs[0].y * u + tri.uvs[1].y * v + tri.uvs[2].y * w;
        return Some((uv_x * atlas.0 as f32, uv_y * atlas.1 as f32));
    }
    None
}

fn layer_paintable(layer: &Layer) -> bool {
    !layer.locked && !layer.is_group && layer.background_color.is_none() && !layer.pixels.is_empty()
}

/// Apply a single-texel tool edit, recording undo data once per stroke texel.
fn paint_texel(
    state: &mut ThreeDState,
    layer: &mut Layer,
    tool: &ActiveTool,
    color: Rgba,
    tx: i64,
    ty: i64,
    isl: super::mesh::Island,
) -> bool {
    if tx < isl.x as i64 || ty < isl.y as i64 {
        return false;
    }
    let (x, y) = (tx as u32, ty as u32);
    if x >= (isl.x + isl.w) as u32 || y >= (isl.y + isl.h) as u32 {
        return false;
    }
    if state.stroke_painted.contains(&(x, y)) {
        return false;
    }
    let edits = match tool {
        ActiveTool::Pencil => apply_pencil(layer, x, y, color),
        ActiveTool::Eraser => apply_eraser(layer, x, y),
        _ => return false,
    };
    let mut changed = false;
    for (ex, ey, old, new) in edits {
        layer.set_pixel(ex, ey, new);
        state.stroke_edits.push((ex, ey, old, new));
        changed = true;
    }
    state.stroke_painted.insert((x, y));
    changed
}

/// Fill a face's entire island with `replacement` — the bucket paints the
/// whole face, not a flood region (a flood would stop after one texel on
/// the default checkerboard).
pub fn fill_island(
    layer: &mut Layer,
    isl: super::mesh::Island,
    replacement: Rgba,
) -> Vec<(u32, u32, Rgba, Rgba)> {
    let mut edits = Vec::new();
    for y in 0..isl.h as u32 {
        for x in 0..isl.w as u32 {
            let (gx, gy) = (isl.x as u32 + x, isl.y as u32 + y);
            let old = layer.get_pixel(gx, gy);
            if old != replacement {
                edits.push((gx, gy, old, replacement));
            }
        }
    }
    edits
}

/// Fill exactly the texels `face` owns — the island texels whose centers lie
/// inside the face's projected outline.
///
/// The island is a bounding RECTANGLE, and in the projected layout a
/// triangle's rectangle legitimately overlaps its coplanar neighbours'
/// texels (a fan-triangulated cap tiles the disc, but every triangle's bbox
/// covers parts of the others). Filling the whole rect would recolor ground
/// the neighbours are standing on. Ownership by texel center is the same
/// rule the layout uses, so the fill seam lands where the rendered edge is.
pub fn fill_face(
    layer: &mut Layer,
    mesh: &super::mesh::Mesh,
    face_idx: u32,
    replacement: Rgba,
) -> Vec<(u32, u32, Rgba, Rgba)> {
    let Some(clip) = FaceClip::new(mesh, face_idx) else {
        return Vec::new();
    };
    let isl = clip.isl;
    let mut edits = Vec::new();
    for y in 0..isl.h as u32 {
        for x in 0..isl.w as u32 {
            let (gx, gy) = (isl.x as u32 + x, isl.y as u32 + y);
            if !clip.contains(gx, gy) {
                continue;
            }
            let old = layer.get_pixel(gx, gy);
            if old != replacement {
                edits.push((gx, gy, old, replacement));
            }
        }
    }
    edits
}

/// A face's texel-ownership test: which absolute atlas texels' CENTERS lie
/// inside the face's projected outline. This is the same rule `fill_face`,
/// the layout, and the rendered edge use, extracted so other face-scoped
/// paint (gradients) clips identically.
pub struct FaceClip {
    pub isl: super::mesh::Island,
    poly: Vec<(f32, f32)>,
    origin: (f32, f32), // (min_u, min_v): plane coords of the island's corner
    /// Island-local (row-major) mask of texels center-claimed by ANOTHER
    /// face whose island overlaps this one (coplanar contests). Those stay
    /// the neighbour's to paint.
    foreign: Vec<bool>,
}

fn point_in_poly(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        if (y0 > p.1) != (y1 > p.1) {
            let t = (p.1 - y0) / (y1 - y0);
            if p.0 < x0 + t * (x1 - x0) {
                inside = !inside;
            }
        }
    }
    inside
}

fn segments_intersect(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    let cross = |o: (f32, f32), p: (f32, f32), q: (f32, f32)| {
        (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0)
    };
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// Does the polygon touch any part of the unit square with min corner `s`?
fn poly_touches_square(poly: &[(f32, f32)], s: (f32, f32)) -> bool {
    let corners = [s, (s.0 + 1.0, s.1), (s.0 + 1.0, s.1 + 1.0), (s.0, s.1 + 1.0)];
    if corners.iter().any(|&c| point_in_poly(poly, c)) {
        return true;
    }
    if poly
        .iter()
        .any(|&(px, py)| px >= s.0 && px <= s.0 + 1.0 && py >= s.1 && py <= s.1 + 1.0)
    {
        return true;
    }
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        for j in 0..4 {
            if segments_intersect(a, b, corners[j], corners[(j + 1) % 4]) {
                return true;
            }
        }
    }
    false
}

impl FaceClip {
    pub fn new(mesh: &super::mesh::Mesh, face_idx: u32) -> Option<Self> {
        let face = mesh.faces.get(face_idx as usize)?;
        let isl = face.island;
        if isl.w == 0 || isl.h == 0 {
            return None;
        }
        let basis = mesh.face_plane_basis(face);
        let (min_u, min_v, _, _) = mesh.face_uv_bounds(face);
        let poly: Vec<(f32, f32)> = face
            .verts
            .iter()
            .map(|&vi| basis.project(mesh.vertices[vi as usize]))
            .collect();

        // Mark texels a coplanar neighbour center-claims: its island overlaps
        // ours (a layout contest) and the texel center falls inside ITS
        // outline in ITS plane coords. Painting those would recolor ground
        // the neighbour is standing on.
        let mut foreign = vec![false; isl.w as usize * isl.h as usize];
        for (oi, other) in mesh.faces.iter().enumerate() {
            if oi == face_idx as usize || other.island.w == 0 {
                continue;
            }
            let o = other.island;
            let x0 = isl.x.max(o.x);
            let y0 = isl.y.max(o.y);
            let x1 = (isl.x + isl.w).min(o.x + o.w);
            let y1 = (isl.y + isl.h).min(o.y + o.h);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let obasis = mesh.face_plane_basis(other);
            let (o_min_u, o_min_v, _, _) = mesh.face_uv_bounds(other);
            let opoly: Vec<(f32, f32)> = other
                .verts
                .iter()
                .map(|&vi| obasis.project(mesh.vertices[vi as usize]))
                .collect();
            for gy in y0..y1 {
                for gx in x0..x1 {
                    let p = (
                        o_min_u + (gx - o.x) as f32 + 0.5,
                        o_min_v + (gy - o.y) as f32 + 0.5,
                    );
                    if point_in_poly(&opoly, p) {
                        let li = (gy - isl.y) as usize * isl.w as usize + (gx - isl.x) as usize;
                        foreign[li] = true;
                    }
                }
            }
        }
        Some(Self { isl, poly, origin: (min_u, min_v), foreign })
    }

    /// A face owns every island texel its outline touches — not only the
    /// center-inside ones, or slanted edges leave rendered-but-unpaintable
    /// slivers — minus texels a coplanar neighbour center-claims.
    pub fn contains(&self, gx: u32, gy: u32) -> bool {
        let isl = self.isl;
        if gx < isl.x as u32
            || gy < isl.y as u32
            || gx >= (isl.x + isl.w) as u32
            || gy >= (isl.y + isl.h) as u32
        {
            return false;
        }
        let (lx, ly) = (gx - isl.x as u32, gy - isl.y as u32);
        if self.foreign[ly as usize * isl.w as usize + lx as usize] {
            return false;
        }
        let s = (self.origin.0 + lx as f32, self.origin.1 + ly as f32);
        if point_in_poly(&self.poly, (s.0 + 0.5, s.1 + 0.5)) {
            return true;
        }
        poly_touches_square(&self.poly, s)
    }
}

/// Handle one frame of paint input. Call with the scene already built for
/// this frame; mutates the active layer and pushes undo commands on release.
#[allow(clippy::too_many_arguments)]
pub fn handle(
    state: &mut ThreeDState,
    project: &mut Project,
    undo: &mut UndoStack,
    color_state: &mut ColorState,
    tool: &ActiveTool,
    scene: &Scene,
    response: &egui::Response,
    ui: &egui::Ui,
    gradient_style: crate::tools::GradientStyle,
    gradient_ramp: Option<&[Rgba]>,
) -> PaintResult {
    let mut result = PaintResult::default();
    if !is_paint_tool(tool) || project.mesh3d.is_none() {
        return result;
    }

    let atlas = (project.canvas_width, project.canvas_height);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let li = project.active_layer;

    // Hover feedback for the workspace to draw (face + exact texel).
    let hover = pointer.and_then(|p| {
        let mesh = project.mesh3d.as_ref().unwrap();
        pick(scene, p, mesh, atlas)
    });
    state.hover_face = hover.map(|h| h.face);
    state.hover_texel = hover.map(|h| (h.texel.0 as u32, h.texel.1 as u32));

    let press_started = response.drag_started_by(egui::PointerButton::Primary)
        || (ui.input(|i| i.pointer.primary_pressed()) && response.hovered());
    if press_started {
        state.stroke_edits.clear();
        state.stroke_painted.clear();
        state.last_paint = None;
    }

    // Gradient: its own press/drag/commit cycle. The drag is locked to the
    // face under the press; the endpoint tracks the face PLANE (extrapolated
    // past the edge), and the preview lives in gradient_preview — never the
    // layer — so the generic stroke machinery below stays untouched.
    if matches!(tool, ActiveTool::Gradient) {
        // A gradient draws the user's palette selection; without at least
        // two selected colors there is nothing to blend, so no drag starts.
        let Some(colors) = gradient_ramp else {
            state.gradient_drag = None;
            if !state.gradient_preview.is_empty() {
                state.gradient_preview.clear();
                result.canvas_dirty = true;
            }
            return result;
        };
        // Same press idiom as the workspace gestures (pressed + rect
        // containment) — Response::hovered() is not reliable on the press
        // frame.
        let grad_press = ui.input(|i| i.pointer.primary_pressed())
            && pointer.is_some_and(|p| response.rect.contains(p));
        if grad_press {
            if let Some(pos) = pointer {
                let mesh = project.mesh3d.as_ref().unwrap();
                if let Some(hit) = pick(scene, pos, mesh, atlas) {
                    let c = (hit.texel.0 as f32 + 0.5, hit.texel.1 as f32 + 0.5);
                    state.gradient_drag =
                        Some(super::GradientDrag { face: hit.face, start: c, end: c });
                }
            }
        }
        if primary_down && !primary_released {
            if let Some(mut drag) = state.gradient_drag {
                let mesh = project.mesh3d.as_ref().unwrap();
                if let Some(pos) = pointer {
                    if let Some(end) = pick_on_face_plane(scene, drag.face, pos, atlas) {
                        // Axis locked to the 8 pixel-art directions.
                        drag.end = crate::tools::snap_axis_8(drag.start, end);
                    }
                }
                let clip = FaceClip::new(mesh, drag.face);
                state.gradient_drag = Some(drag);
                if let Some(clip) = clip {
                    let frame = &project.animations[0].frames[0];
                    if let Some(layer) = frame.layers.get(li) {
                        if layer_paintable(layer) {
                            let isl = clip.isl;
                            let preview: Vec<(u32, u32, Rgba)> = crate::tools::apply_gradient(
                                layer,
                                (isl.x as u32, isl.y as u32, isl.w as u32, isl.h as u32),
                                |x, y| clip.contains(x, y),
                                drag.start,
                                drag.end,
                                gradient_style,
                                colors,
                            )
                            .into_iter()
                            .map(|(x, y, _, new)| (x, y, new))
                            .collect();
                            if preview != state.gradient_preview {
                                state.gradient_preview = preview;
                                result.canvas_dirty = true;
                            }
                        }
                    }
                }
            }
        } else if let Some(drag) = state.gradient_drag.take() {
            state.gradient_preview.clear();
            let clip = FaceClip::new(project.mesh3d.as_ref().unwrap(), drag.face);
            let frame = &mut project.animations[0].frames[0];
            if let (Some(clip), Some(layer)) = (clip, frame.layers.get_mut(li)) {
                if layer_paintable(layer) {
                    let isl = clip.isl;
                    let edits = crate::tools::apply_gradient(
                        layer,
                        (isl.x as u32, isl.y as u32, isl.w as u32, isl.h as u32),
                        |x, y| clip.contains(x, y),
                        drag.start,
                        drag.end,
                        gradient_style,
                        colors,
                    );
                    if !edits.is_empty() {
                        for &(x, y, _, new) in &edits {
                            layer.set_pixel(x, y, new);
                        }
                        frame.dirty = true;
                        undo.push(Command::PaintPixels {
                            animation_id: 0,
                            frame_id: 0,
                            layer_id: li,
                            edits,
                        });
                        result.modified = true;
                    }
                }
            }
            result.canvas_dirty = true;
        }
        return result;
    }

    let painting = primary_down && (response.dragged_by(egui::PointerButton::Primary) || response.hovered());
    if painting {
        if let Some(pos) = pointer {
            let mesh = project.mesh3d.as_ref().unwrap();
            if let Some(hit) = pick(scene, pos, mesh, atlas) {
                let isl = mesh.faces[hit.face as usize].island;
                let frame = &mut project.animations[0].frames[0];
                if let Some(layer) = frame.layers.get_mut(li) {
                    if layer_paintable(layer) {
                        match tool {
                            ActiveTool::Pencil | ActiveTool::Eraser => {
                                let color = color_state.foreground;
                                let mut changed = false;
                                // Stroke continuity within one face only.
                                if let Some((last_face, (lx, ly))) = state.last_paint {
                                    if last_face == hit.face && (lx, ly) != hit.texel {
                                        for (px, py) in bresenham_positions(
                                            lx as i32,
                                            ly as i32,
                                            hit.texel.0 as i32,
                                            hit.texel.1 as i32,
                                        ) {
                                            changed |= paint_texel(
                                                state, layer, tool, color, px as i64, py as i64, isl,
                                            );
                                        }
                                    }
                                }
                                changed |= paint_texel(
                                    state, layer, tool, color, hit.texel.0, hit.texel.1, isl,
                                );
                                if changed {
                                    frame.dirty = true;
                                    result.canvas_dirty = true;
                                }
                                state.last_paint = Some((hit.face, hit.texel));
                            }
                            // Fill acts once per press, not continuously.
                            ActiveTool::Fill if press_started => {
                                let edits = fill_face(layer, mesh, hit.face, color_state.foreground);
                                if !edits.is_empty() {
                                    for &(x, y, _, new) in &edits {
                                        layer.set_pixel(x, y, new);
                                    }
                                    state.stroke_edits.extend(edits);
                                    frame.dirty = true;
                                    result.canvas_dirty = true;
                                }
                            }
                            ActiveTool::Eyedropper => {
                                let c = layer.get_pixel(hit.texel.0 as u32, hit.texel.1 as u32);
                                if c[3] > 0 {
                                    color_state.foreground = c;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if primary_released && !state.stroke_edits.is_empty() {
        let edits = std::mem::take(&mut state.stroke_edits);
        state.stroke_painted.clear();
        state.last_paint = None;
        undo.push(Command::PaintPixels { animation_id: 0, frame_id: 0, layer_id: li, edits });
        result.modified = true;
    }
    if primary_released {
        state.last_paint = None;
    }

    result
}
