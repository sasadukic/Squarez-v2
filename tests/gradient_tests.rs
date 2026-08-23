// tests/gradient_tests.rs
//
// The gradient core: axis math, the three blend styles, masking, and the
// degenerate cases.

use squarez::project::Layer;
use squarez::tools::{apply_gradient, GradientStyle};

const FG: [u8; 4] = [200, 30, 30, 255];
const BG: [u8; 4] = [30, 30, 200, 255];

fn layer(w: u32, h: u32) -> Layer {
    Layer::new("t".to_string(), w, h)
}

#[test]
fn mask_limits_the_edits() {
    let l = layer(8, 8);
    let edits = apply_gradient(
        &l,
        (0, 0, 8, 8),
        |x, _| x < 4,
        (0.5, 0.5),
        (7.5, 0.5),
        GradientStyle::Smooth,
        FG,
        BG,
        None,
    );
    assert!(!edits.is_empty());
    assert!(edits.iter().all(|&(x, _, _, _)| x < 4), "masked-out texels must not be touched");
}

#[test]
fn smooth_hits_endpoints_and_is_monotone() {
    let l = layer(8, 1);
    let edits = apply_gradient(
        &l,
        (0, 0, 8, 1),
        |_, _| true,
        (0.5, 0.5),
        (7.5, 0.5),
        GradientStyle::Smooth,
        FG,
        BG,
        None,
    );
    assert_eq!(edits.len(), 8);
    assert_eq!(edits[0].3, FG, "t=0 texel is exactly the foreground");
    assert_eq!(edits[7].3, BG, "t=1 texel is exactly the background");
    // Red falls, blue rises along the axis.
    for w in edits.windows(2) {
        assert!(w[1].3[0] <= w[0].3[0] && w[1].3[2] >= w[0].3[2], "monotone blend");
    }
}

#[test]
fn dithered_uses_only_the_two_colors_with_a_checker_band() {
    let l = layer(16, 4);
    let edits = apply_gradient(
        &l,
        (0, 0, 16, 4),
        |_, _| true,
        (0.5, 0.5),
        (15.5, 0.5),
        GradientStyle::Dithered,
        FG,
        BG,
        None,
    );
    assert!(edits.iter().all(|e| e.3 == FG || e.3 == BG), "dither emits only fg/bg");
    // Around the middle of the axis the 2x2 Bayer alternates vertically:
    // t ~ 0.5 sits between the 0.375 and 0.625 thresholds.
    let mid: Vec<_> = edits.iter().filter(|e| e.0 == 8).collect();
    assert_eq!(mid.len(), 4);
    assert_ne!(mid[0].3, mid[1].3, "mid-band must checker");
}

#[test]
fn palette_ramp_bands_and_degenerates() {
    let ramp = [[10, 0, 0, 255], [0, 10, 0, 255], [0, 0, 10, 255]];
    let l = layer(6, 1);
    let edits = apply_gradient(
        &l,
        (0, 0, 6, 1),
        |_, _| true,
        (0.5, 0.5),
        (5.5, 0.5),
        GradientStyle::PaletteRamp,
        FG,
        BG,
        Some(&ramp),
    );
    let colors: Vec<_> = edits.iter().map(|e| e.3).collect();
    assert_eq!(colors[0], ramp[0]);
    assert_eq!(colors[5], ramp[2]);
    let distinct: std::collections::HashSet<_> = colors.iter().collect();
    assert_eq!(distinct.len(), 3, "three bands from a three-color ramp");

    // One-color ramp paints solid.
    let edits = apply_gradient(
        &l,
        (0, 0, 6, 1),
        |_, _| true,
        (0.5, 0.5),
        (5.5, 0.5),
        GradientStyle::PaletteRamp,
        FG,
        BG,
        Some(&ramp[..1]),
    );
    assert!(edits.iter().all(|e| e.3 == ramp[0]));
}

#[test]
fn zero_length_drag_paints_solid_start_color() {
    let l = layer(4, 4);
    let edits = apply_gradient(
        &l,
        (0, 0, 4, 4),
        |_, _| true,
        (2.0, 2.0),
        (2.0, 2.0),
        GradientStyle::Smooth,
        FG,
        BG,
        None,
    );
    assert_eq!(edits.len(), 16);
    assert!(edits.iter().all(|e| e.3 == FG));
}

#[test]
fn unchanged_texels_are_omitted() {
    let mut l = layer(4, 1);
    for x in 0..4 {
        l.set_pixel(x, 0, FG);
    }
    let edits = apply_gradient(
        &l,
        (0, 0, 4, 1),
        |_, _| true,
        (0.5, 0.5),
        (0.5, 0.5),
        GradientStyle::Smooth,
        FG,
        BG,
        None,
    );
    assert!(edits.is_empty(), "already-correct texels record no edits");
}
