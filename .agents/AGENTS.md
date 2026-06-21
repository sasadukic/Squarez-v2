# Style Rules

## Hover Tooltips and Popups Style Unification
All hover tooltips, dropdown menus, context submenus, floating windows, and modal popups in this project must follow a unified styling rule:
1. **Curved Corners**: Must have `6px` corner radius (equal to `egui::CornerRadius::same(6)`), which represents approximately 2% of the standard window width.
2. **Drop Shadow**: Must have a drop shadow configured with the standard parameters:
   - Offset: `[0, 14]`
   - Blur: `36`
   - Spread: `0`
   - Color: `Color32::from_rgba_unmultiplied(0, 0, 0, 89)`
3. **No Outline**: Must have no border outline (stroke width `0` or `egui::Stroke::NONE`).
