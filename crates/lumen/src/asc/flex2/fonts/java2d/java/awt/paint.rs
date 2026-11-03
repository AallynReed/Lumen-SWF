//! Ports of the `java.awt` paint state SVG rendering sets: `Color`, `BasicStroke`, `AlphaComposite`, and the
//! `Paint` kinds a `Graphics2D` receives.

use std::rc::Rc;

use crate::asc::flex2::batik::ext::awt::{LinearGradientPaint, RadialGradientPaint};

/// `java.awt.Color`: `value` is the ARGB int.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub value: i32,
}

impl Color {
    /// `new Color(r, g, b, a)`; out-of-range components are Java's `IllegalArgumentException`.
    pub fn new(r: i32, g: i32, b: i32, a: i32) -> Result<Color, String> {
        let mut bad = String::new();
        for (v, n) in [(a, " Alpha"), (r, " Red"), (g, " Green"), (b, " Blue")] {
            if !(0..=255).contains(&v) {
                bad.push_str(n);
            }
        }
        if !bad.is_empty() {
            return Err(format!("Color parameter outside of expected range:{bad}"));
        }
        Ok(Color { value: ((a & 0xFF) << 24) | ((r & 0xFF) << 16) | ((g & 0xFF) << 8) | (b & 0xFF) })
    }

    pub fn rgb(r: i32, g: i32, b: i32) -> Color {
        Color { value: (0xFF << 24) | ((r & 0xFF) << 16) | ((g & 0xFF) << 8) | (b & 0xFF) }
    }

    pub fn get_red(&self) -> i32 {
        (self.value >> 16) & 0xFF
    }
    pub fn get_green(&self) -> i32 {
        (self.value >> 8) & 0xFF
    }
    pub fn get_blue(&self) -> i32 {
        self.value & 0xFF
    }
    pub fn get_alpha(&self) -> i32 {
        (self.value >> 24) & 0xFF
    }
}

pub const CAP_BUTT: i32 = 0;
pub const CAP_ROUND: i32 = 1;
pub const CAP_SQUARE: i32 = 2;
pub const JOIN_MITER: i32 = 0;
pub const JOIN_ROUND: i32 = 1;
pub const JOIN_BEVEL: i32 = 2;

/// `java.awt.BasicStroke`.
#[derive(Clone, Debug, PartialEq)]
pub struct BasicStroke {
    pub width: f32,
    pub cap: i32,
    pub join: i32,
    pub miterlimit: f32,
    pub dash: Option<Vec<f32>>,
    pub dash_phase: f32,
}

impl Default for BasicStroke {
    fn default() -> Self {
        BasicStroke { width: 1.0, cap: CAP_SQUARE, join: JOIN_MITER, miterlimit: 10.0, dash: None, dash_phase: 0.0 }
    }
}

impl BasicStroke {
    pub fn get_line_width(&self) -> f32 {
        self.width
    }
}

/// `AlphaComposite.getInstance(SRC_OVER, alpha)`; `SrcOver` is alpha 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaComposite {
    pub alpha: f32,
}

impl AlphaComposite {
    pub const SRC_OVER: AlphaComposite = AlphaComposite { alpha: 1.0 };

    pub fn get_alpha(&self) -> f32 {
        self.alpha
    }
}

/// The `java.awt.Paint` implementations that reach a `Graphics2D` here.
#[derive(Clone, Debug)]
pub enum Paint {
    Color(Color),
    LinearGradient(Rc<LinearGradientPaint>),
    RadialGradient(Rc<RadialGradientPaint>),
    /// Batik's `PatternPaint`, which the SWF builders do not convert.
    Pattern,
}
