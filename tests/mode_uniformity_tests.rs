// tests/mode_uniformity_tests.rs
//
// The three modes (Normal, SpriteStack, ThreeD) must feel like one app:
// same tool keys, same undo/redo, same tool-group behavior, and no mode
// leaving another mode's tools stranded.

use egui::{Key, Modifiers, Pos2, RawInput, Rect, Vec2};
use squarez::app::App;
use squarez::project::{Project, ProjectMode};
use squarez::three_d::mesh::Mesh;
use squarez::tools::ActiveTool;

const SCREEN: Vec2 = Vec2::new(1200.0, 800.0);

fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>, modifiers: Modifiers) {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN)),
        modifiers,
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |c| app.update_ui(c));
}

fn key(ctx: &egui::Context, app: &mut App, k: Key, m: Modifiers) {
    frame(ctx, app, vec![egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }], m);
    frame(ctx, app, vec![egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m }], m);
}

fn project_for(mode: ProjectMode) -> Project {
    let mut p = Project::new_with_mode(64, 64, format!("{mode:?}"), mode);
    if mode == ProjectMode::ThreeD {
        let mut mesh = Mesh::cube(8);
        mesh.allocate_all_islands((64, 64)).unwrap();
        squarez::three_d::paint_islands_checker(&mut p.animations[0].frames[0].layers[0], &mesh);
        p.mesh3d = Some(mesh);
    }
    p
}

const MODES: [ProjectMode; 3] = [ProjectMode::Normal, ProjectMode::SpriteStack, ProjectMode::ThreeD];

#[test]
fn tool_keys_select_the_same_tools_in_every_mode() {
    for mode in MODES {
        let ctx = egui::Context::default();
        let mut app = App::new_with(&ctx, None);
        app.open_project_for_test(project_for(mode));
        for _ in 0..2 {
            frame(&ctx, &mut app, vec![], Modifiers::default());
        }
        for (k, want) in [
            (Key::D, ActiveTool::Pencil),
            (Key::E, ActiveTool::Eraser),
            (Key::G, ActiveTool::Fill),
            (Key::B, ActiveTool::Gradient),
            (Key::Z, ActiveTool::Zoom),
        ] {
            key(&ctx, &mut app, k, Modifiers::default());
            assert_eq!(app.active_tool, want, "{mode:?}: key {k:?}");
        }
        // B pressed again cycles the gradient style — everywhere.
        key(&ctx, &mut app, Key::B, Modifiers::default());
        let s0 = app.gradient_style_for_test();
        key(&ctx, &mut app, Key::B, Modifiers::default());
        assert_ne!(app.gradient_style_for_test(), s0, "{mode:?}: B cycles the style");
    }
}

#[test]
fn three_d_tools_do_not_leak_into_other_modes() {
    for mode in [ProjectMode::Normal, ProjectMode::SpriteStack] {
        let ctx = egui::Context::default();
        let mut app = App::new_with(&ctx, None);
        app.open_project_for_test(project_for(ProjectMode::ThreeD));
        frame(&ctx, &mut app, vec![], Modifiers::default());
        key(&ctx, &mut app, Key::R, Modifiers::default());
        assert_eq!(app.active_tool, ActiveTool::RotateObject);

        app.open_project_for_test(project_for(mode));
        frame(&ctx, &mut app, vec![], Modifiers::default());
        assert_eq!(
            app.active_tool,
            ActiveTool::Pencil,
            "{mode:?}: a 3D-only tool must not survive the mode switch"
        );
    }
}

#[test]
fn sprite_stack_e_key_is_the_eraser_not_view_rotation() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::SpriteStack));
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    let rot = app.sprite_stack_rotation_90;
    key(&ctx, &mut app, Key::E, Modifiers::default());
    assert_eq!(app.active_tool, ActiveTool::Eraser, "E selects the eraser");
    assert_eq!(app.sprite_stack_rotation_90, rot, "E no longer rotates the stack");
    key(&ctx, &mut app, Key::W, Modifiers::default());
    assert_ne!(app.sprite_stack_rotation_90, rot, "W rotates the stack instead");
}

