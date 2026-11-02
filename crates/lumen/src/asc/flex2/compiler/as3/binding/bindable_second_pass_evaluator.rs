//! Port of `flex2.compiler.as3.binding.BindableSecondPassEvaluator`: hides `[Bindable]` members and adds the
//! getters, setters and `IEventDispatcher` implementation, as ASTs or from `BindableProperty.vm`.

use std::borrow::Cow;
use std::collections::HashSet;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::asc::flex2::compiler::as3::binding::type_analyzer::TypeAnalyzerRef;
use crate::asc::flex2::compiler::as3::evaluator_adapter::{evaluate as adapter_evaluate, Adapter};
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::flex2::compiler::as3::genext::generative_class_info::{AccessorInfoRef, AccessorKind, GenerativeClassInfoRef};
use crate::asc::parser::tokens::{EMPTY_TOKEN, GET_TOKEN, SET_TOKEN, STRICTNOTEQUALS_TOKEN};
use crate::asc::flex2::compiler::as3::genext::generative_extension::IEVENT_DISPATCHER;
use crate::asc::flex2::compiler::as3::genext::generative_second_pass_evaluator::{self as gen, GenerativeSecondPass};
use crate::asc::flex2::compiler::as3::reflect::node_magic;
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator::localized2;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{self, StandardDefs};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::Level;
use crate::asc::flex2::compiler::util::{MultiName, QName};
use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::util::context::ContextRef;

const CODEGEN_TEMPLATE_PATH: &str = "flex2/compiler/as3/binding/";
const STATIC_EVENT_DISPATCHER: &str = "staticEventDispatcher";

type Key = *const std::cell::RefCell<crate::asc::parser::node::Node>;

pub struct BindableSecondPassEvaluator<'a> {
    base: GenerativeSecondPass,
    standard_defs: Rc<StandardDefs>,
    class_map: IndexMap<String, GenerativeClassInfoRef>,
    evaluated_classes: HashSet<Key>,
    type_analyzer: TypeAnalyzerRef,
    symbol_table: &'a SymbolTable,
    bindable_info: Option<GenerativeClassInfoRef>,
    in_class: bool,
}

impl<'a> BindableSecondPassEvaluator<'a> {
    pub fn new(base: GenerativeSecondPass, class_map: IndexMap<String, GenerativeClassInfoRef>, type_analyzer: TypeAnalyzerRef, symbol_table: &'a SymbolTable) -> BindableSecondPassEvaluator<'a> {
        BindableSecondPassEvaluator {
            base,
            standard_defs: StandardDefs::get_standard_defs("halo"),
            class_map,
            evaluated_classes: HashSet::new(),
            type_analyzer,
            symbol_table,
            bindable_info: None,
            in_class: false,
        }
    }

    fn class_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        if self.evaluated_classes.contains(&Rc::as_ptr(node)) {
            return;
        }
        self.in_class = true;
        let class_name = node_magic::get_class_name(node);
        self.bindable_info = self.class_map.get(&class_name).cloned();
        if let Some(bindable_info) = self.bindable_info.clone() {
            let class_info = bindable_info.borrow().get_class_info().expect("NullPointerException");
            if !class_info.borrow().implements_interface(standard_defs::PACKAGE_FLASH_EVENTS, IEVENT_DISPATCHER) {
                bindable_info.borrow_mut().bindable_mut().set_needs_to_implement_ievent_dispatcher(true);
                let multi_name = MultiName::new(standard_defs::PACKAGE_FLASH_EVENTS, IEVENT_DISPATCHER);
                let interface_info = self.type_analyzer.borrow_mut().analyze_interface(context, &multi_name, &class_info, self.symbol_table);
                match interface_info {
                    None => return,
                    Some(i) => {
                        if self.base.check_for_existing_methods(context, node, &class_info, &i) {
                            return;
                        }
                    }
                }
                class_info.borrow_mut().add_interface_multi_name(standard_defs::PACKAGE_FLASH_EVENTS, IEVENT_DISPATCHER);
            }
            let requires = bindable_info.borrow().bindable().get_requires_static_event_dispatcher();
            if requires && !class_info.borrow().defines_variable(STATIC_EVENT_DISPATCHER) && !class_info.borrow().defines_getter_inherited(STATIC_EVENT_DISPATCHER, true) {
                bindable_info.borrow_mut().bindable_mut().set_needs_static_event_dispatcher(true);
            }
            self.base.post_process_class_info(context, &bindable_info);
            prep_class_def(node);
            let statements = node.borrow().class_definition().unwrap().statements.clone();
            if let Some(statements) = statements {
                adapter_evaluate(self, context, &statements);
                self.modify_syntax_tree(context, node, &bindable_info);
            }
            self.bindable_info = None;
        }
        self.in_class = false;
        self.evaluated_classes.insert(Rc::as_ptr(node));
    }

    fn modify_syntax_tree(&self, context: &ContextRef, class_definition: &NodeRef, bindable_info: &GenerativeClassInfoRef) {
        if self.base.generate_abstract_syntax_tree {
            self.modify_syntax_tree_ast(context, class_definition, bindable_info);
            return;
        }
        let class_name = node_magic::get_unqualified_class_name(class_definition);
        let vars = vec![("bindableInfo", Value::Object(Rc::new(BindableInfoView(bindable_info.clone()))))];
        let gen_program_node = self.base.generate_support_code(context, &class_name, self.standard_defs.get_bindable_property_template(), CODEGEN_TEMPLATE_PATH, vars, "-binding-generated.as");
        self.base.modify_syntax_tree(context, class_definition, bindable_info, gen_program_node);
    }

