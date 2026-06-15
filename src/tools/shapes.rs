// src/tools/shapes.rs
use crate::project::{Layer, Rgba};
use super::{PixelEdit, apply_pencil, bresenham_positions};

/// Draw a filled or outlined rectangle.  All coordinates are unconstrained i32 —
/// pixels that land outside the canvas are discarded by apply_pencil's bounds check.
pub fn apply_rect(layer: &Layer, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let (lx, rx) = (x0.min(x1), x0.max(x1));
    let (ly, ry) = (y0.min(y1), y0.max(y1));
    for y in ly..=ry {
        for x in lx..=rx {
            let on_border = x == lx || x == rx || y == ly || y == ry;
            if filled || on_border {
                if x >= 0 && y >= 0 {
                    edits.extend(apply_pencil(layer, x as u32, y as u32, color));
                }
            }
        }
    }
    edits
}

/// Draw a filled or outlined ellipse.  All coordinates are unconstrained i32.
pub fn apply_ellipse(layer: &Layer, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba, filled: bool) -> Vec<PixelEdit> {
    let mut edits = Vec::new();
    let mut x0 = x0;
    let mut y0 = y0;
    let mut x1 = x1;
    let mut y1 = y1;
    
    let mut a = (x1 - x0).abs();
    let b = (y1 - y0).abs();
    let mut b1 = b & 1; /* diameter value sign */
    
    let mut dx = 4 * (1 - a) * b * b;
    let mut dy = 4 * (1 + b1) * a * a; /* error increment */
    let mut err = dx + dy + b1 * a * a; /* error of 1st step */
    
    if x0 > x1 {
        x0 = x1;
        x1 += a;
    }
    if y0 > y1 {
        y0 = y1;
        let _ = b; // suppress unused warning if necessary, though b is used later
    }
    y0 += (b + 1) / 2;
    y1 = y0 - b1; /* starting pixel */
    a *= 8 * a;
    b1 = 8 * b * b;

    let mut points = std::collections::HashSet::new();

    let mut draw_pixel = |x: i32, y: i32| {
        if x >= 0 && y >= 0 {
            points.insert((x as u32, y as u32));
        }
    };

    loop {
        if filled {
            for x in x0..=x1 {
                draw_pixel(x, y0);
                draw_pixel(x, y1);
            }
        } else {
            draw_pixel(x1, y0);
            draw_pixel(x0, y0);
            draw_pixel(x0, y1);
            draw_pixel(x1, y1);
        }
        
        let e2 = 2 * err;
        if e2 >= dx {
            x0 += 1;
            x1 -= 1;
            dx += b1;
            err += dx;
        }
        if e2 <= dy {
            y0 += 1;
            y1 -= 1;
            dy += a;
            err += dy;
        }
        
        if x0 > x1 {
            break;
        }
    }
    
    while y0 - y1 < b {
        draw_pixel(x0 - 1, y0);
        draw_pixel(x1 + 1, y0);
        y0 += 1;
        draw_pixel(x0 - 1, y1);
        draw_pixel(x1 + 1, y1);
        y1 -= 1;
    }

    for (px, py) in points {
        edits.extend(apply_pencil(layer, px, py, color));
    }
    
    edits
}

/// Draw a line between two unconstrained i32 canvas coordinates.
pub fn apply_line(layer: &Layer, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) -> Vec<PixelEdit> {
    super::bresenham_line(layer, x0, y0, x1, y1, color)
}

// ── Isometric box helpers ────────────────────────────────────────────────────

/// Generate display pixels for an isometric box **outline** with true 2:1 pixel-art geometry.
///
/// The bounding rect is snapped to the largest 2:1 rhombus that fits inside it.
/// A 2:1 rhombus has width = 2 × height, with edges stepping 2 horizontal pixels
/// for every 1 vertical pixel (≈ 26.565°).
///
/// `height` can be negative (extends upward) or positive (extends downward).
/// Draws **both** the near and far rhombus outlines plus the 4 connecting edges,
/// so the extruded end is always visible.
pub fn iso_box_preview(x0: u32, y0: u32, x1: u32, y1: u32, height: i32, color: Rgba, w: u32, h: u32) -> Vec<(u32, u32, Rgba)> {
    // Normalise drag rect
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));

    let cx = ((x0 + x1) / 2) as i32;
    let cy = ((y0 + y1) / 2) as i32;

    let half_w = ((x1 - x0) / 2) as i32;
    let half_h = ((y1 - y0) / 2) as i32;

    // Fit the largest 2:1 rhombus inside: rw = 2·rh
    let rh = half_h.min(half_w / 2).max(0);
    let rw = rh * 2;

    if rh == 0 {
        return vec![];
    }

    let ry0 = cy - rh;
    let ry1 = cy + rh;

    // Near rhombus vertices
    let t = (cx, ry0);
    let r = (cx + rw, cy);
    let b = (cx, ry1);
    let l = (cx - rw, cy);

    // Far rhombus vertices (offset by height)
    let t2 = (cx, ry0 + height);
    let r2 = (cx + rw, cy + height);
    let b2 = (cx, ry1 + height);
    let l2 = (cx - rw, cy + height);

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut draw_edge = |a: (i32, i32), b: (i32, i32)| {
        for (x, y) in bresenham_positions(a.0, a.1, b.0, b.1) {
            if x < w && y < h {
                let key = (x, y);
                if seen.insert(key) {
                    result.push((x, y, color));
                }
            }
        }
    };

    // ── Near rhombus (always visible) ──
    draw_edge(t, r);
    draw_edge(r, b);
    draw_edge(b, l);
    draw_edge(l, t);

    // ── Far rhombus (the "end" of the extrusion) ──
    draw_edge(t2, r2);
    draw_edge(r2, b2);
    draw_edge(b2, l2);
    draw_edge(l2, t2);

    // ── Connecting edges between the two rhombuses ──
    draw_edge(t, t2);
    draw_edge(r, r2);
    draw_edge(b, b2);
    draw_edge(l, l2);

    result
}

