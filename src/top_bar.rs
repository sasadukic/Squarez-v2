pub const TOP_BAR_HEIGHT: f32 = 36.0;
pub const BRAND_WIDTH: f32 = 120.0;
pub const MENU_LEFT_GAP: f32 = 16.0; // padding between logo and first menu item
pub const MENU_HORIZONTAL_PADDING: f32 = 22.0;
pub const MENU_FONT_SIZE: f32 = 11.0;
pub const DROPDOWN_WIDTH: f32 = 184.0;
pub const DROPDOWN_ROW_HEIGHT: f32 = 38.0;
pub const DROPDOWN_CORNER_RADIUS: u8 = 0;
pub const DROPDOWN_TOP_GAP: f32 = 0.0;
pub const SELECTED_MENU_HAS_FILL: bool = true;

pub fn menu_zone_width(label: &str) -> f32 {
    let text_width = label.chars().count() as f32 * 6.0;
    text_width + MENU_HORIZONTAL_PADDING * 2.0
}
