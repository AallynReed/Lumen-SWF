//! Port of `flex2.compiler.mxml.ImplementationGenerator`: direct AST generation of the full MXML class.

use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use super::abstract_generator::{AbstractGenerator, BINDABLE, MX_INTERNAL};
use super::gen::states_generator::StatesGenerator;
use super::rep::binding_expression::{BindingExpression, BindingExpressionRef};
use super::rep::deps;
use super::rep::model::ModelRef;
use super::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::flex2::compiler::as3::as3_compiler;
use crate::asc::flex2::compiler::css::style_def::StyleDefRef;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::flash::css::style_declaration::{StyleDeclarationRef, StyleProperty};
use crate::asc::flex2::flash::css::style_selector::StyleSelectorRef;
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::emitter::EmitterRef;
use crate::asc::util::context::{ConfigVar, ContextStatics};
use crate::asc::util::jstr;

const ASTERISK: &str = "*";
const __MODULE_FACTORY_INITIALIZED: &str = "__moduleFactoryInitialized";
const ACCEPT_MEDIA_LIST: &str = "acceptMediaList";
const ADD_EVENT_LISTENER: &str = "addEventListener";
const ADDED_TO_STAGE: &str = "ADDED_TO_STAGE";
const ARRAY: &str = "Array";
const BINDINGS: &str = "bindings";
const BOOLEAN: &str = "Boolean";
const CAPITAL_BINDING: &str = "Binding";
const CHILD_DESCRIPTORS: &str = "childDescriptors";
const CONCAT: &str = "concat";
const CONDITION: &str = "condition";
const CONDITIONS: &str = "conditions";
const CSS_CONDITION: &str = "CSSCondition";
const CSS_SELECTOR: &str = "CSSSelector";
const CSS_STYLE_DECLARATION: &str = "CSSStyleDeclaration";
const DEFAULT_FACTORY: &str = "defaultFactory";
const DESIGN_LAYER: &str = "designLayer";
const DESTINATION: &str = "destination";
const EFFECTS: &str = "effects";
const EMBED: &str = "Embed";
const EVENT_NAME: &str = "event";
const EVENT_TYPE: &str = "Event";
const EVENTS: &str = "events";
const EXECUTE: &str = "execute";
const FACTORY: &str = "factory";
const GET_DEFINITION_BY_NAME: &str = "getDefinitionByName";
const GET_STYLE_DECLARATION: &str = "getStyleDeclaration";
const GET_STYLE_MANAGER: &str = "getStyleManager";
const I: &str = "i";
const ID: &str = "id";
const IFLEX_MODULE_FACTORY: &str = "IFlexModuleFactory";
const INIT: &str = "init";
const INITIALIZE: &str = "initialize";
const INIT_PROTO_CHAIN_ROOTS: &str = "initProtoChainRoots";
const INSPECTABLE: &str = "Inspectable";
const INSTANCE_INDICES: &str = "instanceIndices";
const IS_TWO_WAY_PRIMARY: &str = "isTwoWayPrimary";
const ISTYLE_MANAGER2: &str = "IStyleManager2";
const IWATCHER_SETUP_UTIL2: &str = "IWatcherSetupUtil2";
const LENGTH: &str = "length";
const LOWERCASE_BINDING: &str = "binding";
const MODULE_FACTORY: &str = "moduleFactory";
const OBJECT: &str = "Object";
const PROXY: &str = "Proxy";
const PROPERTIES_FACTORY: &str = "propertiesFactory";
const PROPERTY_NAME: &str = "propertyName";
const PUSH: &str = "push";
const REGISTER_EFFECTS: &str = "registerEffects";
const REMOVE_EVENT_LISTENER: &str = "removeEventListener";
const REPEATABLE_BINDING: &str = "RepeatableBinding";
const REPEATER_INDICES: &str = "repeaterIndices";
const RESOURCE_BUNDLE: &str = "ResourceBundle";
const RESULT: &str = "result";
const SELECTOR: &str = "selector";
const SETUP: &str = "setup";
const SET_DOCUMENT_DESCRIPTOR: &str = "setDocumentDescriptor";
const SET_STYLE_DECLARATION: &str = "setStyleDeclaration";
const STRING: &str = "String";
const STYLE: &str = "style";
const STYLES_FACTORY: &str = "stylesFactory";
const STYLE_DECLARATION: &str = "styleDeclaration";
const STYLE_MANAGER_INSTANCE: &str = "styleManager";
const STYLE_MANAGER: &str = "StyleManager";
const TARGET: &str = "target";
const TWO_WAY_COUNTERPART: &str = "twoWayCounterpart";
const TYPE: &str = "type";
const UINT: &str = "uint";
const UI_COMPONENT_DESCRIPTOR: &str = "UIComponentDescriptor";
const UNDEFINED: &str = "undefined";
const WATCHERS: &str = "watchers";
const WATCHER_SETUP_UTIL: &str = "watcherSetupUtil";
const WATCHER_SETUP_UTIL_CLASS: &str = "watcherSetupUtilClass";
const _BINDINGS: &str = "_bindings";
const _DOCUMENT: &str = "_document";
const _DOCUMENT_DESCRIPTOR_: &str = "_documentDescriptor_";
const _SOURCE_FUNCTION_RETURN_VALUE: &str = "_sourceFunctionReturnValue";
const _WATCHERS: &str = "_watchers";
const _WATCHER_SETUP_UTIL: &str = "_watcherSetupUtil";
const PACKAGE_FLASH_UTILS: &str = "flash.utils";
const EMPTY_DOC_COMMENT: &str = "<description><![CDATA[]]></description>";

pub struct ImplementationGenerator {
    base: AbstractGenerator,
    mxml_document: MxmlDocumentRef,
    process_comments: bool,
}

impl ImplementationGenerator {
    /// `new ImplementationGenerator(mxmlDocument, generateDocComments, contextStatics, source, bytecodeEmitter, defines, processComments)`.
    pub fn new(mxml_document: &MxmlDocumentRef, generate_doc_comments: bool, context_statics: &Rc<std::cell::RefCell<ContextStatics>>, source: &SourceRef, bytecode_emitter: &EmitterRef, defines: &[ConfigVar], process_comments: bool) -> ImplementationGenerator {
        let mut base = AbstractGenerator::new(mxml_document.get_standard_defs());
        base.generate_doc_comments = generate_doc_comments;
        let context = ast::generate_context(context_statics, source, bytecode_emitter, defines);
        base.node_factory = Some(Nf::of(&context));
        base.context = Some(context);
        let mut g = ImplementationGenerator { base, mxml_document: mxml_document.clone(), process_comments };

        let package_doc_comment = if generate_doc_comments {
            Some(g.generate_package_doc_comment(&mxml_document.get_package_name(), &mxml_document.get_class_name(), &mxml_document.get_source_path()))
        } else {
            None
        };
        let cx = g.base.cx().clone();
        let mut config_namespaces = HashSet::new();
        let config_vars = ast::parse_config_vars(&cx, &mut config_namespaces);
        g.base.config_namespaces = config_namespaces;
        let program_node = ast::generate_program_full(&cx, config_vars, &mxml_document.get_package_name(), package_doc_comment, mxml_document.get_root().get_xml_line_number());
        g.base.program_node = Some(program_node.clone());
        let program_statement_list = program_node.borrow().program().unwrap().statements.clone().expect("NullPointerException");
        let program_statement_list = g.generate_imports(program_statement_list);
        let program_statement_list = g.generate_at_resources(program_statement_list);
        let mut program_statement_list = g.base.generate_meta_data(program_statement_list, &mxml_document.get_metadata(), &g.get_path());
        let nf = g.base.nf().clone();
        if process_comments {
            let comment = mxml_document.get_comment();
            let class_doc_comment = match comment {
                Some(c) => ast::generate_doc_comment(&mut nf.f(), &c),
                None => ast::generate_doc_comment(&mut nf.f(), EMPTY_DOC_COMMENT),
            };
            program_statement_list = nf.f().statement_list(Some(program_statement_list), Some(class_doc_comment));
        }
        let class_definition = g.generate_class_definition();
        let program_statement_list = nf.f().statement_list(Some(program_statement_list), Some(class_definition));
        program_node.borrow_mut().program_mut().unwrap().statements = Some(program_statement_list.clone());
        let package_definition = nf.f().finish_package(&cx, None);
        nf.f().statement_list(Some(program_statement_list), package_definition);
        as3_compiler::clean_node_factory(&cx);
        g
    }

    fn nf(&self) -> &Nf {
        self.base.nf()
    }

    fn sd(&self) -> &Rc<deps::StandardDefs> {
        &self.base.standard_defs
    }

    fn doc(&self) -> &MxmlDocumentRef {
        &self.mxml_document
    }

    fn create_interface_names(&self) -> Vec<String> {
        let mut result: BTreeSet<jstr_key::Key> = BTreeSet::new();
        for info in self.doc().get_interface_names().iter() {
            result.insert(jstr_key::Key(info.get_name().to_string()));
        }
        result.into_iter().map(|k| k.0).collect()
    }

    fn generate_at_resources(&self, program_statement_list: NodeRef) -> NodeRef {
        let mut result = program_statement_list;
        for at_resource in self.doc().get_at_resources() {
            let meta_data = ast::generate_meta_data_value(&mut self.nf().f(), RESOURCE_BUNDLE, at_resource.get_bundle());
            result = self.nf().f().statement_list(Some(result), Some(meta_data));
        }
        result
    }

