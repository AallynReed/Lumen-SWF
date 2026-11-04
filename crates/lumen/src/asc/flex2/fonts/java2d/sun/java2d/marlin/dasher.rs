//! `sun.java2d.marlin.Dasher` and `Dasher.LengthIterator`.

use super::float_math;
use super::helpers;
use super::marlin_const::CURVE_LEN_ERR;
use super::renderer_context::RendererContext;
use super::transforming_path_consumer2d::{CurveBasicMonotonizer, CurveClipSplitter};
use super::DPathConsumer2D;

const REC_LIMIT: i32 = 16;
const MIN_T_INC: f64 = 1.0 / (1 << REC_LIMIT) as f64;
const EPS: f64 = 1e-6;
const MAX_CYCLES: f64 = 16000000.0;

/// `Dasher`.
pub struct Dasher<'a> {
    out: Box<dyn DPathConsumer2D + 'a>,
    dash: Vec<f64>,
    dash_len: i32,
    start_phase: f64,
    start_dash_on: bool,
    start_idx: i32,
    starting: bool,
    needs_move_to: bool,
    idx: i32,
    dash_on: bool,
    phase: f64,
    sx0: f64,
    sy0: f64,
    cx0: f64,
    cy0: f64,
    cur_curvepts: [f64; 8 * 2],
    first_segments_buffer: Vec<f64>,
    clip_rect: Option<[f64; 4]>,
    c_out_code: i32,
    subdivide: bool,
    li: LengthIterator,
    curve_splitter: Option<Box<CurveClipSplitter>>,
    cycle_len: f64,
    outside: bool,
    total_skip_len: f64,
    /// `RendererContext.monotonizer`.
    monotonizer: CurveBasicMonotonizer,
}

/// `Dasher.copyDashArray(float[])`.
pub fn copy_dash_array(dashes: &[f32]) -> Vec<f64> {
    dashes.iter().map(|&d| d as f64).collect()
}

impl<'a> Dasher<'a> {
    /// `Dasher.init(DPathConsumer2D, double[], int, double, boolean)`; `line_width` initializes the context's
    /// monotonizer as `DMarlinRenderingEngine.strokeTo` does.
    pub fn new(
        rdr_ctx: &RendererContext,
        pc2d: Box<dyn DPathConsumer2D + 'a>,
        dash: Vec<f64>,
        dash_len: i32,
        mut phase: f64,
        line_width: f64,
        curve_splitter: Option<Box<CurveClipSplitter>>,
    ) -> Dasher<'a> {
        let mut sidx: i32 = 0;
        let mut dash_on = true;

        let mut sum = 0.0;
        for i in 0..dash_len as usize {
            sum += dash[i];
        }
        let cycle_len = sum;

        let cycles = phase / sum;

        if phase < 0.0 {
            if -cycles >= MAX_CYCLES {
                phase = 0.0;
            } else {
                let fullcycles = float_math::floor_int(-cycles);
                if (fullcycles & dash_len & 1) != 0 {
                    dash_on = !dash_on;
                }
                phase += fullcycles as f64 * sum;
                while phase < 0.0 {
                    sidx -= 1;
                    if sidx < 0 {
                        sidx = dash_len - 1;
                    }
                    phase += dash[sidx as usize];
                    dash_on = !dash_on;
                }
            }
        } else if phase > 0.0 {
            if cycles >= MAX_CYCLES {
                phase = 0.0;
            } else {
                let fullcycles = float_math::floor_int(cycles);
                if (fullcycles & dash_len & 1) != 0 {
                    dash_on = !dash_on;
                }
                phase -= fullcycles as f64 * sum;
                loop {
                    let d = dash[sidx as usize];
                    if !(phase >= d) {
                        break;
                    }
                    phase -= d;
                    sidx = (sidx + 1) % dash_len;
                    dash_on = !dash_on;
                }
            }
        }

