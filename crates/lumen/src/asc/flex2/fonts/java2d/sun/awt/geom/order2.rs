//! Port of JDK 17 `sun.awt.geom.Order2`: a quadratic segment, monotonic in y and stored top to bottom.

use std::sync::Arc;

use super::curve::{Curve, Rect, INCREASING};
use super::math;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::SEG_QUADTO;

/// `Order2`.
#[derive(Clone, Debug)]
pub struct Order2 {
    pub direction: i32,
    x0: f64,
    y0: f64,
    cx0: f64,
    cy0: f64,
    x1: f64,
    y1: f64,
    xmin: f64,
    xmax: f64,
    xcoeff0: f64,
    xcoeff1: f64,
    xcoeff2: f64,
    ycoeff0: f64,
    ycoeff1: f64,
    ycoeff2: f64,
}

impl Order2 {
    /// `insert`: splits the quad at its y extremum into monotonic pieces.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(curves: &mut Vec<Arc<Curve>>, tmp: &mut [f64; 23], x0: f64, y0: f64, cx0: f64, cy0: f64, x1: f64, y1: f64, direction: i32) {
        let numparams = Self::get_horizontal_params(y0, cy0, y1, tmp);
        if numparams == 0 {
            Self::add_instance(curves, x0, y0, cx0, cy0, x1, y1, direction);
            return;
        }
        let t = tmp[0];
        tmp[0] = x0;
        tmp[1] = y0;
        tmp[2] = cx0;
        tmp[3] = cy0;
        tmp[4] = x1;
        tmp[5] = y1;
        Self::split(tmp, 0, t);
        let i0 = if direction == INCREASING { 0 } else { 4 };
        let i1 = 4 - i0;
        Self::add_instance(curves, tmp[i0], tmp[i0 + 1], tmp[i0 + 2], tmp[i0 + 3], tmp[i0 + 4], tmp[i0 + 5], direction);
        Self::add_instance(curves, tmp[i1], tmp[i1 + 1], tmp[i1 + 2], tmp[i1 + 3], tmp[i1 + 4], tmp[i1 + 5], direction);
    }

    /// `addInstance`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_instance(curves: &mut Vec<Arc<Curve>>, x0: f64, y0: f64, cx0: f64, cy0: f64, x1: f64, y1: f64, direction: i32) {
        if y0 > y1 {
            curves.push(Arc::new(Curve::Order2(Order2::new(x1, y1, cx0, cy0, x0, y0, -direction))));
        } else if y1 > y0 {
            curves.push(Arc::new(Curve::Order2(Order2::new(x0, y0, cx0, cy0, x1, y1, direction))));
        }
    }

    /// `getHorizontalParams`.
    pub fn get_horizontal_params(mut c0: f64, cp: f64, mut c1: f64, ret: &mut [f64]) -> i32 {
        if c0 <= cp && cp <= c1 {
            return 0;
        }
        c0 -= cp;
        c1 -= cp;
        let denom = c0 + c1;
        if denom == 0.0 {
            return 0;
        }
        let t = c0 / denom;
        if t <= 0.0 || t >= 1.0 {
            return 0;
        }
        ret[0] = t;
        1
    }

    /// `split`.
    pub fn split(coords: &mut [f64], pos: usize, t: f64) {
        let mut x1 = coords[pos + 4];
        coords[pos + 8] = x1;
        let mut y1 = coords[pos + 5];
        coords[pos + 9] = y1;
        let mut cx = coords[pos + 2];
        let mut cy = coords[pos + 3];
        x1 = cx + (x1 - cx) * t;
        y1 = cy + (y1 - cy) * t;
        let mut x0 = coords[pos];
        let mut y0 = coords[pos + 1];
        x0 = x0 + (cx - x0) * t;
        y0 = y0 + (cy - y0) * t;
        cx = x0 + (x1 - x0) * t;
        cy = y0 + (y1 - y0) * t;
        coords[pos + 2] = x0;
        coords[pos + 3] = y0;
        coords[pos + 4] = cx;
        coords[pos + 5] = cy;
        coords[pos + 6] = x1;
        coords[pos + 7] = y1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(x0: f64, y0: f64, cx0: f64, mut cy0: f64, x1: f64, y1: f64, direction: i32) -> Order2 {
        if cy0 < y0 {
            cy0 = y0;
        } else if cy0 > y1 {
            cy0 = y1;
        }
        Order2 {
            direction,
            x0,
            y0,
            cx0,
            cy0,
            x1,
            y1,
            xmin: math::min(math::min(x0, x1), cx0),
            xmax: math::max(math::max(x0, x1), cx0),
            xcoeff0: x0,
            xcoeff1: cx0 + cx0 - x0 - x0,
            xcoeff2: x0 - cx0 - cx0 + x1,
            ycoeff0: y0,
            ycoeff1: cy0 + cy0 - y0 - y0,
            ycoeff2: y0 - cy0 - cy0 + y1,
        }
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
        if y <= self.y0 {
            return self.x0;
        }
        if y >= self.y1 {
            return self.x1;
        }
        self.x_for_t(self.t_for_y(y))
    }

    /// `TforY`.
    pub fn t_for_y(&self, y: f64) -> f64 {
        if y <= self.y0 {
            return 0.0;
        }
        if y >= self.y1 {
            return 1.0;
        }
        Self::t_for_y_coeffs(y, self.ycoeff0, self.ycoeff1, self.ycoeff2)
    }

    /// The static `TforY(y, ycoeff0, ycoeff1, ycoeff2)`.
    pub fn t_for_y_coeffs(y: f64, mut ycoeff0: f64, ycoeff1: f64, ycoeff2: f64) -> f64 {
        ycoeff0 -= y;
        if ycoeff2 == 0.0 {
            let root = -ycoeff0 / ycoeff1;
            if root >= 0.0 && root <= 1.0 {
                return root;
            }
        } else {
            let mut d = ycoeff1 * ycoeff1 - 4.0 * ycoeff2 * ycoeff0;
            if d >= 0.0 {
                d = d.sqrt();
                if ycoeff1 < 0.0 {
                    d = -d;
                }
                let q = (ycoeff1 + d) / -2.0;
                let root = q / ycoeff2;
                if root >= 0.0 && root <= 1.0 {
                    return root;
                }
                if q != 0.0 {
                    let root = ycoeff0 / q;
                    if root >= 0.0 && root <= 1.0 {
                        return root;
                    }
                }
            }
        }
        let y0 = ycoeff0;
        let y1 = ycoeff0 + ycoeff1 + ycoeff2;
        if 0.0 < (y0 + y1) / 2.0 {
            0.0
        } else {
            1.0
        }
    }

    /// `XforT`.
    pub fn x_for_t(&self, t: f64) -> f64 {
        (self.xcoeff2 * t + self.xcoeff1) * t + self.xcoeff0
    }

    /// `YforT`.
    pub fn y_for_t(&self, t: f64) -> f64 {
        (self.ycoeff2 * t + self.ycoeff1) * t + self.ycoeff0
    }

    /// `dYforT`.
    pub fn d_y_for_t(&self, t: f64, deriv: i32) -> f64 {
        match deriv {
            0 => (self.ycoeff2 * t + self.ycoeff1) * t + self.ycoeff0,
            1 => 2.0 * self.ycoeff2 * t + self.ycoeff1,
            2 => 2.0 * self.ycoeff2,
            _ => 0.0,
        }
    }

    /// `nextVertical`.
    pub fn next_vertical(&self, t0: f64, t1: f64) -> f64 {
        let t = -self.xcoeff1 / (2.0 * self.xcoeff2);
        if t > t0 && t < t1 {
            return t;
        }
        t1
    }

    pub fn enlarge(&self, r: &mut Rect) {
        r.add(self.x0, self.y0);
        let t = -self.xcoeff1 / (2.0 * self.xcoeff2);
        if t > 0.0 && t < 1.0 {
            r.add(self.x_for_t(t), self.y_for_t(t));
        }
        r.add(self.x1, self.y1);
    }

    /// `getSubCurve(ystart, yend, dir)`.
    pub fn get_sub_curve(&self, this: &Arc<Curve>, ystart: f64, yend: f64, dir: i32) -> Arc<Curve> {
        let t0;
        let t1;
        if ystart <= self.y0 {
            if yend >= self.y1 {
                return this.get_with_direction(dir);
            }
            t0 = 0.0;
        } else {
            t0 = Self::t_for_y_coeffs(ystart, self.ycoeff0, self.ycoeff1, self.ycoeff2);
        }
        if yend >= self.y1 {
            t1 = 1.0;
        } else {
            t1 = Self::t_for_y_coeffs(yend, self.ycoeff0, self.ycoeff1, self.ycoeff2);
        }
        let mut eqn = [0.0; 10];
        eqn[0] = self.x0;
        eqn[1] = self.y0;
        eqn[2] = self.cx0;
        eqn[3] = self.cy0;
        eqn[4] = self.x1;
        eqn[5] = self.y1;
        if t1 < 1.0 {
            Self::split(&mut eqn, 0, t1);
        }
        let i;
        if t0 <= 0.0 {
            i = 0;
        } else {
            Self::split(&mut eqn, 0, t0 / t1);
            i = 4;
        }
        Arc::new(Curve::Order2(Order2::new(eqn[i], ystart, eqn[i + 2], eqn[i + 3], eqn[i + 4], yend, dir)))
    }

    pub fn get_reversed_curve(&self) -> Order2 {
        Order2::new(self.x0, self.y0, self.cx0, self.cy0, self.x1, self.y1, -self.direction)
    }

    pub fn get_segment(&self, coords: &mut [f64; 6]) -> i32 {
        coords[0] = self.cx0;
        coords[1] = self.cy0;
        if self.direction == INCREASING {
            coords[2] = self.x1;
            coords[3] = self.y1;
        } else {
            coords[2] = self.x0;
            coords[3] = self.y0;
        }
        SEG_QUADTO
    }
}
