// src/io/sqr.rs
use std::io::{Read, Write};
use std::path::Path;
use serde::Deserialize;
use crate::project::{Animation, BlendMode, Frame, Layer, Project};

const MAGIC: &[u8; 4] = b"SQR\0";
const VERSION: u8 = 2;

pub fn save_sqr(project: &Project, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = bincode::serialize(project)?;
    let compressed = lz4_flex::compress_prepend_size(&encoded);
    let mut file = std::fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&[VERSION])?;
    file.write_all(&compressed)?;
    Ok(())
}

pub fn load_sqr(path: &Path) -> Result<Project, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0u8; 4];
    if file.read_exact(&mut magic).is_ok() && &magic == MAGIC {
        let mut version = [0u8; 1];
        file.read_exact(&mut version)?;
        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed)?;
        let decoded = lz4_flex::decompress_size_prepended(&compressed)?;
        match version[0] {
            // Version 2: current Project layout (Frame has no mesh; Project has mesh3d).
            2 => Ok(bincode::deserialize::<Project>(&decoded)?),
            // Version 1: bincode is positional, so old payloads must be decoded through
            // exact structural mirrors of the layouts that produced them.
            1 => match bincode::deserialize::<LegacyProjectV2>(&decoded) {
                Ok(legacy) => Ok(legacy.into_project()),
                Err(v2_error) => match bincode::deserialize::<LegacyProjectV1>(&decoded) {
                    Ok(legacy) => Ok(legacy.into_project()),
                    Err(_) => Err(Box::new(v2_error)),
                },
            },
            v => Err(format!("Unsupported .sqr version: {}", v).into()),
        }
    } else {
        match crate::io::v2::load_v2(path) {
            Ok(project) => Ok(project),
            Err(e) => Err(Box::new(e)),
        }
    }
}

// ── Version-1 payload, late layout ────────────────────────────────────────────
// Exact mirror of the Project layout the last v1-writing builds serialized:
// every Frame carried a Mesh3D (vertices/edges/faces). The mesh was only ever
// written by a long-removed feature and is discarded on load.

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectV2 {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<LegacyAnimationV2>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: LegacyProjectModeV2,
    sprite_stack_max_layers: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum LegacyProjectModeV2 {
    Normal,
    SpriteStack,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyAnimationV2 {
    name: String,
    fps: u8,
    frames: Vec<LegacyFrameV2>,
    tile_start: usize,
    tile_end: usize,
    tile_visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyFrameV2 {
    duration_ms: u32,
    layers: Vec<Layer>,
    #[allow(dead_code)]
    mesh: LegacyMesh3D,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyMesh3D {
    #[allow(dead_code)]
    vertices: Vec<(f32, f32, f32)>,
    #[allow(dead_code)]
    edges: Vec<(u64, u64)>,
    #[allow(dead_code)]
    faces: Vec<LegacyFace3D>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyFace3D {
    #[allow(dead_code)]
    vertex_indices: Vec<u64>,
    #[allow(dead_code)]
    color: [u8; 4],
}

impl LegacyProjectV2 {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self
                .animations
                .into_iter()
                .map(|a| Animation {
                    name: a.name,
                    fps: a.fps,
                    frames: a
                        .frames
                        .into_iter()
                        .map(|f| Frame { duration_ms: f.duration_ms, layers: f.layers, dirty: true })
                        .collect(),
                    tile_start: a.tile_start,
                    tile_end: a.tile_end,
                    tile_visible: a.tile_visible,
                })
                .collect(),
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: match self.mode {
                LegacyProjectModeV2::Normal => crate::project::ProjectMode::Normal,
                LegacyProjectModeV2::SpriteStack => crate::project::ProjectMode::SpriteStack,
            },
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectV1 {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<LegacyAnimationV1>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
}

impl LegacyProjectV1 {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations.into_iter().map(LegacyAnimationV1::into_animation).collect(),
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: 1,
            tiles_w: 1,
            tiles_h: 1,
            tile_w: 0,
            tile_h: 0,
            mode: crate::project::ProjectMode::Normal,
            sprite_stack_max_layers: None,
            mesh3d: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyAnimationV1 {
    name: String,
    fps: u8,
    frames: Vec<LegacyFrameV1>,
}

impl LegacyAnimationV1 {
    fn into_animation(self) -> Animation {
        Animation {
            name: self.name,
            fps: self.fps,
            frames: self.frames.into_iter().map(LegacyFrameV1::into_frame).collect(),
            tile_start: 0,
            tile_end: 0,
            tile_visible: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyFrameV1 {
    duration_ms: u32,
    layers: Vec<LegacyLayerV1>,
}

impl LegacyFrameV1 {
    fn into_frame(self) -> Frame {
        Frame {
            duration_ms: self.duration_ms,
            layers: self.layers.into_iter().map(LegacyLayerV1::into_layer).collect(),
            dirty: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyLayerV1 {
    name: String,
    visible: bool,
    opacity: u8,
    blend_mode: BlendMode,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl LegacyLayerV1 {
    fn into_layer(self) -> Layer {
        Layer {
            name: self.name,
            visible: self.visible,
            locked: false,
            opacity: self.opacity,
            blend_mode: self.blend_mode,
            pixels: self.pixels,
            width: self.width,
            height: self.height,
            id: 0,
            is_group: false,
            group_id: None,
            collapsed: false,
            background_color: None,
        }
    }
}
