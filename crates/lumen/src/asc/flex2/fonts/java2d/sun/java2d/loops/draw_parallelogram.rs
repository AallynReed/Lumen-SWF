//! Port of JDK 17 libawt `DrawParallelogram.c` (`Java_sun_java2d_loops_DrawParallelogram_DrawParallelogram`),
//! AnyInt SrcNoEa, for an `i32` raster, with the `LineUtils_ProcessLine` / `LineUtils_SetupBresenham`
//! / `AnyIntSetLine` path it takes for strokes no wider than a pixel.
//!
//! The entry receives the parallelogram as the converter hands it to the loop: origin, two edge
//! vectors, and the line widths `lw1`, `lw2` as fractions of each edge. The stroke is the outer
//! parallelogram (grown by half a line width) minus the inner one, scanned as up to eight edges.

use super::fill_parallelogram::{
    any_int_set_parallelogram, dbl_to_long, intersect_bounds, ifloor, pgram_init_x, pgram_min_max, sort_pgram, FillTarget,
};

const BUMP_NOOP: i32 = 0x0;
const BUMP_POS_PIXEL: i32 = 0x1;
const BUMP_NEG_PIXEL: i32 = 0x2;
const BUMP_POS_SCAN: i32 = 0x4;
const BUMP_NEG_SCAN: i32 = 0x8;

/// `EdgeInfo`.
#[derive(Clone, Copy, Default)]
struct EdgeInfo {
    x0: f64,
    y0: f64,
    slope: f64,
    dx: i64,
    ystart: i32,
    yend: i32,
}

#[inline]
fn store_edge(x0: f64, y0: f64, y1: f64, slope: f64, dx: i64) -> EdgeInfo {
    EdgeInfo { x0, y0, slope, dx, ystart: ifloor(y0 + 0.5), yend: ifloor(y1 + 0.5) }
}

/// `STORE_PGRAM(pLtEdge, pRtEdge, ...)`: left edges at `lt`, `lt+1`; right edges at `rt`, `rt+1`.
#[allow(clippy::too_many_arguments)]
fn store_pgram(
    e: &mut [EdgeInfo; 8],
    lt: usize,
    rt: usize,
    x0: f64,
    y0: f64,
    dx1: f64,
    dy1: f64,
    dx2: f64,
    dy2: f64,
    slope1: f64,
    slope2: f64,
    delta1: i64,
    delta2: i64,
) {
    let y01 = y0 + dy1;
    let y02 = y0 + dy2;
    let y012 = y01 + dy2;
    e[lt] = store_edge(x0, y0, y01, slope1, delta1);
    e[rt] = store_edge(x0, y0, y02, slope2, delta2);
    e[lt + 1] = store_edge(x0 + dx1, y01, y012, slope2, delta2);
    e[rt + 1] = store_edge(x0 + dx2, y02, y012, slope1, delta1);
}

/// `AnyIntSetLine`.
#[allow(clippy::too_many_arguments)]
fn any_int_set_line(
    raster: &mut [i32],
    scan: usize,
    x: i32,
    y: i32,
    pixel: i32,
    mut steps: i32,
    mut error: i32,
    bumpmajormask: i32,
    errmajor: i32,
    bumpminormask: i32,
    errminor: i32,
) {
    let scan = scan as i64;
    let bump = |m: i32| -> i64 {
        if m & BUMP_POS_PIXEL != 0 {
            1
        } else if m & BUMP_NEG_PIXEL != 0 {
            -1
        } else if m & BUMP_POS_SCAN != 0 {
            scan
        } else {
            -scan
        }
    };
    let bumpmajor = bump(bumpmajormask);
    let bumpminor = bumpmajor
        + if bumpminormask & BUMP_POS_PIXEL != 0 {
            1
        } else if bumpminormask & BUMP_NEG_PIXEL != 0 {
            -1
        } else if bumpminormask & BUMP_POS_SCAN != 0 {
            scan
        } else if bumpminormask & BUMP_NEG_SCAN != 0 {
            -scan
        } else {
            0
        };
    let mut p = y as i64 * scan + x as i64;
    if errmajor == 0 {
        loop {
            raster[p as usize] = pixel;
            p += bumpmajor;
            steps -= 1;
            if steps <= 0 {
                break;
            }
        }
    } else {
        loop {
            raster[p as usize] = pixel;
            if error < 0 {
                p += bumpmajor;
                error = error.wrapping_add(errmajor);
            } else {
                p += bumpminor;
                error = error.wrapping_sub(errminor);
            }
            steps -= 1;
            if steps <= 0 {
                break;
            }
        }
    }
}

