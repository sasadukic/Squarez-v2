// src/io/v2.rs — .sqr v2 format: uncompressed tar with manifest-first + PNG frames
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use image::ImageEncoder;
use crate::project::{Animation, BlendMode, Frame, Layer, Project};

// ---------------------------------------------------------------------------
// Manifest schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestV2 {
    pub format_version: u16,
    pub engine_version: String,
    pub project_name: String,
    pub created_at: String,
    pub files: Vec<ManifestEntry>,
    pub undo: UndoManifestMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoManifestMeta {
    pub slots: u16,
    pub used: u16,
    pub head_index: u16,
    pub strategy: String,
}

// ---------------------------------------------------------------------------
// Metadata schemas (inside tar entries)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub version: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub bg_color: Option<[u8; 4]>,
    pub palette: Vec<[u8; 4]>,
    pub active_animation: usize,
    pub active_frame: usize,
    pub active_layer: usize,
    pub layer_id_counter: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimMeta {
    pub name: String,
    pub fps: u8,
    pub frame_count: usize,
    pub layers_order: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMeta {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: u8,
    pub blend_mode: String,
    pub order: usize,
    pub is_group: bool,
    pub group_id: Option<u64>,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoSnapshotV2 {
    pub rev: u64,
    pub parent: u64,
    pub timestamp: String,
    pub ops: Vec<serde_json::Value>,
    pub desc: String,
}

// ---------------------------------------------------------------------------
// Entry preparation
// ---------------------------------------------------------------------------

pub enum EntrySource {
    InMemory(Vec<u8>),
    TempFile(PathBuf),
}

pub struct PreparedEntry {
    pub archive_path: String,
    pub size: u64,
    pub sha256: String,
    pub source: EntrySource,
}

fn sha256_bytes(buf: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(buf);
    hex::encode(hasher.finalize())
}

fn blend_mode_to_string(m: BlendMode) -> &'static str {
    match m { BlendMode::Normal => "normal", BlendMode::Multiply => "multiply", BlendMode::Screen => "screen" }
}

fn string_to_blend_mode(s: &str) -> BlendMode {
    match s { "multiply" => BlendMode::Multiply, "screen" => BlendMode::Screen, _ => BlendMode::Normal }
}

fn encode_png(pixels: &[u8], w: u32, h: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    encoder.write_image(pixels, w, h, image::ExtendedColorType::Rgba8)?;
    Ok(buf)
}

/// Prepare all entries for the tar archive. Returns entries (excluding manifest.json) + project metadata + undo meta.
pub fn prepare_entries(project: &Project, snapshots: &[UndoSnapshotV2], _work_dir: &Path)
    -> Result<(Vec<PreparedEntry>, ProjectMeta, Vec<AnimMeta>, Vec<Vec<LayerMeta>>, UndoManifestMeta), Box<dyn std::error::Error>>
{
    let mut entries: Vec<PreparedEntry> = Vec::new();

    // project.json
    let proj_meta = ProjectMeta {
        version: "squarez-v2".to_string(),
        canvas_width: project.canvas_width,
        canvas_height: project.canvas_height,
        bg_color: None,
        palette: project.palette.clone(),
        active_animation: project.active_animation,
        active_frame: project.active_frame,
        active_layer: project.active_layer,
        layer_id_counter: project.layer_id_counter,
    };
    let proj_bytes = serde_json::to_vec(&proj_meta)?;
    entries.push(PreparedEntry {
        archive_path: "project.json".into(),
        size: proj_bytes.len() as u64,
        sha256: sha256_bytes(&proj_bytes),
        source: EntrySource::InMemory(proj_bytes),
    });

    let mut all_anim_meta = Vec::new();
    let mut all_layer_meta = Vec::new();

    for (ai, anim) in project.animations.iter().enumerate() {
        // Anim dir
        let layers_order: Vec<u64> = anim.frames.first()
            .map(|f| f.layers.iter().map(|l| l.id).collect())
            .unwrap_or_default();

        let anim_meta = AnimMeta {
            name: anim.name.clone(),
            fps: anim.fps,
            frame_count: anim.frames.len(),
            layers_order: layers_order.clone(),
        };
        let anim_bytes = serde_json::to_vec(&anim_meta)?;
        entries.push(PreparedEntry {
            archive_path: format!("animations/{}/meta.json", ai),
            size: anim_bytes.len() as u64,
            sha256: sha256_bytes(&anim_bytes),
            source: EntrySource::InMemory(anim_bytes),
        });
        all_anim_meta.push(anim_meta);

        // Layer meta (from first frame — all frames share same layer structure)
        let mut layer_metas = Vec::new();
        if let Some(first_frame) = anim.frames.first() {
            for (li, layer) in first_frame.layers.iter().enumerate() {
                let lm = LayerMeta {
                    id: layer.id,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    opacity: layer.opacity,
                    blend_mode: blend_mode_to_string(layer.blend_mode.clone()).to_string(),
                    order: li,
                    is_group: layer.is_group,
                    group_id: layer.group_id,
                    collapsed: layer.collapsed,
                };
                let lb = serde_json::to_vec(&lm)?;
                entries.push(PreparedEntry {
                    archive_path: format!("animations/{}/layers/{}/layer_meta.json", ai, li),
                    size: lb.len() as u64,
                    sha256: sha256_bytes(&lb),
                    source: EntrySource::InMemory(lb),
                });
                layer_metas.push(lm);
            }
        }
        all_layer_meta.push(layer_metas);

        // Frame PNGs per layer
        for (fi, frame) in anim.frames.iter().enumerate() {
            for (li, layer) in frame.layers.iter().enumerate() {
                if layer.is_group {
                    continue; // groups have no pixel data
                }
                let png = encode_png(&layer.pixels, layer.width, layer.height)?;
                let archive_path = format!("animations/{}/layers/{}/frames/{:03}.png", ai, li, fi);
                entries.push(PreparedEntry {
                    size: png.len() as u64,
                    sha256: sha256_bytes(&png),
                    source: EntrySource::InMemory(png),
                    archive_path,
                });
            }
        }
    }

    // Undo snapshots
    let undo_used = snapshots.len().min(99) as u16;
    let undo_head = if undo_used > 0 { (undo_used - 1) as u16 } else { 0 };
    for (i, snap) in snapshots.iter().enumerate().take(99) {
        let snap_bytes = serde_json::to_vec(snap)?;
        entries.push(PreparedEntry {
            archive_path: format!("undo/snap_{:03}.json", i),
            size: snap_bytes.len() as u64,
            sha256: sha256_bytes(&snap_bytes),
            source: EntrySource::InMemory(snap_bytes),
        });
    }
    let undo_meta = UndoManifestMeta {
        slots: 99,
        used: undo_used,
        head_index: undo_head,
        strategy: "delta-json".to_string(),
    };

    Ok((entries, proj_meta, all_anim_meta, all_layer_meta, undo_meta))
}

// ---------------------------------------------------------------------------
// Write tar (two-phase: prepare -> write)
// ---------------------------------------------------------------------------

pub fn save_v2(project: &Project, path: &Path, snapshots: &[UndoSnapshotV2])
    -> Result<(), Box<dyn std::error::Error>>
{
    let work_dir = path.parent().unwrap_or(Path::new("."));
    let (entries, _proj_meta, _anim_metas, _layer_metas, undo_meta) = prepare_entries(project, snapshots, work_dir)?;

    // Build manifest
    let manifest_entries: Vec<ManifestEntry> = entries.iter().map(|e| ManifestEntry {
        path: e.archive_path.clone(),
        size: e.size,
        sha256: e.sha256.clone(),
    }).collect();

    let manifest = ManifestV2 {
        format_version: 1,
        engine_version: "squarez-v2".to_string(),
        project_name: project.name.clone(),
        created_at: chrono_now(),
        files: manifest_entries,
        undo: undo_meta,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;

    // Write tar atomically
    write_tar_atomic(path, &entries, manifest_bytes)
}

fn write_tar_atomic(dest: &Path, entries: &[PreparedEntry], manifest: Vec<u8>)
    -> Result<(), Box<dyn std::error::Error>>
{
    let parent = dest.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let file = tmp.as_file_mut();
        let mut tar = tar::Builder::new(file);

        // Append manifest as first entry
        let mut header = tar::Header::new_gnu();
        header.set_size(manifest.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "manifest.json", &mut Cursor::new(&manifest))?;

        // Append each entry
        for entry in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(entry.size);
            header.set_mode(0o644);
            header.set_cksum();
            match &entry.source {
                EntrySource::InMemory(buf) => {
                    tar.append_data(&mut header, &entry.archive_path, &mut Cursor::new(buf))?;
                }
                EntrySource::TempFile(p) => {
                    let mut f = std::fs::File::open(p)?;
                    tar.append_file(&entry.archive_path, &mut f)?;
                }
            }
        }
        tar.finish()?;
    }

    // fsync file
    tmp.as_file().sync_all()?;

    // fsync parent directory (best-effort, Unix)
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }

    // Atomic rename
    match std::fs::rename(tmp.path(), dest) {
        Ok(()) => {}
        Err(_) => {
            // On Windows, remove destination first
            let _ = std::fs::remove_file(dest);
            std::fs::rename(tmp.path(), dest)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Load (read manifest first, extract to Project)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Format(String),
    Corrupt(String),
    InvalidChecksum(String),
    UnsupportedVersion(u16),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "IO error: {}", e),
            LoadError::Format(msg) => write!(f, "Format error: {}", msg),
            LoadError::Corrupt(msg) => write!(f, "Corrupted file: {}", msg),
            LoadError::InvalidChecksum(p) => write!(f, "Checksum mismatch: {}", p),
            LoadError::UnsupportedVersion(v) => write!(f, "Unsupported format version: {}", v),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self { LoadError::Io(e) }
}

