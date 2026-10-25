//! Constructors and type tests for AS1/2 items.

use crate::abc::decomp::item::*;

/// An item made by the AS1/2 translator.
pub fn amk(k: K, src: Option<usize>, ls: Option<usize>, p: u8, value: Option<It>) -> It {
    let it = mk(k, src, ls, p, value);
    it.borrow_mut().as2 = true;
    it
}

pub fn direct(v: AVal) -> It {
    amk(K::ADirect { v, computed: None }, None, None, prec::PRIMARY, None)
}

pub fn direct_at(src: Option<usize>, ls: Option<usize>, pos: i32, v: AVal) -> It {
    let it = amk(K::ADirect { v, computed: None }, src, ls, prec::PRIMARY, None);
    it.borrow_mut().pos = pos;
    it
}

pub fn dstr(s: &str) -> It {
    direct(AVal::Str(s.to_string()))
}

pub fn abin(op: Bin, src: Option<usize>, ls: Option<usize>, l: It, r: It) -> It {
    amk(K::Bin { op, l, r }, src, ls, op.prec(), None)
}

pub fn afn(name: &'static str, src: Option<usize>, ls: Option<usize>, args: Vec<It>, ret: bool) -> It {
    afn_with(name, src, ls, args, ",", String::new(), ret)
}

pub fn afn_with(name: &'static str, src: Option<usize>, ls: Option<usize>, args: Vec<It>, sep: &'static str, suffix: String, ret: bool) -> It {
    amk(K::AFn { name, args: new_list(args), sep, suffix, ret, side: true }, src, ls, prec::PRIMARY, None)
}

/// `afn` for pure functions such as `int(x)` or `length(s)`.
pub fn afn_pure(name: &'static str, src: Option<usize>, ls: Option<usize>, args: Vec<It>) -> It {
    amk(K::AFn { name, args: new_list(args), sep: ",", suffix: String::new(), ret: true, side: false }, src, ls, prec::PRIMARY, None)
}

pub fn is_direct(it: &It) -> bool {
    matches!(it.borrow().k, K::ADirect { .. })
}

/// `DirectValueActionItem.isString()` and `getAsString()`.
pub fn direct_string(it: &It) -> Option<String> {
    match &it.borrow().k {
        K::ADirect { v: AVal::Str(s), .. } => Some(s.clone()),
        _ => None,
    }
}

pub fn is_empty_string(it: &It) -> bool {
    direct_string(it).is_some_and(|s| s.is_empty())
}

/// `EcmaScript.toNumberAs2`.
pub fn to_number_as2(v: &AVal) -> f64 {
    match v {
        AVal::Int(i) => *i as f64,
        AVal::Double(d) => *d,
        AVal::Float(f) => *f as f64,
        AVal::Null | AVal::Undefined => 0.0,
        AVal::Str(s) => {
            let t = s.trim();
            if t.is_empty() {
                return f64::NAN;
            }
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                return i64::from_str_radix(h, 16).map_or(f64::NAN, |v| v as i32 as f64);
            }
            t.parse::<f64>().unwrap_or(f64::NAN)
        }
        AVal::Register { .. } => f64::NAN,
    }
}

/// `getAsLong()`.
pub fn as_long(it: &It) -> i64 {
    match &it.borrow().k {
        K::ADirect { v, .. } => {
            let n = to_number_as2(v);
            if n.is_nan() { 0 } else { n as i64 }
        }
        K::True => 1,
        _ => 0,
    }
}

/// `getResultAsNumber()` for the values pushed as literals.
pub fn result_as_number(it: &It) -> Option<f64> {
    match &it.borrow().k {
        K::ADirect { v, .. } => Some(to_number_as2(v)),
        K::True => Some(1.0),
        K::False => Some(0.0),
        _ => None,
    }
}

/// `isCompileTime()` and `getResult()` as a string, for the literal checks in GetURL2.
pub fn compile_time_string(it: &It) -> Option<String> {
    match &it.borrow().k {
        K::ADirect { v: AVal::Str(s), .. } => Some(s.clone()),
        K::Bin { op: Bin::StrAdd | Bin::Add, l, r } => Some(compile_time_string(l)? + &compile_time_string(r)?),
        _ => None,
    }
}

