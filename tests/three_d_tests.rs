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
    while mesh.alloc_island(12, 6, atlas).is_ok() {
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
    // Only the +Z face is front-facing; the -Z face renders as a dimmed
    // interior surface, side faces are edge-on (degenerate, skipped).
    assert_eq!(scene.visible_faces.len(), 1);
    assert_eq!(scene.tris.len(), 4);
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
        // Interior pass first, then fronts; far-to-near within each pass.
        assert!(!pair[0].front | pair[1].front, "front tri drawn before an interior tri");
        if pair[0].front == pair[1].front {
            assert!(pair[0].depth <= pair[1].depth, "tris not sorted far-to-near");
        }
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
fn fill_covers_the_whole_island() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 64, 64);
    // Checkerboard content must not stop the fill: it paints every texel.
    squarez::three_d::paint_islands_checker(&mut layer, &mesh);
    let isl = mesh.faces[0].island;
    let edits = fill_island(&mut layer, isl, [255, 0, 0, 255]);
    assert_eq!(edits.len(), 64, "expected the whole 8x8 island filled");
    for &(x, y, _, new) in &edits {
        assert!(x >= isl.x as u32 && x < (isl.x + isl.w) as u32, "x {} escaped island", x);
        assert!(y >= isl.y as u32 && y < (isl.y + isl.h) as u32, "y {} escaped island", y);
        assert_eq!(new, [255, 0, 0, 255]);
    }
    // A second fill with the same color is a no-op.
    for &(x, y, _, new) in &edits {
        layer.set_pixel(x, y, new);
    }
    let again = fill_island(&mut layer, isl, [255, 0, 0, 255]);
    assert!(again.is_empty());
}

// ── Modeling op tests ────────────────────────────────────────────────────────

use squarez::history::{apply_command, Command, Direction};
use squarez::project::{Project, ProjectMode};
use squarez::three_d::edit::{
    add_primitive, delete_faces, delete_vertices, extrude_faces, move_vertices, Primitive,
    DEFAULT_FACE_A, DEFAULT_FACE_B,
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
    assert!(out.pixel_edits.iter().all(|&(_, _, _, new)| new == DEFAULT_FACE_A || new == DEFAULT_FACE_B));
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
    // Anchored 1:1 copy: existing texels are never resampled; newly
    // exposed texels extend the nearest edge colors (clamp-to-edge) so a
    // painted face grows without a seam strip.
    for &(x, y, _, c) in &out.pixel_edits {
        assert!(x >= new.x as u32 && x < (new.x + new.w) as u32);
        assert!(y >= new.y as u32 && y < (new.y + new.h) as u32);
        let i = (x - new.x as u32).min(old.w as u32 - 1);
        let j = (y - new.y as u32).min(old.h as u32 - 1);
        assert_eq!(c, [i as u8, j as u8, 0, 255], "clamped copy mismatch at ({}, {})", i, j);
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

#[test]
fn orbit_views_shade_faces_snap_views_do_not() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    // Orbit: multiple visible faces with distinct shades in a sane range.
    let orbit = Camera3D::default();
    let scene = build_scene(&mesh, &orbit, rect100(), (64, 64));
    let front: std::collections::HashSet<u32> = scene.visible_faces.iter().copied().collect();
    let mut greens: Vec<f32> = scene
        .tris
        .iter()
        .filter(|t| front.contains(&t.face))
        .map(|t| t.shade[1])
        .collect();
    greens.sort_by(f32::total_cmp);
    greens.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    assert!(greens.len() >= 2, "orbit view should shade faces differently: {:?}", greens);
    for tri in &scene.tris {
        let range = if front.contains(&tri.face) { 0.4..=1.0 } else { 0.2..=0.5 };
        assert!(tri.shade.iter().all(|c| range.contains(c)), "shade {:?}", tri.shade);
    }
    // Snap view: front faces unlit so texel colors read true while
    // painting; interior (backface) surfaces render dimmed.
    let snap = cam_at(SnapView::Front, 4.0);
    let scene = build_scene(&mesh, &snap, rect100(), (64, 64));
    let front: std::collections::HashSet<u32> = scene.visible_faces.iter().copied().collect();
    for t in &scene.tris {
        if front.contains(&t.face) {
            assert_eq!(t.shade, [1.0, 1.0, 1.0]);
        } else {
            assert_eq!(t.shade, [0.5, 0.5, 0.5]);
        }
    }
}

// ── New modeling op tests: extrude_n, inset, loop cut, create face ───────────

use squarez::three_d::edit::{create_face, extrude_faces_n, inset_faces, loop_cut, plan_loop};

#[test]
fn extrude_n_units_moves_cap_and_sizes_sides() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let layer = Layer::new("Texture".to_string(), 128, 128);
    let out = extrude_faces_n(&mesh, &layer, &[1], 3, (128, 128)).expect("fits");
    let cap = &out.mesh.faces[1];
    for &vi in &cap.verts {
        assert_eq!(out.mesh.vertices[vi as usize][1], 11.0, "cap should sit at y = 8 + 3");
    }
    // Side islands span edge_len x n.
    for side in &out.mesh.faces[6..] {
        assert_eq!((side.island.w, side.island.h).min((side.island.h, side.island.w)), (3, 8).min((8, 3)));
    }
    out.mesh.validate().expect("valid");
}

