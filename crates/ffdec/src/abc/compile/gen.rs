//! FFDec's `AVM2SourceGenerator`: the item tree to AVM2 instructions.

use std::collections::HashMap;
use std::rc::Rc;

use super::ast::*;
use super::index::{pg_abc, AbcId, Deleted, Index};
use super::ins::{self, code_len, ins, mark, op, pseudo, Code, Pseudo, I};
use super::pool;
use super::{CompileError, R};
use crate::abc::decomp::index::TySpec;
use crate::abc::decomp::names::Dotted;
use crate::abc::opcodes::Dialect;
use crate::abc::{ns_kind, Abc, Multiname};

/// The compilation state: the ABC being compiled into and the index of everything else.
pub struct Cx {
    pub abc: Abc,
    pub others: Vec<Abc>,
    pub index: Index,
    pub deleted: Deleted,
    /// Identity of each class slot, so deletions survive classes being inserted and moved.
    pub class_ids: Vec<usize>,
    pub next_class_id: usize,
    pub document_class: Option<String>,
    /// Bodies generated so far, as instructions until `finalize`.
    pub pending: HashMap<usize, (Code, Vec<Exc>)>,
    pub class_pos: usize,
    /// Body index before `pack` to index after it.
    pub body_map: Vec<Option<usize>>,
    fin_id: i64,
}

/// An `ABCException` the generator fills in.
#[derive(Debug, Clone, Default)]
pub struct Exc {
    pub start: usize,
    pub end: usize,
    pub target: usize,
    pub name: usize,
    pub ty: usize,
}

/// `SourceGeneratorLocalData`.
#[derive(Clone)]
pub struct Ld {
    pub register_vars: Vec<(String, i64)>,
    pub exceptions: Vec<Exc>,
    pub finally_catches: Vec<i64>,
    pub finally_opened_loops: Vec<Vec<i64>>,
    pub finally_counter: HashMap<i64, i64>,
    pub finally_register: i64,
    pub current_class: Option<String>,
    pub activation_reg: i64,
    pub call_stack: CallStack,
    pub pkg: Option<Dotted>,
    pub scope_stack: Nodes,
    pub current_script: Option<usize>,
    pub script_index: Option<usize>,
    pub sub_method: bool,
    pub private_ns: usize,
    pub protected_ns: usize,
    pub static_protected_ns: usize,
    pub internal_ns: usize,
    pub is_static: bool,
    pub opened_loops: Vec<i64>,
    pub catches_opened_loops: Vec<Vec<i64>>,
    pub catches_temp_regs: Vec<i64>,
    pub document_class: Option<String>,
    pub number_context: Option<i64>,
    pub imported: Imports,
    pub opened: NsList,
    pub return_type: Option<Node>,
    /// The statements still to come in each enclosing block, innermost last.
    pub following: Vec<Nodes>,
    /// Counter and collection registers the next for-in takes over from declarations.
    pub for_in_regs: Option<(i64, i64)>,
}

impl Default for Ld {
    fn default() -> Ld {
        Ld {
            register_vars: Vec::new(),
            exceptions: Vec::new(),
            finally_catches: Vec::new(),
            finally_opened_loops: Vec::new(),
            finally_counter: HashMap::new(),
            finally_register: -1,
            current_class: None,
            activation_reg: 0,
            call_stack: empty_stack(),
            pkg: Some(Dotted::default()),
            scope_stack: Vec::new(),
            current_script: None,
            script_index: None,
            sub_method: false,
            private_ns: 0,
            protected_ns: 0,
            static_protected_ns: 0,
            internal_ns: 0,
            is_static: false,
            opened_loops: Vec::new(),
            catches_opened_loops: Vec::new(),
            catches_temp_regs: Vec::new(),
            document_class: None,
            number_context: None,
            imported: Rc::new(std::cell::RefCell::new(Vec::new())),
            opened: ns_list(Vec::new()),
            return_type: None,
            following: Vec::new(),
            for_in_regs: None,
        }
    }
}

impl Ld {
    /// `getFullClass`.
    pub fn full_class(&self) -> Option<String> {
        match &self.pkg {
            None => self.current_class.clone(),
            Some(p) => Some(match &self.current_class {
                Some(c) => p.add_with_suffix(c).raw(),
                None => p.raw(),
            }),
        }
    }

    pub fn has_reg(&self, name: &str) -> bool {
        self.register_vars.iter().any(|(k, _)| k == name)
    }

    pub fn reg_used(&self, i: i64) -> bool {
        self.register_vars.iter().any(|(_, v)| *v == i)
    }
}

pub fn err<X>(line: usize, m: impl Into<String>) -> R<X> {
    Err(CompileError { line, message: m.into() })
}

pub fn set_loc(r: i64) -> Option<I> {
    match r {
        -1 => None,
        0..=3 => Some(ins(op::SET_LOCAL0 + r as u8, &[])),
        _ => Some(ins(op::SET_LOCAL, &[r])),
    }
}

pub fn get_loc(r: i64) -> Option<I> {
    match r {
        -1 => None,
        0..=3 => Some(ins(op::GET_LOCAL0 + r as u8, &[])),
        _ => Some(ins(op::GET_LOCAL, &[r])),
    }
}

fn ins_local(i: &ins::Ins, set: bool) -> Option<i64> {
    let (base, long) = if set { (op::SET_LOCAL0, op::SET_LOCAL) } else { (op::GET_LOCAL0, op::GET_LOCAL) };
    if i.pseudo != Pseudo::None {
        return None;
    }
    match i.code {
        c if (base..base + 4).contains(&c) => Some((c - base) as i64),
        c if c == long => i.ops.first().copied(),
        _ => None,
    }
}

/// Whether `prev` (one statement) ends storing a local that `next` starts by reading, with no
/// jump in `prev` landing after the store.
fn reuses_stored_local(prev: &[I], next: &[I]) -> bool {
    let (Some(last), Some(first)) = (prev.last(), next.first()) else { return false };
    let Some(r) = ins_local(&last.borrow(), true) else { return false };
    if ins_local(&first.borrow(), false) != Some(r) {
        return false;
    }
    // mxmlc reads the local back when the next statement converts it or copies it to another.
    if next.get(1).is_some_and(|i| {
        let i = i.borrow();
        matches!(i.code, 0x70 | 0x73..=0x77 | 0x80..=0x85 | 0x88 | 0x89) || ins_local(&i, true).is_some()
    }) {
        return false;
    }
    let end = code_len(prev);
    let mut addr = 0;
    for i in prev {
        let i = i.borrow();
        let len = i.len() as i64;
        if i.pseudo != Pseudo::None && !i.is_mark() {
            return false;
        }
        if (0x0c..=0x1a).contains(&i.code) && i.pseudo == Pseudo::None && addr + len + i.ops[0] >= end {
            return false;
        }
        if i.code == 0x1b {
            return false;
        }
        addr += len;
    }
    true
}

/// Builds instruction lists the way `toSourceMerge` concatenates its arguments.
pub struct M(pub Code);

impl M {
    pub fn new() -> M {
        M(Vec::new())
    }
    pub fn i(&mut self, i: I) -> &mut Self {
        self.0.push(i);
        self
    }
    pub fn op(&mut self, code: u8, ops: &[i64]) -> &mut Self {
        self.0.push(ins(code, ops));
        self
    }
    pub fn opt(&mut self, i: Option<I>) -> &mut Self {
        if let Some(i) = i {
            self.0.push(i);
        }
        self
    }
    pub fn c(&mut self, c: Code) -> &mut Self {
        self.0.extend(c);
        self
    }
}

impl Default for M {
    fn default() -> Self {
        M::new()
    }
}

pub fn spec_node(t: &TySpec) -> Node {
    match t {
        TySpec::Unbounded => unbounded(),
        TySpec::Null => node(Item::Null),
        TySpec::Type(f, ns) => node(Item::Type { full: f.clone(), ns: ns.clone() }),
        TySpec::Apply(o, ps) => node(Item::ApplyType { obj: spec_node(o), params: ps.iter().map(spec_node).collect() }),
    }
}

pub fn node_spec(n: &Node) -> Option<TySpec> {
    match &*n.borrow() {
        Item::Unbounded => Some(TySpec::Unbounded),
        Item::Null => Some(TySpec::Null),
        Item::Type { full, ns } => Some(TySpec::Type(full.clone(), ns.clone())),
        Item::ApplyType { obj, params } => {
            let o = node_spec(obj)?;
            let ps: Option<Vec<TySpec>> = params.iter().map(node_spec).collect();
            Some(TySpec::Apply(Box::new(o), ps?))
        }
        _ => None,
    }
}

fn is_ty(n: &Node, s: &str) -> bool {
    matches!(&*n.borrow(), Item::Type { full, ns: None } if full.raw() == s)
}

pub fn string_ty() -> Node {
    ty_parts(&["String"])
}

impl Cx {
    pub fn new(abc: Abc, others: Vec<Abc>) -> Cx {
        let n = abc.instances.len();
        Cx { abc, others, index: Index::new(), deleted: Deleted::default(), class_ids: (0..n).collect(), next_class_id: n, document_class: None, pending: HashMap::new(), class_pos: 0, body_map: Vec::new(), fin_id: 1 }
    }