        let mut monotonizer = CurveBasicMonotonizer::new();
        monotonizer.init(line_width);

        let (clip_rect, c_out_code) = if rdr_ctx.do_clip { (Some(rdr_ctx.clip_rect), 0) } else { (None, 0) };

        Dasher {
            out: pc2d,
            dash,
            dash_len,
            start_phase: phase,
            start_dash_on: dash_on,
            start_idx: sidx,
            starting: true,
            needs_move_to: false,
            idx: 0,
            dash_on,
            phase,
            sx0: 0.0,
            sy0: 0.0,
            cx0: 0.0,
            cy0: 0.0,
            cur_curvepts: [0.0; 16],
            first_segments_buffer: Vec::new(),
            clip_rect,
            c_out_code,
            subdivide: super::marlin_const::DO_CLIP_SUBDIVIDER,
            li: LengthIterator::new(),
            curve_splitter,
            cycle_len,
            outside: false,
            total_skip_len: 0.0,
            monotonizer,
        }
    }

    /// `Dasher.dispose()`.
    fn dispose(&mut self) {
        self.first_segments_buffer.clear();
    }

    /// `Dasher.emitSeg`.
    fn emit_seg(out: &mut dyn DPathConsumer2D, buf: &[f64], off: usize, type_: usize) {
        match type_ {
            4 => out.line_to(buf[off], buf[off + 1]),
            8 => out.curve_to(buf[off], buf[off + 1], buf[off + 2], buf[off + 3], buf[off + 4], buf[off + 5]),
            6 => out.quad_to(buf[off], buf[off + 1], buf[off + 2], buf[off + 3]),
            _ => {}
        }
    }

    /// `Dasher.emitFirstSegments()`.
    fn emit_first_segments(&mut self) {
        let fsb = &self.first_segments_buffer;
        let mut i = 0;
        let len = fsb.len();
        while i < len {
            let type_ = fsb[i] as i32 as usize;
            Dasher::emit_seg(&mut *self.out, fsb, i + 1, type_);
            i += type_ - 1;
        }
        self.first_segments_buffer.clear();
    }

    /// `Dasher.goTo` on `cur_curvepts`.
    fn go_to(&mut self, off: usize, type_: usize, on: bool) {
        let index = off + type_;
        let x = self.cur_curvepts[index - 4];
        let y = self.cur_curvepts[index - 3];

        if on {
            if self.starting {
                self.go_to_starting(off, type_);
            } else {
                if self.needs_move_to {
                    self.needs_move_to = false;
                    self.out.move_to(self.cx0, self.cy0);
                }
                Dasher::emit_seg(&mut *self.out, &self.cur_curvepts, off, type_);
            }
        } else {
            if self.starting {
                self.starting = false;
            }
            self.needs_move_to = true;
        }
        self.cx0 = x;
        self.cy0 = y;
    }

    /// `Dasher.goTo_starting` on `cur_curvepts`.
    fn go_to_starting(&mut self, off: usize, type_: usize) {
        let len = type_ - 2;
        self.first_segments_buffer.push(type_ as f64);
        self.first_segments_buffer.extend_from_slice(&self.cur_curvepts[off..off + len]);
    }

    /// `Dasher._lineTo`.
    fn _line_to(&mut self, x1: f64, y1: f64) {
        let dx = x1 - self.cx0;
        let dy = y1 - self.cy0;

        let mut len = dx * dx + dy * dy;
        if len == 0.0 {
            return;
        }
        len = len.sqrt();

        let cx = dx / len;
        let cy = dy / len;

        let dash_len = self.dash_len;

        let mut idx = self.idx;
        let mut dash_on = self.dash_on;
        let mut phase = self.phase;

        loop {
            let d = self.dash[idx as usize] - phase;
            let rem = len - d;

            if rem <= EPS {
                self.cur_curvepts[0] = x1;
                self.cur_curvepts[1] = y1;

                self.go_to(0, 4, dash_on);

                phase += len;

                if rem.abs() <= EPS {
                    phase = 0.0;
                    idx = (idx + 1) % dash_len;
                    dash_on = !dash_on;
                }
                break;
            }

            self.cur_curvepts[0] = self.cx0 + d * cx;
            self.cur_curvepts[1] = self.cy0 + d * cy;

            self.go_to(0, 4, dash_on);

            len = rem;
            idx = (idx + 1) % dash_len;
            dash_on = !dash_on;
            phase = 0.0;
        }
        self.idx = idx;
        self.dash_on = dash_on;
        self.phase = phase;
    }

    /// `Dasher.skipLineTo`.
    fn skip_line_to(&mut self, x1: f64, y1: f64) {
        let dx = x1 - self.cx0;
        let dy = y1 - self.cy0;

        let mut len = dx * dx + dy * dy;
        if len != 0.0 {
            len = len.sqrt();
        }

        self.outside = true;
        self.total_skip_len += len;

        self.needs_move_to = true;
        self.starting = false;

        self.cx0 = x1;
        self.cy0 = y1;
    }

    /// `Dasher.skipLen()`.
    fn skip_len(&mut self) {
        let mut len = self.total_skip_len;
        self.total_skip_len = 0.0;

        let dash_len = self.dash_len;

        let mut idx = self.idx;
        let mut dash_on = self.dash_on;
        let mut phase = self.phase;

        let skip = ((len / self.cycle_len).floor() as i64).wrapping_sub(2);

        if skip > 0 {
            len -= self.cycle_len * skip as f64;

            let iterations = skip.wrapping_mul(dash_len as i64);
            idx = (iterations.wrapping_add(idx as i64) as i32) % dash_len;
            dash_on = (iterations.wrapping_add(if dash_on { 1 } else { 0 }) & 1) == 1;
        }

        loop {
            let d = self.dash[idx as usize] - phase;
            let rem = len - d;

            if rem <= EPS {
                phase += len;

                if rem.abs() <= EPS {
                    phase = 0.0;
                    idx = (idx + 1) % dash_len;
                    dash_on = !dash_on;
                }
                break;
            }

            len = rem;
            idx = (idx + 1) % dash_len;
            dash_on = !dash_on;
            phase = 0.0;
        }
        self.idx = idx;
        self.dash_on = dash_on;
        self.phase = phase;
    }

    /// `Dasher.somethingTo`.
    fn something_to(&mut self, type_: usize) {
        if Dasher::point_curve(&self.cur_curvepts, type_) {
            return;
        }
        let dash_len = self.dash_len;

        self.li.initialize_iteration_on_curve(&self.cur_curvepts, type_);

        let mut idx = self.idx;
        let mut dash_on = self.dash_on;
        let mut phase = self.phase;

        let mut cur_curveoff = 0;
        let mut prev_t = 0.0;
        let mut t;

        let mut left_in_this_dash_segment = self.dash[idx as usize] - phase;

        loop {
            t = self.li.next(left_in_this_dash_segment);
            if !(t < 1.0) {
                break;
            }
            if t != 0.0 {
                helpers::subdivide_at((t - prev_t) / (1.0 - prev_t), &mut self.cur_curvepts, cur_curveoff, 0, type_);
                prev_t = t;
                self.go_to(2, type_, dash_on);
                cur_curveoff = type_;
            }
            idx = (idx + 1) % dash_len;
            dash_on = !dash_on;
            phase = 0.0;
            left_in_this_dash_segment = self.dash[idx as usize];
        }

        self.go_to(cur_curveoff + 2, type_, dash_on);

        phase += self.li.last_seg_len();
        if phase + EPS >= self.dash[idx as usize] {
            phase = 0.0;
            idx = (idx + 1) % dash_len;
            dash_on = !dash_on;
        }
        self.idx = idx;
        self.dash_on = dash_on;
        self.phase = phase;

        self.li.reset();
    }

    /// `Dasher.skipSomethingTo`.
    fn skip_something_to(&mut self, type_: usize) {
        if Dasher::point_curve(&self.cur_curvepts, type_) {
            return;
        }
        self.li.initialize_iteration_on_curve(&self.cur_curvepts, type_);

        let len = self.li.total_length();

        self.outside = true;
        self.total_skip_len += len;

        self.needs_move_to = true;
        self.starting = false;
    }

    /// `Dasher.pointCurve`.
    fn point_curve(curve: &[f64], type_: usize) -> bool {
        for i in 2..type_ {
            if curve[i] != curve[i - 2] {
                return false;
            }
        }
        true
    }

    /// `Dasher._curveTo`.
    fn _curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let monotonizer = self.monotonizer.curve(self.cx0, self.cy0, x1, y1, x2, y2, x3, y3);

        let nb_splits = monotonizer.nb_splits;
        let mid = monotonizer.middle;

        let mut off = 0;
        for _ in 0..=nb_splits {
            self.cur_curvepts[..8].copy_from_slice(&mid[off..off + 8]);
            self.something_to(8);
            off += 6;
        }
    }

    /// `Dasher.skipCurveTo`.
    fn skip_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let c = &mut self.cur_curvepts;
        c[0] = self.cx0;
        c[1] = self.cy0;
        c[2] = x1;
        c[3] = y1;
        c[4] = x2;
        c[5] = y2;
        c[6] = x3;
        c[7] = y3;

        self.skip_something_to(8);

        self.cx0 = x3;
        self.cy0 = y3;
    }

    /// `Dasher._quadTo`.
    fn _quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let monotonizer = self.monotonizer.quad(self.cx0, self.cy0, x1, y1, x2, y2);

        let nb_splits = monotonizer.nb_splits;
        let mid = monotonizer.middle;

        let mut off = 0;
        for _ in 0..=nb_splits {
            self.cur_curvepts[..8].copy_from_slice(&mid[off..off + 8]);
            self.something_to(6);
            off += 4;
        }
    }

    /// `Dasher.skipQuadTo`.
    fn skip_quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let c = &mut self.cur_curvepts;
        c[0] = self.cx0;
        c[1] = self.cy0;
        c[2] = x1;
        c[3] = y1;
        c[4] = x2;
        c[5] = y2;

        self.skip_something_to(6);

        self.cx0 = x2;
        self.cy0 = y2;
    }
}

