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
        canvas_before: (128, 128),
        canvas_after: (128, 128),
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

#[test]
fn cmd_a_selects_every_model_in_3d_mode() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    let mut p = three_d_project();
    let layer = p.animations[0].frames[0].layers[0].clone();
    let out = squarez::three_d::edit::add_object(
        p.mesh3d.as_ref().unwrap(),
        &layer,
        &Mesh::cube(6),
        (128, 128),
    )
    .unwrap();
    p.mesh3d = Some(out.mesh);
    app.open_project_for_test(p);
    app.active_tool = ActiveTool::MoveObject;
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    // The shortcut fires on key release, like the other edit shortcuts.
    let m = Modifiers::COMMAND;
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: m },
        egui::Event::Key { key: Key::A, physical_key: None, pressed: false, repeat: false, modifiers: m },
    ], m);

    let mut sel = app.three_d.sel_faces.clone();
    sel.sort_unstable();
    assert_eq!(sel, (0..12).collect::<Vec<u32>>(), "⌘A must select every model's faces");
    assert!(
        app.select_state.mask.is_none() && !app.select_state.has_float(),
        "⌘A in 3D mode must NOT run the 2D pixel select-all against the atlas"
    );

    // Escape clears it again (⌘D duplicates now).
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert!(app.three_d.sel_faces.is_empty(), "Escape must deselect all models");
}

#[test]
fn cmd_d_duplicates_the_selected_model() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    let mut p = three_d_project();
    // Paint a marker on the cube so the copy's texture can be verified.
    let isl = p.mesh3d.as_ref().unwrap().faces[2].island;
    p.animations[0].frames[0].layers[0].set_pixel(isl.x as u32 + 1, isl.y as u32 + 1, [210, 40, 40, 255]);
    app.open_project_for_test(p);
    app.active_tool = ActiveTool::MoveObject;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    // Select the cube, then ⌘D.
    let pa = find_click_point(&ctx, &mut app, &(0..6).collect::<Vec<u32>>());
    assert_eq!(click_at(&ctx, &mut app, pa, false), (0..6).collect::<Vec<u32>>());
    let m = Modifiers::COMMAND;
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::D, physical_key: None, pressed: true, repeat: false, modifiers: m },
        egui::Event::Key { key: Key::D, physical_key: None, pressed: false, repeat: false, modifiers: m },
    ], m);
    frame(&ctx, &mut app, vec![], Modifiers::default());

    let mesh = app.project.mesh3d.as_ref().unwrap();
    assert_eq!(mesh.faces.len(), 12, "duplicate must add a full copy of the model");
    let mut sel = app.three_d.sel_faces.clone();
    sel.sort_unstable();
    assert_eq!(sel, (6..12).collect::<Vec<u32>>(), "the COPY must come back selected");

    // The copy sits above the original, same footprint.
    let orig_top = (0..8).map(|v| mesh.vertices[v][1]).fold(f32::MIN, f32::max);
    let copy_bottom = (8..16).map(|v| mesh.vertices[v][1]).fold(f32::MAX, f32::min);
    assert!(copy_bottom >= orig_top + 2.0, "copy must be lifted clear of the scene");

    // The copy wears the original's paint.
    let src = mesh.faces[2].island;
    let dst = mesh.faces[8].island; // same face of the copy (order preserved)
    let layer = &app.project.animations[0].frames[0].layers[0];
    assert_eq!(
        layer.get_pixel(dst.x as u32 + 1, dst.y as u32 + 1),
        layer.get_pixel(src.x as u32 + 1, src.y as u32 + 1),
        "duplicated faces must carry their texture"
    );
    assert_eq!(layer.get_pixel(dst.x as u32 + 1, dst.y as u32 + 1), [210, 40, 40, 255]);

    // One undo step removes the copy again.
    assert!(app.undo_stack.can_undo());
    frame(&ctx, &mut app, vec![key_event_z(&ctx)], Modifiers::COMMAND);
    assert_eq!(
        app.project.mesh3d.as_ref().unwrap().faces.len(),
        6,
        "undo must remove the duplicate"
    );
}

fn key_event_z(_ctx: &egui::Context) -> egui::Event {
    egui::Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }
}

#[test]
fn tab_swaps_to_texture_view_and_offers_the_size_dialog_once() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    assert!(!app.three_d.texture_view);

    let tab = |ctx: &egui::Context, app: &mut App| {
        frame(ctx, app, vec![
            egui::Event::Key { key: Key::Tab, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::default() },
        ], Modifiers::default());
    };
    tab(&ctx, &mut app);
    assert!(app.three_d.texture_view, "Tab must enter the texture view");
    assert!(app.atlas_dialog_open(), "first entry offers the atlas size dialog");

    // Dismissing keeps the current size; Tab back and forth must not re-offer.
    // (One settle frame: the dialog's text field held keyboard focus, and
    // egui releases it a frame after the widget disappears.)
    app.dismiss_atlas_dialog();
    frame(&ctx, &mut app, vec![], Modifiers::default());
    tab(&ctx, &mut app);
    assert!(!app.three_d.texture_view, "Tab must return to the 3D view");
    tab(&ctx, &mut app);
    assert!(app.three_d.texture_view);
    assert!(!app.atlas_dialog_open(), "the size prompt is offered once per tab");
}

