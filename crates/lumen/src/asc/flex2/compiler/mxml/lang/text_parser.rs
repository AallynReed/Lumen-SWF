//! Port of `flex2.compiler.mxml.lang.TextParser`: the abstract class is a trait whose required methods are its
//! abstract callbacks; `parse` and the other concrete methods are provided.

use std::rc::Rc;

use super::standard_defs;
use crate::asc::flex2::compiler::mxml::dom::xerces::xml_char;
use crate::asc::flex2::compiler::mxml::reflect::{TypeRef, TypeTable};
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::mxml::rep::model::RValue;
use crate::asc::flex2::flash::css::descriptor::Descriptor;
use crate::asc::flex2::flash::util::string_utils::{self, java_trim};
use crate::asc::util::jstr;

pub const OK: i32 = 0;
pub const ERR_TYPE_NOT_EMBEDDABLE: i32 = 1;
pub const ERR_INVALID_TEXT_FOR_TYPE: i32 = 2;
pub const ERR_INVALID_PERCENTAGE: i32 = 3;
pub const ERR_TYPE_NOT_SERIALIZABLE: i32 = 4;
pub const ERR_PERCENTAGES_NOT_ALLOWED: i32 = 5;
pub const ERR_TYPE_NOT_CONTEXT_ROOTABLE: i32 = 6;
pub const ERR_UNRECOGNIZED_AT_FUNCTION: i32 = 7;
pub const ERR_UNDEFINED_CONTEXT_ROOT: i32 = 8;
pub const ERR_INVALID_TWO_WAY_BIND: i32 = 9;

pub const FLAG_IN_CDATA: i32 = 1;
pub const FLAG_COLLAPSE_WHITE_SPACE: i32 = 2;
pub const FLAG_CONVERT_COLOR_NAMES: i32 = 4;
pub const FLAG_ALLOW_PERCENTAGES: i32 = 8;
pub const FLAG_IGNORE_BINDING: i32 = 16;
pub const FLAG_IGNORE_AT_FUNCTION: i32 = 32;
pub const FLAG_IGNORE_ARRAY_SYNTAX: i32 = 64;
pub const FLAG_IGNORE_AT_FUNCTION_ESCAPE: i32 = 128;
pub const FLAG_RICH_TEXT_CONTENT: i32 = 256;

/// `flex2.compiler.common.MxmlConfiguration.VERSION_4_0`.
pub const VERSION_4_0: i32 = 0x04000000;

/// The constructor's `ignoreTwoWayBinding`.
pub fn ignore_two_way_binding_for(compatibility_version: i32) -> bool {
    if compatibility_version == 0 {
        false
    } else {
        compatibility_version < VERSION_4_0
    }
}

enum Parsed {
    Value(RValue),
    Null,
    Error(i32),
}

fn type_eq(a: &TypeRef, b: &TypeRef) -> bool {
    **a == **b
}

pub trait TextParser {
    fn type_table(&self) -> Rc<TypeTable>;
    fn ignore_two_way_binding(&self) -> bool;

    fn context_root(&mut self, text: &str) -> Option<RValue>;
    fn clear(&mut self) -> Option<RValue>;
    fn embed(&mut self, text: &str, t: &TypeRef) -> Option<RValue>;
    fn resource(&mut self, text: &str, t: &TypeRef) -> Option<RValue>;
    fn binding_expression(&mut self, converted: &str) -> Option<BindingExpressionRef>;
    fn binding_expression_two_way(&mut self, converted: &str, is_two_way: bool) -> Option<BindingExpressionRef>;
    fn parse_binding_expression_line(&mut self, text: &str, line: i32) -> Option<BindingExpressionRef>;
    fn percentage(&mut self, pct: &str) -> Option<RValue>;
    fn array(&mut self, entries: Vec<RValue>, array_element_type: Option<&TypeRef>) -> Option<RValue>;
    fn function_text(&mut self, name: &str) -> Option<RValue>;
    fn class_name(&mut self, name: &str, t: &TypeRef) -> Option<RValue>;
    fn error(&mut self, error: i32, text: &str, t: Option<&TypeRef>, array_element_type: Option<&TypeRef>);