    fn function_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        if !self.in_class {
            return;
        }
        let qname = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_function_name(node).unwrap_or_default());
        let bindable_info = self.bindable_info.clone().expect("NullPointerException");
        let accessor_info = bindable_info.borrow().get_accessor(&qname);
        let Some(accessor_info) = accessor_info.filter(|a| a.borrow().is_function()) else { return };
        if node_magic::function_is_setter(node) {
            gen::hide_function(node, &accessor_info);
            self.base.register_renamed_accessor(&accessor_info);
            accessor_info.borrow_mut().set_setter_info(node);
        } else {
            accessor_info.borrow_mut().set_getter_info(node);
            let class_info = bindable_info.borrow().get_class_info().expect("NullPointerException");
            if !class_info.borrow().defines_setter_inherited(qname.get_local_part(), false) {
                let pos = node.borrow().pos();
                localized2(context, Level::Error, None, pos, format!("[Bindable] on '{}' getter requires a locally defined setter.", qname.get_local_part()));
            }
        }
    }

    fn variable_definition(&mut self, _context: &ContextRef, node: &NodeRef) {
        if !self.in_class {
            return;
        }
        let qname = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_variable_name(node));
        let bindable_info = self.bindable_info.clone().expect("NullPointerException");
        let accessor_info = bindable_info.borrow().get_accessor(&qname);
        if let Some(accessor_info) = accessor_info {
            gen::hide_variable(node, &accessor_info);
            self.base.register_renamed_accessor(&accessor_info);
        }
    }
}

/// `prepClassDef(node)`: strips class-level `[Bindable]`.
fn prep_class_def(node: &NodeRef) {
    let md = node.borrow().class_definition().unwrap().def.meta_data.clone();
    let Some(md) = md else { return };
    let mut b = md.borrow_mut();
    let list = b.as_statement_list_mut().unwrap();
    list.items.retain(|item| {
        let item = item.clone().expect("NullPointerException");
        let ib = item.borrow();
        let m = ib.meta_data().expect("ClassCastException");
        !(m.get_id().as_deref() == Some(standard_defs::MD_BINDABLE) && m.count() == 0)
    });
}

impl Adapter for BindableSecondPassEvaluator<'_> {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let k = match node.borrow().kind {
            Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) => 0,
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => 1,
            Kind::VariableDefinition(_) => 2,
            _ => return false,
        };
        match k {
            0 => self.class_definition(cx, node),
            1 => self.function_definition(cx, node),
            _ => self.variable_definition(cx, node),
        }
        true
    }
}

const fn c(name: &'static str) -> JType {
    JType::Class(Cow::Borrowed(name))
}

static BINDABLE_INFO_METHODS: &[Method] = &[
    Method::new("needsAdditionalInterfaces", &[], JType::Boolean),
    Method::new("getNeedsToImplementIEventDispatcher", &[], JType::Boolean),
    Method::new("getNeedsStaticEventDispatcher", &[], JType::Boolean),
    Method::new("getRequiresStaticEventDispatcher", &[], JType::Boolean),
    Method::new("getAccessors", &[], c("java.util.Map")),
    Method::new("getClassName", &[], JType::String),
    Method::new("getImports", &[], c("java.util.Set")),
];

/// `BindableInfo` as `BindableProperty.vm` sees it.
pub struct BindableInfoView(pub GenerativeClassInfoRef);

impl JavaObject for BindableInfoView {
    fn class_name(&self) -> &str {
        "flex2.compiler.as3.binding.BindableInfo"
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == self.class_name() || class_name == "flex2.compiler.as3.genext.GenerativeClassInfo"
    }

    fn methods(&self) -> &[Method] {
        BINDABLE_INFO_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        let i = self.0.borrow();
        Ok(match &*method.name {
            "needsAdditionalInterfaces" => Value::Boolean(i.needs_additional_interfaces()),
            "getNeedsToImplementIEventDispatcher" => Value::Boolean(i.bindable().get_needs_to_implement_ievent_dispatcher()),
            "getNeedsStaticEventDispatcher" => Value::Boolean(i.bindable().get_needs_static_event_dispatcher()),
            "getRequiresStaticEventDispatcher" => Value::Boolean(i.bindable().get_requires_static_event_dispatcher()),
            "getAccessors" => match i.get_accessors() {
                None => Value::Null,
                Some(m) => {
                    let class = match m {
                        crate::asc::flex2::compiler::as3::genext::generative_class_info::AccessorMap::Linked(_) => "java.util.LinkedHashMap",
                        _ => "java.util.HashMap",
                    };
                    Value::map_of(class, m.entries().into_iter().map(|(k, v)| (Value::str(&k.to_string()), accessor_info(&v))).collect())
                }
            },
            "getClassName" => i.get_class_name().map_or(Value::Null, |s| Value::str(&s)),
            "getImports" => Value::list_of("java.util.HashSet", i.get_imports().iter().map(|s| Value::str(s)).collect()),
            _ => Value::Null,
        })
    }
}

pub fn accessor_info(a: &AccessorInfoRef) -> Value {
    Value::Object(Rc::new(AccessorInfoView(a.clone())))
}

static ACCESSOR_METHODS: &[Method] = &[
    Method::new("getPropertyName", &[], JType::String),
    Method::new("getUserNamespace", &[], JType::String),
    Method::new("getBackingPrefix", &[], JType::String),
    Method::new("getBackingPropertyName", &[], JType::String),
    Method::new("getTypeName", &[], JType::String),
    Method::new("getAttributes", &[], c("java.util.List")),
    Method::new("getAttributeString", &[], JType::String),
    Method::new("getIsExplicit", &[], JType::Boolean),
    Method::new("getIsStatic", &[], JType::Boolean),
    Method::new("getIsFunction", &[], JType::Boolean),
    Method::new("getQualifiedPropertyName", &[], JType::String),
    Method::new("getQualifiedBackingPropertyName", &[], JType::String),
];

struct AccessorInfoView(AccessorInfoRef);

impl JavaObject for AccessorInfoView {
    fn class_name(&self) -> &str {
        if self.0.borrow().is_function() {
            "flex2.compiler.as3.genext.GenerativeClassInfo$GetterSetterInfo"
        } else {
            "flex2.compiler.as3.genext.GenerativeClassInfo$VariableInfo"
        }
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == self.class_name() || class_name == "flex2.compiler.as3.genext.GenerativeClassInfo$AccessorInfo"
    }

