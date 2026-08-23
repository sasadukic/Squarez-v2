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

}

/// The default checker material's color at an atlas position.
///
/// Parity comes from the atlas coordinate, not an island's corner, so the
/// pattern stays continuous across faces that abut or share texels in a
/// projected layout — the default material reads as one surface rather than a
/// grid of independently-phased patches.
pub fn default_texel(x: u32, y: u32) -> Rgba {
    if (x + y).is_multiple_of(2) {
        DEFAULT_FACE_A
    } else {
        DEFAULT_FACE_B
    }
}

/// Where a face's texels come from once the projected layout has decided
/// where its island goes.
///
/// Operations declare this per face; they never place islands themselves,
/// because a face's projected position depends on every other face in its
/// block. Splitting it this way keeps each operation's paint semantics —
/// inset crops the parent's centre, a loop cut crops each half — while the
/// layout stays the single owner of placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaintSource {
    /// Carry an existing island across 1:1, clamp-extending its edge colors
    /// if the face grew. `flip_v` mirrors rows, for content authored before
    /// the atlas v-flip.
    Keep { src: Island, flip_v: bool },
    /// Take a sub-rect of an existing island, resampled to fit.
    Crop { src: Island },
    /// A face with no history: fill with the default checker material.
    Checker,
}

impl PaintSource {
    fn keep(src: Island) -> Self {
        PaintSource::Keep { src, flip_v: false }
    }
}

/// Every face keeps the island it currently has.
fn keep_all(mesh: &Mesh) -> Vec<PaintSource> {
    mesh.faces.iter().map(|f| PaintSource::keep(f.island)).collect()
}

/// Layout for a hand-packed mesh: every correctly-sized island stays put;
/// faces that are new or changed size get the first free rectangle found by
/// scanning the atlas (GUTTER from the border and from every kept island —
/// overlap between *hand-placed* islands is the user's own choice and is
/// preserved, but automatic placement never creates one).
fn manual_plan(mesh: &Mesh, atlas: (u32, u32)) -> Result<super::layout::Layout, AtlasFull> {
    use super::mesh::GUTTER;
    let mut islands: Vec<Island> = Vec::with_capacity(mesh.faces.len());
    let mut pending: Vec<usize> = Vec::new();
    for (fi, face) in mesh.faces.iter().enumerate() {
        let (_, _, w, h) = mesh.face_uv_bounds(face);
        if face.island.w == w && face.island.h == h && face.island.w > 0 {
            islands.push(face.island);
        } else {
            islands.push(Island::default());
            pending.push(fi);
        }
    }

    let (aw, ah) = (atlas.0.min(u16::MAX as u32) as u16, atlas.1.min(u16::MAX as u32) as u16);
    for fi in pending {
        let (_, _, w, h) = mesh.face_uv_bounds(&mesh.faces[fi]);
        let fits = |x: u16, y: u16| -> bool {
            if x + w + GUTTER > aw || y + h + GUTTER > ah {
                return false;
            }
            !islands.iter().any(|o| {
                o.w > 0
                    && x < o.x + o.w + GUTTER
                    && o.x < x + w + GUTTER
                    && y < o.y + o.h + GUTTER
                    && o.y < y + h + GUTTER
            })
        };
        let mut found = None;
        'scan: for y in (GUTTER..ah.saturating_sub(h)).step_by(2) {
            for x in (GUTTER..aw.saturating_sub(w)).step_by(2) {
                if fits(x, y) {
                    found = Some(Island { x, y, w, h });
                    break 'scan;
                }
            }
        }
        match found {
            Some(isl) => islands[fi] = isl,
            None => {
                return Err(AtlasFull {
                    need_w: atlas.0,
                    need_h: atlas.1 + (h + 2 * GUTTER) as u32,
                })
            }
        }
    }
    Ok(super::layout::Layout {
        islands,
        cursor: mesh.atlas_cursor,
        overflowed: Vec::new(),
    })
}

