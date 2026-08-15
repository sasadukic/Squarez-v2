// tests/three_d_interaction_tests.rs
//
// End-to-end 3D interaction through real app frames: click the model with
// the Extrude tool, drag, release — then Cmd+Z must revert it.

use egui::{Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use squarez::app::App;
use squarez::project::{Project, ProjectMode};
use squarez::three_d::mesh::Mesh;
use squarez::tools::ActiveTool;

const SCREEN: Vec2 = Vec2::new(1200.0, 800.0);

fn base_input(events: Vec<egui::Event>, modifiers: Modifiers) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN)),
        modifiers,
        events,
        ..Default::default()
    }
}

fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>, modifiers: Modifiers) {
    let _ = ctx.run(base_input(events, modifiers), |c| app.update_ui(c));
}

fn three_d_project() -> Project {
    let mut p = Project::new_with_mode(128, 128, "interact".to_string(), ProjectMode::ThreeD);
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    squarez::three_d::paint_islands_checker(&mut p.animations[0].frames[0].layers[0], &mesh);
    p.mesh3d = Some(mesh);
    p
}

/// egui applies zoom_factor 1.5, so points ≠ pixels; aim at the middle of
/// the workspace area (right of the toolbar, left of the sidebar).
fn model_point() -> Pos2 {
    Pos2::new(SCREEN.x / 2.0 / 1.5, SCREEN.y / 2.0 / 1.5)
}

#[test]
fn extrude_drag_then_cmd_z_reverts_in_3d() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::Extrude;

    // Settle a couple of frames so layout/camera stabilize.
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    let faces_before = app.project.mesh3d.as_ref().unwrap().faces.len();
    let mesh_before = app.project.mesh3d.clone().unwrap();

    let p = model_point();
    // Press on the model.
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    // Drag upward a few frames (extrude outward).
    for i in 1..=6 {
        let q = Pos2::new(p.x, p.y - i as f32 * 8.0);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(q)], Modifiers::default());
    }
    let q = Pos2::new(p.x, p.y - 48.0);
    // Release.
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![], Modifiers::default());

    let faces_after = app.project.mesh3d.as_ref().unwrap().faces.len();
    assert!(
        faces_after > faces_before,
        "extrude drag should add geometry ({faces_before} -> {faces_after})"
    );
    assert!(app.undo_stack.can_undo(), "the extrude must be recorded in history");

    // Cmd+Z reverts it.
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND },
    ], Modifiers::COMMAND);
    assert_eq!(
        app.project.mesh3d.as_ref(),
        Some(&mesh_before),
        "Cmd+Z after an extrude drag must restore the original mesh"
    );

    // And the restored state must survive subsequent idle frames + a click.
    frame(&ctx, &mut app, vec![], Modifiers::default());
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert_eq!(
        app.project.mesh3d.as_ref(),
        Some(&mesh_before),
        "a stray pointer release must not resurrect the undone edit"
    );
}

#[test]
fn dangling_drag_cannot_clobber_an_undo() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::Select3D;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    let pristine = app.project.mesh3d.clone().unwrap();
    // A newer, committed edit sits on top of history.
    let layer = app.project.animations[0].frames[0].layers[0].clone();
    let out = squarez::three_d::edit::extrude_faces_n(&pristine, &layer, &[1], 2, (128, 128)).unwrap();
    app.project.mesh3d = Some(out.mesh.clone());
    app.undo_stack.push(squarez::history::Command::MeshEdit {
        before: pristine.clone(),
        after: out.mesh.clone(),
        layer_id: 0,
        pixel_edits: out.pixel_edits.clone(),
    });

    // Simulate a drag gesture whose release was never observed (e.g. the
    // press happened, then focus/tool state changed): a stale snapshot of a
    // much older mesh is still parked in the workspace state.
    let stale = Mesh::cube(4);
    app.three_d.drag = Some(squarez::three_d::VertexDrag {
        start_mesh: stale.clone(),
        start_layer: layer.clone(),
        verts: vec![0],
        raw: [0.0; 3],
        applied: [0; 3],
    });

    // Undo the extrude, then let a pointer release arrive.
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND },
    ], Modifiers::COMMAND);
    assert_eq!(app.project.mesh3d.as_ref(), Some(&pristine), "undo should restore the pre-extrude mesh");

    let p = model_point();
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert_eq!(
        app.project.mesh3d.as_ref(),
        Some(&pristine),
        "a stale drag snapshot must never overwrite the undone state"
    );
}
