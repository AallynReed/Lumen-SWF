//! Port of JDK 17 `sun.awt.geom.Order0`: a point, the moveto that starts each `Area` subpath.

use super::curve::{Rect, INCREASING};
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::SEG_MOVETO;

/// `Order0`.
#[derive(Clone, Debug)]
pub struct Order0 {
    pub direction: i32,
    x: f64,
    y: f64,
}

impl Order0 {
    pub fn new(x: f64, y: f64) -> Order0 {
        Order0 { direction: INCREASING, x, y }
    }

    pub fn get_x_top(&self) -> f64 {
        self.x
    }

    pub fn get_y_top(&self) -> f64 {
        self.y
    }

    pub fn get_x_bot(&self) -> f64 {
        self.x
    }

    pub fn get_y_bot(&self) -> f64 {
        self.y
    }

    pub fn get_x_min(&self) -> f64 {
        self.x
    }

    pub fn get_x_max(&self) -> f64 {
        self.x
    }

    pub fn get_x0(&self) -> f64 {
        self.x
    }

    pub fn get_y0(&self) -> f64 {
        self.y
    }

    pub fn get_x1(&self) -> f64 {
        self.x
    }

    pub fn get_y1(&self) -> f64 {
        self.y
    }

    /// `XforY`.
    pub fn x_for_y(&self, y: f64) -> f64 {
        y
    }

    /// `TforY`.
    pub fn t_for_y(&self, _y: f64) -> f64 {
        0.0
    }

    /// `XforT`.
    pub fn x_for_t(&self, _t: f64) -> f64 {
        self.x
    }

    /// `YforT`.
    pub fn y_for_t(&self, _t: f64) -> f64 {
        self.y
    }

    /// `dYforT`.
    pub fn d_y_for_t(&self, _t: f64, _deriv: i32) -> f64 {
        0.0
    }

    /// `nextVertical`.
    pub fn next_vertical(&self, _t0: f64, t1: f64) -> f64 {
        t1
    }

    pub fn enlarge(&self, r: &mut Rect) {
        r.add(self.x, self.y);
    }

    pub fn get_segment(&self, coords: &mut [f64; 6]) -> i32 {
        coords[0] = self.x;
        coords[1] = self.y;
        SEG_MOVETO
    }
}
