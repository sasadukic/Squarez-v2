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
