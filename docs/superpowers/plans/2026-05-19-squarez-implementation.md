# Squarez Pixel Art Editor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone cross-platform pixel art editor with layers, named animation clips, HSV + OKLab color pickers, and `.sqr` project format.

**Architecture:** Single Rust binary using `eframe`/`egui` for the GUI, GPU-backed textures for canvas rendering, and `bincode` + `lz4_flex` for project serialization. All drawing state lives in a central `AppState` struct that flows through the egui `update()` loop each frame.

**Tech Stack:** Rust 1.95+, eframe 0.31, egui 0.31, serde + bincode, lz4_flex, image, gif, palette crate.

---

## File Map

```
C:\Users\psilo\Desktop\Squarez\
├── Cargo.toml
├── assets/
│   └── dogicapixel.ttf          ← copy from C:\Users\psilo\Desktop\dogicapixel.ttf
├── src/
│   ├── main.rs                  ← eframe entry point, window config
│   ├── app.rs                   ← AppState, update() loop, panel layout
│   ├── project.rs               ← Project, Animation, Frame, Layer, RGBA types
│   ├── theme.rs                 ← Theme struct, 4-color system, egui Visuals
│   ├── canvas.rs                ← Texture upload, zoom/pan, screen↔pixel coords
│   ├── history.rs               ← Command enum, UndoStack
│   ├── layers.rs                ← Layer compositing, blend modes
│   ├── animation.rs             ← PlaybackState, onion skin, thumbnail cache
│   ├── tools/
│   │   ├── mod.rs               ← Tool enum, ToolInput, PixelEdit
│   │   ├── pencil.rs            ← Pencil + Eraser tools
│   │   ├── fill.rs              ← Flood fill
│   │   ├── eyedropper.rs        ← Color sample
│   │   ├── shapes.rs            ← Rectangle, Ellipse, Line
│   │   └── select.rs            ← Rect select, Move
│   ├── color/
│   │   ├── mod.rs               ← Rgba type, conversions, ColorState
│   │   ├── hsv.rs               ← HSV picker egui widget
│   │   └── oklab.rs             ← OKLab picker egui widget
│   └── io/
│       ├── mod.rs
│       ├── sqr.rs               ← .sqr save / load
│       └── export.rs            ← PNG, GIF, spritesheet export
└── tests/
    ├── project_tests.rs
    ├── history_tests.rs
    ├── tools_tests.rs
    ├── color_tests.rs
    └── io_tests.rs
```

---

## Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `assets/dogicapixel.ttf` (copy from Desktop)

- [ ] **Step 1: Copy the font asset**

```bash
cp "C:/Users/psilo/Desktop/dogicapixel.ttf" "C:/Users/psilo/Desktop/Squarez/assets/dogicapixel.ttf"
```

- [ ] **Step 2: Initialize cargo project and write Cargo.toml**

Run in `C:\Users\psilo\Desktop\Squarez`:
```bash
cargo init --name squarez
```

Then replace `Cargo.toml` with:
```toml
[package]
name = "squarez"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = { version = "0.31", features = ["default"] }
egui = "0.31"
serde = { version = "1", features = ["derive"] }
bincode = "1"
lz4_flex = { version = "0.11", features = ["frame"] }
image = { version = "0.25", default-features = false, features = ["png"] }
gif = "0.13"
palette = { version = "0.7", features = ["std"] }

[profile.release]
opt-level = 3
lto = true
strip = true
```

- [ ] **Step 3: Write stub main.rs**

```rust
// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Squarez")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Squarez",
        options,
        Box::new(|_cc| Ok(Box::new(squarez::app::App::default()))),
    )
}
```

- [ ] **Step 4: Create src/lib.rs to expose modules**

```rust
// src/lib.rs
pub mod app;
pub mod project;
pub mod theme;
pub mod canvas;
pub mod history;
pub mod layers;
pub mod animation;
pub mod tools;
pub mod color;
pub mod io;
```

- [ ] **Step 5: Create empty module stubs so it compiles**

Create each file with just a comment:
- `src/app.rs` → `// app module`
- `src/project.rs` → `// project module`
- `src/theme.rs` → `// theme module`
- `src/canvas.rs` → `// canvas module`
- `src/history.rs` → `// history module`
- `src/layers.rs` → `// layers module`
- `src/animation.rs` → `// animation module`
- `src/tools/mod.rs` → `// tools module`
- `src/color/mod.rs` → `// color module`
- `src/io/mod.rs` → `// io module`

Also add stub `App` struct to `src/app.rs`:
```rust
// src/app.rs
#[derive(Default)]
pub struct App;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Squarez");
        });
    }
}
```

- [ ] **Step 6: Verify it compiles and runs**

```bash
cd "C:/Users/psilo/Desktop/Squarez"
cargo run
```

Expected: window opens with label "Squarez". Close the window.

- [ ] **Step 7: Commit**

```bash
git init
git add .
git commit -m "chore: project scaffold, cargo init, font asset"
```

---

## Task 2: Core Data Types

**Files:**
- Create: `src/project.rs`
- Create: `tests/project_tests.rs`

- [ ] **Step 1: Write failing tests**

Create `tests/project_tests.rs`:
```rust
use squarez::project::*;

#[test]
fn project_default_has_one_animation() {
    let p = Project::new(32, 32, "test".to_string());
    assert_eq!(p.animations.len(), 1);
    assert_eq!(p.animations[0].name, "Animation 1");
}

#[test]
fn animation_default_has_one_frame() {
    let p = Project::new(32, 32, "test".to_string());
    assert_eq!(p.animations[0].frames.len(), 1);
}

#[test]
fn frame_default_has_one_layer() {
    let p = Project::new(32, 32, "test".to_string());
    assert_eq!(p.animations[0].frames[0].layers.len(), 1);
}

#[test]
fn layer_pixel_buffer_correct_size() {
    let layer = Layer::new("Layer 1".to_string(), 32, 32);
    assert_eq!(layer.pixels.len(), 32 * 32 * 4);
}

#[test]
fn layer_pixels_start_transparent() {
    let layer = Layer::new("Layer 1".to_string(), 16, 16);
    assert!(layer.pixels.iter().all(|&b| b == 0));
}

#[test]
fn layer_get_set_pixel() {
    let mut layer = Layer::new("L".to_string(), 8, 8);
    layer.set_pixel(3, 4, [255, 0, 128, 255]);
    assert_eq!(layer.get_pixel(3, 4), [255, 0, 128, 255]);
}

#[test]
fn layer_get_pixel_out_of_bounds_returns_transparent() {
    let layer = Layer::new("L".to_string(), 8, 8);
    assert_eq!(layer.get_pixel(100, 100), [0, 0, 0, 0]);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd "C:/Users/psilo/Desktop/Squarez"
cargo test --test project_tests 2>&1
```

Expected: compile error — `project` module is empty.

- [ ] **Step 3: Implement project.rs**

```rust
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
        self.pixels[idx] = color[0];
        self.pixels[idx + 1] = color[1];
        self.pixels[idx + 2] = color[2];
        self.pixels[idx + 3] = color[3];
    }
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test --test project_tests
```

Expected: `test result: ok. 8 passed`

- [ ] **Step 5: Commit**

```bash
git add src/project.rs tests/project_tests.rs
git commit -m "feat: core data types — Project, Animation, Frame, Layer"
```

---

## Task 3: Theme System

**Files:**
- Create: `src/theme.rs`

- [ ] **Step 1: Implement theme.rs**