/// Shift the selected faces' islands by whole texels — hand-packing. Texels
/// travel with their island; vacated ground not covered by any island
/// afterwards is cleared to transparent. Marks the mesh as hand-packed, which
/// tells every later relayout and the load-time migration to keep its hands
/// off the arrangement.
pub fn move_islands(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    delta: (i32, i32),
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut sel: Vec<u32> = faces
        .iter()
        .copied()
        .filter(|&f| (f as usize) < mesh.faces.len())
        .collect();
    sel.sort_unstable();
    sel.dedup();
    if sel.is_empty() || delta == (0, 0) {
        return Ok(EditOutcome { mesh: mesh.clone(), ..Default::default() });
    }

    let mut out_mesh = mesh.clone();
    out_mesh.manual_layout = true;
    // Every selected island must stay inside the atlas after the shift.
    for &fi in &sel {
        let isl = mesh.faces[fi as usize].island;
        let nx = isl.x as i32 + delta.0;
        let ny = isl.y as i32 + delta.1;
        if nx < 0
            || ny < 0
            || nx as u32 + isl.w as u32 > atlas.0
            || ny as u32 + isl.h as u32 > atlas.1
        {
            return Ok(EditOutcome { mesh: mesh.clone(), ..Default::default() });
        }
        out_mesh.faces[fi as usize].island = Island { x: nx as u16, y: ny as u16, ..isl };
    }

    // Carry the texels: read all sources from the pristine layer first (the
    // move is a permutation), then write, then clear vacated ground that no
    // island covers any more.
    let mut rec = PixelRecorder::new(layer);
    let mut writes: Vec<(u32, u32, Rgba)> = Vec::new();
    for &fi in &sel {
        let src = mesh.faces[fi as usize].island;
        let dst = out_mesh.faces[fi as usize].island;
        // Placeholder-checker islands re-flow instead of copying, so the
        // pattern stays phase-continuous wherever the island is dropped
        // (same rule as relayout).
        let all_default = {
            let mut all = true;
            'scan: for j in 0..src.h {
                for i in 0..src.w {
                    let c = rec.read((src.x + i) as u32, (src.y + j) as u32);
                    if c != DEFAULT_FACE_A && c != DEFAULT_FACE_B {
                        all = false;
                        break 'scan;
                    }
                }
            }
            all
        };
        for j in 0..src.h {
            for i in 0..src.w {
                let (ax, ay) = ((dst.x + i) as u32, (dst.y + j) as u32);
                let c = if all_default {
                    default_texel(ax, ay)
                } else {
                    rec.read((src.x + i) as u32, (src.y + j) as u32)
                };
                writes.push((ax, ay, c));
            }
        }
    }
    let covered = |x: u32, y: u32| -> bool {
        out_mesh.faces.iter().any(|f| {
            let o = f.island;
            o.w > 0
                && x >= o.x as u32
                && y >= o.y as u32
                && x < (o.x + o.w) as u32
                && y < (o.y + o.h) as u32
        })
    };
    for &fi in &sel {
        let src = mesh.faces[fi as usize].island;
        for j in 0..src.h {
            for i in 0..src.w {
                let (x, y) = ((src.x + i) as u32, (src.y + j) as u32);
                if !covered(x, y) {
                    writes.push((x, y, [0, 0, 0, 0]));
                }
            }
        }
    }
    for (x, y, c) in writes {
        rec.write(x, y, c);
    }
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: sel,
        select_verts: Vec::new(),
    })
}

/// One exact quarter turn of `p` about the signed world `axis`
/// (right-handed). Pure component swap/negate — no trig, no error.
fn quarter_rot(p: [f32; 3], axis: [i32; 3]) -> [f32; 3] {
    let [x, y, z] = p;
    match axis {
        [1, 0, 0] => [x, -z, y],
        [-1, 0, 0] => [x, z, -y],
        [0, 1, 0] => [z, y, -x],
        [0, -1, 0] => [-z, y, x],
        [0, 0, 1] => [-y, x, z],
        [0, 0, -1] => [y, -x, z],
        _ => p,
    }
}

