//! Port of JDK 17 `sun.awt.geom.CurveLink`: the y range of one curve that belongs to the result outline.

use std::sync::Arc;

use super::curve::Curve;
use super::math;
use super::order0::Order0;

/// `CurveLink`; `next` is an index into the same arena.
#[derive(Clone, Debug)]
pub struct CurveLink {
    pub curve: Arc<Curve>,
    pub ytop: f64,
    pub ybot: f64,
    pub etag: i32,
    pub next: Option<usize>,
}

impl CurveLink {
    pub fn new(curve: Arc<Curve>, ystart: f64, yend: f64, etag: i32) -> CurveLink {
        if ystart < curve.get_y_top() || yend > curve.get_y_bot() {
            panic!("bad curvelink [{}=>{}] for {:?}", ystart, yend, curve);
        }
        CurveLink { curve, ytop: ystart, ybot: yend, etag, next: None }
    }

    /// `absorb(Curve, ystart, yend, etag)`; the curve is matched by identity.
    pub fn absorb(&mut self, curve: &Arc<Curve>, ystart: f64, yend: f64, etag: i32) -> bool {
        if !Arc::ptr_eq(&self.curve, curve) || self.etag != etag || self.ybot < ystart || self.ytop > yend {
            return false;
        }
        if ystart < curve.get_y_top() || yend > curve.get_y_bot() {
            panic!("bad curvelink [{}=>{}] for {:?}", ystart, yend, curve);
        }
        self.ytop = math::min(self.ytop, ystart);
        self.ybot = math::max(self.ybot, yend);
        true
    }

    /// `getSubCurve()`.
    pub fn get_sub_curve(&self) -> Arc<Curve> {
        if self.ytop == self.curve.get_y_top() && self.ybot == self.curve.get_y_bot() {
            return self.curve.get_with_direction(self.etag);
        }
        self.curve.get_sub_curve(self.ytop, self.ybot, self.etag)
    }

    /// `getMoveto()`.
    pub fn get_moveto(&self) -> Arc<Curve> {
        Arc::new(Curve::Order0(Order0::new(self.get_x_top(), self.get_y_top())))
    }

    pub fn get_x_top(&self) -> f64 {
        self.curve.x_for_y(self.ytop)
    }

    pub fn get_y_top(&self) -> f64 {
        self.ytop
    }

    pub fn get_x_bot(&self) -> f64 {
        self.curve.x_for_y(self.ybot)
    }

    pub fn get_x(&self) -> f64 {
        self.curve.x_for_y(self.ytop)
    }
}