#[inline]
fn fits(v: i32, bits: u32) -> bool {
    (v << (32 - bits)) >> (32 - bits) == v
}

#[inline]
fn d2i(v: f64) -> i32 {
    super::fill_parallelogram::d2i(v)
}

/// The double-precision pre-clip of `LineUtils_SetupBresenham` to `[-2^29, 2^29-1]` for coordinates
/// that do not fit in 30 bits (each axis adjusted from the original integer endpoints).
fn clamp_line(x1: i32, y1: i32, x2: i32, y2: i32) -> (i32, i32, i32, i32) {
    const LO: i32 = -0x2000_0000;
    const HI: i32 = 0x1fff_ffff;
    let (dlo, dhi) = (LO as f64, HI as f64);
    let (fx1o, fy1o, fx2o, fy2o) = (x1 as f64, y1 as f64, x2 as f64, y2 as f64);
    let dx = fx2o - fx1o;
    let dy = fy2o - fy1o;
    let (mut fx1, mut fy1) = (fx1o, fy1o);
    if x1 < LO {
        fy1 = fy1o + LO.wrapping_sub(x1) as f64 * dy / dx;
        fx1 = dlo;
    } else if x1 > HI {
        fy1 = fy1o - x1.wrapping_sub(HI) as f64 * dy / dx;
        fx1 = dhi;
    }
    let ny1;
    if dlo > fy1 {
        fx1 = LO.wrapping_sub(y1) as f64 * dx / dy + fx1o;
        ny1 = LO;
    } else if fy1 > dhi {
        fx1 = fx1o - y1.wrapping_sub(HI) as f64 * dx / dy;
        ny1 = HI;
    } else {
        ny1 = d2i(fy1);
    }
    let (mut fx2, mut fy2) = (fx2o, fy2o);
    if x2 < LO {
        fy2 = fy2o + LO.wrapping_sub(x2) as f64 * dy / dx;
        fx2 = dlo;
    } else if x2 > HI {
        fy2 = fy2o - x2.wrapping_sub(HI) as f64 * dy / dx;
        fx2 = dhi;
    }
    let ny2;
    if dlo > fy2 {
        fx2 = LO.wrapping_sub(y2) as f64 * dx / dy + fx2o;
        ny2 = LO;
    } else if fy2 > dhi {
        fx2 = fx2o - y2.wrapping_sub(HI) as f64 * dx / dy;
        ny2 = HI;
    } else {
        ny2 = d2i(fy2);
    }
    (d2i(fx1), ny1, d2i(fx2), ny2)
}

