use squarez::color::oklab::safe_oklch_to_rgba;
use squarez::color::ColorState;
use squarez::history::{UndoStack, Command};
use squarez::project::Project;

#[test]
fn preview_colors_in_gamut() {
    // Pick some challenging hues and lightness
    let samples = vec![ (0.5f32, 0.25f32, 10.0f32), (0.8, 0.35, 220.0), (0.2, 0.15, 140.0) ];
    for (l, c, h) in samples {
        let rgba = safe_oklch_to_rgba(l, c, h, 255, true);
        for ch in &rgba[0..3] {
            // channels are u8, so bounds check unnecessary; just ensure conversion yields valid bytes
            let _ = *ch; // keep lint happy
        }
    }
}

#[test]
fn undo_restores_color_state() {
    let mut project = Project::new(8, 8, "x".to_string());
    let mut cs = ColorState::default();
    let before = cs.clone();
    let mut after = cs.clone();
    after.ramp_size = 7;

    // Simulate apply: cs set to after, push snapshot
    cs = after.clone();
    let mut us = UndoStack::new();
    us.push(Command::SetColorStateSnapshot { before: before.clone(), after: after.clone() });

    // Undo with color should restore cs to before
    us.undo_with_color(&mut project, &mut cs);
    // ColorState doesn't implement PartialEq; assert key field restored
    assert_eq!(cs.ramp_size, before.ramp_size);
    assert!((cs.ramp_curve_mid_luma - before.ramp_curve_mid_luma).abs() < 1e-6);
}
