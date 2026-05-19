// src/io/export.rs
use std::path::Path;
use crate::project::Project;
use crate::layers::composite_frame;

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat { Png, Gif, Spritesheet }

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub scale: u32,         // 1, 2, 4, or 8
    pub animation_index: usize,
}

pub fn export(project: &Project, path: &Path, opts: ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    match opts.format {
        ExportFormat::Png         => export_png(project, path, &opts),
        ExportFormat::Gif         => export_gif(project, path, &opts),
        ExportFormat::Spritesheet => export_spritesheet(project, path, &opts),
    }
}

fn export_png(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let frame = &anim.frames[0];
    let w = project.canvas_width;
    let h = project.canvas_height;
    let pixels = composite_frame(frame, w, h);
    let scaled = scale_pixels(&pixels, w, h, opts.scale);
    image::save_buffer(path, &scaled, w * opts.scale, h * opts.scale, image::ColorType::Rgba8)?;
    Ok(())
}

fn export_gif(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let w = project.canvas_width * opts.scale;
    let h = project.canvas_height * opts.scale;
    let delay = (100.0 / anim.fps.max(1) as f32) as u16; // centiseconds
    let file = std::fs::File::create(path)?;
    let mut encoder = gif::Encoder::new(file, w as u16, h as u16, &[])?;
    encoder.set_repeat(gif::Repeat::Infinite)?;
    for frame in &anim.frames {
        let pixels = composite_frame(frame, project.canvas_width, project.canvas_height);
        let scaled = scale_pixels(&pixels, project.canvas_width, project.canvas_height, opts.scale);
        let mut gif_frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut scaled.clone(), 10);
        gif_frame.delay = delay;
        encoder.write_frame(&gif_frame)?;
    }
    Ok(())
}

fn export_spritesheet(project: &Project, path: &Path, opts: &ExportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let anim = &project.animations[opts.animation_index];
    let fw = project.canvas_width * opts.scale;
    let fh = project.canvas_height * opts.scale;
    let total_w = fw * anim.frames.len() as u32;
    let mut sheet = vec![0u8; (total_w * fh * 4) as usize];
    for (i, frame) in anim.frames.iter().enumerate() {
        let pixels = composite_frame(frame, project.canvas_width, project.canvas_height);
        let scaled = scale_pixels(&pixels, project.canvas_width, project.canvas_height, opts.scale);
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
