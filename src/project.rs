// src/project.rs
use serde::{Deserialize, Serialize};

pub type Rgba = [u8; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectMode {
    Normal,
    SpriteStack,
    ThreeD,
    Blob,
    Wang,
}

impl Default for ProjectMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl ProjectMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::SpriteStack => "Sprite Stack",
            Self::ThreeD => "3D",
            Self::Blob => "Blob",
            Self::Wang => "Wang",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub palette: Vec<Rgba>,
    pub animations: Vec<Animation>,
    pub active_animation: usize,
    pub active_frame: usize,
    pub active_layer: usize,
    #[serde(default = "default_id_counter")]
    pub layer_id_counter: u64,
    // Tile/sprite-sheet grid.
    // tiles_w = tiles_h = 1, tile_w = tile_h = 0 means "not tiled" (legacy behavior)
    #[serde(default)]
    pub tiles_w: u32,
    #[serde(default)]
    pub tiles_h: u32,
    #[serde(default)]
    pub tile_w: u32,
    #[serde(default)]
    pub tile_h: u32,
    #[serde(default)]
    pub mode: ProjectMode,
    #[serde(default)]
    pub sprite_stack_max_layers: Option<u32>,
}

fn default_id_counter() -> u64 { 1 }

impl Project {
    pub fn new(width: u32, height: u32, name: String) -> Self {
        Self::new_tiled(width, height, name, 1, 1, width, height)
    }

    pub fn new_tiled(width: u32, height: u32, name: String, tiles_w: u32, tiles_h: u32, tile_w: u32, tile_h: u32) -> Self {
        let frame_count = if tiles_w > 1 || tiles_h > 1 { (tiles_w * tiles_h) as usize } else { 1 };
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            frames.push(Frame::new(width, height, 1));
        }
        Self {
            name,
            canvas_width: width,
            canvas_height: height,
            palette: default_palette(),
            animations: vec![Animation {
                name: "Animation 1".to_string(),
                fps: 12,
                frames,
                tile_start: 0,
                tile_end: if frame_count > 0 { frame_count - 1 } else { 0 },
                tile_visible: true,
            }],
            active_animation: 0,
            active_frame: 0,
            active_layer: 0,
            layer_id_counter: 1,
            tiles_w,
            tiles_h,
            tile_w,
            tile_h,
            mode: ProjectMode::Normal,
            sprite_stack_max_layers: None,
        }
    }

    pub fn new_with_mode(width: u32, height: u32, name: String, mode: ProjectMode) -> Self {
        let mut proj = Self::new(width, height, name);
        proj.mode = mode;
        proj
    }

    pub fn new_tiled_with_mode(width: u32, height: u32, name: String, tiles_w: u32, tiles_h: u32, tile_w: u32, tile_h: u32, mode: ProjectMode) -> Self {
        let mut proj = Self::new_tiled(width, height, name, tiles_w, tiles_h, tile_w, tile_h);
        proj.mode = mode;
        proj
    }

    pub fn is_tiled(&self) -> bool {
        self.tiles_w > 1 || self.tiles_h > 1
    }

    pub fn resize_tilemap(&mut self, new_tiles_w: u32, new_tiles_h: u32, anchor: Anchor) {
        if !self.is_tiled() { return; }
        
        let old_tiles_w = self.tiles_w;
        let old_tiles_h = self.tiles_h;
        let tile_w = self.tile_w;
        let tile_h = self.tile_h;
        
        let new_canvas_width = new_tiles_w * tile_w;
        let new_canvas_height = new_tiles_h * tile_h;
        
        let offset_x = match anchor {
            Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => 0,
            Anchor::Top | Anchor::Center | Anchor::Bottom => (new_tiles_w as i32 - old_tiles_w as i32) / 2,
            Anchor::TopRight | Anchor::Right | Anchor::BottomRight => new_tiles_w as i32 - old_tiles_w as i32,
        };
        
        let offset_y = match anchor {
            Anchor::TopLeft | Anchor::Top | Anchor::TopRight => 0,
            Anchor::Left | Anchor::Center | Anchor::Right => (new_tiles_h as i32 - old_tiles_h as i32) / 2,
            Anchor::BottomLeft | Anchor::Bottom | Anchor::BottomRight => new_tiles_h as i32 - old_tiles_h as i32,
        };
        
        for anim in &mut self.animations {
            let mut new_frames = Vec::new();
            let old_frames = std::mem::take(&mut anim.frames);
            
            for new_ty in 0..new_tiles_h {
                for new_tx in 0..new_tiles_w {
                    let old_tx = new_tx as i32 - offset_x;
                    let old_ty = new_ty as i32 - offset_y;
                    
                    if old_tx >= 0 && old_tx < old_tiles_w as i32 && old_ty >= 0 && old_ty < old_tiles_h as i32 {
                        let old_fi = (old_ty * old_tiles_w as i32 + old_tx) as usize;
                        if old_fi < old_frames.len() {
                            let mut frame = old_frames[old_fi].clone();
                            frame.resize_canvas(new_canvas_width, new_canvas_height);
                            new_frames.push(frame);
                            continue;
                        }
                    }
                    
                    let mut frame = if let Some(template) = old_frames.first() {
                        let layers: Vec<Layer> = template.layers.iter().map(|l| {
                            let mut new_layer = Layer::new_with_id(l.name.clone(), new_canvas_width, new_canvas_height, l.id);
                            new_layer.visible = l.visible;
                            new_layer.locked = l.locked;
                            new_layer.opacity = l.opacity;
                            new_layer.blend_mode = l.blend_mode.clone();
                            new_layer.is_group = l.is_group;
                            new_layer.group_id = l.group_id;
                            new_layer.collapsed = l.collapsed;
                            new_layer
                        }).collect();
                        Frame {
                            duration_ms: template.duration_ms,
                            layers,
                            dirty: true,
                        }
                    } else {
                        Frame::new(new_canvas_width, new_canvas_height, 1)
                    };
                    new_frames.push(frame);
                }
            }
            anim.frames = new_frames;
            
            let frame_count = anim.frames.len();
            anim.tile_start = anim.tile_start.min(frame_count.saturating_sub(1));
            anim.tile_end = anim.tile_end.clamp(anim.tile_start, frame_count.saturating_sub(1));
        }
        
        self.tiles_w = new_tiles_w;
        self.tiles_h = new_tiles_h;
        self.canvas_width = new_canvas_width;
        self.canvas_height = new_canvas_height;
    }

    pub fn next_layer_id(&mut self) -> u64 {
        self.layer_id_counter += 1;
        self.layer_id_counter
    }

    pub fn active_anim(&self) -> &Animation {
        &self.animations[self.active_animation]
    }

    pub fn active_anim_mut(&mut self) -> &mut Animation {
        &mut self.animations[self.active_animation]
    }

    pub fn active_frame_ref(&self) -> &Frame {
        &self.active_anim().frames[self.active_frame]
    }

    pub fn active_frame_mut(&mut self) -> &mut Frame {
        let af = self.active_frame;
        self.active_anim_mut().frames.get_mut(af).unwrap()
    }

    pub fn active_layer_ref(&self) -> &Layer {
        &self.active_frame_ref().layers[self.active_layer]
    }

    pub fn active_layer_mut(&mut self) -> &mut Layer {
        let al = self.active_layer;
        self.active_frame_mut().layers.get_mut(al).unwrap()
    }
}

