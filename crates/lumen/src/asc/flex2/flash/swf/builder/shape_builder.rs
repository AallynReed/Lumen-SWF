//! Ports of swfutils `flash.swf.builder.types.ShapeBuilder`, `ShapeIterator`, `PathIteratorWrapper` and `Point`.

use crate::asc::flex2::flash::swf::types::{Shape, ShapeRecord, StyleChangeRecord};

pub const MOVE_TO: i16 = 0;
pub const LINE_TO: i16 = 1;
pub const QUAD_TO: i16 = 2;
pub const CUBIC_TO: i16 = 3;
pub const CLOSE: i16 = 4;

/// `EdgeRecord.MAX_DELTA_IN_TWIPS`.
pub const MAX_DELTA_IN_TWIPS: i32 = 65535;
/// `SwfConstants.TWIPS_PER_PIXEL`.
pub const TWIPS_PER_PIXEL: i32 = 20;

/// `flash.swf.builder.types.ShapeIterator`.
pub trait ShapeIterator {
    fn current_segment(&mut self, coords: &mut [f64; 6]) -> i16;
    fn is_done(&self) -> bool;
    fn next(&mut self);
}

/// `PathIteratorWrapper` over a `java.awt.geom.PathIterator`'s segments (`SEG_*` codes and their coordinates).
pub struct PathIteratorWrapper {
    pub segments: Vec<(i32, [f64; 6])>,
    pub index: usize,
}

impl PathIteratorWrapper {
    pub fn new(segments: Vec<(i32, [f64; 6])>) -> PathIteratorWrapper {
        PathIteratorWrapper { segments, index: 0 }
    }
}

impl ShapeIterator for PathIteratorWrapper {
    fn current_segment(&mut self, coords: &mut [f64; 6]) -> i16 {
        let (code, c) = self.segments[self.index];
        let n = match code {
            0 | 1 => 2,
            2 => 4,
            3 => 6,
            _ => 0,
        };
        coords[..n].copy_from_slice(&c[..n]);
        code as i16
    }

    fn is_done(&self) -> bool {
        self.index >= self.segments.len()
    }

    fn next(&mut self) {
        self.index += 1;
    }
}

/// `flash.swf.builder.types.Point`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

pub struct ShapeBuilder {
    convert_to_twips: bool,
    shape: Shape,
    pen: Point,
    last_move_to: Point,
    start: Point,
    dx_sum_twips: i32,
    dy_sum_twips: i32,
    line_style: i32,
    fill_style0: i32,
    fill_style1: i32,
    use_fill_style1: bool,
    use_fill_style0: bool,
    line_style_has_changed: bool,
    fill_style1_has_changed: bool,
    fill_style0_has_changed: bool,
    closed: bool,
}

impl ShapeBuilder {
    pub fn new(use_twips: bool) -> ShapeBuilder {
        let origin = Point::default();
        ShapeBuilder {
            convert_to_twips: use_twips,
            shape: Shape { shape_records: Some(Vec::new()) },
            pen: origin,
            last_move_to: origin,
            start: origin,
            dx_sum_twips: 0,
            dy_sum_twips: 0,
            line_style: -1,
            fill_style0: -1,
            fill_style1: -1,
            use_fill_style1: true,
            use_fill_style0: true,
            line_style_has_changed: false,
            fill_style1_has_changed: false,
            fill_style0_has_changed: false,
            closed: false,
        }
    }

    /// `new ShapeBuilder(origin)`.
    pub fn with_origin(origin: Point) -> ShapeBuilder {
        ShapeBuilder { start: origin, last_move_to: origin, pen: origin, ..ShapeBuilder::new(true) }
    }

    pub fn build(self) -> Shape {
        self.shape
    }

    fn records(&mut self) -> &mut Vec<ShapeRecord> {
        self.shape.shape_records.as_mut().unwrap()
    }

    pub fn process_shape(&mut self, si: &mut dyn ShapeIterator) {
        while !si.is_done() {
            let mut coords = [0.0f64; 6];
            let code = si.current_segment(&mut coords);
            match code {
                MOVE_TO => {
                    self.correct_rounding_errors();
                    self.move_to(coords[0], coords[1]);
                    self.closed = false;
                }
                LINE_TO => self.straight(coords[0], coords[1]),
                QUAD_TO => self.curved(coords[0], coords[1], coords[2], coords[3]),
                CUBIC_TO => self.approximate_cubic_bezier(self.pen, Point::new(coords[0], coords[1]), Point::new(coords[2], coords[3]), Point::new(coords[4], coords[5])),
                CLOSE => {
                    self.closed = true;
                    self.close();
                }
                _ => {}
            }
            si.next();
        }
        self.correct_rounding_errors();
    }

    pub fn correct_rounding_errors(&mut self) {
        if (self.dx_sum_twips != 0 || self.dy_sum_twips != 0) && (self.closed || self.fill_style0 > 0 || self.fill_style1 > 0) {
            self.add_line_subdivide_aware(self.dx_sum_twips.wrapping_neg(), self.dy_sum_twips.wrapping_neg());
            self.dx_sum_twips = 0;
            self.dy_sum_twips = 0;
        }
    }

