//! Ports of `flex2.compiler.mxml.reflect`. The helper classes of `TypeTable` are the structs; the interface files
//! re-export them.

pub mod assignable;
pub mod deprecated;
pub mod dynamic_property;
pub mod effect;
pub mod element_type_not_found;
pub mod event;
pub mod inspectable;
pub mod property;
pub mod style;
pub mod stateful;
#[path = "type.rs"]
pub mod r#type;
pub mod type_table;

pub use assignable::Assignable;
pub use type_table::{
    Deprecated, DynamicProperty, DynamicPropertyRef, Effect, EffectRef, Event, EventRef, Inspectable, Property, PropertyRef, Stateful, Style, StyleRef, Type, TypeRef, TypeTable,
    TypeTableRef,
};

#[cfg(test)]
mod tests;
