//! Port of `flex2.compiler.as3.binding.DataBindingFirstPassEvaluator`: walks the MXML class's `*_bindingsSetup`
//! function and builds the watcher tree of each binding expression from the resolved references.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::data_binding_info::DataBindingInfo;
use super::watcher::{Watcher, WatcherRef};
use crate::asc::flex2::compiler::as3::evaluator_adapter::{evaluate, walk, Adapter};
use crate::asc::flex2::compiler::as3::reflect::as3_class::{AbcClass, PUBLIC_NAMESPACE, VISIBILITY_NAMESPACES};
use crate::asc::flex2::compiler::as3::reflect::meta_data::MetaData;
use crate::asc::flex2::compiler::as3::reflect::method::Method;
use crate::asc::flex2::compiler::as3::reflect::node_magic;
use crate::asc::flex2::compiler::as3::reflect::variable::Variable;
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{self, StandardDefs};
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::symbol_table::{SymbolTable, NOTYPE};
use crate::asc::flex2::compiler::toolkit::Level;
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::tokens::{AS_TOKEN, EMPTY_TOKEN, GET_TOKEN, SET_TOKEN};
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::util::context::ContextRef;

const BINDABLE: &str = "Bindable";
const CHANGE_EVENT: &str = "ChangeEvent";
const NON_COMMITTING_CHANGE_EVENT: &str = "NonCommittingChangeEvent";
const EVENT: &str = "event";
const STYLE: &str = "style";
const OBJECT: &str = "Object";

type Key = *const RefCell<crate::asc::parser::node::Node>;
type WatcherList = Rc<RefCell<Vec<WatcherRef>>>;

fn new_list() -> WatcherList {
    Rc::new(RefCell::new(Vec::new()))
}

fn ptr(n: &Option<NodeRef>) -> Option<Key> {
    n.as_ref().map(Rc::as_ptr)
}

/// A `MultiName`'s namespaces; `Visibility` is `SymbolTable.VISIBILITY_NAMESPACES` itself.
enum Ns {
    Visibility,
    List(Vec<String>),
}

impl Ns {
    fn with<R>(&self, f: impl FnOnce(&[&str]) -> R) -> R {
        match self {
            Ns::Visibility => f(&VISIBILITY_NAMESPACES),
            Ns::List(v) => {
                let refs: Vec<&str> = v.iter().map(String::as_str).collect();
                f(&refs)
            }
        }
    }
}

pub struct DataBindingFirstPassEvaluator<'a> {
    show_binding_warnings: bool,
    symbol_table: &'a SymbolTable,
    src_type_stack: Vec<Option<String>>,
    argument_list_stack: Vec<Option<NodeRef>>,
    skip_init_set: HashSet<Option<Key>>,
    reset_set: HashSet<Option<Key>>,
    xml_member: Option<NodeRef>,
    inside_repeater_expression: bool,
    inside_xml_expression: bool,
    inside_bindings_setup_function: bool,
    call_expression_stack: Vec<NodeRef>,
    inside_get_expression: bool,
    inside_array_expression: bool,
    watcher_list: WatcherList,
    current_class_definition: Option<NodeRef>,
    current_binding_expression: Option<BindingExpressionRef>,
    binding_id: usize,
    current_watcher_id: i32,
    binding_function_name: String,
    binding_expressions: Vec<BindingExpressionRef>,
    evaluated_classes: HashSet<Key>,
    data_binding_info: Option<Rc<RefCell<DataBindingInfo>>>,
    data_binding_info_list: Vec<Rc<RefCell<DataBindingInfo>>>,
    make_second_pass: bool,
    standard_defs: Rc<StandardDefs>,
}

