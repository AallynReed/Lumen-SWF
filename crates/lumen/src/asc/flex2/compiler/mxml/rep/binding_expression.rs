//! Port of `flex2.compiler.mxml.rep.BindingExpression` (text codegen only): one `{...}` data binding, captured
//! from MXML with its destination. The binding codegen (watchers, `BindableProperty`) consumes these.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::deps::{self, Property, Type};
use super::model::ModelRef;
use super::mxml_document::{MxmlDocument, MxmlDocumentRef};
use crate::asc::flex2::compiler::util::name_formatter;

pub type BindingExpressionRef = Rc<BindingExpression>;

pub struct BindingExpression {
    self_ref: Weak<BindingExpression>,
    source_expression: String,
    destination: RefCell<Option<ModelRef>>,
    destination_property: RefCell<Option<String>>,
    destination_style: RefCell<Option<String>>,
    array_access: Cell<bool>,
    destination_lvalue: RefCell<Option<String>>,
    id: Cell<i32>,
    is_destination_xml_attribute: Cell<bool>,
    is_destination_xml_node: Cell<bool>,
    is_destination_e4x: Cell<bool>,
    is_destination_object_proxy: Cell<bool>,
    is_from_binding_node: Cell<bool>,
    is_two_way_primary: Cell<bool>,
    pub xml_line_number: Cell<i32>,
    mxml_document: RefCell<Weak<MxmlDocument>>,
    two_way_counterpart: RefCell<Option<BindingExpressionRef>>,
    /// `HashMap<Integer, String>` in insertion order; Java's order is applied when it is printed.
    namespaces: RefCell<Option<Vec<(i32, String)>>>,
    source_as_property: RefCell<Option<String>>,
}

impl PartialEq for BindingExpression {
    fn eq(&self, other: &BindingExpression) -> bool {
        self.id.get() == other.id.get()
    }
}

/// Java's iteration order of a `HashMap<Integer, ?>` holding `keys`: by bucket (`key ^ key >>> 16` masked by the
/// table size), then insertion order within a bucket.
pub fn integer_hash_map_order(entries: &[(i32, String)]) -> Vec<(i32, String)> {
    let mut cap = 16usize;
    while entries.len() > cap * 3 / 4 {
        cap *= 2;
    }
    let bucket = |k: i32| -> usize { ((k ^ ((k as u32) >> 16) as i32) as u32 as usize) & (cap - 1) };
    let mut v: Vec<(usize, usize, (i32, String))> = entries.iter().cloned().enumerate().map(|(i, e)| (bucket(e.0), i, e)).collect();
    v.sort_by_key(|(b, i, _)| (*b, *i));
    v.into_iter().map(|(_, _, e)| e).collect()
}

fn put_namespace(list: &mut Vec<(i32, String)>, k: i32, uri: &str) {
    match list.iter_mut().find(|(key, _)| *key == k) {
        Some(e) => e.1 = uri.to_string(),
        None => list.push((k, uri.to_string())),
    }
}

impl BindingExpression {
    /// `new BindingExpression(bindingExpression, xmlLineNumber, mxmlDocument)`: registers itself with the document.
    pub fn new(binding_expression: &str, xml_line_number: i32, mxml_document: &MxmlDocumentRef) -> BindingExpressionRef {
        let be = Rc::new_cyclic(|w| BindingExpression {
            self_ref: w.clone(),
            source_expression: binding_expression.to_string(),
            destination: RefCell::new(None),
            destination_property: RefCell::new(None),
            destination_style: RefCell::new(None),
            array_access: Cell::new(false),
            destination_lvalue: RefCell::new(None),
            id: Cell::new(0),
            is_destination_xml_attribute: Cell::new(false),
            is_destination_xml_node: Cell::new(false),
            is_destination_e4x: Cell::new(false),
            is_destination_object_proxy: Cell::new(false),
            is_from_binding_node: Cell::new(false),
            is_two_way_primary: Cell::new(false),
            xml_line_number: Cell::new(xml_line_number),
            mxml_document: RefCell::new(Weak::new()),
            two_way_counterpart: RefCell::new(None),
            namespaces: RefCell::new(None),
            source_as_property: RefCell::new(None),
        });
        be.set_mxml_document(mxml_document);
        be
    }

    pub fn self_ref(&self) -> BindingExpressionRef {
        self.self_ref.upgrade().expect("binding expression dropped")
    }

    fn doc(&self) -> MxmlDocumentRef {
        self.mxml_document.borrow().upgrade().expect("document dropped")
    }

    /// `compareTo`: by id.
    pub fn compare_to(&self, other: &BindingExpression) -> i32 {
        self.id.get().cmp(&other.id.get()) as i32
    }