    /// `parse(text, type, arrayElementType, flags)`.
    fn parse(&mut self, text: &str, t: &TypeRef, array_element_type: Option<&TypeRef>, mut flags: i32) -> Option<RValue> {
        let mut text = text.to_string();
        if !in_cdata(flags) {
            if !ignore_binding(flags) {
                if let Some(result) = self.parse_binding_expression(&text) {
                    return Some(RValue::Binding(result));
                } else {
                    text = cleanup_binding_escapes(&text);
                }
            }
            if !ignore_at_function(flags) {
                if let Some(at_function_name) = get_at_function_name(&text) {
                    return self.parse_at_function(&at_function_name, &text, t, array_element_type, flags);
                } else {
                    text = cleanup_at_function_escapes(&text);
                    flags |= FLAG_IGNORE_AT_FUNCTION_ESCAPE;
                }
            }
        }
        if is_rich_text_content(flags) {
            let parsed_string = parse_string(&text, flags);
            if type_eq(t, &self.type_table().array_type()) {
                self.array(vec![RValue::Str(parsed_string)], array_element_type)
            } else {
                Some(RValue::Str(parsed_string))
            }
        } else {
            self.parse_value(&text, t, array_element_type, flags)
        }
    }

    /// `parseBindingExpression(String)`.
    fn parse_binding_expression(&mut self, s: &str) -> Option<BindingExpressionRef> {
        let mut at_idx = -1;
        let mut open_brace_idx = -1;
        if !self.ignore_two_way_binding() {
            at_idx = string_utils::find_next_unescaped('@', 0, s);
            if at_idx >= 0 {
                open_brace_idx = string_utils::find_next_unescaped('{', at_idx + 1, s);
                if open_brace_idx != at_idx + 1 {
                    at_idx = -1;
                }
            }
        }
        if at_idx == -1 {
            open_brace_idx = string_utils::find_next_unescaped('{', 0, s);
        }
        if open_brace_idx == -1 {
            return None;
        }
        let mut close_brace_idx = string_utils::find_closing_token('{', '}', s, open_brace_idx);
        if close_brace_idx == -1 {
            return None;
        }
        if at_idx >= 0 {
            if !java_trim(&jstr::substring(s, 0, at_idx)).is_empty() || !java_trim(&jstr::substring_from(s, close_brace_idx + 1)).is_empty() {
                self.error(ERR_INVALID_TWO_WAY_BIND, s, None, None);
                return None;
            }
            let contents = jstr::substring(s, open_brace_idx + 1, close_brace_idx);
            if contents.is_empty() || is_binding_expression(&contents) {
                self.error(ERR_INVALID_TWO_WAY_BIND, s, None, None);
                return None;
            }
            return self.binding_expression_two_way(&contents, true);
        }
        let mut buf = String::new();
        let lead = jstr::substring(s, 0, open_brace_idx);
        if !java_trim(&lead).is_empty() {
            let text = cleanup_at_function_escapes(&cleanup_binding_escapes(&lead));
            buf.push_str(&string_utils::format_string(&text));
            buf.push_str(" + ");
        }
        while open_brace_idx != -1 {
            buf.push('(');
            let mut contents = jstr::substring(s, open_brace_idx + 1, close_brace_idx);
            if java_trim(&contents).is_empty() {
                contents = "''".to_string();
            }
            buf.push_str(&contents);
            buf.push(')');
            let last_close = close_brace_idx;
            open_brace_idx = string_utils::find_next_unescaped('{', last_close, s);
            if open_brace_idx != -1 {
                buf.push_str(" + ");
                close_brace_idx = string_utils::find_closing_token('{', '}', s, open_brace_idx);
                if close_brace_idx != -1 {
                    let text = cleanup_at_function_escapes(&cleanup_binding_escapes(&jstr::substring(s, last_close + 1, open_brace_idx)));
                    buf.push_str(&string_utils::format_string(&text));
                    buf.push_str(" + ");
                } else {
                    buf.push_str(&string_utils::format_string(&jstr::substring_from(s, last_close + 1)));
                    open_brace_idx = -1;
                }
            } else {
                let tail = jstr::substring_from(s, last_close + 1);
                if !java_trim(&tail).is_empty() {
                    buf.push_str(" + ");
                    let text = cleanup_at_function_escapes(&cleanup_binding_escapes(&tail));
                    buf.push_str(&string_utils::format_string(&text));
                }
            }
        }
        self.binding_expression(&buf)
    }