/// Rotate the given faces' object by `turns` quarter turns about the signed
/// world `axis`, around the integer-rounded center of their bounding box.
/// Quarter turns map the world lattice to itself exactly (half-integer
/// coordinates included), so four turns are byte-identical to none.
///
/// The texture rides the faces: islands are re-planned for the rotated
/// orientation and every destination texel pulls its color by mapping its
/// world center back through the inverse rotation into the pre-rotation
/// atlas. Unrotated faces reduce to relayout's 1:1 carry under the same rule.
pub fn rotate_faces(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    axis: [i32; 3],
    turns: i32,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut sel: Vec<u32> = faces
        .iter()
        .copied()
        .filter(|&f| (f as usize) < mesh.faces.len())
        .collect();
    sel.sort_unstable();
    sel.dedup();
    let t = turns.rem_euclid(4);
    if sel.is_empty() || t == 0 || axis.iter().map(|a| a.abs()).sum::<i32>() != 1 {
        return Ok(EditOutcome {
            mesh: mesh.clone(),
            select_faces: sel,
            ..Default::default()
        });
    }

    let moved: HashSet<u32> = sel
        .iter()
        .flat_map(|&fi| mesh.faces[fi as usize].verts.iter().copied())
        .collect();
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for &vi in &moved {
        let v = mesh.vertices[vi as usize];
        for a in 0..3 {
            min[a] = min[a].min(v[a]);
            max[a] = max[a].max(v[a]);
        }
    }
    // Center on the half-integer lattice: vertices are multiples of 0.5, so
    // min+max is (near-)integral and the exact bbox center keeps quarter
    // turns lattice-exact AND reproducible call over call (the rotated bbox
    // has the same center, so four separate single turns come home).
    let c = [
        (min[0] + max[0]).round() / 2.0,
        (min[1] + max[1]).round() / 2.0,
        (min[2] + max[2]).round() / 2.0,
    ];
    let rot = |p: [f32; 3], ax: [i32; 3]| -> [f32; 3] {
        let mut q = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        for _ in 0..t {
            q = quarter_rot(q, ax);
        }
        [q[0] + c[0], q[1] + c[1], q[2] + c[2]]
    };
    let inv_axis = [-axis[0], -axis[1], -axis[2]];

    let mut out_mesh = mesh.clone();
    for &vi in &moved {
        let v = out_mesh.vertices[vi as usize];
        out_mesh.vertices[vi as usize] = rot(v, axis);
    }

    // Re-plan islands for the rotated orientation (same selection rule as
    // relayout), assigning before the pixel transfer so texel_quad_world
    // speaks the new layout.
    let plan = if out_mesh.manual_layout {
        manual_plan(&out_mesh, atlas)?
    } else {
        super::layout::plan(&out_mesh, atlas)?
    };
    for (face, island) in out_mesh.faces.iter_mut().zip(plan.islands) {
        face.island = island;
    }
    out_mesh.atlas_cursor = plan.cursor;

    let rotated: HashSet<u32> = sel.iter().copied().collect();
    let mut rec = PixelRecorder::new(layer);
    let mut writes: Vec<(u32, u32, Rgba)> = Vec::new();
    for fi in 0..out_mesh.faces.len() {
        let dst = out_mesh.faces[fi].island;
        if dst.w == 0 || dst.h == 0 {
            continue;
        }
        let old_face = &mesh.faces[fi];
        let src = old_face.island;
        let is_rotated = rotated.contains(&(fi as u32));
        if !is_rotated && src == dst {
            continue;
        }
        // Placeholder-checker islands re-flow at atlas parity instead of
        // being remapped (same material rule as relayout / move_islands).
        let all_default = src.w > 0 && {
            let mut all = true;
            'scan: for j in 0..src.h {
                for i in 0..src.w {
                    let cpx = rec.read((src.x + i) as u32, (src.y + j) as u32);
                    if cpx != DEFAULT_FACE_A && cpx != DEFAULT_FACE_B {
                        all = false;
                        break 'scan;
                    }
                }
            }
            all
        };
        if src.w == 0 || all_default {
            for j in 0..dst.h as u32 {
                for i in 0..dst.w as u32 {
                    let (ax, ay) = (dst.x as u32 + i, dst.y as u32 + j);
                    writes.push((ax, ay, default_texel(ax, ay)));
                }
            }
            continue;
        }
        let uv = mesh.face_uv_map(old_face, 0.0);
        for j in 0..dst.h as u32 {
            for i in 0..dst.w as u32 {
                let (ax, ay) = (dst.x as u32 + i, dst.y as u32 + j);
                let Some(quad) = out_mesh.texel_quad_world(&out_mesh.faces[fi], ax, ay)
                else {
                    continue;
                };
                let mut w = [0.0f32; 3];
                for q in &quad {
                    for a in 0..3 {
                        w[a] += q[a] * 0.25;
                    }
                }
                if is_rotated {
                    w = rot(w, inv_axis);
                }
                let (tx, ty) = uv.texel(w);
                // texel() clamps into the source island, so the floor is
                // always a valid read.
                writes.push((ax, ay, rec.read(tx.floor() as u32, ty.floor() as u32)));
            }
        }
    }
    // Ground vacated by the re-plan that no island covers any more.
    let covered = |x: u32, y: u32| -> bool {
        out_mesh.faces.iter().any(|f| {
            let o = f.island;
            o.w > 0
                && x >= o.x as u32
                && y >= o.y as u32
                && x < (o.x + o.w) as u32
                && y < (o.y + o.h) as u32
        })
    };
    for face in &mesh.faces {
        let src = face.island;
        for j in 0..src.h as u32 {
            for i in 0..src.w as u32 {
                let (x, y) = (src.x as u32 + i, src.y as u32 + j);
                if !covered(x, y) {
                    writes.push((x, y, [0, 0, 0, 0]));
                }
            }
        }
    }
    for (x, y, cpx) in writes {
        rec.write(x, y, cpx);
    }
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: sel,
        select_verts: Vec::new(),
    })
}

