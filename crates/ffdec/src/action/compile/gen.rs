//! `ActionSourceGenerator` and the `toSource` of every model item: items to actions.

use std::collections::{BTreeSet, HashMap};

use super::parser::{box_resolved, Parsed, PoolRef};
use super::*;
use crate::action::{DefineFunction2 as Df2, Op, Try as TryOp, Value};

/// A generated action, with the break/continue marks `fixLoop` resolves.
#[derive(Debug, Clone)]
pub struct GA {
    pub a: Action,
    pub brk: bool,
    pub cont: bool,
}

type L = Vec<GA>;

fn ga(code: u8, op: Op) -> GA {
    GA { a: Action::new(code, op), brk: false, cont: false }
}

fn simple(code: u8) -> GA {
    ga(code, Op::Simple)
}

fn push(values: Vec<Value>) -> GA {
    ga(0x96, Op::Push { values, pool: None })
}

fn push1(v: Value) -> GA {
    push(vec![v])
}

fn jump(offset: i32) -> GA {
    ga(0x99, Op::Jump { offset, deobfuscate: false })
}

fn iff(offset: i32) -> GA {
    ga(0x9d, Op::If { offset })
}

fn store_reg(r: usize) -> GA {
    ga(0x87, Op::StoreRegister(r as u8))
}

fn undef2() -> GA {
    push(vec![Value::Undefined, Value::Undefined])
}

const POP: u8 = 0x17;
const NOT: u8 = 0x12;

fn value_of(v: &Val) -> Value {
    match v {
        Val::Str(s) => Value::Str(s.clone()),
        Val::Const(i) => Value::Constant(*i),
        Val::Int(i) => Value::Int(*i),
        Val::Double(d) => Value::Double(*d),
        Val::Float(f) => Value::Float(*f),
        Val::Bool(b) => Value::Bool(*b),
        Val::Null => Value::Null,
        Val::Undefined => Value::Undefined,
        Val::Reg(r) => Value::Register(*r),
    }
}

/// `SourceGeneratorLocalData`.
#[derive(Clone, Default)]
pub struct Ld {
    pub regs: HashMap<String, usize>,
    pub in_function: i32,
    pub in_method: bool,
    pub forin: i32,
}

pub enum Out {
    Done(Vec<Action>),
    Again(Vec<String>),
}

pub struct Gen {
    version: u8,
    cs: Charset,
    pool: PoolRef,
    uniq: u64,
}

fn cerr<T>(line: usize, m: &str) -> R<T> {
    Err(CompileError { line, message: m.into() })
}

/// `actionsFromTree`.
pub fn actions(p: &Parsed, version: u8, cs: Charset, order: bool) -> R<Out> {
    let mut g = Gen { version, cs, pool: p.pool.clone(), uniq: 0 };
    let mut ld = Ld::default();
    let mut list = g.generate(&mut ld, &p.tree)?;
    let mut pool = p.pool.borrow().list.clone();
    if order {
        let in_place = pool.len().saturating_sub(1) <= 255 || pool.is_empty();
        let mut ordered: Vec<String> = Vec::new();
        for x in &mut list {
            if let Op::Push { values, .. } = &mut x.a.op {
                for v in values.iter_mut() {
                    if let Value::Constant(i) = v {
                        let s = pool[*i as usize].clone();
                        let at = match ordered.iter().position(|o| *o == s) {
                            Some(at) => at,
                            None => {
                                ordered.push(s);
                                ordered.len() - 1
                            }
                        };
                        if in_place {
                            *v = Value::Constant(at as u16);
                        }
                    }
                }
            }
        }
        if !in_place {
            return Ok(Out::Again(ordered));
        }
        pool = ordered;
    }
    let mut out: Vec<Action> = list.into_iter().map(|x| x.a).collect();
    if !pool.is_empty() {
        out.insert(0, Action::new(0x88, Op::ConstantPool(pool)));
    }
    Ok(Out::Done(out))
}

impl Gen {
    fn len(&self, l: &[GA]) -> usize {
        l.iter().map(|x| x.a.bytes_len(self.cs) as usize).sum()
    }

    fn alen(&self, a: &GA) -> usize {
        a.a.bytes_len(self.cs) as usize
    }

    /// `pushConst`.
    fn konst(&self, s: &str) -> GA {
        push1(value_of(&self.pool.borrow_mut().konst(s)))
    }

    fn konst_item(&self, s: &str) -> It {
        It::Direct(self.pool.borrow_mut().konst(s))
    }

    fn string_of(&self, it: &It) -> Option<String> {
        match it {
            It::Direct(v) => self.pool.borrow().string(v),
            _ => None,
        }
    }

    fn temp(&self, ld: &mut Ld) -> usize {
        for r in 0..256 {
            if !ld.regs.values().any(|&v| v == r) {
                ld.regs.insert(format!("__temp{r}"), r);
                return r;
            }
        }
        0
    }

    fn release(&self, ld: &mut Ld, r: usize) {
        ld.regs.remove(&format!("__temp{r}"));
    }

    /// `generate(List)`: every command, discarding values.
    pub fn generate(&mut self, ld: &mut Ld, cmds: &[It]) -> R<L> {
        let mut out = Vec::new();
        for c in cmds {
            out.extend(self.ignore(ld, c)?);
        }
        Ok(out)
    }

    fn merge(&mut self, ld: &mut Ld, parts: Vec<Part>) -> R<L> {
        let mut out = Vec::new();
        for p in parts {
            match p {
                Part::I(it) => out.extend(self.src(ld, it)?),
                Part::A(a) => out.push(a),
                Part::L(l) => out.extend(l),
            }
        }
        Ok(out)
    }

    fn has_ret(&self, it: &It) -> bool {
        match it {
            It::Var(v) => !v.borrow().definition,
            It::Paren(v) => self.has_ret(v),
            It::Direct(_) | It::GetVariable(_) | It::GetMember { .. } | It::GetProperty { .. } | It::CallFunction { .. } | It::CallMethod { .. }
            | It::NewObject { .. } | It::NewMethod { .. } | It::Delete { .. } | It::Bin { .. } | It::And(..) | It::Or(..) | It::Not(_)
            | It::Ternary(..) | It::PreInc(_) | It::PreDec(_) | It::PostInc(_) | It::PostDec(_) | It::InitArray(_) | It::InitObject { .. }
            | It::GetTime | It::GetVersion | It::Extract { .. } | It::CastOp { .. } | It::UnresolvedConstant(_) | It::Duplicate | It::Pop
            | It::True | It::StoreRegister { .. } | It::FsCommand2(_) => true,
            It::Op1 { ret, .. } => *ret,
            _ => false,
        }
    }

    /// `toSourceIgnoreReturnValue`.
    fn ignore(&mut self, ld: &mut Ld, it: &It) -> R<L> {
        Ok(match it {
            It::Var(v) => {
                let boxed = v.borrow().boxed.clone();
                match boxed {
                    Some(b) => self.ignore(ld, &b)?,
                    None => Vec::new(),
                }
            }
            It::Cmd0(c) => self.cmd0(*c, false),
            It::Trace(v) => self.merge(ld, vec![Part::I(v), Part::A(simple(0x26))])?,
            It::RemoveSprite(v) => self.merge(ld, vec![Part::I(v), Part::A(simple(0x25))])?,
            It::GetUrl2 { .. } | It::GotoFrame2 { .. } | It::Load { .. } | It::UnloadMovie(_) | It::UnloadMovieNum(_) | It::Print { .. }
            | It::StartDrag { .. } | It::FsCommand { .. } => self.with_ret(ld, it, false)?,
            It::CloneSprite { src, target, depth } => self.merge(ld, vec![Part::I(src), Part::I(target), Part::I(depth), Part::A(simple(0x24))])?,
            It::SetMember { obj, member, value, setter } => {
                if *setter {
                    let mut l = self.src(ld, it)?;
                    l.push(simple(POP));
                    l
                } else {
                    self.merge(ld, vec![Part::I(obj), Part::I(member), Part::I(value), Part::A(simple(0x4f))])?
                }
            }
            It::SetProperty { target, index, value } => self.merge(ld, vec![Part::I(target), Part::A(push1(Value::Int(*index))), Part::I(value), Part::A(simple(0x23))])?,
            It::SetVariable { name, value, .. } => self.merge(ld, vec![Part::I(name), Part::I(value), Part::A(simple(0x1d))])?,
            It::PreInc(v) => self.pre(ld, v, false, 0x50)?,
            It::PreDec(v) => self.pre(ld, v, false, 0x51)?,
            It::PostInc(v) => self.post_ignore(ld, v, 0x50)?,
            It::PostDec(v) => self.post_ignore(ld, v, 0x51)?,
            It::Comma(items) => self.comma(ld, items, false)?,
            _ => {
                let mut l = self.src(ld, it)?;
                if self.has_ret(it) {
                    l.push(simple(POP));
                }
                l
            }
        })
    }

