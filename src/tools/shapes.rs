// src/tools/shapes.rs
use crate::project::{Layer, Rgba};
use super::{PixelEdit, apply_pencil, bresenham_positions, IsoMode};

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
pub fn iso_box_preview(
    x0: u32, y0: u32, x1: u32, y1: u32,
    height: i32, color: Rgba, w: u32, h: u32,
    iso_mode: IsoMode
) -> Vec<(u32, u32, Rgba)> {
    let orig_x0 = x0;
    let orig_y0 = y0;
    let orig_x1 = x1;
    let orig_y1 = y1;

    // Normalise drag rect
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut draw_pixel = |x: i32, y: i32, col: Rgba| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let key = (x as u32, y as u32);
            if seen.insert(key) {
                result.push((x as u32, y as u32, col));
            }
        }
    };

    let top_color = color;
    let left_color = [
        (color[0] as f32 * 0.8) as u8,
        (color[1] as f32 * 0.8) as u8,
        (color[2] as f32 * 0.8) as u8,
        color[3],
    ];
    let right_color = [
        (color[0] as f32 * 0.6) as u8,
        (color[1] as f32 * 0.6) as u8,
        (color[2] as f32 * 0.6) as u8,
        color[3],
    ];

    if iso_mode == IsoMode::TopDown || iso_mode == IsoMode::TopDownFill {
        let y_bot = y1 as i32 + height;
        let (y_min, y_max) = if height > 0 { (y1 as i32, y_bot) } else { (y_bot, y1 as i32) };

        if iso_mode == IsoMode::TopDownFill {
            // Fill top face
            for y in y0..=y1 {
                for x in x0..=x1 {
                    draw_pixel(x as i32, y as i32, top_color);
                }
            }
            // Fill front vertical face
            if height != 0 {
                for y in y_min..=y_max {
                    for x in x0..=x1 {
                        draw_pixel(x as i32, y, left_color);
                    }
                }
            }
        }

        // Draw outlines on top
        if height == 0 {
            for x in x0..=x1 {
                draw_pixel(x as i32, y0 as i32, color);
                draw_pixel(x as i32, y1 as i32, color);
            }
            for y in y0..=y1 {
                draw_pixel(x0 as i32, y as i32, color);
                draw_pixel(x1 as i32, y as i32, color);
            }
        } else {
            for x in x0..=x1 {
                draw_pixel(x as i32, y0 as i32, color);
                draw_pixel(x as i32, y1 as i32, color);
            }
            for y in y0..=y1 {
                draw_pixel(x0 as i32, y as i32, color);
                draw_pixel(x1 as i32, y as i32, color);
            }
            for x in x0..=x1 {
                draw_pixel(x as i32, y_bot, color);
            }
            for y in y_min..=y_max {
                draw_pixel(x0 as i32, y, color);
                draw_pixel(x1 as i32, y, color);
            }
        }
        return result;
    }

    // Isometric modes
    let rh = (orig_y1 as i32 - orig_y0 as i32).abs().max((orig_x1 as i32 - orig_x0 as i32).abs() / 2);
    let cy = orig_y0 as i32;
    let cx = if orig_x1 >= orig_x0 { orig_x0 as i32 + 2 * rh } else { orig_x0 as i32 - 2 * rh };

    if rh == 0 {
        return vec![];
    }

    let cy_top = cy.min(cy + height);
    let cy_bottom = cy.max(cy + height);

    macro_rules! draw_rhombus_outline {
        ($cy_center:expr) => {
            // Top half
            for i in 0..=rh {
                let y = $cy_center - rh + i;
                if i == 0 {
                    draw_pixel(cx, y, color);
                    draw_pixel(cx + 1, y, color);
                } else {
                    draw_pixel(cx - 2 * i, y, color);
                    draw_pixel(cx - 2 * i + 1, y, color);
                    draw_pixel(cx + 2 * i, y, color);
                    draw_pixel(cx + 2 * i + 1, y, color);
                }
            }
            // Bottom half
            for i in 0..rh {
                let y = $cy_center + rh - i;
                if i == 0 {
                    draw_pixel(cx, y, color);
                    draw_pixel(cx + 1, y, color);
                } else {
                    draw_pixel(cx - 2 * i, y, color);
                    draw_pixel(cx - 2 * i + 1, y, color);
                    draw_pixel(cx + 2 * i, y, color);
                    draw_pixel(cx + 2 * i + 1, y, color);
                }
            }
        };
    }

    macro_rules! draw_rhombus_fill {
        ($cy_center:expr, $fill_col:expr) => {
            // Top half
            for i in 0..=rh {
                let y = $cy_center - rh + i;
                let (x_start, x_end) = if i == 0 { (cx, cx + 1) } else { (cx - 2 * i, cx + 2 * i + 1) };
                for x in x_start..=x_end {
                    draw_pixel(x, y, $fill_col);
                }
            }
            // Bottom half
            for i in 0..rh {
                let y = $cy_center + rh - i;
                let (x_start, x_end) = if i == 0 { (cx, cx + 1) } else { (cx - 2 * i, cx + 2 * i + 1) };
                for x in x_start..=x_end {
                    draw_pixel(x, y, $fill_col);
                }
            }
        };
    }

    if iso_mode == IsoMode::IsometricFill {
        // 1. Fill body first (left and right faces if height != 0)
        if height != 0 {
            // Left face: x from cx - 2 * rh to cx
            for x in (cx - 2 * rh)..=cx {
                let y_top_x = cy_top + (x - (cx - 2 * rh)) / 2;
                let y_bot_x = y_top_x + height.abs();
                for y in y_top_x..=y_bot_x {
                    draw_pixel(x, y, left_color);
                }
            }
            // Right face: x from cx to cx + 2 * rh + 1
            for x in cx..=(cx + 2 * rh + 1) {
                let y_top_x = cy_top + rh - (x - cx) / 2;
                let y_bot_x = y_top_x + height.abs();
                for y in y_top_x..=y_bot_x {
                    draw_pixel(x, y, right_color);
                }
            }
        }
        // 2. Fill top face
        draw_rhombus_fill!(cy_top, top_color);
    }

    // Draw outlines on top
    if iso_mode == IsoMode::Isometric {
        draw_rhombus_outline!(cy);
        draw_rhombus_outline!(cy + height);

        // Draw all 4 connecting vertical lines
        let (y_min_t, y_max_t) = if height > 0 { (cy - rh, cy - rh + height) } else { (cy - rh + height, cy - rh) };
        for y in y_min_t..=y_max_t {
            draw_pixel(cx, y, color);
            draw_pixel(cx + 1, y, color);
        }
        let (y_min_b, y_max_b) = if height > 0 { (cy + rh, cy + rh + height) } else { (cy + rh + height, cy + rh) };
        for y in y_min_b..=y_max_b {
            draw_pixel(cx, y, color);
            draw_pixel(cx + 1, y, color);
        }
        let (y_min_l, y_max_l) = if height > 0 { (cy, cy + height) } else { (cy + height, cy) };
        for y in y_min_l..=y_max_l {
            draw_pixel(cx - 2 * rh, y, color);
            draw_pixel(cx + 2 * rh + 1, y, color);
        }
    } else {
        // IsometricHidden or IsometricFill outlines
        // Draw top face fully
        draw_rhombus_outline!(cy_top);

        // Draw bottom face front arc only
        for i in 0..rh {
            let y = cy_bottom + rh - i;
            if i == 0 {
                draw_pixel(cx, y, color);
                draw_pixel(cx + 1, y, color);
            } else {
                draw_pixel(cx - 2 * i, y, color);
                draw_pixel(cx - 2 * i + 1, y, color);
                draw_pixel(cx + 2 * i, y, color);
                draw_pixel(cx + 2 * i + 1, y, color);
            }
        }

        // Draw left, right, and front vertical lines
        let (y_min_l, y_max_l) = (cy_top, cy_bottom);
        for y in y_min_l..=y_max_l {
            draw_pixel(cx - 2 * rh, y, color);
            draw_pixel(cx + 2 * rh + 1, y, color);
        }
        let (y_min_f, y_max_f) = (cy_top + rh, cy_bottom + rh);
        for y in y_min_f..=y_max_f {
            draw_pixel(cx, y, color);
            draw_pixel(cx + 1, y, color);
        }
    }

    result
}

