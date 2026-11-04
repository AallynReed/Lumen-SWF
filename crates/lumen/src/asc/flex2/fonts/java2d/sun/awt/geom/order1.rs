//! Port of JDK 17 `sun.awt.geom.Order1`: a line segment, stored top to bottom.

use std::sync::Arc;

use super::curve::{orderof, Curve, Rect, INCREASING};
use super::math;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::SEG_LINETO;

/// `Order1`.
#[derive(Clone, Debug)]
pub struct Order1 {
    pub direction: i32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    xmin: f64,
    xmax: f64,
}

impl Order1 {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64, direction: i32) -> Order1 {
        let (xmin, xmax) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
        Order1 { direction, x0, y0, x1, y1, xmin, xmax }
    }

    pub fn get_x_top(&self) -> f64 {
        self.x0
    }

    pub fn get_y_top(&self) -> f64 {
        self.y0
    }

    pub fn get_x_bot(&self) -> f64 {
        self.x1
    }

    pub fn get_y_bot(&self) -> f64 {
        self.y1
    }

    pub fn get_x_min(&self) -> f64 {
        self.xmin
    }

    pub fn get_x_max(&self) -> f64 {
        self.xmax
    }

    pub fn get_x0(&self) -> f64 {
        if self.direction == INCREASING {
            self.x0
        } else {
            self.x1
        }
    }

    pub fn get_y0(&self) -> f64 {
        if self.direction == INCREASING {
            self.y0
        } else {
            self.y1
        }
    }

    pub fn get_x1(&self) -> f64 {
        if self.direction == -1 {
            self.x0
        } else {
            self.x1
        }
    }

    pub fn get_y1(&self) -> f64 {
        if self.direction == -1 {
            self.y0
        } else {
            self.y1
        }
    }

    /// `XforY`.
    pub fn x_for_y(&self, y: f64) -> f64 {
        if self.x0 == self.x1 || y <= self.y0 {
            return self.x0;
        }
        if y >= self.y1 {
            return self.x1;
        }
        self.x0 + (y - self.y0) * (self.x1 - self.x0) / (self.y1 - self.y0)
    }

    /// `TforY`.
    pub fn t_for_y(&self, y: f64) -> f64 {
        if y <= self.y0 {
            return 0.0;
        }
        if y >= self.y1 {
            return 1.0;
        }
        (y - self.y0) / (self.y1 - self.y0)
    }

    /// `XforT`.
    pub fn x_for_t(&self, t: f64) -> f64 {
        self.x0 + t * (self.x1 - self.x0)
    }

    /// `YforT`.
    pub fn y_for_t(&self, t: f64) -> f64 {
        self.y0 + t * (self.y1 - self.y0)
    }

    /// `dYforT`.
    pub fn d_y_for_t(&self, t: f64, deriv: i32) -> f64 {
        match deriv {
            0 => self.y0 + t * (self.y1 - self.y0),
            1 => self.y1 - self.y0,
            _ => 0.0,
        }
    }

    /// `nextVertical`.
    pub fn next_vertical(&self, _t0: f64, t1: f64) -> f64 {
        t1
    }

    pub fn enlarge(&self, r: &mut Rect) {
        r.add(self.x0, self.y0);
        r.add(self.x1, self.y1);
    }

    /// `getSubCurve(ystart, yend, dir)`.
    pub fn get_sub_curve(&self, this: &Arc<Curve>, ystart: f64, yend: f64, dir: i32) -> Arc<Curve> {
        if ystart == self.y0 && yend == self.y1 {
            return this.get_with_direction(dir);
        }
        if self.x0 == self.x1 {
            return Arc::new(Curve::Order1(Order1::new(self.x0, ystart, self.x1, yend, dir)));
        }
        let num = self.x0 - self.x1;
        let denom = self.y0 - self.y1;
        let xstart = self.x0 + (ystart - self.y0) * num / denom;
        let xend = self.x0 + (yend - self.y0) * num / denom;
        Arc::new(Curve::Order1(Order1::new(xstart, ystart, xend, yend, dir)))
    }

    pub fn get_reversed_curve(&self) -> Order1 {
        Order1::new(self.x0, self.y0, self.x1, self.y1, -self.direction)
    }

    /// `Order1.compareTo` for two lines.
    pub fn compare_to(&self, c1: &Order1, yrange: &mut [f64; 2]) -> i32 {
        if yrange[1] <= yrange[0] {
            panic!("yrange already screwed up...");
        }
        yrange[1] = math::min(math::min(yrange[1], self.y1), c1.y1);
        if yrange[1] <= yrange[0] {
            panic!("backstepping from {} to {}", yrange[0], yrange[1]);
        }
        if self.xmax <= c1.xmin {
            return if self.xmin == c1.xmax { 0 } else { -1 };
        }
        if self.xmin >= c1.xmax {
            return 1;
        }
        let dxa = self.x1 - self.x0;
        let dya = self.y1 - self.y0;
        let dxb = c1.x1 - c1.x0;
        let dyb = c1.y1 - c1.y0;
        let denom = dxb * dya - dxa * dyb;
        let y;
        if denom != 0.0 {
            let num = (self.x0 - c1.x0) * dya * dyb - self.y0 * dxa * dyb + c1.y0 * dxb * dya;
            let yc = num / denom;
            if yc <= yrange[0] {
                y = math::min(self.y1, c1.y1);
            } else {
                if yc < yrange[1] {
                    yrange[1] = yc;
                }
                y = math::max(self.y0, c1.y0);
            }
        } else {
            y = math::max(self.y0, c1.y0);
        }
        orderof(self.x_for_y(y), c1.x_for_y(y))
    }

    pub fn get_segment(&self, coords: &mut [f64; 6]) -> i32 {
        if self.direction == INCREASING {
            coords[0] = self.x1;
            coords[1] = self.y1;
        } else {
            coords[0] = self.x0;
            coords[1] = self.y0;
        }
        SEG_LINETO
    }
}