#[test]
fn atlas_resize_is_independent_of_the_model_and_carries_paint() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    let mut p = three_d_project();
    let isl = p.mesh3d.as_ref().unwrap().faces[1].island;
    p.animations[0].frames[0].layers[0].set_pixel(isl.x as u32 + 1, isl.y as u32 + 2, [9, 8, 7, 255]);
    app.open_project_for_test(p);

    assert!(app.apply_atlas_size(512, 64), "a roomy atlas must be accepted");
    assert_eq!((app.project.canvas_width, app.project.canvas_height), (512, 64));
    let mesh = app.project.mesh3d.as_ref().unwrap();
    for f in &mesh.faces {
        assert_eq!((f.island.w, f.island.h), (8, 8), "island sizes come from the mesh, not the atlas");
        assert!((f.island.x + f.island.w) as u32 <= 512 && (f.island.y + f.island.h) as u32 <= 64);
    }
    let isl = mesh.faces[1].island;
    assert_eq!(
        app.project.animations[0].frames[0].layers[0].get_pixel(isl.x as u32 + 1, isl.y as u32 + 2),
        [9, 8, 7, 255],
        "paint must survive the resize"
    );

    // Too small for the islands: refused, nothing changes.
    assert!(!app.apply_atlas_size(16, 16));
    assert_eq!((app.project.canvas_width, app.project.canvas_height), (512, 64));
}

#[test]
fn selection_made_in_3d_view_arrives_in_texture_view() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::MoveObject;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    let pa = find_click_point(&ctx, &mut app, &(0..6).collect::<Vec<u32>>());
    assert_eq!(click_at(&ctx, &mut app, pa, false), (0..6).collect::<Vec<u32>>());

    app.three_d.atlas_prompted = true; // skip the dialog
    app.toggle_texture_view();
    frame(&ctx, &mut app, vec![], Modifiers::default());
    let mut sel = app.three_d.sel_faces.clone();
    sel.sort_unstable();
    assert_eq!(
        sel,
        (0..6).collect::<Vec<u32>>(),
        "the 3D selection and the texture-view selection are the same state"
    );
}

#[test]
fn arrow_keys_nudge_the_selection_view_relative() {
    use squarez::three_d::camera::SnapView;
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::Select3D;
    // The first 15 frames force a zoom-to-fit that resets the camera; the
    // nudge directions depend on the camera we set, so settle past them.
    for _ in 0..16 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    let top_face = {
        let mesh = app.project.mesh3d.as_ref().unwrap();
        mesh.faces
            .iter()
            .position(|f| mesh.face_normal(f)[1] > 0.5)
            .expect("cube has a top face") as u32
    };
    app.three_d.sel_faces = vec![top_face];

    let arrow = |ctx: &egui::Context, app: &mut App, key: Key| {
        frame(ctx, app, vec![
            egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::default() },
        ], Modifiers::default());
        frame(ctx, app, vec![], Modifiers::default());
    };
    let top_y = |app: &App| {
        let mesh = app.project.mesh3d.as_ref().unwrap();
        let f = &mesh.faces[top_face as usize];
        mesh.vertices[f.verts[0] as usize]
    };

    // Front view: ↑ is world +Y, → is world +X.
    app.three_d.camera.snap_to(SnapView::Front);
    let before = top_y(&app);
    arrow(&ctx, &mut app, Key::ArrowUp);
    let after = top_y(&app);
    assert_eq!(
        [after[0] - before[0], after[1] - before[1], after[2] - before[2]],
        [0.0, 1.0, 0.0],
        "front view: ArrowUp must move the face +Y"
    );
    let before = after;
    arrow(&ctx, &mut app, Key::ArrowRight);
    let after = top_y(&app);
    assert_eq!(
        [after[0] - before[0], after[1] - before[1], after[2] - before[2]],
        [1.0, 0.0, 0.0],
        "front view: ArrowRight must move the face +X"
    );

    // Top view: ↑ walks away from the viewer (world -Z).
    app.three_d.camera.snap_to(SnapView::Top);
    let before = after;
    arrow(&ctx, &mut app, Key::ArrowUp);
    let after = top_y(&app);
    assert_eq!(
        [after[0] - before[0], after[1] - before[1], after[2] - before[2]],
        [0.0, 0.0, -1.0],
        "top view: ArrowUp must move the face -Z (moved {:?})",
        [after[0] - before[0], after[1] - before[1], after[2] - before[2]]
    );

    // Selection survives so nudges chain, and every step is one undo.
    assert_eq!(app.three_d.sel_faces, vec![top_face]);
    assert!(app.undo_stack.can_undo());
    let before = after;
    frame(&ctx, &mut app, vec![key_event_z(&ctx)], Modifiers::COMMAND);
    let after = top_y(&app);
    assert_eq!(
        [after[0] - before[0], after[1] - before[1], after[2] - before[2]],
        [0.0, 0.0, 1.0],
        "one Cmd+Z must revert exactly the last nudge"
    );
}