    /// `move(x, y)`.
    pub fn move_to(&mut self, x: f64, y: f64) {
        let mut dx = x - self.start.x;
        let mut dy = y - self.start.y;
        if self.convert_to_twips {
            dx *= TWIPS_PER_PIXEL as f64;
            dy *= TWIPS_PER_PIXEL as f64;
        }
        let mut scr = StyleChangeRecord::default();
        scr.set_move(dx.round_ties_even() as i32, dy.round_ties_even() as i32);
        self.dx_sum_twips = 0;
        self.dy_sum_twips = 0;
        if self.line_style_has_changed {
            scr.set_linestyle(self.line_style);
            self.line_style_has_changed = false;
        }
        if self.fill_style0_has_changed && self.use_fill_style0 {
            scr.set_fill_style0(self.fill_style0);
            self.fill_style0_has_changed = false;
        }
        if self.fill_style1_has_changed && self.use_fill_style1 {
            scr.set_fill_style1(self.fill_style1);
            self.fill_style1_has_changed = false;
        }
        self.last_move_to = Point::new(x, y);
        self.pen = Point::new(x, y);
        self.records().push(ShapeRecord::StyleChange(scr));
    }

    pub fn straight(&mut self, x: f64, y: f64) {
        let mut dx = x - self.pen.x;
        let mut dy = y - self.pen.y;
        if self.convert_to_twips {
            dx *= TWIPS_PER_PIXEL as f64;
            dy *= TWIPS_PER_PIXEL as f64;
        }
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let intdx = dx.round_ties_even() as i32;
        let intdy = dy.round_ties_even() as i32;
        self.add_line_subdivide_aware(intdx, intdy);
        self.pen = Point::new(x, y);
        self.dx_sum_twips = self.dx_sum_twips.wrapping_add(intdx);
        self.dy_sum_twips = self.dy_sum_twips.wrapping_add(intdy);
    }

    pub fn curved(&mut self, cx: f64, cy: f64, ax: f64, ay: f64) {
        let points = [self.pen.x, self.pen.y, cx, cy, ax, ay];
        let deltas = self.add_curve_subdivide_aware(&points);
        self.pen = Point::new(ax, ay);
        self.dx_sum_twips = self.dx_sum_twips.wrapping_add(deltas[2].wrapping_add(deltas[0]));
        self.dy_sum_twips = self.dy_sum_twips.wrapping_add(deltas[3].wrapping_add(deltas[1]));
    }

    pub fn close(&mut self) {
        let mut dx = self.last_move_to.x - self.pen.x;
        let mut dy = self.last_move_to.y - self.pen.y;
        if self.convert_to_twips {
            dx *= TWIPS_PER_PIXEL as f64;
            dy *= TWIPS_PER_PIXEL as f64;
        }
        self.pen = self.last_move_to;
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let intdx = dx.round_ties_even() as i32;
        let intdy = dy.round_ties_even() as i32;
        self.add_line_subdivide_aware(intdx, intdy);
        self.dx_sum_twips = self.dx_sum_twips.wrapping_add(intdx);
        self.dy_sum_twips = self.dy_sum_twips.wrapping_add(intdy);
    }

    fn add_line_subdivide_aware(&mut self, x: i32, y: i32) {
        let limit = MAX_DELTA_IN_TWIPS;
        if x.wrapping_abs() > limit || y.wrapping_abs() > limit {
            let mid_x_left = (x as f64 / 2.0).floor().round_ties_even() as i32;
            let mid_y_left = (y as f64 / 2.0).floor().round_ties_even() as i32;
            let mid_x_right = (x as f64 / 2.0).ceil().round_ties_even() as i32;
            let mid_y_right = (y as f64 / 2.0).ceil().round_ties_even() as i32;
            if mid_x_left.wrapping_abs() > limit || mid_y_left.wrapping_abs() > limit {
                self.add_line_subdivide_aware(mid_x_left, mid_y_left);
            } else {
                self.records().push(ShapeRecord::straight(mid_x_left, mid_y_left));
            }
            if mid_x_right.wrapping_abs() > limit || mid_y_right.wrapping_abs() > limit {
                self.add_line_subdivide_aware(mid_x_right, mid_y_right);
            } else {
                self.records().push(ShapeRecord::straight(mid_x_right, mid_y_right));
            }
        } else {
            self.records().push(ShapeRecord::straight(x, y));
        }
    }

