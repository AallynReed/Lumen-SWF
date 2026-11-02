//! Port of `flex2.compiler.as3.reflect.Method` (the only `flex2.compiler.abc.Method`).

use std::ops::Deref;

use super::slot_reflect::SlotReflect;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::slot::Slot;

#[derive(Clone)]
pub struct Method(SlotReflect);

impl Deref for Method {
    type Target = SlotReflect;
    fn deref(&self) -> &SlotReflect {
        &self.0
    }
}

impl Method {
    pub fn new(s: Slot, ns: ObjectValue, name: &str) -> Method {
        Method(SlotReflect::new(s, ns, name))
    }

    pub fn get_qname(&self) -> QName {
        QName::new(&self.namespace.name(), &self.name)
    }

    pub fn get_return_type_name(&self) -> Option<String> {
        self.get_type_name()
    }

    pub fn get_return_element_type_name(&self) -> Option<String> {
        self.get_element_type_name()
    }

    pub fn get_parameter_type_names(&self) -> Option<Vec<Option<String>>> {
        let types = self.slot.get_types()?;
        let types = types.borrow();
        Some(types.iter().map(|t| SlotReflect::type_name_of(Some(t))).collect())
    }

    pub fn get_parameter_element_type_names(&self) -> Option<Vec<Option<String>>> {
        let types = self.slot.get_types()?;
        let types = types.borrow();
        Some(types.iter().map(|t| SlotReflect::element_type_name_of(Some(t))).collect())
    }
}