/// `LineUtils_SetupBresenham`: `(startx, starty, steps, error, errmajor, bumpmajormask, errminor,
/// bumpminormask)`, or `None` when the line misses the bounds.
#[allow(clippy::type_complexity)]
fn setup_bresenham(
    mut x1: i32,
    mut y1: i32,
    mut x2: i32,
    mut y2: i32,
    shorten: i32,
    b: [i32; 4],
) -> Option<(i32, i32, i32, i32, i32, i32, i32, i32)> {
    let cxmin = b[0];
    let cymin = b[1];
    let cxmax = b[2].wrapping_sub(1);
    let cymax = b[3].wrapping_sub(1);
    if !(fits(x1, 15) && fits(y1, 15) && fits(x2, 15) && fits(y2, 15))
        && !(fits(x1, 30) && fits(y1, 30) && fits(x2, 30) && fits(y2, 30))
    {
        (x1, y1, x2, y2) = clamp_line(x1, y1, x2, y2);
    }
    let dx = x2.wrapping_sub(x1);
    let dy = y2.wrapping_sub(y1);
    let ax = if dx < 0 { dx.wrapping_neg() } else { dx };
    let ay = if dy < 0 { dy.wrapping_neg() } else { dy };
    let xmajor = ax >= ay;
    let outcode = |x: i32, y: i32| -> i32 {
        let mut c = if y < cymin {
            1
        } else if y > cymax {
            2
        } else {
            0
        };
        if x < cxmin {
            c |= 4;
        } else if x > cxmax {
            c |= 8;
        }
        c
    };
    let absu = |v: i32| -> i64 { (if v < 0 { v.wrapping_neg() } else { v }) as u32 as i64 };
    let (ax64, ay64) = (ax as i64, ay as i64);
    let two_ax = ax.wrapping_mul(2) as i64;
    let two_ay = ay.wrapping_mul(2) as i64;
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
                let ysteps = absu(ty1.wrapping_sub(y1));
                let mut xsteps = ysteps.wrapping_mul(ax64).wrapping_mul(2).wrapping_add(ay64);
                if xmajor {
                    xsteps = xsteps.wrapping_add(ay.wrapping_sub(ax).wrapping_sub(1) as i64);
                }
                xsteps = xsteps.wrapping_div(two_ay);
                if dx < 0 {
                    xsteps = xsteps.wrapping_neg();
                }
                tx1 = x1.wrapping_add(xsteps as i32);
            } else if code1 & 12 != 0 {
                tx1 = if code1 & 4 != 0 { cxmin } else { cxmax };
                let xsteps = absu(tx1.wrapping_sub(x1));
                let mut ysteps = xsteps.wrapping_mul(ay64).wrapping_mul(2).wrapping_add(ax64);
                if !xmajor {
                    ysteps = ysteps.wrapping_add(ax.wrapping_sub(ay).wrapping_sub(1) as i64);
                }
                ysteps = ysteps.wrapping_div(two_ax);
                if dy < 0 {
                    ysteps = ysteps.wrapping_neg();
                }
                ty1 = y1.wrapping_add(ysteps as i32);
            }
            code1 = outcode(tx1, ty1);
        } else {
            if code2 & 3 != 0 {
                ty2 = if code2 & 1 != 0 { cymin } else { cymax };
                let ysteps = absu(ty2.wrapping_sub(y2));
                let mut xsteps = ysteps.wrapping_mul(ax64).wrapping_mul(2).wrapping_add(ay64);
                if xmajor {
                    xsteps = xsteps.wrapping_add(ay.wrapping_sub(ax) as i64);
                } else {
                    xsteps = xsteps.wrapping_sub(1);
                }
                xsteps = xsteps.wrapping_div(two_ay);
                if dx > 0 {
                    xsteps = xsteps.wrapping_neg();
                }
                tx2 = x2.wrapping_add(xsteps as i32);
            } else if code2 & 12 != 0 {
                tx2 = if code2 & 4 != 0 { cxmin } else { cxmax };
                let xsteps = absu(tx2.wrapping_sub(x2));
                let mut ysteps = xsteps.wrapping_mul(ay64).wrapping_mul(2).wrapping_add(ax64);
                if xmajor {
                    ysteps = ysteps.wrapping_sub(1);
                } else {
                    ysteps = ysteps.wrapping_add(ax.wrapping_sub(ay) as i64);
                }
                ysteps = ysteps.wrapping_div(two_ax);
                if dy > 0 {
                    ysteps = ysteps.wrapping_neg();
                }
                ty2 = y2.wrapping_add(ysteps as i32);
            }
            code2 = outcode(tx2, ty2);
        }
    }
    let (errmajor, errminor, majormask, minormask, mut nax, mut nay, raw, unclipped);
    nax = ax;
    nay = ay;
    if xmajor {
        errmajor = ay.wrapping_mul(2);
        errminor = ax.wrapping_mul(2);
        majormask = if dx < 0 { BUMP_NEG_PIXEL } else { BUMP_POS_PIXEL };
        minormask = if dy < 0 { BUMP_NEG_SCAN } else { BUMP_POS_SCAN };
        nax = nax.wrapping_neg();
        raw = tx2.wrapping_sub(tx1);
        unclipped = x2 == tx2;
    } else {
        errmajor = ax.wrapping_mul(2);
        errminor = ay.wrapping_mul(2);
        majormask = if dy < 0 { BUMP_NEG_SCAN } else { BUMP_POS_SCAN };
        minormask = if dx < 0 { BUMP_NEG_PIXEL } else { BUMP_POS_PIXEL };
        nay = nay.wrapping_neg();
        raw = ty2.wrapping_sub(ty1);
        unclipped = y2 == ty2;
    }
    let mut steps = (if raw < 0 { raw.wrapping_neg() } else { raw }).wrapping_add(1);
    if unclipped {
        steps = steps.wrapping_sub(shorten);
        if steps == 0 {
            return None;
        }
    }
    let mut error = -(errminor / 2);
    if ty1 != y1 {
        let ysteps = ty1.wrapping_sub(y1);
        let ysteps = if ysteps < 0 { ysteps.wrapping_neg() } else { ysteps };
        error = error.wrapping_add(ysteps.wrapping_mul(nax).wrapping_mul(2));
    }
    if tx1 != x1 {
        let xsteps = tx1.wrapping_sub(x1);
        let xsteps = if xsteps < 0 { xsteps.wrapping_neg() } else { xsteps };
        error = error.wrapping_add(xsteps.wrapping_mul(nay).wrapping_mul(2));
    }
    error = error.wrapping_add(errmajor);
    let errminor = errminor.wrapping_sub(errmajor);
    Some((tx1, ty1, steps, error, errmajor, majormask, errminor, minormask))
}

