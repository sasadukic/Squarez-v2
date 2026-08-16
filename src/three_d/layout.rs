// src/three_d/layout.rs
//
// Projected atlas layout: every face's island sits where the face actually
// projects, so the atlas — which is the project's canvas — reads as an
// orthographic blueprint of the model instead of a scatter of shelf-packed
// rectangles.
//
// Faces are grouped into blocks keyed by (connected component, plane basis,
// normal sign): six views per object — top, bottom, front, back, right, left.
// Splitting by sign is not cosmetic: a cube's top and bottom faces project to
// the *identical* rect in the XZ plane. Splitting by component matters because
// `edit::add_object` stacks each new primitive above the last, so without it
// every object's top face would land on the same footprint.
//
// Inside a block a face's island keeps its projected offset from the block's
// origin, which makes the atlas mapping continuous across the whole block:
// `render` computes `tx = isl.x + (u - min_u)`, so holding
// `isl.x - block_x == min_u - block_origin_u` means every face in the block
// maps a given world point to the same atlas texel. Two consequences:
//
//   * Coplanar neighbours tile exactly, with no gutter between them — safe
//     because `mesh::UV_INSET` keeps every fragment inside its own island
//     regardless of what is packed next door.
//   * Coplanar faces may *share* texels. Islands are bounding rectangles, so
//     a fan-triangulated cap's triangles always overlap even though the
//     triangles themselves tile without gaps. That is not corruption: both
//     faces address the shared region identically, so it is one continuous
//     piece of surface with one piece of texture.
//
// Only faces at different depths contest a slot, and the loser is spilled to
// an overflow shelf.
//
// Blocks are never mirrored: the -Z block is the front view of the back faces
// (as if seen through the model) rather than a true back view, and the +/-X
// blocks are not flipped relative to true side views. That is deliberate —
// front/back and left/right stay coordinate-aligned, so painting a matched
// pair needs no mental mirroring.

use super::mesh::{shelf_place, AtlasCursor, AtlasFull, Island, Mesh, PlaneBasis};

/// One of the six projection views a face can belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockKey {
    pub basis: PlaneBasis,
    /// Sign of the face normal along `basis.dropped_axis()`.
    pub positive: bool,
}

/// Fixed emission order — top, bottom, front, back, right, left. Blocks are
/// always visited through this array, never through a map, so a layout is a
/// deterministic function of its inputs.
pub const BLOCK_ORDER: [BlockKey; 6] = [
    BlockKey { basis: PlaneBasis::Xz, positive: true },  // top    (+Y)
    BlockKey { basis: PlaneBasis::Xz, positive: false }, // bottom (-Y)
    BlockKey { basis: PlaneBasis::Xy, positive: true },  // front  (+Z)
    BlockKey { basis: PlaneBasis::Xy, positive: false }, // back   (-Z)
    BlockKey { basis: PlaneBasis::Zy, positive: true },  // right  (+X)
    BlockKey { basis: PlaneBasis::Zy, positive: false }, // left   (-X)
];

/// Frozen world-space origins for blocks that have already been laid out.
///
/// Without it, growing a model in -u or -v moves a block's bounding-box
/// minimum, which shifts *every* island in that block and rewrites all of
/// their texels — on every edit, into every undo entry. With it, a block keeps
/// the origin it was first laid out at and only re-origins when a face would
/// otherwise land at a negative offset.
///
/// Runtime state only: it is deliberately **not** part of `Mesh`. `.sqr` is
/// bincode, which is positional and non-self-describing, so any new field on a
/// serialized 3D type would break every existing file and force a format
/// version bump. Keeping the layout a pure function of `(mesh, atlas)` is what
/// makes this whole change format-compatible.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutAnchor {
    origins: Vec<(u32, BlockKey, f32, f32)>,
}

