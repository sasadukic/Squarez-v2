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
use super::{paint, render, ThreeDState, VertexDrag};
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
    AddCube,
    AddPlane,
    Extrude,
    Delete,
}

const VERTEX_HIT_RADIUS: f32 = 8.0;
const MAX_ATLAS_HEIGHT: u32 = 4096;

/// Run a mesh operation, doubling the atlas height (preserving content)
/// as long as the shelf packer reports it full.
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
            Err(AtlasFull) => {
                if project.canvas_height >= MAX_ATLAS_HEIGHT {
                    return None;
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

/// Nearest projected vertex within the hit radius.
fn vertex_under(
    mesh: &Mesh,
    cam: &super::camera::Camera3D,
    rect: Rect,
    pos: Pos2,
) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;
    for (i, &v) in mesh.vertices.iter().enumerate() {
        let (p, _) = cam.project(v, rect);
        let d = p.distance(pos);
        if d <= VERTEX_HIT_RADIUS && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i as u32, d));
        }
    }
    best.map(|(i, _)| i)
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    state: &mut ThreeDState,
    project: &mut Project,
    undo: &mut UndoStack,
    color_state: &mut ColorState,
    active_tool: &ActiveTool,
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
        .map(|mesh| render::build_scene(mesh, &cam_copy, canvas_rect, atlas));

    if let (Some(mesh), Some(scene)) = (project.mesh3d.as_ref(), scene.as_ref()) {
        if let Some(texture) = canvas.texture.as_ref() {
            render::paint_scene(&painter, scene, texture.id());
        }
        render::paint_wireframe(&painter, mesh, scene, &cam_copy, canvas_rect, theme);
    }

    // ── Button strip ────────────────────────────────────────────────────────
    let is_face_tool = matches!(active_tool, ActiveTool::FaceSelect);
    let is_vertex_tool = matches!(active_tool, ActiveTool::VertexSelect);
    let mut action: Option<Action> = None;
    let mut over_buttons = false;
    {
        let mut defs: Vec<(&str, Action)> =
            vec![("+ Cube", Action::AddCube), ("+ Plane", Action::AddPlane)];
        if is_face_tool && !state.sel_faces.is_empty() {
            defs.push(("Extrude (E)", Action::Extrude));
        }
        let mut x = canvas_rect.min.x + 8.0;
        for (label, act) in defs {
            let w = 14.0 + label.len() as f32 * 6.5;
            let rect = Rect::from_min_size(Pos2::new(x, canvas_rect.min.y + 8.0), Vec2::new(w, 22.0));
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

    // ── Painting ────────────────────────────────────────────────────────────
    if let Some(scene) = scene.as_ref() {
        if !over_buttons {
            let paint_result =
                paint::handle(state, project, undo, color_state, active_tool, scene, &response, ui);
            output.canvas_dirty |= paint_result.canvas_dirty;
            output.modified |= paint_result.modified;
        }

        if paint::is_paint_tool(active_tool) && !over_buttons {
            if let Some(fi) = state.hover_face {
                if let Some(mesh) = project.mesh3d.as_ref() {
                    if let Some(face) = mesh.faces.get(fi as usize) {
                        let stroke = Stroke::new(1.5, theme.accent);
                        let k = face.verts.len();
                        for i in 0..k {
                            let (a, _) =
                                cam_copy.project(mesh.vertices[face.verts[i] as usize], canvas_rect);
                            let (b, _) = cam_copy
                                .project(mesh.vertices[face.verts[(i + 1) % k] as usize], canvas_rect);
                            painter.line_segment([a, b], stroke);
                        }
                    }
                }
            }
        }
    }

    // ── Modeling input ──────────────────────────────────────────────────────
    if is_vertex_tool || is_face_tool {
        state.hover_face = None;

        let pressed = ui.input(|i| i.pointer.primary_pressed()) && pointer_over && !over_buttons;
        let shift = ui.input(|i| i.modifiers.shift);

        if pressed {
            if let (Some(pos), Some(mesh)) = (pointer_pos, project.mesh3d.as_ref()) {
                if is_vertex_tool {
                    match vertex_under(mesh, &cam_copy, canvas_rect, pos) {
                        Some(vi) => {
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
                                state.drag = Some(VertexDrag {
                                    start_mesh: mesh.clone(),
                                    verts: state.sel_verts.clone(),
                                    raw: [0.0; 3],
                                    applied: [0; 3],
                                });
                            }
                        }
                        None => {
                            if !shift {
                                state.sel_verts.clear();
                            }
                        }
                    }
                } else if let Some(scene) = scene.as_ref() {
                    match paint::pick(scene, pos, mesh, atlas) {
                        Some(hit) => {
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
                                state.drag = Some(VertexDrag {
                                    start_mesh: mesh.clone(),
                                    verts: face_selection_verts(&state.sel_faces, mesh),
                                    raw: [0.0; 3],
                                    applied: [0; 3],
                                });
                            }
                        }
                        None => {
                            if !shift {
                                state.sel_faces.clear();
                            }
                        }
                    }
                }
            }
        }

        // Live move drag (vertex or face): mutate geometry only; islands
        // settle on release.
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
                let diff = [
                    snapped[0] - drag.applied[0],
                    snapped[1] - drag.applied[1],
                    snapped[2] - drag.applied[2],
                ];
                if diff != [0, 0, 0] {
                    if let Some(mesh) = project.mesh3d.as_mut() {
                        for &vi in &drag.verts {
                            if let Some(v) = mesh.vertices.get_mut(vi as usize) {
                                v[0] += diff[0] as f32;
                                v[1] += diff[1] as f32;
                                v[2] += diff[2] as f32;
                            }
                        }
                    }
                    drag.applied = snapped;
                }
            }
        }

        // Drag release: settle islands + push one MeshEdit.
        if ui.input(|i| i.pointer.primary_released()) {
            if let Some(drag) = state.drag.take() {
                if drag.applied != [0, 0, 0] {
                    let moved = drag.verts.clone();
                    let applied = drag.applied;
                    let before = drag.start_mesh.clone();
                    let kept_faces = state.sel_faces.clone();
                    if let Some(outcome) = with_atlas_growth(project, li, |_, layer, atlas| {
                        edit::move_vertices(&before, layer, &moved, applied, atlas)
                    }) {
                        commit_edit(state, project, undo, li, drag.start_mesh, outcome, &mut output);
                        if is_face_tool {
                            // Face indices are unchanged by a move — keep the
                            // face selection instead of the moved-verts list.
                            state.sel_faces = kept_faces;
                            state.sel_verts.clear();
                        }
                    } else {
                        // Could not settle islands — revert the whole move.
                        project.mesh3d = Some(drag.start_mesh);
                    }
                } else {
                    // No net movement: restore the pristine snapshot.
                    project.mesh3d = Some(drag.start_mesh);
                }
            }
        }

        // Keyboard actions
        if keys_free {
            if is_face_tool
                && !state.sel_faces.is_empty()
                && ui.input(|i| i.key_pressed(egui::Key::E))
            {
                action = Some(Action::Extrude);
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
            (Action::AddCube, Some(before)) => {
                if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                    edit::add_primitive(mesh, layer, edit::Primitive::Cube, atlas)
                }) {
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                }
            }
            (Action::AddPlane, Some(before)) => {
                if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                    edit::add_primitive(mesh, layer, edit::Primitive::Plane, atlas)
                }) {
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                }
            }
            (Action::Extrude, Some(before)) if !state.sel_faces.is_empty() => {
                let sel = state.sel_faces.clone();
                if let Some(outcome) = with_atlas_growth(project, li, |mesh, layer, atlas| {
                    edit::extrude_faces(mesh, layer, &sel, atlas)
                }) {
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                }
            }
            (Action::Delete, Some(before)) => {
                if is_face_tool && !state.sel_faces.is_empty() {
                    let outcome = edit::delete_faces(&before, &state.sel_faces);
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                } else if is_vertex_tool && !state.sel_verts.is_empty() {
                    let outcome = edit::delete_vertices(&before, &state.sel_verts);
                    commit_edit(state, project, undo, li, before, outcome, &mut output);
                }
            }
            _ => {}
        }
    }

    // ── Selection overlays ──────────────────────────────────────────────────
    if let Some(mesh) = project.mesh3d.as_ref() {
        if is_face_tool && !state.sel_faces.is_empty() {
            let tint = theme.accent.gamma_multiply(0.35);
            let outline = Stroke::new(2.0, theme.accent);
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
        if is_vertex_tool {
            for (i, &v) in mesh.vertices.iter().enumerate() {
                let (p, _) = cam_copy.project(v, canvas_rect);
                if !canvas_rect.contains(p) {
                    continue;
                }
                let selected = state.sel_verts.contains(&(i as u32));
                let half = if selected { 3.5 } else { 2.5 };
                let r = Rect::from_center_size(p, Vec2::splat(half * 2.0));
                if selected {
                    painter.rect_filled(r, 0.0, theme.accent);
                    painter.rect_stroke(r.expand(1.0), 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
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
        ActiveTool::VertexSelect => "Vertex: click select · drag move · shift multi · Del delete",
        ActiveTool::FaceSelect => "Face: click select · drag move · E extrude · Del delete",
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