    pub fn abc_of(&self, id: AbcId) -> &Abc {
        match id {
            AbcId::Sel => &self.abc,
            AbcId::Swf(i) => &self.others[i],
            AbcId::Pg(i) => pg_abc(i),
        }
    }

    pub fn dialect(&self) -> Dialect {
        Dialect::of(self.abc.major, self.abc.minor)
    }

    pub fn fin_id(&mut self) -> i64 {
        let r = self.fin_id;
        self.fin_id += 1;
        r
    }

    pub fn str(&mut self, s: &str) -> usize {
        pool::str_id(&mut self.abc, s)
    }

    /// `NamespaceItem.forceResolve`.
    pub fn force_resolve(&mut self, ns: &Ns) {
        let (kind, name) = {
            let n = ns.borrow();
            (n.kind_u8(), n.raw_name())
        };
        let i = pool::namespace_id(&mut self.abc, kind, &name, 0, true);
        ns.borrow_mut().index = i;
    }

    pub fn ns_name_kind(&self, i: usize) -> (Dotted, u8) {
        let n = self.abc.constants.namespaces.get(i).copied().unwrap_or(crate::abc::Namespace { kind: 0, name: 0 });
        (super::index::raw_names(&self.abc).ns_name(i), n.kind)
    }

    /// `NamespaceItem.getCpoolIndex`.
    pub fn ns_index(&mut self, ns: &Ns) -> R<usize> {
        let (idx, kind, name, line) = {
            let n = ns.borrow();
            (n.index, n.kind, n.raw_name(), n.line)
        };
        if idx > -1 {
            return Ok(idx as usize);
        }
        if kind == ns_kind::NAMESPACE as i32 || kind == KIND_NAMESPACE_CUSTOM {
            return err(line, format!("Namespace \"{name}\" unresolved"));
        }
        let i = pool::namespace_id(&mut self.abc, kind as u8, &name, 0, true);
        ns.borrow_mut().index = i;
        Ok(i as usize)
    }

    pub fn ns_set_of(&mut self, list: &NsList) -> R<usize> {
        let items: Vec<Ns> = list.borrow().clone();
        let mut set = Vec::new();
        for n in &items {
            set.push(self.ns_index(n)?);
        }
        Ok(pool::ns_set_id(&mut self.abc, &set, true) as usize)
    }

    pub fn multiname_l(&mut self, attr: bool, list: &NsList) -> R<usize> {
        let set = self.ns_set_of(list)?;
        Ok(pool::mn(&mut self.abc, Multiname::MultinameL { attr, ns_set: set }))
    }

    /// `traitName`.
    pub fn trait_name(&mut self, ns: usize, var: &str) -> usize {
        let n = self.str(var);
        pool::mn(&mut self.abc, Multiname::QName { attr: false, ns, name: n })
    }

    pub fn namespace(&mut self, kind: u8, name: &str) -> usize {
        pool::ns(&mut self.abc, kind, name)
    }

    pub fn get_free_register(&mut self, ld: &mut Ld) -> i64 {
        let mut i = 0;
        loop {
            if !ld.reg_used(i) {
                ld.register_vars.push((format!("__TEMP__{i}"), i));
                return i;
            }
            i += 1;
        }
    }

    pub fn kill_register(&mut self, ld: &mut Ld, i: i64) -> bool {
        match ld.register_vars.iter().position(|(_, v)| *v == i) {
            Some(p) => {
                ld.register_vars.remove(p);
                true
            }
            None => false,
        }
    }

    pub fn set_temp(&mut self, ld: &mut Ld, reg: &mut i64) -> Code {
        *reg = self.get_free_register(ld);
        set_loc(*reg).into_iter().collect()
    }

    pub fn dup_set_temp(&mut self, ld: &mut Ld, reg: &mut i64) -> Code {
        *reg = self.get_free_register(ld);
        let mut c = vec![ins(op::DUP, &[])];
        c.extend(set_loc(*reg));
        c
    }

    pub fn get_temp(reg: i64) -> Code {
        if reg < 0 {
            return Vec::new();
        }
        get_loc(reg).into_iter().collect()
    }

    pub fn kill_temp(&mut self, ld: &mut Ld, regs: &[i64]) -> Code {
        let mut c = Vec::new();
        for &r in regs {
            if r < 0 {
                continue;
            }
            self.kill_register(ld, r);
            c.push(ins(op::KILL, &[r]));
        }
        c
    }

    pub fn has_ret(&self, n: &Node) -> bool {
        match &*n.borrow() {
            Item::Unresolved(u) => u.resolved.as_ref().is_none_or(|r| self.has_ret(r)),
            Item::Name(x) => !x.definition,
            Item::Paren(v) => self.has_ret(v),
            Item::ConstructSuper { .. }
            | Item::Comma(_)
            | Item::Method(_)
            | Item::Class(_)
            | Item::Interface(_)
            | Item::Slot(_)
            | Item::Block(_)
            | Item::If { .. }
            | Item::While { .. }
            | Item::DoWhile { .. }
            | Item::For { .. }
            | Item::ForIn { .. }
            | Item::Switch { .. }
            | Item::Break(_)
            | Item::Continue(_)
            | Item::ReturnValue(_)
            | Item::ReturnVoid
            | Item::Throw(_)
            | Item::Try(_)
            | Item::With { .. }
            | Item::DefaultXmlNs(_)
            | Item::Empty
            | Item::Push(_)
            | Item::Swap
            | Item::Exception
            | Item::WithObject(_)
            | Item::Default => false,
            _ => true,
        }
    }

    /// `returnType()`.
    pub fn ret_type(&mut self, n: &Node) -> R<Node> {
        let it = n.borrow().clone();
        Ok(match it {
            Item::Str(_) => string_ty(),
            Item::Int(_) => ty_parts(&["int"]),
            Item::Double(_) | Item::Decimal(_) | Item::Float4(_) | Item::Nan => ty_parts(&["Number"]),
            Item::Float(_) => ty_parts(&["float"]),
            Item::Bool(_) | Item::True | Item::False => ty_parts(&["Boolean"]),
            Item::Null => ty_parts(&["null"]),
            Item::Type { .. } | Item::Unbounded => n.clone(),
            Item::Unresolved(u) => match u.resolved {
                Some(r) => self.ret_type(&r)?,
                None => u.ty.unwrap_or_else(unbounded),
            },
            Item::Name(x) => x.ty.unwrap_or_else(unbounded),
            Item::Property(_) => self.property_ret_type(n)?,
            Item::ScriptProperty { ty, .. } => ty,
            Item::Call(c) => {
                let callable = resolved_of(&c.name);
                if matches!(&*callable.borrow(), Item::Type { .. }) {
                    unbounded()
                } else {
                    self.ret_type(&callable)?
                }
            }
            Item::Construct(c) => self.ret_type(&c.name)?,
            Item::Bin { op, l, r } => match op {
                Op::Add => {
                    let lt = self.ret_type(&l)?;
                    let rt = self.ret_type(&r)?;
                    if is_ty(&lt, "String") || is_ty(&rt, "String") {
                        string_ty()
                    } else {
                        ty_parts(&["Number"])
                    }
                }
                Op::Subtract | Op::Multiply | Op::Divide | Op::Modulo => ty_parts(&["Number"]),
                Op::LShift | Op::RShift | Op::URShift | Op::BitAnd | Op::BitOr | Op::BitXor => ty_parts(&["int"]),
                Op::As => self.ret_type(&r)?,
                Op::NullCoalesce => self.ret_type(&l)?,
                _ => ty_parts(&["Boolean"]),
            },
            Item::And { .. } | Item::Or { .. } => ty_parts(&["Boolean"]),
            Item::Un { op, .. } => match op {
                Un::Not => ty_parts(&["Boolean"]),
                Un::Neg | Un::UnPlus => ty_parts(&["Number"]),
                Un::BitNot => ty_parts(&["int"]),
                Un::TypeOf => string_ty(),
            },
            Item::Coerce { ty, .. } => {
                if matches!(&*ty.borrow(), Item::Unbounded | Item::ApplyType { .. } | Item::Type { .. }) {
                    ty
                } else {
                    ty_str(&text(&ty))
                }
            }
            Item::PreInc { ty, .. } | Item::PreDec { ty, .. } | Item::PostInc { ty, .. } | Item::PostDec { ty, .. } => ty,
            Item::Delete { .. } => ty_parts(&["Boolean"]),
            Item::Ternar { t, f, .. } => {
                let a = self.ret_type(&t)?;
                let b = self.ret_type(&f)?;
                if ty_eq(&a, &b) {
                    a
                } else {
                    let num = |x: &Node| is_ty(x, "Number") || is_ty(x, "int") || is_ty(x, "uint");
                    if num(&a) && num(&b) {
                        ty_parts(&["Number"])
                    } else {
                        ty_str("--UNKNOWN--")
                    }
                }
            }
            Item::Comma(v) => match v.last() {
                Some(l) => self.ret_type(l)?,
                None => unbounded(),
            },
            Item::Paren(v) | Item::Dup(v) => self.ret_type(&v)?,
            Item::NewObject(_) => ty_parts(&["Object"]),
            Item::NewArray(_) => ty_parts(&["Array"]),
            Item::InitVector { subtype, .. } => node(Item::ApplyType { obj: ty_parts(VECTOR_FQN), params: vec![subtype] }),
            Item::RegExp { .. } => ty_parts(&["RegExp"]),
            Item::Xml(_) => ty_parts(&["XML"]),
            Item::XmlFilter { .. } => string_ty(),
            Item::Function(_) => ty_parts(&["Function"]),
            Item::HasNext { .. } => ty_parts(&["Boolean"]),
            Item::LocalReg { ty, .. } => ty,
            _ => unbounded(),
        })
    }

