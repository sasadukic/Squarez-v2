use egui::{Pos2, Rect, Vec2};
use squarez::canvas::CanvasState;

#[test]
fn default_canvas_zoom_matches_approved_mockup_scale() {
    assert_eq!(CanvasState::default().zoom, 12.0);
}

#[test]
fn canvas_art_rect_is_centered_in_workspace_before_panning() {
    let canvas = CanvasState { zoom: 8.0, offset: Vec2::ZERO, texture: None, dragging_pan: false, last_mouse_pos: None };
    let workspace = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));

    let art_rect = canvas.art_rect(workspace, 32, 32);

    assert_eq!(art_rect.min, Pos2::new(272.0, 172.0));
    assert_eq!(art_rect.max, Pos2::new(528.0, 428.0));
}

#[test]
fn screen_to_canvas_uses_centered_art_origin() {
    let canvas = CanvasState { zoom: 8.0, offset: Vec2::ZERO, texture: None, dragging_pan: false, last_mouse_pos: None };
    let workspace = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));

    assert_eq!(canvas.screen_to_canvas(Pos2::new(272.0, 172.0), workspace, 32, 32), Some((0, 0)));
    assert_eq!(canvas.screen_to_canvas(Pos2::new(271.0, 172.0), workspace, 32, 32), None);
}

#[test]
fn scroll_zoom_only_changes_when_pointer_is_over_workspace() {
    let workspace = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
    let mut canvas = CanvasState::default();

    canvas.zoom_from_scroll(120.0, Some(Pos2::new(900.0, 300.0)), workspace);
    assert_eq!(canvas.zoom, 12.0);

    canvas.zoom_from_scroll(120.0, Some(Pos2::new(400.0, 300.0)), workspace);
    assert!(canvas.zoom > 12.0);
}