#[test]
fn inset_shrinks_face_and_preserves_painted_center() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 128, 128);
    // Paint a marker 2 texels in from the old island's corner: it lies in the
    // kept center region for d=2 and must survive at the new island's origin.
    let old = mesh.faces[1].island;
    layer.set_pixel(old.x as u32 + 2, old.y as u32 + 2, [200, 10, 10, 255]);

    let out = inset_faces(&mesh, &layer, &[1], 2, (128, 128)).expect("fits");
    assert_eq!(out.mesh.vertices.len(), 12);
    assert_eq!(out.mesh.faces.len(), 10);
    assert_eq!(out.select_faces, vec![1]);
    let center = &out.mesh.faces[1];
    assert_eq!((center.island.w, center.island.h), (4, 4));
    // Center ring vertices sit 2 units inside the old 8x8 top face.
    for &vi in &center.verts {
        let v = out.mesh.vertices[vi as usize];
        assert_eq!(v[1], 8.0);
        assert!(v[0].abs() == 2.0 && v[2].abs() == 2.0, "ring vert {:?}", v);
    }
    // The painted marker got blitted to the center island's corner.
    let dst = center.island;
    let blit = out
        .pixel_edits
        .iter()
        .find(|&&(x, y, _, _)| x == dst.x as u32 && y == dst.y as u32)
        .expect("corner blit exists");
    assert_eq!(blit.3, [200, 10, 10, 255]);
    out.mesh.validate().expect("valid");
}

#[test]
fn loop_cut_rings_around_the_cube() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((256, 256)).unwrap();
    let layer = Layer::new("Texture".to_string(), 256, 256);
    // Front face is index 2 ([3,2,6,7]); entry edge at pos 1 = (2,6), the
    // vertical edge at x=+4. Cut at s=0.25 → y=2.
    let plan = plan_loop(&mesh, 2, 1, 0.25).expect("plan exists");
    assert_eq!(plan.steps.len(), 4, "loop should ring through the 4 side faces");

    let out = loop_cut(&mesh, &layer, &plan, (256, 256)).expect("fits");
    assert_eq!(out.mesh.vertices.len(), 12, "4 shared cut vertices");
    assert_eq!(out.mesh.faces.len(), 10, "4 quads become 8");
    // All cut vertices at y = 2.
    for v in &out.mesh.vertices[8..] {
        assert!((v[1] - 2.0).abs() < 1e-4, "cut vertex {:?}", v);
    }
    // Every split half island crops to 8x2 or 8x6.
    for st in &plan.steps {
        let half = &out.mesh.faces[st.face as usize];
        let dims = (half.island.w.min(half.island.h), half.island.w.max(half.island.h));
        assert!(dims == (2, 8) || dims == (6, 8), "half island {:?}", half.island);
    }
    out.mesh.validate().expect("valid");
}

