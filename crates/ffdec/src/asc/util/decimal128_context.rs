//! Port of `macromedia.asc.util.Decimal128Context`.
#![allow(non_upper_case_globals)]

use super::big_decimal::{MathContext, RoundingMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decimal128Context {
    bdctx: MathContext,
    pub status: i32,
}

impl Default for Decimal128Context {
    fn default() -> Self {
        Decimal128Context::new()
    }
}

impl Decimal128Context {
    pub fn new() -> Decimal128Context {
        Decimal128Context { bdctx: MathContext::new(34, RoundingMode::HalfEven), status: 0 }
    }

    pub fn with(precision: i32, rounding: RoundingMode) -> Decimal128Context {
        Decimal128Context { bdctx: MathContext::new(precision, rounding), status: 0 }
    }

    pub fn get_precision(&self) -> i32 {
        self.bdctx.precision
    }

    pub fn get_rounding_mode(&self) -> RoundingMode {
        self.bdctx.rounding_mode
    }

    pub(crate) fn math_ctx(&self) -> MathContext {
        self.bdctx
    }

    pub fn set_precision(&mut self, precision: i32) -> Result<(), String> {
        if !(1..=34).contains(&precision) {
            return Err("Precision must be between 1 and 34".to_string());
        }
        if self.bdctx.precision != precision {
            self.bdctx = MathContext::new(precision, self.bdctx.rounding_mode);
        }
        Ok(())
    }

    pub fn set_rounding_mode(&mut self, mode: RoundingMode) {
        if self.bdctx.rounding_mode != mode {
            self.bdctx = MathContext::new(self.bdctx.precision, mode);
        }
    }

    /// A fresh `Decimal128Context.DECIMAL128` (Java shares one static instance whose `status` nobody reads).
    pub fn decimal128() -> Decimal128Context {
        Decimal128Context::with(34, RoundingMode::HalfEven)
    }
}

pub const DEC_Conversion_syntax: i32 = 0x00000001;
pub const DEC_Division_by_zero: i32 = 0x00000002;
pub const DEC_Division_impossible: i32 = 0x00000004;
pub const DEC_Division_undefined: i32 = 0x00000008;
pub const DEC_Insufficient_storage: i32 = 0x00000010;
pub const DEC_Inexact: i32 = 0x00000020;
pub const DEC_Invalid_context: i32 = 0x00000040;
pub const DEC_Invalid_operation: i32 = 0x00000080;
pub const DEC_Overflow: i32 = 0x00000200;
pub const DEC_Clamped: i32 = 0x00000400;
pub const DEC_Rounded: i32 = 0x00000800;
pub const DEC_Subnormal: i32 = 0x00001000;
pub const DEC_Underflow: i32 = 0x00002000;
pub const DEC_sNaN: i32 = 0x40000000;

pub const DEC_Condition_CS: &str = "Conversion syntax";
pub const DEC_Condition_DZ: &str = "Division by zero";
pub const DEC_Condition_DI: &str = "Division impossible";
pub const DEC_Condition_DU: &str = "Division undefined";
pub const DEC_Condition_IE: &str = "Inexact";
pub const DEC_Condition_IS: &str = "Insufficient storage";
pub const DEC_Condition_IC: &str = "Invalid context";
pub const DEC_Condition_IO: &str = "Invalid operation";
pub const DEC_Condition_OV: &str = "Overflow";
pub const DEC_Condition_PA: &str = "Clamped";
pub const DEC_Condition_RO: &str = "Rounded";
pub const DEC_Condition_SU: &str = "Subnormal";
pub const DEC_Condition_UN: &str = "Underflow";
pub const DEC_Condition_ZE: &str = "No status";
pub const DEC_Condition_MU: &str = "Multiple status";
pub const DEC_Condition_Length: i32 = 21;
