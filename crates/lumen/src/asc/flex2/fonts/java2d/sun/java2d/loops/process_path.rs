//! Port of JDK 17 `sun.java2d.loops.ProcessPath` as the native `ProcessPath.c` runs it for the
//! `DrawPath`/`FillPath` loops (`doDrawPath`, `doFillPath`, `FillPolygon`), plus the AnyInt set-pixel
//! loops behind them (`LineUtils_ProcessLine`, `LineUtils_SetupBresenham`, `AnyIntSetLine`).

pub const PH_MODE_DRAW_CLIP: i32 = 0;
pub const PH_MODE_FILL_CLIP: i32 = 1;
pub const PH_STROKE_PURE: i32 = 0;
pub const PH_STROKE_DEFAULT: i32 = 1;
pub const WIND_EVEN_ODD: i32 = 0;
pub const WIND_NON_ZERO: i32 = 1;

const SEG_MOVETO: u8 = 0;
const SEG_LINETO: u8 = 1;
const SEG_QUADTO: u8 = 2;
const SEG_CUBICTO: u8 = 3;
const SEG_CLOSE: u8 = 4;

const UPPER_BND: f32 = f32::MAX / 4.0;
const LOWER_BND: f32 = -UPPER_BND;
const MDP_PREC: i32 = 10;
const MDP_MULT: i32 = 1 << MDP_PREC;
const MDP_HALF_MULT: i32 = MDP_MULT >> 1;
const UPPER_OUT_BND: i32 = 1 << (30 - MDP_PREC);
const LOWER_OUT_BND: i32 = -UPPER_OUT_BND;
const CALC_UBND: f32 = (1 << (30 - MDP_PREC)) as f32;
const CALC_LBND: f32 = -CALC_UBND;
const EPSF: f32 = 1.0 / MDP_MULT as f32;
const MDP_W_MASK: i32 = -MDP_MULT;
const MDP_F_MASK: i32 = MDP_MULT - 1;
const MAX_CUB_SIZE: f32 = 256.0;
const MAX_QUAD_SIZE: f32 = 1024.0;
const DF_CUB_STEPS: i32 = 3;
const DF_QUAD_STEPS: i32 = 2;
const DF_CUB_SHIFT: i32 = 6;
const DF_QUAD_SHIFT: i32 = 1;
const DF_CUB_COUNT: i32 = 1 << DF_CUB_STEPS;
const DF_QUAD_COUNT: i32 = 1 << DF_QUAD_STEPS;
const DF_CUB_DEC_BND: i32 = 1 << (DF_CUB_SHIFT * 2 + DF_CUB_STEPS + 3);
const DF_CUB_INC_BND: i32 = 1 << (DF_CUB_SHIFT * 2 + DF_CUB_STEPS);
const DF_QUAD_DEC_BND: i32 = 1 << (DF_QUAD_SHIFT * 2 + DF_QUAD_STEPS + 9);
const CUB_A_MDP_MULT: f32 = (1 << (MDP_PREC - DF_CUB_STEPS)) as f32;
const CUB_B_MDP_MULT: f32 = (1 << (MDP_PREC + 1)) as f32;
const CUB_C_MDP_MULT: f32 = (1 << (MDP_PREC + 3)) as f32;
const QUAD_A_MDP_MULT: f32 = (1 << (MDP_PREC - DF_QUAD_STEPS - 1)) as f32;
const QUAD_B_MDP_MULT: f32 = (1 << (MDP_PREC - 1)) as f32;

const CRES_MIN_CLIPPED: i32 = 0;
const CRES_MAX_CLIPPED: i32 = 1;
const CRES_NOT_CLIPPED: i32 = 3;
const CRES_INVISIBLE: i32 = 4;

/// `ProcessPath.DrawHandler`: clip box and the three primitives.
pub trait DrawHandler {
    fn x_min(&self) -> i32;
    fn y_min(&self) -> i32;
    fn x_max(&self) -> i32;
    fn y_max(&self) -> i32;
    fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32);
    fn draw_pixel(&mut self, x: i32, y: i32);
    fn draw_scanline(&mut self, x0: i32, x1: i32, y: i32);
}

/// `Java_sun_java2d_loops_DrawPath_DrawPath` -> `doDrawPath`.
pub fn draw_path(
    dhnd: &mut dyn DrawHandler,
    trans_x: i32,
    trans_y: i32,
    coords: &[f32],
    types: &[u8],
    stroke: i32,
) -> bool {
    if dhnd.x_max() <= dhnd.x_min() || dhnd.y_max() <= dhnd.y_min() {
        return true;
    }
    let mut hnd = ProcessHandler::new(dhnd, PH_MODE_DRAW_CLIP, stroke);
    do_process_path(&mut hnd, coords, types, trans_x as f32, trans_y as f32)
}

/// `Java_sun_java2d_loops_FillPath_FillPath` -> `doFillPath`.
pub fn fill_path(
    dhnd: &mut dyn DrawHandler,
    trans_x: i32,
    trans_y: i32,
    coords: &[f32],
    types: &[u8],
    fill_rule: i32,
    stroke: i32,
) -> bool {
    if dhnd.x_max() <= dhnd.x_min() || dhnd.y_max() <= dhnd.y_min() {
        return true;
    }
    let mut hnd = ProcessHandler::new(dhnd, PH_MODE_FILL_CLIP, stroke);
    if !do_process_path(&mut hnd, coords, types, trans_x as f32, trans_y as f32) {
        return false;
    }
    fill_polygon(&mut hnd, fill_rule);
    true
}

/// `ProcessPath.ProcessHandler` with the adjusted `DrawHandler` bounds; `clip_mode` selects
/// `DrawProcessHandler` (`ProcessFixedLine`) or `FillProcessHandler` (`StoreFixedLine`).
struct ProcessHandler<'a> {
    dhnd: &'a mut dyn DrawHandler,
    clip_mode: i32,
    stroke: i32,
    x_min: i32,
    y_min: i32,
    x_max: i32,
    y_max: i32,
    x_minf: f32,
    y_minf: f32,
    x_maxf: f32,
    y_maxf: f32,
    fd: FillData,
}

