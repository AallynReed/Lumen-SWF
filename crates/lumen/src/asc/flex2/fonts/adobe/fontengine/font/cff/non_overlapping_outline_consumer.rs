//! Port of afe.jar `com.adobe.fontengine.font.cff.NonOverlappingOutlineConsumer`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;

const MAX_NUM_SUBPATHS: usize = 1000;
const MAX_NUM_SEGMENTS: i32 = 1000;
const SEG_DELETE: i32 = 1;
const SEG_ISECT: i32 = 2;
const SEG_WIND_TEST: i32 = 4;
const NIL: usize = usize::MAX;

/// `Math.round(float)` (JDK 17).
pub(crate) fn java_round_f32(a: f32) -> i32 {
    let int_bits = a.to_bits() as i32;
    let biased_exp = (int_bits & 0x7F80_0000) >> 23;
    let shift = (24 - 2 + 127) - biased_exp;
    if (shift & -32) == 0 {
        let mut r = (int_bits & 0x007F_FFFF) | (0x007F_FFFF + 1);
        if int_bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        a as i32
    }
}

/// `Math.round(double)` (JDK 17).
pub(crate) fn java_round_f64(a: f64) -> i64 {
    let long_bits = a.to_bits() as i64;
    let biased_exp = (long_bits & 0x7FF0_0000_0000_0000) >> 52;
    let shift = (53 - 2 + 1023) - biased_exp;
    if (shift & -64) == 0 {
        let mut r = (long_bits & 0x000F_FFFF_FFFF_FFFF) | (0x000F_FFFF_FFFF_FFFF + 1);
        if long_bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        a as i64
    }
}

/// `Math2.epsilonEquals(float, float)`.
fn epsilon_equals(f: f32, f2: f32) -> bool {
    (f - f2).abs() < 1.0E-7f32
}

fn jmax_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.to_bits() == (-0.0f32).to_bits() {
        return b;
    }
    if a >= b { a } else { b }
}

fn jmin_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.to_bits() == (-0.0f32).to_bits() {
        return b;
    }
    if a <= b { a } else { b }
}

fn milliem(f: f32, f2: f32) -> f32 {
    ((f * f2) as f64 / 1000.0) as f32
}

