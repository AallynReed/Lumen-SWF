//! Port of afe.jar `com.adobe.fontengine.font.OutlineConsumer`.

use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;

pub trait OutlineConsumer {
    fn set_matrix(&mut self, m: &Matrix);
    fn moveto(&mut self, x: f64, y: f64);
    fn lineto(&mut self, x: f64, y: f64);
    /// `curveto(x1, y1, x2, y2, x3, y3)`, a cubic.
    fn curveto(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64);
    /// `curveto(x1, y1, x2, y2)`, a quadratic.
    fn curveto_quad(&mut self, x1: f64, y1: f64, x2: f64, y2: f64);
    fn endchar(&mut self);
}
