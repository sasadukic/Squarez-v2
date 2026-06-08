// src/io/sqr.rs
use std::io::{Read, Write};
use std::path::Path;
use serde::Deserialize;
use crate::project::{Animation, BlendMode, Frame, Layer, Project};

const MAGIC: &[u8; 4] = b"SQR\0";
const VERSION: u8 = 1;

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
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err("Invalid .sqr file: bad magic bytes".into());
    }
    let mut version = [0u8; 1];
    file.read_exact(&mut version)?;
    if version[0] != VERSION {
        return Err(format!("Unsupported .sqr version: {}", version[0]).into());
    }
    let mut compressed = Vec::new();
    file.read_to_end(&mut compressed)?;
    let decoded = lz4_flex::decompress_size_prepended(&compressed)?;
    match bincode::deserialize::<Project>(&decoded) {
        Ok(project) => Ok(project),
        Err(current_error) => match bincode::deserialize::<LegacyProjectV1>(&decoded) {
            Ok(legacy) => Ok(legacy.into_project()),
            Err(_) => Err(Box::new(current_error)),
        },
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
        }
    }
}
