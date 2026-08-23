// tests/undo_integrity_tests.rs
//
// Undo/redo integrity: drive a chain of real 3D + texture-view operations
// through app frames, snapshot the full document after each, then Cmd+Z all
// the way down comparing every checkpoint in reverse, and Cmd+Shift+Z all
// the way back up comparing forward. Any state the undo system forgets or
// corrupts fails the walk.

use egui::{Key, Modifiers, Pos2, RawInput, Rect, Vec2};
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

fn key(ctx: &egui::Context, app: &mut App, k: Key, m: Modifiers) {
    frame(ctx, app, vec![egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }], m);
    frame(ctx, app, vec![egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m }], m);
    frame(ctx, app, vec![], Modifiers::default());
}

fn three_d_project() -> Project {
    let mut p = Project::new_with_mode(128, 128, "undo-integrity".to_string(), ProjectMode::ThreeD);
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    squarez::three_d::paint_islands_checker(&mut p.animations[0].frames[0].layers[0], &mesh);
    p.mesh3d = Some(mesh);
    p
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    mesh: Option<Mesh>,
    pixels: Vec<u8>,
    canvas: (u32, u32),
}

fn snap(app: &App) -> Snapshot {
    let layer = &app.project.animations[0].frames[0].layers[0];
    let mut pixels = Vec::new();
    for y in 0..app.project.canvas_height {
        for x in 0..app.project.canvas_width {
            pixels.extend_from_slice(&layer.get_pixel(x, y));
        }
    }
    Snapshot {
        mesh: app.project.mesh3d.clone(),
        pixels,
        canvas: (app.project.canvas_width, app.project.canvas_height),
    }
}

fn diff(label_a: &str, a: &Snapshot, label_b: &str, b: &Snapshot) -> String {
    let mut out = String::new();
    if a.canvas != b.canvas {
        out += &format!("canvas {:?} vs {:?}; ", a.canvas, b.canvas);
    }
    match (&a.mesh, &b.mesh) {
        (Some(ma), Some(mb)) => {
            if ma.vertices != mb.vertices { out += "vertices differ; "; }
            if ma.faces != mb.faces { out += "faces differ; "; }
            if ma.manual_layout != mb.manual_layout { out += "manual_layout differs; "; }
        }
        _ => out += "mesh presence differs; ",
    }
    if a.canvas == b.canvas && a.pixels != b.pixels {
        let n = a.pixels.iter().zip(&b.pixels).filter(|(x, y)| x != y).count() / 4;
        out += &format!("~{n} pixels differ; ");
    }
    if out.is_empty() { out = "equal".into(); }
    format!("{label_a} vs {label_b}: {out}")
}

#[test]
fn undo_walks_the_full_operation_chain_both_ways() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(three_d_project());
    app.active_tool = ActiveTool::Select3D;
    for _ in 0..16 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    use squarez::three_d::camera::SnapView;
    app.three_d.camera.snap_to(SnapView::Front);

    let top_face = {
        let mesh = app.project.mesh3d.as_ref().unwrap();
        mesh.faces.iter().position(|f| mesh.face_normal(f)[1] > 0.5).unwrap() as u32
    };

    let mut checkpoints: Vec<(String, Snapshot)> = vec![("initial".into(), snap(&app))];
    let mut record = |label: &str, app: &App| {
        let s = snap(app);
        assert!(
            checkpoints.last().unwrap().1 != s || label.starts_with('~'),
            "operation '{label}' should have changed the document"
        );
        checkpoints.push((label.into(), s));
    };

    // 1. Arrow-nudge the top face up (odd delta).
    app.three_d.sel_faces = vec![top_face];
    key(&ctx, &mut app, Key::ArrowUp, Modifiers::default());
    record("nudge top face +Y", &app);

    // 2. Nudge everything left.
    key(&ctx, &mut app, Key::A, Modifiers::COMMAND); // select all models
    frame(&ctx, &mut app, vec![], Modifiers::default());
    key(&ctx, &mut app, Key::ArrowLeft, Modifiers::default());
    record("nudge all -X", &app);

    // 3. Duplicate the model (Cmd+D).
    app.active_tool = ActiveTool::MoveObject;
    frame(&ctx, &mut app, vec![], Modifiers::default());
    app.three_d.sel_faces = (0..app.project.mesh3d.as_ref().unwrap().faces.len() as u32).collect();
    key(&ctx, &mut app, Key::D, Modifiers::COMMAND);
    record("cmd+D duplicate", &app);

    // 4. Paint two pixels in texture view.
    app.three_d.atlas_prompted = true;
    app.toggle_texture_view();
    frame(&ctx, &mut app, vec![], Modifiers::default());
    {
        let isl = app.project.mesh3d.as_ref().unwrap().faces[0].island;
        let (x, y) = (isl.x as u32 + 1, isl.y as u32 + 1);
        app.push_paint_edits(&[(x, y, [220, 40, 40, 255]), (x + 1, y, [40, 220, 40, 255])]);
    }
    record("paint 2 texels", &app);

    // 5. Hand-drag an island (manual layout).
    {
        let before = app.project.mesh3d.as_ref().unwrap().faces[0].island;
        assert!(app.commit_island_move(&[0], (5, 3)), "island move must commit");
        let after = app.project.mesh3d.as_ref().unwrap().faces[0].island;
        assert_ne!(before, after, "island drag must move the island");
    }
    record("hand-move island", &app);

    // 6. A gradient across a face (one PaintPixels).
    {
        app.shading_ramp = Some((0, 1));
        app.shading_dir = 1;
        let isl = app.project.mesh3d.as_ref().unwrap().faces[1].island;
        let layer = app.project.animations[0].frames[0].layers[0].clone();
        app.set_gradient_face_for_test(isl.x as i32, isl.y as i32);
        let edits = app.gradient_edits(
            &layer,
            isl.x as i32,
            isl.y as i32,
            (isl.x + isl.w) as i32 - 1,
            (isl.y + isl.h) as i32 - 1,
        );
        assert!(!edits.is_empty(), "gradient must paint");
        let apply: Vec<(u32, u32, [u8; 4])> =
            edits.iter().map(|&(x, y, _, new)| (x, y, new)).collect();
        app.push_paint_edits(&apply);
    }
    record("gradient on face 1", &app);

    // Walk all the way down…
    for i in (1..checkpoints.len()).rev() {
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
        let want = &checkpoints[i - 1].1;
        let got = snap(&app);
        assert!(
            got == *want,
            "undo of '{}' failed: {}",
            checkpoints[i].0,
            diff("after-undo", &got, "expected", want)
        );
    }
    assert!(!app.undo_stack.can_undo(), "stack should be exhausted");

    // …and back up.
    for i in 1..checkpoints.len() {
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        let want = &checkpoints[i].1;
        let got = snap(&app);
        assert!(
            got == *want,
            "redo of '{}' failed: {}",
            checkpoints[i].0,
            diff("after-redo", &got, "expected", want)
        );
    }
}

