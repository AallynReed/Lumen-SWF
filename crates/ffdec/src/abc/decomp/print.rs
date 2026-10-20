//! Rendering items as ActionScript: FFDec's `appendTo` and `toString` family.

use super::ctx::LocalData;
use super::eq::eq;
use super::item::*;
use super::names::Dotted;
use super::props::*;
use super::writer::{LoopType, Writer};
use crate::abc::names::{ecma_number, escape_as};
use crate::abc::Multiname;

type LD<'x, 'c, 'a> = &'x LocalData<'c, 'a>;

/// `toString(writer, localData, implicitCoerce, noParenthesis)`.
pub fn to_string(it: &It, w: &mut Writer, ld: LD, coerce: &str, no_paren: bool) {
    if coerce == "Boolean" && is::not(it) {
        let v = it.borrow().val();
        if is::not(&v) {
            let inner = v.borrow().val();
            return to_string(&inner, w, ld, "Boolean", false);
        }
    }
    append_try(it, w, ld, coerce, no_paren);
}

pub fn ts(it: &It, w: &mut Writer, ld: LD) {
    to_string(it, w, ld, "", false);
}

pub fn ts_bool(it: &It, w: &mut Writer, ld: LD) {
    to_string(it, w, ld, "Boolean", false);
}

/// Renders to a fresh string (`toString(localData)`).
pub fn to_str(it: &It, ld: LD) -> String {
    let mut w = Writer::new();
    ts(it, &mut w, ld);
    w.finish()
}

pub fn append_try(it: &It, w: &mut Writer, ld: LD, coerce: &str, no_paren: bool) {
    let mut t = it.clone();
    if !coerce.is_empty() {
        let rt = return_type(&t);
        let rts = java_to_string(&rt);
        let nums = ["int", "uint", "Number"];
        if !is_unbounded(&rt) && coerce != rts && !(nums.contains(&coerce) && nums.contains(&rts.as_str())) && !(coerce == "Boolean" && rts != "Function") {
            let p = prec_of(&t);
            t = mk(K::Convert { ty: ty(coerce) }, None, None, p, Some(t));
        }
    }
    if no_paren {
        append_no_paren(&t, w, ld);
    } else {
        append_to(&t, w, ld);
    }
}

pub fn append_no_paren(it: &It, w: &mut Writer, ld: LD) {
    let k = it.borrow().k.clone();
    match k.clone() {
        K::Comma { cmds } => {
            let mut first = true;
            for t in cmds.borrow().iter() {
                if is_empty(t) {
                    continue;
                }
                if !first {
                    w.allow_wrap_here().append(",");
                }
                ts(t, w, ld);
                first = false;
            }
        }
        K::Parenthesis => ts(&it.borrow().val(), w, ld),
        _ => append_to(it, w, ld),
    }
}

pub fn to_string_semicoloned(it: &It, w: &mut Writer, ld: LD) {
    append_try(it, w, ld, "", false);
    if needs_semicolon(it) {
        w.append_no_hilight(";");
    }
}

pub fn append_block(w: &mut Writer, ld: LD, cmds: &List) {
    append_commands(w, ld, cmds, true);
}

pub fn append_commands(w: &mut Writer, ld: LD, cmds: &List, as_block: bool) {
    if as_block {
        w.start_block();
    }
    let mut first = true;
    let mut prev: Option<It> = None;
    let cmds: Vec<It> = cmds.borrow().clone();
    for ti in &cmds {
        if !is_empty(ti) {
            if !first {
                if let Some(p) = &prev {
                    let c = p.borrow().comment.clone();
                    if !c.is_empty() {
                        w.append(" //").append(&c);
                    }
                }
                w.new_line();
            }
            first = false;
            to_string_semicoloned(ti, w, ld);
            prev = Some(ti.clone());
        }
    }
    if let Some(p) = &prev {
        let c = p.borrow().comment.clone();
        if !c.is_empty() {
            w.append(" //").append(&c);
        }
    }
    if as_block {
        if !first {
            w.new_line();
        }
        w.end_block();
    }
}

/// `Graph.graphToString`.
pub fn graph_to_string(tree: &[It], w: &mut Writer, ld: LD) {
    let mut last_new_line = true;
    let mut n = tree.len();
    if tree.last().is_some_and(is::script_end) {
        n -= 1;
    }
    for (i, ti) in tree[..n].iter().enumerate() {
        if is_empty(ti) {
            continue;
        }
        if has_single_new_line_around(ti) && !last_new_line {
            w.new_line();
        }
        to_string_semicoloned(ti, w, ld);
        if !handles_new_line(ti) {
            let c = ti.borrow().comment.clone();
            if !c.is_empty() {
                w.append(" //").append(&c);
            }
            w.new_line();
        }
        last_new_line = false;
        if has_single_new_line_around(ti) && i < n - 1 {
            w.new_line();
            last_new_line = true;
        }
    }
}

fn paren_if(w: &mut Writer, ld: LD, it: &It, cond: bool) {
    if cond {
        w.append("(");
        ts(it, w, ld);
        w.append(")");
    } else {
        ts(it, w, ld);
    }
}

fn args(w: &mut Writer, ld: LD, a: &List, wrap: bool) {
    w.append("(");
    for (i, x) in a.borrow().iter().enumerate() {
        if i > 0 {
            if wrap {
                w.allow_wrap_here();
            }
            w.append(",");
        }
        ts(x, w, ld);
    }
    w.append(")");
}

fn hex_operand(w: &mut Writer, ld: LD, operand: &It, wide: bool) {
    let val: Option<i64> = match &operand.borrow().k {
        K::IntegerValueA { v, .. } => Some(*v as i64),
        K::DoubleValue(d) if wide => Some(*d as i64),
        _ => None,
    };
    if let Some(v) = val {
        if v > 9 {
            let mut h = if wide { format!("{:X}", v) } else { format!("{:X}", v as i32) };
            if h.len() % 2 == 1 {
                h.insert(0, '0');
            }
            w.append(&format!("0x{h}"));
            return;
        }
    }
    to_string(operand, w, ld, "", false);
}

