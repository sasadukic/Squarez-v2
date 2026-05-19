// src/layers.rs
use crate::project::{Frame, Rgba};

/// Composites all visible layers in a frame into a single RGBA buffer (width*height*4 bytes)
pub fn composite_frame(frame: &Frame, width: u32, height: u32) -> Vec<u8> {
    let size = (width * height * 4) as usize;
    let mut out = vec![0u8; size];
    for layer in &frame.layers {
        if !layer.visible { continue; }
        let alpha_factor = layer.opacity as f32 / 255.0;
        for i in 0..(width * height) as usize {
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
        let mut frame = Frame::new(2, 2);
        frame.layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
        let result = composite_frame(&frame, 2, 2);
        assert_eq!(&result[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn invisible_layer_is_skipped() {
        let mut frame = Frame::new(2, 2);
        frame.layers[0].visible = false;
        frame.layers[0].set_pixel(0, 0, [255, 0, 0, 255]);
        let result = composite_frame(&frame, 2, 2);
        assert_eq!(&result[0..4], &[0, 0, 0, 0]);
    }
}
