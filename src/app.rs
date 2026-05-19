// src/app.rs
use egui::{
    CentralPanel, Color32, FontId, FontFamily, Frame, Margin, RichText,
    SidePanel, TopBottomPanel, Vec2,
};
use crate::animation::{FrameThumbnail, PlaybackState};
use crate::canvas::CanvasState;
use crate::color::{ColorState, PickerMode};
use crate::color::hsv::{rgba_to_hsv, hsv_to_rgba};
use crate::color::oklab::{rgba_to_oklab, oklab_to_rgba};
use crate::history::{Command, UndoStack};
use crate::io::sqr::{load_sqr, save_sqr};
use crate::io::export::{export, ExportFormat, ExportOptions};
use crate::layers::composite_frame;
use crate::project::Project;
use crate::theme::{load_fonts, Theme, FONT_SIZE_SM, FONT_SIZE_MD};
use crate::tools::{ActiveTool, apply_pencil, apply_eraser, apply_fill, apply_eyedropper};

pub struct App {
    pub project: Project,
    pub theme: Theme,
    pub canvas: CanvasState,
    pub color_state: ColorState,
    pub active_tool: ActiveTool,
    pub undo_stack: UndoStack,
    pub playback: PlaybackState,
    pub thumbnails: Vec<Vec<FrameThumbnail>>,
    pub current_path: Option<std::path::PathBuf>,
    // Tool drag state
    drag_start: Option<(u32, u32)>,
    stroke_edits: Vec<crate::tools::PixelEdit>,
    canvas_dirty: bool,
    #[allow(dead_code)]
    composite_cache: Option<Vec<u8>>,
    // New project dialog
    show_new_dialog: bool,
    new_width: u32,
    new_height: u32,
    new_name: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        load_fonts(&cc.egui_ctx);
        let project = Project::new(32, 32, "Untitled".to_string());
        let thumbnails = project.animations.iter()
            .map(|a| a.frames.iter().map(|_| FrameThumbnail::default()).collect())
            .collect();
        Self {
            project,
            theme: Theme::default(),
            canvas: CanvasState::default(),
            color_state: ColorState::default(),
            active_tool: ActiveTool::Pencil,
            undo_stack: UndoStack::new(),
            playback: PlaybackState::default(),
            thumbnails,
            current_path: None,
            drag_start: None,
            stroke_edits: Vec::new(),
            canvas_dirty: true,
            composite_cache: None,
            show_new_dialog: false,
            new_width: 32,
            new_height: 32,
            new_name: "Untitled".to_string(),
        }
    }

    fn composite_active_frame(&mut self) -> Vec<u8> {
        let frame = self.project.active_frame_ref();
        composite_frame(frame, self.project.canvas_width, self.project.canvas_height)
    }

    fn rebuild_canvas_texture(&mut self, ctx: &egui::Context) {
        let pixels = self.composite_active_frame();
        self.canvas.upload_texture(ctx, &pixels, self.project.canvas_width, self.project.canvas_height);
        self.canvas_dirty = false;
    }

    // ── Text helpers ──────────────────────────────────────────────────────
    /// Primary text — white, medium size
    fn label_md(&self, text: &str) -> RichText {
        RichText::new(text)
            .font(FontId::new(FONT_SIZE_MD, FontFamily::Monospace))
            .color(self.theme.fg)
    }
    /// Primary text — white, small
    fn label(&self, text: &str) -> RichText {
        RichText::new(text)
            .font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace))
            .color(self.theme.fg)
    }
    /// Secondary / description text — white-80, small
    fn label_desc(&self, text: &str) -> RichText {
        RichText::new(text)
            .font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace))
            .color(self.theme.fg_desc)
    }
    /// Muted text — white-60, small
    fn label_muted(&self, text: &str) -> RichText {
        RichText::new(text)
            .font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace))
            .color(self.theme.fg_muted)
    }

    // ── Panel frame helpers ───────────────────────────────────────────────
    fn panel_frame(&self) -> Frame {
        Frame::new()
            .fill(self.theme.panel)
            .inner_margin(Margin::same(6))
    }
    fn toolbar_frame(&self) -> Frame {
        Frame::new()
            .fill(self.theme.panel)
            .inner_margin(Margin::symmetric(4, 6))
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);

        // Advance playback
        let fps   = self.project.active_anim().fps;
        let total = self.project.active_anim().frames.len();
        if self.playback.tick(fps, &mut self.project.active_frame, total) {
            self.canvas_dirty = true;
        }

        // Keyboard shortcuts
        let ctrl_z = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl);
        let ctrl_y = ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl);
        if ctrl_z { self.undo_stack.undo(&mut self.project); self.canvas_dirty = true; }
        if ctrl_y { self.undo_stack.redo(&mut self.project); self.canvas_dirty = true; }

        // ── Menu bar ──────────────────────────────────────────────────────
        TopBottomPanel::top("menubar")
            .frame(self.panel_frame())
            .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(self.label("File"), |ui| {
                    if ui.button(self.label("New")).clicked() {
                        self.show_new_dialog = true; ui.close_menu();
                    }
                    if ui.button(self.label("Open")).clicked() {
                        if let Some(path) = rfd_open() {
                            if let Ok(p) = load_sqr(&path) {
                                self.project = p;
                                self.canvas_dirty = true;
                                self.current_path = Some(path);
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button(self.label("Save")).clicked() {
                        let path = self.current_path.clone()
                            .unwrap_or_else(|| std::path::PathBuf::from("untitled.sqr"));
                        let _ = save_sqr(&self.project, &path);
                        ui.close_menu();
                    }
                    if ui.button(self.label("Save As")).clicked() {
                        if let Some(path) = rfd_save() {
                            let _ = save_sqr(&self.project, &path);
                            self.current_path = Some(path);
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button(self.label("Edit"), |ui| {
                    if ui.button(self.label("Undo  Ctrl+Z")).clicked() {
                        self.undo_stack.undo(&mut self.project);
                        self.canvas_dirty = true; ui.close_menu();
                    }
                    if ui.button(self.label("Redo  Ctrl+Y")).clicked() {
                        self.undo_stack.redo(&mut self.project);
                        self.canvas_dirty = true; ui.close_menu();
                    }
                });
                ui.menu_button(self.label("Export"), |ui| {
                    let ai = self.project.active_animation;
                    if ui.button(self.label("PNG")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Png, scale: 1, animation_index: ai };
                        let _ = export(&self.project, std::path::Path::new("export.png"), opts);
                        ui.close_menu();
                    }
                    if ui.button(self.label("GIF")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Gif, scale: 1, animation_index: ai };
                        let _ = export(&self.project, std::path::Path::new("export.gif"), opts);
                        ui.close_menu();
                    }
                    if ui.button(self.label("Spritesheet")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Spritesheet, scale: 1, animation_index: ai };
                        let _ = export(&self.project, std::path::Path::new("spritesheet.png"), opts);
                        ui.close_menu();
                    }
                });

                // Right-aligned project name + canvas size
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let zoom = self.canvas.zoom as u32;
                    let w = self.project.canvas_width;
                    let h = self.project.canvas_height;
                    ui.label(self.label_muted(&format!("{}x{}  {}x", w, h, zoom)));
                    ui.label(self.label_desc(&self.project.name.clone()));
                });
            });
        });

        // ── Timeline panel (bottom) ────────────────────────────────────────
        TopBottomPanel::bottom("timeline")
            .min_height(36.0)
            .frame(self.panel_frame())
            .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // Clip selector
                let anim_name = self.project.active_anim().name.clone();
                egui::ComboBox::from_id_salt("clip_selector")
                    .selected_text(self.label_desc(&anim_name))
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for (i, anim) in self.project.animations.iter().enumerate() {
                            let name = anim.name.clone();
                            if ui.selectable_label(
                                self.project.active_animation == i,
                                self.label_desc(&name),
                            ).clicked() {
                                self.project.active_animation = i;
                                self.project.active_frame = 0;
                                self.canvas_dirty = true;
                            }
                        }
                    });
                if ui.small_button(self.label_muted("+")).clicked() {
                    let (w, h) = (self.project.canvas_width, self.project.canvas_height);
                    let n = self.project.animations.len() + 1;
                    self.project.animations.push(
                        crate::project::Animation::new(format!("Anim {}", n), w, h)
                    );
                }

                ui.separator();

                // Frame strip
                let num_frames = self.project.active_anim().frames.len();
                for i in 0..num_frames {
                    let selected = self.project.active_frame == i;
                    let text = if selected {
                        self.label(&format!("F{}", i + 1))
                    } else {
                        self.label_desc(&format!("F{}", i + 1))
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        self.project.active_frame = i;
                        self.canvas_dirty = true;
                    }
                }
                if ui.small_button(self.label_muted("+")).clicked() {
                    let (w, h) = (self.project.canvas_width, self.project.canvas_height);
                    let idx = self.project.active_anim().frames.len();
                    self.undo_stack.push(Command::AddFrame {
                        animation_id: self.project.active_animation,
                        index: idx,
                    });
                    self.project.active_anim_mut().frames.push(crate::project::Frame::new(w, h));
                    if self.thumbnails.len() > self.project.active_animation {
                        self.thumbnails[self.project.active_animation].push(FrameThumbnail::default());
                    }
                }

                ui.separator();

                // Playback controls
                if ui.small_button(self.label_muted("|<")).clicked() {
                    self.project.active_frame = 0; self.canvas_dirty = true;
                }
                if ui.small_button(self.label_muted("<")).clicked() {
                    if self.project.active_frame > 0 { self.project.active_frame -= 1; }
                    self.canvas_dirty = true;
                }
                let play_lbl = if self.playback.is_playing { self.label("||") } else { self.label(">") };
                if ui.button(play_lbl).clicked() {
                    self.playback.is_playing = !self.playback.is_playing;
                }
                let t = self.project.active_anim().frames.len();
                if ui.small_button(self.label_muted(">")).clicked() {
                    self.project.active_frame = (self.project.active_frame + 1) % t;
                    self.canvas_dirty = true;
                }

                ui.separator();

                ui.label(self.label_muted("FPS"));
                let fps = &mut self.project.active_anim_mut().fps;
                let mut fps_val = *fps as u32;
                if ui.add(
                    egui::DragValue::new(&mut fps_val)
                        .range(1..=60)
                        .suffix(" fps")
                ).changed() {
                    *fps = fps_val as u8;
                }
            });
        });

        // ── Left toolbar ──────────────────────────────────────────────────
        SidePanel::left("toolbar")
            .exact_width(36.0)
            .frame(self.toolbar_frame())
            .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(2.0);

                // Drawing tools
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Pencil,   "P");
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Eraser,   "E");
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Fill,     "G");
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Eyedropper, "I");

                ui.add_space(4.0);
                ui.add(egui::Separator::default().horizontal());
                ui.add_space(4.0);

                // Shape tools
                tool_btn(ui, &mut self.active_tool, &self.theme,
                    ActiveTool::Rectangle { filled: false }, "R");
                tool_btn(ui, &mut self.active_tool, &self.theme,
                    ActiveTool::Ellipse { filled: false }, "O");
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Line, "L");

                ui.add_space(4.0);
                ui.add(egui::Separator::default().horizontal());
                ui.add_space(4.0);

                // Selection / Move
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::RectSelect, "S");
                tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Move,       "M");
            });
        });

        // ── Right color panel ─────────────────────────────────────────────
        SidePanel::right("color_panel")
            .min_width(160.0)
            .max_width(200.0)
            .frame(self.panel_frame())
            .show(ctx, |ui| {
            // ── FG / BG swatches ──────────────────────────────────────
            ui.label(self.label_md("Color"));
            ui.add_space(4.0);

            let fg = self.color_state.foreground;
            let bg_c = self.color_state.background;
            let fg_color = Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], fg[3]);
            let bg_color = Color32::from_rgba_unmultiplied(bg_c[0], bg_c[1], bg_c[2], bg_c[3]);

            ui.horizontal(|ui| {
                ui.label(self.label_desc("FG"));
                let fg_btn = egui::Button::new("  ")
                    .fill(fg_color)
                    .stroke(egui::Stroke::new(1.0, self.theme.border))
                    .min_size(Vec2::new(22.0, 14.0));
                if ui.add(fg_btn).clicked() { /* future: open full-screen picker */ }

                ui.label(self.label_desc("BG"));
                let bg_btn = egui::Button::new("  ")
                    .fill(bg_color)
                    .stroke(egui::Stroke::new(1.0, self.theme.border))
                    .min_size(Vec2::new(22.0, 14.0));
                if ui.add(bg_btn).clicked() {}

                if ui.small_button(self.label_muted("X")).clicked() {
                    std::mem::swap(&mut self.color_state.foreground, &mut self.color_state.background);
                }
            });

            ui.add_space(6.0);
            ui.add(egui::Separator::default().horizontal());
            ui.add_space(2.0);

            // ── Picker tabs ───────────────────────────────────────────
            ui.horizontal(|ui| {
                let hsv_selected = self.color_state.active_picker == PickerMode::Hsv;
                let ok_selected  = self.color_state.active_picker == PickerMode::OkLab;
                if ui.selectable_label(hsv_selected, self.label_desc("HSV")).clicked() {
                    self.color_state.active_picker = PickerMode::Hsv;
                }
                if ui.selectable_label(ok_selected, self.label_desc("OKLab")).clicked() {
                    self.color_state.active_picker = PickerMode::OkLab;
                }
            });
            ui.add_space(4.0);

            match self.color_state.active_picker {
                PickerMode::Hsv => {
                    let (mut h, mut s, mut v) = rgba_to_hsv(fg);
                    let mut changed = false;
                    slider_row(ui, self, "H", |ui, _t| {
                        changed |= ui.add(
                            egui::Slider::new(&mut h, 0.0..=360.0).show_value(false)
                        ).changed();
                    });
                    slider_row(ui, self, "S", |ui, _t| {
                        changed |= ui.add(
                            egui::Slider::new(&mut s, 0.0..=1.0).show_value(false)
                        ).changed();
                    });
                    slider_row(ui, self, "V", |ui, _t| {
                        changed |= ui.add(
                            egui::Slider::new(&mut v, 0.0..=1.0).show_value(false)
                        ).changed();
                    });
                    if changed {
                        self.color_state.foreground = hsv_to_rgba(h, s, v, fg[3]);
                    }
                }
                PickerMode::OkLab => {
                    let (mut l, mut a, mut b) = rgba_to_oklab(fg);
                    let mut changed = false;
                    slider_row(ui, self, "L", |ui, _t| {
                        changed |= ui.add(
                            egui::Slider::new(&mut l, 0.0..=1.0).show_value(false)
                        ).changed();
                    });
                    slider_row(ui, self, "a", |ui, _t| {
                        changed |= ui.add(
                            egui::Slider::new(&mut a, -0.5..=0.5).show_value(false)
                        ).changed();
                    });
                    slider_row(ui, self, "b", |ui, _t| {
                        changed |= ui.add(
                            egui::Slider::new(&mut b, -0.5..=0.5).show_value(false)
                        ).changed();
                    });
                    if changed {
                        self.color_state.foreground = oklab_to_rgba(l, a, b, fg[3]);
                    }
                }
            }

            ui.add_space(6.0);
            ui.add(egui::Separator::default().horizontal());
            ui.add_space(2.0);

            // ── Palette ───────────────────────────────────────────────
            ui.label(self.label_desc("Palette"));
            ui.add_space(4.0);

            let palette = self.project.palette.clone();
            let cols = 8usize;
            egui::Grid::new("palette")
                .num_columns(cols)
                .spacing(Vec2::new(2.0, 2.0))
                .show(ui, |ui| {
                for (i, &swatch) in palette.iter().enumerate() {
                    if i > 0 && i % cols == 0 { ui.end_row(); }
                    let color = Color32::from_rgba_unmultiplied(
                        swatch[0], swatch[1], swatch[2], swatch[3],
                    );
                    let is_fg = swatch == self.color_state.foreground;
                    let stroke = if is_fg {
                        egui::Stroke::new(1.5, self.theme.fg)
                    } else {
                        egui::Stroke::new(1.0, self.theme.border)
                    };
                    let btn = egui::Button::new("")
                        .fill(color)
                        .stroke(stroke)
                        .min_size(Vec2::new(14.0, 14.0));
                    if ui.add(btn).clicked() {
                        self.color_state.foreground = swatch;
                    }
                }
            });
        });

        // ── New Project dialog ────────────────────────────────────────────
        if self.show_new_dialog {
            egui::Window::new("New Project")
                .collapsible(false)
                .resizable(false)
                .frame(Frame::window(&ctx.style()))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                egui::Grid::new("new_proj_grid")
                    .num_columns(2)
                    .spacing(Vec2::new(8.0, 6.0))
                    .show(ui, |ui| {
                    ui.label(self.label_desc("Name"));
                    ui.text_edit_singleline(&mut self.new_name);
                    ui.end_row();
                    ui.label(self.label_desc("Width"));
                    ui.add(egui::DragValue::new(&mut self.new_width).range(1..=2048).suffix("px"));
                    ui.end_row();
                    ui.label(self.label_desc("Height"));
                    ui.add(egui::DragValue::new(&mut self.new_height).range(1..=2048).suffix("px"));
                    ui.end_row();
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button(self.label("Create")).clicked() {
                        self.project = Project::new(
                            self.new_width, self.new_height, self.new_name.clone(),
                        );
                        self.canvas_dirty = true;
                        self.show_new_dialog = false;
                    }
                    if ui.button(self.label_muted("Cancel")).clicked() {
                        self.show_new_dialog = false;
                    }
                });
            });
        }

        // ── Central canvas area ───────────────────────────────────────────
        CentralPanel::default()
            .frame(Frame::new().fill(self.theme.bg))
            .show(ctx, |ui| {
            if self.canvas_dirty {
                self.rebuild_canvas_texture(ctx);
            }
            let canvas_rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(canvas_rect);
            self.canvas.draw(
                &painter, canvas_rect,
                self.project.canvas_width, self.project.canvas_height,
                &self.theme,
            );
            self.canvas.handle_input(ui);

            // Draw a thin border around the canvas art area
            let w = self.project.canvas_width as f32 * self.canvas.zoom;
            let h = self.project.canvas_height as f32 * self.canvas.zoom;
            let canvas_art_rect = egui::Rect::from_min_size(
                canvas_rect.min + self.canvas.offset,
                Vec2::new(w, h),
            );
            painter.rect_stroke(
                canvas_art_rect,
                0.0,
                egui::Stroke::new(1.0, self.theme.border),
                egui::StrokeKind::Outside,
            );

            // Handle drawing input
            let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some((px, py)) = self.canvas.screen_to_canvas(pos, canvas_rect) {
                    if px < self.project.canvas_width && py < self.project.canvas_height {
                        let color = self.color_state.foreground;
                        let ai = self.project.active_animation;
                        let fi = self.project.active_frame;
                        let li = self.project.active_layer;

                        if response.drag_started() {
                            self.drag_start = Some((px, py));
                            self.stroke_edits.clear();
                        }

                        match &self.active_tool.clone() {
                            ActiveTool::Pencil => {
                                let edits = apply_pencil(
                                    &self.project.animations[ai].frames[fi].layers[li],
                                    px, py, color,
                                );
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                self.stroke_edits.extend(edits);
                                self.canvas_dirty = true;
                            }
                            ActiveTool::Eraser => {
                                let edits = apply_eraser(
                                    &self.project.animations[ai].frames[fi].layers[li],
                                    px, py,
                                );
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                self.stroke_edits.extend(edits);
                                self.canvas_dirty = true;
                            }
                            ActiveTool::Fill => {
                                let layer = &self.project.animations[ai].frames[fi].layers[li];
                                let target = layer.get_pixel(px, py);
                                let edits = apply_fill(layer, px, py, target, color);
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                if !edits.is_empty() {
                                    self.undo_stack.push(Command::PaintPixels {
                                        animation_id: ai, frame_id: fi, layer_id: li, edits,
                                    });
                                    self.canvas_dirty = true;
                                }
                            }
                            ActiveTool::Eyedropper => {
                                let layer = &self.project.animations[ai].frames[fi].layers[li];
                                self.color_state.foreground = apply_eyedropper(layer, px, py);
                            }
                            _ => {}
                        }

                        if response.drag_stopped() {
                            if !self.stroke_edits.is_empty() {
                                let edits = std::mem::take(&mut self.stroke_edits);
                                self.undo_stack.push(Command::PaintPixels {
                                    animation_id: ai, frame_id: fi, layer_id: li, edits,
                                });
                            }
                            self.drag_start = None;
                        }
                    }
                }
            }
        });

        if self.playback.is_playing { ctx.request_repaint(); }
    }
}

// ── Helper: tool button ───────────────────────────────────────────────────────
fn tool_btn(
    ui: &mut egui::Ui,
    active_tool: &mut ActiveTool,
    theme: &Theme,
    tool: ActiveTool,
    label: &str,
) {
    let selected = *active_tool == tool;
    let text = RichText::new(label)
        .font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace))
        .color(if selected { theme.fg } else { theme.fg_desc });
    let btn = egui::SelectableLabel::new(selected, text);
    if ui.add_sized(Vec2::new(26.0, 20.0), btn).clicked() {
        *active_tool = tool;
    }
}

// ── Helper: labeled slider row ────────────────────────────────────────────────
fn slider_row<F: FnMut(&mut egui::Ui, &Theme)>(
    ui: &mut egui::Ui,
    app: &App,
    label: &str,
    mut content: F,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace))
                .color(app.theme.fg_muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            content(ui, &app.theme);
        });
    });
}

// ── Native file dialogs ───────────────────────────────────────────────────────
fn rfd_open() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Squarez Project", &["sqr"])
        .pick_file()
}

fn rfd_save() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Squarez Project", &["sqr"])
        .save_file()
}
