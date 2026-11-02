//! The direct AST generation half of `flex2.compiler.as3.binding.DataBindingExtension`: the `WatcherSetupUtil`
//! class as an ASC syntax tree.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::data_binding_info::DataBindingInfo;
use super::watcher::{PropertyKind, WatcherKind, WatcherRef};
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::flex2::compiler::as3::as3_compiler;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::StandardDefs;
use crate::asc::flex2::compiler::source::{Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::{DOT_TOKEN, EMPTY_TOKEN, LEFTBRACKET_TOKEN};
use crate::asc::util::context::{ConfigVar, ContextRef};

const ADD_CHILD: &str = "addChild";
const APPLY: &str = "apply";
const ARRAY: &str = "Array";
const ARRAY_ELEMENT_WATCHER: &str = "ArrayElementWatcher";
const ARRAY_WATCHER: &str = "arrayWatcher";
const BINDINGS: &str = "bindings";
const EXCLUDE_CLASS: &str = "ExcludeClass";
const FBS: &str = "fbs";
const FUNCTION: &str = "Function";
const FUNCTION_RETURN_WATCHER: &str = "FunctionReturnWatcher";
const IFLEX_MODULE_FACTORY: &str = "IFlexModuleFactory";
const INIT: &str = "init";
const IWATCHER_SETUP_UTIL2: &str = "IWatcherSetupUtil2";
const OBJECT: &str = "Object";
const PARENT_WATCHER: &str = "parentWatcher";
const PROPERTY_GETTER: &str = "propertyGetter";
const PROPERTY_WATCHER: &str = "PropertyWatcher";
const REPEATER_COMPONENT_WATCHER: &str = "RepeaterComponentWatcher";
const REPEATER_ITEM_WATCHER: &str = "RepeaterItemWatcher";
const SETUP: &str = "setup";
const STATIC_PROPERTY_GETTER: &str = "staticPropertyGetter";
const STATIC_PROPERTY_WATCHER: &str = "StaticPropertyWatcher";
const TARGET: &str = "target";
const UPDATE_PARENT: &str = "updateParent";
const WATCHERS: &str = "watchers";
const WATCHER_SETUP_UTIL: &str = "watcherSetupUtil";
const XML_WATCHER: &str = "XMLWatcher";

struct Gen<'a> {
    nf: Nf,
    cx: ContextRef,
    config_namespaces: &'a HashSet<String>,
    standard_defs: Rc<StandardDefs>,
}

