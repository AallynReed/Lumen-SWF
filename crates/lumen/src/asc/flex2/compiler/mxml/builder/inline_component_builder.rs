//! Port of `flex2.compiler.mxml.builder.InlineComponentBuilder`.

use super::abstract_builder::{analyzer_basics, attr_str, factory_from_class, AbstractBuilder, Builder, BuilderContext};
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::rep::model::ModelRef;
use crate::asc::flex2::compiler::util::name_formatter;

pub struct InlineComponentBuilder {
    pub ab: AbstractBuilder,
    top_level: bool,
    rvalue: Option<ModelRef>,
}

impl InlineComponentBuilder {
    pub fn new(ctx: &BuilderContext, top_level: bool) -> InlineComponentBuilder {
        InlineComponentBuilder { ab: AbstractBuilder::new(ctx), top_level, rvalue: None }
    }

    pub fn get_rvalue(&self) -> Option<ModelRef> {
        self.rvalue.clone()
    }
}

impl Builder for InlineComponentBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }
}

impl Analyzer for InlineComponentBuilder {
    analyzer_basics!();

    fn analyze_inline_component_node(&mut self, node: &NodeRef) {
        let class_qname = node.borrow().get_class_qname().expect("NullPointerException");
        let rvalue = factory_from_class(&self.ab.ctx, &self.ab.standard_defs, &name_formatter::to_dot(&class_qname.to_string()), node.borrow().begin_line);
        self.rvalue = Some(rvalue.clone());
        let id = attr_str(self.get_language_attribute_value(node, sd::PROP_ID));
        if id.is_some() || self.top_level {
            if let Some(comment) = node.borrow().comment.clone() {
                *rvalue.comment.borrow_mut() = Some(comment);
            }
            self.ab.register_model_id(id.as_deref(), &rvalue, self.top_level);
        }
    }
}
