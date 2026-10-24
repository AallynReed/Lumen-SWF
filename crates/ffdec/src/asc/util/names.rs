//! Port of `macromedia/asc/util/Names.java`: the name table, with Java's buckets, chain order and growth.

use std::collections::BTreeMap;

use crate::asc::parser::tokens::*;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::util::jhash::{string_hash, JStr};
use crate::asc::util::qualifiers::Qualifiers;

#[derive(Clone, Default)]
pub struct Names {
    name: Vec<String>,
    namespace: Vec<Option<ObjectValue>>,
    slot: Vec<i32>,
    type_: Vec<u8>,
    next: Vec<i32>,
    capacity_names: usize,

    names_used: usize,

    hash_table: Option<Vec<i32>>,
    hash_table_mask: i32,

    is_empty_names: bool,
}

impl Names {
    pub const GET_NAMES: i32 = 0x0;
    pub const SET_NAMES: i32 = 0x1;
    pub const VAR_NAMES: i32 = 0x2;
    pub const METHOD_NAMES: i32 = 0x3;
    pub const LOCAL_METHOD_NAMES: i32 = 0x4;

    const INITIAL_CAPACITY: usize = 8;

    pub fn new() -> Names {
        Names::default()
    }

    /// `Names.EMPTY_NAMES`.
    pub fn empty_names() -> Names {
        Names { is_empty_names: true, ..Names::default() }
    }

    pub fn dump(&self) {
        for i in 0..self.capacity_names {
            let n = if i < self.names_used { Some(&self.name[i]) } else { None };
            let ns = self.namespace.get(i).cloned().flatten();
            println!(
                "{} '{}",
                n.map_or("null", |s| s.as_str()),
                ns.map_or(String::new(), |ns| format!("{}' {}", ns.name(), ns.get_namespace_kind()))
            );
        }
    }

    fn hash(&self, name: &str) -> usize {
        (string_hash(name) & self.hash_table_mask) as usize
    }

    fn find_type(&self, name: &str, ty: i32) -> i32 {
        let i = self.hash(name);
        let ht = self.hash_table.as_ref().unwrap();
        let mut id = ht[i];
        while id != -1 {
            if self.name[id as usize] == name && self.type_[id as usize] as i32 == ty {
                return id;
            }
            id = self.next[id as usize];
        }
        -1
    }

    fn enter(&mut self, name: &str, namespace: &ObjectValue, ty: i32, hash: usize) -> i32 {
        let id = self.names_used;
        if self.names_used >= self.capacity_names {
            self.grow_names();
        }
        self.name[id] = name.to_string();
        self.namespace[id] = Some(namespace.clone());
        let ht = self.hash_table.as_mut().unwrap();
        self.next[id] = ht[hash];
        self.type_[id] = ty as u8;
        self.slot[id] = -1;
        ht[hash] = id as i32;
        self.names_used += 1;
        id as i32
    }

    fn find(&mut self, name: &str, namespace: &ObjectValue, ty: i32, enter: bool) -> i32 {
        let hv = self.hash(name);
        let mut id = self.hash_table.as_ref().unwrap()[hv];
        if id == -1 {
            if enter {
                return self.enter(name, namespace, ty, hv);
            }
            return -1;
        }
        let namespace_name = namespace.name();
        let namespace_kind = namespace.get_namespace_kind();
        loop {
            let i = id as usize;
            if self.name[i] == name && self.type_[i] as i32 == ty {
                let ns = self.namespace[i].as_ref().expect("NullPointerException");
                if namespace_name == ns.name() && namespace_kind == ns.get_namespace_kind() {
                    return id;
                }
            }
            id = self.next[i];
            if id == -1 {
                break;
            }
        }
        if enter {
            return self.enter(name, namespace, ty, hv);
        }
        -1
    }

    fn find_ro(&self, name: &str, namespace: &ObjectValue, ty: i32) -> i32 {
        let hv = self.hash(name);
        let mut id = self.hash_table.as_ref().unwrap()[hv];
        if id == -1 {
            return -1;
        }
        let namespace_name = namespace.name();
        let namespace_kind = namespace.get_namespace_kind();
        loop {
            let i = id as usize;
            if self.name[i] == name && self.type_[i] as i32 == ty {
                let ns = self.namespace[i].as_ref().expect("NullPointerException");
                if namespace_name == ns.name() && namespace_kind == ns.get_namespace_kind() {
                    return id;
                }
            }
            id = self.next[i];
            if id == -1 {
                return -1;
            }
        }
    }

    pub fn size(&self) -> usize {
        if self.is_empty_names {
            return 0;
        }
        self.names_used
    }

    fn is_full(&self) -> bool {
        5 * (self.names_used + 1) >= self.capacity() * 4
    }

    fn capacity(&self) -> usize {
        self.hash_table.as_ref().map_or(0, |h| h.len())
    }

    fn put_entry(&mut self, name: &str, namespace: &ObjectValue, ty: i32) -> i32 {
        if self.hash_table.is_none() || self.is_full() {
            self.grow();
        }
        self.find(name, namespace, ty, true)
    }

    pub fn put_mask(&mut self, name: &str, namespace: &ObjectValue, ty: i32) {
        self.put_entry(name, namespace, ty);
    }

    pub fn put(&mut self, name: &str, namespace: &ObjectValue, ty: i32, slot: i32) {
        let id = self.put_entry(name, namespace, ty);
        self.slot[id as usize] = slot;
    }

