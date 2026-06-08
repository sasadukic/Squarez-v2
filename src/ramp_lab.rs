// src/ramp_lab.rs
//
// Ramp Lab: floating OKLCh-based color ramp generator for pixel art palettes.

use crate::color::oklab::safe_oklch_to_rgba;
use crate::project::Rgba;

// ─── Harmony ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum HarmonyMode {
    Analogous,
    Complementary,
    Triadic,
    SplitComplementary,
    Custom,
}

impl HarmonyMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Analogous          => "Analogous",
            Self::Complementary      => "Complementary",
            Self::Triadic            => "Triadic",
            Self::SplitComplementary => "Split Comp.",
            Self::Custom             => "Custom",
        }
    }

    pub fn all() -> &'static [HarmonyMode] {
        use HarmonyMode::*;
        &[Analogous, Complementary, Triadic, SplitComplementary, Custom]
    }

    /// Returns N hue offsets (degrees) relative to base hue.
    pub fn hue_offsets(&self, n: usize) -> Vec<f32> {
        if n == 0 { return vec![]; }
        match self {
            Self::Analogous => {
                let spread = (n as f32 * 10.0).min(80.0);
                if n == 1 { return vec![0.0]; }
                (0..n).map(|i| -spread / 2.0 + spread * (i as f32 / (n - 1) as f32)).collect()
            }
            Self::Complementary => {
                (0..n).map(|i| if i % 2 == 0 { 0.0 } else { 180.0 }).collect()
            }
            Self::Triadic => {
                (0..n).map(|i| (i % 3) as f32 * 120.0).collect()
            }
            Self::SplitComplementary => {
                (0..n).map(|i| match i % 3 { 0 => 0.0, 1 => 150.0, _ => 210.0 }).collect()
            }
            Self::Custom => {
                (0..n).map(|i| i as f32 * 360.0 / n as f32).collect()
            }
        }
    }
}

// ─── Curve ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum CurveTab { L, C, H }

/// Quadratic Bézier: P0, P1 (draggable mid), P2 — all in normalized [0,1] value space.
#[derive(Clone, Debug)]
pub struct BezierCurve {
    pub p0: f32,
    pub p1: f32,
    pub p2: f32,
}

impl BezierCurve {
    pub fn new(p0: f32, p1: f32, p2: f32) -> Self { Self { p0, p1, p2 } }

    pub fn eval(&self, t: f32) -> f32 {
        let u = 1.0 - t;
        u * u * self.p0 + 2.0 * u * t * self.p1 + t * t * self.p2
    }

    pub fn sample(&self, n: usize) -> Vec<f32> {
        (0..n).map(|i| {
            let t = if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
            self.eval(t)
        }).collect()
    }
}

// ─── Ramp ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Ramp {
    pub name: String,
    pub locked: bool,
    pub num_steps: usize,
    /// None = use harmony angle; Some(h) = custom hue override.
    pub hue_override: Option<f32>,
    pub colors: Vec<Rgba>,
}

// ─── RampLab ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct RampLab {
    pub open:      bool,
    pub opened_at: f64,   // ctx.input(|i| i.time) when last opened

    // Settings
    pub num_ramps:       usize,
    pub num_steps:       usize,
    pub harmony:         HarmonyMode,
    pub base_hue:        f32,   // 0..360
    pub anchor_first:    bool,
    pub anchor_last:     bool,
    pub show_l_values:   bool,

    // Ramps
    pub ramps: Vec<Ramp>,

    // Curve editor
    pub active_curve_tab: CurveTab,
    pub l_curve: BezierCurve,
    pub c_curve: BezierCurve,
    pub h_curve: BezierCurve,

    /// Index of ramp whose hue-dot was clicked (override bar shown).
    pub override_ramp_idx: Option<usize>,

    /// Index of ramp currently being drag-reordered.
    pub dragging_ramp: Option<usize>,
}

impl Default for RampLab {
    fn default() -> Self {
        let mut lab = Self {
            open:             false,
            opened_at:        0.0,
            num_ramps:        6,
            num_steps:        7,
            harmony:          HarmonyMode::Analogous,
            base_hue:         14.0,
            anchor_first:     true,
            anchor_last:      true,
            show_l_values:    false,
            ramps:            Vec::new(),
            active_curve_tab: CurveTab::L,
            l_curve:          BezierCurve::new(0.05, 0.50, 0.95),
            c_curve:          BezierCurve::new(0.03, 0.18, 0.03),
            h_curve:          BezierCurve::new(0.5,  0.5,  0.5),
            override_ramp_idx: None,
            dragging_ramp:     None,
        };
        lab.regenerate_all();
        lab
    }
}

impl RampLab {
    /// Effective hue for ramp at `idx` (respects per-ramp override).
    pub fn hue_for_ramp(&self, idx: usize) -> f32 {
        if let Some(h) = self.ramps.get(idx).and_then(|r| r.hue_override) {
            return h;
        }
        let offsets = self.harmony.hue_offsets(self.num_ramps);
        let off = offsets.get(idx).copied().unwrap_or(0.0);
        (self.base_hue + off).rem_euclid(360.0)
    }

    /// Generate swatch colors for one ramp using current curves.
    pub fn generate_colors_for(&self, ramp_idx: usize) -> Vec<Rgba> {
        let h_base = self.hue_for_ramp(ramp_idx);
        let n = self.num_steps;
        let ls = self.l_curve.sample(n);
        let cs = self.c_curve.sample(n);
        let hs = self.h_curve.sample(n); // 0..1 → -30..+30° shift

        (0..n).map(|i| {
            if self.anchor_first && i == 0 {
                return safe_oklch_to_rgba(0.08, 0.01, h_base, 255, true);
            }
            if self.anchor_last && i == n - 1 {
                return safe_oklch_to_rgba(0.95, 0.02, h_base, 255, true);
            }
            let l = ls[i].clamp(0.0, 1.0);
            let c = cs[i].clamp(0.0, 0.40);
            let h = (h_base + (hs[i] - 0.5) * 60.0).rem_euclid(360.0);
            safe_oklch_to_rgba(l, c, h, 255, true)
        }).collect()
    }

    /// Regenerate one ramp (skip if locked).
    pub fn regenerate_ramp(&mut self, idx: usize) {
        if self.ramps.get(idx).map_or(false, |r| r.locked) { return; }
        let colors = self.generate_colors_for(idx);
        if let Some(r) = self.ramps.get_mut(idx) {
            r.colors = colors;
        }
    }

    /// Ensure ramp count matches `num_ramps` and regenerate all unlocked ramps.
    pub fn regenerate_all(&mut self) {
        const NAMES: &[&str] = &[
            "Skin", "Grass", "Sky", "Stone", "Lava", "Shadow",
            "Ocean", "Forest", "Fire", "Ice",
        ];
        while self.ramps.len() < self.num_ramps {
            let i = self.ramps.len();
            self.ramps.push(Ramp {
                name:         NAMES.get(i).copied().unwrap_or("Ramp").to_string(),
                locked:       false,
                num_steps:    self.num_steps,
                hue_override: None,
                colors:       Vec::new(),
            });
        }
        self.ramps.truncate(self.num_ramps);
        for i in 0..self.num_ramps {
            self.regenerate_ramp(i);
        }
    }

    /// All swatch colors in row-major order (ramp 0 step 0..N, ramp 1 …).
    pub fn all_colors(&self) -> Vec<Rgba> {
        self.ramps.iter().flat_map(|r| r.colors.iter().copied()).collect()
    }
}