impl<'a> ProcessHandler<'a> {
    fn new(dhnd: &'a mut dyn DrawHandler, clip_mode: i32, stroke: i32) -> Self {
        let (x_min, y_min, x_max, y_max) = (dhnd.x_min(), dhnd.y_min(), dhnd.x_max(), dhnd.y_max());
        ProcessHandler {
            dhnd,
            clip_mode,
            stroke,
            x_min,
            y_min,
            x_max,
            y_max,
            x_minf: 0.0,
            y_minf: 0.0,
            x_maxf: 0.0,
            y_maxf: 0.0,
            fd: FillData::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_fixed_line(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        pixel_info: &mut [i32; 5],
        check_bounds: bool,
        end_sub_path: bool,
    ) {
        if self.clip_mode == PH_MODE_DRAW_CLIP {
            self.draw_fixed_line(x1, y1, x2, y2, pixel_info, check_bounds);
        } else {
            self.store_fixed_line(x1, y1, x2, y2, pixel_info, check_bounds, end_sub_path);
        }
    }

    fn process_end_sub_path(&mut self) {
        if self.clip_mode == PH_MODE_FILL_CLIP && !self.fd.is_empty() {
            self.fd.set_ended();
        }
    }

    fn outside(&self, x: i32, y: i32) -> bool {
        self.y_min > y || self.y_max <= y || self.x_min > x || self.x_max <= x
    }

    /// `DrawProcessHandler.PROCESS_POINT`
    fn process_point(&mut self, fx: i32, fy: i32, check_bounds: bool, pixel_info: &mut [i32; 5]) {
        let x = fx >> MDP_PREC;
        let y = fy >> MDP_PREC;
        if check_bounds && self.outside(x, y) {
            return;
        }
        if pixel_info[0] == 0 {
            *pixel_info = [1, x, y, x, y];
            self.dhnd.draw_pixel(x, y);
        } else if (x != pixel_info[3] || y != pixel_info[4]) && (x != pixel_info[1] || y != pixel_info[2]) {
            self.dhnd.draw_pixel(x, y);
            pixel_info[3] = x;
            pixel_info[4] = y;
        }
    }

    /// `PROCESS_LINE` of ProcessPath.c: unlike `DrawProcessHandler.PROCESS_LINE` it clips the pixel line
    /// to the float box instead of testing endpoints.
    fn process_line(&mut self, fx0: i32, fy0: i32, fx1: i32, fy1: i32, check_bounds: bool, pixel_info: &mut [i32; 5]) {
        let mut c = [fx0 >> MDP_PREC, fy0 >> MDP_PREC, fx1 >> MDP_PREC, fy1 >> MDP_PREC, 0, 0];
        if check_bounds {
            let x_minf = self.x_minf + 0.5;
            let y_minf = self.y_minf + 0.5;
            let x_maxf = self.x_maxf + 0.5;
            let y_maxf = self.y_maxf + 0.5;
            if test_and_clip_fi(y_minf, y_maxf, &mut c, 1, 0, 3, 2) == CRES_INVISIBLE
                || test_and_clip_fi(y_minf, y_maxf, &mut c, 3, 2, 1, 0) == CRES_INVISIBLE
                || test_and_clip_fi(x_minf, x_maxf, &mut c, 0, 1, 2, 3) == CRES_INVISIBLE
                || test_and_clip_fi(x_minf, x_maxf, &mut c, 2, 3, 0, 1) == CRES_INVISIBLE
            {
                return;
            }
        }
        let [x0, y0, x1, y1, _, _] = c;
        if ((x0 ^ x1) | (y0 ^ y1)) == 0 {
            if pixel_info[0] == 0 {
                *pixel_info = [1, x0, y0, x0, y0];
                self.dhnd.draw_pixel(x0, y0);
            } else if (x0 != pixel_info[3] || y0 != pixel_info[4]) && (x0 != pixel_info[1] || y0 != pixel_info[2]) {
                self.dhnd.draw_pixel(x0, y0);
                pixel_info[3] = x0;
                pixel_info[4] = y0;
            }
            return;
        }
        if pixel_info[0] != 0
            && ((pixel_info[1] == x0 && pixel_info[2] == y0) || (pixel_info[3] == x0 && pixel_info[4] == y0))
        {
            self.dhnd.draw_pixel(x0, y0);
        }
        self.dhnd.draw_line(x0, y0, x1, y1);
        if pixel_info[0] == 0 {
            *pixel_info = [1, x0, y0, x0, y0];
        }
        if (pixel_info[1] == x1 && pixel_info[2] == y1) || (pixel_info[3] == x1 && pixel_info[4] == y1) {
            self.dhnd.draw_pixel(x1, y1);
        }
        pixel_info[3] = x1;
        pixel_info[4] = y1;
    }

    /// `DrawProcessHandler.processFixedLine` (`ProcessFixedLine`)
    fn draw_fixed_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, pixel_info: &mut [i32; 5], check_bounds: bool) {
        let c = (x1 ^ x2) | (y1 ^ y2);
        if (c & MDP_W_MASK) == 0 {
            if c == 0 {
                self.process_point(x1 + MDP_HALF_MULT, y1 + MDP_HALF_MULT, check_bounds, pixel_info);
            }
            return;
        }
        let (rx1, ry1, rx2, ry2);
        if x1 == x2 || y1 == y2 {
            rx1 = x1 + MDP_HALF_MULT;
            rx2 = x2 + MDP_HALF_MULT;
            ry1 = y1 + MDP_HALF_MULT;
            ry2 = y2 + MDP_HALF_MULT;
        } else {
            let dx = x2.wrapping_sub(x1);
            let dy = y2.wrapping_sub(y1);
            let fx1 = x1 & MDP_W_MASK;
            let fy1 = y1 & MDP_W_MASK;
            let fx2 = x2 & MDP_W_MASK;
            let fy2 = y2 & MDP_W_MASK;
            if fx1 == x1 || fy1 == y1 {
                rx1 = x1 + MDP_HALF_MULT;
                ry1 = y1 + MDP_HALF_MULT;
            } else {
                let bx1 = if x1 < x2 { fx1 + MDP_MULT } else { fx1 };
                let by1 = if y1 < y2 { fy1 + MDP_MULT } else { fy1 };
                let cross = y1.wrapping_add((bx1 - x1).wrapping_mul(dy).wrapping_div(dx));
                if cross >= fy1 && cross <= fy1 + MDP_MULT {
                    rx1 = bx1;
                    ry1 = cross + MDP_HALF_MULT;
                } else {
                    let cross = x1.wrapping_add((by1 - y1).wrapping_mul(dx).wrapping_div(dy));
                    rx1 = cross + MDP_HALF_MULT;
                    ry1 = by1;
                }
            }
            if fx2 == x2 || fy2 == y2 {
                rx2 = x2 + MDP_HALF_MULT;
                ry2 = y2 + MDP_HALF_MULT;
            } else {
                let bx2 = if x1 > x2 { fx2 + MDP_MULT } else { fx2 };
                let by2 = if y1 > y2 { fy2 + MDP_MULT } else { fy2 };
                let cross = y2.wrapping_add((bx2 - x2).wrapping_mul(dy).wrapping_div(dx));
                if cross >= fy2 && cross <= fy2 + MDP_MULT {
                    rx2 = bx2;
                    ry2 = cross + MDP_HALF_MULT;
                } else {
                    let cross = x2.wrapping_add((by2 - y2).wrapping_mul(dx).wrapping_div(dy));
                    rx2 = cross + MDP_HALF_MULT;
                    ry2 = by2;
                }
            }
        }
        self.process_line(rx1, ry1, rx2, ry2, check_bounds, pixel_info);
    }

    /// `FillProcessHandler.processFixedLine` (`StoreFixedLine`)
    #[allow(clippy::too_many_arguments)]
    fn store_fixed_line(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        pixel_info: &mut [i32; 5],
        check_bounds: bool,
        end_sub_path: bool,
    ) {
        if check_bounds {
            let mut c = [x1, y1, x2, y2, 0, 0];
            let out_x_min = (self.x_minf * MDP_MULT as f32) as i32;
            let out_x_max = (self.x_maxf * MDP_MULT as f32) as i32;
            let out_y_min = (self.y_minf * MDP_MULT as f32) as i32;
            let out_y_max = (self.y_maxf * MDP_MULT as f32) as i32;
            let res = test_and_clip_i(out_y_min, out_y_max, &mut c, 1, 0, 3, 2);
            if res == CRES_INVISIBLE {
                return;
            }
            let res = test_and_clip_i(out_y_min, out_y_max, &mut c, 3, 2, 1, 0);
            if res == CRES_INVISIBLE {
                return;
            }
            let mut last_clipped = is_clipped(res);
            let res = clip_clamp_i(out_x_min, out_x_max, &mut c, 0, 1, 2, 3, 4, 5);
            if res == CRES_MIN_CLIPPED {
                self.store_fixed_line(c[4], c[5], c[0], c[1], pixel_info, false, last_clipped);
            } else if res == CRES_INVISIBLE {
                return;
            }
            let res = clip_clamp_i(out_x_min, out_x_max, &mut c, 2, 3, 0, 1, 4, 5);
            last_clipped = last_clipped || res == CRES_MAX_CLIPPED;
            self.store_fixed_line(c[0], c[1], c[2], c[3], pixel_info, false, last_clipped);
            if res == CRES_MIN_CLIPPED {
                self.store_fixed_line(c[2], c[3], c[4], c[5], pixel_info, false, last_clipped);
            }
            return;
        }
        if self.fd.is_empty() || self.fd.is_ended() {
            self.fd.add_point(x1, y1, false);
        }
        self.fd.add_point(x2, y2, false);
        if end_sub_path {
            self.fd.set_ended();
        }
    }
}

fn clip_f(a1: f32, b1: f32, a2: f32, b2: f32, t: f64) -> f32 {
    (b1 as f64 + (t - a1 as f64) * (b2 - b1) as f64 / (a2 - a1) as f64) as f32
}

fn clip_i(a1: i32, b1: i32, a2: i32, b2: i32, t: f64) -> i32 {
    (b1 as f64 + (t - a1 as f64) * b2.wrapping_sub(b1) as f64 / a2.wrapping_sub(a1) as f64) as i32
}

fn is_clipped(res: i32) -> bool {
    res == CRES_MIN_CLIPPED || res == CRES_MAX_CLIPPED
}

/// `ProcessPath.TESTANDCLIP(float, float, float[], ...)`
fn test_and_clip_f(min: f32, max: f32, c: &mut [f32; 6], a1: usize, b1: usize, a2: usize, b2: usize) -> i32 {
    let mut res = CRES_NOT_CLIPPED;
    if c[a1] < min || c[a1] > max {
        let t;
        if c[a1] < min {
            if c[a2] < min {
                return CRES_INVISIBLE;
            }
            res = CRES_MIN_CLIPPED;
            t = min as f64;
        } else {
            if c[a2] > max {
                return CRES_INVISIBLE;
            }
            res = CRES_MAX_CLIPPED;
            t = max as f64;
        }
        c[b1] = clip_f(c[a1], c[b1], c[a2], c[b2], t);
        c[a1] = t as f32;
    }
    res
}

/// `ProcessPath.TESTANDCLIP(int, int, int[], ...)`
fn test_and_clip_i(min: i32, max: i32, c: &mut [i32; 6], a1: usize, b1: usize, a2: usize, b2: usize) -> i32 {
    let mut res = CRES_NOT_CLIPPED;
    if c[a1] < min || c[a1] > max {
        let t;
        if c[a1] < min {
            if c[a2] < min {
                return CRES_INVISIBLE;
            }
            res = CRES_MIN_CLIPPED;
            t = min as f64;
        } else {
            if c[a2] > max {
                return CRES_INVISIBLE;
            }
            res = CRES_MAX_CLIPPED;
            t = max as f64;
        }
        c[b1] = clip_i(c[a1], c[b1], c[a2], c[b2], t);
        c[a1] = t as i32;
    }
    res
}

/// `(jint)` of a double as `cvttsd2si` does it.
fn d2i_native(v: f64) -> i32 {
    if v > -2147483649.0 && v < 2147483648.0 {
        v as i32
    } else {
        i32::MIN
    }
}

/// `TESTANDCLIP(float bounds, ..., jint, res)` as `PROCESS_LINE` uses it.
fn test_and_clip_fi(min: f32, max: f32, c: &mut [i32; 6], a1: usize, b1: usize, a2: usize, b2: usize) -> i32 {
    let mut res = CRES_NOT_CLIPPED;
    if (c[a1] as f32) < min || c[a1] as f32 > max {
        let t;
        if (c[a1] as f32) < min {
            if (c[a2] as f32) < min {
                return CRES_INVISIBLE;
            }
            res = CRES_MIN_CLIPPED;
            t = min as f64;
        } else {
            if c[a2] as f32 > max {
                return CRES_INVISIBLE;
            }
            res = CRES_MAX_CLIPPED;
            t = max as f64;
        }
        let d = (t - c[a1] as f64) * c[b2].wrapping_sub(c[b1]) as f64 / c[a2].wrapping_sub(c[a1]) as f64;
        c[b1] = d2i_native(c[b1] as f64 + d);
        c[a1] = d2i_native(t);
    }
    res
}

/// `ProcessPath.CLIPCLAMP(float, float, float[], ...)`
#[allow(clippy::too_many_arguments)]
fn clip_clamp_f(min: f32, max: f32, c: &mut [f32; 6], a1: usize, b1: usize, a2: usize, b2: usize, a3: usize, b3: usize) -> i32 {
    c[a3] = c[a1];
    c[b3] = c[b1];
    let mut res = test_and_clip_f(min, max, c, a1, b1, a2, b2);
    if res == CRES_MIN_CLIPPED {
        c[a3] = c[a1];
    } else if res == CRES_MAX_CLIPPED {
        c[a3] = c[a1];
        res = CRES_MAX_CLIPPED;
    } else if res == CRES_INVISIBLE {
        if c[a1] > max {
            res = CRES_INVISIBLE;
        } else {
            c[a1] = min;
            c[a2] = min;
            res = CRES_NOT_CLIPPED;
        }
    }
    res
}

/// `ProcessPath.CLIPCLAMP(int, int, int[], ...)`
#[allow(clippy::too_many_arguments)]
fn clip_clamp_i(min: i32, max: i32, c: &mut [i32; 6], a1: usize, b1: usize, a2: usize, b2: usize, a3: usize, b3: usize) -> i32 {
    c[a3] = c[a1];
    c[b3] = c[b1];
    let mut res = test_and_clip_i(min, max, c, a1, b1, a2, b2);
    if res == CRES_MIN_CLIPPED {
        c[a3] = c[a1];
    } else if res == CRES_MAX_CLIPPED {
        c[a3] = c[a1];
        res = CRES_MAX_CLIPPED;
    } else if res == CRES_INVISIBLE {
        if c[a1] > max {
            res = CRES_INVISIBLE;
        } else {
            c[a1] = min;
            c[a2] = min;
            res = CRES_NOT_CLIPPED;
        }
    }
    res
}

/// `ProcessPath.DrawMonotonicQuad`
fn draw_monotonic_quad(hnd: &mut ProcessHandler, coords: &[f32], check_bounds: bool, pixel_info: &mut [i32; 5]) {
    let mult = MDP_MULT as f32;
    let x0 = (coords[0] * mult) as i32;
    let y0 = (coords[1] * mult) as i32;
    let xe = (coords[4] * mult) as i32;
    let ye = (coords[5] * mult) as i32;
    let mut px = (x0 & MDP_F_MASK) << DF_QUAD_SHIFT;
    let mut py = (y0 & MDP_F_MASK) << DF_QUAD_SHIFT;
    let mut count = DF_QUAD_COUNT;
    let mut shift = DF_QUAD_SHIFT;
    let ax = ((coords[0] - 2.0 * coords[2] + coords[4]) * QUAD_A_MDP_MULT) as i32;
    let ay = ((coords[1] - 2.0 * coords[3] + coords[5]) * QUAD_A_MDP_MULT) as i32;
    let bx = ((-2.0 * coords[0] + 2.0 * coords[2]) * QUAD_B_MDP_MULT) as i32;
    let by = ((-2.0 * coords[1] + 2.0 * coords[3]) * QUAD_B_MDP_MULT) as i32;
    let ddpx = ax.wrapping_mul(2);
    let ddpy = ay.wrapping_mul(2);
    let mut dpx = ax.wrapping_add(bx);
    let mut dpy = ay.wrapping_add(by);
    let mut x2 = x0;
    let mut y2 = y0;
    let mut max_dd = ddpx.wrapping_abs().max(ddpy.wrapping_abs());
    let dx = xe.wrapping_sub(x0);
    let dy = ye.wrapping_sub(y0);
    let x0w = x0 & MDP_W_MASK;
    let y0w = y0 & MDP_W_MASK;
    while max_dd > DF_QUAD_DEC_BND {
        dpx = (dpx << 1).wrapping_sub(ax);
        dpy = (dpy << 1).wrapping_sub(ay);
        count <<= 1;
        max_dd >>= 2;
        px <<= 2;
        py <<= 2;
        shift += 2;
    }
    loop {
        let c = count;
        count -= 1;
        if c <= 1 {
            break;
        }
        px = px.wrapping_add(dpx);
        py = py.wrapping_add(dpy);
        dpx = dpx.wrapping_add(ddpx);
        dpy = dpy.wrapping_add(ddpy);
        let x1 = x2;
        let y1 = y2;
        x2 = x0w.wrapping_add(px.wrapping_shr(shift as u32));
        y2 = y0w.wrapping_add(py.wrapping_shr(shift as u32));
        if (xe.wrapping_sub(x2) ^ dx) < 0 {
            x2 = xe;
        }
        if (ye.wrapping_sub(y2) ^ dy) < 0 {
            y2 = ye;
        }
        hnd.process_fixed_line(x1, y1, x2, y2, pixel_info, check_bounds, false);
    }
    hnd.process_fixed_line(x2, y2, xe, ye, pixel_info, check_bounds, false);
}

/// `ProcessPath.ProcessMonotonicQuad`
fn process_monotonic_quad(hnd: &mut ProcessHandler, coords: &mut [f32], pixel_info: &mut [i32; 5]) {
    let mut coords1 = [0f32; 6];
    let mut x_min = coords[0];
    let mut x_max = coords[0];
    let mut y_min = coords[1];
    let mut y_max = coords[1];
    let mut i = 2;
    while i < 6 {
        x_min = if x_min > coords[i] { coords[i] } else { x_min };
        x_max = if x_max < coords[i] { coords[i] } else { x_max };
        y_min = if y_min > coords[i + 1] { coords[i + 1] } else { y_min };
        y_max = if y_max < coords[i + 1] { coords[i + 1] } else { y_max };
        i += 2;
    }
    if hnd.clip_mode == PH_MODE_DRAW_CLIP {
        if hnd.x_maxf < x_min || hnd.x_minf > x_max || hnd.y_maxf < y_min || hnd.y_minf > y_max {
            return;
        }
    } else {
        if hnd.y_maxf < y_min || hnd.y_minf > y_max || hnd.x_maxf < x_min {
            return;
        }
        if hnd.x_minf > x_max {
            coords[0] = hnd.x_minf;
            coords[2] = hnd.x_minf;
            coords[4] = hnd.x_minf;
        }
    }
    if x_max - x_min > MAX_QUAD_SIZE || y_max - y_min > MAX_QUAD_SIZE {
        coords1[4] = coords[4];
        coords1[5] = coords[5];
        coords1[2] = (coords[2] + coords[4]) / 2.0;
        coords1[3] = (coords[3] + coords[5]) / 2.0;
        coords[2] = (coords[0] + coords[2]) / 2.0;
        coords[3] = (coords[1] + coords[3]) / 2.0;
        coords1[0] = (coords[2] + coords1[2]) / 2.0;
        coords[4] = coords1[0];
        coords1[1] = (coords[3] + coords1[3]) / 2.0;
        coords[5] = coords1[1];
        process_monotonic_quad(hnd, coords, pixel_info);
        process_monotonic_quad(hnd, &mut coords1, pixel_info);
    } else {
        let check_bounds = hnd.x_minf >= x_min || hnd.x_maxf <= x_max || hnd.y_minf >= y_min || hnd.y_maxf <= y_max;
        draw_monotonic_quad(hnd, coords, check_bounds, pixel_info);
    }
}

/// `ProcessPath.ProcessFirstMonotonicPartOfQuad`
fn process_first_monotonic_part_of_quad(hnd: &mut ProcessHandler, coords: &mut [f32], pixel_info: &mut [i32; 5], t: f32) {
    let mut coords1 = [0f32; 6];
    coords1[0] = coords[0];
    coords1[1] = coords[1];
    coords1[2] = coords[0] + t * (coords[2] - coords[0]);
    coords1[3] = coords[1] + t * (coords[3] - coords[1]);
    coords[2] += t * (coords[4] - coords[2]);
    coords[3] += t * (coords[5] - coords[3]);
    coords1[4] = coords1[2] + t * (coords[2] - coords1[2]);
    coords[0] = coords1[4];
    coords1[5] = coords1[3] + t * (coords[3] - coords1[3]);
    coords[1] = coords1[5];
    process_monotonic_quad(hnd, &mut coords1, pixel_info);
}

/// `ProcessPath.ProcessQuad`
fn process_quad(hnd: &mut ProcessHandler, coords: &mut [f32], pixel_info: &mut [i32; 5]) {
    let mut params = [0f64; 2];
    let mut cnt = 0;
    if (coords[0] > coords[2] || coords[2] > coords[4]) && (coords[0] < coords[2] || coords[2] < coords[4]) {
        let ax = (coords[0] - 2.0 * coords[2] + coords[4]) as f64;
        if ax != 0.0 {
            let bx = (coords[0] - coords[2]) as f64;
            let param = bx / ax;
            if param < 1.0 && param > 0.0 {
                params[cnt] = param;
                cnt += 1;
            }
        }
    }
    if (coords[1] > coords[3] || coords[3] > coords[5]) && (coords[1] < coords[3] || coords[3] < coords[5]) {
        let ay = (coords[1] - 2.0 * coords[3] + coords[5]) as f64;
        if ay != 0.0 {
            let by = (coords[1] - coords[3]) as f64;
            let param = by / ay;
            if param < 1.0 && param > 0.0 {
                if cnt > 0 {
                    if params[0] > param {
                        params[cnt] = params[0];
                        cnt += 1;
                        params[0] = param;
                    } else if params[0] < param {
                        params[cnt] = param;
                        cnt += 1;
                    }
                } else {
                    params[cnt] = param;
                    cnt += 1;
                }
            }
        }
    }
    match cnt {
        1 => process_first_monotonic_part_of_quad(hnd, coords, pixel_info, params[0] as f32),
        2 => {
            process_first_monotonic_part_of_quad(hnd, coords, pixel_info, params[0] as f32);
            let param = params[1] - params[0];
            if param > 0.0 {
                process_first_monotonic_part_of_quad(hnd, coords, pixel_info, (param / (1.0 - params[0])) as f32);
            }
        }
        _ => {}
    }
    process_monotonic_quad(hnd, coords, pixel_info);
}

/// `ProcessPath.DrawMonotonicCubic`
fn draw_monotonic_cubic(hnd: &mut ProcessHandler, coords: &[f32], check_bounds: bool, pixel_info: &mut [i32; 5]) {
    let mult = MDP_MULT as f32;
    let x0 = (coords[0] * mult) as i32;
    let y0 = (coords[1] * mult) as i32;
    let xe = (coords[6] * mult) as i32;
    let ye = (coords[7] * mult) as i32;
    let mut px = (x0 & MDP_F_MASK) << DF_CUB_SHIFT;
    let mut py = (y0 & MDP_F_MASK) << DF_CUB_SHIFT;
    let mut inc_step_bnd = DF_CUB_INC_BND;
    let mut dec_step_bnd = DF_CUB_DEC_BND;
    let mut count = DF_CUB_COUNT;
    let mut shift = DF_CUB_SHIFT;
    let ax = ((-coords[0] + 3.0 * coords[2] - 3.0 * coords[4] + coords[6]) * CUB_A_MDP_MULT) as i32;
    let ay = ((-coords[1] + 3.0 * coords[3] - 3.0 * coords[5] + coords[7]) * CUB_A_MDP_MULT) as i32;
    let bx = ((3.0 * coords[0] - 6.0 * coords[2] + 3.0 * coords[4]) * CUB_B_MDP_MULT) as i32;
    let by = ((3.0 * coords[1] - 6.0 * coords[3] + 3.0 * coords[5]) * CUB_B_MDP_MULT) as i32;
    let cx = ((-3.0 * coords[0] + 3.0 * coords[2]) * CUB_C_MDP_MULT) as i32;
    let cy = ((-3.0 * coords[1] + 3.0 * coords[3]) * CUB_C_MDP_MULT) as i32;
    let dddpx = ax.wrapping_mul(6);
    let dddpy = ay.wrapping_mul(6);
    let mut ddpx = dddpx.wrapping_add(bx);
    let mut ddpy = dddpy.wrapping_add(by);
    let mut dpx = ax.wrapping_add(bx >> 1).wrapping_add(cx);
    let mut dpy = ay.wrapping_add(by >> 1).wrapping_add(cy);
    let mut x2 = x0;
    let mut y2 = y0;
    let x0w = x0 & MDP_W_MASK;
    let y0w = y0 & MDP_W_MASK;
    let dx = xe.wrapping_sub(x0);
    let dy = ye.wrapping_sub(y0);
    while count > 0 {
        while ddpx.wrapping_abs() > dec_step_bnd || ddpy.wrapping_abs() > dec_step_bnd {
            ddpx = (ddpx << 1).wrapping_sub(dddpx);
            ddpy = (ddpy << 1).wrapping_sub(dddpy);
            dpx = (dpx << 2).wrapping_sub(ddpx >> 1);
            dpy = (dpy << 2).wrapping_sub(ddpy >> 1);
            count <<= 1;
            dec_step_bnd <<= 3;
            inc_step_bnd <<= 3;
            px <<= 3;
            py <<= 3;
            shift += 3;
        }
        while (count & 1) == 0 && shift > DF_CUB_SHIFT && dpx.wrapping_abs() <= inc_step_bnd && dpy.wrapping_abs() <= inc_step_bnd {
            dpx = (dpx >> 2).wrapping_add(ddpx >> 3);
            dpy = (dpy >> 2).wrapping_add(ddpy >> 3);
            ddpx = ddpx.wrapping_add(dddpx) >> 1;
            ddpy = ddpy.wrapping_add(dddpy) >> 1;
            count >>= 1;
            dec_step_bnd >>= 3;
            inc_step_bnd >>= 3;
            px >>= 3;
            py >>= 3;
            shift -= 3;
        }
        count -= 1;
        if count > 0 {
            px = px.wrapping_add(dpx);
            py = py.wrapping_add(dpy);
            dpx = dpx.wrapping_add(ddpx);
            dpy = dpy.wrapping_add(ddpy);
            ddpx = ddpx.wrapping_add(dddpx);
            ddpy = ddpy.wrapping_add(dddpy);
            let x1 = x2;
            let y1 = y2;
            x2 = x0w.wrapping_add(px.wrapping_shr(shift as u32));
            y2 = y0w.wrapping_add(py.wrapping_shr(shift as u32));
            if (xe.wrapping_sub(x2) ^ dx) < 0 {
                x2 = xe;
            }
            if (ye.wrapping_sub(y2) ^ dy) < 0 {
                y2 = ye;
            }
            hnd.process_fixed_line(x1, y1, x2, y2, pixel_info, check_bounds, false);
        } else {
            hnd.process_fixed_line(x2, y2, xe, ye, pixel_info, check_bounds, false);
        }
    }
}

/// `ProcessPath.ProcessMonotonicCubic`
fn process_monotonic_cubic(hnd: &mut ProcessHandler, coords: &mut [f32], pixel_info: &mut [i32; 5]) {
    let mut coords1 = [0f32; 8];
    let mut x_min = coords[0];
    let mut x_max = coords[0];
    let mut y_min = coords[1];
    let mut y_max = coords[1];
    let mut i = 2;
    while i < 8 {
        x_min = if x_min > coords[i] { coords[i] } else { x_min };
        x_max = if x_max < coords[i] { coords[i] } else { x_max };
        y_min = if y_min > coords[i + 1] { coords[i + 1] } else { y_min };
        y_max = if y_max < coords[i + 1] { coords[i + 1] } else { y_max };
        i += 2;
    }
    if hnd.clip_mode == PH_MODE_DRAW_CLIP {
        if hnd.x_maxf < x_min || hnd.x_minf > x_max || hnd.y_maxf < y_min || hnd.y_minf > y_max {
            return;
        }
    } else {
        if hnd.y_maxf < y_min || hnd.y_minf > y_max || hnd.x_maxf < x_min {
            return;
        }
        if hnd.x_minf > x_max {
            coords[0] = hnd.x_minf;
            coords[2] = hnd.x_minf;
            coords[4] = hnd.x_minf;
            coords[6] = hnd.x_minf;
        }
    }
    if x_max - x_min > MAX_CUB_SIZE || y_max - y_min > MAX_CUB_SIZE {
        coords1[6] = coords[6];
        coords1[7] = coords[7];
        coords1[4] = (coords[4] + coords[6]) / 2.0;
        coords1[5] = (coords[5] + coords[7]) / 2.0;
        let tx = (coords[2] + coords[4]) / 2.0;
        let ty = (coords[3] + coords[5]) / 2.0;
        coords1[2] = (tx + coords1[4]) / 2.0;
        coords1[3] = (ty + coords1[5]) / 2.0;
        coords[2] = (coords[0] + coords[2]) / 2.0;
        coords[3] = (coords[1] + coords[3]) / 2.0;
        coords[4] = (coords[2] + tx) / 2.0;
        coords[5] = (coords[3] + ty) / 2.0;
        coords1[0] = (coords[4] + coords1[2]) / 2.0;
        coords[6] = coords1[0];
        coords1[1] = (coords[5] + coords1[3]) / 2.0;
        coords[7] = coords1[1];
        process_monotonic_cubic(hnd, coords, pixel_info);
        process_monotonic_cubic(hnd, &mut coords1, pixel_info);
    } else {
        let check_bounds = hnd.x_minf > x_min || hnd.x_maxf < x_max || hnd.y_minf > y_min || hnd.y_maxf < y_max;
        draw_monotonic_cubic(hnd, coords, check_bounds, pixel_info);
    }
}

/// `ProcessPath.ProcessFirstMonotonicPartOfCubic`
fn process_first_monotonic_part_of_cubic(hnd: &mut ProcessHandler, coords: &mut [f32], pixel_info: &mut [i32; 5], t: f32) {
    let mut coords1 = [0f32; 8];
    coords1[0] = coords[0];
    coords1[1] = coords[1];
    let tx = coords[2] + t * (coords[4] - coords[2]);
    let ty = coords[3] + t * (coords[5] - coords[3]);
    coords1[2] = coords[0] + t * (coords[2] - coords[0]);
    coords1[3] = coords[1] + t * (coords[3] - coords[1]);
    coords1[4] = coords1[2] + t * (tx - coords1[2]);
    coords1[5] = coords1[3] + t * (ty - coords1[3]);
    coords[4] += t * (coords[6] - coords[4]);
    coords[5] += t * (coords[7] - coords[5]);
    coords[2] = tx + t * (coords[4] - tx);
    coords[3] = ty + t * (coords[5] - ty);
    coords1[6] = coords1[4] + t * (coords[2] - coords1[4]);
    coords[0] = coords1[6];
    coords1[7] = coords1[5] + t * (coords[3] - coords1[5]);
    coords[1] = coords1[7];
    process_monotonic_cubic(hnd, &mut coords1, pixel_info);
}

/// `SOLVEQUADINRANGE` of `ProcessPath.c`; unlike `QuadCurve2D.solveQuadratic` it drops the repeated root.
fn solve_quad_in_range(a: f64, b: f64, c: f64, params: &mut [f64; 4], cnt: &mut usize) {
    if a != 0.0 {
        let mut d = b * b - 4.0 * a * c;
        if d < 0.0 {
            return;
        }
        d = d.sqrt();
        if b < 0.0 {
            d = -d;
        }
        let q = (b + d) / -2.0;
        let param = q / a;
        if param < 1.0 && param > 0.0 {
            params[*cnt] = param;
            *cnt += 1;
        }
        if d == 0.0 || q == 0.0 {
            return;
        }
        let param = c / q;
        if param < 1.0 && param > 0.0 {
            params[*cnt] = param;
            *cnt += 1;
        }
    } else if b != 0.0 {
        let param = -c / b;
        if param < 1.0 && param > 0.0 {
            params[*cnt] = param;
            *cnt += 1;
        }
    }
}

/// `ProcessPath.ProcessCubic`
fn process_cubic(hnd: &mut ProcessHandler, coords: &mut [f32], pixel_info: &mut [i32; 5]) {
    let mut params = [0f64; 4];
    let mut cnt = 0usize;
    if (coords[0] > coords[2] || coords[2] > coords[4] || coords[4] > coords[6])
        && (coords[0] < coords[2] || coords[2] < coords[4] || coords[4] < coords[6])
    {
        let ax = (-coords[0] + 3.0 * coords[2] - 3.0 * coords[4] + coords[6]) as f64;
        let bx = (2.0 * (coords[0] - 2.0 * coords[2] + coords[4])) as f64;
        let cx = (-coords[0] + coords[2]) as f64;
        solve_quad_in_range(ax, bx, cx, &mut params, &mut cnt);
    }
    if (coords[1] > coords[3] || coords[3] > coords[5] || coords[5] > coords[7])
        && (coords[1] < coords[3] || coords[3] < coords[5] || coords[5] < coords[7])
    {
        let ay = (-coords[1] + 3.0 * coords[3] - 3.0 * coords[5] + coords[7]) as f64;
        let by = (2.0 * (coords[1] - 2.0 * coords[3] + coords[5])) as f64;
        let cy = (-coords[1] + coords[3]) as f64;
        solve_quad_in_range(ay, by, cy, &mut params, &mut cnt);
    }
    if cnt > 0 {
        params[..cnt].sort_by(|a, b| a.total_cmp(b));
        process_first_monotonic_part_of_cubic(hnd, coords, pixel_info, params[0] as f32);
        for i in 1..cnt {
            let param = params[i] - params[i - 1];
            if param > 0.0 {
                process_first_monotonic_part_of_cubic(hnd, coords, pixel_info, (param / (1.0 - params[i - 1])) as f32);
            }
        }
    }
    process_monotonic_cubic(hnd, coords, pixel_info);
}

/// `ProcessPath.ProcessLine`
fn process_line(hnd: &mut ProcessHandler, x1: f32, y1: f32, x2: f32, y2: f32, pixel_info: &mut [i32; 5]) {
    let mut c = [x1, y1, x2, y2, 0.0, 0.0];
    let (x_min, y_min, x_max, y_max) = (hnd.x_minf, hnd.y_minf, hnd.x_maxf, hnd.y_maxf);
    let mult = MDP_MULT as f32;
    let res = test_and_clip_f(y_min, y_max, &mut c, 1, 0, 3, 2);
    if res == CRES_INVISIBLE {
        return;
    }
    let mut clipped = is_clipped(res);
    let res = test_and_clip_f(y_min, y_max, &mut c, 3, 2, 1, 0);
    if res == CRES_INVISIBLE {
        return;
    }
    let mut last_clipped = is_clipped(res);
    clipped = clipped || last_clipped;
    if hnd.clip_mode == PH_MODE_DRAW_CLIP {
        let res = test_and_clip_f(x_min, x_max, &mut c, 0, 1, 2, 3);
        if res == CRES_INVISIBLE {
            return;
        }
        clipped = clipped || is_clipped(res);
        let res = test_and_clip_f(x_min, x_max, &mut c, 2, 3, 0, 1);
        if res == CRES_INVISIBLE {
            return;
        }
        last_clipped = last_clipped || is_clipped(res);
        clipped = clipped || last_clipped;
        let fx1 = (c[0] * mult) as i32;
        let fy1 = (c[1] * mult) as i32;
        let fx2 = (c[2] * mult) as i32;
        let fy2 = (c[3] * mult) as i32;
        hnd.process_fixed_line(fx1, fy1, fx2, fy2, pixel_info, clipped, last_clipped);
    } else {
        let res = clip_clamp_f(x_min, x_max, &mut c, 0, 1, 2, 3, 4, 5);
        let fx1 = (c[0] * mult) as i32;
        let fy1 = (c[1] * mult) as i32;
        if res == CRES_MIN_CLIPPED {
            let fx3 = (c[4] * mult) as i32;
            let fy3 = (c[5] * mult) as i32;
            hnd.process_fixed_line(fx3, fy3, fx1, fy1, pixel_info, false, last_clipped);
        } else if res == CRES_INVISIBLE {
            return;
        }
        let res = clip_clamp_f(x_min, x_max, &mut c, 2, 3, 0, 1, 4, 5);
        last_clipped = last_clipped || res == CRES_MAX_CLIPPED;
        let fx2 = (c[2] * mult) as i32;
        let fy2 = (c[3] * mult) as i32;
        hnd.process_fixed_line(fx1, fy1, fx2, fy2, pixel_info, false, last_clipped);
        if res == CRES_MIN_CLIPPED {
            let fx3 = (c[4] * mult) as i32;
            let fy3 = (c[5] * mult) as i32;
            hnd.process_fixed_line(fx2, fy2, fx3, fy3, pixel_info, false, last_clipped);
        }
    }
}

fn in_bnd(v: f32) -> bool {
    v < UPPER_BND && v > LOWER_BND
}

/// `ProcessPath.doProcessPath` (native `ProcessPath`)
fn do_process_path(hnd: &mut ProcessHandler, coords: &[f32], types: &[u8], mut xoff: f32, mut yoff: f32) -> bool {
    let mut t = [0f32; 8];
    let mut close_coord = [0f32; 2];
    let mut pixel_info = [0i32; 5];
    let mut subpath_started = false;
    let mut skip = false;
    let max_coords = coords.len();
    let mut index = 0usize;

    if hnd.stroke == PH_STROKE_PURE {
        close_coord = [-0.5, -0.5];
        xoff -= 0.5;
        yoff -= 0.5;
    }
    hnd.x_min = hnd.x_min.clamp(LOWER_OUT_BND, UPPER_OUT_BND);
    hnd.y_min = hnd.y_min.clamp(LOWER_OUT_BND, UPPER_OUT_BND);
    hnd.x_max = hnd.x_max.clamp(LOWER_OUT_BND, UPPER_OUT_BND);
    hnd.y_max = hnd.y_max.clamp(LOWER_OUT_BND, UPPER_OUT_BND);
    hnd.x_minf = hnd.x_min as f32 - 0.5;
    hnd.y_minf = hnd.y_min as f32 - 0.5;
    hnd.x_maxf = hnd.x_max as f32 - 0.5 - EPSF;
    hnd.y_maxf = hnd.y_max as f32 - 0.5 - EPSF;

    for &ty in types {
        match ty {
            SEG_MOVETO => {
                if index + 2 > max_coords {
                    return false;
                }
                if subpath_started && !skip {
                    if hnd.clip_mode == PH_MODE_FILL_CLIP && (t[0] != close_coord[0] || t[1] != close_coord[1]) {
                        process_line(hnd, t[0], t[1], close_coord[0], close_coord[1], &mut pixel_info);
                    }
                    hnd.process_end_sub_path();
                }
                t[0] = coords[index] + xoff;
                t[1] = coords[index + 1] + yoff;
                index += 2;
                if in_bnd(t[0]) && in_bnd(t[1]) {
                    subpath_started = true;
                    skip = false;
                    close_coord = [t[0], t[1]];
                } else {
                    skip = true;
                }
                pixel_info[0] = 0;
            }
            SEG_LINETO => {
                if index + 2 > max_coords {
                    return false;
                }
                let x = coords[index] + xoff;
                let y = coords[index + 1] + yoff;
                index += 2;
                t[2] = x;
                t[3] = y;
                if in_bnd(x) && in_bnd(y) {
                    if skip {
                        t[0] = x;
                        t[1] = y;
                        close_coord = [x, y];
                        subpath_started = true;
                        skip = false;
                    } else {
                        process_line(hnd, t[0], t[1], t[2], t[3], &mut pixel_info);
                        t[0] = x;
                        t[1] = y;
                    }
                }
            }
            SEG_QUADTO => {
                if index + 4 > max_coords {
                    return false;
                }
                t[2] = coords[index] + xoff;
                t[3] = coords[index + 1] + yoff;
                let x = coords[index + 2] + xoff;
                let y = coords[index + 3] + yoff;
                index += 4;
                t[4] = x;
                t[5] = y;
                if in_bnd(x) && in_bnd(y) {
                    if skip {
                        t[0] = x;
                        t[1] = y;
                        close_coord = [x, y];
                        subpath_started = true;
                        skip = false;
                    } else {
                        if in_bnd(t[2]) && in_bnd(t[3]) {
                            process_quad(hnd, &mut t[..6], &mut pixel_info);
                        } else {
                            process_line(hnd, t[0], t[1], t[4], t[5], &mut pixel_info);
                        }
                        t[0] = x;
                        t[1] = y;
                    }
                }
            }
            SEG_CUBICTO => {
                if index + 6 > max_coords {
                    return false;
                }
                t[2] = coords[index] + xoff;
                t[3] = coords[index + 1] + yoff;
                t[4] = coords[index + 2] + xoff;
                t[5] = coords[index + 3] + yoff;
                let x = coords[index + 4] + xoff;
                let y = coords[index + 5] + yoff;
                index += 6;
                t[6] = x;
                t[7] = y;
                if in_bnd(x) && in_bnd(y) {
                    if skip {
                        t[0] = x;
                        t[1] = y;
                        close_coord = [x, y];
                        subpath_started = true;
                        skip = false;
                    } else {
                        if in_bnd(t[2]) && in_bnd(t[3]) && in_bnd(t[4]) && in_bnd(t[5]) {
                            process_cubic(hnd, &mut t, &mut pixel_info);
                        } else {
                            process_line(hnd, t[0], t[1], t[6], t[7], &mut pixel_info);
                        }
                        t[0] = x;
                        t[1] = y;
                    }
                }
            }
            SEG_CLOSE => {
                if subpath_started && !skip {
                    skip = false;
                    if t[0] != close_coord[0] || t[1] != close_coord[1] {
                        process_line(hnd, t[0], t[1], close_coord[0], close_coord[1], &mut pixel_info);
                        t[0] = close_coord[0];
                        t[1] = close_coord[1];
                    }
                    hnd.process_end_sub_path();
                }
            }
            _ => {}
        }
    }
    if subpath_started && !skip {
        if hnd.clip_mode == PH_MODE_FILL_CLIP && (t[0] != close_coord[0] || t[1] != close_coord[1]) {
            process_line(hnd, t[0], t[1], close_coord[0], close_coord[1], &mut pixel_info);
        }
        hnd.process_end_sub_path();
    }
    true
}

/// `ProcessPath.Point`
struct Point {
    x: i32,
    y: i32,
    last_point: bool,
    prev: Option<usize>,
    next: Option<usize>,
    next_by_y: Option<usize>,
    edge: Option<usize>,
}

/// `ProcessPath.Edge`
struct Edge {
    x: i32,
    dx: i32,
    dir: i32,
    prev: Option<usize>,
    next: Option<usize>,
}

/// `ProcessPath.FillData`
#[derive(Default)]
struct FillData {
    plg_pnts: Vec<Point>,
    plg_y_min: i32,
    plg_y_max: i32,
}

impl FillData {
    fn add_point(&mut self, x: i32, y: i32, last_point: bool) {
        if self.plg_pnts.is_empty() {
            self.plg_y_min = y;
            self.plg_y_max = y;
        } else {
            self.plg_y_min = self.plg_y_min.min(y);
            self.plg_y_max = self.plg_y_max.max(y);
        }
        self.plg_pnts.push(Point { x, y, last_point, prev: None, next: None, next_by_y: None, edge: None });
    }

