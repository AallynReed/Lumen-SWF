//! The subset of `java.math.BigDecimal`/`MathContext`/`RoundingMode` that `Decimal128` relies on.

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundingMode {
    Up,
    Down,
    Ceiling,
    Floor,
    HalfUp,
    HalfDown,
    HalfEven,
    Unnecessary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathContext {
    pub precision: i32,
    pub rounding_mode: RoundingMode,
}

impl MathContext {
    pub const DECIMAL128: MathContext = MathContext { precision: 34, rounding_mode: RoundingMode::HalfEven };

    pub fn new(precision: i32, rounding_mode: RoundingMode) -> MathContext {
        MathContext { precision, rounding_mode }
    }
}

/// Java's `ArithmeticException`/`NumberFormatException` message.
pub type ArithResult<T> = Result<T, String>;

/// Magnitude as decimal digits, most significant first, without leading zeros (zero is empty).
type Mag = Vec<u8>;

fn trim(mut m: Mag) -> Mag {
    let nz = m.iter().position(|&d| d != 0).unwrap_or(m.len());
    m.drain(..nz);
    m
}

fn mag_cmp(a: &[u8], b: &[u8]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn mag_add(a: &[u8], b: &[u8]) -> Mag {
    let n = a.len().max(b.len());
    let mut out = vec![0u8; n + 1];
    let mut carry = 0u8;
    for i in 0..n {
        let da = if i < a.len() { a[a.len() - 1 - i] } else { 0 };
        let db = if i < b.len() { b[b.len() - 1 - i] } else { 0 };
        let s = da + db + carry;
        out[n - i] = s % 10;
        carry = s / 10;
    }
    out[0] = carry;
    trim(out)
}

/// `a - b`, requires `a >= b`.
fn mag_sub(a: &[u8], b: &[u8]) -> Mag {
    let mut out = a.to_vec();
    let mut borrow = 0i8;
    for i in 0..a.len() {
        let idx = a.len() - 1 - i;
        let db = if i < b.len() { b[b.len() - 1 - i] as i8 } else { 0 };
        let mut d = a[idx] as i8 - db - borrow;
        if d < 0 {
            d += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out[idx] = d as u8;
    }
    trim(out)
}

fn mag_mul(a: &[u8], b: &[u8]) -> Mag {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut acc = vec![0u32; a.len() + b.len()];
    for (i, &da) in a.iter().enumerate().rev() {
        for (j, &db) in b.iter().enumerate().rev() {
            acc[i + j + 1] += da as u32 * db as u32;
        }
    }
    for k in (1..acc.len()).rev() {
        let c = acc[k] / 10;
        acc[k] %= 10;
        acc[k - 1] += c;
    }
    trim(acc.into_iter().map(|d| d as u8).collect())
}

fn mag_divrem(a: &[u8], b: &[u8]) -> (Mag, Mag) {
    let mut q = Vec::with_capacity(a.len());
    let mut r: Mag = Vec::new();
    for &d in a {
        r.push(d);
        r = trim(r);
        let mut digit = 0u8;
        while mag_cmp(&r, b) != Ordering::Less {
            r = mag_sub(&r, b);
            digit += 1;
        }
        q.push(digit);
    }
    (trim(q), r)
}

fn mag_pow10(a: &[u8], n: i64) -> Mag {
    if a.is_empty() {
        return Vec::new();
    }
    let mut m = a.to_vec();
    m.extend(std::iter::repeat(0).take(n as usize));
    m
}

fn ten_pow(n: i64) -> Mag {
    mag_pow10(&[1], n)
}

fn mag_from_u64(mut v: u64) -> Mag {
    let mut m = Vec::new();
    while v > 0 {
        m.push((v % 10) as u8);
        v /= 10;
    }
    m.reverse();
    m
}

fn mag_inc(a: &[u8]) -> Mag {
    mag_add(a, &[1])
}

fn is_odd(a: &[u8]) -> bool {
    a.last().is_some_and(|d| d % 2 == 1)
}

fn need_increment(divisor: &[u8], mode: RoundingMode, qsign: i32, q: &[u8], r: &[u8]) -> ArithResult<bool> {
    match mode {
        RoundingMode::Unnecessary => Err("Rounding necessary".to_string()),
        RoundingMode::Up => Ok(true),
        RoundingMode::Down => Ok(false),
        RoundingMode::Ceiling => Ok(qsign > 0),
        RoundingMode::Floor => Ok(qsign < 0),
        _ => {
            let twice = mag_add(r, r);
            match mag_cmp(&twice, divisor) {
                Ordering::Less => Ok(false),
                Ordering::Greater => Ok(true),
                Ordering::Equal => Ok(match mode {
                    RoundingMode::HalfUp => true,
                    RoundingMode::HalfDown => false,
                    _ => is_odd(q),
                }),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BigDecimal {
    neg: bool,
    mag: Mag,
    scale: i32,
}

impl BigDecimal {
    fn make(neg: bool, mag: Mag, scale: i32) -> BigDecimal {
        let mag = trim(mag);
        let neg = neg && !mag.is_empty();
        BigDecimal { neg, mag, scale }
    }

    pub fn zero_with_scale(scale: i32) -> BigDecimal {
        BigDecimal { neg: false, mag: Vec::new(), scale }
    }

    pub fn from_i64(v: i64) -> BigDecimal {
        BigDecimal::make(v < 0, mag_from_u64(v.unsigned_abs()), 0)
    }

    pub fn from_i64_mc(v: i64, mc: MathContext) -> ArithResult<BigDecimal> {
        BigDecimal::from_i64(v).round(mc)
    }

    /// `new BigDecimal(BigInteger, scale)` with the unscaled value given as sign and decimal digits.
    pub fn from_digits(neg: bool, digits: &[u8], scale: i32) -> BigDecimal {
        BigDecimal::make(neg, digits.to_vec(), scale)
    }

    /// `new BigDecimal(double, MathContext)`.
    pub fn from_f64_mc(val: f64, mc: MathContext) -> ArithResult<BigDecimal> {
        if val.is_nan() || val.is_infinite() {
            return Err("Infinite or NaN".to_string());
        }
        if val == 0.0 {
            return Ok(BigDecimal::zero_with_scale(0));
        }
        let bits = val.to_bits();
        let sign = (bits >> 63) != 0;
        let mut exponent = ((bits >> 52) & 0x7ff) as i64;
        let mut significand = if exponent == 0 { (bits & ((1u64 << 52) - 1)) << 1 } else { (bits & ((1u64 << 52) - 1)) | (1u64 << 52) };
        exponent -= 1075;
        while significand & 1 == 0 {
            significand >>= 1;
            exponent += 1;
        }
        let mut mag = mag_from_u64(significand);
        let scale;
        if exponent < 0 {
            for _ in 0..-exponent {
                mag = mag_mul(&mag, &[5]);
            }
            scale = -exponent as i32;
        } else {
            for _ in 0..exponent {
                mag = mag_mul(&mag, &[2]);
            }
            scale = 0;
        }
        BigDecimal::make(sign, mag, scale).round(mc)
    }

    /// `new BigDecimal(char[], offset, len, MathContext)`.
    pub fn parse_mc(s: &[char], mc: MathContext) -> ArithResult<BigDecimal> {
        let err = || Err("Character array is missing \"exponent\" mark 'e' or 'E'.".to_string());
        let mut i = 0;
        let mut neg = false;
        if i < s.len() && (s[i] == '-' || s[i] == '+') {
            neg = s[i] == '-';
            i += 1;
        }
        let mut digits = Vec::new();
        let mut seen_dot = false;
        let mut frac = 0i64;
        let mut any = false;
        while i < s.len() {
            let c = s[i];
            if c.is_ascii_digit() {
                digits.push(c as u8 - b'0');
                any = true;
                if seen_dot {
                    frac += 1;
                }
            } else if c == '.' {
                if seen_dot {
                    return Err("Character array contains more than one decimal point.".to_string());
                }
                seen_dot = true;
            } else if c == 'e' || c == 'E' {
                break;
            } else {
                return err();
            }
            i += 1;
        }
        if !any {
            return Err("No digits found.".to_string());
        }
        let mut exp = 0i64;
        if i < s.len() {
            i += 1;
            let mut eneg = false;
            if i < s.len() && (s[i] == '-' || s[i] == '+') {
                eneg = s[i] == '-';
                i += 1;
            }
            if i >= s.len() {
                return Err("No exponent digits.".to_string());
            }
            while i < s.len() {
                let c = s[i];
                if !c.is_ascii_digit() {
                    return err();
                }
                exp = exp * 10 + (c as u8 - b'0') as i64;
                if exp > i32::MAX as i64 {
                    return Err("Exponent overflow.".to_string());
                }
                i += 1;
            }
            if eneg {
                exp = -exp;
            }
        }
        let scale = frac - exp;
        if scale > i32::MAX as i64 || scale < i32::MIN as i64 {
            return Err("Scale out of range.".to_string());
        }
        BigDecimal::make(neg, digits, scale as i32).round(mc)
    }

    pub fn parse(s: &str) -> ArithResult<BigDecimal> {
        let chars: Vec<char> = s.chars().collect();
        BigDecimal::parse_mc(&chars, MathContext::new(0, RoundingMode::HalfUp))
    }

    pub fn signum(&self) -> i32 {
        if self.mag.is_empty() {
            0
        } else if self.neg {
            -1
        } else {
            1
        }
    }

    pub fn scale(&self) -> i32 {
        self.scale
    }

    pub fn precision(&self) -> i32 {
        if self.mag.is_empty() {
            1
        } else {
            self.mag.len() as i32
        }
    }

    /// Digits of `unscaledValue().abs().toString()`.
    pub fn unscaled_abs_string(&self) -> String {
        if self.mag.is_empty() {
            "0".to_string()
        } else {
            self.mag.iter().map(|d| (b'0' + d) as char).collect()
        }
    }

    pub fn abs(&self) -> BigDecimal {
        BigDecimal { neg: false, mag: self.mag.clone(), scale: self.scale }
    }

    pub fn abs_mc(&self, mc: MathContext) -> ArithResult<BigDecimal> {
        self.abs().round(mc)
    }

    pub fn negate(&self) -> BigDecimal {
        BigDecimal::make(!self.neg, self.mag.clone(), self.scale)
    }

    pub fn negate_mc(&self, mc: MathContext) -> ArithResult<BigDecimal> {
        self.negate().round(mc)
    }

    pub fn compare_to(&self, o: &BigDecimal) -> i32 {
        let (s1, s2) = (self.signum(), o.signum());
        if s1 != s2 {
            return if s1 > s2 { 1 } else { -1 };
        }
        if s1 == 0 {
            return 0;
        }
        let m = self.compare_magnitude(o);
        if s1 < 0 {
            -m
        } else {
            m
        }
    }

    fn compare_magnitude(&self, o: &BigDecimal) -> i32 {
        let sc = self.scale.max(o.scale) as i64;
        let a = mag_pow10(&self.mag, sc - self.scale as i64);
        let b = mag_pow10(&o.mag, sc - o.scale as i64);
        match mag_cmp(&a, &b) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    /// `doRound(this, mc)`.
    pub fn round(self, mc: MathContext) -> ArithResult<BigDecimal> {
        let mut val = self;
        if mc.precision <= 0 {
            return Ok(val);
        }
        let mut drop = val.precision() - mc.precision;
        while drop > 0 {
            let qsign = if val.neg { -1 } else { 1 };
            let divisor = ten_pow(drop as i64);
            let (q, r) = mag_divrem(&val.mag, &divisor);
            let q = if r.is_empty() {
                q
            } else if need_increment(&divisor, mc.rounding_mode, qsign, &q, &r)? {
                mag_inc(&q)
            } else {
                q
            };
            val = BigDecimal::make(val.neg, q, val.scale - drop);
            drop = val.precision() - mc.precision;
        }
        Ok(val)
    }

    pub fn set_scale(&self, new_scale: i32, mode: RoundingMode) -> ArithResult<BigDecimal> {
        let old = self.scale;
        if new_scale == old {
            return Ok(self.clone());
        }
        if new_scale > old {
            return Ok(BigDecimal::make(self.neg, mag_pow10(&self.mag, new_scale as i64 - old as i64), new_scale));
        }
        let divisor = ten_pow(old as i64 - new_scale as i64);
        let (q, r) = mag_divrem(&self.mag, &divisor);
        let qsign = if self.neg { -1 } else { 1 };
        let q = if r.is_empty() {
            q
        } else if need_increment(&divisor, mode, qsign, &q, &r)? {
            mag_inc(&q)
        } else {
            q
        };
        Ok(BigDecimal::make(self.neg, q, new_scale))
    }

    fn strip_zeros_to_match_scale(self, preferred_scale: i64) -> BigDecimal {
        let mut v = self;
        while v.mag.len() >= 2 && (v.scale as i64) > preferred_scale {
            if *v.mag.last().unwrap() != 0 {
                break;
            }
            v.mag.pop();
            v.scale -= 1;
        }
        v
    }

    fn signed_add(a: &BigDecimal, b: &BigDecimal) -> BigDecimal {
        let sc = a.scale.max(b.scale);
        let am = mag_pow10(&a.mag, sc as i64 - a.scale as i64);
        let bm = mag_pow10(&b.mag, sc as i64 - b.scale as i64);
        if a.neg == b.neg {
            BigDecimal::make(a.neg, mag_add(&am, &bm), sc)
        } else {
            match mag_cmp(&am, &bm) {
                Ordering::Less => BigDecimal::make(b.neg, mag_sub(&bm, &am), sc),
                _ => BigDecimal::make(a.neg, mag_sub(&am, &bm), sc),
            }
        }
    }

    pub fn add(&self, o: &BigDecimal) -> BigDecimal {
        BigDecimal::signed_add(self, o)
    }

    pub fn subtract(&self, o: &BigDecimal) -> BigDecimal {
        BigDecimal::signed_add(self, &o.negate())
    }

    pub fn add_mc(&self, augend: &BigDecimal, mc: MathContext) -> ArithResult<BigDecimal> {
        if mc.precision == 0 {
            return Ok(self.add(augend));
        }
        let lhs_zero = self.signum() == 0;
        let aug_zero = augend.signum() == 0;
        if lhs_zero || aug_zero {
            let preferred = self.scale.max(augend.scale);
            if lhs_zero && aug_zero {
                return Ok(BigDecimal::zero_with_scale(preferred));
            }
            let result = if lhs_zero { augend.clone().round(mc)? } else { self.clone().round(mc)? };
            if result.scale == preferred {
                return Ok(result);
            } else if result.scale > preferred {
                return Ok(result.strip_zeros_to_match_scale(preferred as i64));
            } else {
                let precision_diff = mc.precision - result.precision();
                let scale_diff = preferred - result.scale;
                return if precision_diff >= scale_diff {
                    result.set_scale(preferred, RoundingMode::Unnecessary)
                } else {
                    result.set_scale(result.scale + precision_diff, RoundingMode::Unnecessary)
                };
            }
        }
        self.add(augend).round(mc)
    }

    pub fn subtract_mc(&self, o: &BigDecimal, mc: MathContext) -> ArithResult<BigDecimal> {
        self.add_mc(&o.negate(), mc)
    }

    pub fn multiply(&self, o: &BigDecimal) -> BigDecimal {
        BigDecimal::make(self.neg != o.neg, mag_mul(&self.mag, &o.mag), self.scale.wrapping_add(o.scale))
    }

    pub fn multiply_mc(&self, o: &BigDecimal, mc: MathContext) -> ArithResult<BigDecimal> {
        self.multiply(o).round(mc)
    }

    pub fn divide_mc(&self, divisor: &BigDecimal, mc: MathContext) -> ArithResult<BigDecimal> {
        if mc.precision == 0 {
            return Err("exact divide is not supported".to_string());
        }
        let preferred = self.scale as i64 - divisor.scale as i64;
        if divisor.signum() == 0 {
            return Err(if self.signum() == 0 { "Division undefined" } else { "Division by zero" }.to_string());
        }
        if self.signum() == 0 {
            return Ok(BigDecimal::zero_with_scale(preferred.clamp(i32::MIN as i64, i32::MAX as i64) as i32));
        }
        let xscale = self.precision() as i64;
        let mut yscale = divisor.precision() as i64;
        let mcp = mc.precision as i64;
        let xn = mag_pow10(&self.mag, yscale);
        let yn = mag_pow10(&divisor.mag, xscale);
        if mag_cmp(&xn, &yn) == Ordering::Greater {
            yscale -= 1;
        }
        let scl = preferred + yscale - xscale + mcp;
        let raise = mcp + yscale - xscale;
        let (num, den) = if raise > 0 {
            (mag_pow10(&self.mag, raise), divisor.mag.clone())
        } else {
            (self.mag.clone(), mag_pow10(&divisor.mag, -raise))
        };
        let qsign = if self.neg == divisor.neg { 1 } else { -1 };
        let (q, r) = mag_divrem(&num, &den);
        let quotient = if !r.is_empty() {
            let q = if need_increment(&den, mc.rounding_mode, qsign, &q, &r)? { mag_inc(&q) } else { q };
            BigDecimal::make(qsign < 0, q, scl as i32)
        } else if preferred != scl {
            BigDecimal::make(qsign < 0, q, scl as i32).strip_zeros_to_match_scale(preferred)
        } else {
            BigDecimal::make(qsign < 0, q, scl as i32)
        };
        quotient.round(mc)
    }

    fn divide_to_integral_value_mc(&self, divisor: &BigDecimal, mc: MathContext) -> ArithResult<BigDecimal> {
        let preferred = (self.scale as i64 - divisor.scale as i64).clamp(i32::MIN as i64, i32::MAX as i64);
        if self.compare_magnitude(divisor) < 0 {
            return Ok(BigDecimal::zero_with_scale(preferred as i32));
        }
        let mut result = self.divide_mc(divisor, MathContext::new(mc.precision, RoundingMode::Down))?;
        if result.scale < 0 {
            let product = result.multiply(divisor);
            if self.subtract(&product).compare_magnitude(divisor) >= 0 {
                return Err("Division impossible".to_string());
            }
        } else if result.scale > 0 {
            result = result.set_scale(0, RoundingMode::Down)?;
        }
        let precision_diff = mc.precision - result.precision();
        if preferred > result.scale as i64 && precision_diff > 0 {
            let add = (precision_diff as i64).min(preferred - result.scale as i64) as i32;
            let s = result.scale + add;
            result.set_scale(s, RoundingMode::Unnecessary)
        } else {
            Ok(result.strip_zeros_to_match_scale(preferred))
        }
    }

    pub fn remainder_mc(&self, divisor: &BigDecimal, mc: MathContext) -> ArithResult<BigDecimal> {
        if mc.precision == 0 {
            return Err("exact remainder is not supported".to_string());
        }
        if divisor.signum() == 0 {
            return Err("Division by zero".to_string());
        }
        let q = self.divide_to_integral_value_mc(divisor, mc)?;
        Ok(self.subtract(&q.multiply(divisor)))
    }

    fn layout(&self, sci: bool) -> String {
        let coeff = self.unscaled_abs_string();
        let mut buf = String::new();
        if self.neg {
            buf.push('-');
        }
        if self.scale == 0 {
            buf.push_str(&coeff);
            return buf;
        }
        let coeff_len = coeff.len() as i64;
        let mut adjusted = -(self.scale as i64) + (coeff_len - 1);
        if self.scale >= 0 && adjusted >= -6 {
            let pad = self.scale as i64 - coeff_len;
            if pad >= 0 {
                buf.push_str("0.");
                for _ in 0..pad {
                    buf.push('0');
                }
                buf.push_str(&coeff);
            } else {
                let p = (-pad) as usize;
                buf.push_str(&coeff[..p]);
                buf.push('.');
                buf.push_str(&coeff[p..]);
            }
            return buf;
        }
        if sci {
            buf.push_str(&coeff[..1]);
            if coeff_len > 1 {
                buf.push('.');
                buf.push_str(&coeff[1..]);
            }
        } else {
            let mut sig = (adjusted % 3) as i32;
            if sig < 0 {
                sig += 3;
            }
            adjusted -= sig as i64;
            sig += 1;
            if self.signum() == 0 {
                match sig {
                    1 => buf.push('0'),
                    2 => {
                        buf.push_str("0.00");
                        adjusted += 3;
                    }
                    _ => {
                        buf.push_str("0.0");
                        adjusted += 3;
                    }
                }
            } else if sig as i64 >= coeff_len {
                buf.push_str(&coeff);
                for _ in 0..(sig as i64 - coeff_len) {
                    buf.push('0');
                }
            } else {
                buf.push_str(&coeff[..sig as usize]);
                buf.push('.');
                buf.push_str(&coeff[sig as usize..]);
            }
        }
        if adjusted != 0 {
            buf.push('E');
            if adjusted > 0 {
                buf.push('+');
            }
            buf.push_str(&adjusted.to_string());
        }
        buf
    }

    pub fn to_engineering_string(&self) -> String {
        self.layout(false)
    }

    fn to_big_integer_bits(&self) -> u64 {
        let digits: &[u8] = if self.scale > 0 {
            let keep = self.mag.len() as i64 - self.scale as i64;
            if keep <= 0 {
                &[]
            } else {
                &self.mag[..keep as usize]
            }
        } else {
            &self.mag
        };
        let mut v = 0u64;
        for &d in digits {
            v = v.wrapping_mul(10).wrapping_add(d as u64);
        }
        if self.scale < 0 {
            for _ in 0..-(self.scale as i64) {
                v = v.wrapping_mul(10);
                if v == 0 {
                    break;
                }
            }
        }
        if self.neg {
            v.wrapping_neg()
        } else {
            v
        }
    }

    pub fn int_value(&self) -> i32 {
        self.to_big_integer_bits() as i32
    }

    pub fn long_value(&self) -> i64 {
        self.to_big_integer_bits() as i64
    }

    pub fn double_value(&self) -> f64 {
        let s = format!("{}{}e{}", if self.neg { "-" } else { "" }, self.unscaled_abs_string(), -(self.scale as i64));
        s.parse::<f64>().unwrap_or(0.0)
    }

    /// `BigDecimal.hashCode()`: `31 * unscaledValue().hashCode() + scale`.
    pub fn hash_code(&self) -> i32 {
        let mut words: Vec<u32> = Vec::new();
        for &d in &self.mag {
            let mut carry = d as u64;
            for w in words.iter_mut() {
                let v = *w as u64 * 10 + carry;
                *w = v as u32;
                carry = v >> 32;
            }
            if carry != 0 {
                words.push(carry as u32);
            }
        }
        let mut h = 0i32;
        for w in words.iter().rev() {
            h = h.wrapping_mul(31).wrapping_add(*w as i32);
        }
        h = h.wrapping_mul(self.signum());
        h.wrapping_mul(31).wrapping_add(self.scale)
    }
}

impl std::fmt::Display for BigDecimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.layout(true))
    }
}
