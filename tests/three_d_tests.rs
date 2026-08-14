// tests/three_d_tests.rs
use squarez::three_d::mesh::{Island, Mesh, GUTTER};

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

#[test]
fn cube_topology() {
    let mesh = Mesh::cube(8);
    assert_eq!(mesh.vertices.len(), 8);
    assert_eq!(mesh.faces.len(), 6);
    assert!(mesh.faces.iter().all(|f| f.verts.len() == 4));
    assert_eq!(mesh.derive_edges().len(), 12);
    mesh.validate().expect("cube is valid");
}

#[test]
fn cube_normals_point_outward() {
    let size = 8.0f32;
    let mesh = Mesh::cube(8);
    let center = [0.0, size / 2.0, 0.0];
    for face in &mesh.faces {
        let n = normalize(mesh.face_normal(face));
        // centroid of the face
        let mut c = [0.0f32; 3];
        for &vi in &face.verts {
            let v = mesh.vertices[vi as usize];
            c = [c[0] + v[0], c[1] + v[1], c[2] + v[2]];
        }
        let k = face.verts.len() as f32;
        c = [c[0] / k, c[1] / k, c[2] / k];
        let out = [c[0] - center[0], c[1] - center[1], c[2] - center[2]];
        let dot = n[0] * out[0] + n[1] * out[1] + n[2] * out[2];
        assert!(dot > 0.0, "face normal {:?} points inward (dot {})", n, dot);
    }
}

#[test]
fn cube_island_sizes_match_face_extent() {
    let mesh = Mesh::cube(8);
    for face in &mesh.faces {
        let (_, _, w, h) = mesh.face_uv_bounds(face);
        assert_eq!((w, h), (8, 8));
    }
}

#[test]
fn plane_has_one_upward_quad() {
    let mesh = Mesh::plane(8);
    assert_eq!(mesh.faces.len(), 1);
    let n = normalize(mesh.face_normal(&mesh.faces[0]));
    assert!(n[1] > 0.99, "plane normal should be +Y, got {:?}", n);
    let (_, _, w, h) = mesh.face_uv_bounds(&mesh.faces[0]);
    assert_eq!((w, h), (8, 8));
}

#[test]
fn allocator_islands_do_not_overlap_and_keep_gutter() {
    let mut mesh = Mesh::default();
    let atlas = (64u32, 64u32);
    let sizes = [(8u16, 8u16), (8, 8), (8, 8), (3, 5), (16, 2), (8, 8), (30, 8), (8, 8)];
    let mut islands: Vec<Island> = Vec::new();
    for &(w, h) in &sizes {
        let isl = mesh.alloc_island(w, h, atlas).expect("fits");
        assert_eq!((isl.w, isl.h), (w, h));
        assert!(isl.x >= GUTTER && isl.y >= GUTTER);
        assert!(isl.x + isl.w + GUTTER <= atlas.0 as u16);
        assert!(isl.y + isl.h + GUTTER <= atlas.1 as u16);
        islands.push(isl);
    }
    // pairwise: expanded by the gutter, islands must stay disjoint
    for i in 0..islands.len() {
        for j in (i + 1)..islands.len() {
            let a = islands[i];
            let b = islands[j];
            let disjoint = a.x + a.w + GUTTER <= b.x
                || b.x + b.w + GUTTER <= a.x
                || a.y + a.h + GUTTER <= b.y
                || b.y + b.h + GUTTER <= a.y;
            assert!(disjoint, "islands {:?} and {:?} touch or overlap", a, b);
        }
    }
}

#[test]
fn allocator_reports_full_atlas() {
    let mut mesh = Mesh::default();
    let atlas = (16u32, 16u32);
    // island wider than the atlas can never fit
    assert!(mesh.alloc_island(32, 4, atlas).is_err());
    // fill the atlas vertically until it errors
    let mut count = 0;
    while mesh.alloc_island(14, 6, atlas).is_ok() {
        count += 1;
        assert!(count < 100, "allocator never reported full");
    }
    assert!(count >= 1);
}

#[test]
fn allocate_all_islands_covers_every_face() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).expect("fits");
    assert!(mesh.faces.iter().all(|f| f.island.w == 8 && f.island.h == 8));
    // second call is a no-op (islands already allocated)
    let before = mesh.clone();
    mesh.allocate_all_islands((64, 64)).expect("no-op");
    assert_eq!(mesh, before);
}

// ── Camera + scene tests ─────────────────────────────────────────────────────

use egui::{Pos2, Rect, Vec2};
use squarez::three_d::camera::{Camera3D, SnapView};
use squarez::three_d::render::build_scene;