    fn is_empty(&self) -> bool {
        self.plg_pnts.is_empty()
    }

    fn is_ended(&self) -> bool {
        self.plg_pnts.last().is_some_and(|p| p.last_point)
    }

    fn set_ended(&mut self) {
        if let Some(p) = self.plg_pnts.last_mut() {
            p.last_point = true;
        }
    }
}

/// `ProcessPath.ActiveEdgeList`
struct ActiveEdgeList {
    head: Option<usize>,
    edges: Vec<Edge>,
}

impl ActiveEdgeList {
    fn insert(&mut self, pnts: &mut [Point], pnt: usize, cy: i32) {
        let np = pnts[pnt].next.unwrap();
        let (x1, y1) = (pnts[pnt].x, pnts[pnt].y);
        let (x2, y2) = (pnts[np].x, pnts[np].y);
        if y1 == y2 {
            return;
        }
        let d_x = x2.wrapping_sub(x1);
        let d_y = y2.wrapping_sub(y1);
        let (mut x0, dy, dir) = if y1 < y2 { (x1, cy.wrapping_sub(y1), -1) } else { (x2, cy.wrapping_sub(y2), 1) };
        let stepx;
        if d_x as f32 > CALC_UBND || (d_x as f32) < CALC_LBND {
            stepx = (d_x as f64 * MDP_MULT as f64 / d_y as f64) as i32;
            x0 = x0.wrapping_add((d_x as f64 * dy as f64 / d_y as f64) as i32);
        } else {
            stepx = (d_x << MDP_PREC).wrapping_div(d_y);
            x0 = x0.wrapping_add(d_x.wrapping_mul(dy).wrapping_div(d_y));
        }
        let ne = self.edges.len();
        self.edges.push(Edge { x: x0, dx: stepx, dir, prev: None, next: self.head });
        if let Some(h) = self.head {
            self.edges[h].prev = Some(ne);
        }
        pnts[pnt].edge = Some(ne);
        self.head = Some(ne);
    }

