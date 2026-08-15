// src/three_d/paint.rs
//
// Painting directly on the model: cursor → front-most triangle → barycentric
// → atlas texel → the existing pure tool functions against the active layer.
// One Command::PaintPixels per stroke, exactly like the 2D canvas.

use egui::Pos2;

use super::render::Scene;
use super::ThreeDState;
use crate::color::ColorState;
use crate::history::{Command, UndoStack};
use crate::project::{Layer, Project, Rgba};
use crate::tools::{apply_eraser, apply_pencil, bresenham_positions, ActiveTool};

/// Result of one frame of paint handling.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaintResult {
    pub canvas_dirty: bool,
    pub modified: bool,
}

/// A picked point on the model: face index plus the atlas texel under the
/// cursor (clamped into the face's island).
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub face: u32,
    pub texel: (i64, i64),
}

/// Barycentric coordinates of `p` in triangle `t`, or None if outside
/// (or the triangle is degenerate).
fn barycentric(p: Pos2, t: [Pos2; 3]) -> Option<(f32, f32, f32)> {
    let v0 = t[1] - t[0];
    let v1 = t[2] - t[0];
    let v2 = p - t[0];
    let d00 = v0.dot(v0);
    let d01 = v0.dot(v1);
    let d11 = v1.dot(v1);
    let d20 = v2.dot(v0);
    let d21 = v2.dot(v1);
    let denom = d00 * d11 - d01 * d01;
    if denom.abs() < 1e-6 {
        return None;
    }
    let v = (d11 * d20 - d01 * d21) / denom;
    let w = (d00 * d21 - d01 * d20) / denom;
    let u = 1.0 - v - w;
    const EPS: f32 = 1e-4;
    if u >= -EPS && v >= -EPS && w >= -EPS {
        Some((u, v, w))
    } else {
        None
    }
}

/// Find the front-most face + texel under the cursor.
pub fn pick(scene: &Scene, pos: Pos2, mesh: &super::mesh::Mesh, atlas: (u32, u32)) -> Option<Hit> {
    // tris are sorted far → near; walk them near → far.
    for tri in scene.tris.iter().rev() {
        if let Some((u, v, w)) = barycentric(pos, tri.pts) {
            let uv_x = tri.uvs[0].x * u + tri.uvs[1].x * v + tri.uvs[2].x * w;
            let uv_y = tri.uvs[0].y * u + tri.uvs[1].y * v + tri.uvs[2].y * w;
            let isl = mesh.faces[tri.face as usize].island;
            let tx = (uv_x * atlas.0 as f32).floor() as i64;
            let ty = (uv_y * atlas.1 as f32).floor() as i64;
            let tx = tx.clamp(isl.x as i64, (isl.x + isl.w) as i64 - 1);
            let ty = ty.clamp(isl.y as i64, (isl.y + isl.h) as i64 - 1);
            return Some(Hit { face: tri.face, texel: (tx, ty) });
        }
    }
    None
}

/// Whether this tool paints on the model.
pub fn is_paint_tool(tool: &ActiveTool) -> bool {
    matches!(
        tool,
        ActiveTool::Pencil | ActiveTool::Eraser | ActiveTool::Fill | ActiveTool::Eyedropper
    )
}

fn layer_paintable(layer: &Layer) -> bool {
    !layer.locked && !layer.is_group && layer.background_color.is_none() && !layer.pixels.is_empty()
}

/// Apply a single-texel tool edit, recording undo data once per stroke texel.
fn paint_texel(
    state: &mut ThreeDState,
    layer: &mut Layer,
    tool: &ActiveTool,
    color: Rgba,
    tx: i64,
    ty: i64,
    isl: super::mesh::Island,
) -> bool {
    if tx < isl.x as i64 || ty < isl.y as i64 {
        return false;
    }
    let (x, y) = (tx as u32, ty as u32);
    if x >= (isl.x + isl.w) as u32 || y >= (isl.y + isl.h) as u32 {
        return false;
    }
    if state.stroke_painted.contains(&(x, y)) {
        return false;
    }
    let edits = match tool {
        ActiveTool::Pencil => apply_pencil(layer, x, y, color),
        ActiveTool::Eraser => apply_eraser(layer, x, y),
        _ => return false,
    };
    let mut changed = false;
    for (ex, ey, old, new) in edits {
        layer.set_pixel(ex, ey, new);
        state.stroke_edits.push((ex, ey, old, new));
        changed = true;
    }
    state.stroke_painted.insert((x, y));
    changed
}