/// Decide where every island goes, then carry the texels named by `sources`
/// (index-aligned with `mesh.faces`) along.
///
/// Automatic meshes get the projected layout. A hand-packed mesh
/// (`manual_layout`) keeps every island exactly where the user put it; only
/// faces whose island is missing or no longer fits their size get a fresh
/// spot, found by scanning for free space rather than replanning everything.
///
/// Every source texel is read *before* any is written: relayout is a
/// permutation, so a texel about to be overwritten may still be another
/// face's source. Reads go through `PixelRecorder::read`, so writes an
/// operation already recorded are visible.
fn relayout(
    mesh: &mut Mesh,
    rec: &mut PixelRecorder<'_>,
    sources: &[PaintSource],
    atlas: (u32, u32),
) -> Result<(), AtlasFull> {
    let plan = if mesh.manual_layout {
        manual_plan(mesh, atlas)?
    } else {
        super::layout::plan(mesh, atlas)?
    };
    let mut writes: Vec<(u32, u32, Rgba)> = Vec::new();

    for (fi, &dst) in plan.islands.iter().enumerate() {
        let source = sources.get(fi).copied().unwrap_or(PaintSource::Checker);
        let (src, flip_v, resample) = match source {
            // A face that never had an island has nothing to carry.
            PaintSource::Keep { src, .. } | PaintSource::Crop { src }
                if src.w == 0 || src.h == 0 =>
            {
                (Island::default(), false, false)
            }
            PaintSource::Keep { src, flip_v } => (src, flip_v, false),
            PaintSource::Crop { src } => (src, false, true),
            PaintSource::Checker => (Island::default(), false, false),
        };
        // The default checker is a placeholder MATERIAL, not paint: carrying
        // its pixels breaks the pattern (an odd move lands phase-flipped
        // against the atlas-global parity; a grow duplicates the last row via
        // clamp-extend). An island still purely in the two default tones is
        // therefore refilled in place instead of copied — the pattern re-flows
        // continuously wherever the island lands. Painted islands carry 1:1.
        let src_is_default = src.w > 0 && {
            let mut all = true;
            'scan: for j in 0..src.h {
                for i in 0..src.w {
                    let c = rec.read((src.x + i) as u32, (src.y + j) as u32);
                    if c != DEFAULT_FACE_A && c != DEFAULT_FACE_B {
                        all = false;
                        break 'scan;
                    }
                }
            }
            all
        };
        if src.w == 0 || src.h == 0 || src_is_default {
            for j in 0..dst.h {
                for i in 0..dst.w {
                    let (ax, ay) = ((dst.x + i) as u32, (dst.y + j) as u32);
                    writes.push((ax, ay, default_texel(ax, ay)));
                }
            }
            continue;
        }
        // Nothing moved and nothing to mirror: skip the copy entirely, so an
        // unchanged layout costs no pixel edits and no undo bytes.
        if !resample && !flip_v && src == dst {
            continue;
        }
        for j in 0..dst.h {
            for i in 0..dst.w {
                let (si, sj) = if resample {
                    (
                        (i as u32 * src.w as u32 / dst.w.max(1) as u32) as u16,
                        (j as u32 * src.h as u32 / dst.h.max(1) as u32) as u16,
                    )
                } else {
                    (i.min(src.w - 1), j.min(src.h - 1))
                };
                let si = si.min(src.w - 1);
                let sj = sj.min(src.h - 1);
                let sj = if flip_v { src.h - 1 - sj } else { sj };
                let c = rec.read((src.x + si) as u32, (src.y + sj) as u32);
                writes.push(((dst.x + i) as u32, (dst.y + j) as u32, c));
            }
        }
    }

    for (x, y, c) in writes {
        rec.write(x, y, c);
    }
    for (face, island) in mesh.faces.iter_mut().zip(plan.islands) {
        face.island = island;
    }
    mesh.atlas_cursor = plan.cursor;
    Ok(())
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

