//! Port of `flex2.compiler.as3.reflect.SlotReflect`, the base of `Variable` and `Method`.

use super::meta_data::MetaData;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::slot::Slot;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::util::context::NS_PUBLIC;

#[derive(Clone)]
pub struct SlotReflect {
    pub(super) slot: Slot,
    pub(super) namespace: ObjectValue,
    pub(super) name: String,
}

impl SlotReflect {
    pub fn new(s: Slot, ns: ObjectValue, name: &str) -> SlotReflect {
        SlotReflect { slot: s, namespace: ns, name: name.to_string() }
    }

    pub(super) fn type_name_of(ti: Option<&TypeInfo>) -> Option<String> {
        ti.map(|ti| ti.type_value().name().to_string())
    }

    pub fn get_meta_data(&self, id: &str) -> Option<Vec<MetaData>> {
        let list = self.slot.get_metadata();
        let mut result: Option<Vec<MetaData>> = None;
        if let Some(list) = list {
            for m in &list {
                if m.id().as_deref() == Some(id) {
                    result.get_or_insert_with(Vec::new).push(MetaData::from_semantics(m));
                }
            }
        }
        result
    }

    pub(super) fn element_type_name_of(ti: Option<&TypeInfo>) -> Option<String> {
        let ti = ti?;
        ti.type_value().indexed_type().map(|t| t.name().to_string())
    }

    pub fn get_type_name(&self) -> Option<String> {
        Self::type_name_of(self.slot.get_type().as_ref())
    }

    pub fn get_element_type_name(&self) -> Option<String> {
        Self::element_type_name_of(self.slot.get_type().as_ref())
    }

    pub fn is_const(&self) -> bool {
        self.slot.is_const()
    }

    pub fn is_static(&self) -> bool {
        self.slot.declared_by().is_some_and(|d| d.is_type_value())
    }

    pub fn is_public(&self) -> bool {
        self.namespace.get_namespace_kind() == NS_PUBLIC
    }

    pub fn get_slot(&self) -> &Slot {
        &self.slot
    }
}