    fn generate_binding(&self, binding_expression: &BindingExpressionRef) -> NodeRef {
        let nf = self.nf();
        let base = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let literal_number = nf.f().literal_number_int(binding_expression.get_id());
        let literal_number_argument_list = nf.f().argument_list(None, Some(literal_number));
        let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), self.sd().get_binding_package(), CAPITAL_BINDING, false);
        let this_expression = nf.f().this_expression(-1);
        let mut call_argument_list = nf.f().argument_list(None, Some(this_expression));
        if binding_expression.is_source_public_property() && binding_expression.get_destination_type_name() != ARRAY {
            let position = ast::line_number_to_position(&mut nf.f(), binding_expression.get_xml_line_number());
            let literal_null = nf.f().literal_null_pos(position);
            call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(literal_null));
        } else {
            let source_function_common = self.generate_source_function(binding_expression);
            call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(source_function_common));
        }
        if binding_expression.is_simple_chain() && !binding_expression.is_destination_non_public_property() {
            let literal_null = nf.f().literal_null_pos(-1);
            call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(literal_null));
        } else {
            let destination_function_common = self.generate_destination_function(binding_expression);
            call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(destination_function_common));
        }
        let dest_literal_string = nf.f().literal_string(&binding_expression.get_destination_path(false));
        call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(dest_literal_string));
        if binding_expression.is_source_public_property() {
            let src_literal_string = nf.f().literal_string(&binding_expression.get_source_as_property().unwrap_or_default());
            call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(src_literal_string));
        }
        let call_expression = ast::call(nf, qualified_identifier, Some(call_argument_list), true);
        let binding_member_expression = nf.f().member_expression(None, Some(call_expression));
        let set_argument_list = nf.f().argument_list(None, Some(binding_member_expression));
        let selector = nf.f().set_expression(Some(literal_number_argument_list), Some(set_argument_list), false);
        ast::set_mode(&selector, LEFTBRACKET_TOKEN);
        ast::expr_stmt(nf, Some(base), selector)
    }

    fn generate_bindings_for_loop(&self) -> NodeRef {
        let nf = self.nf();
        let i_member_expression = ast::generate_getter_selector(&mut nf.f(), I, false);
        let bindings_member_expression = ast::generate_getter_selector(&mut nf.f(), BINDINGS, false);
        let length_identifier = nf.f().identifier_intern(LENGTH, false);
        let length_get_expression = nf.f().get_expression(Some(length_identifier));
        ast::set_mode(&length_get_expression, DOT_TOKEN);
        let length_member_expression = nf.f().member_expression(Some(bindings_member_expression), Some(length_get_expression));
        let binary_expression = nf.f().binary_expression(LESSTHAN_TOKEN, Some(i_member_expression), Some(length_member_expression));
        let test = nf.f().list(None, Some(binary_expression));
        let i_identifier = nf.f().identifier_intern(I, false);
        let increment_node = nf.f().increment(PLUSPLUS_TOKEN, Some(i_identifier), true);
        let iterator_member_expression = nf.f().member_expression(None, Some(increment_node));
        let increment = nf.f().list(None, Some(iterator_member_expression));
        let binding_identifier = nf.f().identifier_intern(CAPITAL_BINDING, false);
        let bindings_member_expression = ast::generate_getter_selector(&mut nf.f(), BINDINGS, false);
        let i_member_expression = ast::generate_getter_selector(&mut nf.f(), I, false);
        let argument_list = nf.f().argument_list(None, Some(i_member_expression));
        let get_expression = nf.f().get_expression(Some(argument_list));
        ast::set_mode(&get_expression, LEFTBRACKET_TOKEN);
        let member_expression = nf.f().member_expression(Some(bindings_member_expression), Some(get_expression));
        let call_argument_list = nf.f().argument_list(None, Some(member_expression));
        let binding_call_expression = ast::call(nf, binding_identifier, Some(call_argument_list), false);
        let base = nf.f().member_expression(None, Some(binding_call_expression));
        let execute_identifier = nf.f().identifier_intern(EXECUTE, false);
        let execute_call_expression = ast::call(nf, execute_identifier, None, false);
        let expression_statement = ast::expr_stmt(nf, Some(base), execute_call_expression);
        let statement = nf.f().statement_list(None, Some(expression_statement));
        nf.f().for_statement(None, Some(test), Some(increment), Some(statement))
    }

    fn generate_binding_management_vars(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        for variable_declaration in super::rep::mxml_document::MxmlDocument::get_binding_management_vars() {
            if !self.doc().super_has_public_property(variable_declaration.get_name()) {
                if self.base.generate_doc_comments {
                    let doc_comment = ast::generate_private_doc_comment(&mut nf.f());
                    result = Some(nf.f().statement_list(result, Some(doc_comment)));
                }
                let initializer_string = variable_declaration.get_initializer();
                let initializer_node = match initializer_string {
                    "[]" => Some(nf.f().literal_array(None)),
                    "{}" => Some(nf.f().literal_object(None)),
                    _ => panic!("assert false: {initializer_string}"),
                };
                let variable_name = variable_declaration.get_name();
                let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), variable_name, false);
                let attrs = self.base.generate_mx_internal_attribute();
                let variable_definition = ast::generate_variable_attrs(&mut nf.f(), Some(attrs), qualified_identifier, variable_declaration.get_type(), false, initializer_node);
                result = Some(nf.f().statement_list(result, Some(variable_definition)));
            }
        }
        result
    }

    fn generate_bindings_assignment(&self) -> NodeRef {
        let nf = self.nf();
        let mx_internal_getter_selector = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_core_package(), MX_INTERNAL);
        let qualified_identifier = ast::generate_qualified_identifier_member(&mut nf.f(), mx_internal_getter_selector, _BINDINGS, false);
        let rvalue_base = self.generate_mx_internal_getter_selector(_BINDINGS, false);
        let concat_identifier = nf.f().identifier_intern(CONCAT, false);
        let bindings_member_expression = ast::generate_getter_selector(&mut nf.f(), BINDINGS, false);
        let concat_argument_list = nf.f().argument_list(None, Some(bindings_member_expression));
        let call_expression = ast::call(nf, concat_identifier, Some(concat_argument_list), false);
        let argument = nf.f().member_expression(Some(rvalue_base), Some(call_expression));
        let argument_list = nf.f().argument_list(None, Some(argument));
        let set_expression = nf.f().set_expression(Some(qualified_identifier), Some(argument_list), false);
        ast::expr_stmt(nf, None, set_expression)
    }

    fn generate_binding_exprs_function(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let result = statement_list;
        let function_signature = nf.f().function_signature(None, None);
        ast::void_anno(&function_signature);
        let variable_definition = ast::generate_variable_untyped(&mut nf.f(), DESTINATION, false);
        let mut function_statement_list = nf.f().statement_list(None, Some(variable_definition));
        let all_ns = self.doc().get_all_binding_namespaces();
        if !all_ns.is_empty() {
            function_statement_list = generate_namespace_declarations(&all_ns, cx, Some(function_statement_list)).expect("NullPointerException");
        }
        for binding_expression in self.doc().get_binding_expressions() {
            if binding_expression.get_destination().is_none() {
                let text = format!("{} = {}", binding_expression.get_destination_property().unwrap_or_else(|| "null".into()), binding_expression.get_source_expression());
                let node_list = ast::parse_expression(cx, &self.base.config_namespaces, &text, binding_expression.get_xml_line_number(), self.base.generate_doc_comments);
                if let Some(first) = node_list.into_iter().next() {
                    function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(first));
                }
            }
        }
        let function_common = nf.f().function_common(cx, None, Some(function_signature), Some(function_statement_list));
        ast::user_defined_body(&function_common);
        let attribute_list = ast::generate_private_attribute(&mut nf.f());
        let bindings_setup_identifier = nf.f().identifier(&format!("{}_bindingExprs", self.doc().get_converted_class_name()));
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(bindings_setup_identifier));
        let function_definition = nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common));
        nf.f().statement_list(result, Some(function_definition))
    }

    fn generate_bindings_setup(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let mut result = self.generate_bindings_setup_function(statement_list);
        if self.doc().has_binding_tags() {
            result = self.generate_binding_exprs_function(Some(result));
        }
        result = self.generate_set_watcher_setup_util_function(Some(result));
        let attribute_list = ast::generate_private_static_attribute(&mut nf.f());
        let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), _WATCHER_SETUP_UTIL, false);
        let variable_definition = ast::generate_variable_attrs(&mut nf.f(), Some(attribute_list), qualified_identifier, IWATCHER_SETUP_UTIL2, false, None);
        nf.f().statement_list(Some(result), Some(variable_definition))
    }

    fn generate_bindings_setup_function(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let result = statement_list;
        let array_member_expression = ast::generate_getter_selector(&mut nf.f(), ARRAY, false);
        let return_type = nf.f().type_expression(Some(array_member_expression), true, false, -1);
        let function_signature = nf.f().function_signature(None, Some(return_type));
        let result_variable_definition = self.generate_result_variable();
        let mut function_statement_list = nf.f().statement_list(None, Some(result_variable_definition));
        for binding_expression in self.doc().get_binding_expressions() {
            let expression_statement = if binding_expression.is_repeatable() { self.generate_repeatable_binding(&binding_expression) } else { self.generate_binding(&binding_expression) };
            function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(expression_statement));
            if let Some(counterpart) = binding_expression.get_two_way_counterpart() {
                if binding_expression.is_two_way_primary() {
                    let s = self.generate_is_two_way_primary_assignment(binding_expression.get_id());
                    function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(s));
                }
                let s = self.generate_two_way_counterpart_assignment(binding_expression.get_id(), counterpart.get_id());
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(s));
                if counterpart.is_two_way_primary() {
                    let s = self.generate_is_two_way_primary_assignment(counterpart.get_id());
                    function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(s));
                }
                let s = self.generate_two_way_counterpart_assignment(counterpart.get_id(), binding_expression.get_id());
                function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(s));
            }
        }
        let member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let list = nf.f().list(None, Some(member_expression));
        let return_statement = nf.f().return_statement(Some(list));
        function_statement_list = nf.f().statement_list(Some(function_statement_list), Some(return_statement));
        let function_common = nf.f().function_common(cx, None, Some(function_signature), Some(function_statement_list));
        ast::user_defined_body(&function_common);
        let attribute_list = ast::generate_private_attribute(&mut nf.f());
        let bindings_setup_identifier = nf.f().identifier(&format!("{}_bindingsSetup", self.doc().get_converted_class_name()));
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(bindings_setup_identifier));
        let function_definition = nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common));
        nf.f().statement_list(result, Some(function_definition))
    }

    fn generate_bindings_variable(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(BINDINGS));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), ARRAY, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let bindings_setup_identifier = nf.f().identifier(&format!("{}_bindingsSetup", self.doc().get_converted_class_name()));
        let call_expression = ast::call(nf, bindings_setup_identifier, None, false);
        let initializer = nf.f().member_expression(None, Some(call_expression));
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(initializer));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn generate_class_definition(&self) -> NodeRef {
        let nf = self.nf();
        let doc = self.doc();
        let mut statement_list = self.generate_instance_variables(None);
        statement_list = self.generate_type_import_dummies(statement_list);
        let class_name = doc.get_class_name();

        if doc.get_is_iui_component() {
            if doc.get_describe_visual_children() && doc.get_is_container() {
                let member_expression = ast::generate_getter_selector_qualified(&mut nf.f(), self.sd().get_core_package(), UI_COMPONENT_DESCRIPTOR, false);
                let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
                let initializer = get_descriptor_initializer_fragments(nf, &self.base.config_namespaces, self.base.generate_doc_comments, &doc.get_root());
                let document_descriptor_variable_definition = ast::generate_private_variable_init(&mut nf.f(), Some(type_expression), _DOCUMENT_DESCRIPTOR_, Some(initializer));
                statement_list = Some(nf.f().statement_list(statement_list, Some(document_descriptor_variable_definition)));
            }
            let mut constructor_statement_list: Option<NodeRef> = None;
            if doc.get_is_container() || doc.get_is_visual_element_container() {
                let expression_statement = self.generate_document_assignment();
                constructor_statement_list = Some(nf.f().statement_list(constructor_statement_list, Some(expression_statement)));
            }
            constructor_statement_list = self.generate_binding_initializers(constructor_statement_list);
            constructor_statement_list = self.generate_component_initializers(constructor_statement_list);
            constructor_statement_list = self.generate_states_initializers(constructor_statement_list);
            constructor_statement_list = self.generate_initial_binding_executions(constructor_statement_list);
            statement_list = Some(self.generate_constructor(statement_list, self.base.generate_doc_comments, &class_name, None, constructor_statement_list));

            if doc.get_is_iflex_module() {
                let v = self.generate_module_factory_initialized_variable();
                statement_list = Some(nf.f().statement_list(statement_list, Some(v)));
                let module_factory_statement_list = self.generate_component_style_initializers(None);
                if self.base.generate_doc_comments {
                    let doc_comment = ast::generate_private_doc_comment(&mut nf.f());
                    statement_list = Some(nf.f().statement_list(statement_list, Some(doc_comment)));
                }
                let function_definition = self.generate_module_factory_property_override(module_factory_statement_list);
                statement_list = Some(nf.f().statement_list(statement_list, Some(function_definition)));
            }
            if self.base.generate_doc_comments {
                let doc_comment = ast::generate_private_doc_comment(&mut nf.f());
                statement_list = Some(nf.f().statement_list(statement_list, Some(doc_comment)));
            }
            let initialize = self.generate_initialize_function();
            statement_list = Some(nf.f().statement_list(statement_list, Some(initialize)));
        } else {
            let mut constructor_statement_list: Option<NodeRef> = None;
            constructor_statement_list = self.generate_binding_initializers(constructor_statement_list);
            constructor_statement_list = self.generate_states_initializers(constructor_statement_list);
            constructor_statement_list = self.generate_component_initializers(constructor_statement_list);
            constructor_statement_list = self.generate_initial_binding_executions(constructor_statement_list);
            statement_list = Some(self.generate_constructor(statement_list, self.base.generate_doc_comments, &class_name, None, constructor_statement_list));
        }

        if doc.get_has_stage_property_initializers() {
            statement_list = Some(self.generate_added_to_stage_handler_function(statement_list));
        }
        statement_list = self.base.generate_scripts(statement_list, &doc.get_scripts());
        statement_list = self.generate_initializer_support_defs(statement_list);
        statement_list = self.generate_embeds(statement_list);
        if !doc.get_binding_expressions().is_empty() {
            statement_list = self.generate_binding_management_vars(statement_list);
        }
        ast::generate_class_definition(self.base.cx(), &class_name, &doc.get_super_class_name(), &self.create_interface_names(), statement_list)
    }

    fn generate_added_to_stage_handler_function(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let parameter = ast::generate_parameter(&mut nf.f(), EVENT_NAME, EVENT_TYPE, false);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        ast::void_anno(&function_signature);
        let mut body_list = self.generate_event_listener_call(None, false);
        body_list = self.generate_property_initializers(body_list, true);
        let function_common = nf.f().function_common(cx, None, Some(function_signature), body_list);
        ast::user_defined_body(&function_common);
        let attribute_list = ast::generate_private_attribute(&mut nf.f());
        let func_name = self.get_added_to_stage_handler_function_name();
        let identifier = nf.f().identifier_intern(&func_name, true);
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(identifier));
        let function_definition = nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common));
        nf.f().statement_list(statement_list, Some(function_definition))
    }

    fn get_added_to_stage_handler_function_name(&self) -> String {
        format!("_{}_addedToStageHandler", self.doc().get_class_name())
    }

    fn styles_init_call(&self, result: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let doc = self.doc();
        if !doc.get_styles_container().get_style_defs().is_empty() || doc.get_is_flex_application() {
            let function_name = format!("_{}_StylesInit", doc.get_class_name());
            let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), &function_name, true);
            let call_expression = ast::call(nf, qualified_identifier, None, false);
            let expression_statement = ast::expr_stmt(nf, None, call_expression);
            return Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        result
    }

    fn generate_component_initializers(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list;
        if !self.doc().get_is_iflex_module() {
            result = self.generate_component_style_settings(result);
            result = self.styles_init_call(result);
        }
        result = self.generate_design_layer_initializers(result);
        result = self.generate_property_initializers(result, false);
        result = self.generate_event_listener_call(result, true);
        self.generate_event_initializers(result)
    }

    fn generate_component_style_initializers(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let result = self.generate_component_style_settings(statement_list);
        self.styles_init_call(result)
    }

    fn generate_event_listener_call(&self, statement_list: Option<NodeRef>, add_listener: bool) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        if self.doc().get_has_stage_property_initializers() {
            let added_to_stage_identifier_node = nf.f().identifier_intern(ADDED_TO_STAGE, false);
            let added_to_stage_get_expression = nf.f().get_expression(Some(added_to_stage_identifier_node));
            let event_identifier_node = nf.f().identifier_intern(EVENT_TYPE, false);
            let event_get_expression = nf.f().get_expression(Some(event_identifier_node));
            let event_member_expression = nf.f().member_expression(None, Some(event_get_expression));
            let event_added_to_stage_member_expression = nf.f().member_expression(Some(event_member_expression), Some(added_to_stage_get_expression));
            let added_to_stage_func_name = self.get_added_to_stage_handler_function_name();
            let added_to_stage_func_node = ast::generate_getter_selector(&mut nf.f(), &added_to_stage_func_name, true);
            let argument_list = nf.f().argument_list(None, Some(event_added_to_stage_member_expression));
            nf.f().argument_list(Some(argument_list.clone()), Some(added_to_stage_func_node));
            let add_event_listener_identifier = nf.f().identifier_intern(if add_listener { ADD_EVENT_LISTENER } else { REMOVE_EVENT_LISTENER }, false);
            let call_expression = ast::call(nf, add_event_listener_identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, None, call_expression);
            result = Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        result
    }

    fn generate_variable(&self, name: &str, type_: &str) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = self.generate_mx_internal_qualified_identifier(name, false);
        let member_expression = ast::generate_getter_selector(&mut nf.f(), type_, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), None);
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn generate_binding_initializers(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        if !self.doc().get_binding_expressions().is_empty() {
            let v = self.generate_bindings_variable();
            result = Some(nf.f().statement_list(result, Some(v)));
            let v = self.generate_watchers_variable();
            result = Some(nf.f().statement_list(result, Some(v)));
            let v = self.generate_target_variable();
            result = Some(nf.f().statement_list(result, Some(v)));

            let watcher_setup_util_member_expression = ast::generate_getter_selector(&mut nf.f(), _WATCHER_SETUP_UTIL, false);
            let literal_null = nf.f().literal_null_pos(-1);
            let binary_expression = nf.f().binary_expression(EQUALS_TOKEN, Some(watcher_setup_util_member_expression), Some(literal_null));
            let test = nf.f().list(None, Some(binary_expression));
            let v = self.generate_watcher_setup_util_class_variable();
            let mut then = nf.f().statement_list(None, Some(v));
            let base = ast::generate_getter_selector(&mut nf.f(), WATCHER_SETUP_UTIL_CLASS, false);
            let literal_string = nf.f().literal_string_intern(INIT, false);
            let init_argument_list = nf.f().argument_list(None, Some(literal_string));
            let literal_null = nf.f().literal_null_pos(-1);
            let null_argument_list = nf.f().argument_list(None, Some(literal_null));
            let init_selector = nf.f().call_expression(Some(init_argument_list), Some(null_argument_list));
            ast::set_mode(&init_selector, LEFTBRACKET_TOKEN);
            ast::no_rvalue(&init_selector);
            let expression_statement = ast::expr_stmt(nf, Some(base), init_selector);
            then = nf.f().statement_list(Some(then), Some(expression_statement));
            let if_statement = nf.f().if_statement(Some(test), Some(then), None);
            result = Some(nf.f().statement_list(result, Some(if_statement)));

            let watcher_setup_util_base = ast::generate_getter_selector(&mut nf.f(), _WATCHER_SETUP_UTIL, false);
            let setup_identifier = nf.f().identifier_intern(SETUP, false);
            let this_expression = nf.f().this_expression(-1);
            let mut setup_argument_list = nf.f().argument_list(None, Some(this_expression));
            let property_getter = self.generate_property_getter_function();
            setup_argument_list = nf.f().argument_list(Some(setup_argument_list), Some(property_getter));
            let static_property_getter = self.generate_static_property_getter_function();
            setup_argument_list = nf.f().argument_list(Some(setup_argument_list), Some(static_property_getter));
            let bindings_member_expression = ast::generate_getter_selector(&mut nf.f(), BINDINGS, false);
            setup_argument_list = nf.f().argument_list(Some(setup_argument_list), Some(bindings_member_expression));
            let watchers_member_expression = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
            setup_argument_list = nf.f().argument_list(Some(setup_argument_list), Some(watchers_member_expression));
            let setup_selector = ast::call(nf, setup_identifier, Some(setup_argument_list), false);
            let setup_expression_statement = ast::expr_stmt(nf, Some(watcher_setup_util_base), setup_selector);
            result = Some(nf.f().statement_list(result, Some(setup_expression_statement)));
            let s = self.generate_bindings_assignment();
            result = Some(nf.f().statement_list(result, Some(s)));
            let s = self.generate_watchers_assignment();
            result = Some(nf.f().statement_list(result, Some(s)));
        }
        result
    }

    fn generate_initial_binding_executions(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        if !self.doc().get_binding_expressions().is_empty() {
            let qualified_identifier = nf.f().qualified_identifier(None, Some(I));
            let uint_member_expression = ast::generate_getter_selector(&mut nf.f(), UINT, false);
            let type_expression = nf.f().type_expression(Some(uint_member_expression), true, false, -1);
            let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
            let initializer = nf.f().literal_number_int(0);
            let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(initializer));
            let initialize_list = nf.f().list(None, Some(variable_binding));
            let variable_definition = nf.f().variable_definition(None, VAR_TOKEN, Some(initialize_list));
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
            let for_statement = self.generate_bindings_for_loop();
            result = Some(nf.f().statement_list(result, Some(for_statement)));
        }
        result
    }

    fn generate_states_initializers(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list.clone();
        if self.doc().get_version() >= 4 {
            let mut generator = StatesGenerator::new(self.sd().clone());
            result = generator.get_states_ast_initializers(&self.doc().get_stateful_model(), self.nf(), &self.base.config_namespaces, self.base.generate_doc_comments, statement_list);
        }
        result
    }

    fn generate_component_style_settings(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let cx = self.base.cx();
        let mut result = statement_list;
        let root = self.doc().get_root();
        let inits = root.get_style_and_effect_initializer_iterator();
        if !inits.is_empty() {
            let if_statement = self.generate_style_declaration_if_statement();
            result = Some(nf.f().statement_list(result, Some(if_statement)));
            let this_expression = nf.f().this_expression(-1);
            let style_declaration_identifier = nf.f().identifier_intern(STYLE_DECLARATION, false);
            let get_expression = nf.f().get_expression(Some(style_declaration_identifier));
            let base = nf.f().member_expression(Some(this_expression), Some(get_expression));
            let default_factory_identifier = nf.f().identifier_intern(DEFAULT_FACTORY, false);
            let function_signature = nf.f().function_signature(None, None);
            ast::void_anno(&function_signature);
            let mut body: Option<NodeRef> = None;
            for named_initializer in &inits {
                let this_expression = nf.f().this_expression(-1);
                let name_identifier = nf.f().identifier(&named_initializer.get_name());
                let rvalue = named_initializer.generate_value_expr(nf, &self.base.config_namespaces, self.base.generate_doc_comments);
                let argument_list = nf.f().argument_list(None, Some(rvalue));
                let set_expression = nf.f().set_expression(Some(name_identifier), Some(argument_list), false);
                let expression_statement = ast::expr_stmt(nf, Some(this_expression), set_expression);
                body = Some(nf.f().statement_list(body, Some(expression_statement)));
            }
            let function_common = nf.f().function_common(cx, None, Some(function_signature), body);
            let argument_list = nf.f().argument_list(None, Some(function_common));
            let selector = nf.f().set_expression(Some(default_factory_identifier), Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            result = Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        let effects = root.get_effect_initializer_iterator();
        if !effects.is_empty() {
            let this_expression = nf.f().this_expression(-1);
            let identifier = nf.f().identifier_intern(REGISTER_EFFECTS, false);
            let mut effects_argument_list: Option<NodeRef> = None;
            for effect_initializer in &effects {
                let literal_string = nf.f().literal_string(&effect_initializer.get_name());
                effects_argument_list = Some(nf.f().argument_list(effects_argument_list, Some(literal_string)));
            }
            let literal_array = nf.f().literal_array(effects_argument_list);
            let argument_list = nf.f().argument_list(None, Some(literal_array));
            let call_expression = ast::call(nf, identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(this_expression), call_expression);
            result = Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        result
    }

    fn generate_constructor(&self, statement_list: Option<NodeRef>, generate_doc_comments: bool, class_name: &str, parameter_list: Option<NodeRef>, constructor_statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let mut result = statement_list;
        if generate_doc_comments && !self.process_comments {
            let doc_comment = ast::generate_private_doc_comment(&mut nf.f());
            result = Some(nf.f().statement_list(result, Some(doc_comment)));
        }
        if self.process_comments {
            let doc_comment = ast::generate_doc_comment(&mut nf.f(), EMPTY_DOC_COMMENT);
            result = Some(nf.f().statement_list(result, Some(doc_comment)));
        }
        let position = ast::line_number_to_position(&mut Nf::of(self.base.cx()).f(), self.doc().get_root().get_xml_line_number());
        let constructor = ast::generate_constructor(self.base.cx(), class_name, parameter_list, true, constructor_statement_list, position);
        nf.f().statement_list(result, Some(constructor))
    }

    fn generate_styles_package_variable(&self, name: &str, type_: &str) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = self.generate_mx_internal_qualified_identifier(name, false);
        let member_expression = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_styles_package(), type_);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), None);
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn generate_destination_function(&self, binding_expression: &BindingExpressionRef) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let destination_type_name = binding_expression.get_destination_type_name();
        let parameter = ast::generate_parameter(&mut nf.f(), _SOURCE_FUNCTION_RETURN_VALUE, &destination_type_name, true);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        ast::void_anno(&function_signature);
        let mut body: Option<NodeRef> = None;
        let text = if binding_expression.is_style() {
            format!("{}.setStyle(\"{}\", _sourceFunctionReturnValue)", binding_expression.get_destination_path_root(true), binding_expression.get_destination_style().unwrap_or_else(|| "null".into()))
        } else if binding_expression.is_destination_object_proxy() {
            format!("{} = new {}.ObjectProxy(_sourceFunctionReturnValue)", binding_expression.get_destination_path(true), self.sd().get_utils_package())
        } else {
            if !binding_expression.get_namespace_declarations().is_empty() {
                body = generate_namespace_declarations_of(binding_expression, cx, body);
            }
            format!("{} = _sourceFunctionReturnValue", binding_expression.get_destination_path(true))
        };
        let node_list = ast::parse_expression(cx, &self.base.config_namespaces, &text, binding_expression.get_xml_line_number(), self.base.generate_doc_comments);
        if let Some(first) = node_list.into_iter().next() {
            body = Some(nf.f().statement_list(body, Some(first)));
        }
        nf.f().function_common(cx, None, Some(function_signature), body)
    }

    fn generate_document_assignment(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), _DOCUMENT, false);
        let this_expression = nf.f().this_expression(-1);
        let argument_list = nf.f().argument_list(None, Some(this_expression));
        let set_expression = nf.f().set_expression(Some(qualified_identifier), Some(argument_list), false);
        ast::expr_stmt(nf, None, set_expression)
    }

    fn generate_effects_initializer(&self) -> NodeRef {
        let nf = self.nf();
        let effects_identifier = nf.f().identifier_intern(EFFECTS, false);
        let style_member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
        let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), EFFECTS, false);
        let effects_get_expression = nf.f().get_expression(Some(qualified_identifier));
        let style_effects_member_expression = nf.f().member_expression(Some(style_member_expression), Some(effects_get_expression));
        let argument_list = nf.f().argument_list(None, Some(style_effects_member_expression));
        let set_expression = nf.f().set_expression(Some(effects_identifier), Some(argument_list), false);
        ast::expr_stmt(nf, None, set_expression)
    }

    fn generate_embeds(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        for at_embed in self.doc().get_at_embeds() {
            let attributes: Vec<(String, String)> = at_embed.get_attributes().iter().map(|(k, v)| (k.clone(), v.to_string())).collect();
            let meta_data = ast::generate_meta_data_attributes(&mut nf.f(), EMBED, &attributes);
            result = Some(nf.f().statement_list(result, Some(meta_data)));
            let member_expression = ast::generate_getter_selector(&mut nf.f(), at_embed.get_type(), true);
            let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
            let variable_definition = ast::generate_private_variable(&mut nf.f(), Some(type_expression), at_embed.get_prop_name());
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
        }
        result
    }

    fn generate_event_initializers(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        for initializer in self.doc().get_root().get_event_initializer_iterator() {
            let this_expression = nf.f().this_expression(-1);
            result = Some(initializer.generate_assign_expr(nf, &self.base.config_namespaces, self.base.generate_doc_comments, result, this_expression));
        }
        result
    }

    fn generate_get_style_declaration(&self, style_declaration_name: &str) -> NodeRef {
        let nf = self.nf();
        let style_identifier = nf.f().identifier_intern(STYLE, false);
        let base = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER_INSTANCE, false);
        let get_style_declaration_identifier = nf.f().identifier_intern(GET_STYLE_DECLARATION, false);
        let literal_string = nf.f().literal_string(style_declaration_name);
        let call_expression_argument_list = nf.f().argument_list(None, Some(literal_string));
        let selector = ast::call(nf, get_style_declaration_identifier, Some(call_expression_argument_list), false);
        let argument_member_expression = nf.f().member_expression(Some(base), Some(selector));
        let argument_list = nf.f().argument_list(None, Some(argument_member_expression));
        let set_expression = nf.f().set_expression(Some(style_identifier), Some(argument_list), false);
        ast::expr_stmt(nf, None, set_expression)
    }

    fn generate_if_null_effects_and_pushes(&self, style_def: &StyleDefRef, declaration: Option<&StyleDeclarationRef>, statement_list: &NodeRef) {
        let nf = self.nf();
        let member_expression = ast::generate_getter_selector(&mut nf.f(), EFFECTS, false);
        let unary_expression = nf.f().unary_expression(NOT_TOKEN, Some(member_expression));
        let test = nf.f().list(None, Some(unary_expression));
        let effects_identifier = nf.f().identifier_intern(EFFECTS, false);
        let style_member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
        let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), EFFECTS, false);
        let literal_array = nf.f().literal_array(None);
        let mx_internal_effects_argument_list = nf.f().argument_list(None, Some(literal_array));
        let mx_internal_effects_set_expression = nf.f().set_expression(Some(qualified_identifier), Some(mx_internal_effects_argument_list), false);
        let mx_internal_effects_member_expression = nf.f().member_expression(Some(style_member_expression), Some(mx_internal_effects_set_expression));
        let effects_argument_list = nf.f().argument_list(None, Some(mx_internal_effects_member_expression));
        let effects_set_expression = nf.f().set_expression(Some(effects_identifier), Some(effects_argument_list), false);
        let expression_statement = ast::expr_stmt(nf, None, effects_set_expression);
        let if_statement_list = nf.f().statement_list(None, Some(expression_statement));
        let if_statement = nf.f().if_statement(Some(test), Some(if_statement_list), None);
        let statement_list = nf.f().statement_list(Some(statement_list.clone()), Some(if_statement));

        match declaration {
            Some(declaration) => {
                let blocks: Vec<_> = declaration.borrow().get_declaration_blocks().to_vec();
                for block in blocks {
                    let b = block.borrow();
                    let effect_styles: Vec<String> = b.get_effect_styles().to_vec();
                    if b.has_media_list() {
                        let effects_statement_list = self.generate_effect_styles(&effect_styles, None);
                        let expr = self.generate_style_manager_accept_media_list(&b.get_media_list().unwrap().to_string());
                        let media_test = nf.f().list(None, Some(expr));
                        let if_statement = nf.f().if_statement(Some(media_test), effects_statement_list, None);
                        nf.f().statement_list(Some(statement_list.clone()), Some(if_statement));
                    } else {
                        self.generate_effect_styles(&effect_styles, Some(statement_list.clone()));
                    }
                }
            }
            None => {
                let effect_styles: Vec<String> = style_def.borrow().get_effect_styles().to_vec();
                self.generate_effect_styles(&effect_styles, Some(statement_list));
            }
        }
    }

    fn generate_effect_styles(&self, effect_styles: &[String], statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut statement_list = statement_list;
        for effect_style in effect_styles {
            let literal_string = nf.f().literal_string(effect_style);
            let push_expression_statement = self.generate_push(EFFECTS, literal_string);
            statement_list = Some(nf.f().statement_list(statement_list, Some(push_expression_statement)));
        }
        statement_list
    }

    fn generate_if_null_style_declaration_selector(&self) -> NodeRef {
        let nf = self.nf();
        let style_member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
        let unary_expression = nf.f().unary_expression(NOT_TOKEN, Some(style_member_expression));
        let test = nf.f().list(None, Some(unary_expression));
        let style_identifier = nf.f().identifier_intern(STYLE, false);
        let css_style_declaration_identifier = ast::generate_resolved_identifier(&mut nf.f(), self.sd().get_styles_package(), CSS_STYLE_DECLARATION);
        let selector_member_expression = ast::generate_getter_selector(&mut nf.f(), SELECTOR, false);
        let call_argument_list = nf.f().argument_list(None, Some(selector_member_expression));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER_INSTANCE, false);
        let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(member_expression));
        let call_expression = ast::call(nf, css_style_declaration_identifier, Some(call_argument_list), true);
        let argument_member_expression = nf.f().member_expression(None, Some(call_expression));
        let set_argument_list = nf.f().argument_list(None, Some(argument_member_expression));
        let set_expression = nf.f().set_expression(Some(style_identifier), Some(set_argument_list), false);
        let expression_statement = ast::expr_stmt(nf, None, set_expression);
        let statement_list = nf.f().statement_list(None, Some(expression_statement));
        nf.f().if_statement(Some(test), Some(statement_list), None)
    }

    fn generate_if_null_style_declaration(&self, style_def: &StyleDefRef) -> NodeRef {
        let nf = self.nf();
        let style_member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
        let unary_expression = nf.f().unary_expression(NOT_TOKEN, Some(style_member_expression));
        let test = nf.f().list(None, Some(unary_expression));
        let statement_list = {
            let style_identifier = nf.f().identifier_intern(STYLE, false);
            let css_style_declaration_identifier = ast::generate_resolved_identifier(&mut nf.f(), self.sd().get_styles_package(), CSS_STYLE_DECLARATION);
            let literal_null = nf.f().literal_null();
            let argument_list = nf.f().argument_list(None, Some(literal_null));
            let member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER_INSTANCE, false);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(member_expression));
            let call_expression = ast::call(nf, css_style_declaration_identifier, Some(argument_list), true);
            let argument_member_expression = nf.f().member_expression(None, Some(call_expression));
            let argument_list = nf.f().argument_list(None, Some(argument_member_expression));
            let set_expression = nf.f().set_expression(Some(style_identifier), Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, None, set_expression);
            nf.f().statement_list(None, Some(expression_statement))
        };
        {
            let base = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_styles_package(), STYLE_MANAGER);
            let set_style_declaration_identifier = nf.f().identifier_intern(SET_STYLE_DECLARATION, false);
            let sdef = style_def.borrow();
            let literal_string = if sdef.is_type_selector() { nf.f().literal_string(&sdef.get_type_name()) } else { nf.f().literal_string(&format!(".{}", sdef.get_type_name())) };
            drop(sdef);
            let argument_list = nf.f().argument_list(None, Some(literal_string));
            let style_member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(style_member_expression));
            let literal_boolean = nf.f().literal_boolean(false);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(literal_boolean));
            let selector = ast::call(nf, set_style_declaration_identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            nf.f().statement_list(Some(statement_list.clone()), Some(expression_statement));
        }
        nf.f().if_statement(Some(test), Some(statement_list), None)
    }

    fn generate_if_null_style_factory(&self, style_def: &StyleDefRef, declaration: Option<&StyleDeclarationRef>) -> NodeRef {
        let nf = self.nf();
        let test = {
            let base = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
            let factory_identifier = nf.f().identifier_intern(FACTORY, false);
            let selector = nf.f().get_expression(Some(factory_identifier));
            ast::no_rvalue(&selector);
            let member_expression = nf.f().member_expression(Some(base), Some(selector));
            let literal_null = nf.f().literal_null_pos(-1);
            let binary_expression = nf.f().binary_expression(EQUALS_TOKEN, Some(member_expression), Some(literal_null));
            nf.f().list(None, Some(binary_expression))
        };
        let statement_list = {
            let base = ast::generate_getter_selector(&mut nf.f(), STYLE, false);
            let factory_identifier = nf.f().identifier_intern(FACTORY, false);
            let function_common = self.generate_style_factory(style_def, declaration);
            let argument_list = nf.f().argument_list(None, Some(function_common));
            let selector = nf.f().set_expression(Some(factory_identifier), Some(argument_list), false);
            ast::no_rvalue(&selector);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            nf.f().statement_list(None, Some(expression_statement))
        };
        nf.f().if_statement(Some(test), Some(statement_list), None)
    }

    fn generate_imports(&self, statement_list: NodeRef) -> NodeRef {
        let cx = self.base.cx();
        let nf = self.nf();
        let mut result = statement_list;
        for split_import in self.doc().get_split_imports() {
            let import_directive = ast::generate_import_split(cx, &split_import);
            result = nf.f().statement_list(Some(result), Some(import_directive));
        }
        for name_info in self.doc().get_imports().iter() {
            let position = ast::line_number_to_position(&mut nf.f(), name_info.get_line());
            let import_directive = ast::generate_import_pos(cx, name_info.get_name(), position);
            result = nf.f().statement_list(Some(result), Some(import_directive));
        }
        result
    }

    fn generate_initializer_support_defs(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let cns = &self.base.config_namespaces;
        let docs = self.base.generate_doc_comments;
        let mut result = statement_list;
        for i in self.doc().get_top_level_initializer_iterator() {
            result = i.generate_definitions(self.base.cx(), cns, docs, result);
        }
        for i in self.doc().get_stateful_event_iterator() {
            result = i.generate_definitions(self.base.cx(), cns, docs, result);
        }
        for i in self.doc().get_root().get_sub_initializer_iterator() {
            result = i.generate_definitions(self.base.cx(), cns, docs, result);
        }
        if self.doc().get_version() >= 4 {
            for i in self.doc().get_stateful_model().get_sub_initializer_iterators() {
                result = i.generate_definitions(self.base.cx(), cns, docs, result);
            }
        }
        if !self.doc().get_binding_expressions().is_empty() {
            result = Some(self.generate_bindings_setup(result));
        }
        if !self.doc().get_styles_container().get_style_defs().is_empty() || self.doc().get_is_flex_application() {
            result = Some(self.generate_styles_init_function(result));
        }
        result
    }

    fn generate_initialize_function(&self) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let function_signature = nf.f().function_signature(None, None);
        ast::void_anno(&function_signature);
        let mut statement_list: Option<NodeRef> = None;
        if self.doc().get_describe_visual_children() && self.doc().get_is_container() {
            let qualified_identifier = ast::generate_mx_internal_qualified_identifier(&mut nf.f(), SET_DOCUMENT_DESCRIPTOR, false);
            let getter_selector = ast::generate_getter_selector(&mut nf.f(), _DOCUMENT_DESCRIPTOR_, false);
            let argument_list = nf.f().argument_list(None, Some(getter_selector));
            let call_expression = ast::call(nf, qualified_identifier, Some(argument_list), false);
            let expression_statement = ast::expr_stmt(nf, None, call_expression);
            statement_list = Some(nf.f().statement_list(statement_list, Some(expression_statement)));
        }
        let super_expression = nf.f().super_expression(None, -1);
        let identifier = nf.f().identifier_intern(INITIALIZE, false);
        let call_expression = ast::call(nf, identifier, None, false);
        let expression_statement = ast::expr_stmt(nf, Some(super_expression), call_expression);
        statement_list = Some(nf.f().statement_list(statement_list, Some(expression_statement)));
        let function_common = nf.f().function_common(cx, None, Some(function_signature), statement_list);
        ast::user_defined_body(&function_common);
        let attribute_list = ast::generate_override_public_attribute(&mut nf.f());
        let identifier = nf.f().identifier_intern(INITIALIZE, false);
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(identifier));
        nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common))
    }

    fn generate_init_proto_chain_roots(&self) -> NodeRef {
        let nf = self.nf();
        let base = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER_INSTANCE, false);
        let identifier = nf.f().identifier(INIT_PROTO_CHAIN_ROOTS);
        let selector = ast::call(nf, identifier, None, false);
        ast::expr_stmt(nf, Some(base), selector)
    }

    fn generate_instance_variables(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let cx = self.base.cx();
        let docs = self.base.generate_doc_comments;
        let mut result = statement_list;
        for property_declaration in self.doc().get_declaration_iterator() {
            if property_declaration.get_inspectable() {
                let m = ast::generate_meta_data(&mut nf.f(), INSPECTABLE);
                result = Some(nf.f().statement_list(result, Some(m)));
            }
            if !property_declaration.get_id_is_autogenerated() || property_declaration.get_bindability_ensured() {
                let m = ast::generate_meta_data(&mut nf.f(), BINDABLE);
                result = Some(nf.f().statement_list(result, Some(m)));
            }
            if !property_declaration.get_id_is_autogenerated() {
                if self.process_comments {
                    let d = match property_declaration.get_comment() {
                        Some(c) => ast::generate_doc_comment(&mut nf.f(), &c),
                        None => ast::generate_doc_comment(&mut nf.f(), EMPTY_DOC_COMMENT),
                    };
                    result = Some(nf.f().statement_list(result, Some(d)));
                }
                if docs && !self.process_comments {
                    let d = ast::generate_private_doc_comment(&mut nf.f());
                    result = Some(nf.f().statement_list(result, Some(d)));
                }
            } else if docs {
                let d = ast::generate_private_doc_comment(&mut nf.f());
                result = Some(nf.f().statement_list(result, Some(d)));
            }
            let type_expression = ast::generate_type_expression(&mut nf.f(), &property_declaration.get_type_expr(), true);
            let variable_definition = ast::generate_public_variable(cx, type_expression, &property_declaration.get_name());
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
        }
        result
    }

    fn generate_is_two_way_primary_assignment(&self, left_value_id: i32) -> NodeRef {
        let nf = self.nf();
        let left_result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let left_literal_number = nf.f().literal_number_int(left_value_id);
        let left_get_expression_argument_list = nf.f().argument_list(None, Some(left_literal_number));
        let left_get_expression = nf.f().get_expression(Some(left_get_expression_argument_list));
        ast::set_mode(&left_get_expression, LEFTBRACKET_TOKEN);
        let base = nf.f().member_expression(Some(left_result_member_expression), Some(left_get_expression));
        let identifier = nf.f().identifier_intern(IS_TWO_WAY_PRIMARY, false);
        let literal_boolean = nf.f().literal_boolean(true);
        let set_expression_argument_list = nf.f().argument_list(None, Some(literal_boolean));
        let selector = nf.f().set_expression(Some(identifier), Some(set_expression_argument_list), false);
        ast::expr_stmt(nf, Some(base), selector)
    }

    fn generate_mx_internal_getter_selector(&self, name: &str, _intern: bool) -> NodeRef {
        let nf = self.nf();
        let mx_internal_getter_selector = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_core_package(), MX_INTERNAL);
        ast::generate_getter_selector_member(&mut nf.f(), mx_internal_getter_selector, name, true)
    }

    fn generate_null_initializer(&self, name: &str) -> NodeRef {
        let nf = self.nf();
        let identifier = nf.f().identifier(name);
        let literal_null = nf.f().literal_null_pos(-1);
        let argument_list = nf.f().argument_list(None, Some(literal_null));
        let set_expression = nf.f().set_expression(Some(identifier), Some(argument_list), false);
        ast::expr_stmt(nf, None, set_expression)
    }

    fn generate_package_doc_comment(&self, package_name: &str, class_name: &str, path: &str) -> NodeRef {
        let text = format!("<description><![CDATA[\n  Generated by mxmlc 4.0\n  Package: {package_name}\n Class:   {class_name}\n Source:  {path}\n ]]></description>");
        ast::generate_doc_comment(&mut self.nf().f(), &text)
    }

    fn generate_property_getter_function(&self) -> NodeRef {
        let nf = self.nf();
        let parameter = ast::generate_parameter(&mut nf.f(), PROPERTY_NAME, STRING, false);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        let base = ast::generate_getter_selector(&mut nf.f(), TARGET, false);
        let argument = ast::generate_getter_selector(&mut nf.f(), PROPERTY_NAME, false);
        let argument_list = nf.f().argument_list(None, Some(argument));
        let selector = nf.f().get_expression(Some(argument_list));
        ast::set_mode(&selector, LEFTBRACKET_TOKEN);
        let member_expression = nf.f().member_expression(Some(base), Some(selector));
        let list = nf.f().list(None, Some(member_expression));
        let return_statement = nf.f().return_statement(Some(list));
        let statement_list = nf.f().statement_list(None, Some(return_statement));
        nf.f().function_common(self.base.cx(), None, Some(function_signature), Some(statement_list))
    }

    fn generate_property_initializers(&self, statement_list: Option<NodeRef>, stage_properties: bool) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        let inits = if stage_properties { self.doc().get_stage_property_initializer_iterator() } else { self.doc().get_non_stage_property_initializer_iterator() };
        for initializer in inits {
            let this_expression = nf.f().this_expression(-1);
            result = Some(initializer.generate_assign_expr(nf, &self.base.config_namespaces, self.base.generate_doc_comments, result, this_expression));
        }
        result
    }

    fn generate_design_layer_initializers(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        for initializer in self.doc().get_design_layer_property_initializer_iterator() {
            let this_expression = nf.f().this_expression(-1);
            result = Some(initializer.generate_assign_expr(nf, &self.base.config_namespaces, self.base.generate_doc_comments, result, this_expression));
        }
        result
    }

    fn generate_push(&self, variable: &str, node: NodeRef) -> NodeRef {
        let nf = self.nf();
        let variable_member_expression = ast::generate_getter_selector(&mut nf.f(), variable, false);
        let identifier = nf.f().identifier(PUSH);
        let argument_list = nf.f().argument_list(None, Some(node));
        let call_expression = ast::call(nf, identifier, Some(argument_list), false);
        ast::expr_stmt(nf, Some(variable_member_expression), call_expression)
    }

    fn generate_repeatable_binding(&self, binding_expression: &BindingExpressionRef) -> NodeRef {
        let nf = self.nf();
        let base = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let literal_number = nf.f().literal_number_int(binding_expression.get_id());
        let literal_number_argument_list = nf.f().argument_list(None, Some(literal_number));
        let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), self.sd().get_binding_package(), REPEATABLE_BINDING, false);
        let this_expression = nf.f().this_expression(-1);
        let call_argument_list = nf.f().argument_list(None, Some(this_expression));
        let source_function_common = self.generate_repeatable_source_function(binding_expression);
        let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(source_function_common));
        let destination_function_common = self.generate_repeatable_destination_function(binding_expression);
        let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(destination_function_common));
        let literal_string = nf.f().literal_string(&binding_expression.get_destination_path(false));
        let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(literal_string));
        let call_expression = ast::call(nf, qualified_identifier, Some(call_argument_list), true);
        let binding_member_expression = nf.f().member_expression(None, Some(call_expression));
        let set_argument_list = nf.f().argument_list(None, Some(binding_member_expression));
        let selector = nf.f().set_expression(Some(literal_number_argument_list), Some(set_argument_list), false);
        ast::set_mode(&selector, LEFTBRACKET_TOKEN);
        ast::expr_stmt(nf, Some(base), selector)
    }

    fn generate_repeatable_destination_function(&self, binding_expression: &BindingExpressionRef) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let p = ast::generate_parameter(&mut nf.f(), _SOURCE_FUNCTION_RETURN_VALUE, &binding_expression.get_destination_type_name(), true);
        let parameter_list = nf.f().parameter_list(None, Some(p));
        let p = ast::generate_parameter(&mut nf.f(), INSTANCE_INDICES, ARRAY, false);
        let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        ast::void_anno(&function_signature);
        let root = binding_expression.get_destination_path_root(true);
        let lvalue = binding_expression.get_destination_lvalue().unwrap_or_else(|| "null".into());
        let text = if binding_expression.is_style() {
            format!("{root}.setStyle(\"{lvalue}\", _sourceFunctionReturnValue)")
        } else if binding_expression.is_destination_object_proxy() {
            format!("{root}.{lvalue} = new {}.ObjectProxy(_sourceFunctionReturnValue)", self.sd().get_utils_package())
        } else {
            format!("{root}.{lvalue} = _sourceFunctionReturnValue")
        };
        let node_list = ast::parse_expression(cx, &self.base.config_namespaces, &text, binding_expression.get_xml_line_number(), self.base.generate_doc_comments);
        let mut body: Option<NodeRef> = None;
        if let Some(first) = node_list.into_iter().next() {
            assert!(first.borrow().as_expression_statement().is_some(), "ClassCastException");
            body = Some(nf.f().statement_list(None, Some(first)));
        }
        nf.f().function_common(cx, None, Some(function_signature), body)
    }

    fn generate_repeatable_source_function(&self, binding_expression: &BindingExpressionRef) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let p = ast::generate_parameter(&mut nf.f(), INSTANCE_INDICES, ARRAY, false);
        let parameter_list = nf.f().parameter_list(None, Some(p));
        let p = ast::generate_parameter(&mut nf.f(), REPEATER_INDICES, ARRAY, false);
        let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
        let destination_type_name = binding_expression.get_destination_type_name();
        let return_type = ast::generate_type_expression(&mut nf.f(), &destination_type_name, true);
        let function_signature = nf.f().function_signature(Some(parameter_list), return_type);
        let mut body: Option<NodeRef> = None;
        let text = binding_expression.get_repeatable_source_expression();
        let node_list = ast::parse_expression(cx, &self.base.config_namespaces, &text, binding_expression.get_xml_line_number(), self.base.generate_doc_comments);
        if let Some(first) = node_list.into_iter().next() {
            let list = first.borrow().as_expression_statement().expect("ClassCastException").expr.clone().expect("NullPointerException");
            if destination_type_name == STRING {
                let item = list.borrow().list().unwrap().items[0].clone();
                body = self.generate_source_function_string_conversion(body, &destination_type_name, item);
            } else if destination_type_name == ARRAY {
                let item = list.borrow().list().unwrap().items[0].clone();
                body = self.generate_source_function_array_conversion(body, &destination_type_name, item);
            } else {
                let return_statement = nf.f().return_statement(Some(list));
                body = Some(nf.f().statement_list(body, Some(return_statement)));
            }
        }
        nf.f().function_common(cx, None, Some(function_signature), body)
    }

    fn generate_result_variable(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(RESULT));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), ARRAY, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let literal_array = nf.f().literal_array(None);
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(literal_array));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn generate_selector_ancestor(&self, selector: &StyleSelectorRef, statement_list: &NodeRef) {
        let nf = self.nf();
        let ancestor = selector.borrow().get_ancestor();
        if let Some(ancestor) = ancestor {
            self.generate_selector_ancestor(&ancestor, statement_list);
        }
        let conditions = selector.borrow().get_conditions().cloned();
        match conditions {
            Some(conditions) => {
                let identifier = nf.f().identifier_intern(CONDITIONS, false);
                let literal_array = nf.f().literal_array(None);
                let argument_list = nf.f().argument_list(None, Some(literal_array));
                let set_expression = nf.f().set_expression(Some(identifier), Some(argument_list), false);
                let expression_statement = ast::expr_stmt(nf, None, set_expression);
                nf.f().statement_list(Some(statement_list.clone()), Some(expression_statement));
                for condition in &conditions {
                    let condition_identifier = nf.f().identifier_intern(CONDITION, false);
                    let css_condition_identifier = ast::generate_resolved_identifier(&mut nf.f(), self.sd().get_styles_package(), CSS_CONDITION);
                    let literal_string = nf.f().literal_string(condition.get_kind());
                    let call_argument_list = nf.f().argument_list(None, Some(literal_string));
                    let literal_string = nf.f().literal_string(condition.get_value().unwrap_or_default());
                    let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(literal_string));
                    let call_expression = ast::call(nf, css_condition_identifier, Some(call_argument_list), true);
                    let argument_member_expression = nf.f().member_expression(None, Some(call_expression));
                    let set_argument_list = nf.f().argument_list(None, Some(argument_member_expression));
                    let condition_set_expression = nf.f().set_expression(Some(condition_identifier), Some(set_argument_list), false);
                    let condition_expression_statement = ast::expr_stmt(nf, None, condition_set_expression);
                    nf.f().statement_list(Some(statement_list.clone()), Some(condition_expression_statement));
                    let condition_member_expression = ast::generate_getter_selector(&mut nf.f(), CONDITION, false);
                    let push_expression_statement = self.generate_push(CONDITIONS, condition_member_expression);
                    nf.f().statement_list(Some(statement_list.clone()), Some(push_expression_statement));
                }
            }
            None => {
                let conditions_initializer = self.generate_null_initializer(CONDITIONS);
                nf.f().statement_list(Some(statement_list.clone()), Some(conditions_initializer));
            }
        }
        let condition_identifier = nf.f().identifier_intern(SELECTOR, false);
        let css_selector_identifier = ast::generate_resolved_identifier(&mut nf.f(), self.sd().get_styles_package(), CSS_SELECTOR);
        let value = selector.borrow().get_value().unwrap_or_default().to_string();
        let literal_string = nf.f().literal_string(&value);
        let call_argument_list = nf.f().argument_list(None, Some(literal_string));
        let conditions_member_expression = ast::generate_getter_selector(&mut nf.f(), CONDITIONS, false);
        let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(conditions_member_expression));
        let selector_member_expression = ast::generate_getter_selector(&mut nf.f(), SELECTOR, false);
        let call_argument_list = nf.f().argument_list(Some(call_argument_list), Some(selector_member_expression));
        let call_expression = ast::call(nf, css_selector_identifier, Some(call_argument_list), true);
        let argument_member_expression = nf.f().member_expression(None, Some(call_expression));
        let set_argument_list = nf.f().argument_list(None, Some(argument_member_expression));
        let set_expression = nf.f().set_expression(Some(condition_identifier), Some(set_argument_list), false);
        let expression_statement = ast::expr_stmt(nf, None, set_expression);
        nf.f().statement_list(Some(statement_list.clone()), Some(expression_statement));
    }

    fn generate_set_watcher_setup_util_function(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let mut result = statement_list;
        if self.base.generate_doc_comments {
            let doc_comment = ast::generate_private_doc_comment(&mut nf.f());
            result = Some(nf.f().statement_list(result, Some(doc_comment)));
        }
        let attribute_list = ast::generate_public_static_attribute(&mut nf.f());
        let watcher_setup_util_identifier = nf.f().identifier_intern(WATCHER_SETUP_UTIL, false);
        let function_name = nf.f().function_name(SET_TOKEN, Some(watcher_setup_util_identifier));
        let parameter = ast::generate_parameter(&mut nf.f(), WATCHER_SETUP_UTIL, IWATCHER_SETUP_UTIL2, false);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        ast::void_anno(&function_signature);
        let class_member_expression = ast::generate_getter_selector(&mut nf.f(), &self.doc().get_class_name(), true);
        let base = nf.f().list(None, Some(class_member_expression));
        let identifier = nf.f().identifier_intern(_WATCHER_SETUP_UTIL, false);
        let watcher_setup_util_member_expression = ast::generate_getter_selector(&mut nf.f(), WATCHER_SETUP_UTIL, false);
        let argument_list = nf.f().argument_list(None, Some(watcher_setup_util_member_expression));
        let selector = nf.f().set_expression(Some(identifier), Some(argument_list), false);
        ast::no_rvalue(&selector);
        let expression_statement = ast::expr_stmt(nf, Some(base), selector);
        let function_statement_list = nf.f().statement_list(None, Some(expression_statement));
        let function_common = nf.f().function_common(cx, None, Some(function_signature), Some(function_statement_list));
        ast::user_defined_body(&function_common);
        let function_definition = nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common));
        nf.f().statement_list(result, Some(function_definition))
    }

    fn generate_source_function(&self, binding_expression: &BindingExpressionRef) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let destination_type_name = binding_expression.get_destination_type_name();
        let return_type = if destination_type_name != ASTERISK {
            let member_expression = ast::generate_member_expression(&mut nf.f(), &destination_type_name);
            Some(nf.f().type_expression(Some(member_expression), true, false, -1))
        } else {
            None
        };
        let function_signature = nf.f().function_signature(None, return_type);
        let mut body: Option<NodeRef> = None;
        let text = binding_expression.get_source_expression();
        let node_list = ast::parse_expression(cx, &self.base.config_namespaces, &text, binding_expression.get_xml_line_number(), self.base.generate_doc_comments);
        if let Some(first) = node_list.into_iter().next() {
            let list = first.borrow().as_expression_statement().expect("ClassCastException").expr.clone().expect("NullPointerException");
            if destination_type_name == STRING {
                let item = list.borrow().list().unwrap().items[0].clone();
                body = self.generate_source_function_string_conversion(body, &destination_type_name, item);
            } else if destination_type_name == ARRAY {
                let item = list.borrow().list().unwrap().items[0].clone();
                body = self.generate_source_function_array_conversion(body, &destination_type_name, item);
            } else {
                if let Some(counterpart) = binding_expression.get_two_way_counterpart() {
                    if !counterpart.get_namespace_declarations().is_empty() {
                        body = generate_namespace_declarations_of(&counterpart, cx, body);
                    }
                }
                let return_statement = nf.f().return_statement(Some(list));
                body = Some(nf.f().statement_list(body, Some(return_statement)));
            }
        }
        nf.f().function_common(cx, None, Some(function_signature), body)
    }

    fn generate_source_function_array_conversion(&self, statement_list: Option<NodeRef>, _destination_type_name: &str, initializer: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(RESULT));
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), None);
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), initializer);
        let variable_list = nf.f().list(None, Some(variable_binding));
        let variable_definition = nf.f().variable_definition(None, VAR_TOKEN, Some(variable_list));
        let result = nf.f().statement_list(statement_list, Some(variable_definition));
        let result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let literal_null = nf.f().literal_null_pos(-1);
        let binary_expression = nf.f().binary_expression(EQUALS_TOKEN, Some(result_member_expression), Some(literal_null));
        let result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let array_member_expression = ast::generate_getter_selector(&mut nf.f(), ARRAY, false);
        let array_binary_expression = nf.f().binary_expression(IS_TOKEN, Some(result_member_expression), Some(array_member_expression));
        let inner_binary_expression_list = nf.f().list(None, Some(array_binary_expression));
        let inner_binary_expression = nf.f().binary_expression(LOGICALOR_TOKEN, Some(binary_expression), Some(inner_binary_expression_list));
        let result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let proxy_member_expression = ast::generate_getter_selector_qualified(&mut nf.f(), PACKAGE_FLASH_UTILS, PROXY, false);
        let proxy_binary_expression = nf.f().binary_expression(IS_TOKEN, Some(result_member_expression), Some(proxy_member_expression));
        let outer_binary_expression_list = nf.f().list(None, Some(proxy_binary_expression));
        let outer_binary_expression = nf.f().binary_expression(LOGICALOR_TOKEN, Some(inner_binary_expression), Some(outer_binary_expression_list));
        let result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let argument_list = nf.f().argument_list(None, Some(result_member_expression.clone()));
        let literal_array = nf.f().literal_array(Some(argument_list));
        let conditional_expression = nf.f().conditional_expression(Some(outer_binary_expression), Some(result_member_expression), Some(literal_array));
        let return_list = nf.f().list(None, Some(conditional_expression));
        let return_list_list = nf.f().list(None, Some(return_list));
        let return_statement = nf.f().return_statement(Some(return_list_list));
        Some(nf.f().statement_list(Some(result), Some(return_statement)))
    }

    fn generate_source_function_string_conversion(&self, statement_list: Option<NodeRef>, destination_type_name: &str, initializer: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(RESULT));
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), None);
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), initializer);
        let variable_list = nf.f().list(None, Some(variable_binding));
        let variable_definition = nf.f().variable_definition(None, VAR_TOKEN, Some(variable_list));
        let result = nf.f().statement_list(statement_list, Some(variable_definition));
        let result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let undefined_member_expression = ast::generate_getter_selector(&mut nf.f(), UNDEFINED, false);
        let binary_expression = nf.f().binary_expression(EQUALS_TOKEN, Some(result_member_expression), Some(undefined_member_expression));
        let literal_null = nf.f().literal_null_pos(-1);
        let string_identifier = nf.f().identifier_intern(destination_type_name, false);
        let result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let argument_list = nf.f().argument_list(None, Some(result_member_expression));
        let call_expression = ast::call(nf, string_identifier, Some(argument_list), false);
        let string_member_expression = nf.f().member_expression(None, Some(call_expression));
        let conditional_expression = nf.f().conditional_expression(Some(binary_expression), Some(literal_null), Some(string_member_expression));
        let return_list = nf.f().list(None, Some(conditional_expression));
        let return_list_list = nf.f().list(None, Some(return_list));
        let return_statement = nf.f().return_statement(Some(return_list_list));
        Some(nf.f().statement_list(Some(result), Some(return_statement)))
    }

    fn generate_mx_internal_attribute_list(&self) -> NodeRef {
        let nf = self.nf();
        let mx_internal_getter_selector = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_core_package(), MX_INTERNAL);
        let list = nf.f().list(None, Some(mx_internal_getter_selector));
        nf.f().attribute_list(Some(list), None)
    }

    fn generate_mx_internal_qualified_identifier(&self, name: &str, _intern: bool) -> NodeRef {
        let attrs = self.generate_mx_internal_attribute_list();
        self.nf().f().qualified_identifier(Some(attrs), Some(name))
    }

    fn generate_static_property_getter_function(&self) -> NodeRef {
        let nf = self.nf();
        let parameter = ast::generate_parameter(&mut nf.f(), PROPERTY_NAME, STRING, false);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        let base = ast::generate_getter_selector(&mut nf.f(), &self.doc().get_class_name(), true);
        let argument = ast::generate_getter_selector(&mut nf.f(), PROPERTY_NAME, false);
        let argument_list = nf.f().argument_list(None, Some(argument));
        let selector = nf.f().get_expression(Some(argument_list));
        ast::set_mode(&selector, LEFTBRACKET_TOKEN);
        let member_expression = nf.f().member_expression(Some(base), Some(selector));
        let list = nf.f().list(None, Some(member_expression));
        let return_statement = nf.f().return_statement(Some(list));
        let statement_list = nf.f().statement_list(None, Some(return_statement));
        nf.f().function_common(self.base.cx(), None, Some(function_signature), Some(statement_list))
    }

    fn generate_style_declaration_if_statement(&self) -> NodeRef {
        let nf = self.nf();
        let this_expression = nf.f().this_expression(-1);
        let style_declaration_identifier = nf.f().identifier_intern(STYLE_DECLARATION, false);
        let get_expression = nf.f().get_expression(Some(style_declaration_identifier));
        let test_member_expression = nf.f().member_expression(Some(this_expression.clone()), Some(get_expression));
        let unary_expression = nf.f().unary_expression(NOT_TOKEN, Some(test_member_expression));
        let test_list = nf.f().list(None, Some(unary_expression));
        let style_declaration_identifier = nf.f().identifier_intern(STYLE_DECLARATION, false);
        let css_style_declaration_identifier = ast::generate_resolved_identifier(&mut nf.f(), self.sd().get_styles_package(), CSS_STYLE_DECLARATION);
        let literal_null = nf.f().literal_null();
        let argument_list = nf.f().argument_list(None, Some(literal_null));
        let argument_member_expression = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER_INSTANCE, false);
        let argument_list = nf.f().argument_list(Some(argument_list), Some(argument_member_expression));
        let call_expression = ast::call(nf, css_style_declaration_identifier, Some(argument_list), true);
        let argument_member_expression = nf.f().member_expression(None, Some(call_expression));
        let argument_list = nf.f().argument_list(None, Some(argument_member_expression));
        let set_expression = nf.f().set_expression(Some(style_declaration_identifier), Some(argument_list), false);
        let expression_statement = ast::expr_stmt(nf, Some(this_expression), set_expression);
        let then_statement_list = nf.f().statement_list(None, Some(expression_statement));
        nf.f().if_statement(Some(test_list), Some(then_statement_list), None)
    }

    fn generate_style_factory(&self, style_def: &StyleDefRef, declaration: Option<&StyleDeclarationRef>) -> NodeRef {
        let nf = self.nf();
        let function_signature = nf.f().function_signature(None, None);
        ast::void_anno(&function_signature);
        let mut statement_list: Option<NodeRef> = None;
        match declaration {
            Some(declaration) => {
                let blocks: Vec<_> = declaration.borrow().get_declaration_blocks().to_vec();
                for block in blocks {
                    let b = block.borrow();
                    let properties: Vec<Rc<StyleProperty>> = b.get_properties().values().cloned().collect();
                    if b.has_media_list() {
                        let style_prop_statement_list = self.generate_style_properties(None, &properties);
                        let expr = self.generate_style_manager_accept_media_list(&b.get_media_list().unwrap().to_string());
                        let test = nf.f().list(None, Some(expr));
                        let if_statement_node = nf.f().if_statement(Some(test), style_prop_statement_list, None);
                        statement_list = Some(nf.f().statement_list(statement_list, Some(if_statement_node)));
                    } else {
                        statement_list = self.generate_style_properties(statement_list, &properties);
                    }
                }
            }
            None => {
                let properties: Vec<Rc<StyleProperty>> = style_def.borrow().get_styles().values().cloned().collect();
                statement_list = self.generate_style_properties(statement_list, &properties);
            }
        }
        nf.f().function_common(self.base.cx(), None, Some(function_signature), statement_list)
    }

    fn generate_style_properties(&self, statement_list: Option<NodeRef>, properties: &[Rc<StyleProperty>]) -> Option<NodeRef> {
        let nf = self.nf();
        let mut statement_list = statement_list;
        for style_property in properties {
            let text = format!("this.{} = {}", style_property.get_name(), style_property.get_value());
            let node_list = ast::parse_expression(self.base.cx(), &self.base.config_namespaces, &text, style_property.get_line_number(), self.base.generate_doc_comments);
            if let Some(first) = node_list.into_iter().next() {
                statement_list = Some(nf.f().statement_list(statement_list, Some(first)));
            }
        }
        statement_list
    }

    fn generate_style_manager_accept_media_list(&self, media_list: &str) -> NodeRef {
        let nf = self.nf();
        let base = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER_INSTANCE, false);
        let identifier = nf.f().identifier_intern(ACCEPT_MEDIA_LIST, false);
        let literal_string = nf.f().literal_string(media_list);
        let argument_list = nf.f().argument_list(None, Some(literal_string));
        let selector = ast::call(nf, identifier, Some(argument_list), false);
        nf.f().member_expression(Some(base), Some(selector))
    }

    fn generate_styles_init_function(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let doc = self.doc();
        let v = self.generate_styles_init_variable();
        let result = nf.f().statement_list(statement_list, Some(v));
        let function_signature = nf.f().function_signature(None, None);
        ast::void_anno(&function_signature);
        let if_statement = self.generate_styles_init_done_if_statement();
        let mut init = nf.f().statement_list(None, Some(if_statement));
        let v = self.generate_styles_package_variable(STYLE, CSS_STYLE_DECLARATION);
        init = nf.f().statement_list(Some(init), Some(v));
        let v = self.generate_variable(EFFECTS, ARRAY);
        init = nf.f().statement_list(Some(init), Some(v));
        if !doc.get_is_iui_component() {
            let v = self.generate_style_manager_variable_and_init();
            init = nf.f().statement_list(Some(init), Some(v));
        }
        if doc.get_styles_container().is_advanced() {
            let v = self.generate_variable(CONDITIONS, ARRAY);
            init = nf.f().statement_list(Some(init), Some(v));
            let v = self.generate_styles_package_variable(CONDITION, CSS_CONDITION);
            init = nf.f().statement_list(Some(init), Some(v));
            let v = self.generate_styles_package_variable(SELECTOR, CSS_SELECTOR);
            init = nf.f().statement_list(Some(init), Some(v));
        }
        for style_def in doc.get_styles_container().get_style_defs() {
            if style_def.borrow().is_advanced() {
                let declarations: Vec<StyleDeclarationRef> = style_def.borrow().get_declarations().map(|d| d.iter().map(|(_, v)| v.clone()).collect()).unwrap_or_default();
                for style_declaration in declarations {
                    let selector = style_declaration.borrow().get_selector().expect("NullPointerException");
                    let s = self.generate_null_initializer(SELECTOR);
                    init = nf.f().statement_list(Some(init), Some(s));
                    let s = self.generate_null_initializer(CONDITIONS);
                    init = nf.f().statement_list(Some(init), Some(s));
                    self.generate_selector_ancestor(&selector, &init);
                    let selector_text = selector.borrow().to_string();
                    let s = self.generate_get_style_declaration(&selector_text);
                    init = nf.f().statement_list(Some(init), Some(s));
                    let s = self.generate_if_null_style_declaration_selector();
                    init = nf.f().statement_list(Some(init), Some(s));
                    if style_declaration.borrow().has_properties() {
                        let s = self.generate_if_null_style_factory(&style_def, Some(&style_declaration));
                        init = nf.f().statement_list(Some(init), Some(s));
                    }
                    if style_declaration.borrow().has_effect_styles() {
                        let s = self.generate_effects_initializer();
                        init = nf.f().statement_list(Some(init), Some(s));
                        self.generate_if_null_effects_and_pushes(&style_def, Some(&style_declaration), &init);
                    }
                }
            } else {
                let (is_type_selector, subject) = {
                    let s = style_def.borrow();
                    (s.is_type_selector(), s.get_subject().to_string())
                };
                let s = if is_type_selector { self.generate_get_style_declaration(&subject) } else { self.generate_get_style_declaration(&format!(".{subject}")) };
                init = nf.f().statement_list(Some(init), Some(s));
                let s = self.generate_if_null_style_declaration(&style_def);
                init = nf.f().statement_list(Some(init), Some(s));
                if !style_def.borrow().get_styles().is_empty() {
                    let s = self.generate_if_null_style_factory(&style_def, None);
                    init = nf.f().statement_list(Some(init), Some(s));
                }
                if !style_def.borrow().get_effect_styles().is_empty() {
                    let s = self.generate_effects_initializer();
                    init = nf.f().statement_list(Some(init), Some(s));
                    self.generate_if_null_effects_and_pushes(&style_def, None, &init);
                }
            }
        }
        if doc.get_is_flex_application() {
            let s = self.generate_init_proto_chain_roots();
            init = nf.f().statement_list(Some(init), Some(s));
        }
        let function_common = nf.f().function_common(cx, None, Some(function_signature), Some(init));
        ast::user_defined_body(&function_common);
        let attribute_list = self.base.generate_mx_internal_attribute();
        let styles_init_identifier = nf.f().identifier(&format!("_{}_StylesInit", doc.get_class_name()));
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(styles_init_identifier));
        let function_definition = nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common));
        nf.f().statement_list(Some(result), Some(function_definition))
    }

    fn generate_styles_init_done_if_statement(&self) -> NodeRef {
        let nf = self.nf();
        let styles_init_done = format!("_{}_StylesInit_done", self.doc().get_class_name());
        let test_getter_selector = self.generate_mx_internal_getter_selector(&styles_init_done, false);
        let test_list = nf.f().list(None, Some(test_getter_selector));
        let return_statement = nf.f().return_statement(None);
        let then_statement_list = nf.f().statement_list(None, Some(return_statement));
        let qualifier = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_core_package(), MX_INTERNAL);
        let qualified_identifier = nf.f().qualified_identifier(Some(qualifier), Some(&styles_init_done));
        let literal_boolean = nf.f().literal_boolean(true);
        let argument_list = nf.f().argument_list(None, Some(literal_boolean));
        let set_expression = nf.f().set_expression(Some(qualified_identifier), Some(argument_list), false);
        let expression_statement = ast::expr_stmt(nf, None, set_expression);
        let else_statement_list = nf.f().statement_list(None, Some(expression_statement));
        nf.f().if_statement(Some(test_list), Some(then_statement_list), Some(else_statement_list))
    }

    fn generate_styles_init_variable(&self) -> NodeRef {
        let nf = self.nf();
        let attribute_list = self.generate_mx_internal_attribute_list();
        let qualified_identifier = self.generate_mx_internal_qualified_identifier(&format!("_{}_StylesInit_done", self.doc().get_class_name()), true);
        let member_expression = ast::generate_getter_selector(&mut nf.f(), BOOLEAN, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let literal_boolean = nf.f().literal_boolean(false);
        let variable_binding = nf.f().variable_binding(Some(attribute_list.clone()), VAR_TOKEN, Some(typed_identifier), Some(literal_boolean));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(Some(attribute_list), VAR_TOKEN, Some(list))
    }

    fn generate_target_variable(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(TARGET));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), OBJECT, true);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let this_expression = nf.f().this_expression(-1);
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(this_expression));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn generate_two_way_counterpart_assignment(&self, left_value_id: i32, right_value_id: i32) -> NodeRef {
        let nf = self.nf();
        let left_result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let left_literal_number = nf.f().literal_number_int(left_value_id);
        let left_get_expression_argument_list = nf.f().argument_list(None, Some(left_literal_number));
        let left_get_expression = nf.f().get_expression(Some(left_get_expression_argument_list));
        ast::set_mode(&left_get_expression, LEFTBRACKET_TOKEN);
        let base = nf.f().member_expression(Some(left_result_member_expression), Some(left_get_expression));
        let two_way_counterpart_identifier = nf.f().identifier_intern(TWO_WAY_COUNTERPART, false);
        let right_result_member_expression = ast::generate_getter_selector(&mut nf.f(), RESULT, false);
        let right_literal_number = nf.f().literal_number_int(right_value_id);
        let right_get_expression_argument_list = nf.f().argument_list(None, Some(right_literal_number));
        let right_get_expression = nf.f().get_expression(Some(right_get_expression_argument_list));
        ast::set_mode(&right_get_expression, LEFTBRACKET_TOKEN);
        let right_value_member_expression = nf.f().member_expression(Some(right_result_member_expression), Some(right_get_expression));
        let set_expression_argument_list = nf.f().argument_list(None, Some(right_value_member_expression));
        let selector = nf.f().set_expression(Some(two_way_counterpart_identifier), Some(set_expression_argument_list), false);
        ast::expr_stmt(nf, Some(base), selector)
    }

    fn generate_type_import_dummies(&self, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = self.nf();
        let mut result = statement_list;
        for (index, type_ref) in self.doc().get_type_refs().iter().enumerate() {
            let type_expression = ast::generate_type_expression(&mut nf.f(), type_ref, true);
            let variable_definition = ast::generate_private_variable(&mut nf.f(), type_expression, &format!("_typeRef{}", index + 1));
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
        }
        result
    }

    fn generate_watchers_assignment(&self) -> NodeRef {
        let nf = self.nf();
        let mx_internal_getter_selector = ast::generate_resolved_getter_selector(&mut nf.f(), self.sd().get_core_package(), MX_INTERNAL);
        let qualified_identifier = ast::generate_qualified_identifier_member(&mut nf.f(), mx_internal_getter_selector, _WATCHERS, false);
        let rvalue_base = self.generate_mx_internal_getter_selector(_WATCHERS, false);
        let concat_identifier = nf.f().identifier_intern(CONCAT, false);
        let watchers_member_expression = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let concat_argument_list = nf.f().argument_list(None, Some(watchers_member_expression));
        let call_expression = ast::call(nf, concat_identifier, Some(concat_argument_list), false);
        let argument = nf.f().member_expression(Some(rvalue_base), Some(call_expression));
        let argument_list = nf.f().argument_list(None, Some(argument));
        let set_expression = nf.f().set_expression(Some(qualified_identifier), Some(argument_list), false);
        ast::expr_stmt(nf, None, set_expression)
    }

    fn generate_watchers_variable(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(WATCHERS));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), ARRAY, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let literal_array = nf.f().literal_array(None);
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(literal_array));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn generate_watcher_setup_util_class_variable(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(WATCHER_SETUP_UTIL_CLASS));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), OBJECT, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let get_definition_by_name_identifier = nf.f().identifier_intern(GET_DEFINITION_BY_NAME, false);
        let literal_string = nf.f().literal_string(&self.doc().get_watcher_setup_util_class_name());
        let argument_list = nf.f().argument_list(None, Some(literal_string));
        let call_expression = ast::call(nf, get_definition_by_name_identifier, Some(argument_list), false);
        let initializer = nf.f().member_expression(None, Some(call_expression));
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(initializer));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    fn get_path(&self) -> String {
        self.doc().get_source_path()
    }

    fn generate_module_factory_initialized_variable(&self) -> NodeRef {
        let nf = self.nf();
        let member_expression = ast::generate_getter_selector(&mut nf.f(), BOOLEAN, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let literal_boolean = nf.f().literal_boolean(false);
        ast::generate_private_variable_init(&mut nf.f(), Some(type_expression), __MODULE_FACTORY_INITIALIZED, Some(literal_boolean))
    }

    fn generate_module_factory_property_override(&self, statement_list: Option<NodeRef>) -> NodeRef {
        let nf = self.nf();
        let cx = self.base.cx();
        let parameter = ast::generate_parameter(&mut nf.f(), FACTORY, IFLEX_MODULE_FACTORY, true);
        let constructor_parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(constructor_parameter_list), None);
        ast::void_anno(&function_signature);
        let member_expression = ast::generate_getter_selector(&mut nf.f(), FACTORY, false);
        let super_expression = nf.f().super_expression(None, -1);
        let expression_statement = ast::generate_assignment_base(&mut nf.f(), Some(super_expression), MODULE_FACTORY, Some(member_expression));
        let init_statement_list = nf.f().statement_list(None, Some(expression_statement));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), __MODULE_FACTORY_INITIALIZED, false);
        let list_node = nf.f().list(None, Some(member_expression));
        let return_statement = nf.f().return_statement(None);
        let if_statement = nf.f().if_statement(Some(list_node), Some(return_statement), None);
        let init_statement_list = nf.f().statement_list(Some(init_statement_list), Some(if_statement));
        let literal_boolean = nf.f().literal_boolean(true);
        let expression_statement = ast::generate_assignment_base(&mut nf.f(), None, __MODULE_FACTORY_INITIALIZED, Some(literal_boolean));
        let init_statement_list = nf.f().statement_list(Some(init_statement_list), Some(expression_statement));
        let statement_list = nf.f().statement_list_concat(Some(init_statement_list), statement_list);
        let attribute_list = ast::generate_override_public_attribute(&mut nf.f());
        let identifier = nf.f().qualified_identifier(Some(attribute_list.clone()), Some(MODULE_FACTORY));
        let function_common = nf.f().function_common(cx, Some(identifier.clone()), Some(function_signature), Some(statement_list));
        ast::user_defined_body(&function_common);
        let function_name = nf.f().function_name(SET_TOKEN, Some(identifier));
        nf.f().function_definition(cx, Some(attribute_list), Some(function_name), Some(function_common))
    }

    fn generate_style_manager_variable_and_init(&self) -> NodeRef {
        let nf = self.nf();
        let qualified_identifier = nf.f().qualified_identifier(None, Some(STYLE_MANAGER_INSTANCE));
        let member_expression = ast::generate_getter_selector(&mut nf.f(), ISTYLE_MANAGER2, false);
        let type_expression = nf.f().type_expression(Some(member_expression), true, false, -1);
        let typed_identifier = nf.f().typed_identifier(Some(qualified_identifier), Some(type_expression));
        let base = ast::generate_getter_selector(&mut nf.f(), STYLE_MANAGER, false);
        let args = if self.doc().get_is_iflex_module() {
            let m = ast::generate_getter_selector(&mut nf.f(), MODULE_FACTORY, false);
            nf.f().argument_list(None, Some(m))
        } else {
            let n = nf.f().literal_null();
            nf.f().argument_list(None, Some(n))
        };
        let identifier = nf.f().identifier(GET_STYLE_MANAGER);
        let selector = ast::call(nf, identifier, Some(args), false);
        let member_expression = nf.f().member_expression(Some(base), Some(selector));
        let variable_binding = nf.f().variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(member_expression));
        let list = nf.f().list(None, Some(variable_binding));
        nf.f().variable_definition(None, VAR_TOKEN, Some(list))
    }

    pub fn get_context(&self) -> crate::asc::util::context::ContextRef {
        self.base.get_context()
    }

    pub fn get_syntax_tree(&self) -> Option<NodeRef> {
        self.base.get_syntax_tree()
    }
}

