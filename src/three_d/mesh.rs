// src/three_d/mesh.rs
//
// Core 3D data model: a minimal low-poly mesh whose faces each own a
// rectangular "island" in the project's texture atlas (the canvas).
// Texel density is fixed at 1 texel per world grid unit.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Gap kept around every island so NEAREST sampling never bleeds
/// into a neighboring face's texels.
pub const GUTTER: u16 = 1;
/// Hard cap on one island side, in texels.
pub const MAX_ISLAND_SIDE: u16 = 256;

/// A face's rectangle in the texture atlas, in texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Island {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Append-only shelf-packing state. `x`/`y` is the next free position,
/// `row_h` the height of the current shelf row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AtlasCursor {
    pub x: u16,
    pub y: u16,
    pub row_h: u16,
}

/// 3 or 4 vertex indices, counter-clockwise when viewed from outside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub verts: Vec<u32>,
    pub island: Island,
}

/// Returned when the shelf packer cannot place an island in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasFull;

/// The 2D projection basis used for both island sizing and painting:
/// which world axes map to the island's local u/v.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneBasis {
    /// Face dominated by ±Y normal: u = X, v = Z.
    Xz,
    /// Face dominated by ±X normal: u = Z, v = Y.
    Zy,
    /// Face dominated by ±Z normal: u = X, v = Y.
    Xy,
}

impl PlaneBasis {
    /// Project a world position into this basis's (u, v) plane.
    pub fn project(self, p: [f32; 3]) -> (f32, f32) {
        match self {
            PlaneBasis::Xz => (p[0], p[2]),
            PlaneBasis::Zy => (p[2], p[1]),
            PlaneBasis::Xy => (p[0], p[1]),
        }
    }
}

/// A low-poly mesh. Vertices are grid-snapped (whole numbers stored as f32),
/// Y-up, right-handed. Meshes are tiny (tens to low hundreds of faces) —
/// clone freely; undo snapshots the whole thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Mesh {
    pub vertices: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
    pub atlas_cursor: AtlasCursor,
}

impl Mesh {
    /// Axis-aligned cube of `size` world units, sitting on the floor plane
    /// (y in 0..size), centered on the origin in X/Z. Islands are NOT yet
    /// allocated — call `allocate_all_islands` with the atlas dimensions.
    pub fn cube(size: u32) -> Self {
        let s = size as f32;
        let h = s / 2.0;
        let vertices = vec![
            [-h, 0.0, -h], // 0: bottom ring
            [h, 0.0, -h],  // 1
            [h, 0.0, h],   // 2
            [-h, 0.0, h],  // 3
            [-h, s, -h],   // 4: top ring
            [h, s, -h],    // 5
            [h, s, h],     // 6
            [-h, s, h],    // 7
        ];
        let quads: [[u32; 4]; 6] = [
            [0, 1, 2, 3], // bottom (-Y)
            [4, 7, 6, 5], // top (+Y)
            [3, 2, 6, 7], // front (+Z)
            [1, 0, 4, 5], // back (-Z)
            [2, 1, 5, 6], // right (+X)
            [0, 3, 7, 4], // left (-X)
        ];
        Mesh {
            vertices,
            faces: quads
                .iter()
                .map(|q| Face { verts: q.to_vec(), island: Island::default() })
                .collect(),
            atlas_cursor: AtlasCursor::default(),
        }
    }

    /// A single horizontal quad of `size` world units at y = 0, centered on
    /// the origin, normal +Y. Islands not yet allocated.
    pub fn plane(size: u32) -> Self {
        let h = size as f32 / 2.0;
        Mesh {
            vertices: vec![[-h, 0.0, -h], [h, 0.0, -h], [h, 0.0, h], [-h, 0.0, h]],
            faces: vec![Face { verts: vec![0, 3, 2, 1], island: Island::default() }],
            atlas_cursor: AtlasCursor::default(),
        }
    }

