// src/canvas.rs
use egui::{Color32, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use crate::theme::Theme;

pub struct CanvasState {
    pub zoom: f32,
    pub offset: Vec2,      // pan offset in screen pixels
    pub texture: Option<TextureHandle>,
    pub dragging_pan: bool,
    pub last_mouse_pos: Option<Pos2>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 12.0,
            offset: Vec2::ZERO,
            texture: None,
            dragging_pan: false,
            last_mouse_pos: None,
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
        let factor = if scroll > 0.0 { 1.1f32 } else { 1.0 / 1.1 };
        self.zoom = (self.zoom * factor).clamp(1.0, 64.0);
    }

    /// Draw checkerboard background + canvas texture
    pub fn draw(&self, painter: &Painter, canvas_rect: Rect, width: u32, height: u32, theme: &Theme) {
        let canvas_screen_rect = self.art_rect(canvas_rect, width, height);
        // Checkerboard — use panel/surface so transparency is visible but subtle
        let cell = self.zoom.max(1.0);
        let cols = (canvas_screen_rect.width() / cell).ceil() as u32;
        let rows = (canvas_screen_rect.height() / cell).ceil() as u32;
        for row in 0..rows {
            for col in 0..cols {
                let color = if (row + col) % 2 == 0 { theme.checker_dark } else { theme.checker_light };
                let rect = Rect::from_min_size(
                    Pos2::new(
                        canvas_screen_rect.min.x + col as f32 * cell,
                        canvas_screen_rect.min.y + row as f32 * cell,
                    ),
                    Vec2::splat(cell),
                );
                painter.rect_filled(rect, 0.0, color);
            }
        }
        // Canvas texture
        if let Some(tex) = &self.texture {
            painter.image(tex.id(), canvas_screen_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    }

    /// Handle scroll zoom and middle-mouse pan
    pub fn handle_input(&mut self, ui: &egui::Ui, canvas_rect: Rect) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        self.zoom_from_scroll(scroll, pointer_pos, canvas_rect);
        let middle_down = ui.input(|i| i.pointer.middle_down());
        let space_held  = ui.input(|i| i.key_down(egui::Key::Space));
        let left_down   = ui.input(|i| i.pointer.primary_down());
        if middle_down || (space_held && left_down) {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }
    }
}
