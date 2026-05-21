use squarez::ui_metrics::{COLOR_SLIDER_TRACK_HEIGHT, PALETTE_SWATCH_GAP, RIGHT_SECTION_STACK_GAP};

#[test]
fn palette_swatch_gap_is_zero() {
    assert_eq!(PALETTE_SWATCH_GAP, 0.0);
}

#[test]
fn color_slider_uses_flat_mockup_track_height() {
    assert_eq!(COLOR_SLIDER_TRACK_HEIGHT, 8.0);
}

#[test]
fn right_sections_stack_directly_under_palette() {
    assert_eq!(RIGHT_SECTION_STACK_GAP, 0.0);
}
