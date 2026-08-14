// src/three_d/edit.rs
//
// Pure modeling operations. Each takes the current mesh (and layer, when
// texture islands are involved) and returns the resulting mesh plus the
// pixel writes that must accompany it — the caller applies both and pushes
// one Command::MeshEdit so mesh and texture stay atomic through undo.

use std::collections::HashSet;

use super::mesh::{AtlasFull, Face, Island, Mesh};
use crate::project::{Layer, Rgba};

/// Checkerboard tones painted into freshly allocated islands so new faces
/// match the default prototype material from project creation.
pub use super::{DEFAULT_FACE_A, DEFAULT_FACE_B};

pub type PixelEdit = (u32, u32, Rgba, Rgba);

#[derive(Debug, Clone, Default)]
pub struct EditOutcome {
    pub mesh: Mesh,
    pub pixel_edits: Vec<PixelEdit>,
    /// Faces to select after the operation (e.g. extruded caps).
    pub select_faces: Vec<u32>,
    /// Vertices to select after the operation.
    pub select_verts: Vec<u32>,
}

/// Record a pixel write against `layer` state, tracking the pre-edit value.
/// Edits are collected but NOT applied to the layer — pure operation.
struct PixelRecorder<'a> {
    layer: &'a Layer,
    edits: Vec<PixelEdit>,
    /// Values already written this operation (so later reads see them).
    written: std::collections::HashMap<(u32, u32), Rgba>,
}

impl<'a> PixelRecorder<'a> {
    fn new(layer: &'a Layer) -> Self {
        Self { layer, edits: Vec::new(), written: std::collections::HashMap::new() }
    }

    fn read(&self, x: u32, y: u32) -> Rgba {
        *self.written.get(&(x, y)).unwrap_or(&self.layer.get_pixel(x, y))
    }

    fn write(&mut self, x: u32, y: u32, new: Rgba) {
        let old = self.read(x, y);
        if old != new {
            self.edits.push((x, y, old, new));
            self.written.insert((x, y), new);
        }
    }

    /// Fill an island rect with the default checkerboard material
    /// (island-local parity, matching project creation).
    fn fill_island_default(&mut self, isl: Island) {
        for y in 0..isl.h {
            for x in 0..isl.w {
                let c = if (x + y) % 2 == 0 { DEFAULT_FACE_A } else { DEFAULT_FACE_B };
                self.write((isl.x + x) as u32, (isl.y + y) as u32, c);
            }
        }
    }

    /// Nearest-neighbor blit from `src` island to `dst` island.
    fn blit_island(&mut self, src: Island, dst: Island) {
        for j in 0..dst.h as u32 {
            for i in 0..dst.w as u32 {
                let sx = src.x as u32 + (i * src.w as u32) / dst.w as u32;
                let sy = src.y as u32 + (j * src.h as u32) / dst.h as u32;
                let c = self.layer.get_pixel(sx, sy);
                self.write(dst.x as u32 + i, dst.y as u32 + j, c);
            }
        }
    }
}

/// Drop vertices not referenced by any face, remapping face indices.
/// Returns the surviving-vertex mapping old→new alongside the mesh.
fn gc_vertices(mesh: &mut Mesh) -> Vec<Option<u32>> {
    let mut used = vec![false; mesh.vertices.len()];
    for face in &mesh.faces {
        for &vi in &face.verts {
            used[vi as usize] = true;
        }
    }
    let mut map: Vec<Option<u32>> = vec![None; mesh.vertices.len()];
    let mut next = 0u32;
    for (i, &u) in used.iter().enumerate() {
        if u {
            map[i] = Some(next);
            next += 1;
        }
    }
    mesh.vertices = mesh
        .vertices
        .iter()
        .enumerate()
        .filter(|(i, _)| used[*i])
        .map(|(_, v)| *v)
        .collect();
    for face in &mut mesh.faces {
        for vi in &mut face.verts {
            *vi = map[*vi as usize].expect("used vertex must survive GC");
        }
    }
    map
}

