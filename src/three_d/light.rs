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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
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
/// Just above the ray-origin offset so a surface cannot shadow itself;
/// everything else counts — the seam between flush objects is protected by
/// buried-face exclusion, not by distance (a distance cutoff detaches thin
/// objects' shadows from their base).
const MIN_SHADOW_DIST: f32 = 0.05;
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

/// An occluder face as a planar polygon: no fan-triangulation, so no
/// diagonal seam a grazing ray can slip through (that seam produced lit
/// notches across otherwise straight shadows).
struct OccPoly {
    point: [f32; 3],
    normal: [f32; 3],
    /// Vertices in the 2D plane frame (u = tangent, v = bitangent).
    verts2: Vec<(f32, f32)>,
    tangent: [f32; 3],
    bitangent: [f32; 3],
}

impl OccPoly {
    /// Hit distance along `dir`, if the ray crosses the polygon's plane
    /// inside it (edges inclusive).
    fn hit(&self, orig: [f32; 3], dir: [f32; 3]) -> Option<f32> {
        let denom = dot(dir, self.normal);
        if denom.abs() < 1e-9 {
            return None;
        }
        let t = (dot(self.point, self.normal) - dot(orig, self.normal)) / denom;
        if t <= EPS {
            return None;
        }
        let hp = [
            orig[0] + dir[0] * t,
            orig[1] + dir[1] * t,
            orig[2] + dir[2] * t,
        ];
        let rel = [hp[0] - self.point[0], hp[1] - self.point[1], hp[2] - self.point[2]];
        let p2 = (dot(rel, self.tangent), dot(rel, self.bitangent));
        // Inclusive edge test: grow the polygon a hair so boundary hits and
        // shared face edges always count (double-counting is harmless for
        // boolean occlusion).
        let n = self.verts2.len();
        let mut inside = false;
        for i in 0..n {
            let (x0, y0) = self.verts2[i];
            let (x1, y1) = self.verts2[(i + 1) % n];
            if (y0 > p2.1) != (y1 > p2.1) {
                let tt = (p2.1 - y0) / (y1 - y0);
                if p2.0 < x0 + tt * (x1 - x0) + 1e-4 {
                    inside = !inside;
                }
            }
        }
        if inside { Some(t) } else { None }
    }
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

/// Occluder polygons for every non-buried face, pre-extracted per bake.
fn occluder_polys(mesh: &Mesh, buried: &[bool]) -> Vec<OccPoly> {
    let mut polys = Vec::new();
    for (fi, face) in mesh.faces.iter().enumerate() {
        if buried.get(fi).copied().unwrap_or(false) || face.verts.len() < 3 {
            continue;
        }
        let normal = norm(mesh.face_normal(face));
        let p0 = mesh.vertices[face.verts[0] as usize];
        let up = if normal[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let tangent = norm(cross(up, normal));
        let bitangent = cross(normal, tangent);
        let verts2 = face
            .verts
            .iter()
            .map(|&vi| {
                let v = mesh.vertices[vi as usize];
                let rel = [v[0] - p0[0], v[1] - p0[1], v[2] - p0[2]];
                (dot(rel, tangent), dot(rel, bitangent))
            })
            .collect();
        polys.push(OccPoly { point: p0, normal, verts2, tangent, bitangent });
    }
    polys
}

/// Shadow-ray test: anything past the self-hit guard casts.
fn shadowed(polys: &[OccPoly], orig: [f32; 3], dir: [f32; 3]) -> bool {
    polys
        .iter()
        .any(|p| p.hit(orig, dir).is_some_and(|d| d > MIN_SHADOW_DIST))
}

/// Nearest hit distance within range, if any — used for distance-weighted AO.
fn nearest_hit(polys: &[OccPoly], orig: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for p in polys {
        if let Some(d) = p.hit(orig, dir) {
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
    let polys = occluder_polys(mesh, &buried);
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
                            if !shadowed(&polys, orig, *d) {
                                lit += 1;
                            }
                        }
                        lit as f32 / dirs.len() as f32
                    } else if shadowed(&polys, orig, SUN) {
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
                        if let Some(dist) = nearest_hit(&polys, orig, w, AO_RANGE) {
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
    // Collect emitters (world center + color + facing), stride-sampling if
    // plentiful. Light leaves a surface FORWARD: nothing spills behind it.
    let mut emitters: Vec<([f32; 3], crate::project::Rgba, [f32; 3])> = Vec::new();
    'collect: for face in &mesh.faces {
        let isl = face.island;
        let fnorm = norm(mesh.face_normal(face));
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
                    emitters.push((w, c, fnorm));
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
        let rnorm = norm(mesh.face_normal(face));
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
                for (e, c, en) in &emitters {
                    let to_recv = [w[0] - e[0], w[1] - e[1], w[2] - e[2]];
                    // Behind the emitting surface: no light.
                    if dot(to_recv, *en) <= 0.05 {
                        continue;
                    }
                    // The receiving surface must face the emitter.
                    if dot(rnorm, [-to_recv[0], -to_recv[1], -to_recv[2]]) <= 0.0 {
                        continue;
                    }
                    let d2 = to_recv[0].powi(2) + to_recv[1].powi(2) + to_recv[2].powi(2);
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
