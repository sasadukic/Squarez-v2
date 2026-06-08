// src/io/export.rs
use std::path::Path;
use crate::project::Project;
use crate::layers::composite_frame;
use crate::wang_blob::{WangBlobMode, WANG_COLS, WANG_ROWS, BLOB_COLS, BLOB_ROWS};

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat { Png, Gif, Spritesheet }

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub scale: u32,         // 1, 2, 4, or 8
    pub animation_index: usize,
    pub frame_index: usize,
}

pub fn export(project: &Project, path: &Path, opts: ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    match opts.format {
        ExportFormat::Png         => export_png(project, path, &opts),
        ExportFormat::Gif         => export_gif(project, path, &opts),
        ExportFormat::Spritesheet => export_spritesheet(project, path, &opts),
    }
}

pub fn export_png(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let frame = &anim.frames[opts.frame_index];
    let (pixels, w, h) = if project.is_tiled() {
        (crate::layers::composite_frame_tile(frame, project.canvas_width, project.tile_w, project.tile_h), project.tile_w, project.tile_h)
    } else {
        (composite_frame(frame, project.canvas_width, project.canvas_height), project.canvas_width, project.canvas_height)
    };
    let scaled = scale_pixels(&pixels, w, h, opts.scale);
    image::save_buffer(path, &scaled, w * opts.scale, h * opts.scale, image::ColorType::Rgba8)?;
    Ok(())
}

fn export_gif(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let (tile_w, tile_h, clip_start, clip_end) = if project.is_tiled() {
        let frame_count = anim.frames.len();
        let s = anim.tile_start.min(frame_count.saturating_sub(1));
        let e = anim.tile_end.min(frame_count.saturating_sub(1)).max(s);
        (project.tile_w, project.tile_h, s, e)
    } else {
        (project.canvas_width, project.canvas_height, 0, anim.frames.len().saturating_sub(1))
    };
    let w = tile_w * opts.scale;
    let h = tile_h * opts.scale;
    let delay = (100.0 / anim.fps.max(1) as f32) as u16; // centiseconds
    let file = std::fs::File::create(path)?;
    let mut encoder = gif::Encoder::new(file, w as u16, h as u16, &[])?;
    encoder.set_repeat(gif::Repeat::Infinite)?;
    for idx in clip_start..=clip_end {
        let frame = &anim.frames[idx];
        let pixels = if project.is_tiled() {
            crate::layers::composite_frame_tile(frame, project.canvas_width, tile_w, tile_h)
        } else {
            composite_frame(frame, project.canvas_width, project.canvas_height)
        };
        let scaled = scale_pixels(&pixels, tile_w, tile_h, opts.scale);
        let mut gif_frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut scaled.clone(), 10);
        gif_frame.delay = delay;
        encoder.write_frame(&gif_frame)?;
    }
    Ok(())
}

fn export_spritesheet(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let (tile_w, tile_h, clip_start, clip_end) = if project.is_tiled() {
        let frame_count = anim.frames.len();
        let s = anim.tile_start.min(frame_count.saturating_sub(1));
        let e = anim.tile_end.min(frame_count.saturating_sub(1)).max(s);
        (project.tile_w, project.tile_h, s, e)
    } else {
        (project.canvas_width, project.canvas_height, 0, anim.frames.len().saturating_sub(1))
    };
    let fw = tile_w * opts.scale;
    let fh = tile_h * opts.scale;
    let num_frames = clip_end - clip_start + 1;
    let total_w = fw * num_frames as u32;
    let mut sheet = vec![0u8; (total_w * fh * 4) as usize];
    for (i, idx) in (clip_start..=clip_end).enumerate() {
        let frame = &anim.frames[idx];
        let pixels = if project.is_tiled() {
            crate::layers::composite_frame_tile(frame, project.canvas_width, tile_w, tile_h)
        } else {
            composite_frame(frame, project.canvas_width, project.canvas_height)
        };
        let scaled = scale_pixels(&pixels, tile_w, tile_h, opts.scale);
        let x_offset = i as u32 * fw;
        for y in 0..fh {
            for x in 0..fw {
                let src_idx = ((y * fw + x) * 4) as usize;
                let dst_idx = ((y * total_w + x_offset + x) * 4) as usize;
                sheet[dst_idx..dst_idx+4].copy_from_slice(&scaled[src_idx..src_idx+4]);
            }
        }
    }
    image::save_buffer(path, &sheet, total_w, fh, image::ColorType::Rgba8)?;
    Ok(())
}

