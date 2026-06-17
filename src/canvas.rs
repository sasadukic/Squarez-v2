// src/canvas.rs
use egui::{Color32, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use crate::theme::Theme;

pub struct CanvasState {
    pub zoom: f32,
    pub offset: Vec2,      // pan offset in screen pixels
    pub texture: Option<TextureHandle>,
    pub checker_texture: Option<TextureHandle>,
    pub checker_w: u32,
    pub checker_h: u32,
    pub checker_colors: Option<(Color32, Color32)>,
    pub dragging_pan: bool,
    pub last_mouse_pos: Option<Pos2>,
    pub last_mouse_wheel_time: f64,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 12.0,
            offset: Vec2::ZERO,
            texture: None,
            checker_texture: None,
            checker_w: 0,
            checker_h: 0,
            checker_colors: None,
            dragging_pan: false,
            last_mouse_pos: None,
            last_mouse_wheel_time: 0.0,
        }
    }
}

impl CanvasState {
    /// Upload RGBA pixel data as a GPU texture
    pub fn upload_texture(&mut self, ctx: &egui::Context, pixels: &[u8], width: u32, height: u32) {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            pixels,
        );
        self.texture = Some(ctx.load_texture(
            "canvas",
            image,
            TextureOptions::NEAREST, // pixel-perfect, no bilinear blur
        ));
    }

    pub fn art_rect(&self, canvas_rect: Rect, width: u32, height: u32) -> Rect {
        let size = Vec2::new(width as f32 * self.zoom, height as f32 * self.zoom);
        let origin = canvas_rect.center() - (size * 0.5) + self.offset;
        Rect::from_min_size(origin, size)
    }

    /// Convert screen position to canvas pixel coordinate
    pub fn screen_to_canvas(&self, screen_pos: Pos2, canvas_rect: Rect, width: u32, height: u32) -> Option<(u32, u32)> {
        let art_rect = self.art_rect(canvas_rect, width, height);
        let relative = screen_pos - art_rect.min;
        let px = (relative.x / self.zoom).floor() as i32;
        let py = (relative.y / self.zoom).floor() as i32;
        if px < 0 || py < 0 { return None; }
        Some((px as u32, py as u32))
    }

    /// Returns unconstrained canvas coordinates — can be negative or beyond canvas size.
    /// Use for shape tools so the logical endpoint is never clamped; pixels that fall
    /// outside the canvas are discarded by get_pixel/set_pixel bounds checks.
    pub fn screen_to_canvas_i32(&self, screen_pos: Pos2, canvas_rect: Rect, width: u32, height: u32) -> (i32, i32) {
        let art_rect = self.art_rect(canvas_rect, width, height);
        let relative = screen_pos - art_rect.min;
        let px = (relative.x / self.zoom).floor() as i32;
        let py = (relative.y / self.zoom).floor() as i32;
        (px, py)
    }

    /// Returns sub-pixel canvas coordinates. Used by selection move/resize for smooth
    /// transforms that don't snap to integer pixels until commit.
    pub fn screen_to_canvas_f32(&self, screen_pos: Pos2, canvas_rect: Rect, width: u32, height: u32) -> (f32, f32) {
        let art_rect = self.art_rect(canvas_rect, width, height);
        let relative = screen_pos - art_rect.min;
        (relative.x / self.zoom, relative.y / self.zoom)
    }

    /// Zoom in or out keeping `screen_pos` fixed under the cursor.
    /// `factor` > 1.0 zooms in, < 1.0 zooms out.
    pub fn zoom_at_point(&mut self, factor: f32, screen_pos: Pos2, canvas_rect: Rect) {
        let new_zoom = (self.zoom * factor).clamp(1.0, 64.0);
        if (new_zoom - self.zoom).abs() < 0.001 { return; }
        // canvas-space point under cursor (relative to canvas_rect.center())
        let delta = screen_pos - canvas_rect.center() - self.offset;
        // after zoom the same point must stay at screen_pos
        self.offset = screen_pos - canvas_rect.center() - delta * (new_zoom / self.zoom);
        self.zoom = new_zoom;
    }

    /// Fit the canvas inside `canvas_rect` with a small margin, centered.
    pub fn zoom_to_fit(&mut self, canvas_rect: Rect, width: u32, height: u32) {
        let margin = 32.0;
        let avail = canvas_rect.size() - Vec2::splat(margin * 2.0);
        let zoom_x = avail.x / width as f32;
        let zoom_y = avail.y / height as f32;
        self.zoom = zoom_x.min(zoom_y).clamp(1.0, 64.0);
        self.offset = Vec2::ZERO;
    }

    pub fn zoom_from_scroll(&mut self, scroll: f32, pointer_pos: Option<Pos2>, canvas_rect: Rect) {
        if scroll == 0.0 || !pointer_pos.is_some_and(|pos| canvas_rect.contains(pos)) {
            return;
        }
        let factor = if scroll > 0.0 { 1.05f32 } else { 1.0 / 1.05 };
        self.zoom_at_point(factor, pointer_pos.unwrap(), canvas_rect);
    }

    /// Draw checkerboard background + canvas texture.
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        canvas_rect: Rect,
        width: u32,
        height: u32,
        theme: &Theme,
    ) {
        let canvas_screen_rect = self.art_rect(canvas_rect, width, height);
        let clipped = painter.with_clip_rect(canvas_screen_rect);

        // Rebuild checkerboard texture if width, height, or colors changed
        let colors = (theme.checker_dark, theme.checker_light);
        if self.checker_texture.is_none()
            || self.checker_w != width
            || self.checker_h != height
            || self.checker_colors != Some(colors)
        {
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            for y in 0..height {
                for x in 0..width {
                    let color = if (x + y) % 2 == 0 { theme.checker_dark } else { theme.checker_light };
                    let idx = ((y * width + x) * 4) as usize;
                    pixels[idx] = color.r();
                    pixels[idx + 1] = color.g();
                    pixels[idx + 2] = color.b();
                    pixels[idx + 3] = color.a();
                }
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                &pixels,
            );
            self.checker_texture = Some(ctx.load_texture(
                "checkerboard",
                image,
                TextureOptions::NEAREST,
            ));
            self.checker_w = width;
            self.checker_h = height;
            self.checker_colors = Some(colors);
        }

        // Draw checkerboard
        if let Some(tex) = &self.checker_texture {
            clipped.image(
                tex.id(),
                canvas_screen_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // Canvas texture
        if let Some(tex) = &self.texture {
            clipped.image(tex.id(), canvas_screen_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    }

    /// Handle scroll zoom, pinch zoom, and panning/moving
    pub fn handle_input(&mut self, ui: &egui::Ui, canvas_rect: Rect) {
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let now = ui.input(|i| i.time);
        
        // 1. Pinch zoom (Touchpad pinch in/out zooms)
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            if let Some(pos) = pointer_pos {
                if canvas_rect.contains(pos) {
                    self.zoom_at_point(zoom_delta, pos, canvas_rect);
                }
            }
        }
        
        // 2. Event-based scroll zoom vs panning
        // Differentiate:
        // - Mouse scroll wheel (MouseWheelUnit::Line) -> zooms
        // - Trackpad panning (MouseWheelUnit::Point) -> pans/moves
        // Also support zoom modifiers if they occur
        let mut is_mouse_wheel = false;
        let mut is_trackpad_scroll = false;
        let mut scroll_y = 0.0;
        let mut scroll_vector = Vec2::ZERO;

        ui.input(|i| {
            for event in &i.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = event {
                    if *unit == egui::MouseWheelUnit::Line {
                        is_mouse_wheel = true;
                        scroll_y += delta.y;
                    } else {
                        is_trackpad_scroll = true;
                        scroll_vector += *delta;
                    }
                }
            }
        });

        if is_mouse_wheel {
            self.last_mouse_wheel_time = now;
            // Mouse scroll zooms in/out directly (without modifiers)
            self.zoom_from_scroll(scroll_y, pointer_pos, canvas_rect);
        } else if is_trackpad_scroll {
            // Touchpad two-finger drag moves the canvas
            if now - self.last_mouse_wheel_time > 0.3 {
                self.offset += scroll_vector;
            }
        } else {
            // Fallback for general smooth_scroll_delta if events were consumed
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
            if scroll_delta != Vec2::ZERO {
                let has_zoom_modifier = ui.input(|i| {
                    i.modifiers.command || i.modifiers.mac_cmd || i.modifiers.ctrl || i.modifiers.alt
                });
                if has_zoom_modifier {
                    self.zoom_from_scroll(scroll_delta.y, pointer_pos, canvas_rect);
                } else if now - self.last_mouse_wheel_time > 0.3 {
                    self.offset += scroll_delta;
                }
            }
        }
        
        // 3. Middle-mouse press + drag pans
        let middle_down = ui.input(|i| i.pointer.middle_down());
        let space_held  = ui.input(|i| i.key_down(egui::Key::Space));
        let left_down   = ui.input(|i| i.pointer.primary_down());
        if middle_down || (space_held && left_down) {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }
    }
}