/// Re-allocate the island of any face whose required size changed, blitting
/// the old pixels across. Mutates `mesh` islands; records writes in `rec`.
fn refresh_islands(
    mesh: &mut Mesh,
    rec: &mut PixelRecorder<'_>,
    faces: &HashSet<u32>,
    atlas: (u32, u32),
) -> Result<(), AtlasFull> {
    for &fi in faces {
        let Some(face) = mesh.faces.get(fi as usize) else { continue };
        let (_, _, w, h) = mesh.face_uv_bounds(face);
        let old = face.island;
        if old.w == w && old.h == h {
            continue;
        }
        let new_isl = mesh.alloc_island(w, h, atlas)?;
        rec.blit_island(old, new_isl);
        mesh.faces[fi as usize].island = new_isl;
    }
    Ok(())
}

/// Move `verts` by an integer world-space `delta`. Islands of affected faces
/// are resized (with blit) when their footprint changes.
pub fn move_vertices(
    mesh: &Mesh,
    layer: &Layer,
    verts: &[u32],
    delta: [i32; 3],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let moved: HashSet<u32> = verts.iter().copied().collect();
    for &vi in &moved {
        if let Some(v) = out_mesh.vertices.get_mut(vi as usize) {
            v[0] += delta[0] as f32;
            v[1] += delta[1] as f32;
            v[2] += delta[2] as f32;
        }
    }
    let affected: HashSet<u32> = out_mesh
        .faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.verts.iter().any(|vi| moved.contains(vi)))
        .map(|(i, _)| i as u32)
        .collect();
    let mut rec = PixelRecorder::new(layer);
    refresh_islands(&mut out_mesh, &mut rec, &affected, atlas)?;
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: Vec::new(),
        select_verts: verts.to_vec(),
    })
}

/// Dominant-axis unit direction of a face's outward normal.
pub fn extrude_dir(mesh: &Mesh, face: &Face) -> [f32; 3] {
    let n = mesh.face_normal(face);
    let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
    if ay >= ax && ay >= az {
        [0.0, n[1].signum(), 0.0]
    } else if ax >= az {
        [n[0].signum(), 0.0, 0.0]
    } else {
        [0.0, 0.0, n[2].signum()]
    }
}

/// Extrude each selected face 1 unit along the dominant axis of its normal.
pub fn extrude_faces(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    extrude_faces_n(mesh, layer, faces, 1, atlas)
}

/// Extrude each selected face `n` units along the dominant axis of its
/// normal. The cap reuses the original island; side quads get fresh
/// checker islands.
pub fn extrude_faces_n(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    n: u32,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let mut rec = PixelRecorder::new(layer);
    let mut caps: Vec<u32> = Vec::new();
    if n == 0 {
        return Ok(EditOutcome { mesh: out_mesh, ..Default::default() });
    }
    // Process highest index first so earlier removals don't shift later ones.
    let mut order: Vec<u32> = faces.to_vec();
    order.sort_unstable_by(|a, b| b.cmp(a));

    for &fi in &order {
        if fi as usize >= out_mesh.faces.len() {
            continue;
        }
        let face = out_mesh.faces[fi as usize].clone();
        let d = extrude_dir(&out_mesh, &face);
        let dir = [d[0] * n as f32, d[1] * n as f32, d[2] * n as f32];

        // Duplicate the face's vertices, offset along the extrusion axis.
        let mut dup: Vec<u32> = Vec::with_capacity(face.verts.len());
        for &vi in &face.verts {
            let v = out_mesh.vertices[vi as usize];
            out_mesh.vertices.push([v[0] + dir[0], v[1] + dir[1], v[2] + dir[2]]);
            dup.push(out_mesh.vertices.len() as u32 - 1);
        }

        // Side quads: [a, b, b', a'] keeps outward winding.
        let k = face.verts.len();
        for i in 0..k {
            let a = face.verts[i];
            let b = face.verts[(i + 1) % k];
            let a2 = dup[i];
            let b2 = dup[(i + 1) % k];
            let mut side = Face { verts: vec![a, b, b2, a2], island: Island::default() };
            let (_, _, w, h) = out_mesh.face_uv_bounds(&side);
            let isl = out_mesh.alloc_island(w, h, atlas)?;
            rec.fill_island_default(isl);
            side.island = isl;
            out_mesh.faces.push(side);
        }

        // Cap replaces the original face, reusing its island.
        let cap = Face { verts: dup, island: face.island };
        out_mesh.faces[fi as usize] = cap;
        caps.push(fi);
    }

    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: caps,
        select_verts: Vec::new(),
    })
}

