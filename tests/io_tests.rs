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

// ── v1 payload, late layout (Frame carried a mesh) ───────────────────────────
// Structural mirror of the Project layout the last v1-writing builds produced.

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldProjectWithMesh {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<OldAnimationWithMesh>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: OldProjectMode,
    sprite_stack_max_layers: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum OldProjectMode {
    Normal,
    SpriteStack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldAnimationWithMesh {
    name: String,
    fps: u8,
    frames: Vec<OldFrameWithMesh>,
    tile_start: usize,
    tile_end: usize,
    tile_visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldFrameWithMesh {
    duration_ms: u32,
    layers: Vec<OldLayerFull>,
    mesh: OldMesh3D,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldMesh3D {
    vertices: Vec<(f32, f32, f32)>,
    edges: Vec<(u64, u64)>,
    faces: Vec<(Vec<u64>, [u8; 4])>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OldLayerFull {
    name: String,
    visible: bool,
    locked: bool,
    opacity: u8,
    blend_mode: BlendMode,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    id: u64,
    is_group: bool,
    group_id: Option<u64>,
    collapsed: bool,
    background_color: Option<[u8; 4]>,
}

#[test]
fn loads_v1_files_with_per_frame_mesh() {
    let mut pixels = vec![0u8; 4 * 4 * 4];
    // pixel (1, 2) = red
    let idx = ((2 * 4 + 1) * 4) as usize;
    pixels[idx..idx + 4].copy_from_slice(&[255, 0, 0, 255]);

    let old = OldProjectWithMesh {
        name: "meshy".to_string(),
        canvas_width: 4,
        canvas_height: 4,
        palette: vec![[1, 2, 3, 255]],
        animations: vec![OldAnimationWithMesh {
            name: "Animation 1".to_string(),
            fps: 12,
            frames: vec![OldFrameWithMesh {
                duration_ms: 0,
                layers: vec![OldLayerFull {
                    name: "Layer 1".to_string(),
                    visible: true,
                    locked: false,
                    opacity: 255,
                    blend_mode: BlendMode::Normal,
                    pixels,
                    width: 4,
                    height: 4,
                    id: 7,
                    is_group: false,
                    group_id: None,
                    collapsed: false,
                    background_color: None,
                }],
                mesh: OldMesh3D {
                    vertices: vec![(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)],
                    edges: vec![(0, 1)],
                    faces: vec![(vec![0, 1, 0], [9, 9, 9, 255])],
                },
            }],
            tile_start: 0,
            tile_end: 0,
            tile_visible: true,
        }],
        active_animation: 0,
        active_frame: 0,
        active_layer: 0,
        layer_id_counter: 8,
        tiles_w: 1,
        tiles_h: 1,
        tile_w: 0,
        tile_h: 0,
        mode: OldProjectMode::SpriteStack,
        sprite_stack_max_layers: Some(8),
    };

    let encoded = bincode::serialize(&old).unwrap();
    let compressed = lz4_flex::compress_prepend_size(&encoded);
    let path = std::env::temp_dir().join("squarez_v1_with_mesh.sqr");
    let mut bytes = b"SQR\0\x01".to_vec();
    bytes.extend(compressed);
    std::fs::write(&path, bytes).unwrap();

    let loaded = load_sqr(&path).expect("v1-with-mesh load failed");

    assert_eq!(loaded.name, "meshy");
    assert_eq!(loaded.mode, squarez::project::ProjectMode::SpriteStack);
    assert_eq!(loaded.sprite_stack_max_layers, Some(8));
    assert_eq!(loaded.layer_id_counter, 8);
    assert_eq!(loaded.animations[0].frames[0].layers[0].id, 7);
    assert_eq!(loaded.animations[0].frames[0].layers[0].get_pixel(1, 2), [255, 0, 0, 255]);
    assert!(loaded.mesh3d.is_none(), "old per-frame mesh data is discarded");
}

#[test]
fn v2_roundtrip_preserves_mesh3d() {
    let mut project = Project::new(32, 32, "cube".to_string());
    project.mode = squarez::project::ProjectMode::ThreeD;
    let mut mesh = squarez::three_d::mesh::Mesh::cube(8);
    mesh.allocate_all_islands((32, 32)).expect("islands fit");
    project.mesh3d = Some(mesh.clone());

    let path = std::env::temp_dir().join("squarez_v2_mesh.sqr");
    save_sqr(&project, &path).expect("save failed");
    let loaded = load_sqr(&path).expect("load failed");

    assert_eq!(loaded.mode, squarez::project::ProjectMode::ThreeD);
    assert_eq!(loaded.mesh3d, Some(mesh));
}

// ── v1 payloads, tail-field progression ──────────────────────────────────────
// Three real-world builds serialized the current Layer/Frame/Animation shapes
// with a shorter Project tail. Recovered by byte-level dissection of files
// that failed to load: ends-at-tile_h, +mode, +mode+sprite_stack_max_layers.

use squarez::project::Animation;

fn tail_era_animations() -> Vec<Animation> {
    let mut p = Project::new(8, 8, "x".to_string());
    p.animations[0].frames[0].layers[0].set_pixel(2, 3, [9, 8, 7, 255]);
    p.animations.clone()
}

fn write_v1(path: &std::path::Path, payload: &[u8]) {
    let compressed = lz4_flex::compress_prepend_size(payload);
    let mut bytes = b"SQR\0\x01".to_vec();
    bytes.extend(compressed);
    std::fs::write(path, bytes).unwrap();
}

#[derive(Serialize)]
struct TailNoMode {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
}

#[derive(Serialize)]
enum TailMode {
    #[allow(dead_code)]
    Normal,
    SpriteStack,
}

#[derive(Serialize)]
struct TailWithMode {
    base: TailNoMode,
    mode: TailMode,
}

#[derive(Serialize)]
struct TailWithModeStack {
    base: TailNoMode,
    mode: TailMode,
    sprite_stack_max_layers: Option<u32>,
}

fn tail_base(name: &str) -> TailNoMode {
    TailNoMode {
        name: name.to_string(),
        canvas_width: 8,
        canvas_height: 8,
        palette: vec![[1, 2, 3, 255]],
        animations: tail_era_animations(),
        active_animation: 0,
        active_frame: 0,
        active_layer: 0,
        layer_id_counter: 2,
        tiles_w: 4,
        tiles_h: 1,
        tile_w: 2,
        tile_h: 8,
    }
}

#[test]
fn loads_v1_files_that_end_at_tile_h() {
    // Serde flattens nothing here: TailNoMode's own serialization IS the
    // historical byte stream (bincode is positional and untagged).
    let payload = bincode::serialize(&tail_base("soldier-era")).unwrap();
    let path = std::env::temp_dir().join("squarez_tail_nomode.sqr");
    write_v1(&path, &payload);

    let p = load_sqr(&path).expect("no-mode era must load");
    assert_eq!(p.name, "soldier-era");
    assert_eq!((p.tiles_w, p.tiles_h), (4, 1), "tile grid must survive");
    assert_eq!(p.mode, squarez::project::ProjectMode::Normal);
    assert_eq!(
        p.animations[0].frames[0].layers[0].get_pixel(2, 3),
        [9, 8, 7, 255],
        "pixels must survive"
    );
}

#[test]
fn loads_v1_files_with_mode_but_no_stack_limit() {
    // A struct nested first serializes identically to its fields inlined.
    let payload = bincode::serialize(&TailWithMode {
        base: tail_base("spritestack-era"),
        mode: TailMode::SpriteStack,
    })
    .unwrap();
    let path = std::env::temp_dir().join("squarez_tail_mode.sqr");
    write_v1(&path, &payload);

    let p = load_sqr(&path).expect("mode era must load");
    assert_eq!(p.mode, squarez::project::ProjectMode::SpriteStack, "mode must survive");
    assert_eq!(p.sprite_stack_max_layers, None);
}

#[test]
fn loads_v1_files_with_mode_and_stack_limit() {
    let payload = bincode::serialize(&TailWithModeStack {
        base: tail_base("lighthouse-era"),
        mode: TailMode::SpriteStack,
        sprite_stack_max_layers: Some(32),
    })
    .unwrap();
    let path = std::env::temp_dir().join("squarez_tail_modestack.sqr");
    write_v1(&path, &payload);

    let p = load_sqr(&path).expect("mode+stack era must load");
    assert_eq!(p.mode, squarez::project::ProjectMode::SpriteStack);
    assert_eq!(p.sprite_stack_max_layers, Some(32), "stack limit must survive");
}

// ── v2 payload: current Project but Mesh without manual_layout ───────────────

use squarez::three_d::mesh::{AtlasCursor as MeshCursor, Face as MeshFace, Mesh as Mesh3};

#[derive(Serialize)]
struct V2Mesh {
    vertices: Vec<[f32; 3]>,
    faces: Vec<MeshFace>,
    atlas_cursor: MeshCursor,
}

#[derive(Serialize)]
struct V2Project {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    palette: Vec<[u8; 4]>,
    animations: Vec<Animation>,
    active_animation: usize,
    active_frame: usize,
    active_layer: usize,
    layer_id_counter: u64,
    tiles_w: u32,
    tiles_h: u32,
    tile_w: u32,
    tile_h: u32,
    mode: squarez::project::ProjectMode,
    sprite_stack_max_layers: Option<u32>,
    mesh3d: Option<V2Mesh>,
}

#[test]
fn v2_files_load_with_manual_layout_defaulting_off() {
    let mut mesh = Mesh3::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    let base = Project::new_with_mode(
        64,
        64,
        "v2file".to_string(),
        squarez::project::ProjectMode::ThreeD,
    );
    let payload = bincode::serialize(&V2Project {
        name: base.name.clone(),
        canvas_width: 64,
        canvas_height: 64,
        palette: base.palette.clone(),
        animations: base.animations.clone(),
        active_animation: 0,
        active_frame: 0,
        active_layer: 0,
        layer_id_counter: 1,
        tiles_w: 1,
        tiles_h: 1,
        tile_w: 0,
        tile_h: 0,
        mode: squarez::project::ProjectMode::ThreeD,
        sprite_stack_max_layers: None,
        mesh3d: Some(V2Mesh {
            vertices: mesh.vertices.clone(),
            faces: mesh.faces.clone(),
            atlas_cursor: mesh.atlas_cursor,
        }),
    })
    .unwrap();
    let compressed = lz4_flex::compress_prepend_size(&payload);
    let path = std::env::temp_dir().join("squarez_v2_mesh.sqr");
    let mut bytes = b"SQR\0\x02".to_vec();
    bytes.extend(compressed);
    std::fs::write(&path, bytes).unwrap();

    let p = load_sqr(&path).expect("v2 file must load");
    let m = p.mesh3d.expect("mesh must survive");
    assert_eq!(m.vertices, mesh.vertices);
    assert_eq!(m.faces, mesh.faces);
    assert!(!m.manual_layout, "pre-flag files are automatic layouts");
}

#[test]
fn v3_roundtrip_preserves_manual_layout() {
    let mut p = Project::new_with_mode(
        64,
        64,
        "v3manual".to_string(),
        squarez::project::ProjectMode::ThreeD,
    );
    let mut mesh = Mesh3::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    mesh.manual_layout = true;
    p.mesh3d = Some(mesh);

    let path = std::env::temp_dir().join("squarez_v3_manual.sqr");
    save_sqr(&p, &path).unwrap();
    let back = load_sqr(&path).expect("v3 round-trip");
    assert!(
        back.mesh3d.unwrap().manual_layout,
        "hand-packed flag must survive save/load"
    );
}

#[test]
fn v3_files_load_with_empty_glow_and_v4_roundtrips_glow() {
    use squarez::project::{Project, ProjectMode};
    let dir = std::env::temp_dir().join("squarez_v4_glow_test");
    std::fs::create_dir_all(&dir).unwrap();

    // A v3 payload is today's Project minus glow_colors: serialize a current
    // project, strip nothing (bincode tolerates trailing bytes on the mirror
    // side, but a true v3 file simply lacks the tail) — emulate by writing a
    // v3-tagged file from a serialized prefix-compatible struct. Simplest
    // faithful emulation: write the current project WITH empty glow under the
    // v3 version byte; the v3 mirror decodes the prefix and ignores the tail.
    let mut p = Project::new_with_mode(32, 32, "glow".to_string(), ProjectMode::ThreeD);
    p.glow_colors = vec![[255, 0, 200, 255]];
    p.shadow_color = Some([30, 30, 80, 255]);
    p.ao_color = Some([80, 30, 90, 255]);
    let path = dir.join("g.sqr");
    squarez::io::sqr::save_sqr(&p, &path).unwrap();
    let loaded = squarez::io::sqr::load_sqr(&path).unwrap();
    assert_eq!(loaded.glow_colors, vec![[255, 0, 200, 255]], "v5 roundtrip keeps glow");
    assert_eq!(loaded.shadow_color, Some([30, 30, 80, 255]), "v5 keeps the shadow color");
    assert_eq!(loaded.ao_color, Some([80, 30, 90, 255]), "v5 keeps the AO color");

    // Doctor the version byte: each older reader decodes its prefix and
    // defaults the fields it predates (bincode tolerates trailing bytes).
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes[4], 5, "current files are v5");
    let mut v4 = bytes.clone();
    v4[4] = 4;
    let v4_path = dir.join("g4.sqr");
    std::fs::write(&v4_path, &v4).unwrap();
    let loaded = squarez::io::sqr::load_sqr(&v4_path).unwrap();
    assert_eq!(loaded.glow_colors, vec![[255, 0, 200, 255]], "v4 files keep glow");
    assert_eq!(loaded.shadow_color, None, "v4 files default shadow color to None");

    let mut v3 = bytes.clone();
    v3[4] = 3;
    let v3_path = dir.join("g3.sqr");
    std::fs::write(&v3_path, &v3).unwrap();
    let loaded = squarez::io::sqr::load_sqr(&v3_path).unwrap();
    assert!(loaded.glow_colors.is_empty(), "v3 files default to no glow");
    assert_eq!(loaded.canvas_width, 32, "the rest of the project decodes intact");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn glow_flag_is_keyed_by_color_value() {
    use squarez::project::{Project, ProjectMode};
    let mut p = Project::new_with_mode(16, 16, "k".to_string(), ProjectMode::Normal);
    let c = p.palette[3];
    p.toggle_glow_color(c);
    assert!(p.is_glow_color(c));
    // Reordering the palette does not move the flag off the color.
    let moved = p.palette.remove(3);
    p.palette.insert(0, moved);
    assert!(p.is_glow_color(c), "glow follows the color value, not the index");
    p.toggle_glow_color(c);
    assert!(!p.is_glow_color(c));
}