    /// Integer-coordinate pixel octagon of "radius" r in the XZ plane at
    /// height `y`, ordered by increasing angle.
    fn octagon_ring(vertices: &mut Vec<[f32; 3]>, r: f32, y: f32) -> Vec<u32> {
        let a = r / 2.0;
        let ring = [
            (r, a),
            (a, r),
            (-a, r),
            (-r, a),
            (-r, -a),
            (-a, -r),
            (a, -r),
            (r, -a),
        ];
        ring.iter()
            .map(|&(x, z)| {
                vertices.push([x, y, z]);
                vertices.len() as u32 - 1
            })
            .collect()
    }

    /// Side quads between two octagon rings (lower ring first), outward.
    fn band(faces: &mut Vec<Face>, lo: &[u32], hi: &[u32]) {
        for i in 0..8 {
            let j = (i + 1) % 8;
            faces.push(Face {
                verts: vec![lo[j], lo[i], hi[i], hi[j]],
                island: Island::default(),
            });
        }
    }

    /// Close an octagon ring with 3 quads (trapezoid + rectangle +
    /// trapezoid). `top` selects the winding (+Y cap vs -Y cap).
    fn cap(faces: &mut Vec<Face>, ring: &[u32], top: bool) {
        let quads: [[usize; 4]; 3] = [[0, 1, 2, 3], [7, 0, 3, 4], [4, 5, 6, 7]];
        for q in quads {
            let mut verts: Vec<u32> = q.iter().map(|&i| ring[i]).collect();
            if top {
                verts.reverse();
            }
            faces.push(Face { verts, island: Island::default() });
        }
    }

    /// Octagonal prism of `size` world units (integer coordinates): 8 side
    /// quads plus 3-quad caps top and bottom. Islands not yet allocated.
    pub fn cylinder(size: u32) -> Self {
        let r = size as f32 / 2.0;
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        let bottom = Self::octagon_ring(&mut vertices, r, 0.0);
        let top = Self::octagon_ring(&mut vertices, r, size as f32);
        Self::band(&mut faces, &bottom, &top);
        Self::cap(&mut faces, &bottom, false);
        Self::cap(&mut faces, &top, true);
        Mesh { vertices, faces, atlas_cursor: AtlasCursor::default() }
    }

    /// Chunky pixel sphere of `size` world units: stacked integer octagon
    /// rings (r/2, r, r, r/2 profile) with 3-quad caps. Islands not yet
    /// allocated.
    pub fn sphere(size: u32) -> Self {
        let s = size as f32;
        let r = s / 2.0;
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        let r0 = Self::octagon_ring(&mut vertices, r / 2.0, 0.0);
        let r1 = Self::octagon_ring(&mut vertices, r, s / 4.0);
        let r2 = Self::octagon_ring(&mut vertices, r, s - s / 4.0);
        let r3 = Self::octagon_ring(&mut vertices, r / 2.0, s);
        Self::band(&mut faces, &r0, &r1);
        Self::band(&mut faces, &r1, &r2);
        Self::band(&mut faces, &r2, &r3);
        Self::cap(&mut faces, &r0, false);
        Self::cap(&mut faces, &r3, true);
        Mesh { vertices, faces, atlas_cursor: AtlasCursor::default() }
    }

    /// Unnormalized face normal (Newell's method — robust for any planar
    /// polygon and tolerant of mildly non-planar quads).
    pub fn face_normal(&self, face: &Face) -> [f32; 3] {
        let mut n = [0.0f32; 3];
        let k = face.verts.len();
        for i in 0..k {
            let a = self.vertices[face.verts[i] as usize];
            let b = self.vertices[face.verts[(i + 1) % k] as usize];
            n[0] += (a[1] - b[1]) * (a[2] + b[2]);
            n[1] += (a[2] - b[2]) * (a[0] + b[0]);
            n[2] += (a[0] - b[0]) * (a[1] + b[1]);
        }
        n
    }