#[test]
fn create_face_fills_and_orients_outward() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let layer = Layer::new("Texture".to_string(), 128, 128);
    // Remove the top face, then recreate it from its ring — deliberately in
    // the "wrong" (inward) order to exercise the auto-orient.
    let out = delete_faces(&mesh, &[1]);
    assert_eq!(out.mesh.faces.len(), 5);
    let refill = create_face(&out.mesh, &layer, &[5, 6, 7, 4], (128, 128)).expect("fits");
    assert_eq!(refill.mesh.faces.len(), 6);
    let new_face = refill.mesh.faces.last().unwrap();
    let n = refill.mesh.face_normal(new_face);
    assert!(n[1] > 0.0, "recreated top face must point up, got {:?}", n);
    assert_eq!((new_face.island.w, new_face.island.h), (8, 8));
    assert!(!refill.pixel_edits.is_empty(), "fresh island painted");
    refill.mesh.validate().expect("valid");
}

#[test]
fn create_face_rejects_degenerate_input() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let layer = Layer::new("Texture".to_string(), 128, 128);
    // Duplicate vertices → unchanged mesh.
    let out = create_face(&mesh, &layer, &[0, 0, 1], (128, 128)).expect("ok");
    assert_eq!(out.mesh.faces.len(), 6);
    assert!(out.pixel_edits.is_empty());
}

#[test]
fn create_face_sorts_any_click_order() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let layer = Layer::new("Texture".to_string(), 128, 128);
    let base = delete_faces(&mesh, &[1]); // remove the top
    // Zig-zag click order (a bowtie if taken literally): 4,6 are opposite corners.
    let out = create_face(&base.mesh, &layer, &[4, 6, 5, 7], (128, 128)).expect("fits");
    assert_eq!(out.mesh.faces.len(), 6, "face must be created from zig-zag order");
    let face = out.mesh.faces.last().unwrap();
    let n = out.mesh.face_normal(face);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    assert!(len > 1.0, "ring must be non-degenerate, |n| = {}", len);
    assert!(n[1] > 0.0, "recreated top must point up");
    // A proper ring: consecutive verts always differ in exactly one axis
    // for this axis-aligned square (no diagonals).
    for i in 0..4 {
        let a = out.mesh.vertices[face.verts[i] as usize];
        let b = out.mesh.vertices[face.verts[(i + 1) % 4] as usize];
        let diffs = (0..3).filter(|&k| (a[k] - b[k]).abs() > 1e-4).count();
        assert_eq!(diffs, 1, "ring edge {:?} -> {:?} is a diagonal", a, b);
    }
    out.mesh.validate().expect("valid");
}

// ── Primitive + object op tests ──────────────────────────────────────────────

use squarez::three_d::edit::{connected_faces, scale_verts, add_primitive as add_prim, Primitive as Prim};

fn assert_outward_and_integral(mesh: &Mesh, center: [f32; 3]) {
    for v in &mesh.vertices {
        for a in 0..3 {
            assert!((v[a] - v[a].round()).abs() < 1e-4, "non-integer vert {:?}", v);
        }
    }
    for face in &mesh.faces {
        let n = mesh.face_normal(face);
        let k = face.verts.len() as f32;
        let c = face.verts.iter().fold([0.0f32; 3], |acc, &vi| {
            let v = mesh.vertices[vi as usize];
            [acc[0] + v[0] / k, acc[1] + v[1] / k, acc[2] + v[2] / k]
        });
        let out = [c[0] - center[0], c[1] - center[1], c[2] - center[2]];
        let dot = n[0] * out[0] + n[1] * out[1] + n[2] * out[2];
        assert!(dot > 0.0, "face normal {:?} at {:?} points inward", n, c);
    }
}

#[test]
fn cylinder_is_valid_integral_and_outward() {
    let mut mesh = Mesh::cylinder(8);
    mesh.validate().expect("valid");
    assert_eq!(mesh.vertices.len(), 16);
    assert_eq!(mesh.faces.len(), 8 + 12); // 8 side quads + two 6-tri fan caps
    assert_outward_and_integral(&mesh, [0.0, 4.0, 0.0]);
    mesh.allocate_all_islands((256, 256)).expect("islands fit");
}

