#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeKind {
    Rect,
    RoundedRect { radius: f32 },
    Circle,
    Ellipse,
    Line { x1: f32, y1: f32 },
    Triangle { x2: f32, y2: f32, x3: f32, y3: f32 },
    Arc { start: f32, sweep: f32 },
    Polygon { sides: u32 },
    Text,
}

impl Default for ShapeKind {
    fn default() -> Self {
        ShapeKind::Rect
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Default for TextAlign {
    fn default() -> Self {
        TextAlign::Left
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

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn darken(self, amount: f32) -> Self {
        let f = (1.0 - amount).max(0.0);
        Self { r: self.r * f, g: self.g * f, b: self.b * f, a: self.a }
    }

    pub fn lighten(self, amount: f32) -> Self {
        Self {
            r: (self.r + (1.0 - self.r) * amount).min(1.0),
            g: (self.g + (1.0 - self.g) * amount).min(1.0),
            b: (self.b + (1.0 - self.b) * amount).min(1.0),
            a: self.a,
        }
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let h = hex.trim_start_matches('#');
        let n = u32::from_str_radix(h, 16).ok()?;
        match h.len() {
            6 => Some(Self::argb(0xFF000000 | n)),
            8 => Some(Self::argb(n)),
            _ => None,
        }
    }

    pub fn rainbow(hue: f32) -> Self {
        let h = hue.fract() * 6.0;
        let i = h as i32;
        let f = h - i as f32;
        let q = 1.0 - f;
        let (r, g, b) = match i % 6 {
            0 => (1.0, f, 0.0),
            1 => (q, 1.0, 0.0),
            2 => (0.0, 1.0, f),
            3 => (0.0, q, 1.0),
            4 => (f, 0.0, 1.0),
            _ => (1.0, 0.0, q),
        };
        Self { r, g, b, a: 1.0 }
    }

    // --- Extended RGB / HSV support ---

    /// Create from 0-255 RGB
    pub const fn rgb_u8(r: u8, g: u8, b: u8) -> Self { Self::rgba8(r, g, b, 255) }

    /// Create from HSV (h 0..1, s 0..1, v 0..1)
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.fract() * 6.0;
        let i = h as i32;
        let f = h - i as f32;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let (r, g, b) = match i % 6 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Self { r, g, b, a: 1.0 }
    }

    pub fn to_hsv(self) -> (f32, f32, f32) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let d = max - min;
        let h = if d == 0.0 { 0.0 } else if max == self.r {
            60.0 * (((self.g - self.b) / d) % 6.0)
        } else if max == self.g {
            60.0 * ((self.b - self.r) / d + 2.0)
        } else {
            60.0 * ((self.r - self.g) / d + 4.0)
        };
        let s = if max == 0.0 { 0.0 } else { d / max };
        ((h / 360.0).rem_euclid(1.0), s, max)
    }

    pub fn with_red(self, r: f32) -> Self { Self { r: r.clamp(0.0, 1.0), ..self } }
    pub fn with_green(self, g: f32) -> Self { Self { g: g.clamp(0.0, 1.0), ..self } }
    pub fn with_blue(self, b: f32) -> Self { Self { b: b.clamp(0.0, 1.0), ..self } }

    pub fn grayscale(self) -> Self {
        let l = 0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b;
        Self { r: l, g: l, b: l, a: self.a }
    }

    pub fn invert(self) -> Self { Self { r: 1.0 - self.r, g: 1.0 - self.g, b: 1.0 - self.b, a: self.a } }

    pub fn saturate(self, amount: f32) -> Self {
        let (h, s, v) = self.to_hsv();
        Self::from_hsv(h, (s + amount).clamp(0.0, 1.0), v).with_alpha(self.a)
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        [(self.r * 255.0) as u8, (self.g * 255.0) as u8, (self.b * 255.0) as u8, (self.a * 255.0) as u8]
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", (self.r*255.0) as u8, (self.g*255.0) as u8, (self.b*255.0) as u8, (self.a*255.0) as u8)
    }

    // Predefined palette - Catppuccin + Minecraft + basic
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const RED: Self = Self { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const GREEN: Self = Self { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    pub const BLUE: Self = Self { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
}

/// Theme palette for modders to quickly style their client
#[derive(Clone, Debug)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub border: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self { Self::catppuccin_mocha() }
}

impl Theme {
    pub fn catppuccin_mocha() -> Self {
        Self {
            background: Color::argb(0xFF1E1E2E),
            surface: Color::argb(0xFF313244),
            primary: Color::argb(0xFF89B4FA),
            secondary: Color::argb(0xFFA6ADC8),
            accent: Color::argb(0xFFF38BA8),
            text: Color::argb(0xFFCDD6F4),
            text_secondary: Color::argb(0xFFA6ADC8),
            border: Color::argb(0xFF45475A),
            success: Color::argb(0xFFA6E3A1),
            warning: Color::argb(0xFFF9E2AF),
            error: Color::argb(0xFFF38BA8),
        }
    }
    pub fn midnight() -> Self {
        Self {
            background: Color::argb(0xFF0F0F14),
            surface: Color::argb(0xFF1A1A22),
            primary: Color::argb(0xFF7AA5FF),
            secondary: Color::argb(0xFF8E8EA0),
            accent: Color::argb(0xFFFF6B9D),
            text: Color::argb(0xFFEAEAF0),
            text_secondary: Color::argb(0xFF9A9AB0),
            border: Color::argb(0xFF2A2A3A),
            success: Color::argb(0xFF4ECCA3),
            warning: Color::argb(0xFFF0C674),
            error: Color::argb(0xFFFC5C7D),
        }
    }
}

/// Easing helpers for animations (t in 0..1)
pub mod easing {
    pub fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t.clamp(0.0, 1.0) }
    pub fn ease_out_cubic(t: f32) -> f32 { let p = t - 1.0; p * p * p + 1.0 }
    pub fn ease_in_out_cubic(t: f32) -> f32 {
        if t < 0.5 { 4.0 * t * t * t } else { let p = 2.0 * t - 2.0; 0.5 * p * p * p + 1.0 }
    }
    pub fn ease_out_elastic(t: f32) -> f32 {
        if t == 0.0 || t == 1.0 { t } else {
            let p = 0.3;
            (2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * std::f32::consts::PI / p)).sin() + 1.0)
        }
    }
    /// Animate current value towards target with speed (0..1), dt in seconds
    pub fn animate(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
        current + (target - current) * (1.0 - (-speed * dt).exp())
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