impl LayoutAnchor {
    fn get(&self, comp: u32, key: BlockKey) -> Option<(f32, f32)> {
        self.origins
            .iter()
            .find(|(c, k, _, _)| *c == comp && *k == key)
            .map(|(_, _, u, v)| (*u, *v))
    }

    fn set(&mut self, comp: u32, key: BlockKey, u: f32, v: f32) {
        self.origins.push((comp, key, u, v));
    }
}

/// A planned layout: one island per face, index-aligned with `mesh.faces`.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub islands: Vec<Island>,
    /// Shelf state after packing, for `Mesh::atlas_cursor`.
    pub cursor: AtlasCursor,
    /// Origins actually used — feed back into the next `plan` call.
    pub anchor: LayoutAnchor,
    /// Faces that could not keep their projected slot because another face in
    /// the same block already covered it, and were shelf-packed instead.
    pub overflowed: Vec<u32>,
}

/// A face awaiting placement.
#[derive(Debug, Clone)]
struct Slot {
    face: u32,
    min_u: f32,
    min_v: f32,
    w: u16,
    h: u16,
    /// Position along the block's dropped axis; used to give the frontmost
    /// surface first claim on a contested projected slot.
    depth: f32,
    /// Unit normal and plane offset, for the coplanarity test.
    normal: [f32; 3],
    offset: f32,
    /// The face outline projected into the block's plane.
    poly: Vec<(f32, f32)>,
    lx: u16,
    ly: u16,
}

impl Slot {
    /// Whether two faces lie in the same plane. Coplanar faces address a
    /// shared texel identically, so they may overlap freely — it is one
    /// continuous piece of surface wearing one piece of texture.
    fn coplanar_with(&self, other: &Slot) -> bool {
        let dot = self.normal[0] * other.normal[0]
            + self.normal[1] * other.normal[1]
            + self.normal[2] * other.normal[2];
        dot > 0.9999 && (self.offset - other.offset).abs() < 1e-3
    }

    fn rect(&self) -> (u16, u16, u16, u16) {
        (self.lx, self.ly, self.w, self.h)
    }
}

fn rects_overlap(a: (u16, u16, u16, u16), b: (u16, u16, u16, u16)) -> bool {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    ax + aw > bx && bx + bw > ax && ay + ah > by && by + bh > ay
}

/// Do two convex polygons overlap in area? Separating-axis test; sharing an
/// edge or a corner does not count as overlapping.
///
/// Bounding rectangles are far too blunt a test here. Every face's island is a
/// bounding rect, so a fan-triangulated cap's triangles — or a sphere band's
/// wedge-shaped quads — always have overlapping rects while their actual
/// footprints tile the plane without touching. Since all faces in a block
/// share one origin, each face only ever samples texels under its own outline,
/// so disjoint outlines mean disjoint texels no matter how the rects sit.
fn polys_overlap(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    const EPS: f32 = 1e-3;
    for poly in [a, b] {
        for i in 0..poly.len() {
            let (x0, y0) = poly[i];
            let (x1, y1) = poly[(i + 1) % poly.len()];
            let (mut nx, mut ny) = (-(y1 - y0), x1 - x0);
            let len = (nx * nx + ny * ny).sqrt();
            if len < 1e-6 {
                continue;
            }
            nx /= len;
            ny /= len;
            let span = |p: &[(f32, f32)]| {
                p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(x, y)| {
                    let d = x * nx + y * ny;
                    (lo.min(d), hi.max(d))
                })
            };
            let (alo, ahi) = span(a);
            let (blo, bhi) = span(b);
            if ahi <= blo + EPS || bhi <= alo + EPS {
                return false;
            }
        }
    }
    true
}