/// `LineUtils_ProcessLine(..., shorten)` within the bounds `b`.
#[allow(clippy::too_many_arguments)]
fn process_line(raster: &mut [i32], scan: usize, pixel: i32, b: [i32; 4], x1: i32, y1: i32, x2: i32, y2: i32, shorten: bool) {
    if y1 == y2 {
        if y1 >= b[1] && y1 < b[3] {
            let (mut tx1, mut tx2);
            if x1 < x2 {
                tx1 = x1;
                tx2 = x2;
                if shorten {
                    tx2 = tx2.wrapping_sub(1);
                }
            } else {
                tx1 = x2;
                tx2 = x1;
                if shorten {
                    tx1 = tx1.wrapping_add(1);
                }
            }
            let t = tx2.wrapping_add(1);
            if t >= tx1 {
                tx2 = t;
            }
            if tx1 < b[0] {
                tx1 = b[0];
            }
            if tx2 > b[2] {
                tx2 = b[2];
            }
            if tx1 < tx2 {
                any_int_set_line(raster, scan, tx1, y1, pixel, tx2 - tx1, 0, BUMP_POS_PIXEL, 0, BUMP_NOOP, 0);
            }
        }
    } else if x1 == x2 {
        if x1 >= b[0] && x1 < b[2] {
            let (mut ty1, mut ty2);
            if y1 < y2 {
                ty1 = y1;
                ty2 = y2;
                if shorten {
                    ty2 = ty2.wrapping_sub(1);
                }
            } else {
                ty1 = y2;
                ty2 = y1;
                if shorten {
                    ty1 = ty1.wrapping_add(1);
                }
            }
            let t = ty2.wrapping_add(1);
            if t >= ty1 {
                ty2 = t;
            }
            if ty1 < b[1] {
                ty1 = b[1];
            }
            if ty2 > b[3] {
                ty2 = b[3];
            }
            if ty1 < ty2 {
                any_int_set_line(raster, scan, x1, ty1, pixel, ty2 - ty1, 0, BUMP_POS_SCAN, 0, BUMP_NOOP, 0);
            }
        }
    } else if let Some((tx, ty, steps, error, errmajor, majormask, errminor, minormask)) =
        setup_bresenham(x1, y1, x2, y2, shorten as i32, b)
    {
        any_int_set_line(raster, scan, tx, ty, pixel, steps, error, majormask, errmajor, minormask, errminor);
    }
}

