//! `sun.java2d.marlin.DMarlinRenderingEngine` (the stroking half).

use super::dasher::{self, Dasher};
use super::marlin_const::{CAP_BUTT, DO_CLIP, DO_CLIP_RUNTIME_ENABLE, MIN_SUBPIXELS};
use super::renderer_context::{self, PathConsumer2DAdapter, RendererContext};
use super::stroker::Stroker;
use super::transforming_path_consumer2d::{self as tpc, ClosedPathDetector, CurveClipSplitter};
use super::{DPathConsumer2D, PathConsumer2D};
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::{Path2D, WIND_NON_ZERO};
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::{
    AffineTransform, APPLY_IDENTITY, APPLY_SCALE, APPLY_SHEAR, APPLY_TRANSLATE,
};

const SEG_MOVETO: i32 = 0;
const SEG_LINETO: i32 = 1;
const SEG_QUADTO: i32 = 2;
const SEG_CUBICTO: i32 = 3;
const SEG_CLOSE: i32 = 4;

const TYPE_GENERAL_SCALE: i32 = 4;
const TYPE_GENERAL_TRANSFORM: i32 = 32;

/// `DMarlinRenderingEngine.MIN_PEN_SIZE`.
const MIN_PEN_SIZE: f32 = 1.0 / MIN_SUBPIXELS;

/// `DMarlinRenderingEngine.UPPER_BND`.
const UPPER_BND: f64 = f32::MAX as f64 / 2.0;
/// `DMarlinRenderingEngine.LOWER_BND`.
const LOWER_BND: f64 = -UPPER_BND;

/// `DMarlinRenderingEngine.NormMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NormMode {
    OnWithAa,
    OnNoAa,
    Off,
}

/// `DMarlinRenderingEngine.strokeTo(Shape, AffineTransform, Region, BasicStroke, boolean, boolean, boolean,
/// PathConsumer2D)`, as `LoopPipe.getStrokeSpans` calls it.
///
/// `src(t)` returns `src.getPathIterator(t)`'s segments as `currentSegment(double[])` reports them; `clip` is the
/// `Region` (`[lo_x, lo_y, hi_x, hi_y]`), `None` for a null region; `width`, `cap`, `join`, `miter_limit`, `dash` and
/// `dash_phase` are the `BasicStroke`'s.
#[allow(clippy::too_many_arguments)]
pub fn stroke_to(
    src: &dyn Fn(Option<&AffineTransform>) -> Vec<(i32, [f64; 6])>,
    at: &AffineTransform,
    clip: Option<[i32; 4]>,
    width: f32,
    cap: i32,
    join: i32,
    miter_limit: f32,
    dash: Option<&[f32]>,
    dash_phase: f32,
    thin: bool,
    normalize: bool,
    antialias: bool,
    consumer: &mut dyn PathConsumer2D,
) {
    let at = if !at.is_identity() { Some(at) } else { None };

    let norm = if normalize {
        if antialias {
            NormMode::OnWithAa
        } else {
            NormMode::OnNoAa
        }
    } else {
        NormMode::Off
    };

    let mut rdr_ctx = RendererContext::new();

    if let Some([lo_x, lo_y, hi_x, hi_y]) = clip {
        if DO_CLIP || DO_CLIP_RUNTIME_ENABLE {
            let clip_rect = &mut rdr_ctx.clip_rect;
            let rdr_off_x = 0.25;
            let rdr_off_y = 0.25;
            let margin = 1e-3;
            clip_rect[0] = lo_y as f64 - margin + rdr_off_y;
            clip_rect[1] = hi_y as f64 + margin + rdr_off_y;
            clip_rect[2] = lo_x as f64 - margin + rdr_off_x;
            clip_rect[3] = hi_x as f64 + margin + rdr_off_x;
            rdr_ctx.do_clip = true;
        }
    }

    let lw = if thin {
        if antialias {
            user_space_line_width(at, MIN_PEN_SIZE as f64)
        } else {
            user_space_line_width(at, 1.0)
        }
    } else {
        width as f64
    };

    stroke_to_impl(
        &mut rdr_ctx,
        src,
        at,
        lw,
        norm,
        cap,
        join,
        miter_limit,
        dash,
        dash_phase,
        Box::new(PathConsumer2DAdapter::new(consumer)),
    );
}