    /// `containsKey(name, type)`: whether the newest `{name, type}` entry in the bucket has a slot.
    pub fn contains_key_type(&self, name: &str, ty: i32) -> bool {
        if self.is_empty_names || self.hash_table.is_none() {
            return false;
        }
        let id = self.find_type(name, ty);
        id != -1 && self.slot[id as usize] != -1
    }

    pub fn get(&self, name: &str, namespace: &ObjectValue, ty: i32) -> i32 {
        if self.is_empty_names || self.hash_table.is_none() {
            return -1;
        }
        let id = self.find_ro(name, namespace, ty);
        if id != -1 {
            self.slot[id as usize]
        } else {
            -1
        }
    }

    /// `entrySet(type)`: a sorted copy of the entries of one type.
    pub fn entry_set(&self, ty: i32) -> Vec<(String, Qualifiers)> {
        let mut map: BTreeMap<JStr, Qualifiers> = BTreeMap::new();
        for i in 0..self.names_used {
            if self.type_[i] as i32 == ty {
                let q = map.entry(JStr(self.name[i].clone())).or_default();
                q.put(self.namespace[i].clone().expect("NullPointerException"), self.slot[i]);
            }
        }
        map.into_iter().map(|(k, v)| (k.0, v)).collect()
    }

    pub fn exist(&self, name: &str, ty: i32) -> bool {
        if self.is_empty_names {
            return false;
        }
        let hv = self.hash(name);
        let ht = self.hash_table.as_ref().expect("NullPointerException");
        let mut id = ht[hv];
        while id != -1 {
            if self.name[id as usize] == name && self.type_[id as usize] as i32 == ty {
                return true;
            }
            id = self.next[id as usize];
        }
        false
    }

    /// `get(name, type)`: the qualifiers of every `{name, type}` entry in the name's bucket.
    pub fn get_qualifiers(&self, name: &str, ty: i32) -> Option<Qualifiers> {
        if self.is_empty_names {
            return None;
        }
        let mut q: Option<Qualifiers> = None;
        let hv = self.hash(name);
        let ht = self.hash_table.as_ref().expect("NullPointerException");
        let mut id = ht[hv];
        while id != -1 {
            let i = id as usize;
            if self.name[i] == name && self.type_[i] as i32 == ty {
                q.get_or_insert_with(Qualifiers::new).put(self.namespace[i].clone().expect("NullPointerException"), self.slot[i]);
            }
            id = self.next[i];
        }
        q
    }

    pub fn put_all(&mut self, names: &Names) {
        for i in 0..names.names_used {
            let ns = names.namespace[i].clone().expect("NullPointerException");
            self.put(&names.name[i].clone(), &ns, names.type_[i] as i32, names.slot[i]);
        }
    }

    pub fn contains_key(&self, name: &str, namespace: &ObjectValue, ty: i32) -> bool {
        if self.is_empty_names || self.hash_table.is_none() {
            return false;
        }
        let id = self.find_ro(name, namespace, ty);
        id != -1 && self.slot[id as usize] != -1
    }

    fn new_hash_table(&mut self, size: usize) {
        self.hash_table = Some(vec![-1; size]);
        self.hash_table_mask = size as i32 - 1;
    }

    fn grow_names(&mut self) {
        if self.capacity_names == 0 {
            let c = Self::INITIAL_CAPACITY;
            self.name = vec![String::new(); c];
            self.namespace = vec![None; c];
            self.slot = vec![0; c];
            self.next = vec![0; c];
            self.type_ = vec![0; c];
            self.capacity_names = c;
            return;
        }
        let l = if self.capacity_names < 256 { self.capacity_names * 2 } else { self.capacity_names + 256 };
        self.name.resize(l, String::new());
        self.namespace.resize(l, None);
        self.slot.resize(l, 0);
        self.next.resize(l, 0);
        self.type_.resize(l, 0);
        self.capacity_names = l;
    }

    fn grow(&mut self) {
        if self.hash_table.is_none() {
            self.new_hash_table(Self::INITIAL_CAPACITY);
            self.grow_names();
            return;
        }
        let cap = self.capacity() * 2;
        self.new_hash_table(cap);
        for i in 0..self.names_used {
            let hv = self.hash(&self.name[i]);
            let ht = self.hash_table.as_mut().unwrap();
            self.next[i] = ht[hv];
            ht[hv] = i as i32;
        }
    }

    pub fn get_type_from_kind(kind: i32) -> i32 {
        match kind {
            GET_TOKEN => Self::GET_NAMES,
            SET_TOKEN => Self::SET_NAMES,
            VAR_TOKEN => Self::VAR_NAMES,
            _ => Self::METHOD_NAMES,
        }
    }

    pub fn get_name(&self, i: usize) -> String {
        self.name[i].clone()
    }

    pub fn get_namespace(&self, i: usize) -> ObjectValue {
        self.namespace[i].clone().expect("NullPointerException")
    }

    pub fn get_slot(&self, i: usize) -> i32 {
        self.slot[i]
    }

    pub fn get_type(&self, i: usize) -> i32 {
        self.type_[i] as i32
    }

    pub fn has_next(&self, index: i32) -> i32 {
        if (index as usize) < self.names_used {
            index
        } else {
            -1
        }
    }
}
