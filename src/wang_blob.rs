// src/wang_blob.rs

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WangBlobMode {
    None,
    Wang,   // 16 tiles, 4 edges (N/E/S/W), 4×4 grid
    Blob,   // 48 tiles (1 gap), 8 neighbors (TL/T/TR/R/BR/B/BL/L), 12×4 grid
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Bidirectional,  // Paint on any tile → update all matching neighbors (A)
    Unidirectional, // Paint only on center tile → propagate to neighbors (B)
}

pub const WANG_COLS: u32 = 4;
pub const WANG_ROWS: u32 = 4;
pub const WANG_TOTAL: usize = 16;

pub const BLOB_COLS: u32 = 12;
pub const BLOB_ROWS: u32 = 4;
pub const BLOB_TOTAL: usize = 48;

pub const WANG_CENTER_ROW: u32 = 3;
pub const WANG_CENTER_COL: u32 = 3;

pub const BLOB_CENTER_ROW: u32 = 2;
pub const BLOB_CENTER_COL: u32 = 9;

#[derive(Debug, Clone)]
pub struct WangBlobState {
    pub mode:        WangBlobMode,
    pub sync_mode:   SyncMode,
    pub tile_data:   Vec<Vec<[u8; 4]>>,
    pub tile_size:   u32,
}

impl Default for WangBlobState {
    fn default() -> Self {
        Self {
            mode:        WangBlobMode::None,
            sync_mode:   SyncMode::Bidirectional,
            tile_data:   Vec::new(),
            tile_size:   16,
        }
    }
}

impl WangBlobState {
    pub fn grid_cols(&self) -> u32 {
        match self.mode { WangBlobMode::Wang => WANG_COLS, WangBlobMode::Blob => BLOB_COLS, _ => 0 }
    }

    pub fn grid_rows(&self) -> u32 {
        match self.mode { WangBlobMode::Wang => WANG_ROWS, WangBlobMode::Blob => BLOB_ROWS, _ => 0 }
    }

    pub fn expand(
        &mut self,
        mode: WangBlobMode,
        canvas_width: u32,
        canvas_height: u32,
        canvas_pixels: &[u8],
    ) {
        self.mode = mode;
        self.tile_size = canvas_width.max(canvas_height);
        let cols = self.grid_cols();
        let rows = self.grid_rows();
        let ts = self.tile_size as usize;

        self.tile_data = vec![vec![[0, 0, 0, 0]; ts * ts]; (rows * cols) as usize];

        let (center_row, center_col) = match mode {
            WangBlobMode::Wang => (WANG_CENTER_ROW, WANG_CENTER_COL),
            _ => (BLOB_CENTER_ROW, BLOB_CENTER_COL),
        };
        let center_idx = (center_row * cols + center_col) as usize;

        // Copy initial canvas pixels into center tile
        if !canvas_pixels.is_empty() {
            let cw = canvas_width as usize;
            let ch = canvas_height as usize;
            for y in 0..ch.min(ts) {
                for x in 0..cw.min(ts) {
                    let src_idx = (y * cw + x) * 4;
                    if src_idx + 3 < canvas_pixels.len() {
                        let pixel_idx = y * ts + x;
                        if pixel_idx < ts * ts {
                            self.tile_data[center_idx][pixel_idx] = [
                                canvas_pixels[src_idx],
                                canvas_pixels[src_idx + 1],
                                canvas_pixels[src_idx + 2],
                                canvas_pixels[src_idx + 3],
                            ];
                        }
                    }
                }
            }
        }

        // Copy center tile to all other tiles so the grid starts visible
        for tidx in 0..self.tile_data.len() {
            if tidx != center_idx {
                self.tile_data[tidx] = self.tile_data[center_idx].clone();
            }
        }

        // Propagate edges across entire grid for seamless tiling
        self.propagate_all();
    }

