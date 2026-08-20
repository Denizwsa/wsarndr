//! Layout helpers for modders - flex-like row/column, centering, padding.

use super::shape::Color;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self { Self { x, y, w, h } }
    pub fn inset(&self, pad: f32) -> Self { Self { x: self.x + pad, y: self.y + pad, w: (self.w - pad * 2.0).max(0.0), h: (self.h - pad * 2.0).max(0.0) } }
    pub fn inset_xy(&self, px: f32, py: f32) -> Self { Self { x: self.x + px, y: self.y + py, w: (self.w - px * 2.0).max(0.0), h: (self.h - py * 2.0).max(0.0) } }
    pub fn center_x(&self, w: f32) -> Self { Self { x: self.x + (self.w - w) * 0.5, y: self.y, w, h: self.h } }
    pub fn center_y(&self, h: f32) -> Self { Self { x: self.x, y: self.y + (self.h - h) * 0.5, w: self.w, h } }
    pub fn split_top(&self, h: f32) -> (Self, Self) {
        let top = Self { x: self.x, y: self.y, w: self.w, h: h.min(self.h) };
        let bot = Self { x: self.x, y: self.y + h, w: self.w, h: (self.h - h).max(0.0) };
        (top, bot)
    }
    pub fn split_left(&self, w: f32) -> (Self, Self) {
        let left = Self { x: self.x, y: self.y, w: w.min(self.w), h: self.h };
        let right = Self { x: self.x + w, y: self.y, w: (self.w - w).max(0.0), h: self.h };
        (left, right)
    }
}

/// Row layout: place children left-to-right with gap.
pub struct Row {
    pub rect: Rect,
    pub gap: f32,
    cursor: f32,
}

impl Row {
    pub fn new(rect: Rect, gap: f32) -> Self { Self { rect, gap, cursor: rect.x } }
    pub fn next(&mut self, w: f32) -> Rect {
        let r = Rect::new(self.cursor, self.rect.y, w, self.rect.h);
        self.cursor += w + self.gap;
        r
    }
}

/// Column layout: place children top-to-bottom with gap.
pub struct Column {
    pub rect: Rect,
    pub gap: f32,
    cursor: f32,
}

impl Column {
    pub fn new(rect: Rect, gap: f32) -> Self { Self { rect, gap, cursor: rect.y } }
    pub fn next(&mut self, h: f32) -> Rect {
        let r = Rect::new(self.rect.x, self.cursor, self.rect.w, h);
        self.cursor += h + self.gap;
        r
    }
}
