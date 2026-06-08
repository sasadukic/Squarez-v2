// src/tile_browser.rs
//
// Tile Browser: floating window for browsing Wang/Blob tilesets and selecting tiles to stamp.

use crate::theme::Theme;
use egui::{Color32, Frame, Margin, Vec2, FontId, FontFamily, Pos2, Stroke, StrokeKind};

use crate::wang_blob::*;

#[derive(Clone, Debug)]
pub struct TileBrowser {
    pub open:        bool,
    pub opened_at:   f64,
    /// Selected tile grid position (row, col), if any.
    pub selected:    Option<(u32, u32)>,
}

impl Default for TileBrowser {
    fn default() -> Self {
        Self { open: false, opened_at: 0.0, selected: None }
    }
}

impl TileBrowser {
    pub fn close(&mut self) {
        self.open = false;
        self.selected = None;
    }

    pub fn toggle(&mut self) {
        if self.open { self.close(); } else { self.open = true; }
    }
}

pub fn draw_tile_browser(
    browser: &mut TileBrowser,
    ctx:     &egui::Context,
    theme:   &Theme,
    state:   &WangBlobState,
) {
    if !browser.open { return; }

    let win_w = 520.0;
    let win_h = 420.0;

    // ── Close after 0.15s if clicking outside (Ramp Lab style) ───────────────
    let any_resp = ctx.input(|i| i.pointer.any_click());
    let latest_pos = ctx.input(|i| i.pointer.latest_pos());
    let age = ctx.input(|i| i.time - browser.opened_at);

    if any_resp && age > 0.15 {
        let rect = egui::Rect::from_min_size(
            ctx.screen_rect().min,
            ctx.screen_rect().size(),
        );
        if let Some(pos) = latest_pos {
            if !rect.contains(pos) {
                browser.close();
                return;
            }
        }
    }

    let win_resp = egui::Window::new("##tile_browser_win")
        .id(egui::Id::new("tile_browser_win"))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .min_width(win_w)
        .max_width(win_w)
        .min_height(win_h)
        .max_height(win_h)
        .frame(
            Frame::new()
                .fill(theme.panel)
                .stroke(egui::Stroke::NONE)
                .corner_radius(egui::CornerRadius::ZERO)
                .shadow(egui::Shadow {
                    offset: [0, 14],
                    blur: 36,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                })
                .inner_margin(Margin::ZERO),
        )
        .show(ctx, |ui| {
            ui.set_width(win_w);

            // Manual top padding (no title bar).
            ui.add_space(8.0);

            let ts = state.tile_size.max(1) as f32;
            let (grid_cols, grid_rows) = match state.mode {
                WangBlobMode::Wang => (WANG_COLS, WANG_ROWS),
                WangBlobMode::Blob => (BLOB_COLS, BLOB_ROWS),
                _ => return,
            };

            // ── Tile grid (scrollable) ───────────────────────────────────
            let row_h = ts + 2.0; // tile height + small gap
            let _scroll = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show_viewport(ui, |ui, _vp| {
                    for row in 0..grid_rows {
                        for col in 0..grid_cols {
                            if state.is_gap(row, col) { continue; }

                            let is_selected = browser.selected == Some((row, col));
                            let tile_rect = ui.available_rect_before_wrap();
                            let tile_rect = egui::Rect::from_min_size(
                                egui::Pos2::new(tile_rect.min.x + 8.0, tile_rect.min.y),
                                Vec2::new((ts as f32) * (grid_cols as f32) + 8.0, row_h),
                            );

                            let resp = ui.interact(
                                tile_rect,
                                ui.id().with("tile_").with((row, col)),
                                egui::Sense::click(),
                            );

                            let painter = ui.painter_at(tile_rect);

                            // Background: accent when selected, surface on hover, panel otherwise.
                            let bg = if is_selected { theme.accent } else if resp.hovered() { theme.surface } else { theme.panel };
                            painter.rect_filled(tile_rect, 0.0, bg);

                            // Draw the tile image inside a smaller rect (the actual tile pixels).
                            let single_ts = ts as f32 / grid_cols as f32; // single tile width in the grid cell
                            let inner_rect = egui::Rect::from_min_size(
                                egui::Pos2::new(tile_rect.min.x + 8.0, tile_rect.min.y + 4.0),
                                Vec2::new(single_ts - 16.0, row_h - 8.0),
                            );

                            // Render the tile pixels as a small image.
                            let pixels = state.get_single_tile(row, col);
                            if !pixels.is_empty() {
                                let tile_w = state.tile_size as usize;
                                let tile_h = state.tile_size as usize;
                                let img = egui::ColorImage::from_rgba_unmultiplied(
                                    [tile_w, tile_h],
                                    &pixels,
                                );
                                let tex = ui.ctx().load_texture(
                                    &format!("tile_{}_{}", row, col),
                                    img,
                                    egui::TextureOptions::NEAREST,
                                );
                                let scale = (inner_rect.width() / tile_w as f32).min(inner_rect.height() / tile_h as f32);
                                let draw_w = tile_w as f32 * scale;
                                let draw_h = tile_h as f32 * scale;
                                let draw_rect = egui::Rect::from_min_size(
                                    inner_rect.min,
                                    Vec2::new(draw_w, draw_h),
                                );
                                painter.image(
                                    tex.id(),
                                    draw_rect,
                                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                    egui::Color32::WHITE,
                                );
                            }

                            // Show tile index label.
                            let tidx = state.tile_index(row, col);
                            painter.text(
                                egui::Pos2::new(tile_rect.min.x + 10.0, tile_rect.max.y - 14.0),
                                egui::Align2::LEFT_BOTTOM,
                                &format!("{}", tidx),
                                FontId::new(9.0, FontFamily::Proportional),
                                theme.fg_muted,
                            );

                            // Selected border (accent color).
                            if is_selected {
                                painter.rect_stroke(
                                    tile_rect.shrink(1.0),
                                    0.0,
                                    Stroke::new(2.0, theme.accent),
                                    StrokeKind::Middle,
                                );
                            }

                            if resp.clicked() {
                                browser.selected = Some((row, col));
                            }

                            ui.advance_cursor_after_rect(tile_rect);
                            ui.add_space(2.0);
                        }

                        // Move to next row.
                    }
                });

            ui.add_space(8.0);
        });

    // Close on click outside — same rule as Ramp Lab: guard 0.15s so the
    // click that opened it doesn't immediately close it.
    let now = ctx.input(|i| i.time);
    let age = now - browser.opened_at;
    let win_rect = win_resp.as_ref().map(|r| r.response.rect).unwrap_or(egui::Rect::NOTHING);
    let pointer_in_window = ctx.input(|i| i.pointer.latest_pos())
        .map(|p| win_rect.contains(p))
        .unwrap_or(false);
    let clicked_outside = age > 0.15 && ctx.input(|i| i.pointer.any_click()) && !pointer_in_window;

    if clicked_outside {
        browser.close();
    }
}
