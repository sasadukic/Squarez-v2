// src/app.rs
use egui::{CentralPanel, Color32, FontFamily, FontId, Frame, Image, ImageSource, Margin, Pos2, RichText, SidePanel, TopBottomPanel, Vec2};

use crate::animation::{FrameThumbnail, PlaybackState};
use crate::canvas::CanvasState;
use crate::color::hsv::{hsv_to_rgba, rgba_to_hsv};
use crate::color::oklab::{oklch_to_rgba, rgba_to_oklch};
use crate::color::{ColorState, PickerMode};
use crate::history::{Command, UndoStack};
use crate::io::sqr::{load_sqr, save_sqr};
use crate::layers::composite_frame;
use crate::project::{Animation, Frame as ProjectFrame, Layer, Project, Rgba};
use crate::theme::{load_fonts, Theme, FONT_SIZE_SM};
use crate::top_bar::{
    menu_zone_width, BRAND_WIDTH, DROPDOWN_CORNER_RADIUS, DROPDOWN_ROW_HEIGHT, DROPDOWN_TOP_GAP,
    DROPDOWN_WIDTH, MENU_FONT_SIZE, TOP_BAR_HEIGHT,
};
use crate::tools::{apply_eraser, apply_eyedropper, apply_ellipse, apply_fill, apply_line, apply_pencil, apply_rect, bresenham_positions, ActiveTool, SelectState, SelectInteraction, Handle, FloatBuffer, DragAnchor, sample_transformed};
use crate::ui_metrics::RIGHT_SECTION_STACK_GAP;
use crate::ui_state::{Panel, UiState};

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
    pub ui_state: UiState,
    drag_start: Option<(u32, u32)>,
    stroke_edits: Vec<crate::tools::PixelEdit>,
    last_pencil_pos: Option<(u32, u32)>,
    stroke_painted: std::collections::HashSet<(u32, u32)>,
    stroke_pixel_sequence: Vec<(u32, u32)>,
    canvas_dirty: bool,
    show_new_dialog: bool,
    new_width: u32,
    new_height: u32,
    new_name: String,
    frame_menu: Option<(usize, Pos2, f64)>,  // (frame_idx, screen_pos, opened_at_time)
    layer_ctx_menu: Option<(usize, Pos2, f64)>,  // (layer_idx, screen_pos, opened_at_time)
    top_menu_open: Option<(TopMenu, Pos2)>,
    toolbar_anim_y: f32,
    toolbar_anim_vel: f32,
    // Currently displayed tool for each grouped slot (persisted)
    pen_group_current: ActiveTool,    // Pencil or Eraser
    bucket_group_current: ActiveTool, // Fill or Eyedropper
    shape_group_current: ActiveTool,  // Rectangle | Ellipse | Line
    select_group_current: ActiveTool, // RectSelect or Move
    // Which group submenu is open (slot index 0=pen, 1=bucket, 2=shape, 3=select), if any
    open_tool_submenu: Option<usize>,
    pen_slot_rect: Option<egui::Rect>,
    bucket_slot_rect: Option<egui::Rect>,
    shape_slot_rect: Option<egui::Rect>,
    select_slot_rect: Option<egui::Rect>,
    alt_was_down: bool,
    top_menu_hover_left: Option<f64>,
    top_menu_opened_at: f64,
    // Spring-animated sliding highlight in the top menu bar
    menu_anim_x: f32,
    menu_anim_vel: f32,
    menu_anim_initialized: bool,
    // Spring-animated sliding highlight for color picker mode tabs (OKL/HSV/RGB)
    picker_tab_anim_x: f32,
    picker_tab_anim_vel: f32,
    picker_tab_initialized: bool,
    // Tab-switch tween: fixed start values for display fields
    picker_tweening: bool,
    picker_tween_start: f64,
    picker_tween_from_okl_l: f32,
    picker_tween_from_okl_c: f32,
    picker_tween_from_okl_h: f32,
    picker_tween_from_hsv_h: f32,
    picker_tween_from_hsv_s: f32,
    picker_tween_from_hsv_v: f32,
    picker_tween_from_rgb_r: f32,
    picker_tween_from_rgb_g: f32,
    picker_tween_from_rgb_b: f32,
    // Spring-animated clip height for dropdown open bounce
    dropdown_clip_h: f32,
    dropdown_clip_vel: f32,
    dropdown_full_h: f32,
    // Inline rename state for layers: (layer_index, current_edit_string)
    renaming_layer: Option<(usize, String)>,
    // Inline rename state for animations: (anim_index, current_edit_string)
    renaming_animation: Option<(usize, String)>,
    // Palette drag-and-drop reorder
    palette_drag_idx: Option<usize>,
    // Spring-animated selection highlight for layers panel
    layer_sel_y: f32,
    layer_sel_vel: f32,
    // Spring-animated selection highlight for animations panel
    anim_sel_y: f32,
    anim_sel_vel: f32,
    // Layer drag-to-group state: (dragging layer index, current hover layer index)
    layer_drag: Option<usize>,
    layer_drag_over: Option<usize>,
    // Manual double-click detection for zoom tool
    last_zoom_click_time: f64,
    // Double-click on zoom tool button → fit canvas on next canvas render
    last_zoom_tool_btn_click: f64,
    pending_zoom_fit: bool,
    // Manual double-click detection for layer rename: (layer_idx, click_time)
    last_layer_click: Option<(usize, f64)>,
    // Real-time preview pixels for shape tools (overlaid during drag, cleared on commit)
    shape_preview: Vec<(u32, u32, Rgba)>,
    // Selection tool state: current rect (x0, y0, x1, y1) in canvas pixel coords
    select_state: SelectState,
    // Accumulated scroll delta for timeline frame navigation (slows down scroll speed)
    timeline_scroll_accum: f32,
    // View > Show sub-menu open state
    view_show_open: bool,
    // Screen-space right-top of the "Show" row, used to position side submenu
    view_show_pos: Option<egui::Pos2>,
    // Sidebar section order (drag-to-reorder)
    sidebar_order: Vec<Panel>,
    // Drag-to-reorder state (only active in narrow/all-collapsed mode with Cmd held)
    sidebar_drag: Option<Panel>,
    sidebar_drag_over_idx: Option<usize>,
    // Long-press timer: (panel under pointer, time of initial press)
    sidebar_press_start: Option<(Panel, f64)>,
    // Icon row rects recorded each frame for hit-testing (screen space)
    sidebar_icon_rects: Vec<(Panel, egui::Rect)>,
    /// Sprite sheet for the animated logo (16 frames horizontal, 16×16 each). Loaded on first draw.
    logo_sprite: Option<egui::TextureHandle>,
    /// When `Some(start_time)`, the logo plays its animation once. Cleared after last frame.
    logo_anim_start: Option<f64>,
    /// Number of frames in the logo sprite sheet (horizontal frames).
    logo_frames: usize,
    /// If Some(idx), show that frame index statically (used when opening the menu).
    logo_anim_static_frame: Option<usize>,
    /// Optional play range (inclusive start, exclusive end) used to play subranges of the sprite.
    logo_anim_play_range: Option<(usize, usize)>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopMenu {
    File,
    View,
    Layer,
    Animation,
    Windows,
}

impl TopMenu {
    fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::View => "View",
            Self::Layer => "Layer",
            Self::Animation => "Animation",
            Self::Windows => "Windows",
        }
    }

    /// Pixel width of this menu's hit zone in the top bar.
    fn zone_width(self) -> f32 {
        match self {
            Self::File => 38.0, // icon button, no text
            _ => menu_zone_width(self.label()),
        }
    }
}

