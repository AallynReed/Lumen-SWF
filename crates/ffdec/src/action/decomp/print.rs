//! AS1/2 items as source text.

use std::collections::HashSet;
use std::rc::Rc;

use crate::abc::decomp::ctx::LocalData;
use crate::abc::decomp::item::*;
use crate::abc::decomp::names::escape_oidentifier;
use crate::abc::decomp::print::{append_block, append_to_generic, graph_to_string, to_string_semicoloned, ts};
use crate::abc::decomp::props::{needs_new_line, prec_of};
use crate::abc::decomp::writer::{LoopType, Writer};
use crate::abc::names::escape_as;

use super::items::*;

type LD<'x, 'c, 'a> = &'x LocalData<'c, 'a>;

pub const PROPERTY_NAMES: [&str; 22] = [
    "_X", "_Y", "_xscale", "_yscale", "_currentframe", "_totalframes", "_alpha", "_visible", "_width", "_height", "_rotation", "_target",
    "_framesloaded", "_name", "_droptarget", "_url", "_highquality", "_focusrect", "_soundbuftime", "_quality", "_xmouse", "_ymouse",
];

const RESERVED_AS2: &[&str] = &[
    "and", "break", "case", "catch", "class", "continue", "default", "delete", "do", "dynamic", "each", "else", "eq", "extends", "false",
    "finally", "for", "function", "ge", "get", "gt", "if", "ifFrameLoaded", "implements", "import", "in", "instanceof", "interface",
    "intrinsic", "le", "ne", "new", "not", "null", "on", "onClipEvent", "or", "private", "public", "return", "set", "static", "switch",
    "tellTarget", "throw", "true", "try", "typeof", "undefined", "var", "void", "while", "with",
];

fn ident_first(c: char) -> bool {
    c == '_' || c == '$' || c.is_alphabetic()
}

fn ident_next(c: char) -> bool {
    ident_first(c) || c.is_numeric() || matches!(c, '\u{203f}' | '\u{2040}')
}

/// `IdentifiersDeobfuscation.isValidName(false, s, exceptions)`.
pub fn is_valid_name(s: &str, exceptions: &[&str]) -> bool {
    if exceptions.contains(&s) {
        return true;
    }
    if RESERVED_AS2.contains(&s.trim()) {
        return false;
    }
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if ident_first(c) => cs.all(ident_next),
        _ => false,
    }
}

/// `IdentifiersDeobfuscation.printIdentifier(swf, used, false, s)`.
pub fn print_identifier(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    if s.starts_with('\u{a7}') && s.ends_with('\u{a7}') {
        return s.to_string();
    }
    if is_valid_name(s, &[]) {
        return s.to_string();
    }
    obfuscated(s)
}

/// `appendObfuscatedIdentifier`.
pub fn obfuscated(s: &str) -> String {
    format!("\u{a7}{}\u{a7}", escape_oidentifier(s))
}

/// `As2ClassMemberScope`.
#[derive(Debug, Clone)]
pub struct Scope {
    class_path: Option<Vec<String>>,
    members: HashSet<String>,
    shadowed: HashSet<String>,
}

impl Scope {
    pub fn new(class_name: &It, members: HashSet<String>) -> Scope {
        Scope { class_path: scope_path(class_name), members, shadowed: HashSet::new() }
    }

    fn for_function(&self, f: &AFunction) -> Scope {
        let mut names = self.shadowed.clone();
        names.extend(f.params.iter().cloned());
        names.extend(f.reg_names.iter().map(|(_, n)| n.clone()));
        fn visit(it: &It, names: &mut HashSet<String>) -> bool {
            match &it.borrow().k {
                K::AFunction(g) => {
                    names.insert(g.name.clone());
                    false
                }
                K::ADefineLocal { name } => {
                    if let Some(s) = direct_string(name) {
                        names.insert(s);
                    }
                    true
                }
                _ => true,
            }
        }
        for a in f.actions.borrow().iter() {
            if visit(a, &mut names) {
                visit_recursively(a, &mut |i| visit(i, &mut names));
            }
        }
        Scope { class_path: self.class_path.clone(), members: self.members.clone(), shadowed: names }
    }

    fn with_shadowed(&self, name: String) -> Scope {
        let mut s = self.clone();
        s.shadowed.insert(name);
        s
    }
}

