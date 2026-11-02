//! Port of `flex2.compiler.mxml.rep.DesignLayer`'s constructors.

use super::deps::Type;
use super::model::{Model, ModelKind, ModelRef};
use super::mxml_document::MxmlDocumentRef;

impl Model {
    /// `new DesignLayer(document, type, line)` / `new DesignLayer(document, type, parent, line)`.
    pub fn new_design_layer(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, line: i32) -> ModelRef {
        Model::create(document, type_, parent, line, ModelKind::DesignLayer)
    }
}