/// Commit an isometric box outline to a layer, returning undo-compatible pixel edits.
pub fn iso_box_pixels(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, height: i32, color: Rgba) -> Vec<PixelEdit> {
    let w = layer.width;
    let h = layer.height;
    let preview = iso_box_preview(x0, y0, x1, y1, height, color, w, h);
    let mut edits = Vec::new();
    for (x, y, c) in preview {
        edits.extend(apply_pencil(layer, x, y, c));
    }
    edits
}

// ── Isometric cylinder helpers ───────────────────────────────────────────────

/// 1-pixel-thick continuous ellipse outline. A pixel is on the border when it is
/// inside the ellipse but at least one of its 4-neighbours is outside.  This
/// guarantees a closed curve with no gaps.
fn ellipse_outline(cx: i32, cy: i32, rx: i32, ry: i32) -> Vec<(i32, i32)> {
    if rx <= 0 || ry <= 0 {
        return vec![(cx, cy)];
    }
    let mut pts = Vec::new();
    let rx_sq = rx * rx;
    let ry_sq = ry * ry;
    let bound = rx_sq * ry_sq;

    let inside = |dx: i32, dy: i32| dx * dx * ry_sq + dy * dy * rx_sq <= bound;

    for dy in -ry..=ry {
        for dx in -rx..=rx {
            if inside(dx, dy) {
                // Border pixel if any 4-neighbour is outside
                if !inside(dx - 1, dy) || !inside(dx + 1, dy)
                    || !inside(dx, dy - 1) || !inside(dx, dy + 1)
                {
                    pts.push((cx + dx, cy + dy));
                }
            }
        }
    }
    pts
}

/// Generate display pixels for an isometric cylinder **outline**.
///
/// The bounding rect is snapped to the largest 2:1 rhombus that fits inside it.
/// The cylinder faces are 2:1 ellipses (rx = rw, ry = rh) inscribed in that rhombus.
///
/// `height` can be negative (extends upward) or positive (extends downward).
///   • The face that is HIGHER on screen is always drawn fully.
///   • The face that is LOWER on screen only shows its front arc (y ≥ cy_of_that_face).
///   • Two vertical-ish connecting edges join the leftmost / rightmost tangent points.
pub fn iso_cylinder_preview(x0: u32, y0: u32, x1: u32, y1: u32, height: i32, color: Rgba, w: u32, h: u32) -> Vec<(u32, u32, Rgba)> {
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));

    let cx = ((x0 + x1) / 2) as i32;
    let cy = ((y0 + y1) / 2) as i32;

    let half_w = ((x1 - x0) / 2) as i32;
    let half_h = ((y1 - y0) / 2) as i32;

    // Fit the largest 2:1 rhombus inside: rw = 2·rh
    let rh = half_h.min(half_w / 2).max(0);
    let rw = rh * 2;

    if rh == 0 {
        return vec![];
    }

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut add_pt = |x: i32, y: i32| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let key = (x as u32, y as u32);
            if seen.insert(key) {
                result.push((x as u32, y as u32, color));
            }
        }
    };

    let cy_top = cy;          // original ellipse centre
    let cy_bot = cy + height; // extruded ellipse centre

    // Determine which face is higher on screen (smaller Y) → fully visible
    if height < 0 {
        // ── Extruding UP: the higher face (cy_bot) is drawn fully ──
        for (x, y) in ellipse_outline(cx, cy_bot, rw, rh) {
            add_pt(x, y);
        }
        // Lower face (cy_top) → front arc only
        for (x, y) in ellipse_outline(cx, cy_top, rw, rh) {
            if y >= cy_top {
                add_pt(x, y);
            }
        }
    } else {
        // ── Extruding DOWN (or zero): higher face (cy_top) is drawn fully ──
        for (x, y) in ellipse_outline(cx, cy_top, rw, rh) {
            add_pt(x, y);
        }
        // Lower face (cy_bot) → front arc only
        for (x, y) in ellipse_outline(cx, cy_bot, rw, rh) {
            if y >= cy_bot {
                add_pt(x, y);
            }
        }
    }

    // ── Connecting edges ──
    for (x, y) in bresenham_positions(cx - rw, cy_top, cx - rw, cy_bot) {
        add_pt(x as i32, y as i32);
    }
    for (x, y) in bresenham_positions(cx + rw, cy_top, cx + rw, cy_bot) {
        add_pt(x as i32, y as i32);
    }

    result
}

/// Commit an isometric cylinder outline to a layer, returning undo-compatible pixel edits.
pub fn iso_cylinder_pixels(layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32, height: i32, color: Rgba) -> Vec<PixelEdit> {
    let w = layer.width;
    let h = layer.height;
    let preview = iso_cylinder_preview(x0, y0, x1, y1, height, color, w, h);
    let mut edits = Vec::new();
    for (x, y, c) in preview {
        edits.extend(apply_pencil(layer, x, y, c));
    }
    edits
}