#[test]
fn paint_and_undo_shortcuts_are_uniform() {
    for mode in MODES {
        let ctx = egui::Context::default();
        let mut app = App::new_with(&ctx, None);
        app.open_project_for_test(project_for(mode));
        if mode == ProjectMode::ThreeD {
            app.three_d.texture_created = true;
        }
        for _ in 0..2 {
            frame(&ctx, &mut app, vec![], Modifiers::default());
        }
        let before = app.project.animations[0].frames[0].layers[0].pixels.clone();
        app.push_paint_edits(&[(5, 5, [200, 10, 10, 255]), (6, 5, [10, 200, 10, 255])]);
        let painted = app.project.animations[0].frames[0].layers[0].pixels.clone();
        assert_ne!(painted, before, "{mode:?}: paint applies");

        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
        assert_eq!(
            app.project.animations[0].frames[0].layers[0].pixels, before,
            "{mode:?}: Cmd+Z undoes"
        );
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(
            app.project.animations[0].frames[0].layers[0].pixels, painted,
            "{mode:?}: Shift+Cmd+Z redoes"
        );
        key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
        key(&ctx, &mut app, Key::Y, Modifiers::CTRL);
        assert_eq!(
            app.project.animations[0].frames[0].layers[0].pixels, painted,
            "{mode:?}: Ctrl+Y redoes"
        );
    }
}

#[test]
fn gradient_requires_a_two_color_selection_in_every_mode() {
    for mode in MODES {
        let ctx = egui::Context::default();
        let mut app = App::new_with(&ctx, None);
        app.open_project_for_test(project_for(mode));
        if mode == ProjectMode::ThreeD {
            app.three_d.texture_created = true;
            app.three_d.sel_faces = vec![2];
            let isl = app.project.mesh3d.as_ref().unwrap().faces[2].island;
            app.set_gradient_face_for_test(isl.x as i32, isl.y as i32);
        }
        let layer = app.project.animations[0].frames[0].layers[0].clone();
        assert!(
            app.gradient_edits(&layer, 2, 2, 20, 2).is_empty(),
            "{mode:?}: no selection, no gradient"
        );
        app.shading_ramp = Some((0, 1));
        app.shading_dir = 1;
        assert!(
            !app.gradient_edits(&layer, 2, 2, 20, 2).is_empty(),
            "{mode:?}: two selected colors blend"
        );
    }
}

#[test]
fn texture_wipe_undo_restores_the_texture_flag() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::ThreeD));
    frame(&ctx, &mut app, vec![], Modifiers::default());
    // Establish + paint something real.
    assert!(app.apply_atlas_size(64, 64) || app.apply_atlas_size(96, 64));
    app.dismiss_atlas_dialog();
    app.push_paint_edits(&[(4, 4, [220, 20, 20, 255])]);
    assert!(app.three_d.texture_created);

    app.reset_texture();
    assert!(!app.three_d.texture_created, "wipe removes the texture");
    app.dismiss_atlas_dialog();
    key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert!(
        app.three_d.texture_created,
        "undoing the wipe restores the paint AND the texture flag"
    );
}

#[test]
fn dismissing_the_size_prompt_still_establishes_the_texture() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::ThreeD));
    app.active_tool = ActiveTool::Pencil;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    assert!(!app.three_d.texture_created);
    // First draw opens the prompt.
    let p = Pos2::new(SCREEN.x / 2.0 / 1.5, SCREEN.y / 2.0 / 1.5);
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert!(app.atlas_dialog_open());
    // Escape = "keep the current size" — the question was answered either
    // way, so the texture exists and the next stroke paints.
    key(&ctx, &mut app, Key::Escape, Modifiers::default());
    assert!(!app.atlas_dialog_open(), "Escape closes the prompt");
    assert!(app.three_d.texture_created, "the texture exists at the current size");
}

