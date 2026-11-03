//! Ports of Batik's `ext.awt.MultipleGradientPaint`, `LinearGradientPaint` and `RadialGradientPaint` (the state the
//! SWF builders read; painting them is Java2D's).

pub mod geom;

use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::paint::Color;

pub const NO_CYCLE: i32 = 0;
pub const REFLECT: i32 = 1;
pub const REPEAT: i32 = 2;
pub const SRGB: i32 = 0;
pub const LINEAR_RGB: i32 = 1;

#[derive(Clone, Debug)]
pub struct MultipleGradientPaint {
    pub fractions: Vec<f32>,
    pub colors: Vec<Color>,
    pub cycle_method: i32,
    pub color_space: i32,
    pub gradient_transform: AffineTransform,
}

impl MultipleGradientPaint {
    /// The constructor's checks; a failure is Java's `IllegalArgumentException`.
    pub fn new(fractions: &[f32], colors: &[Color], cycle_method: i32, color_space: i32, gradient_transform: AffineTransform) -> Result<MultipleGradientPaint, String> {
        if fractions.len() != colors.len() {
            return Err("Colors and fractions must have equal size".into());
        }
        if colors.len() < 2 {
            return Err("User must specify at least 2 colors".into());
        }
        Ok(MultipleGradientPaint { fractions: fractions.to_vec(), colors: colors.to_vec(), cycle_method, color_space, gradient_transform })
    }
}

#[derive(Clone, Debug)]
pub struct LinearGradientPaint {
    pub base: MultipleGradientPaint,
    pub start: (f64, f64),
    pub end: (f64, f64),
}

impl LinearGradientPaint {
    pub fn new(start: (f64, f64), end: (f64, f64), base: MultipleGradientPaint) -> Result<LinearGradientPaint, String> {
        if start == end {
            return Err("Start point cannot equalendpoint".into());
        }
        Ok(LinearGradientPaint { base, start, end })
    }
}

#[derive(Clone, Debug)]
pub struct RadialGradientPaint {
    pub base: MultipleGradientPaint,
    pub center: (f64, f64),
    pub focus: (f64, f64),
    pub radius: f32,
}

impl RadialGradientPaint {
    pub fn new(center: (f64, f64), radius: f32, focus: (f64, f64), base: MultipleGradientPaint) -> Result<RadialGradientPaint, String> {
        if radius <= 0.0 {
            return Err("radius should be greater than zero".into());
        }
        Ok(RadialGradientPaint { base, center, focus, radius })
    }
}
