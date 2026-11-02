//! Port of `flex2.compiler.util.PrefixMapping`, the E4X namespace numbering of the XML builders. Its static counter is
//! per compiler thread, as Java's is per mxmlc process.

use std::cell::Cell;

use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;

thread_local! {
    static NAMESPACE_NUM: Cell<i32> = const { Cell::new(0) };
}

fn next_namespace_num() -> i32 {
    NAMESPACE_NUM.with(|n| {
        n.set(n.get() + 1);
        n.get()
    })
}

#[derive(Clone, Debug)]
pub struct PrefixMapping {
    ns: i32,
    uri: String,
}

impl PrefixMapping {
    pub fn new(uri: &str) -> PrefixMapping {
        PrefixMapping { ns: next_namespace_num(), uri: uri.to_string() }
    }

    pub fn get_uri(&self) -> &str {
        &self.uri
    }

    pub fn get_ns(&self) -> i32 {
        self.ns
    }
}

pub fn push_node_namespace(node: &NodeRef, namespaces: &mut Vec<PrefixMapping>) {
    let uri = node.borrow().get_namespace().to_string();
    if let Some(pm) = namespaces.iter().find(|pm| pm.uri == uri).cloned() {
        namespaces.push(pm);
        return;
    }
    namespaces.push(PrefixMapping::new(&uri));
}

pub fn get_namespace_id(ns_uri: &str, namespaces: &[PrefixMapping]) -> i32 {
    match namespaces.iter().find(|pm| pm.uri == ns_uri) {
        Some(pm) => pm.ns,
        None => next_namespace_num(),
    }
}

pub fn pop_node_namespace(namespaces: &mut Vec<PrefixMapping>) {
    namespaces.pop();
}

pub fn push_namespaces(be: &BindingExpressionRef, namespaces: &[PrefixMapping]) {
    for pm in namespaces {
        be.add_namespace(&pm.uri, pm.ns);
    }
}