/// `addDescriptorInitializerFragments(nodeFactory, configNamespaces, generateDocComments, model, includePropNames, includeDesignLayer)`.
pub fn add_descriptor_initializer_fragments(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, model: &ModelRef, include_prop_names: Option<&HashSet<String>>, include_design_layer: bool) -> NodeRef {
    add_descriptor_initializer_fragments_props_only(nf, config_namespaces, generate_doc_comments, model, include_prop_names, false, include_design_layer)
}

fn add_descriptor_initializer_fragments_props_only(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, model: &ModelRef, include_prop_names: Option<&HashSet<String>>, props_only: bool, include_design_layer: bool) -> NodeRef {
    model.set_described(true);
    let mx_core_literal_string = nf.f().literal_string_intern(model.get_standard_defs().get_core_package(), false);
    let ui_component_descriptor_qualified_identifier = nf.f().qualified_identifier(Some(mx_core_literal_string), Some(UI_COMPONENT_DESCRIPTOR));
    let type_identifier = nf.f().identifier_intern(TYPE, false);
    let model_type_name = model.get_type().get_name().to_string();
    let package_name = name_formatter::retrieve_package_name(&model_type_name);
    let class_name = name_formatter::retrieve_class_name(&model_type_name);
    let position = ast::line_number_to_position(&mut nf.f(), model.get_xml_line_number());
    let model_type_identifier = if package_name.is_empty() {
        nf.f().identifier_intern(&class_name, false)
    } else {
        let package_name_literal_string = nf.f().literal_string(&package_name);
        nf.f().qualified_identifier(Some(package_name_literal_string), Some(&class_name))
    };
    let type_get_expression = nf.f().get_expression_pos(Some(model_type_identifier), position);
    let type_member_expression = nf.f().member_expression(None, Some(type_get_expression));
    let type_literal_field = nf.f().literal_field(Some(type_identifier), Some(type_member_expression));
    let mut argument_list = nf.f().argument_list(None, Some(type_literal_field));
    if model.is_declared() {
        let id_identifier = nf.f().identifier_intern(ID, false);
        let id_literal_string = nf.f().literal_string(&model.get_id().unwrap_or_default());
        let id_literal_field = nf.f().literal_field(Some(id_identifier), Some(id_literal_string));
        argument_list = nf.f().argument_list(Some(argument_list), Some(id_literal_field));
    }
    if !props_only {
        argument_list = add_descriptor_events(nf, argument_list, model);
    }
    if !props_only {
        argument_list = add_descriptor_effect_names(nf, argument_list, model);
    }
    if !props_only {
        argument_list = add_descriptor_styles_and_effects(nf, config_namespaces, generate_doc_comments, argument_list, model);
    }
    argument_list = add_descriptor_properties(nf, config_namespaces, generate_doc_comments, argument_list, model, include_prop_names, include_design_layer);
    let literal_object = nf.f().literal_object(Some(argument_list));
    let ui_component_descriptor_argument_list = nf.f().argument_list(None, Some(literal_object));
    let call_expression = ast::call(nf, ui_component_descriptor_qualified_identifier, Some(ui_component_descriptor_argument_list), true);
    nf.f().member_expression(None, Some(call_expression))
}