    fn methods(&self) -> &[Method] {
        ACCESSOR_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        let a = self.0.borrow();
        Ok(match &*method.name {
            "getPropertyName" => Value::str(&a.get_property_name()),
            "getUserNamespace" => Value::str(&a.get_user_namespace()),
            "getBackingPrefix" => Value::str(&a.get_backing_prefix()),
            "getBackingPropertyName" => Value::str(&a.get_backing_property_name()),
            "getTypeName" => a.get_type_name().map_or(Value::Null, |s| Value::str(&s)),
            "getAttributes" => Value::list(a.get_attributes().iter().map(|s| Value::str(s)).collect()),
            "getAttributeString" => Value::str(&a.get_attribute_string()),
            "getIsExplicit" => Value::Boolean(a.get_is_explicit()),
            "getIsStatic" => Value::Boolean(a.get_is_static()),
            "getIsFunction" => Value::Boolean(a.is_function()),
            "getQualifiedPropertyName" => Value::str(&a.get_qualified_property_name()),
            "getQualifiedBackingPropertyName" => Value::str(&a.get_qualified_backing_property_name()),
            _ => Value::Null,
        })
    }
}

const ADD_EVENT_LISTENER: &str = "addEventListener";
const BINDABLE: &str = "Bindable";
const BOOLEAN: &str = "Boolean";
const CREATE_UPDATE_EVENT: &str = "createUpdateEvent";
const DISPATCH_EVENT: &str = "dispatchEvent";
const EVENT_VAR: &str = "event";
const EVENT_CLASS: &str = "Event";
const EVENT_DISPATCHER_VAR: &str = "eventDispatcher";
const EVENT_DISPATCHER_CLASS: &str = "EventDispatcher";
const FLASH_EVENTS: &str = "flash.events";
const FUNCTION: &str = "Function";
const HAS_EVENT_LISTENER: &str = "hasEventListener";
const INT: &str = "int";
const I_EVENT_DISPATCHER: &str = "IEventDispatcher";
const LISTENER: &str = "listener";
const MX_EVENTS: &str = "mx.events";
const OBJECT: &str = "Object";
const OLD_VALUE: &str = "oldValue";
const PRIORITY: &str = "priority";
const PROPERTY_CHANGE: &str = "propertyChange";
const PROPERTY_CHANGE_EVENT: &str = "PropertyChangeEvent";
const REMOVE_EVENT_LISTENER: &str = "removeEventListener";
const STRING: &str = "String";
const TYPE: &str = "type";
const USE_CAPTURE: &str = "useCapture";
const VALUE: &str = "value";
const WEAK_REF: &str = "weakRef";
const WILL_TRIGGER: &str = "willTrigger";
const _BINDING_EVENT_DISPATCHER: &str = "_bindingEventDispatcher";
const _STATIC_BINDING_EVENT_DISPATCHER: &str = "_staticBindingEventDispatcher";