fn scope_path(item: &It) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let mut cur = item.clone();
    loop {
        let next = match &cur.borrow().k {
            K::AGetMember { obj, member, .. } => {
                result.insert(0, direct_string(member)?);
                obj.clone()
            }
            _ => break,
        };
        cur = next;
    }
    let root = match &cur.borrow().k {
        K::AGetVar { name, .. } => direct_string(name)?,
        _ => return None,
    };
    if root != "_global" {
        result.insert(0, root);
    }
    Some(result)
}

/// `As2ClassMemberScope.canShorten`.
fn can_shorten(obj: Option<&It>, member: &It, ld: LD) -> bool {
    let scope = ld.as2_scope.borrow();
    let Some(scope) = scope.as_ref() else { return false };
    let Some(name) = direct_string(member) else { return false };
    let Some(cp) = &scope.class_path else { return false };
    let Some(obj) = obj else { return false };
    is_valid_name(&name, &[]) && scope.members.contains(&name) && !scope.shadowed.contains(&name) && !cp.is_empty() && !scope.shadowed.contains(&cp[0]) && scope_path(obj).as_ref() == Some(cp)
}

fn with_scope<T>(ld: LD, scope: Option<Rc<Scope>>, f: impl FnOnce() -> T) -> T {
    let prev = ld.as2_scope.replace(scope);
    let r = f();
    *ld.as2_scope.borrow_mut() = prev;
    r
}

/// `RegisterNumber.translate()`.
pub fn reg_translate(num: i32, name: &Option<String>, ld: LD) -> String {
    match name {
        Some(n) if !n.is_empty() => n.clone(),
        _ => ld.ctx.cfg.reg_name(num),
    }
}

/// `EcmaScript.toString` of a number.
pub fn number(v: f64) -> String {
    if v.fract() == 0.0 && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
        return format!("{}", v as i32);
    }
    crate::abc::names::ecma_number(v)
}

fn aval_no_quotes(v: &AVal, ld: LD) -> String {
    match v {
        AVal::Double(d) => number(*d),
        AVal::Float(f) => number(*f as f64),
        AVal::Str(s) => s.clone(),
        AVal::Register { num, name } => print_identifier(&reg_translate(*num, name, ld)),
        AVal::Int(i) => i.to_string(),
        AVal::Null => "null".into(),
        AVal::Undefined => "undefined".into(),
    }
}

fn aval_source(v: &AVal, ld: LD) -> String {
    match v {
        AVal::Str(s) => format!("\"{}\"", escape_as(s)),
        AVal::Register { num, name } => print_identifier(&reg_translate(*num, name, ld)),
        AVal::Int(i) => number(*i as f64),
        AVal::Double(d) => number(*d),
        AVal::Float(f) => number(*f as f64),
        AVal::Null => "null".into(),
        AVal::Undefined => "undefined".into(),
    }
}

/// `toStringNoQuotes(localData)`.
pub fn no_quotes(it: &It, ld: LD) -> String {
    let mut w = Writer::new();
    append_no_quotes(it, &mut w, ld);
    w.finish()
}

fn append_no_quotes(it: &It, w: &mut Writer, ld: LD) {
    let k = it.borrow().k.clone();
    match k {
        K::ADirect { v, .. } => {
            w.append(&aval_no_quotes(&v, ld));
        }
        K::AUnresolved(i) => {
            w.append(&format!("\u{a7}\u{a7}constant({i})"));
        }
        _ => ts(it, w, ld),
    }
}

/// `toStringNoQuotes(LocalData.empty)` of a literal name.
pub fn plain_no_quotes(it: &It) -> String {
    match &it.borrow().k {
        K::ADirect { v: AVal::Str(s), .. } => s.clone(),
        K::ADirect { v: AVal::Int(i), .. } => i.to_string(),
        K::ADirect { v: AVal::Double(d), .. } => number(*d),
        K::ADirect { v: AVal::Float(f), .. } => number(*f as f64),
        _ => String::new(),
    }
}

/// `stripQuotes`.
fn strip_quotes(it: &It, w: &mut Writer, ld: LD) {
    if let Some(s) = direct_string(it) {
        w.append(&s);
        return;
    }
    ts(it, w, ld);
}

