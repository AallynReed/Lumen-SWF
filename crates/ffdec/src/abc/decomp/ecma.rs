//! FFDec's `EcmaScript` value semantics and the `getResult` of AS3 items, used by the
//! deobfuscator to decide conditions it can evaluate statically.

use std::rc::Rc;

use super::item::{Bin, It, K, Un};

/// A Java value as FFDec's `getResult` returns it.
#[derive(Debug, Clone)]
pub enum V {
    /// Java `null`: not computable.
    None,
    Null,
    Undefined,
    Bool(bool),
    Int(i32),
    Long(i64),
    Double(f64),
    Float(f32),
    Str(String),
    Obj(Rc<Obj>),
}

#[derive(Debug)]
pub enum Obj {
    Plain(Vec<(String, V)>),
    Array(Vec<V>),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Null,
    Str,
    Num,
    Undefined,
    Object,
    Boolean,
    Float,
}

pub fn ty(v: &V) -> Ty {
    match v {
        V::None | V::Null => Ty::Null,
        V::Str(_) => Ty::Str,
        V::Int(_) | V::Long(_) | V::Double(_) => Ty::Num,
        V::Bool(_) => Ty::Boolean,
        V::Undefined => Ty::Undefined,
        V::Float(_) => Ty::Float,
        V::Obj(_) => Ty::Object,
    }
}

fn obj_to_string(o: &Obj) -> String {
    match o {
        Obj::Plain(_) => "[object Object]".into(),
        Obj::Array(v) => v.iter().map(to_string).collect::<Vec<_>>().join(","),
        Obj::Named(s) => s.clone(),
    }
}

/// `Double.valueOf(String)`: Java's floating literal syntax, with surrounding whitespace.
fn java_parse_double(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let body = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    if matches!(body.trim_start_matches(['+', '-']), "Infinity" | "NaN") {
        return body.parse::<f64>().ok().or(Some(if body.ends_with("NaN") { f64::NAN } else if body.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY }));
    }
    if body.chars().any(|c| !(c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))) {
        return None;
    }
    body.parse::<f64>().ok()
}

pub fn to_number(v: &V) -> f64 {
    match v {
        V::None | V::Null => 0.0,
        V::Undefined => f64::NAN,
        V::Bool(b) => *b as i32 as f64,
        V::Float(f) => *f as f64,
        V::Double(d) => *d,
        V::Long(l) => *l as f64,
        V::Int(i) => *i as f64,
        V::Str(s) => {
            if s.is_empty() {
                0.0
            } else {
                java_parse_double(s).unwrap_or(f64::NAN)
            }
        }
        V::Obj(_) => to_number(&to_primitive(v, "Number")),
    }
}

pub fn to_primitive(v: &V, hint: &str) -> V {
    match v {
        V::Obj(o) => match hint {
            "String" => V::Str(obj_to_string(o)),
            "Number" => V::Double(to_number(&V::Str(obj_to_string(o)))),
            _ => V::Str(obj_to_string(o)),
        },
        V::None => V::Undefined,
        _ => v.clone(),
    }
}

pub fn to_string(v: &V) -> String {
    let num = |d: f64| -> String {
        if (d as i32) as f64 == d {
            return (d as i32).to_string();
        }
        if d == f64::INFINITY {
            return "Infinity".into();
        }
        if d == f64::NEG_INFINITY {
            return "-Infinity".into();
        }
        if d.is_nan() {
            return "NaN".into();
        }
        crate::abc::names::ecma_number(d)
    };
    match v {
        V::None => "null".into(),
        V::Null => "null".into(),
        V::Undefined => "undefined".into(),
        V::Bool(b) => b.to_string(),
        V::Int(i) => num(*i as f64),
        V::Long(l) => num(*l as f64),
        V::Double(d) => num(*d),
        V::Float(f) => num(*f as f64),
        V::Str(s) => s.clone(),
        V::Obj(o) => obj_to_string(o),
    }
}

pub fn to_boolean(v: &V) -> bool {
    match v {
        V::None | V::Null | V::Undefined => false,
        V::Bool(b) => *b,
        V::Long(l) => *l != 0,
        V::Int(i) => *i != 0,
        V::Float(f) => !(f.is_nan() || *f == 0.0),
        V::Double(d) => !(d.is_nan() || *d == 0.0),
        V::Str(s) => !s.is_empty(),
        V::Obj(_) => true,
    }
}

pub fn to_uint32(v: &V) -> i64 {
    let n = to_number(v);
    if n.is_nan() || n == 0.0 || n.is_infinite() {
        return 0;
    }
    ((n.signum() * n.abs().floor()) as i64) & 0xffff_ffff
}

pub fn to_int32(v: &V) -> i32 {
    to_uint32(v) as i32
}

fn num_eq(x: f64, y: f64) -> bool {
    !(x.is_nan() || y.is_nan()) && x == y
}

