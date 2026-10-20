//! `ActionScript2Parser`: AS1/2 source to model items.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::lexer::{Group, Lexer, Sym, T, V, NOPRECEDENCE};
use super::*;
use crate::action::decomp::print::PROPERTY_NAMES;

const BUILTIN_CASTS: &[&str] = &[
    "flash.display.BitmapData", "flash.external.ExternalInterface", "flash.filters.BevelFilter", "flash.filters.BitmapFilter",
    "flash.filters.BlurFilter", "flash.filters.ColorMatrixFilter", "flash.filters.ConvolutionFilter",
    "flash.filters.DisplacementMapFilter", "flash.filters.DropShadowFilter", "flash.filters.GlowFilter",
    "flash.filters.GradientBevelFilter", "flash.filters.GradientGlowFilter", "flash.geom.ColorTransform", "flash.geom.Matrix",
    "flash.geom.Point", "flash.geom.Rectangle", "flash.geom.Transform", "flash.net.FileReference", "flash.net.FileReferenceList",
    "flash.text.TextRenderer", "Button", "Camera", "Color", "ContextMenu", "ContextMenuItem", "CustomActions", "Error",
    "System.IME", "LoadVars", "LocalConnection", "Microphone", "MovieClip", "MovieClipLoader", "NetConnection", "NetStream",
    "PrintJob", "System.security", "SharedObject", "Sound", "TextField", "TextFormat", "TextSnapshot", "XML", "XMLNode",
    "XMLSocket", "XMLUI",
];

/// The constant pool shared by the parser and the generator.
pub struct Pool {
    pub list: Vec<String>,
    pub len: usize,
    pub cs: Charset,
    pub version: u8,
}

impl Pool {
    fn size_of(&self, s: &str) -> usize {
        self.cs.byte_len(s) + 1
    }

    /// `pushConstTargetItem`.
    pub fn konst(&mut self, s: &str) -> Val {
        if self.version < 5 {
            return Val::Str(s.into());
        }
        if let Some(i) = self.list.iter().position(|x| x == s) {
            return Val::Const(i as u16);
        }
        let n = self.size_of(s);
        if self.list.len() < 0xffff && self.len + n <= 0xffff {
            self.list.push(s.into());
            self.len += n;
            return Val::Const((self.list.len() - 1) as u16);
        }
        Val::Str(s.into())
    }

    /// A string value, resolving constants.
    pub fn string(&self, v: &Val) -> Option<String> {
        match v {
            Val::Str(s) => Some(s.clone()),
            Val::Const(i) => self.list.get(*i as usize).cloned(),
            _ => None,
        }
    }
}

pub type PoolRef = Rc<RefCell<Pool>>;

pub struct Parsed {
    pub tree: Vec<It>,
    pub pool: PoolRef,
    pub events: Option<Events>,
}

pub struct Parser<'a> {
    lx: Lexer,
    version: u8,
    pool: PoolRef,
    classes: &'a [String],
    swf: Option<&'a Swf>,
    replacements: HashMap<String, String>,
}

type Vars = Vec<VarRef>;
type Funcs = Vec<FuncRef>;

fn err<T>(line: usize, message: impl Into<String>) -> R<T> {
    Err(CompileError { line, message: message.into() })
}

fn var(name: &str, store: Option<It>, definition: bool) -> VarRef {
    Rc::new(RefCell::new(Var { name: name.into(), store, definition, boxed: None, resolved: None }))
}

fn b(it: It) -> B {
    Box::new(it)
}

fn direct_str(it: &It, pool: &Pool) -> Option<String> {
    match it {
        It::Direct(v) => pool.string(v),
        _ => None,
    }
}

/// Every item under `it`, `it` included, as FFDec's `visitRecursively`.
pub fn walk(it: &mut It, f: &mut dyn FnMut(&mut It)) {
    f(it);
    children(it, &mut |c| walk(c, f));
}

/// Direct sub-items, as FFDec's `visit`.
pub fn children(it: &mut It, f: &mut dyn FnMut(&mut It)) {
    let all = |v: &mut Vec<It>, f: &mut dyn FnMut(&mut It)| v.iter_mut().for_each(|c| f(c));
    match it {
        It::Var(v) => {
            let v = v.clone();
            let mut v = v.borrow_mut();
            if let Some(s) = &mut v.store {
                f(s);
            }
        }
        It::Function(fun) => {
            let fun = fun.clone();
            let mut fun = fun.borrow_mut();
            if let Some(body) = &mut fun.body {
                all(body, f);
            }
        }
        It::GetVariable(x) | It::Op1 { value: x, .. } | It::Not(x) | It::Paren(x) | It::PreInc(x) | It::PreDec(x) | It::PostInc(x)
        | It::PostDec(x) | It::Trace(x) | It::RemoveSprite(x) | It::UnloadMovie(x) | It::UnloadMovieNum(x) | It::Enumerate(x)
        | It::Push(x) | It::Throw(x) | It::StoreRegister { value: x, .. } => f(x),
        It::SetVariable { name, value, .. } => {
            f(name);
            f(value);
        }
        It::DefineLocal { name, value } => {
            f(name);
            if let Some(v) = value {
                f(v);
            }
        }
        It::GetMember { obj, member, .. } => {
            f(obj);
            f(member);
        }
        It::SetMember { obj, member, value, .. } => {
            f(obj);
            f(member);
            f(value);
        }
        It::GetProperty { target, .. } => f(target),
        It::SetProperty { target, value, .. } => {
            f(target);
            f(value);
        }
        It::CallFunction { name, args } => {
            f(name);
            all(args, f);
        }
        It::CallMethod { obj, name, args, .. } | It::NewMethod { obj, name, args, .. } => {
            f(obj);
            f(name);
            all(args, f);
        }
        It::NewObject { name, args, .. } => {
            f(name);
            all(args, f);
        }
        It::Delete { obj, name, .. } => {
            if let Some(o) = obj {
                f(o);
            }
            f(name);
        }
        It::Bin { l, r, .. } | It::And(l, r) | It::Or(l, r) => {
            f(l);
            f(r);
        }
        It::Ternary(a, x, y) => {
            f(a);
            f(x);
            f(y);
        }
        It::Comma(v) | It::InitArray(v) | It::FsCommand2(v) | It::Block(v) => all(v, f),
        It::InitObject { names, values } => {
            all(names, f);
            all(values, f);
        }
        It::GetUrl2 { url, target, .. } | It::Load { url, target, .. } => {
            f(url);
            f(target);
        }
        It::GotoFrame2 { frame, .. } => f(frame),
        It::FsCommand { cmd, param } => {
            f(cmd);
            if let Some(p) = param {
                f(p);
            }
        }
        It::CloneSprite { src, target, depth } => {
            f(src);
            f(target);
            f(depth);
        }
        It::StartDrag { target, lock, constrain, x1, y1, x2, y2 } => {
            f(target);
            f(lock);
            f(constrain);
            for x in [x1, y1, x2, y2].into_iter().flatten() {
                f(x);
            }
        }
        It::Print { target, bbox, .. } => {
            f(target);
            f(bbox);
        }
        It::Extract { value, index, count, .. } => {
            f(value);
            f(index);
            f(count);
        }
        It::CastOp { ctor, obj } => {
            f(ctor);
            f(obj);
        }
        It::If { cond, on_true, on_false, .. } => {
            f(cond);
            all(on_true, f);
            if let Some(o) = on_false {
                all(o, f);
            }
        }
        It::While { cond, body, .. } | It::DoWhile { body, cond, .. } => {
            all(cond, f);
            all(body, f);
        }
        It::For { first, cond, last, body, .. } => {
            all(first, f);
            f(cond);
            all(last, f);
            all(body, f);
        }
        It::ForIn { collection, body, .. } => {
            f(collection);
            all(body, f);
        }
        It::Switch { expr, values, cmds, .. } => {
            f(expr);
            values.iter_mut().flatten().for_each(|v| f(v));
            cmds.iter_mut().for_each(|c| all(c, f));
        }
        It::Return(x) => {
            if let Some(x) = x {
                f(x);
            }
        }
        It::Try { body, catches, finally, .. } => {
            all(body, f);
            catches.iter_mut().for_each(|c| all(c, f));
            if let Some(fi) = finally {
                all(fi, f);
            }
        }
        It::With { scope, body } => {
            f(scope);
            all(body, f);
        }
        It::TellTarget { target, body, .. } => {
            f(target);
            all(body, f);
        }
        It::IfFrameLoaded { frame, body, .. } => {
            f(frame);
            all(body, f);
        }
        It::Class { traits, .. } => traits.iter_mut().for_each(|(_, v)| f(v)),
        _ => {}
    }
}

/// `@identifier name = "value"` tags in doc comments, which rename identifiers.
fn doc_replacements(text: &str) -> R<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut rest = text;
    while let Some(start) = rest.find("/**") {
        let body = &rest[start + 3..];
        let end = body.find("*/").unwrap_or(body.len());
        for line in body[..end].lines() {
            let line = line.trim().trim_start_matches('*').trim();
            if let Some(tag) = line.strip_prefix("@identifier") {
                let tag = tag.trim();
                if tag.is_empty() {
                    continue;
                }
                let mut lx = Lexer::new(tag, true);
                let (a, e, c) = (lx.lex()?, lx.lex()?, lx.lex()?);
                if a.t != T::Identifier {
                    return err(0, "Invalid @identifier AsDoc tag value. Identifier expected.");
                }
                if e.t != T::Assign {
                    return err(0, "Invalid @identifier AsDoc tag value. Assign expected.");
                }
                if c.t != T::String {
                    return err(0, "Invalid @identifier AsDoc tag value. String expected.");
                }
                out.insert(a.text(), c.text());
            }
        }
        rest = &body[end..];
    }
    Ok(out)
}

