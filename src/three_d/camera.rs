// src/three_d/camera.rs
//
// Orthographic-only camera: yaw/pitch orbit, screen-px zoom, pixel pan.
// Snap views quantize zoom and offset so axis-aligned faces render with
// exactly N screen pixels per texel.

use egui::{Pos2, Rect, Vec2};

pub const MIN_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 64.0;

/// Classic isometric-style home view: 45° around, atan(1/sqrt(2)) down.
pub const HOME_YAW: f32 = -std::f32::consts::FRAC_PI_4;
pub const HOME_PITCH: f32 = 0.615_479_7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapView {
    Front,
    Back,
    Right,
    Left,
    Top,
    Bottom,
}

impl SnapView {
    /// (yaw, pitch) for this view. Conventions: yaw rotates the world around
    /// +Y; pitch > 0 lifts the camera above the model.
    pub fn angles(self) -> (f32, f32) {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            SnapView::Front => (0.0, 0.0),
            SnapView::Back => (PI, 0.0),
            SnapView::Right => (-FRAC_PI_2, 0.0),
            SnapView::Left => (FRAC_PI_2, 0.0),
            SnapView::Top => (0.0, FRAC_PI_2),
            SnapView::Bottom => (0.0, -FRAC_PI_2),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SnapView::Front => "Front",
            SnapView::Back => "Back",
            SnapView::Right => "Right",
            SnapView::Left => "Left",
            SnapView::Top => "Top",
            SnapView::Bottom => "Bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera3D {
    pub yaw: f32,
    pub pitch: f32,
    /// Screen pixels per world unit (= per texel at the fixed density).
    pub zoom: f32,
    /// Pan in screen pixels.
    pub offset: Vec2,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self { yaw: HOME_YAW, pitch: HOME_PITCH, zoom: 12.0, offset: Vec2::ZERO }
    }
}

impl Camera3D {
    /// Rotate a world point into view space:
    /// x = screen right, y = screen up, z = toward the camera.
    pub fn view(&self, p: [f32; 3]) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sx, cx) = self.pitch.sin_cos();
        // yaw around +Y
        let x1 = p[0] * cy + p[2] * sy;
        let z1 = -p[0] * sy + p[2] * cy;
        // pitch around +X
        let y2 = p[1] * cx - z1 * sx;
        let z2 = p[1] * sx + z1 * cx;
        [x1, y2, z2]
    }

    /// Rotate a direction (e.g. a face normal) into view space.
    /// Identical to `view` — rotations only, kept as a named alias.
    pub fn view_dir(&self, d: [f32; 3]) -> [f32; 3] {
        self.view(d)
    }

    /// Project a world point to a screen position plus depth
    /// (larger depth = closer to the camera).
    pub fn project(&self, p: [f32; 3], rect: Rect) -> (Pos2, f32) {
        let v = self.view(p);
        let center = rect.center() + self.offset;
        (Pos2::new(center.x + v[0] * self.zoom, center.y - v[1] * self.zoom), v[2])
    }

    /// Jump to a snap view: exact angles, integer zoom, whole-pixel offset —
    /// the pixel-perfect painting guarantee.
    pub fn snap_to(&mut self, view: SnapView) {
        let (yaw, pitch) = view.angles();
        self.yaw = yaw;
        self.pitch = pitch;
        self.zoom = self.zoom.round().clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = Vec2::new(self.offset.x.round(), self.offset.y.round());
    }

    /// Which snap view the camera currently sits on exactly, if any.
    pub fn snapped(&self) -> Option<SnapView> {
        const VIEWS: [SnapView; 6] = [
            SnapView::Front,
            SnapView::Back,
            SnapView::Right,
            SnapView::Left,
            SnapView::Top,
            SnapView::Bottom,
        ];
        VIEWS.into_iter().find(|v| {
            let (yaw, pitch) = v.angles();
            self.yaw == yaw && self.pitch == pitch
        })
    }

    pub fn reset_home(&mut self) {
        self.yaw = HOME_YAW;
        self.pitch = HOME_PITCH;
        self.offset = Vec2::ZERO;
    }

    pub fn orbit(&mut self, delta: Vec2) {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        self.yaw = (self.yaw - delta.x * 0.01) % TAU;
        if self.yaw > PI {
            self.yaw -= TAU;
        } else if self.yaw < -PI {
            self.yaw += TAU;
        }
        self.pitch = (self.pitch + delta.y * 0.01).clamp(-FRAC_PI_2, FRAC_PI_2);
    }

    /// Zoom by a factor while keeping the world point under `cursor` fixed.
    pub fn zoom_at(&mut self, factor: f32, cursor: Pos2, rect: Rect) {
        let old_zoom = self.zoom;
        let new_zoom = (old_zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if new_zoom == old_zoom {
            return;
        }
        let center = rect.center();
        let from_center = cursor - center - self.offset;
        self.offset += from_center - from_center * (new_zoom / old_zoom);
        self.zoom = new_zoom;
    }

    /// Fit a world-space bounding sphere of `radius` into `rect`.
    pub fn zoom_to_fit(&mut self, radius: f32, rect: Rect) {
        let r = radius.max(1.0);
        let avail = rect.width().min(rect.height()) * 0.8;
        self.zoom = (avail / (2.0 * r)).clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = Vec2::ZERO;
    }
}
