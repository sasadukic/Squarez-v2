// tests/real_file_check.rs
//
// End-to-end validation against the user's actual model file, if present:
// after the real load + migration path the atlas must be in the projected
// layout, islands must not overlap or run off the atlas, and painted islands
// must carry no baked checker rims.
// Self-skips when the file doesn't exist (CI machines, other checkouts).

use squarez::three_d::edit::islands_need_repack;
use squarez::three_d::{migrate_layout, DEFAULT_FACE_A, DEFAULT_FACE_B};

#[test]
fn users_model_loads_seam_clean() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let path = std::path::PathBuf::from(home).join("Desktop/Untitled.obj");
    if !path.exists() {
        return; // nothing to validate on this machine
    }

    let mut project = squarez::io::obj::load_obj(&path).expect("user's OBJ should load");
    migrate_layout(&mut project);
    let atlas = (project.canvas_width, project.canvas_height);

    let mesh = project.mesh3d.as_ref().expect("3D project");
    assert!(!mesh.faces.is_empty(), "the model must have geometry, or this checks nothing");
    assert!(
        !islands_need_repack(mesh, atlas),
        "islands must sit at their projected positions after load migration"
    );
    println!("{} faces from the real model migrated cleanly", mesh.faces.len());

    // Every island inside the atlas, and no two non-coplanar faces sharing
    // texels (coplanar faces legitimately do — see three_d::layout).
    for (fi, face) in mesh.faces.iter().enumerate() {
        let i = face.island;
        assert!(i.w >= 1 && i.h >= 1, "face {fi} has a degenerate island {i:?}");
        assert!(
            (i.x + i.w) as u32 <= atlas.0 && (i.y + i.h) as u32 <= atlas.1,
            "face {fi} island {i:?} runs past the atlas"
        );
    }

    // No painted island may keep an all-checker outer rim after healing.
    let layer = &project.animations[0].frames[0].layers[0];
    let is_default = |c: [u8; 4]| c == DEFAULT_FACE_A || c == DEFAULT_FACE_B;
    for (fi, face) in mesh.faces.iter().enumerate() {
        let isl = face.island;
        if isl.w < 3 || isl.h < 3 {
            continue;
        }
        let (x0, y0) = (isl.x as u32, isl.y as u32);
        let (x1, y1) = ((isl.x + isl.w - 1) as u32, (isl.y + isl.h - 1) as u32);
        let mut rim_all_default = true;
        let mut inner_has_paint = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = layer.get_pixel(x, y);
                let on_rim = x == x0 || x == x1 || y == y0 || y == y1;
                if on_rim {
                    if !is_default(c) {
                        rim_all_default = false;
                    }
                } else if !is_default(c) {
                    inner_has_paint = true;
                }
            }
        }
        assert!(
            !(rim_all_default && inner_has_paint),
            "face {fi}: painted island still carries a baked checker rim"
        );
    }
}

/// Every 3D `.sqr` on the user's Desktop, through the real load + migration
/// path. Self-skips where there are none.
///
/// Synthetic meshes are all primitives; these are hand-modelled, so they are
/// the only place irregular geometry meets the layout.
#[test]
fn real_sqr_models_migrate_cleanly() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let desktop = std::path::PathBuf::from(home).join("Desktop");
    let Ok(entries) = std::fs::read_dir(&desktop) else { return };

    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sqr") {
            continue;
        }
        let Ok(mut project) = squarez::io::sqr::load_sqr(&path) else { continue };
        if !project.mode.is_three_d() || project.mesh3d.is_none() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let before = project.mesh3d.clone().unwrap();

        migrate_layout(&mut project);
        let atlas = (project.canvas_width, project.canvas_height);
        let mesh = project.mesh3d.as_ref().unwrap();

        assert_eq!(mesh.faces.len(), before.faces.len(), "{name}: face count changed");
        assert_eq!(mesh.vertices, before.vertices, "{name}: geometry changed");
        assert!(!islands_need_repack(mesh, atlas), "{name}: not canonical after migration");
        for (fi, face) in mesh.faces.iter().enumerate() {
            let i = face.island;
            assert!(i.w >= 1 && i.h >= 1, "{name}: face {fi} degenerate island {i:?}");
            assert!(
                (i.x + i.w) as u32 <= atlas.0 && (i.y + i.h) as u32 <= atlas.1,
                "{name}: face {fi} island {i:?} runs past the {atlas:?} atlas"
            );
        }

        let faces = mesh.faces.len();
        // Idempotent: a migrated project must not migrate again.
        assert!(!migrate_layout(&mut project), "{name}: migration is not a fixed point");
        checked += 1;
        println!("{name}: {faces} faces migrated cleanly");
    }
    println!("checked {checked} real 3D .sqr file(s)");
}

/// Every .sqr on the user's Desktop must at least LOAD. Three of them were
/// written by intermediate builds whose Project tail predates the current
/// layout; they were unloadable ("unexpected end of file") until the
/// tail-progression mirrors landed in io/sqr.rs. Self-skips elsewhere.
#[test]
fn every_desktop_sqr_loads() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let desktop = std::path::PathBuf::from(home).join("Desktop");
    let Ok(entries) = std::fs::read_dir(&desktop) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sqr") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let p = squarez::io::sqr::load_sqr(&path)
            .unwrap_or_else(|e| panic!("{name} failed to load: {e}"));
        assert!(!p.animations.is_empty(), "{name}: loaded empty");
        for (ai, anim) in p.animations.iter().enumerate() {
            for (fi, frame) in anim.frames.iter().enumerate() {
                for (li, layer) in frame.layers.iter().enumerate() {
                    assert!(
                        layer.is_group
                            || layer.pixels.len()
                                == (layer.width * layer.height * 4) as usize,
                        "{name}: anim {ai} frame {fi} layer {li} has a corrupt pixel buffer"
                    );
                }
            }
        }
    }
}
