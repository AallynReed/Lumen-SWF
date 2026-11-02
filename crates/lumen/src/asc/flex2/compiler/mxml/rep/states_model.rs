//! Port of `flex2.compiler.mxml.rep.StatesModel` (text codegen only): the document's states, their overrides
//! (`AddItems`, `SetProperty`, `SetStyle`, `SetEventHandler`), shared item factories, reparenting and early
//! (itemCreationPolicy="immediate") instances.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use super::binding_expression::BindingExpression;
use super::deps::{self, type_eq, Node, StandardDefs, Type};
use super::document_info::DocumentInfoRef;
use super::init::initializer::InitializerRef;
use super::model::{Model, ModelRef, RValue};
use super::mxml_document::{JKey, MxmlDocument, MxmlDocumentRef};
use super::script::Script;
use crate::asc::flex2::compiler::mxml::gen::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::mxml::gen::states_generator::INDENT;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::util::jhash::{JHashMap, JHashSet};

/// `StatesModel.SharedObject`.
pub struct SharedObject {
    pub name: String,
    pub is_declared: bool,
    pub model: ModelRef,
}

/// `StatesModel.ReparentInfo`.
struct ReparentInfo {
    states: Vec<String>,
    model: ModelRef,
    #[allow(dead_code)]
    node: Node,
    child_index: i32,
}

/// `StatesModel.Override` and its subclasses.
pub enum OverrideKind {
    AddItems {
        context: Option<String>,
        property_name: Option<String>,
        factory: String,
        parent: Option<ModelRef>,
        model: ModelRef,
        position: Option<String>,
        relative_nodes: Option<Vec<ModelRef>>,
        is_style: bool,
        is_array: bool,
        vector_class_name: Option<String>,
    },
    /// `SetPropertyOverride`, or `SetStyleOverride` when `is_style_override`.
    SetProperty { context: Option<ModelRef>, value: RefCell<InitializerRef>, property: String, clear: bool, factory: Option<String>, is_style_override: bool },
    SetEvent { context: Option<ModelRef>, event: String, value: InitializerRef, clear: bool },
}

pub struct Override {
    pub declaration: RefCell<Option<String>>,
    pub kind: OverrideKind,
}

pub type OverrideRef = Rc<Override>;

/// `StatesModel.State`.
pub struct State {
    pub name: String,
    pub overrides: RefCell<Vec<OverrideRef>>,
    pub model: RefCell<Option<ModelRef>>,
}

pub struct StatesModel {
    pub info: DocumentInfoRef,
    document: Weak<MxmlDocument>,
    pub standard_defs: Rc<StandardDefs>,
    pub states: RefCell<JHashMap<String, Rc<State>>>,
    pub shared_objects: RefCell<BTreeMap<JKey, Rc<SharedObject>>>,
    initialized_models: RefCell<Vec<ModelRef>>,
    pub early_init_objects: RefCell<Vec<String>>,
    reparent_nodes: RefCell<Option<JHashMap<String, Vec<Rc<ReparentInfo>>>>>,
    stateful_models: RefCell<Vec<ModelRef>>,
    states_defined: RefCell<JHashSet<String>>,
}

fn log_error(text: String, path: &str, line: i32) {
    toolkit::log(CompilerMessage::error(text), Some(path), line, -1, None);
}

/// `new HashSet<String>(...)` iteration order of `items`.
fn hash_set_order(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut set = JHashSet::new();
    for i in items {
        set.insert(i);
    }
    set.iter().cloned().collect()
}

impl StatesModel {
    pub fn new(doc: &MxmlDocumentRef, info: &DocumentInfoRef, defs: Rc<StandardDefs>) -> StatesModel {
        if !info.get_state_names().is_empty() && info.get_version() >= 4 {
            info.add_import_name(&name_formatter::to_dot(&defs.class_state), 0);
            info.add_interface_name(&name_formatter::to_dot(&defs.interface_istateclient2), 0);
        }
        StatesModel {
            info: info.clone(),
            document: Rc::downgrade(doc),
            standard_defs: defs,
            states: RefCell::new(JHashMap::new()),
            shared_objects: RefCell::new(BTreeMap::new()),
            initialized_models: RefCell::new(Vec::new()),
            early_init_objects: RefCell::new(Vec::new()),
            reparent_nodes: RefCell::new(None),
            stateful_models: RefCell::new(Vec::new()),
            states_defined: RefCell::new(JHashSet::new()),
        }
    }

    pub(crate) fn document(&self) -> MxmlDocumentRef {
        self.document.upgrade().expect("document dropped")
    }

    pub(crate) fn array_type(&self) -> Type {
        self.document().get_type_table().array_type()
    }

    pub fn validate_state(&self, state: Option<&str>) -> bool {
        let Some(state) = state else { return false };
        self.info.has_state_name(state) || self.info.get_state_group(state).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.states.borrow().is_empty()
    }

    pub fn set_initial_state(&self) {
        if self.info.get_version() >= 4 {
            let root = self.document().get_root();
            let states = self.info.get_state_names();
            if !states.is_empty() && !root.has_property(deps::PROP_CURRENTSTATE) {
                root.set_property(deps::PROP_CURRENTSTATE, RValue::Str(states[0].clone()), 0);
            }
        }
    }

