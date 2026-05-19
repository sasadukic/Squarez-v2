// src/tools/select.rs
// Selection and move tool state — rendered in app.rs
#[derive(Debug, Clone, Default)]
pub struct SelectState {
    pub rect: Option<(u32, u32, u32, u32)>, // (x, y, w, h)
    pub clipboard: Option<Vec<u8>>,
}
