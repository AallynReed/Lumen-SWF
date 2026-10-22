//! AVM2 p-code in FFDec's text format (`trait method …`, `body`, `code`, `ofs0010:`).

use std::collections::BTreeSet;
use std::fmt::Write;

use super::names::{ecma_number, escape_as, escape_pcode, ns_kind_name};
use super::opcodes::{disassemble, Dialect, Instruction, Operand};
use super::{method_flags, Abc, MethodBody, Trait, TraitData, ValueKind};

const INDENT: &str = "   ";

#[derive(Default)]
struct Out {
    s: String,
    depth: usize,
}

impl Out {
    fn line(&mut self, text: &str) {
        for _ in 0..self.depth {
            self.s.push_str(INDENT);
        }
        self.s.push_str(text);
        self.s.push('\n');
    }

    fn at(&mut self, depth: usize, text: &str) {
        let saved = self.depth;
        self.depth = depth;
        self.line(text);
        self.depth = saved;
    }
}

fn addr(a: i64) -> String {
    format!("ofs{:04x}", a.max(0))
}

fn quoted_or_null(abc: &Abc, i: usize, escape: fn(&str) -> String) -> String {
    match abc.constants.string(i) {
        Some(s) => format!("\"{}\"", escape(s)),
        None => "null".into(),
    }
}

pub fn value_pcode(abc: &Abc, v: &ValueKind) -> String {
    let c = &abc.constants;
    let ns = |i: usize| {
        let n = c.namespaces.get(i).copied().unwrap_or(super::Namespace { kind: 0, name: 0 });
        format!("{}(\"{}\")", ns_kind_name(n.kind), abc.str(n.name))
    };
    match v.kind {
        0x03 => format!("Integer({})", c.ints.get(v.index).copied().unwrap_or(0)),
        0x04 => format!("UInteger({})", c.uints.get(v.index).copied().unwrap_or(0)),
        0x06 => format!("Double({})", ecma_number(c.doubles.get(v.index).copied().unwrap_or(f64::NAN))),
        0x02 if abc.minor >= 17 => format!("Decimal({})", v.index),
        0x02 => format!("Float({})", ecma_number(c.floats.get(v.index).copied().unwrap_or(f32::NAN) as f64)),
        0x01 => format!("Utf8(\"{}\")", escape_pcode(abc.str(v.index))),
        0x0b => "True()".into(),
        0x0a => "False()".into(),
        0x0c => "Null()".into(),
        0x00 => "Undefined()".into(),
        0x08 => format!("Namespace({})", ns(v.index)),
        0x16 => format!("PackageNamespace({})", ns(v.index)),
        0x17 => format!("PackageInternalNs({})", ns(v.index)),
        0x18 => format!("ProtectedNamespace({})", ns(v.index)),
        0x19 => format!("ExplicitNamespace({})", ns(v.index)),
        0x1a => format!("StaticProtectedNs({})", ns(v.index)),
        0x05 => format!("PrivateNamespace({})", ns(v.index)),
        _ => "?".into(),
    }
}

/// One instruction without its address, as FFDec's `toStringNoAddress`.
pub fn instruction_text(abc: &Abc, ins: &Instruction) -> String {
    let c = &abc.constants;
    let mut s = ins.name.to_string();
    for (i, op) in ins.kinds.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let v = ins.operands.get(i).copied().unwrap_or(0);
        let idx = v as usize;
        let lookup = |o: Option<String>| o.unwrap_or_else(|| format!("Unknown({idx})"));
        match op {
            Operand::NamespaceIndex => {
                let _ = write!(s, " {}", if idx == 0 { "null".into() } else { abc.ns_pcode(idx) });
            }
            Operand::MultinameIndex => {
                let _ = write!(s, " {}", if idx == 0 { "null".into() } else { abc.multiname_pcode(idx) });
            }
            Operand::StringIndex => {
                let _ = write!(s, " {}", quoted_or_null(abc, idx, escape_pcode));
            }
            Operand::IntIndex => {
                let _ = write!(s, " {}", if idx == 0 { "null".into() } else { lookup(c.ints.get(idx).map(|v| v.to_string())) });
            }
            Operand::UintIndex => {
                let _ = write!(s, " {}", if idx == 0 { "null".into() } else { lookup(c.uints.get(idx).map(|v| v.to_string())) });
            }
            Operand::DoubleIndex => {
                let _ = write!(s, " {}", if idx == 0 { "null".into() } else { lookup(c.doubles.get(idx).map(|v| ecma_number(*v))) });
            }
            Operand::FloatIndex => {
                let _ = write!(s, " {}", if idx == 0 { "null".into() } else { lookup(c.floats.get(idx).map(|v| ecma_number(*v as f64))) });
            }
            Operand::Offset => {
                let _ = write!(s, " {}", addr(ins.offset as i64 + v + ins.len as i64));
            }
            Operand::CaseBaseoffset => {
                let _ = write!(s, " {}", addr(ins.offset as i64 + v));
            }
            Operand::OptCaseOffsets => {
                let offs: Vec<String> = ins.operands[i + 1..].iter().map(|o| addr(ins.offset as i64 + o)).collect();
                let _ = write!(s, " [{}]", offs.join(", "));
                break;
            }
            _ => {
                let _ = write!(s, " {v}");
            }
        }
    }
    s
}

