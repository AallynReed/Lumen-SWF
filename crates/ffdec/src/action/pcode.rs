//! AS1/2 p-code text.

use std::collections::{HashMap, HashSet};

use super::{Action, ActionList, Op, Value};
use crate::abc::names::{ecma_number, escape_as};

pub fn format_address(a: u64) -> String {
    format!("{a:04x}")
}

fn number(v: f64) -> String {
    if v.fract() == 0.0 && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
        return format!("{}", v as i32);
    }
    ecma_number(v)
}

/// P-code text of a push value.
pub fn value_to_string(v: &Value, pool: Option<&[String]>) -> String {
    match v {
        Value::Constant(i) => match pool.and_then(|p| p.get(*i as usize)) {
            Some(s) => format!("\"{}\"", escape_as(s)),
            None => format!("constant{i}"),
        },
        Value::Str(s) => format!("\"{}\"", escape_as(s)),
        Value::Register(n) => format!("register{n}"),
        Value::Float(f) => format!("{}f", number(*f as f64)),
        Value::Double(d) => {
            let s = number(*d);
            if s.bytes().all(|b| b.is_ascii_digit()) && !s.is_empty() { format!("{s}.0") } else { s }
        }
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Undefined => "undefined".into(),
    }
}

/// P-code source of one action.
pub fn action_source(a: &Action) -> String {
    let q = |s: &str| format!("\"{}\"", escape_as(s));
    match &a.op {
        Op::Simple => super::simple_name(a.code).unwrap_or("").to_string(),
        Op::End => "End".into(),
        Op::GotoFrame(f) => format!("GotoFrame {f}"),
        Op::GetUrl { url, target } => format!("GetURL {}, {}", q(url), q(target)),
        Op::WaitForFrame { frame, skip } => format!("WaitForFrame {frame}, {skip}"),
        Op::SetTarget(s) => format!("SetTarget {}", q(s)),
        Op::GoToLabel(s) => format!("GoToLabel {}", q(s)),
        Op::Push { values, pool } => {
            let p = pool.as_deref().map(|p| p.as_slice());
            format!("Push {}", values.iter().map(|v| value_to_string(v, p)).collect::<Vec<_>>().join(", "))
        }
        Op::Jump { .. } => format!("Jump loc{}", format_address(a.target().unwrap())),
        Op::If { .. } => format!("If loc{}", format_address(a.target().unwrap())),
        Op::GetUrl2 { load_variables, load_target, method } => format!("GetURL2 {load_variables}, {load_target}, {method}"),
        Op::GotoFrame2 { scene_bias_flag, play, scene_bias } => {
            let bias = if *scene_bias_flag { format!(", {scene_bias}") } else { String::new() };
            format!("GotoFrame2 {scene_bias_flag}, {play}{bias}")
        }
        Op::WaitForFrame2 { skip } => format!("WaitForFrame2 {skip}"),
        Op::ConstantPool(pool) => {
            let mut s = String::from("ConstantPool");
            for (i, c) in pool.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push(' ');
                s.push_str(&q(c));
            }
            s
        }
        Op::DefineFunction { name, params, .. } => {
            let p: String = params.iter().map(|p| format!(", {} ", q(p))).collect();
            format!("DefineFunction {}, {}{p} {{", q(name), params.len())
        }
        Op::With { .. } => "With {".into(),
        Op::StoreRegister(r) => format!("StoreRegister {r}"),
        Op::DefineFunction2(f) => {
            let p: String = f.params.iter().map(|(r, n)| format!(", {r}, {}", q(n))).collect();
            let head = format!(
                "DefineFunction2 {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}",
                q(&f.name),
                f.params.len(),
                f.register_count,
                f.preload_parent,
                f.preload_root,
                f.suppress_super,
                f.preload_super,
                f.suppress_arguments,
                f.preload_arguments,
                f.suppress_this,
                f.preload_this,
                f.preload_global
            );
            format!("{}{p} {{", head.trim())
        }
        Op::Try(t) => {
            let mut s = String::from("Try ");
            if t.catch_flag {
                if t.catch_in_register {
                    s.push_str(&format!("register{}", t.catch_register));
                } else {
                    s.push_str(&q(t.catch_name.as_deref().unwrap_or("")));
                }
                s.push(' ');
            }
            s.push('{');
            s
        }
        Op::StrictMode(m) => format!("StrictMode {m}"),
        Op::Unknown(d) => {
            let mut s = format!("Unknown_{:02x}", a.code);
            for (i, b) in d.iter().enumerate() {
                s.push_str(if i == 0 { " " } else { ", " });
                s.push_str(&format!("0x{b:02x}"));
            }
            s
        }
    }
}

/// Text between the blocks of a container (`getASMSourceBetween`).
fn between(a: &Action, pos: usize) -> &'static str {
    if let Op::Try(t) = &a.op {
        if pos == 0 {
            if t.catch_flag {
                return "Catch {\n";
            }
            if t.finally_flag {
                return "Finally {\n";
            }
        }
        if pos == 1 && t.catch_flag && t.finally_flag {
            return "Finally {\n";
        }
    }
    ""
}

/// The p-code listing of an action list.
pub fn to_pcode(list: &ActionList) -> String {
    let cs = list.charset;
    let important: HashSet<u64> = list.actions.iter().filter_map(Action::target).collect();
    let mut containers: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut cnt_pos: HashMap<usize, usize> = HashMap::new();
    let mut out = String::new();
    let mut last_push = false;
    let mut offset = 0;
    for (pos, a) in list.actions.iter().enumerate() {
        offset = a.address;
        if !a.ignored {
            if let Some(sizes) = a.container_sizes() {
                cnt_pos.insert(pos, 0);
                let mut addr = a.address + a.bytes_len(cs);
                for s in sizes {
                    addr += s;
                    containers.entry(addr).or_default().push(pos);
                }
            }
        }
        if let Some(cl) = containers.get(&offset) {
            for &c in cl.iter().rev() {
                if last_push {
                    out.push('\n');
                    last_push = false;
                }
                out.push_str("}\n");
                let p = cnt_pos[&c];
                out.push_str(between(&list.actions[c], p));
                cnt_pos.insert(c, p + 1);
            }
        }
        if important.contains(&offset) {
            if last_push {
                out.push('\n');
                last_push = false;
            }
            out.push_str("loc");
            out.push_str(&format_address(offset));
            out.push(':');
        }
        if a.ignored {
            if last_push {
                out.push('\n');
                last_push = false;
            }
            if !matches!(a.op, Op::End) {
                for _ in 0..a.total_len() {
                    out.push_str("Nop\n");
                }
            }
        } else {
            if last_push {
                out.push('\n');
            }
            out.push_str(&action_source(a));
            let is_push = matches!(a.op, Op::Push { .. });
            if !is_push {
                out.push('\n');
            }
            last_push = is_push;
        }
        offset += a.total_len();
    }
    if last_push {
        out.push('\n');
    }
    if let Some(cl) = containers.get(&offset) {
        for &c in cl {
            out.push_str("}\n");
            let p = cnt_pos[&c];
            out.push_str(between(&list.actions[c], p));
            cnt_pos.insert(c, p + 1);
        }
    }
    if important.contains(&offset) {
        out.push_str("loc");
        out.push_str(&format_address(offset));
        out.push_str(":\n");
    }
    out
}
