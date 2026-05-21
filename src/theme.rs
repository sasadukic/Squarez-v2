// src/theme.rs
//
// Approved Blue Slate low-outline UI palette.

use egui::{Color32, FontData, FontDefinitions, FontFamily, Style, Visuals};

pub const FONT_SIZE_SM: f32 = 11.0;
pub const FONT_SIZE_MD: f32 = 16.0;
pub const FONT_SIZE_LG: f32 = 32.0;

#[derive(Debug, Clone)]
pub struct Theme {
    /// #171B22 - main window / canvas background
    pub bg:       Color32,
    /// #242B35 - panels, menu bar, timeline
    pub panel:    Color32,
    /// #343B48 - buttons, inactive widget fills
    pub surface:  Color32,
    /// #4B5667 - active selections
    pub accent:   Color32,
    /// #343B48 - low-contrast separators
    pub border:   Color32,
    /// #8792A3 - muted / disabled
    pub muted:    Color32,
    /// #DDE7F5 - primary text
    pub fg:       Color32,
    /// alias for fg_muted - secondary / description text
    pub fg_desc:  Color32,
    /// #8792A3 - muted text (placeholders, dim labels)
    pub fg_muted: Color32,
    /// Canvas checkerboard dark tile
    pub checker_dark: Color32,
    /// Canvas checkerboard light tile
    pub checker_light: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg:            Color32::from_hex("#171B22").unwrap(),
            panel:         Color32::from_hex("#242B35").unwrap(),
            surface:       Color32::from_hex("#343B48").unwrap(),
            accent:        Color32::from_hex("#4B5667").unwrap(),
            border:        Color32::from_hex("#343B48").unwrap(),
            muted:         Color32::from_hex("#8792A3").unwrap(),
            fg:            Color32::from_hex("#DDE7F5").unwrap(),
            fg_desc:       Color32::from_hex("#8792A3").unwrap(),
            fg_muted:      Color32::from_hex("#8792A3").unwrap(),
            checker_dark:  Color32::from_hex("#AAB2C2").unwrap(),
            checker_light: Color32::from_hex("#C5CCDA").unwrap(),
        }
    }
}

impl Theme {
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();

        // ── Background layers ─────────────────────────────────────────────
        // Central panel (canvas) → bg  (darkest — set per-panel in app.rs)
        // Side panels / bars     → panel
        visuals.panel_fill           = self.panel;
        visuals.window_fill          = self.panel;
        visuals.extreme_bg_color     = self.bg;
        visuals.faint_bg_color       = self.panel;
        visuals.code_bg_color        = self.surface;

        // ── Widget states: noninteractive → inactive → hovered → active ───
        visuals.widgets.noninteractive.bg_fill         = self.panel;
        visuals.widgets.inactive.bg_fill               = self.panel;
        visuals.widgets.hovered.bg_fill                = self.accent;
        visuals.widgets.active.bg_fill                 = self.surface;
        visuals.widgets.open.bg_fill                   = self.surface;

        // weak_bg_fill is used by Button/DragValue (NOT bg_fill).
        // Without setting these, DragValue shows the default gray from Visuals::dark().
        visuals.widgets.noninteractive.weak_bg_fill    = self.panel;
        visuals.widgets.inactive.weak_bg_fill          = self.panel;
        visuals.widgets.hovered.weak_bg_fill           = self.accent;
        visuals.widgets.active.weak_bg_fill            = self.surface;
        visuals.widgets.open.weak_bg_fill              = self.surface;

        // ── Widget corner radius: flat/square everywhere ──────────────────
        let sq = egui::CornerRadius::ZERO;
        visuals.widgets.noninteractive.corner_radius   = sq;
        visuals.widgets.inactive.corner_radius         = sq;
        visuals.widgets.hovered.corner_radius          = sq;
        visuals.widgets.active.corner_radius           = sq;
        visuals.widgets.open.corner_radius             = sq;
        visuals.window_corner_radius                   = sq;
        visuals.menu_corner_radius                     = sq;

        // ── Widget foreground strokes ─────────────────────────────────────
        visuals.widgets.noninteractive.fg_stroke.color = self.fg_desc;
        visuals.widgets.inactive.fg_stroke.color       = self.fg_desc;
        visuals.widgets.hovered.fg_stroke.color        = self.fg;
        visuals.widgets.active.fg_stroke.color         = self.fg;
        visuals.widgets.open.fg_stroke.color           = self.fg;

        // ── Widget border strokes ─────────────────────────────────────────
        visuals.widgets.noninteractive.bg_stroke.color = self.panel;
        visuals.widgets.noninteractive.bg_stroke.width = 0.0;
        visuals.widgets.inactive.bg_stroke.color       = self.border;
        visuals.widgets.inactive.bg_stroke.width       = 0.0;
        visuals.widgets.hovered.bg_stroke.color        = self.muted;
        visuals.widgets.hovered.bg_stroke.width        = 0.0;
        visuals.widgets.active.bg_stroke.color         = self.fg_muted;
        visuals.widgets.active.bg_stroke.width         = 0.0;

        // ── Selection ─────────────────────────────────────────────────────
        visuals.selection.bg_fill      = self.accent;
        visuals.selection.stroke.color = self.fg;

        // ── Window chrome ─────────────────────────────────────────────────
        visuals.window_stroke.color    = self.border;
        // ── Shadows (suppress for flat pixel-editor aesthetic) ────────────
        visuals.popup_shadow  = egui::epaint::Shadow::NONE;
        visuals.window_shadow = egui::epaint::Shadow::NONE;

        // ── Text override ─────────────────────────────────────────────────
        visuals.override_text_color = Some(self.fg);

        let mut style = Style::default();
        style.visuals = visuals;

        // Compact spacing — keeps toolbar/panels tight
        style.spacing.item_spacing   = egui::Vec2::new(4.0, 4.0);
        style.spacing.button_padding = egui::Vec2::new(4.0, 3.0);

        style.override_font_id = Some(egui::FontId::new(FONT_SIZE_SM, FontFamily::Proportional));
        ctx.set_style(style);
    }
}

pub fn load_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "roboto".to_owned(),
        FontData::from_static(include_bytes!("../assets/Roboto.ttf")).into(),
    );
    fonts.font_data.insert(
        "roboto_bold".to_owned(),
        FontData::from_static(include_bytes!("../assets/Roboto-Bold.ttf")).into(),
    );
    fonts.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "roboto".to_owned());
    // True bold family — used by RichText::strong()
    fonts.families
        .entry(FontFamily::Name("bold".into()))
        .or_default()
        .insert(0, "roboto_bold".to_owned());
    fonts.families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "roboto".to_owned());
    ctx.set_fonts(fonts);
}
