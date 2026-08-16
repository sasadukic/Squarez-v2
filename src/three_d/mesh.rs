// src/three_d/mesh.rs
//
// Core 3D data model: a minimal low-poly mesh whose faces each own a
// rectangular "island" in the project's texture atlas (the canvas).
// Texel density is fixed at 1 texel per world grid unit.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Gap kept around every island so NEAREST sampling never bleeds
/// into a neighboring face's texels. Two texels wide so each island owns
/// a 1-texel padding ring (edge colors are dilated into it at texture
/// upload, hiding sampling seams at face boundaries).
pub const GUTTER: u16 = 2;
/// Hard cap on one island side, in texels.
pub const MAX_ISLAND_SIDE: u16 = 256;

/// How far inside its island a face's corner UVs are pulled, in texels.
///
/// In a projected layout neighbouring islands sit flush against each other,
/// with no gutter to absorb error. A corner UV that lands exactly on the
/// island's outer boundary is one interpolation rounding away from flooring
/// onto the *neighbour's* first texel — which reads a different face's paint
/// wherever the artwork has a hard color boundary at the face edge, i.e.
/// exactly where face edges usually are.
///
/// Clamping the corners is enough: `build_scene` evaluates UVs only at
/// vertices and the rasterizer interpolates linearly, so the whole face's
/// span compresses into `[INSET, w - INSET]`. At 1/64 texel that displaces an
/// interior texel boundary by far less than a screen pixel at any usable zoom,
/// while sitting far above f32 interpolation error.
pub const UV_INSET: f32 = 1.0 / 64.0;

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

/// Returned when an island cannot be placed in the atlas, carrying the atlas
/// dimensions that would have been sufficient so the caller can grow the right
/// axis. Never serialized, so it is free to gain fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasFull {
    pub need_w: u32,
    pub need_h: u32,
}

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

/// One face's world → atlas-texel mapping, the single definition shared by
/// the renderer and the OBJ writer.
///
/// `inset` pulls the result away from the island's outer border. Islands may
/// sit flush against each other in a projected layout, so without it an
/// interpolated coordinate that drifts a hair past the boundary can floor onto
/// a *neighbouring* island's texel. The renderer passes `UV_INSET`; the OBJ
/// writer must pass `0.0`, because `io::obj` only accepts UVs that land on
/// exact texel boundaries when reconstructing islands on import.
#[derive(Debug, Clone, Copy)]
pub struct FaceUvMap {
    basis: PlaneBasis,
    min_u: f32,
    min_v: f32,
    island: Island,
    inset: f32,
}

