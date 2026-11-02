//! Port of afe.jar `com.adobe.fontengine.font.Matrix`.

use crate::asc::flex2::fonts::adobe::fontengine::font::point::Point;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Matrix {
    pub const IDENTITY_MATRIX: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    pub fn new(a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64) -> Matrix {
        Matrix { a, b, c, d, tx, ty }
    }

    pub fn from_array(m: &[f64]) -> Matrix {
        Matrix { a: m[0], b: m[1], c: m[2], d: m[3], tx: if m.len() > 4 { m[4] } else { 0.0 }, ty: if m.len() > 5 { m[5] } else { 0.0 } }
    }

    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 0.001 && (self.d - 1.0).abs() < 0.001 && self.b.abs() < 0.001 && self.c.abs() < 0.001 && self.tx.abs() < 0.001 && self.ty.abs() < 0.001
    }

    pub fn is_inverse(&self, m: &Matrix) -> bool {
        self.multiply(m).is_identity()
    }

    pub fn multiply(&self, m: &Matrix) -> Matrix {
        let a = self.a * m.a + self.b * m.c;
        let b = self.a * m.b + self.b * m.d;
        let c = self.c * m.a + self.d * m.c;
        let d = self.c * m.b + self.d * m.d;
        let tx = self.tx * m.a + self.ty * m.c + m.tx;
        let ty = self.tx * m.b + self.ty * m.d + m.ty;
        Matrix::new(a, b, c, d, tx, ty)
    }

    pub fn multiply_scalar(&self, s: f64) -> Matrix {
        Matrix::new(self.a * s, self.b * s, self.c * s, self.d * s, self.tx * s, self.ty * s)
    }

    /// `applyToPoint`; Java's `equals` is `Object.equals`, so only the `IDENTITY_MATRIX` instance itself is skipped.
    pub fn apply_to_point(&self, p: &mut Point) {
        let (x, y) = (p.x, p.y);
        p.x = x * self.a + y * self.c + self.tx;
        p.y = x * self.b + y * self.d + self.ty;
    }

    pub fn apply_to_xy_get_x(&self, x: f64, y: f64) -> f64 {
        x * self.a + y * self.c + self.tx
    }

    pub fn apply_to_xy_get_y(&self, x: f64, y: f64) -> f64 {
        x * self.b + y * self.d + self.ty
    }
}
