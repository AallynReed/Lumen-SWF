//! Nellymoser decoder, ported from the JPEXS copy of lt.dkd.nellymoser.
#![allow(clippy::excessive_precision)]

const BLOCK_LEN: usize = 64;
const SAMPLES: usize = 256;

/// Decodes Nellymoser audio (64-byte blocks, mono) to signed 16-bit samples.
pub fn decode(data: &[u8]) -> Vec<i16> {
    let mut state = [0f32; 64];
    let mut rng = Rng(0x2545_f491);
    let mut audio = [0f32; SAMPLES];
    let mut out = Vec::with_capacity(data.len() / BLOCK_LEN * SAMPLES);
    for block in data.chunks_exact(BLOCK_LEN) {
        decode_block(&mut state, block, &mut audio, &mut rng);
        out.extend(audio.iter().map(|&s| s as i32 as i16));
    }
    out
}

// Stands in for Math.random() in the noise fill.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

struct BitStream {
    byte_offset: usize,
    bit_offset: u32,
}

impl BitStream {
    fn pop(&mut self, len: u32, buf: &[u8]) -> i32 {
        let byte = |i: usize| buf.get(i).copied().unwrap_or(0) as i8 as i32;
        let mut val = (byte(self.byte_offset) & 0xff) >> self.bit_offset;
        let bits_read = 8 - self.bit_offset;
        if len >= bits_read {
            self.byte_offset += 1;
            if len > bits_read {
                val |= byte(self.byte_offset) << bits_read;
            }
        }
        self.bit_offset = (self.bit_offset + len) & 7;
        val & ((1 << len) - 1)
    }
}

fn normalized_scale(val: i32) -> i32 {
    if val == 0 {
        return 31;
    } else if val >= 1 << 30 {
        return 0;
    }
    let mut v = val;
    let mut s = 0;
    loop {
        v = v.wrapping_shl(1);
        s += 1;
        let done = if val > 0 {
            v >= 1 << 30
        } else {
            v <= -(1 << 30)
        };
        if done {
            return s;
        }
    }
}

fn decode_block(state: &mut [f32; 64], input: &[u8], out: &mut [f32; SAMPLES], rng: &mut Rng) {
    let mut var_808 = [0f32; 128];
    let mut var_608 = [0f32; 124];
    let mut var_418 = [0f32; 124];

    let mut bs = BitStream {
        byte_offset: 0,
        bit_offset: 0,
    };

    let unpacked = bs.pop(GAIN_BIT[0], input);
    var_808[0] = TABLE1[unpacked as usize];
    for i in 1..23 {
        let unpacked = bs.pop(GAIN_BIT[i], input);
        var_808[i] = var_808[i - 1] + TABLE2[unpacked as usize];
    }

    for i in 0..23 {
        let pow = 2.0f64.powf(var_808[i] as f64 * (0.5 * 0.0009765625)) as f32;
        for bound in BAND_BOUND[i]..BAND_BOUND[i + 1] {
            var_418[bound] = var_808[i];
            var_608[bound] = pow;
        }
    }

    let mut packed_sizes = [0i32; 124];
    let leftover = wc(&var_418, 198, &mut packed_sizes);

    for out_off in (0..SAMPLES).step_by(128) {
        for i in 0..124 {
            let packed_size = packed_sizes[i];
            let mut val = var_608[i];
            if packed_size > 0 {
                let pow2 = 1i32 << packed_size;
                let unpacked = bs.pop(packed_size as u32, input);
                val *= TABLE3[(pow2 - 1 + unpacked) as usize];
            } else if rng.next() < (1 << 30) + (1 << 14) {
                val = (val as f64 * -0.707099974) as f32;
            } else {
                val = (val as f64 * 0.707099974) as f32;
            }
            var_808[i] = val;
        }
        for v in &mut var_808[124..] {
            *v = 0.0;
        }

        let mut i = leftover;
        while i > 0 {
            if i > 8 {
                bs.pop(8, input);
            } else {
                bs.pop(i as u32, input);
                break;
            }
            i -= 8;
        }

        i_transfm(state, &var_808, 7, &mut out[out_off..out_off + 128]);
    }
}