    /// `toSource`.
    pub fn src(&mut self, ld: &mut Ld, n: &Node) -> R<Code> {
        let it = n.borrow().clone();
        let a = &mut self.abc;
        Ok(match it {
            Item::Str(s) => vec![ins(op::PUSH_STRING, &[pool::str_id(a, &s) as i64])],
            Item::Int(v) => vec![int_ins(a, v)],
            Item::Double(v) => vec![ins(op::PUSH_DOUBLE, &[pool::double_id(a, v) as i64])],
            Item::Decimal(v) => vec![ins(op::PUSH_DECIMAL, &[pool::decimal_id(a, v) as i64])],
            Item::Float(v) => vec![ins(op::PUSH_FLOAT, &[pool::float_id(a, v) as i64])],
            Item::Float4(_) => return err(0, "float4 values are not supported"),
            Item::Bool(b) => vec![ins(if b { op::PUSH_TRUE } else { op::PUSH_FALSE }, &[])],
            Item::True => vec![ins(op::PUSH_TRUE, &[])],
            Item::False => vec![ins(op::PUSH_FALSE, &[])],
            Item::Null => vec![ins(op::PUSH_NULL, &[])],
            Item::Undefined => vec![ins(op::PUSH_UNDEFINED, &[])],
            Item::Nan => vec![ins(op::PUSH_NAN, &[])],
            Item::Type { .. } => self.gen_type(ld, n)?,
            Item::Unbounded | Item::Exception | Item::Empty | Item::Default | Item::Method(_) | Item::Class(_) | Item::Interface(_) | Item::Pop => Vec::new(),
            Item::ApplyType { obj, params } => {
                let mut m = M::new();
                let mut np = Vec::new();
                for p in &params {
                    np.extend(self.src(ld, p)?);
                }
                m.c(self.src(ld, &obj)?).c(np).op(op::APPLY_TYPE, &[params.len() as i64]);
                m.0
            }
            Item::Unresolved(u) => match u.resolved {
                Some(r) => self.src(ld, &r)?,
                None => return err(u.line, format!("Undefined variable or property: {}", u.name.raw())),
            },
            Item::Name(_) => self.name_src(ld, n, true)?,
            Item::Property(_) => self.property_src(ld, n, true)?,
            Item::Index(_) => self.index_src(ld, n, true, false, &[], false, false)?,
            Item::Namespaced(_) => self.namespaced_src(ld, n, true, false, &[], false, false)?,
            Item::ScriptProperty { .. } => self.script_property_src(ld, n, true)?,
            Item::Call(_) => self.call_src(ld, n, true)?,
            Item::Construct(_) => self.construct_src(ld, n, true)?,
            Item::ConstructSuper { obj, args } => {
                let mut m = M::new();
                m.c(self.src(ld, &obj)?);
                for x in &args {
                    m.c(self.src(ld, x)?);
                }
                m.op(op::CONSTRUCT_SUPER, &[args.len() as i64]);
                m.0
            }
            Item::Bin { op: o, l, r } => self.bin_src(ld, o, &l, &r)?,
            Item::And { l, r } => self.and_or(ld, &l, &r, true)?,
            Item::Or { l, r } => self.and_or(ld, &l, &r, false)?,
            Item::Un { op: u, v } => {
                let code = match u {
                    Un::Not => op::NOT,
                    Un::Neg => {
                        if let Some(nc) = ld.number_context {
                            let mut c = self.src(ld, &v)?;
                            c.push(ins(op::NEGATE_P, &[nc]));
                            return Ok(c);
                        }
                        op::NEGATE
                    }
                    Un::BitNot => op::BIT_NOT,
                    Un::TypeOf => op::TYPE_OF,
                    Un::UnPlus => op::UN_PLUS,
                };
                let mut c = self.src(ld, &v)?;
                c.push(ins(code, &[]));
                c
            }
            Item::Coerce { v, ty } => {
                let i = match text(&ty).as_str() {
                    "*" => ins(op::COERCE_A, &[]),
                    "String" => ins(op::COERCE_S, &[]),
                    "Boolean" => ins(op::CONVERT_B, &[]),
                    "int" => ins(op::CONVERT_I, &[]),
                    "uint" => ins(op::CONVERT_U, &[]),
                    "Number" => ins(op::CONVERT_D, &[]),
                    "float" => ins(op::CONVERT_F, &[]),
                    "float4" => ins(op::CONVERT_F4, &[]),
                    "decimal" => match ld.number_context {
                        Some(nc) => ins(op::CONVERT_MP, &[nc]),
                        None => ins(op::CONVERT_M, &[]),
                    },
                    _ => {
                        let t = self.resolve_type(Some(ld), &ty)?;
                        ins(op::COERCE, &[t as i64])
                    }
                };
                let mut c = self.src(ld, &v)?;
                c.push(i);
                c
            }
            Item::PreInc { v, .. } => self.change_src(ld, &v, false, false, true)?,
            Item::PreDec { v, .. } => self.change_src(ld, &v, false, true, true)?,
            Item::PostInc { v, .. } => self.change_src(ld, &v, true, false, true)?,
            Item::PostDec { v, .. } => self.change_src(ld, &v, true, true, true)?,
            Item::Delete { obj, line } => self.delete_src(ld, &obj, line)?,
            Item::Ternar { cond, t, f } => self.generate_if(ld, &cond, &[t], Some(&[f]), true)?,
            Item::Comma(cmds) => self.comma(ld, &cmds, true)?,
            Item::Paren(v) => self.src(ld, &v)?,
            Item::NewObject(pairs) => {
                let mut m = M::new();
                for (k, v) in &pairs {
                    m.c(self.src(ld, k)?);
                    // A numeric key is converted to its name at run time, as mxmlc does.
                    if matches!(&*k.borrow(), Item::Int(_) | Item::Double(_)) {
                        m.op(op::CONVERT_S, &[]);
                    }
                    m.c(self.src(ld, v)?);
                }
                m.op(op::NEW_OBJECT, &[pairs.len() as i64]);
                m.0
            }
            Item::NewArray(vals) => {
                let mut m = M::new();
                for v in &vals {
                    m.c(self.src(ld, v)?);
                }
                m.op(op::NEW_ARRAY, &[vals.len() as i64]);
                m.0
            }
            Item::InitVector { subtype, args, opened } => {
                let mut m = M::new();
                m.c(self.gen_type(ld, &ty_parts(VECTOR_FQN))?).c(self.src(ld, &subtype)?).op(op::APPLY_TYPE, &[1]);
                m.c(vec![int_ins(&mut self.abc, args.len() as i32)]).op(op::CONSTRUCT, &[1]);
                for (i, x) in args.iter().enumerate() {
                    let sp = self.multiname_l(false, &opened)?;
                    m.op(op::DUP, &[]).c(vec![int_ins(&mut self.abc, i as i32)]).c(self.src(ld, x)?).op(op::SET_PROPERTY, &[sp as i64]);
                }
                m.0
            }
            Item::RegExp { pattern, modifier } => {
                let rx = pool::qname_of(&mut self.abc, "RegExp", ns_kind::PACKAGE, "");
                let has_mod = !modifier.is_empty();
                let mut m = M::new();
                m.op(op::GET_LEX, &[rx as i64]);
                m.op(op::PUSH_STRING, &[self.str(&pattern) as i64]);
                if has_mod {
                    m.op(op::PUSH_STRING, &[self.str(&modifier) as i64]);
                }
                m.op(op::CONSTRUCT, &[if has_mod { 2 } else { 1 }]);
                m.0
            }
            Item::Xml(v) => {
                let x = pool::qname_of(&mut self.abc, "XML", ns_kind::PACKAGE, "");
                let mut m = M::new();
                m.op(op::GET_LEX, &[x as i64]).c(self.src(ld, &v)?).op(op::CONSTRUCT, &[1]);
                m.0
            }
            Item::EscXAttr(v) => {
                let mut c = self.src(ld, &v)?;
                c.push(ins(op::ESC_XATTR, &[]));
                c
            }
            Item::EscXElem(v) => {
                let mut c = self.src(ld, &v)?;
                c.push(ins(op::ESC_XELEM, &[]));
                c
            }
            Item::XmlFilter { obj, value, opened } => self.xml_filter(ld, &obj, &value, &opened)?,
            Item::Descendants { obj, name, opened } => {
                let set = self.ns_set_of(&opened)?;
                let ni = pool::string_id(&mut self.abc, name.as_deref(), true) as usize;
                let m_ = pool::mn(&mut self.abc, Multiname::Multiname { attr: false, name: ni, ns_set: set });
                let mut c = self.src(ld, &obj)?;
                c.push(ins(op::GET_DESCENDANTS, &[m_ as i64]));
                c
            }
            Item::Function(_) => self.function_src(ld, n)?,
            Item::Slot(_) => self.slot_src(ld, n)?,
            Item::Block(cmds) => self.gen_list(ld, &cmds)?,
            Item::If { cond, on_true, on_false } => self.generate_if(ld, &cond, &on_true, on_false.as_deref(), false)?,
            Item::While { id, expr, body } => self.gen_while(ld, id, &expr, &body)?,
            Item::DoWhile { id, body, expr } => self.gen_do_while(ld, id, &body, &expr)?,
            Item::For { id, first, expr, fin, body } => self.gen_for(ld, id, &first, &expr, &fin, &body)?,
            Item::ForIn { id, each, expr, body } => {
                let (obj, coll) = match &*expr.borrow() {
                    Item::Bin { l, r, .. } => (l.clone(), r.clone()),
                    _ => return err(0, "In expression required"),
                };
                self.gen_for_in(ld, id, &coll, &obj, &body, each)?
            }
            Item::Switch { id, expr, values, cmds, mapping } => self.gen_switch(ld, id, &expr, &values, &cmds, &mapping)?,
            Item::Break(id) => self.gen_break_continue(ld, id, true)?,
            Item::Continue(id) => self.gen_break_continue(ld, id, false)?,
            Item::ReturnValue(v) => self.gen_return_value(ld, &v)?,
            Item::ReturnVoid => self.gen_return_void(ld)?,
            Item::Throw(v) => {
                let mut c = self.src(ld, &v)?;
                c.push(ins(op::THROW, &[]));
                c
            }
            Item::Try(t) => self.gen_try(ld, &t)?,
            Item::With { scope, items, .. } => {
                let mut c = self.src(ld, &scope)?;
                let mut temp = 0;
                c.extend(self.dup_set_temp(ld, &mut temp));
                ld.scope_stack.push(node(Item::WithObject(node(Item::LocalReg { reg: temp, ty: ty_parts(&["Object"]) }))));
                c.push(ins(op::PUSH_WITH, &[]));
                c.extend(self.gen_list(ld, &items)?);
                c.push(ins(op::POP_SCOPE, &[]));
                c.extend(self.kill_temp(ld, &[temp]));
                ld.scope_stack.pop();
                c
            }
            Item::DefaultXmlNs(v) => {
                let mut c = self.src(ld, &v)?;
                c.push(ins(op::DXNS_LATE, &[]));
                c
            }
            Item::HasNext { index, obj } => {
                let regs = (local_reg(&index), local_reg(&obj));
                if let (Some(ir), Some(or)) = regs {
                    vec![ins(op::HAS_NEXT2, &[or, ir])]
                } else {
                    let mut m = M::new();
                    m.c(self.src(ld, &obj)?).c(self.src(ld, &index)?).op(op::HAS_NEXT, &[]);
                    m.0
                }
            }
            Item::NewActivation => vec![ins(op::NEW_ACTIVATION, &[])],
            Item::NextName { index, obj } => {
                let mut m = M::new();
                m.c(self.src(ld, &obj)?).c(self.src(ld, &index)?).op(op::NEXT_NAME, &[]);
                m.0
            }
            Item::NextValue { index, obj } => {
                let mut m = M::new();
                m.c(self.src(ld, &obj)?).c(self.src(ld, &index)?).op(op::NEXT_VALUE, &[]);
                m.0
            }
            Item::Dup(_) => vec![ins(op::DUP, &[])],
            Item::Push(v) => self.src(ld, &v)?,
            Item::Swap => vec![ins(op::SWAP, &[])],
            Item::LocalReg { reg, .. } => get_loc(reg).into_iter().collect(),
            Item::WithObject(s) => self.src(ld, &s)?,
            Item::FindProperty(t) => {
                let tn = self.type_name(Some(ld), &t)?;
                vec![ins(op::FIND_PROPERTY, &[tn as i64])]
            }
            Item::ForInAssigned { collection, counter, each } => {
                let mut c = Cx::get_temp(collection);
                c.extend(Cx::get_temp(counter));
                c.push(ins(if each { op::NEXT_VALUE } else { op::NEXT_NAME }, &[]));
                c
            }
        })
    }

