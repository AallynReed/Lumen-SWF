//! `sun.java2d.marlin.Curve`.

use super::helpers;

/// `Curve`.
#[derive(Clone, Copy, Default, Debug)]
pub struct Curve {
    pub ax: f64,
    pub ay: f64,
    pub bx: f64,
    pub by: f64,
    pub cx: f64,
    pub cy: f64,
    pub dx: f64,
    pub dy: f64,
    pub dax: f64,
    pub day: f64,
    pub dbx: f64,
    pub dby: f64,
}

impl Curve {
    /// `Curve.set(double[], int)`.
    pub fn set(&mut self, points: &[f64], type_: usize) {
        match type_ {
            8 => self.set8(points[0], points[1], points[2], points[3], points[4], points[5], points[6], points[7]),
            4 => self.set4(points[0], points[1], points[2], points[3]),
            _ => self.set6(points[0], points[1], points[2], points[3], points[4], points[5]),
        }
    }

    /// `Curve.set(double x8)`.
    #[allow(clippy::too_many_arguments)]
    pub fn set8(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64) {
        let dx32 = 3.0 * (x3 - x2);
        let dy32 = 3.0 * (y3 - y2);
        let dx21 = 3.0 * (x2 - x1);
        let dy21 = 3.0 * (y2 - y1);
        self.ax = (x4 - x1) - dx32;
        self.ay = (y4 - y1) - dy32;
        self.bx = dx32 - dx21;
        self.by = dy32 - dy21;
        self.cx = dx21;
        self.cy = dy21;
        self.dx = x1;
        self.dy = y1;
        self.dax = 3.0 * self.ax;
        self.day = 3.0 * self.ay;
        self.dbx = 2.0 * self.bx;
        self.dby = 2.0 * self.by;
    }

    /// `Curve.set(double x6)`.
    pub fn set6(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let dx21 = x2 - x1;
        let dy21 = y2 - y1;
        self.ax = 0.0;
        self.ay = 0.0;
        self.bx = (x3 - x2) - dx21;
        self.by = (y3 - y2) - dy21;
        self.cx = 2.0 * dx21;
        self.cy = 2.0 * dy21;
        self.dx = x1;
        self.dy = y1;
        self.dax = 0.0;
        self.day = 0.0;
        self.dbx = 2.0 * self.bx;
        self.dby = 2.0 * self.by;
    }

    /// `Curve.set(double x4)`.
    pub fn set4(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let dx21 = x2 - x1;
        let dy21 = y2 - y1;
        self.ax = 0.0;
        self.ay = 0.0;
        self.bx = 0.0;
        self.by = 0.0;
        self.cx = dx21;
        self.cy = dy21;
        self.dx = x1;
        self.dy = y1;
        self.dax = 0.0;
        self.day = 0.0;
        self.dbx = 0.0;
        self.dby = 0.0;
    }

    /// `Curve.dxRoots`.
    pub fn dx_roots(&self, roots: &mut [f64], off: usize) -> usize {
        helpers::quadratic_roots(self.dax, self.dbx, self.cx, roots, off)
    }

    /// `Curve.dyRoots`.
    pub fn dy_roots(&self, roots: &mut [f64], off: usize) -> usize {
        helpers::quadratic_roots(self.day, self.dby, self.cy, roots, off)
    }

    /// `Curve.infPoints`.
    pub fn inf_points(&self, pts: &mut [f64], off: usize) -> usize {
        let a = self.dax * self.dby - self.dbx * self.day;
        let b = 2.0 * (self.cy * self.dax - self.day * self.cx);
        let c = self.cy * self.dbx - self.cx * self.dby;
        helpers::quadratic_roots(a, b, c, pts, off)
    }

    /// `Curve.xPoints`.
    pub fn x_points(&self, ts: &mut [f64], off: usize, x: f64) -> usize {
        helpers::cubic_roots_in_ab(self.ax, self.bx, self.cx, self.dx - x, ts, off, 0.0, 1.0)
    }

