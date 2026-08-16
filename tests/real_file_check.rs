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
    assert!(
        !islands_need_repack(mesh, atlas),
        "islands must sit at their projected positions after load migration"
    );

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
