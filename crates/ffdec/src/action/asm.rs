//! AS1/2 p-code parser: compiles p-code text into an ActionList.

use std::collections::HashMap;

use crate::error::{Error, Result};
use super::{simple_name, Action, ActionList, Charset, DefineFunction2, Op, Value};

/// Opcode lookup table by name.
fn opcode_by_name(name: &str) -> Option<u8> {
    for code in 0..=0xff {
        if let Some(n) = simple_name(code) {
            if n.eq_ignore_ascii_case(name) {
                return Some(code);
            }
        }
    }
    match name.to_ascii_lowercase().as_str() {
        "end" => Some(0x00),
        "gotoframe" => Some(0x81),
        "geturl" => Some(0x83),
        "waitforframe" => Some(0x8a),
        "settarget" => Some(0x8b),
        "gotolabel" => Some(0x8c),
        "push" => Some(0x96),
        "jump" => Some(0x99),
        "if" => Some(0x9d),
        "geturl2" => Some(0x9a),
        "gotoframe2" => Some(0x9f),
        "waitforframe2" => Some(0x8d),
        "constantpool" => Some(0x88),
        "definefunction" => Some(0x9b),
        "with" => Some(0x94),
        "storeregister" => Some(0x87),
        "definefunction2" => Some(0x8e),
        "try" => Some(0x8f),
        "strictmode" => Some(0x89),
        _ => None,
    }
}

pub fn parse_pcode(text: &str, version: u8) -> Result<ActionList> {
    let charset = Charset::for_version(version);
    let mut actions: Vec<Action> = Vec::new();
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut pending_jumps: Vec<(usize, String)> = Vec::new();
    let mut block_stack: Vec<(usize, usize)> = Vec::new(); // (action_idx, start_action_idx)

    for line in text.lines() {
        let mut line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(pos) = line.find("//") {
            line = line[..pos].trim();
            if line.is_empty() {
                continue;
            }
        }

        // Handle label prefix or standalone label: locXXXX:
        while let Some(colon) = line.find(':') {
            let candidate = line[..colon].trim();
            if candidate.starts_with("loc") && !candidate.contains(' ') {
                labels.insert(candidate.to_string(), actions.len());
                line = line[colon + 1..].trim();
            } else {
                break;
            }
        }
        if line.is_empty() {
            continue;
        }

        // Handle block closing brace '}'
        if line == "}" {
            if let Some((container_idx, start_idx)) = block_stack.pop() {
                // Total bytes between start_idx and now
                let mut size = 0u64;
                for a in &actions[start_idx..] {
                    size += a.bytes_len(charset);
                }
                actions[container_idx].set_container_size(0, size);
            }
            continue;
        }

        // Tokenize instruction and arguments
        let (name, args_str) = match line.split_once(|c: char| c.is_whitespace()) {
            Some((n, rest)) => (n.trim(), rest.trim()),
            None => (line, ""),
        };

        let Some(code) = opcode_by_name(name) else {
            return Err(Error::Other(format!("unknown action instruction: {name}")));
        };

        if code < 0x80 {
            // Simple action
            actions.push(Action::new(code, Op::Simple));
            continue;
        }

        match code {
            0x00 => actions.push(Action::new(0, Op::End)),
            0x81 => {
                let frame = args_str.parse().unwrap_or(0);
                actions.push(Action::new(0x81, Op::GotoFrame(frame)));
            }
            0x83 => {
                let parts = parse_string_list(args_str);
                let url = parts.get(0).cloned().unwrap_or_default();
                let target = parts.get(1).cloned().unwrap_or_default();
                actions.push(Action::new(0x83, Op::GetUrl { url, target }));
            }
            0x87 => {
                let reg = args_str.parse().unwrap_or(0);
                actions.push(Action::new(0x87, Op::StoreRegister(reg)));
            }
            0x88 => {
                let pool = parse_string_list(args_str);
                actions.push(Action::new(0x88, Op::ConstantPool(pool)));
            }
            0x8b => {
                let target = unescape_string(args_str.trim().trim_matches('"'));
                actions.push(Action::new(0x8b, Op::SetTarget(target)));
            }
            0x8c => {
                let label = unescape_string(args_str.trim().trim_matches('"'));
                actions.push(Action::new(0x8c, Op::GoToLabel(label)));
            }
            0x96 => {
                let values = parse_push_values(args_str)?;
                actions.push(Action::new(0x96, Op::Push { values, pool: None }));
            }
            0x99 => {
                let target_label = args_str.trim().to_string();
                let idx = actions.len();
                pending_jumps.push((idx, target_label));
                actions.push(Action::new(0x99, Op::Jump { offset: 0, deobfuscate: false }));
            }
            0x9d => {
                let target_label = args_str.trim().to_string();
                let idx = actions.len();
                pending_jumps.push((idx, target_label));
                actions.push(Action::new(0x9d, Op::If { offset: 0 }));
            }
            0x9b => {
                let (fn_name, params) = parse_define_func(args_str);
                let idx = actions.len();
                actions.push(Action::new(0x9b, Op::DefineFunction { name: fn_name, params, code_size: 0 }));
                block_stack.push((idx, idx + 1));
            }
            0x94 => {
                let idx = actions.len();
                actions.push(Action::new(0x94, Op::With { code_size: 0 }));
                block_stack.push((idx, idx + 1));
            }
            0x8e => {
                let f2 = parse_define_func2(args_str);
                let idx = actions.len();
                actions.push(Action::new(0x8e, Op::DefineFunction2(Box::new(f2))));
                block_stack.push((idx, idx + 1));
            }
            _ => {
                actions.push(Action::new(code, Op::Simple));
            }
        }
    }

    // Assign addresses and update lengths
    let mut address = 0u64;
    for a in &mut actions {
        a.address = address;
        a.length = a.content_len(charset);
        address += a.total_len();
    }
    let total_end_address = address;

    // Resolve jumps
    for (jump_idx, target_label) in pending_jumps {
        let target_addr = if let Some(&target_idx) = labels.get(&target_label) {
            if target_idx < actions.len() {
                actions[target_idx].address
            } else {
                total_end_address
            }
        } else if let Some(hex) = target_label.strip_prefix("loc") {
            u64::from_str_radix(hex, 16).unwrap_or(total_end_address)
        } else {
            total_end_address
        };

        let jump_action = &actions[jump_idx];
        let base = jump_action.address as i64 + 5;
        let offset = (target_addr as i64 - base) as i32;
        actions[jump_idx].set_offset(offset);
    }

    Ok(ActionList { actions, charset, version })
}