const LAYOUT_STORAGE_KEY: &str = "squarez_layout_v1";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct LayoutState {
    ui_state: UiState,
    sidebar_order: Vec<Panel>,
    color_state: Option<ColorState>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            ui_state: UiState::default(),
            sidebar_order: Vec::new(),
            color_state: None,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        // 1.5× zoom on top of OS DPI scaling for 4K displays.
        cc.egui_ctx.set_zoom_factor(1.5);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // Load persisted layout (panel visibility, collapse, order) if available
        let layout: Option<LayoutState> = cc.storage
            .and_then(|s| s.get_string(LAYOUT_STORAGE_KEY))
            .and_then(|json| serde_json::from_str(&json).ok());
        load_fonts(&cc.egui_ctx);
        let project = Project::new(16, 16, "Untitled".to_string());
        let thumbnails = project
            .animations
            .iter()
            .map(|a| a.frames.iter().map(|_| FrameThumbnail::default()).collect())
            .collect();
        // Seed foreground from first palette color so all picker modes start in sync
        let mut color_state = layout.as_ref()
            .and_then(|l| l.color_state.clone())
            .unwrap_or_default();
        // If no persisted color_state had a foreground, seed it from the palette.
        if layout.as_ref().and_then(|l| l.color_state.as_ref()).is_none() {
            if let Some(first) = project.palette.first() {
                color_state.foreground = *first;
                sync_color_caches(&mut color_state);
            }
        }
        Self {
            project,
            theme: Theme::default(),
            canvas: CanvasState::default(),
            color_state,
            active_tool: ActiveTool::Pencil,
            undo_stack: UndoStack::new(),
            playback: PlaybackState::default(),
            thumbnails,
            current_path: None,
            ui_state: layout.as_ref().map(|l| l.ui_state.clone()).unwrap_or_default(),
            drag_start: None,
            stroke_edits: Vec::new(),
            last_pencil_pos: None,
            stroke_painted: std::collections::HashSet::new(),
            stroke_pixel_sequence: Vec::new(),
            canvas_dirty: true,
            show_new_dialog: false,
            new_width: 16,
            new_height: 16,
            new_name: "Untitled".to_string(),
            frame_menu: None,
            layer_ctx_menu: None,
            top_menu_open: None,
            toolbar_anim_y: 0.0,
            toolbar_anim_vel: 0.0,
            pen_group_current: ActiveTool::Pencil,
            bucket_group_current: ActiveTool::Fill,
            shape_group_current: ActiveTool::Rectangle { filled: false },
            select_group_current: ActiveTool::RectSelect,
            open_tool_submenu: None,
            pen_slot_rect: None,
            bucket_slot_rect: None,
            shape_slot_rect: None,
            select_slot_rect: None,
            alt_was_down: false,
            top_menu_hover_left: None,
            top_menu_opened_at: 0.0,
            menu_anim_x: 0.0,
            menu_anim_vel: 0.0,
            menu_anim_initialized: false,
            picker_tab_anim_x: 0.0,
            picker_tab_anim_vel: 0.0,
            picker_tab_initialized: false,
            picker_tweening: false,
            picker_tween_start: 0.0,
            picker_tween_from_okl_l: 0.7,
            picker_tween_from_okl_c: 0.15,
            picker_tween_from_okl_h: 210.0,
            picker_tween_from_hsv_h: 187.0,
            picker_tween_from_hsv_s: 0.85,
            picker_tween_from_hsv_v: 0.76,
            picker_tween_from_rgb_r: 17.0,
            picker_tween_from_rgb_g: 173.0,
            picker_tween_from_rgb_b: 193.0,
            dropdown_clip_h: 0.0,
            dropdown_clip_vel: 0.0,
            dropdown_full_h: 0.0,
            renaming_layer: None,
            renaming_animation: None,
            palette_drag_idx: None,
            layer_sel_y: 0.0,
            layer_sel_vel: 0.0,
            anim_sel_y: 0.0,
            anim_sel_vel: 0.0,
            layer_drag: None,
            layer_drag_over: None,
            last_zoom_click_time: -1.0,
            last_zoom_tool_btn_click: -1.0,
            pending_zoom_fit: false,
            last_layer_click: None,
            shape_preview: Vec::new(),
            select_state: SelectState::default(),
            timeline_scroll_accum: 0.0,
            view_show_open: false,
            view_show_pos: None,
            sidebar_order: layout.map(|l| l.sidebar_order).unwrap_or_else(|| vec![Panel::Palette, Panel::Color, Panel::Layers, Panel::Animations, Panel::Preview]),
            sidebar_drag: None,
            sidebar_drag_over_idx: None,
            sidebar_press_start: None,
            sidebar_icon_rects: Vec::new(),
            logo_sprite: None,
            logo_anim_start: None,
            logo_frames: 16,
            logo_anim_static_frame: None,
            logo_anim_play_range: None,
        }
    }

    fn composite_active_frame(&mut self) -> Vec<u8> {
        composite_frame(
            self.project.active_frame_ref(),
            self.project.canvas_width,
            self.project.canvas_height,
        )
    }

    /// Creates a new animation whose first frame has the same layer structure
    /// (names, visibility, lock state) as the current animation's active frame,
    /// but with blank pixel data. New animations always inherit the layer count
    /// so layers are never "lost" when switching between animations.
    fn new_animation_from_layers(&self, name: String) -> Animation {
        let w = self.project.canvas_width;
        let h = self.project.canvas_height;
        let layers: Vec<Layer> = self.project.active_frame_ref().layers.iter().map(|l| {
            let mut new = Layer::new_with_id(l.name.clone(), w, h, l.id);
            new.visible = l.visible;
            new.locked  = l.locked;
            new
        }).collect();
        let frame = ProjectFrame { duration_ms: 0, layers, dirty: true };
        Animation { name, fps: 12, frames: vec![frame] }
    }

    fn active_tool_index(&self) -> usize {
        match self.active_tool {
            ActiveTool::Pencil | ActiveTool::Eraser           => 0, // pen group
            ActiveTool::Fill   | ActiveTool::Eyedropper       => 1, // bucket group
            ActiveTool::Rectangle { .. }
            | ActiveTool::Ellipse { .. }
            | ActiveTool::Line                                => 2, // shape group
            ActiveTool::RectSelect | ActiveTool::Move         => 3, // select group
            ActiveTool::Zoom                                  => 4,
        }
    }

    fn is_group_selected(&self, slot: usize) -> bool {
        match slot {
            0 => matches!(self.active_tool, ActiveTool::Pencil | ActiveTool::Eraser),
            1 => matches!(self.active_tool, ActiveTool::Fill | ActiveTool::Eyedropper),
            2 => matches!(self.active_tool, ActiveTool::Rectangle { .. } | ActiveTool::Ellipse { .. } | ActiveTool::Line),
            3 => matches!(self.active_tool, ActiveTool::RectSelect | ActiveTool::Move),
            _ => false,
        }
    }

    /// Cycle to the next tool within the current tool's group (Alt-flip).
    fn cycle_tool_in_group(&mut self) {
        let new_tool = match &self.active_tool {
            ActiveTool::Pencil => ActiveTool::Eraser,
            ActiveTool::Eraser => ActiveTool::Pencil,
            ActiveTool::Fill => ActiveTool::Eyedropper,
            ActiveTool::Eyedropper => ActiveTool::Fill,
            ActiveTool::Rectangle { .. } => ActiveTool::Ellipse { filled: false },
            ActiveTool::Ellipse { .. } => ActiveTool::Line,
            ActiveTool::Line => ActiveTool::Rectangle { filled: false },
            ActiveTool::RectSelect => ActiveTool::RectSelect,
            ActiveTool::Move => ActiveTool::RectSelect,
            _ => return,
        };
        self.set_active_tool(new_tool);
    }

    /// Set active tool and sync the group's "current" display.
    fn set_active_tool(&mut self, t: ActiveTool) {
        match &t {
            ActiveTool::Pencil | ActiveTool::Eraser => self.pen_group_current = t.clone(),
            ActiveTool::Fill | ActiveTool::Eyedropper => self.bucket_group_current = t.clone(),
            ActiveTool::Rectangle { .. } | ActiveTool::Ellipse { .. } | ActiveTool::Line => {
                self.shape_group_current = t.clone();
            }
            ActiveTool::RectSelect | ActiveTool::Move => self.select_group_current = t.clone(),
            _ => {}
        }
        self.active_tool = t;
    }

    fn rebuild_canvas_texture(&mut self, ctx: &egui::Context) {
        let mut pixels = self.composite_active_frame();
        // Overlay real-time shape preview on top of composited frame
        for &(x, y, color) in &self.shape_preview {
            if x < self.project.canvas_width && y < self.project.canvas_height {
                let i = (y * self.project.canvas_width + x) as usize * 4;
                pixels[i]     = color[0];
                pixels[i + 1] = color[1];
                pixels[i + 2] = color[2];
                pixels[i + 3] = color[3];
            }
        }
        // Overlay floating selection (sampled with nearest-neighbor through the transform)
        if self.select_state.has_float() {
            if let Some((ax, ay, aw, ah)) = self.select_state.transformed_aabb() {
                let w = self.project.canvas_width as i32;
                let h = self.project.canvas_height as i32;
                let x0 = (ax.floor() as i32).max(0);
                let y0 = (ay.floor() as i32).max(0);
                let x1 = ((ax + aw).ceil() as i32).min(w);
                let y1 = ((ay + ah).ceil() as i32).min(h);
                for cy in y0..y1 {
                    for cx in x0..x1 {
                        if let Some(sample) = sample_transformed(&self.select_state, cx, cy) {
                            let i = (cy as u32 * self.project.canvas_width + cx as u32) as usize * 4;
                            // Alpha-over: draw sampled pixel over existing
                            let sa = sample[3] as f32 / 255.0;
                            let inv = 1.0 - sa;
                            pixels[i]     = (sample[0] as f32 * sa + pixels[i]     as f32 * inv) as u8;
                            pixels[i + 1] = (sample[1] as f32 * sa + pixels[i + 1] as f32 * inv) as u8;
                            pixels[i + 2] = (sample[2] as f32 * sa + pixels[i + 2] as f32 * inv) as u8;
                            pixels[i + 3] = ((sample[3] as f32 + pixels[i + 3] as f32 * inv).min(255.0)) as u8;
                        }
                    }
                }
            }
        }
        self.canvas.upload_texture(
            ctx,
            &pixels,
            self.project.canvas_width,
            self.project.canvas_height,
        );
        self.canvas_dirty = false;
    }

    fn label(&self, text: &str) -> RichText {
        rich(text, self.theme.fg, FONT_SIZE_SM)
    }

    fn label_desc(&self, text: &str) -> RichText {
        rich(text, self.theme.fg_desc, FONT_SIZE_SM)
    }

    fn label_muted(&self, text: &str) -> RichText {
        rich(text, self.theme.fg_muted, FONT_SIZE_SM)
    }

    fn panel_frame(&self) -> Frame {
        Frame::new().fill(self.theme.panel).inner_margin(Margin::same(0))
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        let bar_rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(BRAND_WIDTH, TOP_BAR_HEIGHT));

        // Full-width click zone for dropdown toggle (manual hit test, no layout)
        let was_clicked = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.hover_pos().is_some_and(|p| bar_rect.contains(p))
        });
        if was_clicked {
            let now = ctx.input(|i| i.time);
            let already_open = matches!(self.top_menu_open, Some((TopMenu::File, _)));
            if already_open {
                self.close_top_menu_with_animation(now);
            } else {
                self.logo_anim_static_frame = None;
                self.logo_anim_play_range = Some((1, 8));
                self.logo_anim_start = Some(now);
                let pos = Pos2::new(0.0, TOP_BAR_HEIGHT + DROPDOWN_TOP_GAP);
                self.top_menu_open = Some((TopMenu::File, pos));
                self.top_menu_opened_at = now;
                self.top_menu_hover_left = None;
                self.view_show_open = false;
                self.dropdown_clip_h   = 0.0;
                self.dropdown_clip_vel = 0.0;
                self.dropdown_full_h   = 0.0;
            }
        }

        egui::Area::new("top_bar".into())
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let theme = self.theme.clone();

                // Background fill
                ui.painter().rect_filled(bar_rect, 0.0, theme.panel);

                // Draw logo on top
                ui.set_min_size(Vec2::new(BRAND_WIDTH, TOP_BAR_HEIGHT));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    self.draw_logo(ui, &theme);
                });
            });
        self.draw_top_menu_dropdown(ctx);
    }

    /// Close the top menu and play the logo closing subrange animation.
    fn close_top_menu_with_animation(&mut self, now: f64) {
        // Play closing frames 7..=12 (exclusive end 13)
        self.logo_anim_play_range = Some((7, 13));
        self.logo_anim_start = Some(now);
        self.logo_anim_static_frame = None;
        self.top_menu_open = None;
        self.top_menu_hover_left = None;
    }

    fn draw_top_menu_dropdown(&mut self, ctx: &egui::Context) {
        let Some((menu, pos)) = self.top_menu_open else { return; };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.top_menu_open = None;
            return;
        }

        // ── Bounce-open spring ────────────────────────────────────────────
        let dt = ctx.input(|i| i.unstable_dt).min(0.05);
        if self.dropdown_full_h > 0.0 {
            // Underdamped spring: stiffness=600, damping=14 → visible overshoot
            let force = (self.dropdown_full_h - self.dropdown_clip_h) * 600.0
                      - self.dropdown_clip_vel * 14.0;
            self.dropdown_clip_vel += force * dt;
            self.dropdown_clip_h   += self.dropdown_clip_vel * dt;
            // Don't let it go negative
            if self.dropdown_clip_h < 0.0 {
                self.dropdown_clip_h   = 0.0;
                self.dropdown_clip_vel = self.dropdown_clip_vel.abs();
            }
            // Once spring first reaches full height, snap to settled immediately.
            // This preserves the fast upward spring motion but prevents the downward
            // oscillation that clips the last row on and off (flicker).
            if self.dropdown_clip_h >= self.dropdown_full_h {
                self.dropdown_clip_h   = self.dropdown_full_h;
                self.dropdown_clip_vel = 0.0;
            } else {
                ctx.request_repaint();
            }
        }

        let theme = self.theme.clone();
        let mut close_menu = false;
        let mut measured_h = 0.0f32;
        let clip_h = self.dropdown_clip_h;
        let full_h = self.dropdown_full_h;

        let area_response = egui::Area::new(egui::Id::new("top_menu_dropdown"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme.panel)
                    .corner_radius(egui::CornerRadius::same(DROPDOWN_CORNER_RADIUS))
                    .shadow(egui::Shadow {
                        offset: [0, 14],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                    })
                    .inner_margin(Margin::same(0))
                    .show(ui, |ui| {
                        // File menu sizes to its own content; others use fixed DROPDOWN_WIDTH
                        if menu != TopMenu::File {
                            ui.set_width(DROPDOWN_WIDTH);
                        }

                        // Apply clip for open animation.
                        // On frame 1 full_h is 0 → clip to 0 (hides content, but layout still
                        // allocates so measured_h is captured correctly on same frame).
                        {
                            let top = ui.cursor().left_top();
                            let visible_h = if full_h > 0.0 {
                                clip_h.min(full_h + 20.0).max(0.0)
                            } else {
                                0.0
                            };
                            let anim_clip = egui::Rect::from_min_size(
                                top,
                                Vec2::new(DROPDOWN_WIDTH + 60.0, visible_h),
                            );
                            ui.set_clip_rect(ui.clip_rect().intersect(anim_clip));
                        }

                        match menu {
                            TopMenu::File => {
                                // Horizontal icon row: New | Open | Save | Exit
                                // Layout: inner_margin(8) each side + 4×36px buttons + 3×8px gaps = 184px
                                const BTN: f32 = 36.0;
                                Frame::new()
                                    .inner_margin(Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing = Vec2::splat(8.0);
                                        ui.horizontal(|ui| {
                                            // New
                                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                                            if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                                            let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                                            ui.put(egui::Rect::from_center_size(r.center(), Vec2::splat(20.0)),
                                                Image::new(egui::include_image!("../assets/icons/new.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                                            if resp.clicked() { self.show_new_dialog = true; close_menu = true; }

                                            // Open
                                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                                            if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                                            let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                                            ui.put(egui::Rect::from_center_size(r.center(), Vec2::splat(20.0)),
                                                Image::new(egui::include_image!("../assets/icons/open.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                                            if resp.clicked() {
                                                if let Some(path) = rfd_open() {
                                                    if let Ok(p) = load_sqr(&path) {
                                                        self.project = p;
                                                        self.canvas_dirty = true;
                                                        self.current_path = Some(path);
                                                    }
                                                }
                                                close_menu = true;
                                            }

                                            // Save
                                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                                            if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                                            let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                                            ui.put(egui::Rect::from_center_size(r.center(), Vec2::splat(20.0)),
                                                Image::new(egui::include_image!("../assets/icons/save.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                                            if resp.clicked() {
                                                let path = self.current_path.clone().unwrap_or_else(|| std::path::PathBuf::from("untitled.sqr"));
                                                let _ = save_sqr(&self.project, &path);
                                                close_menu = true;
                                            }

                                            // Exit
                                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                                            if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                                            let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                                            ui.put(egui::Rect::from_center_size(r.center(), Vec2::splat(20.0)),
                                                Image::new(egui::include_image!("../assets/icons/exit.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                                            if resp.clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                                        });
                                    });
                            }
                            TopMenu::View => {
                                let _ = dropdown_row(ui, &theme, "Zoom with mouse wheel", None, false);
                                let show_resp = dropdown_row(ui, &theme, "Show ▸", None, true);
                                if show_resp.clicked() {
                                    self.view_show_open = !self.view_show_open;
                                    self.dropdown_full_h = 0.0;
                                }
                                // Store right-top of the Show row so the side submenu can be
                                // positioned next to it on this and subsequent frames.
                                self.view_show_pos = Some(show_resp.rect.right_top());
                            }
                            TopMenu::Layer => {
                                let ai = self.project.active_animation;
                                let fi = self.project.active_frame;
                                if dropdown_row(ui, &theme, "Add layer", None, true).clicked() {
                                    let idx = self.project.animations[ai].frames[fi].layers.len();
                                    let new_id = self.project.next_layer_id();
                                    let name = format!("Layer {}", idx + 1);
                                    let w = self.project.canvas_width;
                                    let h = self.project.canvas_height;
                                    self.undo_stack.push(Command::AddLayer { index: idx, name: name.clone(), id: new_id });
                                    for anim in &mut self.project.animations {
                                        for frame in &mut anim.frames {
                                            frame.layers.push(Layer::new_with_id(name.clone(), w, h, new_id));
                                        }
                                    }
                                    self.project.active_layer = idx;
                                    self.canvas_dirty = true;
                                    close_menu = true;
                                }
                                if dropdown_row(ui, &theme, "Delete active layer", None, true).clicked() {
                                    let layers = &mut self.project.animations[ai].frames[fi].layers;
                                    if layers.len() > 1 {
                                        let idx = self.project.active_layer.min(layers.len() - 1);
                                        let snapshot = layers[idx].clone();
                                        self.undo_stack.push(Command::DeleteLayer { animation_id: ai, frame_id: fi, index: idx, snapshot });
                                        layers.remove(idx);
                                        self.project.active_layer = self.project.active_layer.saturating_sub(1).min(layers.len() - 1);
                                        self.canvas_dirty = true;
                                    }
                                    close_menu = true;
                                }
                            }
                            TopMenu::Animation => {
                                if dropdown_row(ui, &theme, "New animation", None, true).clicked() {
                                    let n = self.project.animations.len() + 1;
                                    let anim = self.new_animation_from_layers(format!("anim_{}", n));
                                    self.project.animations.push(anim);
                                    self.project.active_animation = self.project.animations.len() - 1;
                                    self.project.active_frame = 0;
                                    // active_layer is already valid: new anim has same layer count
                                    self.canvas_dirty = true;
                                    close_menu = true;
                                }
                                if dropdown_row(ui, &theme, "Delete animation", None, true).clicked() {
                                    if self.project.animations.len() > 1 {
                                        self.project.animations.remove(self.project.active_animation);
                                        self.project.active_animation = self.project.active_animation.saturating_sub(1);
                                        self.project.active_frame = 0;
                                        self.canvas_dirty = true;
                                    }
                                    close_menu = true;
                                }
                                if dropdown_row(ui, &theme, "Duplicate frame", None, true).clicked() {
                                    self.duplicate_active_frame();
                                    close_menu = true;
                                }
                                if dropdown_row(ui, &theme, "Delete frame", None, true).clicked() {
                                    self.delete_active_frame();
                                    close_menu = true;
                                }
                                let _ = dropdown_row(ui, &theme, "Onion skin", None, false);
                            }
                            TopMenu::Windows => {
                                if dropdown_row(ui, &theme, "Color", window_check(self.ui_state.is_visible(Panel::Color)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Color);
                                }
                                if dropdown_row(ui, &theme, "Palette", window_check(self.ui_state.is_visible(Panel::Palette)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Palette);
                                }
                                if dropdown_row(ui, &theme, "Preview", window_check(self.ui_state.is_visible(Panel::Preview)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Preview);
                                }
                                if dropdown_row(ui, &theme, "Layers", window_check(self.ui_state.is_visible(Panel::Layers)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Layers);
                                }
                                if dropdown_row(ui, &theme, "Animations", window_check(self.ui_state.is_visible(Panel::Animations)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Animations);
                                }
                                if dropdown_row(ui, &theme, "Timeline", window_check(self.ui_state.is_visible(Panel::Timeline)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Timeline);
                                }
                                if dropdown_row(ui, &theme, "Reset layout", None, true).clicked() {
                                    self.ui_state = UiState::default();
                                    close_menu = true;
                                }
                            }
                        }

                        // Measure the full allocated height (unaffected by clip)
                        measured_h = ui.min_rect().height();
                    });
            });

        // Store full height so the spring has a target on subsequent frames
        if measured_h > 1.0 {
            self.dropdown_full_h = measured_h;
            ctx.request_repaint(); // ensure spring ticks
        }

        // ── Side submenu: View > Show ─────────────────────────────────────
        let mut side_submenu_rect: Option<egui::Rect> = None;
        if !close_menu && matches!(menu, TopMenu::View) && self.view_show_open {
            if let Some(show_top_right) = self.view_show_pos {
                let sub_pos = Pos2::new(show_top_right.x + 2.0, show_top_right.y);
                let sub_resp = egui::Area::new(egui::Id::new("view_show_submenu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(sub_pos)
                    .show(ctx, |ui| {
                        Frame::new()
                            .fill(theme.panel)
                            .corner_radius(egui::CornerRadius::same(DROPDOWN_CORNER_RADIUS))
                            .shadow(egui::Shadow {
                                offset: [0, 14],
                                blur: 36,
                                spread: 0,
                                color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                            })
                            .inner_margin(Margin::same(0))
                            .show(ui, |ui| {
                                ui.set_width(DROPDOWN_WIDTH);
                                if dropdown_row(ui, &theme, "Palette", window_check(self.ui_state.is_visible(Panel::Palette)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Palette);
                                }
                                if dropdown_row(ui, &theme, "Color Mixer", window_check(self.ui_state.is_visible(Panel::Color)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Color);
                                }
                                if dropdown_row(ui, &theme, "Preview", window_check(self.ui_state.is_visible(Panel::Preview)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Preview);
                                }
                                if dropdown_row(ui, &theme, "Layers", window_check(self.ui_state.is_visible(Panel::Layers)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Layers);
                                }
                                if dropdown_row(ui, &theme, "Animations", window_check(self.ui_state.is_visible(Panel::Animations)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Animations);
                                }
                                if dropdown_row(ui, &theme, "Timeline", window_check(self.ui_state.is_visible(Panel::Timeline)), true).clicked() {
                                    self.ui_state.toggle_visible(Panel::Timeline);
                                }
                                if dropdown_row(ui, &theme, "Reset layout", None, true).clicked() {
                                    self.ui_state = UiState::default();
                                    self.view_show_open = false;
                                    close_menu = true;
                                }
                            });
                    });
                side_submenu_rect = Some(sub_resp.response.rect);
            }
        }

                        if close_menu {
            let now = ctx.input(|i| i.time);
            self.close_top_menu_with_animation(now);
            return;
        }

        // Click-away: close if a click happened outside the dropdown rect.
        // Skip on the opening frame (same click that opened the menu).
        let now = ctx.input(|i| i.time);
        let menu_age = now - self.top_menu_opened_at;
        let dropdown_rect = area_response.response.rect;
            if menu_age > 0.15 && ctx.input(|i| i.pointer.any_click()) {
                let outside = ctx.input(|i| i.pointer.interact_pos())
                    .map_or(true, |p| {
                        !dropdown_rect.contains(p)
                        && !side_submenu_rect.map_or(false, |r| r.contains(p))
                    });
                if outside {
                    let now = ctx.input(|i| i.time);
                    self.close_top_menu_with_animation(now);
                    return;
                }
            }

        // Hover timeout: close if mouse has been outside the dropdown for >= 2 s
        let pointer_inside = ctx.input(|i| i.pointer.hover_pos())
            .map_or(false, |p| {
                dropdown_rect.contains(p)
                || side_submenu_rect.map_or(false, |r| r.contains(p))
            });
        if pointer_inside {
            self.top_menu_hover_left = None;
        } else {
            if self.top_menu_hover_left.is_none() {
                self.top_menu_hover_left = Some(now);
            }
            if let Some(t) = self.top_menu_hover_left {
                if now - t >= 2.0 {
                    self.close_top_menu_with_animation(now);
                }
            }
            ctx.request_repaint();
        }
    }

    fn draw_left_toolbar(&mut self, ctx: &egui::Context) {
        // ── Spring-physics animation ──────────────────────────────────────
        let dt = ctx.input(|i| i.unstable_dt).min(0.05);
        let target_y = self.active_tool_index() as f32 * 38.0;

        let force = (target_y - self.toolbar_anim_y) * 300.0
                  - self.toolbar_anim_vel * 22.0;
        self.toolbar_anim_vel += force * dt;
        self.toolbar_anim_y   += self.toolbar_anim_vel * dt;

        // No clamp — spring can overshoot; clip rect in the draw step hides any overflow.
        let settled = (self.toolbar_anim_y - target_y).abs() < 0.3
                   && self.toolbar_anim_vel.abs() < 0.3;
        if settled {
            self.toolbar_anim_y   = target_y;
            self.toolbar_anim_vel = 0.0;
        } else {
            ctx.request_repaint();
        }

        egui::Area::new("toolbar".into())
            .fixed_pos(Pos2::new(0.0, TOP_BAR_HEIGHT))
            .default_width(38.0)
            .show(ctx, |ui| {
                ui.set_max_width(38.0);
                ui.set_width(38.0);
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                // Vertically center the button stack in the full screen
                const TOOL_COUNT: f32 = 5.0;
                let tools_h = TOOL_COUNT * 38.0;
                let top_pad = ((ctx.screen_rect().height() - tools_h) / 2.0 - TOP_BAR_HEIGHT).max(0.0);
                ui.add_space(top_pad);

                // Origin of the button stack (after padding)
                let origin = ui.next_widget_position();

                // Draw panel background behind the tool buttons only
                let tools_h = TOOL_COUNT * 38.0;
                let toolbar_bg = egui::Rect::from_min_size(
                    origin,
                    Vec2::new(38.0, tools_h),
                );
                ui.painter().rect_filled(toolbar_bg, 0.0, self.theme.panel);

                // Draw animated selection highlight, clipped to toolbar_bg so
                // spring overshoot is invisible outside the button area.
                let sel_rect = egui::Rect::from_min_size(
                    Pos2::new(origin.x, origin.y + self.toolbar_anim_y),
                    Vec2::splat(38.0),
                );
                ui.painter().with_clip_rect(toolbar_bg).rect_filled(sel_rect, 0.0, self.theme.accent);

                // ── Grouped tool slots ──────────────────────────────────────
                // Slot 0: Pen group (Pencil/Eraser)
                let pen_icon = tool_icon(&self.pen_group_current);
                let pen_resp = tool_btn_raw(ui, &self.theme, self.is_group_selected(0), pen_icon);
                let pen_rect = pen_resp.rect;
                if pen_resp.clicked() {
                    if self.is_group_selected(0) {
                        self.open_tool_submenu = if self.open_tool_submenu == Some(0) { None } else { Some(0) };
                    } else {
                        self.active_tool = self.pen_group_current.clone();
                        self.open_tool_submenu = None;
                    }
                }

                // Slot 1: Bucket group (Fill/Eyedropper)
                let bucket_icon = tool_icon(&self.bucket_group_current);
                let bucket_resp = tool_btn_raw(ui, &self.theme, self.is_group_selected(1), bucket_icon);
                let bucket_rect = bucket_resp.rect;
                if bucket_resp.clicked() {
                    if self.is_group_selected(1) {
                        self.open_tool_submenu = if self.open_tool_submenu == Some(1) { None } else { Some(1) };
                    } else {
                        self.active_tool = self.bucket_group_current.clone();
                        self.open_tool_submenu = None;
                    }
                }

                // Slot 2: Shape group (Rectangle/Ellipse/Line)
                let shape_icon = tool_icon(&self.shape_group_current);
                let shape_resp = tool_btn_raw(ui, &self.theme, self.is_group_selected(2), shape_icon);
                let shape_rect = shape_resp.rect;
                if shape_resp.clicked() {
                    if self.is_group_selected(2) {
                        self.open_tool_submenu = if self.open_tool_submenu == Some(2) { None } else { Some(2) };
                    } else {
                        self.active_tool = self.shape_group_current.clone();
                        self.open_tool_submenu = None;
                    }
                }

                // Slot 3: Select group (RectSelect/Move)
                let select_icon = tool_icon(&self.select_group_current);
                let select_resp = tool_btn_raw(ui, &self.theme, self.is_group_selected(3), select_icon);
                let select_rect = select_resp.rect;
                if select_resp.clicked() {
                    if self.is_group_selected(3) {
                        self.open_tool_submenu = if self.open_tool_submenu == Some(3) { None } else { Some(3) };
                    } else {
                        self.active_tool = self.select_group_current.clone();
                        self.open_tool_submenu = None;
                    }
                }

                // Slot 4: Zoom (ungrouped)
                let zoom_resp = tool_btn(ui, &mut self.active_tool, &self.theme, ActiveTool::Zoom, egui::include_image!("../assets/icons/zoom.svg"));
                if zoom_resp.clicked() {
                    let now = ctx.input(|i| i.time);
                    if now - self.last_zoom_tool_btn_click < 0.4 {
                        self.pending_zoom_fit = true;
                        self.last_zoom_tool_btn_click = -1.0;
                    } else {
                        self.last_zoom_tool_btn_click = now;
                    }
                }

                // Ungrouped clicks should close any open submenu
                if zoom_resp.clicked() { self.open_tool_submenu = None; }

                // Stash slot rects for the submenu overlay drawn after this panel
                self.pen_slot_rect = Some(pen_rect);
                self.bucket_slot_rect = Some(bucket_rect);
                self.shape_slot_rect = Some(shape_rect);
                self.select_slot_rect = Some(select_rect);
            });
    }

    fn draw_tool_submenu(&mut self, ctx: &egui::Context) {
        let Some(slot) = self.open_tool_submenu else { return; };
        let (slot_rect, current, others): (egui::Rect, ActiveTool, Vec<ActiveTool>) = match slot {
            0 => {
                let Some(r) = self.pen_slot_rect else { return; };
                let cur = self.pen_group_current.clone();
                let others = match cur {
                    ActiveTool::Pencil => vec![ActiveTool::Eraser],
                    _                  => vec![ActiveTool::Pencil],
                };
                (r, cur, others)
            }
            1 => {
                let Some(r) = self.bucket_slot_rect else { return; };
                let cur = self.bucket_group_current.clone();
                let others = match cur {
                    ActiveTool::Fill => vec![ActiveTool::Eyedropper],
                    _                => vec![ActiveTool::Fill],
                };
                (r, cur, others)
            }
            2 => {
                let Some(r) = self.shape_slot_rect else { return; };
                let cur = self.shape_group_current.clone();
                let all = vec![
                    ActiveTool::Rectangle { filled: false },
                    ActiveTool::Ellipse { filled: false },
                    ActiveTool::Line,
                ];
                let others: Vec<ActiveTool> = all.into_iter().filter(|t| {
                    std::mem::discriminant(t) != std::mem::discriminant(&cur)
                }).collect();
                (r, cur, others)
            }
            3 => {
                let Some(r) = self.select_slot_rect else { return; };
                let cur = self.select_group_current.clone();
                let others = match cur {
                    ActiveTool::RectSelect => vec![],
                    _                      => vec![ActiveTool::RectSelect],
                };
                (r, cur, others)
            }
            _ => return,
        };

        let theme = self.theme.clone();
        let pos = Pos2::new(slot_rect.right(), slot_rect.top());
        let mut clicked_tool: Option<ActiveTool> = None;

        let resp = egui::Area::new(egui::Id::new(("tool_submenu", slot)))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme.panel)
                    .shadow(egui::Shadow {
                        offset: [6, 0],
                        blur: 20,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                    })
                    .inner_margin(Margin::same(0))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        ui.horizontal(|ui| {
                            for t in &others {
                                let icon = tool_icon(t);
                                let r = tool_btn_raw(ui, &theme, false, icon);
                                if r.clicked() {
                                    clicked_tool = Some(t.clone());
                                }
                            }
                        });
                    });
            });

        if let Some(t) = clicked_tool {
            self.set_active_tool(t);
            self.open_tool_submenu = None;
        }

        // Click outside both submenu and originating slot → close
        let clicked_outside = ctx.input(|i| i.pointer.any_click()) && {
            let hover = ctx.input(|i| i.pointer.hover_pos());
            match hover {
                Some(p) => !resp.response.rect.contains(p) && !slot_rect.contains(p),
                None => false,
            }
        };
        if clicked_outside {
            self.open_tool_submenu = None;
        }

        let _ = current; // silence unused if we don't need it elsewhere
    }

    fn draw_right_sidebar(&mut self, ctx: &egui::Context) {
        let sidebar_order = self.sidebar_order.clone();

        // Narrow mode: every visible panel is collapsed (only icon rows visible)
        let all_narrow = sidebar_order.iter().all(|&p| {
            !self.ui_state.is_visible(p) || self.ui_state.is_collapsed(p)
        });
        let sidebar_w = if all_narrow { 38.0 } else { 176.0 };

        let mut new_icon_rects: Vec<(Panel, egui::Rect)> = Vec::new();

        SidePanel::right("right_sidebar")
            .exact_width(sidebar_w)
            .resizable(false)
            .frame(Frame::new().fill(self.theme.panel))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.set_width(sidebar_w);
                ui.set_max_width(sidebar_w);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                egui::ScrollArea::vertical()
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.set_width(sidebar_w);
                        ui.set_max_width(sidebar_w);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;

                        let sidebar_x = ui.next_widget_position().x;
                        let dragging = self.sidebar_drag;

                        for (i, &panel) in sidebar_order.iter().enumerate() {
                            if i > 0 {
                                ui.add_space(RIGHT_SECTION_STACK_GAP);
                            }

                            let y_before = ui.next_widget_position().y;

                            if dragging == Some(panel) {
                                // Placeholder for the section being dragged
                                let theme = self.theme.clone();
                                Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 3)).show(ui, |ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 26.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_stroke(
                                        rect.shrink(3.0),
                                        4.0,
                                        egui::Stroke::new(1.0, theme.fg_muted),
                                        egui::StrokeKind::Inside,
                                    );
                                });
                            } else {
                                match panel {
                                    Panel::Palette    => self.draw_palette(ui),
                                    Panel::Color      => self.draw_color_picker(ui),
                                    Panel::Layers     => self.draw_layers_section(ui),
                                    Panel::Animations => self.draw_animations_section(ui),
                                    Panel::Preview    => self.draw_preview_section(ui),
                                    Panel::Timeline   => {},
                                }
                            }

                            let y_after = ui.next_widget_position().y;
                            if y_after > y_before {
                                new_icon_rects.push((panel, egui::Rect::from_min_size(
                                    egui::pos2(sidebar_x, y_before),
                                    egui::vec2(sidebar_w, y_after - y_before),
                                )));
                            }
                        }
                    });
            });

        // Update stored rects with this frame's positions
        self.sidebar_icon_rects = new_icon_rects;

        // ── Long-press drag-to-reorder state machine (narrow mode only) ──
        const LONG_PRESS_SECS: f64 = 0.4;
        let now             = ctx.input(|i| i.time);
        let pointer_pos     = ctx.input(|i| i.pointer.hover_pos());
        let primary_pressed  = ctx.input(|i| i.pointer.primary_pressed());
        let primary_released = ctx.input(|i| i.pointer.primary_released());
        let primary_down     = ctx.input(|i| i.pointer.primary_down());

        if !all_narrow {
            // Left narrow mode — cancel everything
            self.sidebar_drag       = None;
            self.sidebar_drag_over_idx = None;
            self.sidebar_press_start   = None;
        } else {
            // Record press start on icon row
            if primary_pressed {
                if let Some(pos) = pointer_pos {
                    for &(panel, rect) in &self.sidebar_icon_rects {
                        if rect.contains(pos) {
                            self.sidebar_press_start = Some((panel, now));
                            break;
                        }
                    }
                }
            }

            // Promote to drag after long-press threshold
            if let Some((panel, t0)) = self.sidebar_press_start {
                if primary_down && now - t0 >= LONG_PRESS_SECS && self.sidebar_drag.is_none() {
                    self.sidebar_drag       = Some(panel);
                    self.sidebar_drag_over_idx = None;
                    self.sidebar_press_start   = None;
                } else if !primary_down {
                    // Released before threshold — normal click, clear timer
                    self.sidebar_press_start = None;
                } else {
                    // Still within threshold — keep repainting so we hit it
                    ctx.request_repaint();
                }
            }

            // Update drop position while dragging
            if self.sidebar_drag.is_some() {
                if let Some(pos) = pointer_pos {
                    let mut drop_idx = self.sidebar_icon_rects.len();
                    for (i, &(_, rect)) in self.sidebar_icon_rects.iter().enumerate() {
                        if pos.y < rect.center().y {
                            drop_idx = i;
                            break;
                        }
                    }
                    self.sidebar_drag_over_idx = Some(drop_idx);
                }
                ctx.request_repaint();
            }
        }

        // Commit on release
        if primary_released {
            if let (Some(dragged), Some(drop_idx)) = (self.sidebar_drag.take(), self.sidebar_drag_over_idx.take()) {
                if let Some(from_idx) = self.sidebar_order.iter().position(|&p| p == dragged) {
                    let effective = if drop_idx > from_idx { drop_idx - 1 } else { drop_idx };
                    let effective = effective.min(self.sidebar_order.len() - 1);
                    self.sidebar_order.remove(from_idx);
                    self.sidebar_order.insert(effective, dragged);
                }
            }
        }

        // ── Ghost icon at cursor ──────────────────────────────────────────
        if let (Some(dragged_panel), Some(pos)) = (self.sidebar_drag, pointer_pos) {
            let icon_src = panel_icon(dragged_panel);
            egui::Area::new(egui::Id::new("sidebar_drag_ghost"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos - Vec2::splat(8.0))
                .show(ctx, |ui| {
                    ui.add(Image::new(icon_src).tint(Color32::WHITE).fit_to_exact_size(Vec2::splat(16.0)));
                });
        }

        // ── Drop indicator line ───────────────────────────────────────────
        if let (Some(drop_idx), Some(_)) = (self.sidebar_drag_over_idx, self.sidebar_drag) {
            let indicator_y = if self.sidebar_icon_rects.is_empty() {
                None
            } else if drop_idx == 0 {
                Some(self.sidebar_icon_rects[0].1.top())
            } else if drop_idx >= self.sidebar_icon_rects.len() {
                Some(self.sidebar_icon_rects.last().unwrap().1.bottom())
            } else {
                let above = self.sidebar_icon_rects[drop_idx - 1].1.bottom();
                let below = self.sidebar_icon_rects[drop_idx].1.top();
                Some((above + below) / 2.0)
            };
            if let Some(y) = indicator_y {
                let x0 = self.sidebar_icon_rects.first().map_or(0.0, |(_, r)| r.left());
                let x1 = self.sidebar_icon_rects.first().map_or(38.0, |(_, r)| r.right());
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("sidebar_drop_indicator"),
                ));
                painter.line_segment(
                    [egui::pos2(x0, y), egui::pos2(x1, y)],
                    egui::Stroke::new(2.0, Color32::WHITE),
                );
            }
        }
    }

    fn draw_color_picker(&mut self, ui: &mut egui::Ui) {
        if !self.ui_state.is_visible(Panel::Color) {
            let theme = self.theme.clone();
            Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 3)).show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 26.0),
                    egui::Sense::hover(),
                );
                let icon_size = Vec2::splat(16.0);
                let icon_rect = egui::Rect::from_center_size(
                    Pos2::new(rect.left() + 8.0, rect.center().y),
                    icon_size,
                );
                let icon_resp = ui.interact(
                    icon_rect,
                    ui.id().with("color_mixer_toggle"),
                    egui::Sense::click(),
                );
                let icon_tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                ui.put(
                    icon_rect,
                    Image::new(egui::include_image!("../assets/icons/color_mixer.svg"))
                        .tint(icon_tint)
                        .fit_to_exact_size(icon_size),
                );
                if icon_resp.clicked() {
                    self.ui_state.toggle_visible(Panel::Color);
                }
            });
            return;
        }

        let (show, _, _) = section_header_with_add(
            ui,
            &self.theme,
            &mut self.ui_state,
            Panel::Color,
            egui::include_image!("../assets/icons/color_mixer.svg"),
            None,
            false,
        );
        if !show { return; }

        let theme = self.theme.clone();
        let fg = self.color_state.foreground;

        Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
            // Top row: color swatch + tabs + hex
            let top_row_w = ui.available_width();
            let color_box_w = 38.0;
            let gap = 8.0;
            let right_w = (top_row_w - color_box_w - gap).max(1.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let fg_color = Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], fg[3]);
                ui.add(
                    egui::Button::new("")
                        .fill(fg_color)
                        .stroke(egui::Stroke::NONE)
                        .min_size(Vec2::new(color_box_w, color_box_w)),
                );
                ui.add_space(gap);

                // Right column: tabs (horizontal) above hex box
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;

                        // Tabs row
                        let tab_labels = ["OKL", "HSV", "RGB"];
                        let tab_h = 19.0;
                        let tab_w = right_w / 3.0;
                        let mut tab_rects: Vec<egui::Rect> = Vec::new();
                        let mut tab_resps: Vec<egui::Response> = Vec::new();

                        for (_i, _label) in tab_labels.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(tab_w, tab_h), egui::Sense::click());
                            tab_rects.push(r);
                            tab_resps.push(resp);
                        }

                        // Spring highlight
                        let active_idx = match self.color_state.active_picker {
                            PickerMode::OkLab => 0,
                            PickerMode::Hsv   => 1,
                            PickerMode::Rgb   => 2,
                        };
                        let target_x = tab_rects[active_idx].left();
                        let target_w = tab_rects[active_idx].width();

                        if !self.picker_tab_initialized {
                            self.picker_tab_anim_x = target_x;
                            self.picker_tab_initialized = true;
                        }

                        let dt = ui.ctx().input(|i| i.unstable_dt).min(0.05);
                        let force = (target_x - self.picker_tab_anim_x) * 300.0
                                  - self.picker_tab_anim_vel * 22.0;
                        self.picker_tab_anim_vel += force * dt;
                        self.picker_tab_anim_x   += self.picker_tab_anim_vel * dt;

                        let settled = (self.picker_tab_anim_x - target_x).abs() < 0.3
                                   && self.picker_tab_anim_vel.abs() < 0.3;
                        if settled {
                            self.picker_tab_anim_x   = target_x;
                            self.picker_tab_anim_vel = 0.0;
                        } else {
                            ui.ctx().request_repaint();
                        }

                        // Draw highlight behind tabs
                        let highlight = egui::Rect::from_min_size(
                            Pos2::new(self.picker_tab_anim_x, tab_rects[0].top()),
                            Vec2::new(target_w, tab_h),
                        );
                        ui.painter().rect_filled(highlight, 0.0, theme.surface);

                        // Draw tab text on top
                        for (i, label) in tab_labels.iter().enumerate() {
                            let r = tab_rects[i];
                            let resp = &tab_resps[i];
                            let text_color = if self.color_state.active_picker == match i {
                                0 => PickerMode::OkLab,
                                1 => PickerMode::Hsv,
                                _ => PickerMode::Rgb,
                            } {
                                theme.fg
                            } else if resp.hovered() {
                                Color32::WHITE
                            } else {
                                theme.fg_desc
                            };
                            ui.painter().text(
                                r.center(),
                                egui::Align2::CENTER_CENTER,
                                label,
                                FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
                                text_color,
                            );
                            if resp.clicked() {
                                let mode = match i {
                                    0 => PickerMode::OkLab,
                                    1 => PickerMode::Hsv,
                                    _ => PickerMode::Rgb,
                                };
                                if mode != self.color_state.active_picker {
                                    let old_mode = self.color_state.active_picker;
                                    let alpha = self.color_state.foreground[3];
                                    // Compute target for new mode from current foreground
                                    match mode {
                                        PickerMode::OkLab => {
                                            let (l, c, h) = rgba_to_oklch(self.color_state.foreground);
                                            self.color_state.oklch_l = l;
                                            self.color_state.oklch_c = c;
                                            self.color_state.oklch_h = h;
                                        }
                                        PickerMode::Hsv => {
                                            let (h, s, v) = rgba_to_hsv(self.color_state.foreground);
                                            self.color_state.hsv_h = h;
                                            self.color_state.hsv_s = s;
                                            self.color_state.hsv_v = v;
                                        }
                                        PickerMode::Rgb => {
                                            let fg = self.color_state.foreground;
                                            self.color_state.rgb_r = fg[0] as f32;
                                            self.color_state.rgb_g = fg[1] as f32;
                                            self.color_state.rgb_b = fg[2] as f32;
                                        }
                                    }
                                    // Compute tween START by converting old mode's DISPLAY values
                                    // into the new mode's space. This guarantees a visible tween
                                    // because the start is the "visual position" we just left.
                                    let old_rgba = match old_mode {
                                        PickerMode::OkLab => oklch_to_rgba(
                                            self.color_state.display_oklch_l,
                                            self.color_state.display_oklch_c,
                                            self.color_state.display_oklch_h,
                                            alpha,
                                        ),
                                        PickerMode::Hsv => hsv_to_rgba(
                                            self.color_state.display_hsv_h,
                                            self.color_state.display_hsv_s,
                                            self.color_state.display_hsv_v,
                                            alpha,
                                        ),
                                        PickerMode::Rgb => [
                                            self.color_state.display_rgb_r as u8,
                                            self.color_state.display_rgb_g as u8,
                                            self.color_state.display_rgb_b as u8,
                                            alpha,
                                        ],
                                    };
                                    match mode {
                                        PickerMode::OkLab => {
                                            let (l, c, h) = rgba_to_oklch(old_rgba);
                                            self.color_state.display_oklch_l = l;
                                            self.color_state.display_oklch_c = c;
                                            self.color_state.display_oklch_h = h;
                                            self.picker_tween_from_okl_l = l;
                                            self.picker_tween_from_okl_c = c;
                                            self.picker_tween_from_okl_h = h;
                                        }
                                        PickerMode::Hsv => {
                                            let (h, s, v) = rgba_to_hsv(old_rgba);
                                            self.color_state.display_hsv_h = h;
                                            self.color_state.display_hsv_s = s;
                                            self.color_state.display_hsv_v = v;
                                            self.picker_tween_from_hsv_h = h;
                                            self.picker_tween_from_hsv_s = s;
                                            self.picker_tween_from_hsv_v = v;
                                        }
                                        PickerMode::Rgb => {
                                            self.color_state.display_rgb_r = old_rgba[0] as f32;
                                            self.color_state.display_rgb_g = old_rgba[1] as f32;
                                            self.color_state.display_rgb_b = old_rgba[2] as f32;
                                            self.picker_tween_from_rgb_r = old_rgba[0] as f32;
                                            self.picker_tween_from_rgb_g = old_rgba[1] as f32;
                                            self.picker_tween_from_rgb_b = old_rgba[2] as f32;
                                        }
                                    }
                                    self.picker_tweening = true;
                                    self.picker_tween_start = ui.ctx().input(|i| i.time);
                                    self.color_state.active_picker = mode;
                                    eprintln!("TAB CLICK: mode={:?} from_okl_l={} target_okl_l={} tweening set true", mode, self.picker_tween_from_okl_l, self.color_state.oklch_l);
                                }
                            }
                        }
                    });

                    // Hex display — directly below tabs, same width, clickable to copy
                    let hex_h = 19.0;
                    let hex_text = format!("#{:02X}{:02X}{:02X}", fg[0], fg[1], fg[2]);
                    let (hex_rect, hex_resp) = ui.allocate_exact_size(Vec2::new(right_w, hex_h), egui::Sense::click());
                    if hex_resp.hovered() {
                        ui.painter().rect_filled(hex_rect, 0.0, theme.surface);
                    } else {
                        ui.painter().rect_filled(hex_rect, 0.0, theme.bg);
                    }
                    ui.painter().text(
                        hex_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &hex_text,
                        FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
                        theme.fg,
                    );
                    if hex_resp.clicked() {
                        ui.ctx().copy_text(hex_text);
                    }
                });
            });

            ui.add_space(8.0);

            // Sync non-active mode caches from current foreground
            match self.color_state.active_picker {
                PickerMode::OkLab => {
                    let (h, s, v) = rgba_to_hsv(self.color_state.foreground);
                    self.color_state.hsv_h = h;
                    self.color_state.hsv_s = s;
                    self.color_state.hsv_v = v;
                    let fg = self.color_state.foreground;
                    self.color_state.rgb_r = fg[0] as f32;
                    self.color_state.rgb_g = fg[1] as f32;
                    self.color_state.rgb_b = fg[2] as f32;
                }
                PickerMode::Hsv => {
                    let (l, c, h) = rgba_to_oklch(self.color_state.foreground);
                    self.color_state.oklch_l = l;
                    self.color_state.oklch_c = c;
                    self.color_state.oklch_h = h;
                    let fg = self.color_state.foreground;
                    self.color_state.rgb_r = fg[0] as f32;
                    self.color_state.rgb_g = fg[1] as f32;
                    self.color_state.rgb_b = fg[2] as f32;
                }
                PickerMode::Rgb => {
                    let (l, c, h) = rgba_to_oklch(self.color_state.foreground);
                    self.color_state.oklch_l = l;
                    self.color_state.oklch_c = c;
                    self.color_state.oklch_h = h;
                    let (h2, s, v) = rgba_to_hsv(self.color_state.foreground);
                    self.color_state.hsv_h = h2;
                    self.color_state.hsv_s = s;
                    self.color_state.hsv_v = v;
                }
            }

            // Tab-switch tween: interpolate display values toward target values
            if self.picker_tweening {
                let now = ui.ctx().input(|i| i.time);
                let elapsed = (now - self.picker_tween_start) as f32;
                eprintln!("TWEEN active_picker={:?} elapsed={:.3} tweening={}", self.color_state.active_picker, elapsed, self.picker_tweening);
                if elapsed >= 2.0 {
                    self.picker_tweening = false;
                    match self.color_state.active_picker {
                        PickerMode::OkLab => {
                            self.color_state.display_oklch_l = self.color_state.oklch_l;
                            self.color_state.display_oklch_c = self.color_state.oklch_c;
                            self.color_state.display_oklch_h = self.color_state.oklch_h;
                        }
                        PickerMode::Hsv => {
                            self.color_state.display_hsv_h = self.color_state.hsv_h;
                            self.color_state.display_hsv_s = self.color_state.hsv_s;
                            self.color_state.display_hsv_v = self.color_state.hsv_v;
                        }
                        PickerMode::Rgb => {
                            self.color_state.display_rgb_r = self.color_state.rgb_r;
                            self.color_state.display_rgb_g = self.color_state.rgb_g;
                            self.color_state.display_rgb_b = self.color_state.rgb_b;
                        }
                    }
                } else {
                    let t = (elapsed / 2.0).clamp(0.0, 1.0);
                    match self.color_state.active_picker {
                        PickerMode::OkLab => {
                            self.color_state.display_oklch_l = lerp(self.picker_tween_from_okl_l, self.color_state.oklch_l, t);
                            self.color_state.display_oklch_c = lerp(self.picker_tween_from_okl_c, self.color_state.oklch_c, t);
                            self.color_state.display_oklch_h = lerp(self.picker_tween_from_okl_h, self.color_state.oklch_h, t);
                        }
                        PickerMode::Hsv => {
                            self.color_state.display_hsv_h = lerp(self.picker_tween_from_hsv_h, self.color_state.hsv_h, t);
                            self.color_state.display_hsv_s = lerp(self.picker_tween_from_hsv_s, self.color_state.hsv_s, t);
                            self.color_state.display_hsv_v = lerp(self.picker_tween_from_hsv_v, self.color_state.hsv_v, t);
                        }
                        PickerMode::Rgb => {
                            self.color_state.display_rgb_r = lerp(self.picker_tween_from_rgb_r, self.color_state.rgb_r, t);
                            self.color_state.display_rgb_g = lerp(self.picker_tween_from_rgb_g, self.color_state.rgb_g, t);
                            self.color_state.display_rgb_b = lerp(self.picker_tween_from_rgb_b, self.color_state.rgb_b, t);
                        }
                    }
                    ui.ctx().request_repaint();
                }
            } else {
                // When not tweening, snap active mode display to target
                match self.color_state.active_picker {
                    PickerMode::OkLab => {
                        self.color_state.display_oklch_l = self.color_state.oklch_l;
                        self.color_state.display_oklch_c = self.color_state.oklch_c;
                        self.color_state.display_oklch_h = self.color_state.oklch_h;
                    }
                    PickerMode::Hsv => {
                        self.color_state.display_hsv_h = self.color_state.hsv_h;
                        self.color_state.display_hsv_s = self.color_state.hsv_s;
                        self.color_state.display_hsv_v = self.color_state.hsv_v;
                    }
                    PickerMode::Rgb => {
                        self.color_state.display_rgb_r = self.color_state.rgb_r;
                        self.color_state.display_rgb_g = self.color_state.rgb_g;
                        self.color_state.display_rgb_b = self.color_state.rgb_b;
                    }
                }
            }

            // Sliders per mode — use display values for visual, sync cache+fg on change
            match self.color_state.active_picker {
                PickerMode::OkLab => {
                    let mut changed = false;
                    let mut l = self.color_state.display_oklch_l;
                    let mut c = self.color_state.display_oklch_c;
                    let mut h = self.color_state.display_oklch_h;
                    changed |= color_slider(ui, &theme, "L", &mut l, 0.0, 1.0);
                    changed |= color_slider(ui, &theme, "C", &mut c, 0.0, 0.4);
                    changed |= color_slider(ui, &theme, "H", &mut h, 0.0, 360.0);
                    if changed {
                        self.picker_tweening = false;
                        self.color_state.oklch_l = l;
                        self.color_state.oklch_c = c;
                        self.color_state.oklch_h = h;
                        self.color_state.display_oklch_l = l;
                        self.color_state.display_oklch_c = c;
                        self.color_state.display_oklch_h = h;
                        self.color_state.foreground = oklch_to_rgba(l, c, h, fg[3]);
                    }
                }
                PickerMode::Hsv => {
                    let mut changed = false;
                    let mut h = self.color_state.display_hsv_h;
                    let mut s = self.color_state.display_hsv_s;
                    let mut v = self.color_state.display_hsv_v;
                    changed |= color_slider(ui, &theme, "H", &mut h, 0.0, 360.0);
                    changed |= color_slider(ui, &theme, "S", &mut s, 0.0, 1.0);
                    changed |= color_slider(ui, &theme, "V", &mut v, 0.0, 1.0);
                    if changed {
                        self.picker_tweening = false;
                        self.color_state.hsv_h = h;
                        self.color_state.hsv_s = s;
                        self.color_state.hsv_v = v;
                        self.color_state.display_hsv_h = h;
                        self.color_state.display_hsv_s = s;
                        self.color_state.display_hsv_v = v;
                        self.color_state.foreground = hsv_to_rgba(h, s, v, fg[3]);
                    }
                }
                PickerMode::Rgb => {
                    let mut changed = false;
                    let mut r = self.color_state.display_rgb_r;
                    let mut g = self.color_state.display_rgb_g;
                    let mut b = self.color_state.display_rgb_b;
                    changed |= color_slider(ui, &theme, "R", &mut r, 0.0, 255.0);
                    changed |= color_slider(ui, &theme, "G", &mut g, 0.0, 255.0);
                    changed |= color_slider(ui, &theme, "B", &mut b, 0.0, 255.0);
                    if changed {
                        self.picker_tweening = false;
                        self.color_state.rgb_r = r;
                        self.color_state.rgb_g = g;
                        self.color_state.rgb_b = b;
                        self.color_state.display_rgb_r = r;
                        self.color_state.display_rgb_g = g;
                        self.color_state.display_rgb_b = b;
                        self.color_state.foreground = [r as u8, g as u8, b as u8, fg[3]];
                    }
                }
            }
        });
    }

    fn draw_palette(&mut self, ui: &mut egui::Ui) {
        // Collapsed: show section-header-style icon row; click to expand.
        if self.ui_state.is_collapsed(Panel::Palette) {
            let theme = self.theme.clone();
            Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 3)).show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 26.0),
                    egui::Sense::hover(),
                );
                let icon_size = Vec2::splat(16.0);
                let icon_rect = egui::Rect::from_center_size(
                    Pos2::new(rect.left() + 8.0, rect.center().y),
                    icon_size,
                );
                let icon_resp = ui.interact(
                    icon_rect,
                    ui.id().with("palette_icon"),
                    egui::Sense::click(),
                );
                let icon_tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                ui.put(
                    icon_rect,
                    Image::new(egui::include_image!("../assets/icons/colors.svg"))
                        .tint(icon_tint)
                        .fit_to_exact_size(icon_size),
                );
                if icon_resp.clicked() {
                    self.ui_state.toggle_collapsed(Panel::Palette);
                }
            });
            return;
        }

        let theme = self.theme.clone();
        const GRID_SIZE: f32 = 176.0;

        let palette_len = self.project.palette.len();

        // Grid is always cols×cols (square). "+" is pinned to the last slot
        // (bottom-right). cols grows so that cols² >= palette_len + 1.
        let cols = ((palette_len + 1) as f32).sqrt().ceil() as usize;
        let cols = cols.max(4);
        let sw = GRID_SIZE / cols as f32; // square swatch width
        let sh = sw;                      // square swatch height
        let total_slots = cols * cols;    // always fills the full 176×176 grid

        Frame::new().fill(theme.panel).inner_margin(Margin::same(0)).show(ui, |ui| {
            let (grid_rect, _) = ui.allocate_exact_size(
                Vec2::new(GRID_SIZE, GRID_SIZE),
                egui::Sense::hover(),
            );

            let painter = ui.painter_at(grid_rect);
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let is_dragging = ui.input(|i| i.pointer.is_decidedly_dragging());
            let released = ui.input(|i| i.pointer.any_released());

            // --- Palette swatches ---
            for i in 0..palette_len {
                let col = i % cols;
                let row = i / cols;
                let rect = egui::Rect::from_min_size(
                    grid_rect.min + Vec2::new(col as f32 * sw, row as f32 * sh),
                    Vec2::new(sw, sh),
                );

                if self.palette_drag_idx == Some(i) {
                    painter.rect_filled(rect, 0.0, theme.surface);
                    continue;
                }

                let swatch = self.project.palette[i];
                let color = Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
                painter.rect_filled(rect, 0.0, color);

                let current_fg = self.color_state.foreground;
                if swatch == current_fg {
                    painter.rect_stroke(rect, 0.0, egui::Stroke::new(2.0, theme.fg), egui::StrokeKind::Inside);
                }

                let resp = ui.interact(rect, ui.id().with(("swatch", i)), egui::Sense::click_and_drag());
                if resp.drag_started() {
                    self.palette_drag_idx = Some(i);
                }
                if resp.clicked() {
                    self.color_state.foreground = swatch;
                    sync_color_caches(&mut self.color_state);
                }
            }

            // --- Empty slots between last color and "+" ---
            for i in palette_len..(total_slots - 1) {
                let col = i % cols;
                let row = i / cols;
                let rect = egui::Rect::from_min_size(
                    grid_rect.min + Vec2::new(col as f32 * sw, row as f32 * sh),
                    Vec2::new(sw, sh),
                );
                painter.rect_filled(rect, 0.0, theme.panel);
            }

            // --- Handle drop to reorder ---
            if released {
                if let Some(drag_idx) = self.palette_drag_idx.take() {
                    if let Some(pos) = pointer_pos {
                        if grid_rect.contains(pos) {
                            let drop_col = ((pos.x - grid_rect.min.x) / sw) as usize;
                            let drop_row = ((pos.y - grid_rect.min.y) / sh) as usize;
                            let drop_idx = (drop_row * cols + drop_col).min(palette_len.saturating_sub(1));
                            if drop_idx != drag_idx {
                                let color = self.project.palette.remove(drag_idx);
                                self.project.palette.insert(drop_idx, color);
                            }
                        }
                    }
                }
            }

            // --- "+" add-swatch button — always bottom-right ---
            {
                let plus_slot = total_slots - 1;
                let col = plus_slot % cols;
                let row = plus_slot / cols;
                let rect = egui::Rect::from_min_size(
                    grid_rect.min + Vec2::new(col as f32 * sw, row as f32 * sh),
                    Vec2::new(sw, sh),
                );
                painter.rect_filled(rect, 0.0, theme.surface);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "+",
                    FontId::new(24.0, FontFamily::Proportional),
                    Color32::WHITE,
                );
                let resp = ui.interact(rect, ui.id().with("add_swatch"), egui::Sense::click());
                if resp.clicked() && palette_len < 256 {
                    self.project.palette.push(self.color_state.foreground);
                }
            }

            // --- Drag ghost for internal palette drag ---
            if let Some(drag_idx) = self.palette_drag_idx {
                if is_dragging {
                    if let Some(pos) = pointer_pos {
                        let ghost_rect = egui::Rect::from_center_size(pos, Vec2::new(sw, sh));
                        let ghost_painter = ui.ctx().layer_painter(egui::LayerId::new(
                            egui::Order::Tooltip,
                            egui::Id::new("palette_drag_ghost"),
                        ));
                        let swatch = self.project.palette[drag_idx];
                        let color = Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
                        ghost_painter.rect_filled(ghost_rect, 4.0, color);
                        ghost_painter.rect_stroke(
                            ghost_rect,
                            4.0,
                            egui::Stroke::new(2.0, theme.fg),
                            egui::StrokeKind::Outside,
                        );
                    }
                }
            }
            // --- Ctrl+Click anywhere in grid to collapse ---
            if ui.input(|i| i.modifiers.ctrl && i.pointer.any_click()) {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    if grid_rect.contains(pos) {
                        self.ui_state.toggle_collapsed(Panel::Palette);
                    }
                }
            }
        });
    }

    fn draw_layers_section(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        let (show, add_clicked, group_clicked) = section_header(ui, &self.theme, &mut self.ui_state, Panel::Layers, egui::include_image!("../assets/icons/layer.svg"), Some(egui::include_image!("../assets/icons/group.svg")));
        if !show { return; }

        let ai = self.project.active_animation;
        let fi = self.project.active_frame;

        if add_clicked {
            let idx = self.project.animations[ai].frames[fi].layers.len();
            let new_id = self.project.next_layer_id();
            let name = format!("Layer {}", idx + 1);
            let w = self.project.canvas_width;
            let h = self.project.canvas_height;
            self.undo_stack.push(Command::AddLayer { index: idx, name: name.clone(), id: new_id });
            // Add a blank copy of this layer to every frame of every animation so they stay in sync.
            for anim in &mut self.project.animations {
                for frame in &mut anim.frames {
                    frame.layers.push(Layer::new_with_id(name.clone(), w, h, new_id));
                }
            }
            self.project.active_layer = idx;
        }

        if group_clicked {
            let idx = self.project.animations[ai].frames[fi].layers.len();
            let new_id = self.project.next_layer_id();
            let n = self.project.animations[ai].frames[fi].layers.iter().filter(|l| l.is_group).count() + 1;
            let name = format!("Group {}", n);
            let w = self.project.canvas_width;
            let h = self.project.canvas_height;
            self.undo_stack.push(Command::AddLayer { index: idx, name: name.clone(), id: new_id });
            for anim in &mut self.project.animations {
                for frame in &mut anim.frames {
                    frame.layers.push(Layer::new_group(name.clone(), w, h, new_id));
                }
            }
            self.project.active_layer = idx;
        }

        let layer_count = self.project.animations[ai].frames[fi].layers.len();
        const ROW_H: f32 = 30.0;
        const MAX_VISIBLE: f32 = 5.0;

        // Spring: visual slot of active layer (list drawn in reverse, top = highest index)
        let active_visual_slot = layer_count.saturating_sub(1).saturating_sub(self.project.active_layer);
        let target_y = active_visual_slot as f32 * ROW_H;
        let dt = ui.ctx().input(|i| i.unstable_dt).min(0.05);
        let force = (target_y - self.layer_sel_y) * 300.0 - self.layer_sel_vel * 22.0;
        self.layer_sel_vel += force * dt;
        self.layer_sel_y   += self.layer_sel_vel * dt;
        if (self.layer_sel_y - target_y).abs() < 0.3 && self.layer_sel_vel.abs() < 0.3 {
            self.layer_sel_y   = target_y;
            self.layer_sel_vel = 0.0;
        } else {
            ui.ctx().request_repaint();
        }

        // Pending context-menu action: (0=duplicate, 1=merge_down, 2=delete, layer_index)

        let list_width = ui.available_width();

        // Count only visible rows (hidden children of collapsed groups are skipped)
        let visible_count = (0..layer_count).filter(|&i| {
            let layer = &self.project.animations[ai].frames[fi].layers[i];
            if let Some(gid) = layer.group_id {
                !self.project.animations[ai].frames[fi].layers
                    .iter().any(|l| l.is_group && l.id == gid && l.collapsed)
            } else { true }
        }).count();

        egui::ScrollArea::vertical()
            .id_salt("layers_scroll")
            .max_height(MAX_VISIBLE * ROW_H)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let list_width = list_width;
                let list_height = (visible_count as f32 * ROW_H).max(MAX_VISIBLE * ROW_H);
                let list_origin = ui.next_widget_position();
                let list_rect = egui::Rect::from_min_size(list_origin, Vec2::new(list_width, list_height));
                ui.painter().rect_filled(list_rect, 0.0, theme.panel);
                let sel_rect = egui::Rect::from_min_size(
                    Pos2::new(list_origin.x, list_origin.y + self.layer_sel_y),
                    Vec2::new(list_width, ROW_H),
                );
                ui.painter().with_clip_rect(list_rect).rect_filled(sel_rect, 0.0, theme.surface);

                for idx in (0..layer_count).rev() {
                    // Skip children whose parent group is collapsed
                    {
                        let layer = &self.project.animations[ai].frames[fi].layers[idx];
                        if let Some(gid) = layer.group_id {
                            let parent_collapsed = self.project.animations[ai].frames[fi].layers
                                .iter().any(|l| l.is_group && l.id == gid && l.collapsed);
                            if parent_collapsed { continue; }
                        }
                    }
                    let is_active = self.project.active_layer == idx;
                    let is_renaming = matches!(&self.renaming_layer, Some((i, _)) if *i == idx);

                    // Secondary-click check BEFORE any widgets are placed in this row
                    let row_origin = ui.next_widget_position();
                    let row_rect = egui::Rect::from_min_size(row_origin, Vec2::new(list_width, ROW_H));
                    if ui.input(|i| {
                        i.pointer.secondary_clicked() &&
                        i.pointer.interact_pos().map(|p| row_rect.contains(p)).unwrap_or(false)
                    }) {
                        let pos = ui.input(|i| i.pointer.interact_pos().unwrap_or(row_origin));
                        let now = ui.ctx().input(|i| i.time);
                        self.layer_ctx_menu = Some((idx, pos, now));
                    }

                    ui.allocate_ui_with_layout(
                        Vec2::new(list_width, ROW_H),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                                let bg_resp = ui.interact(
                                    ui.max_rect(),
                                    egui::Id::new(("layer_row", idx)),
                                    egui::Sense::click_and_drag(),
                                );
                                if bg_resp.clicked() && !is_renaming {
                                    let now = ui.ctx().input(|i| i.time);
                                    if let Some((last_idx, last_t)) = self.last_layer_click {
                                        if last_idx == idx && (now - last_t) < 0.4 {
                                            // Double-click: start rename
                                            let name = self.project.animations[ai].frames[fi].layers[idx].name.clone();
                                            self.renaming_layer = Some((idx, name));
                                            self.last_layer_click = None;
                                        } else {
                                            self.project.active_layer = idx;
                                            self.last_layer_click = Some((idx, now));
                                        }
                                    } else {
                                        self.project.active_layer = idx;
                                        self.last_layer_click = Some((idx, now));
                                    }
                                }
                                // Drag start
                                if bg_resp.drag_started() {
                                    self.layer_drag = Some(idx);
                                }
                                // Drag hover — raw pointer check because egui captures the pointer
                                // to the drag-source widget, making bg_resp.hovered() false on
                                // all other rows during the drag.
                                if self.layer_drag.is_some() {
                                    let over = ui.input(|i| i.pointer.hover_pos())
                                        .map(|p| row_rect.contains(p))
                                        .unwrap_or(false);
                                    if over {
                                        self.layer_drag_over = Some(idx);
                                    }
                                }
                                // Drop: release drag
                                if bg_resp.drag_stopped() {
                                    if let (Some(drag_idx), Some(over_idx)) = (self.layer_drag, self.layer_drag_over) {
                                        if drag_idx != over_idx {
                                            let layers = &self.project.animations[ai].frames[fi].layers;
                                            let is_target_group    = over_idx  < layers.len() && layers[over_idx].is_group;
                                            let dragging_non_group = drag_idx  < layers.len() && !layers[drag_idx].is_group;
                                            let drag_group_id      = layers.get(drag_idx).and_then(|l| l.group_id);
                                            let over_group_id      = layers.get(over_idx).and_then(|l| l.group_id);

                                            if is_target_group && dragging_non_group {
                                                // Drop non-group onto a group → assign + reposition below group
                                                let group_id = layers[over_idx].id;
                                                let layers = &mut self.project.animations[ai].frames[fi].layers;
                                                let mut dragged = layers.remove(drag_idx);
                                                dragged.group_id = Some(group_id);
                                                let new_g = if drag_idx < over_idx { over_idx - 1 } else { over_idx };
                                                layers.insert(new_g, dragged);
                                                self.project.active_layer = new_g;
                                            } else if !is_target_group && drag_group_id == over_group_id {
                                                // Reorder within same group (or both ungrouped): swap positions
                                                let layers = &mut self.project.animations[ai].frames[fi].layers;
                                                layers.swap(drag_idx, over_idx);
                                                self.project.active_layer = over_idx;
                                            }
                                        }
                                    }
                                    self.layer_drag = None;
                                    self.layer_drag_over = None;
                                }

                                let is_group = self.project.animations[ai].frames[fi].layers[idx].is_group;
                                let in_group = self.project.animations[ai].frames[fi].layers[idx].group_id.is_some();
                                let this_group_id = self.project.animations[ai].frames[fi].layers[idx].group_id;
                                let is_drag_over_group = self.layer_drag.is_some()
                                    && self.layer_drag_over == Some(idx) && is_group;
                                // Reorder target: cursor is over this row, it's not a group, and it
                                // shares the same group membership as the layer being dragged.
                                let is_drag_reorder_target = self.layer_drag.map(|di| {
                                    let layers = &self.project.animations[ai].frames[fi].layers;
                                    let drag_gid = layers.get(di).and_then(|l| l.group_id);
                                    di != idx && !is_group && drag_gid == this_group_id
                                }).unwrap_or(false) && self.layer_drag_over == Some(idx);

                                // Highlight drop target
                                if is_drag_over_group {
                                    // Group-drop: bright border = "will become a child"
                                    ui.painter().rect_stroke(ui.max_rect(), 0.0, egui::Stroke::new(1.5, theme.fg), egui::epaint::StrokeKind::Inside);
                                } else if is_drag_reorder_target {
                                    // Reorder: accent fill = "will swap here"
                                    ui.painter().rect_filled(ui.max_rect(), 0.0, theme.accent);
                                }

                                // Indent for child layers
                                let indent = if in_group { 14.0 } else { 10.0 };
                                ui.add_space(indent);

                                // Collapse toggle for group layers is now on the right side

                            if is_renaming {
                                let buf = &mut self.renaming_layer.as_mut().unwrap().1;
                                let rename_font = if is_group {
                                    egui::FontId::new(FONT_SIZE_SM, FontFamily::Name("bold".into()))
                                } else {
                                    egui::FontId::new(FONT_SIZE_SM, egui::FontFamily::Proportional)
                                };
                                let edit = egui::TextEdit::singleline(buf)
                                    .font(rename_font)
                                    .desired_width(ui.available_width() - 56.0)
                                    .frame(false);
                                let resp = ui.add(edit);
                                resp.request_focus();
                                let commit = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                if commit || (!resp.has_focus() && !resp.gained_focus()) {
                                    if let Some((i, new_name)) = self.renaming_layer.take() {
                                        if !new_name.trim().is_empty() {
                                            self.project.animations[ai].frames[fi].layers[i].name =
                                                new_name.trim().to_string();
                                        }
                                    }
                                } else if cancel {
                                    self.renaming_layer = None;
                                }
                            } else {
                                let name = self.project.animations[ai].frames[fi].layers[idx].name.clone();
                                let label_rt = if is_group {
                                    egui::RichText::new(&name)
                                        .color(if is_active { theme.fg } else { theme.fg_desc })
                                        .font(egui::FontId::new(FONT_SIZE_SM, FontFamily::Name("bold".into())))
                                } else {
                                    egui::RichText::new(&name)
                                        .color(if is_active { theme.fg } else { theme.fg_desc })
                                        .font(egui::FontId::new(FONT_SIZE_SM, egui::FontFamily::Proportional))
                                };
                                ui.add(egui::Label::new(label_rt).sense(egui::Sense::hover()));
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // 10px right margin aligns lock center with header "+" center;
                                // 4px gaps between icons give the same 20px pitch as the header.
                                ui.add_space(10.0);
                                let locked = self.project.animations[ai].frames[fi].layers[idx].locked;
                                if icon_flat_button(ui, &theme, if locked {
                                    egui::include_image!("../assets/icons/lock.svg")
                                } else {
                                    egui::include_image!("../assets/icons/lock_open.svg")
                                }).clicked() {
                                    self.project.animations[ai].frames[fi].layers[idx].locked = !locked;
                                }
                                ui.add_space(4.0);
                                let visible = self.project.animations[ai].frames[fi].layers[idx].visible;
                                if icon_flat_button(ui, &theme, if visible {
                                    egui::include_image!("../assets/icons/visibility.svg")
                                } else {
                                    egui::include_image!("../assets/icons/visibility_off.svg")
                                }).clicked() {
                                    self.project.animations[ai].frames[fi].layers[idx].visible = !visible;
                                    self.canvas_dirty = true;
                                }
                                // Folder collapse toggle — added last so it's leftmost (first of three)
                                if is_group {
                                    ui.add_space(4.0);
                                    let collapsed = self.project.animations[ai].frames[fi].layers[idx].collapsed;
                                    let folder_img = if collapsed {
                                        egui::include_image!("../assets/icons/folder_closed.svg")
                                    } else {
                                        egui::include_image!("../assets/icons/folder_open.svg")
                                    };
                                    if icon_flat_button(ui, &theme, folder_img).clicked() {
                                        self.project.animations[ai].frames[fi].layers[idx].collapsed = !collapsed;
                                    }
                                }
                                // Opacity input (0-100). Skipped for group rows (no pixel data).
                                if !is_group {
                                    ui.add_space(6.0);
                                    let current_u8 = self.project.animations[ai].frames[fi].layers[idx].opacity;
                                    let mut pct: u32 = ((current_u8 as f32 / 255.0) * 100.0).round() as u32;
                                    // Match row: bg = selection color (theme.surface), text = icon color (theme.fg_desc).
                                    let bg = if is_active { theme.surface } else { theme.panel };
                                    let text_col = theme.fg_desc;
                                    let v = ui.visuals_mut();
                                    v.widgets.inactive.bg_fill   = bg;
                                    v.widgets.inactive.weak_bg_fill = bg;
                                    v.widgets.hovered.bg_fill    = bg;
                                    v.widgets.hovered.weak_bg_fill = bg;
                                    v.widgets.active.bg_fill     = bg;
                                    v.widgets.active.weak_bg_fill = bg;
                                    v.widgets.inactive.fg_stroke.color = text_col;
                                    v.widgets.hovered.fg_stroke.color  = Color32::WHITE;
                                    v.widgets.active.fg_stroke.color   = Color32::WHITE;
                                    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                                    v.widgets.hovered.bg_stroke  = egui::Stroke::NONE;
                                    v.widgets.active.bg_stroke   = egui::Stroke::NONE;
                                    v.selection.stroke.color = text_col;
                                    v.override_text_color = Some(text_col);
                                    let resp = ui.add(
                                        egui::DragValue::new(&mut pct)
                                            .range(0..=100)
                                            .suffix("%")
                                            .speed(1.0)
                                    );
                                    if resp.changed() {
                                        let new_u8 = ((pct as f32 / 100.0) * 255.0).round().clamp(0.0, 255.0) as u8;
                                        if new_u8 != current_u8 {
                                            self.project.animations[ai].frames[fi].layers[idx].opacity = new_u8;
                                            self.canvas_dirty = true;
                                        }
                                    }
                                }
                            });
                        },
                    );
                }
            });

        // Apply deferred context-menu actions
    }

    fn draw_animations_section(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        let was_collapsed = self.ui_state.is_collapsed(Panel::Animations);
        let (show, add_clicked, _) = section_header(ui, &self.theme, &mut self.ui_state, Panel::Animations, egui::include_image!("../assets/icons/animation.svg"), None);
        let now_collapsed = self.ui_state.is_collapsed(Panel::Animations);

        // When the animation icon is clicked, sync timeline visibility with the section.
        if now_collapsed != was_collapsed {
            self.ui_state.show_timeline = !now_collapsed;
        }
        if !show { return; }

        if add_clicked {
            let n = self.project.animations.len() + 1;
            let anim = self.new_animation_from_layers(format!("anim_{}", n));
            self.project.animations.push(anim);
        }

        let anim_count = self.project.animations.len();
        const ROW_H: f32 = 30.0;
        const MAX_VISIBLE: f32 = 5.0;

        // Spring: target is active_animation * ROW_H
        let target_y = self.project.active_animation as f32 * ROW_H;
        let dt = ui.ctx().input(|i| i.unstable_dt).min(0.05);
        let force = (target_y - self.anim_sel_y) * 300.0 - self.anim_sel_vel * 22.0;
        self.anim_sel_vel += force * dt;
        self.anim_sel_y   += self.anim_sel_vel * dt;
        let settled = (self.anim_sel_y - target_y).abs() < 0.3 && self.anim_sel_vel.abs() < 0.3;
        if settled {
            self.anim_sel_y   = target_y;
            self.anim_sel_vel = 0.0;
        } else {
            ui.ctx().request_repaint();
        }

        let list_width = ui.available_width();
        egui::ScrollArea::vertical()
            .id_salt("animations_scroll")
            .max_height(MAX_VISIBLE * ROW_H)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let list_width = list_width;
                let list_height = (anim_count as f32 * ROW_H).max(MAX_VISIBLE * ROW_H);
                let list_origin = ui.next_widget_position();
                let list_rect = egui::Rect::from_min_size(list_origin, Vec2::new(list_width, list_height));
                ui.painter().rect_filled(list_rect, 0.0, theme.panel);
                let sel_rect = egui::Rect::from_min_size(
                    Pos2::new(list_origin.x, list_origin.y + self.anim_sel_y),
                    Vec2::new(list_width, ROW_H),
                );
                ui.painter().with_clip_rect(list_rect).rect_filled(sel_rect, 0.0, theme.surface);

                for i in 0..anim_count {
                    let selected = self.project.active_animation == i;
                    let is_renaming = matches!(&self.renaming_animation, Some((idx, _)) if *idx == i);

                    Frame::new().fill(Color32::TRANSPARENT).inner_margin(Margin::same(0)).show(ui, |ui| {
                        ui.set_min_width(list_width);
                        ui.allocate_ui_with_layout(
                            Vec2::new(list_width, ROW_H),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let bg_resp = ui.interact(
                                    ui.max_rect(),
                                    egui::Id::new(("anim_row", i)),
                                    egui::Sense::click(),
                                );
                                if bg_resp.clicked() && !is_renaming {
                                    self.project.active_animation = i;
                                    self.project.active_frame = 0;
                                    let layer_count = self.project.animations[i].frames[0].layers.len();
                                    self.project.active_layer = self.project.active_layer.min(layer_count.saturating_sub(1));
                                    self.canvas_dirty = true;
                                }

                                ui.add_space(10.0);

                                if is_renaming {
                                    let buf = &mut self.renaming_animation.as_mut().unwrap().1;
                                    let edit = egui::TextEdit::singleline(buf)
                                        .font(egui::FontId::new(FONT_SIZE_SM, egui::FontFamily::Proportional))
                                        .desired_width(ui.available_width() - 40.0)
                                        .frame(false);
                                    let resp = ui.add(edit);
                                    resp.request_focus();
                                    let commit = ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                                    let cancel = ui.input(|inp| inp.key_pressed(egui::Key::Escape));
                                    if commit || (!resp.has_focus() && !resp.gained_focus()) {
                                        if let Some((idx, new_name)) = self.renaming_animation.take() {
                                            if !new_name.trim().is_empty() {
                                                self.project.animations[idx].name = new_name.trim().to_string();
                                            }
                                        }
                                    } else if cancel {
                                        self.renaming_animation = None;
                                    }
                                } else {
                                    let name = self.project.animations[i].name.clone();
                                    let label_resp = ui.add(
                                        egui::Label::new(rich(&name, if selected { theme.fg } else { theme.fg_desc }, FONT_SIZE_SM))
                                            .sense(egui::Sense::click()),
                                    );
                                    if label_resp.double_clicked() {
                                        self.renaming_animation = Some((i, name));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add_space(10.0);
                                        ui.label(rich(&format!("{}fps", self.project.animations[i].fps), theme.fg_muted, FONT_SIZE_SM));
                                    });
                                }
                            },
                        );
                    });
                }
            });
    }

    fn draw_anim_toolbar(&mut self, ctx: &egui::Context) {
        if self.ui_state.is_collapsed(Panel::Animations) { return; }

        let theme = self.theme.clone();
        TopBottomPanel::bottom("anim_toolbar")
            .exact_height(TOP_BAR_HEIGHT)
            .frame(Frame::new().fill(theme.panel).inner_margin(Margin::same(0)))
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                    ui.add_space(6.0);

                    // Prev frame
                    {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
                        let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.put(r, Image::new(egui::include_image!("../assets/icons/back.svg")).tint(tint).fit_to_exact_size(Vec2::splat(16.0)));
                        if resp.clicked() {
                            let total = self.project.active_anim().frames.len();
                            if total > 0 {
                                self.project.active_frame = (self.project.active_frame + total - 1) % total;
                                self.canvas_dirty = true;
                            }
                        }
                    }

                    // Play / Pause
                    {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
                        let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.put(r, Image::new(egui::include_image!("../assets/icons/play.svg")).tint(tint).fit_to_exact_size(Vec2::splat(16.0)));
                        if resp.clicked() { self.playback.is_playing = !self.playback.is_playing; }
                    }

                    // Next frame
                    {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
                        let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.put(r, Image::new(egui::include_image!("../assets/icons/forward.svg")).tint(tint).fit_to_exact_size(Vec2::splat(16.0)));
                        if resp.clicked() {
                            let total = self.project.active_anim().frames.len();
                            if total > 0 {
                                self.project.active_frame = (self.project.active_frame + 1) % total;
                                self.canvas_dirty = true;
                            }
                        }
                    }

                    ui.add_space(6.0);

                    // FPS drag — suffix "fps", no separate label
                    let mut fps = self.project.animations[self.project.active_animation].fps as f32;
                    ui.visuals_mut().override_text_color = Some(theme.fg_desc);
                    if ui.add(
                        egui::DragValue::new(&mut fps)
                            .range(1.0..=60.0)
                            .speed(0.5)
                            .suffix(" fps")
                    ).changed() {
                        self.project.animations[self.project.active_animation].fps =
                            fps.round() as u8;
                    }
                    ui.visuals_mut().override_text_color = None;
                });
            });
    }

    fn draw_timeline(&mut self, ctx: &egui::Context) {
        if !self.ui_state.is_visible(Panel::Timeline) {
            return;
        }
        let panel_resp = TopBottomPanel::bottom("timeline")
            .exact_height(104.0)
            .frame(Frame::new().fill(self.theme.bg).inner_margin(Margin { left: 10, right: 10, top: 10, bottom: 0 }))
            .show_separator_line(false)
            .show(ctx, |ui| {
                // Floating scrollbar: invisible at rest, fades in on hover, sits in bottom gap
                ui.style_mut().spacing.scroll = egui::style::ScrollStyle {
                    bar_width: 9.0,
                    ..egui::style::ScrollStyle::floating()
                };
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let frame_count = self.project.active_anim().frames.len();
                        for i in 0..frame_count {
                            let selected = self.project.active_frame == i;
                            let fill = if selected { self.theme.accent } else { self.theme.panel };
                            let response = ui.add(
                                egui::Button::new(self.label_muted(""))
                                    .fill(fill)
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(Vec2::splat(84.0)),
                            );
                            if response.clicked() {
                                self.project.active_frame = i;
                                self.canvas_dirty = true;
                            }
                            if response.secondary_clicked() {
                                self.project.active_frame = i;
                                let menu_outer_w = 144.0;
                                let menu_outer_h = 44.0;
                                let x = response.rect.center().x - menu_outer_w / 2.0;
                                let y = response.rect.top() - menu_outer_h - 4.0;
                    let now = ui.ctx().input(|i| i.time);
                                self.frame_menu = Some((i, Pos2::new(x, y), now));
                                self.canvas_dirty = true;
                            }
                        }
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(84.0), egui::Sense::click());
                        let tint = if resp.hovered() { Color32::WHITE } else { self.theme.fg_desc };
                        if resp.hovered() { ui.painter().rect_filled(r, 0.0, self.theme.accent); }
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "+", FontId::new(22.0, FontFamily::Proportional), tint);
                        if resp.clicked() { self.add_frame(); }
                    });
                });
            });

        // Mouse scroll wheel over timeline → navigate frames
        let timeline_rect = panel_resp.response.rect;
        let hovered = ctx.pointer_hover_pos().map(|p| timeline_rect.contains(p)).unwrap_or(false);
        if hovered {
            let delta = ctx.input(|i| i.raw_scroll_delta.y);
            self.timeline_scroll_accum += delta;
            let total = self.project.active_anim().frames.len();
            if total > 0 {
                while self.timeline_scroll_accum > 30.0 {
                    self.timeline_scroll_accum -= 30.0;
                    self.project.active_frame = (self.project.active_frame + total - 1) % total;
                    self.canvas_dirty = true;
                }
                while self.timeline_scroll_accum < -30.0 {
                    self.timeline_scroll_accum += 30.0;
                    self.project.active_frame = (self.project.active_frame + 1) % total;
                    self.canvas_dirty = true;
                }
            }
        } else {
            self.timeline_scroll_accum = 0.0;
        }

        self.draw_frame_menu(ctx);
    }

    fn draw_layer_context_menu(&mut self, ctx: &egui::Context) {
        let Some((idx, pos, opened_at)) = self.layer_ctx_menu else { return; };
        let ai = self.project.active_animation;
        let fi = self.project.active_frame;
        let layer_count = self.project.animations[ai].frames[fi].layers.len();
        if idx >= layer_count { self.layer_ctx_menu = None; return; }

        let theme = self.theme.clone();
        let can_merge = idx > 0;
        let can_delete = layer_count > 1;

        const BTN: f32 = 36.0;
        const PAD: f32 = 4.0;
        const MENU_W: f32 = BTN * 3.0 + PAD * 4.0;
        const MENU_H: f32 = BTN + PAD * 2.0;

        let mut action: Option<u8> = None;
        let inner = egui::Area::new(egui::Id::new("layer_ctx_menu"))
            .fixed_pos(Pos2::new(pos.x - MENU_W, pos.y - MENU_H))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme.panel)
                    .inner_margin(egui::Margin::same(4))
                    .shadow(egui::Shadow {
                        offset: [0, 14],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                    })
                    .show(ui, |ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::ZERO;

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                            // Duplicate
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                            if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                            let icon_rect = egui::Rect::from_center_size(r.center(), Vec2::splat(20.0));
                            let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                            ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/duplicate.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                            if resp.clicked() { action = Some(0); }

                            ui.add_space(PAD);

                            // Merge Down
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), if can_merge { egui::Sense::click() } else { egui::Sense::hover() });
                            if can_merge && resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                            let icon_rect = egui::Rect::from_center_size(r.center(), Vec2::splat(20.0));
                            let merge_tint = if !can_merge { theme.fg_muted } else if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                            ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/merge.svg")).tint(merge_tint).fit_to_exact_size(Vec2::splat(20.0)));
                            if resp.clicked() { action = Some(1); }

                            ui.add_space(PAD);

                            // Delete
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), if can_delete { egui::Sense::click() } else { egui::Sense::hover() });
                            if can_delete && resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                            let icon_rect = egui::Rect::from_center_size(r.center(), Vec2::splat(20.0));
                            let delete_tint = if !can_delete { theme.fg_muted } else if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                            ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/delete.svg")).tint(delete_tint).fit_to_exact_size(Vec2::splat(20.0)));
                            if resp.clicked() { action = Some(2); }
                        });
                    });
            });

        // Auto-close after 2s hover-away
        let menu_id = egui::Id::new("layer_ctx_menu_timer");
        let now = ctx.input(|i| i.time);
        let is_hovered = ctx.pointer_hover_pos()
            .map(|p| inner.response.rect.contains(p))
            .unwrap_or(false);
        let should_close_timer = ctx.data_mut(|d| {
            let last: &mut f64 = d.get_temp_mut_or_insert_with(menu_id, || now);
            if is_hovered { *last = now; false } else { now - *last > 2.0 }
        });
        ctx.request_repaint();

        // Close on click outside — guarded: ignore clicks within 0.15s of opening
        // (the right-click that opened the menu is still "any_click" on the same frame)
        let age = now - opened_at;
        let clicked_outside = age > 0.15 && ctx.input(|i| i.pointer.any_click()) && !is_hovered;

        if action.is_some() || should_close_timer || clicked_outside {
            self.layer_ctx_menu = None;
        }

        if let Some(a) = action {
            let layers = &mut self.project.animations[ai].frames[fi].layers;
            match a {
                0 => {
                    let mut copy = layers[idx].clone();
                    copy.name = format!("{} copy", copy.name);
                    layers.insert(idx + 1, copy);
                    self.project.active_layer = idx + 1;
                    self.canvas_dirty = true;
                }
                1 => {
                    let top = layers[idx].clone();
                    let bottom = &mut layers[idx - 1];
                    let pixel_count = (bottom.width * bottom.height) as usize;
                    for i in 0..pixel_count {
                        let sa = top.pixels[i * 4 + 3] as f32 / 255.0;
                        if sa > 0.0 {
                            let da = bottom.pixels[i * 4 + 3] as f32 / 255.0;
                            let out_a = sa + da * (1.0 - sa);
                            if out_a > 0.0 {
                                for c in 0..3 {
                                    let sc = top.pixels[i * 4 + c] as f32 / 255.0;
                                    let dc = bottom.pixels[i * 4 + c] as f32 / 255.0;
                                    bottom.pixels[i * 4 + c] =
                                        ((sc * sa + dc * da * (1.0 - sa)) / out_a * 255.0).round() as u8;
                                }
                                bottom.pixels[i * 4 + 3] = (out_a * 255.0).round() as u8;
                            }
                        }
                    }
                    layers.remove(idx);
                    self.project.active_layer = self.project.active_layer.min(layers.len().saturating_sub(1));
                    self.canvas_dirty = true;
                }
                2 => {
                    layers.remove(idx);
                    self.project.active_layer = self.project.active_layer.min(layers.len().saturating_sub(1));
                    self.canvas_dirty = true;
                }
                _ => {}
            }
        }
    }

    fn draw_frame_menu(&mut self, ctx: &egui::Context) {
        let Some((frame_index, pos, opened_at)) = self.frame_menu else { return; };
        if frame_index >= self.project.active_anim().frames.len() {
            self.frame_menu = None;
            return;
        }
        let inner = egui::Area::new(egui::Id::new("frame_context_menu"))
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(self.theme.panel)
                    .inner_margin(Margin::same(4))
                    .shadow(egui::Shadow {
                        offset: [0, 14],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                    })
                    .show(ui, |ui| {
                    // Single row: [DragValue ms] [duplicate] [delete]
                    const BTN: f32 = 36.0;
                    const PAD: f32 = 4.0;
                    let theme = self.theme.clone();
                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                        // DragValue for duration — first
                        let duration = &mut self.project.active_anim_mut().frames[frame_index].duration_ms;
                        let mut d = if *duration == 0 { 100 } else { *duration };
                        ui.visuals_mut().override_text_color = Some(theme.fg_desc);
                        ui.add_sized(
                            Vec2::new(52.0, BTN),
                            egui::DragValue::new(&mut d).range(10..=5000).suffix("ms"),
                        );
                        ui.visuals_mut().override_text_color = None;
                        if d != *duration { *duration = d; }
                        // Duplicate
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                        if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                        let icon_rect = egui::Rect::from_center_size(r.center(), Vec2::splat(20.0));
                        let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/duplicate.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                        if resp.clicked() {
                            self.project.active_frame = frame_index;
                            self.duplicate_active_frame();
                            self.frame_menu = None;
                        }
                        ui.add_space(PAD);
                        // Delete
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(BTN), egui::Sense::click());
                        if resp.hovered() { ui.painter().rect_filled(r, 0.0, theme.accent); }
                        let icon_rect = egui::Rect::from_center_size(r.center(), Vec2::splat(20.0));
                        let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/delete.svg")).tint(tint).fit_to_exact_size(Vec2::splat(20.0)));
                        if resp.clicked() {
                            self.project.active_frame = frame_index;
                            self.delete_active_frame();
                            self.frame_menu = None;
                        }
                    });
                });
            });

        // Auto-close after 2s hover-away
        let menu_id = egui::Id::new("frame_menu_timer");
        let now = ctx.input(|i| i.time);
        let age = now - opened_at;
        let is_hovered = ctx.pointer_hover_pos()
            .map(|p| inner.response.rect.contains(p))
            .unwrap_or(false);
        let should_close_timer = ctx.data_mut(|d| {
            let last: &mut f64 = d.get_temp_mut_or_insert_with(menu_id, || now);
            if age < 0.15 || is_hovered { *last = now; false } else { now - *last > 2.0 }
        });
        ctx.request_repaint();

        // Close on click outside — guarded: ignore clicks within 0.15s of opening
        let clicked_outside = age > 0.15 && ctx.input(|i| i.pointer.any_click()) && !is_hovered;

        if should_close_timer || clicked_outside {
            self.frame_menu = None;
        }
    }

    // Ramp Lab removed: draw_ramp_lab function deleted.

    // Ramp Lab removed: preview_ramp_rgba deleted.


    fn draw_workspace(&mut self, ctx: &egui::Context) {
        CentralPanel::default()
            .frame(Frame::new().fill(self.theme.bg))
            .show(ctx, |ui| {
                if self.canvas_dirty {
                    self.rebuild_canvas_texture(ctx);
                }
                let canvas_rect = ui.available_rect_before_wrap();
                if self.pending_zoom_fit {
                    self.canvas.zoom_to_fit(canvas_rect, self.project.canvas_width, self.project.canvas_height);
                    self.pending_zoom_fit = false;
                }
                let painter = ui.painter_at(canvas_rect);
                self.canvas.draw(
                    &painter,
                    canvas_rect,
                    self.project.canvas_width,
                    self.project.canvas_height,
                    &self.theme,
                );
                self.canvas.handle_input(ui, canvas_rect);

                let art_rect = self.canvas.art_rect(canvas_rect, self.project.canvas_width, self.project.canvas_height);
                painter.rect_stroke(
                    art_rect,
                    0.0,
                    egui::Stroke::new(1.0, self.theme.muted),
                    egui::StrokeKind::Outside,
                );

                // Selection rect overlay (during initial marquee drag)
                if matches!(self.active_tool, ActiveTool::RectSelect) {
                    if let Some((rx, ry, rw, rh)) = self.select_state.rect {
                        let zoom = self.canvas.zoom;
                        let sel_min = egui::Pos2::new(
                            art_rect.min.x + rx as f32 * zoom,
                            art_rect.min.y + ry as f32 * zoom,
                        );
                        let sel_max = egui::Pos2::new(
                            art_rect.min.x + (rx + rw) as f32 * zoom,
                            art_rect.min.y + (ry + rh) as f32 * zoom,
                        );
                        let sel_rect = egui::Rect::from_min_max(sel_min, sel_max);
                        painter.rect_stroke(sel_rect, 0.0, egui::Stroke::new(2.0, egui::Color32::from_black_alpha(120)), egui::StrokeKind::Outside);
                        painter.rect_stroke(sel_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::WHITE), egui::StrokeKind::Outside);
                    }

                    // Floating selection overlay: corners + handles + rotation stem.
                    if self.select_state.has_float() {
                        let zoom = self.canvas.zoom;
                        let to_screen = |(x, y): (f32, f32)| egui::Pos2::new(
                            art_rect.min.x + x * zoom,
                            art_rect.min.y + y * zoom,
                        );
                        if let Some(corners) = self.select_state.rotated_corners() {
                            let pts: Vec<egui::Pos2> = corners.iter().map(|&c| to_screen(c)).collect();
                            // Outline (shadow + white)
                            for i in 0..4 {
                                let a = pts[i];
                                let b = pts[(i + 1) % 4];
                                painter.line_segment([a, b], egui::Stroke::new(2.0, egui::Color32::from_black_alpha(120)));
                                painter.line_segment([a, b], egui::Stroke::new(1.0, egui::Color32::WHITE));
                            }
                        }
                        if let Some(handles) = self.selection_handle_positions() {
                            // Helper: find a handle's screen position by variant.
                            let find = |target: Handle| handles.iter()
                                .find(|(h, _)| *h == target)
                                .map(|(_, p)| to_screen(*p));

                            // Stems: N→Rotate, W→FlipH, S→FlipV
                            for (from, to) in [(Handle::N, Handle::Rotate), (Handle::W, Handle::FlipH), (Handle::S, Handle::FlipV)] {
                                if let (Some(a), Some(b)) = (find(from), find(to)) {
                                    painter.line_segment([a, b], egui::Stroke::new(2.0, egui::Color32::from_black_alpha(120)));
                                    painter.line_segment([a, b], egui::Stroke::new(1.0, egui::Color32::WHITE));
                                }
                            }

                            // Handles
                            for (h, p) in handles {
                                let center = to_screen(p);
                                let size = if matches!(h, Handle::Rotate | Handle::FlipH | Handle::FlipV) { 5.0 } else { 4.0 };
                                let hr = egui::Rect::from_center_size(center, egui::Vec2::splat(size * 2.0));
                                painter.rect_filled(hr, 1.0, egui::Color32::WHITE);
                                painter.rect_stroke(hr, 1.0, egui::Stroke::new(1.0, egui::Color32::BLACK), egui::StrokeKind::Outside);
                            }
                        }
                    }
                }

                let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());
                if self.active_tool == ActiveTool::Zoom {
                    self.handle_zoom_tool_input(&response, canvas_rect);
                } else {
                    self.handle_canvas_input(response, canvas_rect);
                }
            });
    }

    fn draw_preview_section(&mut self, ui: &mut egui::Ui) {
        if !self.ui_state.is_visible(Panel::Preview) { return; }

        let collapsed = self.ui_state.is_collapsed(Panel::Preview);
        let theme = self.theme.clone();
        // Header — same visual as section_header but without the "+" button.
        Frame::new().fill(theme.panel).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), egui::Sense::hover());
            let icon_size = Vec2::splat(16.0);
            let icon_rect = egui::Rect::from_center_size(
                egui::pos2(rect.left() + 8.0, rect.center().y), icon_size,
            );
            let icon_resp = ui.interact(icon_rect, egui::Id::new("hdr_icon_preview"), egui::Sense::click());
            let tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
            ui.put(icon_rect, egui::Image::new(egui::include_image!("../assets/icons/visibility.svg"))
                .tint(tint)
                .fit_to_exact_size(icon_size));
            if icon_resp.clicked() {
                self.ui_state.toggle_collapsed(Panel::Preview);
            }
        });

        if collapsed { return; }

        let pixels = self.composite_active_frame();
        let tex = ui.ctx().load_texture(
            "preview_sidebar",
            egui::ColorImage::from_rgba_unmultiplied(
                [self.project.canvas_width as usize, self.project.canvas_height as usize],
                &pixels,
            ),
            egui::TextureOptions::NEAREST,
        );
        let theme = self.theme.clone();
        Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
            let avail = ui.available_width();
            let cw = self.project.canvas_width as f32;
            let ch = self.project.canvas_height as f32;
            let aspect = cw / ch;
            let (pw, ph) = if aspect >= 1.0 {
                (avail, avail / aspect)
            } else {
                (avail * aspect, avail)
            };
            let (rect, _) = ui.allocate_exact_size(Vec2::new(pw, ph), egui::Sense::hover());
            ui.painter().image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        });
    }

    fn handle_zoom_tool_input(&mut self, response: &egui::Response, canvas_rect: egui::Rect) {
        let w = self.project.canvas_width;
        let h = self.project.canvas_height;

        // Left click — manual double-click detection (response.double_clicked() is unreliable
        // because the first click may change canvas state, making egui lose the widget continuity)
        if response.clicked() {
            let now = response.ctx.input(|i| i.time);
            let since_last = now - self.last_zoom_click_time;
            if since_last < 0.4 {
                // Double-click → zoom to fit
                self.canvas.zoom_to_fit(canvas_rect, w, h);
                self.last_zoom_click_time = -1.0; // reset so triple-click doesn't keep fitting
            } else {
                // Single click → zoom in 1.5× at cursor
                if let Some(pos) = response.interact_pointer_pos() {
                    self.canvas.zoom_at_point(1.5, pos, canvas_rect);
                }
                self.last_zoom_click_time = now;
            }
            return;
        }

        // Right click → zoom out at cursor
        let right_clicked = response.ctx.input(|i| {
            i.pointer.secondary_clicked()
                && i.pointer.hover_pos().map(|p| canvas_rect.contains(p)).unwrap_or(false)
        });
        if right_clicked {
            let pos = response.ctx.input(|i| i.pointer.hover_pos().unwrap_or(canvas_rect.center()));
            self.canvas.zoom_at_point(1.0 / 1.5, pos, canvas_rect);
        }
    }

    /// Check if the last three pixels in the stroke form an L-shape inside a 2x2 area.
    /// If so, remove the middle (corner) pixel from the layer, stroke_edits, stroke_painted,
    /// and stroke_pixel_sequence. Repeats until no more L-shapes at the tail.
    fn check_and_remove_l_shape(&mut self, ai: usize, fi: usize, li: usize) {
        while self.stroke_pixel_sequence.len() >= 3 {
            let n = self.stroke_pixel_sequence.len();
            let a = self.stroke_pixel_sequence[n - 3];
            let b = self.stroke_pixel_sequence[n - 2];
            let c = self.stroke_pixel_sequence[n - 1];

            // a and c must be diagonal neighbors (dx=1, dy=1)
            let dx = (a.0 as i32 - c.0 as i32).abs();
            let dy = (a.1 as i32 - c.1 as i32).abs();
            if dx != 1 || dy != 1 {
                break;
            }

            // b must be inside the 2x2 bounding box of a and c, and not equal to a or c
            let min_x = a.0.min(c.0);
            let max_x = a.0.max(c.0);
            let min_y = a.1.min(c.1);
            let max_y = a.1.max(c.1);
            if b.0 < min_x || b.0 > max_x || b.1 < min_y || b.1 > max_y || b == a || b == c {
                break;
            }

            // L-shape found: remove b (the corner pixel)
            if let Some(idx) = self.stroke_edits.iter().position(|(x, y, _, _)| *x == b.0 && *y == b.1) {
                let (_, _, old_color, _) = self.stroke_edits.remove(idx);
                self.project.animations[ai].frames[fi].layers[li].set_pixel(b.0, b.1, old_color);
                self.canvas_dirty = true;
            }
            self.stroke_painted.remove(&b);
            self.stroke_pixel_sequence.remove(n - 2);
        }
    }

    fn handle_canvas_input(&mut self, response: egui::Response, canvas_rect: egui::Rect) {
        let w = self.project.canvas_width;
        let h = self.project.canvas_height;
        let ai = self.project.active_animation;
        let fi = self.project.active_frame;
        let li = self.project.active_layer;

        let is_shape_tool = matches!(self.active_tool,
            ActiveTool::Rectangle { .. } | ActiveTool::Ellipse { .. } | ActiveTool::Line);
        let is_select_tool = matches!(self.active_tool, ActiveTool::RectSelect);

        // Floating selection: if active, the RectSelect tool routes input to the
        // transform handler. Returns true if input was consumed by a handle/move/rotate.
        if is_select_tool && self.select_state.has_float() {
            if self.handle_selection_transform(&response, canvas_rect) {
                return;
            }
            // If the user starts a drag outside the selection, handle_selection_transform
            // commits the float and falls through here so we can start a new marquee.
        }

        // --- Resolve current pointer position ---
        // For shape drags: use the global interact_pos so tracking continues even when the
        // mouse leaves the central panel. Fall back to hover_pos (covers the release frame
        // where interact_pos may be None).
        // For stroke tools: require the pointer to be inside the canvas widget.
        let primary_down = response.ctx.input(|i| i.pointer.primary_down());
        let pos_opt: Option<egui::Pos2> = if (is_shape_tool || is_select_tool) && self.drag_start.is_some() {
            response.ctx.input(|i| i.pointer.latest_pos())
        } else {
            response.interact_pointer_pos()
        };

        // --- drag_stopped must run even when pos is None (release outside window) ---
        // Also trigger commit when primary button is released globally but drag_stopped
        // didn't fire because the cursor left the central panel.
        let should_commit = response.drag_stopped()
            || ((is_shape_tool || is_select_tool) && self.drag_start.is_some() && !primary_down);
        if should_commit {
            let color = self.color_state.foreground;
            self.shape_preview.clear();
            if !self.project.animations[ai].frames[fi].layers[li].locked
                && !self.project.animations[ai].frames[fi].layers[li].is_group
            {
                if let (Some((x0, y0)), Some(pos)) = (self.drag_start, pos_opt) {
                    let (epx, epy) = self.canvas.screen_to_canvas_i32(pos, canvas_rect, w, h);
                    let shift_commit = response.ctx.input(|i| i.modifiers.shift);
                    let active_tool = self.active_tool.clone();
                    let (eff_epx, eff_epy) = if shift_commit {
                        shape_shift_constrain(&active_tool, x0 as i32, y0 as i32, epx, epy)
                    } else {
                        (epx, epy)
                    };
                    let shape_edits: Vec<_> = match &active_tool {
                        ActiveTool::Rectangle { filled } => {
                            apply_rect(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, eff_epx, eff_epy, color, *filled)
                        }
                        ActiveTool::Ellipse { filled } => {
                            let cx = (x0 as i32 + eff_epx) / 2;
                            let cy = (y0 as i32 + eff_epy) / 2;
                            let rx = (eff_epx - x0 as i32).abs() / 2;
                            let ry = (eff_epy - y0 as i32).abs() / 2;
                            apply_ellipse(&self.project.animations[ai].frames[fi].layers[li], cx, cy, rx, ry, color, *filled)
                        }
                        ActiveTool::Line => {
                            apply_line(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, eff_epx, eff_epy, color)
                        }
                        _ => vec![],
                    };
                    if !shape_edits.is_empty() {
                        for &(x, y, _old, new) in &shape_edits {
                            self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                        }
                        self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits: shape_edits });
                        self.canvas_dirty = true;
                    }
                }
            }
            // Stroke tools: commit accumulated edits
            if !self.stroke_edits.is_empty() {
                let edits = std::mem::take(&mut self.stroke_edits);
                self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
            }
            self.last_pencil_pos = None;
            self.stroke_painted.clear();
            self.stroke_pixel_sequence.clear();
            // RectSelect: lift selected pixels into a floating buffer.
            if is_select_tool {
                if let Some(rect) = self.select_state.rect {
                    if rect.2 > 0 && rect.3 > 0 {
                        self.lift_selection_to_float(rect);
                    } else {
                        self.select_state.clear();
                    }
                }
            }
            self.drag_start = None;
            return;
        }

        // --- For everything else we need a valid position ---
        let pos = match pos_opt {
            Some(p) => p,
            None => return,
        };

        // Unconstrained i32 canvas coordinates for shape tools — can be negative or
        // beyond canvas edges.  Pixels that fall outside are discarded by get_pixel/set_pixel.
        let (shape_px, shape_py): (i32, i32) = self.canvas.screen_to_canvas_i32(pos, canvas_rect, w, h);
        let shift_held = response.ctx.input(|i| i.modifiers.shift);

        // Stroke tools: require cursor to be inside the canvas.
        // For shape tools with an active drag the cursor may be outside — in that case we
        // skip the bounds check and only use shape_px/shape_py for the shape preview/commit.
        let (px, py) = if (is_shape_tool || is_select_tool) && self.drag_start.is_some() {
            // Already have shape_px/shape_py; (px, py) unused for shape mid-drag arms.
            (0u32, 0u32)
        } else {
            let Some((px, py)) = self.canvas.screen_to_canvas(pos, canvas_rect, w, h) else { return; };
            if px >= w || py >= h { return; }
            (px, py)
        };

        // Selection is non-destructive — allow it even on locked/group layers.
        if !is_select_tool {
            if self.project.animations[ai].frames[fi].layers[li].locked { return; }
            if self.project.animations[ai].frames[fi].layers[li].is_group { return; }
        }

        if response.drag_started() {
            // For select tool, clamp the drag-start to canvas bounds so that
            // starting a marquee outside the canvas still begins from the edge.
            let start = if is_select_tool {
                let sx = shape_px.clamp(0, w as i32 - 1) as u32;
                let sy = shape_py.clamp(0, h as i32 - 1) as u32;
                (sx, sy)
            } else {
                (px, py)
            };
            self.drag_start = Some(start);
            self.stroke_edits.clear();
            self.shape_preview.clear();
            self.last_pencil_pos = None;
            self.stroke_painted.clear();
            self.stroke_pixel_sequence.clear();
            if is_select_tool {
                self.select_state.rect = None; // clear previous selection on new drag
            }
        }

        let color = self.color_state.foreground;
        match &self.active_tool.clone() {
            ActiveTool::Pencil => {
                let positions = if let Some((lx, ly)) = self.last_pencil_pos {
                    bresenham_positions(lx as i32, ly as i32, px as i32, py as i32)
                } else {
                    vec![(px, py)]
                };
                for pos in positions {
                    if self.stroke_painted.contains(&pos) {
                        continue;
                    }
                    self.stroke_pixel_sequence.push(pos);
                    let edits = apply_pencil(&self.project.animations[ai].frames[fi].layers[li], pos.0, pos.1, color);
                    for &(x, y, old, new) in &edits {
                        self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                        self.stroke_edits.push((x, y, old, new));
                    }
                    self.stroke_painted.insert(pos);
                    self.check_and_remove_l_shape(ai, fi, li);
                }
                self.last_pencil_pos = Some((px, py));
                self.canvas_dirty = true;
            }
            ActiveTool::Eraser => {
                let positions = if let Some((lx, ly)) = self.last_pencil_pos {
                    bresenham_positions(lx as i32, ly as i32, px as i32, py as i32)
                } else {
                    vec![(px, py)]
                };
                for pos in positions {
                    if self.stroke_painted.contains(&pos) {
                        continue;
                    }
                    self.stroke_pixel_sequence.push(pos);
                    let edits = apply_eraser(&self.project.animations[ai].frames[fi].layers[li], pos.0, pos.1);
                    for &(x, y, old, new) in &edits {
                        self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, new);
                        self.stroke_edits.push((x, y, old, new));
                    }
                    self.stroke_painted.insert(pos);
                    self.check_and_remove_l_shape(ai, fi, li);
                }
                self.last_pencil_pos = Some((px, py));
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
                let picked = apply_eyedropper(layer, px, py);
                self.color_state.foreground = picked;
                sync_color_caches(&mut self.color_state);
            }
            ActiveTool::RectSelect => {
                // Handled below in the live-drag block.
            }
            _ => {}
        }

        // Shape tools: recompute preview every frame while button is held and drag is active.
        // Use primary_down (global button state) instead of response.dragged() so the
        // preview keeps updating even when the cursor moves outside the central panel.
        if is_shape_tool && self.drag_start.is_some() && primary_down {
            response.ctx.request_repaint();
            if let Some((x0, y0)) = self.drag_start {
                let active_tool = self.active_tool.clone();
                let (eff_px, eff_py) = if shift_held {
                    shape_shift_constrain(&active_tool, x0 as i32, y0 as i32, shape_px, shape_py)
                } else {
                    (shape_px, shape_py)
                };
                let preview_edits: Vec<_> = match &active_tool {
                    ActiveTool::Rectangle { filled } => {
                        apply_rect(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, eff_px, eff_py, color, *filled)
                    }
                    ActiveTool::Ellipse { filled } => {
                        let cx = (x0 as i32 + eff_px) / 2;
                        let cy = (y0 as i32 + eff_py) / 2;
                        let rx = (eff_px - x0 as i32).abs() / 2;
                        let ry = (eff_py - y0 as i32).abs() / 2;
                        apply_ellipse(&self.project.animations[ai].frames[fi].layers[li], cx, cy, rx, ry, color, *filled)
                    }
                    ActiveTool::Line => {
                        apply_line(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, eff_px, eff_py, color)
                    }
                    _ => vec![],
                };
                self.shape_preview = preview_edits.into_iter().map(|(x, y, _old, new)| (x, y, new)).collect();
                self.canvas_dirty = true;
            }
        }

        // Selection rect: update live during drag.
        if is_select_tool && self.drag_start.is_some() && primary_down {
            response.ctx.request_repaint();
            if let Some((x0, y0)) = self.drag_start {
                let ex = shape_px.clamp(0, w as i32 - 1) as u32;
                let ey = shape_py.clamp(0, h as i32 - 1) as u32;
                let (rx, ry) = (x0.min(ex), y0.min(ey));
                let (rw, rh) = (x0.max(ex) - rx + 1, y0.max(ey) - ry + 1);
                self.select_state.rect = Some((rx, ry, rw, rh));
            }
        }
    }

    /// Lift the pixels inside `rect` from the active layer into a FloatBuffer,
    /// clearing those cells in the layer. Records an undo command.
    fn lift_selection_to_float(&mut self, rect: (u32, u32, u32, u32)) {
        let (rx, ry, rw, rh) = rect;
        if rw == 0 || rh == 0 { return; }
        let ai = self.project.active_animation;
        let fi = self.project.active_frame;
        let li = self.project.active_layer;
        if self.project.animations[ai].frames[fi].layers[li].locked { return; }
        if self.project.animations[ai].frames[fi].layers[li].is_group { return; }

        let layer = &self.project.animations[ai].frames[fi].layers[li];
        let mut pixels: Vec<Rgba> = Vec::with_capacity((rw * rh) as usize);
        let mut edits: Vec<crate::tools::PixelEdit> = Vec::new();
        for y in 0..rh {
            for x in 0..rw {
                let cx = rx + x;
                let cy = ry + y;
                let old = layer.get_pixel(cx, cy);
                pixels.push(old);
                if old[3] != 0 {
                    edits.push((cx, cy, old, [0, 0, 0, 0]));
                }
            }
        }
        // Apply the clear
        for &(x, y, _o, n) in &edits {
            self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, n);
        }
        if !edits.is_empty() {
            self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
        }
        self.select_state.begin_float(FloatBuffer { w: rw, h: rh, pixels }, rect);
        self.canvas_dirty = true;
    }

    /// Stamp the currently transformed float pixels onto the active layer and
    /// clear the selection state. Records an undo command.
    fn commit_float_to_layer(&mut self) {
        if !self.select_state.has_float() { return; }
        let ai = self.project.active_animation;
        let fi = self.project.active_frame;
        let li = self.project.active_layer;
        if self.project.animations[ai].frames[fi].layers[li].locked {
            // Cannot commit; just drop the float (destructive but unavoidable).
            self.select_state.clear();
            return;
        }
        if self.project.animations[ai].frames[fi].layers[li].is_group {
            self.select_state.clear();
            return;
        }

        let Some((ax, ay, aw, ah)) = self.select_state.transformed_aabb() else {
            self.select_state.clear();
            return;
        };
        let w = self.project.canvas_width as i32;
        let h = self.project.canvas_height as i32;
        let x0 = (ax.floor() as i32).max(0);
        let y0 = (ay.floor() as i32).max(0);
        let x1 = ((ax + aw).ceil() as i32).min(w);
        let y1 = ((ay + ah).ceil() as i32).min(h);

        let layer = &self.project.animations[ai].frames[fi].layers[li];
        let mut edits: Vec<crate::tools::PixelEdit> = Vec::new();
        for cy in y0..y1 {
            for cx in x0..x1 {
                if let Some(new) = sample_transformed(&self.select_state, cx, cy) {
                    let old = layer.get_pixel(cx as u32, cy as u32);
                    if old != new {
                        edits.push((cx as u32, cy as u32, old, new));
                    }
                }
            }
        }
        for &(x, y, _o, n) in &edits {
            self.project.animations[ai].frames[fi].layers[li].set_pixel(x, y, n);
        }
        if !edits.is_empty() {
            self.undo_stack.push(Command::PaintPixels { animation_id: ai, frame_id: fi, layer_id: li, edits });
        }
        self.select_state.clear();
        self.canvas_dirty = true;
    }

    /// Returns the canvas-space pixel position of each handle for the current
    /// (rotated, scaled) selection. Layout (in unrotated local coords):
    ///   NW N NE
    ///   W     E
    ///   SW S SE
    /// Plus a rotation handle 18 canvas-pixels above the N handle (along the
    /// "up" axis of the rotated rect).
    fn selection_handle_positions(&self) -> Option<[(Handle, (f32, f32)); 11]> {
        let (w0, h0) = self.select_state.float_size()?;
        let sw = w0 as f32 * self.select_state.scale.0.abs();
        let sh = h0 as f32 * self.select_state.scale.1.abs();
        let (ox, oy) = self.select_state.offset;
        let cx = ox + sw * 0.5;
        let cy = oy + sh * 0.5;
        let (s, c) = self.select_state.rotation.sin_cos();
        let map = |lx: f32, ly: f32| -> (f32, f32) {
            let dx = lx - cx;
            let dy = ly - cy;
            (cx + dx * c - dy * s, cy + dx * s + dy * c)
        };
        // Rotation + flip handle offsets: 18 canvas-pixels outside the rect (local space).
        let off = 18.0 / self.canvas.zoom.max(0.0001);
        Some([
            (Handle::NW, map(ox,        oy)),
            (Handle::N,  map(ox + sw/2., oy)),
            (Handle::NE, map(ox + sw,    oy)),
            (Handle::W,  map(ox,         oy + sh/2.)),
            (Handle::E,  map(ox + sw,    oy + sh/2.)),
            (Handle::SW, map(ox,         oy + sh)),
            (Handle::S,  map(ox + sw/2., oy + sh)),
            (Handle::SE, map(ox + sw,    oy + sh)),
            (Handle::Rotate, map(ox + sw/2., oy - off)),
            (Handle::FlipH,  map(ox - off,    oy + sh/2.)),
            (Handle::FlipV,  map(ox + sw/2.,  oy + sh + off)),
        ])
    }

    /// Hit-test handles, rotation stem, and inside-rect at a given canvas
    /// pixel position. Returns the Handle that was hit, if any.
    fn hit_test_selection(&self, cx_px: f32, cy_px: f32) -> Option<Handle> {
        let handles = self.selection_handle_positions()?;
        // Handle hit radius — 16 screen pixels expressed in canvas space.
        let r = 16.0 / self.canvas.zoom.max(0.0001);
        for (h, (hx, hy)) in handles {
            let dx = cx_px - hx;
            let dy = cy_px - hy;
            if dx*dx + dy*dy <= r*r {
                return Some(h);
            }
        }
        // Inside test (rotated rect): inverse-rotate the point and check vs local rect.
        let (w0, h0) = self.select_state.float_size()?;
        let sw = w0 as f32 * self.select_state.scale.0.abs();
        let sh = h0 as f32 * self.select_state.scale.1.abs();
        let (ox, oy) = self.select_state.offset;
        let cx = ox + sw * 0.5;
        let cy = oy + sh * 0.5;
        let (s, c) = (-self.select_state.rotation).sin_cos();
        let dx = cx_px - cx;
        let dy = cy_px - cy;
        let lx = cx + dx * c - dy * s;
        let ly = cy + dx * s + dy * c;
        if lx >= ox && lx <= ox + sw && ly >= oy && ly <= oy + sh {
            return Some(Handle::Inside);
        }
        None
    }

    /// Handle move/resize/rotate of an active floating selection.
    /// Returns true if the event was consumed (caller should skip normal input).
    fn handle_selection_transform(&mut self, response: &egui::Response, canvas_rect: egui::Rect) -> bool {
        if !self.select_state.has_float() { return false; }
        if !matches!(self.active_tool, ActiveTool::RectSelect) { return false; }

        let primary_down = response.ctx.input(|i| i.pointer.primary_down());
        let shift_held = response.ctx.input(|i| i.modifiers.shift);
        let pos_opt: Option<egui::Pos2> = if self.select_state.interaction != SelectInteraction::None {
            response.ctx.input(|i| i.pointer.latest_pos())
        } else {
            response.hover_pos()
        };
        let Some(pos) = pos_opt else {
            // Drag may have ended off-window — commit interaction end on release
            if !primary_down && self.select_state.interaction != SelectInteraction::None {
                self.select_state.interaction = SelectInteraction::None;
                self.select_state.drag_anchor = None;
            }
            return false;
        };

        let w = self.project.canvas_width;
        let h = self.project.canvas_height;
        let (cx_px, cy_px) = self.canvas.screen_to_canvas_f32(pos, canvas_rect, w, h);

        // Click on flip handles → mirror (no drag).
        if response.clicked() {
            if let Some(handle) = self.hit_test_selection(cx_px, cy_px) {
                match handle {
                    Handle::FlipH => {
                        self.select_state.scale.0 = -self.select_state.scale.0;
                        self.canvas_dirty = true;
                        return true;
                    }
                    Handle::FlipV => {
                        self.select_state.scale.1 = -self.select_state.scale.1;
                        self.canvas_dirty = true;
                        return true;
                    }
                    _ => {}
                }
            }
        }

        // Start an interaction on drag_started inside a handle or the rect.
        if response.drag_started() {
            if let Some(handle) = self.hit_test_selection(cx_px, cy_px) {
                // Flip handles are click-only; pressing-and-dragging on them does nothing.
                if matches!(handle, Handle::FlipH | Handle::FlipV) {
                    return true;
                }
                let interaction = match handle {
                    Handle::Inside => SelectInteraction::Moving,
                    Handle::Rotate => SelectInteraction::Rotating,
                    h => SelectInteraction::Resizing(h),
                };
                self.select_state.interaction = interaction;
                self.select_state.drag_anchor = Some(DragAnchor {
                    mouse_x: cx_px,
                    mouse_y: cy_px,
                    offset: self.select_state.offset,
                    scale: self.select_state.scale,
                    rotation: self.select_state.rotation,
                });
                self.canvas_dirty = true;
                return true;
            } else {
                // Drag started outside the selection — commit current float and let the
                // normal RectSelect handler start a new marquee.
                self.commit_float_to_layer();
                return false;
            }
        }

        // While dragging, update transform
        if self.select_state.interaction != SelectInteraction::None && primary_down {
            response.ctx.request_repaint();
            if let Some(anchor) = self.select_state.drag_anchor {
                let dx = cx_px - anchor.mouse_x;
                let dy = cy_px - anchor.mouse_y;
                match self.select_state.interaction {
                    SelectInteraction::Moving => {
                        self.select_state.offset = (anchor.offset.0 + dx, anchor.offset.1 + dy);
                    }
                    SelectInteraction::Resizing(handle) => {
                        if let Some((w0, h0)) = self.select_state.float_size() {
                            let sw0 = w0 as f32 * anchor.scale.0.abs();
                            let sh0 = h0 as f32 * anchor.scale.1.abs();
                            // We treat resize as adjusting the AABB edges in local
                            // (un-rotated) space. Project the mouse delta into that
                            // space using the inverse rotation.
                            let (s, c) = (-anchor.rotation).sin_cos();
                            let ldx = dx * c - dy * s;
                            let ldy = dx * s + dy * c;
                            let mut nx = anchor.offset.0;
                            let mut ny = anchor.offset.1;
                            let mut nw = sw0;
                            let mut nh = sh0;
                            match handle {
                                Handle::E  => { nw = (sw0 + ldx).max(1.0); }
                                Handle::W  => { nx = anchor.offset.0 + ldx; nw = (sw0 - ldx).max(1.0); }
                                Handle::S  => { nh = (sh0 + ldy).max(1.0); }
                                Handle::N  => { ny = anchor.offset.1 + ldy; nh = (sh0 - ldy).max(1.0); }
                                Handle::SE => { nw = (sw0 + ldx).max(1.0); nh = (sh0 + ldy).max(1.0); }
                                Handle::NE => { nw = (sw0 + ldx).max(1.0); ny = anchor.offset.1 + ldy; nh = (sh0 - ldy).max(1.0); }
                                Handle::SW => { nx = anchor.offset.0 + ldx; nw = (sw0 - ldx).max(1.0); nh = (sh0 + ldy).max(1.0); }
                                Handle::NW => { nx = anchor.offset.0 + ldx; nw = (sw0 - ldx).max(1.0); ny = anchor.offset.1 + ldy; nh = (sh0 - ldy).max(1.0); }
                                _ => {}
                            }
                            // We're growing/shrinking around the original rotation center
                            // for axis edges, but our offset is a top-left in unrotated
                            // space — we need to keep the rotated rect's visual center
                            // stable for the unchanged-edge corners. We instead just
                            // apply the deltas directly and let the rotation compose;
                            // for non-zero rotation this is an approximation that
                            // matches user expectations from Figma/Photoshop.
                            self.select_state.scale = (nw / w0 as f32, nh / h0 as f32);
                            self.select_state.offset = (nx, ny);
                        }
                    }
                    SelectInteraction::Rotating => {
                        if let Some((w0, h0)) = self.select_state.float_size() {
                            let sw = w0 as f32 * anchor.scale.0.abs();
                            let sh = h0 as f32 * anchor.scale.1.abs();
                            let cx = anchor.offset.0 + sw * 0.5;
                            let cy = anchor.offset.1 + sh * 0.5;
                            let a0 = (anchor.mouse_y - cy).atan2(anchor.mouse_x - cx);
                            let a1 = (cy_px - cy).atan2(cx_px - cx);
                            let mut new_rot = anchor.rotation + (a1 - a0);
                            if shift_held {
                                let step = std::f32::consts::FRAC_PI_4; // 45°
                                new_rot = (new_rot / step).round() * step;
                            }
                            self.select_state.rotation = new_rot;
                        }
                    }
                    SelectInteraction::None => {}
                }
                self.canvas_dirty = true;
            }
            return true;
        }

        // Release ends the interaction.
        if !primary_down && self.select_state.interaction != SelectInteraction::None {
            self.select_state.interaction = SelectInteraction::None;
            self.select_state.drag_anchor = None;
            self.canvas_dirty = true;
        }
        // Consume hovers inside the selection so cursor can change later.
        false
    }

    fn add_frame(&mut self) {
        let idx = self.project.active_anim().frames.len();
        let w = self.project.canvas_width;
        let h = self.project.canvas_height;
        self.undo_stack.push(Command::AddFrame { animation_id: self.project.active_animation, index: idx });
        let new_frame_id = self.project.next_layer_id();
        self.project.active_anim_mut().frames.push(ProjectFrame::new(w, h, new_frame_id));
        self.project.active_frame = idx;
        if self.thumbnails.len() > self.project.active_animation {
            self.thumbnails[self.project.active_animation].push(FrameThumbnail::default());
        }
        self.canvas_dirty = true;
    }

    fn duplicate_active_frame(&mut self) {
        let idx = self.project.active_frame + 1;
        let frame = self.project.active_frame_ref().clone();
        self.undo_stack.push(Command::DuplicateFrame { animation_id: self.project.active_animation, index: idx, snapshot: frame.clone() });
        self.project.active_anim_mut().frames.insert(idx, frame);
        self.project.active_frame = idx;
        if self.thumbnails.len() > self.project.active_animation {
            self.thumbnails[self.project.active_animation].insert(idx, FrameThumbnail::default());
        }
        self.canvas_dirty = true;
    }

    fn delete_active_frame(&mut self) {
        let ai = self.project.active_animation;
        if self.project.animations[ai].frames.len() <= 1 {
            return;
        }
        let idx = self.project.active_frame;
        let snapshot = self.project.animations[ai].frames[idx].clone();
        self.undo_stack.push(Command::DeleteFrame { animation_id: ai, index: idx, snapshot });
        self.project.animations[ai].frames.remove(idx);
        self.project.active_frame = self.project.active_frame.saturating_sub(1).min(self.project.animations[ai].frames.len() - 1);
        self.canvas_dirty = true;
    }

    fn draw_new_project_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_new_dialog {
            return;
        }
        egui::Window::new("New Project")
            .collapsible(false)
            .resizable(false)
            .frame(Frame::window(&ctx.style()))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Grid::new("new_project_grid").num_columns(2).show(ui, |ui| {
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
                ui.horizontal(|ui| {
                    if ui.button(self.label("Create")).clicked() {
                        self.project = Project::new(self.new_width, self.new_height, self.new_name.clone());
                        self.canvas_dirty = true;
                        self.show_new_dialog = false;
                    }
                    if ui.button(self.label_muted("Cancel")).clicked() {
                        self.show_new_dialog = false;
                    }
                });
            });
    }

    /// Animated logo (sprite sheet, 16 frames horizontal, 16×16 each).
    ///
    /// Frame 0 is shown by default. Clicking the icon OR the "SQUAREZ" text
    /// plays frames 0..15 once at 30 FPS, then returns to frame 0.
    fn draw_logo(&mut self, ui: &mut egui::Ui, theme: &Theme) {
        // Lazy-load the sprite sheet once.
        if self.logo_sprite.is_none() {
            // Load external PNG from desktop (preferred) falling back to bundled asset.
            let bytes = std::fs::read("/Users/sasadukic/Desktop/logo.png").unwrap_or_else(|_| include_bytes!("../assets/logo_sprite.png").to_vec());
            if let Ok(img) = image::load_from_memory(&bytes) {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
                self.logo_sprite = Some(ui.ctx().load_texture(
                    "logo_sprite",
                    color_image,
                    egui::TextureOptions::NEAREST,
                ));
                // compute frames assuming horizontal strip where each frame is 16px wide and 16px tall
                self.logo_frames = (w / 16).max(1);
            }
        }

        let frames = self.logo_frames.max(1);
        // Speed up animation by 40% relative to previous 21.0 FPS -> ~29.4 FPS
        let fps: f64 = 21.0 * 1.4;
        let now = ui.ctx().input(|i| i.time);
        // Determine which frame to show. Priority:
        // 1) If we have a static frame (used when menu opens) show it.
        // 2) If an animation is playing, compute frame from start time and clamp to a play_range if set.
        // 3) Otherwise show frame 0.
        let frame_idx: usize = if let Some(s) = self.logo_anim_static_frame {
            s.min(frames - 1)
        } else if let Some(start) = self.logo_anim_start {
            // compute raw frame index from elapsed time
            let elapsed = now - start;
            let mut idx = (elapsed * fps) as isize;
            // If a play range is set, map idx into that subrange
            if let Some((r0, r1)) = self.logo_anim_play_range {
                let len = (r1 as isize).saturating_sub(r0 as isize).max(1);
                if idx >= len as isize {
                    // Finished the subrange. For closing animation (r0 >= 7),
                    // snap to frame 0 so we're ready for next open. Otherwise
                    // snap to the last frame of the range.
                    let last = if r0 >= 7 {
                        0
                    } else {
                        (r1.saturating_sub(1)).min(frames - 1)
                    };
                    self.logo_anim_static_frame = Some(last);
                    self.logo_anim_play_range = None;
                    self.logo_anim_start = None;
                    idx = last as isize;
                } else {
                    idx = r0 as isize + idx;
                }
            } else {
                if idx as usize >= frames {
                    self.logo_anim_start = None;
                    idx = 0;
                }
            }
            idx.max(0).min((frames - 1) as isize) as usize
        } else {
            0
        };
        if self.logo_anim_start.is_some() {
            ui.ctx().request_repaint();
        }

        ui.allocate_ui_with_layout(
            Vec2::new(BRAND_WIDTH, TOP_BAR_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_space(10.0);

                // Icon: 20×20, painted with one sprite frame.
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                if let Some(tex) = &self.logo_sprite {
                    let u0 = frame_idx as f32 / frames as f32;
                    let u1 = (frame_idx + 1) as f32 / frames as f32;
                    let uv = egui::Rect::from_min_max(Pos2::new(u0, 0.0), Pos2::new(u1, 1.0));
                    ui.painter().image(tex.id(), icon_rect, uv, Color32::WHITE);
                }

                ui.add_space(7.0);

                // Text
                let text = RichText::new("SQUAREZ")
                    .color(theme.fg)
                    .font(FontId::new(MENU_FONT_SIZE, FontFamily::Name("bold".into())));
                ui.add(egui::Label::new(text).sense(egui::Sense::hover()));
            },
        );
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = LayoutState {
            ui_state: self.ui_state.clone(),
            sidebar_order: self.sidebar_order.clone(),
            color_state: Some(self.color_state.clone()),
        };
        if let Ok(json) = serde_json::to_string(&state) {
            storage.set_string(LAYOUT_STORAGE_KEY, json);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);

        if cfg!(debug_assertions) {
            // debug-only app state print removed
        }

        let fps = self.project.active_anim().fps;
        let total = self.project.active_anim().frames.len();
        if self.playback.tick(fps, &mut self.project.active_frame, total) {
            self.canvas_dirty = true;
        }

        // Alt → cycle to next tool in the active tool's group (detect on rising edge)
        let alt_now = ctx.input(|i| i.modifiers.alt);
        if alt_now && !self.alt_was_down {
            self.cycle_tool_in_group();
        }
        self.alt_was_down = alt_now;

        // Escape commits a floating selection back to the layer.
        if self.select_state.has_float() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.commit_float_to_layer();
        }
        // If the user switches away from the RectSelect tool, commit any float.
        if self.select_state.has_float() && !matches!(self.active_tool, ActiveTool::RectSelect) {
            self.commit_float_to_layer();
        }

        let ctrl_z = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl);
        let ctrl_y = ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl);
        if ctrl_z {
            // Prefer color-aware undo so ColorState snapshots (ramp edits) are restored.
            self.undo_stack.undo_with_color(&mut self.project, &mut self.color_state);
            self.canvas_dirty = true;
        }
        if ctrl_y {
            self.undo_stack.redo_with_color(&mut self.project, &mut self.color_state);
            self.canvas_dirty = true;
        }

        self.draw_top_bar(ctx);
        self.draw_right_sidebar(ctx); // full height — right edge
        self.draw_anim_toolbar(ctx);  // full-width — must be first bottom panel
        self.draw_timeline(ctx);      // gets x=0..W-176, frames start at left edge
        self.draw_left_toolbar(ctx);  // occupies left strip above timeline only
        self.draw_tool_submenu(ctx);  // floating tool group submenu (right of toolbar)
        self.draw_workspace(ctx);
        self.draw_layer_context_menu(ctx);
        self.draw_new_project_dialog(ctx);

        if self.playback.is_playing {
            ctx.request_repaint();
        }
    }
}

