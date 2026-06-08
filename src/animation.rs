// src/animation.rs
use std::time::Instant;
use egui::TextureHandle;
use crate::project::Frame;
use crate::layers::composite_frame;

pub struct PlaybackState {
    pub is_playing: bool,
    pub last_tick: Instant,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self { is_playing: false, last_tick: Instant::now() }
    }
}

impl PlaybackState {
    /// Returns true and advances frame if enough time has elapsed.
    /// Plays within `clip_start .. clip_start + clip_len` range.
    pub fn tick(&mut self, fps: u8, current_frame: &mut usize, total_frames: usize, clip_start: usize, clip_len: usize) -> bool {
        if !self.is_playing || total_frames == 0 || clip_len == 0 { return false; }
        let interval = std::time::Duration::from_secs_f32(1.0 / fps.max(1) as f32);
        if self.last_tick.elapsed() >= interval {
            let end = clip_start + clip_len - 1;
            if *current_frame < clip_start || *current_frame > end {
                *current_frame = clip_start;
            } else if *current_frame >= end {
                *current_frame = clip_start;
            } else {
                *current_frame += 1;
            }
            self.last_tick = Instant::now();
            return true;
        }
        false
    }
}

/// Per-frame thumbnail cache entry
pub struct FrameThumbnail {
    pub handle: Option<TextureHandle>,
    pub dirty: bool,
}

impl Default for FrameThumbnail {
    fn default() -> Self { Self { handle: None, dirty: true } }
}

/// Generates a thumbnail RGBA buffer for a frame at a given scale
pub fn make_thumbnail(frame: &Frame, width: u32, height: u32) -> Vec<u8> {
    composite_frame(frame, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_does_not_advance_when_paused() {
        let mut state = PlaybackState::default();
        let mut frame = 0usize;
        let advanced = state.tick(12, &mut frame, 5, 0, 5);
        assert!(!advanced);
        assert_eq!(frame, 0);
    }

    #[test]
    fn playback_wraps_at_end() {
        let mut state = PlaybackState {
            is_playing: true,
            last_tick: Instant::now() - std::time::Duration::from_secs(1),
        };
        let mut frame = 4usize;
        state.tick(12, &mut frame, 5, 0, 5);
        assert_eq!(frame, 0);
    }

    #[test]
    fn playback_respects_clip_range() {
        let mut state = PlaybackState {
            is_playing: true,
            last_tick: Instant::now() - std::time::Duration::from_secs(1),
        };
        let mut frame = 2usize;
        state.tick(12, &mut frame, 5, 2, 3); // clip 2..=4
        assert_eq!(frame, 3);
        state.last_tick = Instant::now() - std::time::Duration::from_secs(1);
        state.tick(12, &mut frame, 5, 2, 3);
        assert_eq!(frame, 4);
        state.last_tick = Instant::now() - std::time::Duration::from_secs(1);
        state.tick(12, &mut frame, 5, 2, 3);
        assert_eq!(frame, 2); // wraps back to clip_start
    }
}
