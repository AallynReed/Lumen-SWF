//! Port of `macromedia.abc.OpcodeVisitor`: every `Visitor` method does nothing.

use super::visitor::Visitor;

#[derive(Debug, Default, Clone, Copy)]
pub struct OpcodeVisitor;

impl Visitor for OpcodeVisitor {}