fn paren_ts(w: &mut Writer, ld: LD, it: &It, cond: bool) {
    if cond {
        w.append("(");
        ts(it, w, ld);
        w.append(")");
    } else {
        ts(it, w, ld);
    }
}

/// `toStringNL`.
fn ts_nl(it: &It, w: &mut Writer, ld: LD) {
    ts(it, w, ld);
    if needs_new_line(it) {
        w.new_line();
    }
}

fn call_args(w: &mut Writer, ld: LD, args: &List, nl: bool) {
    w.append("(");
    for (i, a) in args.borrow().iter().enumerate() {
        if i > 0 {
            if nl {
                w.allow_wrap_here();
            }
            w.append(",");
        }
        if nl {
            ts_nl(a, w, ld);
        } else {
            ts(a, w, ld);
        }
    }
    w.append(")");
}

fn hex_operand(w: &mut Writer, ld: LD, operand: &It) {
    let num = match &operand.borrow().k {
        K::ADirect { v: AVal::Int(_) | AVal::Double(_) | AVal::Float(_), .. } => Some(as_long(operand)),
        _ => None,
    };
    if let Some(v) = num {
        if v > 9 {
            let mut h = format!("{v:X}");
            if h.len() % 2 == 1 {
                h.insert(0, '0');
            }
            w.append(&format!("0x{h}"));
            return;
        }
    }
    ts(operand, w, ld);
}

fn bin_default(w: &mut Writer, ld: LD, op: Bin, p: u8, l: &It, r: &It) {
    let bitwise = matches!(op, Bin::BitAnd | Bin::BitOr | Bin::BitXor);
    let operand = |w: &mut Writer, x: &It| if bitwise { hex_operand(w, ld, x) } else { ts(x, w, ld) };
    let lp = prec_of(l);
    if lp > p && lp != prec::NOPRECEDENCE {
        w.append("(");
        operand(w, l);
        w.append(")");
    } else {
        operand(w, l);
    }
    w.append(" ").append(op.op()).append(" ");
    let rp = prec_of(r);
    if rp >= p && rp != prec::NOPRECEDENCE {
        w.append("(");
        operand(w, r);
        w.append(")");
    } else {
        operand(w, r);
    }
}

fn bin_right_paren(w: &mut Writer, ld: LD, op: Bin, p: u8, l: &It, r: &It) {
    paren_ts(w, ld, l, prec_of(l) > p);
    w.append(" ").append(op.op()).append(" ");
    w.append("(");
    ts(r, w, ld);
    w.append(")");
}

fn is_zero(it: &It) -> bool {
    match &it.borrow().k {
        K::ADirect { v: AVal::Float(f), .. } => *f == 0.0,
        K::ADirect { v: AVal::Double(d), .. } => *d == 0.0,
        K::ADirect { v: AVal::Int(i), .. } => *i == 0,
        K::ADirect { v: AVal::Str(s), .. } => s == "0",
        _ => false,
    }
}

fn append_bin(w: &mut Writer, ld: LD, op: Bin, p: u8, l: &It, r: &It) {
    match op {
        Bin::Add if prec_of(r) >= p => bin_right_paren(w, ld, op, p, l, r),
        Bin::Sub if is_zero(l) => {
            w.append(op.op()).append(" ");
            let rp = prec_of(r);
            paren_ts(w, ld, r, rp >= p && rp != prec::NOPRECEDENCE);
        }
        Bin::Sub if prec_of(r) >= p => bin_right_paren(w, ld, op, p, l, r),
        Bin::BitXor if matches!(&r.borrow().k, K::ADirect { v: AVal::Double(d), .. } if *d == 4294967295.0) => {
            w.append("~");
            paren_ts(w, ld, l, prec_of(l) > prec::UNARY);
        }
        _ => bin_default(w, ld, op, p, l, r),
    }
}

fn set_tail(w: &mut Writer, ld: LD, compound_op: &Option<String>, compound_value: &Option<It>, value: &Option<It>) {
    if let Some(op) = compound_op {
        w.append(" ").append(op).append("= ");
        ts(compound_value.as_ref().expect("compound value"), w, ld);
        return;
    }
    w.append(" = ");
    if let Some(v) = value {
        ts(v, w, ld);
    }
}

