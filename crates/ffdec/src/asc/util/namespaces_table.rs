//! Port of `macromedia/asc/util/NamespacesTable.java`: interns `Namespaces` by their elements' identities.

use std::collections::HashMap;

use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::util::namespaces::Namespaces;

/// Entries are filed under their contents at insertion, but match only while their current contents still equal the
/// probe, as a Java `HashMap` does for a key mutated after insertion (callers intern live lists that later grow).
#[derive(Default)]
pub struct NamespacesTable {
    table: HashMap<Vec<usize>, Vec<Namespaces>>,
}

fn key(nss: &Namespaces) -> Vec<usize> {
    nss.borrow().iter().map(|n| n.addr()).collect()
}

impl NamespacesTable {
    pub fn new() -> NamespacesTable {
        NamespacesTable { table: HashMap::new() }
    }

    fn get(&self, k: &[usize]) -> Option<Namespaces> {
        self.table.get(k)?.iter().find(|n| key(n) == k).cloned()
    }

    /// Interns the one-element list `[ns]`. Java's `[null]` for a null qualifier becomes the empty list here.
    pub fn intern_one(&mut self, ns: Option<&ObjectValue>) -> Namespaces {
        let k: Vec<usize> = ns.iter().map(|n| n.addr()).collect();
        if let Some(n) = self.get(&k) {
            return n;
        }
        let n = match ns {
            Some(ns) => Namespaces::of(ns.clone()),
            None => Namespaces::new(),
        };
        self.table.entry(k).or_default().push(n.clone());
        n
    }

    pub fn intern(&mut self, ns: &Namespaces) -> Namespaces {
        let k = key(ns);
        if let Some(n) = self.get(&k) {
            return n;
        }
        self.table.entry(k).or_default().push(ns.clone());
        ns.clone()
    }

    pub fn clear(&mut self) {
        self.table.clear();
    }
}
