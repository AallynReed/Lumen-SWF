//! Port of the JavaCC-generated `flex2.compiler.mxml.Token`, with the token stream's element type: a start tag or
//! character data is the `Node` itself, an end tag or EOF is a plain token.

use std::cell::RefCell;
use std::rc::Rc;

use super::dom::node::NodeRef;

#[derive(Clone, Debug, Default)]
pub struct Token {
    pub kind: i32,
    pub begin_line: i32,
    pub begin_column: i32,
    pub end_line: i32,
    pub end_column: i32,
    pub image: String,
}

impl Token {
    pub fn new(kind: i32, image: &str) -> Token {
        Token { kind, image: image.to_string(), ..Default::default() }
    }
}

/// A `Token` reference as the parser and visitors see it.
#[derive(Clone)]
pub enum Tok {
    Node(NodeRef),
    Plain(Rc<RefCell<Token>>),
}

impl Tok {
    pub fn plain(t: Token) -> Tok {
        Tok::Plain(Rc::new(RefCell::new(t)))
    }

    pub fn kind(&self) -> i32 {
        match self {
            Tok::Node(n) => n.borrow().kind,
            Tok::Plain(t) => t.borrow().kind,
        }
    }

    pub fn image(&self) -> String {
        match self {
            Tok::Node(n) => n.borrow().image.clone(),
            Tok::Plain(t) => t.borrow().image.clone(),
        }
    }

    pub fn begin_line(&self) -> i32 {
        match self {
            Tok::Node(n) => n.borrow().begin_line,
            Tok::Plain(t) => t.borrow().begin_line,
        }
    }

    pub fn begin_column(&self) -> i32 {
        match self {
            Tok::Node(n) => n.borrow().begin_column,
            Tok::Plain(t) => t.borrow().begin_column,
        }
    }

    /// `(Node) token`.
    pub fn as_node(&self) -> NodeRef {
        match self {
            Tok::Node(n) => n.clone(),
            Tok::Plain(_) => panic!("ClassCastException: flex2.compiler.mxml.Token cannot be cast to flex2.compiler.mxml.dom.Node"),
        }
    }

    pub fn ptr_eq(&self, other: &Tok) -> bool {
        match (self, other) {
            (Tok::Node(a), Tok::Node(b)) => Rc::ptr_eq(a, b),
            (Tok::Plain(a), Tok::Plain(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
