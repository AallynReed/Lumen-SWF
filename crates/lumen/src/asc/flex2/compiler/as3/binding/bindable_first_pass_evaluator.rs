//! Port of `flex2.compiler.as3.binding.BindableFirstPassEvaluator`: finds `[Bindable]` classes, variables and
//! getter/setters and records them as `BindableInfo`s.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use indexmap::IndexMap;

use super::bindable_info::BindableInfo;
use crate::asc::flex2::compiler::as3::evaluator_adapter::{walk, Adapter};
use crate::asc::flex2::compiler::as3::genext::generative_class_info::{GenerativeClassInfo, GenerativeClassInfoRef, GenerativeKind};
use crate::asc::flex2::compiler::as3::reflect::node_magic;
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{self, StandardDefs};
use crate::asc::flex2::compiler::mxml::lang::text_parser;
use crate::asc::flex2::compiler::toolkit::Level;
use crate::asc::flex2::compiler::util::{name_formatter, QName};
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::tokens::CONST_TOKEN;
use crate::asc::util::context::ContextRef;

const BINDABLE_NONE: i32 = 0;
const BINDABLE_USER: i32 = 1;
const BINDABLE_CODEGEN_PROP: i32 = 2;
const BINDABLE_CODEGEN_CLASS: i32 = 3;
const STYLE: &str = "style";

const CLASS_BINDABLE_UNNECESSARY_ON_MANAGED_CLASS: &str = "[Managed] classes are automatically [Bindable] - metadata will be ignored.";
const PROPERTY_BINDABLE_UNNECESSARY_ON_MANAGED_CLASS: &str = "Properties of a [Managed] class are automatically [Bindable] - metadata will be ignored.";
const PROPERTY_BINDABLE_UNNECESSARY_ON_BINDABLE_CLASS: &str = "Class already marked [Bindable]; property-level [Bindable] is redundant and will be ignored.";
const BINDABLE_FUNCTION_REQUIRES_EVENT_NAME: &str = "[Bindable] requires an event name parameter when applied to a function that is not a getter or setter.";
const BINDABLE_NOT_ALLOWED_INSIDE_FUNCTION_DEFINITION: &str = "[Bindable] is not allowed inside function definition.";
const BINDABLE_NOT_ALLOWED_HERE: &str = "[Bindable] not allowed here.";
const BINDABLE_NOT_ALLOWED_ON_GLOBAL_OR_PACKAGE_VARIABLES: &str = "[Bindable] not allowed on global or package variables.";
const BINDABLE_NOT_ALLOWED_ON_CONST_MEMBER_VARIABLES: &str = "[Bindable] not allowed on const member variables.";
const BINDABLE_NOT_ALLOWED_HERE_ON_NON_PUBLIC_MEMBER_VARIABLES: &str = "[Bindable] not allowed on non-public member variables in this context.";
const BINDABLE_NOT_ALLOWED_ON_GLOBAL_OR_PACKAGE_FUNCTIONS: &str = "[Bindable] not allowed on global or package-level functions.";
const BINDABLE_NOT_ALLOWED_HERE_ON_NON_PUBLIC_FUNCTIONS: &str = "[Bindable] not allowed on non-public functions in this context.";
const EVENT_NAME_NOT_VALID: &str = "The event name is not a valid ActionScript identifier.";

type Key = *const std::cell::RefCell<crate::asc::parser::node::Node>;

fn key(n: &NodeRef) -> Key {
    Rc::as_ptr(n)
}

pub struct BindableFirstPassEvaluator {
    standard_defs: Rc<StandardDefs>,
    meta_data: Vec<NodeRef>,
    bindable_classes: Option<HashSet<Key>>,
    managed_classes: Option<HashSet<Key>>,
    evaluated_classes: HashSet<Key>,
    class_map: IndexMap<String, GenerativeClassInfoRef>,
    current_class_node: Option<NodeRef>,
    bindable_info: Option<GenerativeClassInfoRef>,
    visited_props: Option<HashMap<String, i32>>,
    in_function: bool,
}

fn new_bindable_info() -> GenerativeClassInfoRef {
    Rc::new(std::cell::RefCell::new(GenerativeClassInfo::new(GenerativeKind::Bindable(BindableInfo::default()))))
}

fn warn(cx: &ContextRef, pos: i32, text: &str) {
    localized2(cx, Level::Warning, None, pos, text.to_string());
}

fn error(cx: &ContextRef, pos: i32, text: &str) {
    localized2(cx, Level::Error, None, pos, text.to_string());
}

fn has_attribute(def: &NodeRef, name: &str) -> bool {
    let attrs = def.borrow().definition().and_then(|d| d.attrs.clone());
    attrs.is_some_and(|a| a.borrow().has_attribute(name))
}

