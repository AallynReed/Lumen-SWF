//! Port of `org.apache.flex.forks.batik.css.parser.CSSLexicalUnit` (Flex fork, with line numbers and
//! `appendToStringBuffer`) and its subclasses.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::asc::flex2::batik::css::sac::lexical_unit::*;

pub type LexicalUnitRef = Rc<CSSLexicalUnit>;

#[derive(Debug)]
pub enum Kind {
    Simple,
    Integer(i32),
    Float(f32),
    Dimension(f32, String),
    Function(String, Option<LexicalUnitRef>),
    PredefinedFunction(Option<LexicalUnitRef>),
    String(String),
}

#[derive(Debug)]
pub struct CSSLexicalUnit {
    lexical_unit_type: i16,
    next_lexical_unit: RefCell<Option<LexicalUnitRef>>,
    previous_lexical_unit: RefCell<Weak<CSSLexicalUnit>>,
    line_number: i32,
    pub kind: Kind,
}

pub const UNIT_TEXT_CENTIMETER: &str = "cm";
pub const UNIT_TEXT_DEGREE: &str = "deg";
pub const UNIT_TEXT_EM: &str = "em";
pub const UNIT_TEXT_EX: &str = "ex";
pub const UNIT_TEXT_GRADIAN: &str = "grad";
pub const UNIT_TEXT_HERTZ: &str = "Hz";
pub const UNIT_TEXT_INCH: &str = "in";
pub const UNIT_TEXT_KILOHERTZ: &str = "kHz";
pub const UNIT_TEXT_MILLIMETER: &str = "mm";
pub const UNIT_TEXT_MILLISECOND: &str = "ms";
pub const UNIT_TEXT_PERCENTAGE: &str = "%";
pub const UNIT_TEXT_PICA: &str = "pc";
pub const UNIT_TEXT_PIXEL: &str = "px";
pub const UNIT_TEXT_POINT: &str = "pt";
pub const UNIT_TEXT_RADIAN: &str = "rad";
pub const UNIT_TEXT_REAL: &str = "";
pub const UNIT_TEXT_SECOND: &str = "s";
pub const UNIT_TEXT_DPI: &str = "dpi";
pub const UNIT_TEXT_DPCM: &str = "dpcm";

fn create(t: i16, prev: Option<&LexicalUnitRef>, line_number: i32, kind: Kind) -> LexicalUnitRef {
    let lu = Rc::new(CSSLexicalUnit { lexical_unit_type: t, next_lexical_unit: RefCell::new(None), previous_lexical_unit: RefCell::new(Weak::new()), line_number, kind });
    if let Some(p) = prev {
        *lu.previous_lexical_unit.borrow_mut() = Rc::downgrade(p);
        *p.next_lexical_unit.borrow_mut() = Some(lu.clone());
    }
    lu
}

/// `createSimple`: Batik throws `Error("Internal CSS parser error!")` for other types.
pub fn create_simple(t: i16, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    assert!(t == SAC_OPERATOR_COMMA || t == SAC_OPERATOR_SLASH || t == SAC_INHERIT, "Internal CSS parser error!");
    create(t, prev, line_number, Kind::Simple)
}

pub fn create_integer(val: i32, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    create(SAC_INTEGER, prev, line_number, Kind::Integer(val))
}

pub fn create_float(t: i16, val: f32, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    create(t, prev, line_number, Kind::Float(val))
}

pub fn create_dimension(val: f32, dim: String, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    create(SAC_DIMENSION, prev, line_number, Kind::Dimension(val, dim))
}

pub fn create_function(f: String, params: Option<LexicalUnitRef>, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    create(SAC_FUNCTION, prev, line_number, Kind::Function(f, params))
}

pub fn create_predefined_function(t: i16, params: Option<LexicalUnitRef>, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    create(t, prev, line_number, Kind::PredefinedFunction(params))
}

pub fn create_string(t: i16, val: String, prev: Option<&LexicalUnitRef>, line_number: i32) -> LexicalUnitRef {
    create(t, prev, line_number, Kind::String(val))
}

/// The `IllegalStateException` a getter throws for the wrong kind of unit.
#[derive(Clone, Copy, Debug)]
pub struct IllegalState;

impl CSSLexicalUnit {
    pub fn get_lexical_unit_type(&self) -> i16 {
        self.lexical_unit_type
    }

    pub fn get_next_lexical_unit(&self) -> Option<LexicalUnitRef> {
        self.next_lexical_unit.borrow().clone()
    }

    pub fn set_next_lexical_unit(&self, lu: Option<LexicalUnitRef>) {
        *self.next_lexical_unit.borrow_mut() = lu;
    }

