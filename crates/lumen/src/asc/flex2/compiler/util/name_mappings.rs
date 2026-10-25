//! Ports of `flex2.compiler.util.NameMappings` (namespace URI to component class names) and `ManifestParser`.

use std::cell::RefCell;

use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::util::jhash::{JHashMap, JHashSet};

#[derive(Clone, Default)]
pub struct NameMappings {
    namespace_map: RefCell<JHashMap<String, JHashMap<String, String>>>,
    lookup_only: RefCell<JHashMap<String, JHashSet<String>>>,
}

impl NameMappings {
    pub fn new() -> NameMappings {
        NameMappings::default()
    }

    pub fn copy(&self) -> NameMappings {
        self.clone()
    }

    pub fn lookup_package_name(&self, ns_uri: &str, local_part: &str) -> Option<String> {
        let class_name = self.lookup_class_name(ns_uri, local_part)?;
        Some(match class_name.find(':') {
            None => String::new(),
            Some(i) => class_name[..i].to_string(),
        })
    }

    pub fn lookup_class_name(&self, ns_uri: &str, local_part: &str) -> Option<String> {
        self.namespace_map.borrow().get(ns_uri).and_then(|m| m.get(local_part).cloned())
    }

    pub fn resolve_class_name(&self, namespace_uri: &str, local_part: &str) -> Option<String> {
        let mut class_name = self.lookup_class_name(namespace_uri, local_part);
        if class_name.is_none() {
            if namespace_uri == "*" {
                class_name = Some(local_part.to_string());
            } else if crate::asc::util::jstr::utf16_len(namespace_uri) > 2 && namespace_uri.ends_with(".*") {
                class_name = Some(format!("{}:{local_part}", &namespace_uri[..namespace_uri.len() - 2]));
            }
        }
        class_name
    }

    pub fn get_namespace(&self, ns_uri: &str) -> Option<JHashMap<String, String>> {
        self.namespace_map.borrow().get(ns_uri).cloned()
    }

    pub fn get_namespaces(&self) -> Vec<String> {
        self.namespace_map.borrow().keys().cloned().collect()
    }

    pub fn add_mappings(&self, other: &NameMappings) {
        let entries: Vec<(String, Vec<(String, String)>)> = other.namespace_map.borrow().iter().map(|(k, v)| (k.clone(), v.iter().map(|(a, b)| (a.clone(), b.clone())).collect())).collect();
        for (namespace_uri, mappings) in entries {
            for (local, class_name) in mappings {
                self.add_class(&namespace_uri, &local, &class_name);
            }
        }
    }

    pub fn add_class(&self, namespace_uri: &str, local_part: &str, class_name: &str) -> bool {
        let mut nm = self.namespace_map.borrow_mut();
        if !nm.contains_key(namespace_uri) {
            nm.insert(namespace_uri.to_string(), JHashMap::new());
        }
        let class_map = nm.get_mut(namespace_uri).unwrap();
        match class_map.get(local_part) {
            None => {
                class_map.insert(local_part.to_string(), class_name.to_string());
                true
            }
            Some(current) => current == class_name,
        }
    }

    pub fn add_lookup_only(&self, namespace_uri: &str, cls: &str) {
        let mut lo = self.lookup_only.borrow_mut();
        if !lo.contains_key(namespace_uri) {
            lo.insert(namespace_uri.to_string(), JHashSet::new());
        }
        lo.get_mut(namespace_uri).unwrap().insert(cls.to_string());
    }

    pub fn is_lookup_only(&self, namespace_uri: &str, cls: &str) -> bool {
        self.lookup_only.borrow().get(namespace_uri).is_some_and(|s| s.contains(cls))
    }
}

/// `ManifestParser.parse(namespaceURI, file, mappings)`.
pub fn parse_manifest(namespace_uri: &str, file: &std::path::Path, mappings: &NameMappings) {
    let Ok(bytes) = std::fs::read(file) else { return };
    let name = crate::asc::flex2::compiler::source::canonical_path(file);
    let text = String::from_utf8_lossy(&bytes);
    let doc = match roxmltree::Document::parse(&text) {
        Ok(d) => d,
        Err(e) => {
            toolkit::log_path(toolkit::Level::Error, &name, &e.to_string(), -1);
            return;
        }
    };
    for node in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "component") {
        let line = doc.text_pos_at(node.range().start).row as i32;
        let mut id = node.attribute("id").map(str::to_string);
        let Some(class_attr) = node.attribute("class") else {
            toolkit::log(CompilerMessage::error(format!("Manifest: Error loading namespace {name}: {line} Component {} has undefined class!", id.as_deref().unwrap_or("null"))), None, -1, -1, None);
            continue;
        };
        let class_name = if class_attr == "*" {
            toolkit::log(CompilerMessage::error(format!("Manifest: Error loading namespace {name}: {line} Component {} can not use * as the defined class.", id.as_deref().unwrap_or("null"))), None, -1, -1, None);
            class_attr.to_string()
        } else {
            super::name_formatter::to_colon(class_attr)
        };
        if id.is_none() {
            id = Some(super::name_formatter::retrieve_class_name(&class_name));
        }
        let id = id.unwrap();
        let lookup_only = node.attribute("lookupOnly").is_some_and(|v| v.eq_ignore_ascii_case("true"));
        if !mappings.add_class(namespace_uri, &id, &class_name) {
            toolkit::log(CompilerMessage::error(format!("Manifest: Error loading namespace {name}: {line} Component {id} is defined more than once in this namespace.")), None, -1, -1, None);
            continue;
        }
        if lookup_only {
            mappings.add_lookup_only(namespace_uri, &class_name);
        }
    }
}

/// `CompilerAPI.getNameMappings(configuration)` over `(namespace, manifests)` in configuration order.
pub fn get_name_mappings(manifests: &[(String, Vec<std::path::PathBuf>)]) -> NameMappings {
    let mappings = NameMappings::new();
    for (ns, files) in manifests {
        for f in files {
            parse_manifest(ns, f, &mappings);
        }
    }
    mappings
}
