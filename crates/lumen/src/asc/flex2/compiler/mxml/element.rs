//! Port of `flex2.compiler.mxml.Element`: a tag's namespace, name, attributes (a `LinkedQNameMap`), prefix mappings and
//! children, with the `Token` fields it extends. `Node` (in `dom::node`) derefs to it.

use super::attribute::{AttrValue, Attribute};
use super::dom::node::NodeRef;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::util::jhash::JHashMap;

#[derive(Clone, Default)]
pub struct Element {
    pub kind: i32,
    pub begin_line: i32,
    pub begin_column: i32,
    pub end_line: i32,
    pub end_column: i32,
    pub image: String,
    uri: String,
    local_part: String,
    attributes: Option<Vec<(QName, Attribute)>>,
    prefix_mappings: Option<JHashMap<String, Option<String>>>,
    pub(crate) children: Option<Vec<NodeRef>>,
}

impl Element {
    pub fn new(uri: &str, local_part: &str, size: usize) -> Element {
        Element { uri: uri.to_string(), local_part: local_part.to_string(), attributes: if size > 0 { Some(Vec::with_capacity(size)) } else { None }, ..Default::default() }
    }

    pub fn add_prefix_mapping(&mut self, uri: &str, prefix: Option<&str>) {
        self.prefix_mappings.get_or_insert_with(|| JHashMap::with_capacity(8)).insert(uri.to_string(), prefix.map(str::to_string));
    }

    pub fn add_attribute(&mut self, uri: &str, local_part: &str, value: AttrValue, line: i32) {
        let attrs = self.attributes.get_or_insert_with(Vec::new);
        let attr = Attribute::new(uri, local_part, value, line);
        match attrs.iter_mut().find(|(q, _)| q.equals(uri, local_part)) {
            Some(slot) => slot.1 = attr,
            None => attrs.push((QName::new(uri, local_part), attr)),
        }
    }

    pub fn get_attribute(&self, local_name: &str) -> Option<&Attribute> {
        self.get_attribute_uri("", local_name)
    }

    pub fn get_attribute_uri(&self, uri: &str, local_name: &str) -> Option<&Attribute> {
        self.attributes.as_ref()?.iter().find(|(q, _)| q.equals(uri, local_name)).map(|(_, a)| a)
    }

    pub fn get_attribute_value(&self, local_name: &str) -> Option<&AttrValue> {
        self.get_attribute_value_uri("", local_name)
    }

    pub fn get_attribute_value_uri(&self, uri: &str, local_name: &str) -> Option<&AttrValue> {
        self.get_attribute_uri(uri, local_name).map(|a| a.get_value())
    }

    pub fn get_attribute_value_qname(&self, qname: &QName) -> Option<&AttrValue> {
        self.get_attribute_value_uri(qname.get_namespace(), qname.get_local_part())
    }

    /// `(String) getAttributeValue(localName)`.
    pub fn get_attribute_str(&self, local_name: &str) -> Option<&str> {
        self.get_attribute_value(local_name).and_then(|v| v.as_str())
    }

    pub fn get_line_number(&self, local_name: &str) -> i32 {
        self.get_line_number_uri("", local_name)
    }

    pub fn get_line_number_uri(&self, uri: &str, local_name: &str) -> i32 {
        match &self.attributes {
            None => self.begin_line,
            Some(_) => self.get_attribute_uri(uri, local_name).map(|a| a.get_line()).unwrap_or(self.begin_line),
        }
    }

    pub fn get_line_number_qname(&self, qname: &QName) -> i32 {
        self.get_line_number_uri(qname.get_namespace(), qname.get_local_part())
    }

    /// `getAttributeNames()`, in insertion order.
    pub fn get_attribute_names(&self) -> Vec<QName> {
        self.attributes.as_ref().map(|a| a.iter().map(|(q, _)| q.clone()).collect()).unwrap_or_default()
    }

    pub fn get_attribute_count(&self) -> usize {
        self.attributes.as_ref().map_or(0, |a| a.len())
    }

    pub fn get_namespace(&self) -> &str {
        &self.uri
    }

    pub fn get_local_part(&self) -> &str {
        &self.local_part
    }

    pub fn get_prefix(&self) -> Option<String> {
        self.get_prefix_uri(&self.uri)
    }

    pub fn get_prefix_uri(&self, uri: &str) -> Option<String> {
        self.prefix_mappings.as_ref()?.get(uri).cloned().flatten()
    }

    /// The prefix mappings in `HashMap` order, for `toStartElement`.
    pub fn get_prefix_mappings(&self) -> Vec<(String, Option<String>)> {
        self.prefix_mappings.as_ref().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default()
    }

    pub fn remove_attribute(&mut self, qname: &QName) {
        if let Some(a) = &mut self.attributes {
            a.retain(|(q, _)| q != qname);
        }
    }

    /// `copy(element)`: the attributes and children are shared in Java; here they are copied.
    pub fn copy(&self, element: &mut Element) {
        element.uri = self.uri.clone();
        element.local_part = self.local_part.clone();
        element.attributes = self.attributes.clone();
        element.children = self.children.clone();
        element.prefix_mappings = self.prefix_mappings.clone();
    }

    pub fn replace_node(&mut self, index: usize, children: Vec<NodeRef>) {
        let c = self.children.get_or_insert_with(Vec::new);
        c.splice(index..index + 1, children);
    }

    pub fn to_start_element(&self, w: &mut String) {
        w.push('<');
        if let Some(p) = self.get_prefix_uri(&self.uri).filter(|p| !p.is_empty()) {
            w.push_str(&p);
            w.push(':');
        }
        w.push_str(&self.local_part);
        for q in self.get_attribute_names() {
            w.push(' ');
            if let Some(p) = self.get_prefix_uri(q.get_namespace()).filter(|p| !p.is_empty()) {
                w.push_str(&p);
                w.push(':');
            }
            w.push_str(q.get_local_part());
            match self.get_attribute_value_qname(&q) {
                Some(AttrValue::AtResource(r)) => {
                    w.push_str("={");
                    w.push_str(&r.get_value_expression());
                    w.push('}');
                }
                v => {
                    w.push_str("=\"");
                    w.push_str(&v.map(|v| v.to_java_string()).unwrap_or_else(|| "null".to_string()));
                    w.push('"');
                }
            }
        }
        for (ns, _) in self.get_prefix_mappings() {
            if let Some(px) = self.get_prefix_uri(&ns) {
                w.push_str(" xmlns");
                if !px.is_empty() {
                    w.push(':');
                    w.push_str(&px);
                }
                w.push_str("=\"");
                w.push_str(&ns);
                w.push('"');
            }
        }
        w.push('>');
    }

    pub fn to_end_element(&self, w: &mut String) {
        w.push_str("</");
        if let Some(p) = self.get_prefix_uri(&self.uri).filter(|p| !p.is_empty()) {
            w.push_str(&p);
            w.push(':');
        }
        w.push_str(&self.local_part);
        w.push('>');
    }
}
