//! `sun.awt.image.ImagingLib.convolveRaster` as OpenJDK 17 runs it on x86_64: libawt's
//! `awt_ImagingLib.c` driving libmlib_image's `mlib_ImageConvMxN` on a direct byte view of a
//! 4x8-bit packed int raster.

const MLIB_BYTE_CHANNELS: usize = 4;

/// `ConvolveOp(kernel).filter(src, dst)` for 4x8-bit packed int rasters (scan = width, origin 0): `ImagingLib.convolveRaster`.
pub fn convolve_raster(
    src: &[i32],
    dst: &mut [i32],
    width: usize,
    height: usize,
    kernel_width: usize,
    kernel_height: usize,
    kernel: &[f32],
) -> Result<(), &'static str> {
    const ERR: &str = "Unable to convolve src image";
    assert!(kernel_width > 0 && kernel_height > 0);
    let kwidth = kernel_width as i32;
    let kheight = kernel_height as i32;
    let klen = kwidth * kheight;
    let kern = &kernel[..klen as usize];

    let w = if kwidth & 1 == 0 { kwidth + 1 } else { kwidth };
    let h = if kheight & 1 == 0 { kheight + 1 } else { kheight };
    if !(w > 0 && h > 0 && (i32::MAX / w) / h > 8) {
        return Err(ERR);
    }
    let mut dkern = vec![0f64; (w * h) as usize];

    let mut kmax = kern[klen as usize - 1];
    let mut i = klen - 1;
    for y in 0..kheight {
        for x in 0..kwidth {
            let k = kern[i as usize];
            dkern[(y * w + x) as usize] = k as f64;
            if k > kmax {
                kmax = k;
            }
            i -= 1;
        }
    }
    if kmax > 65536.0 {
        return Err(ERR);
    }

    if width == 0 || height == 0 {
        return Err(ERR);
    }
    let n_pix = width * height;
    let src_img: Vec<u8> = src[..n_pix].iter().flat_map(|p| p.to_le_bytes()).collect();
    let mut dst_img: Vec<u8> = dst[..n_pix].iter().flat_map(|p| p.to_le_bytes()).collect();

    let mut kdata = vec![0i32; (w * h) as usize];
    let scale = mlib_image_conv_kernel_convert(&mut kdata, &dkern, w, h).ok_or(ERR)?;

    let cmask = (1 << MLIB_BYTE_CHANNELS) - 1;
    let ok = mlib_image_conv_mxn(
        &mut dst_img,
        &src_img,
        width as i32,
        height as i32,
        &kdata,
        w,
        h,
        (w - 1) / 2,
        (h - 1) / 2,
        scale,
        cmask,
    );

    for (p, b) in dst[..n_pix].iter_mut().zip(dst_img.chunks_exact(4)) {
        *p = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    }
    if ok { Ok(()) } else { Err(ERR) }
}

/// `(mlib_s32)x` as `cvttsd2si`: out of range and NaN give `0x80000000`.
fn cvtt(x: f64) -> i32 {
    if x >= -2147483648.0 && x < 2147483648.0 { x as i32 } else { i32::MIN }
}

/// `CLAMP_S32(dst, src)`.
fn clamp_s32(x: f64) -> i32 {
    if x > 2147483647.0 {
        i32::MAX
    } else if x < -2147483648.0 {
        i32::MIN
    } else {
        cvtt(x)
    }
}

/// `mlib_ilogb`.
fn mlib_ilogb(x: f64) -> i32 {
    if x == 0.0 {
        return 0x8000_0001u32 as i32;
    }
    let e = ((x.to_bits() >> 32) as i32) & 0x7ff0_0000;
    if e == 0 {
        let y = x * 4503599627370496.0;
        return (((y.to_bits() >> 52) & 0x7ff) as i32) - 0x433;
    }
    if e == 0x7ff0_0000 { i32::MAX } else { (e >> 20) - 0x3ff }
}

/// `mlib_ImageConvVersion` (MLIB_BYTE): 0 selects the floating-point kernels, 2 the integer ones.
fn mlib_image_conv_version(m: i32, n: i32, scale: i32) -> i32 {
    let dscale = 1.0 / (1i32.wrapping_shl(scale as u32) as f64);
    if (m.wrapping_mul(n) as f64) * dscale * 32768.0 > 8.0 { 0 } else { 2 }
}

