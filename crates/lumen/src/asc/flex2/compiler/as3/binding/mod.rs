//! Ports of `flex2.compiler.as3.binding`: `[Bindable]` codegen, data binding watchers and the `TypeAnalyzer`.

pub mod bindable_extension;
pub mod bindable_first_pass_evaluator;
pub mod bindable_info;
pub mod bindable_second_pass_evaluator;
pub mod class_info;
pub mod data_binding_extension;
pub mod data_binding_extension_ast;
pub mod data_binding_first_pass_evaluator;
pub mod data_binding_info;
pub mod info;
pub mod interface_info;
pub mod prefixed_pretty_printer;
pub mod type_analyzer;
pub mod watcher;