/// Move `verts` by an integer world-space `delta`. Every face keeps its
/// paint; the layout decides where the islands end up.
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
    let sources = keep_all(&out_mesh);
    let mut rec = PixelRecorder::new(layer);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
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
/// normal — positive pulls out, negative pushes in (recess); the side
/// walls wind correctly in both directions. The cap reuses the original
/// island; side quads get fresh checker islands.
pub fn extrude_faces_n(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    n: i32,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let mut rec = PixelRecorder::new(layer);
    let mut sources = keep_all(&out_mesh);
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
            let side = Face { verts: vec![a, b, b2, a2], island: Island::default() };
            out_mesh.faces.push(side);
            sources.push(PaintSource::Checker);
        }

        // Cap replaces the original face, keeping its paint.
        let cap = Face { verts: dup, island: face.island };
        out_mesh.faces[fi as usize] = cap;
        caps.push(fi);
    }

    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: caps,
        select_verts: Vec::new(),
    })
}

/// Delete faces; orphaned vertices are garbage-collected.
///
/// Deleting shrinks the blocks the removed faces belonged to, so the survivors
/// have to be laid out again — which is why this now needs the layer and can
/// report a full atlas, unlike the pure topology operation it used to be.
pub fn delete_faces(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
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
    let sources = keep_all(&out_mesh);
    let mut rec = PixelRecorder::new(layer);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome { mesh: out_mesh, pixel_edits: rec.edits, ..Default::default() })
}

/// Delete vertices along with every face that uses them.
pub fn delete_vertices(
    mesh: &Mesh,
    layer: &Layer,
    verts: &[u32],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let doomed: HashSet<u32> = verts.iter().copied().collect();
    let mut out_mesh = mesh.clone();
    out_mesh.faces.retain(|f| !f.verts.iter().any(|vi| doomed.contains(vi)));
    // Doomed-but-still-referenced can't happen after the retain; plain GC
    // also drops the now-unreferenced doomed vertices.
    gc_vertices(&mut out_mesh);
    let sources = keep_all(&out_mesh);
    let mut rec = PixelRecorder::new(layer);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome { mesh: out_mesh, pixel_edits: rec.edits, ..Default::default() })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    Cube,
    Plane,
    Sphere,
    Cylinder,
}

