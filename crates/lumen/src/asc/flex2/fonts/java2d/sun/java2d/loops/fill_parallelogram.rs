//! Port of JDK 17 libawt `FillParallelogram.c` (`Java_sun_java2d_loops_FillParallelogram_FillParallelogram`)
//! and the AnyInt SrcNoEa span filler `AnyIntSetParallelogram`, for an `i32` raster.
//!
//! The entry receives the parallelogram as `PixelToParallelogramConverter` hands it to the loop: an
//! origin `(x0,y0)` and two edge vectors. Edges are 32.32 fixed point (`DblToLong`), sampled at pixel
//! centres; `PGRAM_INIT_X` adds `LongOneHalf - 1` so the loop's `>> 32` rounds.

/// The loop's destination: the raster (pixel `(x,y)` at `y * scan + x`), its clip box
/// (`SurfaceDataBounds`: the composite clip intersected with the surface) and the pixel value.
pub struct FillTarget<'a> {
    pub raster: &'a mut [i32],
    pub scan: usize,
    pub clip_lox: i32,
    pub clip_loy: i32,
    pub clip_hix: i32,
    pub clip_hiy: i32,
    pub pixel: i32,
}

/// `cvttsd2si` to `jint` (out of range / NaN -> `0x80000000`).
#[inline]
pub fn d2i(v: f64) -> i32 {
    if v > -2147483649.0 && v < 2147483648.0 {
        v as i32
    } else {
        i32::MIN
    }
}

/// `(jint) floor(v)`.
#[inline]
pub fn ifloor(v: f64) -> i32 {
    d2i(v.floor())
}

/// `DblToLong`: `(jlong)(v * 2^32)` with `cvttsd2si` overflow semantics.
#[inline]
pub fn dbl_to_long(v: f64) -> i64 {
    let d = v * 4294967296.0;
    if d >= -9223372036854775808.0 && d < 9223372036854775808.0 {
        d as i64
    } else {
        i64::MIN
    }
}

/// `PGRAM_INIT_X(starty, x, y, slope)`.
#[inline]
pub fn pgram_init_x(starty: i32, x: f64, y: f64, slope: f64) -> i64 {
    dbl_to_long(x + slope * ((starty as f64 + 0.5) - y)).wrapping_add(0x7fff_ffff)
}

/// `SurfaceData_IntersectBoundsXYXY` of the clip box with a primitive's device bounds.
#[inline]
pub fn intersect_bounds(t: &FillTarget, x1: i32, y1: i32, x2: i32, y2: i32) -> [i32; 4] {
    let mut b = [t.clip_lox, t.clip_loy, t.clip_hix, t.clip_hiy];
    if b[0] < x1 {
        b[0] = x1;
    }
    if b[1] < y1 {
        b[1] = y1;
    }
    if b[2] > x2 {
        b[2] = x2;
    }
    if b[3] > y2 {
        b[3] = y2;
    }
    b
}

/// `AnyIntSetParallelogram`: for each row in `[loy, hiy)` set `[max(lox, leftx>>32), min(hix,
/// rightx>>32))`, then step both edges.
#[allow(clippy::too_many_arguments)]
pub fn any_int_set_parallelogram(
    raster: &mut [i32],
    scan: usize,
    lox: i32,
    mut loy: i32,
    hix: i32,
    hiy: i32,
    mut leftx: i64,
    dleftx: i64,
    mut rightx: i64,
    drightx: i64,
    pixel: i32,
) {
    while loy < hiy {
        let mut lx = (leftx >> 32) as i32;
        let mut rx = (rightx >> 32) as i32;
        if lx < lox {
            lx = lox;
        }
        if rx > hix {
            rx = hix;
        }
        if lx < rx {
            let row = loy as i64 * scan as i64;
            for x in lx..rx {
                raster[(row + x as i64) as usize] = pixel;
            }
        }
        leftx = leftx.wrapping_add(dleftx);
        rightx = rightx.wrapping_add(drightx);
        loy += 1;
    }
}

/// `SORT_PGRAM`: give both edge vectors a non-negative y and put edge 1 to the left of edge 2.
/// Returns whether the edges were swapped.
#[inline]
pub fn sort_pgram(x0: &mut f64, y0: &mut f64, dx1: &mut f64, dy1: &mut f64, dx2: &mut f64, dy2: &mut f64) -> bool {
    if *dy1 < 0.0 {
        *x0 += *dx1;
        *y0 += *dy1;
        *dx1 = -*dx1;
        *dy1 = -*dy1;
    }
    if *dy2 < 0.0 {
        *x0 += *dx2;
        *y0 += *dy2;
        *dx2 = -*dx2;
        *dy2 = -*dy2;
    }
    if *dx1 * *dy2 > *dx2 * *dy1 {
        std::mem::swap(dx1, dx2);
        std::mem::swap(dy1, dy2);
        true
    } else {
        false
    }
}

