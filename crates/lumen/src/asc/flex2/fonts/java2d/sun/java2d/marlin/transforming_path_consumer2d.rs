//! The `sun.java2d.marlin.TransformingPathConsumer2D` filters the stroking pipeline uses.

use std::cell::Cell;
use std::rc::Rc;

use super::curve::Curve;
use super::helpers::{self, PolyStack};
use super::marlin_const::SUBDIVIDER_MIN_LENGTH;
use super::renderer_context::RendererContext;
use super::DPathConsumer2D;
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;

/// `TransformingPathConsumer2D.deltaTransformConsumer`.
pub fn delta_transform_consumer<'a>(
    rdr_ctx: &mut RendererContext,
    out: Box<dyn DPathConsumer2D + 'a>,
    at: Option<&AffineTransform>,
) -> Box<dyn DPathConsumer2D + 'a> {
    let at = match at {
        None => return out,
        Some(at) => at,
    };
    let mxx = at.get_scale_x();
    let mxy = at.get_shear_x();
    let myx = at.get_shear_y();
    let myy = at.get_scale_y();

    if mxy == 0.0 && myx == 0.0 {
        if mxx == 1.0 && myy == 1.0 {
            out
        } else {
            if rdr_ctx.do_clip {
                rdr_ctx.clip_inv_scale = adjust_clip_scale(&mut rdr_ctx.clip_rect, mxx, myy);
            }
            Box::new(DeltaScaleFilter { out, sx: mxx, sy: myy })
        }
    } else {
        if rdr_ctx.do_clip {
            rdr_ctx.clip_inv_scale = adjust_clip_inverse_delta(&mut rdr_ctx.clip_rect, mxx, mxy, myx, myy);
        }
        Box::new(DeltaTransformFilter { out, mxx, mxy, myx, myy })
    }
}

/// `TransformingPathConsumer2D.adjustClipScale`.
fn adjust_clip_scale(clip_rect: &mut [f64; 4], mxx: f64, myy: f64) -> f64 {
    let scale_y = 1.0 / myy;
    clip_rect[0] *= scale_y;
    clip_rect[1] *= scale_y;

    if clip_rect[1] < clip_rect[0] {
        let tmp = clip_rect[0];
        clip_rect[0] = clip_rect[1];
        clip_rect[1] = tmp;
    }

    let scale_x = 1.0 / mxx;
    clip_rect[2] *= scale_x;
    clip_rect[3] *= scale_x;

    if clip_rect[3] < clip_rect[2] {
        let tmp = clip_rect[2];
        clip_rect[2] = clip_rect[3];
        clip_rect[3] = tmp;
    }
    0.5 * (scale_x.abs() + scale_y.abs())
}

/// `TransformingPathConsumer2D.adjustClipInverseDelta`.
fn adjust_clip_inverse_delta(clip_rect: &mut [f64; 4], mxx: f64, mxy: f64, myx: f64, myy: f64) -> f64 {
    let det = mxx * myy - mxy * myx;
    let imxx = myy / det;
    let imxy = -mxy / det;
    let imyx = -myx / det;
    let imyy = mxx / det;

    let mut xmin;
    let mut xmax;
    let mut ymin;
    let mut ymax;
    let mut x;
    let mut y;

    x = clip_rect[2] * imxx + clip_rect[0] * imxy;
    y = clip_rect[2] * imyx + clip_rect[0] * imyy;

    xmin = x;
    xmax = x;
    ymin = y;
    ymax = y;

    x = clip_rect[3] * imxx + clip_rect[0] * imxy;
    y = clip_rect[3] * imyx + clip_rect[0] * imyy;

    if x < xmin {
        xmin = x;
    } else if x > xmax {
        xmax = x;
    }
    if y < ymin {
        ymin = y;
    } else if y > ymax {
        ymax = y;
    }

    x = clip_rect[2] * imxx + clip_rect[1] * imxy;
    y = clip_rect[2] * imyx + clip_rect[1] * imyy;

    if x < xmin {
        xmin = x;
    } else if x > xmax {
        xmax = x;
    }
    if y < ymin {
        ymin = y;
    } else if y > ymax {
        ymax = y;
    }

    x = clip_rect[3] * imxx + clip_rect[1] * imxy;
    y = clip_rect[3] * imyx + clip_rect[1] * imyy;

    if x < xmin {
        xmin = x;
    } else if x > xmax {
        xmax = x;
    }
    if y < ymin {
        ymin = y;
    } else if y > ymax {
        ymax = y;
    }

    clip_rect[0] = ymin;
    clip_rect[1] = ymax;
    clip_rect[2] = xmin;
    clip_rect[3] = xmax;

    let scale_x = (imxx * imxx + imxy * imxy).sqrt();
    let scale_y = (imyx * imyx + imyy * imyy).sqrt();
    0.5 * (scale_x + scale_y)
}