impl FaceUvMap {
    /// Atlas texel coordinates for a world position on the face.
    pub fn texel(&self, p: [f32; 3]) -> (f32, f32) {
        let (u, v) = self.basis.project(p);
        let span = |extent: u16, d: f32| -> f32 {
            let hi = (extent as f32 - self.inset).max(self.inset);
            d.clamp(self.inset, hi)
        };
        (
            self.island.x as f32 + span(self.island.w, u - self.min_u),
            self.island.y as f32 + span(self.island.h, v - self.min_v),
        )
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

    /// Regular n-gon "pixel ring" of radius r in the XZ plane at height
    /// `y`: vertices at half-step angle offsets, rounded onto the integer
    /// grid, ordered by increasing angle. (For n = 8, r = 4 this rounds
    /// exactly onto the classic pixel octagon.)
    fn poly_ring(vertices: &mut Vec<[f32; 3]>, n: u32, r: f32, y: f32) -> Vec<u32> {
        let n = n.max(3);
        (0..n)
            .map(|k| {
                let angle = (k as f32 + 0.5) * std::f32::consts::TAU / n as f32;
                let x = (r * angle.cos()).round();
                let z = (r * angle.sin()).round();
                vertices.push([x, y, z]);
                vertices.len() as u32 - 1
            })
            .collect()
    }

    /// How many sides a ring of radius `r` supports before grid rounding
    /// collapses neighboring vertices (spacing under ~1 unit).
    pub fn max_sides_for_radius(r: f32) -> u32 {
        if r < 0.75 {
            return 4;
        }
        let per_side = (1.0 / (2.0 * r)).min(1.0).asin();
        ((std::f32::consts::PI / per_side).floor() as u32).clamp(4, 24)
    }

    /// Side quads between two same-length rings (lower ring first), outward.
    fn band(faces: &mut Vec<Face>, lo: &[u32], hi: &[u32]) {
        let n = lo.len();
        for i in 0..n {
            let j = (i + 1) % n;
            faces.push(Face {
                verts: vec![lo[j], lo[i], hi[i], hi[j]],
                island: Island::default(),
            });
        }
    }

    /// Close a ring with a triangle fan. `top` selects the winding
    /// (+Y cap vs -Y cap).
    fn cap(faces: &mut Vec<Face>, ring: &[u32], top: bool) {
        for i in 1..ring.len() - 1 {
            let mut verts = vec![ring[0], ring[i], ring[i + 1]];
            if top {
                verts.reverse();
            }
            faces.push(Face { verts, island: Island::default() });
        }
    }

    /// n-sided prism of `size` diameter/height (integer coordinates):
    /// side quads plus fan caps. Sides are clamped to what the grid
    /// supports at this radius. Islands not yet allocated.
    pub fn cylinder_n(sides: u32, size: u32) -> Self {
        let r = size as f32 / 2.0;
        let n = sides.clamp(3, Self::max_sides_for_radius(r));
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        let bottom = Self::poly_ring(&mut vertices, n, r, 0.0);
        let top = Self::poly_ring(&mut vertices, n, r, size as f32);
        Self::band(&mut faces, &bottom, &top);
        Self::cap(&mut faces, &bottom, false);
        Self::cap(&mut faces, &top, true);
        Mesh { vertices, faces, atlas_cursor: AtlasCursor::default() }
    }

    /// 8-sided default cylinder.
    pub fn cylinder(size: u32) -> Self {
        Self::cylinder_n(8, size)
    }

    /// Chunky pixel sphere of `size` diameter: stacked pixel rings
    /// (r/2, r, r, r/2 profile) with fan caps. Sides clamped by the
    /// smallest (pole) ring so rounding never collapses vertices.
    pub fn sphere_n(sides: u32, size: u32) -> Self {
        let s = size as f32;
        let r = s / 2.0;
        let n = sides.clamp(3, Self::max_sides_for_radius(r / 2.0));
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        let r0 = Self::poly_ring(&mut vertices, n, r / 2.0, 0.0);
        let r1 = Self::poly_ring(&mut vertices, n, r, (s / 4.0).round());
        let r2 = Self::poly_ring(&mut vertices, n, r, (s - s / 4.0).round());
        let r3 = Self::poly_ring(&mut vertices, n, r / 2.0, s);
        Self::band(&mut faces, &r0, &r1);
        Self::band(&mut faces, &r1, &r2);
        Self::band(&mut faces, &r2, &r3);
        Self::cap(&mut faces, &r0, false);
        Self::cap(&mut faces, &r3, true);
        Mesh { vertices, faces, atlas_cursor: AtlasCursor::default() }
    }

    /// 8-sided default sphere.
    pub fn sphere(size: u32) -> Self {
        Self::sphere_n(8, size)
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

    /// The face's world → atlas-texel mapping, with the per-face terms
    /// (basis, bounds, island) resolved once so it can be applied per vertex.
    pub fn face_uv_map(&self, face: &Face, inset: f32) -> FaceUvMap {
        let (min_u, min_v, _, _) = self.face_uv_bounds(face);
        FaceUvMap {
            basis: self.face_plane_basis(face),
            min_u,
            min_v,
            island: face.island,
            inset,
        }
    }

    /// Allocate one island via the shelf packer. Append-only: freed islands
    /// are never reused (a repack can rebuild the whole atlas if needed).
    pub fn alloc_island(&mut self, w: u16, h: u16, atlas: (u32, u32)) -> Result<Island, AtlasFull> {
        let aw = atlas.0.min(u16::MAX as u32) as u16;
        let ah = atlas.1.min(u16::MAX as u32) as u16;
        let w = w.clamp(1, MAX_ISLAND_SIDE);
        let h = h.clamp(1, MAX_ISLAND_SIDE);
        if GUTTER + w + GUTTER > aw {
            return Err(AtlasFull {
                need_w: (GUTTER + w + GUTTER) as u32,
                need_h: atlas.1,
            });
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
            return Err(AtlasFull {
                need_w: atlas.0,
                need_h: (y as u32) + (h + GUTTER) as u32,
            });
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

    /// World-space quad corners of a single atlas texel on `face` — used to
    /// preview exactly which texel a paint stroke would hit. Returns None if
    /// the texel lies outside the face's island.
    pub fn texel_quad_world(&self, face: &Face, tx: u32, ty: u32) -> Option<[[f32; 3]; 4]> {
        let isl = face.island;
        if tx < isl.x as u32
            || ty < isl.y as u32
            || tx >= (isl.x + isl.w) as u32
            || ty >= (isl.y + isl.h) as u32
        {
            return None;
        }
        let basis = self.face_plane_basis(face);
        let (min_u, min_v, _, _) = self.face_uv_bounds(face);
        let u0 = min_u + (tx - isl.x as u32) as f32;
        let v0 = min_v + (ty - isl.y as u32) as f32;
        // Solve the face's plane for the coordinate the basis drops, so the
        // quad lies exactly on slanted faces too.
        let n = self.face_normal(face);
        let p0 = *self.vertices.get(*face.verts.first()? as usize)?;
        let d = n[0] * p0[0] + n[1] * p0[1] + n[2] * p0[2];
        let point = |u: f32, v: f32| -> [f32; 3] {
            match basis {
                PlaneBasis::Xz => {
                    let y = if n[1].abs() > 1e-6 { (d - n[0] * u - n[2] * v) / n[1] } else { p0[1] };
                    [u, y, v]
                }
                PlaneBasis::Zy => {
                    let x = if n[0].abs() > 1e-6 { (d - n[2] * u - n[1] * v) / n[0] } else { p0[0] };
                    [x, v, u]
                }
                PlaneBasis::Xy => {
                    let z = if n[2].abs() > 1e-6 { (d - n[0] * u - n[1] * v) / n[2] } else { p0[2] };
                    [u, v, z]
                }
            }
        };
        Some([
            point(u0, v0),
            point(u0 + 1.0, v0),
            point(u0 + 1.0, v0 + 1.0),
            point(u0, v0 + 1.0),
        ])
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