/// Fill a face's entire island with `replacement` — the bucket paints the
/// whole face, not a flood region (a flood would stop after one texel on
/// the default checkerboard).
pub fn fill_island(
    layer: &mut Layer,
    isl: super::mesh::Island,
    replacement: Rgba,
) -> Vec<(u32, u32, Rgba, Rgba)> {
    let mut edits = Vec::new();
    for y in 0..isl.h as u32 {
        for x in 0..isl.w as u32 {
            let (gx, gy) = (isl.x as u32 + x, isl.y as u32 + y);
            let old = layer.get_pixel(gx, gy);
            if old != replacement {
                edits.push((gx, gy, old, replacement));
            }
        }
    }
    edits
}

/// Handle one frame of paint input. Call with the scene already built for
/// this frame; mutates the active layer and pushes undo commands on release.
#[allow(clippy::too_many_arguments)]
pub fn handle(
    state: &mut ThreeDState,
    project: &mut Project,
    undo: &mut UndoStack,
    color_state: &mut ColorState,
    tool: &ActiveTool,
    scene: &Scene,
    response: &egui::Response,
    ui: &egui::Ui,
) -> PaintResult {
    let mut result = PaintResult::default();
    if !is_paint_tool(tool) || project.mesh3d.is_none() {
        return result;
    }

    let atlas = (project.canvas_width, project.canvas_height);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let li = project.active_layer;

    // Hover feedback for the workspace to draw (face + exact texel).
    let hover = pointer.and_then(|p| {
        let mesh = project.mesh3d.as_ref().unwrap();
        pick(scene, p, mesh, atlas)
    });
    state.hover_face = hover.map(|h| h.face);
    state.hover_texel = hover.map(|h| (h.texel.0 as u32, h.texel.1 as u32));

    let press_started = response.drag_started_by(egui::PointerButton::Primary)
        || (ui.input(|i| i.pointer.primary_pressed()) && response.hovered());
    if press_started {
        state.stroke_edits.clear();
        state.stroke_painted.clear();
        state.last_paint = None;
    }

    let painting = primary_down && (response.dragged_by(egui::PointerButton::Primary) || response.hovered());
    if painting {
        if let Some(pos) = pointer {
            let mesh = project.mesh3d.as_ref().unwrap();
            if let Some(hit) = pick(scene, pos, mesh, atlas) {
                let isl = mesh.faces[hit.face as usize].island;
                let frame = &mut project.animations[0].frames[0];
                if let Some(layer) = frame.layers.get_mut(li) {
                    if layer_paintable(layer) {
                        match tool {
                            ActiveTool::Pencil | ActiveTool::Eraser => {
                                let color = color_state.foreground;
                                let mut changed = false;
                                // Stroke continuity within one face only.
                                if let Some((last_face, (lx, ly))) = state.last_paint {
                                    if last_face == hit.face && (lx, ly) != hit.texel {
                                        for (px, py) in bresenham_positions(
                                            lx as i32,
                                            ly as i32,
                                            hit.texel.0 as i32,
                                            hit.texel.1 as i32,
                                        ) {
                                            changed |= paint_texel(
                                                state, layer, tool, color, px as i64, py as i64, isl,
                                            );
                                        }
                                    }
                                }
                                changed |= paint_texel(
                                    state, layer, tool, color, hit.texel.0, hit.texel.1, isl,
                                );
                                if changed {
                                    frame.dirty = true;
                                    result.canvas_dirty = true;
                                }
                                state.last_paint = Some((hit.face, hit.texel));
                            }
                            // Fill acts once per press, not continuously.
                            ActiveTool::Fill if press_started => {
                                let edits = fill_island(layer, isl, color_state.foreground);
                                if !edits.is_empty() {
                                    for &(x, y, _, new) in &edits {
                                        layer.set_pixel(x, y, new);
                                    }
                                    state.stroke_edits.extend(edits);
                                    frame.dirty = true;
                                    result.canvas_dirty = true;
                                }
                            }
                            ActiveTool::Eyedropper => {
                                let c = layer.get_pixel(hit.texel.0 as u32, hit.texel.1 as u32);
                                if c[3] > 0 {
                                    color_state.foreground = c;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if primary_released && !state.stroke_edits.is_empty() {
        let edits = std::mem::take(&mut state.stroke_edits);
        state.stroke_painted.clear();
        state.last_paint = None;
        undo.push(Command::PaintPixels { animation_id: 0, frame_id: 0, layer_id: li, edits });
        result.modified = true;
    }
    if primary_released {
        state.last_paint = None;
    }

    result
}