/// Delete faces; orphaned vertices are garbage-collected.
pub fn delete_faces(mesh: &Mesh, faces: &[u32]) -> EditOutcome {
    let doomed: HashSet<u32> = faces.iter().copied().collect();
    let mut out_mesh = mesh.clone();
    out_mesh.faces = out_mesh
        .faces
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !doomed.contains(&(*i as u32)))
        .map(|(_, f)| f)
        .collect();
    gc_vertices(&mut out_mesh);
    EditOutcome { mesh: out_mesh, ..Default::default() }
}

/// Delete vertices along with every face that uses them.
pub fn delete_vertices(mesh: &Mesh, verts: &[u32]) -> EditOutcome {
    let doomed: HashSet<u32> = verts.iter().copied().collect();
    let mut out_mesh = mesh.clone();
    out_mesh.faces.retain(|f| !f.verts.iter().any(|vi| doomed.contains(vi)));
    // Doomed-but-still-referenced can't happen after the retain; plain GC
    // also drops the now-unreferenced doomed vertices.
    gc_vertices(&mut out_mesh);
    EditOutcome { mesh: out_mesh, ..Default::default() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    Cube,
    Plane,
}

/// Merge a new primitive into the mesh, stacked above existing geometry.
pub fn add_primitive(
    mesh: &Mesh,
    layer: &Layer,
    kind: Primitive,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let prim = match kind {
        Primitive::Cube => Mesh::cube(8),
        Primitive::Plane => Mesh::plane(8),
    };
    let mut out_mesh = mesh.clone();
    let lift = if out_mesh.vertices.is_empty() {
        0.0
    } else {
        out_mesh.vertices.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil() + 2.0
    };
    let base = out_mesh.vertices.len() as u32;
    for v in &prim.vertices {
        out_mesh.vertices.push([v[0], v[1] + lift, v[2]]);
    }
    let mut rec = PixelRecorder::new(layer);
    let mut new_faces = Vec::new();
    for face in &prim.faces {
        let mut f = Face {
            verts: face.verts.iter().map(|vi| vi + base).collect(),
            island: Island::default(),
        };
        let (_, _, w, h) = out_mesh.face_uv_bounds(&f);
        let isl = out_mesh.alloc_island(w, h, atlas)?;
        rec.fill_island_default(isl);
        f.island = isl;
        new_faces.push(out_mesh.faces.len() as u32);
        out_mesh.faces.push(f);
    }
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: new_faces,
        select_verts: Vec::new(),
    })
}

/// Map (u, v) plane-basis coordinates back into a 3D point, carrying the
/// off-plane coordinate from `original`.
fn basis_unproject(basis: super::mesh::PlaneBasis, original: [f32; 3], u: f32, v: f32) -> [f32; 3] {
    use super::mesh::PlaneBasis;
    match basis {
        PlaneBasis::Xz => [u, original[1], v],
        PlaneBasis::Zy => [original[0], v, u],
        PlaneBasis::Xy => [u, v, original[2]],
    }
}