    fn ensure_highest_level_model_declared(&self, model: &ModelRef) {
        let doc = self.doc();
        if !(model.is_xml() || model.is_anonymous_object_graph() || Rc::ptr_eq(model, &doc.get_root())) && model.get_id().is_some() {
            doc.ensure_declaration(model);
        }
    }

    pub fn set_mxml_document(&self, mxml_document: &MxmlDocumentRef) {
        *self.mxml_document.borrow_mut() = Rc::downgrade(mxml_document);
        mxml_document.add_binding_expression(&self.self_ref());
    }

    pub fn get_mxml_document(&self) -> MxmlDocumentRef {
        self.doc()
    }

    pub fn is_destination_xml_attribute(&self) -> bool {
        self.is_destination_xml_attribute.get()
    }

    pub fn is_destination_xml_node(&self) -> bool {
        self.is_destination_xml_node.get()
    }

    pub fn is_destination_e4x(&self) -> bool {
        self.is_destination_e4x.get()
    }

    pub fn is_destination_object_proxy(&self) -> bool {
        self.is_destination_object_proxy.get()
    }

    pub fn get_source_as_property(&self) -> Option<String> {
        self.source_as_property.borrow().clone()
    }

    pub fn is_destination_non_public_property(&self) -> bool {
        let destination = self.get_destination();
        let prop = self.get_destination_property();
        if let (Some(destination), Some(prop)) = (destination, prop) {
            if self.destination_style.borrow().is_none() && !self.is_array_access() && !(self.is_destination_xml_attribute() || self.is_destination_xml_node()) {
                if let Some(property) = destination.get_type().get_property(&prop) {
                    return !property.has_public();
                }
            }
        }
        false
    }

    pub fn is_source_public_property(&self) -> bool {
        let se = &self.source_expression;
        let potential_property =
            if se.starts_with('(') && se.ends_with(')') { crate::asc::util::jstr::substring(se, 1, crate::asc::util::jstr::utf16_len(se) - 1) } else { se.clone() };
        let doc = self.doc();
        let skeleton_class = doc.get_skeleton_class();
        let mut result = false;
        if !potential_property.contains(':') {
            let property: Option<Property> = skeleton_class.and_then(|c| c.get_property_ns(deps::PUBLIC_NAMESPACE, &potential_property));
            if let Some(property) = property.filter(|p| p.has_public()) {
                result = true;
                *self.source_as_property.borrow_mut() = Some(potential_property);
                if doc.show_deprecation_warnings() {
                    deps::check_deprecation(&property, &doc.get_source_path(), self.xml_line_number.get());
                }
            }
        }
        result
    }

    /// `generateDestinationStack()`, top of stack last.
    fn generate_destination_stack(&self) -> Vec<ModelRef> {
        let mut stack = Vec::new();
        let mut model = self.get_destination();
        while let Some(m) = model {
            stack.push(m.clone());
            if m.get_id().is_none() || m.get_is_anonymous() {
                model = m.get_parent();
            } else {
                break;
            }
        }
        stack
    }

    pub fn get_destination_path(&self, mut do_xml: bool) -> String {
        let mut buffer = String::new();
        if self.is_destination_e4x() {
            do_xml = true;
        }
        let xml = self.is_destination_xml_attribute() || self.is_destination_xml_node();
        if !(do_xml && xml) {
            buffer.push_str(&self.get_destination_path_root(false));
        }
        let lvalue = self.get_destination_lvalue();
        let prop = self.get_destination_property();
        let style = self.get_destination_style();
        if self.get_destination().is_some() && (prop.is_some() || style.is_some()) && !self.is_array_access() && !(do_xml && xml) {
            buffer.push('.');
        }
        if self.is_array_access() {
            buffer.push('[');
        }
        if (do_xml || !xml) && lvalue.is_some() {
            buffer.push_str(lvalue.as_deref().unwrap());
        } else if let Some(p) = &prop {
            buffer.push_str(p);
        } else if let Some(s) = &style {
            buffer.push_str(s);
        }
        if do_xml && self.is_destination_xml_node() && !self.is_destination_e4x() {
            buffer.push_str(".nodeValue");
        } else if self.is_array_access() {
            buffer.push(']');
        }
        buffer
    }

