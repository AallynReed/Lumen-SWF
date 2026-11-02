//! Port of `flex2.compiler.as3.binding.DataBindingInfo`, with the Velocity views `WatcherSetupUtil.vm` reads.

use std::borrow::Cow;
use std::rc::Rc;

use super::watcher::{ChangeEvent, WatcherRef};
use crate::asc::flex2::compiler::mxml::gen::template_values;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};
use crate::asc::util::jhash::{JHashMap, JHashSet};

pub struct DataBindingInfo {
    class_name: Option<String>,
    root_watchers: JHashMap<String, WatcherRef>,
    binding_expressions: Option<Vec<BindingExpressionRef>>,
    watcher_setup_util_class_name: Option<String>,
    imports: JHashSet<String>,
}

impl DataBindingInfo {
    pub fn new(imports: JHashSet<String>) -> DataBindingInfo {
        DataBindingInfo { class_name: None, root_watchers: JHashMap::new(), binding_expressions: None, watcher_setup_util_class_name: None, imports }
    }

    pub fn get_binding_expressions(&self) -> Option<&[BindingExpressionRef]> {
        self.binding_expressions.as_deref()
    }

    pub fn get_class_name(&self) -> Option<String> {
        self.class_name.clone()
    }

    pub fn get_imports(&self) -> &JHashSet<String> {
        &self.imports
    }

    pub fn get_root_watchers(&mut self) -> &mut JHashMap<String, WatcherRef> {
        &mut self.root_watchers
    }

    pub fn get_watcher_setup_util_class_name(&self) -> Option<String> {
        self.watcher_setup_util_class_name.clone()
    }

    pub fn set_binding_expressions(&mut self, binding_expressions: Vec<BindingExpressionRef>) {
        self.binding_expressions = Some(binding_expressions);
    }

    pub fn set_class_name(&mut self, class_name: &str) {
        let c = name_formatter::to_dot(class_name);
        self.watcher_setup_util_class_name = Some(format!("_{}WatcherSetupUtil", c.replace('.', "_")));
        self.class_name = Some(c);
    }
}

const fn c(name: &'static str) -> JType {
    JType::Class(Cow::Borrowed(name))
}

fn opt(s: Option<String>) -> Value {
    s.map_or(Value::Null, |s| Value::str(&s))
}

static INFO_METHODS: &[Method] = &[
    Method::new("getBindingExpressions", &[], c("java.util.List")),
    Method::new("getClassName", &[], JType::String),
    Method::new("getImports", &[], c("java.util.Set")),
    Method::new("getRootWatchers", &[], c("java.util.Map")),
    Method::new("getWatcherSetupUtilClassName", &[], JType::String),
    Method::new("setClassName", &[JType::String], JType::Void),
];

pub struct DataBindingInfoView(pub Rc<std::cell::RefCell<DataBindingInfo>>);

impl JavaObject for DataBindingInfoView {
    fn class_name(&self) -> &str {
        "flex2.compiler.as3.binding.DataBindingInfo"
    }

    fn methods(&self) -> &[Method] {
        INFO_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        let i = self.0.borrow();
        Ok(match &*method.name {
            "getBindingExpressions" => match &i.binding_expressions {
                Some(l) => Value::list(l.iter().map(template_values::binding_expression).collect()),
                None => Value::Null,
            },
            "getClassName" => opt(i.get_class_name()),
            "getImports" => Value::list_of("java.util.HashSet", i.imports.iter().map(|s| Value::str(s)).collect()),
            "getRootWatchers" => Value::map_of("java.util.HashMap", i.root_watchers.iter().map(|(k, v)| (Value::str(k), watcher(v))).collect()),
            "getWatcherSetupUtilClassName" => opt(i.get_watcher_setup_util_class_name()),
            _ => Value::Null,
        })
    }
}

pub fn watcher(w: &WatcherRef) -> Value {
    Value::Object(Rc::new(watcher_view(w.clone())))
}

static WATCHER_METHODS: &[Method] = &[
    Method::new("getBindingExpressions", &[], c("java.util.Set")),
    Method::new("getChangeEvents", &[], c("java.util.Set")),
    Method::new("getClassName", &[], JType::String),
    Method::new("getId", &[], JType::Int),
    Method::new("getParent", &[], c("flex2.compiler.as3.binding.Watcher")),
    Method::new("shouldWriteChildren", &[], JType::Boolean),
    Method::new("isPartOfAnonObjectGraph", &[], JType::Boolean),
    Method::new("setPartOfAnonObjectGraph", &[JType::Boolean], JType::Void),
    Method::new("setShouldWriteChildren", &[JType::Boolean], JType::Void),
    Method::new("shouldWriteSelf", &[], JType::Boolean),
    Method::new("getChild", &[JType::String], c("flex2.compiler.as3.binding.PropertyWatcher")),
    Method::new("getChildren", &[], c("java.util.Collection")),
    Method::new("addChangeEvent", &[JType::String], JType::Void),
    Method::new("addChangeEvent", &[JType::String, JType::Boolean], JType::Void),
    Method::new("addNoChangeEvent", &[], JType::Void),
    Method::new("isOperation", &[], JType::Boolean),
    Method::new("setClassName", &[JType::String], JType::Void),
    Method::new("setOperation", &[JType::Boolean], JType::Void),
];

static PROPERTY_WATCHER_METHODS: &[Method] = &[
    Method::new("getPathToProperty", &[], JType::String),
    Method::new("getProperty", &[], JType::String),
    Method::new("getStaticProperty", &[], JType::Boolean),
    Method::new("setStaticProperty", &[JType::Boolean], JType::Void),
    Method::new("suppress", &[], JType::Void),
];

