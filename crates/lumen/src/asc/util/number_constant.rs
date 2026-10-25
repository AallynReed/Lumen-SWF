//! Port of `macromedia/asc/util/NumberConstant.java` and its subclasses `IntNumberConstant`, `UintNumberConstant`,
//! `DoubleNumberConstant` and `DecimalNumberConstant`.

use std::fmt;

use crate::asc::util::decimal128::Decimal128;
use crate::asc::util::number_usage::NumberUsage;
use crate::xfl::num::java_double;

#[derive(Clone, Debug)]
pub enum NumberConstant {
    Int(i32),
    /// Kept as a `long`, as Java does.
    Uint(i64),
    Double(f64),
    Decimal(Decimal128),
}

impl NumberConstant {
    pub const MAXUINT: f64 = (i32::MAX as f64) * 2.0 + 1.0;

    pub fn number_type(&self) -> u8 {
        (match self {
            NumberConstant::Int(_) => NumberUsage::use_int,
            NumberConstant::Uint(_) => NumberUsage::use_uint,
            NumberConstant::Double(_) => NumberUsage::use_double,
            NumberConstant::Decimal(_) => NumberUsage::use_decimal,
        }) as u8
    }

    pub fn int_value(&self) -> i32 {
        match self {
            NumberConstant::Int(v) => *v,
            NumberConstant::Uint(v) => *v as i32,
            NumberConstant::Double(v) => java_d2i(*v),
            NumberConstant::Decimal(v) => v.int_value().expect("ArithmeticException"),
        }
    }

    pub fn uint_value(&self) -> i64 {
        match self {
            NumberConstant::Int(v) => *v as i64,
            NumberConstant::Uint(v) => *v,
            NumberConstant::Double(v) => java_d2l(*v) as i32 as i64,
            NumberConstant::Decimal(v) => v.long_value().expect("ArithmeticException") as i32 as i64,
        }
    }

    pub fn double_value(&self) -> f64 {
        match self {
            NumberConstant::Int(v) => *v as f64,
            NumberConstant::Uint(v) => *v as f64,
            NumberConstant::Double(v) => *v,
            NumberConstant::Decimal(v) => v.double_value(),
        }
    }

    pub fn decimal_value(&self) -> Decimal128 {
        match self {
            NumberConstant::Int(v) => Decimal128::from_i32(*v),
            NumberConstant::Uint(v) => Decimal128::from_i64(*v),
            NumberConstant::Double(v) => Decimal128::from_f64(*v),
            NumberConstant::Decimal(v) => v.clone(),
        }
    }
}

impl fmt::Display for NumberConstant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NumberConstant::Int(v) => write!(f, "{}", v),
            NumberConstant::Uint(v) => write!(f, "{}", v),
            NumberConstant::Double(v) => f.write_str(&java_double(*v)),
            NumberConstant::Decimal(v) => write!(f, "{}", v),
        }
    }
}

/// Java's `(int)d`.
pub fn java_d2i(d: f64) -> i32 {
    if d.is_nan() {
        0
    } else {
        d as i32
    }
}

/// Java's `(long)d`.
pub fn java_d2l(d: f64) -> i64 {
    if d.is_nan() {
        0
    } else {
        d as i64
    }
}
