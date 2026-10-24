//! Port of `macromedia.asc.util.Decimal128`.
#![allow(non_upper_case_globals)]

use super::big_decimal::{BigDecimal, MathContext, RoundingMode};
use super::decimal128_context::*;
use super::densely_packed_decimal::{bin2_dpd, dpd2_bin};

const DEC_NAN: u8 = 0x01;
const DEC_SNAN: u8 = 0x02;
const DEC_INF: u8 = 0x04;
const DEC_NEG: u8 = 0x08;

const DEC_SPECIAL: u8 = DEC_NAN | DEC_SNAN | DEC_INF;

pub const MAX_PRECISION: i32 = 34;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal128 {
    flags: u8,
    value: BigDecimal,
}

fn big_zero() -> BigDecimal {
    BigDecimal::zero_with_scale(0)
}

/// Which of Java's shared `ZERO`/`ONE`/`NEG1` instances (or a NaN) `compareTo` handed back.
enum Comparison {
    Zero,
    One,
    Neg1,
    NaN,
}

const MASK: [i32; 11] = [0x000, 0x001, 0x003, 0x007, 0x00F, 0x01F, 0x03F, 0x07F, 0x0FF, 0x1FF, 0x3FF];

struct IEEERep {
    data: [i32; 4],
    negative: bool,
    q_nan: bool,
    s_nan: bool,
    infinite: bool,
    finalized: bool,
    exponent: i32,
    top_digit: i16,
    current_datum: i32,
    current_shift: i32,
    total_digits: i32,
    current_bit: i32,
}

impl IEEERep {
    fn new() -> IEEERep {
        IEEERep {
            data: [0; 4],
            negative: false,
            q_nan: false,
            s_nan: false,
            infinite: false,
            finalized: false,
            exponent: 0,
            top_digit: 0,
            current_datum: 3,
            current_shift: 0,
            total_digits: 0,
            current_bit: 0,
        }
    }

    fn set_neg(&mut self) {
        self.negative = true;
    }

    fn set_nan(&mut self, signalling: bool) {
        if signalling {
            self.s_nan = true;
        } else {
            self.q_nan = true;
        }
    }

    fn set_infinite(&mut self) {
        self.infinite = true;
    }

    fn set_exponent(&mut self, exp: i32) {
        self.exponent = exp;
    }

    fn add_unit(&mut self, unit: i16) {
        if self.total_digits == 34 {
            panic!("trying to add more than 34 digits to IEEE128");
        }
        if self.total_digits == 33 {
            if unit > 9 {
                panic!("trying to add more than 34 digits to IEEE128");
            }
            self.top_digit = unit;
            self.total_digits += 1;
            return;
        }
        self.total_digits += 3;
        let dpd = bin2_dpd(unit) as i32;
        let cd = self.current_datum as usize;
        if self.current_shift <= 22 {
            self.data[cd] |= dpd.wrapping_shl(self.current_shift as u32);
            self.current_shift += 10;
        } else {
            let on_this = 32 - self.current_shift;
            if on_this != 0 {
                self.data[cd] |= dpd.wrapping_shl(self.current_shift as u32);
            }
            self.data[cd - 1] |= ((dpd as u32) >> on_this) as i32;
            self.current_datum -= 1;
            self.current_shift = 10 - on_this;
        }
    }

    fn get_rep(&mut self) -> [u8; 16] {
        if !self.finalized {
            let combo;
            if self.negative {
                self.data[0] |= 0x80000000u32 as i32;
            }
            if self.infinite {
                combo = 0x1E;
            } else if self.s_nan || self.q_nan {
                combo = 0x1F;
                if self.s_nan {
                    self.exponent = 0x800;
                } else {
                    self.exponent = 0;
                }
            } else if self.top_digit < 8 {
                combo = self.top_digit as i32 | (((self.exponent as u32) >> 9) as i32 & 0x18);
            } else {
                combo = 0x18 | (((((self.exponent as u32) >> 12) as i32) & 0x3) << 1) | (self.top_digit as i32 - 8);
            }
            self.data[0] |= combo.wrapping_shl(26) | ((self.exponent & 0xFFF) << 14);
            self.finalized = true;
        }
        let mut result = [0u8; 16];
        for (ndx, i) in (0..16).step_by(4).enumerate() {
            let datum = self.data[ndx] as u32;
            result[i] = (datum >> 24) as u8;
            result[i + 1] = ((datum >> 16) & 0xFF) as u8;
            result[i + 2] = ((datum >> 8) & 0xFF) as u8;
            result[i + 3] = (datum & 0xFF) as u8;
        }
        result
    }

    fn from_rep(rep: &[u8]) -> IEEERep {
        let mut r = IEEERep::new();
        for (ndx, i) in (0..16).step_by(4).enumerate() {
            r.data[ndx] = (((rep[i] as u32) << 24) + ((rep[i + 1] as u32) << 16) + ((rep[i + 2] as u32) << 8) + rep[i + 3] as u32) as i32;
        }
        if (r.data[0] as u32 & 0x80000000) != 0 {
            r.negative = true;
        }
        let combo = (r.data[0] >> 26) & 0x1F;
        if combo == 0x1E {
            r.infinite = true;
        } else if combo == 0x1F {
            if (r.data[0] & 0x02000000) != 0 {
                r.s_nan = true;
            } else {
                r.q_nan = true;
            }
        } else {
            let expmsb;
            if (combo & 0x18) == 0x18 {
                expmsb = (combo >> 1) & 0x3;
                r.top_digit = (8 + (combo & 0x01)) as i16;
            } else {
                expmsb = ((combo as u32) >> 3) as i32;
                r.top_digit = (combo & 0x7) as i16;
            }
            r.exponent = ((((r.data[0] as u32) >> 14) as i32) & 0xFFF) | (expmsb << 12);
        }
        r.current_datum = -1;
        r
    }

