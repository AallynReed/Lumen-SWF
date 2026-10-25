//! The decompiler's expression and statement tree.
//!
//! Items are shared and mutated in place, so they live behind `Rc<RefCell<..>>` and
//! identity (`Rc::ptr_eq`) stands in for Java's `==`.

use std::cell::RefCell;
use std::rc::Rc;

use super::names::Dotted;
use crate::abc::Exception;

pub type It = Rc<RefCell<Item>>;
pub type List = Rc<RefCell<Vec<It>>>;

pub mod prec {
    pub const PRIMARY: u8 = 0;
    pub const POSTFIX: u8 = 1;
    pub const UNARY: u8 = 2;
    pub const MULTIPLICATIVE: u8 = 3;
    pub const ADDITIVE: u8 = 4;
    pub const BITWISESHIFT: u8 = 5;
    pub const RELATIONAL: u8 = 6;
    pub const EQUALITY: u8 = 7;
    pub const BITWISEAND: u8 = 8;
    pub const BITWISEXOR: u8 = 9;
    pub const BITWISEOR: u8 = 10;
    pub const LOGICALAND: u8 = 11;
    pub const LOGICALOR: u8 = 12;
    pub const NULLCOALESCE: u8 = 13;
    pub const CONDITIONAL: u8 = 14;
    pub const ASSIGNMENT: u8 = 15;
    pub const COMMA: u8 = 16;
    pub const NOPRECEDENCE: u8 = 17;
}

/// Index of a graph part in the graph's arena.
pub type PartId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bin {
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LShift,
    RShift,
    URShift,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Neq,
    StrictEq,
    StrictNeq,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    InstanceOf,
    IsType,
    AsType,
    NullCoalesce,
    // AS1/2 operators.
    AAnd,
    AOr,
    StrAdd,
    StrEq,
    StrNe,
    StrLt,
    StrLe,
    StrGt,
    StrGe,
}

impl Bin {
    pub fn op(self) -> &'static str {
        use Bin::*;
        match self {
            And => "&&",
            Or => "||",
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Mod => "%",
            LShift => "<<",
            RShift => ">>",
            URShift => ">>>",
            BitAnd => "&",
            BitOr => "|",
            BitXor => "^",
            Eq => "==",
            Neq => "!=",
            StrictEq => "===",
            StrictNeq => "!==",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            In => "in",
            InstanceOf => "instanceof",
            IsType => "is",
            AsType => "as",
            NullCoalesce => "??",
            AAnd => "and",
            AOr => "or",
            StrAdd => "add",
            StrEq => "eq",
            StrNe => "ne",
            StrLt => "lt",
            StrLe => "le",
            StrGt => "gt",
            StrGe => "ge",
        }
    }

    pub fn prec(self) -> u8 {
        use Bin::*;
        match self {
            And => prec::LOGICALAND,
            Or => prec::LOGICALOR,
            Add | Sub => prec::ADDITIVE,
            Mul | Div | Mod => prec::MULTIPLICATIVE,
            LShift | RShift | URShift => prec::BITWISESHIFT,
            BitAnd => prec::BITWISEAND,
            BitOr => prec::BITWISEOR,
            BitXor => prec::BITWISEXOR,
            Eq | Neq | StrictEq | StrictNeq => prec::EQUALITY,
            Lt | Le | Gt | Ge | In | InstanceOf | IsType | AsType => prec::RELATIONAL,
            NullCoalesce => prec::NULLCOALESCE,
            AAnd => prec::LOGICALAND,
            AOr => prec::LOGICALOR,
            StrAdd => prec::ADDITIVE,
            StrEq | StrNe => prec::EQUALITY,
            StrLt | StrLe | StrGt | StrGe => prec::RELATIONAL,
        }
    }

    /// `coerceLeft`/`coerceRight` of the Java class (both sides are always equal).
    pub fn coerce(self) -> &'static str {
        use Bin::*;
        match self {
            And | Or => "Boolean",
            Sub | Mul | Div | Mod | LShift | RShift | URShift | BitAnd | BitOr | BitXor => "Number",
            _ => "",
        }
    }

    pub fn is_bitwise(self) -> bool {
        matches!(self, Bin::BitAnd | Bin::BitOr | Bin::BitXor)
    }

    /// Classes implementing `CompoundableBinaryOp`.
    pub fn compoundable(self) -> bool {
        use Bin::*;
        matches!(self, And | Or | Add | Sub | Mul | Div | Mod | LShift | RShift | URShift | BitAnd | BitOr | BitXor)
    }

    /// Classes implementing `LogicalOpItem`.
    pub fn logical(self) -> bool {
        use Bin::*;
        matches!(self, Eq | Neq | StrictEq | StrictNeq | Lt | Le | Gt | Ge)
    }

    /// Classes implementing `IfCondition`.
    pub fn if_condition(self) -> bool {
        self.logical()
    }

    pub fn inverted(self) -> Option<Bin> {
        use Bin::*;
        Some(match self {
            Eq => Neq,
            Neq => Eq,
            StrictEq => StrictNeq,
            StrictNeq => StrictEq,
            Lt => Ge,
            Ge => Lt,
            Gt => Le,
            Le => Gt,
            StrEq => StrNe,
            StrNe => StrEq,
            StrLt => StrGe,
            StrGe => StrLt,
            StrGt => StrLe,
            StrLe => StrGt,
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
    PreIncrement,
    PreDecrement,
}

impl Un {
    pub fn op(self) -> &'static str {
        match self {
            Un::Not => "!",
            Un::Neg => "-",
            Un::BitNot => "~",
            Un::TypeOf => "typeof ",
            Un::UnPlus => "+",
            Un::PreIncrement => "++",
            Un::PreDecrement => "--",
        }
    }

    pub fn coerce(self) -> &'static str {
        match self {
            Un::Not => "Boolean",
            Un::Neg | Un::UnPlus => "Number",
            Un::BitNot => "int",
            _ => "",
        }
    }
}

