// tests/tools_tests.rs
use squarez::tools::{apply_pencil, apply_eraser, apply_fill};
use squarez::project::Layer;

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
    let layer = Layer::new("L".to_string(), 4, 4);
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
