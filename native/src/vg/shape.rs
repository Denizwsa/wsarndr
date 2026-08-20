#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeKind {
    Rect,
    RoundedRect { radius: f32 },
    Circle,
    Line { x1: f32, y1: f32 },
    Text,
}

impl Default for ShapeKind {
    fn default() -> Self {
        ShapeKind::Rect
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradMode {
    None,
    Linear,
    Radial,
}

impl Default for GradMode {
    fn default() -> Self {
        GradMode::None
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Shape {
    pub kind: ShapeKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub fill_color: Color,
    pub grad_color: Color,
    pub grad_mode: GradMode,
    pub grad_from: [f32; 2],
    pub grad_to: [f32; 2],
    pub grad_inner_radius: f32,
    pub stroke_width: f32,
    pub uv_override: Option<[f32; 4]>,
    pub _line_n: [f32; 2],
}

impl Shape {
    /// Returns (bounds [x0, y0, x1, y1], half size)
    pub fn bounds(&self) -> ([f32; 4], f32) {
        let pad = match self.kind {
            ShapeKind::Line { .. } => self.stroke_width * 0.5 + 2.0,
            _ => 0.0,
        };
        let (x0, y0) = (self.x - pad, self.y - pad);
        let (x1, y1) = (self.x + self.w + pad, self.y + self.h + pad);
        let half = ((x1 - x0) * 0.5).max((y1 - y0) * 0.5);
        ([x0, y0, x1, y1], half)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// 0xRRGGBBAA or 0xAARRGGBB-compatible int (same as MC's packed ARGB when high byte = alpha)
    pub const fn argb(argb: u32) -> Self {
        Self {
            r: ((argb >> 16) & 0xFF) as f32 / 255.0,
            g: ((argb >> 8) & 0xFF) as f32 / 255.0,
            b: (argb & 0xFF) as f32 / 255.0,
            a: ((argb >> 24) & 0xFF) as f32 / 255.0,
        }
    }

    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

impl From<[f32; 4]> for Color {
    fn from(c: [f32; 4]) -> Self {
        Self {
            r: c[0],
            g: c[1],
            b: c[2],
            a: c[3],
        }
    }
}

impl From<[f32; 3]> for Color {
    fn from(c: [f32; 3]) -> Self {
        Self {
            r: c[0],
            g: c[1],
            b: c[2],
            a: 1.0,
        }
    }
}

impl From<u32> for Color {
    fn from(argb: u32) -> Self {
        Self::argb(argb)
    }
}

impl From<(f32, f32, f32, f32)> for Color {
    fn from((r, g, b, a): (f32, f32, f32, f32)) -> Self {
        Self { r, g, b, a }
    }
}