/// Fields shared by the `SetTypeAVM2Item` classes.
#[derive(Debug, Clone, Default)]
pub struct SetInfo {
    pub declaration: Option<It>,
    pub compound_value: Option<It>,
    pub compound_op: Option<String>,
}

#[derive(Debug, Clone)]
pub enum K {
    // Generic items.
    Bin { op: Bin, l: It, r: It },
    Un { op: Un, ty: Option<It> },
    Ternar { expr: It, on_true: It, on_false: It },
    If { expr: It, on_true: List, on_false: List, decision_part: Option<PartId>, on_true_part: Option<PartId>, on_false_part: Option<PartId> },
    While { loop_id: i64, expr: List, cmds: List, label_used: bool, universal: bool },
    DoWhile { loop_id: i64, cmds: List, expr: List, label_used: bool },
    For { loop_id: i64, first: List, expr: It, fin: List, cmds: List, label_used: bool },
    Switch { loop_id: i64, obj: It, case_values: List, case_cmds: Vec<List>, mapping: Vec<i32>, add_default_pos: i32, add_default_values: List, label_used: bool },
    Break { loop_id: i64, label_required: bool, is_script_end: bool },
    Continue { loop_id: i64, label_required: bool },
    Label { name: String },
    Goto { name: String, target: Option<List> },
    Push { as_comment: bool },
    Pop,
    Duplicate { temp: i32 },
    DuplicateSource { temp: i32, declaration: bool },
    SetTemporary { temp: i32, declaration: bool, ref_count: i32, suffix: String },
    Temporary { temp: i32 },
    Swap,
    Comma { cmds: List },
    Parenthesis,
    True,
    False,
    IntegerValue(i32),
    Comment(Vec<Option<String>>),
    ScriptEnd,
    Empty,
    Default,
    Any,
    Block { cmds: List },
    UnboundedType,
    Type { full: Dotted, ns: Option<String> },
    TypeFunction(String),

