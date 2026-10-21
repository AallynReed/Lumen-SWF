//! Java number formatting and `java.util.Random`, as the XFL output depends on them.

/// A minimal unsigned big integer for `FloatingDecimal.dtoa`.
#[derive(Clone, PartialEq, Eq)]
struct Big(Vec<u32>);

impl Big {
    fn from(v: u64) -> Big {
        let mut b = Big(vec![v as u32, (v >> 32) as u32]);
        b.trim();
        b
    }

    fn trim(&mut self) {
        while self.0.len() > 1 && *self.0.last().unwrap() == 0 {
            self.0.pop();
        }
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for d in &mut self.0 {
            let v = *d as u64 * m as u64 + carry;
            *d = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            self.0.push(carry as u32);
        }
    }

    fn shl(&mut self, bits: i32) {
        if bits <= 0 {
            return;
        }
        let (words, rest) = ((bits / 32) as usize, bits % 32);
        if rest > 0 {
            let mut carry = 0u32;
            for d in &mut self.0 {
                let v = (*d as u64) << rest;
                *d = v as u32 | carry;
                carry = (v >> 32) as u32;
            }
            if carry > 0 {
                self.0.push(carry);
            }
        }
        if words > 0 {
            let mut v = vec![0u32; words];
            v.extend_from_slice(&self.0);
            self.0 = v;
        }
        self.trim();
    }

    fn pow52(p5: i32, p2: i32) -> Big {
        let mut b = Big::from(1);
        for _ in 0..p5 {
            b.mul_small(5);
        }
        b.shl(p2);
        b
    }

    fn cmp(&self, o: &Big) -> std::cmp::Ordering {
        let (a, b) = (&self.0, &o.0);
        if a.len() != b.len() {
            return a.len().cmp(&b.len());
        }
        for i in (0..a.len()).rev() {
            if a[i] != b[i] {
                return a[i].cmp(&b[i]);
            }
        }
        std::cmp::Ordering::Equal
    }

    fn add(&self, o: &Big) -> Big {
        let n = self.0.len().max(o.0.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let v = *self.0.get(i).unwrap_or(&0) as u64 + *o.0.get(i).unwrap_or(&0) as u64 + carry;
            out.push(v as u32);
            carry = v >> 32;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        let mut b = Big(out);
        b.trim();
        b
    }

    fn sub(&mut self, o: &Big) {
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let v = self.0[i] as i64 - *o.0.get(i).unwrap_or(&0) as i64 - borrow;
            borrow = (v < 0) as i64;
            self.0[i] = v.rem_euclid(1 << 32) as u32;
        }
        self.trim();
    }

    /// `quoRemIteration`: the quotient digit, leaving ten times the remainder.
    fn quo_rem(&mut self, s: &Big) -> i32 {
        let mut q = 0;
        while self.cmp(s) != std::cmp::Ordering::Less {
            self.sub(s);
            q += 1;
        }
        self.mul_small(10);
        q
    }
}

const SMALL_5_POW: [i32; 14] = [1, 5, 25, 125, 625, 3125, 15625, 78125, 390625, 1953125, 9765625, 48828125, 244140625, 1220703125];
const N_5_BITS: [i32; 27] = [0, 3, 5, 7, 10, 12, 14, 17, 19, 21, 24, 26, 28, 31, 33, 35, 38, 40, 42, 45, 47, 49, 52, 54, 56, 59, 61];
const INSIGNIFICANT: [i32; 64] = [
    0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 9, 10, 10, 10, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 14, 14, 14, 15, 15, 15, 15, 16, 16, 16, 17, 17, 17, 18, 18, 18, 19,
];
const SIGNIF: u64 = (1 << 52) - 1;

fn long_5_pow(i: i32) -> i64 {
    5i64.pow(i as u32)
}

fn estimate_dec_exp(fract_bits: i64, bin_exp: i32) -> i32 {
    let d2 = f64::from_bits((1023u64 << 52) | (fract_bits as u64 & SIGNIF));
    let d = (d2 - 1.5) * 0.289529654 + 0.176091259 + bin_exp as f64 * 0.301029995663981;
    let bits = d.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let negative = bits >> 63 != 0;
    if (0..52).contains(&exponent) {
        let mask = SIGNIF >> exponent;
        let r = (((bits & SIGNIF) | (1u64 << 52)) >> (52 - exponent)) as i32;
        match (negative, mask & bits == 0) {
            (false, _) => r,
            (true, true) => -r,
            (true, false) => -r - 1,
        }
    } else if exponent < 0 {
        if bits & !(1u64 << 63) == 0 || !negative {
            0
        } else {
            -1
        }
    } else {
        d as i32
    }
}

