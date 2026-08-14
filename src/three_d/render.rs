// src/three_d/render.rs
//
// Projects the mesh through the camera into screen-space triangles
// (painter's algorithm, backface-culled) and draws them as one textured
// egui::Mesh sampling the project's atlas texture. The same `Scene` doubles
// as the picking structure for painting and face selection.

use egui::{Color32, Pos2, Rect, Stroke};

use super::camera::{Camera3D, SnapView};
use super::mesh::Mesh;
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
    let mut scene = Scene::default();
    let aw = atlas.0.max(1) as f32;
    let ah = atlas.1.max(1) as f32;

    // Per-face lighting from a view-space key light (top-left-front), like
    // Blender's solid viewport — with a pixel-art touch: lit faces shift
    // slightly warm, shadowed faces slightly cool, instead of a flat gray
    // multiply. Snapped views render unlit so texel colors read true.
    let unlit = cam.snapped().is_some();
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
        let basis = mesh.face_plane_basis(face);
        let (min_u, min_v, _, _) = mesh.face_uv_bounds(face);
        let isl = face.island;
        let corners: Vec<(Pos2, Pos2, f32)> = face
            .verts
            .iter()
            .map(|&vi| {
                let p = mesh.vertices[vi as usize];
                let (pos, depth) = cam.project(p, rect);
                let (u, v) = basis.project(p);
                let tx = isl.x as f32 + (u - min_u).clamp(0.0, isl.w as f32);
                let ty = isl.y as f32 + (v - min_v).clamp(0.0, isl.h as f32);
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
            });
        }
    }

    // Two passes: every interior triangle first, then front triangles,
    // each far-to-near — interiors can never overdraw the outside, which
    // avoids average-depth misorders at corners.
    scene
        .tris
        .sort_by(|a, b| a.front.cmp(&b.front).then(a.depth.total_cmp(&b.depth)));
    scene
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

/// Edge overlay: a dark silhouette outline (edges bordering exactly one
/// visible face) plus a faint interior wireframe, so topology (loop cuts,
/// insets) stays visible with every tool.
pub fn paint_wireframe(
    painter: &egui::Painter,
    mesh: &Mesh,
    scene: &Scene,
    cam: &Camera3D,
    rect: Rect,
    theme: &Theme,
) {
    // Count how many visible faces borders each edge.
    let mut edge_faces: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    for &fi in &scene.visible_faces {
        let face = &mesh.faces[fi as usize];
        let k = face.verts.len();
        for i in 0..k {
            let a = face.verts[i];
            let b = face.verts[(i + 1) % k];
            *edge_faces.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let silhouette = Stroke::new(2.0, Color32::from_black_alpha(150));
    let interior = Stroke::new(1.0, theme.muted.gamma_multiply(0.55));
    for (&(a, b), &count) in &edge_faces {
        let is_silhouette = count == 1;
        let (pa, _) = cam.project(mesh.vertices[a as usize], rect);
        let (pb, _) = cam.project(mesh.vertices[b as usize], rect);
        painter.line_segment([pa, pb], if is_silhouette { silhouette } else { interior });
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