    pub fn get_destination_path_root(&self, do_repeatable: bool) -> String {
        if self.get_destination().is_none() {
            return String::new();
        }
        let doc = self.doc();
        let root = doc.get_root();
        let mut destination_root = String::new();
        let mut destination_stack = self.generate_destination_stack();
        let top = destination_stack.last().cloned().unwrap();
        self.ensure_highest_level_model_declared(&top);
        let mut write_repeater_indices = do_repeatable;
        while let Some(model) = destination_stack.pop() {
            if Rc::ptr_eq(&model, &root) {
                destination_root.push_str("this");
            } else {
                let parent_index = model.get_parent_index();
                if let (Some(pi), false) = (&parent_index, destination_root.is_empty()) {
                    destination_root.push('[');
                    destination_root.push_str(pi);
                    destination_root.push(']');
                } else if let Some(id) = model.get_id() {
                    if !model.get_is_anonymous() {
                        doc.ensure_declaration(&model);
                    }
                    destination_root.push_str(&id);
                }
            }
            if write_repeater_indices && self.is_repeatable() {
                for i in 0..model.get_repeater_level() {
                    destination_root.push_str(&format!("[instanceIndices[{i}]]"));
                }
                write_repeater_indices = false;
            }
            if let Some(child) = destination_stack.last() {
                if child.get_parent_index().is_none() {
                    destination_root.push('.');
                }
            }
        }
        destination_root
    }

    pub fn get_destination_type_name(&self) -> String {
        let mut type_: Option<Type> = None;
        if let Some(destination) = self.get_destination() {
            if !destination.is_anonymous_object_graph() && !destination.is_xml() {
                if let Some(prop) = self.get_destination_property() {
                    let destination_type = destination.get_type();
                    if destination_type.get_name() != self.doc().get_standard_defs().class_objectproxy {
                        type_ = destination_type.get_property(&prop).and_then(|p| p.get_type());
                    }
                } else if let Some(style) = self.get_destination_style() {
                    type_ = destination.get_type().get_style(&style).and_then(|s| s.get_type());
                } else {
                    type_ = Some(destination.get_type().clone());
                }
            }
        }
        match type_ {
            Some(t) => name_formatter::to_dot(&t.get_name().to_string()),
            None => crate::asc::flex2::compiler::symbol_table::NOTYPE.to_string(),
        }
    }

    pub fn get_id(&self) -> i32 {
        self.id.get()
    }

    pub fn get_repeatable_source_expression(&self) -> String {
        let mut repeatable_source_expression = self.source_expression.clone();
        let destination = self.get_destination().expect("NullPointerException");
        for repeater in destination.get_repeater_parents() {
            let repeater_level = repeater.get_repeater_level();
            let mut buffer = String::new();
            for i in 0..repeater_level {
                buffer.push_str(&format!("[instanceIndices[{i}]]"));
            }
            buffer.push_str(&format!(".mx_internal::getItemAt(repeaterIndices[{repeater_level}])"));
            let rid = repeater.get_id().unwrap_or_else(|| "null".into());
            repeatable_source_expression = java_replace_all(&repeatable_source_expression, &format!("{rid}\\.currentItem"), &format!("{rid}{buffer}"));
            repeatable_source_expression = java_replace_all(&repeatable_source_expression, &format!("{rid}\\.currentIndex"), &format!("repeaterIndices[{repeater_level}]"));
        }
        repeatable_source_expression
    }

    pub fn get_source_expression(&self) -> String {
        self.source_expression.clone()
    }

    pub fn set_destination_property(&self, destination_property: Option<&str>) {
        *self.destination_property.borrow_mut() = destination_property.map(Into::into);
    }

    /// `setDestinationProperty(int)`.
    pub fn set_destination_property_index(&self, destination_property: i32) {
        *self.destination_property.borrow_mut() = Some(destination_property.to_string());
        self.array_access.set(true);
    }

    pub fn set_destination_style(&self, destination_style: Option<&str>) {
        *self.destination_style.borrow_mut() = destination_style.map(Into::into);
    }

    pub fn get_destination_style(&self) -> Option<String> {
        self.destination_style.borrow().clone()
    }

    pub fn is_simple_chain(&self) -> bool {
        !self.is_from_binding_node()
            && !self.is_style()
            && !self.is_destination_object_proxy()
            && self.get_namespace_declarations().is_empty()
            && self.two_way_counterpart.borrow().is_none()
            && !self.get_destination_path(false).contains('[')
    }

    pub fn is_style(&self) -> bool {
        self.destination_style.borrow().is_some()
    }

    pub fn set_id(&self, id: i32) {
        self.id.set(id);
    }

    pub fn set_destination_xml_attribute(&self, b: bool) {
        self.is_destination_xml_attribute.set(b);
    }

    pub fn set_destination_xml_node(&self, b: bool) {
        self.is_destination_xml_node.set(b);
    }

    pub fn set_destination_e4x(&self, b: bool) {
        self.is_destination_e4x.set(b);
    }

    pub fn set_destination_object_proxy(&self, b: bool) {
        self.is_destination_object_proxy.set(b);
    }

    pub fn get_destination_property(&self) -> Option<String> {
        self.destination_property.borrow().clone()
    }

