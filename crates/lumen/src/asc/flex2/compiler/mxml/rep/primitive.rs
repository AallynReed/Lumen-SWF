//! Port of `flex2.compiler.mxml.rep.Primitive`, the [`ModelKind::Primitive`] of a [`Model`].

use std::cell::RefCell;

use super::deps::Type;
use super::model::{Model, ModelKind, ModelRef, RValue};
use super::mxml_document::MxmlDocumentRef;

#[derive(Default)]
pub struct PrimitiveData {
    value: RefCell<Option<RValue>>,
}

impl PrimitiveData {
    pub fn get_value(&self) -> Option<RValue> {
        self.value.borrow().clone()
    }

    pub fn has_bindings(&self) -> bool {
        matches!(&*self.value.borrow(), Some(RValue::Binding(_)))
    }
}

impl Model {
    /// `new Primitive(document, type, line)` / `new Primitive(document, type, parent, line)`.
    pub fn new_primitive(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, line: i32) -> ModelRef {
        Model::create(document, type_, parent, line, ModelKind::Primitive(PrimitiveData::default()))
    }

    /// `new Primitive(document, type, value, line)`: the value as its `toString()`.
    pub fn new_primitive_value(document: &MxmlDocumentRef, type_: Type, value: &RValue, line: i32) -> ModelRef {
        let m = Model::new_primitive(document, type_, None, line);
        m.set_value(Some(RValue::Str(value.to_java_string())));
        m
    }

    /// `Primitive.getValue()`.
    pub fn get_value(&self) -> Option<RValue> {
        match &self.kind {
            ModelKind::Primitive(p) => p.get_value(),
            _ => None,
        }
    }

    /// `Primitive.setValue(value)`.
    pub fn set_value(&self, value: Option<RValue>) {
        if let ModelKind::Primitive(p) = &self.kind {
            *p.value.borrow_mut() = value;
        }
    }
}