/// Whether `append_to` should use this module for the item.
pub fn handles(it: &It) -> bool {
    let b = it.borrow();
    (b.as2 && matches!(b.k, K::Bin { .. })) || is_action_kind(&b.k)
}

pub fn append(it: &It, w: &mut Writer, ld: LD) {
    let (k, p, value) = {
        let b = it.borrow();
        (b.k.clone(), b.prec, b.value.clone())
    };
    let val = || value.clone().expect("item value");
    match k {
        K::Bin { op, l, r } => append_bin(w, ld, op, p, &l, &r),
        K::ADirect { v, .. } => {
            w.append(&aval_source(&v, ld));
        }
        K::AUnresolved(i) => {
            w.append(&format!("\u{a7}\u{a7}constant({i})"));
        }
        K::AGetVar { name, print_obf, .. } => {
            let s = direct_string(&name);
            if is_direct(&name) && print_obf {
                let mut sw = Writer::new();
                strip_quotes(&name, &mut sw, ld);
                w.append(&print_identifier(&sw.finish()));
            } else if s.as_ref().is_some_and(|s| print_obf || is_valid_name(s, &["this", "super"])) {
                strip_quotes(&name, w, ld);
            } else {
                w.append("eval(");
                ts(&name, w, ld);
                w.append(")");
            }
        }
        K::ASetVar { name, compound_value, compound_op } => {
            let valid = direct_string(&name).is_some_and(|s| is_valid_name(&s, &["this", "super"]));
            let result_needed = value.as_ref().is_some_and(|v| matches!(v.borrow().k, K::AStoreReg { temporary: true, .. }));
            if valid || result_needed {
                w.append(&print_identifier(&no_quotes(&name, ld)));
                set_tail(w, ld, &compound_op, &compound_value, &value);
            } else {
                w.append("set(");
                ts(&name, w, ld);
                w.append(",");
                ts(&val(), w, ld);
                w.append(")");
            }
        }
        K::AGetMember { obj, member, print_obf, .. } => {
            if can_shorten(Some(&obj), &member, ld) {
                strip_quotes(&member, w, ld);
                return;
            }
            ts(&obj, w, ld);
            if is_direct(&member) && print_obf {
                w.allow_wrap_here().append(".");
                let mut sw = Writer::new();
                strip_quotes(&member, &mut sw, ld);
                w.append(&print_identifier(&sw.finish()));
                return;
            }
            let valid = direct_string(&member).is_some_and(|s| print_obf || is_valid_name(&s, &[]));
            if !valid {
                w.append("[");
                ts(&member, w, ld);
                w.append("]");
                return;
            }
            w.allow_wrap_here().append(".");
            strip_quotes(&member, w, ld);
        }
        K::ASetMember { obj, member, compound_value, compound_op, .. } => {
            if can_shorten(Some(&obj), &member, ld) {
                strip_quotes(&member, w, ld);
            } else {
                paren_ts(w, ld, &obj, matches!(obj.borrow().k, K::ADirect { v: AVal::Int(_), .. }));
                let valid = direct_string(&member).is_some_and(|s| is_valid_name(&s, &[]));
                if !valid {
                    w.append("[");
                    ts(&member, w, ld);
                    w.append("]");
                } else {
                    w.allow_wrap_here().append(".");
                    strip_quotes(&member, w, ld);
                }
            }
            set_tail(w, ld, &compound_op, &compound_value, &value);
        }
        K::AGetProp { target, index, .. } => {
            let name = PROPERTY_NAMES.get(index as usize).copied().unwrap_or("");
            if is_empty_string(&target) {
                w.append(name);
                return;
            }
            w.append("getProperty(");
            ts(&target, w, ld);
            w.append(", ").append(name).append(")");
        }
        K::ASetProp { target, index } => {
            let name = PROPERTY_NAMES.get(index as usize).copied().unwrap_or("");
            if is_empty_string(&target) {
                w.append(name).append(" = ");
                ts(&val(), w, ld);
                return;
            }
            w.append("setProperty(");
            append_to_generic_or(&target, w, ld);
            w.append(", ").append(name).append(", ");
            append_to_generic_or(&val(), w, ld);
            w.append(")");
        }
        K::ADefineLocal { name } => {
            w.append("var ");
            let invalid = direct_string(&name).is_some_and(|s| !is_valid_name(&s, &["this", "super", "true", "false", "NaN", "null", "newline", "Infinity", "undefined", "get", "set", "each"]));
            if invalid {
                w.append(&obfuscated(&no_quotes(&name, ld)));
            } else {
                strip_quotes(&name, w, ld);
            }
            match &value {
                None => {}
                Some(v) if matches!(v.borrow().k, K::ADirect { v: AVal::Undefined, .. }) => {}
                Some(v) => {
                    w.append(" = ");
                    ts(v, w, ld);
                }
            }
        }
        K::ADelete { obj, prop } => {
            w.append("delete ");
            if can_shorten(obj.as_ref(), &prop, ld) {
                append_no_quotes(&prop, w, ld);
                return;
            }
            if let Some(o) = &obj {
                append_no_quotes(o, w, ld);
                if direct_string(&prop).is_some_and(|s| is_valid_name(&s, &[])) {
                    w.append(".");
                    append_no_quotes(&prop, w, ld);
                } else {
                    w.append("[");
                    ts(&prop, w, ld);
                    w.append("]");
                }
                return;
            }
            let paren = prec_of(&prop) > p;
            if paren {
                w.append("(");
            }
            if is_valid_name(&no_quotes(&prop, ld), &[]) {
                append_no_quotes(&prop, w, ld);
            } else {
                ts(&prop, w, ld);
            }
            if paren {
                w.append(")");
            }
        }
        K::AStoreReg { reg, name, define, temporary, compound_value, compound_op } => {
            if temporary {
                ts(&val(), w, ld);
                return;
            }
            if define {
                w.append("var ");
            }
            w.append(&print_identifier(&reg_translate(reg, &name, ld)));
            if let Some(op) = &compound_op {
                w.append(" ").append(op).append("= ");
                ts(compound_value.as_ref().expect("compound value"), w, ld);
                return;
            }
            if let Some(v) = &value {
                if !matches!(v.borrow().k, K::ADirect { v: AVal::Undefined, .. }) {
                    w.append(" = ");
                    ts(v, w, ld);
                }
            }
        }
        K::ACallFunction { name, args, .. } => {
            if is_direct(&name) {
                let nq = no_quotes(&name, ld);
                if !is_valid_name(&nq, &[]) {
                    ts(&name, w, ld);
                } else {
                    append_no_quotes(&name, w, ld);
                }
            } else {
                paren_ts(w, ld, &name, prec_of(&name) > p);
            }
            call_args(w, ld, &args, true);
        }
        K::ACallMethod { obj, name, args } => {
            if is_direct(&name) {
                let blank = match &name.borrow().k {
                    K::ADirect { v: AVal::Undefined, .. } => true,
                    K::ADirect { v: AVal::Str(s), .. } => s.is_empty(),
                    _ => false,
                };
                if !blank {
                    if can_shorten(Some(&obj), &name, ld) {
                        append_no_quotes(&name, w, ld);
                    } else {
                        let paren = prec_of(&obj) > p || matches!(obj.borrow().k, K::ADirect { v: AVal::Int(_), .. });
                        paren_ts(w, ld, &obj, paren);
                        let is_reg = matches!(name.borrow().k, K::ADirect { v: AVal::Register { .. }, .. });
                        if !is_reg && is_valid_name(&no_quotes(&name, ld), &[]) {
                            w.allow_wrap_here().append(".");
                            append_no_quotes(&name, w, ld);
                        } else {
                            w.append("[");
                            ts(&name, w, ld);
                            w.append("]");
                        }
                    }
                } else {
                    ts(&obj, w, ld);
                }
            } else {
                paren_ts(w, ld, &obj, prec_of(&obj) > p);
                w.append("[");
                ts(&name, w, ld);
                w.append("]");
            }
            call_args(w, ld, &args, true);
        }
        K::ANewMethod { obj, name, args } => {
            let blank = match &name.borrow().k {
                K::ADirect { v: AVal::Undefined, .. } => true,
                K::ADirect { v: AVal::Str(s), .. } => s.is_empty(),
                _ => false,
            };
            w.append("new ");
            if can_shorten(Some(&obj), &name, ld) {
                append_no_quotes(&name, w, ld);
            } else {
                ts(&obj, w, ld);
                if !blank {
                    let nk = name.borrow().k.clone();
                    match nk {
                        K::ADirect { v: AVal::Undefined, .. } => {}
                        K::ADirect { v: AVal::Str(s), .. } if is_valid_name(&s, &[]) => {
                            w.append(".");
                            append_no_quotes(&name, w, ld);
                        }
                        _ => {
                            w.append("[");
                            ts(&name, w, ld);
                            w.append("]");
                        }
                    }
                }
            }
            call_args(w, ld, &args, false);
        }
        K::ANewObject { name, args } => {
            w.append("new ");
            w.append(&print_identifier(&no_quotes(&name, ld)));
            call_args(w, ld, &args, false);
        }
        K::AInitArray { values } => {
            w.append("[");
            for (i, v) in values.borrow().iter().enumerate() {
                if i > 0 {
                    w.append(",");
                }
                ts(v, w, ld);
            }
            w.append("]");
        }
        K::AInitObject { names, values } => {
            w.append("{");
            let (names, values) = (names.borrow().clone(), values.borrow().clone());
            for i in (0..values.len()).rev() {
                if i < values.len() - 1 {
                    w.append(",");
                }
                let simple = matches!(&names[i].borrow().k, K::ADirect { v, .. } if !matches!(v, AVal::Register { .. }));
                if simple {
                    w.append(&print_identifier(&no_quotes(&names[i], ld)));
                } else {
                    w.append("(");
                    append_to_generic_or(&names[i], w, ld);
                    w.append(")");
                }
                w.append(":");
                paren_ts(w, ld, &values[i], matches!(values[i].borrow().k, K::Ternar { .. }));
            }
            w.append("}");
        }
        K::AInc { obj } | K::ADec { obj } => {
            paren_ts(w, ld, &obj, prec_of(&obj) > p);
            w.append(if matches!(it.borrow().k, K::AInc { .. }) { " + 1" } else { " - 1" });
        }
        K::APostInc { obj } => {
            ts(&obj, w, ld);
            w.append("++");
        }
        K::APostDec { obj } => {
            ts(&obj, w, ld);
            w.append("--");
        }
        K::AEnumerate { obj } => {
            w.append("\u{a7}\u{a7}enumerate(");
            ts(&obj, w, ld);
            w.append(")");
        }
        K::AEnumerated => {
            w.append("\u{a7}\u{a7}enumeration()");
        }
        K::AEnumAssign => {
            w.append("\u{a7}\u{a7}enum_assign()");
        }
        K::AFunction(f) => append_function(&f, w, ld),
        K::AReturn => match &value {
            None => {
                w.append("return");
            }
            Some(v) => {
                w.append("return ");
                ts(v, w, ld);
            }
        },
        K::AEval => {
            w.append("eval(");
            ts(&val(), w, ld);
            w.append(")");
        }
        K::ATypeOf => {
            w.append("typeof ");
            let v = val();
            paren_ts(w, ld, &v, prec_of(&v) > p);
        }
        K::ATellTarget { target, cmds } => {
            w.append("tellTarget(");
            ts(&target, w, ld);
            w.append(")").new_line();
            w.append("{").new_line();
            w.indent();
            for c in cmds.borrow().clone() {
                to_string_semicoloned(&c, w, ld);
                w.new_line();
            }
            w.unindent();
            w.append("}");
        }
        K::ASetTarget(t) => {
            w.append("tellTarget(\"").append(&escape_as(&t)).append("\")");
        }
        K::ASetTarget2 { target } => {
            w.append("tellTarget(");
            ts(&target, w, ld);
            w.append(")");
        }
        K::AWith { scope, items } => {
            w.append("with(");
            ts(&scope, w, ld);
            w.append(")");
            with_scope(ld, None, || append_block(w, ld, &items));
        }
        K::ATry { try_cmds, names, types, catch_cmds, finally } => {
            w.append("try");
            append_block(w, ld, &try_cmds);
            let names = names.borrow().clone();
            for (e, n) in names.iter().enumerate() {
                w.new_line();
                w.append("catch(");
                append_no_quotes(n, w, ld);
                if let Some(Some(t)) = types.get(e) {
                    w.append(":");
                    append_no_quotes(t, w, ld);
                }
                w.append(")");
                let scope = ld.as2_scope.borrow().as_ref().map(|s| Rc::new(s.with_shadowed(no_quotes(n, ld))));
                let cmds = catch_cmds[e].clone();
                with_scope(ld, scope, || append_block(w, ld, &cmds));
            }
            if names.is_empty() || !finally.borrow().is_empty() {
                w.new_line();
                w.append("finally");
                append_block(w, ld, &finally);
            }
        }
        K::AForIn { loop_id, var, obj, cmds, label_used } => {
            w.start_loop(loop_id, LoopType::Loop);
            if label_used {
                w.append("loop").append_int(loop_id).append(":").new_line();
            }
            w.append("for(");
            let vk = var.borrow().k.clone();
            if is_set_type_action(&var) {
                if matches!(vk, K::AStoreReg { define: true, .. }) {
                    w.append("var ");
                }
                let vn = set_type_object(&var);
                if let K::AGetVar { print_obf, .. } = &mut vn.borrow_mut().k {
                    *print_obf = true;
                }
                strip_quotes(&vn, w, ld);
            }
            w.append(" in ");
            paren_ts(w, ld, &obj, prec_of(&obj) > prec::PRIMARY);
            w.append(")").start_block();
            for c in cmds.borrow().clone() {
                to_string_semicoloned(&c, w, ld);
                w.new_line();
            }
            w.end_block();
            if let Some(l) = w.end_loop(loop_id) {
                if let K::AForIn { label_used, .. } = &mut it.borrow_mut().k {
                    *label_used = l.used;
                }
            }
        }
        K::AIfFrameLoaded { frame, actions } => {
            w.append("ifFrameLoaded(");
            ts(&frame, w, ld);
            w.append(")").start_block();
            graph_to_string(&actions.borrow().clone(), w, ld);
            w.end_block();
        }
        K::AClass(c) => append_class(&c, w, ld),
        K::AInterface { name, supers } => {
            w.append("interface ");
            append_no_quotes(&name, w, ld);
            let supers = supers.borrow().clone();
            if !supers.is_empty() {
                w.append(" extends ");
            }
            for (i, s) in supers.iter().enumerate() {
                if i > 0 {
                    w.append(", ");
                }
                append_no_quotes(&without_global(s), w, ld);
            }
            w.start_block();
            w.end_block();
        }
        K::ATempReg { .. } => ts(&val(), w, ld),
        K::ATempRegMark { .. } => {}
        K::AStrict(mode) => {
            if mode == 1 {
                w.append("#strict");
            } else {
                w.append(&format!("\u{a7}\u{a7}strict({mode})"));
            }
        }
        K::AGotoFrame(f) => {
            w.append("gotoAndStop(").append_int(f as i64 + 1).append(")");
        }
        K::AExtends { sub, sup } => {
            ts(&sub, w, ld);
            w.append(" extends ");
            strip_quotes(&sup, w, ld);
        }
        K::AImplements { sub, supers } => {
            ts(&sub, w, ld);
            w.append(" implements ");
            for (i, s) in supers.borrow().iter().enumerate() {
                if i > 0 {
                    w.append(",");
                }
                ts(s, w, ld);
            }
        }
        K::ACastOp { ctor, obj } => {
            strip_quotes(&ctor, w, ld);
            w.append("(");
            ts(&obj, w, ld);
            w.append(")");
        }
        K::AFn { name, args, sep, suffix, .. } => {
            w.append(name).append("(");
            for (i, a) in args.borrow().iter().enumerate() {
                if i > 0 {
                    w.append(sep);
                }
                ts(a, w, ld);
            }
            w.append(&suffix).append(")");
        }
        K::AUnsupported(s) => {
            w.append("// Unsupported by decompiler:").append(&s);
        }
        _ => append_to_generic(it, w, ld),
    }
}