fn bin_default(w: &mut Writer, ld: LD, op: Bin, p: u8, l: &It, r: &It) {
    let operand = |w: &mut Writer, x: &It| {
        if op.is_bitwise() {
            hex_operand(w, ld, x, true);
        } else {
            to_string(x, w, ld, "", false);
        }
    };
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
    paren_if(w, ld, l, prec_of(l) > p);
    w.append(" ").append(op.op()).append(" ");
    w.append("(");
    ts(r, w, ld);
    w.append(")");
}

fn loop_label(w: &mut Writer, id: i64, label_used: bool, ty: LoopType) {
    w.start_loop(id, ty);
    if label_used {
        w.append("loop").append_int(id).append(":").new_line();
    }
}

fn loop_end(w: &mut Writer, it: &It, id: i64) {
    if let Some(l) = w.end_loop(id) {
        let mut b = it.borrow_mut();
        match &mut b.k {
            K::While { label_used, .. } | K::DoWhile { label_used, .. } | K::For { label_used, .. } | K::Switch { label_used, .. } => *label_used = l.used,
            K::ForIn { label_used, .. } | K::ForEachIn { label_used, .. } => *label_used = l.used,
            _ => {}
        }
    }
}

fn exprs(w: &mut Writer, ld: LD, expr: &List) {
    let e = expr.borrow().clone();
    let mut first = true;
    for (i, x) in e.iter().enumerate() {
        if is_empty(x) {
            continue;
        }
        if !first {
            w.append(", ");
        }
        first = false;
        if i == e.len() - 1 {
            ts_bool(x, w, ld);
        } else {
            ts(x, w, ld);
        }
    }
}

/// `SetTypeIns.handleSetCoerce`.
pub fn handle_set_coerce(value: &It, ty: Option<&It>) -> It {
    let Some(t) = ty else { return value.clone() };
    if !matches!(value.borrow().k, K::Convert { .. } | K::Coerce { .. }) {
        return value.clone();
    }
    let inner = value.borrow().val();
    let convert_type = return_type(value);
    let inside = return_type(&inner);
    if eq(t, &convert_type) && is_unbounded(&inside) && !is_unbounded(t) {
        return inner;
    }
    if (is_type(t, "int") || is_type(t, "uint")) && is_type(&inside, "Number") {
        return inner;
    }
    value.clone()
}

fn compound_or_value(w: &mut Writer, ld: LD, it: &It, set: &SetInfo, ty: Option<&It>) {
    if let Some(op) = &set.compound_op {
        w.append(" ").append(op).append("= ");
        ts(set.compound_value.as_ref().expect("compound value"), w, ld);
        return;
    }
    w.append(" = ");
    let v = it.borrow().val();
    ts(&handle_set_coerce(&v, ty), w, ld);
}

fn this_class(ld: LD) -> Option<It> {
    if ld.class_index < 0 {
        return None;
    }
    let inst = &ld.abc().instances[ld.class_index as usize];
    Some(ty_dotted(ld.names().name_with_ns(inst.name, false)))
}

/// `AVM2Item.formatProperty`.
pub fn format_property(w: &mut Writer, ld: LD, object: &It, property: &It, is_static: bool, null_cond: bool) {
    let mut property = property.clone();
    let td = through_duplicate(object);
    let mut empty = is::find_property(&td);
    if let K::LocalReg { computed: Some(c), .. } = &object.borrow().k {
        if is::find_property(&through_not_compilable(c)) {
            empty = true;
        }
    }
    if let K::FindProperty { prop: fp } = &td.borrow().k {
        if is::full_multiname(fp) {
            let not_prop = matches!(&property.borrow().k, K::FullMultiname { property: false, .. });
            if not_prop {
                if let K::FullMultiname { property, .. } = &mut fp.borrow_mut().k {
                    *property = false;
                }
            }
            property = fp.clone();
        }
    }
    if !empty {
        if prec_of(object) > prec::PRIMARY || is::integer_a(object) {
            w.append("(");
            ts(object, w, ld);
            w.append(")");
            empty = false;
        } else {
            let before = w.len();
            ts(object, w, ld);
            if before == w.len() {
                empty = true;
            }
        }
    }
    if empty {
        let parent = this_class(ld);
        full_multiname_append(&property, w, ld, false, parent.as_ref(), is_static);
        return;
    }
    if is::full_multiname(&property) {
        let mn = match &property.borrow().k {
            K::FullMultiname { mn, .. } => *mn,
            _ => unreachable!(),
        };
        let rt = return_type(object);
        let operator = if null_cond { "?." } else { "." };
        let names = ld.names();
        let mut valid = true;
        if mn < ld.abc().constants.multinames.len() {
            let is_attr = names.mn(mn).is_some_and(|m| m.is_attribute());
            let (local, _) = names.name_and_custom_ns(mn, &[], true, true);
            let suffix = names.suffix(mn);
            valid = local == "*" || is_attr || !suffix.is_empty() || super::names::is_valid_name(&local);
        }
        if !ld.ctx.cfg.qname_obfuscated_in_brackets {
            valid = true;
        }
        if valid {
            let (has_name, has_ns) = match &property.borrow().k {
                K::FullMultiname { name, ns, .. } => (name.is_some(), ns.is_some()),
                _ => (false, false),
            };
            if !has_name || has_ns {
                w.allow_wrap_here().special(operator);
            }
        }
        full_multiname_append(&property, w, ld, true, Some(&rt), is_static);
    } else {
        w.append("[").allow_wrap_here();
        ts(&property, w, ld);
        w.append("]");
    }
}