    pub fn get_previous_lexical_unit(&self) -> Option<LexicalUnitRef> {
        self.previous_lexical_unit.borrow().upgrade()
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_integer_value(&self) -> Result<i32, IllegalState> {
        match self.kind {
            Kind::Integer(v) => Ok(v),
            _ => Err(IllegalState),
        }
    }

    pub fn get_float_value(&self) -> Result<f32, IllegalState> {
        match self.kind {
            Kind::Float(v) | Kind::Dimension(v, _) => Ok(v),
            _ => Err(IllegalState),
        }
    }

    pub fn get_dimension_unit_text(&self) -> Result<String, String> {
        if let Kind::Dimension(_, d) = &self.kind {
            return Ok(d.clone());
        }
        Ok(match self.lexical_unit_type {
            SAC_CENTIMETER => UNIT_TEXT_CENTIMETER,
            SAC_DEGREE => UNIT_TEXT_DEGREE,
            SAC_DPI => UNIT_TEXT_DPI,
            SAC_DPCM => UNIT_TEXT_DPCM,
            SAC_EM => UNIT_TEXT_EM,
            SAC_EX => UNIT_TEXT_EX,
            SAC_GRADIAN => UNIT_TEXT_GRADIAN,
            SAC_HERTZ => UNIT_TEXT_HERTZ,
            SAC_INCH => UNIT_TEXT_INCH,
            SAC_KILOHERTZ => UNIT_TEXT_KILOHERTZ,
            SAC_MILLIMETER => UNIT_TEXT_MILLIMETER,
            SAC_MILLISECOND => UNIT_TEXT_MILLISECOND,
            SAC_PERCENTAGE => UNIT_TEXT_PERCENTAGE,
            SAC_PICA => UNIT_TEXT_PICA,
            SAC_PIXEL => UNIT_TEXT_PIXEL,
            SAC_POINT => UNIT_TEXT_POINT,
            SAC_RADIAN => UNIT_TEXT_RADIAN,
            SAC_REAL => UNIT_TEXT_REAL,
            SAC_SECOND => UNIT_TEXT_SECOND,
            t => return Err(format!("No Unit Text for type: {t}")),
        }
        .to_string())
    }

    pub fn get_function_name(&self) -> Result<String, IllegalState> {
        match &self.kind {
            Kind::Function(n, _) => Ok(n.clone()),
            _ => Err(IllegalState),
        }
    }

    pub fn get_parameters(&self) -> Result<Option<LexicalUnitRef>, IllegalState> {
        match &self.kind {
            Kind::Function(_, p) | Kind::PredefinedFunction(p) => Ok(p.clone()),
            _ => Err(IllegalState),
        }
    }

    pub fn get_string_value(&self) -> Result<String, IllegalState> {
        match &self.kind {
            Kind::String(s) => Ok(s.clone()),
            _ => Err(IllegalState),
        }
    }

    pub fn get_sub_values(&self) -> Result<Option<LexicalUnitRef>, IllegalState> {
        Err(IllegalState)
    }

    fn predefined_name(&self) -> &'static str {
        match self.lexical_unit_type {
            SAC_COUNTER_FUNCTION => "counter",
            SAC_COUNTERS_FUNCTION => "counters",
            SAC_RGBCOLOR => "rgb",
            SAC_RECT_FUNCTION => "rect",
            _ => panic!("Internal CSS parser error!"),
        }
    }

    pub fn append_to_string_buffer(&self, buf: &mut String) {
        match &self.kind {
            Kind::Simple => {
                match self.lexical_unit_type {
                    SAC_OPERATOR_COMMA => buf.push(','),
                    SAC_OPERATOR_SLASH => buf.push('/'),
                    SAC_INHERIT => {
                        buf.push_str("inherit");
                        if self.next_lexical_unit.borrow().is_some() {
                            buf.push(' ');
                        }
                    }
                    _ => {}
                }
                if let Some(n) = self.get_next_lexical_unit() {
                    n.append_to_string_buffer(buf);
                }
            }
            Kind::Integer(v) => {
                buf.push_str(&v.to_string());
                self.append_next_to_string_buffer(buf);
            }
            Kind::Float(v) => {
                buf.push_str(&crate::xfl::num::java_float(*v));
                self.append_next_to_string_buffer(buf);
                buf.push_str(&self.get_dimension_unit_text().unwrap_or_default());
            }
            Kind::Dimension(v, d) => {
                buf.push_str(&crate::xfl::num::java_float(*v));
                buf.push_str(d);
                self.append_next_to_string_buffer(buf);
            }
            Kind::Function(name, params) => {
                buf.push_str(name);
                buf.push('(');
                params.as_ref().expect("NullPointerException").append_to_string_buffer(buf);
                buf.push(')');
                self.append_next_to_string_buffer(buf);
            }
            Kind::PredefinedFunction(params) => {
                buf.push_str(self.predefined_name());
                buf.push('(');
                params.as_ref().expect("NullPointerException").append_to_string_buffer(buf);
                buf.push(')');
                self.append_next_to_string_buffer(buf);
            }
            Kind::String(v) => {
                buf.push('"');
                buf.push_str(&v.replace('"', "'"));
                buf.push('"');
                self.append_next_to_string_buffer(buf);
            }
        }
    }

    fn append_next_to_string_buffer(&self, buf: &mut String) {
        if let Some(n) = self.get_next_lexical_unit() {
            buf.push(' ');
            n.append_to_string_buffer(buf);
        }
    }
}

impl std::fmt::Display for CSSLexicalUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buf = String::new();
        self.append_to_string_buffer(&mut buf);
        f.write_str(&buf)
    }
}