fn rich(text: &str, color: Color32, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::new(size, FontFamily::Proportional))
        .color(color)
}

fn top_menu_zone(ui: &mut egui::Ui, theme: &Theme, label: &str, selected: bool) -> egui::Response {
    let size = Vec2::new(menu_zone_width(label), TOP_BAR_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    // Highlight fill is drawn externally by the spring highlight in draw_top_bar.
    // Only draw the text here.
    let is_active = selected || response.hovered();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(MENU_FONT_SIZE, FontFamily::Proportional),
        if is_active { theme.fg } else { theme.fg_desc },
    );
    response
}

fn dropdown_row(ui: &mut egui::Ui, theme: &Theme, label: &str, right: Option<&str>, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(DROPDOWN_WIDTH, DROPDOWN_ROW_HEIGHT),
        if enabled { egui::Sense::click() } else { egui::Sense::hover() },
    );
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme.accent);
    }

    let color = if enabled { theme.fg_desc } else { theme.fg_muted };
    let y = rect.center().y;
    ui.painter().text(
        Pos2::new(rect.left() + 14.0, y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
        color,
    );
    if let Some(right) = right {
        ui.painter().text(
            Pos2::new(rect.right() - 14.0, y),
            egui::Align2::RIGHT_CENTER,
            right,
            FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
            color,
        );
    }
    response
}