/// `appendTo` (no `toString` wrapper) of an operand.
fn append_to_generic_or(it: &It, w: &mut Writer, ld: LD) {
    crate::abc::decomp::print::append_to(it, w, ld);
}

fn trim_accessor(name: &str, f: &AFunction) -> String {
    if (f.getter && name.starts_with("__get__")) || (f.setter && name.starts_with("__set__")) {
        return name[7..].to_string();
    }
    name.to_string()
}

fn append_function(f: &AFunction, w: &mut Writer, ld: LD) {
    w.append("function");
    if f.getter {
        w.append(" get");
    }
    if f.setter {
        w.append(" set");
    }
    let named = match &f.calculated_name {
        Some(c) => Some(trim_accessor(&no_quotes(c, ld), f)),
        None if !f.name.is_empty() => Some(trim_accessor(&f.name, f)),
        None => None,
    };
    if let Some(n) = named {
        w.append(" ");
        if !is_valid_name(&n, &[]) {
            w.append(&obfuscated(&n));
        } else {
            w.append(&n);
        }
    }
    w.append("(");
    for (p, pname) in f.params.iter().enumerate() {
        if p > 0 {
            w.append(", ");
        }
        let pname = if pname.is_empty() {
            let reg = match &f.param_registers {
                Some(r) => r[p],
                None => f.reg_start + p as i32,
            };
            reg_translate(reg, &None, ld)
        } else {
            pname.clone()
        };
        if !is_valid_name(&pname, &[]) {
            w.append(&obfuscated(&pname));
        } else {
            w.append(&pname);
        }
    }
    w.append(")").start_block();
    let scope = ld.as2_scope.borrow().as_ref().map(|s| Rc::new(s.for_function(f)));
    with_scope(ld, scope, || graph_to_string(&f.actions.borrow().clone(), w, ld));
    w.end_block();
}