/// `FullMultinameAVM2Item.appendTo(writer, localData, afterDot, parentType, isStatic)`.
pub fn full_multiname_append(it: &It, w: &mut Writer, ld: LD, after_dot: bool, parent: Option<&It>, _is_static: bool) {
    let (mn, name, ns) = match &it.borrow().k {
        K::FullMultiname { mn, name, ns, .. } => (*mn, name.clone(), ns.clone()),
        _ => return append_to(it, w, ld),
    };
    if let Some(ns) = &ns {
        ts(ns, w, ld);
        w.append("::");
    }
    if let Some(n) = &name {
        w.append("[");
        ts(n, w, ld);
        w.append("]");
        return;
    }
    let names = ld.names();
    let count = ld.abc().constants.multinames.len();
    if mn == 0 || mn >= count {
        w.append("\u{a7}\u{a7}multiname(").append_int(mn as i64).append(")");
        return;
    }
    let empty: Vec<Dotted> = Vec::new();
    let fqn: &[Dotted] = if after_dot { &empty } else { &ld.fqn };
    let simple = names.name(mn, fqn, true, false);
    if simple == "*" {
        w.append("*");
        return;
    }
    let is_attr = names.mn(mn).is_some_and(|m| m.is_attribute());
    let suffix = names.suffix(mn);
    let bracket_mode = !is_attr && after_dot && suffix.is_empty() && ld.ctx.cfg.qname_obfuscated_in_brackets;
    let (local, custom) = if bracket_mode { names.name_and_custom_ns(mn, fqn, true, true) } else { names.name_and_custom_ns(mn, fqn, false, true) };
    let ambiguous: Option<bool> = match parent {
        Some(p) if matches!(p.borrow().k, K::Type { .. }) => super::index::is_property_ambiguous(ld, mn, p),
        _ => None,
    };
    if ambiguous != Some(false) {
        if let Some(c) = &custom {
            let id = ld.ctx.ids.print(c.last());
            w.special(&id);
            w.append_no_hilight("::");
        } else if ambiguous.is_some() {
            if let Some(ns) = names.single_ns(mn) {
                if let Some(prefix) = crate::abc::names::NS_KINDS.iter().find(|k| k.0 == names.ns_kind(ns)).map(|k| k.2) {
                    w.append(prefix).append("::");
                }
            }
        }
    }
    if bracket_mode {
        if super::names::is_valid_name(&local) {
            w.append(&local);
        } else if local == "0" || (local.starts_with(|c: char| ('1'..='9').contains(&c)) && local.bytes().all(|b| b.is_ascii_digit())) {
            w.append("[").append(&local).append("]");
        } else {
            w.append("[\"").append(&escape_as(&local)).append("\"]");
        }
    } else {
        w.append(&local);
    }
}

fn type_append(full: &Dotted, w: &mut Writer, ld: LD) {
    if ld.fqn.contains(&Dotted::parse_with_suffix(full.last())) {
        w.special(&full.printable(&ld.ctx.ids));
    } else {
        w.special(&ld.ctx.ids.print(full.last()));
    }
}

fn float_str(f: f32) -> String {
    let mut s = ecma_number(f as f64);
    if f.is_finite() {
        s.push('f');
    }
    s
}

fn slot_name_str(ld: LD, slot_name: Option<usize>, slot_obj: &It, idx: i32) -> String {
    match slot_name {
        Some(n) => ld.names().name(n, &ld.fqn, false, true),
        None => format!("{}.\u{a7}\u{a7}slot[{idx}]", to_str(slot_obj, ld)),
    }
}

fn exception_var_name(ld: LD, e: &crate::abc::Exception) -> String {
    if e.var_name == 0 {
        return "_loc_e_".into();
    }
    ld.names().name(e.var_name, &ld.fqn, false, true)
}

fn exception_type_name(ld: LD, e: &crate::abc::Exception) -> String {
    if e.type_name == 0 {
        return "*".into();
    }
    ld.names().name(e.type_name, &ld.fqn, false, true)
}

fn get_definition(w: &mut Writer, ld: LD, prop: &It) -> bool {
    let (mn, name) = match &prop.borrow().k {
        K::FullMultiname { mn, name, .. } => (*mn, name.clone()),
        _ => return false,
    };
    if mn == 0 {
        return false;
    }
    let names = ld.names();
    let Some(Multiname::MultinameL { attr: false, .. }) = names.mn(mn) else { return false };
    let Some(set) = names.ns_set(mn) else { return false };
    if set.len() != 1 {
        return false;
    }
    w.append("(");
    if ld.fqn.contains(&Dotted::parse_no_suffix("flash.utils.getDefinitionByName")) {
        w.append("flash.utils.getDefinitionByName");
    } else {
        w.append("getDefinitionByName");
    }
    w.append("(");
    let nname = names.ns_name(set[0]).raw();
    if !nname.is_empty() {
        w.append("\"").append(&escape_as(&nname)).append("\"+\".\"+");
    }
    if let Some(n) = name {
        append_to(&n, w, ld);
    }
    w.append("))");
    true
}

fn super_receiver(w: &mut Writer, ld: LD, obj: &It) {
    if java_to_string(obj) != "this" && !is::find_property(&through_duplicate(obj)) {
        ts(obj, w, ld);
        w.append(".");
    }
}

