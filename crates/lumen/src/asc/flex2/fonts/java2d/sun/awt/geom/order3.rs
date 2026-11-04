//! Port of JDK 17 `sun.awt.geom.Order3`: a cubic segment, monotonic in y and stored top to bottom. `TforY`'s
//! three-entry cache is left out: it only memoizes a pure function.

use std::sync::Arc;

use super::curve::{Curve, Rect, INCREASING};
use super::math;
use super::order2::Order2;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::SEG_CUBICTO;

/// `Order3`.
#[derive(Clone, Debug)]
pub struct Order3 {
    pub direction: i32,
    x0: f64,
    y0: f64,
    cx0: f64,
    cy0: f64,
    cx1: f64,
    cy1: f64,
    x1: f64,
    y1: f64,
    xmin: f64,
    xmax: f64,
    xcoeff0: f64,
    xcoeff1: f64,
    xcoeff2: f64,
    xcoeff3: f64,
    ycoeff0: f64,
    ycoeff1: f64,
    ycoeff2: f64,
    ycoeff3: f64,
}

/// `QuadCurve2D.solveQuadratic(eqn, res)` with `eqn` and `res` the same array.
fn solve_quadratic(eqn: &mut [f64]) -> i32 {
    let a = eqn[2];
    let b = eqn[1];
    let c = eqn[0];
    let mut roots = 0;
    if a == 0.0 {
        if b == 0.0 {
            return -1;
        }
        eqn[roots] = -c / b;
        roots += 1;
    } else {
        let mut d = b * b - 4.0 * a * c;
        if d < 0.0 {
            return 0;
        }
        d = d.sqrt();
        if b < 0.0 {
            d = -d;
        }
        let q = (b + d) / -2.0;
        eqn[roots] = q / a;
        roots += 1;
        if q != 0.0 {
            eqn[roots] = c / q;
            roots += 1;
        }
    }
    roots as i32
}