    pub fn propagate_all(&mut self) {
        let mut dirty: HashSet<usize> = (0..self.tile_data.len()).collect();
        let mut iterations = 0;
        let max_iter = self.tile_data.len() * 2;

        while !dirty.is_empty() && iterations < max_iter {
            iterations += 1;
            let current: Vec<usize> = dirty.drain().collect();
            for &tidx in &current {
                if tidx >= self.tile_data.len() { continue; }
                let (tr, tc) = self.tile_coords(tidx).unwrap_or((0, 0));
                self.propagate_from_tile(tr, tc, &mut dirty);
            }
        }
    }

    /// Propagate only from tiles that were modified, avoiding
    /// the back-propagation issue where unedited neighbors overwrite source tiles.
    pub fn propagate_modified(&mut self, changed_tiles: &[usize]) {
        let mut dirty: HashSet<usize> = changed_tiles.iter().copied().collect();
        let mut iterations = 0;
        let max_iter = self.tile_data.len() * 2;

        while !dirty.is_empty() && iterations < max_iter {
            iterations += 1;
            let current: Vec<usize> = dirty.drain().collect();
            for &tidx in &current {
                if tidx >= self.tile_data.len() { continue; }
                let (tr, tc) = self.tile_coords(tidx).unwrap_or((0, 0));
                self.propagate_from_tile(tr, tc, &mut dirty);
            }
        }
    }

    fn propagate_from_tile(&mut self, tile_row: u32, tile_col: u32, dirty: &mut HashSet<usize>) {
        let ts = self.tile_size as usize;
        let tidx = self.tile_index(tile_row, tile_col);
        if tidx >= self.tile_data.len() { return; }

        // Collect all pending edge copies: (neighbor_idx, dst_pixel_idx, src_pixel)
        let mut pending: Vec<(usize, usize, [u8; 4])> = Vec::new();

        let source_data = &self.tile_data[tidx];

        for i in 0..source_data.len() {
            let local_y = i / ts;
            let local_x = i % ts;

            if local_y == 0 {
                self.collect_edge(i, tile_row, tile_col, Dir::N, ts, source_data, &mut pending);
            }
            if local_y == ts - 1 {
                self.collect_edge(i, tile_row, tile_col, Dir::S, ts, source_data, &mut pending);
            }
            if local_x == 0 {
                self.collect_edge(i, tile_row, tile_col, Dir::W, ts, source_data, &mut pending);
            }
            if local_x == ts - 1 {
                self.collect_edge(i, tile_row, tile_col, Dir::E, ts, source_data, &mut pending);
            }

            if self.mode == WangBlobMode::Blob {
                if local_x == 0 && local_y == 0 {
                    self.collect_edge(i, tile_row, tile_col, Dir::TL, ts, source_data, &mut pending);
                }
                if local_x == ts - 1 && local_y == 0 {
                    self.collect_edge(i, tile_row, tile_col, Dir::TR, ts, source_data, &mut pending);
                }
                if local_x == 0 && local_y == ts - 1 {
                    self.collect_edge(i, tile_row, tile_col, Dir::BL, ts, source_data, &mut pending);
                }
                if local_x == ts - 1 && local_y == ts - 1 {
                    self.collect_edge(i, tile_row, tile_col, Dir::BR, ts, source_data, &mut pending);
                }
            }
        }

        // Apply all pending copies (borrow of source_data is released)
        for (nidx, dst_i, src) in pending {
            if dst_i < self.tile_data[nidx].len() {
                let old = self.tile_data[nidx][dst_i];
                if old != src {
                    self.tile_data[nidx][dst_i] = src;
                    dirty.insert(nidx);
                }
            }
        }
    }