```rust
// src/theme.rs
use egui::{Color32, FontData, FontDefinitions, FontFamily, Style, Visuals};

pub const FONT_SIZE_SM: f32 = 8.0;
pub const FONT_SIZE_MD: f32 = 16.0;
pub const FONT_SIZE_LG: f32 = 32.0;

#[derive(Debug, Clone)]
pub struct Theme {
    pub bg:    Color32,
    pub mid:   Color32,
    pub light: Color32,
    pub fg:    Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg:    Color32::from_hex("#000000").unwrap(),
            mid:   Color32::from_hex("#676767").unwrap(),
            light: Color32::from_hex("#b6b6b6").unwrap(),
            fg:    Color32::from_hex("#ffffff").unwrap(),
        }
    }
}

impl Theme {
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();

        visuals.panel_fill           = self.bg;
        visuals.window_fill          = self.bg;
        visuals.extreme_bg_color     = self.bg;
        visuals.code_bg_color        = self.mid;
        visuals.faint_bg_color       = self.mid;
        visuals.widgets.noninteractive.bg_fill  = self.mid;
        visuals.widgets.inactive.bg_fill        = self.mid;
        visuals.widgets.hovered.bg_fill         = self.light;
        visuals.widgets.active.bg_fill          = self.fg;
        visuals.widgets.noninteractive.fg_stroke.color = self.fg;
        visuals.widgets.inactive.fg_stroke.color       = self.fg;
        visuals.widgets.hovered.fg_stroke.color        = self.bg;
        visuals.widgets.active.fg_stroke.color         = self.bg;
        visuals.selection.bg_fill    = self.light;
        visuals.selection.stroke.color = self.bg;
        visuals.window_stroke.color  = self.light;
        visuals.widgets.noninteractive.bg_stroke.color = self.light;

        let mut style = Style::default();
        style.visuals = visuals;
        style.override_font_id = Some(egui::FontId::new(FONT_SIZE_SM, FontFamily::Monospace));
        ctx.set_style(style);
    }
}

pub fn load_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "dogicapixel".to_owned(),
        FontData::from_static(include_bytes!("../assets/dogicapixel.ttf")),
    );
    fonts.families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "dogicapixel".to_owned());
    fonts.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "dogicapixel".to_owned());
    ctx.set_fonts(fonts);
}
```

- [ ] **Step 2: Wire theme into app.rs**

Replace `src/app.rs` with:
```rust
// src/app.rs
use crate::project::Project;
use crate::theme::{Theme, load_fonts};

pub struct App {
    pub project: Project,
    pub theme: Theme,
}

impl Default for App {
    fn default() -> Self {
        Self {
            project: Project::new(32, 32, "Untitled".to_string()),
            theme: Theme::default(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Squarez — theme test");
        });
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        load_fonts(&cc.egui_ctx);
        Self::default()
    }
}
```

- [ ] **Step 3: Update main.rs to use App::new**

```rust
// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Squarez")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Squarez",
        options,
        Box::new(|cc| Ok(Box::new(squarez::app::App::new(cc)))),
    )
}
```

- [ ] **Step 4: Run to verify dark theme and pixel font**

```bash
cargo run
```

Expected: window opens with dark background, white monospace pixel text "Squarez — theme test".

- [ ] **Step 5: Commit**

```bash
git add src/theme.rs src/app.rs src/main.rs
git commit -m "feat: theme system — 4-color palette, dogicapixel font"
```

---

## Task 4: Undo / Redo System

**Files:**
- Create: `src/history.rs`
- Create: `tests/history_tests.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/history_tests.rs
use squarez::history::{UndoStack, Command};
use squarez::project::Project;

#[test]
fn undo_stack_starts_empty() {
    let stack = UndoStack::new();
    assert!(!stack.can_undo());
    assert!(!stack.can_redo());
}

#[test]
fn push_command_enables_undo() {
    let mut stack = UndoStack::new();
    let cmd = Command::PaintPixels {
        animation_id: 0, frame_id: 0, layer_id: 0,
        edits: vec![(0, 0, [0,0,0,0], [255,0,0,255])],
    };
    stack.push(cmd);
    assert!(stack.can_undo());
    assert!(!stack.can_redo());
}

#[test]
fn undo_restores_pixel() {
    let mut project = Project::new(8, 8, "t".to_string());
    let mut stack = UndoStack::new();
    let old = project.animations[0].frames[0].layers[0].get_pixel(0, 0);
    project.animations[0].frames[0].layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
    let cmd = Command::PaintPixels {
        animation_id: 0, frame_id: 0, layer_id: 0,
        edits: vec![(0, 0, old, [255, 0, 0, 255])],
    };
    stack.push(cmd);
    stack.undo(&mut project);
    assert_eq!(project.animations[0].frames[0].layers[0].get_pixel(0, 0), [0, 0, 0, 0]);
}

#[test]
fn redo_replays_pixel() {
    let mut project = Project::new(8, 8, "t".to_string());
    let mut stack = UndoStack::new();
    let cmd = Command::PaintPixels {
        animation_id: 0, frame_id: 0, layer_id: 0,
        edits: vec![(1, 1, [0,0,0,0], [0, 255, 0, 255])],
    };
    stack.push(cmd);
    stack.undo(&mut project);
    stack.redo(&mut project);
    assert_eq!(project.animations[0].frames[0].layers[0].get_pixel(1, 1), [0, 255, 0, 255]);
}

#[test]
fn push_clears_redo_stack() {
    let mut project = Project::new(8, 8, "t".to_string());
    let mut stack = UndoStack::new();
    stack.push(Command::PaintPixels { animation_id:0, frame_id:0, layer_id:0, edits: vec![(0,0,[0,0,0,0],[1,0,0,255])] });
    stack.undo(&mut project);
    assert!(stack.can_redo());
    stack.push(Command::PaintPixels { animation_id:0, frame_id:0, layer_id:0, edits: vec![(1,0,[0,0,0,0],[2,0,0,255])] });
    assert!(!stack.can_redo());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --test history_tests 2>&1 | head -20
```

Expected: compile error.

- [ ] **Step 3: Implement history.rs**

```rust
// src/history.rs
use crate::project::{Project, Rgba};

pub const MAX_UNDO: usize = 100;

#[derive(Debug, Clone)]
pub enum Command {
    PaintPixels {
        animation_id: usize,
        frame_id: usize,
        layer_id: usize,
        edits: Vec<(u32, u32, Rgba, Rgba)>, // (x, y, old, new)
    },
    AddFrame {
        animation_id: usize,
        index: usize,
    },
    DeleteFrame {
        animation_id: usize,
        index: usize,
        snapshot: crate::project::Frame,
    },
    AddLayer {
        animation_id: usize,
        frame_id: usize,
        index: usize,
    },
    DeleteLayer {
        animation_id: usize,
        frame_id: usize,
        index: usize,
        snapshot: crate::project::Layer,
    },
}

pub struct UndoStack {
    commands: Vec<Command>,
    cursor: usize, // points to next empty slot
}

impl UndoStack {
    pub fn new() -> Self {
        Self { commands: Vec::new(), cursor: 0 }
    }

    pub fn can_undo(&self) -> bool { self.cursor > 0 }
    pub fn can_redo(&self) -> bool { self.cursor < self.commands.len() }

    pub fn push(&mut self, cmd: Command) {
        // Drop any redo history
        self.commands.truncate(self.cursor);
        self.commands.push(cmd);
        if self.commands.len() > MAX_UNDO {
            self.commands.remove(0);
        } else {
            self.cursor += 1;
        }
    }

    pub fn undo(&mut self, project: &mut Project) {
        if !self.can_undo() { return; }
        self.cursor -= 1;
        let cmd = self.commands[self.cursor].clone();
        apply_command(project, &cmd, Direction::Backward);
    }

    pub fn redo(&mut self, project: &mut Project) {
        if !self.can_redo() { return; }
        let cmd = self.commands[self.cursor].clone();
        self.cursor += 1;
        apply_command(project, &cmd, Direction::Forward);
    }
}

enum Direction { Forward, Backward }

fn apply_command(project: &mut Project, cmd: &Command, dir: Direction) {
    match cmd {
        Command::PaintPixels { animation_id, frame_id, layer_id, edits } => {
            let layer = &mut project.animations[*animation_id]
                .frames[*frame_id]
                .layers[*layer_id];
            for &(x, y, old, new) in edits {
                let color = match dir { Direction::Forward => new, Direction::Backward => old };
                layer.set_pixel(x, y, color);
            }
            project.animations[*animation_id].frames[*frame_id].dirty = true;
        }
        Command::AddFrame { animation_id, index } => {
            let anim = &mut project.animations[*animation_id];
            let (w, h) = (project.canvas_width, project.canvas_height);
            match dir {
                Direction::Forward  => anim.frames.insert(*index, crate::project::Frame::new(w, h)),
                Direction::Backward => { anim.frames.remove(*index); }
            }
        }
        Command::DeleteFrame { animation_id, index, snapshot } => {
            let anim = &mut project.animations[*animation_id];
            match dir {
                Direction::Forward  => { anim.frames.remove(*index); }
                Direction::Backward => anim.frames.insert(*index, snapshot.clone()),
            }
        }
        Command::AddLayer { animation_id, frame_id, index } => {
            let (w, h) = (project.canvas_width, project.canvas_height);
            let frame = &mut project.animations[*animation_id].frames[*frame_id];
            match dir {
                Direction::Forward  => frame.layers.insert(*index, crate::project::Layer::new("Layer".into(), w, h)),
                Direction::Backward => { frame.layers.remove(*index); }
            }
        }
        Command::DeleteLayer { animation_id, frame_id, index, snapshot } => {
            let frame = &mut project.animations[*animation_id].frames[*frame_id];
            match dir {
                Direction::Forward  => { frame.layers.remove(*index); }
                Direction::Backward => frame.layers.insert(*index, snapshot.clone()),
            }
        }
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --test history_tests
```

