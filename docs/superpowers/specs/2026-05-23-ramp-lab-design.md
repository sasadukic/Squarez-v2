Title: Ramp Lab (Preview-only) Design
Date: 2026-05-23
Author: OpenCode

Overview
- Add a "Ramp Lab" modal to author color ramps using OKLab or HSV.
- Minimal, preview-only workflow for first iteration: edits update a preview but do not mutate palette/canvas until the user presses Apply. This keeps behavior predictable and reduces undo surface area.

Goals
- Provide a compact, discoverable UI to generate N-step ramps anchored to the current foreground color.
- Use gamut-safe OKLab conversions so generated ramps never produce out-of-gamut sRGB colors visible to users.
- Support grouped undo for color state changes (ColorState snapshot) when committing edits.
- Keep implementation minimal to ship quickly and iteratively improve later.

Constraints
- Preview-only: no live mutation of palette or canvas while dragging. Changes are committed only on Apply.
- Reuse existing ColorState ramp curve fields (ramp_curve_start_*, _mid_*, _end_*) where practical; maintain a local edit buffer in App while modal is open.
- Use safe_oklch_to_rgba(...) for OKLab -> sRGB conversions (preserve_l = true for UI ramps).
- No new persistent data format for presets in v1; presets can be added later.

High-level UX
- Entry: right-click a ramp strip or palette swatch -> "Open Ramp Lab" (or a toolbar/menu entry). Ramp Lab is modal and blocks canvas/timeline input while open.
- Modal layout (left-to-right):
  - Left column: Generated preview strip (horizontal) and a row of swatches (click to select a swatch to copy to FG). "Push To Palette" button to insert preview colors into palette.
  - Right column: Mode toggle (OKLab / HSV), Hue slider, 3-handle curve editor for lightness (start/mid/end), numeric fields for precise values, Ramp size selector (3..=9), "Generate" button, and footer actions: Apply / Cancel.
- Live preview updates while editing (no commit). If any color was adjusted for gamut, show a subtle "Gamut-adjusted" badge.

Data model & state
- Use existing ColorState as the persistent color model. It already contains ramp_curve_start_luma, ramp_curve_mid_luma, ramp_curve_end_luma, ramp_curve_* for sat/hue, ramp_size, hue_shift_deg, sat_curve_depth, ramp_anchor, light_end_l, light_end_v.
- Add a temporary edit buffer in App when Ramp Lab opens:
  struct RampLabBuffer {
    mode: PickerMode,
    hue: f32,
    curve_start_luma: f32,
    curve_mid_luma: f32,
    curve_end_luma: f32,
    curve_start_sat: f32,
    curve_mid_sat: f32,
    curve_end_sat: f32,
    curve_start_hue: f32,
    curve_mid_hue: f32,
    curve_end_hue: f32,
    ramp_size: usize,
  }
- On modal open: copy values from ColorState into RampLabBuffer. UI edits modify RampLabBuffer only.
- Preview generation: compute N-step ramp using existing generate_ramp / generate_ramp_hsv / generate_ramp_endpoints functions into OKLCh/HSV tuples; then map to RGBA using safe_oklch_to_rgba(...) when in OKLab mode and hsv_to_rgba(...) when in HSV mode.

Gamut handling
- OKLab branch: always call safe_oklch_to_rgba(l, c, h, 255, preserve_l=true) to produce preview swatches. If safe_oklch_to_rgba had to reduce chroma or adjust L, mark that preview with "Gamut-adjusted" indicator.
- If safe_oklch_to_rgba returns achromatic fallback (c ~ 0), preview may show neutral values. This is acceptable for now.

Commit / Undo behavior
- Because this iteration is preview-only, mutations only happen on Apply:
  1. Capture before_color_state = color_state.clone().
  2. Update color_state fields from RampLabBuffer (copy curve values and ramp_size, and optionally update foreground if user picks a swatch as FG).
  3. If user requested "Push to Palette": insert preview colors into project.palette (append in order). For each inserted color, push a Command that represents that change — if a dedicated Command variant exists for palettes, use it; otherwise the UI may directly modify palette and we push a SetColorStateSnapshot (see below) and then store the palette mutation as a single Command::PaintPixels or equivalent. (Implementation detail left to the next step.)
  4. Push undo snapshot: undo_stack.push(Command::SetColorStateSnapshot { before: before_color_state, after: color_state.clone() });

- Undo/Redo usage: when the user triggers Undo after Apply, call undo_with_color(&mut project, &mut color_state) so ColorState is restored alongside project mutations. Redo similarly via redo_with_color.

Accessibility & small-details
- Keyboard: Enter applies, Esc cancels.
- Cursor changes and tooltips on handles.
- Numeric fields use typed input with basic clamping (0..1 for L/V; C clamped to allowed range in generate_ramp).

Testing
- Unit tests (existing style):
  - Extend tests for safe_oklch_to_rgba to assert that preview colors remain in sRGB gamut (0..=255) for wide ranges of H and L.
  - Add test to simulate: copy ColorState, apply RampLab changes (call the same helper used by Apply), then call undo_with_color and assert ColorState equals before snapshot.

Implementation tasks (next steps)
1. Spec file written and reviewed (this file). (this step)
2. writing-plans: create a detailed task list with small commits for each UI and logic change.
3. Implement RampLabBuffer in App and open/close modal wiring (toggle ramp_lab_open true/false). Ensure modal blocks canvas/timeline input when open.
4. Implement modal UI in src/app.rs using existing UI patterns. Keep the curve editor simple: 3 vertical draggable handles on fixed X positions (0, 0.5, 1.0). Numeric fields below.
5. Hook preview generation: use generate_ramp / generate_ramp_hsv + safe_oklch_to_rgba for OKLab preview.
6. Implement Apply logic: copy buffer -> ColorState, persist ramp curve values, optionally push to palette, and push Command::SetColorStateSnapshot into undo_stack. Use undo_with_color/redo_with_color in keybindings.
7. Add small unit tests described above.
8. Polish: add "Gamut-adjusted" indicator, keyboard shortcuts, accessibility notes.

Open questions (pick one if you want now; otherwise I will proceed to writing-plans after you review):
- Palette push behavior: should pushing preview to palette replace or append? Default behavior proposed: append.
- Palette duplicates: should we dedupe identical colors on push? Default: allow duplicates.

Review & Approval
- I created this spec at docs/superpowers/specs/2026-05-23-ramp-lab-design.md in the repo. Please review this file. When you approve, I will run the writing-plans skill to break this into implementation tasks and then begin coding per the plan.
