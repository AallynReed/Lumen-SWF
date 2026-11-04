//! `sun.java2d.marlin.Stroker`.

use std::cell::Cell;
use std::rc::Rc;

use super::helpers::{self, PolyStack};
use super::marlin_const::{CAP_ROUND, CAP_SQUARE, DO_CLIP_SUBDIVIDER, JOIN_MITER, JOIN_ROUND, MIN_SUBPIXELS};
use super::math::ulp;
use super::renderer_context::{self, RendererContext};
use super::transforming_path_consumer2d::{CurveBasicMonotonizer, CurveClipSplitter};
use super::DPathConsumer2D;

const MOVE_TO: i32 = 0;
const DRAWING_OP_TO: i32 = 1;
const CLOSE: i32 = 2;

/// `Stroker.ERR_JOIN`.
const ERR_JOIN: f64 = (1.0f32 / MIN_SUBPIXELS) as f64;
/// `Stroker.ROUND_JOIN_THRESHOLD`.
const ROUND_JOIN_THRESHOLD: f64 = ERR_JOIN * ERR_JOIN;

/// `Stroker.C`.
fn c() -> f64 {
    (4.0 * (2.0f64.sqrt() - 1.0)) / 3.0
}

/// `Stroker.SQRT_2`.
fn sqrt_2() -> f64 {
    2.0f64.sqrt()
}

/// `Stroker`.
pub struct Stroker<'a> {
    out: Box<dyn DPathConsumer2D + 'a>,
    cap_style: i32,
    join_style: i32,
    line_width2: f64,
    inv_half_line_width2_sq: f64,
    offset0: [f64; 2],
    offset1: [f64; 2],
    offset2: [f64; 2],
    miter: [f64; 2],
    miter_limit_sq: f64,
    prev: i32,
    sx0: f64,
    sy0: f64,
    sdx: f64,
    sdy: f64,
    cx0: f64,
    cy0: f64,
    cdx: f64,
    cdy: f64,
    smx: f64,
    smy: f64,
    cmx: f64,
    cmy: f64,
    reverse: PolyStack,
    lp: [f64; 8],
    rp: [f64; 8],
    clip_rect: Option<[f64; 4]>,
    c_out_code: i32,
    s_out_code: i32,
    opened: bool,
    cap_start: bool,
    monotonize: bool,
    subdivide: bool,
    curve_splitter: Option<Box<CurveClipSplitter>>,
    /// `RendererContext.monotonizer`.
    monotonizer: CurveBasicMonotonizer,
    /// `RendererContext.closedPath`.
    closed_path: Rc<Cell<bool>>,
}