fn add_descriptor_properties(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, argument_list: NodeRef, model: &ModelRef, include_prop_names: Option<&HashSet<String>>, include_design_layer: bool) -> NodeRef {
    let mut argument_list = argument_list;
    let props: Vec<_> = model.get_property_initializer_iterator_bindings(false).into_iter().filter(|i| include_prop_names.is_none_or(|n| n.contains(&i.get_name()))).collect();
    let children = if model.is_movie_clip() && model.has_children() { model.children() } else { Vec::new() };
    let layer_parent = model.layer_parent.borrow().clone();
    let sd = model.get_standard_defs();
    let has_design_layer = include_design_layer && layer_parent.is_some() && model.get_type().is_assignable_to_name(Some(&sd.interface_ivisualelement));
    if !props.is_empty() || !children.is_empty() || has_design_layer {
        let cx = nf.f().get_context();
        let properties_factory_identifier = nf.f().identifier_intern(PROPERTIES_FACTORY, false);
        let object_member_expression = ast::generate_getter_selector(&mut nf.f(), OBJECT, false);
        let type_expression = nf.f().type_expression(Some(object_member_expression), true, false, -1);
        let function_signature = nf.f().function_signature(None, Some(type_expression));
        let mut properties_factory_argument_list: Option<NodeRef> = None;
        for init in &props {
            if !init.is_state_specific() {
                let property_identifier = nf.f().identifier(&init.get_name());
                let value_expr = init.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                let property_literal_field = nf.f().literal_field(Some(property_identifier), Some(value_expr));
                properties_factory_argument_list = Some(nf.f().argument_list(properties_factory_argument_list, Some(property_literal_field)));
            }
        }
        if has_design_layer && model.get_type().is_assignable_to_name(Some(&sd.interface_ivisualelement)) {
            let layer_property_identifier = nf.f().identifier_intern(DESIGN_LAYER, false);
            let id = layer_parent.as_ref().and_then(|l| l.get_id()).unwrap_or_default();
            let member_expression = ast::generate_getter_selector(&mut nf.f(), &id, true);
            let layer_literal_field = nf.f().literal_field(Some(layer_property_identifier), Some(member_expression));
            properties_factory_argument_list = Some(nf.f().argument_list(properties_factory_argument_list, Some(layer_literal_field)));
        }
        if !children.is_empty() {
            let child_descriptors_identifier = nf.f().identifier_intern(CHILD_DESCRIPTORS, false);
            let mut child_descriptors_argument_list: Option<NodeRef> = None;
            for init in &children {
                let child = init.as_value().and_then(|v| v.get_model()).expect("ClassCastException");
                if child.is_descriptor_init() {
                    let member_expression = add_descriptor_initializer_fragments(nf, config_namespaces, generate_doc_comments, &child, None, true);
                    child_descriptors_argument_list = Some(nf.f().argument_list(child_descriptors_argument_list, Some(member_expression)));
                }
            }
            let literal_array = nf.f().literal_array(child_descriptors_argument_list);
            let child_descriptors_literal_field = nf.f().literal_field(Some(child_descriptors_identifier), Some(literal_array));
            properties_factory_argument_list = Some(nf.f().argument_list(properties_factory_argument_list, Some(child_descriptors_literal_field)));
        }
        let literal_object = nf.f().literal_object(properties_factory_argument_list);
        let list = nf.f().list(None, Some(literal_object));
        let return_statement = nf.f().return_statement(Some(list));
        let body = nf.f().statement_list(None, Some(return_statement));
        let function_common = nf.f().function_common(&cx, None, Some(function_signature), Some(body));
        ast::user_defined_body(&function_common);
        let properties_factory_literal_field = nf.f().literal_field(Some(properties_factory_identifier), Some(function_common));
        argument_list = nf.f().argument_list(Some(argument_list), Some(properties_factory_literal_field));
    }
    argument_list
}