fn window_check(visible: bool) -> Option<&'static str> {
    visible.then_some("✓")
}

/// Returns `(show_content, add_clicked)`.
fn section_header(ui: &mut egui::Ui, theme: &Theme, state: &mut UiState, panel: Panel, icon: ImageSource<'static>, extra_btn: Option<ImageSource<'static>>) -> (bool, bool, bool) {
    section_header_with_add(ui, theme, state, panel, icon, extra_btn, true)
}

fn section_header_with_add(ui: &mut egui::Ui, theme: &Theme, state: &mut UiState, panel: Panel, icon: ImageSource<'static>, extra_btn: Option<ImageSource<'static>>, show_add: bool) -> (bool, bool, bool) {
    if !state.is_visible(panel) {
        return (false, false, false);
    }
    let collapsed = state.is_collapsed(panel);
    let mut add_clicked = false;
    let mut extra_clicked = false;
    Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 3)).show(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        // Left: section icon — clickable to collapse/expand
        let icon_size = Vec2::splat(16.0);
        let icon_rect = egui::Rect::from_center_size(Pos2::new(rect.left() + 8.0, rect.center().y), icon_size);
        let icon_resp = ui.interact(icon_rect, egui::Id::new(("hdr_icon", panel)), egui::Sense::click());
        let icon_tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
        ui.put(icon_rect, Image::new(icon).tint(icon_tint).fit_to_exact_size(icon_size));
        if icon_resp.clicked() {
            state.toggle_collapsed(panel);
        }
        if !collapsed {
            // Right: "+" add button (optional)
            if show_add {
                let plus_rect = egui::Rect::from_center_size(Pos2::new(rect.right() - 8.0, rect.center().y), Vec2::splat(16.0));
                let plus_resp = ui.interact(plus_rect, egui::Id::new(("hdr_plus", panel)), egui::Sense::click());
                let plus_color = if plus_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                painter.text(plus_rect.center(), egui::Align2::CENTER_CENTER, "+", FontId::new(16.0, FontFamily::Proportional), plus_color);
                if plus_resp.clicked() {
                    add_clicked = true;
                }
            }
            // Optional extra button (folder/group icon), placed left of "+"
            if let Some(extra_icon) = extra_btn {
                let extra_rect = egui::Rect::from_center_size(Pos2::new(rect.right() - 28.0, rect.center().y), Vec2::splat(14.0));
                let extra_resp = ui.interact(extra_rect, egui::Id::new(("hdr_extra", panel)), egui::Sense::click());
                let tint = if extra_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                ui.put(extra_rect, Image::new(extra_icon).tint(tint).fit_to_exact_size(Vec2::splat(14.0)));
                if extra_resp.clicked() { extra_clicked = true; }
            }
        }
    });
    (!collapsed, add_clicked, extra_clicked)
}

