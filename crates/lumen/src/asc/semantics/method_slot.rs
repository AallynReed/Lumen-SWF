//! Port of `macromedia/asc/semantics/MethodSlot.java`. Its overrides live in `Slot`'s `SlotKind::Method` arms.

use crate::asc::semantics::slot::{DeclStyles, Slot, SlotKind};
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;

pub struct MethodSlotData {
    pub method_name: String,
    pub method_id: i32,
    /// For functions: a `PARAM_Required`, `PARAM_Optional` or `PARAM_Rest` per parameter.
    pub decl_styles: Option<DeclStyles>,
}

/// `new MethodSlot(TypeValue type, int id)`.
pub fn new_method_slot(type_: Option<&TypeValue>, id: i32) -> Slot {
    new_method_slot_info(type_.map(|t| t.get_default_type_info()), id)
}

/// `new MethodSlot(TypeInfo type, int id)`.
pub fn new_method_slot_info(type_: Option<TypeInfo>, id: i32) -> Slot {
    Slot::construct(type_, id, SlotKind::Method(MethodSlotData { method_name: String::new(), method_id: -1, decl_styles: None }))
}