fn add_descriptor_styles_and_effects(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, argument_list: NodeRef, model: &ModelRef) -> NodeRef {
    let inits = model.get_style_and_effect_initializer_iterator();
    if inits.is_empty() {
        return argument_list;
    }
    let cx = nf.f().get_context();
    let styles_factory_identifier = nf.f().identifier_intern(STYLES_FACTORY, false);
    let function_signature = nf.f().function_signature(None, None);
    ast::void_anno(&function_signature);
    let mut body: Option<NodeRef> = None;
    for init in &inits {
        let this_expression = nf.f().this_expression(-1);
        let identifier = nf.f().identifier(&init.get_name());
        let value_expr = init.generate_value_expr(nf, config_namespaces, generate_doc_comments);
        let style_and_effect_argument_list = nf.f().argument_list(None, Some(value_expr));
        let set_expression = nf.f().set_expression(Some(identifier), Some(style_and_effect_argument_list), false);
        let expression_statement = ast::expr_stmt(nf, Some(this_expression), set_expression);
        body = Some(nf.f().statement_list(body, Some(expression_statement)));
    }
    let function_common = nf.f().function_common(&cx, None, Some(function_signature), body);
    ast::user_defined_body(&function_common);
    let styles_factory_literal_field = nf.f().literal_field(Some(styles_factory_identifier), Some(function_common));
    nf.f().argument_list(Some(argument_list), Some(styles_factory_literal_field))
}

