# Squarez Pixel Art Editor — Design Document

*Date: 2026-05-19*
*Status: Approved*

---

## Overview

A standalone cross-platform pixel art editor for Windows, macOS, and Linux. Built with Rust + egui. Supports layers, multiple named animation clips per project, HSV and OKLab color pickers, and exports to PNG, GIF, and spritesheets. Projects saved as `.sqr` (custom binary format).

---

## Tech Stack

| Concern | Crate / Tool |
|---|---|
| GUI framework | `eframe` + `egui` |
| Canvas rendering | egui texture API (GPU-backed) |
| PNG encode/decode | `image` |
| GIF export | `gif` |
| Project serialization | `bincode` + `lz4_flex` |
| Color math | `palette` (OKLab, HSV, sRGB conversions) |
| Font | `dogicapixel.ttf` — 8px / 16px / 32px only |

---

## UI Design

### Theme System

4-color palette, fully swappable to support theming. Applied globally via egui `Visuals`.

| Slot | Prototype value | Role |
|---|---|---|
| `color_bg` | `#000000` | Canvas background, deepest panels |
| `color_mid` | `#676767` | Toolbar background, panel fills |
| `color_light` | `#b6b6b6` | Borders, inactive elements, dividers |
| `color_fg` | `#ffffff` | Text, active icons, highlights |

Canvas checkerboard transparency pattern uses `color_mid` and `color_light`.

### Typography

Font: `dogicapixel.ttf` (embedded at compile time via `include_bytes!`).
Allowed sizes: **8px** (body/labels), **16px** (panel headers, tool tips), **32px** (large headings only).
No other sizes — the font does not render correctly at arbitrary sizes.

### Layout

```
┌──────────────────────────────────────────────────────────┐
│ Menu bar: File / Edit / View / Export                     │
├──────┬───────────────────────────────────┬───────────────┤
│      │                                   │  Color Panel  │
│  T   │                                   │  [HSV|OKLab]  │
│  o   │          Canvas (center)          │  ──────────── │
│  o   │       checkerboard + pixels       │  Hex / RGB    │
│  l   │       zoom / pan with mouse       │  FG / BG swap │
│  b   │                                   │  ──────────── │
│  a   │                                   │  Palette grid │
│  r   │                                   │  (256 swatches│
│      │                                   │   max)        │
├──────┴───────────────────────────────────┴───────────────┤
│ [Clip ▼][+][Rename][Del] │ [F1][F2][F3][+] │ [▶] FPS:[12]│
└──────────────────────────────────────────────────────────┘
```

### Left Toolbar Tools (top to bottom)

Pencil, Eraser, Fill Bucket, Eyedropper, Rectangle, Ellipse, Line, Rect Select, Move

---

## Data Model

All project state lives in a single `Project` struct owned by `AppState`.

```
Project
├── name: String
├── canvas_width: u32
├── canvas_height: u32
├── palette: Vec<RGBA>              ← shared color swatches (up to 256)
├── active_animation: usize
├── active_frame: usize
├── active_layer: usize
└── animations: Vec<Animation>
        ├── name: String            ← "idle", "walk", "run", etc.
        ├── fps: u8
        └── frames: Vec<Frame>
                ├── duration_ms: u32   ← per-frame timing override
                └── layers: Vec<Layer>
                        ├── name: String
                        ├── visible: bool
                        ├── opacity: u8          ← 0–255
                        ├── blend_mode: BlendMode ← Normal, Multiply, Screen, …
                        └── pixels: Vec<u8>      ← RGBA flat array, width×height×4 bytes
```

Internal color representation is always 32-bit RGBA `[u8; 4]`. OKLab and HSV exist only as picker UI state and are converted to RGBA before writing to any pixel.

---

## Color System

### HSV Picker
- 2D square: X axis = saturation, Y axis = value (brightness), background tinted by current hue
- Horizontal hue bar below the square
- Alpha slider

### OKLab Picker
- Horizontal `L` (lightness) slider — perceptually linear, 0.0–1.0
- 2D `a`/`b` chromatic plane (green–red × blue–yellow axes)
- Perceptual uniformity: equal slider distance = equal perceived color change

### Shared Controls
- Hex input field (`#RRGGBBAA`)
- Numeric R / G / B fields (0–255)
- Foreground / background color swatch pair with swap button (keyboard: `X`)
- Palette grid: up to 256 swatches; left-click = set fg color; right-click = replace with current fg

Color math implemented via the `palette` crate (OKLab ↔ sRGB is a small matrix multiply).

---

## Tools & Input System

Each tool is a variant of a `Tool` enum. Tools receive `ToolInput` (canvas pixel coordinate, mouse button, modifier keys) and produce `Vec<PixelEdit>` consumed by the undo system.

