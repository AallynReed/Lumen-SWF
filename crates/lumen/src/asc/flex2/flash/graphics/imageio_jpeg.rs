//! The JPEG `ImageIO` writer as `LosslessImageTranscoder.bufferedImageToJPEG` drives it: `JPEGImageWriter` with an
//! explicit quality, an RGB destination type and no metadata (so a default JFIF segment, the scaled K.1/K.2 tables,
//! the standard Huffman tables and 2x2 chroma subsampling), over the IJG libjpeg 6b compressor the JDK bundles
//! (`jccolor`, `jcsample`, `jcprepct`, `jccoefct`, `jfdctint`, `jcdctmgr`, `jchuff`, `jcmarker`).

const K1: [i32; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56,
    68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

const K2: [i32; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

const NATURAL_ORDER: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43,
    36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

const DC_LUM_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_CHROM_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const AC_LUM_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
const AC_LUM_VALS: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08,
    0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
    0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6,
    0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
    0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
];
const AC_CHROM_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
const AC_CHROM_VALS: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91,
    0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
    0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
    0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4,
    0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
    0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
];

/// `JPEG.convertToLinearQuality`.
fn convert_to_linear_quality(mut quality: f32) -> f32 {
    if quality <= 0.0 {
        quality = 0.01;
    }
    if quality > 1.0 {
        quality = 1.0;
    }
    if quality < 0.5 {
        0.5 / quality
    } else {
        2.0 - quality * 2.0
    }
}

/// `JPEGQTable.getScaledInstance(scaleFactor, true)`.
fn scaled(table: &[i32; 64], scale: f32) -> [i32; 64] {
    let mut t = [0; 64];
    for i in 0..64 {
        t[i] = ((table[i] as f32 * scale + 0.5f32) as i32).clamp(1, 255);
    }
    t
}

/// `jpeg_make_c_derived_tbl`: code and size per symbol.
struct DerivedTable {
    code: [u32; 256],
    size: [u8; 256],
}

fn derive(bits: &[u8; 16], vals: &[u8]) -> DerivedTable {
    let mut huffsize = Vec::new();
    for (l, &n) in bits.iter().enumerate() {
        for _ in 0..n {
            huffsize.push((l + 1) as u8);
        }
    }
    let mut huffcode = Vec::with_capacity(huffsize.len());
    let mut code: u32 = 0;
    let mut si = huffsize.first().copied().unwrap_or(0);
    let mut p = 0;
    while p < huffsize.len() {
        while p < huffsize.len() && huffsize[p] == si {
            huffcode.push(code);
            code += 1;
            p += 1;
        }
        code <<= 1;
        si += 1;
    }
    let mut t = DerivedTable { code: [0; 256], size: [0; 256] };
    for (i, &v) in vals.iter().enumerate() {
        t.code[v as usize] = huffcode[i];
        t.size[v as usize] = huffsize[i];
    }
    t
}

/// `jchuff`'s bit buffer, with 0xFF stuffing.
struct BitWriter {
    out: Vec<u8>,
    put_buffer: u32,
    put_bits: u32,
}

impl BitWriter {
    fn emit_bits(&mut self, code: u32, size: u32) {
        let mut put_buffer = code & ((1u32 << size) - 1);
        let put_bits = self.put_bits + size;
        put_buffer <<= 24 - put_bits;
        put_buffer |= self.put_buffer;
        let mut pb = put_bits;
        while pb >= 8 {
            let c = ((put_buffer >> 16) & 0xff) as u8;
            self.out.push(c);
            if c == 0xff {
                self.out.push(0);
            }
            put_buffer <<= 8;
            pb -= 8;
        }
        self.put_buffer = put_buffer & 0x00ff_ffff;
        self.put_bits = pb;
    }

    fn flush(&mut self) {
        self.emit_bits(0x7f, 7);
        self.put_buffer = 0;
        self.put_bits = 0;
    }
}

const CONST_BITS: i32 = 13;
const PASS1_BITS: i32 = 2;
const FIX_0_298631336: i32 = 2446;
const FIX_0_390180644: i32 = 3196;
const FIX_0_541196100: i32 = 4433;
const FIX_0_765366865: i32 = 6270;
const FIX_0_899976223: i32 = 7373;
const FIX_1_175875602: i32 = 9633;
const FIX_1_501321110: i32 = 12299;
const FIX_1_847759065: i32 = 15137;
const FIX_1_961570560: i32 = 16069;
const FIX_2_053119869: i32 = 16819;
const FIX_2_562915447: i32 = 20995;
const FIX_3_072711026: i32 = 25172;

fn descale(x: i32, n: i32) -> i32 {
    (x + (1 << (n - 1))) >> n
}

