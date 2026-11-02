//! Port of `flex2.compiler.as3.genext.GenerativeSecondPassEvaluator`'s shared logic (text path): hiding the original
//! members, running the support-code template, and splicing the parsed result into the class.

use std::cell::RefCell;
use std::rc::Rc;

use super::generative_class_info::{AccessorInfoRef, AccessorKind, GenerativeClassInfoRef};
use super::position_reset_evaluator::PositionResetEvaluator;
use crate::asc::flex2::compiler::as3::as3_compiler::{clean_node_factory, As3CompilerHandler};
use crate::asc::flex2::compiler::as3::binding::class_info::ClassInfoRef;
use crate::asc::flex2::compiler::as3::binding::interface_info::InterfaceInfoRef;
use crate::asc::flex2::compiler::as3::evaluator_adapter::evaluate as adapter_evaluate;
use crate::asc::flex2::compiler::as3::reflect::node_magic;
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::velocity::{self, Value, VelocityContext};
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::parser::Parser;
use crate::asc::util::context::{CompilerHandler, Context, ContextRef};
use crate::asc::util::jhash::JHashMap;

/// `CompilerContext.RENAMED_VARIABLE_MAP`.
pub const RENAMED_VARIABLE_MAP: &str = "RenamedVariableMap";
pub type RenamedVariableMap = RefCell<JHashMap<String, String>>;

pub struct GenerativeSecondPass {
    pub unit: UnitRef,
    pub generated_output_directory: Option<String>,
    pub generate_abstract_syntax_tree: bool,
    pub process_comments: bool,
}

impl GenerativeSecondPass {
    /// `checkForExistingMethods(context, node, classInfo, interfaceInfo)`.
    pub fn check_for_existing_methods(&self, context: &ContextRef, node: &NodeRef, class_info: &ClassInfoRef, interface_info: &InterfaceInfoRef) -> bool {
        let mut result = false;
        let function_names = interface_info.borrow().get_function_names().map(|f| f.to_vec());
        if let Some(function_names) = function_names {
            let pos = node.borrow().pos();
            let class_name = node_magic::get_unqualified_class_name(node);
            for q_name in function_names {
                let function_name = q_name.get_local_part().to_string();
                if function_name != "$construct" && class_info.borrow().defines_function_inherited(&function_name, true) {
                    let interface_name = interface_info.borrow().get_interface_name().to_string();
                    localized2(
                        context,
                        Level::Error,
                        None,
                        pos,
                        format!("Implementation of interface '{interface_name}' cannot be added to class '{class_name}', due to collision with existing method or property '{function_name}'."),
                    );
                    result = true;
                }
            }
        }
        result
    }

    /// `registerRenamedAccessor(info)`.
    pub fn register_renamed_accessor(&self, info: &AccessorInfoRef) {
        let existing = self.unit.borrow().context.get_attribute_as::<RenamedVariableMap>(RENAMED_VARIABLE_MAP);
        let map = match existing {
            Some(m) => m,
            None => {
                let m: Rc<RenamedVariableMap> = Rc::new(RefCell::new(JHashMap::new()));
                self.unit.borrow_mut().context.set_attribute(RENAMED_VARIABLE_MAP, m.clone());
                m
            }
        };
        let i = info.borrow();
        map.borrow_mut().insert(i.get_backing_property_name(), i.get_property_name());
    }