| Tool | Behavior |
|---|---|
| Pencil | Single pixels; Bresenham line interpolation between drag points |
| Eraser | Sets pixels to transparent (alpha = 0) |
| Fill | Flood-fill from clicked pixel, 4-connected |
| Eyedropper | Samples pixel color into active foreground color |
| Rectangle | Filled or outline rectangle, drawn on mouse release |
| Ellipse | Filled or outline ellipse, drawn on mouse release |
| Line | Straight line between press and release points |
| Rect Select | Selects rectangular region; selected pixels can be moved or copied |
| Move | Moves entire active layer's pixels by drag offset |

**Zoom:** scroll wheel.
**Pan:** middle-mouse drag, or Space + left-drag.
All coordinates are converted from screen-space to canvas pixel-space before any tool processes them.

---

## Animation & Playback

Projects contain multiple named animation clips. Each clip has its own frame list and FPS setting.

**PlaybackState:**
```rust
struct PlaybackState {
    is_playing: bool,
    current_frame: usize,
    last_tick: Instant,
}
```

On each `update()`: if playing, check `elapsed >= 1.0 / fps` and advance frame (wraps to 0 at end of clip).

**Onion skinning:** previous frame rendered at ~30% opacity behind current frame. Toggle button in timeline. Configurable: 1 or 2 ghost frames.

**Frame thumbnails:** each frame composites all visible layers into a small RGBA bitmap, uploaded as an egui texture. Regenerated only when the frame is marked dirty (pixels changed).

**Timeline panel controls:**
- Clip dropdown + Add / Rename / Delete clip buttons
- Frame strip with thumbnails, click to jump, drag to reorder
- Add / Delete / Duplicate frame buttons
- Playback: `|◀` `◀◀` `▶` `▶▶` + FPS numeric input
- Onion skin toggle

---

## File I/O & Export

### `.sqr` Project Format

Binary layout:
```
[4 bytes magic: "SQR\0"]
[1 byte version: 0x01]
[remaining bytes: lz4-compressed bincode-serialized Project struct]
```

Per-layer LZ4 compression keeps file sizes small (pixel art has large flat-color regions that compress aggressively). The format is versioned for future compatibility. Not human-readable by design — open formats are available via export.

### Export

| Format | Crate | Notes |
|---|---|---|
| PNG | `image` | Single frame, all layers merged |
| Animated GIF | `gif` | All frames of active clip, respects per-frame FPS |
| Spritesheet | `image` | All frames tiled horizontally (or N×M grid) |
| Single frame PNG | `image` | Same as PNG export |

**Export dialog options:** output path, format selector, pixel scale factor (1× / 2× / 4× / 8× for pixel-perfect upscaling), frame range (active clip or all clips).

---

## Undo / Redo

Command-based (not full snapshots — snapshots would be prohibitively large).

```rust
enum Command {
    PaintPixels {
        animation_id: usize,
        frame_id: usize,
        layer_id: usize,
        edits: Vec<(u32, u32, [u8;4], [u8;4])>, // (x, y, old_rgba, new_rgba)
    },
    AddFrame    { animation_id: usize, index: usize },
    DeleteFrame { animation_id: usize, index: usize, snapshot: Frame },
    AddLayer    { animation_id: usize, frame_id: usize, index: usize },
    DeleteLayer { animation_id: usize, frame_id: usize, index: usize, snapshot: Layer },
    MoveLayer   { animation_id: usize, frame_id: usize, from: usize, to: usize },
    RenameLayer { layer_id: usize, old_name: String, new_name: String },
}
```

`UndoStack`: `Vec<Command>` with a cursor. Maximum depth: 100 commands. Paint stroke commands are **merged** while the mouse button is held — one complete stroke = one undo step.

---

## Module Structure

```
src/
├── main.rs              ← eframe entry point
├── app.rs               ← AppState, top-level update() loop, panel layout
├── canvas.rs            ← GPU texture management, zoom/pan, hit testing
├── tools/
│   ├── mod.rs           ← Tool enum, ToolInput, ToolOutput
│   ├── pencil.rs
│   ├── eraser.rs
│   ├── fill.rs
│   ├── eyedropper.rs
│   ├── shapes.rs        ← rectangle, ellipse, line
│   └── select.rs        ← rect select, move
├── layers.rs            ← layer compositing, blend modes
├── animation.rs         ← PlaybackState, onion skin, thumbnail generation
├── color/
│   ├── mod.rs           ← RGBA type, conversions
│   ├── hsv.rs           ← HSV picker widget
│   └── oklab.rs         ← OKLab picker widget
├── history.rs           ← Command enum, UndoStack
├── io/
│   ├── mod.rs
│   ├── sqr.rs           ← .sqr save/load
│   └── export.rs        ← PNG, GIF, spritesheet export
├── theme.rs             ← Theme struct, 4-color system, egui Visuals mapping
└── project.rs           ← Project, Animation, Frame, Layer data structs
```

---

## Constraints & Non-Goals

- Font sizes: **8px, 16px, 32px only** (dogicapixel.ttf rendering requirement)
- UI colors: **4 slots only** — no additional colors in the UI chrome
- No network features, no cloud sync
- No text tool (out of scope for v1)
- No custom brush shapes (out of scope for v1)
- Canvas size: no hard limit, but optimized for typical pixel art sizes (16×16 to 512×512)
