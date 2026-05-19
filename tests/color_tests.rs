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
