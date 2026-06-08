// src/palette_browser.rs
//
// Lospec Palette Browser: embedded palette collection browser window.

use crate::project::Rgba;
use crate::theme::{Theme, FONT_SIZE_SM};
use egui::{Color32, Frame, Margin, Vec2, FontId, FontFamily, Pos2, Order};

// ─── Data ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct LospecPalette {
    pub slug:   String,
    pub name:   String,
    pub author: String,
    pub colors: Vec<String>, // hex strings e.g. "ff0044"
}

#[derive(Clone, Debug)]
pub struct PaletteBrowser {
    pub open:       bool,
    pub opened_at:  f64,
    pub selected:   Option<usize>, // index into palettes
    pub palettes:   Vec<LospecPalette>,
    pub load_dialog: Option<LoadDialog>,
    // Double-click tracking
    last_click_time: f64,
    last_click_idx:  Option<usize>,
    // Save palette name prompt
    pub save_dialog_name:   String,
    pub save_dialog_colors: Option<Vec<Rgba>>,
    // Right-click theme menu state: (palette_idx, screen_pos)
    theme_menu_palette: Option<(usize, Pos2)>,
    // Which palette is currently used as the app theme (if any)
    pub theme_from_palette: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum LoadDialog {
    Pending { palette_idx: usize },
}

impl Default for PaletteBrowser {
    fn default() -> Self {
        Self {
            open:        false,
            opened_at:   0.0,
            selected:    None,
            palettes:    load_palettes(),
            load_dialog: None,
            last_click_time: -1.0,
            last_click_idx:  None,
            save_dialog_name:   String::new(),
            save_dialog_colors: None,
            theme_menu_palette: None,
            theme_from_palette: None,
        }
    }
}

impl PaletteBrowser {
    /// Add the current project palette as a new user palette at the top.
    pub fn add_user_palette(&mut self, name: String, colors: Vec<Rgba>) {
        let hex_colors: Vec<String> = colors.iter().map(|c| {
            format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
        }).collect();
        let slug = name.to_lowercase().replace(" ", "-").replace(|c: char| !c.is_alphanumeric() && c != '-', "");
        self.palettes.insert(0, LospecPalette {
            slug,
            name,
            author: "User".to_string(),
            colors: hex_colors,
        });
        self.selected = Some(0);
    }
}

fn load_palettes() -> Vec<LospecPalette> {
    const JSON: &str = include_str!("../assets/lospec_palettes.json");
    let raw: Vec<serde_json::Value> = serde_json::from_str(JSON).unwrap_or_default();
    raw.into_iter()
        .filter_map(|v| {
            let slug = v.get("slug")?.as_str()?.to_string();
            let name = v.get("name")?.as_str()?.to_string();
            let author = v.get("author").and_then(|a| a.as_str()).unwrap_or("").to_string();
            let colors = v.get("colors")?
                .as_array()?
                .iter()
                .filter_map(|c| c.as_str().map(|s| s.to_string()))
                .collect();
            Some(LospecPalette { slug, name, author, colors })
        })
        .collect()
}

// ─── UI ──────────────────────────────────────────────────────────────────────

pub fn draw_palette_browser(
    browser: &mut PaletteBrowser,
    ctx:     &egui::Context,
    theme:   &mut Theme,
    default_theme: &Theme,
    project: &mut crate::project::Project,
) {
    // ── Browser window ───────────────────────────────────────────────────────
    if browser.open {
        let win_w = 520.0;
        let win_h = 420.0;

        let win_resp = egui::Window::new("##palette_browser_win")
            .id(egui::Id::new("palette_browser_win"))
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

                ui.add_space(8.0);

                // ── Palette list ─────────────────────────────────────────
                let mut to_remove: Option<usize> = None;
                let _scroll = egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show_viewport(ui, |ui, _vp| {
                        let row_h = 40.0;
                        let gap = 2.0;
                        for (idx, pal) in browser.palettes.iter().enumerate() {
                            let is_selected = browser.selected == Some(idx);

                            let row_rect = ui.available_rect_before_wrap();
                            let row_rect = egui::Rect::from_min_size(
                                egui::Pos2::new(row_rect.min.x + 8.0, row_rect.min.y),
                                Vec2::new(win_w - 16.0, row_h),
                            );

                            let resp = ui.interact(row_rect, ui.id().with("pal_").with(idx), egui::Sense::click());
                            let bg = if is_selected { theme.accent } else if resp.hovered() { theme.surface } else { theme.panel };
                            let painter = ui.painter_at(row_rect);
                            painter.rect_filled(row_rect, 0.0, bg);

                            // Name + author
                            let text_x = row_rect.min.x + 8.0;
                            let text_y = row_rect.min.y + 4.0;
                            painter.text(
                                egui::Pos2::new(text_x, text_y),
                                egui::Align2::LEFT_TOP,
                                &pal.name,
                                egui::FontId::new(12.0, egui::FontFamily::Proportional),
                                if is_selected { theme.fg } else { theme.fg_desc },
                            );
                            if !pal.author.is_empty() {
                                painter.text(
                                    egui::Pos2::new(text_x, text_y + 16.0),
                                    egui::Align2::LEFT_TOP,
                                    &format!("by {}", pal.author),
                                    egui::FontId::new(10.0, egui::FontFamily::Proportional),
                                    theme.fg_muted,
                                );
                            }

                            // Color strip (right side)
                            let strip_h = 16.0;
                            let strip_w = (win_w - 16.0) * 0.38;
                            let strip_x = row_rect.max.x - strip_w - 32.0;
                            let strip_y = row_rect.center().y - strip_h / 2.0;
                            let swatch_w = strip_w / pal.colors.len().max(1) as f32;
                            for (ci, hex) in pal.colors.iter().enumerate() {
                                let c = hex_to_color32(hex);
                                let srect = egui::Rect::from_min_size(
                                    egui::Pos2::new(strip_x + ci as f32 * swatch_w, strip_y),
                                    Vec2::new(swatch_w, strip_h),
                                );
                                painter.rect_filled(srect, 0.0, c);
                            }
                            // Delete icon
                            let delete_rect = egui::Rect::from_center_size(
                                egui::Pos2::new(row_rect.max.x - 16.0, row_rect.center().y),
                                Vec2::splat(16.0),
                            );
                            let delete_resp = ui.interact(delete_rect, ui.id().with("del_").with(idx), egui::Sense::click());
                            let delete_tint = if delete_resp.hovered() { theme.fg } else { theme.fg_muted };
                            let delete_icon = egui::Image::new(egui::include_image!("../assets/icons/delete.svg"))
                                .tint(delete_tint)
                                .fit_to_exact_size(Vec2::splat(16.0));
                            ui.put(delete_rect, delete_icon);
                            if delete_resp.clicked() {
                                to_remove = Some(idx);
                            }

                            if resp.clicked() && !delete_resp.hovered() {
                                browser.selected = Some(idx);
                                let now = ui.input(|i| i.time);
                                let is_dbl = browser.last_click_idx == Some(idx)
                                    && now - browser.last_click_time < 0.4;
                                browser.last_click_time = now;
                                browser.last_click_idx = Some(idx);
                                if is_dbl {
                                    browser.load_dialog = Some(LoadDialog::Pending { palette_idx: idx });
                                }
                            }

                            if resp.secondary_clicked() && pal.colors.len() == 6 {
                                let pos = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(Pos2::ZERO);
                                browser.theme_menu_palette = Some((idx, pos));
                            }

                            ui.advance_cursor_after_rect(row_rect);
                            ui.add_space(gap);
                        }
                    });

                ui.add_space(8.0);

                if let Some(idx) = to_remove {
                    browser.palettes.remove(idx);
                    match browser.selected {
                        Some(s) if s == idx => browser.selected = None,
                        Some(s) if s > idx  => browser.selected = Some(s - 1),
                        _ => {}
                    }
                }
            });