/// Inset each selected quad face by `d` units: the face shrinks to a center
/// face (keeping its index and its painted texture, blitted from the old
/// island's center crop) surrounded by four fresh border quads.
/// Non-quads and faces too small for the inset are skipped.
pub fn inset_faces(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    d: u32,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let mut rec = PixelRecorder::new(layer);
    let mut centers: Vec<u32> = Vec::new();
    if d == 0 {
        return Ok(EditOutcome { mesh: out_mesh, ..Default::default() });
    }

    for &fi in faces {
        if fi as usize >= out_mesh.faces.len() {
            continue;
        }
        let face = out_mesh.faces[fi as usize].clone();
        if face.verts.len() != 4 {
            continue;
        }
        let basis = out_mesh.face_plane_basis(&face);
        let (min_u, min_v, w, h) = out_mesh.face_uv_bounds(&face);
        if 2 * d >= w.min(h) as u32 {
            continue; // inset would collapse the face
        }
        let (max_u, max_v) = (min_u + w as f32, min_v + h as f32);
        let (cu, cv) = ((min_u + max_u) / 2.0, (min_v + max_v) / 2.0);

        // Inset ring: each corner moves d toward the bbox center per axis.
        let df = d as f32;
        let mut ring: Vec<u32> = Vec::with_capacity(4);
        for &vi in &face.verts {
            let p = out_mesh.vertices[vi as usize];
            let (u, v) = basis.project(p);
            let u2 = if u <= cu { (u + df).min(cu) } else { (u - df).max(cu) };
            let v2 = if v <= cv { (v + df).min(cv) } else { (v - df).max(cv) };
            out_mesh.vertices.push(basis_unproject(basis, p, u2, v2));
            ring.push(out_mesh.vertices.len() as u32 - 1);
        }

        // Border quads: [outer_i, outer_next, inner_next, inner_i].
        let k = face.verts.len();
        for i in 0..k {
            let mut border = Face {
                verts: vec![face.verts[i], face.verts[(i + 1) % k], ring[(i + 1) % k], ring[i]],
                island: Island::default(),
            };
            let (_, _, bw, bh) = out_mesh.face_uv_bounds(&border);
            let isl = out_mesh.alloc_island(bw, bh, atlas)?;
            rec.fill_island_default(isl);
            border.island = isl;
            out_mesh.faces.push(border);
        }

        // Center face replaces the original, keeping its painted center.
        let mut center = Face { verts: ring, island: Island::default() };
        let (_, _, cw, ch) = out_mesh.face_uv_bounds(&center);
        let isl = out_mesh.alloc_island(cw, ch, atlas)?;
        let old = face.island;
        let src = Island {
            x: old.x + d.min(old.w as u32 - 1) as u16,
            y: old.y + d.min(old.h as u32 - 1) as u16,
            w: cw.min(old.w),
            h: ch.min(old.h),
        };
        rec.blit_island(src, isl);
        center.island = isl;
        out_mesh.faces[fi as usize] = center;
        centers.push(fi);
    }

    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: centers,
        select_verts: Vec::new(),
    })
}

// ── Loop cut ─────────────────────────────────────────────────────────────────

/// One face the loop passes through: the entry edge is
/// `(verts[entry_pos], verts[entry_pos + 1])` and the cut crosses it at
/// fraction `s` from the entry edge's first vertex.
#[derive(Debug, Clone, Copy)]
pub struct LoopStep {
    pub face: u32,
    pub entry_pos: usize,
    pub s: f32,
}

#[derive(Debug, Clone)]
pub struct LoopPlan {
    pub steps: Vec<LoopStep>,
    /// World-space cut segments (entry point → exit point), one per step —
    /// used to draw the preview polyline.
    pub segments: Vec<([f32; 3], [f32; 3])>,
}

fn lerp3(a: [f32; 3], b: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s, a[2] + (b[2] - a[2]) * s]
}

fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The other face (besides `exclude`) containing undirected edge {a, b},
/// with the position of that edge within it.
fn face_with_edge(mesh: &Mesh, exclude: u32, a: u32, b: u32) -> Option<(u32, usize)> {
    for (fi, face) in mesh.faces.iter().enumerate() {
        if fi as u32 == exclude {
            continue;
        }
        let k = face.verts.len();
        for i in 0..k {
            let x = face.verts[i];
            let y = face.verts[(i + 1) % k];
            if (x == a && y == b) || (x == b && y == a) {
                return Some((fi as u32, i));
            }
        }
    }
    None
}