    pub fn register_state(&self, model: &ModelRef, node: &Node) {
        if let Some(name) = deps::node_attribute_value(node, "", "name") {
            if self.states_defined.borrow().contains(&name) {
                log_error(format!("Multiple definitions for state '{name}'."), &self.document().get_source_path(), model.get_xml_line_number());
                return;
            }
            let state = self.state_by_name(&name);
            *state.model.borrow_mut() = Some(model.clone());
            self.states_defined.borrow_mut().insert(name);
        }
    }

    /// Siblings of a stateful `context` (its children or entries), as initializers.
    fn siblings_of(context: &ModelRef) -> Vec<InitializerRef> {
        if context.is_movie_clip() {
            context.children()
        } else {
            context.get_entries()
        }
    }

    fn is_list_context(&self, context: &ModelRef) -> bool {
        type_eq(context.get_type(), &self.array_type()) || context.get_type().get_element_type().is_some()
    }

    pub fn register_state_specific_node(&self, model: &ModelRef, node: &Node, included_states: &[String], excluded_states: &[String]) {
        self.stateful_models.borrow_mut().push(model.clone());
        let mut context = model.get_parent();
        let ctx = context.clone().expect("NullPointerException");
        if deps::node_local_part(node) == "Reparent" {
            let count = self.get_sibling_count(&ctx);
            self.queue_reparent(model, node, included_states, excluded_states, count);
            return;
        }

        let mut relative_nodes = None;
        let mut position = None;
        if self.is_list_context(&ctx) || ctx.is_movie_clip() {
            let siblings = Self::siblings_of(&ctx);
            let start = if siblings.is_empty() { -1 } else { siblings.len() as i32 - 1 };
            let rn = self.build_relative_sibling_list(model, &siblings, start);
            position = Some(if rn.is_empty() { "first" } else { "after" });
            relative_nodes = Some(rn);
        }

        let mut parent_property = None;
        if self.is_list_context(&ctx) {
            parent_property = ctx.get_parent_index().or_else(|| ctx.get_id());
            context = ctx.get_parent();
            model.ensure_declaration();
        }

        if model.is_array() {
            model.ensure_declaration();
            model.ensure_bindable();
        }

        if let Some(c) = &context {
            c.ensure_declaration();
            c.ensure_bindable();
        }

        let states = self.consolidate_state_names(included_states, excluded_states);
        model.set_state_specific(true);
        model.set_states(Some(states.clone()));
        model.set_descriptor_init(false);

        let destination = context.clone().filter(|c| c.get_parent().is_some());
        let parent_type = match &destination {
            Some(d) => d.get_type().clone(),
            None => self.document().get_root().get_type().clone(),
        };

        if self.standard_defs.is_items_component(&parent_type) && parent_type.get_default_property().map(|p| p.get_name().unwrap_or("null").to_string()) == parent_property {
            parent_property = None;
        }

        if !self.validate_stateful_model_type(model, destination.as_ref(), parent_property.as_deref()) {
            return;
        }

        self.construct_add_items(model, destination.as_ref(), parent_property.as_deref(), context.as_ref().and_then(|c| c.get_id()).as_deref(), position, relative_nodes, &states);
    }

    fn process_reparent_node(&self, reparent_node: &Rc<ReparentInfo>, target_model: &ModelRef) -> bool {
        let mut context = reparent_node.model.get_parent();
        let ctx = context.clone().expect("NullPointerException");
        let mut relative_nodes = None;
        let mut position = None;
        if self.is_list_context(&ctx) || ctx.is_movie_clip() {
            let siblings = Self::siblings_of(&ctx);
            let rn = self.build_relative_sibling_list(target_model, &siblings, reparent_node.child_index - 1);
            position = Some(if rn.is_empty() { "first" } else { "after" });
            relative_nodes = Some(rn);
        }

        let mut parent_property = None;
        if self.is_list_context(&ctx) {
            parent_property = ctx.get_parent_index().or_else(|| ctx.get_id());
            context = ctx.get_parent();
        }

        if let Some(c) = context.as_ref().filter(|c| c.get_id_is_autogenerated()) {
            self.document().add_declaration_named(&c.get_id().unwrap_or_default(), &c.get_type().get_name().to_string(), c.get_xml_line_number(), true, true, true, true, None);
        }

        let destination = context.clone().filter(|c| c.get_parent().is_some());
        let parent_type = match &destination {
            Some(d) => d.get_type().clone(),
            None => self.document().get_root().get_type().clone(),
        };

        if self.standard_defs.is_items_component(&parent_type) && parent_type.get_default_property().map(|p| p.get_name().unwrap_or("null").to_string()) == parent_property {
            parent_property = None;
        }

        if !self.validate_reparent_states(&reparent_node.states, reparent_node, target_model) {
            return false;
        }

        if !self.validate_reparent_type(target_model, destination.as_ref(), parent_property.as_deref(), reparent_node) {
            return false;
        }

        self.construct_add_items(target_model, destination.as_ref(), parent_property.as_deref(), context.as_ref().and_then(|c| c.get_id()).as_deref(), position, relative_nodes, &reparent_node.states);
        true
    }