    // AVM2 items.
    AlchemyLoad { ty: String, size: i32, ofs: It },
    AlchemySignExtend { size: i32 },
    AlchemyStore { ty: String, size: i32, ofs: It },
    ApplyType { obj: It, params: List },
    Boolean(bool),
    Call { receiver: It, function: It, args: List },
    CallMethod { receiver: It, name: String, args: List },
    CallStatic { receiver: It, name: String, args: List },
    CallProperty { receiver: It, prop: It, args: List, is_void: bool, ty: It, is_static: bool },
    CallSuper { receiver: It, mn: It, args: List, is_void: bool },
    Class { mn: Option<usize>, name: Option<Dotted> },
    Construct { obj: It, args: List },
    ConstructProp { obj: It, prop: It, args: List, ty: It, is_static: bool },
    ConstructSuper { obj: It, args: List },
    Coerce { ty: It },
    Convert { ty: It },
    DNan,
    DecLocal { reg: i32, ty: It },
    IncLocal { reg: i32, ty: It },
    DecimalValue([u8; 16]),
    DoubleValue(f64),
    FloatValue(f32),
    Float4Value([f32; 4]),
    IntegerValueA { v: i32, formatted: String },
    Nan,
    Declaration { assignment: It, ty: Option<It>, type_is_null: bool, show_value: bool },
    Decrement,
    Increment,
    DefaultXmlNs { ns: It },
    DeleteProperty { obj: It, prop: It, is_static: bool },
    EscapeXAttr,
    EscapeXElem,
    ExceptionItem(Exception),
    Filter { expr: It, coll: It },
    FilteredCheck { obj: It },
    FindDef { mn: usize },
    FindProperty { prop: It },
    ForEachIn { loop_id: i64, expr: It, cmds: List, label_used: bool },
    ForIn { loop_id: i64, expr: It, cmds: List, label_used: bool },
    FullMultiname { mn: usize, name: Option<It>, ns: Option<It>, property: bool, resolved: Option<String> },
    GetDescendants { obj: It, mn: It },
    GetLex { mn: usize, ty: It, call_ty: It, is_static: bool, full: Dotted },
    GetProperty { obj: It, prop: It, ty: It, call_ty: It, is_static: bool, null_cond: bool },
    GetSlot { slot_name: Option<usize>, scope: It, slot_obj: It, idx: i32, slot_ty: It },
    GetSuper { obj: It, prop: It, ty: It, call_ty: It, is_static: bool },
    Global,
    HasNext { index: It, obj: It },
    /// `model.InAVM2Item`: the `x in obj` head of for-in loops.
    InFor { object: It, collection: It },
    InitProperty { obj: It, prop: It, set: SetInfo, ty: It, call_ty: It, is_static: bool },
    InitVector { subtype: It, args: List },
    LocalReg { reg: i32, computed: Option<It>, ty: It, is_ct: bool },
    NameSpace(usize),
    NameValuePair { name: It },
    NewActivation,
    NewArray { values: List },
    NewFunction { name: String, path: String, is_static: bool, script: i32, class: i32, method: usize, scope: Vec<It> },
    NewObject { pairs: List },
    NextName { index: It, obj: It, local_reg: Option<It> },
    NextValue { index: It, obj: It, local_reg: Option<It> },
    Null,
    Undefined,
    Package { items: List, name: String },
    PostDecrement { obj: It, ty: It },
    PostIncrement { obj: It, ty: It },
    RegExp { pattern: String, modifier: String },
    ReturnValue,
    ReturnVoid,
    Throw,
    SetLocal { reg: i32, set: SetInfo, ty: It, hide_value: bool, caused_by_dup: bool, directly_caused_by_dup: bool },
    SetProperty { obj: It, prop: It, set: SetInfo, ty: It, call_ty: It, is_static: bool },
    SetSlot { slot_name: Option<usize>, scope: It, slot_obj: It, idx: i32, set: SetInfo, ty: It },
    SetSuper { obj: It, prop: It, set: SetInfo, ty: It, call_ty: It, is_static: bool },
    StoreNewActivation { reg: i32 },
    Str(String),
    This { class_name: Option<Dotted>, basic: bool, show_class: bool },
    TraitSlotConst { script: i32, class: i32, trait_index: i32, is_static: bool, initializer: i32, method: usize, value: Option<It> },
    Try { try_cmds: List, catch_ex: Vec<Exception>, catch_cmds: Vec<List>, finally: Option<List>, fin_catch_name: String },
    Unparsed(String),
    With { scope: It, items: List },
    WithEnd { scope: It },
    WithObject { scope: It },
    Xml { parts: List },

    // AS1/2 items (`flash.action.model`).
    ADirect { v: AVal, computed: Option<It> },
    AUnresolved(i32),
    AGetVar { name: It, print_obf: bool, computed: Option<It> },
    ASetVar { name: It, compound_value: Option<It>, compound_op: Option<String> },
    AGetMember { obj: It, member: It, print_obf: bool, getter: bool },
    ASetMember { obj: It, member: It, compound_value: Option<It>, compound_op: Option<String>, setter: bool },
    AGetProp { target: It, index: i32, use_fn: bool },
    ASetProp { target: It, index: i32 },
    ADefineLocal { name: It },
    ADelete { obj: Option<It>, prop: It },
    AStoreReg { reg: i32, name: Option<String>, define: bool, temporary: bool, compound_value: Option<It>, compound_op: Option<String> },
    ACallFunction { name: It, args: List, calculated: Option<It> },
    ACallMethod { obj: It, name: It, args: List },
    ANewMethod { obj: It, name: It, args: List },
    ANewObject { name: It, args: List },
    AInitArray { values: List },
    AInitObject { names: List, values: List },
    AInc { obj: It },
    ADec { obj: It },
    APostInc { obj: It },
    APostDec { obj: It },
    AEnumerate { obj: It },
    AEnumerated,
    AEnumAssign,
    AFunction(Box<AFunction>),
    AReturn,
    AEval,
    ATypeOf,
    ATellTarget { target: It, cmds: List },
    ASetTarget(String),
    ASetTarget2 { target: It },
    AWith { scope: It, items: List },
    ATry { try_cmds: List, names: List, types: Vec<Option<It>>, catch_cmds: Vec<List>, finally: List },
    AForIn { loop_id: i64, var: It, obj: It, cmds: List, label_used: bool },
    AIfFrameLoaded { frame: It, actions: List },
    AClass(Box<AClass>),
    AInterface { name: It, supers: List },
    ATempReg { reg: i32, used: std::rc::Rc<std::cell::Cell<bool>> },
    ATempRegMark { reg: It },
    AStrict(i32),
    AGotoFrame(i32),
    AExtends { sub: It, sup: It },
    AImplements { sub: It, supers: List },
    ACastOp { ctor: It, obj: It },
    /// Call-like statements and expressions printed as `name(args)`.
    AFn { name: &'static str, args: List, sep: &'static str, suffix: String, ret: bool, side: bool },
    AUnsupported(String),
}