fn i_transfm(state: &mut [f32; 64], input: &[f32], len_log2: u32, out: &mut [f32]) {
    let len = 1usize << len_log2;
    let quarter_len = len >> 2;
    let half = len >> 1;

    auxceps(input, len_log2, out);

    for i in 0..quarter_len {
        let (j, x, y) = (half - 1 - i, half + i, len - 1 - i);
        let state_i = state[i] as f64;
        let state_j = state[j] as f64;
        let out_x = out[x] as f64;
        let out_y = out[y] as f64;

        state[i] = -out[j];
        state[j] = -out[i];

        out[i] = (state_i * TABLE7[y] as f64 + out_x * TABLE7[i] as f64) as f32;
        out[j] = (state_j * TABLE7[x] as f64 + out_y * TABLE7[j] as f64) as f32;
        out[x] = (TABLE7[x] as f64 * -out_y + TABLE7[j] as f64 * state_j) as f32;
        out[y] = (TABLE7[y] as f64 * -out_x + TABLE7[i] as f64 * state_i) as f32;
    }
}

fn auxceps(input: &[f32], len_log2: u32, out: &mut [f32]) {
    let len = 1usize << len_log2;
    let half_len_m1 = (len >> 1) - 1;
    let quarter_len = len >> 2;

    for i in 0..quarter_len {
        let i2 = i << 1;
        let j = len - 1 - i2;
        let k = j - 1;

        let in_i2 = input[i2] as f64;
        let in_i2_1 = input[i2 + 1] as f64;
        let in_j = input[j] as f64;
        let in_k = input[k] as f64;

        let t4a = TABLE4[i] as f64;
        let t6a = TABLE6[i] as f64;
        let t4b = TABLE4[half_len_m1 - i] as f64;
        let t6b = TABLE6[half_len_m1 - i] as f64;

        out[i2] = (t4a * in_i2 - t6a * in_j) as f32;
        out[i2 + 1] = (in_j * t4a + in_i2 * t6a) as f32;
        out[k] = (t4b * in_k - t6b * in_i2_1) as f32;
        out[j] = (in_i2_1 * t4b + in_k * t6b) as f32;
    }

    har_xfm(out, len_log2 - 1);

    let last_out = out[len - 1];
    let pre_last_out = out[len - 2];

    out[0] *= TABLE5[0];
    out[len - 1] = out[1] * -TABLE5[0];
    out[len - 2] = TABLE5[half_len_m1] * out[len - 2] + TABLE5[1] * last_out;
    out[1] = pre_last_out * TABLE5[1] - last_out * TABLE5[half_len_m1];

    let mut i_out = len - 3;
    let mut i_tbl = half_len_m1;
    let mut j = 3;
    for _ in 1..quarter_len {
        let old_a = out[i_out] as f64;
        let old_b = out[i_out - 1] as f64;
        let old_c = out[j] as f64;
        let old_d = out[j - 1] as f64;

        let t_tbl = TABLE5[i_tbl] as f64;
        let t_tbl_m1 = TABLE5[i_tbl - 1] as f64;
        let t_lo = TABLE5[(j - 1) >> 1] as f64;
        let t_hi = TABLE5[(j + 1) >> 1] as f64;

        out[j - 1] = (t_tbl * old_c + t_lo * old_d) as f32;
        out[j] = (old_b * t_hi - old_a * t_tbl_m1) as f32;
        out[i_out] = (old_d * t_tbl - old_c * t_lo) as f32;
        out[i_out - 1] = (t_hi * old_a + t_tbl_m1 * old_b) as f32;

        i_tbl -= 1;
        i_out -= 2;
        j += 2;
    }
}

