// tests/render_seam_tests.rs
//
// Pixel-level seam regression: software-rasterize the real render pipeline
// output (build_scene triangles + the real padded atlas) with NEAREST
// sampling including boundary-rounding jitter, and assert that every
// rendered fragment of every face samples that face's own paint color.
// This locks the entire "white seams at face boundaries" defect class.

use egui::{Pos2, Rect, Vec2};
use squarez::project::{Layer, Rgba};
use squarez::three_d::camera::Camera3D;
use squarez::three_d::mesh::{Mesh, UV_INSET};
use squarez::three_d::paint::fill_island;
use squarez::three_d::render::build_scene;
use squarez::three_d::pad_island_gutters;

const ATLAS: (u32, u32) = (256, 256);

/// Six distinct opaque fill colors, one per cube face.
fn face_color(i: usize) -> Rgba {
    [
        [200, 40, 40, 255],
        [40, 200, 40, 255],
        [40, 40, 200, 255],
        [200, 200, 40, 255],
        [40, 200, 200, 255],
        [200, 40, 200, 255],
    ][i % 6]
}

fn painted_cube() -> (Mesh, Layer) {
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands(ATLAS).unwrap();
    let mut layer = Layer::new("Texture".to_string(), ATLAS.0, ATLAS.1);
    for (i, face) in mesh.faces.iter().enumerate() {
        for (x, y, _, new) in fill_island(&mut layer, face.island, face_color(i)) {
            layer.set_pixel(x, y, new);
        }
    }
    (mesh, layer)
}

