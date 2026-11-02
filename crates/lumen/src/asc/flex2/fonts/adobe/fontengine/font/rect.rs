//! Port of afe.jar `com.adobe.fontengine.font.Rect`.

use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub xmin: f64,
    pub ymin: f64,
    pub xmax: f64,
    pub ymax: f64,
}

impl Rect {
    pub const EMPTY_RECT: Rect = Rect { xmin: 0.0, ymin: 0.0, xmax: 0.0, ymax: 0.0 };

    pub fn new(xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> Rect {
        Rect { xmin, ymin, xmax, ymax }
    }

    pub fn from_array(a: &[f64]) -> Rect {
        Rect::new(a[0], a[1], a[2], a[3])
    }

    pub fn apply_matrix(&self, m: &Matrix) -> Rect {
        if m.is_identity() {
            return *self;
        }
        let x1 = m.apply_to_xy_get_x(self.xmin, self.ymax);
        let y1 = m.apply_to_xy_get_y(self.xmin, self.ymax);
        let x2 = m.apply_to_xy_get_x(self.xmax, self.ymax);
        let y2 = m.apply_to_xy_get_y(self.xmax, self.ymax);
        let x3 = m.apply_to_xy_get_x(self.xmax, self.ymin);
        let y3 = m.apply_to_xy_get_y(self.xmax, self.ymin);
        let x4 = m.apply_to_xy_get_x(self.xmin, self.ymin);
        let y4 = m.apply_to_xy_get_y(self.xmin, self.ymin);
        Rect::new(jmin(jmin(x1, x2), jmin(x3, x4)), jmin(jmin(y1, y2), jmin(y3, y4)), jmax(jmax(x1, x2), jmax(x3, x4)), jmax(jmax(y1, y2), jmax(y3, y4)))
    }

    pub fn to_em_space(&self, x: f64, y: f64) -> Rect {
        Rect::new(self.xmin / x, self.ymin / y, self.xmax / x, self.ymax / y)
    }

    pub fn to_design_space(&self, x: f64, y: f64) -> Rect {
        Rect::new(self.xmin * x, self.ymin * y, self.xmax * x, self.ymax * y)
    }
}

/// `Math.min(double, double)`: NaN wins, and -0.0 is below 0.0.
pub fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        return a;
    }
    if a <= b {
        a
    } else {
        b
    }
}

/// `Math.max(double, double)`: NaN wins, and 0.0 is above -0.0.
pub fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.is_sign_positive() {
        return a;
    }
    if a >= b {
        a
    } else {
        b
    }
}
