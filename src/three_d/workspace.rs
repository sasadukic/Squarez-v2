// src/three_d/workspace.rs
//
// The 3D workspace: owns all input inside the canvas rect (orbit, pan,
// zoom, snap views — painting and modeling arrive in later phases) and
// renders the textured model with grid + wireframe overlays.

use egui::{PointerButton, Rect, Stroke, Vec2};

use super::camera::SnapView;
use super::{paint, render};
use super::ThreeDState;
use crate::canvas::CanvasState;
use crate::color::ColorState;
use crate::history::UndoStack;
use crate::project::Project;
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
    let cam = &mut state.camera;

    // ── Navigation ──────────────────────────────────────────────────────────
    let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());
    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    let pointer_over = pointer_pos.is_some_and(|p| canvas_rect.contains(p));
    let now = ui.input(|i| i.time);

    if response.dragged_by(PointerButton::Secondary) {
        cam.orbit(response.drag_delta());
    }
    if response.dragged_by(PointerButton::Middle) {
        cam.offset += response.drag_delta();
    }

    // Pinch zoom
    let zoom_delta = ui.input(|i| i.zoom_delta());
    if zoom_delta != 1.0 && pointer_over {
        if let Some(pos) = pointer_pos {
            cam.zoom_at(zoom_delta, pos, canvas_rect);
        }
    }

    // Wheel zoom vs trackpad pan (same discrimination as the 2D canvas)
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
            cam.zoom_at(factor, pos, canvas_rect);
        }
    } else if trackpad != Vec2::ZERO && now - state.last_mouse_wheel_time > 0.3 {
        cam.offset += trackpad;
    }

    // Snap views + home reset (only while no text field wants the keyboard)
    if !ui.ctx().wants_keyboard_input() {
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
                cam.snap_to(view);
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Num0)) {
            cam.reset_home();
        }
    }

    // ── Render ──────────────────────────────────────────────────────────────
    render::paint_grid(&painter, cam, canvas_rect, theme);

    let atlas = (project.canvas_width, project.canvas_height);
    let cam_copy = *cam;
    if let Some(mesh) = project.mesh3d.as_ref() {
        let scene = render::build_scene(mesh, &cam_copy, canvas_rect, atlas);
        if let Some(texture) = canvas.texture.as_ref() {
            render::paint_scene(&painter, &scene, texture.id());
        }
        render::paint_wireframe(&painter, mesh, &scene, &cam_copy, canvas_rect, theme);

        // ── Painting on the model ───────────────────────────────────────────
        let paint_result = paint::handle(
            state, project, undo, color_state, active_tool, &scene, &response, ui,
        );
        output.canvas_dirty |= paint_result.canvas_dirty;
        output.modified |= paint_result.modified;

        // Hovered-face outline (only for paint tools, and not mid-navigation)
        if paint::is_paint_tool(active_tool) {
            if let Some(fi) = state.hover_face {
                if let Some(mesh) = project.mesh3d.as_ref() {
                    if let Some(face) = mesh.faces.get(fi as usize) {
                        let stroke = Stroke::new(1.5, theme.accent);
                        let k = face.verts.len();
                        for i in 0..k {
                            let (a, _) = cam_copy
                                .project(mesh.vertices[face.verts[i] as usize], canvas_rect);
                            let (b, _) = cam_copy
                                .project(mesh.vertices[face.verts[(i + 1) % k] as usize], canvas_rect);
                            painter.line_segment([a, b], stroke);
                        }
                    }
                }
            }
        }
    }

    // View label (bottom-left corner of the workspace)
    let cam_now = state.camera;
    let label = match cam_now.snapped() {
        Some(v) => v.label().to_string(),
        None => "Orbit".to_string(),
    };
    painter.text(
        canvas_rect.left_bottom() + Vec2::new(8.0, -8.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{}  ·  {:.0}px/texel", label, cam_now.zoom),
        egui::FontId::proportional(11.0),
        theme.fg_muted,
    );

    output
}
