//! Port of `macromedia/asc/util/Multinames.java`: a `TreeMap<String, Namespaces>`. Java's copy-on-write delegate is an
//! optimisation; here `put_all` copies straight away, which reads the same.

use std::collections::BTreeMap;

use crate::asc::util::jhash::JStr;
use crate::asc::util::namespaces::Namespaces;

#[derive(Clone, Default)]
pub struct Multinames {
    map: BTreeMap<JStr, Namespaces>,
}

impl Multinames {
    pub fn new() -> Multinames {
        Multinames { map: BTreeMap::new() }
    }

    pub fn put_all(&mut self, m: &Multinames) {
        for (k, v) in &m.map {
            self.map.insert(k.clone(), v.clone());
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn put(&mut self, name: &str, nss: Namespaces) -> Option<Namespaces> {
        self.map.insert(JStr(name.to_string()), nss)
    }

    pub fn remove(&mut self, name: &str) -> Option<Namespaces> {
        self.map.remove(name)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<Namespaces> {
        self.map.get(name).cloned()
    }

    pub fn size(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `entrySet()` in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Namespaces)> {
        self.map.iter().map(|(k, v)| (k.0.as_str(), v))
    }

    pub fn first_key(&self) -> Option<String> {
        self.map.keys().next().map(|k| k.0.clone())
    }

    pub fn last_key(&self) -> Option<String> {
        self.map.keys().next_back().map(|k| k.0.clone())
    }
}
