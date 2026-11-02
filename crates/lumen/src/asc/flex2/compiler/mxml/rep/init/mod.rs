//! Ports of `flex2.compiler.mxml.rep.init`; `NamedInitializer` is the named `ValueInitializerKind`s, with its
//! `getAssignExpr` as `ValueInitializer::named_assign_expr`.

pub mod array_element_initializer;
pub mod dynamic_property_initializer;
pub mod effect_initializer;
pub mod event_initializer;
pub mod initializer;
pub mod static_property_initializer;
pub mod style_initializer;
pub mod value_initializer;
pub mod value_initializer_ast;
pub mod visual_child_initializer;
