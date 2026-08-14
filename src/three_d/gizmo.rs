// src/three_d/gizmo.rs
//
// Blender-style navigation gizmo in the corner of the 3D workspace:
// six axis balls (±X ±Y ±Z) projected through the camera rotation.
// Drag anywhere on it to orbit; click a ball to snap to that view;
// click the ball of the current view to flip to the opposite side.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::camera::{Camera3D, SnapView};
use crate::theme::Theme;

const RADIUS: f32 = 42.0;
const BALL_ORBIT: f32 = 30.0;
const BALL_R: f32 = 8.0;

const AXIS_X: Color32 = Color32::from_rgb(226, 84, 84);
const AXIS_Y: Color32 = Color32::from_rgb(108, 194, 92);
const AXIS_Z: Color32 = Color32::from_rgb(94, 136, 226);

struct Ball {
    dir: [f32; 3],
    label: &'static str,
    color: Color32,
    view: SnapView,
    positive: bool,
}

fn balls() -> [Ball; 6] {
    [
        Ball { dir: [1.0, 0.0, 0.0], label: "X", color: AXIS_X, view: SnapView::Right, positive: true },
        Ball { dir: [-1.0, 0.0, 0.0], label: "-X", color: AXIS_X, view: SnapView::Left, positive: false },
        Ball { dir: [0.0, 1.0, 0.0], label: "Y", color: AXIS_Y, view: SnapView::Top, positive: true },
        Ball { dir: [0.0, -1.0, 0.0], label: "-Y", color: AXIS_Y, view: SnapView::Bottom, positive: false },
        Ball { dir: [0.0, 0.0, 1.0], label: "Z", color: AXIS_Z, view: SnapView::Front, positive: true },
        Ball { dir: [0.0, 0.0, -1.0], label: "-Z", color: AXIS_Z, view: SnapView::Back, positive: false },
    ]
}

fn opposite(view: SnapView) -> SnapView {
    match view {
        SnapView::Front => SnapView::Back,
        SnapView::Back => SnapView::Front,
        SnapView::Right => SnapView::Left,
        SnapView::Left => SnapView::Right,
        SnapView::Top => SnapView::Bottom,
        SnapView::Bottom => SnapView::Top,
    }
}

/// Interact with and draw the gizmo. Returns true while the pointer is over
/// it, so the workspace can keep paint/selection clicks away.
pub fn ui(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    cam: &mut Camera3D,
    canvas_rect: Rect,
    theme: &Theme,
) -> bool {
    let center = Pos2::new(canvas_rect.right() - RADIUS - 14.0, canvas_rect.top() + RADIUS + 14.0);
    let rect = Rect::from_center_size(center, Vec2::splat(RADIUS * 2.0));
    let response = ui.interact(rect, ui.id().with("threed_gizmo"), Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let over = pointer.is_some_and(|p| rect.contains(p)) || response.dragged();

    if response.dragged() {
        cam.orbit(response.drag_delta());
    }

    // Background disc while engaged.
    if over {
        painter.circle_filled(center, RADIUS, theme.panel.gamma_multiply(0.85));
    }

    // Project the six axis balls through the camera rotation.
    let mut drawn: Vec<(Pos2, f32, &'static str, Color32, SnapView, bool)> = balls()
        .iter()
        .map(|b| {
            let v = cam.view_dir(b.dir);
            let pos = center + Vec2::new(v[0], -v[1]) * BALL_ORBIT;
            (pos, v[2], b.label, b.color, b.view, b.positive)
        })
        .collect();
    // Far → near so close balls draw on top.
    drawn.sort_by(|a, b| a.1.total_cmp(&b.1));

    // Axis lines from center to the positive balls (under the balls).
    for &(pos, depth, _, color, _, positive) in &drawn {
        if positive {
            let alpha = if depth >= 0.0 { 0.8 } else { 0.35 };
            painter.line_segment([center, pos], Stroke::new(1.5, color.gamma_multiply(alpha)));
        }
    }

    let current = cam.snapped();
    let hovered_ball = pointer.and_then(|p| {
        drawn
            .iter()
            .filter(|(pos, ..)| pos.distance(p) <= BALL_R + 2.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|&(_, _, _, _, view, _)| view)
    });

    for &(pos, depth, label, color, view, positive) in &drawn {
        let front = depth >= 0.0;
        let alpha = if front { 1.0 } else { 0.45 };
        let is_current = current == Some(view);
        let is_hovered = over && hovered_ball == Some(view);
        let r = if is_hovered { BALL_R + 1.5 } else { BALL_R };
        if positive || is_current || is_hovered {
            painter.circle_filled(pos, r, color.gamma_multiply(alpha));
        } else {
            painter.circle_filled(pos, r, theme.bg.gamma_multiply(0.9));
            painter.circle_stroke(pos, r, Stroke::new(1.5, color.gamma_multiply(alpha)));
        }
        if is_current {
            painter.circle_stroke(pos, r + 1.5, Stroke::new(1.5, theme.fg));
        }
        if positive || is_hovered {
            painter.text(
                pos,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.0),
                if positive { Color32::from_black_alpha(210) } else { theme.fg },
            );
        }
    }

    // Click a ball to snap; clicking the current view's ball flips to the
    // opposite side (Blender behavior).
    if response.clicked() {
        if let Some(view) = hovered_ball {
            let target = if current == Some(view) { opposite(view) } else { view };
            cam.snap_to(target);
        }
    }

    over
}