    fn get_unit(&mut self) -> i16 {
        let mut result: i16 = 0;
        if self.current_datum == -1 {
            self.current_datum = 0;
            self.current_bit = 14;
            return self.top_digit;
        } else if self.current_datum > 3 {
            panic!("trying to read more than 34 digits from IEEE128");
        } else if self.current_bit >= 10 {
            result = ((((self.data[self.current_datum as usize] as u32) >> (self.current_bit - 10)) as i32) & 0x3FF) as i16;
            self.current_bit -= 10;
        } else {
            let overlap = 10 - self.current_bit;
            if self.current_bit != 0 {
                result = ((self.data[self.current_datum as usize] & MASK[self.current_bit as usize]) << overlap) as i16;
            }
            self.current_datum += 1;
            result |= ((self.data[self.current_datum as usize] as u32) >> (32 - overlap)) as i16;
            self.current_bit = 32 - overlap;
        }
        dpd2_bin(result)
    }

    fn is_infinite(&self) -> bool {
        self.infinite
    }

    fn is_nan(&self) -> bool {
        self.q_nan | self.s_nan
    }

    fn is_snan(&self) -> bool {
        self.s_nan
    }

    fn is_negative(&self) -> bool {
        self.negative
    }

    fn get_exponent(&self) -> i32 {
        self.exponent
    }
}

const DECIMAL128_Pmax: i32 = 34;
const DECIMAL128_Emax: i32 = 6144;
const DECIMAL128_Emin: i32 = -6143;
const DECIMAL128_Bias: i32 = 6176;
const DECIMAL128_Ehigh: i32 = DECIMAL128_Emax + DECIMAL128_Bias - DECIMAL128_Pmax + 1;

fn unit_of(digits: &[u8], i: usize) -> i16 {
    let d = |k: usize| (digits[k] - b'0') as i16;
    match i {
        0 => d(0),
        1 => 10 * d(0) + d(1),
        _ => 100 * d(i - 2) + 10 * d(i - 1) + d(i),
    }
}

impl Decimal128 {
    fn raw(value: BigDecimal, flags: u8) -> Decimal128 {
        Decimal128 { flags, value }
    }

    fn blank() -> Decimal128 {
        Decimal128 { flags: 0, value: big_zero() }
    }

    pub fn zero() -> Decimal128 {
        Decimal128::raw(big_zero(), 0)
    }

    pub fn negzero() -> Decimal128 {
        Decimal128::raw(big_zero(), DEC_NEG)
    }

    pub fn one() -> Decimal128 {
        Decimal128::raw(BigDecimal::from_i64(1), 0)
    }

    pub fn neg1() -> Decimal128 {
        Decimal128::raw(BigDecimal::from_i64(-1), DEC_NEG)
    }

    pub fn nan() -> Decimal128 {
        Decimal128::raw(big_zero(), DEC_NAN)
    }

    pub fn infinity() -> Decimal128 {
        Decimal128::raw(big_zero(), DEC_INF)
    }

    pub fn neginfinity() -> Decimal128 {
        Decimal128::raw(big_zero(), DEC_INF | DEC_NEG)
    }

    pub fn from_str(s: &str) -> Result<Decimal128, String> {
        Decimal128::from_str_ctx(s, &Decimal128Context::decimal128())
    }

    pub fn from_str_ctx(s: &str, ctx: &Decimal128Context) -> Result<Decimal128, String> {
        let chars: Vec<char> = s.chars().collect();
        Decimal128::from_chars_ctx(&chars, ctx)
    }

    pub fn from_chars(input: &[char]) -> Result<Decimal128, String> {
        Decimal128::from_chars_ctx(input, &Decimal128Context::decimal128())
    }

    pub fn from_chars_ctx(input: &[char], ctx: &Decimal128Context) -> Result<Decimal128, String> {
        let mut d = Decimal128::blank();
        d.parse_decimal_number(input, ctx)?;
        Ok(d)
    }

    fn parse_decimal_number(&mut self, input: &[char], ctx: &Decimal128Context) -> Result<(), String> {
        let mut seen_s = false;
        self.value = big_zero();
        let mut number = false;
        let mut i = 0;
        while i < input.len() {
            let c = input[i];
            if c.is_ascii_digit() || c == '.' {
                if !seen_s {
                    number = true;
                }
                break;
            }
            if c == '+' {
                if seen_s {
                    break;
                }
                i += 1;
                continue;
            }
            if c == '-' {
                if seen_s {
                    break;
                }
                self.flags = DEC_NEG;
                i += 1;
                continue;
            }
            if c == 'S' || c == 's' {
                if seen_s {
                    break;
                }
                seen_s = true;
                i += 1;
                continue;
            }
            if c == 'N' {
                if (i + 2) < input.len() && input[i + 1] == 'a' && input[i + 2] == 'N' {
                    self.flags |= if seen_s { DEC_SNAN } else { DEC_NAN };
                    if (i + 3) < input.len() {
                        for &cj in &input[i + 3..] {
                            if !cj.is_ascii_digit() {
                                return Err("invalid NaN payload".to_string());
                            }
                        }
                        self.value = BigDecimal::parse_mc(&input[i + 3..], ctx.math_ctx())?;
                        if self.value.precision() > 33 {
                            self.value = big_zero();
                        }
                    }
                    return Ok(());
                } else {
                    break;
                }
            }
            if c == 'I' {
                if seen_s {
                    break;
                }
                let tail: String = input[i..].iter().collect();
                if tail == "Infinity" || tail == "Inf" {
                    self.flags |= DEC_INF;
                    return Ok(());
                }
                break;
            }
            i += 1;
        }
        if !number {
            return Err("invalid decimal literal".to_string());
        }
        self.value = BigDecimal::parse_mc(input, ctx.math_ctx())?;
        self.clamp_overflow(ctx);
        Ok(())
    }