fn roundup(digits: &mut [u8], dec_exponent: &mut i32) {
    let mut i = digits.len() - 1;
    let mut q = digits[i];
    if q == b'9' {
        while q == b'9' && i > 0 {
            digits[i] = b'0';
            i -= 1;
            q = digits[i];
        }
        if q == b'9' {
            *dec_exponent += 1;
            digits[0] = b'1';
            return;
        }
    }
    digits[i] = q + 1;
}

fn develop_long_digits(mut v: i64, insignificant: i32) -> (Vec<u8>, i32) {
    let mut dec_exponent = 0;
    if insignificant != 0 {
        let pow10 = long_5_pow(insignificant) << insignificant;
        let residue = v % pow10;
        v /= pow10;
        dec_exponent += insignificant;
        if residue >= pow10 >> 1 {
            v += 1;
        }
    }
    let mut out = Vec::new();
    let mut c = v % 10;
    v /= 10;
    while c == 0 {
        dec_exponent += 1;
        c = v % 10;
        v /= 10;
    }
    while v != 0 {
        out.push(b'0' + c as u8);
        dec_exponent += 1;
        c = v % 10;
        v /= 10;
    }
    out.push(b'0' + c as u8);
    out.reverse();
    (out, dec_exponent + 1)
}

/// One step of the digit loop on machine integers, as the int and long paths of `dtoa` do.
macro_rules! small_digits {
    ($t:ty, $fract:expr, $b5:expr, $b2:expr, $s5:expr, $s2:expr, $m5:expr, $m2:expr, $pow:expr, $dec_exp:expr, $digits:expr) => {{
        let mut b: $t = ($fract as $t).wrapping_mul($pow($b5)).wrapping_shl($b2 as u32);
        let s: $t = $pow($s5).wrapping_shl($s2 as u32);
        let mut m: $t = $pow($m5).wrapping_shl($m2 as u32);
        let tens = s.wrapping_mul(10);
        let mut q = b / s;
        b = (b % s).wrapping_mul(10);
        m = m.wrapping_mul(10);
        let mut lo = b < m;
        let mut hi = b.wrapping_add(m) > tens;
        if q == 0 && !hi {
            $dec_exp -= 1;
        } else {
            $digits.push(b'0' + q as u8);
        }
        if $dec_exp < -3 || $dec_exp >= 8 {
            hi = false;
            lo = false;
        }
        while !lo && !hi {
            q = b / s;
            b = (b % s).wrapping_mul(10);
            m = m.wrapping_mul(10);
            if m > 0 {
                lo = b < m;
                hi = b.wrapping_add(m) > tens;
            } else {
                lo = true;
                hi = true;
            }
            $digits.push(b'0' + q as u8);
        }
        (lo, hi, b.wrapping_shl(1).wrapping_sub(tens) as i64)
    }};
}

