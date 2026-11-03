//! Port of swfutils `flash.graphics.g2d.GraphicContext`: transform, paint, stroke, composite, clip, pen and hints.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{create_transformed_shape, Path2D, Rectangle2D, Shape, WIND_NON_ZERO};
use crate::asc::flex2::fonts::java2d::java::awt::paint::{AlphaComposite, BasicStroke, Color, Paint};

/// `RenderingHints`, keyed and valued by the constants' names.
pub type RenderingHints = BTreeMap<&'static str, &'static str>;

pub const KEY_RENDERING: &str = "KEY_RENDERING";
pub const KEY_ANTIALIASING: &str = "KEY_ANTIALIASING";
pub const KEY_TEXT_ANTIALIASING: &str = "KEY_TEXT_ANTIALIASING";
pub const KEY_FRACTIONALMETRICS: &str = "KEY_FRACTIONALMETRICS";
pub const KEY_STROKE_CONTROL: &str = "KEY_STROKE_CONTROL";
pub const KEY_COLOR_RENDERING: &str = "KEY_COLOR_RENDERING";
pub const KEY_ALPHA_INTERPOLATION: &str = "KEY_ALPHA_INTERPOLATION";
pub const KEY_INTERPOLATION: &str = "KEY_INTERPOLATION";

#[derive(Clone, Debug)]
pub struct GraphicContext {
    transform: AffineTransform,
    composite: AlphaComposite,
    pen: (f64, f64),
    paint: Option<Paint>,
    stroke: BasicStroke,
    background: Color,
    clip: Option<Shape>,
    hints: RenderingHints,
}

impl GraphicContext {
    /// `new GraphicContext(width, height)` (its offscreen image only serves font metrics).
    pub fn new() -> GraphicContext {
        let mut hints = RenderingHints::new();
        hints.insert(KEY_RENDERING, "VALUE_RENDER_DEFAULT");
        GraphicContext {
            transform: AffineTransform::new(),
            composite: AlphaComposite::SRC_OVER,
            pen: (0.0, 0.0),
            paint: Some(Paint::Color(Color::rgb(0, 0, 0))),
            stroke: BasicStroke::default(),
            background: Color { value: 0x00FFFFFF },
            clip: None,
            hints,
        }
    }

    pub fn get_composite(&self) -> AlphaComposite {
        self.composite
    }

    pub fn set_composite(&mut self, c: AlphaComposite) {
        self.composite = c;
    }

    pub fn get_paint(&self) -> Option<&Paint> {
        self.paint.as_ref()
    }

    pub fn set_paint(&mut self, paint: Option<Paint>) {
        self.paint = paint;
    }

    pub fn get_stroke(&self) -> &BasicStroke {
        &self.stroke
    }

    pub fn set_stroke(&mut self, s: BasicStroke) {
        self.stroke = s;
    }

    pub fn get_background(&self) -> Color {
        self.background
    }

    pub fn get_pen(&self) -> (f64, f64) {
        self.pen
    }

    pub fn set_pen(&mut self, pen: (f64, f64)) -> (f64, f64) {
        std::mem::replace(&mut self.pen, pen)
    }

    pub fn get_rendering_hints(&self) -> RenderingHints {
        self.hints.clone()
    }

    pub fn add_rendering_hints(&mut self, h: &RenderingHints) {
        for (k, v) in h {
            self.hints.insert(k, v);
        }
    }

    pub fn set_rendering_hints(&mut self, h: RenderingHints) {
        self.hints = h;
    }

    /// `clip(s)`: an `Area` intersection; only its bounds matter here, so it is kept as the intersected bounds.
    pub fn clip(&mut self, s: Option<&Shape>) {
        let s = s.map(|s| Shape::Path(create_transformed_shape(&self.transform, s)));
        match (&self.clip, s) {
            (Some(c), Some(s)) => {
                let r = c.get_bounds2d().create_intersection(&s.get_bounds2d());
                let mut p = Path2D::new_float(WIND_NON_ZERO);
                if !r.is_empty() {
                    p.move_to(r.x, r.y);
                    p.line_to(r.x + r.w, r.y);
                    p.line_to(r.x + r.w, r.y + r.h);
                    p.line_to(r.x, r.y + r.h);
                    p.close_path();
                }
                self.clip = Some(Shape::Path(p));
            }
            (_, s) => self.clip = s,
        }
    }

    pub fn set_clip(&mut self, clip: Option<&Shape>) {
        self.clip = clip.map(|c| Shape::Path(create_transformed_shape(&self.transform, c)));
    }

    /// `getClip()`: the device clip in user space.
    pub fn get_clip(&self) -> Option<Shape> {
        let c = self.clip.as_ref()?;
        let inv = self.transform.create_inverse().ok()?;
        Some(Shape::Path(create_transformed_shape(&inv, c)))
    }

    pub fn get_clip_bounds2d(&self) -> Option<Rectangle2D> {
        self.get_clip().map(|c| c.get_bounds2d())
    }

    pub fn rotate(&mut self, theta: f64) {
        self.transform.rotate(theta);
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.transform.scale(sx, sy);
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        self.transform.translate(tx, ty);
    }

    pub fn transform(&mut self, tx: &AffineTransform) {
        self.transform.concatenate(tx);
    }

    pub fn get_transform(&self) -> AffineTransform {
        self.transform
    }

    pub fn set_transform(&mut self, tx: &AffineTransform) {
        self.transform = *tx;
    }
}

impl Default for GraphicContext {
    fn default() -> Self {
        GraphicContext::new()
    }
}
