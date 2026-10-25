//! The parse tree, mutable because resolution and code generation update it in place.

use std::cell::RefCell;
use std::rc::Rc;

use crate::abc::decomp::names::Dotted;
use crate::abc::ns_kind;

pub type Node = Rc<RefCell<Item>>;
pub type Nodes = Vec<Node>;

pub fn node(i: Item) -> Node {
    Rc::new(RefCell::new(i))
}

pub const KIND_NAMESPACE_CUSTOM: i32 = -2;

/// `NamespaceItem`.
#[derive(Debug, Clone)]
pub struct NsItem {
    pub name: Option<Dotted>,
    pub kind: i32,
    pub index: i64,
    pub line: usize,
}

pub type Ns = Rc<RefCell<NsItem>>;
pub type NsList = Rc<RefCell<Vec<Ns>>>;
pub type AllNs = Rc<RefCell<Vec<NsList>>>;
pub type Imports = Rc<RefCell<Vec<Dotted>>>;

pub fn ns_item(name: Dotted, kind: i32, line: usize) -> Ns {
    Rc::new(RefCell::new(NsItem { name: Some(name), kind, index: -1, line }))
}

pub fn ns_item_str(name: &str, kind: i32, line: usize) -> Ns {
    ns_item(Dotted::parse_with_suffix(name), kind, line)
}

pub fn ns_index_item(index: i64) -> Ns {
    Rc::new(RefCell::new(NsItem { name: None, kind: 0, index, line: 0 }))
}

pub fn ns_eq(a: &Ns, b: &Ns) -> bool {
    let (a, b) = (a.borrow(), b.borrow());
    a.name == b.name && a.kind == b.kind
}

pub fn ns_list_contains(l: &NsList, n: &Ns) -> bool {
    l.borrow().iter().any(|x| ns_eq(x, n))
}

pub fn ns_list(v: Vec<Ns>) -> NsList {
    Rc::new(RefCell::new(v))
}

pub fn ns_list_copy(l: &NsList) -> NsList {
    ns_list(l.borrow().clone())
}

impl NsItem {
    pub fn kind_u8(&self) -> u8 {
        self.kind as u8
    }

    pub fn raw_name(&self) -> String {
        self.name.as_ref().map_or(String::new(), |n| n.raw())
    }
}

pub type Metadata = Vec<(String, Vec<(String, String)>)>;

/// Loops and switches, numbered by the parser; switches are negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    LShift,
    RShift,
    URShift,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    StrictEq,
    Neq,
    StrictNeq,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    InstanceOf,
    Is,
    As,
    NullCoalesce,
}

impl Op {
    /// `CompoundableBinaryOp`, apart from `&&` and `||`.
    pub fn compoundable(self) -> bool {
        use Op::*;
        matches!(self, Add | Subtract | Multiply | Divide | Modulo | LShift | RShift | URShift | BitAnd | BitOr | BitXor)
    }

