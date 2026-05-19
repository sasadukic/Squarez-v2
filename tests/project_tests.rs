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