fn is_class_definition(n: &NodeRef) -> bool {
    matches!(n.borrow().kind, Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) | Kind::InterfaceDefinition(_))
}

impl BindableFirstPassEvaluator {
    pub fn new(standard_defs: Rc<StandardDefs>, meta_data: Vec<NodeRef>) -> BindableFirstPassEvaluator {
        BindableFirstPassEvaluator {
            standard_defs,
            meta_data,
            bindable_classes: None,
            managed_classes: None,
            evaluated_classes: HashSet::new(),
            class_map: IndexMap::new(),
            current_class_node: None,
            bindable_info: None,
            visited_props: None,
            in_function: false,
        }
    }

    fn program(&mut self, context: &ContextRef, program_node: &NodeRef) {
        for md in self.meta_data.clone() {
            let (id, def) = {
                let b = md.borrow();
                let m = b.meta_data().unwrap();
                (m.get_id(), m.def.clone())
            };
            if id.as_deref() == Some(standard_defs::MD_MANAGED) {
                if let Some(def) = def.filter(is_class_definition) {
                    self.register_managed_class(&def);
                }
            }
        }
        for md in self.meta_data.clone() {
            let (id, def, pos) = {
                let b = md.borrow();
                let m = b.meta_data().unwrap();
                (m.get_id(), m.def.clone(), b.pos())
            };
            if id.as_deref() == Some(standard_defs::MD_BINDABLE) {
                if let Some(def) = def.filter(is_class_definition) {
                    if self.is_managed_class(&def) {
                        warn(context, pos, CLASS_BINDABLE_UNNECESSARY_ON_MANAGED_CLASS);
                    } else if get_event_name(&md, context).is_none() {
                        self.register_bindable_class(&def);
                    }
                }
            }
        }
        walk(self, context, program_node);
    }