/// Partition faces into connected components over shared vertices, returning
/// a component id per face. Ids are handed out in ascending lowest-face-index
/// order, so the partition is deterministic.
fn components(mesh: &Mesh) -> Vec<u32> {
    let mut comp = vec![u32::MAX; mesh.faces.len()];
    let mut next = 0u32;
    for seed in 0..mesh.faces.len() {
        if comp[seed] != u32::MAX {
            continue;
        }
        for fi in super::edit::connected_faces(mesh, seed as u32) {
            comp[fi as usize] = next;
        }
        next += 1;
    }
    comp
}

/// Which block a face belongs to, how deep it sits along that block's axis,
/// and the plane it lies in.
fn face_block(mesh: &Mesh, fi: usize) -> (BlockKey, f32, [f32; 3], f32) {
    let face = &mesh.faces[fi];
    let basis = mesh.face_plane_basis(face);
    let axis = basis.dropped_axis();
    let n = mesh.face_normal(face);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let unit = if len > 1e-6 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 0.0] };
    let (depth, offset) = match face.verts.first() {
        Some(&v0) => {
            let p0 = mesh.vertices[v0 as usize];
            let depth = face
                .verts
                .iter()
                .map(|&vi| mesh.vertices[vi as usize][axis])
                .sum::<f32>()
                / face.verts.len() as f32;
            (depth, unit[0] * p0[0] + unit[1] * p0[1] + unit[2] * p0[2])
        }
        None => (0.0, 0.0),
    };
    (BlockKey { basis, positive: n[axis] >= 0.0 }, depth, unit, offset)
}

