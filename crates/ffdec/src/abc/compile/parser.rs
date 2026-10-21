//! FFDec's `ActionScript3Parser`: AS3 source to the item tree, resolving names it can as it goes.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::ast::*;
use super::gen::Cx;
use super::lexer::{Group, Lexer, RecEvent, State, Sym, T, V};
use super::{CompileError, R};
use crate::abc::decomp::names::Dotted;
use crate::abc::ns_kind;

const AS3_NAMESPACE: &str = "http://adobe.com/AS3/2006/builtin";

pub type Vars = Rc<RefCell<Nodes>>;

fn vars() -> Vars {
    Rc::new(RefCell::new(Vec::new()))
}

/// What FFDec threads through every parse method.
#[derive(Clone)]
pub struct Env {
    pub all_ns: AllNs,
    pub this_type: Option<Node>,
    pub pkg: Option<Ns>,
    pub needs_act: Rc<Cell<bool>>,
    pub imported: Imports,
    pub opened: NsList,
    pub variables: Vars,
}

impl Env {
    fn with_vars(&self, v: &Vars) -> Env {
        Env { variables: v.clone(), ..self.clone() }
    }

    fn with_act(&self, a: &Rc<Cell<bool>>) -> Env {
        Env { needs_act: a.clone(), ..self.clone() }
    }
}

#[derive(Default)]
struct Loops {
    stack: Vec<i64>,
    labels: HashMap<i64, String>,
}

#[derive(Clone, Copy)]
enum E {
    T(T),
    G(Group),
}

fn upper_snake(t: T) -> String {
    let s = format!("{t:?}");
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

fn e_name(e: E) -> String {
    match e {
        E::T(t) => upper_snake(t),
        E::G(g) => format!("{g:?}").to_uppercase(),
    }
}

fn int_of(s: &Sym) -> i32 {
    match s.v {
        V::Int(i) => i,
        V::Double(d) => d as i32,
        _ => s.text().parse().unwrap_or(0),
    }
}

pub struct Parser<'a> {
    pub lx: Lexer,
    uniq: i64,
    pub cx: &'a mut Cx,
    /// Printed identifiers to restore: FFDec's `@identifier` replacements.
    pub replacements: HashMap<String, String>,
}

type Meta = Metadata;

