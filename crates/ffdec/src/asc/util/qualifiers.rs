//! Port of `macromedia/asc/util/Qualifiers.java`: a `TreeMap<ObjectValue, Integer>` ordered by `ObjectValueCompare`.

use crate::asc::semantics::object_value::{object_value_compare, ObjectValue};

#[derive(Clone, Default)]
pub struct Qualifiers {
    entries: Vec<(ObjectValue, i32)>,
}

impl Qualifiers {
    pub fn new() -> Qualifiers {
        Qualifiers { entries: Vec::new() }
    }

    fn find(&self, ns: &ObjectValue) -> Result<usize, usize> {
        self.entries.binary_search_by(|(k, _)| object_value_compare(k, ns).cmp(&0))
    }

    /// `put`: a key that compares equal (same name and kind) replaces the value but keeps the first key.
    pub fn put(&mut self, ns: ObjectValue, slot: i32) -> Option<i32> {
        match self.find(&ns) {
            Ok(i) => Some(std::mem::replace(&mut self.entries[i].1, slot)),
            Err(i) => {
                self.entries.insert(i, (ns, slot));
                None
            }
        }
    }

    pub fn get(&self, ns: &ObjectValue) -> Option<i32> {
        self.find(ns).ok().map(|i| self.entries[i].1)
    }

    pub fn contains_key(&self, ns: &ObjectValue) -> bool {
        self.find(ns).is_ok()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ObjectValue, i32)> {
        self.entries.iter().map(|(k, v)| (k, *v))
    }

    pub fn key_set(&self) -> Vec<ObjectValue> {
        self.entries.iter().map(|(k, _)| k.clone()).collect()
    }
}