/// The direct AST generation half of `BindableSecondPassEvaluator`.
impl BindableSecondPassEvaluator<'_> {
    fn add_ievent_dispatcher_implementation(&self, context: &ContextRef, class_definition: &NodeRef) {
        let nf = Nf::of(context);
        let member_expression = ast::generate_getter_selector_qualified(&mut nf.f(), FLASH_EVENTS, I_EVENT_DISPATCHER, false);
        let interfaces = class_definition.borrow().class_definition().unwrap().interfaces.clone();
        let interfaces = nf.f().list(interfaces, Some(member_expression));
        class_definition.borrow_mut().class_definition_mut().unwrap().interfaces = Some(interfaces);
        let append = |node: NodeRef| {
            let statements = class_definition.borrow().class_definition().unwrap().statements.clone();
            let statements = nf.f().statement_list(statements, Some(node));
            class_definition.borrow_mut().class_definition_mut().unwrap().statements = Some(statements);
        };
        append(generate_binding_event_dispatcher_variable(&nf));
        let doc = ast::generate_inherit_doc_comment(&mut nf.f());
        append(doc);
        append(generate_add_event_listener_function_definition(context));
        let doc = ast::generate_inherit_doc_comment(&mut nf.f());
        append(doc);
        append(generate_dispatch_event_function_definition(context));
        let doc = ast::generate_inherit_doc_comment(&mut nf.f());
        append(doc);
        append(generate_has_event_listener_function_definition(context));
        let doc = ast::generate_inherit_doc_comment(&mut nf.f());
        append(doc);
        append(generate_remove_event_listener_function_definition(context));
        let doc = ast::generate_inherit_doc_comment(&mut nf.f());
        append(doc);
        append(generate_will_trigger_function_definition(context));
    }

    fn add_static_event_dispatcher_implementation(&self, context: &ContextRef, class_definition: &NodeRef) {
        let nf = Nf::of(context);
        let append = |node: NodeRef| {
            let statements = class_definition.borrow().class_definition().unwrap().statements.clone();
            let statements = nf.f().statement_list(statements, Some(node));
            class_definition.borrow_mut().class_definition_mut().unwrap().statements = Some(statements);
        };
        append(generate_static_binding_event_dispatcher_variable(&nf));
        append(generate_static_event_dispatcher_getter(context));
    }

    /// `modifySyntaxTree(context, classDefinition, bindableInfo)` with `generateAbstractSyntaxTree`.
    fn modify_syntax_tree_ast(&self, context: &ContextRef, class_definition: &NodeRef, bindable_info: &GenerativeClassInfoRef) {
        let accessors = bindable_info.borrow().get_accessors().map(|a| a.values());
        if let Some(accessors) = accessors {
            for accessor_info in accessors {
                let nf = Nf::of(context);
                let class_name = bindable_info.borrow().get_class_name().expect("NullPointerException");
                let statement_list = class_definition.borrow().class_definition().unwrap().statements.clone().expect("NullPointerException");
                let is_function = accessor_info.borrow().is_function();
                if !is_function {
                    let getter = generate_getter(context, &class_name, &accessor_info);
                    let from = accessor_info.borrow().get_definition_node();
                    self.move_meta_data_to_new_definition(&nf, Some(&from), &getter, &statement_list, true);
                    nf.f().statement_list(Some(statement_list.clone()), Some(getter));
                }
                let setter = generate_setter(context, &class_name, &accessor_info);
                if is_function {
                    let (getter_fd, setter_fd) = match &accessor_info.borrow().kind {
                        AccessorKind::GetterSetter { getter_function_definition, setter_function_definition, .. } => (getter_function_definition.clone(), setter_function_definition.clone()),
                        _ => (None, None),
                    };
                    let processed_bindable = self.process_getter_meta_data(&nf, getter_fd.as_ref());
                    self.move_meta_data_to_new_definition(&nf, setter_fd.as_ref(), &setter, &statement_list, !processed_bindable);
                }
                nf.f().statement_list(Some(statement_list), Some(setter));
            }
        }
        let (needs_iev, needs_static) = {
            let b = bindable_info.borrow();
            (b.bindable().get_needs_to_implement_ievent_dispatcher(), b.bindable().get_needs_static_event_dispatcher())
        };
        if needs_iev {
            self.add_ievent_dispatcher_implementation(context, class_definition);
        }
        if needs_static {
            self.add_static_event_dispatcher_implementation(context, class_definition);
        }
    }

    /// `moveMetaDataToNewDefinition(nodeFactory, fromDefinition, toDefinition, classDefinitionStatementList, addBindableMetaData)`.
    fn move_meta_data_to_new_definition(&self, nf: &Nf, from_definition: Option<&NodeRef>, to_definition: &NodeRef, class_definition_statement_list: &NodeRef, add_bindable_meta_data: bool) {
        let context = nf.f().get_context();
        let mut processed_bindable_meta_data = false;
        let from_meta = from_definition.and_then(|d| d.borrow().definition().and_then(|d| d.meta_data.clone()));
        if let (Some(from_definition), Some(from_meta)) = (from_definition, from_meta) {
            let items: Vec<NodeRef> = from_meta.borrow().as_statement_list().unwrap().items.iter().flatten().cloned().collect();
            for meta_data in items {
                let is_empty_bindable = {
                    let b = meta_data.borrow();
                    let m = b.meta_data().expect("ClassCastException");
                    m.get_id().as_deref() == Some(standard_defs::MD_BINDABLE) && m.count() == 0
                };
                if is_empty_bindable {
                    self.process_bindable_meta_data(&context, &meta_data);
                    processed_bindable_meta_data = true;
                }
                meta_data.borrow_mut().meta_data_mut().unwrap().def = Some(to_definition.clone());
                {
                    let mut s = class_definition_statement_list.borrow_mut();
                    let items = &mut s.as_statement_list_mut().unwrap().items;
                    if let Some(i) = items.iter().position(|n| n.as_ref().is_some_and(|n| Rc::ptr_eq(n, &meta_data))) {
                        items.remove(i);
                    }
                }
                nf.f().statement_list(Some(class_definition_statement_list.clone()), Some(meta_data.clone()));
                let to_meta = to_definition.borrow().definition().unwrap().meta_data.clone();
                let to_meta = nf.f().statement_list(to_meta, Some(meta_data));
                to_definition.borrow_mut().definition_mut().unwrap().meta_data = Some(to_meta);
            }
            from_definition.borrow_mut().definition_mut().unwrap().meta_data = None;
        }
        if add_bindable_meta_data && !processed_bindable_meta_data {
            let bindable_meta_data = ast::generate_meta_data_key_value(&mut nf.f(), BINDABLE, EVENT_VAR, PROPERTY_CHANGE);
            nf.f().statement_list(Some(class_definition_statement_list.clone()), Some(bindable_meta_data.clone()));
            self.base.prep_meta_data_node(&context, &bindable_meta_data);
        }
    }

    /// `processBindableMetaData(nodeFactory, metaData)`: gives empty `[Bindable]` an `event` attribute.
    fn process_bindable_meta_data(&self, context: &ContextRef, meta_data: &NodeRef) {
        let vals = vec![crate::asc::semantics::value::Value::KeyValuePair(Rc::new(crate::asc::semantics::value::KeyValuePair::new(EVENT_VAR, PROPERTY_CHANGE)))];
        meta_data.borrow_mut().meta_data_mut().unwrap().set_values(Some(vals));
        self.base.prep_meta_data_node(context, meta_data);
    }

    /// `processGetterMetaData(nodeFactory, getter)`.
    fn process_getter_meta_data(&self, nf: &Nf, getter: Option<&NodeRef>) -> bool {
        let context = nf.f().get_context();
        let mut processed_bindable_meta_data = false;
        let meta = getter.and_then(|g| g.borrow().definition().and_then(|d| d.meta_data.clone()));
        if let Some(meta) = meta {
            let items: Vec<NodeRef> = meta.borrow().as_statement_list().unwrap().items.iter().flatten().cloned().collect();
            for meta_data in items {
                let is_empty_bindable = {
                    let b = meta_data.borrow();
                    let m = b.meta_data().expect("ClassCastException");
                    m.get_id().as_deref() == Some(standard_defs::MD_BINDABLE) && m.count() == 0
                };
                if is_empty_bindable {
                    self.process_bindable_meta_data(&context, &meta_data);
                    processed_bindable_meta_data = true;
                }
            }
        }
        processed_bindable_meta_data
    }
}

fn function_definition(context: &ContextRef, nf: &Nf, attribute_list: NodeRef, function_name: NodeRef, function_signature: NodeRef, function_statement_list: NodeRef) -> NodeRef {
    let function_common = nf.f().function_common(context, None, Some(function_signature), Some(function_statement_list));
    ast::user_defined_body(&function_common);
    nf.f().function_definition(context, Some(attribute_list), Some(function_name), Some(function_common))
}

fn generate_attribute_list(nf: &Nf, attribute_string: &str) -> Option<NodeRef> {
    if attribute_string.is_empty() {
        return None;
    }
    match attribute_string.find(' ') {
        Some(index) => {
            let identifier = nf.f().identifier(&attribute_string[index + 1..]);
            let attribute_list = nf.f().attribute_list(Some(identifier), None);
            let member_expression = ast::generate_getter_selector(&mut nf.f(), &attribute_string[..index], true);
            let list = nf.f().list(None, Some(member_expression));
            Some(nf.f().attribute_list(Some(list), Some(attribute_list)))
        }
        None => {
            let member_expression = ast::generate_getter_selector(&mut nf.f(), attribute_string, true);
            let list = nf.f().list(None, Some(member_expression));
            Some(nf.f().attribute_list(Some(list), None))
        }
    }
}