/// Kinds printed by `action::decomp::print`.
pub fn is_action_kind(k: &K) -> bool {
    matches!(
        k,
        K::ADirect { .. }
            | K::AUnresolved(_)
            | K::AGetVar { .. }
            | K::ASetVar { .. }
            | K::AGetMember { .. }
            | K::ASetMember { .. }
            | K::AGetProp { .. }
            | K::ASetProp { .. }
            | K::ADefineLocal { .. }
            | K::ADelete { .. }
            | K::AStoreReg { .. }
            | K::ACallFunction { .. }
            | K::ACallMethod { .. }
            | K::ANewMethod { .. }
            | K::ANewObject { .. }
            | K::AInitArray { .. }
            | K::AInitObject { .. }
            | K::AInc { .. }
            | K::ADec { .. }
            | K::APostInc { .. }
            | K::APostDec { .. }
            | K::AEnumerate { .. }
            | K::AEnumerated
            | K::AEnumAssign
            | K::AFunction(_)
            | K::AReturn
            | K::AEval
            | K::ATypeOf
            | K::ATellTarget { .. }
            | K::ASetTarget(_)
            | K::ASetTarget2 { .. }
            | K::AWith { .. }
            | K::ATry { .. }
            | K::AForIn { .. }
            | K::AIfFrameLoaded { .. }
            | K::AClass(_)
            | K::AInterface { .. }
            | K::ATempReg { .. }
            | K::ATempRegMark { .. }
            | K::AStrict(_)
            | K::AGotoFrame(_)
            | K::AExtends { .. }
            | K::AImplements { .. }
            | K::ACastOp { .. }
            | K::AFn { .. }
            | K::AUnsupported(_)
    )
}

/// Classes implementing `SetTypeActionItem`.
pub fn is_set_type_action(it: &It) -> bool {
    matches!(it.borrow().k, K::ASetVar { .. } | K::ASetMember { .. } | K::ASetProp { .. } | K::ADefineLocal { .. } | K::AStoreReg { .. } | K::APostInc { .. } | K::APostDec { .. })
}

/// `SetTypeActionItem.getObject()`.
pub fn set_type_object(it: &It) -> It {
    let (k, src, ls) = {
        let b = it.borrow();
        (b.k.clone(), b.src, b.line_start)
    };
    match k {
        K::ASetVar { name, .. } => amk(K::AGetVar { name, print_obf: false, computed: None }, src, ls, prec::PRIMARY, None),
        K::ASetMember { obj, member, .. } => amk(K::AGetMember { obj, member, print_obf: false, getter: false }, src, ls, prec::PRIMARY, None),
        K::ASetProp { target, index } => amk(K::AGetProp { target, index, use_fn: true }, src, ls, prec::PRIMARY, None),
        K::ADefineLocal { name } => amk(K::ADefineLocal { name }, src, ls, prec::PRIMARY, None),
        K::AStoreReg { reg, name, .. } => {
            let d = direct_at(src, ls, -1, AVal::Register { num: reg, name });
            d
        }
        K::APostInc { obj } | K::APostDec { obj } => obj,
        _ => it.clone(),
    }
}

/// `SetTypeActionItem.getValue()`.
pub fn set_type_value(it: &It) -> Option<It> {
    let k = it.borrow().k.clone();
    match k {
        K::APostInc { obj } => Some(abin(Bin::Add, None, None, obj, direct(AVal::Int(1)))),
        K::APostDec { obj } => Some(abin(Bin::Sub, None, None, obj, direct(AVal::Int(1)))),
        _ => it.borrow().value.clone(),
    }
}

/// `Action.getWithoutGlobal`.
pub fn without_global(ti: &It) -> It {
    if !matches!(ti.borrow().k, K::AGetMember { .. }) {
        return ti.clone();
    }
    let mut last: Option<It> = None;
    let mut t = ti.clone();
    loop {
        let obj = match &t.borrow().k {
            K::AGetMember { obj, .. } => obj.clone(),
            _ => unreachable!(),
        };
        if matches!(obj.borrow().k, K::AGetMember { .. }) {
            last = Some(t.clone());
            t = obj;
        } else {
            break;
        }
    }
    let (obj, member) = match &t.borrow().k {
        K::AGetMember { obj, member, .. } => (obj.clone(), member.clone()),
        _ => unreachable!(),
    };
    let is_global = matches!(&obj.borrow().k, K::AGetVar { name, .. } if direct_string(name).as_deref() == Some("_global"));
    if is_global {
        let gvt = amk(K::AGetVar { name: member, print_obf: false, computed: None }, None, None, prec::PRIMARY, None);
        match last {
            None => return gvt,
            Some(l) => {
                if let K::AGetMember { obj, .. } = &mut l.borrow_mut().k {
                    *obj = gvt;
                }
            }
        }
    }
    ti.clone()
}
