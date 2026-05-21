// tests/io_tests.rs
use serde::{Deserialize, Serialize};
use squarez::project::{BlendMode, Project};
use squarez::io::sqr::{save_sqr, load_sqr};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyProjectV1 {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<LegacyAnimationV1>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyAnimationV1 {
    name: String,
    fps: u8,
    frames: Vec<LegacyFrameV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyFrameV1 {
    duration_ms: u32,
    layers: Vec<LegacyLayerV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyLayerV1 {
    name: String,
    visible: bool,
    opacity: u8,
    blend_mode: BlendMode,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

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

#[test]
fn loads_legacy_v1_files_without_locked_layer_field() {
    let legacy = LegacyProjectV1 {
        name: "legacy".to_string(),
        canvas_width: 4,
        canvas_height: 4,
        palette: vec![[0, 0, 0, 255]],
        animations: vec![LegacyAnimationV1 {
            name: "Animation 1".to_string(),
            fps: 12,
            frames: vec![LegacyFrameV1 {
                duration_ms: 0,
                layers: vec![LegacyLayerV1 {
                    name: "Layer 1".to_string(),
                    visible: true,
                    opacity: 255,
                    blend_mode: BlendMode::Normal,
                    pixels: vec![0; 4 * 4 * 4],
                    width: 4,
                    height: 4,
                }],
            }],
        }],
        active_animation: 0,
        active_frame: 0,
        active_layer: 0,
    };

    let encoded = bincode::serialize(&legacy).unwrap();
    let compressed = lz4_flex::compress_prepend_size(&encoded);
    let path = std::env::temp_dir().join("squarez_legacy_v1.sqr");
    let mut bytes = b"SQR\0\x01".to_vec();
    bytes.extend(compressed);
    std::fs::write(&path, bytes).unwrap();

    let loaded = load_sqr(&path).expect("legacy load failed");

    assert_eq!(loaded.name, "legacy");
    assert!(!loaded.animations[0].frames[0].layers[0].locked);
}