    pub fn apply_metadata(&self) {
        let states = self.info.get_state_names();
        if !states.is_empty() {
            let mut metadata = String::from("[States(");
            for (i, state) in states.iter().enumerate() {
                metadata.push('"');
                metadata.push_str(state);
                metadata.push('"');
                if i + 1 < states.len() {
                    metadata.push_str(", ");
                }
            }
            metadata.push_str(")]");
            self.document().add_metadata(Script::from_text(&metadata));
        }
    }

    fn queue_reparent(&self, model: &ModelRef, node: &Node, included_states: &[String], excluded_states: &[String], child_index: i32) {
        let states = self.consolidate_state_names(included_states, excluded_states);
        model.set_states(Some(states.clone()));
        let id = model.get_id().unwrap_or_default();
        let mut rn = self.reparent_nodes.borrow_mut();
        let map = rn.get_or_insert_with(JHashMap::new);
        let mut nodes = map.get(&id).cloned().unwrap_or_default();
        nodes.push(Rc::new(ReparentInfo { states, model: model.clone(), node: node.clone(), child_index }));
        map.insert(id, nodes);
    }

    fn ensure_compatible_ancestors(&self, model: &ModelRef, destination: Option<&ModelRef>, states: &[String], start_node: &ModelRef) -> bool {
        let path = self.document().get_source_path();
        let mut current_model = destination.cloned();
        while let Some(current) = current_model {
            if Rc::ptr_eq(start_node, &current) {
                log_error(format!("Circular reference detected within state '{}' due to Reparent tag.", states.first().cloned().unwrap_or_default()), &path, current.get_xml_line_number());
                return false;
            }

            let extra_states = self.check_for_extra_states(&current, states);
            for state in extra_states {
                match self.get_reparent_for_state(&current, &state) {
                    Some(reparent) => {
                        if !self.ensure_compatible_ancestors(model, Some(&reparent), std::slice::from_ref(&state), start_node) {
                            return false;
                        }
                    }
                    None => {
                        log_error(format!("Component cannot be realized within state '{state}' because an ancestor is excluded from '{state}'. "), &path, model.get_xml_line_number());
                        return false;
                    }
                }
            }

            if let Some(pis) = current.get_parent_index_state() {
                let state = states.first().cloned().unwrap_or_default();
                if states.len() != 1 || state != pis {
                    log_error(format!("Component cannot be realized within state '{state}' because an ancestor is excluded from '{state}'. "), &path, model.get_xml_line_number());
                    return false;
                }
            }

            current_model = current.get_parent();
        }
        true
    }

    fn check_for_extra_states(&self, ancestor: &ModelRef, states: &[String]) -> Vec<String> {
        if ancestor.is_state_specific() {
            states.iter().filter(|s| !ancestor.has_state(s)).cloned().collect()
        } else {
            Vec::new()
        }
    }

    fn validate_reparent_states(&self, states: &[String], reparent_node: &Rc<ReparentInfo>, target: &ModelRef) -> bool {
        let path = self.document().get_source_path();
        let target_id = target.get_id().unwrap_or_default();
        let list = self.reparent_nodes.borrow().as_ref().and_then(|m| m.get(&target_id).cloned()).unwrap_or_default();
        for current_state in states {
            if !target.is_state_specific() || target.has_state(current_state) {
                log_error(format!("Cannot reparent target '{target_id}'.  It already exists in state ('{current_state}')."), &path, reparent_node.model.get_xml_line_number());
                return false;
            }
            for node in &list {
                if !Rc::ptr_eq(node, reparent_node) && node.states.contains(current_state) {
                    log_error(format!("Cannot reparent the same target '{target_id}' to multiple destinations within the same state ('{current_state}')."), &path, reparent_node.model.get_xml_line_number());
                    return false;
                }
            }
        }
        true
    }

    fn validate_reparent_type(&self, target_model: &ModelRef, destination: Option<&ModelRef>, parent_property: Option<&str>, reparent_node: &Rc<ReparentInfo>) -> bool {
        let child_type = target_model.get_type();
        let parent_type = match destination {
            Some(d) => d.get_type().clone(),
            None => self.document().get_root().get_type().clone(),
        };
        let sd = &self.standard_defs;

        if sd.is_container(&parent_type) && sd.is_iui_component(child_type) && parent_property.is_none() {
            return true;
        } else if parent_property.is_some() || parent_type.get_default_property().is_some() {
            let property = match parent_property {
                Some(p) => parent_type.get_property(p),
                None => parent_type.get_default_property(),
            };
            let style = parent_property.and_then(|p| parent_type.get_style(p));

            if sd.is_items_component(&parent_type) {
                let prop_name = property.as_ref().expect("NullPointerException").get_name().unwrap_or("null").to_string();
                if Some(prop_name) == parent_type.get_default_property().map(|p| p.get_name().unwrap_or("null").to_string()) {
                    return true;
                }
            }

            if let Some(property) = &property {
                let pt = property.get_type().expect("NullPointerException");
                if type_eq(&pt, &self.array_type()) || pt.get_element_type().is_some() {
                    match property.get_element_type() {
                        None => return true,
                        Some(et) if child_type.is_assignable_to(Some(&et)) => return true,
                        _ => {}
                    }
                } else if self.is_stateful_compatible_type(&pt) {
                    return true;
                } else if style.as_ref().is_some_and(|s| self.is_stateful_compatible_type(&s.get_type().expect("NullPointerException"))) {
                    return true;
                }
            } else if style.as_ref().is_some_and(|s| self.is_stateful_compatible_type(&s.get_type().expect("NullPointerException"))) {
                return true;
            }
        }
        log_error(
            format!("Cannot reparent target '{}' to specified destination.  Incompatible type or scalar property.", target_model.get_id().unwrap_or_default()),
            &self.document().get_source_path(),
            reparent_node.model.get_xml_line_number(),
        );
        false
    }

