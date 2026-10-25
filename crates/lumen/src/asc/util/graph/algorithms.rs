//! Port of `macromedia/asc/util/graph/Algorithms.java`.

use std::collections::VecDeque;

use super::graph::Graph;
use super::vertex::Vertex;
use super::visitor::Visitor;
use crate::asc::util::jhash::JavaHash;

pub fn topological_sort<V: JavaHash + Eq, E>(g: &Graph<V, E>, mut visitor: Option<&mut dyn Visitor<V, E>>) {
    let n = g.get_vertices().len();
    let mut in_degree = vec![0i32; n];
    let mut vertices: Vec<Option<Vertex<V, E>>> = vec![None; n];

    for v in g.get_vertices().iter() {
        let id = v.id() as usize;
        vertices[id] = Some(v.clone());
        in_degree[id] = v.in_degrees();
    }

    let mut queue = VecDeque::new();
    for i in 0..n {
        if in_degree[i] == 0 {
            queue.push_back(vertices[i].clone());
        }
    }

    while let Some(v) = queue.pop_front() {
        let v = v.expect("NullPointerException");
        if let Some(visitor) = visitor.as_mut() {
            visitor.visit(&v);
        }
        if let Some(succs) = v.get_successors() {
            for head in succs {
                let id = head.id() as usize;
                in_degree[id] -= 1;
                if in_degree[id] == 0 {
                    queue.push_back(Some(head));
                }
            }
        }
    }
}