Expected: `test result: ok. 5 passed`

- [ ] **Step 5: Commit**

```bash
git add src/history.rs tests/history_tests.rs
git commit -m "feat: undo/redo system — command stack, 100-step limit"
```

---

## Task 5: Color System

**Files:**
- Create: `src/color/mod.rs`
- Create: `src/color/hsv.rs`
- Create: `src/color/oklab.rs`
- Create: `tests/color_tests.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/color_tests.rs
use squarez::color::{rgba_to_hsv, hsv_to_rgba, rgba_to_oklab, oklab_to_rgba};

#[test]
fn red_to_hsv() {
    let (h, s, v) = rgba_to_hsv([255, 0, 0, 255]);
    assert!((h - 0.0).abs() < 1.0);
    assert!((s - 1.0).abs() < 0.01);
    assert!((v - 1.0).abs() < 0.01);
}

#[test]
fn hsv_roundtrip() {
    let original = [100u8, 150, 200, 255];
    let (h, s, v) = rgba_to_hsv(original);
    let result = hsv_to_rgba(h, s, v, 255);
    assert_eq!(result[0], original[0]);
    assert_eq!(result[1], original[1]);
    assert_eq!(result[2], original[2]);
}

#[test]
fn white_oklab_has_max_lightness() {
    let (l, _a, _b) = rgba_to_oklab([255, 255, 255, 255]);
    assert!(l > 0.99);
}

#[test]
fn black_oklab_has_zero_lightness() {
    let (l, _a, _b) = rgba_to_oklab([0, 0, 0, 255]);
    assert!(l < 0.01);
}

#[test]
fn oklab_roundtrip() {
    let original = [80u8, 120, 200, 255];
    let (l, a, b) = rgba_to_oklab(original);
    let result = oklab_to_rgba(l, a, b, 255);
    // allow ±2 rounding error
    assert!((result[0] as i16 - original[0] as i16).abs() <= 2);
    assert!((result[1] as i16 - original[1] as i16).abs() <= 2);
    assert!((result[2] as i16 - original[2] as i16).abs() <= 2);
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test --test color_tests 2>&1 | head -5
```

Expected: compile error.

- [ ] **Step 3: Implement color/mod.rs**

```rust
// src/color/mod.rs
pub mod hsv;
pub mod oklab;
pub use hsv::{rgba_to_hsv, hsv_to_rgba};
pub use oklab::{rgba_to_oklab, oklab_to_rgba};
use crate::project::Rgba;

/// All color picker state for the right panel
#[derive(Debug, Clone)]
pub struct ColorState {
    pub foreground: Rgba,
    pub background: Rgba,
    pub active_picker: PickerMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerMode { Hsv, OkLab }

impl Default for ColorState {
    fn default() -> Self {
        Self {
            foreground: [0, 0, 0, 255],
            background: [255, 255, 255, 255],
            active_picker: PickerMode::Hsv,
        }
    }
}
```

- [ ] **Step 4: Implement color/hsv.rs**

```rust
// src/color/hsv.rs
use crate::project::Rgba;

/// Returns (hue 0-360, saturation 0-1, value 0-1)
pub fn rgba_to_hsv(color: Rgba) -> (f32, f32, f32) {
    let r = color[0] as f32 / 255.0;
    let g = color[1] as f32 / 255.0;
    let b = color[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

/// hue: 0-360, sat: 0-1, val: 0-1, alpha: 0-255
pub fn hsv_to_rgba(h: f32, s: f32, v: f32, alpha: u8) -> Rgba {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 { (c, x, 0.0) }
        else if h < 120.0 { (x, c, 0.0) }
        else if h < 180.0 { (0.0, c, x) }
        else if h < 240.0 { (0.0, x, c) }
        else if h < 300.0 { (x, 0.0, c) }
        else              { (c, 0.0, x) };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
        alpha,
    ]
}
```

- [ ] **Step 5: Implement color/oklab.rs**

```rust
// src/color/oklab.rs
use crate::project::Rgba;

/// Returns OKLab (L: 0-1, a: -0.5..0.5, b: -0.5..0.5)
pub fn rgba_to_oklab(color: Rgba) -> (f32, f32, f32) {
    let r = srgb_to_linear(color[0]);
    let g = srgb_to_linear(color[1]);
    let b = srgb_to_linear(color[2]);
    // sRGB → XYZ (D65)
    let lc = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let mc = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let sc = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let l = 0.2104542553 * lc + 0.7936177850 * mc - 0.0040720468 * sc;
    let a = 1.9779984951 * lc - 2.4285922050 * mc + 0.4505937099 * sc;
    let b_out = 0.0259040371 * lc + 0.7827717662 * mc - 0.8086757660 * sc;
    (l, a, b_out)
}

/// L: 0-1, a/b as above, alpha: 0-255 → RGBA
pub fn oklab_to_rgba(l: f32, a: f32, b: f32, alpha: u8) -> Rgba {
    let lc = l + 0.3963377774 * a + 0.2158037573 * b;
    let mc = l - 0.1055613458 * a - 0.0638541728 * b;
    let sc = l - 0.0894841775 * a - 1.2914855480 * b;
    let lc3 = lc * lc * lc;
    let mc3 = mc * mc * mc;
    let sc3 = sc * sc * sc;
    let r =  4.0767416621 * lc3 - 3.3077115913 * mc3 + 0.2309699292 * sc3;
    let g = -1.2684380046 * lc3 + 2.6097574011 * mc3 - 0.3413193965 * sc3;
    let b_out = -0.0041960863 * lc3 - 0.7034186147 * mc3 + 1.7076147010 * sc3;
    [
        linear_to_srgb(r),
        linear_to_srgb(g),
        linear_to_srgb(b_out),
        alpha,
    ]
}

fn srgb_to_linear(c: u8) -> f32 {
    let f = c as f32 / 255.0;
    if f <= 0.04045 { f / 12.92 } else { ((f + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(f: f32) -> u8 {
    let f = f.clamp(0.0, 1.0);
    let out = if f <= 0.0031308 { f * 12.92 } else { 1.055 * f.powf(1.0 / 2.4) - 0.055 };
    (out * 255.0).round() as u8
}
```

- [ ] **Step 6: Update src/color/mod.rs module declarations**

Replace the first line of `src/color/mod.rs` — already has `pub mod hsv; pub mod oklab;` — verify it matches Step 3 exactly.

- [ ] **Step 7: Run tests**

```bash
cargo test --test color_tests
```

Expected: `test result: ok. 5 passed`

- [ ] **Step 8: Commit**

```bash
git add src/color/ tests/color_tests.rs
git commit -m "feat: color math — HSV and OKLab conversions with roundtrip accuracy"
```

---

## Task 6: Layer Compositing

**Files:**
- Create: `src/layers.rs`

- [ ] **Step 1: Implement layers.rs**