/// `DirectValueActionItem.value`.
#[derive(Debug, Clone, PartialEq)]
pub enum AVal {
    Str(String),
    Float(f32),
    Double(f64),
    Int(i64),
    Null,
    Undefined,
    Register { num: i32, name: Option<String> },
}

#[derive(Debug, Clone)]
pub struct AFunction {
    pub name: String,
    pub params: Vec<String>,
    pub reg_names: Vec<(i32, String)>,
    pub actions: List,
    pub calculated_name: Option<It>,
    pub getter: bool,
    pub setter: bool,
    pub reg_start: i32,
    pub param_registers: Option<Vec<i32>>,
}

#[derive(Debug, Clone)]
pub struct AClass {
    pub name: It,
    pub extends: Option<It>,
    pub implements: List,
    pub traits: Vec<(It, Option<It>)>,
    pub statics: Vec<bool>,
    pub uninitialized: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub k: K,
    /// Instruction index of the source instruction.
    pub src: Option<usize>,
    pub line_start: Option<usize>,
    pub prec: u8,
    pub value: Option<It>,
    pub first_part: Option<PartId>,
    pub comment: String,
    pub pos: i32,
    /// Created by the AS1/2 translator (`ActionGraphTargetDialect`).
    pub as2: bool,
}

pub fn new_list(v: Vec<It>) -> List {
    Rc::new(RefCell::new(v))
}

pub fn empty_list() -> List {
    new_list(Vec::new())
}

pub fn mk(k: K, src: Option<usize>, line_start: Option<usize>, prec: u8, value: Option<It>) -> It {
    Rc::new(RefCell::new(Item { k, src, line_start, prec, value, first_part: None, comment: String::new(), pos: 0, as2: false }))
}

/// A source-less item.
pub fn bare(k: K, prec: u8) -> It {
    mk(k, None, None, prec, None)
}

pub fn same(a: &It, b: &It) -> bool {
    Rc::ptr_eq(a, b)
}