fn generate_binding_event_dispatcher_variable(nf: &Nf) -> NodeRef {
    let attribute_list = ast::generate_private_attribute(&mut nf.f());
    let identifier = nf.f().identifier_intern(_BINDING_EVENT_DISPATCHER, false);
    let event_dispatcher_qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), FLASH_EVENTS, EVENT_DISPATCHER_CLASS, false);
    let i_event_dispatcher_qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), FLASH_EVENTS, I_EVENT_DISPATCHER, false);
    let this_expression = nf.f().this_expression(0);
    let cast_argument_list = nf.f().argument_list(None, Some(this_expression));
    let cast_call_expression = ast::call(nf, i_event_dispatcher_qualified_identifier, Some(cast_argument_list), false);
    let inner_member_expression = nf.f().member_expression(None, Some(cast_call_expression));
    let argument_list = nf.f().argument_list(None, Some(inner_member_expression));
    let call_expression = ast::call(nf, event_dispatcher_qualified_identifier, Some(argument_list), true);
    let member_expression = nf.f().member_expression(None, Some(call_expression));
    ast::generate_variable_attrs_ns(&mut nf.f(), Some(attribute_list), identifier, FLASH_EVENTS, EVENT_DISPATCHER_CLASS, false, Some(member_expression))
}

fn generate_dispatch_event_call(nf: &Nf, then: Option<NodeRef>, qualified_property_name: &str) -> NodeRef {
    let inner_this_expression = nf.f().this_expression(0);
    let dispatch_event_identifier = nf.f().identifier_intern(DISPATCH_EVENT, false);
    let property_change_event_member_expression = ast::generate_getter_selector_qualified(&mut nf.f(), MX_EVENTS, PROPERTY_CHANGE_EVENT, false);
    let create_update_event_identifier = nf.f().identifier_intern(CREATE_UPDATE_EVENT, false);
    let this_expression = nf.f().this_expression(0);
    let args = nf.f().argument_list(None, Some(inner_this_expression));
    let literal_string = nf.f().literal_string(qualified_property_name);
    let args = nf.f().argument_list(Some(args), Some(literal_string));
    let old_value_member_expression = ast::generate_getter_selector(&mut nf.f(), OLD_VALUE, false);
    let args = nf.f().argument_list(Some(args), Some(old_value_member_expression));
    let value_member_expression = ast::generate_getter_selector(&mut nf.f(), VALUE, false);
    let args = nf.f().argument_list(Some(args), Some(value_member_expression));
    let create_update_event_call_expression = ast::call(nf, create_update_event_identifier, Some(args), false);
    let create_update_event_member_expression = nf.f().member_expression(Some(property_change_event_member_expression), Some(create_update_event_call_expression));
    let dispatch_event_argument_list = nf.f().argument_list(None, Some(create_update_event_member_expression));
    let dispatch_event_call_expression = ast::call(nf, dispatch_event_identifier, Some(dispatch_event_argument_list), false);
    let expression_statement = ast::expr_stmt(nf, Some(this_expression), dispatch_event_call_expression);
    let if_this_expression = nf.f().this_expression(0);
    let has_event_listener_identifier = nf.f().identifier_intern(HAS_EVENT_LISTENER, false);
    let prop_change_literal_string = nf.f().literal_string(PROPERTY_CHANGE);
    let has_args = nf.f().argument_list(None, Some(prop_change_literal_string));
    let has_event_listener_call_expression = ast::call(nf, has_event_listener_identifier, Some(has_args), false);
    let if_member_expression = nf.f().member_expression(Some(if_this_expression), Some(has_event_listener_call_expression));
    let iftest = nf.f().list(None, Some(if_member_expression));
    let if_statement = nf.f().if_statement(Some(iftest), Some(expression_statement), None);
    nf.f().statement_list(then, Some(if_statement))
}

fn generate_add_event_listener_function_definition(context: &ContextRef) -> NodeRef {
    let nf = Nf::of(context);
    let attribute_list = ast::generate_public_attribute(&mut nf.f());
    let add_event_listener_identifier = nf.f().identifier_intern(ADD_EVENT_LISTENER, false);
    let function_name = nf.f().function_name(EMPTY_TOKEN, Some(add_event_listener_identifier));
    let p = ast::generate_parameter(&mut nf.f(), TYPE, STRING, false);
    let parameter_list = nf.f().parameter_list(None, Some(p));
    let p = ast::generate_parameter(&mut nf.f(), LISTENER, FUNCTION, false);
    let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
    let literal_boolean = nf.f().literal_boolean(false);
    let p = ast::generate_parameter_init(&mut nf.f(), USE_CAPTURE, BOOLEAN, false, Some(literal_boolean));
    let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
    let literal_number = nf.f().literal_number_int(0);
    let p = ast::generate_parameter_init(&mut nf.f(), PRIORITY, INT, false, Some(literal_number));
    let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
    let literal_boolean = nf.f().literal_boolean(false);
    let p = ast::generate_parameter_init(&mut nf.f(), WEAK_REF, BOOLEAN, false, Some(literal_boolean));
    let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
    let function_signature = nf.f().function_signature(Some(parameter_list), None);
    ast::void_anno(&function_signature);
    let base = ast::generate_getter_selector(&mut nf.f(), _BINDING_EVENT_DISPATCHER, false);
    let identifier = nf.f().identifier_intern(ADD_EVENT_LISTENER, false);
    let mut argument_list: Option<NodeRef> = None;
    for name in [TYPE, LISTENER, USE_CAPTURE, PRIORITY, WEAK_REF] {
        let g = ast::generate_getter_selector(&mut nf.f(), name, false);
        argument_list = Some(nf.f().argument_list(argument_list, Some(g)));
    }
    let call_expression = ast::call(&nf, identifier, argument_list, false);
    let expression_statement = ast::expr_stmt(&nf, Some(base), call_expression);
    let function_statement_list = nf.f().statement_list(None, Some(expression_statement));
    function_definition(context, &nf, attribute_list, function_name, function_signature, function_statement_list)
}

