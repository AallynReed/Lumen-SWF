//! Port of `flex2.compiler.as3.genext.GenerativeClassInfo` with its `AccessorInfo`, `VariableInfo` and
//! `GetterSetterInfo`; subclass data (`BindableInfo`) is the `kind`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::compiler::as3::binding::bindable_info::BindableInfo;
use crate::asc::flex2::compiler::as3::binding::class_info::ClassInfoRef;
use crate::asc::flex2::compiler::as3::reflect::node_magic;
use crate::asc::flex2::compiler::mxml::lang::standard_defs;
use crate::asc::flex2::compiler::util::{name_formatter, QName};
use crate::asc::parser::node::NodeRef;
use crate::asc::util::jhash::{string_hash, JHashMap, JHashSet};

pub type GenerativeClassInfoRef = Rc<RefCell<GenerativeClassInfo>>;
pub type AccessorInfoRef = Rc<RefCell<AccessorInfo>>;

pub enum GenerativeKind {
    Bindable(BindableInfo),
}

/// `accessorMap`: a `LinkedHashMap` when a variable is added first, a `HashMap` when a function is.
pub enum AccessorMap {
    Linked(Vec<(QName, AccessorInfoRef)>),
    Hash(JHashMap<QName, AccessorInfoRef>),
}

impl AccessorMap {
    pub fn get(&self, q: &QName) -> Option<&AccessorInfoRef> {
        match self {
            AccessorMap::Linked(m) => m.iter().find(|(k, _)| k == q).map(|(_, v)| v),
            AccessorMap::Hash(m) => m.get(q),
        }
    }

    pub fn contains_key(&self, q: &QName) -> bool {
        self.get(q).is_some()
    }

    pub fn put(&mut self, q: QName, v: AccessorInfoRef) {
        match self {
            AccessorMap::Linked(m) => match m.iter_mut().find(|(k, _)| *k == q) {
                Some(e) => e.1 = v,
                None => m.push((q, v)),
            },
            AccessorMap::Hash(m) => {
                m.insert(q, v);
            }
        }
    }

    pub fn remove(&mut self, q: &QName) {
        match self {
            AccessorMap::Linked(m) => m.retain(|(k, _)| k != q),
            AccessorMap::Hash(m) => {
                m.remove(q);
            }
        }
    }