#[test]
fn atlas_growth_and_atlas_resize_are_undoable() {
    // Two paths used to escape history: an edit that grows the atlas left
    // the canvas resized after undo, and the atlas-size dialog wiped the
    // entire undo stack ("undo stopped working" right after using it).
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    // Tight atlas: 6 islands of 8x8 on 32x32 leaves no room to grow a face.
    let mut p = Project::new_with_mode(32, 32, "grow".to_string(), ProjectMode::ThreeD);
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((32, 32)).unwrap();
    squarez::three_d::paint_islands_checker(&mut p.animations[0].frames[0].layers[0], &mesh);
    p.mesh3d = Some(mesh);
    app.open_project_for_test(p);
    app.active_tool = ActiveTool::Select3D;
    for _ in 0..16 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    use squarez::three_d::camera::SnapView;
    app.three_d.camera.snap_to(SnapView::Front);

    let start = snap(&app);
    assert_eq!(start.canvas, (32, 32));

    // Growing edit: pull the top face up — side islands get taller, the
    // packer needs more room, the atlas must grow.
    let top_face = {
        let mesh = app.project.mesh3d.as_ref().unwrap();
        mesh.faces.iter().position(|f| mesh.face_normal(f)[1] > 0.5).unwrap() as u32
    };
    app.three_d.sel_faces = vec![top_face];
    let mut steps: Vec<Snapshot> = Vec::new();
    for _ in 0..12 {
        key(&ctx, &mut app, Key::ArrowUp, Modifiers::default());
        steps.push(snap(&app));
        if steps.last().unwrap().canvas != (32, 32) {
            break;
        }
    }
    let grown = steps.last().unwrap().clone();
    assert_ne!(grown.canvas, (32, 32), "repeated nudges must eventually grow the atlas");

    for i in (0..steps.len()).rev() {
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
        let want = if i == 0 { &start } else { &steps[i - 1] };
        let got = snap(&app);
        assert!(
            got == *want,
            "undo step {i} of the growing chain failed: {}",
            diff("after-undo", &got, "expected", want)
        );
    }
    for (i, want) in steps.iter().enumerate() {
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        let got = snap(&app);
        assert!(
            got == *want,
            "redo step {i} of the growing chain failed: {}",
            diff("after-redo", &got, "expected", want)
        );
    }

    // Atlas-size dialog: must neither wipe history nor be unundoable itself.
    let before_resize = snap(&app);
    assert!(app.apply_atlas_size(96, 64), "resize must be accepted");
    let resized = snap(&app);
    assert_eq!(resized.canvas, (96, 64));

    key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    let undone = snap(&app);
    assert!(
        undone == before_resize,
        "undo of the atlas resize must restore everything: {}",
        diff("after-undo", &undone, "before-resize", &before_resize)
    );
    // History from before the resize is still alive: unwind the whole
    // growth chain beneath it, all the way to the initial state.
    for _ in 0..steps.len() {
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    }
    let undone2 = snap(&app);
    assert!(
        undone2 == start,
        "the pre-resize history must survive the resize: {}",
        diff("after-full-unwind", &undone2, "start", &start)
    );
    // And redo replays the entire chain plus the resize exactly.
    for _ in 0..=steps.len() {
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    }
    let replayed = snap(&app);
    assert!(
        replayed == resized,
        "redo through the resize must replay exactly: {}",
        diff("after-full-replay", &replayed, "resized", &resized)
    );
}