    fn validate_stateful_model_type(&self, target_model: &ModelRef, destination: Option<&ModelRef>, parent_property: Option<&str>) -> bool {
        let parent_type = match destination {
            Some(d) => d.get_type().clone(),
            None => self.document().get_root().get_type().clone(),
        };
        let property = match parent_property {
            Some(p) => parent_type.get_property(p),
            None => parent_type.get_default_property(),
        };
        let style = parent_property.and_then(|p| parent_type.get_style(p));

        if let Some(property) = &property {
            if self.standard_defs.is_items_component(&parent_type) && Some(property.get_name().unwrap_or("null").to_string()) == parent_type.get_default_property().map(|p| p.get_name().unwrap_or("null").to_string()) {
                return true;
            }
        }

        let path = self.document().get_source_path();
        if property.as_ref().is_some_and(|p| !self.is_stateful_compatible_type(&p.get_type().expect("NullPointerException"))) {
            log_error("Scalar value nodes may not be state-specific.".to_string(), &path, target_model.get_xml_line_number());
            return false;
        } else if property.is_none() && style.as_ref().is_some_and(|s| !self.is_stateful_compatible_type(&s.get_type().expect("NullPointerException"))) {
            log_error("Scalar value nodes may not be state-specific.".to_string(), &path, target_model.get_xml_line_number());
            return false;
        }
        true
    }

    fn is_stateful_compatible_type(&self, t: &Type) -> bool {
        let tt = self.document().get_type_table();
        type_eq(t, &tt.array_type()) || t.get_element_type().is_some() || type_eq(t, &tt.object_type()) || type_eq(t, &tt.no_type())
    }

    fn consolidate_state_names(&self, included_states: &[String], excluded_states: &[String]) -> Vec<String> {
        let states = if !included_states.is_empty() { included_states.to_vec() } else { self.inverse_of(excluded_states) };
        self.expand_state_list(&states)
    }

    fn expand_state(&self, state: &str) -> Vec<String> {
        match self.info.get_state_group(state) {
            Some(group) => hash_set_order(group),
            None => hash_set_order([state.to_string()]),
        }
    }

    fn expand_state_list(&self, state_list: &[String]) -> Vec<String> {
        let mut expanded = Vec::new();
        for state in state_list {
            match self.info.get_state_group(state) {
                Some(group) => expanded.extend(group),
                None => expanded.push(state.clone()),
            }
        }
        hash_set_order(expanded)
    }

    fn get_sibling_count(&self, model: &ModelRef) -> i32 {
        if self.is_list_context(model) || model.is_movie_clip() {
            return Self::siblings_of(model).len() as i32;
        }
        0
    }

