//! Per-class behaviour of items: return types, "through" accessors, inversion and flags.

use super::item::*;
use super::names::Dotted;

pub fn prec_of(it: &It) -> u8 {
    let b = it.borrow();
    match &b.k {
        K::SetLocal { hide_value: true, .. } => prec::PRIMARY,
        _ => b.prec,
    }
}

fn value_of(it: &It) -> Option<It> {
    it.borrow().value.clone()
}

fn value_rt(it: &It) -> It {
    match value_of(it) {
        Some(v) => return_type(&v),
        None => unbounded(),
    }
}

fn num_like(t: &It) -> bool {
    is_type(t, "Number") || is_type(t, "int") || is_type(t, "uint")
}

pub fn return_type(it: &It) -> It {
    let k = it.borrow().k.clone();
    match k {
        K::Bin { op, l, r } => match op {
            Bin::Add => {
                if is_type(&return_type(&l), "String") || is_type(&return_type(&r), "String") {
                    ty("String")
                } else {
                    ty("Number")
                }
            }
            Bin::Sub | Bin::Mul | Bin::Div | Bin::Mod => ty("Number"),
            Bin::LShift | Bin::RShift | Bin::URShift | Bin::BitAnd | Bin::BitOr | Bin::BitXor => ty("int"),
            Bin::AsType => return_type(&r),
            Bin::NullCoalesce => return_type(&l),
            _ => ty("Boolean"),
        },
        K::Un { op, ty: t } => match op {
            Un::Not => ty("Boolean"),
            Un::Neg | Un::UnPlus => ty("Number"),
            Un::BitNot => ty("int"),
            Un::TypeOf => ty("String"),
            Un::PreIncrement | Un::PreDecrement => t.unwrap_or_else(unbounded),
        },
        K::Ternar { on_true, on_false, .. } => {
            let a = return_type(&on_true);
            let b = return_type(&on_false);
            if super::eq::eq(&a, &b) {
                return a;
            }
            if num_like(&a) && num_like(&b) {
                return ty("Number");
            }
            unknown()
        }
        K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. } | K::Parenthesis | K::SetSuper { .. } => value_rt(it),
        K::Comma { cmds } => match cmds.borrow().last() {
            Some(c) => return_type(c),
            None => unbounded(),
        },
        K::True | K::False | K::Boolean(_) | K::HasNext { .. } | K::InFor { .. } => ty("Boolean"),
        K::UnboundedType | K::Type { .. } | K::TypeFunction(_) => it.clone(),
        K::AlchemyLoad { ty: t, .. } => match t.as_str() {
            "i" => ty("int"),
            "f" => ty("Number"),
            "f4" => ty("float4"),
            _ => unbounded(),
        },
        K::AlchemySignExtend { .. } | K::IntegerValueA { .. } => ty("int"),
        K::Call { function, .. } => match &function.borrow().k {
            K::GetProperty { call_ty, .. } | K::GetLex { call_ty, .. } => call_ty.clone(),
            _ => unbounded(),
        },
        K::CallProperty { ty: t, .. } | K::ConstructProp { ty: t, .. } | K::Convert { ty: t } | K::GetLex { ty: t, .. } | K::GetSuper { ty: t, .. } => t,
        K::LocalReg { ty: t, .. } | K::PostDecrement { ty: t, .. } | K::PostIncrement { ty: t, .. } | K::SetLocal { ty: t, .. } => t,
        K::SetProperty { ty: t, .. } | K::SetSlot { ty: t, .. } => t,
        K::GetSlot { slot_ty, .. } => slot_ty,
        K::Construct { obj, .. } => return_type(&obj),
        K::Coerce { ty: t } => {
            if matches!(t.borrow().k, K::UnboundedType | K::ApplyType { .. } | K::Type { .. }) {
                t.clone()
            } else {
                ty(&java_to_string(&t))
            }
        }
        K::DNan => ty("decimal"),
        K::DecimalValue(_) | K::DoubleValue(_) | K::Float4Value(_) | K::Nan | K::Decrement | K::Increment => ty("Number"),
        K::FloatValue(_) => ty("float"),
        K::Declaration { ty: t, .. } => t.unwrap_or_else(unbounded),
        K::DeleteProperty { .. } => ty("Boolean"),
        K::GetProperty { obj, prop, ty: t, .. } => {
            if let K::FindProperty { prop: fp } = &obj.borrow().k {
                if let K::FullMultiname { resolved, .. } = &fp.borrow().k {
                    if super::eq::eq(&prop, fp) {
                        match resolved.as_deref() {
                            Some("NaN") => return ty("Number"),
                            Some("undefined") => return ty("Undefined"),
                            _ => {}
                        }
                    }
                }
            }
            t
        }
        K::Global => ty("global"),
        K::InitVector { subtype, .. } => mk(
            K::ApplyType { obj: ty_dotted(Dotted::new(&["__AS3__", "vec", "Vector"])), params: new_list(vec![subtype]) },
            None,
            None,
            prec::PRIMARY,
            None,
        ),
        K::NameSpace(_) => ty("Namespace"),
        K::NewArray { .. } => ty("Array"),
        K::NewFunction { .. } => ty("Function"),
        K::NewObject { .. } => ty("Object"),
        K::Null => ty("null"),
        K::RegExp { .. } => ty("RegExp"),
        K::Str(_) => ty("String"),
        K::This { class_name, .. } => ty_dotted(class_name.unwrap_or_default()),
        K::Xml { .. } => ty("XML"),
        _ => unbounded(),
    }
}

