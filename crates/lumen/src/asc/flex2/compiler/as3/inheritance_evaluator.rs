//! Port of `flex2.compiler.as3.InheritanceEvaluator`: the superclass and interface names of a source, before any type
//! information exists.

use std::collections::BTreeMap;

use super::evaluator_adapter::{walk, Adapter};
use crate::asc::flex2::compiler::util::{MultiName, Name};
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::{JHashSet, JStr};

pub struct InheritanceEvaluator {
    imports: Option<Vec<String>>,
    qualified_imports: Option<BTreeMap<JStr, String>>,
    inheritance_names: Vec<String>,
    inheritance_multi_names: JHashSet<Name>,
    definition_multi_names: Vec<MultiName>,
}

impl Default for InheritanceEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

fn id_name(n: &Option<NodeRef>) -> String {
    n.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default()
}

fn pkg_part(package_name: &Option<NodeRef>) -> Option<String> {
    let pn = package_name.as_ref()?;
    let pn = pn.borrow();
    let id = pn.as_package_name()?.id.clone()?;
    let id = id.borrow();
    id.as_package_identifiers()?.pkg_part.clone()
}

fn import_parts(node: &NodeRef) -> Option<(String, String)> {
    let b = node.borrow();
    let name = b.as_import_directive()?.name.clone()?;
    let name = name.borrow();
    let id = name.as_package_name()?.id.clone()?;
    let id = id.borrow();
    let pi = id.as_package_identifiers()?;
    Some((pi.def_part.clone().unwrap_or_default(), pi.pkg_part.clone().unwrap_or_default()))
}

/// `toString(IdentifierNode)`.
fn identifier_string(identifier: &NodeRef) -> String {
    let b = identifier.borrow();
    if let Some(q) = b.qualified_identifier() {
        if let Some(lit) = q.qualifier.as_ref().and_then(|q| q.borrow().as_literal_string().map(|l| l.value.clone())) {
            return format!("{}:{}", lit, q.id.name);
        }
        panic!("Unhandled QualifiedIdentifierNode qualifier type");
    }
    b.identifier().map(|i| i.name.clone()).unwrap_or_default()
}

/// `memberExpression.selector.getIdentifier()` of a base class or interface expression.
fn selector_identifier(member: &NodeRef) -> Option<NodeRef> {
    let b = member.borrow();
    let sel = b.as_member_expression()?.selector.clone()?;
    let sel = sel.borrow();
    sel.selector().and_then(|s| s.get_identifier())
}

impl InheritanceEvaluator {
    pub fn new() -> InheritanceEvaluator {
        let mut e = InheritanceEvaluator {
            imports: None,
            qualified_imports: None,
            inheritance_names: Vec::new(),
            inheritance_multi_names: JHashSet::new(),
            definition_multi_names: Vec::new(),
        };
        e.add_import("");
        e
    }

    /// A `TreeSet`, so kept sorted in Java string order.
    fn add_import(&mut self, import_name: &str) {
        let imports = self.imports.get_or_insert_with(Vec::new);
        if let Err(pos) = imports.binary_search_by(|p| crate::asc::util::jhash::java_compare(p, import_name)) {
            imports.insert(pos, import_name.to_string());
        }
    }

    fn add_qualified_import(&mut self, local_part: &str, namespace: &str) {
        self.qualified_imports.get_or_insert_with(BTreeMap::new).insert(JStr(local_part.to_string()), namespace.to_string());
    }

    fn process_imports(&mut self, statements: &Option<NodeRef>) {
        let Some(st) = statements else { return };
        let items: Vec<NodeRef> = st.borrow().as_statement_list().map(|l| l.items.iter().flatten().cloned().collect()).unwrap_or_default();
        for n in items {
            if let Some((def_part, pkg)) = import_parts(&n) {
                if def_part.is_empty() {
                    self.add_import(&pkg);
                } else {
                    self.add_qualified_import(&def_part, &pkg);
                }
            }
        }
    }

    fn process_interfaces(&mut self, interfaces: &Option<NodeRef>) {
        let Some(interfaces) = interfaces else { return };
        let items: Vec<NodeRef> = interfaces.borrow().list().map(|l| l.items.iter().flatten().cloned().collect()).unwrap_or_default();
        for member in items {
            let Some(identifier) = selector_identifier(&member) else { continue };
            let name = identifier_string(&identifier);
            let ns = identifier.borrow().identifier().and_then(|i| i.ref_.clone()).map(|r| r.data().namespaces.get(0).name());
            match ns {
                Some(ns) if !ns.is_empty() => {
                    self.inheritance_multi_names.insert(Name::M(MultiName::new(&ns, &name)));
                }
                _ => self.inheritance_names.push(name),
            }
        }
    }

