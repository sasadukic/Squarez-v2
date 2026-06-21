#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Panel {
    Color,
    Palette,
    Preview,
    Layers,
    Animations,
    Timeline,
    Tiles,
    Brushes,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiState {
    pub show_color: bool,
    pub show_palette: bool,
    pub show_preview: bool,
    pub show_layers: bool,
    pub show_animations: bool,
    pub show_timeline: bool,
    pub show_tiles: bool,
    pub show_brushes: bool,
    pub collapse_color: bool,
    pub collapse_palette: bool,
    pub collapse_preview: bool,
    pub collapse_layers: bool,
    pub collapse_animations: bool,
    pub collapse_tiles: bool,
    pub collapse_brushes: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_color: true,
            show_palette: true,
            show_preview: true,
            show_layers: true,
            show_animations: true,
            show_timeline: true,
            show_tiles: true,
            show_brushes: true,
            collapse_color: false,
            collapse_palette: false,
            collapse_preview: false,
            collapse_layers: false,
            collapse_animations: false,
            collapse_tiles: false,
            collapse_brushes: false,
        }
    }
}

impl UiState {
    pub fn is_visible(&self, panel: Panel) -> bool {
        match panel {
            Panel::Color => self.show_color,
            Panel::Palette => self.show_palette,
            Panel::Preview => self.show_preview,
            Panel::Layers => self.show_layers,
            Panel::Animations => self.show_animations,
            Panel::Timeline => self.show_timeline,
            Panel::Tiles => self.show_tiles,
            Panel::Brushes => self.show_brushes,
        }
    }

    pub fn is_collapsed(&self, panel: Panel) -> bool {
        match panel {
            Panel::Color => self.collapse_color,
            Panel::Palette => self.collapse_palette,
            Panel::Preview => self.collapse_preview,
            Panel::Layers => self.collapse_layers,
            Panel::Animations => self.collapse_animations,
            Panel::Timeline => false,
            Panel::Tiles => self.collapse_tiles,
            Panel::Brushes => self.collapse_brushes,
        }
    }

    pub fn toggle_visible(&mut self, panel: Panel) {
        *self.visible_mut(panel) = !self.is_visible(panel);
    }

    pub fn toggle_collapsed(&mut self, panel: Panel) {
        if panel == Panel::Timeline {
            return;
        }
        *self.collapsed_mut(panel) = !self.is_collapsed(panel);
    }

    pub fn close(&mut self, panel: Panel) {
        *self.visible_mut(panel) = false;
    }

    fn visible_mut(&mut self, panel: Panel) -> &mut bool {
        match panel {
            Panel::Color => &mut self.show_color,
            Panel::Palette => &mut self.show_palette,
            Panel::Preview => &mut self.show_preview,
            Panel::Layers => &mut self.show_layers,
            Panel::Animations => &mut self.show_animations,
            Panel::Timeline => &mut self.show_timeline,
            Panel::Tiles => &mut self.show_tiles,
            Panel::Brushes => &mut self.show_brushes,
        }
    }

    fn collapsed_mut(&mut self, panel: Panel) -> &mut bool {
        match panel {
            Panel::Color => &mut self.collapse_color,
            Panel::Palette => &mut self.collapse_palette,
            Panel::Preview => &mut self.collapse_preview,
            Panel::Layers => &mut self.collapse_layers,
            Panel::Animations => &mut self.collapse_animations,
            Panel::Timeline => panic!("timeline is hideable from the Windows menu but not collapsible"),
            Panel::Tiles => &mut self.collapse_tiles,
            Panel::Brushes => &mut self.collapse_brushes,
        }
    }
}