/// `FloatingDecimal.BinaryToASCIIBuffer.dtoa` (compatible format): digits and decimal exponent.
fn dtoa(bin_exp: i32, mut fract_bits: i64, n_significant: i32) -> (Vec<u8>, i32) {
    let tail_zeros = fract_bits.trailing_zeros() as i32;
    let n_fract_bits = 53 - tail_zeros;
    let n_tiny = 0.max(n_fract_bits - bin_exp - 1);
    if (-21..=62).contains(&bin_exp) && n_tiny == 0 && n_fract_bits < 64 {
        let insignificant = if bin_exp > n_significant {
            let p2 = bin_exp - n_significant - 1;
            if p2 > 1 && (p2 as usize) < INSIGNIFICANT.len() {
                INSIGNIFICANT[p2 as usize]
            } else {
                0
            }
        } else {
            0
        };
        if bin_exp >= 52 {
            fract_bits <<= bin_exp - 52;
        } else {
            fract_bits = ((fract_bits as u64) >> (52 - bin_exp)) as i64;
        }
        return develop_long_digits(fract_bits, insignificant);
    }
    let mut dec_exp = estimate_dec_exp(fract_bits, bin_exp);
    let b5 = 0.max(-dec_exp);
    let mut b2 = b5 + n_tiny + bin_exp;
    let s5 = 0.max(dec_exp);
    let mut s2 = s5 + n_tiny;
    let m5 = b5;
    let mut m2 = b2 - n_significant;
    fract_bits = ((fract_bits as u64) >> tail_zeros) as i64;
    b2 -= n_fract_bits - 1;
    let common = b2.min(s2);
    b2 -= common;
    s2 -= common;
    m2 -= common;
    if n_fract_bits == 1 {
        m2 -= 1;
    }
    if m2 < 0 {
        b2 -= m2;
        s2 -= m2;
        m2 = 0;
    }
    let mut digits: Vec<u8> = Vec::new();
    let n5 = |i: i32| if (i as usize) < N_5_BITS.len() { N_5_BITS[i as usize] } else { i * 3 };
    let b_bits = n_fract_bits + b2 + n5(b5);
    let ten_s_bits = s2 + 1 + n5(s5 + 1);
    let (low, high, low_diff) = if b_bits < 64 && ten_s_bits < 64 {
        if b_bits < 32 && ten_s_bits < 32 {
            small_digits!(i32, fract_bits, b5, b2, s5, s2, m5, m2, |i: i32| SMALL_5_POW[i as usize], dec_exp, digits)
        } else {
            small_digits!(i64, fract_bits, b5, b2, s5, s2, m5, m2, long_5_pow, dec_exp, digits)
        }
    } else {
        let sv = Big::pow52(s5, s2);
        let mut bv = Big::from(fract_bits as u64);
        for _ in 0..b5 {
            bv.mul_small(5);
        }
        bv.shl(b2);
        let mut mv = Big::pow52(m5 + 1, m2 + 1);
        let ten_s = Big::pow52(s5 + 1, s2 + 1);
        let mut q = bv.quo_rem(&sv);
        let mut lo = bv.cmp(&mv).is_lt();
        let mut hi = ten_s.cmp(&bv.add(&mv)).is_le();
        if q == 0 && !hi {
            dec_exp -= 1;
        } else {
            digits.push(b'0' + q as u8);
        }
        if dec_exp < -3 || dec_exp >= 8 {
            hi = false;
            lo = false;
        }
        while !lo && !hi {
            q = bv.quo_rem(&sv);
            mv.mul_small(10);
            lo = bv.cmp(&mv).is_lt();
            hi = ten_s.cmp(&bv.add(&mv)).is_le();
            digits.push(b'0' + q as u8);
        }
        let diff = if hi && lo {
            bv.shl(1);
            bv.cmp(&ten_s) as i64
        } else {
            0
        };
        (lo, hi, diff)
    };
    let mut dec_exponent = dec_exp + 1;
    if high && (!low || low_diff > 0 || (low_diff == 0 && digits.last().is_some_and(|d| d & 1 != 0))) {
        roundup(&mut digits, &mut dec_exponent);
    }
    (digits, dec_exponent)
}

/// `BinaryToASCIIBuffer.getChars`.
fn java_chars(negative: bool, digits: &[u8], dec_exponent: i32) -> String {
    let d = std::str::from_utf8(digits).unwrap_or("0");
    let n = digits.len() as i32;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if dec_exponent > 0 && dec_exponent < 8 {
        let len = n.min(dec_exponent);
        out += &d[..len as usize];
        if len < dec_exponent {
            out += &"0".repeat((dec_exponent - len) as usize);
            out += ".0";
        } else {
            out.push('.');
            if len < n {
                out += &d[len as usize..];
            } else {
                out.push('0');
            }
        }
    } else if dec_exponent <= 0 && dec_exponent > -3 {
        out += "0.";
        out += &"0".repeat((-dec_exponent) as usize);
        out += d;
    } else {
        out += &d[..1];
        out.push('.');
        if n > 1 {
            out += &d[1..];
        } else {
            out.push('0');
        }
        out.push('E');
        if dec_exponent <= 0 {
            out.push('-');
            out += &(-dec_exponent + 1).to_string();
        } else {
            out += &(dec_exponent - 1).to_string();
        }
    }
    out
}