    /// `evaluate(ClassDefinitionNode)`, shared by `evaluateInterface` except where noted.
    fn class_definition(&mut self, node: &NodeRef, interface: bool) {
        let (pkgdef, name, statements, baseclass, interfaces) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (c.def.pkgdef.clone(), id_name(&c.name), c.statements.clone(), c.baseclass.clone(), c.interfaces.clone())
        };
        if let Some(pkgdef) = &pkgdef {
            let (pkg_statements, pkg_name) = {
                let p = pkgdef.borrow();
                let p = p.as_package_definition().unwrap();
                (p.statements.clone(), p.name.clone())
            };
            self.process_imports(&pkg_statements);
            if let Some(pkg) = pkg_part(&pkg_name) {
                self.definition_multi_names.push(MultiName::new(&pkg, &name));
            }
        }
        self.process_imports(&statements);
        match &baseclass {
            Some(base) => {
                let lit = base.borrow().as_literal_string().map(|l| l.value.clone());
                if base.borrow().as_member_expression().is_some() {
                    if let Some(identifier) = selector_identifier(base) {
                        self.inheritance_names.push(identifier_string(&identifier));
                    }
                } else if let (Some(lit), false) = (lit, interface) {
                    self.inheritance_names.push(lit);
                }
            }
            None => self.inheritance_names.push(":Object".to_string()),
        }
        self.process_interfaces(&interfaces);
    }

    fn binary_import(&mut self, node: &NodeRef) {
        let ns = node.borrow().class_definition().and_then(|c| c.cframe.clone()).and_then(|f| f.name_opt()).and_then(|q| q.ns()).map(|ns| ns.name());
        if let Some(ns) = ns {
            if !ns.is_empty() {
                self.add_import(&ns);
            }
        }
    }

    pub fn get_inheritance(mut self) -> JHashSet<Name> {
        for name in std::mem::take(&mut self.inheritance_names) {
            let m = self.get_multi_name(&name);
            self.inheritance_multi_names.insert(Name::M(m));
        }
        let mut remove = Vec::new();
        for n in self.inheritance_multi_names.iter() {
            let Name::M(im) = n else { continue };
            for d in &self.definition_multi_names {
                let namespace = &d.namespace_uri[0];
                if im.local_part == d.local_part && im.namespace_uri.iter().any(|ns| ns == namespace) {
                    remove.push(n.clone());
                }
            }
        }
        for n in remove {
            self.inheritance_multi_names.remove(&n);
        }
        self.inheritance_multi_names
    }

    fn get_multi_name(&self, name: &str) -> MultiName {
        let last = name.rfind(':').or_else(|| name.rfind('.'));
        if let Some(i) = last {
            MultiName::with_namespaces(vec![name[..i].to_string()], &name[i + 1..])
        } else if let Some(ns) = self.qualified_imports.as_ref().and_then(|q| q.get(&JStr(name.to_string()))) {
            MultiName::with_namespaces(vec![ns.clone()], name)
        } else if let Some(imports) = &self.imports {
            MultiName::with_namespaces(imports.clone(), name)
        } else {
            MultiName::parse(name)
        }
    }
}

impl Adapter for InheritanceEvaluator {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let kind = match &node.borrow().kind {
            Kind::BinaryInterfaceDefinition(_) => 1,
            Kind::BinaryClassDef(_) => 2,
            Kind::ClassDefinition(_) => 3,
            Kind::InterfaceDefinition(_) => 4,
            Kind::ImportDirective(_) => 5,
            Kind::PackageDefinition(_) => 6,
            _ => 0,
        };
        match kind {
            1 => {
                self.binary_import(node);
                self.class_definition(node, true);
            }
            2 => {
                self.binary_import(node);
                self.class_definition(node, false);
            }
            3 => self.class_definition(node, false),
            4 => self.class_definition(node, true),
            5 => {
                if let Some((def_part, pkg)) = import_parts(node) {
                    if def_part.is_empty() {
                        self.add_import(&pkg);
                    } else {
                        self.add_qualified_import(&def_part, &pkg);
                    }
                }
            }
            6 => {
                let name = node.borrow().as_package_definition().unwrap().name.clone();
                if let Some(pkg) = pkg_part(&name) {
                    self.add_import(&pkg);
                }
            }
            _ => {
                walk(self, cx, node);
            }
        }
        true
    }
}
