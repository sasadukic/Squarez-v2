// tests/depth_order_tests.rs
//
// Draw-order correctness: the painter's algorithm output must match a
// per-pixel z-buffer ground truth, rasterized by the SAME code so the only
// possible difference is triangle order.
//
// This locks the defect class where average-depth sorting let a large face
// overdraw small geometry resting on it — a slab's top face has its average
// depth near the camera, so it outranked the boxes sitting on its far end and
// ate triangular chunks out of them.

use egui::{Pos2, Rect, Vec2};
use squarez::project::Layer;
use squarez::three_d::camera::Camera3D;
use squarez::three_d::edit;
use squarez::three_d::mesh::{Face, Island, Mesh};
use squarez::three_d::render::build_scene;

const ATLAS: (u32, u32) = (256, 256);
const SIZE: u32 = 320;

/// Axis-aligned box from min to max corner, wound like Mesh::cube.
fn push_box(mesh: &mut Mesh, min: [f32; 3], max: [f32; 3]) {
    let base = mesh.vertices.len() as u32;
    let (x0, y0, z0) = (min[0], min[1], min[2]);
    let (x1, y1, z1) = (max[0], max[1], max[2]);
    mesh.vertices.extend([
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y0, z1],
        [x0, y0, z1],
        [x0, y1, z0],
        [x1, y1, z0],
        [x1, y1, z1],
        [x0, y1, z1],
    ]);
    let quads: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // bottom (-Y)
        [4, 7, 6, 5], // top (+Y)
        [3, 2, 6, 7], // front (+Z)
        [1, 0, 4, 5], // back (-Z)
        [2, 1, 5, 6], // right (+X)
        [0, 3, 7, 4], // left (-X)
    ];
    for q in quads {
        mesh.faces.push(Face {
            verts: q.iter().map(|v| v + base).collect(),
            island: Island::default(),
        });
    }
}

/// Rasterize the scene's triangles in their stored order. `zbuffer` switches
/// the fragment rule from "last drawn wins" (what the GPU does with the
/// painter's algorithm) to "nearest wins" (ground truth). Ties resolve to the
/// later triangle in BOTH modes, so shared edges between adjacent coplanar
/// faces can never produce a spurious mismatch.
fn render(mesh: &Mesh, cam: &Camera3D, zbuffer: bool) -> Vec<u32> {
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(SIZE as f32, SIZE as f32));
    let scene = build_scene(mesh, cam, rect, ATLAS);
    let mut color = vec![u32::MAX; (SIZE * SIZE) as usize];
    let mut depth = vec![f32::MIN; (SIZE * SIZE) as usize];
    for tri in &scene.tris {
        let [a, b, c] = tri.pts;
        let den = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        if den.abs() < 1e-6 {
            continue;
        }
        for y in 0..SIZE {
            for x in 0..SIZE {
                let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                let w1 = ((p.x - a.x) * (c.y - a.y) - (c.x - a.x) * (p.y - a.y)) / den;
                let w2 = ((b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)) / den;
                let w0 = 1.0 - w1 - w2;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let i = (y * SIZE + x) as usize;
                let z = w0 * tri.depths[0] + w1 * tri.depths[1] + w2 * tri.depths[2];
                if zbuffer && z < depth[i] - 1e-4 {
                    continue;
                }
                depth[i] = z;
                // Interiors are dimmed on screen, so front-ness is part of
                // what the eye sees — fold it into the compared value.
                color[i] = tri.face * 2 + tri.front as u32;
            }
        }
    }
    color
}

fn assert_painter_matches_zbuffer(name: &str, mesh: &Mesh, angles: &[(f32, f32)]) {
    for &(yaw, pitch) in angles {
        let cam = Camera3D { yaw, pitch, zoom: 12.0, offset: Vec2::ZERO, ..Default::default() };
        let painter = render(mesh, &cam, false);
        let truth = render(mesh, &cam, true);
        let bad = painter.iter().zip(truth.iter()).filter(|(a, b)| a != b).count();
        assert_eq!(
            bad, 0,
            "{name} at yaw {yaw} pitch {pitch}: {bad} pixels show the wrong face"
        );
    }
}

const ANGLES: [(f32, f32); 5] = [(0.7, 0.5), (0.3, 0.9), (2.2, 0.4), (4.0, 0.6), (5.5, 0.25)];

#[test]
fn slab_never_overdraws_boxes_resting_on_it() {
    // The reported scene: a wide slab with two boxes stacked on top. The
    // slab's huge top face is the classic average-depth misorder.
    let mut mesh = Mesh::default();
    push_box(&mut mesh, [-12.0, 0.0, -8.0], [12.0, 2.0, 8.0]);
    push_box(&mut mesh, [-3.0, 2.0, -3.0], [4.0, 4.0, 3.0]);
    push_box(&mut mesh, [-2.0, 4.0, -2.0], [2.0, 7.0, 2.0]);
    mesh.allocate_all_islands(ATLAS).unwrap();
    assert_painter_matches_zbuffer("slab + boxes", &mesh, &ANGLES);
}

#[test]
fn recessed_cube_draws_correctly() {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands(ATLAS).unwrap();
    let layer = Layer::new("Texture".to_string(), ATLAS.0, ATLAS.1);
    let front = mesh
        .faces
        .iter()
        .position(|f| mesh.face_normal(f)[2] > 0.5)
        .unwrap() as u32;
    let ins = edit::inset_faces(&mesh, &layer, &[front], 1, ATLAS).unwrap();
    let mesh = edit::extrude_faces_n(&ins.mesh, &layer, &[front], -3, ATLAS).unwrap().mesh;
    assert_painter_matches_zbuffer("recessed cube", &mesh, &ANGLES);
}

#[test]
fn curved_primitives_draw_correctly() {
    let layer = Layer::new("Texture".to_string(), ATLAS.0, ATLAS.1);
    let mut sphere = Mesh::sphere(8);
    sphere.allocate_all_islands(ATLAS).unwrap();
    let mesh = edit::add_object(&sphere, &layer, &Mesh::cylinder(6), ATLAS).unwrap().mesh;
    assert_painter_matches_zbuffer("sphere + cylinder", &mesh, &ANGLES[..3]);
}

#[test]
fn interiors_still_draw_before_all_fronts() {
    // The occlusion ordering works within each pass; the pass boundary itself
    // (dimmed interiors first, then fronts) is a separate invariant that keeps
    // the inside of open shells visible but never overdrawing the outside.
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands(ATLAS).unwrap();
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(SIZE as f32, SIZE as f32));
    let cam = Camera3D { yaw: 0.7, pitch: 0.5, zoom: 12.0, offset: Vec2::ZERO, ..Default::default() };
    let scene = build_scene(&mesh, &cam, rect, ATLAS);
    let first_front = scene.tris.iter().position(|t| t.front).unwrap_or(scene.tris.len());
    assert!(
        scene.tris[first_front..].iter().all(|t| t.front),
        "front triangles must form one contiguous block after the interiors"
    );
}