    fn collect_edge(
        &self,
        src_i: usize,
        tile_row: u32,
        tile_col: u32,
        dir: Dir,
        ts: usize,
        source_data: &[[u8; 4]],
        pending: &mut Vec<(usize, usize, [u8; 4])>,
    ) {
        let local_y = src_i / ts;
        let local_x = src_i % ts;

        let (nr, nc, dst_local_x, dst_local_y) = match dir {
            Dir::N => (tile_row.wrapping_sub(1), tile_col, local_x, ts - 1),
            Dir::S => (tile_row + 1, tile_col, local_x, 0),
            Dir::W => (tile_row, tile_col.wrapping_sub(1), ts - 1, local_y),
            Dir::E => (tile_row, tile_col + 1, 0, local_y),
            Dir::TL => (tile_row.wrapping_sub(1), tile_col.wrapping_sub(1), ts - 1, ts - 1),
            Dir::TR => (tile_row.wrapping_sub(1), tile_col + 1, 0, ts - 1),
            Dir::BL => (tile_row + 1, tile_col.wrapping_sub(1), ts - 1, 0),
            Dir::BR => (tile_row + 1, tile_col + 1, 0, 0),
            _ => return,
        };

        let rows = self.grid_rows();
        let cols = self.grid_cols();
        let nr = nr % rows;
        let nc = nc % cols;

        if self.is_gap(nr, nc) { return; }

        let nidx = self.tile_index(nr, nc);
        if nidx >= self.tile_data.len() { return; }

        let dst_i = (dst_local_y * ts + dst_local_x) as usize;
        if dst_i >= self.tile_data[nidx].len() { return; }

        let src = source_data[src_i];
        pending.push((nidx, dst_i, src));
    }

    pub fn propagate_bidirectional(&mut self, tile_row: u32, tile_col: u32) {
        let mut dirty = HashSet::new();
        dirty.insert((tile_row * self.grid_cols() + tile_col) as usize);
        self.propagate_from_tile(tile_row, tile_col, &mut dirty);
        let mut iterations = 0;
        while !dirty.is_empty() && iterations < self.tile_data.len() * 2 {
            iterations += 1;
            let current: Vec<usize> = dirty.drain().collect();
            for &tidx in &current {
                if tidx >= self.tile_data.len() { continue; }
                let (r, c) = self.tile_coords(tidx).unwrap_or((0, 0));
                self.propagate_from_tile(r, c, &mut dirty);
            }
        }
    }

    pub fn propagate_unidirectional(&mut self, tile_row: u32, tile_col: u32) {
        let (center_row, center_col) = match self.mode {
            WangBlobMode::Wang => (WANG_CENTER_ROW, WANG_CENTER_COL),
            _ => (BLOB_CENTER_ROW, BLOB_CENTER_COL),
        };

        if tile_row != center_row || tile_col != center_col { return; }

        let mut dirty = HashSet::new();
        dirty.insert(self.tile_index(tile_row, tile_col));
        self.propagate_from_tile(tile_row, tile_col, &mut dirty);
        let mut iterations = 0;
        while !dirty.is_empty() && iterations < self.tile_data.len() * 2 {
            iterations += 1;
            let current: Vec<usize> = dirty.drain().collect();
            for &tidx in &current {
                if tidx >= self.tile_data.len() { continue; }
                let (r, c) = self.tile_coords(tidx).unwrap_or((0, 0));
                self.propagate_from_tile(r, c, &mut dirty);
            }
        }
    }

    pub fn grid_dims(&self) -> (u32, u32) {
        match self.mode {
            WangBlobMode::Wang => (self.tile_size * WANG_COLS, self.tile_size * WANG_ROWS),
            WangBlobMode::Blob => (self.tile_size * BLOB_COLS, self.tile_size * BLOB_ROWS),
            _ => (0, 0),
        }
    }

    pub fn screen_to_tile(&self, canvas_x: u32, canvas_y: u32) -> Option<(u32, u32, u32, u32)> {
        let (cols, rows) = self.grid_dims();
        if canvas_x >= cols || canvas_y >= rows { return None; }

        let tile_col = canvas_x / self.tile_size;
        let tile_row = canvas_y / self.tile_size;
        let local_x  = canvas_x % self.tile_size;
        let local_y  = canvas_y % self.tile_size;

        Some((tile_row, tile_col, local_x, local_y))
    }

