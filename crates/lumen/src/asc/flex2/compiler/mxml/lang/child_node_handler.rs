//! Port of `flex2.compiler.mxml.lang.ChildNodeHandler`; implementors keep its fields in a [`ChildNodeHandlerState`].
//! Its `CoreDeclarationHandler` is `core_invoke`, which makes each state transition before the callback.

use std::rc::Rc;

use super::declaration_handler::{resolve, Declaration};
use super::text_parser;
use super::value_node_handler::is_value_node;
use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::reflect::{EffectRef, EventRef, PropertyRef, StyleRef, TypeRef, TypeTable};

pub const STATE_BEFORE_DP_CHILDREN: i32 = 0;
pub const STATE_IN_DP_CHILDREN: i32 = 1;
pub const STATE_AFTER_DP_CHILDREN: i32 = 2;
pub const NUM_STATES: i32 = STATE_AFTER_DP_CHILDREN + 1;
pub const STATE_BITS: i32 = (1 << 4) - 1;
pub const INPUT_NON_DP_CHILD: i32 = 0;
pub const INPUT_DP_CHILD: i32 = 1;
pub const NUM_INPUTS: i32 = INPUT_DP_CHILD + 1;
pub const ERR_DP_LOCATION: i32 = 1 << STATE_BITS;
const TRAN: [i32; 6] = [
    STATE_BEFORE_DP_CHILDREN,
    STATE_IN_DP_CHILDREN,
    STATE_AFTER_DP_CHILDREN,
    STATE_IN_DP_CHILDREN,
    STATE_AFTER_DP_CHILDREN,
    STATE_AFTER_DP_CHILDREN | ERR_DP_LOCATION,
];

pub struct ChildNodeHandlerState {
    pub type_table: Rc<TypeTable>,
    pub is_fxg: bool,
    pub parent: Option<NodeRef>,
    pub parent_type: Option<TypeRef>,
    pub child: Option<NodeRef>,
    pub state: i32,
}

impl ChildNodeHandlerState {
    pub fn new(type_table: Rc<TypeTable>, is_fxg: bool) -> ChildNodeHandlerState {
        ChildNodeHandlerState { type_table, is_fxg, parent: None, parent_type: None, child: None, state: 0 }
    }

    pub fn parent(&self) -> NodeRef {
        self.parent.clone().expect("parent")
    }

    pub fn parent_type(&self) -> TypeRef {
        self.parent_type.clone().expect("parentType")
    }

    pub fn child(&self) -> NodeRef {
        self.child.clone().expect("child")
    }

    pub fn get_state(&self) -> i32 {
        self.state & STATE_BITS
    }

    pub fn get_error(&self) -> i32 {
        self.state & !STATE_BITS
    }

    pub fn reset_state(&mut self) {
        self.state = STATE_BEFORE_DP_CHILDREN;
    }

    pub fn transition(&mut self, input: i32) {
        self.state = TRAN[(self.get_state() * NUM_INPUTS + input) as usize];
    }
}

pub trait ChildNodeHandler {
    fn cnh(&mut self) -> &mut ChildNodeHandlerState;

    fn event(&mut self, event: &EventRef);
    fn property(&mut self, property: &PropertyRef);
    fn states(&mut self, property: &PropertyRef);
    fn effect(&mut self, effect: &EffectRef);
    fn style(&mut self, style: &StyleRef);
    fn dynamic_property(&mut self, name: &str, state: Option<&str>);
    fn default_property_element(&mut self, loc_error: bool);
    fn nested_declaration(&mut self);
    fn text_content(&mut self);
    fn language_node(&mut self);

    /// `scanChildNodes(node, type)`.
    fn scan_child_nodes(&mut self, node: &NodeRef, t: &TypeRef) {
        self.cnh().reset_state();
        let children = node.borrow().get_children();
        for child in children {
            self.invoke_child(node, t, &child);
        }
    }

    /// `invoke(parent, parentType, child)`.
    fn invoke_child(&mut self, parent: &NodeRef, parent_type: &TypeRef, child: &NodeRef) {
        {
            let st = self.cnh();
            st.parent = Some(parent.clone());
            st.parent_type = Some(parent_type.clone());
            st.child = Some(child.clone());
        }
        if child.borrow().is_cdata_node() {
            if parent_type.get_default_property().is_some() {
                self.cnh().transition(INPUT_DP_CHILD);
                let loc_error = self.cnh().get_error() == ERR_DP_LOCATION;
                self.default_property_element(loc_error);
            } else {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.text_content();
            }
        } else if is_value_node(child) {
            let (namespace, mut local_part, count) = {
                let c = child.borrow();
                (c.get_namespace().to_string(), c.get_local_part().to_string(), c.get_attribute_count())
            };
            if count == 0 && namespace == parent.borrow().get_namespace() {
                if text_parser::is_scoped_name(&local_part) {
                    if let Some(stateful_name) = text_parser::analyze_scoped_name(&local_part) {
                        local_part = stateful_name[0].clone();
                    }
                }
                self.core_invoke(parent_type, &namespace, &local_part, None);
            } else {
                self.unknown(&namespace, &local_part);
            }
        } else {
            self.language_node();
        }
    }

    /// `invoke(type, namespace, localPart, state)`, deferring to the core declaration handler.
    fn invoke_state(&mut self, t: &TypeRef, namespace: &str, local_part: &str, state: Option<&str>) {
        self.core_invoke(t, namespace, local_part, state);
    }

    /// `CoreDeclarationHandler.invoke`.
    fn core_invoke(&mut self, t: &TypeRef, namespace: &str, local_part: &str, state: Option<&str>) {
        match resolve(t, namespace, local_part, state) {
            Declaration::Event(e) => {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.event(&e);
            }
            Declaration::States(p) => {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.states(&p);
            }
            Declaration::Property(p) => {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.property(&p);
            }
            Declaration::Effect(e) => {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.effect(&e);
            }
            Declaration::Style(s) => {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.style(&s);
            }
            Declaration::DynamicProperty(n, s) => {
                self.cnh().transition(INPUT_NON_DP_CHILD);
                self.dynamic_property(&n, s.as_deref());
            }
            Declaration::Unknown(ns, l) => self.unknown(&ns, &l),
        }
    }

    /// `unknown(namespace, localPart)` (final).
    fn unknown(&mut self, _namespace: &str, _local_part: &str) {
        let parent_type = self.cnh().parent_type();
        if parent_type.get_default_property().is_some() {
            self.cnh().transition(INPUT_DP_CHILD);
            let loc_error = self.cnh().get_error() == ERR_DP_LOCATION;
            self.default_property_element(loc_error);
        } else {
            self.cnh().transition(INPUT_NON_DP_CHILD);
            self.nested_declaration();
        }
    }
}