/// Color slider with label inside the handle. Returns true when value changed.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Sync all mode caches and display values from current foreground.
fn sync_color_caches(state: &mut ColorState) {
    let fg = state.foreground;
    let (l, c, h) = rgba_to_oklch(fg);
    state.oklch_l = l;
    state.oklch_c = c;
    state.oklch_h = h;
    state.display_oklch_l = l;
    state.display_oklch_c = c;
    state.display_oklch_h = h;
    let (h2, s, v) = rgba_to_hsv(fg);
    state.hsv_h = h2;
    state.hsv_s = s;
    state.hsv_v = v;
    state.display_hsv_h = h2;
    state.display_hsv_s = s;
    state.display_hsv_v = v;
    state.rgb_r = fg[0] as f32;
    state.rgb_g = fg[1] as f32;
    state.rgb_b = fg[2] as f32;
    state.display_rgb_r = fg[0] as f32;
    state.display_rgb_g = fg[1] as f32;
    state.display_rgb_b = fg[2] as f32;
}

fn color_slider(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &mut f32, min: f32, max: f32) -> bool {
    const TRACK_H: f32 = 16.0;
    const PAD_BELOW: f32 = 6.0;
    const RADIUS: f32 = TRACK_H * 0.01; // ~0.16px — subtle, near-sharp rounding
    let mut changed = false;

    // Full-width track; no separate label — label drawn inside the thumb.
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), TRACK_H + PAD_BELOW),
        egui::Sense::click_and_drag(),
    );

    let track_rect = egui::Rect::from_min_max(
        rect.left_top(),
        Pos2::new(rect.right(), rect.top() + TRACK_H),
    );

    let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);

    // Background
    ui.painter().rect_filled(track_rect, RADIUS, theme.bg);

    // Fill
    let fill = egui::Rect::from_min_max(
        track_rect.left_top(),
        Pos2::new(track_rect.left() + track_rect.width() * t, track_rect.bottom()),
    );
    ui.painter().rect_filled(fill, RADIUS, theme.accent);

    // Thumb (same height as track, matches fill color) — clamp center to track edges
    let half = TRACK_H * 0.5;
    let cx = (track_rect.left() + half + (track_rect.width() - TRACK_H) * t)
        .clamp(track_rect.left() + half, track_rect.right() - half);
    let cy = track_rect.center().y;
    let thumb = egui::Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(TRACK_H, TRACK_H));
    ui.painter().rect_filled(thumb, RADIUS, theme.accent);

    // Hover check: anywhere on the track rect makes handle text white
    let is_hovered = ui.ctx().input(|i| {
        i.pointer.latest_pos().map_or(false, |pos| track_rect.contains(pos))
    }) || resp.hovered();

    // Label inside thumb (white on hover, muted otherwise)
    let label_color = if is_hovered { Color32::WHITE } else { theme.fg_muted };
    ui.painter().text(
        thumb.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
        label_color,
    );

    // Interaction — use latest pointer pos so drag works even if interact_pointer_pos is None.
    let pointer = ui.ctx().input(|i| i.pointer.clone());
    if resp.dragged() || (is_hovered && pointer.primary_down()) || resp.clicked() {
        if let Some(pos) = pointer.latest_pos() {
            let eff_w = (track_rect.width() - TRACK_H).max(1.0);
            let rel = ((pos.x - track_rect.left() - half) / eff_w).clamp(0.0, 1.0);
            let new_val = min + (max - min) * rel;
            if (new_val - *value).abs() > 0.001 {
                *value = new_val;
                changed = true;
            }
        }
    }

    changed
}

