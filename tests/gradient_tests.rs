// tests/gradient_tests.rs
//
// The gradient core: axis math + snapping, the three blend styles over a
// color selection, masking, and the degenerate cases.

use squarez::project::Layer;
use squarez::tools::{apply_gradient, snap_axis_8, GradientStyle};

const A: [u8; 4] = [200, 30, 30, 255];
const B: [u8; 4] = [30, 30, 200, 255];

fn layer(w: u32, h: u32) -> Layer {
    Layer::new("t".to_string(), w, h)
}

#[test]
fn fewer_than_two_colors_paints_nothing() {
    let l = layer(8, 8);
    for colors in [&[][..], &[A][..]] {
        let edits = apply_gradient(
            &l,
            (0, 0, 8, 8),
            |_, _| true,
            (0.5, 0.5),
            (7.5, 0.5),
            GradientStyle::Smooth,
            colors,
        );
        assert!(edits.is_empty(), "a gradient needs at least two selected colors");
    }
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
        &[A, B],
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
        &[A, B],
    );
    assert_eq!(edits.len(), 8);
    assert_eq!(edits[0].3, A, "t=0 texel is exactly the first selected color");
    assert_eq!(edits[7].3, B, "t=1 texel is exactly the last selected color");
    for w in edits.windows(2) {
        assert!(w[1].3[0] <= w[0].3[0] && w[1].3[2] >= w[0].3[2], "monotone blend");
    }
}

#[test]
fn dithered_uses_only_selected_colors_with_a_checker_band() {
    let l = layer(16, 4);
    let edits = apply_gradient(
        &l,
        (0, 0, 16, 4),
        |_, _| true,
        (0.5, 0.5),
        (15.5, 0.5),
        GradientStyle::Dithered,
        &[A, B],
    );
    assert!(edits.iter().all(|e| e.3 == A || e.3 == B), "dither emits only selected colors");
    let mid: Vec<_> = edits.iter().filter(|e| e.0 == 8).collect();
    assert_eq!(mid.len(), 4);
    assert_ne!(mid[0].3, mid[1].3, "mid-band must checker");
}

#[test]
fn dithered_three_colors_never_mixes_non_adjacent_pairs() {
    let g = [10, 200, 10, 255];
    let l = layer(24, 2);
    let edits = apply_gradient(
        &l,
        (0, 0, 24, 2),
        |_, _| true,
        (0.5, 0.5),
        (23.5, 0.5),
        GradientStyle::Dithered,
        &[A, g, B],
    );
    assert!(edits.iter().all(|e| e.3 == A || e.3 == g || e.3 == B));
    // First half of the axis only mixes A/g; second half only g/B.
    for e in &edits {
        if e.0 < 10 {
            assert_ne!(e.3, B, "the far color must not appear near the start");
        }
        if e.0 > 13 {
            assert_ne!(e.3, A, "the near color must not appear near the end");
        }
    }
}

#[test]
fn banded_and_degenerates() {
    let ramp = [[10, 0, 0, 255], [0, 10, 0, 255], [0, 0, 10, 255]];
    let l = layer(6, 1);
    let edits = apply_gradient(
        &l,
        (0, 0, 6, 1),
        |_, _| true,
        (0.5, 0.5),
        (5.5, 0.5),
        GradientStyle::PaletteRamp,
        &ramp,
    );
    let colors: Vec<_> = edits.iter().map(|e| e.3).collect();
    assert_eq!(colors[0], ramp[0]);
    assert_eq!(colors[5], ramp[2]);
    let distinct: std::collections::HashSet<_> = colors.iter().collect();
    assert_eq!(distinct.len(), 3, "three bands from a three-color selection");
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
        &[A, B],
    );
    assert_eq!(edits.len(), 16);
    assert!(edits.iter().all(|e| e.3 == A));
}

#[test]
fn unchanged_texels_are_omitted() {
    let mut l = layer(4, 1);
    for x in 0..4 {
        l.set_pixel(x, 0, A);
    }
    let edits = apply_gradient(
        &l,
        (0, 0, 4, 1),
        |_, _| true,
        (0.5, 0.5),
        (0.5, 0.5),
        GradientStyle::Smooth,
        &[A, B],
    );
    assert!(edits.is_empty(), "already-correct texels record no edits");
}

#[test]
fn axis_snaps_to_the_eight_directions() {
    let s = (10.0, 10.0);
    // 30 degrees snaps to 45.
    let e = snap_axis_8(s, (10.0 + 8.66, 10.0 + 5.0));
    let (dx, dy) = (e.0 - s.0, e.1 - s.1);
    assert!((dx - dy).abs() < 1e-4, "30-degree drag snaps to the diagonal: {dx} vs {dy}");
    // 10 degrees snaps to horizontal.
    let e = snap_axis_8(s, (20.0, 11.7));
    assert!((e.1 - s.1).abs() < 1e-4, "shallow drag snaps to horizontal");
    // 80 degrees snaps to vertical.
    let e = snap_axis_8(s, (11.0, 20.0));
    assert!((e.0 - s.0).abs() < 1e-4, "steep drag snaps to vertical");
    // Zero-length stays put.
    assert_eq!(snap_axis_8(s, s), s);
}
