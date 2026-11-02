//! Port of `flex2.compiler.as3.binding.Info`, the members, imports and interfaces `ClassInfo` and `InterfaceInfo`
//! share (held as their `info` field).

use super::interface_info::InterfaceInfoRef;
use crate::asc::flex2::compiler::util::{MultiName, QName};
use crate::asc::util::jstr;

#[derive(Default)]
pub struct Info {
    /// `TreeSet<String>`.
    imports: Option<Vec<String>>,
    /// `TreeMap<String, String>`.
    qualified_imports: Option<Vec<(String, String)>>,
    interface_names: Option<Vec<String>>,
    interface_multi_names: Option<Vec<MultiName>>,
    interface_info_list: Option<Vec<InterfaceInfoRef>>,
    functions: Option<Vec<QName>>,
    getters: Option<Vec<QName>>,
    setters: Option<Vec<QName>>,
}

fn defines(list: &Option<Vec<QName>>, name: &str) -> bool {
    list.as_ref().is_some_and(|l| l.iter().any(|q| q.get_local_part() == name))
}

impl Info {
    pub fn add_function(&mut self, function_name: QName) {
        self.functions.get_or_insert_with(Vec::new).push(function_name);
    }

    pub fn add_getter(&mut self, getter_name: QName) {
        self.getters.get_or_insert_with(Vec::new).push(getter_name);
    }

    pub fn add_import(&mut self, import_name: &str) {
        let imports = self.imports.get_or_insert_with(Vec::new);
        if let Err(i) = imports.binary_search_by(|x| jstr::compare(x, import_name)) {
            imports.insert(i, import_name.to_string());
        }
    }

    pub fn add_interface_multi_name_namespaces(&mut self, namespaces: Vec<String>, interface_name: &str) {
        self.interface_multi_names.get_or_insert_with(Vec::new).push(MultiName::with_namespaces(namespaces, interface_name));
    }

    pub fn add_interface_multi_name(&mut self, namespace: &str, interface_name: &str) {
        self.interface_multi_names.get_or_insert_with(Vec::new).push(MultiName::new(namespace, interface_name));
    }

    pub fn add_interface_name(&mut self, interface_name: &str) {
        self.interface_names.get_or_insert_with(Vec::new).push(interface_name.to_string());
    }

    pub fn add_interface_info(&mut self, interface_info: InterfaceInfoRef) {
        self.interface_info_list.get_or_insert_with(Vec::new).push(interface_info);
    }

    pub fn add_qualified_import(&mut self, local_part: &str, namespace: &str) {
        let q = self.qualified_imports.get_or_insert_with(Vec::new);
        match q.binary_search_by(|x| jstr::compare(&x.0, local_part)) {
            Ok(i) => q[i].1 = namespace.to_string(),
            Err(i) => q.insert(i, (local_part.to_string(), namespace.to_string())),
        }
    }

    pub fn add_setter(&mut self, setter_name: QName) {
        self.setters.get_or_insert_with(Vec::new).push(setter_name);
    }

    pub fn defines_function(&self, function_name: &str) -> bool {
        defines(&self.functions, function_name)
    }

    pub fn defines_getter(&self, getter_name: &str) -> bool {
        defines(&self.getters, getter_name)
    }

    pub fn defines_setter(&self, setter_name: &str) -> bool {
        defines(&self.setters, setter_name)
    }

    pub fn get_function_names(&self) -> Option<&[QName]> {
        self.functions.as_deref()
    }

    pub fn get_imports(&self) -> Option<&[String]> {
        self.imports.as_deref()
    }

    pub fn get_interface_multi_names(&mut self) -> Vec<MultiName> {
        if self.interface_multi_names.is_none() {
            let names = self.interface_names.clone().unwrap_or_default();
            let multi_names = names.iter().map(|n| self.get_multi_name(n)).collect();
            self.interface_multi_names = Some(multi_names);
        }
        self.interface_multi_names.clone().unwrap()
    }

    pub fn get_multi_name(&self, name: &str) -> MultiName {
        let mut last_index = jstr::last_index_of(name, ":");
        if last_index < 0 {
            last_index = jstr::last_index_of(name, ".");
        }
        if last_index > 0 {
            MultiName::with_namespaces(vec![jstr::substring(name, 0, last_index)], &jstr::substring_from(name, last_index + 1))
        } else if let Some(ns) = self.qualified_imports.as_ref().and_then(|q| q.iter().find(|(k, _)| k == name)) {
            MultiName::with_namespaces(vec![ns.1.clone()], name)
        } else if let Some(imports) = &self.imports {
            let mut namespaces = imports.clone();
            namespaces.push(String::new());
            MultiName::with_namespaces(namespaces, name)
        } else {
            MultiName::parse(name)
        }
    }

    pub fn implements_interface(&self, namespace: &str, interface_name: &str) -> bool {
        let mut result = false;
        let full = format!("{namespace}:{interface_name}");
        for interface_info in self.interface_info_list.iter().flatten() {
            let i = interface_info.borrow();
            if i.get_interface_name() == full || i.extends_interface(namespace, interface_name) || i.implements_interface(namespace, interface_name) {
                result = true;
            }
        }
        result
    }
}
