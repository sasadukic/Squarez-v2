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
}

impl Project {
    pub fn new(width: u32, height: u32, name: String) -> Self {
        Self {
            name,
            canvas_width: width,
            canvas_height: height,
            palette: default_palette(),
            animations: vec![Animation::new("Animation 1".to_string(), width, height)],
            active_animation: 0,
            active_frame: 0,
            active_layer: 0,
        }
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
    vec![
        [0, 0, 0, 255],
        [255, 255, 255, 255],
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub fps: u8,
    pub frames: Vec<Frame>,
}

impl Animation {
    pub fn new(name: String, width: u32, height: u32) -> Self {
        Self {
            name,
            fps: 12,
            frames: vec![Frame::new(width, height)],
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
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            duration_ms: 0,
            layers: vec![Layer::new("Layer 1".to_string(), width, height)],
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
    pub opacity: u8,
    pub blend_mode: BlendMode,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Layer {
    pub fn new(name: String, width: u32, height: u32) -> Self {
        Self {
            name,
            visible: true,
            opacity: 255,
            blend_mode: BlendMode::Normal,
            pixels: vec![0u8; (width * height * 4) as usize],
            width,
            height,
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba {
        if x >= self.width || y >= self.height {
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
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.pixels[idx]     = color[0];
        self.pixels[idx + 1] = color[1];
        self.pixels[idx + 2] = color[2];
        self.pixels[idx + 3] = color[3];
    }
}
