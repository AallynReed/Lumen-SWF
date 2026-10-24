//! Literal rendering helpers: regular expressions, XML literals and decimals.

use super::ctx::LocalData;
use super::item::*;
use super::print::{append_to, to_string};
use super::writer::Writer;
use crate::abc::names::escape_as;

/// `RegExpAvm2Item.escapeRegExpString`: `None` when the pattern cannot be a literal.
pub fn escape_regexp(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut escape = false;
    for c in s.chars() {
        match c {
            '/' => {
                if !escape {
                    return None;
                }
                out.push('/');
            }
            '\\' => {
                out.push('\\');
                escape = true;
                continue;
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 32 => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => out.push(c),
        }
        escape = false;
    }
    Some(out)
}

fn xml_name_start(c: char) -> bool {
    matches!(c, ':' | 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}')
}

fn xml_name_char(c: char) -> bool {
    xml_name_start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// `XMLAVM2Item.handleSingleXml`: escapes newlines and tabs inside attribute values.
fn handle_single_xml(s: &str, in_attr: &mut bool, in_tag: &mut bool) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut j = 0;
    while j < chars.len() {
        let c = chars[j];
        match c {
            '"' => {
                if *in_tag {
                    *in_attr = !*in_attr;
                }
                out.push(c);
            }
            '<' => {
                out.push(c);
                if j + 1 < chars.len() && xml_name_start(chars[j + 1]) {
                    *in_tag = true;
                    let mut e = j + 2;
                    while e < chars.len() && xml_name_char(chars[e]) {
                        e += 1;
                    }
                    out.extend(&chars[j + 1..e]);
                    j = e - 1;
                }
            }
            '>' => {
                *in_tag = false;
                out.push(c);
            }
            _ if *in_attr => match c {
                '\r' => out.push_str("&#13;"),
                '\n' => out.push_str("&#10;"),
                '\t' => out.push_str("&#9;"),
                c => out.push(c),
            },
            c => out.push(c),
        }
        j += 1;
    }
    out
}

/// A small XML well-formedness check.
fn basic_xml(s: &str) -> bool {
    let b = s.trim();
    if !b.starts_with('<') || !b.ends_with('>') {
        return false;
    }
    let chars: Vec<char> = b.chars().collect();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    let mut roots = 0;
    while i < chars.len() {
        if chars[i] != '<' {
            if stack.is_empty() && !chars[i].is_whitespace() {
                return false;
            }
            i += 1;
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(p) => i += rest[..p + 3].chars().count(),
                None => return false,
            }
            if stack.is_empty() {
                roots += 1;
            }
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            match rest.find("]]>") {
                Some(p) => i += rest[..p + 3].chars().count(),
                None => return false,
            }
            if stack.is_empty() {
                roots += 1;
            }
            continue;
        }
        if rest.starts_with("<?") {
            match rest.find("?>") {
                Some(p) => i += rest[..p + 2].chars().count(),
                None => return false,
            }
            if stack.is_empty() {
                roots += 1;
            }
            continue;
        }
        let Some(end) = rest.find('>') else { return false };
        let tag: String = rest[1..end].to_string();
        i += rest[..end + 1].chars().count();
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name.trim() => {}
                _ => return false,
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let name: String = tag.trim_end_matches('/').chars().take_while(|&c| xml_name_char(c)).collect();
        if name.is_empty() || !xml_name_start(name.chars().next().unwrap()) {
            return false;
        }
        if stack.is_empty() {
            roots += 1;
        }
        if !self_closing {
            stack.push(name);
        }
    }
    stack.is_empty() && roots == 1
}

pub fn xml_append(parts: &List, w: &mut Writer, ld: &LocalData) {
    let ps = parts.borrow().clone();
    let (mut in_attr, mut in_tag) = (false, false);
    if ps.len() == 1 {
        if let K::Str(s) = &ps[0].borrow().k {
            let s = handle_single_xml(s, &mut in_attr, &mut in_tag);
            if !basic_xml(&s) {
                w.append("new XML(\"").append(&escape_as(&s)).append("\")");
                return;
            }
            w.append(&s);
            return;
        }
    }
    for (i, part) in ps.iter().enumerate() {
        let before = if i > 0 { Some(&ps[i - 1]) } else { None };
        let after = ps.get(i + 1);
        let is_attr = |x: Option<&It>| x.is_some_and(|x| matches!(x.borrow().k, K::EscapeXAttr));
        let s = match &part.borrow().k {
            K::Str(s) => Some(s.clone()),
            _ => None,
        };
        if let Some(mut s) = s {
            if is_attr(after) && s.ends_with('"') {
                s.pop();
            }
            if is_attr(before) && s.starts_with('"') {
                s.remove(0);
            }
            w.append(&handle_single_xml(&s, &mut in_attr, &mut in_tag));
        } else if matches!(part.borrow().k, K::EscapeXElem | K::EscapeXAttr) {
            to_string(part, w, ld, "", false);
        } else {
            w.append("{");
            append_to(part, w, ld);
            w.append("}");
        }
    }
}

/// `Decimal128.toActionScriptString`: BID-encoded IEEE 754 decimal128 with an `m` suffix.
pub fn decimal_to_string(d: &[u8; 16]) -> String {
    let v = u128::from_le_bytes(*d);
    let sign = v >> 127 != 0;
    let comb = (v >> 122) & 0x1f;
    if comb == 0x1f {
        return "NaN".into();
    }
    if comb == 0x1e {
        return if sign { "-Infinity".into() } else { "Infinity".into() };
    }
    let (exp, coeff) = if (v >> 125) & 3 == 3 {
        (((v >> 111) & 0x3fff) as i32, (v & ((1u128 << 111) - 1)) | (1u128 << 113))
    } else {
        (((v >> 113) & 0x3fff) as i32, v & ((1u128 << 113) - 1))
    };
    let exp = exp - 6176;
    let digits = coeff.to_string();
    let mut s = if exp >= 0 {
        format!("{digits}{}", "0".repeat(exp as usize))
    } else {
        let e = (-exp) as usize;
        if digits.len() > e {
            format!("{}.{}", &digits[..digits.len() - e], &digits[digits.len() - e..])
        } else {
            format!("0.{}{digits}", "0".repeat(e - digits.len()))
        }
    };
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    format!("{}{s}m", if sign { "-" } else { "" })
}