#[derive(Debug)]
struct ISectException(#[allow(dead_code)] &'static str);

type IResult<T> = Result<T, ISectException>;

pub struct NonOverlappingOutlineConsumer {
    cpx: f32,
    cpy: f32,
    current_sub_path: usize,
    path: Vec<usize>,
    new_path: Option<Vec<usize>>,
    saved_path: Option<Vec<usize>>,
    isect_list: Vec<usize>,
    /// `segList`; a segment's index here is its `segList.indexOf`.
    seg_list: Vec<Segment>,
    units_per_em: f32,
    self_isect_flag: bool,
    oc: Rc<RefCell<dyn OutlineConsumer>>,
    debug_num_glyphs_with_overlaps: i32,
    debug_num_failures: i32,
    sub_paths: Vec<SubPath>,
    isects: Vec<Intersect>,
    /// Curves and lines made while intersecting, which never join a subpath.
    tmp: Vec<Segment>,
}

impl NonOverlappingOutlineConsumer {
    pub fn new(consumer: Rc<RefCell<dyn OutlineConsumer>>, units_per_em: f64) -> NonOverlappingOutlineConsumer {
        NonOverlappingOutlineConsumer {
            cpx: 0.0,
            cpy: 0.0,
            current_sub_path: NIL,
            path: Vec::new(),
            new_path: None,
            saved_path: None,
            isect_list: Vec::new(),
            seg_list: Vec::new(),
            units_per_em: units_per_em as f32,
            self_isect_flag: false,
            oc: consumer,
            debug_num_glyphs_with_overlaps: 0,
            debug_num_failures: 0,
            sub_paths: Vec::new(),
            isects: Vec::new(),
            tmp: Vec::new(),
        }
    }

    fn save_isect(&mut self, point: Point, segment: usize, f: f32, l: i64) {
        let point2 = Point::new(java_round_f32(point.x) as f32, java_round_f32(point.y) as f32);
        if !self.self_isect_flag {
            for &i in &self.isect_list {
                let intersect = &self.isects[i];
                let sub_path = self.seg_list[intersect.seg].sub_path;
                if sub_path != self.seg_list[segment].sub_path
                    || !epsilon_equals(java_round_f32(intersect.p.x) as f32, point2.x)
                    || !epsilon_equals(java_round_f32(intersect.p.y) as f32, point2.y)
                {
                    continue;
                }
                return;
            }
        }
        self.isects.push(Intersect { t: f, p: point, seg: segment, split_seg: NIL, id: l });
        self.isect_list.push(self.isects.len() - 1);
        let sp = self.seg_list[segment].sub_path;
        self.sub_paths[sp].path_i_sected = true;
    }

    fn save_isect_pair(&mut self, segment: usize, mut f: f32, segment2: usize, mut f2: f32) {
        let l5 = self.isect_list.len() as i64;
        let s = &self.seg_list[segment];
        let s2 = &self.seg_list[segment2];
        let point = if f == 0.0 {
            s.get_first_point()
        } else if f2 == 0.0 {
            s2.get_first_point()
        } else if f == 1.0 {
            s.get_last_point()
        } else if f2 == 1.0 {
            s2.get_last_point()
        } else if s.line {
            s.compute_isect_point(f)
        } else if s2.line {
            s2.compute_isect_point(f2)
        } else {
            s.compute_isect_point(f)
        };
        let l6 = java_round_f32(point.x) as i64;
        let l7 = java_round_f32(point.y) as i64;
        if 0.0 < f && f < 1.0 {
            let l4 = java_round_f32(s.get_first_point().x) as i64;
            let l3 = java_round_f32(s.get_first_point().y) as i64;
            let l2 = java_round_f32(s.get_last_point().x) as i64;
            let l = java_round_f32(s.get_last_point().y) as i64;
            if l6 == l4 && l7 == l3 || l6 == l2 && l7 == l {
                f = java_round_f32(f) as f32;
            }
        }
        if 0.0 < f2 && f2 < 1.0 {
            let l4 = java_round_f32(s2.get_first_point().x) as i64;
            let l3 = java_round_f32(s2.get_first_point().y) as i64;
            let l2 = java_round_f32(s2.get_last_point().x) as i64;
            let l = java_round_f32(s2.get_last_point().y) as i64;
            if l6 == l4 && l7 == l3 || l6 == l2 && l7 == l {
                f2 = java_round_f32(f2) as f32;
            }
        }
        self.save_isect(point, segment, f, l5);
        self.save_isect(point, segment2, f2, l5 + 1);
    }

    fn isect_line_line_segs(&mut self, segment: usize, segment2: usize) {
        if !self.seg_list[segment].line || !self.seg_list[segment2].line {
            return;
        }
        let list = self.seg_list[segment].i_sect_line(&self.seg_list[segment2]);
        if list.len() == 2 {
            let a = list[1];
            self.save_isect_pair(segment, a[0], segment2, a[1]);
            let a = list[0];
            self.save_isect_pair(segment, a[0], segment2, a[1]);
        } else if list.len() == 1 {
            let a = list[0];
            self.save_isect_pair(segment, a[0], segment2, a[1]);
        }
    }

    fn tmp_push(&mut self, s: Segment) -> usize {
        self.tmp.push(s);
        self.tmp.len() - 1
    }

    /// `isectLineCurveSegs`; `line` and `curve` index `tmp`.
    fn isect_line_curve_segs(&mut self, line: usize, segment2: usize) {
        if !self.tmp[line].line || self.tmp[segment2].line {
            return;
        }
        let mut curve = segment2;
        loop {
            let upem = self.units_per_em;
            if self.tmp[curve].is_flat_enough(upem) {
                let l2 = self.tmp[curve].make_line();
                let list = l2.i_sect_line(&self.tmp[line]);
                let (c_t0, c_t1) = (self.tmp[curve].t0, self.tmp[curve].t1);
                let (l_t0, l_t1) = (self.tmp[line].t0, self.tmp[line].t1);
                let o2 = self.tmp[segment2].orig_seg;
                let o1 = self.tmp[line].orig_seg;
                if list.len() == 2 {
                    let a = list[1];
                    self.save_isect_pair(o2, c_t0 + a[0] * (c_t1 - c_t0), o1, l_t0 + a[1] * (l_t1 - l_t0));
                    let a = list[0];
                    self.save_isect_pair(o2, c_t0 + a[0] * (c_t1 - c_t0), o1, l_t0 + a[1] * (l_t1 - l_t0));
                    return;
                }
                if list.len() == 1 {
                    let a = list[0];
                    self.save_isect_pair(o2, c_t0 + a[0] * (c_t1 - c_t0), o1, l_t0 + a[1] * (l_t1 - l_t0));
                    return;
                }
                return;
            }
            let b = self.tmp[curve].bounds;
            if !self.tmp[line].i_sect_rect(&b) {
                break;
            }
            let (h0, h1) = self.tmp[curve].split_mid();
            let h0 = self.tmp_push(h0);
            curve = self.tmp_push(h1);
            self.isect_line_curve_segs(line, h0);
        }
    }

    /// `isectCurveCurve`; both index `tmp`.
    fn isect_curve_curve(&mut self, curve: usize, curve2: usize) {
        if !self.tmp[curve].bounds.overlap(&self.tmp[curve2].bounds) {
            return;
        }
        let upem = self.units_per_em;
        if self.tmp[curve].is_flat_enough(upem) {
            let line = self.tmp[curve].make_line();
            let line = self.tmp_push(line);
            self.isect_line_curve_segs(line, curve2);
        } else {
            let (c3, c) = self.tmp[curve].split_mid();
            let c3 = self.tmp_push(c3);
            let c = self.tmp_push(c);
            self.isect_curve_curve(curve2, c3);
            self.isect_curve_curve(curve2, c);
        }
    }

    fn isect_curve_curve_segs(&mut self, segment: usize, segment2: usize) {
        if self.seg_list[segment].line || self.seg_list[segment2].line {
            return;
        }
        if self.seg_list[segment].equal_control_points(&self.seg_list[segment2]) {
            self.save_isect_pair(segment, 0.0, segment2, 0.0);
            self.save_isect_pair(segment, 1.0, segment2, 1.0);
        } else if self.seg_list[segment].reverse_control_points(&self.seg_list[segment2]) {
            self.save_isect_pair(segment, 0.0, segment2, 1.0);
            self.save_isect_pair(segment, 1.0, segment2, 0.0);
        } else {
            let a = self.seg_list[segment].clone();
            let b = self.seg_list[segment2].clone();
            let a = self.tmp_push(a);
            let b = self.tmp_push(b);
            self.isect_curve_curve(a, b);
        }
    }

    fn isect_seg_pair(&mut self, segment: usize, segment2: usize) {
        self.seg_list[segment].orig_seg = segment;
        self.seg_list[segment2].orig_seg = segment2;
        self.tmp.clear();
        if self.seg_list[segment].line {
            if self.seg_list[segment2].line {
                self.isect_line_line_segs(segment, segment2);
            } else {
                let a = self.seg_list[segment].clone();
                let b = self.seg_list[segment2].clone();
                let a = self.tmp_push(a);
                let b = self.tmp_push(b);
                self.isect_line_curve_segs(a, b);
            }
        } else if self.seg_list[segment2].line {
            let a = self.seg_list[segment2].clone();
            let b = self.seg_list[segment].clone();
            let a = self.tmp_push(a);
            let b = self.tmp_push(b);
            self.isect_line_curve_segs(a, b);
        } else {
            self.isect_curve_curve_segs(segment, segment2);
        }
        self.tmp.clear();
    }

    fn isect_path_pair(&mut self, sub_path: usize, sub_path2: usize) {
        let mut segment = self.sub_paths[sub_path].first_segment;
        if segment == NIL {
            return;
        }
        loop {
            let mut segment2 = self.sub_paths[sub_path2].first_segment;
            if segment2 == NIL {
                return;
            }
            loop {
                if self.seg_list[segment].bounds.overlap(&self.seg_list[segment2].bounds) {
                    self.isect_seg_pair(segment, segment2);
                }
                segment2 = self.seg_list[segment2].next_segment;
                if segment2 == self.sub_paths[sub_path2].first_segment {
                    break;
                }
            }
            segment = self.seg_list[segment].next_segment;
            if segment == self.sub_paths[sub_path].first_segment {
                break;
            }
        }
    }

    fn split_segment(&mut self, intersect: usize, intersect2: usize) {
        let mut f = self.isects[intersect2].t;
        let mut segment2 = self.isects[intersect2].seg;
        if intersect != NIL && self.isects[intersect].seg == self.isects[intersect2].seg {
            if f == self.isects[intersect].t || f == 1.0 {
                self.isects[intersect2].split_seg = self.isects[intersect].split_seg;
                return;
            }
            if self.isects[intersect].split_seg != NIL {
                let t = self.isects[intersect].t;
                f = (f - t) / (1.0 - t);
                segment2 = self.isects[intersect].split_seg;
            }
        } else if f == 0.0 || f == 1.0 {
            return;
        }
        let ip = self.isects[intersect2].p;
        let segment;
        if self.seg_list[segment2].line {
            let line = Segment::new_line_pts(ip, self.seg_list[segment2].p1);
            self.seg_list[segment2].p1.x = ip.x;
            self.seg_list[segment2].p1.y = ip.y;
            segment = line;
        } else {
            let upem = self.units_per_em;
            let c = &mut self.seg_list[segment2];
            let point = c.p0;
            let point2 = c.p1;
            let point3 = c.p2;
            let point4 = c.p3;
            c.p1.x = java_round_f32(f * (point2.x - point.x) + c.p0.x) as f32;
            c.p1.y = java_round_f32(f * (point2.y - point.y) + c.p0.y) as f32;
            c.p2.x = java_round_f32(f * f * (point3.x - 2.0 * point2.x + point.x) + 2.0 * c.p1.x - c.p0.x) as f32;
            c.p2.y = java_round_f32(f * f * (point3.y - 2.0 * point2.y + point.y) + 2.0 * c.p1.y - c.p0.y) as f32;
            c.p3.x = java_round_f32(ip.x) as f32;
            c.p3.y = java_round_f32(ip.y) as f32;
            if c.is_line(upem) {
                let object2 = c.make_line();
                let sp = self.seg_list[segment2].sub_path;
                self.sub_path_replace_segment(sp, segment2, object2);
            }
            f = 1.0 - f;
            let object2 = point4;
            let point5 = Point::new(
                java_round_f32(f * (point3.x - point4.x) + object2.x) as f32,
                java_round_f32(f * (point3.y - point4.y) + object2.y) as f32,
            );
            let point6 = Point::new(
                java_round_f32(f * f * (point2.x - 2.0 * point3.x + point4.x) + 2.0 * point5.x - object2.x) as f32,
                java_round_f32(f * f * (point2.y - 2.0 * point3.y + point4.y) + 2.0 * point5.y - object2.y) as f32,
            );
            let point7 = self.seg_list[segment2].p3;
            let curve = Segment::new_curve_pts(point7, point6, point5, object2);
            if curve.is_line(upem) {
                segment = curve.make_line();
            } else {
                segment = curve;
            }
        }
        let sp = self.seg_list[segment2].sub_path;
        let new_seg = self.sub_path_split_segment(sp, segment2, segment);
        self.isects[intersect2].split_seg = new_seg;
    }

    fn split_isect_segs(&mut self) {
        let mut list = std::mem::take(&mut self.isect_list);
        stable_sort(&mut list, |a, b| self.isects[*a].cmp_segs(&self.isects[*b]));
        self.isect_list = list;
        let mut object2 = NIL;
        for k in 0..self.isect_list.len() {
            let i = self.isect_list[k];
            self.split_segment(object2, i);
            object2 = i;
        }
        let arr = self.isect_list.clone();
        let mut i = arr.len() as isize - 1;
        while i > 0 {
            let o2 = arr[(i - 1) as usize];
            let o = arr[i as usize];
            i -= 1;
            if self.isects[o].split_seg == NIL || o2 == NIL || self.isects[o2].seg != self.isects[o].seg || self.isects[o2].split_seg == NIL {
                continue;
            }
            self.isects[o].seg = self.isects[o2].split_seg;
            let s = self.isects[o].seg;
            self.isects[o].split_seg = self.seg_list[s].next_segment;
        }
        let upem = self.units_per_em;
        for k in 0..self.isect_list.len() {
            let it = self.isect_list[k];
            let object = self.isects[it].seg;
            if self.isects[it].split_seg == NIL {
                self.isects[it].split_seg = if self.isects[it].t == 0.0 { self.seg_list[object].prev_segment } else { self.seg_list[object].next_segment };
            } else {
                self.seg_list[object].round(upem);
                let ss = self.isects[it].split_seg;
                self.seg_list[ss].round(upem);
            }
            self.seg_list[object].flags |= SEG_ISECT;
            let ss = self.isects[it].split_seg;
            self.seg_list[ss].flags |= SEG_ISECT;
        }
    }

    fn path_index_of(&self, sp: usize) -> isize {
        self.path.iter().position(|&p| p == sp).map(|p| p as isize).unwrap_or(-1)
    }

    fn wind_test_seg(&mut self, segment: usize) {
        if (self.seg_list[segment].flags & (SEG_DELETE | SEG_WIND_TEST)) != 0 {
            return;
        }
        let mut point = Point::default();
        {
            let s = &self.seg_list[segment];
            if s.line {
                point.x = (s.p0.x + s.p1.x) / 2.0;
                point.y = (s.p0.y + s.p1.y) / 2.0;
            } else {
                point.x = (s.p3.x + 3.0 * (s.p2.x + s.p1.x) + s.p0.x) / 8.0;
                point.y = (s.p3.y + 3.0 * (s.p2.y + s.p1.y) + s.p0.y) / 8.0;
            }
        }
        let mut n: i32 = 0;
        let bl = self.seg_list[segment].horizontal();
        let n2 = if bl { self.seg_list[segment].get_winding_x() } else { self.seg_list[segment].get_winding_y() };
        let mut sub_path = self.seg_list[segment].sub_path;
        loop {
            let sb = self.sub_paths[sub_path].bounds;
            let skip = if bl {
                sb.top < point.y || sb.left > point.x || sb.right <= point.x
            } else {
                sb.left > point.x || sb.bottom > point.y || sb.top <= point.y
            };
            if !skip {
                let first = self.sub_paths[sub_path].first_segment;
                let mut segment3 = first;
                loop {
                    if segment3 != segment {
                        let rect = self.seg_list[segment3].bounds;
                        if bl {
                            if !(rect.top < point.y || rect.left > point.x || rect.right <= point.x || rect.left == rect.right) {
                                if rect.bottom > point.y {
                                    let s3 = &self.seg_list[segment3];
                                    n = n.wrapping_add(Segment::get_winding_at_value(s3.get_first_point().x, s3.get_last_point().x, point.x));
                                } else {
                                    if self.seg_list[segment3].reverse_control_points(&self.seg_list[segment]) {
                                        self.seg_list[segment].flags |= SEG_DELETE | SEG_WIND_TEST;
                                        return;
                                    }
                                    if self.seg_list[segment3].equal_control_points(&self.seg_list[segment]) {
                                        if self.path_index_of(self.seg_list[segment3].sub_path) < self.path_index_of(self.seg_list[segment].sub_path) {
                                            self.seg_list[segment].flags |= SEG_DELETE;
                                        }
                                        self.seg_list[segment].flags |= SEG_WIND_TEST;
                                        return;
                                    }
                                    for ic in self.seg_list[segment3].solve_at_x(point.x) {
                                        if !(ic.ic > point.y) {
                                            continue;
                                        }
                                        n = n.wrapping_add(ic.winding);
                                    }
                                }
                            }
                        } else if !(rect.left > point.x || rect.bottom > point.y || rect.top <= point.y || rect.top == rect.bottom) {
                            if rect.right < point.x {
                                let s3 = &self.seg_list[segment3];
                                n = n.wrapping_add(Segment::get_winding_at_value(s3.get_first_point().y, s3.get_last_point().y, point.y));
                            } else {
                                if self.seg_list[segment3].reverse_control_points(&self.seg_list[segment]) {
                                    self.seg_list[segment].flags |= SEG_DELETE | SEG_WIND_TEST;
                                    return;
                                }
                                if self.seg_list[segment3].equal_control_points(&self.seg_list[segment]) {
                                    if self.path_index_of(self.seg_list[segment3].sub_path) < self.path_index_of(self.seg_list[segment].sub_path) {
                                        self.seg_list[segment].flags |= SEG_DELETE;
                                    }
                                    self.seg_list[segment].flags |= SEG_WIND_TEST;
                                    return;
                                }
                                for ic in self.seg_list[segment3].solve_at_y(point.y) {
                                    if !(ic.ic < point.x) {
                                        continue;
                                    }
                                    n = n.wrapping_add(ic.winding);
                                }
                            }
                        }
                    }
                    segment3 = self.seg_list[segment3].next_segment;
                    if segment3 == self.sub_paths[sub_path].first_segment {
                        break;
                    }
                }
            }
            sub_path = self.sub_paths[sub_path].next_sub_path;
            if sub_path == self.seg_list[segment].sub_path {
                break;
            }
        }
        if n != 0 && n.wrapping_add(n2) != 0 {
            self.seg_list[segment].flags |= SEG_DELETE;
        }
        self.seg_list[segment].flags |= SEG_WIND_TEST;
    }

    fn relink(&mut self, a: usize, b: usize) {
        self.seg_list[a].next_segment = b;
        self.seg_list[b].prev_segment = a;
    }

    fn delete_bad_segs(&mut self) -> IResult<()> {
        if self.isect_list.len() % 2 != 0 {
            return Err(ISectException("error - uneven intersects"));
        }
        let mut list = std::mem::take(&mut self.isect_list);
        stable_sort(&mut list, |a, b| self.isects[*a].cmp_ids(&self.isects[*b]));
        self.isect_list = list;
        for k in 0..self.isect_list.len() {
            let i = self.isect_list[k];
            let s = self.isects[i].seg;
            self.wind_test_seg(s);
            let s = self.isects[i].split_seg;
            self.wind_test_seg(s);
        }
        let mut k = 0;
        while k < self.isect_list.len() {
            let intersect = self.isect_list[k];
            let intersect2 = self.isect_list[k + 1];
            k += 2;
            let (object, segment3) = {
                let it = &self.isects[intersect];
                if self.seg_list[it.seg].next_segment == it.split_seg { (it.seg, it.split_seg) } else { (it.split_seg, it.seg) }
            };
            let (segment2, segment) = {
                let it = &self.isects[intersect2];
                if self.seg_list[it.seg].next_segment == it.split_seg { (it.seg, it.split_seg) } else { (it.split_seg, it.seg) }
            };
            let fl = |s: usize| self.seg_list[s].flags & 1;
            let sw = fl(object) << 3 | fl(segment3) << 2 | fl(segment2) << 1 | fl(segment);
            match sw {
                0 | 3 | 12 | 15 => {}
                5 | 10 => return Err(ISectException("error - illegal segment flags")),
                1 => {
                    self.seg_list[object].flags |= 1;
                    self.relink(segment2, segment3);
                }
                2 => {
                    self.seg_list[segment3].flags |= 1;
                    self.relink(object, segment);
                }
                4 => {
                    self.seg_list[segment2].flags |= 1;
                    self.relink(object, segment);
                }
                6 => self.relink(object, segment),
                7 => {
                    if self.seg_list[object].get_last_point().equals(&self.seg_list[segment].get_first_point()) {
                        self.seg_list[segment].flags &= !1;
                        self.relink(object, segment);
                    }
                }
                8 => {
                    self.seg_list[segment].flags |= 1;
                    self.relink(segment2, segment3);
                }
                9 => self.relink(segment2, segment3),
                11 => {
                    if self.seg_list[segment2].get_last_point().equals(&self.seg_list[segment3].get_first_point()) {
                        self.seg_list[segment2].flags &= !1;
                        self.relink(segment2, segment3);
                    }
                }
                13 => {
                    if self.seg_list[segment2].get_last_point().equals(&self.seg_list[segment3].get_first_point()) {
                        self.seg_list[segment3].flags &= !1;
                        self.relink(segment2, segment3);
                    }
                }
                14 => {
                    if self.seg_list[object].get_last_point().equals(&self.seg_list[segment].get_first_point()) {
                        self.seg_list[object].flags &= !1;
                        self.relink(object, segment);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn new_sub_path(&mut self, segment: usize) -> IResult<usize> {
        let sp = self.alloc_sub_path(SubPath::new());
        self.sub_path_add_segments(sp, segment)?;
        Ok(sp)
    }

    fn build_sub_path(&mut self, segment: usize, new_path: &mut Vec<usize>) -> IResult<()> {
        if (self.seg_list[segment].flags & 1) != 0 || new_path.contains(&self.seg_list[segment].sub_path) {
            return Ok(());
        }
        let sp = self.new_sub_path(segment)?;
        self.add_path(new_path, sp);
        Ok(())
    }

    fn build_new_paths(&mut self) -> IResult<()> {
        let mut new_path = Vec::new();
        self.new_path = Some(Vec::new());
        let r = self.build_new_paths_inner(&mut new_path);
        self.new_path = Some(new_path);
        r
    }

    fn build_new_paths_inner(&mut self, new_path: &mut Vec<usize>) -> IResult<()> {
        for k in 0..self.path.len() {
            let sp = self.path[k];
            if self.sub_paths[sp].path_i_sected {
                continue;
            }
            self.add_path(new_path, sp);
        }
        for k in 0..self.isect_list.len() {
            let i = self.isect_list[k];
            let s = self.isects[i].seg;
            self.build_sub_path(s, new_path)?;
            let s = self.isects[i].split_seg;
            self.build_sub_path(s, new_path)?;
        }
        for k in 0..self.path.len() {
            let object = self.path[k];
            if !self.sub_paths[object].path_i_sected {
                continue;
            }
            let segment = self.sub_paths[object].first_segment;
            let sub_path = self.seg_list[segment].sub_path;
            if !new_path.contains(&sub_path) || (self.seg_list[segment].flags & 1) == 1 {
                continue;
            }
            self.sub_paths[sub_path].first_segment = segment;
            self.sub_paths[sub_path].last_segment = self.seg_list[segment].prev_segment;
        }
        for k in 0..new_path.len() {
            let object = new_path[k];
            let mut n = 0;
            let mut n2 = 0usize;
            let mut segment = self.sub_paths[object].first_segment;
            loop {
                let mut n3 = 0;
                if self.seg_list[segment].line {
                    let (p0, p1) = (self.seg_list[segment].p0, self.seg_list[segment].p1);
                    if epsilon_equals(p0.x, p1.x) {
                        n3 = 1;
                    } else if epsilon_equals(p0.y, p1.y) {
                        n3 = -1;
                    } else {
                        n2 += 1;
                        if n2 > self.seg_list.len() {
                            return Err(ISectException("Infinite loop condition!"));
                        }
                        n = n3;
                        segment = self.seg_list[segment].next_segment;
                        if segment == self.sub_paths[object].first_segment {
                            break;
                        }
                        continue;
                    }
                    if segment != self.sub_paths[object].first_segment && n3 == n {
                        let point = p1;
                        let segment3 = self.seg_list[segment].prev_segment;
                        self.sub_path_remove_segment(object, segment)?;
                        let s3 = &mut self.seg_list[segment3];
                        if s3.line {
                            s3.p1 = point;
                        } else {
                            s3.p3 = point;
                        }
                        s3.set_bounds();
                    }
                    n2 += 1;
                    if n2 > self.seg_list.len() {
                        return Err(ISectException("Infinite loop condition!"));
                    }
                }
                n = n3;
                segment = self.seg_list[segment].next_segment;
                if segment == self.sub_paths[object].first_segment {
                    break;
                }
            }
        }
        Ok(())
    }

    fn self_isect_path(&mut self, sub_path: usize) -> IResult<()> {
        if self.sub_paths[sub_path].num_segments > MAX_NUM_SEGMENTS {
            return Err(ISectException("error - too many segments in subpath"));
        }
        if self.sub_paths[sub_path].num_segments < 3 {
            return Ok(());
        }
        let mut segment = self.sub_paths[sub_path].first_segment;
        while self.seg_list[segment].next_segment != self.sub_paths[sub_path].last_segment {
            let mut segment2 = self.seg_list[segment].next_segment;
            loop {
                segment2 = self.seg_list[segment2].next_segment;
                if !(self.seg_list[segment2].next_segment == segment || !self.seg_list[segment].bounds.overlap(&self.seg_list[segment2].bounds)) {
                    self.isect_seg_pair(segment, segment2);
                }
                if segment2 == self.sub_paths[sub_path].last_segment {
                    break;
                }
            }
            segment = self.seg_list[segment].next_segment;
        }
        Ok(())
    }

    fn isect_path(&mut self) -> IResult<()> {
        let list = self.path.clone();
        if list.len() > MAX_NUM_SUBPATHS {
            return Err(ISectException("error - too many subpaths"));
        }
        self.isect_list = Vec::new();
        self.self_isect_flag = true;
        for &sp in &list {
            self.self_isect_path(sp)?;
        }
        self.self_isect_flag = false;
        for i in 0..list.len() {
            let sub_path = list[i];
            for j in i + 1..list.len() {
                let sub_path2 = list[j];
                if !self.sub_paths[sub_path2].bounds.overlap(&self.sub_paths[sub_path].bounds) {
                    continue;
                }
                self.isect_path_pair(sub_path, sub_path2);
            }
        }
        if !self.isect_list.is_empty() {
            self.saved_path = Some(self.copy_path(&list)?);
            self.split_isect_segs();
            self.delete_bad_segs()?;
            self.build_new_paths()?;
            self.debug_num_glyphs_with_overlaps += 1;
        } else {
            self.new_path = Some(self.path.clone());
        }
        Ok(())
    }

    fn add_path(&mut self, list: &mut Vec<usize>, sub_path: usize) {
        let n = list.len();
        if n > 0 {
            let sub_path2 = list[n - 1];
            self.sub_paths[sub_path].next_sub_path = self.sub_paths[sub_path2].next_sub_path;
            self.sub_paths[sub_path2].next_sub_path = sub_path;
        } else {
            self.sub_paths[sub_path].next_sub_path = sub_path;
        }
        list.push(sub_path);
    }

    fn copy_path(&mut self, list: &[usize]) -> IResult<Vec<usize>> {
        let mut out = Vec::new();
        for &sub_path in list {
            let sp2 = self.alloc_sub_path(SubPath::new());
            self.sub_path_copy_sub_path(sp2, sub_path)?;
            out.push(sp2);
        }
        Ok(out)
    }

    fn generate_outline(&mut self, list: Option<Vec<usize>>) {
        let mut list = match list {
            Some(l) => l,
            None => return,
        };
        if let Some(saved) = &self.saved_path {
            let mut bl = false;
            let n = self.seg_list.len();
            let mut it = 0;
            while it < list.len() && !bl {
                let sub_path = list[it];
                it += 1;
                let mut segment = self.sub_paths[sub_path].first_segment;
                if segment == NIL {
                    return;
                }
                let mut n2 = 0usize;
                loop {
                    n2 += 1;
                    if n2 > n {
                        bl = true;
                    }
                    segment = self.seg_list[segment].next_segment;
                    if segment == self.sub_paths[sub_path].first_segment || bl {
                        break;
                    }
                }
                if bl {
                    list = saved.clone();
                }
            }
        }
        let oc = self.oc.clone();
        for &sub_path in &list {
            let mut segment2 = self.sub_paths[sub_path].first_segment;
            if segment2 == NIL {
                return;
            }
            let mut bl = true;
            loop {
                let s = &self.seg_list[segment2];
                if s.line {
                    if bl {
                        oc.borrow_mut().moveto(s.p0.x as f64, s.p0.y as f64);
                        bl = false;
                    }
                    if segment2 != self.sub_paths[sub_path].last_segment {
                        oc.borrow_mut().lineto(s.p1.x as f64, s.p1.y as f64);
                    }
                } else {
                    if bl {
                        oc.borrow_mut().moveto(s.p0.x as f64, s.p0.y as f64);
                        bl = false;
                    }
                    oc.borrow_mut().curveto(s.p1.x as f64, s.p1.y as f64, s.p2.x as f64, s.p2.y as f64, s.p3.x as f64, s.p3.y as f64);
                }
                segment2 = self.seg_list[segment2].next_segment;
                if segment2 == self.sub_paths[sub_path].first_segment {
                    break;
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.current_sub_path = NIL;
        self.path.clear();
        self.saved_path = None;
        self.new_path = None;
        self.isect_list = Vec::new();
        self.seg_list.clear();
        self.cpx = 0.0;
        self.cpy = 0.0;
        self.sub_paths.clear();
        self.isects.clear();
        self.tmp.clear();
    }

    pub fn get_num_errors(&self) -> i32 {
        self.debug_num_failures
    }

    fn alloc_sub_path(&mut self, sp: SubPath) -> usize {
        self.sub_paths.push(sp);
        self.sub_paths.len() - 1
    }

    fn link(&mut self, this: usize, segment: usize) {
        if segment == this {
            self.seg_list[this].next_segment = this;
            self.seg_list[this].prev_segment = this;
        } else {
            let nx = self.seg_list[this].next_segment;
            self.seg_list[segment].next_segment = nx;
            self.seg_list[nx].prev_segment = segment;
            self.seg_list[segment].prev_segment = this;
            self.seg_list[this].next_segment = segment;
        }
    }

    fn push_seg(&mut self, s: Segment) -> usize {
        self.seg_list.push(s);
        self.seg_list.len() - 1
    }

    /// `SubPath.addSegment`; `added` is the segment's id when it is already in `segList`.
    fn sub_path_add_segment(&mut self, sp: usize, segment: usize) {
        self.seg_list[segment].sub_path = sp;
        let last = self.sub_paths[sp].last_segment;
        if last == NIL {
            self.sub_paths[sp].first_segment = segment;
            self.sub_paths[sp].last_segment = segment;
            self.link(segment, segment);
        } else {
            self.link(last, segment);
            self.sub_paths[sp].last_segment = segment;
        }
        let b = self.seg_list[segment].bounds;
        self.sub_paths[sp].bounds.grow(&b);
        self.sub_paths[sp].num_segments += 1;
    }

    fn sub_path_copy_sub_path(&mut self, this: usize, sub_path: usize) -> IResult<()> {
        let mut segment = self.sub_paths[sub_path].first_segment;
        if self.seg_list[segment].sub_path == this {
            return Ok(());
        }
        let n = self.seg_list.len() as i32;
        loop {
            let segment2 = self.seg_list[segment].copy_segment();
            segment = self.seg_list[segment].next_segment;
            let id = self.push_seg(segment2);
            self.sub_path_add_segment(this, id);
            if self.sub_paths[this].num_segments > n {
                return Err(ISectException("error - infinite loop"));
            }
            if segment == self.sub_paths[sub_path].first_segment {
                break;
            }
        }
        Ok(())
    }

    fn sub_path_add_segments(&mut self, this: usize, mut segment: usize) -> IResult<()> {
        let n = self.seg_list.len() as i32;
        if self.seg_list[segment].sub_path == this {
            return Ok(());
        }
        loop {
            let segment2 = self.seg_list[segment].next_segment;
            self.sub_path_add_segment(this, segment);
            if self.sub_paths[this].num_segments > n {
                return Err(ISectException("error - infinite loop"));
            }
            segment = segment2;
            if self.seg_list[segment].sub_path == this {
                break;
            }
        }
        Ok(())
    }

    fn sub_path_split_segment(&mut self, this: usize, segment: usize, mut segment2: Segment) -> usize {
        segment2.sub_path = this;
        let id = self.push_seg(segment2);
        self.link(segment, id);
        self.sub_paths[this].num_segments += 1;
        id
    }

    fn sub_path_replace_segment(&mut self, this: usize, segment: usize, mut segment2: Segment) -> usize {
        segment2.sub_path = this;
        segment2.next_segment = self.seg_list[segment].next_segment;
        segment2.prev_segment = self.seg_list[segment].prev_segment;
        self.push_seg(segment2)
    }

    fn sub_path_remove_segment(&mut self, this: usize, segment: usize) -> IResult<()> {
        if self.seg_list[segment].sub_path == this {
            if self.sub_paths[this].num_segments == 1 {
                if self.sub_paths[this].first_segment == segment {
                    self.sub_paths[this].first_segment = NIL;
                    self.sub_paths[this].last_segment = NIL;
                } else {
                    return Err(ISectException("error - cant handle"));
                }
            }
            if segment == self.sub_paths[this].first_segment {
                self.sub_paths[this].first_segment = self.seg_list[segment].next_segment;
            }
            if segment == self.sub_paths[this].last_segment {
                self.sub_paths[this].last_segment = self.seg_list[segment].prev_segment;
            }
            let (p, n) = (self.seg_list[segment].prev_segment, self.seg_list[segment].next_segment);
            self.seg_list[p].next_segment = n;
            self.seg_list[n].prev_segment = p;
            self.sub_paths[this].num_segments -= 1;
        }
        Ok(())
    }

    fn sub_path_closepath(&mut self, this: usize, f: f32, f2: f32) {
        let sp = self.sub_paths[this].start_point;
        if epsilon_equals(sp.x, f) && epsilon_equals(sp.y, f2) {
            return;
        }
        let line = Segment::new_line_pts(Point::new(f, f2), sp);
        let id = self.push_seg(line);
        self.sub_path_add_segment(this, id);
    }
}

impl OutlineConsumer for NonOverlappingOutlineConsumer {
    fn set_matrix(&mut self, m: &Matrix) {
        self.oc.borrow_mut().set_matrix(m);
    }

    fn moveto(&mut self, x: f64, y: f64) {
        if self.current_sub_path != NIL && self.sub_paths[self.current_sub_path].num_segments > 0 {
            let (cpx, cpy, cur) = (self.cpx, self.cpy, self.current_sub_path);
            self.sub_path_closepath(cur, cpx, cpy);
            let mut path = std::mem::take(&mut self.path);
            self.add_path(&mut path, cur);
            self.path = path;
        }
        self.current_sub_path = self.alloc_sub_path(SubPath::new_at(x as f32, y as f32));
        self.cpx = x as f32;
        self.cpy = y as f32;
    }

    fn lineto(&mut self, x: f64, y: f64) {
        if x == self.cpx as f64 && y == self.cpy as f64 {
            return;
        }
        if self.current_sub_path == NIL {
            self.moveto(0.0, 0.0);
        }
        let line = Segment::new_line(self.cpx, self.cpy, x as f32, y as f32);
        let id = self.push_seg(line);
        let cur = self.current_sub_path;
        self.sub_path_add_segment(cur, id);
        self.cpx = x as f32;
        self.cpy = y as f32;
    }

    fn curveto(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        if self.current_sub_path == NIL {
            self.moveto(0.0, 0.0);
        }
        let curve = Segment::new_curve(self.cpx, self.cpy, x1 as f32, y1 as f32, x2 as f32, y2 as f32, x3 as f32, y3 as f32);
        let id = self.push_seg(curve);
        let cur = self.current_sub_path;
        self.sub_path_add_segment(cur, id);
        self.cpx = x3 as f32;
        self.cpy = y3 as f32;
    }

    fn curveto_quad(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let cpx = self.cpx as f64;
        let cpy = self.cpy as f64;
        self.curveto(
            java_round_f64((cpx + 2.0 * x1) / 3.0) as f64,
            java_round_f64((cpy + 2.0 * y1) / 3.0) as f64,
            java_round_f64((2.0 * x1 + x2) / 3.0) as f64,
            java_round_f64((2.0 * y1 + y2) / 3.0) as f64,
            java_round_f64(x2) as f64,
            java_round_f64(y2) as f64,
        );
    }

    fn endchar(&mut self) {
        if self.current_sub_path != NIL && self.sub_paths[self.current_sub_path].num_segments > 0 {
            let (cpx, cpy, cur) = (self.cpx, self.cpy, self.current_sub_path);
            self.sub_path_closepath(cur, cpx, cpy);
            let mut path = std::mem::take(&mut self.path);
            self.add_path(&mut path, cur);
            self.path = path;
        }
        if self.isect_path().is_err() {
            self.debug_num_failures += 1;
            self.new_path = self.saved_path.clone();
        }
        let np = self.new_path.clone();
        self.generate_outline(np);
        let oc = self.oc.clone();
        oc.borrow_mut().endchar();
        self.reset();
    }
}

/// Stable insertion sort; never panics on an inconsistent comparator (Java's TimSort may order those differently).
fn stable_sort<F: FnMut(&usize, &usize) -> i32>(v: &mut [usize], mut cmp: F) {
    for i in 1..v.len() {
        let x = v[i];
        let mut j = i;
        while j > 0 && cmp(&x, &v[j - 1]) < 0 {
            v[j] = v[j - 1];
            j -= 1;
        }
        v[j] = x;
    }
}

struct SubPath {
    bounds: Rect,
    start_point: Point,
    path_i_sected: bool,
    first_segment: usize,
    last_segment: usize,
    num_segments: i32,
    next_sub_path: usize,
}

impl SubPath {
    fn new() -> SubPath {
        SubPath { bounds: Rect::new(0.0, 0.0, 0.0, 0.0), start_point: Point::default(), path_i_sected: false, first_segment: NIL, last_segment: NIL, num_segments: 0, next_sub_path: NIL }
    }

    fn new_at(f: f32, f2: f32) -> SubPath {
        SubPath { bounds: Rect::new(f, f2, f, f2), start_point: Point::new(f, f2), path_i_sected: false, first_segment: NIL, last_segment: NIL, num_segments: 0, next_sub_path: NIL }
    }
}

struct Intersect {
    t: f32,
    p: Point,
    seg: usize,
    split_seg: usize,
    id: i64,
}

impl Intersect {
    fn cmp_ids(&self, intersect: &Intersect) -> i32 {
        if self.id < intersect.id {
            return -1;
        }
        if self.id > intersect.id {
            return 1;
        }
        0
    }

    /// `segList.indexOf` is the segment id.
    fn cmp_segs(&self, intersect: &Intersect) -> i32 {
        let n2 = self.seg as isize;
        let n = intersect.seg as isize;
        if n2 < n {
            return -1;
        }
        if n2 > n {
            return 1;
        }
        if self.t < intersect.t {
            return -1;
        }
        if self.t > intersect.t {
            return 1;
        }
        0
    }
}

/// `Segment` with its `Line` and `Curve` subclasses; a line uses `p0` and `p1`.
#[derive(Clone)]
struct Segment {
    line: bool,
    sub_path: usize,
    flags: i32,
    next_segment: usize,
    prev_segment: usize,
    orig_seg: usize,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    t0: f32,
    t1: f32,
    depth: i64,
    bounds: Rect,
    id: i32,
}

struct ICept {
    ic: f32,
    winding: i32,
}

impl Segment {
    fn blank(line: bool, p0: Point, p1: Point, p2: Point, p3: Point) -> Segment {
        Segment { line, sub_path: NIL, flags: 0, next_segment: NIL, prev_segment: NIL, orig_seg: NIL, p0, p1, p2, p3, t0: 0.0, t1: 1.0, depth: 0, bounds: Rect::new(0.0, 0.0, 0.0, 0.0), id: 0 }
    }

    fn new_line(f: f32, f2: f32, f3: f32, f4: f32) -> Segment {
        Segment::new_line_pts(Point::new(f, f2), Point::new(f3, f4))
    }

    fn new_line_pts(point: Point, point2: Point) -> Segment {
        let mut s = Segment::blank(true, point, point2, Point::default(), Point::default());
        s.set_bounds();
        s
    }

    #[allow(clippy::too_many_arguments)]
    fn new_curve(f: f32, f2: f32, f3: f32, f4: f32, f5: f32, f6: f32, f7: f32, f8: f32) -> Segment {
        Segment::new_curve_pts(Point::new(f, f2), Point::new(f3, f4), Point::new(f5, f6), Point::new(f7, f8))
    }

    fn new_curve_pts(point: Point, point2: Point, point3: Point, point4: Point) -> Segment {
        let mut s = Segment::blank(false, point, point2, point3, point4);
        s.set_bounds();
        s
    }

    fn round(&mut self, f: f32) {
        self.round_control_points(f);
        self.set_bounds();
    }

    fn get_winding(f: f32, f2: f32) -> i32 {
        if f > f2 { 1 } else { -1 }
    }

    fn get_winding_at_value(f: f32, f2: f32, f3: f32) -> i32 {
        if f3 < f && f3 < f2 || f3 > f && f3 > f2 {
            return 0;
        }
        if f > f2 { 1 } else { -1 }
    }

    fn get_first_point(&self) -> Point {
        self.p0
    }

    fn get_last_point(&self) -> Point {
        if self.line { self.p1 } else { self.p3 }
    }

    fn horizontal(&self) -> bool {
        let l = self.get_last_point();
        (l.x - self.p0.x).abs() > (l.y - self.p0.y).abs()
    }

    fn get_winding_x(&self) -> i32 {
        Segment::get_winding(self.p0.x, self.get_last_point().x)
    }

    fn get_winding_y(&self) -> i32 {
        Segment::get_winding(self.p0.y, self.get_last_point().y)
    }

    fn equal_control_points(&self, segment: &Segment) -> bool {
        if self.line != segment.line {
            return false;
        }
        if self.line {
            return epsilon_equals(self.p0.x, segment.p0.x) && epsilon_equals(self.p0.y, segment.p0.y) && epsilon_equals(self.p1.x, segment.p1.x) && epsilon_equals(self.p1.y, segment.p1.y);
        }
        epsilon_equals(self.p0.x, segment.p0.x)
            && epsilon_equals(self.p0.y, segment.p0.y)
            && epsilon_equals(self.p1.x, segment.p1.x)
            && epsilon_equals(self.p1.y, segment.p1.y)
            && epsilon_equals(self.p2.x, segment.p2.x)
            && epsilon_equals(self.p2.y, segment.p2.y)
            && epsilon_equals(self.p3.x, segment.p3.x)
            && epsilon_equals(self.p3.y, segment.p3.y)
    }

    fn reverse_control_points(&self, segment: &Segment) -> bool {
        if self.line != segment.line {
            return false;
        }
        if self.line {
            return epsilon_equals(self.p0.x, segment.p1.x) && epsilon_equals(self.p0.y, segment.p1.y) && epsilon_equals(self.p1.x, segment.p0.x) && epsilon_equals(self.p1.y, segment.p0.y);
        }
        epsilon_equals(self.p0.x, segment.p3.x)
            && epsilon_equals(self.p0.y, segment.p3.y)
            && epsilon_equals(self.p1.x, segment.p2.x)
            && epsilon_equals(self.p1.y, segment.p2.y)
            && epsilon_equals(self.p2.x, segment.p1.x)
            && epsilon_equals(self.p2.y, segment.p1.y)
            && epsilon_equals(self.p3.x, segment.p0.x)
            && epsilon_equals(self.p3.y, segment.p0.y)
    }

    fn copy_segment(&self) -> Segment {
        let mut s = if self.line { Segment::new_line_pts(self.p0, self.p1) } else { Segment::new_curve_pts(self.p0, self.p1, self.p2, self.p3) };
        s.t0 = self.t0;
        s.t1 = self.t1;
        s.id = self.id;
        s.flags = self.flags;
        s.orig_seg = self.orig_seg;
        s
    }

    fn round_control_points(&mut self, f: f32) {
        self.p0.x = java_round_f32(self.p0.x) as f32;
        self.p0.y = java_round_f32(self.p0.y) as f32;
        self.p1.x = java_round_f32(self.p1.x) as f32;
        self.p1.y = java_round_f32(self.p1.y) as f32;
        if self.line {
            return;
        }
        self.p2.x = java_round_f32(self.p2.x) as f32;
        self.p2.y = java_round_f32(self.p2.y) as f32;
        self.p3.x = java_round_f32(self.p3.x) as f32;
        self.p3.y = java_round_f32(self.p3.y) as f32;
        if self.is_line(f) {
            let _ = self.make_line();
        }
    }

    fn set_bounds(&mut self) {
        if self.line {
            let (f4, f3) = if self.p0.x < self.p1.x { (self.p0.x, self.p1.x) } else { (self.p1.x, self.p0.x) };
            let (f2, f) = if self.p0.y < self.p1.y { (self.p0.y, self.p1.y) } else { (self.p1.y, self.p0.y) };
            self.bounds = Rect::new(f4, f2, f3, f);
            return;
        }
        let mut rect = Segment::new_line_pts(self.p0, self.p3).bounds;
        if self.p1.x < rect.left || self.p1.x > rect.right || self.p2.x < rect.left || self.p2.x > rect.right {
            let (mn, mx) = Segment::get_bez_limit(self.p0.x, self.p1.x, self.p2.x, self.p3.x, rect.left, rect.right);
            rect.left = mn;
            rect.right = mx;
        }
        if self.p1.y < rect.bottom || self.p1.y > rect.top || self.p2.y < rect.bottom || self.p2.y > rect.top {
            let (mn, mx) = Segment::get_bez_limit(self.p0.y, self.p1.y, self.p2.y, self.p3.y, rect.bottom, rect.top);
            rect.bottom = mn;
            rect.top = mx;
        }
        self.bounds = rect;
    }

    /// `Curve.getBezLimit`, returning `Limit(min, max)`.
    fn get_bez_limit(f: f32, f2: f32, f3: f32, f4: f32, f5: f32, f6: f32) -> (f32, f32) {
        let mut arr = [0.0f32; 2];
        let mut n: isize = 0;
        let f8 = f4 - 3.0 * (f3 - f2) - f;
        let f9 = f3 - 2.0 * f2 + f;
        let f10 = f2 - f;
        let mut f11 = f5;
        let mut f12 = f6;
        if f8 == 0.0 {
            if f9 != 0.0 {
                arr[n as usize] = -f10 / (2.0 * f9);
                n += 1;
            }
        } else {
            let mut f7 = f9 * f9 - f8 * f10;
            if f7 >= 0.0 {
                f7 = (f7 as f64).sqrt() as f32;
                arr[n as usize] = (-f9 + f7) / f8;
                n += 1;
                arr[n as usize] = (-f9 - f7) / f8;
                n += 1;
            }
        }
        n -= 1;
        while n >= 0 {
            let t = arr[n as usize];
            if t > 0.0 && t < 1.0 {
                let f7 = t * (t * (t * f8 + 3.0 * f9) + 3.0 * f10) + f;
                if f7 < f5 {
                    f11 = f7;
                } else if f7 > f6 {
                    f12 = f7;
                }
            }
            n -= 1;
        }
        (f11, f12)
    }

    fn compute_isect_point(&self, f: f32) -> Point {
        if self.line {
            let f2 = self.p0.x + f * (self.p1.x - self.p0.x);
            let f3 = self.p0.y + f * (self.p1.y - self.p0.y);
            return Point::new(f2, f3);
        }
        let (p0, p1, p2, p3) = (self.p0, self.p1, self.p2, self.p3);
        let f2 = f * (f * (f * (p3.x - 3.0 * (p2.x - p1.x) - p0.x) + 3.0 * (p2.x - 2.0 * p1.x + p0.x)) + 3.0 * (p1.x - p0.x)) + p0.x;
        let f3 = f * (f * (f * (p3.y - 3.0 * (p2.y - p1.y) - p0.y) + 3.0 * (p2.y - 2.0 * p1.y + p0.y)) + 3.0 * (p1.y - p0.y)) + p0.y;
        Point::new(f2, f3)
    }

    fn is_line(&self, f: f32) -> bool {
        let f2 = self.p3.y - self.p0.y;
        let f3 = self.p3.x - self.p0.x;
        if f2.abs() <= 1.0 && f3.abs() <= 1.0 {
            return true;
        }
        let f4 = f2 * (self.p1.x - self.p0.x) + f3 * (self.p0.y - self.p1.y);
        let f5 = f2 * (self.p2.x - self.p0.x) + f3 * (self.p0.y - self.p2.y);
        let f6 = milliem(f, 1.0);
        f4 * f4 + f5 * f5 < f6 * f6 * (f2 * f2 + f3 * f3)
    }

    fn make_line(&self) -> Segment {
        let mut line = Segment::new_line(self.p0.x, self.p0.y, self.p3.x, self.p3.y);
        line.id = self.id;
        line.t0 = self.t0;
        line.t1 = self.t1;
        line.orig_seg = self.orig_seg;
        line
    }

    fn split_at_t(&self, mut f: f32) -> (Segment, Segment) {
        let (p0, p1, p2, p3) = (self.p0, self.p1, self.p2, self.p3);
        let f2 = p0.x;
        let f3 = p0.y;
        let f4 = f * (p1.x - p0.x) + f2;
        let f5 = f * (p1.y - p0.y) + f3;
        let f6 = f * f * (p2.x - 2.0 * p1.x + p0.x) + 2.0 * f4 - f2;
        let f7 = f * f * (p2.y - 2.0 * p1.y + p0.y) + 2.0 * f5 - f3;
        let f8 = f * f * f * (p3.x - 3.0 * (p2.x - p1.x) - p0.x) + 3.0 * (f6 - f4) + f2;
        let f9 = f * f * f * (p3.y - 3.0 * (p2.y - p1.y) - p0.y) + 3.0 * (f7 - f5) + f3;
        let mut curve = Segment::new_curve(f2, f3, f4, f5, f6, f7, f8, f9);
        curve.t0 = 0.0;
        curve.t1 = f;
        curve.orig_seg = self.orig_seg;
        f = 1.0 - f;
        let f10 = p3.x;
        let f11 = p3.y;
        let f12 = f * (p2.x - p3.x) + f10;
        let f13 = f * (p2.y - p3.y) + f11;
        let f14 = f * f * (p1.x - 2.0 * p2.x + p3.x) + 2.0 * f12 - f10;
        let f15 = f * f * (p1.y - 2.0 * p2.y + p3.y) + 2.0 * f13 - f11;
        let mut curve2 = Segment::new_curve(f8, f9, f14, f15, f12, f13, f10, f11);
        curve2.t0 = f;
        curve2.t1 = 1.0;
        curve2.orig_seg = self.orig_seg;
        (curve, curve2)
    }

    fn split_mid(&self) -> (Segment, Segment) {
        let (p0, p1, p2, p3) = (self.p0, self.p1, self.p2, self.p3);
        let point = Point::new((p2.x + p3.x) / 2.0, (p2.y + p3.y) / 2.0);
        let point2 = Point::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
        let point3 = Point::new((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
        let point4 = Point::new((point.x + 2.0 * point2.x + point3.x) / 4.0, (point.y + 2.0 * point2.y + point3.y) / 4.0);
        let f = (self.t0 + self.t1) / 2.0;
        let mut curve = Segment::new_curve(p0.x, p0.y, point3.x, point3.y, (point2.x + point3.x) / 2.0, (point2.y + point3.y) / 2.0, point4.x, point4.y);
        curve.t0 = self.t0;
        curve.t1 = f;
        curve.depth = self.depth + 1;
        curve.orig_seg = self.orig_seg;
        let mut curve2 = Segment::new_curve(point4.x, point4.y, (point.x + point2.x) / 2.0, (point.y + point2.y) / 2.0, point.x, point.y, p3.x, p3.y);
        curve2.t0 = f;
        curve2.t1 = self.t1;
        curve2.depth = self.depth;
        curve2.orig_seg = self.orig_seg;
        (curve, curve2)
    }

    fn is_flat_enough(&mut self, f: f32) -> bool {
        if self.depth == 6 {
            return true;
        }
        if (self.bounds.right - self.bounds.left).abs() > milliem(f, 127.0) || (self.bounds.top - self.bounds.bottom).abs() > milliem(f, 127.0) {
            self.depth = 0;
            return false;
        }
        let (p0, p1, p2, p3) = (self.p0, self.p1, self.p2, self.p3);
        if (p0.x > p1.x || p1.x > p2.x || p2.x > p3.x) && (p3.x > p2.x || p2.x > p1.x || p1.x > p0.x) || (p0.y > p1.y || p1.y > p2.y || p2.y > p3.y) && (p3.y > p2.y || p2.y > p1.y || p1.y > p0.y) {
            return false;
        }
        let f2 = milliem(f, 9.0);
        let f3 = (p1.x - p0.x).abs();
        let f4 = (p3.x - p0.x).abs();
        if (f4 - 3.0 * f3).abs() > f2 {
            return false;
        }
        let f5 = (p2.x - p0.x).abs();
        if (2.0 * (f4 - f5) - f5).abs() > f2 {
            return false;
        }
        let f6 = (p1.y - p0.y).abs();
        let f7 = (p3.y - p0.y).abs();
        if (f7 - 3.0 * f6).abs() > f2 {
            return false;
        }
        let f8 = (p2.y - p0.y).abs();
        !((2.0 * (f7 - f8) - f8).abs() > f2)
    }

    fn find_extrema(f: f32, f2: f32, f3: f32, f4: f32) -> Vec<f32> {
        let mut arr = [0.0f32; 2];
        let mut n: isize = 0;
        let mut n2 = 0usize;
        let f5 = f4 - 3.0 * (f3 - f2) - f;
        let f6 = f3 - 2.0 * f2 + f;
        let f7 = f2 - f;
        if f5 == 0.0 {
            if f6 != 0.0 {
                arr[n as usize] = -f7 / (2.0 * f6);
                n += 1;
            }
        } else {
            let mut f8 = f6 * f6 - f5 * f7;
            if f8 >= 0.0 {
                f8 = (f8 as f64).sqrt() as f32;
                arr[n as usize] = (-f6 + f8) / f5;
                n += 1;
                arr[n as usize] = (-f6 - f8) / f5;
                n += 1;
            }
        }
        let mut arr2 = [0.0f32; 2];
        n -= 1;
        while n >= 0 {
            if 0.0 < arr[n as usize] && arr[n as usize] < 1.0 {
                arr2[n2] = arr[n as usize];
                n2 += 1;
            }
            n -= 1;
        }
        if n2 == 2 && arr2[0] > arr2[1] {
            arr2[0] = arr[1];
            arr2[1] = arr[0];
        }
        arr2[..n2].to_vec()
    }

    fn solve_at_value(f: f32, f2: f32, f3: f32, f4: f32, f5: f32) -> f32 {
        let f8 = f2 - 3.0 * (f3 - f4) - f5;
        let f9 = 3.0 * (f3 - 2.0 * f4 + f5);
        let f10 = 3.0 * (f4 - f5);
        let mut f11 = 0.0f32;
        let mut f12 = 1.0f32;
        loop {
            let f6 = (f11 + f12) / 2.0;
            let f7 = f6 * (f6 * (f6 * f8 + f9) + f10) + f5 - f;
            if (f7.abs() as f64) < 0.01 {
                return f6;
            }
            if f7 < 0.0 {
                f11 = f6;
                continue;
            }
            f12 = f6;
        }
    }

    /// `Curve.solveAtY` / `solveAtX` (`y` selects which).
    fn curve_solve_at(&self, f: f32, y: bool) -> Vec<ICept> {
        let c = |p: Point| if y { p.y } else { p.x };
        let o = |p: Point| if y { p.x } else { p.y };
        let list = Segment::find_extrema(c(self.p3), c(self.p2), c(self.p1), c(self.p0));
        let n = list.len();
        let mut curves: Vec<Segment> = Vec::with_capacity(n + 1);
        if n > 0 {
            let f2 = list[0];
            let (a, b) = self.split_at_t(f2);
            curves.push(a);
            curves.push(b.clone());
            if n > 1 {
                let f3 = list[1];
                let f4 = (f3 - f2) / (1.0 - f2);
                let (c0, _) = curves[1].split_at_t(f4);
                curves[1] = c0;
                curves.push(b);
            }
        } else {
            curves.push(Segment::new_curve_pts(self.p0, self.p1, self.p2, self.p3));
        }
        let mut out = Vec::new();
        for cv in curves.iter().take(n + 1) {
            let (point4, point3, point2, point) = if c(cv.p3) > c(cv.p0) { (cv.p0, cv.p1, cv.p2, cv.p3) } else { (cv.p3, cv.p2, cv.p1, cv.p0) };
            let n2 = if y { cv.get_winding_y() } else { cv.get_winding_x() };
            if f < c(point4) || f > c(point) {
                continue;
            }
            let f5 = if epsilon_equals(f, c(point4)) {
                o(point4)
            } else if epsilon_equals(f, c(point)) {
                o(point)
            } else {
                let f6 = Segment::solve_at_value(f, c(point), c(point2), c(point3), c(point4));
                f6 * (f6 * (f6 * (o(point) - 3.0 * (o(point2) - o(point3)) - o(point4)) + 3.0 * (o(point2) - 2.0 * o(point3) + o(point4))) + 3.0 * (o(point3) - o(point4))) + o(point4)
            };
            out.push(ICept { ic: f5, winding: n2 });
        }
        out
    }

    fn solve_at_x(&self, f: f32) -> Vec<ICept> {
        if self.line {
            let f2 = self.p0.y + (f - self.p0.x) * (self.p1.y - self.p0.y) / (self.p1.x - self.p0.x);
            return vec![ICept { ic: f2, winding: self.get_winding_x() }];
        }
        self.curve_solve_at(f, false)
    }

    fn solve_at_y(&self, f: f32) -> Vec<ICept> {
        if self.line {
            let f2 = self.p0.x + (f - self.p0.y) * (self.p1.x - self.p0.x) / (self.p1.y - self.p0.y);
            return vec![ICept { ic: f2, winding: self.get_winding_y() }];
        }
        self.curve_solve_at(f, true)
    }

    fn i_sect_rect(&self, rect: &Rect) -> bool {
        let mut n = if self.p0.x < rect.left { 1 } else if self.p0.x > rect.right { 2 } else { 0 };
        if self.p0.y > rect.top {
            n |= 8;
        } else if self.p0.y < rect.bottom {
            n |= 4;
        }
        if n == 0 {
            return true;
        }
        let mut n2 = if self.p1.x < rect.left { 1 } else if self.p1.x > rect.right { 2 } else { 0 };
        if self.p1.y > rect.top {
            n2 |= 8;
        } else if self.p1.y < rect.bottom {
            n2 |= 4;
        }
        if n2 == 0 {
            return true;
        }
        (n & n2) == 0
    }

    fn i_sect_line(&self, line: &Segment) -> Vec<[f32; 2]> {
        let f = self.p1.y - self.p0.y;
        let f2 = self.p1.x - self.p0.x;
        let f3 = line.p1.y - line.p0.y;
        let f4 = line.p1.x - line.p0.x;
        let f5 = f3 * f2 - f4 * f;
        let f6 = self.p0.y - line.p0.y;
        let f7 = self.p0.x - line.p0.x;
        let f8 = f4 * f6 - f3 * f7;
        let mut out = Vec::new();
        if f5 == 0.0 {
            if f8 == 0.0 {
                let rect = line.bounds;
                if !self.bounds.overlap(&rect) {
                    return out;
                }
                let rect2 = self.bounds.i_sect(&rect);
                if rect2.left == rect2.right && rect2.bottom == rect2.top {
                    let mut a = [0.0f32; 2];
                    if f2 != 0.0 {
                        a[0] = if epsilon_equals(self.p1.x, rect2.left) { 1.0 } else { 0.0 };
                        a[1] = if epsilon_equals(line.p1.x, rect2.left) { 1.0 } else { 0.0 };
                    } else {
                        a[0] = if epsilon_equals(self.p1.y, rect2.bottom) { 1.0 } else { 0.0 };
                        a[1] = if epsilon_equals(line.p1.y, rect2.bottom) { 1.0 } else { 0.0 };
                    }
                    out.push(a);
                    return out;
                }
                if rect2.right - rect2.left > rect2.top - rect2.bottom {
                    out.push([(rect2.left - self.p0.x) / f2, (rect2.left - line.p0.x) / f4]);
                    out.push([(rect2.right - self.p0.x) / f2, (rect2.right - line.p0.x) / f4]);
                    return out;
                }
                out.push([(rect2.bottom - self.p0.y) / f, (rect2.bottom - line.p0.y) / f3]);
                out.push([(rect2.top - self.p0.y) / f, (rect2.top - line.p0.y) / f3]);
                return out;
            }
            return out;
        }
        let a0 = f8 / f5;
        if a0 < 0.0 || a0 > 1.0 {
            return out;
        }
        let a1 = (f2 * f6 - f * f7) / f5;
        if a1 < 0.0 || a1 > 1.0 {
            return out;
        }
        out.push([a0, a1]);
        out
    }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
}

impl Rect {
    fn new(f: f32, f2: f32, f3: f32, f4: f32) -> Rect {
        Rect { left: f, bottom: f2, right: f3, top: f4 }
    }

    fn grow(&mut self, rect: &Rect) {
        if self.left > rect.left {
            self.left = rect.left;
        }
        if self.bottom > rect.bottom {
            self.bottom = rect.bottom;
        }
        if self.right < rect.right {
            self.right = rect.right;
        }
        if self.top < rect.top {
            self.top = rect.top;
        }
    }

    fn overlap(&self, rect: &Rect) -> bool {
        let bl = self.right < rect.left || self.left > rect.right || self.top < rect.bottom || self.bottom > rect.top;
        !bl
    }

    fn i_sect(&self, rect: &Rect) -> Rect {
        let f = jmax_f32(self.left, rect.left);
        let f2 = jmin_f32(self.right, rect.right);
        let f3 = jmin_f32(self.top, rect.top);
        let f4 = jmax_f32(self.bottom, rect.bottom);
        Rect::new(f, f4, f2, f3)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Point {
    x: f32,
    y: f32,
}

impl Point {
    fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    fn equals(&self, point: &Point) -> bool {
        epsilon_equals(self.x, point.x) && epsilon_equals(self.y, point.y)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::asc::flex2::fonts::adobe::fontengine::font::hinted_outline_consumer::HintedOutlineConsumer;
    use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
    use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;

    /// Dumps written by the `AcDump` Java harness of the autocolor port.
    pub const DUMPS: &str = r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\autocolor\dumps";

    pub fn d(v: f64) -> String {
        if v == v.round_ties_even() && v.abs() < 1e9 && !(v == 0.0 && v.is_sign_negative()) {
            format!("{}", v as i64)
        } else {
            format!("x{:x}", v.to_bits())
        }
    }

    pub fn num(s: &str) -> f64 {
        match s.strip_prefix('x') {
            Some(h) => f64::from_bits(u64::from_str_radix(h, 16).unwrap()),
            None => s.parse::<i64>().unwrap() as f64,
        }
    }

    pub struct Rec {
        pub prefix: &'static str,
        pub lines: Vec<String>,
    }

    impl Rec {
        fn ln(&mut self, s: String) {
            self.lines.push(format!("{} {}", self.prefix, s));
        }
    }

    fn b(v: bool) -> &'static str {
        if v { "1" } else { "0" }
    }

    impl OutlineConsumer for Rec {
        fn set_matrix(&mut self, m: &Matrix) {
            self.ln(format!("s {} {} {} {} {} {}", d(m.a), d(m.b), d(m.c), d(m.d), d(m.tx), d(m.ty)));
        }
        fn moveto(&mut self, x: f64, y: f64) {
            self.ln(format!("m {} {}", d(x), d(y)));
        }
        fn lineto(&mut self, x: f64, y: f64) {
            self.ln(format!("l {} {}", d(x), d(y)));
        }
        fn curveto(&mut self, a: f64, b: f64, c: f64, e: f64, f: f64, g: f64) {
            self.ln(format!("c {} {} {} {} {} {}", d(a), d(b), d(c), d(e), d(f), d(g)));
        }
        fn curveto_quad(&mut self, a: f64, b: f64, c: f64, e: f64) {
            self.ln(format!("q {} {} {} {}", d(a), d(b), d(c), d(e)));
        }
        fn endchar(&mut self) {
            self.ln("e".to_string());
        }
    }

    impl HintedOutlineConsumer for Rec {
        fn stem(&mut self, a: f64, c: f64, x: bool, y: bool, z: bool) {
            self.ln(format!("t {} {} {}{}{}", d(a), d(c), b(x), b(y), b(z)));
        }
        fn stem3(&mut self, a: f64, c: f64, e: f64, f: f64, g: f64, h: f64, x: bool, y: bool) {
            self.ln(format!("T {} {} {} {} {} {} {}{}", d(a), d(c), d(e), d(f), d(g), d(h), b(x), b(y)));
        }
        fn global_color_on(&mut self) {
            self.ln("g".to_string());
        }
        fn no_counters(&mut self) {
            self.ln("n".to_string());
        }
        fn no_hints(&mut self) {
            self.ln("h".to_string());
        }
        fn width(&mut self, w: f64) -> bool {
            self.ln(format!("w {}", d(w)));
            true
        }
        fn closepath(&mut self) {
            self.ln("z".to_string());
        }
        fn flex(&mut self, a: f64, b: f64, c: f64, e: f64, f: f64, g: f64, h: f64, i: f64, j: f64, k: f64, l: f64, m: f64, n: f64) {
            self.ln(format!("f {} {} {} {} {} {} {} {} {} {} {} {} {}", d(a), d(b), d(c), d(e), d(f), d(g), d(h), d(i), d(j), d(k), d(l), d(m), d(n)));
        }
    }

    /// Feeds recorded `I` lines (without prefix) to `oc`.
    pub fn replay(input: &[String], oc: &mut dyn OutlineConsumer) {
        for l in input {
            let t: Vec<&str> = l.split(' ').collect();
            let v: Vec<f64> = t[1..].iter().map(|s| num(s)).collect();
            match t[0] {
                "s" => oc.set_matrix(&Matrix::new(v[0], v[1], v[2], v[3], v[4], v[5])),
                "m" => oc.moveto(v[0], v[1]),
                "l" => oc.lineto(v[0], v[1]),
                "c" => oc.curveto(v[0], v[1], v[2], v[3], v[4], v[5]),
                "q" => oc.curveto_quad(v[0], v[1], v[2], v[3]),
                "e" => oc.endchar(),
                x => panic!("bad op {}", x),
            }
        }
    }

    #[derive(Default)]
    pub struct Glyph {
        pub gid: i32,
        pub idx: i32,
        pub adv: f64,
        pub code: i32,
        pub err: bool,
        pub input: Vec<String>,
        pub n: Vec<String>,
        pub a: Vec<String>,
        pub z: String,
        pub d: Vec<String>,
        pub e: String,
        /// Streams whose Java run threw (`NX`, `AX`, ...).
        pub xs: String,
    }

    #[derive(Default)]
    pub struct Face {
        pub upem_y: f64,
        pub upem_x: f64,
        pub top: Vec<i32>,
        pub bot: Vec<i32>,
        pub glyphs: Vec<Glyph>,
        pub zchars: Vec<Glyph>,
    }

    fn ints(s: &str) -> Vec<i32> {
        s.split(' ').skip(1).map(|x| x.parse().unwrap()).collect()
    }

    pub fn load(name: &str) -> Option<Vec<Face>> {
        let text = std::fs::read_to_string(format!("{}\\{}.txt", DUMPS, name)).ok()?;
        let mut faces: Vec<Face> = Vec::new();
        let mut in_z = false;
        for line in text.lines() {
            let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
            let face = faces.last_mut();
            match tag {
                "#" => {}
                "F" => {
                    let t: Vec<&str> = rest.split(' ').collect();
                    faces.push(Face { upem_y: num(t[1]), upem_x: num(t[2]), ..Default::default() });
                    in_z = false;
                }
                "ZT" => face.unwrap().top = ints(rest),
                "ZB" => face.unwrap().bot = ints(rest),
                "G" => {
                    let t: Vec<&str> = rest.split(' ').collect();
                    let code = t.get(3).map_or(0, |c| c.parse().unwrap());
                    face.unwrap().glyphs.push(Glyph { gid: t[0].parse().unwrap(), idx: t[1].parse().unwrap(), adv: num(t[2]), code, ..Default::default() });
                }
                "C" => {
                    let t: Vec<&str> = rest.split(' ').collect();
                    face.unwrap().zchars.push(Glyph { code: t[0].parse().unwrap(), gid: t[1].parse().unwrap(), adv: num(t[2]), ..Default::default() });
                    in_z = true;
                }
                _ => {
                    let f = face.unwrap();
                    let g = if in_z { f.zchars.last_mut().unwrap() } else { f.glyphs.last_mut().unwrap() };
                    match tag {
                        "X" => g.err = true,
                        "I" => g.input.push(rest.to_string()),
                        "N" => g.n.push(line.to_string()),
                        "A" => g.a.push(line.to_string()),
                        "Z" => g.z = rest.to_string(),
                        "D" => g.d.push(line.to_string()),
                        "E" => g.e = rest.to_string(),
                        "NX" | "AX" | "DX" | "ZX" | "EX" => g.xs.push_str(&tag[..1]),
                        _ => panic!("bad line {}", line),
                    }
                }
            }
        }
        Some(faces)
    }

    pub const FONTS: &[&str] = &[
        "SegUIVar", "arial", "arialbd", "arialbi", "ariali", "cambria", "comic", "consola", "georgia", "impact", "l_10646", "malgun", "marlett", "micross", "msgothic", "nogsub", "nokern", "nos2", "opensans", "segoeui", "simsun", "symbol",
        "tahoma", "times", "timesbd", "verdana", "webdings", "wingding",
    ];
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    #[test]
    fn non_overlapping_matches_java() {
        let mut total = 0;
        let mut shown = 0;
        for name in FONTS {
            let Some(faces) = load(name) else { return };
            let (mut ok, mut n, mut ov, mut fl) = (0, 0, 0, 0);
            for face in &faces {
                let rec = Rc::new(RefCell::new(Rec { prefix: "N", lines: Vec::new() }));
                let mut noo = NonOverlappingOutlineConsumer::new(rec.clone(), face.upem_y);
                for g in face.glyphs.iter().filter(|g| !g.err) {
                    rec.borrow_mut().lines.clear();
                    replay(&g.input, &mut noo);
                    n += 1;
                    if rec.borrow().lines == g.n {
                        ok += 1;
                    } else if shown < 5 {
                        shown += 1;
                        eprintln!("{} gid {}:\n  java {:?}\n  rust {:?}", name, g.gid, g.n, rec.borrow().lines);
                    }
                }
                ov += noo.debug_num_glyphs_with_overlaps;
                fl += noo.debug_num_failures;
            }
            eprintln!("{}: {}/{} glyphs ({} with overlaps, {} failed)", name, ok, n, ov, fl);
            total += n - ok;
        }
        assert_eq!(total, 0);
    }

    #[test]
    fn non_overlapping_synthetic_matches_java() {
        let (mut ok, mut n, mut ov, mut fl, mut shown) = (0, 0, 0, 0, 0);
        let faces: Vec<Face> = ["synth1", "synth2", "synth3"].iter().filter_map(|n| load(n)).flatten().collect();
        for face in &faces {
            let rec = Rc::new(RefCell::new(Rec { prefix: "N", lines: Vec::new() }));
            let mut noo = NonOverlappingOutlineConsumer::new(rec.clone(), face.upem_y);
            for g in &face.glyphs {
                assert!(!g.xs.contains('N'));
                rec.borrow_mut().lines.clear();
                replay(&g.input, &mut noo);
                n += 1;
                if rec.borrow().lines == g.n {
                    ok += 1;
                } else if shown < 3 {
                    shown += 1;
                    eprintln!("gid {}:
  in   {:?}
  java {:?}
  rust {:?}", g.gid, g.input, g.n, rec.borrow().lines);
                }
            }
            ov += noo.debug_num_glyphs_with_overlaps;
            fl += noo.debug_num_failures;
        }
        eprintln!("synthetic: {}/{} glyphs ({} with overlaps, {} failed)", ok, n, ov, fl);
        assert_eq!(ok, n);
    }
}