    pub fn from_i32(val: i32) -> Decimal128 {
        let value = BigDecimal::from_i64_mc(val as i64, MathContext::DECIMAL128).expect("HALF_EVEN never fails");
        Decimal128::raw(value, if val >= 0 { 0 } else { DEC_NEG })
    }

    pub fn from_i32_ctx(val: i32, ctx: &Decimal128Context) -> Result<Decimal128, String> {
        let value = BigDecimal::from_i64_mc(val as i64, ctx.math_ctx())?;
        Ok(Decimal128::raw(value, if val >= 0 { 0 } else { DEC_NEG }))
    }

    pub fn from_f64(val: f64) -> Decimal128 {
        Decimal128::from_f64_mc(val, MathContext::DECIMAL128).expect("HALF_EVEN never fails")
    }

    pub fn from_f64_ctx(val: f64, ctx: &Decimal128Context) -> Result<Decimal128, String> {
        Decimal128::from_f64_mc(val, ctx.math_ctx())
    }

    fn from_f64_mc(val: f64, mc: MathContext) -> Result<Decimal128, String> {
        let mut flags = if val >= 0.0 { 0 } else { DEC_NEG };
        let mut value = big_zero();
        if val.is_nan() {
            flags |= DEC_NAN;
        } else if val.is_infinite() {
            flags |= DEC_INF;
        } else {
            value = BigDecimal::from_f64_mc(val, mc)?;
        }
        Ok(Decimal128::raw(value, flags))
    }

    pub fn from_i64(val: i64) -> Decimal128 {
        let value = BigDecimal::from_i64_mc(val, MathContext::DECIMAL128).expect("HALF_EVEN never fails");
        Decimal128::raw(value, if val >= 0 { 0 } else { DEC_NEG })
    }

    pub fn from_i64_ctx(val: i64, ctx: &Decimal128Context) -> Result<Decimal128, String> {
        let value = BigDecimal::from_i64_mc(val, ctx.math_ctx())?;
        Ok(Decimal128::raw(value, if val >= 0 { 0 } else { DEC_NEG }))
    }

