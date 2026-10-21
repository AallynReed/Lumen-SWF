//! AS3 source of whole scripts: FFDec's `ScriptPack`, the `Trait*.toString` family and import resolution.

use std::collections::{HashMap, HashSet};

use super::code::{op, Code};
use super::ctx::{Ctx, LocalData};
use super::index;
use super::item::*;
use super::method::{self, ConvertData, Converted, MethodReq, TRAIT_CLASS_INITIALIZER, TRAIT_INSTANCE_INITIALIZER, TRAIT_SCRIPT_INITIALIZER};
use super::names::Dotted;
use super::writer::Writer;
use crate::abc::opcodes::Operand;
use crate::abc::{instance_flags, method_flags, ns_kind, trait_attr, Abc, Multiname, Trait, TraitData};

const BUILT_IN_CLASSES: &[&str] = &[
    "ArgumentError", "arguments", "Array", "Boolean", "Class", "Date", "DefinitionError", "Error", "EvalError", "Function", "int", "JSON", "Math", "Namespace", "Number", "Object", "QName",
    "RangeError", "ReferenceError", "RegExp", "SecurityError", "String", "SyntaxError", "TypeError", "uint", "URIError", "VerifyError", "XML", "XMLList",
];

const DEP_NAMESPACE: u8 = 0;
const DEP_INHERITANCE: u8 = 1;
const DEP_SIGNATURE: u8 = 2;
const DEP_EXPRESSION: u8 = 3;

/// A script pack: one public definition and the private ones that follow it (`ScriptInfo.getPacks`).
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    pub script: usize,
    pub traits: Vec<usize>,
    pub is_simple: bool,
}

fn is_package_kind(k: u8) -> bool {
    k == ns_kind::PACKAGE || k == ns_kind::PACKAGE_INTERNAL
}

/// `ScriptInfo.getPacks`.
pub fn packs(ctx: &Ctx, script: usize) -> Vec<Pack> {
    let names = ctx.names();
    let traits = &ctx.abc.scripts[script].traits;
    let kind = |t: &Trait| names.simple_ns_kind(t.name);
    let mut initial_private: Vec<usize> = Vec::new();
    for (j, t) in traits.iter().enumerate() {
        if is_package_kind(kind(t)) {
            break;
        }
        initial_private.push(j);
    }
    let public_count = traits.iter().filter(|t| is_package_kind(kind(t))).count();
    let is_simple = public_count == 1;
    let mut ret = Vec::new();
    for (j, t) in traits.iter().enumerate() {
        if !is_simple && matches!(t.data, TraitData::Slot { .. } | TraitData::Const { .. }) {
            continue;
        }
        if !is_package_kind(kind(t)) {
            continue;
        }
        let mut ti = vec![j];
        ti.append(&mut initial_private);
        for (k, t2) in traits.iter().enumerate().skip(j + 1) {
            if is_package_kind(kind(t2)) {
                break;
            }
            ti.push(k);
        }
        ret.push(Pack { script, traits: ti, is_simple });
    }
    if ret.is_empty() && !initial_private.is_empty() {
        for &t in &initial_private {
            ret.push(Pack { script, traits: vec![t], is_simple: false });
        }
    }
    if !is_simple {
        ret.push(Pack { script, traits: Vec::new(), is_simple: false });
    }
    ret
}

/// Each pack's class path with its script's dependencies, as `SwfToSwcExporter` lists them.
pub fn swc_packs(ctx: &Ctx, with_deps: bool) -> Vec<(Dotted, Vec<(Dotted, u8)>)> {
    let names = ctx.names();
    let sp = Sp { ctx, cd: ConvertData::default(), convs: HashMap::new(), class_init_empty: HashMap::new(), script_init_empty: true, fla_ignored: HashMap::new() };
    let mut out = Vec::new();
    for script in 0..ctx.abc.scripts.len() {
        let mut deps = Vec::new();
        if with_deps {
            let ignore = Some(Dotted::new(&["NO:PACKAGE"]));
            for t in &ctx.abc.scripts[script].traits {
                sp.trait_deps(t, script, -1, false, &None, &mut deps, &ignore, &mut Vec::new());
            }
        }
        let deps: Vec<(Dotted, u8)> = deps.into_iter().map(|d| (d.id, d.ty)).collect();
        for p in packs(ctx, script) {
            let path = match p.traits.first() {
                Some(&t) => {
                    let n = ctx.abc.scripts[script].traits[t].name;
                    names.simple_ns_name(n).unwrap_or_default().add(&names.name(n, &[], true, false), "")
                }
                None => Dotted::new(&[&format!("script_{script}")]),
            };
            out.push((path, deps.clone()));
        }
    }
    out
}