fn method_header(abc: &Abc, method: usize, out: &mut Out) {
    let Some(m) = abc.methods.get(method) else { return };
    out.line(&format!("name {}", quoted_or_null(abc, m.name, escape_as)));
    for (flag, name) in [
        (method_flags::EXPLICIT, "NATIVE"),
        (method_flags::HAS_OPTIONAL, "HAS_OPTIONAL"),
        (method_flags::HAS_PARAM_NAMES, "HAS_PARAM_NAMES"),
        (method_flags::IGNORE_REST, "IGNORE_REST"),
        (method_flags::NEED_ACTIVATION, "NEED_ACTIVATION"),
        (method_flags::NEED_ARGUMENTS, "NEED_ARGUMENTS"),
        (method_flags::NEED_REST, "NEED_REST"),
        (method_flags::SET_DXNS, "SET_DXNS"),
    ] {
        if m.flags & flag != 0 {
            out.line(&format!("flag {name}"));
        }
    }
    for &p in &m.param_types {
        out.line(&format!("param {}", abc.multiname_pcode(p)));
    }
    if m.flags & method_flags::HAS_PARAM_NAMES != 0 {
        for &n in &m.param_names {
            out.line(&format!("paramname {}", if n == 0 { "null".into() } else { format!("\"{}\"", abc.str(n)) }));
        }
    }
    if m.flags & method_flags::HAS_OPTIONAL != 0 {
        for v in &m.optional {
            out.line(&format!("optional {}", value_pcode(abc, v)));
        }
    }
    out.line(&format!("returns {}", abc.multiname_pcode(m.return_type)));
}

/// Index of the first instruction at or after `a`, mapped back to its offset.
fn boundary(code: &[Instruction], a: usize) -> usize {
    code.iter().find(|i| i.offset >= a).map_or_else(|| code.last().map_or(0, |l| l.offset + l.len), |i| i.offset)
}

fn body(abc: &Abc, b: &MethodBody, out: &mut Out) {
    out.line("body");
    out.depth += 1;
    out.line(&format!("maxstack {}", b.max_stack));
    out.line(&format!("localcount {}", b.max_regs));
    out.line(&format!("initscopedepth {}", b.init_scope_depth));
    out.line(&format!("maxscopedepth {}", b.max_scope_depth));
    for t in &b.traits {
        trait_header(abc, t, out);
        out.depth -= 1;
        out.line("end ; trait");
    }
    out.s.push('\n');
    out.line("code");
    out.depth += 1;
    let code = disassemble(&b.code, &b.exceptions.iter().map(|e| e.target).collect::<Vec<_>>(), Dialect::of(abc.major, abc.minor));
    let mut labels: BTreeSet<usize> = BTreeSet::new();
    for e in &b.exceptions {
        labels.insert(boundary(&code, e.start));
        labels.insert(boundary(&code, e.end));
        labels.insert(e.target);
    }
    for ins in &code {
        labels.extend(ins.targets());
    }
    let label_depth = out.depth.saturating_sub(3);
    for ins in &code {
        if labels.contains(&ins.offset) {
            out.at(label_depth, &format!("{}:", addr(ins.offset as i64)));
        }
        out.line(&instruction_text(abc, ins));
    }
    out.depth -= 1;
    out.line("end ; code");
    for e in &b.exceptions {
        out.line(&format!(
            "try from {} to {} target {} type {} name {} end",
            addr(boundary(&code, e.start) as i64),
            addr(boundary(&code, e.end) as i64),
            addr(e.target as i64),
            if e.type_name == 0 { "null".into() } else { abc.multiname_pcode(e.type_name) },
            if e.var_name == 0 { "null".into() } else { abc.multiname_pcode(e.var_name) },
        ));
    }
    out.depth -= 1;
    out.line("end ; body");
}

/// `method` … `end ; method`, with the body when the method has one.
fn method(abc: &Abc, index: usize, out: &mut Out) {
    out.line("method");
    out.depth += 1;
    method_header(abc, index, out);
    if let Some(b) = abc.body(index) {
        out.s.push('\n');
        body(abc, b, out);
    }
    out.depth -= 1;
    out.line("end ; method");
}