    fn cmd0(&self, c: Cmd0, needs_return: bool) -> L {
        let code = match c {
            Cmd0::NextFrame => 0x04,
            Cmd0::Play => 0x06,
            Cmd0::PrevFrame => 0x05,
            Cmd0::Stop => 0x07,
            Cmd0::StopAllSounds => 0x09,
            Cmd0::ToggleHighQuality => 0x08,
            Cmd0::StopDrag => 0x28,
        };
        let mut l = vec![simple(code)];
        if needs_return {
            l.push(undef2());
        }
        l
    }

    fn level(&mut self, num: &It) -> It {
        match num {
            It::Direct(Val::Int(n)) => self.konst_item(&format!("_level{n}")),
            other => It::Bin { op: Bin::StrAdd, l: Box::new(self.konst_item("_level")), r: Box::new(other.clone()) },
        }
    }

    fn geturl2(method: u8, load_variables: bool, load_target: bool) -> GA {
        ga(0x9a, Op::GetUrl2 { load_variables, load_target, method })
    }

    /// The commands that, used as values, push two undefineds.
    fn with_ret(&mut self, ld: &mut Ld, it: &It, needs: bool) -> R<L> {
        let tail = |l: &mut L| {
            if needs {
                l.push(undef2());
            }
        };
        let mut l = match it {
            It::GetUrl2 { url, target, method } => self.merge(ld, vec![Part::I(url), Part::I(target), Part::A(Self::geturl2(*method, false, false))])?,
            It::GotoFrame2 { frame, bias_flag, play, bias } => {
                let label = if *bias_flag { None } else { self.string_of(frame) };
                if let Some(label) = label {
                    let mut l = vec![ga(0x8c, Op::GoToLabel(label))];
                    if *play {
                        l.push(simple(0x06));
                    }
                    l
                } else if let (false, It::Direct(Val::Int(n))) = (*bias_flag, frame.as_ref()) {
                    let mut l = vec![ga(0x81, Op::GotoFrame((*n - 1) as u16))];
                    if *play {
                        l.push(simple(0x06));
                    }
                    l
                } else {
                    let mut l = self.src(ld, frame)?;
                    l.push(ga(0x9f, Op::GotoFrame2 { scene_bias_flag: *bias_flag, play: *play, scene_bias: *bias as u16 }));
                    l
                }
            }
            It::Load { kind, url, target, method } => {
                let (lv, lt) = match kind {
                    LoadKind::Movie => (false, true),
                    LoadKind::Variables => (true, true),
                    LoadKind::MovieNum => (false, false),
                    LoadKind::VariablesNum => (true, false),
                };
                let target = if matches!(kind, LoadKind::MovieNum | LoadKind::VariablesNum) { self.level(target) } else { (**target).clone() };
                self.merge(ld, vec![Part::I(url), Part::I(&target), Part::A(Self::geturl2(*method, lv, lt))])?
            }
            It::UnloadMovie(target) => self.merge(ld, vec![Part::A(push1(Value::Str(String::new()))), Part::I(target), Part::A(Self::geturl2(0, false, true))])?,
            It::UnloadMovieNum(num) => match num.as_ref() {
                It::Direct(Val::Int(n)) => vec![ga(0x83, Op::GetUrl { url: String::new(), target: format!("_level{n}") })],
                other => {
                    let lev = It::Bin { op: Bin::StrAdd, l: Box::new(self.konst_item("_level")), r: Box::new(other.clone()) };
                    self.merge(ld, vec![Part::A(push1(Value::Str(String::new()))), Part::I(&lev), Part::A(Self::geturl2(0, false, false))])?
                }
            },
            It::Print { kind, target, bbox } => {
                let bitmap = matches!(kind, PrintKind::AsBitmap | PrintKind::AsBitmapNum);
                let prefix = if bitmap { "printasbitmap:" } else { "print:" };
                let target = if matches!(kind, PrintKind::PrintNum | PrintKind::AsBitmapNum) { self.level(target) } else { (**target).clone() };
                if self.string_of(bbox).as_deref() == Some("bmovie") {
                    let p = self.konst_item(prefix);
                    self.merge(ld, vec![Part::I(&p), Part::I(&target), Part::A(Self::geturl2(0, false, false))])?
                } else {
                    let add = It::Bin { op: Bin::Add2, l: Box::new(self.konst_item(&format!("{prefix}#"))), r: bbox.clone() };
                    self.merge(ld, vec![Part::I(&add), Part::I(&target), Part::A(Self::geturl2(0, false, false))])?
                }
            }
            It::StartDrag { target, lock, constrain, x1, y1, x2, y2 } => {
                let no_constrain = matches!(constrain.as_ref(), It::Direct(v) if to_number(v) == 0.0);
                let mut parts = Vec::new();
                if !no_constrain {
                    for x in [x1, y1, x2, y2].into_iter().flatten() {
                        parts.push(Part::I(x));
                    }
                }
                parts.extend([Part::I(constrain), Part::I(lock), Part::I(target), Part::A(simple(0x27))]);
                self.merge(ld, parts)?
            }
            It::FsCommand { cmd, param } => {
                let cmd_str = self.string_of(cmd);
                let param_str = param.as_ref().map(|p| self.string_of(p));
                if let (Some(c), None | Some(Some(_))) = (&cmd_str, &param_str) {
                    let target = param_str.flatten().unwrap_or_default();
                    return Ok(vec![ga(0x83, Op::GetUrl { url: format!("FSCommand:{c}"), target })]);
                }
                let add = It::Bin { op: Bin::StrAdd, l: Box::new(self.konst_item("FSCommand:")), r: cmd.clone() };
                let p = match param {
                    Some(p) => (**p).clone(),
                    None => self.konst_item(""),
                };
                self.merge(ld, vec![Part::I(&add), Part::I(&p), Part::A(Self::geturl2(1, false, false))])?
            }
            _ => unreachable!(),
        };
        tail(&mut l);
        Ok(l)
    }

