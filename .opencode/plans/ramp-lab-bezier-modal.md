# Ramp Lab: Bezier, full modal, top-to-bottom curves

## Changes

### 1. `src/app.rs` — draw_ramp_lab: shrink swatches, extend curve rect to canvas top

Replace:
```rust
let swatch_h = 30.0;
let box_size = 20.0;
for (i, rgba) in ramp_rgba.iter().enumerate() {
    let x = canvas_rect.left() + cell_w * i as f32 + (cell_w - box_size) * 0.5;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(Pos2::new(x, canvas_rect.top() + 6.0), Vec2::splat(box_size)),
        3.0,
        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], 255),
    );
}
let curve_rect = egui::Rect::from_min_max(
    Pos2::new(canvas_rect.left() + 8.0, canvas_rect.top() + swatch_h + 8.0),
    Pos2::new(canvas_rect.right() - 8.0, canvas_rect.bottom() - 8.0),
);
```

With:
```rust
let swatch_h = 16.0;
let box_size = 12.0;
for (i, rgba) in ramp_rgba.iter().enumerate() {
    let x = canvas_rect.left() + cell_w * i as f32 + (cell_w - box_size) * 0.5;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(Pos2::new(x, canvas_rect.top() + 2.0), Vec2::splat(box_size)),
        2.0,
        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], 255),
    );
}
let curve_rect = egui::Rect::from_min_max(
    Pos2::new(canvas_rect.left() + 4.0, canvas_rect.top() + 2.0),
    Pos2::new(canvas_rect.right() - 4.0, canvas_rect.bottom() - 4.0),
);
```

Effect: curve rect now starts 2px from canvas top (was 38px). Handle at y=1.0 reaches nearly top edge.

### 2. `src/app.rs` — draw_ramp_lab: add full-screen event blocker

Add BEFORE the modal Area, AFTER the dim layer:

```rust
// Full-screen event blocker — absorbs clicks, drags, scrolls outside the modal
egui::Area::new(egui::Id::new("ramp_lab_blocker"))
    .order(egui::Order::Foreground)
    .fixed_pos(screen_rect.min)
    .show(ctx, |ui| {
        ui.allocate_rect(screen_rect, egui::Sense::click_and_drag());
    });
```

This blocks all mouse interactions on the background.

### 3. `src/app.rs` — guard canvas + zoom input with ramp_lab_open

Around line 3165 (in draw_canvas_panel):
```rust
// From:
self.canvas.handle_input(ui, canvas_rect);

// To:
if !self.ramp_lab_open {
    self.canvas.handle_input(ui, canvas_rect);
}
```

Around line 3236-3239:
```rust
// From:
if self.active_tool == ActiveTool::Zoom {
    self.handle_zoom_tool_input(&response, canvas_rect);
} else {
    self.handle_canvas_input(response, canvas_rect);
}

// To:
if !self.ramp_lab_open {
    if self.active_tool == ActiveTool::Zoom {
        self.handle_zoom_tool_input(&response, canvas_rect);
    } else {
        self.handle_canvas_input(response, canvas_rect);
    }
}
```

### 4. `src/app.rs` — guard timeline scroll with ramp_lab_open

Around line 2498-2499:
```rust
// From:
if hovered {
    let delta = ctx.input(|i| i.raw_scroll_delta.y);
    ...

// To:
if !self.ramp_lab_open && hovered {
    let delta = ctx.input(|i| i.raw_scroll_delta.y);
    ...
```

### 5. Build & test
```bash
pkill -f squarez 2>/dev/null; cargo build && nohup ./target/debug/squarez > /dev/null 2>&1 &
```
