// src/three_d/render.rs
//
// Projects the mesh through the camera into screen-space triangles
// (painter's algorithm, backface-culled) and draws them as one textured
// egui::Mesh sampling the project's atlas texture. The same `Scene` doubles
// as the picking structure for painting and face selection.

use egui::{Color32, Pos2, Rect, Stroke};

use super::camera::{Camera3D, SnapView};
use super::mesh::{Mesh, UV_INSET};
use crate::theme::Theme;

/// Axis colors, shared with the navigation gizmo (Blender convention).
pub const AXIS_X: Color32 = Color32::from_rgb(226, 84, 84);
pub const AXIS_Y: Color32 = Color32::from_rgb(108, 194, 92);
pub const AXIS_Z: Color32 = Color32::from_rgb(94, 136, 226);

#[derive(Debug, Clone, Copy)]
pub struct SceneTri {
    /// Projected screen positions.
    pub pts: [Pos2; 3],
    /// Normalized atlas UVs.
    pub uvs: [Pos2; 3],
    /// Average view-space z; larger = closer to the camera.
    pub depth: f32,
    /// Index of the face this triangle belongs to.
    pub face: u32,
    /// Per-channel lighting tint multiplied into the texture
    /// ([1, 1, 1] = unlit). Lit faces lean warm, shadowed faces cool.
    pub shade: [f32; 3],
    /// Front-facing (outside) surface; false = dimmed interior.
    pub front: bool,
    /// Per-corner view-space z, for interpolated occlusion tests.
    pub depths: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    /// Sorted far → near.
    pub tris: Vec<SceneTri>,
    /// Face indices that survived backface culling.
    pub visible_faces: Vec<u32>,
}

/// Project every front-facing face into screen-space triangles, sorted
/// far → near. `atlas` is the texture dimensions in texels.
pub fn build_scene(mesh: &Mesh, cam: &Camera3D, rect: Rect, atlas: (u32, u32)) -> Scene {
    build_scene_with_shading(mesh, cam, rect, atlas, true)
}

/// `build_scene` with the viewport lighting under caller control: `shaded:
/// false` renders every face in its raw texel colors (the workspace's
/// shading toggle). Interior surfaces stay dimmed either way — that dimming
/// is depth legibility, not lighting.
pub fn build_scene_with_shading(
    mesh: &Mesh,
    cam: &Camera3D,
    rect: Rect,
    atlas: (u32, u32),
    shaded: bool,
) -> Scene {
    let mut scene = Scene::default();
    let aw = atlas.0.max(1) as f32;
    let ah = atlas.1.max(1) as f32;

    // Per-face lighting from a view-space key light (top-left-front), like
    // Blender's solid viewport — with a pixel-art touch: lit faces shift
    // slightly warm, shadowed faces slightly cool, instead of a flat gray
    // multiply. Snapped views render unlit so texel colors read true.
    let unlit = !shaded || cam.snapped().is_some();
    const LIGHT: [f32; 3] = [-0.324, 0.417, 0.849]; // normalized
    const WARM: [f32; 3] = [1.0, 0.985, 0.94];
    const COOL: [f32; 3] = [0.9, 0.94, 1.0];

    for (fi, face) in mesh.faces.iter().enumerate() {
        let n = cam.view_dir(mesh.face_normal(face));
        let front = n[2] > 0.0;
        if front {
            scene.visible_faces.push(fi as u32);
        }
        let mut shade = if unlit {
            [1.0, 1.0, 1.0]
        } else {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            let lambert = ((n[0] * LIGHT[0] + n[1] * LIGHT[1] + n[2] * LIGHT[2]) / len).max(0.0);
            let level = 0.55 + 0.45 * lambert;
            [
                level * (COOL[0] + (WARM[0] - COOL[0]) * lambert),
                level * (COOL[1] + (WARM[1] - COOL[1]) * lambert),
                level * (COOL[2] + (WARM[2] - COOL[2]) * lambert),
            ]
        };
        if !front {
            // Interior surfaces (seen through holes) render dimmed so the
            // inside of an open shell is visible but clearly "inside".
            shade = [shade[0] * 0.5, shade[1] * 0.5, shade[2] * 0.5];
        }

        // Screen positions + per-corner normalized UVs via the face's plane basis.
        let uv = mesh.face_uv_map(face, UV_INSET);
        let corners: Vec<(Pos2, Pos2, f32)> = face
            .verts
            .iter()
            .map(|&vi| {
                let p = mesh.vertices[vi as usize];
                let (pos, depth) = cam.project(p, rect);
                let (tx, ty) = uv.texel(p);
                (pos, Pos2::new(tx / aw, ty / ah), depth)
            })
            .collect();

        // Fan-triangulate (quads become two tris; tris stay as-is).
        for k in 1..corners.len() - 1 {
            let (a, b, c) = (corners[0], corners[k], corners[k + 1]);
            // Skip degenerate (zero-area) triangles.
            let area = (b.0.x - a.0.x) * (c.0.y - a.0.y) - (b.0.y - a.0.y) * (c.0.x - a.0.x);
            if area.abs() < f32::EPSILON {
                continue;
            }
            scene.tris.push(SceneTri {
                pts: [a.0, b.0, c.0],
                uvs: [a.1, b.1, c.1],
                depth: (a.2 + b.2 + c.2) / 3.0,
                face: fi as u32,
                shade,
                front,
                depths: [a.2, b.2, c.2],
            });
        }
    }

    // Two passes: every interior triangle first, then front triangles —
    // interiors can never overdraw the outside. Within each pass, order by
    // actual screen-space occlusion, not just average depth: a large face's
    // average can sit nearer than a small face resting on top of it, which
    // made slabs overdraw boxes placed on them.
    scene
        .tris
        .sort_by(|a, b| a.front.cmp(&b.front).then(a.depth.total_cmp(&b.depth)));
    let split = scene.tris.iter().position(|t| t.front).unwrap_or(scene.tris.len());
    order_by_occlusion(&mut scene.tris[..split]);
    order_by_occlusion(&mut scene.tris[split..]);
    scene
}