impl<'a> Stroker<'a> {
    /// `Stroker.init(DPathConsumer2D, double, int, int, double, boolean)`; pads `rdr_ctx.clip_rect` and initializes
    /// `curve_splitter` (the context's `CurveClipSplitter`) when clipping.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rdr_ctx: &mut RendererContext,
        pc2d: Box<dyn DPathConsumer2D + 'a>,
        line_width: f64,
        cap_style: i32,
        join_style: i32,
        miterlimit: f64,
        subdivide_curves: bool,
        curve_splitter: &mut CurveClipSplitter,
    ) -> Stroker<'a> {
        let line_width2 = line_width / 2.0;
        let limit = miterlimit * line_width2;
        let carried = renderer_context::carried_state();
        let mut s = Stroker {
            out: pc2d,
            cap_style,
            join_style,
            line_width2,
            inv_half_line_width2_sq: 1.0 / (2.0 * line_width2 * line_width2),
            offset0: [0.0; 2],
            offset1: [0.0; 2],
            offset2: [0.0; 2],
            miter: [0.0; 2],
            miter_limit_sq: limit * limit,
            prev: CLOSE,
            sx0: 0.0,
            sy0: 0.0,
            sdx: 0.0,
            sdy: 0.0,
            cx0: 0.0,
            cy0: 0.0,
            cdx: 0.0,
            cdy: 0.0,
            smx: carried.stroker_smx,
            smy: carried.stroker_smy,
            cmx: 0.0,
            cmy: 0.0,
            reverse: PolyStack::new(),
            lp: [0.0; 8],
            rp: [0.0; 8],
            clip_rect: None,
            c_out_code: 0,
            s_out_code: 0,
            opened: false,
            cap_start: false,
            monotonize: subdivide_curves,
            subdivide: false,
            curve_splitter: None,
            monotonizer: CurveBasicMonotonizer::new(),
            closed_path: rdr_ctx.closed_path.clone(),
        };
        s.monotonizer.init(line_width);

        if rdr_ctx.do_clip {
            let mut margin = line_width2;

            if cap_style == CAP_SQUARE {
                margin *= sqrt_2();
            }
            if join_style == JOIN_MITER && margin < limit {
                margin = limit;
            }

            let clip_rect = &mut rdr_ctx.clip_rect;
            clip_rect[0] -= margin;
            clip_rect[1] += margin;
            clip_rect[2] -= margin;
            clip_rect[3] += margin;
            s.clip_rect = Some(*clip_rect);

            if DO_CLIP_SUBDIVIDER {
                s.subdivide = subdivide_curves;
                curve_splitter.init(*clip_rect, rdr_ctx.clip_inv_scale);
            } else {
                s.subdivide = false;
            }
        }
        s
    }

    /// Hands this stroker the context's `CurveClipSplitter` (used while clipping).
    pub fn set_curve_splitter(&mut self, curve_splitter: Box<CurveClipSplitter>) {
        self.curve_splitter = Some(curve_splitter);
    }

    /// `Stroker.disableClipping()`.
    pub fn disable_clipping(&mut self) {
        self.clip_rect = None;
        self.c_out_code = 0;
        self.s_out_code = 0;
    }

    /// `Stroker.dispose()`.
    fn dispose(&mut self) {
        self.reverse.dispose();
        self.opened = false;
        self.cap_start = false;
    }

    /// `Stroker.computeOffset`.
    fn compute_offset(lx: f64, ly: f64, w: f64, m: &mut [f64; 2]) {
        let mut len = lx * lx + ly * ly;
        if len == 0.0 {
            m[0] = 0.0;
            m[1] = 0.0;
        } else {
            len = len.sqrt();
            m[0] = (ly * w) / len;
            m[1] = -(lx * w) / len;
        }
    }

    /// `Stroker.isCW`.
    fn is_cw(dx1: f64, dy1: f64, dx2: f64, dy2: f64) -> bool {
        dx1 * dy2 <= dy1 * dx2
    }

    /// `Stroker.mayDrawRoundJoin`.
    #[allow(clippy::too_many_arguments)]
    fn may_draw_round_join(&mut self, cx: f64, cy: f64, mut omx: f64, mut omy: f64, mut mx: f64, mut my: f64, rev: bool) {
        if (omx == 0.0 && omy == 0.0) || (mx == 0.0 && my == 0.0) {
            return;
        }

        let domx = omx - mx;
        let domy = omy - my;
        let len_sq = domx * domx + domy * domy;

        if len_sq < ROUND_JOIN_THRESHOLD {
            return;
        }

        if rev {
            omx = -omx;
            omy = -omy;
            mx = -mx;
            my = -my;
        }
        self.draw_round_join(cx, cy, omx, omy, mx, my, rev);
    }

    /// `Stroker.drawRoundJoin`.
    #[allow(clippy::too_many_arguments)]
    fn draw_round_join(&mut self, x: f64, y: f64, omx: f64, omy: f64, mx: f64, my: f64, rev: bool) {
        let cosext = omx * mx + omy * my;
        if cosext >= 0.0 {
            self.draw_bez_approx_for_arc(x, y, omx, omy, mx, my, rev);
        } else {
            let nx = my - omy;
            let ny = omx - mx;
            let nlen = (nx * nx + ny * ny).sqrt();
            let scale = self.line_width2 / nlen;
            let mut mmx = nx * scale;
            let mut mmy = ny * scale;

            if rev {
                mmx = -mmx;
                mmy = -mmy;
            }
            self.draw_bez_approx_for_arc(x, y, omx, omy, mmx, mmy, rev);
            self.draw_bez_approx_for_arc(x, y, mmx, mmy, mx, my, rev);
        }
    }

    /// `Stroker.drawBezApproxForArc`.
    #[allow(clippy::too_many_arguments)]
    fn draw_bez_approx_for_arc(&mut self, cx: f64, cy: f64, omx: f64, omy: f64, mx: f64, my: f64, rev: bool) {
        let cosext2 = (omx * mx + omy * my) * self.inv_half_line_width2_sq;

        if cosext2 >= 0.5 {
            return;
        }

        let mut cv = ((4.0 / 3.0) * (0.5 - cosext2).sqrt()) / (1.0 + (cosext2 + 0.5).sqrt());
        if rev {
            cv = -cv;
        }
        let x1 = cx + omx;
        let y1 = cy + omy;
        let x2 = x1 - cv * omy;
        let y2 = y1 + cv * omx;

        let x4 = cx + mx;
        let y4 = cy + my;
        let x3 = x4 + cv * my;
        let y3 = y4 - cv * mx;

        self.emit_curve_to_rev_or(x1, y1, x2, y2, x3, y3, x4, y4, rev);
    }

    /// `Stroker.drawRoundCap`.
    fn draw_round_cap(&mut self, cx: f64, cy: f64, mx: f64, my: f64) {
        let c = c();
        let cmx = c * mx;
        let cmy = c * my;
        self.emit_curve_to(cx + mx - cmy, cy + my + cmx, cx - my + cmx, cy + mx + cmy, cx - my, cy + mx);
        self.emit_curve_to(cx - my - cmx, cy + mx - cmy, cx - mx - cmy, cy - my + cmx, cx - mx, cy - my);
    }

    /// `Stroker.computeMiter`.
    #[allow(clippy::too_many_arguments)]
    fn compute_miter(x0: f64, y0: f64, x1: f64, y1: f64, x0p: f64, y0p: f64, x1p: f64, y1p: f64, m: &mut [f64; 2]) {
        let x10 = x1 - x0;
        let y10 = y1 - y0;
        let x10p = x1p - x0p;
        let y10p = y1p - y0p;

        let den = x10 * y10p - x10p * y10;
        let mut t = x10p * (y0 - y0p) - y10p * (x0 - x0p);
        t /= den;
        m[0] = x0 + t * x10;
        m[1] = y0 + t * y10;
    }

    /// `Stroker.safeComputeMiter` (writes `m[2]`, `m[3]`).
    #[allow(clippy::too_many_arguments)]
    fn safe_compute_miter(x0: f64, y0: f64, x1: f64, y1: f64, x0p: f64, y0p: f64, x1p: f64, y1p: f64, m: &mut [f64; 8]) {
        let x10 = x1 - x0;
        let y10 = y1 - y0;
        let x10p = x1p - x0p;
        let y10p = y1p - y0p;

        let den = x10 * y10p - x10p * y10;
        if den == 0.0 {
            m[2] = (x0 + x0p) / 2.0;
            m[3] = (y0 + y0p) / 2.0;
        } else {
            let mut t = x10p * (y0 - y0p) - y10p * (x0 - x0p);
            t /= den;
            m[2] = x0 + t * x10;
            m[3] = y0 + t * y10;
        }
    }

    /// `Stroker.drawMiter`.
    #[allow(clippy::too_many_arguments)]
    fn draw_miter(
        &mut self,
        pdx: f64,
        pdy: f64,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        mut omx: f64,
        mut omy: f64,
        mut mx: f64,
        mut my: f64,
        rev: bool,
    ) {
        if (mx == omx && my == omy) || (pdx == 0.0 && pdy == 0.0) || (dx == 0.0 && dy == 0.0) {
            return;
        }

        if rev {
            omx = -omx;
            omy = -omy;
            mx = -mx;
            my = -my;
        }

        Stroker::compute_miter(
            (x0 - pdx) + omx,
            (y0 - pdy) + omy,
            x0 + omx,
            y0 + omy,
            (dx + x0) + mx,
            (dy + y0) + my,
            x0 + mx,
            y0 + my,
            &mut self.miter,
        );

        let miter_x = self.miter[0];
        let miter_y = self.miter[1];
        let len_sq = (miter_x - x0) * (miter_x - x0) + (miter_y - y0) * (miter_y - y0);

        if len_sq < self.miter_limit_sq {
            self.emit_line_to_rev_or(miter_x, miter_y, rev);
        }
    }

    /// `Stroker._moveTo`.
    fn _move_to(&mut self, x0: f64, y0: f64, outcode: i32) {
        if self.prev == MOVE_TO {
            self.cx0 = x0;
            self.cy0 = y0;
        } else {
            if self.prev == DRAWING_OP_TO {
                self.finish(outcode);
            }
            self.prev = MOVE_TO;
            self.cx0 = x0;
            self.cy0 = y0;
            self.cdx = 1.0;
            self.cdy = 0.0;
        }
    }

    /// `Stroker.lineTo(double, double, boolean)`.
    fn line_to_force(&mut self, x1: f64, y1: f64, force: bool) {
        let outcode0 = self.c_out_code;

        if !force {
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
                        self._move_to(x1, y1, outcode0);
                        self.opened = true;
                        return;
                    }
                }

                self.c_out_code = outcode1;
            }
        }

        let mut dx = x1 - self.cx0;
        let dy = y1 - self.cy0;
        if dx == 0.0 && dy == 0.0 {
            dx = 1.0;
        }
        Stroker::compute_offset(dx, dy, self.line_width2, &mut self.offset0);
        let mx = self.offset0[0];
        let my = self.offset0[1];

        self.draw_join(self.cdx, self.cdy, self.cx0, self.cy0, dx, dy, self.cmx, self.cmy, mx, my, outcode0);

        self.emit_line_to(self.cx0 + mx, self.cy0 + my);
        self.emit_line_to(x1 + mx, y1 + my);

        self.emit_line_to_rev(self.cx0 - mx, self.cy0 - my);
        self.emit_line_to_rev(x1 - mx, y1 - my);

        self.prev = DRAWING_OP_TO;
        self.cx0 = x1;
        self.cy0 = y1;
        self.cdx = dx;
        self.cdy = dy;
        self.cmx = mx;
        self.cmy = my;
    }

    /// `Stroker.emitReverse()`.
    fn emit_reverse(&mut self) {
        self.reverse.pop_all(&mut *self.out);
    }

    /// `Stroker.finish(int)`.
    fn finish(&mut self, outcode: i32) {
        if self.closed_path.get() {
            self.emit_reverse();
        } else {
            if outcode == 0 {
                if self.cap_style == CAP_ROUND {
                    self.draw_round_cap(self.cx0, self.cy0, self.cmx, self.cmy);
                } else if self.cap_style == CAP_SQUARE {
                    self.emit_line_to(self.cx0 - self.cmy + self.cmx, self.cy0 + self.cmx + self.cmy);
                    self.emit_line_to(self.cx0 - self.cmy - self.cmx, self.cy0 + self.cmx - self.cmy);
                }
            }
            self.emit_reverse();

            if !self.cap_start {
                self.cap_start = true;

                if self.s_out_code == 0 {
                    if self.cap_style == CAP_ROUND {
                        self.draw_round_cap(self.sx0, self.sy0, -self.smx, -self.smy);
                    } else if self.cap_style == CAP_SQUARE {
                        self.emit_line_to(self.sx0 + self.smy - self.smx, self.sy0 - self.smx - self.smy);
                        self.emit_line_to(self.sx0 + self.smy + self.smx, self.sy0 - self.smx + self.smy);
                    }
                }
            }
        }
        self.emit_close();
    }

    /// `Stroker.emitMoveTo`.
    fn emit_move_to(&mut self, x0: f64, y0: f64) {
        self.out.move_to(x0, y0);
    }

    /// `Stroker.emitLineTo(double, double)`.
    fn emit_line_to(&mut self, x1: f64, y1: f64) {
        self.out.line_to(x1, y1);
    }

    /// `Stroker.emitLineToRev`.
    fn emit_line_to_rev(&mut self, x1: f64, y1: f64) {
        self.reverse.push_line(x1, y1);
    }

    /// `Stroker.emitLineTo(double, double, boolean)`.
    fn emit_line_to_rev_or(&mut self, x1: f64, y1: f64, rev: bool) {
        if rev {
            self.emit_line_to_rev(x1, y1);
        } else {
            self.emit_line_to(x1, y1);
        }
    }

    /// `Stroker.emitQuadTo`.
    fn emit_quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.out.quad_to(x1, y1, x2, y2);
    }

    /// `Stroker.emitQuadToRev`.
    fn emit_quad_to_rev(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        self.reverse.push_quad(x0, y0, x1, y1);
    }

    /// `Stroker.emitCurveTo(double x6)`.
    fn emit_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.out.curve_to(x1, y1, x2, y2, x3, y3);
    }

    /// `Stroker.emitCurveToRev`.
    fn emit_curve_to_rev(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.reverse.push_cubic(x0, y0, x1, y1, x2, y2);
    }

    /// `Stroker.emitCurveTo(double x8, boolean)`.
    #[allow(clippy::too_many_arguments)]
    fn emit_curve_to_rev_or(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, rev: bool) {
        if rev {
            self.reverse.push_cubic(x0, y0, x1, y1, x2, y2);
        } else {
            self.out.curve_to(x1, y1, x2, y2, x3, y3);
        }
    }

    /// `Stroker.emitClose`.
    fn emit_close(&mut self) {
        self.out.close_path();
    }

    /// `Stroker.drawJoin`.
    #[allow(clippy::too_many_arguments)]
    fn draw_join(
        &mut self,
        pdx: f64,
        pdy: f64,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        omx: f64,
        omy: f64,
        mx: f64,
        my: f64,
        outcode: i32,
    ) {
        if self.prev != DRAWING_OP_TO {
            self.emit_move_to(x0 + mx, y0 + my);
            if !self.opened {
                self.sdx = dx;
                self.sdy = dy;
                self.smx = mx;
                self.smy = my;
            }
        } else {
            let cw = Stroker::is_cw(pdx, pdy, dx, dy);
            if outcode == 0 {
                if self.join_style == JOIN_MITER {
                    self.draw_miter(pdx, pdy, x0, y0, dx, dy, omx, omy, mx, my, cw);
                } else if self.join_style == JOIN_ROUND {
                    self.may_draw_round_join(x0, y0, omx, omy, mx, my, cw);
                }
            }
            self.emit_line_to_rev_or(x0, y0, !cw);
        }
        self.prev = DRAWING_OP_TO;
    }

    /// `Stroker.within(double x4, double)`.
    fn within(x1: f64, y1: f64, x2: f64, y2: f64, err: f64) -> bool {
        helpers::within(x1, x2, err) && helpers::within(y1, y2, err)
    }

    /// `Stroker.getLineOffsets`.
    fn get_line_offsets(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, left: &mut [f64; 8], right: &mut [f64; 8]) {
        Stroker::compute_offset(x2 - x1, y2 - y1, self.line_width2, &mut self.offset0);
        let mx = self.offset0[0];
        let my = self.offset0[1];
        left[0] = x1 + mx;
        left[1] = y1 + my;
        left[2] = x2 + mx;
        left[3] = y2 + my;

        right[0] = x1 - mx;
        right[1] = y1 - my;
        right[2] = x2 - mx;
        right[3] = y2 - my;
    }

    /// `Stroker.computeOffsetCubic`.
    fn compute_offset_cubic(&mut self, pts: &[f64], off: usize, left_off: &mut [f64; 8], right_off: &mut [f64; 8]) -> usize {
        let x1 = pts[off];
        let y1 = pts[off + 1];
        let x2 = pts[off + 2];
        let y2 = pts[off + 3];
        let x3 = pts[off + 4];
        let y3 = pts[off + 5];
        let x4 = pts[off + 6];
        let y4 = pts[off + 7];

        let mut dx4 = x4 - x3;
        let mut dy4 = y4 - y3;
        let mut dx1 = x2 - x1;
        let mut dy1 = y2 - y1;

        let p1eqp2 = Stroker::within(x1, y1, x2, y2, 6.0 * ulp(y2));
        let p3eqp4 = Stroker::within(x3, y3, x4, y4, 6.0 * ulp(y4));

        if p1eqp2 && p3eqp4 {
            self.get_line_offsets(x1, y1, x4, y4, left_off, right_off);
            return 4;
        } else if p1eqp2 {
            dx1 = x3 - x1;
            dy1 = y3 - y1;
        } else if p3eqp4 {
            dx4 = x4 - x2;
            dy4 = y4 - y2;
        }

        let mut dotsq = dx1 * dx4 + dy1 * dy4;
        dotsq *= dotsq;
        let l1sq = dx1 * dx1 + dy1 * dy1;
        let l4sq = dx4 * dx4 + dy4 * dy4;

        if helpers::within(dotsq, l1sq * l4sq, 4.0 * ulp(dotsq)) {
            self.get_line_offsets(x1, y1, x4, y4, left_off, right_off);
            return 4;
        }

        let x = (x1 + 3.0 * (x2 + x3) + x4) / 8.0;
        let y = (y1 + 3.0 * (y2 + y3) + y4) / 8.0;
        let dxm = x3 + x4 - x1 - x2;
        let dym = y3 + y4 - y1 - y2;

        Stroker::compute_offset(dx1, dy1, self.line_width2, &mut self.offset0);
        Stroker::compute_offset(dxm, dym, self.line_width2, &mut self.offset1);
        Stroker::compute_offset(dx4, dy4, self.line_width2, &mut self.offset2);
        let mut x1p = x1 + self.offset0[0];
        let mut y1p = y1 + self.offset0[1];
        let mut xi = x + self.offset1[0];
        let mut yi = y + self.offset1[1];
        let mut x4p = x4 + self.offset2[0];
        let mut y4p = y4 + self.offset2[1];

        let invdet43 = 4.0 / (3.0 * (dx1 * dy4 - dy1 * dx4));

        let mut two_pi_m_p1_m_p4x = 2.0 * xi - x1p - x4p;
        let mut two_pi_m_p1_m_p4y = 2.0 * yi - y1p - y4p;
        let mut c1 = invdet43 * (dy4 * two_pi_m_p1_m_p4x - dx4 * two_pi_m_p1_m_p4y);
        let mut c2 = invdet43 * (dx1 * two_pi_m_p1_m_p4y - dy1 * two_pi_m_p1_m_p4x);

        let mut x2p = x1p + c1 * dx1;
        let mut y2p = y1p + c1 * dy1;
        let mut x3p = x4p + c2 * dx4;
        let mut y3p = y4p + c2 * dy4;

        left_off[0] = x1p;
        left_off[1] = y1p;
        left_off[2] = x2p;
        left_off[3] = y2p;
        left_off[4] = x3p;
        left_off[5] = y3p;
        left_off[6] = x4p;
        left_off[7] = y4p;

        x1p = x1 - self.offset0[0];
        y1p = y1 - self.offset0[1];
        xi -= 2.0 * self.offset1[0];
        yi -= 2.0 * self.offset1[1];
        x4p = x4 - self.offset2[0];
        y4p = y4 - self.offset2[1];

        two_pi_m_p1_m_p4x = 2.0 * xi - x1p - x4p;
        two_pi_m_p1_m_p4y = 2.0 * yi - y1p - y4p;
        c1 = invdet43 * (dy4 * two_pi_m_p1_m_p4x - dx4 * two_pi_m_p1_m_p4y);
        c2 = invdet43 * (dx1 * two_pi_m_p1_m_p4y - dy1 * two_pi_m_p1_m_p4x);

        x2p = x1p + c1 * dx1;
        y2p = y1p + c1 * dy1;
        x3p = x4p + c2 * dx4;
        y3p = y4p + c2 * dy4;

        right_off[0] = x1p;
        right_off[1] = y1p;
        right_off[2] = x2p;
        right_off[3] = y2p;
        right_off[4] = x3p;
        right_off[5] = y3p;
        right_off[6] = x4p;
        right_off[7] = y4p;

        8
    }

    /// `Stroker.computeOffsetQuad`.
    fn compute_offset_quad(&mut self, pts: &[f64], off: usize, left_off: &mut [f64; 8], right_off: &mut [f64; 8]) -> usize {
        let x1 = pts[off];
        let y1 = pts[off + 1];
        let x2 = pts[off + 2];
        let y2 = pts[off + 3];
        let x3 = pts[off + 4];
        let y3 = pts[off + 5];

        let dx3 = x3 - x2;
        let dy3 = y3 - y2;
        let dx1 = x2 - x1;
        let dy1 = y2 - y1;

        let p1eqp2 = Stroker::within(x1, y1, x2, y2, 6.0 * ulp(y2));
        let p2eqp3 = Stroker::within(x2, y2, x3, y3, 6.0 * ulp(y3));

        if p1eqp2 || p2eqp3 {
            self.get_line_offsets(x1, y1, x3, y3, left_off, right_off);
            return 4;
        }

        let mut dotsq = dx1 * dx3 + dy1 * dy3;
        dotsq *= dotsq;
        let l1sq = dx1 * dx1 + dy1 * dy1;
        let l3sq = dx3 * dx3 + dy3 * dy3;

        if helpers::within(dotsq, l1sq * l3sq, 4.0 * ulp(dotsq)) {
            self.get_line_offsets(x1, y1, x3, y3, left_off, right_off);
            return 4;
        }

        Stroker::compute_offset(dx1, dy1, self.line_width2, &mut self.offset0);
        Stroker::compute_offset(dx3, dy3, self.line_width2, &mut self.offset1);

        let mut x1p = x1 + self.offset0[0];
        let mut y1p = y1 + self.offset0[1];
        let mut x3p = x3 + self.offset1[0];
        let mut y3p = y3 + self.offset1[1];
        Stroker::safe_compute_miter(x1p, y1p, x1p + dx1, y1p + dy1, x3p, y3p, x3p - dx3, y3p - dy3, left_off);
        left_off[0] = x1p;
        left_off[1] = y1p;
        left_off[4] = x3p;
        left_off[5] = y3p;

        x1p = x1 - self.offset0[0];
        y1p = y1 - self.offset0[1];
        x3p = x3 - self.offset1[0];
        y3p = y3 - self.offset1[1];
        Stroker::safe_compute_miter(x1p, y1p, x1p + dx1, y1p + dy1, x3p, y3p, x3p - dx3, y3p - dy3, right_off);
        right_off[0] = x1p;
        right_off[1] = y1p;
        right_off[4] = x3p;
        right_off[5] = y3p;
        6
    }

    /// `Stroker._curveTo`.
    #[allow(clippy::too_many_arguments)]
    fn _curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, outcode0: i32) {
        let mut dxs = x1 - self.cx0;
        let mut dys = y1 - self.cy0;
        let mut dxf = x3 - x2;
        let mut dyf = y3 - y2;

        if dxs == 0.0 && dys == 0.0 {
            dxs = x2 - self.cx0;
            dys = y2 - self.cy0;
            if dxs == 0.0 && dys == 0.0 {
                dxs = x3 - self.cx0;
                dys = y3 - self.cy0;
            }
        }
        if dxf == 0.0 && dyf == 0.0 {
            dxf = x3 - x1;
            dyf = y3 - y1;
            if dxf == 0.0 && dyf == 0.0 {
                dxf = x3 - self.cx0;
                dyf = y3 - self.cy0;
            }
        }
        if dxs == 0.0 && dys == 0.0 {
            if self.clip_rect.is_some() {
                self.c_out_code = outcode0;
            }
            self.line_to(self.cx0, self.cy0);
            return;
        }

        if dxs.abs() < 0.1 && dys.abs() < 0.1 {
            let len = (dxs * dxs + dys * dys).sqrt();
            dxs /= len;
            dys /= len;
        }
        if dxf.abs() < 0.1 && dyf.abs() < 0.1 {
            let len = (dxf * dxf + dyf * dyf).sqrt();
            dxf /= len;
            dyf /= len;
        }

        Stroker::compute_offset(dxs, dys, self.line_width2, &mut self.offset0);
        self.draw_join(self.cdx, self.cdy, self.cx0, self.cy0, dxs, dys, self.cmx, self.cmy, self.offset0[0], self.offset0[1], outcode0);

        let mut n_splits = 0;
        let mut mid = [0.0f64; 68];
        if self.monotonize {
            let m = self.monotonizer.curve(self.cx0, self.cy0, x1, y1, x2, y2, x3, y3);
            n_splits = m.nb_splits;
            mid = m.middle;
        } else {
            mid[0] = self.cx0;
            mid[1] = self.cy0;
            mid[2] = x1;
            mid[3] = y1;
            mid[4] = x2;
            mid[5] = y2;
            mid[6] = x3;
            mid[7] = y3;
        }
        let mut l = self.lp;
        let mut r = self.rp;

        let mut kind = 0;
        let mut off = 0;
        for _ in 0..=n_splits {
            kind = self.compute_offset_cubic(&mid, off, &mut l, &mut r);

            self.emit_line_to(l[0], l[1]);

            match kind {
                8 => {
                    self.emit_curve_to(l[2], l[3], l[4], l[5], l[6], l[7]);
                    self.emit_curve_to_rev(r[0], r[1], r[2], r[3], r[4], r[5]);
                }
                4 => {
                    self.emit_line_to(l[2], l[3]);
                    self.emit_line_to_rev(r[0], r[1]);
                }
                _ => {}
            }
            self.emit_line_to_rev(r[kind - 2], r[kind - 1]);
            off += 6;
        }
        self.lp = l;
        self.rp = r;

        self.prev = DRAWING_OP_TO;
        self.cx0 = x3;
        self.cy0 = y3;
        self.cdx = dxf;
        self.cdy = dyf;
        self.cmx = (l[kind - 2] - r[kind - 2]) / 2.0;
        self.cmy = (l[kind - 1] - r[kind - 1]) / 2.0;
    }

    /// `Stroker._quadTo`.
    fn _quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, outcode0: i32) {
        let mut dxs = x1 - self.cx0;
        let mut dys = y1 - self.cy0;
        let mut dxf = x2 - x1;
        let mut dyf = y2 - y1;

        if (dxs == 0.0 && dys == 0.0) || (dxf == 0.0 && dyf == 0.0) {
            dxs = x2 - self.cx0;
            dxf = dxs;
            dys = y2 - self.cy0;
            dyf = dys;
        }
        if dxs == 0.0 && dys == 0.0 {
            if self.clip_rect.is_some() {
                self.c_out_code = outcode0;
            }
            self.line_to(self.cx0, self.cy0);
            return;
        }
        if dxs.abs() < 0.1 && dys.abs() < 0.1 {
            let len = (dxs * dxs + dys * dys).sqrt();
            dxs /= len;
            dys /= len;
        }
        if dxf.abs() < 0.1 && dyf.abs() < 0.1 {
            let len = (dxf * dxf + dyf * dyf).sqrt();
            dxf /= len;
            dyf /= len;
        }
        Stroker::compute_offset(dxs, dys, self.line_width2, &mut self.offset0);
        self.draw_join(self.cdx, self.cdy, self.cx0, self.cy0, dxs, dys, self.cmx, self.cmy, self.offset0[0], self.offset0[1], outcode0);

        let mut n_splits = 0;
        let mut mid = [0.0f64; 68];
        if self.monotonize {
            let m = self.monotonizer.quad(self.cx0, self.cy0, x1, y1, x2, y2);
            n_splits = m.nb_splits;
            mid = m.middle;
        } else {
            mid[0] = self.cx0;
            mid[1] = self.cy0;
            mid[2] = x1;
            mid[3] = y1;
            mid[4] = x2;
            mid[5] = y2;
        }
        let mut l = self.lp;
        let mut r = self.rp;

        let mut kind = 0;
        let mut off = 0;
        for _ in 0..=n_splits {
            kind = self.compute_offset_quad(&mid, off, &mut l, &mut r);

            self.emit_line_to(l[0], l[1]);

            match kind {
                6 => {
                    self.emit_quad_to(l[2], l[3], l[4], l[5]);
                    self.emit_quad_to_rev(r[0], r[1], r[2], r[3]);
                }
                4 => {
                    self.emit_line_to(l[2], l[3]);
                    self.emit_line_to_rev(r[0], r[1]);
                }
                _ => {}
            }
            self.emit_line_to_rev(r[kind - 2], r[kind - 1]);
            off += 4;
        }
        self.lp = l;
        self.rp = r;

        self.prev = DRAWING_OP_TO;
        self.cx0 = x2;
        self.cy0 = y2;
        self.cdx = dxf;
        self.cdy = dyf;
        self.cmx = (l[kind - 2] - r[kind - 2]) / 2.0;
        self.cmy = (l[kind - 1] - r[kind - 1]) / 2.0;
    }
}