    fn delete(&mut self, e: usize) {
        let prev = self.edges[e].prev;
        let next = self.edges[e].next;
        match prev {
            Some(p) => self.edges[p].next = next,
            None => self.head = next,
        }
        if let Some(n) = next {
            self.edges[n].prev = prev;
        }
    }

    fn sort(&mut self) {
        let e = &mut self.edges;
        let mut s: Option<usize> = None;
        let mut was_swap = true;
        while s != e[self.head.unwrap()].next && was_swap {
            let mut p = self.head.unwrap();
            let mut r = p;
            let mut q = e[p].next;
            was_swap = false;
            while Some(p) != s {
                let qi = q.unwrap();
                if e[p].x >= e[qi].x {
                    was_swap = true;
                    let temp = e[qi].next;
                    e[qi].next = Some(p);
                    e[p].next = temp;
                    if Some(p) == self.head {
                        self.head = Some(qi);
                    } else {
                        e[r].next = Some(qi);
                    }
                    r = qi;
                } else {
                    r = p;
                    p = e[p].next.unwrap();
                }
                q = e[p].next;
                if q == s {
                    s = Some(p);
                }
            }
        }
        let mut p = self.head;
        let mut prev = None;
        while let Some(i) = p {
            e[i].prev = prev;
            prev = p;
            p = e[i].next;
        }
    }
}

/// `ProcessPath.FillPolygon`
fn fill_polygon(hnd: &mut ProcessHandler, fill_rule: i32) {
    let right_bnd = hnd.x_max - 1;
    let fd = &mut hnd.fd;
    let y_min = fd.plg_y_min;
    let y_max = fd.plg_y_max;
    let hash_size = ((y_max - y_min) >> MDP_PREC) + 4;
    let hash_offset = (y_min - 1) & MDP_W_MASK;
    let counter_mask = if fill_rule == WIND_NON_ZERO { -1 } else { 1 };
    let pnts = &mut fd.plg_pnts;
    let n = pnts.len();
    if n <= 1 {
        return;
    }
    let mut y_hash: Vec<Option<usize>> = vec![None; hash_size as usize];
    pnts[0].prev = None;
    for i in 0..n - 1 {
        let k = ((pnts[i].y - hash_offset - 1) >> MDP_PREC) as usize;
        pnts[i].next_by_y = y_hash[k];
        y_hash[k] = Some(i);
        pnts[i].next = Some(i + 1);
        pnts[i + 1].prev = Some(i);
    }
    let k = ((pnts[n - 1].y - hash_offset - 1) >> MDP_PREC) as usize;
    pnts[n - 1].next_by_y = y_hash[k];
    y_hash[k] = Some(n - 1);

    let mut active = ActiveEdgeList { head: None, edges: Vec::new() };
    let mut y = hash_offset + MDP_MULT;
    let mut k = 0;
    while y <= y_max && k < hash_size {
        let mut pt = y_hash[k as usize];
        while let Some(pi) = pt {
            if let Some(prev) = pnts[pi].prev {
                if !pnts[prev].last_point {
                    match pnts[prev].edge {
                        Some(e) if pnts[prev].y <= y => {
                            active.delete(e);
                            pnts[prev].edge = None;
                        }
                        _ => {
                            if pnts[prev].y > y {
                                active.insert(pnts, prev, y);
                            }
                        }
                    }
                }
            }
            if !pnts[pi].last_point {
                if let Some(next) = pnts[pi].next {
                    match pnts[pi].edge {
                        Some(e) if pnts[next].y <= y => {
                            active.delete(e);
                            pnts[pi].edge = None;
                        }
                        _ => {
                            if pnts[next].y > y {
                                active.insert(pnts, pi, y);
                            }
                        }
                    }
                }
            }
            pt = pnts[pi].next_by_y;
        }
        if active.head.is_some() {
            active.sort();
            let mut counter = 0i32;
            let mut drawing = false;
            let mut xl = hnd.x_min;
            let mut cur = active.head;
            while let Some(e) = cur {
                let edge = &mut active.edges[e];
                counter = counter.wrapping_add(edge.dir);
                if (counter & counter_mask) != 0 && !drawing {
                    xl = edge.x.wrapping_add(MDP_MULT - 1) >> MDP_PREC;
                    drawing = true;
                }
                if (counter & counter_mask) == 0 && drawing {
                    let xr = edge.x.wrapping_sub(1) >> MDP_PREC;
                    if xl <= xr {
                        hnd.dhnd.draw_scanline(xl, xr, y >> MDP_PREC);
                    }
                    drawing = false;
                }
                edge.x = edge.x.wrapping_add(edge.dx);
                cur = edge.next;
            }
            if drawing && xl <= right_bnd {
                hnd.dhnd.draw_scanline(xl, right_bnd, y >> MDP_PREC);
            }
        }
        y += MDP_MULT;
        k += 1;
    }
}

const BUMP_NOOP: i32 = 0x0;
const BUMP_POS_PIXEL: i32 = 0x1;
const BUMP_NEG_PIXEL: i32 = 0x2;
const BUMP_POS_SCAN: i32 = 0x4;
const BUMP_NEG_SCAN: i32 = 0x8;

/// The native AnyInt loop behaviour: a handler that sets `pixel` into an i32 raster (width, height, scan)
/// clipped to [lox,hix)x[loy,hiy), with `draw_line` doing exactly what `LineUtils_ProcessLine` plus
/// `AnyIntSetLine` do; `draw_pixel` and `draw_scanline` are unclipped like `processPoint`/`drawScanline`.
pub struct SetPixelHandler<'a> {
    pub pixels: &'a mut [i32],
    pub scan: usize,
    pub pixel: i32,
    pub lox: i32,
    pub loy: i32,
    pub hix: i32,
    pub hiy: i32,
}