impl DPathConsumer2D for Dasher<'_> {
    /// `Dasher.moveTo`.
    fn move_to(&mut self, x0: f64, y0: f64) {
        if !self.first_segments_buffer.is_empty() {
            self.out.move_to(self.sx0, self.sy0);
            self.emit_first_segments();
        }
        self.needs_move_to = true;
        self.idx = self.start_idx;
        self.dash_on = self.start_dash_on;
        self.phase = self.start_phase;
        self.cx0 = x0;
        self.cy0 = y0;
        self.sx0 = x0;
        self.sy0 = y0;
        self.starting = true;

        if let Some(clip_rect) = self.clip_rect {
            let outcode = helpers::outcode(x0, y0, &clip_rect);
            self.c_out_code = outcode;
            self.outside = false;
            self.total_skip_len = 0.0;
        }
    }

    /// `Dasher.lineTo`.
    fn line_to(&mut self, x1: f64, y1: f64) {
        let outcode0 = self.c_out_code;

        if let Some(clip_rect) = self.clip_rect {
            let outcode1 = helpers::outcode(x1, y1, &clip_rect);

            let or_code = outcode0 | outcode1;

            if or_code != 0 {
                let side_code = outcode0 & outcode1;

                if side_code == 0 {
                    if self.subdivide {
                        self.subdivide = false;
                        let mut sp = self.curve_splitter.take().expect("CurveClipSplitter");
                        let ret = sp.split_line(self.cx0, self.cy0, x1, y1, or_code, self);
                        self.curve_splitter = Some(sp);
                        self.subdivide = true;
                        if ret {
                            return;
                        }
                    }
                } else {
                    self.c_out_code = outcode1;
                    self.skip_line_to(x1, y1);
                    return;
                }
            }

            self.c_out_code = outcode1;

            if self.outside {
                self.outside = false;
                self.skip_len();
            }
        }
        self._line_to(x1, y1);
    }

    /// `Dasher.curveTo`.
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let outcode0 = self.c_out_code;

        if let Some(clip_rect) = self.clip_rect {
            let outcode1 = helpers::outcode(x1, y1, &clip_rect);
            let outcode2 = helpers::outcode(x2, y2, &clip_rect);
            let outcode3 = helpers::outcode(x3, y3, &clip_rect);

            let or_code = outcode0 | outcode1 | outcode2 | outcode3;
            if or_code != 0 {
                let side_code = outcode0 & outcode1 & outcode2 & outcode3;

                if side_code == 0 {
                    if self.subdivide {
                        self.subdivide = false;
                        let mut sp = self.curve_splitter.take().expect("CurveClipSplitter");
                        let ret = sp.split_curve(self.cx0, self.cy0, x1, y1, x2, y2, x3, y3, or_code, self);
                        self.curve_splitter = Some(sp);
                        self.subdivide = true;
                        if ret {
                            return;
                        }
                    }
                } else {
                    self.c_out_code = outcode3;
                    self.skip_curve_to(x1, y1, x2, y2, x3, y3);
                    return;
                }
            }

            self.c_out_code = outcode3;

            if self.outside {
                self.outside = false;
                self.skip_len();
            }
        }
        self._curve_to(x1, y1, x2, y2, x3, y3);
    }

    /// `Dasher.quadTo`.
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let outcode0 = self.c_out_code;

        if let Some(clip_rect) = self.clip_rect {
            let outcode1 = helpers::outcode(x1, y1, &clip_rect);
            let outcode2 = helpers::outcode(x2, y2, &clip_rect);

            let or_code = outcode0 | outcode1 | outcode2;
            if or_code != 0 {
                let side_code = outcode0 & outcode1 & outcode2;

                if side_code == 0 {
                    if self.subdivide {
                        self.subdivide = false;
                        let mut sp = self.curve_splitter.take().expect("CurveClipSplitter");
                        let ret = sp.split_quad(self.cx0, self.cy0, x1, y1, x2, y2, or_code, self);
                        self.curve_splitter = Some(sp);
                        self.subdivide = true;
                        if ret {
                            return;
                        }
                    }
                } else {
                    self.c_out_code = outcode2;
                    self.skip_quad_to(x1, y1, x2, y2);
                    return;
                }
            }

            self.c_out_code = outcode2;

            if self.outside {
                self.outside = false;
                self.skip_len();
            }
        }
        self._quad_to(x1, y1, x2, y2);
    }

    /// `Dasher.closePath`.
    fn close_path(&mut self) {
        if self.cx0 != self.sx0 || self.cy0 != self.sy0 {
            self.line_to(self.sx0, self.sy0);
        }
        if !self.first_segments_buffer.is_empty() {
            if !self.dash_on || self.needs_move_to {
                self.out.move_to(self.sx0, self.sy0);
            }
            self.emit_first_segments();
        }
        self.move_to(self.sx0, self.sy0);
    }

    /// `Dasher.pathDone`.
    fn path_done(&mut self) {
        if !self.first_segments_buffer.is_empty() {
            self.out.move_to(self.sx0, self.sy0);
            self.emit_first_segments();
        }
        self.out.path_done();

        self.dispose();
    }
}