/// Java `toString()` of items, used for type names and a few `"this"` checks.
pub fn java_to_string(it: &It) -> String {
    let b = it.borrow();
    match &b.k {
        K::Type { full, .. } => full.raw(),
        K::UnboundedType => "*".into(),
        K::TypeFunction(n) => format!("Function[{n}]"),
        K::This { .. } => "this".into(),
        K::ApplyType { obj, params } => {
            let mut s = java_to_string(obj);
            let ps = params.borrow();
            if !ps.is_empty() {
                s.push_str(".<");
                s.push_str(&ps.iter().map(java_to_string).collect::<Vec<_>>().join(", "));
                s.push('>');
            }
            s
        }
        _ => String::new(),
    }
}

pub fn has_return_value(it: &It) -> bool {
    let b = it.borrow();
    match &b.k {
        K::If { .. } | K::While { .. } | K::DoWhile { .. } | K::For { .. } | K::Switch { .. } | K::Break { .. } | K::Continue { .. } => false,
        K::Label { .. } | K::Goto { .. } | K::Push { .. } | K::Swap | K::Comma { .. } | K::Comment(_) | K::ScriptEnd | K::Empty | K::Block { .. } => false,
        K::Parenthesis => b.value.as_ref().is_some_and(has_return_value),
        K::AlchemyStore { .. } | K::CallSuper { .. } | K::Class { .. } | K::ConstructSuper { .. } | K::DecLocal { .. } | K::IncLocal { .. } => false,
        K::Declaration { .. } | K::DefaultXmlNs { .. } | K::ExceptionItem(_) | K::Filter { .. } | K::ForEachIn { .. } | K::ForIn { .. } => false,
        K::InitProperty { .. } | K::Package { .. } | K::ReturnValue | K::ReturnVoid | K::Throw | K::SetLocal { .. } | K::SetProperty { .. } => false,
        K::SetSlot { .. } | K::SetSuper { .. } | K::StoreNewActivation { .. } | K::TraitSlotConst { .. } | K::Try { .. } | K::Unparsed(_) => false,
        K::With { .. } | K::WithEnd { .. } | K::WithObject { .. } => false,
        _ => true,
    }
}

pub fn needs_semicolon(it: &It) -> bool {
    let b = it.borrow();
    match &b.k {
        K::If { .. } | K::While { .. } | K::For { .. } | K::Switch { .. } | K::Label { .. } | K::Comment(_) | K::ScriptEnd => false,
        K::Goto { target, .. } => target.is_none(),
        K::ForEachIn { .. } | K::ForIn { .. } | K::Package { .. } | K::StoreNewActivation { .. } | K::TraitSlotConst { .. } | K::Try { .. } => false,
        K::With { .. } | K::WithEnd { .. } => false,
        _ => true,
    }
}

pub fn needs_new_line(_it: &It) -> bool {
    false
}

pub fn handles_new_line(it: &It) -> bool {
    matches!(it.borrow().k, K::TraitSlotConst { .. } | K::With { .. })
}

pub fn has_single_new_line_around(it: &It) -> bool {
    matches!(it.borrow().k, K::TraitSlotConst { .. })
}

pub fn is_empty(it: &It) -> bool {
    match &it.borrow().k {
        K::NewFunction { name, .. } => name.is_empty(),
        K::StoreNewActivation { .. } => true,
        _ => false,
    }
}

fn through_value(it: &It) -> bool {
    matches!(it.borrow().k, K::Push { .. } | K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. })
}

pub fn not_coerced(it: &It) -> It {
    let b = it.borrow();
    match &b.k {
        K::Push { .. } | K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. } | K::Coerce { .. } | K::Convert { .. } => {
            not_coerced(b.value.as_ref().expect("value"))
        }
        _ => it.clone(),
    }
}

pub fn not_coerced_no_dup(it: &It) -> It {
    let b = it.borrow();
    match &b.k {
        K::Push { .. } | K::Coerce { .. } | K::Convert { .. } => not_coerced_no_dup(b.value.as_ref().expect("value")),
        K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. } => it.clone(),
        _ => {
            drop(b);
            not_coerced(it)
        }
    }
}

pub fn through_register(it: &It) -> It {
    if through_value(it) {
        return through_register(&value_of(it).expect("value"));
    }
    if let K::LocalReg { computed: Some(c), .. } = &it.borrow().k {
        return through_register(c);
    }
    it.clone()
}

pub fn through_duplicate(it: &It) -> It {
    if through_value(it) {
        return through_duplicate(&value_of(it).expect("value"));
    }
    it.clone()
}

pub fn through_not_compilable(it: &It) -> It {
    it.clone()
}

