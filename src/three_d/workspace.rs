// src/three_d/workspace.rs
//
// The 3D workspace: owns all input inside the canvas rect — navigation
// (orbit/pan/zoom/snap views), painting on the model, and modeling
// (vertex move, face select, extrude, delete, add primitives) — and
// renders the textured model with grid + wireframe + selection overlays.

use egui::{Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Vec2};

use super::camera::SnapView;
use super::edit::{self, EditOutcome};
use super::mesh::{AtlasFull, Mesh};
use super::{gizmo, paint, render, OpDrag, OpKind, ThreeDState, VertexDrag};
use crate::canvas::CanvasState;
use crate::color::ColorState;
use crate::history::{Command, UndoStack};
use crate::project::{Layer, Project};
use crate::theme::Theme;
use crate::tools::ActiveTool;

/// What the 3D workspace wants the app to do after this frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct Output {
    /// Texture/composite changed — app should rebuild the canvas texture.
    pub canvas_dirty: bool,
    /// Document changed — app should mark the tab modified.
    pub modified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Extrude,
    Delete,
    CreateFace,
}

const VERTEX_HIT_RADIUS: f32 = 8.0;

/// Run a mesh operation, growing the atlas (preserving content) toward
/// whatever dimensions the packer reports it needed. Gives up once the atlas
/// can no longer grow in the axis that fell short.
fn with_atlas_growth<F>(project: &mut Project, layer_idx: usize, mut op: F) -> Option<EditOutcome>
where
    F: FnMut(&Mesh, &Layer, (u32, u32)) -> Result<EditOutcome, AtlasFull>,
{
    loop {
        let atlas = (project.canvas_width, project.canvas_height);
        let mesh = project.mesh3d.as_ref()?;
        let layer = project.animations[0].frames[0].layers.get(layer_idx)?;
        match op(mesh, layer, atlas) {
            Ok(outcome) => return Some(outcome),
            Err(need) => {
                if !super::grow_atlas(project, need.need_w, need.need_h) {
                    return None;
                }
            }
        }
    }
}

