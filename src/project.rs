// src/project.rs
use serde::{Deserialize, Serialize};

pub type Rgba = [u8; 4];

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
}

fn default_id_counter() -> u64 { 1 }

impl Project {
    pub fn new(width: u32, height: u32, name: String) -> Self {
        Self {
            name,
            canvas_width: width,
            canvas_height: height,
            palette: default_palette(),
            animations: vec![Animation::new("Animation 1".to_string(), width, height, 1)],
            active_animation: 0,
            active_frame: 0,
            active_layer: 0,
            layer_id_counter: 1,
        }
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
    // Lospec-style default palette
    vec![
        [0xff, 0xff, 0xff, 0xff], // #ffffff
        [0x6d, 0xf7, 0xc1, 0xff], // #6df7c1
        [0x11, 0xad, 0xc1, 0xff], // #11adc1
        [0x60, 0x6c, 0x81, 0xff], // #606c81
        [0x39, 0x34, 0x57, 0xff], // #393457
        [0x1e, 0x88, 0x75, 0xff], // #1e8875
        [0x5b, 0xb3, 0x61, 0xff], // #5bb361
        [0xa1, 0xe5, 0x5a, 0xff], // #a1e55a
        [0xf7, 0xe4, 0x76, 0xff], // #f7e476
        [0xf9, 0x92, 0x52, 0xff], // #f99252
        [0xcb, 0x4d, 0x68, 0xff], // #cb4d68
        [0x6a, 0x37, 0x71, 0xff], // #6a3771
        [0xc9, 0x24, 0x64, 0xff], // #c92464
        [0xf4, 0x8c, 0xb6, 0xff], // #f48cb6
        [0xf7, 0xb6, 0x9e, 0xff], // #f7b69e
        [0x9b, 0x9c, 0x82, 0xff], // #9b9c82
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub fps: u8,
    pub frames: Vec<Frame>,
}

impl Animation {
    pub fn new(name: String, width: u32, height: u32, layer_id: u64) -> Self {
        Self {
            name,
            fps: 12,
            frames: vec![Frame::new(width, height, layer_id)],
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
            layers: vec![Layer::new("Layer 1".to_string(), width, height, layer_id)],
            dirty: true,
        }
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
}

impl Layer {
    pub fn new(name: String, width: u32, height: u32, id: u64) -> Self {
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
        }
    }

    pub fn new_group(name: String, width: u32, height: u32, id: u64) -> Self {
        let mut l = Self::new(name, width, height, id);
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
