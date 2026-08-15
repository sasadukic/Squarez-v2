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
use squarez::three_d::mesh::Mesh;
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
