// tests/app_undo_ui_tests.rs
//
// UI-level regression: drive real app frames headlessly and press Cmd+Z,
// proving the shortcut reaches the handler and reverts 3D edits.

use egui::{Key, Modifiers, RawInput};
use squarez::app::App;
use squarez::history::Command;
use squarez::project::{Project, ProjectMode};
use squarez::three_d::mesh::Mesh;

fn key_event(key: Key, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn frame_with(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
    // egui takes modifier state from RawInput, not from the event itself.
    let modifiers = events
        .iter()
        .find_map(|e| match e {
            egui::Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    let input = RawInput { modifiers, events, ..Default::default() };
    let _ = ctx.run(input, |ctx| app.update_ui(ctx));
}

fn three_d_project() -> (Project, Mesh, Mesh) {
    let mut p = Project::new_with_mode(128, 128, "undo".to_string(), ProjectMode::ThreeD);
    let mut before = Mesh::cube(8);
    before.allocate_all_islands((128, 128)).unwrap();
    squarez::three_d::paint_islands_checker(&mut p.animations[0].frames[0].layers[0], &before);
    let layer = p.animations[0].frames[0].layers[0].clone();
    let out = squarez::three_d::edit::extrude_faces_n(&before, &layer, &[1], 2, (128, 128)).unwrap();
    let after = out.mesh.clone();
    for &(x, y, _, new) in &out.pixel_edits {
        p.animations[0].frames[0].layers[0].set_pixel(x, y, new);
    }
    p.mesh3d = Some(after.clone());
    (p, before, after)
}

#[test]
fn cmd_z_undoes_a_3d_mesh_edit() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    let (project, before, after) = three_d_project();
    app.project = project;
    app.undo_stack.push(Command::MeshEdit {
        before: before.clone(),
        after: after.clone(),
        layer_id: 0,
        pixel_edits: Vec::new(),
    });

    frame_with(&ctx, &mut app, vec![]); // settle one frame
    assert_eq!(app.project.mesh3d.as_ref(), Some(&after));

    frame_with(&ctx, &mut app, vec![key_event(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(
        app.project.mesh3d.as_ref(),
        Some(&before),
        "Cmd+Z must revert the mesh edit in 3D mode"
    );

    // Shift+Cmd+Z redoes.
    frame_with(&ctx, &mut app, vec![key_event(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)]);
    assert_eq!(app.project.mesh3d.as_ref(), Some(&after), "Shift+Cmd+Z must redo in 3D mode");
}

#[test]
fn cmd_z_undoes_a_3d_paint_stroke() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    let (project, mesh, _) = three_d_project();
    app.project = project;
    app.project.mesh3d = Some(mesh.clone());

    let isl = mesh.faces[0].island;
    let (px, py) = (isl.x as u32, isl.y as u32);
    let original = app.project.animations[0].frames[0].layers[0].get_pixel(px, py);
    let painted = [7, 8, 9, 255];
    app.project.animations[0].frames[0].layers[0].set_pixel(px, py, painted);
    app.undo_stack.push(Command::PaintPixels {
        animation_id: 0,
        frame_id: 0,
        layer_id: 0,
        edits: vec![(px, py, original, painted)],
    });

    frame_with(&ctx, &mut app, vec![key_event(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(
        app.project.animations[0].frames[0].layers[0].get_pixel(px, py),
        original,
        "Cmd+Z must revert a 3D paint stroke"
    );
}
