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
    DeleteLayer {
        animation_id: usize,
        frame_id: usize,
        index: usize,
        snapshot: crate::project::Layer,
    },
    /// Snapshot the ColorState before/after a grouped color edit (undo/redo restores it).
    SetColorStateSnapshot {
        before: ColorState,
        after: ColorState,
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

enum Direction { Forward, Backward }

fn apply_command(project: &mut Project, color_state: Option<&mut ColorState>, cmd: &Command, dir: Direction) {
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
        Command::DeleteLayer { animation_id, frame_id, index, snapshot } => {
            let frame = &mut project.animations[*animation_id].frames[*frame_id];
            match dir {
                Direction::Forward  => { frame.layers.remove(*index); }
                Direction::Backward => frame.layers.insert(*index, snapshot.clone()),
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
    }
}