pub fn same_opt(a: &Option<It>, b: &Option<It>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// Copies a list's items into a fresh list (Java's `new ArrayList<>(list)`).
pub fn copy_list(l: &List) -> List {
    new_list(l.borrow().clone())
}

/// Shallow copy of an item (Java's `clone()`).
pub fn clone_item(it: &It) -> It {
    Rc::new(RefCell::new(it.borrow().clone()))
}

// Types.

pub fn ty(name: &str) -> It {
    bare(K::Type { full: Dotted::parse_with_suffix(name), ns: None }, prec::NOPRECEDENCE)
}

pub fn ty_dotted(full: Dotted) -> It {
    bare(K::Type { full, ns: None }, prec::NOPRECEDENCE)
}

pub fn unbounded() -> It {
    bare(K::UnboundedType, prec::NOPRECEDENCE)
}

pub fn unknown() -> It {
    ty("--UNKNOWN--")
}

pub fn is_unbounded(t: &It) -> bool {
    matches!(t.borrow().k, K::UnboundedType)
}

/// Whether `t` is `TypeItem(name)`.
pub fn is_type(t: &It, name: &str) -> bool {
    match &t.borrow().k {
        K::Type { full, ns: None } => full.len() == 1 && full.last() == name && full.0[0].suffix.is_empty(),
        _ => false,
    }
}

pub fn is_unknown(t: &It) -> bool {
    is_type(t, "--UNKNOWN--")
}

// Constructors for common items.

pub fn true_item(src: Option<usize>, ls: Option<usize>) -> It {
    mk(K::True, src, ls, prec::PRIMARY, None)
}

pub fn false_item(src: Option<usize>, ls: Option<usize>) -> It {
    mk(K::False, src, ls, prec::PRIMARY, None)
}

pub fn not_item(src: Option<usize>, ls: Option<usize>, v: It) -> It {
    mk(K::Un { op: Un::Not, ty: None }, src, ls, prec::UNARY, Some(v))
}

pub fn bin(op: Bin, src: Option<usize>, ls: Option<usize>, l: It, r: It) -> It {
    mk(K::Bin { op, l, r }, src, ls, op.prec(), None)
}

pub fn and_item(src: Option<usize>, ls: Option<usize>, l: It, r: It) -> It {
    bin(Bin::And, src, ls, l, r)
}

pub fn or_item(src: Option<usize>, ls: Option<usize>, l: It, r: It) -> It {
    bin(Bin::Or, src, ls, l, r)
}

pub fn push_item(v: It) -> It {
    let (src, ls, p) = {
        let b = v.borrow();
        (b.src, b.line_start, b.prec)
    };
    mk(K::Push { as_comment: false }, src, ls, p, Some(v))
}

pub fn break_item(src: Option<usize>, ls: Option<usize>, loop_id: i64) -> It {
    mk(K::Break { loop_id, label_required: false, is_script_end: false }, src, ls, prec::NOPRECEDENCE, None)
}

pub fn continue_item(src: Option<usize>, ls: Option<usize>, loop_id: i64) -> It {
    mk(K::Continue { loop_id, label_required: false }, src, ls, prec::NOPRECEDENCE, None)
}

pub fn if_item(src: Option<usize>, ls: Option<usize>, expr: It, on_true: List, on_false: List) -> It {
    mk(K::If { expr, on_true, on_false, decision_part: None, on_true_part: None, on_false_part: None }, src, ls, prec::NOPRECEDENCE, None)
}

pub fn ternar(src: Option<usize>, ls: Option<usize>, expr: It, on_true: It, on_false: It) -> It {
    mk(K::Ternar { expr, on_true, on_false }, src, ls, prec::CONDITIONAL, None)
}

pub fn script_end() -> It {
    bare(K::ScriptEnd, prec::NOPRECEDENCE)
}

pub fn integer_a(src: Option<usize>, ls: Option<usize>, v: i32) -> It {
    mk(K::IntegerValueA { v, formatted: super::super::names::ecma_number(v as f64) }, src, ls, prec::PRIMARY, None)
}

pub fn undefined(src: Option<usize>, ls: Option<usize>) -> It {
    mk(K::Undefined, src, ls, prec::PRIMARY, None)
}

pub fn null(src: Option<usize>, ls: Option<usize>) -> It {
    mk(K::Null, src, ls, prec::PRIMARY, None)
}

pub fn string_item(src: Option<usize>, ls: Option<usize>, s: String) -> It {
    mk(K::Str(s), src, ls, prec::PRIMARY, None)
}

impl Item {
    pub fn kind(&self) -> &K {
        &self.k
    }

    pub fn val(&self) -> It {
        self.value.clone().expect("item value")
    }
}

/// Kind tests used all over the graph code, Java's `instanceof`.
pub mod is {
    use super::*;

    pub fn not(it: &It) -> bool {
        matches!(it.borrow().k, K::Un { op: Un::Not, .. })
    }
    pub fn true_(it: &It) -> bool {
        matches!(it.borrow().k, K::True)
    }
    pub fn false_(it: &It) -> bool {
        matches!(it.borrow().k, K::False)
    }
    pub fn and(it: &It) -> bool {
        matches!(it.borrow().k, K::Bin { op: Bin::And, .. })
    }
    pub fn or(it: &It) -> bool {
        matches!(it.borrow().k, K::Bin { op: Bin::Or, .. })
    }
    pub fn break_(it: &It) -> bool {
        matches!(it.borrow().k, K::Break { .. })
    }
    pub fn continue_(it: &It) -> bool {
        matches!(it.borrow().k, K::Continue { .. })
    }
    pub fn if_(it: &It) -> bool {
        matches!(it.borrow().k, K::If { .. })
    }
    pub fn push(it: &It) -> bool {
        matches!(it.borrow().k, K::Push { .. })
    }
    pub fn pop(it: &It) -> bool {
        matches!(it.borrow().k, K::Pop)
    }
    pub fn script_end(it: &It) -> bool {
        matches!(it.borrow().k, K::ScriptEnd)
    }
    pub fn duplicate(it: &It) -> bool {
        matches!(it.borrow().k, K::Duplicate { .. })
    }
    pub fn duplicate_source(it: &It) -> bool {
        matches!(it.borrow().k, K::DuplicateSource { .. })
    }
    pub fn set_temporary(it: &It) -> bool {
        matches!(it.borrow().k, K::SetTemporary { .. })
    }
    pub fn ternar(it: &It) -> bool {
        matches!(it.borrow().k, K::Ternar { .. })
    }
    pub fn local_reg(it: &It) -> bool {
        matches!(it.borrow().k, K::LocalReg { .. })
    }
    pub fn set_local(it: &It) -> bool {
        matches!(it.borrow().k, K::SetLocal { .. })
    }
    pub fn integer_a(it: &It) -> bool {
        matches!(it.borrow().k, K::IntegerValueA { .. })
    }
    pub fn find_property(it: &It) -> bool {
        matches!(it.borrow().k, K::FindProperty { .. })
    }
    pub fn full_multiname(it: &It) -> bool {
        matches!(it.borrow().k, K::FullMultiname { .. })
    }
    pub fn new_function(it: &It) -> bool {
        matches!(it.borrow().k, K::NewFunction { .. })
    }
    pub fn loop_(it: &It) -> bool {
        matches!(it.borrow().k, K::While { .. } | K::DoWhile { .. } | K::For { .. } | K::Switch { .. } | K::ForIn { .. } | K::ForEachIn { .. })
    }
    pub fn exit(it: &It) -> bool {
        matches!(it.borrow().k, K::ScriptEnd | K::ReturnValue | K::ReturnVoid | K::Throw | K::AReturn)
    }
    /// Classes implementing `Block`.
    pub fn block(it: &It) -> bool {
        matches!(
            it.borrow().k,
            K::If { .. } | K::While { .. } | K::DoWhile { .. } | K::For { .. } | K::Switch { .. } | K::Goto { .. } | K::ForIn { .. } | K::ForEachIn { .. } | K::Try { .. }
                | K::ATellTarget { .. } | K::AForIn { .. } | K::AIfFrameLoaded { .. } | K::ATry { .. } | K::AClass(_)
        )
    }
    /// Classes implementing `SimpleValue`, with `isSimpleValue()` true.
    pub fn simple_value(it: &It) -> bool {
        let b = it.borrow();
        match &b.k {
            K::True | K::False | K::DNan | K::Nan | K::Null | K::Undefined | K::Str(_) => true,
            K::ADirect { v, .. } => !matches!(v, AVal::Register { .. }),
            K::DecimalValue(_) | K::DoubleValue(_) | K::FloatValue(_) | K::Float4Value(_) | K::IntegerValueA { .. } => true,
            K::Duplicate { .. } | K::DuplicateSource { .. } | K::SetTemporary { .. } | K::Temporary { .. } => b.value.as_ref().is_some_and(simple_value),
            _ => false,
        }
    }
}

/// `getSubs()` of `Block` items.
pub fn subs(it: &It) -> Vec<List> {
    let b = it.borrow();
    match &b.k {
        K::If { on_true, on_false, .. } => vec![on_true.clone(), on_false.clone()],
        K::While { expr, cmds, .. } | K::DoWhile { expr, cmds, .. } => vec![expr.clone(), cmds.clone()],
        K::For { first, cmds, fin, .. } => vec![first.clone(), cmds.clone(), fin.clone()],
        K::Switch { case_cmds, .. } => case_cmds.clone(),
        K::Goto { target, .. } => target.iter().cloned().collect(),
        K::ForIn { cmds, .. } | K::ForEachIn { cmds, .. } => vec![cmds.clone()],
        K::Try { try_cmds, catch_cmds, finally, .. } => {
            let mut v = vec![try_cmds.clone()];
            v.extend(catch_cmds.iter().cloned());
            if let Some(f) = finally {
                v.push(f.clone());
            }
            v
        }
        K::ATellTarget { cmds, .. } | K::AForIn { cmds, .. } => vec![cmds.clone()],
        K::AIfFrameLoaded { actions, .. } => vec![actions.clone()],
        K::ATry { try_cmds, catch_cmds, finally, .. } => {
            let mut v = vec![try_cmds.clone()];
            v.extend(catch_cmds.iter().cloned());
            v.push(finally.clone());
            v
        }
        _ => Vec::new(),
    }
}

/// Visits direct children like `visit(GraphTargetVisitorInterface)`. `no_block` gives `visitNoBlock`.
pub fn children(it: &It, no_block: bool) -> Vec<It> {
    let b = it.borrow();
    let mut v: Vec<It> = Vec::new();
    let all = |v: &mut Vec<It>, l: &List| v.extend(l.borrow().iter().cloned());
    let value = |v: &mut Vec<It>| {
        if let Some(x) = &b.value {
            v.push(x.clone());
        }
    };
    match &b.k {
        K::Bin { l, r, .. } => {
            v.push(l.clone());
            v.push(r.clone());
        }
        K::Ternar { expr, on_true, on_false } => {
            v.push(expr.clone());
            v.push(on_true.clone());
            v.push(on_false.clone());
        }
        K::If { expr, on_true, on_false, .. } => {
            v.push(expr.clone());
            if !no_block {
                all(&mut v, on_true);
                all(&mut v, on_false);
            }
        }
        K::While { expr, cmds, .. } => {
            all(&mut v, expr);
            if !no_block {
                all(&mut v, cmds);
            }
        }
        K::DoWhile { expr, cmds, .. } => {
            if !no_block {
                all(&mut v, cmds);
            }
            all(&mut v, expr);
        }
        K::For { first, expr, fin, cmds, .. } => {
            if no_block {
                v.push(expr.clone());
            } else {
                all(&mut v, first);
                v.push(expr.clone());
                all(&mut v, cmds);
                all(&mut v, fin);
            }
        }
        K::Switch { obj, case_values, case_cmds, .. } => {
            v.push(obj.clone());
            all(&mut v, case_values);
            if !no_block {
                for c in case_cmds {
                    all(&mut v, c);
                }
            }
        }
        K::Goto { target, .. } => {
            if !no_block {
                if let Some(t) = target {
                    all(&mut v, t);
                }
            }
        }
        K::Comma { cmds } | K::Block { cmds } => all(&mut v, cmds),
        K::Duplicate { .. } | K::Temporary { .. } => {}
        K::AlchemyLoad { ofs, .. } => v.push(ofs.clone()),
        K::AlchemyStore { ofs, .. } => {
            v.push(ofs.clone());
            value(&mut v);
        }
        K::ApplyType { obj, params } => {
            v.push(obj.clone());
            all(&mut v, params);
        }
        K::Call { receiver, function, args } => {
            v.push(receiver.clone());
            v.push(function.clone());
            all(&mut v, args);
        }
        K::CallMethod { receiver, args, .. } | K::CallStatic { receiver, args, .. } => {
            v.push(receiver.clone());
            all(&mut v, args);
        }
        K::CallProperty { receiver, prop, args, .. } => {
            v.push(receiver.clone());
            v.push(prop.clone());
            all(&mut v, args);
        }
        K::CallSuper { receiver, mn, args, .. } => {
            v.push(receiver.clone());
            v.push(mn.clone());
            all(&mut v, args);
        }
        K::Construct { obj, args } | K::ConstructSuper { obj, args } => {
            v.push(obj.clone());
            all(&mut v, args);
        }
        K::ConstructProp { obj, prop, args, .. } => {
            v.push(obj.clone());
            v.push(prop.clone());
            all(&mut v, args);
        }
        K::Coerce { ty } | K::Convert { ty } => {
            v.push(ty.clone());
            value(&mut v);
        }
        K::Declaration { assignment, ty, .. } => {
            if let Some(t) = ty {
                v.push(t.clone());
            }
            v.push(assignment.clone());
        }
        K::DefaultXmlNs { ns } => v.push(ns.clone()),
        K::DeleteProperty { obj, prop, .. } => {
            v.push(obj.clone());
            v.push(prop.clone());
        }
        K::Filter { expr, coll } => {
            v.push(expr.clone());
            v.push(coll.clone());
        }
        K::FilteredCheck { obj } => v.push(obj.clone()),
        K::FindProperty { prop } => v.push(prop.clone()),
        K::ForEachIn { expr, cmds, .. } | K::ForIn { expr, cmds, .. } => {
            v.push(expr.clone());
            if !no_block {
                all(&mut v, cmds);
            }
        }
        K::FullMultiname { name, ns, .. } => {
            if let Some(n) = name {
                v.push(n.clone());
            }
            if let Some(n) = ns {
                v.push(n.clone());
            }
        }
        K::GetDescendants { obj, .. } => v.push(obj.clone()),
        K::GetProperty { obj, prop, .. } | K::GetSuper { obj, prop, .. } => {
            v.push(obj.clone());
            v.push(prop.clone());
        }
        K::GetSlot { scope, slot_obj, .. } => {
            v.push(scope.clone());
            v.push(slot_obj.clone());
        }
        K::InFor { object, collection } => {
            v.push(object.clone());
            v.push(collection.clone());
        }
        K::HasNext { index, obj } | K::NextName { index, obj, .. } | K::NextValue { index, obj, .. } => {
            v.push(index.clone());
            v.push(obj.clone());
        }
        K::InitProperty { obj, prop, .. } => {
            v.push(obj.clone());
            v.push(prop.clone());
            value(&mut v);
        }
        K::InitVector { subtype, args } => {
            v.push(subtype.clone());
            all(&mut v, args);
        }
        K::NameValuePair { name } => {
            v.push(name.clone());
            value(&mut v);
        }
        K::NewArray { values } => all(&mut v, values),
        K::NewObject { pairs } => all(&mut v, pairs),
        K::Package { items, .. } => {
            if !no_block {
                all(&mut v, items);
            }
        }
        K::PostDecrement { obj, .. } | K::PostIncrement { obj, .. } => v.push(obj.clone()),
        K::SetProperty { obj, prop, .. } | K::SetSuper { obj, prop, .. } => {
            v.push(obj.clone());
            v.push(prop.clone());
            value(&mut v);
        }
        K::SetSlot { scope, slot_obj, .. } => {
            v.push(scope.clone());
            v.push(slot_obj.clone());
            value(&mut v);
        }
        K::Try { try_cmds, catch_cmds, finally, .. } => {
            if !no_block {
                all(&mut v, try_cmds);
                for c in catch_cmds {
                    all(&mut v, c);
                }
                if let Some(f) = finally {
                    all(&mut v, f);
                }
            }
        }
        K::With { scope, items } => {
            v.push(scope.clone());
            all(&mut v, items);
        }
        K::WithEnd { scope } | K::WithObject { scope } => v.push(scope.clone()),
        K::Xml { parts } => all(&mut v, parts),
        K::AGetVar { name, .. } => v.push(name.clone()),
        K::AGetMember { obj, member, .. } => {
            v.push(obj.clone());
            v.push(member.clone());
        }
        K::ASetMember { obj, member, .. } => {
            v.push(obj.clone());
            v.push(member.clone());
            value(&mut v);
        }
        K::AGetProp { target, .. } => v.push(target.clone()),
        K::ADefineLocal { name } => {
            v.push(name.clone());
            value(&mut v);
        }
        K::ADelete { obj, prop } => {
            if let Some(o) = obj {
                v.push(o.clone());
            }
            v.push(prop.clone());
        }
        K::ACallFunction { args, .. } => all(&mut v, args),
        K::ACallMethod { obj, args, .. } => {
            all(&mut v, args);
            v.push(obj.clone());
        }
        K::ANewMethod { obj, args, .. } => {
            v.push(obj.clone());
            all(&mut v, args);
        }
        K::ANewObject { name, args } => {
            v.push(name.clone());
            all(&mut v, args);
        }
        K::AInitArray { values } => all(&mut v, values),
        K::AInitObject { names, values } => {
            all(&mut v, names);
            all(&mut v, values);
        }
        K::AInc { obj } | K::ADec { obj } | K::APostInc { obj } | K::APostDec { obj } | K::AEnumerate { obj } => v.push(obj.clone()),
        K::AFunction(f) => {
            if !no_block {
                all(&mut v, &f.actions);
            }
        }
        K::ATry { try_cmds, catch_cmds, finally, .. } => {
            if !no_block {
                all(&mut v, try_cmds);
                for c in catch_cmds {
                    all(&mut v, c);
                }
                all(&mut v, finally);
            }
        }
        K::AForIn { var, obj, cmds, .. } => {
            v.push(var.clone());
            v.push(obj.clone());
            if !no_block {
                all(&mut v, cmds);
            }
        }
        K::AIfFrameLoaded { frame, actions } => {
            v.push(frame.clone());
            if !no_block {
                all(&mut v, actions);
            }
        }
        K::AClass(c) => {
            if !no_block {
                for t in c.traits.iter().filter_map(|(_, t)| t.as_ref()) {
                    v.push(t.clone());
                }
            }
        }
        K::ACastOp { ctor, obj } => {
            v.push(ctor.clone());
            v.push(obj.clone());
        }
        K::AFn { args, .. } => all(&mut v, args),
        K::ATellTarget { .. } | K::ASetTarget2 { .. } | K::AWith { .. } | K::AInterface { .. } | K::AExtends { .. } | K::AImplements { .. } => {}
        _ => value(&mut v),
    }
    v
}

/// `visitRecursively`: every item below `root`, each once, depth-first; `f` returning false stops descent.
pub fn visit_recursively(root: &It, f: &mut dyn FnMut(&It) -> bool) {
    let mut seen: Vec<*const RefCell<Item>> = Vec::new();
    fn go(it: &It, no_block: bool, seen: &mut Vec<*const RefCell<Item>>, f: &mut dyn FnMut(&It) -> bool) {
        for c in children(it, no_block) {
            let p = Rc::as_ptr(&c);
            if seen.contains(&p) {
                continue;
            }
            seen.push(p);
            if f(&c) {
                go(&c, no_block, seen, f);
            }
        }
    }
    go(root, false, &mut seen, f);
}

/// `visitRecursivelyNoBlock`: like `visit_recursively` but without entering block bodies.
/// Its visited set uses Java equality, so equal sub-items are visited once.
pub fn visit_recursively_no_block(root: &It, f: &mut dyn FnMut(&It, &[It])) {
    let mut seen: Vec<It> = Vec::new();
    let mut parents: Vec<It> = vec![root.clone()];
    fn go(it: &It, seen: &mut Vec<It>, parents: &mut Vec<It>, f: &mut dyn FnMut(&It, &[It])) {
        for c in children(it, true) {
            if super::eq::contains(seen, &c) {
                continue;
            }
            seen.push(c.clone());
            f(&c, parents);
            parents.push(c.clone());
            go(&c, seen, parents, f);
            parents.pop();
        }
    }
    go(root, &mut seen, &mut parents, f);
}

/// `getAllSubItemsRecursively`.
pub fn all_sub_items(root: &It) -> Vec<It> {
    let mut v = Vec::new();
    visit_recursively(root, &mut |i| {
        v.push(i.clone());
        true
    });
    v
}