    pub fn to_ieee_ctx(&self, ctx: &mut Decimal128Context) -> [u8; 16] {
        let mut ieee = IEEERep::new();
        if (self.flags & DEC_NEG) != 0 {
            ieee.set_neg();
        }
        'body: {
            if (self.flags & DEC_SPECIAL) == 0 {
                let mut coefficient: Vec<u8> = self.value.abs().unscaled_abs_string().into_bytes();
                let mut exponent = -self.value.scale();
                let mut digits = coefficient.len() as i32;
                let ae = exponent + digits - 1;
                if digits > DECIMAL128_Pmax || ae > DECIMAL128_Emax || ae < DECIMAL128_Emin {
                    let n2 = self.add_ctx(&Decimal128::zero(), &mut Decimal128Context::decimal128());
                    if (n2.flags & DEC_INF) != 0 {
                        ieee.set_infinite();
                        break 'body;
                    }
                    coefficient = n2.value.abs().unscaled_abs_string().into_bytes();
                    exponent = -n2.value.scale();
                    digits = coefficient.len() as i32;
                }
                if self.value.compare_to(&big_zero()) == 0 {
                    let exp;
                    if exponent < -DECIMAL128_Bias {
                        exp = 0;
                        ctx.status |= DEC_Clamped;
                    } else {
                        let mut e = exponent + DECIMAL128_Bias;
                        if e > DECIMAL128_Ehigh {
                            e = DECIMAL128_Ehigh;
                            ctx.status |= DEC_Clamped;
                        }
                        exp = e;
                    }
                    ieee.set_exponent(exp);
                } else {
                    let mut exp = exponent + DECIMAL128_Bias;
                    if exp > DECIMAL128_Ehigh {
                        let shift = exp - DECIMAL128_Ehigh;
                        exp = DECIMAL128_Ehigh;
                        ctx.status |= DEC_Clamped;
                        coefficient.extend(std::iter::repeat(b'0').take(shift as usize));
                        digits += shift;
                    }
                    ieee.set_exponent(exp);
                    let mut i = digits - 1;
                    while i >= 0 {
                        ieee.add_unit(unit_of(&coefficient, i as usize));
                        i -= 3;
                    }
                }
            } else if (self.flags & DEC_INF) != 0 {
                ieee.set_infinite();
            } else {
                ieee.set_nan((self.flags & DEC_SNAN) != 0);
                if self.value.signum() != 0 {
                    let payload = self.value.abs().unscaled_abs_string().into_bytes();
                    let digits = payload.len() as i32;
                    ieee.set_exponent(0);
                    let mut i = digits - 1;
                    while i >= 0 {
                        ieee.add_unit(unit_of(&payload, i as usize));
                        i -= 3;
                    }
                }
            }
        }
        ieee.get_rep()
    }

    pub fn to_ieee(&self) -> [u8; 16] {
        self.to_ieee_ctx(&mut Decimal128Context::decimal128())
    }

    /// `toByteList(result)`: the list's contents are replaced by the 16 IEEE bytes.
    pub fn to_byte_list(&self) -> Vec<u8> {
        self.to_ieee().to_vec()
    }

    pub fn from_ieee(rep: &[u8]) -> Result<Decimal128, String> {
        if rep.len() != 16 {
            return Err("Decimal128 needs 16 bytes for representation".to_string());
        }
        let mut ieee = IEEERep::from_rep(rep);
        let mut flags = if ieee.is_negative() { DEC_NEG } else { 0 };
        if ieee.is_nan() {
            if ieee.is_snan() {
                flags |= DEC_SNAN;
            } else {
                flags |= DEC_NAN;
            }
        } else if ieee.is_infinite() {
            return Ok(Decimal128::raw(big_zero(), flags | DEC_INF));
        }
        let mut coefficient: Vec<u8> = Vec::new();
        let mut leading_zeroes = true;
        for _ in 0..12 {
            let unit = ieee.get_unit();
            if !leading_zeroes || unit != 0 {
                if leading_zeroes {
                    leading_zeroes = false;
                    coefficient = (unit as i32).to_string().into_bytes().iter().map(|b| b - b'0').collect();
                } else {
                    coefficient.extend(format!("{:03}", unit).bytes().map(|b| b - b'0'));
                }
            }
        }
        let scale = if ieee.is_nan() { 0 } else { -(ieee.get_exponent() - DECIMAL128_Bias) };
        let mut value = BigDecimal::from_digits(false, &coefficient, scale);
        if (flags & (DEC_NEG | DEC_NAN | DEC_SNAN)) == DEC_NEG {
            value = value.negate_mc(MathContext::DECIMAL128).expect("HALF_EVEN never fails");
        }
        Ok(Decimal128::raw(value, flags))
    }

    pub fn int_value(&self) -> Result<i32, String> {
        if (self.flags & DEC_SPECIAL) != 0 {
            return Err("no valid integer value".to_string());
        }
        Ok(self.value.int_value())
    }

    pub fn long_value(&self) -> Result<i64, String> {
        if (self.flags & DEC_SPECIAL) != 0 {
            return Err("no valid long value".to_string());
        }
        Ok(self.value.long_value())
    }

    pub fn double_value(&self) -> f64 {
        if (self.flags & (DEC_NAN | DEC_SNAN)) != 0 {
            return f64::NAN;
        }
        if (self.flags & DEC_INF) != 0 {
            return if (self.flags & DEC_NEG) != 0 { f64::NEG_INFINITY } else { f64::INFINITY };
        }
        self.value.double_value()
    }

    pub fn to_engineering_string(&self) -> String {
        if (self.flags & (DEC_NAN | DEC_SNAN)) != 0 {
            "NaN".to_string()
        } else if (self.flags & DEC_INF) != 0 {
            "Infinity".to_string()
        } else {
            self.value.to_engineering_string()
        }
    }

    pub fn is_nan(&self) -> bool {
        (self.flags & (DEC_NAN | DEC_SNAN)) != 0
    }

    pub fn is_qnan(&self) -> bool {
        (self.flags & DEC_NAN) != 0
    }

    pub fn is_snan(&self) -> bool {
        (self.flags & DEC_SNAN) != 0
    }

    pub fn is_finite(&self) -> bool {
        (self.flags & DEC_INF) == 0
    }

    pub fn is_negative(&self) -> bool {
        (self.flags & DEC_NEG) != 0
    }

    fn op_nans(&self, operand: Option<&Decimal128>, ctx: &mut Decimal128Context) -> Decimal128 {
        let mut lhs = self;
        if (self.flags & DEC_SNAN) != 0 {
            ctx.status |= DEC_Invalid_operation | DEC_sNaN;
        } else if let Some(op) = operand.filter(|o| (o.flags & DEC_SNAN) != 0) {
            lhs = op;
            ctx.status |= DEC_Invalid_operation | DEC_sNaN;
        } else if (self.flags & DEC_NAN) == 0 {
            lhs = operand.expect("operand is the NaN");
        }
        let mut result = Decimal128::raw(lhs.value.clone(), lhs.flags);
        result.flags &= !DEC_SNAN;
        result.flags |= DEC_NAN;
        result
    }

    fn clamp_overflow(&mut self, ctx: &Decimal128Context) {
        if (self.flags & DEC_SPECIAL) != 0 {
            return;
        }
        let min_scale = -(DECIMAL128_Emax - DECIMAL128_Pmax + 1);
        let max_scale = DECIMAL128_Bias;
        let scale = self.value.scale();
        let result_val;
        if scale > max_scale {
            let num_precision = self.value.precision();
            let delta = scale - max_scale;
            if (num_precision - delta) >= 1 {
                result_val = self.value.set_scale(max_scale, ctx.get_rounding_mode()).expect("rounding mode allows inexact results");
            } else {
                self.value = BigDecimal::zero_with_scale(max_scale);
                return;
            }
        } else if scale < min_scale {
            let num_precision = self.value.precision();
            let delta = min_scale - scale;
            if (num_precision + delta) <= ctx.get_precision() || self.value.compare_to(&big_zero()) == 0 {
                result_val = self.value.set_scale(min_scale, ctx.get_rounding_mode()).expect("scale increase is exact");
            } else {
                let mode = ctx.get_rounding_mode();
                if (self.flags & DEC_NEG) != 0 {
                    if !(mode == RoundingMode::Ceiling || mode == RoundingMode::Down) {
                        self.flags = DEC_NEG | DEC_INF;
                        self.value = big_zero();
                        return;
                    }
                } else if !(mode == RoundingMode::Floor || mode == RoundingMode::Down) {
                    self.flags = DEC_INF;
                    self.value = big_zero();
                    return;
                }
                let nines: Vec<u8> = vec![9; ctx.get_precision().max(1) as usize];
                let mut rv = BigDecimal::from_digits(false, &nines, min_scale);
                if (self.flags & DEC_NEG) != 0 {
                    rv = rv.negate_mc(ctx.math_ctx()).expect("value already fits the precision");
                }
                result_val = rv;
            }
        } else {
            return;
        }
        self.value = result_val;
    }

    pub fn abs(&self) -> Decimal128 {
        self.abs_ctx(&Decimal128Context::decimal128())
    }

    pub fn abs_ctx(&self, ctx: &Decimal128Context) -> Decimal128 {
        let mut result = Decimal128::blank();
        result.flags = self.flags;
        result.flags &= !DEC_NEG;
        result.value = self.value.abs_mc(ctx.math_ctx()).expect("rounding mode allows inexact results");
        result
    }

    pub fn add(&self, addend: &Decimal128) -> Decimal128 {
        self.add_ctx(addend, &mut Decimal128Context::decimal128())
    }

    pub fn add_ctx(&self, addend: &Decimal128, ctx: &mut Decimal128Context) -> Decimal128 {
        let specialbits = (addend.flags | self.flags) & DEC_SPECIAL;
        if specialbits != 0 {
            if (specialbits & (DEC_NAN | DEC_SNAN)) != 0 {
                return self.op_nans(Some(addend), ctx);
            }
            let mut result = Decimal128::blank();
            if (self.flags & DEC_INF) != 0 {
                if (addend.flags & DEC_INF) != 0 && ((self.flags ^ addend.flags) & DEC_NEG) != 0 {
                    ctx.status |= DEC_Invalid_operation;
                    result.flags = DEC_NAN;
                    return result;
                }
                result.flags = self.flags;
            } else {
                result.flags = addend.flags;
            }
            return result;
        }
        let mut result = Decimal128::blank();
        result.value = self.value.add_mc(&addend.value, ctx.math_ctx()).expect("rounding mode allows inexact results");
        if result.value.compare_to(&big_zero()) < 0 {
            result.flags |= DEC_NEG;
        }
        result.clamp_overflow(ctx);
        result
    }

    fn compare(&self, val: &Decimal128, ctx: &mut Decimal128Context) -> Comparison {
        if ((self.flags | val.flags) & (DEC_NAN | DEC_SNAN)) != 0 {
            self.op_nans(Some(val), ctx);
            return Comparison::NaN;
        }
        if (self.flags & DEC_INF) != 0 {
            if (self.flags & DEC_NEG) != 0 {
                if (val.flags & (DEC_INF | DEC_NEG)) == (DEC_INF | DEC_NEG) {
                    ctx.status |= DEC_Invalid_operation;
                    return Comparison::NaN;
                }
                Comparison::Neg1
            } else {
                if (val.flags & (DEC_INF | DEC_NEG)) == DEC_INF {
                    ctx.status |= DEC_Invalid_operation;
                    return Comparison::NaN;
                }
                Comparison::One
            }
        } else if (val.flags & DEC_INF) != 0 {
            if (val.flags & DEC_NEG) != 0 {
                Comparison::One
            } else {
                Comparison::Neg1
            }
        } else {
            match self.value.compare_to(&val.value) {
                1 => Comparison::One,
                -1 => Comparison::Neg1,
                _ => Comparison::Zero,
            }
        }
    }

    pub fn equals(&self, num: &Decimal128) -> bool {
        matches!(self.compare(num, &mut Decimal128Context::decimal128()), Comparison::Zero)
    }

    pub fn greater_than(&self, num: &Decimal128) -> bool {
        matches!(self.compare(num, &mut Decimal128Context::decimal128()), Comparison::One)
    }

    pub fn greater_than_or_equal(&self, num: &Decimal128) -> bool {
        matches!(self.compare(num, &mut Decimal128Context::decimal128()), Comparison::One | Comparison::Zero)
    }

    pub fn less_than(&self, num: &Decimal128) -> bool {
        matches!(self.compare(num, &mut Decimal128Context::decimal128()), Comparison::Neg1)
    }

    pub fn less_than_or_equal(&self, num: &Decimal128) -> bool {
        matches!(self.compare(num, &mut Decimal128Context::decimal128()), Comparison::Neg1 | Comparison::Zero)
    }

    pub fn divide(&self, divisor: &Decimal128) -> Decimal128 {
        self.divide_ctx(divisor, &mut Decimal128Context::decimal128())
    }

    pub fn divide_ctx(&self, divisor: &Decimal128, ctx: &mut Decimal128Context) -> Decimal128 {
        let differentsigns = ((self.flags ^ divisor.flags) & DEC_NEG) != 0;
        let specialbits = (divisor.flags | self.flags) & DEC_SPECIAL;
        if specialbits != 0 {
            if (specialbits & (DEC_NAN | DEC_SNAN)) != 0 {
                return self.op_nans(Some(divisor), ctx);
            }
            let mut result = Decimal128::blank();
            if (self.flags & DEC_INF) != 0 {
                if (divisor.flags & DEC_INF) != 0 {
                    ctx.status |= DEC_Invalid_operation;
                    result.flags = DEC_NAN;
                } else {
                    result.flags = DEC_INF;
                    if differentsigns {
                        result.flags |= DEC_NEG;
                    }
                }
            } else {
                result.value = BigDecimal::zero_with_scale(DECIMAL128_Bias);
                if differentsigns {
                    result.flags = DEC_NEG;
                }
            }
            return result;
        }
        let mut result = Decimal128::blank();
        if divisor.value.compare_to(&big_zero()) == 0 {
            if self.value.compare_to(&big_zero()) == 0 {
                ctx.status |= DEC_Invalid_operation;
                result.flags = DEC_NAN;
            } else {
                result.value = big_zero();
                result.flags |= DEC_INF;
            }
        } else {
            result.value = self.value.divide_mc(&divisor.value, ctx.math_ctx()).expect("rounding mode allows inexact results");
        }
        if differentsigns {
            result.flags |= DEC_NEG;
        }
        result.clamp_overflow(ctx);
        result
    }

    pub fn multiply(&self, multiplicand: &Decimal128) -> Decimal128 {
        self.multiply_ctx(multiplicand, &mut Decimal128Context::decimal128())
    }

    pub fn multiply_ctx(&self, multiplicand: &Decimal128, ctx: &mut Decimal128Context) -> Decimal128 {
        let differentsigns = ((self.flags ^ multiplicand.flags) & DEC_NEG) != 0;
        let specialbits = (multiplicand.flags | self.flags) & DEC_SPECIAL;
        if specialbits != 0 {
            if (specialbits & (DEC_NAN | DEC_SNAN)) != 0 {
                return self.op_nans(Some(multiplicand), ctx);
            }
            let mut result = Decimal128::blank();
            if (self.flags & DEC_INF) != 0 {
                if (multiplicand.flags & DEC_INF) == 0 && multiplicand.value.compare_to(&big_zero()) == 0 {
                    ctx.status |= DEC_Invalid_operation;
                    result.flags = DEC_NAN;
                    return result;
                }
            } else if self.value.compare_to(&big_zero()) == 0 {
                ctx.status |= DEC_Invalid_operation;
                result.flags = DEC_NAN;
                return result;
            }
            result.flags = DEC_INF;
            if differentsigns {
                result.flags |= DEC_NEG;
            }
            return result;
        }
        let mut result = Decimal128::blank();
        result.value = self.value.multiply_mc(&multiplicand.value, ctx.math_ctx()).expect("rounding mode allows inexact results");
        if differentsigns {
            result.flags |= DEC_NEG;
        }
        result.clamp_overflow(ctx);
        result
    }

    pub fn remainder(&self, divisor: &Decimal128) -> Decimal128 {
        self.remainder_ctx(divisor, &mut Decimal128Context::decimal128())
    }

    pub fn remainder_ctx(&self, divisor: &Decimal128, ctx: &mut Decimal128Context) -> Decimal128 {
        let differentsigns = ((self.flags ^ divisor.flags) & DEC_NEG) != 0;
        let specialbits = (divisor.flags | self.flags) & DEC_SPECIAL;
        if specialbits != 0 {
            if (specialbits & (DEC_NAN | DEC_SNAN)) != 0 {
                return self.op_nans(Some(divisor), ctx);
            }
            let mut result = Decimal128::blank();
            if (self.flags & DEC_INF) != 0 || ((divisor.flags & DEC_INF) == 0 && divisor.value.compare_to(&big_zero()) == 0) {
                ctx.status |= DEC_Invalid_operation;
                result.flags = DEC_NAN;
            } else {
                result.value = self.value.clone();
                if differentsigns {
                    result.flags = DEC_NEG;
                }
            }
            return result;
        }
        let mut result = Decimal128::blank();
        if divisor.value.compare_to(&big_zero()) == 0 {
            result.flags = DEC_NAN;
        } else if self.value.compare_to(&big_zero()) == 0 {
            result.value = self.value.clone();
        } else {
            let computed = if ctx.math_ctx().precision == 34 {
                self.value.remainder_mc(&divisor.value, ctx.math_ctx())
            } else {
                self.value
                    .remainder_mc(&divisor.value, Decimal128Context::decimal128().math_ctx())
                    .and_then(|v| v.add_mc(&big_zero(), ctx.math_ctx()))
            };
            match computed {
                Ok(v) => result.value = v,
                Err(_) => result.flags = DEC_NAN,
            }
        }
        if differentsigns {
            result.flags |= DEC_NEG;
        }
        result.clamp_overflow(ctx);
        result
    }

    pub fn subtract(&self, subtrahend: &Decimal128) -> Decimal128 {
        self.subtract_ctx(subtrahend, &mut Decimal128Context::decimal128())
    }

    pub fn subtract_ctx(&self, subtrahend: &Decimal128, ctx: &mut Decimal128Context) -> Decimal128 {
        let specialbits = (subtrahend.flags | self.flags) & DEC_SPECIAL;
        if specialbits != 0 {
            if (specialbits & (DEC_NAN | DEC_SNAN)) != 0 {
                return self.op_nans(Some(subtrahend), ctx);
            }
            let mut result = Decimal128::blank();
            if (self.flags & DEC_INF) != 0 {
                if (subtrahend.flags & DEC_INF) != 0 && ((self.flags ^ subtrahend.flags) & DEC_NEG) == 0 {
                    ctx.status |= DEC_Invalid_operation;
                    result.flags = DEC_NAN;
                    return result;
                }
                result.flags = self.flags;
            } else {
                result.flags = subtrahend.flags;
                result.flags ^= DEC_NEG;
            }
            return result;
        }
        let mut result = Decimal128::blank();
        result.value = self.value.subtract_mc(&subtrahend.value, ctx.math_ctx()).expect("rounding mode allows inexact results");
        if result.value.compare_to(&big_zero()) < 0 {
            result.flags |= DEC_NEG;
        }
        result.clamp_overflow(ctx);
        result
    }

    pub fn hash_code(&self) -> i32 {
        self.value.hash_code() ^ self.flags as i32
    }
}

