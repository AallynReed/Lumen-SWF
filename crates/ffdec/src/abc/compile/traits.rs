//! Methods, classes, traits and the script initializer: the declaration half of
//! `AVM2SourceGenerator`.

use std::collections::HashMap;
use std::rc::Rc;

use super::ast::*;
use super::gen::{err, int_ins, set_loc, Cx, Exc, Ld};
use super::index::{raw_names, Owner};
use super::ins::{self, code_len, ins, op, Code, Pseudo};
use super::pool;
use super::{CompileError, R};
use crate::abc::decomp::names::Dotted;
use crate::abc::{instance_flags, method_flags, ns_kind, trait_attr, Class, Instance, MethodBody, MethodInfo, Metadata, Multiname, Trait, TraitData, ValueKind};

/// Where a generated trait lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraitRef {
    pub owner: Owner,
    pub index: usize,
}

const MASK_PREFIX: &str = "_loc";
const MASK_SUFFIX: &str = "_";
const UNUSED: &str = "~~unused";

fn mask_reg(name: &str) -> Option<usize> {
    let d = name.strip_prefix(MASK_PREFIX)?.strip_suffix(MASK_SUFFIX)?;
    if d.is_empty() || !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    d.parse().ok()
}

/// Java's `HashMap` iteration order for a few string keys.
pub fn java_hash_order(keys: &[String]) -> Vec<usize> {
    let hash = |s: &str| -> u32 {
        let mut h: i32 = 0;
        for c in s.encode_utf16() {
            h = h.wrapping_mul(31).wrapping_add(c as i32);
        }
        let h = h as u32;
        h ^ (h >> 16)
    };
    let mut n = 16usize;
    while keys.len() as f64 > n as f64 * 0.75 {
        n *= 2;
    }
    let mut idx: Vec<usize> = (0..keys.len()).collect();
    idx.sort_by_key(|&i| (hash(&keys[i]) as usize) & (n - 1));
    idx
}

pub fn value_kind_opt(v: (usize, u8)) -> Option<ValueKind> {
    if v.0 == 0 {
        None
    } else {
        Some(ValueKind { index: v.0, kind: v.1 })
    }
}

pub struct ClassItems<'a> {
    pub imported: &'a Imports,
    pub opened: &'a NsList,
}

impl Cx {
    pub fn trait_mut(&mut self, r: TraitRef) -> &mut Trait {
        match r.owner {
            Owner::Script(s) => &mut self.abc.scripts[s].traits[r.index],
            Owner::Instance(c) => &mut self.abc.instances[c].traits[r.index],
            Owner::Class(c) => &mut self.abc.classes[c].traits[r.index],
        }
    }

    fn traits_of(&mut self, o: Owner) -> &mut Vec<Trait> {
        match o {
            Owner::Script(s) => &mut self.abc.scripts[s].traits,
            Owner::Instance(c) => &mut self.abc.instances[c].traits,
            Owner::Class(c) => &mut self.abc.classes[c].traits,
        }
    }

    /// `addClass`: inserts an empty class, moving later class indices up.
    pub fn add_class(&mut self, index: usize, inst: Instance, class: Class) {
        let shift = |t: &mut Trait| {
            if let TraitData::Class { class, .. } = &mut t.data {
                if *class >= index {
                    *class += 1;
                }
            }
        };
        for s in &mut self.abc.scripts {
            s.traits.iter_mut().for_each(shift);
        }
        for b in &mut self.abc.bodies {
            b.traits.iter_mut().for_each(shift);
        }
        self.abc.instances.insert(index, inst);
        self.abc.classes.insert(index, class);
        self.class_ids.insert(index, self.next_class_id);
        self.next_class_id += 1;
    }

    /// `moveClass`.
    pub fn move_class(&mut self, old: usize, new: usize) {
        let mv = |cur: usize| -> usize {
            let mut n = new;
            if n > old {
                n -= 1;
            }
            if cur == old {
                return n;
            }
            let mut c = cur;
            if c > old {
                c -= 1;
            }
            if c >= n {
                c += 1;
            }
            c
        };
        let fix = |t: &mut Trait| {
            if let TraitData::Class { class, .. } = &mut t.data {
                *class = mv(*class);
            }
        };
        for s in &mut self.abc.scripts {
            s.traits.iter_mut().for_each(fix);
        }
        for b in &mut self.abc.bodies {
            b.traits.iter_mut().for_each(fix);
        }
        let n = if new > old { new - 1 } else { new };
        let ii = self.abc.instances.remove(old);
        let ci = self.abc.classes.remove(old);
        let id = self.class_ids.remove(old);
        self.abc.instances.insert(n, ii);
        self.abc.classes.insert(n, ci);
        self.class_ids.insert(n, id);
    }

    pub fn add_method_info(&mut self, mi: MethodInfo) -> usize {
        self.abc.methods.push(mi);
        self.abc.body_of.push(None);
        self.abc.methods.len() - 1
    }

    /// `generateMetadata`.
    pub fn generate_metadata(&mut self, metadata: &Metadata_) -> Vec<usize> {
        let mut out = Vec::new();
        for (name, values) in metadata {
            if name == "API" && self.is_api_versioned() {
                continue;
            }
            // Like mxmlc, [SWF] (stage settings, which the decompiler prints) is not kept.
            if name == "SWF" {
                continue;
            }
            let keys: Vec<String> = values.iter().map(|(k, _)| k.clone()).collect();
            let order = java_hash_order(&keys);
            let mut ks = Vec::new();
            let mut vs = Vec::new();
            for i in order {
                ks.push(self.str(&values[i].0));
                vs.push(self.str(&values[i].1));
            }
            let n = self.str(name);
            out.push(self.abc.metadata.len());
            self.abc.metadata.push(Metadata { name: n, keys: ks, values: vs });
        }
        out
    }

    fn is_api_versioned(&self) -> bool {
        false
    }

    /// `getValueKind`.
    pub fn value_kind(&mut self, ty: Option<&Node>, val: Option<&Node>, generated_ns: bool) -> Option<(usize, u8)> {
        self.value_kind_of(ty, val, generated_ns, false)
    }

    /// `getValueKind`; a parameter's default may also be the global `NaN`, which FFDec rejects.
    pub fn value_kind_of(&mut self, ty: Option<&Node>, val: Option<&Node>, generated_ns: bool, param: bool) -> Option<(usize, u8)> {
        let val = val?;
        let is_ns = ty.is_some_and(|t| matches!(&*t.borrow(), Item::Name(n) if n.var == "namespace") || is_type_named(t, &["Namespace"]));
        let it = val.borrow().clone();
        Some(match it {
            Item::Bool(true) | Item::True => (0x0b, 0x0b),
            Item::Bool(false) | Item::False => (0x0a, 0x0a),
            Item::Str(s) => {
                if is_ns {
                    (self.namespace(if generated_ns { ns_kind::PACKAGE_INTERNAL } else { ns_kind::NAMESPACE }, &s), 0x08)
                } else {
                    (self.str(&s), 0x01)
                }
            }
            Item::Int(v) => (pool::int_id(&mut self.abc, v), 0x03),
            Item::Double(v) => (pool::double_id(&mut self.abc, v), 0x06),
            Item::Decimal(v) => (pool::decimal_id(&mut self.abc, v), 0x02),
            Item::Float(v) => (pool::float_id(&mut self.abc, v), 0x02),
            Item::Nan => (pool::double_id(&mut self.abc, f64::NAN), 0x06),
            Item::Unresolved(u) if param && u.name.len() == 1 && u.name.last() == "NaN" && u.resolved.is_none() => (pool::double_id(&mut self.abc, f64::NAN), 0x06),
            Item::Null => (0x0c, 0x0c),
            Item::Undefined => (0, 0),
            _ => return None,
        })
    }

