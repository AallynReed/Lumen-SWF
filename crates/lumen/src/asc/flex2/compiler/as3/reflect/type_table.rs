//! Port of `flex2.compiler.as3.reflect.TypeTable`: class lookup through the symbol table's class map, which it shares
//! (weakly) instead of holding the `SymbolTable`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::as3_class::AbcClass;
use super::meta_data::MetaData;
use crate::asc::flex2::compiler::symbol_table::{SymbolTable, NOTYPE};
use crate::asc::flex2::compiler::util::QNameList;
use crate::asc::parser::node::{DefinitionNode, NodeRef};
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::util::jhash::JHashMap;

/// `SymbolTable.classTable`, shared with the type tables made from it.
pub type ClassTable = Rc<RefCell<JHashMap<String, Rc<AbcClass>>>>;

pub fn new_class_table() -> ClassTable {
    Rc::new(RefCell::new(JHashMap::with_capacity(300)))
}

thread_local! {
    static NO_TYPE_CLASS: Rc<AbcClass> = Rc::new(AbcClass::no_type());
}

/// `SymbolTable.getClass(className)`.
pub fn get_class_from(class_table: &JHashMap<String, Rc<AbcClass>>, class_name: Option<&str>) -> Option<Rc<AbcClass>> {
    let class_name = class_name?;
    if class_name == NOTYPE {
        return Some(NO_TYPE_CLASS.with(|c| c.clone()));
    }
    class_table.get(class_name).cloned()
}

#[derive(Clone, Default)]
pub struct TypeTable {
    class_table: Weak<RefCell<JHashMap<String, Rc<AbcClass>>>>,
}

impl TypeTable {
    pub fn new(symbol_table: &SymbolTable) -> TypeTable {
        TypeTable::from_class_table(&symbol_table.class_table())
    }

    pub fn from_class_table(class_table: &ClassTable) -> TypeTable {
        TypeTable { class_table: Rc::downgrade(class_table) }
    }

    /// The symbol table's class map, while the symbol table lives.
    pub fn class_table(&self) -> Option<ClassTable> {
        self.class_table.upgrade()
    }

    pub fn get_class(&self, class_name: Option<&str>) -> Option<Rc<AbcClass>> {
        let table = self.class_table.upgrade().expect("NullPointerException");
        let t = table.borrow();
        get_class_from(&t, class_name)
    }

    pub fn convert_name(typeref: &ReferenceValue) -> String {
        let ns = typeref.data().namespaces.to_vec().first().cloned();
        match ns {
            Some(ns) if !ns.name().is_empty() => format!("{}:{}", ns.name(), typeref.name()),
            _ => typeref.name(),
        }
    }

    /// `createClasses(clsdefs, toplevelDefinitions)`.
    pub fn create_classes(&self, clsdefs: &[NodeRef], toplevel_definitions: &QNameList) -> JHashMap<String, Rc<AbcClass>> {
        let mut classes = JHashMap::new();
        for clsdef in clsdefs {
            let q = {
                let b = clsdef.borrow();
                let c = b.class_definition().expect("ClassCastException");
                c.cframe.as_ref().expect("NullPointerException").builder().and_then(|bu| bu.classname()).expect("NullPointerException")
            };
            let ns = q.ns().map(|n| n.name()).unwrap_or_default();
            if toplevel_definitions.contains(&ns, q.name()) {
                let cls = Rc::new(AbcClass::new(clsdef, Some(self.clone())));
                classes.insert(cls.get_name().to_string(), cls);
            }
        }
        classes
    }

    /// `createMetaData(def)`: null when the definition has no metadata.
    pub fn create_meta_data(def: &DefinitionNode) -> Option<Vec<MetaData>> {
        let mut list: Option<Vec<MetaData>> = None;
        let Some(md) = def.meta_data.as_ref() else { return None };
        let md = md.borrow();
        let items = md.as_statement_list().map(|l| l.items.clone()).unwrap_or_default();
        for item in items.into_iter().flatten() {
            let n = item.borrow();
            let n = n.meta_data().expect("ClassCastException");
            list.get_or_insert_with(Vec::new).push(MetaData::from_node(n));
        }
        list
    }
}
