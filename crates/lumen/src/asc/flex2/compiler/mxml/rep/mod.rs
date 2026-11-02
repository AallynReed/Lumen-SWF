//! Ports of `flex2.compiler.mxml.rep` and its subpackages.

pub mod anonymous_object_graph;
pub mod array;
pub mod at_clear;
pub mod at_embed;
pub mod at_resource;
pub mod binding_expression;
pub mod decl;
pub mod design_layer;
pub mod document_info;
pub mod event_handler;
pub mod init;
pub mod method;
pub mod model;
pub mod movie_clip;
pub mod mxml_document;
pub mod operation;
pub mod primitive;
pub mod reparent;
pub mod script;
pub mod states_model;
pub mod states_model_ast;
pub mod variable_declaration;
pub mod vector;
pub mod xml;
pub mod xml_list;

#[cfg(test)]
mod tests;

/// The names rep uses from the other MXML packages, in one place.
pub mod deps {
    pub use crate::asc::flex2::compiler::mxml::reflect::type_table::{Stateful, TypeTable};
    pub use crate::asc::flex2::compiler::mxml::reflect::{EffectRef as Effect, EventRef as Event, PropertyRef as Property, StyleRef as Style, TypeRef as Type};

    pub use self::stub::*;

    pub const PUBLIC_NAMESPACE: &str = "";

    /// `type == other` (TypeTable keeps one Type per name, and Type equality is by name).
    pub fn type_eq(a: &Type, b: &Type) -> bool {
        **a == **b
    }

    /// Adapters to the other MXML packages.
    mod stub {
        use std::collections::HashSet;
        use std::rc::Rc;

        use crate::asc::flex2::compiler::css::document::CssConfiguration;
        use crate::asc::flex2::compiler::util::name_mappings::NameMappings;

        pub use crate::asc::flex2::compiler::mxml::mxml_configuration::MxmlConfiguration;
        pub type Node = crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
        pub type DocumentNode = Node;
        pub type DesignLayerNode = Node;

        pub use crate::asc::flex2::compiler::mxml::lang::standard_defs::{
            is_built_in_type_name, is_stage_property, StandardDefs, CLASS_VECTOR, CLASS_VECTOR_SHORTNAME, MD_IMXMLOBJECT, NAMESPACE_MX_INTERNAL_LOCALNAME, NULL, PROP_CONTAINER_CHILDREPEATERS,
            PROP_CURRENTSTATE, UNDEFINED,
        };
        pub use crate::asc::flex2::compiler::mxml::lang::text_parser::is_valid_identifier;
        pub use crate::asc::flex2::compiler::mxml::lang::framework_defs::binding_management_vars;

        pub fn required_top_level_descriptor_properties() -> HashSet<String> {
            crate::asc::flex2::compiler::mxml::lang::framework_defs::REQUIRED_TOP_LEVEL_DESCRIPTOR_PROPERTIES.iter().map(|s| s.to_string()).collect()
        }

        pub use crate::asc::flex2::compiler::mxml::builder::abstract_builder::check_deprecation;

        pub fn css_configuration(c: &MxmlConfiguration) -> Rc<CssConfiguration> {
            c.css.clone()
        }
        pub fn show_deprecation_warnings(c: &MxmlConfiguration) -> bool {
            c.show_deprecation_warnings
        }
        pub fn allow_duplicate_default_style_declarations(c: &MxmlConfiguration) -> bool {
            c.allow_duplicate_default_style_declarations
        }


        pub fn same_node(a: &Node, b: &Node) -> bool {
            Rc::ptr_eq(a, b)
        }
        pub fn node_attribute_value(n: &Node, uri: &str, local: &str) -> Option<String> {
            n.borrow().get_attribute_value_uri(uri, local).and_then(|v| v.as_str()).map(str::to_string)
        }
        pub fn node_local_part(n: &Node) -> String {
            n.borrow().get_local_part().to_string()
        }
        pub fn document_node_outer_document_class_name(n: &DocumentNode) -> Option<String> {
            n.borrow().get_outer_document_class_name().map(str::to_string)
        }
        pub fn document_node_version(n: &DocumentNode) -> i32 {
            n.borrow().get_version()
        }
        pub fn document_node_language_namespace(n: &DocumentNode) -> Option<String> {
            n.borrow().get_language_namespace().map(str::to_string)
        }
        pub fn document_node_is_inline_component(n: &DocumentNode) -> bool {
            n.borrow().is_inline_component()
        }
        pub fn document_node_add_local_class(n: &DocumentNode, ns: &str, local: &str, class: &str) {
            n.borrow_mut().add_local_class(ns, local, class)
        }
        pub fn document_node_get_local_class(n: &DocumentNode, ns: &str, local: &str) -> Option<String> {
            n.borrow().get_local_class(ns, local)
        }
        pub fn document_node_get_local_class_mappings(n: &DocumentNode) -> Option<Rc<NameMappings>> {
            n.borrow().get_local_class_mappings()
        }
        pub fn document_node_layer_declaration_nodes(n: &DocumentNode) -> Vec<DesignLayerNode> {
            n.borrow().layer_declaration_nodes().to_vec()
        }
    }

}
