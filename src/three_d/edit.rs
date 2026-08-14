// src/three_d/edit.rs
//
// Pure modeling operations. Each takes the current mesh (and layer, when
// texture islands are involved) and returns the resulting mesh plus the
// pixel writes that must accompany it — the caller applies both and pushes
// one Command::MeshEdit so mesh and texture stay atomic through undo.

use std::collections::HashSet;

use super::mesh::{AtlasFull, Face, Island, Mesh};
use crate::project::{Layer, Rgba};

/// Default color painted into freshly allocated islands so new faces are
/// visible immediately (same gray as project creation).
pub const NEW_FACE_COLOR: Rgba = [128, 128, 128, 255];

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

    /// Fill an island rect with a solid color.
    fn fill_island(&mut self, isl: Island, color: Rgba) {
        for y in isl.y..isl.y + isl.h {
            for x in isl.x..isl.x + isl.w {
                self.write(x as u32, y as u32, color);
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

/// Extrude each selected face 1 unit along the dominant axis of its normal.
/// The cap reuses the original island; side quads get fresh gray islands.
pub fn extrude_faces(
    mesh: &Mesh,
    layer: &Layer,
    faces: &[u32],
    atlas: (u32, u32),
) -> Result<EditOutcome, AtlasFull> {
    let mut out_mesh = mesh.clone();
    let mut rec = PixelRecorder::new(layer);
    let mut caps: Vec<u32> = Vec::new();
    // Process highest index first so earlier removals don't shift later ones.
    let mut order: Vec<u32> = faces.to_vec();
    order.sort_unstable_by(|a, b| b.cmp(a));

    for &fi in &order {
        if fi as usize >= out_mesh.faces.len() {
            continue;
        }
        let face = out_mesh.faces[fi as usize].clone();
        let n = out_mesh.face_normal(&face);
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        let dir: [f32; 3] = if ay >= ax && ay >= az {
            [0.0, n[1].signum(), 0.0]
        } else if ax >= az {
            [n[0].signum(), 0.0, 0.0]
        } else {
            [0.0, 0.0, n[2].signum()]
        };

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
            rec.fill_island(isl, NEW_FACE_COLOR);
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
        rec.fill_island(isl, NEW_FACE_COLOR);
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
