// src/io/obj.rs
//
// Native save format for 3D-mode projects: Wavefront OBJ + MTL with the
// texture atlas written as a PNG alongside (model.obj / model.mtl /
// model.png). The writer emits our exact subset; the reader accepts that
// subset back (exact round-trip) and loads foreign OBJs best-effort.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::layers::composite_frame;
use crate::project::{Project, ProjectMode};
use crate::three_d::mesh::{AtlasCursor, Face, Island, Mesh, GUTTER};

pub const MATERIAL_NAME: &str = "squarez_mat";

type BoxError = Box<dyn std::error::Error>;

fn sibling(path: &Path, ext: &str) -> PathBuf {
    path.with_extension(ext)
}

/// Write `stem.obj` + `stem.mtl` + `stem.png` next to each other.
pub fn save_obj(project: &Project, path: &Path) -> Result<(), BoxError> {
    let mesh = project.mesh3d.as_ref().ok_or("project has no 3D model")?;
    let aw = project.canvas_width.max(1) as f32;
    let ah = project.canvas_height.max(1) as f32;
    let mtl_path = sibling(path, "mtl");
    let png_path = sibling(path, "png");
    let mtl_name = mtl_path.file_name().and_then(|s| s.to_str()).ok_or("bad path")?.to_string();
    let png_name = png_path.file_name().and_then(|s| s.to_str()).ok_or("bad path")?.to_string();

    // ── OBJ ──
    let mut obj = String::new();
    obj.push_str("# Squarez 3D model\n");
    obj.push_str(&format!("mtllib {}\n", mtl_name));
    obj.push_str(&format!("o {}\n", project.name.replace(' ', "_")));
    for v in &mesh.vertices {
        obj.push_str(&format!("v {} {} {}\n", v[0], v[1], v[2]));
    }
    // One vt per face corner (duplicates are fine; keeps the writer dumb).
    // OBJ UV origin is bottom-left, our texel rows are top-down: flip V.
    for face in &mesh.faces {
        let basis = mesh.face_plane_basis(face);
        let (min_u, min_v, _, _) = mesh.face_uv_bounds(face);
        let isl = face.island;
        for &vi in &face.verts {
            let (u, v) = basis.project(mesh.vertices[vi as usize]);
            let tx = isl.x as f32 + (u - min_u).clamp(0.0, isl.w as f32);
            let ty = isl.y as f32 + (v - min_v).clamp(0.0, isl.h as f32);
            obj.push_str(&format!("vt {} {}\n", tx / aw, 1.0 - ty / ah));
        }
    }
    obj.push_str(&format!("usemtl {}\n", MATERIAL_NAME));
    obj.push_str("s off\n");
    let mut vt_index = 1usize; // 1-based
    for face in &mesh.faces {
        obj.push('f');
        for &vi in &face.verts {
            obj.push_str(&format!(" {}/{}", vi + 1, vt_index));
            vt_index += 1;
        }
        obj.push('\n');
    }
    std::fs::File::create(path)?.write_all(obj.as_bytes())?;

    // ── MTL ──
    let mtl = format!(
        "newmtl {}\nKd 1.000 1.000 1.000\nillum 0\nmap_Kd {}\n",
        MATERIAL_NAME, png_name
    );
    std::fs::File::create(&mtl_path)?.write_all(mtl.as_bytes())?;

    // ── PNG (flattened atlas) ──
    let frame = &project.animations[0].frames[0];
    let pixels = composite_frame(frame, project.canvas_width, project.canvas_height);
    image::save_buffer(
        &png_path,
        &pixels,
        project.canvas_width,
        project.canvas_height,
        image::ColorType::Rgba8,
    )?;

    Ok(())
}