/// `PGRAM_MIN_MAX(bmin, bmax, v0, dv1, dv2, JNI_FALSE)` with `v0 + dv1` evaluated as `v0dv1`
/// (callers pass the macro's unparenthesised sum) and `dv1`, `dv2` as the tested deltas.
#[inline]
pub fn pgram_min_max(v0: f64, v0dv1: f64, dv1: f64, dv2: f64) -> (i32, i32) {
    let (mut vmin, mut vmax) = if dv1 < 0.0 { (v0dv1, v0) } else { (v0, v0dv1) };
    if dv2 < 0.0 {
        vmin += dv2;
    } else {
        vmax += dv2;
    }
    (ifloor(vmin + 0.5), ifloor(vmax + 0.5))
}

/// `Java_sun_java2d_loops_FillParallelogram_FillParallelogram` (AnyInt SrcNoEa).
#[allow(clippy::too_many_arguments)]
pub fn fill_parallelogram(
    t: &mut FillTarget,
    mut x0: f64,
    mut y0: f64,
    mut dx1: f64,
    mut dy1: f64,
    mut dx2: f64,
    mut dy2: f64,
) {
    if (dy1 == 0.0 && dx1 == 0.0) || (dy2 == 0.0 && dx2 == 0.0) {
        return;
    }
    sort_pgram(&mut x0, &mut y0, &mut dx1, &mut dy1, &mut dx2, &mut dy2);

    let (ix1, ix2) = pgram_min_max(x0, x0 + dx1, dx1, dx2);
    let iy1 = ifloor(y0 + 0.5);
    let iy2 = ifloor(y0 + dy1 + dy2 + 0.5);
    let [ix1, iy1, ix2, iy2] = intersect_bounds(t, ix1, iy1, ix2, iy2);
    if iy2 <= iy1 || ix2 <= ix1 {
        return;
    }

    let lslope = if dy1 == 0.0 { 0.0 } else { dx1 / dy1 };
    let rslope = if dy2 == 0.0 { 0.0 } else { dx2 / dy2 };
    let ldx = dbl_to_long(lslope);
    let rdx = dbl_to_long(rslope);
    let (ax, ay) = (x0 + dx1, y0 + dy1);
    let (bx, by) = (x0 + dx2, y0 + dy2);
    let cy1 = ifloor(ay + 0.5);
    let cy2 = ifloor(by + 0.5);
    let (raster, scan, pixel) = (&mut *t.raster, t.scan, t.pixel);

    // top triangle
    let mut hiy = if cy1 <= cy2 { cy1 } else { cy2 };
    if hiy > iy2 {
        hiy = iy2;
    }
    if hiy > iy1 {
        let lx = pgram_init_x(iy1, x0, y0, lslope);
        let rx = pgram_init_x(iy1, x0, y0, rslope);
        any_int_set_parallelogram(raster, scan, ix1, iy1, ix2, hiy, lx, ldx, rx, rdx, pixel);
    }

    // middle parallelogram, slanting with whichever edge is longer
    if cy1 < cy2 {
        let loy = if iy1 >= cy1 { iy1 } else { cy1 };
        let hiy = if iy2 <= cy2 { iy2 } else { cy2 };
        if hiy > loy {
            let lx = pgram_init_x(loy, ax, ay, rslope);
            let rx = pgram_init_x(loy, x0, y0, rslope);
            any_int_set_parallelogram(raster, scan, ix1, loy, ix2, hiy, lx, rdx, rx, rdx, pixel);
        }
    } else if cy1 > cy2 {
        let loy = if iy1 >= cy2 { iy1 } else { cy2 };
        let hiy = if iy2 <= cy1 { iy2 } else { cy1 };
        if hiy > loy {
            let lx = pgram_init_x(loy, x0, y0, lslope);
            let rx = pgram_init_x(loy, bx, by, lslope);
            any_int_set_parallelogram(raster, scan, ix1, loy, ix2, hiy, lx, ldx, rx, ldx, pixel);
        }
    }

    // bottom triangle
    let m = if cy1 < cy2 { cy2 } else { cy1 };
    let loy = if m >= iy1 { m } else { iy1 };
    if iy2 > loy {
        let lx = pgram_init_x(loy, ax, ay, rslope);
        let rx = pgram_init_x(loy, bx, by, lslope);
        any_int_set_parallelogram(raster, scan, ix1, loy, ix2, iy2, lx, rdx, rx, ldx, pixel);
    }
}