impl std::fmt::Display for Decimal128 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = String::new();
        if (self.flags & DEC_SPECIAL) != 0 && (self.flags & DEC_NEG) != 0 {
            result.push('-');
        }
        if (self.flags & (DEC_NAN | DEC_SNAN)) != 0 {
            if (self.flags & DEC_SNAN) != 0 {
                result.push('S');
            }
            result.push_str("NaN");
            if self.value.compare_to(&big_zero()) != 0 {
                result.push_str(&self.value.to_string());
            }
        } else if (self.flags & DEC_INF) != 0 {
            result.push_str("Infinity");
        } else {
            if (self.flags & DEC_NEG) != 0 && self.value.compare_to(&big_zero()) == 0 {
                result.push('-');
            }
            result.push_str(&self.value.to_string());
        }
        f.write_str(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    fn describe(d: &Decimal128) -> String {
        let iv = d.int_value().map_or("EX".to_string(), |v| v.to_string());
        let lv = d.long_value().map_or("EX".to_string(), |v| v.to_string());
        let rep = d.to_ieee();
        let round = Decimal128::from_ieee(&rep).unwrap().to_string();
        format!(
            "{}|{}|{}|{}|{:x}|{}|{}|{}",
            d,
            hex(&rep),
            round,
            d.to_engineering_string(),
            d.double_value().to_bits(),
            iv,
            lv,
            d.hash_code()
        )
    }

    fn parse(s: &str) -> Result<Decimal128, String> {
        match &s[..1] {
            "d" => Ok(Decimal128::from_f64(f64::from_bits(u64::from_str_radix(&s[1..], 16).unwrap()))),
            "i" => Ok(Decimal128::from_i32(s[1..].parse().unwrap())),
            "l" => Ok(Decimal128::from_i64(s[1..].parse().unwrap())),
            _ => Decimal128::from_str(&s[1..]),
        }
    }

    fn mode(s: &str) -> RoundingMode {
        match s {
            "UP" => RoundingMode::Up,
            "DOWN" => RoundingMode::Down,
            "CEILING" => RoundingMode::Ceiling,
            "FLOOR" => RoundingMode::Floor,
            "HALF_UP" => RoundingMode::HalfUp,
            "HALF_DOWN" => RoundingMode::HalfDown,
            "HALF_EVEN" => RoundingMode::HalfEven,
            _ => RoundingMode::Unnecessary,
        }
    }

    fn run(spec: &str) -> String {
        let p: Vec<&str> = spec.split(' ').collect();
        let result = (|| -> Result<String, String> {
            if p.len() == 1 {
                return Ok(describe(&parse(p[0])?));
            }
            let a = parse(p[0])?;
            let b = parse(p[2])?;
            let mut ctx = Decimal128Context::with(p[3].parse().unwrap(), mode(p[4]));
            let r = match p[1] {
                "add" => a.add_ctx(&b, &mut ctx),
                "sub" => a.subtract_ctx(&b, &mut ctx),
                "mul" => a.multiply_ctx(&b, &mut ctx),
                "div" => a.divide_ctx(&b, &mut ctx),
                _ => a.remainder_ctx(&b, &mut ctx),
            };
            Ok(format!("{}|{},{},{}", describe(&r), a.equals(&b), a.less_than(&b), a.greater_than_or_equal(&b)))
        })();
        result.unwrap_or_else(|_| "THROW".to_string())
    }

    fn check(lines: &str) {
        let mut bad = Vec::new();
        for line in lines.lines().filter(|l| !l.is_empty()) {
            let (spec, want) = line.split_once('\t').unwrap();
            let got = run(spec);
            let ok = if want.starts_with("THROW") { got == "THROW" } else { got == want };
            if !ok {
                bad.push(format!("{spec}\n  java: {want}\n  rust: {got}"));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    /// Results of the same operations from the real ASC jar (`macromedia.asc.util.Decimal128`).
    const JAVA: &str = r"s-0	-0|a2080000000000000000000000000000|-0|0|0|0|0|8
s1.5	1.5|2207c000000000000000000000000015|1.5|1.5|3ff8000000000000|1|1|466
s123456789012345678901234567890123456789	1.234567890123456789012345678901235E+38|2609534b9c1e28e56f3c127177823535|1.234567890123456789012345678901235E+38|123.4567890123456789012345678901235E+36|47d7383a69580580|-1371919904|-6134004772338259488|1437866148
s1e-7	1E-7|22064000000000000000000000000001|1E-7|100E-9|3e7ad7f29abcaf48|0|0|38
s0.000	0.000|22074000000000000000000000000000|0.000|0.000|0|0|0|3
s1E+3	1E+3|2208c000000000000000000000000001|1E+3|1E+3|408f400000000000|1000|1000|28
s1E6145	Infinity|78000000000000000000000000000000|Infinity|Infinity|7ff0000000000000|EX|EX|4
s1E-6200	0E-6176|00000000000000000000000000000000|0E-6176|0.00E-6174|0|0|0|6176
s9.999999999999999999999999999999999E+6144	9.999999999999999999999999999999999E+6144|77ffcff3fcff3fcff3fcff3fcff3fcff|9.999999999999999999999999999999999E+6144|9.999999999999999999999999999999999E+6144|7ff0000000000000|0|0|302910062
s0E+6200	0E+6111|43ffc000000000000000000000000000|0E+6111|0E+6111|0|0|0|-6111
sNaN123	NaN123|7c0000000000000000000000000000a3|NaN123|NaN|7ff8000000000000|EX|EX|3812
sSNaN7	SNaN7|7e000000000000000000000000000007|SNaN7|NaN|7ff8000000000000|EX|EX|219
s-Infinity	-Infinity|f8000000000000000000000000000000|-Infinity|Infinity|fff0000000000000|EX|EX|12
s2147483648	2147483648|2208000000000000000000008c78af48|2147483648|2147483648|41e0000000000000|-2147483648|2147483648|-2147483648
s-9223372036854775809	-9223372036854775809|a208000000000000948df20da5cfd42f|-9223372036854775809|-9223372036854775809|c3e0000000000000|-1|9223372036854775807|2147483625
s1x	THROW NumberFormatException
d3fb999999999999a	0.1000000000000000055511151231257827|25ff8000000000000156913453155f2d|0.1000000000000000055511151231257827|0.1000000000000000055511151231257827|3fb999999999999a|0|0|-2129539748
d0000000000000001	4.940656458412465441765687928682214E-324|31aecccd5a5884a65907e5dadaeca914|4.940656458412465441765687928682214E-324|4.940656458412465441765687928682214E-324|1|0|0|-1649969694
d7ff8000000000000	-NaN|fc000000000000000000000000000000|-NaN|NaN|7ff8000000000000|EX|EX|9
i-2147483648	-2147483648|a208000000000000000000008c78af48|-2147483648|-2147483648|c1e0000000000000|-2147483648|-2147483648|-2147483640
i2 div i3 34 HALF_EVEN	0.6666666666666666666666666666666667|39ffb66d9b66d9b66d9b66d9b66d9b67|0.6666666666666666666666666666666667|0.6666666666666666666666666666666667|3fe5555555555555|0|0|202232160|false,true,false
s100 div s0.25 34 HALF_EVEN	4E+2|22088000000000000000000000000004|4E+2|400|4079000000000000|400|400|122|false,false,true
i1 div sInfinity 34 HALF_EVEN	0E-6176|00000000000000000000000000000000|0E-6176|0.00E-6174|0|0|0|6176|false,true,false
i-2 div i3 5 FLOOR	-0.66667|a206c000000000000000000000019b67|-0.66667|-0.66667|bfe5555c52e72da1|0|0|-2066664|false,true,false
s9.99 div i1 2 HALF_UP	10|22080000000000000000000000000010|10|10|4024000000000000|10|10|310|false,false,true
s7.50 rem s0.2 34 HALF_EVEN	0.10|22078000000000000000000000000010|0.10|0.10|3fb999999999999a|0|0|312|false,false,true
s1E+40 rem i7 34 HALF_EVEN	NaN|7c000000000000000000000000000000|NaN|NaN|7ff8000000000000|EX|EX|1|false,false,true
i7 rem s0.33 5 HALF_EVEN	0.07|22078000000000000000000000000007|0.07|0.07|3fb1eb851eb851ec|0|0|219|false,false,true
s0.000 add s1.20 34 HALF_EVEN	1.200|22074000000000000000000000000500|1.200|1.200|3ff3333333333333|1|1|37203|false,true,false
s1.2E+5 add s0.000 3 HALF_EVEN	1.20E+5|2208c0000000000000000000000000a0|1.20E+5|120E+3|40fd4c0000000000|120000|120000|3717|false,false,true
i99 add i1 2 HALF_EVEN	1.0E+2|22084000000000000000000000000010|1.0E+2|100|4059000000000000|100|100|309|false,false,true
i1 sub s1E-40 34 HALF_EVEN	1.000000000000000000000000000000000|25ffc000000000000000000000000000|1.000000000000000000000000000000000|1.000000000000000000000000000000000|3ff0000000000000|1|1|1747626245|false,false,true
s1E+6000 mul s1E+6000 34 FLOOR	9.999999999999999999999999999999999E+6144|77ffcff3fcff3fcff3fcff3fcff3fcff|9.999999999999999999999999999999999E+6144|9.999999999999999999999999999999999E+6144|7ff0000000000000|0|0|302910062|true,false,true
s0.0 mul i-3 34 HALF_EVEN	-0.0|a207c000000000000000000000000000|-0.0|0.0|0|0|0|9|false,false,true
";

    #[test]
    fn matches_java() {
        check(JAVA);
    }

    /// Compares against `DecDrv` output from the real ASC jar, given as `ASC_DEC_JAVA`.
    #[test]
    #[ignore]
    fn matches_java_output_file() {
        let Ok(path) = std::env::var("ASC_DEC_JAVA") else {
            eprintln!("skipped: set ASC_DEC_JAVA");
            return;
        };
        check(&std::fs::read_to_string(path).unwrap());
    }
}