    /// `parseValue(text, type, arrayElementType, flags)`.
    fn parse_value(&mut self, text: &str, t: &TypeRef, array_element_type: Option<&TypeRef>, flags: i32) -> Option<RValue> {
        let tt = self.type_table();
        let result: Parsed = if type_eq(t, &tt.no_type()) || type_eq(t, &tt.object_type()) {
            match self.parse_object(text, array_element_type, flags) {
                Some(v) => Parsed::Value(v),
                None => Parsed::Null,
            }
        } else if type_eq(t, &tt.string_type()) {
            Parsed::Value(RValue::Str(parse_string(text, flags)))
        } else if type_eq(t, &tt.number_type()) || type_eq(t, &tt.int_type()) || type_eq(t, &tt.uint_type()) {
            let isint = type_eq(t, &tt.int_type()) && !type_eq(t, &tt.number_type());
            let isuint = !isint && type_eq(t, &tt.uint_type()) && !type_eq(t, &tt.number_type());
            if jstr::index_of_unit(text, b'%' as u16, 0) >= 0 {
                if allow_percentages(flags) {
                    match parse_percentage(text) {
                        Some(p) => match self.percentage(&p) {
                            Some(v) => Parsed::Value(v),
                            None => Parsed::Null,
                        },
                        None => Parsed::Error(ERR_INVALID_PERCENTAGE),
                    }
                } else {
                    Parsed::Error(ERR_PERCENTAGES_NOT_ALLOWED)
                }
            } else {
                let v = if isint {
                    parse_int(text, flags).map(RValue::Integer)
                } else if isuint {
                    parse_uint(text, flags).map(RValue::Long)
                } else {
                    parse_number(text, flags)
                };
                match v {
                    Some(v) => Parsed::Value(v),
                    None => Parsed::Null,
                }
            }
        } else if type_eq(t, &tt.boolean_type()) {
            match parse_boolean(text) {
                Some(b) => Parsed::Value(RValue::Boolean(b)),
                None => Parsed::Null,
            }
        } else if type_eq(t, &tt.reg_exp_type()) {
            match parse_reg_exp(text) {
                Some(r) => Parsed::Value(RValue::Str(r)),
                None => Parsed::Null,
            }
        } else if type_eq(t, &tt.array_type()) {
            match self.parse_array(text, array_element_type, true, flags) {
                Some(c) => match self.array(c, array_element_type) {
                    Some(v) => Parsed::Value(v),
                    None => Parsed::Null,
                },
                None => Parsed::Null,
            }
        } else if type_eq(t, &tt.function_type()) {
            let f = parse_function(text);
            match self.function_text(&f) {
                Some(v) => Parsed::Value(v),
                None => Parsed::Null,
            }
        } else if self.accepts_class_ref(t) {
            match parse_class_name(text) {
                Some(c) => match self.class_name(&c, t) {
                    Some(v) => Parsed::Value(v),
                    None => Parsed::Null,
                },
                None => Parsed::Null,
            }
        } else {
            Parsed::Error(ERR_TYPE_NOT_SERIALIZABLE)
        };
        let result = match result {
            Parsed::Null => Parsed::Error(ERR_INVALID_TEXT_FOR_TYPE),
            r => r,
        };
        match result {
            Parsed::Error(errno) => {
                self.error(errno, text, Some(t), array_element_type);
                None
            }
            Parsed::Value(v) => Some(v),
            Parsed::Null => unreachable!(),
        }
    }

    fn accepts_class_ref(&self, t: &TypeRef) -> bool {
        let tt = self.type_table();
        type_eq(t, &tt.class_type()) || tt.get_standard_defs().is_instance_generator(t)
    }

