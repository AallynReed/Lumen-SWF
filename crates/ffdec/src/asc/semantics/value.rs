//! Port of `macromedia/asc/semantics/Value.java`: the root of the value hierarchy, here an enum over its subclasses
//! (`ObjectValue` and its subclasses, `ReferenceValue`, and `MetaDataEvaluator`'s `KeylessValue`/`KeyValuePair`).

#![allow(non_upper_case_globals)]

use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use crate::asc::semantics::object_value::ObjectValue;
pub use crate::asc::semantics::reference_value::ReferenceValue;
pub use crate::asc::semantics::type_info::TypeInfo;
pub use crate::asc::semantics::type_value::TypeValue;
use crate::asc::util::context::Context;

// The model types the AST imports through `value::*`.
pub use crate::asc::semantics::builder::Builder;
pub use crate::asc::semantics::meta_data::MetaData;
pub use crate::asc::semantics::q_name::QName;
pub use crate::asc::semantics::slot::Slot;
pub use crate::asc::util::bit_set::BitSet;
pub use crate::asc::util::block::{Block, BlockRef};
pub use crate::asc::util::byte_list::ByteList;
pub use crate::asc::util::multinames::Multinames;
pub use crate::asc::util::namespaces::Namespaces;
pub use crate::asc::util::number_constant::NumberConstant;
pub use crate::asc::util::number_usage::NumberUsage;

pub const HAS_VALUE_Flag: i32 = 1;
pub const HAS_METHOD_INDEX_Flag: i32 = 2;
pub const HAS_QUALIFIER_Flag: i32 = 4;
pub const IS_PACKAGE_Flag: i32 = 8;
pub const IS_ATTRID_Flag: i32 = 16;
pub const TYPE_ANNOTATION_Flag: i32 = 32;
pub const KIND_Mask: i32 = 0x0000FF00;
pub const KIND_Shift: i32 = 8;
pub const SCOPE_INDEX_Mask: i32 = 0xFFFF0000u32 as i32;
pub const SCOPE_INDEX_Shift: i32 = 16;

/// `Node.useDebugToStrings`.
pub static USE_DEBUG_TO_STRINGS: AtomicBool = AtomicBool::new(false);

pub fn use_debug_to_strings() -> bool {
    USE_DEBUG_TO_STRINGS.load(Ordering::Relaxed)
}

/// `ContextStatics.omitTrace` (Java default false).
pub fn omit_trace() -> bool {
    crate::asc::util::context::OMIT_TRACE.load(Ordering::Relaxed)
}

#[derive(Clone)]
pub enum Value {
    Object(ObjectValue),
    Reference(ReferenceValue),
    KeylessValue(Rc<KeylessValue>),
    KeyValuePair(Rc<KeyValuePair>),
}

/// `MetaDataEvaluator.KeylessValue`.
#[derive(Debug, Clone, Default)]
pub struct KeylessValue {
    pub obj: String,
}

impl KeylessValue {
    pub fn new(v: &str) -> KeylessValue {
        KeylessValue { obj: v.to_string() }
    }
}

/// `MetaDataEvaluator.KeyValuePair`.
#[derive(Debug, Clone, Default)]
pub struct KeyValuePair {
    pub key: String,
    pub obj: String,
}

impl KeyValuePair {
    pub fn new(key: &str, value: &str) -> KeyValuePair {
        KeyValuePair { key: key.to_string(), obj: value.to_string() }
    }
}

impl From<ObjectValue> for Value {
    fn from(o: ObjectValue) -> Value {
        Value::Object(o)
    }
}

impl From<TypeValue> for Value {
    fn from(t: TypeValue) -> Value {
        Value::Object(t.into_object())
    }
}

impl From<ReferenceValue> for Value {
    fn from(r: ReferenceValue) -> Value {
        Value::Reference(r)
    }
}

impl Value {
    /// Java `==`.
    pub fn ptr_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Object(a), Value::Object(b)) => a.ptr_eq(b),
            (Value::Reference(a), Value::Reference(b)) => a.ptr_eq(b),
            (Value::KeylessValue(a), Value::KeylessValue(b)) => Rc::ptr_eq(a, b),
            (Value::KeyValuePair(a), Value::KeyValuePair(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub fn as_keyless_value(&self) -> Option<Rc<KeylessValue>> {
        match self {
            Value::KeylessValue(k) => Some(k.clone()),
            _ => None,
        }
    }

    pub fn as_key_value_pair(&self) -> Option<Rc<KeyValuePair>> {
        match self {
            Value::KeyValuePair(k) => Some(k.clone()),
            _ => None,
        }
    }

    pub fn as_reference_value(&self) -> Option<ReferenceValue> {
        match self {
            Value::Reference(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// `instanceof ObjectValue`.
    pub fn as_object_value(&self) -> Option<ObjectValue> {
        match self {
            Value::Object(o) => Some(o.clone()),
            _ => None,
        }
    }

    /// `instanceof TypeValue`.
    pub fn as_type_value(&self) -> Option<TypeValue> {
        match self {
            Value::Object(o) => o.as_type_value(),
            _ => None,
        }
    }

    pub fn get_value(&self, cx: &Context) -> Option<Value> {
        match self {
            Value::Reference(r) => r.get_value(cx),
            _ => Some(self.clone()),
        }
    }

    pub fn get_type(&self, cx: &Context) -> Option<TypeInfo> {
        match self {
            Value::Object(o) => o.get_type(cx),
            Value::Reference(r) => Some(r.get_type(cx)),
            _ => None,
        }
    }

    pub fn get_type_id(&self) -> i32 {
        match self {
            Value::Object(o) => o.get_type_id(),
            _ => 0,
        }
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Value::Reference(_))
    }

    pub fn boolean_value(&self) -> bool {
        match self {
            Value::Object(o) => o.boolean_value(),
            _ => false,
        }
    }

    pub fn get_printable_name(&self) -> String {
        match self {
            Value::Object(o) => o.get_printable_name(),
            Value::Reference(r) => format!("macromedia.asc.semantics.ReferenceValue@{:x}", r.addr()),
            Value::KeylessValue(k) => format!("macromedia.asc.parser.MetaDataEvaluator$KeylessValue@{:x}", Rc::as_ptr(k) as usize),
            Value::KeyValuePair(k) => format!("macromedia.asc.parser.MetaDataEvaluator$KeyValuePair@{:x}", Rc::as_ptr(k) as usize),
        }
    }

    pub fn has_value(&self) -> bool {
        match self {
            Value::Object(o) => o.has_value(),
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Object(o) => fmt::Display::fmt(o, f),
            Value::Reference(r) => fmt::Display::fmt(r, f),
            _ => Ok(()),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Object(o) => fmt::Debug::fmt(o, f),
            Value::Reference(r) => write!(f, "ReferenceValue({})", r.name()),
            Value::KeylessValue(k) => write!(f, "{:?}", k),
            Value::KeyValuePair(k) => write!(f, "{:?}", k),
        }
    }
}