    pub fn tile_index(&self, row: u32, col: u32) -> usize {
        (row * self.grid_cols() + col) as usize
    }

    pub fn tile_coords(&self, idx: usize) -> Option<(u32, u32)> {
        let cols = self.grid_cols();
        if cols == 0 { return None; }
        let row = (idx / cols as usize) as u32;
        let col = (idx % cols as usize) as u32;
        Some((row, col))
    }

    pub fn is_gap(&self, row: u32, col: u32) -> bool {
        if self.mode != WangBlobMode::Blob { return false; }
        row == 1 && col == 10
    }

    pub fn paint_tile(&mut self, edits: &[crate::tools::PixelEdit], tile_row: u32, tile_col: u32) {
        let ts = self.tile_size as usize;
        let tidx = self.tile_index(tile_row, tile_col);
        if tidx >= self.tile_data.len() { return; }

        // Apply edits to the target tile
        for &(x, y, _old, new) in edits {
            let local_x = (x % self.tile_size) as usize;
            let local_y = (y % self.tile_size) as usize;
            let pixel_idx = local_y * ts + local_x;
            if pixel_idx < self.tile_data[tidx].len() {
                self.tile_data[tidx][pixel_idx] = [new[0], new[1], new[2], new[3]];
            }
        }

        // Propagate to neighbors
        match self.sync_mode {
            SyncMode::Bidirectional => self.propagate_bidirectional(tile_row, tile_col),
            SyncMode::Unidirectional => self.propagate_unidirectional(tile_row, tile_col),
        }
    }

    /// Flatten tile_data into a single RGBA pixel buffer
    pub fn flatten_to_buffer(&self, out: &mut [u8], canvas_width: u32) {
        let ts = self.tile_size as usize;
        let cols = self.grid_cols() as usize;
        let rows = self.grid_rows() as usize;
        let cw = canvas_width as usize;

        for tr in 0..rows {
            for tc in 0..cols {
                let tidx = tr * cols + tc;
                if tidx >= self.tile_data.len() { continue; }
                let tile = &self.tile_data[tidx];
                let base_y = tr * ts;
                let base_x = tc * ts;
                for ly in 0..ts {
                    for lx in 0..ts {
                        let pixel_idx = ly * ts + lx;
                        if pixel_idx >= tile.len() { continue; }
                        let canvas_y = base_y + ly;
                        let canvas_x = base_x + lx;
                        let out_idx = (canvas_y * cw + canvas_x) * 4;
                        if out_idx + 3 < out.len() {
                            let p = tile[pixel_idx];
                            out[out_idx]     = p[0];
                            out[out_idx + 1] = p[1];
                            out[out_idx + 2] = p[2];
                            out[out_idx + 3] = p[3];
                        }
                    }
                }
            }
        }
    }

    pub fn get_tileset_pixels(&self) -> Vec<u8> {
        let (width, height) = self.grid_dims();
        let mut result = vec![0u8; (width * height * 4) as usize];
        self.flatten_to_buffer(&mut result, width);
        result
    }

    pub fn get_single_tile(&self, row: u32, col: u32) -> Vec<u8> {
        let tidx = self.tile_index(row, col);
        if tidx >= self.tile_data.len() { return Vec::new(); }

        let ts = self.tile_size as usize;
        let mut result = Vec::with_capacity(ts * ts * 4);
        for pixel in &self.tile_data[tidx] {
            result.extend_from_slice(&[pixel[0], pixel[1], pixel[2], pixel[3]]);
        }
        result
    }