/// A parameterized add-primitive request from the toolbar flyout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddPrimitive {
    pub kind: Primitive,
    pub sides: u32,
    pub size: u32,
}

impl AddPrimitive {
    /// Build the primitive mesh (islands unallocated).
    pub fn build(self) -> Mesh {
        let size = self.size.clamp(1, 64);
        match self.kind {
            Primitive::Cube => Mesh::cube(size),
            Primitive::Plane => Mesh::plane(size),
            Primitive::Sphere => Mesh::sphere_n(self.sides, size),
            Primitive::Cylinder => Mesh::cylinder_n(self.sides, size),
        }
    }
}

/// Merge a new default-sized primitive into the mesh.
pub fn add_primitive(
    mesh: &Mesh,
    layer: &Layer,
    kind: Primitive,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    add_object(mesh, layer, &AddPrimitive { kind, sides: 8, size: 8 }.build(), atlas)
}

/// Merge a prebuilt object mesh into the scene, stacked above existing
/// geometry, with fresh checker islands for every new face.
/// Duplicate the selected faces as a new object, lifted clear above the
/// scene (the add_object convention), carrying their painted texture. A
/// selection spanning several models copies them as one unit, relative
/// positions preserved. The copies come back selected so a Move drag can
/// place them immediately.
pub fn duplicate_faces(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut sel: Vec<u32> = faces
        .iter()
        .copied()
        .filter(|&f| (f as usize) < mesh.faces.len())
        .collect();
    sel.sort_unstable();
    sel.dedup();
    if sel.is_empty() {
        return Ok(EditOutcome { mesh: mesh.clone(), ..Default::default() });
    }

    let mut out_mesh = mesh.clone();
    let scene_top = out_mesh.vertices.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil();
    let sel_bottom = sel
        .iter()
        .flat_map(|&fi| mesh.faces[fi as usize].verts.iter())
        .map(|&vi| mesh.vertices[vi as usize][1])
        .fold(f32::MAX, f32::min);
    let lift = scene_top + 2.0 - sel_bottom;

    // One shifted copy per vertex the selection uses, shared across its faces.
    let mut map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut sources = keep_all(&out_mesh);
    let mut new_faces = Vec::new();
    for &fi in &sel {
        let face = mesh.faces[fi as usize].clone();
        let verts = face
            .verts
            .iter()
            .map(|&vi| {
                *map.entry(vi).or_insert_with(|| {
                    let v = mesh.vertices[vi as usize];
                    out_mesh.vertices.push([v[0], v[1] + lift, v[2]]);
                    out_mesh.vertices.len() as u32 - 1
                })
            })
            .collect();
        new_faces.push(out_mesh.faces.len() as u32);
        out_mesh.faces.push(Face { verts, island: Island::default() });
        // The copy wears the original's paint.
        sources.push(PaintSource::keep(face.island));
    }

    let mut rec = PixelRecorder::new(layer);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: new_faces,
        select_verts: Vec::new(),
    })
}

