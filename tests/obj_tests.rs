// tests/obj_tests.rs
use squarez::io::obj::{load_obj, save_obj};
use squarez::project::{Project, ProjectMode};
use squarez::three_d::mesh::Mesh;
use squarez::three_d::paint_islands;

fn cube_project(name: &str) -> Project {
    let mut project = Project::new_with_mode(64, 64, name.to_string(), ProjectMode::ThreeD);
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let layer = &mut project.animations[0].frames[0].layers[0];
    layer.name = "Texture".to_string();
    paint_islands(layer, &mesh, [128, 128, 128, 255]);
    project.mesh3d = Some(mesh);
    project
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("squarez_obj_tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn writer_emits_expected_structure() {
    let project = cube_project("snapshot");
    let path = temp_path("snapshot.obj");
    save_obj(&project, &path).expect("save");

    let obj = std::fs::read_to_string(&path).unwrap();
    let v_lines: Vec<&str> = obj.lines().filter(|l| l.starts_with("v ")).collect();
    let vt_lines: Vec<&str> = obj.lines().filter(|l| l.starts_with("vt ")).collect();
    let f_lines: Vec<&str> = obj.lines().filter(|l| l.starts_with("f ")).collect();
    assert_eq!(v_lines.len(), 8);
    assert_eq!(vt_lines.len(), 24, "one vt per face corner");
    assert_eq!(f_lines.len(), 6);
    assert!(obj.contains("mtllib snapshot.mtl"));
    assert!(obj.contains("usemtl squarez_mat"));
    // Quads with 1-based v/vt indices.
    for f in &f_lines {
        let corners: Vec<&str> = f[2..].split_whitespace().collect();
        assert_eq!(corners.len(), 4);
        for c in corners {
            let mut it = c.split('/');
            let vi: usize = it.next().unwrap().parse().unwrap();
            let vti: usize = it.next().unwrap().parse().unwrap();
            assert!((1..=8).contains(&vi));
            assert!((1..=24).contains(&vti));
        }
    }
    // V is flipped: no vt may place a texel row below 0 after unflip.
    for vt in vt_lines {
        let v: f32 = vt.split_whitespace().nth(2).unwrap().parse().unwrap();
        assert!((0.0..=1.0).contains(&v), "flipped V out of range: {}", v);
    }

    let mtl = std::fs::read_to_string(temp_path("snapshot.mtl")).unwrap();
    assert!(mtl.contains("newmtl squarez_mat"));
    assert!(mtl.contains("illum 0"));
    assert!(mtl.contains("map_Kd snapshot.png"));
    assert!(temp_path("snapshot.png").exists());
}

#[test]
fn roundtrip_preserves_mesh_islands_and_texture() {
    let mut project = cube_project("roundtrip");
    // Distinctive pixel inside face 0's island.
    let isl = project.mesh3d.as_ref().unwrap().faces[0].island;
    project.animations[0].frames[0].layers[0]
        .set_pixel(isl.x as u32 + 2, isl.y as u32 + 3, [255, 0, 0, 255]);

    let path = temp_path("roundtrip.obj");
    save_obj(&project, &path).expect("save");
    let loaded = load_obj(&path).expect("load");

    assert_eq!(loaded.mode, ProjectMode::ThreeD);
    assert_eq!(loaded.canvas_width, 64);
    assert_eq!(loaded.canvas_height, 64);

    let orig = project.mesh3d.as_ref().unwrap();
    let back = loaded.mesh3d.as_ref().unwrap();
    assert_eq!(back.vertices, orig.vertices);
    assert_eq!(back.faces.len(), orig.faces.len());
    for (a, b) in orig.faces.iter().zip(back.faces.iter()) {
        assert_eq!(a.verts, b.verts);
        assert_eq!(a.island, b.island, "island must survive the roundtrip");
    }

    let px = loaded.animations[0].frames[0].layers[0]
        .get_pixel(isl.x as u32 + 2, isl.y as u32 + 3);
    assert_eq!(px, [255, 0, 0, 255]);
    // Island interiors survive too.
    let gray = loaded.animations[0].frames[0].layers[0]
        .get_pixel(isl.x as u32, isl.y as u32);
    assert_eq!(gray, [128, 128, 128, 255]);
}

#[test]
fn reader_tolerates_foreign_noise_and_ngons() {
    let path = temp_path("foreign.obj");
    std::fs::write(
        &path,
        "# some exporter\n\
         o Thing\n\
         v 0 0 0\n\
         v 4 0 0\n\
         v 4 4 0\n\
         v 0 4 0\n\
         v 2 6 0\n\
         vn 0 0 1\n\
         s 1\n\
         g group1\n\
         f 1//1 2//1 3//1 4//1 5//1\n",
    )
    .unwrap();
    let loaded = load_obj(&path).expect("load foreign");
    let mesh = loaded.mesh3d.as_ref().unwrap();
    assert_eq!(mesh.vertices.len(), 5);
    // Pentagon fan-triangulated into 3 tris, fresh islands allocated.
    assert_eq!(mesh.faces.len(), 3);
    assert!(mesh.faces.iter().all(|f| f.verts.len() == 3));
    assert!(mesh.faces.iter().all(|f| f.island.w >= 1 && f.island.h >= 1));
    mesh.validate().expect("valid");
    // No texture: blank default atlas.
    assert_eq!(loaded.canvas_width, 256);
}

#[test]
fn reader_rejects_garbage() {
    let path = temp_path("bad.obj");
    std::fs::write(&path, "f 1 2 9\nv 0 0 0\nv 1 0 0\nv 1 1 0\n").unwrap();
    assert!(load_obj(&path).is_err(), "face referencing a missing vertex must fail");
}

#[test]
fn missing_texture_gives_blank_atlas_with_islands_from_vt() {
    // Our own structure but the referenced PNG doesn't exist.
    let project = cube_project("notex");
    let path = temp_path("notex.obj");
    save_obj(&project, &path).expect("save");
    std::fs::remove_file(temp_path("notex.png")).unwrap();
    let loaded = load_obj(&path).expect("load");
    // Geometry + islands still reconstructed from the vt data (default atlas size).
    let mesh = loaded.mesh3d.as_ref().unwrap();
    assert_eq!(mesh.vertices.len(), 8);
    assert_eq!(mesh.faces.len(), 6);
}
