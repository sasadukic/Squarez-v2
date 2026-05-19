// src/app.rs
use egui::{CentralPanel, FontId, FontFamily, RichText, SidePanel, TopBottomPanel, Vec2};
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
    pub thumbnails: Vec<Vec<FrameThumbnail>>,  // [animation][frame]
    pub current_path: Option<std::path::PathBuf>,
    // Tool drag state
    drag_start: Option<(u32, u32)>,
    stroke_edits: Vec<crate::tools::PixelEdit>,
    canvas_dirty: bool,
    // Composite cache (reserved for future tile-based caching)
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

    fn label_md(&self, text: &str) -> RichText {
        RichText::new(text).font(FontId::new(FONT_SIZE_MD, FontFamily::Monospace)).color(self.theme.fg)
    }

    fn label_sm(&self, text: &str) -> RichText {
        RichText::new(text).font(FontId::new(FONT_SIZE_SM, FontFamily::Monospace)).color(self.theme.fg)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);

        // Advance animation playback
        let fps = self.project.active_anim().fps;
        let total = self.project.active_anim().frames.len();
        let af = &mut self.project.active_frame;
        if self.playback.tick(fps, af, total) {
            self.canvas_dirty = true;
        }

        // Keyboard shortcuts
        let ctrl_z = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl);
        let ctrl_y = ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl);
        if ctrl_z { self.undo_stack.undo(&mut self.project); self.canvas_dirty = true; }
        if ctrl_y { self.undo_stack.redo(&mut self.project); self.canvas_dirty = true; }

        // Menu bar
        TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(self.label_sm("File"), |ui| {
                    if ui.button(self.label_sm("New")).clicked() {
                        self.show_new_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Open")).clicked() {
                        if let Some(path) = rfd_open() {
                            if let Ok(p) = load_sqr(&path) {
                                self.project = p;
                                self.canvas_dirty = true;
                                self.current_path = Some(path);
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Save")).clicked() {
                        let path = self.current_path.clone().unwrap_or_else(|| {
                            std::path::PathBuf::from("untitled.sqr")
                        });
                        let _ = save_sqr(&self.project, &path);
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Save As")).clicked() {
                        if let Some(path) = rfd_save() {
                            let _ = save_sqr(&self.project, &path);
                            self.current_path = Some(path);
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button(self.label_sm("Edit"), |ui| {
                    if ui.button(self.label_sm("Undo  Ctrl+Z")).clicked() {
                        self.undo_stack.undo(&mut self.project);
                        self.canvas_dirty = true;
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Redo  Ctrl+Y")).clicked() {
                        self.undo_stack.redo(&mut self.project);
                        self.canvas_dirty = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button(self.label_sm("Export"), |ui| {
                    if ui.button(self.label_sm("PNG")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Png, scale: 1, animation_index: self.project.active_animation };
                        let _ = export(&self.project, std::path::Path::new("export.png"), opts);
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("GIF")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Gif, scale: 1, animation_index: self.project.active_animation };
                        let _ = export(&self.project, std::path::Path::new("export.gif"), opts);
                        ui.close_menu();
                    }
                    if ui.button(self.label_sm("Spritesheet")).clicked() {
                        let opts = ExportOptions { format: ExportFormat::Spritesheet, scale: 1, animation_index: self.project.active_animation };
                        let _ = export(&self.project, std::path::Path::new("spritesheet.png"), opts);
                        ui.close_menu();
                    }
                });
            });
        });

        // Timeline panel (bottom)
        TopBottomPanel::bottom("timeline").min_height(80.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Clip selector
                let anim_name = self.project.active_anim().name.clone();
                egui::ComboBox::from_id_salt("clip_selector")
                    .selected_text(self.label_sm(&anim_name))
                    .show_ui(ui, |ui| {
                        for (i, anim) in self.project.animations.iter().enumerate() {
                            let name = anim.name.clone();
                            if ui.selectable_label(self.project.active_animation == i, self.label_sm(&name)).clicked() {
                                self.project.active_animation = i;
                                self.project.active_frame = 0;
                                self.canvas_dirty = true;
                            }
                        }
                    });
                if ui.button(self.label_sm("+")).clicked() {
                    let w = self.project.canvas_width;
                    let h = self.project.canvas_height;
                    let n = self.project.animations.len() + 1;
                    self.project.animations.push(crate::project::Animation::new(format!("Animation {}", n), w, h));
                }
                ui.separator();
                // Frame strip
                let num_frames = self.project.active_anim().frames.len();
                for i in 0..num_frames {
                    let selected = self.project.active_frame == i;
                    let label = self.label_sm(&format!("F{}", i + 1));
                    if ui.selectable_label(selected, label).clicked() {
                        self.project.active_frame = i;
                        self.canvas_dirty = true;
                    }
                }
                if ui.button(self.label_sm("+")).clicked() {
                    let w = self.project.canvas_width;
                    let h = self.project.canvas_height;
                    let idx = self.project.active_anim().frames.len();
                    self.undo_stack.push(Command::AddFrame { animation_id: self.project.active_animation, index: idx });
                    self.project.active_anim_mut().frames.push(crate::project::Frame::new(w, h));
                    if self.thumbnails.len() > self.project.active_animation {
                        self.thumbnails[self.project.active_animation].push(FrameThumbnail::default());
                    }
                }
                ui.separator();
                // Playback controls
                if ui.button(self.label_sm("|<")).clicked() { self.project.active_frame = 0; self.canvas_dirty = true; }
                if ui.button(self.label_sm("<")).clicked()  { if self.project.active_frame > 0 { self.project.active_frame -= 1; } self.canvas_dirty = true; }
                let play_label = if self.playback.is_playing { "||" } else { ">" };
                if ui.button(self.label_sm(play_label)).clicked() { self.playback.is_playing = !self.playback.is_playing; }
                let t = self.project.active_anim().frames.len();
                if ui.button(self.label_sm(">")).clicked() { self.project.active_frame = (self.project.active_frame + 1) % t; self.canvas_dirty = true; }
                ui.separator();
                ui.label(self.label_sm("FPS:"));
                let fps = &mut self.project.active_anim_mut().fps;
                let mut fps_val = *fps as u32;
                if ui.add(egui::DragValue::new(&mut fps_val).range(1..=60)).changed() { *fps = fps_val as u8; }
            });
        });

        // Left toolbar
        SidePanel::left("toolbar").exact_width(28.0).show(ctx, |ui| {
            ui.vertical(|ui| {
                let tools: &[(ActiveTool, &str)] = &[
                    (ActiveTool::Pencil,                  "P"),
                    (ActiveTool::Eraser,                  "E"),
                    (ActiveTool::Fill,                    "G"),
                    (ActiveTool::Eyedropper,              "I"),
                    (ActiveTool::Rectangle { filled: false }, "R"),
                    (ActiveTool::Ellipse   { filled: false }, "O"),
                    (ActiveTool::Line,                    "L"),
                    (ActiveTool::RectSelect,              "S"),
                    (ActiveTool::Move,                    "M"),
                ];
                for (tool, label) in tools {
                    let selected = self.active_tool == *tool;
                    if ui.selectable_label(selected, self.label_sm(label)).clicked() {
                        self.active_tool = tool.clone();
                    }
                }
            });
        });

        // Right color panel
        SidePanel::right("color_panel").min_width(180.0).show(ctx, |ui| {
            ui.label(self.label_md("Color"));
            ui.separator();
            // Picker mode tabs
            ui.horizontal(|ui| {
                if ui.selectable_label(self.color_state.active_picker == PickerMode::Hsv, self.label_sm("HSV")).clicked() {
                    self.color_state.active_picker = PickerMode::Hsv;
                }
                if ui.selectable_label(self.color_state.active_picker == PickerMode::OkLab, self.label_sm("OKLab")).clicked() {
                    self.color_state.active_picker = PickerMode::OkLab;
                }
            });
            let fg = self.color_state.foreground;
            match self.color_state.active_picker {
                PickerMode::Hsv => {
                    let (mut h, mut s, mut v) = rgba_to_hsv(fg);
                    let mut changed = false;
                    ui.label(self.label_sm("H")); changed |= ui.add(egui::Slider::new(&mut h, 0.0..=360.0)).changed();
                    ui.label(self.label_sm("S")); changed |= ui.add(egui::Slider::new(&mut s, 0.0..=1.0)).changed();
                    ui.label(self.label_sm("V")); changed |= ui.add(egui::Slider::new(&mut v, 0.0..=1.0)).changed();
                    if changed {
                        self.color_state.foreground = hsv_to_rgba(h, s, v, fg[3]);
                    }
                }
                PickerMode::OkLab => {
                    let (mut l, mut a, mut b) = rgba_to_oklab(fg);
                    let mut changed = false;
                    ui.label(self.label_sm("L")); changed |= ui.add(egui::Slider::new(&mut l, 0.0..=1.0)).changed();
                    ui.label(self.label_sm("a")); changed |= ui.add(egui::Slider::new(&mut a, -0.5..=0.5)).changed();
                    ui.label(self.label_sm("b")); changed |= ui.add(egui::Slider::new(&mut b, -0.5..=0.5)).changed();
                    if changed {
                        self.color_state.foreground = oklab_to_rgba(l, a, b, fg[3]);
                    }
                }
            }
            ui.separator();
            // FG/BG swatches
            ui.horizontal(|ui| {
                let fg_color = egui::Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], fg[3]);
                let bg = self.color_state.background;
                let bg_color = egui::Color32::from_rgba_unmultiplied(bg[0], bg[1], bg[2], bg[3]);
                ui.label(self.label_sm("FG"));
                ui.colored_label(fg_color, self.label_sm("  "));
                ui.label(self.label_sm("BG"));
                ui.colored_label(bg_color, self.label_sm("  "));
                if ui.button(self.label_sm("X")).clicked() {
                    std::mem::swap(&mut self.color_state.foreground, &mut self.color_state.background);
                }
            });
            ui.separator();
            // Palette grid
            ui.label(self.label_sm("Palette"));
            let palette = self.project.palette.clone();
            let cols = 8;
            egui::Grid::new("palette").num_columns(cols).show(ui, |ui| {
                for (i, &swatch) in palette.iter().enumerate() {
                    if i > 0 && i % cols == 0 { ui.end_row(); }
                    let color = egui::Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
                    let btn = egui::Button::new("  ").fill(color).min_size(Vec2::new(16.0, 16.0));
                    if ui.add(btn).clicked() {
                        self.color_state.foreground = swatch;
                    }
                }
            });
        });

        // New project dialog
        if self.show_new_dialog {
            egui::Window::new("New Project")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(self.label_sm("Name:"));
                    ui.text_edit_singleline(&mut self.new_name);
                    ui.label(self.label_sm("Width:"));
                    ui.add(egui::DragValue::new(&mut self.new_width).range(1..=2048));
                    ui.label(self.label_sm("Height:"));
                    ui.add(egui::DragValue::new(&mut self.new_height).range(1..=2048));
                    ui.horizontal(|ui| {
                        if ui.button(self.label_sm("Create")).clicked() {
                            self.project = Project::new(self.new_width, self.new_height, self.new_name.clone());
                            self.canvas_dirty = true;
                            self.show_new_dialog = false;
                        }
                        if ui.button(self.label_sm("Cancel")).clicked() {
                            self.show_new_dialog = false;
                        }
                    });
                });
        }

        // Main canvas
        CentralPanel::default().show(ctx, |ui| {
            if self.canvas_dirty {
                self.rebuild_canvas_texture(ctx);
            }
            let canvas_rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(canvas_rect);
            self.canvas.draw(&painter, canvas_rect, self.project.canvas_width, self.project.canvas_height, &self.theme);
            self.canvas.handle_input(ui);

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
                                let edits = apply_pencil(&self.project.animations[ai].frames[fi].layers[li], px, py, color);
                                for &(x, y, _old, new) in &edits {
                                    self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                                }
                                self.stroke_edits.extend(edits);
                                self.canvas_dirty = true;
                            }
                            ActiveTool::Eraser => {
                                let edits = apply_eraser(&self.project.animations[ai].frames[fi].layers[li], px, py);
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
                                    self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
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
                                self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
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
