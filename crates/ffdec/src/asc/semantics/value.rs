//! Placeholders for the `macromedia.asc.semantics` and `macromedia.asc.util` types the AST references.

#![allow(non_upper_case_globals)]

use std::fmt;

use crate::asc::util::context::Context;

const PLACEHOLDER: &str = "placeholder semantics type, not ported yet";

/// Placeholder for the `Context`/`ContextStatics` members the AST uses that `util/context.rs` does not have yet.
pub trait PendingContext {
    /// `cx.scriptAssistParsing` (Java default false).
    fn script_assist_parsing(&self) -> bool {
        false
    }
    fn no_type(&self) -> TypeValue {
        todo!("{PLACEHOLDER}")
    }
    fn int_type(&self) -> TypeValue {
        todo!("{PLACEHOLDER}")
    }
    fn scope(&self) -> ObjectValue {
        todo!("{PLACEHOLDER}")
    }
    /// `cx.parser.error(code)`.
    fn parser_error(&self, _code: i32) {
        todo!("{PLACEHOLDER}")
    }
}

impl PendingContext for Context {}

/// `ContextStatics.omitTrace` (Java default false).
pub fn omit_trace() -> bool {
    false
}

#[derive(Debug, Clone, Default)]
pub struct Value;

impl Value {
    pub fn ptr_eq(&self, _other: &Value) -> bool {
        todo!("{PLACEHOLDER}")
    }
    pub fn as_keyless_value(&self) -> Option<KeylessValue> {
        todo!("{PLACEHOLDER}")
    }
    pub fn as_key_value_pair(&self) -> Option<KeyValuePair> {
        todo!("{PLACEHOLDER}")
    }
    pub fn as_reference_value(&self) -> Option<ReferenceValue> {
        todo!("{PLACEHOLDER}")
    }
    pub fn as_object_value(&self) -> Option<ObjectValue> {
        todo!("{PLACEHOLDER}")
    }
    pub fn as_type_value(&self) -> Option<TypeValue> {
        todo!("{PLACEHOLDER}")
    }
}

/// `MetaDataEvaluator.KeylessValue`.
#[derive(Debug, Clone, Default)]
pub struct KeylessValue {
    pub obj: String,
}

/// `MetaDataEvaluator.KeyValuePair`.
#[derive(Debug, Clone, Default)]
pub struct KeyValuePair {
    pub key: String,
    pub obj: String,
}

#[derive(Debug, Clone, Default)]
pub struct ObjectValue;