fn walk_loop(
    mesh: &Mesh,
    mut face: u32,
    mut entry_pos: usize,
    mut s: f32,
    visited: &mut HashSet<u32>,
) -> Vec<LoopStep> {
    let mut steps = Vec::new();
    loop {
        if !visited.insert(face) {
            break; // closed the loop
        }
        let f = &mesh.faces[face as usize];
        if f.verts.len() != 4 {
            break; // strips are quads only
        }
        steps.push(LoopStep { face, entry_pos, s });
        let c = f.verts[(entry_pos + 2) % 4];
        let d = f.verts[(entry_pos + 3) % 4];
        // Exit point on the opposite edge {c, d}, running d → c to match
        // the entry direction.
        let exit = lerp3(mesh.vertices[d as usize], mesh.vertices[c as usize], s);
        let Some((nf, npos)) = face_with_edge(mesh, face, c, d) else { break };
        let f2 = &mesh.faces[nf as usize];
        if f2.verts.len() != 4 {
            break;
        }
        let na = mesh.vertices[f2.verts[npos] as usize];
        let nb = mesh.vertices[f2.verts[(npos + 1) % 4] as usize];
        let len = dist3(na, nb).max(1e-6);
        face = nf;
        entry_pos = npos;
        s = (dist3(exit, na) / len).clamp(0.0, 1.0);
    }
    steps
}

/// Plan a loop cut entering `face` through the edge at `entry_pos`, at
/// fraction `s` (0 < s < 1) along it. Walks the quad strip in both
/// directions until it closes or hits a boundary.
pub fn plan_loop(mesh: &Mesh, face: u32, entry_pos: usize, s: f32) -> Option<LoopPlan> {
    if face as usize >= mesh.faces.len() || mesh.faces[face as usize].verts.len() != 4 {
        return None;
    }
    if !(0.001..=0.999).contains(&s) {
        return None;
    }
    let mut visited = HashSet::new();
    let mut steps = walk_loop(mesh, face, entry_pos, s, &mut visited);

    // If the walk hit a boundary, extend backward across the entry edge.
    if !visited.contains(&face) || steps.is_empty() {
        return None;
    }
    let closed = {
        // walk_loop stops on revisit; if the last step's opposite edge leads
        // back to `face`, the loop closed.
        let last = steps.last().unwrap();
        let f = &mesh.faces[last.face as usize];
        let c = f.verts[(last.entry_pos + 2) % 4];
        let d = f.verts[(last.entry_pos + 3) % 4];
        face_with_edge(mesh, last.face, c, d).is_some_and(|(nf, _)| nf == face)
    };
    if !closed {
        let f = &mesh.faces[face as usize];
        let a = f.verts[entry_pos];
        let b = f.verts[(entry_pos + 1) % 4];
        if let Some((bf, bpos)) = face_with_edge(mesh, face, a, b) {
            // Entry point on the shared edge, re-parametrized for bf.
            let entry_pt = lerp3(
                mesh.vertices[a as usize],
                mesh.vertices[b as usize],
                s,
            );
            let na = mesh.vertices[mesh.faces[bf as usize].verts[bpos] as usize];
            let nb = mesh.vertices
                [mesh.faces[bf as usize].verts[(bpos + 1) % 4] as usize];
            let len = dist3(na, nb).max(1e-6);
            let s2 = (dist3(entry_pt, na) / len).clamp(0.0, 1.0);
            let mut back = walk_loop(mesh, bf, bpos, s2, &mut visited);
            back.reverse();
            back.extend(steps);
            steps = back;
        }
    }

    let segments = steps
        .iter()
        .map(|st| {
            let f = &mesh.faces[st.face as usize];
            let a = mesh.vertices[f.verts[st.entry_pos] as usize];
            let b = mesh.vertices[f.verts[(st.entry_pos + 1) % 4] as usize];
            let c = mesh.vertices[f.verts[(st.entry_pos + 2) % 4] as usize];
            let d = mesh.vertices[f.verts[(st.entry_pos + 3) % 4] as usize];
            (lerp3(a, b, st.s), lerp3(d, c, st.s))
        })
        .collect();
    Some(LoopPlan { steps, segments })
}

