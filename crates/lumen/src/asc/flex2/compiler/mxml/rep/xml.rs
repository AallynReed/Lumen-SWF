//! Port of `flex2.compiler.mxml.rep.XML`, the [`ModelKind::Xml`] of a [`Model`].

use std::cell::{Cell, RefCell};

use super::deps::Type;
use super::model::{Model, ModelKind, ModelRef};
use super::mxml_document::MxmlDocumentRef;

pub struct XmlData {
    literal_xml: RefCell<Option<String>>,
    is_e4x: bool,
    pub(crate) has_bindings: Cell<bool>,
}

impl XmlData {
    pub fn get_literal_xml(&self) -> Option<String> {
        self.literal_xml.borrow().clone()
    }

    pub fn get_is_e4x(&self) -> bool {
        self.is_e4x
    }
}

impl Model {
    /// `new XML(document, type, parent, isE4X, line)`.
    pub fn new_xml(document: &MxmlDocumentRef, t: Type, parent: Option<ModelRef>, is_e4x: bool, line: i32) -> ModelRef {
        Model::create(document, t, parent, line, ModelKind::Xml(XmlData { literal_xml: RefCell::new(None), is_e4x, has_bindings: Cell::new(false) }))
    }

    /// `XML/XMLList.setHasBindings(b)`.
    pub fn set_has_bindings(&self, has_bindings: bool) {
        match &self.kind {
            ModelKind::Xml(x) => x.has_bindings.set(has_bindings),
            ModelKind::XmlList(x) => x.has_bindings.set(has_bindings),
            _ => {}
        }
    }

    /// `XML/XMLList.getLiteralXML()`.
    pub fn get_literal_xml(&self) -> Option<String> {
        match &self.kind {
            ModelKind::Xml(x) => x.get_literal_xml(),
            ModelKind::XmlList(x) => x.get_literal_xml(),
            _ => None,
        }
    }

    /// `XML/XMLList.setLiteralXML(s)`.
    pub fn set_literal_xml(&self, literal_xml: Option<&str>) {
        match &self.kind {
            ModelKind::Xml(x) => *x.literal_xml.borrow_mut() = literal_xml.map(Into::into),
            ModelKind::XmlList(x) => x.set_literal_xml(literal_xml),
            _ => {}
        }
    }

    /// `XML.getIsE4X()`.
    pub fn get_is_e4x(&self) -> bool {
        matches!(&self.kind, ModelKind::Xml(x) if x.is_e4x)
    }
}