    /// `toSource`.
    pub fn src(&mut self, ld: &mut Ld, it: &It) -> R<L> {
        let a = |code: u8| Part::A(simple(code));
        Ok(match it {
            It::Direct(v) => vec![push1(value_of(v))],
            It::Var(v) => {
                let boxed = v.borrow().boxed.clone();
                match boxed {
                    Some(b) => self.src(ld, &b)?,
                    None => Vec::new(),
                }
            }
            It::GetVariable(n) => self.merge(ld, vec![Part::I(n), a(0x1c)])?,
            It::SetVariable { name, value, force_set } => {
                if *force_set {
                    self.merge(ld, vec![Part::I(name), Part::I(value), a(0x1d), Part::A(push1(Value::Undefined))])?
                } else {
                    let t = self.temp(ld);
                    let r = self.merge(ld, vec![Part::I(name), Part::I(value), Part::A(store_reg(t)), a(0x1d), Part::A(push1(Value::Register(t as u8)))]);
                    self.release(ld, t);
                    r?
                }
            }
            It::DefineLocal { name, value } => match value.as_deref() {
                None | Some(It::Direct(Val::Undefined)) => self.merge(ld, vec![Part::I(name), a(0x41)])?,
                Some(v) => self.merge(ld, vec![Part::I(name), Part::I(v), a(0x3c)])?,
            },
            It::GetMember { obj, member, getter } => {
                if *getter {
                    let name = self.string_of(member).unwrap_or_default();
                    let k = self.konst(&format!("__get__{name}"));
                    self.merge(ld, vec![Part::A(push1(Value::Int(0))), Part::I(obj), Part::A(k), a(0x52)])?
                } else {
                    self.merge(ld, vec![Part::I(obj), Part::I(member), a(0x4e)])?
                }
            }
            It::SetMember { obj, member, value, setter } => {
                if *setter {
                    let name = self.string_of(member).unwrap_or_default();
                    let k = self.konst(&format!("__set__{name}"));
                    self.merge(ld, vec![Part::I(value), Part::A(push1(Value::Int(1))), Part::I(obj), Part::A(k), a(0x52)])?
                } else {
                    let t = self.temp(ld);
                    let r = self.merge(ld, vec![Part::I(obj), Part::I(member), Part::I(value), Part::A(store_reg(t)), a(0x4f), Part::A(push1(Value::Register(t as u8)))]);
                    self.release(ld, t);
                    r?
                }
            }
            It::GetProperty { target, index } => self.merge(ld, vec![Part::I(target), Part::A(push1(Value::Int(*index))), a(0x22)])?,
            It::SetProperty { target, index, value } => {
                let t = self.temp(ld);
                let r = self.merge(ld, vec![Part::I(target), Part::A(push1(Value::Int(*index))), Part::I(value), Part::A(store_reg(t)), a(0x23), Part::A(push1(Value::Register(t as u8)))]);
                self.release(ld, t);
                r?
            }
            It::CallFunction { name, args } => {
                let call = self.call_args(ld, args)?;
                if let It::Var(v) = name.as_ref() {
                    let (boxed, vname) = {
                        let b = v.borrow();
                        (b.boxed.clone(), b.name.clone())
                    };
                    match boxed {
                        Some(It::GetMember { obj, member, .. }) => {
                            let cm = It::CallMethod { obj, name: member, args: args.clone(), getter: false };
                            return self.src(ld, &cm);
                        }
                        Some(It::Direct(Val::Reg(r))) => {
                            return self.merge(ld, vec![Part::L(call), Part::A(push1(Value::Register(r))), Part::A(push1(Value::Undefined)), a(0x52)]);
                        }
                        _ => {
                            let k = self.konst(&vname);
                            return self.merge(ld, vec![Part::L(call), Part::A(k), a(0x3d)]);
                        }
                    }
                }
                self.merge(ld, vec![Part::L(call), Part::I(name), a(0x3d)])?
            }
            It::CallMethod { obj, name, args, getter } => {
                let call = self.call_args(ld, args)?;
                if *getter {
                    let n = self.string_of(name).unwrap_or_default();
                    let k = self.konst(&format!("__get__{n}"));
                    self.merge(ld, vec![Part::L(call), Part::A(push1(Value::Int(0))), Part::I(obj), Part::A(k), a(0x52), Part::A(push1(Value::Undefined)), a(0x52)])?
                } else {
                    self.merge(ld, vec![Part::L(call), Part::I(obj), Part::I(name), a(0x52)])?
                }
            }
            It::NewObject { name, args, resolved } => {
                if let Some(r) = resolved {
                    let nm = It::NewMethod { obj: r.clone(), name: name.clone(), args: args.clone(), getter: false };
                    return self.src(ld, &nm);
                }
                let call = self.call_args(ld, args)?;
                self.merge(ld, vec![Part::L(call), Part::I(name), a(0x40)])?
            }
            It::NewMethod { obj, name, args, getter } => {
                let call = self.call_args(ld, args)?;
                if *getter {
                    let n = self.string_of(name).unwrap_or_default();
                    let k = self.konst(&format!("__get__{n}"));
                    self.merge(ld, vec![Part::L(call), Part::A(push1(Value::Int(0))), Part::I(obj), Part::A(k), a(0x52), Part::A(push1(Value::Undefined)), a(0x53)])?
                } else {
                    self.merge(ld, vec![Part::L(call), Part::I(obj), Part::I(name), a(0x53)])?
                }
            }
            It::Delete { obj, name, getter } => {
                if *getter {
                    let n = self.string_of(name).unwrap_or_default();
                    let k = self.konst(&format!("__get__{n}"));
                    let obj: Part = match obj {
                        Some(o) => Part::I(o),
                        None => Part::L(Vec::new()),
                    };
                    self.merge(ld, vec![Part::A(push1(Value::Int(0))), obj, Part::A(k), a(0x52), a(0x3b)])?
                } else {
                    match obj {
                        None => self.merge(ld, vec![Part::I(name), a(0x3b)])?,
                        Some(o) => self.merge(ld, vec![Part::I(o), Part::I(name), a(0x3a)])?,
                    }
                }
            }
            It::Function(f) => self.function(ld, f)?,
            It::Op1 { value, code, .. } => self.merge(ld, vec![Part::I(value), a(*code)])?,
            It::Bin { op, l, r } => self.bin(ld, *op, l, r)?,
            It::And(l, r) => {
                let mut ret = self.src(ld, l)?;
                ret.push(simple(0x4c));
                ret.push(simple(NOT));
                let mut rhs = self.src(ld, r)?;
                rhs.insert(0, simple(POP));
                ret.push(iff(self.len(&rhs) as i32));
                ret.extend(rhs);
                ret
            }
            It::Or(l, r) => {
                let mut ret = self.src(ld, l)?;
                ret.push(simple(0x4c));
                let mut rhs = self.src(ld, r)?;
                rhs.insert(0, simple(POP));
                ret.push(iff(self.len(&rhs) as i32));
                ret.extend(rhs);
                ret
            }
            It::Not(v) => self.merge(ld, vec![Part::I(v), a(NOT)])?,
            It::Ternary(c, x, y) => self.gen_if(ld, c, std::slice::from_ref(x), Some(std::slice::from_ref(y)), true, 0, "Ternar")?,
            It::Paren(v) => self.src(ld, v)?,
            It::Comma(items) => self.comma(ld, items, true)?,
            It::PreInc(v) => self.pre(ld, v, true, 0x50)?,
            It::PreDec(v) => self.pre(ld, v, true, 0x51)?,
            It::PostInc(v) | It::PostDec(v) => {
                let code = if matches!(it, It::PostInc(_)) { 0x50 } else { 0x51 };
                let mut l = self.src(ld, v)?;
                l.extend(self.post_ignore(ld, v, code)?);
                l
            }
            It::InitArray(values) => {
                let call = self.call_args(ld, values)?;
                self.merge(ld, vec![Part::L(call), a(0x42)])?
            }
            It::InitObject { names, values } => {
                let mut ret = Vec::new();
                for i in (0..values.len()).rev() {
                    ret.extend(self.src(ld, &names[i])?);
                    ret.extend(self.src(ld, &values[i])?);
                }
                ret.push(push1(Value::Int(values.len() as i64)));
                ret.push(simple(0x43));
                ret
            }
            It::Trace(v) => self.merge(ld, vec![Part::I(v), a(0x26), Part::A(undef2())])?,
            It::RemoveSprite(v) => self.merge(ld, vec![Part::I(v), a(0x25), Part::A(undef2())])?,
            It::Cmd0(c) => self.cmd0(*c, true),
            It::GetUrl { url, target } => vec![ga(0x83, Op::GetUrl { url: url.clone(), target: target.clone() })],
            It::GetUrl2 { .. } | It::GotoFrame2 { .. } | It::Load { .. } | It::UnloadMovie(_) | It::UnloadMovieNum(_) | It::Print { .. }
            | It::StartDrag { .. } | It::FsCommand { .. } => self.with_ret(ld, it, true)?,
            It::FsCommand2(args) => {
                let mut ret = Vec::new();
                for x in args {
                    ret.extend(self.src(ld, x)?);
                }
                ret.push(push1(Value::Int(args.len() as i64)));
                ret.push(simple(0x2d));
                ret
            }
            It::CloneSprite { src, target, depth } => self.merge(ld, vec![Part::I(src), Part::I(target), Part::I(depth), a(0x24), Part::A(undef2())])?,
            It::GetTime => vec![simple(0x34)],
            It::GetVersion => vec![push1(Value::Str("/:$version".into())), simple(0x1c)],
            It::Extract { value, index, count, mb } => self.merge(ld, vec![Part::I(value), Part::I(index), Part::I(count), a(if *mb { 0x35 } else { 0x15 })])?,
            It::CastOp { ctor, obj } => self.merge(ld, vec![Part::I(ctor), Part::I(obj), a(0x2b)])?,
            It::Enumerate(v) => self.merge(ld, vec![Part::I(v), a(0x55)])?,
            It::UnresolvedConstant(i) => vec![push1(Value::Constant(*i))],
            It::Duplicate => vec![simple(0x4c)],
            It::Push(v) => self.src(ld, v)?,
            It::Pop => Vec::new(),
            It::Swap => vec![simple(0x4d)],
            It::StrictMode(m) => vec![ga(0x89, Op::StrictMode(*m as u8))],
            It::True => vec![push1(Value::Bool(true))],
            It::StoreRegister { reg, value } => self.merge(ld, vec![Part::I(value), Part::A(store_reg(*reg as usize))])?,
            It::If { cond, on_true, on_false, line } => self.gen_if(ld, cond, on_true, on_false.as_deref(), false, *line, "If")?,
            It::While { cond, body, line } => self.gen_while(ld, cond, body, *line)?,
            It::DoWhile { body, cond, line } => self.gen_do_while(ld, body, cond, *line)?,
            It::For { first, cond, last, body, line } => self.gen_for(ld, first, cond, last, body, *line)?,
            It::ForIn { var, collection, body, line } => self.gen_for_in(ld, var, collection, body, *line)?,
            It::Switch { expr, values, cmds, mapping, line } => self.gen_switch(ld, expr, values, cmds, mapping, *line)?,
            It::Break => vec![GA { brk: true, ..jump(0) }],
            It::Continue => vec![GA { cont: true, ..jump(0) }],
            It::Return(v) => {
                let mut ret = Vec::new();
                for _ in 0..ld.forin {
                    let mut l = vec![push1(Value::Null), simple(0x49), simple(NOT)];
                    let f = iff(0);
                    let total = self.len(&l) + self.alen(&f);
                    l.push(GA { a: Action::new(0x9d, Op::If { offset: -(total as i32) }), ..f });
                    ret.extend(l);
                }
                match v {
                    None => ret.push(push1(Value::Undefined)),
                    Some(v) => ret.extend(self.src(ld, v)?),
                }
                ret.push(simple(0x3e));
                ret
            }
            It::Throw(v) => self.merge(ld, vec![Part::I(v), a(0x2a)])?,
            It::Try { body, names, types, catches, finally } => self.gen_try(ld, body, names, types, catches, finally.as_deref())?,
            It::With { scope, body } => {
                let data = self.generate(ld, body)?;
                let len = self.len(&data);
                self.merge(ld, vec![Part::I(scope), Part::A(ga(0x94, Op::With { code_size: len as u32 })), Part::L(data)])?
            }
            It::TellTarget { target, body, nested } => {
                let mut ret = Vec::new();
                if *nested {
                    ret.push(push1(Value::Str(String::new())));
                    ret.push(push1(Value::Int(11)));
                    ret.push(simple(0x22));
                }
                match target.as_ref() {
                    It::Direct(v @ (Val::Str(_) | Val::Const(_))) => ret.push(ga(0x8b, Op::SetTarget(self.pool.borrow().string(v).unwrap_or_default()))),
                    t => {
                        ret.extend(self.src(ld, t)?);
                        ret.push(simple(0x20));
                    }
                }
                ret.extend(self.generate(ld, body)?);
                ret.push(ga(0x8b, Op::SetTarget(String::new())));
                if *nested {
                    ret.push(simple(0x20));
                }
                ret
            }
            It::IfFrameLoaded { frame, body, line } => {
                let b = self.generate(ld, body)?;
                if b.len() > 0xff {
                    return cerr(*line, "ifFrameLoaded body exceeds limit of 255 actions");
                }
                self.merge(ld, vec![Part::I(frame), Part::A(ga(0x8d, Op::WaitForFrame2 { skip: b.len() })), Part::L(b)])?
            }
            It::Block(cmds) => self.generate(ld, cmds)?,
            It::Empty => Vec::new(),
            It::Class { name, extends, implements, traits, statics } => {
                let mut ld2 = ld.clone();
                ld2.in_method = true;
                self.traits(&mut ld2, false, name, extends.as_deref(), implements, traits, statics)?
            }
            It::Interface { name, extends } => self.traits(ld, true, name, None, extends, &[], &[])?,
        })
    }

