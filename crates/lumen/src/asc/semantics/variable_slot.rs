//! Port of `macromedia/asc/semantics/VariableSlot.java`. Its overrides live in `Slot`'s `SlotKind::Variable` arms.

use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::slot::{Slot, SlotKind};
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;

pub struct VariableSlotData {
    pub var_index: i32,
    pub typeref: Option<ReferenceValue>,
    /// `AUX_MethodName`.
    pub method_name: Option<String>,
}

/// `new VariableSlot(TypeValue type, int id, int var_index)`.
pub fn new_variable_slot(type_: Option<&TypeValue>, id: i32, var_index: i32) -> Slot {
    new_variable_slot_info(type_.map(|t| t.get_default_type_info()), id, var_index)
}

/// `new VariableSlot(TypeInfo type, int id, int var_index)`.
pub fn new_variable_slot_info(type_: Option<TypeInfo>, id: i32, var_index: i32) -> Slot {
    Slot::construct(type_, id, SlotKind::Variable(VariableSlotData { var_index, typeref: None, method_name: None }))
}