    pub fn is_array_access(&self) -> bool {
        self.array_access.get()
    }

    pub fn get_destination_lvalue(&self) -> Option<String> {
        self.destination_lvalue.borrow().clone()
    }

    pub fn set_destination_lvalue(&self, lvalue: Option<&str>) {
        *self.destination_lvalue.borrow_mut() = lvalue.map(Into::into);
    }

    pub fn get_destination(&self) -> Option<ModelRef> {
        self.destination.borrow().clone()
    }

    pub fn set_destination(&self, destination: &ModelRef) {
        *self.destination.borrow_mut() = Some(destination.clone());
        if self.xml_line_number.get() == 0 {
            self.xml_line_number.set(destination.get_xml_line_number());
        }
    }

    pub fn is_repeatable(&self) -> bool {
        self.get_destination().is_some_and(|d| d.get_repeater_level() > 0)
    }

    pub fn get_repeater_level(&self, var: &str) -> i32 {
        let var = match var.find("[repeaterIndices") {
            Some(_) => &var[..var.find('[').unwrap()],
            None => var,
        };
        let mut repeater_level = -1;
        if let Some(destination) = self.get_destination() {
            let repeaters = destination.get_repeater_parents();
            repeater_level = repeaters.len() as i32 - 1;
            while repeater_level >= 0 {
                if repeaters[repeater_level as usize].get_id().as_deref() == Some(var) {
                    break;
                }
                repeater_level -= 1;
            }
        }
        repeater_level
    }

    pub fn get_repeater_id(&self, level: usize) -> Option<String> {
        self.get_destination().expect("NullPointerException").get_repeater_parents()[level].get_id()
    }

    pub fn get_two_way_counterpart(&self) -> Option<BindingExpressionRef> {
        self.two_way_counterpart.borrow().clone()
    }

    pub fn set_two_way_counterpart(&self, two_way_counterpart: Option<&BindingExpressionRef>) {
        *self.two_way_counterpart.borrow_mut() = two_way_counterpart.cloned();
    }

    pub fn get_xml_line_number(&self) -> i32 {
        self.xml_line_number.get()
    }

    pub fn add_namespace(&self, ns_uri: &str, i: i32) {
        put_namespace(self.namespaces.borrow_mut().get_or_insert_with(Vec::new), i, ns_uri);
    }

    /// `getNamespaces()`: entries in insertion order (see [`integer_hash_map_order`] for Java's).
    pub fn get_namespaces(&self) -> Option<Vec<(i32, String)>> {
        self.namespaces.borrow().clone()
    }

    /// `getAllBindingNamespaceDeclarations(bindingExpressions)`.
    pub fn get_all_binding_namespace_declarations(binding_expressions: &[BindingExpressionRef]) -> String {
        let mut all = Vec::new();
        for be in binding_expressions {
            for (k, v) in be.get_namespaces().unwrap_or_default() {
                put_namespace(&mut all, k, &v);
            }
        }
        Self::get_namespace_declarations_of(Some(&all))
    }

    /// The static `getNamespaceDeclarations(namespaceMap)`.
    pub fn get_namespace_declarations_of(namespace_map: Option<&[(i32, String)]>) -> String {
        match namespace_map {
            Some(m) => {
                let mut b = String::new();
                for (k, uri) in integer_hash_map_order(m) {
                    b.push_str(&format!("var ns{k}:Namespace = new Namespace(\"{uri}\");\n"));
                }
                b
            }
            None => String::new(),
        }
    }

    pub fn get_namespace_declarations(&self) -> String {
        Self::get_namespace_declarations_of(self.namespaces.borrow().as_deref())
    }

    pub fn is_from_binding_node(&self) -> bool {
        self.is_from_binding_node.get()
    }

    pub fn set_from_binding_node(&self, b: bool) {
        self.is_from_binding_node.set(b);
    }

    pub fn is_two_way_primary(&self) -> bool {
        self.is_two_way_primary.get()
    }

    pub fn set_two_way_primary(&self, b: bool) {
        self.is_two_way_primary.set(b);
    }
}

/// `String.replaceAll(regex, replacement)` for the literal-with-`\.` patterns used above.
fn java_replace_all(s: &str, regex: &str, replacement: &str) -> String {
    let re = regex::Regex::new(regex).expect("regex");
    re.replace_all(s, regex::NoExpand(replacement)).into_owned()
}

/// `HashMap.putAll` over namespace maps (`MxmlDocument.getAllBindingNamespaces`).
pub(crate) fn merge_namespaces(into: &mut Vec<(i32, String)>, from: &[(i32, String)]) {
    for (k, v) in from {
        put_namespace(into, *k, v);
    }
}

