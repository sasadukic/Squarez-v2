// src/ui_widgets.rs
//
// Shared UI primitives: reusable menu button and unified context menu state.

use egui::{Color32, Image, ImageSource, Pos2, Vec2};
use crate::theme::Theme;

/// Standard button size used across all context menus.
pub const MENU_BTN: f32 = 36.0;
/// Standard padding between menu buttons.
pub const MENU_PAD: f32 = 4.0;
/// Icon size rendered inside a menu button.
const ICON_SIZE: f32 = 20.0;

/// Renders a square icon button used in all context menus.
///
/// Returns `true` if the button was clicked.
///
/// - `enabled = true`  → hover highlight, white tint on hover, click registered.
/// - `enabled = false` → muted icon, no hover fill, no click.
pub fn menu_icon_btn(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: ImageSource<'static>,
    enabled: bool,
) -> bool {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(MENU_BTN), sense);
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, 0.0, theme.accent);
    }
    let icon_rect = egui::Rect::from_center_size(r.center(), Vec2::splat(ICON_SIZE));
    let tint = if !enabled {
        theme.fg_muted
    } else if resp.hovered() {
        Color32::WHITE
    } else {
        theme.fg_desc
    };
    ui.put(icon_rect, Image::new(icon).tint(tint).fit_to_exact_size(Vec2::splat(ICON_SIZE)));
    resp.clicked()
}

/// Standard menu frame shadow used by all context menus.
fn menu_shadow() -> egui::Shadow {
    egui::Shadow {
        offset: [0, 14],
        blur: 36,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(0, 0, 0, 89),
    }
}

/// Result of a single [`show_context_menu`] frame.
pub enum MenuOutcome {
    /// Menu should stay open; here is the action the user clicked (if any).
    Open(Option<u8>),
    /// Menu should close; here is the action the user clicked (if any).
    Close(Option<u8>),
}

impl MenuOutcome {
    /// Returns the action id if one was taken this frame, regardless of close/open.
    pub fn action(&self) -> Option<u8> {
        match self {
            Self::Open(a) | Self::Close(a) => *a,
        }
    }

    /// Returns `true` if the menu should be closed.
    pub fn should_close(&self) -> bool {
        matches!(self, Self::Close(_))
    }
}

/// Unified context menu state and close logic.
///
/// Every context menu in the app should use this instead of hand-rolling
/// its own open/close/timer logic.  The rules are:
///
/// 1. **Age guard** — ignore clicks for the first 0.15 s after opening
///    (the right-click that opened the menu is still in the input buffer).
/// 2. **Action closes** — if the content closure returns `Some(action)`, close immediately.
/// 3. **Click outside closes** — after the age guard, any click outside the menu rect closes it.
/// 4. **Hover-away timer** — when the pointer leaves the menu (and optional extra hover rect),
///    a 2-second countdown starts.  Moving back resets it.  Expiry closes the menu.
/// 5. **Escape closes** — pressing Escape closes immediately.
pub struct ContextMenuState {
    /// Where to position the menu (top-left corner, adjusted by caller).
    pos: Pos2,
    /// Timestamp when the menu was opened.
    opened_at: f64,
    /// Last time the pointer was inside the menu or extra hover zone.
    last_hover: f64,
    /// Whether the hover timer has been seeded (first frame).
    timer_seeded: bool,
}

impl ContextMenuState {
    /// Seconds after opening during which outside clicks are ignored.
    const AGE_GUARD: f64 = 0.15;
    /// Seconds the pointer must leave all hover zones before auto-close.
    const HOVER_TIMEOUT: f64 = 2.0;

    /// Create a new context menu state at the given screen position.
    pub fn new(pos: Pos2, now: f64) -> Self {
        Self {
            pos,
            opened_at: now,
            last_hover: now,
            timer_seeded: false,
        }
    }

    /// Returns the position this menu was opened at.
    pub fn pos(&self) -> Pos2 {
        self.pos
    }
}

/// Show a context menu for one frame and return the [`MenuOutcome`].
///
/// This is a standalone function (not a method) so that callers can temporarily
/// `.take()` the `Option<ContextMenuState>` from their struct, pass it here, and
/// then decide whether to put it back — avoiding borrow conflicts with the rest
/// of the caller's state.
///
/// - `state` — mutable reference to the menu's state (typically taken from an `Option`).
/// - `ctx` — egui context.
/// - `theme` — current theme.
/// - `menu_id` — unique egui ID string for this menu instance.
/// - `extra_hover` — optional screen rect that also counts as "hovering the menu"
///   (e.g. the canvas rect for the brush size popup).  When the pointer is inside
///   this rect the hover-away timer is also reset.
/// - `content` — renders the menu body; call [`menu_icon_btn`] inside and return
///   `Some(action_id)` on click.
pub fn show_context_menu(
    state: &mut ContextMenuState,
    ctx: &egui::Context,
    theme: &Theme,
    menu_id: &str,
    extra_hover: Option<egui::Rect>,
    content: impl FnOnce(&mut egui::Ui) -> Option<u8>,
) -> MenuOutcome {
    let now = ctx.input(|i| i.time);
    let age = now - state.opened_at;

    let mut action: Option<u8> = None;
    let inner = egui::Area::new(egui::Id::new(menu_id))
        .fixed_pos(state.pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme.panel)
                .inner_margin(egui::Margin::same(4))
                .shadow(menu_shadow())
                .show(ui, |ui| {
                    ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                    action = content(ui);
                });
        });

    let menu_rect = inner.response.rect;
    let pointer_pos = ctx.pointer_hover_pos();
    let is_hovered_menu = pointer_pos.map(|p| menu_rect.contains(p)).unwrap_or(false);
    let is_hovered_extra = extra_hover
        .and_then(|r| pointer_pos.map(|p| r.contains(p)))
        .unwrap_or(false);
    let is_hovered = is_hovered_menu || is_hovered_extra;

    // Seed timer on first frame.
    if !state.timer_seeded {
        state.last_hover = now;
        state.timer_seeded = true;
    }

    // Reset hover timer when pointer is inside any hover zone.
    if is_hovered {
        state.last_hover = now;
    }

    let hover_expired = !is_hovered && (now - state.last_hover) > ContextMenuState::HOVER_TIMEOUT;
    let clicked_outside = age > ContextMenuState::AGE_GUARD
        && ctx.input(|i| i.pointer.any_click())
        && !is_hovered;
    let escape_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    // Keep repainting while the menu is open so the hover timer ticks.
    ctx.request_repaint();

    if action.is_some() || hover_expired || clicked_outside || escape_pressed {
        MenuOutcome::Close(action)
    } else {
        MenuOutcome::Open(action)
    }
}