/// `TransformingPathConsumer2D.inverseDeltaTransformConsumer`.
pub fn inverse_delta_transform_consumer<'a>(
    out: Box<dyn DPathConsumer2D + 'a>,
    at: Option<&AffineTransform>,
) -> Box<dyn DPathConsumer2D + 'a> {
    let at = match at {
        None => return out,
        Some(at) => at,
    };
    let mxx = at.get_scale_x();
    let mxy = at.get_shear_x();
    let myx = at.get_shear_y();
    let myy = at.get_scale_y();

    if mxy == 0.0 && myx == 0.0 {
        if mxx == 1.0 && myy == 1.0 {
            out
        } else {
            Box::new(DeltaScaleFilter { out, sx: 1.0 / mxx, sy: 1.0 / myy })
        }
    } else {
        let det = mxx * myy - mxy * myx;
        Box::new(DeltaTransformFilter { out, mxx: myy / det, mxy: -mxy / det, myx: -myx / det, myy: mxx / det })
    }
}

/// `TransformingPathConsumer2D.DeltaScaleFilter`.
pub struct DeltaScaleFilter<'a> {
    out: Box<dyn DPathConsumer2D + 'a>,
    sx: f64,
    sy: f64,
}

impl DPathConsumer2D for DeltaScaleFilter<'_> {
    fn move_to(&mut self, x0: f64, y0: f64) {
        self.out.move_to(x0 * self.sx, y0 * self.sy);
    }
    fn line_to(&mut self, x1: f64, y1: f64) {
        self.out.line_to(x1 * self.sx, y1 * self.sy);
    }
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.out.quad_to(x1 * self.sx, y1 * self.sy, x2 * self.sx, y2 * self.sy);
    }
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.out.curve_to(x1 * self.sx, y1 * self.sy, x2 * self.sx, y2 * self.sy, x3 * self.sx, y3 * self.sy);
    }
    fn close_path(&mut self) {
        self.out.close_path();
    }
    fn path_done(&mut self) {
        self.out.path_done();
    }
}

/// `TransformingPathConsumer2D.DeltaTransformFilter`.
pub struct DeltaTransformFilter<'a> {
    out: Box<dyn DPathConsumer2D + 'a>,
    mxx: f64,
    mxy: f64,
    myx: f64,
    myy: f64,
}

impl DPathConsumer2D for DeltaTransformFilter<'_> {
    fn move_to(&mut self, x0: f64, y0: f64) {
        self.out.move_to(x0 * self.mxx + y0 * self.mxy, x0 * self.myx + y0 * self.myy);
    }
    fn line_to(&mut self, x1: f64, y1: f64) {
        self.out.line_to(x1 * self.mxx + y1 * self.mxy, x1 * self.myx + y1 * self.myy);
    }
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.out.quad_to(
            x1 * self.mxx + y1 * self.mxy,
            x1 * self.myx + y1 * self.myy,
            x2 * self.mxx + y2 * self.mxy,
            x2 * self.myx + y2 * self.myy,
        );
    }
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.out.curve_to(
            x1 * self.mxx + y1 * self.mxy,
            x1 * self.myx + y1 * self.myy,
            x2 * self.mxx + y2 * self.mxy,
            x2 * self.myx + y2 * self.myy,
            x3 * self.mxx + y3 * self.mxy,
            x3 * self.myx + y3 * self.myy,
        );
    }
    fn close_path(&mut self) {
        self.out.close_path();
    }
    fn path_done(&mut self) {
        self.out.path_done();
    }
}

/// `TransformingPathConsumer2D.ClosedPathDetector`; `closed_path` is `RendererContext.closedPath`.
pub struct ClosedPathDetector<'a> {
    closed_path: Rc<Cell<bool>>,
    stack: PolyStack,
    out: Box<dyn DPathConsumer2D + 'a>,
}

impl<'a> ClosedPathDetector<'a> {
    pub fn new(closed_path: Rc<Cell<bool>>, out: Box<dyn DPathConsumer2D + 'a>) -> ClosedPathDetector<'a> {
        ClosedPathDetector { closed_path, stack: PolyStack::new(), out }
    }

    /// `ClosedPathDetector.finish(boolean)`.
    fn finish(&mut self, closed: bool) {
        self.closed_path.set(closed);
        self.stack.pull_all(&mut *self.out);
    }
}

impl DPathConsumer2D for ClosedPathDetector<'_> {
    fn path_done(&mut self) {
        self.finish(false);
        self.out.path_done();
        self.stack.dispose();
    }
    fn close_path(&mut self) {
        self.finish(true);
        self.out.close_path();
    }
    fn move_to(&mut self, x0: f64, y0: f64) {
        self.finish(false);
        self.out.move_to(x0, y0);
    }
    fn line_to(&mut self, x1: f64, y1: f64) {
        self.stack.push_line(x1, y1);
    }
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.stack.push_cubic(x3, y3, x2, y2, x1, y1);
    }
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.stack.push_quad(x2, y2, x1, y1);
    }
}

