//! Port of `flex2.compiler.mxml.rep.init.VisualChildInitializer`.

use super::initializer::InitializerRef;
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::mxml::rep::deps;
use crate::asc::flex2::compiler::mxml::rep::model::{ModelRef, RValue};

impl ValueInitializer {
    /// `new VisualChildInitializer(movieClip)`.
    pub fn new_visual_child(movie_clip: &ModelRef) -> InitializerRef {
        ValueInitializer::create(RValue::Model(movie_clip.clone()), movie_clip.get_xml_line_number(), movie_clip.get_standard_defs(), ValueInitializerKind::VisualChild)
    }

    pub(crate) fn visual_child_assign_expr(&self, lvalue_base: &str) -> String {
        if self.standard_defs.is_repeater(&self.get_lvalue_type()) {
            let cr = format!("{lvalue_base}.{}::{}", deps::NAMESPACE_MX_INTERNAL_LOCALNAME, deps::PROP_CONTAINER_CHILDREPEATERS);
            let mut sb = String::new();
            sb.push_str(&format!("var repeater:{} = {};\n", self.standard_defs.class_repeater_dot, self.get_value_expr()));
            sb.push_str(&format!("\trepeater.initializeRepeater({lvalue_base}, true);\n"));
            sb.push_str(&format!("\t({cr} ? {cr} : ({cr}=[])).push(repeater)"));
            sb
        } else {
            format!("{lvalue_base}.addChild({})", self.get_value_expr())
        }
    }
}
