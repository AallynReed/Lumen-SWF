//! Ports of `flex2.compiler.as3.binding.Watcher` and its subclasses (`PropertyWatcher`, `XMLWatcher`,
//! `RepeaterComponentWatcher`, `RepeaterItemWatcher`, `EvaluationWatcher`, `ArrayElementWatcher`,
//! `FunctionReturnWatcher`) plus `ChangeEvent`: one struct whose `kind` carries the subclass.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::prefixed_pretty_printer::PrefixedPrettyPrinter;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::parser::node::NodeRef;
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::{string_hash, JHashMap, JHashSet, JavaHash};

pub type WatcherRef = Rc<RefCell<Watcher>>;

#[derive(Clone, PartialEq, Eq)]
pub struct ChangeEvent {
    name: String,
    committing: bool,
}

impl ChangeEvent {
    pub fn new(name: &str, committing: bool) -> ChangeEvent {
        ChangeEvent { name: name.to_string(), committing }
    }

    pub fn get_committing(&self) -> bool {
        self.committing
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
}

impl JavaHash for ChangeEvent {
    fn java_hash(&self) -> i32 {
        string_hash(&self.name)
    }
}

/// A `childWatchers` key: the property (`String`) of a `PropertyWatcher`, else the id (`Integer`).
#[derive(Clone, PartialEq, Eq)]
pub enum ChildKey {
    Property(String),
    Id(i32),
}

impl JavaHash for ChildKey {
    fn java_hash(&self) -> i32 {
        match self {
            ChildKey::Property(s) => string_hash(s),
            ChildKey::Id(i) => *i,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Property,
    Xml,
    RepeaterComponent(i32),
}

pub struct Evaluation {
    args: Option<NodeRef>,
    cx: ContextRef,
    binding_expression: BindingExpressionRef,
    parent_watcher: Option<Weak<RefCell<Watcher>>>,
}

pub enum WatcherKind {
    Property { property: String, suppressed: bool, static_property: bool, sub: PropertyKind },
    RepeaterItem,
    ArrayElement(Evaluation),
    FunctionReturn { eval: Evaluation, function_name: String, is_style_watcher: bool },
}

pub struct Watcher {
    child_watchers: JHashMap<ChildKey, WatcherRef>,
    id: i32,
    parent: Option<Weak<RefCell<Watcher>>>,
    is_part_of_anon_object_graph: bool,
    change_events: JHashSet<ChangeEvent>,
    should_write_children: bool,
    operation: bool,
    class_name: Option<String>,
    /// A `TreeSet` ordered by `BindingExpression.compareTo`.
    binding_expressions: Option<Vec<BindingExpressionRef>>,
    pub kind: WatcherKind,
}

impl Watcher {
    fn new(id: i32, kind: WatcherKind) -> WatcherRef {
        Rc::new(RefCell::new(Watcher {
            child_watchers: JHashMap::new(),
            id,
            parent: None,
            is_part_of_anon_object_graph: false,
            change_events: JHashSet::new(),
            should_write_children: true,
            operation: false,
            class_name: None,
            binding_expressions: None,
            kind,
        }))
    }

    pub fn property(id: i32, property: &str) -> WatcherRef {
        Self::new(id, WatcherKind::Property { property: property.to_string(), suppressed: false, static_property: false, sub: PropertyKind::Property })
    }

    pub fn xml(id: i32, property: &str) -> WatcherRef {
        Self::new(id, WatcherKind::Property { property: property.to_string(), suppressed: false, static_property: false, sub: PropertyKind::Xml })
    }

    pub fn repeater_component(id: i32, property: &str, repeater_level: i32) -> WatcherRef {
        Self::new(id, WatcherKind::Property { property: property.to_string(), suppressed: false, static_property: false, sub: PropertyKind::RepeaterComponent(repeater_level) })
    }

    pub fn repeater_item(id: i32) -> WatcherRef {
        Self::new(id, WatcherKind::RepeaterItem)
    }

    pub fn array_element(id: i32, binding_expression: &BindingExpressionRef, args: Option<NodeRef>, cx: &ContextRef) -> WatcherRef {
        Self::new(id, WatcherKind::ArrayElement(Evaluation { args, cx: cx.clone(), binding_expression: binding_expression.clone(), parent_watcher: None }))
    }

    pub fn function_return(id: i32, binding_expression: &BindingExpressionRef, function_name: &str, args: Option<NodeRef>, cx: &ContextRef) -> WatcherRef {
        Self::new(
            id,
            WatcherKind::FunctionReturn {
                eval: Evaluation { args, cx: cx.clone(), binding_expression: binding_expression.clone(), parent_watcher: None },
                function_name: function_name.to_string(),
                is_style_watcher: false,
            },
        )
    }

    /// `getClass().getName()`.
    pub fn java_class_name(&self) -> &'static str {
        match &self.kind {
            WatcherKind::Property { sub: PropertyKind::Property, .. } => "flex2.compiler.as3.binding.PropertyWatcher",
            WatcherKind::Property { sub: PropertyKind::Xml, .. } => "flex2.compiler.as3.binding.XMLWatcher",
            WatcherKind::Property { sub: PropertyKind::RepeaterComponent(_), .. } => "flex2.compiler.as3.binding.RepeaterComponentWatcher",
            WatcherKind::RepeaterItem => "flex2.compiler.as3.binding.RepeaterItemWatcher",
            WatcherKind::ArrayElement(_) => "flex2.compiler.as3.binding.ArrayElementWatcher",
            WatcherKind::FunctionReturn { .. } => "flex2.compiler.as3.binding.FunctionReturnWatcher",
        }
    }

    pub fn is_property_watcher(&self) -> bool {
        matches!(self.kind, WatcherKind::Property { .. })
    }

    pub fn is_xml_watcher(&self) -> bool {
        matches!(self.kind, WatcherKind::Property { sub: PropertyKind::Xml, .. })
    }

    pub fn is_array_element_watcher(&self) -> bool {
        matches!(self.kind, WatcherKind::ArrayElement(_))
    }

    pub fn is_function_return_watcher(&self) -> bool {
        matches!(self.kind, WatcherKind::FunctionReturn { .. })
    }

    pub fn add_binding_expression(&mut self, binding_expression: &BindingExpressionRef) {
        let list = self.binding_expressions.get_or_insert_with(Vec::new);
        match list.binary_search_by(|b| b.compare_to(binding_expression).cmp(&0)) {
            Ok(_) => {}
            Err(i) => list.insert(i, binding_expression.clone()),
        }
    }

    pub fn get_binding_expressions(&self) -> Option<&[BindingExpressionRef]> {
        self.binding_expressions.as_deref()
    }

    pub fn get_change_events(&self) -> Vec<ChangeEvent> {
        self.change_events.iter().cloned().collect()
    }

    pub fn get_class_name(&self) -> Option<String> {
        self.class_name.clone()
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn get_parent(&self) -> Option<WatcherRef> {
        self.parent.as_ref().and_then(|p| p.upgrade())
    }

    pub fn should_write_children(&self) -> bool {
        match &self.kind {
            WatcherKind::ArrayElement(_) => self.should_write_self(),
            _ => self.should_write_children,
        }
    }

    pub fn is_part_of_anon_object_graph(&self) -> bool {
        self.is_part_of_anon_object_graph
    }

    pub fn set_part_of_anon_object_graph(&mut self, b: bool) {
        self.is_part_of_anon_object_graph = b;
    }

    pub fn set_should_write_children(&mut self, b: bool) {
        self.should_write_children = b;
    }

    pub fn should_write_self(&self) -> bool {
        match &self.kind {
            WatcherKind::Property { sub: PropertyKind::Xml, .. } => !self.get_parent().is_some_and(|p| p.borrow().is_xml_watcher()),
            WatcherKind::Property { suppressed, .. } => {
                let mut result = !*suppressed;
                if *suppressed {
                    for watcher in self.child_watchers.values() {
                        let w = watcher.borrow();
                        if !w.is_array_element_watcher() && w.should_write_self() {
                            result = true;
                            break;
                        }
                    }
                }
                result
            }
            WatcherKind::RepeaterItem => true,
            WatcherKind::ArrayElement(_) => match self.get_parent() {
                Some(p) => {
                    let p = p.borrow();
                    !(p.is_xml_watcher() || !p.should_write_self())
                }
                None => true,
            },
            WatcherKind::FunctionReturn { is_style_watcher, .. } => !self.child_watchers.is_empty() || !self.change_events.is_empty() || *is_style_watcher,
        }
    }

    pub fn add_child(this: &WatcherRef, child: &WatcherRef) {
        let key = match &child.borrow().kind {
            WatcherKind::Property { property, .. } => ChildKey::Property(property.clone()),
            _ => ChildKey::Id(child.borrow().id),
        };
        this.borrow_mut().child_watchers.insert(key, child.clone());
        child.borrow_mut().parent = Some(Rc::downgrade(this));
    }

    /// `getChild(property)`: the child under that property key (a `PropertyWatcher`).
    pub fn get_child(&self, property: &str) -> Option<WatcherRef> {
        self.child_watchers.get(&ChildKey::Property(property.to_string())).cloned()
    }

    pub fn get_children(&self) -> Vec<WatcherRef> {
        self.child_watchers.values().cloned().collect()
    }

    pub fn add_change_event(&mut self, name: &str) {
        self.add_change_event_validate(name, true);
    }

    pub fn add_change_event_validate(&mut self, name: &str, validate: bool) {
        self.change_events.insert(ChangeEvent::new(name, validate));
    }

    pub fn is_operation(&self) -> bool {
        self.operation
    }

    pub fn set_class_name(&mut self, class_name: &str) {
        self.class_name = Some(name_formatter::to_dot(class_name));
    }

    pub fn set_operation(&mut self, operation: bool) {
        self.operation = operation;
    }

    pub fn get_property(&self) -> Option<String> {
        match &self.kind {
            WatcherKind::Property { property, .. } => Some(property.clone()),
            _ => None,
        }
    }

    pub fn get_static_property(&self) -> bool {
        matches!(self.kind, WatcherKind::Property { static_property: true, .. })
    }

    pub fn set_static_property(&mut self, b: bool) {
        if let WatcherKind::Property { static_property, .. } = &mut self.kind {
            *static_property = b;
        }
    }

    pub fn suppress(&mut self) {
        if let WatcherKind::Property { suppressed, .. } = &mut self.kind {
            *suppressed = true;
        }
    }

    pub fn get_path_to_property(&self) -> String {
        let property = self.get_property().unwrap_or_default();
        match self.get_parent() {
            Some(p) if p.borrow().is_property_watcher() => format!("{}.{property}", p.borrow().get_path_to_property()),
            _ => property,
        }
    }

    pub fn get_repeater_level(&self) -> i32 {
        match self.kind {
            WatcherKind::Property { sub: PropertyKind::RepeaterComponent(l), .. } => l,
            _ => 0,
        }
    }

    fn evaluation(&self) -> Option<&Evaluation> {
        match &self.kind {
            WatcherKind::ArrayElement(e) => Some(e),
            WatcherKind::FunctionReturn { eval, .. } => Some(eval),
            _ => None,
        }
    }

    /// `EvaluationWatcher.getEvaluationPart()`.
    pub fn get_evaluation_part(&self) -> String {
        let Some(e) = self.evaluation() else { return String::new() };
        match &e.args {
            Some(args) => {
                let mut p = PrefixedPrettyPrinter::new("target");
                p.print(&e.cx, args);
                p.into_string()
            }
            None => String::new(),
        }
    }

    pub fn get_binding_expression(&self) -> Option<BindingExpressionRef> {
        self.evaluation().map(|e| e.binding_expression.clone())
    }

    pub fn set_parent_watcher(&mut self, parent_watcher: &WatcherRef) {
        match &mut self.kind {
            WatcherKind::ArrayElement(e) => e.parent_watcher = Some(Rc::downgrade(parent_watcher)),
            WatcherKind::FunctionReturn { eval, .. } => eval.parent_watcher = Some(Rc::downgrade(parent_watcher)),
            _ => {}
        }
    }

    pub fn get_function_name(&self) -> Option<String> {
        match &self.kind {
            WatcherKind::FunctionReturn { function_name, .. } => Some(function_name.clone()),
            _ => None,
        }
    }

    pub fn is_style_watcher(&self) -> bool {
        matches!(self.kind, WatcherKind::FunctionReturn { is_style_watcher: true, .. })
    }

    pub fn set_style_watcher(&mut self, b: bool) {
        if let WatcherKind::FunctionReturn { is_style_watcher, .. } = &mut self.kind {
            *is_style_watcher = b;
        }
    }
}