static REPEATER_COMPONENT_METHODS: &[Method] = &[Method::new("getRepeaterLevel", &[], JType::Int)];

static EVALUATION_METHODS: &[Method] = &[
    Method::new("getEvaluationPart", &[], JType::String),
    Method::new("getBindingExpression", &[], c("flex2.compiler.mxml.rep.BindingExpression")),
    Method::new("getParentWatcher", &[], c("flex2.compiler.as3.binding.Watcher")),
];

static FUNCTION_RETURN_METHODS: &[Method] = &[
    Method::new("getFunctionName", &[], JType::String),
    Method::new("isStyleWatcher", &[], JType::Boolean),
    Method::new("setStyleWatcher", &[JType::Boolean], JType::Void),
];

struct WatcherView {
    w: WatcherRef,
    methods: Vec<Method>,
}

fn watcher_view(w: WatcherRef) -> WatcherView {
    let class = w.borrow().java_class_name();
    let mut methods: Vec<Method> = WATCHER_METHODS.to_vec();
    match class {
        "flex2.compiler.as3.binding.PropertyWatcher" | "flex2.compiler.as3.binding.XMLWatcher" => methods.extend(PROPERTY_WATCHER_METHODS.iter().cloned()),
        "flex2.compiler.as3.binding.RepeaterComponentWatcher" => {
            methods.extend(PROPERTY_WATCHER_METHODS.iter().cloned());
            methods.extend(REPEATER_COMPONENT_METHODS.iter().cloned());
        }
        "flex2.compiler.as3.binding.ArrayElementWatcher" => methods.extend(EVALUATION_METHODS.iter().cloned()),
        "flex2.compiler.as3.binding.FunctionReturnWatcher" => {
            methods.extend(EVALUATION_METHODS.iter().cloned());
            methods.extend(FUNCTION_RETURN_METHODS.iter().cloned());
        }
        _ => {}
    }
    WatcherView { w, methods }
}

impl JavaObject for WatcherView {
    fn class_name(&self) -> &str {
        self.w.borrow().java_class_name()
    }

    fn instance_of(&self, class_name: &str) -> bool {
        let own = self.w.borrow().java_class_name();
        class_name == own
            || class_name == "flex2.compiler.as3.binding.Watcher"
            || (class_name == "flex2.compiler.as3.binding.PropertyWatcher" && self.w.borrow().is_property_watcher())
            || (class_name == "flex2.compiler.as3.binding.EvaluationWatcher" && (self.w.borrow().is_array_element_watcher() || self.w.borrow().is_function_return_watcher()))
    }

    fn methods(&self) -> &[Method] {
        &self.methods
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        let w = self.w.borrow();
        Ok(match &*method.name {
            "getBindingExpressions" => match w.get_binding_expressions() {
                Some(l) => Value::list_of("java.util.TreeSet", l.iter().map(template_values::binding_expression).collect()),
                None => Value::Null,
            },
            "getChangeEvents" => Value::list_of("java.util.HashSet", w.get_change_events().iter().map(change_event).collect()),
            "getClassName" => opt(w.get_class_name()),
            "getId" => Value::Integer(w.get_id()),
            "getParent" => w.get_parent().as_ref().map_or(Value::Null, watcher),
            "shouldWriteChildren" => Value::Boolean(w.should_write_children()),
            "isPartOfAnonObjectGraph" => Value::Boolean(w.is_part_of_anon_object_graph()),
            "shouldWriteSelf" => Value::Boolean(w.should_write_self()),
            "getChild" => Value::Null,
            "getChildren" => Value::list_of("java.util.HashMap$Values", w.get_children().iter().map(watcher).collect()),
            "isOperation" => Value::Boolean(w.is_operation()),
            "getPathToProperty" => Value::str(&w.get_path_to_property()),
            "getProperty" => opt(w.get_property()),
            "getStaticProperty" => Value::Boolean(w.get_static_property()),
            "getRepeaterLevel" => Value::Integer(w.get_repeater_level()),
            "getEvaluationPart" => Value::str(&w.get_evaluation_part()),
            "getBindingExpression" => w.get_binding_expression().as_ref().map_or(Value::Null, template_values::binding_expression),
            "getFunctionName" => opt(w.get_function_name()),
            "isStyleWatcher" => Value::Boolean(w.is_style_watcher()),
            _ => Value::Null,
        })
    }

    fn java_equals(&self, other: &Value) -> bool {
        match other {
            Value::Object(o) => o.to_java_string() == self.to_java_string(),
            _ => false,
        }
    }

    fn to_java_string(&self) -> String {
        format!("{}@{:x}", self.class_name(), Rc::as_ptr(&self.w) as *const u8 as usize)
    }
}

fn change_event(e: &ChangeEvent) -> Value {
    Value::Object(Rc::new(ChangeEventView(e.clone())))
}

static CHANGE_EVENT_METHODS: &[Method] = &[
    Method::new("equals", &[JType::Object], JType::Boolean),
    Method::new("getCommitting", &[], JType::Boolean),
    Method::new("getName", &[], JType::String),
    Method::new("hashCode", &[], JType::Int),
];

struct ChangeEventView(ChangeEvent);

impl JavaObject for ChangeEventView {
    fn class_name(&self) -> &str {
        "flex2.compiler.as3.binding.ChangeEvent"
    }

    fn methods(&self) -> &[Method] {
        CHANGE_EVENT_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getCommitting" => Value::Boolean(self.0.get_committing()),
            "getName" => Value::str(self.0.get_name()),
            _ => Value::Null,
        })
    }
}