const MONOTONIZER_MAX_N_CURVES: usize = 11;

/// `TransformingPathConsumer2D.CurveBasicMonotonizer`.
pub struct CurveBasicMonotonizer {
    lw2: f64,
    pub nb_splits: usize,
    pub middle: [f64; 68],
    subdiv_ts: [f64; MONOTONIZER_MAX_N_CURVES - 1],
    curve: Curve,
}

impl CurveBasicMonotonizer {
    pub fn new() -> CurveBasicMonotonizer {
        CurveBasicMonotonizer {
            lw2: 0.0,
            nb_splits: 0,
            middle: [0.0; 68],
            subdiv_ts: [0.0; MONOTONIZER_MAX_N_CURVES - 1],
            curve: Curve::default(),
        }
    }

    /// `CurveBasicMonotonizer.init(double)`.
    pub fn init(&mut self, line_width: f64) {
        self.lw2 = (line_width * line_width) / 4.0;
    }

    /// `CurveBasicMonotonizer.curve`.
    #[allow(clippy::too_many_arguments)]
    pub fn curve(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> &CurveBasicMonotonizer {
        let mid = &mut self.middle;
        mid[0] = x0;
        mid[1] = y0;
        mid[2] = x1;
        mid[3] = y1;
        mid[4] = x2;
        mid[5] = y2;
        mid[6] = x3;
        mid[7] = y3;

        let nb_split = helpers::find_subdiv_points(&mut self.curve, &self.middle, &mut self.subdiv_ts, 8, self.lw2);

        let mut prev_t = 0.0;
        let mut off = 0;
        for i in 0..nb_split {
            let t = self.subdiv_ts[i];
            helpers::subdivide_cubic_at((t - prev_t) / (1.0 - prev_t), &mut self.middle, off, off, off + 6);
            prev_t = t;
            off += 6;
        }
        self.nb_splits = nb_split;
        self
    }

    /// `CurveBasicMonotonizer.quad`.
    pub fn quad(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> &CurveBasicMonotonizer {
        let mid = &mut self.middle;
        mid[0] = x0;
        mid[1] = y0;
        mid[2] = x1;
        mid[3] = y1;
        mid[4] = x2;
        mid[5] = y2;

        let nb_split = helpers::find_subdiv_points(&mut self.curve, &self.middle, &mut self.subdiv_ts, 6, self.lw2);

        let mut prev_t = 0.0;
        let mut off = 0;
        for i in 0..nb_split {
            let t = self.subdiv_ts[i];
            helpers::subdivide_quad_at((t - prev_t) / (1.0 - prev_t), &mut self.middle, off, off, off + 4);
            prev_t = t;
            off += 4;
        }
        self.nb_splits = nb_split;
        self
    }
}

impl Default for CurveBasicMonotonizer {
    fn default() -> Self {
        CurveBasicMonotonizer::new()
    }
}

const SPLITTER_MAX_N_CURVES: usize = 12;

/// `TransformingPathConsumer2D.CurveClipSplitter`.
pub struct CurveClipSplitter {
    min_length: f64,
    clip_rect: [f64; 4],
    clip_rect_pad: [f64; 4],
    init_clip_rect_pad: bool,
    middle: [f64; 98],
    subdiv_ts: [f64; SPLITTER_MAX_N_CURVES],
    curve: Curve,
}

impl CurveClipSplitter {
    /// `CurveClipSplitter.LEN_TH`.
    const LEN_TH: f64 = SUBDIVIDER_MIN_LENGTH as f64;
    /// `CurveClipSplitter.DO_CHECK_LENGTH`.
    const DO_CHECK_LENGTH: bool = Self::LEN_TH > 0.0;

    pub fn new() -> CurveClipSplitter {
        CurveClipSplitter {
            min_length: 0.0,
            clip_rect: [0.0; 4],
            clip_rect_pad: [0.0; 4],
            init_clip_rect_pad: false,
            middle: [0.0; 98],
            subdiv_ts: [0.0; SPLITTER_MAX_N_CURVES],
            curve: Curve::default(),
        }
    }

    /// `CurveClipSplitter.init()`; `clip_rect` is `RendererContext.clipRect` as the splitter later reads it.
    pub fn init(&mut self, clip_rect: [f64; 4], clip_inv_scale: f64) {
        self.clip_rect = clip_rect;
        self.init_clip_rect_pad = true;

        if Self::DO_CHECK_LENGTH {
            self.min_length = if clip_inv_scale == 0.0 { Self::LEN_TH } else { Self::LEN_TH * clip_inv_scale };
        }
    }

    /// `CurveClipSplitter.initPaddedClip()`.
    fn init_padded_clip(&mut self) {
        let clip_rect = &self.clip_rect;
        let clip_rect_pad = &mut self.clip_rect_pad;
        clip_rect_pad[0] = clip_rect[0] - 0.25;
        clip_rect_pad[1] = clip_rect[1] + 0.25;
        clip_rect_pad[2] = clip_rect[2] - 0.25;
        clip_rect_pad[3] = clip_rect[3] + 0.25;
    }

    /// `CurveClipSplitter.splitLine`.
    pub fn split_line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, out_code_or: i32, out: &mut dyn DPathConsumer2D) -> bool {
        if Self::DO_CHECK_LENGTH && helpers::fast_line_len(x0, y0, x1, y1) <= self.min_length {
            return false;
        }

        let mid = &mut self.middle;
        mid[0] = x0;
        mid[1] = y0;
        mid[2] = x1;
        mid[3] = y1;

        self.subdivide_at_intersections(4, out_code_or, out)
    }

    /// `CurveClipSplitter.splitQuad`.
    #[allow(clippy::too_many_arguments)]
    pub fn split_quad(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64, out_code_or: i32, out: &mut dyn DPathConsumer2D) -> bool {
        if Self::DO_CHECK_LENGTH && helpers::fast_quad_len(x0, y0, x1, y1, x2, y2) <= self.min_length {
            return false;
        }

        let mid = &mut self.middle;
        mid[0] = x0;
        mid[1] = y0;
        mid[2] = x1;
        mid[3] = y1;
        mid[4] = x2;
        mid[5] = y2;

        self.subdivide_at_intersections(6, out_code_or, out)
    }

    /// `CurveClipSplitter.splitCurve`.
    #[allow(clippy::too_many_arguments)]
    pub fn split_curve(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
        out_code_or: i32,
        out: &mut dyn DPathConsumer2D,
    ) -> bool {
        if Self::DO_CHECK_LENGTH && helpers::fast_curvelen(x0, y0, x1, y1, x2, y2, x3, y3) <= self.min_length {
            return false;
        }

        let mid = &mut self.middle;
        mid[0] = x0;
        mid[1] = y0;
        mid[2] = x1;
        mid[3] = y1;
        mid[4] = x2;
        mid[5] = y2;
        mid[6] = x3;
        mid[7] = y3;

        self.subdivide_at_intersections(8, out_code_or, out)
    }

    /// `CurveClipSplitter.subdivideAtIntersections`.
    fn subdivide_at_intersections(&mut self, type_: usize, out_code_or: i32, out: &mut dyn DPathConsumer2D) -> bool {
        if self.init_clip_rect_pad {
            self.init_clip_rect_pad = false;
            self.init_padded_clip();
        }

        let n_splits =
            helpers::find_clip_points(&mut self.curve, &self.middle, &mut self.subdiv_ts, type_, out_code_or, &self.clip_rect_pad);

        if n_splits == 0 {
            return false;
        }

        let mut prev_t = 0.0;
        let mut off = 0;
        for i in 0..n_splits {
            let t = self.subdiv_ts[i];
            helpers::subdivide_at((t - prev_t) / (1.0 - prev_t), &mut self.middle, off, off, type_);
            prev_t = t;
            off += type_;
        }

        let mut off = 0;
        for _ in 0..=n_splits {
            Self::emit_current(type_, &self.middle, off, out);
            off += type_;
        }
        true
    }

    /// `CurveClipSplitter.emitCurrent`.
    fn emit_current(type_: usize, pts: &[f64], off: usize, out: &mut dyn DPathConsumer2D) {
        if type_ == 8 {
            out.curve_to(pts[off + 2], pts[off + 3], pts[off + 4], pts[off + 5], pts[off + 6], pts[off + 7]);
        } else if type_ == 4 {
            out.line_to(pts[off + 2], pts[off + 3]);
        } else {
            out.quad_to(pts[off + 2], pts[off + 3], pts[off + 4], pts[off + 5]);
        }
    }
}

impl Default for CurveClipSplitter {
    fn default() -> Self {
        CurveClipSplitter::new()
    }
}
