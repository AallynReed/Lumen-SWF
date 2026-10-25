//! Port of `macromedia/asc/util/NumberUsage.java`. A shared handle, since Java nodes and values share one instance.

#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NumberUsageData {
    usage: i32,
    rounding: i32,
    precision: i32,
    floating_usage: i32,
}

#[derive(Clone, Debug)]
pub struct NumberUsage(Rc<RefCell<NumberUsageData>>);

/// `java.math.RoundingMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

impl NumberUsage {
    pub const use_Number: i32 = 0;
    pub const use_decimal: i32 = 1;
    pub const use_double: i32 = 2;
    pub const use_int: i32 = 3;
    pub const use_uint: i32 = 4;

    pub const round_CEILING: i32 = 0;
    pub const round_UP: i32 = 1;
    pub const round_HALF_UP: i32 = 2;
    pub const round_HALF_EVEN: i32 = 3;
    pub const round_HALF_DOWN: i32 = 4;
    pub const round_DOWN: i32 = 5;
    pub const round_FLOOR: i32 = 6;

    /// `defaultparam`, `(new NumberUsage()).encode()`.
    pub const defaultparam: i32 = Self::use_Number | (Self::round_HALF_EVEN << 3);

    pub const roundingModeName: [&'static str; 7] = ["CEILING", "UP", "HALF_UP", "HALF_EVEN", "HALF_DOWN", "DOWN", "FLOOR"];

    pub fn from_dec_number_rounding(&self, ndx: i32) -> Option<RoundingMode> {
        match ndx {
            0 => Some(RoundingMode::Ceiling),
            1 => Some(RoundingMode::Up),
            2 => Some(RoundingMode::HalfUp),
            3 => Some(RoundingMode::HalfEven),
            4 => Some(RoundingMode::HalfDown),
            5 => Some(RoundingMode::Down),
            6 => Some(RoundingMode::Floor),
            _ => None,
        }
    }

    pub fn new() -> NumberUsage {
        NumberUsage(Rc::new(RefCell::new(NumberUsageData {
            usage: Self::use_Number,
            rounding: Self::round_HALF_EVEN,
            precision: 34,
            floating_usage: Self::use_Number,
        })))
    }

    /// `new NumberUsage(nu)`.
    pub fn copy_of(nu: &NumberUsage) -> NumberUsage {
        NumberUsage(Rc::new(RefCell::new(*nu.0.borrow())))
    }

    pub fn ptr_eq(&self, other: &NumberUsage) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn set_usage(&self, u: i32) {
        debug_assert!(u <= 4);
        let mut d = self.0.borrow_mut();
        d.usage = u;
        if u <= Self::use_double {
            d.floating_usage = u;
        }
    }

    pub fn set_rounding(&self, r: i32) {
        debug_assert!(r <= 6);
        self.0.borrow_mut().rounding = r;
    }

    pub fn set_precision(&self, p: i32) {
        debug_assert!(p <= 34);
        self.0.borrow_mut().precision = p;
    }

    pub fn get_usage(&self) -> i32 {
        self.0.borrow().usage
    }

    pub fn get_floating_usage(&self) -> i32 {
        self.0.borrow().floating_usage
    }

    pub fn get_rounding(&self) -> i32 {
        self.0.borrow().rounding
    }

    pub fn get_java_rounding_mode(&self) -> Option<RoundingMode> {
        self.from_dec_number_rounding(self.get_rounding())
    }

    pub fn get_precision(&self) -> i32 {
        self.0.borrow().precision
    }

    pub fn is_default(&self) -> bool {
        let d = self.0.borrow();
        d.usage == Self::use_Number && d.rounding == Self::round_HALF_EVEN && d.precision == 34
    }

    pub fn encode(&self) -> i32 {
        let d = self.0.borrow();
        let mut ret = d.usage;
        if d.usage <= Self::use_decimal {
            ret |= d.rounding << 3;
            if d.precision < 34 {
                ret |= d.precision << 6;
            }
        }
        ret
    }
}

impl Default for NumberUsage {
    fn default() -> Self {
        Self::new()
    }
}