/// `Dasher.LengthIterator`.
pub struct LengthIterator {
    rec_curve_stack: [[f64; 8]; REC_LIMIT as usize + 1],
    sides_right: [bool; REC_LIMIT as usize],
    curve_type: usize,
    next_t: f64,
    len_at_next_t: f64,
    last_t: f64,
    len_at_last_t: f64,
    len_at_last_split: f64,
    last_seg_len: f64,
    rec_level: i32,
    done: bool,
    cur_leaf_ctrl_poly_lengths: [f64; 3],
    cached_have_low_acceleration: i32,
    next_roots: [f64; 4],
    flat_leaf_coef_cache: [f64; 4],
}

impl LengthIterator {
    pub fn new() -> LengthIterator {
        LengthIterator {
            rec_curve_stack: [[0.0; 8]; REC_LIMIT as usize + 1],
            sides_right: [false; REC_LIMIT as usize],
            curve_type: 0,
            next_t: f64::MAX,
            len_at_next_t: f64::MAX,
            last_t: 0.0,
            len_at_last_t: 0.0,
            len_at_last_split: f64::from_bits(1),
            last_seg_len: f64::MAX,
            rec_level: i32::MIN,
            done: true,
            cur_leaf_ctrl_poly_lengths: [0.0; 3],
            cached_have_low_acceleration: -1,
            next_roots: [0.0; 4],
            flat_leaf_coef_cache: [0.0, 0.0, -1.0, 0.0],
        }
    }

