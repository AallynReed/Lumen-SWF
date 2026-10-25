//! Port of `macromedia/asc/semantics/UnresolvedNamespace.java`: a public `NamespaceValue` remembering the reference that
//! names it, resolved later by `Context::process_unresolved_namespaces`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::parser::node::NodeRef;
use crate::asc::semantics::namespace_value::NamespaceData;
use crate::asc::semantics::object_value::{ObjectKind, ObjectValue};
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::util::context::{Context, ContextRef, NS_PUBLIC};

pub struct UnresolvedNamespaceData {
    pub node: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub resolved: bool,
    /// A copy of the context current at creation, for reporting errors against the right file.
    pub cx: Option<ContextRef>,
}

/// `new UnresolvedNamespace(cx, node, ref)`.
pub fn new_unresolved_namespace(cx: &Context, node: Option<NodeRef>, ref_: ReferenceValue) -> ObjectValue {
    let data = UnresolvedNamespaceData { node, ref_: Some(ref_), resolved: false, cx: Some(Rc::new(RefCell::new(cx.make_copy_of()))) };
    ObjectValue::from_kind(ObjectKind::Namespace(NamespaceData { ns_kind: NS_PUBLIC, config_ns: false, unresolved: Some(Box::new(data)) }))
}

impl ObjectValue {
    /// The fields of an `UnresolvedNamespace`; panics like a failed cast otherwise.
    pub fn with_unresolved<R>(&self, f: impl FnOnce(&mut UnresolvedNamespaceData) -> R) -> R {
        match &mut self.data_mut().kind {
            ObjectKind::Namespace(NamespaceData { unresolved: Some(u), .. }) => f(u),
            _ => panic!("ClassCastException: not an UnresolvedNamespace"),
        }
    }
}