    pub fn get_spritesheet(&self) -> Vec<u8> {
        self.get_tileset_pixels()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    N, S, E, W,  // Wang cardinal directions
    TL, TR, BL, BR, T, B, L, R,  // Blob directions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> [u8; 4] { [255, 0, 0, 255] }
    fn blue() -> [u8; 4] { [0, 0, 255, 255] }
    fn green() -> [u8; 4] { [0, 255, 0, 255] }
    fn black() -> [u8; 4] { [0, 0, 0, 0] }

    /// Helper: create canvas with red
    fn make_red_canvas(w: u32, h: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize * 4;
                if idx + 3 < pixels.len() {
                    pixels[idx..idx + 4].copy_from_slice(&[255, 0, 0, 255]);
                }
            }
        }
        pixels
    }

    fn check_pixel(buf: &[u8], w: u32, x: u32, y: u32, expected: [u8; 4]) {
        let idx = (y * w + x) as usize * 4;
        assert!(idx + 3 < buf.len(), "pixel ({},{}) out of bounds", x, y);
        assert_eq!(&buf[idx..idx + 4], &expected, "pixel ({},{}) mismatch", x, y);
    }

    #[test]
    fn test_expand_creates_correct_grid_size() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        assert_eq!(state.tile_data.len(), 16); // 4×4 = 16 tiles
        assert_eq!(state.tile_size, 16);

        // Every tile should have 16*16 = 256 pixels
        for (i, tile) in state.tile_data.iter().enumerate() {
            assert_eq!(tile.len(), 256, "tile {} has wrong size", i);
        }
    }

    #[test]
    fn test_expand_copies_center_to_all_tiles() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        // All tiles should be red initially (copied from center)
        for (i, tile) in state.tile_data.iter().enumerate() {
            assert_eq!(tile[0], red(), "tile {} first pixel not red", i);
        }
    }

    #[test]
    fn test_propagate_edge_syncs_adjacent_tiles() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        // Manually paint top edge of center tile (3,3) blue
        let center_tile = WANG_CENTER_ROW * WANG_COLS + WANG_CENTER_COL;
        let ts = state.tile_size as usize;
        for x in 0..ts {
            state.tile_data[center_tile as usize][x] = blue(); // top row
        }

        // Propagate from center tile
        state.propagate_bidirectional(WANG_CENTER_ROW, WANG_CENTER_COL);

        // Top neighbor (2,3) should have its bottom edge blue now
        let top_idx = (2 * WANG_COLS + 3) as usize;
        for x in 0..ts {
            let bottom_edge_pixel = (ts - 1) * ts + x;
            assert_eq!(state.tile_data[top_idx][bottom_edge_pixel], blue(),
                "top neighbor bottom edge not synced at x={}", x);
        }

        // The source tile should still have its blue top edge
        for x in 0..ts {
            assert_eq!(state.tile_data[center_tile as usize][x], blue(),
                "center tile top edge changed at x={}", x);
        }
    }

    #[test]
    fn test_propagate_modified_preserves_source_edits() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        let ts = state.tile_size as usize;
        let ct = (WANG_CENTER_ROW * WANG_COLS + WANG_CENTER_COL) as usize;

        // Paint the center tile's first column blue (left edge)
        for y in 0..ts {
            state.tile_data[ct][y * ts] = blue();
        }

        // Propagate from the changed tile only (as sync_frame_to_wang would do)
        state.propagate_modified(&[ct]);

        // Source tile's edit is preserved
        assert_eq!(state.tile_data[ct][0], blue());

        // Direct neighbor left (3,2) should have matching right edge
        let left_nb = (WANG_CENTER_ROW * WANG_COLS + (WANG_CENTER_COL - 1)) as usize;
        assert_eq!(state.tile_data[left_nb][0 * ts + (ts - 1)], blue(),
            "neighbor (3,2) right edge not synced from (3,3)'s blue left edge");
    }

    #[test]
    fn test_propagate_all_no_regression_on_identical_tiles() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        // All tiles are identical red — propagate_all is a no-op
        let before = state.tile_data.clone();
        state.propagate_all();
        assert_eq!(state.tile_data, before,
            "propagate_all modified identical tiles");
    }

    #[test]
    fn test_flatten_to_buffer_output_size() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        let (new_w, new_h) = state.grid_dims();
        let mut out = vec![0u8; (new_w * new_h * 4) as usize];
        state.flatten_to_buffer(&mut out, new_w);

        assert_eq!(out.len(), (64 * 64 * 4) as usize);
        // Top-left pixel should be red
        check_pixel(&out, new_w, 0, 0, red());
        // Bottom-right pixel should be red (all tiles same after expand)
        check_pixel(&out, new_w, 63, 63, red());
    }

    #[test]
    fn test_sync_frame_to_wang_preserves_edits() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        // Simulate a frame buffer with a modified tile
        let (w, h) = state.grid_dims();
        let ts = state.tile_size;
        let mut frame = vec![0u8; (w * h * 4) as usize];
        // Fill with red
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize * 4;
                frame[idx..idx+4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }

        // Modify tile (0,0)'s top-left pixel to blue in the frame
        frame[0] = 0; frame[1] = 0; frame[2] = 255; frame[3] = 255;

        // Sync frame back to tile_data manually
        for tr in 0..state.grid_rows() {
            for tc in 0..state.grid_cols() {
                if state.is_gap(tr, tc) { continue; }
                let tidx = state.tile_index(tr, tc);
                for ly in 0..ts {
                    for lx in 0..ts {
                        let cy = tr * ts + ly;
                        let cx = tc * ts + lx;
                        let idx = (cy * w + cx) as usize * 4;
                        state.tile_data[tidx][(ly * ts + lx) as usize] = [
                            frame[idx], frame[idx+1], frame[idx+2], frame[idx+3],
                        ];
                    }
                }
            }
        }

        // Check that tile (0,0) has blue at its first pixel
        assert_eq!(state.tile_data[0][0], blue());
        // Other tiles should still be red
        assert_eq!(state.tile_data[1][0], red(), "other tile modified unexpectedly");
    }

    #[test]
    fn test_center_tile_coordinates() {
        let state = WangBlobState {
            mode: WangBlobMode::Wang,
            tile_size: 16,
            tile_data: vec![vec![[0,0,0,0]; 256]; 16],
            ..Default::default()
        };

        // Center tile is at row 3, col 3 (0-indexed)
        assert_eq!(WANG_CENTER_ROW, 3);
        assert_eq!(WANG_CENTER_COL, 3);
        assert_eq!(state.tile_index(3, 3), 15); // 3*4 + 3 = 15

        // For Blob, center is at row 2, col 9
        assert_eq!(BLOB_CENTER_ROW, 2);
        assert_eq!(BLOB_CENTER_COL, 9);
        let blob_state = WangBlobState {
            mode: WangBlobMode::Blob,
            tile_size: 16,
            tile_data: vec![vec![[0,0,0,0]; 256]; 48],
            ..Default::default()
        };
        assert_eq!(blob_state.tile_index(2, 9), 33); // 2*12 + 9 = 33
    }

    #[test]
    fn test_loop_wrapping() {
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        let ts = state.tile_size as usize;

        // Paint the left edge of tile (0,0) blue
        let tile0_idx = state.tile_index(0, 0);
        for y in 0..ts {
            state.tile_data[tile0_idx][y * ts] = blue(); // left column
        }

        // Paint the left edge of tile (0,3) green
        let tile3_idx = state.tile_index(0, 3);
        for y in 0..ts {
            state.tile_data[tile3_idx][y * ts] = green();
        }

        // Propagate from tile (0,0) - its W edge wraps to tile (0,3)'s right edge
        state.propagate_bidirectional(0, 0);

        // Tile (0,3)'s right edge should be blue now (wrapped from (0,0)'s left edge)
        let dst_pixel_idx = ts - 1; // rightmost column, row 0
        assert_eq!(state.tile_data[tile3_idx][dst_pixel_idx], blue(),
            "loop wrap failed: tile (0,3) right edge not blue");

        // But tile (0,3)'s left edge should still be green (not overwritten)
        assert_eq!(state.tile_data[tile3_idx][0], green(),
            "tile (0,3) left edge was overwritten");
    }

    #[test]
    fn test_sync_integration_propagates_edge_to_neighbor() {
        // Simulate the full sync_frame_to_wang flow
        let mut state = WangBlobState::default();
        let pixels = make_red_canvas(16, 16);
        state.expand(WangBlobMode::Wang, 16, 16, &pixels);

        let (cw, ch) = state.grid_dims(); // 64×64
        assert_eq!(cw, 64);
        assert_eq!(ch, 64);

        // Simulate the frame layer (as sync_frame_to_wang uses it)
        let mut layer_pixels = vec![0u8; (cw * ch * 4) as usize];
        state.flatten_to_buffer(&mut layer_pixels, cw);

        // All pixels should be red
        check_pixel(&layer_pixels, cw, 0, 0, red());
        check_pixel(&layer_pixels, cw, 63, 63, red());

        // Simulate user drawing on the RIGHT edge of tile (0, 0) at canvas (15, 10)
        let edit_x = 15u32; // right edge of tile (0, 0)
        let edit_y = 10u32;
        let idx = (edit_y * cw + edit_x) as usize * 4;
        layer_pixels[idx..idx+4].copy_from_slice(&[0, 0, 255, 255]); // blue

        // Now simulate sync_frame_to_wang:
        // 1. Clone old tile_data
        let old_tiles: Vec<Vec<[u8; 4]>> = state.tile_data.clone();

        // 2. Copy frame pixels to tile_data
        let ts = state.tile_size;
        let rows = state.grid_rows();
        let cols = state.grid_cols();
        for tr in 0..rows {
            for tc in 0..cols {
                if state.is_gap(tr, tc) { continue; }
                let tidx = state.tile_index(tr, tc);
                if tidx >= state.tile_data.len() { continue; }
                let tile = &mut state.tile_data[tidx];
                for ly in 0..ts {
                    for lx in 0..ts {
                        let canvas_y = tr * ts + ly;
                        let canvas_x = tc * ts + lx;
                        let idx2 = (canvas_y * cw + canvas_x) as usize * 4;
                        if idx2 + 3 < layer_pixels.len() {
                            tile[(ly * ts + lx) as usize] = [
                                layer_pixels[idx2],
                                layer_pixels[idx2 + 1],
                                layer_pixels[idx2 + 2],
                                layer_pixels[idx2 + 3],
                            ];
                        }
                    }
                }
            }
        }

        // 3. Find changed tiles
        let mut changed: Vec<usize> = Vec::new();
        for (tidx, (old, new)) in old_tiles.iter().zip(state.tile_data.iter()).enumerate() {
            if old != new {
                changed.push(tidx);
            }
        }
        // Tile (0, 0) should be in changed because pixel (15, 10) is now blue
        assert!(changed.contains(&0), "tile (0,0) should be in changed set; found {:?}", changed);

        // 4. Propagate from changed tiles
        if !changed.is_empty() {
            state.propagate_modified(&changed);
        }

        // 5. Flatten back to layer pixels
        state.flatten_to_buffer(&mut layer_pixels, cw);

        // The neighbor to the right of tile (0,0) is tile (0,1)
        // Its left edge at canvas coords (16, 10) should now be blue
        check_pixel(&layer_pixels, cw, 16, 10, blue());
    }

    #[test]
    fn test_gap_tile_skipped() {
        let mut state = WangBlobState {
            mode: WangBlobMode::Blob,
            tile_size: 16,
            tile_data: vec![vec![[0,0,0,0]; 256]; 48],
            ..Default::default()
        };

        // Gap is at row 1, col 10
        assert!(state.is_gap(1, 10));
        assert!(!state.is_gap(0, 0));
        assert!(!state.is_gap(1, 9));
        assert!(!state.is_gap(1, 11));
    }
}