/// The overlap polygon of two screen triangles (Sutherland–Hodgman clip of
/// `sub` against `clip`). Empty when they don't overlap in area.
fn tri_overlap(sub: &[Pos2; 3], clip: &[Pos2; 3]) -> Vec<Pos2> {
    let area2 = (clip[1].x - clip[0].x) * (clip[2].y - clip[0].y)
        - (clip[2].x - clip[0].x) * (clip[1].y - clip[0].y);
    if area2.abs() < 1e-6 {
        return Vec::new();
    }
    // Orient the clip triangle CCW so "inside" is a consistent side.
    let ccw = if area2 > 0.0 { *clip } else { [clip[0], clip[2], clip[1]] };
    let mut poly: Vec<Pos2> = sub.to_vec();
    for i in 0..3 {
        let (e0, e1) = (ccw[i], ccw[(i + 1) % 3]);
        let side = |p: Pos2| (e1.x - e0.x) * (p.y - e0.y) - (e1.y - e0.y) * (p.x - e0.x);
        let input = std::mem::take(&mut poly);
        for j in 0..input.len() {
            let (cur, nxt) = (input[j], input[(j + 1) % input.len()]);
            let (dc, dn) = (side(cur), side(nxt));
            if dc >= 0.0 {
                poly.push(cur);
            }
            if (dc >= 0.0) != (dn >= 0.0) {
                let t = dc / (dc - dn);
                poly.push(Pos2::new(cur.x + (nxt.x - cur.x) * t, cur.y + (nxt.y - cur.y) * t));
            }
        }
        if poly.is_empty() {
            return poly;
        }
    }
    poly
}

/// Interpolated view-space depth of `tri` at screen point `p` (barycentric;
/// exact under the orthographic camera). None for degenerate triangles.
fn tri_depth_at(tri: &SceneTri, p: Pos2) -> Option<f32> {
    let [a, b, c] = tri.pts;
    let den = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
    if den.abs() < 1e-6 {
        return None;
    }
    let w1 = ((p.x - a.x) * (c.y - a.y) - (c.x - a.x) * (p.y - a.y)) / den;
    let w2 = ((b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y)) / den;
    let w0 = 1.0 - w1 - w2;
    Some(w0 * tri.depths[0] + w1 * tri.depths[1] + w2 * tri.depths[2])
}