fn har_xfm(data: &mut [f32], half_len_log2: u32) {
    let half_len = 1usize << half_len_log2;

    har_xfm_helper(data, half_len);

    for j in (0..(half_len >> 1) * 4).step_by(4) {
        let (j0, j1, j2, j3) = (data[j], data[j + 1], data[j + 2], data[j + 3]);
        data[j] = j0 + j2;
        data[j + 1] = j1 + j3;
        data[j + 2] = j0 - j2;
        data[j + 3] = j1 - j3;
    }

    for j in (0..(half_len >> 2) * 8).step_by(8) {
        let (j0, j1, j2, j3) = (data[j], data[j + 1], data[j + 2], data[j + 3]);
        let (j4, j5, j6, j7) = (data[j + 4], data[j + 5], data[j + 6], data[j + 7]);
        data[j] = j0 + j4;
        data[j + 1] = j1 + j5;
        data[j + 2] = j2 + j7;
        data[j + 3] = j3 - j6;
        data[j + 4] = j0 - j4;
        data[j + 5] = j1 - j5;
        data[j + 6] = j2 - j7;
        data[j + 7] = j3 + j6;
    }

    let mut i = 0usize;
    let mut x = half_len >> 3;
    let mut y = 64usize;
    let mut z = 4usize;
    for _ in 0..half_len_log2.saturating_sub(2) {
        let mut j = 0usize;
        for _ in 0..x {
            for _ in 0..z >> 1 {
                let k = j + (z << 1);
                let (j0, j1) = (data[j] as f64, data[j + 1] as f64);
                let (k0, k1) = (data[k] as f64, data[k + 1] as f64);
                let c = TABLE10[128 - i] as f64;
                let s = TABLE10[i] as f64;
                data[k] = (j0 - (k0 * c + k1 * s)) as f32;
                data[j] = (j0 + (k0 * c + k1 * s)) as f32;
                data[k + 1] = (j1 + (k0 * s - k1 * c)) as f32;
                data[j + 1] = (j1 - (k0 * s - k1 * c)) as f32;
                j += 2;
                i += y;
            }
            for _ in 0..z >> 1 {
                let k = j + (z << 1);
                let (j0, j1) = (data[j] as f64, data[j + 1] as f64);
                let (k0, k1) = (data[k] as f64, data[k + 1] as f64);
                let c = TABLE10[128 - i] as f64;
                let s = TABLE10[i] as f64;
                data[k] = (j0 + (k0 * c - k1 * s)) as f32;
                data[j] = (j0 - (k0 * c - k1 * s)) as f32;
                data[k + 1] = (j1 + (k1 * c + k0 * s)) as f32;
                data[j + 1] = (j1 - (k1 * c + k0 * s)) as f32;
                j += 2;
                i -= y;
            }
            j += z << 1;
        }
        z <<= 1;
        y >>= 1;
        x >>= 1;
    }
}

fn har_xfm_helper(data: &mut [f32], half_len: usize) {
    let len = half_len << 1;
    let mut j = 1usize;
    for i in (1..len).step_by(2) {
        if i < j {
            data.swap(i, j);
            data.swap(i - 1, j - 1);
        }
        let mut x = half_len;
        while x > 1 && x < j {
            j -= x;
            x >>= 1;
        }
        j += x;
    }
}

// Factor(124) from the original, which is a hardcoded special case.
const FACTOR_VALUE: i32 = 4228;
const FACTOR_SHIFT: i32 = 19;

fn shift(v: i32, s: i32) -> i32 {
    if s >= 0 {
        v.wrapping_shl(s as u32)
    } else {
        v.wrapping_shr((-s) as u32)
    }
}