impl Order3 {
    /// `insert`: splits the cubic at its y extrema into monotonic pieces.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        curves: &mut Vec<Arc<Curve>>,
        tmp: &mut [f64; 23],
        x0: f64,
        y0: f64,
        cx0: f64,
        cy0: f64,
        cx1: f64,
        cy1: f64,
        x1: f64,
        y1: f64,
        direction: i32,
    ) {
        let mut numparams = Self::get_horizontal_params(y0, cy0, cy1, y1, tmp);
        if numparams == 0 {
            Self::add_instance(curves, x0, y0, cx0, cy0, cx1, cy1, x1, y1, direction);
            return;
        }
        tmp[3] = x0;
        tmp[4] = y0;
        tmp[5] = cx0;
        tmp[6] = cy0;
        tmp[7] = cx1;
        tmp[8] = cy1;
        tmp[9] = x1;
        tmp[10] = y1;
        let mut t = tmp[0];
        if numparams > 1 && t > tmp[1] {
            tmp[0] = tmp[1];
            tmp[1] = t;
            t = tmp[0];
        }
        Self::split(tmp, 3, t);
        if numparams > 1 {
            t = (tmp[1] - t) / (1.0 - t);
            Self::split(tmp, 9, t);
        }
        let mut index = 3i32;
        if direction == -1 {
            index += numparams * 6;
        }
        while numparams >= 0 {
            let i = index as usize;
            Self::add_instance(curves, tmp[i], tmp[i + 1], tmp[i + 2], tmp[i + 3], tmp[i + 4], tmp[i + 5], tmp[i + 6], tmp[i + 7], direction);
            numparams -= 1;
            if direction == INCREASING {
                index += 6;
            } else {
                index -= 6;
            }
        }
    }

    /// `addInstance`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_instance(curves: &mut Vec<Arc<Curve>>, x0: f64, y0: f64, cx0: f64, cy0: f64, cx1: f64, cy1: f64, x1: f64, y1: f64, direction: i32) {
        if y0 > y1 {
            curves.push(Arc::new(Curve::Order3(Order3::new(x1, y1, cx1, cy1, cx0, cy0, x0, y0, -direction))));
        } else if y1 > y0 {
            curves.push(Arc::new(Curve::Order3(Order3::new(x0, y0, cx0, cy0, cx1, cy1, x1, y1, direction))));
        }
    }

    /// `getHorizontalParams`.
    pub fn get_horizontal_params(c0: f64, mut cp0: f64, mut cp1: f64, mut c1: f64, ret: &mut [f64]) -> i32 {
        if c0 <= cp0 && cp0 <= cp1 && cp1 <= c1 {
            return 0;
        }
        c1 -= cp1;
        cp1 -= cp0;
        cp0 -= c0;
        ret[0] = cp0;
        ret[1] = (cp1 - cp0) * 2.0;
        ret[2] = c1 - cp1 - cp1 + cp0;
        let numroots = solve_quadratic(ret);
        let mut j = 0;
        let mut i = 0;
        while i < numroots {
            let t = ret[i as usize];
            if t > 0.0 && t < 1.0 {
                if j < i {
                    ret[j as usize] = t;
                }
                j += 1;
            }
            i += 1;
        }
        j
    }

    /// `split`.
    pub fn split(coords: &mut [f64], pos: usize, t: f64) {
        let mut x3 = coords[pos + 6];
        coords[pos + 12] = x3;
        let mut y3 = coords[pos + 7];
        coords[pos + 13] = y3;
        let mut x2 = coords[pos + 4];
        let mut y2 = coords[pos + 5];
        x3 = x2 + (x3 - x2) * t;
        y3 = y2 + (y3 - y2) * t;
        let mut x0 = coords[pos];
        let mut y0 = coords[pos + 1];
        let mut x1 = coords[pos + 2];
        let mut y1 = coords[pos + 3];
        x0 = x0 + (x1 - x0) * t;
        y0 = y0 + (y1 - y0) * t;
        x1 = x1 + (x2 - x1) * t;
        y1 = y1 + (y2 - y1) * t;
        x2 = x1 + (x3 - x1) * t;
        y2 = y1 + (y3 - y1) * t;
        x1 = x0 + (x1 - x0) * t;
        y1 = y0 + (y1 - y0) * t;
        coords[pos + 2] = x0;
        coords[pos + 3] = y0;
        coords[pos + 4] = x1;
        coords[pos + 5] = y1;
        coords[pos + 6] = x1 + (x2 - x1) * t;
        coords[pos + 7] = y1 + (y2 - y1) * t;
        coords[pos + 8] = x2;
        coords[pos + 9] = y2;
        coords[pos + 10] = x3;
        coords[pos + 11] = y3;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(x0: f64, y0: f64, cx0: f64, mut cy0: f64, cx1: f64, mut cy1: f64, x1: f64, y1: f64, direction: i32) -> Order3 {
        if cy0 < y0 {
            cy0 = y0;
        }
        if cy1 > y1 {
            cy1 = y1;
        }
        Order3 {
            direction,
            x0,
            y0,
            cx0,
            cy0,
            cx1,
            cy1,
            x1,
            y1,
            xmin: math::min(math::min(x0, x1), math::min(cx0, cx1)),
            xmax: math::max(math::max(x0, x1), math::max(cx0, cx1)),
            xcoeff0: x0,
            xcoeff1: (cx0 - x0) * 3.0,
            xcoeff2: (cx1 - cx0 - cx0 + x0) * 3.0,
            xcoeff3: x1 - (cx1 - cx0) * 3.0 - x0,
            ycoeff0: y0,
            ycoeff1: (cy0 - y0) * 3.0,
            ycoeff2: (cy1 - cy0 - cy0 + y0) * 3.0,
            ycoeff3: y1 - (cy1 - cy0) * 3.0 - y0,
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

    /// `TforY`.
    pub fn t_for_y(&self, y: f64) -> f64 {
        if y <= self.y0 {
            return 0.0;
        }
        if y >= self.y1 {
            return 1.0;
        }
        if self.ycoeff3 == 0.0 {
            return Order2::t_for_y_coeffs(y, self.ycoeff0, self.ycoeff1, self.ycoeff2);
        }
        let a = self.ycoeff2 / self.ycoeff3;
        let b = self.ycoeff1 / self.ycoeff3;
        let c = (self.ycoeff0 - y) / self.ycoeff3;
        let mut q = (a * a - 3.0 * b) / 9.0;
        let mut r = (2.0 * a * a * a - 9.0 * a * b + 27.0 * c) / 54.0;
        let r2 = r * r;
        let q3 = q * q * q;
        let a_3 = a / 3.0;
        let mut t;
        if r2 < q3 {
            let theta = math::acos(r / q3.sqrt());
            q = -2.0 * q.sqrt();
            t = self.refine(a, b, c, y, q * math::cos(theta / 3.0) - a_3);
            if t < 0.0 {
                t = self.refine(a, b, c, y, q * math::cos((theta + 6.283185307179586) / 3.0) - a_3);
            }
            if t < 0.0 {
                t = self.refine(a, b, c, y, q * math::cos((theta - 6.283185307179586) / 3.0) - a_3);
            }
        } else {
            let neg = r < 0.0;
            let s = (r2 - q3).sqrt();
            if neg {
                r = -r;
            }
            let mut big_a = math::pow_one_third(r + s);
            if !neg {
                big_a = -big_a;
            }
            let big_b = if big_a == 0.0 { 0.0 } else { q / big_a };
            t = self.refine(a, b, c, y, (big_a + big_b) - a_3);
        }
        if t < 0.0 {
            let mut t0 = 0.0;
            let mut t1 = 1.0;
            loop {
                t = (t0 + t1) / 2.0;
                if t == t0 || t == t1 {
                    break;
                }
                let yt = self.y_for_t(t);
                if yt < y {
                    t0 = t;
                } else if yt > y {
                    t1 = t;
                } else {
                    break;
                }
            }
        }
        t
    }

    /// `refine`.
    pub fn refine(&self, _a: f64, _b: f64, _c: f64, target: f64, mut t: f64) -> f64 {
        if t < -0.1 || t > 1.1 {
            return -1.0;
        }
        let mut y = self.y_for_t(t);
        let (mut t0, mut t1);
        if y < target {
            t0 = t;
            t1 = 1.0;
        } else {
            t0 = 0.0;
            t1 = t;
        }
        let mut useslope = true;
        while y != target {
            if !useslope {
                let t2 = (t0 + t1) / 2.0;
                if t2 == t0 || t2 == t1 {
                    break;
                }
                t = t2;
            } else {
                let slope = self.d_y_for_t(t, 1);
                if slope == 0.0 {
                    useslope = false;
                    continue;
                }
                let t2 = t + ((target - y) / slope);
                if t2 == t || t2 <= t0 || t2 >= t1 {
                    useslope = false;
                    continue;
                }
                t = t2;
            }
            y = self.y_for_t(t);
            if y < target {
                t0 = t;
            } else if y > target {
                t1 = t;
            } else {
                break;
            }
        }
        if t > 1.0 {
            -1.0
        } else {
            t
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

    /// `XforT`.
    pub fn x_for_t(&self, t: f64) -> f64 {
        ((self.xcoeff3 * t + self.xcoeff2) * t + self.xcoeff1) * t + self.xcoeff0
    }

    /// `YforT`.
    pub fn y_for_t(&self, t: f64) -> f64 {
        ((self.ycoeff3 * t + self.ycoeff2) * t + self.ycoeff1) * t + self.ycoeff0
    }

    /// `dYforT`.
    pub fn d_y_for_t(&self, t: f64, deriv: i32) -> f64 {
        match deriv {
            0 => ((self.ycoeff3 * t + self.ycoeff2) * t + self.ycoeff1) * t + self.ycoeff0,
            1 => (3.0 * self.ycoeff3 * t + 2.0 * self.ycoeff2) * t + self.ycoeff1,
            2 => 6.0 * self.ycoeff3 * t + 2.0 * self.ycoeff2,
            3 => 6.0 * self.ycoeff3,
            _ => 0.0,
        }
    }

    /// `nextVertical`.
    pub fn next_vertical(&self, t0: f64, mut t1: f64) -> f64 {
        let mut eqn = [self.xcoeff1, 2.0 * self.xcoeff2, 3.0 * self.xcoeff3];
        let numroots = solve_quadratic(&mut eqn);
        for i in 0..numroots.max(0) as usize {
            if eqn[i] > t0 && eqn[i] < t1 {
                t1 = eqn[i];
            }
        }
        t1
    }

    pub fn enlarge(&self, r: &mut Rect) {
        r.add(self.x0, self.y0);
        let mut eqn = [self.xcoeff1, 2.0 * self.xcoeff2, 3.0 * self.xcoeff3];
        let numroots = solve_quadratic(&mut eqn);
        for &t in eqn.iter().take(numroots.max(0) as usize) {
            if t > 0.0 && t < 1.0 {
                r.add(self.x_for_t(t), self.y_for_t(t));
            }
        }
        r.add(self.x1, self.y1);
    }

    /// `getSubCurve(ystart, yend, dir)`.
    pub fn get_sub_curve(&self, this: &Arc<Curve>, ystart: f64, yend: f64, dir: i32) -> Arc<Curve> {
        if ystart <= self.y0 && yend >= self.y1 {
            return this.get_with_direction(dir);
        }
        let mut eqn = [0.0; 14];
        let mut t0 = self.t_for_y(ystart);
        let mut t1 = self.t_for_y(yend);
        eqn[0] = self.x0;
        eqn[1] = self.y0;
        eqn[2] = self.cx0;
        eqn[3] = self.cy0;
        eqn[4] = self.cx1;
        eqn[5] = self.cy1;
        eqn[6] = self.x1;
        eqn[7] = self.y1;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        if t1 < 1.0 {
            Self::split(&mut eqn, 0, t1);
        }
        let i;
        if t0 <= 0.0 {
            i = 0;
        } else {
            Self::split(&mut eqn, 0, t0 / t1);
            i = 6;
        }
        Arc::new(Curve::Order3(Order3::new(eqn[i], ystart, eqn[i + 2], eqn[i + 3], eqn[i + 4], eqn[i + 5], eqn[i + 6], yend, dir)))
    }

    pub fn get_reversed_curve(&self) -> Order3 {
        Order3::new(self.x0, self.y0, self.cx0, self.cy0, self.cx1, self.cy1, self.x1, self.y1, -self.direction)
    }

    pub fn get_segment(&self, coords: &mut [f64; 6]) -> i32 {
        if self.direction == INCREASING {
            coords[0] = self.cx0;
            coords[1] = self.cy0;
            coords[2] = self.cx1;
            coords[3] = self.cy1;
            coords[4] = self.x1;
            coords[5] = self.y1;
        } else {
            coords[0] = self.cx1;
            coords[1] = self.cy1;
            coords[2] = self.cx0;
            coords[3] = self.cy0;
            coords[4] = self.x0;
            coords[5] = self.y0;
        }
        SEG_CUBICTO
    }
}
