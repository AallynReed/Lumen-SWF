//! The direct AST generation half of `flex2.compiler.mxml.rep.StatesModel`: `State.generateDefinitionBody` and the
//! overrides' `generateDefinitionBody`.

use std::collections::HashSet;
use std::rc::Rc;

use super::deps::type_eq;
use super::states_model::{Override, OverrideKind, OverrideRef, State, StatesModel};
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::parser::node::NodeRef;

const _FACTORY: &str = "_factory";
const ADDITEMS: &str = "AddItems";
const DESTINATION: &str = "destination";
const DESTRUCTIONPOLICY: &str = "destructionPolicy";
const HANDLERFUNCTION: &str = "handlerFunction";
const INITIALIZEFROMOBJECT: &str = "initializeFromObject";
const ISARRAY: &str = "isArray";
const ISBASEVALUEDATABOUND: &str = "isBaseValueDataBound";
const ISSTYLE: &str = "isStyle";
const ITEMSFACTORY: &str = "itemsFactory";
const NAME: &str = "name";
const OVERRIDES: &str = "overrides";
const ORIGINALHANDLERFUNCTION: &str = "originalHandlerFunction";
const POSITION: &str = "position";
const PROPERTYNAME: &str = "propertyName";
const RELATIVETO: &str = "relativeTo";
const SETPROPERTY: &str = "SetProperty";
const SETSTYLE: &str = "SetStyle";
const SETEVENTHANDLER: &str = "SetEventHandler";
const STATE: &str = "State";
const STATEGROUPS: &str = "stateGroups";
const TARGET: &str = "target";
const UNDEFINED: &str = "undefined";
const VALUE: &str = "value";
const VALUEFACTORY: &str = "valueFactory";
const VECTORCLASS: &str = "vectorClass";

fn field(nf: &Nf, args: Option<NodeRef>, name: &str, value: NodeRef) -> NodeRef {
    let identifier = nf.f().identifier_intern(name, false);
    let literal_field = nf.f().literal_field(Some(identifier), Some(value));
    nf.f().argument_list(args, Some(literal_field))
}

fn getter(nf: &Nf, name: &str) -> NodeRef {
    let identifier = nf.f().identifier_intern(name, false);
    let get_expression = nf.f().get_expression(Some(identifier));
    nf.f().member_expression(None, Some(get_expression))
}

/// `new <type>().initializeFromObject({args})`.
fn initialize_from_object(nf: &Nf, type_identifier: NodeRef, init_object_identifier: NodeRef, args: Option<NodeRef>) -> NodeRef {
    let literal_object = nf.f().literal_object(args);
    let argument_list = nf.f().argument_list(None, Some(literal_object));
    let init_expression = ast::call(nf, init_object_identifier, Some(argument_list), false);
    let call_expression = ast::call(nf, type_identifier, None, true);
    let base = nf.f().member_expression(None, Some(call_expression));
    nf.f().member_expression(Some(base), Some(init_expression))
}

impl State {
    /// `generateDefinitionBody(nodeFactory, configNamespaces, generateDocComments, bindingsQueue)`.
    pub fn generate_definition_body(&self, sm: &StatesModel, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, bindings_queue: &mut Vec<OverrideRef>) -> NodeRef {
        let state_identifier = nf.f().identifier_intern(STATE, false);
        let name_identifier = nf.f().identifier_intern(NAME, false);
        let overrides_identifier = nf.f().identifier_intern(OVERRIDES, false);
        let literal_string = nf.f().literal_string(&self.name);
        let literal_field = nf.f().literal_field(Some(name_identifier), Some(literal_string));
        let mut state_arguments = nf.f().argument_list(None, Some(literal_field));
        let model = self.model.borrow().clone().expect("NullPointerException");
        for init in model.get_property_initializer_iterator_bindings(false) {
            if init.is_named() {
                let init_name = init.get_name();
                if init_name != "name" && init_name != "stateGroups" && init_name != "overrides" {
                    let prop_identifier = nf.f().identifier(&init_name);
                    let value = init.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                    let literal_field = nf.f().literal_field(Some(prop_identifier), Some(value));
                    state_arguments = nf.f().argument_list(Some(state_arguments), Some(literal_field));
                }
            }
        }
        let groups = sm.get_groups_for_state(&self.name);
        if !groups.is_empty() {
            let mut group_arguments: Option<NodeRef> = None;
            for g in &groups {
                let sibling = nf.f().literal_string(g);
                group_arguments = Some(nf.f().argument_list(group_arguments, Some(sibling)));
            }
            let rel_array = nf.f().literal_array(group_arguments);
            state_arguments = field(nf, Some(state_arguments), STATEGROUPS, rel_array);
        }
        let mut overrides_argument_list: Option<NodeRef> = None;
        let overrides = self.overrides.borrow().clone();
        for o in &overrides {
            let member_expression = o.generate_definition_body(sm, nf, config_namespaces, generate_doc_comments);
            overrides_argument_list = Some(nf.f().argument_list(overrides_argument_list, Some(member_expression)));
            if o.declaration.borrow().is_some() {
                bindings_queue.push(o.clone());
            }
        }
        let overrides_array = nf.f().literal_array(overrides_argument_list);
        let literal_field = nf.f().literal_field(Some(overrides_identifier), Some(overrides_array));
        let state_arguments = nf.f().argument_list(Some(state_arguments), Some(literal_field));
        let literal_object = nf.f().literal_object(Some(state_arguments));
        let state_descriptor_argument_list = nf.f().argument_list(None, Some(literal_object));
        let call_expression = ast::call(nf, state_identifier, Some(state_descriptor_argument_list), true);
        nf.f().member_expression(None, Some(call_expression))
    }
}