fn unescape_string(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_string_list(args_str: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_str = false;
    let mut current = String::new();
    let mut escape = false;

    for c in args_str.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }
        if c == '\\' {
            current.push(c);
            escape = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            if !in_str {
                out.push(unescape_string(&current));
                current.clear();
            }
            continue;
        }
        if in_str {
            current.push(c);
        }
    }
    out
}

fn parse_push_values(args_str: &str) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    let tokens = split_args(args_str);

    for token in tokens {
        let t = token.trim();
        if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            values.push(Value::Str(unescape_string(&t[1..t.len() - 1])));
        } else if t == "null" {
            values.push(Value::Null);
        } else if t == "undefined" {
            values.push(Value::Undefined);
        } else if t == "true" {
            values.push(Value::Bool(true));
        } else if t == "false" {
            values.push(Value::Bool(false));
        } else if let Some(r) = t.strip_prefix("register") {
            let reg: u8 = r.parse().map_err(|e| Error::Other(format!("bad register {t}: {e}")))?;
            values.push(Value::Register(reg));
        } else if let Some(c) = t.strip_prefix("constant") {
            let idx: u16 = c.parse().map_err(|e| Error::Other(format!("bad constant {t}: {e}")))?;
            values.push(Value::Constant(idx));
        } else if t.ends_with('f') {
            let f: f32 = t[..t.len() - 1].parse().map_err(|e| Error::Other(format!("bad float {t}: {e}")))?;
            values.push(Value::Float(f));
        } else if t.contains('.') {
            let d: f64 = t.parse().map_err(|e| Error::Other(format!("bad double {t}: {e}")))?;
            values.push(Value::Double(d));
        } else if let Ok(i) = t.parse::<i64>() {
            values.push(Value::Int(i));
        } else {
            // Treat as string constant
            values.push(Value::Str(t.to_string()));
        }
    }
    Ok(values)
}

fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut escape = false;

    for c in s.chars() {
        if escape {
            cur.push(c);
            escape = false;
            continue;
        }
        if c == '\\' {
            cur.push(c);
            escape = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            cur.push(c);
            continue;
        }
        if c == ',' && !in_str {
            out.push(cur.trim().to_string());
            cur.clear();
            continue;
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn parse_define_func(s: &str) -> (String, Vec<String>) {
    let s = s.trim_end_matches('{').trim();
    let strings = parse_string_list(s);
    let name = strings.get(0).cloned().unwrap_or_default();
    let params = if strings.len() > 1 { strings[1..].to_vec() } else { vec![] };
    (name, params)
}

fn parse_define_func2(s: &str) -> DefineFunction2 {
    let s = s.trim_end_matches('{').trim();
    let tokens = split_args(s);
    let name = tokens.get(0).map(|s| unescape_string(s.trim_matches('"'))).unwrap_or_default();
    let reg_count = tokens.get(2).and_then(|t| t.parse().ok()).unwrap_or(0);
    DefineFunction2 {
        name,
        params: vec![],
        register_count: reg_count,
        preload_parent: false,
        preload_root: false,
        suppress_super: false,
        preload_super: false,
        suppress_arguments: false,
        preload_arguments: false,
        suppress_this: false,
        preload_this: false,
        preload_global: false,
        code_size: 0,
    }
}
