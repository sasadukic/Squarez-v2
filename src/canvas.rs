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
            zoom: 8.0,
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

    /// Convert screen position to canvas pixel coordinate
    pub fn screen_to_canvas(&self, screen_pos: Pos2, canvas_rect: Rect) -> Option<(u32, u32)> {
        let relative = screen_pos - canvas_rect.min - self.offset;
        let px = (relative.x / self.zoom).floor() as i32;
        let py = (relative.y / self.zoom).floor() as i32;
        if px < 0 || py < 0 { return None; }
        Some((px as u32, py as u32))
    }

    /// Draw checkerboard background + canvas texture
    pub fn draw(&self, painter: &Painter, canvas_rect: Rect, width: u32, height: u32, theme: &Theme) {
        let canvas_screen_rect = Rect::from_min_size(
            canvas_rect.min + self.offset,
            Vec2::new(width as f32 * self.zoom, height as f32 * self.zoom),
        );
        // Checkerboard
        let cell = self.zoom.max(1.0);
        let cols = (canvas_screen_rect.width() / cell).ceil() as u32;
        let rows = (canvas_screen_rect.height() / cell).ceil() as u32;
        for row in 0..rows {
            for col in 0..cols {
                let color = if (row + col) % 2 == 0 { theme.mid } else { theme.light };
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
    pub fn handle_input(&mut self, ui: &egui::Ui) {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = if scroll > 0.0 { 1.1f32 } else { 1.0 / 1.1 };
            self.zoom = (self.zoom * factor).clamp(1.0, 64.0);
        }
        let middle_down = ui.input(|i| i.pointer.middle_down());
        let space_held  = ui.input(|i| i.key_down(egui::Key::Space));
        let left_down   = ui.input(|i| i.pointer.primary_down());
        if middle_down || (space_held && left_down) {
            let delta = ui.input(|i| i.pointer.delta());
            self.offset += delta;
        }
    }
}
