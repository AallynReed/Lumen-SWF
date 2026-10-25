//! Port of `macromedia/asc/util/graph/Edge.java`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::vertex::{Vertex, VertexData};
use crate::asc::util::jhash::JavaHash;

struct EdgeData<V, W> {
    head: Weak<RefCell<VertexData<V, W>>>,
    tail: Weak<RefCell<VertexData<V, W>>>,
    weight: Option<W>,
}

/// An `Edge` reference; it holds its vertices weakly, the graph owns them.
pub struct Edge<V, W>(Rc<EdgeData<V, W>>);

impl<V, W> Clone for Edge<V, W> {
    fn clone(&self) -> Self {
        Edge(self.0.clone())
    }
}

impl<V: JavaHash + Eq, W> Edge<V, W> {
    pub fn new(tail: &Vertex<V, W>, head: &Vertex<V, W>, weight: Option<W>) -> Edge<V, W> {
        let e = Edge(Rc::new(EdgeData { head: head.downgrade(), tail: tail.downgrade(), weight }));

        tail.add_emanating_edge(e.clone());
        tail.add_successor(head.clone());
        head.add_incident_edge(e.clone());
        head.add_predecessor(tail.clone());
        e
    }
}

impl<V, W> Edge<V, W> {
    pub fn get_head(&self) -> Vertex<V, W> {
        Vertex::upgrade(&self.0.head)
    }

    pub fn get_tail(&self) -> Vertex<V, W> {
        Vertex::upgrade(&self.0.tail)
    }

    pub fn get_weight(&self) -> Option<&W> {
        self.0.weight.as_ref()
    }

    pub fn ptr_eq(&self, other: &Edge<V, W>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