    /// `parseObject(text, arrayElementType, flags)`.
    fn parse_object(&mut self, text: &str, array_element_type: Option<&TypeRef>, flags: i32) -> Option<RValue> {
        let temp = java_trim(text);
        if let Some(b) = parse_boolean(&temp) {
            Some(RValue::Boolean(b))
        } else if let Some(res) = self.parse_array(&temp, array_element_type, false, flags) {
            self.array(res, array_element_type)
        } else if let Some(n) = parse_number(&temp, flags) {
            Some(n)
        } else {
            Some(RValue::Str(text.to_string()))
        }
    }

    /// `parseArray(text, elementType, coerceSingleton, flags)`.
    fn parse_array(&mut self, text: &str, element_type: Option<&TypeRef>, coerce_singleton: bool, flags: i32) -> Option<Vec<RValue>> {
        let trimmed = java_trim(text);
        let object_type = self.type_table().object_type();
        if ignore_array_syntax(flags) || !is_array(&trimmed) {
            if coerce_singleton {
                let et = element_type.cloned().expect("NullPointerException");
                let element = self.parse_value(text, &et, Some(&object_type), flags);
                return element.map(|e| vec![e]);
            } else {
                return None;
            }
        }
        if is_empty_array(&trimmed) {
            return Some(Vec::new());
        }
        let mut result = Vec::new();
        let mut buffer: Vec<u16> = Vec::new();
        let mut quote_char = b'\'' as u16;
        let mut in_quotes = false;
        let chars = jstr::to_utf16(&trimmed);
        for &c in &chars[1..] {
            match c {
                0x5b => {
                    if in_quotes {
                        buffer.push(c);
                    }
                }
                0x22 | 0x27 => {
                    if in_quotes {
                        if quote_char == c {
                            in_quotes = false;
                        } else {
                            buffer.push(c);
                        }
                    } else {
                        in_quotes = true;
                        quote_char = c;
                    }
                }
                0x2c | 0x5d => {
                    if in_quotes {
                        buffer.push(c);
                    } else {
                        let element_text = java_trim(&jstr::from_utf16(&buffer));
                        buffer.clear();
                        let et = element_type.cloned().expect("NullPointerException");
                        match self.parse_value(&element_text, &et, Some(&object_type), 0) {
                            Some(e) => result.push(e),
                            None => return None,
                        }
                    }
                }
                _ => buffer.push(c),
            }
        }
        Some(result)
    }

    /// `parseAtFunction(functionName, text, type, arrayElementType, flags)`.
    fn parse_at_function(&mut self, function_name: &str, text: &str, t: &TypeRef, array_element_type: Option<&TypeRef>, _flags: i32) -> Option<RValue> {
        let string_type = self.type_table().string_type();
        match function_name {
            "Embed" => {
                if string_type.is_assignable_to(Some(t)) || self.accepts_class_ref(t) {
                    self.embed(text, t)
                } else {
                    self.error(ERR_TYPE_NOT_EMBEDDABLE, text, Some(t), array_element_type);
                    None
                }
            }
            "ContextRoot" => {
                if string_type.is_assignable_to(Some(t)) {
                    self.context_root(text)
                } else {
                    self.error(ERR_TYPE_NOT_CONTEXT_ROOTABLE, text, Some(t), array_element_type);
                    None
                }
            }
            "Resource" => self.resource(text, t),
            "Clear" => self.clear(),
            _ => {
                self.error(ERR_UNRECOGNIZED_AT_FUNCTION, text, Some(t), array_element_type);
                None
            }
        }
    }
}

fn has_leading_zeros(s: &str) -> bool {
    jstr::utf16_len(s) > 1 && s.starts_with('0') && !(s.starts_with("0x") || s.starts_with("0X") || s.starts_with("0."))
}

/// `Long.decode(s)`, limited to the range the caller checks.
fn long_decode(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let (neg, rest) = match s.as_bytes()[0] {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let (radix, digits) = if let Some(r) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16, r)
    } else if let Some(r) = rest.strip_prefix('#') {
        (16, r)
    } else if rest.starts_with('0') && rest.len() > 1 {
        (8, &rest[1..])
    } else {
        (10, rest)
    };
    if digits.is_empty() || digits.starts_with('-') || digits.starts_with('+') || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let v = i128::from_str_radix(digits, radix).ok()?;
    let v = if neg { -v } else { v };
    i64::try_from(v).ok()
}