pub fn add_object(
    mesh: &Mesh,
    layer: &Layer,
    prim: &Mesh,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
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
    let mut sources = keep_all(&out_mesh);
    let mut new_faces = Vec::new();
    for face in &prim.faces {
        let f = Face {
            verts: face.verts.iter().map(|vi| vi + base).collect(),
            island: Island::default(),
        };
        new_faces.push(out_mesh.faces.len() as u32);
        out_mesh.faces.push(f);
        sources.push(PaintSource::Checker);
    }
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
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
    basis.unproject(u, v, original[basis.dropped_axis()])
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
    let mut sources = keep_all(&out_mesh);
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
            let border = Face {
                verts: vec![face.verts[i], face.verts[(i + 1) % k], ring[(i + 1) % k], ring[i]],
                island: Island::default(),
            };
            out_mesh.faces.push(border);
            sources.push(PaintSource::Checker);
        }

        // Center face replaces the original, keeping its painted center.
        let center = Face { verts: ring, island: Island::default() };
        let (_, _, cw, ch) = out_mesh.face_uv_bounds(&center);
        let old = face.island;
        let src = Island {
            x: old.x + d.min(old.w as u32 - 1) as u16,
            y: old.y + d.min(old.h as u32 - 1) as u16,
            w: cw.min(old.w),
            h: ch.min(old.h),
        };
        out_mesh.faces[fi as usize] = center;
        sources[fi as usize] = PaintSource::Crop { src };
        centers.push(fi);
    }

    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
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
    let mut sources = keep_all(&out_mesh);
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

        // Each half takes the crop of the parent island that sat under it.
        let make_half = |m: &Mesh, verts: Vec<u32>| -> (Face, PaintSource) {
            let half = Face { verts, island: Island::default() };
            let (hmin_u, hmin_v, hw, hh) = m.face_uv_bounds(&half);
            let du = (hmin_u - face_min_u).max(0.0).round() as u16;
            let dv = (hmin_v - face_min_v).max(0.0).round() as u16;
            let src = Island {
                x: old.x + du.min(old.w.saturating_sub(1)),
                y: old.y + dv.min(old.h.saturating_sub(1)),
                w: hw.min(old.w),
                h: hh.min(old.h),
            };
            (half, PaintSource::Crop { src })
        };

        let (half_a, src_a) = make_half(&out_mesh, vec![a, p, q, d]);
        let (half_b, src_b) = make_half(&out_mesh, vec![p, b, c, q]);
        out_mesh.faces[st.face as usize] = half_a;
        sources[st.face as usize] = src_a;
        out_mesh.faces.push(half_b);
        sources.push(src_b);
    }

    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: Vec::new(),
        select_verts: Vec::new(),
    })
}

/// Create a face from 3–4 selected vertices (any click order — the ring is
/// sorted by angle around the shared plane). Winding is auto-flipped to
/// face away from the mesh centroid; the face gets a fresh checker island.
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

    // Sort the vertices into a perimeter ring so any click order works
    // (a zig-zag order would otherwise make a degenerate bowtie).
    let pts: Vec<[f32; 3]> = verts.iter().map(|&vi| out_mesh.vertices[vi as usize]).collect();
    let cross = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    };
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let norm2 = |a: [f32; 3]| dot(a, a);
    // Plane normal from any non-collinear triple.
    let mut plane_n = cross(sub(pts[1], pts[0]), sub(pts[2], pts[0]));
    if norm2(plane_n) < 1e-6 && pts.len() == 4 {
        plane_n = cross(sub(pts[1], pts[0]), sub(pts[3], pts[0]));
    }
    if norm2(plane_n) < 1e-6 {
        return Ok(EditOutcome { mesh: out_mesh, ..Default::default() }); // collinear
    }
    let k = pts.len() as f32;
    let ring_center = pts.iter().fold([0.0f32; 3], |acc, p| {
        [acc[0] + p[0] / k, acc[1] + p[1] / k, acc[2] + p[2] / k]
    });
    // In-plane basis for angular sorting.
    let u_axis = sub(pts[0], ring_center);
    let v_axis = cross(plane_n, u_axis);
    let mut ordered: Vec<u32> = verts.to_vec();
    ordered.sort_by(|&a, &b| {
        let ang = |vi: u32| {
            let d = sub(out_mesh.vertices[vi as usize], ring_center);
            dot(d, v_axis).atan2(dot(d, u_axis))
        };
        ang(a).total_cmp(&ang(b))
    });

    let mut face = Face { verts: ordered, island: Island::default() };
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

    let mut rec = PixelRecorder::new(layer);
    let mut sources = keep_all(&out_mesh);
    out_mesh.faces.push(face);
    sources.push(PaintSource::Checker);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    let new_idx = out_mesh.faces.len() as u32 - 1;

    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: vec![new_idx],
        select_verts: Vec::new(),
    })
}