    fn meta_data_node(&mut self, context: &ContextRef, meta_data_node: &NodeRef) {
        let (id, def, md_pos) = {
            let b = meta_data_node.borrow();
            let m = b.meta_data().unwrap();
            (m.get_id(), m.def.clone(), b.pos())
        };
        if id.as_deref() != Some(standard_defs::MD_BINDABLE) {
            return;
        }
        let def_kind = def.as_ref().map(|d| match d.borrow().kind {
            Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) | Kind::InterfaceDefinition(_) => 0,
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => 1,
            Kind::VariableDefinition(_) => 2,
            _ => 3,
        });
        match def_kind {
            Some(0) => {}
            Some(1) => {
                let node = def.unwrap();
                let pos = node.borrow().pos();
                let style = meta_data_node.borrow().meta_data().unwrap().get_value_key(STYLE);
                if style.as_deref() == Some("true") {
                } else if get_event_name(meta_data_node, context).is_none() {
                    if self.in_managed_class() {
                        warn(context, pos, PROPERTY_BINDABLE_UNNECESSARY_ON_MANAGED_CLASS);
                    } else if self.in_bindable_class() {
                        warn(context, pos, PROPERTY_BINDABLE_UNNECESSARY_ON_BINDABLE_CLASS);
                    } else {
                        let is_getter = node_magic::function_is_getter(&node);
                        if is_getter || node_magic::function_is_setter(&node) {
                            if self.check_bindable_getter_setter(context, &node, false, false) {
                                self.register_bindable_getter_setter(&node, true, is_getter);
                            }
                        } else {
                            warn(context, pos, BINDABLE_FUNCTION_REQUIRES_EVENT_NAME);
                        }
                    }
                } else if node_magic::function_is_getter(&node) || node_magic::function_is_setter(&node) {
                    let name = node_magic::get_function_name(&node).unwrap_or_default();
                    if self.get_visited_getter_setter_bind_type(&name) == BINDABLE_CODEGEN_CLASS {
                        let qname = QName::new(&node_magic::get_user_namespace(&node), &name);
                        self.unregister_bindable_accessor(&qname);
                    }
                    self.register_visited_getter_setter(&name, BINDABLE_USER);
                }
            }
            Some(2) => {
                let node = def.unwrap();
                let pos = node.borrow().pos();
                if self.in_function {
                    error(context, pos, BINDABLE_NOT_ALLOWED_INSIDE_FUNCTION_DEFINITION);
                } else if get_event_name(meta_data_node, context).is_none() {
                    if self.in_managed_class() {
                        warn(context, pos, PROPERTY_BINDABLE_UNNECESSARY_ON_MANAGED_CLASS);
                    } else if self.in_bindable_class() {
                        warn(context, pos, PROPERTY_BINDABLE_UNNECESSARY_ON_BINDABLE_CLASS);
                    } else if self.check_bindable_variable(context, &node, false, false) {
                        self.register_bindable_variable(&node, true);
                    }
                }
            }
            _ => error(context, md_pos, BINDABLE_NOT_ALLOWED_HERE),
        }
    }

    fn class_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        if self.evaluated_classes.contains(&key(node)) {
            return;
        }
        self.evaluated_classes.insert(key(node));
        self.set_current_class(Some(node.clone()));
        let (statements, instanceinits) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (c.statements.clone(), c.instanceinits.as_ref().map(|l| l.borrow().clone()))
        };
        if let Some(statements) = statements {
            if let Some(instanceinits) = instanceinits {
                for instanceinit in instanceinits {
                    crate::asc::flex2::compiler::as3::evaluator_adapter::evaluate(self, context, &instanceinit);
                }
            }
            crate::asc::flex2::compiler::as3::evaluator_adapter::evaluate(self, context, &statements);
        }
        if let Some(bindable_info) = self.bindable_info.clone() {
            bindable_info.borrow_mut().set_class_name(&node_magic::get_unqualified_class_name(node));
            self.class_map.insert(node_magic::get_class_name(node), bindable_info);
            self.add_imports(context, node);
        }
        self.set_current_class(None);
    }

    fn add_imports(&self, context: &ContextRef, node: &NodeRef) {
        let sd = &self.standard_defs;
        for c in [&sd.class_event, &sd.class_eventdispatcher, &sd.interface_ieventdispatcher, &sd.class_bindingmanager] {
            node_magic::add_import(context, node, &name_formatter::to_dot(c));
        }
    }

    fn function_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        let name = node_magic::get_function_name(node).unwrap_or_default();
        if self.in_bindable_class() {
            let is_getter = node_magic::function_is_getter(node);
            if (is_getter || node_magic::function_is_setter(node)) && self.get_visited_getter_setter_bind_type(&name) != BINDABLE_USER {
                self.register_bindable_getter_setter(node, false, is_getter);
            }
        }
        self.in_function = true;
        walk(self, context, node);
        self.in_function = false;
    }

    fn variable_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        let qname = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_variable_name(node));
        if self.in_bindable_class() && !self.in_function && !self.is_bindable_accessor(&qname) && self.check_bindable_variable(context, node, true, true) {
            self.register_bindable_variable(node, false);
        }
    }

    fn check_bindable_variable(&self, context: &ContextRef, def: &NodeRef, quiet: bool, public_only: bool) -> bool {
        let pos = def.borrow().pos();
        if !self.in_class() {
            if !quiet {
                error(context, pos, BINDABLE_NOT_ALLOWED_ON_GLOBAL_OR_PACKAGE_VARIABLES);
            }
            return false;
        }
        let first_kind = {
            let b = def.borrow();
            let v = b.as_variable_definition().unwrap();
            v.list.as_ref().and_then(|l| {
                let lb = l.borrow();
                let item = lb.list().and_then(|l| l.items.first().cloned()).flatten()?;
                let ib = item.borrow();
                ib.as_variable_binding().map(|vb| vb.kind)
            })
        };
        if first_kind == Some(CONST_TOKEN) {
            if !quiet {
                error(context, pos, BINDABLE_NOT_ALLOWED_ON_CONST_MEMBER_VARIABLES);
            }
            return false;
        }
        if public_only && !has_attribute(def, node_magic::PUBLIC) {
            if !quiet {
                error(context, pos, BINDABLE_NOT_ALLOWED_HERE_ON_NON_PUBLIC_MEMBER_VARIABLES);
            }
            return false;
        }
        true
    }

    fn check_bindable_getter_setter(&self, context: &ContextRef, def: &NodeRef, quiet: bool, public_only: bool) -> bool {
        let pos = def.borrow().pos();
        if !self.in_class() {
            if !quiet {
                error(context, pos, BINDABLE_NOT_ALLOWED_ON_GLOBAL_OR_PACKAGE_FUNCTIONS);
            }
            return false;
        }
        if public_only && !has_attribute(def, node_magic::PUBLIC) {
            if !quiet {
                error(context, pos, BINDABLE_NOT_ALLOWED_HERE_ON_NON_PUBLIC_FUNCTIONS);
            }
            return false;
        }
        true
    }

    fn set_current_class(&mut self, node: Option<NodeRef>) {
        self.current_class_node = node;
        self.visited_props = None;
        let bindable = self.current_class_node.as_ref().is_some_and(|n| self.is_bindable_class(n));
        self.bindable_info = if bindable { Some(new_bindable_info()) } else { None };
    }

    fn in_class(&self) -> bool {
        self.current_class_node.is_some()
    }

    fn in_managed_class(&self) -> bool {
        self.current_class_node.as_ref().is_some_and(|n| self.is_managed_class(n))
    }

    fn in_bindable_class(&self) -> bool {
        self.current_class_node.as_ref().is_some_and(|n| self.is_bindable_class(n))
    }

    fn register_bindable_class(&mut self, def: &NodeRef) {
        self.bindable_classes.get_or_insert_with(HashSet::new).insert(key(def));
    }

    fn is_bindable_class(&self, def: &NodeRef) -> bool {
        self.bindable_classes.as_ref().is_some_and(|s| s.contains(&key(def)))
    }

    /// `registerBindableVariable(context, classNode, varNode)`.
    pub fn register_bindable_class_variable(&mut self, context: &ContextRef, class_node: &NodeRef, var_node: &NodeRef) {
        let name = node_magic::get_class_name(class_node);
        let b_info = match self.class_map.get(&name) {
            Some(b) => b.clone(),
            None => {
                let b = new_bindable_info();
                b.borrow_mut().set_class_name(&node_magic::get_unqualified_class_name(class_node));
                self.class_map.insert(name, b.clone());
                self.add_imports(context, class_node);
                b
            }
        };
        b_info.borrow_mut().add_accessor_variable(var_node, false);
    }

    fn register_bindable_variable(&mut self, node: &NodeRef, prop_level: bool) {
        let info = self.bindable_info.get_or_insert_with(new_bindable_info).clone();
        if has_attribute(node, node_magic::STATIC) {
            info.borrow_mut().bindable_mut().set_requires_static_event_dispatcher(true);
        }
        info.borrow_mut().add_accessor_variable(node, prop_level);
    }

    fn register_bindable_getter_setter(&mut self, node: &NodeRef, prop_level: bool, is_getter: bool) {
        let info = self.bindable_info.get_or_insert_with(new_bindable_info).clone();
        if has_attribute(node, node_magic::STATIC) {
            info.borrow_mut().bindable_mut().set_requires_static_event_dispatcher(true);
        }
        info.borrow_mut().add_accessor_function(node, prop_level, is_getter);
        self.register_visited_getter_setter(&node_magic::get_function_name(node).unwrap_or_default(), if prop_level { BINDABLE_CODEGEN_PROP } else { BINDABLE_CODEGEN_CLASS });
    }

    fn unregister_bindable_accessor(&mut self, qname: &QName) {
        if let Some(b) = &self.bindable_info {
            b.borrow_mut().remove_accessor(qname);
        }
    }

    fn is_bindable_accessor(&self, qname: &QName) -> bool {
        self.bindable_info.as_ref().is_some_and(|b| b.borrow().has_accessor(qname))
    }

    fn register_managed_class(&mut self, def: &NodeRef) {
        self.managed_classes.get_or_insert_with(HashSet::new).insert(key(def));
    }

    fn is_managed_class(&self, def: &NodeRef) -> bool {
        self.managed_classes.as_ref().is_some_and(|s| s.contains(&key(def)))
    }

    fn register_visited_getter_setter(&mut self, name: &str, bind_type: i32) {
        self.visited_props.get_or_insert_with(HashMap::new).insert(name.to_string(), bind_type);
    }

    fn get_visited_getter_setter_bind_type(&self, name: &str) -> i32 {
        self.visited_props.as_ref().and_then(|m| m.get(name).copied()).unwrap_or(BINDABLE_NONE)
    }

    pub fn get_class_map(&self) -> &IndexMap<String, GenerativeClassInfoRef> {
        &self.class_map
    }

    pub fn make_second_pass(&self) -> bool {
        !self.class_map.is_empty()
    }
}

fn get_event_name(node: &NodeRef, context: &ContextRef) -> Option<String> {
    let (event_name, pos) = {
        let b = node.borrow();
        let m = b.meta_data().unwrap();
        let mut e = m.get_value_key(standard_defs::MDPARAM_BINDABLE_EVENT);
        if e.is_none() && m.count() == 1 {
            e = m.get_value_index(0);
        }
        (e, b.pos())
    };
    if let Some(e) = &event_name {
        if !text_parser::is_valid_identifier(e) {
            error(context, pos, EVENT_NAME_NOT_VALID);
        }
    }
    event_name
}

impl Adapter for BindableFirstPassEvaluator {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let k = match node.borrow().kind {
            Kind::Program(_) | Kind::BinaryProgram(_) => 0,
            Kind::MetaData(_) => 1,
            Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) => 2,
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => 3,
            Kind::VariableDefinition(_) => 4,
            _ => return false,
        };
        match k {
            0 => self.program(cx, node),
            1 => self.meta_data_node(cx, node),
            2 => self.class_definition(cx, node),
            3 => self.function_definition(cx, node),
            _ => self.variable_definition(cx, node),
        }
        true
    }
}