    /// `toSourceIgnoreReturnValue`.
    pub fn src_ignore(&mut self, ld: &mut Ld, n: &Node) -> R<Code> {
        let it = n.borrow().clone();
        match it {
            Item::Unresolved(u) => match u.resolved {
                Some(r) => self.src_ignore(ld, &r),
                None => err(u.line, format!("Undefined variable or property: {}", u.name.raw())),
            },
            Item::Name(nm) => match nm.redirect {
                Some(r) => self.src_ignore(ld, &r),
                None => self.name_src(ld, n, false),
            },
            Item::Property(_) => self.property_src(ld, n, false),
            Item::Index(_) => self.index_src(ld, n, false, false, &[], false, false),
            Item::Namespaced(_) => self.namespaced_src(ld, n, false, false, &[], false, false),
            Item::ScriptProperty { .. } => self.script_property_src(ld, n, false),
            Item::Call(_) => self.call_src(ld, n, false),
            Item::Construct(_) => self.construct_src(ld, n, false),
            Item::Comma(cmds) => self.comma(ld, &cmds, false),
            Item::PreInc { v, .. } => self.change_src(ld, &v, false, false, false),
            Item::PreDec { v, .. } => self.change_src(ld, &v, false, true, false),
            Item::PostInc { v, .. } => self.change_src(ld, &v, true, false, false),
            Item::PostDec { v, .. } => self.change_src(ld, &v, true, true, false),
            _ => {
                let mut c = self.src(ld, n)?;
                if self.has_ret(n) {
                    c.push(ins(op::POP, &[]));
                }
                Ok(c)
            }
        }
    }

    /// `generate(localData, commands)`, with mxmlc's peephole: a statement that stores a local
    /// the next one starts by reading becomes `dup; setlocal`.
    pub fn gen_list(&mut self, ld: &mut Ld, cmds: &[Node]) -> R<Code> {
        let mut c: Code = Vec::new();
        let mut last_start = 0;
        for (k, i) in cmds.iter().enumerate() {
            ld.for_in_regs = if k >= 2 { self.declared_for_in(ld, &cmds[k - 2], &cmds[k - 1], i) } else { None };
            ld.following.push(cmds[k + 1..].to_vec());
            let part = self.src_ignore(ld, i);
            ld.following.pop();
            ld.for_in_regs = None;
            let mut part = part?;
            let reusable = !matches!(&*i.borrow(), Item::DoWhile { .. } | Item::Try(_) | Item::With { .. });
            if reusable && reuses_stored_local(&c[last_start..], &part) {
                part.remove(0);
                let at = c.len() - 1;
                c.insert(at, ins(op::DUP, &[]));
            }
            last_start = c.len();
            c.extend(part);
        }
        Ok(c)
    }

    fn comma(&mut self, ld: &mut Ld, cmds: &[Node], with_ret: bool) -> R<Code> {
        let Some((last, rest)) = cmds.split_last() else { return Ok(Vec::new()) };
        let mut c = self.gen_list(ld, rest)?;
        c.extend(if with_ret { self.src(ld, last)? } else { self.src_ignore(ld, last)? });
        Ok(c)
    }

    fn change_src(&mut self, ld: &mut Ld, v: &Node, post: bool, dec: bool, needs_ret: bool) -> R<Code> {
        if is_assignable(v) {
            return self.to_source_change(ld, v, post, dec, needs_ret);
        }
        Ok(Vec::new())
    }

    fn bin_src(&mut self, ld: &mut Ld, o: Op, l: &Node, r: &Node) -> R<Code> {
        if o == Op::NullCoalesce {
            let mut c = self.src(ld, l)?;
            c.push(ins(op::PUSH_NULL, &[]));
            let if_eq = ins(op::IF_EQ, &[0]);
            c.push(if_eq.clone());
            let mut on_not_null = self.src(ld, l)?;
            let jump = ins(op::JUMP, &[0]);
            on_not_null.push(jump.clone());
            if_eq.borrow_mut().ops[0] = code_len(&on_not_null);
            let on_null = self.src(ld, r)?;
            jump.borrow_mut().ops[0] = code_len(&on_null);
            c.extend(on_not_null);
            c.extend(on_null);
            return Ok(c);
        }
        let nc = ld.number_context;
        let (code, extra): (u8, Option<u8>) = match o {
            Op::Add => (if nc.is_some() { op::ADD_P } else { op::ADD }, None),
            Op::Subtract => (if nc.is_some() { op::SUBTRACT_P } else { op::SUBTRACT }, None),
            Op::Multiply => (if nc.is_some() { op::MULTIPLY_P } else { op::MULTIPLY }, None),
            Op::Divide => (if nc.is_some() { op::DIVIDE_P } else { op::DIVIDE }, None),
            Op::Modulo => (if nc.is_some() { op::MODULO_P } else { op::MODULO }, None),
            Op::LShift => (op::LSHIFT, None),
            Op::RShift => (op::RSHIFT, None),
            Op::URShift => (op::URSHIFT, None),
            Op::BitAnd => (op::BIT_AND, None),
            Op::BitOr => (op::BIT_OR, None),
            Op::BitXor => (op::BIT_XOR, None),
            Op::Eq => (op::EQUALS, None),
            Op::StrictEq => (op::STRICT_EQUALS, None),
            Op::Neq => (op::EQUALS, Some(op::NOT)),
            Op::StrictNeq => (op::STRICT_EQUALS, Some(op::NOT)),
            Op::Lt => (op::LESS_THAN, None),
            Op::Le => (op::LESS_EQUALS, None),
            Op::Gt => (op::GREATER_THAN, None),
            Op::Ge => (op::GREATER_EQUALS, None),
            Op::In => (op::IN, None),
            Op::InstanceOf => (op::INSTANCE_OF, None),
            Op::Is => (op::IS_TYPE_LATE, None),
            Op::As => (op::AS_TYPE_LATE, None),
            Op::NullCoalesce => unreachable!(),
        };
        let mut c = self.src(ld, l)?;
        c.extend(self.src(ld, r)?);
        let p = matches!(o, Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::Modulo) && nc.is_some();
        c.push(ins(code, if p { std::slice::from_ref(nc.as_ref().unwrap()) } else { &[] }));
        if let Some(e) = extra {
            c.push(ins(e, &[]));
        }
        Ok(c)
    }