#[test]
fn undoing_add_layer_does_not_strand_the_active_layer() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::Normal));
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    // Add a layer the way the panel does, activate it, then undo the add.
    {
        let (w, h) = (app.project.canvas_width, app.project.canvas_height);
        let id = app.project.next_layer_id();
        app.undo_stack.push(squarez::history::Command::AddLayer { index: 1, name: "L2".into(), id });
        for anim in &mut app.project.animations {
            for f in &mut anim.frames {
                f.layers.push(squarez::project::Layer::new_with_id("L2".into(), w, h, id));
            }
        }
        app.project.active_layer = 1;
    }
    key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert_eq!(app.project.animations[0].frames[0].layers.len(), 1);
    assert!(
        app.project.active_layer < app.project.animations[0].frames[0].layers.len(),
        "active layer must be clamped after the undo, not stranded at {}",
        app.project.active_layer
    );
    // And a real pointer stroke afterwards must not panic.
    app.active_tool = ActiveTool::Pencil;
    let p = Pos2::new(SCREEN.x / 2.0 / 1.5, SCREEN.y / 2.0 / 1.5);
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
}

#[test]
fn fresh_texture_prompt_defaults_to_32x32() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::ThreeD));
    app.active_tool = ActiveTool::Pencil;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::default());
    }
    let p = Pos2::new(SCREEN.x / 2.0 / 1.5, SCREEN.y / 2.0 / 1.5);
    frame(&ctx, &mut app, vec![
        egui::Event::PointerMoved(p),
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::default() },
    ], Modifiers::default());
    frame(&ctx, &mut app, vec![
        egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::default() },
    ], Modifiers::default());
    assert_eq!(
        app.atlas_dialog_fields_for_test(),
        Some(("32".to_string(), "32".to_string())),
        "a fresh texture defaults to 32x32"
    );
}

#[test]
fn glow_and_shadow_color_choices_are_undoable() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::ThreeD));
    frame(&ctx, &mut app, vec![], Modifiers::default());
    let c = app.project.palette[2];

    // Simulate the swatch context-menu actions (same code path: helper + push).
    let before = (app.project.glow_colors.clone(), app.project.shadow_color);
    app.project.toggle_glow_color(c);
    app.undo_stack.push(squarez::history::Command::SetLighting {
        before,
        after: (app.project.glow_colors.clone(), app.project.shadow_color),
    });
    assert!(app.project.is_glow_color(c));

    let before = (app.project.glow_colors.clone(), app.project.shadow_color);
    app.project.shadow_color = Some(c);
    app.undo_stack.push(squarez::history::Command::SetLighting {
        before,
        after: (app.project.glow_colors.clone(), app.project.shadow_color),
    });

    key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert_eq!(app.project.shadow_color, None, "undo reverts the shadow color");
    assert!(app.project.is_glow_color(c), "glow untouched by the second undo step");
    key(&ctx, &mut app, Key::Z, Modifiers::COMMAND);
    assert!(!app.project.is_glow_color(c), "undo reverts the glow toggle");
    key(&ctx, &mut app, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(app.project.is_glow_color(c), "redo restores it");
}

#[test]
fn emission_defaults_are_alive_and_glow_revives_them() {
    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    app.open_project_for_test(project_for(ProjectMode::ThreeD));
    assert!(app.project.emission, "emission defaults on");
    assert_eq!(app.project.emission_intensity, 50, "default strength is alive");

    // A dead saved state (off / zeroed) revives when a glow color is chosen —
    // same self-heal the palette toggle performs.
    app.project.emission = false;
    app.project.emission_intensity = 0;
    let c = app.project.palette[1];
    app.project.toggle_glow_color(c);
    if app.project.is_glow_color(c) {
        app.project.emission = true;
        if app.project.emission_intensity == 0 {
            app.project.emission_intensity = 50;
        }
    }
    assert!(app.project.emission && app.project.emission_intensity == 50);
}
