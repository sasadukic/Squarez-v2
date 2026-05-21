// src/app.rs
use egui::{CentralPanel, Color32, FontFamily, FontId, Frame, Image, ImageSource, Margin, Pos2, RichText, SidePanel, TopBottomPanel, Vec2};

use crate::animation::{FrameThumbnail, PlaybackState};
use crate::canvas::CanvasState;
use crate::color::hsv::{hsv_to_rgba, rgba_to_hsv};
use crate::color::oklab::{oklab_to_rgba, rgba_to_oklab, oklch_to_rgba, rgba_to_oklch, generate_ramp, generate_ramp_hsv};
use crate::color::RampAnchor;
use crate::color::{ColorState, PickerMode};
use crate::history::{Command, UndoStack};
use crate::io::export::{export, ExportFormat, ExportOptions};
use crate::io::sqr::{load_sqr, save_sqr};
use crate::layers::composite_frame;
use crate::project::{Animation, Frame as ProjectFrame, Layer, Project, Rgba};
use crate::theme::{load_fonts, Theme, FONT_SIZE_SM};
use crate::top_bar::{
    menu_zone_width, BRAND_WIDTH, MENU_LEFT_GAP, DROPDOWN_CORNER_RADIUS, DROPDOWN_ROW_HEIGHT, DROPDOWN_TOP_GAP,
    DROPDOWN_WIDTH, MENU_FONT_SIZE, TOP_BAR_HEIGHT,
};
use crate::tools::{apply_eraser, apply_eyedropper, apply_ellipse, apply_fill, apply_line, apply_pencil, apply_rect, ActiveTool};
use crate::ui_metrics::{COLOR_SLIDER_TRACK_HEIGHT, RIGHT_SECTION_STACK_GAP};
use crate::ui_state::{Panel, UiState};

/// Which slider parameter the right-click popup controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SliderParam {
    HueShift,
    SatCurve,
}

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
    canvas_dirty: bool,
    show_new_dialog: bool,
    new_width: u32,
    new_height: u32,
    new_name: String,
    frame_menu: Option<(usize, Pos2, f64)>,  // (frame_idx, screen_pos, opened_at_time)
    /// Right-click menu on OKL tab: position + time it was opened.
    ramp_size_menu: Option<(Pos2, f64)>,
    /// Right-click menu on a slider label: which param + pos + opened time.
    slider_param_menu: Option<(SliderParam, Pos2, f64)>,
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
    // Accumulated scroll delta for timeline frame navigation (slows down scroll speed)
    timeline_scroll_accum: f32,
    // View > Show sub-menu open state
    view_show_open: bool,
    // Screen-space right-top of the "Show" row, used to position side submenu
    view_show_pos: Option<egui::Pos2>,
    // Alpha for the anim toolbar fade (0 = hidden, 1 = fully visible)
    anim_toolbar_alpha: f32,
    // Sidebar section order (drag-to-reorder)
    sidebar_order: Vec<Panel>,
    // Drag-to-reorder state (only active in narrow/all-collapsed mode with Cmd held)
    sidebar_drag: Option<Panel>,
    sidebar_drag_over_idx: Option<usize>,
    // Long-press timer: (panel under pointer, time of initial press)
    sidebar_press_start: Option<(Panel, f64)>,
    // Icon row rects recorded each frame for hit-testing (screen space)
    sidebar_icon_rects: Vec<(Panel, egui::Rect)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopMenu {
    File,
    Edit,
    View,
    Layer,
    Animation,
    Windows,
}

impl TopMenu {
    fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Layer => "Layer",
            Self::Animation => "Animation",
            Self::Windows => "Windows",
        }
    }

    /// Pixel width of this menu's hit zone in the top bar.
    fn zone_width(self) -> f32 {
        match self {
            Self::File | Self::Edit => 38.0, // icon buttons, no text
            _ => menu_zone_width(self.label()),
        }
    }
}


const LAYOUT_STORAGE_KEY: &str = "squarez_layout_v1";