impl From<serde_json::Error> for LoadError {
    fn from(e: serde_json::Error) -> Self { LoadError::Format(e.to_string()) }
}

/// Load a v2 .sqr file. Returns true if loaded from v2 format, false if old format.
pub fn load_v2(path: &Path) -> Result<Project, LoadError> {
    let mut file = std::fs::File::open(path)?;
    let mut all_data = Vec::new();
    file.read_to_end(&mut all_data)?;

    // Try to parse as tar and read manifest
    let mut archive = tar::Archive::new(Cursor::new(&all_data));
    let mut entries_map: HashMap<String, Vec<u8>> = HashMap::new();
    let mut manifest: Option<ManifestV2> = None;

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let entry_path = entry.path()?.to_string_lossy().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;

        if entry_path == "manifest.json" {
            manifest = Some(serde_json::from_slice(&data)?);
        }
        entries_map.insert(entry_path, data);
    }

    let manifest = manifest.ok_or_else(|| LoadError::Format("manifest.json not found".into()))?;

    if manifest.format_version > 1 {
        return Err(LoadError::UnsupportedVersion(manifest.format_version));
    }

    // Verify checksums for all listed files
    for mf in &manifest.files {
        if let Some(data) = entries_map.get(&mf.path) {
            let actual = sha256_bytes(data);
            if actual != mf.sha256 {
                return Err(LoadError::InvalidChecksum(mf.path.clone()));
            }
        }
    }

    // Parse project.json
    let proj_data = entries_map.get("project.json")
        .ok_or_else(|| LoadError::Format("project.json not found".into()))?;
    let proj_meta: ProjectMeta = serde_json::from_slice(proj_data)?;

    // Parse per-animation metadata and layer metadata, rebuild project
    let mut animations: Vec<Animation> = Vec::new();

    // Count animations from entries
    let mut anim_ids: Vec<usize> = Vec::new();
    for path_str in entries_map.keys() {
        if let Some(rest) = path_str.strip_prefix("animations/") {
            if let Some(ai_str) = rest.split('/').next() {
                if let Ok(ai) = ai_str.parse::<usize>() {
                    if !anim_ids.contains(&ai) {
                        anim_ids.push(ai);
                    }
                }
            }
        }
    }
    anim_ids.sort();

    for ai in anim_ids {
        // Animation meta
        let anim_key = format!("animations/{}/meta.json", ai);
        let anim_data = entries_map.get(&anim_key)
            .ok_or_else(|| LoadError::Format(format!("{} not found", anim_key)))?;
        let anim_meta: AnimMeta = serde_json::from_slice(anim_data)?;

        // Layer metas
        let mut layer_metas: Vec<(usize, LayerMeta)> = Vec::new();
        for path_str in entries_map.keys() {
            let prefix = format!("animations/{}/layers/", ai);
            if path_str.starts_with(&prefix) && path_str.ends_with("/layer_meta.json") {
                let li_str = path_str.trim_start_matches(&prefix)
                    .trim_end_matches("/layer_meta.json");
                if let Ok(li) = li_str.parse::<usize>() {
                    let lm: LayerMeta = serde_json::from_slice(entries_map.get(path_str).unwrap())?;
                    layer_metas.push((li, lm));
                }
            }
        }
        layer_metas.sort_by_key(|(i, _)| *i);

        // Build frames
        let mut frames: Vec<Frame> = Vec::new();
        for fi in 0..anim_meta.frame_count {
            let mut layers: Vec<Layer> = Vec::new();
            for (_, lm) in &layer_metas {
                let (w, h) = (proj_meta.canvas_width, proj_meta.canvas_height);
                let png_path = format!("animations/{}/layers/{}/frames/{:03}.png", ai, lm.order, fi);
                let pixels = if lm.is_group {
                    Vec::new()
                } else if let Some(png_data) = entries_map.get(&png_path) {
                    decode_png_to_rgba(png_data)?
                } else {
                    vec![0u8; (w * h * 4) as usize]
                };

                layers.push(Layer {
                    id: lm.id,
                    name: lm.name.clone(),
                    visible: lm.visible,
                    locked: lm.locked,
                    opacity: lm.opacity,
                    blend_mode: string_to_blend_mode(&lm.blend_mode),
                    pixels,
                    width: w,
                    height: h,
                    is_group: lm.is_group,
                    group_id: lm.group_id,
                    collapsed: lm.collapsed,
                });
            }
            frames.push(Frame { duration_ms: 0, layers, dirty: false });
        }

        animations.push(Animation {
            name: anim_meta.name,
            fps: anim_meta.fps,
            frames,
        });
    }

    let anim_count = animations.len();
    Ok(Project {
        name: manifest.project_name,
        canvas_width: proj_meta.canvas_width,
        canvas_height: proj_meta.canvas_height,
        palette: proj_meta.palette,
        animations,
        active_animation: proj_meta.active_animation.min(anim_count.saturating_sub(1)),
        active_frame: proj_meta.active_frame,
        active_layer: proj_meta.active_layer,
        layer_id_counter: proj_meta.layer_id_counter.max(1),
    })
}