    /// The deterministic 2D basis for a face, chosen by its dominant normal
    /// axis. Used by island sizing, UV mapping, and painting alike.
    pub fn face_plane_basis(&self, face: &Face) -> PlaneBasis {
        let n = self.face_normal(face);
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        if ay >= ax && ay >= az {
            PlaneBasis::Xz
        } else if ax >= az {
            PlaneBasis::Zy
        } else {
            PlaneBasis::Xy
        }
    }

    /// The face's bounding box in its plane basis, as (min_u, min_v, w, h)
    /// with w/h in whole texels (ceil, min 1, clamped to MAX_ISLAND_SIDE).
    pub fn face_uv_bounds(&self, face: &Face) -> (f32, f32, u16, u16) {
        let basis = self.face_plane_basis(face);
        let mut min_u = f32::MAX;
        let mut max_u = f32::MIN;
        let mut min_v = f32::MAX;
        let mut max_v = f32::MIN;
        for &vi in &face.verts {
            let (u, v) = basis.project(self.vertices[vi as usize]);
            min_u = min_u.min(u);
            max_u = max_u.max(u);
            min_v = min_v.min(v);
            max_v = max_v.max(v);
        }
        let w = ((max_u - min_u).ceil() as i64).clamp(1, MAX_ISLAND_SIDE as i64) as u16;
        let h = ((max_v - min_v).ceil() as i64).clamp(1, MAX_ISLAND_SIDE as i64) as u16;
        (min_u, min_v, w, h)
    }

    /// Allocate one island via the shelf packer. Append-only: freed islands
    /// are never reused (a repack can rebuild the whole atlas if needed).
    pub fn alloc_island(&mut self, w: u16, h: u16, atlas: (u32, u32)) -> Result<Island, AtlasFull> {
        let aw = atlas.0.min(u16::MAX as u32) as u16;
        let ah = atlas.1.min(u16::MAX as u32) as u16;
        let w = w.clamp(1, MAX_ISLAND_SIDE);
        let h = h.clamp(1, MAX_ISLAND_SIDE);
        if GUTTER + w + GUTTER > aw {
            return Err(AtlasFull);
        }
        let mut x = self.atlas_cursor.x.max(GUTTER);
        let mut y = self.atlas_cursor.y.max(GUTTER);
        let mut row_h = self.atlas_cursor.row_h;
        if x + w + GUTTER > aw {
            x = GUTTER;
            y += row_h + GUTTER;
            row_h = 0;
        }
        if y + h + GUTTER > ah {
            return Err(AtlasFull);
        }
        let island = Island { x, y, w, h };
        self.atlas_cursor = AtlasCursor { x: x + w + GUTTER, y, row_h: row_h.max(h) };
        Ok(island)
    }

    /// Allocate islands for every face that doesn't have one yet
    /// (island of zero size = unallocated).
    pub fn allocate_all_islands(&mut self, atlas: (u32, u32)) -> Result<(), AtlasFull> {
        for i in 0..self.faces.len() {
            if self.faces[i].island.w == 0 || self.faces[i].island.h == 0 {
                let (_, _, w, h) = self.face_uv_bounds(&self.faces[i]);
                self.faces[i].island = self.alloc_island(w, h, atlas)?;
            }
        }
        Ok(())
    }

    /// Undirected edge set derived from faces (sorted index pairs).
    pub fn derive_edges(&self) -> HashSet<(u32, u32)> {
        let mut edges = HashSet::new();
        for face in &self.faces {
            let k = face.verts.len();
            for i in 0..k {
                let a = face.verts[i];
                let b = face.verts[(i + 1) % k];
                edges.insert((a.min(b), a.max(b)));
            }
        }
        edges
    }

    /// Debug invariant check: every face has 3 or 4 in-range vertex indices.
    pub fn validate(&self) -> Result<(), String> {
        for (i, face) in self.faces.iter().enumerate() {
            if face.verts.len() < 3 || face.verts.len() > 4 {
                return Err(format!("face {} has {} vertices", i, face.verts.len()));
            }
            for &vi in &face.verts {
                if vi as usize >= self.vertices.len() {
                    return Err(format!("face {} references missing vertex {}", i, vi));
                }
            }
        }
        Ok(())
    }
}