impl Gen<'_> {
    fn generate_accessor_function_array_element(&self, watcher: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let function_signature = nf.f().function_signature(None, None);
        let node_list = ast::parse_expression_text(&self.cx, self.config_namespaces, &watcher.borrow().get_evaluation_part());
        let list = node_list.first().map(|n| n.borrow().as_expression_statement().expect("ClassCastException").expr.clone().expect("NullPointerException"));
        let return_statement = nf.f().return_statement(list);
        let statement_list = nf.f().statement_list(None, Some(return_statement));
        nf.f().function_common(&self.cx, None, Some(function_signature), Some(statement_list))
    }

    fn generate_accessor_function_function_return(&self, watcher: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let member_expression = ast::generate_getter_selector(&mut nf.f(), ARRAY, false);
        let return_type = nf.f().type_expression(Some(member_expression), true, false, -1);
        let function_signature = nf.f().function_signature(None, Some(return_type));
        let node_list = ast::parse_expression_text(&self.cx, self.config_namespaces, &watcher.borrow().get_evaluation_part());
        let mut argument_list: Option<NodeRef> = None;
        if let Some(first) = node_list.first() {
            let list = first.borrow().as_expression_statement().expect("ClassCastException").expr.clone().expect("NullPointerException");
            let items: Vec<Option<NodeRef>> = list.borrow().list().expect("ClassCastException").items.clone();
            for item in items {
                argument_list = Some(nf.f().argument_list(argument_list, item));
            }
        }
        let literal_array = nf.f().literal_array(argument_list);
        let outer_list = nf.f().list(None, Some(literal_array));
        let return_statement = nf.f().return_statement(Some(outer_list));
        let statement_list = nf.f().statement_list(None, Some(return_statement));
        nf.f().function_common(&self.cx, None, Some(function_signature), Some(statement_list))
    }

    fn generate_add_child(&self, watcher: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let watchers_member_expression = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let parent_id = watcher.borrow().get_parent().expect("NullPointerException").borrow().get_id();
        let get_expression = self.generate_array_index(parent_id);
        ast::set_mode(&get_expression, LEFTBRACKET_TOKEN);
        let base = nf.f().member_expression(Some(watchers_member_expression), Some(get_expression));
        let identifier = nf.f().identifier_intern(ADD_CHILD, false);
        let child_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let child_selector = self.generate_array_index(watcher.borrow().get_id());
        ast::set_mode(&child_selector, LEFTBRACKET_TOKEN);
        let child_member_expression = nf.f().member_expression(Some(child_base), Some(child_selector));
        let args = nf.f().argument_list(None, Some(child_member_expression));
        let selector = ast::call(nf, identifier, Some(args), false);
        ast::expr_stmt(nf, Some(base), selector)
    }

    fn generate_array_index(&self, index: i32) -> NodeRef {
        let literal_number = self.nf.f().literal_number_int(index);
        let argument_list = self.nf.f().argument_list(None, Some(literal_number));
        self.nf.f().get_expression(Some(argument_list))
    }

    fn generate_change_events(&self, watcher: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let events = watcher.borrow().get_change_events();
        if events.is_empty() {
            return nf.f().literal_null();
        }
        let mut argument_list: Option<NodeRef> = None;
        for change_event in &events {
            let identifier_node = nf.f().identifier(change_event.get_name());
            let literal_boolean = nf.f().literal_boolean(change_event.get_committing());
            let literal_field = nf.f().literal_field(Some(identifier_node), Some(literal_boolean));
            argument_list = Some(nf.f().argument_list(argument_list, Some(literal_field)));
        }
        nf.f().literal_object(argument_list)
    }

    fn generate_class_definition(&self, name: &str, info: &Rc<RefCell<DataBindingInfo>>) -> NodeRef {
        let nf = &self.nf;
        nf.f().start_class_defs();
        let iwatcher_setup_util_member_expression = ast::generate_getter_selector_qualified(&mut nf.f(), self.standard_defs.get_binding_package(), IWATCHER_SETUP_UTIL2, false);
        let interfaces = nf.f().list(None, Some(iwatcher_setup_util_member_expression));
        let inheritance = nf.f().inheritance(None, Some(interfaces));
        let constructor_function_definition = ast::generate_constructor(&self.cx, name, None, true, None, -1);
        let mut statement_list = nf.f().statement_list(None, Some(constructor_function_definition));
        let init_function_common = self.generate_init_function_common(info);
        ast::user_defined_body(&init_function_common);
        let init_function_definition = self.generate_init_function_definition(init_function_common);
        statement_list = nf.f().statement_list(Some(statement_list), Some(init_function_definition));
        let setup_function_common = self.generate_setup_function_common(info);
        ast::user_defined_body(&setup_function_common);
        let setup_function_definition = self.generate_setup_function_definition(setup_function_common);
        statement_list = nf.f().statement_list(Some(statement_list), Some(setup_function_definition));
        let attrs = ast::generate_public_attribute(&mut nf.f());
        let qid = ast::generate_public_qualified_identifier(&mut nf.f(), name);
        nf.f().class_definition(&self.cx, Some(attrs), Some(qid), Some(inheritance), Some(statement_list))
    }

    fn generate_evaluation_watcher_part(&self, watcher: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let watchers_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let watchers_selector = self.generate_array_index(watcher.borrow().get_id());
        let watcher_base = nf.f().member_expression(Some(watchers_base), Some(watchers_selector.clone()));
        ast::set_mode(&watchers_selector, LEFTBRACKET_TOKEN);
        let parent_variable_identifier = match &watcher.borrow().kind {
            WatcherKind::ArrayElement(_) => nf.f().identifier_intern(ARRAY_WATCHER, false),
            WatcherKind::FunctionReturn { .. } => nf.f().identifier_intern(PARENT_WATCHER, false),
            _ => panic!("Unhandled EvaluationWatcher type"),
        };
        let parent_watchers_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let parent_id = watcher.borrow().get_parent().expect("NullPointerException").borrow().get_id();
        let parent_watchers_selector = self.generate_array_index(parent_id);
        ast::set_mode(&parent_watchers_selector, LEFTBRACKET_TOKEN);
        let parent_watcher_base = nf.f().member_expression(Some(parent_watchers_base), Some(parent_watchers_selector));
        let argument_list = nf.f().argument_list(None, Some(parent_watcher_base));
        let set_expression = nf.f().set_expression(Some(parent_variable_identifier), Some(argument_list), false);
        ast::set_mode(&set_expression, DOT_TOKEN);
        ast::expr_stmt(nf, Some(watcher_base), set_expression)
    }

    fn generate_init_function_common(&self, info: &Rc<RefCell<DataBindingInfo>>) -> NodeRef {
        let nf = &self.nf;
        let parameter = ast::generate_parameter(&mut nf.f(), FBS, IFLEX_MODULE_FACTORY, false);
        let parameter_list = nf.f().parameter_list(None, Some(parameter));
        let function_signature = nf.f().function_signature(Some(parameter_list), None);
        ast::void_anno(&function_signature);
        let class_name = info.borrow().get_class_name().expect("NullPointerException");
        let import_directive = ast::generate_import(&self.cx, &class_name);
        let statement_list = nf.f().statement_list(None, Some(import_directive));
        let base = match class_name.rfind('.').filter(|&i| i > 0) {
            Some(index) => {
                let s = ast::generate_getter_selector_qualified(&mut nf.f(), &class_name[..index], &class_name[index + 1..], true);
                nf.f().list(None, Some(s))
            }
            None => {
                let s = ast::generate_getter_selector(&mut nf.f(), &class_name, true);
                nf.f().list(None, Some(s))
            }
        };
        let watcher_setup_util_identifier = nf.f().identifier_intern(WATCHER_SETUP_UTIL, false);
        let watcher_setup_util_class_identifier = nf.f().identifier(&info.borrow().get_watcher_setup_util_class_name().expect("NullPointerException"));
        let call_expression = ast::call(nf, watcher_setup_util_class_identifier, None, true);
        let new_member_expression = nf.f().member_expression(None, Some(call_expression));
        let argument_list = nf.f().argument_list(None, Some(new_member_expression));
        let selector = nf.f().set_expression(Some(watcher_setup_util_identifier), Some(argument_list), false);
        let expression_statement = ast::expr_stmt(nf, Some(base), selector);
        let statement_list = nf.f().statement_list(Some(statement_list), Some(expression_statement));
        nf.f().function_common(&self.cx, None, Some(function_signature), Some(statement_list))
    }

    fn generate_init_function_definition(&self, function_common: NodeRef) -> NodeRef {
        let nf = &self.nf;
        let attribute_list = ast::generate_public_static_attribute(&mut nf.f());
        let identifier = nf.f().identifier_intern(INIT, false);
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(identifier));
        nf.f().function_definition(&self.cx, Some(attribute_list), Some(function_name), Some(function_common))
    }

    fn generate_listener(&self, id: i32) -> NodeRef {
        let nf = &self.nf;
        let base = ast::generate_getter_selector(&mut nf.f(), BINDINGS, false);
        let selector = self.generate_array_index(id);
        ast::set_mode(&selector, LEFTBRACKET_TOKEN);
        let member_expression = nf.f().member_expression(Some(base), Some(selector));
        let argument_list = nf.f().argument_list(None, Some(member_expression));
        nf.f().literal_array(Some(argument_list))
    }

    fn generate_listeners(&self, watcher: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let mut argument_list: Option<NodeRef> = None;
        let binding_expressions: Vec<_> = watcher.borrow().get_binding_expressions().map(|b| b.to_vec()).unwrap_or_default();
        for binding_expression in binding_expressions {
            let base = ast::generate_getter_selector(&mut nf.f(), BINDINGS, false);
            let selector = self.generate_array_index(binding_expression.get_id());
            ast::set_mode(&selector, LEFTBRACKET_TOKEN);
            let member_expression = nf.f().member_expression(Some(base), Some(selector));
            argument_list = Some(nf.f().argument_list(argument_list, Some(member_expression)));
        }
        nf.f().literal_array(argument_list)
    }

    fn generate_path(&self, path: &str) -> NodeRef {
        let (string, base) = match path.rfind('.').filter(|&i| i > 0) {
            Some(index) => (&path[index + 1..], Some(self.generate_path(&path[..index]))),
            None => (path, None),
        };
        let identifier = self.nf.f().identifier(string);
        let get_expression = self.nf.f().get_expression(Some(identifier));
        ast::set_mode(&get_expression, DOT_TOKEN);
        self.nf.f().member_expression(base, Some(get_expression))
    }

    fn generate_root_watcher_bottom(&self, statement_list: Option<NodeRef>, watcher: &WatcherRef) -> Option<NodeRef> {
        let nf = &self.nf;
        let mut result = statement_list;
        let watchers_member_expression = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let get_expression = self.generate_array_index(watcher.borrow().get_id());
        ast::set_mode(&get_expression, LEFTBRACKET_TOKEN);
        let base = nf.f().member_expression(Some(watchers_member_expression), Some(get_expression));
        let identifier = nf.f().identifier_intern(UPDATE_PARENT, false);
        let class_name = watcher.borrow().get_class_name();
        let parent_member_expression = match class_name {
            Some(class_name) => {
                let import = ast::generate_import(&self.cx, &class_name);
                result = Some(nf.f().statement_list(result, Some(import)));
                match class_name.rfind('.').filter(|&i| i > 0) {
                    Some(index) => ast::generate_getter_selector_qualified(&mut nf.f(), &class_name[..index], &class_name[index + 1..], true),
                    None => ast::generate_getter_selector(&mut nf.f(), &class_name, true),
                }
            }
            None => ast::generate_getter_selector(&mut nf.f(), TARGET, false),
        };
        let args = nf.f().argument_list(None, Some(parent_member_expression));
        let selector = ast::call(nf, identifier, Some(args), false);
        let expression_statement = ast::expr_stmt(nf, Some(base), selector);
        Some(nf.f().statement_list(result, Some(expression_statement)))
    }

    fn generate_setup_function_common(&self, info: &Rc<RefCell<DataBindingInfo>>) -> NodeRef {
        let nf = &self.nf;
        let mut parameter_list: Option<NodeRef> = None;
        for (name, type_) in [(TARGET, OBJECT), (PROPERTY_GETTER, FUNCTION), (STATIC_PROPERTY_GETTER, FUNCTION), (BINDINGS, ARRAY), (WATCHERS, ARRAY)] {
            let p = ast::generate_parameter(&mut nf.f(), name, type_, false);
            parameter_list = Some(nf.f().parameter_list(parameter_list, Some(p)));
        }
        let function_signature = nf.f().function_signature(parameter_list, None);
        ast::void_anno(&function_signature);
        let mut statement_list: Option<NodeRef> = None;
        let imports: Vec<String> = info.borrow().get_imports().iter().cloned().collect();
        for import in imports {
            let import_directive = ast::generate_import(&self.cx, &import);
            statement_list = Some(nf.f().statement_list(statement_list, Some(import_directive)));
        }
        let document_class_name = info.borrow().get_class_name();
        let roots: Vec<WatcherRef> = info.borrow_mut().get_root_watchers().values().cloned().collect();
        for watcher in &roots {
            if watcher.borrow().should_write_self() {
                statement_list = self.generate_watcher(statement_list, watcher, document_class_name.as_deref());
            }
            if watcher.borrow().should_write_children() {
                statement_list = self.generate_watcher_children(statement_list, watcher, document_class_name.as_deref());
            }
        }
        for watcher in &roots {
            statement_list = self.generate_watcher_bottom(statement_list, watcher);
        }
        nf.f().function_common(&self.cx, None, Some(function_signature), statement_list)
    }

    fn generate_setup_function_definition(&self, function_common: NodeRef) -> NodeRef {
        let nf = &self.nf;
        let attribute_list = ast::generate_public_attribute(&mut nf.f());
        let identifier = nf.f().identifier_intern(SETUP, false);
        let function_name = nf.f().function_name(EMPTY_TOKEN, Some(identifier));
        nf.f().function_definition(&self.cx, Some(attribute_list), Some(function_name), Some(function_common))
    }

    fn generate_update_parent_property(&self, watcher_id: i32, parent: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let watchers_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let id_selector = self.generate_array_index(watcher_id);
        ast::set_mode(&id_selector, LEFTBRACKET_TOKEN);
        let watcher_base = nf.f().member_expression(Some(watchers_base), Some(id_selector));
        let update_parent_identifier = nf.f().identifier_intern(UPDATE_PARENT, false);
        let parent_member_expression = self.generate_path(&format!("{TARGET}.{}", parent.borrow().get_path_to_property()));
        let args = nf.f().argument_list(None, Some(parent_member_expression));
        let update_parent_selector = ast::call(nf, update_parent_identifier, Some(args), false);
        ast::expr_stmt(nf, Some(watcher_base), update_parent_selector)
    }

    fn generate_update_parent_private_property(&self, watcher_id: i32, parent: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let watchers_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let id_selector = self.generate_array_index(watcher_id);
        ast::set_mode(&id_selector, LEFTBRACKET_TOKEN);
        let watcher_base = nf.f().member_expression(Some(watchers_base), Some(id_selector));
        let update_parent_identifier = nf.f().identifier_intern(UPDATE_PARENT, false);
        let property_getter_base = ast::generate_getter_selector(&mut nf.f(), PROPERTY_GETTER, false);
        let apply_identifier = nf.f().identifier_intern(APPLY, false);
        let target_identifier = nf.f().identifier_intern(TARGET, false);
        let apply_args = nf.f().argument_list(None, Some(target_identifier));
        let literal_string = nf.f().literal_string(&parent.borrow().get_property().unwrap_or_default());
        let argument_list = nf.f().argument_list(None, Some(literal_string));
        let literal_array = nf.f().literal_array(Some(argument_list));
        let apply_args = nf.f().argument_list(Some(apply_args), Some(literal_array));
        let property_getter_selector = ast::call(nf, apply_identifier, Some(apply_args), false);
        let property_getter_member_expression = nf.f().member_expression(Some(property_getter_base), Some(property_getter_selector));
        let update_parent_args = nf.f().argument_list(None, Some(property_getter_member_expression));
        let update_parent_selector = ast::call(nf, update_parent_identifier, Some(update_parent_args), false);
        ast::expr_stmt(nf, Some(watcher_base), update_parent_selector)
    }

    fn generate_update_parent_static_property(&self, watcher_id: i32, parent: &WatcherRef) -> NodeRef {
        let nf = &self.nf;
        let watchers_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
        let id_selector = self.generate_array_index(watcher_id);
        ast::set_mode(&id_selector, LEFTBRACKET_TOKEN);
        let watcher_base = nf.f().member_expression(Some(watchers_base), Some(id_selector));
        let update_parent_identifier = nf.f().identifier_intern(UPDATE_PARENT, false);
        let path = {
            let p = parent.borrow();
            format!("{}.{}", p.get_class_name().unwrap_or_else(|| "null".into()), p.get_path_to_property())
        };
        let parent_member_expression = self.generate_path(&path);
        let args = nf.f().argument_list(None, Some(parent_member_expression));
        let update_parent_selector = ast::call(nf, update_parent_identifier, Some(args), false);
        ast::expr_stmt(nf, Some(watcher_base), update_parent_selector)
    }

    /// `watchers[id] = new <watcher>(args)`.
    fn watcher_assignment(&self, statement_list: Option<NodeRef>, base: NodeRef, qualified_identifier: NodeRef, expression: NodeRef, argument_list: Option<NodeRef>) -> Option<NodeRef> {
        let nf = &self.nf;
        let call_expression = ast::call(nf, qualified_identifier, argument_list, true);
        let call_member_expression = nf.f().member_expression(None, Some(call_expression));
        let args = nf.f().argument_list(None, Some(call_member_expression));
        let selector = nf.f().set_expression(Some(expression), Some(args), false);
        ast::set_mode(&selector, LEFTBRACKET_TOKEN);
        let expression_statement = ast::expr_stmt(nf, Some(base), selector);
        Some(nf.f().statement_list(statement_list, Some(expression_statement)))
    }

    /// The `generateWatcher` overloads, chosen by watcher class.
    fn generate_watcher(&self, statement_list: Option<NodeRef>, watcher: &WatcherRef, document_class_name: Option<&str>) -> Option<NodeRef> {
        let nf = &self.nf;
        let bp = self.standard_defs.get_binding_package();
        let (is_array, is_function, sub, is_static) = {
            let w = watcher.borrow();
            match &w.kind {
                WatcherKind::ArrayElement(_) => (true, false, None, false),
                WatcherKind::FunctionReturn { .. } => (false, true, None, false),
                WatcherKind::RepeaterItem => (false, false, None, false),
                WatcherKind::Property { sub, static_property, .. } => (false, false, Some(match sub {
                    PropertyKind::Property => 0,
                    PropertyKind::Xml => 1,
                    PropertyKind::RepeaterComponent(_) => 2,
                }), *static_property),
            }
        };
        let id = watcher.borrow().get_id();
        if is_array {
            if !watcher.borrow().should_write_children() {
                return statement_list;
            }
            let base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
            let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), bp, ARRAY_ELEMENT_WATCHER, false);
            let literal_number = nf.f().literal_number_int(id);
            let expression = nf.f().argument_list(None, Some(literal_number));
            let target_member_expression = ast::generate_getter_selector(&mut nf.f(), TARGET, false);
            let argument_list = nf.f().argument_list(None, Some(target_member_expression));
            let accessor = self.generate_accessor_function_array_element(watcher);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(accessor));
            let listener_id = watcher.borrow().get_binding_expression().expect("NullPointerException").get_id();
            let listener = self.generate_listener(listener_id);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(listener));
            return self.watcher_assignment(statement_list, base, qualified_identifier, expression, Some(argument_list));
        }
        if is_function {
            let base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
            let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), bp, FUNCTION_RETURN_WATCHER, false);
            let literal_number = nf.f().literal_number_int(id);
            let expression = nf.f().argument_list(None, Some(literal_number));
            let literal_string = nf.f().literal_string(&watcher.borrow().get_function_name().unwrap_or_default());
            let argument_list = nf.f().argument_list(None, Some(literal_string));
            let target_member_expression = ast::generate_getter_selector(&mut nf.f(), TARGET, false);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(target_member_expression));
            let accessor = self.generate_accessor_function_function_return(watcher);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(accessor));
            let change_events = self.generate_change_events(watcher);
            let argument_list = nf.f().argument_list(Some(argument_list), Some(change_events));
            let listener_id = watcher.borrow().get_binding_expression().expect("NullPointerException").get_id();
            let listener = self.generate_listener(listener_id);
            let mut argument_list = nf.f().argument_list(Some(argument_list), Some(listener));
            let (has_parent, has_class) = {
                let w = watcher.borrow();
                (w.get_parent().is_some(), w.get_class_name().is_some())
            };
            if has_parent || has_class {
                let n = nf.f().literal_null();
                argument_list = nf.f().argument_list(Some(argument_list), Some(n));
            } else {
                let g = ast::generate_getter_selector(&mut nf.f(), PROPERTY_GETTER, false);
                argument_list = nf.f().argument_list(Some(argument_list), Some(g));
            }
            if watcher.borrow().is_style_watcher() {
                let b = nf.f().literal_boolean(true);
                argument_list = nf.f().argument_list(Some(argument_list), Some(b));
            }
            return self.watcher_assignment(statement_list, base, qualified_identifier, expression, Some(argument_list));
        }
        match sub {
            Some(2) => {
                let base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
                let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), bp, REPEATER_COMPONENT_WATCHER, false);
                let literal_number = nf.f().literal_number_int(id);
                let expression = nf.f().argument_list(None, Some(literal_number));
                let literal_string = nf.f().literal_string(&watcher.borrow().get_property().unwrap_or_default());
                let argument_list = nf.f().argument_list(None, Some(literal_string));
                let change_events = self.generate_change_events(watcher);
                let argument_list = nf.f().argument_list(Some(argument_list), Some(change_events));
                let listeners = self.generate_listeners(watcher);
                let argument_list = nf.f().argument_list(Some(argument_list), Some(listeners));
                let g = ast::generate_getter_selector(&mut nf.f(), PROPERTY_GETTER, false);
                let argument_list = nf.f().argument_list(Some(argument_list), Some(g));
                self.watcher_assignment(statement_list, base, qualified_identifier, expression, Some(argument_list))
            }
            None => {
                let base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
                let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), bp, REPEATER_ITEM_WATCHER, false);
                let literal_number = nf.f().literal_number_int(id);
                let expression = nf.f().argument_list(None, Some(literal_number));
                let parent_watchers_base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
                let parent_id = watcher.borrow().get_parent().expect("NullPointerException").borrow().get_id();
                let parent_number = nf.f().literal_number_int(parent_id);
                let parent_expression = nf.f().argument_list(None, Some(parent_number));
                let parent_selector = nf.f().get_expression(Some(parent_expression));
                ast::set_mode(&parent_selector, LEFTBRACKET_TOKEN);
                let parent_member_expression = nf.f().member_expression(Some(parent_watchers_base), Some(parent_selector));
                let argument_list = nf.f().argument_list(None, Some(parent_member_expression));
                self.watcher_assignment(statement_list, base, qualified_identifier, expression, Some(argument_list))
            }
            Some(1) => {
                let base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
                let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), bp, XML_WATCHER, false);
                let literal_number = nf.f().literal_number_int(id);
                let expression = nf.f().argument_list(None, Some(literal_number));
                let literal_string = nf.f().literal_string(&watcher.borrow().get_property().unwrap_or_default());
                let argument_list = nf.f().argument_list(None, Some(literal_string));
                let listeners = self.generate_listeners(watcher);
                let argument_list = nf.f().argument_list(Some(argument_list), Some(listeners));
                self.watcher_assignment(statement_list, base, qualified_identifier, expression, Some(argument_list))
            }
            _ => {
                let base = ast::generate_getter_selector(&mut nf.f(), WATCHERS, false);
                let type_ = if is_static { STATIC_PROPERTY_WATCHER } else { PROPERTY_WATCHER };
                let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), bp, type_, false);
                let literal_number = nf.f().literal_number_int(id);
                let expression = nf.f().argument_list(None, Some(literal_number));
                let literal_string = nf.f().literal_string(&watcher.borrow().get_property().unwrap_or_default());
                let argument_list = nf.f().argument_list(None, Some(literal_string));
                let change_events = self.generate_change_events(watcher);
                let argument_list = nf.f().argument_list(Some(argument_list), Some(change_events));
                let listeners = self.generate_listeners(watcher);
                let mut argument_list = nf.f().argument_list(Some(argument_list), Some(listeners));
                let (has_parent, class_name) = {
                    let w = watcher.borrow();
                    (w.get_parent().is_some(), w.get_class_name())
                };
                if has_parent || (class_name.is_some() && class_name.as_deref() != document_class_name) {
                    let n = nf.f().literal_null();
                    argument_list = nf.f().argument_list(Some(argument_list), Some(n));
                } else if is_static {
                    let g = ast::generate_getter_selector(&mut nf.f(), STATIC_PROPERTY_GETTER, false);
                    argument_list = nf.f().argument_list(Some(argument_list), Some(g));
                } else {
                    let g = ast::generate_getter_selector(&mut nf.f(), PROPERTY_GETTER, false);
                    argument_list = nf.f().argument_list(Some(argument_list), Some(g));
                }
                self.watcher_assignment(statement_list, base, qualified_identifier, expression, Some(argument_list))
            }
        }
    }

    fn generate_watcher_children(&self, statement_list: Option<NodeRef>, watcher: &WatcherRef, document_class_name: Option<&str>) -> Option<NodeRef> {
        let mut result = statement_list;
        let children = watcher.borrow().get_children();
        for child_watcher in &children {
            if child_watcher.borrow().should_write_self() {
                result = self.generate_watcher(result, child_watcher, document_class_name);
            }
            if child_watcher.borrow().should_write_children() {
                result = self.generate_watcher_children(result, child_watcher, document_class_name);
            }
        }
        result
    }

    fn generate_watcher_bottom(&self, statement_list: Option<NodeRef>, watcher: &WatcherRef) -> Option<NodeRef> {
        let nf = &self.nf;
        let mut result = statement_list;
        if watcher.borrow().should_write_self() {
            let (is_array, is_function) = {
                let w = watcher.borrow();
                (w.is_array_element_watcher(), w.is_function_return_watcher())
            };
            if (is_array && watcher.borrow().should_write_self() && watcher.borrow().should_write_children()) || is_function {
                let parent = watcher.borrow().get_parent();
                if parent.is_some_and(|p| p.borrow().should_write_self()) {
                    let s = self.generate_evaluation_watcher_part(watcher);
                    result = Some(nf.f().statement_list(result, Some(s)));
                }
            }
            let parent = watcher.borrow().get_parent();
            match parent {
                Some(parent) => {
                    if parent.borrow().should_write_self() {
                        let s = self.generate_add_child(watcher);
                        result = Some(nf.f().statement_list(result, Some(s)));
                    } else if parent.borrow().is_property_watcher() {
                        let watcher_id = watcher.borrow().get_id();
                        if parent.borrow().get_static_property() {
                            let s = self.generate_update_parent_static_property(watcher_id, &parent);
                            result = Some(nf.f().statement_list(result, Some(s)));
                        } else if parent.borrow().get_parent().is_some() {
                            let s = self.generate_update_parent_property(watcher_id, &parent);
                            result = Some(nf.f().statement_list(result, Some(s)));
                        } else {
                            let s = self.generate_update_parent_private_property(watcher_id, &parent);
                            result = Some(nf.f().statement_list(result, Some(s)));
                        }
                    }
                }
                None => result = self.generate_root_watcher_bottom(result, watcher),
            }
        }
        if watcher.borrow().should_write_children() {
            let children = watcher.borrow().get_children();
            for child_watcher in &children {
                result = self.generate_watcher_bottom(result, child_watcher);
            }
        }
        result
    }
}

