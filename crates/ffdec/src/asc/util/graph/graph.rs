//! Port of `macromedia/asc/util/graph/Graph.java`.

use super::edge::Edge;
use super::vertex::Vertex;
use crate::asc::util::jhash::{JHashSet, JavaHash};

pub struct Graph<V: JavaHash + Eq, E> {
    counter: i32,
    root: Option<Vertex<V, E>>,
    vertices: JHashSet<Vertex<V, E>>,
    /// `HashSet<Edge>`: identity-hashed, so a list.
    edges: Vec<Edge<V, E>>,
}

impl<V: JavaHash + Eq, E> Default for Graph<V, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: JavaHash + Eq, E> Graph<V, E> {
    pub fn new() -> Graph<V, E> {
        Graph { counter: 0, root: None, vertices: JHashSet::new(), edges: Vec::new() }
    }

    pub fn get_root(&self) -> Option<Vertex<V, E>> {
        self.root.clone()
    }

    pub fn get_vertices(&self) -> &JHashSet<Vertex<V, E>> {
        &self.vertices
    }

    pub fn get_edges(&self) -> &[Edge<V, E>] {
        &self.edges
    }

    pub fn add_vertex(&mut self, v: Vertex<V, E>) {
        if self.vertices.is_empty() {
            self.root = Some(v.clone());
        }
        v.set_id(self.counter);
        self.counter += 1;
        self.vertices.insert(v);
    }

    pub fn add_edge(&mut self, e: Edge<V, E>) {
        self.edges.push(e);
    }

    pub fn normalize(&mut self) {
        self.counter = 0;
        for v in self.vertices.iter() {
            v.set_id(self.counter);
            self.counter += 1;
        }
    }
}

impl<V: JavaHash + Eq, E> Drop for Graph<V, E> {
    fn drop(&mut self) {
        for v in self.vertices.iter() {
            v.clear_links();
        }
    }
}