    /// `LengthIterator.reset()`.
    pub fn reset(&mut self) {}

    /// `LengthIterator.initializeIterationOnCurve`.
    pub fn initialize_iteration_on_curve(&mut self, pts: &[f64], type_: usize) {
        self.rec_curve_stack[0].copy_from_slice(&pts[..8]);
        self.curve_type = type_;
        self.rec_level = 0;
        self.last_t = 0.0;
        self.len_at_last_t = 0.0;
        self.next_t = 0.0;
        self.len_at_next_t = 0.0;
        self.go_left();
        self.len_at_last_split = 0.0;
        if self.rec_level > 0 {
            self.sides_right[0] = false;
            self.done = false;
        } else {
            self.sides_right[0] = true;
            self.done = true;
        }
        self.last_seg_len = 0.0;
    }

    /// `LengthIterator.haveLowAcceleration`.
    fn have_low_acceleration(&mut self, err: f64) -> bool {
        if self.cached_have_low_acceleration == -1 {
            let len1 = self.cur_leaf_ctrl_poly_lengths[0];
            let len2 = self.cur_leaf_ctrl_poly_lengths[1];
            if !helpers::within(len1, len2, err * len2) {
                self.cached_have_low_acceleration = 0;
                return false;
            }
            if self.curve_type == 8 {
                let len3 = self.cur_leaf_ctrl_poly_lengths[2];
                let err_len3 = err * len3;
                if !(helpers::within(len2, len3, err_len3) && helpers::within(len1, len3, err_len3)) {
                    self.cached_have_low_acceleration = 0;
                    return false;
                }
            }
            self.cached_have_low_acceleration = 1;
            return true;
        }

        self.cached_have_low_acceleration == 1
    }

