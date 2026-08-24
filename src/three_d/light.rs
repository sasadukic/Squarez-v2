// src/three_d/light.rs
//
// Baked per-texel lighting: a fixed WORLD-space sun (unlike render.rs's
// view-space LIGHT, which orbits with the camera), cast shadows via a ray
// toward the sun, and hemisphere ambient occlusion. Baked into the DISPLAYED
// canvas texture on rebuild — never into layer data — so orbiting costs
// nothing and edits re-bake.

use super::mesh::Mesh;

/// World-space sun direction (normalized): above, left, toward the viewer's
/// home orbit.
pub const SUN: [f32; 3] = [-0.394, 0.788, 0.473];

const SHADOW_DIM: f32 = 0.55;
const AO_STRENGTH: f32 = 0.45;
const AO_RANGE: f32 = 8.0;
const EPS: f32 = 1e-4;

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Möller–Trumbore; returns the hit distance along `dir` if positive.
fn ray_tri(orig: [f32; 3], dir: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<f32> {
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t_vec = [orig[0] - a[0], orig[1] - a[1], orig[2] - a[2]];
    let u = dot(t_vec, p) * inv;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = cross(t_vec, e1);
    let v = dot(dir, q) * inv;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let t = dot(e2, q) * inv;
    if t > EPS { Some(t) } else { None }
}

/// The mesh's triangles (fan-triangulated quads), pre-extracted once per bake.
fn triangles(mesh: &Mesh) -> Vec<[[f32; 3]; 3]> {
    let mut tris = Vec::new();
    for face in &mesh.faces {
        let v = &face.verts;
        for i in 1..v.len().saturating_sub(1) {
            tris.push([
                mesh.vertices[v[0] as usize],
                mesh.vertices[v[i] as usize],
                mesh.vertices[v[i + 1] as usize],
            ]);
        }
    }
    tris
}

fn occluded(tris: &[[[f32; 3]; 3]], orig: [f32; 3], dir: [f32; 3], max_t: f32) -> bool {
    tris.iter().any(|t| {
        ray_tri(orig, dir, t[0], t[1], t[2]).is_some_and(|d| d < max_t)
    })
}

/// 16 fixed cosine-ish hemisphere directions around +Z, rotated per normal.
fn hemi_dirs() -> [[f32; 3]; 16] {
    let mut dirs = [[0.0f32; 3]; 16];
    let mut i = 0;
    for ring in 0..2 {
        let z = if ring == 0 { 0.85 } else { 0.4 };
        let r = (1.0f32 - z * z).sqrt();
        for k in 0..8 {
            let a = (k as f32 + ring as f32 * 0.5) * std::f32::consts::TAU / 8.0;
            dirs[i] = [r * a.cos(), r * a.sin(), z];
            i += 1;
        }
    }
    dirs
}

/// Per-atlas-texel brightness multiplier, 0..=255 (255 = fully lit). Texels
/// outside every island stay 255. Contested texels: first face wins.
pub fn bake_lightmap(mesh: &Mesh, atlas: (u32, u32), shadows: bool, ao: bool) -> Vec<u8> {
    let (aw, ah) = atlas;
    let mut map = vec![255u8; (aw as usize) * (ah as usize)];
    let mut claimed = vec![0u64; ((aw as usize) * (ah as usize)).div_ceil(64)];
    let tris = triangles(mesh);
    let hemi = hemi_dirs();

    for face in &mesh.faces {
        let isl = face.island;
        if isl.w == 0 || isl.h == 0 {
            continue;
        }
        let n = norm(mesh.face_normal(face));
        // Flat world-space lambert per face.
        let lambert = 0.55 + 0.45 * dot(n, SUN).max(0.0);
        // Tangent frame for the hemisphere.
        let up = if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let tx = norm(cross(up, n));
        let ty = cross(n, tx);

        for j in 0..isl.h as u32 {
            for i in 0..isl.w as u32 {
                let (gx, gy) = (isl.x as u32 + i, isl.y as u32 + j);
                let bit = (gy as usize) * (aw as usize) + gx as usize;
                if gx >= aw || gy >= ah || claimed[bit / 64] & (1 << (bit % 64)) != 0 {
                    continue;
                }
                claimed[bit / 64] |= 1 << (bit % 64);
                let Some(quad) = mesh.texel_quad_world(face, gx, gy) else { continue };
                let mut c = [0.0f32; 3];
                for q in &quad {
                    for a in 0..3 {
                        c[a] += q[a] * 0.25;
                    }
                }
                let orig = [c[0] + n[0] * 0.01, c[1] + n[1] * 0.01, c[2] + n[2] * 0.01];

                let mut level = lambert;
                if shadows && occluded(&tris, orig, SUN, f32::MAX) {
                    level *= SHADOW_DIM;
                }
                if ao {
                    let mut occ = 0u32;
                    for d in &hemi {
                        let w = [
                            tx[0] * d[0] + ty[0] * d[1] + n[0] * d[2],
                            tx[1] * d[0] + ty[1] * d[1] + n[1] * d[2],
                            tx[2] * d[0] + ty[2] * d[1] + n[2] * d[2],
                        ];
                        if occluded(&tris, orig, w, AO_RANGE) {
                            occ += 1;
                        }
                    }
                    level *= 1.0 - AO_STRENGTH * (occ as f32 / hemi.len() as f32);
                }
                map[bit] = (level.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    map
}

/// Cache key for a bake: geometry + islands + atlas + toggles.
pub fn lightmap_key(mesh: &Mesh, atlas: (u32, u32), shadows: bool, ao: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in &mesh.vertices {
        for a in v {
            a.to_bits().hash(&mut h);
        }
    }
    for f in &mesh.faces {
        f.verts.hash(&mut h);
        (f.island.x, f.island.y, f.island.w, f.island.h).hash(&mut h);
    }
    atlas.hash(&mut h);
    (shadows, ao).hash(&mut h);
    h.finish()
}
