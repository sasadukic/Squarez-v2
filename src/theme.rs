// src/theme.rs
use egui::{Color32, FontData, FontDefinitions, FontFamily, Style, Visuals};

pub const FONT_SIZE_SM: f32 = 8.0;
pub const FONT_SIZE_MD: f32 = 16.0;
pub const FONT_SIZE_LG: f32 = 32.0;

#[derive(Debug, Clone)]
pub struct Theme {
    pub bg:    Color32,
    pub mid:   Color32,
    pub light: Color32,
    pub fg:    Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg:    Color32::from_hex("#000000").unwrap(),
            mid:   Color32::from_hex("#676767").unwrap(),
            light: Color32::from_hex("#b6b6b6").unwrap(),
            fg:    Color32::from_hex("#ffffff").unwrap(),
        }
    }
}

impl Theme {
    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();

        visuals.panel_fill                             = self.bg;
        visuals.window_fill                            = self.bg;
        visuals.extreme_bg_color                       = self.bg;
        visuals.code_bg_color                          = self.mid;
        visuals.faint_bg_color                         = self.mid;
        visuals.widgets.noninteractive.bg_fill         = self.mid;
        visuals.widgets.inactive.bg_fill               = self.mid;
        visuals.widgets.hovered.bg_fill                = self.light;
        visuals.widgets.active.bg_fill                 = self.fg;
        visuals.widgets.noninteractive.fg_stroke.color = self.fg;
        visuals.widgets.inactive.fg_stroke.color       = self.fg;
        visuals.widgets.hovered.fg_stroke.color        = self.bg;
        visuals.widgets.active.fg_stroke.color         = self.bg;
        visuals.selection.bg_fill                      = self.light;
        visuals.selection.stroke.color                 = self.bg;
        visuals.window_stroke.color                    = self.light;
        visuals.widgets.noninteractive.bg_stroke.color = self.light;

        let mut style = Style::default();
        style.visuals = visuals;
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