    /// `methodName`: the method's debug name.
    #[allow(clippy::too_many_arguments)]
    fn method_name(&mut self, outside_pkg: bool, pkg: Option<&Dotted>, name: &str, ns: Option<&Ns>, class_name: Option<&str>, custom_ns: Option<&str>, suffix: Option<&str>) -> usize {
        let mut sb = String::new();
        if let Some(c) = class_name {
            if let Some(p) = pkg {
                if !p.is_empty() {
                    sb.push_str(&p.raw());
                    sb.push(':');
                }
            }
            sb.push_str(c);
        }
        if let Some(c) = custom_ns {
            sb.push_str(c);
        } else if let Some(ns) = ns {
            let k = ns.borrow().kind;
            if k == ns_kind::PACKAGE_INTERNAL as i32 {
                sb.push_str(&pkg.map_or(String::new(), |p| p.raw()));
            } else if k == ns_kind::PRIVATE as i32 {
                if !outside_pkg {
                    sb.push_str("/private");
                }
            } else if k == ns_kind::PROTECTED as i32 || k == ns_kind::STATIC_PROTECTED as i32 {
                sb.push_str("/protected");
            }
        }
        sb.push('/');
        sb.push_str(name);
        if let Some(s) = suffix {
            if !s.is_empty() {
                sb.push('/');
                sb.push_str(s);
            }
        }
        self.str(&sb)
    }

    fn new_ld(&self, outer: &Ld, class_base: Option<String>, is_static: bool, sub_method: bool, ret_type: &Node) -> Ld {
        Ld {
            current_class: class_base,
            pkg: outer.pkg.clone(),
            call_stack: Rc::new(std::cell::RefCell::new(outer.call_stack.borrow().clone())),
            current_script: outer.current_script,
            script_index: outer.script_index,
            document_class: outer.document_class.clone(),
            private_ns: outer.private_ns,
            protected_ns: outer.protected_ns,
            static_protected_ns: outer.static_protected_ns,
            internal_ns: outer.internal_ns,
            is_static,
            sub_method,
            number_context: outer.number_context,
            return_type: Some(ret_type.clone()),
            ..Ld::default()
        }
    }

    /// `methodHeader`: the method's info, before its body.
    #[allow(clippy::too_many_arguments)]
    pub fn method_header(&mut self, is_static: bool, name_index: usize, sub_method: bool, class_base: Option<String>, constructor: bool, outer: &Ld, param_types: &[Node], param_values: &[Node], ret_type: &Node, line: usize) -> R<usize> {
        let ld = self.new_ld(outer, class_base, is_static, sub_method, ret_type);
        let mut pts = Vec::new();
        for t in param_types {
            pts.push(self.type_name(Some(&ld), t)?);
        }
        let mut optional = Vec::new();
        for (i, v) in param_values.iter().enumerate() {
            let t = &param_types[param_types.len() - param_values.len() + i];
            match self.value_kind_of(Some(t), Some(v), false, true) {
                Some((index, kind)) => optional.push(ValueKind { index, kind }),
                None => return err(line, "Default value must be compiletime constant"),
            }
        }
        let rt = if constructor { 0 } else { self.type_name(Some(&ld), ret_type)? };
        Ok(self.add_method_info(MethodInfo { param_types: pts, return_type: rt, name: name_index, flags: 0, optional, param_names: Vec::new() }))
    }