#[test]
fn sphere_is_valid_integral_and_outward() {
    let mut mesh = Mesh::sphere(8);
    mesh.validate().expect("valid");
    assert_eq!(mesh.vertices.len(), 32);
    assert_eq!(mesh.faces.len(), 24 + 12); // 24 band quads + two 6-tri fan caps
    assert_outward_and_integral(&mesh, [0.0, 4.0, 0.0]);
    mesh.allocate_all_islands((256, 256)).expect("islands fit");
}

#[test]
fn connected_faces_separates_objects() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((256, 256)).unwrap();
    let layer = Layer::new("Texture".to_string(), 256, 256);
    let out = add_prim(&mesh, &layer, Prim::Cube, (256, 256)).expect("fits");
    let two = out.mesh;
    assert_eq!(two.faces.len(), 12);
    let first = connected_faces(&two, 0);
    let second = connected_faces(&two, 6);
    assert_eq!(first, (0..6).collect::<Vec<u32>>());
    assert_eq!(second, (6..12).collect::<Vec<u32>>());
}

#[test]
fn scale_grows_and_shrinks_on_the_grid() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((256, 256)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 256, 256);
    squarez::three_d::paint_islands_checker(&mut layer, &mesh);
    let verts: Vec<u32> = (0..8).collect();

    let grown = scale_verts(&mesh, &layer, &verts, 2, (256, 256)).expect("fits");
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &grown.mesh.vertices {
        for a in 0..3 {
            min[a] = min[a].min(v[a]);
            max[a] = max[a].max(v[a]);
            assert!((v[a] - v[a].round()).abs() < 1e-4, "off-grid vert {:?}", v);
        }
    }
    assert_eq!(max[0] - min[0], 10.0);
    assert_eq!(max[1] - min[1], 10.0);
    // Islands re-fit to the new 10x10 faces.
    assert!(grown.mesh.faces.iter().all(|f| f.island.w == 10 && f.island.h == 10));
    assert!(!grown.pixel_edits.is_empty());

    // Shrinking far below 1 clamps to a 1-unit object.
    let tiny = scale_verts(&mesh, &layer, &verts, -20, (256, 256)).expect("fits");
    let (mut tmin, mut tmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &tiny.mesh.vertices {
        for a in 0..3 {
            tmin[a] = tmin[a].min(v[a]);
            tmax[a] = tmax[a].max(v[a]);
        }
    }
    assert_eq!(tmax[1] - tmin[1], 1.0);
}


#[test]
fn parametric_primitives_respect_grid_caps() {
    use squarez::three_d::mesh::Mesh as M;
    // 12-sided cylinder at diameter 16: allowed (r=8 supports 24).
    let c = M::cylinder_n(12, 16);
    c.validate().expect("valid");
    assert_eq!(c.vertices.len(), 24);
    assert_outward_and_integral(&c, [0.0, 8.0, 0.0]);
    // Requesting an absurd side count clamps to the grid maximum.
    let tiny = M::cylinder_n(24, 4); // r=2 supports far fewer than 24
    tiny.validate().expect("valid");
    let ring = tiny.vertices.len() / 2;
    assert!(ring < 24, "sides must clamp, got {}", ring);
    // Distinct consecutive ring vertices (no rounding collapse).
    for i in 0..ring {
        let a = tiny.vertices[i];
        let b = tiny.vertices[(i + 1) % ring];
        assert!(a != b, "collapsed ring vertices at {}", i);
    }
    // Parametric sphere stays valid and integral too.
    let s = M::sphere_n(10, 16);
    s.validate().expect("valid");
    assert_outward_and_integral(&s, [0.0, 8.0, 0.0]);
}