/// `jpeg_fdct_islow`.
fn fdct_islow(data: &mut [i32; 64]) {
    for r in 0..8 {
        let d = &mut data[r * 8..r * 8 + 8];
        let tmp0 = d[0] + d[7];
        let tmp7 = d[0] - d[7];
        let tmp1 = d[1] + d[6];
        let tmp6 = d[1] - d[6];
        let tmp2 = d[2] + d[5];
        let tmp5 = d[2] - d[5];
        let tmp3 = d[3] + d[4];
        let tmp4 = d[3] - d[4];
        let tmp10 = tmp0 + tmp3;
        let tmp13 = tmp0 - tmp3;
        let tmp11 = tmp1 + tmp2;
        let tmp12 = tmp1 - tmp2;
        d[0] = (tmp10 + tmp11) << PASS1_BITS;
        d[4] = (tmp10 - tmp11) << PASS1_BITS;
        let z1 = (tmp12 + tmp13) * FIX_0_541196100;
        d[2] = descale(z1 + tmp13 * FIX_0_765366865, CONST_BITS - PASS1_BITS);
        d[6] = descale(z1 + tmp12 * (-FIX_1_847759065), CONST_BITS - PASS1_BITS);
        odd(d, tmp4, tmp5, tmp6, tmp7, CONST_BITS - PASS1_BITS);
    }
    for c in 0..8 {
        let g = |data: &[i32; 64], i: usize| data[i * 8 + c];
        let tmp0 = g(data, 0) + g(data, 7);
        let tmp7 = g(data, 0) - g(data, 7);
        let tmp1 = g(data, 1) + g(data, 6);
        let tmp6 = g(data, 1) - g(data, 6);
        let tmp2 = g(data, 2) + g(data, 5);
        let tmp5 = g(data, 2) - g(data, 5);
        let tmp3 = g(data, 3) + g(data, 4);
        let tmp4 = g(data, 3) - g(data, 4);
        let tmp10 = tmp0 + tmp3;
        let tmp13 = tmp0 - tmp3;
        let tmp11 = tmp1 + tmp2;
        let tmp12 = tmp1 - tmp2;
        let mut col = [0i32; 8];
        col[0] = descale(tmp10 + tmp11, PASS1_BITS);
        col[4] = descale(tmp10 - tmp11, PASS1_BITS);
        let z1 = (tmp12 + tmp13) * FIX_0_541196100;
        col[2] = descale(z1 + tmp13 * FIX_0_765366865, CONST_BITS + PASS1_BITS);
        col[6] = descale(z1 + tmp12 * (-FIX_1_847759065), CONST_BITS + PASS1_BITS);
        odd(&mut col, tmp4, tmp5, tmp6, tmp7, CONST_BITS + PASS1_BITS);
        for i in 0..8 {
            data[i * 8 + c] = col[i];
        }
    }
}

/// The odd part of both `jpeg_fdct_islow` passes.
fn odd(d: &mut [i32], tmp4: i32, tmp5: i32, tmp6: i32, tmp7: i32, shift: i32) {
    let z1 = tmp4 + tmp7;
    let z2 = tmp5 + tmp6;
    let z3 = tmp4 + tmp6;
    let z4 = tmp5 + tmp7;
    let z5 = (z3 + z4) * FIX_1_175875602;
    let tmp4 = tmp4 * FIX_0_298631336;
    let tmp5 = tmp5 * FIX_2_053119869;
    let tmp6 = tmp6 * FIX_3_072711026;
    let tmp7 = tmp7 * FIX_1_501321110;
    let z1 = z1 * -FIX_0_899976223;
    let z2 = z2 * -FIX_2_562915447;
    let z3 = z3 * -FIX_1_961570560 + z5;
    let z4 = z4 * -FIX_0_390180644 + z5;
    d[7] = descale(tmp4 + z1 + z3, shift);
    d[5] = descale(tmp5 + z2 + z4, shift);
    d[3] = descale(tmp6 + z2 + z3, shift);
    d[1] = descale(tmp7 + z1 + z4, shift);
}

/// A downsampled, edge-padded component plane.
struct Plane {
    width: usize,
    data: Vec<u8>,
}

impl Plane {
    fn at(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.width + x]
    }
}

