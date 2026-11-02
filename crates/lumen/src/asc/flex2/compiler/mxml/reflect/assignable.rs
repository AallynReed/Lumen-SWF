//! Port of `flex2.compiler.mxml.reflect.Assignable`: the four implementations as one enum, so `instanceof` checks
//! become matches.

use super::type_table::{DynamicPropertyRef, EffectRef, PropertyRef, Stateful, StyleRef, TypeRef};

#[derive(Clone)]
pub enum Assignable {
    Property(PropertyRef),
    Style(StyleRef),
    Effect(EffectRef),
    DynamicProperty(DynamicPropertyRef),
}

impl Assignable {
    pub fn get_name(&self) -> Option<String> {
        match self {
            Assignable::Property(p) => p.get_name().map(str::to_string),
            Assignable::Style(s) => Some(s.get_name().to_string()),
            Assignable::Effect(e) => Some(e.get_name().to_string()),
            Assignable::DynamicProperty(d) => Some(d.get_name().to_string()),
        }
    }

    pub fn get_type(&self) -> Option<TypeRef> {
        match self {
            Assignable::Property(p) => p.get_type(),
            Assignable::Style(s) => s.get_type(),
            Assignable::Effect(e) => e.get_type(),
            Assignable::DynamicProperty(d) => d.get_type(),
        }
    }

    pub fn get_lvalue_type(&self) -> Option<TypeRef> {
        match self {
            Assignable::Property(p) => p.get_lvalue_type(),
            Assignable::Style(s) => s.get_lvalue_type(),
            Assignable::Effect(e) => e.get_lvalue_type(),
            Assignable::DynamicProperty(d) => d.get_lvalue_type(),
        }
    }

    pub fn get_element_type(&self) -> Option<TypeRef> {
        match self {
            Assignable::Property(p) => p.get_element_type(),
            Assignable::Style(s) => s.get_element_type(),
            Assignable::Effect(e) => e.get_element_type(),
            Assignable::DynamicProperty(d) => d.get_element_type(),
        }
    }

    fn stateful(&self) -> &dyn Stateful {
        match self {
            Assignable::Property(p) => &**p,
            Assignable::Style(s) => &**s,
            Assignable::Effect(e) => &**e,
            Assignable::DynamicProperty(d) => &**d,
        }
    }

    /// Identity, as Java's `==`.
    pub fn ptr_eq(&self, other: &Assignable) -> bool {
        match (self, other) {
            (Assignable::Property(a), Assignable::Property(b)) => std::rc::Rc::ptr_eq(a, b),
            (Assignable::Style(a), Assignable::Style(b)) => std::rc::Rc::ptr_eq(a, b),
            (Assignable::Effect(a), Assignable::Effect(b)) => std::rc::Rc::ptr_eq(a, b),
            (Assignable::DynamicProperty(a), Assignable::DynamicProperty(b)) => std::rc::Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Stateful for Assignable {
    fn set_state_name(&self, name: Option<&str>) {
        self.stateful().set_state_name(name)
    }
    fn get_state_name(&self) -> Option<String> {
        self.stateful().get_state_name()
    }
}