/// The pack a script entry of the tree belongs to.
pub fn pack_of(ctx: &Ctx, script: usize, trait_index: Option<usize>) -> Pack {
    let all = packs(ctx, script);
    match trait_index {
        Some(t) => all.iter().find(|p| p.traits.first() == Some(&t)).cloned().unwrap_or(Pack { script, traits: vec![t], is_simple: false }),
        None => all.into_iter().find(|p| p.traits.is_empty()).unwrap_or(Pack { script, traits: Vec::new(), is_simple: false }),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Dep {
    id: Dotted,
    ty: u8,
}

fn push_dep(deps: &mut Vec<Dep>, d: Dep) {
    if !deps.contains(&d) {
        deps.push(d);
    }
}

/// Printing state of one script pack.
struct Sp<'c, 'a> {
    ctx: &'c Ctx<'a>,
    cd: ConvertData,
    convs: HashMap<usize, Result<Converted, String>>,
    class_init_empty: HashMap<usize, bool>,
    script_init_empty: bool,
    /// Instance traits left out in FLA mode, by class.
    fla_ignored: HashMap<usize, Vec<String>>,
}

fn call_prop(it: &It) -> Option<(String, crate::abc::decomp::item::List)> {
    let b = it.borrow();
    let K::CallProperty { prop, args, .. } = &b.k else { return None };
    let p = prop.borrow();
    let K::FullMultiname { resolved: Some(r), .. } = &p.k else { return None };
    Some((r.clone(), args.clone()))
}

fn resolved_of(it: &It) -> Option<String> {
    match &it.borrow().k {
        K::FullMultiname { resolved, .. } => resolved.clone(),
        _ => None,
    }
}

/// The getter `obj.name` with a resolved name, as (object, name).
fn prop_get(it: &It) -> Option<(It, Option<String>)> {
    match &it.borrow().k {
        K::GetProperty { obj, prop, .. } => Some((obj.clone(), resolved_of(prop))),
        _ => None,
    }
}

/// AS3 source of a script pack (`ScriptPack.toSource`).
pub fn pack_source(ctx: &Ctx, pack: &Pack) -> String {
    let mut sp = Sp { ctx, cd: ConvertData::default(), convs: HashMap::new(), class_init_empty: HashMap::new(), script_init_empty: true, fla_ignored: HashMap::new() };
    sp.convert_pack(pack);
    let mut w = Writer::new();
    sp.append_pack(pack, &mut w);
    w.finish()
}

fn global_item() -> It {
    bare(K::Global, prec::PRIMARY)
}

impl<'c, 'a> Sp<'c, 'a> {
    fn abc(&self) -> &'a Abc {
        self.ctx.abc
    }

    fn names(&self) -> super::names::Names<'_> {
        self.ctx.names()
    }

    fn mn(&self, i: usize) -> Option<&'a Multiname> {
        self.abc().constants.multinames.get(i).filter(|_| i != 0)
    }

    fn own_ns(&self, mn: usize) -> usize {
        match self.mn(mn) {
            Some(Multiname::QName { ns, .. }) => *ns,
            _ => 0,
        }
    }

    fn is_interface(&self, class: i32) -> bool {
        class >= 0 && self.abc().instances.get(class as usize).is_some_and(|i| i.flags & instance_flags::INTERFACE != 0)
    }

    fn has_body(&self, method: usize) -> bool {
        self.abc().body(method).is_some()
    }

    fn req(&self, method: usize, is_static: bool, script: usize, class: i32, fqn: &[Dotted], scope: Vec<It>) -> MethodReq<'a> {
        let mut r = MethodReq::new(method);
        r.is_static = is_static;
        r.script_index = script as i32;
        r.class_index = class;
        r.fqn = fqn.to_vec();
        r.scope = scope;
        r
    }

    fn convert_body(&mut self, req: &MethodReq, w: &mut Writer) {
        let r = method::convert_into(self.ctx, req, &mut self.cd, &HashSet::new(), w);
        self.convs.insert(req.method, r);
    }

    fn print_body(&mut self, req: &MethodReq, class_index: i32, w: &mut Writer) {
        if !self.convs.contains_key(&req.method) {
            let mut nw = Writer::nul();
            self.convert_body(req, &mut nw);
        }
        match self.convs.get(&req.method).unwrap() {
            Ok(conv) => {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| method::to_source(self.ctx, req, conv, &HashSet::new(), class_index, w)));
                if r.is_err() {
                    method::error_comment(w, "printing failed");
                }
            }
            Err(e) => {
                let e = e.clone();
                method::error_comment(w, &e);
            }
        }
    }

    // Conversion pass.

    fn convert_pack(&mut self, pack: &Pack) {
        let abc = self.abc();
        let script = &abc.scripts[pack.script];
        if self.has_body(script.init) && (pack.is_simple || pack.traits.is_empty()) {
            let classes: Vec<usize> = script.traits.iter().filter_map(|t| if let TraitData::Class { class, .. } = t.data { Some(class) } else { None }).collect();
            let mut req = self.req(script.init, true, pack.script, -1, &[], Vec::new());
            req.init_traits = Some(&script.traits);
            req.init_trait_classes = classes;
            req.initializer = TRAIT_SCRIPT_INITIALIZER;
            req.path = ".scriptinitializer".into();
            let mut w = Writer::nul();
            w.mark();
            self.convert_body(&req, &mut w);
            self.script_init_empty = !w.get_mark();
        }
        let scope = vec![global_item()];
        for &t in &pack.traits {
            self.convert_trait(&script.traits[t], false, pack.script, -1, &scope);
        }
    }

    fn convert_trait(&mut self, t: &'a Trait, is_static: bool, script: usize, class: i32, scope: &[It]) {
        match t.data {
            TraitData::Class { class: ci, .. } => self.convert_class(ci, script, scope),
            TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } => {
                if !(self.is_interface(class) || !self.has_body(method)) {
                    let req = self.req(method, is_static, script, class, &[], scope.to_vec());
                    self.convert_body(&req, &mut Writer::nul());
                }
            }
            TraitData::Function { method, .. } => {
                if self.has_body(method) {
                    let req = self.req(method, is_static, script, class, &[], scope.to_vec());
                    self.convert_body(&req, &mut Writer::nul());
                }
            }
            _ => {}
        }
    }

    fn convert_class(&mut self, ci: usize, script: usize, scope: &[It]) {
        let abc = self.abc();
        let inst = &abc.instances[ci];
        let class = &abc.classes[ci];
        let cname = self.names().name_with_ns(inst.name, true);
        self.cd.this_has_default_to_primitive = match &self.ctx.index {
            Some(idx) => !idx.local_instance_property(&cname, "toString"),
            None => true,
        };
        let mut new_scope = scope.to_vec();
        if self.has_body(class.cinit) {
            if inst.flags & instance_flags::INTERFACE == 0 {
                if let Some(idx) = &self.ctx.index {
                    let pkg = index::script_package(self.ctx, script as i32);
                    for a in idx.ancestors(&cname, pkg).into_iter().rev() {
                        new_scope.push(bare(K::Class { mn: None, name: Some(a) }, prec::PRIMARY));
                    }
                }
            }
            let mut req = self.req(class.cinit, true, script, ci as i32, &[], new_scope.clone());
            req.init_traits = Some(&class.traits);
            req.initializer = TRAIT_CLASS_INITIALIZER;
            let mut w = Writer::nul();
            w.mark();
            self.convert_body(&req, &mut w);
            new_scope.push(bare(K::Class { mn: Some(inst.name), name: None }, prec::PRIMARY));
            self.class_init_empty.insert(ci, !w.get_mark());
        }
        if inst.flags & instance_flags::INTERFACE == 0 && self.has_body(inst.iinit) {
            let mut req = self.req(inst.iinit, false, script, ci as i32, &[], Vec::new());
            req.init_traits = Some(&inst.traits);
            req.initializer = TRAIT_INSTANCE_INITIALIZER;
            self.convert_body(&req, &mut Writer::nul());
            if self.ctx.fla {
                self.strip_fla_constructor(ci, inst.iinit);
            }
        }
        for t in &class.traits {
            self.convert_trait(t, true, script, ci as i32, &new_scope);
        }
        for t in &inst.traits {
            self.convert_trait(t, false, script, ci as i32, &new_scope);
        }
    }

    /// `ignoreFrameScripts` and `ignoreAccessibility`: drops `addFrameScript`, the generated
    /// accessibility calls and assignments from the converted constructor, and records the
    /// traits they name.
    fn strip_fla_constructor(&mut self, ci: usize, iinit: usize) {
        let Some(Ok(conv)) = self.convs.get(&iinit) else { return };
        let names = self.ctx.names();
        let abc = self.abc();
        let mut ignored: Vec<String> = Vec::new();
        conv.items.borrow_mut().retain(|it| {
            if let Some((name, args)) = call_prop(it) {
                if name == "addFrameScript" {
                    let args = args.borrow();
                    let mut i = 0;
                    while i + 1 < args.len() {
                        if matches!(args[i].borrow().k, K::IntegerValueA { .. }) {
                            match &args[i + 1].borrow().k {
                                K::GetLex { mn, .. } => ignored.push(names.name(*mn, &[], false, true)),
                                K::GetProperty { obj, prop, .. } if matches!(obj.borrow().k, K::This { .. }) => {
                                    if let K::FullMultiname { mn, .. } = &prop.borrow().k {
                                        ignored.push(names.name(*mn, &[], false, true));
                                    }
                                }
                                _ => {}
                            }
                        }
                        i += 2;
                    }
                    return false;
                }
                if (name.starts_with("__setAcc_") || name.starts_with("__setTab_")) && args.borrow().is_empty() {
                    return false;
                }
                if name == "addEventListener" {
                    let args = args.borrow();
                    if args.len() == 5 {
                        let first = prop_get(&args[0]).and_then(|p| p.1);
                        let second = prop_get(&args[1]).and_then(|p| p.1);
                        if first.as_deref() == Some("FRAME_CONSTRUCTED") && matches!(second.as_deref(), Some("__setAcc_handler" | "__setTab_handler")) {
                            return false;
                        }
                    }
                }
            }
            let b = it.borrow();
            if let K::SetProperty { obj, .. } = &b.k {
                if let Some(v) = &b.value {
                    if let K::ConstructProp { prop, .. } = &v.borrow().k {
                        if resolved_of(prop).as_deref() == Some("AccessibilityProperties") {
                            return false;
                        }
                    }
                }
                if let Some((o, Some(p))) = prop_get(obj) {
                    if p == "accessibilityProperties" {
                        if let Some((o2, Some(r))) = prop_get(&o) {
                            if r == "root" && matches!(o2.borrow().k, K::This { .. }) {
                                return false;
                            }
                        }
                    }
                }
            }
            true
        });
        for t in &abc.instances[ci].traits {
            let n = names.name(t.name, &[], true, false);
            if n.starts_with("__setAcc_") || n.starts_with("__setTab_") {
                ignored.push(names.name(t.name, &[], false, false));
            }
        }
        for v in ["__setAccDict", "__setTabDict", "__lastFrameAcc", "__lastFrameTab"] {
            ignored.push(v.to_string());
        }
        self.fla_ignored.insert(ci, ignored);
    }

    // Printing pass.

    fn append_pack(&mut self, pack: &Pack, w: &mut Writer) {
        let abc = self.abc();
        let script = &abc.scripts[pack.script];
        let names = self.ctx.names();
        let mut first = true;
        if !pack.is_simple && pack.traits.is_empty() {
            for t in &script.traits {
                if matches!(t.data, TraitData::Slot { .. } | TraitData::Const { .. }) {
                    continue;
                }
                let ns = self.own_ns(t.name);
                if !is_package_kind(names.ns_kind(ns)) || ns == 0 {
                    continue;
                }
                let full = names.name_with_ns(t.name, false).printable(&self.ctx.ids);
                w.append_no_hilight(&format!("include \"{}.as\";", full.replace('.', "/"))).new_line();
            }
            w.new_line();
        }
        let mut list: Vec<usize> = pack.traits.clone();
        let mut fqn: Vec<Dotted> = Vec::new();
        let mut i = 0;
        while i < list.len() {
            let t = &script.traits[list[i]];
            if !is_package_kind(names.simple_ns_kind(t.name)) {
                i += 1;
                continue;
            }
            if !first {
                w.new_line();
            }
            self.to_string_packaged(t, false, pack.script, -1, w, &mut fqn);
            first = false;
            list.remove(i);
        }
        if !first {
            w.new_line();
        }
        if !list.is_empty() {
            let needs_init = list.iter().any(|&t| matches!(script.traits[t].data, TraitData::Slot { .. } | TraitData::Const { .. }));
            let ts: Vec<&Trait> = list.iter().map(|&t| &script.traits[t]).collect();
            self.write_imports(&ts, if needs_init { script.init as i64 } else { -1 }, pack.script, -1, true, None, w, &mut fqn);
        }
        first = true;
        let mut i = 0;
        while i < list.len() {
            let t = &script.traits[list[i]];
            if matches!(t.data, TraitData::Slot { .. } | TraitData::Const { .. }) {
                i += 1;
                continue;
            }
            if !first {
                w.new_line();
            }
            self.trait_to_string(t, false, pack.script, -1, w, &mut Vec::new(), false);
            first = false;
            list.remove(i);
        }
        for &ti in &list {
            let t = &script.traits[ti];
            if !matches!(t.data, TraitData::Slot { .. } | TraitData::Const { .. }) || self.cd.get(t).is_some() {
                continue;
            }
            if !first {
                w.new_line();
            }
            if is_package_kind(names.simple_ns_kind(t.name)) {
                self.to_string_packaged(t, false, pack.script, -1, w, &mut Vec::new());
            } else {
                self.trait_to_string(t, false, pack.script, -1, w, &mut Vec::new(), false);
            }
            first = false;
        }
        if self.has_body(script.init) && (pack.is_simple || pack.traits.is_empty()) {
            if !self.script_init_empty {
                if !first {
                    w.new_line();
                }
                let mut req = self.req(script.init, true, pack.script, -1, &fqn, Vec::new());
                req.initializer = TRAIT_SCRIPT_INITIALIZER;
                req.init_traits = Some(&script.traits);
                self.print_body(&req, -1, w);
                w.new_line();
            }
        }
    }

    fn to_string_packaged(&mut self, t: &'a Trait, is_static: bool, script: usize, class: i32, w: &mut Writer, fqn: &mut Vec<Dotted>) {
        let names = self.ctx.names();
        if !is_package_kind(names.simple_ns_kind(t.name)) {
            return;
        }
        let pkg = names.simple_ns_name(t.name).unwrap_or_default();
        let nsname = pkg.printable(&self.ctx.ids);
        w.append_no_hilight("package");
        if !nsname.is_empty() {
            w.append_no_hilight(&format!(" {nsname}"));
        }
        w.start_block();
        self.write_imports(&[t], -1, script, class, is_static, Some(pkg), w, fqn);
        self.trait_to_string(t, is_static, script, class, w, fqn, false);
        w.end_block();
        w.new_line();
    }

    #[allow(clippy::too_many_arguments)]
    fn trait_to_string(&mut self, t: &'a Trait, is_static: bool, script: usize, class: i32, w: &mut Writer, fqn: &mut Vec<Dotted>, inside_interface: bool) {
        match t.data {
            TraitData::Class { class: ci, .. } => self.class_to_string(t, ci, script, w, fqn),
            TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } => self.method_to_string(t, method, is_static, script, class, w, fqn, inside_interface),
            TraitData::Function { method, .. } => self.function_to_string(t, method, is_static, script, class, w, fqn, inside_interface),
            TraitData::Slot { .. } | TraitData::Const { .. } => {
                let cd = self.cd.clone();
                slot_to_string(self.ctx, &cd, t, is_static, class, w, fqn, inside_interface);
            }
        }
    }

    fn class_header(&self, ci: usize, w: &mut Writer, fqn: &[Dotted]) {
        let abc = self.abc();
        let inst = &abc.instances[ci];
        let names = self.ctx.names();
        let mut modifiers = ns_prefix(names.ns_kind(self.own_ns(inst.name)));
        if modifiers == "private" {
            modifiers = String::new();
        }
        if !modifiers.is_empty() {
            modifiers.push(' ');
        }
        if inst.flags & instance_flags::FINAL != 0 {
            modifiers.push_str("final ");
        }
        let iface = inst.flags & instance_flags::INTERFACE != 0;
        if !iface && inst.flags & instance_flags::SEALED == 0 {
            modifiers.push_str("dynamic ");
        }
        w.append_no_hilight(&format!("{modifiers}{}", if iface { "interface " } else { "class " }));
        w.special(&names.name(inst.name, &[], false, true));
        if inst.flags & instance_flags::NON_NULLABLE != 0 {
            w.append_no_hilight("!");
        }
        if inst.super_name > 0 {
            let parent = names.name(inst.super_name, fqn, false, true);
            if parent != "Object" {
                w.append_no_hilight(" extends ");
                w.special(&parent);
            }
        }
        if !inst.interfaces.is_empty() {
            w.append_no_hilight(if iface { " extends " } else { " implements " });
            for (i, &x) in inst.interfaces.iter().enumerate() {
                if i > 0 {
                    w.append(", ");
                }
                w.special(&names.name(x, fqn, false, true));
            }
        }
    }

    fn class_to_string(&mut self, t: &'a Trait, ci: usize, script: usize, w: &mut Writer, fqn: &mut Vec<Dotted>) {
        let abc = self.abc();
        let inst = &abc.instances[ci];
        let class = &abc.classes[ci];
        let iface = inst.flags & instance_flags::INTERFACE != 0;
        if let Some(d) = &self.ctx.doc {
            if d.name == self.names().name_with_ns(inst.name, false).raw() {
                let rate = if (d.frame_rate - d.frame_rate.round()).abs() < 0.001 { format!("{}", d.frame_rate.round() as i64) } else { format!("{}", d.frame_rate) };
                let bg = d.background.as_ref().map(|b| format!(", backgroundColor=\"{b}\"")).unwrap_or_default();
                w.append_no_hilight(&format!("[SWF(width=\"{}\", height=\"{}\"{bg}, frameRate=\"{rate}\")]", d.width, d.height)).new_line();
            }
        }
        metadata(self.ctx, t, w);
        if let Some(lines) = self.ctx.embeds.get(&self.names().name_with_ns(inst.name, false).raw()) {
            for l in lines {
                w.append_no_hilight(l).new_line();
            }
        }
        self.class_header(ci, w, fqn);
        w.start_block();
        let mut first = true;
        let has_cinit = self.has_body(class.cinit);
        w.append("");
        w.new_line();
        let c = ci as i32;
        self.traits_to_string(&class.traits, &mut first, &[Kind::Slot], true, script, c, w, fqn, &[], iface);
        if has_cinit && !self.class_init_empty.get(&ci).copied().unwrap_or(true) {
            if !first {
                w.new_line();
            }
            first = false;
            let mut req = self.req(class.cinit, true, script, c, fqn, Vec::new());
            req.init_traits = Some(&class.traits);
            req.initializer = TRAIT_CLASS_INITIALIZER;
            self.print_body(&req, c, w);
        }
        let ignored = self.fla_ignored.get(&ci).cloned().unwrap_or_default();
        self.traits_to_string(&inst.traits, &mut first, &[Kind::Slot], false, script, c, w, fqn, &ignored, iface);
        if !iface {
            if !first {
                w.new_line();
            }
            first = false;
            w.append_no_hilight("public ");
            w.append_no_hilight("function ");
            w.append_no_hilight(&self.names().name(inst.name, &[], false, true));
            w.append_no_hilight("(");
            method::param_str(self.ctx, inst.iinit, fqn, w);
            w.append_no_hilight(")");
            w.start_block();
            if self.has_body(inst.iinit) {
                let mut req = self.req(inst.iinit, false, script, c, fqn, Vec::new());
                req.init_traits = Some(&inst.traits);
                req.initializer = TRAIT_INSTANCE_INITIALIZER;
                self.print_body(&req, c, w);
            }
            w.end_block().new_line();
        }
        self.traits_to_string(&class.traits, &mut first, &[Kind::Class, Kind::Function, Kind::Method], true, script, c, w, fqn, &[], iface);
        self.traits_to_string(&inst.traits, &mut first, &[Kind::Class, Kind::Function, Kind::Method], false, script, c, w, fqn, &ignored, iface);
        if first {
            w.new_line();
        }
        w.end_block();
        w.new_line();
    }

    #[allow(clippy::too_many_arguments)]
    fn traits_to_string(&mut self, traits: &'a [Trait], first: &mut bool, kinds: &[Kind], is_static: bool, script: usize, class: i32, w: &mut Writer, fqn: &mut Vec<Dotted>, ignored: &[String], inside_interface: bool) {
        for t in traits {
            if !kinds.contains(&Kind::of(t)) {
                continue;
            }
            if ignored.contains(&self.names().name(t.name, &[], false, false)) {
                continue;
            }
            if Kind::of(t) == Kind::Slot && is_static && self.cd.get(t).is_some() {
                continue;
            }
            if !*first {
                w.new_line();
            }
            *first = false;
            self.trait_to_string(t, is_static, script, class, w, fqn, inside_interface);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn method_to_string(&mut self, t: &'a Trait, mi: usize, is_static: bool, script: usize, class: i32, w: &mut Writer, fqn: &mut Vec<Dotted>, inside_interface: bool) {
        let abc = self.abc();
        metadata(self.ctx, t, w);
        let add = match t.data {
            TraitData::Getter { .. } => "get ",
            TraitData::Setter { .. } => "set ",
            _ => "",
        };
        modifiers(self.ctx, t, is_static, inside_interface, class, w);
        if abc.methods[mi].flags & method_flags::EXPLICIT != 0 {
            w.append_no_hilight("native ");
        }
        w.special(&format!("function {add}"));
        w.special(&self.names().name(t.name, &[], false, true));
        w.append_no_hilight("(");
        method::param_str(self.ctx, mi, fqn, w);
        w.append_no_hilight(") : ");
        w.special(&method::return_type_str(self.ctx, mi, fqn));
        if self.is_interface(class) || !self.has_body(mi) {
            w.append_no_hilight(";");
        } else {
            w.start_block();
            let req = self.req(mi, is_static, script, class, fqn, Vec::new());
            self.print_body(&req, class, w);
            w.end_block();
        }
        w.new_line();
    }

    #[allow(clippy::too_many_arguments)]
    fn function_to_string(&mut self, t: &'a Trait, mi: usize, is_static: bool, script: usize, class: i32, w: &mut Writer, fqn: &mut Vec<Dotted>, inside_interface: bool) {
        metadata(self.ctx, t, w);
        if !self.has_body(mi) {
            w.append_no_hilight("native ");
        }
        modifiers(self.ctx, t, is_static, inside_interface, class, w);
        w.special("function ");
        w.special(&self.names().name(t.name, fqn, false, true));
        w.append_no_hilight("(");
        method::param_str(self.ctx, mi, fqn, w);
        w.append_no_hilight(") : ");
        w.special(&method::return_type_str(self.ctx, mi, fqn));
        w.start_block();
        if self.has_body(mi) {
            let req = self.req(mi, is_static, script, class, fqn, Vec::new());
            self.print_body(&req, class, w);
        }
        w.end_block();
        w.new_line();
    }

    // Imports.

    fn find_custom_ns_uri(&self, uri: &str) -> Option<Dotted> {
        self.ctx.index.as_ref()?.ns_value_to_name(uri)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_ns(&self, custom: &Option<String>, ns_index: usize, ignore: &Option<Dotted>, name: &str, dtype: u8, deps: &mut Vec<Dep>, uses: &mut Vec<String>) {
        let names = self.names();
        let kind = names.ns_kind(ns_index);
        let raw = names.ns_raw_name(ns_index);
        let name = if name.is_empty() { "*" } else { name };
        if kind == ns_kind::NAMESPACE || kind == ns_kind::PACKAGE_INTERNAL {
            if let Some(nsimport) = self.find_custom_ns_uri(&raw) {
                if nsimport == Dotted::new(&["-"]) {
                    return;
                }
                let last = nsimport.last().to_string();
                if !uses.contains(&last) {
                    uses.push(last);
                }
                if !nsimport.is_empty() {
                    let dep = Dep { id: nsimport.clone(), ty: DEP_NAMESPACE };
                    if (ignore.is_none() || nsimport.without_last() != *ignore.as_ref().unwrap()) && !deps.contains(&dep) {
                        deps.push(dep);
                    }
                    let _ = custom;
                    return;
                }
            }
        }
        if dtype == DEP_NAMESPACE || kind != ns_kind::PACKAGE {
            return;
        }
        let id = Dotted::parse_no_suffix(&raw).add_with_suffix(name);
        let dep = Dep { id: id.clone(), ty: dtype };
        if !deps.contains(&dep) {
            if id.without_last() == Dotted::new(&["__AS3__", "vec"]) {
                return;
            }
            deps.push(dep);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_mn(&self, custom: &Option<String>, mn: usize, ignore: &Option<Dotted>, fqn: &[Dotted], dtype: u8, deps: &mut Vec<Dep>, uses: &mut Vec<String>) {
        let Some(m) = self.mn(mn) else { return };
        if let Multiname::TypeName { name, params } = m {
            if *name != 0 {
                self.parse_mn(custom, *name, ignore, fqn, dtype, deps, uses);
            }
            for &p in params {
                if p != 0 {
                    self.parse_mn(custom, p, ignore, fqn, dtype, deps, uses);
                }
            }
            return;
        }
        let names = self.names();
        let name = names.name(mn, fqn, true, true);
        if let Multiname::QName { ns, .. } = m {
            if *ns != 0 && !self.ctx.display_ns.contains(&mn) {
                self.parse_ns(custom, *ns, ignore, &name, dtype, deps, uses);
            }
        }
        if let Some(set) = names.ns_set(mn) {
            let single = set.len() == 1;
            for &n in set {
                self.parse_ns(custom, n, ignore, if single { &name } else { "" }, dtype, deps, uses);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn trait_deps(&self, t: &Trait, script: usize, class: i32, is_static: bool, custom: &Option<String>, deps: &mut Vec<Dep>, ignore: &Option<Dotted>, uses: &mut Vec<String>) {
        let names = self.names();
        let pkg = names.simple_ns_name(t.name);
        let base = |me: &Self, custom: &Option<String>, ignore: &Option<Dotted>, deps: &mut Vec<Dep>, uses: &mut Vec<String>| {
            let mut c = custom.clone();
            if c.is_none() && names.simple_ns_kind(t.name) == ns_kind::NAMESPACE {
                c = names.simple_ns_name(t.name).map(|d| d.raw());
            }
            me.parse_mn(&c, t.name, ignore, &[], DEP_NAMESPACE, deps, uses);
        };
        let own_custom = || {
            let mut c = custom.clone();
            if c.is_none() && names.simple_ns_kind(t.name) == ns_kind::NAMESPACE {
                c = names.simple_ns_name(t.name).map(|d| d.raw());
            }
            c
        };
        match t.data {
            TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } => {
                let ig = if ignore.is_none() { pkg.clone() } else { ignore.clone() };
                base(self, custom, &ig, deps, uses);
                self.method_deps(method, script, class, is_static, &own_custom(), &ig, &mut Vec::new(), deps, uses);
            }
            TraitData::Function { method, .. } => {
                let ig = if ignore.is_none() { pkg.clone() } else { ignore.clone() };
                base(self, custom, &ig, deps, uses);
                self.method_deps(method, script, class, false, custom, &ig, &mut Vec::new(), deps, uses);
            }
            TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } => {
                let ig = if ignore.is_none() { pkg.clone() } else { ignore.clone() };
                base(self, custom, &ig, deps, uses);
                self.parse_mn(custom, type_name, &pkg, &[], DEP_SIGNATURE, deps, uses);
            }
            TraitData::Class { class: ci, .. } => {
                let ig = if ignore.is_none() { pkg.clone() } else { ignore.clone() };
                base(self, custom, &ig, deps, uses);
                let abc = self.abc();
                let inst = &abc.instances[ci];
                let cls = &abc.classes[ci];
                let ins_ns = self.own_ns(inst.name);
                let package = Some(if ins_ns == 0 { Dotted::default() } else { names.ns_name(ins_ns) });
                if inst.super_name > 0 {
                    self.parse_mn(custom, inst.super_name, &package, &[], DEP_INHERITANCE, deps, uses);
                }
                for &i in &inst.interfaces {
                    self.parse_mn(custom, i, &package, &[], DEP_INHERITANCE, deps, uses);
                }
                for st in &cls.traits {
                    self.trait_deps(st, script, ci as i32, true, custom, deps, &package, uses);
                }
                self.method_deps(cls.cinit, script, ci as i32, true, custom, &package, &mut Vec::new(), deps, uses);
                for it in &inst.traits {
                    self.trait_deps(it, script, ci as i32, false, custom, deps, &package, uses);
                }
                self.method_deps(inst.iinit, script, ci as i32, false, custom, &package, &mut Vec::new(), deps, uses);
            }
        }
    }

    fn prepared(&self, method: usize, script: usize, class: i32, is_static: bool) -> Option<Code> {
        let body = self.abc().body(method)?;
        Some(method::prepare_code(self.ctx, body, self.ctx.cfg.deobfuscate, method, is_static, script as i32, class))
    }

    #[allow(clippy::too_many_arguments)]
    fn method_deps(&self, method: usize, script: usize, class: i32, is_static: bool, custom: &Option<String>, ignore: &Option<Dotted>, visited: &mut Vec<usize>, deps: &mut Vec<Dep>, uses: &mut Vec<String>) {
        let abc = self.abc();
        let Some(mi) = abc.methods.get(method) else { return };
        visited.push(method);
        if mi.return_type != 0 {
            self.parse_mn(custom, mi.return_type, ignore, &[], DEP_SIGNATURE, deps, uses);
        }
        for &t in &mi.param_types {
            if t != 0 {
                self.parse_mn(custom, t, ignore, &[], DEP_SIGNATURE, deps, uses);
            }
        }
        let Some(body) = abc.body(method) else { return };
        let Some(code) = self.prepared(method, script, class, is_static) else { return };
        for t in &body.traits {
            self.trait_deps(t, script, class, is_static, custom, deps, ignore, uses);
        }
        for e in &code.exceptions {
            self.parse_mn(custom, e.type_name, ignore, &[], DEP_EXPRESSION, deps, uses);
        }
        let ins = &code.ins;
        let last_new_class = ins.iter().rposition(|i| i.code == op::NEWCLASS).map_or(-1, |p| p as i64);
        let has_new_class = last_new_class >= 0;
        let mut was_new_class = false;
        let mut prev: Option<usize> = None;
        for (i, x) in ins.iter().enumerate() {
            if class == -1 && has_new_class && !was_new_class {
                if x.code == op::NEWCLASS {
                    was_new_class = true;
                }
                continue;
            }
            if x.code == op::GETLEX && (i as i64) < last_new_class {
                continue;
            }
            if matches!(x.name, "li8" | "li16" | "li32" | "lf32" | "lf64" | "si8" | "si16" | "si32" | "sf32" | "sf64" | "sxi1" | "sxi8" | "sxi16") {
                push_dep(deps, Dep { id: Dotted::new(&["avm2", "intrinsics", "memory", x.name]), ty: DEP_EXPRESSION });
            }
            if x.code == op::NEWFUNCTION {
                let m = x.op(0) as usize;
                if m != method && !visited.contains(&m) {
                    self.method_deps(m, script, class, is_static, custom, ignore, visited, deps, uses);
                }
            }
            let ml = |mn: usize| matches!(self.mn(mn), Some(Multiname::MultinameL { attr: false, .. }));
            if x.code == op::CONSTRUCTPROP && ml(x.op(0) as usize) {
                deps.push(Dep { id: Dotted::new(&["flash", "utils", "getDefinitionByName"]), ty: DEP_NAMESPACE });
            }
            if x.code == op::CONSTRUCT {
                if let Some(p) = prev.map(|p| &ins[p]) {
                    if p.code == op::GETPROPERTY && ml(p.op(0) as usize) {
                        deps.push(Dep { id: Dotted::new(&["flash", "utils", "getDefinitionByName"]), ty: DEP_NAMESPACE });
                    }
                }
            }
            if x.code == op::CALLPROPERTY && class > -1 && followed_by_set_local(ins, i) {
                if let Some(p) = prev {
                    self.call_property_return_deps(script, class, &ins[p], x, custom, ignore, deps, uses);
                }
            }
            if class > -1 && x.name == "getouterscope" && x.op(0) > 0 {
                if let Some(idx) = &self.ctx.index {
                    let cname = self.names().name_with_ns(abc.instances[class as usize].name, true);
                    let mut chain = idx.ancestors(&cname, index::script_package(self.ctx, script as i32));
                    chain.reverse();
                    if (x.op(0) as usize) < 1 + chain.len() {
                        push_dep(deps, Dep { id: chain[x.op(0) as usize - 1].clone(), ty: DEP_EXPRESSION });
                    }
                }
            }
            for (k, kind) in x.kinds.iter().enumerate() {
                if matches!(kind, Operand::MultinameIndex) {
                    let m = x.op(k) as usize;
                    if m < abc.constants.multinames.len() {
                        self.parse_mn(custom, m, ignore, &[], DEP_EXPRESSION, deps, uses);
                    }
                }
            }
            prev = Some(i);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call_property_return_deps(&self, script: usize, class: i32, prev: &super::code::Ins, call: &super::code::Ins, custom: &Option<String>, ignore: &Option<Dotted>, deps: &mut Vec<Dep>, uses: &mut Vec<String>) {
        let _ = (script, custom, uses);
        if prev.code != op::GETLEX && prev.code != op::GETPROPERTY {
            return;
        }
        let rmn = prev.op(0) as usize;
        if self.mn(rmn).is_none() {
            return;
        }
        let names = self.names();
        let rname = names.name(rmn, &[], true, true);
        if rname.is_empty() {
            return;
        }
        let cur = ty_dotted(names.name_with_ns(self.abc().instances[class as usize].name, true));
        let mut rt = index::find_property_type(self.ctx, &cur, &rname, self.own_ns(rmn), true, true, true).map(|p| p.ty).unwrap_or_else(unbounded);
        if is_unbounded(&rt) || is_unknown(&rt) {
            rt = index::mn_type(self.ctx, rmn);
        }
        if is_unbounded(&rt) || is_unknown(&rt) {
            return;
        }
        let mut lookup = rt.clone();
        if let K::ApplyType { obj, .. } = &rt.borrow().k {
            lookup = obj.clone();
        }
        if !matches!(lookup.borrow().k, K::Type { .. }) || is_unknown(&lookup) {
            return;
        }
        let cmn = call.op(0) as usize;
        if self.mn(cmn).is_none() {
            return;
        }
        let pname = names.name(cmn, &[], true, true);
        if pname.is_empty() {
            return;
        }
        let ct = index::find_property_type(self.ctx, &lookup, &pname, 0, true, true, true).map(|p| p.call_ty).unwrap_or_else(unbounded);
        type_deps(&ct, ignore, deps);
    }

    #[allow(clippy::too_many_arguments)]
    fn write_imports(&self, traits: &[&Trait], method: i64, script: usize, class: i32, is_static: bool, ignore: Option<Dotted>, w: &mut Writer, fqn: &mut Vec<Dotted>) -> bool {
        let abc = self.abc();
        let names = self.names();
        let idx = self.ctx.index.clone();
        let names_in_package: Vec<String> = match (&idx, &ignore) {
            (Some(i), Some(p)) => i.names_in_package(p),
            _ => Vec::new(),
        };
        let mut script_names: Vec<String> = Vec::new();
        for st in &abc.scripts[script].traits {
            if let TraitData::Class { class: ci, .. } = st.data {
                let inst = &abc.instances[ci];
                for t in inst.traits.iter().chain(abc.classes[ci].traits.iter()) {
                    script_names.push(names.name(t.name, &[], true, true));
                }
                if inst.super_name != 0 {
                    if let Some(i) = &idx {
                        script_names.extend(i.inherited_trait_names(&names.name_with_ns(inst.super_name, true), index::script_package(self.ctx, script as i32)));
                    }
                }
            } else {
                script_names.push(names.name(st.name, &[], true, true));
            }
        }
        let mut seen = HashSet::new();
        script_names.retain(|n| seen.insert(n.clone()));
        let mut deps: Vec<Dep> = Vec::new();
        let mut uses: Vec<String> = Vec::new();
        let mut custom: Option<String> = None;
        for t in traits {
            if names.simple_ns_kind(t.name) == ns_kind::NAMESPACE {
                custom = names.simple_ns_name(t.name).map(|d| d.raw());
            }
            self.trait_deps(t, script, class, is_static, &custom, &mut deps, &ignore, &mut uses);
        }
        let from_traits: Vec<Dotted> = deps.iter().map(|d| d.id.clone()).collect();
        if method != -1 {
            let m = method as usize;
            self.method_deps(m, script, class, is_static, &custom, &ignore, &mut Vec::new(), &mut deps, &mut uses);
            let mut nf: Vec<Dep> = Vec::new();
            if let Some(code) = self.prepared(m, script, class, is_static) {
                let mut visited = Vec::new();
                for x in &code.ins {
                    if x.code == op::NEWFUNCTION {
                        let f = x.op(0) as usize;
                        if f != m && !visited.contains(&f) {
                            self.method_deps(f, script, class, is_static, &custom, &ignore, &mut visited, &mut nf, &mut uses);
                        }
                    }
                }
            }
            let from_nf: Vec<Dotted> = nf.iter().map(|d| d.id.clone()).collect();
            let defined: Vec<Dotted> = abc.scripts[script].traits.iter().filter(|st| is_package_kind(names.simple_ns_kind(st.name))).map(|st| names.name_with_ns(st.name, true)).collect();
            deps.retain(|d| !(defined.contains(&d.id) && !from_traits.contains(&d.id) && !from_nf.contains(&d.id)));
        }
        let mut imports: Vec<Dotted> = Vec::new();
        for d in &deps {
            if !imports.contains(&d.id) && !BUILT_IN_CLASSES.contains(&d.id.raw().as_str()) {
                imports.push(d.id.clone());
            }
        }
        let packages: Vec<Dotted> = abc.scripts[script].traits.iter().filter(|st| is_package_kind(names.simple_ns_kind(st.name))).map(|st| names.simple_ns_name(st.name).unwrap_or_default()).collect();
        imports.retain(|i| !(i.last() == "*" && packages.contains(&i.without_last())));
        let mut imported: Vec<String> = BUILT_IN_CLASSES.iter().map(|s| s.to_string()).collect();
        imported.extend(names_in_package);
        imported.extend(script_names);
        for imp in &imports {
            if imp.last() == "*" {
                if Some(imp.without_last()) == ignore {
                    continue;
                }
                if let Some(i) = &idx {
                    imported.extend(i.package_objects(&imp.without_last()));
                }
            }
        }
        let all = imports.clone();
        imports.retain(|i| i.last() == "*" || !all.contains(&i.without_last().add("*", "")));
        if let Some(ig) = &ignore {
            if !ig.is_empty() {
                imports.retain(|i| i.without_last() != *ig);
            }
        }
        for imp in &imports {
            imported.push(imp.last().to_string());
        }
        let mut unique: Vec<String> = Vec::new();
        for n in imported {
            if unique.contains(&n) {
                fqn.push(Dotted::parse_with_suffix(&n));
            } else {
                unique.push(n);
            }
        }
        imports.sort_by_key(|a| a.raw());
        let mut has_import = false;
        for imp in &imports {
            if imp.len() > 1 {
                w.append_no_hilight("import ");
                w.append_no_hilight(&imp.without_last().printable(&self.ctx.ids));
                w.append_no_hilight(".");
                if imp.last() == "*" {
                    w.append_no_hilight("*");
                } else {
                    w.special(&self.ctx.ids.print(imp.last()));
                }
                w.append_no_hilight(";").new_line();
                has_import = true;
            }
        }
        let has_use = !uses.is_empty();
        if has_use {
            if has_import {
                w.new_line();
            }
            for u in &uses {
                w.append_no_hilight(&format!("use namespace {u};")).new_line();
            }
        }
        if has_import || has_use {
            w.new_line();
        }
        has_import || has_use
    }
}

fn type_deps(t: &It, ignore: &Option<Dotted>, deps: &mut Vec<Dep>) {
    if is_unbounded(t) || is_unknown(t) {
        return;
    }
    let k = t.borrow().k.clone();
    match k {
        K::ApplyType { obj, params } => {
            type_deps(&obj, ignore, deps);
            for p in params.borrow().iter() {
                type_deps(p, ignore, deps);
            }
        }
        K::Type { full, .. } => {
            if full.is_empty() || (full.last() == "*" && full.len() <= 1) || full.without_last() == Dotted::new(&["__AS3__", "vec"]) {
                return;
            }
            let dep = Dep { id: full.clone(), ty: DEP_EXPRESSION };
            if (ignore.is_none() || full.without_last() != *ignore.as_ref().unwrap()) && !deps.contains(&dep) {
                deps.push(dep);
            }
        }
        _ => {}
    }
}

fn followed_by_set_local(code: &[super::code::Ins], at: usize) -> bool {
    for x in &code[at + 1..] {
        if matches!(x.code, op::DEBUGLINE | op::DEBUGFILE | op::DEBUG) {
            continue;
        }
        return x.set_local_reg().is_some();
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Slot,
    Class,
    Function,
    Method,
}

impl Kind {
    fn of(t: &Trait) -> Kind {
        match t.data {
            TraitData::Slot { .. } | TraitData::Const { .. } => Kind::Slot,
            TraitData::Class { .. } => Kind::Class,
            TraitData::Function { .. } => Kind::Function,
            _ => Kind::Method,
        }
    }
}

fn ns_prefix(kind: u8) -> String {
    match kind {
        ns_kind::NAMESPACE => "",
        ns_kind::PRIVATE => "private",
        ns_kind::PACKAGE => "public",
        ns_kind::PACKAGE_INTERNAL => "internal",
        ns_kind::PROTECTED => "protected",
        ns_kind::EXPLICIT => "explicit",
        ns_kind::STATIC_PROTECTED => "protected",
        _ => "?",
    }
    .into()
}

/// `findCustomNsOfMultiname`, keeping Java's null.
fn custom_ns_of(ctx: &Ctx, mn: usize) -> Option<Dotted> {
    let names = ctx.names();
    let kind = names.simple_ns_kind(mn);
    if kind != ns_kind::NAMESPACE && kind != ns_kind::PACKAGE_INTERNAL {
        return None;
    }
    let name = names.simple_ns_name(mn).unwrap_or_default().raw();
    if name == "http://adobe.com/AS3/2006/builtin" {
        return Some(Dotted::new(&["AS3"]));
    }
    ctx.custom_ns.get(&name).cloned()
}

fn is_namespace_value(t: &Trait) -> bool {
    match t.data {
        TraitData::Slot { value: Some(v), .. } | TraitData::Const { value: Some(v), .. } => matches!(v.kind, 0x08 | 0x16 | 0x17 | 0x18 | 0x19 | 0x1a | 0x05),
        _ => false,
    }
}

/// `Trait.getModifiers`.
fn modifiers(ctx: &Ctx, t: &Trait, is_static: bool, inside_interface: bool, class: i32, w: &mut Writer) {
    let abc = ctx.abc;
    let names = ctx.names();
    if t.attrs & trait_attr::FINAL != 0 && !is_static {
        w.append_no_hilight("final ");
    }
    if t.attrs & trait_attr::OVERRIDE != 0 {
        w.append_no_hilight("override ");
    }
    if t.name != 0 {
        let dc = custom_ns_of(ctx, t.name);
        let nsname = dc.as_ref().map(|d| d.last().to_string());
        let mut kind = names.simple_ns_kind(t.name);
        if inside_interface {
        } else if kind == ns_kind::NAMESPACE && nsname.is_none() {
            w.append("\u{a7}\u{a7}namespace(\"");
            w.append(&crate::abc::names::escape_as(&names.simple_ns_name(t.name).unwrap_or_default().raw()));
            w.append("\") ");
        } else if let (Some(_), Some(n)) = (&dc, &nsname) {
            let id = ctx.ids.print(n);
            if !id.is_empty() {
                w.special(&id).append_no_hilight(" ");
            }
        } else if kind != 0 {
            if kind == ns_kind::PRIVATE {
                let this_ns = match abc.constants.multinames.get(t.name) {
                    Some(Multiname::QName { ns, .. }) => *ns,
                    _ => 0,
                };
                if class == -1 {
                    kind = ns_kind::PACKAGE_INTERNAL;
                } else {
                    let cn = abc.instances[class as usize].name;
                    let class_ns = match abc.constants.multinames.get(cn) {
                        Some(Multiname::QName { ns, .. }) => *ns,
                        _ => 0,
                    };
                    if class_ns == this_ns {
                        kind = ns_kind::PACKAGE_INTERNAL;
                    }
                }
            }
            if !(class == -1 && kind == ns_kind::PACKAGE_INTERNAL) {
                let p = ns_prefix(kind);
                if !p.is_empty() {
                    w.append_no_hilight(&p).append_no_hilight(" ");
                }
            }
        }
    }
    if is_static && class > -1 && !is_namespace_value(t) {
        w.append_no_hilight("static ");
    }
}

/// `Trait.getMetaData`.
fn metadata(ctx: &Ctx, t: &Trait, w: &mut Writer) {
    let abc = ctx.abc;
    for &m in &t.metadata {
        let Some(md) = abc.metadata.get(m) else { continue };
        let name = abc.str(md.name);
        if name == "__go_to_definition_help" || name == "__go_to_ctor_definition_help" {
            continue;
        }
        w.append("[").append(&ctx.ids.print(name));
        if !md.keys.is_empty() {
            w.append("(");
            let mut seen: Vec<String> = Vec::new();
            let mut pairs: Vec<(String, String)> = Vec::new();
            for (k, v) in md.keys.iter().zip(md.values.iter()) {
                let key = abc.str(*k).to_string();
                let val = abc.str(*v).to_string();
                match seen.iter().position(|x| *x == key) {
                    Some(p) => pairs[p].1 = val,
                    None => {
                        seen.push(key.clone());
                        pairs.push((key, val));
                    }
                }
            }
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    w.append(",");
                }
                if !k.is_empty() {
                    w.append(&ctx.ids.print(k)).append("=");
                }
                w.append("\"").append(&crate::abc::names::escape_as(v)).append("\"");
            }
            w.append(")");
        }
        w.append("]");
        w.new_line();
    }
}

fn value_str_len(t: &Trait) -> Option<crate::abc::ValueKind> {
    match t.data {
        TraitData::Slot { value, .. } | TraitData::Const { value, .. } => value.filter(|v| v.kind != 0),
        _ => None,
    }
}

/// `TraitSlotConst.toString`.
#[allow(clippy::too_many_arguments)]
fn slot_to_string(ctx: &Ctx, cd: &ConvertData, t: &Trait, is_static: bool, class: i32, w: &mut Writer, fqn: &[Dotted], inside_interface: bool) {
    let abc = ctx.abc;
    let names = ctx.names();
    metadata(ctx, t, w);
    let mut show = true;
    if class == -1 && t.name != 0 {
        match abc.constants.multinames.get(t.name) {
            Some(Multiname::QName { ns, .. }) if *ns != 0 => {
                if !is_package_kind(names.ns_kind(*ns)) {
                    show = false;
                }
            }
            _ => show = false,
        }
    }
    if show {
        modifiers(ctx, t, is_static, inside_interface, class, w);
    }
    let assigned = cd.get(t).cloned();
    let value_ld = |method: usize| {
        let mut ld = LocalData::new(ctx);
        ld.fqn = fqn.to_vec();
        ld.class_index = class;
        ld.call_stack = vec![method];
        ld
    };
    if let Some(a) = &assigned {
        if is::new_function(&a.value) {
            super::print::ts(&a.value, w, &value_ld(a.method));
            return;
        }
    }
    let (type_name, is_const) = match t.data {
        TraitData::Slot { type_name, .. } => (type_name, false),
        TraitData::Const { type_name, .. } => (type_name, true),
        _ => (0, false),
    };
    let mut type_str = if type_name > 0 { names.name(type_name, fqn, false, true) } else { "*".into() };
    type_str = format!(":{type_str}");
    let mut kw = if is_const { "const" } else { "var" };
    if value_str_len(t).is_some() && type_name == 0 && is_namespace_value(t) {
        kw = "namespace";
        type_str = String::new();
    }
    w.special(&format!("{kw} "));
    w.special(&names.name(t.name, &[], false, true));
    w.special(&type_str);
    let has_value = assigned.is_some()
        || match value_str_len(t) {
            Some(v) if v.kind == 0x08 => abc.constants.namespaces.get(v.index).is_none_or(|n| n.kind != ns_kind::PACKAGE_INTERNAL),
            Some(_) => true,
            None => false,
        };
    if has_value {
        w.append_no_hilight(" = ");
        match &assigned {
            Some(a) => super::print::ts(&a.value, w, &value_ld(a.method)),
            None => {
                if let Some(v) = value_str_len(t) {
                    w.special(&method::value_kind_str(ctx, &v));
                }
            }
        }
    }
    w.append_no_hilight(";").new_line();
}

/// `TraitSlotConstAVM2Item.appendTo`: a slot assigned in an initializer, printed as its declaration.
pub fn print_trait_slot_const(it: &It, w: &mut Writer, ld: &LocalData) {
    let (script, class, ti, is_static, initializer, method, value) = match &it.borrow().k {
        K::TraitSlotConst { script, class, trait_index, is_static, initializer, method, value } => (*script, *class, *trait_index, *is_static, *initializer, *method, value.clone()),
        _ => return,
    };
    let abc = ld.ctx.abc;
    let traits: Option<&Vec<Trait>> = if class < 0 {
        abc.scripts.get(script.max(0) as usize).map(|s| &s.traits)
    } else if is_static {
        abc.classes.get(class.max(0) as usize).map(|c| &c.traits)
    } else {
        abc.instances.get(class.max(0) as usize).map(|c| &c.traits)
    };
    let Some(t) = traits.and_then(|ts| ts.get(ti.max(0) as usize)) else { return };
    let Some(value) = value else { return };
    let mut cd = ConvertData::default();
    cd.assigned.push((t as *const Trait, method::Assigned { command: None, value, initializer, method }));
    let inside_interface = class > -1 && abc.instances.get(class as usize).is_some_and(|i| i.flags & instance_flags::INTERFACE != 0);
    slot_to_string(ld.ctx, &cd, t, is_static, class, w, &ld.fqn, inside_interface);
}
