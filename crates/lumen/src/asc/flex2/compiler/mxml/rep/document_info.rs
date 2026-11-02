//! Port of `flex2.compiler.mxml.rep.DocumentInfo`: what the InterfaceCompiler learns about a document (names,
//! imports, interfaces, states, scripts, metadata), shared with `MxmlDocument`. Interior mutability, `&self` methods.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};

use super::deps::{self, DocumentNode, StandardDefs};
use super::script::Script;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};
use crate::asc::util::jhash::JHashMap;
use crate::asc::util::jstr;

pub const OUTER_DOCUMENT_PROP: &str = "outerDocument";

pub type DocumentInfoRef = Rc<DocumentInfo>;

/// `DocumentInfo.VarDecl`.
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub name: String,
    pub class_name: String,
    pub line: i32,
}

/// `DocumentInfo.NameInfo`: ordered by name only.
#[derive(Clone, Debug)]
pub struct NameInfo {
    name: String,
    line: i32,
}

impl NameInfo {
    pub fn new(name: &str, line: i32) -> NameInfo {
        NameInfo { name: name.to_string(), line }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_line(&self) -> i32 {
        self.line
    }

    pub fn value(&self) -> Value {
        Value::Object(Rc::new(self.clone()))
    }
}

static NAME_INFO_METHODS: &[Method] = &[Method::new("getName", &[], JType::String), Method::new("getLine", &[], JType::Int)];

impl JavaObject for NameInfo {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.DocumentInfo$NameInfo"
    }

    fn methods(&self) -> &[Method] {
        NAME_INFO_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getName" => Value::str(&self.name),
            "getLine" => Value::Integer(self.line),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        self.name.clone()
    }
}

/// A `TreeSet<NameInfo>`: one entry per name (the first added keeps its line), in `String.compareTo` order.
#[derive(Clone, Debug, Default)]
pub struct NameInfoSet(BTreeMap<jstr_key::Key, NameInfo>);

mod jstr_key {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Key(pub String);
    impl PartialOrd for Key {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Key {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            crate::asc::util::jstr::compare(&self.0, &o.0)
        }
    }
}

impl NameInfoSet {
    pub fn add(&mut self, info: NameInfo) {
        self.0.entry(jstr_key::Key(info.name.clone())).or_insert(info);
    }

    pub fn remove(&mut self, name: &str) {
        self.0.remove(&jstr_key::Key(name.to_string()));
    }

