use squarez::top_bar::{
    menu_zone_width, BRAND_WIDTH, DROPDOWN_CORNER_RADIUS, DROPDOWN_ROW_HEIGHT, DROPDOWN_TOP_GAP,
    DROPDOWN_WIDTH, MENU_FONT_SIZE, MENU_HORIZONTAL_PADDING, SELECTED_MENU_HAS_FILL, TOP_BAR_HEIGHT,
};

#[test]
fn top_bar_uses_mockup_geometry_tokens() {
    assert_eq!(TOP_BAR_HEIGHT, 36.0);
    assert_eq!(BRAND_WIDTH, 120.0);
    assert_eq!(MENU_HORIZONTAL_PADDING, 22.0);
    assert_eq!(MENU_FONT_SIZE, 11.0);
    assert_eq!(DROPDOWN_WIDTH, 184.0);
    assert_eq!(DROPDOWN_ROW_HEIGHT, 38.0);
    assert_eq!(DROPDOWN_CORNER_RADIUS, 0);
    assert_eq!(DROPDOWN_TOP_GAP, 0.0);
    // Mockup: .menu-item.active { background:#343B48 } — active item must show surface fill
    assert!(SELECTED_MENU_HAS_FILL);
}

#[test]
fn menu_zone_width_includes_mockup_horizontal_padding() {
    assert_eq!(menu_zone_width("File"), 68.0);
    assert_eq!(menu_zone_width("Animation"), 98.0);
}