fn cam_at(view: SnapView, zoom: f32) -> Camera3D {
    let mut cam = Camera3D { yaw: 0.0, pitch: 0.0, zoom, offset: Vec2::ZERO };
    cam.snap_to(view);
    cam
}

fn rect100() -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0))
}

#[test]
fn front_view_projects_axis_aligned() {
    let cam = cam_at(SnapView::Front, 4.0);
    let rect = rect100();
    let (p, depth) = cam.project([1.0, 2.0, 3.0], rect);
    let c = rect.center();
    assert!((p.x - (c.x + 4.0)).abs() < 1e-4, "x: {}", p.x);
    assert!((p.y - (c.y - 8.0)).abs() < 1e-4, "y: {}", p.y);
    assert!((depth - 3.0).abs() < 1e-4);
}

#[test]
fn right_view_faces_positive_x() {
    let cam = cam_at(SnapView::Right, 1.0);
    // +X should point toward the camera (positive depth)
    let v = cam.view([1.0, 0.0, 0.0]);
    assert!(v[2] > 0.99, "depth of +X in right view: {:?}", v);
    // -Z should appear screen-right
    let v2 = cam.view([0.0, 0.0, -1.0]);
    assert!(v2[0] > 0.99, "screen-x of -Z in right view: {:?}", v2);
}

#[test]
fn top_view_looks_down() {
    let cam = cam_at(SnapView::Top, 1.0);
    let v = cam.view([0.0, 1.0, 0.0]);
    assert!(v[2] > 0.99, "+Y toward camera in top view: {:?}", v);
    // +Z appears down-screen (negative view y)
    let v2 = cam.view([0.0, 0.0, 1.0]);
    assert!(v2[1] < -0.99, "+Z down-screen in top view: {:?}", v2);
}

#[test]
fn snap_quantizes_zoom_and_offset() {
    let mut cam = Camera3D { yaw: 0.3, pitch: 0.2, zoom: 11.7, offset: Vec2::new(3.4, -2.6) };
    cam.snap_to(SnapView::Front);
    assert_eq!(cam.zoom, 12.0);
    assert_eq!(cam.offset, Vec2::new(3.0, -3.0));
    assert_eq!(cam.snapped(), Some(SnapView::Front));
}

#[test]
fn zoom_at_keeps_cursor_point_fixed() {
    let mut cam = cam_at(SnapView::Front, 8.0);
    let rect = rect100();
    let world = [2.0, 1.0, 0.0];
    let (before, _) = cam.project(world, rect);
    cam.zoom_at(1.5, before, rect);
    let (after, _) = cam.project(world, rect);
    assert!((before.x - after.x).abs() < 1e-3 && (before.y - after.y).abs() < 1e-3,
        "point moved: {:?} -> {:?}", before, after);
}

#[test]
fn front_view_scene_shows_only_front_face() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let cam = cam_at(SnapView::Front, 4.0);
    let scene = build_scene(&mesh, &cam, rect100(), (64, 64));
    // Only the +Z face survives: side faces are edge-on (culled), back faces away.
    assert_eq!(scene.visible_faces.len(), 1);
    assert_eq!(scene.tris.len(), 2);
    let fi = scene.visible_faces[0] as usize;
    let n = mesh.face_normal(&mesh.faces[fi]);
    assert!(n[2] > 0.0, "visible face should be +Z, normal {:?}", n);
}

#[test]
fn orbit_view_sorts_far_to_near() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let cam = Camera3D::default(); // 3/4 orbit: three faces visible
    let scene = build_scene(&mesh, &cam, rect100(), (64, 64));
    assert!(scene.visible_faces.len() >= 2);
    for pair in scene.tris.windows(2) {
        assert!(pair[0].depth <= pair[1].depth, "tris not sorted far-to-near");
    }
}

#[test]
fn scene_uvs_stay_inside_island() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let cam = Camera3D::default();
    let scene = build_scene(&mesh, &cam, rect100(), (64, 64));
    for tri in &scene.tris {
        let isl = mesh.faces[tri.face as usize].island;
        let (x0, y0) = (isl.x as f32 / 64.0, isl.y as f32 / 64.0);
        let (x1, y1) = ((isl.x + isl.w) as f32 / 64.0, (isl.y + isl.h) as f32 / 64.0);
        for uv in tri.uvs {
            assert!(uv.x >= x0 - 1e-4 && uv.x <= x1 + 1e-4, "uv.x {} outside [{}, {}]", uv.x, x0, x1);
            assert!(uv.y >= y0 - 1e-4 && uv.y <= y1 + 1e-4, "uv.y {} outside [{}, {}]", uv.y, y0, y1);
        }
    }
}

