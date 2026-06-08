// src/layers.rs
use crate::project::{Frame, Rgba};

/// Composites all visible layers in a frame into a single RGBA buffer (width*height*4 bytes)
pub fn composite_frame(frame: &Frame, width: u32, height: u32) -> Vec<u8> {
    let size = (width * height * 4) as usize;
    let mut out = vec![0u8; size];
    for layer in &frame.layers {
        if !layer.visible { continue; }
        if layer.is_group { continue; } // group layers have no pixel data
        if layer.pixels.is_empty() { continue; }
        let pixel_count = (width * height) as usize;
        let needed = pixel_count * 4;
        if layer.pixels.len() < needed {
            continue;
        }
        let alpha_factor = layer.opacity as f32 / 255.0;
        for i in 0..pixel_count {
            let idx = i * 4;
            let src = [
                layer.pixels[idx],
                layer.pixels[idx + 1],
                layer.pixels[idx + 2],
                layer.pixels[idx + 3],
            ];
            let dst = [out[idx], out[idx+1], out[idx+2], out[idx+3]];
            let blended = blend_normal(src, dst, alpha_factor);
            out[idx]   = blended[0];
            out[idx+1] = blended[1];
            out[idx+2] = blended[2];
            out[idx+3] = blended[3];
        }
    }
    out
}

/// Composites only the top-left (tile_w * tile_h) region of a frame's layers,
/// knowing that the layer pixels are stored in row-major order with width `cw`.
pub fn composite_frame_tile(frame: &Frame, cw: u32, tile_w: u32, tile_h: u32) -> Vec<u8> {
    let size = (tile_w * tile_h * 4) as usize;
    let mut out = vec![0u8; size];
    for layer in &frame.layers {
        if !layer.visible { continue; }
        if layer.is_group { continue; }
        if layer.pixels.is_empty() { continue; }
        let alpha_factor = layer.opacity as f32 / 255.0;
        for py in 0..tile_h {
            let src_row_start = (py * cw * 4) as usize;
            let dst_row_start = (py * tile_w * 4) as usize;
            for px in 0..tile_w {
                let s_idx = src_row_start + (px * 4) as usize;
                let d_idx = dst_row_start + (px * 4) as usize;
                if s_idx + 4 > layer.pixels.len() { continue; }
                let src = [
                    layer.pixels[s_idx],
                    layer.pixels[s_idx + 1],
                    layer.pixels[s_idx + 2],
                    layer.pixels[s_idx + 3],
                ];
                let dst = [out[d_idx], out[d_idx + 1], out[d_idx + 2], out[d_idx + 3]];
                let blended = blend_normal(src, dst, alpha_factor);
                out[d_idx]     = blended[0];
                out[d_idx + 1] = blended[1];
                out[d_idx + 2] = blended[2];
                out[d_idx + 3] = blended[3];
            }
        }
    }
    out
}

/// Helper function to crop a specific tile out of parent_pixels (size cw * ch)
pub fn crop_tile(parent_pixels: &[u8], cw: u32, _ch: u32, tile_w: u32, tile_h: u32, tiles_w: u32, idx: usize) -> Vec<u8> {
    let tx = idx as u32 % tiles_w;
    let ty = idx as u32 / tiles_w;
    let mut tile_pixels = vec![0u8; (tile_w * tile_h * 4) as usize];
    for py in 0..tile_h {
        let src_row_start = ((ty * tile_h + py) * cw * 4) as usize;
        let dst_row_start = (py * tile_w * 4) as usize;
        let src_x_start = (tx * tile_w * 4) as usize;
        for px in 0..tile_w {
            let s_idx = src_row_start + src_x_start + (px * 4) as usize;
            let d_idx = dst_row_start + (px * 4) as usize;
            if s_idx + 4 <= parent_pixels.len() && d_idx + 4 <= tile_pixels.len() {
                tile_pixels[d_idx..d_idx+4].copy_from_slice(&parent_pixels[s_idx..s_idx+4]);
            }
        }
    }
    tile_pixels
}

fn blend_normal(src: Rgba, dst: Rgba, layer_alpha: f32) -> Rgba {
    let sa = (src[3] as f32 / 255.0) * layer_alpha;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a == 0.0 { return [0, 0, 0, 0]; }
    let blend_channel = |sc: u8, dc: u8| -> u8 {
        let s = sc as f32 / 255.0;
        let d = dc as f32 / 255.0;
        ((s * sa + d * da * (1.0 - sa)) / out_a * 255.0).round() as u8
    };
    [
        blend_channel(src[0], dst[0]),
        blend_channel(src[1], dst[1]),
        blend_channel(src[2], dst[2]),
        (out_a * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Frame;

    #[test]
    fn opaque_layer_covers_transparent_background() {
        let mut frame = Frame::new(2, 2, 1);
        frame.layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
        let result = composite_frame(&frame, 2, 2);
        assert_eq!(&result[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn invisible_layer_is_skipped() {
        let mut frame = Frame::new(2, 2, 1);
        frame.layers[0].visible = false;
        frame.layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
        let result = composite_frame(&frame, 2, 2);
        assert_eq!(&result[0..4], &[0, 0, 0, 0]);
    }
}