    fn add_curve_subdivide_aware(&mut self, curve: &[f64; 6]) -> [i32; 4] {
        let delta = self.curve_deltas(curve);
        if exceeds_edge_record_limit(&delta) {
            let mut left = [0.0; 6];
            let mut right = [0.0; 6];
            divide_quad(curve, &mut left, &mut right);
            let delta_left = self.curve_deltas(&left);
            let delta_right = self.curve_deltas(&right);
            if exceeds_edge_record_limit(&delta_left) {
                self.add_curve_subdivide_aware(&left);
            } else {
                self.curve_record(delta_left);
            }
            if exceeds_edge_record_limit(&delta_right) {
                self.add_curve_subdivide_aware(&right);
            } else {
                self.curve_record(delta_right);
            }
        } else {
            self.curve_record(delta);
        }
        delta
    }

    fn curve_record(&mut self, d: [i32; 4]) {
        self.records().push(ShapeRecord::CurvedEdge { control_delta_x: d[0], control_delta_y: d[1], anchor_delta_x: d[2], anchor_delta_y: d[3] });
    }

    fn curve_deltas(&self, curve: &[f64; 6]) -> [i32; 4] {
        let mut dcx = curve[2] - curve[0];
        let mut dcy = curve[3] - curve[1];
        let mut dax = curve[4] - curve[2];
        let mut day = curve[5] - curve[3];
        if self.convert_to_twips {
            let t = TWIPS_PER_PIXEL as f64;
            dcx *= t;
            dcy *= t;
            dax *= t;
            day *= t;
        }
        [dcx.round_ties_even() as i32, dcy.round_ties_even() as i32, dax.round_ties_even() as i32, day.round_ties_even() as i32]
    }

    pub fn set_current_line_style(&mut self, index: i32) {
        if index != self.line_style {
            self.line_style_has_changed = true;
            self.line_style = index;
        }
    }

    pub fn set_current_fill_style0(&mut self, index: i32) {
        if index != self.fill_style0 {
            self.fill_style0_has_changed = true;
            self.fill_style0 = index;
        }
    }

    pub fn set_current_fill_style1(&mut self, index: i32) {
        if index != self.fill_style1 {
            self.fill_style1_has_changed = true;
            self.fill_style1 = index;
        }
    }

    pub fn set_use_fill_style1(&mut self, b: bool) {
        self.use_fill_style1 = b;
    }

    pub fn set_use_fill_style0(&mut self, b: bool) {
        self.use_fill_style0 = b;
    }

    fn approximate_cubic_bezier(&mut self, p0: Point, p1: Point, p2: Point, p3: Point) {
        let pa = get_point_on_segment(p0, p1, 3.0 / 4.0);
        let pb = get_point_on_segment(p3, p2, 3.0 / 4.0);
        let dx = (p3.x - p0.x) / 16.0;
        let dy = (p3.y - p0.y) / 16.0;
        let c1 = get_point_on_segment(p0, p1, 3.0 / 8.0);
        let mut c2 = get_point_on_segment(pa, pb, 3.0 / 8.0);
        c2.x -= dx;
        c2.y -= dy;
        let mut c3 = get_point_on_segment(pb, pa, 3.0 / 8.0);
        c3.x += dx;
        c3.y += dy;
        let c4 = get_point_on_segment(p3, p2, 3.0 / 8.0);
        let a1 = Point::new((c1.x + c2.x) / 2.0, (c1.y + c2.y) / 2.0);
        let a2 = Point::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0);
        let a3 = Point::new((c3.x + c4.x) / 2.0, (c3.y + c4.y) / 2.0);
        self.curved(c1.x, c1.y, a1.x, a1.y);
        self.curved(c2.x, c2.y, a2.x, a2.y);
        self.curved(c3.x, c3.y, a3.x, a3.y);
        self.curved(c4.x, c4.y, p3.x, p3.y);
    }
}

pub fn get_point_on_segment(p0: Point, p1: Point, ratio: f64) -> Point {
    Point::new(p0.x + (p1.x - p0.x) * ratio, p0.y + (p1.y - p0.y) * ratio)
}

fn exceeds_edge_record_limit(values: &[i32; 4]) -> bool {
    values.iter().any(|v| v.wrapping_abs() > MAX_DELTA_IN_TWIPS)
}

/// `divideQuad(src, 0, left, 0, right, 0)`.
pub fn divide_quad(src: &[f64; 6], left: &mut [f64; 6], right: &mut [f64; 6]) {
    let (mut x1, mut y1, mut ctrlx, mut ctrly, mut x2, mut y2) = (src[0], src[1], src[2], src[3], src[4], src[5]);
    left[0] = x1;
    left[1] = y1;
    right[4] = x2;
    right[5] = y2;
    x1 = (x1 + ctrlx) / 2.0;
    y1 = (y1 + ctrly) / 2.0;
    x2 = (x2 + ctrlx) / 2.0;
    y2 = (y2 + ctrly) / 2.0;
    ctrlx = (x1 + x2) / 2.0;
    ctrly = (y1 + y2) / 2.0;
    left[2] = x1;
    left[3] = y1;
    left[4] = ctrlx;
    left[5] = ctrly;
    right[0] = ctrlx;
    right[1] = ctrly;
    right[2] = x2;
    right[3] = y2;
}
