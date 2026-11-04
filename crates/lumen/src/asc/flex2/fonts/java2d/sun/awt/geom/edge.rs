//! Port of JDK 17 `sun.awt.geom.Edge`: a curve tagged with the operand it came from, plus its last comparison.

use std::sync::Arc;

use super::curve::Curve;

/// `Edge`; edges refer to each other by index into the `AreaOp` edge list.
#[derive(Clone, Debug)]
pub struct Edge {
    pub curve: Arc<Curve>,
    pub ctag: i32,
    pub etag: i32,
    pub activey: f64,
    pub equivalence: i32,
    last_edge: Option<usize>,
    last_result: i32,
    last_limit: f64,
}

impl Edge {
    pub fn new(curve: Arc<Curve>, ctag: i32) -> Edge {
        Edge { curve, ctag, etag: 0, activey: 0.0, equivalence: 0, last_edge: None, last_result: 0, last_limit: 0.0 }
    }

    /// `record`.
    pub fn record(&mut self, yend: f64, etag: i32) {
        self.activey = yend;
        self.etag = etag;
    }

    /// `isActiveFor`.
    pub fn is_active_for(&self, y: f64, etag: i32) -> bool {
        self.etag == etag && self.activey >= y
    }
}

/// `edges[this].compareTo(edges[other], yrange)`.
pub fn compare_to(edges: &mut [Edge], this: usize, other: usize, yrange: &mut [f64; 2]) -> i32 {
    let (last_edge, last_limit, last_result) = (edges[this].last_edge, edges[this].last_limit, edges[this].last_result);
    if last_edge == Some(other) && yrange[0] < last_limit {
        if yrange[1] > last_limit {
            yrange[1] = last_limit;
        }
        return last_result;
    }
    let o = &edges[other];
    if o.last_edge == Some(this) && yrange[0] < o.last_limit {
        if yrange[1] > o.last_limit {
            yrange[1] = o.last_limit;
        }
        return 0 - o.last_result;
    }
    let ret = edges[this].curve.compare_to(&edges[other].curve, yrange);
    let e = &mut edges[this];
    e.last_edge = Some(other);
    e.last_limit = yrange[1];
    e.last_result = ret;
    ret
}
