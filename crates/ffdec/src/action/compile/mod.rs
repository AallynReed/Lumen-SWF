//! AS1/2 compiler: parser and code generator.

mod gen;
mod lexer;
mod parser;

use std::cell::RefCell;
use std::rc::Rc;

use super::{Action, ActionList, Charset};
use crate::Swf;

/// A compile error on a 1-based line.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for CompileError {}

pub type R<T> = Result<T, CompileError>;

/// A value pushed as is (`DirectValueActionItem`).
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Str(String),
    Const(u16),
    Int(i64),
    Double(f64),
    Float(f32),
    Bool(bool),
    Null,
    Undefined,
    Reg(u8),
}

pub type VarRef = Rc<RefCell<Var>>;
pub type FuncRef = Rc<RefCell<Func>>;

/// `VariableActionItem`: a name resolved to its register, local or variable access after parsing.
#[derive(Debug)]
pub struct Var {
    pub name: String,
    pub store: Option<It>,
    pub definition: bool,
    pub boxed: Option<It>,
    pub resolved: Option<(It, It)>,
}

#[derive(Debug)]
pub struct Func {
    pub name: String,
    pub params: Vec<String>,
    pub body: Option<Vec<It>>,
    pub vars: Vec<VarRef>,
    pub inner: Vec<FuncRef>,
    pub has_eval: bool,
    pub calc_name: Option<It>,
    pub is_getter: bool,
    pub is_setter: bool,
    pub line: usize,
}

/// Commands that take no operands and, used as values, push two undefineds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd0 {
    NextFrame,
    Play,
    PrevFrame,
    Stop,
    StopAllSounds,
    ToggleHighQuality,
    StopDrag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bin {
    Add,
    Add2,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    StrictEq,
    Neq,
    StrictNeq,
    Lt,
    Le,
    Gt,
    Ge,
    FullAnd,
    FullOr,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    InstanceOf,
    StrAdd,
    StrEq,
    StrNe,
    StrLt,
    StrLe,
    StrGt,
    StrGe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    Movie,
    MovieNum,
    Variables,
    VariablesNum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintKind {
    Print,
    PrintNum,
    AsBitmap,
    AsBitmapNum,
}

pub type B = Box<It>;

/// The model items the parser builds.
#[derive(Debug, Clone)]
pub enum It {
    Direct(Val),
    Var(VarRef),
    GetVariable(B),
    SetVariable { name: B, value: B, force_set: bool },
    DefineLocal { name: B, value: Option<B> },
    GetMember { obj: B, member: B, getter: bool },
    SetMember { obj: B, member: B, value: B, setter: bool },
    GetProperty { target: B, index: i64 },
    SetProperty { target: B, index: i64, value: B },
    CallFunction { name: B, args: Vec<It> },
    CallMethod { obj: B, name: B, args: Vec<It>, getter: bool },
    NewObject { name: B, args: Vec<It>, resolved: Option<B> },
    NewMethod { obj: B, name: B, args: Vec<It>, getter: bool },
    Delete { obj: Option<B>, name: B, getter: bool },
    Function(FuncRef),
    /// Value then one action; the flag is `hasReturnValue`.
    Op1 { value: B, code: u8, ret: bool },
    Bin { op: Bin, l: B, r: B },
    And(B, B),
    Or(B, B),
    Not(B),
    Ternary(B, B, B),
    Paren(B),
    Comma(Vec<It>),
    PreInc(B),
    PreDec(B),
    PostInc(B),
    PostDec(B),
    InitArray(Vec<It>),
    InitObject { names: Vec<It>, values: Vec<It> },
    Trace(B),
    RemoveSprite(B),
    Cmd0(Cmd0),
    GetUrl { url: String, target: String },
    GetUrl2 { url: B, target: B, method: u8 },
    GotoFrame2 { frame: B, bias_flag: bool, play: bool, bias: i64 },
    FsCommand { cmd: B, param: Option<B> },
    FsCommand2(Vec<It>),
    CloneSprite { src: B, target: B, depth: B },
    StartDrag { target: B, lock: B, constrain: B, x1: Option<B>, y1: Option<B>, x2: Option<B>, y2: Option<B> },
    Load { kind: LoadKind, url: B, target: B, method: u8 },
    UnloadMovie(B),
    UnloadMovieNum(B),
    Print { kind: PrintKind, target: B, bbox: B },
    GetTime,
    GetVersion,
    Extract { value: B, index: B, count: B, mb: bool },
    CastOp { ctor: B, obj: B },
    Enumerate(B),
    UnresolvedConstant(u16),
    Duplicate,
    Push(B),
    Pop,
    Swap,
    StrictMode(i64),
    True,
    If { cond: B, on_true: Vec<It>, on_false: Option<Vec<It>>, line: usize },
    While { cond: Vec<It>, body: Vec<It>, line: usize },
    DoWhile { body: Vec<It>, cond: Vec<It>, line: usize },
    For { first: Vec<It>, cond: B, last: Vec<It>, body: Vec<It>, line: usize },
    ForIn { var: VarRef, collection: B, body: Vec<It>, line: usize },
    Switch { expr: B, values: Vec<Option<It>>, cmds: Vec<Vec<It>>, mapping: Vec<usize>, line: usize },
    Break,
    Continue,
    Return(Option<B>),
    Throw(B),
    Try { body: Vec<It>, names: Vec<It>, types: Vec<Option<It>>, catches: Vec<Vec<It>>, finally: Option<Vec<It>> },
    With { scope: B, body: Vec<It> },
    TellTarget { target: B, body: Vec<It>, nested: bool },
    IfFrameLoaded { frame: B, body: Vec<It>, line: usize },
    Block(Vec<It>),
    Empty,
    Class { name: B, extends: Option<B>, implements: Vec<It>, traits: Vec<(It, It)>, statics: Vec<bool> },
    Interface { name: B, extends: Vec<It> },
    StoreRegister { reg: u8, value: B },
}

/// Which handler a script compiles for: plain actions, or a button condition / clip event
/// written as `on(...) { }` or `onClipEvent(...) { }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Actions,
    ButtonCond,
    ClipAction,
}