/// `AffineTransform.getDeterminant()`.
pub(crate) fn get_determinant(at: &AffineTransform) -> f64 {
    match at.state {
        s if s == (APPLY_SHEAR | APPLY_SCALE | APPLY_TRANSLATE) || s == (APPLY_SHEAR | APPLY_SCALE) => {
            at.m00 * at.m11 - at.m01 * at.m10
        }
        s if s == (APPLY_SHEAR | APPLY_TRANSLATE) || s == APPLY_SHEAR => -(at.m01 * at.m10),
        s if s == (APPLY_SCALE | APPLY_TRANSLATE) || s == APPLY_SCALE => at.m00 * at.m11,
        s if s == APPLY_TRANSLATE || s == APPLY_IDENTITY => 1.0,
        _ => panic!("missing case in transform state switch"),
    }
}

/// `DMarlinRenderingEngine.userSpaceLineWidth`.
fn user_space_line_width(at: Option<&AffineTransform>, lw: f64) -> f64 {
    let width_scale = match at {
        None => 1.0,
        Some(at) if (at.get_type() & (TYPE_GENERAL_TRANSFORM | TYPE_GENERAL_SCALE)) != 0 => get_determinant(at).abs().sqrt(),
        Some(at) => {
            let a = at.get_scale_x();
            let c = at.get_shear_x();
            let b = at.get_shear_y();
            let d = at.get_scale_y();

            let ea = a * a + b * b;
            let eb = 2.0 * (a * c + b * d);
            let ec = c * c + d * d;

            let hypot = (eb * eb + (ea - ec) * (ea - ec)).sqrt();
            let widthsquared = (ea + ec + hypot) / 2.0;

            widthsquared.sqrt()
        }
    };

    lw / width_scale
}

/// `DMarlinRenderingEngine.strokeTo(RendererContext, Shape, AffineTransform, double, NormMode, int, int, float,
/// float[], float, DPathConsumer2D)`.
#[allow(clippy::too_many_arguments)]
fn stroke_to_impl<'a>(
    rdr_ctx: &mut RendererContext,
    src: &dyn Fn(Option<&AffineTransform>) -> Vec<(i32, [f64; 6])>,
    mut at: Option<&AffineTransform>,
    mut width: f64,
    norm: NormMode,
    caps: i32,
    join: i32,
    miterlimit: f32,
    dashes: Option<&[f32]>,
    mut dashphase: f32,
    mut pc2d: Box<dyn DPathConsumer2D + 'a>,
) {
    let mut strokerat: Option<&AffineTransform> = None;

    let mut dash_len: i32 = -1;
    let mut dashes_d: Option<Vec<f64>> = None;

    if let Some(dashes) = dashes {
        dash_len = dashes.len() as i32;
        dashes_d = Some(dasher::copy_dash_array(dashes));
    }

    match at.filter(|t| !t.is_identity()) {
        Some(t) => {
            let a = t.get_scale_x();
            let b = t.get_shear_x();
            let c = t.get_shear_y();
            let d = t.get_scale_y();
            let det = a * d - c * b;

            if det.abs() <= 2.0 * f64::from_bits(1) {
                pc2d.move_to(0.0, 0.0);
                pc2d.path_done();
                return;
            }

            if near_zero(a * b + c * d) && near_zero(a * a + c * c - (b * b + d * d)) {
                let scale = (a * a + c * c).sqrt();

                if let Some(dashes_d) = dashes_d.as_mut() {
                    for v in dashes_d.iter_mut().take(dash_len as usize) {
                        *v *= scale;
                    }
                    dashphase = (dashphase as f64 * scale) as f32;
                }
                width *= scale;
            } else {
                strokerat = Some(t);
            }
        }
        None => at = None,
    }

    pc2d = tpc::delta_transform_consumer(rdr_ctx, pc2d, strokerat);

    let mut curve_splitter = Box::new(CurveClipSplitter::new());
    let mut stroker = Stroker::new(rdr_ctx, pc2d, width, caps, join, miterlimit as f64, dashes_d.is_none(), &mut curve_splitter);

    if let Some(dashes_d) = dashes_d {
        stroker.disable_clipping();
        pc2d = Box::new(Dasher::new(rdr_ctx, Box::new(stroker), dashes_d, dash_len, dashphase as f64, width, Some(curve_splitter)));
    } else {
        stroker.set_curve_splitter(curve_splitter);
        pc2d = Box::new(stroker);
        if rdr_ctx.do_clip && caps != CAP_BUTT {
            pc2d = Box::new(ClosedPathDetector::new(rdr_ctx.closed_path.clone(), pc2d));
        }
    }
    pc2d = tpc::inverse_delta_transform_consumer(pc2d, strokerat);

    let segments = src(at);
    let mut pi = NormalizingPathIterator::new(norm);

    path_to(&mut pi, &segments, &mut *pc2d);
}