    /// `LengthIterator.next(double)`.
    pub fn next(&mut self, len: f64) -> f64 {
        let target_length = self.len_at_last_split + len;
        while self.len_at_next_t < target_length {
            if self.done {
                self.last_seg_len = self.len_at_next_t - self.len_at_last_split;
                return 1.0;
            }
            self.go_to_next_leaf();
        }
        self.len_at_last_split = target_length;
        let leaflen = self.len_at_next_t - self.len_at_last_t;
        let mut t = (target_length - self.len_at_last_t) / leaflen;

        if !self.have_low_acceleration(0.05) {
            let fc = &mut self.flat_leaf_coef_cache;

            if fc[2] < 0.0 {
                let x = self.cur_leaf_ctrl_poly_lengths[0];
                let y = x + self.cur_leaf_ctrl_poly_lengths[1];
                if self.curve_type == 8 {
                    let z = y + self.cur_leaf_ctrl_poly_lengths[2];
                    fc[0] = 3.0 * (x - y) + z;
                    fc[1] = 3.0 * (y - 2.0 * x);
                    fc[2] = 3.0 * x;
                    fc[3] = -z;
                } else if self.curve_type == 6 {
                    fc[0] = 0.0;
                    fc[1] = y - 2.0 * x;
                    fc[2] = 2.0 * x;
                    fc[3] = -y;
                }
            }
            let a = fc[0];
            let b = fc[1];
            let c = fc[2];
            let d = t * fc[3];

            let n = helpers::cubic_roots_in_ab(a, b, c, d, &mut self.next_roots, 0, 0.0, 1.0);
            if n == 1 && !self.next_roots[0].is_nan() {
                t = self.next_roots[0];
            }
        }
        t = t * (self.next_t - self.last_t) + self.last_t;
        if t >= 1.0 {
            t = 1.0;
            self.done = true;
        }
        self.last_seg_len = len;
        t
    }