const KEYNAMES: [Option<&str>; 33] = [
    None, Some("<Left>"), Some("<Right>"), Some("<Home>"), Some("<End>"), Some("<Insert>"), Some("<Delete>"), None,
    Some("<Backspace>"), None, None, None, None, Some("<Enter>"), Some("<Up>"), Some("<Down>"), Some("<PageUp>"),
    Some("<PageDown>"), Some("<Tab>"), Some("<Escape>"), None, None, None, None, None, None, None, None, None, None, None,
    None, Some("<Space>"),
];

fn string_to_key(s: &str) -> Option<u32> {
    if let Some(i) = KEYNAMES.iter().position(|k| *k == Some(s)) {
        return Some(i as u32);
    }
    let mut c = s.chars();
    match (c.next(), c.next()) {
        (Some(ch), None) => Some(ch as u32),
        _ => None,
    }
}

fn property_index(name: &str) -> Option<i64> {
    let lower = name.to_lowercase();
    PROPERTY_NAMES.iter().position(|p| p.to_lowercase() == lower).map(|i| i as i64)
}

impl<'a> Parser<'a> {
    pub fn run(text: &str, version: u8, target: Target, classes: &'a [String], swf: Option<&'a Swf>, pool: Vec<String>) -> R<Parsed> {
        let cs = Charset::for_version(version);
        let len = 2 + pool.iter().map(|s| cs.byte_len(s) + 1).sum::<usize>();
        let pool = Rc::new(RefCell::new(Pool { list: pool, len, cs, version }));
        let mut p = Parser { lx: Lexer::new(text, version >= 7), version, pool: pool.clone(), classes, swf, replacements: doc_replacements(text)? };
        let events = p.header(target)?;
        let mut vars: Vars = Vec::new();
        let mut funcs: Funcs = Vec::new();
        let mut has_eval = false;
        let tree = p.commands(false, false, false, 0, false, &mut vars, &mut funcs, &mut has_eval)?;
        for v in &vars {
            p.box_top(v);
        }
        if target != Target::Actions {
            p.expected_type(&[T::CurlyClose])?;
        }
        if p.lex()?.t != T::Eof {
            return err(p.lx.line, "Parsing finished before end of the file");
        }
        Ok(Parsed { tree, pool, events })
    }

    fn line(&self) -> usize {
        self.lx.line
    }

    fn konst(&self, s: &str) -> It {
        It::Direct(self.pool.borrow_mut().konst(s))
    }

    fn box_top(&self, v: &VarRef) {
        if box_resolved(v) {
            return;
        }
        let (name, stored, definition) = {
            let b = v.borrow();
            (b.name.clone(), b.store.clone(), b.definition)
        };
        let prop = property_index(&name);
        let boxed = if definition {
            It::DefineLocal { name: b(self.konst(&name)), value: stored.map(b) }
        } else if let Some(s) = stored {
            match prop {
                Some(i) => It::SetProperty { target: b(self.konst("")), index: i, value: b(s) },
                None => It::SetVariable { name: b(self.konst(&name)), value: b(s), force_set: false },
            }
        } else {
            match prop {
                Some(i) => It::GetProperty { target: b(self.konst("")), index: i },
                None => It::GetVariable(b(self.konst(&name))),
            }
        };
        v.borrow_mut().boxed = Some(boxed);
    }

    /// `on(...)`/`onClipEvent(...)` before the body of a button condition or clip event.
    fn header(&mut self, target: Target) -> R<Option<Events>> {
        if target == Target::Actions {
            return Ok(None);
        }
        let s = self.lex()?;
        let word = s.text();
        let ok = s.t == T::Identifier && (word == "on" || (target == Target::ClipAction && word == "onClipEvent"));
        if !ok {
            let what = if target == Target::ButtonCond { "on" } else { "on or onClipEvent" };
            return err(self.line(), format!("{what} keyword expected but {:?} found", s.t));
        }
        self.expected_type(&[T::ParentOpen])?;
        let mut ev = Events::default();
        if word == "on" {
            let mut s = self.lex()?;
            let mut empty = true;
            while s.t == T::Identifier {
                empty = false;
                let name = s.text();
                let clip = |n: u32| Some(1u32 << (31 - n));
                let bit: Option<u32> = if target == Target::ButtonCond {
                    match name.as_str() {
                        "press" => Some(0x0400),
                        "release" => Some(0x0800),
                        "releaseOutside" => Some(0x4000),
                        "rollOver" => Some(0x0100),
                        "rollOut" => Some(0x0200),
                        "dragOut" => Some(0x1000),
                        "dragOver" => Some(0x2000),
                        "keyPress" => None,
                        _ => return err(self.line(), "Unrecognized event type"),
                    }
                } else {
                    match name.as_str() {
                        "press" => clip(13),
                        "release" => clip(12),
                        "releaseOutside" => clip(11),
                        "rollOver" => clip(10),
                        "rollOut" => clip(9),
                        "dragOut" => clip(23),
                        "dragOver" => clip(8),
                        "initialize" => clip(14),
                        "construct" => clip(21),
                        "keyPress" => None,
                        _ => return err(self.line(), "Unrecognized event type"),
                    }
                };
                match bit {
                    Some(bit) => ev.flags |= bit,
                    None => {
                        let k = self.lex()?;
                        if k.t != T::String {
                            return err(self.line(), format!("String expected but {:?} found", k.t));
                        }
                        ev.key = string_to_key(&k.text()).ok_or_else(|| CompileError { line: self.line(), message: "Invalid key".into() })?;
                        if target == Target::ClipAction {
                            ev.flags |= 1 << (31 - 22);
                        }
                    }
                }
                s = self.lex()?;
                if s.t == T::ParentClose {
                    break;
                }
                self.expected(&s, &[T::Comma])?;
                s = self.lex()?;
            }
            self.expected(&s, &[T::ParentClose])?;
            if empty {
                return err(self.line(), "condition must be non empty");
            }
        } else {
            let s = self.lex()?;
            if s.t != T::Identifier {
                return err(self.line(), format!("Identifier expected but {:?} found", s.t));
            }
            let n = match s.text().as_str() {
                "keyUp" => 0,
                "keyDown" => 1,
                "mouseUp" => 2,
                "mouseDown" => 3,
                "mouseMove" => 4,
                "unload" => 5,
                "enterFrame" => 6,
                "load" => 7,
                "data" => 15,
                _ => return err(self.line(), "Unrecognized clipEvent type"),
            };
            ev.flags |= 1 << (31 - n);
            self.expected_type(&[T::ParentClose])?;
        }
        self.expected_type(&[T::CurlyOpen])?;
        Ok(Some(ev))
    }

    fn lex(&mut self) -> R<Sym> {
        let mut s = self.lx.lex()?;
        if s.t == T::Identifier {
            if let Some(r) = self.replacements.get(&s.text()) {
                s.v = V::Str(r.clone());
            }
        }
        Ok(s)
    }

    fn peek(&mut self) -> R<Sym> {
        let s = self.lex()?;
        self.lx.pushback(s.clone());
        Ok(s)
    }

    fn expected(&self, s: &Sym, ts: &[T]) -> R<()> {
        if ts.contains(&s.t) {
            return Ok(());
        }
        let names: Vec<String> = ts.iter().map(|t| format!("{t:?}")).collect();
        err(self.line(), format!("{} expected but {:?} found", names.join(" or "), s.t))
    }

    fn expected_type(&mut self, ts: &[T]) -> R<Sym> {
        let s = self.lex()?;
        self.expected(&s, ts)?;
        Ok(s)
    }

    fn is_identifier(s: &Sym, also: &[T]) -> bool {
        also.contains(&s.t)
            || matches!(s.t, T::Identifier | T::True | T::False | T::Get | T::Set | T::Each | T::NumberOp | T::StringOp)
            || matches!(s.group, Group::GlobalConst | Group::GlobalFunc)
    }

    fn expected_identifier(&self, s: &Sym, also: &[T]) -> R<()> {
        if Self::is_identifier(s, also) {
            return Ok(());
        }
        err(self.line(), format!("IDENTIFIER expected but {:?} found", s.t))
    }

