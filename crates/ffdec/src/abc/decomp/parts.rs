//! Graph parts (basic blocks), loops and throw states, kept in arenas and referenced by index.
//!
//! FFDec's `GraphPart.equals` compares `start` and `end`, and several decisions depend on Java's
//! `HashSet` iteration order, so `PartSet` reproduces that order.

use super::item::PartId as P;

/// One branch taken: (ip, index), linked to the branches before it.
#[derive(Debug, PartialEq, Eq)]
pub struct Branch {
    pub ip: i32,
    pub index: i32,
    pub prev: Option<std::rc::Rc<Branch>>,
}

impl Drop for Branch {
    fn drop(&mut self) {
        let mut prev = self.prev.take();
        while let Some(rc) = prev {
            match std::rc::Rc::try_unwrap(rc) {
                Ok(mut b) => prev = b.prev.take(),
                Err(_) => break,
            }
        }
    }
}

/// `GraphPath`, as a shared list so `sub` does not copy the whole history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphPath {
    pub root: String,
    pub last: Option<std::rc::Rc<Branch>>,
    pub len: usize,
}

impl GraphPath {
    pub fn root(name: &str) -> GraphPath {
        GraphPath { root: name.into(), ..Default::default() }
    }

    pub fn sub(&self, index: i32, ip: i32) -> GraphPath {
        GraphPath { root: self.root.clone(), last: Some(std::rc::Rc::new(Branch { ip, index, prev: self.last.clone() })), len: self.len + 1 }
    }
}

#[derive(Debug, Clone)]
pub struct Part {
    pub start: i32,
    pub end: i32,
    pub next: Vec<P>,
    pub refs: Vec<P>,
    pub path: GraphPath,
    pub level: i32,
    pub closed_time: i32,
}

impl Part {
    pub fn new(start: i32, end: i32) -> Part {
        Part { start, end, next: Vec::new(), refs: Vec::new(), path: GraphPath::default(), level: 0, closed_time: 0 }
    }

    pub fn height(&self) -> i32 {
        self.end - self.start + 1
    }

    pub fn contains_ip(&self, ip: i32) -> bool {
        ip >= self.start && ip <= self.end
    }
}

/// Java `HashMap` bucket of a hash code for a table of `cap` entries.
fn bucket(h: i32, cap: usize) -> usize {
    let h = h as u32;
    ((h ^ (h >> 16)) as usize) & (cap - 1)
}

fn table_size_for(c: usize) -> usize {
    c.max(1).next_power_of_two()
}

/// A `HashSet<GraphPart>`: equality by `(start, end)`, iteration in Java's bucket order.
#[derive(Debug, Clone)]
pub struct PartSet {
    items: Vec<(P, i32, i32)>,
    cap: usize,
}

impl Default for PartSet {
    fn default() -> Self {
        PartSet { items: Vec::new(), cap: 16 }
    }
}

impl PartSet {
    pub fn new() -> PartSet {
        PartSet::default()
    }

    /// `new HashSet<>(collection)`.
    pub fn from_parts(parts: &[Part], ids: &[P]) -> PartSet {
        let n = ids.len();
        let cap = table_size_for(((n as f32 / 0.75) as usize + 1).max(16));
        let mut s = PartSet { items: Vec::new(), cap };
        for &id in ids {
            s.insert(parts, id);
        }
        s
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, parts: &[Part], id: P) -> bool {
        let p = &parts[id];
        self.items.iter().any(|&(_, s, e)| s == p.start && e == p.end)
    }

    pub fn insert(&mut self, parts: &[Part], id: P) -> bool {
        if self.contains(parts, id) {
            return false;
        }
        let p = &parts[id];
        self.items.push((id, p.start, p.end));
        if self.items.len() > self.cap * 3 / 4 {
            self.cap *= 2;
        }
        true
    }

    pub fn remove(&mut self, parts: &[Part], id: P) -> bool {
        let p = &parts[id];
        match self.items.iter().position(|&(_, s, e)| s == p.start && e == p.end) {
            Some(i) => {
                self.items.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Members in Java iteration order.
    pub fn iter(&self) -> Vec<P> {
        let mut v: Vec<(usize, usize, P)> = self.items.iter().enumerate().map(|(i, &(id, s, _))| (bucket(249 + s, self.cap), i, id)).collect();
        v.sort();
        v.into_iter().map(|(_, _, id)| id).collect()
    }

    pub fn first(&self) -> Option<P> {
        self.iter().first().copied()
    }
}

/// A `LinkedHashSet<GraphPart>`: insertion order, equality by `(start, end)`.
#[derive(Debug, Clone, Default)]
pub struct PartList(pub Vec<P>);

impl PartList {
    pub fn contains(&self, parts: &[Part], id: P) -> bool {
        self.0.iter().any(|&x| peq(parts, x, id))
    }

    pub fn insert(&mut self, parts: &[Part], id: P) -> bool {
        if self.contains(parts, id) {
            return false;
        }
        self.0.push(id);
        true
    }

    pub fn remove(&mut self, parts: &[Part], id: P) {
        if let Some(i) = self.0.iter().position(|&x| peq(parts, x, id)) {
            self.0.remove(i);
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `GraphPart.equals`.
pub fn peq(parts: &[Part], a: P, b: P) -> bool {
    a == b || (parts[a].start == parts[b].start && parts[a].end == parts[b].end)
}

pub fn peq_opt(parts: &[Part], a: Option<P>, b: Option<P>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => peq(parts, a, b),
        (None, None) => true,
        _ => false,
    }
}

/// `list.contains(part)`.
pub fn pcontains(parts: &[Part], list: &[P], id: P) -> bool {
    list.iter().any(|&x| peq(parts, x, id))
}

pub fn pindex_of(parts: &[Part], list: &[P], id: P) -> Option<usize> {
    list.iter().position(|&x| peq(parts, x, id))
}

/// `GraphPartEdge`, compared with part equality.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub from: Option<P>,
    pub to: P,
}

pub fn edge_eq(parts: &[Part], a: &Edge, b: &Edge) -> bool {
    peq_opt(parts, a.from, b.from) && peq(parts, a.to, b.to)
}

pub fn edges_contain(parts: &[Part], list: &[Edge], e: &Edge) -> bool {
    list.iter().any(|x| edge_eq(parts, x, e))
}

pub fn edges_insert(parts: &[Part], list: &mut Vec<Edge>, e: Edge) {
    if !edges_contain(parts, list, &e) {
        list.push(e);
    }
}

#[derive(Debug, Clone)]
pub struct Loop {
    pub cont: Option<P>,
    pub brk: Option<P>,
    pub pre_cont: Option<P>,
    pub back_edges: PartSet,
    pub id: i64,
    pub reachable_mark: i32,
    pub phase: i32,
    pub body: PartList,
    pub edges_outside: Vec<Edge>,
    pub order_part: Option<P>,
    pub parent: Option<usize>,
}

impl Loop {
    pub fn new(id: i64, cont: Option<P>, brk: Option<P>) -> Loop {
        Loop { cont, brk, pre_cont: None, back_edges: PartSet::new(), id, reachable_mark: 0, phase: 0, body: PartList::default(), edges_outside: Vec::new(), order_part: None, parent: None }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ThrowState {
    pub exception_id: i32,
    pub state: i32,
    pub throwing: PartSet,
    pub target: Option<P>,
    pub start: Option<P>,
    pub catch_parts: PartSet,
}
