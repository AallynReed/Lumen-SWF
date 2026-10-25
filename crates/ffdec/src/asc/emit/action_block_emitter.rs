//! Port of `macromedia/asc/embedding/avmplus/ActionBlockEmitter.java`: the `Emitter` that turns `CodeGenerator`'s
//! calls into AVM2 bytecode and builds the ABC (constant pools, method infos and bodies, traits, classes, scripts).

#![allow(clippy::too_many_arguments, unused_assignments, dead_code)]

use std::cell::{RefCell, RefMut};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::bytecode::{il_format_double_as_string, int, int24, ByteCodeFactory};
use super::constants::*;
use super::debug_info::DebugInfo;
use super::features::TARGET_AVM1;
use super::runtime_constants::*;
use crate::abc::decomp::ecma::java_parse_double;
use crate::asc::parser::chars::java_digit16;
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::{EMPTY_TOKEN, GET_TOKEN, SET_TOKEN};
use crate::asc::semantics::builder::REMOVE_BUILDER_NAMES;
use crate::asc::semantics::emitter::{self, Emitter, EmitterBase};
use crate::asc::semantics::meta_data::MetaData;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::slot::Slot;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::Value;
use crate::asc::util::byte_list::ByteList;
use crate::asc::util::context::{Context, ContextRef, NS_EXPLICIT, NS_INTERNAL, NS_PRIVATE, NS_PROTECTED, NS_STATIC_PROTECTED};
use crate::asc::util::decimal128::Decimal128;
use crate::asc::util::int_list::IntList;
use crate::asc::util::names::Names;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::number_constant::NumberConstant;
use crate::asc::util::number_usage::NumberUsage;
use crate::asc::util::qualifiers::Qualifiers;

pub const MAJOR: i32 = 46;
pub const MINOR: i32 = 16;

pub const IKIND_push: i32 = 1;
pub const IKIND_other: i32 = 2;
pub const IKIND_coerce: i32 = 3;

pub const DI_BAD: i32 = 0;
pub const DI_LOCAL: i32 = 1;

const MAXUINT: i64 = i32::MAX as i64 + i32::MAX as i64 + 1;

/// `PrintWriter.println()` uses the platform line separator.
const NL: &str = if cfg!(windows) { "\r\n" } else { "\n" };

fn il_last(l: &IntList) -> i32 {
    l.last().copied().unwrap_or(0)
}

fn il_remove_last(l: &mut IntList) -> i32 {
    l.pop().unwrap_or(0)
}

fn il_get(l: &IntList, i: i32) -> i32 {
    if i < 0 || i as usize >= l.len() {
        panic!("ArrayIndexOutOfBoundsException: {}", i);
    }
    l[i as usize]
}

fn const_name(s: &str) -> String {
    s.replace(['.', '/', ':', '|', '$'], "_")
}