    /// The instruction a compound assignment applies, `getOperatorInstruction`.
    pub fn compound_op(&mut self, ld: &Ld, n: &Node) -> Option<I> {
        match &*n.borrow() {
            Item::Bin { op: o, .. } if o.compoundable() => {
                let nc = ld.number_context;
                let pick = |plain: u8, p: u8| match nc {
                    Some(c) => ins(p, &[c]),
                    None => ins(plain, &[]),
                };
                Some(match o {
                    Op::Add => pick(op::ADD, op::ADD_P),
                    Op::Subtract => pick(op::SUBTRACT, op::SUBTRACT_P),
                    Op::Multiply => pick(op::MULTIPLY, op::MULTIPLY_P),
                    Op::Divide => pick(op::DIVIDE, op::DIVIDE_P),
                    Op::Modulo => pick(op::MODULO, op::MODULO_P),
                    Op::LShift => ins(op::LSHIFT, &[]),
                    Op::RShift => ins(op::RSHIFT, &[]),
                    Op::URShift => ins(op::URSHIFT, &[]),
                    Op::BitAnd => ins(op::BIT_AND, &[]),
                    Op::BitOr => ins(op::BIT_OR, &[]),
                    Op::BitXor => ins(op::BIT_XOR, &[]),
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    fn and_or(&mut self, ld: &mut Ld, l: &Node, r: &Node, and: bool) -> R<Code> {
        let mut c = self.src(ld, l)?;
        c.push(ins(op::DUP, &[]));
        let lt = self.ret_type(l)?;
        if text(&lt) != "Boolean" {
            c.push(ins(op::CONVERT_B, &[]));
        }
        let mut e = self.src(ld, r)?;
        e.insert(0, ins(op::POP, &[]));
        c.push(ins(if and { op::IF_FALSE } else { op::IF_TRUE }, &[code_len(&e)]));
        c.extend(e);
        Ok(c)
    }

    fn condition(&mut self, ld: &mut Ld, t: &Node, offset: i64, negate: bool) -> R<Code> {
        let cond = match &*t.borrow() {
            Item::Bin { op: o, l, r } => o.if_codes().map(|c| (c, l.clone(), r.clone())),
            _ => None,
        };
        if let Some(((yes, no), l, r)) = cond {
            let mut c = self.src(ld, &l)?;
            c.extend(self.src(ld, &r)?);
            c.push(ins(if negate { no } else { yes }, &[offset]));
            return Ok(c);
        }
        let mut c = self.src(ld, t)?;
        c.push(ins(if negate { op::IF_FALSE } else { op::IF_TRUE }, &[offset]));
        Ok(c)
    }

    fn generate_if(&mut self, ld: &mut Ld, expr: &Node, on_true_cmds: &[Node], on_false_cmds: Option<&[Node]>, ternar: bool) -> R<Code> {
        let mut on_true = if ternar { self.src(ld, &on_true_cmds[0])? } else { self.gen_list(ld, on_true_cmds)? };
        let mut on_false = None;
        if let Some(f) = on_false_cmds {
            if !f.is_empty() {
                on_false = Some(if ternar { self.src(ld, &f[0])? } else { self.gen_list(ld, f)? });
            }
        }
        let mut ajmp = None;
        if on_false.is_some() {
            let last_jumps = on_true_cmds.last().is_some_and(|l| matches!(&*l.borrow(), Item::Continue(_) | Item::Break(_)));
            if on_true_cmds.is_empty() || !last_jumps {
                let j = ins(op::JUMP, &[0]);
                on_true.push(j.clone());
                ajmp = Some(j);
            }
        }
        let true_len = code_len(&on_true);
        let mut ret = self.condition(ld, expr, true_len, true)?;
        ret.extend(on_true);
        if let Some(f) = on_false {
            if let Some(j) = ajmp {
                j.borrow_mut().ops[0] = code_len(&f);
            }
            ret.extend(f);
        }
        Ok(ret)
    }

    fn fix_loop(code: &[I], break_offset: i64, continue_offset: Option<i64>, id: i64) {
        let mut pos = 0;
        for i in code {
            pos += i.borrow().len() as i64;
            let p = i.borrow().pseudo;
            match p {
                Pseudo::Continue(l) if l == id => {
                    if let Some(co) = continue_offset {
                        let mut b = i.borrow_mut();
                        b.ops[0] = -pos + co;
                        b.pseudo = Pseudo::None;
                        b.code = op::JUMP;
                    }
                }
                Pseudo::Break(l) if l == id => {
                    let mut b = i.borrow_mut();
                    b.ops[0] = -pos + break_offset;
                    b.pseudo = Pseudo::None;
                    b.code = op::JUMP;
                }
                _ => {}
            }
        }
    }

    fn split_expr(exprs: &[Node]) -> (Vec<Node>, Option<Node>) {
        let mut ex: Vec<Node> = exprs.to_vec();
        let mut last = ex.pop();
        while let Some(l) = last.clone() {
            let inner = match &*l.borrow() {
                Item::Comma(c) => Some(c.clone()),
                _ => None,
            };
            match inner {
                Some(c) => {
                    ex.extend(c);
                    last = ex.pop();
                }
                None => break,
            }
        }
        (ex, last)
    }

    fn gen_while(&mut self, ld: &mut Ld, id: LoopId, expr: &[Node], body: &[Node]) -> R<Code> {
        ld.opened_loops.push(id.0);
        let mut ret = Vec::new();
        let (ex, last) = Cx::split_expr(expr);
        let mut while_expr = self.gen_list(ld, &ex)?;
        let mut while_body = self.gen_list(ld, body)?;
        let fwd = ins(op::JUMP, &[0]);
        ret.push(fwd.clone());
        while_body.insert(0, ins(op::LABEL, &[]));
        ret.extend(while_body.iter().cloned());
        let body_len = code_len(&while_body);
        fwd.borrow_mut().ops[0] = body_len;
        let last = last.ok_or(CompileError { line: 0, message: "Missing loop condition".into() })?;
        while_expr.extend(self.condition(ld, &last, 0, false)?);
        let expr_len = code_len(&while_expr);
        while_expr.last().unwrap().borrow_mut().ops[0] = -(expr_len + body_len);
        ret.extend(while_expr);
        Cx::fix_loop(&while_body, body_len + expr_len, Some(body_len), id.0);
        Ok(ret)
    }

    fn gen_do_while(&mut self, ld: &mut Ld, id: LoopId, body: &[Node], expr: &[Node]) -> R<Code> {
        ld.opened_loops.push(id.0);
        let mut ret = Vec::new();
        let (ex, last) = Cx::split_expr(expr);
        let mut while_expr = self.gen_list(ld, &ex)?;
        let body_c = self.gen_list(ld, body)?;
        let label = vec![ins(op::LABEL, &[])];
        let label_len = code_len(&label);
        ret.push(ins(op::JUMP, &[label_len]));
        ret.extend(label);
        ret.extend(body_c.iter().cloned());
        let body_len = code_len(&body_c);
        let last = last.ok_or(CompileError { line: 0, message: "Missing loop condition".into() })?;
        while_expr.extend(self.condition(ld, &last, 0, false)?);
        let expr_len = code_len(&while_expr);
        while_expr.last().unwrap().borrow_mut().ops[0] = -(expr_len + body_len + label_len);
        ret.extend(while_expr);
        Cx::fix_loop(&body_c, body_len + expr_len, Some(body_len), id.0);
        Ok(ret)
    }

    fn gen_for(&mut self, ld: &mut Ld, id: LoopId, first: &[Node], expr: &Node, fin: &[Node], body: &[Node]) -> R<Code> {
        ld.opened_loops.push(id.0);
        let mut ret = Vec::new();
        let (ex, last) = Cx::split_expr(std::slice::from_ref(expr));
        let mut for_expr = self.gen_list(ld, &ex)?;
        let mut for_body = self.gen_list(ld, body)?;
        let for_final = self.gen_list(ld, fin)?;
        ret.extend(self.gen_list(ld, first)?);
        let fwd = ins(op::JUMP, &[0]);
        ret.push(fwd.clone());
        for_body.insert(0, ins(op::LABEL, &[]));
        ret.extend(for_body.iter().cloned());
        ret.extend(for_final.iter().cloned());
        let body_len = code_len(&for_body);
        let fin_len = code_len(&for_final);
        fwd.borrow_mut().ops[0] = body_len + fin_len;
        let last = last.unwrap_or_else(|| node(Item::Bool(true)));
        for_expr.extend(self.condition(ld, &last, 0, false)?);
        let expr_len = code_len(&for_expr);
        for_expr.last().unwrap().borrow_mut().ops[0] = -(expr_len + body_len + fin_len);
        ret.extend(for_expr);
        Cx::fix_loop(&for_body, body_len + fin_len + expr_len, Some(body_len), id.0);
        Ok(ret)
    }

    /// `var i:int = 0; var o = x; for(k in o)`, as the decompiler prints mxmlc's for-in: the
    /// loop's registers are `i` and `o` when nothing else reads them.
    fn declared_for_in(&self, ld: &Ld, counter: &Node, coll: &Node, for_in: &Node) -> Option<(i64, i64)> {
        let decl = |n: &Node| match &*n.borrow() {
            Item::Name(nm) if nm.definition => nm.assigned.clone().map(|a| (nm.var.clone(), a)),
            _ => None,
        };
        let (cname, cval) = decl(counter)?;
        let (oname, _) = decl(coll)?;
        if !matches!(&*cval.borrow(), Item::Int(0)) || cname == oname {
            return None;
        }
        let (expr, body) = match &*for_in.borrow() {
            Item::ForIn { expr, body, .. } => (expr.clone(), body.clone()),
            _ => return None,
        };
        let names_coll = match &*expr.borrow() {
            Item::Bin { r, .. } => match &*r.borrow() {
                Item::Name(nm) => nm.var == oname,
                Item::Unresolved(u) => u.resolved.as_ref().is_some_and(|x| matches!(&*x.borrow(), Item::Name(nm) if nm.var == oname)),
                _ => false,
            },
            _ => false,
        };
        if !names_coll {
            return None;
        }
        let quoted = [format!("\"{cname}\""), format!("\"{oname}\"")];
        let mentions = |nodes: &[Node]| nodes.iter().any(|n| {
            let t = format!("{:?}", n.borrow());
            quoted.iter().any(|q| t.contains(q.as_str()))
        });
        if mentions(&body) || ld.following.iter().any(|f| mentions(f)) {
            return None;
        }
        let reg = |name: &str| ld.register_vars.iter().find(|(k, _)| k == name).map(|(_, r)| *r);
        Some((reg(&cname)?, reg(&oname)?))
    }

    fn gen_for_in(&mut self, ld: &mut Ld, id: LoopId, collection: &Node, assignable: &Node, commands: &[Node], each: bool) -> R<Code> {
        let declared = ld.for_in_regs.take();
        ld.opened_loops.push(id.0);
        let mut ret = Vec::new();
        let mut counter = 0;
        let mut coll = 0;
        let assignable = {
            let r = match &*assignable.borrow() {
                Item::Unresolved(u) => u.resolved.clone(),
                _ => None,
            };
            r.unwrap_or_else(|| assignable.clone())
        };
        if let Some((c, o)) = declared {
            (counter, coll) = (c, o);
        } else {
            ret.push(ins(op::PUSH_BYTE, &[0]));
            ret.extend(self.set_temp(ld, &mut counter));
            ret.extend(self.src(ld, collection)?);
            ret.push(self.generate_coerce(Some(ld), &unbounded())?);
            ret.extend(self.set_temp(ld, &mut coll));
        }
        let assigned = node(Item::ForInAssigned { collection: coll, counter, each });
        set_assigned(&assignable, Some(assigned));
        let mut for_body = vec![ins(op::LABEL, &[])];
        for_body.extend(self.src_ignore(ld, &assignable)?);
        for_body.extend(self.gen_list(ld, commands)?);
        let body_len = code_len(&for_body);
        ret.push(ins(op::JUMP, &[body_len]));
        let back = ins(op::IF_TRUE, &[0]);
        let expr = vec![ins(op::HAS_NEXT2, &[coll, counter]), back.clone()];
        let expr_len = code_len(&expr);
        back.borrow_mut().ops[0] = -(expr_len + body_len);
        Cx::fix_loop(&for_body, body_len + expr_len, Some(body_len), id.0);
        ret.extend(for_body);
        ret.extend(expr);
        ret.extend(self.kill_temp(ld, &[coll, counter]));
        Ok(ret)
    }

    fn gen_switch(&mut self, ld: &mut Ld, id: LoopId, switched: &Node, values: &[Node], cmds: &[Nodes], mapping: &[usize]) -> R<Code> {
        ld.opened_loops.push(id.0);
        let mut ret = Vec::new();
        let mut switched_reg = 0;
        let fwd = ins(op::JUMP, &[0]);
        ret.push(fwd.clone());
        let mut def_index = values.len();
        let mut has_default = false;
        for i in (0..values.len()).rev() {
            if matches!(&*values[i].borrow(), Item::Default) {
                has_default = true;
                def_index = i;
                break;
            }
        }
        let real = values.len() as i64 - i64::from(has_default);
        let mut cases: Code = vec![int_ins(&mut self.abc, -1)];
        let mut pre = self.src(ld, switched)?;
        pre.extend(self.set_temp(ld, &mut switched_reg));
        let mut pos = real - 1;
        for i in (0..values.len()).rev() {
            if matches!(&*values[i].borrow(), Item::Default) {
                continue;
            }
            let mut sub = vec![int_ins(&mut self.abc, pos as i32)];
            pos -= 1;
            sub.push(ins(op::JUMP, &[code_len(&cases)]));
            let sub_len = code_len(&sub);
            let mut head = self.src(ld, &values[i])?;
            head.extend(Cx::get_temp(switched_reg));
            head.push(ins(op::IF_STRICT_NE, &[sub_len]));
            head.extend(sub);
            head.extend(cases);
            cases = head;
        }
        pre.extend(cases);
        let mut cases = pre;
        let lookup = ins(op::LOOKUP_SWITCH, &vec![0; (2 + real) as usize]);
        cases.extend(self.kill_temp(ld, &[switched_reg]));
        let cases_len = code_len(&cases);
        let mut bodies: Code = vec![ins(op::LABEL, &[]), pseudo(Pseudo::Break(id.0))];
        let mut offsets: Vec<i64> = Vec::new();
        let mut def_offset = -(code_len(&bodies) + cases_len);
        for i in (0..cmds.len()).rev() {
            let mut b = self.gen_list(ld, &cmds[i])?;
            b.insert(0, ins(op::LABEL, &[]));
            b.extend(bodies);
            bodies = b;
            offsets.insert(0, -(code_len(&bodies) + cases_len));
        }
        if has_default {
            def_offset = offsets[mapping[def_index]];
        }
        {
            let mut l = lookup.borrow_mut();
            l.ops[0] = def_offset;
            l.ops[1] = real - 1;
            let mut p = 0;
            for i in 0..mapping.len() {
                if matches!(&*values[i].borrow(), Item::Default) {
                    continue;
                }
                l.ops[2 + p] = offsets[mapping[i]];
                p += 1;
            }
        }
        fwd.borrow_mut().ops[0] = code_len(&bodies);
        ret.extend(bodies);
        ret.extend(cases);
        ret.push(lookup);
        let total = code_len(&ret);
        Cx::fix_loop(&ret, total, None, id.0);
        Ok(ret)
    }

    fn finally_jumps(&mut self, ld: &mut Ld, loop_id: Option<i64>, ret: &mut Code, ascending: bool) {
        let n = ld.finally_catches.len();
        let order: Vec<usize> = if ascending { (0..n).collect() } else { (0..n).rev().collect() };
        for i in order {
            if let Some(l) = loop_id {
                if !ld.finally_opened_loops[i].contains(&l) {
                    continue;
                }
            }
            let first = if ascending { i == 0 } else { i == n - 1 };
            if !first {
                ret.push(ins(op::LABEL, &[]));
            }
            let clause = ld.finally_catches[i];
            let cnt = ld.finally_counter.get(&clause).copied().unwrap_or(-1) + 1;
            ld.finally_counter.insert(clause, cnt);
            ret.push(int_ins(&mut self.abc, cnt as i32));
            ret.push(pseudo(Pseudo::Finally(clause)));
            ret.push(ins(op::LABEL, &[]));
            ret.push(ins(op::POP, &[]));
        }
    }

    fn gen_break_continue(&mut self, ld: &mut Ld, id: i64, brk: bool) -> R<Code> {
        let mut ret = Vec::new();
        if !ld.finally_catches.is_empty() {
            self.finally_jumps(ld, Some(id), &mut ret, false);
            ret.push(ins(op::LABEL, &[]));
        }
        for i in 0..ld.catches_opened_loops.len() {
            if ld.catches_opened_loops[i].contains(&id) {
                ret.push(ins(op::POP_SCOPE, &[]));
            }
            ret.push(ins(op::KILL, &[ld.catches_temp_regs[i]]));
        }
        ret.push(pseudo(if brk { Pseudo::Break(id) } else { Pseudo::Continue(id) }));
        Ok(ret)
    }

    fn gen_return_value(&mut self, ld: &mut Ld, v: &Node) -> R<Code> {
        if let Some(rt) = ld.return_type.clone() {
            handle_and_or_coerce(v, &rt);
        }
        let mut ret = self.src(ld, v)?;
        if !ld.finally_catches.is_empty() {
            ret.push(ins(op::COERCE_A, &[]));
            ret.extend(set_loc(ld.finally_register));
            self.finally_jumps(ld, None, &mut ret, false);
            ret.push(ins(op::LABEL, &[]));
            ret.extend(get_loc(ld.finally_register));
            ret.push(ins(op::KILL, &[ld.finally_register]));
        }
        ret.push(ins(op::RETURN_VALUE, &[]));
        Ok(ret)
    }

    fn gen_return_void(&mut self, ld: &mut Ld) -> R<Code> {
        let mut ret = Vec::new();
        if !ld.finally_catches.is_empty() {
            self.finally_jumps(ld, None, &mut ret, true);
            ret.push(ins(op::LABEL, &[]));
        }
        ret.push(ins(op::RETURN_VOID, &[]));
        Ok(ret)
    }

    fn scope_restore(&mut self, ld: &mut Ld) -> R<Code> {
        let mut c = Vec::new();
        for s in ld.scope_stack.clone() {
            c.extend(self.src(ld, &s)?);
            let with = matches!(&*s.borrow(), Item::WithObject(_));
            c.push(ins(if with { op::PUSH_WITH } else { op::PUSH_SCOPE }, &[]));
        }
        Ok(c)
    }

    fn gen_try(&mut self, ld: &mut Ld, item: &Try) -> R<Code> {
        let mut ret: Code = Vec::new();
        let mut new_finally_reg = false;
        let mut newex: Vec<Exc> = Vec::new();
        let mut alone_finally: i64 = -1;
        let mut finally_ex: i64 = -1;
        for e in &item.catch_exceptions {
            let (var, ety) = match &*e.borrow() {
                Item::Name(n) => (n.var.clone(), n.ty.clone().unwrap_or_else(unbounded)),
                _ => continue,
            };
            let sn = self.str(&var);
            let pub_ns = pool::ns(&mut self.abc, ns_kind::PACKAGE, "");
            let name = pool::mn(&mut self.abc, Multiname::QName { attr: false, ns: pub_ns, name: sn });
            let ty = self.type_name(Some(ld), &ety)?;
            newex.push(Exc { name, ty, ..Default::default() });
        }
        let mut fin = 0;
        if item.finally_cmds.is_some() {
            if item.catch_exceptions.is_empty() {
                newex.push(Exc::default());
                alone_finally = newex.len() as i64 - 1;
            }
            newex.push(Exc::default());
            finally_ex = newex.len() as i64 - 1;
            if ld.finally_register == -1 {
                ld.finally_register = self.get_free_register(ld);
                let r = ld.finally_register;
                self.kill_register(ld, r);
                new_finally_reg = true;
            }
            fin = self.fin_id();
        }
        if finally_ex > -1 {
            ld.finally_catches.push(fin);
            ld.finally_opened_loops.push(ld.opened_loops.clone());
        }
        let try_cmds = self.gen_list(ld, &item.try_cmds)?;
        let mut catches: Code = Vec::new();
        let mut temp = 0;
        let mut current_ids = Vec::new();
        let mut catch_cmds: Vec<Code> = Vec::new();
        for c in 0..item.catch_cmds.len() {
            let i = ld.exceptions.len();
            ld.exceptions.push(newex[c].clone());
            current_ids.push(i);
            let mut cc = vec![ins(op::NEW_CATCH, &[i as i64])];
            cc.extend(self.dup_set_temp(ld, &mut temp));
            cc.push(ins(op::DUP, &[]));
            cc.push(ins(op::PUSH_SCOPE, &[]));
            cc.push(ins(op::SWAP, &[]));
            cc.push(ins(op::SET_SLOT, &[1]));
            let ename = match &*item.catch_exceptions[c].borrow() {
                Item::Name(n) => n.var.clone(),
                _ => String::new(),
            };
            for a in &item.catch_variables[c] {
                let r = match &*a.borrow() {
                    Item::Unresolved(u) => u.resolved_root.clone(),
                    _ => Some(a.clone()),
                };
                if let Some(r) = r {
                    if let Item::Name(n) = &mut *r.borrow_mut() {
                        if n.var == ename {
                            n.slot_scope = ld.scope_stack.len() as i64;
                        }
                    }
                }
            }
            ld.catches_temp_regs.push(temp);
            ld.scope_stack.push(node(Item::LocalReg { reg: temp, ty: ty_parts(&["Object"]) }));
            ld.catches_opened_loops.push(ld.opened_loops.clone());
            cc.extend(self.gen_list(ld, &item.catch_cmds[c])?);
            ld.catches_opened_loops.pop();
            ld.scope_stack.pop();
            ld.catches_temp_regs.pop();
            let ends_jump = item.catch_cmds[c].last().is_some_and(|l| matches!(&*l.borrow(), Item::Continue(_) | Item::Break(_)));
            if ends_jump {
                self.kill_register(ld, temp);
            } else {
                cc.push(ins(op::POP_SCOPE, &[]));
                cc.extend(self.kill_temp(ld, &[temp]));
            }
            catch_cmds.push(cc);
        }
        for c in (0..item.catch_cmds.len()).rev() {
            let mut pre = self.scope_restore(ld)?;
            pre.extend(catch_cmds[c].iter().cloned());
            pre.extend(catches);
            catches = pre;
            catches.insert(0, mark(current_ids[c], ins::MARK_TARGET));
            let l = code_len(&catches);
            catches.insert(0, ins(op::JUMP, &[l]));
        }
        if alone_finally > -1 {
            ld.exceptions.push(newex[alone_finally as usize].clone());
            alone_finally = ld.exceptions.len() as i64 - 1;
        }
        if finally_ex > -1 {
            ld.exceptions.push(newex[finally_ex as usize].clone());
            finally_ex = ld.exceptions.len() as i64 - 1;
        }
        for &i in &current_ids {
            ret.push(mark(i, ins::MARK_START));
        }
        if alone_finally > -1 {
            ret.push(mark(alone_finally as usize, ins::MARK_START));
        }
        if finally_ex > -1 {
            ret.push(mark(finally_ex as usize, ins::MARK_START));
        }
        ret.extend(try_cmds);
        for &i in &current_ids {
            ret.push(mark(i, ins::MARK_END));
        }
        if alone_finally > -1 {
            ret.push(mark(alone_finally as usize, ins::MARK_END));
            let mut pre = self.scope_restore(ld)?;
            pre.push(ins(op::NEW_CATCH, &[alone_finally]));
            pre.extend(self.dup_set_temp(ld, &mut temp));
            pre.push(ins(op::PUSH_SCOPE, &[]));
            pre.push(ins(op::THROW, &[]));
            pre.push(ins(op::POP_SCOPE, &[]));
            pre.extend(self.kill_temp(ld, &[temp]));
            catches.push(ins(op::JUMP, &[code_len(&pre)]));
            catches.push(mark(alone_finally as usize, ins::MARK_TARGET));
            catches.extend(pre);
        }
        let mut fin_switch: Option<I> = None;
        let push_def = ins(op::PUSH_BYTE, &[0]);
        let mut def_pos = 0;
        if finally_ex > -1 {
            let fe = finally_ex as usize;
            let mut pre = vec![mark(fe, ins::MARK_TARGET)];
            pre.extend(self.scope_restore(ld)?);
            pre.push(ins(op::NEW_CATCH, &[finally_ex]));
            pre.extend(self.dup_set_temp(ld, &mut temp));
            pre.push(ins(op::PUSH_SCOPE, &[]));
            pre.push(ins(op::POP_SCOPE, &[]));
            let mut temp2 = 0;
            pre.push(ins(op::KILL, &[temp]));
            pre.push(ins(op::COERCE_A, &[]));
            pre.extend(self.set_temp(ld, &mut temp2));
            pre.push(push_def.clone());
            let sw = ins(op::LOOKUP_SWITCH, &[0, 0, 0]);
            let sl = sw.borrow().len() as i64;
            sw.borrow_mut().ops[0] = sl;
            let pre_switch = vec![ins(op::LABEL, &[]), ins(op::POP, &[])];
            let pre_switch_len = code_len(&pre_switch);
            let mut switch_cmds = vec![ins(op::LABEL, &[])];
            switch_cmds.extend(Cx::get_temp(temp2));
            switch_cmds.push(ins(op::KILL, &[temp2]));
            switch_cmds.push(ins(op::THROW, &[]));
            let mut sw_len = code_len(&switch_cmds);
            pre.push(ins(op::JUMP, &[pre_switch_len + sw_len]));
            let fjump = ins(op::JUMP, &[code_len(&pre) + pre_switch_len + sw_len]);
            pre.insert(0, fjump);
            pre.insert(0, mark(fe, ins::MARK_END));
            pre.insert(0, ins(op::PUSH_BYTE, &[-1]));
            switch_cmds.push(mark(fe, ins::MARK_FINALLY_PART));
            let old = ld.finally_register;
            ld.finally_register = self.get_free_register(ld);
            let cnt = ld.finally_counter.get(&fin).copied().unwrap_or(-1);
            def_pos = cnt;
            ld.finally_counter.insert(fin, cnt + 1);
            switch_cmds.extend(self.gen_list(ld, item.finally_cmds.as_deref().unwrap_or(&[]))?);
            let fr = ld.finally_register;
            self.kill_register(ld, fr);
            ld.finally_register = old;
            sw_len = code_len(&switch_cmds);
            sw.borrow_mut().ops[2] = -sw_len;
            pre.extend(pre_switch);
            pre.extend(switch_cmds);
            pre.push(sw.clone());
            catches.extend(pre);
            self.kill_temp(ld, &[temp, temp2]);
            fin_switch = Some(sw);
        }
        ret.extend(catches);
        if finally_ex > -1 {
            ld.finally_catches.pop();
            ld.finally_opened_loops.pop();
        }
        if new_finally_reg {
            ld.finally_register = -1;
            self.kill_register(ld, -1);
        }
        let mut pos = 0i64;
        let mut finally_pos = 0i64;
        let mut switch_pos = 0i64;
        let mut s = 0;
        while s < ret.len() {
            if fin_switch.as_ref().is_some_and(|f| Rc::ptr_eq(f, &ret[s])) {
                switch_pos = pos;
            }
            let p = ret[s].borrow().pseudo;
            if let Pseudo::Mark { exception, kind } = p {
                if exception as i64 == finally_ex && kind == ins::MARK_FINALLY_PART {
                    finally_pos = pos;
                    ret.remove(s);
                    continue;
                }
            }
            pos += ret[s].borrow().len() as i64;
            s += 1;
        }
        if let Some(sw) = &fin_switch {
            pos = 0;
            let def_loc = sw.borrow().ops[2];
            let mut switch_loc: Vec<i64> = Vec::new();
            let mut was_def = false;
            for i in &ret {
                let p = i.borrow().pseudo;
                if p == Pseudo::Finally(fin) {
                    let bet_len = 2;
                    let l = i.borrow().len() as i64;
                    {
                        let mut b = i.borrow_mut();
                        b.ops[0] = if was_def { 0 } else { finally_pos - (pos + l) };
                        b.pseudo = Pseudo::None;
                        b.code = op::JUMP;
                    }
                    switch_loc.push(pos + l + bet_len - switch_pos);
                }
                pos += i.borrow().len() as i64;
                if def_pos == switch_loc.len() as i64 - 1 {
                    switch_loc.push(def_loc);
                    was_def = true;
                }
            }
            push_def.borrow_mut().ops[0] = def_pos + 1;
            let mut ops = vec![0, switch_loc.len() as i64 - 1];
            ops.extend(switch_loc);
            sw.borrow_mut().ops = ops;
            let after = sw.borrow().len() as i64;
            sw.borrow_mut().ops[0] = after;
        }
        Ok(ret)
    }

    fn xml_filter(&mut self, ld: &mut Ld, obj: &Node, value: &Node, opened: &NsList) -> R<Code> {
        let mut ret = Vec::new();
        let mut counter = 0;
        let mut coll = 0;
        let mut list = 0;
        let list_set = self.set_temp(ld, &mut list);
        let xl = pool::qname_of(&mut self.abc, "XMLList", ns_kind::PACKAGE, "");
        let empty = self.str("");
        ret.push(ins(op::PUSH_BYTE, &[0]));
        ret.extend(self.set_temp(ld, &mut counter));
        ret.extend(self.src(ld, obj)?);
        ret.push(ins(op::CHECK_FILTER, &[]));
        ret.push(self.generate_coerce(Some(ld), &unbounded())?);
        ret.extend(self.set_temp(ld, &mut coll));
        ret.push(ins(op::GET_LEX, &[xl as i64]));
        ret.push(ins(op::PUSH_STRING, &[empty as i64]));
        ret.push(ins(op::CONSTRUCT, &[1]));
        ret.extend(list_set);
        let mut t1 = 0;
        let mut t2 = 0;
        let mut body = vec![ins(op::LABEL, &[])];
        body.extend(Cx::get_temp(coll));
        body.extend(Cx::get_temp(counter));
        body.push(ins(op::NEXT_VALUE, &[]));
        body.extend(self.dup_set_temp(ld, &mut t1));
        body.extend(self.dup_set_temp(ld, &mut t2));
        body.push(ins(op::PUSH_WITH, &[]));
        ld.scope_stack.push(node(Item::LocalReg { reg: t2, ty: ty_parts(&["Object"]) }));
        body.extend(self.src(ld, value)?);
        let sp = self.multiname_l(false, opened)?;
        let mut tb = Cx::get_temp(list);
        tb.extend(Cx::get_temp(counter));
        tb.extend(Cx::get_temp(t1));
        tb.push(ins(op::SET_PROPERTY, &[sp as i64]));
        body.push(ins(op::IF_FALSE, &[code_len(&tb)]));
        body.extend(tb);
        body.push(ins(op::POP_SCOPE, &[]));
        ld.scope_stack.pop();
        body.extend(self.kill_temp(ld, &[t2, t1]));
        let body_len = code_len(&body);
        ret.push(ins(op::JUMP, &[body_len]));
        let back = ins(op::IF_TRUE, &[0]);
        let expr = vec![ins(op::HAS_NEXT2, &[coll, counter]), back.clone()];
        let expr_len = code_len(&expr);
        back.borrow_mut().ops[0] = -(expr_len + body_len);
        ret.extend(body);
        ret.extend(expr);
        ret.extend(self.kill_temp(ld, &[coll, counter]));
        ret.extend(Cx::get_temp(list));
        ret.extend(self.kill_temp(ld, &[list]));
        Ok(ret)
    }

    /// `generate(localData, TypeItem)`.
    pub fn gen_type(&mut self, ld: &mut Ld, item: &Node) -> R<Code> {
        let current = ld.full_class();
        let (size, s) = match &*item.borrow() {
            Item::Type { full, .. } => (full.len(), full.raw()),
            _ => (0, text(item)),
        };
        let mut global_slot = 0;
        if size == 1 && Some(&s) == current.as_ref() {
            if let Some(c) = self.find_class_by_name(&s) {
                if let Some(si) = ld.current_script {
                    for t in &self.abc.scripts[si].traits {
                        if let crate::abc::TraitData::Class { slot_id, class } = t.data {
                            if class == c {
                                global_slot = slot_id;
                                break;
                            }
                        }
                    }
                }
            }
        }
        if ld.is_static && Some(&s) == current.as_ref() {
            return Ok(vec![ins(op::GET_LOCAL0, &[])]);
        }
        if global_slot > 0 {
            return Ok(vec![ins(op::GET_GLOBAL_SCOPE, &[]), ins(op::GET_SLOT, &[global_slot as i64])]);
        }
        let t = self.resolve_type(Some(ld), item)?;
        Ok(vec![ins(op::GET_LEX, &[t as i64])])
    }

    /// `findClassByName`, skipping deleted classes.
    pub fn find_class_by_name(&self, name: &str) -> Option<usize> {
        let names = super::index::raw_names(&self.abc);
        for (c, ii) in self.abc.instances.iter().enumerate() {
            if self.deleted.classes.contains(&self.class_ids[c]) {
                continue;
            }
            if names.name_with_ns(ii.name, true).raw() == name {
                return Some(c);
            }
        }
        None
    }

    /// `typeName`.
    pub fn type_name(&mut self, ld: Option<&Ld>, t: &Node) -> R<usize> {
        if is_unbounded(t) || text(t) == "*" {
            return Ok(0);
        }
        self.resolve_type(ld, t)
    }

    /// `NameAVM2Item.generateCoerce`.
    pub fn generate_coerce(&mut self, ld: Option<&Ld>, t: &Node) -> R<I> {
        let t = resolved_of(t);
        Ok(match text(&t).as_str() {
            "int" => ins(op::CONVERT_I, &[]),
            "*" => ins(op::COERCE_A, &[]),
            "String" => ins(op::COERCE_S, &[]),
            "Boolean" => ins(op::CONVERT_B, &[]),
            "uint" => ins(op::CONVERT_U, &[]),
            "Number" => ins(op::CONVERT_D, &[]),
            "float" => ins(op::CONVERT_F, &[]),
            "float4" => ins(op::CONVERT_F4, &[]),
            _ => {
                let i = self.resolve_type(ld, &t)?;
                ins(op::COERCE, &[i as i64])
            }
        })
    }

    /// `NameAVM2Item.generateConvert`.
    pub fn generate_convert(&self, t: &Node) -> Option<I> {
        let t = resolved_of(t);
        Some(match text(&t).as_str() {
            "int" => ins(op::CONVERT_I, &[]),
            "String" => ins(op::CONVERT_S, &[]),
            "Boolean" => ins(op::CONVERT_B, &[]),
            "uint" => ins(op::CONVERT_U, &[]),
            "Number" => ins(op::CONVERT_D, &[]),
            "float" => ins(op::CONVERT_F, &[]),
            "float4" => ins(op::CONVERT_F4, &[]),
            _ => return None,
        })
    }
}

pub fn int_ins(abc: &mut Abc, v: i32) -> I {
    if (-128..=127).contains(&v) {
        ins(op::PUSH_BYTE, &[v as i64])
    } else if (-32768..=32767).contains(&v) {
        ins(op::PUSH_SHORT, &[v as i64])
    } else {
        ins(op::PUSH_INT, &[pool::int_id(abc, v) as i64])
    }
}

fn local_reg(n: &Node) -> Option<i64> {
    match &*n.borrow() {
        Item::LocalReg { reg, .. } => Some(*reg),
        _ => None,
    }
}

/// What an unresolved name resolved to, else the item itself.
pub fn resolved_of(n: &Node) -> Node {
    match &*n.borrow() {
        Item::Unresolved(u) => u.resolved.clone().unwrap_or_else(|| n.clone()),
        _ => n.clone(),
    }
}