/// Apply a planned loop cut: every quad in the strip splits in two, cut
/// vertices are shared between neighboring faces, and each half inherits
/// the matching crop of its face's painted island.
pub fn loop_cut(
    mesh: &Mesh,
    layer: &Layer,
    plan: &LoopPlan,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let mut rec = PixelRecorder::new(layer);
    // One cut vertex per crossed edge, shared by both adjacent faces.
    let mut cut_verts: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    let mut get_cut = |m: &mut Mesh, a: u32, b: u32, pt: [f32; 3]| -> u32 {
        let key = (a.min(b), a.max(b));
        *cut_verts.entry(key).or_insert_with(|| {
            m.vertices.push(pt);
            m.vertices.len() as u32 - 1
        })
    };

    for (st, seg) in plan.steps.iter().zip(plan.segments.iter()) {
        let face = out_mesh.faces[st.face as usize].clone();
        let a = face.verts[st.entry_pos];
        let b = face.verts[(st.entry_pos + 1) % 4];
        let c = face.verts[(st.entry_pos + 2) % 4];
        let d = face.verts[(st.entry_pos + 3) % 4];
        let p = get_cut(&mut out_mesh, a, b, seg.0);
        let q = get_cut(&mut out_mesh, c, d, seg.1);

        let (face_min_u, face_min_v, _, _) = out_mesh.face_uv_bounds(&face);
        let old = face.island;

        let make_half = |m: &mut Mesh,
                             rec: &mut PixelRecorder<'_>,
                             verts: Vec<u32>|
         -> Result<Face, AtlasFull> {
            let mut half = Face { verts, island: Island::default() };
            let (hmin_u, hmin_v, hw, hh) = m.face_uv_bounds(&half);
            let isl = m.alloc_island(hw, hh, atlas)?;
            let du = (hmin_u - face_min_u).max(0.0).round() as u16;
            let dv = (hmin_v - face_min_v).max(0.0).round() as u16;
            let src = Island {
                x: old.x + du.min(old.w.saturating_sub(1)),
                y: old.y + dv.min(old.h.saturating_sub(1)),
                w: hw.min(old.w),
                h: hh.min(old.h),
            };
            rec.blit_island(src, isl);
            half.island = isl;
            Ok(half)
        };

        let half_a = make_half(&mut out_mesh, &mut rec, vec![a, p, q, d])?;
        let half_b = make_half(&mut out_mesh, &mut rec, vec![p, b, c, q])?;
        out_mesh.faces[st.face as usize] = half_a;
        out_mesh.faces.push(half_b);
    }

    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: Vec::new(),
        select_verts: Vec::new(),
    })
}

/// Create a face from 3–4 selected vertices (click order). Winding is
/// auto-flipped to face away from the mesh centroid; the face gets a fresh
/// checker island.
pub fn create_face(
    mesh: &Mesh,
    layer: &Layer,
    verts: &[u32],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let unique: HashSet<u32> = verts.iter().copied().collect();
    if !(3..=4).contains(&verts.len())
        || unique.len() != verts.len()
        || verts.iter().any(|&v| v as usize >= out_mesh.vertices.len())
    {
        return Ok(EditOutcome { mesh: out_mesh, ..Default::default() });
    }

    let mut face = Face { verts: verts.to_vec(), island: Island::default() };
    let n = out_mesh.face_normal(&face);
    if (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]) < 1e-6 {
        return Ok(EditOutcome { mesh: out_mesh, ..Default::default() }); // degenerate
    }
    // Orient outward: away from the mesh centroid.
    let len = out_mesh.vertices.len() as f32;
    let centroid = out_mesh.vertices.iter().fold([0.0f32; 3], |acc, v| {
        [acc[0] + v[0] / len, acc[1] + v[1] / len, acc[2] + v[2] / len]
    });
    let k = face.verts.len() as f32;
    let center = face.verts.iter().fold([0.0f32; 3], |acc, &vi| {
        let v = out_mesh.vertices[vi as usize];
        [acc[0] + v[0] / k, acc[1] + v[1] / k, acc[2] + v[2] / k]
    });
    let outward = [center[0] - centroid[0], center[1] - centroid[1], center[2] - centroid[2]];
    if n[0] * outward[0] + n[1] * outward[1] + n[2] * outward[2] < 0.0 {
        face.verts.reverse();
    }

    let (_, _, w, h) = out_mesh.face_uv_bounds(&face);
    let isl = out_mesh.alloc_island(w, h, atlas)?;
    let mut rec = PixelRecorder::new(layer);
    rec.fill_island_default(isl);
    face.island = isl;
    out_mesh.faces.push(face);
    let new_idx = out_mesh.faces.len() as u32 - 1;

    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: vec![new_idx],
        select_verts: Vec::new(),
    })
}
