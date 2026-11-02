//! Port of `flex2.compiler.mxml.rep.init.Initializer`: either a [`ValueInitializer`] (and its subclasses) or an
//! [`EventInitializer`].

use std::collections::HashSet;
use std::rc::Rc;

use super::event_initializer::EventInitializer;
use super::value_initializer::ValueInitializer;
use crate::asc::flex2::compiler::mxml::gen::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::mxml::rep::deps::Type;
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::Nf;
use crate::asc::parser::node::NodeRef;

pub type InitializerRef = Rc<Initializer>;

pub enum Initializer {
    Value(ValueInitializer),
    Event(EventInitializer),
}

impl Initializer {
    pub fn as_value(&self) -> Option<&ValueInitializer> {
        match self {
            Initializer::Value(v) => Some(v),
            Initializer::Event(_) => None,
        }
    }

    pub fn as_event(&self) -> Option<&EventInitializer> {
        match self {
            Initializer::Event(e) => Some(e),
            Initializer::Value(_) => None,
        }
    }

    pub fn get_line_ref(&self) -> i32 {
        match self {
            Initializer::Value(v) => v.get_line_ref(),
            Initializer::Event(e) => e.get_line_ref(),
        }
    }

    pub fn is_binding(&self) -> bool {
        match self {
            Initializer::Value(v) => v.is_binding(),
            Initializer::Event(_) => false,
        }
    }

    pub fn get_lvalue_type(&self) -> Type {
        match self {
            Initializer::Value(v) => v.get_lvalue_type(),
            Initializer::Event(e) => e.get_lvalue_type(),
        }
    }

    pub fn get_value_expr(&self) -> String {
        match self {
            Initializer::Value(v) => v.get_value_expr(),
            Initializer::Event(e) => e.get_value_expr(),
        }
    }

    pub fn generate_value_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool) -> NodeRef {
        match self {
            Initializer::Value(v) => v.generate_value_expr(nf, config_namespaces, generate_doc_comments),
            Initializer::Event(e) => e.generate_value_expr(nf),
        }
    }

    pub fn get_assign_expr(&self, lvalue_base: &str) -> String {
        match self {
            Initializer::Value(v) => v.get_assign_expr(lvalue_base),
            Initializer::Event(e) => e.get_assign_expr(lvalue_base),
        }
    }

    pub fn generate_assign_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>, lvalue_base: NodeRef) -> NodeRef {
        match self {
            Initializer::Value(v) => v.generate_assign_expr(nf, config_namespaces, generate_doc_comments, statement_list, lvalue_base),
            Initializer::Event(e) => e.generate_assign_expr(nf, statement_list, lvalue_base),
        }
    }

    pub fn has_definition(&self) -> bool {
        match self {
            Initializer::Value(v) => v.has_definition(),
            Initializer::Event(_) => true,
        }
    }

    pub fn is_state_specific(&self) -> bool {
        match self {
            Initializer::Value(v) => v.is_state_specific(),
            Initializer::Event(_) => false,
        }
    }

    pub fn get_definitions_iterator(&self) -> Vec<CodeFragmentList> {
        match self {
            Initializer::Value(v) => v.get_definitions_iterator(),
            Initializer::Event(e) => e.get_definitions_iterator(),
        }
    }

    pub fn generate_definitions(&self, nf: &crate::asc::util::context::ContextRef, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        match self {
            Initializer::Value(v) => v.generate_definitions(nf, config_namespaces, generate_doc_comments, statement_list),
            Initializer::Event(e) => Some(e.generate_definitions(nf, config_namespaces, generate_doc_comments, statement_list)),
        }
    }

    /// `NamedInitializer.getName()` / `EventInitializer.getName()`; empty for unnamed initializers.
    pub fn get_name(&self) -> String {
        match self {
            Initializer::Value(v) => v.get_name().unwrap_or_default(),
            Initializer::Event(e) => e.get_name(),
        }
    }

    /// `instanceof NamedInitializer`.
    pub fn is_named(&self) -> bool {
        matches!(self, Initializer::Value(v) if v.is_named())
    }

    pub fn is_design_layer(&self) -> bool {
        matches!(self, Initializer::Value(v) if v.is_design_layer())
    }
}