/// Read an OBJ (+ MTL + texture) back into a ThreeD-mode project.
///
/// Accepts `v`, `vt`, `f` (v, v/vt, v/vt/vn, v//vn), `mtllib`; ignores
/// `vn`/`o`/`g`/`s`/`usemtl`/comments. Faces with more than 4 corners are
/// fan-triangulated. Files we wrote round-trip exactly (islands rebuilt
/// from vt bounding boxes); foreign files keep their geometry but get
/// fresh blank islands.
pub fn load_obj(path: &Path) -> Result<Project, BoxError> {
    let text = std::fs::read_to_string(path)?;
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut vts: Vec<(f32, f32)> = Vec::new();
    // Each parsed face: vertex indices + optional per-corner vt indices.
    let mut raw_faces: Vec<(Vec<u32>, Option<Vec<u32>>)> = Vec::new();
    let mut mtllib: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let x: f32 = parts.next().ok_or("bad v line")?.parse()?;
                let y: f32 = parts.next().ok_or("bad v line")?.parse()?;
                let z: f32 = parts.next().ok_or("bad v line")?.parse()?;
                vertices.push([x, y, z]);
            }
            Some("vt") => {
                let u: f32 = parts.next().ok_or("bad vt line")?.parse()?;
                let v: f32 = parts.next().unwrap_or("0").parse()?;
                vts.push((u, v));
            }
            Some("f") => {
                let mut vis: Vec<u32> = Vec::new();
                let mut vtis: Vec<Option<u32>> = Vec::new();
                for corner in parts {
                    let mut it = corner.split('/');
                    let vi: i64 = it.next().ok_or("bad f corner")?.parse()?;
                    if vi <= 0 {
                        return Err("negative/zero OBJ indices are not supported".into());
                    }
                    vis.push(vi as u32 - 1);
                    let vti = match it.next() {
                        Some("") | None => None,
                        Some(s) => {
                            let n: i64 = s.parse()?;
                            if n <= 0 {
                                return Err("negative/zero OBJ vt indices are not supported".into());
                            }
                            Some(n as u32 - 1)
                        }
                    };
                    vtis.push(vti);
                }
                if vis.len() < 3 {
                    return Err("face with fewer than 3 corners".into());
                }
                let all_vt: Option<Vec<u32>> = vtis.iter().copied().collect();
                // Fan-triangulate anything beyond a quad.
                if vis.len() <= 4 {
                    raw_faces.push((vis, all_vt));
                } else {
                    for k in 1..vis.len() - 1 {
                        let tri = vec![vis[0], vis[k], vis[k + 1]];
                        let tri_vt = all_vt.as_ref().map(|vt| vec![vt[0], vt[k], vt[k + 1]]);
                        raw_faces.push((tri, tri_vt));
                    }
                }
            }
            Some("mtllib") => {
                mtllib = parts.next().map(|s| s.to_string());
            }
            _ => {}
        }
    }

    for (vis, _) in &raw_faces {
        for &vi in vis {
            if vi as usize >= vertices.len() {
                return Err(format!("face references missing vertex {}", vi + 1).into());
            }
        }
    }

    // ── Texture: resolve map_Kd relative to the OBJ ──
    let mut atlas_w = 256u32;
    let mut atlas_h = 256u32;
    let mut texture_pixels: Option<Vec<u8>> = None;
    if let Some(mtl_name) = &mtllib {
        let mtl_path = path.parent().unwrap_or(Path::new(".")).join(mtl_name);
        if let Ok(mtl_text) = std::fs::read_to_string(&mtl_path) {
            for line in mtl_text.lines() {
                let mut parts = line.trim().split_whitespace();
                if parts.next() == Some("map_Kd") {
                    if let Some(tex_name) = parts.next() {
                        let tex_path = mtl_path.parent().unwrap_or(Path::new(".")).join(tex_name);
                        if let Ok(img) = image::open(&tex_path) {
                            let rgba = img.to_rgba8();
                            atlas_w = rgba.width().max(1);
                            atlas_h = rgba.height().max(1);
                            texture_pixels = Some(rgba.into_raw());
                        }
                    }
                }
            }
        }
    }

    // ── Islands from vt bounding boxes (exact for our own files) ──
    let mut faces: Vec<Face> = Vec::new();
    let mut islands_ok = !raw_faces.is_empty();
    for (vis, vt_idx) in &raw_faces {
        let island = vt_idx
            .as_ref()
            .and_then(|idx| island_from_vts(idx, &vts, atlas_w, atlas_h));
        match island {
            Some(isl) => faces.push(Face { verts: vis.clone(), island: isl }),
            None => {
                islands_ok = false;
                faces.push(Face { verts: vis.clone(), island: Island::default() });
            }
        }
    }

    let mut mesh = Mesh { vertices, faces, atlas_cursor: AtlasCursor::default() };
    if islands_ok {
        // Future allocations go on a fresh row below every existing island.
        let max_bottom = mesh
            .faces
            .iter()
            .map(|f| f.island.y + f.island.h)
            .max()
            .unwrap_or(0);
        mesh.atlas_cursor = AtlasCursor { x: GUTTER, y: max_bottom + GUTTER, row_h: 0 };
    } else {
        // Foreign file: geometry only, fresh blank islands for every face.
        for face in &mut mesh.faces {
            face.island = Island::default();
        }
        mesh.atlas_cursor = AtlasCursor::default();
        // Grow the atlas height until everything fits.
        loop {
            let mut trial = mesh.clone();
            match trial.allocate_all_islands((atlas_w, atlas_h)) {
                Ok(()) => {
                    mesh = trial;
                    break;
                }
                Err(_) if atlas_h < 4096 => atlas_h *= 2,
                Err(_) => return Err("model too large for the texture atlas".into()),
            }
        }
    }
    mesh.validate().map_err(|e| format!("invalid mesh: {}", e))?;

    // ── Project ──
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Model")
        .to_string();
    let mut project = Project::new_with_mode(atlas_w, atlas_h, name, ProjectMode::ThreeD);
    {
        let layer = &mut project.animations[0].frames[0].layers[0];
        layer.name = "Texture".to_string();
        if let Some(pixels) = texture_pixels {
            if pixels.len() == (atlas_w * atlas_h * 4) as usize {
                layer.pixels = pixels;
                layer.width = atlas_w;
                layer.height = atlas_h;
            }
        }
    }
    project.mesh3d = Some(mesh);
    Ok(project)
}

/// Reconstruct a face's island from its vt indices: the rounded bounding box
/// of the (V-unflipped) texel coordinates. None if anything looks degenerate.
fn island_from_vts(vt_idx: &[u32], vts: &[(f32, f32)], atlas_w: u32, atlas_h: u32) -> Option<Island> {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for &i in vt_idx {
        let &(u, v) = vts.get(i as usize)?;
        let tx = u * atlas_w as f32;
        let ty = (1.0 - v) * atlas_h as f32;
        min_x = min_x.min(tx);
        max_x = max_x.max(tx);
        min_y = min_y.min(ty);
        max_y = max_y.max(ty);
    }
    let x = min_x.round();
    let y = min_y.round();
    let w = (max_x - min_x).round();
    let h = (max_y - min_y).round();
    // Sanity: integral, at least 1 texel, inside the atlas, u16-safe.
    let close = |a: f32, b: f32| (a - b).abs() < 0.01;
    if !close(x, min_x) || !close(y, min_y) || w < 1.0 || h < 1.0 {
        return None;
    }
    if x < 0.0 || y < 0.0 || x + w > atlas_w as f32 + 0.01 || y + h > atlas_h as f32 + 0.01 {
        return None;
    }
    if x > u16::MAX as f32 || y > u16::MAX as f32 {
        return None;
    }
    Some(Island { x: x as u16, y: y as u16, w: w as u16, h: h as u16 })
}