fn tv_eq(a: &Option<TypeValue>, b: &Option<TypeValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn ov_opt_eq(a: &Option<ObjectValue>, b: &Option<ObjectValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn declared_by_is(slot: &Slot, obj: &ObjectValue) -> bool {
    slot.declared_by().is_some_and(|d| d.ptr_eq(obj))
}

/// `Character.digit(c, 16)`.
fn parse_double(s: &str) -> f64 {
    java_parse_double(s).unwrap_or_else(|| panic!("NumberFormatException: For input string: \"{}\"", s))
}

fn parse_decimal(s: &str) -> Decimal128 {
    Decimal128::from_str(s).unwrap_or_else(|e| panic!("NumberFormatException: {}", e))
}

/// `Integer.toHexString`.
fn hex(v: i32) -> String {
    format!("{:x}", v as u32)
}

/// `ActionBlock.addBytesToTable`: the 1-based index of an equal entry, else appends.
pub fn add_bytes_to_table(table: &mut Vec<ByteList>, bytes: ByteList) -> i32 {
    if let Some(idx) = table.iter().position(|l| *l == bytes) {
        return idx as i32 + 1;
    }
    table.push(bytes);
    table.len() as i32
}

fn add_pooled(map: &mut HashMap<ByteList, i32>, pool: &mut Vec<ByteList>, bytes: ByteList, show: bool, bf: &RefCell<ByteCodeFactory>) -> i32 {
    if let Some(&i) = map.get(&bytes) {
        return i + 1;
    }
    map.insert(bytes.clone(), pool.len() as i32);
    pool.push(bytes);
    let index = pool.len() as i32;
    if show {
        bf.borrow_mut().cpool_out.push_str(&format!(" -> {}", index));
    }
    index
}

/// `ActionBlockEmitter.ActionBlock`.
pub struct ActionBlock {
    pub minor_version: i32,
    pub major_version: i32,
    pub vars_count: i32,
    pub vars: Vec<ByteList>,
    pub methods_count: i32,
    pub methods: Vec<ByteList>,
    pub metadata_count: i32,
    pub metadata: Vec<ByteList>,
    pub bodies_count: i32,
    pub bodies: Vec<ByteList>,
    pub dispids_count: i32,
    pub dispids: Vec<ByteList>,
    pub classes_count: i32,
    pub classes: Vec<ByteList>,
    pub instances: Vec<ByteList>,
    pub interfaces_count: i32,
    pub interfaces: Vec<ByteList>,
    pub scripts_count: i32,
    pub scripts: Vec<ByteList>,
    pub main_index: i32,

    bodies_map: HashMap<ByteList, i32>,
    constant_utf8_map: HashMap<ByteList, i32>,
    pub constant_utf8_pool: Vec<ByteList>,
    constant_mn_map: HashMap<ByteList, i32>,
    pub constant_mn_pool: Vec<ByteList>,
    constant_nss_map: HashMap<ByteList, i32>,
    pub constant_nss_pool: Vec<ByteList>,
    constant_ns_map: HashMap<ByteList, i32>,
    pub constant_ns_pool: Vec<ByteList>,
    constant_double_map: HashMap<ByteList, i32>,
    pub constant_double_pool: Vec<ByteList>,
    constant_uint_map: HashMap<ByteList, i32>,
    pub constant_uint_pool: Vec<ByteList>,
    constant_int_map: HashMap<ByteList, i32>,
    pub constant_int_pool: Vec<ByteList>,
    constant_decimal_map: HashMap<ByteList, i32>,
    pub constant_decimal_pool: Vec<ByteList>,

    pub exception_count: i32,
    pub exception_table: ByteList,
    pub code: Option<ByteList>,

    pub show_bytecode: bool,
    bytecode_factory: Rc<RefCell<ByteCodeFactory>>,
}

impl ActionBlock {
    pub fn new(cx: &Context, show_bytecode: bool, bytecode_factory: Rc<RefCell<ByteCodeFactory>>) -> ActionBlock {
        ActionBlock {
            minor_version: if cx.statics.borrow().es4_numerics { MINORwithDECIMAL } else { MINOR },
            major_version: MAJOR,
            vars_count: 0,
            vars: Vec::new(),
            methods_count: 0,
            methods: Vec::new(),
            metadata_count: 0,
            metadata: Vec::new(),
            bodies_count: 0,
            bodies: Vec::new(),
            dispids_count: 0,
            dispids: Vec::new(),
            classes_count: 0,
            classes: Vec::new(),
            instances: Vec::new(),
            interfaces_count: 0,
            interfaces: Vec::new(),
            scripts_count: 0,
            scripts: Vec::new(),
            main_index: 0,
            bodies_map: HashMap::new(),
            constant_utf8_map: HashMap::new(),
            constant_utf8_pool: Vec::new(),
            constant_mn_map: HashMap::new(),
            constant_mn_pool: Vec::new(),
            constant_nss_map: HashMap::new(),
            constant_nss_pool: Vec::new(),
            constant_ns_map: HashMap::new(),
            constant_ns_pool: Vec::new(),
            constant_double_map: HashMap::new(),
            constant_double_pool: Vec::new(),
            constant_uint_map: HashMap::new(),
            constant_uint_pool: Vec::new(),
            constant_int_map: HashMap::new(),
            constant_int_pool: Vec::new(),
            constant_decimal_map: HashMap::new(),
            constant_decimal_pool: Vec::new(),
            exception_count: 0,
            exception_table: ByteCodeFactory::alloc_bytes(),
            code: None,
            show_bytecode,
            bytecode_factory,
        }
    }

    pub fn add_body(&mut self, bytes: ByteList) -> i32 {
        if let Some(&i) = self.bodies_map.get(&bytes) {
            return i + 1;
        }
        self.bodies_map.insert(bytes.clone(), self.bodies.len() as i32);
        self.bodies.push(bytes);
        self.bodies.len() as i32
    }

    pub fn add_utf8_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_utf8_map, &mut self.constant_utf8_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_multi_name_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_mn_map, &mut self.constant_mn_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_ns_set_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_nss_map, &mut self.constant_nss_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_ns_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_ns_map, &mut self.constant_ns_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_double_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_double_map, &mut self.constant_double_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_uint_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_uint_map, &mut self.constant_uint_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_int_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_int_map, &mut self.constant_int_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }

    pub fn add_decimal_constant(&mut self, bytes: ByteList) -> i32 {
        add_pooled(&mut self.constant_decimal_map, &mut self.constant_decimal_pool, bytes, self.show_bytecode, &self.bytecode_factory)
    }
}

/// `ActionBlockEmitter.ExceptionBlock`.
#[derive(Clone)]
pub struct ExceptionBlock {
    pub try_start: i32,
    pub try_end: i32,
    pub fixups: IntList,
    pub finally_addrs: IntList,
    pub scope_index: i32,
    pub has_finally: bool,
    pub cur_locals: i32,
    pub loop_index: i32,
}

impl Default for ExceptionBlock {
    fn default() -> Self {
        ExceptionBlock {
            try_start: 0,
            try_end: 0,
            fixups: Vec::new(),
            finally_addrs: Vec::new(),
            scope_index: -1,
            has_finally: false,
            cur_locals: 0,
            loop_index: -1,
        }
    }
}

/// The methods a subclass (flex2's `BytecodeEmitter`) overrides; each default is the `ActionBlockEmitter` body.
pub trait ActionBlockEmitterHooks {
    fn add_metadata(&mut self, abe: &mut ActionBlockEmitter, metadata: Option<&[MetaData]>) -> Option<IntList> {
        abe.base_add_metadata(metadata)
    }

    fn debug_slot(&mut self, abe: &mut ActionBlockEmitter, name: &str, slot: i32, linenum: i32) {
        abe.base_debug_slot(name, slot, linenum)
    }

    fn debug_file(&mut self, abe: &mut ActionBlockEmitter, name: &str) {
        abe.base_debug_file(name)
    }

    /// `DebugLine(ByteList code, int linenum)`.
    fn debug_line_op(&mut self, abe: &mut ActionBlockEmitter, linenum: i32) {
        abe.base_debug_line_op(linenum)
    }
}

macro_rules! hook {
    ($self:ident.$m:ident($($a:expr),*) else $base:ident) => {{
        match $self.hooks.take() {
            Some(mut h) => {
                let r = h.$m($self $(, $a)*);
                $self.hooks = Some(h);
                r
            }
            None => $self.$base($($a),*),
        }
    }};
}

/// `if (show_instructions) { code_out.println(); code_out.print(...); }`
macro_rules! ci {
    ($self:ident, $($arg:tt)*) => {
        if $self.show_instructions {
            $self.code_out.push_str(NL);
            let s = format!($($arg)*);
            $self.code_out.push_str(&s);
        }
    };
}

/// `if (show_instructions) code_out.write(" [" + cur_stack + "]");`
macro_rules! cs {
    ($self:ident) => {
        if $self.show_instructions {
            let s = format!(" [{}]", $self.cur_stack);
            $self.code_out.push_str(&s);
        }
    };
}

/// `if (bytecodeFactory.show_bytecode) code_out.write("\n      " + getIP() + ":" + ...);`
macro_rules! bc {
    ($self:ident, $($arg:tt)*) => {
        if $self.show_bytecode() {
            let s = format!("\n      {}:{}", $self.ip(), format!($($arg)*));
            $self.code_out.push_str(&s);
        }
    };
}

/// Instructions with no operand: `stack(delta)` (or none), the trace line, the opcode.
macro_rules! simple_ops {
    ($($name:ident($delta:expr, $label:expr, $op:expr);)*) => {
        $(pub fn $name(&mut self) {
            if let Some(d) = $delta {
                self.stack(d);
            }
            bc!(self, "{}", $label);
            self.byte($op);
        })*
    };
}

/// Instructions with one U30 operand.
macro_rules! int_ops {
    ($($name:ident($delta:expr, $label:expr, $op:expr);)*) => {
        $(pub fn $name(&mut self, index: i32) {
            if let Some(d) = $delta {
                self.stack(d);
            }
            bc!(self, "{} {}", $label, index);
            self.byte($op);
            self.int(index);
        })*
    };
}

/// Instructions with a decimal-params U30 operand.
macro_rules! param_ops {
    ($($name:ident($delta:expr, $label:expr, $op:expr);)*) => {
        $(pub fn $name(&mut self, decimal_params: i32) {
            self.stack($delta);
            bc!(self, "{}{})", $label, hex(decimal_params));
            self.byte($op);
            self.int(decimal_params);
        })*
    };
}

/// Conditional branches: `If*(code)` is `If*(code, 0)`.
macro_rules! if_ops {
    ($($name:ident($delta:expr, $label:expr, $op:expr);)*) => {
        $(pub fn $name(&mut self, offset: i32) {
            self.stack($delta);
            bc!(self, "{}", $label);
            self.byte($op);
            self.int24(offset);
        })*
    };
}

pub struct ActionBlockEmitter {
    pub base: EmitterBase,
    pub ab: ActionBlock,
    cx: ContextRef,
    bytecode_factory: Rc<RefCell<ByteCodeFactory>>,

    pub max_locals: i32,
    pub cur_locals: i32,
    pub max_stack: i32,
    pub cur_stack: i32,
    pub max_scope: i32,
    pub cur_scope: i32,
    pub max_params: i32,
    pub scriptname: Option<String>,
    pub modulename: Option<String>,

    pub exception_blocks: Vec<ExceptionBlock>,

    pub if_addrs: IntList,
    pub try_addrs: IntList,
    pub else_addrs: IntList,
    pub loopbegin_addrs: IntList,
    pub break_addrs: Vec<IntList>,
    pub break_scope_depth: IntList,
    pub break_temp_count: IntList,
    pub continue_addrs: Vec<IntList>,
    pub continue_scope_depth: IntList,
    pub switchbegin_addrs: IntList,
    pub case_addrs: Vec<IntList>,
    pub default_addrs: IntList,
    pub seen_default_case: IntList,
    sets_dxns: bool,

    ns_constants: HashMap<ObjectValue, i32>,

    pub method_infos: Vec<String>,
    pub method_infos_map: HashMap<String, i32>,
    pub metadata_infos: Vec<Option<String>>,
    pub metadata_infos_map: HashMap<Option<String>, i32>,
    pub class_infos: Vec<QName>,
    pub package_infos: Vec<String>,
    pub global_method_names: Vec<String>,

    pub stack_depth_stack: IntList,
    pub scope_depth_stack: IntList,

    pub last_in: i32,
    pub last_ip: i32,

    doing_api_versioning: bool,

    pub show_instructions: bool,
    pub show_linenums: bool,
    pub emit_debug_info: bool,
    pub emit_code_hints: bool,

    debug_info: DebugInfo,

    pub code_out: String,
    pub header_out: String,
    pub native_method_count: i32,
    pub native_class_count: i32,
    pub native_package_count: i32,

    hooks: Option<Box<dyn ActionBlockEmitterHooks>>,
}

impl ActionBlockEmitter {
    /// `ActionBlockEmitter(cx, scriptname)`.
    pub fn new_default(cx: &ContextRef, scriptname: Option<&str>) -> ActionBlockEmitter {
        ActionBlockEmitter::new(cx, scriptname, false, true, false, false, false)
    }

    /// The 9-argument constructor; `code_out` and `header_out` are the `code_out`/`header_out` strings.
    pub fn new(
        cx: &ContextRef,
        scriptname: Option<&str>,
        show_instructions: bool,
        show_bytecode: bool,
        show_linenums: bool,
        emit_debug_info: bool,
        emit_code_hints: bool,
    ) -> ActionBlockEmitter {
        let bytecode_factory = cx.borrow().get_byte_code_factory();
        bytecode_factory.borrow_mut().show_bytecode = show_bytecode;
        let ab = ActionBlock::new(&cx.borrow(), show_bytecode, bytecode_factory.clone());
        ActionBlockEmitter {
            base: EmitterBase::new(None),
            ab,
            cx: cx.clone(),
            bytecode_factory,
            max_locals: 0,
            cur_locals: 0,
            max_stack: 0,
            cur_stack: 0,
            max_scope: 0,
            cur_scope: 0,
            max_params: 0,
            scriptname: scriptname.map(String::from),
            modulename: None,
            exception_blocks: Vec::new(),
            if_addrs: Vec::new(),
            try_addrs: Vec::new(),
            else_addrs: Vec::new(),
            loopbegin_addrs: Vec::new(),
            break_addrs: Vec::new(),
            break_scope_depth: Vec::new(),
            break_temp_count: Vec::new(),
            continue_addrs: Vec::new(),
            continue_scope_depth: Vec::new(),
            switchbegin_addrs: Vec::new(),
            case_addrs: Vec::new(),
            default_addrs: Vec::new(),
            seen_default_case: Vec::new(),
            sets_dxns: false,
            ns_constants: HashMap::new(),
            method_infos: Vec::new(),
            method_infos_map: HashMap::new(),
            metadata_infos: Vec::new(),
            metadata_infos_map: HashMap::new(),
            class_infos: Vec::new(),
            package_infos: Vec::new(),
            global_method_names: Vec::new(),
            stack_depth_stack: Vec::new(),
            scope_depth_stack: Vec::new(),
            last_in: 0,
            last_ip: 0,
            doing_api_versioning: false,
            show_instructions,
            show_linenums,
            emit_debug_info,
            emit_code_hints: emit_debug_info || emit_code_hints,
            debug_info: DebugInfo::default(),
            code_out: String::new(),
            header_out: String::new(),
            native_method_count: 0,
            native_class_count: 0,
            native_package_count: 1,
            hooks: None,
        }
    }

    /// Installs a subclass's overrides (see [`ActionBlockEmitterHooks`]).
    pub fn set_hooks(&mut self, hooks: Option<Box<dyn ActionBlockEmitterHooks>>) {
        self.hooks = hooks;
    }

    pub fn context(&self) -> ContextRef {
        self.cx.clone()
    }

    fn bf(&self) -> RefMut<'_, ByteCodeFactory> {
        self.bytecode_factory.borrow_mut()
    }

    fn show_bytecode(&self) -> bool {
        self.bytecode_factory.borrow().show_bytecode
    }

    fn code(&mut self) -> &mut ByteList {
        self.ab.code.as_mut().expect("NullPointerException")
    }

    fn byte(&mut self, v: i32) {
        self.code().push(v as u8);
    }

    fn int(&mut self, v: i32) {
        int(self.code(), v as i64);
    }

    fn int24(&mut self, v: i32) {
        int24(self.code(), v);
    }

    fn patch24(&mut self, at: i32, offset: i32) {
        let code = self.code();
        code[at as usize] = offset as u8;
        code[at as usize + 1] = (offset >> 8) as u8;
        code[at as usize + 2] = (offset >> 16) as u8;
    }

    fn header_println(&mut self, s: &str) {
        self.header_out.push_str(s);
        self.header_out.push_str(NL);
    }

    fn utf8(&mut self, s: &str) -> i32 {
        let b = self.bf().constant_utf8_info(s);
        self.ab.add_utf8_constant(b)
    }

    fn mn_qname(&mut self, name_index: i32, namespace_index: i32, is_attr: bool) -> i32 {
        let b = self.bf().constant_qualified_name(name_index, namespace_index, is_attr);
        self.ab.add_multi_name_constant(b)
    }

    fn mn_multiname(&mut self, name_index: i32, namespace_set_index: i32, is_attr: bool) -> i32 {
        let b = self.bf().constant_multiname(name_index, namespace_set_index, is_attr);
        self.ab.add_multi_name_constant(b)
    }

    fn mn_multiname_late(&mut self, namespace_set_index: i32, is_attr: bool) -> i32 {
        let b = self.bf().constant_multiname_late(namespace_set_index, is_attr);
        self.ab.add_multi_name_constant(b)
    }

    fn mn_rtqname(&mut self, name_index: i32, is_attr: bool) -> i32 {
        let b = self.bf().constant_runtime_qualified_name(name_index, is_attr);
        self.ab.add_multi_name_constant(b)
    }

    fn mn_rtqname_late(&mut self, is_attr: bool) -> i32 {
        let b = self.bf().constant_runtime_qualified_late(is_attr);
        self.ab.add_multi_name_constant(b)
    }

    fn int_const(&mut self, v: i32) -> i32 {
        let b = self.bf().constant_integer_info(v);
        self.ab.add_int_constant(b)
    }

    fn uint_const(&mut self, v: i64) -> i32 {
        let b = self.bf().constant_uint_info(v);
        self.ab.add_uint_constant(b)
    }

    fn double_const(&mut self, v: f64) -> i32 {
        let b = self.bf().constant_double_info(v);
        self.ab.add_double_constant(b)
    }

    fn decimal_const(&mut self, v: &Decimal128) -> i32 {
        let b = self.bf().constant_decimal_info(v);
        self.ab.add_decimal_constant(b)
    }

    /// `name` (the name part of a `"*"`-aware lookup): 0 is the any name.
    fn name_index(&mut self, name: &str) -> i32 {
        if name == "*" {
            0
        } else {
            self.utf8(name)
        }
    }

    pub fn add_class_name(&mut self, name: Option<&QName>) -> i32 {
        let name = match name {
            Some(n) if n.to_string() != "*" => n,
            _ => return 0,
        };
        let classname_string = name.name().to_string();
        let namespace_index = self.add_namespace(&name.ns().expect("NullPointerException"));
        let name_index = self.utf8(&classname_string);
        let mut qname_index = self.mn_qname(name_index, namespace_index, false);

        if let Some(type_params) = name.type_params() {
            let mut params: IntList = Vec::with_capacity(type_params.len());
            for p in type_params {
                params.push(self.add_class_name(Some(p)));
            }
            let b = self.bf().constant_type_name(qname_index, &params);
            qname_index = self.ab.add_multi_name_constant(b);
        }
        qname_index
    }

    pub fn add_namespace(&mut self, ns: &ObjectValue) -> i32 {
        self.add_namespace_v(ns, -1)
    }

    fn mark_uri(&self, ns: &ObjectValue, v: i32) -> String {
        if v < 0 || !self.is_versioned_namespace(ns) {
            return ns.name();
        }
        let mut uri = ns.name();
        uri.push(char::from_u32(0xE000 + v as u32).expect("version mark"));
        uri
    }

    fn format_ns_kind(&self, ns: &ObjectValue) -> &'static str {
        match ns.get_namespace_kind() {
            NS_PRIVATE => "private",
            NS_PROTECTED | NS_STATIC_PROTECTED => "protected",
            NS_INTERNAL => "internal",
            _ => "public",
        }
    }

    fn is_versioned_namespace(&self, ns: &ObjectValue) -> bool {
        match ns.get_namespace_kind() {
            NS_PRIVATE | NS_PROTECTED | NS_STATIC_PROTECTED | NS_INTERNAL | NS_EXPLICIT => {}
            _ => {
                if ns.is_package() {
                    return true;
                }
            }
        }
        false
    }

    pub fn add_namespace_v(&mut self, ns: &ObjectValue, v: i32) -> i32 {
        let any = self.cx.borrow().any_namespace();
        if ns.ptr_eq(&any) {
            return 0;
        }
        let uri = self.mark_uri(ns, v);
        let namespace_name_index = self.utf8(&uri);
        let b = if ns.is_package() {
            self.bf().constant_package_namespace(namespace_name_index)
        } else {
            match ns.get_namespace_kind() {
                NS_INTERNAL => self.bf().constant_internal_namespace(namespace_name_index),
                NS_PRIVATE => self.bf().constant_private_namespace(namespace_name_index),
                NS_PROTECTED => self.bf().constant_protected_namespace(namespace_name_index),
                NS_STATIC_PROTECTED => self.bf().constant_static_protected_namespace(namespace_name_index),
                _ => self.bf().constant_namespace(namespace_name_index),
            }
        };
        let namespace_index = self.ab.add_ns_constant(b);
        self.ns_constants.insert(ns.clone(), namespace_index);
        namespace_index
    }

    pub fn make_namespace_set(&mut self, namespaces: &Namespaces) -> i32 {
        let mut namespace_set = BTreeSet::new();
        for ns in namespaces.to_vec() {
            namespace_set.insert(self.add_namespace(&ns));
        }
        let set: Vec<i32> = namespace_set.into_iter().collect();
        let b = self.bf().constant_namespace_set(&set);
        self.ab.add_ns_set_constant(b)
    }

    fn make_versioned_namespace_set(&mut self, ns: Option<&ObjectValue>, versions: &BTreeSet<i32>) -> i32 {
        let mut namespace_set = BTreeSet::new();
        if versions.is_empty() {
            self.cx.borrow().internal_error("internal error: empty versions set");
        } else {
            for &v in versions {
                match ns {
                    Some(ns) => {
                        namespace_set.insert(self.add_namespace_v(ns, v));
                    }
                    None => self.cx.borrow().internal_error("internal error: non object value for namespace"),
                }
            }
        }
        let set: Vec<i32> = namespace_set.into_iter().collect();
        let b = self.bf().constant_namespace_set(&set);
        self.ab.add_ns_set_constant(b)
    }

    pub fn make_multiname(&mut self, name: &str, namespaces: &Namespaces) -> i32 {
        let name_index = self.utf8(name);
        let namespaces_set = self.make_namespace_set(namespaces);
        self.mn_multiname(name_index, namespaces_set, false)
    }

    fn get_value_index(&mut self, obj_value: &ObjectValue) -> i32 {
        let cxr = self.cx.clone();
        let cx = cxr.borrow();
        let mut value_index = 0;
        let value = obj_value.to_string();
        let default_value_type = obj_value.type_().and_then(|t| t.get_type_value());
        let number_usage = obj_value.get_number_usage();
        let dvt = &default_value_type;
        let es4 = cx.statics.borrow().es4_numerics;
        if tv_eq(dvt, &Some(cx.boolean_type())) {
            value_index = if value == "true" { CONSTANT_True } else { CONSTANT_False };
        } else if tv_eq(dvt, &Some(cx.string_type())) {
            value_index = self.utf8(&value);
        } else if tv_eq(dvt, &Some(cx.int_type())) {
            let v = self.get_value_of_number_literal(&value, &mut None, number_usage.as_ref()).unwrap().int_value();
            value_index = self.int_const(v);
        } else if tv_eq(dvt, &Some(cx.double_type())) || (cx.abc_version(TARGET_AVM1) && tv_eq(dvt, &Some(cx.uint_type()))) {
            let v = self.get_value_of_number_literal(&value, &mut None, number_usage.as_ref()).unwrap().double_value();
            value_index = self.double_const(v);
        } else if tv_eq(dvt, &Some(cx.uint_type())) {
            let v = self.get_value_of_number_literal(&value, &mut None, number_usage.as_ref()).unwrap().uint_value();
            value_index = self.uint_const(v);
        } else if es4 && tv_eq(dvt, &cx.decimal_type()) {
            let v = self.get_value_of_number_literal(&value, &mut None, number_usage.as_ref()).unwrap().decimal_value();
            value_index = self.decimal_const(&v);
        } else if tv_eq(dvt, &Some(cx.null_type())) {
            value_index = CONSTANT_Null;
        } else if tv_eq(dvt, &Some(cx.void_type())) {
            value_index = 0;
        } else if cx.is_namespace(Some(obj_value)) {
            value_index = self.add_namespace(obj_value);
        }
        value_index
    }

    fn get_value_kind(&mut self, obj_value: &ObjectValue) -> i8 {
        let cxr = self.cx.clone();
        let cx = cxr.borrow();
        let mut value_kind = 0;
        let value = obj_value.to_string();
        let dvt = &obj_value.type_().and_then(|t| t.get_type_value());
        let es4 = cx.statics.borrow().es4_numerics;
        if tv_eq(dvt, &Some(cx.boolean_type())) {
            value_kind = if value == "true" { CONSTANT_True } else { CONSTANT_False };
        } else if tv_eq(dvt, &Some(cx.string_type())) {
            value_kind = CONSTANT_Utf8;
        } else if tv_eq(dvt, &Some(cx.int_type())) {
            value_kind = CONSTANT_Integer;
        } else if tv_eq(dvt, &Some(cx.double_type())) || (cx.abc_version(TARGET_AVM1) && tv_eq(dvt, &Some(cx.uint_type()))) {
            value_kind = CONSTANT_Double;
        } else if tv_eq(dvt, &Some(cx.uint_type())) {
            value_kind = CONSTANT_UInteger;
        } else if es4 && tv_eq(dvt, &cx.decimal_type()) {
            value_kind = CONSTANT_Decimal;
        } else if tv_eq(dvt, &Some(cx.null_type())) || tv_eq(dvt, &Some(cx.void_type())) {
            value_kind = CONSTANT_Null;
        } else if cx.is_namespace(Some(obj_value)) {
            value_kind = CONSTANT_Namespace;
        }
        value_kind as i8
    }

    pub fn make_dispatch_method(&mut self, _cx: &ContextRef) {}

    pub fn get_metadata_info(&mut self, name: Option<&str>) -> i32 {
        let key = name.map(String::from);
        if let Some(&id) = self.metadata_infos_map.get(&key) {
            return id;
        }
        let id = self.metadata_infos_map.len() as i32;
        self.metadata_infos_map.insert(key.clone(), id);
        self.metadata_infos.push(key);
        self.ab.metadata.push(ByteCodeFactory::alloc_bytes());
        if self.metadata_infos.len() != self.ab.metadata.len() {
            self.cx.borrow().internal_error("internal error: internal metadata name and info out of sync");
        }
        id
    }

    pub fn get_class_info(&mut self, name: &QName) -> i32 {
        let mut i = 0;
        while i < self.class_infos.len() {
            if self.class_infos[i].java_equals(name) {
                return i as i32;
            }
            i += 1;
        }
        self.class_infos.push(name.clone());
        self.ab.classes.push(ByteCodeFactory::alloc_bytes());
        self.ab.instances.push(ByteCodeFactory::alloc_bytes());
        if self.class_infos.len() != self.ab.classes.len() {
            self.cx.borrow().internal_error("internal error: internal class name and info out of sync");
        }
        i as i32
    }

    pub fn get_package_info(&mut self, name: &str) -> i32 {
        self.get_package_info_add(name, false)
    }

    pub fn get_package_info_add(&mut self, name: &str, dont_add: bool) -> i32 {
        if let Some(i) = self.package_infos.iter().position(|p| p == name) {
            return i as i32;
        }
        if dont_add {
            return -1;
        }
        let i = self.package_infos.len() as i32;
        self.package_infos.push(name.to_string());
        self.ab.scripts.push(ByteCodeFactory::alloc_bytes());
        if self.package_infos.len() != self.ab.scripts.len() {
            self.cx.borrow().internal_error("internal error: internal class name and info out of sync");
        }
        i
    }

    pub fn get_method_name(&mut self, n: i32) -> String {
        if n > 0 && n as usize <= self.global_method_names.len() {
            return self.global_method_names[n as usize - 1].clone();
        }
        self.cx.borrow().internal_error("invalid method name");
        String::new()
    }

    pub fn get_binary_name(op_index: i32) -> &'static str {
        match op_index {
            BINARY_BinaryPlusOp_II => "BinaryPlusOp_II",
            BINARY_BinaryPlusOp => "BinaryPlusOp",
            BINARY_BinaryMinusOp => "BinaryMinusOp",
            BINARY_BinaryMinusOp_II => "BinaryMinusOp_II",
            BINARY_MultiplyOp => "MultiplyOp",
            BINARY_MultiplyOp_II => "MultiplyOp_II",
            BINARY_DivideOp => "DivideOp",
            BINARY_ModulusOp => "ModulusOp",
            BINARY_LeftShiftOp => "LeftShiftOp",
            BINARY_LeftShiftOp_II => "LeftShiftOp_II",
            BINARY_RightShiftOp => "RightShiftOp",
            BINARY_RightShiftOp_II => "RightShiftOp_II",
            BINARY_UnsignedRightShiftOp => "UnsignedRightShiftOp",
            BINARY_UnsignedRightShiftOp_II => "UnsignedRightShiftOp_II",
            BINARY_LessThanOp => "LessThanOp",
            BINARY_GreaterThanOp => "GreaterThanOp",
            BINARY_LessThanOrEqualOp => "LessThanOrEqualOp",
            BINARY_GreaterThanOrEqualOp => "GreaterThanOrEqualOp",
            BINARY_InstanceofOp => "InstanceofOp",
            BINARY_InOp => "InOp",
            BINARY_EqualsOp_II => "EqualsOp_II",
            BINARY_EqualsOp => "EqualsOp",
            BINARY_NotEqualsOp_II => "NotEqualsOp_II",
            BINARY_NotEqualsOp => "NotEqualsOp",
            BINARY_StrictEqualsOp => "StrictEqualsOp",
            BINARY_StrictEqualsOp_II => "StrictEqualsOp_II",
            BINARY_StrictNotEqualsOp => "StrictNotEqualsOp",
            BINARY_StrictNotEqualsOp_II => "StrictNotEqualsOp_II",
            BINARY_BitwiseAndOp => "BitwiseAndOp",
            BINARY_BitwiseAndOp_II => "BitwiseAndOp_II",
            BINARY_BitwiseXorOp => "BitwiseXorOp",
            BINARY_BitwiseXorOp_II => "BitwiseXorOp_II",
            BINARY_BitwiseOrOp => "BitwiseOrOp",
            BINARY_BitwiseOrOp_II => "BitwiseOrOp_II",
            BINARY_LogicalAndOp => "LogicalAndOp",
            BINARY_LogicalAndOp_II => "LogicalAndOp_II",
            BINARY_LogicalOrOp => "LogicalOrOp",
            BINARY_LogicalOrOp_II => "LogicalOrOp_II",
            BINARY_IsLateOp => "IsLateOp",
            BINARY_IsOp => "IsOp",
            BINARY_AsLateOp => "AsLateOp",
            BINARY_AsOp => "AsOp",
            _ => "N/A",
        }
    }

    fn get_decimal_value_or_int(&self, cx: &Context, str: &str, force: bool, pp_type: &mut Option<TypeValue>) -> NumberConstant {
        if str == "NaN" {
            *pp_type = cx.decimal_type();
            return NumberConstant::Decimal(Decimal128::nan());
        } else if str == "Infinity" {
            *pp_type = cx.decimal_type();
            return NumberConstant::Decimal(Decimal128::infinity());
        } else if str == "-Infinity" {
            *pp_type = cx.decimal_type();
            return NumberConstant::Decimal(Decimal128::neginfinity());
        }

        let s: Vec<u16> = crate::asc::util::jstr::to_utf16(str);
        let mut dval;
        let mut is_int = false;
        if (str.contains('.') || str.contains('e') || str.contains('E') || s.len() > 10) && !(str.contains('x') || str.contains('X')) {
            dval = parse_decimal(str);
        } else {
            is_int = true;
            let mut base = 10;
            let mut start_index = 0usize;
            let len = s.len();
            let mut negative = false;
            if len > 1 {
                let c0 = s[start_index];
                if c0 == '-' as u16 || c0 == '+' as u16 {
                    start_index += 1;
                    if c0 == '-' as u16 {
                        negative = true;
                    }
                }
            }
            if len > 2 && s[start_index] == '0' as u16 {
                start_index += 1;
                let c0 = s[start_index];
                if c0 == 'x' as u16 || c0 == 'X' as u16 {
                    base = 16;
                    start_index += 1;
                }
            }
            while (start_index as i64) < len as i64 - 1 && s[start_index] == '0' as u16 {
                start_index += 1;
            }
            if base == 10 {
                let str2 = crate::asc::util::jstr::from_utf16(&s[start_index..len]);
                dval = parse_decimal(&str2);
            } else {
                let sixteen = Decimal128::from_i32(16);
                dval = Decimal128::zero();
                for &c in &s[start_index..len] {
                    let new_digit = java_digit16(c);
                    if new_digit < 0 {
                        panic!("ArrayIndexOutOfBoundsException: {}", new_digit);
                    }
                    dval = sixteen.multiply(&dval).add(&Decimal128::from_i32(new_digit));
                }
            }
            if negative {
                if dval.equals(&Decimal128::zero()) {
                    dval = Decimal128::negzero();
                    is_int = false;
                } else {
                    dval = Decimal128::zero().subtract(&dval);
                }
            }
        }
        if !force && is_int {
            let ival = dval.long_value().expect("ArithmeticException");
            if (i32::MIN as i64..=i32::MAX as i64).contains(&ival) {
                *pp_type = Some(cx.int_type());
                return NumberConstant::Int(ival as i32);
            } else if (0..=MAXUINT).contains(&ival) {
                *pp_type = Some(cx.uint_type());
                return NumberConstant::Uint(ival);
            }
        }
        *pp_type = cx.decimal_type();
        if dval.is_nan() {
            dval = Decimal128::nan();
        } else if !dval.is_finite() {
            dval = if s[0] == '-' as u16 { Decimal128::neginfinity() } else { Decimal128::infinity() };
        }
        NumberConstant::Decimal(dval)
    }

    fn get_double_value_or_int(&self, cx: &Context, str: &str, force: bool, pp_type: &mut Option<TypeValue>) -> NumberConstant {
        let mut sum = 0f64;
        if str == "NaN" {
            *pp_type = Some(cx.double_type());
            return NumberConstant::Double(f64::NAN);
        } else if str == "Infinity" {
            *pp_type = Some(cx.double_type());
            return NumberConstant::Double(f64::INFINITY);
        } else if str == "-Infinity" {
            *pp_type = Some(cx.double_type());
            return NumberConstant::Double(f64::NEG_INFINITY);
        }

        let s: Vec<u16> = crate::asc::util::jstr::units(&str).collect();
        let mut is_int = false;
        if (str.contains('.') || str.contains('e') || str.contains('E') || s.len() > 10) && !(str.contains('x') || str.contains('X')) {
            sum = parse_double(str);
        } else {
            is_int = true;
            let mut base = 10;
            let mut start_index = 0usize;
            let mut len = s.len();
            let mut negative = false;
            if len > 1 {
                let c0 = s[0];
                if c0 == b'-' as u16 || c0 == b'+' as u16 {
                    start_index += 1;
                    if c0 == b'-' as u16 {
                        negative = true;
                    }
                }
            }
            if len > 2 && s[start_index] == b'0' as u16 {
                start_index += 1;
                let c0 = s[start_index];
                if c0 == b'x' as u16 || c0 == b'X' as u16 {
                    base = 16;
                    start_index += 1;
                }
            }
            while (start_index as i64) < len as i64 - 1 && s[start_index] == b'0' as u16 {
                start_index += 1;
            }
            let str2: Vec<u16> = s[start_index..len].to_vec();
            len -= start_index;
            start_index = 0;
            let mut new_digit = 0;
            let mut end;
            let mut round_up;
            match base {
                10 => {
                    end = start_index;
                    while end < len {
                        new_digit = str2[end] as i32 - '0' as i32;
                        sum = sum * base as f64 + new_digit as f64;
                        end += 1;
                    }
                    if sum > 2147483647.0 {
                        let t = crate::asc::util::jstr::from_utf16(&str2);
                        sum = parse_double(&t);
                    }
                }
                16 => {
                    end = start_index;
                    while end * 4 < 53 {
                        if end >= len {
                            break;
                        }
                        new_digit = java_digit16(str2[end]);
                        sum = sum * base as f64 + new_digit as f64;
                        end += 1;
                    }
                    if end < len {
                        let bit53 = new_digit & 0x1;
                        new_digit = java_digit16(str2[end]);
                        let bit54 = (new_digit & 0x8) >> 3;
                        round_up = (new_digit & 0x7) != 0;

                        let mut factor = base as f64;
                        loop {
                            end += 1;
                            if end >= len {
                                break;
                            }
                            new_digit = java_digit16(str2[end]);
                            round_up |= new_digit != 0;
                            factor *= base as f64;
                        }
                        round_up = (bit54 != 0) && ((bit53 != 0) || round_up);
                        sum += if round_up { 1.0 } else { 0.0 };
                        sum *= factor;
                    }
                }
                _ => {}
            }
            if negative {
                if sum == 0.0 {
                    sum = -0.0;
                    is_int = false;
                } else {
                    sum = -sum;
                }
            }
        }
        if !force && is_int {
            let ival = sum as i64;
            if (i32::MIN as i64..=i32::MAX as i64).contains(&ival) {
                *pp_type = Some(cx.int_type());
                return NumberConstant::Int(ival as i32);
            } else if cx.statics.borrow().es4_numerics && (0..=MAXUINT).contains(&ival) {
                *pp_type = Some(cx.uint_type());
                return NumberConstant::Uint(ival);
            }
        }
        *pp_type = Some(cx.double_type());
        let mut d = sum;
        if d.is_nan() {
            d = f64::NAN;
        } else if d.is_infinite() {
            d = if s[0] == b'-' as u16 { f64::NEG_INFINITY } else { f64::INFINITY };
        }
        NumberConstant::Double(d)
    }

    fn value_of_number_literal(&self, str: &str, pp_type: &mut Option<TypeValue>, number_usage: Option<&NumberUsage>) -> NumberConstant {
        let cxr = self.cx.clone();
        let cx = cxr.borrow();
        if !cx.statics.borrow().es4_numerics {
            return self.get_double_value_or_int(&cx, str, false, pp_type);
        }

        let mut ftype: Option<TypeValue> = None;
        let mut floating_ftype: Option<TypeValue>;
        let mut force_type;
        let int_t = Some(cx.int_type());
        let uint_t = Some(cx.uint_type());
        let double_t = Some(cx.double_type());
        let decimal_t = cx.decimal_type();

        match number_usage {
            None => {
                floating_ftype = double_t.clone();
                force_type = false;
            }
            Some(nu) => {
                floating_ftype = if nu.get_floating_usage() == NumberUsage::use_decimal { decimal_t.clone() } else { double_t.clone() };
                force_type = true;
                match nu.get_usage() {
                    NumberUsage::use_int => ftype = int_t.clone(),
                    NumberUsage::use_double => ftype = double_t.clone(),
                    NumberUsage::use_uint => ftype = uint_t.clone(),
                    NumberUsage::use_decimal => ftype = decimal_t.clone(),
                    _ => force_type = false,
                }
            }
        }

        let mut str = str.to_string();
        let last = crate::asc::util::jstr::char_at(&str, crate::asc::util::jstr::utf16_len(&str) - 1);
        let suffixed = match char::from_u32(last as u32).unwrap_or('\0') {
            'i' => {
                ftype = int_t.clone();
                true
            }
            'u' => {
                ftype = uint_t.clone();
                true
            }
            'm' => {
                ftype = decimal_t.clone();
                floating_ftype = decimal_t.clone();
                true
            }
            'd' if !(str.contains('x') || str.contains('X')) => {
                ftype = double_t.clone();
                floating_ftype = double_t.clone();
                true
            }
            _ => false,
        };
        if suffixed {
            force_type = true;
            str.pop();
        }

        if force_type {
            if tv_eq(&ftype, &double_t) {
                return self.get_double_value_or_int(&cx, &str, true, pp_type);
            } else if tv_eq(&ftype, &decimal_t) {
                return self.get_decimal_value_or_int(&cx, &str, true, pp_type);
            }
        }
        let mut result = if tv_eq(&floating_ftype, &decimal_t) {
            self.get_decimal_value_or_int(&cx, &str, false, pp_type)
        } else {
            self.get_double_value_or_int(&cx, &str, false, pp_type)
        };

        if force_type {
            *pp_type = ftype.clone();
            if tv_eq(&ftype, &int_t) {
                if !matches!(result, NumberConstant::Int(_)) {
                    result = NumberConstant::Int(result.int_value());
                }
            } else if !matches!(result, NumberConstant::Uint(_)) {
                result = NumberConstant::Uint(result.uint_value());
            }
        }
        result
    }

    pub fn get_unary_name(op_index: i32) -> &'static str {
        match op_index {
            UNARY_Put => "put",
            UNARY_Get => "get",
            UNARY_HasMoreNames => "HasMoreNamesOp",
            UNARY_NextName => "NextNameOp",
            UNARY_NextValue => "NextValueOp",
            UNARY_DeleteOp => "DeleteOp",
            UNARY_TypeofOp => "TypeofOp",
            UNARY_TypeofOp_I => "TypeofOp_I",
            UNARY_IncrementOp => "IncrementOp",
            UNARY_IncrementOp_I => "IncrementOp_I",
            UNARY_IncrementLocalOp => "IncrementLocalOp",
            UNARY_IncrementLocalOp_I => "IncrementLocalOp_I",
            UNARY_DecrementOp => "DecrementOp",
            UNARY_DecrementOp_I => "DecrementOp_I",
            UNARY_DecrementLocalOp => "DecrementLocalOp",
            UNARY_DecrementLocalOp_I => "DecrementLocalOp_I",
            UNARY_UnaryPlusOp => "UnaryPlusOp",
            UNARY_UnaryPlusOp_I => "UnaryPlusOp_I",
            UNARY_UnaryMinusOp => "UnaryMinusOp",
            UNARY_UnaryMinusOp_I => "UnaryMinusOp_I",
            UNARY_BitwiseNotOp => "BitwiseNotOp",
            UNARY_BitwiseNotOp_I => "BitwiseNotOp_I",
            UNARY_LogicalNotOp => "LogicalNotOp",
            UNARY_LogicalNotOp_B => "LogicalNotOp_B",
            UNARY_LogicalNotOp_I => "LogicalNotOp_I",
            UNARY_ToXMLString => "ToXMLString",
            UNARY_ToXMLAttrString => "ToXMLAttrString",
            UNARY_CheckFilterOp => "CheckFilterOperand",
            _ => "N/A",
        }
    }

    pub fn string_to_double(str: &str) -> f64 {
        let c = char::from_u32(crate::asc::util::jstr::char_at(str, 0) as u32).unwrap_or('\0');
        let sign = if c == '-' { -1.0 } else { 1.0 };
        let start_index = if c == '+' || c == '-' { 1 } else { 0 };
        let rest = &str[start_index..];
        let is_hex = rest.starts_with("0x") || rest.starts_with("0X");
        let is_float = !is_hex && (str.contains('.') || str.contains('e') || str.contains('E'));
        let is_octal = !is_hex && !is_float && rest.starts_with('0');
        let radix = if is_hex { 16 } else if is_octal { 8 } else { 10 };
        if radix != 10 {
            let sub = if is_hex { &rest[2..] } else { rest };
            i64::from_str_radix(sub, radix).map_or(0.0, |v| sign * v as f64)
        } else {
            java_parse_double(str).unwrap_or(0.0)
        }
    }

    pub fn stack(&mut self, size: i32) {
        self.cur_stack += size;
        if self.cur_stack > self.max_stack {
            self.max_stack = self.cur_stack;
        }
    }

    pub fn scope(&mut self, delta: i32) {
        self.cur_scope += delta;
        if self.cur_scope > self.max_scope {
            self.max_scope = self.cur_scope;
        }
    }

    pub fn save_stack_depth(&mut self) {
        self.stack_depth_stack.push(self.cur_stack);
        self.scope_depth_stack.push(self.cur_scope);
    }

    pub fn restore_stack_depth(&mut self) {
        self.cur_stack = il_remove_last(&mut self.stack_depth_stack);
        self.cur_scope = il_last(&self.scope_depth_stack);
        il_remove_last(&mut self.scope_depth_stack);
    }

    fn ip(&self) -> i32 {
        self.ab.code.as_ref().map_or(-1, |c| c.len() as i32)
    }

    pub fn show_line_number(&mut self) {
        if self.show_linenums {
            let (mut name, mut ln, mut col) = (None, 0, 0);
            self.get_origin_and_position(&mut name, &mut ln, &mut col);
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("[Ln {}]", ln));
        }
    }

    /// `getTypeOfNumberLiteral(LiteralNumberNode)`.
    pub fn get_type_of_number_literal(&mut self, node: &NodeRef) -> Option<TypeValue> {
        let (value, number_usage) = {
            let n = node.borrow();
            let l = n.as_literal_number().expect("ClassCastException");
            (l.value.clone(), l.number_usage.clone())
        };
        let mut nu_type = None;
        let numeric_value = self.value_of_number_literal(&value, &mut nu_type, number_usage.as_ref());
        let cxr = self.cx.clone();
        let cx = cxr.borrow();
        let ty = if value.contains('.') || ((value.contains('e') || value.contains('E')) && !(value.contains('x') || value.contains('X'))) {
            if cx.statics.borrow().es4_numerics && number_usage.as_ref().is_some_and(|nu| nu.get_floating_usage() == NumberUsage::use_decimal) {
                cx.decimal_type()
            } else {
                Some(cx.double_type())
            }
        } else {
            nu_type
        };
        let mut n = node.borrow_mut();
        let l = n.as_literal_number_mut().unwrap();
        l.numeric_value = Some(numeric_value);
        l.type_ = ty.clone();
        ty
    }

    fn exception_block_mut(&mut self, i: usize) -> &mut ExceptionBlock {
        &mut self.exception_blocks[i]
    }

    fn last_exception_block(&mut self) -> &mut ExceptionBlock {
        self.exception_blocks.last_mut().expect("NullPointerException")
    }

    /// The finally calls and scope pops of `Break` and `Continue`.
    fn unwind_to(&mut self, loop_index: i32, target_scope_index: i32) {
        let cxr = self.cx.clone();
        let reg_offset = self.get_register_offset(&cxr.borrow());
        let mut cur_scope_index = cxr.borrow().get_scope_depth() as i32 - 1;
        let mut i = self.exception_blocks.len();
        while i > 0 && self.exception_blocks[i - 1].loop_index >= loop_index {
            let scope_index = self.exception_blocks[i - 1].scope_index;
            while cur_scope_index > scope_index {
                self.op_popscope();
                let temp_reg = cxr.borrow().scope_at(cur_scope_index).builder().expect("NullPointerException").data().temp_reg;
                if temp_reg != -1 {
                    self.op_kill(temp_reg + reg_offset);
                }
                cur_scope_index -= 1;
            }

            if self.exception_blocks[i - 1].has_finally {
                self.op_label();
                let finally_index = self.exception_blocks[i - 1].finally_addrs.len() as i32;
                self.op_pushbyte(finally_index);
                self.op_jump();
                let ip = self.ip() - 3;
                self.exception_block_mut(i - 1).fixups.push(ip);
                self.op_label();
                self.pop();
                let ip = self.ip();
                self.exception_block_mut(i - 1).finally_addrs.push(ip);
                self.op_label();
            }
            i -= 1;
        }

        while cur_scope_index > target_scope_index {
            self.op_popscope();
            let temp_reg = cxr.borrow().scope_at(cur_scope_index).builder().expect("NullPointerException").data().temp_reg;
            if temp_reg != -1 {
                self.op_kill(temp_reg + reg_offset);
            }
            cur_scope_index -= 1;
        }
    }

    fn debug_slot(&mut self, name: &str, slot: i32, linenum: i32) {
        hook!(self.debug_slot(name, slot, linenum) else base_debug_slot)
    }

    pub fn base_debug_slot(&mut self, name: &str, slot: i32, linenum: i32) {
        let index = self.utf8(name);
        self.op_debug_local(index, slot, if linenum < 0 { 0 } else { linenum });
    }

    fn debug_file(&mut self, name: &str) {
        hook!(self.debug_file(name) else base_debug_file)
    }

    pub fn base_debug_file(&mut self, name: &str) {
        let index = self.utf8(name);
        self.op_debug_file(index);
    }

    fn debug_line(&mut self, pos: i32) {
        self.debug_line_op(pos);
    }

    fn debug_line_op(&mut self, linenum: i32) {
        hook!(self.debug_line_op(linenum) else base_debug_line_op)
    }

    /// `DebugLine(ByteList code, int linenum)`.
    pub fn base_debug_line_op(&mut self, linenum: i32) {
        bc!(self, "DebugLine {}", linenum);
        self.byte(OP_debugline);
        self.int(linenum);
    }

    fn get_version_from_old_metadata(&self, s: &str) -> i32 {
        match s {
            "9" => 660,
            "air1" => 661,
            "10" => 662,
            "air1.5" => 663,
            "air1.5.1" => 664,
            "10.0.32" => 665,
            "air1.5.2" => 666,
            "10.1" => 667,
            "airAthena" | "air2.0" => 668,
            "100" => 668,
            _ => {
                self.cx.borrow().internal_error(&format!("internal error: unrecognized value '{}' in 'Version' metadata", s));
                -1
            }
        }
    }

    fn api_versions_of_slot(&self, slot: &Slot, _name: &str, _ns: &ObjectValue) -> BTreeSet<i32> {
        let mut versions = BTreeSet::new();
        if let Some(list) = slot.get_metadata() {
            for md in &list {
                let id = md.id();
                if id.as_deref() == Some("API") {
                    for i in 0..md.count() {
                        let v = md.get_value_index(i);
                        match v.as_deref().and_then(|s| s.parse::<i32>().ok()) {
                            Some(v) => {
                                versions.insert(v);
                            }
                            None => self.cx.borrow().internal_error(&format!(
                                "internal error: unrecognized version '{}' in API metadata",
                                v.as_deref().unwrap_or("null")
                            )),
                        }
                    }
                } else if id.as_deref() == Some("Version") {
                    for i in 0..md.count() {
                        let v = self.get_version_from_old_metadata(md.get_value_index(i).as_deref().expect("NullPointerException"));
                        versions.insert(v);
                    }
                }
            }
        }
        if versions.is_empty() {
            versions.insert(0);
        }
        versions
    }

    fn push_trait(&mut self, traits: &mut Vec<ByteList>, qname_index: i32, kind: i32, id: i32, info: i32, other: i32, other2: i8, metadata: Option<&[i32]>) {
        traits.push(ByteCodeFactory::alloc_bytes());
        let mut t = traits.pop().unwrap();
        self.bf().trait_info(&mut t, qname_index, kind, id, info, other, other2, metadata);
        traits.push(t.clone());
        add_bytes_to_table(traits, t);
    }

    pub fn add_slot_trait(&mut self, obj: &ObjectValue, traits: &mut Vec<ByteList>, name: &str, quals: &Qualifiers) {
        let cxr = self.cx.clone();
        let cx = cxr.borrow();
        let var_offset = obj.builder().expect("NullPointerException").data().var_offset;

        let mut namespaces: IntList = Vec::with_capacity(quals.len());
        let mut prev_var_index = -1;
        let mut var_index;
        let mut _flags = 0;
        let mut prev_slot: Option<Slot> = None;

        let keys: Vec<ObjectValue> = quals.iter().map(|(k, _)| k.clone()).collect();
        let mut it = keys.into_iter();
        let mut qual_it = it.next();

        while let Some(first) = qual_it.clone() {
            namespaces.clear();
            var_index = -1;

            let mut ns = first;
            let mut slot_index = obj.get_slot_index(&cx, GET_TOKEN, name, Some(&ns));
            let mut slot = obj.get_slot(&cx, slot_index).expect("NullPointerException");
            var_index = slot.get_var_index() + var_offset + 1;

            if !declared_by_is(&slot, obj) {
                qual_it = it.next();
                continue;
            }

            loop {
                let ns_index = if self.doing_api_versioning {
                    let versions = self.api_versions_of_slot(&slot, name, &ns);
                    self.make_versioned_namespace_set(Some(&ns), &versions)
                } else {
                    self.add_namespace(&ns)
                };
                namespaces.push(ns_index);

                qual_it = it.next();
                let Some(next) = qual_it.clone() else {
                    break;
                };

                prev_var_index = var_index;
                prev_slot = Some(slot.clone());

                ns = next;
                slot_index = obj.get_slot_index(&cx, GET_TOKEN, name, Some(&ns));
                if slot_index <= 0 {
                    continue;
                }
                slot = obj.get_slot(&cx, slot_index).expect("NullPointerException");
                var_index = slot.get_var_index() + var_offset + 1;
                _flags &= if slot.is_final() { TRAIT_FLAG_final } else { 0 };
                _flags &= if slot.is_override() { TRAIT_FLAG_override } else { 0 };

                let ps = prev_slot.clone().expect("NullPointerException");
                if var_index < 0 || var_index != prev_var_index || !ov_opt_eq(&slot.declared_by(), &ps.declared_by()) {
                    var_index = prev_var_index;
                    slot = ps;
                    break;
                }
            }

            let name_index = self.utf8(name);
            let back = il_last(&namespaces);
            let qname_index = if self.doing_api_versioning {
                self.mn_multiname(name_index, back, false)
            } else {
                self.mn_qname(name_index, back, false)
            };

            let objval = slot.get_initializer_value();
            let mut value_index = 0;
            let mut value_kind = 0i8;
            let kind;
            let info;
            let class_builder = objval.as_ref().and_then(|o| o.builder()).filter(|b| b.is_class_builder());
            if let Some(b) = class_builder {
                kind = TRAIT_Class;
                info = self.get_class_info(&b.classname().expect("NullPointerException"));
            } else {
                kind = if slot.is_const() { TRAIT_Const } else { TRAIT_Var };
                if let Some(ov) = &objval {
                    value_index = self.get_value_index(ov);
                    value_kind = self.get_value_kind(ov);
                }
                let type_name = slot.get_type().expect("NullPointerException").get_name_cx(&cx);
                info = self.add_class_name(Some(&type_name));
            }

            let meta_data_indices = self.add_metadata_slot(Some(&slot));
            let id = if obj.can_early_bind() { var_index } else { 0 };
            self.push_trait(traits, qname_index, kind, id, info, value_index, value_kind, meta_data_indices.as_deref());
        }
    }

    pub fn add_metadata_slot(&mut self, slot: Option<&Slot>) -> Option<IntList> {
        if let Some(slot) = slot {
            let md = slot.get_metadata();
            return self.add_metadata(md.as_deref());
        }
        None
    }

    pub fn add_metadata(&mut self, metadata: Option<&[MetaData]>) -> Option<IntList> {
        hook!(self.add_metadata(metadata) else base_add_metadata)
    }

    pub fn base_add_metadata(&mut self, metadata: Option<&[MetaData]>) -> Option<IntList> {
        let mut meta_data_indices = None;
        if let Some(metadata) = metadata.filter(|m| !m.is_empty()) {
            let mut indices = Vec::with_capacity(metadata.len());
            for entry in metadata {
                let id = entry.id();
                let values = entry.values();
                indices.push(self.add_metadata_info(id.as_deref(), values.as_deref()));
            }
            meta_data_indices = Some(indices);
        }
        meta_data_indices
    }

    pub fn add_metadata_info(&mut self, id: Option<&str>, values: Option<&[Value]>) -> i32 {
        let mut meta_data_key: Option<String> = id.map(String::from);
        let concat = |k: &mut Option<String>, s: &str| {
            let mut v = k.take().unwrap_or_else(|| "null".to_string());
            v.push_str(s);
            *k = Some(v);
        };

        let meta_name_index = match id {
            None => 0,
            Some(id) => self.utf8(id),
        };
        let meta_values_count = values.map_or(0, |v| v.len() as i32);
        let mut meta_keys: IntList = Vec::with_capacity(meta_values_count as usize);
        let mut meta_values: IntList = Vec::with_capacity(meta_values_count as usize);
        for val in values.unwrap_or(&[]) {
            let (key, value) = match val {
                Value::KeylessValue(k) => (None, k.obj.clone()),
                Value::KeyValuePair(p) => (Some(p.key.clone()), p.obj.clone()),
                other => (None, other.to_string()),
            };
            match key {
                Some(key) => {
                    let k = self.utf8(&key);
                    meta_keys.push(k);
                    concat(&mut meta_data_key, &key);
                }
                None => meta_keys.push(0),
            }
            concat(&mut meta_data_key, &value);
            let v = self.utf8(&value);
            meta_values.push(v);
        }
        let meta_data_index = self.get_metadata_info(meta_data_key.as_deref());
        if self.ab.metadata[meta_data_index as usize].is_empty() {
            let mut bytes = std::mem::take(&mut self.ab.metadata[meta_data_index as usize]);
            self.bf().metadata_info(&mut bytes, meta_name_index, meta_values_count, &meta_keys, &meta_values, meta_data_index);
            self.ab.metadata[meta_data_index as usize] = bytes.clone();
            add_bytes_to_table(&mut self.ab.metadata, bytes);
        }
        meta_data_index
    }

    pub fn add_method_trait(&mut self, kind: i32, obj: &ObjectValue, traits: &mut Vec<ByteList>, name: &str, quals: &Qualifiers) {
        if name == "$construct" {
            return;
        }
        let cxr = self.cx.clone();
        let cx = cxr.borrow();

        let mut namespaces: IntList = Vec::with_capacity(quals.len());
        let keys: Vec<ObjectValue> = quals.iter().map(|(k, _)| k.clone()).collect();
        let is_interface = obj.type_().is_some_and(|t| t.is_interface());

        for ns in keys {
            let slot_kind = if kind == TRAIT_Setter { SET_TOKEN } else { GET_TOKEN };
            let slot_index = obj.get_slot_index(&cx, slot_kind, name, Some(&ns));
            let mut slot = obj.get_slot(&cx, slot_index);

            if kind == TRAIT_Method {
                let implicit_index = obj.get_implicit_index(&cx, slot_index, EMPTY_TOKEN);
                slot = obj.get_slot(&cx, implicit_index);
            }

            let slot = match slot {
                Some(s) if declared_by_is(&s, obj) && !s.get_method_name().is_empty() => s,
                _ => continue,
            };

            let slot_value = match slot.get_value() {
                Some(Value::Object(o)) => Some(o),
                _ => None,
            };
            let meta_data = slot.get_metadata();

            if !is_interface && ns.is_interface() {
                continue;
            }

            let is_override = slot.is_override();
            let mut flags = 0;
            flags |= if slot.is_final() { TRAIT_FLAG_final } else { 0 };
            flags |= if is_override { TRAIT_FLAG_override } else { 0 };

            let ns_index = if self.doing_api_versioning {
                let versions = self.api_versions_of_slot(&slot, name, &ns);
                self.make_versioned_namespace_set(Some(&ns), &versions)
            } else {
                self.add_namespace(&ns)
            };
            namespaces.push(ns_index);

            let method_info;
            let mut method_id = slot.get_method_id();
            if method_id >= 0 {
                method_info = self.get_method_info(&slot.get_method_name());
            } else if let Some(sv) = slot_value.as_ref().filter(|sv| sv.method_info() >= 0) {
                method_info = sv.method_info();
            } else {
                cx.internal_error("internal error");
                continue;
            }

            let name_index = self.utf8(name);
            let back = il_last(&namespaces);
            let qname_index = if self.doing_api_versioning {
                self.mn_multiname(name_index, back, false)
            } else {
                self.mn_qname(name_index, back, false)
            };

            let meta_data_indices = self.add_metadata(meta_data.as_deref());

            if method_info >= 0 {
                method_id = if method_id < 0 { 0 } else { method_id };
                let id = if obj.can_early_bind() { method_id } else { 0 };
                self.push_trait(traits, qname_index, kind, id, method_info, flags, 0, meta_data_indices.as_deref());
            } else {
                cx.internal_error("internal error: invalid method info");
            }
        }
    }

    fn finish_traits(&mut self, obj: &ObjectValue, traits: &mut Vec<ByteList>) {
        let names = obj.builder().expect("NullPointerException").get_names();
        let Some(names) = names else {
            return;
        };
        let cxr = self.cx.clone();
        if REMOVE_BUILDER_NAMES {
            let loop_label = ObjectValue::loop_label_namespace();
            let mut i = 0;
            loop {
                i = names.has_next(i);
                if i == -1 {
                    break;
                }
                let iu = i as usize;
                let mut q = Qualifiers::new();
                let slot_id = names.get_slot(iu);
                let name = names.get_name(iu);
                q.put(names.get_namespace(iu), 0);

                if slot_id >= 1 {
                    let s = obj.get_slot(&cxr.borrow(), slot_id);
                    match s {
                        Some(s) if !names.get_namespace(iu).ptr_eq(&loop_label) && declared_by_is(&s, obj) => {
                            if s.is_variable_slot() && names.get_type(iu) != Names::SET_NAMES {
                                self.add_slot_trait(obj, traits, &name, &q);
                            }
                            if s.is_method_slot() {
                                let mut method_kind = TRAIT_Method;
                                if s.is_getter() {
                                    method_kind = TRAIT_Getter;
                                }
                                if names.get_type(iu) == Names::SET_NAMES {
                                    method_kind = TRAIT_Setter;
                                }
                                self.add_method_trait(method_kind, obj, traits, &name, &q);
                            }
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
        } else {
            for (n, q) in names.entry_set(Names::VAR_NAMES) {
                self.add_slot_trait(obj, traits, &n, &q);
            }
            for (n, q) in names.entry_set(Names::METHOD_NAMES) {
                self.add_method_trait(TRAIT_Method, obj, traits, &n, &q);
            }
            for (n, q) in names.entry_set(Names::GET_NAMES) {
                self.add_method_trait(TRAIT_Getter, obj, traits, &n, &q);
            }
            for (n, q) in names.entry_set(Names::SET_NAMES) {
                self.add_method_trait(TRAIT_Setter, obj, traits, &n, &q);
            }
        }
    }

    fn print_namespace_set(&mut self, namespaces: &Namespaces) {
        let public = self.cx.borrow().public_namespace();
        let mut namespace_set = BTreeSet::new();
        for ns in namespaces.to_vec() {
            let mut ns_name = if ns.ptr_eq(&public) { "public".to_string() } else { ns.name() };
            match ns.get_namespace_kind() {
                NS_PROTECTED => ns_name.push_str("(protected)"),
                NS_PRIVATE => ns_name.push_str("(private)"),
                NS_INTERNAL => ns_name.push_str("(package-internal)"),
                NS_STATIC_PROTECTED => ns_name.push_str("(static-protected)"),
                _ => {}
            }
            namespace_set.insert(crate::asc::util::jhash::JStr(ns_name));
        }
        self.code_out.push_str(&format!("{} {{", namespace_set.len()));
        for s in &namespace_set {
            self.code_out.push_str(&format!(" {}", s.0));
        }
        self.code_out.push_str(" }");
    }

    fn get_property_common(&mut self, name: &str, qualifiers: Option<&Namespaces>, is_qualified: bool, is_attr: bool) -> i32 {
        let name_index = self.name_index(name);
        let qualifiers = qualifiers.expect("NullPointerException");
        if is_qualified && qualifiers.size() == 1 {
            let namespace = self.add_namespace(&qualifiers.last());
            self.mn_qname(name_index, namespace, is_attr)
        } else {
            let namespaces_set = self.make_namespace_set(qualifiers);
            self.mn_multiname(name_index, namespaces_set, is_attr)
        }
    }

    fn late_name(&mut self, is_qualified: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>, qualified_delta: i32, delta: i32) -> i32 {
        if is_qualified {
            let index = self.mn_rtqname_late(is_attr);
            self.stack(qualified_delta);
            index
        } else {
            let namespaces_set = self.make_namespace_set(used_def_namespaces.expect("NullPointerException"));
            let index = self.mn_multiname_late(namespaces_set, is_attr);
            self.stack(delta);
            index
        }
    }

    fn call_property_common(
        &mut self,
        name: &str,
        qualifiers: Option<&Namespaces>,
        size: i32,
        is_qualified: bool,
        is_super: bool,
        is_attr: bool,
        is_new: bool,
        is_lex: bool,
    ) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            if is_new {
                self.code_out.push_str("ConstructProperty ");
            } else if is_lex {
                self.code_out.push_str("CallPropLex ");
            } else {
                self.code_out.push_str("CallProperty ");
            }
            self.code_out.push_str(&format!("{} ", name));
            self.print_namespace_set(qualifiers.expect("NullPointerException"));
            self.code_out.push_str(&format!(" {} is_qualified={} is_super={} is_attr={}", size, is_qualified, is_super, is_attr));
        }

        self.flush_debug_info();

        self.last_ip = self.ip();

        let index = self.get_property_common(name, qualifiers, is_qualified, is_attr);

        if is_super {
            self.op_callsuper(index, size);
        } else if is_new {
            self.op_constructproperty(index, size);
        } else if is_lex {
            self.op_callproplex(index, size);
        } else {
            self.op_callproperty(index, size);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    pub fn apply_type_op(&mut self, size: i32) {
        self.stack(1 - size - 1);
        self.byte(OP_applytype);
        self.int(size);
    }

    pub fn get_register_offset(&self, cx: &Context) -> i32 {
        let scopes = cx.get_scopes();
        let mut i = scopes.len() as i32;
        loop {
            i -= 1;
            if i < 0 {
                break;
            }
            let builder = cx.scope_at(i).builder().expect("NullPointerException");
            if builder.has_register_offset() {
                return builder.data().reg_offset;
            }
        }
        -1
    }

    pub fn flush_debug_info(&mut self) {
        if self.emit_debug_info
            && self.debug_info.debug_linenum_dirty
            && self.debug_info.debug_linenum > -1
            && self.ab.code.is_some()
            && !self.debug_info.suppress_debug_method
        {
            if self.debug_info.debug_file_dirty {
                let file = self.debug_info.debug_file.clone().expect("NullPointerException");
                self.debug_file(&file);
                self.debug_info.debug_file_dirty = false;
            }
            let ln = self.debug_info.debug_linenum;
            self.debug_line(ln);
            self.debug_info.debug_linenum_dirty = false;
        }
    }

    pub fn api_versioning(&mut self) {
        self.doing_api_versioning = true;
    }

    pub fn reorder_main_script(&mut self) {
        let s = self.ab.scripts.remove(0);
        self.ab.scripts.push(s);
        let p = self.package_infos.remove(0);
        self.package_infos.push(p);
    }

    pub fn il_str(&self) -> String {
        let mut out = String::new();
        let bf = self.bytecode_factory.borrow();
        if bf.show_bytecode {
            out.push_str("constants----------------------------------------------------");
            out.push_str(&bf.cpool_out);
            out.push_str("\ndefinitions--------------------------------------------------");
            out.push_str(&bf.defns_out);
            out.push_str("\ncode---------------------------------------------------------");
        }
        out.push_str(&self.code_out);
        out
    }

    fn cleanup_string(bl: &[u8]) -> String {
        bl.iter().map(|&b| b as char).filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
    }

    fn cleanup_name(&self, bl: &[u8]) -> String {
        fn read_u32(b: &[u8], pos: &mut usize) -> i32 {
            let mut result = 0i64;
            for shift in (0..35).step_by(7) {
                let byte = *b.get(*pos).unwrap_or(&0) as i64;
                *pos += 1;
                result |= (byte & 0x7F) << shift;
                if byte & 0x80 == 0 {
                    break;
                }
            }
            result as i32
        }
        let mut pos = 0;
        if bl.first().copied().map(|b| b as i32) == Some(CONSTANT_Qname) {
            pos += 1;
            let ns_index = read_u32(bl, &mut pos);
            let nsbl = &self.ab.constant_ns_pool[ns_index as usize - 1];
            let nskind = nsbl[0] as i32;
            if nskind == CONSTANT_Namespace || nskind == CONSTANT_PackageNamespace {
                let mut npos = 1;
                let uri_index = read_u32(nsbl, &mut npos);
                let name_index = read_u32(bl, &mut pos);
                return format!(
                    "{}_{}",
                    Self::cleanup_string(&self.ab.constant_utf8_pool[uri_index as usize - 1]),
                    Self::cleanup_string(&self.ab.constant_utf8_pool[name_index as usize - 1])
                );
            }
        }
        String::new()
    }

    pub fn dump_cpool_vars(&mut self) {
        let mut done = HashSet::new();
        let mut lines = Vec::new();
        for (i, bl) in self.ab.constant_utf8_pool.iter().enumerate() {
            let s = Self::cleanup_string(bl);
            if !s.is_empty() && done.insert(s.clone()) {
                lines.push(format!("const int abcstr_{} = {};", s, i + 1));
            }
        }
        for (i, bl) in self.ab.constant_mn_pool.iter().enumerate() {
            let s = self.cleanup_name(bl);
            if !s.is_empty() {
                lines.push(format!("const int abcname_{} = {};", s, i + 1));
            }
        }
        for l in lines {
            self.header_println(&l);
        }
    }

    fn dump_package_infos(&mut self) {
        for i in 0..self.package_infos.len() {
            let name = self.package_infos[i].clone();
            let package_info = i as i32;
            if crate::asc::util::jstr::utf16_len(&name) > 1 {
                let line = format!("const int abcpackage_{} = {};", const_name(&name), package_info);
                self.header_println(&line);
                if package_info >= self.native_package_count {
                    self.native_package_count = package_info + 1;
                }
            }
        }
    }

    pub fn header_str(&mut self) -> Option<String> {
        self.dump_package_infos();
        if !self.header_out.is_empty() {
            return Some(self.header_out.clone());
        }
        None
    }

    // Virtual machine instructions.

    simple_ops! {
        op_bkpt(Some(0), "Bkpt", OP_bkpt);
        op_nop(Some(0), "Nop", OP_nop);
        op_throw(Some(-1), "Throw", OP_throw);
        op_label(Some(0), "Label", OP_label);
        op_lookupswitch(Some(-1), "Lookupswitch", OP_lookupswitch);
        op_pushnull(Some(1), "Pushnull", OP_pushnull);
        op_pushundefined(Some(1), "Pushundefined", OP_pushundefined);
        op_pushuninitialized(Some(1), "Pushuninitialized ", OP_pushuninitialized);
        op_pushtrue(Some(1), "Pushtrue", OP_pushtrue);
        op_pushfalse(Some(1), "Pushfalse", OP_pushfalse);
        op_pushnan(Some(1), "Pushnan", OP_pushnan);
        op_pop(Some(-1), "Pop", OP_pop);
        op_dup(Some(1), "Dup", OP_dup);
        op_swap(Some(0), "Swap", OP_swap);
        op_returnvoid(Some(0), "Returnvoid", OP_returnvoid);
        op_returnvalue(Some(-1), "Returnvalue", OP_returnvalue);
        op_newactivation(Some(1), "Newactivation", OP_newactivation);
        op_getglobalscope(Some(1), "Getglobalscope", OP_getglobalscope);
        op_nextname(Some(-1), "Nextname", OP_nextname);
        op_nextvalue(Some(-1), "Nextvalue", OP_nextvalue);
        op_descendants_late(Some(-1), "Descendants", OP_getdescendants);
        op_hasnext(Some(-1), "Hasnext", OP_hasnext);
        op_convert_s(Some(0), "Convert.s", OP_convert_s);
        op_esc_xelem(Some(0), "Esc.xelem", OP_esc_xelem);
        op_esc_xattr(Some(0), "Esc.xattr", OP_esc_xattr);
        op_check_filter(Some(0), "CheckFilter", OP_checkfilter);
        op_convert_i(Some(0), "Convert.i", OP_convert_i);
        op_convert_u(Some(0), "Convert.u", OP_convert_u);
        op_convert_d(Some(0), "Convert.d", OP_convert_d);
        op_convert_m(Some(0), "Convert.m", OP_convert_m);
        op_convert_b(Some(0), "Convert.b", OP_convert_b);
        op_convert_o(Some(0), "Convert.o", OP_convert_o);
        op_coerce_a(Some(0), "Coerce.o", OP_coerce_a);
        op_coerce_s(Some(0), "Coerce.s", OP_coerce_s);
        op_astypelate(Some(-1), "Astypelate", OP_astypelate);
        op_negate(Some(0), "Negate", OP_negate);
        op_negate_i(Some(0), "Negate.i", OP_negate_i);
        op_increment(Some(0), "Increment", OP_increment);
        op_increment_i(Some(0), "Increment.i", OP_increment_i);
        op_decrement(Some(0), "Decrement", OP_decrement);
        op_decrement_i(Some(0), "Decrement.i", OP_decrement_i);
        op_typeof(Some(0), "Typeof", OP_typeof);
        op_not(Some(0), "Not", OP_not);
        op_add(Some(-1), "Add", OP_add);
        op_add_i(Some(-1), "Add.i", OP_add_i);
        op_subtract(Some(-1), "Subtract", OP_subtract);
        op_subtract_i(Some(-1), "Subtract.i", OP_subtract_i);
        op_multiply(Some(-1), "Multiply", OP_multiply);
        op_multiply_i(Some(-1), "Multiply.i", OP_multiply_i);
        op_divide(Some(-1), "Divide", OP_divide);
        op_modulo(Some(-1), "Modulo", OP_modulo);
        op_lshift(Some(-1), "Lshift", OP_lshift);
        op_rshift(Some(-1), "Rshift", OP_rshift);
        op_urshift(Some(-1), "Urshift", OP_urshift);
        op_bitand(Some(-1), "Bitand", OP_bitand);
        op_bitor(Some(-1), "Bitor", OP_bitor);
        op_bitxor(Some(-1), "Bitxor", OP_bitxor);
        op_equals(Some(-1), "Equals", OP_equals);
        op_strictequals(Some(-1), "Strictequals", OP_strictequals);
        op_lessthan(Some(-1), "Lessthan", OP_lessthan);
        op_lessequals(Some(-1), "Lessequals", OP_lessequals);
        op_greaterthan(Some(-1), "Greaterthan", OP_greaterthan);
        op_greaterequals(Some(-1), "Greaterequals", OP_greaterequals);
        op_instanceof(Some(-1), "Instanceof", OP_instanceof);
        op_in(Some(-1), "In", OP_in);
        op_istypelate(Some(-1), "Istypelate", OP_istypelate);
        op_bitnot(Some(0), "Bitnot", OP_bitnot);
        op_dxnslate(Some(-1), "Dxnslate", OP_dxnslate);
    }

    int_ops! {
        op_pushstring(Some(1), "Pushstring", OP_pushstring);
        op_pushnamespace(Some(1), "Pushnamespace", OP_pushnamespace);
        op_pushuint(Some(1), "Pushuint", OP_pushuint);
        op_pushdouble(Some(1), "Pushdouble", OP_pushdouble);
        op_pushdecimal(Some(1), "Pushdouble", OP_pushdecimal);
        op_pushint(Some(1), "Pushint", OP_pushint);
        op_pushshort(Some(1), "Pushshort", OP_pushshort);
        op_newfunction(Some(1), "Newfunction", OP_newfunction);
        op_newclass(Some(0), "Newclass", OP_newclass);
        op_newcatch(Some(1), "Newcatch", OP_newcatch);
        op_getproperty(Some(0), "Getproperty", OP_getproperty);
        op_getsuper(Some(0), "Getsuper", OP_getsuper);
        op_descendants(Some(0), "Descendants", OP_getdescendants);
        op_deletedescendants(Some(0), "Deletedescendants", OP_deldescendants);
        op_delproperty(Some(0), "Delproperty", OP_deleteproperty);
        op_setproperty(None::<i32>, "Setproperty", OP_setproperty);
        op_initproperty(None::<i32>, "Initproperty", OP_initproperty);
        op_setsuper(None::<i32>, "Setsuper", OP_setsuper);
        op_coerce(Some(0), "Coerce", OP_coerce);
        op_astype(Some(0), "Astype", OP_astype);
        op_inclocal(Some(0), "Inclocal", OP_inclocal);
        op_inclocal_i(Some(0), "Inclocal.i", OP_inclocal_i);
        op_declocal(Some(0), "Declocal", OP_declocal);
        op_declocal_i(Some(0), "Declocal.i", OP_declocal_i);
        op_istype(Some(0), "Istype", OP_istype);
        op_finddef(Some(1), "Finddef", OP_finddef);
        op_findpropstrict(Some(1), "Findpropstrict", OP_findpropstrict);
        op_findproperty(Some(1), "Findproperty", OP_findproperty);
        op_getslot(Some(0), "Getslot", OP_getslot);
        op_setslot(Some(-2), "Setslot", OP_setslot);
        op_debug_file(None::<i32>, "DebugFile", OP_debugfile);
        op_dxns(Some(1), "Dxns", OP_dxns);
        op_kill(None::<i32>, "Kill", OP_kill);
    }

    param_ops! {
        op_convert_m_p(0, "Convert.m_dp(0x", OP_convert_m_p);
        op_negate_p(0, "Negate_p(", OP_negate_p);
        op_increment_p(0, "Increment_p(0x", OP_increment_p);
        op_decrement_p(0, "Decrement_p(0x", OP_decrement_p);
        op_add_p(-1, "Add_p(0x", OP_add_p);
        op_subtract_p(-1, "Subtract_p(0x", OP_subtract_p);
        op_multiply_p(-1, "Multiply_p(0x", OP_multiply_p);
        op_divide_p(-1, "Divide_dp(0x", OP_divide_p);
        op_modulo_p(-1, "Modulo_p(0x", OP_modulo_p);
    }

    if_ops! {
        op_iffalse(-1, "Iffalse", OP_iffalse);
        op_ifeq(-2, "Ifeq", OP_ifeq);
        op_ifne(-2, "Ifne", OP_ifne);
        op_iflt(-2, "Iflt", OP_iflt);
        op_ifle(-2, "Ifle", OP_ifle);
        op_ifgt(-2, "Ifgt", OP_ifgt);
        op_ifge(-2, "Ifge", OP_ifge);
        op_ifstricteq(-2, "Ifstricteq", OP_ifstricteq);
        op_ifstrictne(-2, "Ifstrictne", OP_ifstrictne);
        op_ifnlt(-2, "Ifnlt", OP_ifnlt);
        op_ifnle(-2, "Ifnle", OP_ifnle);
        op_ifngt(-2, "Ifngt", OP_ifngt);
        op_ifnge(-2, "Ifnge", OP_ifnge);
    }

    pub fn op_iftrue(&mut self, offset: i32) {
        self.stack(-1);
        bc!(self, "Iftrue {}", offset);
        self.byte(OP_iftrue);
        self.int24(offset);
    }

    pub fn op_jump(&mut self) {
        self.stack(0);
        bc!(self, "Jump");
        self.byte(OP_jump);
        self.int24(0);
    }

    pub fn op_pushbyte(&mut self, n: i32) {
        self.stack(1);
        bc!(self, "Pushbyte {}", n);
        self.byte(OP_pushbyte);
        self.byte(n);
    }

    pub fn op_callfunction(&mut self, isnew: bool, size: i32) {
        if !isnew {
            bc!(self, "Call {}", size);
            self.stack(1 - size - 2);
            self.byte(OP_call);
        } else {
            bc!(self, "Construct {}", size);
            self.stack(1 - size - 1);
            self.byte(OP_construct);
        }
        self.int(size);
    }

    fn index_size_op(&mut self, label: &str, op: i32, index: i32, size: i32) {
        self.stack(1 - size - 1);
        bc!(self, "{} {} {}", label, index, size);
        self.byte(op);
        self.int(index);
        self.int(size);
    }

    pub fn op_callstatic(&mut self, index: i32, size: i32) {
        self.index_size_op("Callstatic", OP_callstatic, index, size);
    }

    pub fn op_callmethod(&mut self, index: i32, size: i32) {
        self.index_size_op("Callmethod", OP_callmethod, index, size);
    }

    pub fn op_callsuper(&mut self, index: i32, size: i32) {
        self.index_size_op("Callsuper", OP_callsuper, index, size);
    }

    pub fn op_callproperty(&mut self, index: i32, size: i32) {
        self.index_size_op("Callproperty", OP_callproperty, index, size);
    }

    pub fn op_callproplex(&mut self, index: i32, size: i32) {
        self.index_size_op("Callproplex", OP_callproplex, index, size);
    }

    pub fn op_constructproperty(&mut self, index: i32, size: i32) {
        self.index_size_op("Constructproperty", OP_constructprop, index, size);
    }

    pub fn op_constructsuper(&mut self, size: i32) {
        self.stack(-size - 1);
        bc!(self, "Constructsuper {}", size);
        self.byte(OP_constructsuper);
        self.int(size);
    }

    pub fn op_newobject(&mut self, size: i32) {
        self.stack(1 - size * 2);
        bc!(self, "Newobject {}", size);
        self.byte(OP_newobject);
        self.int(size);
    }

    pub fn op_newarray(&mut self, size: i32) {
        self.stack(1 - size);
        bc!(self, "Newarray {}", size);
        self.byte(OP_newarray);
        self.int(size);
    }

    pub fn op_getlocal(&mut self, index: i32) {
        self.stack(1);
        if index <= 3 {
            bc!(self, "Getlocal{}", index);
            self.byte(OP_getlocal0 + index);
        } else {
            bc!(self, "Getlocal {}", index);
            self.byte(OP_getlocal);
            self.int(index);
        }
    }

    pub fn op_setlocal(&mut self, index: i32) {
        self.stack(-1);
        if index <= 3 {
            bc!(self, "Setlocal{}", index);
            self.byte(OP_setlocal0 + index);
        } else {
            bc!(self, "Setlocal {}", index);
            self.byte(OP_setlocal);
            self.int(index);
        }
    }

    pub fn op_getscopeobject(&mut self, index: i32) {
        self.stack(1);
        bc!(self, "Getscopeobject {}", index);
        self.byte(OP_getscopeobject);
        self.byte(index);
    }

    pub fn op_pushwith(&mut self) {
        self.stack(-1);
        self.scope(1);
        bc!(self, "Pushwith");
        self.byte(OP_pushwith);
    }

    pub fn op_pushscope(&mut self) {
        self.stack(-1);
        self.scope(1);
        bc!(self, "Pushscope");
        self.byte(OP_pushscope);
    }

    pub fn op_popscope(&mut self) {
        self.stack(0);
        self.scope(-1);
        bc!(self, "Popscope");
        self.byte(OP_popscope);
    }

    pub fn op_hasnext2(&mut self, object_register: i32, index_register: i32) {
        self.stack(1);
        bc!(self, "Hasnext2 {} {}", object_register, index_register);
        self.byte(OP_hasnext2);
        self.int(object_register);
        self.int(index_register);
    }

    pub fn op_inclocal_p(&mut self, index: i32, decimal_params: i32) {
        self.stack(0);
        bc!(self, "Inclocal_p(0x{}) {}", hex(decimal_params), index);
        self.byte(OP_inclocal_p);
        self.int(decimal_params);
        self.int(index);
    }

    pub fn op_declocal_p(&mut self, index: i32, decimal_params: i32) {
        self.stack(0);
        bc!(self, "Declocal_p(0x{}) {}", hex(decimal_params), index);
        self.byte(OP_declocal_p);
        self.int(decimal_params);
        self.int(index);
    }

    pub fn op_debug_local(&mut self, index: i32, slot: i32, linenum: i32) {
        bc!(self, "Debug DI_SLOT {} slot {} line {}", index, slot, linenum);
        self.byte(OP_debug);
        self.byte(DI_LOCAL);
        self.int(index);
        self.byte(slot);
        self.int(linenum);
    }

    fn if_kind_str(kind: i32) -> &'static str {
        match kind {
            IF_false => "false",
            IF_true => "true",
            IF_lt => "lt",
            IF_le => "le",
            IF_gt => "gt",
            IF_ge => "ge",
            IF_eq => "eq",
            IF_ne => "ne",
            IF_stricteq => "stricteq",
            IF_strictne => "strictne",
            IF_nlt => "nlt",
            IF_nle => "nle",
            IF_ngt => "ngt",
            IF_nge => "nge",
            _ => "",
        }
    }

    fn if_op(&mut self, kind: i32, offset: i32) {
        match kind {
            IF_false => self.op_iffalse(offset),
            IF_true => self.op_iftrue(offset),
            IF_lt => self.op_iflt(offset),
            IF_le => self.op_ifle(offset),
            IF_gt => self.op_ifgt(offset),
            IF_ge => self.op_ifge(offset),
            IF_eq => self.op_ifeq(offset),
            IF_ne => self.op_ifne(offset),
            IF_stricteq => self.op_ifstricteq(offset),
            IF_strictne => self.op_ifstrictne(offset),
            IF_nlt => self.op_ifnlt(offset),
            IF_nle => self.op_ifnle(offset),
            IF_ngt => self.op_ifngt(offset),
            IF_nge => self.op_ifnge(offset),
            _ => {}
        }
    }

    fn patch_jump(&mut self, label: &str, index: i32, target: i32) {
        let offset = target - index + 1 - 4;
        if self.show_bytecode() {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("      {}@{} <- {}", label, index - 1, offset));
        }
        self.patch24(index, offset);
    }
}

impl Emitter for ActionBlockEmitter {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }

    fn get_method_info(&mut self, name: &str) -> i32 {
        if let Some(&id) = self.method_infos_map.get(name) {
            return id;
        }
        let id = self.method_infos.len() as i32;
        self.method_infos_map.insert(name.to_string(), id);
        self.method_infos.push(name.to_string());
        self.ab.methods.push(ByteCodeFactory::alloc_bytes());
        if self.method_infos.len() != self.ab.methods.len() {
            self.cx.borrow().internal_error("internal error: internal method name and info out of sync");
        }
        id
    }

    fn get_method_id(&mut self, name: &str, _unused: &Namespaces) -> i32 {
        if let Some(i) = self.global_method_names.iter().position(|n| n == name) {
            return i as i32 + 1;
        }
        self.global_method_names.push(name.to_string());
        self.global_method_names.len() as i32
    }

    fn emit(&mut self, bytes: &mut ByteList) {
        let ab = &self.ab;
        let pooled = |p: &Vec<ByteList>| if p.is_empty() { 0 } else { p.len() as i32 + 1 };
        self.bytecode_factory.borrow_mut().action_block(
            bytes,
            ab.minor_version,
            ab.major_version,
            pooled(&ab.constant_int_pool),
            &ab.constant_int_pool,
            pooled(&ab.constant_uint_pool),
            &ab.constant_uint_pool,
            pooled(&ab.constant_double_pool),
            &ab.constant_double_pool,
            pooled(&ab.constant_decimal_pool),
            &ab.constant_decimal_pool,
            pooled(&ab.constant_utf8_pool),
            &ab.constant_utf8_pool,
            pooled(&ab.constant_mn_pool),
            &ab.constant_mn_pool,
            pooled(&ab.constant_nss_pool),
            &ab.constant_nss_pool,
            pooled(&ab.constant_ns_pool),
            &ab.constant_ns_pool,
            ab.methods.len() as i32,
            &ab.methods,
            ab.metadata.len() as i32,
            &ab.metadata,
            ab.classes.len() as i32,
            &ab.instances,
            &ab.classes,
            ab.scripts.len() as i32,
            &ab.scripts,
            ab.bodies.len() as i32,
            &ab.bodies,
        );
    }

    fn get_value_of_number_literal(&mut self, str: &str, type_: &mut Option<TypeValue>, usage: Option<&NumberUsage>) -> Option<NumberConstant> {
        Some(self.value_of_number_literal(str, type_, usage))
    }

    fn set_position(&mut self, ln_num: i32, col_pos: i32, pos: i32) {
        emitter::base::set_position(self, ln_num, col_pos, pos);
        if self.emit_debug_info && pos > 0 && self.debug_info.debug_linenum != ln_num {
            self.debug_info.debug_linenum_dirty = true;
            self.debug_info.debug_linenum = ln_num;
        }
    }

    fn clear_position_info(&mut self) {
        if self.emit_debug_info {
            self.debug_info.debug_linenum_dirty = false;
            self.debug_info.debug_linenum = -1;
        }
    }

    fn set_origin(&mut self, origin: &str) {
        emitter::base::set_origin(self, origin);
        if self.emit_debug_info {
            self.debug_info.debug_file = Some(origin.to_string());
            self.debug_info.debug_file_dirty = true;
        }
    }

    fn define_slot_variable(&mut self, _cx: &ContextRef, name: &str, _func_name: &str, _pos: i32, _type: Option<&TypeInfo>, slot_index: i32) {
        if self.emit_debug_info {
            let ln = self.debug_info.debug_linenum;
            self.debug_slot(name, slot_index, ln);
        }
    }

    fn start_method_empty(&mut self) {
        self.start_method("", 0, 0, 0, false, 0);
    }

    fn start_method(&mut self, name: &str, param_count: i32, local_count: i32, temp_count: i32, needs_activation: bool, needs_arguments: i32) {
        ci!(self, "// ++StartMethod {}", name);
        self.cur_stack = 0;
        self.max_stack = 0;
        self.cur_scope = 0;
        self.max_scope = 0;
        self.max_params = param_count + 1;
        if !needs_activation {
            self.max_locals = param_count + local_count + temp_count + 1;
        } else {
            self.max_locals = param_count + temp_count + if needs_arguments != 0 { 1 } else { 0 } + 1;
        }
        self.cur_locals = self.max_locals;
        self.ab.code = Some(ByteCodeFactory::alloc_bytes());
        self.last_in = IKIND_other;

        if name == "$init" {
            self.method_infos_map.remove(name);
        }

        self.get_method_info(name);

        self.debug_info.debug_file_dirty = true;
        self.debug_info.debug_linenum_dirty = true;
        self.debug_info.suppress_debug_method = name.contains("$iinit") || name.contains("$cinit");
        self.sets_dxns = false;
    }

    fn finish_method(
        &mut self,
        cx: &ContextRef,
        name: &str,
        type_: Option<&TypeInfo>,
        _types: Option<&[TypeInfo]>,
        activation: Option<&ObjectValue>,
        needs_arguments: i32,
        scope_depth: i32,
        debug_name: &str,
        is_native: bool,
        is_interface: bool,
        arg_names: Option<&[String]>,
    ) -> i32 {
        ci!(self, "// --FinishMethod {} {}", name, debug_name);
        self.show_line_number();

        let cx = cx.borrow();
        let method_info = self.get_method_info(name);

        let mut traits: Vec<ByteList> = Vec::new();

        if let Some(obj) = activation {
            self.finish_traits(obj, &mut traits);
        }

        let mut flags = 0;
        flags |= needs_arguments;
        flags |= if activation.is_some() { METHOD_Activation } else { 0 };

        let debug_name_index = self.utf8(if self.emit_code_hints { debug_name } else { "" });

        if is_native {
            flags |= METHOD_Native;
        }

        if self.sets_dxns {
            flags |= METHOD_Setsdxns;
        }

        let return_type = match type_ {
            Some(t) => self.add_class_name(Some(&t.get_name_cx(&cx))),
            None => 0,
        };

        let mut param_types: Option<IntList> = None;
        let mut param_values: Option<IntList> = None;
        let mut param_kinds: Option<ByteList> = None;

        {
            let obj = cx.scope().expect("NullPointerException");
            if let Some(slots) = obj.slots() {
                let mut types: IntList = Vec::with_capacity(slots.len());
                let mut values: IntList = Vec::with_capacity(slots.len());
                let mut kinds: ByteList = Vec::with_capacity(slots.len());

                let mut it = slots.iter();
                let mut value_required = false;
                let mut i = 1;
                while i < self.max_params {
                    let Some(slot) = it.next() else {
                        break;
                    };
                    let type_name = slot.get_type().expect("NullPointerException").get_name_cx(&cx);
                    let type_index = self.add_class_name(Some(&type_name));
                    types.push(type_index);

                    let mut value_index = 0;
                    let mut value_kind = 0;
                    if let Some(iv) = slot.get_initializer_value() {
                        value_required = true;
                        let value = iv.to_string();
                        let number_usage = iv.get_number_usage();
                        let dvt = &iv.type_().and_then(|t| t.get_type_value());
                        let es4 = cx.statics.borrow().es4_numerics;
                        if tv_eq(dvt, &Some(cx.boolean_type())) {
                            value_index = if value == "true" { CONSTANT_True } else { CONSTANT_False };
                            value_kind = value_index;
                        } else if tv_eq(dvt, &Some(cx.string_type())) {
                            value_index = self.utf8(&value);
                            value_kind = CONSTANT_Utf8;
                        } else if tv_eq(dvt, &Some(cx.int_type())) {
                            let v = self.value_of_number_literal(&value, &mut None, number_usage.as_ref()).int_value();
                            value_index = self.int_const(v);
                            value_kind = CONSTANT_Integer;
                        } else if tv_eq(dvt, &Some(cx.double_type())) || (cx.abc_version(TARGET_AVM1) && tv_eq(dvt, &Some(cx.uint_type()))) {
                            let v = self.value_of_number_literal(&value, &mut None, number_usage.as_ref()).double_value();
                            value_index = self.double_const(v);
                            value_kind = CONSTANT_Double;
                        } else if tv_eq(dvt, &Some(cx.uint_type())) {
                            let v = self.value_of_number_literal(&value, &mut None, number_usage.as_ref()).uint_value();
                            value_index = self.uint_const(v);
                            value_kind = CONSTANT_UInteger;
                        } else if es4 && tv_eq(dvt, &cx.decimal_type()) {
                            let v = self.value_of_number_literal(&value, &mut None, number_usage.as_ref()).decimal_value();
                            value_index = self.decimal_const(&v);
                            value_kind = CONSTANT_Decimal;
                        } else if tv_eq(dvt, &Some(cx.null_type())) {
                            value_index = CONSTANT_Null;
                            value_kind = CONSTANT_Null;
                        } else if tv_eq(dvt, &Some(cx.void_type())) {
                            value_index = 0;
                            value_kind = 0;
                        } else if iv.is_namespace_value() {
                            value_index = self.add_namespace(&iv);
                            value_kind = CONSTANT_Namespace;
                        }
                    }
                    if value_required {
                        values.push(value_index);
                        kinds.push(value_kind as u8);
                    }
                    i += 1;
                }
                if value_required {
                    flags |= METHOD_HasOptional;
                }
                param_types = Some(types);
                param_values = Some(values);
                param_kinds = Some(kinds);
            }
        }

        let mut param_names: Option<IntList> = None;
        if self.emit_code_hints {
            if let Some(arg_names) = arg_names {
                flags |= METHOD_HasParamNames;
                let mut names = Vec::with_capacity(arg_names.len());
                for a in arg_names {
                    names.push(self.utf8(a));
                }
                param_names = Some(names);
            }
        }

        {
            let mut bytes = std::mem::take(&mut self.ab.methods[method_info as usize]);
            self.bf().method_info(
                &mut bytes,
                self.max_params - 1,
                return_type,
                param_types.as_deref(),
                param_values.as_deref(),
                param_kinds.as_deref().unwrap_or(&[]),
                param_names.as_deref(),
                debug_name_index,
                flags,
                method_info,
            );
            self.ab.methods[method_info as usize] = bytes;
        }

        if !is_native {
            if !is_interface {
                let mut body = ByteCodeFactory::alloc_bytes();
                {
                    let code = self.ab.code.as_ref().expect("NullPointerException");
                    self.bytecode_factory.borrow_mut().method_body(
                        &mut body,
                        self.max_stack as i16 as i32,
                        self.max_locals,
                        scope_depth,
                        scope_depth + self.max_scope,
                        code.len() as i32,
                        code,
                        self.ab.exception_count,
                        &self.ab.exception_table,
                        &traits,
                        method_info,
                    );
                }
                self.ab.add_body(body);
            }
        } else {
            let line = format!("const int {} = {};", const_name(debug_name), method_info);
            self.header_println(&line);
            if method_info >= self.native_method_count {
                self.native_method_count = method_info + 1;
            }
        }

        self.cur_locals = 0;
        self.max_locals = 0;

        self.ab.exception_count = 0;
        self.ab.exception_table.clear();

        drop(cx);
        self.clear_position_info();

        method_info
    }

    fn start_class(&mut self, filename: &str) {
        ci!(self, "// ++StartClass {}", filename);
        self.show_line_number();
    }

    fn finish_class(&mut self, cx: &ContextRef, name: &QName, basename: Option<&QName>, is_dynamic: bool, is_final: bool, is_interface: bool, is_nullable: bool) {
        let cx = cx.borrow();
        let mut static_traits: Vec<ByteList> = Vec::new();
        let mut instance_traits: Vec<ByteList> = Vec::new();
        let mut interfaces: IntList = Vec::new();
        let scope_depth = cx.get_scopes().len() as i32;

        let mut protected_namespace: Option<ObjectValue> = None;

        if !is_interface {
            let obj = cx.scope_at(scope_depth - 2);
            let bui = obj.builder().expect("NullPointerException");
            protected_namespace = bui.class_data().protected_namespace.clone();
            self.finish_traits(&obj, &mut static_traits);
        }

        {
            let obj = cx.scope_at(scope_depth - 1);
            let bui = obj.builder().filter(|b| b.is_instance_builder()).expect("NullPointerException");
            let refs = bui.instance_data().interface_refs.clone();
            for r in &refs {
                let nss = r.get_immutable_namespaces();
                interfaces.push(self.make_multiname(&r.name(), &nss));
            }
            self.finish_traits(&obj, &mut instance_traits);
        }

        let qname_index = self.add_class_name(Some(name));
        let base_index = match basename {
            None => 0,
            Some(b) => self.add_class_name(Some(b)),
        };
        let iinit_info;

        {
            let obj = cx.scope().expect("NullPointerException");
            let mut slot_id = obj.get_slot_index(&cx, GET_TOKEN, "$construct", Some(&cx.public_namespace()));
            slot_id = obj.get_implicit_index(&cx, slot_id, EMPTY_TOKEN);
            let slot = obj.get_slot(&cx, slot_id).expect("NullPointerException");
            iinit_info = self.get_method_info(&slot.get_method_name());
        }

        let class_info = self.get_class_info(name);

        let mut flags = 0;
        if !is_dynamic {
            flags |= CLASS_FLAG_sealed;
        }
        if is_final {
            flags |= CLASS_FLAG_final;
        }
        if is_interface {
            flags |= CLASS_FLAG_interface;
        }
        if !is_nullable {
            flags |= CLASS_FLAG_non_nullable;
        }

        let protected_index = match &protected_namespace {
            Some(ns) => self.add_namespace(ns),
            None => 0,
        };
        if protected_index != 0 {
            flags |= CLASS_FLAG_protected;
        }

        let ci = class_info as usize;
        if ci >= self.ab.instances.len() {
            panic!("IndexOutOfBoundsException: {}", ci);
        }
        let mut inst = std::mem::take(&mut self.ab.instances[ci]);
        self.bf().instance_info(
            &mut inst,
            qname_index,
            base_index,
            flags,
            protected_index,
            interfaces.len() as i32,
            &interfaces,
            iinit_info,
            &instance_traits,
            class_info,
        );
        self.ab.instances[ci] = inst.clone();
        add_bytes_to_table(&mut self.ab.instances, inst);

        let cinit_info = self.get_method_info(&format!("{}$cinit", name));
        let mut cls = std::mem::take(&mut self.ab.classes[ci]);
        self.bf().class_info(&mut cls, cinit_info, &static_traits, class_info);
        self.ab.classes[ci] = cls.clone();
        add_bytes_to_table(&mut self.ab.classes, cls);

        {
            let line = format!("const int abcclass_{} = {};", const_name(&name.to_string()), class_info);
            self.header_println(&line);
            if class_info >= self.native_class_count {
                self.native_class_count = class_info + 1;
            }
        }

        ci!(self, "// --FinishClass {} {}", name, basename.map_or(String::new(), |b| b.to_string()));
        drop(cx);
        self.show_line_number();
    }

    fn start_program(&mut self, name: &str) {
        self.modulename = Some(name.to_string());
        ci!(self, "// ++StartProgram {}", name);
    }

    fn finish_program(&mut self, cx: &ContextRef, name: &str, init_info: i32) {
        let obj = cx.borrow().scope().expect("NullPointerException");
        let mut traits: Vec<ByteList> = Vec::new();

        self.finish_traits(&obj, &mut traits);

        let package_info = self.get_package_info(name);

        let pi = package_info as usize;
        let mut bytes = std::mem::take(&mut self.ab.scripts[pi]);
        self.bf().script_info(&mut bytes, init_info, &traits, package_info);
        self.ab.scripts[pi] = bytes.clone();
        add_bytes_to_table(&mut self.ab.scripts, bytes);

        ci!(self, "// --FinishProgram {}", self.modulename.as_deref().unwrap_or("null"));
    }

    fn allocate_temp(&mut self) -> i32 {
        let temp = self.cur_locals;
        self.cur_locals += 1;
        ci!(self, "AllocTemp {}", temp);
        if self.cur_locals > self.max_locals {
            self.max_locals = self.cur_locals;
        }
        temp - 1
    }

    fn free_temp(&mut self, t: i32) {
        let t = t + 1;
        if self.show_instructions {
            ci!(self, "FreeTemp {}", t);
            if t != self.cur_locals - 1 {
                self.code_out.push_str("  out of order");
            }
        }
        self.op_kill(t);
        self.cur_locals -= 1;
    }

    fn kill(&mut self, t: i32) {
        let t = t + 1;
        ci!(self, "Kill {}", t);
        self.op_kill(t);
    }

    fn get_temp_count(&mut self) -> i32 {
        self.cur_locals
    }

    fn get_ip(&mut self) -> i32 {
        self.ip()
    }

    fn break_(&mut self, loop_index: i32) {
        self.show_line_number();
        ci!(self, "Break target {}", loop_index);

        self.flush_debug_info();

        self.last_in = IKIND_other;

        let break_temp_count = il_get(&self.break_temp_count, loop_index);
        let mut i = self.cur_locals - 1;
        while i >= break_temp_count {
            self.op_kill(i);
            i -= 1;
        }

        let break_scope_index = il_get(&self.break_scope_depth, loop_index) - 1;
        self.unwind_to(loop_index, break_scope_index);

        if (loop_index as i64) < self.break_addrs.len() as i64 {
            self.op_jump();
            let ip = self.ip() - 3;
            self.break_addrs[loop_index as usize].push(ip);
        }

        cs!(self);
    }

    fn case_label(&mut self, is_default: bool) {
        self.show_line_number();
        if is_default {
            ci!(self, "CaseLabel default");
        } else {
            ci!(self, "CaseLabel");
        }

        self.restore_stack_depth();
        self.save_stack_depth();

        let mut need_label = true;
        if is_default {
            if il_last(&self.seen_default_case) == 0 {
                let ip = self.ip();
                self.default_addrs.push(ip);
                self.case_addrs.last_mut().expect("NullPointerException").push(ip);
                let n = self.seen_default_case.len();
                self.seen_default_case[n - 1] = 1;
            }
        } else {
            let ip = self.ip();
            let last = self.case_addrs.last_mut().expect("NullPointerException");
            if !last.is_empty() && il_last(last) == ip - 1 {
                need_label = false;
            }
            last.push(if need_label { ip } else { ip - 1 });
        }

        if need_label {
            self.op_label();
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn check_type(&mut self, name: &QName) {
        let fullname = name.to_string();
        if fullname.is_empty() {
            return;
        }

        self.show_line_number();
        ci!(self, "CheckType {}", name);

        self.flush_debug_info();

        if self.last_in == IKIND_coerce {
            self.stack(0);
            let erase = self.ip() - self.last_ip;
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("* ERASING {} bytes", erase));
            let last_ip = self.last_ip as usize;
            let code = self.code();
            if last_ip >= code.len() {
                panic!("ArrayIndexOutOfBoundsException: {}", last_ip);
            }
            code.truncate(last_ip);
        }

        self.last_ip = self.ip();
        self.last_in = IKIND_coerce;

        match fullname.as_str() {
            "*" => self.op_coerce_a(),
            "String" => self.op_coerce_s(),
            "Boolean" => self.op_convert_b(),
            "Number" => self.op_convert_d(),
            "int" => self.op_convert_i(),
            "uint" => self.op_convert_u(),
            _ => {
                let class_index = self.add_class_name(Some(name));
                self.op_coerce(class_index);
            }
        }

        cs!(self);
    }

    fn continue_(&mut self, loop_index: i32) {
        self.show_line_number();
        ci!(self, "Continue {}", loop_index);

        self.flush_debug_info();

        self.last_in = IKIND_other;

        let continue_scope_index = il_get(&self.continue_scope_depth, loop_index) - 1;
        self.unwind_to(loop_index, continue_scope_index);

        if loop_index >= 0 && (loop_index as usize) < self.continue_addrs.len() {
            self.op_jump();
            let ip = self.ip() - 3;
            self.continue_addrs[loop_index as usize].push(ip);
        }

        cs!(self);
    }

    fn dup(&mut self) {
        self.show_line_number();
        ci!(self, "Dup");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_dup();
        cs!(self);
    }

    fn else_(&mut self) {
        self.show_line_number();
        ci!(self, "Else");
        self.flush_debug_info();
        self.op_jump();
        let ip = self.ip() - 3;
        self.else_addrs.push(ip);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_property(&mut self, is_qualified: bool, is_super: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>) {
        self.show_line_number();
        ci!(self, "GetProperty is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.late_name(is_qualified, is_attr, used_def_namespaces, -2, -1);

        if is_super {
            self.op_getsuper(index);
        } else {
            self.op_getproperty(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_property_name(&mut self, name: &str, is_super: bool, is_attr: bool) {
        self.show_line_number();
        ci!(self, "GetProperty {} is_super={} is_attr={}", name, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let name_index = self.name_index(name);
        let index = self.mn_rtqname(name_index, is_attr);

        if is_super {
            self.op_getsuper(index);
        } else {
            self.op_getproperty(index);
        }

        self.stack(-1);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_property_name_nss(&mut self, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("GetProperty {} ", name));
            self.print_namespace_set(namespaces.expect("NullPointerException"));
            self.code_out.push_str(&format!(" is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr));
        }
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.get_property_common(name, namespaces, is_qualified, is_attr);

        if is_super {
            self.op_getsuper(index);
        } else {
            self.op_getproperty(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn set_property(&mut self, is_qualified: bool, is_super: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>, is_constinit: bool) {
        self.show_line_number();
        ci!(self, "SetProperty is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.late_name(is_qualified, is_attr, used_def_namespaces, -4, -3);

        if is_super {
            self.op_setsuper(index);
        } else if is_constinit {
            self.op_initproperty(index);
        } else {
            self.op_setproperty(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn set_property_name(&mut self, name: &str, is_super: bool, is_attr: bool) {
        self.show_line_number();
        ci!(self, "SetProperty {} is_super={} is_attr={}", name, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let name_index = self.name_index(name);
        let index = self.mn_rtqname(name_index, is_attr);

        if is_super {
            self.op_setsuper(index);
        } else {
            self.op_setproperty(index);
        }

        self.stack(-3);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn set_property_name_nss(&mut self, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool, is_constinit: bool) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("SetProperty {} ", name));
            self.print_namespace_set(namespaces.expect("NullPointerException"));
            self.code_out.push_str(&format!(
                " is_qualified={} is_super={} is_attr={} is_constinit={}",
                is_qualified, is_super, is_attr, is_constinit
            ));
        }
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.get_property_common(name, namespaces, is_qualified, is_attr);

        if is_super {
            self.op_setsuper(index);
        } else if is_constinit {
            self.op_initproperty(index);
        } else {
            self.op_setproperty(index);
        }

        self.stack(-2);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn delete_property(&mut self, is_qualified: bool, is_super: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>) {
        self.show_line_number();
        ci!(self, "DeleteProperty is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.late_name(is_qualified, is_attr, used_def_namespaces, -2, -1);

        if !is_super {
            self.op_delproperty(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn delete_property_name(&mut self, name: &str, is_super: bool, is_attr: bool) {
        self.show_line_number();
        ci!(self, "DeleteProperty {} is_super={} is_attr={}", name, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let name_index = self.name_index(name);
        let index = self.mn_rtqname(name_index, is_attr);

        if !is_super {
            self.op_delproperty(index);
        }

        self.stack(-1);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn delete_property_name_nss(&mut self, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("DeleteProperty {} ", name));
            self.print_namespace_set(namespaces.expect("NullPointerException"));
            self.code_out.push_str(&format!(" is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr));
        }
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.get_property_common(name, namespaces, is_qualified, is_attr);

        if !is_super {
            self.op_delproperty(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_descendants(&mut self, is_qualified: bool, is_attr: bool, used_def_namespaces: Option<&Namespaces>) {
        self.show_line_number();
        ci!(self, "GetDescendants is_qualified={} is_attr={}", is_qualified, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.late_name(is_qualified, is_attr, used_def_namespaces, -2, -1);

        self.op_descendants(index);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_descendants_name(&mut self, name: &str, is_super: bool, is_attr: bool) {
        self.show_line_number();
        ci!(self, "GetDescendants {} is_super={} is_attr={}", name, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let name_index = self.name_index(name);
        let index = self.mn_rtqname(name_index, is_attr);

        if !is_super {
            self.op_descendants(index);
        }

        self.stack(-1);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_descendants_name_nss(&mut self, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("GetDescendants {} ", name));
            self.print_namespace_set(namespaces.expect("NullPointerException"));
            self.code_out.push_str(&format!(" is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr));
        }
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.get_property_common(name, namespaces, is_qualified, is_attr);

        if !is_super {
            self.op_descendants(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn delete_descendants_name(&mut self, name: &str, is_super: bool, is_attr: bool) {
        self.show_line_number();
        ci!(self, "DeleteDescendants {} is_super={} is_attr={}", name, is_super, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();

        let name_index = self.name_index(name);
        let index = self.mn_rtqname(name_index, is_attr);

        if !is_super {
            self.op_deletedescendants(index);
        }

        self.stack(-1);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn delete_descendants_name_nss(&mut self, name: &str, namespaces: Option<&Namespaces>, is_qualified: bool, is_super: bool, is_attr: bool) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("DeleteDescendants {} ", name));
            self.print_namespace_set(namespaces.expect("NullPointerException"));
            self.code_out.push_str(&format!(" is_qualified={} is_super={} is_attr={}", is_qualified, is_super, is_attr));
        }
        self.flush_debug_info();
        self.last_ip = self.ip();

        let index = self.get_property_common(name, namespaces, is_qualified, is_attr);

        if !is_super {
            self.op_deletedescendants(index);
        }

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn find_property_name_nss(&mut self, name: &str, namespaces: Option<&Namespaces>, is_strict: bool, is_qualified: bool, is_attr: bool) {
        self.show_line_number();
        if self.show_instructions {
            self.code_out.push_str(NL);
            self.code_out.push_str(&format!("FindProperty {} ", name));
            self.print_namespace_set(namespaces.expect("NullPointerException"));
            self.code_out.push_str(&format!(" is_strict={} is_qualified={} is_attr={}", is_strict, is_qualified, is_attr));
        }
        self.flush_debug_info();

        self.last_ip = self.ip();
        self.last_in = IKIND_other;

        let index = self.get_property_common(name, namespaces, is_qualified, is_attr);
        if is_strict {
            self.op_findpropstrict(index);
        } else {
            self.op_findproperty(index);
        }

        cs!(self);
    }

    fn find_property_name(&mut self, name: &str, is_strict: bool, is_attr: bool) {
        self.show_line_number();
        ci!(self, "FindProperty {} is_strict={} is_attr={}", name, is_strict, is_attr);
        self.flush_debug_info();
        self.stack(-1);
        self.last_ip = self.ip();
        self.last_in = IKIND_other;

        let name_index = self.name_index(name);
        let index = self.mn_rtqname(name_index, is_attr);

        if is_strict {
            self.op_findpropstrict(index);
        } else {
            self.op_findproperty(index);
        }

        cs!(self);
    }

    fn find_property(&mut self, is_strict: bool, is_attr: bool, is_qualified: bool, used_def_namespaces: Option<&Namespaces>) {
        self.show_line_number();
        ci!(self, "FindProperty  is_strict={} is_attr={}", is_strict, is_attr);
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_other;

        let index = self.late_name(is_qualified, is_attr, used_def_namespaces, -2, -1);

        if is_strict {
            self.op_findpropstrict(index);
        } else {
            self.op_findproperty(index);
        }

        cs!(self);
    }

    fn get_global_scope(&mut self) {
        self.show_line_number();
        ci!(self, "GetGlobalScope");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_getglobalscope();
        cs!(self);
    }

    fn get_base_object(&mut self, scope_index: i32) {
        self.show_line_number();
        ci!(self, "GetBaseObject {}", scope_index);
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_getscopeobject(scope_index);
        cs!(self);
    }

    fn get_scope_chain(&mut self) {
        self.show_line_number();
        ci!(self, "GetScopeChain");
        self.flush_debug_info();
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn get_scope_on_top(&mut self) {
        self.show_line_number();
        ci!(self, "GetScopeOnTop");
        self.flush_debug_info();
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn try_(&mut self, has_finally: bool) {
        self.show_line_number();
        ci!(self, "Try");

        let eb = ExceptionBlock {
            try_start: self.ip(),
            has_finally,
            scope_index: self.cx.borrow().get_scope_depth() as i32 - 1,
            loop_index: self.break_addrs.len() as i32 - 1,
            ..Default::default()
        };
        self.exception_blocks.push(eb);

        self.last_in = IKIND_other;
        cs!(self);

        self.flush_debug_info();
    }

    fn catch_clauses_begin(&mut self) {
        self.show_line_number();
        ci!(self, "CatchClausesBegin");

        let ip = self.ip();
        let cur_locals = self.cur_locals;
        let eb = self.last_exception_block();
        eb.try_end = ip;
        eb.cur_locals = cur_locals;

        self.last_in = IKIND_other;
        cs!(self);

        self.flush_debug_info();
    }

    fn catch_clauses_end(&mut self) {
        self.show_line_number();
        ci!(self, "CatchClausesEnd");

        let target = self.ip();

        let fixups = std::mem::take(&mut self.last_exception_block().fixups);
        for &jump_index in fixups.iter().rev() {
            self.patch_jump("Jump", jump_index, target);
        }

        self.last_in = IKIND_other;
        cs!(self);

        self.flush_debug_info();
    }

    fn call_finally(&mut self, num_finallys: i32) {
        self.show_line_number();
        ci!(self, "CallFinally");

        self.flush_debug_info();

        let mut finallys_invoked = 0;
        let mut i = self.exception_blocks.len();
        while i > 0 {
            let cur_locals = self.cur_locals;
            let eb = self.exception_block_mut(i - 1);
            if cur_locals > eb.cur_locals {
                eb.cur_locals = cur_locals;
            }

            if self.exception_blocks[i - 1].has_finally {
                let finally_index = self.exception_blocks[i - 1].finally_addrs.len() as i32;
                self.op_pushbyte(finally_index);

                self.op_jump();
                let ip = self.ip() - 3;
                self.exception_block_mut(i - 1).fixups.push(ip);

                self.op_label();
                self.pop();

                let ip = self.ip();
                self.exception_block_mut(i - 1).finally_addrs.push(ip);
                self.op_label();

                finallys_invoked += 1;
                if finallys_invoked == num_finallys {
                    break;
                }
            }
            i -= 1;
        }
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn finally_clause_begin(&mut self) {
        let cur_locals = self.cur_locals;
        let exblock = self.last_exception_block();
        let temp = exblock.cur_locals;
        exblock.cur_locals = cur_locals;
        self.cur_locals = temp;
    }

    fn finally_clause_end(&mut self) {
        self.show_line_number();
        ci!(self, "FinallyClauseEnd");

        self.flush_debug_info();

        self.last_in = IKIND_other;

        let fixups = self.last_exception_block().finally_addrs.clone();
        if !fixups.is_empty() {
            let start = self.ip();
            self.op_lookupswitch();

            self.int24(0);

            self.int(fixups.len() as i32 - 1);

            for &f in &fixups {
                self.int24(f - start);
            }

            let offset = self.ip() - start;
            self.patch24(start + 1, offset);
        }

        self.cur_locals = self.last_exception_block().cur_locals;

        self.exception_blocks.pop();

        cs!(self);
    }

    fn catch(&mut self, type_: Option<&TypeValue>, name: Option<&QName>) {
        self.show_line_number();
        ci!(self, "Catch");

        self.flush_debug_info();

        self.op_jump();
        let ip = self.ip() - 3;
        let (try_start, try_end) = {
            let eb = self.last_exception_block();
            eb.fixups.push(ip);
            (eb.try_start, eb.try_end)
        };

        int(&mut self.ab.exception_table, try_start as i64);
        int(&mut self.ab.exception_table, try_end as i64);
        let ip = self.ip();
        int(&mut self.ab.exception_table, ip as i64);

        let classname = type_.expect("NullPointerException").as_object().builder().expect("NullPointerException").classname();
        let class_index = self.add_class_name(classname.as_ref());
        int(&mut self.ab.exception_table, class_index as i64);

        let name_index = match name {
            Some(n) => self.add_class_name(Some(n)),
            None => 0,
        };
        int(&mut self.ab.exception_table, name_index as i64);

        self.stack(1);

        self.ab.exception_count += 1;

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn throw(&mut self) {
        self.show_line_number();
        ci!(self, "Throw");
        self.flush_debug_info();
        self.op_throw();
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn if_(&mut self, kind: i32) {
        self.show_line_number();
        ci!(self, "If {}", Self::if_kind_str(kind));

        self.flush_debug_info();

        self.if_op(kind, 0);

        let ip = self.ip() - 3;
        self.if_addrs.push(ip);

        self.save_stack_depth();

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn invoke_binary(&mut self, op_index: i32, number_usage: Option<&NumberUsage>) {
        self.show_line_number();
        ci!(self, "InvokeBinary {}", Self::get_binary_name(op_index));

        self.flush_debug_info();

        let mut param = 0;
        let mut use_param = false;
        if self.cx.borrow().statics.borrow().es4_numerics {
            let nu = number_usage.expect("NullPointerException");
            if !nu.is_default() {
                param = nu.encode();
                use_param = true;
            }
        }

        self.last_in = IKIND_other;

        match op_index {
            BINARY_BinaryPlusOp_II => self.op_add_i(),
            BINARY_BinaryPlusOp => {
                if use_param {
                    self.op_add_p(param)
                } else {
                    self.op_add()
                }
            }
            BINARY_BinaryMinusOp_II => self.op_subtract_i(),
            BINARY_BinaryMinusOp => {
                if use_param {
                    self.op_subtract_p(param)
                } else {
                    self.op_subtract()
                }
            }
            BINARY_MultiplyOp_II => self.op_multiply_i(),
            BINARY_MultiplyOp => {
                if use_param {
                    self.op_multiply_p(param)
                } else {
                    self.op_multiply()
                }
            }
            BINARY_DivideOp => {
                if use_param {
                    self.op_divide_p(param)
                } else {
                    self.op_divide()
                }
            }
            BINARY_ModulusOp => {
                if use_param {
                    self.op_modulo_p(param)
                } else {
                    self.op_modulo()
                }
            }
            BINARY_LeftShiftOp_II | BINARY_LeftShiftOp => self.op_lshift(),
            BINARY_RightShiftOp_II | BINARY_RightShiftOp => self.op_rshift(),
            BINARY_UnsignedRightShiftOp_II | BINARY_UnsignedRightShiftOp => self.op_urshift(),
            BINARY_LessThanOp => self.op_lessthan(),
            BINARY_GreaterThanOp => self.op_greaterthan(),
            BINARY_LessThanOrEqualOp => self.op_lessequals(),
            BINARY_GreaterThanOrEqualOp => self.op_greaterequals(),
            BINARY_EqualsOp_II | BINARY_EqualsOp => self.op_equals(),
            BINARY_StrictEqualsOp_II | BINARY_StrictEqualsOp => self.op_strictequals(),
            BINARY_NotEqualsOp_II | BINARY_NotEqualsOp => {
                self.op_equals();
                self.op_not();
            }
            BINARY_StrictNotEqualsOp_II | BINARY_StrictNotEqualsOp => {
                self.op_strictequals();
                self.op_not();
            }
            BINARY_BitwiseAndOp_II | BINARY_BitwiseAndOp => self.op_bitand(),
            BINARY_BitwiseXorOp_II | BINARY_BitwiseXorOp => self.op_bitxor(),
            BINARY_BitwiseOrOp_II | BINARY_BitwiseOrOp => self.op_bitor(),
            BINARY_LogicalAndOp_II | BINARY_LogicalAndOp | BINARY_LogicalOrOp_II | BINARY_LogicalOrOp => {}
            BINARY_InstanceofOp => self.op_instanceof(),
            BINARY_InOp => self.op_in(),
            BINARY_IsLateOp => self.op_istypelate(),
            BINARY_AsLateOp => self.op_astypelate(),
            _ => {}
        }

        cs!(self);
    }

    fn apply_type(&mut self, size: i32) {
        self.show_line_number();
        ci!(self, "ApplyType {}", size);
        self.flush_debug_info();
        self.apply_type_op(size);
        cs!(self);
    }

    fn invoke_closure(&mut self, as_construct: bool, size: i32) {
        self.show_line_number();
        ci!(self, "InvokeClosure {}, {}", if as_construct { "construct" } else { "call" }, size);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_callfunction(as_construct, size);
        cs!(self);
    }

    fn construct_property(&mut self, name: &str, namespaces: Option<&Namespaces>, size: i32, is_qualified: bool, is_super: bool, is_attr: bool) {
        self.call_property_common(name, namespaces, size, is_qualified, is_super, is_attr, true, false);
    }

    fn call_property(&mut self, name: &str, namespaces: Option<&Namespaces>, size: i32, is_qualified: bool, is_super: bool, is_attr: bool, is_lex: bool) {
        self.call_property_common(name, namespaces, size, is_qualified, is_super, is_attr, false, is_lex);
    }

    fn invoke_method(&mut self, local_dispatch: bool, method_id: i32, size: i32) {
        self.show_line_number();
        ci!(self, "InvokeMethod {} {} {}", if local_dispatch { "local" } else { "global" }, method_id, size);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        if local_dispatch {
            self.op_callmethod(method_id, size);
        } else {
            self.op_callstatic(method_id, size);
        }
        cs!(self);
    }

    fn invoke_super(&mut self, construct: bool, size: i32) {
        self.show_line_number();
        ci!(self, "InvokeSuper {} {}", if construct { "construct" } else { "call" }, size);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        if construct {
            self.op_constructsuper(size);
        } else {
            self.cx.borrow().internal_error("internal error in InvokeSuper()");
        }
        cs!(self);
    }

    fn invoke_unary(&mut self, operator_id: i32, size: i32, data: i32, used_def_namespaces: Option<&Namespaces>, number_usage: Option<&NumberUsage>) {
        self.show_line_number();

        let mut param = 0;
        let mut use_param = false;
        if self.cx.borrow().statics.borrow().es4_numerics {
            if let Some(nu) = number_usage.filter(|nu| !nu.is_default()) {
                param = nu.encode();
                use_param = true;
            }
        }

        if self.show_instructions {
            ci!(self, "InvokeUnary {} {}", Self::get_unary_name(operator_id), size);
            if data >= 0 {
                self.code_out.push_str(&format!(" {}", data));
            }
        }

        self.flush_debug_info();

        self.last_in = IKIND_other;

        match operator_id {
            UNARY_TypeofOp_B => {
                let s = self.utf8("boolean");
                self.op_pushstring(s);
            }
            UNARY_TypeofOp_I | UNARY_TypeofOp_D | UNARY_TypeofOp_M => {
                let s = self.utf8("number");
                self.op_pushstring(s);
            }
            UNARY_TypeofOp_S => {
                let s = self.utf8("string");
                self.op_pushstring(s);
            }
            UNARY_TypeofOp_U => {
                let s = self.utf8("undefined");
                self.op_pushstring(s);
            }
            UNARY_TypeofOp => self.op_typeof(),
            UNARY_IncrementOp => {
                if use_param {
                    self.op_increment_p(param)
                } else {
                    self.op_increment()
                }
            }
            UNARY_IncrementOp_I => self.op_increment_i(),
            UNARY_IncrementLocalOp => {
                if use_param {
                    self.op_inclocal_p(data, param)
                } else {
                    self.op_inclocal(data)
                }
            }
            UNARY_IncrementLocalOp_I => self.op_inclocal_i(data),
            UNARY_DecrementOp => {
                if use_param {
                    self.op_decrement_p(param)
                } else {
                    self.op_decrement()
                }
            }
            UNARY_DecrementOp_I => self.op_decrement_i(),
            UNARY_DecrementLocalOp => {
                if use_param {
                    self.op_declocal_p(data, param)
                } else {
                    self.op_declocal(data)
                }
            }
            UNARY_DecrementLocalOp_I => self.op_declocal_i(data),
            UNARY_UnaryPlusOp_I => {}
            UNARY_UnaryPlusOp => self.op_convert_d(),
            UNARY_UnaryPlusOp_M => self.op_convert_m(),
            UNARY_UnaryMinusOp_I | UNARY_UnaryMinusOp => {
                if use_param {
                    self.op_negate_p(param)
                } else {
                    self.op_negate()
                }
            }
            UNARY_LogicalNotOp_B | UNARY_LogicalNotOp_I => self.op_not(),
            UNARY_Put => {
                let nss = self.make_namespace_set(used_def_namespaces.expect("NullPointerException"));
                let index = self.mn_multiname_late(nss, false);
                self.op_setproperty(index);
            }
            UNARY_Get => {
                let nss = self.make_namespace_set(used_def_namespaces.expect("NullPointerException"));
                let index = self.mn_multiname_late(nss, false);
                self.op_getproperty(index);
            }
            UNARY_LogicalNotOp => self.op_not(),
            UNARY_HasMoreNames => self.op_hasnext(),
            UNARY_DeleteOp => {
                let nss = self.make_namespace_set(used_def_namespaces.expect("NullPointerException"));
                let index = self.mn_multiname_late(nss, false);
                self.op_delproperty(index);
            }
            UNARY_BitwiseNotOp_I | UNARY_BitwiseNotOp => self.op_bitnot(),
            UNARY_NextName => self.op_nextname(),
            UNARY_NextValue => self.op_nextvalue(),
            UNARY_VoidOp => {}
            UNARY_ToXMLString => self.op_esc_xelem(),
            UNARY_ToXMLAttrString => self.op_esc_xattr(),
            UNARY_CheckFilterOp => self.op_check_filter(),
            _ => {}
        }
        cs!(self);
    }

    fn load_global(&mut self, var_index: i32, type_id: i32) {
        self.show_line_number();
        ci!(self, "LoadGlobal {}, {}", var_index, type_to_string(type_id));
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_getglobalscope();
        self.op_getslot(var_index + 1);
        cs!(self);
    }

    fn load_global_name(&mut self, name: &str) {
        self.show_line_number();
        ci!(self, "LoadGlobal {}", name);
        self.flush_debug_info();
        self.cx.borrow().internal_error("LoadGlobal(name) is deprecated");
        cs!(self);
    }

    fn load_register(&mut self, reg: i32, type_id: i32) {
        self.show_line_number();
        ci!(self, "LoadRegister {}, {}", reg, type_to_string(type_id));
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_getlocal(reg);
        cs!(self);
    }

    fn load_this(&mut self) {
        self.show_line_number();
        ci!(self, "LoadThis");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_getlocal(0);
        cs!(self);
    }

    fn load_super(&mut self) {
        self.show_line_number();
        ci!(self, "LoadSuper");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.cx.borrow().internal_error("super expressions are not implemented");
        self.last_in = IKIND_push;
        cs!(self);
    }

    fn load_var(&mut self, var_index: i32) {
        self.show_line_number();
        ci!(self, "LoadVar {}", var_index);
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.op_getslot(var_index + 1);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn label_statement_begin(&mut self) {
        let depth = self.cx.borrow().get_scope_depth() as i32;
        self.break_addrs.push(Vec::new());
        self.break_scope_depth.push(depth);
        self.break_temp_count.push(self.cur_locals);
        self.continue_addrs.push(Vec::new());
        self.continue_scope_depth.push(depth);
    }

    fn label_statement_end(&mut self, loop_index: i32) {
        self.patch_break(loop_index);
        self.patch_continue(loop_index);
    }

    fn loop_begin(&mut self) {
        self.show_line_number();
        ci!(self, "LoopBegin");

        let depth = self.cx.borrow().get_scope_depth() as i32;
        self.break_addrs.push(Vec::new());
        self.break_scope_depth.push(depth);
        self.break_temp_count.push(self.cur_locals);

        self.continue_addrs.push(Vec::new());
        self.continue_scope_depth.push(depth);

        self.op_jump();
        let ip = self.ip() - 3;
        self.loopbegin_addrs.push(ip);

        self.op_label();

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn loop_end(&mut self, kind: i32) {
        self.show_line_number();
        ci!(self, "LoopEnd {}", Self::if_kind_str(kind));

        self.flush_debug_info();

        let addr = self.ip();
        let offset = il_last(&self.loopbegin_addrs) - addr + 2 - 3;

        self.if_op(kind, offset);

        il_remove_last(&mut self.loopbegin_addrs);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn new_array(&mut self, size: i32) {
        self.show_line_number();
        ci!(self, "NewArray {}", size);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_newarray(size);
        cs!(self);
    }

    fn new_class_object(&mut self, name: &QName) {
        self.show_line_number();
        ci!(self, "NewClassObject {}", name);
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_other;
        let info = self.get_class_info(name);
        self.op_newclass(info);
        cs!(self);
    }

    fn new_function_object(&mut self, name: &str) {
        self.show_line_number();
        ci!(self, "NewFunctionObject {}", name);
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_other;
        let method_info = self.get_method_info(name);
        self.op_newfunction(method_info);
        cs!(self);
    }

    fn new_activation(&mut self) {
        self.show_line_number();
        ci!(self, "NewActivation");
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_newactivation();
        cs!(self);
    }

    fn new_catch(&mut self, index: i32) {
        self.show_line_number();
        ci!(self, "NewCatch {}", index);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_newcatch(index);
        cs!(self);
    }

    fn new_object(&mut self, size: i32) {
        self.show_line_number();
        ci!(self, "NewObject {}", size);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_newobject(size);
        cs!(self);
    }

    fn patch_break(&mut self, loop_index: i32) {
        self.show_line_number();
        ci!(self, "PatchBreak {}", loop_index);

        self.flush_debug_info();

        let target = self.ip();
        let mut break_addr = self.break_addrs.pop().expect("NullPointerException");
        while let Some(break_index) = break_addr.pop() {
            self.patch_jump("Jump", break_index, target);
        }
        il_remove_last(&mut self.break_scope_depth);
        il_remove_last(&mut self.break_temp_count);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn patch_continue(&mut self, loop_index: i32) {
        self.show_line_number();
        ci!(self, "PatchContinue {}", loop_index);

        self.flush_debug_info();

        let target = self.ip();
        let mut continue_addr = self.continue_addrs.pop().expect("NullPointerException");
        while let Some(continue_index) = continue_addr.pop() {
            self.patch_jump("Jump", continue_index, target);
        }
        il_remove_last(&mut self.continue_scope_depth);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn patch_else(&mut self, target: i32) {
        self.show_line_number();
        ci!(self, "PatchElse {}", il_last(&self.else_addrs));

        self.flush_debug_info();

        let else_index = il_remove_last(&mut self.else_addrs);
        self.patch_jump("Jump", else_index, target);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn patch_if(&mut self, target: i32) {
        self.show_line_number();
        ci!(self, "PatchIf {}", il_last(&self.if_addrs));

        self.flush_debug_info();

        self.restore_stack_depth();

        let if_index = il_remove_last(&mut self.if_addrs);
        self.patch_jump("If", if_index, target);

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn patch_loop_begin(&mut self, target: i32) {
        self.show_line_number();
        ci!(self, "PatchLoopBegin {}", target);

        self.flush_debug_info();

        let loopbegin_index = il_last(&self.loopbegin_addrs);
        self.patch_jump("Jump", loopbegin_index, target);
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn patch_switch_begin(&mut self, addr: i32) {
        self.show_line_number();
        ci!(self, "PatchSwitchBegin {}", addr);

        self.flush_debug_info();

        self.restore_stack_depth();

        let switchbegin_index = il_remove_last(&mut self.switchbegin_addrs);
        self.patch_jump("Jump", switchbegin_index, addr);
        self.last_in = IKIND_other;
        il_remove_last(&mut self.seen_default_case);
        cs!(self);
    }

    fn pop(&mut self) {
        self.show_line_number();
        ci!(self, "Pop");
        self.flush_debug_info();
        self.op_pop();
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn push_boolean(&mut self, value: bool) {
        self.show_line_number();
        ci!(self, "PushBoolean {}", if value { 1 } else { 0 });
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        if value {
            self.op_pushtrue();
        } else {
            self.op_pushfalse();
        }
        cs!(self);
    }

    fn push_empty(&mut self) {
        self.show_line_number();
        ci!(self, "PushEmpty");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_pushnull();
        cs!(self);
    }

    fn push_case_index(&mut self, index: i32) {
        self.show_line_number();
        ci!(self, "PushCaseIndex {}", index);
        self.flush_debug_info();
        self.op_pushshort(index);
        cs!(self);
    }

    fn push_null(&mut self) {
        self.show_line_number();
        ci!(self, "PushNull");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_pushnull();
        cs!(self);
    }

    fn push_number(&mut self, val: &NumberConstant, type_id: i32) {
        self.show_line_number();
        if self.show_instructions {
            let nn = if val.number_type() as i32 == NumberUsage::use_decimal {
                val.decimal_value().to_string()
            } else {
                let mut s = String::new();
                il_format_double_as_string(val.double_value(), &mut s);
                s
            };
            ci!(self, "PushNumber {}, {}", nn, type_to_string(type_id));
        }

        self.flush_debug_info();

        self.last_ip = self.ip();
        self.last_in = IKIND_push;

        let avm1 = self.cx.borrow().abc_version(TARGET_AVM1);
        if type_id == TYPE_int || (type_id == TYPE_uint && avm1) {
            let ival = val.int_value();
            if ival as i8 as i32 == ival {
                self.op_pushbyte(ival);
            } else if ival as i16 as i32 == ival {
                self.op_pushshort(ival);
            } else {
                let index = self.int_const(ival);
                self.op_pushint(index);
            }
        } else if type_id == TYPE_uint {
            let ival = val.uint_value();
            if (ival & 0x7F) == ival {
                self.op_pushbyte(ival as i32);
            } else if (ival & 0x7FFFF) == ival {
                self.op_pushshort(ival as i32);
            } else {
                let index = self.uint_const(ival as i32 as i64);
                self.op_pushuint(index);
            }
        } else if type_id == TYPE_double {
            let dval = val.double_value();
            if dval.is_nan() {
                self.op_pushnan();
            } else if dval != 0.0 && !dval.is_infinite() && dval == val.int_value() as f64 {
                let ival = val.int_value();
                if ival as i8 as i32 == ival {
                    self.op_pushbyte(ival);
                } else if ival as i16 as i32 == ival {
                    self.op_pushshort(ival);
                } else {
                    let index = self.int_const(ival);
                    self.op_pushint(index);
                }
            } else {
                let index = self.double_const(dval);
                self.op_pushdouble(index);
            }
        } else if type_id == TYPE_decimal {
            let dval = val.decimal_value();
            let index = self.decimal_const(&dval);
            self.op_pushdecimal(index);
        }

        cs!(self);
    }

    fn new_namespace(&mut self, ns: &ObjectValue) {
        self.show_line_number();
        ci!(self, "NewNamespace {}", ns.name());
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_other;
        self.add_namespace(ns);
        cs!(self);
    }

    fn push_string(&mut self, str: &str) {
        self.show_line_number();
        ci!(self, "PushString {}", str);
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        let str_index = self.utf8(str);
        self.op_pushstring(str_index);
        cs!(self);
    }

    fn push_namespace(&mut self, ns: &ObjectValue) {
        self.show_line_number();
        ci!(self, "PushNamespace {}", ns.name());
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        let ns_index = self.add_namespace(ns);
        self.op_pushnamespace(ns_index);
        cs!(self);
    }

    fn push_undefined(&mut self) {
        self.show_line_number();
        ci!(self, "PushUndefined");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_pushundefined();
        cs!(self);
    }

    fn push_uninitialized(&mut self) {
        self.show_line_number();
        ci!(self, "PushUninitialized");
        self.flush_debug_info();
        self.last_ip = self.ip();
        self.last_in = IKIND_push;
        self.op_pushuninitialized();
        cs!(self);
    }

    fn return_(&mut self, type_id: i32) {
        self.show_line_number();
        ci!(self, "Return");
        self.flush_debug_info();
        if type_id == TYPE_void {
            self.op_returnvoid();
        } else {
            self.op_returnvalue();
        }
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn store_global(&mut self, var_index: i32, type_id: i32) {
        self.show_line_number();
        ci!(self, "StoreGlobal {}, {}", var_index, type_to_string(type_id));
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_getglobalscope();
        self.op_swap();
        self.op_setslot(var_index + 1);
        cs!(self);
    }

    fn store_global_name(&mut self, name: &str) {
        self.show_line_number();
        ci!(self, "StoreGlobal {}", name);
        self.flush_debug_info();
        self.cx.borrow().internal_error("StoreGlobal(name) is deprecated");
        cs!(self);
    }

    fn store_register_name(&mut self, reg: i32, type_id: i32, _var_name: &str) {
        self.show_line_number();
        ci!(self, "StoreRegister {}, {}", reg, type_to_string(type_id));
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_setlocal(reg);
        cs!(self);
    }

    fn has_next(&mut self, object_register: i32, index_register: i32) {
        self.show_line_number();
        ci!(self, "HasNext {}, {}", object_register, index_register);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_hasnext2(object_register, index_register);
        cs!(self);
    }

    fn store_var(&mut self, var_index: i32) {
        self.show_line_number();
        ci!(self, "StoreVar {}", var_index);
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_setslot(var_index + 1);
        cs!(self);
    }

    fn switch_begin(&mut self) {
        self.show_line_number();
        ci!(self, "SwitchBegin");

        let depth = self.cx.borrow().get_scope_depth() as i32;
        self.seen_default_case.push(0);
        self.case_addrs.push(Vec::new());
        self.break_addrs.push(Vec::new());
        self.break_scope_depth.push(depth);
        self.break_temp_count.push(self.cur_locals);
        self.continue_addrs.push(Vec::new());
        self.continue_scope_depth.push(depth);

        self.op_jump();
        let ip = self.ip() - 3;
        self.switchbegin_addrs.push(ip);

        self.save_stack_depth();

        self.last_in = IKIND_other;
        cs!(self);
    }

    fn switch_table(&mut self) {
        self.show_line_number();
        ci!(self, "SwitchTable");

        self.flush_debug_info();

        self.last_in = IKIND_other;

        let start = self.ip();
        self.op_lookupswitch();

        let default_addr = il_remove_last(&mut self.default_addrs);
        self.int24(default_addr - start);

        let case_addr = self.case_addrs.pop().expect("NullPointerException");
        self.int(case_addr.len() as i32 - 1);

        for &c in &case_addr {
            self.int24(c - start);
        }

        cs!(self);

        let switch_continues = self.continue_addrs.pop().expect("NullPointerException");
        if let Some(outer) = self.continue_addrs.last_mut() {
            outer.extend_from_slice(&switch_continues);
        }
        il_remove_last(&mut self.continue_scope_depth);
    }

    fn to_boolean(&mut self, type_id: i32) {
        self.show_line_number();
        ci!(self, "ToBoolean {}", type_to_string(type_id));
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_convert_b();
        cs!(self);
    }

    fn to_native_bool(&mut self) {
        self.show_line_number();
        ci!(self, "ToNativeBool");
        self.flush_debug_info();
    }

    fn to_int(&mut self) {
        self.show_line_number();
        ci!(self, "ToInt");
        self.op_convert_i();
        self.flush_debug_info();
    }

    fn to_uint(&mut self) {
        self.show_line_number();
        ci!(self, "ToUint");
        self.op_convert_i();
        self.flush_debug_info();
    }

    fn to_double(&mut self, type_id: i32) {
        self.show_line_number();
        ci!(self, "ToDouble {}", type_to_string(type_id));
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_convert_d();
        cs!(self);
    }

    fn to_decimal(&mut self, type_id: i32) {
        self.show_line_number();
        ci!(self, "ToDecimal {}", type_to_string(type_id));
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_convert_m();
        cs!(self);
    }

    fn to_object(&mut self) {
        self.show_line_number();
        ci!(self, "ToObject");
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_convert_o();
        cs!(self);
    }

    fn to_string_(&mut self) {
        self.show_line_number();
        ci!(self, "ToString");
        self.flush_debug_info();
        self.last_in = IKIND_other;
        self.op_convert_s();
        cs!(self);
    }

    fn push_scope(&mut self) {
        self.show_line_number();
        ci!(self, "PushScope");
        self.flush_debug_info();
        self.op_pushscope();
        self.last_ip = self.ip();
        cs!(self);
    }

    fn push_with(&mut self) {
        self.show_line_number();
        ci!(self, "PushWith");
        self.flush_debug_info();
        self.op_pushwith();
        self.last_ip = self.ip();
        cs!(self);
    }

    fn pop_scope(&mut self) {
        self.show_line_number();
        ci!(self, "PopScope");
        self.flush_debug_info();
        self.op_popscope();
        self.last_ip = self.ip();
        cs!(self);
    }

    fn pop_with(&mut self) {
        self.show_line_number();
        ci!(self, "PopWith");
        self.flush_debug_info();
        self.op_popscope();
        self.last_ip = self.ip();
        cs!(self);
    }

    fn swap(&mut self) {
        self.show_line_number();
        ci!(self, "Swap");
        self.flush_debug_info();
        self.op_swap();
        self.last_ip = self.ip();
        cs!(self);
    }

    fn default_xml_namespace(&mut self) {
        self.show_line_number();
        ci!(self, "DefaultXMLNamespace");
        self.flush_debug_info();
        self.op_dxnslate();
        self.sets_dxns = true;
        self.last_in = IKIND_other;
        cs!(self);
    }

    fn default_xml_namespace_name(&mut self, name: &str) {
        self.show_line_number();
        ci!(self, "DefaultXMLNamespace {}", name);
        self.flush_debug_info();
        let index = self.utf8(name);
        self.op_dxns(index);
        self.sets_dxns = true;
        self.last_in = IKIND_other;
        cs!(self);
    }
}

#[cfg(test)]
mod tests {
    //! Each scenario is mirrored call for call by a Java driver against the original classes; the expected
    //! length and FNV-1a hash are of the Java output (ABC hex, then `il_str`/`header_str` when shown).

    use super::*;
    use crate::asc::emit::activation_builder::new_activation_builder;
    use crate::asc::emit::class_builder::new_class_builder;
    use crate::asc::emit::features::{DIALECT_AS3, TARGET_AVM2};
    use crate::asc::emit::global_builder::new_global_builder;
    use crate::asc::emit::instance_builder::new_instance_builder;
    use crate::asc::semantics::emitter::EmitterRef;
    use crate::asc::semantics::reference_value::ReferenceValue;
    use crate::asc::semantics::value::{KeyValuePair, KeylessValue};
    use crate::asc::util::context::ContextStatics;

    struct T {
        cx: ContextRef,
        e: Rc<RefCell<ActionBlockEmitter>>,
        global: ObjectValue,
        pub_: ObjectValue,
        internal: ObjectValue,
        priv_: ObjectValue,
        pkg: ObjectValue,
        prot: ObjectValue,
        sprot: ObjectValue,
    }

    macro_rules! e {
        ($t:ident) => {
            $t.e.borrow_mut()
        };
    }

    fn setup(debug: bool, show: bool) -> T {
        TypeValue::init();
        ObjectValue::init();
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        {
            let mut s = statics.borrow_mut();
            s.use_static_semantics = true;
            s.dialect = DIALECT_AS3;
            s.set_abc_version(TARGET_AVM2);
        }
        let cx = Rc::new(RefCell::new(Context::new(statics)));
        let e = Rc::new(RefCell::new(ActionBlockEmitter::new(&cx, Some("t"), show, show, show, debug, false)));
        cx.borrow().set_emitter(Some(e.clone() as EmitterRef));
        let c = cx.borrow();
        let global = ObjectValue::with_builder(&c, new_global_builder(), None);
        c.push_scope(global.clone());
        let pub_ = c.public_namespace();
        let internal = c.get_namespace_kind("t.as$0", NS_INTERNAL);
        let priv_ = c.get_namespace_kind("t.as$1", NS_PRIVATE);
        let pkg = c.get_namespace("flash.display");
        pkg.set_package(true);
        let prot = c.get_namespace_kind("A", NS_PROTECTED);
        let sprot = c.get_namespace_kind("A", NS_STATIC_PROTECTED);
        drop(c);
        T { cx, e, global, pub_, internal, priv_, pkg, prot, sprot }
    }

    fn nss(a: &[&ObjectValue]) -> Namespaces {
        let n = Namespaces::new();
        for o in a {
            n.add((*o).clone());
        }
        n
    }

    fn md(id: &str, kv: &[Option<&str>]) -> MetaData {
        let m = MetaData::new();
        m.set_id(Some(id.to_string()));
        if !kv.is_empty() {
            let mut values = Vec::new();
            for p in kv.chunks(2) {
                values.push(match p[0] {
                    None => Value::KeylessValue(Rc::new(KeylessValue::new(p[1].unwrap()))),
                    Some(k) => Value::KeyValuePair(Rc::new(KeyValuePair::new(k, p[1].unwrap()))),
                });
            }
            m.set_values(Some(values));
        }
        m
    }

    impl T {
        fn def_var(&self, ob: &ObjectValue, name: &str, ns: &ObjectValue, ty: &TypeValue, init: Option<(&str, TypeValue)>, is_const: bool) -> Slot {
            let c = self.cx.borrow();
            let sid = ob.builder().unwrap().explicit_var_nss(&c, ob, name, &nss(&[ns]), Some(ty), -1, -1, -1);
            let s = ob.get_slot(&c, sid).unwrap();
            if let Some((v, t)) = init {
                s.set_object_value(Some(ObjectValue::with_value(v, Some(&t))));
            }
            if is_const {
                s.set_const(true);
            }
            s
        }

        fn def_method(&self, ob: &ObjectValue, name: &str, ns: &ObjectValue, method_name: &str, kind: i32) -> Slot {
            let c = self.cx.borrow();
            let b = ob.builder().unwrap();
            let n = nss(&[ns]);
            let no = c.no_type();
            let sid = if kind == GET_TOKEN {
                let mid = b.method(&c, ob, &format!("{}$get", name), &n, false);
                b.explicit_get_nss(&c, ob, name, &n, Some(&no), false, false, -1, mid, -1)
            } else if kind == SET_TOKEN {
                let mid = b.method(&c, ob, &format!("{}$set", name), &n, false);
                b.explicit_set_nss(&c, ob, name, &n, Some(&no), false, false, -1, mid, -1)
            } else {
                let mid = b.method(&c, ob, name, &n, false);
                b.explicit_call_nss(&c, ob, name, &n, Some(&no), false, false, -1, mid, -1)
            };
            let iid = ob.get_implicit_index(&c, sid, EMPTY_TOKEN);
            let s = ob.get_slot(&c, iid).unwrap();
            s.set_method_name(method_name);
            s
        }

        fn def_ctor(&self, ob: &ObjectValue, method_name: &str) {
            let c = self.cx.borrow();
            let b = ob.builder().unwrap();
            let n = nss(&[&self.pub_]);
            let mid = b.method(&c, ob, "$construct", &n, false);
            let sid = b.explicit_call_nss(&c, ob, "$construct", &n, Some(&c.no_type()), true, false, -1, mid, -1);
            ob.get_slot(&c, ob.get_implicit_index(&c, sid, EMPTY_TOKEN)).unwrap().set_method_name(method_name);
        }

        fn param(&self, act: &ObjectValue, ty: &TypeValue, init: Option<(&str, TypeValue)>) {
            let c = self.cx.borrow();
            let vc = act.var_count();
            act.set_var_count(vc + 1);
            let id = act.add_variable_slot(&c, Some(ty), vc);
            if let Some((v, t)) = init {
                act.get_slot(&c, id).unwrap().set_object_value(Some(ObjectValue::with_value(v, Some(&t))));
            }
        }

        fn activation(&self) -> ObjectValue {
            let c = self.cx.borrow();
            let act = ObjectValue::with_builder(&c, new_activation_builder(), None);
            c.push_scope(act.clone());
            act
        }

        fn pos(&self, ln: i32) {
            e!(self).set_position(ln, 1, ln * 10);
        }

        fn finish_init(&self) -> i32 {
            e!(self).finish_method(&self.cx, "$init", None, None, None, 0, 1, "", false, false, None)
        }

        fn int(&self) -> TypeValue {
            self.cx.borrow().int_type()
        }

        fn string(&self) -> TypeValue {
            self.cx.borrow().string_type()
        }

        fn double(&self) -> TypeValue {
            self.cx.borrow().double_type()
        }

        fn boolean(&self) -> TypeValue {
            self.cx.borrow().boolean_type()
        }

        fn uint(&self) -> TypeValue {
            self.cx.borrow().uint_type()
        }

        fn no(&self) -> TypeValue {
            self.cx.borrow().no_type()
        }

        fn qn(&self, ns: &ObjectValue, name: &str) -> QName {
            QName::new(Some(ns.clone()), name)
        }
    }

    fn prog(t: &T) {
        let (pub_, internal, priv_, pkg, prot, sprot) = (&t.pub_, &t.internal, &t.priv_, &t.pkg, &t.prot, &t.sprot);
        let g = &t.global;
        t.def_var(g, "vi", pub_, &t.int(), Some(("5", t.int())), false);
        let vs = t.def_var(g, "vs", internal, &t.string(), Some(("hi", t.string())), true);
        vs.add_metadata(Some(md("Foo", &[None, Some("a"), Some("k"), Some("v")])));
        vs.add_metadata(Some(md("Bar", &[])));
        let vd = t.def_var(g, "vd", pub_, &t.double(), Some(("1.5", t.double())), false);
        vd.add_metadata(Some(md("Foo", &[None, Some("a"), Some("k"), Some("v")])));
        t.def_var(g, "vu", pub_, &t.uint(), Some(("4000000000", t.uint())), false);
        t.def_var(g, "vb", priv_, &t.boolean(), Some(("true", t.boolean())), false);
        t.def_var(g, "vf", priv_, &t.boolean(), Some(("false", t.boolean())), true);
        let (object, null, void) = {
            let c = t.cx.borrow();
            (c.object_type(), c.null_type(), c.void_type())
        };
        t.def_var(g, "vn", pub_, &object, Some(("null", null)), false);
        t.def_var(g, "vv", pub_, &t.no(), Some(("undefined", void)), false);
        t.def_var(g, "vx", pkg, &t.no(), None, false);
        t.def_var(g, "vh", pub_, &t.int(), Some(("0x7f", t.int())), false);
        t.def_var(g, "vbig", pub_, &t.double(), Some(("12345678901", t.double())), false);
        let vns = t.def_var(g, "vns", pub_, &t.no(), None, true);
        vns.set_object_value(Some(internal.clone()));
        t.def_method(g, "gf", pub_, "gf", EMPTY_TOKEN);
        t.def_method(g, "gg", pub_, "gg/get", GET_TOKEN);

        e!(t).start_program("t");
        e!(t).set_origin("C:\\x\\t.as");
        e!(t).start_method("$init", 0, 0, 0, false, 0);
        t.pos(1);
        e!(t).get_global_scope();
        e!(t).push_scope();
        t.pos(2);
        e!(t).find_property_name_nss("vi", Some(&nss(&[pub_])), true, false, false);
        e!(t).push_number(&NumberConstant::Int(5), TYPE_int);
        e!(t).set_property_name_nss("vi", Some(&nss(&[pub_])), true, false, false, true);
        e!(t).find_property_name_nss("vs", Some(&nss(&[pub_, internal])), false, false, false);
        e!(t).push_string("hi");
        e!(t).set_property_name_nss("vs", Some(&nss(&[pub_, internal])), false, false, false, false);
        t.pos(3);
        let ints = [0, -1, 127, 128, -128, -129, 32767, 32768, -32768, -32769, 100000, -100000, i32::MAX, i32::MIN, 100000];
        for v in ints {
            e!(t).push_number(&NumberConstant::Int(v), TYPE_int);
            e!(t).pop();
        }
        let uints: [i64; 7] = [0, 127, 128, 0x7FFFF, 0x80000, 4000000000, 0xFFFFFFFF];
        for v in uints {
            e!(t).push_number(&NumberConstant::Uint(v), TYPE_uint);
            e!(t).pop();
        }
        let ds = [0.0, -0.0, 1.0, -1.0, 1.5, 300.0, 1e10, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 2147483648.0, 0.1, 1.5, -70000.0];
        for v in ds {
            e!(t).push_number(&NumberConstant::Double(v), TYPE_double);
            e!(t).pop();
        }
        t.pos(3);
        e!(t).push_string("hi");
        e!(t).pop();
        e!(t).push_string("h\u{e9}llo \u{4e2d}");
        e!(t).pop();
        e!(t).push_string("");
        e!(t).pop();
        e!(t).push_boolean(true);
        e!(t).push_boolean(false);
        e!(t).push_null();
        e!(t).push_undefined();
        e!(t).push_uninitialized();
        e!(t).push_empty();
        for _ in 0..6 {
            e!(t).pop();
        }
        let any = t.cx.borrow().any_namespace();
        for ns in [pub_, internal, priv_, pkg, prot, sprot, &any] {
            e!(t).push_namespace(ns);
        }
        for _ in 0..7 {
            e!(t).pop();
        }
        e!(t).new_namespace(pkg);
        t.pos(4);
        e!(t).find_property_name_nss("trace", Some(&nss(&[pub_, internal, pkg])), true, false, false);
        e!(t).push_string("x");
        e!(t).call_property("trace", Some(&nss(&[pub_, internal, pkg])), 1, false, false, false, false);
        e!(t).pop();
        e!(t).find_property_name_nss("Foo", Some(&nss(&[pkg])), true, true, false);
        e!(t).construct_property("Foo", Some(&nss(&[pkg])), 0, true, false, false);
        e!(t).pop();
        e!(t).find_property_name_nss("*", Some(&nss(&[pub_])), false, true, false);
        e!(t).call_property("bar", Some(&nss(&[pub_, priv_])), 0, false, false, false, true);
        e!(t).call_property("bar", Some(&nss(&[pub_])), 0, true, true, false, false);
        e!(t).pop();
        e!(t).get_global_scope();
        e!(t).get_property_name_nss("vi", Some(&nss(&[pub_])), true, false, false);
        e!(t).get_property_name_nss("vi", Some(&nss(&[pub_])), true, true, false);
        e!(t).get_property_name_nss("a", Some(&nss(&[pub_])), false, false, true);
        e!(t).get_property_name_nss("*", Some(&nss(&[pub_, internal])), false, false, false);
        e!(t).pop();
        t.pos(5);
        {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.push_string("n");
            e.get_property(false, false, false, Some(&nss(&[pub_, internal])));
            e.load_this();
            e.push_namespace(pub_);
            e.push_string("n");
            e.get_property(true, false, false, None);
            e.load_this();
            e.push_namespace(pub_);
            e.push_string("n");
            e.get_property(true, true, true, None);
            e.load_this();
            e.push_namespace(pub_);
            e.get_property_name("n", false, false);
            e.load_this();
            e.push_namespace(pub_);
            e.get_property_name("n", true, true);
            e.load_this();
            e.push_namespace(pub_);
            e.get_property_name("*", false, false);
            for _ in 0..6 {
                e.pop();
            }
            e.load_this();
            e.push_string("n");
            e.push_null();
            e.set_property(false, false, false, Some(&nss(&[pub_])), false);
            e.load_this();
            e.push_namespace(pub_);
            e.push_string("n");
            e.push_null();
            e.set_property(true, false, true, None, true);
            e.load_this();
            e.push_string("n");
            e.push_null();
            e.set_property(false, true, false, Some(&nss(&[pub_])), false);
            e.load_this();
            e.push_namespace(pub_);
            e.push_null();
            e.set_property_name("n", false, false);
            e.load_this();
            e.push_namespace(pub_);
            e.push_null();
            e.set_property_name("n", true, true);
            e.load_this();
            e.push_null();
            e.set_property_name_nss("m", Some(&nss(&[pub_, internal])), false, false, true, false);
            e.load_this();
            e.push_null();
            e.set_property_name_nss("m", Some(&nss(&[priv_])), true, true, false, false);
            e.load_this();
            e.push_string("n");
            e.delete_property(false, false, false, Some(&nss(&[pub_])));
            e.pop();
            e.load_this();
            e.push_namespace(pub_);
            e.push_string("n");
            e.delete_property(true, false, false, None);
            e.pop();
            e.load_this();
            e.push_namespace(pub_);
            e.delete_property_name("n", false, true);
            e.pop();
            e.load_this();
            e.delete_property_name_nss("n", Some(&nss(&[pub_, internal])), false, false, false);
            e.pop();
            e.load_this();
            e.delete_property_name_nss("n", Some(&nss(&[pub_])), true, true, false);
            e.pop();
            e.load_this();
            e.push_string("n");
            e.get_descendants(false, false, Some(&nss(&[pub_])));
            e.pop();
            e.load_this();
            e.push_namespace(pub_);
            e.push_string("n");
            e.get_descendants(true, true, None);
            e.pop();
            e.load_this();
            e.push_namespace(pub_);
            e.get_descendants_name("n", false, false);
            e.pop();
            e.load_this();
            e.get_descendants_name_nss("d", Some(&nss(&[pub_, internal])), false, false, true);
            e.pop();
            e.load_this();
            e.push_namespace(pub_);
            e.delete_descendants_name("n", false, false);
            e.pop();
            e.load_this();
            e.delete_descendants_name_nss("d", Some(&nss(&[pub_])), true, false, false);
            e.pop();
            e.push_string("n");
            e.find_property(true, false, false, Some(&nss(&[pub_, pkg])));
            e.pop();
            e.push_namespace(pub_);
            e.push_string("n");
            e.find_property(false, true, true, None);
            e.pop();
            e.push_namespace(pub_);
            e.find_property_name("n", true, false);
            e.pop();
            e.push_namespace(pub_);
            e.find_property_name("*", false, true);
            e.pop();
        }
        t.pos(6);
        let bins = [
            BINARY_BinaryPlusOp, BINARY_BinaryPlusOp_II, BINARY_BinaryMinusOp, BINARY_BinaryMinusOp_II, BINARY_MultiplyOp, BINARY_MultiplyOp_II,
            BINARY_DivideOp, BINARY_ModulusOp, BINARY_LeftShiftOp, BINARY_LeftShiftOp_II, BINARY_RightShiftOp, BINARY_RightShiftOp_II,
            BINARY_UnsignedRightShiftOp, BINARY_UnsignedRightShiftOp_II, BINARY_LessThanOp, BINARY_GreaterThanOp, BINARY_LessThanOrEqualOp,
            BINARY_GreaterThanOrEqualOp, BINARY_InstanceofOp, BINARY_InOp, BINARY_IsOp, BINARY_IsLateOp, BINARY_EqualsOp, BINARY_EqualsOp_II,
            BINARY_NotEqualsOp, BINARY_NotEqualsOp_II, BINARY_StrictEqualsOp, BINARY_StrictEqualsOp_II, BINARY_StrictNotEqualsOp,
            BINARY_StrictNotEqualsOp_II, BINARY_BitwiseAndOp, BINARY_BitwiseAndOp_II, BINARY_BitwiseXorOp, BINARY_BitwiseXorOp_II,
            BINARY_BitwiseOrOp, BINARY_BitwiseOrOp_II, BINARY_LogicalAndOp, BINARY_LogicalOrOp, BINARY_AsOp, BINARY_AsLateOp,
        ];
        for op in bins {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.load_this();
            e.invoke_binary(op, None);
            e.pop();
        }
        let uns = [
            UNARY_TypeofOp, UNARY_TypeofOp_B, UNARY_TypeofOp_I, UNARY_TypeofOp_D, UNARY_TypeofOp_M, UNARY_TypeofOp_S, UNARY_TypeofOp_U,
            UNARY_IncrementOp, UNARY_IncrementOp_I, UNARY_IncrementLocalOp, UNARY_IncrementLocalOp_I, UNARY_DecrementOp, UNARY_DecrementOp_I,
            UNARY_DecrementLocalOp, UNARY_DecrementLocalOp_I, UNARY_UnaryPlusOp, UNARY_UnaryPlusOp_I, UNARY_UnaryPlusOp_M, UNARY_UnaryMinusOp,
            UNARY_UnaryMinusOp_I, UNARY_BitwiseNotOp, UNARY_BitwiseNotOp_I, UNARY_LogicalNotOp, UNARY_LogicalNotOp_B, UNARY_LogicalNotOp_I,
            UNARY_Put, UNARY_Get, UNARY_HasMoreNames, UNARY_DeleteOp, UNARY_NextName, UNARY_NextValue, UNARY_VoidOp, UNARY_ToXMLString,
            UNARY_ToXMLAttrString, UNARY_CheckFilterOp, UNARY_Call,
        ];
        for op in uns {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.invoke_unary(op, 1, 2, Some(&nss(&[pub_, internal])), None);
            e.pop();
        }
        t.pos(7);
        {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.to_boolean(TYPE_none);
            e.to_int();
            e.to_uint();
            e.to_double(TYPE_int);
            e.to_decimal(TYPE_int);
            e.to_object();
            e.to_string_();
            e.to_native_bool();
            e.check_type(&t.qn(pub_, "int"));
            e.check_type(&t.qn(pub_, "String"));
            e.dup();
            e.check_type(&t.qn(pub_, "*"));
            e.pop();
            e.check_type(&t.qn(pub_, "Number"));
            e.pop();
            e.swap();
            e.check_type(&t.qn(pub_, "Boolean"));
            e.push_scope();
            e.load_this();
            e.check_type(&t.qn(pub_, "uint"));
            e.pop();
            e.load_this();
            e.check_type(&t.qn(pkg, "Sprite"));
            e.check_type(&t.qn(pub_, ""));
            e.pop();
            e.pop_scope();
        }
        t.pos(8);
        {
            let mut e = t.e.borrow_mut();
            let tt = e.allocate_temp();
            let t2 = e.allocate_temp();
            e.load_this();
            e.store_register_name(tt + 1, TYPE_none, "x");
            e.load_register(tt + 1, TYPE_none);
            e.pop();
            e.load_this();
            e.store_register_name(t2 + 1, TYPE_int, "");
            e.load_register(5, TYPE_int);
            e.store_register_name(4, TYPE_int, "");
            e.kill(t2);
            e.free_temp(t2);
            e.free_temp(tt);
            e.load_global(2, TYPE_int);
            e.pop();
            e.push_null();
            e.store_global(3, TYPE_none);
            e.get_base_object(0);
            e.load_var(1);
            e.pop();
            e.get_base_object(1);
            e.push_null();
            e.store_var(2);
            e.push_null();
            e.push_null();
            e.new_array(2);
            e.pop();
            e.push_string("k");
            e.push_null();
            e.new_object(1);
            e.pop();
            e.new_array(0);
            e.new_object(0);
            e.pop();
            e.pop();
            e.get_scope_chain();
            e.get_scope_on_top();
            e.new_function_object("gf");
            e.pop();
            e.new_activation();
            e.pop();
            e.new_catch(0);
            e.pop();
            e.load_this();
            e.push_with();
            e.pop_with();
        }
        t.pos(9);
        {
            let mut e = t.e.borrow_mut();
            e.find_property_name_nss("Vector", Some(&nss(&[pub_])), true, false, false);
            e.get_property_name_nss("Vector", Some(&nss(&[pub_])), false, false, false);
            e.get_global_scope();
            e.get_property_name_nss("int", Some(&nss(&[pub_])), true, false, false);
            e.apply_type(1);
            e.pop();
            e.load_this();
            e.push_null();
            e.push_null();
            e.invoke_closure(false, 1);
            e.pop();
            e.load_this();
            e.push_null();
            e.invoke_closure(true, 1);
            e.pop();
            e.load_this();
            e.push_null();
            e.invoke_method(true, 3, 1);
            e.pop();
            e.load_this();
            e.invoke_method(false, 4, 0);
            e.pop();
            e.load_this();
            e.push_null();
            e.invoke_super(true, 1);
            e.default_xml_namespace_name("http://x");
            e.push_string("y");
            e.default_xml_namespace();
            e.has_next(1, 2);
            e.pop();
            e.push_null();
            e.throw();
            e.return_(TYPE_void);
            e.return_(TYPE_int);
        }
        let init = t.finish_init();
        e!(t).finish_program(&t.cx, "", init);
    }

    fn func(t: &T) {
        let (pub_, internal) = (&t.pub_, &t.internal);
        e!(t).start_program("t");
        e!(t).set_origin("t.as");
        t.def_method(&t.global, "f", pub_, "f", EMPTY_TOKEN);
        t.def_method(&t.global, "g", internal, "pkg:g", EMPTY_TOKEN);
        let (null, void, array) = {
            let c = t.cx.borrow();
            (c.null_type(), c.void_type(), c.array_type())
        };

        let act = t.activation();
        t.param(&act, &t.int(), None);
        t.param(&act, &t.string(), Some(("x", t.string())));
        t.param(&act, &t.double(), Some(("2.5", t.double())));
        t.param(&act, &t.boolean(), Some(("true", t.boolean())));
        t.param(&act, &t.int(), Some(("-3", t.int())));
        t.param(&act, &t.uint(), Some(("7", t.uint())));
        t.param(&act, &t.no(), Some(("null", null)));
        t.param(&act, &t.no(), None);
        t.param(&act, &t.no(), Some(("undefined", void.clone())));
        t.param(&act, &array, None);
        e!(t).start_method("f", 9, 2, 1, false, METHOD_Needrest);
        t.pos(10);
        e!(t).define_slot_variable(&t.cx, "a", "f", 100, None, 1);
        e!(t).define_slot_variable(&t.cx, "b", "f", 100, None, 2);
        e!(t).load_register(1, TYPE_int);
        t.pos(11);
        e!(t).load_register(2, TYPE_none);
        e!(t).invoke_binary(BINARY_BinaryPlusOp, None);
        e!(t).return_(TYPE_int);
        let names: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h", "i"].iter().map(|s| s.to_string()).collect();
        let ti = t.int().get_default_type_info();
        e!(t).finish_method(&t.cx, "f", Some(&ti), None, None, METHOD_Needrest, 2, "f", false, false, Some(&names));
        t.cx.borrow().pop_scope();

        let act2 = t.activation();
        t.def_var(&act2, "loc", internal, &t.int(), None, false);
        t.def_var(&act2, "k", internal, &t.string(), Some(("kk", t.string())), true);
        e!(t).start_method("pkg:g", 0, 0, 2, true, 1);
        t.pos(20);
        {
            let mut e = t.e.borrow_mut();
            e.new_activation();
            e.dup();
            e.push_scope();
            e.define_slot_variable(&t.cx, "loc", "g", 200, None, 1);
            e.get_scope_on_top();
            e.push_number(&NumberConstant::Int(1), TYPE_int);
            e.set_property_name_nss("loc", Some(&nss(&[internal])), true, false, false, false);
            e.default_xml_namespace_name("ns");
            e.return_(TYPE_void);
            e.finish_method(&t.cx, "pkg:g", None, None, Some(&act2), METHOD_Arguments, 1, "pkg:g", false, false, None);
        }
        t.cx.borrow().pop_scope();

        let act3 = t.activation();
        t.param(&act3, &t.string(), None);
        {
            let mut e = t.e.borrow_mut();
            e.start_method("n", 1, 0, 0, false, 0);
            let vt = void.get_default_type_info();
            e.finish_method(&t.cx, "n", Some(&vt), None, None, 0, 1, "my.pkg:n", true, false, Some(&["s".to_string()]));
            e.start_method("im", 1, 0, 0, false, 0);
            e.finish_method(&t.cx, "im", None, None, None, 0, 1, "I/im", false, true, None);
        }
        t.cx.borrow().pop_scope();

        e!(t).start_method("X$iinit", 0, 0, 0, false, 0);
        t.pos(30);
        {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.pop();
            e.return_(TYPE_void);
            e.finish_method(&t.cx, "X$iinit", None, None, None, 0, 3, "X", false, false, None);
            e.start_method("$init", 0, 0, 0, false, 0);
        }
        t.pos(40);
        {
            let mut e = t.e.borrow_mut();
            e.get_global_scope();
            e.push_scope();
            e.new_function_object("f");
            e.pop();
            e.return_(TYPE_void);
        }
        let init = t.finish_init();
        e!(t).finish_program(&t.cx, "", init);

        e!(t).start_method("$init", 0, 0, 0, false, 0);
        e!(t).return_(TYPE_void);
        let init2 = t.finish_init();
        e!(t).finish_program(&t.cx, "second", init2);
    }

    fn cls(t: &T) {
        let (pub_, internal, pkg, prot, sprot) = (&t.pub_, &t.internal, &t.pkg, &t.prot, &t.sprot);
        e!(t).start_program("t");
        e!(t).set_origin("A.as");

        let (iname, iframe_c, iiframe, ins) = {
            let c = t.cx.borrow();
            let iname = t.qn(pkg, "I");
            let icb = new_class_builder(iname.clone(), None, None);
            icb.class_data_mut().is_interface = true;
            let iframe_c = TypeValue::define_type_value(&c, icb, &iname, TYPE_object);
            let iib = new_instance_builder(iname.clone());
            let iiframe = ObjectValue::with_builder(&c, iib, Some(&iframe_c));
            iframe_c.set_prototype(Some(iiframe.clone()));
            let ins = c.get_namespace("flash.display:I");
            (iname, iframe_c, iiframe, ins)
        };
        t.def_method(&iiframe, "im", &ins, "flash.display:I/flash.display:I::im", EMPTY_TOKEN);
        t.def_ctor(&iiframe, "flash.display:I$iinit");

        let (cname, cframe, iframe) = {
            let c = t.cx.borrow();
            let cname = t.qn(pkg, "A");
            let cb = new_class_builder(cname.clone(), Some(prot.clone()), Some(sprot.clone()));
            let cframe = TypeValue::define_type_value(&c, cb, &cname, TYPE_object);
            let ib = new_instance_builder(cname.clone());
            let iframe = ObjectValue::with_builder(&c, ib.clone(), Some(&cframe));
            cframe.set_prototype(Some(iframe.clone()));
            ib.instance_data_mut().interface_refs.push(ReferenceValue::new(&c, None, "I", Some(pkg)));
            (cname, cframe, iframe)
        };

        let type_type = t.cx.borrow().type_type();
        let cs = t.def_var(&t.global, "A", pkg, &type_type, None, true);
        cs.set_object_value(Some(cframe.as_object().clone()));
        let is = t.def_var(&t.global, "I", pkg, &type_type, None, true);
        is.set_object_value(Some(iframe_c.as_object().clone()));

        let cfo = cframe.as_object().clone();
        t.def_var(&cfo, "sv", pub_, &t.int(), Some(("3", t.int())), false);
        t.def_var(&cfo, "SC", sprot, &t.string(), Some(("c", t.string())), true);
        t.def_method(&cfo, "sm", pub_, "flash.display:A/sm", EMPTY_TOKEN);

        t.def_var(&iframe, "iv", internal, &t.double(), None, false);
        t.def_var(&iframe, "pv", prot, &cframe, None, false);
        let m = t.def_method(&iframe, "m", pub_, "flash.display:A/m", EMPTY_TOKEN);
        m.add_metadata(Some(md("Inspectable", &[Some("name"), Some("m")])));
        t.def_method(&iframe, "g", pub_, "flash.display:A/get:g", GET_TOKEN);
        t.def_method(&iframe, "g", pub_, "flash.display:A/set:g", SET_TOKEN);
        t.def_method(&iframe, "im", &ins, "flash.display:A/flash.display:I::im", EMPTY_TOKEN);
        t.def_ctor(&iframe, "flash.display:A");

        e!(t).start_class("I");
        t.cx.borrow().push_scope(iframe_c.as_object().clone());
        {
            let mut e = t.e.borrow_mut();
            e.start_method("flash.display:I$cinit", 0, 0, 0, false, 0);
            e.return_(TYPE_void);
            e.finish_method(&t.cx, "flash.display:I$cinit", None, None, None, 0, 2, "", false, false, None);
        }
        t.cx.borrow().push_scope(iiframe.clone());
        {
            let mut e = t.e.borrow_mut();
            e.start_method("flash.display:I/flash.display:I::im", 0, 0, 0, false, 0);
            e.finish_method(&t.cx, "flash.display:I/flash.display:I::im", None, None, None, 0, 3, "", false, true, None);
            e.start_method("flash.display:I$iinit", 0, 0, 0, false, 0);
            e.finish_method(&t.cx, "flash.display:I$iinit", None, None, None, 0, 3, "", false, true, None);
            e.finish_class(&t.cx, &iname, None, false, false, true, true);
        }
        t.cx.borrow().pop_scope();
        t.cx.borrow().pop_scope();

        e!(t).start_class("A");
        t.cx.borrow().push_scope(cfo.clone());
        e!(t).start_method("flash.display:A$cinit", 0, 0, 0, false, 0);
        t.pos(2);
        {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.return_(TYPE_void);
            e.finish_method(&t.cx, "flash.display:A$cinit", None, None, None, 0, 2, "", false, false, None);
            e.start_method("flash.display:A/sm", 0, 0, 0, false, 0);
        }
        t.pos(3);
        {
            let mut e = t.e.borrow_mut();
            e.return_(TYPE_void);
            e.finish_method(&t.cx, "flash.display:A/sm", None, None, None, 0, 2, "A/sm", false, false, None);
        }
        t.cx.borrow().push_scope(iframe.clone());
        let act = t.activation();
        t.param(&act, &t.int(), None);
        e!(t).start_method("flash.display:A", 1, 0, 0, false, 0);
        t.pos(5);
        {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.push_scope();
            e.load_this();
            e.invoke_super(true, 0);
            e.return_(TYPE_void);
            e.finish_method(&t.cx, "flash.display:A", None, None, None, 0, 4, "A", false, false, Some(&["x".to_string()]));
            e.start_method("flash.display:A/m", 1, 1, 0, false, 0);
        }
        t.pos(6);
        {
            let mut e = t.e.borrow_mut();
            e.load_this();
            e.push_scope();
            e.get_base_object(1);
            e.pop();
            e.return_(TYPE_void);
            let vt = t.cx.borrow().void_type().get_default_type_info();
            e.finish_method(&t.cx, "flash.display:A/m", Some(&vt), None, None, 0, 4, "A/m", false, false, Some(&["p".to_string()]));
        }
        t.cx.borrow().pop_scope();
        for n in ["flash.display:A/get:g", "flash.display:A/set:g", "flash.display:A/flash.display:I::im"] {
            let mut e = t.e.borrow_mut();
            e.start_method(n, 0, 0, 0, false, 0);
            e.return_(TYPE_void);
            e.finish_method(&t.cx, n, None, None, None, 0, 3, n, false, false, None);
        }
        e!(t).finish_class(&t.cx, &cname, Some(&t.qn(pub_, "Object")), false, true, false, true);
        t.cx.borrow().pop_scope();
        t.cx.borrow().pop_scope();

        e!(t).start_method("$init", 0, 0, 0, false, 0);
        t.pos(1);
        {
            let mut e = t.e.borrow_mut();
            e.get_global_scope();
            e.push_scope();
            e.get_global_scope();
            e.find_property_name_nss("Object", Some(&nss(&[pub_])), true, false, false);
            e.get_property_name_nss("Object", Some(&nss(&[pub_])), true, false, false);
            e.push_scope();
            e.find_property_name_nss("Object", Some(&nss(&[pub_])), true, false, false);
            e.get_property_name_nss("Object", Some(&nss(&[pub_])), true, false, false);
            e.new_class_object(&cname);
            e.pop_scope();
            e.set_property_name_nss("A", Some(&nss(&[pkg])), true, false, false, true);
            e.get_global_scope();
            e.push_null();
            e.new_class_object(&iname);
            e.set_property_name_nss("I", Some(&nss(&[pkg])), true, false, false, true);
            e.return_(TYPE_void);
        }
        let init = t.finish_init();
        e!(t).finish_program(&t.cx, "", init);
    }

    fn exc(t: &T) {
        let pub_ = &t.pub_;
        let mut e = t.e.borrow_mut();
        e.start_program("t");
        e.set_origin("e.as");
        e.start_method("$init", 0, 1, 2, false, 0);
        e.set_position(1, 1, 10);
        e.get_global_scope();
        e.push_scope();
        let (err, no) = {
            let c = t.cx.borrow();
            (c.object_type(), c.no_type())
        };

        e.try_(true);
        e.set_position(2, 1, 20);
        e.try_(false);
        e.push_null();
        e.throw();
        e.catch_clauses_begin();
        e.catch(Some(&err), Some(&t.qn(pub_, "e")));
        e.new_catch(0);
        e.dup();
        let tt = e.allocate_temp();
        e.store_register_name(tt + 1, TYPE_none, "");
        e.push_scope();
        e.pop();
        e.pop_scope();
        e.kill(tt);
        e.free_temp(tt);
        e.catch(Some(&no), None);
        e.pop();
        e.catch_clauses_end();
        e.call_finally(1);
        e.catch_clauses_begin();
        e.catch(Some(&no), None);
        e.pop();
        e.call_finally(-1);
        e.throw();
        e.catch_clauses_end();
        e.finally_clause_begin();
        e.set_position(3, 1, 30);
        e.push_null();
        e.pop();
        e.finally_clause_end();

        e.label_statement_begin();
        e.loop_begin();
        e.set_position(4, 1, 40);
        e.load_this();
        e.if_(IF_false);
        e.break_(1);
        e.else_();
        let ip = e.get_ip();
        e.patch_if(ip);
        e.continue_(1);
        let ip = e.get_ip();
        e.patch_else(ip);
        e.break_(0);
        let ip = e.get_ip();
        e.patch_loop_begin(ip);
        e.patch_continue(1);
        e.load_this();
        e.load_this();
        e.loop_end(IF_lt);
        e.patch_break(1);
        e.label_statement_end(0);

        let kinds = [IF_false, IF_true, IF_lt, IF_le, IF_gt, IF_ge, IF_eq, IF_ne, IF_stricteq, IF_strictne, IF_nlt, IF_nle, IF_ngt, IF_nge];
        for k in kinds {
            e.load_this();
            e.load_this();
            e.if_(k);
            e.push_null();
            e.pop();
            let ip = e.get_ip();
            e.patch_if(ip);
        }
        for k in kinds {
            e.loop_begin();
            let ip = e.get_ip();
            e.patch_loop_begin(ip);
            e.load_this();
            e.load_this();
            e.loop_end(k);
            e.patch_break(0);
            e.patch_continue(0);
        }

        e.loop_begin();
        e.load_this();
        e.push_with();
        e.try_(true);
        e.break_(0);
        e.continue_(0);
        e.catch_clauses_begin();
        e.catch(Some(&no), None);
        e.throw();
        e.catch_clauses_end();
        e.finally_clause_begin();
        e.finally_clause_end();
        e.pop_with();
        let ip = e.get_ip();
        e.patch_loop_begin(ip);
        e.patch_continue(0);
        e.push_boolean(true);
        e.loop_end(IF_true);
        e.patch_break(0);

        e.load_this();
        e.switch_begin();
        e.case_label(false);
        e.set_position(6, 1, 60);
        e.push_null();
        e.pop();
        e.case_label(false);
        e.case_label(false);
        e.break_(0);
        e.case_label(true);
        e.break_(0);
        let ip = e.get_ip();
        e.patch_switch_begin(ip);
        e.push_case_index(0);
        e.switch_table();
        e.patch_break(0);

        e.loop_begin();
        e.switch_begin();
        e.case_label(false);
        e.continue_(0);
        let ip = e.get_ip();
        e.patch_switch_begin(ip);
        e.push_case_index(70000);
        e.switch_table();
        e.patch_break(1);
        let ip = e.get_ip();
        e.patch_loop_begin(ip);
        e.patch_continue(0);
        e.push_boolean(true);
        e.loop_end(IF_true);
        e.patch_break(0);

        e.return_(TYPE_void);
        let init = e.finish_method(&t.cx, "$init", None, None, None, 0, 1, "", false, false, None);
        e.finish_program(&t.cx, "", init);
    }

    fn run(scenario: fn(&T), name: &str, debug: bool, show: bool) -> String {
        let t = setup(debug, show);
        scenario(&t);
        let mut bytes = Vec::new();
        e!(t).emit(&mut bytes);
        let mut out: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        out.push('\n');
        if show {
            out.push_str(&format!("IL:{}\n", e!(t).il_str()));
            let hdr = e!(t).header_str();
            out.push_str(&format!("HDR:{}\n", hdr.as_deref().unwrap_or("null")));
        }
        out.push_str(&format!("ERR:{}\n", t.cx.borrow().error_count()));
        if let Ok(dir) = std::env::var("ASC_ABE_DUMP") {
            let tag = if show { "show".to_string() } else { (debug as i32).to_string() };
            std::fs::write(format!("{}/r_{}_{}.txt", dir, name, tag), &out).unwrap();
        }
        out
    }

    fn fnv(s: &str) -> u64 {
        s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
    }

    fn check(scenario: fn(&T), name: &str, expected: [(usize, u64); 3]) {
        let got: Vec<(usize, u64)> = [(false, false), (true, false), (true, true)]
            .into_iter()
            .map(|(debug, show)| {
                let out = run(scenario, name, debug, show);
                (out.len(), fnv(&out))
            })
            .collect();
        assert_eq!(got, expected, "{} (plain, debug, debug+show)", name);
    }

    #[test]
    fn program_constants_names_and_instructions() {
        check(prog, "prog", [(2839, 0xc6367d6ea9e09642), (2899, 0x28c339fd0a9990a7), (47601, 0xe0020ad56201790c)]);
    }

    #[test]
    fn method_signatures_optional_rest_native_interface() {
        check(func, "func", [(607, 0x1d9ff1b5b894928c), (771, 0xf8d9ae7e737c1318), (9573, 0x93a90f508fc6d8f7)]);
    }

    #[test]
    fn classes_interfaces_and_traits() {
        check(cls, "class", [(883, 0xf85271082f1ede2c), (1115, 0xbac62ed88c9396af), (14343, 0xb7052d8383cd52cd)]);
    }

    #[test]
    fn exceptions_loops_switches_and_finally() {
        check(exc, "exc", [(1101, 0x53d6617c2fe7372c), (1135, 0x4eb6ba660028a7a3), (14108, 0x2bbfcf128daa13e2)]);
    }
}