pub fn export_palette_png(path: &Path, colors: &[[u8; 4]]) -> Result<(), Box<dyn std::error::Error>> {
    if colors.is_empty() { return Ok(()); }
    let w = colors.len() as u32;
    let h = 1u32;
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for c in colors {
        buf.extend_from_slice(c);
    }
    image::save_buffer(path, &buf, w, h, image::ColorType::Rgba8)?;
    Ok(())
}

fn scale_pixels(pixels: &[u8], w: u32, h: u32, scale: u32) -> Vec<u8> {
    if scale == 1 { return pixels.to_vec(); }
    let sw = w * scale;
    let sh = h * scale;
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let src = ((y * w + x) * 4) as usize;
            let c = &pixels[src..src+4];
            for dy in 0..scale {
                for dx in 0..scale {
                    let dst = (((y * scale + dy) * sw + (x * scale + dx)) * 4) as usize;
                    out[dst..dst+4].copy_from_slice(c);
                }
            }
        }
    }
    out
}

// ─── Wang/Blob tile export functions ──────────────────────────────────────────

/// Export a single tile as PNG (single tile at its original size).
pub fn export_tile_png(
    path: &Path,
    pixels: &[u8],
    tile_size: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    if pixels.is_empty() || tile_size == 0 { return Ok(()); }
    image::save_buffer(
        path,
        pixels,
        tile_size as u32,
        tile_size as u32,
        image::ColorType::Rgba8,
    )?;
    Ok(())
}

/// Export full tileset as a spritesheet PNG (all tiles laid out in grid order).
pub fn export_tileset_spritesheet(
    path: &Path,
    mode: WangBlobMode,
    tile_size: u32,
    tiles: &Vec<Vec<[u8; 4]>>, // flattened grid of tile pixels
) -> Result<(), Box<dyn std::error::Error>> {
    let (cols, rows) = match mode {
        WangBlobMode::Wang => (WANG_COLS as usize, WANG_ROWS as usize),
        WangBlobMode::Blob => (BLOB_COLS as usize, BLOB_ROWS as usize),
        _ => return Ok(()),
    };

    // Count non-gap tiles.
    let mut count = 0usize;
    for row in 0..rows {
        for col in 0..cols {
            let tidx = row * cols + col;
            // Skip gap (Blob mode: row=1, col=10)
            if mode == WangBlobMode::Blob && row == 1 && col == 10 { continue; }
            count += 1;
        }
    }

    let ts = tile_size as usize;
    let mut sheet = Vec::with_capacity(count * ts * ts * 4);

    for row in 0..rows {
        for col in 0..cols {
            let tidx = row * cols + col;
            // Skip gap.
            if mode == WangBlobMode::Blob && row == 1 && col == 10 { continue; }
            if tidx < tiles.len() {
                sheet.extend_from_slice(&tiles[tidx][..ts * ts * 4]);
            } else {
                sheet.extend_from_slice(&vec![[0u8; 4]; ts * ts]);
            }
        }
    }

    if !sheet.is_empty() {
        let total_tiles = count;
        // Layout: single row of tiles (horizontal layout like spritesheet).
        let sheet_w = (total_tiles as u32) * tile_size;
        
        // Flatten tiles to a Vec<u8> buffer
        let mut flat_sheet = Vec::with_capacity(count * ts * ts * 4);
        
        for r in 0..rows {
            for c in 0..cols {
                let tidx = r * cols + c;
                // Skip gap (Blob mode: row=1, col=10)
                if mode == WangBlobMode::Blob && r == 1 && c == 10 { continue; }
                
                if tidx < tiles.len() {
                    let tile = &tiles[tidx];
                    for pixel in tile {
                        flat_sheet.extend_from_slice(&[pixel[0], pixel[1], pixel[2], pixel[3]]);
                    }
                } else {
                    flat_sheet.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
        
        image::save_buffer(
            path,
            &flat_sheet,
            sheet_w as u32,
            tile_size as u32,
            image::ColorType::Rgba8,
        )?;
    }

    Ok(())
}

/// Export a single tile by index as PNG.
pub fn export_single_tile(
    path: &Path,
    mode: WangBlobMode,
    tile_size: u32,
    tiles: &Vec<Vec<[u8; 4]>>,
    tile_index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let (cols, _rows) = match mode {
        WangBlobMode::Wang => (WANG_COLS as usize, WANG_ROWS as usize),
        _ => (BLOB_COLS as usize, BLOB_ROWS as usize),
    };

    let tidx = tile_index as usize;
    if tidx >= tiles.len() { return Ok(()); }

    let ts = tile_size as usize;
    
    // Flatten the single tile's pixel data to Vec<u8>
    let mut flat_pixels = Vec::with_capacity(ts * ts * 4);
    for pixel in &tiles[tidx] {
        flat_pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], pixel[3]]);
    }
    
    export_tile_png(path, &flat_pixels, tile_size)
}