impl ObjectValue {
    /// `ObjectValue.internalNamespace`.
    pub fn internal_namespace() -> ObjectValue {
        todo!("{PLACEHOLDER}")
    }
    pub fn builder(&self) -> Builder {
        todo!("{PLACEHOLDER}")
    }
    pub fn name(&self) -> String {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_value(&self) -> String {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_namespace_kind(&self) -> u8 {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeValue;

impl TypeValue {
    pub fn builder(&self) -> Builder {
        todo!("{PLACEHOLDER}")
    }
    pub fn name(&self) -> QName {
        todo!("{PLACEHOLDER}")
    }
    pub fn indexed_type(&self) -> Option<TypeValue> {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_default_type_info(&self) -> TypeInfo {
        todo!("{PLACEHOLDER}")
    }
}

impl fmt::Display for TypeValue {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ReferenceValue;

impl ReferenceValue {
    pub fn new(_cx: &mut Context, _base: Option<ObjectValue>, _name: &str, _qualifier: ObjectValue) -> ReferenceValue {
        todo!("{PLACEHOLDER}")
    }
    pub fn slot(&self) -> Option<Slot> {
        todo!("{PLACEHOLDER}")
    }
    pub fn name(&self) -> String {
        todo!("{PLACEHOLDER}")
    }
    pub fn type_params(&self) -> Option<Vec<ReferenceValue>> {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_slot(&self, _cx: &mut Context) -> Option<Slot> {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_slot_kind(&self, _cx: &mut Context, _kind: i32) -> Option<Slot> {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Slot;

impl Slot {
    pub fn get_def_bits(&self) -> Option<BitSet> {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_debug_name(&self) -> String {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_value(&self) -> Option<Value> {
        todo!("{PLACEHOLDER}")
    }
    pub fn is_const(&self) -> bool {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Builder;

impl Builder {
    pub fn is_activation_builder(&self) -> bool {
        todo!("{PLACEHOLDER}")
    }
    pub fn is_class_builder(&self) -> bool {
        todo!("{PLACEHOLDER}")
    }
    pub fn is_instance_builder(&self) -> bool {
        todo!("{PLACEHOLDER}")
    }
    pub fn classname(&self) -> QName {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct QName;

impl QName {
    pub fn ns(&self) -> ObjectValue {
        todo!("{PLACEHOLDER}")
    }
    pub fn name(&self) -> String {
        todo!("{PLACEHOLDER}")
    }
    pub fn is_parameterized_name(&self) -> bool {
        todo!("{PLACEHOLDER}")
    }
}

impl fmt::Display for QName {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParameterizedName;

#[derive(Debug, Clone, Default)]
pub struct TypeInfo;

impl TypeInfo {
    pub fn ptr_eq(&self, _other: &TypeInfo) -> bool {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct MetaData {
    pub id: Option<String>,
    pub values: Option<Vec<Value>>,
}

impl MetaData {
    pub fn new() -> MetaData {
        MetaData { id: None, values: None }
    }
    pub fn get_value_key(&self, _key: &str) -> Option<String> {
        todo!("{PLACEHOLDER}")
    }
    pub fn get_value_index(&self, _index: i32) -> Option<String> {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct NumberConstant;

#[derive(Debug, Clone, Default)]
pub struct NumberUsage;

impl NumberUsage {
    pub const use_Number: i32 = 0;
    pub const use_decimal: i32 = 1;
    pub const use_double: i32 = 2;
    pub const use_int: i32 = 3;
    pub const use_uint: i32 = 4;
    pub const round_CEILING: i32 = 0;
    pub const round_UP: i32 = 1;
    pub const round_HALF_UP: i32 = 2;
    pub const round_HALF_EVEN: i32 = 3;
    pub const round_HALF_DOWN: i32 = 4;
    pub const round_DOWN: i32 = 5;
    pub const round_FLOOR: i32 = 6;
    pub const roundingModeName: [&'static str; 7] =
        ["CEILING", "UP", "HALF_UP", "HALF_EVEN", "HALF_DOWN", "DOWN", "FLOOR"];
}

#[derive(Debug, Clone, Default)]
pub struct Block;

impl Block {
    /// `block.succs.size()`.
    pub fn succs_size(&self) -> usize {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct BitSet;

impl BitSet {
    pub fn or(_a: Option<BitSet>, _b: Option<BitSet>) -> Option<BitSet> {
        todo!("{PLACEHOLDER}")
    }
    pub fn xor(_a: Option<BitSet>, _b: Option<BitSet>) -> Option<BitSet> {
        todo!("{PLACEHOLDER}")
    }
    pub fn reset_set(_target: Option<BitSet>, _r: Option<BitSet>, _s: Option<BitSet>) -> Option<BitSet> {
        todo!("{PLACEHOLDER}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Namespaces;

impl Namespaces {
    pub fn new() -> Namespaces {
        Namespaces
    }
    pub fn with_capacity(_capacity: usize) -> Namespaces {
        Namespaces
    }
}

#[derive(Debug, Clone, Default)]
pub struct Multinames;

impl Multinames {
    pub fn new() -> Multinames {
        Multinames
    }
}

#[derive(Debug, Clone, Default)]
pub struct ByteList;

impl ByteList {
    pub fn with_capacity(_capacity: usize) -> ByteList {
        ByteList
    }
    pub fn push_back(&mut self, _b: i8) {
        todo!("{PLACEHOLDER}")
    }
}