pub fn append_to(it: &It, w: &mut Writer, ld: LD) {
    let (k, p, value) = {
        let b = it.borrow();
        (b.k.clone(), b.prec, b.value.clone())
    };
    let val = || value.clone().expect("item value");
    match k.clone() {
        K::Bin { op, l, r } => match op {
            Bin::Add | Bin::Sub | Bin::Div | Bin::Mod if prec_of(&r) >= p => bin_right_paren(w, ld, op, p, &l, &r),
            Bin::Mul if matches!(r.borrow().k, K::Bin { op: Bin::Mod, .. }) => bin_right_paren(w, ld, op, p, &l, &r),
            _ => bin_default(w, ld, op, p, &l, &r),
        },
        K::Un { op, .. } => {
            w.append(op.op());
            match &value {
                Some(v) => {
                    let paren = prec_of(v) > p;
                    if paren {
                        w.append("(");
                    }
                    if op == Un::BitNot {
                        hex_operand(w, ld, v, false);
                    } else {
                        to_string(v, w, ld, "", false);
                    }
                    if paren {
                        w.append(")");
                    }
                }
                None => {
                    w.append("null");
                }
            }
        }
        K::Ternar { expr, on_true, on_false } => {
            let ep = prec_of(&expr) >= p;
            if ep {
                w.append("(");
            }
            to_string(&expr, w, ld, "Boolean", true);
            if ep {
                w.append(")");
            }
            w.append(" ? ");
            for (i, x) in [&on_true, &on_false].into_iter().enumerate() {
                if i == 1 {
                    w.append(" : ");
                }
                let xp = prec_of(x);
                let paren = xp >= p && xp != prec::NOPRECEDENCE;
                if paren {
                    w.append("(");
                }
                to_string(x, w, ld, "", true);
                if paren {
                    w.append(")");
                }
            }
        }
        K::If { expr, on_true, on_false, .. } => {
            let mut e = expr.clone();
            let (mut a, mut b) = (on_true.clone(), on_false.clone());
            if on_true.borrow().is_empty() {
                if on_false.borrow().is_empty() {
                    if is::not(&e) {
                        let v = e.borrow().val();
                        e = v;
                    }
                } else {
                    e = invert(&e, None);
                    a = on_false.clone();
                    b = on_true.clone();
                }
            }
            w.append("if(");
            ts_bool(&e, w, ld);
            w.append(")");
            append_block(w, ld, &a);
            let bl = b.borrow().clone();
            if !bl.is_empty() {
                let else_if = bl.len() == 1 && is::if_(&bl[0]);
                w.new_line();
                w.append("else");
                if !else_if {
                    append_block(w, ld, &b);
                } else {
                    w.append(" ");
                    to_string_semicoloned(&bl[0], w, ld);
                }
            }
        }
        K::While { loop_id, expr, cmds, label_used, .. } => {
            loop_label(w, loop_id, label_used, LoopType::Loop);
            w.append("while(");
            exprs(w, ld, &expr);
            w.append(")");
            append_block(w, ld, &cmds);
            loop_end(w, it, loop_id);
        }
        K::DoWhile { loop_id, cmds, expr, label_used } => {
            loop_label(w, loop_id, label_used, LoopType::Loop);
            w.append("do");
            append_block(w, ld, &cmds);
            w.new_line();
            w.append("while(");
            exprs(w, ld, &expr);
            w.append(")");
            loop_end(w, it, loop_id);
        }
        K::For { loop_id, first, expr, fin, cmds, label_used } => {
            loop_label(w, loop_id, label_used, LoopType::Loop);
            w.append("for(");
            let mut n = 0;
            for x in first.borrow().iter() {
                if is_empty(x) {
                    continue;
                }
                if n > 0 {
                    w.append(",");
                }
                ts(x, w, ld);
                n += 1;
            }
            w.append("; ");
            ts_bool(&expr, w, ld);
            w.append("; ");
            n = 0;
            for x in fin.borrow().iter() {
                if is_empty(x) {
                    continue;
                }
                if n > 0 {
                    w.append(",");
                }
                ts(x, w, ld);
                n += 1;
            }
            w.append(")");
            append_block(w, ld, &cmds);
            loop_end(w, it, loop_id);
        }
        K::Switch { loop_id, obj, case_values, case_cmds, mapping, label_used, .. } => {
            loop_label(w, loop_id, label_used, LoopType::Switch);
            w.append("switch(");
            ts(&obj, w, ld);
            w.append(")").start_block();
            let cv = case_values.borrow().clone();
            for (i, cc) in case_cmds.iter().enumerate() {
                for (k, &m) in mapping.iter().enumerate() {
                    if m == i as i32 {
                        if !matches!(cv[k].borrow().k, K::Default) {
                            w.append("case ");
                        }
                        to_string(&cv[k], w, ld, "", true);
                        w.append(":").new_line();
                    }
                }
                w.indent();
                for c in cc.borrow().clone().iter() {
                    if !is_empty(c) {
                        to_string_semicoloned(c, w, ld);
                        w.new_line();
                    }
                }
                w.unindent();
            }
            w.end_block();
            loop_end(w, it, loop_id);
        }
        K::Break { loop_id, label_required, is_script_end: _ } => {
            let mut req = label_required;
            if w.is_nul() {
                req = loop_id != w.current_loop();
                if req {
                    w.set_loop_used(loop_id);
                }
                if let K::Break { label_required, .. } = &mut it.borrow_mut().k {
                    *label_required = req;
                }
            }
            w.append("break");
            if req {
                w.append(" loop").append_int(loop_id);
            }
        }
        K::Continue { loop_id, label_required } => {
            w.append("continue");
            let mut req = label_required;
            if w.is_nul() {
                req = loop_id != w.non_switch_loop();
                if req {
                    w.set_loop_used(loop_id);
                }
                if let K::Continue { label_required, .. } = &mut it.borrow_mut().k {
                    *label_required = req;
                }
            }
            if req {
                w.append(" loop").append_int(loop_id);
            }
        }
        K::Label { name } => {
            w.append(&name).append(":");
        }
        K::Goto { name, target } => match target {
            Some(t) => {
                if !name.is_empty() {
                    w.append(&name).append(":").new_line();
                }
                append_commands(w, ld, &t, false);
            }
            None => {
                w.append("\u{a7}\u{a7}goto(").append(&name).append(")");
            }
        },
        K::Push { as_comment } => {
            if as_comment {
                append_try(&val(), w, ld, "", false);
            } else {
                w.append("\u{a7}\u{a7}push(");
                append_try(&val(), w, ld, "", false);
                w.append(")");
            }
        }
        K::Pop => {
            w.append("\u{a7}\u{a7}pop()");
        }
        K::Duplicate { temp } => {
            if temp == 0 {
                w.append("\u{a7}\u{a7}dup(");
                append_try(&val(), w, ld, "", false);
                w.append(")");
            } else {
                w.append("_temp_").append_int(temp as i64);
            }
        }
        K::DuplicateSource { temp, declaration } => {
            if temp == 0 {
                w.append("\u{a7}\u{a7}dupsrc(");
                append_try(&val(), w, ld, "", false);
                w.append(")");
            } else {
                let v = skip_temp(&val(), temp);
                if declaration {
                    temp_declaration(w, ld, "", temp, Some(&v));
                } else {
                    w.append("_temp_").append_int(temp as i64);
                }
            }
        }
        K::SetTemporary { temp, declaration, .. } => {
            let v = value.clone().map(|v| skip_temp(&v, temp));
            if declaration {
                temp_declaration(w, ld, "", temp, v.as_ref());
            } else {
                w.append("_temp_").append_int(temp as i64).append(" = ");
                append_try(&val(), w, ld, "", false);
            }
        }
        K::Temporary { temp } => {
            w.append("_temp_").append_int(temp as i64);
        }
        K::Swap => {
            w.append("\u{a7}\u{a7}swap()");
        }
        K::Comma { cmds } => {
            if !cmds.borrow().is_empty() {
                w.append("(");
                append_no_paren(it, w, ld);
                w.append(")");
            }
        }
        K::Parenthesis => {
            w.append("(");
            append_no_paren(it, w, ld);
            w.append(")");
        }
        K::True => {
            w.append("true");
        }
        K::False => {
            w.append("false");
        }
        K::IntegerValue(v) => {
            w.append_int(v as i64);
        }
        K::Comment(lines) => {
            let n = lines.iter().filter(|l| l.is_some()).count();
            w.append(if n == 1 { "// " } else { "/* " });
            for (i, l) in lines.iter().enumerate() {
                if let Some(l) = l {
                    w.append(l);
                    if i != lines.len() - 1 {
                        w.new_line();
                    }
                }
            }
            if n > 1 {
                w.append(" */");
            }
        }
        K::ScriptEnd => {
            w.append("return;");
        }
        K::Empty => {}
        K::Default => {
            w.append("default");
        }
        K::Any => {
            w.append("\u{a7}\u{a7}any()");
        }
        K::Block { cmds } => {
            w.start_block();
            graph_to_string(&cmds.borrow().clone(), w, ld);
            w.end_block();
        }
        K::UnboundedType => {
            w.append("*");
        }
        K::Type { full, .. } => type_append(&full, w, ld),
        K::TypeFunction(n) => {
            w.append(&n);
        }
        K::AlchemyLoad { ty: t, size, ofs } => {
            let ts_ = if t == "f4" { "f32x4".to_string() } else { format!("{t}{size}") };
            w.append("l").append(&ts_).append("(");
            ts(&ofs, w, ld);
            w.append(")");
        }
        K::AlchemySignExtend { size } => {
            w.append("si").append_int(size as i64).append("(");
            ts(&val(), w, ld);
            w.append(")");
        }
        K::AlchemyStore { ty: t, size, ofs } => {
            let ts_ = if t == "f4" { "f32x4".to_string() } else { format!("{t}{size}") };
            w.append("s").append(&ts_).append("(");
            ts(&val(), w, ld);
            w.append(",");
            ts(&ofs, w, ld);
            w.append(")");
        }
        K::ApplyType { obj, params } => {
            ts(&obj, w, ld);
            let ps = params.borrow().clone();
            if !ps.is_empty() {
                w.append(".<");
                for (i, x) in ps.iter().enumerate() {
                    if i > 0 {
                        w.append(",");
                    }
                    if matches!(x.borrow().k, K::Null) {
                        w.append("*");
                    } else {
                        ts(x, w, ld);
                    }
                }
                w.append(">");
            }
        }
        K::Boolean(b) => {
            w.append(if b { "true" } else { "false" });
        }
        K::Call { function, args: a, .. } => {
            paren_if(w, ld, &function, prec_of(&function) > p || is::new_function(&function));
            args(w, ld, &a, true);
        }
        K::CallMethod { receiver, name, args: a } => {
            paren_if(w, ld, &receiver, prec_of(&receiver) > p || is::integer_a(&receiver));
            w.allow_wrap_here().append(".");
            w.append(&name);
            args(w, ld, &a, true);
        }
        K::CallStatic { receiver, name, args: a } => {
            paren_if(w, ld, &receiver, prec_of(&receiver) > p || is::integer_a(&receiver));
            w.append(".").append(&name);
            args(w, ld, &a, false);
        }
        K::CallProperty { receiver, prop, args: a, is_static, .. } => {
            format_property(w, ld, &receiver, &prop, is_static, false);
            args(w, ld, &a, true);
        }
        K::CallSuper { receiver, mn, args: a, .. } => {
            super_receiver(w, ld, &receiver);
            w.append("super.");
            ts(&mn, w, ld);
            args(w, ld, &a, false);
        }
        K::Class { mn, name } => {
            if let Some(n) = name {
                if ld.fqn.contains(&n) {
                    w.append(&n.printable(&ld.ctx.ids));
                } else {
                    w.append(&ld.ctx.ids.print(n.last()));
                }
            } else if let Some(m) = mn {
                w.append(&ld.names().name(m, &ld.fqn, false, true));
            }
        }
        K::Construct { obj, args: a } => {
            w.append("new ");
            if is::new_function(&obj) {
                ts(&obj, w, ld);
                return;
            }
            let is_call = matches!(obj.borrow().k, K::Call { .. } | K::CallProperty { .. } | K::CallMethod { .. } | K::CallStatic { .. } | K::CallSuper { .. });
            let mut is_def = false;
            if let K::GetProperty { prop, .. } = &obj.borrow().k {
                is_def = get_definition(w, ld, prop);
            }
            if !is_def {
                paren_if(w, ld, &obj, prec_of(&obj) > p || is_call);
            }
            if matches!(obj.borrow().k, K::InitVector { .. }) {
                return;
            }
            args(w, ld, &a, true);
        }
        K::ConstructProp { obj, prop, args: a, is_static, .. } => {
            w.append("new ");
            if !get_definition(w, ld, &prop) {
                format_property(w, ld, &obj, &prop, is_static, false);
            }
            args(w, ld, &a, true);
        }
        K::ConstructSuper { obj, args: a } => {
            if java_to_string(&obj) != "this" {
                ts(&obj, w, ld);
                w.allow_wrap_here().append(".");
            }
            w.append("super");
            args(w, ld, &a, true);
        }
        K::Coerce { ty: t } => {
            let v = val();
            let vrt = return_type(&v);
            let show = match java_to_string(&t).as_str() {
                "Boolean" => !is_type(&vrt, "Boolean"),
                "Number" => !is_type(&vrt, "int") && !is_type(&vrt, "Number") && !is_type(&vrt, "uint"),
                "float" => !is_type(&vrt, "int") && !is_type(&vrt, "float") && !is_type(&vrt, "uint"),
                "int" => !is_type(&vrt, "int"),
                "uint" => uint_display(&v, &vrt),
                _ => false,
            };
            convert_like(w, ld, &t, &v, show);
        }
        K::Convert { ty: t } => {
            let v = val();
            let vrt = return_type(&v);
            let show = match java_to_string(&t).as_str() {
                "Boolean" => !is_type(&vrt, "Boolean"),
                "Number" => !is_type(&vrt, "int") && !is_type(&vrt, "Number") && !is_type(&vrt, "uint"),
                "float" => !is_type(&vrt, "int") && !is_type(&vrt, "float") && !is_type(&vrt, "uint"),
                "int" => !is_type(&vrt, "int"),
                "uint" => uint_display(&v, &vrt),
                "String" => !is_type(&vrt, "String") && !is_type(&vrt, "null"),
                _ => true,
            };
            convert_like(w, ld, &t, &v, show);
        }
        K::DNan | K::Nan => {
            w.append("NaN");
        }
        K::DecLocal { reg, .. } => {
            w.append(&ld.reg_name(reg)).append("--");
        }
        K::IncLocal { reg, .. } => {
            w.append(&ld.reg_name(reg)).append("++");
        }
        K::DecimalValue(d) => {
            w.append(&super::values::decimal_to_string(&d));
        }
        K::DoubleValue(d) => {
            w.append(&ecma_number(d));
        }
        K::FloatValue(f) => {
            w.append(&float_str(f));
        }
        K::Float4Value(v) => {
            w.append("float4(");
            for (i, f) in v.iter().enumerate() {
                if i > 0 {
                    w.append(",");
                }
                w.append(&float_str(*f));
            }
            w.append(")");
        }
        K::IntegerValueA { formatted, .. } => {
            w.append(&formatted);
        }
        K::Declaration { assignment, ty: t, show_value, .. } => declaration_append(w, ld, &assignment, t.as_ref(), show_value),
        K::Decrement | K::Increment => {
            let v = val();
            paren_if(w, ld, &v, prec_of(&v) > p);
            w.append(if matches!(k, K::Decrement) { " - 1" } else { " + 1" });
        }
        K::DefaultXmlNs { ns } => {
            w.append("default xml namespace = ");
            ts(&ns, w, ld);
        }
        K::DeleteProperty { obj, prop, is_static } => {
            w.append("delete ");
            format_property(w, ld, &obj, &prop, is_static, false);
        }
        K::EscapeXAttr | K::EscapeXElem => {
            w.append("{");
            ts(&val(), w, ld);
            w.append("}");
        }
        K::ExceptionItem(e) => {
            w.append(&exception_var_name(ld, &e));
        }
        K::Filter { expr, coll } => {
            ts(&coll, w, ld);
            w.append(".(");
            to_string(&expr, w, ld, "Boolean", true);
            w.append(")");
        }
        K::FilteredCheck { obj } => {
            w.append("\u{a7}\u{a7}checkfilter(");
            ts(&obj, w, ld);
            w.append(")");
        }
        K::FindDef { mn } => {
            let names = ld.names();
            let ns = names.own_ns(mn);
            w.append(&names.ns_name(ns).printable(&ld.ctx.ids));
        }
        K::FindProperty { prop } => {
            w.append("\u{a7}\u{a7}findproperty(");
            append_to(&prop, w, ld);
            w.append(")");
        }
        K::ForEachIn { loop_id, expr, cmds, label_used } | K::ForIn { loop_id, expr, cmds, label_used } => {
            loop_label(w, loop_id, label_used, LoopType::Loop);
            w.append(if matches!(k, K::ForEachIn { .. }) { "for each(" } else { "for(" });
            ts(&expr, w, ld);
            w.append(")");
            append_block(w, ld, &cmds);
            loop_end(w, it, loop_id);
        }
        K::FullMultiname { .. } => full_multiname_append(it, w, ld, false, None, false),
        K::GetDescendants { obj, mn } => {
            paren_if(w, ld, &obj, prec_of(&obj) > p);
            w.append("..");
            ts(&mn, w, ld);
        }
        K::GetLex { mn, .. } => {
            let names = ld.names();
            let (local, custom) = names.name_and_custom_ns(mn, &ld.fqn, false, true);
            if let Some(c) = custom {
                let id = ld.ctx.ids.print(c.last());
                let ambiguous = this_class(ld).and_then(|t| super::index::is_property_ambiguous_name(ld, &local, &t));
                if ambiguous != Some(false) {
                    w.special(&id);
                    w.append_no_hilight("::");
                }
            }
            w.append(&local);
        }
        K::GetProperty { obj, prop, is_static, null_cond, .. } => format_property(w, ld, &obj, &prop, is_static, null_cond),
        K::GetSlot { slot_name, slot_obj, idx, .. } => {
            w.append(&slot_name_str(ld, slot_name, &slot_obj, idx));
        }
        K::GetSuper { obj, prop, .. } => {
            super_receiver(w, ld, &obj);
            w.append("super.");
            ts(&prop, w, ld);
        }
        K::Global => {
            w.append("global");
        }
        K::InFor { object, collection } => {
            ts(&object, w, ld);
            w.append(" in ");
            ts(&collection, w, ld);
        }
        K::HasNext { index, obj } => {
            w.append("\u{a7}\u{a7}hasnext(");
            append_try(&obj, w, ld, "", false);
            w.append(",");
            append_try(&index, w, ld, "", false);
            w.append(")");
        }
        K::InitProperty { obj, prop, set, is_static, .. } => {
            format_property(w, ld, &obj, &prop, is_static, false);
            if let Some(op) = &set.compound_op {
                w.append(" ").append(op).append("= ");
                ts(set.compound_value.as_ref().expect("compound"), w, ld);
            } else {
                w.append(" = ");
                ts(&val(), w, ld);
            }
        }
        K::InitVector { subtype, args: a } => {
            w.append("<");
            append_try(&subtype, w, ld, "", false);
            w.append(">[");
            for (i, x) in a.borrow().iter().enumerate() {
                if i > 0 {
                    w.append(",");
                }
                append_try(x, w, ld, "", false);
            }
            w.append("]");
        }
        K::LocalReg { reg, computed, .. } => {
            if let Some(c) = &computed {
                if matches!(c.borrow().k, K::Filter { .. }) {
                    ts(c, w, ld);
                    return;
                }
            }
            w.append(&ld.reg_name(reg));
        }
        K::NameSpace(ns) => {
            if ns == 0 {
                w.append("*");
                return;
            }
            let names = ld.names();
            if let Some(dc) = names.custom_ns_of_namespace(ns) {
                if !dc.is_empty() {
                    let id = ld.ctx.ids.print(dc.last());
                    if !id.is_empty() {
                        w.append(&id);
                        return;
                    }
                }
            }
            w.append("new Namespace(");
            w.append("\"").append(&escape_as(&names.ns_raw_name(ns))).append("\"");
            w.append(")");
        }
        K::NameValuePair { name } => {
            let nc = not_coerced(&name);
            let needs = !matches!(nc.borrow().k, K::DecimalValue(_) | K::DoubleValue(_) | K::FloatValue(_) | K::Float4Value(_) | K::IntegerValueA { .. } | K::Str(_));
            if needs {
                w.append("(");
            }
            let mut n = name.clone();
            let conv_str = matches!(&name.borrow().k, K::Convert { ty: t } if is_type(t, "String"));
            if conv_str {
                n = name.borrow().val();
                if let K::NameValuePair { name: nm } = &mut it.borrow_mut().k {
                    *nm = n.clone();
                }
            }
            ts(&n, w, ld);
            if needs {
                w.append(")");
            }
            w.append(":");
            let v = val();
            paren_if(w, ld, &v, is::ternar(&v));
        }
        K::NewActivation => {
            w.append("\u{a7}\u{a7}newactivation()");
        }
        K::NewArray { values } => {
            w.append("[");
            for (i, x) in values.borrow().iter().enumerate() {
                if i > 0 {
                    w.allow_wrap_here().append(",");
                }
                ts(x, w, ld);
            }
            w.append("]");
        }
        K::NewFunction { .. } => super::method::print_new_function(it, w, ld),
        K::NewObject { pairs } => {
            let ps = pairs.borrow().clone();
            let single = ps.len() < 2;
            w.append("{");
            if !single {
                w.new_line();
                w.indent();
            }
            for (i, x) in ps.iter().enumerate() {
                if i > 0 {
                    w.append(",").new_line();
                }
                ts(x, w, ld);
            }
            if !single {
                w.new_line();
                w.unindent();
            }
            w.append("}");
        }
        K::NextName { index, obj, local_reg } | K::NextValue { index, obj, local_reg } => {
            if let Some(r) = local_reg {
                append_to(&r, w, ld);
                return;
            }
            w.append(if matches!(k, K::NextName { .. }) { "\u{a7}\u{a7}nextname(" } else { "\u{a7}\u{a7}nextvalue(" });
            ts(&index, w, ld);
            w.append(",");
            ts(&obj, w, ld);
            w.append(")");
        }
        K::Null => {
            w.append("null");
        }
        K::Undefined => {
            w.append("undefined");
        }
        K::Package { items, name } => {
            w.append_no_hilight("package");
            if !name.is_empty() {
                w.append_no_hilight(&format!(" {}", Dotted::parse_with_suffix(&name).printable(&ld.ctx.ids)));
            }
            w.start_block();
            graph_to_string(&items.borrow().clone(), w, ld);
            w.end_block();
            w.new_line();
        }
        K::PostDecrement { obj, .. } => {
            ts(&obj, w, ld);
            w.append("--");
        }
        K::PostIncrement { obj, .. } => {
            ts(&obj, w, ld);
            w.append("++");
        }
        K::RegExp { pattern, modifier } => match super::values::escape_regexp(&pattern) {
            Some(e) => {
                w.append("/").append(&e).append("/").append(&modifier);
            }
            None => {
                w.append("new RegExp(");
                w.append(&format!("\"{}\"", escape_as(&pattern)));
                if !modifier.is_empty() {
                    w.append(",").append(&format!("\"{modifier}\""));
                }
                w.append(")");
            }
        },
        K::ReturnValue => {
            w.append("return ");
            ts(&val(), w, ld);
        }
        K::ReturnVoid => {
            w.append("return");
        }
        K::Throw => {
            w.append("throw ");
            ts(&val(), w, ld);
        }
        K::SetLocal { reg, set, ty: t, hide_value, .. } => {
            w.append(&ld.reg_name(reg));
            if hide_value {
                return;
            }
            compound_or_value(w, ld, it, &set, Some(&t));
        }
        K::SetProperty { obj, prop, set, ty: t, is_static, .. } => {
            format_property(w, ld, &obj, &prop, is_static, false);
            compound_or_value(w, ld, it, &set, Some(&t));
        }
        K::SetSlot { slot_name, slot_obj, idx, set, ty: t, .. } => {
            let name = slot_name_str(ld, slot_name, &slot_obj, idx);
            if name == to_str(&val(), ld) {
                return;
            }
            w.append(&name);
            compound_or_value(w, ld, it, &set, Some(&t));
        }
        K::SetSuper { obj, prop, set, ty: t, .. } => {
            super_receiver(w, ld, &obj);
            w.append("super.");
            ts(&prop, w, ld);
            compound_or_value(w, ld, it, &set, Some(&t));
        }
        K::StoreNewActivation { .. } => {}
        K::Str(s) => {
            w.append("\"").append(&escape_as(&s)).append("\"");
        }
        K::This { class_name, show_class, .. } => {
            if show_class {
                if let Some(c) = &class_name {
                    if ld.fqn.contains(c) {
                        w.append(&c.printable(&ld.ctx.ids)).append(".this");
                    } else {
                        w.append(&ld.ctx.ids.print(c.last())).append(".this");
                    }
                    return;
                }
            }
            w.append("this");
        }
        K::TraitSlotConst { .. } => super::method::print_trait_slot_const(it, w, ld),
        K::Try { try_cmds, catch_ex, catch_cmds, finally, fin_catch_name } => {
            w.append("try");
            append_block(w, ld, &try_cmds);
            for (e, ex) in catch_ex.iter().enumerate() {
                w.new_line();
                w.append("catch(");
                let mut name = exception_var_name(ld, ex);
                if name.is_empty() {
                    name = fin_catch_name.clone();
                }
                w.special(&name);
                w.append(":");
                w.special(&exception_type_name(ld, ex));
                w.append(")");
                append_block(w, ld, &catch_cmds[e]);
            }
            let fin = finally.clone().unwrap_or_else(empty_list);
            if catch_ex.is_empty() || !fin.borrow().is_empty() {
                w.new_line();
                w.append("finally");
                append_block(w, ld, &fin);
            }
        }
        K::Unparsed(s) => {
            w.append(&s);
        }
        K::With { scope, .. } => {
            w.append("with(");
            ts(&scope, w, ld);
            w.append(")").start_block();
        }
        K::WithEnd { .. } => {
            w.end_block();
        }
        K::WithObject { .. } => {}
        K::Xml { parts } => super::values::xml_append(&parts, w, ld),
    }
}

