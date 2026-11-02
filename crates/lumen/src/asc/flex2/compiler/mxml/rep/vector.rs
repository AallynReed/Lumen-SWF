//! Port of `flex2.compiler.mxml.rep.Vector`: an [`ArrayData`](super::array::ArrayData) with `vector` set.

use super::deps::Type;
use super::model::{Model, ModelRef};
use super::mxml_document::MxmlDocumentRef;

pub struct VectorData {
    pub fixed: bool,
}

impl Model {
    /// `new Vector(document, line, elementType, fixed)` / `new Vector(document, parent, line, elementType, fixed)`.
    pub fn new_vector(document: &MxmlDocumentRef, parent: Option<ModelRef>, line: i32, element_type: Type, fixed: bool) -> ModelRef {
        let vector_type = document.get_type_table().get_vector_type(&element_type);
        Model::new_array_typed(document, vector_type, parent, line, Some(element_type), Some(VectorData { fixed }))
    }

    /// `Vector.getElementTypeName()`.
    pub fn get_element_type_name(&self) -> String {
        self.as_array().map(|a| a.get_element_type_name()).unwrap_or_default()
    }

    /// `Vector.isFixed()`.
    pub fn is_fixed(&self) -> bool {
        self.as_array().is_some_and(|a| a.is_fixed())
    }
}