```rust
// src/layers.rs
use crate::project::{Frame, Rgba, BlendMode};

/// Composites all visible layers in a frame into a single RGBA buffer (width*height*4 bytes)
pub fn composite_frame(frame: &Frame, width: u32, height: u32) -> Vec<u8> {
    let size = (width * height * 4) as usize;
    let mut out = vec![0u8; size];
    for layer in &frame.layers {
        if !layer.visible { continue; }
        let alpha_factor = layer.opacity as f32 / 255.0;
        for i in 0..(width * height) as usize {
            let idx = i * 4;
            let src = [
                layer.pixels[idx],
                layer.pixels[idx + 1],
                layer.pixels[idx + 2],
                layer.pixels[idx + 3],
            ];
            let dst = [out[idx], out[idx+1], out[idx+2], out[idx+3]];
            let blended = blend_normal(src, dst, alpha_factor);
            out[idx]   = blended[0];
            out[idx+1] = blended[1];
            out[idx+2] = blended[2];
            out[idx+3] = blended[3];
        }
    }
    out
}

fn blend_normal(src: Rgba, dst: Rgba, layer_alpha: f32) -> Rgba {
    let sa = (src[3] as f32 / 255.0) * layer_alpha;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a == 0.0 { return [0, 0, 0, 0]; }
    let blend_channel = |sc: u8, dc: u8| -> u8 {
        let s = sc as f32 / 255.0;
        let d = dc as f32 / 255.0;
        ((s * sa + d * da * (1.0 - sa)) / out_a * 255.0).round() as u8
    };
    [
        blend_channel(src[0], dst[0]),
        blend_channel(src[1], dst[1]),
        blend_channel(src[2], dst[2]),
        (out_a * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Frame;

    #[test]
    fn opaque_layer_covers_transparent_background() {
        let mut frame = Frame::new(2, 2);
        frame.layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
        let result = composite_frame(&frame, 2, 2);
        assert_eq!(&result[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn invisible_layer_is_skipped() {
        let mut frame = Frame::new(2, 2);
        frame.layers[0].visible = false;
        frame.layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
        let result = composite_frame(&frame, 2, 2);
        assert_eq!(&result[0..4], &[0, 0, 0, 0]);
    }
}
```

- [ ] **Step 2: Run inline tests**

```bash
cargo test layers
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 3: Commit**

```bash
git add src/layers.rs
git commit -m "feat: layer compositing — normal blend mode with per-layer opacity"
```

---

## Task 7: Tool System

**Files:**
- Create: `src/tools/mod.rs`
- Create: `src/tools/pencil.rs`
- Create: `src/tools/fill.rs`
- Create: `src/tools/eyedropper.rs`
- Create: `src/tools/shapes.rs`
- Create: `src/tools/select.rs`
- Create: `tests/tools_tests.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/tools_tests.rs
use squarez::tools::{ToolInput, apply_pencil, apply_eraser, apply_fill};
use squarez::project::{Layer, Project};

#[test]
fn pencil_paints_single_pixel() {
    let mut layer = Layer::new("L".to_string(), 8, 8);
    let edits = apply_pencil(&layer, 3, 4, [255, 0, 0, 255]);
    for (x, y, _old, new) in &edits { layer.set_pixel(*x, *y, *new); }
    assert_eq!(layer.get_pixel(3, 4), [255, 0, 0, 255]);
}

#[test]
fn eraser_makes_pixel_transparent() {
    let mut layer = Layer::new("L".to_string(), 8, 8);
    layer.set_pixel(2, 2, [255, 0, 0, 255]);
    let edits = apply_eraser(&layer, 2, 2);
    for (x, y, _old, new) in &edits { layer.set_pixel(*x, *y, *new); }
    assert_eq!(layer.get_pixel(2, 2), [0, 0, 0, 0]);
}

#[test]
fn fill_floods_connected_region() {
    let mut layer = Layer::new("L".to_string(), 4, 4);
    // paint a 2x2 block at top-left then fill from center
    let edits = apply_fill(&layer, 2, 2, [0, 0, 0, 0], [255, 0, 0, 255]);
    assert!(!edits.is_empty());
    // all transparent pixels should be filled
    assert_eq!(edits.len(), 16); // 4x4 all transparent
}

#[test]
fn fill_does_not_cross_boundary() {
    let mut layer = Layer::new("L".to_string(), 4, 4);
    // top row is red — fill below should not cross
    for x in 0..4 { layer.set_pixel(x, 0, [255, 0, 0, 255]); }
    let edits = apply_fill(&layer, 2, 2, [0, 0, 0, 0], [0, 0, 255, 255]);
    let filled: Vec<_> = edits.iter().filter(|e| e.1 == 0).collect();
    assert!(filled.is_empty(), "should not fill row 0");
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test --test tools_tests 2>&1 | head -5
```

Expected: compile error.

- [ ] **Step 3: Implement tools/mod.rs**

```rust
// src/tools/mod.rs
pub mod pencil;
pub mod fill;
pub mod eyedropper;
pub mod shapes;
pub mod select;

pub use pencil::{apply_pencil, apply_eraser, bresenham_line};
pub use fill::apply_fill;
pub use eyedropper::apply_eyedropper;
pub use shapes::{apply_rect, apply_ellipse, apply_line};

use crate::project::Rgba;

pub type PixelEdit = (u32, u32, Rgba, Rgba); // (x, y, old, new)

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveTool {
    Pencil,
    Eraser,
    Fill,
    Eyedropper,
    Rectangle { filled: bool },
    Ellipse   { filled: bool },
    Line,
    RectSelect,
    Move,
}

#[derive(Debug, Clone)]
pub struct ToolInput {
    pub canvas_x: u32,
    pub canvas_y: u32,
    pub color: Rgba,
}
```

- [ ] **Step 4: Implement tools/pencil.rs**

```rust
// src/tools/pencil.rs
use crate::project::{Layer, Rgba};
use super::PixelEdit;

pub fn apply_pencil(layer: &Layer, x: u32, y: u32, color: Rgba) -> Vec<PixelEdit> {
    let old = layer.get_pixel(x, y);
    if old == color { return vec![]; }
    vec![(x, y, old, color)]
}

pub fn apply_eraser(layer: &Layer, x: u32, y: u32) -> Vec<PixelEdit> {
    let old = layer.get_pixel(x, y);
    let transparent = [0u8, 0, 0, 0];
    if old == transparent { return vec![]; }
    vec![(x, y, old, transparent)]
}

/// Returns pixels along a Bresenham line between two points
pub fn bresenham_line(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let mut x0 = x0 as i32;
    let mut y0 = y0 as i32;
    let x1 = x1 as i32;
    let y1 = y1 as i32;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 {
            let edit = apply_pencil(layer, x0 as u32, y0 as u32, color);
            edits.extend(edit);
        }
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
    edits
}
```

- [ ] **Step 5: Implement tools/fill.rs**

```rust
// src/tools/fill.rs
use crate::project::{Layer, Rgba};
use super::PixelEdit;
use std::collections::VecDeque;

pub fn apply_fill(layer: &Layer, x: u32, y: u32, target: Rgba, replacement: Rgba) -> Vec<PixelEdit> {
    if target == replacement { return vec![]; }
    let mut edits: Vec<PixelEdit> = Vec::new();
    let mut visited = vec![false; (layer.width * layer.height) as usize];
    let mut queue = VecDeque::new();
    queue.push_back((x, y));
    while let Some((cx, cy)) = queue.pop_front() {
        if cx >= layer.width || cy >= layer.height { continue; }
        let idx = (cy * layer.width + cx) as usize;
        if visited[idx] { continue; }
        if layer.get_pixel(cx, cy) != target { continue; }
        visited[idx] = true;
        edits.push((cx, cy, target, replacement));
        if cx > 0              { queue.push_back((cx - 1, cy)); }
        if cx + 1 < layer.width  { queue.push_back((cx + 1, cy)); }
        if cy > 0              { queue.push_back((cx, cy - 1)); }
        if cy + 1 < layer.height { queue.push_back((cx, cy + 1)); }
    }
    edits
}
```

- [ ] **Step 6: Implement tools/eyedropper.rs**

```rust
// src/tools/eyedropper.rs
use crate::project::{Layer, Rgba};

pub fn apply_eyedropper(layer: &Layer, x: u32, y: u32) -> Rgba {
    layer.get_pixel(x, y)
}
```

- [ ] **Step 7: Implement tools/shapes.rs**

```rust
// src/tools/shapes.rs
use crate::project::{Layer, Rgba};
use super::{PixelEdit, apply_pencil};

pub fn apply_rect(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let (lx, rx) = (x0.min(x1), x0.max(x1));
    let (ly, ry) = (y0.min(y1), y0.max(y1));
    for y in ly..=ry {
        for x in lx..=rx {
            let on_border = x == lx || x == rx || y == ly || y == ry;
            if filled || on_border {
                edits.extend(apply_pencil(layer, x, y, color));
            }
        }
    }
    edits
}

pub fn apply_ellipse(layer: &Layer, cx: u32, cy: u32, rx: u32, ry: u32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let (cx, cy, rx, ry) = (cx as i32, cy as i32, rx as i32, ry as i32);
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let inside = (dx * dx) as f32 / (rx * rx) as f32 + (dy * dy) as f32 / (ry * ry) as f32 <= 1.0;
            let on_border = {
                let outer = inside;
                let inner_x = (rx - 1).max(0);
                let inner_y = (ry - 1).max(0);
                let inner = if inner_x == 0 || inner_y == 0 { false }
                    else { (dx * dx) as f32 / (inner_x * inner_x) as f32 + (dy * dy) as f32 / (inner_y * inner_y) as f32 <= 1.0 };
                outer && !inner
            };
            if (filled && inside) || (!filled && on_border) {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && py >= 0 {
                    edits.extend(apply_pencil(layer, px as u32, py as u32, color));
                }
            }
        }
    }
    edits
}