impl SetPixelHandler<'_> {
    fn store(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x as usize >= self.scan {
            return;
        }
        if let Some(p) = self.pixels.get_mut(y as usize * self.scan + x as usize) {
            *p = self.pixel;
        }
    }

    /// `AnyIntSetLine`
    #[allow(clippy::too_many_arguments)]
    fn set_line(&mut self, mut x: i32, mut y: i32, mut steps: i32, mut error: i32, major: i32, errmajor: i32, minor: i32, errminor: i32) {
        let bump = |mask: i32| -> (i32, i32) {
            if mask & BUMP_POS_PIXEL != 0 {
                (1, 0)
            } else if mask & BUMP_NEG_PIXEL != 0 {
                (-1, 0)
            } else if mask & BUMP_POS_SCAN != 0 {
                (0, 1)
            } else if mask & BUMP_NEG_SCAN != 0 {
                (0, -1)
            } else {
                (0, 0)
            }
        };
        let (mjx, mjy) = bump(major);
        let (mnx, mny) = bump(minor);
        let (mnx, mny) = (mnx + mjx, mny + mjy);
        loop {
            self.store(x, y);
            if errmajor == 0 || error < 0 {
                x += mjx;
                y += mjy;
                error = error.wrapping_add(errmajor);
            } else {
                x += mnx;
                y += mny;
                error = error.wrapping_sub(errminor);
            }
            steps -= 1;
            if steps <= 0 {
                break;
            }
        }
    }
}