/// Build the projected layout for `mesh`.
///
/// `anchor` carries block origins forward from a previous layout; pass `None`
/// for the canonical layout (project load, migration, a fresh mesh).
///
/// A block too wide for the atlas does not fail the whole layout — its faces
/// fall back to the overflow shelf, which always fits because a single island
/// is capped at `MAX_ISLAND_SIDE`. Growing the atlas cannot fix a
/// wider-than-atlas block, so failing there would be an unrecoverable dead end.
/// `AtlasFull` is only returned when the shelf itself runs out, which growth
/// *can* fix.
pub fn plan(
    mesh: &Mesh,
    atlas: (u32, u32),
    anchor: Option<&LayoutAnchor>,
) -> Result<Layout, AtlasFull> {
    let mut out = Layout {
        islands: vec![Island::default(); mesh.faces.len()],
        ..Default::default()
    };
    if mesh.faces.is_empty() {
        return Ok(out);
    }

    // ── Classify: faces in index order, grouped by (component, block).
    let comp = components(mesh);
    let comp_count = comp.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    let mut groups: Vec<(u32, BlockKey, Vec<Slot>)> = Vec::new();
    for c in 0..comp_count {
        for key in BLOCK_ORDER {
            let mut slots = Vec::new();
            for (fi, &fc) in comp.iter().enumerate() {
                if fc != c {
                    continue;
                }
                let (fkey, depth, normal, offset) = face_block(mesh, fi);
                if fkey != key {
                    continue;
                }
                let face = &mesh.faces[fi];
                let (min_u, min_v, w, h) = mesh.face_uv_bounds(face);
                let poly = face
                    .verts
                    .iter()
                    .map(|&vi| key.basis.project(mesh.vertices[vi as usize]))
                    .collect();
                slots.push(Slot {
                    face: fi as u32,
                    min_u,
                    min_v,
                    w,
                    h,
                    depth,
                    normal,
                    offset,
                    poly,
                    lx: 0,
                    ly: 0,
                });
            }
            if !slots.is_empty() {
                groups.push((c, key, slots));
            }
        }
    }

    // ── Place faces inside their block, at their projected offset.
    let mut blocks: Vec<(u32, BlockKey, Vec<Slot>, u16, u16)> = Vec::new();
    let mut overflow: Vec<Slot> = Vec::new();
    for (c, key, mut slots) in groups {
        let bbox = |f: fn(&Slot) -> f32, s: &[Slot]| s.iter().map(f).fold(f32::MAX, f32::min);
        let fresh = (bbox(|s| s.min_u, &slots), bbox(|s| s.min_v, &slots));
        let mut origin = anchor.and_then(|a| a.get(c, key)).unwrap_or(fresh);
        // A stale origin that would push a face to a negative offset is
        // discarded; growing in +u/+v never re-origins.
        if slots.iter().any(|s| s.min_u < origin.0 - 0.5 || s.min_v < origin.1 - 0.5) {
            origin = fresh;
        }

        // Frontmost first, so the face that wins a contested slot is the one
        // you would actually see from this view.
        slots.sort_by(|a, b| {
            let (da, db) = if key.positive { (b.depth, a.depth) } else { (a.depth, b.depth) };
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal).then(a.face.cmp(&b.face))
        });

        let mut placed: Vec<Slot> = Vec::new();
        let (mut bw, mut bh) = (0u16, 0u16);
        for mut s in slots {
            let lx = (s.min_u - origin.0).round() as i64;
            let ly = (s.min_v - origin.1).round() as i64;
            let fits = lx >= 0
                && ly >= 0
                && lx + s.w as i64 <= u16::MAX as i64
                && ly + s.h as i64 <= u16::MAX as i64;
            if !fits {
                overflow.push(s);
                continue;
            }
            s.lx = lx as u16;
            s.ly = ly as u16;
            // Rect overlap is only a cheap pre-filter; what actually contests a
            // slot is two distinct surfaces covering the same ground.
            let contested = placed.iter().any(|p| {
                rects_overlap(p.rect(), s.rect())
                    && !p.coplanar_with(&s)
                    && polys_overlap(&p.poly, &s.poly)
            });
            if contested {
                overflow.push(s);
                continue;
            }
            bw = bw.max(s.lx + s.w);
            bh = bh.max(s.ly + s.h);
            placed.push(s);
        }

        if placed.is_empty() {
            continue;
        }
        blocks.push((c, key, placed, bw, bh));
    }

    // ── Shelf-place the blocks themselves, then the overflow faces.
    let mut cursor = AtlasCursor::default();
    for (c, key, placed, bw, bh) in blocks {
        match shelf_place(&mut cursor, bw, bh, atlas) {
            Ok((bx, by)) => {
                out.anchor.set(
                    c,
                    key,
                    placed[0].min_u - placed[0].lx as f32,
                    placed[0].min_v - placed[0].ly as f32,
                );
                for s in placed {
                    out.islands[s.face as usize] =
                        Island { x: bx + s.lx, y: by + s.ly, w: s.w, h: s.h };
                }
            }
            Err(full) => {
                // Too wide for this atlas: growth cannot help a block that is
                // wider than the atlas can ever be, and a taller atlas would
                // not change its width. Spill it rather than dead-end.
                if full.need_w > atlas.0 {
                    overflow.extend(placed);
                } else {
                    return Err(full);
                }
            }
        }
    }

    overflow.sort_by_key(|s| s.face);
    for s in &overflow {
        let (x, y) = shelf_place(&mut cursor, s.w, s.h, atlas)?;
        out.islands[s.face as usize] = Island { x, y, w: s.w, h: s.h };
        out.overflowed.push(s.face);
    }

    out.cursor = cursor;
    debug_assert!(
        out.islands.iter().all(|i| i.w >= 1 && i.h >= 1),
        "every face must get a non-degenerate island: a zero-size island panics paint::pick"
    );
    Ok(out)
}

/// Whether `mesh`'s islands already are the canonical projected layout — the
/// one `plan` produces with no anchor. Used to decide whether a loaded file
/// needs migrating, and to assert the invariant in tests.
pub fn is_canonical(mesh: &Mesh, atlas: (u32, u32)) -> bool {
    match plan(mesh, atlas, None) {
        Ok(l) => l
            .islands
            .iter()
            .zip(mesh.faces.iter())
            .all(|(want, face)| *want == face.island),
        Err(_) => false,
    }
}
