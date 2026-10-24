//! Port of `macromedia/asc/util/graph/DependencyGraph.java`.

use std::ops::{Deref, DerefMut};

use super::edge::Edge;
use super::graph::Graph;
use super::vertex::Vertex;
use crate::asc::util::jhash::JHashMap;

/// `DependencyGraph<T> extends Graph<String, Object>`; edge weights are always null.
pub struct DependencyGraph<T> {
    graph: Graph<String, ()>,
    map: JHashMap<String, T>,
    vertices: JHashMap<String, Vertex<String, ()>>,
}

impl<T> Default for DependencyGraph<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Deref for DependencyGraph<T> {
    type Target = Graph<String, ()>;
    fn deref(&self) -> &Graph<String, ()> {
        &self.graph
    }
}

impl<T> DerefMut for DependencyGraph<T> {
    fn deref_mut(&mut self) -> &mut Graph<String, ()> {
        &mut self.graph
    }
}

impl<T> DependencyGraph<T> {
    pub fn new() -> DependencyGraph<T> {
        DependencyGraph { graph: Graph::new(), map: JHashMap::new(), vertices: JHashMap::new() }
    }

    pub fn put(&mut self, key: &str, value: T) {
        self.map.insert(key.to_string(), value);
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.map.get(key)
    }

    pub fn remove(&mut self, key: &str) {
        self.map.remove(key);
    }

    pub fn key_set(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }

    pub fn size(&self) -> usize {
        self.map.len()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn contains_vertex(&self, key: &str) -> bool {
        self.vertices.contains_key(key)
    }

    pub fn add_vertex(&mut self, v: Vertex<String, ()>) {
        self.graph.add_vertex(v.clone());
        let w = v.get_weight().clone();
        self.vertices.insert(w, v);
    }

    pub fn add_dependency(&mut self, name: &str, dep: &str) {
        let head = match self.vertices.get(name) {
            Some(h) => h.clone(),
            None => {
                let h = Vertex::new(name.to_string());
                self.add_vertex(h.clone());
                h
            }
        };

        let tail = match self.vertices.get(dep) {
            Some(t) => t.clone(),
            None => {
                let t = Vertex::new(dep.to_string());
                self.add_vertex(t.clone());
                t
            }
        };

        let e = Edge::new(&tail, &head, None);
        self.graph.add_edge(e);
    }
}
