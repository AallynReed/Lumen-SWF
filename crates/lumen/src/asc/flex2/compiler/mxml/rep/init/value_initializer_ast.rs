//! The direct AST generation half of `flex2.compiler.mxml.rep.init.ValueInitializer` (and its subclasses'
//! `generateAssignExpr`), plus `StringUtils.unformatString`.

use std::collections::HashSet;

use indexmap::IndexSet;

use super::initializer::InitializerRef;
use super::value_initializer::{ValueInitializer, ValueInitializerKind};
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::flex2::compiler::mxml::implementation_generator;
use crate::asc::flex2::compiler::mxml::rep::deps::{self, type_eq, Type};
use crate::asc::flex2::compiler::mxml::rep::model::{ModelKind, ModelRef, RValue};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::{ASSIGN_TOKEN, EMPTY_TOKEN, LEFTBRACKET_TOKEN, LESSTHAN_TOKEN, NOT_TOKEN, PLUSPLUS_TOKEN};
use crate::asc::util::context::ContextRef;
use crate::asc::util::jstr;

const TEMP: &str = "temp";
const INT: &str = "int";
const ARRAY: &str = "Array";

/// `StringUtils.unformatString(s)`: resolves `\uXXXX` escapes and keeps other backslash escapes as written.
pub fn unformat_string(s: &str) -> String {
    let units: Vec<u16> = jstr::to_utf16(s);
    let mut result: Vec<u16> = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let mut c = units[i];
        i += 1;
        if c == b'\\' as u16 && i < units.len() {
            c = units[i];
            i += 1;
            if c == b'u' as u16 && i + 3 < units.len() {
                let mut value: u32 = 0;
                for _ in 0..4 {
                    let d = char::from_u32(units[i] as u32).and_then(|ch| ch.to_digit(16)).expect("Malformed \\uxxxx encoding.");
                    i += 1;
                    value = (value << 4) + d;
                }
                result.push(value as u16);
            } else if c == b'u' as u16 {
                result.push(c);
            } else {
                result.push(b'\\' as u16);
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }
    jstr::from_utf16(&result)
}

/// `Boolean.parseBoolean(s)`.
fn parse_boolean(s: &str) -> bool {
    s.eq_ignore_ascii_case("true")
}

impl ValueInitializer {
    /// `generateValueExpr(nodeFactory, configNamespaces, generateDocComments)`.
    pub fn generate_value_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool) -> NodeRef {
        let lvalue_type = self.get_lvalue_type();
        if self.standard_defs.is_ideferred_instance(&lvalue_type) {
            let (type_identifier, args) = if self.rvalue_is_class_ref() {
                let type_identifier = ast::generate_qualified_identifier(&mut nf.f(), self.standard_defs.get_core_package(), "DeferredInstanceFromClass", false);
                let inline_rvalue = self.generate_inline_rvalue(nf, config_namespaces, generate_doc_comments);
                let args = nf.f().argument_list(None, Some(inline_rvalue));
                (type_identifier, args)
            } else {
                let type_identifier = ast::generate_qualified_identifier(&mut nf.f(), self.standard_defs.get_core_package(), "DeferredInstanceFromFunction", false);
                let identifier = ast::generate_identifier(&mut nf.f(), &self.get_definition_name(), true);
                let mut args = nf.f().argument_list(None, Some(identifier));
                if self.standard_defs.is_itransient_deferred_instance(&lvalue_type) {
                    let name = format!("{}_r", self.get_model().expect("ClassCastException").get_definition_name().unwrap_or_default());
                    let identifier = ast::generate_identifier(&mut nf.f(), &name, true);
                    args = nf.f().argument_list(Some(args), Some(identifier));
                }
                (type_identifier, args)
            };
            let call_expression = ast::call(nf, type_identifier, Some(args), true);
            nf.f().member_expression(None, Some(call_expression))
        } else if self.has_definition() {
            let identifier = ast::generate_identifier(&mut nf.f(), &self.get_definition_name(), true);
            let call_expression = ast::call(nf, identifier, None, false);
            nf.f().member_expression(None, Some(call_expression))
        } else {
            self.generate_inline_rvalue(nf, config_namespaces, generate_doc_comments)
        }
    }

    fn generate_inline_rvalue(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool) -> NodeRef {
        let value = self.get_value();
        match &value {
            RValue::Model(model) => match &model.kind {
                ModelKind::Primitive(p) => self.generate_format_expr(nf, config_namespaces, generate_doc_comments, model.get_type(), &p.get_value().expect("assert value != null")),
                ModelKind::Array(a) if a.vector.is_some() => as_vector_literal(nf, config_namespaces, generate_doc_comments, model),
                ModelKind::Array(_) => as_array_literal(nf, config_namespaces, generate_doc_comments, model),
                ModelKind::Xml(_) => self.as_xml_literal_ast(nf, model),
                ModelKind::XmlList(_) => as_xml_list(nf, model),
                _ if type_eq(model.get_type(), &self.get_type_table().object_type()) => as_object_literal(nf, config_namespaces, generate_doc_comments, model),
                _ => panic!("can't generate inline expr for values of type {}", model.kind_class_simple_name()),
            },
            _ => self.generate_format_expr(nf, config_namespaces, generate_doc_comments, &self.get_lvalue_type(), &value),
        }
    }

    /// `generateDefinitionBody(context, configNamespaces, generateDocComments, statementList)`.
    pub fn generate_definition_body(&self, context: &ContextRef, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf_owned = Nf::of(context);
        let nf = &nf_owned;
        let mut result = statement_list;
        let var_name = TEMP;
        let self_model = self.get_model().expect("non-Model value has definition in getDefinitionBody()");
        let self_type = self_model.get_type().clone();
        let self_type_name = match &self_model.kind {
            ModelKind::Array(a) if a.vector.is_some() => format!("{}.<{}>", deps::CLASS_VECTOR, a.get_element_type_name()),
            _ => name_formatter::to_dot(&self_type.get_name().to_string()),
        };
        let is_declared = self_model.is_declared();
        let id = if is_declared { self_model.get_id().unwrap_or_default() } else { var_name.to_string() };
        let sd = self.standard_defs.clone();

        let return_type = ast::generate_type_expression(&mut nf.f(), &self_type_name, true);
        let function_signature = nf.f().function_signature(None, return_type);
        let position = ast::line_number_to_position(&mut nf.f(), self.get_line_ref());

        let variable_definition = if self.model_has_inline_rvalue() {
            let inline_rvalue = self.generate_inline_rvalue(nf, config_namespaces, generate_doc_comments);
            ast::generate_variable_pos(&mut nf.f(), var_name, &self_type_name, true, Some(inline_rvalue), position)
        } else {
            match &self_model.kind {
                ModelKind::Array(a) if a.vector.is_some() => {
                    let literal_number = nf.f().literal_number_int(a.size() as i32);
                    let mut argument_list = nf.f().argument_list(None, Some(literal_number));
                    if a.is_fixed() {
                        let literal_boolean = nf.f().literal_boolean(a.is_fixed());
                        argument_list = nf.f().argument_list(Some(argument_list), Some(literal_boolean));
                    }
                    ast::generate_variable_new_args(&mut nf.f(), var_name, &self_type_name, Some(argument_list), position)
                }
                _ => ast::generate_variable_new(&mut nf.f(), var_name, &self_type_name, position),
            }
        };

        let mut function_statement_list = nf.f().statement_list(None, Some(variable_definition));

        if !self.model_has_inline_rvalue() {
            match &self_model.kind {
                ModelKind::Array(a) if a.vector.is_some() => add_assign_exprs(nf, config_namespaces, generate_doc_comments, &function_statement_list, &a.get_element_initializer_iterator(), var_name),
                _ => add_assign_exprs(nf, config_namespaces, generate_doc_comments, &function_statement_list, &self_model.get_property_initializer_iterator_bindings(self_model.get_type().has_dynamic()), var_name),
            }
        }
        add_assign_exprs(nf, config_namespaces, generate_doc_comments, &function_statement_list, &self_model.get_style_initializer_iterator(), var_name);
        add_assign_exprs(nf, config_namespaces, generate_doc_comments, &function_statement_list, &self_model.get_effect_initializer_iterator(), var_name);
        add_assign_exprs(nf, config_namespaces, generate_doc_comments, &function_statement_list, &self_model.get_event_initializer_iterator(), var_name);

        let effects = self_model.get_effect_initializer_iterator();
        if !effects.is_empty() {
            let base = ast::generate_getter_selector(&mut nf.f(), var_name, false);
            let identifier = nf.f().identifier_intern("registerEffects", false);
            let mut effect_event_names_argument_list: Option<NodeRef> = None;
            for effect_initializer in &effects {
                let literal_string = nf.f().literal_string(&effect_initializer.get_name());
                effect_event_names_argument_list = Some(nf.f().argument_list(effect_event_names_argument_list, Some(literal_string)));
            }
            let literal_array = nf.f().literal_array(effect_event_names_argument_list);
            let argument_list = nf.f().argument_list(None, Some(literal_array));
            let selector = ast::call(nf, identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
        }

        if is_declared && sd.is_iui_component_with_id_property(&self_type) {
            let base = ast::generate_getter_selector(&mut nf.f(), var_name, false);
            let identifier = nf.f().identifier_intern("id", false);
            let literal_string = nf.f().literal_string(&id);
            let argument_list = nf.f().argument_list(None, Some(literal_string));
            let selector = nf.f().set_expression(Some(identifier), Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
        }

        let layer_parent = self_model.layer_parent.borrow().clone();
        if let Some(layer_parent) = layer_parent {
            let layer_id = layer_parent.get_id().unwrap_or_default();
            if self_model.is_design_layer() {
                let method = nf.f().identifier_intern("addLayer", false);
                let layer_identifier = nf.f().identifier(&layer_id);
                let get_expression = nf.f().get_expression(Some(layer_identifier));
                let layer_member_expression = nf.f().member_expression(None, Some(get_expression));
                let var_identifier = nf.f().identifier(var_name);
                let var_get_expression = nf.f().get_expression(Some(var_identifier));
                let var_member_expression = nf.f().member_expression(None, Some(var_get_expression));
                let argument_list = nf.f().argument_list(None, Some(var_member_expression));
                let selector = ast::call(nf, method, Some(argument_list), false);
                let expression_statement = ast::expr_stmt(nf, Some(layer_member_expression), selector);
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
            } else if self_model.get_type().is_assignable_to_name(Some(&sd.interface_ivisualelement)) {
                let var_identifier = nf.f().identifier(var_name);
                let var_get_expression = nf.f().get_expression(Some(var_identifier));
                let base = nf.f().member_expression(None, Some(var_get_expression));
                let identifier = nf.f().identifier_intern("designLayer", false);
                let layer_identifier = nf.f().identifier(&layer_id);
                let get_expression = nf.f().get_expression(Some(layer_identifier));
                let rvalue_member_expression = nf.f().member_expression(None, Some(get_expression));
                let argument_list = nf.f().argument_list(None, Some(rvalue_member_expression));
                let selector = nf.f().set_expression(Some(identifier), Some(argument_list), false);
                let expression_statement = ast::expr_stmt(nf, Some(base), selector);
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
            }
        }

        if sd.is_iui_component(&self_type) {
            let ModelKind::MovieClip(movie_clip) = &self_model.kind else { panic!("isIUIComponent(selfType) but !(self instanceof MovieClip)") };
            let test = {
                let base = ast::generate_getter_selector(&mut nf.f(), var_name, false);
                let identifier = nf.f().identifier_intern("document", false);
                let selector = nf.f().get_expression(Some(identifier));
                let member_expression = nf.f().member_expression(Some(base), Some(selector));
                let unary_expression = nf.f().unary_expression(NOT_TOKEN, Some(member_expression));
                nf.f().list(None, Some(unary_expression))
            };
            let then = {
                let base = ast::generate_getter_selector(&mut nf.f(), var_name, false);
                let identifier = nf.f().identifier_intern("document", false);
                let this_expression = nf.f().this_expression(-1);
                let argument_list = nf.f().argument_list(None, Some(this_expression));
                let selector = nf.f().set_expression(Some(identifier), Some(argument_list), false);
                let expression_statement = ast::expr_stmt(nf, Some(base), selector);
                nf.f().statement_list(None, Some(expression_statement))
            };
            let if_statement = nf.f().if_statement(Some(test), Some(then), None);
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(if_statement));

            if !sd.is_repeater(&self_type) {
                if sd.is_container(&self_type) {
                    if movie_clip.has_children() {
                        function_statement_list = generate_document_descriptor_assignment(nf, config_namespaces, generate_doc_comments, &self_model, var_name, function_statement_list);
                        function_statement_list = generate_document_descriptor_document_assignment(nf, var_name, function_statement_list);
                    }
                } else {
                    for init in movie_clip.get_child_initializer_iterator() {
                        let child = init.as_value().and_then(|v| v.get_model()).expect("ClassCastException");
                        if !child.is_state_specific() {
                            let member_expression = ast::generate_getter_selector(&mut nf.f(), var_name, true);
                            function_statement_list = init.generate_assign_expr(nf, config_namespaces, generate_doc_comments, Some(function_statement_list), member_expression);
                        }
                    }
                }
            } else {
                let child_descriptor_variable_definition = self.generate_child_descriptor_variable(nf, config_namespaces, generate_doc_comments, var_name, &self_model);
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(child_descriptor_variable_definition));
                let for_statement = generate_repeater_child_descriptor_loop(nf);
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(for_statement));
            }
        }

        if self_model.get_type().is_assignable_to_name(Some(&sd.interface_imxmlobject)) || self_model.get_type().has_metadata(deps::MD_IMXMLOBJECT, true) {
            let var_name_member_expression = ast::generate_getter_selector(&mut nf.f(), var_name, false);
            let identifier = nf.f().identifier_intern("initialized", false);
            let this_expression = nf.f().this_expression(-1);
            let argument_list = nf.f().argument_list(None, Some(this_expression));
            let second = if is_declared { nf.f().literal_string(&id) } else { nf.f().literal_null_pos(-1) };
            let argument_list = nf.f().argument_list(Some(argument_list), Some(second));
            let call_expression = ast::call(nf, identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(var_name_member_expression), call_expression);
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
        }

        if is_declared {
            if self_model.get_repeater_level() == 0 {
                let expression_statement = ast::generate_assignment(&mut nf.f(), &id, var_name);
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
            } else {
                toolkit::log(
                    CompilerMessage::error("Unable to generate initialization code within Repeater, due to id or data binding on a component that is not a visual child."),
                    Some(&self_model.get_document().get_source_path()),
                    self.get_line_ref(),
                    -1,
                    None,
                );
            }
            let base = ast::generate_getter_selector_qualified(&mut nf.f(), sd.get_binding_package(), "BindingManager", false);
            let identifier = nf.f().identifier_intern("executeBindings", false);
            let this_expression = nf.f().this_expression(-1);
            let argument_list = nf.f().argument_list(None, Some(this_expression));
            let literal_string = nf.f().literal_string(&id);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(literal_string));
            let id_member_expression = ast::generate_getter_selector(&mut nf.f(), &id, true);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(id_member_expression));
            let selector = ast::call(nf, identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
        }

        if sd.is_container(&self_type) && self_model.is_early_init() {
            let var_name_member_expression = ast::generate_getter_selector(&mut nf.f(), var_name, false);
            let identifier = nf.f().identifier_intern("initialize", false);
            let call_expression = ast::call(nf, identifier, None, false);
            let expression_statement = ast::expr_stmt(nf, Some(var_name_member_expression), call_expression);
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
        }

        let member_expression = ast::generate_getter_selector(&mut nf.f(), var_name, false);
        let list = nf.f().list(None, Some(member_expression));
        let return_statement = nf.f().return_statement(Some(list));
        function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(return_statement));

        let function_identifier = nf.f().identifier(&self.get_definition_name());
        let function_common = nf.f().function_common(context, Some(function_identifier.clone()), Some(function_signature), Some(function_statement_list));
        ast::user_defined_body(&function_common);
        let attribute_list = ast::generate_private_attribute(&mut nf.f());
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(function_identifier));
        let function_definition = nf.f().function_definition(context, Some(attribute_list), Some(function_name), Some(function_common));
        result = Some(nf.f().statement_list(result, Some(function_definition)));

        let lvalue_type = self.get_lvalue_type();
        if sd.is_itransient_deferred_instance(&lvalue_type) || self_model.get_is_transient() {
            result = self.generate_destructor_body(context, result);
        }
        result
    }

    /// `generateDestructorBody(context, statementList)`.
    fn generate_destructor_body(&self, context: &ContextRef, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf_owned = Nf::of(context);
        let nf = &nf_owned;
        let model = self.get_model().expect("ClassCastException");
        let mut ids = IndexSet::new();
        if model.is_declared() {
            ids.insert(model.get_id().unwrap_or_default());
        }
        Self::collect_declarations(&model.get_sub_initializer_iterator(), &mut ids);
        let mut function_statement_list: Option<NodeRef> = None;
        for id in &ids {
            let literal_null = nf.f().literal_null();
            let identifier = nf.f().identifier(id);
            let expression_statement = nf.f().assignment_expression(Some(identifier), ASSIGN_TOKEN, Some(literal_null));
            function_statement_list = Some(nf.f().statement_list(function_statement_list, Some(expression_statement)));
        }
        let function_signature = nf.f().function_signature(None, None);
        ast::void_anno(&function_signature);
        let function_identifier = nf.f().identifier(&format!("{}_r", model.get_definition_name().unwrap_or_default()));
        let function_common = nf.f().function_common(context, Some(function_identifier.clone()), Some(function_signature), function_statement_list);
        ast::user_defined_body(&function_common);
        let attribute_list = ast::generate_private_attribute(&mut nf.f());
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(function_identifier));
        let function_definition = nf.f().function_definition(context, Some(attribute_list), Some(function_name), Some(function_common));
        let result = nf.f().statement_list(statement_list, Some(function_definition));
        Some(result)
    }

    /// `generateChildDescriptorVariable(...)`; like Java, each child's entry describes `movie_clip` itself.
    pub fn generate_child_descriptor_variable(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, var_name: &str, movie_clip: &ModelRef) -> NodeRef {
        let base = ast::generate_getter_selector(&mut nf.f(), var_name, true);
        let child_descriptors_identifier = nf.f().identifier_intern("childDescriptors", false);
        let mut visual_child_argument_list: Option<NodeRef> = None;
        let ModelKind::MovieClip(mc) = &movie_clip.kind else { panic!("ClassCastException") };
        for _init in mc.children() {
            let member_expression = implementation_generator::add_descriptor_initializer_fragments(nf, config_namespaces, generate_doc_comments, movie_clip, None, true);
            visual_child_argument_list = Some(nf.f().argument_list(visual_child_argument_list, Some(member_expression)));
        }
        let literal_array = nf.f().literal_array(visual_child_argument_list);
        let argument_list = nf.f().argument_list(None, Some(literal_array));
        let selector = nf.f().set_expression(Some(child_descriptors_identifier), Some(argument_list), false);
        ast::no_rvalue(&selector);
        let initializer = nf.f().member_expression(Some(base), Some(selector));
        ast::generate_variable(&mut nf.f(), "cd", ARRAY, false, Some(initializer))
    }

    /// `generateDefinitions(context, configNamespaces, generateDocComments, statementList)`.
    pub fn generate_definitions(&self, nf: &ContextRef, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list;
        if self.has_definition() {
            let described = self.get_model().is_some_and(|m| m.is_described());
            if !described {
                result = self.generate_definition_body(nf, config_namespaces, generate_doc_comments, result);
            }
        }
        if let Some(model) = self.get_model() {
            for initializer in model.get_sub_initializer_iterator() {
                result = initializer.generate_definitions(nf, config_namespaces, generate_doc_comments, result);
            }
        }
        result
    }

    /// `formatExpr(nodeFactory, configNamespaces, generateDocComments, targetType, value)`.
    pub(crate) fn generate_format_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, target_type: &Type, value: &RValue) -> NodeRef {
        let type_table = self.get_type_table();
        let tt = &*type_table;
        let is_scalar = |t: &Type| type_eq(t, &tt.boolean_type()) || type_eq(t, &tt.number_type()) || type_eq(t, &tt.int_type()) || type_eq(t, &tt.uint_type());
        match value {
            RValue::Binding(_) => {
                return if is_scalar(target_type) { nf.f().identifier_intern(deps::UNDEFINED, false) } else { nf.f().literal_null_pos(-1) };
            }
            RValue::AtEmbed(e) => return ast::generate_getter_selector(&mut nf.f(), e.get_prop_name(), true),
            RValue::AtResource(r) => return r.generate_value_expression(nf),
            _ => {}
        }
        if type_eq(target_type, &tt.string_type()) {
            return nf.f().literal_string(&unformat_string(&value.to_java_string()));
        }
        if type_eq(target_type, &tt.boolean_type()) {
            nf.f().literal_boolean(parse_boolean(&value.to_java_string()))
        } else if type_eq(target_type, &tt.number_type()) || type_eq(target_type, &tt.int_type()) || type_eq(target_type, &tt.uint_type()) {
            nf.f().literal_number(&value.to_java_string())
        } else if type_eq(target_type, &tt.object_type()) || type_eq(target_type, &tt.no_type()) {
            match value {
                RValue::Str(s) => nf.f().literal_string(&unformat_string(s)),
                RValue::Integer(_) | RValue::Long(_) | RValue::Double(_) => nf.f().literal_number(&value.to_java_string()),
                RValue::Boolean(_) => nf.f().literal_boolean(parse_boolean(&value.to_java_string())),
                _ => panic!("formatExpr: unsupported rvalue type for lvalue type 'Object'"),
            }
        } else if type_eq(target_type, &tt.class_type()) {
            let v = value.to_java_string();
            if v == "this" {
                nf.f().this_expression(-1)
            } else if v == deps::NULL {
                nf.f().literal_null_pos(-1)
            } else {
                let position = ast::line_number_to_position(&mut nf.f(), self.get_line_ref());
                ast::generate_member_expression_pos(&mut nf.f(), &v, position)
            }
        } else if type_eq(target_type, &tt.function_type()) {
            let context = nf.f().get_context();
            let list = ast::parse(&context, config_namespaces, &format!("var f = {}", value.to_java_string()), self.get_line_ref(), generate_doc_comments);
            let first = list.first().cloned().expect("IndexOutOfBoundsException");
            let doc_def = first.borrow().as_doc_comment().map(|d| d.meta.def.clone());
            let variable_definition = match doc_def {
                Some(def) => def.expect("NullPointerException"),
                None => first.clone(),
            };
            let variable_binding = {
                let b = variable_definition.borrow();
                let list = b.as_variable_definition().expect("ClassCastException").list.clone().expect("NullPointerException");
                let l = list.borrow();
                l.list().unwrap().items[0].clone().expect("NullPointerException")
            };
            let initializer = variable_binding.borrow().as_variable_binding().expect("ClassCastException").initializer.clone();
            initializer.expect("NullPointerException")
        } else if type_eq(target_type, &tt.reg_exp_type()) {
            nf.f().literal_reg_exp(&value.to_java_string(), -1)
        } else if type_eq(target_type, &tt.xml_type()) {
            self.as_xml_literal_ast(nf, value.as_model().expect("ClassCastException"))
        } else if type_eq(target_type, &tt.xml_list_type()) {
            as_xml_list(nf, value.as_model().expect("ClassCastException"))
        } else {
            panic!("formatExpr: unsupported lvalue type: {}", target_type.get_name())
        }
    }

    fn as_xml_literal_ast(&self, nf: &Nf, component: &ModelRef) -> NodeRef {
        let ModelKind::Xml(x) = &component.kind else { panic!("ClassCastException") };
        let xml = x.get_literal_xml().unwrap_or_default();
        if x.get_is_e4x() {
            let literal_string = nf.f().literal_string(&xml);
            let list = nf.f().list(None, Some(literal_string));
            nf.f().literal_xml(Some(list), false, -1)
        } else {
            let base = ast::generate_getter_selector_qualified(&mut nf.f(), self.standard_defs.get_utils_package(), "XMLUtil", false);
            let create_xml_document_identifier = nf.f().identifier_intern("createXMLDocument", false);
            let literal_string = nf.f().literal_string(&xml);
            let argument_list = nf.f().argument_list(None, Some(literal_string));
            let selector = ast::call(nf, create_xml_document_identifier, Some(argument_list), false);
            let member_expression = nf.f().member_expression(Some(base), Some(selector));
            let first_child_identifier = nf.f().identifier_intern("firstChild", false);
            let get_expression = nf.f().get_expression(Some(first_child_identifier));
            nf.f().member_expression(Some(member_expression), Some(get_expression))
        }
    }

    /// `generateAssignExpr(nodeFactory, configNamespaces, generateDocComments, statementList, lvalueBase)`, per subclass.
    pub fn generate_assign_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>, lvalue_base: NodeRef) -> NodeRef {
        match &self.kind {
            ValueInitializerKind::Style { .. } | ValueInitializerKind::Effect { .. } => {
                let identifier = nf.f().identifier_intern("setStyle", false);
                let literal_string = nf.f().literal_string(&self.get_name().unwrap_or_default());
                let argument_list = nf.f().argument_list(None, Some(literal_string));
                let value_expr = self.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                let argument_list = nf.f().argument_list(Some(argument_list), Some(value_expr));
                let call_expression = ast::call(nf, identifier, Some(argument_list), false);
                let expression_statement = ast::expr_stmt(nf, Some(lvalue_base), call_expression);
                nf.f().statement_list(statement_list, Some(expression_statement))
            }
            ValueInitializerKind::ArrayElement { index, .. } => {
                let literal_number = nf.f().literal_number_int(*index as i32);
                let expr = nf.f().argument_list(None, Some(literal_number));
                let value = self.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                let argument_list = nf.f().argument_list(None, Some(value));
                let set_expression = nf.f().set_expression(Some(expr), Some(argument_list), false);
                ast::set_mode(&set_expression, LEFTBRACKET_TOKEN);
                let expression_statement = ast::expr_stmt(nf, Some(lvalue_base), set_expression);
                nf.f().statement_list(statement_list, Some(expression_statement))
            }
            ValueInitializerKind::VisualChild => self.visual_child_generate_assign_expr(nf, config_namespaces, generate_doc_comments, statement_list, lvalue_base),
            ValueInitializerKind::InitializedPropertyDeclaration { .. } => {
                let is_this = lvalue_base.borrow().as_this_expression().is_some();
                if is_this && self.has_definition() && self.get_model().expect("ClassCastException").is_declared() {
                    let value_expr = self.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                    let list = nf.f().list(None, Some(value_expr));
                    let expression_statement = nf.f().expression_statement(Some(list));
                    nf.f().statement_list(statement_list, Some(expression_statement))
                } else {
                    self.named_generate_assign_expr(nf, config_namespaces, generate_doc_comments, statement_list, lvalue_base)
                }
            }
            _ => self.named_generate_assign_expr(nf, config_namespaces, generate_doc_comments, statement_list, lvalue_base),
        }
    }

    /// `NamedInitializer.generateAssignExpr(...)`.
    fn named_generate_assign_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>, lvalue_base: NodeRef) -> NodeRef {
        let name = self.get_name().unwrap_or_default();
        assert!(deps::is_valid_identifier(&name), "assert false");
        let value_expr = self.generate_value_expr(nf, config_namespaces, generate_doc_comments);
        let expression_statement = ast::generate_assignment_base(&mut nf.f(), Some(lvalue_base), &name, Some(value_expr));
        nf.f().statement_list(statement_list, Some(expression_statement))
    }

    /// `VisualChildInitializer.generateAssignExpr(...)`.
    fn visual_child_generate_assign_expr(&self, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>, lvalue_base: NodeRef) -> NodeRef {
        if self.standard_defs.is_repeater(&self.get_lvalue_type()) {
            let name = {
                let b = lvalue_base.borrow();
                let m = b.as_member_expression().expect("ClassCastException");
                let sel = m.selector.clone().expect("NullPointerException");
                let s = sel.borrow();
                let e = s.selector().expect("ClassCastException").expr.clone().expect("NullPointerException");
                let n = e.borrow().as_identifier().expect("ClassCastException").name.clone();
                n
            };
            let value = self.generate_value_expr(nf, config_namespaces, generate_doc_comments);
            let variable_definition = ast::generate_variable(&mut nf.f(), "repeater", &self.standard_defs.class_repeater_dot, true, Some(value));
            nf.f().statement_list(statement_list.clone(), Some(variable_definition));

            let repeater_base = ast::generate_getter_selector(&mut nf.f(), "repeater", false);
            let initialize_repeater_identifier = nf.f().identifier_intern("initializeRepeater", false);
            let lvalue_base_member_expression = ast::generate_getter_selector(&mut nf.f(), &name, true);
            let initialize_repeater_argument_list = nf.f().argument_list(None, Some(lvalue_base_member_expression));
            let literal_boolean = nf.f().literal_boolean(true);
            let initialize_repeater_argument_list = nf.f().argument_list(Some(initialize_repeater_argument_list), Some(literal_boolean));
            let initialize_repeater_call_expression = ast::call(nf, initialize_repeater_identifier, Some(initialize_repeater_argument_list), false);
            let initialize_repeater_expression_statement = ast::expr_stmt(nf, Some(repeater_base), initialize_repeater_call_expression);
            nf.f().statement_list(statement_list.clone(), Some(initialize_repeater_expression_statement));

            let condition = {
                let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), deps::PROP_CONTAINER_CHILDREPEATERS, false);
                let get_expression = nf.f().get_expression(Some(qualified_identifier));
                nf.f().member_expression(Some(lvalue_base), Some(get_expression))
            };
            let then_expr = {
                let base = ast::generate_getter_selector(&mut nf.f(), &name, true);
                let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), deps::PROP_CONTAINER_CHILDREPEATERS, false);
                let selector = nf.f().get_expression(Some(qualified_identifier));
                nf.f().member_expression(Some(base), Some(selector))
            };
            let else_expr = {
                let base = ast::generate_getter_selector(&mut nf.f(), &name, true);
                let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), deps::PROP_CONTAINER_CHILDREPEATERS, false);
                let literal_array = nf.f().literal_array(None);
                let argument_list = nf.f().argument_list(None, Some(literal_array));
                let selector = nf.f().set_expression(Some(qualified_identifier), Some(argument_list), false);
                ast::no_rvalue(&selector);
                let member_expression = nf.f().member_expression(Some(base), Some(selector));
                nf.f().list(None, Some(member_expression))
            };
            let conditional_expression = nf.f().conditional_expression(Some(condition), Some(then_expr), Some(else_expr));
            let base = nf.f().list(None, Some(conditional_expression));
            let push_identifier = nf.f().identifier_intern("push", false);
            let repeater_member_expression = ast::generate_getter_selector(&mut nf.f(), "repeater", false);
            let push_argument_list = nf.f().argument_list(None, Some(repeater_member_expression));
            let push_call_expression = ast::call(nf, push_identifier, Some(push_argument_list), false);
            let push_expression_statement = ast::expr_stmt(nf, Some(base), push_call_expression);
            nf.f().statement_list(statement_list, Some(push_expression_statement))
        } else {
            let identifier = nf.f().identifier_intern("addChild", false);
            let value_expr = self.generate_value_expr(nf, config_namespaces, generate_doc_comments);
            let argument_list = nf.f().argument_list(None, Some(value_expr));
            let call_expression = ast::call(nf, identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(lvalue_base), call_expression);
            nf.f().statement_list(statement_list, Some(expression_statement))
        }
    }
}