/// All faces connected to `seed_face` through shared vertices — one "object"
/// in a multi-object mesh.
pub fn connected_faces(mesh: &Mesh, seed_face: u32) -> Vec<u32> {
    if seed_face as usize >= mesh.faces.len() {
        return Vec::new();
    }
    let mut in_component_verts: HashSet<u32> =
        mesh.faces[seed_face as usize].verts.iter().copied().collect();
    let mut component: HashSet<u32> = HashSet::from([seed_face]);
    // Fixed-point iteration: tiny meshes, simplicity over asymptotics.
    loop {
        let mut grew = false;
        for (fi, face) in mesh.faces.iter().enumerate() {
            if component.contains(&(fi as u32)) {
                continue;
            }
            if face.verts.iter().any(|v| in_component_verts.contains(v)) {
                component.insert(fi as u32);
                in_component_verts.extend(face.verts.iter().copied());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let mut faces: Vec<u32> = component.into_iter().collect();
    faces.sort_unstable();
    faces
}

/// Uniformly scale a vertex set so its largest bounding-box dimension
/// changes by `k` whole units, anchored at the bbox min corner; every
/// vertex is rounded back onto the grid. Islands of affected faces re-fit
/// with 1:1 anchored copies.
pub fn scale_verts(
    mesh: &Mesh,
    layer: &Layer,
    verts: &[u32],
    k: i32,
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let moved: HashSet<u32> = verts.iter().copied().collect();
    if k == 0 || moved.is_empty() {
        return Ok(EditOutcome { mesh: out_mesh, select_verts: verts.to_vec(), ..Default::default() });
    }
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for &vi in &moved {
        let Some(v) = out_mesh.vertices.get(vi as usize) else { continue };
        for a in 0..3 {
            min[a] = min[a].min(v[a]);
            max[a] = max[a].max(v[a]);
        }
    }
    let base = (0..3).map(|a| max[a] - min[a]).fold(0.0f32, f32::max);
    if base < 1.0 {
        return Ok(EditOutcome { mesh: out_mesh, select_verts: verts.to_vec(), ..Default::default() });
    }
    let target = (base + k as f32).max(1.0);
    let factor = target / base;
    for &vi in &moved {
        if let Some(v) = out_mesh.vertices.get_mut(vi as usize) {
            for a in 0..3 {
                v[a] = min[a] + ((v[a] - min[a]) * factor).round();
            }
        }
    }
    let sources = keep_all(&out_mesh);
    let mut rec = PixelRecorder::new(layer);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome {
        mesh: out_mesh,
        pixel_edits: rec.edits,
        select_faces: Vec::new(),
        select_verts: verts.to_vec(),
    })
}

/// Whether a mesh's islands still need to be moved to their projected
/// positions — i.e. the file was written by the old shelf packer.
///
/// This replaces the old gutter-spacing check. That check asserted every pair
/// of islands sat at least GUTTER apart, which a projected layout violates by
/// design: coplanar neighbours abut so their texture stays continuous.
pub fn islands_need_repack(mesh: &Mesh, atlas: (u32, u32)) -> bool {
    !mesh.manual_layout && !super::layout::is_canonical(mesh, atlas)
}

/// Move every island to its projected position, carrying each face's paint.
///
/// This is the load-time migration for models laid out by the old shelf
/// packer. It is lossless: island *sizes* are a pure function of the mesh and
/// are unchanged by the projected layout, so the move is an exact permutation
/// of identically-sized rects.
///
/// `mirror_v` handles content authored before the atlas v-flip. Post-flip
/// `min_v = -max_y` while the height is identical, so a face's rows are simply
/// reversed — an exact, invertible mirror, applied only to the two bases whose
/// v maps to world Y.
pub fn relayout_existing(
    mesh: &Mesh,
    layer: &Layer,
    atlas: (u32, u32),
    mirror_v: bool,
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let sources: Vec<PaintSource> = out_mesh
        .faces
        .iter()
        .map(|f| {
            let flip = mirror_v
                && out_mesh.face_plane_basis(f) != super::mesh::PlaneBasis::Xz;
            PaintSource::Keep { src: f.island, flip_v: flip }
        })
        .collect();
    let mut rec = PixelRecorder::new(layer);
    relayout(&mut out_mesh, &mut rec, &sources, atlas)?;
    Ok(EditOutcome { mesh: out_mesh, pixel_edits: rec.edits, ..Default::default() })
}