impl<'a> Parser<'a> {
    pub fn new(cx: &'a mut Cx, src: &str) -> Parser<'a> {
        Parser { lx: Lexer::new(src), uniq: 0, cx, replacements: HashMap::new() }
    }

    fn uniq_id(&mut self) -> i64 {
        self.uniq += 1;
        self.uniq
    }

    fn line(&self) -> usize {
        self.lx.yyline()
    }

    fn err<X>(&self, m: impl Into<String>) -> R<X> {
        Err(CompileError { line: self.line(), message: m.into() })
    }

    fn lex(&mut self) -> R<Sym> {
        let mut s = self.lx.lex()?;
        if s.t == T::Identifier {
            if let V::Str(v) = &s.v {
                if let Some(r) = self.replacements.get(v) {
                    s.v = V::Str(r.clone());
                }
            }
        }
        Ok(s)
    }

    fn pushback(&mut self, s: Sym) {
        self.lx.pushback(s);
    }

    fn expected(&self, s: &Sym, exp: &[E]) -> R<()> {
        let found = exp.iter().any(|e| match e {
            E::T(t) => s.t == *t,
            E::G(g) => s.group == *g,
        });
        if found {
            return Ok(());
        }
        let list: Vec<String> = exp.iter().map(|e| e_name(*e)).collect();
        self.err(format!("{} expected but {} found", list.join(" or "), upper_snake(s.t)))
    }

    fn expect(&mut self, exp: &[E]) -> R<Sym> {
        let s = self.lex()?;
        self.expected(&s, exp)?;
        Ok(s)
    }

    fn expect_t(&mut self, t: T) -> R<Sym> {
        self.expect(&[E::T(t)])
    }

    fn is_ident(s: &Sym) -> bool {
        s.group == Group::Identifier
    }

    fn commands(&mut self, env: &Env, loops: &mut Loops) -> R<Nodes> {
        let mut ret = Vec::new();
        while let Some(c) = self.command(env, loops, true)? {
            ret.push(c);
        }
        Ok(ret)
    }

    fn type_(&mut self, env: &Env) -> R<Node> {
        let s = self.lex()?;
        if s.t == T::Multiply {
            return Ok(unbounded());
        } else if s.t == T::Void {
            return Ok(ty_parts(&["void"]));
        }
        self.pushback(s);
        let t = self.name(env, true)?;
        self.apply_type(env, t)
    }

    fn member_or_call(&mut self, env: &Env, newcmds: Node) -> R<Node> {
        let mut s = self.lex()?;
        let mut ret = newcmds;
        while s.is(&[T::Dot, T::NullDot, T::ParentOpen, T::BracketOpen, T::TypeName, T::Filter, T::Descendants]) {
            match s.t {
                T::BracketOpen | T::Dot | T::NullDot | T::TypeName => {
                    self.pushback(s);
                    ret = self.member(env, ret)?;
                }
                T::Filter => {
                    env.needs_act.set(true);
                    let e = self.expression(env, false, true)?.unwrap_or_else(unbounded);
                    ret = node(Item::XmlFilter { obj: ret, value: e, opened: env.opened.clone() });
                    self.expect_t(T::ParentClose)?;
                }
                T::ParentOpen => {
                    let line = self.line();
                    let args = self.call(env)?;
                    ret = node(Item::Call(Call { name: ret, args, line, opened: env.opened.clone(), this_type: env.this_type.clone() }));
                }
                _ => {
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier), E::T(T::Multiply)])?;
                    let name = if s.t == T::Multiply { None } else { Some(s.text()) };
                    ret = node(Item::Descendants { obj: ret, name, opened: env.opened.clone() });
                }
            }
            s = self.lex()?;
        }
        if s.t == T::Increment || s.t == T::Decrement {
            if !is_name_or_prop(&ret) {
                return self.err("Invalid assignment");
            }
            let ty = ty_parts(&["Number"]);
            ret = node(if s.t == T::Increment { Item::PostInc { v: ret, ty } } else { Item::PostDec { v: ret, ty } });
            s = self.lex()?;
        }
        self.pushback(s);
        Ok(ret)
    }

    fn apply_type(&mut self, env: &Env, obj: Node) -> R<Node> {
        let mut s = self.lex()?;
        if s.t != T::TypeName {
            self.pushback(s);
            return Ok(obj);
        }
        let mut params = Vec::new();
        loop {
            s = self.lex()?;
            if s.t == T::Multiply {
                params.push(node(Item::Null));
            } else {
                self.pushback(s);
                params.push(self.expression_primary(env, false)?.ok_or_else(|| self.error("Missing type"))?);
            }
            s = self.lex()?;
            if s.t != T::Comma {
                break;
            }
        }
        let gt = |p: usize| Sym { pos: p, group: Group::Operator, t: T::GreaterThan, v: V::Str(">".into()) };
        if s.t == T::UShiftRight {
            s = gt(s.pos);
            self.pushback(s.clone());
            self.pushback(s.clone());
        }
        if s.t == T::ShiftRight {
            s = gt(s.pos);
            self.pushback(s.clone());
        }
        self.expected(&s, &[E::T(T::GreaterThan)])?;
        Ok(node(Item::ApplyType { obj, params }))
    }

    fn error(&self, m: &str) -> CompileError {
        CompileError { line: self.line(), message: m.into() }
    }

    fn member(&mut self, env: &Env, obj: Node) -> R<Node> {
        let mut ret = obj;
        let mut s = self.lex()?;
        while s.is(&[T::Dot, T::NullDot, T::BracketOpen, T::TypeName]) {
            let s2 = self.lex()?;
            let mut attr = false;
            let mut null_dot = false;
            if s.t == T::NullDot {
                null_dot = true;
                self.pushback(s2);
            } else if s.t == T::Dot {
                if s2.t == T::Attribute {
                    attr = true;
                } else {
                    self.pushback(s2);
                }
            } else {
                self.pushback(s2);
            }
            if s.t == T::TypeName {
                self.pushback(s);
                ret = self.apply_type(env, ret)?;
                s = self.lex()?;
            } else if s.t == T::BracketOpen {
                let index = self.expression(env, false, false)?.ok_or_else(|| self.error("Missing index"))?;
                self.expect_t(T::BracketClose)?;
                ret = node(Item::Index(Index { attr, object: ret, index, opened: env.opened.clone(), assigned: None }));
                s = self.lex()?;
            } else {
                s = self.lex()?;
                self.expected(&s, &[E::G(Group::Identifier), E::T(T::Multiply), E::T(T::Public), E::T(T::Protected), E::T(T::Private), E::T(T::Internal)])?;
                let mut prop_name = Some(s.text());
                let ns_keyword = if s.is(&[T::Public, T::Protected, T::Private, T::Internal]) { Some(s.text()) } else { None };
                let mut prop_item = None;
                s = self.lex()?;
                let mut suffix = String::new();
                let mut ns = None;
                if s.t == T::NamespaceOp {
                    if ns_keyword.is_none() {
                        let n = unresolved(Vec::new(), env.imported.clone(), false, None, self.line(), Dotted::one(prop_name.as_deref().unwrap_or(""), ""), None, env.opened.clone(), env.this_type.clone());
                        env.variables.borrow_mut().push(n.clone());
                        ns = Some(n);
                    }
                    s = self.lex()?;
                    if s.t == T::BracketOpen {
                        prop_item = self.expression(env, false, false)?;
                        self.expect_t(T::BracketClose)?;
                        prop_name = None;
                    } else {
                        self.expected(&s, &[E::G(Group::Identifier)])?;
                        prop_name = Some(s.text());
                        prop_item = None;
                    }
                } else {
                    if let Some(k) = &ns_keyword {
                        return self.err(format!("{k} not expected in this situation"));
                    }
                    if s.t == T::NamespaceSuffix {
                        suffix = format!("#{}", int_of(&s));
                    } else {
                        self.pushback(s);
                    }
                }
                ret = match ns {
                    Some(ns) => node(Item::Namespaced(Namespaced { ns, name: prop_name, name_item: prop_item, obj: Some(ret), attr, opened: env.opened.clone(), assigned: None })),
                    None => property(Some(ret), attr, prop_name.as_deref().unwrap_or(""), &suffix, env.opened.clone(), empty_stack(), null_dot, ns_keyword, self.line(), env.this_type.clone()),
                };
                s = self.lex()?;
            }
        }
        self.pushback(s);
        Ok(ret)
    }

    fn name(&mut self, env: &Env, type_only: bool) -> R<Node> {
        let mut s = self.lex()?;
        let mut name = Dotted::default();
        let mut attribute = false;
        if s.t == T::Attribute {
            attribute = true;
            s = self.lex()?;
        }
        self.expected(&s, &[E::G(Group::Identifier), E::T(T::This), E::T(T::Super), E::T(T::Public), E::T(T::Protected), E::T(T::Private), E::T(T::Internal)])?;
        let mut ns_keyword = if s.is(&[T::Public, T::Protected, T::Private, T::Internal]) { Some(s.text()) } else { None };
        let mut name2 = s.text();
        s = self.lex()?;
        let mut suffix = String::new();
        if s.t == T::NamespaceSuffix {
            if let Some(k) = &ns_keyword {
                return self.err(format!("{k} not expected in this situation"));
            }
            suffix = format!("#{}", int_of(&s));
            s = self.lex()?;
        }
        name.0.push(crate::abc::decomp::names::Part { name: name2.clone(), attr: attribute, suffix: suffix.clone() });
        while s.t == T::Dot {
            if let Some(k) = &ns_keyword {
                return self.err(format!("{k} not expected in this situation"));
            }
            s = self.lex()?;
            name2 = String::new();
            attribute = false;
            if s.t == T::Attribute {
                attribute = true;
                s = self.lex()?;
                if s.t == T::Multiply || s.group == Group::Identifier {
                    name2.push_str(&s.text());
                } else {
                    if s.t != T::BracketOpen {
                        return self.err("Attribute identifier or bracket expected");
                    }
                    continue;
                }
            } else {
                self.expected(&s, &[E::G(Group::Identifier), E::T(T::Namespace), E::T(T::Multiply), E::T(T::Public), E::T(T::Protected), E::T(T::Private), E::T(T::Internal)])?;
                name2.push_str(&s.text());
                if s.is(&[T::Public, T::Protected, T::Private, T::Internal]) {
                    ns_keyword = Some(s.text());
                }
            }
            s = self.lex()?;
            suffix = String::new();
            if s.t == T::NamespaceSuffix {
                if let Some(k) = &ns_keyword {
                    return self.err(format!("{k} not expected in this situation"));
                }
                suffix = format!("#{}", int_of(&s));
                s = self.lex()?;
            }
            name.0.push(crate::abc::decomp::names::Part { name: name2.clone(), attr: attribute, suffix: suffix.clone() });
        }
        let mut ns_name = None;
        let mut ns_prop = None;
        let mut ns_attribute = false;
        let mut ns_prop_item = None;
        if s.t == T::NamespaceOp {
            ns_name = Some(name.last().to_string());
            ns_attribute = name.0.last().is_some_and(|p| p.attr);
            s = self.lex()?;
            if s.group == Group::Identifier {
                ns_prop = Some(s.text());
            } else if s.t == T::BracketOpen {
                ns_prop_item = self.expression(env, false, false)?;
                self.expect_t(T::BracketClose)?;
            }
            name = name.without_last();
            s = self.lex()?;
        } else if let Some(k) = &ns_keyword {
            return self.err(format!("{k} not expected in this situation"));
        }
        let mut ret = None;
        if !name.is_empty() {
            let u = unresolved(Vec::new(), env.imported.clone(), type_only, None, self.line(), name, None, env.opened.clone(), env.this_type.clone());
            env.variables.borrow_mut().push(u.clone());
            ret = Some(u);
        }
        if let Some(nsname) = ns_name {
            if ns_keyword.is_some() && ns_prop_item.is_none() {
                ret = Some(property(ret, ns_attribute, ns_prop.as_deref().unwrap_or(""), &suffix, env.opened.clone(), empty_stack(), false, ns_keyword, self.line(), env.this_type.clone()));
            } else {
                let ns = unresolved(Vec::new(), env.imported.clone(), type_only, None, self.line(), Dotted::new(&[&nsname]), None, env.opened.clone(), env.this_type.clone());
                env.variables.borrow_mut().push(ns.clone());
                ret = Some(node(Item::Namespaced(Namespaced { ns, name: ns_prop, name_item: ns_prop_item, obj: ret, attr: ns_attribute, opened: env.opened.clone(), assigned: None })));
            }
        }
        let ret = ret.ok_or_else(|| self.error("Name expected"))?;
        if s.is(&[T::BracketOpen, T::Dot, T::NullDot, T::TypeName]) {
            self.pushback(s);
            return self.member(env, ret);
        }
        self.pushback(s);
        Ok(ret)
    }

    fn call(&mut self, env: &Env) -> R<Nodes> {
        let mut ret = Vec::new();
        let mut s = self.lex()?;
        while s.t != T::ParentClose {
            if s.t != T::Comma {
                self.pushback(s);
            }
            if let Some(e) = self.expression(env, false, false)? {
                ret.push(e);
            }
            s = self.lex()?;
            self.expected(&s, &[E::T(T::Comma), E::T(T::ParentClose)])?;
        }
        Ok(ret)
    }

    #[allow(clippy::too_many_arguments)]
    fn method(&mut self, env: &Env, outside_package: bool, is_private: bool, metadata: Meta, pkg: Option<Ns>, is_interface: bool, is_native: bool, custom_ns: Option<String>, is_override: bool, is_final: bool, is_static: bool, name: &str, kind: MethodKind) -> R<Node> {
        let f = self.function(env, metadata, pkg, is_interface, is_native, name, true)?;
        Ok(node(Item::Method(Method { f, kind, is_static, is_final, is_override, is_private, outside_package, custom_ns, all_opened: env.all_ns.clone() })))
    }

    #[allow(clippy::too_many_arguments)]
    fn function(&mut self, env: &Env, metadata: Meta, pkg: Option<Ns>, is_interface: bool, is_native: bool, name: &str, _is_method: bool) -> R<Func> {
        let opened = ns_list_copy(&env.opened);
        env.all_ns.borrow_mut().push(opened.clone());
        let env = Env { opened, ..env.clone() };
        let line = self.line();
        self.expect_t(T::ParentOpen)?;
        let mut s = self.lex()?;
        let mut param_names = Vec::new();
        let mut param_types = Vec::new();
        let mut param_values = Vec::new();
        let mut has_rest = false;
        while s.t != T::ParentClose {
            if s.t != T::Comma {
                self.pushback(s);
            }
            s = self.lex()?;
            if s.t == T::Rest {
                has_rest = true;
                s = self.lex()?;
            }
            self.expected(&s, &[E::G(Group::Identifier)])?;
            param_names.push(s.text());
            s = self.lex()?;
            if !has_rest {
                if s.t == T::Colon {
                    param_types.push(self.type_(&env)?);
                    s = self.lex()?;
                } else {
                    param_types.push(unbounded());
                }
                if s.t == T::Assign {
                    let v = self.expression(&env.with_act(&Rc::new(Cell::new(false))), false, false)?.ok_or_else(|| self.error("Missing default value"))?;
                    param_values.push(v);
                    s = self.lex()?;
                } else if !param_values.is_empty() {
                    return self.err("Some of parameters do not have default values");
                }
            }
            if !s.is(&[T::Comma, T::ParentClose]) {
                self.expected(&s, &[E::T(T::Comma), E::T(T::ParentClose)])?;
            }
            if has_rest {
                self.expected(&s, &[E::T(T::ParentClose)])?;
            }
        }
        s = self.lex()?;
        let ret_type = if s.t == T::Colon {
            self.type_(&env)?
        } else {
            self.pushback(s);
            unbounded()
        };
        let sub = vars();
        let l = self.line();
        let mk = |t: Option<Node>, n: &str| name_node(t, l, n, None, true, env.opened.clone(), false);
        sub.borrow_mut().push(mk(env.this_type.clone(), "this"));
        for i in 0..param_names.len() - usize::from(has_rest) {
            sub.borrow_mut().push(mk(Some(param_types[i].clone()), &param_names[i]));
        }
        if has_rest {
            sub.borrow_mut().push(mk(Some(unbounded()), param_names.last().unwrap()));
        }
        sub.borrow_mut().push(mk(Some(ty_parts(&["Array"])), "arguments"));
        let par_cnt = sub.borrow().len();
        let act2 = Rc::new(Cell::new(false));
        let body = if !is_interface && !is_native {
            self.expect_t(T::CurlyOpen)?;
            let benv = Env { needs_act: act2.clone(), variables: sub.clone(), ..env.clone() };
            let b = self.commands(&benv, &mut Loops::default())?;
            self.expect_t(T::CurlyClose)?;
            Some(b)
        } else {
            self.expect_t(T::Semicolon)?;
            None
        };
        let subvariables: Nodes = sub.borrow()[par_cnt..].to_vec();
        Ok(Func {
            metadata,
            pkg,
            is_interface,
            is_native,
            needs_activation: act2.get(),
            has_rest,
            line,
            name: name.to_string(),
            param_types,
            param_names,
            param_values,
            body,
            subvariables,
            ret_type,
        })
    }

    fn parse_metadata(&mut self) -> R<Meta> {
        let mut metadata = Vec::new();
        let mut s = self.lex()?;
        while s.t == T::BracketOpen {
            s = self.lex()?;
            self.expected(&s, &[E::G(Group::Identifier)])?;
            let name = s.text();
            let mut values: Vec<(String, String)> = Vec::new();
            s = self.lex()?;
            if s.t == T::ParentOpen {
                s = self.lex()?;
                if s.group == Group::String {
                    put(&mut values, String::new(), s.text());
                    s = self.lex()?;
                } else {
                    self.pushback(s);
                    loop {
                        s = self.lex()?;
                        self.expected(&s, &[E::G(Group::Identifier)])?;
                        let key = s.text();
                        self.expect_t(T::Assign)?;
                        s = self.lex()?;
                        self.expected(&s, &[E::G(Group::String)])?;
                        put(&mut values, key, s.text());
                        s = self.lex()?;
                        if s.t != T::Comma {
                            break;
                        }
                    }
                }
                self.expected(&s, &[E::T(T::ParentClose)])?;
                s = self.lex()?;
            }
            self.expected(&s, &[E::T(T::BracketClose)])?;
            s = self.lex()?;
            if name != "Embed" {
                metadata.push((name, values));
            }
        }
        self.pushback(s);
        Ok(metadata)
    }

    fn check_duplicate_field(&mut self, fields: &mut Vec<(Option<(Option<Dotted>, i32)>, HashSet<String>)>, ns: &Ns, name: &str, line: usize) -> R<()> {
        let (mut nname, mut kind) = {
            let n = ns.borrow();
            (n.name.clone(), n.kind)
        };
        if ns.borrow().index > -1 {
            let (d, k) = self.cx.ns_name_kind(ns.borrow().index as usize);
            nname = Some(d);
            kind = k as i32;
        }
        let key = if [ns_kind::PACKAGE, ns_kind::PACKAGE_INTERNAL, ns_kind::PRIVATE, ns_kind::PROTECTED, ns_kind::STATIC_PROTECTED].iter().any(|&k| k as i32 == kind) { None } else { Some((nname, kind)) };
        let pos = match fields.iter().position(|(k, _)| *k == key) {
            Some(p) => p,
            None => {
                fields.push((key, HashSet::new()));
                fields.len() - 1
            }
        };
        if !fields[pos].1.insert(name.to_string()) {
            return Err(CompileError { line, message: format!("Duplicate class field \"{name}\"") });
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn class_traits(
        &mut self,
        env: &Env,
        outside_package: bool,
        cinit_variables: &Vars,
        cinit_act: &Rc<Cell<bool>>,
        pkg: &Ns,
        class_name: &str,
        is_interface: bool,
        traits: &mut Nodes,
        iinit_variables: &Vars,
        iinit_act: &Rc<Cell<bool>>,
        iinit: &mut Option<Node>,
    ) -> R<()> {
        let pkg_raw = pkg.borrow().raw_name();
        let qualified = if pkg_raw.is_empty() { class_name.to_string() } else { format!("{pkg_raw}:{class_name}") };
        let public_ns = ns_item_str("", ns_kind::PACKAGE as i32, 0);
        let private_ns = ns_item_str(&qualified, ns_kind::PRIVATE as i32, 0);
        let protected_ns = ns_item_str(&qualified, ns_kind::PROTECTED as i32, 0);
        let static_protected_ns = ns_item_str(&qualified, ns_kind::STATIC_PROTECTED as i32, 0);
        let pkg_name = pkg.borrow().name.clone().unwrap_or_default();
        let package_internal_ns = ns_item(pkg_name.clone(), ns_kind::PACKAGE_INTERNAL as i32, 0);
        let interface_ns = ns_item_str(&qualified, ns_kind::NAMESPACE as i32, 0);
        let opened = ns_list_copy(&env.opened);
        env.all_ns.borrow_mut().push(opened.clone());
        for ln in env.all_ns.borrow().iter() {
            if !ns_list_contains(ln, &public_ns) {
                ln.borrow_mut().push(public_ns.clone());
            }
        }
        opened.borrow_mut().push(private_ns.clone());
        opened.borrow_mut().push(protected_ns.clone());
        opened.borrow_mut().push(static_protected_ns.clone());
        let env = Env { opened, ..env.clone() };
        let mut cinit_loops = Loops::default();
        let mut instance_fields = Vec::new();
        let mut static_fields = Vec::new();
        loop {
            let this_type = ty(pkg_name.add_with_suffix(class_name));
            let mut is_getter = false;
            let mut is_setter = false;
            let mut is_override = false;
            let mut is_static = false;
            let mut is_final = false;
            let mut is_private = false;
            let mut is_native = false;
            let mut custom_ns: Option<String> = None;
            let mut pre: Vec<Sym> = Vec::new();
            let mut namespace: Option<Ns> = None;
            let metadata = self.parse_metadata()?;
            let mut s = self.lex()?;
            'mods: while s.is(&[T::Native, T::Static, T::Public, T::Private, T::Protected, T::Override, T::Final, T::Dynamic, T::Internal, T::Preprocessor]) || s.group == Group::Identifier {
                if s.t == T::Final {
                    if is_final {
                        return self.err("Only one final keyword allowed");
                    }
                    pre.push(s.clone());
                    is_final = true;
                } else if s.t == T::Override {
                    if is_override {
                        return self.err("Only one override keyword allowed");
                    }
                    pre.push(s.clone());
                    is_override = true;
                } else if s.t == T::Static {
                    if is_interface {
                        return self.err("Interface cannot have static traits");
                    }
                    if is_static {
                        return self.err("Only one static keyword allowed");
                    }
                    pre.push(s.clone());
                    is_static = true;
                } else if s.t == T::Namespace {
                    pre.push(s.clone());
                    break;
                } else if s.t == T::Native {
                    if is_native {
                        return self.err("Only one native keyword allowed");
                    }
                    pre.push(s.clone());
                    is_native = true;
                } else if s.group == Group::Identifier {
                    custom_ns = Some(s.text());
                    if is_interface {
                        return self.err("Namespace attributes are not permitted on interface methods");
                    }
                    pre.push(s.clone());
                } else if namespace.is_some() {
                    return self.err("Only one access identifier allowed");
                }
                let access = "Interface members cannot be declared public, private, protected, or internal";
                match s.t {
                    T::Public => {
                        namespace = Some(public_ns.clone());
                        if is_interface {
                            return self.err(access);
                        }
                        pre.push(s.clone());
                    }
                    T::Private => {
                        is_private = true;
                        namespace = Some(private_ns.clone());
                        if is_interface {
                            return self.err(access);
                        }
                        pre.push(s.clone());
                    }
                    T::Protected => {
                        namespace = Some(protected_ns.clone());
                        if is_interface {
                            return self.err(access);
                        }
                        pre.push(s.clone());
                    }
                    T::Internal => {
                        namespace = Some(package_internal_ns.clone());
                        if is_interface {
                            return self.err(access);
                        }
                        pre.push(s.clone());
                    }
                    T::Preprocessor => {
                        if s.text().to_lowercase() == "namespace" {
                            pre.push(s.clone());
                            s = self.lex()?;
                            self.expected(&s, &[E::T(T::ParentOpen)])?;
                            pre.push(s.clone());
                            s = self.lex()?;
                            self.expected(&s, &[E::T(T::String)])?;
                            pre.push(s.clone());
                            namespace = Some(ns_item_str(&s.text(), ns_kind::NAMESPACE as i32, self.line()));
                            s = self.lex()?;
                            self.expected(&s, &[E::T(T::ParentClose)])?;
                            pre.push(s.clone());
                        } else {
                            self.pushback(s.clone());
                            break 'mods;
                        }
                    }
                    _ => {}
                }
                s = self.lex()?;
            }
            if is_interface {
                namespace = Some(interface_ns.clone());
            }
            if namespace.is_none() && custom_ns.is_none() {
                namespace = Some(package_internal_ns.clone());
            }
            if namespace.as_ref().is_some_and(|n| Rc::ptr_eq(n, &protected_ns)) && is_static {
                namespace = Some(static_protected_ns.clone());
            }
            if namespace.is_none() {
                if let Some(c) = &custom_ns {
                    namespace = Some(ns_item_str(c, KIND_NAMESPACE_CUSTOM, self.line()));
                }
            }
            let mut namespace = namespace.unwrap();
            match s.t {
                T::Function => {
                    s = self.lex()?;
                    if s.t == T::Get {
                        is_getter = true;
                        s = self.lex()?;
                    } else if s.t == T::Set {
                        is_setter = true;
                        s = self.lex()?;
                    }
                    self.expected(&s, &[E::G(Group::Identifier), E::T(T::ParentOpen)])?;
                    let fname;
                    if s.t == T::ParentOpen {
                        self.pushback(s);
                        if is_getter {
                            fname = "get".to_string();
                            is_getter = false;
                        } else if is_setter {
                            fname = "set".to_string();
                            is_setter = false;
                        } else {
                            return self.err("Missing method name");
                        }
                    } else {
                        fname = s.text();
                    }
                    if fname == class_name {
                        if is_static {
                            return self.err("Constructor cannot be static");
                        }
                        if is_override {
                            return self.err("Override flag not allowed for constructor");
                        }
                        if is_final {
                            return self.err("Final flag not allowed for constructor");
                        }
                        if is_interface {
                            return self.err("Interface cannot have constructor");
                        }
                        let ienv = Env { this_type: Some(this_type.clone()), pkg: Some(pkg.clone()), needs_act: iinit_act.clone(), variables: iinit_variables.clone(), ..env.clone() };
                        *iinit = Some(self.method(&ienv, outside_package, is_private, metadata, Some(pkg.clone()), false, false, custom_ns, false, false, false, "", MethodKind::Method)?);
                    } else {
                        let s2 = self.lex()?;
                        if s2.t == T::NamespaceSuffix {
                            namespace = ns_index_item(int_of(&s2) as i64);
                        } else {
                            self.pushback(s2);
                        }
                        let menv = Env { this_type: Some(this_type.clone()), needs_act: Rc::new(Cell::new(false)), variables: vars(), ..env.clone() };
                        let kind = if is_getter {
                            MethodKind::Getter
                        } else if is_setter {
                            MethodKind::Setter
                        } else {
                            MethodKind::Method
                        };
                        let ft = self.method(&menv, outside_package, is_private, metadata, Some(namespace.clone()), is_interface, is_native, custom_ns, is_override, is_final, is_static, &fname, kind)?;
                        if let Item::Method(m) = &*ft.borrow() {
                            if is_getter && !m.f.param_types.is_empty() {
                                return self.err("Getter can't have any parameters");
                            }
                            if is_setter && m.f.param_types.len() != 1 {
                                return self.err("Getter must have exactly one parameter");
                            }
                        }
                        traits.push(ft);
                    }
                }
                T::Namespace => {
                    if is_interface {
                        return self.err("Interface cannot have namespace fields");
                    }
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let nname = s.text();
                    s = self.lex()?;
                    let mut generated = false;
                    let nval = if s.t == T::Assign {
                        s = self.lex()?;
                        self.expected(&s, &[E::T(T::String)])?;
                        let v = s.text();
                        s = self.lex()?;
                        v
                    } else {
                        generated = true;
                        format!("{}:{}/{}", pkg_raw, class_name, nname)
                    };
                    if s.t != T::Semicolon {
                        self.pushback(s);
                    }
                    traits.push(node(Item::Slot(Slot {
                        metadata,
                        pkg: namespace,
                        custom_ns,
                        is_static: true,
                        var: nname,
                        ty: ty_parts(&["Namespace"]),
                        value: Some(node(Item::Str(nval))),
                        line: self.line(),
                        is_const: true,
                        ns: true,
                        generated_ns: generated,
                    })));
                }
                T::Const | T::Var => {
                    let is_const = s.t == T::Const;
                    let what = if is_const { "consts" } else { "vars" };
                    if is_override {
                        return self.err(format!("Override flag not allowed for {what}"));
                    }
                    if is_final {
                        return self.err(format!("Final flag not allowed for {what}"));
                    }
                    if is_interface {
                        return self.err("Interface cannot have variable/const fields");
                    }
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let vcname = s.text();
                    let field_line = self.line();
                    s = self.lex()?;
                    if s.t == T::NamespaceSuffix {
                        namespace = ns_index_item(int_of(&s) as i64);
                        s = self.lex()?;
                    }
                    let fields = if is_static { &mut static_fields } else { &mut instance_fields };
                    self.check_duplicate_field(fields, &namespace, &vcname, field_line)?;
                    let tenv = Env { this_type: Some(this_type.clone()), needs_act: Rc::new(Cell::new(false)), variables: vars(), ..env.clone() };
                    let ty = if s.t == T::Colon {
                        let t = self.type_(&tenv)?;
                        s = self.lex()?;
                        t
                    } else {
                        unbounded()
                    };
                    let mut value = None;
                    if s.t == T::Assign {
                        let v = if is_static || is_const { cinit_variables } else { iinit_variables };
                        let venv = Env { this_type: Some(this_type.clone()), needs_act: Rc::new(Cell::new(false)), variables: v.clone(), ..env.clone() };
                        value = self.expression(&venv, false, false)?;
                        s = self.lex()?;
                    }
                    traits.push(node(Item::Slot(Slot { metadata, pkg: namespace, custom_ns, is_static, var: vcname, ty, value, line: self.line(), is_const, ns: false, generated_ns: false })));
                    if s.t != T::Semicolon {
                        self.pushback(s);
                    }
                }
                _ => {
                    self.pushback(s);
                    for p in pre.into_iter().rev() {
                        self.pushback(p);
                    }
                    let cenv = Env { this_type: None, pkg: Some(public_ns.clone()), needs_act: cinit_act.clone(), variables: cinit_variables.clone(), ..env.clone() };
                    match self.command(&cenv, &mut cinit_loops, false)? {
                        Some(cmd) => traits.push(cmd),
                        None => break,
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn script_traits_block(&mut self, imported: &Imports, opened: &NsList, all_ns: &AllNs, script_index: usize, script_name: &str, traits: &mut Nodes, sinit_act: &Rc<Cell<bool>>, sinit_loops: &mut Loops, sinit_variables: &Vars, pkg_ref: &mut Option<Dotted>) -> R<bool> {
        let mut in_package = false;
        let mut s = self.lex()?;
        let mut pkg_name = Dotted::default();
        let public_ns;
        let package_internal_ns;
        if s.t == T::Package {
            s = self.lex()?;
            if s.t != T::CurlyOpen {
                self.expected(&s, &[E::G(Group::Identifier)])?;
                pkg_name = pkg_name.add_with_suffix(&s.text());
                s = self.lex()?;
            }
            while s.t == T::Dot {
                s = self.lex()?;
                self.expected(&s, &[E::G(Group::Identifier)])?;
                pkg_name = pkg_name.add_with_suffix(&s.text());
                s = self.lex()?;
            }
            self.expected(&s, &[E::T(T::CurlyOpen)])?;
            *pkg_ref = Some(pkg_name.clone());
            public_ns = Some(ns_item(pkg_name.clone(), ns_kind::PACKAGE as i32, 0));
            package_internal_ns = ns_item(pkg_name.clone(), ns_kind::PACKAGE_INTERNAL as i32, 0);
            s = self.lex()?;
            in_package = true;
        } else {
            public_ns = None;
            package_internal_ns = ns_item_str(&format!("{script_name}${script_index}"), ns_kind::PRIVATE as i32, 0);
        }
        self.pushback(s);
        all_ns.borrow_mut().push(opened.clone());
        opened.borrow_mut().push(ns_item_str("", ns_kind::PACKAGE as i32, 0));
        let as3 = ns_item_str(AS3_NAMESPACE, ns_kind::NAMESPACE as i32, 0);
        self.cx.force_resolve(&as3);
        opened.borrow_mut().push(as3);
        for ln in all_ns.borrow().iter() {
            if let Some(p) = &public_ns {
                if !ns_list_contains(ln, p) {
                    ln.borrow_mut().push(p.clone());
                }
            }
            if !ns_list_contains(ln, &package_internal_ns) {
                ln.borrow_mut().push(package_internal_ns.clone());
            }
        }
        self.parse_imports_usages(imported, opened)?;
        let mut is_empty = true;
        let env0 = Env { all_ns: all_ns.clone(), this_type: None, pkg: None, needs_act: Rc::new(Cell::new(false)), imported: imported.clone(), opened: opened.clone(), variables: vars() };
        loop {
            let metadata = self.parse_metadata()?;
            s = self.lex()?;
            let mut is_final = false;
            let mut is_dynamic = false;
            let mut is_public = false;
            let mut is_native = false;
            let mut is_internal = false;
            let mut ns = package_internal_ns.clone();
            let mut pre: Vec<Sym> = Vec::new();
            while s.is(&[T::Final, T::Dynamic, T::Public, T::Internal, T::Native]) {
                if s.t == T::Final {
                    if is_final {
                        return self.err("Only one final keyword allowed");
                    }
                    is_final = true;
                    pre.push(s.clone());
                }
                if s.t == T::Internal {
                    if !in_package {
                        return self.err("internal only allowed inside package");
                    }
                    if is_public || is_internal {
                        return self.err("Only one of public/internal keyword allowed");
                    }
                    is_internal = true;
                    ns = package_internal_ns.clone();
                    pre.push(s.clone());
                }
                if s.t == T::Public {
                    if !in_package {
                        return self.err("public only allowed inside package");
                    }
                    if is_public || is_internal {
                        return self.err("Only one of public/internal keyword allowed");
                    }
                    is_public = true;
                    ns = public_ns.clone().unwrap();
                    pre.push(s.clone());
                }
                if s.t == T::Dynamic {
                    if is_dynamic {
                        return self.err("Only one dynamic keyword allowed");
                    }
                    is_dynamic = true;
                    pre.push(s.clone());
                }
                if s.t == T::Native {
                    if is_native {
                        return self.err("Only one native keyword allowed");
                    }
                    is_native = true;
                    pre.push(s.clone());
                }
                s = self.lex()?;
            }
            match s.t {
                T::Class | T::Interface => {
                    is_empty = false;
                    let sub_opened = ns_list_copy(opened);
                    let is_interface = s.t == T::Interface;
                    let mut extends = None;
                    let mut interfaces = Vec::new();
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let sub_name = s.text();
                    s = self.lex()?;
                    let mut nullable = true;
                    if s.t == T::Not {
                        s = self.lex()?;
                        nullable = false;
                    } else if s.t == T::Ternar {
                        s = self.lex()?;
                    }
                    let tenv = Env { pkg: Some(ns.clone()), needs_act: Rc::new(Cell::new(false)), variables: vars(), ..env0.clone() };
                    if !is_interface {
                        if s.t == T::Extends {
                            extends = Some(self.type_(&tenv)?);
                            s = self.lex()?;
                        }
                        if s.t == T::Implements {
                            loop {
                                interfaces.push(self.type_(&tenv)?);
                                s = self.lex()?;
                                if s.t != T::Comma {
                                    break;
                                }
                            }
                        }
                    } else if s.t == T::Extends {
                        loop {
                            interfaces.push(self.type_(&tenv)?);
                            s = self.lex()?;
                            if s.t != T::Comma {
                                break;
                            }
                        }
                    }
                    self.expected(&s, &[E::T(T::CurlyOpen)])?;
                    if let Some(ext) = &extends {
                        let full = pkg_name.add_with_suffix(&sub_name).raw();
                        for (name, nsname) in self.cx.parent_names_of_extends(ext, &full)? {
                            if nsname.is_empty() {
                                continue;
                            }
                            sub_opened.borrow_mut().push(ns_item_str(&format!("{nsname}:{name}"), ns_kind::STATIC_PROTECTED as i32, 0));
                        }
                    }
                    let cinit_act = Rc::new(Cell::new(false));
                    let mut iinit = None;
                    let cinit_variables = vars();
                    let iinit_variables = vars();
                    let iinit_act = Rc::new(Cell::new(false));
                    let mut sub_traits = Vec::new();
                    let cenv = Env { opened: sub_opened.clone(), ..env0.clone() };
                    self.class_traits(&cenv, !in_package, &cinit_variables, &cinit_act, &ns, &sub_name, is_interface, &mut sub_traits, &iinit_variables, &iinit_act, &mut iinit)?;
                    if is_interface {
                        traits.push(node(Item::Interface(Interface { metadata, imported: imported.clone(), pkg: ns.clone(), opened: sub_opened, is_final, base_name: sub_name, super_interfaces: interfaces, methods: sub_traits, nullable })));
                    } else {
                        traits.push(node(Item::Class(Class {
                            metadata,
                            imported: imported.clone(),
                            pkg: ns.clone(),
                            opened: sub_opened,
                            is_final,
                            is_dynamic,
                            base_name: sub_name,
                            extends,
                            implements: interfaces,
                            cinit_activation: cinit_act.get(),
                            cinit_variables: cinit_variables.borrow().clone(),
                            iinit,
                            iinit_variables: iinit_variables.borrow().clone(),
                            traits: sub_traits,
                            iinit_activation: iinit_act.get(),
                            nullable,
                        })));
                    }
                    self.expect_t(T::CurlyClose)?;
                }
                T::Function => {
                    is_empty = false;
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let fname = s.text();
                    let menv = Env { this_type: None, needs_act: Rc::new(Cell::new(false)), variables: vars(), ..env0.clone() };
                    traits.push(self.method(&menv, !in_package, false, metadata, Some(ns.clone()), false, is_native, None, false, is_final, true, &fname, MethodKind::Method)?);
                }
                T::Const | T::Var => {
                    is_empty = false;
                    let is_const = s.t == T::Const;
                    if is_final {
                        return self.err(format!("Final flag not allowed for {}", if is_const { "consts" } else { "vars" }));
                    }
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let vcname = s.text();
                    s = self.lex()?;
                    let tenv = Env { pkg: Some(ns.clone()), needs_act: Rc::new(Cell::new(false)), variables: vars(), ..env0.clone() };
                    let ty = if s.t == T::Colon {
                        let t = self.type_(&tenv)?;
                        s = self.lex()?;
                        t
                    } else {
                        unbounded()
                    };
                    let mut value = None;
                    if s.t == T::Assign {
                        value = self.expression(&tenv.with_vars(sinit_variables), false, false)?;
                        s = self.lex()?;
                    }
                    traits.push(node(Item::Slot(Slot { metadata, pkg: ns.clone(), custom_ns: None, is_static: true, var: vcname, ty, value, line: self.line(), is_const, ns: false, generated_ns: false })));
                    if s.t != T::Semicolon {
                        self.pushback(s);
                    }
                }
                T::Namespace => {
                    is_empty = false;
                    if is_final {
                        return self.err("Final flag not allowed for namespaces");
                    }
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let nname = s.text();
                    s = self.lex()?;
                    let mut generated = false;
                    let nval = if s.t == T::Assign {
                        s = self.lex()?;
                        self.expected(&s, &[E::T(T::String)])?;
                        let v = s.text();
                        s = self.lex()?;
                        v
                    } else {
                        generated = true;
                        format!("{}:{}", ns.borrow().raw_name(), nname)
                    };
                    if s.t != T::Semicolon {
                        self.pushback(s);
                    }
                    traits.push(node(Item::Slot(Slot {
                        metadata,
                        pkg: ns.clone(),
                        custom_ns: None,
                        is_static: true,
                        var: nname,
                        ty: ty_parts(&["Namespace"]),
                        value: Some(node(Item::Str(nval))),
                        line: self.line(),
                        is_const: true,
                        ns: true,
                        generated_ns: generated,
                    })));
                }
                _ => {
                    self.pushback(s);
                    for p in pre.into_iter().rev() {
                        self.pushback(p);
                    }
                    if self.parse_imports_usages(imported, opened)? {
                        continue;
                    }
                    let cenv = Env { this_type: None, pkg: public_ns.clone(), needs_act: sinit_act.clone(), variables: sinit_variables.clone(), ..env0.clone() };
                    match self.command(&cenv, sinit_loops, false)? {
                        Some(cmd) => {
                            traits.push(cmd);
                            is_empty = false;
                        }
                        None => break,
                    }
                }
            }
        }
        if in_package {
            self.expect_t(T::CurlyClose)?;
        }
        Ok(!is_empty)
    }

    fn xmltag(&mut self, env: &Env, uses_vars: &Cell<bool>, opened_tags: &mut Vec<String>) -> R<Nodes> {
        let mut rets: Nodes = Vec::new();
        let mut sb = String::new();
        loop {
            let mut s = self.lex()?;
            let mut sub: Vec<String> = Vec::new();
            let subuses = Cell::new(false);
            match s.t {
                T::XmlAttrNameVarBegin => {
                    uses_vars.set(true);
                    add_s(&mut rets, &mut sb);
                    rets.push(self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?);
                    self.expect_t(T::CurlyClose)?;
                    self.expect_t(T::Assign)?;
                    sb.push('=');
                    self.lx.state = State::XmlOpenTagAttrib;
                }
                T::XmlAttrValVarBegin => {
                    uses_vars.set(true);
                    sb.push('"');
                    add_s(&mut rets, &mut sb);
                    let e = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    rets.push(node(Item::EscXAttr(e)));
                    sb.push('"');
                    self.expect_t(T::CurlyClose)?;
                    self.lx.state = State::XmlOpenTag;
                }
                T::XmlVarBegin => {
                    uses_vars.set(true);
                    add_s(&mut rets, &mut sb);
                    let e = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    rets.push(node(Item::EscXElem(e)));
                    self.expect_t(T::CurlyClose)?;
                    self.lx.state = State::Xml;
                }
                T::XmlFinishVarTagBegin => {
                    uses_vars.set(true);
                    sb.push_str("</");
                    add_s(&mut rets, &mut sb);
                    rets.push(self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?);
                    self.expect_t(T::CurlyClose)?;
                    self.lx.begin(State::XmlCloseTagFinish);
                    s = self.lex()?;
                    while s.t == T::XmlText {
                        sb.push_str(&s.text());
                        s = self.lex()?;
                    }
                    self.expected(&s, &[E::T(T::GreaterThan)])?;
                    sb.push('>');
                    add_s(&mut rets, &mut sb);
                    if opened_tags.is_empty() {
                        return self.err("XML : Closing unopened tag");
                    }
                    opened_tags.pop();
                }
                T::XmlStartVarTagBegin => {
                    let ex = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    self.expect_t(T::CurlyClose)?;
                    self.lx.state = State::XmlOpenTag;
                    sub.push("*".into());
                    sb.push('<');
                    add_s(&mut rets, &mut sb);
                    rets.push(ex);
                    let st = self.xmltag(env, &subuses, &mut sub)?;
                    rets.extend(st);
                }
                T::XmlStartTagBegin => {
                    let v = s.text();
                    sub.push(v.trim().chars().skip(1).collect());
                    let st = self.xmltag(env, &subuses, &mut sub)?;
                    sb.push_str(&v);
                    add_s(&mut rets, &mut sb);
                    rets.extend(st);
                }
                T::XmlFinishTag => {
                    let v = s.text();
                    let cs: Vec<char> = v.chars().collect();
                    let tname: String = if cs.len() >= 3 { cs[2..cs.len() - 1].iter().collect::<String>().trim().to_string() } else { String::new() };
                    let Some(last) = opened_tags.last() else { return self.err("XML : Closing unopened tag") };
                    if *last == tname || last == "*" {
                        opened_tags.pop();
                    } else {
                        return self.err("XML : Closing unopened tag");
                    }
                    sb.push_str(&v);
                }
                T::XmlStartFinishTagEnd => {
                    opened_tags.pop();
                    sb.push_str(&s.text());
                }
                T::Eof => return self.err("End of file before XML finish"),
                _ => sb.push_str(&s.text()),
            }
            if opened_tags.is_empty() {
                break;
            }
        }
        add_s(&mut rets, &mut sb);
        Ok(rets)
    }

    fn xml(&mut self, env: &Env) -> R<Node> {
        let mut opened_tags = Vec::new();
        let mut parts = self.xmltag(env, &Cell::new(false), &mut opened_tags)?;
        self.lx.enable_white_space = true;
        self.lx.begin(State::Initial);
        let mut s = self.lex()?;
        while s.t == T::XmlWhitespace {
            let mut sb = s.text();
            add_s(&mut parts, &mut sb);
            s = self.lex()?;
        }
        self.lx.enable_white_space = false;
        self.pushback(s);
        let ret = add_all(parts).unwrap_or_else(|| node(Item::Str(String::new())));
        Ok(node(Item::Xml(ret)))
    }

    fn command(&mut self, env: &Env, loops: &mut Loops, _must_be_command: bool) -> R<Option<Node>> {
        self.lx.recorders.push(Vec::new());
        let r = self.command_inner(env, loops);
        if r.is_err() || matches!(r, Ok(CmdOut::Done(_))) {
            self.lx.recorders.pop();
        }
        match r? {
            CmdOut::Done(n) => Ok(n),
            CmdOut::Ret(ret) => {
                let events = self.lx.recorders.pop().unwrap_or_default();
                let ret = match ret {
                    Some(r) => Some(r),
                    None => {
                        let mut items: Vec<Sym> = Vec::new();
                        for e in events {
                            match e {
                                RecEvent::Lex(s) => items.push(s),
                                RecEvent::Push(s) => {
                                    if items.last() == Some(&s) {
                                        items.pop();
                                    }
                                }
                            }
                        }
                        for s in items.into_iter().rev() {
                            self.pushback(s);
                        }
                        self.expression(env, false, false)?
                    }
                };
                self.finish_command(ret)
            }
        }
    }

    fn finish_command(&mut self, ret: Option<Node>) -> R<Option<Node>> {
        let s = self.lex()?;
        if s.t != T::Semicolon {
            self.pushback(s);
        }
        Ok(ret)
    }

    fn command_inner(&mut self, env: &Env, loops: &mut Loops) -> R<CmdOut> {
        let mut ret: Option<Node> = None;
        let mut s = self.lex()?;
        if s.t == T::Eof {
            return Ok(CmdOut::Done(None));
        }
        let mut loop_label = None;
        if s.group == Group::Identifier {
            let sc = self.lex()?;
            if sc.t == T::Colon {
                loop_label = Some(s.text());
                s = self.lex()?;
            } else {
                self.pushback(sc);
            }
        }
        if s.t == T::Default {
            let sx = self.lex()?;
            if sx.group != Group::Identifier || sx.text() != "xml" {
                self.pushback(sx);
            } else {
                self.expect_t(T::Namespace)?;
                self.expect_t(T::Assign)?;
                let ns = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                ret = Some(node(Item::DefaultXmlNs(ns)));
            }
        }
        if ret.is_none() {
            match s.t {
                T::Use => {
                    self.expect_t(T::Namespace)?;
                    let ns = self.type_(env)?;
                    let n = ns_item_str(&text(&ns), KIND_NAMESPACE_CUSTOM, self.line());
                    env.opened.borrow_mut().push(n);
                }
                T::With => {
                    env.needs_act.set(true);
                    self.expect_t(T::ParentOpen)?;
                    let wvar = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    if !is_name_or_prop(&wvar) {
                        return self.err("Not a property or name");
                    }
                    self.expect_t(T::ParentClose)?;
                    self.expect_t(T::CurlyOpen)?;
                    let with_vars = vars();
                    let wcmd = self.commands(&env.with_vars(&with_vars), loops)?;
                    let wv = with_vars.borrow().clone();
                    env.variables.borrow_mut().extend(wv.iter().cloned());
                    for a in &wv {
                        if let Item::Unresolved(u) = &mut *a.borrow_mut() {
                            u.scope_stack.insert(0, wvar.clone());
                        }
                    }
                    self.expect_t(T::CurlyClose)?;
                    ret = Some(node(Item::With { scope: wvar, items: wcmd, subvariables: wv }));
                }
                T::Function => {
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    env.needs_act.set(true);
                    let f = self.function(env, Vec::new(), env.pkg.clone(), false, false, &s.text(), false)?;
                    ret = Some(node(Item::Function(f)));
                }
                T::Var | T::Const => {
                    let is_const = s.t == T::Const;
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    let var_id = s.text();
                    s = self.lex()?;
                    let ty = if s.t == T::Colon {
                        let t = self.type_(env)?;
                        s = self.lex()?;
                        t
                    } else {
                        unbounded()
                    };
                    if s.t == T::Assign {
                        let v = self.expression(env, false, false)?;
                        let n = node(new_name(Some(ty), self.line(), false, &var_id, "", v, true, env.opened.clone(), is_const));
                        env.variables.borrow_mut().push(n.clone());
                        ret = Some(n);
                    } else {
                        let n = node(new_name(Some(ty), self.line(), false, &var_id, "", None, true, env.opened.clone(), is_const));
                        env.variables.borrow_mut().push(n.clone());
                        ret = Some(n);
                        self.pushback(s);
                    }
                }
                T::CurlyOpen => {
                    let c = self.commands(env, loops)?;
                    ret = Some(node(Item::Block(c)));
                    self.expect_t(T::CurlyClose)?;
                }
                T::Super => {
                    let ss2 = self.lex()?;
                    if ss2.t == T::ParentOpen {
                        let args = self.call(env)?;
                        ret = Some(node(Item::ConstructSuper { obj: node(Item::LocalReg { reg: 0, ty: ty_parts(&["Object"]) }), args }));
                    } else {
                        self.pushback(ss2);
                        self.pushback(s.clone());
                    }
                }
                T::If => {
                    self.expect_t(T::ParentOpen)?;
                    let cond = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    self.expect_t(T::ParentClose)?;
                    let on_true = self.command(env, loops, true)?;
                    let on_true: Nodes = on_true.into_iter().collect();
                    s = self.lex()?;
                    let mut on_false = None;
                    if s.t == T::Else {
                        on_false = Some(self.command(env, loops, true)?.into_iter().collect());
                    } else {
                        self.pushback(s.clone());
                    }
                    ret = Some(node(Item::If { cond, on_true, on_false }));
                }
                T::While => {
                    self.expect_t(T::ParentOpen)?;
                    let expr: Nodes = self.expression(env, false, true)?.into_iter().collect();
                    self.expect_t(T::ParentClose)?;
                    let id = self.uniq_id();
                    if let Some(l) = &loop_label {
                        loops.labels.insert(id, l.clone());
                    }
                    loops.stack.push(id);
                    let body: Nodes = self.command(env, loops, true)?.into_iter().collect();
                    ret = Some(node(Item::While { id: LoopId(id), expr, body }));
                    loops.stack.pop();
                }
                T::Do => {
                    let id = self.uniq_id();
                    loops.stack.push(id);
                    if let Some(l) = &loop_label {
                        loops.labels.insert(id, l.clone());
                    }
                    let body: Nodes = self.command(env, loops, true)?.into_iter().collect();
                    self.expect_t(T::While)?;
                    self.expect_t(T::ParentOpen)?;
                    let expr: Nodes = self.expression(env, false, true)?.into_iter().collect();
                    self.expect_t(T::ParentClose)?;
                    ret = Some(node(Item::DoWhile { id: LoopId(id), body, expr }));
                    loops.stack.pop();
                }
                T::For => {
                    s = self.lex()?;
                    let mut forin = false;
                    let mut each = false;
                    if s.t == T::Each {
                        each = true;
                        forin = true;
                        s = self.lex()?;
                    }
                    self.expected(&s, &[E::T(T::ParentOpen)])?;
                    let mut first = self.command(env, loops, false)?;
                    if let Some(f) = &first {
                        let def_no_value = matches!(&*f.borrow(), Item::Name(n) if n.definition && n.assigned.is_none());
                        if def_no_value {
                            first = Some(self.expression1(env, f.clone(), super::lexer::NOPRECEDENCE)?);
                        }
                    }
                    let mut inexpr = None;
                    if let Some(f) = &first {
                        if matches!(&*f.borrow(), Item::Bin { op: Op::In, .. }) {
                            forin = true;
                            inexpr = Some(f.clone());
                        }
                    }
                    if inexpr.is_none() && forin {
                        return self.err("In expression required");
                    }
                    let id = self.uniq_id();
                    loops.stack.push(id);
                    if let Some(l) = &loop_label {
                        loops.labels.insert(id, l.clone());
                    }
                    let mut fin = Vec::new();
                    let mut expr = None;
                    let mut first_cmds = Vec::new();
                    if !forin {
                        let s2 = self.lex()?;
                        if let Some(f) = first.take() {
                            first_cmds.push(f);
                        }
                        self.pushback(s2);
                        expr = self.expression(env, false, false)?;
                        if expr.is_none() {
                            expr = Some(node(Item::True));
                        }
                        self.expect_t(T::Semicolon)?;
                        if let Some(c) = self.command(env, loops, true)? {
                            fin.push(c);
                        }
                    }
                    self.expect_t(T::ParentClose)?;
                    let body: Nodes = self.command(env, loops, true)?.into_iter().collect();
                    ret = Some(if forin {
                        node(Item::ForIn { id: LoopId(id), each, expr: inexpr.unwrap(), body })
                    } else {
                        node(Item::For { id: LoopId(id), first: first_cmds, expr: expr.unwrap(), fin, body })
                    });
                    loops.stack.pop();
                }
                T::Switch => {
                    let id = -self.uniq_id();
                    loops.stack.push(id);
                    if let Some(l) = &loop_label {
                        loops.labels.insert(id, l.clone());
                    }
                    self.expect_t(T::ParentOpen)?;
                    let expr = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    self.expect_t(T::ParentClose)?;
                    self.expect_t(T::CurlyOpen)?;
                    s = self.lex()?;
                    self.uniq_id();
                    let mut cmds = Vec::new();
                    let mut values = Vec::new();
                    let mut mapping = Vec::new();
                    let mut pos = 0;
                    while s.t == T::Case || s.t == T::Default {
                        while s.t == T::Case || s.t == T::Default {
                            let v = if s.t == T::Default { node(Item::Default) } else { self.expression(env, false, true)?.ok_or_else(|| self.error("Expression expected"))? };
                            self.expect_t(T::Colon)?;
                            s = self.lex()?;
                            values.push(v);
                            mapping.push(pos);
                        }
                        pos += 1;
                        self.pushback(s);
                        cmds.push(self.commands(env, loops)?);
                        s = self.lex()?;
                    }
                    self.expected(&s, &[E::T(T::CurlyClose)])?;
                    ret = Some(node(Item::Switch { id: LoopId(id), expr, values, cmds, mapping }));
                    loops.stack.pop();
                }
                T::Break => {
                    s = self.lex()?;
                    if loops.stack.is_empty() {
                        return self.err("No loop to break");
                    }
                    let mut id = 0;
                    if s.group == Group::Identifier {
                        let l = s.text();
                        for lid in &loops.stack {
                            if loops.labels.get(lid) == Some(&l) {
                                id = *lid;
                                break;
                            }
                        }
                        if id == 0 {
                            return self.err("Identifier of loop expected");
                        }
                    } else {
                        self.pushback(s.clone());
                        id = *loops.stack.last().unwrap();
                    }
                    ret = Some(node(Item::Break(id)));
                }
                T::Continue => {
                    s = self.lex()?;
                    if loops.stack.is_empty() {
                        return self.err("No loop to continue");
                    }
                    let mut id = 0;
                    if s.group == Group::Identifier {
                        let l = s.text();
                        for lid in &loops.stack {
                            if *lid < 0 {
                                continue;
                            }
                            if loops.labels.get(lid) == Some(&l) {
                                id = *lid;
                                break;
                            }
                        }
                    } else {
                        self.pushback(s.clone());
                        for lid in loops.stack.iter().rev() {
                            if *lid >= 0 {
                                id = *lid;
                                break;
                            }
                        }
                        if id <= 0 {
                            return self.err("No loop to continue");
                        }
                    }
                    ret = Some(node(Item::Continue(id)));
                }
                T::Return => {
                    let e = self.expression_allow_empty(env, true, false)?;
                    ret = Some(match e {
                        None => node(Item::ReturnVoid),
                        Some(e) => node(Item::ReturnValue(e)),
                    });
                }
                T::Try => {
                    env.needs_act.set(true);
                    let try_cmds: Nodes = self.command(env, loops, true)?.into_iter().collect();
                    s = self.lex()?;
                    let mut found = false;
                    let mut catch_cmds = Vec::new();
                    let mut catch_exceptions = Vec::new();
                    let mut catches_vars = Vec::new();
                    while s.t == T::Catch {
                        self.expect_t(T::ParentOpen)?;
                        s = self.lex()?;
                        self.expected(&s, &[E::G(Group::Identifier), E::T(T::This), E::T(T::Super)])?;
                        let ename = s.text();
                        self.expect_t(T::Colon)?;
                        let etype = self.type_(env)?;
                        let e = node(new_name(Some(etype), self.line(), false, &ename, "", Some(node(Item::Exception)), true, env.opened.clone(), false));
                        if let Item::Name(n) = &mut *e.borrow_mut() {
                            n.slot = 1;
                            n.slot_scope = i32::MAX as i64;
                        }
                        catch_exceptions.push(e.clone());
                        self.expect_t(T::ParentClose)?;
                        let catch_vars = vars();
                        self.expect_t(T::CurlyOpen)?;
                        let cc = self.commands(&env.with_vars(&catch_vars), loops)?;
                        self.expect_t(T::CurlyClose)?;
                        let cv = catch_vars.borrow().clone();
                        catches_vars.push(cv.clone());
                        let mut new_vars = cv.clone();
                        let mut i = 0;
                        while i < new_vars.len() {
                            let a = new_vars[i].clone();
                            let same = matches!(&*a.borrow(), Item::Unresolved(u) if u.name == Dotted::parse_with_suffix(&ename));
                            if same {
                                let _ = self.cx.resolve_unresolved(&a, None, None, None, &[], &[], &empty_stack(), &[e.clone()]);
                                let (slot, scope) = name_slot(&e);
                                set_unresolved_slot(&a, slot, scope);
                                new_vars.remove(i);
                                continue;
                            }
                            i += 1;
                        }
                        env.variables.borrow_mut().extend(new_vars);
                        catch_cmds.push(cc);
                        s = self.lex()?;
                        found = true;
                    }
                    let mut finally_cmds = None;
                    if s.t == T::Finally {
                        finally_cmds = Some(self.command(env, loops, true)?.into_iter().collect());
                        found = true;
                        s = self.lex()?;
                    }
                    if !found {
                        self.expected(&s, &[E::T(T::Catch), E::T(T::Finally)])?;
                    }
                    self.pushback(s.clone());
                    ret = Some(node(Item::Try(Try { try_cmds, catch_cmds, finally_cmds, catch_variables: catches_vars, catch_exceptions })));
                }
                T::Throw => {
                    let e = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    ret = Some(node(Item::Throw(e)));
                }
                _ => {
                    if s.t == T::Semicolon {
                        return Ok(CmdOut::Done(Some(node(Item::Empty))));
                    }
                    self.pushback(s.clone());
                    ret = self.expression(env, false, true)?;
                }
            }
        }
        Ok(CmdOut::Ret(ret))
    }

    fn brackets(&mut self, env: &Env, ret: &mut Nodes) -> R<i64> {
        let mut s = self.lex()?;
        let mut cnt = 0;
        if s.t == T::BracketOpen {
            s = self.lex()?;
            while s.t != T::BracketClose {
                if s.t != T::Comma {
                    self.pushback(s);
                }
                cnt += 1;
                if let Some(e) = self.expression(env, false, false)? {
                    ret.push(e);
                }
                s = self.lex()?;
                if !s.is(&[T::Comma, T::BracketClose]) {
                    self.expected(&s, &[E::T(T::Comma), E::T(T::BracketClose)])?;
                }
            }
        } else {
            self.pushback(s);
            return Ok(-1);
        }
        Ok(cnt)
    }

    fn expression(&mut self, env: &Env, _allow_remainder: bool, allow_comma: bool) -> R<Option<Node>> {
        self.expression_allow_empty(env, false, allow_comma)
    }

    fn expression_allow_empty(&mut self, env: &Env, allow_empty: bool, allow_comma: bool) -> R<Option<Node>> {
        let _ = allow_empty;
        let mut items = Vec::new();
        loop {
            let Some(prim) = self.expression_primary(env, true)? else { return Ok(None) };
            let item = self.expression1(env, prim, super::lexer::NOPRECEDENCE)?;
            items.push(item);
            let symb = self.lex()?;
            if allow_comma && symb.t == T::Comma {
                continue;
            }
            self.pushback(symb);
            break;
        }
        if items.len() == 1 {
            return Ok(items.pop());
        }
        Ok(Some(node(Item::Comma(items))))
    }

    fn xml_to_lower_than_fix(&mut self, s: &mut Sym) {
        if s.is(&[T::XmlStartVarTagBegin, T::XmlStartTagBegin]) {
            let mut pb: String = s.text().chars().skip(1).collect();
            s.t = T::LowerThan;
            s.group = Group::Operator;
            s.v = V::Str("<".into());
            let mut pos = 1;
            if pb.starts_with('=') {
                s.t = T::LowerEqual;
                s.v = V::Str("<=".into());
                pb = pb.chars().skip(1).collect();
                pos += 1;
            }
            self.lx.pushback_str(&pb, State::Initial, pos);
        }
    }

    fn regexp_to_divide_fix(&mut self, s: &mut Sym) {
        if s.t == T::Regexp {
            let mut pb: String = s.text().chars().skip(1).collect();
            s.t = T::Divide;
            s.group = Group::Operator;
            s.v = V::Str("/".into());
            let mut pos = 1;
            if pb.starts_with('=') {
                s.t = T::AssignDivide;
                s.v = V::Str("/=".into());
                pb = pb.chars().skip(1).collect();
                pos += 1;
            }
            self.lx.pushback_str(&pb, State::Initial, pos);
        }
    }

    fn peek_expr_token(&mut self) -> R<Sym> {
        let mut l = self.lex()?;
        self.xml_to_lower_than_fix(&mut l);
        self.regexp_to_divide_fix(&mut l);
        self.pushback(l.clone());
        Ok(l)
    }

    fn expression1(&mut self, env: &Env, mut lhs: Node, min_prec: i32) -> R<Node> {
        let mut look = self.peek_expr_token()?;
        while look.t.is_binary() && look.t.precedence() <= min_prec {
            let op = look.clone();
            self.lex()?;
            let mut mhs = None;
            if op.t == T::Ternar {
                mhs = self.expression(env, false, false)?;
                self.expect_t(T::Colon)?;
            }
            let Some(mut rhs) = self.expression_primary(env, true)? else { return self.err("Missing operand") };
            look = self.peek_expr_token()?;
            while (look.t.is_binary() && look.t.precedence() < op.t.precedence()) || (look.t.is_right_associative() && look.t.precedence() == op.t.precedence()) {
                rhs = self.expression1(env, rhs, look.t.precedence())?;
                look = self.peek_expr_token()?;
            }
            let bin = |o: Op, l: Node, r: Node| node(Item::Bin { op: o, l, r });
            lhs = match op.t {
                T::As => bin(Op::As, lhs, rhs),
                T::In => bin(Op::In, lhs, rhs),
                T::Ternar => node(Item::Ternar { cond: lhs, t: mhs.ok_or_else(|| self.error("Missing operand"))?, f: rhs }),
                T::ShiftLeft => bin(Op::LShift, lhs, rhs),
                T::ShiftRight => bin(Op::RShift, lhs, rhs),
                T::UShiftRight => bin(Op::URShift, lhs, rhs),
                T::BitAnd => bin(Op::BitAnd, lhs, rhs),
                T::BitOr => bin(Op::BitOr, lhs, rhs),
                T::Divide => bin(Op::Divide, lhs, rhs),
                T::Modulo => bin(Op::Modulo, lhs, rhs),
                T::Equals => bin(Op::Eq, lhs, rhs),
                T::StrictEquals => bin(Op::StrictEq, lhs, rhs),
                T::NotEqual => bin(Op::Neq, lhs, rhs),
                T::StrictNotEqual => bin(Op::StrictNeq, lhs, rhs),
                T::LowerThan => bin(Op::Lt, lhs, rhs),
                T::LowerEqual => bin(Op::Le, lhs, rhs),
                T::GreaterThan => bin(Op::Gt, lhs, rhs),
                T::GreaterEqual => bin(Op::Ge, lhs, rhs),
                T::And => node(Item::And { l: lhs, r: rhs }),
                T::Or => node(Item::Or { l: lhs, r: rhs }),
                T::Minus => bin(Op::Subtract, lhs, rhs),
                T::Multiply => bin(Op::Multiply, lhs, rhs),
                T::Plus => bin(Op::Add, lhs, rhs),
                T::Xor => bin(Op::BitXor, lhs, rhs),
                T::InstanceOf => bin(Op::InstanceOf, lhs, rhs),
                T::Is => bin(Op::Is, lhs, rhs),
                T::NullCoalesce => bin(Op::NullCoalesce, lhs, rhs),
                T::Assign | T::AssignBitAnd | T::AssignBitOr | T::AssignDivide | T::AssignMinus | T::AssignModulo | T::AssignMultiply | T::AssignPlus | T::AssignShiftLeft | T::AssignShiftRight | T::AssignUShiftRight | T::AssignXor | T::AssignAnd | T::AssignOr => {
                    let l = lhs.clone();
                    let assigned = match op.t {
                        T::AssignBitAnd => bin(Op::BitAnd, l, rhs),
                        T::AssignBitOr => bin(Op::BitOr, l, rhs),
                        T::AssignDivide => bin(Op::Divide, l, rhs),
                        T::AssignMinus => bin(Op::Subtract, l, rhs),
                        T::AssignModulo => bin(Op::Modulo, l, rhs),
                        T::AssignMultiply => bin(Op::Multiply, l, rhs),
                        T::AssignPlus => bin(Op::Add, l, rhs),
                        T::AssignShiftLeft => bin(Op::LShift, l, rhs),
                        T::AssignShiftRight => bin(Op::RShift, l, rhs),
                        T::AssignUShiftRight => bin(Op::URShift, l, rhs),
                        T::AssignXor => bin(Op::BitXor, l, rhs),
                        T::AssignAnd => node(Item::And { l, r: rhs }),
                        T::AssignOr => node(Item::Or { l, r: rhs }),
                        _ => rhs,
                    };
                    if !is_assignable(&lhs) {
                        return self.err("Invalid assignment");
                    }
                    let a = copy_assignable(&lhs);
                    if matches!(&*a.borrow(), Item::Unresolved(_) | Item::Name(_)) {
                        env.variables.borrow_mut().push(a.clone());
                    }
                    set_assigned(&a, Some(assigned));
                    if let Item::Name(n) = &mut *lhs.borrow_mut() {
                        n.definition = false;
                    }
                    a
                }
                _ => lhs,
            };
        }
        Ok(lhs)
    }

    fn expression_primary(&mut self, env: &Env, _allow_remainder: bool) -> R<Option<Node>> {
        let mut s = self.lex()?;
        let mut allow_member = false;
        let ret: Option<Node>;
        let float = self.cx.dialect().float;
        match s.t {
            T::Preprocessor => {
                self.expect_t(T::ParentOpen)?;
                let pv = s.text();
                let two = |p: &mut Self| -> R<(Node, Node)> {
                    let a = p.expression(env, false, false)?.ok_or_else(|| p.error("Expression expected"))?;
                    p.expect_t(T::Comma)?;
                    let b = p.expression(env, false, false)?.ok_or_else(|| p.error("Expression expected"))?;
                    Ok((a, b))
                };
                ret = Some(match pv.as_str() {
                    "hasnext" => {
                        let (index, obj) = two(self)?;
                        node(Item::HasNext { index, obj })
                    }
                    "newactivation" => node(Item::NewActivation),
                    "nextname" => {
                        let (index, obj) = two(self)?;
                        allow_member = true;
                        node(Item::NextName { index, obj })
                    }
                    "nextvalue" => {
                        let (index, obj) = two(self)?;
                        allow_member = true;
                        node(Item::NextValue { index, obj })
                    }
                    "dup" => {
                        let e = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                        allow_member = true;
                        node(Item::Dup(e))
                    }
                    "push" => node(Item::Push(self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?)),
                    "pop" => {
                        allow_member = true;
                        node(Item::Pop)
                    }
                    "swap" => node(Item::Swap),
                    "goto" | "multiname" => return self.err(format!("Compiling \u{a7}\u{a7}{pv} is not available, sorry")),
                    _ => return self.err(format!("Unknown preprocessor instruction: \u{a7}\u{a7}{pv}")),
                });
                self.expect_t(T::ParentClose)?;
            }
            T::Regexp => {
                let p: String = s.text().chars().skip(1).collect();
                let spos = p.rfind('/').unwrap_or(p.len());
                let modifier = p[(spos + 1).min(p.len())..].to_string();
                ret = Some(node(Item::RegExp { pattern: p[..spos].to_string(), modifier }));
                allow_member = true;
            }
            T::XmlStartTagBegin | T::XmlStartVarTagBegin | T::XmlCData | T::XmlComment => {
                self.pushback(s);
                ret = Some(self.xml(env)?);
            }
            T::String => {
                ret = Some(node(Item::Str(s.text())));
                allow_member = true;
            }
            T::Negate => {
                let v = self.expression_primary(env, false)?.ok_or_else(|| self.error("Missing operand"))?;
                ret = Some(node(Item::Un { op: Un::BitNot, v }));
            }
            T::Plus => {
                let v = self.expression_primary(env, true)?.ok_or_else(|| self.error("Missing operand"))?;
                ret = Some(if float { node(Item::Un { op: Un::UnPlus, v }) } else { node(Item::Coerce { v, ty: ty_parts(&["Number"]) }) });
            }
            T::Minus => {
                s = self.lex()?;
                if s.t == T::Double {
                    let V::Double(d) = s.v else { return self.err("Number expected") };
                    ret = Some(node(Item::Double(-d)));
                } else if s.t == T::Integer {
                    ret = Some(node(Item::Int(int_of(&s).wrapping_neg())));
                } else if s.t == T::Float {
                    let V::Float(f) = s.v else { return self.err("Number expected") };
                    ret = Some(node(Item::Float(-f)));
                } else if s.t == T::Decimal {
                    return self.err("The ABC has no decimal support");
                } else {
                    self.pushback(s);
                    let num = self.expression_primary(env, true)?.ok_or_else(|| self.error("Missing operand"))?;
                    let neg = {
                        let mut b = num.borrow_mut();
                        match &mut *b {
                            Item::Int(v) => {
                                *v = v.wrapping_neg();
                                true
                            }
                            Item::Double(d) => {
                                *d = if d.is_infinite() { f64::NEG_INFINITY } else { -*d };
                                true
                            }
                            _ => false,
                        }
                    };
                    ret = Some(if neg { num } else { node(Item::Un { op: Un::Neg, v: num }) });
                }
            }
            T::TypeOf => {
                let v = self.expression_primary(env, false)?.ok_or_else(|| self.error("Missing operand"))?;
                ret = Some(node(Item::Un { op: Un::TypeOf, v }));
            }
            T::True => ret = Some(node(Item::Bool(true))),
            T::Null => ret = Some(node(Item::Null)),
            T::Undefined => ret = Some(node(Item::Undefined)),
            T::False => ret = Some(node(Item::Bool(false))),
            T::CurlyOpen => {
                s = self.lex()?;
                let mut nvs = Vec::new();
                while s.t != T::CurlyClose {
                    if s.t != T::Comma {
                        self.pushback(s);
                    }
                    s = self.lex()?;
                    self.expected(&s, &[E::G(Group::Identifier), E::T(T::String), E::T(T::Integer), E::T(T::Double), E::T(T::ParentOpen)])?;
                    let n = if s.t == T::ParentOpen {
                        let e = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                        self.expect_t(T::ParentClose)?;
                        e
                    } else {
                        node(Item::Str(s.text()))
                    };
                    self.expect_t(T::Colon)?;
                    let v = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    nvs.push((n, v));
                    s = self.lex()?;
                    if !s.is(&[T::Comma, T::CurlyClose]) {
                        self.expected(&s, &[E::T(T::Comma), E::T(T::CurlyClose)])?;
                    }
                }
                ret = Some(node(Item::NewObject(nvs)));
                allow_member = true;
            }
            T::BracketOpen => {
                self.pushback(s);
                let mut items = Vec::new();
                self.brackets(env, &mut items)?;
                ret = Some(node(Item::NewArray(items)));
                allow_member = true;
            }
            T::Function => {
                s = self.lex()?;
                let mut fname = String::new();
                if s.group == Group::Identifier {
                    fname = s.text();
                } else {
                    self.pushback(s);
                }
                env.needs_act.set(true);
                let f = self.function(env, Vec::new(), env.pkg.clone(), false, false, &fname, false)?;
                ret = Some(node(Item::Function(f)));
                allow_member = true;
            }
            T::Infinity => ret = Some(node(Item::Double(f64::INFINITY))),
            T::Integer => ret = Some(node(Item::Int(int_of(&s)))),
            T::Double => {
                let V::Double(d) = s.v else { return self.err("Number expected") };
                ret = Some(node(Item::Double(d)));
                allow_member = true;
            }
            T::Decimal => return self.err("The ABC has no decimal support"),
            T::Float => {
                if !float {
                    return self.err("The ABC has no float support");
                }
                let V::Float(f) = s.v else { return self.err("Number expected") };
                ret = Some(node(Item::Float(f)));
                allow_member = true;
            }
            T::Float4 => {
                if !float {
                    let t = self.lx.yytext();
                    let rest: String = t.chars().skip(6).collect();
                    self.lx.pushback_str(&rest, State::Initial, 6);
                    self.pushback(Sym { pos: s.pos, group: Group::Identifier, t: T::Identifier, v: V::Str("float4".into()) });
                    ret = Some(self.name(env, false)?);
                } else {
                    let V::Float4(f) = s.v else { return self.err("Number expected") };
                    ret = Some(node(Item::Float4(f)));
                }
                allow_member = true;
            }
            T::Delete => {
                let v = self.expression_primary(env, true)?.ok_or_else(|| self.error("Missing operand"))?;
                if !is_name_or_prop(&v) {
                    return self.err("Not a property or name");
                }
                ret = Some(node(Item::Delete { obj: v, line: self.line() }));
            }
            T::Increment | T::Decrement => {
                let v = self.expression_primary(env, false)?.ok_or_else(|| self.error("Missing operand"))?;
                if !is_name_or_prop(&v) {
                    return self.err("Not a property or name");
                }
                let ty = ty_parts(&["Number"]);
                ret = Some(node(if s.t == T::Increment { Item::PreInc { v, ty } } else { Item::PreDec { v, ty } }));
            }
            T::Not => {
                let v = self.expression_primary(env, false)?.ok_or_else(|| self.error("Missing operand"))?;
                ret = Some(node(Item::Un { op: Un::Not, v }));
            }
            T::ParentOpen => {
                let e = self.expression(env, false, true)?;
                self.expect_t(T::ParentClose)?;
                let Some(e) = e else { return self.err("Expression in parenthesis expected") };
                ret = Some(node(Item::Paren(e)));
                allow_member = true;
            }
            T::New => {
                s = self.lex()?;
                if s.t == T::XmlStartTagBegin {
                    let rest: String = s.text().chars().skip(1).collect();
                    self.lx.pushback_str(&rest, State::Initial, 1);
                    s = Sym { pos: s.pos, group: Group::Operator, t: T::LowerThan, v: V::Str("<".into()) };
                }
                let line = self.line();
                if s.t == T::Function {
                    s = self.lex()?;
                    let mut fname = String::new();
                    if s.group == Group::Identifier {
                        fname = s.text();
                    } else {
                        self.pushback(s);
                    }
                    env.needs_act.set(true);
                    let f = self.function(env, Vec::new(), env.pkg.clone(), false, false, &fname, false)?;
                    ret = Some(node(Item::Function(f)));
                } else if s.t == T::LowerThan {
                    let subtype = self.type_(env)?;
                    self.expect_t(T::GreaterThan)?;
                    s = self.lex()?;
                    self.expected(&s, &[E::T(T::BracketOpen)])?;
                    self.pushback(s);
                    let mut params = Vec::new();
                    self.brackets(env, &mut params)?;
                    ret = Some(node(Item::InitVector { subtype, args: params, opened: env.opened.clone() }));
                } else if s.t == T::ParentOpen {
                    let v = self.expression(env, false, false)?.ok_or_else(|| self.error("Expression expected"))?;
                    let v = self.apply_type(env, v)?;
                    self.expect_t(T::ParentClose)?;
                    self.expect_t(T::ParentOpen)?;
                    let args = self.call(env)?;
                    ret = Some(node(Item::Construct(Call { name: v, args, line, opened: env.opened.clone(), this_type: env.this_type.clone() })));
                } else {
                    self.pushback(s);
                    let v = self.name(env, false)?;
                    let v = self.apply_type(env, v)?;
                    self.expect_t(T::ParentOpen)?;
                    let line = self.line();
                    let args = self.call(env)?;
                    ret = Some(node(Item::Construct(Call { name: v, args, line, opened: env.opened.clone(), this_type: env.this_type.clone() })));
                }
                allow_member = true;
            }
            T::Identifier | T::This | T::Super | T::Attribute | T::Public | T::Private | T::Protected | T::Internal => {
                self.pushback(s);
                ret = Some(self.name(env, false)?);
                allow_member = true;
            }
            _ => {
                if s.group == Group::Identifier {
                    self.pushback(s);
                    ret = Some(self.name(env, false)?);
                    allow_member = true;
                } else {
                    self.pushback(s);
                    ret = None;
                }
            }
        }
        match ret {
            Some(r) if allow_member => Ok(Some(self.member_or_call(env, r)?)),
            r => Ok(r),
        }
    }

    fn parse_imports_usages(&mut self, imported: &Imports, opened: &NsList) -> R<bool> {
        let mut is_empty = true;
        let mut s = self.lex()?;
        while s.is(&[T::Import, T::Use]) {
            if s.t == T::Import {
                is_empty = false;
                s = self.lex()?;
                self.expected(&s, &[E::G(Group::Identifier)])?;
                let mut full = Dotted::default().add(&s.text(), "");
                s = self.lex()?;
                let mut star = false;
                while s.t == T::Dot {
                    s = self.lex()?;
                    if s.t == T::Multiply {
                        star = true;
                        s = self.lex()?;
                        break;
                    }
                    self.expected(&s, &[E::G(Group::Identifier)])?;
                    full = full.add(&s.text(), "");
                    s = self.lex()?;
                }
                if star {
                    opened.borrow_mut().push(ns_item(full, ns_kind::PACKAGE as i32, 0));
                } else {
                    imported.borrow_mut().push(full);
                }
                self.expected(&s, &[E::T(T::Semicolon)])?;
            } else {
                is_empty = false;
                loop {
                    s = self.lex()?;
                    if s.t == T::Namespace {
                        s = self.lex()?;
                        self.expected(&s, &[E::G(Group::Identifier)])?;
                        let mut full = Dotted::default().add(&s.text(), "");
                        s = self.lex()?;
                        while s.t == T::Dot {
                            s = self.lex()?;
                            if s.t == T::Multiply {
                                s = self.lex()?;
                                break;
                            }
                            self.expected(&s, &[E::G(Group::Identifier)])?;
                            full = full.add(&s.text(), "");
                            s = self.lex()?;
                        }
                        self.pushback(s);
                        opened.borrow_mut().push(ns_item(full, KIND_NAMESPACE_CUSTOM, self.line()));
                    } else {
                        return self.err("Invalid use kind");
                    }
                    s = self.lex()?;
                    if s.t != T::Comma {
                        break;
                    }
                }
                self.expected(&s, &[E::T(T::Semicolon)])?;
            }
            s = self.lex()?;
        }
        self.pushback(s);
        Ok(!is_empty)
    }

    /// `scriptTraitsFromString`: the parsed script plus what code generation needs.
    pub fn parse_script(&mut self, file_name: &str, script_index: usize) -> R<Script> {
        let mut file = file_name.to_string();
        if let Some(p) = file.rfind('/') {
            file = file[p + 1..].to_string();
        }
        if let Some(p) = file.rfind('\\') {
            file = file[p + 1..].to_string();
        }
        let imported: Imports = Rc::new(RefCell::new(Vec::new()));
        let opened = ns_list(Vec::new());
        let all_ns: AllNs = Rc::new(RefCell::new(Vec::new()));
        let sinit_act = Rc::new(Cell::new(false));
        let sinit_variables = vars();
        let mut pkg = None;
        let mut items = Vec::new();
        let mut loops = Loops::default();
        while self.script_traits_block(&imported, &opened, &all_ns, script_index, &file, &mut items, &sinit_act, &mut loops, &sinit_variables, &mut pkg)? {}
        if self.lex()?.t != T::Eof {
            return self.err("Parsing finished before end of the file");
        }
        let sv = sinit_variables.borrow().clone();
        Ok(Script { items, imported, opened, all_ns, sinit_activation: sinit_act.get(), sinit_variables: sv, pkg })
    }
}

enum CmdOut {
    Done(Option<Node>),
    Ret(Option<Node>),
}

pub struct Script {
    pub items: Nodes,
    pub imported: Imports,
    pub opened: NsList,
    pub all_ns: AllNs,
    pub sinit_activation: bool,
    pub sinit_variables: Nodes,
    pub pkg: Option<Dotted>,
}

fn put(v: &mut Vec<(String, String)>, k: String, val: String) {
    match v.iter_mut().find(|(x, _)| *x == k) {
        Some(e) => e.1 = val,
        None => v.push((k, val)),
    }
}

fn is_name_or_prop(n: &Node) -> bool {
    matches!(&*n.borrow(), Item::Unresolved(_) | Item::Name(_) | Item::Property(_) | Item::Index(_))
}

fn name_slot(n: &Node) -> (i64, i64) {
    match &*n.borrow() {
        Item::Name(x) => (x.slot, x.slot_scope),
        _ => (-1, 0),
    }
}

/// `UnresolvedAVM2Item.setSlotNumber/setSlotScope`: applies to what it resolved to.
pub fn set_unresolved_slot(u: &Node, slot: i64, scope: i64) {
    let r = match &*u.borrow() {
        Item::Unresolved(x) => x.resolved.clone(),
        _ => None,
    };
    if let Some(r) = r {
        if let Item::Name(n) = &mut *r.borrow_mut() {
            n.slot = slot;
            n.slot_scope = scope;
        }
    }
}

fn add_s(rets: &mut Nodes, sb: &mut String) {
    if sb.is_empty() {
        return;
    }
    if let Some(last) = rets.last() {
        if let Item::Str(v) = &mut *last.borrow_mut() {
            v.push_str(sb);
            sb.clear();
            return;
        }
    }
    rets.push(node(Item::Str(std::mem::take(sb))));
}

/// `add(List)`: concatenation, folding adjacent strings.
fn add_all(parts: Nodes) -> Option<Node> {
    let mut it = parts.into_iter();
    let mut o = it.next()?;
    for p in it {
        let folded = match (&*o.borrow(), &*p.borrow()) {
            (Item::Str(a), Item::Str(b)) => Some(format!("{a}{b}")),
            _ => None,
        };
        o = match folded {
            Some(s) => node(Item::Str(s)),
            None => node(Item::Bin { op: Op::Add, l: o, r: p }),
        };
    }
    Some(o)
}
