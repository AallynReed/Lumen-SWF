//! Port of `flex2.compiler.mxml.dom`: the MXML scanner, its nodes and analyzer visitors.

pub mod analyzer;
pub mod analyzer_adapter;
pub mod mxml_scanner;
pub mod node;
pub mod scanner_error;
pub mod syntax_tree_builder;
pub mod xerces;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod dom_tests;