/// `generateWatcherSetupUtilAST(compilationUnit, symbolTable, dataBindingInfo)`.
pub fn generate_watcher_setup_util_ast(unit: &UnitRef, symbol_table: &SymbolTable, info: &Rc<RefCell<DataBindingInfo>>, defines: &[ConfigVar]) -> SourceRef {
    let class_name = info.borrow().get_watcher_setup_util_class_name().expect("NullPointerException");
    let short_name = class_name[class_name.rfind('.').map_or(0, |i| i + 1)..].to_string();
    let file_name = format!("{}.as", class_name.replace('.', std::path::MAIN_SEPARATOR_STR));
    let last_modified = unit.borrow().get_source().borrow().get_last_modified();
    let empty_file = VirtualFile::text(String::new(), &file_name, None, None, mime_mappings::AS, last_modified);
    let result = Source::new(empty_file, None, "", &short_name, Owner::None, false, false, false);
    let cx = ast::generate_context(&symbol_table.per_compile_data, &result, &symbol_table.emitter, defines);
    let nf = Nf::of(&cx);
    let mut config_namespaces = HashSet::new();
    let config_vars = ast::parse_config_vars(&cx, &mut config_namespaces);
    let program = ast::generate_program(&cx, config_vars, "");
    let mut program_statement_list = program.borrow().program().unwrap().statements.clone().expect("NullPointerException");
    let standard_defs = StandardDefs::get_standard_defs("halo");
    for watcher_import in standard_defs.get_imports() {
        let import_directive = ast::generate_import(&cx, &watcher_import);
        program_statement_list = nf.f().statement_list(Some(program_statement_list), Some(import_directive));
    }
    let meta_data_node = ast::generate_meta_data(&mut nf.f(), EXCLUDE_CLASS);
    program_statement_list = nf.f().statement_list(Some(program_statement_list), Some(meta_data_node));
    let g = Gen { nf: nf.clone(), cx: cx.clone(), config_namespaces: &config_namespaces, standard_defs: standard_defs.clone() };
    let class_definition = g.generate_class_definition(&short_name, info);
    program_statement_list = nf.f().statement_list(Some(program_statement_list), Some(class_definition));
    program.borrow_mut().program_mut().unwrap().statements = Some(program_statement_list.clone());
    let package_definition = nf.f().finish_package(&cx, None);
    nf.f().statement_list(Some(program_statement_list), package_definition);
    let unit = Source::new_compilation_unit(&result, Some(program));
    unit.borrow_mut().context.set_asc_context(Some(cx.clone()));
    as3_compiler::clean_node_factory(&cx);
    result
}