fn generate_dispatch_event_function_definition(context: &ContextRef) -> NodeRef {
    let nf = Nf::of(context);
    let attribute_list = ast::generate_public_attribute(&mut nf.f());
    let dispatch_event_identifier = nf.f().identifier_intern(DISPATCH_EVENT, false);
    let function_name = nf.f().function_name(EMPTY_TOKEN, Some(dispatch_event_identifier));
    let parameter = ast::generate_parameter_ns(&mut nf.f(), EVENT_VAR, FLASH_EVENTS, EVENT_CLASS, false);
    let parameter_list = nf.f().parameter_list(None, Some(parameter));
    let return_type_member_expression = ast::generate_getter_selector(&mut nf.f(), BOOLEAN, true);
    let return_type = nf.f().type_expression(Some(return_type_member_expression), true, false, -1);
    let function_signature = nf.f().function_signature(Some(parameter_list), Some(return_type));
    let base = ast::generate_getter_selector(&mut nf.f(), _BINDING_EVENT_DISPATCHER, false);
    let identifier = nf.f().identifier_intern(DISPATCH_EVENT, false);
    let event_getter_selector = ast::generate_getter_selector(&mut nf.f(), EVENT_VAR, false);
    let argument_list = nf.f().argument_list(None, Some(event_getter_selector));
    let call_expression = ast::call(&nf, identifier, Some(argument_list), false);
    let member_expression = nf.f().member_expression(Some(base), Some(call_expression));
    let return_list = nf.f().list(None, Some(member_expression));
    let return_statement = nf.f().return_statement(Some(return_list));
    let function_statement_list = nf.f().statement_list(None, Some(return_statement));
    function_definition(context, &nf, attribute_list, function_name, function_signature, function_statement_list)
}

fn generate_event_dispatcher_not_null(nf: &Nf) -> NodeRef {
    let member_expression = ast::generate_getter_selector(&mut nf.f(), EVENT_DISPATCHER_VAR, false);
    let literal_null = nf.f().literal_null();
    let binary_expression = nf.f().binary_expression(STRICTNOTEQUALS_TOKEN, Some(member_expression), Some(literal_null));
    nf.f().list(None, Some(binary_expression))
}

fn generate_getter(context: &ContextRef, class_name: &str, accessor_info: &AccessorInfoRef) -> NodeRef {
    let nf = Nf::of(context);
    let a = accessor_info.borrow();
    let type_name = a.get_type_name().unwrap_or_default();
    let position = match &a.kind {
        AccessorKind::Variable { position, .. } => *position,
        _ => panic!("ClassCastException"),
    };
    let return_type = ast::generate_type_expression(&mut nf.f(), &type_name, true);
    let function_signature = nf.f().function_signature(None, return_type);
    let attribute_list = generate_attribute_list(&nf, &a.get_attribute_string());
    let identifier = nf.f().identifier(&a.get_property_name());
    let function_name = nf.f().function_name(GET_TOKEN, Some(identifier));
    let qualified_backing_property_name = a.get_qualified_backing_property_name();
    let return_statement = if a.get_is_static() {
        let getter_selector = ast::generate_getter_selector(&mut nf.f(), class_name, false);
        let identifier = ast::generate_identifier(&mut nf.f(), &qualified_backing_property_name, false);
        let get_expression = nf.f().get_expression(Some(identifier));
        let member_expression = nf.f().member_expression(Some(getter_selector), Some(get_expression));
        let return_list = nf.f().list(None, Some(member_expression));
        nf.f().return_statement(Some(return_list))
    } else {
        let this_expression = nf.f().this_expression(0);
        let identifier = ast::generate_identifier(&mut nf.f(), &qualified_backing_property_name, false);
        let get_expression = nf.f().get_expression(Some(identifier));
        let member_expression = nf.f().member_expression(Some(this_expression), Some(get_expression));
        let return_list = nf.f().list(None, Some(member_expression));
        nf.f().return_statement(Some(return_list))
    };
    let function_statement_list = nf.f().statement_list(None, Some(return_statement));
    let function_common = nf.f().function_common_pos(context, None, Some(function_signature), Some(function_statement_list), position);
    ast::user_defined_body(&function_common);
    let result = nf.f().function_definition(context, attribute_list, Some(function_name), Some(function_common));
    result
}

fn generate_has_event_listener_function_definition(context: &ContextRef) -> NodeRef {
    generate_type_to_boolean_forwarder(context, HAS_EVENT_LISTENER)
}

/// `public function <name>(type:String):Boolean { return _bindingEventDispatcher.<name>(type); }`.
fn generate_type_to_boolean_forwarder(context: &ContextRef, name: &str) -> NodeRef {
    let nf = Nf::of(context);
    let attribute_list = ast::generate_public_attribute(&mut nf.f());
    let name_identifier = nf.f().identifier_intern(name, false);
    let function_name = nf.f().function_name(EMPTY_TOKEN, Some(name_identifier));
    let type_parameter = ast::generate_parameter(&mut nf.f(), TYPE, STRING, false);
    let parameter_list = nf.f().parameter_list(None, Some(type_parameter));
    let return_type_member_expression = ast::generate_getter_selector(&mut nf.f(), BOOLEAN, true);
    let return_type = nf.f().type_expression(Some(return_type_member_expression), true, false, -1);
    let function_signature = nf.f().function_signature(Some(parameter_list), Some(return_type));
    let base = ast::generate_getter_selector(&mut nf.f(), _BINDING_EVENT_DISPATCHER, false);
    let identifier = nf.f().identifier_intern(name, false);
    let type_getter_selector = ast::generate_getter_selector(&mut nf.f(), TYPE, false);
    let argument_list = nf.f().argument_list(None, Some(type_getter_selector));
    let call_expression = ast::call(&nf, identifier, Some(argument_list), false);
    let member_expression = nf.f().member_expression(Some(base), Some(call_expression));
    let return_list = nf.f().list(None, Some(member_expression));
    let return_statement = nf.f().return_statement(Some(return_list));
    let function_statement_list = nf.f().statement_list(None, Some(return_statement));
    function_definition(context, &nf, attribute_list, function_name, function_signature, function_statement_list)
}