/// `Float.toString` (JDK 17).
pub fn java_float(v: f32) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let bits = v.to_bits();
    let negative = bits >> 31 != 0;
    let mut fract = (bits & 0x7f_ffff) as i32;
    let mut bin_exp = ((bits >> 23) & 0xff) as i32;
    if bin_exp == 0 && fract == 0 {
        return if negative { "-0.0".into() } else { "0.0".into() };
    }
    let n_sig;
    if bin_exp == 0 {
        let lz = fract.leading_zeros() as i32;
        let shift = lz - (31 - 23);
        fract <<= shift;
        bin_exp = 1 - shift;
        n_sig = 32 - lz;
    } else {
        fract |= 1 << 23;
        n_sig = 24;
    }
    bin_exp -= 127;
    let (digits, e) = dtoa(bin_exp, (fract as i64) << (52 - 23), n_sig);
    java_chars(negative, &digits, e)
}

/// `Double.toString` (JDK 17's FloatingDecimal, which is not always the shortest form).
pub fn java_double(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let bits = v.to_bits();
    let negative = bits >> 63 != 0;
    let mut fract = (bits & SIGNIF) as i64;
    let mut bin_exp = ((bits >> 52) & 0x7ff) as i32;
    if bin_exp == 0 && fract == 0 {
        return if negative { "-0.0".into() } else { "0.0".into() };
    }
    let n_sig;
    if bin_exp == 0 {
        let lz = fract.leading_zeros() as i32;
        let shift = lz - (63 - 52);
        fract <<= shift;
        bin_exp = 1 - shift;
        n_sig = 64 - lz;
    } else {
        fract |= 1 << 52;
        n_sig = 53;
    }
    bin_exp -= 1023;
    let (digits, e) = dtoa(bin_exp, fract, n_sig);
    java_chars(negative, &digits, e)
}

/// FFDec's `doubleToString`: Java's form without a trailing `.0`.
pub fn double_str(v: f64) -> String {
    let s = java_double(v);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// `doubleToString(d, precision)`.
pub fn double_str_prec(v: f64, precision: i32) -> String {
    let m = 10f64.powi(precision);
    double_str(java_round(v * m) as f64 / m)
}

/// `Math.round` for doubles.
pub fn java_round(v: f64) -> i64 {
    if v.is_nan() {
        return 0;
    }
    (v + 0.5).floor() as i64
}

/// `java.util.Random`.
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    pub fn new(seed: u64) -> JavaRandom {
        JavaRandom { seed: (seed ^ 0x5DEECE66D) & ((1 << 48) - 1) }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.seed >> (48 - bits)) as i32
    }

    pub fn next_int(&mut self, bound: i32) -> i32 {
        if bound & (-bound) == bound {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits - val + (bound - 1) >= 0 {
                return val;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_java() {
        assert_eq!(java_double(1.0), "1.0");
        assert_eq!(java_double(0.05), "0.05");
        assert_eq!(java_double(1e7), "1.0E7");
        assert_eq!(java_double(1.5e-5), "1.5E-5");
        assert_eq!(java_float(0.1), "0.1");
        assert_eq!(double_str(24.0), "24");
        let mut r = JavaRandom::new(123);
        assert_eq!((0..6).map(|_| r.next_int(256)).collect::<Vec<_>>(), [185, 60, 253, 77, 64, 146]);
        assert_eq!(java_double(-0.0), "-0.0");
        assert_eq!(java_double(123456789.0), "1.23456789E8");
        assert_eq!(java_double(15.609634399414062), "15.609634399414062");
        assert_eq!(java_double(2e-3), "0.002");
        assert_eq!(java_double(1.0E23), "9.999999999999999E22");
        assert_eq!(java_double(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(java_float(1.0E-5), "1.0E-5");
        assert_eq!(java_float(3.4028235E38), "3.4028235E38");
    }
}

#[cfg(test)]
mod java_samples {
    /// Compares against `Double.toString`/`Float.toString` output listed in `FFDEC_JAVA_DOUBLES`.
    #[test]
    fn matches_java_samples() {
        let Ok(path) = std::env::var("FFDEC_JAVA_DOUBLES") else { return };
        let text = std::fs::read_to_string(path).unwrap();
        let mut bad = 0;
        for line in text.lines() {
            let p: Vec<&str> = line.split(' ').collect();
            let d = f64::from_bits(u64::from_str_radix(p[0], 16).unwrap());
            let f = f32::from_bits(u32::from_str_radix(p[2], 16).unwrap());
            if super::java_double(d) != p[1] || super::java_float(f) != p[3] {
                if bad < 10 {
                    eprintln!("{line} -> {} {}", super::java_double(d), super::java_float(f));
                }
                bad += 1;
            }
        }
        assert_eq!(bad, 0);
    }
}