// ── Painting tests ───────────────────────────────────────────────────────────

use squarez::project::Layer;
use squarez::three_d::paint::{fill_island, pick};

/// In a snap view at integer zoom, picking must be texel-exact: the screen
/// pixel block covering texel (i, j) maps back to exactly that texel.
#[test]
fn pick_is_pixel_perfect_in_snap_view() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let zoom = 4.0;
    let cam = cam_at(SnapView::Front, zoom);
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0));
    let scene = build_scene(&mesh, &cam, rect, (64, 64));
    assert_eq!(scene.visible_faces.len(), 1);
    let fi = scene.visible_faces[0] as usize;
    let isl = mesh.faces[fi].island;
    let c = rect.center();

    for i in 0..8u32 {
        for j in 0..8u32 {
            // World point at the center of texel (i, j): the front face spans
            // x in [-4, 4], y in [0, 8]; u = x - min_u, v = y - min_v.
            let wx = -4.0 + i as f32 + 0.5;
            let wy = j as f32 + 0.5;
            let p = Pos2::new(c.x + wx * zoom, c.y - wy * zoom);
            let hit = pick(&scene, p, &mesh, (64, 64)).expect("hit expected");
            assert_eq!(hit.face as usize, fi);
            assert_eq!(
                hit.texel,
                ((isl.x as u32 + i) as i64, (isl.y as u32 + j) as i64),
                "texel mismatch at ({}, {})", i, j
            );
        }
    }
}

#[test]
fn pick_misses_outside_model() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let cam = cam_at(SnapView::Front, 4.0);
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0));
    let scene = build_scene(&mesh, &cam, rect, (64, 64));
    assert!(pick(&scene, Pos2::new(2.0, 2.0), &mesh, (64, 64)).is_none());
}

#[test]
fn fill_stays_inside_island() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 64, 64);
    // Uniform transparent atlas; fill face 0's island with red.
    let isl = mesh.faces[0].island;
    let edits = fill_island(&mut layer, isl, 0, 0, [255, 0, 0, 255]);
    assert_eq!(edits.len(), 64, "expected the whole 8x8 island filled");
    for &(x, y, _, new) in &edits {
        assert!(x >= isl.x as u32 && x < (isl.x + isl.w) as u32, "x {} escaped island", x);
        assert!(y >= isl.y as u32 && y < (isl.y + isl.h) as u32, "y {} escaped island", y);
        assert_eq!(new, [255, 0, 0, 255]);
    }
    // A second fill with the same color is a no-op.
    for &(x, y, _, new) in &edits {
        let _ = (x, y, new);
        layer.set_pixel(x, y, new);
    }
    let again = fill_island(&mut layer, isl, 0, 0, [255, 0, 0, 255]);
    assert!(again.is_empty());
}

// ── Modeling op tests ────────────────────────────────────────────────────────

use squarez::history::{apply_command, Command, Direction};
use squarez::project::{Project, ProjectMode};
use squarez::three_d::edit::{
    add_primitive, delete_faces, delete_vertices, extrude_faces, move_vertices, Primitive,
    NEW_FACE_COLOR,
};

#[test]
fn extrude_top_face_adds_cap_and_sides() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let layer = Layer::new("Texture".to_string(), 128, 128);
    // face 1 is the +Y top in Mesh::cube
    let top = 1u32;
    let old_island = mesh.faces[top as usize].island;
    let out = extrude_faces(&mesh, &layer, &[top], (128, 128)).expect("fits");

    assert_eq!(out.mesh.vertices.len(), 12);
    assert_eq!(out.mesh.faces.len(), 10);
    assert_eq!(out.select_faces, vec![top]);
    // Cap reuses the original island; sides got fresh gray-filled islands.
    assert_eq!(out.mesh.faces[top as usize].island, old_island);
    assert!(out.pixel_edits.iter().all(|&(_, _, _, new)| new == NEW_FACE_COLOR));
    assert!(!out.pixel_edits.is_empty());
    out.mesh.validate().expect("valid after extrude");

    // Cap sits 1 unit above the original top (y = 9), normals still outward.
    let cap = &out.mesh.faces[top as usize];
    for &vi in &cap.verts {
        assert_eq!(out.mesh.vertices[vi as usize][1], 9.0);
    }
    let n = out.mesh.face_normal(cap);
    assert!(n[1] > 0.0, "cap normal should stay +Y");
}