fn sample(pixels: &[u8], x: i64, y: i64) -> Rgba {
    let x = x.clamp(0, ATLAS.0 as i64 - 1) as u32;
    let y = y.clamp(0, ATLAS.1 as i64 - 1) as u32;
    let i = ((y * ATLAS.0 + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// Rasterize every triangle and return the number of fragments whose
/// sampled color (under any boundary-rounding jitter) is not the color
/// expected for that triangle's face.
fn count_violations(mesh: &Mesh, pixels: &[u8], expected: impl Fn(u32) -> Rgba) -> usize {
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0));
    let mut violations = 0;
    let angles: Vec<(f32, f32)> = (0..12)
        .map(|k| (k as f32 * std::f32::consts::TAU / 12.0 + 0.13, 0.6154))
        .chain((0..6).map(|k| (k as f32 * std::f32::consts::TAU / 6.0 + 0.4, -0.5)))
        .collect();
    for (yaw, pitch) in angles {
        for zoom in [12.0f32, 5.0] {
            let cam = Camera3D { yaw, pitch, zoom, offset: Vec2::ZERO };
            let scene = build_scene(mesh, &cam, rect, ATLAS);
            for tri in &scene.tris {
                let want = expected(tri.face);
                let min_x = tri.pts.iter().map(|p| p.x).fold(f32::MAX, f32::min).floor() as i32;
                let max_x = tri.pts.iter().map(|p| p.x).fold(f32::MIN, f32::max).ceil() as i32;
                let min_y = tri.pts.iter().map(|p| p.y).fold(f32::MAX, f32::min).floor() as i32;
                let max_y = tri.pts.iter().map(|p| p.y).fold(f32::MIN, f32::max).ceil() as i32;
                let [a, b, c] = tri.pts;
                let denom = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
                if denom.abs() < 1e-6 {
                    continue;
                }
                for py in min_y..=max_y {
                    for px in min_x..=max_x {
                        let p = Pos2::new(px as f32 + 0.5, py as f32 + 0.5);
                        let w1 = ((p.x - a.x) * (c.y - a.y) - (c.x - a.x) * (p.y - a.y)) / denom;
                        let w2 = ((b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)) / denom;
                        let w0 = 1.0 - w1 - w2;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let u = w0 * tri.uvs[0].x + w1 * tri.uvs[1].x + w2 * tri.uvs[2].x;
                        let v = w0 * tri.uvs[0].y + w1 * tri.uvs[1].y + w2 * tri.uvs[2].y;
                        // NEAREST fetch with boundary-rounding jitter: a GPU
                        // may resolve exact texel boundaries either way.
                        //
                        // +/-0.49 is only survivable because every island here
                        // has a gutter, whose 1-texel dilation ring carries the
                        // face's own edge color. Do NOT reuse this margin on a
                        // mesh whose islands touch — there is no ring there, and
                        // containment (see uvs_never_leave_their_island) is what
                        // protects those instead.
                        for jitter in [-0.49f32, 0.0, 0.49] {
                            let tx = (u * ATLAS.0 as f32 + jitter).floor() as i64;
                            let ty = (v * ATLAS.1 as f32 + jitter).floor() as i64;
                            let got = sample(pixels, tx, ty);
                            if got != want {
                                violations += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    violations
}

/// Number of rendered fragments whose sampled texel falls outside the island
/// of the face being drawn. `jitter` is interpolation slop, in texels.
///
/// This is the containment invariant that lets islands sit flush against each
/// other: if no fragment ever leaves its own island, a neighbour's paint can
/// never be read, whatever is packed next door.
fn count_island_escapes(mesh: &Mesh, jitter: f32) -> usize {
    let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0));
    let mut escapes = 0;
    for k in 0..12 {
        let cam = Camera3D {
            yaw: k as f32 * std::f32::consts::TAU / 12.0 + 0.13,
            pitch: 0.6154,
            zoom: 12.0,
            offset: Vec2::ZERO,
        };
        let scene = build_scene(mesh, &cam, rect, ATLAS);
        for tri in &scene.tris {
            let isl = mesh.faces[tri.face as usize].island;
            let [a, b, c] = tri.pts;
            let denom = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
            if denom.abs() < 1e-6 {
                continue;
            }
            let min_x = tri.pts.iter().map(|p| p.x).fold(f32::MAX, f32::min).floor() as i32;
            let max_x = tri.pts.iter().map(|p| p.x).fold(f32::MIN, f32::max).ceil() as i32;
            let min_y = tri.pts.iter().map(|p| p.y).fold(f32::MAX, f32::min).floor() as i32;
            let max_y = tri.pts.iter().map(|p| p.y).fold(f32::MIN, f32::max).ceil() as i32;
            for py in min_y..=max_y {
                for px in min_x..=max_x {
                    let p = Pos2::new(px as f32 + 0.5, py as f32 + 0.5);
                    let w1 = ((p.x - a.x) * (c.y - a.y) - (c.x - a.x) * (p.y - a.y)) / denom;
                    let w2 = ((b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)) / denom;
                    let w0 = 1.0 - w1 - w2;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let u = w0 * tri.uvs[0].x + w1 * tri.uvs[1].x + w2 * tri.uvs[2].x;
                    let v = w0 * tri.uvs[0].y + w1 * tri.uvs[1].y + w2 * tri.uvs[2].y;
                    for j in [-jitter, 0.0, jitter] {
                        let tx = (u * ATLAS.0 as f32 + j).floor() as i64;
                        let ty = (v * ATLAS.1 as f32 + j).floor() as i64;
                        let inside = tx >= isl.x as i64
                            && ty >= isl.y as i64
                            && tx < (isl.x + isl.w) as i64
                            && ty < (isl.y + isl.h) as i64;
                        if !inside {
                            escapes += 1;
                        }
                    }
                }
            }
        }
    }
    escapes
}

#[test]
fn uvs_never_leave_their_island() {
    // The margin UV_INSET buys. Note this is a far smaller jitter than the
    // +/-0.49 used by the seam tests below: that one models a GPU resolving an
    // exact texel boundary either way, and is absorbed by the 1-texel dilation
    // ring, which only exists where an island has a gutter. Containment is what
    // protects islands that touch.
    let jitter = UV_INSET * 0.9;
    for (name, mut mesh) in [
        ("cube", Mesh::cube(8)),
        ("plane", Mesh::plane(8)),
        ("cylinder", Mesh::cylinder(8)),
        ("sphere", Mesh::sphere(8)),
    ] {
        mesh.allocate_all_islands(ATLAS).unwrap();
        assert_eq!(
            count_island_escapes(&mesh, jitter),
            0,
            "{name}: every fragment must sample inside its own face's island"
        );
    }
}

#[test]
fn escape_detector_catches_a_missing_inset() {
    // Sanity: the detector must be able to SEE an escape. A jitter well past
    // the inset is exactly the error the inset is sized to absorb.
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands(ATLAS).unwrap();
    assert!(
        count_island_escapes(&mesh, 0.49) > 0,
        "detector failed to reproduce an escape at jitter far beyond UV_INSET"
    );
}

#[test]
fn painted_faces_render_without_seams() {
    let (mesh, layer) = painted_cube();

    // The real upload path: composited pixels + gutter padding.
    let mut padded = layer.pixels.clone();
    pad_island_gutters(&mut padded, ATLAS.0, ATLAS.1, &mesh);

    let violations = count_violations(&mesh, &padded, |fi| face_color(fi as usize));
    assert_eq!(violations, 0, "padded atlas must render every face in its own color only");
}

#[test]
fn seam_detector_catches_unpadded_atlas() {
    // Sanity: the same rasterizer must be able to SEE the defect. Without
    // padding, boundary-jittered fetches fall into the empty gutter.
    let (mesh, layer) = painted_cube();
    let violations = count_violations(&mesh, &layer.pixels, |fi| face_color(fi as usize));
    assert!(violations > 0, "detector failed to reproduce the seam defect on an unpadded atlas");
}

#[test]
fn dilation_never_overwrites_a_neighbours_texels() {
    // Under a projected layout islands abut with no gutter between them.
    // Dilating a border outward there would replace a neighbour's real paint
    // with a stranger's edge color — so every claimed texel must survive the
    // padding pass byte for byte.
    let mut mesh = Mesh::plane(8);
    // Split the quad in X so the two halves share an edge and abut in the atlas.
    mesh.vertices.push([0.0, 0.0, -4.0]);
    mesh.vertices.push([0.0, 0.0, 4.0]);
    mesh.faces[0].verts = vec![0, 3, 5, 4];
    mesh.faces.push(squarez::three_d::mesh::Face {
        verts: vec![4, 5, 2, 1],
        island: squarez::three_d::mesh::Island::default(),
    });
    mesh.allocate_all_islands(ATLAS).unwrap();
    let (a, b) = (mesh.faces[0].island, mesh.faces[1].island);
    assert_eq!(a.x + a.w, b.x, "halves must abut for this test to mean anything");

    let mut layer = Layer::new("Texture".to_string(), ATLAS.0, ATLAS.1);
    for (i, face) in mesh.faces.iter().enumerate() {
        for (x, y, _, new) in fill_island(&mut layer, face.island, face_color(i)) {
            layer.set_pixel(x, y, new);
        }
    }

    let mut padded = layer.pixels.clone();
    pad_island_gutters(&mut padded, ATLAS.0, ATLAS.1, &mesh);

    for (i, face) in mesh.faces.iter().enumerate() {
        let isl = face.island;
        for y in isl.y..isl.y + isl.h {
            for x in isl.x..isl.x + isl.w {
                assert_eq!(
                    sample(&padded, x as i64, y as i64),
                    face_color(i),
                    "padding overwrote face {i}'s texel at ({x}, {y})"
                );
            }
        }
    }

    // ...while the outward sides, which are genuinely unclaimed, still dilate.
    assert_eq!(sample(&padded, a.x as i64 - 1, a.y as i64), face_color(0));
    assert_eq!(sample(&padded, (b.x + b.w) as i64, b.y as i64), face_color(1));
}