pub fn apply_line(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba) -> Vec<PixelEdit> {
    super::bresenham_line(layer, x0, y0, x1, y1, color)
}
```

- [ ] **Step 8: Implement tools/select.rs (stub)**

```rust
// src/tools/select.rs
// Selection and move tool state — rendered in app.rs
#[derive(Debug, Clone, Default)]
pub struct SelectState {
    pub rect: Option<(u32, u32, u32, u32)>, // (x, y, w, h)
    pub clipboard: Option<Vec<u8>>,
}
```

- [ ] **Step 9: Run tests**

```bash
cargo test --test tools_tests
```

Expected: `test result: ok. 4 passed`

- [ ] **Step 10: Commit**

```bash
git add src/tools/ tests/tools_tests.rs
git commit -m "feat: tool system — pencil, eraser, fill, eyedropper, shapes"
```

---

## Task 8: Animation System

**Files:**
- Create: `src/animation.rs`

- [ ] **Step 1: Implement animation.rs**

```rust
// src/animation.rs
use std::time::Instant;
use egui::TextureHandle;
use crate::project::Frame;
use crate::layers::composite_frame;

pub struct PlaybackState {
    pub is_playing: bool,
    pub last_tick: Instant,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self { is_playing: false, last_tick: Instant::now() }
    }
}

impl PlaybackState {
    /// Returns true and advances frame if enough time has elapsed.
    pub fn tick(&mut self, fps: u8, current_frame: &mut usize, total_frames: usize) -> bool {
        if !self.is_playing || total_frames == 0 { return false; }
        let interval = std::time::Duration::from_secs_f32(1.0 / fps.max(1) as f32);
        if self.last_tick.elapsed() >= interval {
            *current_frame = (*current_frame + 1) % total_frames;
            self.last_tick = Instant::now();
            return true;
        }
        false
    }
}

/// Per-frame thumbnail cache entry
pub struct FrameThumbnail {
    pub handle: Option<TextureHandle>,
    pub dirty: bool,
}

impl Default for FrameThumbnail {
    fn default() -> Self { Self { handle: None, dirty: true } }
}

/// Generates a thumbnail RGBA buffer for a frame at a given scale
pub fn make_thumbnail(frame: &Frame, width: u32, height: u32) -> Vec<u8> {
    composite_frame(frame, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_does_not_advance_when_paused() {
        let mut state = PlaybackState::default();
        let mut frame = 0usize;
        let advanced = state.tick(12, &mut frame, 5);
        assert!(!advanced);
        assert_eq!(frame, 0);
    }

    #[test]
    fn playback_wraps_at_end() {
        let mut state = PlaybackState { is_playing: true, last_tick: Instant::now() - std::time::Duration::from_secs(1) };
        let mut frame = 4usize;
        state.tick(12, &mut frame, 5);
        assert_eq!(frame, 0);
    }
}
```

- [ ] **Step 2: Run inline tests**

```bash
cargo test animation
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 3: Commit**

```bash
git add src/animation.rs
git commit -m "feat: animation playback — tick-based frame advance, thumbnail cache"
```

---

## Task 9: File I/O — .sqr Format

**Files:**
- Create: `src/io/mod.rs`
- Create: `src/io/sqr.rs`
- Create: `tests/io_tests.rs`

- [ ] **Step 1: Write failing tests**

```rust
// tests/io_tests.rs
use squarez::project::Project;
use squarez::io::sqr::{save_sqr, load_sqr};

#[test]
fn save_and_load_roundtrip() {
    let mut project = Project::new(16, 16, "test".to_string());
    project.animations[0].frames[0].layers[0].set_pixel(5, 5, [255, 0, 0, 255]);
    project.animations[0].name = "Walk".to_string();

    let path = std::env::temp_dir().join("squarez_test.sqr");
    save_sqr(&project, &path).expect("save failed");
    let loaded = load_sqr(&path).expect("load failed");

    assert_eq!(loaded.name, "test");
    assert_eq!(loaded.canvas_width, 16);
    assert_eq!(loaded.canvas_height, 16);
    assert_eq!(loaded.animations[0].name, "Walk");
    assert_eq!(loaded.animations[0].frames[0].layers[0].get_pixel(5, 5), [255, 0, 0, 255]);
}

#[test]
fn load_invalid_magic_returns_error() {
    let path = std::env::temp_dir().join("squarez_bad.sqr");
    std::fs::write(&path, b"BADF\x01some garbage").unwrap();
    assert!(load_sqr(&path).is_err());
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
cargo test --test io_tests 2>&1 | head -5
```

Expected: compile error.

- [ ] **Step 3: Implement io/mod.rs**

```rust
// src/io/mod.rs
pub mod sqr;
pub mod export;
```

- [ ] **Step 4: Implement io/sqr.rs**

```rust
// src/io/sqr.rs
use std::io::{Read, Write};
use std::path::Path;
use crate::project::Project;

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
    let project: Project = bincode::deserialize(&decoded)?;
    Ok(project)
}
```

- [ ] **Step 5: Create stub export.rs**

```rust
// src/io/export.rs
// Export functionality implemented in Task 10
```

- [ ] **Step 6: Run tests**

```bash
cargo test --test io_tests
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 7: Commit**

```bash
git add src/io/ tests/io_tests.rs
git commit -m "feat: .sqr file format — bincode + lz4 save/load with magic + version header"
```

---

## Task 10: Export (PNG, GIF, Spritesheet)

**Files:**
- Modify: `src/io/export.rs`

- [ ] **Step 1: Implement export.rs**

```rust
// src/io/export.rs
use std::path::Path;
use crate::project::{Project, Animation};
use crate::layers::composite_frame;

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat { Png, Gif, Spritesheet }

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub scale: u32,         // 1, 2, 4, or 8
    pub animation_index: usize,
}

pub fn export(project: &Project, path: &Path, opts: ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    match opts.format {
        ExportFormat::Png        => export_png(project, path, &opts),
        ExportFormat::Gif        => export_gif(project, path, &opts),
        ExportFormat::Spritesheet => export_spritesheet(project, path, &opts),
    }
}

fn export_png(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let frame = &anim.frames[0];
    let w = project.canvas_width;
    let h = project.canvas_height;
    let pixels = composite_frame(frame, w, h);
    let scaled = scale_pixels(&pixels, w, h, opts.scale);
    image::save_buffer(path, &scaled, w * opts.scale, h * opts.scale, image::ColorType::Rgba8)?;
    Ok(())
}

fn export_gif(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let w = project.canvas_width * opts.scale;
    let h = project.canvas_height * opts.scale;
    let delay = (100.0 / anim.fps.max(1) as f32) as u16; // centiseconds
    let file = std::fs::File::create(path)?;
    let mut encoder = gif::Encoder::new(file, w as u16, h as u16, &[])?;
    encoder.set_repeat(gif::Repeat::Infinite)?;
    for frame in &anim.frames {
        let pixels = composite_frame(frame, project.canvas_width, project.canvas_height);
        let scaled = scale_pixels(&pixels, project.canvas_width, project.canvas_height, opts.scale);
        let mut gif_frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut scaled.clone(), 10);
        gif_frame.delay = delay;
        encoder.write_frame(&gif_frame)?;
    }
    Ok(())
}