fn decode_png_to_rgba(data: &[u8]) -> Result<Vec<u8>, LoadError> {
    let img = image::load_from_memory(data)
        .map_err(|e| LoadError::Format(format!("PNG decode: {}", e)))?;
    Ok(img.to_rgba8().to_vec())
}

// ---------------------------------------------------------------------------
// Recovery: extract files from corrupted tar
// ---------------------------------------------------------------------------

/// Recover what we can from a corrupted .sqr file. Writes extracted files to out_dir.
pub fn recover_v2(path: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(out_dir)?;
    let mut file = std::fs::File::open(path)?;
    let mut all_data = Vec::new();
    file.read_to_end(&mut all_data)?;

    // Try tar extraction
    let mut archive = tar::Archive::new(Cursor::new(&all_data));
    let mut extracted_any = false;
    for entry_result in archive.entries()? {
        if let Ok(mut entry) = entry_result {
            let entry_path = entry.path()?.to_string_lossy().to_string();
            let mut data = Vec::new();
            if entry.read_to_end(&mut data).is_ok() {
                let out_path = out_dir.join(&entry_path);
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&out_path, &data);
                extracted_any = true;
            }
        }
    }

    if extracted_any {
        eprintln!("Recovery: extracted {} entries from archive", count_extracted(out_dir));
        return Ok(());
    }

    // Fallback: scan for PNG signatures
    eprintln!("Recovery: tar extraction failed, scanning for PNG signatures...");
    let sig = b"\x89PNG\r\n\x1a\n";
    let mut pos = 0;
    let mut png_idx = 0;
    while pos < all_data.len() {
        if let Some(offset) = find_bytes(&all_data[pos..], sig) {
            let start = pos + offset;
            if let Some(end) = find_png_end(&all_data[start..]) {
                let png_bytes = &all_data[start..start + end];
                let out_path = out_dir.join(format!("recovered_{:04}.png", png_idx));
                std::fs::write(&out_path, png_bytes)?;
                png_idx += 1;
                pos = start + end;
            } else {
                pos = start + 1;
            }
        } else {
            break;
        }
    }
    eprintln!("Recovery: extracted {} PNG images", png_idx);
    Ok(())
}

