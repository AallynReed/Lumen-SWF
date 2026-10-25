//! Port of `flash.css.Descriptor`: a CSS property value and its conversion to ActionScript literal text.

use crate::asc::flex2::batik::css::parser::css_lexical_unit::LexicalUnitRef;
use crate::asc::flex2::batik::css::parser::scanner::JavaEqIc;
use crate::asc::flex2::batik::css::sac::lexical_unit::*;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};

pub struct Descriptor {
    name: String,
    value: LexicalUnitRef,
    line_number: i32,
    path: String,
}

fn color_not_supported(color: &str) -> CompilerMessage {
    CompilerMessage::error(format!("the color '{color}' is not supported."))
}

fn invalid_format() -> CompilerMessage {
    CompilerMessage::error("invalid format.")
}

fn value_not_supported(value: Option<&str>) -> CompilerMessage {
    CompilerMessage::error(format!("CSS Value for '{}' not supported.", value.unwrap_or("null")))
}

fn function_not_supported(function: &str) -> CompilerMessage {
    CompilerMessage::error(format!("the function '{function}' is not supported."))
}

/// `Character.forDigit(d, 16)`.
fn for_digit(d: i32) -> char {
    std::char::from_digit(d as u32, 16).unwrap_or('\0')
}

/// `(new Float(f)).intValue()`.
fn float_int(f: f32) -> i32 {
    f as i32
}

type CResult<T> = Result<T, CompilerMessage>;

impl Descriptor {
    pub fn new(name: String, lexical_unit: LexicalUnitRef, path: String) -> Descriptor {
        let line_number = lexical_unit.get_line_number();
        Descriptor { name, value: lexical_unit, line_number, path }
    }

