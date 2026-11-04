//! Port of JDK 17 libawt `IntArgb.h` / `AnyByteBinary`-style loops needed for `TYPE_INT_ARGB`:
//! the native `IntArgbToIntArgbSrcOverMaskBlit` (with and without a mask) and the
//! `IntRgbToIntArgbConvert` Blit (`SrcNoEa`).
//!
//! Strides and offsets are in `i32` pixels (the natural unit for an `&[i32]` raster), matching the
//! native byte arithmetic divided by the 4-byte pixel stride. `extra_alpha` is
//! `AlphaComposite.getAlpha()`; the native `CompositeInfo`/loop converts it with
//! `(jint)((jdouble)extraAlpha * 255.0 + 0.5)`, which this reproduces.

use super::alpha_math::{div8, mul8};

/// `AlphaComposite.getAlpha()` -> the loop's integer extra alpha, exactly as the native
/// `CompositeInfo.alpha` handling does it.
#[inline]
pub fn extra_alpha_to_int(extra_alpha: f32) -> i32 {
    ((extra_alpha as f64) * 255.0 + 0.5) as i32
}

/// `IntArgbToIntArgbSrcOverMaskBlit`.
///
/// `mask` is `Some((pMask, maskoff, maskscan))` for the masked loop, or `None` for the no-mask loop
/// — the form `Blit$GeneralMaskBlit.Blit` invokes (it passes `mask=null, maskoff=0, maskscan=0`).
/// `extra_alpha_i` is the already-converted integer extra alpha (see [`extra_alpha_to_int`]).
#[allow(clippy::too_many_arguments)]
pub fn int_argb_to_int_argb_srcover_maskblit(
    src: &[i32],
    src_off: usize,
    src_scan: usize,
    dst: &mut [i32],
    dst_off: usize,
    dst_scan: usize,
    mask: Option<(&[u8], usize, usize)>,
    width: usize,
    height: usize,
    extra_alpha_i: i32,
) {
    let mut s = src_off;
    let mut d = dst_off;
    let (mut m, mask_scan) = match mask {
        Some((_, moff, mscan)) => (moff, mscan),
        None => (0, 0),
    };
    for _ in 0..height {
        for x in 0..width {
            let path_a = match mask {
                Some((mbuf, _, _)) => {
                    let mval = mbuf[m + x] as i32;
                    if mval == 0 {
                        continue;
                    }
                    mul8(mval, extra_alpha_i)
                }
                None => extra_alpha_i,
            };
            let spix = src[s + x];
            let src_alpha = (spix >> 24) & 0xff;
            let sa = mul8(path_a, src_alpha);
            if sa == 0 {
                continue;
            }
            let sr = (spix >> 16) & 0xff;
            let sg = (spix >> 8) & 0xff;
            let sb = spix & 0xff;
            let (res_a, mut r, mut g, mut b);
            if sa == 0xff {
                res_a = 0xff;
                r = sr;
                g = sg;
                b = sb;
            } else {
                let dpix = dst[d + x];
                let da = (dpix >> 24) & 0xff;
                let dr = (dpix >> 16) & 0xff;
                let dg = (dpix >> 8) & 0xff;
                let db = dpix & 0xff;
                let inv = mul8(0xff - sa, da);
                res_a = sa + inv;
                r = mul8(sa, sr) + mul8(inv, dr);
                g = mul8(sa, sg) + mul8(inv, dg);
                b = mul8(sa, sb) + mul8(inv, db);
                if res_a <= 0xfe {
                    r = div8(res_a, r);
                    g = div8(res_a, g);
                    b = div8(res_a, b);
                }
            }
            dst[d + x] = (res_a << 24) | (r << 16) | (g << 8) | b;
        }
        s += src_scan;
        d += dst_scan;
        m += mask_scan;
    }
}

/// `IntRgbToIntArgbConvert` (the `Blit` `IntRgb`->`IntArgb` `SrcNoEa`): copy, forcing full alpha.
#[allow(clippy::too_many_arguments)]
pub fn int_rgb_to_int_argb_convert(
    src: &[i32],
    src_off: usize,
    src_scan: usize,
    dst: &mut [i32],
    dst_off: usize,
    dst_scan: usize,
    width: usize,
    height: usize,
) {
    let mut s = src_off;
    let mut d = dst_off;
    for _ in 0..height {
        for x in 0..width {
            dst[d + x] = src[s + x] | 0xff00_0000u32 as i32;
        }
        s += src_scan;
        d += dst_scan;
    }
}