    pub fn iter(&self) -> impl Iterator<Item = &NameInfo> {
        self.0.values()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub struct DocumentInfo {
    root_node: RefCell<Option<DocumentNode>>,
    path: String,
    class_name: RefCell<Option<String>>,
    package_name: RefCell<Option<String>>,
    qname: RefCell<Option<QName>>,
    qualified_super_class_name: RefCell<Option<String>>,
    interface_names: RefCell<NameInfoSet>,
    import_names: RefCell<NameInfoSet>,
    split_import_names: RefCell<BTreeMap<jstr_key::Key, Vec<String>>>,
    state_names: RefCell<IndexSet<String>>,
    state_groups: RefCell<Option<JHashMap<String, Vec<String>>>>,
    var_decls: RefCell<IndexMap<String, VarDecl>>,
    scripts: RefCell<Vec<Script>>,
    metadata: RefCell<Vec<Script>>,
    language_namespace: RefCell<Option<String>>,
    version: Cell<i32>,
    standard_defs: Rc<StandardDefs>,
}

impl DocumentInfo {
    pub fn new(path: &str, standard_defs: Rc<StandardDefs>) -> DocumentInfoRef {
        Rc::new(DocumentInfo {
            root_node: RefCell::new(None),
            path: path.to_string(),
            class_name: RefCell::new(None),
            package_name: RefCell::new(None),
            qname: RefCell::new(None),
            qualified_super_class_name: RefCell::new(None),
            interface_names: RefCell::new(NameInfoSet::default()),
            import_names: RefCell::new(NameInfoSet::default()),
            split_import_names: RefCell::new(BTreeMap::new()),
            state_names: RefCell::new(IndexSet::new()),
            state_groups: RefCell::new(None),
            var_decls: RefCell::new(IndexMap::new()),
            scripts: RefCell::new(Vec::new()),
            metadata: RefCell::new(Vec::new()),
            language_namespace: RefCell::new(Some(String::new())),
            version: Cell::new(0),
            standard_defs,
        })
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn set_root_node(&self, root_node: DocumentNode, line: i32) {
        let outer = deps::document_node_outer_document_class_name(&root_node);
        let version = deps::document_node_version(&root_node);
        let language_namespace = deps::document_node_language_namespace(&root_node);
        *self.root_node.borrow_mut() = Some(root_node);
        if let Some(outer) = outer {
            self.add_var_decl(OUTER_DOCUMENT_PROP, &outer, line);
        }
        self.version.set(version);
        let missing = language_namespace.is_none();
        *self.language_namespace.borrow_mut() = language_namespace;
        if version < 0 || missing {
            toolkit::log(CompilerMessage::error("Unable to resolve MXML language version. Please specify the language namespace on the root document tag."), None, -1, -1, None);
        }
    }

    pub fn get_standard_defs(&self) -> Rc<StandardDefs> {
        self.standard_defs.clone()
    }

    pub fn get_root_node(&self) -> DocumentNode {
        self.root_node.borrow().clone().expect("rootNode")
    }

    pub fn set_class_name(&self, class_name: &str) {
        *self.class_name.borrow_mut() = Some(class_name.to_string());
    }

    pub fn get_class_name(&self) -> String {
        self.class_name.borrow().clone().expect("className")
    }

    pub fn set_package_name(&self, package_name: &str) {
        *self.package_name.borrow_mut() = Some(package_name.to_string());
    }

    pub fn get_package_name(&self) -> String {
        self.package_name.borrow().clone().expect("packageName")
    }

    pub fn get_qname(&self) -> QName {
        if self.qname.borrow().is_none() {
            *self.qname.borrow_mut() = Some(QName::new(&self.package_name.borrow().clone().unwrap_or_default(), &self.class_name.borrow().clone().unwrap_or_default()));
        }
        self.qname.borrow().clone().unwrap()
    }

    pub fn set_qualified_super_class_name(&self, qualified_super_class_name: &str, line: i32) {
        *self.qualified_super_class_name.borrow_mut() = Some(qualified_super_class_name.to_string());
        self.add_import_name(qualified_super_class_name, line);
    }

    pub fn get_qualified_super_class_name(&self) -> String {
        self.qualified_super_class_name.borrow().clone().expect("qualifiedSuperClassName")
    }

    pub fn add_interface_name(&self, interface_name: &str, line: i32) {
        self.interface_names.borrow_mut().add(NameInfo::new(interface_name, line));
        self.add_import_name(interface_name, line);
    }

    pub fn get_interface_names(&self) -> NameInfoSet {
        self.interface_names.borrow().clone()
    }

    pub fn add_import_name(&self, import_name: &str, line: i32) {
        let vec_long = format!("{}.<", deps::CLASS_VECTOR);
        let vec_short = format!("{}.<", deps::CLASS_VECTOR_SHORTNAME);
        if import_name.starts_with(&vec_long) {
            let n = jstr::utf16_len(import_name);
            self.add_import_name(&jstr::substring(import_name, jstr::utf16_len(deps::CLASS_VECTOR) + 2, n - 1), line);
        } else if import_name.starts_with(&vec_short) {
            let n = jstr::utf16_len(import_name);
            self.add_import_name(&jstr::substring(import_name, jstr::utf16_len(deps::CLASS_VECTOR_SHORTNAME) + 2, n - 1), line);
        } else if import_name != "*" && !deps::is_built_in_type_name(import_name) {
            self.import_names.borrow_mut().add(NameInfo::new(import_name, line));
        }
    }

    pub fn add_import_names(&self, names: &[String], line: i32) {
        for name in names {
            self.add_import_name(name, line);
        }
    }

    pub fn get_import_names(&self) -> NameInfoSet {
        self.import_names.borrow().clone()
    }

    pub fn remove_import_name(&self, import_name: &str) {
        self.import_names.borrow_mut().remove(import_name);
    }

    pub fn add_split_import_name(&self, import_name: &str, split_import_name: Vec<String>) {
        self.split_import_names.borrow_mut().insert(jstr_key::Key(import_name.to_string()), split_import_name);
    }

    pub fn add_split_import_names(&self, names: Vec<(String, Vec<String>)>) {
        for (k, v) in names {
            self.add_split_import_name(&k, v);
        }
    }

    pub fn get_split_import_names(&self) -> Vec<Vec<String>> {
        self.split_import_names.borrow().values().cloned().collect()
    }

    pub fn remove_split_import_name(&self, import_name: &str) {
        self.split_import_names.borrow_mut().remove(&jstr_key::Key(import_name.to_string()));
    }

    pub fn add_state_name(&self, state_name: &str, line: i32) {
        if self.state_groups.borrow().as_ref().is_some_and(|g| g.contains_key(state_name)) {
            toolkit::log(CompilerMessage::error(format!("Identifier '{state_name}' used for both a state group and a state.")), Some(&self.path), line, -1, None);
        }
        self.state_names.borrow_mut().insert(state_name.to_string());
    }

    pub fn add_state_group(&self, group_name: &str, state_name: &str, line: i32) {
        if self.state_names.borrow().contains(group_name) {
            toolkit::log(CompilerMessage::error(format!("Identifier '{group_name}' used for both a state group and a state.")), Some(&self.path), line, -1, None);
        }
        let mut g = self.state_groups.borrow_mut();
        let groups = g.get_or_insert_with(JHashMap::new);
        let mut states = groups.get(group_name).cloned().unwrap_or_default();
        states.push(state_name.to_string());
        groups.insert(group_name.to_string(), states);
    }

    /// `getStateGroups()`, in `HashMap` order.
    pub fn get_state_groups(&self) -> Vec<(String, Vec<String>)> {
        self.state_groups.borrow().as_ref().map(|g| g.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default()
    }

    pub fn get_state_group(&self, name: &str) -> Option<Vec<String>> {
        self.state_groups.borrow().as_ref().and_then(|g| g.get(name).cloned())
    }

    /// `getStateNames()`, in declaration order.
    pub fn get_state_names(&self) -> Vec<String> {
        self.state_names.borrow().iter().cloned().collect()
    }

    pub fn has_state_name(&self, name: &str) -> bool {
        self.state_names.borrow().contains(name)
    }

    pub fn add_var_decl(&self, name: &str, class_name: &str, line: i32) {
        self.var_decls.borrow_mut().insert(name.to_string(), VarDecl { name: name.to_string(), class_name: class_name.to_string(), line });
        self.add_import_name(class_name, line);
    }

    pub fn add_vector_var_decl(&self, name: &str, line: i32, element_type_name: &str) {
        let class_name = format!("{}.<{element_type_name}>", deps::CLASS_VECTOR);
        self.var_decls.borrow_mut().insert(name.to_string(), VarDecl { name: name.to_string(), class_name, line });
        let dot_less_than_index = jstr::last_index_of(element_type_name, ".<");
        if dot_less_than_index != -1 {
            let greater_than_index = jstr::index_of(element_type_name, ">", 0);
            if greater_than_index != -1 {
                self.add_import_name(deps::CLASS_VECTOR, line);
                self.add_import_name(&jstr::substring(element_type_name, dot_less_than_index + 2, greater_than_index), line);
            }
        } else {
            self.add_import_name(element_type_name, line);
        }
    }

    pub fn get_var_decls(&self) -> Vec<VarDecl> {
        self.var_decls.borrow().values().cloned().collect()
    }

    pub fn contains_var_decl(&self, id: &str) -> bool {
        self.var_decls.borrow().contains_key(id)
    }

    pub fn add_script(&self, script: Script) {
        self.scripts.borrow_mut().push(script);
    }

    pub fn get_scripts(&self) -> Vec<Script> {
        self.scripts.borrow().clone()
    }

    pub fn add_metadata(&self, metadatum: Script) {
        self.metadata.borrow_mut().push(metadatum);
    }

    /// `getMetadata().add(0, metadatum)`.
    pub fn add_metadata_first(&self, metadatum: Script) {
        self.metadata.borrow_mut().insert(0, metadatum);
    }

    pub fn get_metadata(&self) -> Vec<Script> {
        self.metadata.borrow().clone()
    }

    pub fn add_local_class(&self, namespace: &str, local_part: &str, class_name: &str) {
        let root = self.root_node.borrow().clone().expect("rootNode");
        deps::document_node_add_local_class(&root, namespace, local_part, class_name);
    }

    pub fn get_local_class(&self, namespace: &str, local_part: &str) -> Option<String> {
        let root = self.root_node.borrow().clone()?;
        deps::document_node_get_local_class(&root, namespace, local_part)
    }

    pub fn get_local_class_mappings(&self) -> Option<Rc<NameMappings>> {
        let root = self.root_node.borrow().clone()?;
        deps::document_node_get_local_class_mappings(&root)
    }

    pub fn get_language_namespace(&self) -> Option<String> {
        self.language_namespace.borrow().clone()
    }

    pub fn set_language_namespace(&self, namespace: Option<&str>) {
        *self.language_namespace.borrow_mut() = namespace.map(Into::into);
    }

    pub fn get_version(&self) -> i32 {
        self.version.get()
    }

    pub fn set_version(&self, version: i32) {
        self.version.set(version);
    }
}
