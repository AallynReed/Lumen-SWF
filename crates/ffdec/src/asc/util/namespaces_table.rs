//! Port of `macromedia/asc/util/NamespacesTable.java`: interns `Namespaces` by their elements' identities.

use std::collections::HashMap;

use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::util::namespaces::Namespaces;

#[derive(Default)]
pub struct NamespacesTable {
    table: HashMap<Vec<usize>, Namespaces>,
}

fn key(nss: &Namespaces) -> Vec<usize> {
    nss.borrow().iter().map(|n| n.addr()).collect()
}

impl NamespacesTable {
    pub fn new() -> NamespacesTable {
        NamespacesTable { table: HashMap::new() }
    }

    /// Interns the one-element list `[ns]`. Java's `[null]` for a null qualifier becomes the empty list here.
    pub fn intern_one(&mut self, ns: Option<&ObjectValue>) -> Namespaces {
        let k: Vec<usize> = ns.iter().map(|n| n.addr()).collect();
        if let Some(n) = self.table.get(&k) {
            return n.clone();
        }
        let n = match ns {
            Some(ns) => Namespaces::of(ns.clone()),
            None => Namespaces::new(),
        };
        self.table.insert(k, n.clone());
        n
    }

    pub fn intern(&mut self, ns: &Namespaces) -> Namespaces {
        let k = key(ns);
        if let Some(n) = self.table.get(&k) {
            return n.clone();
        }
        self.table.insert(k, ns.clone());
        ns.clone()
    }

    pub fn clear(&mut self) {
        self.table.clear();
    }
}