    pub fn convert_color_name(color: &str) -> Option<&'static str> {
        let first = crate::asc::util::jstr::units(color).next()?;
        let is = |n: &str| color.java_eq_ic(n);
        let r = match first {
            0x62 | 0x42 => {
                if is("black") {
                    "0x000000"
                } else if is("blue") {
                    "0x0000FF"
                } else {
                    return None;
                }
            }
            0x67 | 0x47 => {
                if is("green") {
                    "0x008000"
                } else if is("gray") {
                    "0x808080"
                } else {
                    return None;
                }
            }
            0x73 | 0x53 if is("silver") => "0xC0C0C0",
            0x6C | 0x4C if is("lime") => "0x00FF00",
            0x6F | 0x4F if is("olive") => "0x808000",
            0x77 | 0x57 if is("white") => "0xFFFFFF",
            0x79 | 0x59 if is("yellow") => "0xFFFF00",
            0x6D | 0x4D => {
                if is("maroon") {
                    "0x800000"
                } else if is("magenta") {
                    "0xFF00FF"
                } else {
                    return None;
                }
            }
            0x6E | 0x4E if is("navy") => "0x000080",
            0x72 | 0x52 if is("red") => "0xFF0000",
            0x70 | 0x50 if is("purple") => "0x800080",
            0x74 | 0x54 if is("teal") => "0x008080",
            0x66 | 0x46 if is("fuchsia") => "0xFF00FF",
            0x61 | 0x41 if is("aqua") => "0x00FFFF",
            0x63 | 0x43 if is("cyan") => "0x00FFFF",
            0x68 | 0x48 => {
                if is("haloGreen") {
                    "0x80FF4D"
                } else if is("haloBlue") {
                    "0x009DFF"
                } else if is("haloOrange") {
                    "0xFFB600"
                } else if is("haloSilver") {
                    "0xAECAD9"
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        Some(r)
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_value(&self) -> &LexicalUnitRef {
        &self.value
    }

    pub fn get_ident_as_string(&self) -> String {
        let mut sb = String::new();
        let mut lu = Some(self.value.clone());
        while let Some(u) = lu {
            let piece = match u.get_lexical_unit_type() {
                SAC_IDENT => u.get_string_value().ok(),
                SAC_OPERATOR_COMMA => Some(",".to_string()),
                SAC_INTEGER => u.get_integer_value().ok().map(|i| i.to_string()),
                _ => u.get_string_value().ok(),
            };
            match piece {
                Some(p) => sb.push_str(&p),
                None => toolkit::log(CompilerMessage::error("characters within CSS could not be parsed."), Some(&self.path), self.line_number, -1, None),
            }
            lu = u.get_next_lexical_unit();
        }
        sb
    }

    pub fn get_color_as_string(&self) -> CResult<String> {
        Self::color_as_string(Some(&self.name), &self.value)
    }

    fn color_as_string(name: Option<&str>, lexical_unit: &LexicalUnitRef) -> CResult<String> {
        match lexical_unit.get_lexical_unit_type() {
            SAC_IDENT => {
                let s = lexical_unit.get_string_value().unwrap_or_default();
                match Self::convert_color_name(&s) {
                    Some(c) => Ok(c.to_string()),
                    None => Err(color_not_supported(&s)),
                }
            }
            SAC_RGBCOLOR => {
                let mut sb = String::from("0x");
                let mut parameter = lexical_unit.get_parameters().ok().flatten();
                while let Some(p) = parameter {
                    let digit = match p.get_lexical_unit_type() {
                        SAC_INTEGER => Some(p.get_integer_value().unwrap_or(0)),
                        SAC_PERCENTAGE => Some(float_int(p.get_float_value().unwrap_or(0.0)).wrapping_mul(255) / 100),
                        _ => None,
                    };
                    if let Some(d) = digit {
                        sb.push(for_digit((d >> 4) & 15));
                        sb.push(for_digit(d & 15));
                    }
                    parameter = p.get_next_lexical_unit();
                }
                Ok(sb)
            }
            _ => Err(value_not_supported(name)),
        }
    }

    pub fn get_length_as_string(&self) -> CResult<String> {
        Self::length_as_string(Some(&self.name), &self.value)
    }

    fn length_as_string(name: Option<&str>, lexical_unit: &LexicalUnitRef) -> CResult<String> {
        let f = || lexical_unit.get_float_value().unwrap_or(0.0);
        let length: f32 = match lexical_unit.get_lexical_unit_type() {
            SAC_CENTIMETER => (f() * 72.0) / 2.54f32,
            SAC_MILLIMETER => ((f() * 72.0) / 10.0) / 2.54f32,
            SAC_INCH => f() * 72.0,
            SAC_PICA => f() * 12.0,
            SAC_PIXEL | SAC_POINT => f(),
            SAC_IDENT => {
                let s = lexical_unit.get_string_value().unwrap_or_default();
                if s.java_eq_ic("xx-small") {
                    7.0
                } else if s.java_eq_ic("x-small") {
                    8.0
                } else if s.java_eq_ic("small") {
                    10.0
                } else if s.java_eq_ic("medium") {
                    12.0
                } else if s.java_eq_ic("large") {
                    14.0
                } else if s.java_eq_ic("x-large") {
                    17.0
                } else if s.java_eq_ic("xx-large") {
                    21.0
                } else {
                    return Err(value_not_supported(name));
                }
            }
            _ => return Err(value_not_supported(name)),
        };
        Ok(float_int(length).to_string())
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    fn list_as_string(name: Option<&str>, value: Option<LexicalUnitRef>, quote_identifier: bool) -> CResult<String> {
        let mut sb = String::new();
        let mut current = value;
        while let Some(c) = current {
            sb.push_str(&Self::lexical_unit_as_string(name, &c, quote_identifier)?);
            match c.get_next_lexical_unit() {
                Some(next) => {
                    if next.get_lexical_unit_type() == SAC_OPERATOR_COMMA {
                        current = next.get_next_lexical_unit();
                        sb.push_str(", ");
                    } else if next.get_lexical_unit_type() == SAC_OPERATOR_SLASH {
                        current = next.get_next_lexical_unit();
                        sb.push_str(" = ");
                    } else {
                        return Err(invalid_format());
                    }
                }
                None => current = None,
            }
        }
        Ok(sb)
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn get_time_as_string(&self) -> CResult<String> {
        Self::time_as_string(Some(&self.name), &self.value)
    }

    fn time_as_string(name: Option<&str>, lexical_unit: &LexicalUnitRef) -> CResult<String> {
        let f = lexical_unit.get_float_value().unwrap_or(0.0);
        let time = match lexical_unit.get_lexical_unit_type() {
            SAC_MILLISECOND => f,
            SAC_SECOND => f * 60.0,
            _ => return Err(value_not_supported(name)),
        };
        Ok(float_int(time).to_string())
    }

    pub fn get_value_as_string(&self) -> CResult<String> {
        let next = self.value.get_next_lexical_unit();
        if next.is_some_and(|n| n.get_lexical_unit_type() == SAC_OPERATOR_COMMA) {
            Ok(format!("[{}]", Self::list_as_string(Some(&self.name), Some(self.value.clone()), true)?))
        } else {
            Self::lexical_unit_as_string(Some(&self.name), &self.value, true)
        }
    }

    fn lexical_unit_as_string(name: Option<&str>, lexical_unit: &LexicalUnitRef, quote_identifier: bool) -> CResult<String> {
        Ok(match lexical_unit.get_lexical_unit_type() {
            SAC_MILLISECOND | SAC_SECOND => Self::time_as_string(name, lexical_unit)?,
            SAC_CENTIMETER | SAC_MILLIMETER | SAC_INCH | SAC_PICA | SAC_POINT | SAC_PIXEL => Self::length_as_string(name, lexical_unit)?,
            SAC_RGBCOLOR => Self::color_as_string(name, lexical_unit)?,
            SAC_INTEGER => lexical_unit.get_integer_value().unwrap_or(0).to_string(),
            SAC_REAL => crate::xfl::num::java_float(lexical_unit.get_float_value().unwrap_or(0.0)),
            SAC_STRING_VALUE | SAC_URI => format!("\"{}\"", lexical_unit.get_string_value().unwrap_or_default().replace('"', "'")),
            SAC_IDENT => match Self::color_as_string(name, lexical_unit) {
                Ok(c) => c,
                Err(_) => match Self::length_as_string(name, lexical_unit) {
                    Ok(l) => l,
                    Err(_) => {
                        let string_value = lexical_unit.get_string_value().unwrap_or_default();
                        if string_value.java_eq_ic("false") || string_value.java_eq_ic("true") || !quote_identifier {
                            string_value
                        } else {
                            format!("\"{}\"", string_value.replace('"', "'"))
                        }
                    }
                },
            },
            SAC_FUNCTION => {
                let function_name = lexical_unit.get_function_name().unwrap_or_default();
                if function_name == "Embed" || function_name == "ClassReference" || function_name == "PropertyReference" {
                    format!("{function_name}({})", Self::list_as_string(None, lexical_unit.get_parameters().ok().flatten(), false)?)
                } else {
                    return Err(function_not_supported(&function_name));
                }
            }
            _ => return Err(value_not_supported(name)),
        })
    }
}

impl std::fmt::Display for Descriptor {
    /// `Object.toString()`, which `FontFaceRule.formatFontStyle` prints.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "flash.css.Descriptor@{:x}", self as *const Descriptor as usize)
    }
}