fn uint_display(v: &It, vrt: &It) -> bool {
    if is_type(vrt, "int") {
        if let K::IntegerValueA { v: n, .. } = v.borrow().k {
            return n < 0;
        }
    }
    !is_type(vrt, "uint")
}

fn convert_like(w: &mut Writer, ld: LD, t: &It, v: &It, show: bool) {
    if show {
        ts(t, w, ld);
        w.append("(");
    }
    ts(v, w, ld);
    if show {
        w.append(")");
    }
}

fn skip_temp(v: &It, temp: i32) -> It {
    let mut v = v.clone();
    loop {
        let next = {
            let b = v.borrow();
            match &b.k {
                K::Duplicate { temp: t } | K::DuplicateSource { temp: t, .. } | K::SetTemporary { temp: t, .. } | K::Temporary { temp: t } if *t == temp => b.value.clone(),
                _ => None,
            }
        };
        match next {
            Some(n) => v = n,
            None => return v,
        }
    }
}

/// `AVM2GraphTargetDialect.writeTemporaryDeclaration`.
fn temp_declaration(w: &mut Writer, ld: LD, suffix: &str, temp: i32, value: Option<&It>) {
    w.append("var _temp").append(suffix).append("_").append_int(temp as i64).append(":*");
    if let Some(v) = value {
        w.append(" = ");
        append_try(v, w, ld, "", false);
    }
}

