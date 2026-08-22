// src/history.rs
use crate::project::{Project, Rgba};
use crate::color::ColorState;

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
    DuplicateFrame {
        animation_id: usize,
        index: usize,
        snapshot: crate::project::Frame,
    },
    /// Adds/removes a blank layer at `index` across ALL animations and ALL frames.
    /// Keeps layer structure in sync so every animation always has the same layers.
    AddLayer {
        index: usize,
        name: String,
        id: u64,
    },
    /// Removes/restores the layer at `index` across ALL animations and ALL
    /// frames — the same global scope as AddLayer, or the per-frame layer
    /// structures diverge and every later index-based command hits the wrong
    /// layer. One snapshot per frame, in (animation, frame) iteration order.
    DeleteLayer {
        index: usize,
        snapshots: Vec<crate::project::Layer>,
    },
    /// Inserts/removes a copy of the layer at `index` (at `index + 1`) across
    /// ALL animations and frames. One pre-made copy per frame, in
    /// (animation, frame) iteration order — redo must reproduce the pixels as
    /// they were at duplication time, not whatever the source holds later.
    DuplicateLayer {
        index: usize,
        snapshots: Vec<crate::project::Layer>,
    },
    /// Merges the layer at `index` into the one below it (and removes it)
    /// across ALL animations and frames. Per frame, in iteration order: the
    /// removed top layer and the lower layer as it was before the merge.
    MergeLayerDown {
        index: usize,
        tops: Vec<crate::project::Layer>,
        bottoms: Vec<crate::project::Layer>,
    },
    /// Snapshot the ColorState before/after a grouped color edit (undo/redo restores it).
    SetColorStateSnapshot {
        before: ColorState,
        after: ColorState,
    },
    SwapColors {
        color_a: Rgba,
        color_b: Rgba,
    },
    /// One modeling gesture in ThreeD mode: full mesh snapshots before/after,
    /// plus any texture-island pixel moves that accompanied the topology change
    /// (so mesh and texture stay atomic through undo/redo).
    ///
    /// `canvas_before`/`canvas_after` capture the atlas dimensions around the
    /// gesture: an edit can grow the atlas (and the explicit atlas-size dialog
    /// is one of these commands too), and undo must shrink it back or the
    /// canvas silently stays resized. `pixel_edits` coordinates live in the
    /// AFTER space — redo resizes first then applies, undo reverts the edits
    /// first then resizes back (row re-striding preserves (x, y) content, so
    /// the two orders compose exactly).
    MeshEdit {
        before: crate::three_d::mesh::Mesh,
        after: crate::three_d::mesh::Mesh,
        layer_id: usize,
        canvas_before: (u32, u32),
        canvas_after: (u32, u32),
        pixel_edits: Vec<(u32, u32, Rgba, Rgba)>, // (x, y, old, new)
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

    /// Backward-compatible undo: does not touch ColorState snapshots.
    pub fn undo(&mut self, project: &mut Project) {
        if !self.can_undo() { return; }
        self.cursor -= 1;
        let cmd = self.commands[self.cursor].clone();
        apply_command(project, None, &cmd, Direction::Backward);
    }

    /// Backward-compatible redo: does not touch ColorState snapshots.
    pub fn redo(&mut self, project: &mut Project) {
        if !self.can_redo() { return; }
        let cmd = self.commands[self.cursor].clone();
        self.cursor += 1;
        apply_command(project, None, &cmd, Direction::Forward);
    }

    /// Extended undo that also restores ColorState snapshots when available.
    pub fn undo_with_color(&mut self, project: &mut Project, color_state: &mut ColorState) {
        if !self.can_undo() { return; }
        self.cursor -= 1;
        let cmd = self.commands[self.cursor].clone();
        apply_command(project, Some(color_state), &cmd, Direction::Backward);
    }

    /// Extended redo that also restores ColorState snapshots when available.
    pub fn redo_with_color(&mut self, project: &mut Project, color_state: &mut ColorState) {
        if !self.can_redo() { return; }
        let cmd = self.commands[self.cursor].clone();
        self.cursor += 1;
        apply_command(project, Some(color_state), &cmd, Direction::Forward);
    }
}

pub enum Direction { Forward, Backward }