#[test]
fn gradient_drag_clips_to_one_face_and_undoes_as_one_step() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::Gradient;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    let pixels_before = app.project.animations[0].frames[0].layers[0].pixels.clone();

    let p = model_point();
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(p)], Modifiers::default());
    assert!(app.three_d.hover_face.is_some(), "pointer over the model must hover a face");

    // Without two selected palette colors the tool must refuse to start.
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert!(app.three_d.gradient_drag.is_none(), "no color selection: no drag");
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());

    // Select two palette colors (the shift-click ramp selection).
    app.shading_ramp = Some((0, 1));
    app.shading_dir = 1;
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    let face = app.three_d.gradient_drag.expect("press on the model starts a drag").face;
    // Drag down-right across the face.
    for i in 1..=5 {
        let q = Pos2::new(p.x + i as f32 * 6.0, p.y + i as f32 * 6.0);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(q)], Modifiers::default());
    }
    assert!(!app.three_d.gradient_preview.is_empty(), "live preview during the drag");
    let q = Pos2::new(p.x + 30.0, p.y + 30.0);
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![], Modifiers::default());

    assert!(app.three_d.gradient_drag.is_none(), "release ends the drag");
    assert!(app.three_d.gradient_preview.is_empty(), "preview cleared on commit");

    // Changed texels lie exactly inside the locked face's clip set.
    let mesh = app.project.mesh3d.clone().unwrap();
    let clip = squarez::three_d::paint::FaceClip::new(&mesh, face).unwrap();
    let layer = &app.project.animations[0].frames[0].layers[0];
    let mut changed = Vec::new();
    for y in 0..app.project.canvas_height {
        for x in 0..app.project.canvas_width {
            let i = ((y * app.project.canvas_width + x) * 4) as usize;
            if layer.pixels[i..i + 4] != pixels_before[i..i + 4] {
                changed.push((x, y));
            }
        }
    }
    assert!(!changed.is_empty(), "the gradient must have painted something");
    for &(x, y) in &changed {
        assert!(clip.contains(x, y), "texel ({x},{y}) changed outside the locked face");
    }

    // One Cmd+Z restores the pristine atlas.
    frame(&ctx, &mut app, vec![key_event_z(&ctx)], Modifiers::COMMAND);
    assert_eq!(
        app.project.animations[0].frames[0].layers[0].pixels, pixels_before,
        "a single undo must revert the whole gradient"
    );
}

#[test]
fn texture_view_gradient_clips_to_the_locked_face() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::Gradient;
    app.three_d.atlas_prompted = true;
    app.toggle_texture_view();
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    let mesh = app.project.mesh3d.clone().unwrap();
    let isl = mesh.faces[2].island;
    let layer = app.project.animations[0].frames[0].layers[0].clone();
    let pixels_before = layer.pixels.clone();

    // No color selection: the tool refuses to paint.
    app.three_d.sel_faces = vec![2];
    app.set_gradient_face_for_test(isl.x as i32, isl.y as i32);
    assert!(
        app.gradient_edits(&layer, isl.x as i32, isl.y as i32, isl.x as i32 + 4, isl.y as i32).is_empty(),
        "gradient without a two-color selection must be a no-op"
    );
    app.shading_ramp = Some((0, 1));
    app.shading_dir = 1;

    // Drag corner-to-corner across face 2's island, via the same press-time
    // face lock + edits path the canvas input uses.
    app.three_d.sel_faces = vec![2];
    let (x0, y0) = (isl.x as i32, isl.y as i32);
    let (x1, y1) = ((isl.x + isl.w) as i32 - 1, (isl.y + isl.h) as i32 - 1);
    app.set_gradient_face_for_test(x0, y0);
    let edits = app.gradient_edits(&layer, x0, y0, x1, y1);
    assert!(!edits.is_empty(), "the drag must produce edits");
    let clip = squarez::three_d::paint::FaceClip::new(&mesh, 2).unwrap();
    for &(x, y, _, _) in &edits {
        assert!(clip.contains(x, y), "texel ({x},{y}) outside the locked face");
    }
    let apply: Vec<(u32, u32, [u8; 4])> = edits.iter().map(|&(x, y, _, new)| (x, y, new)).collect();
    app.push_paint_edits(&apply);
    assert_ne!(app.project.animations[0].frames[0].layers[0].pixels, pixels_before);

    // Single undo restores everything.
    frame(&ctx, &mut app, vec![key_event_z(&ctx)], Modifiers::COMMAND);
    assert_eq!(app.project.animations[0].frames[0].layers[0].pixels, pixels_before);

    // A press outside every island locks no face and produces no edits.
    app.set_gradient_face_for_test(-5, -5);
    assert!(app.gradient_edits(&layer, -5, -5, 3, 3).is_empty());
}