fn generate_old_value_strictly_not_equals_value_text(nf: &Nf) -> NodeRef {
    let old_value_member_expression = ast::generate_getter_selector(&mut nf.f(), OLD_VALUE, false);
    let value_member_expression = ast::generate_getter_selector(&mut nf.f(), VALUE, false);
    let binary_expression = nf.f().binary_expression(STRICTNOTEQUALS_TOKEN, Some(old_value_member_expression), Some(value_member_expression));
    nf.f().list(None, Some(binary_expression))
}

fn generate_old_value_variable(nf: &Nf, setter_access_property_name: &str) -> NodeRef {
    let this_expression = nf.f().this_expression(0);
    let identifier = ast::generate_identifier(&mut nf.f(), setter_access_property_name, false);
    let get_expression = nf.f().get_expression(Some(identifier));
    let member_expression = nf.f().member_expression(Some(this_expression), Some(get_expression));
    ast::generate_variable(&mut nf.f(), OLD_VALUE, OBJECT, false, Some(member_expression))
}

fn generate_remove_event_listener_function_definition(context: &ContextRef) -> NodeRef {
    let nf = Nf::of(context);
    let attribute_list = ast::generate_public_attribute(&mut nf.f());
    let remove_event_listener_identifier = nf.f().identifier_intern(REMOVE_EVENT_LISTENER, false);
    let function_name = nf.f().function_name(EMPTY_TOKEN, Some(remove_event_listener_identifier));
    let p = ast::generate_parameter(&mut nf.f(), TYPE, STRING, false);
    let parameter_list = nf.f().parameter_list(None, Some(p));
    let p = ast::generate_parameter(&mut nf.f(), LISTENER, FUNCTION, false);
    let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
    let literal_boolean = nf.f().literal_boolean(false);
    let p = ast::generate_parameter_init(&mut nf.f(), USE_CAPTURE, BOOLEAN, false, Some(literal_boolean));
    let parameter_list = nf.f().parameter_list(Some(parameter_list), Some(p));
    let function_signature = nf.f().function_signature(Some(parameter_list), None);
    ast::void_anno(&function_signature);
    let base = ast::generate_getter_selector(&mut nf.f(), _BINDING_EVENT_DISPATCHER, false);
    let identifier = nf.f().identifier_intern(REMOVE_EVENT_LISTENER, false);
    let mut argument_list: Option<NodeRef> = None;
    for name in [TYPE, LISTENER, USE_CAPTURE] {
        let g = ast::generate_getter_selector(&mut nf.f(), name, false);
        argument_list = Some(nf.f().argument_list(argument_list, Some(g)));
    }
    let call_expression = ast::call(&nf, identifier, argument_list, false);
    let expression_statement = ast::expr_stmt(&nf, Some(base), call_expression);
    let function_statement_list = nf.f().statement_list(None, Some(expression_statement));
    function_definition(context, &nf, attribute_list, function_name, function_signature, function_statement_list)
}

fn generate_setter(context: &ContextRef, class_name: &str, accessor_info: &AccessorInfoRef) -> NodeRef {
    let nf = Nf::of(context);
    let a = accessor_info.borrow();
    let position = match &a.kind {
        AccessorKind::Variable { position, .. } => *position,
        AccessorKind::GetterSetter { .. } => a.get_setter_position(),
    };
    let parameter = ast::generate_parameter_pos(&mut nf.f(), VALUE, &a.get_type_name().unwrap_or_default(), true, position);
    let parameter_list = nf.f().parameter_list(None, Some(parameter));
    let function_signature = nf.f().function_signature(Some(parameter_list), None);
    ast::void_anno(&function_signature);
    let attribute_list = generate_attribute_list(&nf, &a.get_attribute_string());
    let property_name_identifier = nf.f().identifier(&a.get_property_name());
    let function_name = nf.f().function_name(SET_TOKEN, Some(property_name_identifier));
    let qualified_backing_property_name = a.get_qualified_backing_property_name();
    let setter_access_property_name = if a.is_function() { a.get_qualified_property_name() } else { qualified_backing_property_name.clone() };
    let function_statement_list = if a.get_is_static() {
        let variable_definition = generate_static_old_value_variable(&nf, class_name, &setter_access_property_name);
        let function_statement_list = nf.f().statement_list(None, Some(variable_definition));
        let test = generate_old_value_strictly_not_equals_value_text(&nf);
        let then = generate_static_setter_assignment(&nf, class_name, &qualified_backing_property_name);
        let then = generate_static_dispatch_event_call(&nf, Some(then), class_name, &a.get_qualified_property_name());
        let if_statement = nf.f().if_statement(Some(test), Some(then), None);
        nf.f().statement_list(Some(function_statement_list), Some(if_statement))
    } else {
        let variable_definition = generate_old_value_variable(&nf, &setter_access_property_name);
        let function_statement_list = nf.f().statement_list(None, Some(variable_definition));
        let test = generate_old_value_strictly_not_equals_value_text(&nf);
        let then = generate_setter_assignment(&nf, &qualified_backing_property_name);
        let then = generate_dispatch_event_call(&nf, Some(then), &a.get_qualified_property_name());
        let if_statement = nf.f().if_statement(Some(test), Some(then), None);
        nf.f().statement_list(Some(function_statement_list), Some(if_statement))
    };
    let function_common = nf.f().function_common(context, None, Some(function_signature), Some(function_statement_list));
    ast::user_defined_body(&function_common);
    let result = nf.f().function_definition(context, attribute_list, Some(function_name), Some(function_common));
    result
}

fn generate_setter_assignment(nf: &Nf, qualified_backing_property_name: &str) -> NodeRef {
    let outer_this_expression = nf.f().this_expression(0);
    let identifier = ast::generate_identifier(&mut nf.f(), qualified_backing_property_name, false);
    let getter_selector = ast::generate_getter_selector(&mut nf.f(), VALUE, false);
    let argument_list = nf.f().argument_list(None, Some(getter_selector));
    let set_expression = nf.f().set_expression(Some(identifier), Some(argument_list), false);
    let expression_statement = ast::expr_stmt(nf, Some(outer_this_expression), set_expression);
    nf.f().statement_list(None, Some(expression_statement))
}