    /// `generateSupportCode(context, className)`: the template's output parsed as a program.
    pub fn generate_support_code(&self, context: &ContextRef, class_name: &str, template_name: &str, template_path: &str, template_vars: Vec<(&str, Value)>, suffix: &str) -> Option<NodeRef> {
        let template = match velocity::get_template(&format!("{template_path}{template_name}")) {
            Ok(t) => t,
            Err(_) => {
                toolkit::log(CompilerMessage::error(format!("Unable to load template '{template_name}'")), None, -1, -1, None);
                return None;
            }
        };
        let velocity_context = VelocityContext::new();
        for (k, v) in template_vars {
            velocity_context.put(k, v);
        }
        let text = match template.merge(&velocity_context) {
            Ok(t) => t,
            Err(e) => {
                toolkit::log_plain(Level::Error, &format!("Internal error: exception while generating from template '{template_name}' for class '${{className]': {e}"));
                return None;
            }
        };
        let source_name = self.unit.borrow().get_source().borrow().get_name().to_string();
        let sep = std::path::MAIN_SEPARATOR;
        let prefix = match source_name.rfind(sep) {
            Some(i) => &source_name[..i + 1],
            None => "",
        };
        let mut generated_name = format!("{prefix}{class_name}{suffix}");
        if let Some(dir) = &self.generated_output_directory {
            generated_name = crate::asc::flex2::compiler::mxml::mxml_compiler::add_path_components(Some(dir), &format!("_{class_name}{suffix}"), sep);
            if let Some(parent) = std::path::Path::new(&generated_name).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&generated_name, text.as_bytes());
        }
        let statics = context.borrow().statics.clone();
        let cx: ContextRef = Rc::new(RefCell::new(Context::new(statics.clone())));
        {
            let c = cx.borrow();
            c.set_path(Some(String::new()));
            c.set_script_name(Some(generated_name.clone()));
        }
        let handler: Rc<RefCell<dyn CompilerHandler>> = Rc::new(RefCell::new(As3CompilerHandler::default()));
        cx.borrow_mut().set_handler(handler);
        statics.borrow_mut().handler = cx.borrow().get_handler();
        let program_node = Parser::new_doc(&cx, &text, &generated_name, self.process_comments, false).parse_program();
        clean_node_factory(&cx);
        program_node
    }

    /// `modifySyntaxTree(context, classDef, classInfo)`.
    pub fn modify_syntax_tree(&self, context: &ContextRef, class_def: &NodeRef, class_info: &GenerativeClassInfoRef, gen_program_node: Option<NodeRef>) {
        let gen_program_node = gen_program_node.expect("NullPointerException");
        let statements = gen_program_node.borrow().program().unwrap().statements.clone();
        if statements.is_some() {
            self.apply_generated_support_code(context, class_def, class_info, &gen_program_node);
        }
    }

    fn apply_generated_support_code(&self, context: &ContextRef, class_def: &NodeRef, class_info: &GenerativeClassInfoRef, gen_program_node: &NodeRef) {
        let statements = gen_program_node.borrow().program().unwrap().statements.clone().unwrap();
        let items: Vec<NodeRef> = statements.borrow().as_statement_list().unwrap().items.iter().flatten().cloned().collect();
        for gen_item in items {
            if gen_item.borrow().as_class_definition().is_none() {
                continue;
            }
            let gen_statements = gen_item.borrow().class_definition().unwrap().statements.clone();
            let Some(gen_statements) = gen_statements else { continue };
            let stmts: Vec<NodeRef> = gen_statements.borrow().as_statement_list().unwrap().items.iter().flatten().cloned().collect();
            for gen_class_stmt in stmts {
                let k = match gen_class_stmt.borrow().kind {
                    Kind::MetaData(_) => 0,
                    Kind::FunctionDefinition(_) => 1,
                    Kind::VariableDefinition(_) => 2,
                    _ => 3,
                };
                match k {
                    0 => self.prep_meta_data_node(context, &gen_class_stmt),
                    1 => self.prep_function_definition_node(context, class_def, class_info, &gen_class_stmt),
                    2 => {
                        prep_variable_definition_node(class_def, &gen_class_stmt);
                        reset_positions(context, &gen_class_stmt, -1);
                    }
                    _ => {}
                }
                add_generated_member(class_def, &gen_class_stmt);
            }
            apply_generated_interfaces(class_info, class_def, &gen_item);
        }
    }

    /// `prepMetaDataNode(context, metaData)`.
    pub fn prep_meta_data_node(&self, context: &ContextRef, meta_data: &NodeRef) {
        if meta_data.borrow().meta_data().unwrap().data.is_some() {
            crate::asc::parser::evaluator::evaluate(meta_data, context, &mut crate::asc::parser::meta_data_evaluator::MetaDataEvaluator::new());
        }
        self.unit.borrow_mut().metadata.push(meta_data.clone());
    }

    fn prep_function_definition_node(&self, context: &ContextRef, class_def: &NodeRef, class_info: &GenerativeClassInfoRef, gen_func_def: &NodeRef) {
        let function_name = node_magic::get_function_name(gen_func_def);
        let (cx, pkgdef) = {
            let b = class_def.borrow();
            let c = b.class_definition().unwrap();
            (c.cx.clone(), c.def.pkgdef.clone())
        };
        {
            let mut b = gen_func_def.borrow_mut();
            let f = b.function_definition_mut().unwrap();
            f.cx = cx.clone();
            if let Some(fexpr) = &f.fexpr {
                fexpr.borrow_mut().as_function_common_mut().unwrap().cx = cx;
            }
            f.def.pkgdef = pkgdef;
            f.def.skip_node(true);
        }
        let has_accessors = class_info.borrow().get_accessors().is_some();
        if let (Some(function_name), true) = (function_name, has_accessors) {
            let accessor_info = class_info.borrow().get_accessor_id(&function_name);
            match accessor_info {
                Some(accessor_info) => {
                    let (is_var, position, meta_data) = {
                        let a = accessor_info.borrow();
                        match &a.kind {
                            AccessorKind::Variable { position, meta_data } => (true, *position, meta_data.clone()),
                            _ => (false, 0, Vec::new()),
                        }
                    };
                    if is_var {
                        reset_positions(context, gen_func_def, position);
                        if node_magic::function_is_getter(gen_func_def) {
                            patch_meta_data(gen_func_def, &meta_data);
                        }
                    } else if node_magic::function_is_getter(gen_func_def) {
                        let (pos, md) = {
                            let a = accessor_info.borrow();
                            match &a.kind {
                                AccessorKind::GetterSetter { getter_pos, getter_meta_data, .. } => (*getter_pos, getter_meta_data.clone().unwrap_or_default()),
                                _ => unreachable!(),
                            }
                        };
                        reset_positions(context, gen_func_def, pos);
                        patch_meta_data(gen_func_def, &md);
                    } else if node_magic::function_is_setter(gen_func_def) {
                        let (pos, md) = {
                            let a = accessor_info.borrow();
                            match &a.kind {
                                AccessorKind::GetterSetter { setter_pos, setter_meta_data, .. } => (*setter_pos, setter_meta_data.clone().unwrap_or_default()),
                                _ => unreachable!(),
                            }
                        };
                        reset_positions(context, gen_func_def, pos);
                        patch_meta_data(gen_func_def, &md);
                    }
                    let def = accessor_info.borrow().get_definition_node();
                    class_info.borrow().remove_original_meta_data(&def);
                }
                None => reset_positions(context, gen_func_def, -1),
            }
        }
    }

    /// `postProcessClassInfo(context, classInfo)`: drops getter/setter accessors that are not read-write.
    pub fn post_process_class_info(&self, context: &ContextRef, class_info: &GenerativeClassInfoRef) {
        let entries = match class_info.borrow().get_accessors() {
            Some(m) => m.entries(),
            None => return,
        };
        let reflection_info = class_info.borrow().get_class_info();
        for (q, info) in entries {
            let (is_function, prop_name, explicit, getter_pos, setter_pos) = {
                let i = info.borrow();
                (i.is_function(), i.get_property_name(), i.get_is_explicit(), i.get_getter_position(), i.get_setter_position())
            };
            if !is_function {
                continue;
            }
            let ri = reflection_info.as_ref().expect("NullPointerException");
            if !ri.borrow().defines_getter_inherited(&prop_name, true) {
                if explicit {
                    localized2(context, Level::Warning, None, setter_pos, "[Bindable] on write-only setter is unreachable and will be ignored.".to_string());
                }
                class_info.borrow_mut().get_accessors_mut().unwrap().remove(&q);
            } else if !ri.borrow().defines_setter_inherited(&prop_name, true) {
                if explicit {
                    localized2(context, Level::Warning, None, getter_pos, "[Bindable] on read-only getter is unnecessary and will be ignored.".to_string());
                }
                class_info.borrow_mut().get_accessors_mut().unwrap().remove(&q);
            }
        }
    }
}

