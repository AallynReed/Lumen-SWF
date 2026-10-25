//! Port of `macromedia/asc/semantics/NamespaceValue.java`: an `ObjectValue` of kind `Namespace`. Its overrides
//! (`isInternal`, `isProtected`, `isPrivate`, `isConfigNS`, `getNamespaceKind`) are in `object_value.rs`.

use crate::asc::semantics::object_value::{ObjectKind, ObjectValue};
use crate::asc::semantics::unresolved_namespace::UnresolvedNamespaceData;
use crate::asc::util::context::NS_PUBLIC;

pub struct NamespaceData {
    pub ns_kind: u8,
    pub config_ns: bool,
    /// Set for an `UnresolvedNamespace`.
    pub unresolved: Option<Box<UnresolvedNamespaceData>>,
}

/// `new NamespaceValue()`.
pub fn new_public_namespace_value() -> ObjectValue {
    new_namespace_value(NS_PUBLIC)
}

/// `new NamespaceValue(ns_kind)`.
pub fn new_namespace_value(ns_kind: u8) -> ObjectValue {
    ObjectValue::from_kind(ObjectKind::Namespace(NamespaceData { ns_kind, config_ns: false, unresolved: None }))
}

impl ObjectValue {
    /// The `ns_kind` field of a `NamespaceValue`.
    pub fn set_ns_kind(&self, ns_kind: u8) {
        match &mut self.data_mut().kind {
            ObjectKind::Namespace(n) => n.ns_kind = ns_kind,
            _ => panic!("ClassCastException: not a NamespaceValue"),
        }
    }

    /// The `config_ns` field of a `NamespaceValue`.
    pub fn set_config_ns(&self, config_ns: bool) {
        match &mut self.data_mut().kind {
            ObjectKind::Namespace(n) => n.config_ns = config_ns,
            _ => panic!("ClassCastException: not a NamespaceValue"),
        }
    }
}