pub fn apply_command(project: &mut Project, color_state: Option<&mut ColorState>, cmd: &Command, dir: Direction) {
    match cmd {
        Command::PaintPixels { animation_id, frame_id, layer_id, edits } => {
            if project.is_tiled() {
                let tile_w = project.tile_w;
                let tile_h = project.tile_h;
                let tiles_w = project.tiles_w;
                let tiles_h = project.tiles_h;
                for &(x, y, old, new) in edits {
                    let tx = x / tile_w;
                    let ty = y / tile_h;
                    let ox = x % tile_w;
                    let oy = y % tile_h;
                    if tx < tiles_w && ty < tiles_h {
                        let fi = (ty * tiles_w + tx) as usize;
                        if fi < project.animations[*animation_id].frames.len() {
                            let layer = &mut project.animations[*animation_id]
                                .frames[fi]
                                .layers[*layer_id];
                            let color = match dir { Direction::Forward => new, Direction::Backward => old };
                            layer.set_pixel(ox, oy, color);
                            project.animations[*animation_id].frames[fi].dirty = true;
                        }
                    }
                }
            } else {
                let layer = &mut project.animations[*animation_id]
                    .frames[*frame_id]
                    .layers[*layer_id];
                for &(x, y, old, new) in edits {
                    let color = match dir { Direction::Forward => new, Direction::Backward => old };
                    layer.set_pixel(x, y, color);
                }
                project.animations[*animation_id].frames[*frame_id].dirty = true;
            }
        }
        Command::AddFrame { animation_id, index } => {
            let (w, h) = (project.canvas_width, project.canvas_height);
            let anim = &mut project.animations[*animation_id];
            match dir {
                Direction::Forward  => anim.frames.insert(*index, crate::project::Frame::new(w, h, 0)),
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
        Command::DuplicateFrame { animation_id, index, snapshot } => {
            let anim = &mut project.animations[*animation_id];
            match dir {
                Direction::Forward => anim.frames.insert(*index, snapshot.clone()),
                Direction::Backward => { anim.frames.remove(*index); }
            }
        }
        Command::AddLayer { index, name, id } => {
            let (w, h) = (project.canvas_width, project.canvas_height);
            // Layer structure is global: every animation and every frame stays in sync.
            for anim in &mut project.animations {
                for frame in &mut anim.frames {
                    match dir {
                        Direction::Forward  => frame.layers.insert(*index, crate::project::Layer::new_with_id(name.clone(), w, h, *id)),
                        Direction::Backward => { if frame.layers.len() > *index { frame.layers.remove(*index); } }
                    }
                }
            }
        }
        Command::DuplicateLayer { index, snapshots } => {
            let mut snap = snapshots.iter();
            for anim in &mut project.animations {
                for frame in &mut anim.frames {
                    match dir {
                        Direction::Forward => {
                            if let Some(s) = snap.next() {
                                let at = (*index + 1).min(frame.layers.len());
                                frame.layers.insert(at, s.clone());
                            }
                        }
                        Direction::Backward => {
                            if frame.layers.len() > *index + 1 {
                                frame.layers.remove(*index + 1);
                            }
                        }
                    }
                    frame.dirty = true;
                }
            }
        }
        Command::MergeLayerDown { index, tops, bottoms } => {
            let mut top = tops.iter();
            let mut bottom = bottoms.iter();
            for anim in &mut project.animations {
                for frame in &mut anim.frames {
                    let (Some(t), Some(b)) = (top.next(), bottom.next()) else { continue };
                    match dir {
                        Direction::Forward => {
                            if *index >= 1 && frame.layers.len() > *index {
                                frame.layers[*index - 1] =
                                    crate::project::merge_layer_over(t, b);
                                frame.layers.remove(*index);
                            }
                        }
                        Direction::Backward => {
                            if *index >= 1 && frame.layers.len() >= *index {
                                frame.layers[*index - 1] = b.clone();
                                frame.layers.insert(*index, t.clone());
                            }
                        }
                    }
                    frame.dirty = true;
                }
            }
        }
        Command::DeleteLayer { index, snapshots } => {
            let mut snap = snapshots.iter();
            for anim in &mut project.animations {
                for frame in &mut anim.frames {
                    match dir {
                        Direction::Forward => {
                            if frame.layers.len() > *index {
                                frame.layers.remove(*index);
                            }
                        }
                        Direction::Backward => {
                            if let Some(s) = snap.next() {
                                let at = (*index).min(frame.layers.len());
                                frame.layers.insert(at, s.clone());
                            }
                        }
                    }
                    frame.dirty = true;
                }
            }
        }
        Command::SetColorStateSnapshot { before, after } => {
            if let Some(cs) = color_state {
                match dir {
                    Direction::Forward => *cs = after.clone(),
                    Direction::Backward => *cs = before.clone(),
                }
            }
        }
        Command::SwapColors { color_a, color_b } => {
            for anim in &mut project.animations {
                for frame in &mut anim.frames {
                    for layer in &mut frame.layers {
                        if !layer.is_group {
                            let w = layer.width;
                            let h = layer.height;
                            for y in 0..h {
                                for x in 0..w {
                                    let pixel = layer.get_pixel(x, y);
                                    if pixel == *color_a {
                                        layer.set_pixel(x, y, *color_b);
                                    } else if pixel == *color_b {
                                        layer.set_pixel(x, y, *color_a);
                                    }
                                }
                            }
                        }
                    }
                    frame.dirty = true;
                }
            }
        }
        Command::MeshEdit { before, after, layer_id, canvas_before, canvas_after, pixel_edits } => {
            let resize_to = |project: &mut Project, (w, h): (u32, u32)| {
                if (project.canvas_width, project.canvas_height) == (w, h) {
                    return;
                }
                for anim in &mut project.animations {
                    for frame in &mut anim.frames {
                        frame.resize_canvas(w, h);
                    }
                }
                project.canvas_width = w;
                project.canvas_height = h;
            };
            let apply_pixels = |project: &mut Project, forward: bool| {
                let frame = &mut project.animations[0].frames[0];
                if let Some(layer) = frame.layers.get_mut(*layer_id) {
                    for &(x, y, old, new) in pixel_edits {
                        layer.set_pixel(x, y, if forward { new } else { old });
                    }
                }
            };
            // pixel_edits live in the AFTER canvas space: grow before applying
            // them, revert them before shrinking back.
            match dir {
                Direction::Forward => {
                    resize_to(project, *canvas_after);
                    apply_pixels(project, true);
                    project.mesh3d = Some(after.clone());
                }
                Direction::Backward => {
                    apply_pixels(project, false);
                    resize_to(project, *canvas_before);
                    project.mesh3d = Some(before.clone());
                }
            }
            if let Some(frame) = project.animations.get_mut(0).and_then(|a| a.frames.get_mut(0)) {
                frame.dirty = true;
            }
        }
    }
}