    /// `IfCondition`: (jump if true, jump if false).
    pub fn if_codes(self) -> Option<(u8, u8)> {
        use super::ins::op::*;
        Some(match self {
            Op::Eq => (IF_EQ, IF_NE),
            Op::Neq => (IF_NE, IF_EQ),
            Op::StrictEq => (IF_STRICT_EQ, IF_STRICT_NE),
            Op::StrictNeq => (IF_STRICT_NE, IF_STRICT_EQ),
            Op::Lt => (IF_LT, IF_NLT),
            Op::Le => (IF_LE, IF_NLE),
            Op::Gt => (IF_GT, IF_NGT),
            Op::Ge => (IF_GE, IF_NGE),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Un {
    Not,
    Neg,
    BitNot,
    TypeOf,
    UnPlus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Method,
    Getter,
    Setter,
}

#[derive(Debug, Clone)]
pub struct Unresolved {
    pub name: Dotted,
    pub opened: NsList,
    pub line: usize,
    pub ty: Option<Node>,
    pub resolved: Option<Node>,
    pub resolved_root: Option<Node>,
    pub must_be_type: bool,
    pub imported: Imports,
    pub scope_stack: Nodes,
    pub subtypes: Nodes,
    pub this_type: Option<Node>,
    pub assigned: Option<Node>,
}

#[derive(Debug, Clone)]
pub struct Name {
    pub attribute: bool,
    pub var: String,
    pub definition: bool,
    pub ns_kind: i32,
    pub opened: NsList,
    pub line: usize,
    pub ty: Option<Node>,
    pub ns: Option<Node>,
    pub reg: i64,
    pub unresolved: bool,
    pub slot: i64,
    pub slot_scope: i64,
    pub redirect: Option<Node>,
    pub suffix: String,
    pub is_const: bool,
    pub assigned: Option<Node>,
}

/// The method bodies a name is looked up in, innermost last: their activation traits.
pub type CallStack = Rc<RefCell<Vec<Body>>>;
pub type Body = Rc<RefCell<Vec<crate::abc::Trait>>>;

#[derive(Debug, Clone)]
pub struct Property {
    pub attribute: bool,
    pub name: String,
    pub object: Option<Node>,
    pub suffix: String,
    pub opened: NsList,
    pub call_stack: CallStack,
    pub scope_stack: Nodes,
    pub nullish: bool,
    pub ns_keyword: Option<String>,
    pub line: usize,
    pub this_type: Option<Node>,
    pub assigned: Option<Node>,
}

#[derive(Debug, Clone)]
pub struct Index {
    pub attr: bool,
    pub object: Node,
    pub index: Node,
    pub opened: NsList,
    pub assigned: Option<Node>,
}

#[derive(Debug, Clone)]
pub struct Namespaced {
    pub ns: Node,
    pub name: Option<String>,
    pub name_item: Option<Node>,
    pub obj: Option<Node>,
    pub attr: bool,
    pub opened: NsList,
    pub assigned: Option<Node>,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub name: Node,
    pub args: Nodes,
    pub line: usize,
    pub opened: NsList,
    pub this_type: Option<Node>,
}

#[derive(Debug, Clone)]
pub struct Func {
    pub metadata: Metadata,
    pub pkg: Option<Ns>,
    pub is_interface: bool,
    pub is_native: bool,
    pub needs_activation: bool,
    pub has_rest: bool,
    pub line: usize,
    pub name: String,
    pub param_types: Nodes,
    pub param_names: Vec<String>,
    pub param_values: Nodes,
    pub body: Option<Nodes>,
    pub subvariables: Nodes,
    pub ret_type: Node,
}

#[derive(Debug, Clone)]
pub struct Method {
    pub f: Func,
    pub kind: MethodKind,
    pub is_static: bool,
    pub is_final: bool,
    pub is_override: bool,
    pub is_private: bool,
    pub outside_package: bool,
    pub custom_ns: Option<String>,
    pub all_opened: AllNs,
}

#[derive(Debug, Clone)]
pub struct Class {
    pub metadata: Metadata,
    pub imported: Imports,
    pub pkg: Ns,
    pub opened: NsList,
    pub is_final: bool,
    pub is_dynamic: bool,
    pub base_name: String,
    pub extends: Option<Node>,
    pub implements: Nodes,
    pub cinit_activation: bool,
    pub cinit_variables: Nodes,
    pub iinit: Option<Node>,
    pub iinit_variables: Nodes,
    pub traits: Nodes,
    pub iinit_activation: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub struct Interface {
    pub metadata: Metadata,
    pub imported: Imports,
    pub pkg: Ns,
    pub opened: NsList,
    pub is_final: bool,
    pub base_name: String,
    pub super_interfaces: Nodes,
    pub methods: Nodes,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
pub struct Slot {
    pub metadata: Metadata,
    pub pkg: Ns,
    pub custom_ns: Option<String>,
    pub is_static: bool,
    pub var: String,
    pub ty: Node,
    pub value: Option<Node>,
    pub line: usize,
    pub is_const: bool,
    /// A `namespace` declaration.
    pub ns: bool,
    pub generated_ns: bool,
}

#[derive(Debug, Clone)]
pub struct Try {
    pub try_cmds: Nodes,
    pub catch_cmds: Vec<Nodes>,
    pub finally_cmds: Option<Nodes>,
    pub catch_variables: Vec<Nodes>,
    pub catch_exceptions: Nodes,
}

#[derive(Debug, Clone)]
pub enum Item {
    Str(String),
    Int(i32),
    Double(f64),
    Decimal([u8; 16]),
    Float(f32),
    Float4([f32; 4]),
    Bool(bool),
    True,
    False,
    Null,
    Undefined,
    Nan,
    Type { full: Dotted, ns: Option<String> },
    Unbounded,
    ApplyType { obj: Node, params: Nodes },
    Unresolved(Unresolved),
    Name(Name),
    Property(Property),
    Index(Index),
    Namespaced(Namespaced),
    ScriptProperty { ty: Node, assigned: Option<Node> },
    Call(Call),
    Construct(Call),
    ConstructSuper { obj: Node, args: Nodes },
    Bin { op: Op, l: Node, r: Node },
    And { l: Node, r: Node },
    Or { l: Node, r: Node },
    Un { op: Un, v: Node },
    Coerce { v: Node, ty: Node },
    PreInc { v: Node, ty: Node },
    PreDec { v: Node, ty: Node },
    PostInc { v: Node, ty: Node },
    PostDec { v: Node, ty: Node },
    Delete { obj: Node, line: usize },
    Ternar { cond: Node, t: Node, f: Node },
    Comma(Nodes),
    Paren(Node),
    NewObject(Vec<(Node, Node)>),
    NewArray(Nodes),
    InitVector { subtype: Node, args: Nodes, opened: NsList },
    RegExp { pattern: String, modifier: String },
    Xml(Node),
    EscXAttr(Node),
    EscXElem(Node),
    XmlFilter { obj: Node, value: Node, opened: NsList },
    Descendants { obj: Node, name: Option<String>, opened: NsList },
    Function(Func),
    Method(Method),
    Class(Class),
    Interface(Interface),
    Slot(Slot),
    Block(Nodes),
    If { cond: Node, on_true: Nodes, on_false: Option<Nodes> },
    While { id: LoopId, expr: Nodes, body: Nodes },
    DoWhile { id: LoopId, body: Nodes, expr: Nodes },
    For { id: LoopId, first: Nodes, expr: Node, fin: Nodes, body: Nodes },
    ForIn { id: LoopId, each: bool, expr: Node, body: Nodes },
    Switch { id: LoopId, expr: Node, values: Nodes, cmds: Vec<Nodes>, mapping: Vec<usize> },
    Default,
    Break(i64),
    Continue(i64),
    ReturnValue(Node),
    ReturnVoid,
    Throw(Node),
    Try(Try),
    With { scope: Node, items: Nodes, subvariables: Nodes },
    DefaultXmlNs(Node),
    Empty,
    HasNext { index: Node, obj: Node },
    NewActivation,
    NextName { index: Node, obj: Node },
    NextValue { index: Node, obj: Node },
    Dup(Node),
    Push(Node),
    Pop,
    Swap,
    LocalReg { reg: i64, ty: Node },
    Exception,
    WithObject(Node),
    FindProperty(Node),
    ForInAssigned { collection: i64, counter: i64, each: bool },
}

pub fn ty(full: Dotted) -> Node {
    node(Item::Type { full, ns: None })
}

pub fn ty_str(s: &str) -> Node {
    ty(Dotted::parse_with_suffix(s))
}

pub fn unbounded() -> Node {
    node(Item::Unbounded)
}

pub fn ty_parts(parts: &[&str]) -> Node {
    ty(Dotted::new(parts))
}

pub const VECTOR_FQN: &[&str] = &["__AS3__", "vec", "Vector"];

pub fn is_unbounded(n: &Node) -> bool {
    matches!(&*n.borrow(), Item::Unbounded)
}

/// `TypeItem.equals`.
pub fn ty_eq(a: &Node, b: &Node) -> bool {
    match (&*a.borrow(), &*b.borrow()) {
        (Item::Type { full: f1, ns: n1 }, Item::Type { full: f2, ns: n2 }) => f1 == f2 && n1 == n2,
        (Item::Unbounded, Item::Unbounded) => true,
        _ => Rc::ptr_eq(a, b),
    }
}

pub fn is_type_named(n: &Node, parts: &[&str]) -> bool {
    matches!(&*n.borrow(), Item::Type { full, ns: None } if *full == Dotted::new(parts))
}

/// An item's text as the compiler renders it.
pub fn text(n: &Node) -> String {
    match &*n.borrow() {
        Item::Type { full, .. } => full.raw(),
        Item::Unbounded => "*".into(),
        Item::Unresolved(u) => match &u.resolved {
            Some(r) => text(r),
            None => u.name.raw(),
        },
        Item::Name(nm) => nm.var.clone(),
        Item::Property(p) => format!("{}.{}", p.object.as_ref().map_or("null".into(), text), p.name),
        Item::ScriptProperty { ty, .. } => text(ty),
        Item::Str(s) => s.clone(),
        Item::Null => "null".into(),
        other => format!("{other:?}").split(['(', ' ', '{']).next().unwrap_or("").to_string(),
    }
}

pub fn is_assignable(n: &Node) -> bool {
    matches!(&*n.borrow(), Item::Unresolved(_) | Item::Name(_) | Item::Property(_) | Item::Index(_) | Item::Namespaced(_) | Item::ScriptProperty { .. })
}

pub fn assigned(n: &Node) -> Option<Node> {
    match &*n.borrow() {
        Item::Unresolved(x) => x.assigned.clone(),
        Item::Name(x) => x.assigned.clone(),
        Item::Property(x) => x.assigned.clone(),
        Item::Index(x) => x.assigned.clone(),
        Item::Namespaced(x) => x.assigned.clone(),
        Item::ScriptProperty { assigned, .. } => assigned.clone(),
        _ => None,
    }
}

pub fn set_assigned(n: &Node, v: Option<Node>) {
    match &mut *n.borrow_mut() {
        Item::Unresolved(x) => x.assigned = v,
        Item::Name(x) => x.assigned = v,
        Item::Property(x) => x.assigned = v,
        Item::Index(x) => x.assigned = v,
        Item::Namespaced(x) => x.assigned = v,
        Item::ScriptProperty { assigned, .. } => *assigned = v,
        _ => {}
    }
}

/// `AssignableAVM2Item.copy`.
pub fn copy_assignable(n: &Node) -> Node {
    let it = n.borrow().clone();
    match it {
        Item::Name(nm) => {
            let mut c = new_name(nm.ty.clone(), nm.line, nm.attribute, &nm.var, &nm.suffix, nm.assigned.clone(), nm.definition, nm.opened.clone(), nm.is_const);
            if let Item::Name(cn) = &mut c {
                cn.ns = nm.ns;
                cn.reg = nm.reg;
                cn.unresolved = nm.unresolved;
                cn.ns_kind = nm.ns_kind;
            }
            node(c)
        }
        Item::Property(mut p) => {
            p.scope_stack = Vec::new();
            p.assigned = None;
            node(Item::Property(p))
        }
        Item::Unresolved(mut u) => {
            u.scope_stack = Vec::new();
            u.resolved_root = None;
            node(Item::Unresolved(u))
        }
        Item::ScriptProperty { ty, .. } => node(Item::ScriptProperty { ty, assigned: None }),
        other => node(other),
    }
}

/// `new NameAVM2Item(...)`, which coerces `&&`/`||` operands of a typed store value.
#[allow(clippy::too_many_arguments)]
pub fn new_name(ty: Option<Node>, line: usize, attribute: bool, var: &str, suffix: &str, store: Option<Node>, definition: bool, opened: NsList, is_const: bool) -> Item {
    if let (Some(s), Some(t)) = (&store, &ty) {
        handle_and_or_coerce(s, t);
    }
    Item::Name(Name {
        attribute,
        var: var.to_string(),
        definition,
        ns_kind: -1,
        opened,
        line,
        ty,
        ns: None,
        reg: -1,
        unresolved: false,
        slot: -1,
        slot_scope: 0,
        redirect: None,
        suffix: suffix.to_string(),
        is_const,
        assigned: store,
    })
}

pub fn name_node(ty: Option<Node>, line: usize, var: &str, store: Option<Node>, definition: bool, opened: NsList, is_const: bool) -> Node {
    node(new_name(ty, line, false, var, "", store, definition, opened, is_const))
}

/// `AVM2SourceGenerator.handleAndOrCoerce`: coerces each operand of `&&`/`||` in place.
pub fn handle_and_or_coerce(item: &Node, ty: &Node) {
    let sides = match &*item.borrow() {
        Item::And { l, r } | Item::Or { l, r } => Some((l.clone(), r.clone())),
        _ => None,
    };
    let Some((l, r)) = sides else { return };
    let fix = |side: &Node| -> Node {
        if matches!(&*side.borrow(), Item::And { .. } | Item::Or { .. }) {
            handle_and_or_coerce(side, ty);
            side.clone()
        } else {
            node(Item::Coerce { v: side.clone(), ty: ty.clone() })
        }
    };
    let (nl, nr) = (fix(&l), fix(&r));
    if let Item::And { l, r } | Item::Or { l, r } = &mut *item.borrow_mut() {
        *l = nl;
        *r = nr;
    }
}

pub fn property(object: Option<Node>, attribute: bool, name: &str, suffix: &str, opened: NsList, call_stack: CallStack, nullish: bool, ns_keyword: Option<String>, line: usize, this_type: Option<Node>) -> Node {
    node(Item::Property(Property {
        attribute,
        name: name.to_string(),
        object,
        suffix: suffix.to_string(),
        opened,
        call_stack,
        scope_stack: Vec::new(),
        nullish,
        ns_keyword,
        line,
        this_type,
        assigned: None,
    }))
}

pub fn empty_stack() -> CallStack {
    Rc::new(RefCell::new(Vec::new()))
}

#[allow(clippy::too_many_arguments)]
pub fn unresolved(subtypes: Nodes, imported: Imports, must_be_type: bool, ty: Option<Node>, line: usize, name: Dotted, assigned: Option<Node>, opened: NsList, this_type: Option<Node>) -> Node {
    node(Item::Unresolved(Unresolved {
        name,
        opened,
        line,
        ty,
        resolved: None,
        resolved_root: None,
        must_be_type,
        imported,
        scope_stack: Vec::new(),
        subtypes,
        this_type,
        assigned,
    }))
}

pub fn is_ns_kind_package(k: i32) -> bool {
    k == ns_kind::PACKAGE as i32
}