impl Override {
    /// `generateDefinitionBody(nodeFactory, configNamespaces, generateDocComments)`, per override class.
    pub fn generate_definition_body(&self, sm: &StatesModel, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool) -> NodeRef {
        let document = sm.document();
        let root = document.get_root();
        match &self.kind {
            OverrideKind::AddItems { property_name, factory, parent, model, position, relative_nodes, is_style, is_array, vector_class_name, .. } => {
                let add_items_identifier = nf.f().identifier_intern(ADDITEMS, false);
                let init_object_identifier = nf.f().identifier_intern(INITIALIZEFROMOBJECT, false);
                let mut ai_arguments: Option<NodeRef> = None;
                if model.get_is_transient() {
                    let prop_node = nf.f().literal_string("auto");
                    ai_arguments = Some(field(nf, ai_arguments, DESTRUCTIONPOLICY, prop_node));
                }
                let items_identifier = nf.f().identifier_intern(ITEMSFACTORY, false);
                let factory_member_expression = getter(nf, &format!("{factory}{_FACTORY}"));
                let literal_field = nf.f().literal_field(Some(items_identifier), Some(factory_member_expression));
                ai_arguments = Some(nf.f().argument_list(ai_arguments, Some(literal_field)));
                if *is_style {
                    let value = nf.f().literal_boolean(true);
                    ai_arguments = Some(field(nf, ai_arguments, ISSTYLE, value));
                }
                let mut parent_element_count = 0;
                if let Some(p) = model.get_parent() {
                    if type_eq(p.get_type(), &sm.array_type()) || p.get_type().get_element_type().is_some() {
                        parent_element_count = p.get_entries().len();
                    }
                }
                if *is_array && parent_element_count > 1 {
                    let value = nf.f().literal_boolean(true);
                    ai_arguments = Some(field(nf, ai_arguments, ISARRAY, value));
                }
                if let Some(v) = vector_class_name {
                    let vector_class_identifier = nf.f().identifier_intern(VECTORCLASS, false);
                    let apply_type_expr = ast::generate_apply_type_expr_at(&mut nf.f(), v, v.find('<').expect("StringIndexOutOfBoundsException"));
                    let type_value = nf.f().member_expression(None, Some(apply_type_expr));
                    let literal_field = nf.f().literal_field(Some(vector_class_identifier), Some(type_value));
                    ai_arguments = Some(nf.f().argument_list(ai_arguments, Some(literal_field)));
                }
                let dest_identifier = nf.f().identifier_intern(DESTINATION, false);
                let value_node = match parent {
                    Some(p) => nf.f().literal_string(&p.get_id().unwrap_or_default()),
                    None => nf.f().literal_null(),
                };
                let literal_field = nf.f().literal_field(Some(dest_identifier), Some(value_node));
                ai_arguments = Some(nf.f().argument_list(ai_arguments, Some(literal_field)));
                if let Some(pn) = property_name {
                    let prop_node = nf.f().literal_string(pn);
                    ai_arguments = Some(field(nf, ai_arguments, PROPERTYNAME, prop_node));
                }
                if let Some(position) = position {
                    let pos_node = nf.f().literal_string(position);
                    ai_arguments = Some(field(nf, ai_arguments, POSITION, pos_node));
                    let rn = relative_nodes.clone().unwrap_or_default();
                    if position == "after" && !rn.is_empty() {
                        let mut rel_arguments: Option<NodeRef> = None;
                        for n in &rn {
                            let sibling = nf.f().literal_string(&n.get_id().unwrap_or_default());
                            rel_arguments = Some(nf.f().argument_list(rel_arguments, Some(sibling)));
                        }
                        let rel_array = nf.f().literal_array(rel_arguments);
                        ai_arguments = Some(field(nf, ai_arguments, RELATIVETO, rel_array));
                    }
                }
                initialize_from_object(nf, add_items_identifier, init_object_identifier, ai_arguments)
            }
            OverrideKind::SetProperty { context, value, property, clear, factory, is_style_override } => {
                let context = context.as_ref().expect("NullPointerException");
                let declared_class = if *is_style_override { SETSTYLE } else { SETPROPERTY };
                let is_data_bound = if declared_class == SETPROPERTY { context.has_data_bound_property(property) } else { context.has_data_bound_style(property) };
                let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), sm.standard_defs.get_states_package(), declared_class, false);
                let init_object_identifier = nf.f().identifier_intern(INITIALIZEFROMOBJECT, false);
                let mut sp_arguments: Option<NodeRef> = None;
                if is_data_bound {
                    let v = nf.f().literal_boolean(true);
                    sp_arguments = Some(field(nf, sp_arguments, ISBASEVALUEDATABOUND, v));
                }
                if !Rc::ptr_eq(&root, context) {
                    let v = nf.f().literal_string(&context.get_id().unwrap_or_default());
                    sp_arguments = Some(field(nf, sp_arguments, TARGET, v));
                }
                let name_node = nf.f().literal_string(property);
                sp_arguments = Some(field(nf, sp_arguments, NAME, name_node));
                if !clear {
                    match factory {
                        Some(f) => {
                            let items_identifier = nf.f().identifier_intern(VALUEFACTORY, false);
                            let factory_member_expression = getter(nf, &format!("{f}{_FACTORY}"));
                            let literal_field = nf.f().literal_field(Some(items_identifier), Some(factory_member_expression));
                            sp_arguments = Some(nf.f().argument_list(sp_arguments, Some(literal_field)));
                        }
                        None => {
                            let value_identifier = nf.f().identifier_intern(VALUE, false);
                            let v = value.borrow().clone();
                            let value_node = if v.is_binding() { getter(nf, UNDEFINED) } else { v.generate_value_expr(nf, config_namespaces, generate_doc_comments) };
                            let literal_field = nf.f().literal_field(Some(value_identifier), Some(value_node));
                            sp_arguments = Some(nf.f().argument_list(sp_arguments, Some(literal_field)));
                        }
                    }
                }
                let exp_node = initialize_from_object(nf, qualified_identifier.clone(), init_object_identifier, sp_arguments);
                let declaration = self.declaration.borrow().clone();
                if let Some(declaration) = declaration {
                    let cast_arg_list2 = nf.f().argument_list(None, Some(exp_node));
                    let cast_expression = ast::call(nf, qualified_identifier, Some(cast_arg_list2), false);
                    let cast_node = nf.f().member_expression(None, Some(cast_expression));
                    let cast_arg_list = nf.f().argument_list(None, Some(cast_node));
                    let decl_identifier = nf.f().identifier(&declaration);
                    let decl_selector = nf.f().set_expression(Some(decl_identifier), Some(cast_arg_list), false);
                    return nf.f().member_expression(None, Some(decl_selector));
                }
                exp_node
            }
            OverrideKind::SetEvent { context, event, value, clear } => {
                let context = context.as_ref().expect("NullPointerException");
                let is_data_bound = context.has_data_bound_event(event);
                let se_items_identifier = nf.f().identifier_intern(SETEVENTHANDLER, false);
                let init_object_identifier = nf.f().identifier_intern(INITIALIZEFROMOBJECT, false);
                let mut sp_arguments: Option<NodeRef> = None;
                let base_value = context.get_event_initializer(event);
                if is_data_bound {
                    let v = nf.f().literal_boolean(true);
                    sp_arguments = Some(field(nf, sp_arguments, ISBASEVALUEDATABOUND, v));
                }
                if !Rc::ptr_eq(&root, context) {
                    let v = nf.f().literal_string(&context.get_id().unwrap_or_default());
                    sp_arguments = Some(field(nf, sp_arguments, TARGET, v));
                }
                let name_node = nf.f().literal_string(event);
                sp_arguments = Some(field(nf, sp_arguments, NAME, name_node));
                if let Some(base_value) = &base_value {
                    let value_identifier = nf.f().identifier_intern(ORIGINALHANDLERFUNCTION, false);
                    let value_node = base_value.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                    let literal_field = nf.f().literal_field(Some(value_identifier), Some(value_node));
                    sp_arguments = Some(nf.f().argument_list(sp_arguments, Some(literal_field)));
                }
                if !clear {
                    let value_identifier = nf.f().identifier_intern(HANDLERFUNCTION, false);
                    let value_node = value.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                    let literal_field = nf.f().literal_field(Some(value_identifier), Some(value_node));
                    sp_arguments = Some(nf.f().argument_list(sp_arguments, Some(literal_field)));
                }
                initialize_from_object(nf, se_items_identifier, init_object_identifier, sp_arguments)
            }
        }
    }
}
