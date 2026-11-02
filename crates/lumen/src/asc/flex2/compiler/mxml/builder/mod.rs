//! Ports of `flex2.compiler.mxml.builder` (text path).

pub mod abstract_builder;
pub mod anonymous_object_graph_builder;
pub mod array_builder;
pub mod component_builder;
pub mod document_builder;
pub mod http_service_builder;
pub mod inline_component_builder;
pub mod model_builder;
pub mod prefix_mapping;
pub mod primitive_builder;
pub mod remote_object_builder;
pub mod service_request_builder;
pub mod vector_builder;
pub mod web_service_builder;
pub mod xml_builder;
pub mod xml_list_builder;
pub mod xml_string_serializer;

#[cfg(test)]
mod tests;