    fn call_args(&mut self, ld: &mut Ld, args: &[It]) -> R<L> {
        let mut ret = Vec::new();
        for a in args {
            let mut x = self.src(ld, a)?;
            x.extend(ret);
            ret = x;
        }
        ret.push(push1(Value::Int(args.len() as i64)));
        Ok(ret)
    }

    fn comma(&mut self, ld: &mut Ld, items: &[It], value: bool) -> R<L> {
        let Some((last, cmds)) = items.split_last() else { return Ok(Vec::new()) };
        let mut ret = self.generate(ld, cmds)?;
        ret.extend(if value { self.src(ld, last)? } else { self.ignore(ld, last)? });
        Ok(ret)
    }

    fn bin(&mut self, ld: &mut Ld, op: Bin, l: &It, r: &It) -> R<L> {
        let v = self.version;
        let less = if v >= 5 { 0x48 } else { 0x0f };
        let (swap, codes): (bool, Vec<u8>) = match op {
            Bin::Add => (false, vec![0x0a]),
            Bin::Add2 => (false, vec![0x47]),
            Bin::Sub => (false, vec![0x0b]),
            Bin::Mul => (false, vec![0x0c]),
            Bin::Div => (false, vec![0x0d]),
            Bin::Mod => (false, vec![0x3f]),
            Bin::Eq => (false, vec![0x49]),
            Bin::StrictEq => (false, vec![0x66]),
            Bin::Neq => (false, vec![0x49, NOT]),
            Bin::StrictNeq => (false, vec![0x66, NOT]),
            Bin::Lt => (false, vec![less]),
            Bin::Ge => (false, vec![less, NOT]),
            Bin::Gt if v >= 6 => (false, vec![0x67]),
            Bin::Gt => (true, vec![less]),
            Bin::Le if v >= 6 => (false, vec![0x67, NOT]),
            Bin::Le => (true, vec![less, NOT]),
            Bin::FullAnd => (false, vec![0x10]),
            Bin::FullOr => (false, vec![0x11]),
            Bin::BitAnd => (false, vec![0x60]),
            Bin::BitOr => (false, vec![0x61]),
            Bin::BitXor => (false, vec![0x62]),
            Bin::Shl => (false, vec![0x63]),
            Bin::Shr => (false, vec![0x64]),
            Bin::UShr => (false, vec![0x65]),
            Bin::InstanceOf => (false, vec![0x54]),
            Bin::StrAdd => (false, vec![0x21]),
            Bin::StrEq => (false, vec![0x13]),
            Bin::StrNe => (false, vec![0x13, NOT]),
            Bin::StrLt => (false, vec![0x29]),
            Bin::StrGe => (false, vec![0x29, NOT]),
            Bin::StrGt if v >= 6 => (false, vec![0x68]),
            Bin::StrGt => (true, vec![0x29]),
            Bin::StrLe if v >= 6 => (false, vec![0x68, NOT]),
            Bin::StrLe => (true, vec![0x29, NOT]),
        };
        let (a, b) = if swap { (r, l) } else { (l, r) };
        let mut ret = self.src(ld, a)?;
        ret.extend(self.src(ld, b)?);
        ret.extend(codes.into_iter().map(simple));
        Ok(ret)
    }

    fn boxed_of(it: &It) -> It {
        match it {
            It::Var(v) => v.borrow().boxed.clone().unwrap_or(It::Empty),
            other => other.clone(),
        }
    }

    /// `PreIncrement/PreDecrement.toSourceBoth`.
    fn pre(&mut self, ld: &mut Ld, v: &It, want: bool, code: u8) -> R<L> {
        let val = Self::boxed_of(v);
        let mut ret = Vec::new();
        match &val {
            It::GetVariable(_) => {
                ret.extend(self.src(ld, &val)?);
                ret.pop();
                ret.extend(self.src(ld, &val)?);
                ret.push(simple(code));
                let t = self.temp(ld);
                ret.push(store_reg(t));
                ret.push(simple(0x1d));
                ret.push(push1(Value::Register(t as u8)));
                self.release(ld, t);
            }
            It::GetMember { obj, member, getter } => {
                if *getter {
                    let n = self.string_of(member).unwrap_or_default();
                    ret.extend(self.src(ld, &val)?);
                    ret.push(simple(code));
                    let k = self.konst(&format!("__set__{n}"));
                    ret.extend(self.merge(ld, vec![Part::A(push1(Value::Int(1))), Part::I(obj), Part::A(k), Part::A(simple(0x52))])?);
                } else {
                    ret.extend(self.src(ld, &val)?);
                    ret.pop();
                    ret.extend(self.src(ld, &val)?);
                    ret.push(simple(code));
                    let mut t = 0;
                    if want {
                        t = self.temp(ld);
                        ret.push(store_reg(t));
                    }
                    ret.push(simple(0x4f));
                    if want {
                        ret.push(push1(Value::Register(t as u8)));
                        self.release(ld, t);
                    }
                    return Ok(ret);
                }
            }
            It::Direct(Val::Reg(r)) => {
                ret.push(push1(Value::Register(*r)));
                ret.push(simple(code));
                ret.push(store_reg(*r as usize));
            }
            It::GetProperty { .. } => {
                ret.extend(self.src(ld, &val)?);
                ret.extend(self.src(ld, &val)?);
                ret.pop();
                ret.extend(self.src(ld, &val)?);
                ret.push(simple(code));
                ret.push(simple(0x23));
            }
            _ => {}
        }
        if !want {
            ret.push(simple(POP));
        }
        Ok(ret)
    }