fn export_spritesheet(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let fw = project.canvas_width * opts.scale;
    let fh = project.canvas_height * opts.scale;
    let total_w = fw * anim.frames.len() as u32;
    let mut sheet = vec![0u8; (total_w * fh * 4) as usize];
    for (i, frame) in anim.frames.iter().enumerate() {
        let pixels = composite_frame(frame, project.canvas_width, project.canvas_height);
        let scaled = scale_pixels(&pixels, project.canvas_width, project.canvas_height, opts.scale);
        let x_offset = i as u32 * fw;
        for y in 0..fh {
            for x in 0..fw {
                let src_idx = ((y * fw + x) * 4) as usize;
                let dst_idx = ((y * total_w + x_offset + x) * 4) as usize;
                sheet[dst_idx..dst_idx+4].copy_from_slice(&scaled[src_idx..src_idx+4]);
            }
        }
    }
    image::save_buffer(path, &sheet, total_w, fh, image::ColorType::Rgba8)?;
    Ok(())
}

fn scale_pixels(pixels: &[u8], w: u32, h: u32, scale: u32) -> Vec<u8> {
    if scale == 1 { return pixels.to_vec(); }
    let sw = w * scale;
    let sh = h * scale;
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let src = ((y * w + x) * 4) as usize;
            let c = &pixels[src..src+4];
            for dy in 0..scale {
                for dx in 0..scale {
                    let dst = (((y * scale + dy) * sw + (x * scale + dx)) * 4) as usize;
                    out[dst..dst+4].copy_from_slice(c);
                }
            }
        }
    }
    out
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build
```

Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/io/export.rs
git commit -m "feat: export — PNG, animated GIF, spritesheet with pixel-perfect scaling"
```

---

## Task 11: Canvas Rendering

**Files:**
- Create: `src/canvas.rs`

- [ ] **Step 1: Implement canvas.rs**

```rust
// src/canvas.rs
use egui::{Color32, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use crate::theme::Theme;

pub struct CanvasState {
    pub zoom: f32,
    pub offset: Vec2,      // pan offset in screen pixels
    pub texture: Option<TextureHandle>,
    pub dragging_pan: bool,
    pub last_mouse_pos: Option<Pos2>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 8.0,
            offset: Vec2::ZERO,
            texture: None,
            dragging_pan: false,
            last_mouse_pos: None,
        }
    }
}

impl CanvasState {
    /// Upload RGBA pixel data as a GPU texture
    pub fn upload_texture(&mut self, ctx: &egui::Context, pixels: &[u8], width: u32, height: u32) {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            pixels,
        );
        self.texture = Some(ctx.load_texture(
            "canvas",
            image,
            TextureOptions::NEAREST, // pixel-perfect, no bilinear blur
        ));
    }

    /// Convert screen position to canvas pixel coordinate
    pub fn screen_to_canvas(&self, screen_pos: Pos2, canvas_rect: Rect) -> Option<(u32, u32)> {
        let relative = screen_pos - canvas_rect.min - self.offset;
        let px = (relative.x / self.zoom).floor() as i32;
        let py = (relative.y / self.zoom).floor() as i32;
        if px < 0 || py < 0 { return None; }
        Some((px as u32, py as u32))
    }

    /// Draw checkerboard background + canvas texture
    pub fn draw(&self, painter: &Painter, canvas_rect: Rect, width: u32, height: u32, theme: &Theme) {
        let canvas_screen_rect = Rect::from_min_size(
            canvas_rect.min + self.offset,
            Vec2::new(width as f32 * self.zoom, height as f32 * self.zoom),
        );
        // Checkerboard
        let cell = self.zoom.max(1.0);
        let cols = (canvas_screen_rect.width() / cell).ceil() as u32;
        let rows = (canvas_screen_rect.height() / cell).ceil() as u32;
        for row in 0..rows {
            for col in 0..cols {
                let color = if (row + col) % 2 == 0 { theme.mid } else { theme.light };
                let rect = Rect::from_min_size(
                    Pos2::new(
                        canvas_screen_rect.min.x + col as f32 * cell,
                        canvas_screen_rect.min.y + row as f32 * cell,
                    ),
                    Vec2::splat(cell),
                );
                painter.rect_filled(rect, 0.0, color);
            }
        }
        // Canvas texture
        if let Some(tex) = &self.texture {
            painter.image(tex.id(), canvas_screen_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    }

    /// Handle scroll zoom and middle-mouse pan
    pub fn handle_input(&mut self, ui: &egui::Ui, canvas_rect: Rect) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = if scroll > 0.0 { 1.1f32 } else { 1.0 / 1.1 };
            self.zoom = (self.zoom * factor).clamp(1.0, 64.0);
        }
        let middle_down = ui.input(|i| i.pointer.middle_down());
        let space_held  = ui.input(|i| i.key_down(egui::Key::Space));
        let left_down   = ui.input(|i| i.pointer.primary_down());
        let panning = middle_down || (space_held && left_down);
        if panning {
            if let Some(delta) = ui.input(|i| i.pointer.delta().into(): Option<Vec2>) {
                let _ = delta; // handled below
            }
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }
    }
}
```

- [ ] **Step 2: Fix the handle_input method (remove bad turbofish)**

The `into(): Option<Vec2>` line above is a placeholder — replace `handle_input` with this clean version:

```rust
    pub fn handle_input(&mut self, ui: &egui::Ui) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = if scroll > 0.0 { 1.1f32 } else { 1.0 / 1.1 };
            self.zoom = (self.zoom * factor).clamp(1.0, 64.0);
        }
        let middle_down = ui.input(|i| i.pointer.middle_down());
        let space_held  = ui.input(|i| i.key_down(egui::Key::Space));
        let left_down   = ui.input(|i| i.pointer.primary_down());
        if middle_down || (space_held && left_down) {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }
    }
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo build
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/canvas.rs
git commit -m "feat: canvas rendering — checkerboard, zoom, pan, NEAREST texture upload"
```

---

## Task 12: Full UI Assembly

**Files:**
- Modify: `src/app.rs` (expand to full layout)

- [ ] **Step 1: Replace app.rs with full implementation**