/// `Java_sun_java2d_loops_DrawParallelogram_DrawParallelogram` (AnyInt SrcNoEa).
#[allow(clippy::too_many_arguments)]
pub fn draw_parallelogram(
    t: &mut FillTarget,
    mut px0: f64,
    mut py0: f64,
    mut dx1: f64,
    mut dy1: f64,
    mut dx2: f64,
    mut dy2: f64,
    mut lw1: f64,
    mut lw2: f64,
) {
    if sort_pgram(&mut px0, &mut py0, &mut dx1, &mut dy1, &mut dx2, &mut dy2) {
        std::mem::swap(&mut lw1, &mut lw2);
    }
    let ldx1 = dx1 * lw1;
    let ldy1 = dy1 * lw1;
    let ldx2 = dx2 * lw2;
    let ldy2 = dy2 * lw2;
    let ox0 = px0 - (ldx1 + ldx2) / 2.0;
    let oy0 = py0 - (ldy1 + ldy2) / 2.0;

    let (ix1, ix2) = pgram_min_max(ox0, ox0 + dx1 + ldx1, dx1 + ldx1, dx2 + ldx2);
    let iy1 = ifloor(oy0 + 0.5);
    let iy2 = ifloor(oy0 + dy1 + ldy1 + dy2 + ldy2 + 0.5);
    let b = intersect_bounds(t, ix1, iy1, ix2, iy2);
    if b[3] <= b[1] || b[2] <= b[0] {
        return;
    }
    let [ix1, iy1, ix2, iy2] = b;
    let (scan, pixel) = (t.scan, t.pixel);
    let raster = &mut *t.raster;

    let lslope = if dy1 == 0.0 { 0.0 } else { dx1 / dy1 };
    let rslope = if dy2 == 0.0 { 0.0 } else { dx2 / dy2 };
    let ldx = dbl_to_long(lslope);
    let rdx = dbl_to_long(rslope);

    let mut edges = [EdgeInfo::default(); 8];
    let numedges;
    if lw1 < 1.0 && lw2 < 1.0 {
        let w1 = (ldx1 * ldx1 + ldy1 * ldy1).sqrt();
        let w2 = (ldx2 * ldx2 + ldy2 * ldy2).sqrt();
        if w1 <= 1.0001 && w2 <= 1.0001 {
            // thin stroke: the four sides as single-pixel lines, each without its end point
            let ix0 = ifloor(px0);
            let iy0 = ifloor(py0);
            let ix1p = ifloor(px0 + dx1);
            let iy1p = ifloor(py0 + dy1);
            let ix3 = ifloor(px0 + dx1 + dx2);
            let iy3 = ifloor(py0 + dy1 + dy2);
            let ix2p = ifloor(px0 + dx2);
            let iy2p = ifloor(py0 + dy2);
            process_line(raster, scan, pixel, b, ix0, iy0, ix1p, iy1p, true);
            process_line(raster, scan, pixel, b, ix1p, iy1p, ix3, iy3, true);
            process_line(raster, scan, pixel, b, ix3, iy3, ix2p, iy2p, true);
            process_line(raster, scan, pixel, b, ix2p, iy2p, ix0, iy0, true);
            return;
        }
        store_pgram(
            &mut edges,
            2,
            4,
            ox0 + ldx1 + ldx2,
            oy0 + ldy1 + ldy2,
            dx1 - ldx1,
            dy1 - ldy1,
            dx2 - ldx2,
            dy2 - ldy2,
            lslope,
            rslope,
            ldx,
            rdx,
        );
        store_pgram(&mut edges, 0, 6, ox0, oy0, dx1 + ldx1, dy1 + ldy1, dx2 + ldx2, dy2 + ldy2, lslope, rslope, ldx, rdx);
        numedges = 8;
    } else {
        store_pgram(&mut edges, 0, 2, ox0, oy0, dx1 + ldx1, dy1 + ldy1, dx2 + ldx2, dy2 + ldy2, lslope, rslope, ldx, rdx);
        numedges = 4;
    }

    let mut loy = edges[0].ystart;
    if loy < iy1 {
        loy = iy1;
    }
    let mut active = [0usize; 8];
    while loy < iy2 {
        let mut numactive = 0;
        let mut hiy = iy2;
        for (cur, e) in edges.iter().enumerate().take(numedges) {
            if loy < e.yend {
                if loy < e.ystart {
                    if hiy > e.ystart {
                        hiy = e.ystart;
                    }
                } else {
                    active[numactive] = cur;
                    numactive += 1;
                    if hiy > e.yend {
                        hiy = e.yend;
                    }
                }
            }
        }
        let mut cur = 0;
        while cur < numactive {
            let l = edges[active[cur]];
            let r = edges[active[cur + 1]];
            let lx = pgram_init_x(loy, l.x0, l.y0, l.slope);
            let rx = pgram_init_x(loy, r.x0, r.y0, r.slope);
            any_int_set_parallelogram(raster, scan, ix1, loy, ix2, hiy, lx, l.dx, rx, r.dx, pixel);
            cur += 2;
        }
        loy = hiy;
    }
}