fn add_descriptor_effect_names(nf: &Nf, argument_list: NodeRef, model: &ModelRef) -> NodeRef {
    let effects = model.get_effects();
    if effects.is_empty() {
        return argument_list;
    }
    let effects_identifier = nf.f().identifier_intern(EFFECTS, false);
    let mut effects_argument_list: Option<NodeRef> = None;
    for (_, effect_initializer) in &effects {
        let literal_string = nf.f().literal_string(&effect_initializer.get_name());
        effects_argument_list = Some(nf.f().argument_list(effects_argument_list, Some(literal_string)));
    }
    let literal_array = nf.f().literal_array(effects_argument_list);
    let literal_field = nf.f().literal_field(Some(effects_identifier), Some(literal_array));
    nf.f().argument_list(Some(argument_list), Some(literal_field))
}

fn add_descriptor_events(nf: &Nf, argument_list: NodeRef, model: &ModelRef) -> NodeRef {
    let events = model.get_event_initializer_iterator();
    if events.is_empty() {
        return argument_list;
    }
    let events_identifier = nf.f().identifier_intern(EVENTS, false);
    let mut events_argument_list: Option<NodeRef> = None;
    for init in &events {
        let identifier = nf.f().identifier(&init.get_name());
        let literal_string = nf.f().literal_string(&init.get_value_expr());
        let literal_field = nf.f().literal_field(Some(identifier), Some(literal_string));
        events_argument_list = Some(nf.f().argument_list(events_argument_list, Some(literal_field)));
    }
    let events_literal_object = nf.f().literal_object(events_argument_list);
    let events_literal_field = nf.f().literal_field(Some(events_identifier), Some(events_literal_object));
    nf.f().argument_list(Some(argument_list), Some(events_literal_field))
}

