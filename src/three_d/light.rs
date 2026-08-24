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

/// Cast-shadow rendering mode (3D effects menu, per tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShadowMode {
    #[default]
    Off,
    Hard,
    Soft,
}

impl ShadowMode {
    pub fn next(self) -> Self {
        match self {
            ShadowMode::Off => ShadowMode::Hard,
            ShadowMode::Hard => ShadowMode::Soft,
            ShadowMode::Soft => ShadowMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShadowMode::Off => "Off",
            ShadowMode::Hard => "On",
            ShadowMode::Soft => "Soft",
        }
    }
}

pub const SHADOW_DIM: f32 = 0.55;
/// Grid-flush rule: a hit closer than this is the attached seam itself
/// (an object resting on another must not shadow the contact crack). Kept
/// well under one grid unit so cast shadows still hug an object's base —
/// a larger cutoff visibly detaches the shadow from the object.
const MIN_SHADOW_DIST: f32 = 0.35;
pub const AO_STRENGTH: f32 = 0.5;
/// Tight contact range: only creases and contact points darken, never a
/// broad film over the model.
const AO_RANGE: f32 = 2.5;
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

fn point_in_poly(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let n = poly.len();
    let mut inside = false;
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        if (y0 > p.1) != (y1 > p.1) {
            let t = (p.1 - y0) / (y1 - y0);
            if p.0 < x0 + t * (x1 - x0) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Faces fully sandwiched against an opposed coincident face (the hidden
/// planes between grid-flush stacked objects). They are invisible and must
/// neither receive light nor occlude anything.
pub fn buried_faces(mesh: &Mesh) -> Vec<bool> {
    let n = mesh.faces.len();
    let mut buried = vec![false; n];
    let normals: Vec<[f32; 3]> = mesh.faces.iter().map(|f| norm(mesh.face_normal(f))).collect();
    for a in 0..n {
        let fa = &mesh.faces[a];
        let basis = mesh.face_plane_basis(fa);
        let axis = basis.dropped_axis();
        let plane_a = mesh.vertices[fa.verts[0] as usize][axis];
        // Sample points slightly inside face a (its vertices pulled toward
        // the centroid) — full containment in some opposed face buries it.
        let mut centroid = [0.0f32; 3];
        for &vi in &fa.verts {
            let v = mesh.vertices[vi as usize];
            for k in 0..3 {
                centroid[k] += v[k] / fa.verts.len() as f32;
            }
        }
        'other: for b in 0..n {
            if a == b || dot(normals[a], normals[b]) > -0.9 {
                continue;
            }
            let fb = &mesh.faces[b];
            let plane_b = mesh.vertices[fb.verts[0] as usize][axis];
            if (plane_a - plane_b).abs() > 0.05 {
                continue;
            }
            let poly_b: Vec<(f32, f32)> = fb
                .verts
                .iter()
                .map(|&vi| basis.project(mesh.vertices[vi as usize]))
                .collect();
            for &vi in &fa.verts {
                let v = mesh.vertices[vi as usize];
                let inset = [
                    v[0] + (centroid[0] - v[0]) * 0.02,
                    v[1] + (centroid[1] - v[1]) * 0.02,
                    v[2] + (centroid[2] - v[2]) * 0.02,
                ];
                if !point_in_poly(&poly_b, basis.project(inset)) {
                    continue 'other;
                }
            }
            buried[a] = true;
            break;
        }
    }
    buried
}

/// The mesh's triangles (fan-triangulated quads), pre-extracted once per bake.
fn triangles(mesh: &Mesh, buried: &[bool]) -> Vec<[[f32; 3]; 3]> {
    let mut tris = Vec::new();
    for (fi, face) in mesh.faces.iter().enumerate() {
        if buried.get(fi).copied().unwrap_or(false) {
            continue;
        }
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

/// Shadow-ray test honoring the grid-flush rule: hits closer than
/// MIN_SHADOW_DIST come from attached geometry and cast nothing.
fn shadowed(tris: &[[[f32; 3]; 3]], orig: [f32; 3], dir: [f32; 3]) -> bool {
    tris.iter().any(|t| {
        ray_tri(orig, dir, t[0], t[1], t[2]).is_some_and(|d| d > MIN_SHADOW_DIST)
    })
}

/// Nearest hit distance within range, if any — used for distance-weighted AO.
fn nearest_hit(tris: &[[[f32; 3]; 3]], orig: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for t in tris {
        if let Some(d) = ray_tri(orig, dir, t[0], t[1], t[2]) {
            if d < max_t && best.is_none_or(|b| d < b) {
                best = Some(d);
            }
        }
    }
    best
}

/// 16 fixed hemisphere directions around +Z, biased upward — grazing rays
/// read distant walls as occlusion and smear AO into a film, so both rings
/// stay high.
fn hemi_dirs() -> [[f32; 3]; 16] {
    let mut dirs = [[0.0f32; 3]; 16];
    let mut i = 0;
    for ring in 0..2 {
        let z = if ring == 0 { 0.85 } else { 0.6 };
        let r = (1.0f32 - z * z).sqrt();
        for k in 0..8 {
            let a = (k as f32 + ring as f32 * 0.5) * std::f32::consts::TAU / 8.0;
            dirs[i] = [r * a.cos(), r * a.sin(), z];
            i += 1;
        }
    }
    dirs
}

/// Per-texel lighting channels, each 0..=255:
/// `lambert` = base directional level, `shadow` = 255 fully sunlit .. 0 fully
/// in cast shadow (fractional with soft shadows), `ao` = 255 open .. 0 fully
/// occluded contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightTexel {
    pub lambert: u8,
    pub shadow: u8,
    pub ao: u8,
}

pub const UNLIT: LightTexel = LightTexel { lambert: 255, shadow: 255, ao: 255 };

/// Per-atlas-texel lighting channels. Texels outside every island stay
/// UNLIT. Contested texels: first face wins. `soft_shadows` casts a small
/// cone of jittered sun rays for fractional (anti-aliased) shadow edges.
pub fn bake_lightmap(
    mesh: &Mesh,
    atlas: (u32, u32),
    shadow_mode: ShadowMode,
    ao: bool,
) -> Vec<LightTexel> {
    let (aw, ah) = atlas;
    let mut map = vec![UNLIT; (aw as usize) * (ah as usize)];
    let mut claimed = vec![0u64; ((aw as usize) * (ah as usize)).div_ceil(64)];
    let buried = buried_faces(mesh);
    let tris = triangles(mesh, &buried);
    let hemi = hemi_dirs();

    for (fi, face) in mesh.faces.iter().enumerate() {
        let isl = face.island;
        if isl.w == 0 || isl.h == 0 || buried[fi] {
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

                let mut texel = LightTexel {
                    lambert: (lambert.clamp(0.0, 1.0) * 255.0).round() as u8,
                    shadow: 255,
                    ao: 255,
                };
                if shadow_mode != ShadowMode::Off {
                    let lit_frac = if shadow_mode == ShadowMode::Soft {
                        // A small cone of jittered rays: the lit fraction
                        // gives fractional penumbra instead of a hard step.
                        let mut lit = 0u32;
                        let jitter = 0.12f32;
                        let dirs = [
                            SUN,
                            norm([SUN[0] + jitter, SUN[1], SUN[2]]),
                            norm([SUN[0] - jitter, SUN[1], SUN[2]]),
                            norm([SUN[0], SUN[1] + jitter, SUN[2]]),
                            norm([SUN[0], SUN[1] - jitter, SUN[2]]),
                            norm([SUN[0], SUN[1], SUN[2] + jitter]),
                            norm([SUN[0], SUN[1], SUN[2] - jitter]),
                            norm([SUN[0] + jitter, SUN[1], SUN[2] - jitter]),
                        ];
                        for d in &dirs {
                            if !shadowed(&tris, orig, *d) {
                                lit += 1;
                            }
                        }
                        lit as f32 / dirs.len() as f32
                    } else if shadowed(&tris, orig, SUN) {
                        0.0
                    } else {
                        1.0
                    };
                    texel.shadow = (lit_frac * 255.0).round() as u8;
                }
                if ao {
                    // Distance-weighted: a hit right at the surface occludes
                    // fully, one at the range edge barely at all.
                    let mut occ = 0.0f32;
                    for d in &hemi {
                        let w = [
                            tx[0] * d[0] + ty[0] * d[1] + n[0] * d[2],
                            tx[1] * d[0] + ty[1] * d[1] + n[1] * d[2],
                            tx[2] * d[0] + ty[2] * d[1] + n[2] * d[2],
                        ];
                        if let Some(dist) = nearest_hit(&tris, orig, w, AO_RANGE) {
                            occ += 1.0 - dist / AO_RANGE;
                        }
                    }
                    let frac = (occ / hemi.len() as f32).clamp(0.0, 1.0);
                    texel.ao = ((1.0 - frac) * 255.0).round() as u8;
                }
                map[bit] = texel;
            }
        }
    }
    map
}

/// Cache key for a bake: geometry + islands + atlas + toggles.
pub fn lightmap_key(mesh: &Mesh, atlas: (u32, u32), shadow_mode: ShadowMode, ao: bool) -> u64 {
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
    (shadow_mode, ao).hash(&mut h);
    h.finish()
}


/// Additive light bounce from emissive texels onto nearby geometry.
/// Recomputed per canvas rebuild (it depends on paint, not just the mesh —
/// and it is cheap: emitters are capped). Returns per-texel additive RGB
/// scaled by `intensity` (0-100).
pub fn emissive_bounce(
    mesh: &Mesh,
    frame: &crate::project::Frame,
    glow: &[crate::project::Rgba],
    atlas: (u32, u32),
    intensity: u8,
) -> Vec<[u16; 3]> {
    const RANGE: f32 = 5.0;
    const MAX_EMITTERS: usize = 160;
    let (aw, ah) = atlas;
    let mut add = vec![[0u16; 3]; (aw as usize) * (ah as usize)];
    if glow.is_empty() || intensity == 0 {
        return add;
    }
    let top_color = |x: u32, y: u32| -> Option<crate::project::Rgba> {
        for layer in frame.layers.iter().rev() {
            if !layer.visible || layer.is_group || layer.pixels.is_empty() {
                continue;
            }
            let c = layer.get_pixel(x, y);
            if c[3] > 0 {
                return Some(c);
            }
        }
        None
    };
    // Collect emitters (world center + color), stride-sampling if plentiful.
    let mut emitters: Vec<([f32; 3], crate::project::Rgba)> = Vec::new();
    'collect: for face in &mesh.faces {
        let isl = face.island;
        for j in 0..isl.h as u32 {
            for i in 0..isl.w as u32 {
                let (gx, gy) = (isl.x as u32 + i, isl.y as u32 + j);
                let Some(c) = top_color(gx, gy) else { continue };
                if !glow.contains(&c) {
                    continue;
                }
                if let Some(q) = mesh.texel_quad_world(face, gx, gy) {
                    let mut w = [0.0f32; 3];
                    for corner in &q {
                        for a in 0..3 {
                            w[a] += corner[a] * 0.25;
                        }
                    }
                    emitters.push((w, c));
                    if emitters.len() >= MAX_EMITTERS {
                        break 'collect;
                    }
                }
            }
        }
    }
    if emitters.is_empty() {
        return add;
    }
    let strength = intensity as f32 / 100.0 * 0.5;
    for face in &mesh.faces {
        let isl = face.island;
        for j in 0..isl.h as u32 {
            for i in 0..isl.w as u32 {
                let (gx, gy) = (isl.x as u32 + i, isl.y as u32 + j);
                let Some(q) = mesh.texel_quad_world(face, gx, gy) else { continue };
                let mut w = [0.0f32; 3];
                for corner in &q {
                    for a in 0..3 {
                        w[a] += corner[a] * 0.25;
                    }
                }
                let mut acc = [0.0f32; 3];
                for (e, c) in &emitters {
                    let d2 = (w[0] - e[0]).powi(2) + (w[1] - e[1]).powi(2) + (w[2] - e[2]).powi(2);
                    if d2 >= RANGE * RANGE || d2 < 0.25 {
                        continue; // own texel / immediate neighbours glow already
                    }
                    let f = (1.0 - d2.sqrt() / RANGE).powi(2) * strength;
                    acc[0] += c[0] as f32 * f;
                    acc[1] += c[1] as f32 * f;
                    acc[2] += c[2] as f32 * f;
                }
                if acc[0] + acc[1] + acc[2] > 1.0 {
                    let idx = (gy * aw + gx) as usize;
                    add[idx] = [
                        acc[0].min(255.0) as u16,
                        acc[1].min(255.0) as u16,
                        acc[2].min(255.0) as u16,
                    ];
                }
            }
        }
    }
    add
}