#[test]
fn move_vertex_resizes_island_with_blit() {
    let mut mesh = Mesh::plane(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 64, 64);
    // Give the old island recognizable content: column stripes by x.
    let old = mesh.faces[0].island;
    for j in 0..old.h as u32 {
        for i in 0..old.w as u32 {
            layer.set_pixel(old.x as u32 + i, old.y as u32 + j, [i as u8, j as u8, 0, 255]);
        }
    }
    // Stretch the plane 1 unit in -X: footprint becomes 9x8.
    let out = move_vertices(&mesh, &layer, &[0, 1], [-1, 0, 0], (64, 64)).expect("fits");
    let new = out.mesh.faces[0].island;
    assert_eq!((new.w, new.h), (9, 8));
    assert_ne!(new, old, "island must move to a fresh slot");
    // Blit: every dest texel sampled from src via nearest-neighbor.
    for &(x, y, _, c) in &out.pixel_edits {
        assert!(x >= new.x as u32 && x < (new.x + new.w) as u32);
        assert!(y >= new.y as u32 && y < (new.y + new.h) as u32);
        let i = x - new.x as u32;
        let j = y - new.y as u32;
        let si = (i * old.w as u32) / new.w as u32;
        let sj = (j * old.h as u32) / new.h as u32;
        assert_eq!(c, [si as u8, sj as u8, 0, 255], "blit mismatch at dest ({}, {})", i, j);
    }
    // Geometry actually moved.
    assert_eq!(out.mesh.vertices[0][0], mesh.vertices[0][0] - 1.0);
}

#[test]
fn delete_face_keeps_shared_vertices() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let out = delete_faces(&mesh, &[0]);
    assert_eq!(out.mesh.faces.len(), 5);
    assert_eq!(out.mesh.vertices.len(), 8, "all cube verts still used by other faces");
    out.mesh.validate().expect("valid after face delete");
}

#[test]
fn delete_vertex_removes_its_faces_and_orphans() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let out = delete_vertices(&mesh, &[0]);
    // v0 belongs to bottom, back, left — three faces removed.
    assert_eq!(out.mesh.faces.len(), 3);
    assert_eq!(out.mesh.vertices.len(), 7, "v0 gone, everything else still referenced");
    out.mesh.validate().expect("valid after vertex delete");
}

#[test]
fn add_primitive_stacks_above_existing() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let layer = Layer::new("Texture".to_string(), 128, 128);
    let out = add_primitive(&mesh, &layer, Primitive::Cube, (128, 128)).expect("fits");
    assert_eq!(out.mesh.vertices.len(), 16);
    assert_eq!(out.mesh.faces.len(), 12);
    assert_eq!(out.select_faces.len(), 6);
    // New cube's lowest point sits above the old cube's top (8 + 2).
    let min_y = out.mesh.vertices[8..].iter().map(|v| v[1]).fold(f32::MAX, f32::min);
    assert_eq!(min_y, 10.0);
    out.mesh.validate().expect("valid after add");
}

#[test]
fn mesh_edit_command_replays_both_directions() {
    let mut project = Project::new_with_mode(64, 64, "m".to_string(), ProjectMode::ThreeD);
    let mut before = Mesh::cube(8);
    before.allocate_all_islands((64, 64)).unwrap();
    project.mesh3d = Some(before.clone());
    project.animations[0].frames[0].layers[0].set_pixel(3, 3, [9, 9, 9, 255]);

    let layer = project.animations[0].frames[0].layers[0].clone();
    let out = extrude_faces(&before, &layer, &[1], (64, 64)).expect("fits");
    let cmd = Command::MeshEdit {
        before: before.clone(),
        after: out.mesh.clone(),
        layer_id: 0,
        pixel_edits: out.pixel_edits.clone(),
    };

    apply_command(&mut project, None, &cmd, Direction::Forward);
    assert_eq!(project.mesh3d.as_ref(), Some(&out.mesh));
    if let Some(&(x, y, _, new)) = out.pixel_edits.first() {
        assert_eq!(project.animations[0].frames[0].layers[0].get_pixel(x, y), new);
    }

    apply_command(&mut project, None, &cmd, Direction::Backward);
    assert_eq!(project.mesh3d.as_ref(), Some(&before));
    if let Some(&(x, y, old, _)) = out.pixel_edits.first() {
        assert_eq!(project.animations[0].frames[0].layers[0].get_pixel(x, y), old);
    }
    // Untouched pixel survives the roundtrip.
    assert_eq!(project.animations[0].frames[0].layers[0].get_pixel(3, 3), [9, 9, 9, 255]);
}