fn convert_color(s: &str, flags: i32) -> String {
    if convert_color_names(flags) {
        if let Some(c) = Descriptor::convert_color_name(s) {
            return c.to_string();
        }
    }
    s.to_string()
}

/// `parseInt(s, flags)`: `Integer.decode`, with 0x and # prefixes.
fn parse_int(s: &str, flags: i32) -> Option<i32> {
    let s = convert_color(s, flags);
    if has_leading_zeros(&s) {
        return None;
    }
    long_decode(&s).and_then(|v| i32::try_from(v).ok())
}

fn parse_uint(s: &str, flags: i32) -> Option<i64> {
    let s = convert_color(s, flags);
    if has_leading_zeros(&s) {
        return None;
    }
    let val = long_decode(&s)?;
    if val == val.abs() && val <= 0xffffffff {
        Some(val)
    } else {
        None
    }
}

fn parse_number(s: &str, flags: i32) -> Option<RValue> {
    if has_leading_zeros(s) {
        return None;
    }
    if let Some(i) = parse_int(s, flags) {
        return Some(RValue::Integer(i));
    }
    java_double_value_of(s).map(RValue::Double)
}

/// `Double.valueOf(s)`.
pub fn java_double_value_of(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c <= ' ');
    if t.is_empty() {
        return None;
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    match body {
        "Infinity" => return Some(sign * f64::INFINITY),
        "NaN" => return Some(f64::NAN),
        _ => {}
    }
    let body = body.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(body);
    if body.starts_with("0x") || body.starts_with("0X") {
        return parse_hex_float(&body[2..]).map(|v| sign * v);
    }
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    if i != bytes.len() {
        return None;
    }
    body.parse::<f64>().ok().map(|v| sign * v)
}

fn parse_hex_float(s: &str) -> Option<f64> {
    let (mant, exp) = s.split_once(['p', 'P'])?;
    let exp: i32 = exp.parse().ok()?;
    let (int_part, frac_part) = mant.split_once('.').unwrap_or((mant, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    let mut v = 0f64;
    for c in int_part.chars() {
        v = v * 16.0 + c.to_digit(16)? as f64;
    }
    let mut scale = 1.0 / 16.0;
    for c in frac_part.chars() {
        v += c.to_digit(16)? as f64 * scale;
        scale /= 16.0;
    }
    Some(v * 2f64.powi(exp))
}

fn parse_string(text: &str, flags: i32) -> String {
    let mut text = text.to_string();
    if collapse_white_space(flags) && !in_cdata(flags) {
        text = string_utils::collapse_whitespace(&text, ' ');
    }
    let u = jstr::to_utf16(&text);
    if !ignore_at_function_escape(flags) && u.len() > 1 && u[0] == 0x5c && (u[1] == 0x5c || u[1] == b'@' as u16) {
        text = jstr::from_utf16(&u[1..]);
    }
    text
}

/// `parseStringList(text)`.
pub fn parse_string_list(text: Option<&str>) -> Vec<String> {
    let mut text = match text {
        Some(t) => {
            let trimmed = java_trim(t);
            if !is_array(&trimmed) {
                format!("[{t}]")
            } else if is_empty_array(&trimmed) {
                return Vec::new();
            } else {
                t.to_string()
            }
        }
        None => String::new(),
    };
    let chars = jstr::to_utf16(&text);
    text.clear();
    let mut result = Vec::new();
    let mut buffer: Vec<u16> = Vec::new();
    let mut quote_char = b'\'' as u16;
    let mut in_quotes = false;
    for &c in chars.iter().skip(1) {
        match c {
            0x5b => {
                if in_quotes {
                    buffer.push(c);
                }
            }
            0x22 | 0x27 => {
                if in_quotes {
                    if quote_char == c {
                        in_quotes = false;
                    } else {
                        buffer.push(c);
                    }
                } else {
                    in_quotes = true;
                    quote_char = c;
                }
            }
            0x2c | 0x5d => {
                if in_quotes {
                    buffer.push(c);
                } else {
                    result.push(java_trim(&jstr::from_utf16(&buffer)));
                    buffer.clear();
                }
            }
            _ => buffer.push(c),
        }
    }
    result
}

/// `Boolean.valueOf(collapseWhitespace(text))`, null unless true or literally "false".
fn parse_boolean(text: &str) -> Option<bool> {
    let b = string_utils::collapse_whitespace(text, ' ').eq_ignore_ascii_case("true");
    if b || text.eq_ignore_ascii_case("false") {
        Some(b)
    } else {
        None
    }
}

/// `/.*/[gimsx]*`.
fn parse_reg_exp(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"\A/[^\n\r\u{85}\u{2028}\u{2029}]*/[gimsx]*\z").unwrap();
    if re.is_match(text) {
        Some(text.to_string())
    } else {
        None
    }
}

/// `\s*((\d+)(.(\d)+)?)\s*%\s*`, returning group 1 plus '%'.
fn parse_percentage(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"\A[ \t\n\x0B\f\r]*(([0-9]+)([^\n\r\u{85}\u{2028}\u{2029}]([0-9])+)?)[ \t\n\x0B\f\r]*%[ \t\n\x0B\f\r]*\z").unwrap();
    re.captures(text).map(|c| format!("{}%", &c[1]))
}