fn trait_header(abc: &Abc, t: &Trait, out: &mut Out) {
    out.line(&format!("trait {} {}", t.kind_name(), abc.multiname_pcode(t.name)));
    out.depth += 1;
    if t.attrs & super::trait_attr::FINAL != 0 {
        out.line("flag FINAL");
    }
    if t.attrs & super::trait_attr::OVERRIDE != 0 {
        out.line("flag OVERRIDE");
    }
    if t.attrs & super::trait_attr::METADATA != 0 {
        out.line("flag METADATA");
    }
    if t.attrs & 0x8 != 0 {
        out.line("flag 0x8");
    }
    for &m in &t.metadata {
        let Some(md) = abc.metadata.get(m) else { continue };
        out.line(&format!("metadata \"{}\"", escape_pcode(abc.str(md.name))));
        out.depth += 1;
        for (k, v) in md.keys.iter().zip(&md.values) {
            let q = |i: usize| if i == 0 { "null".into() } else { format!("\"{}\"", escape_pcode(abc.str(i))) };
            out.line(&format!("item {} {}", q(*k), q(*v)));
        }
        out.depth -= 1;
        out.line("end ; metadata");
    }
    match &t.data {
        TraitData::Slot { slot_id, type_name, value } | TraitData::Const { slot_id, type_name, value } => {
            out.line(&format!("slotid {slot_id}"));
            out.line(&format!("type {}", abc.multiname_pcode(*type_name)));
            if let Some(v) = value {
                if v.kind != 0 {
                    out.line(&format!("value {}", value_pcode(abc, v)));
                }
            }
        }
        TraitData::Method { disp_id, .. } | TraitData::Getter { disp_id, .. } | TraitData::Setter { disp_id, .. } => {
            out.line(&format!("dispid {disp_id}"));
        }
        TraitData::Function { slot_id, .. } => {
            out.line(&format!("slotid {slot_id}"));
        }
        TraitData::Class { slot_id, class } => {
            out.line(&format!("slotid {slot_id}"));
            class_header(abc, *class, out);
        }
    }
}

fn class_header(abc: &Abc, class: usize, out: &mut Out) {
    let Some(ii) = abc.instances.get(class) else { return };
    out.line("class");
    out.depth += 1;
    out.line(&format!("instance {}", abc.multiname_pcode(ii.name)));
    out.depth += 1;
    out.line(&format!("extends {}", abc.multiname_pcode(ii.super_name)));
    for &i in &ii.interfaces {
        out.line(&format!("implements {}", abc.multiname_pcode(i)));
    }
    use super::instance_flags::*;
    for (f, n) in [(SEALED, "SEALED"), (FINAL, "FINAL"), (INTERFACE, "INTERFACE"), (PROTECTED_NS, "PROTECTEDNS"), (NON_NULLABLE, "NON_NULLABLE")] {
        if ii.flags & f != 0 {
            out.line(&format!("flag {n}"));
        }
    }
    if ii.flags & PROTECTED_NS != 0 {
        out.line(&format!("protectedns {}", abc.ns_pcode(ii.protected_ns)));
    }
    out.depth -= 1;
    out.line("end ; instance");
    out.depth -= 1;
    out.line("end ; class");
}

/// `convertTraitHeader`: the trait lines before its method.
pub fn trait_header_pcode(abc: &Abc, t: &Trait) -> String {
    let mut out = Out::default();
    trait_header(abc, t, &mut out);
    out.s
}

/// A trait with its method (for methods, getters, setters and functions).
pub fn trait_pcode(abc: &Abc, t: &Trait) -> String {
    let mut out = Out::default();
    trait_header(abc, t, &mut out);
    if let Some(m) = t.method() {
        method(abc, m, &mut out);
    }
    out.depth -= 1;
    out.line("end ; trait");
    out.s
}

/// A bare method, such as a class or instance initializer.
pub fn method_pcode(abc: &Abc, index: usize) -> String {
    let mut out = Out::default();
    method(abc, index, &mut out);
    out.s
}

/// A whole class: its header, initializers and every trait.
pub fn class_pcode(abc: &Abc, class: usize) -> String {
    let mut s = String::new();
    let Some(ii) = abc.instances.get(class) else { return s };
    let mut out = Out::default();
    class_header(abc, class, &mut out);
    s.push_str(&out.s);
    s.push_str("\n; instance initializer\n");
    s.push_str(&method_pcode(abc, ii.iinit));
    if let Some(c) = abc.classes.get(class) {
        s.push_str("\n; class initializer\n");
        s.push_str(&method_pcode(abc, c.cinit));
        for t in &c.traits {
            s.push_str("\n; static\n");
            s.push_str(&trait_pcode(abc, t));
        }
    }
    for t in &ii.traits {
        s.push('\n');
        s.push_str(&trait_pcode(abc, t));
    }
    s
}

/// A script: its initializer and its traits (classes as headers only).
pub fn script_pcode(abc: &Abc, script: usize) -> String {
    let mut s = String::new();
    let Some(sc) = abc.scripts.get(script) else { return s };
    s.push_str("; script initializer\n");
    s.push_str(&method_pcode(abc, sc.init));
    for t in &sc.traits {
        s.push('\n');
        s.push_str(&trait_pcode(abc, t));
    }
    s
}