#[test]
fn extrude_negative_pushes_a_recess_inward() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((256, 256)).unwrap();
    let layer = Layer::new("Texture".to_string(), 256, 256);
    // Push the top face 3 units down into the cube.
    let out = extrude_faces_n(&mesh, &layer, &[1], -3, (256, 256)).expect("fits");
    let cap = &out.mesh.faces[1];
    for &vi in &cap.verts {
        assert_eq!(out.mesh.vertices[vi as usize][1], 5.0, "cap should sink to y = 8 - 3");
    }
    // Cap still faces up (out of the pocket toward the viewer).
    let n = out.mesh.face_normal(cap);
    assert!(n[1] > 0.0, "recessed cap must keep +Y, got {:?}", n);
    // Pocket walls face inward, toward the cavity center axis (x=0, z=0).
    for side in &out.mesh.faces[6..] {
        let n = out.mesh.face_normal(side);
        let k = side.verts.len() as f32;
        let c = side.verts.iter().fold([0.0f32; 3], |acc, &vi| {
            let v = out.mesh.vertices[vi as usize];
            [acc[0] + v[0] / k, acc[1] + v[1] / k, acc[2] + v[2] / k]
        });
        let toward_axis = [-c[0], 0.0, -c[2]];
        let dot = n[0] * toward_axis[0] + n[2] * toward_axis[2];
        assert!(dot > 0.0, "pocket wall at {:?} should face the cavity, normal {:?}", c, n);
    }
    out.mesh.validate().expect("valid");
}

#[test]
fn old_tight_island_packing_gets_repacked() {
    use squarez::three_d::edit::{islands_need_repack, repack_islands};
    // Simulate a pre-gutter-widening file: islands packed 1 texel apart.
    let mut mesh = Mesh::cube(8);
    let mut x = 1u16;
    for f in &mut mesh.faces {
        f.island = Island { x, y: 1, w: 8, h: 8 };
        x += 9; // 1-texel gaps
    }
    assert!(islands_need_repack(&mesh));

    let mut layer = Layer::new("Texture".to_string(), 256, 256);
    // Distinct color per island so we can verify the moves.
    for (i, f) in mesh.faces.iter().enumerate() {
        for y in 0..8u32 {
            for xx in 0..8u32 {
                layer.set_pixel(f.island.x as u32 + xx, f.island.y as u32 + y, [i as u8 + 1, 0, 0, 255]);
            }
        }
    }
    let out = repack_islands(&mesh, &layer, (256, 256)).expect("fits");
    assert!(!islands_need_repack(&out.mesh), "repacked islands must respect the gutter");
    // Every face's texture moved with it.
    for &(x, y, _, _) in &out.pixel_edits {
        let _ = (x, y);
    }
    for (i, f) in out.mesh.faces.iter().enumerate() {
        let isl = f.island;
        let expected = [i as u8 + 1, 0, 0, 255];
        let edit = out
            .pixel_edits
            .iter()
            .find(|&&(x, y, _, _)| x == isl.x as u32 && y == isl.y as u32);
        if let Some(&(_, _, _, new)) = edit {
            assert_eq!(new, expected, "face {} texture moved intact", i);
        }
    }
}

// ── Seam-damage healing tests ────────────────────────────────────────────────

#[test]
fn pad_island_gutters_dilates_edges() {
    use squarez::three_d::pad_island_gutters;
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 64, 64);
    let isl = mesh.faces[0].island;
    // Distinct edge colors on the island's border.
    for i in 0..isl.w as u32 {
        layer.set_pixel(isl.x as u32 + i, isl.y as u32, [1, i as u8, 0, 255]);
        layer.set_pixel(isl.x as u32 + i, (isl.y + isl.h - 1) as u32, [2, i as u8, 0, 255]);
    }
    let mut px = layer.pixels.clone();
    pad_island_gutters(&mut px, 64, 64, &mesh);
    let at = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * 64 + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    };
    // Row above the island copies the top edge; row below copies the bottom.
    for i in 0..isl.w as u32 {
        assert_eq!(at(isl.x as u32 + i, isl.y as u32 - 1), [1, i as u8, 0, 255]);
        assert_eq!(at(isl.x as u32 + i, (isl.y + isl.h) as u32), [2, i as u8, 0, 255]);
    }
    // A pixel two rows above is untouched (still transparent).
    assert_eq!(at(isl.x as u32, isl.y as u32 - 2), [0, 0, 0, 0]);
}