/// `forward_DCT` for the 8x8 block at (`x`, `y`), quantized by `divisors`.
fn forward_dct(p: &Plane, x: usize, y: usize, divisors: &[i32; 64]) -> [i32; 64] {
    let mut ws = [0i32; 64];
    for r in 0..8 {
        for c in 0..8 {
            ws[r * 8 + c] = p.at(x + c, y + r) as i32 - 128;
        }
    }
    fdct_islow(&mut ws);
    let mut out = [0i32; 64];
    for i in 0..64 {
        let qval = divisors[i];
        let mut temp = ws[i];
        if temp < 0 {
            temp = -temp + (qval >> 1);
            temp = if temp >= qval { temp / qval } else { 0 };
            temp = -temp;
        } else {
            temp += qval >> 1;
            temp = if temp >= qval { temp / qval } else { 0 };
        }
        out[i] = temp;
    }
    out
}

fn encode_block(w: &mut BitWriter, block: &[i32; 64], last_dc: &mut i32, dc: &DerivedTable, ac: &DerivedTable) {
    let mut temp = block[0] - *last_dc;
    let mut temp2 = temp;
    *last_dc = block[0];
    if temp < 0 {
        temp = -temp;
        temp2 -= 1;
    }
    let mut nbits = 0u32;
    while temp != 0 {
        nbits += 1;
        temp >>= 1;
    }
    w.emit_bits(dc.code[nbits as usize], dc.size[nbits as usize] as u32);
    if nbits != 0 {
        w.emit_bits(temp2 as u32, nbits);
    }
    let mut r = 0;
    for k in 1..64 {
        let mut temp = block[NATURAL_ORDER[k]];
        if temp == 0 {
            r += 1;
        } else {
            while r > 15 {
                w.emit_bits(ac.code[0xf0], ac.size[0xf0] as u32);
                r -= 16;
            }
            let mut temp2 = temp;
            if temp < 0 {
                temp = -temp;
                temp2 -= 1;
            }
            let mut nbits = 1u32;
            loop {
                temp >>= 1;
                if temp == 0 {
                    break;
                }
                nbits += 1;
            }
            let i = ((r << 4) + nbits as usize) & 0xff;
            w.emit_bits(ac.code[i], ac.size[i] as u32);
            w.emit_bits(temp2 as u32, nbits);
            r = 0;
        }
    }
    if r > 0 {
        w.emit_bits(ac.code[0], ac.size[0] as u32);
    }
}

fn marker(out: &mut Vec<u8>, m: u8, body: &[u8]) {
    out.push(0xff);
    out.push(m);
    out.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(body);
}