fn add_generated_member(class_def: &NodeRef, gen_class_stmt: &NodeRef) {
    if gen_class_stmt.borrow().namespace_definition().is_some() {
        return;
    }
    let statements = class_def.borrow().class_definition().unwrap().statements.clone().unwrap();
    statements.borrow_mut().as_statement_list_mut().unwrap().items.push(Some(gen_class_stmt.clone()));
}

fn patch_meta_data(def: &NodeRef, metadata: &[NodeRef]) {
    for md in metadata {
        let skip = {
            let b = md.borrow();
            let m = b.meta_data().unwrap();
            m.get_id().as_deref() == Some(standard_defs::MD_BINDABLE) && m.get_values().is_none_or(|v| v.is_empty())
        };
        if skip {
            continue;
        }
        if md.borrow().as_doc_comment().is_some() {
            md.borrow_mut().meta_data_mut().unwrap().def = Some(def.clone());
        }
        def.borrow_mut().definition_mut().unwrap().add_meta_data_node(Some(md.clone()));
    }
}

/// `resetPositions(context, node, position)`.
pub fn reset_positions(context: &ContextRef, node: &NodeRef, position: i32) {
    adapter_evaluate(&mut PositionResetEvaluator::new(position), context, node);
}

fn prep_variable_definition_node(class_def: &NodeRef, variable_definition: &NodeRef) {
    let (cx, pkgdef) = {
        let b = class_def.borrow();
        let c = b.class_definition().unwrap();
        (c.cx.clone(), c.def.pkgdef.clone())
    };
    let mut b = variable_definition.borrow_mut();
    let v = b.as_variable_definition_mut().unwrap();
    v.cx = cx;
    v.def.pkgdef = pkgdef;
    v.def.skip_node(true);
}

