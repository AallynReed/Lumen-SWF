//! Java `equals` semantics for items: identity unless the class overrides it.

use std::rc::Rc;

use super::item::{It, List, K};

fn opt_eq(a: &Option<It>, b: &Option<It>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

pub fn list_eq(a: &List, b: &List) -> bool {
    if Rc::ptr_eq(a, b) {
        return true;
    }
    let (a, b) = (a.borrow(), b.borrow());
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| eq(x, y))
}

pub fn eq(a: &It, b: &It) -> bool {
    if Rc::ptr_eq(a, b) {
        return true;
    }
    let (x, y) = (a.borrow(), b.borrow());
    let v = || opt_eq(&x.value, &y.value);
    match (&x.k, &y.k) {
        (K::Bin { op: o1, l: l1, r: r1 }, K::Bin { op: o2, l: l2, r: r2 }) => o1 == o2 && eq(l1, l2) && eq(r1, r2),
        (K::Un { op: o1, .. }, K::Un { op: o2, .. }) => o1 == o2 && v(),
        (K::Ternar { expr: e1, on_true: t1, on_false: f1 }, K::Ternar { expr: e2, on_true: t2, on_false: f2 }) => eq(e1, e2) && eq(t1, t2) && eq(f1, f2),
        (K::True, K::True) | (K::False, K::False) | (K::DNan, K::DNan) | (K::Global, K::Global) | (K::Nan, K::Nan) => true,
        (K::NewActivation, K::NewActivation) | (K::Null, K::Null) | (K::ReturnVoid, K::ReturnVoid) | (K::Undefined, K::Undefined) => true,
        (K::This { .. }, K::This { .. }) | (K::UnboundedType, K::UnboundedType) => true,
        (K::IntegerValue(p), K::IntegerValue(q)) => p == q,
        (K::Push { .. }, K::Push { .. }) => v(),
        (K::AlchemyLoad { ty: t1, size: s1, ofs: o1 }, K::AlchemyLoad { ty: t2, size: s2, ofs: o2 }) => s1 == s2 && t1 == t2 && eq(o1, o2),
        (K::AlchemySignExtend { size: s1 }, K::AlchemySignExtend { size: s2 }) => s1 == s2 && v(),
        (K::AlchemyStore { ty: t1, size: s1, ofs: o1 }, K::AlchemyStore { ty: t2, size: s2, ofs: o2 }) => s1 == s2 && t1 == t2 && eq(o1, o2) && v(),
        (K::ApplyType { obj: o1, params: p1 }, K::ApplyType { obj: o2, params: p2 }) => eq(o1, o2) && list_eq(p1, p2),
        (K::Boolean(p), K::Boolean(q)) => p == q,
        (K::Call { receiver: r1, function: f1, args: a1 }, K::Call { receiver: r2, function: f2, args: a2 }) => eq(r1, r2) && eq(f1, f2) && list_eq(a1, a2),
        (K::CallMethod { receiver: r1, name: n1, args: a1 }, K::CallMethod { receiver: r2, name: n2, args: a2 })
        | (K::CallStatic { receiver: r1, name: n1, args: a1 }, K::CallStatic { receiver: r2, name: n2, args: a2 }) => n1 == n2 && eq(r1, r2) && list_eq(a1, a2),
        (K::CallProperty { receiver: r1, prop: p1, args: a1, is_void: v1, .. }, K::CallProperty { receiver: r2, prop: p2, args: a2, is_void: v2, .. }) => {
            v1 == v2 && eq(r1, r2) && eq(p1, p2) && list_eq(a1, a2)
        }
        (K::CallSuper { receiver: r1, mn: m1, args: a1, is_void: v1 }, K::CallSuper { receiver: r2, mn: m2, args: a2, is_void: v2 }) => {
            v1 == v2 && eq(r1, r2) && eq(m1, m2) && list_eq(a1, a2)
        }
        (K::Construct { obj: o1, args: a1 }, K::Construct { obj: o2, args: a2 }) => eq(o1, o2) && list_eq(a1, a2) && v(),
        (K::ConstructProp { obj: o1, prop: p1, args: a1, .. }, K::ConstructProp { obj: o2, prop: p2, args: a2, .. }) => eq(o1, o2) && eq(p1, p2) && list_eq(a1, a2),
        (K::ConstructSuper { obj: o1, args: a1 }, K::ConstructSuper { obj: o2, args: a2 }) => eq(o1, o2) && list_eq(a1, a2),
        (K::Coerce { ty: t1 }, K::Coerce { ty: t2 }) | (K::Convert { ty: t1 }, K::Convert { ty: t2 }) => eq(t1, t2) && v(),
        (K::DecLocal { reg: r1, .. }, K::DecLocal { reg: r2, .. }) | (K::IncLocal { reg: r1, .. }, K::IncLocal { reg: r2, .. }) => r1 == r2,
        (K::DecimalValue(p), K::DecimalValue(q)) => p == q,
        (K::DoubleValue(p), K::DoubleValue(q)) => p.to_bits() == q.to_bits(),
        (K::FloatValue(p), K::FloatValue(q)) => p.to_bits() == q.to_bits(),
        (K::Float4Value(p), K::Float4Value(q)) => p.iter().zip(q.iter()).all(|(a, b)| a.to_bits() == b.to_bits()),
        (K::IntegerValueA { v: p, .. }, K::IntegerValueA { v: q, .. }) => p == q,
        (K::Decrement, K::Decrement) | (K::Increment, K::Increment) | (K::EscapeXAttr, K::EscapeXAttr) | (K::EscapeXElem, K::EscapeXElem) => v(),
        (K::ReturnValue, K::ReturnValue) | (K::Throw, K::Throw) => v(),
        (K::DefaultXmlNs { ns: n1 }, K::DefaultXmlNs { ns: n2 }) => eq(n1, n2) && v(),
        (K::DeleteProperty { obj: o1, prop: p1, .. }, K::DeleteProperty { obj: o2, prop: p2, .. }) => eq(o1, o2) && eq(p1, p2),
        (K::FilteredCheck { obj: o1 }, K::FilteredCheck { obj: o2 }) => eq(o1, o2),
        (K::FindDef { mn: m1 }, K::FindDef { mn: m2 }) => m1 == m2,
        (K::FindProperty { prop: p1 }, K::FindProperty { prop: p2 }) => eq(p1, p2),
        (K::FullMultiname { mn: m1, name: n1, ns: s1, property: p1, .. }, K::FullMultiname { mn: m2, name: n2, ns: s2, property: p2, .. }) => {
            m1 == m2 && p1 == p2 && opt_eq(n1, n2) && opt_eq(s1, s2)
        }
        (K::GetDescendants { obj: o1, mn: m1 }, K::GetDescendants { obj: o2, mn: m2 }) => eq(o1, o2) && eq(m1, m2),
        (K::GetLex { mn: m1, .. }, K::GetLex { mn: m2, .. }) => m1 == m2,
        (K::GetProperty { obj: o1, prop: p1, .. }, K::GetProperty { obj: o2, prop: p2, .. })
        | (K::GetSuper { obj: o1, prop: p1, .. }, K::GetSuper { obj: o2, prop: p2, .. })
        | (K::InitProperty { obj: o1, prop: p1, .. }, K::InitProperty { obj: o2, prop: p2, .. }) => eq(o1, o2) && eq(p1, p2),
        (K::GetSlot { idx: i1, scope: s1, .. }, K::GetSlot { idx: i2, scope: s2, .. }) => i1 == i2 && eq(s1, s2),
        (K::HasNext { index: i1, obj: o1 }, K::HasNext { index: i2, obj: o2 })
        | (K::NextName { index: i1, obj: o1, .. }, K::NextName { index: i2, obj: o2, .. })
        | (K::NextValue { index: i1, obj: o1, .. }, K::NextValue { index: i2, obj: o2, .. }) => eq(i1, i2) && eq(o1, o2),
        (K::InFor { object: o1, collection: c1 }, K::InFor { object: o2, collection: c2 }) => eq(o1, o2) && eq(c1, c2),
        (K::InitVector { subtype: s1, args: a1 }, K::InitVector { subtype: s2, args: a2 }) => eq(s1, s2) && list_eq(a1, a2),
        (K::LocalReg { reg: r1, .. }, K::LocalReg { reg: r2, .. }) => r1 == r2,
        (K::NameSpace(p), K::NameSpace(q)) => p == q,
        (K::NameValuePair { name: n1 }, K::NameValuePair { name: n2 }) => eq(n1, n2) && v(),
        (K::NewArray { values: a }, K::NewArray { values: b }) => list_eq(a, b),
        (K::NewFunction { method: m1, name: n1, .. }, K::NewFunction { method: m2, name: n2, .. }) => m1 == m2 && n1 == n2,
        (K::NewObject { pairs: a }, K::NewObject { pairs: b }) => list_eq(a, b),
        (K::PostDecrement { obj: o1, .. }, K::PostDecrement { obj: o2, .. }) | (K::PostIncrement { obj: o1, .. }, K::PostIncrement { obj: o2, .. }) => eq(o1, o2),
        (K::RegExp { pattern: p1, modifier: m1 }, K::RegExp { pattern: p2, modifier: m2 }) => p1 == p2 && m1 == m2,
        (K::SetLocal { reg: r1, .. }, K::SetLocal { reg: r2, .. }) => r1 == r2 && v(),
        (K::SetSlot { idx: i1, scope: s1, .. }, K::SetSlot { idx: i2, scope: s2, .. }) => i1 == i2 && eq(s1, s2) && v(),
        (K::SetSuper { obj: o1, prop: p1, .. }, K::SetSuper { obj: o2, prop: p2, .. }) => eq(o1, o2) && eq(p1, p2) && v(),
        (K::Str(p), K::Str(q)) | (K::Unparsed(p), K::Unparsed(q)) => p == q,
        (K::Type { full: f1, ns: n1 }, K::Type { full: f2, ns: n2 }) => f1 == f2 && n1 == n2,
        (K::TypeFunction(p), K::TypeFunction(q)) => p == q,
        (K::With { scope: s1, items: i1 }, K::With { scope: s2, items: i2 }) => eq(s1, s2) && list_eq(i1, i2),
        (K::WithEnd { scope: s1 }, K::WithEnd { scope: s2 }) | (K::WithObject { scope: s1 }, K::WithObject { scope: s2 }) => eq(s1, s2),
        (K::Xml { parts: a }, K::Xml { parts: b }) => list_eq(a, b),
        _ => false,
    }
}

pub fn opt(a: &Option<It>, b: &Option<It>) -> bool {
    opt_eq(a, b)
}

/// `list.contains(item)`.
pub fn contains(list: &[It], it: &It) -> bool {
    list.iter().any(|x| eq(x, it))
}

/// `list.indexOf(item)`.
pub fn index_of(list: &[It], it: &It) -> Option<usize> {
    list.iter().position(|x| eq(x, it))
}