fn tool_btn(ui: &mut egui::Ui, active_tool: &mut ActiveTool, theme: &Theme, tool: ActiveTool, icon: ImageSource<'static>) -> egui::Response {
    let selected = *active_tool == tool;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::click());
    let tint = if selected {
        theme.fg
    } else if response.hovered() {
        Color32::WHITE
    } else {
        theme.fg_desc
    };
    ui.put(rect, Image::new(icon).fit_to_exact_size(Vec2::splat(18.0)).tint(tint));
    if response.clicked() {
        *active_tool = tool;
    }
    response
}

/// Stateless tool button (no auto-selection). Caller decides what click does.
fn tool_btn_raw(ui: &mut egui::Ui, theme: &Theme, selected: bool, icon: ImageSource<'static>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::click());
    let tint = if selected {
        theme.fg
    } else if response.hovered() {
        Color32::WHITE
    } else {
        theme.fg_desc
    };
    ui.put(rect, Image::new(icon).fit_to_exact_size(Vec2::splat(18.0)).tint(tint));
    response
}

/// Map a tool variant to its icon image.
fn tool_icon(tool: &ActiveTool) -> ImageSource<'static> {
    match tool {
        ActiveTool::Pencil           => egui::include_image!("../assets/icons/pencil.svg"),
        ActiveTool::Eraser           => egui::include_image!("../assets/icons/eraser.svg"),
        ActiveTool::Fill             => egui::include_image!("../assets/icons/fill.svg"),
        ActiveTool::Eyedropper       => egui::include_image!("../assets/icons/eyedropper.svg"),
        ActiveTool::Rectangle { .. } => egui::include_image!("../assets/icons/rectangle.svg"),
        ActiveTool::Ellipse { .. }   => egui::include_image!("../assets/icons/ellipse.svg"),
        ActiveTool::Line             => egui::include_image!("../assets/icons/line.svg"),
        ActiveTool::RectSelect       => egui::include_image!("../assets/icons/select.svg"),
        ActiveTool::Move             => egui::include_image!("../assets/icons/move.svg"),
        ActiveTool::Zoom             => egui::include_image!("../assets/icons/zoom.svg"),
    }
}

