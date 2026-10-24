//! Port of `macromedia/asc/util/graph/Visitor.java`.

use super::vertex::Vertex;

pub trait Visitor<T, E> {
    fn visit(&mut self, object: &Vertex<T, E>);
}

impl<T, E, F: FnMut(&Vertex<T, E>)> Visitor<T, E> for F {
    fn visit(&mut self, object: &Vertex<T, E>) {
        self(object)
    }
}