/// `mlib_ImageConvKernelConvert` (MLIB_BYTE): returns `iscale`, `None` for `MLIB_FAILURE`.
fn mlib_image_conv_kernel_convert(ikernel: &mut [i32], fkernel: &[f64], m: i32, n: i32) -> Option<i32> {
    let mn = (m * n) as usize;
    let fkernel = &fkernel[..mn];
    let ikernel = &mut ikernel[..mn];

    let mut sum_pos = 0.0f64;
    let mut sum_neg = 0.0f64;
    for &f in fkernel {
        if f > 0.0 { sum_pos += f } else { sum_neg -= f }
    }
    let sum = if sum_pos > sum_neg { sum_pos } else { sum_neg };
    let mut scale = 30i32.wrapping_sub(mlib_ilogb(sum));
    if scale <= 16 {
        return None;
    }
    if scale > 31 {
        scale = 31;
    }

    if mlib_image_conv_version(m, n, scale) == 0 {
        let norm = (1u32 << scale) as f64;
        for (ik, &f) in ikernel.iter_mut().zip(fkernel) {
            *ik = clamp_s32(f * norm);
        }
        return Some(scale);
    }

    let scale1 = 8;
    let norm = (1i32 << (scale - scale1)) as f64;
    for (ik, &f) in ikernel.iter_mut().zip(fkernel) {
        *ik = if f > 0.0 { cvtt(f * norm + 0.5) } else { cvtt(f * norm - 0.5) };
    }

    let mut isum_pos = 0i32;
    let mut isum_neg = 0i32;
    for &ik in ikernel.iter() {
        if ik > 0 { isum_pos = isum_pos.wrapping_add(ik) } else { isum_neg = isum_neg.wrapping_sub(ik) }
    }
    let isum = if isum_pos >= isum_neg { isum_pos } else { isum_neg };

    if isum >= 1 << (31 - scale1) {
        for (ik, &f) in ikernel.iter_mut().zip(fkernel) {
            *ik = cvtt(f * norm).wrapping_shl(scale1 as u32);
        }
    } else {
        for ik in ikernel.iter_mut() {
            *ik = ik.wrapping_shl(scale1 as u32);
        }
    }
    Some(scale)
}

/// `mlib_ImageConvMxN` -> `mlib_ImageConvMxN_f` with `MLIB_EDGE_DST_FILL_ZERO`, src and dst
/// the same size (`mlib_ImageClippingMxN` then leaves both whole). Returns `MLIB_SUCCESS`.
#[allow(clippy::too_many_arguments)]
fn mlib_image_conv_mxn(
    dst: &mut [u8],
    src: &[u8],
    width: i32,
    height: i32,
    kernel: &[i32],
    m: i32,
    n: i32,
    dm: i32,
    dn: i32,
    scale: i32,
    cmask: i32,
) -> bool {
    if !(16..=31).contains(&scale) || m < 1 || n < 1 || dm < 0 || dm > m - 1 || dn < 0 || dn > n - 1 {
        return false;
    }
    let (dx_l, dx_r, dy_t, dy_b) = (dm, m - 1 - dm, dn, n - 1 - dn);
    if cmask & ((1 << MLIB_BYTE_CHANNELS) - 1) == 0 {
        return true;
    }

    let mut ret = true;
    if width >= m && height >= n {
        ret = mlib_conv_mxn_nw_u8(dst, src, width, height, kernel, m, n, dm, dn, scale, cmask);
    }
    if dx_l + dx_r + dy_t + dy_b != 0 {
        mlib_image_conv_clear_edge(dst, width, height, dx_l, dx_r, dy_t, dy_b, cmask);
    }
    ret
}

/// `mlib_convMxNnw_u8`.
#[allow(clippy::too_many_arguments)]
fn mlib_conv_mxn_nw_u8(
    dst: &mut [u8],
    src: &[u8],
    width: i32,
    height: i32,
    kernel: &[i32],
    m: i32,
    n: i32,
    dm: i32,
    dn: i32,
    scale: i32,
    cmask: i32,
) -> bool {
    let (width, height) = (width as usize, height as usize);
    let geom = NwGeometry { width, height, m: m as usize, n: n as usize, dm: dm as usize, dn: dn as usize, cmask };
    if mlib_image_conv_version(m, n, scale) == 0 {
        mlib_c_conv_mxn_nw_u8(dst, src, &geom, kernel, scale)
    } else {
        mlib_i_conv_mxn_nw_u8(dst, src, &geom, kernel, scale)
    }
}

struct NwGeometry {
    width: usize,
    height: usize,
    m: usize,
    n: usize,
    dm: usize,
    dn: usize,
    cmask: i32,
}

/// The "nw" loop both u8 versions share: `dst[dn + j][dm + i] = store(taps of src[j..j + n][i..i + m])`.
fn conv_nw_u8(dst: &mut [u8], src: &[u8], g: &NwGeometry, mut pixel: impl FnMut(&mut dyn FnMut(usize, usize) -> u8) -> u8) {
    let nch = MLIB_BYTE_CHANNELS;
    let sll = g.width * nch;
    let wid = g.width - (g.m - 1);
    let hgt = g.height - (g.n - 1);
    let dst_off = g.dn * sll + g.dm * nch;
    for c in 0..nch {
        if g.cmask & (1 << (nch - 1 - c)) == 0 {
            continue;
        }
        for j in 0..hgt {
            for i in 0..wid {
                let base = j * sll + i * nch + c;
                let mut tap = |l: usize, off: usize| src[base + l * sll + off * nch];
                dst[dst_off + j * sll + i * nch + c] = pixel(&mut tap);
            }
        }
    }
}

