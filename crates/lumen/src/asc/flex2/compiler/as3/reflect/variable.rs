//! Port of `flex2.compiler.as3.reflect.Variable` (the only `flex2.compiler.abc.Variable`).

use std::ops::Deref;

use super::slot_reflect::SlotReflect;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::slot::Slot;

#[derive(Clone)]
pub struct Variable(SlotReflect);

impl Deref for Variable {
    type Target = SlotReflect;
    fn deref(&self) -> &SlotReflect {
        &self.0
    }
}

impl Variable {
    pub fn new(s: Slot, ns: ObjectValue, name: &str) -> Variable {
        Variable(SlotReflect::new(s, ns, name))
    }

    pub fn get_qname(&self) -> QName {
        QName::new(&self.namespace.name(), &self.name)
    }

    pub fn get_declaring_class_name(&self) -> String {
        let d = self.slot.declared_by().expect("NullPointerException");
        d.builder().expect("NullPointerException").classname().expect("NullPointerException").name().to_string()
    }
}
