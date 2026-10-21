//! Name resolution: `UnresolvedAVM2Item.resolve`, `PropertyAVM2Item.resolve`,
//! `NamespaceItem.resolveCustomNs` and `AVM2SourceGenerator.resolveType`.

use super::ast::*;
use super::gen::{err, node_spec, spec_node, Cx, Ld};
use super::index::{class_name, ns_key, prop_key, raw_names, trait_ret, AbcId, ClassRef, TraitIndex};
use super::ins::{ins, op, I};
use super::pool;
use super::R;
use crate::abc::decomp::index::TySpec;
use crate::abc::decomp::names::Dotted;
use crate::abc::{ns_kind, Multiname, Trait, TraitData};

/// What `PropertyAVM2Item.resolve` reports through its out-parameters.
#[derive(Clone, Default)]
pub struct Res {
    pub is_type: bool,
    pub obj_type: Option<Node>,
    pub prop_type: Option<Node>,
    pub prop_index: usize,
    pub value: Option<(usize, u8)>,
    pub value_abc: Option<AbcId>,
    pub tr: Option<(Trait, AbcId)>,
}

/// `resolveObject`'s result: an item to generate or a ready instruction.
pub enum Obj {
    Item(Node),
    Ins(I),
}

struct Proto {
    name: String,
    ns: Dotted,
    prop_ns: Dotted,
    prop_ns_kind: u8,
    prop_ns_index: usize,
    prop_type: TySpec,
    value: Option<(usize, u8)>,
    abc: AbcId,
    tr: Trait,
    is_type: bool,
}

fn trait_ns(cx: &Cx, abc: AbcId, t: &Trait) -> (usize, u8, Dotted) {
    let a = cx.abc_of(abc);
    let ns = match a.constants.multinames.get(t.name) {
        Some(Multiname::QName { ns, .. }) => *ns,
        _ => 0,
    };
    let kind = a.constants.namespaces.get(ns).map_or(0, |n| n.kind);
    (ns, kind, raw_names(a).ns_name(ns))
}

fn class_pkg(cx: &Cx, script: Option<usize>) -> Option<Dotted> {
    script.and_then(|s| super::index::script_pkg(&cx.abc, s))
}