/// `addAssignExprs(nodeFactory, configNamespaces, generateDocComments, statementList, initIter, name)`.
fn add_assign_exprs(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: &NodeRef, inits: &[InitializerRef], name: &str) {
    let mut statement_list = statement_list.clone();
    for init in inits {
        if !init.is_state_specific() {
            let member_expression = ast::generate_getter_selector(&mut nf.f(), name, true);
            statement_list = init.generate_assign_expr(nf, config_namespaces, generate_doc_comments, Some(statement_list), member_expression);
        }
    }
}

fn generate_document_descriptor_assignment(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, movie_clip: &ModelRef, var_name: &str, statement_list: NodeRef) -> NodeRef {
    let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), "_documentDescriptor", false);
    let descriptors_member_expression = implementation_generator::add_descriptor_initializer_fragments(nf, config_namespaces, generate_doc_comments, movie_clip, Some(&HashSet::new()), false);
    let argument_list = nf.f().argument_list(None, Some(descriptors_member_expression));
    let set_expression = nf.f().set_expression(Some(qualified_identifier), Some(argument_list), false);
    let base = ast::generate_getter_selector(&mut nf.f(), var_name, false);
    let expression_statement = ast::expr_stmt(nf, Some(base), set_expression);
    nf.f().statement_list(Some(statement_list), Some(expression_statement))
}