/// `LineUtils_SetupBresenham` with `shorten == 0`: (x, y, steps, error, errmajor, majormask, errminor, minormask).
#[allow(clippy::type_complexity)]
fn setup_bresenham(x1: i32, y1: i32, x2: i32, y2: i32, b: [i32; 4]) -> Option<(i32, i32, i32, i32, i32, i32, i32, i32)> {
    let dx = x2.wrapping_sub(x1);
    let dy = y2.wrapping_sub(y1);
    let mut ax = dx.wrapping_abs();
    let mut ay = dy.wrapping_abs();
    let (cxmin, cymin, cxmax, cymax) = (b[0], b[1], b[2] - 1, b[3] - 1);
    let xmajor = ax >= ay;
    let outcode = |x: i32, y: i32| -> i32 {
        let mut code = if y < cymin {
            1
        } else if y > cymax {
            2
        } else {
            0
        };
        if x < cxmin {
            code |= 4;
        } else if x > cxmax {
            code |= 8;
        }
        code
    };
    let (mut tx1, mut ty1, mut tx2, mut ty2) = (x1, y1, x2, y2);
    let mut code1 = outcode(tx1, ty1);
    let mut code2 = outcode(tx2, ty2);
    while (code1 | code2) != 0 {
        if (code1 & code2) != 0 {
            return None;
        }
        if code1 != 0 {
            if code1 & 3 != 0 {
                ty1 = if code1 & 1 != 0 { cymin } else { cymax };
                let ysteps = ty1.wrapping_sub(y1).wrapping_abs();
                let mut xsteps = 2i32.wrapping_mul(ysteps).wrapping_mul(ax).wrapping_add(ay);
                if xmajor {
                    xsteps = xsteps.wrapping_add(ay - ax - 1);
                }
                xsteps = xsteps.wrapping_div(2i32.wrapping_mul(ay));
                if dx < 0 {
                    xsteps = -xsteps;
                }
                tx1 = x1.wrapping_add(xsteps);
            } else if code1 & 12 != 0 {
                tx1 = if code1 & 4 != 0 { cxmin } else { cxmax };
                let xsteps = tx1.wrapping_sub(x1).wrapping_abs();
                let mut ysteps = 2i32.wrapping_mul(xsteps).wrapping_mul(ay).wrapping_add(ax);
                if !xmajor {
                    ysteps = ysteps.wrapping_add(ax - ay - 1);
                }
                ysteps = ysteps.wrapping_div(2i32.wrapping_mul(ax));
                if dy < 0 {
                    ysteps = -ysteps;
                }
                ty1 = y1.wrapping_add(ysteps);
            }
            code1 = outcode(tx1, ty1);
        } else {
            if code2 & 3 != 0 {
                ty2 = if code2 & 1 != 0 { cymin } else { cymax };
                let ysteps = ty2.wrapping_sub(y2).wrapping_abs();
                let mut xsteps = 2i32.wrapping_mul(ysteps).wrapping_mul(ax).wrapping_add(ay);
                if xmajor {
                    xsteps = xsteps.wrapping_add(ay - ax);
                } else {
                    xsteps -= 1;
                }
                xsteps = xsteps.wrapping_div(2i32.wrapping_mul(ay));
                if dx > 0 {
                    xsteps = -xsteps;
                }
                tx2 = x2.wrapping_add(xsteps);
            } else if code2 & 12 != 0 {
                tx2 = if code2 & 4 != 0 { cxmin } else { cxmax };
                let xsteps = tx2.wrapping_sub(x2).wrapping_abs();
                let mut ysteps = 2i32.wrapping_mul(xsteps).wrapping_mul(ay).wrapping_add(ax);
                if xmajor {
                    ysteps -= 1;
                } else {
                    ysteps = ysteps.wrapping_add(ax - ay);
                }
                ysteps = ysteps.wrapping_div(2i32.wrapping_mul(ax));
                if dy > 0 {
                    ysteps = -ysteps;
                }
                ty2 = y2.wrapping_add(ysteps);
            }
            code2 = outcode(tx2, ty2);
        }
    }
    let (errmajor, mut errminor, major, minor, steps);
    if xmajor {
        errmajor = ay.wrapping_mul(2);
        errminor = ax.wrapping_mul(2);
        major = if dx < 0 { BUMP_NEG_PIXEL } else { BUMP_POS_PIXEL };
        minor = if dy < 0 { BUMP_NEG_SCAN } else { BUMP_POS_SCAN };
        ax = -ax;
        steps = tx2.wrapping_sub(tx1);
    } else {
        errmajor = ax.wrapping_mul(2);
        errminor = ay.wrapping_mul(2);
        major = if dy < 0 { BUMP_NEG_SCAN } else { BUMP_POS_SCAN };
        minor = if dx < 0 { BUMP_NEG_PIXEL } else { BUMP_POS_PIXEL };
        ay = -ay;
        steps = ty2.wrapping_sub(ty1);
    }
    let steps = steps.wrapping_abs().wrapping_add(1);
    if steps == 0 {
        return None;
    }
    let mut error = -(errminor / 2);
    if ty1 != y1 {
        let ysteps = ty1.wrapping_sub(y1).wrapping_abs();
        error = error.wrapping_add(ysteps.wrapping_mul(ax).wrapping_mul(2));
    }
    if tx1 != x1 {
        let xsteps = tx1.wrapping_sub(x1).wrapping_abs();
        error = error.wrapping_add(xsteps.wrapping_mul(ay).wrapping_mul(2));
    }
    error = error.wrapping_add(errmajor);
    errminor = errminor.wrapping_sub(errmajor);
    Some((tx1, ty1, steps, error, errmajor, major, errminor, minor))
}