/// `DMarlinRenderingEngine.nearZero`.
fn near_zero(num: f64) -> bool {
    num.abs() < 2.0 * super::math::ulp(num)
}

/// `DMarlinRenderingEngine.pathTo` / `pathToLoop` (with the `NormMode`'s path iterator).
fn path_to(pi: &mut NormalizingPathIterator, segments: &[(i32, [f64; 6])], pc2d: &mut dyn DPathConsumer2D) {
    let mut coords = [0.0f64; 6];
    let mut subpath_started = false;

    for (type_, seg_coords) in segments {
        let n = match *type_ {
            SEG_MOVETO | SEG_LINETO => 2,
            SEG_QUADTO => 4,
            SEG_CUBICTO => 6,
            _ => 0,
        };
        coords[..n].copy_from_slice(&seg_coords[..n]);
        let in_bounds = |v: f64| v < UPPER_BND && v > LOWER_BND;

        match pi.current_segment(*type_, &mut coords) {
            SEG_MOVETO => {
                if in_bounds(coords[0]) && in_bounds(coords[1]) {
                    pc2d.move_to(coords[0], coords[1]);
                    subpath_started = true;
                }
            }
            SEG_LINETO => {
                if in_bounds(coords[0]) && in_bounds(coords[1]) {
                    if subpath_started {
                        pc2d.line_to(coords[0], coords[1]);
                    } else {
                        pc2d.move_to(coords[0], coords[1]);
                        subpath_started = true;
                    }
                }
            }
            SEG_QUADTO => {
                if in_bounds(coords[2]) && in_bounds(coords[3]) {
                    if subpath_started {
                        if in_bounds(coords[0]) && in_bounds(coords[1]) {
                            pc2d.quad_to(coords[0], coords[1], coords[2], coords[3]);
                        } else {
                            pc2d.line_to(coords[2], coords[3]);
                        }
                    } else {
                        pc2d.move_to(coords[2], coords[3]);
                        subpath_started = true;
                    }
                }
            }
            SEG_CUBICTO => {
                if in_bounds(coords[4]) && in_bounds(coords[5]) {
                    if subpath_started {
                        if in_bounds(coords[0]) && in_bounds(coords[1]) && in_bounds(coords[2]) && in_bounds(coords[3]) {
                            pc2d.curve_to(coords[0], coords[1], coords[2], coords[3], coords[4], coords[5]);
                        } else {
                            pc2d.line_to(coords[4], coords[5]);
                        }
                    } else {
                        pc2d.move_to(coords[4], coords[5]);
                        subpath_started = true;
                    }
                }
            }
            SEG_CLOSE => {
                if subpath_started {
                    pc2d.close_path();
                }
            }
            _ => {}
        }
    }
    pc2d.path_done();
}

impl Drop for NormalizingPathIterator {
    fn drop(&mut self) {
        let a = [self.curx_adjust, self.cury_adjust, self.movx_adjust, self.movy_adjust];
        match self.mode {
            NormMode::OnWithAa => renderer_context::update_carried_state(|c| c.npc_adjust = a),
            NormMode::OnNoAa => renderer_context::update_carried_state(|c| c.npq_adjust = a),
            NormMode::Off => {}
        }
    }
}