    fn version_required(&self, s: &Sym, min: u8) -> R<()> {
        if self.version < min {
            return err(self.line(), format!("{} requires at least SWF version {min}", s.text()));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn commands(&mut self, in_with: bool, in_fn: bool, in_method: bool, forin: i32, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<Vec<It>> {
        let mut out = Vec::new();
        while let Some(c) = self.command(in_with, in_fn, in_method, forin, in_tell, vars, funcs, eval)? {
            out.push(c);
        }
        Ok(out)
    }

    fn type_name(&mut self, vars: &mut Vars) -> R<It> {
        let s = self.lex()?;
        self.expected_identifier(&s, &[])?;
        let v = var(&s.text(), None, false);
        vars.push(v.clone());
        let mut ret = It::Var(v);
        let mut s = self.lex()?;
        while s.t == T::Dot {
            s = self.lex()?;
            self.expected_identifier(&s, &[])?;
            ret = It::GetMember { obj: b(ret), member: b(self.konst(&s.text())), getter: false };
            s = self.lex()?;
        }
        self.lx.pushback(s);
        Ok(ret)
    }

    fn call(&mut self, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<Vec<It>> {
        let mut out = Vec::new();
        let mut s = self.lex()?;
        while s.t != T::ParentClose {
            if s.t != T::Comma {
                self.lx.pushback(s);
            }
            out.push(self.expression_req(in_with, in_fn, in_method, in_tell, true, vars, funcs, false, eval)?);
            s = self.lex()?;
            self.expected(&s, &[T::Comma, T::ParentClose])?;
        }
        Ok(out)
    }

    fn function(&mut self, with_body: bool, name: &str, is_method: bool, vars: &mut Vars, funcs: &mut Funcs, in_tell: bool, eval: &mut bool) -> R<FuncRef> {
        let line = self.line();
        self.expected_type(&[T::ParentOpen])?;
        let mut s = self.lex()?;
        let mut params = Vec::new();
        while s.t != T::ParentClose {
            if s.t != T::Comma {
                self.lx.pushback(s);
            }
            s = self.lex()?;
            self.expected_identifier(&s, &[])?;
            params.push(s.text());
            s = self.lex()?;
            if s.t == T::Colon {
                self.type_name(vars)?;
                s = self.lex()?;
            }
            if !s.is(&[T::Comma, T::ParentClose]) {
                self.expected(&s, &[T::Comma, T::ParentClose])?;
            }
        }
        s = self.lex()?;
        if s.t == T::Colon {
            self.type_name(&mut Vec::new())?;
        } else {
            self.lx.pushback(s);
        }
        let mut sub_vars = Vec::new();
        let mut sub_funcs = Vec::new();
        let mut sub_eval = false;
        let mut body = None;
        if with_body {
            self.expected_type(&[T::CurlyOpen])?;
            body = Some(self.commands(false, true, is_method, 0, in_tell, &mut sub_vars, &mut sub_funcs, &mut sub_eval)?);
            self.expected_type(&[T::CurlyClose])?;
        }
        if sub_eval {
            *eval = true;
        }
        let f = Rc::new(RefCell::new(Func {
            name: name.into(),
            params,
            body,
            vars: sub_vars,
            inner: sub_funcs,
            has_eval: sub_eval,
            calc_name: None,
            is_getter: false,
            is_setter: false,
            line,
        }));
        funcs.push(f.clone());
        Ok(f)
    }

    /// `getMembersPath`.
    fn members_path(&self, it: &It) -> Option<Vec<String>> {
        let mut out = Vec::new();
        let mut cur = it;
        let pool = self.pool.borrow();
        while let It::GetMember { obj, member, .. } = cur {
            out.insert(0, direct_str(member, &pool)?);
            cur = obj;
        }
        match cur {
            It::GetVariable(n) => out.insert(0, direct_str(n, &pool)?),
            It::Var(v) => out.insert(0, v.borrow().name.clone()),
            _ => return None,
        }
        Some(out)
    }

    /// Getter and setter names of a superclass compiled in this SWF (`fetchGettersSetters`).
    fn fetch_getters_setters(&self, parts: Option<Vec<String>>, gets: &mut HashSet<String>, sets: &mut HashSet<String>, depth: usize) {
        let (Some(mut parts), Some(swf)) = (parts, self.swf) else { return };
        if depth > 32 || parts.is_empty() {
            return;
        }
        let full = parts.join(".");
        let cls = parts.pop().unwrap();
        let mut path = String::from("__Packages/");
        for p in &parts {
            path.push_str(p);
            path.push('/');
        }
        path.push_str(&cls);
        let Some((_, src)) = crate::action::scripts::export_paths(&crate::action::scripts::tree(swf)).into_iter().find(|(n, _)| *n == path) else {
            return;
        };
        let uninit = Default::default();
        let Ok(tree) = crate::action::decomp::actions_to_tree(&src.actions(swf), true, &path, &Rc::new(uninit)) else { return };
        for (name, traits, extends) in crate::action::decomp::class_traits(&tree) {
            if name == full {
                for t in traits {
                    if let Some(p) = t.strip_prefix("__set__") {
                        sets.insert(p.into());
                    }
                    if let Some(p) = t.strip_prefix("__get__") {
                        gets.insert(p.into());
                    }
                }
            }
            if let Some(e) = extends {
                self.fetch_getters_setters(Some(e.split('.').map(String::from).collect()), gets, sets, depth + 1);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn traits(&mut self, is_interface: bool, name: It, extends: Option<It>, implements: Vec<It>, vars: &mut Vars, funcs: &mut Funcs, in_tell: bool, eval: &mut bool) -> R<It> {
        let mut super_gets = HashSet::new();
        let mut super_sets = HashSet::new();
        if let Some(e) = &extends {
            self.fetch_getters_setters(self.members_path(e), &mut super_gets, &mut super_sets, 0);
        }
        let mut this_gets = super_gets.clone();
        let mut this_sets = super_sets.clone();
        let mut traits: Vec<(It, It)> = Vec::new();
        let mut statics: Vec<bool> = Vec::new();
        let mut static_members: HashSet<String> = HashSet::new();
        loop {
            let mut s = self.lex()?;
            let (mut getter, mut setter, mut is_static) = (false, false, false);
            while s.is(&[T::Static, T::Public, T::Private]) {
                if s.t == T::Static {
                    is_static = true;
                }
                s = self.lex()?;
            }
            match s.t {
                T::Function => {
                    s = self.lex()?;
                    if s.t == T::Set {
                        setter = true;
                        s = self.lex()?;
                    } else if s.t == T::Get {
                        getter = true;
                        s = self.lex()?;
                    }
                    self.expected_identifier(&s, &[])?;
                    let fname = s.text();
                    if is_static {
                        static_members.insert(fname.clone());
                    } else {
                        if getter {
                            this_gets.insert(fname.clone());
                        }
                        if setter {
                            this_sets.insert(fname.clone());
                        }
                    }
                    if !is_interface {
                        let f = self.function(true, "", true, vars, funcs, in_tell, eval)?;
                        let key = self.konst(&fname);
                        {
                            let mut fm = f.borrow_mut();
                            fm.calc_name = Some(key.clone());
                            fm.is_setter = setter;
                            fm.is_getter = getter;
                        }
                        if setter {
                            let obj = if is_static {
                                name.clone()
                            } else {
                                let this = var("this", None, false);
                                f.borrow_mut().vars.push(this.clone());
                                It::Var(this)
                            };
                            let call = It::CallMethod { obj: b(obj), name: b(self.konst(&format!("__get__{fname}"))), args: Vec::new(), getter: false };
                            if let Some(body) = &mut f.borrow_mut().body {
                                body.push(It::Return(Some(b(call))));
                            }
                        }
                        traits.push((key, It::Function(f)));
                        statics.push(is_static);
                    }
                }
                T::Var => {
                    s = self.lex()?;
                    self.expected_identifier(&s, &[])?;
                    let ident = s.text();
                    if is_static {
                        static_members.insert(ident.clone());
                    }
                    s = self.lex()?;
                    if s.t == T::Colon {
                        self.type_name(vars)?;
                        s = self.lex()?;
                    }
                    if s.t == T::Assign {
                        let key = self.konst(&ident);
                        let value = self.expression_req(false, false, false, false, true, vars, funcs, false, eval)?;
                        traits.push((key, value));
                        statics.push(is_static);
                        s = self.lex()?;
                    }
                    if s.t != T::Semicolon {
                        self.lx.pushback(s);
                    }
                }
                _ => {
                    self.lx.pushback(s);
                    break;
                }
            }
        }
        for (_, val) in &mut traits {
            self.resolve_static_members(val, &name, &static_members, &HashSet::new());
            self.mark_accessors(val, &this_gets, &this_sets, &super_gets, &super_sets);
        }
        if is_interface {
            Ok(It::Interface { name: b(name), extends: implements })
        } else {
            Ok(It::Class { name: b(name), extends: extends.map(b), implements, traits, statics })
        }
    }

    fn mark_accessors(&self, val: &mut It, this_gets: &HashSet<String>, this_sets: &HashSet<String>, super_gets: &HashSet<String>, super_sets: &HashSet<String>) {
        let pool = self.pool.clone();
        let scope = |v: &It| -> Option<String> {
            match v {
                It::Var(v) => Some(v.borrow().name.clone()),
                _ => None,
            }
        };
        let gets = |who: &str, m: &str| (who == "this" && this_gets.contains(m)) || (who == "super" && super_gets.contains(m));
        let sets = |who: &str, m: &str| (who == "this" && this_sets.contains(m)) || (who == "super" && super_sets.contains(m));
        walk(val, &mut |si| {
            let pool = pool.borrow();
            let path = if matches!(si, It::GetMember { .. }) { self.members_path_with(si, &pool) } else { None };
            match si {
                It::GetMember { .. } => {
                    if let Some(p) = path {
                        if p.len() > 1 && gets(&p[0], &p[1]) {
                            first_get_member(si).map(|g| {
                                if let It::GetMember { getter, .. } = g {
                                    *getter = true;
                                }
                            });
                        }
                    }
                }
                It::SetMember { obj, member, setter, .. } => {
                    if let (Some(m), Some(who)) = (direct_str(member, &pool), scope(obj)) {
                        if sets(&who, &m) {
                            *setter = true;
                        }
                    }
                }
                It::CallMethod { obj, name, getter, .. } | It::NewMethod { obj, name, getter, .. } => {
                    if let (Some(m), Some(who)) = (direct_str(name, &pool), scope(obj)) {
                        if gets(&who, &m) {
                            *getter = true;
                        }
                    }
                }
                It::Delete { obj: Some(obj), name, getter } => {
                    if let (Some(m), Some(who)) = (direct_str(name, &pool), scope(obj)) {
                        if gets(&who, &m) {
                            *getter = true;
                        }
                    }
                }
                _ => {}
            }
        });
    }

    fn members_path_with(&self, it: &It, pool: &Pool) -> Option<Vec<String>> {
        let mut out = Vec::new();
        let mut cur = it;
        while let It::GetMember { obj, member, .. } = cur {
            out.insert(0, direct_str(member, pool)?);
            cur = obj;
        }
        match cur {
            It::GetVariable(n) => out.insert(0, direct_str(n, pool)?),
            It::Var(v) => out.insert(0, v.borrow().name.clone()),
            _ => return None,
        }
        Some(out)
    }

    fn resolve_static_members(&self, item: &mut It, class: &It, statics: &HashSet<String>, shadowed: &HashSet<String>) {
        if let It::Try { body, names, catches, finally, .. } = item {
            for c in body.iter_mut() {
                self.resolve_static_members(c, class, statics, shadowed);
            }
            for (i, cc) in catches.iter_mut().enumerate() {
                let mut scope = shadowed.clone();
                if let Some(n) = names.get(i).and_then(|n| direct_str(n, &self.pool.borrow())) {
                    scope.insert(n);
                }
                for c in cc.iter_mut() {
                    self.resolve_static_members(c, class, statics, &scope);
                }
            }
            if let Some(f) = finally {
                for c in f.iter_mut() {
                    self.resolve_static_members(c, class, statics, shadowed);
                }
            }
            return;
        }
        let mut shadowed = shadowed.clone();
        if let It::Function(f) = item {
            let f = f.borrow();
            shadowed.extend(f.params.iter().cloned());
            for v in &f.vars {
                let v = v.borrow();
                if v.definition {
                    shadowed.insert(v.name.clone());
                }
            }
            for inner in &f.inner {
                let n = inner.borrow().name.clone();
                if !n.is_empty() {
                    shadowed.insert(n);
                }
            }
        }
        match item {
            It::Var(v) => {
                let (def, name) = {
                    let b = v.borrow();
                    (b.definition, b.name.clone())
                };
                if !def && statics.contains(&name) && !shadowed.contains(&name) {
                    v.borrow_mut().resolved = Some((class.clone(), self.konst(&name)));
                }
            }
            It::Delete { obj, name, .. } => {
                if obj.is_none() {
                    if let Some(n) = direct_str(name, &self.pool.borrow()) {
                        if statics.contains(&n) && !shadowed.contains(&n) {
                            *obj = Some(b(class.clone()));
                        }
                    }
                }
            }
            It::NewObject { name, resolved, .. } => {
                if let Some(n) = direct_str(name, &self.pool.borrow()) {
                    if statics.contains(&n) && !shadowed.contains(&n) {
                        *resolved = Some(b(class.clone()));
                    }
                }
            }
            _ => {}
        }
        if let It::CallFunction { name, .. } = item {
            self.resolve_static_members(name, class, statics, &shadowed);
        }
        children(item, &mut |c| self.resolve_static_members(c, class, statics, &shadowed));
    }

    #[allow(clippy::too_many_arguments)]
    fn expression_commands(&mut self, s: &Sym, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<Option<It>> {
        macro_rules! e {
            () => {
                self.expression_req(in_with, in_fn, in_method, in_tell, true, vars, funcs, false, eval)?
            };
        }
        if in_with
            && matches!(
                s.t,
                T::DuplicateMovieClip | T::GetUrl | T::GotoAndStop | T::GotoAndPlay | T::NextFrame | T::Play | T::PrevFrame | T::Stop
                    | T::UnloadMovie | T::UnloadMovieNum | T::LoadVariables | T::LoadMovie | T::LoadVariablesNum | T::LoadMovieNum
                    | T::RemoveMovieClip
            )
        {
            let name = self.konst(&s.text());
            self.expected_type(&[T::ParentOpen])?;
            let args = self.call(in_with, in_fn, in_method, in_tell, vars, funcs, eval)?;
            return Ok(Some(It::CallFunction { name: b(name), args }));
        }
        let open = |p: &mut Self| p.expected_type(&[T::ParentOpen]).map(|_| ());
        let close = |p: &mut Self| p.expected_type(&[T::ParentClose]).map(|_| ());
        let comma = |p: &mut Self| p.expected_type(&[T::Comma]).map(|_| ());
        let op1 = |v: It, code: u8| It::Op1 { value: b(v), code, ret: true };
        let ret = match s.t {
            T::DuplicateMovieClip => {
                open(self)?;
                let src = e!();
                comma(self)?;
                let target = e!();
                comma(self)?;
                let depth = e!();
                close(self)?;
                It::CloneSprite { src: b(src), target: b(target), depth: b(depth) }
            }
            T::FsCommand => {
                open(self)?;
                let cmd = e!();
                let s = self.lex()?;
                let param = if s.t == T::Comma {
                    Some(b(e!()))
                } else {
                    self.lx.pushback(s);
                    None
                };
                close(self)?;
                It::FsCommand { cmd: b(cmd), param }
            }
            T::FsCommand2 => {
                open(self)?;
                let mut args = vec![e!()];
                let mut s = self.lex()?;
                while s.t == T::Comma {
                    args.insert(0, e!());
                    s = self.lex()?;
                }
                self.expected(&s, &[T::ParentClose])?;
                It::FsCommand2(args)
            }
            T::Set => {
                open(self)?;
                let name = e!();
                comma(self)?;
                let value = e!();
                close(self)?;
                *eval = true;
                It::SetVariable { name: b(name), value: b(value), force_set: true }
            }
            T::Trace => {
                open(self)?;
                let v = e!();
                close(self)?;
                It::Trace(b(v))
            }
            T::GetUrl => {
                open(self)?;
                let s = self.lex()?;
                if s.t == T::String {
                    let url = s.clone();
                    let s2 = self.lex()?;
                    if s2.t == T::Comma {
                        let target = self.lex()?;
                        if target.t == T::String {
                            let s3 = self.lex()?;
                            if s3.t == T::ParentClose {
                                return Ok(Some(It::GetUrl { url: url.text(), target: target.text() }));
                            }
                            self.lx.pushback(s3);
                        }
                        self.lx.pushback(target);
                    } else if s2.t == T::ParentClose {
                        return Ok(Some(It::GetUrl { url: url.text(), target: String::new() }));
                    }
                    self.lx.pushback(s2);
                    self.lx.pushback(url);
                } else {
                    self.lx.pushback(s);
                }
                let url = e!();
                let s = self.lex()?;
                self.expected(&s, &[T::ParentClose, T::Comma])?;
                let mut method = 0;
                let target = if s.t == T::Comma {
                    let t = e!();
                    let s = self.lex()?;
                    if s.t == T::Comma {
                        let m = self.lex()?;
                        self.expected(&m, &[T::String])?;
                        method = match m.text().as_str() {
                            "GET" => 1,
                            "POST" => 2,
                            _ => return err(self.line(), "Invalid method, \"GET\" or \"POST\" expected."),
                        };
                    } else {
                        self.lx.pushback(s);
                    }
                    t
                } else {
                    self.lx.pushback(s);
                    It::Direct(Val::Str(String::new()))
                };
                close(self)?;
                It::GetUrl2 { url: b(url), target: b(target), method }
            }
            T::GotoAndStop | T::GotoAndPlay => {
                let play = s.t == T::GotoAndPlay;
                open(self)?;
                let mut frame = e!();
                let mut bias = -1;
                let s = self.lex()?;
                if s.t == T::Comma {
                    match frame {
                        It::Direct(Val::Int(v)) => bias = v,
                        _ => return err(self.line(), "Scene bias must be number"),
                    }
                    frame = e!();
                } else {
                    self.lx.pushback(s);
                }
                close(self)?;
                It::GotoFrame2 { frame: b(frame), bias_flag: bias != -1, play, bias }
            }
            T::NextFrame | T::Play | T::PrevFrame | T::Stop | T::StopAllSounds | T::ToggleHighQuality | T::StopDrag => {
                open(self)?;
                close(self)?;
                It::Cmd0(match s.t {
                    T::NextFrame => Cmd0::NextFrame,
                    T::Play => Cmd0::Play,
                    T::PrevFrame => Cmd0::PrevFrame,
                    T::Stop => Cmd0::Stop,
                    T::StopAllSounds => Cmd0::StopAllSounds,
                    T::ToggleHighQuality => Cmd0::ToggleHighQuality,
                    _ => Cmd0::StopDrag,
                })
            }
            T::UnloadMovie | T::UnloadMovieNum => {
                open(self)?;
                let v = e!();
                close(self)?;
                if s.t == T::UnloadMovie { It::UnloadMovie(b(v)) } else { It::UnloadMovieNum(b(v)) }
            }
            T::Print | T::PrintAsBitmap | T::PrintAsBitmapNum | T::PrintNum => {
                open(self)?;
                let target = e!();
                comma(self)?;
                let bbox = e!();
                close(self)?;
                let kind = match s.t {
                    T::Print => PrintKind::Print,
                    T::PrintNum => PrintKind::PrintNum,
                    T::PrintAsBitmap => PrintKind::AsBitmap,
                    _ => PrintKind::AsBitmapNum,
                };
                It::Print { kind, target: b(target), bbox: b(bbox) }
            }
            T::LoadVariables | T::LoadMovie | T::LoadVariablesNum | T::LoadMovieNum => {
                open(self)?;
                let url = e!();
                comma(self)?;
                let target = e!();
                let s2 = self.lex()?;
                self.expected(&s2, &[T::ParentClose, T::Comma])?;
                let mut method = 0;
                if s2.t == T::Comma {
                    let m = self.lex()?;
                    self.expected(&m, &[T::String])?;
                    method = match m.text().as_str() {
                        "POST" => 2,
                        "GET" => 1,
                        _ => return err(self.line(), "Invalid method, \"GET\" or \"POST\" expected."),
                    };
                } else {
                    self.lx.pushback(s2);
                }
                close(self)?;
                let kind = match s.t {
                    T::LoadVariables => LoadKind::Variables,
                    T::LoadMovie => LoadKind::Movie,
                    T::LoadVariablesNum => LoadKind::VariablesNum,
                    _ => LoadKind::MovieNum,
                };
                It::Load { kind, url: b(url), target: b(target), method }
            }
            T::RemoveMovieClip => {
                open(self)?;
                let v = e!();
                close(self)?;
                It::RemoveSprite(b(v))
            }
            T::StartDrag => {
                open(self)?;
                let target = e!();
                let zero = || It::Direct(Val::Int(0));
                let (lock, constrain, x1, y1, x2, y2);
                let s = self.lex()?;
                if s.t == T::Comma {
                    lock = e!();
                    let s = self.lex()?;
                    if s.t == T::Comma {
                        constrain = It::Direct(Val::Int(1));
                        x1 = Some(e!());
                        let s = self.lex()?;
                        if s.t == T::Comma {
                            y1 = Some(e!());
                            let s = self.lex()?;
                            if s.t == T::Comma {
                                x2 = Some(e!());
                                let s = self.lex()?;
                                if s.t == T::Comma {
                                    y2 = Some(e!());
                                } else {
                                    self.lx.pushback(s);
                                    y2 = Some(zero());
                                }
                            } else {
                                self.lx.pushback(s);
                                x2 = Some(zero());
                                y2 = Some(zero());
                            }
                        } else {
                            self.lx.pushback(s);
                            x2 = Some(zero());
                            y2 = Some(zero());
                            y1 = Some(zero());
                        }
                    } else {
                        self.lx.pushback(s);
                        constrain = zero();
                        x1 = None;
                        y1 = None;
                        x2 = None;
                        y2 = None;
                    }
                } else {
                    lock = zero();
                    constrain = zero();
                    x1 = None;
                    y1 = None;
                    x2 = None;
                    y2 = None;
                    self.lx.pushback(s);
                }
                close(self)?;
                It::StartDrag { target: b(target), lock: b(lock), constrain: b(constrain), x1: x1.map(b), y1: y1.map(b), x2: x2.map(b), y2: y2.map(b) }
            }
            T::Call => {
                open(self)?;
                let v = e!();
                close(self)?;
                It::Op1 { value: b(v), code: 0x9e, ret: false }
            }
            T::GetVersion => {
                open(self)?;
                close(self)?;
                It::GetVersion
            }
            T::MbOrd | T::MbChr | T::MbLength | T::Length | T::Random | T::Int | T::Ord | T::Chr | T::TargetPath => {
                open(self)?;
                let v = e!();
                close(self)?;
                op1(
                    v,
                    match s.t {
                        T::MbOrd => 0x36,
                        T::MbChr => 0x37,
                        T::MbLength => 0x31,
                        T::Length => 0x14,
                        T::Random => 0x30,
                        T::Int => 0x18,
                        T::Ord => 0x32,
                        T::Chr => 0x33,
                        _ => 0x45,
                    },
                )
            }
            T::MbSubstring | T::Substring => {
                open(self)?;
                let value = e!();
                comma(self)?;
                let index = e!();
                comma(self)?;
                let count = e!();
                close(self)?;
                It::Extract { value: b(value), index: b(index), count: b(count), mb: s.t == T::MbSubstring }
            }
            T::NumberOp | T::StringOp => {
                let name = s.text();
                let s2 = self.lex()?;
                if s2.t == T::Dot {
                    self.lx.pushback(s2);
                    let v = var(&name, None, false);
                    vars.push(v.clone());
                    It::Var(v)
                } else {
                    self.expected(&s2, &[T::ParentOpen])?;
                    let v = e!();
                    close(self)?;
                    op1(v, if s.t == T::NumberOp { 0x4a } else { 0x4b })
                }
            }
            T::GetTimer => {
                open(self)?;
                close(self)?;
                It::GetTime
            }
            _ => return Ok(None),
        };
        Ok(Some(ret))
    }

    #[allow(clippy::too_many_arguments)]
    fn command(&mut self, in_with: bool, in_fn: bool, in_method: bool, forin: i32, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<Option<It>> {
        let mark = self.lx.recorded.replace(Vec::new());
        let r = self.command_inner(in_with, in_fn, in_method, forin, in_tell, vars, funcs, eval);
        let rec = std::mem::replace(&mut self.lx.recorded, mark);
        if let (Some(outer), Some(inner)) = (&mut self.lx.recorded, rec) {
            outer.extend(inner);
        }
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn command_inner(&mut self, in_with: bool, in_fn: bool, in_method: bool, forin: i32, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<Option<It>> {
        macro_rules! e {
            ($remainder:expr, $comma:expr) => {
                self.expression(in_with, in_fn, in_method, in_tell, $remainder, vars, funcs, $comma, eval)?
            };
        }
        macro_rules! req {
            ($remainder:expr, $comma:expr) => {
                self.expression_req(in_with, in_fn, in_method, in_tell, $remainder, vars, funcs, $comma, eval)?
            };
        }
        macro_rules! cmd {
            ($with:expr, $forin:expr, $tell:expr) => {
                self.command($with, in_fn, in_method, $forin, $tell, vars, funcs, eval)?
            };
        }
        let s = self.lex()?;
        if s.t == T::Eof {
            return Ok(None);
        }
        if s.group == Group::GlobalFunc {
            let s2 = self.lex()?;
            if s2.t != T::ParentOpen {
                self.lx.pushback(s2);
                self.lx.pushback(s);
                let ret = e!(true, false);
                let s = self.lex()?;
                if s.t != T::Semicolon {
                    self.lx.pushback(s);
                }
                return Ok(ret);
            }
            self.lx.pushback(s2);
        }
        let ret: Option<It> = match s.t {
            T::With => {
                self.expected_type(&[T::ParentOpen])?;
                let scope = req!(false, false);
                self.expected_type(&[T::ParentClose])?;
                self.expected_type(&[T::CurlyOpen])?;
                let body = self.commands(true, in_fn, in_method, forin, in_tell, vars, funcs, eval)?;
                self.expected_type(&[T::CurlyClose])?;
                Some(It::With { scope: b(scope), body })
            }
            T::Delete => {
                let target = req!(false, false);
                Some(match target {
                    It::GetMember { obj, member, .. } => It::Delete { obj: Some(obj), name: member, getter: false },
                    It::Var(v) => {
                        vars.retain(|x| !Rc::ptr_eq(x, &v));
                        let name = v.borrow().name.clone();
                        It::Delete { obj: None, name: b(self.konst(&name)), getter: false }
                    }
                    It::Op1 { value, code: 0x1c, .. } | It::Paren(value) => It::Delete { obj: None, name: value, getter: false },
                    other => It::Delete { obj: None, name: b(other), getter: false },
                })
            }
            T::TellTarget => {
                self.expected_type(&[T::ParentOpen])?;
                let target = req!(true, false);
                self.expected_type(&[T::ParentClose])?;
                self.expected_type(&[T::CurlyOpen])?;
                let body = self.commands(in_with, in_fn, in_method, forin, true, vars, funcs, eval)?;
                self.expected_type(&[T::CurlyClose])?;
                Some(It::TellTarget { target: b(target), body, nested: in_tell })
            }
            T::IfFrameLoaded => {
                let line = self.line();
                self.expected_type(&[T::ParentOpen])?;
                let frame = req!(true, false);
                self.expected_type(&[T::ParentClose])?;
                self.expected_type(&[T::CurlyOpen])?;
                let body = self.commands(in_with, in_fn, in_method, forin, in_tell, vars, funcs, eval)?;
                self.expected_type(&[T::CurlyClose])?;
                Some(It::IfFrameLoaded { frame: b(frame), body, line })
            }
            T::Class => {
                let name = self.type_name(vars)?;
                let mut s = self.lex()?;
                let mut extends = None;
                if s.t == T::Extends {
                    extends = Some(self.type_name(vars)?);
                    s = self.lex()?;
                }
                let mut implements = Vec::new();
                if s.t == T::Implements {
                    loop {
                        implements.push(self.type_name(vars)?);
                        s = self.lex()?;
                        if s.t != T::Comma {
                            break;
                        }
                    }
                }
                self.expected(&s, &[T::CurlyOpen])?;
                let r = self.traits(false, name, extends, implements, vars, funcs, in_tell, eval)?;
                self.expected_type(&[T::CurlyClose])?;
                Some(r)
            }
            T::Interface => {
                let name = self.type_name(vars)?;
                let mut s = self.lex()?;
                let mut extends = Vec::new();
                if s.t == T::Extends {
                    loop {
                        extends.push(self.type_name(vars)?);
                        s = self.lex()?;
                        if s.t != T::Comma {
                            break;
                        }
                    }
                }
                self.expected(&s, &[T::CurlyOpen])?;
                let r = self.traits(true, name, None, extends, vars, funcs, in_tell, eval)?;
                self.expected_type(&[T::CurlyClose])?;
                Some(r)
            }
            T::Function => {
                let s = self.lex()?;
                self.expected_identifier(&s, &[])?;
                Some(It::Function(self.function(true, &s.text(), false, vars, funcs, in_tell, eval)?))
            }
            T::Var => {
                let s = self.lex()?;
                self.expected_identifier(&s, &[])?;
                let name = s.text();
                let mut s = self.lex()?;
                if s.t == T::Colon {
                    self.type_name(vars)?;
                    s = self.lex()?;
                }
                let v = if s.t == T::Assign {
                    let value = req!(true, false);
                    var(&name, Some(value), true)
                } else {
                    self.lx.pushback(s);
                    var(&name, Some(It::Direct(Val::Undefined)), true)
                };
                vars.push(v.clone());
                Some(It::Var(v))
            }
            T::CurlyOpen => {
                let body = self.commands(in_with, in_fn, in_method, forin, in_tell, vars, funcs, eval)?;
                self.expected_type(&[T::CurlyClose])?;
                Some(It::Block(body))
            }
            T::Increment | T::Decrement => {
                let v = req!(false, false);
                Some(if s.t == T::Increment { It::PreInc(b(v)) } else { It::PreDec(b(v)) })
            }
            T::Super => {
                let s2 = self.lex()?;
                if s2.t == T::ParentOpen {
                    let args = self.call(in_with, in_fn, in_method, in_tell, vars, funcs, eval)?;
                    let sup = var(&s.text(), None, false);
                    vars.push(sup.clone());
                    Some(It::CallMethod { obj: b(It::Var(sup)), name: b(It::Direct(Val::Undefined)), args, getter: false })
                } else {
                    self.lx.pushback(s2);
                    self.lx.pushback(s.clone());
                    None
                }
            }
            T::If => {
                let line = self.line();
                self.expected_type(&[T::ParentOpen])?;
                let cond = req!(true, false);
                self.expected_type(&[T::ParentClose])?;
                let on_true: Vec<It> = cmd!(in_with, forin, in_tell).into_iter().collect();
                let s = self.lex()?;
                let on_false = if s.t == T::Else {
                    Some(cmd!(in_with, forin, in_tell).into_iter().collect())
                } else {
                    self.lx.pushback(s);
                    None
                };
                Some(It::If { cond: b(cond), on_true, on_false, line })
            }
            T::While => {
                let line = self.line();
                self.expected_type(&[T::ParentOpen])?;
                let cond = vec![req!(true, true)];
                self.expected_type(&[T::ParentClose])?;
                let body = cmd!(in_with, forin, in_tell).into_iter().collect();
                Some(It::While { cond, body, line })
            }
            T::Do => {
                let line = self.line();
                let body = cmd!(in_with, forin, in_tell).into_iter().collect();
                self.expected_type(&[T::While])?;
                self.expected_type(&[T::ParentOpen])?;
                let cond = vec![req!(true, true)];
                self.expected_type(&[T::ParentClose])?;
                Some(It::DoWhile { body, cond, line })
            }
            T::For => {
                let line = self.line();
                self.expected_type(&[T::ParentOpen])?;
                let s = self.lex()?;
                let mut forin_var: Option<VarRef> = None;
                let mut collection = None;
                if s.t == T::Var || Self::is_identifier(&s, &[]) {
                    let mut s2 = None;
                    let mut sel = s.clone();
                    let define = s.t == T::Var;
                    if define {
                        let n = self.lex()?;
                        s2 = Some(n.clone());
                        sel = n;
                    }
                    if Self::is_identifier(&sel, &[]) {
                        let s3 = self.lex()?;
                        if s3.t == T::In {
                            let v = var(&sel.text(), Some(It::Empty), define);
                            vars.push(v.clone());
                            forin_var = Some(v);
                            collection = Some(req!(true, false));
                        } else {
                            self.lx.pushback(s3);
                            if let Some(n) = s2 {
                                self.lx.pushback(n);
                            }
                            self.lx.pushback(s);
                        }
                    } else {
                        if let Some(n) = s2 {
                            self.lx.pushback(n);
                        }
                        self.lx.pushback(s);
                    }
                } else {
                    self.lx.pushback(s);
                }
                let mut first = Vec::new();
                let mut last = Vec::new();
                let mut cond = None;
                if forin_var.is_none() {
                    if let Some(fc) = cmd!(in_with, forin, in_tell) {
                        first.push(fc);
                    }
                    cond = Some(e!(true, false).unwrap_or(It::True));
                    self.expected_type(&[T::Semicolon])?;
                    if let Some(fc) = cmd!(in_with, forin, in_tell) {
                        last.push(fc);
                    }
                }
                self.expected_type(&[T::ParentClose])?;
                let level = if forin_var.is_some() { forin + 1 } else { forin };
                let body = cmd!(in_with, level, in_tell).into_iter().collect();
                Some(match forin_var {
                    Some(v) => It::ForIn { var: v, collection: b(collection.unwrap()), body, line },
                    None => It::For { first, cond: b(cond.unwrap()), last, body, line },
                })
            }
            T::Switch => {
                let line = self.line();
                self.expected_type(&[T::ParentOpen])?;
                let expr = req!(true, false);
                self.expected_type(&[T::ParentClose])?;
                self.expected_type(&[T::CurlyOpen])?;
                let mut s = self.lex()?;
                let mut values = Vec::new();
                let mut cmds = Vec::new();
                let mut mapping = Vec::new();
                let mut pos = 0;
                while s.is(&[T::Case, T::Default]) {
                    while s.is(&[T::Case, T::Default]) {
                        let v = if s.t == T::Default { None } else { Some(req!(true, false)) };
                        self.expected_type(&[T::Colon])?;
                        s = self.lex()?;
                        values.push(v);
                        mapping.push(pos);
                    }
                    pos += 1;
                    self.lx.pushback(s);
                    cmds.push(self.commands(in_with, in_fn, in_method, forin, in_tell, vars, funcs, eval)?);
                    s = self.lex()?;
                }
                self.expected(&s, &[T::CurlyClose])?;
                Some(It::Switch { expr: b(expr), values, cmds, mapping, line })
            }
            T::Break => Some(It::Break),
            T::Continue => Some(It::Continue),
            T::Return => {
                let v = e!(true, false).unwrap_or(It::Direct(Val::Undefined));
                Some(It::Return(Some(b(v))))
            }
            T::Try => {
                let body: Vec<It> = cmd!(in_with, forin, in_tell).into_iter().collect();
                let mut s = self.lex()?;
                let mut found = false;
                let (mut names, mut types, mut catches) = (Vec::new(), Vec::new(), Vec::new());
                while s.t == T::Catch {
                    self.expected_type(&[T::ParentOpen])?;
                    let n = self.lex()?;
                    self.expected_identifier(&n, &[T::String])?;
                    names.push(self.konst(&n.text()));
                    let c = self.lex()?;
                    if c.t == T::Colon {
                        types.push(Some(self.type_name(vars)?));
                    } else {
                        types.push(None);
                        self.lx.pushback(c);
                    }
                    self.expected_type(&[T::ParentClose])?;
                    catches.push(cmd!(in_with, forin, in_tell).into_iter().collect());
                    s = self.lex()?;
                    found = true;
                }
                let mut finally = None;
                if s.t == T::Finally {
                    finally = Some(cmd!(in_with, forin, in_tell).into_iter().collect());
                    found = true;
                    s = self.lex()?;
                }
                if !found {
                    self.expected(&s, &[T::Catch, T::Finally])?;
                }
                self.lx.pushback(s);
                Some(It::Try { body, names, types, catches, finally })
            }
            T::Throw => Some(It::Throw(b(req!(true, false)))),
            T::Semicolon => return Ok(Some(It::Empty)),
            T::Directive => match s.text().as_str() {
                "strict" => Some(It::StrictMode(1)),
                d => return err(self.line(), format!("Unknown directive: #{d}")),
            },
            _ => {
                self.lx.pushback(s);
                e!(true, true)
            }
        };
        let ret = match ret {
            Some(r) => Some(r),
            None => {
                let rec = self.lx.recorded.take().unwrap_or_default();
                for sym in rec.into_iter().rev() {
                    self.lx.pushed.push(sym);
                }
                self.lx.recorded = Some(Vec::new());
                e!(true, false)
            }
        };
        let s = self.lex()?;
        if s.t != T::Semicolon {
            self.lx.pushback(s);
        }
        Ok(ret)
    }

    #[allow(clippy::too_many_arguments)]
    fn expression_req(&mut self, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, remainder: bool, vars: &mut Vars, funcs: &mut Funcs, comma: bool, eval: &mut bool) -> R<It> {
        match self.expression(in_with, in_fn, in_method, in_tell, remainder, vars, funcs, comma, eval)? {
            Some(e) => Ok(e),
            None => {
                let s = self.peek()?;
                err(self.line(), format!("Expression expected but {:?} found", s.t))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn expression(&mut self, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, remainder: bool, vars: &mut Vars, funcs: &mut Funcs, comma: bool, eval: &mut bool) -> R<Option<It>> {
        let mut items = Vec::new();
        loop {
            let Some(prim) = self.primary(false, in_with, in_fn, in_method, in_tell, remainder, vars, funcs, true, eval)? else { return Ok(None) };
            items.push(self.expression1(prim, NOPRECEDENCE, in_with, in_fn, in_method, in_tell, remainder, vars, funcs, eval)?);
            let s = self.lex()?;
            if !(comma && s.t == T::Comma) {
                self.lx.pushback(s);
                break;
            }
        }
        if items.len() == 1 {
            return Ok(items.pop());
        }
        Ok(Some(It::Comma(items)))
    }

    fn is_binary(s: &Sym) -> bool {
        (s.t == T::Identifier && ["add", "eq", "ne", "lt", "ge", "gt", "le"].contains(&s.text().as_str())) || s.t.info().1
    }

    fn prec(s: &Sym) -> i32 {
        if s.t == T::Identifier {
            match s.text().as_str() {
                "add" => return super::lexer::ADDITIVE,
                "eq" | "ne" => return super::lexer::EQUALITY,
                "lt" | "ge" | "gt" | "le" => return super::lexer::RELATIONAL,
                _ => {}
            }
        }
        s.t.info().0
    }

    #[allow(clippy::too_many_arguments)]
    fn expression1(&mut self, mut lhs: It, min: i32, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, remainder: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<It> {
        let mut look = self.peek()?;
        while Self::is_binary(&look) && Self::prec(&look) <= min {
            let op = look;
            self.lex()?;
            let mut mhs = None;
            if op.t == T::Ternar {
                mhs = Some(self.expression_req(in_with, in_fn, in_method, in_tell, remainder, vars, funcs, false, eval)?);
                self.expected_type(&[T::Colon])?;
            }
            let Some(mut rhs) = self.primary(remainder, in_with, in_fn, in_method, in_tell, remainder, vars, funcs, true, eval)? else {
                return err(self.line(), "Missing operand");
            };
            look = self.peek()?;
            while (Self::is_binary(&look) && Self::prec(&look) < Self::prec(&op)) || (look.t.info().2 && Self::prec(&look) == Self::prec(&op)) {
                rhs = self.expression1(rhs, Self::prec(&look), in_with, in_fn, in_method, in_tell, remainder, vars, funcs, eval)?;
                look = self.peek()?;
            }
            let bin = |op: Bin, l: It, r: It| It::Bin { op, l: b(l), r: b(r) };
            let add = if self.version >= 5 { Bin::Add2 } else { Bin::Add };
            lhs = match op.t {
                T::Ternar => It::Ternary(b(lhs), b(mhs.unwrap()), b(rhs)),
                T::ShiftLeft => bin(Bin::Shl, lhs, rhs),
                T::ShiftRight => bin(Bin::Shr, lhs, rhs),
                T::UShiftRight => bin(Bin::UShr, lhs, rhs),
                T::BitAnd => bin(Bin::BitAnd, lhs, rhs),
                T::BitOr => bin(Bin::BitOr, lhs, rhs),
                T::Divide => bin(Bin::Div, lhs, rhs),
                T::Modulo => bin(Bin::Mod, lhs, rhs),
                T::Equals => bin(Bin::Eq, lhs, rhs),
                T::StrictEquals => bin(Bin::StrictEq, lhs, rhs),
                T::NotEqual => bin(Bin::Neq, lhs, rhs),
                T::StrictNotEqual => bin(Bin::StrictNeq, lhs, rhs),
                T::LowerThan => bin(Bin::Lt, lhs, rhs),
                T::LowerEqual => bin(Bin::Le, lhs, rhs),
                T::GreaterThan => bin(Bin::Gt, lhs, rhs),
                T::GreaterEqual => bin(Bin::Ge, lhs, rhs),
                T::And => It::And(b(lhs), b(rhs)),
                T::Or => It::Or(b(lhs), b(rhs)),
                T::FullAnd => bin(Bin::FullAnd, lhs, rhs),
                T::FullOr => bin(Bin::FullOr, lhs, rhs),
                T::Minus => bin(Bin::Sub, lhs, rhs),
                T::Multiply => bin(Bin::Mul, lhs, rhs),
                T::Plus => bin(add, lhs, rhs),
                T::Xor => bin(Bin::BitXor, lhs, rhs),
                T::InstanceOf => bin(Bin::InstanceOf, lhs, rhs),
                T::Assign | T::AssignBitAnd | T::AssignBitOr | T::AssignDivide | T::AssignMinus | T::AssignModulo | T::AssignMultiply
                | T::AssignPlus | T::AssignShiftLeft | T::AssignShiftRight | T::AssignUShiftRight | T::AssignXor => {
                    let compound = op.t != T::Assign;
                    let assigned = match op.t {
                        T::Assign => rhs,
                        T::AssignBitAnd => bin(Bin::BitAnd, lhs.clone(), rhs),
                        T::AssignBitOr => bin(Bin::BitOr, lhs.clone(), rhs),
                        T::AssignDivide => bin(Bin::Div, lhs.clone(), rhs),
                        T::AssignMinus => bin(Bin::Sub, lhs.clone(), rhs),
                        T::AssignModulo => bin(Bin::Mod, lhs.clone(), rhs),
                        T::AssignMultiply => bin(Bin::Mul, lhs.clone(), rhs),
                        T::AssignPlus => bin(add, lhs.clone(), rhs),
                        T::AssignShiftLeft => bin(Bin::Shl, lhs.clone(), rhs),
                        T::AssignShiftRight => bin(Bin::Shr, lhs.clone(), rhs),
                        T::AssignUShiftRight => bin(Bin::UShr, lhs.clone(), rhs),
                        _ => bin(Bin::BitXor, lhs.clone(), rhs),
                    };
                    match lhs {
                        It::GetProperty { target, index } => It::SetProperty { target, index, value: b(assigned) },
                        It::Var(v) => {
                            if compound {
                                let name = v.borrow().name.clone();
                                let nv = var(&name, Some(assigned), false);
                                vars.push(nv.clone());
                                It::Var(nv)
                            } else {
                                {
                                    let mut m = v.borrow_mut();
                                    m.store = Some(assigned);
                                    m.definition = false;
                                }
                                It::Var(v)
                            }
                        }
                        It::GetMember { obj, member, .. } => It::SetMember { obj, member, value: b(assigned), setter: false },
                        _ => return err(self.line(), "Invalid assignment"),
                    }
                }
                T::Identifier => match op.text().as_str() {
                    "add" => bin(Bin::StrAdd, lhs, rhs),
                    "eq" => bin(Bin::StrEq, lhs, rhs),
                    "ne" => bin(Bin::StrNe, lhs, rhs),
                    "lt" => bin(Bin::StrLt, lhs, rhs),
                    "ge" => bin(Bin::StrGe, lhs, rhs),
                    "gt" => bin(Bin::StrGt, lhs, rhs),
                    _ => bin(Bin::StrLe, lhs, rhs),
                },
                _ => lhs,
            };
        }
        Ok(lhs)
    }

    #[allow(clippy::too_many_arguments)]
    fn brackets(&mut self, out: &mut Vec<It>, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, eval: &mut bool) -> R<()> {
        let mut s = self.lex()?;
        if s.t != T::BracketOpen {
            self.lx.pushback(s);
            return Ok(());
        }
        s = self.lex()?;
        while s.t != T::BracketClose {
            if s.t != T::Comma {
                self.lx.pushback(s);
            }
            out.push(self.expression_req(in_with, in_fn, in_method, in_tell, true, vars, funcs, false, eval)?);
            s = self.lex()?;
            if !s.is(&[T::Comma, T::BracketClose]) {
                self.expected(&s, &[T::Comma, T::BracketClose])?;
            }
        }
        Ok(())
    }

    fn handle_variable(&mut self, s: &Sym, vars: &mut Vars, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, funcs: &mut Funcs, eval: &mut bool) -> R<(It, bool)> {
        if s.text() == "not" {
            let v = self.primary(false, in_with, in_fn, in_method, in_tell, false, vars, funcs, true, eval)?;
            let v = v.ok_or_else(|| CompileError { line: self.line(), message: "Expression expected".into() })?;
            return Ok((It::Not(b(v)), false));
        }
        let v = var(&s.text(), None, false);
        vars.push(v.clone());
        Ok((It::Var(v), true))
    }

    #[allow(clippy::too_many_arguments)]
    fn primary(&mut self, allow_empty: bool, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, remainder: bool, vars: &mut Vars, funcs: &mut Funcs, allow_call: bool, eval: &mut bool) -> R<Option<It>> {
        let _ = allow_empty;
        macro_rules! prim {
            ($rem:expr, $call:expr) => {
                self.primary(false, in_with, in_fn, in_method, in_tell, $rem, vars, funcs, $call, eval)?
                    .ok_or_else(|| CompileError { line: self.line(), message: "Expression expected".into() })?
            };
        }
        macro_rules! req {
            ($rem:expr) => {
                self.expression_req(in_with, in_fn, in_method, in_tell, $rem, vars, funcs, false, eval)?
            };
        }
        let mut member_or_call = false;
        let s = self.lex()?;
        let ret: Option<It> = match s.t {
            T::Preprocessor => {
                self.expected_type(&[T::ParentOpen])?;
                let r = match s.text().as_str() {
                    "constant" => {
                        let n = self.lex()?;
                        self.expected(&n, &[T::Integer])?;
                        let V::Int(i) = n.v else { unreachable!() };
                        It::UnresolvedConstant(i as u16)
                    }
                    "enumerate" => It::Enumerate(b(req!(remainder))),
                    "dup" => {
                        req!(remainder);
                        member_or_call = true;
                        It::Duplicate
                    }
                    "push" => It::Push(b(req!(remainder))),
                    "pop" => {
                        member_or_call = true;
                        It::Pop
                    }
                    "swap" => It::Swap,
                    "strict" => {
                        let n = self.lex()?;
                        self.expected(&n, &[T::Integer])?;
                        let V::Int(i) = n.v else { unreachable!() };
                        It::StrictMode(i)
                    }
                    "goto" => return err(self.line(), format!("Compiling §§{} is not available, sorry", s.text())),
                    other => return err(self.line(), format!("Unknown preprocessor instruction: §§{other}")),
                };
                self.expected_type(&[T::ParentClose])?;
                Some(r)
            }
            T::Negate => {
                self.version_required(&s, 5)?;
                let v = prim!(false, true);
                Some(It::Bin { op: Bin::BitXor, l: b(v), r: b(It::Direct(Val::Double(4294967295.0))) })
            }
            T::Minus => {
                let n = self.lex()?;
                match n.v {
                    V::Double(d) if n.t == T::Double => Some(It::Direct(Val::Double(-d))),
                    V::Int(i) if n.t == T::Integer => Some(It::Direct(Val::Int(-i))),
                    _ => {
                        self.lx.pushback(n);
                        let num = prim!(true, true);
                        Some(match num {
                            It::Direct(Val::Int(i)) => It::Direct(Val::Int(-i)),
                            It::Direct(Val::Double(d)) => It::Direct(Val::Double(if d.is_infinite() { f64::NEG_INFINITY } else { -d })),
                            It::Direct(Val::Float(f)) => It::Direct(Val::Float(-f)),
                            other => It::Bin { op: Bin::Sub, l: b(It::Direct(Val::Int(0))), r: b(other) },
                        })
                    }
                }
            }
            T::TypeOf => {
                let v = prim!(false, true);
                member_or_call = true;
                Some(It::Op1 { value: b(v), code: 0x44, ret: true })
            }
            T::True => {
                member_or_call = true;
                Some(It::Direct(Val::Bool(true)))
            }
            T::Null => {
                member_or_call = true;
                Some(It::Direct(Val::Null))
            }
            T::Undefined => {
                member_or_call = true;
                Some(It::Direct(Val::Undefined))
            }
            T::False => {
                member_or_call = true;
                Some(It::Direct(Val::Bool(false)))
            }
            T::CurlyOpen => {
                let mut s = self.lex()?;
                let mut names = Vec::new();
                let mut values = Vec::new();
                while s.t != T::CurlyClose {
                    if s.t != T::Comma {
                        self.lx.pushback(s);
                    }
                    let n = self.lex()?;
                    self.expected_identifier(&n, &[])?;
                    names.insert(0, self.konst(&n.text()));
                    self.expected_type(&[T::Colon])?;
                    values.insert(0, req!(true));
                    s = self.lex()?;
                    if !s.is(&[T::Comma, T::CurlyClose]) {
                        self.expected(&s, &[T::Comma, T::CurlyClose])?;
                    }
                }
                member_or_call = true;
                Some(It::InitObject { names, values })
            }
            T::BracketOpen => {
                self.lx.pushback(s);
                let mut items = Vec::new();
                self.brackets(&mut items, in_with, in_fn, in_method, in_tell, vars, funcs, eval)?;
                member_or_call = true;
                Some(It::InitArray(items))
            }
            T::Function => {
                let n = self.lex()?;
                let name = if Self::is_identifier(&n, &[]) {
                    n.text()
                } else {
                    self.lx.pushback(n);
                    String::new()
                };
                member_or_call = true;
                Some(It::Function(self.function(true, &name, false, vars, funcs, in_tell, eval)?))
            }
            T::String => {
                member_or_call = true;
                Some(self.konst(&s.text()))
            }
            T::Newline => {
                member_or_call = true;
                Some(It::Direct(Val::Str("\n".into())))
            }
            T::Integer | T::Double => {
                member_or_call = true;
                Some(It::Direct(match s.v {
                    V::Int(i) => Val::Int(i),
                    V::Double(d) => Val::Double(d),
                    _ => unreachable!(),
                }))
            }
            T::Delete => {
                let target = prim!(false, true);
                Some(match target {
                    It::GetMember { obj, member, .. } => It::Delete { obj: Some(obj), name: member, getter: false },
                    It::Var(v) => {
                        let name = v.borrow().name.clone();
                        It::Delete { obj: None, name: b(self.konst(&name)), getter: false }
                    }
                    other => It::Delete { obj: None, name: b(other), getter: false },
                })
            }
            T::Increment | T::Decrement => {
                let v = prim!(false, true);
                Some(if s.t == T::Increment { It::PreInc(b(v)) } else { It::PreDec(b(v)) })
            }
            T::Not => Some(It::Not(b(prim!(false, true)))),
            T::ParentOpen => {
                let e = self.expression(in_with, in_fn, in_method, in_tell, true, vars, funcs, true, eval)?;
                let e = e.ok_or_else(|| CompileError { line: self.line(), message: "Expression expected".into() })?;
                self.expected_type(&[T::ParentClose])?;
                member_or_call = true;
                Some(It::Paren(b(e)))
            }
            T::New => {
                let target = prim!(false, false);
                let r = match target {
                    It::Op1 { value, code: 0x4a, .. } => It::NewObject { name: b(self.konst("Number")), args: vec![*value], resolved: None },
                    It::Op1 { value, code: 0x4b, .. } => It::NewObject { name: b(self.konst("String")), args: vec![*value], resolved: None },
                    It::GetMember { obj, member, .. } => {
                        self.expected_type(&[T::ParentOpen])?;
                        let args = self.call(in_with, in_fn, in_method, in_tell, vars, funcs, eval)?;
                        It::NewMethod { obj, name: member, args, getter: false }
                    }
                    It::Var(v) => {
                        self.expected_type(&[T::ParentOpen])?;
                        let args = self.call(in_with, in_fn, in_method, in_tell, vars, funcs, eval)?;
                        let name = v.borrow().name.clone();
                        It::NewObject { name: b(self.konst(&name)), args, resolved: None }
                    }
                    _ => return err(self.line(), "Invalid new item"),
                };
                member_or_call = true;
                Some(r)
            }
            T::Eval => {
                self.expected_type(&[T::ParentOpen])?;
                let v = req!(true);
                self.expected_type(&[T::ParentClose])?;
                *eval = true;
                member_or_call = true;
                Some(It::Op1 { value: b(v), code: 0x1c, ret: true })
            }
            T::Identifier | T::This | T::Super => {
                let (it, m) = self.handle_variable(&s, vars, in_with, in_fn, in_method, in_tell, funcs, eval)?;
                member_or_call = m;
                Some(it)
            }
            _ => {
                let mut global_var = false;
                let mut r = None;
                if s.group == Group::GlobalFunc && self.peek()?.t != T::ParentOpen {
                    let (it, m) = self.handle_variable(&s, vars, in_with, in_fn, in_method, in_tell, funcs, eval)?;
                    member_or_call = m;
                    r = Some(it);
                    global_var = true;
                }
                if !global_var {
                    match self.expression_commands(&s, in_with, in_fn, in_method, in_tell, vars, funcs, eval)? {
                        Some(c) => {
                            r = Some(c);
                            member_or_call = true;
                        }
                        None => self.lx.pushback(s),
                    }
                }
                r
            }
        };
        match ret {
            Some(r) if member_or_call => Ok(Some(self.member_or_call(r, in_with, in_fn, in_method, in_tell, vars, funcs, allow_call, eval)?)),
            other => Ok(other),
        }
    }

    fn is_cast_op(&self, it: &It) -> bool {
        let pool = self.pool.borrow();
        let mut items = Vec::new();
        let mut cur = it;
        while let It::GetMember { obj, member, .. } = cur {
            if let It::Direct(v) = member.as_ref() {
                items.insert(0, pool.string(v).unwrap_or_else(|| val_text(v)));
            }
            cur = obj;
        }
        if let It::Var(v) = cur {
            items.insert(0, v.borrow().name.clone());
        }
        if items.is_empty() {
            return false;
        }
        let full = items.join(".");
        BUILTIN_CASTS.contains(&full.as_str()) || self.classes.contains(&full)
    }

    #[allow(clippy::too_many_arguments)]
    fn member_or_call(&mut self, mut ret: It, in_with: bool, in_fn: bool, in_method: bool, in_tell: bool, vars: &mut Vars, funcs: &mut Funcs, allow_call: bool, eval: &mut bool) -> R<It> {
        let mut op = self.lex()?;
        while op.is(&[T::ParentOpen, T::BracketOpen, T::Dot]) {
            if op.t == T::ParentOpen {
                if !allow_call {
                    break;
                }
                let args = self.call(in_with, in_fn, in_method, in_tell, vars, funcs, eval)?;
                ret = if self.is_cast_op(&ret) && args.len() == 1 {
                    It::CastOp { ctor: b(ret), obj: b(args.into_iter().next().unwrap()) }
                } else {
                    match ret {
                        It::GetMember { obj, member, .. } => It::CallMethod { obj, name: member, args, getter: false },
                        It::Var(v) => {
                            let name = v.borrow().name.clone();
                            let prop = |a: &It| match a {
                                It::Var(p) => property_index(&p.borrow().name),
                                _ => None,
                            };
                            if name == "getProperty" && args.len() == 2 && prop(&args[1]).is_some() {
                                let i = prop(&args[1]).unwrap();
                                It::GetProperty { target: b(args.into_iter().next().unwrap()), index: i }
                            } else if name == "setProperty" && args.len() == 3 && prop(&args[1]).is_some() {
                                let i = prop(&args[1]).unwrap();
                                let mut it = args.into_iter();
                                let t = it.next().unwrap();
                                it.next();
                                It::SetProperty { target: b(t), index: i, value: b(it.next().unwrap()) }
                            } else {
                                It::CallFunction { name: b(It::Var(v)), args }
                            }
                        }
                        It::Op1 { value, code: 0x1c, .. } => It::CallFunction { name: value, args },
                        other => It::CallFunction { name: b(other), args },
                    }
                };
            }
            if op.t == T::BracketOpen {
                let rhs = self.expression_req(in_with, in_fn, in_method, in_tell, false, vars, funcs, false, eval)?;
                ret = It::GetMember { obj: b(ret), member: b(rhs), getter: false };
                self.expected_type(&[T::BracketClose])?;
            }
            if op.t == T::Dot {
                let s = self.lex()?;
                self.expected_identifier(&s, &[T::This, T::Super])?;
                ret = It::GetMember { obj: b(ret), member: b(self.konst(&s.text())), getter: false };
            }
            op = self.lex()?;
        }
        match op.t {
            T::Increment | T::Decrement => {
                if !matches!(ret, It::Var(_) | It::GetMember { .. }) {
                    return err(self.line(), "Invalid assignment");
                }
                ret = if op.t == T::Increment { It::PostInc(b(ret)) } else { It::PostDec(b(ret)) };
                op = self.lex()?;
            }
            _ => {}
        }
        self.lx.pushback(op);
        Ok(ret)
    }
}

fn val_text(v: &Val) -> String {
    match v {
        Val::Str(s) => s.clone(),
        Val::Int(i) => format!("{}.0", i),
        Val::Double(d) => d.to_string(),
        _ => String::new(),
    }
}

/// The innermost `GetMember` of a member chain (`getFirstGetMember`).
fn first_get_member(it: &mut It) -> Option<&mut It> {
    let inner_is_member = matches!(it, It::GetMember { obj, .. } if matches!(obj.as_ref(), It::GetMember { .. }));
    if !matches!(it, It::GetMember { .. }) {
        return None;
    }
    if inner_is_member {
        let It::GetMember { obj, .. } = it else { unreachable!() };
        return first_get_member(obj);
    }
    Some(it)
}

/// `boxResolvedMember`: a static member used without its class.
pub fn box_resolved(v: &VarRef) -> bool {
    let (resolved, store) = {
        let b = v.borrow();
        (b.resolved.clone(), b.store.clone())
    };
    let Some((obj, member)) = resolved else { return false };
    let boxed = match store {
        None => It::GetMember { obj: b(obj), member: b(member), getter: false },
        Some(s) => It::SetMember { obj: b(obj), member: b(member), value: b(s), setter: false },
    };
    v.borrow_mut().boxed = Some(boxed);
    true
}