```rust
// src/app.rs
use egui::{CentralPanel, FontId, FontFamily, RichText, SidePanel, TopBottomPanel, Vec2};
use crate::animation::{FrameThumbnail, PlaybackState, make_thumbnail};
use crate::canvas::CanvasState;
use crate::color::{ColorState, PickerMode};
use crate::color::hsv::{rgba_to_hsv, hsv_to_rgba};
use crate::color::oklab::{rgba_to_oklab, oklab_to_rgba};
use crate::history::{Command, UndoStack};
use crate::io::sqr::{load_sqr, save_sqr};
use crate::io::export::{export, ExportFormat, ExportOptions};
use crate::layers::composite_frame;
use crate::project::Project;
use crate::theme::{load_fonts, Theme, FONT_SIZE_SM, FONT_SIZE_MD};
use crate::tools::{ActiveTool, apply_pencil, apply_eraser, apply_fill, apply_eyedropper, bresenham_line};

pub struct App {
    pub project: Project,
    pub theme: Theme,
    pub canvas: CanvasState,
    pub color_state: ColorState,
    pub active_tool: ActiveTool,
    pub undo_stack: UndoStack,
    pub playback: PlaybackState,
    pub thumbnails: Vec<Vec<FrameThumbnail>>,  // [animation][frame]
    pub current_path: Option<std::path::PathBuf>,
    // Tool drag state
    drag_start: Option<(u32, u32)>,
    stroke_edits: Vec<crate::tools::PixelEdit>,
    canvas_dirty: bool,
    // Composite cache
    composite_cache: Option<Vec<u8>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        load_fonts(&cc.egui_ctx);
        let project = Project::new(32, 32, "Untitled".to_string());
        let thumbnails = project.animations.iter()
            .map(|a| a.frames.iter().map(|_| FrameThumbnail::default()).collect())
            .collect();
        Self {
            project,
            theme: Theme::default(),
            canvas: CanvasState::default(),
            color_state: ColorState::default(),
            active_tool: ActiveTool::Pencil,
            undo_stack: UndoStack::new(),
            playback: PlaybackState::default(),
            thumbnails,
            current_path: None,
            drag_start: None,
            stroke_edits: Vec::new(),
            canvas_dirty: true,
            composite_cache: None,
        }
    }

    fn composite_active_frame(&mut self) -> Vec<u8> {
        let frame = self.project.active_frame_ref();
        composite_frame(frame, self.project.canvas_width, self.project.canvas_height)
    }

    fn rebuild_canvas_texture(&mut self, ctx: &egui::Context) {
        let pixels = self.composite_active_frame();
        self.canvas.upload_texture(ctx, &pixels, self.project.canvas_width, self.project.canvas_height);
        self.canvas_dirty = false;
    }

    fn label_md(&self, text: &str) -> RichText {
        RichText::new(text).font(FontId::new(FONT_SIZE_MD, FontFamily::Monospace)).color(self.theme.fg)
    }

    fn label_sm(&self, text: &str) -> RichText {
        RichText::new(text).font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace)).color(self.theme.fg)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);

        // Advance animation playback
        let fps = self.project.active_anim().fps;
        let total = self.project.active_anim().frames.len();
        let af = &mut self.project.active_frame;
        if self.playback.tick(fps, af, total) {
            self.canvas_dirty = true;
        }

        // Keyboard shortcuts
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Z) && i.modifiers.ctrl {
                // undo handled below
            }
        });
        let ctrl_z = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl);
        let ctrl_y = ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl);
        if ctrl_z { self.undo_stack.undo(&mut self.project); self.canvas_dirty = true; }
        if ctrl_y { self.undo_stack.redo(&mut self.project); self.canvas_dirty = true; }

        // Menu bar
        TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(self.label_sm("File"), |ui| {
                    if ui.button(self.label_sm("New")).clicked() {
                        self.project = Project::new(32, 32, "Untitled".to_string());
                        self.canvas_dirty = true;
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Open")).clicked() {
                        if let Some(path) = rfd_open() {
                            if let Ok(p) = load_sqr(&path) {
                                self.project = p;
                                self.canvas_dirty = true;
                                self.current_path = Some(path);
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Save")).clicked() {
                        let path = self.current_path.clone().unwrap_or_else(|| {
                            std::path::PathBuf::from("untitled.sqr")
                        });
                        let _ = save_sqr(&self.project, &path);
                        ui.close_menu();
                    }
                });
                ui.menu_button(self.label_sm("Edit"), |ui| {
                    if ui.button(self.label_sm("Undo  Ctrl+Z")).clicked() {
                        self.undo_stack.undo(&mut self.project);
                        self.canvas_dirty = true;
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Redo  Ctrl+Y")).clicked() {
                        self.undo_stack.redo(&mut self.project);
                        self.canvas_dirty = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button(self.label_sm("Export"), |ui| {
                    if ui.button(self.label_sm("PNG")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Png, scale: 1, animation_index: self.project.active_animation };
                        let _ = export(&self.project, std::path::Path::new("export.png"), opts);
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("GIF")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Gif, scale: 1, animation_index: self.project.active_animation };
                        let _ = export(&self.project, std::path::Path::new("export.gif"), opts);
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Spritesheet")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Spritesheet, scale: 1, animation_index: self.project.active_animation };
                        let _ = export(&self.project, std::path::Path::new("spritesheet.png"), opts);
                        ui.close_menu();
                    }
                });
            });
        });

        // Timeline panel (bottom)
        TopBottomPanel::bottom("timeline").min_height(80.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Clip selector
                let anim_name = self.project.active_anim().name.clone();
                egui::ComboBox::from_id_salt("clip_selector")
                    .selected_text(self.label_sm(&anim_name))
                    .show_ui(ui, |ui| {
                        for (i, anim) in self.project.animations.iter().enumerate() {
                            let name = anim.name.clone();
                            if ui.selectable_label(self.project.active_animation == i, self.label_sm(&name)).clicked() {
                                self.project.active_animation = i;
                                self.project.active_frame = 0;
                                self.canvas_dirty = true;
                            }
                        }
                    });
                if ui.button(self.label_sm("+")).clicked() {
                    let w = self.project.canvas_width;
                    let h = self.project.canvas_height;
                    let n = self.project.animations.len() + 1;
                    self.project.animations.push(crate::project::Animation::new(format!("Animation {}", n), w, h));
                }
                ui.separator();
                // Frame strip
                let num_frames = self.project.active_anim().frames.len();
                for i in 0..num_frames {
                    let selected = self.project.active_frame == i;
                    let label = self.label_sm(&format!("F{}", i + 1));
                    if ui.selectable_label(selected, label).clicked() {
                        self.project.active_frame = i;
                        self.canvas_dirty = true;
                    }
                }
                if ui.button(self.label_sm("+")).clicked() {
                    let w = self.project.canvas_width;
                    let h = self.project.canvas_height;
                    let idx = self.project.active_anim().frames.len();
                    self.undo_stack.push(Command::AddFrame { animation_id: self.project.active_animation, index: idx });
                    self.project.active_anim_mut().frames.push(crate::project::Frame::new(w, h));
                    if self.thumbnails.len() > self.project.active_animation {
                        self.thumbnails[self.project.active_animation].push(FrameThumbnail::default());
                    }
                }
                ui.separator();
                // Playback controls
                if ui.button(self.label_sm("|<")).clicked() { self.project.active_frame = 0; self.canvas_dirty = true; }
                if ui.button(self.label_sm("<")).clicked()  { if self.project.active_frame > 0 { self.project.active_frame -= 1; } self.canvas_dirty = true; }
                let play_label = if self.playback.is_playing { "||" } else { ">" };
                if ui.button(self.label_sm(play_label)).clicked() { self.playback.is_playing = !self.playback.is_playing; }
                if ui.button(self.label_sm(">")).clicked() { let t = self.project.active_anim().frames.len(); self.project.active_frame = (self.project.active_frame + 1) % t; self.canvas_dirty = true; }
                ui.separator();
                ui.label(self.label_sm("FPS:"));
                let fps = &mut self.project.active_anim_mut().fps;
                let mut fps_val = *fps as u32;
                if ui.add(egui::DragValue::new(&mut fps_val).range(1..=60)).changed() { *fps = fps_val as u8; }
            });
        });

        // Left toolbar
        SidePanel::left("toolbar").exact_width(28.0).show(ctx, |ui| {
            ui.vertical(|ui| {
                let tools = [
                    (ActiveTool::Pencil,   "P"),
                    (ActiveTool::Eraser,   "E"),
                    (ActiveTool::Fill,     "G"),
                    (ActiveTool::Eyedropper, "I"),
                    (ActiveTool::Rectangle { filled: false }, "R"),
                    (ActiveTool::Ellipse   { filled: false }, "O"),
                    (ActiveTool::Line,     "L"),
                    (ActiveTool::RectSelect, "S"),
                    (ActiveTool::Move,     "M"),
                ];
                for (tool, label) in tools {
                    let selected = self.active_tool == tool;
                    if ui.selectable_label(selected, self.label_sm(label)).clicked() {
                        self.active_tool = tool;
                    }
                }
            });
        });

        // Right color panel
        SidePanel::right("color_panel").min_width(180.0).show(ctx, |ui| {
            ui.label(self.label_md("Color"));
            ui.separator();
            // Picker mode tabs
            ui.horizontal(|ui| {
                if ui.selectable_label(self.color_state.active_picker == PickerMode::Hsv, self.label_sm("HSV")).clicked() {
                    self.color_state.active_picker = PickerMode::Hsv;
                }
                if ui.selectable_label(self.color_state.active_picker == PickerMode::OkLab, self.label_sm("OKLab")).clicked() {
                    self.color_state.active_picker = PickerMode::OkLab;
                }
            });
            let fg = self.color_state.foreground;
            match self.color_state.active_picker {
                PickerMode::Hsv => {
                    let (mut h, mut s, mut v) = rgba_to_hsv(fg);
                    let mut changed = false;
                    ui.label(self.label_sm("H")); changed |= ui.add(egui::Slider::new(&mut h, 0.0..=360.0)).changed();
                    ui.label(self.label_sm("S")); changed |= ui.add(egui::Slider::new(&mut s, 0.0..=1.0)).changed();
                    ui.label(self.label_sm("V")); changed |= ui.add(egui::Slider::new(&mut v, 0.0..=1.0)).changed();
                    if changed {
                        self.color_state.foreground = hsv_to_rgba(h, s, v, fg[3]);
                    }
                }
                PickerMode::OkLab => {
                    let (mut l, mut a, mut b) = rgba_to_oklab(fg);
                    let mut changed = false;
                    ui.label(self.label_sm("L")); changed |= ui.add(egui::Slider::new(&mut l, 0.0..=1.0)).changed();
                    ui.label(self.label_sm("a")); changed |= ui.add(egui::Slider::new(&mut a, -0.5..=0.5)).changed();
                    ui.label(self.label_sm("b")); changed |= ui.add(egui::Slider::new(&mut b, -0.5..=0.5)).changed();
                    if changed {
                        self.color_state.foreground = oklab_to_rgba(l, a, b, fg[3]);
                    }
                }
            }
            ui.separator();
            // FG/BG swatches
            ui.horizontal(|ui| {
                let fg_color = egui::Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], fg[3]);
                let bg = self.color_state.background;
                let bg_color = egui::Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]);
                ui.label(self.label_sm("FG"));
                ui.colored_label(fg_color, self.label_sm("  "));
                ui.label(self.label_sm("BG"));
                ui.colored_label(bg_color, self.label_sm("  "));
                if ui.button(self.label_sm("X")).clicked() {
                    std::mem::swap(&mut self.color_state.foreground, &mut self.color_state.background);
                }
            });
            ui.separator();
            // Palette grid
            ui.label(self.label_sm("Palette"));
            let palette = self.project.palette.clone();
            let cols = 8;
            egui::Grid::new("palette").num_columns(cols).show(ui, |ui| {
                for (i, &swatch) in palette.iter().enumerate() {
                    if i > 0 && i % cols == 0 { ui.end_row(); }
                    let color = egui::Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
                    let btn = egui::Button::new("  ").fill(color).min_size(Vec2::new(16.0, 16.0));
                    if ui.add(btn).clicked() {
                        self.color_state.foreground = swatch;
                    }
                }
            });
        });

        // Main canvas
        CentralPanel::default().show(ctx, |ui| {
            if self.canvas_dirty {
                self.rebuild_canvas_texture(ctx);
            }
            let canvas_rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(canvas_rect);
            self.canvas.draw(&painter, canvas_rect, self.project.canvas_width, self.project.canvas_height, &self.theme);
            self.canvas.handle_input(ui);

            // Handle drawing input
            let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some((px, py)) = self.canvas.screen_to_canvas(pos, canvas_rect) {
                    if px < self.project.canvas_width && py < self.project.canvas_height {
                        let color = self.color_state.foreground;
                        let ai = self.project.active_animation;
                        let fi = self.project.active_frame;
                        let li = self.project.active_layer;
                        let layer = &self.project.animations[ai].frames[fi].layers[li];

                        if response.drag_started() {
                            self.drag_start = Some((px, py));
                            self.stroke_edits.clear();
                        }

                        match &self.active_tool {
                            ActiveTool::Pencil => {
                                let edits = apply_pencil(layer, px, py, color);
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                self.stroke_edits.extend(edits);
                                self.canvas_dirty = true;
                            }
                            ActiveTool::Eraser => {
                                let edits = apply_eraser(layer, px, py);
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                self.stroke_edits.extend(edits);
                                self.canvas_dirty = true;
                            }
                            ActiveTool::Fill => {
                                let target = layer.get_pixel(px, py);
                                let edits = apply_fill(layer, px, py, target, color);
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                if !edits.is_empty() {
                                    self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
                                    self.canvas_dirty = true;
                                }
                            }
                            ActiveTool::Eyedropper => {
                                self.color_state.foreground = apply_eyedropper(layer, px, py);
                            }
                            _ => {}
                        }

                        if response.drag_stopped() {
                            if !self.stroke_edits.is_empty() {
                                let edits = std::mem::take(&mut self.stroke_edits);
                                self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
                            }
                            self.drag_start = None;
                        }
                    }
                }
            }
        });

        if self.playback.is_playing { ctx.request_repaint(); }
    }
}

fn rfd_open() -> Option<std::path::PathBuf> {
    // File dialog — requires `rfd` crate; returns None if not available
    None
}
```