    /// `method`: generates a method's body.
    #[allow(clippy::too_many_arguments)]
    pub fn method(
        &mut self,
        replace: Option<usize>,
        is_static: bool,
        name_index: usize,
        sub_method: bool,
        is_interface: bool,
        is_native: bool,
        call_stack: &CallStack,
        pkg: Option<Dotted>,
        needs_activation: bool,
        subvariables: &mut Nodes,
        init_scope: usize,
        has_rest: bool,
        line: usize,
        class_base: Option<String>,
        super_type: Option<String>,
        constructor: bool,
        outer: &Ld,
        param_types: &mut Nodes,
        param_names: &[String],
        param_values: &[Node],
        mut body: Option<&mut Nodes>,
        ret_type: &Node,
    ) -> R<usize> {
        let mut ld = self.new_ld(outer, class_base.clone(), is_static, sub_method, ret_type);
        ld.activation_reg = 0;
        let full = ld.full_class();
        let mut i = 0;
        while i < subvariables.len() {
            let an = subvariables[i].clone();
            let pending = matches!(&*an.borrow(), Item::Unresolved(u) if u.resolved.is_none());
            if pending {
                let tt = ty_str(full.as_deref().unwrap_or(""));
                let snapshot = subvariables.clone();
                let res = self.resolve_unresolved(&an, Some(&ld), full.as_deref(), Some(tt), param_types, param_names, call_stack, &snapshot)?;
                match res {
                    Some(r) if is_assignable(&r) => subvariables[i] = r,
                    _ => {
                        subvariables.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
        for t in 0..param_types.len() {
            let an = param_types[t].clone();
            let pending = matches!(&*an.borrow(), Item::Unresolved(u) if u.resolved.is_none());
            if pending {
                let tt = ty_str(full.as_deref().unwrap_or(""));
                let snapshot = subvariables.clone();
                if let Some(r) = self.resolve_unresolved(&an, Some(&ld), full.as_deref(), Some(tt), param_types, param_names, call_stack, &snapshot)? {
                    param_types[t] = r;
                }
            }
        }
        let mut has_arguments = false;
        let mut slot_names: Vec<String> = vec!["--first".into()];
        let mut slot_types: Vec<Node> = vec![unbounded()];
        let param_line = 0usize;
        let mut reg_names: Vec<String> = Vec::new();
        let mut reg_lines: Vec<usize> = Vec::new();
        let mut reg_types: Vec<Node> = Vec::new();
        if class_base.is_some() && !sub_method {
            let fc = pkg.clone().unwrap_or_default().add_with_suffix(class_base.as_deref().unwrap()).raw();
            reg_types.push(ty_str(&fc));
            ld.scope_stack.push(node(Item::LocalReg { reg: reg_names.len() as i64, ty: ty_str(&fc) }));
        } else {
            reg_types.push(ty_str("global"));
        }
        reg_names.push("this".into());
        reg_lines.push(0);
        for t in param_types.iter() {
            reg_types.push(t.clone());
            slot_types.push(t.clone());
        }
        for _ in param_names {
            reg_lines.push(param_line);
        }
        reg_names.extend(param_names.iter().cloned());
        slot_names.extend(param_names.iter().cloned());
        if has_rest {
            reg_types.push(ty_parts(&["Array"]));
            slot_types.push(ty_parts(&["Array"]));
        }
        ld.register_vars.clear();
        for an in subvariables.iter() {
            if let Item::Name(n) = &*an.borrow() {
                if n.var == "arguments" && !n.definition {
                    reg_names.push("arguments".into());
                    reg_types.push(ty_parts(&["Array"]));
                    reg_lines.push(0);
                    slot_names.push(n.var.clone());
                    slot_types.push(ty_parts(&["Array"]));
                    has_arguments = true;
                    break;
                }
            }
        }
        let param_reg_count = reg_names.len();
        if needs_activation {
            reg_names.push("+$activation".into());
            reg_lines.push(0);
            ld.activation_reg = reg_names.len() as i64 - 1;
            reg_types.push(ty_parts(&["Object"]));
            ld.scope_stack.push(node(Item::LocalReg { reg: ld.activation_reg, ty: ty_parts(&["Object"]) }));
        }
        for round in 1..=2 {
            for an in subvariables.iter() {
                let (def, var, nty, nline) = match &*an.borrow() {
                    Item::Name(n) => (n.definition, n.var.clone(), n.ty.clone().unwrap_or_else(unbounded), n.line),
                    _ => continue,
                };
                if !def || reg_names.contains(&var) {
                    continue;
                }
                if !needs_activation {
                    let m = mask_reg(&var);
                    let mut add_new = false;
                    if round == 1 && m.is_some() {
                        let ri = m.unwrap();
                        let exists = ri < reg_names.len() && reg_names[ri] != UNUSED && reg_names[ri] != var;
                        if exists {
                            add_new = true;
                        } else {
                            while reg_names.len() <= ri {
                                reg_names.push(UNUSED.into());
                                reg_types.push(unbounded());
                                reg_lines.push(param_line);
                            }
                            reg_names[ri] = var.clone();
                            reg_types[ri] = nty.clone();
                            reg_lines[ri] = nline;
                        }
                    } else if round == 2 && m.is_none() {
                        add_new = true;
                    }
                    if add_new {
                        let ni = match reg_names.iter().position(|r| r == UNUSED) {
                            Some(p) => p,
                            None => {
                                reg_names.push(UNUSED.into());
                                reg_types.push(unbounded());
                                reg_lines.push(param_line);
                                reg_names.len() - 1
                            }
                        };
                        reg_names[ni] = var.clone();
                        reg_types[ni] = nty;
                        reg_lines[ni] = nline;
                    }
                } else if round == 1 && !slot_names.contains(&var) {
                    slot_names.push(var.clone());
                    slot_types.push(nty);
                }
            }
        }
        for j in (0..reg_names.len()).rev() {
            if reg_names[j] == UNUSED {
                reg_names.remove(j);
                reg_types.remove(j);
                reg_lines.remove(j);
            }
        }
        let slot_scope: i64 = if sub_method { 0 } else { 1 };
        for an in subvariables.iter() {
            if let Item::Name(n) = &mut *an.borrow_mut() {
                let this_or_super = n.var == "this" || n.var == "super";
                if !this_or_super && needs_activation {
                    if n.slot <= 0 {
                        n.slot = slot_names.iter().position(|s| *s == n.var).map_or(-1, |p| p as i64);
                        n.slot_scope = slot_scope;
                    }
                } else if this_or_super {
                    n.reg = 0;
                } else {
                    n.reg = reg_names.iter().position(|s| *s == n.var).map_or(-1, |p| p as i64);
                }
            }
        }
        for (i, r) in reg_names.iter().enumerate() {
            if needs_activation && i as i64 > ld.activation_reg {
                break;
            }
            ld.register_vars.push((r.clone(), i as i64));
        }
        let mut declarations: Nodes = Vec::new();
        'loopn: for an in subvariables.iter() {
            let n = match &*an.borrow() {
                Item::Name(n) => n.clone(),
                _ => continue,
            };
            if needs_activation {
                if n.slot_scope != slot_scope {
                    continue;
                } else if n.slot < param_reg_count as i64 {
                    continue;
                }
            }
            for d in &declarations {
                if let Item::Name(dn) = &*d.borrow() {
                    if dn.var == n.var {
                        continue 'loopn;
                    }
                }
            }
            if let Some(b) = body.as_deref() {
                for it in b {
                    if let Item::Name(n2) = &*it.borrow() {
                        if n2.definition && n2.assigned.is_some() && n2.var == n.var {
                            continue 'loopn;
                        }
                        if !n2.definition && n2.var == n.var {
                            break;
                        }
                    }
                }
            }
            if n.unresolved || n.redirect.is_some() || n.ns.is_some() {
                continue;
            }
            if n.var == "this" || n.var == "super" || param_names.contains(&n.var) || n.var == "arguments" {
                continue;
            }
            let d = node(new_name(n.ty.clone(), n.line, n.attribute, &n.var, &n.suffix, None, true, n.opened.clone(), n.is_const));
            if let Item::Name(dn) = &mut *d.borrow_mut() {
                if needs_activation {
                    if dn.slot <= 0 {
                        dn.slot = n.slot;
                        dn.slot_scope = n.slot_scope;
                    }
                } else {
                    dn.reg = n.reg;
                }
            }
            declarations.push(d);
        }
        let mut pts = Vec::new();
        for t in param_types.iter() {
            pts.push(self.type_name(Some(&ld), t)?);
        }
        let mut optional = Vec::new();
        for (i, v) in param_values.iter().enumerate() {
            let t = param_types[param_types.len() - param_values.len() + i].clone();
            match self.value_kind_of(Some(&t), Some(v), false, true) {
                Some((index, kind)) => optional.push(ValueKind { index, kind }),
                None => return err(line, "Default value must be compiletime constant"),
            }
        }
        let rt = if constructor { 0 } else { self.type_name(Some(&ld), ret_type)? };
        let mut mi = MethodInfo { param_types: pts, return_type: rt, name: name_index, flags: 0, optional, param_names: Vec::new() };
        if let Some(r) = replace {
            mi = self.abc.methods[r].clone();
        }
        if has_arguments {
            mi.flags |= method_flags::NEED_ARGUMENTS;
        }
        if !param_values.is_empty() {
            mi.flags |= method_flags::HAS_OPTIONAL;
        }
        if has_rest {
            mi.flags |= method_flags::NEED_REST;
        }
        if is_native {
            mi.flags |= method_flags::EXPLICIT;
        }
        if let Some(r) = replace {
            self.abc.methods[r] = mi.clone();
        }
        if is_interface || is_native {
            return Ok(match replace {
                Some(r) => r,
                None => self.add_method_info(mi),
            });
        }
        let mbody: Body = Rc::new(std::cell::RefCell::new(Vec::new()));
        if needs_activation {
            let mut slot_id = 1;
            for i in 1..slot_names.len() {
                let s = self.str(&slot_names[i]);
                let nsi = match &pkg {
                    Some(p) => pool::ns(&mut self.abc, ns_kind::PACKAGE_INTERNAL, &p.raw()),
                    None => pool::namespace_id_of(&mut self.abc, ns_kind::PACKAGE_INTERNAL, 0, 0, true) as usize,
                };
                let name = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns: nsi, name: s });
                let tn = self.type_name(Some(&ld), &slot_types[i])?;
                mbody.borrow_mut().push(Trait { name, attrs: 0, data: TraitData::Slot { slot_id, type_name: tn, value: None }, metadata: Vec::new() });
                slot_id += 1;
            }
            for i in 1..param_reg_count {
                let p = name_node(Some(reg_types[i].clone()), 0, &reg_names[i], None, false, ns_list(Vec::new()), false);
                if let Item::Name(pn) = &mut *p.borrow_mut() {
                    pn.reg = i as i64;
                }
                let d = name_node(Some(reg_types[i].clone()), 0, &reg_names[i], Some(p), true, ns_list(Vec::new()), false);
                if let Item::Name(dn) = &mut *d.borrow_mut() {
                    dn.slot_scope = slot_scope;
                    dn.slot = slot_names.iter().position(|s| *s == reg_names[i]).map_or(-1, |x| x as i64);
                }
                declarations.push(d);
            }
        }
        let mut add_ret = false;
        let mut empty: Nodes = Vec::new();
        let has_body = body.is_some();
        let b: &mut Nodes = match body.as_deref_mut() {
            Some(b) => b,
            None => &mut empty,
        };
        if has_body {
            let mut nb = declarations.clone();
            nb.append(b);
            *b = nb;
            let last_ret = b.last().is_some_and(|l| matches!(&*l.borrow(), Item::ReturnValue(_) | Item::ReturnVoid));
            if b.is_empty() || !last_ret {
                add_ret = true;
            }
        }
        ld.exceptions = Vec::new();
        ld.call_stack.borrow_mut().push(mbody.clone());
        let src = if has_body { self.gen_list(&mut ld, b)? } else { Vec::new() };
        let method_index = match replace {
            Some(r) => r,
            None => self.add_method_info(mi),
        };
        let mut code: Code = src;
        if needs_activation {
            let mut acts = vec![ins(op::NEW_ACTIVATION, &[]), ins(op::DUP, &[])];
            acts.extend(set_loc(ld.activation_reg));
            acts.push(ins(op::PUSH_SCOPE, &[]));
            acts.extend(code);
            code = acts;
        }
        if constructor {
            let mut min_ac = 0i64;
            if let Some(ci) = super_type.as_deref().and_then(|s| self.find_class(&crate::abc::decomp::index::TySpec::Type(Dotted::parse_with_suffix(s), None), ld.script_index)) {
                let c = self.index.class(ci).clone();
                let a = self.abc_of(c.abc);
                let pmi = &a.methods[a.instances[c.index].iinit];
                min_ac = pmi.param_types.len() as i64;
                if pmi.flags & method_flags::HAS_OPTIONAL != 0 {
                    min_ac -= pmi.optional.len() as i64;
                }
            }
            let mut ac = -1;
            for i in &code {
                let b = i.borrow();
                if b.code == op::CONSTRUCT_SUPER && b.pseudo == Pseudo::None {
                    ac = b.ops[0];
                    if min_ac > ac {
                        return err(line, "Parent constructor call requires different number of arguments");
                    }
                }
            }
            if ac == -1 {
                if min_ac == 0 {
                    code.insert(0, ins(op::GET_LOCAL0, &[]));
                    code.insert(1, ins(op::CONSTRUCT_SUPER, &[0]));
                } else {
                    return err(line, "Parent constructor must be called");
                }
            }
        }
        for i in 1..reg_names.len() {
            let s = self.str(&reg_names[i]);
            code.insert(i - 1, ins(op::DEBUG, &[1, s as i64, i as i64 - 1, reg_lines[i] as i64]));
        }
        if !sub_method {
            code.insert(0, ins(op::GET_LOCAL0, &[]));
            code.insert(1, ins(op::PUSH_SCOPE, &[]));
        }
        if add_ret {
            let rts = text(ret_type);
            if rts == "*" || rts == "void" || constructor {
                code.push(ins(op::RETURN_VOID, &[]));
            } else {
                code.push(ins(op::PUSH_UNDEFINED, &[]));
                code.push(ins(op::RETURN_VALUE, &[]));
            }
        }
        let mut exceptions = ld.exceptions.clone();
        resolve_marks(&mut code, &mut exceptions);
        let traits = mbody.borrow().clone();
        let bi = self.abc.bodies.len();
        self.abc.bodies.push(MethodBody { method: method_index, max_stack: 0, max_regs: 0, init_scope_depth: 0, max_scope_depth: 0, code: Vec::new(), exceptions: Vec::new(), traits });
        self.abc.body_of[method_index] = Some(bi);
        self.pending.insert(bi, (code, exceptions));
        self.auto_fill(bi, init_scope);
        Ok(method_index)
    }

    /// `generate(localData, FunctionAVM2Item)`.
    pub fn function_src(&mut self, ld: &mut Ld, n: &Node) -> R<Code> {
        let f = match &*n.borrow() {
            Item::Function(f) => f.clone(),
            _ => unreachable!(),
        };
        let mut ret = Vec::new();
        let mut scope = 0;
        if !f.name.is_empty() {
            ret.push(ins(op::NEW_OBJECT, &[0]));
            ret.push(ins(op::PUSH_WITH, &[]));
            scope = ld.scope_stack.len() as i64;
            ld.scope_stack.push(property(None, false, &f.name, "", ns_list(Vec::new()), ld.call_stack.clone(), false, None, 0, Some(ty_str(""))));
        }
        let name = self.str(&f.name);
        let mut sub = f.subvariables.clone();
        let mut pts = f.param_types.clone();
        let mut body = f.body.clone();
        let cs = ld.call_stack.clone();
        let mi = self.method(None, false, name, true, false, false, &cs, ld.pkg.clone(), f.needs_activation, &mut sub, 0, f.has_rest, f.line, ld.current_class.clone(), None, false, ld, &mut pts, &f.param_names, &f.param_values, body.as_mut(), &f.ret_type)?;
        if let Item::Function(ff) = &mut *n.borrow_mut() {
            ff.subvariables = sub;
            ff.param_types = pts;
            ff.body = body;
        }
        ret.push(ins(op::NEW_FUNCTION, &[mi as i64]));
        if !f.name.is_empty() {
            ret.push(ins(op::DUP, &[]));
            ret.push(ins(op::GET_SCOPE_OBJECT, &[scope]));
            ret.push(ins(op::SWAP, &[]));
            let s = self.str(&f.name);
            let pk = ld.pkg.clone().map_or(String::new(), |p| p.raw());
            let nsi = if ld.pkg.is_none() { pool::namespace_id_of(&mut self.abc, ns_kind::PACKAGE, 0, 0, true) as usize } else { pool::ns(&mut self.abc, ns_kind::PACKAGE, &pk) };
            let m = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns: nsi, name: s });
            ret.push(ins(op::SET_PROPERTY, &[m as i64]));
            ret.push(ins(op::POP_SCOPE, &[]));
            ld.scope_stack.pop();
        }
        Ok(ret)
    }

    /// `SlotAVM2Item`/`ConstAVM2Item.toSource`: the initializer in a class or script init.
    pub fn slot_src(&mut self, ld: &mut Ld, n: &Node) -> R<Code> {
        let s = match &*n.borrow() {
            Item::Slot(s) => s.clone(),
            _ => unreachable!(),
        };
        if ld.is_static != s.is_static {
            return Ok(Vec::new());
        }
        if s.ns {
            return Ok(Vec::new());
        }
        let pkg = s.pkg.borrow().name.clone().unwrap_or_default();
        let (imported, opened) = (ld.imported.clone(), ld.opened.clone());
        let ns = self.gen_ns(&imported, &pkg, &s.pkg, &opened, Some(ld))?;
        let mut ret = Vec::new();
        if let Some(v) = &s.value {
            handle_and_or_coerce(v, &s.ty);
            let tn = self.trait_name(ns, &s.var);
            ret.push(ins(op::FIND_PROPERTY, &[tn as i64]));
            ld.is_static = true;
            ret.extend(self.src(ld, v)?);
            let tn = self.trait_name(ns, &s.var);
            ret.push(ins(if s.is_const { op::INIT_PROPERTY } else { op::SET_PROPERTY }, &[tn as i64]));
        }
        Ok(ret)
    }

    /// `generateTraitsPhase1`: creates the traits, with class slots for classes.
    #[allow(clippy::too_many_arguments)]
    pub fn traits_phase1(&mut self, imported: &Imports, opened: &NsList, class_name: Option<&str>, generate_static: bool, ld: &Ld, items: &[Node], owner: Owner, class_index: &mut usize, script_traits: bool) -> R<Vec<Option<TraitRef>>> {
        let mut traits = vec![None; items.len()];
        let mut slot_id = 1;
        let mut disp_id = 3;
        for (k, item) in items.iter().enumerate() {
            let it = item.borrow().clone();
            let push = |cx: &mut Cx, t: Trait| -> TraitRef {
                let list = cx.traits_of(owner);
                list.push(t);
                TraitRef { owner, index: list.len() - 1 }
            };
            match it {
                Item::Interface(i) => {
                    let ci = *class_index;
                    self.add_class(ci, Instance { name: 0, super_name: 0, flags: instance_flags::INTERFACE, protected_ns: 0, interfaces: Vec::new(), iinit: 0, traits: Vec::new() }, Class { cinit: 0, traits: Vec::new() });
                    *class_index += 1;
                    let r = push(self, Trait { name: 0, attrs: 0, data: TraitData::Class { slot_id: 0, class: ci }, metadata: Vec::new() });
                    traits[k] = Some(r);
                    let md = self.generate_metadata(&i.metadata);
                    let t = self.trait_mut(r);
                    if !md.is_empty() {
                        t.attrs |= trait_attr::METADATA;
                    }
                    t.metadata = md;
                }
                Item::Class(c) => {
                    let ci = *class_index;
                    self.add_class(ci, Instance { name: 0, super_name: 0, flags: 0, protected_ns: 0, interfaces: Vec::new(), iinit: 0, traits: Vec::new() }, Class { cinit: 0, traits: Vec::new() });
                    *class_index += 1;
                    let r = push(self, Trait { name: 0, attrs: 0, data: TraitData::Class { slot_id, class: ci }, metadata: Vec::new() });
                    slot_id += 1;
                    traits[k] = Some(r);
                    let md = self.generate_metadata(&c.metadata);
                    let t = self.trait_mut(r);
                    if !md.is_empty() {
                        t.attrs |= trait_attr::METADATA;
                    }
                    t.metadata = md;
                }
                Item::Slot(s) => {
                    if !script_traits && s.is_static != generate_static {
                        continue;
                    }
                    let pkg = ld.pkg.clone().unwrap_or_default();
                    self.resolve_custom_ns(&s.pkg, imported, &pkg, opened, Some(ld))?;
                    let namespace = self.ns_index(&s.pkg)?;
                    let is_namespace = s.is_const && text(&s.ty) == "Namespace";
                    let md = self.generate_metadata(&s.metadata);
                    let name = if is_namespace { self.trait_name(namespace, &s.var) } else { 0 };
                    let sid = if !script_traits && s.is_static {
                        slot_id += 1;
                        slot_id - 1
                    } else {
                        0
                    };
                    let data = if s.is_const { TraitData::Const { slot_id: sid, type_name: 0, value: None } } else { TraitData::Slot { slot_id: sid, type_name: 0, value: None } };
                    let attrs = if md.is_empty() { 0 } else { trait_attr::METADATA };
                    traits[k] = Some(push(self, Trait { name, attrs, data, metadata: md }));
                }
                Item::Method(m) => {
                    if !script_traits && m.is_static != generate_static {
                        continue;
                    }
                    let did = if m.is_static {
                        disp_id += 1;
                        disp_id - 1
                    } else {
                        0
                    };
                    let data = match m.kind {
                        MethodKind::Getter => TraitData::Getter { disp_id: did, method: 0 },
                        MethodKind::Setter => TraitData::Setter { disp_id: did, method: 0 },
                        MethodKind::Method => TraitData::Method { disp_id: did, method: 0 },
                    };
                    let mut attrs = 0;
                    if m.is_final || (class_name.is_some() && m.is_static) {
                        attrs |= trait_attr::FINAL;
                    }
                    if m.is_override {
                        attrs |= trait_attr::OVERRIDE;
                    }
                    let r = push(self, Trait { name: 0, attrs, data, metadata: Vec::new() });
                    traits[k] = Some(r);
                    let md = self.generate_metadata(&m.f.metadata);
                    let t = self.trait_mut(r);
                    if !md.is_empty() {
                        t.attrs |= trait_attr::METADATA;
                    }
                    t.metadata = md;
                }
                _ => {}
            }
        }
        Ok(traits)
    }

    /// `generateTraitsPhase2`: trait and class names.
    pub fn traits_phase2(&mut self, imported: &Imports, pkg: Option<&Dotted>, items: &[Node], traits: &[Option<TraitRef>], opened: &NsList, ld: &mut Ld) -> R<()> {
        let pkgd = pkg.cloned().unwrap_or_default();
        for (k, item) in items.iter().enumerate() {
            let Some(r) = traits[k] else { continue };
            let it = item.borrow().clone();
            let name = match it {
                Item::Interface(i) => {
                    let ns = self.ns_index(&i.pkg)?;
                    self.trait_name(ns, &i.base_name)
                }
                Item::Class(c) => {
                    let ns = self.ns_index(&c.pkg)?;
                    self.trait_name(ns, &c.base_name)
                }
                Item::Method(m) => {
                    let p = m.f.pkg.clone().unwrap_or_else(|| ns_item_str("", ns_kind::PACKAGE as i32, 0));
                    let ns = self.gen_ns(imported, &pkgd, &p, opened, Some(ld))?;
                    self.trait_name(ns, &m.f.name)
                }
                Item::Slot(s) => {
                    let ns = self.gen_ns(imported, &pkgd, &s.pkg, opened, Some(ld))?;
                    let n = self.trait_name(ns, &s.var);
                    if s.ns {
                        let lp = ld.pkg.clone().unwrap_or_default();
                        self.resolve_custom_ns(&s.pkg, imported, &lp, opened, Some(ld))?;
                        let namespace = self.ns_index(&s.pkg)?;
                        let _ = namespace;
                        let vk = self.value_kind(Some(&s.ty), s.value.as_ref(), s.generated_ns).unwrap_or((0, 0));
                        let t = self.trait_mut(r);
                        if let TraitData::Const { type_name, value, .. } | TraitData::Slot { type_name, value, .. } = &mut t.data {
                            *type_name = 0;
                            *value = value_kind_opt(vk);
                        }
                    }
                    n
                }
                _ => continue,
            };
            self.trait_mut(r).name = name;
        }
        for (k, item) in items.iter().enumerate() {
            let Some(r) = traits[k] else { continue };
            let it = item.borrow().clone();
            let (base, pkgns) = match it {
                Item::Class(c) => (c.base_name.clone(), c.pkg.clone()),
                Item::Interface(i) => (i.base_name.clone(), i.pkg.clone()),
                _ => continue,
            };
            ld.current_class = Some(base.clone());
            let ci = match self.trait_mut(r).data {
                TraitData::Class { class, .. } => class,
                _ => continue,
            };
            let s = self.str(&base);
            let ns = self.ns_index(&pkgns)?;
            self.abc.instances[ci].name = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns, name: s });
            ld.current_class = None;
        }
        Ok(())
    }

    /// `superIntName`.
    fn super_int_name(&mut self, ld: &Ld, un: &Node) -> R<usize> {
        let mut un = un.clone();
        if matches!(&*un.borrow(), Item::Unresolved(_)) {
            let fc = ld.full_class();
            self.resolve_unresolved(&un, Some(ld), fc.as_deref(), None, &[], &[], &empty_stack(), &[])?;
            un = super::gen::resolved_of(&un);
        }
        if !matches!(&*un.borrow(), Item::Type { .. }) {
            return err(0, "Invalid type");
        }
        let pid = self.resolve_type(Some(ld), &un)?;
        let (name, ns) = match &self.abc.constants.multinames[pid] {
            Multiname::QName { ns, name, .. } => (*name, *ns),
            _ => (0, 0),
        };
        let set = pool::ns_set_id(&mut self.abc, &[ns], true) as usize;
        Ok(pool::mn(&mut self.abc, Multiname::Multiname { attr: false, name, ns_set: set }))
    }

    fn class_index_by_mn(&self, mn: usize) -> Option<usize> {
        let name = raw_names(&self.abc).name_with_ns(mn, true).raw();
        self.find_class_by_name(&name)
    }

    /// `generateTraitsPhase3`: superclasses and interfaces, with classes ordered after them.
    pub fn traits_phase3(&mut self, items: &[Node], traits: &[Option<TraitRef>], ld: &mut Ld) -> R<()> {
        for (k, item) in items.iter().enumerate() {
            let Some(r) = traits[k] else { continue };
            let it = item.borrow().clone();
            match it {
                Item::Class(c) => {
                    let mut min = -1i64;
                    ld.current_class = Some(c.base_name.clone());
                    let ci = match self.trait_mut(r).data {
                        TraitData::Class { class, .. } => class,
                        _ => continue,
                    };
                    let sup = match &c.extends {
                        Some(e) => self.type_name(Some(ld), e)?,
                        None => {
                            let s = self.str("Object");
                            let ns = self.namespace(ns_kind::PACKAGE, "");
                            pool::mn(&mut self.abc, Multiname::QName { attr: false, ns, name: s })
                        }
                    };
                    self.abc.instances[ci].super_name = sup;
                    if sup != 0 {
                        if let Some(f) = self.class_index_by_mn(sup) {
                            min = min.max(f as i64);
                        }
                    }
                    let mut ifs = Vec::new();
                    for i in &c.implements {
                        let m = self.super_int_name(ld, i)?;
                        ifs.push(m);
                        if let Some(f) = self.class_index_by_mn(m) {
                            min = min.max(f as i64);
                        }
                    }
                    let ci = match self.trait_mut(r).data {
                        TraitData::Class { class, .. } => class,
                        _ => ci,
                    };
                    self.abc.instances[ci].interfaces = ifs;
                    if ci as i64 <= min {
                        self.move_class(ci, (min + 1) as usize);
                    }
                    ld.current_class = None;
                }
                Item::Interface(i) => {
                    let mut min = -1i64;
                    ld.current_class = Some(i.base_name.clone());
                    let mut ifs = Vec::new();
                    for s in &i.super_interfaces {
                        let m = self.super_int_name(ld, s)?;
                        ifs.push(m);
                        if let Some(f) = self.class_index_by_mn(m) {
                            min = min.max(f as i64);
                        }
                    }
                    let ci = match self.trait_mut(r).data {
                        TraitData::Class { class, .. } => class,
                        _ => continue,
                    };
                    self.abc.instances[ci].interfaces = ifs;
                    if ci as i64 <= min {
                        self.move_class(ci, (min + 1) as usize);
                    }
                    ld.current_class = None;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// `generateTraitsPhase4`: types, values, method bodies and classes.
    #[allow(clippy::too_many_arguments)]
    pub fn traits_phase4(&mut self, imported: &Imports, opened: &NsList, method_init_scope: usize, is_interface: bool, class_name: Option<&str>, super_name: Option<&str>, generate_static: bool, ld: &mut Ld, items: &[Node], traits: &[Option<TraitRef>], init_scopes: &HashMap<usize, usize>, class_index: &mut usize, script_traits: bool) -> R<()> {
        for (k, item) in items.iter().enumerate() {
            let Some(r) = traits[k] else { continue };
            let it = item.borrow().clone();
            match it {
                Item::Slot(s) => {
                    if !script_traits && s.is_static != generate_static {
                        continue;
                    }
                    if s.is_const && s.ns {
                        continue;
                    }
                    let lp = ld.pkg.clone().unwrap_or_default();
                    self.resolve_custom_ns(&s.pkg, imported, &lp, opened, Some(ld))?;
                    self.ns_index(&s.pkg)?;
                    let tn = self.type_name(Some(ld), &s.ty)?;
                    let vk = self.value_kind(Some(&s.ty), s.value.as_ref(), s.generated_ns).unwrap_or((0, 0));
                    let t = self.trait_mut(r);
                    if let TraitData::Const { type_name, value, .. } | TraitData::Slot { type_name, value, .. } = &mut t.data {
                        *type_name = tn;
                        *value = value_kind_opt(vk);
                    }
                }
                Item::Interface(_) | Item::Class(_) => {
                    let ci = match self.trait_mut(r).data {
                        TraitData::Class { class, .. } => class,
                        _ => continue,
                    };
                    let scope = init_scopes.get(&k).copied().unwrap_or(1);
                    self.generate_class(ci, scope, ld, item, class_index)?;
                }
                Item::Method(m) => {
                    if !script_traits && m.is_static != generate_static {
                        continue;
                    }
                    for ln in m.all_opened.borrow().iter() {
                        let lp = ld.pkg.clone().unwrap_or_default();
                        let list: Vec<Ns> = ln.borrow().clone();
                        for n in &list {
                            self.resolve_custom_ns(n, imported, &lp, ln, Some(ld))?;
                        }
                    }
                    let suffix = match m.kind {
                        MethodKind::Getter => Some("get"),
                        MethodKind::Setter => Some("set"),
                        MethodKind::Method => None,
                    };
                    let pkg = ld.pkg.clone();
                    let mn = self.method_name(m.outside_package, pkg.as_ref(), &m.f.name, m.f.pkg.as_ref(), class_name, m.custom_ns.as_deref(), suffix);
                    let scope = method_init_scope + if m.is_static { 0 } else { 1 };
                    let mi = self.method_header(m.is_static, mn, false, class_name.map(String::from), false, ld, &m.f.param_types, &m.f.param_values, &m.f.ret_type, m.f.line)?;
                    let _ = scope;
                    if let TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } = &mut self.trait_mut(r).data {
                        *method = mi;
                    }
                }
                _ => {}
            }
        }
        self.refresh();
        for (k, item) in items.iter().enumerate() {
            let Some(r) = traits[k] else { continue };
            let is_method = matches!(&*item.borrow(), Item::Method(_));
            if !is_method {
                continue;
            }
            let m = match &*item.borrow() {
                Item::Method(m) => m.clone(),
                _ => unreachable!(),
            };
            if !script_traits && m.is_static != generate_static {
                continue;
            }
            let suffix = match m.kind {
                MethodKind::Getter => Some("get"),
                MethodKind::Setter => Some("set"),
                MethodKind::Method => None,
            };
            let pkg = ld.pkg.clone();
            let mn = self.method_name(m.outside_package, pkg.as_ref(), &m.f.name, m.f.pkg.as_ref(), class_name, m.custom_ns.as_deref(), suffix);
            let replace = match self.trait_mut(r).data {
                TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } => method,
                _ => continue,
            };
            let mut sub = m.f.subvariables.clone();
            let mut pts = m.f.param_types.clone();
            let mut body = m.f.body.clone();
            let scope = method_init_scope + if m.is_static { 0 } else { 1 };
            let mi = self.method(Some(replace), m.is_static, mn, false, is_interface, m.f.is_native, &empty_stack(), ld.pkg.clone(), m.f.needs_activation, &mut sub, scope, m.f.has_rest, m.f.line, class_name.map(String::from), super_name.map(String::from), false, ld, &mut pts, &m.f.param_names, &m.f.param_values, body.as_mut(), &m.f.ret_type)?;
            if let Item::Method(mm) = &mut *item.borrow_mut() {
                mm.f.subvariables = sub;
                mm.f.param_types = pts;
                mm.f.body = body;
            }
            if let TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } = &mut self.trait_mut(r).data {
                *method = mi;
            }
        }
        Ok(())
    }

    /// `refreshSelected`.
    pub fn refresh(&mut self) {
        let view = self.deleted_view();
        let abc = std::mem::replace(&mut self.abc, crate::abc::Abc::empty());
        self.index.refresh(&abc, &view);
        self.abc = abc;
    }

    /// Deletions keyed by current class positions, for the index.
    pub fn deleted_view(&self) -> super::index::Deleted {
        let pos = |id: usize| self.class_ids.iter().position(|&x| x == id);
        let mut v = super::index::Deleted { scripts: self.deleted.scripts.clone(), methods: self.deleted.methods.clone(), ..Default::default() };
        for &id in &self.deleted.classes {
            if let Some(p) = pos(id) {
                v.classes.insert(p);
            }
        }
        for &(o, t) in &self.deleted.traits {
            let o2 = match o {
                Owner::Script(s) => Some(Owner::Script(s)),
                Owner::Instance(id) => pos(id).map(Owner::Instance),
                Owner::Class(id) => pos(id).map(Owner::Class),
            };
            if let Some(o2) = o2 {
                v.traits.insert((o2, t));
            }
        }
        v
    }

    /// `generateClass`.
    pub fn generate_class(&mut self, ci: usize, init_scope: usize, ld: &mut Ld, item: &Node, class_index: &mut usize) -> R<()> {
        let it = item.borrow().clone();
        match it {
            Item::Class(c) => {
                let pkgd = c.pkg.borrow().name.clone().unwrap_or_default();
                let sup = c.extends.as_ref().map_or("Object".to_string(), text);
                let mut traits = c.traits.clone();
                self.generate_class_body(ci, &c.imported, c.cinit_variables.clone(), c.cinit_activation, &c.opened, init_scope, &pkgd, ld, false, &c.base_name, Some(&sup), c.extends.clone(), &c.implements, c.iinit.clone(), c.iinit_variables.clone(), &mut traits, &c.metadata)?;
                if let Item::Class(cc) = &mut *item.borrow_mut() {
                    cc.traits = traits;
                }
                let ii = &mut self.abc.instances[ci];
                if !c.is_dynamic {
                    ii.flags |= instance_flags::SEALED;
                }
                if c.is_final {
                    ii.flags |= instance_flags::FINAL;
                }
                if !c.nullable {
                    ii.flags |= instance_flags::NON_NULLABLE;
                }
                ii.flags |= instance_flags::PROTECTED_NS;
                let q = if pkgd.raw().is_empty() { c.base_name.clone() } else { format!("{}:{}", pkgd.raw(), c.base_name) };
                let p = self.namespace(ns_kind::PROTECTED, &q);
                self.abc.instances[ci].protected_ns = p;
            }
            Item::Interface(i) => {
                {
                    let ii = &mut self.abc.instances[ci];
                    ii.flags |= instance_flags::INTERFACE | instance_flags::SEALED;
                    if !i.nullable {
                        ii.flags |= instance_flags::NON_NULLABLE;
                    }
                }
                let pkgd = i.pkg.borrow().name.clone().unwrap_or_default();
                let mut methods = i.methods.clone();
                self.generate_class_body(ci, &i.imported, Vec::new(), false, &i.opened, init_scope, &pkgd, ld, true, &i.base_name, None, None, &i.super_interfaces, None, Vec::new(), &mut methods, &i.metadata)?;
            }
            _ => {}
        }
        let _ = class_index;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_class_body(
        &mut self,
        ci: usize,
        imported: &Imports,
        mut cinit_variables: Nodes,
        cinit_activation: bool,
        opened: &NsList,
        init_scope: usize,
        pkg: &Dotted,
        ld: &mut Ld,
        is_interface: bool,
        base: &str,
        super_name: Option<&str>,
        extends: Option<Node>,
        implements: &[Node],
        iinit: Option<Node>,
        iinit_variables: Nodes,
        commands: &mut Nodes,
        _metadata: &Meta,
    ) -> R<()> {
        let trait_items: Nodes = commands.iter().filter(|it| matches!(&*it.borrow(), Item::Slot(_) | Item::Class(_) | Item::Interface(_) | Item::Method(_))).cloned().collect();
        for t in &trait_items {
            let md = match &*t.borrow() {
                Item::Method(m) => m.f.metadata.clone(),
                Item::Slot(s) => s.metadata.clone(),
                _ => Vec::new(),
            };
            if md.iter().any(|(k, _)| k == "SkinPart") {
                return err(0, "SkinPart metadata is not supported yet");
            }
        }
        ld.current_class = Some(base.to_string());
        ld.pkg = Some(pkg.clone());
        let q = if pkg.raw().is_empty() { base.to_string() } else { format!("{}:{}", pkg.raw(), base) };
        ld.private_ns = self.namespace(ns_kind::PRIVATE, &q);
        ld.protected_ns = self.namespace(ns_kind::PROTECTED, &q);
        ld.static_protected_ns = self.namespace(ns_kind::STATIC_PROTECTED, &q);
        ld.internal_ns = self.namespace(ns_kind::PACKAGE_INTERNAL, &pkg.raw());
        let extends = if extends.is_none() && !is_interface { Some(ty_parts(&["Object"])) } else { extends };
        let mut cidx = 0;
        let it = self.traits_phase1(imported, opened, Some(base), false, ld, &trait_items, Owner::Instance(ci), &mut cidx, false)?;
        let st = self.traits_phase1(imported, opened, Some(base), true, ld, &trait_items, Owner::Class(ci), &mut cidx, false)?;
        self.traits_phase2(imported, Some(pkg), &trait_items, &it, opened, ld)?;
        self.traits_phase2(imported, Some(pkg), &trait_items, &st, opened, ld)?;
        self.refresh();
        let scopes = HashMap::new();
        self.traits_phase4(imported, opened, init_scope, is_interface, Some(base), super_name, false, ld, &trait_items, &it, &scopes, &mut cidx, false)?;
        self.traits_phase4(imported, opened, init_scope, is_interface, Some(base), super_name, true, ld, &trait_items, &st, &scopes, &mut cidx, false)?;
        let ext_text = extends.as_ref().map(text);
        let init;
        let iinit_node = if is_interface { None } else { iinit.clone() };
        match &iinit_node {
            None => {
                let ename = if is_interface { None } else { Some(base.to_string()) };
                init = self.method(None, false, 0, false, is_interface, false, &empty_stack(), Some(pkg.clone()), false, &mut Vec::new(), init_scope + 1, false, 0, ename, ext_text.clone(), true, ld, &mut Vec::new(), &[], &[], Some(&mut Vec::new()), &unbounded())?;
            }
            Some(m) => {
                let mm = match &*m.borrow() {
                    Item::Method(x) => x.clone(),
                    _ => return err(0, "Invalid constructor"),
                };
                let mut all = iinit_variables.clone();
                all.extend(mm.f.subvariables.iter().cloned());
                let name = if pkg.raw().is_empty() { self.str(&format!("{base}/{base}")) } else { self.str(&format!("{}:{base}/{base}", pkg.raw())) };
                let mut pts = mm.f.param_types.clone();
                let mut body = mm.f.body.clone();
                init = self.method(None, false, name, false, false, false, &empty_stack(), Some(pkg.clone()), mm.f.needs_activation, &mut all, init_scope + 1, mm.f.has_rest, mm.f.line, Some(base.to_string()), ext_text.clone(), true, ld, &mut pts, &mm.f.param_names, &mm.f.param_values, body.as_mut(), &unbounded())?;
            }
        }
        self.abc.instances[ci].iinit = init;
        let cname = self.str("");
        let cscope = init_scope + usize::from(!implements.is_empty());
        let cinit = self.method(None, true, cname, false, false, false, &empty_stack(), Some(pkg.clone()), cinit_activation, &mut cinit_variables, cscope, false, 0, if is_interface { None } else { Some(base.to_string()) }, super_name.map(String::from), false, ld, &mut Vec::new(), &[], &[], Some(commands), &unbounded())?;
        let mut i = 0;
        while i < cinit_variables.len() {
            let an = cinit_variables[i].clone();
            let pending = matches!(&*an.borrow(), Item::Unresolved(u) if u.resolved.is_none());
            if pending {
                let fc = ld.full_class();
                let tt = ty_str(fc.as_deref().unwrap_or(""));
                let snap = cinit_variables.clone();
                let cb = self.abc.body_of[cinit].map(|b| Rc::new(std::cell::RefCell::new(self.abc.bodies[b].traits.clone())));
                let cs: CallStack = Rc::new(std::cell::RefCell::new(cb.into_iter().collect()));
                match self.resolve_unresolved(&an, Some(ld), fc.as_deref(), Some(tt), &[], &[], &cs, &snap)? {
                    Some(r) if is_assignable(&r) => cinit_variables[i] = r,
                    _ => {
                        cinit_variables.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
        let mut initcode: Code = Vec::new();
        for ti in commands.iter() {
            let s = match &*ti.borrow() {
                Item::Slot(s) => s.clone(),
                _ => continue,
            };
            let ns = self.gen_ns(imported, pkg, &s.pkg, opened, Some(ld))?;
            if s.is_const && text(&s.ty) == "Namespace" {
                continue;
            }
            if !s.is_static {
                if let Some(val) = &s.value {
                    let simple = matches!(&*val.borrow(), Item::Int(_) | Item::Double(_) | Item::Str(_) | Item::Bool(_) | Item::Null | Item::Undefined);
                    if !simple {
                        handle_and_or_coerce(val, &s.ty);
                        initcode.push(ins(op::GET_LOCAL0, &[]));
                        ld.is_static = false;
                        initcode.extend(self.src(ld, val)?);
                        let tn = self.trait_name(ns, &s.var);
                        initcode.push(ins(if s.is_const { op::INIT_PROPERTY } else { op::SET_PROPERTY }, &[tn as i64]));
                    }
                }
            }
        }
        if !is_interface {
            if let Some(bi) = self.abc.body_of[init] {
                let len = code_len(&initcode) as usize;
                let at = if iinit_node.is_none() { 0 } else { 2 };
                if let Some((code, exc)) = self.pending.get_mut(&bi) {
                    let tail = code.split_off(at.min(code.len()));
                    code.extend(initcode);
                    code.extend(tail);
                    for e in exc.iter_mut() {
                        e.start += len;
                        e.end += len;
                        e.target += len;
                    }
                }
            }
        }
        if let Some(bi) = self.abc.body_of[cinit] {
            self.auto_fill(bi, cscope);
        }
        self.abc.classes[ci].cinit = cinit;
        if !is_interface {
            if let Some(bi) = self.abc.body_of[init] {
                self.auto_fill(bi, init_scope + 1);
            }
        }
        let mut ifs = Vec::new();
        for i in implements {
            ifs.push(self.super_int_name(ld, i)?);
        }
        self.abc.instances[ci].interfaces = ifs;
        Ok(())
    }

    /// `generateScriptInfo`.
    pub fn generate_script(&mut self, script: &super::parser::Script, script_index: usize, class_pos: usize) -> R<()> {
        let items = &script.items;
        let trait_items: Nodes = items.iter().filter(|it| matches!(&*it.borrow(), Item::Slot(_) | Item::Class(_) | Item::Interface(_) | Item::Method(_))).cloned().collect();
        let mut ld = Ld { document_class: self.document_class.clone(), current_script: Some(script_index), script_index: Some(script_index), ..Ld::default() };
        self.class_pos = class_pos;
        let mut cidx = class_pos;
        let empty_imports: Imports = Rc::new(std::cell::RefCell::new(Vec::new()));
        let empty_opened = ns_list(Vec::new());
        let traits = self.traits_phase1(&empty_imports, &empty_opened, None, true, &ld, &trait_items, Owner::Script(script_index), &mut cidx, true)?;
        self.class_pos = cidx;
        self.traits_phase2(&empty_imports, None, &trait_items, &traits, &empty_opened, &mut ld)?;
        ld.pkg = script.pkg.clone();
        let opened: Vec<Ns> = script.opened.borrow().clone();
        for ns in &opened {
            let (idx, kind) = {
                let n = ns.borrow();
                (n.index, n.kind)
            };
            if idx > -1 || kind == KIND_NAMESPACE_CUSTOM || kind == ns_kind::NAMESPACE as i32 {
                continue;
            }
            self.ns_index(ns)?;
        }
        let pkgd = script.pkg.clone().unwrap_or_default();
        for ns in &opened {
            self.resolve_custom_ns(ns, &script.imported, &pkgd, &script.opened, Some(&ld))?;
        }
        self.refresh();
        self.traits_phase3(&trait_items, &traits, &mut ld)?;
        self.refresh();
        self.str("");
        ld.imported = script.imported.clone();
        ld.opened = script.opened.clone();
        let mut sinit_vars = script.sinit_variables.clone();
        let mut cmds = items.clone();
        let sname = self.str("");
        let sinit = self.method(None, true, sname, false, false, false, &empty_stack(), None, script.sinit_activation, &mut sinit_vars, 0, false, 0, None, None, false, &ld, &mut Vec::new(), &[], &[], Some(&mut cmds), &unbounded())?;
        let doc = ld.document_class.clone().filter(|d| !d.is_empty()).map(|d| Dotted::parse_no_suffix(&d));
        let mut init_scopes: HashMap<usize, usize> = HashMap::new();
        let mut sinitcode: Code = Vec::new();
        let script_traits = self.abc.scripts[script_index].traits.clone();
        for (k, item) in trait_items.iter().enumerate() {
            let _ = item;
            let Some(r) = traits[k] else { continue };
            let t = script_traits[r.index].clone();
            let TraitData::Class { class, .. } = t.data else { continue };
            let class_name = raw_names(&self.abc).name_with_ns(t.name, true);
            let mut parents: Vec<usize> = Vec::new();
            let mut trait_scope = 1;
            if doc.as_ref() == Some(&class_name) {
                sinitcode.push(ins(op::GET_SCOPE_OBJECT, &[0]));
            } else {
                let (name, ns) = match &self.abc.constants.multinames[t.name] {
                    Multiname::QName { name, ns, .. } => (*name, *ns),
                    _ => (0, 0),
                };
                let set = pool::ns_set_id(&mut self.abc, &[ns], true) as usize;
                let m = pool::mn(&mut self.abc, Multiname::Multiname { attr: false, name, ns_set: set });
                sinitcode.push(ins(op::FIND_PROPERTY_STRICT, &[m as i64]));
            }
            trait_scope += 1;
            if self.abc.instances[class].flags & instance_flags::INTERFACE != 0 {
                sinitcode.push(ins(op::PUSH_NULL, &[]));
            } else {
                let cty = super::index::mn_type(&self.abc, self.abc.instances[class].name);
                let mut ci = self.find_class(&cty, Some(script_index));
                while let Some(c) = ci {
                    let Some(p) = self.index.class(c).parent else { break };
                    ci = Some(p);
                    let e = self.index.class(p).clone();
                    let a = self.abc_of(e.abc);
                    let om = a.constants.multinames[a.instances[e.index].name].clone();
                    if let Multiname::QName { attr, ns, name } = om {
                        let n = a.constants.namespaces.get(ns).copied().unwrap_or(crate::abc::Namespace { kind: 0, name: 0 });
                        let nstr = if name == 0 { None } else { a.constants.strings.get(name).cloned() };
                        let nsstr = if n.name == 0 { None } else { a.constants.strings.get(n.name).cloned() };
                        let s = pool::string_id(&mut self.abc, nstr.as_deref(), true) as usize;
                        let nm = pool::string_id(&mut self.abc, nsstr.as_deref(), true);
                        let nsi = pool::namespace_id_of(&mut self.abc, n.kind, nm, 0, true) as usize;
                        parents.push(pool::mn(&mut self.abc, Multiname::QName { attr, ns: nsi, name: s }));
                    }
                }
                for i in (0..parents.len()).rev() {
                    sinitcode.push(ins(op::GET_LEX, &[parents[i] as i64]));
                    sinitcode.push(ins(op::PUSH_SCOPE, &[]));
                    trait_scope += 1;
                }
                if !parents.is_empty() {
                    sinitcode.push(ins(op::GET_LEX, &[parents[0] as i64]));
                }
            }
            sinitcode.push(ins(op::NEW_CLASS, &[self.class_ids[class] as i64]));
            for _ in 0..parents.len() {
                sinitcode.push(ins(op::POP_SCOPE, &[]));
            }
            sinitcode.push(ins(op::INIT_PROPERTY, &[t.name as i64]));
            init_scopes.insert(k, trait_scope);
        }
        if let Some(bi) = self.abc.body_of[sinit] {
            let len = code_len(&sinitcode) as usize;
            if let Some((code, exc)) = self.pending.get_mut(&bi) {
                let tail = code.split_off(2.min(code.len()));
                code.extend(sinitcode);
                code.extend(tail);
                for e in exc.iter_mut() {
                    e.start += len;
                    e.end += len;
                    e.target += len;
                }
            }
        }
        self.abc.scripts[script_index].init = sinit;
        ld.pkg = Some(Dotted::default());
        let mut cidx = self.class_pos;
        self.traits_phase4(&empty_imports, &empty_opened, 1, false, None, None, true, &mut ld, &trait_items, &traits, &init_scopes, &mut cidx, true)?;
        if let Some(bi) = self.abc.body_of[sinit] {
            self.auto_fill(bi, 1);
        }
        Ok(())
    }
}

pub type Metadata_ = super::ast::Metadata;
type Meta = super::ast::Metadata;

/// Turns exception marks into offsets and drops them.
pub fn resolve_marks(code: &mut Code, exc: &mut [Exc]) {
    let mut offset = 0usize;
    let mut i = 0;
    while i < code.len() {
        let p = code[i].borrow().pseudo;
        if let Pseudo::Mark { exception, kind } = p {
            if let Some(e) = exc.get_mut(exception) {
                match kind {
                    ins::MARK_START => e.start = offset,
                    ins::MARK_END => e.end = offset,
                    ins::MARK_TARGET => e.target = offset,
                    _ => {}
                }
            }
            code.remove(i);
            continue;
        }
        offset += code[i].borrow().len();
        i += 1;
    }
}

pub fn compile_error(m: &str) -> CompileError {
    CompileError { line: 0, message: m.into() }
}

#[allow(dead_code)]
fn unused(_: &mut Code) -> Code {
    vec![int_ins(&mut crate::abc::Abc::empty(), 0)]
}