/// `DMarlinRenderingEngine.NormalizingPathIterator` (`NearestPixelCenter` / `NearestPixelQuarter`), or the source
/// iterator itself for `NormMode.OFF`.
struct NormalizingPathIterator {
    mode: NormMode,
    curx_adjust: f64,
    cury_adjust: f64,
    movx_adjust: f64,
    movy_adjust: f64,
}

impl NormalizingPathIterator {
    fn new(mode: NormMode) -> NormalizingPathIterator {
        let c = renderer_context::carried_state();
        let a = if mode == NormMode::OnWithAa { c.npc_adjust } else { c.npq_adjust };
        NormalizingPathIterator { mode, curx_adjust: a[0], cury_adjust: a[1], movx_adjust: a[2], movy_adjust: a[3] }
    }

    /// `NormalizingPathIterator.normCoord`.
    fn norm_coord(&self, coord: f64) -> f64 {
        match self.mode {
            NormMode::OnWithAa => coord.floor() + 0.5,
            _ => (coord + 0.25).floor() + 0.25,
        }
    }

    /// `NormalizingPathIterator.currentSegment(double[])`.
    fn current_segment(&mut self, type_: i32, coords: &mut [f64; 6]) -> i32 {
        if self.mode == NormMode::Off {
            return type_;
        }
        let last_coord = match type_ {
            SEG_MOVETO | SEG_LINETO => 0,
            SEG_QUADTO => 2,
            SEG_CUBICTO => 4,
            SEG_CLOSE => {
                self.curx_adjust = self.movx_adjust;
                self.cury_adjust = self.movy_adjust;
                return type_;
            }
            _ => panic!("Unrecognized curve type"),
        };

        let mut coord = coords[last_coord];
        let mut x_adjust = self.norm_coord(coord);
        coords[last_coord] = x_adjust;
        x_adjust -= coord;

        coord = coords[last_coord + 1];
        let mut y_adjust = self.norm_coord(coord);
        coords[last_coord + 1] = y_adjust;
        y_adjust -= coord;

        match type_ {
            SEG_MOVETO => {
                self.movx_adjust = x_adjust;
                self.movy_adjust = y_adjust;
            }
            SEG_QUADTO => {
                coords[0] += (self.curx_adjust + x_adjust) / 2.0;
                coords[1] += (self.cury_adjust + y_adjust) / 2.0;
            }
            SEG_CUBICTO => {
                coords[0] += self.curx_adjust;
                coords[1] += self.cury_adjust;
                coords[2] += x_adjust;
                coords[3] += y_adjust;
            }
            _ => {}
        }
        self.curx_adjust = x_adjust;
        self.cury_adjust = y_adjust;
        type_
    }
}

/// `TransformingPathConsumer2D.Path2DWrapper`: the stroker's output appended to a `Path2D.Double`.
struct Path2DWrapper<'a>(&'a mut Path2D);

impl DPathConsumer2D for Path2DWrapper<'_> {
    fn move_to(&mut self, x0: f64, y0: f64) {
        self.0.move_to(x0, y0);
    }
    fn line_to(&mut self, x1: f64, y1: f64) {
        self.0.line_to(x1, y1);
    }
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.0.curve_to(x1, y1, x2, y2, x3, y3);
    }
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.0.quad_to(x1, y1, x2, y2);
    }
    fn close_path(&mut self) {
        self.0.close_path();
    }
    fn path_done(&mut self) {}
}

/// `DMarlinRenderingEngine.createStrokedShape(src, width, caps, join, miterlimit, dashes, dashphase)`, as
/// `BasicStroke.createStrokedShape` calls it: the outline as a `Path2D.Double`.
#[allow(clippy::too_many_arguments)]
pub fn create_stroked_shape(src: &dyn Fn(Option<&AffineTransform>) -> Vec<(i32, [f64; 6])>, width: f32, caps: i32, join: i32, miterlimit: f32, dashes: Option<&[f32]>, dashphase: f32) -> Path2D {
    let mut p2d = Path2D::new_double(WIND_NON_ZERO);
    let mut rdr_ctx = RendererContext::new();
    stroke_to_impl(&mut rdr_ctx, src, None, width as f64, NormMode::Off, caps, join, miterlimit, dashes, dashphase, Box::new(Path2DWrapper(&mut p2d)));
    p2d
}