fn apply_generated_interfaces(class_info: &GenerativeClassInfoRef, orig_class_def: &NodeRef, gen_class_def: &NodeRef) {
    if !class_info.borrow().needs_additional_interfaces() {
        return;
    }
    let gen_interfaces = gen_class_def.borrow().class_definition().unwrap().interfaces.clone();
    let orig_interfaces = orig_class_def.borrow().class_definition().unwrap().interfaces.clone();
    match orig_interfaces {
        None => orig_class_def.borrow_mut().class_definition_mut().unwrap().interfaces = gen_interfaces,
        Some(orig) => {
            if let Some(gen) = gen_interfaces {
                let items = gen.borrow().list().unwrap().items.clone();
                orig.borrow_mut().list_mut().unwrap().items.extend(items);
            }
        }
    }
}

/// `hideVariable(variableDefinition, info)`.
pub fn hide_variable(variable_definition: &NodeRef, info: &AccessorInfoRef) {
    let binding = node_magic::get_variable_binding(variable_definition);
    node_magic::set_variable_binding_name(&binding, &info.borrow().get_backing_property_name());
    make_attr_list_private(variable_definition);
}

/// `hideFunction(functionDefinition, info)`.
pub fn hide_function(function_definition: &NodeRef, info: &AccessorInfoRef) {
    node_magic::prefix_function_name(function_definition, &info.borrow().get_backing_prefix());
    make_attr_list_private(function_definition);
}

fn identifier_name(identifier: &Option<NodeRef>) -> Option<String> {
    identifier.as_ref().and_then(|i| i.borrow().identifier().map(|i| i.name.clone()))
}

fn make_attr_list_private(def: &NodeRef) {
    let attrs = def.borrow().definition().and_then(|d| d.attrs.clone());
    let Some(attrs) = attrs else { return };
    let items: Vec<Option<NodeRef>> = attrs.borrow().as_attribute_list().unwrap().items.clone();
    let mut kept: Vec<Option<NodeRef>> = Vec::new();
    for item in items {
        let Some(node) = item.clone() else {
            kept.push(item);
            continue;
        };
        let mut remove = false;
        if node.borrow().as_member_expression().is_some() {
            let identifier = node_magic::get_identifier(&node);
            if identifier_name(&identifier).as_deref() == Some(node_magic::OVERRIDE) {
                remove = true;
            } else {
                ensure_non_public(&identifier);
            }
        } else if node.borrow().list().is_some() {
            let list_items: Vec<NodeRef> = node.borrow().list().unwrap().items.iter().flatten().cloned().collect();
            for list_node in list_items {
                if list_node.borrow().as_member_expression().is_some() {
                    let identifier = node_magic::get_identifier(&list_node);
                    if identifier_name(&identifier).as_deref() == Some(node_magic::OVERRIDE) {
                        remove = true;
                    } else {
                        ensure_non_public(&identifier);
                    }
                }
            }
        }
        if !remove {
            kept.push(item);
        }
    }
    attrs.borrow_mut().as_attribute_list_mut().unwrap().items = kept;
}

/// `ensureNonPublic(identifier)`.
pub fn ensure_non_public(identifier: &Option<NodeRef>) {
    if identifier_name(identifier).as_deref() == Some(node_magic::PUBLIC) {
        identifier.as_ref().unwrap().borrow_mut().identifier_mut().unwrap().name = node_magic::PRIVATE.to_string();
    }
}