#[derive(serde::Serialize, serde::Deserialize)]
struct LayoutState {
    ui_state: UiState,
    sidebar_order: Vec<Panel>,
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
        let mut color_state = ColorState::default();
        if let Some(first) = project.palette.first() {
            color_state.foreground = *first;
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
            canvas_dirty: true,
            show_new_dialog: false,
            new_width: 16,
            new_height: 16,
            new_name: "Untitled".to_string(),
            frame_menu: None,
            ramp_size_menu: None,
            slider_param_menu: None,
            layer_ctx_menu: None,            top_menu_open: None,
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
            timeline_scroll_accum: 0.0,
            view_show_open: false,
            view_show_pos: None,
            anim_toolbar_alpha: 1.0,
            sidebar_order: layout.map(|l| l.sidebar_order).unwrap_or_else(|| vec![Panel::Palette, Panel::Color, Panel::Layers, Panel::Animations, Panel::Preview]),
            sidebar_drag: None,
            sidebar_drag_over_idx: None,
            sidebar_press_start: None,
            sidebar_icon_rects: Vec::new(),
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
            let mut new = Layer::new(l.name.clone(), w, h, l.id);
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
            ActiveTool::RectSelect => ActiveTool::Move,
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
        let dt = ctx.input(|i| i.unstable_dt).min(0.05);
        let all_menus = [TopMenu::File, TopMenu::Edit];

        TopBottomPanel::top("top_bar")
            .exact_height(TOP_BAR_HEIGHT)
            .frame(self.panel_frame())
            .show_separator_line(false)
            .show(ctx, |ui| {
                let theme = self.theme.clone();
                ui.set_height(TOP_BAR_HEIGHT);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal(|ui| {
                    draw_logo(ui, &theme);
                    ui.add_space(MENU_LEFT_GAP);

                    // Compute screen-space rects for all menu zones (before laying them out)
                    let origin_x = ui.next_widget_position().x;
                    let bar_rect = ui.max_rect(); // full panel rect — correct y/height regardless of cursor
                    let mut x_off = 0.0f32;
                    let menu_rects: Vec<egui::Rect> = all_menus.iter().map(|m| {
                        let w = m.zone_width();
                        let r = egui::Rect::from_min_size(
                            Pos2::new(origin_x + x_off, bar_rect.top()),
                            Vec2::new(w, bar_rect.height()),
                        );
                        x_off += w;
                        r
                    }).collect();

                    // Determine spring target: hovered item takes priority over open item
                    let open_idx = self.top_menu_open.and_then(|(m, _)| all_menus.iter().position(|x| *x == m));
                    let hover_pos = ctx.input(|i| i.pointer.hover_pos());
                    let hover_idx = hover_pos.and_then(|p| menu_rects.iter().position(|r| r.contains(p)));
                    let target_idx = hover_idx.or(open_idx);

                    if let Some(idx) = target_idx {
                        let target_x = menu_rects[idx].left();
                        let target_w  = menu_rects[idx].width();

                        // Snap to position on first encounter so it doesn't fly in from 0,0
                        if !self.menu_anim_initialized {
                            self.menu_anim_x = target_x;
                            self.menu_anim_initialized = true;
                        }

                        // Spring physics (same tuning as toolbar)
                        let force = (target_x - self.menu_anim_x) * 300.0
                                  - self.menu_anim_vel * 22.0;
                        self.menu_anim_vel += force * dt;
                        self.menu_anim_x   += self.menu_anim_vel * dt;

                        let settled = (self.menu_anim_x - target_x).abs() < 0.3
                                   && self.menu_anim_vel.abs() < 0.3;
                        if settled {
                            self.menu_anim_x   = target_x;
                            self.menu_anim_vel = 0.0;
                        } else {
                            ctx.request_repaint();
                        }

                        // Draw the sliding highlight BEFORE the text zones
                        let highlight = egui::Rect::from_min_size(
                            Pos2::new(self.menu_anim_x, bar_rect.top()),
                            Vec2::new(target_w, bar_rect.height()),
                        );
                        ui.painter().rect_filled(highlight, 0.0, theme.surface);
                    }

                    // Lay out the menu zones (no fill drawn inside them)
                    for menu in all_menus.iter() {
                        let selected = self.top_menu_open.is_some_and(|(open, _)| open == *menu);
                        let response = if *menu == TopMenu::File {
                            // File: file.svg icon — use the pre-computed menu_rect so it lines up with the highlight
                            let i = all_menus.iter().position(|m| m == menu).unwrap();
                            let zone_rect = menu_rects[i];
                            let resp = ui.allocate_rect(zone_rect, egui::Sense::click());
                            let is_active = selected || resp.hovered();
                            let tint = if is_active { theme.fg } else { theme.fg_desc };
                            let icon_rect = egui::Rect::from_center_size(zone_rect.center(), Vec2::splat(16.0));
                            ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/file.svg")).tint(tint).fit_to_exact_size(Vec2::splat(16.0)));
                            resp
                        } else if *menu == TopMenu::Edit {
                            // Edit: tools.svg icon — use the pre-computed menu_rect so it lines up with the highlight
                            let i = all_menus.iter().position(|m| m == menu).unwrap();
                            let zone_rect = menu_rects[i];
                            let resp = ui.allocate_rect(zone_rect, egui::Sense::click());
                            let is_active = selected || resp.hovered();
                            let tint = if is_active { theme.fg } else { theme.fg_desc };
                            let icon_rect = egui::Rect::from_center_size(zone_rect.center(), Vec2::splat(16.0));
                            ui.put(icon_rect, Image::new(egui::include_image!("../assets/icons/tools.svg")).tint(tint).fit_to_exact_size(Vec2::splat(16.0)));
                            resp
                        } else {
                            top_menu_zone(ui, &theme, menu.label(), selected)
                        };
                        if response.clicked() {
                            let pos = Pos2::new(response.rect.left(), response.rect.bottom() + DROPDOWN_TOP_GAP);
                            if selected {
                                self.top_menu_open = None;
                            } else {
                                self.top_menu_open    = Some((*menu, pos));
                                self.top_menu_opened_at = ctx.input(|i| i.time);
                                self.top_menu_hover_left = None;
                                self.view_show_open = false;
                                // Reset dropdown open animation
                                self.dropdown_clip_h   = 0.0;
                                self.dropdown_clip_vel = 0.0;
                                self.dropdown_full_h   = 0.0;
                            }
                        }
                    }
                });
            });
        self.draw_top_menu_dropdown(ctx);
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
                            TopMenu::Edit => {
                                dropdown_row(ui, &theme, "Rotate", None, false);
                                dropdown_row(ui, &theme, "Flip Horizontal", None, false);
                                dropdown_row(ui, &theme, "Flip Vertical", None, false);
                                dropdown_row(ui, &theme, "Transform", None, false);
                                dropdown_row(ui, &theme, "Replace Color", None, false);
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
                                            frame.layers.push(Layer::new(name.clone(), w, h, new_id));
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
            self.top_menu_open = None;
            self.top_menu_hover_left = None;
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
                self.top_menu_open = None;
                self.top_menu_hover_left = None;
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
                    self.top_menu_open = None;
                    self.top_menu_hover_left = None;
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

        SidePanel::left("toolbar")
            .exact_width(38.0)
            .resizable(false)
            .frame(Frame::new().fill(self.theme.bg)) // bg below tools, panel only behind buttons
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                // Vertically center the button stack
                const TOOL_COUNT: f32 = 5.0;
                let tools_h = TOOL_COUNT * 38.0;
                let top_pad = ((ui.available_height() - tools_h) / 2.0).max(0.0);
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
                    ActiveTool::RectSelect => vec![ActiveTool::Move],
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
        // Not visible: render a standalone header-row icon (same geometry as section_header)
        // so the user can click it to bring the section back.
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
                let icon_tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg };
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

        // Visible: section_header handles collapse/expand (same pattern as Layers/Animations)
        let (show, _, _) = section_header(
            ui,
            &self.theme,
            &mut self.ui_state,
            Panel::Color,
            egui::include_image!("../assets/icons/color_mixer.svg"),
            None,
        );
        if !show { return; }

        let theme = self.theme.clone();
        let fg = self.color_state.foreground;
        Frame::new().fill(theme.panel).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO; // own all spacing explicitly
                let fg_color = Color32::from_rgba_unmultiplied(fg[0], fg[1], fg[2], fg[3]);
                ui.add(
                    egui::Button::new("")
                        .fill(fg_color)
                        .stroke(egui::Stroke::NONE)
                        .min_size(Vec2::new(38.0, 38.0)),
                );
                ui.add_space(8.0); // explicit gap, no extra item_spacing added
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let okl_resp = tab_button(ui, &theme, self.color_state.active_picker == PickerMode::OkLab, "OKL");
                        if okl_resp.clicked() {
                            self.color_state.active_picker = PickerMode::OkLab;
                        }
                        if okl_resp.secondary_clicked() {
                            let now = ui.ctx().input(|i| i.time);
                            self.ramp_size_menu = Some((okl_resp.rect.left_bottom(), now));
                        }
                        if tab_button(ui, &theme, self.color_state.active_picker == PickerMode::Hsv, "HSV").clicked() {
                            self.color_state.active_picker = PickerMode::Hsv;
                        }
                        if tab_button(ui, &theme, self.color_state.active_picker == PickerMode::Rgb, "RGB").clicked() {
                            self.color_state.active_picker = PickerMode::Rgb;
                        }
                    });
                    // Slider track always ends at inner_width(156) - 4 = 152px from frame left.
                    // Hex box starts at color_box(38) + gap(8) = 46px.
                    // hex_outer = 152 - 46 = 106px; hex_w = 106 - inner_margin(6+6=12) = 94px.
                    Frame::new().fill(theme.bg).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                        ui.set_width(94.0);
                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                            ui.label(rich(&format!("#{:02X}{:02X}{:02X}", fg[0], fg[1], fg[2]), theme.fg, FONT_SIZE_SM));
                        });
                    });
                });
            });

            ui.add_space(6.0); // padding between color box row and first slider
            match self.color_state.active_picker {
                PickerMode::Hsv => {
                    let (mut h, mut s, mut v) = rgba_to_hsv(fg);
                    let n = self.color_state.ramp_size;
                    let mut changed = false;
                    let now = ui.ctx().input(|i| i.time);
                    let out_h = value_slider_snap(ui, &theme, "H", &mut h, 0.0..=360.0,
                        &mut self.color_state.snap_hsv_h, (0.0, 360.0), n);
                    let out_s = value_slider_snap(ui, &theme, "S", &mut s, 0.0..=1.0,
                        &mut self.color_state.snap_hsv_s, (0.0, 1.0), n);
                    let out_v = value_slider_snap(ui, &theme, "V", &mut v, 0.0..=1.0,
                        &mut self.color_state.snap_hsv_v, (0.20, 0.95), n);
                    changed |= out_h.changed | out_s.changed | out_v.changed;
                    if let Some(pos) = out_h.label_rclick {
                        self.slider_param_menu = Some((SliderParam::HueShift, pos, now));
                    }
                    if let Some(pos) = out_s.label_rclick {
                        self.slider_param_menu = Some((SliderParam::SatCurve, pos, now));
                    }
                    if changed {
                        self.color_state.foreground = hsv_to_rgba(h, s, v, fg[3]);
                    }

                    // Ramp strip + push-to-palette button (mirror of OKL branch)
                    ui.add_space(6.0);
                    let ramp_hsv = generate_ramp_hsv(h, s, v, n, self.color_state.ramp_anchor,
                        self.color_state.hue_shift_deg, self.color_state.sat_curve_depth);
                    let ramp_rgba: Vec<crate::project::Rgba> = ramp_hsv.iter()
                        .map(|&(h, s, v)| hsv_to_rgba(h, s, v, 255))
                        .collect();
                    let anchor_idx = match self.color_state.ramp_anchor {
                        RampAnchor::Middle    => n / 2,
                        RampAnchor::BaseStep3 => 2.min(n.saturating_sub(1)),
                        RampAnchor::Endpoints => 0,
                    };
                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                        let avail = ui.available_width();
                        const ADD_BTN_W: f32 = 18.0;
                        const GAP: f32 = 4.0;
                        let strip_w = (avail - ADD_BTN_W - GAP).max(0.0);
                        const STRIP_H: f32 = 18.0;
                        const MARKER_H: f32 = 4.0;
                        let row_h = STRIP_H + MARKER_H;
                        let strip_size = Vec2::new(strip_w, row_h);
                        let (strip_rect, strip_resp) = ui.allocate_exact_size(strip_size, egui::Sense::click());
                        let cell_w = strip_w / (n.max(1) as f32);
                        let cells_top = strip_rect.top();
                        let cells_bot = strip_rect.top() + STRIP_H;
                        for (i, rgba) in ramp_rgba.iter().enumerate() {
                            let x0 = strip_rect.left() + cell_w * i as f32;
                            let x1 = strip_rect.left() + cell_w * (i + 1) as f32;
                            let cell = egui::Rect::from_min_max(
                                Pos2::new(x0, cells_top),
                                Pos2::new(x1, cells_bot),
                            );
                            ui.painter().rect_filled(cell, 0.0, Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]));
                        }
                        let anchor_cx = strip_rect.left() + cell_w * (anchor_idx as f32 + 0.5);
                        let tri_top_y = cells_bot + 1.0;
                        let tri_bot_y = cells_bot + MARKER_H;
                        let half_w = 3.0;
                        ui.painter().add(egui::Shape::convex_polygon(
                            vec![
                                Pos2::new(anchor_cx, tri_top_y),
                                Pos2::new(anchor_cx - half_w, tri_bot_y),
                                Pos2::new(anchor_cx + half_w, tri_bot_y),
                            ],
                            theme.fg,
                            egui::Stroke::NONE,
                        ));
                        if strip_resp.secondary_clicked() {
                            self.color_state.ramp_anchor = match self.color_state.ramp_anchor {
                                RampAnchor::Middle    => RampAnchor::BaseStep3,
                                RampAnchor::BaseStep3 => RampAnchor::Endpoints,
                                RampAnchor::Endpoints => RampAnchor::Middle,
                            };
                        }
                        if strip_resp.clicked() {
                            if let Some(pos) = strip_resp.interact_pointer_pos() {
                                let rel = (pos.x - strip_rect.left()).clamp(0.0, strip_rect.width() - 0.001);
                                let idx = (rel / cell_w) as usize;
                                if let Some(picked) = ramp_rgba.get(idx) {
                                    self.color_state.foreground = *picked;
                                }
                            }
                        }
                        ui.add_space(GAP);
                        let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::splat(ADD_BTN_W), egui::Sense::click());
                        let bg = if btn_resp.hovered() { theme.accent } else { theme.bg };
                        ui.painter().rect_filled(btn_rect, 0.0, bg);
                        let tint = if btn_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.painter().text(
                            btn_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "+",
                            FontId::new(14.0, FontFamily::Proportional),
                            tint,
                        );
                        if btn_resp.clicked() {
                            for rgba in &ramp_rgba {
                                if !self.project.palette.contains(rgba) {
                                    self.project.palette.push(*rgba);
                                }
                            }
                        }
                    });
                }
                PickerMode::OkLab => {
                    let (mut l, mut c, mut h) = rgba_to_oklch(fg);
                    // Preserve prior hue when chroma collapses (achromatic colors have undefined H).
                    if c < 1e-4 { h = self.color_state.last_oklch_h; }
                    let n = self.color_state.ramp_size;
                    let mut changed = false;
                    let now = ui.ctx().input(|i| i.time);
                    let out_l = value_slider_snap(ui, &theme, "L", &mut l, 0.0..=1.0,
                        &mut self.color_state.snap_oklch_l, (0.15, 0.90), n);
                    let out_c = value_slider_snap(ui, &theme, "C", &mut c, 0.0..=0.4,
                        &mut self.color_state.snap_oklch_c, (0.0, 0.4), n);
                    let out_h = value_slider_snap(ui, &theme, "H", &mut h, 0.0..=360.0,
                        &mut self.color_state.snap_oklch_h, (0.0, 360.0), n);
                    changed |= out_l.changed | out_c.changed | out_h.changed;
                    // Right-click on H label → hue-shift popup; on C label → sat-curve popup.
                    if let Some(pos) = out_h.label_rclick {
                        self.slider_param_menu = Some((SliderParam::HueShift, pos, now));
                    }
                    if let Some(pos) = out_c.label_rclick {
                        self.slider_param_menu = Some((SliderParam::SatCurve, pos, now));
                    }
                    if changed {
                        self.color_state.last_oklch_h = h;
                        self.color_state.foreground = oklch_to_rgba(l, c, h, fg[3]);
                    }

                    // Ramp strip + push-to-palette button
                    ui.add_space(6.0);
                    let ramp_lch = generate_ramp(l, c, h, n, self.color_state.ramp_anchor,
                        self.color_state.hue_shift_deg, self.color_state.sat_curve_depth);
                    let ramp_rgba: Vec<crate::project::Rgba> = ramp_lch.iter()
                        .map(|&(l, c, h)| oklch_to_rgba(l, c, h, 255))
                        .collect();
                    let anchor_idx = match self.color_state.ramp_anchor {
                        RampAnchor::Middle    => n / 2,
                        RampAnchor::BaseStep3 => 2.min(n.saturating_sub(1)),
                        RampAnchor::Endpoints => 0,
                    };
                    ui.horizontal(|ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                        // Strip: full available width minus add button (18px) and 4px gap.
                        let avail = ui.available_width();
                        const ADD_BTN_W: f32 = 18.0;
                        const GAP: f32 = 4.0;
                        let strip_w = (avail - ADD_BTN_W - GAP).max(0.0);
                        const STRIP_H: f32 = 18.0;
                        const MARKER_H: f32 = 4.0;
                        let row_h = STRIP_H + MARKER_H;
                        let strip_size = Vec2::new(strip_w, row_h);
                        let (strip_rect, strip_resp) = ui.allocate_exact_size(strip_size, egui::Sense::click());
                        let cell_w = strip_w / (n.max(1) as f32);
                        let cells_top = strip_rect.top();
                        let cells_bot = strip_rect.top() + STRIP_H;
                        for (i, rgba) in ramp_rgba.iter().enumerate() {
                            let x0 = strip_rect.left() + cell_w * i as f32;
                            let x1 = strip_rect.left() + cell_w * (i + 1) as f32;
                            let cell = egui::Rect::from_min_max(
                                Pos2::new(x0, cells_top),
                                Pos2::new(x1, cells_bot),
                            );
                            ui.painter().rect_filled(cell, 0.0, Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]));
                        }
                        // Anchor indicator: small triangle pointing up under the anchored cell.
                        let anchor_cx = strip_rect.left() + cell_w * (anchor_idx as f32 + 0.5);
                        let tri_top_y = cells_bot + 1.0;
                        let tri_bot_y = cells_bot + MARKER_H;
                        let half_w = 3.0;
                        ui.painter().add(egui::Shape::convex_polygon(
                            vec![
                                Pos2::new(anchor_cx, tri_top_y),
                                Pos2::new(anchor_cx - half_w, tri_bot_y),
                                Pos2::new(anchor_cx + half_w, tri_bot_y),
                            ],
                            theme.fg,
                            egui::Stroke::NONE,
                        ));
                        // Right-click cycles anchor mode.
                        if strip_resp.secondary_clicked() {
                            self.color_state.ramp_anchor = match self.color_state.ramp_anchor {
                                RampAnchor::Middle    => RampAnchor::BaseStep3,
                                RampAnchor::BaseStep3 => RampAnchor::Endpoints,
                                RampAnchor::Endpoints => RampAnchor::Middle,
                            };
                        }
                        // Left-click cell sets FG to that color.
                        if strip_resp.clicked() {
                            if let Some(pos) = strip_resp.interact_pointer_pos() {
                                let rel = (pos.x - strip_rect.left()).clamp(0.0, strip_rect.width() - 0.001);
                                let idx = (rel / cell_w) as usize;
                                if let Some(picked) = ramp_rgba.get(idx) {
                                    self.color_state.foreground = *picked;
                                }
                            }
                        }
                        ui.add_space(GAP);
                        // "+" add-to-palette button
                        let (btn_rect, btn_resp) = ui.allocate_exact_size(Vec2::splat(ADD_BTN_W), egui::Sense::click());
                        let bg = if btn_resp.hovered() { theme.accent } else { theme.bg };
                        ui.painter().rect_filled(btn_rect, 0.0, bg);
                        let tint = if btn_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
                        ui.painter().text(
                            btn_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "+",
                            FontId::new(14.0, FontFamily::Proportional),
                            tint,
                        );
                        if btn_resp.clicked() {
                            for rgba in &ramp_rgba {
                                if !self.project.palette.contains(rgba) {
                                    self.project.palette.push(*rgba);
                                }
                            }
                        }
                    });
                }
                PickerMode::Rgb => {
                    let mut r = fg[0] as f32;
                    let mut g = fg[1] as f32;
                    let mut b = fg[2] as f32;
                    let mut changed = false;
                    changed |= value_slider(ui, &theme, "R", &mut r, 0.0..=255.0);
                    changed |= value_slider(ui, &theme, "G", &mut g, 0.0..=255.0);
                    changed |= value_slider(ui, &theme, "B", &mut b, 0.0..=255.0);
                    if changed {
                        self.color_state.foreground = [r as u8, g as u8, b as u8, fg[3]];
                    }
                }
            }
            ui.add_space(7.0);
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
                let icon_tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg };
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

                if swatch == self.color_state.foreground {
                    painter.rect_stroke(rect, 0.0, egui::Stroke::new(2.0, theme.fg), egui::StrokeKind::Inside);
                }

                let resp = ui.interact(rect, ui.id().with(("swatch", i)), egui::Sense::click_and_drag());
                if resp.drag_started() {
                    self.palette_drag_idx = Some(i);
                }
                if resp.clicked() {
                    self.color_state.foreground = swatch;
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

            // --- Drag ghost ---
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

            // --- Ctrl+Click anywhere in the palette grid to collapse ---
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
                    frame.layers.push(Layer::new(name.clone(), w, h, new_id));
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
        self.draw_ramp_size_menu(ctx);
        self.draw_slider_param_menu(ctx);
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

    fn draw_ramp_size_menu(&mut self, ctx: &egui::Context) {
        let Some((pos, opened_at)) = self.ramp_size_menu else { return; };
        let theme = self.theme.clone();
        let now = ctx.input(|i| i.time);
        let inner = egui::Area::new(egui::Id::new("ramp_size_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme.panel)
                    .inner_margin(Margin::same(4))
                    .shadow(egui::Shadow {
                        offset: [0, 14],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                    })
                    .show(ui, |ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                        ui.horizontal(|ui| {
                            ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                            ui.visuals_mut().override_text_color = Some(theme.fg_desc);
                            let mut n = self.color_state.ramp_size as u8;
                            ui.add_sized(
                                Vec2::new(32.0, 36.0),
                                egui::DragValue::new(&mut n).range(3..=9),
                            );
                            ui.visuals_mut().override_text_color = None;
                            self.color_state.ramp_size = n as usize;
                        });
                    });
            });
        let is_hovered = ctx.pointer_hover_pos()
            .map(|p| inner.response.rect.contains(p))
            .unwrap_or(false);
        let menu_id = egui::Id::new("ramp_size_menu_timer");
        let should_close = ctx.data_mut(|d| {
            let last: &mut f64 = d.get_temp_mut_or_insert_with(menu_id, || now);
            if now - opened_at < 0.15 || is_hovered { *last = now; false } else { now - *last > 2.0 }
        });
        ctx.request_repaint();
        if should_close || (now - opened_at > 0.15 && ctx.input(|i| i.pointer.any_click()) && !is_hovered) {
            self.ramp_size_menu = None;
        }
    }

    fn draw_slider_param_menu(&mut self, ctx: &egui::Context) {
        let Some((param, pos, opened_at)) = self.slider_param_menu else { return; };
        let theme = self.theme.clone();
        let now = ctx.input(|i| i.time);
        let inner = egui::Area::new(egui::Id::new("slider_param_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme.panel)
                    .inner_margin(Margin::same(4))
                    .shadow(egui::Shadow {
                        offset: [0, 14],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
                    })
                    .show(ui, |ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                        ui.horizontal(|ui| {
                            ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                            ui.visuals_mut().override_text_color = Some(theme.fg_desc);
                            match param {
                                SliderParam::HueShift => {
                                    let mut deg = self.color_state.hue_shift_deg;
                                    ui.add_sized(
                                        Vec2::new(52.0, 36.0),
                                        egui::DragValue::new(&mut deg)
                                            .range(0.0..=60.0)
                                            .speed(0.5)
                                            .suffix("°"),
                                    );
                                    self.color_state.hue_shift_deg = deg;
                                }
                                SliderParam::SatCurve => {
                                    let mut pct = (self.color_state.sat_curve_depth * 100.0).round() as i32;
                                    ui.add_sized(
                                        Vec2::new(52.0, 36.0),
                                        egui::DragValue::new(&mut pct)
                                            .range(0..=80)
                                            .suffix("%"),
                                    );
                                    self.color_state.sat_curve_depth = pct as f32 / 100.0;
                                }
                            }
                            ui.visuals_mut().override_text_color = None;
                        });
                    });
            });
        let is_hovered = ctx.pointer_hover_pos()
            .map(|p| inner.response.rect.contains(p))
            .unwrap_or(false);
        let menu_id = egui::Id::new("slider_param_menu_timer");
        let should_close = ctx.data_mut(|d| {
            let last: &mut f64 = d.get_temp_mut_or_insert_with(menu_id, || now);
            if now - opened_at < 0.15 || is_hovered { *last = now; false } else { now - *last > 2.0 }
        });
        ctx.request_repaint();
        if should_close || (now - opened_at > 0.15 && ctx.input(|i| i.pointer.any_click()) && !is_hovered) {
            self.slider_param_menu = None;
        }
    }

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
            let tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg };
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

    fn handle_canvas_input(&mut self, response: egui::Response, canvas_rect: egui::Rect) {
        let w = self.project.canvas_width;
        let h = self.project.canvas_height;
        let ai = self.project.active_animation;
        let fi = self.project.active_frame;
        let li = self.project.active_layer;

        let is_shape_tool = matches!(self.active_tool,
            ActiveTool::Rectangle { .. } | ActiveTool::Ellipse { .. } | ActiveTool::Line);

        // --- Resolve current pointer position ---
        // For shape drags: use the global interact_pos so tracking continues even when the
        // mouse leaves the central panel. Fall back to hover_pos (covers the release frame
        // where interact_pos may be None).
        // For stroke tools: require the pointer to be inside the canvas widget.
        let primary_down = response.ctx.input(|i| i.pointer.primary_down());
        let pos_opt: Option<egui::Pos2> = if is_shape_tool && self.drag_start.is_some() {
            // latest_pos() = raw last-known pointer position, always Some once the pointer
            // has been seen, regardless of which widget owns the interaction or panel bounds.
            response.ctx.input(|i| i.pointer.latest_pos())
        } else {
            response.interact_pointer_pos()
        };

        // --- drag_stopped must run even when pos is None (release outside window) ---
        // Also trigger commit when primary button is released globally but drag_stopped
        // didn't fire because the cursor left the central panel.
        let should_commit = response.drag_stopped()
            || (is_shape_tool && self.drag_start.is_some() && !primary_down);
        if should_commit {
            let color = self.color_state.foreground;
            self.shape_preview.clear();
            if !self.project.animations[ai].frames[fi].layers[li].locked
                && !self.project.animations[ai].frames[fi].layers[li].is_group
            {
                if let (Some((x0, y0)), Some(pos)) = (self.drag_start, pos_opt) {
                    let (epx, epy) = self.canvas.screen_to_canvas_i32(pos, canvas_rect, w, h);
                    let active_tool = self.active_tool.clone();
                    let shape_edits: Vec<_> = match &active_tool {
                        ActiveTool::Rectangle { filled } => {
                            apply_rect(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, epx, epy, color, *filled)
                        }
                        ActiveTool::Ellipse { filled } => {
                            let cx = (x0 as i32 + epx) / 2;
                            let cy = (y0 as i32 + epy) / 2;
                            let rx = (epx - x0 as i32).abs() / 2;
                            let ry = (epy - y0 as i32).abs() / 2;
                            apply_ellipse(&self.project.animations[ai].frames[fi].layers[li], cx, cy, rx, ry, color, *filled)
                        }
                        ActiveTool::Line => {
                            apply_line(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, epx, epy, color)
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

        // Stroke tools: require cursor to be inside the canvas.
        // For shape tools with an active drag the cursor may be outside — in that case we
        // skip the bounds check and only use shape_px/shape_py for the shape preview/commit.
        let (px, py) = if is_shape_tool && self.drag_start.is_some() {
            // Already have shape_px/shape_py; (px, py) unused for shape mid-drag arms.
            (0u32, 0u32)
        } else {
            let Some((px, py)) = self.canvas.screen_to_canvas(pos, canvas_rect, w, h) else { return; };
            if px >= w || py >= h { return; }
            (px, py)
        };

        if self.project.animations[ai].frames[fi].layers[li].locked { return; }
        if self.project.animations[ai].frames[fi].layers[li].is_group { return; }

        if response.drag_started() {
            self.drag_start = Some((px, py));
            self.stroke_edits.clear();
            self.shape_preview.clear();
        }

        let color = self.color_state.foreground;
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

        // Shape tools: recompute preview every frame while button is held and drag is active.
        // Use primary_down (global button state) instead of response.dragged() so the
        // preview keeps updating even when the cursor moves outside the central panel.
        if is_shape_tool && self.drag_start.is_some() && primary_down {
            // Keep requesting repaints so the preview stays live outside the panel.
            response.ctx.request_repaint();
            if let Some((x0, y0)) = self.drag_start {
                let active_tool = self.active_tool.clone();
                let preview_edits: Vec<_> = match &active_tool {
                    ActiveTool::Rectangle { filled } => {
                        apply_rect(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, shape_px, shape_py, color, *filled)
                    }
                    ActiveTool::Ellipse { filled } => {
                        let cx = (x0 as i32 + shape_px) / 2;
                        let cy = (y0 as i32 + shape_py) / 2;
                        let rx = (shape_px - x0 as i32).abs() / 2;
                        let ry = (shape_py - y0 as i32).abs() / 2;
                        apply_ellipse(&self.project.animations[ai].frames[fi].layers[li], cx, cy, rx, ry, color, *filled)
                    }
                    ActiveTool::Line => {
                        apply_line(&self.project.animations[ai].frames[fi].layers[li], x0 as i32, y0 as i32, shape_px, shape_py, color)
                    }
                    _ => vec![],
                };
                self.shape_preview = preview_edits.into_iter().map(|(x, y, _old, new)| (x, y, new)).collect();
                self.canvas_dirty = true;
            }
        }
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
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = LayoutState {
            ui_state: self.ui_state.clone(),
            sidebar_order: self.sidebar_order.clone(),
        };
        if let Ok(json) = serde_json::to_string(&state) {
            storage.set_string(LAYOUT_STORAGE_KEY, json);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.theme.apply(ctx);

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

        let ctrl_z = ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl);
        let ctrl_y = ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl);
        if ctrl_z {
            self.undo_stack.undo(&mut self.project);
            self.canvas_dirty = true;
        }
        if ctrl_y {
            self.undo_stack.redo(&mut self.project);
            self.canvas_dirty = true;
        }

        self.draw_top_bar(ctx);
        self.draw_anim_toolbar(ctx);  // full-width — must be first bottom panel
        self.draw_right_sidebar(ctx); // claims full right column (topbar → anim toolbar)
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

fn draw_logo(ui: &mut egui::Ui, theme: &Theme) {
    ui.allocate_ui_with_layout(Vec2::new(BRAND_WIDTH, TOP_BAR_HEIGHT), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.add_space(10.0);
        ui.add(
            Image::new(egui::include_image!("../assets/logo.png"))
                .fit_to_exact_size(Vec2::splat(20.0)),
        );
        ui.add_space(7.0);
        ui.label(
            RichText::new("SQUAREZ")
                .color(theme.fg)
                .font(FontId::new(MENU_FONT_SIZE, FontFamily::Name("bold".into()))),
        );
    });
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

fn dropdown_separator(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(DROPDOWN_WIDTH, 9.0), egui::Sense::hover());
    let y = rect.center().y;
    ui.painter().line_segment(
        [Pos2::new(rect.left() + 14.0, y), Pos2::new(rect.right() - 14.0, y)],
        egui::Stroke::new(1.0, theme.surface),
    );
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
        let icon_tint = if icon_resp.hovered() { Color32::WHITE } else { theme.fg };
        ui.put(icon_rect, Image::new(icon).tint(icon_tint).fit_to_exact_size(icon_size));
        if icon_resp.clicked() {
            state.toggle_collapsed(panel);
        }
        if !collapsed {
            // Right: "+" add button
            let plus_rect = egui::Rect::from_center_size(Pos2::new(rect.right() - 8.0, rect.center().y), Vec2::splat(16.0));
            let plus_resp = ui.interact(plus_rect, egui::Id::new(("hdr_plus", panel)), egui::Sense::click());
            let plus_color = if plus_resp.hovered() { Color32::WHITE } else { theme.fg_desc };
            painter.text(plus_rect.center(), egui::Align2::CENTER_CENTER, "+", FontId::new(16.0, FontFamily::Proportional), plus_color);
            if plus_resp.clicked() {
                add_clicked = true;
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

/// Slider with optional quantized snapping.
/// - Ctrl+click on track toggles `snap_on` (does NOT change value on that click).
/// - When `snap_on`: value snaps to the nearest of `n_steps` evenly spaced positions
///   across `snap_range` (which may be a subrange of `range`).
/// - When `snap_on`: tick marks are drawn at each step position on the track.
/// Returns true if `value` or `snap_on` changed (caller should treat either as a redraw trigger;
/// value-change is the only one that should rewrite the color).
/// Outcome of a slider tick.
struct SliderOut {
    /// Value or snap state changed this frame.
    changed: bool,
    /// If the slider's label was right-clicked this frame, contains the screen-space click position.
    label_rclick: Option<Pos2>,
}

fn value_slider_snap(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    snap_on: &mut bool,
    snap_range: (f32, f32),
    n_steps: usize,
) -> SliderOut {
    ui.horizontal(|ui| {
        // Label as its own clickable rect (so we can detect right-click on it).
        const LABEL_W: f32 = 8.0;
        let (label_rect, label_resp) = ui.allocate_exact_size(
            Vec2::new(LABEL_W, COLOR_SLIDER_TRACK_HEIGHT),
            egui::Sense::click(),
        );
        ui.painter().text(
            label_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
            theme.fg_muted,
        );
        let label_rclick = if label_resp.secondary_clicked() {
            Some(label_resp.rect.left_bottom())
        } else { None };

        ui.add_space(4.0);
        let desired_size = Vec2::new((ui.available_width() - 4.0).max(24.0), COLOR_SLIDER_TRACK_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
        let start = *range.start();
        let end = *range.end();
        let span = end - start;
        if span <= 0.0 { return SliderOut { changed: false, label_rclick }; }

        let t = ((*value - start) / span).clamp(0.0, 1.0);
        ui.painter().rect_filled(rect, 0.0, theme.bg);
        let fill_rect = egui::Rect::from_min_max(
            rect.left_top(),
            Pos2::new(rect.left() + rect.width() * t, rect.bottom()),
        );
        ui.painter().rect_filled(fill_rect, 0.0, theme.accent);

        if *snap_on && n_steps >= 2 {
            let (sn_lo, sn_hi) = snap_range;
            for i in 0..n_steps {
                let s_val = sn_lo + (sn_hi - sn_lo) * (i as f32 / (n_steps - 1) as f32);
                let s_t = ((s_val - start) / span).clamp(0.0, 1.0);
                let x = rect.left() + rect.width() * s_t;
                ui.painter().line_segment(
                    [Pos2::new(x, rect.bottom() - 3.0), Pos2::new(x, rect.bottom())],
                    egui::Stroke::new(1.0, theme.fg_muted),
                );
            }
        }

        let mut changed = false;
        if let Some(pos) = response.interact_pointer_pos() {
            if response.secondary_clicked() {
                *snap_on = !*snap_on;
                changed = true;
            } else if response.dragged() || response.clicked() {
                let new_t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let mut new_v = start + span * new_t;
                if *snap_on && n_steps >= 2 {
                    let (sn_lo, sn_hi) = snap_range;
                    let step = (sn_hi - sn_lo) / (n_steps - 1) as f32;
                    let idx = ((new_v - sn_lo) / step).round().clamp(0.0, (n_steps - 1) as f32);
                    new_v = sn_lo + step * idx;
                }
                if (new_v - *value).abs() > f32::EPSILON {
                    *value = new_v;
                    changed = true;
                }
            }
        }
        SliderOut { changed, label_rclick }
    }).inner
}

fn value_slider(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) -> bool {    ui.horizontal(|ui| {
        ui.label(rich(label, theme.fg_muted, FONT_SIZE_SM));
        ui.add_space(4.0); // gap between label letter and slider track
        let desired_size = Vec2::new((ui.available_width() - 4.0).max(24.0), COLOR_SLIDER_TRACK_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
        let start = *range.start();
        let end = *range.end();
        let span = end - start;
        if span > 0.0 {
            let t = ((*value - start) / span).clamp(0.0, 1.0);
            ui.painter().rect_filled(rect, 0.0, theme.bg);
            let fill_rect = egui::Rect::from_min_max(
                rect.left_top(),
                Pos2::new(rect.left() + rect.width() * t, rect.bottom()),
            );
            ui.painter().rect_filled(fill_rect, 0.0, theme.accent);

            if let Some(pos) = response.interact_pointer_pos() {
                if response.dragged() || response.clicked() {
                    let new_t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    *value = start + span * new_t;
                    return true;
                }
            }
        }
        false
    }).inner
}

fn tab_button(ui: &mut egui::Ui, theme: &Theme, selected: bool, label: &str) -> egui::Response {
    // Width = hex outer (94 inner + 6*2 margin = 106) / 3 tabs, spacing already 0.
    const TAB_W: f32 = (94.0 + 12.0) / 3.0;
    // Use allocate_exact_size so the FULL rect (including text pixels) owns the response.
    // egui::Button only senses over its background, missing clicks on text glyphs.
    let (rect, response) = ui.allocate_exact_size(Vec2::new(TAB_W, 18.0), egui::Sense::click());
    let bg = if selected { theme.accent } else if response.hovered() { theme.surface } else { theme.surface };
    ui.painter().rect_filled(rect, 0.0, bg);
    let text_color = if selected { theme.fg } else { theme.fg_desc };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(FONT_SIZE_SM, FontFamily::Proportional),
        text_color,
    );
    response
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

fn flat_button(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    ui.add_sized(
        Vec2::new(16.0, 16.0),
        egui::Button::new(rich(label, theme.fg_desc, FONT_SIZE_SM))
            .fill(theme.panel)
            .stroke(egui::Stroke::NONE),
    )
}

fn menu_item(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(rich(label, theme.fg_desc, FONT_SIZE_SM))
            .fill(theme.panel)
            .stroke(egui::Stroke::NONE)
            .min_size(Vec2::new(132.0, 28.0)),
    )
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

fn rfd_save() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Squarez Project", &["sqr"])
        .save_file()
}