        // Close on click outside — same rule as Ramp Lab:
        // guard 0.15s so the click that opened it doesn't immediately close it.
        let now = ctx.input(|i| i.time);
        let age = now - browser.opened_at;
        let win_rect = win_resp
            .as_ref()
            .map(|r| r.response.rect)
            .unwrap_or(egui::Rect::NOTHING);
        let pointer_in_window = ctx
            .input(|i| i.pointer.latest_pos())
            .map(|p| win_rect.contains(p))
            .unwrap_or(false);
        let clicked_outside = age > 0.15
            && ctx.input(|i| i.pointer.any_click())
            && !pointer_in_window;

        if clicked_outside {
            browser.open = false;
        }
    }

    // ── Theme context menu (rigth-click a 6‑color palette) ─────────────────
    if let Some((menu_idx, menu_pos)) = browser.theme_menu_palette {
        let is_theme = browser.theme_from_palette == Some(menu_idx);
        let mut close_menu = false;

        egui::Area::new("theme_menu".into())
            .fixed_pos(menu_pos)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let menu_w = 140.0;
                let item_h = 22.0;
                let inner = Margin::symmetric(4, 2);
                Frame::new()
                    .fill(theme.panel)
                    .stroke(egui::Stroke::new(1.0, theme.surface))
                    .corner_radius(0)
                    .shadow(egui::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(80),
                    })
                    .inner_margin(inner)
                    .show(ui, |ui| {
                        ui.set_min_width(menu_w);

                        let label = if is_theme { "Back to default" } else { "Use as theme" };
                        let item_rect = {
                            let (r, _) = ui.allocate_exact_size(
                                Vec2::new(menu_w, item_h),
                                egui::Sense::click(),
                            );
                            r
                        };
                        let item_resp = ui.interact(item_rect, ui.id().with("theme_item"), egui::Sense::click());
                        let item_bg = if item_resp.hovered() { theme.surface } else { Color32::TRANSPARENT };
                        if item_bg != Color32::TRANSPARENT {
                            ui.painter().rect_filled(item_rect, 0.0, item_bg);
                        }
                        ui.painter().text(
                            item_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
                            if item_resp.hovered() { theme.fg } else { theme.fg_desc },
                        );

                        if item_resp.clicked() {
                            if is_theme {
                                // Restore default
                                *theme = default_theme.clone();
                                browser.theme_from_palette = None;
                            } else {
                                apply_palette_as_theme(theme, &browser.palettes[menu_idx]);
                                browser.theme_from_palette = Some(menu_idx);
                            }
                            close_menu = true;
                        }
                    });
            });

        // Close on Escape or click outside menu
        let any_click = ctx.input(|i| i.pointer.any_click());
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            close_menu = true;
        }
        if any_click && !close_menu {
            let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
            if let Some(pos) = pointer_pos {
                // Approximate menu rect
                let menu_rect = egui::Rect::from_min_size(menu_pos, Vec2::new(140.0, 28.0));
                if !menu_rect.contains(pos) {
                    close_menu = true;
                }
            }
        }
        if close_menu {
            browser.theme_menu_palette = None;
        }
    }

    // ── Load dialog ──────────────────────────────────────────────────────────
    if let Some(LoadDialog::Pending { palette_idx }) = browser.load_dialog.clone() {
        let dialog_resp = rfd::MessageDialog::new()
            .set_title("Load Palette")
            .set_description(&format!(
                "Load '{}' into the current project?\n\nYes = Replace current palette\nNo = Append colors\nCancel = Do nothing",
                browser.palettes[palette_idx].name
            ))
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();

        match dialog_resp {
            rfd::MessageDialogResult::Yes => {
                let pal = &browser.palettes[palette_idx];
                project.palette = pal.colors.iter()
                    .map(|hex| hex_to_rgba(hex))
                    .collect();
            }
            rfd::MessageDialogResult::No => {
                let pal = &browser.palettes[palette_idx];
                for hex in &pal.colors {
                    let rgba = hex_to_rgba(hex);
                    if !project.palette.contains(&rgba) {
                        project.palette.push(rgba);
                    }
                }
            }
            _ => {}
        }
        browser.load_dialog = None;
    }

    // ── Save name dialog ─────────────────────────────────────────────────────
    if browser.save_dialog_colors.is_some() {
        let dialog_w = 280.0;
        let mut should_save = false;
        let mut should_cancel = false;
        egui::Window::new("##save_palette_name")
            .id(egui::Id::new("save_palette_name"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .resizable(false)
            .collapsible(false)
            .title_bar(false)
            .min_width(dialog_w)
            .max_width(dialog_w)
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
                    .inner_margin(Margin::symmetric(12, 10)),
            )
            .show(ctx, |ui| {
                ui.set_width(dialog_w);

                ui.label(
                    egui::RichText::new("Save Palette")
                        .size(13.0)
                        .color(theme.fg)
                        .font(FontId::new(13.0, FontFamily::Proportional)),
                );
                ui.add_space(8.0);

                let edit = egui::TextEdit::singleline(&mut browser.save_dialog_name)
                    .desired_width(dialog_w - 24.0)
                    .font(FontId::new(12.0, FontFamily::Proportional))
                    .margin(Margin::symmetric(6, 4));
                let resp = ui.add(edit);
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                    let btn_h = 19.0;

                    let ok_label = "OK";
                    let ok_w = ok_label.chars().count() as f32 * 6.0 + 16.0;
                    let (ok_rect, ok_resp) = ui.allocate_exact_size(
                        Vec2::new(ok_w, btn_h), egui::Sense::click(),
                    );
                    let ok_bg = if ok_resp.hovered() { theme.surface } else { Color32::TRANSPARENT };
                    if ok_bg != Color32::TRANSPARENT {
                        ui.painter().rect_filled(ok_rect, 0.0, ok_bg);
                    }
                    let ok_col = if ok_resp.hovered() { theme.fg } else { theme.fg_desc };
                    ui.painter().text(
                        ok_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        ok_label,
                        FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
                        ok_col,
                    );
                    if ok_resp.clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        should_save = true;
                    }

                    let cancel_label = "Cancel";
                    let cancel_w = cancel_label.chars().count() as f32 * 6.0 + 16.0;
                    let (cancel_rect, cancel_resp) = ui.allocate_exact_size(
                        Vec2::new(cancel_w, btn_h), egui::Sense::click(),
                    );
                    let cancel_bg = if cancel_resp.hovered() { theme.surface } else { Color32::TRANSPARENT };
                    if cancel_bg != Color32::TRANSPARENT {
                        ui.painter().rect_filled(cancel_rect, 0.0, cancel_bg);
                    }
                    let cancel_col = if cancel_resp.hovered() { theme.fg } else { theme.fg_desc };
                    ui.painter().text(
                        cancel_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        cancel_label,
                        FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
                        cancel_col,
                    );
                    if cancel_resp.clicked() {
                        should_cancel = true;
                    }
                });
            });

        if should_save {
            let name = browser.save_dialog_name.trim().to_string();
            if !name.is_empty() {
                if let Some(colors) = browser.save_dialog_colors.take() {
                    browser.add_user_palette(name, colors);
                }
            }
            browser.save_dialog_colors = None;
            browser.save_dialog_name.clear();
        } else if should_cancel {
            browser.save_dialog_colors = None;
            browser.save_dialog_name.clear();
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn apply_palette_as_theme(theme: &mut Theme, pal: &LospecPalette) {
    let cols: Vec<Color32> = pal.colors.iter().map(|h| hex_to_color32(h)).collect();
    if cols.len() >= 6 {
        theme.bg       = cols[0];
        theme.panel    = cols[1];
        theme.surface  = cols[2];
        theme.border   = cols[2];
        theme.accent   = cols[3];
        theme.muted    = cols[4];
        theme.fg_desc  = cols[4];
        theme.fg_muted = cols[4];
        theme.fg       = cols[5];
    }
}

fn hex_to_color32(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    Color32::from_rgb(r, g, b)
}

fn hex_to_rgba(hex: &str) -> Rgba {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    [r, g, b, 255]
}

fn do_export_png(pal: &LospecPalette) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("PNG", &["png"])
        .set_file_name(&format!("{}.png", pal.slug))
        .save_file()
    {
        let colors: Vec<Rgba> = pal.colors.iter().map(|h| hex_to_rgba(h)).collect();
        if let Err(e) = crate::io::export::export_palette_png(&path, &colors) {
            eprintln!("Failed to export palette PNG: {}", e);
        }
    }
}
