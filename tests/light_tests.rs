// tests/light_tests.rs — baked shadows + ambient occlusion.

use squarez::three_d::edit;
use squarez::three_d::light::{bake_lightmap, lightmap_key};
use squarez::three_d::mesh::Mesh;
use squarez::project::Layer;

const ATLAS: (u32, u32) = (128, 128);

/// A wide ground plane with a cube floating above its middle.
fn cube_over_plane() -> Mesh {
    let mut plane = Mesh::plane(16);
    plane.allocate_all_islands(ATLAS).unwrap();
    let layer = Layer::new("t".to_string(), ATLAS.0, ATLAS.1);
    let mut cube = Mesh::cube(4);
    for v in &mut cube.vertices {
        v[1] += 3.0; // float the cube 3 units up
    }
    let out = edit::add_object(&plane, &layer, &cube, ATLAS).unwrap();
    out.mesh
}

fn top_face_of_plane(mesh: &Mesh) -> u32 {
    // The plane is face 0 (single-face mesh came first).
    assert!(mesh.face_normal(&mesh.faces[0])[1] > 0.0);
    0
}

#[test]
fn shadows_darken_under_the_cube_and_not_far_away() {
    let mesh = cube_over_plane();
    let plane = top_face_of_plane(&mesh);
    let isl = mesh.faces[plane as usize].island;
    let lit = bake_lightmap(&mesh, ATLAS, false, false, false);
    let shadowed = bake_lightmap(&mesh, ATLAS, true, false, false);

    // Somewhere on the plane must be darker with shadows on…
    let mut darker = 0;
    let mut changed_far_corner = false;
    for j in 0..isl.h as u32 {
        for i in 0..isl.w as u32 {
            let idx = ((isl.y as u32 + j) * ATLAS.0 + isl.x as u32 + i) as usize;
            if shadowed[idx].shadow < lit[idx].shadow {
                darker += 1;
                if (i <= 1 || i >= isl.w as u32 - 2) && (j <= 1 || j >= isl.h as u32 - 2) {
                    changed_far_corner = true;
                }
            }
        }
    }
    assert!(darker > 4, "the cube must cast a shadow onto the plane ({darker} texels)");
    assert!(
        darker < (isl.w as usize * isl.h as usize) / 2,
        "the shadow must not swallow the whole plane"
    );
    assert!(!changed_far_corner, "far corners stay out of the shadow");
}

#[test]
fn ao_darkens_contact_more_than_open_ground() {
    // Cube RESTING on the plane: AO darkens the ring around the contact.
    let mut plane = Mesh::plane(16);
    plane.allocate_all_islands(ATLAS).unwrap();
    let layer = Layer::new("t".to_string(), ATLAS.0, ATLAS.1);
    let out = edit::add_object(&plane, &layer, &Mesh::cube(4), ATLAS).unwrap();
    let mut mesh = out.mesh;
    // add_object lifts the cube above the scene; drop it onto the plane.
    let plane_top = 0.0f32;
    let cube_min = mesh
        .vertices
        .iter()
        .skip(4)
        .map(|v| v[1])
        .fold(f32::MAX, f32::min);
    let dy = cube_min - plane_top;
    for v in mesh.vertices.iter_mut().skip(4) {
        v[1] -= dy;
    }
    let isl = mesh.faces[0].island;
    let no_ao = bake_lightmap(&mesh, ATLAS, false, false, false);
    let ao = bake_lightmap(&mesh, ATLAS, false, false, true);
    let mut near = Vec::new();
    let mut far = Vec::new();
    for j in 0..isl.h as u32 {
        for i in 0..isl.w as u32 {
            let idx = ((isl.y as u32 + j) * ATLAS.0 + isl.x as u32 + i) as usize;
            let drop = no_ao[idx].ao as i32 - ao[idx].ao as i32;
            let (cx, cy) = (isl.w as f32 / 2.0, isl.h as f32 / 2.0);
            let d = ((i as f32 - cx).powi(2) + (j as f32 - cy).powi(2)).sqrt();
            if d < 4.0 {
                near.push(drop);
            } else if d > 6.5 {
                far.push(drop);
            }
        }
    }
    let avg = |v: &[i32]| v.iter().sum::<i32>() as f32 / v.len().max(1) as f32;
    assert!(
        avg(&near) > avg(&far) + 2.0,
        "AO must darken near the cube more than open ground (near {} vs far {})",
        avg(&near),
        avg(&far)
    );
}

#[test]
fn lightmap_cache_key_tracks_geometry_not_paint() {
    let mesh = cube_over_plane();
    let k1 = lightmap_key(&mesh, ATLAS, true, false, true);
    assert_eq!(k1, lightmap_key(&mesh, ATLAS, true, false, true), "stable");
    let mut moved = mesh.clone();
    moved.vertices[0][0] += 1.0;
    assert_ne!(k1, lightmap_key(&moved, ATLAS, true, false, true), "geometry changes the key");
    assert_ne!(
        k1,
        lightmap_key(&mesh, ATLAS, false, false, true),
        "toggle changes the key"
    );
}

#[test]
fn soft_shadows_produce_fractional_penumbra() {
    let mesh = cube_over_plane();
    let hard = bake_lightmap(&mesh, ATLAS, true, false, false);
    let soft = bake_lightmap(&mesh, ATLAS, true, true, false);
    let frac = |m: &[squarez::three_d::light::LightTexel]| {
        m.iter().filter(|t| t.shadow > 0 && t.shadow < 255).count()
    };
    assert_eq!(frac(&hard), 0, "hard shadows are binary");
    assert!(frac(&soft) > 3, "soft shadows must have penumbra texels ({})", frac(&soft));
}
