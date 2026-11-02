//! Port of `flex2.compiler.mxml.rep.XMLList`, the [`ModelKind::XmlList`] of a [`Model`]; its accessors are shared
//! with `XML` in `xml.rs`.

use std::cell::{Cell, RefCell};

use super::deps::Type;
use super::model::{Model, ModelKind, ModelRef};
use super::mxml_document::MxmlDocumentRef;

#[derive(Default)]
pub struct XmlListData {
    literal_xml: RefCell<Option<String>>,
    pub(crate) has_bindings: Cell<bool>,
}

impl XmlListData {
    pub fn get_literal_xml(&self) -> Option<String> {
        self.literal_xml.borrow().clone()
    }

    pub(crate) fn set_literal_xml(&self, literal_xml: Option<&str>) {
        *self.literal_xml.borrow_mut() = literal_xml.map(Into::into);
    }
}

impl Model {
    /// `new XMLList(document, type, parent, line)`.
    pub fn new_xml_list(document: &MxmlDocumentRef, type_: Type, parent: Option<ModelRef>, line: i32) -> ModelRef {
        Model::create(document, type_, parent, line, ModelKind::XmlList(XmlListData::default()))
    }
}