#[test]
fn heal_checker_rims_removes_baked_strips() {
    use squarez::three_d::{heal_checker_rims, DEFAULT_FACE_A, DEFAULT_FACE_B};
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let mut layer = Layer::new("Texture".to_string(), 64, 64);
    let isl = mesh.faces[0].island;
    // Painted face...
    for y in 0..isl.h as u32 {
        for x in 0..isl.w as u32 {
            layer.set_pixel(isl.x as u32 + x, isl.y as u32 + y, [10, 60, 120, 255]);
        }
    }
    // ...with a baked checker rim (the historical growth damage).
    for x in 0..isl.w {
        for y in [0, isl.h - 1] {
            let c = if (x + y) % 2 == 0 { DEFAULT_FACE_A } else { DEFAULT_FACE_B };
            layer.set_pixel((isl.x + x) as u32, (isl.y + y) as u32, c);
        }
    }
    for y in 0..isl.h {
        for x in [0, isl.w - 1] {
            let c = if (x + y) % 2 == 0 { DEFAULT_FACE_A } else { DEFAULT_FACE_B };
            layer.set_pixel((isl.x + x) as u32, (isl.y + y) as u32, c);
        }
    }
    assert!(heal_checker_rims(&mut layer, &mesh));
    for y in 0..isl.h as u32 {
        for x in 0..isl.w as u32 {
            assert_eq!(
                layer.get_pixel(isl.x as u32 + x, isl.y as u32 + y),
                [10, 60, 120, 255],
                "rim texel ({}, {}) should be healed to the paint color", x, y
            );
        }
    }
    // Fully-checker (unpainted) islands stay untouched.
    let isl2 = mesh.faces[1].island;
    squarez::three_d::paint_islands_checker(&mut layer, &mesh);
    let before: Vec<u8> = layer.pixels.clone();
    let _ = heal_checker_rims(&mut layer, &mesh);
    let after_c = layer.get_pixel(isl2.x as u32, isl2.y as u32);
    let idx = ((isl2.y as u32 * 64 + isl2.x as u32) * 4) as usize;
    assert_eq!(after_c, [before[idx], before[idx + 1], before[idx + 2], before[idx + 3]]);
}

// ── Depth-aware picking (stacked geometry in snapped views) ─────────────────

#[test]
fn clicking_stacked_vertices_picks_the_nearest() {
    use squarez::three_d::workspace::{edge_under, vertex_under};
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0));
    let cam = cam_at(SnapView::Front, 8.0);

    // In Front view the back-bottom-left vert (0) and front-bottom-left vert
    // (3) land on the exact same screen pixel; 3 is nearer the camera.
    let (p0, d0) = cam.project(mesh.vertices[0], rect);
    let (p3, d3) = cam.project(mesh.vertices[3], rect);
    assert!((p0 - p3).length() < 0.01, "test premise: verts overlap on screen");
    assert!(d3 > d0, "test premise: vertex 3 is nearer the camera");

    assert_eq!(
        vertex_under(&mesh, &cam, rect, p3, false),
        Some(3),
        "clicking stacked vertices must select the one closest to the viewer"
    );
    assert_eq!(
        vertex_under(&mesh, &cam, rect, p3, true),
        Some(0),
        "alt-click must reach the vertex stacked behind"
    );

    // Same for edges: the front bottom edge (2,3) sits over the back one (0,1).
    let scene = build_scene(&mesh, &cam, rect, (128, 128));
    let mid_front = {
        let a = cam.project(mesh.vertices[2], rect).0;
        let b = cam.project(mesh.vertices[3], rect).0;
        Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
    };
    assert_eq!(
        edge_under(&mesh, &scene, &cam, rect, mid_front, false),
        Some((2, 3)),
        "clicking stacked edges must select the one closest to the viewer"
    );
    assert_eq!(
        edge_under(&mesh, &scene, &cam, rect, mid_front, true),
        Some((0, 1)),
        "alt-click must reach the edge stacked behind"
    );
}