/// Event flags and key code from an `on(...)` or `onClipEvent(...)` header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Events {
    /// Condition bits as stored, most significant first: BUTTONCONDACTION's 16 without the key,
    /// or CLIPEVENTFLAGS' 32.
    pub flags: u32,
    pub key: u32,
}

/// What `actionsFromString` produces.
#[derive(Debug)]
pub struct Compiled {
    pub actions: ActionList,
    pub events: Option<Events>,
}

/// Class names exported as `__Packages.*` by the SWF, which casts recognize.
pub fn swf_classes(swf: &Swf) -> Vec<String> {
    let names = crate::names::Names::new(swf);
    let mut out = Vec::new();
    swf.walk(|_, t| {
        if t.code == crate::tags::code::DO_INIT_ACTION {
            if let Some(n) = t.body.get(..2).and_then(|b| names.export.get(&u16::from_le_bytes([b[0], b[1]]))) {
                if let Some(c) = n.strip_prefix("__Packages.") {
                    out.push(c.to_string());
                }
            }
        }
    });
    out
}

/// `ActionScript2Parser.actionsFromString`.
pub fn compile(text: &str, version: u8, target: Target, classes: &[String], swf: Option<&Swf>) -> R<Compiled> {
    let cs = Charset::for_version(version);
    let first = parser::Parser::run(text, version, target, classes, swf, Vec::new())?;
    match gen::actions(&first, version, cs, true)? {
        gen::Out::Done(actions) => Ok(Compiled { actions: finish(actions, cs, version), events: first.events }),
        gen::Out::Again(pool) => {
            let second = parser::Parser::run(text, version, target, classes, swf, pool)?;
            match gen::actions(&second, version, cs, false)? {
                gen::Out::Done(actions) => Ok(Compiled { actions: finish(actions, cs, version), events: second.events }),
                gen::Out::Again(_) => Err(CompileError { line: 0, message: "constant pool did not settle".into() }),
            }
        }
    }
}

fn finish(mut actions: Vec<Action>, cs: Charset, version: u8) -> ActionList {
    actions.push(Action::new(0, super::Op::End));
    let mut address = 0;
    for a in &mut actions {
        a.length = a.content_len(cs);
        a.address = address;
        address += a.total_len();
    }
    ActionList { actions, charset: cs, version }
}
