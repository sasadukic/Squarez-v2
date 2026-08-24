// src/io/sqr.rs
use std::io::{Read, Write};
use std::path::Path;
use serde::Deserialize;
use crate::project::{Animation, BlendMode, Frame, Layer, Project};

const MAGIC: &[u8; 4] = b"SQR\0";
const VERSION: u8 = 7;

/// Version 6 file layout: today's `Project` without the trailing 3D
/// lighting settings.
#[derive(serde::Deserialize)]
struct LegacyProjectV7Less {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<crate::project::Rgba>,
    animations: Vec<crate::project::Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: crate::project::ProjectMode,
    sprite_stack_max_layers: Option<u32>,
    mesh3d: Option<crate::three_d::mesh::Mesh>,
    glow_colors: Vec<crate::project::Rgba>,
    shadow_color: Option<crate::project::Rgba>,
    shadow_intensity: u8,
    emission_intensity: u8,
}

impl LegacyProjectV7Less {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode,
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: self.mesh3d,
            glow_colors: self.glow_colors,
            shadow_color: self.shadow_color,
            shadow_intensity: self.shadow_intensity,
            emission_intensity: self.emission_intensity,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

/// Version 5 file layout: separate `shadow_color` and `ao_color`, no
/// intensity fields. The two colors merged in v6 — an old ao_color survives
/// as the unified shadow color when no shadow color was set.
#[derive(serde::Deserialize)]
struct LegacyProjectV6Less {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<crate::project::Rgba>,
    animations: Vec<crate::project::Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: crate::project::ProjectMode,
    sprite_stack_max_layers: Option<u32>,
    mesh3d: Option<crate::three_d::mesh::Mesh>,
    glow_colors: Vec<crate::project::Rgba>,
    shadow_color: Option<crate::project::Rgba>,
    ao_color: Option<crate::project::Rgba>,
}

impl LegacyProjectV6Less {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode,
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: self.mesh3d,
            glow_colors: self.glow_colors,
            shadow_color: self.shadow_color.or(self.ao_color),
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

/// Version 4 file layout: today's `Project` without the trailing
/// `shadow_color` / `ao_color`.
#[derive(serde::Deserialize)]
struct LegacyProjectV5Less {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<crate::project::Rgba>,
    animations: Vec<crate::project::Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: crate::project::ProjectMode,
    sprite_stack_max_layers: Option<u32>,
    mesh3d: Option<crate::three_d::mesh::Mesh>,
    glow_colors: Vec<crate::project::Rgba>,
}

impl LegacyProjectV5Less {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode,
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: self.mesh3d,
            glow_colors: self.glow_colors,
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

/// Version 3 file layout: today's `Project` without the trailing
/// `glow_colors`. Bincode is positional, so decoding a v3 payload as the
/// current `Project` would misread — this mirror decodes the exact prefix
/// and defaults the new field.
#[derive(serde::Deserialize)]
struct LegacyProjectV4Less {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<crate::project::Rgba>,
    animations: Vec<crate::project::Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: crate::project::ProjectMode,
    sprite_stack_max_layers: Option<u32>,
    mesh3d: Option<crate::three_d::mesh::Mesh>,
}

impl LegacyProjectV4Less {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode,
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: self.mesh3d,
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

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
            // Version 7: current Project layout (3D lighting settings saved).
            7 => Ok(bincode::deserialize::<Project>(&decoded)?),
            // Version 6: no shading/shadow_mode/bake_ao/emission fields.
            6 => Ok(bincode::deserialize::<LegacyProjectV7Less>(&decoded)?.into_project()),
            // Version 5: separate shadow_color / ao_color, no intensities.
            5 => Ok(bincode::deserialize::<LegacyProjectV6Less>(&decoded)?.into_project()),
            // Version 4: no shadow_color / ao_color yet.
            4 => Ok(bincode::deserialize::<LegacyProjectV5Less>(&decoded)?.into_project()),
            // Version 3: same Project except no glow_colors field.
            3 => Ok(bincode::deserialize::<LegacyProjectV4Less>(&decoded)?.into_project()),
            // Version 2: additionally, Mesh had no manual_layout flag.
            2 => Ok(bincode::deserialize::<LegacyProjectV3Less>(&decoded)?.into_project()),
            // Version 1: bincode is positional, so old payloads must be decoded through
            // exact structural mirrors of the layouts that produced them. The version
            // byte was never bumped as Project evolved, so several distinct layouts
            // all claim "version 1" — try them longest-first (bincode tolerates
            // trailing bytes, so a shorter mirror could silently swallow a longer
            // file and drop its tail fields), and sanity-check each decode so a
            // misparse falls through to the next mirror instead of loading garbage.
            1 => load_v1(&decoded),
            v => Err(format!("Unsupported .sqr version: {}", v).into()),
        }
    } else {
        match crate::io::v2::load_v2(path) {
            Ok(project) => Ok(project),
            Err(e) => Err(Box::new(e)),
        }
    }
}

/// Decode a version-1 payload through the known historical layouts.
fn load_v1(decoded: &[u8]) -> Result<Project, Box<dyn std::error::Error>> {
    // Frame-mesh era: every Frame carried a (discarded) mesh. Distinct branch
    // of history from the tail-field progression below, so it goes first.
    let first_error = match bincode::deserialize::<LegacyProjectV2>(decoded) {
        Ok(legacy) => {
            let p = legacy.into_project();
            if plausible(&p) {
                return Ok(p);
            }
            "frame-mesh layout decoded but failed sanity checks".into()
        }
        Err(e) => Box::<dyn std::error::Error>::from(e),
    };
    // Tail-field progression, newest first: Project grew mode, then
    // sprite_stack_max_layers, then (in version 2) mesh3d.
    if let Ok(legacy) = bincode::deserialize::<LegacyProjectModeStack>(decoded) {
        let p = legacy.into_project();
        if plausible(&p) {
            return Ok(p);
        }
    }
    if let Ok(legacy) = bincode::deserialize::<LegacyProjectMode>(decoded) {
        let p = legacy.into_project();
        if plausible(&p) {
            return Ok(p);
        }
    }
    if let Ok(legacy) = bincode::deserialize::<LegacyProjectNoMode>(decoded) {
        let p = legacy.into_project();
        if plausible(&p) {
            return Ok(p);
        }
    }
    if let Ok(legacy) = bincode::deserialize::<LegacyProjectV1>(decoded) {
        let p = legacy.into_project();
        if plausible(&p) {
            return Ok(p);
        }
    }
    Err(first_error)
}

/// Does a decoded project look like a real project rather than a misparse?
/// bincode has no schema, so a wrong mirror can "succeed" by reading pixel
/// data as struct fields — but it will not produce layers whose buffers
/// agree with their dimensions.
fn plausible(p: &Project) -> bool {
    let dims_ok = (1..=16384).contains(&p.canvas_width) && (1..=16384).contains(&p.canvas_height);
    dims_ok
        && !p.animations.is_empty()
        && p.animations.iter().all(|a| {
            a.frames.iter().all(|f| {
                f.layers.iter().all(|l| {
                    l.is_group || l.pixels.len() as u64 == l.width as u64 * l.height as u64 * 4
                })
            })
        })
}

// ── Version-1 payloads, tail-field progression ────────────────────────────────
// These three builds serialized the current Layer/Frame/Animation shapes; only
// the Project tail differs. Recovered by byte-level dissection of real files:
// Soldier.sqr (ends at tile_h), SpriteStack.sqr (+ mode), lighthouse.sqr
// (+ mode + sprite_stack_max_layers).

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectNoMode {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectMode {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: LegacyProjectModeV2,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectModeStack {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<Animation>,
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

impl LegacyProjectNoMode {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: crate::project::ProjectMode::Normal,
            sprite_stack_max_layers: None,
            mesh3d: None,
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

impl LegacyProjectMode {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode.into_mode(),
            sprite_stack_max_layers: None,
            mesh3d: None,
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

impl LegacyProjectModeStack {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode.into_mode(),
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: None,
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}

impl LegacyProjectModeV2 {
    fn into_mode(self) -> crate::project::ProjectMode {
        match self {
            LegacyProjectModeV2::Normal => crate::project::ProjectMode::Normal,
            LegacyProjectModeV2::SpriteStack => crate::project::ProjectMode::SpriteStack,
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
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
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
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
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

// ── Version-2 payload: current Project, Mesh without manual_layout ───────────
// The only difference from the current layout is the trailing flag on Mesh;
// everything else deserializes through the crate types directly.

#[derive(Debug, Clone, Deserialize)]
struct LegacyProjectV3Less {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: crate::project::ProjectMode,
    sprite_stack_max_layers: Option<u32>,
    mesh3d: Option<LegacyMeshV2>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyMeshV2 {
    vertices: Vec<[f32; 3]>,
    faces: Vec<crate::three_d::mesh::Face>,
    atlas_cursor: crate::three_d::mesh::AtlasCursor,
}

impl LegacyProjectV3Less {
    fn into_project(self) -> Project {
        Project {
            name: self.name,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            palette: self.palette,
            animations: self.animations,
            active_animation: self.active_animation,
            active_frame: self.active_frame,
            active_layer: self.active_layer,
            layer_id_counter: self.layer_id_counter,
            tiles_w: self.tiles_w,
            tiles_h: self.tiles_h,
            tile_w: self.tile_w,
            tile_h: self.tile_h,
            mode: self.mode,
            sprite_stack_max_layers: self.sprite_stack_max_layers,
            mesh3d: self.mesh3d.map(|m| crate::three_d::mesh::Mesh {
                vertices: m.vertices,
                faces: m.faces,
                atlas_cursor: m.atlas_cursor,
                manual_layout: false,
            }),
            glow_colors: Vec::new(),
            shadow_color: None,
            shadow_intensity: 45,
            emission_intensity: 50,
            shading: crate::three_d::Shading::Off,
            shadow_mode: crate::three_d::light::ShadowMode::Off,
            bake_ao: false,
            emission: true,
        }
    }
}