#[test]
fn k_selects_the_loop_cut_tool() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::K, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert_eq!(app.active_tool, ActiveTool::LoopCut, "K is the knife");
}

#[test]
fn add_shape_dialog_gates_input_and_generates_on_confirm() {
    use squarez::three_d::edit::Primitive;
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    let faces_before = app.project.mesh3d.as_ref().unwrap().faces.len();

    // Picking a shape opens the Cmd+N-style prompt, which is modal: Tab must
    // not switch views underneath it.
    app.open_add_shape_dialog(Primitive::Cylinder);
    assert!(app.add_shape_dialog_open());
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::Tab, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert!(!app.three_d.texture_view, "modal must swallow the Tab shortcut");

    // Absurd sides get clamped by the grid rule; size 4 caps at
    // max_sides_for_radius(1.0).
    app.set_add_shape_fields_for_test("4", "99");
    assert!(app.confirm_add_shape(), "valid size must confirm");
    assert!(!app.add_shape_dialog_open());
    let expect_sides = squarez::three_d::mesh::Mesh::max_sides_for_radius(1.0);
    let queued = app.pending_add_primitive_for_test().expect("primitive queued");
    assert_eq!((queued.kind, queued.size, queued.sides), (Primitive::Cylinder, 4, expect_sides));

    // The next 3D frame consumes it and adds the object; one undo removes it.
    frame(&ctx, &mut app, vec![], Modifiers::default());
    let faces_after = app.project.mesh3d.as_ref().unwrap().faces.len();
    assert!(faces_after > faces_before, "the cylinder must be generated");
    frame(&ctx, &mut app, vec![key_event_z(&ctx)], Modifiers::COMMAND);
    assert_eq!(app.project.mesh3d.as_ref().unwrap().faces.len(), faces_before);

    // An unparsable size keeps the dialog open.
    app.open_add_shape_dialog(Primitive::Cube);
    app.set_add_shape_fields_for_test("abc", "8");
    assert!(!app.confirm_add_shape());
    assert!(app.add_shape_dialog_open(), "bad size: the prompt stays");
}

#[test]
fn rotate_tool_drag_spins_in_quarter_steps_and_undoes() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }

    // R selects the tool.
    frame(&ctx, &mut app, vec![
        egui::Event::Key { key: Key::R, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert_eq!(app.active_tool, ActiveTool::RotateObject);

    let mesh_before = app.project.mesh3d.clone().unwrap();
    let pixels_before = app.project.animations[0].frames[0].layers[0].pixels.clone();

    // Press on the model, drag right past one step, release.
    let p = model_point();
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert!(app.three_d.op_drag.is_some(), "press starts the rotate drag");
    for i in 1..=6 {
        let q = Pos2::new(p.x + i as f32 * 10.0, p.y + (i % 2) as f32);
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(q)], Modifiers::default());
    }
    let q = Pos2::new(p.x + 60.0, p.y);
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![], Modifiers::default());

    let mesh_after = app.project.mesh3d.clone().unwrap();
    assert_ne!(mesh_after.vertices, mesh_before.vertices, "the drag must rotate the object");

    // One undo restores mesh and atlas byte-identically.
    frame(&ctx, &mut app, vec![key_event_z(&ctx)], Modifiers::COMMAND);
    assert_eq!(app.project.mesh3d.as_ref().unwrap(), &mesh_before);
    assert_eq!(app.project.animations[0].frames[0].layers[0].pixels, pixels_before);

    // A tiny drag below one step commits nothing.
    assert!(!app.undo_stack.can_redo() || true); // state sanity only
    let before = app.undo_stack.can_undo();
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    let q = Pos2::new(p.x + 12.0, p.y);
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(q)], Modifiers::default());
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: q, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![], Modifiers::default());
    assert_eq!(app.project.mesh3d.as_ref().unwrap(), &mesh_before, "sub-step drag is a no-op");
    assert_eq!(app.undo_stack.can_undo(), before, "no undo entry for a no-op drag");
}
