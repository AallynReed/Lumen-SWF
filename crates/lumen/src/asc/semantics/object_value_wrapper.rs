//! Port of `macromedia/asc/semantics/ObjectValueWrapper.java`: an `ObjectValue` of kind `Wrapper` that forwards most
//! lookups to the wrapped object (the forwarding is in `object_value.rs`).

use crate::asc::semantics::object_value::{ObjectKind, ObjectValue};

/// `new ObjectValueWrapper(wrapped)`.
pub fn new_object_value_wrapper(wrapped: ObjectValue) -> ObjectValue {
    ObjectValue::from_kind(ObjectKind::Wrapper(wrapped))
}