fn make_print_obfuscated(item: &It) {
    let mut cur = item.clone();
    loop {
        let next = {
            let mut b = cur.borrow_mut();
            match &mut b.k {
                K::AGetMember { obj, print_obf, .. } => {
                    *print_obf = true;
                    obj.clone()
                }
                K::AGetVar { print_obf, .. } => {
                    *print_obf = true;
                    return;
                }
                _ => return,
            }
        };
        cur = next;
    }
}

fn append_class(c: &AClass, w: &mut Writer, ld: LD) {
    let mut statics = HashSet::new();
    for (i, (k, v)) in c.traits.iter().enumerate() {
        if c.statics[i] {
            let name = no_quotes(k, ld);
            if let Some(v) = v {
                if let K::AFunction(f) = &v.borrow().k {
                    if (f.getter && name.starts_with("__get__")) || (f.setter && name.starts_with("__set__")) {
                        statics.insert(name[7..].to_string());
                    }
                }
            }
            statics.insert(name);
        }
    }
    let scope = Rc::new(Scope::new(&c.name, statics));
    with_scope(ld, Some(scope), || {
        make_print_obfuscated(&c.name);
        if let Some(e) = &c.extends {
            make_print_obfuscated(e);
        }
        for i in c.implements.borrow().iter() {
            make_print_obfuscated(i);
        }
        w.append("class ");
        append_no_quotes(&c.name, w, ld);
        if let Some(e) = &c.extends {
            w.append(" extends ");
            append_no_quotes(e, w, ld);
        }
        let imps = c.implements.borrow().clone();
        if !imps.is_empty() {
            w.append(" implements ");
            for (i, t) in imps.iter().enumerate() {
                if i > 0 {
                    w.append(", ");
                }
                ts(&without_global(t), w, ld);
            }
        }
        w.start_block();
        for pass in 1..=2 {
            for (i, (k, v)) in c.traits.iter().enumerate() {
                let is_fn = v.as_ref().is_some_and(|v| matches!(v.borrow().k, K::AFunction(_)));
                if (pass == 1 && is_fn) || (pass == 2 && !is_fn) {
                    continue;
                }
                if c.statics[i] {
                    w.append("static ");
                }
                if is_fn {
                    ts(v.as_ref().unwrap(), w, ld);
                    w.new_line();
                } else {
                    w.append("var ");
                    w.append(&print_identifier(&no_quotes(k, ld)));
                    if let Some(v) = v {
                        w.append(" = ");
                        ts(v, w, ld);
                    }
                    w.append(";").new_line();
                }
            }
        }
        for v in &c.uninitialized {
            w.append("var ").append(v).append(";").new_line();
        }
        w.end_block();
    });
}
