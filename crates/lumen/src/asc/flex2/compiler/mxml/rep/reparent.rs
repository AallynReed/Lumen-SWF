//! Port of `flex2.compiler.mxml.rep.Reparent`.

use super::deps::Type;
use super::model::{Model, ModelKind, ModelRef};
use super::mxml_document::MxmlDocumentRef;

impl Model {
    /// `new Reparent(document, type, line)` / `new Reparent(document, type, parent, line)`.
    pub fn new_reparent(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, line: i32) -> ModelRef {
        Model::create(document, type_, parent, line, ModelKind::Reparent)
    }

    /// `new Reparent(document, type, parent, target, line)`.
    pub fn new_reparent_target(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, target: &str, line: i32) -> ModelRef {
        let m = Model::new_reparent(document, type_, parent, line);
        m.set_id(Some(target), true);
        m.set_state_specific(true);
        m
    }

    /// `Reparent.getTarget()`.
    pub fn get_target(&self) -> Option<String> {
        self.get_id()
    }

    /// `Reparent.setTarget(value)`.
    pub fn set_target(&self, value: &str) {
        self.set_id(Some(value), true);
    }
}