fn declared_type(w: &mut Writer, ld: LD, t: Option<&It>) {
    if let Some(t) = t {
        if !is_unbounded(t) && !is_unknown(t) {
            w.append(":");
            append_try(t, w, ld, "", false);
        }
    }
}

fn declaration_append(w: &mut Writer, ld: LD, assignment: &It, t: Option<&It>, show_value: bool) {
    let k = assignment.borrow().k.clone();
    let typed = |w: &mut Writer| {
        w.append(":");
        match t {
            Some(t) => append_try(t, w, ld, "", false),
            None => {}
        }
    };
    let value = |w: &mut Writer| {
        if show_value {
            w.append(" = ");
            let v = assignment.borrow().val();
            ts(&handle_set_coerce(&v, t), w, ld);
        }
    };
    match k {
        K::LocalReg { reg, .. } => {
            w.append("var ").append(&ld.reg_name(reg));
            declared_type(w, ld, t);
        }
        K::GetSlot { slot_name, slot_obj, idx, .. } => {
            w.append("var ").append(&slot_name_str(ld, slot_name, &slot_obj, idx));
            declared_type(w, ld, t);
        }
        K::SetLocal { reg, .. } => {
            w.append("var ").append(&ld.reg_name(reg));
            typed(w);
            value(w);
        }
        K::SetSlot { slot_name, slot_obj, idx, .. } => {
            w.append("var ").append(&slot_name_str(ld, slot_name, &slot_obj, idx));
            typed(w);
            value(w);
        }
        K::SetProperty { prop, .. } => {
            let resolved = match &prop.borrow().k {
                K::FullMultiname { resolved, .. } => resolved.clone().unwrap_or_default(),
                _ => String::new(),
            };
            w.append("var ").append(&ld.ctx.ids.print(&resolved));
            typed(w);
            value(w);
        }
        _ => {
            w.append("var ");
            ts(assignment, w, ld);
            typed(w);
        }
    }
}
