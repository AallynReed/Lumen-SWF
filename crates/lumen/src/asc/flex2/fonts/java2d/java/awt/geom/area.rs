//! Port of JDK 17 `java.awt.geom.Area` and its `AreaIterator`, on top of the `sun.awt.geom` curve sweep.

use std::sync::Arc;

use super::affine_transform::AffineTransform;
use super::shapes::{coords_for, Seg, SEG_CLOSE, SEG_CUBICTO, SEG_LINETO, SEG_MOVETO, SEG_QUADTO, WIND_EVEN_ODD};
use crate::asc::flex2::fonts::java2d::sun::awt::geom::area_op::{AreaOp, OpKind};
use crate::asc::flex2::fonts::java2d::sun::awt::geom::curve::{self, Curve, Rect};

/// `Area`. Clones share their curves, as `new Area(area)` does.
#[derive(Clone, Debug, Default)]
pub struct Area {
    curves: Vec<Arc<Curve>>,
}

impl Area {
    pub fn new() -> Area {
        Area { curves: Vec::new() }
    }

    /// `new Area(shape)`: from a shape's `getPathIterator(null)` segments and winding rule (`pathToCurves`).
    pub fn from_path(segments: &[Seg], winding_rule: i32) -> Area {
        let mut curves = Vec::new();
        let mut coords = [0.0f64; 23];
        let (mut movx, mut movy, mut curx, mut cury) = (0.0, 0.0, 0.0, 0.0);
        for &(t, c) in segments {
            let n = coords_for(t);
            coords[..n].copy_from_slice(&c[..n]);
            match t {
                SEG_MOVETO => {
                    curve::insert_line(&mut curves, curx, cury, movx, movy);
                    movx = coords[0];
                    curx = movx;
                    movy = coords[1];
                    cury = movy;
                    curve::insert_move(&mut curves, movx, movy);
                }
                SEG_LINETO => {
                    let (newx, newy) = (coords[0], coords[1]);
                    curve::insert_line(&mut curves, curx, cury, newx, newy);
                    curx = newx;
                    cury = newy;
                }
                SEG_QUADTO => {
                    let (newx, newy) = (coords[2], coords[3]);
                    curve::insert_quad(&mut curves, curx, cury, &mut coords);
                    curx = newx;
                    cury = newy;
                }
                SEG_CUBICTO => {
                    let (newx, newy) = (coords[4], coords[5]);
                    curve::insert_cubic(&mut curves, curx, cury, &mut coords);
                    curx = newx;
                    cury = newy;
                }
                SEG_CLOSE => {
                    curve::insert_line(&mut curves, curx, cury, movx, movy);
                    curx = movx;
                    cury = movy;
                }
                _ => {}
            }
        }
        curve::insert_line(&mut curves, curx, cury, movx, movy);
        let kind = if winding_rule == WIND_EVEN_ODD { OpKind::EOWind } else { OpKind::NZWind };
        Area { curves: AreaOp::new(kind).calculate(&curves, &[]) }
    }

    pub fn add(&mut self, rhs: &Area) {
        self.curves = AreaOp::new(OpKind::Add).calculate(&self.curves, &rhs.curves);
    }

    pub fn subtract(&mut self, rhs: &Area) {
        self.curves = AreaOp::new(OpKind::Sub).calculate(&self.curves, &rhs.curves);
    }

    pub fn intersect(&mut self, rhs: &Area) {
        self.curves = AreaOp::new(OpKind::Int).calculate(&self.curves, &rhs.curves);
    }

    /// `exclusiveOr`.
    pub fn exclusive_or(&mut self, rhs: &Area) {
        self.curves = AreaOp::new(OpKind::Xor).calculate(&self.curves, &rhs.curves);
    }

    pub fn reset(&mut self) {
        self.curves = Vec::new();
    }

    pub fn is_empty(&self) -> bool {
        self.curves.is_empty()
    }

    pub fn is_polygonal(&self) -> bool {
        self.curves.iter().all(|c| c.get_order() <= 1)
    }

    /// `isRectangular`; like Java it indexes curves 1 and 2, so a one- or two-curve area panics.
    pub fn is_rectangular(&self) -> bool {
        let size = self.curves.len();
        if size == 0 {
            return true;
        }
        if size > 3 {
            return false;
        }
        let c1 = &self.curves[1];
        let c2 = &self.curves[2];
        if c1.get_order() != 1 || c2.get_order() != 1 {
            return false;
        }
        if c1.get_x_top() != c1.get_x_bot() || c2.get_x_top() != c2.get_x_bot() {
            return false;
        }
        if c1.get_y_top() != c2.get_y_top() || c1.get_y_bot() != c2.get_y_bot() {
            return false;
        }
        true
    }

    /// `getBounds2D` as `Rectangle2D.Double` (x, y, w, h).
    pub fn get_bounds2d(&self) -> (f64, f64, f64, f64) {
        let mut r = Rect::default();
        if let Some(c) = self.curves.first() {
            r.set_rect(c.get_x0(), c.get_y0(), 0.0, 0.0);
            for c in &self.curves[1..] {
                c.enlarge(&mut r);
            }
        }
        (r.x, r.y, r.w, r.h)
    }

    /// `getPathIterator(at)` (an `AreaIterator`, always `WIND_NON_ZERO`): the segments as `currentSegment(double[])`
    /// reports them, unused coordinates zero.
    pub fn get_path_iterator(&self, at: Option<&AffineTransform>) -> Vec<Seg> {
        let curves = &self.curves;
        let mut out = Vec::new();
        let mut index = 0;
        let mut prevcurve: Option<&Arc<Curve>> = None;
        let mut thiscurve: Option<&Arc<Curve>> = curves.first();
        while prevcurve.is_some() || thiscurve.is_some() {
            let mut coords = [0.0f64; 6];
            let segtype;
            let mut numpoints;
            if prevcurve.is_some() {
                match thiscurve {
                    Some(tc) if tc.get_order() != 0 => {
                        coords[0] = tc.get_x0();
                        coords[1] = tc.get_y0();
                        segtype = SEG_LINETO;
                        numpoints = 1;
                    }
                    _ => {
                        segtype = SEG_CLOSE;
                        numpoints = 0;
                    }
                }
            } else {
                let tc = thiscurve.expect("area iterator out of bounds");
                segtype = tc.get_segment(&mut coords);
                numpoints = tc.get_order() as usize;
                if numpoints == 0 {
                    numpoints = 1;
                }
            }
            if segtype != SEG_CLOSE {
                if let Some(at) = at {
                    at.transform_coords(&mut coords[..numpoints * 2]);
                }
            }
            out.push((segtype, coords));
            if prevcurve.is_some() {
                prevcurve = None;
            } else {
                prevcurve = thiscurve;
                index += 1;
                if index < curves.len() {
                    let tc = &curves[index];
                    thiscurve = Some(tc);
                    if tc.get_order() != 0 {
                        let pc = prevcurve.unwrap();
                        if pc.get_x1() == tc.get_x0() && pc.get_y1() == tc.get_y0() {
                            prevcurve = None;
                        }
                    }
                } else {
                    thiscurve = None;
                }
            }
        }
        out
    }
}