fn default_palette() -> Vec<Rgba> {
    // Endesga 36 Palette
    vec![
        [0xdb, 0xe0, 0xe7, 0xff], // #dbe0e7
        [0xa3, 0xac, 0xbe, 0xff], // #a3acbe
        [0x67, 0x70, 0x8b, 0xff], // #67708b
        [0x4e, 0x53, 0x71, 0xff], // #4e5371
        [0x39, 0x3a, 0x56, 0xff], // #393a56
        [0x26, 0x24, 0x3a, 0xff], // #26243a
        [0x14, 0x10, 0x20, 0xff], // #141020
        [0x7b, 0xcf, 0x5c, 0xff], // #7bcf5c
        [0x50, 0x9b, 0x4b, 0xff], // #509b4b
        [0x2e, 0x6a, 0x42, 0xff], // #2e6a42
        [0x1a, 0x45, 0x3b, 0xff], // #1a453b
        [0x0f, 0x27, 0x38, 0xff], // #0f2738
        [0x0d, 0x2f, 0x6d, 0xff], // #0d2f6d
        [0x0f, 0x4d, 0xa3, 0xff], // #0f4da3
        [0x0e, 0x82, 0xce, 0xff], // #0e82ce
        [0x13, 0xb2, 0xf2, 0xff], // #13b2f2
        [0x41, 0xf3, 0xfc, 0xff], // #41f3fc
        [0xf0, 0xd2, 0xaf, 0xff], // #f0d2af
        [0xe5, 0xae, 0x78, 0xff], // #e5ae78
        [0xc5, 0x81, 0x58, 0xff], // #c58158
        [0x94, 0x55, 0x42, 0xff], // #945542
        [0x62, 0x35, 0x30, 0xff], // #623530
        [0x46, 0x21, 0x1f, 0xff], // #46211f
        [0x97, 0x43, 0x2a, 0xff], // #97432a
        [0xe5, 0x70, 0x28, 0xff], // #e57028
        [0xf7, 0xac, 0x37, 0xff], // #f7ac37
        [0xfb, 0xdf, 0x6b, 0xff], // #fbdf6b
        [0xfe, 0x97, 0x9b, 0xff], // #fe979b
        [0xed, 0x52, 0x59, 0xff], // #ed5259
        [0xc4, 0x2c, 0x36, 0xff], // #c42c36
        [0x78, 0x1f, 0x2c, 0xff], // #781f2c
        [0x35, 0x14, 0x28, 0xff], // #351428
        [0x4d, 0x23, 0x52, 0xff], // #4d2352
        [0x7f, 0x3b, 0x86, 0xff], // #7f3b86
        [0xb4, 0x5e, 0xb3, 0xff], // #b45eb3
        [0xe3, 0x8d, 0xd6, 0xff], // #e38dd6
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub fps: u8,
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub tile_start: usize,
    #[serde(default)]
    pub tile_end: usize,
    #[serde(default = "true_default")]
    pub tile_visible: bool,
}

pub fn true_default() -> bool { true }

impl Animation {
    pub fn new(name: String, width: u32, height: u32, layer_id: u64) -> Self {
        Self {
            name,
            fps: 12,
            frames: vec![Frame::new(width, height, layer_id)],
            tile_start: 0,
            tile_end: 0,
            tile_visible: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub duration_ms: u32,
    pub layers: Vec<Layer>,
    #[serde(skip)]
    pub dirty: bool,
}

impl Frame {
    pub fn new(width: u32, height: u32, layer_id: u64) -> Self {
        Self {
            duration_ms: 0,
            layers: vec![Layer::new_with_id("Layer 1".to_string(), width, height, layer_id)],
            dirty: true,
        }
    }

    pub fn resize_canvas(&mut self, new_width: u32, new_height: u32) {
        for layer in &mut self.layers {
            if layer.is_group { continue; }
            let old_pixels = std::mem::take(&mut layer.pixels);
            let new_len = (new_width * new_height * 4) as usize;
            let mut new_pixels = vec![0u8; new_len];
            let copy_w = layer.width.min(new_width) as usize;
            let copy_h = layer.height.min(new_height) as usize;
            for y in 0..copy_h {
                let old_row_start = y * layer.width as usize * 4;
                let new_row_start = y * new_width as usize * 4;
                let copy_bytes = copy_w * 4;
                let src = &old_pixels[old_row_start..old_row_start + copy_bytes];
                new_pixels[new_row_start..new_row_start + copy_bytes].copy_from_slice(src);
            }
            layer.pixels = new_pixels;
            layer.width = new_width;
            layer.height = new_height;
        }
        self.dirty = true;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: u8,
    pub blend_mode: BlendMode,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub group_id: Option<u64>,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub background_color: Option<Rgba>,
}

impl Layer {
    /// Create a layer with explicit id (used by project/frame constructors).
    pub fn new_with_id(name: String, width: u32, height: u32, id: u64) -> Self {
        Self {
            name,
            visible: true,
            locked: false,
            opacity: 255,
            blend_mode: BlendMode::Normal,
            pixels: vec![0u8; (width * height * 4) as usize],
            width,
            height,
            id,
            is_group: false,
            group_id: None,
            collapsed: false,
            background_color: None,
        }
    }

    /// Convenience constructor used by tests and simple call sites. ID will be 0.
    pub fn new(name: String, width: u32, height: u32) -> Self {
        Self::new_with_id(name, width, height, 0)
    }

    pub fn new_group(name: String, width: u32, height: u32, id: u64) -> Self {
        let mut l = Self::new_with_id(name, width, height, id);
        l.is_group = true;
        l.pixels = Vec::new(); // groups have no pixel data
        l
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba {
        if self.pixels.is_empty() || x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let idx = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        if self.pixels.is_empty() || x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.pixels[idx]     = color[0];
        self.pixels[idx + 1] = color[1];
        self.pixels[idx + 2] = color[2];
        self.pixels[idx + 3] = color[3];
    }
}
