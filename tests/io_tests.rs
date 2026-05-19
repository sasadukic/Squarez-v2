// tests/io_tests.rs
use squarez::project::Project;
use squarez::io::sqr::{save_sqr, load_sqr};

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