fn parse_function(text: &str) -> String {
    java_trim(text)
}

/// `parseClassName(text)`.
pub fn parse_class_name(text: &str) -> Option<String> {
    let name = java_trim(text);
    if is_qualified_name(&name) {
        Some(name)
    } else {
        None
    }
}

fn in_cdata(flags: i32) -> bool {
    flags & FLAG_IN_CDATA == FLAG_IN_CDATA
}
fn collapse_white_space(flags: i32) -> bool {
    flags & FLAG_COLLAPSE_WHITE_SPACE == FLAG_COLLAPSE_WHITE_SPACE
}
fn convert_color_names(flags: i32) -> bool {
    flags & FLAG_CONVERT_COLOR_NAMES == FLAG_CONVERT_COLOR_NAMES
}
fn allow_percentages(flags: i32) -> bool {
    flags & FLAG_ALLOW_PERCENTAGES == FLAG_ALLOW_PERCENTAGES
}
fn ignore_binding(flags: i32) -> bool {
    flags & FLAG_IGNORE_BINDING == FLAG_IGNORE_BINDING
}
fn ignore_at_function(flags: i32) -> bool {
    flags & FLAG_IGNORE_AT_FUNCTION == FLAG_IGNORE_AT_FUNCTION
}
fn ignore_array_syntax(flags: i32) -> bool {
    flags & FLAG_IGNORE_ARRAY_SYNTAX == FLAG_IGNORE_ARRAY_SYNTAX
}
fn ignore_at_function_escape(flags: i32) -> bool {
    flags & FLAG_IGNORE_AT_FUNCTION_ESCAPE == FLAG_IGNORE_AT_FUNCTION_ESCAPE
}
fn is_rich_text_content(flags: i32) -> bool {
    flags & FLAG_RICH_TEXT_CONTENT == FLAG_RICH_TEXT_CONTENT
}

pub fn cleanup_binding_escapes(to_clean: &str) -> String {
    let s = string_utils::cleanup_escaped_char('{', to_clean);
    string_utils::cleanup_escaped_char('}', &s)
}

pub fn cleanup_at_function_escapes(to_clean: &str) -> String {
    string_utils::cleanup_escaped_char('@', to_clean)
}

pub fn replace_binding_escapes_for_e4x(to_clean: &str) -> String {
    let s = string_utils::cleanup_escaped_char_for_xml('{', to_clean);
    let s = string_utils::cleanup_escaped_char_for_xml('}', &s);
    string_utils::cleanup_escaped_char_for_xml('@', &s)
}

pub fn is_binding_expression(s: &str) -> bool {
    let open_brace_idx = string_utils::find_nth_unescaped('{', 1, s);
    if open_brace_idx == -1 {
        return false;
    }
    string_utils::find_closing_token('{', '}', s, open_brace_idx) != -1
}