pub fn iso_box_pixels(
    layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32,
    height: i32, color: Rgba, iso_mode: IsoMode
) -> Vec<PixelEdit> {
    let w = layer.width;
    let h = layer.height;
    let preview = iso_box_preview(x0, y0, x1, y1, height, color, w, h, iso_mode);
    let mut edits = Vec::new();
    for (x, y, c) in preview {
        edits.extend(apply_pencil(layer, x, y, c));
    }
    edits
}

// ── Isometric cylinder helpers ───────────────────────────────────────────────

fn ellipse_outline(cx: i32, cy: i32, rx: i32, ry: i32) -> Vec<(i32, i32)> {
    if rx <= 0 || ry <= 0 {
        return vec![(cx, cy)];
    }
    
    let x0 = cx - rx;
    let y0 = cy - ry;
    let x1 = cx + rx;
    let y1 = cy + ry;
    
    let mut x0 = x0;
    let mut y0 = y0;
    let mut x1 = x1;
    let mut y1 = y1;
    
    let mut a = (x1 - x0).abs();
    let b = (y1 - y0).abs();
    let mut b1 = b & 1;
    
    let mut dx = 4 * (1 - a) * b * b;
    let mut dy = 4 * (1 + b1) * a * a;
    let mut err = dx + dy + b1 * a * a;
    
    y0 += (b + 1) / 2;
    y1 = y0 - b1;
    a *= 8 * a;
    b1 = 8 * b * b;

    let mut points = std::collections::HashSet::new();

    let mut draw_pixel = |x: i32, y: i32| {
        points.insert((x, y));
    };

    loop {
        draw_pixel(x1, y0);
        draw_pixel(x0, y0);
        draw_pixel(x0, y1);
        draw_pixel(x1, y1);
        
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

    points.into_iter().collect()
}

fn ellipse_fill(cx: i32, cy: i32, rx: i32, ry: i32) -> Vec<(i32, i32)> {
    let mut points = Vec::new();
    if rx <= 0 || ry <= 0 {
        return vec![(cx, cy)];
    }
    for y_offset in -ry..=ry {
        let term = 1.0 - (y_offset as f32 / ry as f32).powi(2);
        let half_w = if term > 0.0 {
            (rx as f32 * term.sqrt()).round() as i32
        } else {
            0
        };
        for x_offset in -half_w..=half_w {
            points.push((cx + x_offset, cy + y_offset));
        }
    }
    points
}

pub fn iso_cylinder_preview(
    x0: u32, y0: u32, x1: u32, y1: u32,
    height: i32, color: Rgba, w: u32, h: u32,
    iso_mode: IsoMode
) -> Vec<(u32, u32, Rgba)> {
    let orig_x0 = x0;
    let orig_y0 = y0;
    let orig_x1 = x1;
    let orig_y1 = y1;

    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut draw_pixel = |x: i32, y: i32, col: Rgba| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let key = (x as u32, y as u32);
            if seen.insert(key) {
                result.push((x as u32, y as u32, col));
            }
        }
    };

    let top_color = color;
    let left_color = [
        (color[0] as f32 * 0.8) as u8,
        (color[1] as f32 * 0.8) as u8,
        (color[2] as f32 * 0.8) as u8,
        color[3],
    ];
    let right_color = [
        (color[0] as f32 * 0.6) as u8,
        (color[1] as f32 * 0.6) as u8,
        (color[2] as f32 * 0.6) as u8,
        color[3],
    ];

    if iso_mode == IsoMode::TopDown || iso_mode == IsoMode::TopDownFill {
        let local_outline = |left: i32, top: i32, right: i32, bottom: i32| -> Vec<(i32, i32)> {
            if left >= right || top >= bottom {
                return vec![];
            }
            let mut x0 = left;
            let mut y0 = top;
            let mut x1 = right;
            let mut y1 = bottom;
            let mut a = (x1 - x0).abs();
            let b = (y1 - y0).abs();
            let mut b1 = b & 1;
            let mut dx = 4 * (1 - a) * b * b;
            let mut dy = 4 * (1 + b1) * a * a;
            let mut err = dx + dy + b1 * a * a;
            y0 += (b + 1) / 2;
            y1 = y0 - b1;
            a *= 8 * a;
            b1 = 8 * b * b;
            let mut pts = std::collections::HashSet::new();
            loop {
                pts.insert((x1, y0));
                pts.insert((x0, y0));
                pts.insert((x0, y1));
                pts.insert((x1, y1));
                let e2 = 2 * err;
                if e2 >= dx { x0 += 1; x1 -= 1; dx += b1; err += dx; }
                if e2 <= dy { y0 += 1; y1 -= 1; dy += a; err += dy; }
                if x0 > x1 { break; }
            }
            while y0 - y1 < b {
                pts.insert((x0 - 1, y0));
                pts.insert((x1 + 1, y0));
                y0 += 1;
                pts.insert((x0 - 1, y1));
                pts.insert((x1 + 1, y1));
                y1 -= 1;
            }
            pts.into_iter().collect()
        };

        if iso_mode == IsoMode::TopDownFill {
            // Fill top face
            let cx_top = ((x0 + x1) / 2) as i32;
            let cy_top = ((y0 + y1) / 2) as i32;
            let rx = ((x1 - x0) / 2) as i32;
            let ry = ((y1 - y0) / 2) as i32;
            let top_pts = ellipse_fill(cx_top, cy_top, rx, ry);
            for &(x, y) in &top_pts {
                draw_pixel(x, y, top_color);
            }
            // Fill body
            if height != 0 {
                for &(x, y) in &top_pts {
                    let y_bot = y + height;
                    let (y_start, y_end) = if height > 0 { (y, y_bot) } else { (y_bot, y) };
                    for y_body in y_start..=y_end {
                        draw_pixel(x, y_body, left_color);
                    }
                }
            }
        }

        // Draw outlines on top
        if height == 0 {
            for (x, y) in local_outline(x0 as i32, y0 as i32, x1 as i32, y1 as i32) {
                draw_pixel(x, y, color);
            }
        } else {
            // Draw top ellipse
            for (x, y) in local_outline(x0 as i32, y0 as i32, x1 as i32, y1 as i32) {
                draw_pixel(x, y, color);
            }
            // Draw bottom ellipse (front half only: y >= cy_bot)
            let cy_bot = ((y0 + y1) / 2) as i32 + height;
            for (x, y) in local_outline(x0 as i32, y0 as i32 + height, x1 as i32, y1 as i32 + height) {
                if y >= cy_bot {
                    draw_pixel(x, y, color);
                }
            }
            // Draw vertical connecting edges at the sides of the ellipse
            let cy_top = ((y0 + y1) / 2) as i32;
            let (y_min, y_max) = if height > 0 { (cy_top, cy_bot) } else { (cy_bot, cy_top) };
            for y in y_min..=y_max {
                draw_pixel(x0 as i32, y, color);
                draw_pixel(x1 as i32, y, color);
            }
        }
        return result;
    }

    // Isometric modes
    let rh = (orig_y1 as i32 - orig_y0 as i32).abs().max((orig_x1 as i32 - orig_x0 as i32).abs() / 2);
    let cy = orig_y0 as i32;
    let cx = if orig_x1 >= orig_x0 { orig_x0 as i32 + 2 * rh } else { orig_x0 as i32 - 2 * rh };
    let rw = rh * 2;

    if rh == 0 {
        return vec![];
    }

    let cy_top = cy.min(cy + height);
    let cy_bottom = cy.max(cy + height);

    if iso_mode == IsoMode::IsometricFill {
        // 1. Fill body first
        if height != 0 {
            let top_pts = ellipse_fill(cx, cy_top, rw, rh);
            for &(x, y) in &top_pts {
                let y_bot = y + height.abs();
                let body_col = if x < cx { left_color } else { right_color };
                for y_body in y..=y_bot {
                    draw_pixel(x, y_body, body_col);
                }
            }
        }
        // 2. Fill top face
        let top_pts = ellipse_fill(cx, cy_top, rw, rh);
        for &(x, y) in &top_pts {
            draw_pixel(x, y, top_color);
        }
    }

    // Draw outlines on top
    if iso_mode == IsoMode::Isometric {
        // Draw top face fully
        for (x, y) in ellipse_outline(cx, cy_top, rw, rh) {
            draw_pixel(x, y, color);
        }
        // Draw bottom face fully
        for (x, y) in ellipse_outline(cx, cy_bottom, rw, rh) {
            draw_pixel(x, y, color);
        }
        // Draw connecting vertical edges
        let (y_min, y_max) = (cy_top, cy_bottom);
        for y in y_min..=y_max {
            draw_pixel(cx - rw, y, color);
            draw_pixel(cx + rw, y, color);
        }
    } else {
        // IsometricHidden or IsometricFill outlines
        // Draw top face fully
        for (x, y) in ellipse_outline(cx, cy_top, rw, rh) {
            draw_pixel(x, y, color);
        }
        // Draw bottom face front arc only
        for (x, y) in ellipse_outline(cx, cy_bottom, rw, rh) {
            if y >= cy_bottom {
                draw_pixel(x, y, color);
            }
        }
        // Draw connecting vertical edges
        let (y_min, y_max) = (cy_top, cy_bottom);
        for y in y_min..=y_max {
            draw_pixel(cx - rw, y, color);
            draw_pixel(cx + rw, y, color);
        }
    }

    result
}

pub fn iso_cylinder_pixels(
    layer: &Layer, x0: u32, y0: u32, x1: u32, y1: u32,
    height: i32, color: Rgba, iso_mode: IsoMode
) -> Vec<PixelEdit> {
    let w = layer.width;
    let h = layer.height;
    let preview = iso_cylinder_preview(x0, y0, x1, y1, height, color, w, h, iso_mode);
    let mut edits = Vec::new();
    for (x, y, c) in preview {
        edits.extend(apply_pencil(layer, x, y, c));
    }
    edits
}