/// Apply an edit outcome: write island pixels, swap in the new mesh, push
/// one MeshEdit command, and adopt the outcome's selection.
fn commit_edit(
    state: &mut ThreeDState,
    project: &mut Project,
    undo: &mut UndoStack,
    layer_idx: usize,
    before: Mesh,
    outcome: EditOutcome,
    output: &mut Output,
) {
    let frame = &mut project.animations[0].frames[0];
    if let Some(layer) = frame.layers.get_mut(layer_idx) {
        for &(x, y, _, new) in &outcome.pixel_edits {
            layer.set_pixel(x, y, new);
        }
    }
    frame.dirty = true;
    undo.push(Command::MeshEdit {
        before,
        after: outcome.mesh.clone(),
        layer_id: layer_idx,
        pixel_edits: outcome.pixel_edits,
    });
    project.mesh3d = Some(outcome.mesh);
    state.sel_faces = outcome.select_faces;
    state.sel_verts = outcome.select_verts;
    output.canvas_dirty = true;
    output.modified = true;
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// For the Loop Cut tool: the face edge nearest the cursor plus the cut
/// fraction along it, snapped to whole world units (never on a corner).
fn face_edge_param(
    mesh: &Mesh,
    face_idx: u32,
    cam: &super::camera::Camera3D,
    rect: Rect,
    pos: Pos2,
) -> Option<(usize, f32)> {
    let face = mesh.faces.get(face_idx as usize)?;
    if face.verts.len() != 4 {
        return None;
    }
    let mut best: Option<(usize, f32, f32)> = None; // (entry_pos, raw s, screen dist)
    for i in 0..4 {
        let a = mesh.vertices[face.verts[i] as usize];
        let b = mesh.vertices[face.verts[(i + 1) % 4] as usize];
        let (pa, _) = cam.project(a, rect);
        let (pb, _) = cam.project(b, rect);
        let ab = pb - pa;
        let len2 = ab.length_sq();
        if len2 < 1e-6 {
            continue;
        }
        let t = ((pos - pa).dot(ab) / len2).clamp(0.0, 1.0);
        let d = pos.distance(pa + ab * t);
        if best.is_none_or(|(_, _, bd)| d < bd) {
            best = Some((i, t, d));
        }
    }
    let (entry_pos, s_raw, _) = best?;
    let a = mesh.vertices[face.verts[entry_pos] as usize];
    let b = mesh.vertices[face.verts[(entry_pos + 1) % 4] as usize];
    let len = dist3(a, b).round();
    if len < 2.0 {
        return None; // a 1-unit edge has no interior cut position
    }
    let t_units = (s_raw * len).round().clamp(1.0, len - 1.0);
    Some((entry_pos, t_units / len))
}

/// The unique endpoints of the selected edges, sorted.
fn edge_selection_verts(sel_edges: &[(u32, u32)]) -> Vec<u32> {
    let mut set = std::collections::HashSet::new();
    for &(a, b) in sel_edges {
        set.insert(a);
        set.insert(b);
    }
    let mut verts: Vec<u32> = set.into_iter().collect();
    verts.sort_unstable();
    verts
}

/// Faces that contain any of the given edges (as consecutive vertices).
fn faces_with_edges(mesh: &Mesh, edges: &[(u32, u32)]) -> Vec<u32> {
    let set: std::collections::HashSet<(u32, u32)> = edges.iter().copied().collect();
    mesh.faces
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let k = f.verts.len();
            (0..k).any(|i| {
                let a = f.verts[i];
                let b = f.verts[(i + 1) % k];
                set.contains(&(a.min(b), a.max(b)))
            })
        })
        .map(|(i, _)| i as u32)
        .collect()
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Nearest visible edge within the hit distance, as a sorted index pair.
/// Overlapping edges resolve by depth — nearest wins, or furthest when
/// `prefer_far` is set (Alt-click).
pub fn edge_under(
    mesh: &Mesh,
    scene: &render::Scene,
    cam: &super::camera::Camera3D,
    rect: Rect,
    pos: Pos2,
    prefer_far: bool,
) -> Option<(u32, u32)> {
    const EDGE_HIT_DIST: f32 = 6.0;
    let mut seen = std::collections::HashSet::new();
    let mut best: Option<((u32, u32), f32, f32)> = None; // (edge, dist, depth)
    // Normally only edges of front-facing faces are pickable; reaching
    // behind (Alt) must also consider edges hidden by the model.
    let candidates: Vec<u32> = if prefer_far {
        (0..mesh.faces.len() as u32).collect()
    } else {
        scene.visible_faces.clone()
    };
    for &fi in &candidates {
        let face = &mesh.faces[fi as usize];
        let k = face.verts.len();
        for i in 0..k {
            let a = face.verts[i];
            let b = face.verts[(i + 1) % k];
            let key = (a.min(b), a.max(b));
            if !seen.insert(key) {
                continue;
            }
            let (pa, da) = cam.project(mesh.vertices[a as usize], rect);
            let (pb, db) = cam.project(mesh.vertices[b as usize], rect);
            let d = dist_to_segment(pos, pa, pb);
            if d > EDGE_HIT_DIST {
                continue;
            }
            // Depth at the point on the edge nearest the cursor.
            let ab = pb - pa;
            let t = if ab.length_sq() < 1e-6 {
                0.0
            } else {
                ((pos - pa).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
            };
            let depth = da + (db - da) * t;
            best = Some(match best {
                None => (key, d, depth),
                Some((bkey, bd, bdepth)) => {
                    let clearly_closer_on_screen = d < bd - COINCIDENT_PX;
                    let same_spot = (d - bd).abs() <= COINCIDENT_PX;
                    let wins_depth = if prefer_far { depth < bdepth } else { depth > bdepth };
                    if clearly_closer_on_screen || (same_spot && wins_depth) {
                        (key, d, depth)
                    } else {
                        (bkey, bd, bdepth)
                    }
                }
            });
        }
    }
    best.map(|(key, _, _)| key)
}

/// The unique vertices of the selected faces, sorted.
fn face_selection_verts(sel_faces: &[u32], mesh: &Mesh) -> Vec<u32> {
    let mut set = std::collections::HashSet::new();
    for &fi in sel_faces {
        if let Some(face) = mesh.faces.get(fi as usize) {
            for &vi in &face.verts {
                set.insert(vi);
            }
        }
    }
    let mut verts: Vec<u32> = set.into_iter().collect();
    verts.sort_unstable();
    verts
}

/// Screen distances this close count as "the same spot": vertices stacked
/// along the view axis (common in snapped orthographic views) then resolve
/// by depth instead of by index order.
const COINCIDENT_PX: f32 = 1.5;

/// Nearest projected vertex within the hit radius. Among candidates that
/// land on effectively the same screen position, the one closest to the
/// camera wins — or the one furthest away when `prefer_far` is set
/// (Alt-click, to reach geometry behind).
pub fn vertex_under(
    mesh: &Mesh,
    cam: &super::camera::Camera3D,
    rect: Rect,
    pos: Pos2,
    prefer_far: bool,
) -> Option<u32> {
    let mut best: Option<(u32, f32, f32)> = None; // (index, screen dist, depth)
    for (i, &v) in mesh.vertices.iter().enumerate() {
        let (p, depth) = cam.project(v, rect);
        let d = p.distance(pos);
        if d > VERTEX_HIT_RADIUS {
            continue;
        }
        best = Some(match best {
            None => (i as u32, d, depth),
            Some((bi, bd, bdepth)) => {
                let clearly_closer_on_screen = d < bd - COINCIDENT_PX;
                let same_spot = (d - bd).abs() <= COINCIDENT_PX;
                let wins_depth = if prefer_far { depth < bdepth } else { depth > bdepth };
                if clearly_closer_on_screen || (same_spot && wins_depth) {
                    (i as u32, d, depth)
                } else {
                    (bi, bd, bdepth)
                }
            }
        });
    }
    best.map(|(i, _, _)| i)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &mut ThreeDState,
    project: &mut Project,
    undo: &mut UndoStack,
    color_state: &mut ColorState,
    active_tool: &ActiveTool,
    pending_add: Option<edit::AddPrimitive>,
    canvas: &CanvasState,
    theme: &Theme,
    ui: &mut egui::Ui,
    canvas_rect: Rect,
) -> Output {
    let mut output = Output::default();
    let painter = ui.painter_at(canvas_rect);

    // ── Navigation ──────────────────────────────────────────────────────────
    let response = ui.allocate_rect(canvas_rect, Sense::click_and_drag());
    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let pointer_over = pointer_pos.is_some_and(|p| canvas_rect.contains(p));
    let now = ui.input(|i| i.time);

    if response.dragged_by(PointerButton::Secondary) {
        state.camera.orbit(response.drag_delta());
    }
    if response.dragged_by(PointerButton::Middle) {
        state.camera.offset += response.drag_delta();
    }

    let zoom_delta = ui.input(|i| i.zoom_delta());
    if zoom_delta != 1.0 && pointer_over {
        if let Some(pos) = pointer_pos {
            state.camera.zoom_at(zoom_delta, pos, canvas_rect);
        }
    }

    let mut wheel_lines = 0.0f32;
    let mut trackpad = Vec2::ZERO;
    if pointer_over {
        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = event {
                    if *unit == egui::MouseWheelUnit::Line {
                        wheel_lines += delta.y;
                    } else {
                        trackpad += *delta;
                    }
                }
            }
        });
    }
    if wheel_lines != 0.0 {
        state.last_mouse_wheel_time = now;
        if let Some(pos) = pointer_pos {
            let factor = (wheel_lines * 0.25).exp();
            state.camera.zoom_at(factor, pos, canvas_rect);
        }
    } else if trackpad != Vec2::ZERO && now - state.last_mouse_wheel_time > 0.3 {
        state.camera.offset += trackpad;
    }

    let keys_free = !ui.ctx().wants_keyboard_input();
    if keys_free {
        let snaps = [
            (egui::Key::Num1, SnapView::Front),
            (egui::Key::Num2, SnapView::Back),
            (egui::Key::Num3, SnapView::Right),
            (egui::Key::Num4, SnapView::Left),
            (egui::Key::Num5, SnapView::Top),
            (egui::Key::Num6, SnapView::Bottom),
        ];
        for (key, view) in snaps {
            if ui.input(|i| i.key_pressed(key)) {
                state.camera.snap_to(view);
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Num0)) {
            state.camera.reset_home();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.sel_verts.clear();
            state.sel_edges.clear();
            state.sel_faces.clear();
        }
    }

    // ── Render base ─────────────────────────────────────────────────────────
    render::paint_grid(&painter, &state.camera, canvas_rect, theme);

    let atlas = (project.canvas_width, project.canvas_height);
    let cam_copy = state.camera;
    let li = project.active_layer;

    let scene = project
        .mesh3d
        .as_ref()
        .map(|mesh| render::build_scene_styled(mesh, &cam_copy, canvas_rect, atlas, state.shading));

    if let (Some(mesh), Some(scene)) = (project.mesh3d.as_ref(), scene.as_ref()) {
        render::paint_contact_shadow(&painter, mesh, &cam_copy, canvas_rect);
        if let Some(texture) = canvas.texture.as_ref() {
            if state.shading == super::Shading::Dither {
                let patterns = render::dither_patterns(ui.ctx());
                // Pattern cells scale with zoom: half a texel per cell, so the
                // dither reads the same at any magnification.
                let cell = (cam_copy.zoom * 0.5).clamp(2.0, 8.0);
                render::paint_scene_dithered(&painter, scene, texture.id(), &patterns, cell);
            } else {
                render::paint_scene(&painter, scene, texture.id());
            }
        }
        // Off is a clean preview: raw texel colors with no edge seams.
        // Selection/hover overlays still draw, so the tools stay usable.
        if state.shading != super::Shading::Off {
            render::paint_wireframe(&painter, mesh, scene, &cam_copy, canvas_rect, theme);
        }
    }

    // ── Button strip ────────────────────────────────────────────────────────
    let is_select_tool = matches!(active_tool, ActiveTool::Select3D);
    let is_modify_tool = matches!(active_tool, ActiveTool::Extrude | ActiveTool::Inset);
    let is_loop_tool = matches!(active_tool, ActiveTool::LoopCut);
    let is_move_object = matches!(active_tool, ActiveTool::MoveObject);
    let is_scale_object = matches!(active_tool, ActiveTool::ScaleObject);
    let is_object_tool = is_move_object || is_scale_object;
    let mut action: Option<Action> = None;
    if let Some(req) = pending_add {
        if let Some(before) = project.mesh3d.clone() {
            let obj = req.build();
            if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                edit::add_object(mesh, layer, &obj, atlas)
            }) {
                commit_edit(state, project, undo, li, before, outcome, &mut output);
            }
        }
    }
    let mut over_buttons = false;
    {
        let mut x = canvas_rect.min.x + 8.0;
        // Contextual text buttons.
        let mut defs: Vec<(&str, Action)> = Vec::new();
        if is_select_tool && !state.sel_faces.is_empty() {
            defs.push(("Extrude (E)", Action::Extrude));
        }
        if is_select_tool && (3..=4).contains(&state.sel_verts.len()) {
            defs.push(("Create Face (F)", Action::CreateFace));
        }
        if is_select_tool
            && (!state.sel_verts.is_empty()
                || !state.sel_edges.is_empty()
                || !state.sel_faces.is_empty())
        {
            defs.push(("Delete (⌫)", Action::Delete));
        }
        for (label, act) in defs {
            let w = 14.0 + label.len() as f32 * 6.5;
            let rect = Rect::from_min_size(Pos2::new(x, canvas_rect.min.y + 8.0), Vec2::new(w, 24.0));
            let resp = ui.interact(rect, ui.id().with(("threed_btn", label)), Sense::click());
            let bg = if resp.hovered() { theme.surface } else { theme.panel };
            painter.rect_filled(rect, 3.0, bg);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.0),
                theme.fg,
            );
            if resp.hovered() {
                over_buttons = true;
            }
            if resp.clicked() {
                action = Some(act);
            }
            x += w + 6.0;
        }
    }

    // ── Shading toggle (top-right corner) ───────────────────────────────────
    // Cycles Soft (smooth tint) → Dither (picoCAD-style pattern shadows) →
    // Off (raw texel colors, no wireframe).
    {
        let label = state.shading.label();
        let w = 14.0 + label.len() as f32 * 6.5;
        let rect = Rect::from_min_size(
            Pos2::new(canvas_rect.max.x - w - 8.0, canvas_rect.min.y + 8.0),
            Vec2::new(w, 24.0),
        );
        let resp = ui.interact(rect, ui.id().with("threed_shading_toggle"), Sense::click());
        let bg = if resp.hovered() { theme.surface } else { theme.panel };
        painter.rect_filled(rect, 3.0, bg);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.0),
            if state.shading == super::Shading::Off { theme.fg_desc } else { theme.fg },
        );
        if resp.hovered() {
            over_buttons = true;
        }
        if resp.clicked() {
            state.shading = state.shading.next();
        }
    }

    // ── Navigation gizmo (top-right corner) ─────────────────────────────────
    let over_gizmo = gizmo::ui(ui, &painter, &mut state.camera, canvas_rect, theme);
    let over_ui = over_buttons || over_gizmo;

    // ── Painting ────────────────────────────────────────────────────────────
    if let Some(scene) = scene.as_ref() {
        if !over_ui {
            let paint_result =
                paint::handle(state, project, undo, color_state, active_tool, scene, &response, ui);
            output.canvas_dirty |= paint_result.canvas_dirty;
            output.modified |= paint_result.modified;
        }

        if paint::is_paint_tool(active_tool) {
            if over_ui {
                // Cursor left the model area — drop stale hover feedback.
                state.hover_face = None;
                state.hover_texel = None;
            }
            if let (Some(fi), Some(mesh)) = (state.hover_face, project.mesh3d.as_ref()) {
                if let Some(face) = mesh.faces.get(fi as usize) {
                    // Fill paints the whole face: outline the face itself.
                    // The other tools act on one texel: preview that texel.
                    let face_outline = matches!(active_tool, ActiveTool::Fill);
                    if face_outline {
                        let stroke = Stroke::new(1.5, Color32::from_white_alpha(140));
                        let k = face.verts.len();
                        for i in 0..k {
                            let (a, _) =
                                cam_copy.project(mesh.vertices[face.verts[i] as usize], canvas_rect);
                            let (b, _) = cam_copy
                                .project(mesh.vertices[face.verts[(i + 1) % k] as usize], canvas_rect);
                            painter.line_segment([a, b], stroke);
                        }
                    } else if let Some((tx, ty)) = state.hover_texel {
                        if let Some(quad) = mesh.texel_quad_world(face, tx, ty) {
                            let pts: Vec<Pos2> = quad
                                .iter()
                                .map(|&w| cam_copy.project(w, canvas_rect).0)
                                .collect();
                            let fg = color_state.foreground;
                            let (fill, outline) = match active_tool {
                                // Pencil: show the exact color that lands.
                                ActiveTool::Pencil => (
                                    Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], 220),
                                    Color32::WHITE,
                                ),
                                // Eraser: hollow cursor, nothing added.
                                ActiveTool::Eraser => (Color32::from_black_alpha(70), Color32::WHITE),
                                // Eyedropper: pure outline.
                                _ => (Color32::TRANSPARENT, Color32::from_white_alpha(200)),
                            };
                            painter.add(egui::Shape::convex_polygon(
                                pts.clone(),
                                fill,
                                Stroke::NONE,
                            ));
                            for i in 0..pts.len() {
                                painter.line_segment(
                                    [pts[i], pts[(i + 1) % pts.len()]],
                                    Stroke::new(1.0, outline),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Finalize gestures ───────────────────────────────────────────────────
    // Runs every frame, independent of the active tool: a gesture ends when
    // the button is released OR is simply no longer held (tool switch, tab
    // switch, focus loss). Stranding a gesture here used to leave a stale
    // mesh+layer snapshot that a later release would slam back over the
    // document, silently reverting committed work.
    {
        let released = ui.input(|i| i.pointer.primary_released());
        let held = ui.input(|i| i.pointer.primary_down());
        if released || !held {
            if let Some(od) = state.op_drag.take() {
                // Restore pristine state, then commit the final amount
                // through the normal atlas-growth + undo path.
                project.mesh3d = Some(od.start_mesh.clone());
                {
                    let frame = &mut project.animations[0].frames[0];
                    if let Some(layer) = frame.layers.get_mut(li) {
                        *layer = od.start_layer.clone();
                    }
                    frame.dirty = true;
                }
                output.canvas_dirty = true;
                if od.applied != 0 {
                    let (kind, face, n) = (od.kind, od.face, od.applied);
                    let verts = od.verts.clone();
                    let before = od.start_mesh.clone();
                    let kept_faces = state.sel_faces.clone();
                    if let Some(outcome) = with_atlas_growth(project, li, |m, layer, atlas| match kind {
                        OpKind::Extrude => edit::extrude_faces_n(m, layer, &[face], n, atlas),
                        OpKind::Inset => edit::inset_faces(m, layer, &[face], n as u32, atlas),
                        OpKind::Scale => edit::scale_verts(m, layer, &verts, n, atlas),
                    }) {
                        commit_edit(state, project, undo, li, before, outcome, &mut output);
                        if kind == OpKind::Scale {
                            // Face indices survive a scale — keep the object highlighted.
                            state.sel_faces = kept_faces;
                            state.sel_verts.clear();
                        }
                    }
                }
            }
            if let Some(drag) = state.drag.take() {
                project.mesh3d = Some(drag.start_mesh.clone());
                {
                    let frame = &mut project.animations[0].frames[0];
                    if let Some(layer) = frame.layers.get_mut(li) {
                        *layer = drag.start_layer.clone();
                    }
                    frame.dirty = true;
                }
                output.canvas_dirty = true;
                if drag.applied != [0, 0, 0] {
                    let moved = drag.verts.clone();
                    let applied = drag.applied;
                    let before = drag.start_mesh.clone();
                    let kept_verts = state.sel_verts.clone();
                    let kept_edges = state.sel_edges.clone();
                    let kept_faces = state.sel_faces.clone();
                    if let Some(outcome) = with_atlas_growth(project, li, |_, layer, atlas| {
                        edit::move_vertices(&before, layer, &moved, applied, atlas)
                    }) {
                        commit_edit(state, project, undo, li, drag.start_mesh, outcome, &mut output);
                        // Vertex/edge/face identities all survive a move —
                        // restore the selection exactly as it was.
                        state.sel_verts = kept_verts;
                        state.sel_edges = kept_edges;
                        state.sel_faces = kept_faces;
                    }
                }
            }
        }
    }

    // ── Extrude / Inset / Scale drag ────────────────────────────────────────
    if is_modify_tool || is_scale_object {
        state.hover_face = None;
        let kind = match active_tool {
            ActiveTool::Inset => OpKind::Inset,
            ActiveTool::ScaleObject => OpKind::Scale,
            _ => OpKind::Extrude,
        };

        // Hover affordance while not dragging.
        if state.op_drag.is_none() && !over_ui {
            if let (Some(pos), Some(scene), Some(mesh)) =
                (pointer_pos, scene.as_ref(), project.mesh3d.as_ref())
            {
                if let Some(hit) = paint::pick(scene, pos, mesh, atlas) {
                    if let Some(face) = mesh.faces.get(hit.face as usize) {
                        let stroke = Stroke::new(1.5, Color32::from_white_alpha(140));
                        let k = face.verts.len();
                        for i in 0..k {
                            let (a, _) = cam_copy.project(mesh.vertices[face.verts[i] as usize], canvas_rect);
                            let (b, _) = cam_copy.project(mesh.vertices[face.verts[(i + 1) % k] as usize], canvas_rect);
                            painter.line_segment([a, b], stroke);
                        }
                    }
                }
            }
        }

        let pressed = ui.input(|i| i.pointer.primary_pressed()) && pointer_over && !over_ui;
        if pressed && state.op_drag.is_none() {
            if let (Some(pos), Some(scene), Some(mesh)) =
                (pointer_pos, scene.as_ref(), project.mesh3d.as_ref())
            {
                if let Some(hit) = paint::pick(scene, pos, mesh, atlas) {
                    if let Some(start_layer) =
                        project.animations[0].frames[0].layers.get(li).cloned()
                    {
                        let (verts, component) = if kind == OpKind::Scale {
                            let component = edit::connected_faces(mesh, hit.face);
                            (face_selection_verts(&component, mesh), component)
                        } else {
                            (Vec::new(), Vec::new())
                        };
                        state.op_drag = Some(OpDrag {
                            kind,
                            face: hit.face,
                            verts,
                            start_mesh: mesh.clone(),
                            start_layer,
                            raw: 0.0,
                            applied: 0,
                        });
                        if kind == OpKind::Scale {
                            state.sel_faces = component;
                            state.sel_verts.clear();
                            state.sel_edges.clear();
                        }
                    }
                }
            }
        }

        // Live preview: recompute the whole op from the pristine snapshots
        // whenever the snapped amount changes.
        if response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if delta != Vec2::ZERO && state.op_drag.is_some() {
                let zoom = cam_copy.zoom.max(0.001);
                let Some(od) = state.op_drag.as_mut() else { unreachable!() };
                od.raw += match od.kind {
                    OpKind::Extrude => {
                        let face = &od.start_mesh.faces[od.face as usize];
                        let axis = edit::extrude_dir(&od.start_mesh, face);
                        let w = cam_copy.unview([delta.x / zoom, -delta.y / zoom, 0.0]);
                        w[0] * axis[0] + w[1] * axis[1] + w[2] * axis[2]
                    }
                    // Drag right/down to grow the inset border.
                    OpKind::Inset => (delta.x + delta.y) / (2.0 * zoom),
                    // Drag right/up to grow the object.
                    OpKind::Scale => (delta.x - delta.y) / (2.0 * zoom),
                };
                let n: i32 = match od.kind {
                    // Extrude and Scale go both ways; Inset only grows.
                    OpKind::Extrude | OpKind::Scale => od.raw.round() as i32,
                    OpKind::Inset => od.raw.round().max(0.0) as i32,
                };
                if n != od.applied {
                    let outcome = match od.kind {
                        OpKind::Extrude => {
                            edit::extrude_faces_n(&od.start_mesh, &od.start_layer, &[od.face], n, atlas)
                        }
                        OpKind::Inset => {
                            edit::inset_faces(&od.start_mesh, &od.start_layer, &[od.face], n as u32, atlas)
                        }
                        OpKind::Scale => {
                            edit::scale_verts(&od.start_mesh, &od.start_layer, &od.verts, n, atlas)
                        }
                    };
                    if let Ok(outcome) = outcome {
                        let start_layer = od.start_layer.clone();
                        od.applied = n;
                        project.mesh3d = Some(outcome.mesh);
                        let frame = &mut project.animations[0].frames[0];
                        if let Some(layer) = frame.layers.get_mut(li) {
                            *layer = start_layer;
                            for &(x, y, _, new) in &outcome.pixel_edits {
                                layer.set_pixel(x, y, new);
                            }
                        }
                        frame.dirty = true;
                        output.canvas_dirty = true;
                    }
                    // Err(AtlasFull): hold the previous preview; growth
                    // happens on commit.
                }
            }
        }

    }

    // ── Loop cut ────────────────────────────────────────────────────────────
    if is_loop_tool {
        state.hover_face = None;
        if !over_ui {
            let mut pending: Option<(Mesh, edit::LoopPlan)> = None;
            if let (Some(pos), Some(scene), Some(mesh)) =
                (pointer_pos, scene.as_ref(), project.mesh3d.as_ref())
            {
                if let Some(hit) = paint::pick(scene, pos, mesh, atlas) {
                    if let Some((entry_pos, s)) = face_edge_param(mesh, hit.face, &cam_copy, canvas_rect, pos) {
                        if let Some(plan) = edit::plan_loop(mesh, hit.face, entry_pos, s) {
                            for seg in &plan.segments {
                                let (a, _) = cam_copy.project(seg.0, canvas_rect);
                                let (b, _) = cam_copy.project(seg.1, canvas_rect);
                                painter.line_segment([a, b], Stroke::new(2.0, Color32::WHITE));
                            }
                            if ui.input(|i| i.pointer.primary_pressed()) {
                                pending = Some((mesh.clone(), plan));
                            }
                        }
                    }
                }
            }
            if let Some((before, plan)) = pending {
                if let Some(outcome) = with_atlas_growth(project, li, |m, layer, atlas| {
                    edit::loop_cut(m, layer, &plan, atlas)
                }) {
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                }
            }
        }
    }

    // ── Modeling input (smart select + object move) ─────────────────────────
    if is_select_tool || is_move_object {
        state.hover_face = None;

        let pressed = ui.input(|i| i.pointer.primary_pressed()) && pointer_over && !over_ui;
        let shift = ui.input(|i| i.modifiers.shift);
        // Alt reaches past the nearest element to the one stacked behind it.
        let prefer_far = ui.input(|i| i.modifiers.alt);

        if pressed {
            if let (Some(pos), Some(mesh)) = (pointer_pos, project.mesh3d.as_ref()) {
                if is_move_object {
                    // Click an object: select its whole connected component
                    // and start moving it. Shift toggles models in and out of
                    // the selection instead, so several can move as one.
                    let hit = scene.as_ref().and_then(|sc| paint::pick(sc, pos, mesh, atlas));
                    match hit {
                        Some(hit) => {
                            let component = edit::connected_faces(mesh, hit.face);
                            state.sel_verts.clear();
                            state.sel_edges.clear();
                            let already_selected =
                                component.iter().all(|f| state.sel_faces.contains(f));
                            if shift {
                                // Toggle the whole model; no drag on a
                                // shift-click — it is a selection edit.
                                if already_selected {
                                    state.sel_faces.retain(|f| !component.contains(f));
                                } else {
                                    for f in component {
                                        if !state.sel_faces.contains(&f) {
                                            state.sel_faces.push(f);
                                        }
                                    }
                                }
                            } else {
                                // Plain click: pressing on any model of the
                                // current selection drags them all together;
                                // pressing elsewhere selects just that model.
                                if !already_selected {
                                    state.sel_faces = component;
                                }
                                if let Some(start_layer) =
                                    project.animations[0].frames[0].layers.get(li).cloned()
                                {
                                    state.drag = Some(VertexDrag {
                                        start_mesh: mesh.clone(),
                                        start_layer,
                                        verts: face_selection_verts(&state.sel_faces, mesh),
                                        raw: [0.0; 3],
                                        applied: [0; 3],
                                    });
                                }
                            }
                        }
                        // Empty space: plain click deselects; a missed
                        // shift-click keeps the set being built.
                        None if !shift => state.sel_faces.clear(),
                        None => {}
                    }
                } else if let Some(vi) = vertex_under(mesh, &cam_copy, canvas_rect, pos, prefer_far) {
                    // Smart select, priority 1: vertex.
                    state.sel_edges.clear();
                    state.sel_faces.clear();
                    {
                            if shift {
                                if let Some(idx) = state.sel_verts.iter().position(|&v| v == vi) {
                                    state.sel_verts.remove(idx);
                                } else {
                                    state.sel_verts.push(vi);
                                }
                            } else if !state.sel_verts.contains(&vi) {
                                state.sel_verts = vec![vi];
                            }
                            if state.sel_verts.contains(&vi) {
                                if let Some(start_layer) =
                                    project.animations[0].frames[0].layers.get(li).cloned()
                                {
                                    state.drag = Some(VertexDrag {
                                        start_mesh: mesh.clone(),
                                        start_layer,
                                        verts: state.sel_verts.clone(),
                                        raw: [0.0; 3],
                                        applied: [0; 3],
                                    });
                                }
                            }
                    }
                } else if let Some(edge) =
                    scene.as_ref().and_then(|sc| edge_under(mesh, sc, &cam_copy, canvas_rect, pos, prefer_far))
                {
                    // Smart select, priority 2: edge.
                    state.sel_verts.clear();
                    state.sel_faces.clear();
                    {
                        {
                            {
                                if shift {
                                    if let Some(idx) =
                                        state.sel_edges.iter().position(|&e| e == edge)
                                    {
                                        state.sel_edges.remove(idx);
                                    } else {
                                        state.sel_edges.push(edge);
                                    }
                                } else if !state.sel_edges.contains(&edge) {
                                    state.sel_edges = vec![edge];
                                }
                                // Pressing on a selected edge starts an edge move.
                                if state.sel_edges.contains(&edge) {
                                    if let Some(start_layer) =
                                        project.animations[0].frames[0].layers.get(li).cloned()
                                    {
                                        state.drag = Some(VertexDrag {
                                            start_mesh: mesh.clone(),
                                            start_layer,
                                            verts: edge_selection_verts(&state.sel_edges),
                                            raw: [0.0; 3],
                                            applied: [0; 3],
                                        });
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(hit) =
                    scene.as_ref().and_then(|sc| paint::pick(sc, pos, mesh, atlas))
                {
                    // Smart select, priority 3: face.
                    state.sel_verts.clear();
                    state.sel_edges.clear();
                    {
                        {
                            if shift {
                                if let Some(idx) =
                                    state.sel_faces.iter().position(|&f| f == hit.face)
                                {
                                    state.sel_faces.remove(idx);
                                } else {
                                    state.sel_faces.push(hit.face);
                                }
                            } else if !state.sel_faces.contains(&hit.face) {
                                state.sel_faces = vec![hit.face];
                            }
                            // Pressing on a selected face starts a face move.
                            if state.sel_faces.contains(&hit.face) {
                                if let Some(start_layer) =
                                    project.animations[0].frames[0].layers.get(li).cloned()
                                {
                                    state.drag = Some(VertexDrag {
                                        start_mesh: mesh.clone(),
                                        start_layer,
                                        verts: face_selection_verts(&state.sel_faces, mesh),
                                        raw: [0.0; 3],
                                        applied: [0; 3],
                                    });
                                }
                            }
                        }
                    }
                } else if !shift {
                    // Clicked empty space: clear every selection kind.
                    state.sel_verts.clear();
                    state.sel_edges.clear();
                    state.sel_faces.clear();
                }
            }
        }

        // Live move drag (vertex/edge/face): replay the full move — islands
        // and 1:1 texture copies included — from the pristine snapshots on
        // every grid step, so the texture never skews mid-drag.
        if response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if delta != Vec2::ZERO && state.drag.is_some() {
                let zoom = cam_copy.zoom.max(0.001);
                let world = cam_copy.unview([delta.x / zoom, -delta.y / zoom, 0.0]);
                let Some(drag) = state.drag.as_mut() else { unreachable!() };
                drag.raw[0] += world[0];
                drag.raw[1] += world[1];
                drag.raw[2] += world[2];
                let snapped = [
                    drag.raw[0].round() as i32,
                    drag.raw[1].round() as i32,
                    drag.raw[2].round() as i32,
                ];
                if snapped != drag.applied {
                    let outcome = edit::move_vertices(
                        &drag.start_mesh,
                        &drag.start_layer,
                        &drag.verts,
                        snapped,
                        atlas,
                    );
                    if let Ok(outcome) = outcome {
                        drag.applied = snapped;
                        let start_layer = drag.start_layer.clone();
                        project.mesh3d = Some(outcome.mesh);
                        let frame = &mut project.animations[0].frames[0];
                        if let Some(layer) = frame.layers.get_mut(li) {
                            *layer = start_layer;
                            for &(x, y, _, new) in &outcome.pixel_edits {
                                layer.set_pixel(x, y, new);
                            }
                        }
                        frame.dirty = true;
                        output.canvas_dirty = true;
                    }
                    // Err(AtlasFull): hold the previous preview; growth
                    // happens on commit.
                }
            }
        }

        // Keyboard actions
        if keys_free {
            if is_select_tool
                && !state.sel_faces.is_empty()
                && ui.input(|i| i.key_pressed(egui::Key::E))
            {
                action = Some(Action::Extrude);
            }
            if is_select_tool
                && (3..=4).contains(&state.sel_verts.len())
                && ui.input(|i| i.key_pressed(egui::Key::F))
            {
                action = Some(Action::CreateFace);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            {
                action = Some(Action::Delete);
            }
        }
    }

    // ── Apply pending actions ───────────────────────────────────────────────
    if let Some(act) = action {
        let before = project.mesh3d.clone();
        match (act, before) {
            (Action::Extrude, Some(before)) if !state.sel_faces.is_empty() => {
                let sel = state.sel_faces.clone();
                if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                    edit::extrude_faces(mesh, layer, &sel, atlas)
                }) {
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                }
            }
            (Action::CreateFace, Some(before)) if (3..=4).contains(&state.sel_verts.len()) => {
                let verts = state.sel_verts.clone();
                if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                    edit::create_face(mesh, layer, &verts, atlas)
                }) {
                    if outcome.mesh.faces.len() != before.faces.len() {
                        commit_edit(state, project, undo, li, before, outcome, &mut output);
                    }
                }
            }
            // Deleting shrinks the blocks the removed faces belonged to, so
            // these relayout the survivors and can need a bigger atlas — the
            // reason they go through with_atlas_growth like every other edit.
            (Action::Delete, Some(before)) => {
                if !state.sel_faces.is_empty() {
                    let doomed = state.sel_faces.clone();
                    if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                        edit::delete_faces(mesh, layer, &doomed, atlas)
                    }) {
                        commit_edit(state, project, undo, li, before, outcome, &mut output);
                    }
                } else if !state.sel_edges.is_empty() {
                    let doomed = faces_with_edges(&before, &state.sel_edges);
                    if !doomed.is_empty() {
                        if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                            edit::delete_faces(mesh, layer, &doomed, atlas)
                        }) {
                            commit_edit(state, project, undo, li, before, outcome, &mut output);
                        }
                    }
                    state.sel_edges.clear();
                } else if !state.sel_verts.is_empty() {
                    let doomed = state.sel_verts.clone();
                    if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                        edit::delete_vertices(mesh, layer, &doomed, atlas)
                    }) {
                        commit_edit(state, project, undo, li, before, outcome, &mut output);
                    }
                }
            }
            _ => {}
        }
    }

    // ── Selection overlays ──────────────────────────────────────────────────
    if let Some(mesh) = project.mesh3d.as_ref() {
        if is_object_tool && !state.sel_faces.is_empty() {
            // Selected object: white outline only, no fill. Strictly the outer
            // boundary — interior contours like the rim of a recess are not
            // part of the object's outline (see render::silhouette_edges).
            if let Some(scene) = scene.as_ref() {
                let outline = Stroke::new(2.0, Color32::WHITE);
                for (_, pa, pb) in
                    render::silhouette_edges(mesh, scene, &cam_copy, canvas_rect, &state.sel_faces)
                {
                    painter.line_segment([pa, pb], outline);
                }
            }
        } else if is_select_tool && !state.sel_faces.is_empty() {
            // Lighten the selected face rather than darkening it.
            let tint = Color32::from_white_alpha(60);
            let outline = Stroke::new(2.0, Color32::WHITE);
            for &fi in &state.sel_faces {
                if let Some(face) = mesh.faces.get(fi as usize) {
                    let pts: Vec<Pos2> = face
                        .verts
                        .iter()
                        .map(|&vi| cam_copy.project(mesh.vertices[vi as usize], canvas_rect).0)
                        .collect();
                    painter.add(egui::Shape::convex_polygon(pts, tint, outline));
                }
            }
        }
        if is_select_tool {
            // Hovered edge (lighter) under the selected edges (full accent).
            if !over_ui {
                if let (Some(pos), Some(scene)) = (pointer_pos, scene.as_ref()) {
                    if let Some((a, b)) = edge_under(mesh, scene, &cam_copy, canvas_rect, pos, false) {
                        if !state.sel_edges.contains(&(a, b)) {
                            let (pa, _) = cam_copy.project(mesh.vertices[a as usize], canvas_rect);
                            let (pb, _) = cam_copy.project(mesh.vertices[b as usize], canvas_rect);
                            painter.line_segment([pa, pb], Stroke::new(2.0, Color32::from_white_alpha(140)));
                        }
                    }
                }
            }
            for &(a, b) in &state.sel_edges {
                if (a as usize) < mesh.vertices.len() && (b as usize) < mesh.vertices.len() {
                    let (pa, _) = cam_copy.project(mesh.vertices[a as usize], canvas_rect);
                    let (pb, _) = cam_copy.project(mesh.vertices[b as usize], canvas_rect);
                    painter.line_segment([pa, pb], Stroke::new(3.0, Color32::WHITE));
                }
            }
        }
        if is_select_tool {
            for (i, &v) in mesh.vertices.iter().enumerate() {
                let (p, _) = cam_copy.project(v, canvas_rect);
                if !canvas_rect.contains(p) {
                    continue;
                }
                let selected = state.sel_verts.contains(&(i as u32));
                let half = if selected { 3.5 } else { 2.5 };
                let r = Rect::from_center_size(p, Vec2::splat(half * 2.0));
                if selected {
                    painter.rect_filled(r, 0.0, Color32::WHITE);
                    painter.rect_stroke(r.expand(1.0), 0.0, Stroke::new(1.0, Color32::from_black_alpha(180)), egui::StrokeKind::Outside);
                } else {
                    painter.rect_filled(r, 0.0, theme.fg_muted);
                }
            }
        }
    }

    // View label + tool hints (bottom-left corner of the workspace)
    let cam_now = state.camera;
    let label = match cam_now.snapped() {
        Some(v) => v.label().to_string(),
        None => "Orbit".to_string(),
    };
    let hint = match active_tool {
        ActiveTool::Select3D => "Select: click vertex/edge/face · alt = the one behind · drag move · shift multi · E extrude · F fill 3-4 verts · Del delete",
        ActiveTool::Extrude => "Extrude: drag a face to pull it out (whole units)",
        ActiveTool::Inset => "Inset: drag a face to grow an inset border",
        ActiveTool::LoopCut => "Loop Cut: hover to preview the ring · click to cut",
        ActiveTool::MoveObject => "Move: click an object · shift-click to add more · drag moves them all",
        ActiveTool::ScaleObject => "Scale: click an object · drag right/up to resize in whole units",
        t if paint::is_paint_tool(t) => "Paint on the model · RMB orbit · MMB pan · 1-6 snap views",
        _ => "RMB orbit · MMB pan · scroll zoom · 1-6 snap views",
    };
    painter.text(
        canvas_rect.left_bottom() + Vec2::new(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{}  ·  {:.0}px/texel   —   {}", label, cam_now.zoom, hint),
        egui::FontId::proportional(11.0),
        theme.fg_muted,
    );

    output
}