fn generate_document_descriptor_document_assignment(nf: &Nf, var_name: &str, statement_list: NodeRef) -> NodeRef {
    let var_name_member_expression = ast::generate_getter_selector(&mut nf.f(), var_name, false);
    let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), "_documentDescriptor", false);
    let document_descriptor_get_expression = nf.f().get_expression(Some(qualified_identifier));
    let base = nf.f().member_expression(Some(var_name_member_expression), Some(document_descriptor_get_expression));
    let identifier = nf.f().identifier_intern("document", false);
    let this_expression = nf.f().this_expression(-1);
    let argument_list = nf.f().argument_list(None, Some(this_expression));
    let set_expression = nf.f().set_expression(Some(identifier), Some(argument_list), false);
    let expression_statement = ast::expr_stmt(nf, Some(base), set_expression);
    nf.f().statement_list(Some(statement_list), Some(expression_statement))
}

fn generate_repeater_child_descriptor_loop(nf: &Nf) -> NodeRef {
    let initializer = nf.f().literal_number_int(0);
    let init = ast::generate_variable(&mut nf.f(), "i", INT, false, Some(initializer));
    let i_member_expression = ast::generate_getter_selector(&mut nf.f(), "i", false);
    let cd_member_expression = ast::generate_getter_selector(&mut nf.f(), "cd", false);
    let length_identifier = nf.f().identifier_intern("length", false);
    let length_get_expression = nf.f().get_expression(Some(length_identifier));
    let cd_length_member_expression = nf.f().member_expression(Some(cd_member_expression), Some(length_get_expression));
    let binary_expression = nf.f().binary_expression(LESSTHAN_TOKEN, Some(i_member_expression), Some(cd_length_member_expression));
    let test = nf.f().list(None, Some(binary_expression));
    let i_identifier = nf.f().identifier_intern("i", false);
    let increment = nf.f().increment(PLUSPLUS_TOKEN, Some(i_identifier), true);
    let incr_member_expression = nf.f().member_expression(None, Some(increment));
    let incr = nf.f().list(None, Some(incr_member_expression));
    let cd_member_expression = ast::generate_getter_selector(&mut nf.f(), "cd", false);
    let i_member_expression = ast::generate_getter_selector(&mut nf.f(), "i", false);
    let get_argument_list = nf.f().argument_list(None, Some(i_member_expression));
    let i_get_expression = nf.f().get_expression(Some(get_argument_list));
    ast::set_mode(&i_get_expression, LEFTBRACKET_TOKEN);
    let base = nf.f().member_expression(Some(cd_member_expression), Some(i_get_expression));
    let document_identifier = nf.f().identifier_intern("document", false);
    let this_expression = nf.f().this_expression(-1);
    let set_argument_list = nf.f().argument_list(None, Some(this_expression));
    let selector = nf.f().set_expression(Some(document_identifier), Some(set_argument_list), false);
    let expression_statement = ast::expr_stmt(nf, Some(base), selector);
    let body = nf.f().statement_list(None, Some(expression_statement));
    nf.f().for_statement(Some(init), Some(test), Some(incr), Some(body))
}