impl<'a> DataBindingFirstPassEvaluator<'a> {
    pub fn new(binding_expressions: Vec<BindingExpressionRef>, symbol_table: &'a SymbolTable, show_binding_warnings: bool) -> DataBindingFirstPassEvaluator<'a> {
        DataBindingFirstPassEvaluator {
            show_binding_warnings,
            symbol_table,
            src_type_stack: Vec::new(),
            argument_list_stack: Vec::new(),
            skip_init_set: HashSet::new(),
            reset_set: HashSet::new(),
            xml_member: None,
            inside_repeater_expression: false,
            inside_xml_expression: false,
            inside_bindings_setup_function: false,
            call_expression_stack: Vec::new(),
            inside_get_expression: false,
            inside_array_expression: false,
            watcher_list: new_list(),
            current_class_definition: None,
            current_binding_expression: None,
            binding_id: 0,
            current_watcher_id: 0,
            binding_function_name: String::new(),
            binding_expressions,
            evaluated_classes: HashSet::new(),
            data_binding_info: None,
            data_binding_info_list: Vec::new(),
            make_second_pass: false,
            standard_defs: StandardDefs::get_standard_defs("halo"),
        }
    }

    pub fn get_data_binding_info_list(&self) -> &[Rc<RefCell<DataBindingInfo>>] {
        &self.data_binding_info_list
    }

    pub fn make_second_pass(&self) -> bool {
        self.make_second_pass
    }

    fn next_id(&mut self) -> i32 {
        let id = self.current_watcher_id;
        self.current_watcher_id += 1;
        id
    }

    fn first_element(&self) -> Option<String> {
        self.src_type_stack.first().cloned().expect("NoSuchElementException")
    }

    fn peek(&self) -> Option<String> {
        self.src_type_stack.last().cloned().expect("EmptyStackException")
    }

    fn be(&self) -> BindingExpressionRef {
        self.current_binding_expression.clone().expect("NullPointerException")
    }

    fn last_watcher(&self) -> Option<WatcherRef> {
        self.watcher_list.borrow().last().cloned()
    }

    fn add_bindables(&self, watcher: &WatcherRef, bindables: Option<&[MetaData]>) -> bool {
        let mut added_bindable = false;
        if let Some(bindables) = bindables {
            for meta_data in bindables {
                if meta_data.get_value_key(STYLE).as_deref() == Some("true") {
                    if watcher.borrow().is_function_return_watcher() {
                        watcher.borrow_mut().set_style_watcher(true);
                        added_bindable = true;
                    }
                } else {
                    match get_event_name(meta_data) {
                        Some(event) => watcher.borrow_mut().add_change_event(&event),
                        None => watcher.borrow_mut().add_change_event(standard_defs::MDPARAM_PROPERTY_CHANGE),
                    }
                    added_bindable = true;
                }
            }
        }
        added_bindable
    }

    fn check_for_static_property(is_static: bool, watcher: &WatcherRef, src_type_name: &str) {
        if is_static && watcher.borrow().is_property_watcher() {
            watcher.borrow_mut().set_static_property(true);
            watcher.borrow_mut().set_class_name(src_type_name);
        }
    }

    fn add_change_events(watcher: &WatcherRef, change_events: Option<&[MetaData]>, committing: bool) -> bool {
        let mut added = false;
        if let Some(change_events) = change_events {
            for meta_data in change_events {
                if let Some(event) = meta_data.get_value_index(0) {
                    watcher.borrow_mut().add_change_event_validate(&event, committing);
                    added = true;
                }
            }
        }
        added
    }

    fn argument_list(&mut self, context: &ContextRef, node: &NodeRef) {
        if !self.inside_bindings_setup_function {
            return;
        }
        let size = node.borrow().as_argument_list().unwrap().items.len();
        for i in 0..size {
            let argument = node.borrow().as_argument_list().unwrap().items[i].clone();
            let temp_watcher_list = self.watcher_list.clone();
            self.argument_list_stack.push(Some(node.clone()));
            let first = self.first_element();
            self.src_type_stack.push(first);
            if !self.skip_init_set.remove(&Some(Rc::as_ptr(node))) {
                self.watcher_list = new_list();
            }
            if let Some(argument) = argument {
                evaluate(self, context, &argument);
            }
            if self.reset_set.remove(&Some(Rc::as_ptr(node))) {
                self.watcher_list = temp_watcher_list;
            }
            self.src_type_stack.pop();
            self.argument_list_stack.pop();
        }
    }

    fn binary_expression(&mut self, context: &ContextRef, node: &NodeRef) {
        let (lhs, rhs, op) = {
            let b = node.borrow();
            let n = b.as_binary_expression().unwrap();
            (n.lhs.clone(), n.rhs.clone(), n.op)
        };
        if let Some(lhs) = lhs {
            evaluate(self, context, &lhs);
        }
        if self.inside_bindings_setup_function && op != AS_TOKEN {
            self.watcher_list = new_list();
        }
        if let Some(rhs) = rhs {
            evaluate(self, context, &rhs);
        }
        if self.inside_bindings_setup_function && op != AS_TOKEN {
            self.watcher_list = new_list();
        }
    }

    fn call_expression(&mut self, context: &ContextRef, node: &NodeRef) {
        let (expr, args) = {
            let b = node.borrow();
            let n = b.as_call_expression().unwrap();
            (n.sel.expr.clone(), n.args.clone())
        };
        if !self.inside_bindings_setup_function {
            return;
        }
        let Some(expr) = expr else { return };
        let identifier_name = expr.borrow().identifier().map(|i| i.name.clone());
        if self.inside_repeater_expression && identifier_name.as_deref() == Some("getItemAt") {
            self.skip_init_set.insert(ptr(&args));
            if let Some(args) = &args {
                evaluate(self, context, args);
            }
            return;
        }
        self.argument_list_stack.push(args.clone());
        if let Some(name) = identifier_name {
            let id_ref = expr.borrow().identifier().unwrap().ref_.clone();
            let is_cast = match &id_ref {
                Some(r) => {
                    let t = r.get_type(&context.borrow());
                    name == t.get_name().name()
                }
                None => true,
            };
            if !is_cast {
                self.call_expression_stack.push(node.clone());
                evaluate(self, context, &expr);
                self.call_expression_stack.pop();
                self.reset_set.insert(ptr(&args));
            } else {
                self.skip_init_set.insert(ptr(&args));
            }
        }
        if let Some(args) = &args {
            let first = self.first_element();
            self.src_type_stack.push(first);
            evaluate(self, context, args);
            self.src_type_stack.pop();
        }
        self.argument_list_stack.pop();
    }

    fn class_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        if self.evaluated_classes.contains(&Rc::as_ptr(node)) {
            return;
        }
        self.current_class_definition = Some(node.clone());
        let (class_name, fully_qualified_class_name, fexprs) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            let name = c.name.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).expect("NullPointerException");
            let fq = c.cframe.as_ref().expect("NullPointerException").name().to_string();
            (name, fq, c.fexprs.as_ref().map(|l| l.borrow().clone()))
        };
        let converted_class_name = format!("_{}", class_name.replace(['.', ':'], "_"));
        self.binding_function_name = format!("{converted_class_name}_bindingsSetup");
        self.src_type_stack.push(Some(fully_qualified_class_name.clone()));
        if let Some(fexprs) = fexprs {
            for function_common in fexprs {
                evaluate(self, context, &function_common);
            }
        }
        self.src_type_stack.pop();
        if let Some(info) = self.data_binding_info.take() {
            info.borrow_mut().set_binding_expressions(self.binding_expressions.clone());
            info.borrow_mut().set_class_name(&fully_qualified_class_name);
            self.data_binding_info_list.push(info);
        }
        self.evaluated_classes.insert(Rc::as_ptr(node));
        self.current_class_definition = None;
    }

    fn conditional_expression(&mut self, context: &ContextRef, node: &NodeRef) {
        let (c, t, e) = {
            let b = node.borrow();
            let n = b.as_conditional_expression().unwrap();
            (n.condition.clone(), n.thenexpr.clone(), n.elseexpr.clone())
        };
        if let Some(c) = c {
            evaluate(self, context, &c);
        }
        if self.inside_bindings_setup_function {
            self.watcher_list = new_list();
        }
        if let Some(t) = t {
            evaluate(self, context, &t);
        }
        if self.inside_bindings_setup_function {
            self.watcher_list = new_list();
        }
        if let Some(e) = e {
            evaluate(self, context, &e);
        }
        if self.inside_bindings_setup_function {
            self.watcher_list = new_list();
        }
    }

    fn function_common(&mut self, context: &ContextRef, function_common: &NodeRef) {
        let (name, body) = {
            let b = function_common.borrow();
            let f = b.as_function_common().unwrap();
            (f.identifier.as_ref().and_then(|i| i.borrow().identifier().map(|i| i.name.clone())), f.body.clone())
        };
        if name.as_deref() != Some(self.binding_function_name.as_str()) {
            return;
        }
        self.inside_bindings_setup_function = true;
        let imported = self.current_class_definition.as_ref().unwrap().borrow().class_definition().unwrap().imported_names.clone();
        self.data_binding_info = Some(Rc::new(RefCell::new(DataBindingInfo::new(node_magic::get_imports(&imported)))));
        let items: Vec<Option<NodeRef>> = body.as_ref().map(|b| b.borrow().as_statement_list().unwrap().items.clone()).unwrap_or_default();
        for item1 in items.into_iter().flatten() {
            if item1.borrow().as_expression_statement().is_none() {
                continue;
            }
            let item2 = {
                let b = item1.borrow();
                let list1 = b.as_expression_statement().unwrap().expr.clone().unwrap();
                let lb = list1.borrow();
                let member1 = lb.list().unwrap().items[0].clone().unwrap();
                let mb = member1.borrow();
                let set = mb.as_member_expression().unwrap().selector.clone().unwrap();
                let sb = set.borrow();
                let args = sb.as_set_expression().unwrap().args.clone().unwrap();
                let ab = args.borrow();
                let a = ab.as_argument_list().unwrap().items[0].clone().unwrap();
                a
            };
            let call = {
                let b = item2.borrow();
                match b.as_member_expression() {
                    Some(m) => m.selector.clone().filter(|s| s.borrow().as_call_expression().is_some()),
                    None => None,
                }
            };
            let Some(call) = call else { continue };
            let call_args: Vec<Option<NodeRef>> = {
                let b = call.borrow();
                let args = b.as_call_expression().unwrap().args.clone().unwrap();
                let ab = args.borrow();
                ab.as_argument_list().unwrap().items.clone()
            };
            let arg1 = call_args[1].clone().unwrap();
            self.current_binding_expression = Some(self.binding_expressions[self.binding_id].clone());
            self.binding_id += 1;
            self.watcher_list = new_list();
            let kind = match arg1.borrow().kind {
                Kind::FunctionCommon(_) => 0,
                Kind::LiteralNull(_) => 1,
                _ => 2,
            };
            if kind == 0 {
                let (type_expression, items) = {
                    let b = arg1.borrow();
                    let f = b.as_function_common().unwrap();
                    let sig = f.signature.clone().unwrap();
                    let result = sig.borrow().as_function_signature().unwrap().result.clone();
                    (result, f.body.as_ref().map(|b| b.borrow().as_statement_list().unwrap().items.clone()).unwrap_or_default())
                };
                let size = items.len();
                let array_or_string = type_expression.as_ref().is_some_and(is_array_or_string);
                if array_or_string {
                    let target = {
                        let es = items[size - 3].clone().unwrap();
                        let eb = es.borrow();
                        let list2 = eb.as_expression_statement().unwrap().expr.clone().unwrap();
                        let lb = list2.borrow();
                        let m4 = lb.list().unwrap().items[0].clone().unwrap();
                        let mb = m4.borrow();
                        let s3 = mb.as_member_expression().unwrap().selector.clone().unwrap();
                        let sb = s3.borrow();
                        let args = sb.as_set_expression().unwrap().args.clone().unwrap();
                        let ab = args.borrow();
                        let t = ab.as_argument_list().unwrap().items[0].clone().unwrap();
                        t
                    };
                    evaluate(self, context, &target);
                } else {
                    let expr = {
                        let rs = items[size - 2].clone().unwrap();
                        let rb = rs.borrow();
                        rb.as_return_statement().expect("ClassCastException").expr.clone()
                    };
                    if let Some(expr) = expr {
                        evaluate(self, context, &expr);
                    }
                }
            } else if kind == 1 {
                let name = call_args[4].clone().unwrap().borrow().as_literal_string().unwrap().value.clone();
                let watcher = self.watch_expression_string_as_property(&name);
                let pos = arg1.borrow().pos();
                self.find_events(context, &name, &Ns::List(vec![PUBLIC_NAMESPACE.to_string()]), &name, pos, &watcher, None);
            }
            self.inside_xml_expression = false;
            self.xml_member = None;
        }
        self.inside_bindings_setup_function = false;
        self.make_second_pass = true;
    }

    fn get_expression(&mut self, context: &ContextRef, node: &NodeRef) {
        let expr = node.borrow().as_get_expression().unwrap().sel.expr.clone();
        let Some(expr) = expr else { return };
        self.inside_get_expression = true;
        if expr.borrow().as_argument_list().is_some() {
            if !self.inside_repeater_expression {
                if !self.inside_xml_expression && self.show_binding_warnings {
                    let pos = node.borrow().pos();
                    localized2(
                        context,
                        Level::Warning,
                        None,
                        pos,
                        "Data binding will not be able to detect changes when using square bracket operator.  For Array, please use ArrayCollection.getItemAt() instead.".to_string(),
                    );
                }
                if !self.inside_xml_expression {
                    self.argument_list_stack.push(Some(expr.clone()));
                    self.watch_expression_array(context);
                    self.argument_list_stack.pop();
                }
                self.reset_set.insert(Some(Rc::as_ptr(&expr)));
                evaluate(self, context, &expr);
            }
        } else {
            evaluate(self, context, &expr);
        }
        self.inside_get_expression = false;
    }

    fn identifier(&mut self, context: &ContextRef, node: &NodeRef) {
        let (name, r, pos) = {
            let b = node.borrow();
            let i = b.as_identifier().unwrap();
            (i.name.clone(), i.ref_.clone(), b.pos())
        };
        if self.inside_bindings_setup_function && name != "instanceIndices" {
            if let Some(watcher) = self.watch_identifier(context, &name) {
                self.find_events(context, &name, &Ns::Visibility, &name, pos, &watcher, r);
            }
        }
    }

    fn invoke(&mut self, context: &ContextRef, node: &NodeRef) {
        if self.inside_bindings_setup_function && self.inside_xml_expression {
            let args = node.borrow().as_invoke().unwrap().args.clone();
            self.skip_init_set.insert(ptr(&args));
        }
        walk(self, context, node);
    }

    fn literal_number(&mut self, context: &ContextRef) {
        if self.inside_bindings_setup_function && self.inside_get_expression {
            self.watch_expression_array(context);
        }
    }

    fn with_statement(&mut self, context: &ContextRef, node: &NodeRef) {
        let (expr, statement) = {
            let b = node.borrow();
            let n = b.as_with_statement().unwrap();
            (n.expr.clone(), n.statement.clone())
        };
        if let Some(expr) = expr {
            evaluate(self, context, &expr);
        }
        let saved_watcher_list = self.watcher_list.clone();
        let normal_warning_mode = self.show_binding_warnings;
        if self.inside_bindings_setup_function && !self.inside_array_expression {
            if let Some(xml_member) = self.xml_member.clone() {
                let name = xml_member.borrow().as_member_expression().unwrap().ref_.as_ref().expect("NullPointerException").name();
                if let Some(watcher) = self.watch_expression_string_as_xml(&name) {
                    let pos = xml_member.borrow().pos();
                    self.find_events(context, &name, &Ns::Visibility, &name, pos, &watcher, None);
                }
                self.xml_member = None;
            }
            if self.inside_xml_expression {
                self.watcher_list = new_list();
                self.show_binding_warnings = false;
            }
        }
        if let Some(statement) = statement {
            evaluate(self, context, &statement);
        }
        if self.inside_bindings_setup_function && !self.inside_array_expression && self.inside_xml_expression {
            self.watcher_list = saved_watcher_list;
            self.show_binding_warnings = normal_warning_mode;
        }
    }

    fn member_expression(&mut self, context: &ContextRef, node: &NodeRef) {
        let (base, selector, node_ref, indexed) = {
            let b = node.borrow();
            let m = b.as_member_expression().unwrap();
            (m.base.clone(), m.selector.clone(), m.ref_.clone(), m.is_indexed_member_expression())
        };
        if self.inside_bindings_setup_function && !self.inside_repeater_expression && self.is_repeater_base(&base) {
            self.inside_repeater_expression = true;
            evaluate(self, context, base.as_ref().unwrap());
            self.src_type_stack.push(None);
            evaluate(self, context, selector.as_ref().expect("NullPointerException"));
            self.src_type_stack.pop();
            self.inside_repeater_expression = false;
        } else if self.inside_bindings_setup_function && is_repeater_indices_base(&base) {
            self.setup_repeater_watchers(node);
        } else {
            let mut pushed = 0;
            let old_array_expression = self.inside_array_expression;
            self.inside_array_expression = indexed;
            if let Some(base) = &base {
                evaluate(self, context, base);
                if self.inside_bindings_setup_function && (!self.inside_array_expression || self.inside_xml_expression) {
                    let r = get_ref(node);
                    self.push_src_type(context, r, base);
                    pushed += 1;
                }
            }
            let mut static_reference = false;
            if self.inside_bindings_setup_function {
                if let Some(r) = &node_ref {
                    if is_static_reference(&selector, r) {
                        static_reference = true;
                        let ov = r.slot().unwrap().get_object_value().unwrap().to_string();
                        self.src_type_stack.push(Some(ov));
                        pushed += 1;
                    }
                    let t = r.get_type(&context.borrow()).get_name().to_string();
                    if t == "XML" || t == "XMLList" {
                        self.xml_member = Some(node.clone());
                        self.inside_xml_expression = true;
                    }
                }
            }
            if let (Some(selector), false) = (&selector, static_reference) {
                evaluate(self, context, selector);
            }
            self.inside_array_expression = old_array_expression;
            for _ in 0..pushed {
                self.src_type_stack.pop();
            }
        }
    }

    fn qualified_identifier(&mut self, context: &ContextRef, node: &NodeRef) {
        if !self.inside_bindings_setup_function {
            return;
        }
        let q_name = node_magic::get_qname(node);
        let (r, pos) = {
            let b = node.borrow();
            (b.as_qualified_identifier().unwrap().id.ref_.clone(), b.pos())
        };
        let namespaces = self.be().get_namespaces();
        let matches_binding_ns = namespaces.is_some_and(|n| n.iter().any(|(_, v)| v == q_name.get_namespace()));
        if matches_binding_ns {
            let name = q_name.get_local_part().to_string();
            if let Some(watcher) = self.watch_identifier(context, &name) {
                self.find_events(context, &name, &Ns::Visibility, &name, pos, &watcher, r);
            }
        } else {
            let name = format!("{}::{}", q_name.get_namespace(), q_name.get_local_part());
            if let Some(watcher) = self.watch_identifier(context, &name) {
                self.find_events(context, &name, &Ns::List(vec![q_name.get_namespace().to_string()]), q_name.get_local_part(), pos, &watcher, r);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn find_events(&mut self, context: &ContextRef, name: &str, ns: &Ns, local_part: &str, pos: i32, watcher: &WatcherRef, r: Option<ReferenceValue>) {
        let src_type_name = if watcher.borrow().is_part_of_anon_object_graph() {
            Some(self.standard_defs.class_objectproxy.clone())
        } else if !self.src_type_stack.is_empty() {
            self.peek()
        } else {
            None
        };
        let parent_watcher = watcher.borrow().get_parent();
        if parent_watcher.as_ref().is_some_and(|p| p.borrow().is_operation()) {
            watcher.borrow_mut().add_change_event("resultForBinding");
            return;
        }
        let Some(src_type_name) = src_type_name else { return };
        let Some(watched_class) = self.symbol_table.get_class(Some(&src_type_name)) else { return };
        let cx = context.borrow();
        if watched_class.is_subclass_of(&self.standard_defs.class_objectproxy) {
            watcher.borrow_mut().set_part_of_anon_object_graph(true);
        }
        let meta_data = watched_class.get_meta_data(BINDABLE, true);
        let mut found_events = self.add_bindables(watcher, Some(&meta_data));
        let mut found_source = false;
        let variable = get_variable(&cx, &watched_class, r.as_ref(), ns, local_part);
        if let Some(variable) = &variable {
            let md = variable.get_meta_data(BINDABLE);
            found_events = self.add_bindables(watcher, md.as_deref()) || found_events;
            let md = variable.get_meta_data(CHANGE_EVENT);
            found_events = Self::add_change_events(watcher, md.as_deref(), true) || found_events;
            let md = variable.get_meta_data(NON_COMMITTING_CHANGE_EVENT);
            found_events = Self::add_change_events(watcher, md.as_deref(), false) || found_events;
            let is_object_length = local_part == "length" && variable.get_declaring_class_name() == OBJECT;
            if variable.is_const() && !is_object_length {
                found_events = true;
                if watcher.borrow().is_property_watcher() {
                    watcher.borrow_mut().suppress();
                }
            }
            if !is_object_length {
                Self::check_for_static_property(variable.is_static(), watcher, &src_type_name);
            }
            found_source = true;
        }
        if !found_events {
            let getter = get_method_from(&cx, &watched_class, r.as_ref(), ns, local_part, GET_TOKEN);
            if let Some(getter) = &getter {
                found_events = self.add_method_events(watcher, getter) || found_events;
                Self::check_for_static_property(getter.is_static(), watcher, &src_type_name);
                found_source = true;
            }
            let setter = get_method_from(&cx, &watched_class, r.as_ref(), ns, local_part, SET_TOKEN);
            if let Some(setter) = &setter {
                found_events = self.add_method_events(watcher, setter) || found_events;
                Self::check_for_static_property(setter.is_static(), watcher, &src_type_name);
                found_source = true;
            } else if getter.is_some() {
                found_events = true;
            }
        }
        if !found_source {
            let function = get_method_from(&cx, &watched_class, r.as_ref(), ns, local_part, EMPTY_TOKEN);
            if let Some(function) = &function {
                found_events = self.add_method_events(watcher, function) || found_events;
                found_source = true;
                if !found_events && self.call_expression_stack.is_empty() {
                    found_events = true;
                    if watcher.borrow().is_property_watcher() {
                        watcher.borrow_mut().suppress();
                    }
                }
            }
        }
        if !found_source && watched_class.is_subclass_of(&self.standard_defs.class_abstractservice) {
            watcher.borrow_mut().set_operation(true);
        } else if !found_events && !watcher.borrow().is_function_return_watcher() && !watcher.borrow().is_xml_watcher() && !watcher.borrow().is_operation() {
            let wc = watched_class.get_name();
            if !(wc == OBJECT || wc == NOTYPE) {
                drop(cx);
                if self.show_binding_warnings {
                    localized2(context, Level::Warning, None, pos, format!("Data binding will not be able to detect assignments to \"{name}\"."));
                }
                if watcher.borrow().is_property_watcher() {
                    watcher.borrow_mut().suppress();
                }
            }
        }
    }

    fn add_method_events(&self, watcher: &WatcherRef, m: &Method) -> bool {
        let mut found = false;
        let md = m.get_meta_data(BINDABLE);
        found = self.add_bindables(watcher, md.as_deref()) || found;
        let md = m.get_meta_data(CHANGE_EVENT);
        found = Self::add_change_events(watcher, md.as_deref(), true) || found;
        let md = m.get_meta_data(NON_COMMITTING_CHANGE_EVENT);
        found = Self::add_change_events(watcher, md.as_deref(), false) || found;
        found
    }

    fn is_repeater_base(&self, node: &Option<NodeRef>) -> bool {
        let Some(node) = node else { return false };
        let b = node.borrow();
        let Some(m) = b.as_member_expression() else { return false };
        if m.base.is_some() {
            return self.is_repeater_base(&m.base);
        }
        let Some(sel) = &m.selector else { return false };
        let sb = sel.borrow();
        let Some(g) = sb.as_get_expression() else { return false };
        let Some(expr) = &g.sel.expr else { return false };
        let eb = expr.borrow();
        match eb.identifier() {
            Some(i) => self.be().get_repeater_level(&i.name) >= 0,
            None => false,
        }
    }

    fn push_src_type(&mut self, context: &ContextRef, r: Option<ReferenceValue>, base: &NodeRef) {
        if let Some(r) = r.filter(|r| r.slot().is_some()) {
            let slot = r.slot().unwrap();
            let ov = slot.get_object_value().map(|o| o.to_string());
            if let Some(ov) = ov.filter(|s| !s.is_empty()) {
                self.src_type_stack.push(Some(ov));
                return;
            }
            let t = r.get_type(&context.borrow()).get_name().to_string();
            if !t.is_empty() {
                self.src_type_stack.push(Some(t));
                return;
            }
            if let Some(st) = slot.get_type().map(|t| t.get_name().to_string()).filter(|s| !s.is_empty()) {
                self.src_type_stack.push(Some(st));
                return;
            }
            self.src_type_stack.push(None);
        } else if base.borrow().as_this_expression().is_some() {
            let first = self.first_element();
            self.src_type_stack.push(first);
        } else {
            self.src_type_stack.push(None);
        }
    }

    fn setup_repeater_watchers(&mut self, node: &NodeRef) {
        let selector = node.borrow().as_member_expression().unwrap().selector.clone().unwrap();
        let repeater_watcher = if self.watcher_list.borrow().is_empty() {
            let level = {
                let sb = selector.borrow();
                let args = sb.as_get_expression().unwrap().sel.expr.clone().unwrap();
                let ab = args.borrow();
                let ln = ab.as_argument_list().unwrap().items[0].clone().unwrap();
                let lb = ln.borrow();
                let n = lb.as_literal_number().unwrap();
                n.value.parse::<f64>().map(|v| v as i32).unwrap_or(0)
            };
            let id = self.next_id();
            let property = self.be().get_repeater_id(level as usize).unwrap_or_default();
            let w = Watcher::property(id, &property);
            self.watcher_list.borrow_mut().push(w.clone());
            w
        } else {
            self.last_watcher().unwrap()
        };
        let existing = repeater_watcher.borrow().get_child("dataProvider");
        let data_provider_watcher = match existing {
            Some(w) => w,
            None => {
                let id = self.next_id();
                let w = Watcher::property(id, "dataProvider");
                Watcher::add_child(&repeater_watcher, &w);
                w
            }
        };
        self.watcher_list.borrow_mut().push(data_provider_watcher.clone());
        let be = self.be();
        data_provider_watcher.borrow_mut().add_binding_expression(&be);
        let id = self.next_id();
        let repeater_item_watcher = Watcher::repeater_item(id);
        repeater_item_watcher.borrow_mut().add_binding_expression(&be);
        Watcher::add_child(&data_provider_watcher, &repeater_item_watcher);
        self.watcher_list.borrow_mut().push(repeater_item_watcher);
        let expr = selector.borrow().as_get_expression().and_then(|g| g.sel.expr.clone());
        if let Some(expr) = expr.filter(|e| e.borrow().as_argument_list().is_some()) {
            self.skip_init_set.insert(Some(Rc::as_ptr(&expr)));
        }
    }

    fn watch_identifier(&mut self, context: &ContextRef, name: &str) -> Option<WatcherRef> {
        let size = self.src_type_stack.len();
        if self.inside_get_expression && !(size == 1 && matches!(name, "Infinity" | "-Infinity" | "NaN" | "undefined")) {
            let src = self.peek();
            let last_is_xml = self.last_watcher().is_some_and(|w| w.borrow().is_xml_watcher());
            if last_is_xml || src.as_deref().is_some_and(|s| s == "XML" || s == "XMLList") {
                let w = self.watch_expression_string_as_xml(name);
                self.xml_member = None;
                w
            } else {
                Some(self.watch_expression_string_as_property(name))
            }
        } else if !self.call_expression_stack.is_empty()
            && !(size == 1
                && matches!(
                    name,
                    "Array" | "Boolean" | "decodeURI" | "decodeURIComponent" | "encodeURI" | "encodeURIComponent" | "escape" | "int" | "isFinite" | "isNaN" | "isXMLName" | "Number" | "Object" | "parseFloat" | "parseInt" | "trace" | "uint" | "unescape" | "XML" | "XMLList"
                ))
        {
            Some(self.watch_expression_string_as_function(context, name))
        } else {
            None
        }
    }

    fn watch_expression_array(&mut self, context: &ContextRef) {
        let id = self.next_id();
        let be = self.be();
        let args = self.argument_list_stack.last().cloned().flatten();
        let watcher = Watcher::array_element(id, &be, args, context);
        if let Some(parent_watcher) = self.last_watcher() {
            watcher.borrow_mut().set_parent_watcher(&parent_watcher);
            Watcher::add_child(&parent_watcher, &watcher);
            if parent_watcher.borrow().is_part_of_anon_object_graph() {
                watcher.borrow_mut().set_part_of_anon_object_graph(true);
            }
        }
        self.watcher_list.borrow_mut().push(watcher);
    }

    fn watch_expression_string_as_function(&mut self, context: &ContextRef, value: &str) -> WatcherRef {
        let id = self.next_id();
        let be = self.be();
        let args = self.argument_list_stack.last().cloned().flatten();
        let watcher = Watcher::function_return(id, &be, value, args, context);
        match self.last_watcher() {
            Some(parent_watcher) => {
                Watcher::add_child(&parent_watcher, &watcher);
                watcher.borrow_mut().set_parent_watcher(&parent_watcher);
            }
            None => {
                let key = format!("{value}{}", watcher.borrow().get_id());
                self.data_binding_info.as_ref().unwrap().borrow_mut().get_root_watchers().insert(key, watcher.clone());
            }
        }
        let src = self.peek();
        if self.src_type_stack.len() > 1 && src.is_some() && self.first_element() != src && self.watcher_list.borrow().is_empty() {
            watcher.borrow_mut().set_class_name(src.as_deref().unwrap());
        }
        self.watcher_list.borrow_mut().push(watcher.clone());
        watcher
    }

    fn watch_expression_string_as_xml(&mut self, value: &str) -> Option<WatcherRef> {
        let watcher = if self.watcher_list.borrow().is_empty() {
            let info = self.data_binding_info.clone().unwrap();
            if info.borrow_mut().get_root_watchers().contains_key(value) {
                return None;
            }
            let id = self.next_id();
            let w = Watcher::xml(id, value);
            info.borrow_mut().get_root_watchers().insert(value.to_string(), w.clone());
            w
        } else {
            let parent_watcher = self.last_watcher().unwrap();
            let child = parent_watcher.borrow().get_child(value);
            match child.filter(|c| c.borrow().is_xml_watcher()) {
                Some(c) => c,
                None => {
                    let id = self.next_id();
                    let w = Watcher::xml(id, value);
                    Watcher::add_child(&parent_watcher, &w);
                    w
                }
            }
        };
        let src = self.peek();
        if self.src_type_stack.len() > 1 && src.is_some() && self.first_element() != src && self.watcher_list.borrow().is_empty() {
            watcher.borrow_mut().set_class_name(src.as_deref().unwrap());
        }
        self.watcher_list.borrow_mut().push(watcher.clone());
        let be = self.be();
        watcher.borrow_mut().add_binding_expression(&be);
        Some(watcher)
    }

    fn watch_expression_string_as_property(&mut self, value: &str) -> WatcherRef {
        let watcher = if self.watcher_list.borrow().is_empty() {
            self.watch_root_property(value)
        } else {
            let parent_watcher = self.last_watcher().unwrap();
            let existing = parent_watcher.borrow().get_child(value);
            match existing {
                Some(w) => w,
                None => {
                    let id = self.next_id();
                    let w = Watcher::property(id, value);
                    if parent_watcher.borrow().is_part_of_anon_object_graph() {
                        w.borrow_mut().set_part_of_anon_object_graph(true);
                    } else if !parent_watcher.borrow().should_write_children() {
                        w.borrow_mut().set_should_write_children(false);
                    }
                    Watcher::add_child(&parent_watcher, &w);
                    w
                }
            }
        };
        self.watcher_list.borrow_mut().push(watcher.clone());
        let be = self.be();
        watcher.borrow_mut().add_binding_expression(&be);
        watcher
    }

    fn watch_root_property(&mut self, property_name: &str) -> WatcherRef {
        let mut key = property_name.to_string();
        let src_type = self.peek();
        let mut class_name = None;
        if self.src_type_stack.len() > 1 && src_type.is_some() && self.first_element() != src_type {
            key = format!("{}.{property_name}", src_type.as_deref().unwrap());
            class_name = src_type;
        }
        let info = self.data_binding_info.clone().unwrap();
        let existing = info.borrow_mut().get_root_watchers().get(&key).cloned();
        match existing {
            Some(w) => w,
            None => {
                let destination = self.be().get_destination();
                let id = self.next_id();
                let result = match destination.filter(|d| d.get_repeater_level() > 1) {
                    Some(d) => Watcher::repeater_component(id, property_name, d.get_repeater_level() as i32),
                    None => Watcher::property(id, property_name),
                };
                if let Some(c) = &class_name {
                    result.borrow_mut().set_class_name(c);
                }
                info.borrow_mut().get_root_watchers().insert(key, result.clone());
                result
            }
        }
    }
}

/// `getEventName(metaData)`.
fn get_event_name(meta_data: &MetaData) -> Option<String> {
    if let Some(e) = meta_data.get_value_key(EVENT) {
        return Some(e);
    }
    if meta_data.count() == 1 {
        let param = meta_data.get_value_index(0).expect("NullPointerException");
        if Some(param.as_str()) != meta_data.get_id() {
            return Some(param);
        }
    }
    None
}

fn get_variable(cx: &crate::asc::util::context::Context, watched_class: &AbcClass, r: Option<&ReferenceValue>, ns: &Ns, local_part: &str) -> Option<Variable> {
    match r {
        Some(r) => {
            let mut var = None;
            if r.slot().is_some() && r.get_kind() == GET_TOKEN {
                if let Some(s) = r.get_slot_kind(cx, GET_TOKEN) {
                    if s.is_variable_slot() {
                        var = Some(Variable::new(s, r.get_immutable_namespaces().at(0), &r.name()));
                    }
                }
            }
            if var.is_none() {
                var = watched_class.get_variable_ns(&r.get_immutable_namespaces(), &r.name(), true);
            }
            var
        }
        None => ns.with(|n| watched_class.get_variable(n, local_part, true)),
    }
}

/// `getGetter`, `getSetter` and `getMethod` (`kind` GET, SET or EMPTY), each through `getMethodFromRef` when there is
/// a reference.
fn get_method_from(cx: &crate::asc::util::context::Context, watched_class: &AbcClass, r: Option<&ReferenceValue>, ns: &Ns, local_part: &str, orig_kind: i32) -> Option<Method> {
    let Some(r) = r else {
        return ns.with(|n| match orig_kind {
            SET_TOKEN => watched_class.get_setter(n, local_part, true),
            _ => watched_class.get_getter(n, local_part, true),
        });
    };
    let mut meth = None;
    let kind = if orig_kind == EMPTY_TOKEN { GET_TOKEN } else { orig_kind };
    if r.slot().is_some() && r.get_kind() == kind {
        if let Some(s) = r.get_slot_kind(cx, orig_kind) {
            if s.is_method_slot() && ((orig_kind == GET_TOKEN) == s.is_getter()) {
                meth = Some(Method::new(s, r.get_immutable_namespaces().at(0), &r.name()));
            }
        }
    }
    if meth.is_none() {
        meth = match orig_kind {
            EMPTY_TOKEN => watched_class.get_method_ns(&r.get_immutable_namespaces(), &r.name(), true),
            GET_TOKEN => watched_class.get_getter_ns(&r.get_immutable_namespaces(), &r.name(), true),
            SET_TOKEN => watched_class.get_setter_ns(&r.get_immutable_namespaces(), &r.name(), true),
            _ => None,
        };
    }
    meth
}

/// `getRef(memberExpression)`.
fn get_ref(member_expression: &NodeRef) -> Option<ReferenceValue> {
    let base = member_expression.borrow().as_member_expression().unwrap().base.clone()?;
    let b = base.borrow();
    match &b.kind {
        Kind::CallExpression(c) => c.sel.ref_.clone(),
        Kind::MemberExpression(m) => {
            let r = m.ref_.clone();
            if r.as_ref().is_none_or(|r| r.slot().is_none()) {
                drop(b);
                get_ref(&base)
            } else {
                r
            }
        }
        Kind::GetExpression(g) => g.sel.ref_.clone(),
        Kind::List(l) => {
            let node = l.items[0].clone()?;
            let nb = node.borrow();
            let be = nb.as_binary_expression()?;
            if be.op == AS_TOKEN {
                let rhs = be.rhs.clone()?;
                let rb = rhs.borrow();
                rb.as_member_expression().and_then(|m| m.ref_.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_repeater_indices_base(node: &Option<NodeRef>) -> bool {
    let Some(node) = node else { return false };
    let b = node.borrow();
    let Some(m) = b.as_member_expression() else { return false };
    if m.base.is_some() {
        return is_repeater_indices_base(&m.base);
    }
    let Some(sel) = &m.selector else { return false };
    let sb = sel.borrow();
    let Some(g) = sb.as_get_expression() else { return false };
    let Some(expr) = &g.sel.expr else { return false };
    let eb = expr.borrow();
    eb.identifier().is_some_and(|i| i.name == "repeaterIndices")
}

/// `isStaticReference(selector, referenceValue)`.
fn is_static_reference(selector: &Option<NodeRef>, r: &ReferenceValue) -> bool {
    let selector_ok = selector.as_ref().is_none_or(|s| s.borrow().as_get_expression().is_some());
    if !selector_ok {
        return false;
    }
    let Some(slot) = r.slot() else { return false };
    if !slot.is_variable_slot() {
        return false;
    }
    let Some(t) = slot.get_type() else { return false };
    let name = t.get_name();
    let ns_empty = name.ns().map_or(String::new(), |n| n.to_string()).is_empty();
    ns_empty && name.name() == "Class" && slot.get_object_value().is_some()
}

/// `isArrayOrString(typeExpression)`.
pub fn is_array_or_string(type_expression: &NodeRef) -> bool {
    let b = type_expression.borrow();
    let Some(te) = b.as_type_expression() else { return false };
    let Some(expr) = &te.expr else { return false };
    let eb = expr.borrow();
    let Some(m) = eb.as_member_expression() else { return false };
    let Some(sel) = &m.selector else { return false };
    let sb = sel.borrow();
    let Some(g) = sb.as_get_expression() else { return false };
    let Some(e) = &g.sel.expr else { return false };
    let ib = e.borrow();
    ib.as_identifier().is_some_and(|i| i.name == "Array" || i.name == "String")
}

impl Adapter for DataBindingFirstPassEvaluator<'_> {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let k = match node.borrow().kind {
            Kind::ArgumentList(_) => 0,
            Kind::BinaryExpression(_) => 1,
            Kind::CallExpression(_) => 2,
            Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) => 3,
            Kind::ConditionalExpression(_) => 4,
            Kind::FunctionCommon(_) => 5,
            Kind::GetExpression(_) => 6,
            Kind::Identifier(_) => 7,
            Kind::Invoke(_) => 8,
            Kind::LiteralNumber(_) => 9,
            Kind::WithStatement(_) => 10,
            Kind::MemberExpression(_) => 11,
            Kind::MetaData(_) => 12,
            Kind::QualifiedIdentifier(_) => 13,
            _ => return false,
        };
        match k {
            0 => self.argument_list(cx, node),
            1 => self.binary_expression(cx, node),
            2 => self.call_expression(cx, node),
            3 => self.class_definition(cx, node),
            4 => self.conditional_expression(cx, node),
            5 => self.function_common(cx, node),
            6 => self.get_expression(cx, node),
            7 => self.identifier(cx, node),
            8 => self.invoke(cx, node),
            9 => self.literal_number(cx),
            10 => self.with_statement(cx, node),
            11 => self.member_expression(cx, node),
            12 => {}
            _ => self.qualified_identifier(cx, node),
        }
        true
    }
}