/// Reorder one pass so every triangle draws after whatever it occludes.
///
/// Average-depth sorting is only a heuristic: it compares triangle centers,
/// so a large triangle whose center is near the camera outranks a small
/// triangle resting on top of its far end. The real constraint is pairwise —
/// where two triangles overlap on screen, the one behind at that overlap must
/// draw first. Build exactly those constraints (true polygon overlap, depth
/// compared at the overlap's centroid; exact for planar faces under an
/// orthographic camera) and emit a topological order, taking the farthest
/// ready triangle first so unconstrained regions keep the old far-to-near
/// behavior. Occlusion cycles (rare; impossible for non-intersecting
/// axis-aligned solids) fall back to average depth.
///
/// Expected slice: already sorted far-to-near. O(n²) pair tests with cheap
/// bbox rejection — meshes are tens to low hundreds of faces.
fn order_by_occlusion(tris: &mut [SceneTri]) {
    let n = tris.len();
    if n < 2 {
        return;
    }
    let bounds: Vec<(Rect, f32, f32)> = tris
        .iter()
        .map(|t| {
            let r = Rect::from_min_max(
                Pos2::new(
                    t.pts.iter().map(|p| p.x).fold(f32::MAX, f32::min),
                    t.pts.iter().map(|p| p.y).fold(f32::MAX, f32::min),
                ),
                Pos2::new(
                    t.pts.iter().map(|p| p.x).fold(f32::MIN, f32::max),
                    t.pts.iter().map(|p| p.y).fold(f32::MIN, f32::max),
                ),
            );
            let lo = t.depths.iter().copied().fold(f32::MAX, f32::min);
            let hi = t.depths.iter().copied().fold(f32::MIN, f32::max);
            (r, lo, hi)
        })
        .collect();

    let mut succ: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut indeg = vec![0u32; n];
    for i in 0..n {
        for j in (i + 1)..n {
            // Same face: coplanar by construction, no occlusion between them.
            if tris[i].face == tris[j].face || !bounds[i].0.intersects(bounds[j].0) {
                continue;
            }
            let poly = tri_overlap(&tris[i].pts, &tris[j].pts);
            if poly.len() < 3 {
                continue;
            }
            // Shoelace area: ignore degenerate slivers (shared edges between
            // adjacent faces clip to zero-area polygons).
            let area = poly
                .windows(2)
                .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
                .sum::<f32>()
                + poly[poly.len() - 1].x * poly[0].y
                - poly[0].x * poly[poly.len() - 1].y;
            if area.abs() < 0.05 {
                continue;
            }
            let k = poly.len() as f32;
            let centroid = Pos2::new(
                poly.iter().map(|p| p.x).sum::<f32>() / k,
                poly.iter().map(|p| p.y).sum::<f32>() / k,
            );
            let (Some(di), Some(dj)) =
                (tri_depth_at(&tris[i], centroid), tri_depth_at(&tris[j], centroid))
            else {
                continue;
            };
            if (di - dj).abs() < 1e-3 {
                continue; // coplanar across faces: order is irrelevant
            }
            // Larger depth = nearer the camera; the smaller draws first.
            let (behind, front) = if di < dj { (i, j) } else { (j, i) };
            succ[behind].push(front as u32);
            indeg[front] += 1;
        }
    }

    // Kahn's algorithm, farthest-first among the ready set.
    let mut ready: std::collections::BinaryHeap<(std::cmp::Reverse<ordered::F32>, usize)> =
        (0..n)
            .filter(|&k| indeg[k] == 0)
            .map(|k| (std::cmp::Reverse(ordered::F32(tris[k].depth)), k))
            .collect();
    let mut done = vec![false; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    while order.len() < n {
        let k = loop {
            match ready.pop() {
                Some((_, k)) if !done[k] => break Some(k),
                Some(_) => continue,
                None => break None,
            }
        };
        // Empty ready set with work left = an occlusion cycle; break it by
        // taking the farthest remaining triangle.
        let k = k.unwrap_or_else(|| {
            (0..n)
                .filter(|&k| !done[k])
                .min_by(|&a, &b| tris[a].depth.total_cmp(&tris[b].depth))
                .expect("order incomplete implies something remains")
        });
        done[k] = true;
        order.push(k);
        for &s in &succ[k] {
            let s = s as usize;
            if !done[s] {
                indeg[s] -= 1;
                if indeg[s] == 0 {
                    ready.push((std::cmp::Reverse(ordered::F32(tris[s].depth)), s));
                }
            }
        }
    }
    let sorted: Vec<SceneTri> = order.into_iter().map(|k| tris[k]).collect();
    tris.copy_from_slice(&sorted);
}

/// Total-ordered f32 wrapper so depths can key a heap.
mod ordered {
    #[derive(PartialEq)]
    pub struct F32(pub f32);
    impl Eq for F32 {}
    impl PartialOrd for F32 {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for F32 {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.0.total_cmp(&other.0)
        }
    }
}

/// Draw the textured triangles as one egui mesh sampling `texture_id`
/// (the project's NEAREST atlas texture).
pub fn paint_scene(painter: &egui::Painter, scene: &Scene, texture_id: egui::TextureId) {
    if scene.tris.is_empty() {
        return;
    }
    let mut em = egui::Mesh::with_texture(texture_id);
    for tri in &scene.tris {
        let base = em.vertices.len() as u32;
        let tint = Color32::from_rgb(
            (tri.shade[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (tri.shade[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (tri.shade[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        );
        for i in 0..3 {
            em.vertices.push(egui::epaint::Vertex {
                pos: tri.pts[i],
                uv: tri.uvs[i],
                color: tint,
            });
        }
        em.indices.extend([base, base + 1, base + 2]);
    }
    painter.add(egui::Shape::mesh(em));
}

fn axis_color(dir: [f32; 3]) -> Color32 {
    if dir[0] != 0.0 {
        AXIS_X
    } else if dir[1] != 0.0 {
        AXIS_Y
    } else {
        AXIS_Z
    }
}

/// 1-world-unit grid with highlighted origin axes. In free orbit this is the
/// XZ floor; in a snapped view it lies in the view plane through the origin
/// (Blender-style), so it coincides exactly with the integer grid that
/// vertex/face drags snap to.
pub fn paint_grid(painter: &egui::Painter, cam: &Camera3D, rect: Rect, theme: &Theme) {
    const EXTENT: i32 = 16;
    // (u_dir, v_dir): the two world axes spanning the grid plane.
    let (u_dir, v_dir): ([f32; 3], [f32; 3]) = match cam.snapped() {
        Some(SnapView::Front) | Some(SnapView::Back) => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        Some(SnapView::Left) | Some(SnapView::Right) => ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        // Top/Bottom and free orbit: the XZ floor.
        _ => ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    };
    let grid_stroke = Stroke::new(0.5, theme.muted.gamma_multiply(0.25));
    let u_axis = Stroke::new(1.0, axis_color(u_dir).gamma_multiply(0.6));
    let v_axis = Stroke::new(1.0, axis_color(v_dir).gamma_multiply(0.6));
    let e = EXTENT as f32;
    let at = |u: f32, v: f32| -> [f32; 3] {
        [
            u_dir[0] * u + v_dir[0] * v,
            u_dir[1] * u + v_dir[1] * v,
            u_dir[2] * u + v_dir[2] * v,
        ]
    };
    for i in -EXTENT..=EXTENT {
        let t = i as f32;
        // Lines of constant u run along v_dir, and vice versa.
        let (a, _) = cam.project(at(t, -e), rect);
        let (b, _) = cam.project(at(t, e), rect);
        let (c, _) = cam.project(at(-e, t), rect);
        let (d, _) = cam.project(at(e, t), rect);
        if i == 0 {
            painter.line_segment([a, b], v_axis);
            painter.line_segment([c, d], u_axis);
        } else {
            painter.line_segment([a, b], grid_stroke);
            painter.line_segment([c, d], grid_stroke);
        }
    }
}

/// Is the screen point at `depth` hidden behind a strictly nearer triangle?
/// Uses interpolated per-corner depths so slanted occluders test exactly.
pub fn point_occluded(scene: &Scene, p: Pos2, depth: f32) -> bool {
    const EPS: f32 = 0.05;
    for tri in &scene.tris {
        let [t0, t1, t2] = tri.pts;
        let v0 = t1 - t0;
        let v1 = t2 - t0;
        let v2 = p - t0;
        let denom = v0.x * v1.y - v1.x * v0.y;
        if denom.abs() < 1e-6 {
            continue;
        }
        let b1 = (v2.x * v1.y - v1.x * v2.y) / denom;
        let b2 = (v0.x * v2.y - v2.x * v0.y) / denom;
        let b0 = 1.0 - b1 - b2;
        // Strict interior: boundary points (the edge's own faces) don't count.
        if b0 <= 0.01 || b1 <= 0.01 || b2 <= 0.01 {
            continue;
        }
        let tri_depth = b0 * tri.depths[0] + b1 * tri.depths[1] + b2 * tri.depths[2];
        if tri_depth > depth + EPS {
            return true;
        }
    }
    false
}

/// Is the screen point covered by any front-facing triangle of `faces`?
fn covered_by(scene: &Scene, faces: &std::collections::HashSet<u32>, p: Pos2) -> bool {
    for tri in &scene.tris {
        if !tri.front || !faces.contains(&tri.face) {
            continue;
        }
        let [t0, t1, t2] = tri.pts;
        let (v0, v1, v2) = (t1 - t0, t2 - t0, p - t0);
        let denom = v0.x * v1.y - v1.x * v0.y;
        if denom.abs() < 1e-6 {
            continue;
        }
        let b1 = (v2.x * v1.y - v1.x * v2.y) / denom;
        let b2 = (v0.x * v2.y - v2.x * v0.y) / denom;
        let b0 = 1.0 - b1 - b2;
        if b0 >= 0.0 && b1 >= 0.0 && b2 >= 0.0 {
            return true;
        }
    }
    false
}

/// The outer screen-space outline of `faces`: the boundary between the area
/// they cover and the background, clipped segment-wise to what the viewer can
/// actually see. Returns `(edge id, screen start, screen end)` per sub-segment.
///
/// Candidate edges are the ones bordering exactly one visible selected face —
/// but that set is the boundary of the *visible-face set*, which also fires
/// on interior contours (a recess's rim), and an edge that IS on the group
/// boundary for part of its length may run behind or across other geometry
/// for the rest (a table edge passing behind an object standing on it). So
/// every candidate is sampled along its length and only the runs that are
/// both locally on the outer boundary AND not hidden behind nearer geometry
/// survive. The result hugs the group's outline from any camera, with no
/// stretch ever crossing something in front of it.
pub fn silhouette_edges(
    mesh: &Mesh,
    scene: &Scene,
    cam: &Camera3D,
    rect: Rect,
    faces: &[u32],
) -> Vec<((u32, u32), Pos2, Pos2)> {
    /// How far off the edge to look, in screen pixels. Below a texel at any
    /// usable zoom, and well above projection round-off.
    const PROBE: f32 = 1.0;
    /// Sampling pitch along the edge, in screen pixels.
    const STEP_PX: f32 = 5.0;

    let selected: std::collections::HashSet<u32> = faces.iter().copied().collect();
    let visible: std::collections::HashSet<u32> = scene
        .visible_faces
        .iter()
        .copied()
        .filter(|fi| selected.contains(fi))
        .collect();

    let mut border: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    for &fi in &visible {
        let Some(face) = mesh.faces.get(fi as usize) else { continue };
        let k = face.verts.len();
        for i in 0..k {
            let (a, b) = (face.verts[i], face.verts[(i + 1) % k]);
            *border.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }

    let mut out: Vec<((u32, u32), Pos2, Pos2)> = Vec::new();
    for (&(a, b), &count) in &border {
        if count != 1 {
            continue;
        }
        let (Some(&va), Some(&vb)) =
            (mesh.vertices.get(a as usize), mesh.vertices.get(b as usize))
        else {
            continue;
        };
        let (pa, da) = cam.project(va, rect);
        let (pb, db) = cam.project(vb, rect);
        let dir = pb - pa;
        let len = dir.length();
        if len < 1e-3 {
            continue; // edge-on: nothing drawable
        }
        let normal = egui::Vec2::new(-dir.y / len, dir.x / len);
        let steps = ((len / STEP_PX).ceil() as usize).clamp(3, 96);

        // Walk the edge; a sample survives when at least one side of it is
        // background (locally on the outer boundary) and nothing nearer
        // covers it (locally in front). Contiguous survivors form segments.
        let mut run_start: Option<f32> = None;
        let mut last_kept = 0.0f32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let p = pa + dir * t;
            let depth = da + (db - da) * t;
            let keep = !point_occluded(scene, p, depth)
                && (!covered_by(scene, &visible, p + normal * PROBE)
                    || !covered_by(scene, &visible, p - normal * PROBE));
            match (keep, run_start) {
                (true, None) => {
                    run_start = Some(t);
                    last_kept = t;
                }
                (true, Some(_)) => last_kept = t,
                (false, Some(s)) => {
                    if last_kept > s {
                        out.push(((a, b), pa + dir * s, pa + dir * last_kept));
                    }
                    run_start = None;
                }
                (false, None) => {}
            }
        }
        if let Some(s) = run_start {
            if last_kept > s {
                out.push(((a, b), pa + dir * s, pa + dir * last_kept));
            }
        }
    }
    out.sort_unstable_by_key(|x| x.0);
    out
}

/// Edge overlay: a single uniform thin wireframe over every visible edge —
/// topology (loop cuts, insets) stays visible with every tool, and no edge
/// ever turns bold or black while orbiting. Edges hidden behind nearer
/// geometry are clipped away segment-wise.
pub fn paint_wireframe(
    painter: &egui::Painter,
    mesh: &Mesh,
    scene: &Scene,
    cam: &Camera3D,
    rect: Rect,
    theme: &Theme,
) {
    // Deduplicate edges over the visible faces.
    let mut edges: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for &fi in &scene.visible_faces {
        let face = &mesh.faces[fi as usize];
        let k = face.verts.len();
        for i in 0..k {
            let a = face.verts[i];
            let b = face.verts[(i + 1) % k];
            edges.insert((a.min(b), a.max(b)));
        }
    }
    let stroke = Stroke::new(1.0, theme.muted.gamma_multiply(0.55));
    for &(a, b) in &edges {
        let va = mesh.vertices[a as usize];
        let vb = mesh.vertices[b as usize];
        draw_edge_occlusion_clipped(painter, scene, cam, rect, va, vb, stroke);
    }
}

/// Draw only the visible runs of an edge: sample along its screen length
/// and break the line wherever nearer geometry covers it. Depth varies
/// linearly along a segment under orthographic projection, so the
/// interpolated test is exact.
fn draw_edge_occlusion_clipped(
    painter: &egui::Painter,
    scene: &Scene,
    cam: &Camera3D,
    rect: Rect,
    va: [f32; 3],
    vb: [f32; 3],
    stroke: Stroke,
) {
    let (pa, da) = cam.project(va, rect);
    let (pb, db) = cam.project(vb, rect);
    let len = pa.distance(pb);
    if len < 0.5 {
        return;
    }
    let samples = ((len / 6.0).ceil() as usize).clamp(2, 40);
    let mut run_start: Option<Pos2> = None;
    let mut prev = pa;
    for i in 0..=samples {
        let t = i as f32 / samples as f32;
        let p = Pos2::new(pa.x + (pb.x - pa.x) * t, pa.y + (pb.y - pa.y) * t);
        let d = da + (db - da) * t;
        let visible = !point_occluded(scene, p, d);
        match (visible, run_start) {
            (true, None) => run_start = Some(p),
            (false, Some(start)) => {
                painter.line_segment([start, prev], stroke);
                run_start = None;
            }
            _ => {}
        }
        prev = p;
    }
    if let Some(start) = run_start {
        painter.line_segment([start, pb], stroke);
    }
}

/// Soft fake contact shadow on the floor plane under the model.
/// Skipped in snapped views (the floor disk would be edge-on anyway).
pub fn paint_contact_shadow(painter: &egui::Painter, mesh: &Mesh, cam: &Camera3D, rect: Rect) {
    if cam.snapped().is_some() || mesh.vertices.is_empty() {
        return;
    }
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    let (mut min_z, mut max_z) = (f32::MAX, f32::MIN);
    for v in &mesh.vertices {
        min_x = min_x.min(v[0]);
        max_x = max_x.max(v[0]);
        min_z = min_z.min(v[2]);
        max_z = max_z.max(v[2]);
    }
    let cx = (min_x + max_x) / 2.0;
    let cz = (min_z + max_z) / 2.0;
    let rx = ((max_x - min_x) / 2.0 + 0.8).max(1.0);
    let rz = ((max_z - min_z) / 2.0 + 0.8).max(1.0);
    // Three stacked translucent disks approximate a blurred shadow.
    for (scale, alpha) in [(1.0, 16), (0.78, 20), (0.55, 24)] {
        let pts: Vec<Pos2> = (0..24)
            .map(|i| {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                let world = [cx + a.cos() * rx * scale, 0.0, cz + a.sin() * rz * scale];
                cam.project(world, rect).0
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(
            pts,
            Color32::from_black_alpha(alpha),
            Stroke::NONE,
        ));
    }
}