fn as_array_literal(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, array: &ModelRef) -> NodeRef {
    let ModelKind::Array(a) = &array.kind else { unreachable!() };
    let mut argument_list: Option<NodeRef> = None;
    for initializer in a.get_element_initializer_iterator() {
        if !initializer.is_state_specific() {
            let value_expr_node = initializer.generate_value_expr(nf, config_namespaces, generate_doc_comments);
            argument_list = Some(nf.f().argument_list(argument_list, Some(value_expr_node)));
        }
    }
    nf.f().literal_array(argument_list)
}

fn as_object_literal(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, model: &ModelRef) -> NodeRef {
    let mut argument_list: Option<NodeRef> = None;
    for init in model.get_property_initializer_iterator() {
        let identifier = nf.f().identifier(&init.get_name());
        let value = init.generate_value_expr(nf, config_namespaces, generate_doc_comments);
        let literal_field = nf.f().literal_field(Some(identifier), Some(value));
        argument_list = Some(nf.f().argument_list(argument_list, Some(literal_field)));
    }
    nf.f().literal_object(argument_list)
}

fn as_vector_literal(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, vector: &ModelRef) -> NodeRef {
    let ModelKind::Array(a) = &vector.kind else { unreachable!() };
    let mut argument_list: Option<NodeRef> = None;
    for initializer in a.get_element_initializer_iterator() {
        if !initializer.is_state_specific() {
            let value_expr_node = initializer.generate_value_expr(nf, config_namespaces, generate_doc_comments);
            argument_list = Some(nf.f().argument_list(argument_list, Some(value_expr_node)));
        }
    }
    let apply_type_expr = ast::generate_apply_type_expr(&mut nf.f(), &a.get_element_type_name());
    nf.f().literal_vector(Some(apply_type_expr), argument_list, 0)
}

fn as_xml_list(nf: &Nf, component: &ModelRef) -> NodeRef {
    let ModelKind::XmlList(x) = &component.kind else { panic!("ClassCastException") };
    let literal_string = nf.f().literal_string(&x.get_literal_xml().unwrap_or_default());
    let list = nf.f().list(None, Some(literal_string));
    let literal_xml = nf.f().literal_xml(Some(list), false, -1);
    literal_xml.borrow_mut().as_literal_xml_mut().unwrap().is_xmllist = true;
    literal_xml
}