    /// `Curve.yPoints`.
    pub fn y_points(&self, ts: &mut [f64], off: usize, y: f64) -> usize {
        helpers::cubic_roots_in_ab(self.ay, self.by, self.cy, self.dy - y, ts, off, 0.0, 1.0)
    }

    /// `Curve.perpendiculardfddf`.
    fn perpendiculardfddf(&self, pts: &mut [f64], off: usize) -> usize {
        let a = 2.0 * (self.dax * self.dax + self.day * self.day);
        let b = 3.0 * (self.dax * self.dbx + self.day * self.dby);
        let c = 2.0 * (self.dax * self.cx + self.day * self.cy) + self.dbx * self.dbx + self.dby * self.dby;
        let d = self.dbx * self.cx + self.dby * self.cy;
        helpers::cubic_roots_in_ab(a, b, c, d, pts, off, 0.0, 1.0)
    }

    /// `Curve.rootsOfROCMinusW`.
    pub fn roots_of_roc_minus_w(&self, roots: &mut [f64], off: usize, w2: f64, err: f64) -> usize {
        let mut ret = off;
        let num_perp_pts = off + self.perpendiculardfddf(roots, off);
        roots[num_perp_pts] = 1.0;

        let mut t0 = 0.0;
        let mut ft0 = self.roc_sq(t0) - w2;

        for i in off..=num_perp_pts {
            let t1 = roots[i];
            let ft1 = self.roc_sq(t1) - w2;
            if ft0 == 0.0 {
                roots[ret] = t0;
                ret += 1;
            } else if ft1 * ft0 < 0.0 {
                roots[ret] = self.false_position_roc_sq_minus_x(t0, t1, w2, err);
                ret += 1;
            }
            t0 = t1;
            ft0 = ft1;
        }

        ret - off
    }

    /// `Curve.eliminateInf`.
    fn eliminate_inf(x: f64) -> f64 {
        if x == f64::INFINITY {
            f64::MAX
        } else if x == f64::NEG_INFINITY {
            f64::from_bits(1)
        } else {
            x
        }
    }

    /// `Curve.falsePositionROCsqMinusX`.
    fn false_position_roc_sq_minus_x(&self, t0: f64, t1: f64, w2: f64, err: f64) -> f64 {
        let iter_limit = 100;
        let mut side: i32 = 0;
        let mut t = t1;
        let mut ft = Self::eliminate_inf(self.roc_sq(t) - w2);
        let mut s = t0;
        let mut fs = Self::eliminate_inf(self.roc_sq(s) - w2);
        let mut r = s;

        let mut i = 0;
        while i < iter_limit && (t - s).abs() > err * (t + s).abs() {
            r = (fs * t - ft * s) / (fs - ft);
            let fr = self.roc_sq(r) - w2;
            if Self::same_sign(fr, ft) {
                ft = fr;
                t = r;
                if side < 0 {
                    fs /= 1i32.wrapping_shl((-side) as u32) as f64;
                    side -= 1;
                } else {
                    side = -1;
                }
            } else if fr * fs > 0.0 {
                fs = fr;
                s = r;
                if side > 0 {
                    ft /= 1i32.wrapping_shl(side as u32) as f64;
                    side += 1;
                } else {
                    side = 1;
                }
            } else {
                break;
            }
            i += 1;
        }
        r
    }

    /// `Curve.sameSign`.
    fn same_sign(x: f64, y: f64) -> bool {
        (x < 0.0 && y < 0.0) || (x > 0.0 && y > 0.0)
    }

    /// `Curve.ROCsq`.
    fn roc_sq(&self, t: f64) -> f64 {
        let dx = t * (t * self.dax + self.dbx) + self.cx;
        let dy = t * (t * self.day + self.dby) + self.cy;
        let ddx = 2.0 * self.dax * t + self.dbx;
        let ddy = 2.0 * self.day * t + self.dby;
        let dx2dy2 = dx * dx + dy * dy;
        let ddx2ddy2 = ddx * ddx + ddy * ddy;
        let ddxdxddydy = ddx * dx + ddy * dy;
        dx2dy2 * ((dx2dy2 * dx2dy2) / (dx2dy2 * ddx2ddy2 - ddxdxddydy * ddxdxddydy))
    }
}
