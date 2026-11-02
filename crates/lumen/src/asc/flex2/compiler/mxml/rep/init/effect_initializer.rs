//! Port of `flex2.compiler.mxml.rep.init.EffectInitializer`.

use std::rc::Rc;

use super::initializer::{Initializer, InitializerRef};
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps::{Effect, StandardDefs, Type};
use crate::asc::flex2::compiler::mxml::rep::model::RValue;

impl ValueInitializer {
    /// `new EffectInitializer(effect, value, type, line, defs)`.
    pub fn new_effect(effect: Effect, value: RValue, type_: Type, line: i32, defs: Rc<StandardDefs>) -> InitializerRef {
        ValueInitializer::create(value, line, defs, ValueInitializerKind::Effect { effect, type_ })
    }

    /// `EffectInitializer.getEventName()`.
    pub fn get_event_name(&self) -> Option<String> {
        match &self.kind {
            ValueInitializerKind::Effect { effect, .. } => effect.get_event().map(|e| e.to_string()),
            _ => None,
        }
    }
}

/// `((EffectInitializer) init).getEventName()`.
pub fn get_event_name(init: &Initializer) -> String {
    init.as_value().and_then(|v| v.get_event_name()).unwrap_or_else(|| "null".to_string())
}