    pub fn entries(&self) -> Vec<(QName, AccessorInfoRef)> {
        match self {
            AccessorMap::Linked(m) => m.clone(),
            AccessorMap::Hash(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        }
    }

    pub fn values(&self) -> Vec<AccessorInfoRef> {
        self.entries().into_iter().map(|(_, v)| v).collect()
    }

    pub fn len(&self) -> usize {
        match self {
            AccessorMap::Linked(m) => m.len(),
            AccessorMap::Hash(m) => m.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct GenerativeClassInfo {
    class_name: Option<String>,
    accessor_map: Option<AccessorMap>,
    class_info: Option<ClassInfoRef>,
    pub kind: GenerativeKind,
}

impl GenerativeClassInfo {
    pub fn new(kind: GenerativeKind) -> GenerativeClassInfo {
        GenerativeClassInfo { class_name: None, accessor_map: None, class_info: None, kind }
    }

    pub fn bindable(&self) -> &BindableInfo {
        match &self.kind {
            GenerativeKind::Bindable(b) => b,
        }
    }

    pub fn bindable_mut(&mut self) -> &mut BindableInfo {
        match &mut self.kind {
            GenerativeKind::Bindable(b) => b,
        }
    }

    pub fn needs_additional_interfaces(&self) -> bool {
        match &self.kind {
            GenerativeKind::Bindable(b) => b.needs_additional_interfaces(),
        }
    }

    pub fn add_accessor_variable(&mut self, node: &NodeRef, explicit: bool) {
        let map = self.accessor_map.get_or_insert_with(|| AccessorMap::Linked(Vec::new()));
        let q_name = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_variable_name(node));
        let mut variable_info = AccessorInfo::variable(q_name.clone(), node, explicit);
        if has_attribute(node, node_magic::STATIC) {
            variable_info.is_static = true;
        }
        map.put(q_name, Rc::new(RefCell::new(variable_info)));
    }

    pub fn add_accessor_function(&mut self, node: &NodeRef, explicit: bool, is_getter: bool) {
        let map = self.accessor_map.get_or_insert_with(|| AccessorMap::Hash(JHashMap::new()));
        let q_name = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_function_name(node).unwrap_or_default());
        let existing = map.get(&q_name).cloned();
        let info = match existing {
            Some(i) if i.borrow().is_function() => i,
            _ => {
                let mut i = AccessorInfo::getter_setter(q_name.clone(), node, explicit);
                if has_attribute(node, node_magic::STATIC) {
                    i.is_static = true;
                }
                let i = Rc::new(RefCell::new(i));
                map.put(q_name, i.clone());
                i
            }
        };
        if is_getter {
            info.borrow_mut().set_getter_info(node);
        } else {
            info.borrow_mut().set_setter_info(node);
        }
    }

    pub fn remove_accessor(&mut self, q_name: &QName) {
        if let Some(m) = &mut self.accessor_map {
            m.remove(q_name);
        }
    }

    /// `removeOriginalMetaData(definitionNode)`.
    pub fn remove_original_meta_data(&self, definition_node: &NodeRef) {
        match &self.kind {
            GenerativeKind::Bindable(_) => node_magic::remove_meta_data(definition_node, standard_defs::MD_BINDABLE),
        }
    }

    pub fn has_accessor(&self, q_name: &QName) -> bool {
        self.accessor_map.as_ref().is_some_and(|m| m.contains_key(q_name))
    }

    /// `getAccessor(String id)`: the accessor in the public namespace.
    pub fn get_accessor_id(&self, id: &str) -> Option<AccessorInfoRef> {
        self.get_accessor(&QName::new("", id))
    }

    pub fn get_accessor(&self, q_name: &QName) -> Option<AccessorInfoRef> {
        self.accessor_map.as_ref().and_then(|m| m.get(q_name).cloned())
    }

    pub fn get_accessors(&self) -> Option<&AccessorMap> {
        self.accessor_map.as_ref()
    }

    pub fn get_accessors_mut(&mut self) -> Option<&mut AccessorMap> {
        self.accessor_map.as_mut()
    }

    pub fn get_class_info(&self) -> Option<ClassInfoRef> {
        self.class_info.clone()
    }

    pub fn get_class_name(&self) -> Option<String> {
        self.class_name.clone()
    }

    pub fn get_imports(&self) -> JHashSet<String> {
        let mut imports = JHashSet::new();
        if let Some(map) = &self.accessor_map {
            let class_info = self.class_info.as_ref().expect("NullPointerException");
            let mut add_all_imports = false;
            for accessor_info in map.values() {
                let type_name = accessor_info.borrow().get_type_name().expect("NullPointerException");
                let multi_name = class_info.borrow().get_multi_name(&type_name);
                if multi_name.get_namespace().len() == 1 {
                    let ns = &multi_name.get_namespace()[0];
                    let class_name = multi_name.get_local_part().to_string();
                    let less_than_index = class_name.find('<');
                    if let Some(lt) = less_than_index {
                        if let Some(gt) = class_name.rfind('>') {
                            let element_type_name = &class_name[lt + 1..gt];
                            if !standard_defs::is_built_in_type_name(element_type_name) {
                                imports.insert(name_formatter::to_dot(element_type_name));
                            }
                        }
                    } else {
                        let fully_qualified_type_name = name_formatter::to_dot_ns(ns, &class_name);
                        if !standard_defs::is_built_in_type_name(&fully_qualified_type_name) {
                            imports.insert(fully_qualified_type_name);
                        }
                    }
                } else {
                    add_all_imports = true;
                }
            }
            if add_all_imports {
                for import_name in class_info.borrow().get_imports().map(|s| s.to_vec()).unwrap_or_default() {
                    imports.insert(format!("{import_name}.*"));
                }
            }
        }
        imports
    }

    pub fn set_class_info(&mut self, class_info: Option<ClassInfoRef>) {
        self.class_info = class_info;
    }

    pub fn set_class_name(&mut self, class_name: &str) {
        self.class_name = Some(class_name.to_string());
    }
}

fn has_attribute(node: &NodeRef, name: &str) -> bool {
    let attrs = node.borrow().definition().and_then(|d| d.attrs.clone());
    attrs.is_some_and(|a| a.borrow().has_attribute(name))
}

pub enum AccessorKind {
    Variable { position: i32, meta_data: Vec<NodeRef> },
    GetterSetter {
        getter_pos: i32,
        setter_pos: i32,
        getter_meta_data: Option<Vec<NodeRef>>,
        setter_meta_data: Option<Vec<NodeRef>>,
        getter_function_definition: Option<NodeRef>,
        setter_function_definition: Option<NodeRef>,
    },
}

pub struct AccessorInfo {
    q_name: QName,
    backing_prefix: String,
    type_name: Option<String>,
    attributes: Vec<String>,
    is_explicit: bool,
    pub is_static: bool,
    definition_node: NodeRef,
    pub kind: AccessorKind,
}

impl AccessorInfo {
    fn new(q_name: QName, type_name: Option<String>, definition_node: &NodeRef, is_explicit: bool, kind: AccessorKind) -> AccessorInfo {
        let backing_prefix = mangled_prefix(q_name.get_local_part());
        let attributes = node_magic::get_attributes(definition_node);
        AccessorInfo { q_name, backing_prefix, type_name, attributes, is_explicit, is_static: false, definition_node: definition_node.clone(), kind }
    }

    /// `new VariableInfo(qName, node, isExplicit)`.
    pub fn variable(q_name: QName, node: &NodeRef, is_explicit: bool) -> AccessorInfo {
        let position = node.borrow().pos();
        let meta_data = node_magic::get_meta_data(node);
        AccessorInfo::new(q_name, Some(node_magic::get_variable_type_name(node)), node, is_explicit, AccessorKind::Variable { position, meta_data })
    }

    /// `new GetterSetterInfo(qName, node, isExplicit)`.
    pub fn getter_setter(q_name: QName, node: &NodeRef, is_explicit: bool) -> AccessorInfo {
        AccessorInfo::new(
            q_name,
            None,
            node,
            is_explicit,
            AccessorKind::GetterSetter { getter_pos: -1, setter_pos: -1, getter_meta_data: None, setter_meta_data: None, getter_function_definition: None, setter_function_definition: None },
        )
    }

    pub fn get_property_name(&self) -> String {
        self.q_name.get_local_part().to_string()
    }

    pub fn get_user_namespace(&self) -> String {
        self.q_name.get_namespace().to_string()
    }

    pub fn get_backing_prefix(&self) -> String {
        self.backing_prefix.clone()
    }

    pub fn get_backing_property_name(&self) -> String {
        format!("{}{}", self.backing_prefix, self.q_name.get_local_part())
    }

    pub fn get_type_name(&self) -> Option<String> {
        self.type_name.clone()
    }

    pub fn get_attributes(&self) -> &[String] {
        &self.attributes
    }

    pub fn get_attribute_string(&self) -> String {
        let mut result = String::new();
        for a in &self.attributes {
            result.push_str(a);
            result.push(' ');
        }
        crate::asc::flex2::flash::util::string_utils::java_trim(&result)
    }

    pub fn get_is_explicit(&self) -> bool {
        self.is_explicit
    }

    pub fn get_is_static(&self) -> bool {
        self.is_static
    }

    pub fn is_function(&self) -> bool {
        matches!(self.kind, AccessorKind::GetterSetter { .. })
    }

    fn get_qualifier(&self) -> String {
        let ns = self.get_user_namespace();
        if !ns.is_empty() {
            format!("{ns}::")
        } else {
            String::new()
        }
    }

    pub fn get_qualified_property_name(&self) -> String {
        format!("{}{}", self.get_qualifier(), self.get_property_name())
    }

    pub fn get_qualified_backing_property_name(&self) -> String {
        format!("{}{}", self.get_qualifier(), self.get_backing_property_name())
    }

    pub fn get_definition_node(&self) -> NodeRef {
        self.definition_node.clone()
    }

    pub fn set_getter_info(&mut self, node: &NodeRef) {
        debug_assert!(node_magic::function_is_getter(node));
        self.type_name = Some(node_magic::get_function_type_name(node));
        if let AccessorKind::GetterSetter { getter_pos, getter_meta_data, getter_function_definition, .. } = &mut self.kind {
            *getter_pos = node.borrow().pos();
            *getter_meta_data = Some(node_magic::get_meta_data(node));
            *getter_function_definition = Some(node.clone());
        }
    }

    pub fn set_setter_info(&mut self, node: &NodeRef) {
        debug_assert!(node_magic::function_is_setter(node));
        self.type_name = Some(node_magic::get_function_param_type_name(node, 0));
        if let AccessorKind::GetterSetter { setter_pos, setter_meta_data, setter_function_definition, .. } = &mut self.kind {
            *setter_pos = node.borrow().pos();
            *setter_meta_data = Some(node_magic::get_meta_data(node));
            *setter_function_definition = Some(node.clone());
        }
    }

    pub fn get_getter_position(&self) -> i32 {
        match &self.kind {
            AccessorKind::GetterSetter { getter_pos, .. } => *getter_pos,
            _ => -1,
        }
    }

    pub fn get_setter_position(&self) -> i32 {
        match &self.kind {
            AccessorKind::GetterSetter { setter_pos, .. } => *setter_pos,
            _ => -1,
        }
    }
}

fn mangled_prefix(s: &str) -> String {
    format!("_{}", string_hash(s).wrapping_abs())
}

/// `mangledName(s)`.
pub fn mangled_name(s: &str) -> String {
    format!("{}{s}", mangled_prefix(s))
}
