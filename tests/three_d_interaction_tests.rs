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

/// Press+release a plain click at `p`, settle, and return the selection.
fn click_at(ctx: &egui::Context, app: &mut App, p: Pos2, shift: bool) -> Vec<u32> {
    let m = if shift { Modifiers::SHIFT } else { Modifiers::default() };
    frame(ctx, app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m },
    ], m);
    frame(ctx, app, vec![
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: m },
    ], m);
    frame(ctx, app, vec![], Modifiers::default());
    let mut sel = app.three_d.sel_faces.clone();
    sel.sort_unstable();
    sel
}

/// Scan the workspace for a screen point whose plain click selects exactly
/// `target` — robust against layout/zoom-to-fit placing the models anywhere.
fn find_click_point(ctx: &egui::Context, app: &mut App, target: &[u32]) -> Pos2 {
    let c = model_point();
    for dy in (-13..=13).map(|k| k as f32 * 12.0) {
        for dx in (-13..=13).map(|k| k as f32 * 12.0) {
            let p = Pos2::new(c.x + dx, c.y + dy);
            if click_at(ctx, app, p, false) == target {
                return p;
            }
        }
    }
    panic!("no screen point selects faces {target:?}");
}

#[test]
fn shift_click_moves_several_models_as_one() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    let mut p = three_d_project();
    // Second model stacked above the first (add_object lifts it clear).
    let layer = p.animations[0].frames[0].layers[0].clone();
    let out = squarez::three_d::edit::add_object(
        p.mesh3d.as_ref().unwrap(),
        &layer,
        &Mesh::cube(6),
        (128, 128),
    )
    .unwrap();
    for &(x, y, _, new) in &out.pixel_edits {
        p.animations[0].frames[0].layers[0].set_pixel(x, y, new);
    }
    p.mesh3d = Some(out.mesh);
    app.open_project_for_test(p);
    app.active_tool = ActiveTool::MoveObject;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    let cube_a: Vec<u32> = (0..6).collect();
    let cube_b: Vec<u32> = (6..12).collect();
    let both: Vec<u32> = (0..12).collect();

    let pa = find_click_point(&ctx, &mut app, &cube_a);
    let pb = find_click_point(&ctx, &mut app, &cube_b);

    // Plain click selects one model; shift-click adds the other.
    assert_eq!(click_at(&ctx, &mut app, pa, false), cube_a);
    assert_eq!(click_at(&ctx, &mut app, pb, true), both, "shift-click must add the second model");
    // Shift-click on a selected model toggles it back out...
    assert_eq!(click_at(&ctx, &mut app, pb, true), cube_a);
    // ...and back in for the move.
    assert_eq!(click_at(&ctx, &mut app, pb, true), both);

    // Drag from the FIRST model: everything selected must move together.
    let before = app.project.mesh3d.clone().unwrap();
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(pa),
        egui::Event::PointerButton { pos: pa, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    for i in 1..=6 {
        let q = Pos2::new(pa.x + i as f32 * 10.0, pa.y);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(q)], Modifiers::default());
    }
    let q = Pos2::new(pa.x + 60.0, pa.y);
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![], Modifiers::default());

    let after = app.project.mesh3d.clone().unwrap();
    let delta: Vec<[f32; 3]> = before
        .vertices
        .iter()
        .zip(after.vertices.iter())
        .map(|(b, a)| [a[0] - b[0], a[1] - b[1], a[2] - b[2]])
        .collect();
    assert!(
        delta[0] != [0.0, 0.0, 0.0],
        "the drag must actually move the selection"
    );
    assert!(
        delta.iter().all(|d| *d == delta[0]),
        "every vertex of BOTH models must move by the same delta: {delta:?}"
    );
    assert!(app.undo_stack.can_undo(), "the group move must land in history");

    // Plain click on empty space still clears a multi-selection.
    let empty = Pos2::new(model_point().x, 60.0);
    assert_eq!(click_at(&ctx, &mut app, empty, false), Vec::<u32>::new());
}