fn count_extracted(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            if entry.path().is_file() {
                // recursive
                count_dir(&entry.path(), &mut count);
            }
        }
    }
    count
}

fn count_dir(dir: &Path, count: &mut usize) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            if entry.path().is_dir() {
                count_dir(&entry.path(), count);
            } else {
                *count += 1;
            }
        }
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() { return None; }
    haystack.windows(needle.len())
        .position(|w| w == needle)
}

fn find_png_end(data: &[u8]) -> Option<usize> {
    // PNG IEND chunk marker: 00 00 00 00 49 45 4E 44 AE 42 60 82
    let iend = b"\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    find_bytes(data, iend).map(|pos| pos + iend.len())
}

// ---------------------------------------------------------------------------
// Helper: current timestamp as ISO 8601
// ---------------------------------------------------------------------------

fn chrono_now() -> String {
    // Use localtime format: 2026-05-25T12:34:56Z (approximate with std)
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Simple breakdown — enough for ISO format without chrono dependency
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Rough year/month/day from days since epoch
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days = if leap { &MONTH_DAYS_LEAP[..] } else { &MONTH_DAYS[..] };
    let mut m = 0;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md as i64 { m = i; break; }
        remaining -= md as i64;
    }
    let day = remaining + 1;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m + 1, day, hours, minutes, seconds)
}

const MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const MONTH_DAYS_LEAP: [i64; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Layer;

    fn make_test_project() -> Project {
        let mut project = Project::new(8, 8, "test".to_string());
        // Add a second frame
        project.animations[0].frames.push(
            Frame {
                duration_ms: 0,
                layers: vec![Layer::new("Layer 1".to_string(), 8, 8)],
                dirty: false,
            }
        );
        // Put some non-zero pixels in the first frame's layer
        let l = &mut project.animations[0].frames[0].layers[0];
        l.pixels[0] = 255;
        l.pixels[1] = 128;
        l.pixels[2] = 64;
        l.pixels[3] = 255;
        project
    }

    #[test]
    fn test_save_load_roundtrip() {
        let project = make_test_project();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("sqr");

        let result = save_v2(&project, &path, &[]);
        assert!(result.is_ok(), "save failed: {:?}", result);

        let loaded = load_v2(&path);
        assert!(loaded.is_ok(), "load failed: {:?}", loaded);
        let loaded = loaded.unwrap();

        assert_eq!(loaded.name, project.name);
        assert_eq!(loaded.canvas_width, project.canvas_width);
        assert_eq!(loaded.canvas_height, project.canvas_height);
        assert_eq!(loaded.animations.len(), project.animations.len());
        assert_eq!(loaded.animations[0].frames.len(), project.animations[0].frames.len());
        assert_eq!(loaded.animations[0].frames[0].layers.len(), project.animations[0].frames[0].layers.len());
        assert_eq!(loaded.animations[0].frames[0].layers[0].pixels[0], 255);
        assert_eq!(loaded.animations[0].frames[0].layers[0].pixels[1], 128);
        assert_eq!(loaded.animations[0].frames[0].layers[0].pixels[2], 64);
        assert_eq!(loaded.animations[0].frames[0].layers[0].pixels[3], 255);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_manifest_is_first_entry() {
        let project = make_test_project();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("sqr");

        save_v2(&project, &path, &[]).unwrap();

        let data = std::fs::read(&path).unwrap();
        // First few bytes should be tar header for manifest.json
        // We can check that "manifest.json" appears early
        let header = String::from_utf8_lossy(&data[..200]);
        assert!(header.contains("manifest.json"), "manifest not first entry: {:?}", header);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_checksum_validation() {
        let project = make_test_project();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("sqr");

        save_v2(&project, &path, &[]).unwrap();

        // Corrupt the file
        let mut data = std::fs::read(&path).unwrap();
        if data.len() > 500 {
            data[500] ^= 0xFF; // flip some bits in the middle
            std::fs::write(&path, &data).unwrap();

            let result = load_v2(&path);
            assert!(result.is_err(), "should fail on corrupt checksum");
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_recover_png_from_corrupt() {
        let project = make_test_project();
        let tmpdir = tempfile::tempdir().unwrap();
        let sqr_path = tmpdir.path().join("test.sqr");
        save_v2(&project, &sqr_path, &[]).unwrap();

        // Truncate the file to simulate corruption
        let data = std::fs::read(&sqr_path).unwrap();
        let truncated = &data[..data.len() / 2];
        let corrupt_path = tmpdir.path().join("corrupt.sqr");
        std::fs::write(&corrupt_path, truncated).unwrap();

        let out_dir = tmpdir.path().join("recovered");
        let result = recover_v2(&corrupt_path, &out_dir);
        assert!(result.is_ok(), "recovery failed: {:?}", result);

        // Should have recovered at least the manifest
        assert!(out_dir.join("manifest.json").exists() || {
            // or at least some PNGs
            let count = std::fs::read_dir(&out_dir).map(|d| d.count()).unwrap_or(0);
            count > 0
        });
    }
}