impl Drop for Stroker<'_> {
    fn drop(&mut self) {
        let (smx, smy) = (self.smx, self.smy);
        renderer_context::update_carried_state(|c| {
            c.stroker_smx = smx;
            c.stroker_smy = smy;
        });
    }
}

impl DPathConsumer2D for Stroker<'_> {
    /// `Stroker.moveTo`.
    fn move_to(&mut self, x0: f64, y0: f64) {
        self._move_to(x0, y0, self.c_out_code);
        self.sx0 = x0;
        self.sy0 = y0;
        self.sdx = 1.0;
        self.sdy = 0.0;
        self.opened = false;
        self.cap_start = false;

        if let Some(clip_rect) = self.clip_rect {
            let outcode = helpers::outcode(x0, y0, &clip_rect);
            self.c_out_code = outcode;
            self.s_out_code = outcode;
        }
    }

    /// `Stroker.lineTo(double, double)`.
    fn line_to(&mut self, x1: f64, y1: f64) {
        self.line_to_force(x1, y1, false);
    }

    /// `Stroker.closePath`.
    fn close_path(&mut self) {
        if self.prev != DRAWING_OP_TO && !self.opened {
            if self.prev == CLOSE {
                return;
            }
            self.emit_move_to(self.cx0, self.cy0 - self.line_width2);

            self.sdx = 1.0;
            self.sdy = 0.0;
            self.cdx = 1.0;
            self.cdy = 0.0;

            self.smx = 0.0;
            self.smy = -self.line_width2;
            self.cmx = 0.0;
            self.cmy = -self.line_width2;

            self.finish(self.c_out_code);
            return;
        }

        if (self.s_out_code & self.c_out_code) == 0 {
            if self.cx0 != self.sx0 || self.cy0 != self.sy0 {
                self.line_to_force(self.sx0, self.sy0, true);
            }

            self.draw_join(self.cdx, self.cdy, self.cx0, self.cy0, self.sdx, self.sdy, self.cmx, self.cmy, self.smx, self.smy, self.s_out_code);

            self.emit_line_to(self.sx0 + self.smx, self.sy0 + self.smy);

            if self.opened {
                self.emit_line_to(self.sx0 - self.smx, self.sy0 - self.smy);
            } else {
                self.emit_move_to(self.sx0 - self.smx, self.sy0 - self.smy);
            }
        }
        self.emit_reverse();

        self.prev = CLOSE;
        self.cx0 = self.sx0;
        self.cy0 = self.sy0;
        self.c_out_code = self.s_out_code;

        if self.opened {
            self.opened = false;
        } else {
            self.emit_close();
        }
    }

    /// `Stroker.pathDone`.
    fn path_done(&mut self) {
        if self.prev == DRAWING_OP_TO {
            self.finish(self.c_out_code);
        }

        self.out.path_done();

        self.prev = CLOSE;

        self.dispose();
    }

    /// `Stroker.curveTo`.
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
                    self._move_to(x3, y3, outcode0);
                    self.opened = true;
                    return;
                }
            }

            self.c_out_code = outcode3;
        }
        self._curve_to(x1, y1, x2, y2, x3, y3, outcode0);
    }

    /// `Stroker.quadTo`.
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
                    self._move_to(x2, y2, outcode0);
                    self.opened = true;
                    return;
                }
            }

            self.c_out_code = outcode2;
        }
        self._quad_to(x1, y1, x2, y2, outcode0);
    }
}
