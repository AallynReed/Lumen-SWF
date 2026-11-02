//! Port of `flex2.compiler.mxml.lang.DeclarationHandler`: `resolve` is `invoke`'s lookup (with its `setStateName`
//! side effects), and the trait dispatches its result to the abstract callbacks.

use super::standard_defs::PROP_UICOMPONENT_STATES;
use crate::asc::flex2::compiler::mxml::reflect::{EffectRef, EventRef, PropertyRef, Stateful, StyleRef, TypeRef};

pub enum Declaration {
    Event(EventRef),
    Property(PropertyRef),
    States(PropertyRef),
    Effect(EffectRef),
    Style(StyleRef),
    DynamicProperty(String, Option<String>),
    Unknown(String, String),
}

/// The body of `invoke(type, namespace, localPart, state)` up to the callback.
pub fn resolve(t: &TypeRef, namespace: &str, local_part: &str, state: Option<&str>) -> Declaration {
    if let Some(event) = t.get_event(local_part) {
        event.set_state_name(state);
        return Declaration::Event(event);
    }
    if let Some(property) = t.get_property(local_part) {
        property.set_state_name(state);
        return if local_part == PROP_UICOMPONENT_STATES { Declaration::States(property) } else { Declaration::Property(property) };
    }
    if let Some(effect) = t.get_effect(local_part) {
        effect.set_state_name(state);
        return Declaration::Effect(effect);
    }
    if let Some(style) = t.get_style(local_part) {
        style.set_state_name(state);
        return Declaration::Style(style);
    }
    if t.has_dynamic() {
        return Declaration::DynamicProperty(local_part.to_string(), state.map(str::to_string));
    }
    Declaration::Unknown(namespace.to_string(), local_part.to_string())
}

pub trait DeclarationHandler {
    fn event(&mut self, event: &EventRef);
    fn property(&mut self, property: &PropertyRef);
    fn states(&mut self, property: &PropertyRef);
    fn effect(&mut self, effect: &EffectRef);
    fn style(&mut self, style: &StyleRef);
    fn dynamic_property(&mut self, name: &str, state: Option<&str>);
    fn unknown(&mut self, namespace: &str, local_part: &str);

    fn invoke_state(&mut self, t: &TypeRef, namespace: &str, local_part: &str, state: Option<&str>) {
        self.dispatch(resolve(t, namespace, local_part, state));
    }

    fn invoke(&mut self, t: &TypeRef, namespace: &str, local_part: &str) {
        self.invoke_state(t, namespace, local_part, None);
    }

    fn dispatch(&mut self, d: Declaration) {
        match d {
            Declaration::Event(e) => self.event(&e),
            Declaration::Property(p) => self.property(&p),
            Declaration::States(p) => self.states(&p),
            Declaration::Effect(e) => self.effect(&e),
            Declaration::Style(s) => self.style(&s),
            Declaration::DynamicProperty(n, s) => self.dynamic_property(&n, s.as_deref()),
            Declaration::Unknown(ns, l) => self.unknown(&ns, &l),
        }
    }
}