fn generate_static_binding_event_dispatcher_variable(nf: &Nf) -> NodeRef {
    let attribute_list = ast::generate_private_static_attribute(&mut nf.f());
    let identifier = nf.f().identifier_intern(_STATIC_BINDING_EVENT_DISPATCHER, false);
    let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), FLASH_EVENTS, EVENT_DISPATCHER_CLASS, false);
    let call_expression = ast::call(nf, qualified_identifier, None, true);
    let member_expression = nf.f().member_expression(None, Some(call_expression));
    ast::generate_variable_attrs_ns(&mut nf.f(), Some(attribute_list), identifier, FLASH_EVENTS, EVENT_DISPATCHER_CLASS, false, Some(member_expression))
}

fn generate_static_dispatch_event_call(nf: &Nf, outer_then: Option<NodeRef>, class_name: &str, qualified_property_name: &str) -> NodeRef {
    let base = ast::generate_getter_selector(&mut nf.f(), class_name, false);
    let static_event_dispatcher_identifier = nf.f().identifier_intern(STATIC_EVENT_DISPATCHER, false);
    let selector = nf.f().get_expression(Some(static_event_dispatcher_identifier));
    let rvalue = nf.f().member_expression(Some(base), Some(selector));
    let variable_definition = ast::generate_variable_ns(&mut nf.f(), EVENT_DISPATCHER_VAR, FLASH_EVENTS, I_EVENT_DISPATCHER, false, Some(rvalue));
    let outer_then = nf.f().statement_list(outer_then, Some(variable_definition));
    let test = generate_event_dispatcher_not_null(nf);
    let dispatch_event_identifier = nf.f().identifier_intern(DISPATCH_EVENT, false);
    let property_change_event_member_expression = ast::generate_getter_selector_qualified(&mut nf.f(), MX_EVENTS, PROPERTY_CHANGE_EVENT, false);
    let create_update_event_identifier = nf.f().identifier_intern(CREATE_UPDATE_EVENT, false);
    let getter_selector = ast::generate_getter_selector(&mut nf.f(), class_name, false);
    let args = nf.f().argument_list(None, Some(getter_selector));
    let literal_string = nf.f().literal_string(qualified_property_name);
    let args = nf.f().argument_list(Some(args), Some(literal_string));
    let old_value_member_expression = ast::generate_getter_selector(&mut nf.f(), OLD_VALUE, false);
    let args = nf.f().argument_list(Some(args), Some(old_value_member_expression));
    let value_member_expression = ast::generate_getter_selector(&mut nf.f(), VALUE, false);
    let args = nf.f().argument_list(Some(args), Some(value_member_expression));
    let create_update_event_call_expression = ast::call(nf, create_update_event_identifier, Some(args), false);
    let create_update_event_member_expression = nf.f().member_expression(Some(property_change_event_member_expression), Some(create_update_event_call_expression));
    let dispatch_event_argument_list = nf.f().argument_list(None, Some(create_update_event_member_expression));
    let dispatch_event_call_expression = ast::call(nf, dispatch_event_identifier, Some(dispatch_event_argument_list), false);
    let event_dispatcher_member_expression = ast::generate_getter_selector(&mut nf.f(), EVENT_DISPATCHER_VAR, false);
    let expression_statement = ast::expr_stmt(nf, Some(event_dispatcher_member_expression), dispatch_event_call_expression);
    let then = nf.f().statement_list(None, Some(expression_statement));
    let if_statement = nf.f().if_statement(Some(test), Some(then), None);
    nf.f().statement_list(Some(outer_then), Some(if_statement))
}

fn generate_static_event_dispatcher_getter(context: &ContextRef) -> NodeRef {
    let nf = Nf::of(context);
    let return_type_member_expression = ast::generate_getter_selector(&mut nf.f(), I_EVENT_DISPATCHER, true);
    let return_type = nf.f().type_expression(Some(return_type_member_expression), true, false, -1);
    let function_signature = nf.f().function_signature(None, Some(return_type));
    let attribute_list = ast::generate_public_static_attribute(&mut nf.f());
    let static_event_dispatcher_identifier = nf.f().identifier_intern(STATIC_EVENT_DISPATCHER, false);
    let function_name = nf.f().function_name(GET_TOKEN, Some(static_event_dispatcher_identifier));
    let member_expression = ast::generate_getter_selector(&mut nf.f(), _STATIC_BINDING_EVENT_DISPATCHER, false);
    let return_list = nf.f().list(None, Some(member_expression));
    let return_statement = nf.f().return_statement(Some(return_list));
    let function_statement_list = nf.f().statement_list(None, Some(return_statement));
    function_definition(context, &nf, attribute_list, function_name, function_signature, function_statement_list)
}

fn generate_static_old_value_variable(nf: &Nf, class_name: &str, qualified_backing_property_name: &str) -> NodeRef {
    let base = ast::generate_getter_selector(&mut nf.f(), class_name, false);
    let identifier = nf.f().identifier_intern(qualified_backing_property_name, false);
    let get_expression = nf.f().get_expression(Some(identifier));
    let member_expression = nf.f().member_expression(Some(base), Some(get_expression));
    ast::generate_variable(&mut nf.f(), OLD_VALUE, OBJECT, false, Some(member_expression))
}

fn generate_static_setter_assignment(nf: &Nf, class_name: &str, qualified_backing_property_name: &str) -> NodeRef {
    let base = ast::generate_getter_selector(&mut nf.f(), class_name, false);
    let identifier = ast::generate_identifier(&mut nf.f(), qualified_backing_property_name, false);
    let getter_selector = ast::generate_getter_selector(&mut nf.f(), VALUE, false);
    let argument_list = nf.f().argument_list(None, Some(getter_selector));
    let set_expression = nf.f().set_expression(Some(identifier), Some(argument_list), false);
    let expression_statement = ast::expr_stmt(nf, Some(base), set_expression);
    nf.f().statement_list(None, Some(expression_statement))
}

fn generate_will_trigger_function_definition(context: &ContextRef) -> NodeRef {
    generate_type_to_boolean_forwarder(context, WILL_TRIGGER)
}