fn icon_flat_button(ui: &mut egui::Ui, theme: &Theme, icon: ImageSource<'static>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::click());
    let tint = if resp.hovered() { Color32::WHITE } else { theme.fg_desc };
    ui.put(rect, Image::new(icon).fit_to_exact_size(Vec2::splat(14.0)).tint(tint));
    resp
}

fn panel_icon(panel: Panel) -> egui::ImageSource<'static> {
    match panel {
        Panel::Palette    => egui::include_image!("../assets/icons/colors.svg"),
        Panel::Color      => egui::include_image!("../assets/icons/color_mixer.svg"),
        Panel::Layers     => egui::include_image!("../assets/icons/layer.svg"),
        Panel::Animations => egui::include_image!("../assets/icons/animation.svg"),
        Panel::Preview    => egui::include_image!("../assets/icons/visibility.svg"),
        Panel::Timeline   => egui::include_image!("../assets/icons/visibility.svg"),
    }
}

fn rfd_open() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Squarez Project", &["sqr"])
        .pick_file()
}

/// Shift-constrain the end point of a shape drag.
/// - Rectangle / Ellipse → square / circle: clamp to the smaller axis delta.
/// - Line → snap to the nearest 45° direction.
fn shape_shift_constrain(tool: &ActiveTool, x0: i32, y0: i32, ex: i32, ey: i32) -> (i32, i32) {
    match tool {
        ActiveTool::Rectangle { .. } | ActiveTool::Ellipse { .. } => {
            let dx = ex - x0;
            let dy = ey - y0;
            let side = dx.abs().min(dy.abs());
            (x0 + side * dx.signum(), y0 + side * dy.signum())
        }
        ActiveTool::Line => {
            let dx = (ex - x0) as f32;
            let dy = (ey - y0) as f32;
            let len = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);
            let snap = (std::f32::consts::PI / 4.0).round(); // 45° in radians
            let snapped = (angle / (std::f32::consts::PI / 4.0)).round() * (std::f32::consts::PI / 4.0);
            let _ = snap;
            (
                x0 + (len * snapped.cos()).round() as i32,
                y0 + (len * snapped.sin()).round() as i32,
            )
        }
        _ => (ex, ey),
    }
}