pub fn equals(x: &V, y: &V) -> bool {
    let (tx, tyy) = (ty(x), ty(y));
    if tx == tyy {
        return match (x, y) {
            _ if tx == Ty::Null || tx == Ty::Undefined => true,
            _ if tx == Ty::Num => num_eq(to_number(x), to_number(y)),
            (V::Str(a), V::Str(b)) => a == b,
            (V::Bool(a), V::Bool(b)) => a == b,
            (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
    }
    match (tx, tyy) {
        (Ty::Null, Ty::Undefined) | (Ty::Undefined, Ty::Null) => true,
        (Ty::Num, Ty::Str) => equals(x, &V::Double(to_number(y))),
        (Ty::Str, Ty::Num) => equals(&V::Double(to_number(x)), y),
        (Ty::Boolean, _) => equals(&V::Double(to_number(x)), y),
        (_, Ty::Boolean) => equals(x, &V::Double(to_number(y))),
        _ => false,
    }
}

/// `EcmaScript.compare`: -1, 0 or 1, or None for undefined.
pub fn compare(x: &V, y: &V) -> Option<i32> {
    if let (V::Str(sx), V::Str(sy)) = (x, y) {
        if sx == sy {
            return Some(0);
        }
        if sx.starts_with(sy.as_str()) {
            return Some(1);
        }
        if sy.starts_with(sx.as_str()) {
            return Some(-1);
        }
        let (a, b): (Vec<u16>, Vec<u16>) = (sx.encode_utf16().collect(), sy.encode_utf16().collect());
        for k in 0..a.len().max(b.len()) {
            let (m, n) = (a.get(k).copied().unwrap_or(0), b.get(k).copied().unwrap_or(0));
            if m != n {
                return Some(if m < n { -1 } else { 1 });
            }
        }
        return Some(0);
    }
    let (nx, ny) = (to_number(x), to_number(y));
    if nx.is_nan() || ny.is_nan() {
        return None;
    }
    if nx.total_cmp(&ny).is_eq() || nx == ny {
        return Some(0);
    }
    if nx == f64::INFINITY {
        return Some(1);
    }
    if ny == f64::INFINITY {
        return Some(-1);
    }
    if nx == f64::NEG_INFINITY {
        return Some(1);
    }
    if ny == f64::NEG_INFINITY {
        return Some(-1);
    }
    Some(if nx < ny { -1 } else { 1 })
}

fn is_ty(it: &It, name: &str) -> bool {
    match &it.borrow().k {
        K::Type { full, .. } => full.raw() == name,
        K::Any => name == "*",
        _ => false,
    }
}

fn num(it: &It) -> f64 {
    to_number(&result(it))
}

/// `getResultAsBoolean`.
pub fn result_as_bool(it: &It) -> bool {
    if matches!(it.borrow().k, K::This { .. }) {
        return true;
    }
    to_boolean(&result(it))
}

/// `GraphTargetItem.getResult` for the items FFDec can evaluate.
pub fn result(it: &It) -> V {
    let b = it.borrow();
    let v = b.value.clone();
    match &b.k {
        K::Boolean(x) => V::Bool(*x),
        K::True => V::Bool(true),
        K::False => V::Bool(false),
        K::IntegerValueA { v, .. } => V::Int(*v),
        K::IntegerValue(i) => V::Long(*i as i64),
        K::DoubleValue(d) => V::Double(*d),
        K::FloatValue(f) => V::Float(*f),
        K::Str(s) => V::Str(s.clone()),
        K::Null => V::Null,
        K::Undefined => V::Undefined,
        K::Swap | K::Pop => V::Null,
        K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. } => v.map_or(V::None, |v| result(&v)),
        K::LocalReg { computed, .. } => computed.as_ref().map_or(V::None, result),
        K::This { basic, class_name, .. } => {
            if *basic {
                let last = class_name.as_ref().map(|c| c.last().to_string()).unwrap_or_default();
                V::Obj(Rc::new(Obj::Named(format!("[object {last}]"))))
            } else {
                V::None
            }
        }
        K::Global => V::Obj(Rc::new(Obj::Named("global".into()))),
        K::NewArray { values } => {
            let mut out = Vec::new();
            for x in values.borrow().iter() {
                let r = result(x);
                if matches!(r, V::None) {
                    return V::None;
                }
                out.push(r);
            }
            V::Obj(Rc::new(Obj::Array(out)))
        }
        K::NewObject { pairs } => {
            let mut props = Vec::new();
            for p in pairs.borrow().iter() {
                let pb = p.borrow();
                if let K::NameValuePair { name } = &pb.k {
                    props.push((to_string(&result(name)), pb.value.as_ref().map_or(V::None, result)));
                }
            }
            V::Obj(Rc::new(Obj::Plain(props)))
        }
        K::GetProperty { obj, prop, .. } => {
            let ores = result(obj);
            let t = ty(&ores);
            if t != Ty::Object {
                if let K::FullMultiname { resolved: Some(name), .. } = &prop.borrow().k {
                    return if t == Ty::Str && name == "length" { V::Int(to_string(&ores).encode_utf16().count() as i32) } else { V::None };
                }
            }
            match &obj.borrow().k {
                K::NewArray { values } if values.borrow().is_empty() => V::Undefined,
                K::NewObject { pairs } if pairs.borrow().is_empty() => V::Undefined,
                _ => V::None,
            }
        }
        K::Coerce { ty: t } => {
            let inner = v.expect("value");
            let r = result(&inner);
            if is_ty(t, "String") {
                return match r {
                    V::Null => V::Null,
                    V::Undefined => V::Null,
                    _ => V::Str(to_string(&r)),
                };
            }
            r
        }
        K::Convert { ty: t } => {
            let inner = v.expect("value");
            let r = result(&inner);
            if is_ty(t, "Boolean") {
                V::Bool(to_boolean(&r))
            } else if is_ty(t, "Number") {
                V::Double(to_number(&r))
            } else if is_ty(t, "int") {
                V::Int(to_int32(&V::Double(to_number(&r))))
            } else if is_ty(t, "uint") {
                V::Long(to_uint32(&r))
            } else if is_ty(t, "String") {
                V::Str(to_string(&r))
            } else if is_ty(t, "Object") {
                r
            } else {
                V::Obj(Rc::new(Obj::Plain(Vec::new())))
            }
        }
        K::Increment => V::Double(num(&v.expect("value")) + 1.0),
        K::Decrement => V::Double(num(&v.expect("value")) - 1.0),
        K::Un { op, .. } => {
            let inner = v.expect("value");
            match op {
                Un::Not => V::Bool(!result_as_bool(&inner)),
                Un::Neg => V::Double(-num(&inner)),
                Un::BitNot => V::Long(!(num(&inner) as i64)),
                Un::UnPlus => V::Double(num(&inner)),
                Un::TypeOf => V::Str(
                    match ty(&result(&inner)) {
                        Ty::Undefined => "undefined",
                        Ty::Null => "object",
                        Ty::Boolean => "Boolean",
                        Ty::Num => "number",
                        Ty::Str => "string",
                        _ => "object",
                    }
                    .into(),
                ),
                _ => V::None,
            }
        }
        K::Bin { op, l, r } => {
            bin_result(*op, l, r)
        }
        _ => V::None,
    }
}

fn bin_result(op: Bin, l: &It, r: &It) -> V {
    let lnum = || num(l) as i64;
    let rnum = || num(r) as i64;
    match op {
        Bin::Add => {
            let a = to_primitive(&result(l), "");
            let b = to_primitive(&result(r), "");
            if ty(&a) == Ty::Str || ty(&b) == Ty::Str {
                V::Str(to_string(&a) + &to_string(&b))
            } else {
                V::Double(to_number(&a) + to_number(&b))
            }
        }
        Bin::Sub => V::Double(num(l) - num(r)),
        Bin::Mul => V::Double(num(l) * num(r)),
        Bin::Div => {
            let (a, b) = (num(l), num(r));
            if b.to_bits() == 0 {
                V::Double(if a < 0.0 { f64::NEG_INFINITY } else if a > 0.0 { f64::INFINITY } else { f64::NAN })
            } else {
                V::Double(a / b)
            }
        }
        Bin::Mod => {
            let b = num(r);
            if b.is_nan() || b.to_bits() == 0 {
                V::Double(f64::NAN)
            } else {
                let d = b as i64;
                if d == 0 {
                    V::Double(f64::NAN)
                } else {
                    V::Long(lnum().wrapping_rem(d))
                }
            }
        }
        Bin::BitAnd => V::Long(lnum() & rnum()),
        Bin::BitOr => V::Long(lnum() | rnum()),
        Bin::BitXor => V::Long(lnum() ^ rnum()),
        Bin::LShift => V::Int((num(l) as i32).wrapping_shl(num(r) as i32 as u32)),
        Bin::RShift => V::Long(lnum().wrapping_shr(rnum() as u32)),
        Bin::URShift => V::Long(((lnum() as u64).wrapping_shr(rnum() as u32)) as i64),
        Bin::Eq => V::Bool(equals(&result(l), &result(r))),
        Bin::Neq => V::Bool(!equals(&result(l), &result(r))),
        Bin::StrictEq => {
            let (x, y) = (result(l), result(r));
            V::Bool(ty(&x) == ty(&y) && equals(&x, &y))
        }
        Bin::StrictNeq => {
            let (x, y) = (result(l), result(r));
            V::Bool(ty(&x) != ty(&y) || !equals(&x, &y))
        }
        Bin::Lt => compare(&result(l), &result(r)).map_or(V::Undefined, |c| V::Bool(c == -1)),
        Bin::Ge => compare(&result(l), &result(r)).map_or(V::Undefined, |c| V::Bool(c != -1)),
        Bin::Gt => compare(&result(r), &result(l)).map_or(V::Undefined, |c| V::Bool(c == -1)),
        Bin::Le => compare(&result(r), &result(l)).map_or(V::Undefined, |c| V::Bool(c != -1)),
        _ => V::None,
    }
}
