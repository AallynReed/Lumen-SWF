//! Port of `flex2.compiler.as3.genext.PositionResetEvaluator`: `checkFeature` sets every visited node's position.

use crate::asc::flex2::compiler::as3::evaluator_adapter::Adapter;
use crate::asc::parser::node::NodeRef;
use crate::asc::util::context::ContextRef;

pub struct PositionResetEvaluator {
    position: i32,
}

impl PositionResetEvaluator {
    pub fn new(position: i32) -> PositionResetEvaluator {
        PositionResetEvaluator { position }
    }
}

impl Adapter for PositionResetEvaluator {
    fn visit(&mut self, _cx: &ContextRef, node: &NodeRef) -> bool {
        node.borrow_mut().set_pos(self.position);
        false
    }
}