pub fn is_two_way_binding_expression(s: &str) -> bool {
    let open_brace_idx = string_utils::find_nth_unescaped('{', 1, s);
    if open_brace_idx == -1 || open_brace_idx == 0 {
        return false;
    }
    let at_idx = string_utils::find_nth_unescaped('@', 1, s);
    if at_idx != open_brace_idx - 1 {
        return false;
    }
    string_utils::find_closing_token('{', '}', s, open_brace_idx) != -1
}

/// `getAtFunctionName(value)`.
pub fn get_at_function_name(value: &str) -> Option<String> {
    let value = java_trim(value);
    let u = jstr::to_utf16(&value);
    if u.len() > 1 && u[0] == b'@' as u16 {
        let open_paren = jstr::index_of_unit(&value, b'(' as u16, 0);
        if open_paren > 1 && jstr::index_of_unit(&value, b')' as u16, 0) > open_paren {
            return Some(jstr::substring(&value, 1, open_paren));
        }
    }
    None
}

fn is_array(text: &str) -> bool {
    let u = jstr::to_utf16(text);
    !(u.len() < 2 || u[0] != b'[' as u16 || u[u.len() - 1] != b']' as u16)
}

fn is_empty_array(text: &str) -> bool {
    is_array(text) && java_trim(&jstr::substring(text, 1, jstr::utf16_len(text) - 1)).is_empty()
}

pub fn is_valid_identifier(id: &str) -> bool {
    let u = jstr::to_utf16(id);
    if u.is_empty() || !is_java_identifier_start(u[0]) {
        return false;
    }
    if !u[1..].iter().all(|&c| is_java_identifier_part(c)) {
        return false;
    }
    !standard_defs::is_reserved_word(id)
}

pub fn is_valid_state_identifier(id: Option<&str>) -> bool {
    let Some(id) = id else { return false };
    let u = jstr::to_utf16(id);
    if !u.iter().all(|&c| is_java_identifier_part(c)) {
        return false;
    }
    is_valid_xml_name(&u)
}

/// `XMLChar.isValidName(name)`.
fn is_valid_xml_name(u: &[u16]) -> bool {
    if u.is_empty() || !xml_char::is_name_start(u[0] as i32) {
        return false;
    }
    u[1..].iter().all(|&c| xml_char::is_name(c as i32))
}

pub fn is_scoped_name(name: &str) -> bool {
    name.contains('.')
}

/// `analyzeScopedName(name)`: `name.split("\\.")` when it has exactly two parts.
pub fn analyze_scoped_name(name: &str) -> Option<Vec<String>> {
    let mut parts: Vec<String> = name.split('.').map(str::to_string).collect();
    while parts.len() > 1 && parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    if parts.len() == 1 && parts[0].is_empty() && !name.is_empty() {
        parts.clear();
    }
    if parts.len() != 2 {
        None
    } else {
        Some(parts)
    }
}

/// `Character.isJavaIdentifierStart(char)`.
pub fn is_java_identifier_start(c: u16) -> bool {
    match c {
        0x24 | 0x5f => true,
        0x41..=0x5a | 0x61..=0x7a => true,
        0..=0x7f => false,
        0xa2..=0xa5 | 0x20a0..=0x20cf => true,
        _ => char::from_u32(c as u32).is_some_and(|ch| ch.is_alphabetic()),
    }
}

/// `Character.isJavaIdentifierPart(char)`.
pub fn is_java_identifier_part(c: u16) -> bool {
    match c {
        0..=8 | 0x0e..=0x1b | 0x7f..=0x9f => true,
        0x30..=0x39 => true,
        0..=0x7e => is_java_identifier_start(c),
        0xad | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x2064 | 0xfeff => true,
        _ => is_java_identifier_start(c) || char::from_u32(c as u32).is_some_and(|ch| ch.is_alphanumeric()),
    }
}

/// `([a-zA-Z_]\w*)(\.([a-zA-Z_]\w*))*`, not a reserved word.
fn is_qualified_name(text: &str) -> bool {
    let ok = !text.is_empty()
        && text.split('.').all(|w| {
            let b = w.as_bytes();
            !b.is_empty() && (b[0].is_ascii_alphabetic() || b[0] == b'_') && b[1..].iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        });
    ok && !standard_defs::is_reserved_word(text)
}