/// `getDescriptorInitializerFragments(nodeFactory, configNamespaces, generateDocComments, model)`; unlike the text path
/// it keeps events, effects and styles, as its `true` is the public overload's `includeDesignLayer`.
fn get_descriptor_initializer_fragments(nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, model: &ModelRef) -> NodeRef {
    let required = deps::required_top_level_descriptor_properties();
    add_descriptor_initializer_fragments(nf, config_namespaces, generate_doc_comments, model, Some(&required), true)
}

/// `BindingExpression.generateNamespaceDeclarations(context, statementList)`.
pub fn generate_namespace_declarations_of(binding_expression: &BindingExpression, context: &crate::asc::util::context::ContextRef, statement_list: Option<NodeRef>) -> Option<NodeRef> {
    match binding_expression.get_namespaces() {
        Some(ns) => generate_namespace_declarations(&ns, context, statement_list),
        None => statement_list,
    }
}

/// `BindingExpression.generateNamespaceDeclarations(map, context, statementList)`: `var nsK:Namespace = new
/// Namespace(uri)` per entry, in `HashMap<Integer, String>` key order.
pub fn generate_namespace_declarations(map: &[(i32, String)], context: &crate::asc::util::context::ContextRef, statement_list: Option<NodeRef>) -> Option<NodeRef> {
    let mut result = statement_list;
    let nf_owned = Nf::of(context);
    let nf = &nf_owned;
    for (k, uri) in super::rep::binding_expression::integer_hash_map_order(map) {
        let identifier = nf.f().identifier_intern("Namespace", false);
        let literal_string = nf.f().literal_string(&uri);
        let argument_list = nf.f().argument_list(None, Some(literal_string));
        let call_expression = ast::call(nf, identifier, Some(argument_list), true);
        let initializer = nf.f().member_expression(None, Some(call_expression));
        let variable_definition = ast::generate_variable(&mut nf.f(), &format!("ns{k}"), "Namespace", false, Some(initializer));
        result = Some(nf.f().statement_list(result, Some(variable_definition)));
    }
    result
}

mod jstr_key {
    /// A `TreeSet<String>` element: ordered by Java `String.compareTo`.
    #[derive(PartialEq, Eq)]
    pub struct Key(pub String);

    impl PartialOrd for Key {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for Key {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            super::jstr::compare(&self.0, &other.0)
        }
    }
}
