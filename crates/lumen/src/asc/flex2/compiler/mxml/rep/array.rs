//! Port of `flex2.compiler.mxml.rep.Array`, the [`ModelKind::Array`] of a [`Model`] (also `Vector`'s base).

use std::cell::RefCell;

use super::deps::Type;
use super::init::initializer::InitializerRef;
use super::init::value_initializer::ValueInitializer;
use super::model::{Model, ModelKind, ModelRef, RValue};
use super::mxml_document::MxmlDocumentRef;
use super::vector::VectorData;
use crate::asc::flex2::compiler::util::name_formatter;

pub struct ArrayData {
    pub(crate) list: RefCell<Vec<InitializerRef>>,
    pub(crate) element_type: Option<Type>,
    /// Set for `Vector`.
    pub vector: Option<VectorData>,
}

impl ArrayData {
    pub fn get_entries(&self) -> Vec<InitializerRef> {
        self.list.borrow().clone()
    }

    pub fn size(&self) -> usize {
        self.list.borrow().len()
    }

    /// `getElementInitializerIterator()`: entries other than `Reparent`s.
    pub fn get_element_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.list.borrow().iter().filter(|i| !i.as_value().and_then(|v| v.get_model()).is_some_and(|m| m.is_reparent())).cloned().collect()
    }

    pub fn get_element_type(&self) -> Option<&Type> {
        self.element_type.as_ref()
    }

    /// `Vector.getElementTypeName()`.
    pub fn get_element_type_name(&self) -> String {
        name_formatter::to_dot(&self.element_type.as_ref().map(|t| t.get_name().to_string()).unwrap_or_default())
    }

    /// `Vector.isFixed()`.
    pub fn is_fixed(&self) -> bool {
        self.vector.as_ref().is_some_and(|v| v.fixed)
    }
}

impl Model {
    /// `new Array(document, line, elementType)` / `new Array(document, parent, line, elementType)`.
    pub fn new_array(document: &MxmlDocumentRef, parent: Option<ModelRef>, line: i32, element_type: Option<Type>) -> ModelRef {
        let array_type = document.get_type_table().array_type();
        Model::new_array_typed(document, array_type, parent, line, element_type, None)
    }

    /// The protected `Array(document, type, parent, line, elementType)`.
    pub(crate) fn new_array_typed(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, line: i32, element_type: Option<Type>, vector: Option<VectorData>) -> ModelRef {
        Model::create(document, type_, parent, line, ModelKind::Array(ArrayData { list: RefCell::new(Vec::new()), element_type, vector }))
    }

    pub fn as_array(&self) -> Option<&ArrayData> {
        match &self.kind {
            ModelKind::Array(a) => Some(a),
            _ => None,
        }
    }

    fn array(&self) -> &ArrayData {
        self.as_array().expect("not an Array")
    }

    /// `addEntry(Model entry)`.
    pub fn add_entry_model(&self, entry: &ModelRef) {
        let a = self.array();
        let index = a.size();
        let initializer = ValueInitializer::new_array_element(self.element_type_or_object(), index, RValue::Model(entry.clone()), entry.get_xml_line_number(), self.get_standard_defs());
        initializer.as_value().unwrap().set_state_specific(entry.is_state_specific());
        a.list.borrow_mut().push(initializer);
    }

    /// `addEntry(Object entry, int line)`.
    pub fn add_entry(&self, entry: RValue, line: i32) {
        let a = self.array();
        let index = a.size();
        let initializer = ValueInitializer::new_array_element(self.element_type_or_object(), index, entry, line, self.get_standard_defs());
        a.list.borrow_mut().push(initializer);
    }

    /// `addEntries(entries, line)`.
    pub fn add_entries(&self, entries: Vec<RValue>, line: i32) {
        for e in entries {
            self.add_entry(e, line);
        }
    }

    pub fn set_entries(&self, entries: Vec<InitializerRef>) {
        *self.array().list.borrow_mut() = entries;
    }

    pub fn get_entries(&self) -> Vec<InitializerRef> {
        self.array().get_entries()
    }

    pub fn size(&self) -> usize {
        self.array().size()
    }

    pub fn get_element_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.array().get_element_initializer_iterator()
    }

    /// The `ArrayElementInitializer`s' lvalue type: Java passes a possibly-null `elementType`, which only `formatExpr`
    /// and the instance-generator checks read; `Object` stands in for null.
    fn element_type_or_object(&self) -> Type {
        match &self.array().element_type {
            Some(t) => t.clone(),
            None => self.get_type().get_type_table().object_type(),
        }
    }
}
