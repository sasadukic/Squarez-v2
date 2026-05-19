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