/// `invert(src)`.
pub fn invert(it: &It, src: Option<usize>) -> It {
    let b = it.borrow();
    let ls = b.line_start;
    match &b.k {
        K::Un { op: Un::Not, .. } => {
            let v = b.value.clone().expect("value");
            if is::not(&v) {
                return v;
            }
            if !is_type(&return_type(&v), "Boolean") && !is::duplicate(&v) {
                drop(b);
                return not_item(None, None, it.clone());
            }
            v
        }
        K::True => false_item(b.src, ls),
        K::False => true_item(b.src, ls),
        K::Bin { op, l, r } if op.inverted().is_some() => bin(op.inverted().unwrap(), b.src, ls, l.clone(), r.clone()),
        _ => {
            drop(b);
            not_item(src, ls, it.clone())
        }
    }
}

pub fn has_side_effect(it: &It) -> bool {
    let b = it.borrow();
    match &b.k {
        K::Pop | K::Swap | K::Call { .. } | K::CallMethod { .. } | K::CallStatic { .. } | K::CallProperty { .. } | K::CallSuper { .. } => true,
        K::Construct { .. } | K::ConstructProp { .. } | K::ConstructSuper { .. } | K::DecLocal { .. } | K::IncLocal { .. } => true,
        K::PostDecrement { .. } | K::PostIncrement { .. } | K::Un { op: Un::PreIncrement | Un::PreDecrement, .. } => true,
        K::ReturnValue | K::ReturnVoid | K::Throw | K::SetLocal { .. } | K::SetProperty { .. } | K::SetSlot { .. } | K::SetSuper { .. } => true,
        K::Bin { l, r, .. } => has_side_effect(l) || has_side_effect(r),
        K::Un { .. } | K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. } => b.value.as_ref().is_some_and(has_side_effect),
        K::NameValuePair { name } => has_side_effect(name) || b.value.as_ref().is_some_and(has_side_effect),
        K::NewArray { values } => values.borrow().iter().any(has_side_effect),
        K::NewObject { pairs } => pairs.borrow().iter().any(has_side_effect),
        _ => {
            drop(b);
            let mut found = false;
            visit_recursively(it, &mut |i| {
                if !found && has_side_effect(i) {
                    found = true;
                }
                true
            });
            found
        }
    }
}

/// `getFirstPart()`.
pub fn first_part(it: &It) -> Option<PartId> {
    let b = it.borrow();
    match &b.k {
        K::Bin { l, .. } => first_part(l).or(b.first_part),
        K::SetProperty { .. } | K::SetSlot { .. } | K::SetSuper { .. } => b.value.as_ref().and_then(first_part),
        _ => match &b.value {
            Some(v) => first_part(v).or(b.first_part),
            None => b.first_part,
        },
    }
}

pub fn line_start(it: &It) -> Option<usize> {
    it.borrow().line_start
}

pub fn src_of(it: &It) -> Option<usize> {
    it.borrow().src
}

/// Loop id of loop items.
pub fn loop_id(it: &It) -> Option<i64> {
    match &it.borrow().k {
        K::While { loop_id, .. } | K::DoWhile { loop_id, .. } | K::For { loop_id, .. } | K::Switch { loop_id, .. } | K::ForIn { loop_id, .. } | K::ForEachIn { loop_id, .. } => Some(*loop_id),
        _ => None,
    }
}

/// `getContinues()` of block items.
pub fn continues(it: &It) -> Vec<It> {
    collect_kind(it, true)
}

/// `getBreaks()` of block items.
pub fn breaks(it: &It) -> Vec<It> {
    collect_kind(it, false)
}

fn collect_kind(it: &It, cont: bool) -> Vec<It> {
    let lists: Vec<List> = {
        let b = it.borrow();
        match &b.k {
            K::If { on_true, on_false, .. } => vec![on_true.clone(), on_false.clone()],
            K::While { cmds, .. } | K::DoWhile { cmds, .. } | K::For { cmds, .. } | K::ForIn { cmds, .. } | K::ForEachIn { cmds, .. } => vec![cmds.clone()],
            K::Switch { case_cmds, .. } => case_cmds.clone(),
            K::Goto { target, .. } => {
                let mut ret = Vec::new();
                if let Some(t) = target {
                    for c in t.borrow().iter() {
                        if (cont && is::continue_(c)) || (!cont && is::break_(c)) {
                            ret.push(c.clone());
                        }
                    }
                }
                return ret;
            }
            K::Try { try_cmds, catch_cmds, finally, .. } => {
                let mut v = vec![try_cmds.clone()];
                if let Some(f) = finally {
                    v.push(f.clone());
                }
                v.extend(catch_cmds.iter().cloned());
                v
            }
            _ => Vec::new(),
        }
    };
    let mut ret = Vec::new();
    for l in lists {
        for ti in l.borrow().iter() {
            if (cont && is::continue_(ti)) || (!cont && is::break_(ti)) {
                ret.push(ti.clone());
            }
            if is::block(ti) {
                ret.extend(collect_kind(ti, cont));
            }
        }
    }
    ret
}
