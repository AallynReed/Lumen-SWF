//! Port of `macromedia/asc/util/graph/Vertex.java`. Equality and hash are the weight's, as in Java.

use std::cell::{Ref, RefCell};
use std::rc::{Rc, Weak};

use super::edge::Edge;
use crate::asc::util::jhash::{JHashSet, JavaHash};

const INITIAL_CAPACITY: usize = 5;

pub struct VertexData<V, W> {
    weight: V,
    id: i32,
    /// `HashSet<Edge>`: edges hash by identity, so every added edge is kept.
    incident_edges: Option<Vec<Edge<V, W>>>,
    emanating_edges: Option<Vec<Edge<V, W>>>,
    predecessors: Option<JHashSet<Vertex<V, W>>>,
    successors: Option<Vec<Vertex<V, W>>>,
}

pub struct Vertex<V, W>(Rc<RefCell<VertexData<V, W>>>);

impl<V, W> Clone for Vertex<V, W> {
    fn clone(&self) -> Self {
        Vertex(self.0.clone())
    }
}

impl<V: PartialEq, W> PartialEq for Vertex<V, W> {
    fn eq(&self, other: &Self) -> bool {
        self.0.borrow().weight == other.0.borrow().weight
    }
}

impl<V: Eq, W> Eq for Vertex<V, W> {}

impl<V: JavaHash, W> JavaHash for Vertex<V, W> {
    fn java_hash(&self) -> i32 {
        self.0.borrow().weight.java_hash()
    }
}

impl<V, W> Vertex<V, W> {
    pub fn new(weight: V) -> Vertex<V, W> {
        Vertex(Rc::new(RefCell::new(VertexData {
            weight,
            id: 0,
            incident_edges: None,
            emanating_edges: None,
            predecessors: None,
            successors: None,
        })))
    }

    pub(super) fn downgrade(&self) -> Weak<RefCell<VertexData<V, W>>> {
        Rc::downgrade(&self.0)
    }

    pub(super) fn upgrade(w: &Weak<RefCell<VertexData<V, W>>>) -> Vertex<V, W> {
        Vertex(w.upgrade().expect("vertex outlived its graph"))
    }

    pub(super) fn set_id(&self, id: i32) {
        self.0.borrow_mut().id = id;
    }

    /// Drops the adjacency lists, which form reference cycles.
    pub(super) fn clear_links(&self) {
        let mut d = self.0.borrow_mut();
        d.incident_edges = None;
        d.emanating_edges = None;
        d.predecessors = None;
        d.successors = None;
    }

    pub fn ptr_eq(&self, other: &Vertex<V, W>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn id(&self) -> i32 {
        self.0.borrow().id
    }

    pub fn get_weight(&self) -> Ref<'_, V> {
        Ref::map(self.0.borrow(), |d| &d.weight)
    }

    pub fn add_incident_edge(&self, e: Edge<V, W>) {
        self.0.borrow_mut().incident_edges.get_or_insert_with(|| Vec::with_capacity(INITIAL_CAPACITY)).push(e);
    }

    pub fn get_incident_edges(&self) -> Option<Vec<Edge<V, W>>> {
        self.0.borrow().incident_edges.clone()
    }

    pub fn add_emanating_edge(&self, e: Edge<V, W>) {
        self.0.borrow_mut().emanating_edges.get_or_insert_with(|| Vec::with_capacity(INITIAL_CAPACITY)).push(e);
    }

    pub fn get_emanating_edges(&self) -> Option<Vec<Edge<V, W>>> {
        self.0.borrow().emanating_edges.clone()
    }

    pub fn add_successor(&self, v: Vertex<V, W>) {
        self.0.borrow_mut().successors.get_or_insert_with(|| Vec::with_capacity(INITIAL_CAPACITY)).push(v);
    }

    pub fn get_successors(&self) -> Option<Vec<Vertex<V, W>>> {
        self.0.borrow().successors.clone()
    }

    pub fn in_degrees(&self) -> i32 {
        self.0.borrow().incident_edges.as_ref().map_or(0, |e| e.len() as i32)
    }

    pub fn out_degrees(&self) -> i32 {
        self.0.borrow().emanating_edges.as_ref().map_or(0, |e| e.len() as i32)
    }
}

impl<V: JavaHash + Eq, W> Vertex<V, W> {
    pub fn add_predecessor(&self, v: Vertex<V, W>) {
        self.0.borrow_mut().predecessors.get_or_insert_with(|| JHashSet::with_capacity(INITIAL_CAPACITY)).insert(v);
    }

    pub fn get_predecessors(&self) -> Option<Vec<Vertex<V, W>>> {
        self.0.borrow().predecessors.as_ref().map(|p| p.iter().cloned().collect())
    }
}