fn wc(input: &[f32; 124], total_bits: i32, packed_sizes: &mut [i32; 124]) -> i32 {
    let mut max_input = 0f32;
    for &v in input {
        if v > max_input {
            max_input = v;
        }
    }

    let mut max_input_scale = normalized_scale(max_input as i32) - 16;

    let mut scaled = [0i16; 124];
    for (s, &v) in scaled.iter_mut().zip(input) {
        *s = shift(v as i32, max_input_scale) as i16;
    }
    for v in &mut scaled {
        *v = ((*v as i32 * 3) >> 2) as i16;
    }

    let mut sum = 0i32;
    for &v in &scaled {
        sum = sum.wrapping_add(v as i32);
    }

    max_input_scale += 11;
    sum = sum.wrapping_sub(total_bits.wrapping_shl(max_input_scale as u32));
    let mut base = {
        let val = sum.wrapping_sub(total_bits.wrapping_shl(max_input_scale as u32));
        let b = (val >> 16).wrapping_mul(FACTOR_VALUE) >> 15;
        shift(b, 31 - FACTOR_SHIFT - normalized_scale(val))
    };

    let mut bits_used = get_d(&scaled, max_input_scale, 6, base);
    if bits_used != total_bits {
        let mut diff = bits_used - total_bits;
        let mut diff_scale = 0i32;
        if diff <= 0 {
            while diff >= -16384 {
                diff = diff.wrapping_shl(1);
                diff_scale += 1;
            }
        } else {
            while diff < 16384 {
                diff = diff.wrapping_shl(1);
                diff_scale += 1;
            }
        }

        let base_delta = shift(
            diff.wrapping_mul(FACTOR_VALUE) >> 15,
            max_input_scale - (FACTOR_SHIFT + diff_scale - 15),
        );

        let mut num_revisions = 1;
        let mut last_bits_used;
        let mut last_base;
        loop {
            last_bits_used = bits_used;
            last_base = base;
            base = base.wrapping_add(base_delta);
            bits_used = get_d(&scaled, max_input_scale, 6, base);
            num_revisions += 1;
            if num_revisions > 19
                || (bits_used - total_bits).wrapping_mul(last_bits_used - total_bits) <= 0
            {
                break;
            }
        }

        if bits_used != total_bits {
            let mut base_1;
            let mut bits_used_1;
            let mut bits_used_2;
            if bits_used > total_bits {
                base_1 = base;
                base = last_base;
                bits_used_1 = bits_used;
                bits_used_2 = last_bits_used;
            } else {
                base_1 = last_base;
                bits_used_1 = last_bits_used;
                bits_used_2 = bits_used;
            }

            while bits_used != total_bits && num_revisions < 20 {
                let avg = base.wrapping_add(base_1) >> 1;
                bits_used = get_d(&scaled, max_input_scale, 6, avg);
                num_revisions += 1;
                if bits_used > total_bits {
                    base_1 = avg;
                    bits_used_1 = bits_used;
                } else {
                    base = avg;
                    bits_used_2 = bits_used;
                }
            }

            let dev_1 = (bits_used_1 - total_bits).wrapping_abs();
            let dev_2 = (bits_used_2 - total_bits).wrapping_abs();
            if dev_1 < dev_2 {
                base = base_1;
                bits_used = bits_used_1;
            } else {
                bits_used = bits_used_2;
            }
        }
    }

    let round = 1i32.wrapping_shl((max_input_scale - 1) as u32);
    for (p, &s) in packed_sizes.iter_mut().zip(&scaled) {
        let tmp = (s as i32).wrapping_sub(base);
        let tmp = if tmp >= 0 {
            tmp.wrapping_add(round).wrapping_shr(max_input_scale as u32)
        } else {
            0
        };
        *p = tmp.min(6);
    }

    if bits_used > total_bits {
        let mut i = 0;
        let mut bit_count = 0;
        while bit_count < total_bits {
            bit_count += packed_sizes[i];
            i += 1;
        }
        bit_count -= packed_sizes[i - 1];
        packed_sizes[i - 1] = total_bits - bit_count;
        bits_used = total_bits;
        for p in &mut packed_sizes[i..] {
            *p = 0;
        }
    }

    total_bits - bits_used
}

fn get_d(input: &[i16; 124], scale: i32, upper_bound: i32, base: i32) -> i32 {
    let round = 1i32.wrapping_shl((scale - 1) as u32);
    let mut d = 0i32;
    for &v in input {
        let var_2 = (v as i32).wrapping_sub(base);
        let var_2 = if var_2 < 0 {
            0
        } else {
            var_2.wrapping_add(round).wrapping_shr(scale as u32)
        };
        d = d.wrapping_add(var_2.min(upper_bound));
    }
    d
}

const BAND_BOUND: [usize; 24] = [
    0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 21, 24, 28, 32, 37, 43, 49, 56, 64, 73, 83, 95, 109, 124,
];