    /// `LengthIterator.totalLength()`.
    pub fn total_length(&mut self) -> f64 {
        while !self.done {
            self.go_to_next_leaf();
        }
        self.reset();

        self.len_at_next_t
    }

    /// `LengthIterator.lastSegLen()`.
    pub fn last_seg_len(&self) -> f64 {
        self.last_seg_len
    }

    /// `LengthIterator.goToNextLeaf()`.
    fn go_to_next_leaf(&mut self) {
        let mut rec_level = self.rec_level;
        rec_level -= 1;

        while self.sides_right[rec_level as usize] {
            if rec_level == 0 {
                self.rec_level = 0;
                self.done = true;
                return;
            }
            rec_level -= 1;
        }

        self.sides_right[rec_level as usize] = true;
        self.rec_curve_stack[rec_level as usize + 1] = self.rec_curve_stack[rec_level as usize];
        rec_level += 1;
        self.rec_level = rec_level;
        self.go_left();
    }

    /// `LengthIterator.goLeft()`.
    fn go_left(&mut self) {
        let len = self.on_leaf();
        if len >= 0.0 {
            self.last_t = self.next_t;
            self.len_at_last_t = self.len_at_next_t;
            self.next_t += (1i32 << (REC_LIMIT - self.rec_level)) as f64 * MIN_T_INC;
            self.len_at_next_t += len;
            self.flat_leaf_coef_cache[2] = -1.0;
            self.cached_have_low_acceleration = -1;
        } else {
            let lvl = self.rec_level as usize;
            let (lo, hi) = self.rec_curve_stack.split_at_mut(lvl + 1);
            helpers::subdivide(&mut lo[lvl], &mut hi[0], self.curve_type);

            self.sides_right[lvl] = false;
            self.rec_level += 1;
            self.go_left();
        }
    }

    /// `LengthIterator.onLeaf()`.
    fn on_leaf(&mut self) -> f64 {
        let curve = &self.rec_curve_stack[self.rec_level as usize];
        let curve_type = self.curve_type;
        let mut poly_len = 0.0;

        let mut x0 = curve[0];
        let mut y0 = curve[1];
        let mut i = 2;
        while i < curve_type {
            let x1 = curve[i];
            let y1 = curve[i + 1];
            let len = helpers::linelen(x0, y0, x1, y1);
            poly_len += len;
            self.cur_leaf_ctrl_poly_lengths[(i >> 1) - 1] = len;
            x0 = x1;
            y0 = y1;
            i += 2;
        }

        let line_len = helpers::linelen(curve[0], curve[1], x0, y0);

        if (poly_len - line_len) < CURVE_LEN_ERR as f64 || self.rec_level == REC_LIMIT {
            return (poly_len + line_len) / 2.0;
        }
        -1.0
    }
}

impl Default for LengthIterator {
    fn default() -> Self {
        LengthIterator::new()
    }
}
