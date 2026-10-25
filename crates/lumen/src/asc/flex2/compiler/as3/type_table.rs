//! The parts of `flex2.compiler.as3.reflect` (`TypeTable`, `As3Class`, `MetaData`) a pure-ActionScript build reads:
//! class names, superclass names and class metadata, for `[Frame]` lookups.

use crate::asc::parser::node::NodeRef;
use crate::asc::semantics::meta_data::MetaData;

pub struct AbcClass {
    pub name: String,
    pub super_type_name: Option<String>,
    pub metadata: Vec<MetaData>,
}

impl AbcClass {
    pub fn no_type() -> AbcClass {
        AbcClass { name: "*".to_string(), super_type_name: None, metadata: Vec::new() }
    }

    /// `new As3Class(clsdef, typeTable)`.
    pub fn from_class_definition(clsdef: &NodeRef) -> AbcClass {
        let n = clsdef.borrow();
        let c = n.class_definition().expect("ClassCastException");
        let cframe = c.cframe.clone().expect("NullPointerException");
        let name = cframe.builder().and_then(|b| b.classname()).map(|q| q.to_string()).unwrap_or_default();
        let super_type_name = cframe.baseclass().map(|b| b.name().to_string());
        let mut metadata = Vec::new();
        if let Some(md) = n.definition().and_then(|d| d.meta_data.clone()) {
            for item in md.borrow().as_statement_list().map(|l| l.items.clone()).unwrap_or_default().into_iter().flatten() {
                if let Some(m) = item.borrow().meta_data().and_then(|m| m.get_metadata()) {
                    metadata.push(m);
                }
            }
        }
        AbcClass { name, super_type_name, metadata }
    }

    /// `getMetaData(id, inherited)`; `lookup` is `TypeTable.getClass`.
    pub fn get_meta_data(&self, id: &str, inherited: bool, lookup: &dyn Fn(&str) -> Option<std::rc::Rc<AbcClass>>) -> Vec<MetaData> {
        let mut list: Vec<MetaData> = self.metadata.iter().filter(|m| m.id().as_deref() == Some(id)).cloned().collect();
        if inherited {
            if let Some(sup) = self.super_type_name.as_deref().and_then(lookup) {
                list.extend(sup.get_meta_data(id, true, lookup));
            }
        }
        list
    }
}