const GAIN_BIT: [u32; 32] = [
    6, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const TABLE1: [f32; 64] = [
    3134.0, 5342.0, 6870.0, 7792.0, 8569.0, 9185.0, 9744.0, 10191.0, 10631.0, 11061.0, 11434.0,
    11770.0, 12116.0, 12513.0, 12925.0, 13300.0, 13674.0, 14027.0, 14352.0, 14716.0, 15117.0,
    15477.0, 15824.0, 16157.0, 16513.0, 16804.0, 17090.0, 17401.0, 17679.0, 17948.0, 18238.0,
    18520.0, 18764.0, 19078.0, 19381.0, 19640.0, 19921.0, 20205.0, 20500.0, 20813.0, 21162.0,
    21465.0, 21794.0, 22137.0, 22453.0, 22756.0, 23067.0, 23350.0, 23636.0, 23926.0, 24227.0,
    24521.0, 24819.0, 25107.0, 25414.0, 25730.0, 26120.0, 26497.0, 26895.0, 27344.0, 27877.0,
    28463.0, 29426.0, 31355.0,
];

const TABLE2: [f32; 32] = [
    -11725.0, -9420.0, -7910.0, -6801.0, -5948.0, -5233.0, -4599.0, -4039.0, -3507.0, -3030.0,
    -2596.0, -2170.0, -1774.0, -1383.0, -1016.0, -660.0, -329.0, -1.0, 337.0, 696.0, 1085.0,
    1512.0, 1962.0, 2433.0, 2968.0, 3569.0, 4314.0, 5279.0, 6622.0, 8154.0, 10076.0, 12975.0,
];

const TABLE3: [f32; 128] = [
    0.0,
    -0.847256005,
    0.722470999,
    -1.52474797,
    -0.453148007,
    0.375360996,
    1.47178996,
    -1.98225796,
    -1.19293797,
    -0.582937002,
    -0.0693780035,
    0.390956998,
    0.906920016,
    1.486274,
    2.22154093,
    -2.38878703,
    -1.80675399,
    -1.41054201,
    -1.07736099,
    -0.799501002,
    -0.555810988,
    -0.333402008,
    -0.132449001,
    0.0568020009,
    0.254877001,
    0.477355003,
    0.738685012,
    1.04430604,
    1.39544594,
    1.80987501,
    2.39187598,
    -2.38938308,
    -1.98846805,
    -1.75140405,
    -1.56431198,
    -1.39221299,
    -1.216465,
    -1.04694998,
    -0.890510023,
    -0.764558017,
    -0.645457983,
    -0.52592802,
    -0.405954987,
    -0.302971989,
    -0.209690005,
    -0.123986997,
    -0.0479229987,
    0.025773,
    0.100134,
    0.173718005,
    0.258554012,
    0.352290004,
    0.456988007,
    0.576775014,
    0.700316012,
    0.842552006,
    1.00938797,
    1.18213499,
    1.35345602,
    1.53208196,
    1.73326194,
    1.97223496,
    2.39781404,
    -2.5756309,
    -2.05733204,
    -1.89849198,
    -1.77278101,
    -1.66626,
    -1.57421803,
    -1.49933195,
    -1.43166399,
    -1.36522806,
    -1.30009902,
    -1.22809303,
    -1.15885794,
    -1.09212506,
    -1.013574,
    -0.920284986,
    -0.828705013,
    -0.737488985,
    -0.644775987,
    -0.559094012,
    -0.485713989,
    -0.411031991,
    -0.345970005,
    -0.285115987,
    -0.234162003,
    -0.187058002,
    -0.144250005,
    -0.110716999,
    -0.0739680007,
    -0.0365610011,
    -0.00732900016,
    0.0203610007,
    0.0479039997,
    0.0751969963,
    0.0980999991,
    0.122038998,
    0.145899996,
    0.169434994,
    0.197045997,
    0.225243002,
    0.255686998,
    0.287010014,
    0.319709986,
    0.352582991,
    0.388906986,
    0.433492005,
    0.476945996,
    0.520482004,
    0.564453006,
    0.612204015,
    0.668592989,
    0.734165013,
    0.803215981,
    0.878404021,
    0.956620991,
    1.03970695,
    1.12937701,
    1.22111595,
    1.30802798,
    1.40248001,
    1.50568199,
    1.62277305,
    1.77249599,
    1.94308805,
    2.29039311,
    0.0,
];

const TABLE4: [f32; 64] = [
    0.999981225,
    0.999529421,
    0.998475611,
    0.996820271,
    0.994564593,
    0.991709828,
    0.988257587,
    0.984210074,
    0.979569793,
    0.974339426,
    0.968522072,
    0.962121427,
    0.955141187,
    0.947585583,
    0.939459205,
    0.930767,
    0.921513975,
    0.911705971,
    0.901348829,
    0.890448689,
    0.879012227,
    0.867046177,
    0.854557991,
    0.841554999,
    0.828045011,
    0.81403631,
    0.799537301,
    0.784556627,
    0.769103289,
    0.753186822,
    0.736816585,
    0.720002472,
    0.702754676,
    0.685083687,
    0.666999876,
    0.64851439,
    0.629638195,
    0.610382795,
    0.590759695,
    0.570780694,
    0.550458014,
    0.529803574,
    0.50883007,
    0.487550199,
    0.465976506,
    0.444122106,
    0.422000289,
    0.399624199,
    0.377007395,
    0.354163498,
    0.331106305,
    0.307849586,
    0.284407496,
    0.260794103,
    0.237023607,
    0.213110298,
    0.189068705,
    0.164913103,
    0.1406582,
    0.116318598,
    0.0919089988,
    0.0674438998,
    0.0429382995,
    0.0184067003,
];

const TABLE5: [f32; 64] = [
    0.125,
    0.124962397,
    0.124849401,
    0.124661297,
    0.124398097,
    0.124059901,
    0.123647101,
    0.123159699,
    0.122598201,
    0.121962801,
    0.1212539,
    0.120471999,
    0.119617499,
    0.118690997,
    0.117693,
    0.116624102,
    0.115484901,
    0.114276201,
    0.112998702,
    0.111653,
    0.110240199,
    0.108760901,
    0.107216097,
    0.105606697,
    0.103933699,
    0.102198102,
    0.100400902,
    0.0985433012,
    0.0966262966,
    0.094651103,
    0.0926188976,
    0.0905309021,
    0.0883883014,
    0.0861926004,
    0.0839449018,
    0.0816465989,
    0.0792991966,
    0.076903902,
    0.0744623989,
    0.0719759986,
    0.069446303,
    0.0668746978,
    0.0642627999,
    0.0616123006,
    0.0589246005,
    0.0562013984,
    0.0534444004,
    0.0506552011,
    0.0478353985,
    0.0449868999,
    0.0421111993,
    0.0392102003,
    0.0362856016,
    0.0333391018,
    0.0303725004,
    0.0273876991,
    0.0243862998,
    0.0213702004,
    0.0183412991,
    0.0153013002,
    0.0122520998,
    0.0091955997,
    0.00613350002,
    0.00306769996,
];

const TABLE6: [f32; 64] = [
    -0.00613590004,
    -0.0306748003,
    -0.0551952012,
    -0.0796824023,
    -0.104121603,
    -0.128498107,
    -0.152797207,
    -0.177004203,
    -0.201104596,
    -0.225083902,
    -0.248927593,
    -0.272621393,
    -0.296150893,
    -0.319501996,
    -0.342660695,
    -0.365613014,
    -0.388345003,
    -0.410843194,
    -0.433093786,
    -0.455083609,
    -0.47679919,
    -0.498227686,
    -0.519356012,
    -0.540171504,
    -0.560661614,
    -0.580814004,
    -0.600616515,
    -0.620057225,
    -0.639124393,
    -0.657806695,
    -0.676092684,
    -0.693971515,
    -0.711432219,
    -0.728464425,
    -0.745057821,
    -0.761202395,
    -0.77688849,
    -0.792106628,
    -0.806847572,
    -0.8211025,
    -0.834862888,
    -0.848120272,
    -0.860866904,
    -0.873094976,
    -0.884797096,
    -0.895966172,
    -0.906595707,
    -0.916679084,
    -0.926210225,
    -0.935183525,
    -0.943593502,
    -0.95143503,
    -0.958703518,
    -0.965394378,
    -0.971503913,
    -0.977028072,
    -0.981963873,
    -0.986308098,
    -0.990058184,
    -0.993211925,
    -0.995767415,
    -0.997723103,
    -0.999077678,
    -0.999830604,
];

const TABLE7: [f32; 128] = [
    0.00613590004,
    0.0184067003,
    0.0306748003,
    0.0429382995,
    0.0551952012,
    0.0674438998,
    0.0796824023,
    0.0919089988,
    0.104121603,
    0.116318598,
    0.128498107,
    0.1406582,
    0.152797207,
    0.164913103,
    0.177004203,
    0.189068705,
    0.201104596,
    0.213110298,
    0.225083902,
    0.237023607,
    0.248927593,
    0.260794103,
    0.272621393,
    0.284407496,
    0.296150893,
    0.307849586,
    0.319501996,
    0.331106305,
    0.342660695,
    0.354163498,
    0.365613014,
    0.377007395,
    0.388345003,
    0.399624199,
    0.410843194,
    0.422000289,
    0.433093786,
    0.444122106,
    0.455083609,
    0.465976506,
    0.47679919,
    0.487550199,
    0.498227686,
    0.50883007,
    0.519356012,
    0.529803574,
    0.540171504,
    0.550458014,
    0.560661614,
    0.570780694,
    0.580814004,
    0.590759695,
    0.600616515,
    0.610382795,
    0.620057225,
    0.629638195,
    0.639124393,
    0.64851439,
    0.657806695,
    0.666999876,
    0.676092684,
    0.685083687,
    0.693971515,
    0.702754676,
    0.711432219,
    0.720002472,
    0.728464425,
    0.736816585,
    0.745057821,
    0.753186822,
    0.761202395,
    0.769103289,
    0.77688849,
    0.784556627,
    0.792106628,
    0.799537301,
    0.806847572,
    0.81403631,
    0.8211025,
    0.828045011,
    0.834862888,
    0.841554999,
    0.848120272,
    0.854557991,
    0.860866904,
    0.867046177,
    0.873094976,
    0.879012227,
    0.884797096,
    0.890448689,
    0.895966172,
    0.901348829,
    0.906595707,
    0.911705971,
    0.916679084,
    0.921513975,
    0.926210225,
    0.930767,
    0.935183525,
    0.939459205,
    0.943593502,
    0.947585583,
    0.95143503,
    0.955141187,
    0.958703518,
    0.962121427,
    0.965394378,
    0.968522072,
    0.971503913,
    0.974339426,
    0.977028072,
    0.979569793,
    0.981963873,
    0.984210074,
    0.986308098,
    0.988257587,
    0.990058184,
    0.991709828,
    0.993211925,
    0.994564593,
    0.995767415,
    0.996820271,
    0.997723103,
    0.998475611,
    0.999077678,
    0.999529421,
    0.999830604,
    0.999981225,
];

const TABLE10: [f32; 129] = [
    0.0,
    0.0122715384,
    0.024541229,
    0.0368072242,
    0.0490676723,
    0.061320737,
    0.0735645667,
    0.0857973099,
    0.0980171412,
    0.110222213,
    0.122410677,
    0.134580716,
    0.146730468,
    0.158858135,
    0.170961887,
    0.183039889,
    0.195090324,
    0.207111374,
    0.219101235,
    0.231058106,
    0.242980182,
    0.254865646,
    0.266712755,
    0.27851969,
    0.290284693,
    0.302005947,
    0.313681751,
    0.32531029,
    0.336889863,
    0.348418683,
    0.359895051,
    0.371317178,
    0.382683426,
    0.393992037,
    0.405241311,
    0.416429549,
    0.427555084,
    0.438616246,
    0.449611336,
    0.460538715,
    0.471396744,
    0.482183784,
    0.492898196,
    0.50353837,
    0.514102757,
    0.524589658,
    0.534997642,
    0.545324981,
    0.555570245,
    0.565731823,
    0.575808167,
    0.585797846,
    0.59569931,
    0.605511069,
    0.615231574,
    0.624859512,
    0.634393275,
    0.643831551,
    0.653172851,
    0.662415802,
    0.671558976,
    0.680601001,
    0.689540565,
    0.698376238,
    0.707106769,
    0.715730846,
    0.724247098,
    0.732654274,
    0.740951121,
    0.749136388,
    0.757208824,
    0.765167296,
    0.773010433,
    0.780737221,
    0.78834641,
    0.795836926,
    0.803207517,
    0.81045723,
    0.817584813,
    0.824589312,
    0.831469595,
    0.838224709,
    0.84485358,
    0.851355195,
    0.857728601,
    0.863972843,
    0.870086968,
    0.876070082,
    0.881921232,
    0.887639642,
    0.893224299,
    0.898674488,
    0.903989315,
    0.909168005,
    0.914209783,
    0.919113874,
    0.923879504,
    0.928506076,
    0.932992816,
    0.937339008,
    0.941544056,
    0.945607305,
    0.949528158,
    0.953306019,
    0.956940353,
    0.960430503,
    0.963776052,
    0.966976464,
    0.970031261,
    0.972939968,
    0.975702107,
    0.97831738,
    0.980785251,
    0.983105481,
    0.985277653,
    0.987301409,
    0.989176512,
    0.990902662,
    0.992479503,
    0.993906975,
    0.99518472,
    0.996312618,
    0.997290432,
    0.998118103,
    0.99879545,
    0.999322355,
    0.999698818,
    0.999924719,
    1.0,
];

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_synthetic_blocks() {
        assert_eq!(decode(&[0u8; 64 * 3]).len(), 256 * 3);
        assert_eq!(decode(&[0u8; 63]).len(), 0);
        assert_eq!(decode(&[0xffu8; 128 + 5]).len(), 512);

        let mut x = 0x1234_5678u32;
        let data: Vec<u8> = (0..64 * 500 + 10)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect();
        assert_eq!(decode(&data).len(), 256 * 500);
    }
}