    pub fn process_reparents(&self) -> bool {
        let entries: Vec<Vec<Rc<ReparentInfo>>> = match self.reparent_nodes.borrow().as_ref() {
            Some(m) if !m.is_empty() => m.values().cloned().collect(),
            _ => return true,
        };
        for list in entries {
            for node in &list {
                let target = node.model.get_id().unwrap_or_default();
                let decl = self.document().get_declaration(&target);
                match decl.as_ref().and_then(|d| d.as_initializer()).and_then(|i| i.as_value().and_then(|v| v.get_model())) {
                    Some(model) => {
                        if !self.process_reparent_node(node, &model) {
                            return false;
                        }
                    }
                    None => {
                        log_error(format!("Cannot resolve the specified reparent target '{target}'."), &self.document().get_source_path(), node.model.get_xml_line_number());
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn process_stateful_models(&self) {
        let models = self.stateful_models.borrow().clone();
        for model in models {
            self.ensure_compatible_ancestors(&model, model.get_parent().as_ref(), &model.get_states().unwrap_or_default(), &model);
        }
    }

    fn build_relative_sibling_list(&self, model: &ModelRef, siblings: &[InitializerRef], start_index: i32) -> Vec<ModelRef> {
        let mut relative_siblings = Vec::new();
        let mut i = start_index;
        while i > -1 {
            let sibling = siblings[i as usize].as_value().and_then(|v| v.get_model()).expect("ClassCastException");
            let mut intersects = false;
            if sibling.is_state_specific() {
                for state in sibling.get_states().unwrap_or_default() {
                    if model.has_state(&state) {
                        intersects = true;
                        break;
                    }
                }
            }
            if !sibling.is_state_specific() || intersects {
                relative_siblings.push(sibling.clone());
                if sibling.get_id_is_autogenerated() && !sibling.is_reparent() {
                    self.document().add_declaration_named(&sibling.get_id().unwrap_or_default(), &sibling.get_type().get_name().to_string(), sibling.get_xml_line_number(), true, true, true, sibling.get_bindability_ensured(), None);
                }
            }
            if !sibling.is_state_specific() {
                break;
            }
            i -= 1;
        }
        relative_siblings
    }

    pub fn register_state_specific_property(&self, model: Option<&ModelRef>, property: &str, value: &InitializerRef, state_name: Option<&str>) {
        self.generate_property_overrides(model, property, value, state_name, false);
    }

    pub fn register_state_specific_style(&self, model: Option<&ModelRef>, property: &str, value: &InitializerRef, state_name: Option<&str>) {
        self.generate_property_overrides(model, property, value, state_name, true);
    }

    fn generate_property_overrides(&self, model: Option<&ModelRef>, property: &str, value: &InitializerRef, state_name: Option<&str>, is_style: bool) {
        let state_names = self.expand_state(state_name.unwrap_or("null"));
        let mut factory = None;
        let vi = value.as_value().expect("ValueInitializer");
        if state_names.len() > 1 && !vi.get_value().is_binding() && vi.has_definition() {
            if let Some(value_model) = vi.get_model() {
                let name = value_model.get_definition_name().unwrap_or_default();
                factory = Some(name.clone());
                self.shared_objects.borrow_mut().insert(JKey(name.clone()), Rc::new(SharedObject { name, is_declared: value_model.is_declared(), model: value_model.clone() }));
            }
        }
        for name in state_names {
            let state = self.state_by_name(&name);
            let o = self.new_set_property_override(model.cloned(), property, value.clone(), factory.clone(), is_style);
            self.post_process_binding_instance(value, &o);
            state.add_override(self, o, model.map(|m| m.get_xml_line_number()).unwrap_or(0));
        }
    }

    pub fn register_state_specific_event_handler(&self, model: Option<&ModelRef>, event: &str, value: &InitializerRef, state_name: Option<&str>) {
        for name in self.expand_state(state_name.unwrap_or("null")) {
            let state = self.state_by_name(&name);
            let o = self.new_set_event_override(model.cloned(), event, value.clone());
            state.add_override(self, o, model.map(|m| m.get_xml_line_number()).unwrap_or(0));
        }
    }

    pub fn register_early_init_node(&self, model: &ModelRef) {
        let name = model.get_definition_name().unwrap_or_default();
        self.shared_objects.borrow_mut().insert(JKey(name.clone()), Rc::new(SharedObject { name: name.clone(), is_declared: model.is_declared(), model: model.clone() }));
        self.early_init_objects.borrow_mut().push(name);
        model.set_early_init(true);
    }

    #[allow(clippy::too_many_arguments)]
    fn construct_add_items(&self, model: &ModelRef, parent: Option<&ModelRef>, parent_index: Option<&str>, parent_id: Option<&str>, position: Option<&'static str>, relative_nodes: Option<Vec<ModelRef>>, states_list: &[String]) {
        let def_name = model.get_definition_name().unwrap_or_default();
        self.shared_objects.borrow_mut().insert(JKey(def_name.clone()), Rc::new(SharedObject { name: def_name.clone(), is_declared: model.is_declared(), model: model.clone() }));

        let is_style = match (parent, parent_index) {
            (Some(p), Some(pi)) => p.get_type().get_style(pi).is_some(),
            _ => false,
        };
        let is_array = type_eq(model.get_type(), &self.array_type());
        let model_parent = model.get_parent().expect("NullPointerException");
        let vector_class_name = model_parent.get_type().get_element_type().map(|_| name_formatter::to_dot(&model_parent.get_type().get_name().to_string()));

        for name in hash_set_order(states_list.iter().cloned()) {
            let state = self.state_by_name(&name);
            let o = self.new_add_items_override(
                parent.cloned(),
                parent_index.map(Into::into),
                parent_id.map(Into::into),
                def_name.clone(),
                position.map(Into::into),
                relative_nodes.clone(),
                is_style,
                is_array,
                vector_class_name.clone(),
                model.clone(),
            );
            state.add_override(self, o, model.get_xml_line_number());
        }
    }

    fn post_process_binding_instance(&self, value: &InitializerRef, o: &OverrideRef) {
        let vi = value.as_value().expect("ValueInitializer");
        let RValue::Binding(source_expression) = vi.get_value() else { return };
        let document = self.document();
        let declared_type = o.get_declared_type(self);
        let t = document.get_type_table().get_type(&declared_type).expect("NullPointerException");
        let model = Model::with_parent(&document, t, None, 0);
        document.ensure_declaration(&model);
        model.ensure_bindable();

        let binding_expression = BindingExpression::new(&source_expression.get_source_expression(), source_expression.xml_line_number.get(), &document);
        binding_expression.set_destination(&model);
        binding_expression.set_destination_lvalue(Some("value"));
        binding_expression.set_destination_property(Some("value"));
        binding_expression.set_destination_style(None);
        *o.declaration.borrow_mut() = model.get_id();
        source_expression.set_id(-1);
        document.remove_binding_expression(&source_expression);

        let value_clone = vi.clone_with_value(RValue::Binding(binding_expression));
        if let OverrideKind::SetProperty { value, .. } = &o.kind {
            *value.borrow_mut() = value_clone;
        }
    }

    fn inverse_of(&self, list: &[String]) -> Vec<String> {
        let expanded = self.expand_state_list(list);
        let mut inverse: Vec<String> = self.info.get_state_names().into_iter().filter(|n| !expanded.contains(n)).collect();
        inverse.sort_by(|a, b| crate::asc::util::jstr::compare(a, b));
        inverse.dedup();
        inverse
    }

    pub fn state_by_name(&self, name: &str) -> Rc<State> {
        if let Some(s) = self.states.borrow().get(name) {
            return s.clone();
        }
        let entry = Rc::new(State { name: name.to_string(), overrides: RefCell::new(Vec::new()), model: RefCell::new(None) });
        self.states.borrow_mut().insert(name.to_string(), entry.clone());
        entry
    }

    fn get_reparent_for_state(&self, model: &ModelRef, state: &str) -> Option<ModelRef> {
        let rn = self.reparent_nodes.borrow();
        let map = rn.as_ref()?;
        let list = map.get(&model.get_id().unwrap_or_default()).expect("NullPointerException");
        list.iter().find(|ri| ri.states.iter().any(|s| s == state)).map(|ri| ri.model.clone())
    }

    pub fn get_groups_for_state(&self, name: &str) -> Vec<String> {
        self.info.get_state_groups().into_iter().filter(|(_, states)| states.iter().any(|s| s == name)).map(|(g, _)| g).collect()
    }

    fn is_initialized(&self, m: &ModelRef) -> bool {
        self.initialized_models.borrow().iter().any(|x| Rc::ptr_eq(x, m))
    }

    pub fn get_sub_definition_iterators(&self) -> Vec<CodeFragmentList> {
        let mut iter_list = Vec::new();
        for name in self.info.get_state_names() {
            let state = self.state_by_name(&name);
            for ei in state.get_events() {
                iter_list.extend(ei.get_definitions_iterator());
            }
            let overrides = state.overrides.borrow().clone();
            for o in overrides {
                if let OverrideKind::SetProperty { value, .. } = &o.kind {
                    let initializer = value.borrow().clone();
                    if let Some(m) = initializer.as_value().and_then(|v| v.get_model()) {
                        if !self.is_initialized(&m) {
                            iter_list.extend(initializer.get_definitions_iterator());
                            self.initialized_models.borrow_mut().push(m);
                        }
                    }
                }
            }
        }
        iter_list
    }

    pub fn get_sub_initializer_iterators(&self) -> Vec<InitializerRef> {
        let mut iter_list = Vec::new();
        for name in self.info.get_state_names() {
            let state = self.state_by_name(&name);
            iter_list.extend(state.get_events());
            let overrides = state.overrides.borrow().clone();
            for o in overrides {
                if let OverrideKind::SetProperty { value, .. } = &o.kind {
                    let initializer = value.borrow().clone();
                    if let Some(m) = initializer.as_value().and_then(|v| v.get_model()) {
                        if !self.is_initialized(&m) {
                            iter_list.push(initializer.clone());
                            self.initialized_models.borrow_mut().push(m);
                        }
                    }
                }
            }
        }
        iter_list
    }

    #[allow(clippy::too_many_arguments)]
    fn new_add_items_override(
        &self,
        parent: Option<ModelRef>,
        property_name: Option<String>,
        context: Option<String>,
        factory: String,
        position: Option<String>,
        relative_nodes: Option<Vec<ModelRef>>,
        is_style: bool,
        is_array: bool,
        vector_class_name: Option<String>,
        model: ModelRef,
    ) -> OverrideRef {
        let o = Rc::new(Override {
            declaration: RefCell::new(None),
            kind: OverrideKind::AddItems { context, property_name, factory, parent, model, position, relative_nodes, is_style, is_array, vector_class_name },
        });
        self.info.add_import_name(&name_formatter::to_dot(&o.get_declared_type(self)), 0);
        o
    }

    fn new_set_property_override(&self, context: Option<ModelRef>, property: &str, value: InitializerRef, factory: Option<String>, is_style_override: bool) -> OverrideRef {
        let clear = matches!(value.as_value().map(|v| v.get_value()), Some(RValue::AtClear(_)));
        let o = Rc::new(Override {
            declaration: RefCell::new(None),
            kind: OverrideKind::SetProperty { context, value: RefCell::new(value), property: property.to_string(), clear, factory, is_style_override },
        });
        self.info.add_import_name(&name_formatter::to_dot(&o.get_declared_type(self)), 0);
        o
    }

    fn new_set_event_override(&self, context: Option<ModelRef>, event: &str, value: InitializerRef) -> OverrideRef {
        let clear = value.as_event().is_some_and(|e| e.get_handler_text() == "@Clear()");
        let o = Rc::new(Override { declaration: RefCell::new(None), kind: OverrideKind::SetEvent { context, event: event.to_string(), value, clear } });
        self.info.add_import_name(&name_formatter::to_dot(&o.get_declared_type(self)), 0);
        o
    }
}

impl State {
    pub fn add_override(&self, model: &StatesModel, o: OverrideRef, line: i32) {
        if !self.previously_initialized(model, &o, line) {
            let is_array_or_vector_value = match &o.kind {
                OverrideKind::SetProperty { value, .. } => value.borrow().as_value().and_then(|v| v.get_model()).is_some_and(|m| m.is_array()),
                _ => false,
            };
            let is_add_items = matches!(o.kind, OverrideKind::AddItems { .. });
            if is_array_or_vector_value || is_add_items {
                self.overrides.borrow_mut().insert(0, o);
            } else {
                self.overrides.borrow_mut().push(o);
            }
        }
    }

    pub fn get_definition_body(&self, sm: &StatesModel, list: &mut CodeFragmentList, indent: &str, bindings_queue: &mut Vec<OverrideRef>) {
        let mut indent = indent.to_string();
        list.add2(&indent, "new State ({", 0);
        indent.push_str(INDENT);
        list.add2(&indent, &format!("name: \"{}\",", self.name), 0);

        if let Some(model) = self.model.borrow().clone() {
            for init in model.get_property_initializer_iterator_bindings(false) {
                if init.is_named() {
                    let init_name = init.get_name();
                    if init_name != "name" && init_name != "stateGroups" && init_name != "overrides" {
                        list.add2(&indent, &format!("{init_name}: {},", init.get_value_expr()), 0);
                    }
                }
            }
        } else {
            panic!("NullPointerException");
        }

        let groups = sm.get_groups_for_state(&self.name);
        if !groups.is_empty() {
            let mut group_names = String::new();
            for (i, g) in groups.iter().enumerate() {
                group_names.push('\'');
                group_names.push_str(g);
                group_names.push_str(if i + 1 < groups.len() { "'," } else { "'" });
            }
            list.add2(&indent, &format!("stateGroups: [{group_names}],"), 0);
        }

        list.add2(&indent, "overrides: [", 0);
        indent.push_str(INDENT);
        let overrides = self.overrides.borrow().clone();
        for (i, o) in overrides.iter().enumerate() {
            o.get_definition_body(sm, list, &indent);
            if o.declaration.borrow().is_some() {
                bindings_queue.push(o.clone());
            }
            if i + 1 < overrides.len() {
                list.add2(&indent, ",", 0);
            }
        }
        indent.truncate(indent.len() - INDENT.len());
        list.add2(&indent, "]", 0);
        indent.truncate(indent.len() - INDENT.len());
        list.add2(&indent, "})", 0);
    }

    fn previously_initialized(&self, sm: &StatesModel, o: &OverrideRef, line: i32) -> bool {
        if is_property_override(o) {
            for current in self.overrides.borrow().iter() {
                if o.get_declared_type(sm) == current.get_declared_type(sm) {
                    let (conflict, property) = match (&o.kind, &current.kind) {
                        (OverrideKind::SetProperty { context, property, .. }, OverrideKind::SetProperty { context: cc, property: cp, .. }) => (cp == property && same_opt(cc, context), property.clone()),
                        (OverrideKind::SetEvent { context, event, .. }, OverrideKind::SetEvent { context: cc, event: ce, .. }) => (ce == event && same_opt(cc, context), event.clone()),
                        _ => (false, String::new()),
                    };
                    if conflict {
                        log_error(format!("Multiple initializers for property '{property}' in state '{}'.", self.name), &sm.document().get_source_path(), line);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `getEvents()`: the State model's event initializers.
    pub fn get_events(&self) -> Vec<InitializerRef> {
        self.model.borrow().as_ref().map(|m| m.get_event_initializer_iterator()).unwrap_or_default()
    }

    pub fn is_declared(&self) -> bool {
        self.model.borrow().as_ref().is_some_and(|m| m.is_declared())
    }

    pub fn get_id(&self) -> Option<String> {
        self.model.borrow().as_ref().and_then(|m| m.get_id())
    }
}

fn same_opt(a: &Option<ModelRef>, b: &Option<ModelRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn is_property_override(o: &Override) -> bool {
    matches!(o.kind, OverrideKind::SetProperty { .. } | OverrideKind::SetEvent { .. })
}

impl Override {
    pub fn get_declared_type(&self, sm: &StatesModel) -> String {
        let sd = &sm.standard_defs;
        match &self.kind {
            OverrideKind::AddItems { .. } => sd.class_additems.clone(),
            OverrideKind::SetProperty { is_style_override: false, .. } => sd.class_setproperty.clone(),
            OverrideKind::SetProperty { is_style_override: true, .. } => sd.class_setstyle.clone(),
            OverrideKind::SetEvent { .. } => sd.class_seteventhandler.clone(),
        }
    }

    pub fn get_definition_body(&self, sm: &StatesModel, list: &mut CodeFragmentList, indent: &str) {
        let mut indent = indent.to_string();
        let document = sm.document();
        let root = document.get_root();
        match &self.kind {
            OverrideKind::AddItems { property_name, factory, parent, model, position, relative_nodes, is_style, is_array, vector_class_name, .. } => {
                let type_name = name_formatter::retrieve_class_name(&self.get_declared_type(sm));
                list.add2(&indent, &format!("new {type_name}().initializeFromObject({{"), 0);
                indent.push_str(INDENT);
                if model.get_is_transient() {
                    list.add2(&indent, "destructionPolicy: \"auto\",", 0);
                }
                list.add2(&indent, &format!("itemsFactory: {factory}_factory,"), 0);
                if *is_style {
                    list.add2(&indent, "isStyle: true ,", 0);
                }
                let mut parent_element_count = 0;
                if let Some(p) = model.get_parent() {
                    if type_eq(p.get_type(), &sm.array_type()) || p.get_type().get_element_type().is_some() {
                        parent_element_count = p.get_entries().len();
                    }
                }
                if *is_array && parent_element_count > 1 {
                    list.add2(&indent, "isArray: true ,", 0);
                }
                if let Some(v) = vector_class_name {
                    list.add2(&indent, &format!("vectorClass: {v},"), 0);
                }
                let destination = match parent {
                    Some(p) => format!("\"{}\"", p.get_id().unwrap_or_else(|| "null".into())),
                    None => "null".to_string(),
                };
                list.add2(&indent, &format!("destination: {destination}{}", if property_name.is_some() || position.is_some() { "," } else { "" }), 0);
                if let Some(pn) = property_name {
                    list.add2(&indent, &format!("propertyName: \"{pn}\"{}", if position.is_some() { "," } else { "" }), 0);
                }
                if let Some(position) = position {
                    list.add2(&indent, &format!("position: \"{position}\"{}", if position != "first" { "," } else { "" }), 0);
                    let rn = relative_nodes.clone().unwrap_or_default();
                    if position == "after" && !rn.is_empty() {
                        let mut sibling_list = String::new();
                        for (i, n) in rn.iter().enumerate() {
                            sibling_list.push('"');
                            sibling_list.push_str(&n.get_id().unwrap_or_else(|| "null".into()));
                            sibling_list.push('"');
                            if i != rn.len() - 1 {
                                sibling_list.push_str(", ");
                            }
                        }
                        list.add2(&indent, &format!("relativeTo: [{sibling_list}]"), 0);
                    }
                }
                indent.truncate(indent.len() - INDENT.len());
                list.add2(&indent, "})", 0);
            }
            OverrideKind::SetProperty { context, value, property, clear, factory, is_style_override } => {
                let context = context.as_ref().expect("NullPointerException");
                let is_data_bound = if !is_style_override { context.has_data_bound_property(property) } else { context.has_data_bound_style(property) };
                let type_name = name_formatter::to_dot(&self.get_declared_type(sm));
                let declaration = self.declaration.borrow().clone();
                let prefix = match &declaration {
                    Some(d) => format!("{d} = {type_name}( "),
                    None => String::new(),
                };
                let suffix = if declaration.is_some() { ")" } else { "" };
                list.add2(&indent, &format!("{prefix}new {type_name}().initializeFromObject({{"), 0);
                indent.push_str(INDENT);
                if is_data_bound {
                    list.add2(&indent, "isBaseValueDataBound: true ,", 0);
                }
                if !Rc::ptr_eq(&root, context) {
                    list.add2(&indent, &format!("target: \"{}\",", context.get_id().unwrap_or_else(|| "null".into())), 0);
                }
                list.add2(&indent, &format!("name: \"{property}\"{}", if !clear { "," } else { "" }), 0);
                if !clear {
                    match factory {
                        Some(f) => list.add2(&indent, &format!("valueFactory: {f}_factory"), 0),
                        None => list.add2(&indent, &format!("value: {}", value.borrow().get_value_expr()), 0),
                    };
                }
                indent.truncate(indent.len() - INDENT.len());
                list.add2(&indent, &format!("}}){suffix}"), 0);
            }
            OverrideKind::SetEvent { context, event, value, clear } => {
                let context = context.as_ref().expect("NullPointerException");
                let type_name = name_formatter::retrieve_class_name(&self.get_declared_type(sm));
                let is_data_bound = context.has_data_bound_event(event);
                list.add2(&indent, &format!("new {type_name}().initializeFromObject({{"), 0);
                indent.push_str(INDENT);
                if is_data_bound {
                    list.add2(&indent, "isBaseValueDataBound: true ,", 0);
                }
                let base = context.get_event_initializer(event);
                if !Rc::ptr_eq(&root, context) {
                    list.add2(&indent, &format!("target: \"{}\",", context.get_id().unwrap_or_else(|| "null".into())), 0);
                }
                list.add2(&indent, &format!("name: \"{event}\"{}", if !clear || base.is_some() { "," } else { "" }), 0);
                if let Some(base) = &base {
                    list.add2(&indent, &format!("originalHandlerFunction: {}{}", base.get_value_expr(), if !clear { "," } else { "" }), 0);
                }
                if !clear {
                    list.add2(&indent, &format!("handlerFunction: {}", value.get_value_expr()), 0);
                }
                indent.truncate(indent.len() - INDENT.len());
                list.add2(&indent, "})", 0);
            }
        }
    }
}
