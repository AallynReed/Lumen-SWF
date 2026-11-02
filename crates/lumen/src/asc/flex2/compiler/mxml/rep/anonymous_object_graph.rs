//! Port of `flex2.compiler.mxml.rep.AnonymousObjectGraph`'s constructor (its members are [`Model`]'s).

use super::deps::Type;
use super::model::{Model, ModelKind, ModelRef};
use super::mxml_document::MxmlDocumentRef;

impl Model {
    /// `new AnonymousObjectGraph(document, objectType, line)`.
    pub fn new_anonymous_object_graph(document: &MxmlDocumentRef, object_type: Type, line: i32) -> ModelRef {
        Model::create(document, object_type, None, line, ModelKind::AnonymousObjectGraph)
    }
}
