// src/theme.rs
//
// Color system — "Shades of gray" palette (from design spec):
//
//   black-100  #0A0E15  — main window / canvas bg      (OLED near-black)
//   black-90   #212631  — panels, menu bar, timeline    (raised surface)
//   black-80   #373F4E  — buttons, inactive widgets     (interactive surface)
//   black-70   #4E576A  — borders, separators           (subtle edge)
//   black-60   #667085  — muted / disabled              (receded element)
//   white-100  #FFFFFF  — primary text  (d.header)      (AAA 19:1)
//   white-80   #E0E4EB  — secondary text (d.description)(AAA 15:1)
//   white-60   #BFC6D4  — muted / placeholder text      (AA 11:1)

use egui::{Color32, FontData, FontDefinitions, FontFamily, Style, Visuals};

pub const FONT_SIZE_SM: f32 = 8.0;
pub const FONT_SIZE_MD: f32 = 16.0;
pub const FONT_SIZE_LG: f32 = 32.0;

#[derive(Debug, Clone)]
pub struct Theme {
    /// #0A0E15 — main window / canvas background
    pub bg:       Color32,
    /// #212631 — panels, menu bar, timeline
    pub panel:    Color32,
    /// #373F4E — buttons, inactive widget fills
    pub surface:  Color32,
    /// #4E576A — borders, separators
    pub border:   Color32,
    /// #667085 — muted / disabled
    pub muted:    Color32,
    /// #FFFFFF — primary text (d.header)
    pub fg:       Color32,
    /// #E0E4EB — secondary / description text
    pub fg_desc:  Color32,
    /// #BFC6D4 — muted text (placeholders, dim labels)
    pub fg_muted: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg:       Color32::from_hex("#0A0E15").unwrap(),
            panel:    Color32::from_hex("#212631").unwrap(),
            surface:  Color32::from_hex("#373F4E").unwrap(),
            border:   Color32::from_hex("#4E576A").unwrap(),
            muted:    Color32::from_hex("#667085").unwrap(),
            fg:       Color32::from_hex("#FFFFFF").unwrap(),
            fg_desc:  Color32::from_hex("#E0E4EB").unwrap(),
            fg_muted: Color32::from_hex("#BFC6D4").unwrap(),
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
        visuals.widgets.inactive.bg_fill               = self.surface;
        visuals.widgets.hovered.bg_fill                = self.border;
        visuals.widgets.active.bg_fill                 = self.muted;
        visuals.widgets.open.bg_fill                   = self.surface;

        // ── Widget foreground strokes ─────────────────────────────────────
        visuals.widgets.noninteractive.fg_stroke.color = self.fg_desc;
        visuals.widgets.inactive.fg_stroke.color       = self.fg_desc;
        visuals.widgets.hovered.fg_stroke.color        = self.fg;
        visuals.widgets.active.fg_stroke.color         = self.fg;
        visuals.widgets.open.fg_stroke.color           = self.fg;

        // ── Widget border strokes ─────────────────────────────────────────
        visuals.widgets.noninteractive.bg_stroke.color = self.border;
        visuals.widgets.noninteractive.bg_stroke.width = 1.0;
        visuals.widgets.inactive.bg_stroke.color       = self.border;
        visuals.widgets.inactive.bg_stroke.width       = 1.0;
        visuals.widgets.hovered.bg_stroke.color        = self.muted;
        visuals.widgets.hovered.bg_stroke.width        = 1.0;
        visuals.widgets.active.bg_stroke.color         = self.fg_muted;
        visuals.widgets.active.bg_stroke.width         = 1.0;

        // ── Selection ─────────────────────────────────────────────────────
        visuals.selection.bg_fill      = self.surface;
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

        style.override_font_id = Some(egui::FontId::new(FONT_SIZE_SM, FontFamily::Monospace));
        ctx.set_style(style);
    }
}

pub fn load_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "dogicapixel".to_owned(),
        FontData::from_static(include_bytes!("../assets/dogicapixel.ttf")).into(),
    );
    fonts.families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "dogicapixel".to_owned());
    fonts.families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "dogicapixel".to_owned());
    ctx.set_fonts(fonts);
}
