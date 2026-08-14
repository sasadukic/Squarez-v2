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