impl DrawHandler for SetPixelHandler<'_> {
    fn x_min(&self) -> i32 {
        self.lox
    }

    fn y_min(&self) -> i32 {
        self.loy
    }

    fn x_max(&self) -> i32 {
        self.hix
    }

    fn y_max(&self) -> i32 {
        self.hiy
    }

    /// `drawLine` of DrawPath.c: `LineUtils_ProcessLine`
    fn draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        if y1 == y2 {
            if y1 >= self.loy && y1 < self.hiy {
                let (lo, hi) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
                let mut tx2 = hi.wrapping_add(1);
                if tx2 < lo {
                    tx2 = hi;
                }
                let tx1 = lo.max(self.lox);
                let tx2 = tx2.min(self.hix);
                if tx1 < tx2 {
                    self.set_line(tx1, y1, tx2 - tx1, 0, BUMP_POS_PIXEL, 0, BUMP_NOOP, 0);
                }
            }
        } else if x1 == x2 {
            if x1 >= self.lox && x1 < self.hix {
                let (lo, hi) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
                let mut ty2 = hi.wrapping_add(1);
                if ty2 < lo {
                    ty2 = hi;
                }
                let ty1 = lo.max(self.loy);
                let ty2 = ty2.min(self.hiy);
                if ty1 < ty2 {
                    self.set_line(x1, ty1, ty2 - ty1, 0, BUMP_POS_SCAN, 0, BUMP_NOOP, 0);
                }
            }
        } else if let Some((x, y, steps, error, errmajor, major, errminor, minor)) =
            setup_bresenham(x1, y1, x2, y2, [self.lox, self.loy, self.hix, self.hiy])
        {
            self.set_line(x, y, steps, error, major, errmajor, minor, errminor);
        }
    }

    /// `processPoint` of DrawPath.c
    fn draw_pixel(&mut self, x: i32, y: i32) {
        self.store(x, y);
    }

    /// `drawScanline` of FillPath.c
    fn draw_scanline(&mut self, x0: i32, x1: i32, y: i32) {
        self.set_line(x0, y, x1.wrapping_sub(x0).wrapping_add(1), 0, BUMP_POS_PIXEL, 0, BUMP_NOOP, 0);
    }
}