/// `mlib_i_convMxNnw_u8`: s32 taps of `kernel >> 8`, then `x >>= scale - 8; CLAMP_STORE`.
fn mlib_i_conv_mxn_nw_u8(dst: &mut [u8], src: &[u8], g: &NwGeometry, kernel: &[i32], scale: i32) -> bool {
    let (m, n) = (g.m, g.n);
    let k_loc: Vec<i32> = kernel[..m * n].iter().map(|&k| k >> 8).collect();
    let shift2 = scale - 8;
    conv_nw_u8(dst, src, g, |tap| {
        let mut s = 0i32;
        for l in 0..n {
            for off in 0..m {
                s = s.wrapping_add(k_loc[l * m + off].wrapping_mul(tap(l, off) as i32));
            }
        }
        let x = s >> shift2;
        if x as u32 <= 255 { x as u8 } else { (!x >> 31) as u8 }
    });
    true
}

/// `mlib_c_convMxNnw_u8` (and its inlined 1xN case): d64 taps of `kernel * 2^24 / 2^scale`, then
/// `D2I(x - 2^31)` and `(d >> 24) ^ 128`. Every product and partial sum is a multiple of
/// `2^(24 - scale)` below 2^41 of them, so the d64 sum is exact in any order: summed here in i64.
fn mlib_c_conv_mxn_nw_u8(dst: &mut [u8], src: &[u8], g: &NwGeometry, kernel: &[i32], scale: i32) -> bool {
    let (m, n) = (g.m, g.n);
    let (mut fscale, mut scale) = (16777216.0f64, scale);
    if scale > 30 {
        fscale = 1.0 / 64.0;
        scale -= 30;
    }
    fscale /= (1i32 << scale) as f64;
    conv_nw_u8(dst, src, g, |tap| {
        let mut s = 0i64;
        for l in 0..n {
            for off in 0..m {
                s += kernel[l * m + off] as i64 * tap(l, off) as i64;
            }
        }
        let v = s as f64 * fscale - 2147483648.0;
        if -2147483648.0 >= v {
            0
        } else if v >= 2147483647.0 {
            0xff
        } else {
            ((cvtt(v) >> 24) - 128) as u8
        }
    });
    true
}

/// `mlib_ImageConvZeroEdge` -> `mlib_ImageConvClearEdge` (MLIB_BYTE, color 0).
#[allow(clippy::too_many_arguments)]
fn mlib_image_conv_clear_edge(
    img: &mut [u8],
    width: i32,
    height: i32,
    mut dx_l: i32,
    mut dx_r: i32,
    mut dy_t: i32,
    mut dy_b: i32,
    cmask: i32,
) {
    if dx_l + dx_r > width {
        dx_l = width;
        dx_r = 0;
    }
    if dy_t + dy_b > height {
        dy_t = height;
        dy_b = 0;
    }
    let nch = MLIB_BYTE_CHANNELS;
    for c in 0..nch {
        if cmask & (1 << (nch - 1 - c)) == 0 {
            continue;
        }
        for y in 0..height {
            for x in 0..width {
                if y < dy_t || y >= height - dy_b || x < dx_l || x >= width - dx_r {
                    img[(y * width + x) as usize * nch + c] = 0;
                }
            }
        }
    }
}

/// `ImagingLib.transformBI` with nearest-neighbour interpolation (`mlib_ImageAffine`, `MLIB_NEAREST`): each row's run of
/// destination pixels whose centre maps inside the source, walked in 16.16 fixed point from the run's first pixel.
/// `m` is the forward transform `m00, m10, m01, m11, m02, m12`; pixels outside the run are left as they are.
pub fn transform_nearest(src: &[i32], sw: usize, sh: usize, dst: &mut [i32], dw: usize, dh: usize, m: [f64; 6]) {
    let [m00, m10, m01, m11, m02, m12] = m;
    let det = m00 * m11 - m01 * m10;
    let (i00, i01, i10, i11) = (m11 / det, -m01 / det, -m10 / det, m00 / det);
    let d_x = (i00 * 65536.0) as i32;
    let d_y = (i10 * 65536.0) as i32;
    let (swf, shf) = (sw as f64, sh as f64);
    for y in 0..dh {
        let map = |x: usize| {
            let (px, py) = (x as f64 + 0.5 - m02, y as f64 + 0.5 - m12);
            (i00 * px + i01 * py, i10 * px + i11 * py)
        };
        let inside = |(u, v): (f64, f64)| (0.0..swf).contains(&u) && (0.0..shf).contains(&v);
        let Some(x0) = (0..dw).find(|&x| inside(map(x))) else { continue };
        let x1 = (0..dw).rev().find(|&x| inside(map(x))).unwrap();
        let (u0, v0) = map(x0);
        let (mut fx, mut fy) = ((u0 * 65536.0) as i32, (v0 * 65536.0) as i32);
        for x in x0..=x1 {
            let (sx, sy) = (fx >> 16, fy >> 16);
            if sx >= 0 && (sx as usize) < sw && sy >= 0 && (sy as usize) < sh {
                dst[y * dw + x] = src[sy as usize * sw + sx as usize];
            }
            fx = fx.wrapping_add(d_x);
            fy = fy.wrapping_add(d_y);
        }
    }
}