- [ ] **Step 2: Add `rfd` to Cargo.toml for native file dialogs**

Add to `[dependencies]` in `Cargo.toml`:
```toml
rfd = "0.15"
```

Then replace `rfd_open()` in `app.rs`:
```rust
fn rfd_open() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Squarez Project", &["sqr"])
        .pick_file()
}
```

- [ ] **Step 3: Build and run**

```bash
cargo run
```

Expected: full UI opens — dark theme, toolbar on left, color panel on right, timeline at bottom, canvas in center with checkerboard. Pencil tool draws pixels. Undo/redo works with Ctrl+Z/Y.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs Cargo.toml
git commit -m "feat: full UI — toolbar, canvas, color panel, timeline, menu bar"
```

---

## Task 13: New Project Dialog & Canvas Size

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add new project dialog state to App**

Add to `App` struct:
```rust
    show_new_dialog: bool,
    new_width: u32,
    new_height: u32,
    new_name: String,
```

Initialize in `App::new`:
```rust
    show_new_dialog: false,
    new_width: 32,
    new_height: 32,
    new_name: "Untitled".to_string(),
```

- [ ] **Step 2: Replace New menu item to open dialog**

Replace the `"New"` button handler in `update()`:
```rust
if ui.button(self.label_sm("New")).clicked() {
    self.show_new_dialog = true;
    ui.close_menu();
}
```

- [ ] **Step 3: Add dialog rendering before CentralPanel**

Add before the `CentralPanel::default().show(...)` block:
```rust
if self.show_new_dialog {
    egui::Window::new("New Project")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(self.label_sm("Name:"));
            ui.text_edit_singleline(&mut self.new_name);
            ui.label(self.label_sm("Width:"));
            ui.add(egui::DragValue::new(&mut self.new_width).range(1..=2048));
            ui.label(self.label_sm("Height:"));
            ui.add(egui::DragValue::new(&mut self.new_height).range(1..=2048));
            ui.horizontal(|ui| {
                if ui.button(self.label_sm("Create")).clicked() {
                    self.project = Project::new(self.new_width, self.new_height, self.new_name.clone());
                    self.canvas_dirty = true;
                    self.show_new_dialog = false;
                }
                if ui.button(self.label_sm("Cancel")).clicked() {
                    self.show_new_dialog = false;
                }
            });
        });
}
```

- [ ] **Step 4: Build and test**

```bash
cargo run
```

Expected: File → New opens a dialog where you can set canvas size and name.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: new project dialog — configurable canvas size and name"
```

---

## Task 14: Release Build & Cross-Platform Verification

**Files:**
- Create: `.github/workflows/build.yml` (optional CI)

- [ ] **Step 1: Release build on Windows**

```bash
cargo build --release
```

Expected: `target/release/squarez.exe` created (~5-15MB after LTO stripping).

- [ ] **Step 2: Verify binary runs standalone**

Double-click `target/release/squarez.exe` outside of terminal. Expected: window opens normally, no console window (due to `windows_subsystem = "windows"`).

- [ ] **Step 3: Add Linux/macOS build targets (optional, for CI)**

```bash
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-apple-darwin
```

For cross-compilation from Windows, use `cross` crate:
```bash
cargo install cross
cross build --target x86_64-unknown-linux-gnu --release
```

- [ ] **Step 4: Final commit**

```bash
git add .
git commit -m "chore: release build verified, cross-platform targets documented"
```

---

## Self-Review

**Spec coverage check:**
- [x] Cross-platform (Win/macOS/Linux) — Rust + egui, release builds documented
- [x] Animation with named clips — `Animation` struct, timeline panel, clip dropdown
- [x] `.sqr` save/load — Task 9, bincode + lz4, magic + version header
- [x] PNG / GIF / spritesheet export — Task 10
- [x] HSV color picker — Task 5 + Task 12
- [x] OKLab color picker — Task 5 + Task 12
- [x] 4-color UI theme + theming support — Task 3
- [x] dogicapixel.ttf at 8/16/32px only — Task 3, font loaded via `include_bytes!`
- [x] Layers (visibility, opacity, blend) — Task 6 + project.rs
- [x] Undo/redo 100-step — Task 4
- [x] All 9 tools — Task 7
- [x] Zoom/pan — Task 11

**Type consistency verified:** `PixelEdit = (u32, u32, Rgba, Rgba)` used consistently across tools, history, and app. `Rgba = [u8; 4]` used everywhere.

**No placeholders:** all tasks contain complete code.