/// `bufferedImageToJPEG(bufferedImage, quality)` for a `TYPE_INT_ARGB` image of `pixels`.
pub fn buffered_image_to_jpeg(pixels: &[i32], width: i32, height: i32, quality: f32) -> Result<Vec<u8>, String> {
    if width <= 0 || height <= 0 || pixels.len() < (width * height) as usize {
        return Err("Empty JPEG image (DNL not supported)".to_string());
    }
    let (w, h) = (width as usize, height as usize);
    let lin = convert_to_linear_quality(quality);
    let q0 = scaled(&K1, lin);
    let q1 = scaled(&K2, lin);

    // jccolor rgb_ycc_convert.
    const SCALEBITS: i32 = 16;
    let fix = |x: f64| (x * (1i64 << SCALEBITS) as f64 + 0.5) as i32;
    let one_half = 1 << (SCALEBITS - 1);
    let cbcr_offset = 128 << SCALEBITS;
    let (mut ys, mut cbs, mut crs) = (vec![0u8; w * h], vec![0u8; w * h], vec![0u8; w * h]);
    for i in 0..w * h {
        let p = pixels[i];
        let (r, g, b) = ((p >> 16) & 0xff, (p >> 8) & 0xff, p & 0xff);
        ys[i] = ((fix(0.29900) * r + fix(0.58700) * g + fix(0.11400) * b + one_half) >> SCALEBITS) as u8;
        cbs[i] = (((-fix(0.16874)) * r + (-fix(0.33126)) * g + fix(0.50000) * b + cbcr_offset + one_half - 1) >> SCALEBITS) as u8;
        crs[i] = ((fix(0.50000) * r + cbcr_offset + one_half - 1 + (-fix(0.41869)) * g + (-fix(0.08131)) * b) >> SCALEBITS) as u8;
    }

    let mcus_x = w.div_ceil(16);
    let mcus_y = h.div_ceil(16);
    let y_wib = w.div_ceil(8);
    let y_hib = h.div_ceil(8);
    let c_wib = w.div_ceil(16);
    let rows_padded = mcus_y * 16;

    // fullsize_downsample + jcprepct bottom padding for Y.
    let yw = y_wib * 8;
    let mut yp = Plane { width: yw, data: vec![0; yw * rows_padded] };
    for y in 0..rows_padded {
        let sy = y.min(h - 1);
        for x in 0..yw {
            yp.data[y * yw + x] = ys[sy * w + x.min(w - 1)];
        }
    }
    // h2v2_downsample over bottom-padded row groups, then the iMCU bottom padding.
    let cw = c_wib * 8;
    let c_rows = mcus_y * 8;
    let real_c_rows = h.div_ceil(2);
    let down = |src: &[u8]| {
        let mut p = Plane { width: cw, data: vec![0; cw * c_rows] };
        for oy in 0..c_rows {
            let gy = oy.min(real_c_rows - 1);
            let r0 = (2 * gy).min(h - 1);
            let r1 = (2 * gy + 1).min(h - 1);
            let mut bias = 1;
            for ox in 0..cw {
                let x0 = (2 * ox).min(w - 1);
                let x1 = (2 * ox + 1).min(w - 1);
                let s = src[r0 * w + x0] as i32 + src[r0 * w + x1] as i32 + src[r1 * w + x0] as i32 + src[r1 * w + x1] as i32;
                p.data[oy * cw + ox] = ((s + bias) >> 2) as u8;
                bias ^= 3;
            }
        }
        p
    };
    let cbp = down(&cbs);
    let crp = down(&crs);

    let d0: [i32; 64] = std::array::from_fn(|i| q0[i] << 3);
    let d1: [i32; 64] = std::array::from_fn(|i| q1[i] << 3);
    let dc_lum = derive(&DC_LUM_BITS, &DC_VALS);
    let ac_lum = derive(&AC_LUM_BITS, &AC_LUM_VALS);
    let dc_chrom = derive(&DC_CHROM_BITS, &DC_VALS);
    let ac_chrom = derive(&AC_CHROM_BITS, &AC_CHROM_VALS);

    let mut bw = BitWriter { out: Vec::new(), put_buffer: 0, put_bits: 0 };
    let (mut last_y, mut last_cb, mut last_cr) = (0, 0, 0);
    let last_col_width = if y_wib % 2 == 0 { 2 } else { y_wib % 2 };
    let last_row_height = if y_hib % 2 == 0 { 2 } else { y_hib % 2 };
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            let mut blocks: Vec<[i32; 64]> = Vec::with_capacity(4);
            let blockcnt = if mx < mcus_x - 1 { 2 } else { last_col_width };
            for yi in 0..2 {
                if my < mcus_y - 1 || yi < last_row_height {
                    for xi in 0..2 {
                        if xi < blockcnt {
                            blocks.push(forward_dct(&yp, mx * 16 + xi * 8, my * 16 + yi * 8, &d0));
                        } else {
                            let mut b = [0i32; 64];
                            b[0] = blocks.last().unwrap()[0];
                            blocks.push(b);
                        }
                    }
                } else {
                    let dc = blocks.last().unwrap()[0];
                    for _ in 0..2 {
                        let mut b = [0i32; 64];
                        b[0] = dc;
                        blocks.push(b);
                    }
                }
            }
            for b in &blocks {
                encode_block(&mut bw, b, &mut last_y, &dc_lum, &ac_lum);
            }
            encode_block(&mut bw, &forward_dct(&cbp, mx * 8, my * 8, &d1), &mut last_cb, &dc_chrom, &ac_chrom);
            encode_block(&mut bw, &forward_dct(&crp, mx * 8, my * 8, &d1), &mut last_cr, &dc_chrom, &ac_chrom);
        }
    }
    bw.flush();

    let mut out = vec![0xff, 0xd8];
    marker(&mut out, 0xe0, &[0x4a, 0x46, 0x49, 0x46, 0x00, 1, 2, 0, 0, 1, 0, 1, 0, 0]);
    for (n, q) in [(0u8, &q0), (1u8, &q1)] {
        let mut b = vec![n];
        b.extend(NATURAL_ORDER.iter().map(|&i| q[i] as u8));
        marker(&mut out, 0xdb, &b);
    }
    let mut sof = vec![8];
    sof.extend_from_slice(&(height as u16).to_be_bytes());
    sof.extend_from_slice(&(width as u16).to_be_bytes());
    sof.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    marker(&mut out, 0xc0, &sof);
    for (index, bits, vals) in [(0x00u8, &DC_LUM_BITS, &DC_VALS[..]), (0x10, &AC_LUM_BITS, &AC_LUM_VALS[..]), (0x01, &DC_CHROM_BITS, &DC_VALS[..]), (0x11, &AC_CHROM_BITS, &AC_CHROM_VALS[..])] {
        let mut b = vec![index];
        b.extend_from_slice(bits);
        b.extend_from_slice(vals);
        marker(&mut out, 0xc4, &b);
    }
    marker(&mut out, 0xda, &[3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);
    out.extend_from_slice(&bw.out);
    out.extend_from_slice(&[0xff, 0xd9]);
    Ok(out)
}