    /// `PostIncrement/PostDecrement.toSourceIgnoreReturnValue`.
    fn post_ignore(&mut self, ld: &mut Ld, v: &It, code: u8) -> R<L> {
        let val = Self::boxed_of(v);
        let mut ret = Vec::new();
        match &val {
            It::GetVariable(_) => {
                ret.extend(self.src(ld, &val)?);
                ret.pop();
                ret.extend(self.src(ld, &val)?);
                ret.push(simple(code));
                ret.push(simple(0x1d));
            }
            It::GetMember { obj, member, getter } => {
                if *getter {
                    let n = self.string_of(member).unwrap_or_default();
                    let k = self.konst(&format!("__set__{n}"));
                    ret.extend(self.merge(ld, vec![Part::I(&val), Part::A(simple(code)), Part::A(push1(Value::Int(1))), Part::I(obj), Part::A(k), Part::A(simple(0x52)), Part::A(simple(POP))])?);
                } else {
                    ret.extend(self.src(ld, &val)?);
                    ret.pop();
                    ret.extend(self.src(ld, &val)?);
                    ret.push(simple(code));
                    ret.push(simple(0x4f));
                }
            }
            It::Direct(Val::Reg(r)) => {
                ret.push(push1(Value::Register(*r)));
                ret.push(simple(code));
                ret.push(store_reg(*r as usize));
                ret.push(simple(POP));
            }
            It::GetProperty { .. } => {
                ret.extend(self.src(ld, &val)?);
                ret.pop();
                ret.extend(self.src(ld, &val)?);
                ret.push(simple(code));
                ret.push(simple(0x23));
            }
            _ => {}
        }
        Ok(ret)
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_if(&mut self, ld: &mut Ld, cond: &It, on_true: &[It], on_false: Option<&[It]>, ternar: bool, line: usize, title: &str) -> R<L> {
        let mut ret = match cond {
            It::Not(v) => self.src(ld, v)?,
            c => {
                let mut l = self.src(ld, c)?;
                l.push(simple(NOT));
                l
            }
        };
        let t = if ternar { self.src(ld, &on_true[0])? } else { self.generate(ld, on_true)? };
        let f = match on_false {
            Some(f) if !f.is_empty() => Some(if ternar { self.src(ld, &f[0])? } else { self.generate(ld, f)? }),
            _ => None,
        };
        let mut t_len = self.len(&t);
        let too_big = |n: usize| -> R<()> {
            if n > 32767 {
                return cerr(line, &format!("Generated offset for onTrue part of {title} is larger than maximum allowed for SI16."));
            }
            Ok(())
        };
        too_big(t_len)?;
        let if_at = ret.len();
        ret.push(iff(t_len as i32));
        ret.extend(t);
        if let Some(f) = f {
            let last_is_jump = on_true.last().is_some_and(|c| matches!(c, It::Continue | It::Break));
            let mut jmp_at = None;
            if on_true.is_empty() || !last_is_jump {
                let j = jump(0);
                t_len += self.alen(&j);
                jmp_at = Some(ret.len());
                ret.push(j);
            }
            too_big(t_len)?;
            ret[if_at].a.set_offset(t_len as i32);
            let f_len = self.len(&f);
            if let Some(j) = jmp_at {
                ret[j].a.set_offset(f_len as i32);
            }
            ret.extend(f);
        }
        Ok(ret)
    }

    fn fix_loop(&self, code: &mut [GA], brk: i64, cont: Option<i64>, line: usize) -> R<()> {
        let mut pos: i64 = 0;
        for x in code.iter_mut() {
            pos += x.a.bytes_len(self.cs) as i64;
            if matches!(x.a.op, Op::Jump { .. }) {
                if x.cont {
                    if let Some(c) = cont {
                        let n = -pos + c;
                        if !(-32768..=32767).contains(&n) {
                            return cerr(line, "Generated offset for Continue is outside bounds of SI16.");
                        }
                        x.a.set_offset(n as i32);
                        x.cont = false;
                    }
                }
                if x.brk {
                    let n = -pos + brk;
                    if !(-32768..=32767).contains(&n) {
                        return cerr(line, "Generated offset for Break is outside bounds of SI16.");
                    }
                    x.a.set_offset(n as i32);
                    x.brk = false;
                }
            }
        }
        Ok(())
    }

    /// Commands, then the last expression's value.
    fn cond_list(&mut self, ld: &mut Ld, ex: &[It]) -> R<L> {
        let Some((last, rest)) = ex.split_last() else { return Ok(Vec::new()) };
        let mut l = self.generate(ld, rest)?;
        l.extend(self.src(ld, last)?);
        Ok(l)
    }

    fn gen_while(&mut self, ld: &mut Ld, cond: &[It], body: &[It], line: usize) -> R<L> {
        let mut expr = self.cond_list(ld, cond)?;
        let mut b = self.generate(ld, body)?;
        expr.push(simple(NOT));
        expr.push(iff(0));
        b.push(jump(0));
        let e_len = self.len(&expr) as i64;
        let b_len = self.len(&b) as i64;
        if b_len > 32767 {
            return cerr(line, "Generated offset for While is larger than maximum allowed for SI16.");
        }
        let back = -(e_len + b_len);
        if back < -32768 {
            return cerr(line, "Generated offset for While is lower than minimum allowed for SI16.");
        }
        let bl = b.len();
        b[bl - 1].a.set_offset(back as i32);
        let el = expr.len();
        expr[el - 1].a.set_offset(b_len as i32);
        self.fix_loop(&mut b, b_len, Some(-e_len), line)?;
        expr.extend(b);
        Ok(expr)
    }

    fn gen_do_while(&mut self, ld: &mut Ld, body: &[It], cond: &[It], line: usize) -> R<L> {
        let expr = self.cond_list(ld, cond)?;
        let mut b = self.generate(ld, body)?;
        let b_len = self.len(&b) as i64;
        let e_len = self.len(&expr) as i64;
        let f = iff(0);
        let offset = b_len + e_len + self.alen(&f) as i64;
        if -offset < -32768 {
            return cerr(line, "Generated offset for DoWhile is lower than minimum allowed for SI16.");
        }
        self.fix_loop(&mut b, offset, Some(b_len), line)?;
        let mut ret = b;
        ret.extend(expr);
        let mut f = f;
        f.a.set_offset(-offset as i32);
        ret.push(f);
        Ok(ret)
    }

    fn gen_for(&mut self, ld: &mut Ld, first: &[It], cond: &It, last: &[It], body: &[It], line: usize) -> R<L> {
        let mut expr = self.src(ld, cond)?;
        let mut b = self.generate(ld, body)?;
        let fin = self.generate(ld, last)?;
        expr.push(simple(NOT));
        expr.push(iff(0));
        let j = jump(0);
        let j_len = self.alen(&j) as i64;
        let e_len = self.len(&expr) as i64;
        let b_len = self.len(&b) as i64;
        let f_len = self.len(&fin) as i64;
        let jmp = -(e_len + b_len + f_len + j_len);
        let if_off = b_len + f_len + j_len;
        if jmp < -32768 {
            return cerr(line, "Generated offset for For is lower than minimum allowed for SI16.");
        }
        if if_off > 32767 {
            return cerr(line, "Generated offset for For is larger than maximum allowed for SI16.");
        }
        let el = expr.len();
        expr[el - 1].a.set_offset(if_off as i32);
        self.fix_loop(&mut b, b_len + f_len + j_len, Some(b_len), line)?;
        let mut ret = self.generate(ld, first)?;
        ret.extend(expr);
        ret.extend(b);
        ret.extend(fin);
        let mut j = j;
        j.a.set_offset(jmp as i32);
        ret.push(j);
        Ok(ret)
    }

    fn with_value(boxed: &It, v: It) -> It {
        let v = Box::new(v);
        match boxed.clone() {
            It::SetVariable { name, force_set, .. } => It::SetVariable { name, value: v, force_set },
            It::DefineLocal { name, .. } => It::DefineLocal { name, value: Some(v) },
            It::StoreRegister { reg, .. } => It::StoreRegister { reg, value: v },
            It::SetMember { obj, member, setter, .. } => It::SetMember { obj, member, value: v, setter },
            It::SetProperty { target, index, .. } => It::SetProperty { target, index, value: v },
            other => other,
        }
    }

    fn gen_for_in(&mut self, ld: &mut Ld, var: &VarRef, collection: &It, body: &[It], line: usize) -> R<L> {
        let mut ret = self.src(ld, collection)?;
        ret.push(simple(0x55));
        let reg = self.temp(ld);
        let mut expr = vec![store_reg(reg), push1(Value::Null), simple(0x49), iff(0)];
        let boxed = var.borrow().boxed.clone().unwrap_or(It::Empty);
        let set = Self::with_value(&boxed, It::Direct(Val::Reg(reg as u8)));
        let mut b = self.ignore(ld, &set)?;
        let old = ld.forin;
        ld.forin = old + 1;
        b.extend(self.generate(ld, body)?);
        ld.forin = old;
        b.push(jump(0));
        let has_break = b.iter().any(|x| x.brk && matches!(x.a.op, Op::Jump { .. }));
        let mut pop = Vec::new();
        let mut pop_len = 0;
        if has_break {
            pop = vec![push1(Value::Null), simple(0x49), simple(NOT), iff(0)];
            pop_len = self.len(&pop) as i64;
            let pl = pop.len();
            pop[pl - 1].a.set_offset(-pop_len as i32);
        }
        let b_len = self.len(&b) as i64;
        let e_len = self.len(&expr) as i64;
        let bl = b.len();
        b[bl - 1].a.set_offset((-b_len - e_len) as i32);
        expr[3].a.set_offset((b_len + pop_len) as i32);
        self.fix_loop(&mut b, b_len, Some(-e_len), line)?;
        ret.extend(expr);
        ret.extend(b);
        ret.extend(pop);
        self.release(ld, reg);
        Ok(ret)
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_switch(&mut self, ld: &mut Ld, expr: &It, values: &[Option<It>], cmds: &[Vec<It>], mapping: &[usize], line: usize) -> R<L> {
        let mut reg = 0;
        for i in 0..256 {
            if !ld.regs.values().any(|&v| v == i) {
                self.uniq += 1;
                ld.regs.insert(format!("__switch{}", self.uniq), i);
                reg = i;
                break;
            }
        }
        let mut ret = self.src(ld, expr)?;
        let mut first = true;
        let mut case_ifs: Vec<Vec<usize>> = Vec::new();
        let mut case_cmds: Vec<L> = Vec::new();
        let mut exprs_all: Vec<Vec<usize>> = Vec::new();
        let mut default_pos: Option<usize> = None;
        let mut m = 0;
        while m < values.len() {
            let mut lens = Vec::new();
            let mut ifs = Vec::new();
            let map = mapping[m];
            while m < values.len() {
                if mapping[m] != map {
                    break;
                }
                match &values[m] {
                    None => default_pos = Some(case_ifs.len()),
                    Some(v) => {
                        let mut c = self.src(ld, v)?;
                        c.insert(0, if first { store_reg(reg) } else { push1(Value::Register(reg as u8)) });
                        c.push(simple(0x66));
                        c.push(iff(0));
                        lens.push(self.len(&c));
                        ifs.push(ret.len() + c.len() - 1);
                        ret.extend(c);
                    }
                }
                first = false;
                m += 1;
            }
            exprs_all.push(lens);
            case_ifs.push(ifs);
            case_cmds.push(self.generate(ld, &cmds[map])?);
        }
        let def_at = ret.len();
        let dj = jump(0);
        let dj_len = self.alen(&dj);
        ret.push(dj);
        let case_lens: Vec<usize> = case_cmds.iter().map(|c| self.len(c)).collect();
        for i in 0..case_ifs.len() {
            for c in 0..case_ifs[i].len() {
                let mut pos = 0;
                for j in c + 1..case_ifs[i].len() {
                    pos += exprs_all[i][j];
                }
                for k in i + 1..case_ifs.len() {
                    pos += exprs_all[k].iter().sum::<usize>();
                }
                pos += dj_len;
                pos += case_lens[..i].iter().sum::<usize>();
                if pos > 32767 {
                    return cerr(line, "Generated offset for Switch is larger than maximum allowed for SI16.");
                }
                ret[case_ifs[i][c]].a.set_offset(pos as i32);
            }
        }
        let mut def_pos = 0;
        for (i, l) in case_lens.iter().enumerate() {
            if default_pos.is_none_or(|d| i < d) {
                def_pos += l;
            }
        }
        if def_pos > 32767 {
            return cerr(line, "Generated offset for Switch is larger than maximum allowed for SI16.");
        }
        ret[def_at].a.set_offset(def_pos as i32);
        let mut all: L = case_cmds.into_iter().flatten().collect();
        let brk: usize = case_lens.iter().sum();
        self.fix_loop(&mut all, brk as i64, None, line)?;
        ret.extend(all);
        Ok(ret)
    }

    fn gen_try(&mut self, ld: &mut Ld, body: &[It], names: &[It], types: &[Option<It>], catches: &[Vec<It>], finally: Option<&[It]>) -> R<L> {
        let mut try_a = self.generate(ld, body)?;
        let fin_a = match finally {
            Some(f) => Some(self.generate(ld, f)?),
            None => None,
        };
        let mut catch_a: Option<L> = None;
        let mut catch_name = None;
        let mut catch_reg = 0;
        let mut in_reg = false;
        let mut catch_size = 0;
        if !catches.is_empty() {
            let mut full: L = Vec::new();
            if names.len() == 1 && types[0].is_none() {
                catch_name = self.string_of(&names[0]);
                full = self.generate_src_merge(ld, &catches[0])?;
            } else {
                in_reg = true;
                catch_reg = self.temp(ld);
                let all = types.iter().any(Option::is_none);
                if !all {
                    full.splice(0..0, vec![push1(Value::Register(catch_reg as u8)), simple(0x2a)]);
                }
                for i in (0..names.len()).rev() {
                    let body = self.generate_src_merge(ld, &catches[i])?;
                    match &types[i] {
                        None => {
                            let mut part = vec![push1(Value::Register(catch_reg as u8))];
                            part.extend(self.src(ld, &names[i])?);
                            part.push(simple(0x4d));
                            part.push(simple(0x3c));
                            part.extend(body);
                            full.splice(0..0, part);
                        }
                        Some(t) => {
                            let mut if_body = self.src(ld, &names[i])?;
                            if_body.push(simple(0x4d));
                            if_body.push(simple(0x3c));
                            if_body.extend(body);
                            full.insert(0, simple(POP));
                            let to_finish = self.len(&full);
                            if_body.push(jump(to_finish as i32));
                            let if_size = self.len(&if_body);
                            full.splice(0..0, if_body);
                            let mut head = self.src(ld, t)?;
                            head.extend([push1(Value::Register(catch_reg as u8)), simple(0x2b), simple(0x4c), push1(Value::Null), simple(0x49), iff(if_size as i32)]);
                            full.splice(0..0, head);
                        }
                    }
                }
                self.release(ld, catch_reg);
            }
            catch_size = self.len(&full);
            try_a.push(jump(catch_size as i32));
            catch_a = Some(full);
        }
        let fin_size = fin_a.as_ref().map_or(0, |f| self.len(f));
        let try_size = self.len(&try_a);
        let t = TryOp {
            catch_in_register: in_reg,
            finally_flag: fin_a.is_some(),
            catch_flag: !catches.is_empty(),
            catch_name,
            catch_register: catch_reg as u8,
            try_size: try_size as u32,
            catch_size: catch_size as u32,
            finally_size: fin_size as u32,
        };
        let mut ret = vec![ga(0x8f, Op::Try(Box::new(t)))];
        ret.extend(try_a);
        if let Some(c) = catch_a {
            ret.extend(c);
        }
        if let Some(f) = fin_a {
            ret.extend(f);
        }
        Ok(ret)
    }

    /// `toSourceMerge` over a command list: values are kept, as in catch bodies.
    fn generate_src_merge(&mut self, ld: &mut Ld, cmds: &[It]) -> R<L> {
        let mut out = Vec::new();
        for c in cmds {
            out.extend(self.src(ld, c)?);
        }
        Ok(out)
    }

    /// `getVarParts`: a dotted name as its parts.
    fn var_parts(&self, it: Option<&It>) -> Vec<String> {
        let mut ret = Vec::new();
        let Some(mut cur) = it else { return ret };
        while let It::GetMember { obj, member, .. } = cur {
            ret.insert(0, self.name_of(member).unwrap_or_default());
            cur = obj;
        }
        if let Some(f) = self.name_of(cur) {
            ret.insert(0, f);
        }
        ret
    }

    fn name_of(&self, it: &It) -> Option<String> {
        match it {
            It::Var(v) => Some(v.borrow().name.clone()),
            It::Direct(v) => self.pool.borrow().string(v),
            It::GetVariable(n) => self.name_of(n),
            It::GetMember { member, .. } => self.name_of(member),
            _ => None,
        }
    }

    /// `typeToActions`.
    fn type_actions(&self, ty: &[String], value: Option<L>) -> L {
        let mut ret = Vec::new();
        if ty.is_empty() {
            return ret;
        }
        ret.push(self.konst(&ty[0]));
        let mut value = value;
        if ty.len() == 1 && value.is_some() {
            ret.extend(value.take().unwrap());
            ret.push(simple(0x1d));
        } else {
            ret.push(simple(0x1c));
        }
        for i in 1..ty.len() {
            ret.push(self.konst(&ty[i]));
            if i == ty.len() - 1 && value.is_some() {
                ret.extend(value.take().unwrap());
                ret.push(simple(0x4f));
            } else {
                ret.push(simple(0x4e));
            }
        }
        ret
    }

    fn empty_fn() -> GA {
        ga(0x9b, Op::DefineFunction { name: String::new(), params: Vec::new(), code_size: 0 })
    }

    /// `generateTraits`.
    #[allow(clippy::too_many_arguments)]
    fn traits(&mut self, ld: &mut Ld, is_interface: bool, name: &It, extends: Option<&It>, implements: &[It], traits: &[(It, It)], statics: &[bool]) -> R<L> {
        let ext = self.var_parts(extends);
        let mut ret = Vec::new();
        let names = self.var_parts(Some(name));
        for i in 0..names.len().saturating_sub(1) {
            let mut glob = vec!["_global".to_string()];
            glob.extend(names[..=i].iter().cloned());
            let val = vec![push1(Value::Double(0.0)), self.konst("Object"), simple(0x40)];
            let not_body = self.type_actions(&glob, Some(val));
            ret.extend(self.type_actions(&glob, None));
            ret.push(simple(NOT));
            ret.push(simple(NOT));
            ret.push(iff(self.len(&not_body) as i32));
            ret.extend(not_body);
            ret.push(simple(POP));
        }
        let mut if_body: L = Vec::new();
        let mut glob = vec!["_global".to_string()];
        glob.extend(names.iter().cloned());
        let ctor_name = names.last().cloned().unwrap_or_default();
        let mut ctor_index = None;
        for (t, (k, v)) in traits.iter().enumerate() {
            if matches!(v, It::Function(_)) && self.name_of(k).as_deref() == Some(ctor_name.as_str()) {
                ctor_index = Some(t);
                break;
            }
        }
        let mut constr: L = match ctor_index {
            None => {
                let mut val = vec![Self::empty_fn()];
                if !is_interface {
                    val.push(store_reg(1));
                }
                self.type_actions(&glob, Some(val))
            }
            Some(t) => {
                let mut c = self.src(ld, &traits[t].1)?;
                c.push(store_reg(1));
                self.type_actions(&glob, Some(c))
            }
        };
        let mut properties: Vec<String> = Vec::new();
        let mut setters = std::collections::HashSet::new();
        let mut getters = std::collections::HashSet::new();
        let mut static_props: Vec<String> = Vec::new();
        let mut static_setters = std::collections::HashSet::new();
        let mut static_getters = std::collections::HashSet::new();
        let add = |v: &mut Vec<String>, s: String| {
            if !v.contains(&s) {
                v.push(s);
            }
        };
        if !is_interface {
            for pass in 1..=2 {
                for (t, (k, v)) in traits.iter().enumerate() {
                    if ctor_index == Some(t) {
                        continue;
                    }
                    let st = statics[t];
                    match (pass, v) {
                        (1, It::Function(f)) => {
                            let (is_get, is_set, calc) = {
                                let fb = f.borrow();
                                (fb.is_getter, fb.is_setter, fb.calc_name.as_ref().and_then(|c| self.name_of(c)).unwrap_or_default())
                            };
                            if is_get || is_set {
                                add(if st { &mut static_props } else { &mut properties }, calc.clone());
                            }
                            if calc.starts_with("__get__") || calc.starts_with("__set__") {
                                add(if st { &mut static_props } else { &mut properties }, calc[7..].to_string());
                            }
                            if let Some(p) = calc.strip_prefix("__get__") {
                                (if st { &mut static_getters } else { &mut getters }).insert(p.to_string());
                            }
                            if let Some(p) = calc.strip_prefix("__set__") {
                                (if st { &mut static_setters } else { &mut setters }).insert(p.to_string());
                            }
                            let mut prefix = "";
                            if is_get {
                                (if st { &mut static_getters } else { &mut getters }).insert(calc.clone());
                                prefix = "__get__";
                            }
                            if is_set {
                                (if st { &mut static_setters } else { &mut setters }).insert(calc.clone());
                                prefix = "__set__";
                            }
                            if_body.push(push1(Value::Register(if st { 1 } else { 2 })));
                            if_body.push(self.konst(&format!("{prefix}{}", self.name_of(k).unwrap_or_default())));
                            if_body.extend(self.src(ld, v)?);
                            if_body.push(simple(0x4f));
                        }
                        (2, v) if !matches!(v, It::Function(_)) => {
                            if_body.push(push1(Value::Register(if st { 1 } else { 2 })));
                            if_body.push(self.konst(&self.name_of(k).unwrap_or_default()));
                            if_body.extend(self.src(ld, v)?);
                            if_body.push(simple(0x4f));
                        }
                        _ => {}
                    }
                }
            }
        }
        for (props, sets, gets, reg) in [(&static_props, &static_setters, &static_getters, 1u8), (&properties, &setters, &getters, 2u8)] {
            for prop in props {
                if sets.contains(prop) {
                    if_body.push(push(vec![Value::Register(reg), Value::Str(format!("__set__{prop}"))]));
                    if_body.push(simple(0x4e));
                } else {
                    if_body.push(Self::empty_fn());
                }
                if gets.contains(prop) {
                    if_body.push(push(vec![Value::Register(reg), Value::Str(format!("__get__{prop}"))]));
                    if_body.push(simple(0x4e));
                } else {
                    if_body.push(Self::empty_fn());
                }
                if_body.push(push(vec![Value::Str(prop.clone()), Value::Int(3), Value::Register(reg), Value::Str("addProperty".into())]));
                if_body.push(simple(0x52));
            }
        }
        if !is_interface {
            if_body.push(push1(Value::Int(1)));
            if_body.push(push1(Value::Null));
            if_body.extend(self.type_actions(&glob, None));
            if_body.push(self.konst("prototype"));
            if_body.push(simple(0x4e));
            if_body.push(push1(Value::Int(3)));
            if_body.push(self.konst("ASSetPropFlags"));
            if_body.push(simple(0x3d));
        }
        if constr.is_empty() {
            let mut val = vec![Self::empty_fn()];
            if !is_interface {
                val.push(store_reg(1));
            }
            constr = self.type_actions(&glob, Some(val));
        }
        if !ext.is_empty() {
            constr.extend(self.type_actions(&glob, None));
            constr.extend(self.type_actions(&ext, None));
            constr.push(simple(0x69));
        }
        if !is_interface {
            constr.push(push1(Value::Register(1)));
            constr.push(self.konst("prototype"));
            constr.push(simple(0x4e));
            constr.push(store_reg(2));
            constr.push(simple(POP));
        }
        if !implements.is_empty() {
            for imp in implements {
                let mut g = vec!["_global".to_string()];
                g.extend(self.var_parts(Some(imp)));
                constr.extend(self.type_actions(&g, None));
            }
            constr.push(push1(Value::Int(implements.len() as i64)));
            constr.extend(self.type_actions(&glob, None));
            constr.push(simple(0x2c));
        }
        constr.extend(if_body);
        let if_body = constr;
        ret.extend(self.type_actions(&glob, None));
        ret.push(simple(NOT));
        ret.push(simple(NOT));
        let mut off = self.len(&if_body);
        if off > 0x7fff {
            off = 0;
        }
        ret.push(iff(off as i32));
        ret.extend(if_body);
        ret.push(simple(POP));
        Ok(ret)
    }

    /// `FunctionActionItem.toSource`.
    fn function(&mut self, ld: &mut Ld, f: &FuncRef) -> R<L> {
        let (name, mut params, vars, inner, has_eval, body, line) = {
            let fb = f.borrow();
            (fb.name.clone(), fb.params.clone(), fb.vars.clone(), fb.inner.clone(), fb.has_eval, fb.body.clone(), fb.line)
        };
        let used: std::collections::HashSet<String> = vars.iter().map(|v| v.borrow().name.clone()).collect();
        let mut copy = ld.clone();
        copy.in_function += 1;
        let (mut p_parent, mut p_root, mut p_super, mut p_args, mut p_this, mut p_global) = (false, false, false, false, false, false);
        let (mut s_super, mut s_args, mut s_this) = (false, false, false);
        let mut needs2 = false;
        let can2 = self.version >= 7;
        let mut regs: Vec<String> = Vec::new();
        if can2 {
            regs.push("***** ZERO *****".into());
            if used.contains("this") {
                needs2 = true;
                p_this = true;
                regs.push("this".into());
            } else {
                s_this = true;
            }
            if used.contains("arguments") {
                p_args = true;
                needs2 = true;
                regs.push("arguments".into());
            } else {
                s_args = true;
            }
            if used.contains("super") {
                p_super = true;
                needs2 = true;
                regs.push("super".into());
            } else {
                s_super = true;
            }
            if used.contains("_root") {
                p_root = true;
                needs2 = true;
                regs.push("_root".into());
            }
            if used.contains("_parent") {
                p_parent = true;
                needs2 = true;
                regs.push("_parent".into());
            }
            if used.contains("_global") {
                needs2 = true;
                p_global = true;
                regs.push("_global".into());
            }
            if !params.is_empty() {
                needs2 = true;
            }
            if ld.in_function > 1 {
                needs2 = true;
            }
        }
        let mut top: std::collections::HashSet<String> = vars.iter().filter(|v| v.borrow().definition).map(|v| v.borrow().name.clone()).collect();
        top.extend(params.iter().cloned());
        let mut deep = std::collections::HashSet::new();
        for i in &inner {
            deeply_used(&top, i, &mut deep);
        }
        let mut param_regs = vec![0usize; params.len()];
        if needs2 && !has_eval {
            let reg_num = |s: &str| -> Option<usize> { s.strip_prefix("_loc")?.strip_suffix('_')?.parse().ok().filter(|_| s[4..s.len() - 1].bytes().all(|b| b.is_ascii_digit())) };
            let mut new_regs: Vec<String> = Vec::new();
            for v in &vars {
                let v = v.borrow();
                if v.definition && !regs.contains(&v.name) && !params.contains(&v.name) && !new_regs.contains(&v.name) && !deep.contains(&v.name) && reg_num(&v.name).is_some() {
                    new_regs.push(v.name.clone());
                }
            }
            for i in 0..params.len() {
                let n = params[i].clone();
                if new_regs.contains(&n) {
                    continue;
                }
                if let Some(r) = reg_num(&n) {
                    while regs.len() <= r {
                        regs.push("**EMPTY**".into());
                    }
                    regs[r] = n;
                    params[i] = String::new();
                    param_regs[i] = r;
                }
            }
            new_regs.sort_by(|a, b| crate::action::decomp::uninit::natural_cmp(a, b));
            let before = regs.len();
            for n in &new_regs {
                let r = reg_num(n).unwrap();
                while regs.len() <= r {
                    regs.push("**EMPTY**".into());
                }
                regs[r] = n.clone();
            }
            let mut pos = before;
            for i in 0..params.len() {
                if param_regs[i] != 0 || deep.contains(&params[i]) {
                    continue;
                }
                while pos < regs.len() && regs[pos] != "**EMPTY**" {
                    pos += 1;
                }
                while regs.len() <= pos {
                    regs.push("**EMPTY**".into());
                }
                param_regs[i] = pos;
                regs[pos] = params[i].clone();
            }
        }
        copy.in_function += 1;
        for v in &vars {
            if box_resolved(v) {
                continue;
            }
            let (vname, stored, def) = {
                let b = v.borrow();
                (b.name.clone(), b.store.clone(), b.definition)
            };
            let boxed = if let Some(i) = regs.iter().position(|r| *r == vname) {
                match stored {
                    Some(s) => It::StoreRegister { reg: i as u8, value: Box::new(s) },
                    None => It::Direct(Val::Reg(i as u8)),
                }
            } else if def {
                It::DefineLocal { name: Box::new(self.konst_item(&vname)), value: stored.map(Box::new) }
            } else if let Some(s) = stored {
                It::SetVariable { name: Box::new(self.konst_item(&vname)), value: Box::new(s), force_set: false }
            } else {
                It::GetVariable(Box::new(self.konst_item(&vname)))
            };
            v.borrow_mut().boxed = Some(boxed);
        }
        for (i, r) in regs.iter().enumerate().skip(1) {
            copy.regs.insert(r.clone(), i);
        }
        let mut ret: L = match &body {
            Some(b) if !b.is_empty() => self.generate(&mut copy, b)?,
            _ => Vec::new(),
        };
        let mut reg_count = regs.len();
        for x in &ret {
            if let Op::Push { values, .. } = &x.a.op {
                for v in values {
                    if let Value::Register(r) = v {
                        if *r as usize >= reg_count {
                            reg_count += 1;
                        }
                    }
                }
            }
        }
        if !needs2 {
            ret = self.preserve_registers(ret, line)?;
        }
        let len = self.len(&ret);
        if len > 0xffff {
            return cerr(line, "Function body is too large to fit into UI16.");
        }
        let head = if !needs2 {
            ga(0x9b, Op::DefineFunction { name, params, code_size: len as u32 })
        } else {
            ga(
                0x8e,
                Op::DefineFunction2(Box::new(Df2 {
                    name,
                    params: params.into_iter().zip(param_regs).map(|(n, r)| (r as u8, n)).collect(),
                    register_count: reg_count as u8,
                    preload_parent: p_parent,
                    preload_root: p_root,
                    suppress_super: s_super,
                    preload_super: p_super,
                    suppress_arguments: s_args,
                    preload_arguments: p_args,
                    suppress_this: s_this,
                    preload_this: p_this,
                    preload_global: p_global,
                    code_size: len as u32,
                })),
            )
        };
        ret.insert(0, head);
        Ok(ret)
    }

    fn restore(&self, out: &mut L, regs: &[usize], returning: bool) -> usize {
        let mut len = 0;
        for &r in regs.iter().rev() {
            if returning {
                out.push(simple(0x4d));
                len += 1;
            }
            let s = store_reg(r);
            len += self.alen(&s) + 1;
            out.push(s);
            out.push(simple(POP));
        }
        len
    }

    /// `preserveRegisters`: a DefineFunction saves and restores the registers it stores.
    fn preserve_registers(&self, body: L, line: usize) -> R<L> {
        let mut registers = BTreeSet::new();
        let mut returns = Vec::new();
        let mut has_try = false;
        let mut offset = 0usize;
        let mut nested_end: i64 = -1;
        for (i, x) in body.iter().enumerate() {
            if offset as i64 >= nested_end {
                match &x.a.op {
                    Op::DefineFunction { .. } | Op::DefineFunction2(_) => nested_end = (offset + self.alen(x) + x.a.container_sizes().unwrap()[0] as usize) as i64,
                    Op::StoreRegister(r) => {
                        registers.insert(*r as usize);
                    }
                    Op::Try(t) => {
                        has_try = true;
                        if t.catch_in_register {
                            registers.insert(t.catch_register as usize);
                        }
                    }
                    Op::Simple if x.a.code == 0x3e => returns.push(i),
                    _ => {}
                }
            }
            offset += self.alen(x);
        }
        if registers.is_empty() {
            return Ok(body);
        }
        let saved: Vec<usize> = registers.into_iter().collect();
        let save = push(saved.iter().map(|&r| Value::Register(r as u8)).collect());
        let save_len = self.alen(&save);
        let mut result = vec![save];
        if has_try {
            if offset > 0xffff {
                return cerr(line, "Try body is too large to fit into UI16.");
            }
            let t = TryOp { catch_in_register: false, finally_flag: true, catch_flag: false, catch_name: None, catch_register: 0, try_size: offset as u32, catch_size: 0, finally_size: (saved.len() * 5) as u32 };
            result.push(ga(0x8f, Op::Try(Box::new(t))));
            result.extend(body);
            self.restore(&mut result, &saved, false);
            return Ok(result);
        }
        let mut relocated: HashMap<usize, usize> = HashMap::new();
        let mut indexes = Vec::new();
        offset = 0;
        let mut new_offset = save_len;
        for (i, x) in body.iter().enumerate() {
            relocated.insert(offset, new_offset);
            indexes.push(result.len());
            if returns.contains(&i) {
                new_offset += self.restore(&mut result, &saved, true);
            }
            result.push(x.clone());
            offset += self.alen(x);
            new_offset += self.alen(x);
        }
        relocated.insert(offset, new_offset);
        indexes.push(result.len());
        self.restore(&mut result, &saved, false);
        offset = 0;
        for (index, x) in body.iter().enumerate() {
            let length = self.alen(x);
            let at = indexes[index];
            if let Some(n) = x.a.store_size() {
                let start = at + 1;
                let end = indexes.get(index + 1 + n).copied().unwrap_or(result.len());
                if end - start > 0xff {
                    return cerr(line, "Frame-loaded skip count is too large after preserving function registers.");
                }
                result[at].a.set_store_size(end - start);
            }
            if let Op::Jump { offset: o, .. } | Op::If { offset: o } = x.a.op {
                let target = (offset as i64 + length as i64 + o as i64) as usize;
                let rel = relocated.get(&target).copied().unwrap_or(new_offset) as i64 - relocated[&offset] as i64 - length as i64;
                if !(i16::MIN as i64..=i16::MAX as i64).contains(&rel) {
                    return cerr(line, "Jump offset is too large after preserving function registers.");
                }
                result[at].a.set_offset(rel as i32);
            }
            if let Some(sizes) = x.a.container_sizes() {
                let mut start = offset + length;
                for (i, s) in sizes.iter().enumerate() {
                    let end = start + *s as usize;
                    let size = relocated.get(&end).copied().unwrap_or(new_offset) - relocated.get(&start).copied().unwrap_or(new_offset);
                    if size > 0xffff {
                        return cerr(line, "Container body is too large after preserving function registers.");
                    }
                    result[at].a.set_container_size(i, size as u64);
                    start = end;
                }
            }
            offset += length;
        }
        Ok(result)
    }
}

fn deeply_used(top: &std::collections::HashSet<String>, f: &FuncRef, out: &mut std::collections::HashSet<String>) {
    let fb = f.borrow();
    let defined: std::collections::HashSet<String> = fb.vars.iter().filter(|v| v.borrow().definition).map(|v| v.borrow().name.clone()).collect();
    for v in &fb.vars {
        let v = v.borrow();
        if !v.definition && !defined.contains(&v.name) && top.contains(&v.name) {
            out.insert(v.name.clone());
        }
    }
    for i in &fb.inner {
        deeply_used(top, i, out);
    }
}

fn to_number(v: &Val) -> f64 {
    match v {
        Val::Int(i) => *i as f64,
        Val::Double(d) => *d,
        Val::Float(f) => *f as f64,
        Val::Bool(b) => *b as i32 as f64,
        Val::Str(s) => s.trim().parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

enum Part<'x> {
    I(&'x It),
    A(GA),
    L(L),
}