impl Cx {
    pub fn abc_fn<'s>(&'s self) -> impl Fn(AbcId) -> &'s crate::abc::Abc + 's {
        move |id| self.abc_of(id)
    }

    pub fn find_class(&self, ty: &TySpec, script: Option<usize>) -> Option<ClassRef> {
        self.index.find_class(ty, class_pkg(self, script))
    }

    fn find_class_node(&self, n: &Node, script: Option<usize>) -> Option<ClassRef> {
        self.find_class(&node_spec(n)?, script)
    }

    pub fn class_full_name(&self, r: ClassRef) -> Dotted {
        let f = self.abc_fn();
        class_name(&self.index, &f, r)
    }

    /// `AVM2SourceGenerator.resolveType`.
    pub fn resolve_type(&mut self, ld: Option<&Ld>, item: &Node) -> R<usize> {
        let mut item = item.clone();
        if matches!(&*item.borrow(), Item::Unresolved(_)) {
            let full = ld.and_then(|l| l.full_class());
            let tt = ty_str(full.as_deref().unwrap_or(""));
            item = self.resolve_unresolved(&item, ld, full.as_deref(), Some(tt), &[], &[], &empty_stack(), &[])?.unwrap_or_else(unbounded);
        }
        let type_item = match &*item.borrow() {
            Item::Type { .. } => item.clone(),
            Item::ApplyType { obj, .. } => obj.clone(),
            _ => return err(0, format!("Invalid type:{}", text(&item))),
        };
        let mut type_item = type_item;
        if matches!(&*type_item.borrow(), Item::Unresolved(_)) {
            let full = ld.and_then(|l| l.full_class());
            let tt = ty_str(full.as_deref().unwrap_or(""));
            type_item = self.resolve_unresolved(&type_item, ld, full.as_deref(), Some(tt), &[], &[], &empty_stack(), &[])?.unwrap_or_else(unbounded);
        }
        let dname = match &*type_item.borrow() {
            Item::Type { full, .. } => full.clone(),
            _ => return err(0, "Invalid type"),
        };
        let pkg = dname.without_last();
        let name = dname.last().to_string();
        let mut name_index = 0;
        if let Some(ci) = self.find_class(&TySpec::Type(dname.clone(), None), ld.and_then(|l| l.script_index)) {
            let c = self.index.class(ci).clone();
            let a = self.abc_of(c.abc);
            let mni = a.instances[c.index].name;
            if mni != 0 {
                let names = raw_names(a);
                let n = names.name(mni, &[], true, true);
                let ns = match a.constants.multinames.get(mni) {
                    Some(Multiname::QName { ns, .. }) if *ns != 0 => Some(*ns),
                    _ => None,
                };
                let (kind, nsn) = match ns {
                    None => (ns_kind::PACKAGE, None),
                    Some(ns) => (a.constants.namespaces[ns].kind, Some(names.ns_raw_name(ns))),
                };
                let si = pool::str_id(&mut self.abc, &n);
                let nsi = match nsn {
                    Some(s) => pool::ns(&mut self.abc, kind, &s),
                    None => pool::namespace_id_of(&mut self.abc, kind, 0, 0, true) as usize,
                };
                name_index = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns: nsi, name: si });
            }
        }
        if name_index == 0 {
            if pkg.is_empty() {
                if let Some(script) = ld.and_then(|l| l.current_script) {
                    let names = raw_names(&self.abc);
                    for t in &self.abc.scripts[script].traits {
                        if names.name(t.name, &[], true, true) == name {
                            name_index = t.name;
                            break;
                        }
                    }
                }
            }
            if name_index == 0 {
                let si = self.str(&name);
                let nsi = pool::ns(&mut self.abc, ns_kind::PACKAGE, &pkg.raw());
                name_index = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns: nsi, name: si });
            }
        }
        let params = match &*item.borrow() {
            Item::ApplyType { params, .. } => Some(params.clone()),
            _ => None,
        };
        if let Some(params) = params {
            let mut ps = Vec::new();
            for p in &params {
                ps.push(if matches!(&*p.borrow(), Item::Null) { 0 } else { self.resolve_type(ld, p)? });
            }
            return Ok(pool::mn(&mut self.abc, Multiname::TypeName { name: name_index, params: ps }));
        }
        Ok(name_index)
    }

    /// `UnresolvedAVM2Item.resolve`; returns `resolvedRoot`.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_unresolved(&mut self, un: &Node, ld: Option<&Ld>, class_full: Option<&str>, this_type: Option<Node>, param_types: &[Node], param_names: &[String], call_stack: &CallStack, variables: &[Node]) -> R<Option<Node>> {
        let u = match &*un.borrow() {
            Item::Unresolved(u) => u.clone(),
            _ => return Ok(Some(un.clone())),
        };
        let name = &u.name;
        let n0 = name.0.first().map(|p| p.name.clone()).unwrap_or_default();
        let part = |i: usize| name.0[i].clone();
        let chain = |cx: &mut Cx, root: Node, from: usize, stack: &CallStack| -> Node {
            let _ = cx;
            let mut resolved = root;
            for i in from..name.len() {
                let p = part(i);
                resolved = property(Some(resolved), p.attr, &p.name, &p.suffix, u.opened.clone(), stack.clone(), false, None, u.line, u.this_type.clone());
                if i == name.len() - 1 {
                    set_assigned(&resolved, u.assigned.clone());
                }
            }
            resolved
        };
        let set = |resolved: Node, root: Node| {
            if let Item::Unresolved(x) = &mut *un.borrow_mut() {
                x.resolved = Some(resolved);
                x.resolved_root = Some(root.clone());
            }
            Some(root)
        };
        if u.scope_stack.is_empty() {
            let def = variables.iter().find(|v| matches!(&*v.borrow(), Item::Name(n) if n.definition && n.var == n0)).cloned();
            if let Some(d) = def {
                let (dt, dl, dc, ds, dsn, dr, dns) = match &*d.borrow() {
                    Item::Name(n) => (n.ty.clone(), n.line, n.is_const, n.slot_scope, n.slot, n.reg, n.ns.clone()),
                    _ => unreachable!(),
                };
                let p0 = part(0);
                let ret = node(new_name(dt, dl, p0.attr, &p0.name, &p0.suffix, None, false, u.opened.clone(), dc));
                if let Item::Name(n) = &mut *ret.borrow_mut() {
                    n.slot_scope = ds;
                    n.slot = dsn;
                    n.reg = dr;
                }
                let resolved = chain(self, ret.clone(), 1, &empty_stack());
                if name.len() == 1 {
                    set_assigned(&ret, u.assigned.clone());
                }
                if let Item::Name(n) = &mut *ret.borrow_mut() {
                    n.ns = dns;
                }
                return Ok(set(resolved, ret));
            }
        }
        if param_names.contains(&n0) || n0 == "arguments" {
            let t = match param_names.iter().position(|p| *p == n0) {
                Some(i) if i < param_types.len() => param_types[i].clone(),
                _ => unbounded(),
            };
            let p0 = part(0);
            let ret = node(new_name(Some(t), u.line, p0.attr, &p0.name, &p0.suffix, None, false, u.opened.clone(), false));
            let resolved = chain(self, ret.clone(), 1, &empty_stack());
            if name.len() == 1 {
                set_assigned(&ret, u.assigned.clone());
            }
            return Ok(set(resolved, ret));
        }
        let mut is_property = false;
        if let Some(ld) = ld {
            let p0 = part(0);
            let rx = property(None, p0.attr, &p0.name, &p0.suffix, u.opened.clone(), call_stack.clone(), false, None, u.line, u.this_type.clone());
            if let Item::Property(p) = &mut *rx.borrow_mut() {
                p.scope_stack = u.scope_stack.clone();
                p.assigned = u.assigned.clone();
            }
            let r = self.property_resolve(&rx, true, ld)?;
            if r.obj_type.is_some() && !r.is_type {
                is_property = true;
            }
        }
        if class_full.is_some() && !is_property {
            let pkg = Dotted::parse_with_suffix(class_full.unwrap()).without_last();
            if !pkg.is_empty() {
                let ti = TySpec::Type(pkg.add_with_suffix(&n0), None);
                if self.index.find_class(&ti, None).is_some() {
                    let tn = spec_node(&ti);
                    let resolved = chain(self, tn.clone(), 1, &empty_stack());
                    return Ok(set(resolved, tn));
                }
            }
        }
        if class_full.is_some() && !is_property {
            let ti = TySpec::Type(Dotted::default().add_with_suffix(&n0), None);
            if self.index.find_class(&ti, None).is_some() {
                for imp in u.imported.borrow().iter() {
                    if imp.last() == n0 {
                        return err(u.line, format!("The type \"{n0}\" exists on toplevel package and also as an import from different package. Please make it fully qualified so it matches the desired import."));
                    }
                }
                let tn = spec_node(&ti);
                let resolved = chain(self, tn.clone(), 1, &empty_stack());
                return Ok(set(resolved, tn));
            }
        }
        if !is_property {
            let imports = u.imported.borrow().clone();
            for imp in &imports {
                if imp == name {
                    let it = TySpec::Type(imp.clone(), None);
                    if self.find_class(&it, ld.and_then(|l| l.script_index)).is_none() && self.index.find_script_property(imp.last(), &imp.without_last()).is_some() {
                        let ret = node(Item::ScriptProperty { ty: spec_node(&it), assigned: u.assigned.clone() });
                        return Ok(set(ret.clone(), ret));
                    }
                }
                if imp.last() == n0 {
                    let it = TySpec::Type(imp.clone(), None);
                    let tn = spec_node(&it);
                    let mut resolved = chain(self, tn.clone(), 1, &empty_stack());
                    if name.len() == 1 && self.find_class(&it, ld.and_then(|l| l.script_index)).is_none() && self.index.find_script_property(imp.last(), &imp.without_last()).is_some() {
                        resolved = node(Item::ScriptProperty { ty: tn, assigned: u.assigned.clone() });
                        return Ok(set(resolved.clone(), resolved));
                    }
                    return Ok(set(resolved, tn));
                }
            }
        }
        if !is_property {
            for i in 0..name.len() {
                let fname = Dotted(name.0[..i + 1].to_vec());
                let ti = TySpec::Type(fname.clone(), None);
                let found = match ld {
                    Some(l) => self.find_class(&ti, l.script_index).is_some(),
                    None => self.index.find_class(&ti, None).is_some(),
                };
                if found {
                    let tn = spec_node(&ti);
                    let resolved = chain(self, tn.clone(), i + 1, &empty_stack());
                    if name.len() == i + 1 && u.assigned.is_some() {
                        return err(u.line, "Cannot assign type");
                    }
                    return Ok(set(resolved, tn));
                }
            }
            let pkg = Dotted::parse_with_suffix(class_full.unwrap_or("")).without_last();
            let opened: Vec<Ns> = u.opened.borrow().clone();
            for n in &opened {
                self.resolve_custom_ns(n, &u.imported, &pkg, &u.opened, ld)?;
                let idx = self.ns_index(n)?;
                let nsname = raw_names(&self.abc).ns_name(idx);
                let ti = TySpec::Type(nsname.add_with_suffix(&n0), None);
                if self.index.find_class(&ti, None).is_some() {
                    let tn = spec_node(&ti);
                    let resolved = chain(self, tn.clone(), 1, &empty_stack());
                    if name.len() == 1 && u.assigned.is_some() {
                        return err(u.line, "Cannot assign type");
                    }
                    return Ok(set(resolved, tn));
                }
            }
        }
        if !is_property && (n0 == "this" || n0 == "super") {
            let Some(tt) = this_type.clone() else { return err(u.line, "Cannot use this in that context") };
            let mut ntype = tt.clone();
            if n0 == "super" {
                let Some(ci) = self.find_class_node(&tt, None) else { return err(u.line, "This class not found") };
                ntype = match self.index.class(ci).parent {
                    None => ty_parts(&["Object"]),
                    Some(p) => ty(self.class_full_name(p)),
                };
            }
            let p0 = part(0);
            let ret = node(new_name(Some(ntype), u.line, p0.attr, &p0.name, &p0.suffix, None, false, u.opened.clone(), false));
            let resolved = chain(self, ret.clone(), 1, &empty_stack());
            if name.len() == 1 {
                set_assigned(&ret, u.assigned.clone());
            }
            return Ok(set(resolved, ret));
        }
        if !is_property && name.len() == 1 && n0 == "Vector" {
            let ret = ty_parts(VECTOR_FQN);
            return Ok(set(ret.clone(), ret));
        }
        if u.must_be_type {
            return err(u.line, format!("{} is not an existing type", name.raw()));
        }
        let mut resolved: Option<Node> = None;
        let mut root: Option<Node> = None;
        for i in 0..name.len() {
            let p = part(i);
            let r = property(resolved.clone(), p.attr, &p.name, &p.suffix, u.opened.clone(), call_stack.clone(), false, None, u.line, u.this_type.clone());
            if root.is_none() {
                if let Item::Property(x) = &mut *r.borrow_mut() {
                    x.scope_stack = u.scope_stack.clone();
                }
                root = Some(r.clone());
            }
            if i == name.len() - 1 {
                set_assigned(&r, u.assigned.clone());
            }
            resolved = Some(r);
        }
        match (resolved, root) {
            (Some(r), Some(rt)) => Ok(set(r, rt)),
            _ => Ok(None),
        }
    }

    /// `NamespaceItem.resolveCustomNs`.
    pub fn resolve_custom_ns(&mut self, ns: &Ns, imported: &Imports, pkg: &Dotted, opened: &NsList, ld: Option<&Ld>) -> R<()> {
        let (idx, kind, name, line) = {
            let n = ns.borrow();
            (n.index, n.kind, n.name.clone().unwrap_or_default(), n.line)
        };
        if idx > -1 {
            return Ok(());
        }
        if kind == ns_kind::NAMESPACE as i32 {
            let i = pool::namespace_id(&mut self.abc, ns_kind::NAMESPACE, &name.raw(), 0, true);
            ns.borrow_mut().index = i;
        }
        if kind == KIND_NAMESPACE_CUSTOM {
            let custom = name.raw();
            let prop = property(None, false, &custom, "", opened.clone(), empty_stack(), false, None, 0, Some(ty_str("")));
            let default_ld = Ld::default();
            let r = self.property_resolve(&prop, true, ld.unwrap_or(&default_ld))?;
            let Some(value) = r.value else {
                let mut ti: Option<TraitIndex> = None;
                for imp in imported.borrow().iter() {
                    if imp.last() == custom {
                        ti = self.index.find_script_property(imp.last(), &imp.without_last()).cloned();
                        break;
                    }
                }
                if ti.is_none() {
                    let full = pkg.add(&custom, "");
                    ti = self.index.find_script_property(full.last(), &full.without_last()).cloned();
                }
                if let Some(ti) = ti {
                    if let TraitData::Slot { value: Some(v), .. } | TraitData::Const { value: Some(v), .. } = ti.tr.data {
                        if is_ns_value_kind(v.kind) {
                            let a = self.abc_of(ti.abc);
                            let n = a.constants.namespaces.get(v.index).copied().unwrap_or(crate::abc::Namespace { kind: 0, name: 0 });
                            let nname = raw_names(a).ns_name(v.index).raw();
                            let i = pool::namespace_id(&mut self.abc, n.kind, &nname, 0, true);
                            let mut b = ns.borrow_mut();
                            b.index = i;
                            b.kind = n.kind as i32;
                            return Ok(());
                        }
                    }
                }
                return err(line, format!("Namespace \"{}\" not defined", name.raw()));
            };
            let vabc = r.value_abc.unwrap_or(AbcId::Sel);
            let a = self.abc_of(vabc);
            let n = a.constants.namespaces.get(value.0).copied().unwrap_or(crate::abc::Namespace { kind: 0, name: 0 });
            let nname = raw_names(a).ns_name(value.0).raw();
            let i = pool::namespace_id(&mut self.abc, n.kind, &nname, 0, true);
            let mut b = ns.borrow_mut();
            b.index = i;
            b.kind = ns_kind::NAMESPACE as i32;
        }
        Ok(())
    }

    /// `genNs`.
    pub fn gen_ns(&mut self, imported: &Imports, pkg: &Dotted, ns: &Ns, opened: &NsList, ld: Option<&Ld>) -> R<usize> {
        self.resolve_custom_ns(ns, imported, pkg, opened, ld)?;
        self.ns_index(ns)
    }

    fn search_one(&mut self, selected_ns: i64, instance_only: bool, pkg: &Dotted, obj: &str, prop: &str) -> Option<Proto> {
        let full = pkg.add_with_suffix(prop);
        let mut sp: Option<TraitIndex> = self.index.find_script_property(full.last(), &full.without_last()).cloned();
        if sp.is_none() {
            let parent = TySpec::Type(pkg.add_with_suffix(obj), None);
            let key = prop_key(prop, parent, Some(&self.abc), selected_ns.max(0) as usize);
            sp = {
                let f = self.abc_fn();
                self.index.find_property(&f, &key, !instance_only, true, true).map(|(t, _)| t)
            };
        }
        let sp = sp?;
        let is_type = matches!(sp.tr.data, TraitData::Class { .. });
        let (name, ns) = match &sp.obj_type {
            TySpec::Type(f, _) => (f.last().to_string(), f.without_last()),
            _ => (String::new(), Dotted::default()),
        };
        let (nsi, kind, prop_ns) = trait_ns(self, sp.abc, &sp.tr);
        let prop_ns_index = if sp.abc == AbcId::Sel { pool::ns_sub_index(&self.abc, nsi) } else { 0 };
        Some(Proto { name, ns, prop_ns, prop_ns_kind: kind, prop_ns_index, prop_type: sp.ret.clone(), value: sp.value, abc: sp.abc, tr: sp.tr.clone(), is_type })
    }

    /// `searchPrototypeChain` with the namespace order FFDec tries.
    #[allow(clippy::too_many_arguments)]
    fn search_prototype_chain(&mut self, ns_keyword: Option<&str>, suffix: Option<i64>, other_ns: &[i64], ld: &Ld, instance_only: bool, pkg: &Dotted, obj: &str, prop: &str) -> Option<Proto> {
        let public_ns = pool::namespace_id(&mut self.abc, ns_kind::PACKAGE, "", 0, false);
        if let Some(k) = ns_keyword {
            match k {
                "public" => return self.search_one(public_ns, instance_only, pkg, obj, prop),
                "protected" => {
                    if let Some(r) = self.search_one(ld.static_protected_ns as i64, instance_only, pkg, obj, prop) {
                        return Some(r);
                    }
                    return self.search_one(ld.protected_ns as i64, instance_only, pkg, obj, prop);
                }
                "private" => return self.search_one(ld.private_ns as i64, instance_only, pkg, obj, prop),
                "internal" => return self.search_one(ld.internal_ns as i64, instance_only, pkg, obj, prop),
                _ => {}
            }
        }
        if let Some(s) = suffix {
            if let Some(r) = self.search_one(s, instance_only, pkg, obj, prop) {
                return Some(r);
            }
        }
        if let Some(r) = self.search_one(ld.private_ns as i64, instance_only, pkg, obj, prop) {
            return Some(r);
        }
        if let Some(r) = self.search_one(ld.protected_ns as i64, instance_only, pkg, obj, prop) {
            return Some(r);
        }
        for &n in other_ns {
            if let Some(r) = self.search_one(n, instance_only, pkg, obj, prop) {
                return Some(r);
            }
        }
        self.search_one(public_ns, instance_only, pkg, obj, prop)
    }

    fn proto_prop_index(&mut self, prop: &str, suffix: Option<i64>, p: &Proto) -> usize {
        let s = self.str(prop);
        let ns = match suffix {
            Some(x) => x as usize,
            None => {
                let nm = pool::string_id(&mut self.abc, Some(&p.prop_ns.raw()), true);
                pool::namespace_id_of(&mut self.abc, p.prop_ns_kind, nm, p.prop_ns_index, true) as usize
            }
        };
        pool::mn(&mut self.abc, Multiname::QName { attr: false, ns, name: s })
    }

    /// `PropertyAVM2Item.resolve`.
    pub fn property_resolve(&mut self, pn: &Node, must_exist: bool, ld: &Ld) -> R<Res> {
        let p = match &*pn.borrow() {
            Item::Property(p) => p.clone(),
            _ => return Ok(Res::default()),
        };
        let mut opened_ids: Vec<i64> = Vec::new();
        for n in p.opened.borrow().iter() {
            let i = n.borrow().index;
            if i > -1 {
                opened_ids.push(i);
            }
        }
        let mut suffix: Option<i64> = if p.suffix.is_empty() { None } else { p.suffix[1..].parse().ok() };
        if let Some(k) = &p.ns_keyword {
            match k.as_str() {
                "public" => {
                    let pubns = pool::namespace_id(&mut self.abc, ns_kind::PACKAGE, "", 0, false);
                    suffix = Some(pubns);
                    opened_ids = vec![pubns];
                }
                "private" => {
                    suffix = Some(ld.private_ns as i64);
                    opened_ids = vec![ld.private_ns as i64];
                }
                "protected" => opened_ids = vec![ld.static_protected_ns as i64, ld.protected_ns as i64],
                "internal" => {
                    suffix = Some(ld.internal_ns as i64);
                    opened_ids = vec![ld.internal_ns as i64];
                }
                _ => {}
            }
        }
        let mut res = Res::default();
        let this_type = ty_str(ld.full_class().as_deref().unwrap_or(""));
        let mut obj_type: Option<Node> = None;
        if let Some(o) = &p.object {
            let mut oret = self.ret_type(o)?;
            let unres = match &*oret.borrow() {
                Item::Unresolved(u) => Some(u.resolved.clone()),
                _ => None,
            };
            if let Some(r) = unres {
                oret = r.ok_or(super::CompileError { line: p.line, message: format!("Undefined variable or property: {}", text(o)) })?;
            }
            obj_type = Some(match &*oret.borrow() {
                Item::ApplyType { .. } | Item::Type { .. } => oret.clone(),
                _ => ty_str(&text(&oret)),
            });
        }
        let mut prop_type: Option<Node> = None;
        let mut prop_index = 0usize;
        if !p.attribute && p.scope_stack.is_empty() {
            let mut ttype = obj_type.clone().unwrap_or_else(|| this_type.clone());
            if is_type_named(&ttype, VECTOR_FQN) {
                ttype = ty_parts(&["__AS3__", "vec", "Vector$object"]);
            }
            let mut found_in_stack = false;
            if obj_type.is_none() && p.ns_keyword.is_none() {
                let bodies: Vec<Body> = p.call_stack.borrow().clone();
                for b in &bodies {
                    let traits = b.borrow().clone();
                    for t in &traits {
                        if raw_names(&self.abc).name(t.name, &[], true, true) == p.name {
                            if let TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } = t.data {
                                obj_type = Some(ty_parts(&["Function"]));
                                prop_type = Some(spec_node(&super::index::mn_type(&self.abc, type_name)));
                                prop_index = t.name;
                                found_in_stack = true;
                            }
                        }
                    }
                }
            }
            if !found_in_stack {
                let ftn = match &*ttype.borrow() {
                    Item::Type { full, .. } => Some(full.clone()),
                    _ => None,
                };
                if let Some(ftn) = ftn {
                    let other_ns = opened_ids.clone();
                    let is_super = p.object.as_ref().is_some_and(|o| matches!(&*o.borrow(), Item::Name(n) if n.var == "super"));
                    if is_super {
                        if let Some(ot) = obj_type.as_ref().and_then(node_spec) {
                            if let Some(ci) = self.index.find_class(&ot, None) {
                                let c = self.index.class(ci).clone();
                                let ca = self.abc_of(c.abc);
                                let sup_prot = ca.instances[c.index].protected_ns;
                                let key = prop_key(&p.name, ot.clone(), Some(ca), sup_prot);
                                let sp = {
                                    let f = self.abc_fn();
                                    self.index.find_property(&f, &key, false, true, true).map(|(t, _)| t)
                                };
                                if let Some(sp) = sp {
                                    obj_type = Some(spec_node(&sp.obj_type));
                                    let (nsi, kind, nsname) = trait_ns(self, sp.abc, &sp.tr);
                                    let sub = if sp.abc == AbcId::Sel { pool::ns_sub_index(&self.abc, nsi) } else { 0 };
                                    let s = self.str(&p.name);
                                    let nm = pool::string_id(&mut self.abc, Some(&nsname.raw()), true);
                                    let ns = pool::namespace_id_of(&mut self.abc, kind, nm, sub, true) as usize;
                                    prop_index = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns, name: s });
                                    prop_type = Some(spec_node(&sp.ret));
                                    res.value = sp.value;
                                    res.value_abc = Some(sp.abc);
                                    res.tr = Some((sp.tr.clone(), sp.abc));
                                }
                            }
                        }
                    }
                    if prop_type.is_none() {
                        if let Some(pr) = self.search_prototype_chain(p.ns_keyword.as_deref(), suffix, &other_ns, ld, false, &ftn.without_last(), ftn.last(), &p.name) {
                            res.is_type = pr.is_type;
                            obj_type = Some(ty(pr.ns.add_with_suffix(&pr.name)));
                            prop_type = Some(spec_node(&pr.prop_type));
                            prop_index = self.proto_prop_index(&p.name, suffix, &pr);
                            res.value = pr.value;
                            res.value_abc = Some(pr.abc);
                            res.tr = Some((pr.tr.clone(), pr.abc));
                            let (nsi, kind, nsname) = trait_ns(self, pr.abc, &pr.tr);
                            let pkg = ftn.without_last().raw();
                            match kind {
                                ns_kind::PRIVATE => {
                                    if !(other_ns.contains(&(nsi as i64)) && pr.abc == AbcId::Sel) {
                                        return err(p.line, format!("Property {} has private access in {}", p.name, text(obj_type.as_ref().unwrap())));
                                    }
                                }
                                ns_kind::PACKAGE_INTERNAL => {
                                    if ld.pkg.as_ref().map(|d| d.raw()) != Some(pkg) {
                                        return err(p.line, format!("Property {} has package internal access in {}", p.name, text(obj_type.as_ref().unwrap())));
                                    }
                                }
                                ns_kind::PROTECTED | ns_kind::STATIC_PROTECTED => {
                                    let ns_name = nsname.raw();
                                    let mut ci = node_spec(&this_type).and_then(|t| self.find_class(&t, ld.script_index));
                                    let mut found = false;
                                    while let Some(c) = ci {
                                        let full = self.class_full_name(c);
                                        let cp = full.without_last();
                                        let cn = full.last().to_string();
                                        let cns = if cp.is_empty() { cn } else { format!("{}:{}", cp.raw(), cn) };
                                        if ns_name == cns {
                                            found = true;
                                            break;
                                        }
                                        ci = self.index.class(c).parent;
                                    }
                                    if !found {
                                        return err(p.line, format!("Property {} has protected access in {}", p.name, text(obj_type.as_ref().unwrap())));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            if obj_type.is_none() {
                if let Some(r) = self.resolve_in_opened(&p, &opened_ids, suffix, ld)? {
                    res.is_type = r.is_type;
                    obj_type = r.obj_type;
                    prop_type = r.prop_type;
                    prop_index = r.prop_index;
                    if r.tr.is_some() {
                        res.value = r.value;
                        res.value_abc = r.value_abc;
                        res.tr = r.tr;
                    }
                }
            }
        }
        if prop_index == 0 && !must_exist {
            let m = if p.attribute && p.name.is_empty() {
                let pk = ld.pkg.as_ref().map_or(String::new(), |d| d.raw());
                let ns = pool::ns(&mut self.abc, ns_kind::PACKAGE_INTERNAL, &pk);
                let set = pool::ns_set_id(&mut self.abc, &[ns], true) as usize;
                Multiname::MultinameL { attr: true, ns_set: set }
            } else {
                let ni = pool::string_id(&mut self.abc, if p.name == "*" { None } else { Some(&p.name) }, true) as usize;
                let set = self.ns_set_of(&p.opened)?;
                Multiname::Multiname { attr: p.attribute, name: ni, ns_set: set }
            };
            prop_index = pool::mn(&mut self.abc, m);
            prop_type = Some(unbounded());
            obj_type = Some(unbounded());
            res.value = None;
            res.value_abc = None;
        }
        res.obj_type = obj_type;
        res.prop_type = prop_type;
        res.prop_index = prop_index;
        Ok(res)
    }

    fn resolve_in_opened(&mut self, p: &Property, opened_ids: &[i64], suffix: Option<i64>, ld: &Ld) -> R<Option<Res>> {
        for &nsindex in opened_ids {
            let nsindex = nsindex as usize;
            let Some(ns) = self.abc.constants.namespaces.get(nsindex).copied() else { continue };
            let nsname = raw_names(&self.abc).ns_name(nsindex);
            if nsname.is_empty() {
                continue;
            }
            let sid = pool::string_id(&mut self.abc, Some(&p.name), false);
            let mut name_index = 0;
            if sid > -1 {
                for (m, mn) in self.abc.constants.multinames.iter().enumerate().skip(1) {
                    if let Multiname::QName { attr: false, ns, name } = mn {
                        if *name as i64 == sid && *ns == nsindex {
                            name_index = m;
                            break;
                        }
                    }
                }
            }
            if name_index > 0 {
                for (si, s) in self.abc.scripts.iter().enumerate() {
                    if self.deleted.scripts.contains(&si) {
                        continue;
                    }
                    for t in &s.traits {
                        if t.name == name_index {
                            let mut r = Res { is_type: matches!(t.data, TraitData::Class { .. }), obj_type: Some(ty_parts(&["Object"])), prop_type: Some(spec_node(&generator_trait_ret(&self.abc, t))), prop_index: t.name, ..Default::default() };
                            if let TraitData::Slot { value, .. } | TraitData::Const { value, .. } = t.data {
                                r.value = Some(value.map_or((0, 0), |v| (v.index, v.kind)));
                                r.value_abc = Some(AbcId::Sel);
                                r.tr = Some((t.clone(), AbcId::Sel));
                            }
                            return Ok(Some(r));
                        }
                    }
                }
            }
            if ns.kind == ns_kind::PACKAGE {
                let key = ns_key(&p.name, nsname.clone(), Some((&self.abc, AbcId::Sel)), nsindex);
                let found = {
                    let f = self.abc_fn();
                    self.index.find_ns_property(&f, &key, true, true).map(|(t, _)| t)
                };
                if let Some(pt) = found {
                    if let TySpec::Type(full, _) = &pt.obj_type {
                        let last = full.last().to_string();
                        if let Some(pr) = self.search_prototype_chain(p.ns_keyword.as_deref(), suffix, opened_ids, ld, false, &nsname, &last, &p.name) {
                            let prop_index = self.proto_prop_index(&p.name, suffix, &pr);
                            return Ok(Some(Res {
                                is_type: pr.is_type,
                                obj_type: Some(ty(pr.ns.add_with_suffix(&pr.name))),
                                prop_type: Some(spec_node(&pt.ret)),
                                prop_index,
                                value: pt.value,
                                value_abc: Some(pr.abc),
                                tr: Some((pr.tr.clone(), pr.abc)),
                            }));
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    /// `PropertyAVM2Item.returnType`.
    pub fn property_ret_type(&mut self, pn: &Node) -> R<Node> {
        let this_type = match &*pn.borrow() {
            Item::Property(p) => p.this_type.clone(),
            _ => None,
        };
        let mut ld = Ld::default();
        if let Some(t) = this_type {
            if let Item::Type { full, .. } = &*t.borrow() {
                ld.pkg = Some(full.without_last());
                ld.current_class = Some(full.last().to_string());
            }
        }
        match self.property_resolve(pn, false, &ld) {
            Ok(r) => Ok(r.prop_type.unwrap_or_else(unbounded)),
            Err(_) => Ok(unbounded()),
        }
    }

    pub fn resolve_property_index(&mut self, pn: &Node, ld: &Ld) -> R<usize> {
        Ok(self.property_resolve(pn, false, ld)?.prop_index)
    }

    /// `resolveObject`.
    pub fn resolve_object(&mut self, pn: &Node, ld: &Ld, must_exist: bool) -> R<Obj> {
        let obj = match &*pn.borrow() {
            Item::Property(p) => p.object.clone(),
            _ => None,
        };
        if let Some(o) = obj {
            return Ok(Obj::Item(o));
        }
        let r = self.property_resolve(pn, false, ld)?;
        Ok(Obj::Ins(ins(if must_exist { op::FIND_PROPERTY_STRICT } else { op::FIND_PROPERTY }, &[r.prop_index as i64])))
    }

    /// The parser's handling of `extends`: protected namespaces of every ancestor.
    pub fn parent_names_of_extends(&mut self, ext: &Node, full: &str) -> R<Vec<(String, String)>> {
        let resolved = self.resolve_unresolved(ext, None, Some(full), None, &[], &[], &empty_stack(), &[])?.unwrap_or_else(unbounded);
        let mut ld = Ld::default();
        ld.pkg = Some(Dotted::default());
        let ni = self.resolve_type(Some(&ld), &resolved)?;
        let name = raw_names(&self.abc).name_with_ns(ni, true);
        let mut out = Vec::new();
        let mut foreign = Vec::new();
        let mut ci = self.index.find_class(&TySpec::Type(name, None), None);
        while let Some(c) = ci {
            let e = self.index.class(c).clone();
            let a = self.abc_of(e.abc);
            let names = raw_names(a);
            let mni = a.instances[e.index].name;
            let n = names.name(mni, &[], true, true);
            let ns = match a.constants.multinames.get(mni) {
                Some(Multiname::QName { ns, .. }) => *ns,
                _ => 0,
            };
            let nsname = names.ns_name(ns).raw();
            if e.abc != AbcId::Sel {
                let kind = a.constants.namespaces.get(ns).map_or(0, |x| x.kind);
                foreign.push((names.name(mni, &[], true, true), kind, nsname.clone()));
            }
            out.push((n, nsname));
            ci = e.parent;
        }
        for (n, kind, nsname) in foreign {
            let s = self.str(&n);
            let ns = pool::ns(&mut self.abc, kind, &nsname);
            pool::mn(&mut self.abc, Multiname::QName { attr: false, ns, name: s });
        }
        Ok(out)
    }
}

/// `AVM2SourceGenerator.getTraitReturnType`, which leaves plain methods untyped.
fn generator_trait_ret(abc: &crate::abc::Abc, t: &Trait) -> TySpec {
    match t.data {
        TraitData::Method { .. } => TySpec::Unbounded,
        _ => trait_ret(abc, t),
    }
}

pub fn is_ns_value_kind(k: u8) -> bool {
    matches!(k, 0x08 | 0x16 | 0x17 | 0x18 | 0x19 | 0x1a | 0x05)
}
