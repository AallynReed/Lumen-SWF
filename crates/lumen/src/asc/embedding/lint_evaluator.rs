//! Port of `macromedia/asc/embedding/LintEvaluator.java`: the `-warnings` (coach) pass, which warns about valid
//! code that probably does not do what its author intended.

#![allow(non_upper_case_globals)]

use std::any::Any;
use std::cell::{Ref, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::asc::emit::runtime_constants::{TYPE_bool, TYPE_boolean, TYPE_decimal, TYPE_double, TYPE_int, TYPE_uint};
use crate::asc::embedding::warning_constants::*;
use crate::asc::parser::evaluator::*;
use crate::asc::parser::input::InputBuffer;
use crate::asc::parser::node::*;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::emitter::{Emitter, EmitterBase};
use crate::asc::semantics::slot::PARAM_Rest;
use crate::asc::semantics::value::*;
use crate::asc::util::context::{Context, ContextRef, USE_SANITY_STYLE_ERRORS, USE_SIMPLE_LOG_WARNINGS};
use crate::asc::util::jhash::java_compare;
use crate::asc::util::jstr;
use crate::asc::util::number_usage::NumberUsage;

pub static debug: AtomicBool = AtomicBool::new(false);

const newline: &str = if cfg!(windows) { "\r\n" } else { "\n" };

type Input = Option<Rc<RefCell<InputBuffer>>>;

struct CodeLocation {
    pos: i32,
    input: Input,
}

struct WarningRecord {
    loc: CodeLocation,
    line_num: i32,
    col_num: i32,
    code: i32,
    err_string_arg1: Option<String>,
    err_string_arg2: Option<String>,
    err_string_arg3: Option<String>,
}

/// A `HashMap<TypeValue, Integer>`; keys are compared by identity and iterated in insertion order.
#[derive(Clone, Default)]
struct TypeCodeMap(Vec<(Option<TypeValue>, i32)>);

impl TypeCodeMap {
    fn put(&mut self, key: Option<TypeValue>, value: i32) {
        for e in &mut self.0 {
            if opt_eq(&e.0, &key) {
                e.1 = value;
                return;
            }
        }
        self.0.push((key, value));
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct DefaultTypeDesc {
    code: i32,
    name_space: &'static str,
    name: &'static str,
}

const fn dtd(code: i32, name_space: &'static str, name: &'static str) -> DefaultTypeDesc {
    DefaultTypeDesc { code, name_space, name }
}

static typeDescriptors: [DefaultTypeDesc; kNumDefaultTypes as usize] = [
    dtd(kVoidType, "", "null"),
    dtd(kObjectType, "", "Object"),
    dtd(kFunctionType, "", "Function"),
    dtd(kStringType, "", "String"),
    dtd(kNumberType, "", "Number"),
    dtd(kBooleanType, "", "Boolean"),
    dtd(kArrayType, "", "Array"),
    dtd(kDateType, "", "Date"),
    dtd(kMathType, "", "Math"),
    dtd(kErrorType, "", "Error"),
    dtd(kRegExpType, "", "RegExp"),
    dtd(kDisplayObjectType, "flash.display", "DisplayObject"),
    dtd(kMovieClipType, "flash.display", "MovieClip"),
    dtd(kTextFieldType, "flash.text", "TextField"),
    dtd(kTextFormatType, "flash.text", "TextFormat"),
    dtd(kMicrophoneType, "flash.media", "Microphone"),
    dtd(kSimpleButtonType, "flash.display", "SimpleButton"),
    dtd(kVideoType, "flash.media", "Video"),
    dtd(kStyleSheetType, "flash.text", "StyleSheet"),
    dtd(kSelectionType, "flash.obsolete", "SelectionType"),
    dtd(kColorType, "flash.obsolete", "Color"),
    dtd(kStageType, "flash.display", "Stage"),
    dtd(kMouseType, "flash.ui", "Mouse"),
    dtd(kKeyboardType, "flash.ui", "Keyboard"),
    dtd(kSoundType, "flash.media", "Sound"),
    dtd(kSystemType, "flash.system", "System"),
    dtd(kXMLType, "", "XML"),
    dtd(kXMLSocketType, "flash.net", "XMLSocket"),
    dtd(kXMLListType, "", "XMLList"),
    dtd(kQNameType, "", "QName"),
    dtd(kLoadVarsType, "flash.net", "LoadVars"),
    dtd(kCameraType, "flash.media", "Camera"),
    dtd(kContextMenuType, "flash.ui", "ContextMenu"),
    dtd(kContextMenuItemType, "flash.ui", "ContextMenuItem"),
    dtd(kMovieClipLoaderType, "flash.obsolete", "MovieClipLoader"),
    dtd(kNetStreamType, "flash.net", "NetStream"),
    dtd(kAccessibilityType, "flash.accessibility", "Accessibility"),
    dtd(kActivityEventType, "flash.events", "ActivityEvent"),
    dtd(kByteArrayType, "flash.util", "ByteArray"),
    dtd(kColorTransformType, "flash.geom", "ColorTransform"),
    dtd(kDisplayObjectContainerType, "flash.display", "DisplayObjectContainer"),
    dtd(kCustomActionsType, "macromedia.util", "CustomActions"),
    dtd(kDataEventType, "flash.events", "DataEvent"),
    dtd(kExternalInterfaceType, "flash.external", "ExternalInterface"),
    dtd(kErrorEventType, "flash.events", "ErrorEvent"),
    dtd(kEventType, "flash.events", "Event"),
    dtd(kFocusEventType, "flash.events", "FocusEvent"),
    dtd(kGraphicsType, "flash.display", "Graphics"),
    dtd(kBitmapFilterType, "flash.filters", "BitmapFilter"),
    dtd(kInteractiveObjectType, "flash.display", "InteractiveObject"),
    dtd(kKeyboardEventType, "flash.events", "KeyboardEvent"),
    dtd(kLoaderType, "flash.display", "Loader"),
    dtd(kLoaderInfoType, "flash.display", "LoaderInfo"),
    dtd(kLocalConnectionType, "flash.net", "LocalConnection"),
    dtd(kContextMenuEventType, "flash.events", "ContextMenuEvent"),
    dtd(kProductManagerType, "macromedia.util", "ProductManager"),
    dtd(kPointType, "flash.geom", "Point"),
    dtd(kProxyType, "flash.util", "Proxy"),
    dtd(kProfilerType, "flash.profiler", ""),
    dtd(kProgressEventType, "flash.events", "ProgressEvent"),
    dtd(kRectangleType, "flash.geom", "Rectangle"),
    dtd(kSoundTransformType, "flash.media", "SoundTransform"),
    dtd(kSocketType, "flash.net", "Socket"),
    dtd(kSharedObjectType, "flash.net", "SharedObject"),
    dtd(kSpriteType, "flash.display", "Sprite"),
    dtd(kIMEType, "flash.system", "IME"),
    dtd(kSWFLoaderInfoType, "flash.display", "SWFLoaderInfo"),
    dtd(kTextSnapshotType, "flash.text", "TextSnapshot"),
    dtd(kURLLoaderType, "flash.net", "URLLoader"),
    dtd(kURLStreamType, "flash.net", "URLStream"),
    dtd(kURLRequestType, "flash.net", "URLRequest"),
    dtd(kXMLDocumentType, "flash.xml", "XMLDocument"),
    dtd(kXMLNodeType, "flash.xml", "XMLNode"),
    dtd(kNetConnectionType, "flash.net", "NetConnection"),
    dtd(kSyncEventType, "flash.events", "SyncEvent"),
    dtd(kBitmapDataType, "flash.display", "BitmapData"),
    dtd(kXMLUIType, "macromedia.util", "XMLUI"),
    dtd(kFileReferenceListType, "flash.net", "FileReferenceList"),
    dtd(kFileReferenceType, "flash.net", "FileReference"),
];

pub static simpleTypeNames: [&str; 73] = [
    "null", "Object", "Function", "String", "Number", "Boolean", "Array", "Date", "Math", "Error", "RegExp",
    "DisplayObject", "MovieClip", "TextField", "TextFormat", "Microphone", "SimpleButton", "Video", "StyleSheet",
    "Selection", "Color", "Stage", "Mouse", "Keyboard", "Sound", "System", "XML", "XMLSocket", "XMLList", "QName",
    "LoadVars", "Camera", "ContextMenu", "ContextMenuItem", "MovieClipLoader", "NetStream", "Accessibility",
    "ActivityEvent", "ByteArray", "ColorTransform", "DisplayObjectContainer", "CustomActions", "DataEvent",
    "ExternalInterface", "ErrorEvent", "Event", "FocusEvent", "Graphics", "BitmapFilter", "InteractiveObject",
    "KeyboardEvent", "Loader", "LoaderInfo", "LocalConnection", "ContextMenuEvent", "ProductManager", "Point", "Proxy",
    "Profiler", "ProgressEvent", "Rectangle", "SoundTransform", "Socket", "SharedObject", "Sprite", "IME",
    "SWFLoaderInfo", "TextSnapshot", "URLLoader", "URLStream", "URLRequest", "XMLDocument", "XMLNode",
];

static toIgnore: [&str; 5] = ["_global", "rest", "NaN", "arguments", "undefined"];

/// `hackIgnoreIdentifierMap.get(name)`.
fn hack_ignore_identifier(name: &str) -> Option<bool> {
    if toIgnore.contains(&name) {
        Some(true)
    } else {
        None
    }
}

struct LintDataRecord {
    has_return_value: bool,
    is_registered_for_event: bool,
    has_been_declared: bool,
    declaration_pos: i32,
}

impl LintDataRecord {
    fn new() -> LintDataRecord {
        LintDataRecord { has_return_value: false, has_been_declared: false, is_registered_for_event: false, declaration_pos: 0 }
    }
}

fn rb(n: &Option<NodeRef>) -> Ref<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow()
}

fn kind_is(n: &Option<NodeRef>, f: fn(&Node) -> bool) -> bool {
    n.as_ref().map_or(false, |n| f(&n.borrow()))
}

fn val(t: TypeValue) -> Option<Value> {
    Some(Value::from(t))
}

fn opt_val(t: Option<TypeValue>) -> Option<Value> {
    t.map(Value::from)
}

/// Java `v == t`.
fn is(v: &Option<Value>, t: &TypeValue) -> bool {
    matches!(v, Some(Value::Object(o)) if o.ptr_eq(t.as_object()))
}

fn is_obj(v: &Option<Value>, o: &Option<ObjectValue>) -> bool {
    match (v, o) {
        (Some(Value::Object(a)), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn same(a: &Option<Value>, b: &Option<Value>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn same_tv(v: &Option<Value>, t: &Option<TypeValue>) -> bool {
    match t {
        Some(t) => is(v, t),
        None => v.is_none(),
    }
}

fn opt_is(t: &Option<TypeValue>, u: &TypeValue) -> bool {
    t.as_ref().map_or(false, |t| t.ptr_eq(u))
}

fn opt_eq(a: &Option<TypeValue>, b: &Option<TypeValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

/// `v instanceof TypeValue ? (TypeValue)v : null`.
fn tv_of(v: &Option<Value>) -> Option<TypeValue> {
    v.as_ref().and_then(|v| v.as_type_value())
}

/// `(TypeValue)v`.
fn cast_tv(v: &Option<Value>) -> Option<TypeValue> {
    match v {
        None => None,
        Some(v) => Some(v.as_type_value().expect("ClassCastException")),
    }
}

/// `s.getType().getTypeValue()`.
fn slot_type_value(s: &Slot) -> Option<TypeValue> {
    s.get_type().expect("NullPointerException").get_type_value()
}

fn tv_name(t: &Option<TypeValue>) -> String {
    t.as_ref().expect("NullPointerException").name().to_string()
}

fn is_class_type(t: &Option<TypeValue>) -> bool {
    t.as_ref().map_or(false, |t| t.name().to_string() == "Class")
}

/// `"".equals(s.trim())`.
fn trims_to_empty(s: &str) -> bool {
    s.chars().all(|c| (c as u32) <= 0x20)
}

fn attrs_name_access(attrs: &Option<NodeRef>) -> bool {
    match attrs {
        Some(a) => {
            let a = a.borrow();
            let a = a.as_attribute_list().expect("ClassCastException");
            a.has_user_namespace() || a.has_private || a.has_public || a.has_protected || a.has_internal
        }
        None => false,
    }
}

fn is_activation_scope(o: &ObjectValue) -> bool {
    o.builder().map_or(false, |b| b.is_activation_builder())
}

fn node_list(l: &Option<NodeList>) -> Vec<NodeRef> {
    l.as_ref().map_or_else(Vec::new, |l| l.borrow().clone())
}

/// The name of the reference of a `MemberExpressionNode`.
fn member_ref_name(n: &Option<NodeRef>) -> String {
    rb(n).as_member_expression().unwrap().ref_.as_ref().expect("NullPointerException").name()
}

/// `java.lang.String.equals`, where the receiver must not be null.
fn jequals(a: &Option<String>, b: &Option<String>) -> bool {
    let a = a.as_ref().expect("NullPointerException");
    b.as_ref().map_or(false, |b| a == b)
}

pub struct LintEvaluator {
    base: EmitterBase,

    types: Vec<Option<TypeValue>>,

    warning_constants_map: HashMap<i32, &'static str>,
    unsupported_props_map: HashMap<String, TypeCodeMap>,
    unsupported_methods_map: HashMap<String, TypeCodeMap>,
    unsupported_events_map: HashMap<String, TypeCodeMap>,

    enabled_map: Option<HashMap<i32, bool>>,

    warning_enabled_map_file: Option<String>,

    p_warnings: BTreeMap<i32, Vec<Rc<WarningRecord>>>,
    /// Kept sorted by `String.compareTo`; each line's records are in insertion order (Java: identity hash order).
    warnings_by_loc: Vec<(String, BTreeMap<i32, Vec<Rc<WarningRecord>>>)>,

    initialized: bool,
    script_name: String,
    undefined_literal: Option<ObjectValue>,
    body_has_return: bool,
    body_has_super: bool,

    output_to_file: bool,

    doing_method: bool,
    doing_class: bool,

    first_pass: bool,

    expected_return_type: Option<TypeValue>,

    pub base_type_context: Vec<Option<TypeValue>>,
    pub base_ref_context: Vec<Option<ReferenceValue>>,

    this_contexts: Vec<i32>,
    pub current_function_ref: Vec<Option<ReferenceValue>>,

    in_with: bool,

    open_namespaces: Vec<Option<Namespaces>>,

    slots_to_clean: Vec<Slot>,
}

impl Emitter for LintEvaluator {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }
}

impl LintEvaluator {
    fn eval(&mut self, cx: &ContextRef, n: &Option<NodeRef>) -> Option<Value> {
        evaluate(n.as_ref().expect("NullPointerException"), cx, self)
    }

    fn input(cx: &ContextRef) -> Input {
        cx.borrow().input.clone()
    }

    fn check_deprecated_slot(&mut self, cx: &ContextRef, node: Option<NodeRef>, ref_: &ReferenceValue, s: &Option<Slot>) {
        let md = match s {
            Some(s) => s.get_metadata(),
            None => None,
        };
        if let Some(md) = md {
            for md_node in &md {
                if md_node.id().as_deref() == Some("Deprecated") {
                    let mut since: Option<String> = None;
                    let mut message: Option<String> = None;
                    let mut replacement: Option<String> = None;

                    let values = md_node.values();
                    for r in 0..md_node.count() as usize {
                        let v = &values.as_ref().expect("NullPointerException")[r];
                        if let Some(temp) = v.as_key_value_pair() {
                            if temp.key == "message" {
                                message = Some(temp.obj.clone());
                            } else if temp.key == "replacement" {
                                replacement = Some(temp.obj.clone());
                            } else if temp.key == "since" {
                                since = Some(temp.obj.clone());
                            }
                        } else if let Some(temp) = v.as_keyless_value() {
                            message = Some(temp.obj.clone());
                        }
                    }

                    self.log_deprecation_warning(&node, cx, &ref_.name(), since, message, replacement);
                }
            }
        }
    }

    fn log_deprecation_warning(
        &mut self,
        node: &Option<NodeRef>,
        cx: &ContextRef,
        name: &str,
        since: Option<String>,
        message: Option<String>,
        replacement: Option<String>,
    ) {
        let pos = rb(node).get_position();
        let input = Self::input(cx);

        let has_since = since.as_ref().map_or(false, |s| !s.is_empty());
        let has_message = message.as_ref().map_or(false, |s| !s.is_empty());
        let has_replacement = replacement.as_ref().map_or(false, |s| !s.is_empty());

        if has_message {
            self.warning1(pos, &input, kWarning_DeprecatedMessage, message.as_deref().unwrap());
        } else if has_replacement {
            if has_since {
                self.warning3(pos, &input, kWarning_DeprecatedSince, name, since.as_deref().unwrap(), replacement.as_deref().unwrap());
            } else {
                self.warning2(pos, &input, kWarning_DeprecatedUseReplacement, name, replacement.as_deref().unwrap());
            }
        } else if has_since {
            self.warning2(pos, &input, kWarning_DeprecatedSinceNoReplacement, name, since.as_deref().unwrap());
        } else {
            self.warning1(pos, &input, kWarning_Deprecated, name);
        }
    }

    /// The type a static member access really searches: the class itself when the base is a `Class` reference.
    fn class_search_type(&self, cx: &ContextRef, mut search_type: Option<TypeValue>) -> (Option<TypeValue>, Option<Slot>) {
        let br = self.base_ref_context.last().cloned().expect("IndexOutOfBoundsException");
        let s = br.and_then(|br| br.get_slot_kind(&cx.borrow(), GET_TOKEN));
        let t = s.as_ref().and_then(|s| s.get_object_value()).and_then(|o| o.as_type_value());
        if t.is_some() {
            search_type = t;
        }
        (search_type, s)
    }

    fn is_es3_dynamic_type(&self, cx: &ContextRef, t: &Option<TypeValue>, with_xml: bool) -> bool {
        let c = cx.borrow();
        opt_eq(t, &self.types[kDateType as usize])
            || opt_is(t, &c.reg_exp_type())
            || (with_xml && (opt_is(t, &c.xml_type()) || opt_is(t, &c.xml_list_type())))
            || self.types[kErrorType as usize].as_ref().map_or(false, |e| e.includes(&c, t.as_ref()))
    }

    fn evaluate_generic_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr, args, is_new, pos, gpos) = {
            let b = node.borrow();
            let c = b.as_call_expression().unwrap();
            (c.sel.ref_.clone(), c.sel.expr.clone(), c.args.clone(), c.is_new, b.pos(), b.get_position())
        };
        if self.first_pass {
            if args.is_some() {
                let base_type = self.base_type_context.pop().expect("IndexOutOfBoundsException");
                self.base_ref_context.push(None);
                self.eval(cx, &args);
                self.base_type_context.push(base_type);
                self.base_ref_context.pop();
            }
            self.eval(cx, &expr);
            return val(cx.borrow().no_type());
        }

        let mut s = r.as_ref().and_then(|r| r.get_slot_kind(&cx.borrow(), if is_new { NEW_TOKEN } else { EMPTY_TOKEN }));
        if let (Some(r), None) = (&r, &s) {
            let mut search_type = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");

            if is_class_type(&search_type) {
                let (t, slot) = self.class_search_type(cx, search_type);
                search_type = t;
                s = slot;
            }

            if self.is_es3_dynamic_type(cx, &search_type, true) {
                let input = Self::input(cx);
                let name = search_type.as_ref().expect("NullPointerException").name().name().to_string();
                self.warning2(pos, &input, kWarning_BadES3TypeMethod, &r.name(), &name);
            } else {
                let search = self.unsupported_methods_map.get(&r.name()).cloned();
                if let Some(search) = search.filter(|s| !s.is_empty()) {
                    for (type_, code) in &search.0 {
                        if let Some(type_) = type_ {
                            if type_.includes(&cx.borrow(), search_type.as_ref()) {
                                let input = Self::input(cx);
                                let msg = self.warning_constants_map.get(code).copied();
                                self.warning_opt(gpos, &input, kWarning_DepricatedFunctionError, Some(&r.name()), msg, Some(""));
                            }
                        }
                    }
                }
            }
        }

        if let (Some(r), true) = (&r, is_new) {
            let call_slot = r.get_slot_kind(&cx.borrow(), EMPTY_TOKEN);
            if let Some(call_slot) = &call_slot {
                if !opt_is(&slot_type_value(call_slot), &cx.borrow().type_type()) {
                    let get_slot = r.get_slot_kind(&cx.borrow(), GET_TOKEN);
                    let base = r.get_base();
                    if base.is_none() && get_slot.is_some() && self.slot_get_has_return_value(&s) {
                        let input = Self::input(cx);
                        self.warning2(gpos, &input, kWarning_ConstructorReturnsValue, &r.name(), &r.name());
                    }
                }
            }
        }

        if s.is_some() {
            self.check_deprecated_slot(cx, Some(node.clone()), r.as_ref().expect("NullPointerException"), &s);
        }

        if args.is_some() {
            let base_type = self.base_type_context.pop().expect("IndexOutOfBoundsException");
            self.base_ref_context.push(None);
            self.eval(cx, &args);
            self.base_ref_context.pop();
            self.base_type_context.push(base_type);
        }

        self.eval(cx, &expr);

        let r = match r {
            Some(r) => r,
            None => return val(cx.borrow().no_type()),
        };
        let name = r.name();
        let input = Self::input(cx);

        if is_new {
            let (nargs, first_arg) = match &args {
                Some(a) => {
                    let a = a.borrow();
                    let l = a.as_argument_list().unwrap();
                    (l.items.len(), l.items.first().cloned().flatten())
                }
                None => (0, None),
            };
            if name == "Boolean" && !(args.is_some() && nargs != 0) {
                self.warning(gpos, &input, kWarning_BooleanConstructorWithNoArgs);
            } else if name == "Number" && args.is_some() && nargs == 1 {
                let arg_type_val = self.eval(cx, &first_arg);
                if is(&arg_type_val, &cx.borrow().string_type())
                    && rb(&first_arg).as_literal_string().map_or(false, |l| trims_to_empty(&l.value))
                {
                    self.warning(gpos, &input, kWarning_NumberFromStringChanges);
                }
            } else if name == "XML" {
                self.warning(gpos, &input, kWarning_XML_ClassHasChanged);
            }
        } else if name == "Array" {
            self.warning(gpos, &input, kWarning_BadArrayCast);
        } else if name == "Date" {
            self.warning(gpos, &input, kWarning_BadDateCast);
        } else if name == "XML" {
            self.warning(gpos, &input, kWarning_XML_ClassHasChanged);
        }

        match s.as_ref().and_then(|s| s.get_type()) {
            Some(t) => opt_val(t.get_type_value()),
            None => val(cx.borrow().no_type()),
        }
    }

    fn get_slot(&self, proto: &ObjectValue, cx: &ContextRef, node: &NodeRef, type_: i32) -> Option<Slot> {
        let r = node.borrow().selector().unwrap().ref_.clone().expect("NullPointerException");
        let c = cx.borrow();
        let mut slot = None;
        let ns = proto.has_names(&c, type_, &r.name(), &r.get_immutable_namespaces());
        if let Some(ns) = ns {
            let last = ns.first();
            let slot_index = proto.get_slot_index(&c, type_, &r.name(), Some(&last));
            slot = proto.get_slot(&c, slot_index);
        }
        slot
    }

    fn check_return_statement_type(&mut self, cx: &ContextRef, item: &Option<NodeRef>, return_type: &Option<Value>) {
        let input = Self::input(cx);
        let (function_type, uint_type, boolean_type, no_type, object_type) = {
            let c = cx.borrow();
            (c.function_type(), c.uint_type(), c.boolean_type(), c.no_type(), c.object_type())
        };
        if is(return_type, &function_type) {
            if kind_is(item, Node::is_member_expression) {
                let func_name = member_ref_name(item);
                let pos = rb(item).pos();
                let expected = tv_name(&self.expected_return_type);
                self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &expected, &func_name);
            }
        } else if opt_is(&self.expected_return_type, &uint_type) {
            if let Some(ln) = item.as_ref().filter(|i| i.borrow().as_literal_number().is_some()) {
                let d = ln.borrow().as_literal_number().unwrap().numeric_value.as_ref().expect("NullPointerException").double_value();
                if d < 0.0 {
                    let pos = ln.borrow().pos();
                    self.warning(pos, &input, kWarning_NegativeUintLiteral);
                }
            }
        } else if opt_is(&self.expected_return_type, &boolean_type)
            && tv_of(return_type).is_some()
            && !is(return_type, &no_type)
            && !is(return_type, &object_type)
        {
            let rt = tv_of(return_type);
            let pos = rb(item).get_position();
            self.warning1(pos, &input, kWarning_BadBoolAssignment, &tv_name(&rt));
        } else if self.expected_return_type.as_ref().map_or(false, |e| e.is_numeric(&cx.borrow()) || e.ptr_eq(&boolean_type)) {
            if kind_is(item, |n| n.as_literal_null().is_some()) {
                let pos = rb(item).get_position();
                let expected = tv_name(&self.expected_return_type);
                self.warning1(pos, &input, kWarning_BadNullAssignment, &expected);
            }
        }
    }

    // Supporting tables and maps for identifying unsupported AS2 Flash player apis

    pub fn new_with_file(cx: &ContextRef, name: &str, enabled_file: Option<&str>) -> LintEvaluator {
        let mut l = Self::new_fields(cx, name);
        l.warning_enabled_map_file = enabled_file.map(str::to_string);
        l.output_to_file = true;
        l.in_with = false;
        l
    }

    /// The constructor that takes a pre-filled warnings map, as from `getWarningDefaults()`.
    pub fn new(cx: &ContextRef, name: &str, enabled_warnings_map: HashMap<i32, bool>) -> LintEvaluator {
        let mut l = Self::new_fields(cx, name);
        l.enabled_map = Some(enabled_warnings_map);
        l.output_to_file = true;
        l
    }

    fn new_fields(cx: &ContextRef, name: &str) -> LintEvaluator {
        LintEvaluator {
            base: EmitterBase::new(None),
            types: vec![None; kNumDefaultTypes as usize],
            warning_constants_map: HashMap::new(),
            unsupported_props_map: HashMap::new(),
            unsupported_methods_map: HashMap::new(),
            unsupported_events_map: HashMap::new(),
            enabled_map: None,
            warning_enabled_map_file: None,
            p_warnings: BTreeMap::new(),
            warnings_by_loc: Vec::new(),
            initialized: false,
            script_name: name.to_string(),
            undefined_literal: None,
            body_has_return: false,
            body_has_super: false,
            output_to_file: false,
            doing_method: false,
            doing_class: false,
            first_pass: false,
            expected_return_type: Some(cx.borrow().no_type()),
            base_type_context: Vec::new(),
            base_ref_context: Vec::new(),
            this_contexts: Vec::new(),
            current_function_ref: Vec::new(),
            in_with: false,
            open_namespaces: Vec::new(),
            slots_to_clean: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.initialized = false;
        self.unsupported_props_map.clear();
        self.unsupported_methods_map.clear();
        self.unsupported_events_map.clear();
        self.p_warnings.clear();
        self.warnings_by_loc.clear();
        self.undefined_literal = None;
        for s in &self.slots_to_clean {
            clear_lint_data(s);
        }
    }

    /// Logs the warnings with little formatting to the handler; this is what mxmlc calls.
    pub fn simple_log_warnings(&mut self, cx: &ContextRef, log_as_errors: bool) -> i32 {
        let handler = {
            let c = cx.borrow();
            let h = c.get_handler();
            match h {
                Some(h) => Some(h),
                None => c.statics.borrow().handler.clone(),
            }
        };

        let mut count = 0;
        for (_, loc_map) in &self.warnings_by_loc {
            for records in loc_map.values() {
                for record in records {
                    let mut sb = String::new();
                    let input = record.loc.input.as_ref().expect("NullPointerException");

                    self.create_error_message(record, &mut sb, record.code);
                    let source = input.borrow_mut().get_line_text(record.loc.pos);
                    let origin = input.borrow().origin.clone().unwrap_or_else(|| "null".to_string());

                    let handler = handler.as_ref().expect("NullPointerException");
                    if log_as_errors {
                        handler.borrow_mut().error(&origin, record.line_num, record.col_num, &sb, &source, record.code);
                    } else {
                        handler.borrow_mut().warning(&origin, record.line_num, record.col_num, &sb, &source, record.code);
                    }
                    count += 1;
                }
            }
        }
        count
    }

    /// Logs formatted warnings to `<scriptname>_warnings.txt` and the warning stream.
    pub fn log_warnings(&mut self, cx: &ContextRef) {
        if USE_SANITY_STYLE_ERRORS.load(Ordering::Relaxed) || USE_SIMPLE_LOG_WARNINGS.load(Ordering::Relaxed) {
            self.simple_log_warnings(cx, false);
        } else {
            let mut out = String::new();
            out.push_str(newline);
            out.push_str("Warning Report:");
            out.push_str(newline);
            out.push_str("---------------");
            out.push_str(newline);
            out.push_str(newline);

            for (code, warnings) in &self.p_warnings {
                let msg = self.warning_constants_map.get(code).copied().unwrap_or("null");
                out.push_str(&format!("[Coach] Warning #{}: {}{}", code, msg, newline));
                out.push_str("-------------------------------------------------------------------------");
                out.push_str(newline);
                for p_rec in warnings {
                    self.create_warning(p_rec, &mut out, *code);
                    out.push_str(newline);
                }
                out.push_str("-------------------------------------------------------------------------");
                out.push_str(newline);
                out.push_str(newline);
            }

            if !self.p_warnings.is_empty() {
                let handler = cx.borrow().get_handler();
                match handler {
                    Some(h) => h.borrow_mut().warning_no_code("", -1, -1, &out, ""),
                    None => eprintln!("{}", jstr::to_display(&out)),
                }

                if self.output_to_file {
                    let script_name = &self.script_name;
                    let mut dot_pos = jstr::index_of_unit(script_name, b'.' as u16, 0);
                    if dot_pos == -1 {
                        dot_pos = jstr::utf16_len(script_name);
                    }
                    let out_name = jstr::substring(script_name, 0, dot_pos) + "_warnings.txt";
                    if let Err(ex) = std::fs::write(&out_name, jstr::to_java_utf8_bytes(&out)) {
                        eprintln!("{}", ex);
                    }
                }
            }
        }
    }

    fn create_warning(&self, p_rec: &WarningRecord, out: &mut String, code: i32) {
        let input = p_rec.loc.input.as_ref().expect("NullPointerException");
        let origin = input.borrow().origin.clone().unwrap_or_else(|| "null".to_string());

        out.push_str(&format!("  {}({}): ", origin, p_rec.line_num));

        self.create_error_message(p_rec, out, code);

        out.push_str(newline);
        let line = input.borrow_mut().get_line_text(p_rec.loc.pos);
        out.push_str("    ");
        out.push_str(&line);
        out.push_str(newline);
        out.push_str("    ");
        out.push_str(&InputBuffer::get_line_pointer(p_rec.col_num));
        out.push_str(newline);
    }

    fn create_error_message(&self, p_rec: &WarningRecord, out: &mut String, code: i32) {
        let arg = |a: &Option<String>| a.clone().unwrap_or_else(|| "null".to_string());
        if USE_SANITY_STYLE_ERRORS.load(Ordering::Relaxed) {
            out.push_str(&format!(
                "code={}; arg1={}; arg2={}; arg3={}",
                code,
                arg(&p_rec.err_string_arg1),
                arg(&p_rec.err_string_arg2),
                arg(&p_rec.err_string_arg3)
            ));
        } else {
            let template_str = self.warning_constants_map.get(&(code + 1)).copied().expect("NullPointerException");

            let mut next_loc = Context::replace_string_arg(out, template_str, 0, &arg(&p_rec.err_string_arg1));
            next_loc = Context::replace_string_arg(out, template_str, next_loc, &arg(&p_rec.err_string_arg2));
            next_loc = Context::replace_string_arg(out, template_str, next_loc, &arg(&p_rec.err_string_arg3));

            if next_loc != -1 {
                out.push_str(&jstr::substring(template_str, next_loc, jstr::utf16_len(template_str)));
            }
        }
    }

    fn warning(&mut self, pos: i32, input: &Input, code: i32) {
        self.warning3(pos, input, code, "", "", "");
    }

    fn warning1(&mut self, pos: i32, input: &Input, code: i32, error_arg1: &str) {
        self.warning3(pos, input, code, error_arg1, "", "");
    }

    fn warning2(&mut self, pos: i32, input: &Input, code: i32, error_arg1: &str, error_arg2: &str) {
        self.warning3(pos, input, code, error_arg1, error_arg2, "");
    }

    fn warning3(&mut self, pos: i32, input: &Input, code: i32, error_arg1: &str, error_arg2: &str, error_arg3: &str) {
        self.warning_opt(pos, input, code, Some(error_arg1), Some(error_arg2), Some(error_arg3));
    }

    /// `warning(...)` with arguments that may be null.
    fn warning_opt(&mut self, pos: i32, input: &Input, code: i32, a1: Option<&str>, a2: Option<&str>, a3: Option<&str>) {
        let disabled = |this: &Self| {
            let m = this.enabled_map.as_ref().expect("NullPointerException");
            m.contains_key(&code) && !m[&code]
        };
        if self.first_pass || disabled(self) || self.ignorable_file(input) || pos == 0 {
            return;
        }

        let loc = CodeLocation { pos, input: input.clone() };

        if let Some(inp) = input {
            let mut col_pos = inp.borrow_mut().get_col_pos(loc.pos);
            if col_pos > 300 {
                col_pos = 1;
            }
            let line_num = inp.borrow_mut().get_ln_num(loc.pos);

            let rec = Rc::new(WarningRecord {
                loc,
                line_num,
                col_num: col_pos,
                code,
                err_string_arg1: a1.map(str::to_string),
                err_string_arg2: a2.map(str::to_string),
                err_string_arg3: a3.map(str::to_string),
            });

            let warn_list = self.p_warnings.entry(code).or_default();

            for test in warn_list.iter() {
                if test.col_num == rec.col_num
                    && test.line_num == rec.line_num
                    && jequals(&test.err_string_arg1, &rec.err_string_arg1)
                    && jequals(&test.err_string_arg2, &rec.err_string_arg2)
                    && jequals(&test.err_string_arg3, &rec.err_string_arg3)
                    && opt_input_eq(&test.loc.input, &rec.loc.input)
                {
                    return;
                }
            }

            warn_list.push(rec.clone());

            let origin = inp.borrow().origin.clone().expect("NullPointerException");
            let i = match self.warnings_by_loc.binary_search_by(|(k, _)| java_compare(k, &origin)) {
                Ok(i) => i,
                Err(i) => {
                    self.warnings_by_loc.insert(i, (origin, BTreeMap::new()));
                    i
                }
            };
            self.warnings_by_loc[i].1.entry(rec.line_num).or_default().push(rec);
        }
    }

    fn ignorable_file(&self, input: &Input) -> bool {
        let input = input.as_ref().expect("NullPointerException").borrow();
        let origin = input.origin.as_ref().expect("NullPointerException");
        let sep = std::path::MAIN_SEPARATOR_STR;
        origin.ends_with(&format!("{}playerglobal.as", sep)) || origin.ends_with(&format!("{}Global.as", sep))
    }

    fn get_simple_type_name(&self, type_: &Option<TypeValue>) -> String {
        type_.as_ref().expect("NullPointerException").name().name().to_string()
    }

    fn initialize(&mut self, cx: &ContextRef) {
        if self.initialized {
            return;
        }

        self.initialized = true;

        let c = cx.borrow();
        self.undefined_literal = Some(ObjectValue::with_value("", Some(&c.void_type())));

        for d in &typeDescriptors {
            let ns = c.get_namespace(d.name_space);
            let type_ref = ReferenceValue::new(&c, None, d.name, Some(&ns));
            let s = type_ref.get_slot_kind(&c, GET_TOKEN);
            let t = s.and_then(|s| s.get_object_value()).and_then(|o| o.as_type_value());
            self.types[d.code as usize] = t;
        }

        self.types[kVoidType as usize] = Some(c.null_type());

        // Only the English table is ported; `cx.statics.languageID` picks among the localised ones in Java.
        for w in &warningConstantsEN {
            self.warning_constants_map.insert(w.code, w.p_warning);
        }

        for x in 0..kNumPropertyWarnings as usize {
            let p = &unsupportedProperties[x];
            let t = self.types[p.base_type as usize].clone();
            self.unsupported_props_map.entry(p.name.to_string()).or_default().put(t, p.code);
        }

        for x in 0..kNumMethodWarnings as usize {
            let p = &unsupportedMethods[x];
            let t = self.types[p.base_type as usize].clone();
            self.unsupported_methods_map.entry(p.name.to_string()).or_default().put(t, p.code);
        }

        for x in 0..kNumEventWarnings as usize {
            let p = &unsupportedEvents[x];
            let t = self.types[p.base_type as usize].clone();
            self.unsupported_events_map.entry(p.name.to_string()).or_default().put(t, p.code);
        }

        self.base_type_context.push(Some(c.null_type()));
        self.base_ref_context.push(None);

        if self.enabled_map.is_none() {
            let filename = self.warning_enabled_map_file.clone().unwrap_or_else(|| "EnabledWarnings.xml".to_string());
            self.enabled_map = Some(Self::parse_enabled_warnings_file(&filename).unwrap_or_default());
        }
    }

    pub fn get_warning_defaults() -> HashMap<i32, bool> {
        let mut enabled_map = HashMap::new();
        for w in &warningConstantsEN {
            enabled_map.insert(w.code, true);
        }
        enabled_map
    }

    pub fn parse_enabled_warnings_file(filename: &str) -> std::io::Result<HashMap<i32, bool>> {
        let mut enabled_map = Self::get_warning_defaults();

        let warnings_file = std::path::Path::new(filename);
        if warnings_file.exists() && debug.load(Ordering::Relaxed) {
            let name = warnings_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            eprintln!("[coach] Using '{}'...", name);
        }

        let bytes = std::fs::read(warnings_file)?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let mut lines = java_lines(&text).into_iter();
        let mut read_line = || lines.next().expect("NullPointerException");

        let mut text_as_string = read_line();
        let mut text_length = jstr::utf16_len(&text_as_string);

        let mut endpos = jstr::index_of(&text_as_string, "</warnings>", 0);
        if endpos == -1 {
            endpos = text_length;
        }

        let mut pos = 0;
        let mut at_least_one_warning_disabled = false;

        while endpos == text_length {
            pos = jstr::index_of(&text_as_string, "<warning id=", pos);
            if pos == -1 {
                pos = text_length;
            }

            if pos != endpos {
                let code: i32 = jstr::substring(&text_as_string, pos + 13, pos + 17).parse().expect("NumberFormatException");
                pos = jstr::index_of(&text_as_string, "enabled=", pos);
                if pos == -1 {
                    pos = text_length;
                }

                if pos != endpos && jstr::substring(&text_as_string, pos + 9, pos + 14) == "false" {
                    enabled_map.insert(code, false);
                    at_least_one_warning_disabled = true;
                }

                pos = jstr::index_of(&text_as_string, "</warning>", pos);
                if pos == -1 {
                    pos = text_length;
                } else {
                    pos += 11;
                }
            }

            if pos >= endpos {
                text_as_string = read_line();
                text_length = jstr::utf16_len(&text_as_string);
                endpos = jstr::index_of(&text_as_string, "</warnings>", 0);
                if endpos == -1 {
                    endpos = text_length;
                }
                pos = 0;
            }
        }

        if debug.load(Ordering::Relaxed) && at_least_one_warning_disabled {
            eprintln!("[coach] Some coach-mode warnings are disabled");
        }

        Ok(enabled_map)
    }

    fn slot_get_compliance_record(&mut self, s: &Slot) -> Rc<RefCell<LintDataRecord>> {
        if s.get_embedded_data().is_none() {
            s.set_embedded_data(Some(Rc::new(RefCell::new(LintDataRecord::new())) as Rc<dyn Any>));
            self.slots_to_clean.push(s.clone());
        }
        s.get_embedded_data().unwrap().downcast::<RefCell<LintDataRecord>>().expect("ClassCastException")
    }

    fn slot_mark_as_declared(&mut self, s: &Slot, pos: i32) {
        self.slot_get_compliance_record(s).borrow_mut().has_been_declared = true;
        self.slot_get_compliance_record(s).borrow_mut().declaration_pos = pos;
    }

    fn slot_set_has_return_value(&mut self, s: &Slot, has_return: bool) {
        self.slot_get_compliance_record(s).borrow_mut().has_return_value = has_return;
    }

    fn slot_mark_as_registered_for_event(&mut self, s: &Slot, is_registered: bool) {
        self.slot_get_compliance_record(s).borrow_mut().is_registered_for_event = is_registered;
    }

    fn slot_is_already_declared(&mut self, s: &Option<Slot>) -> bool {
        if let Some(s) = s {
            if s.get_embedded_data().is_some() {
                return self.slot_get_compliance_record(s).borrow().has_been_declared;
            }
        }
        false
    }

    fn slot_get_has_return_value(&mut self, s: &Option<Slot>) -> bool {
        if let Some(s) = s {
            if s.get_embedded_data().is_some() {
                let r = self.slot_get_compliance_record(s);
                return r.borrow().has_return_value;
            }
        }
        false
    }

    fn slot_get_registered_for_event(&mut self, s: &Option<Slot>) -> bool {
        if let Some(s) = s {
            if s.get_embedded_data().is_some() {
                return self.slot_get_compliance_record(s).borrow().is_registered_for_event;
            }
        }
        false
    }

    pub fn test_error_strings(&mut self, _cx: &ContextRef) {}

    pub fn set_output_to_file(&mut self, otf: bool) {
        self.output_to_file = otf;
    }
}

fn clear_lint_data(s: &Slot) {
    s.set_embedded_data(None);
}

fn opt_input_eq(a: &Input, b: &Input) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

/// `BufferedReader.readLine()` over the whole text.
fn java_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let mut pending = false;
    while let Some(c) = chars.next() {
        pending = true;
        match c {
            '\n' => {
                lines.push(std::mem::take(&mut cur));
                pending = false;
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines.push(std::mem::take(&mut cur));
                pending = false;
            }
            _ => cur.push(c),
        }
    }
    if pending {
        lines.push(cur);
    }
    lines
}

#[rustfmt::skip]
impl Evaluator for LintEvaluator {
    fn check_feature(&mut self, _cx: &ContextRef, _node: &NodeRef) -> bool {
        true
    }

    fn evaluate_node(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let r = node.borrow().identifier().unwrap().ref_.clone();
        if let Some(r) = r {
            let t = r.get_type(&cx.borrow());
            return opt_val(t.get_type_value());
        }
        val(cx.borrow().no_type())
    }

    fn evaluate_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let base_type = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");
        let mut evaluated = false;
        let mut return_val: Option<Value> = None;
        let (r, args, is_new, gpos) = {
            let b = node.borrow();
            let c = b.as_call_expression().unwrap();
            (c.sel.ref_.clone(), c.args.clone(), c.is_new, b.get_position())
        };
        let items = |args: &Option<NodeRef>| -> Vec<Option<NodeRef>> {
            args.as_ref().map_or_else(Vec::new, |a| a.borrow().as_argument_list().unwrap().items.clone())
        };

        if self.first_pass {
            if r.as_ref().map_or(false, |r| r.name() == "addEventListener") {
                let items = items(&args);
                if args.is_some() && items.len() > 1 {
                    let mut arg2 = items[1].clone();

                    if let Some(e) = arg2.as_ref().and_then(|a| a.borrow().as_coerce().map(|c| c.expr.clone())) {
                        arg2 = e;
                    }
                    if let Some(e) = arg2.as_ref().and_then(|a| a.borrow().as_coerce().map(|c| c.expr.clone())) {
                        arg2 = e;
                    }

                    if rb(&arg2).is_member_expression() {
                        let (mref, selector) = {
                            let b = rb(&arg2);
                            let m = b.as_member_expression().unwrap();
                            (m.ref_.clone(), m.selector.clone())
                        };
                        if let Some(mref) = mref {
                            if rb(&selector).is_get_expression() {
                                let s = mref.get_slot_kind(&cx.borrow(), GET_TOKEN);
                                if let Some(s) = &s {
                                    self.slot_mark_as_registered_for_event(s, true);
                                }
                            }
                        }
                    }
                }
            }

            return self.evaluate_generic_call_expression(cx, node);
        }

        if is_new || r.is_none() {
            if !evaluated {
                return_val = self.evaluate_generic_call_expression(cx, node);
            }
            return return_val;
        } else if let Some(r) = &r {
            let items = items(&args);
            let num_args = items.len();
            let name = r.name();
            let null_type = cx.borrow().null_type();
            let input = Self::input(cx);

            if name == "__resolve" {
                self.warning(gpos, &input, kWarning_ChangesInResolve);
            } else if opt_is(&base_type, &null_type) && name == "Number" && num_args == 1 {
                let arg = items[0].clone();
                let arg_type_val = self.eval(cx, &arg);

                evaluated = true;
                return_val = val(cx.borrow().double_type());
                if is(&arg_type_val, &cx.borrow().string_type())
                    && rb(&arg).as_literal_string().map_or(false, |l| trims_to_empty(&l.value))
                {
                    self.warning(gpos, &input, kWarning_NumberFromStringChanges);
                }
            } else if (opt_is(&base_type, &null_type) && name == "String" && num_args == 1)
                || (!opt_is(&base_type, &null_type) && name == "toString" && num_args == 0)
            {
                let arg_type_val: Option<TypeValue>;

                if num_args == 1 {
                    let arg = items[0].clone();
                    let arg_type = self.eval(cx, &arg);

                    arg_type_val = tv_of(&arg_type);
                } else {
                    arg_type_val = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");
                }
                evaluated = true;
                return_val = val(cx.borrow().string_type());

                if opt_is(&arg_type_val, &cx.borrow().array_type()) {
                    self.warning(gpos, &input, kWarning_ArrayToStringChanges);
                }

                let s = r.get_slot_kind(&cx.borrow(), if is_new { NEW_TOKEN } else { EMPTY_TOKEN });
                self.check_deprecated_slot(cx, Some(node.clone()), r, &s);
            }
        }

        if !evaluated {
            return_val = self.evaluate_generic_call_expression(cx, node);
        }
        return_val
    }

    fn evaluate_invoke(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (args, name) = {
            let b = node.borrow();
            let i = b.as_invoke().unwrap();
            (i.args.clone(), i.name.clone())
        };
        if args.is_some() {
            self.eval(cx, &args);
        }

        let c = cx.borrow();
        match name.as_str() {
            "[[HasMoreNames]]" => val(c.boolean_type()),
            "[[NextValue]]" => val(c.no_type()),
            "[[NextName]]" => val(c.string_type()),
            "[[ToXMLString]]" => val(c.string_type()),
            _ => None,
        }
    }

    fn evaluate_delete_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr, void_result) = {
            let b = node.borrow();
            let d = b.as_delete_expression().unwrap();
            (d.sel.ref_.clone(), d.sel.expr.clone(), d.void_result)
        };
        self.eval(cx, &expr);

        let slot = r.as_ref().and_then(|r| r.get_slot_kind(&cx.borrow(), GET_TOKEN));

        if slot.is_some() && !self.in_with {
            let input = Self::input(cx);
            let pos = rb(&expr).pos();
            self.warning1(pos, &input, kWarning_DeleteOfFixedProperty, &r.as_ref().unwrap().name());
        }

        let c = cx.borrow();
        val(if void_result { c.void_type() } else { c.boolean_type() })
    }

    fn evaluate_apply_type_expr(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let type_args = node.borrow().as_apply_type_expr().unwrap().type_args.clone();
        self.eval(cx, &type_args);
        None
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr, mode, sel_base, pos, gpos) = {
            let b = node.borrow();
            let s = b.selector().unwrap();
            (s.ref_.clone(), s.expr.clone(), s.get_mode(), s.base.clone(), b.pos(), b.get_position())
        };
        let mut result: Option<Value> = None;

        if self.first_pass {
            if r.is_none() {
                self.eval(cx, &expr);
            }

            return result;
        }

        if mode == LEFTBRACKET_TOKEN {
            if rb(&expr).is_literal_integer() {
                result = val(cx.borrow().no_type());
            } else {
                self.eval(cx, &expr);
                result = val(cx.borrow().no_type());
            }
        } else if let Some(r) = &r {
            let mut slot = r.get_slot_kind(&cx.borrow(), GET_TOKEN);

            let basetype = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");
            if slot.is_none() && is_class_type(&basetype) {
                let base_ref = self.base_ref_context.last().cloned().expect("IndexOutOfBoundsException");

                if let Some(base_ref) = base_ref {
                    let c = cx.borrow();
                    let real_base = base_ref.get_value(&c).map(|v| v.as_object_value().expect("ClassCastException"));
                    let old_base = r.get_base();
                    r.set_base(real_base);
                    slot = r.get_slot_kind(&c, GET_TOKEN);
                    r.set_base(old_base);
                }
            }

            if slot.is_none() {
                if let Some(proto) = basetype.as_ref().and_then(|b| b.prototype()) {
                    slot = self.get_slot(&proto, cx, node, GET_TOKEN);
                }
            }

            if slot.is_some() {
                self.check_deprecated_slot(cx, Some(node.clone()), r, &slot);
            }

            let ti = slot.as_ref().and_then(|s| s.get_type());
            let type_ = ti.and_then(|t| t.get_type_value());

            result = opt_val(type_.clone());
            if result.is_none() {
                result = val(cx.borrow().void_type());
            } else if r.name() == "undefined" {
                result = self.undefined_literal.clone().map(Value::Object);
            }

            let ignore_keyword = hack_ignore_identifier(&r.name());

            let base_index = r.get_scope_index_kind(GET_TOKEN);
            let slot_index = r.get_slot_index(GET_TOKEN);
            let is_globalref = base_index == 0;
            let is_dotref = base_index == -2;
            let is_unbound_lexref = base_index == -1;
            let is_unbound_dotref = is_dotref && slot_index < 0;
            let is_unbound_globalref = is_globalref && slot_index < 0;
            let is_unbound_ref = is_unbound_dotref || is_unbound_lexref || is_unbound_globalref;

            if slot.is_none() && ignore_keyword != Some(true) && is_unbound_ref {
                if is_class_type(&basetype) && r.name() == "prototype" {
                    return self.eval(cx, &expr);
                }
                let input = Self::input(cx);
                if self.is_es3_dynamic_type(cx, &basetype, false) {
                    let tn = self.get_simple_type_name(&basetype);
                    self.warning2(pos, &input, kWarning_BadES3TypeProp, &r.name(), &tn);
                } else {
                    let mut unsupported = false;
                    let search = self.unsupported_props_map.get(&r.name()).cloned();

                    if let Some(search) = search.filter(|s| !s.is_empty()) {
                        let mut search_type = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");

                        if is_class_type(&search_type) {
                            search_type = self.class_search_type(cx, search_type).0;
                        }

                        for (match_type, code) in &search.0 {
                            if let Some(match_type) = match_type {
                                if match_type.includes(&cx.borrow(), search_type.as_ref()) {
                                    unsupported = true;
                                    let msg = self.warning_constants_map.get(code).copied();
                                    self.warning_opt(gpos, &input, kWarning_DepricatedPropertyError, Some(&r.name()), msg, Some(""));
                                }
                            }
                        }
                    }

                    if !unsupported && r.name().starts_with("_level") && sel_base.is_none() {
                        self.warning(gpos, &input, kWarning_LevelNotSupported);
                    }

                    let base = r.get_base();
                    let c = cx.borrow();
                    return val(if base.map_or(false, |b| b.is_dynamic()) { c.no_type() } else { c.void_type() });
                }
            }
            if opt_is(&type_, &cx.borrow().function_type()) {
                slot = r.get_slot_kind(&cx.borrow(), EMPTY_TOKEN);
                let (xml, xml_list) = {
                    let c = cx.borrow();
                    (c.xml_type(), c.xml_list_type())
                };
                if slot.is_some() && !opt_is(&basetype, &xml) && !opt_is(&basetype, &xml_list) {
                    let input = Self::input(cx);
                    let base_name = sel_base.as_ref().map_or_else(String::new, |b| b.name());
                    self.warning2(gpos, &input, kWarning_ScopingChangeInThis, &r.name(), &base_name);
                }
            }
        } else {
            result = self.eval(cx, &expr);
        }

        result
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, expr, args, gpos) = {
            let b = node.borrow();
            let s = b.as_set_expression().unwrap();
            (s.sel.ref_.clone(), s.sel.expr.clone(), s.args.clone(), b.get_position())
        };
        if self.first_pass {
            return self.eval(cx, &args);
        }

        let mut slot: Option<Slot> = None;
        if let Some(r) = &r {
            slot = r.get_slot_kind(&cx.borrow(), GET_TOKEN);

            let base_type = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");
            if is_class_type(&base_type) {
                let base_ref = self.base_ref_context.last().cloned().expect("IndexOutOfBoundsException");

                if let Some(base_ref) = base_ref {
                    let c = cx.borrow();
                    let real_base = base_ref.get_value(&c).map(|v| v.as_object_value().expect("ClassCastException"));
                    let old_base = r.get_base();
                    r.set_base(real_base);
                    slot = r.get_slot_kind(&c, GET_TOKEN);
                    r.set_base(old_base);
                }
            }

            if slot.is_none() {
                if let Some(proto) = base_type.as_ref().and_then(|b| b.prototype()) {
                    slot = self.get_slot(&proto, cx, node, SET_TOKEN);
                }
            }
            let base_index = r.get_scope_index_kind(SET_TOKEN);
            let slot_index = r.get_slot_index(SET_TOKEN);
            let is_globalref = base_index == 0;
            let is_dotref = base_index == -2;
            let is_unbound_lexref = base_index == -1;
            let is_unbound_dotref = is_dotref && slot_index < 0;
            let is_unbound_globalref = is_globalref && slot_index < 0;
            let is_unbound_ref = is_unbound_dotref || is_unbound_lexref || is_unbound_globalref;

            if slot.is_some() {
                self.check_deprecated_slot(cx, expr.clone(), r, &slot);
            }

            let input = Self::input(cx);
            let uint_type = cx.borrow().uint_type();
            let first_arg = args.as_ref().and_then(|a| {
                let a = a.borrow();
                let l = a.as_argument_list().unwrap();
                if l.items.len() == 1 { Some(l.items[0].clone()) } else { None }
            });

            if is_class_type(&base_type) && r.name() == "prototype" {
            } else if slot.as_ref().map_or(false, |s| opt_is(&slot_type_value(s), &uint_type))
                && first_arg.as_ref().map_or(false, |a| kind_is(a, |n| n.as_literal_number().is_some()))
            {
                let ln = first_arg.unwrap();
                let d = rb(&ln).as_literal_number().unwrap().numeric_value.as_ref().expect("NullPointerException").double_value();
                if d < 0.0 {
                    self.warning(gpos, &input, kWarning_NegativeUintLiteral);
                }
            } else if slot.is_none() && is_unbound_ref {
                let pos = if expr.is_some() { rb(&expr).get_position() } else { gpos };
                let mut unsupported = false;
                if self.is_es3_dynamic_type(cx, &base_type, false) {
                    let npos = node.borrow().pos();
                    let tn = self.get_simple_type_name(&base_type);
                    self.warning2(npos, &input, kWarning_BadES3TypeProp, &r.name(), &tn);
                    unsupported = true;
                } else {
                    let search = self.unsupported_props_map.get(&r.name()).cloned();

                    if let Some(search) = search.filter(|s| !s.is_empty()) {
                        let mut search_type = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");

                        if is_class_type(&search_type) {
                            search_type = self.class_search_type(cx, search_type).0;
                        }

                        for (type_, code) in &search.0 {
                            if let Some(type_) = type_ {
                                if type_.includes(&cx.borrow(), search_type.as_ref()) {
                                    unsupported = true;
                                    let msg = self.warning_constants_map.get(code).copied();
                                    self.warning_opt(pos, &input, kWarning_DepricatedPropertyError, Some(&r.name()), msg, Some(""));
                                }
                            }
                        }
                    }
                }

                if !unsupported && base_type.is_some() {
                    let search = self.unsupported_events_map.get(&r.name()).cloned();
                    if let Some(search) = search.filter(|s| !s.is_empty()) {
                        let _scope = cx.borrow().scope();

                        if base_type.is_some() {
                            let mut search_type = self.base_type_context.last().cloned().expect("IndexOutOfBoundsException");

                            if is_class_type(&search_type) {
                                search_type = self.class_search_type(cx, search_type).0;
                            }

                            for (type_, code) in &search.0 {
                                if let Some(type_) = type_ {
                                    if type_.includes(&cx.borrow(), search_type.as_ref()) {
                                        let msg = self.warning_constants_map.get(code).copied();
                                        self.warning_opt(pos, &input, kWarning_DepricatedEventHandlerError, msg, Some(""), Some(""));
                                    }
                                }
                            }
                        }
                    }
                }

                let base = r.get_base();
                let sealed = {
                    let c = cx.borrow();
                    let bt = base_type.as_ref();
                    (base_type.is_some() && !opt_is(&base_type, &c.void_type()) && !opt_is(&base_type, &c.null_type()))
                        && (base.as_ref().map_or(false, |b| b.is_final() && !b.is_dynamic())
                            || c.double_type().includes(&c, bt)
                            || opt_is(&base_type, &c.number_type())
                            || (c.statics.borrow().es4_numerics && opt_eq(&c.decimal_type(), &base_type))
                            || opt_is(&base_type, &c.string_type())
                            || opt_is(&base_type, &c.boolean_type())
                            || opt_eq(&self.types[kMathType as usize], &base_type))
                };
                if sealed {
                    let tn = self.get_simple_type_name(&base_type);
                    self.warning1(gpos, &input, kWarning_ClassIsSealed, &tn);
                }
            } else if opt_eq(&base_type, &self.types[kTextFieldType as usize]) && r.name() == "text" {
                let arg1 = {
                    let a = rb(&args);
                    a.as_argument_list().unwrap().items.first().cloned().expect("IndexOutOfBoundsException")
                };
                let lhs = arg1.as_ref().and_then(|a| a.borrow().as_binary_expression().map(|b| b.lhs.clone()));
                if let Some(lhs) = lhs {
                    let memb_arg_base = lhs.as_ref().and_then(|l| l.borrow().as_member_expression().map(|m| m.base.clone()));
                    if let Some(memb_arg_base) = memb_arg_base {
                        let base_ref = memb_arg_base
                            .as_ref()
                            .and_then(|b| b.borrow().as_member_expression().map(|m| m.ref_.clone()));
                        let br = self.base_ref_context.last().cloned().expect("IndexOutOfBoundsException");
                        if let (Some(Some(mref)), Some(br)) = (&base_ref, &br) {
                            let same_slot = match (br.slot(), mref.slot()) {
                                (Some(a), Some(b)) => a.ptr_eq(&b),
                                (None, None) => true,
                                _ => false,
                            };
                            if same_slot {
                                self.warning(gpos, &input, kWarning_SlowTextFieldAddition);
                            }
                        }
                    }
                }
            }
        } else {
            self.eval(cx, &expr);
        }

        let result = self.eval(cx, &args);
        let (null_type, boolean_type, no_type, object_type) = {
            let c = cx.borrow();
            (c.null_type(), c.boolean_type(), c.no_type(), c.object_type())
        };
        if is(&result, &null_type) && slot.is_some() {
            let t = slot_type_value(slot.as_ref().unwrap());
            if t.as_ref().map_or(false, |t| t.is_numeric(&cx.borrow()) || t.ptr_eq(&boolean_type)) {
                let input = Self::input(cx);
                let apos = rb(&args).get_position();
                self.warning1(apos, &input, kWarning_BadNullAssignment, &tv_name(&t));
            }
        } else if slot.as_ref().map_or(false, |s| opt_is(&slot_type_value(s), &boolean_type))
            && tv_of(&result).is_some()
            && !is(&result, &boolean_type)
            && !is(&result, &no_type)
            && !is(&result, &object_type)
        {
            let rt = tv_of(&result);
            let input = Self::input(cx);
            let apos = rb(&args).get_position();
            self.warning1(apos, &input, kWarning_BadBoolAssignment, &tv_name(&rt));
        }

        result
    }

    fn evaluate_this_expression(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        let this_context = *self.this_contexts.last().expect("IndexOutOfBoundsException");

        let c = cx.borrow();
        let this_value = if this_context == global_this {
            Some(c.scope_at(0))
        } else if this_context == instance_this {
            let scope_depth = c.get_scope_depth() as i32 - 1;
            Some(c.scope_at(scope_depth - 1))
        } else {
            None
        };

        this_value.and_then(|v| v.get_type(&c).expect("NullPointerException").get_type_value()).map(Value::from)
    }

    fn evaluate_literal_boolean(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().boolean_type())
    }

    fn evaluate_literal_number(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        opt_val(node.borrow().as_literal_number().unwrap().type_.clone())
    }

    fn evaluate_literal_string(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().string_type())
    }

    fn evaluate_literal_null(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().null_type())
    }

    fn evaluate_literal_reg_exp(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().reg_exp_type())
    }

    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = node.borrow().as_literal_xml().unwrap().list.clone();
        if list.is_some() {
            self.eval(cx, &list);
        }
        val(cx.borrow().xml_type())
    }

    fn evaluate_paren_expression(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_paren_list_expression(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let fieldlist = node.borrow().as_literal_object().unwrap().fieldlist.clone();
        if fieldlist.is_some() {
            self.eval(cx, &fieldlist);
        }

        val(cx.borrow().no_type())
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (name, value, r) = {
            let b = node.borrow();
            let f = b.as_literal_field().unwrap();
            (f.name.clone(), f.value.clone(), f.ref_.clone())
        };
        self.eval(cx, &name);
        self.eval(cx, &value);

        r.and_then(|r| r.get_type(&cx.borrow()).get_type_value()).map(Value::from)
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let elementlist = node.borrow().as_literal_array().unwrap().elementlist.clone();
        if elementlist.is_some() {
            self.eval(cx, &elementlist);
        }

        val(cx.borrow().array_type())
    }

    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (type_, elementlist) = {
            let b = node.borrow();
            let v = b.as_literal_vector().unwrap();
            (v.type_.clone(), v.elementlist.clone())
        };
        self.eval(cx, &type_);

        if elementlist.is_some() {
            self.eval(cx, &elementlist);
        }

        val(cx.borrow().vector_type())
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (base, selector) = {
            let b = node.borrow();
            let m = b.as_member_expression().unwrap();
            (m.base.clone(), m.selector.clone())
        };
        if base.is_some() {
            let result = self.eval(cx, &base);
            let base_type = tv_of(&result);

            if base_type.is_none() || opt_is(&base_type, &cx.borrow().void_type()) {
                return val(cx.borrow().void_type());
            }
            self.base_type_context.push(base_type);

            let simple_base_ref = rb(&base).as_member_expression().map(|m| m.ref_.clone());

            self.base_ref_context.push(simple_base_ref.flatten());
        } else {
            self.base_type_context.push(Some(cx.borrow().null_type()));
            self.base_ref_context.push(None);
        }

        let result = self.eval(cx, &selector);
        let base_type = self.base_type_context.pop().expect("IndexOutOfBoundsException");
        self.base_ref_context.pop();

        let c = cx.borrow();
        if opt_is(&base_type, &c.xml_type()) || opt_is(&base_type, &c.xml_list_type()) {
            val(c.no_type())
        } else {
            result
        }
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, op, slot) = {
            let b = node.borrow();
            let u = b.as_unary_expression().unwrap();
            (u.expr.clone(), u.op, u.slot.clone())
        };
        let result = self.eval(cx, &expr);

        if is(&result, &cx.borrow().function_type()) {
            if kind_is(&expr, Node::is_member_expression) {
                let func_name = member_ref_name(&expr);
                let input = Self::input(cx);
                let pos = rb(&expr).pos();
                let object_name = cx.borrow().object_type().to_string();
                self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &object_name, &func_name);
            }
        }

        if op == TYPEOF_TOKEN {
            return val(cx.borrow().no_type());
        }
        match &slot {
            Some(s) => opt_val(slot_type_value(s)),
            None => val(cx.borrow().void_type()),
        }
    }

    fn evaluate_increment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, slot, number_usage) = {
            let b = node.borrow();
            let i = b.as_increment().unwrap();
            (i.sel.expr.clone(), i.slot.clone(), i.number_usage.clone())
        };
        self.eval(cx, &expr);
        if let Some(s) = &slot {
            opt_val(slot_type_value(s))
        } else {
            let c = cx.borrow();
            let current_number_type = match number_usage.map(|n| n.get_usage()) {
                Some(NumberUsage::use_int) => Some(c.int_type()),
                Some(NumberUsage::use_uint) => Some(c.uint_type()),
                Some(NumberUsage::use_decimal) => c.decimal_type(),
                _ => Some(c.double_type()),
            };
            opt_val(current_number_type)
        }
    }

    fn evaluate_binary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (lhs, rhs, op, slot, gpos) = {
            let b = node.borrow();
            let e = b.as_binary_expression().unwrap();
            (e.lhs.clone(), e.rhs.clone(), e.op, e.slot.clone(), b.get_position())
        };
        let mut lhs_type: Option<Value> = None;
        let mut rhs_type: Option<Value> = None;
        let slot_type = |cx: &ContextRef| match &slot {
            Some(s) => opt_val(slot_type_value(s)),
            None => val(cx.borrow().void_type()),
        };

        if self.first_pass {
            if lhs.is_some() {
                self.eval(cx, &lhs);
            }
            if rhs.is_some() {
                self.eval(cx, &rhs);
            }
            return slot_type(cx);
        }

        if lhs.is_some() {
            lhs_type = self.eval(cx, &lhs);
        }

        let input = Self::input(cx);
        match op {
            LESSTHAN_TOKEN | GREATERTHAN_TOKEN | LESSTHANOREQUALS_TOKEN | GREATERTHANOREQUALS_TOKEN | STRICTEQUALS_TOKEN
            | STRICTNOTEQUALS_TOKEN | EQUALS_TOKEN | NOTEQUALS_TOKEN => {
                if rhs.is_some() {
                    rhs_type = self.eval(cx, &rhs);
                }

                let (void_type, no_type, null_type) = {
                    let c = cx.borrow();
                    (c.void_type(), c.no_type(), c.null_type())
                };
                let undef = self.undefined_literal.clone();
                if !same(&lhs_type, &rhs_type) && !is(&lhs_type, &void_type) && !is(&rhs_type, &void_type) {
                    if (is_obj(&lhs_type, &undef) && !is(&rhs_type, &no_type))
                        || (!is(&lhs_type, &no_type) && is_obj(&rhs_type, &undef))
                    {
                        let type_name = if is_obj(&lhs_type, &undef) {
                            self.get_simple_type_name(&cast_tv(&rhs_type))
                        } else {
                            self.get_simple_type_name(&cast_tv(&lhs_type))
                        };
                        self.warning2(gpos, &input, kWarning_BadUndefinedComparision, &type_name, &type_name);
                    } else if is(&lhs_type, &null_type) || is(&rhs_type, &null_type) {
                        let non_void_type = if is(&lhs_type, &null_type) { cast_tv(&rhs_type) } else { cast_tv(&lhs_type) };

                        match non_void_type.as_ref().expect("NullPointerException").get_type_id() {
                            TYPE_boolean | TYPE_int | TYPE_uint | TYPE_double | TYPE_decimal => {
                                let tn = self.get_simple_type_name(&non_void_type);
                                self.warning1(gpos, &input, kWarning_BadNullComparision, &tn);
                            }
                            _ => {}
                        }
                    }
                }
                let (double_type, decimal_type, es4_numerics) = {
                    let c = cx.borrow();
                    let es4 = c.statics.borrow().es4_numerics;
                    (c.double_type(), c.decimal_type(), es4)
                };
                let nan_getter = |side: &Option<NodeRef>| -> bool {
                    let Some(side) = side else { return false };
                    let b = side.borrow();
                    let Some(mem) = b.as_member_expression() else { return false };
                    let Some(sel) = &mem.selector else { return false };
                    let s = sel.borrow();
                    let Some(getter) = s.as_get_expression() else { return false };
                    getter.sel.ref_.as_ref().expect("NullPointerException").name() == "NaN"
                };
                if is(&lhs_type, &double_type) || (es4_numerics && same_tv(&lhs_type, &decimal_type)) {
                    if nan_getter(&lhs) {
                        self.warning(gpos, &input, kWarning_BadNaNComparision);
                    }
                }
                // Java tests lhsType for decimal here too.
                if is(&rhs_type, &double_type) || (es4_numerics && same_tv(&lhs_type, &decimal_type)) {
                    if nan_getter(&rhs) {
                        self.warning(gpos, &input, kWarning_BadNaNComparision);
                    }
                }
            }

            INSTANCEOF_TOKEN => {
                let rpos = rb(&rhs).get_position();
                self.warning(rpos - 11, &input, kWarning_InstanceOfChanges);
                if rhs.is_some() {
                    self.eval(cx, &rhs);
                }
            }

            MULT_TOKEN | DIV_TOKEN | MODULUS_TOKEN | MINUS_TOKEN | LEFTSHIFT_TOKEN | RIGHTSHIFT_TOKEN
            | UNSIGNEDRIGHTSHIFT_TOKEN | BITWISEAND_TOKEN | BITWISEXOR_TOKEN | BITWISEOR_TOKEN | LOGICALAND_TOKEN
            | LOGICALOR_TOKEN => {
                if rhs.is_some() {
                    rhs_type = self.eval(cx, &rhs);
                }

                let (function_type, object_name) = {
                    let c = cx.borrow();
                    (c.function_type(), c.object_type().to_string())
                };
                if is(&lhs_type, &function_type) {
                    if kind_is(&lhs, Node::is_member_expression) {
                        let func_name = member_ref_name(&lhs);
                        let pos = rb(&lhs).pos();
                        self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &object_name, &func_name);
                    }
                }
                if is(&rhs_type, &function_type) {
                    if kind_is(&rhs, Node::is_member_expression) {
                        let func_name = member_ref_name(&rhs);
                        let pos = rb(&rhs).pos();
                        self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &object_name, &func_name);
                    }
                }
            }

            _ => {
                if rhs.is_some() {
                    self.eval(cx, &rhs);
                }
            }
        }

        slot_type(cx)
    }

    fn evaluate_conditional_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (condition, thenexpr, elseexpr) = {
            let b = node.borrow();
            let c = b.as_conditional_expression().unwrap();
            (c.condition.clone(), c.thenexpr.clone(), c.elseexpr.clone())
        };
        let mut result = None;
        let mut result2 = None;

        if condition.is_some() {
            self.eval(cx, &condition);
        }

        if thenexpr.is_some() {
            result = self.eval(cx, &thenexpr);
        }

        if elseexpr.is_some() {
            result2 = self.eval(cx, &elseexpr);
        }

        if same(&result, &result2) {
            result
        } else {
            let type1 = tv_of(&result);
            let type2 = tv_of(&result2);

            match (type1, type2) {
                (Some(type1), Some(type2)) => {
                    let c = cx.borrow();
                    if type1.includes(&c, Some(&type2)) {
                        val(type1)
                    } else if type2.includes(&c, Some(&type1)) {
                        val(type2)
                    } else {
                        val(c.no_type())
                    }
                }
                _ => None,
            }
        }
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut result: Option<Value> = None;
        let (items, decl_styles, expected_types) = {
            let b = node.borrow();
            let a = b.as_argument_list().unwrap();
            (a.items.clone(), a.decl_styles.clone(), a.expected_types.clone())
        };
        let (void_type, function_type, object_type, no_type, uint_type, boolean_type) = {
            let c = cx.borrow();
            (c.void_type(), c.function_type(), c.object_type(), c.no_type(), c.uint_type(), c.boolean_type())
        };
        let expected_at = |i: usize| -> Option<TypeValue> {
            let et = expected_types.as_ref().expect("NullPointerException");
            et.get(i).expect("IndexOutOfBoundsException").as_ref().expect("NullPointerException").get_type_value()
        };

        let num_declared_params = decl_styles.as_ref().map_or(0, |d| d.len());
        if num_declared_params != 0 && !opt_is(&expected_at(0), &void_type) {
            let decl_styles = decl_styles.unwrap();
            let mut param_count = 0;
            let mut expected_type: Option<TypeValue>;

            for item in &items {
                result = self.eval(cx, item);

                if param_count < num_declared_params
                    && decl_styles[param_count] as i32 != PARAM_Rest
                    && result.is_some()
                    && tv_of(&result).is_some()
                {
                    expected_type = expected_at(param_count);

                    let input = Self::input(cx);
                    if !same_tv(&result, &expected_type)
                        && is(&result, &function_type)
                        && !opt_is(&expected_type, &object_type)
                        && !opt_is(&expected_type, &no_type)
                    {
                        if kind_is(item, Node::is_member_expression) {
                            let func_name = member_ref_name(item);
                            let pos = rb(item).pos();
                            self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &tv_name(&expected_type), &func_name);
                        }
                    } else if opt_is(&expected_type, &uint_type)
                        && kind_is(item, |n| n.as_literal_number().is_some())
                        && rb(item).as_literal_number().unwrap().numeric_value.as_ref().expect("NullPointerException").double_value() < 0.0
                    {
                        let pos = rb(item).pos();
                        self.warning(pos, &input, kWarning_NegativeUintLiteral);
                    } else if expected_type.as_ref().map_or(false, |e| e.is_numeric(&cx.borrow()) || e.ptr_eq(&boolean_type))
                        && kind_is(item, |n| n.as_literal_null().is_some())
                    {
                        let pos = rb(item).get_position();
                        self.warning1(pos, &input, kWarning_BadNullAssignment, &tv_name(&expected_type));
                    } else if opt_is(&expected_type, &boolean_type)
                        && tv_of(&result).is_some()
                        && !is(&result, &boolean_type)
                        && !is(&result, &no_type)
                        && !is(&result, &object_type)
                    {
                        let rt = tv_of(&result);
                        let pos = rb(item).get_position();
                        self.warning1(pos, &input, kWarning_BadBoolAssignment, &tv_name(&rt));
                    }

                    if decl_styles[param_count] as i32 != PARAM_Rest {
                        param_count += 1;
                    }
                }
            }
        } else {
            for item in &items {
                result = self.eval(cx, item);
            }
        }

        result
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut result = None;
        let items = node.borrow().list().unwrap().items.clone();

        for item in &items {
            result = self.eval(cx, item);
        }

        result
    }

    // Statements

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut result = None;
        let items = node.borrow().as_statement_list().unwrap().items.clone();

        for item in &items {
            if !self.doing_method {
                self.doing_method = true;
            }

            if item.is_some() {
                result = self.eval(cx, item);

                // "func;" where "func();" was meant.
                if is(&result, &cx.borrow().function_type()) && kind_is(item, Node::is_expression_statement) {
                    let exp_expr = rb(item).as_expression_statement().unwrap().expr.clone();
                    let an_item = exp_expr.as_ref().and_then(|e| {
                        let e = e.borrow();
                        e.list().filter(|l| l.items.len() == 1).map(|l| l.items[0].clone())
                    });
                    if let Some(an_item) = an_item {
                        let is_get_member = an_item.as_ref().map_or(false, |a| {
                            a.borrow().as_member_expression().map_or(false, |m| kind_is(&m.selector, Node::is_get_expression))
                        });
                        if is_get_member {
                            let input = Self::input(cx);
                            let pos = rb(item).pos();
                            let void_name = cx.borrow().void_type().name().to_string();
                            self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &void_name, "the function");
                        }
                    }
                }
            }
        }

        result
    }

    fn evaluate_empty_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_expression_statement().unwrap().expr.clone();
        let mut result = None;
        if expr.is_some() {
            result = self.eval(cx, &expr);
        }

        result
    }

    fn evaluate_labeled_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let statement = node.borrow().as_labeled_statement().unwrap().statement.clone();
        if statement.is_some() {
            self.eval(cx, &statement);
        }

        None
    }

    fn evaluate_if_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (condition, thenactions, elseactions, gpos) = {
            let b = node.borrow();
            let i = b.as_if_statement().unwrap();
            (i.condition.clone(), i.thenactions.clone(), i.elseactions.clone(), b.get_position())
        };
        if condition.is_some() {
            if !self.first_pass {
                let assigns = {
                    let c = rb(&condition);
                    c.list().filter(|l| l.items.len() == 1).map_or(false, |l| {
                        l.items[0].as_ref().map_or(false, |item| {
                            item.borrow().as_coerce().map_or(false, |cn| {
                                cn.expr.as_ref().map_or(false, |e| {
                                    e.borrow().as_member_expression().map_or(false, |m| kind_is(&m.selector, Node::is_set_expression))
                                })
                            })
                        })
                    })
                };
                if assigns {
                    let input = Self::input(cx);
                    self.warning(gpos, &input, kWarning_AssignmentWithinConditional);
                }
            }

            self.eval(cx, &condition);
        }

        if thenactions.is_some() {
            if !self.first_pass && single_empty_statement(&thenactions) {
                let input = Self::input(cx);
                self.warning(gpos, &input, kWarning_UnexpectedEmptyStatement);
            }

            self.eval(cx, &thenactions);
        }

        if elseactions.is_some() {
            self.eval(cx, &elseactions);
        }

        None
    }

    fn evaluate_switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statements) = {
            let b = node.borrow();
            let s = b.as_switch_statement().unwrap();
            (s.expr.clone(), s.statements.clone())
        };
        if expr.is_some() {
            self.eval(cx, &expr);
        }

        if statements.is_some() {
            self.eval(cx, &statements);
        }

        None
    }

    fn evaluate_case_label(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let label = node.borrow().as_case_label().unwrap().label.clone();
        if label.is_some() {
            self.eval(cx, &label);
        }

        None
    }

    fn evaluate_do_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statements) = {
            let b = node.borrow();
            let d = b.as_do_statement().unwrap();
            (d.expr.clone(), d.statements.clone())
        };
        if expr.is_some() {
            self.eval(cx, &expr);
        }

        if statements.is_some() {
            self.eval(cx, &statements);
        }

        None
    }

    fn evaluate_while_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (statement, expr, gpos) = {
            let b = node.borrow();
            let w = b.as_while_statement().unwrap();
            (w.statement.clone(), w.expr.clone(), b.get_position())
        };
        if statement.is_some() {
            if !self.first_pass && single_empty_statement(&statement) {
                let input = Self::input(cx);
                self.warning(gpos, &input, kWarning_UnexpectedEmptyStatement);
            }

            self.eval(cx, &statement);
        }
        if expr.is_some() {
            self.eval(cx, &expr);
        }

        None
    }

    fn evaluate_for_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (initialize, increment, test, statement, is_forin, gpos) = {
            let b = node.borrow();
            let f = b.as_for_statement().unwrap();
            (f.initialize.clone(), f.increment.clone(), f.test.clone(), f.statement.clone(), f.is_forin, b.get_position())
        };
        if is_forin && !self.first_pass {
            let input = Self::input(cx);
            self.warning(gpos, &input, kWarning_ForVarInChanges);
        }

        if let Some(init) = &initialize {
            self.eval(cx, &initialize);
            init.borrow_mut().void_result();
        }
        if let Some(inc) = &increment {
            self.eval(cx, &increment);
            inc.borrow_mut().void_result();
        }
        if test.is_some() {
            self.eval(cx, &test);
        }
        if statement.is_some() {
            if !self.first_pass && single_empty_statement(&statement) {
                let input = Self::input(cx);
                self.warning(gpos, &input, kWarning_UnexpectedEmptyStatement);
            }

            self.eval(cx, &statement);
        }
        None
    }

    fn evaluate_with_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, statement, activation) = {
            let b = node.borrow();
            let w = b.as_with_statement().unwrap();
            (w.expr.clone(), w.statement.clone(), w.activation.clone())
        };
        if expr.is_some() {
            self.eval(cx, &expr);
        }

        if statement.is_some() {
            cx.borrow().push_scope(activation.expect("NullPointerException"));

            let saved_in_with = self.in_with;
            self.in_with = true;

            let save_with_depth = cx.borrow().statics.borrow().with_depth;
            let depth = cx.borrow().get_scope_depth() as i32 - 1;
            cx.borrow().statics.borrow_mut().with_depth = depth;

            self.eval(cx, &statement);

            cx.borrow().statics.borrow_mut().with_depth = save_with_depth;

            self.in_with = saved_in_with;

            cx.borrow().pop_scope();
        }

        None
    }

    fn evaluate_continue_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let id = node.borrow().as_continue_statement().unwrap().id.clone();
        if id.is_some() {
            self.eval(cx, &id);
        }

        None
    }

    fn evaluate_break_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_return_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_return_statement().unwrap().expr.clone();
        if expr.is_some() {
            let return_type = self.eval(cx, &expr);
            if !is(&return_type, &cx.borrow().void_type()) {
                self.body_has_return = true;
            }

            if !self.first_pass && !same_tv(&return_type, &self.expected_return_type) {
                let mut item = expr.clone();
                // "return (a = 0, a += b, a)" checks the last item.
                loop {
                    let next = match &item {
                        Some(i) => match i.borrow().list() {
                            Some(list) => list.items.last().cloned().expect("IndexOutOfBoundsException"),
                            None => break,
                        },
                        None => break,
                    };
                    item = next;
                }

                let cond = item.as_ref().and_then(|i| {
                    i.borrow().as_conditional_expression().map(|c| (c.thenexpr.clone(), c.elseexpr.clone()))
                });
                if let Some((thenexpr, elseexpr)) = cond {
                    let unwrap_coerce = |e: &Option<NodeRef>| -> Option<Option<NodeRef>> {
                        e.as_ref().and_then(|e| e.borrow().as_coerce().map(|c| c.expr.clone()))
                    };
                    match unwrap_coerce(&thenexpr) {
                        Some(cn_expr) => self.check_return_statement_type(cx, &cn_expr, &return_type),
                        None => self.check_return_statement_type(cx, &thenexpr, &return_type),
                    }

                    match unwrap_coerce(&elseexpr) {
                        Some(cn_expr) => self.check_return_statement_type(cx, &cn_expr, &return_type),
                        None => self.check_return_statement_type(cx, &elseexpr, &return_type),
                    }
                } else {
                    self.check_return_statement_type(cx, &item, &return_type);
                }
            }
        }
        None
    }

    // Definitions

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if node.borrow().skip() {
            return val(cx.borrow().no_type());
        }

        let (node_context, list) = {
            let b = node.borrow();
            let v = b.as_variable_definition().unwrap();
            (v.get_context(), v.list.clone())
        };
        let use_cx = node_context.unwrap_or_else(|| cx.clone());
        self.eval(&use_cx, &list)
    }

    fn evaluate_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if !self.first_pass {
            let (r, initializer, typeref, variable, attrs, gpos) = {
                let b = node.borrow();
                let v = b.as_variable_binding().unwrap();
                (v.ref_.clone(), v.initializer.clone(), v.typeref.clone(), v.variable.clone(), v.attrs.clone(), b.get_position())
            };
            let input = Self::input(cx);
            let s = r.as_ref().and_then(|r| r.get_slot_kind(&cx.borrow(), GET_TOKEN));
            if let Some(slot) = &s {
                // Hoisted duplicate definitions, which surprise those used to block scoping.
                if self.slot_is_already_declared(&s) {
                    let orig_def = "";
                    self.warning1(gpos, &input, kWarning_DuplicateVariableDef, orig_def);
                } else {
                    self.slot_mark_as_declared(slot, gpos);
                }
            }

            if initializer.is_none() {
                if s.as_ref().map_or(false, |s| s.is_const()) {
                    self.warning(gpos, &input, kWarning_ConstNotInitialized);
                }
            } else if !self.doing_method {
                let init_only = |cx: &ContextRef, b: bool| {
                    let c = cx.borrow();
                    if c.statics.borrow().es4_nullability {
                        let scope = c.scope().expect("NullPointerException");
                        if scope.builder().map_or(false, |bu| bu.is_instance_builder()) {
                            scope.set_init_only(b);
                        }
                    }
                };
                init_only(cx, true);

                self.eval(cx, &initializer);

                init_only(cx, false);
            }

            let (no_anno, var_type) = {
                let v = rb(&variable);
                let t = v.as_typed_identifier().expect("ClassCastException");
                (t.no_anno, t.type_.clone())
            };
            if typeref.is_none() && r.is_some() && no_anno {
                self.warning2(gpos, &input, kWarning_NoTypeDecl, "variable", &r.as_ref().unwrap().name());
            }

            if let Some(typeref) = &typeref {
                let type_slot = typeref.get_slot(&cx.borrow());
                if type_slot.is_some() {
                    self.check_deprecated_slot(cx, var_type, typeref, &type_slot);
                }
            }

            if let Some(r) = &r {
                let open_namespaces = self.open_namespaces.last().cloned().expect("IndexOutOfBoundsException");
                let open_namespaces = open_namespaces.expect("NullPointerException").to_vec();
                for namespace in &open_namespaces {
                    if namespace.is_package() && r.name() == namespace.name() {
                        self.warning(gpos, &input, kWarning_DefinitionShadowedByPackageName);
                    }
                }
            }
            // check for access specifier
            let (scopes, top) = {
                let c = cx.borrow();
                let n = c.get_scope_depth() as i32;
                (n, c.scope_at(n - 1))
            };

            if scopes == 1 || is_activation_scope(&top) {
                return None;
            }

            if attrs_name_access(&attrs) {
                return None;
            }

            let ns = cx.borrow().scope_at(scopes - 2).get_printable_name() + ":";
            let name = r.as_ref().expect("NullPointerException").name();
            self.warning2(gpos, &input, kWarning_MissingNamespaceDecl, &format!("var '{}'", name), &ns);
        }

        None
    }

    fn evaluate_binary_function_definition(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_function_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, r, attrs, gpos) = {
            let b = node.borrow();
            let f = b.function_definition().unwrap();
            (f.cx.clone().expect("NullPointerException"), f.ref_.clone(), f.def.attrs.clone(), b.get_position())
        };
        let cx = &cx;
        let function_type = || val(cx.borrow().function_type());

        if node.borrow().skip() {
            return function_type();
        }

        let is_interface = {
            let c = cx.borrow();
            let scope = c.scope().expect("NullPointerException");
            scope.type_().map_or(false, |t| t.is_interface())
        };

        if let (false, Some(r), false) = (self.first_pass, &r, is_interface) {
            let (scopes, top) = {
                let c = cx.borrow();
                let n = c.get_scope_depth() as i32;
                (n, c.scope_at(n - 1))
            };

            if scopes == 1 || is_activation_scope(&top) {
                return function_type();
            }

            if attrs_name_access(&attrs) {
                return function_type();
            }
            // Constructors are public whether or not they say so.
            let is_constructor = r.name() == "$construct";
            if is_constructor {
                return function_type();
            }

            let ns = cx.borrow().scope_at(scopes - 2).get_printable_name() + ":";
            let input = Self::input(cx);
            self.warning2(gpos, &input, kWarning_MissingNamespaceDecl, &format!("function '{}'", r.name()), &ns);
        }

        function_type()
    }

    fn evaluate_function_common(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, r, used_namespaces, identifier, fun, is_function_definition, fexprs, signature, body, gpos) = {
            let b = node.borrow();
            let f = b.as_function_common().unwrap();
            (
                f.cx.clone().expect("NullPointerException"),
                f.ref_.clone(),
                f.used_namespaces.clone(),
                f.identifier.clone(),
                f.fun.clone(),
                f.is_function_definition(),
                f.fexprs.clone(),
                f.signature.clone(),
                f.body.clone(),
                b.get_position(),
            )
        };
        let cx = &cx;

        if self.doing_method {
            return val(cx.borrow().function_type());
        }

        self.open_namespaces.push(used_namespaces);
        if let (false, Some(r)) = (self.first_pass, &r) {
            let search = self.unsupported_events_map.get(&r.name()).cloned();
            if let Some(search) = search.filter(|s| !s.is_empty()) {
                let s = r.get_slot_kind(&cx.borrow(), GET_TOKEN);
                let base_type = {
                    let c = cx.borrow();
                    c.scope().and_then(|scope| scope.type_().expect("NullPointerException").get_type_value())
                };

                if base_type.is_some() && s.is_some() && !self.slot_get_registered_for_event(&s) {
                    for (type_, code) in &search.0 {
                        if let Some(type_) = type_ {
                            if type_.includes(&cx.borrow(), base_type.as_ref()) {
                                let pos = if identifier.is_some() { rb(&identifier).get_position() } else { gpos };
                                let input = Self::input(cx);
                                let msg = self.warning_constants_map.get(code).copied();
                                self.warning_opt(pos, &input, kWarning_DepricatedEventHandlerError, msg, Some(""), Some(""));
                            }
                        }
                    }
                }
            }
        }
        let fun = fun.expect("NullPointerException");
        cx.borrow().push_scope(fun.activation().expect("NullPointerException"));
        if !is_function_definition {
            self.this_contexts.push(global_this);
        }

        for item in fexprs.as_ref().expect("NullPointerException").borrow().clone() {
            evaluate(&item, cx, self);
        }

        self.current_function_ref.push(r.clone());

        let r_name = r.as_ref().expect("NullPointerException").name();
        let is_constructor = r_name == "$construct";
        if is_constructor {
            self.expected_return_type = Some(cx.borrow().void_type());
        } else {
            let t = rb(&signature).as_function_signature().unwrap().type_.clone();
            self.expected_return_type = match t {
                Some(t) => t.get_type_value(),
                None => Some(cx.borrow().no_type()),
            };
        }

        self.body_has_return = false;
        if body.is_some() {
            self.eval(cx, &body);
            self.doing_method = false;
        }
        self.expected_return_type = Some(cx.borrow().no_type());

        self.eval(cx, &signature);
        self.current_function_ref.pop();

        // Pop the activation before looking up the slot, or it could bind to a definition inside it.
        cx.borrow().pop_scope();
        let s = r.as_ref().unwrap().get_slot_kind(&cx.borrow(), GET_TOKEN);

        if self.first_pass && s.is_some() {
            if self.body_has_return {
                self.slot_set_has_return_value(s.as_ref().unwrap(), true);
            }
        } else if !self.first_pass {
            let parameter = signature.as_ref().and_then(|s| s.borrow().as_function_signature().unwrap().parameter.clone());
            if let Some(parameter) = parameter {
                let items = parameter.borrow().as_parameter_list().unwrap().items.clone();
                let mut param_names: HashSet<String> = HashSet::new();
                for item in &items {
                    let (name, pos) = {
                        let b = rb(item);
                        let p = b.parameter().expect("ClassCastException");
                        (p.ref_.as_ref().expect("NullPointerException").name(), b.pos())
                    };
                    if param_names.contains(&name) {
                        let input = Self::input(cx);
                        self.warning1(pos, &input, kWarning_DuplicateArgumentNames, &name);
                    } else {
                        param_names.insert(name);
                    }
                }
            }
        }

        self.open_namespaces.pop();
        if !is_function_definition {
            self.this_contexts.pop();
        }

        let function_type = cx.borrow().function_type();
        val(function_type)
    }

    fn evaluate_function_name(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_function_signature(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let r = self.current_function_ref.last().cloned().expect("IndexOutOfBoundsException");

        if let Some(r) = &r {
            // Look the function up outside its activation, so it does not bind to a like-named parameter.
            let c = cx.borrow();
            let old_scope = c.scope().expect("NullPointerException");
            c.pop_scope();
            let _s = r.get_slot_kind(&c, GET_TOKEN);
            c.push_scope(old_scope);
        }

        let (parameter, result, no_anno, gpos) = {
            let b = node.borrow();
            let s = b.as_function_signature().unwrap();
            (s.parameter.clone(), s.result.clone(), s.no_anno, b.get_position())
        };
        if parameter.is_some() {
            self.eval(cx, &parameter);
        }

        if result.is_some() {
            self.eval(cx, &result);
        } else if let (false, Some(r)) = (self.first_pass, &r) {
            if r.name() != "$construct" && no_anno {
                let input = Self::input(cx);
                self.warning2(gpos - 2, &input, kWarning_NoTypeDecl, "return value for function", &r.name());
            }
        }

        None
    }

    fn evaluate_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, type_, no_anno, gpos) = {
            let b = node.borrow();
            let p = b.as_parameter().unwrap();
            (p.ref_.clone(), p.type_.clone(), p.no_anno, b.get_position())
        };
        if let (false, Some(r)) = (self.first_pass, &r) {
            if type_.is_none() && no_anno {
                let input = Self::input(cx);
                self.warning2(gpos, &input, kWarning_NoTypeDecl, "parameter", &r.name());
            }
        }

        None
    }

    fn evaluate_rest_parameter(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_rest_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_parameter_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().as_parameter_list().unwrap().items.clone();
        for item in &items {
            self.eval(cx, item);
        }

        None
    }

    fn evaluate_binary_program(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_binary_class_def(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_binary_interface_definition(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_program(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, used_def_namespaces, pkgdefs, clsdefs, fexprs, statements) = {
            let b = node.borrow();
            let p = b.program().unwrap();
            (
                p.cx.clone().expect("NullPointerException"),
                p.used_def_namespaces.clone(),
                p.pkgdefs.clone(),
                p.clsdefs.clone(),
                p.fexprs.clone(),
                p.statements.clone(),
            )
        };
        let cx = &cx;

        let input = Self::input(cx);
        if input.is_none() || self.ignorable_file(&input) {
            return None;
        }

        if !self.initialized {
            self.initialize(cx);
        }

        // Two walks: the first records per-slot facts (registered as event handlers, has a return value), the
        // second warns.
        for num_pass in 0..2 {
            self.first_pass = num_pass == 0;
            self.this_contexts.push(global_this);

            self.doing_class = false;
            self.doing_method = false;

            self.open_namespaces.push(Some(used_def_namespaces.clone()));
            for def in &pkgdefs {
                evaluate(def, cx, self);
            }

            if clsdefs.is_some() {
                for def in node_list(&clsdefs) {
                    evaluate(&def, cx, self);
                }
            }

            if fexprs.is_some() {
                for def in node_list(&fexprs) {
                    evaluate(&def, cx, self);
                }
            }

            if statements.is_some() {
                self.eval(cx, &statements);
            }
            self.doing_method = false;
            self.doing_class = false;

            self.this_contexts.pop();
            self.open_namespaces.pop();
        }

        None
    }

    fn evaluate_package_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut b = node.borrow_mut();
        let p = b.as_package_definition_mut().unwrap();
        if !p.in_this_pkg {
            p.in_this_pkg = true;
            self.open_namespaces.push(Some(p.used_namespaces.clone()));
        } else {
            p.in_this_pkg = false;
            self.open_namespaces.pop();
        }

        None
    }

    fn evaluate_error(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_to_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_to_object().unwrap().expr.clone();
        self.eval(cx, &expr);
        val(cx.borrow().no_type())
    }

    fn evaluate_box(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, void_result, actual) = {
            let b = node.borrow();
            let x = b.as_box().unwrap();
            (x.expr.clone(), x.void_result, x.actual.clone())
        };
        let mut result = self.eval(cx, &expr);

        if void_result {
            result = val(cx.borrow().void_type());
        } else {
            let actual = actual.expect("NullPointerException");
            let type_id = actual.get_type_id();
            if type_id == TYPE_bool || type_id == TYPE_int {
                result = val(actual);
            }
        }

        result
    }

    fn evaluate_coerce(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, expected) = {
            let b = node.borrow();
            let c = b.as_coerce().unwrap();
            (c.expr.clone(), c.expected.clone())
        };
        // ConstantEvaluator coerces nearly everything untyped to Object, so ignore coercions to *.
        let result = self.eval(cx, &expr);
        let expected = expected.expect("NullPointerException");
        let (no_type, function_type) = {
            let c = cx.borrow();
            (c.no_type(), c.function_type())
        };
        if opt_is(&expected.get_type_value(), &no_type) || is_obj(&result, &self.undefined_literal) {
            return result;
        }
        if is(&result, &function_type) && !opt_is(&expected.get_type_value(), &function_type) {
            if kind_is(&expr, Node::is_member_expression) {
                let func_name = member_ref_name(&expr);
                let input = Self::input(cx);
                let pos = rb(&expr).pos();
                let expected_name = expected.get_name_cx(&cx.borrow()).to_string();
                self.warning2(pos, &input, kWarning_UnlikelyFunctionValue, &expected_name, &func_name);
            }
        }
        result
    }

    fn evaluate_load_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let t = node.borrow().as_load_register().unwrap().type_.clone();
        val(t.unwrap_or_else(|| cx.borrow().no_type()))
    }

    fn evaluate_store_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, t) = {
            let b = node.borrow();
            let s = b.as_store_register().unwrap();
            (s.expr.clone(), s.type_.clone())
        };
        self.eval(cx, &expr);
        val(t.unwrap_or_else(|| cx.borrow().no_type()))
    }

    fn evaluate_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let t = node.borrow().as_register().unwrap().type_.clone();
        val(t.unwrap_or_else(|| cx.borrow().no_type()))
    }

    fn evaluate_has_next(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().no_type())
    }

    fn evaluate_throw_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_throw_statement().unwrap().expr.clone();
        if expr.is_some() {
            return self.eval(cx, &expr);
        }
        None
    }

    fn evaluate_try_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (tryblock, catchlist) = {
            let b = node.borrow();
            let t = b.as_try_statement().unwrap();
            (t.tryblock.clone(), t.catchlist.clone())
        };
        if tryblock.is_some() {
            self.eval(cx, &tryblock);
        }
        if catchlist.is_some() {
            self.eval(cx, &catchlist);
        }
        None
    }

    fn evaluate_catch_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (parameter, statements) = {
            let b = node.borrow();
            let c = b.as_catch_clause().unwrap();
            (c.parameter.clone(), c.statements.clone())
        };
        if parameter.is_some() {
            self.eval(cx, &parameter);
        }

        if statements.is_some() {
            self.eval(cx, &statements);
        }
        None
    }

    fn evaluate_finally_clause(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_class_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, cframe, iframe, used_namespaces, attrs, name, baseclass, interfaces, staticfexprs, clsdefs, statements) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (
                c.cx.clone(),
                c.cframe.clone(),
                c.iframe.clone(),
                c.used_namespaces.clone(),
                c.def.attrs.clone(),
                c.name.clone(),
                c.baseclass.clone(),
                c.interfaces.clone(),
                c.staticfexprs.clone(),
                c.clsdefs.clone(),
                c.statements.clone(),
            )
        };
        let cframe_val = || opt_val(cframe.clone());

        if self.doing_method || self.doing_class || node.borrow().skip() {
            return cframe_val();
        }
        let cx = &cx.expect("NullPointerException");
        self.open_namespaces.push(Some(used_namespaces));
        self.doing_class = true;

        if attrs.is_some() {
            self.eval(cx, &attrs);
        }

        if name.is_some() {
            self.eval(cx, &name);
        }
        if baseclass.is_some() {
            self.eval(cx, &baseclass);
        }
        if interfaces.is_some() {
            self.eval(cx, &interfaces);
        }

        let cframe_tv = cframe.clone().expect("NullPointerException");
        cx.borrow().push_static_class_scopes(&cframe_tv);
        self.this_contexts.push(error_this);

        if staticfexprs.is_some() {
            for item in node_list(&staticfexprs) {
                self.doing_method = false;
                evaluate(&item, cx, self);
            }
        }

        if clsdefs.is_some() {
            for clsdef in node_list(&clsdefs) {
                evaluate(&clsdef, cx, self);
            }
        }

        if statements.is_some() {
            self.eval(cx, &statements);
        }
        self.doing_method = false;

        self.this_contexts.pop();
        self.this_contexts.push(instance_this);
        let iframe = iframe.expect("NullPointerException");
        cx.borrow().push_scope(iframe.clone());

        let mut has_instance_vars = false;

        let instanceinits = node.borrow().class_definition().unwrap().instanceinits.clone();
        if instanceinits.is_some() {
            self.doing_method = true;
            let es4_nullability = cx.borrow().statics.borrow().es4_nullability;
            for item in node_list(&instanceinits) {
                let is_definition = item.borrow().is_definition();
                if es4_nullability && !is_definition {
                    iframe.set_init_only(true);
                }

                evaluate(&item, cx, self);
                // Only instance vars make a missing constructor likely to matter.
                if item.borrow().as_variable_definition().is_some() {
                    has_instance_vars = true;
                }

                if es4_nullability && !is_definition {
                    iframe.set_init_only(false);
                }
            }

            self.doing_method = false;
        }

        let fexprs = node.borrow().class_definition().unwrap().fexprs.clone();
        if fexprs.is_some() {
            let mut found_constructor = false;
            let mut _has_instance_methods = false;
            let (is_interface, class_name) = {
                let b = node.borrow();
                let c = b.class_definition().unwrap();
                (b.is_interface(), c.ref_.as_ref().map(|r| r.name()))
            };
            for fn_ in node_list(&fexprs) {
                let (is_ctor, is_synthetic, fpos) = {
                    let b = fn_.borrow();
                    let f = b.as_function_common().expect("ClassCastException");
                    (f.ref_.as_ref().expect("NullPointerException").name() == "$construct", b.is_synthetic(), b.pos())
                };
                if is_ctor {
                    if !is_synthetic {
                        found_constructor = true;
                        self.body_has_super = false;
                    }
                } else {
                    _has_instance_methods = true;
                }

                evaluate(&fn_, cx, self);
                if is_ctor && !is_interface && !is_synthetic && !self.body_has_super {
                    let input = Self::input(cx);
                    self.warning1(fpos, &input, kWarning_NoExplicitSuperCallInConstructor, &class_name.clone().expect("NullPointerException"));
                    self.body_has_super = false;
                }
            }
            // Classes that only hold statics do not need a constructor.
            if !found_constructor && has_instance_vars && !is_interface {
                let input = Self::input(cx);
                let pos = node.borrow().pos();
                self.warning1(pos, &input, kWarning_NoConstructor, &class_name.expect("NullPointerException"));
            }
        }

        self.doing_method = false;
        self.doing_class = false;

        self.this_contexts.pop();
        cx.borrow().pop_scope();
        cx.borrow().pop_static_class_scopes(Some(&cframe_tv));
        self.open_namespaces.pop();

        let (r, has_pkgdef, gpos) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (c.ref_.clone(), c.def.pkgdef.is_some(), b.get_position())
        };
        if let (false, Some(r), true) = (self.first_pass, &r, has_pkgdef) {
            if attrs_name_access(&attrs) {
                return cframe_val();
            }
            let ns = cframe_tv.name().ns().expect("NullPointerException").name();
            let input = Self::input(cx);
            self.warning2(gpos, &input, kWarning_MissingNamespaceDecl, &format!("class '{}'", r.name()), &ns);
        }

        cframe_val()
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if self.doing_method || self.doing_class {
            return None;
        }

        let implobj = cx.borrow().scope().expect("NullPointerException");
        if !implobj.builder().map_or(false, |b| b.is_instance_builder()) {
            return self.evaluate_class_definition(cx, node);
        }

        None
    }

    fn evaluate_class_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (pkgname, ident) = {
            let b = node.borrow();
            let c = b.as_class_name().unwrap();
            (c.pkgname.clone(), c.ident.clone())
        };
        if pkgname.is_some() {
            self.eval(cx, &pkgname);
        }
        if ident.is_some() {
            self.eval(cx, &ident);
        }
        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_inheritance(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (baseclass, interfaces) = {
            let b = node.borrow();
            let i = b.as_inheritance().unwrap();
            (i.baseclass.clone(), i.interfaces.clone())
        };
        if baseclass.is_some() {
            self.eval(cx, &baseclass);
        }
        if interfaces.is_some() {
            self.eval(cx, &interfaces);
        }
        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().as_attribute_list().unwrap().items.clone();
        for item in &items {
            self.eval(cx, item);
        }

        Some(Value::Object(ObjectValue::undefined_value()))
    }

    fn evaluate_include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (in_this_include, ncx) = {
            let b = node.borrow();
            let i = b.as_include_directive().unwrap();
            (i.in_this_include, i.cx.clone())
        };
        if !in_this_include {
            let prev_cx = Rc::new(RefCell::new(Context::new(cx.borrow().statics.clone())));
            prev_cx.borrow_mut().switch_to_context(&cx.borrow());
            {
                let mut b = node.borrow_mut();
                let i = b.as_include_directive_mut().unwrap();
                i.in_this_include = true;
                i.prev_cx = Some(prev_cx);
            }

            // This switches the context of the enclosing statement list.
            let ncx = ncx.expect("NullPointerException");
            if !Rc::ptr_eq(cx, &ncx) {
                cx.borrow_mut().switch_to_context(&ncx.borrow());
            }
        } else {
            let prev_cx = {
                let mut b = node.borrow_mut();
                let i = b.as_include_directive_mut().unwrap();
                i.in_this_include = false;
                i.prev_cx.take().expect("NullPointerException")
            };
            cx.borrow_mut().switch_to_context(&prev_cx.borrow());
        }

        None
    }

    fn evaluate_import_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_super_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut super_type = cx.borrow().no_type();
        let this_value: Option<TypeValue>;

        let expr = node.borrow().as_super_expression().unwrap().expr.clone();
        if expr.is_some() {
            let result = self.eval(cx, &expr);
            this_value = tv_of(&result);
        } else {
            let c = cx.borrow();
            let scope_depth = c.get_scope_depth() as i32 - 1;
            let result = c.scope_at(scope_depth - 1);
            this_value = result.as_type_value();
        }

        if let Some(b) = this_value.and_then(|t| t.baseclass()) {
            super_type = b;
        }

        val(super_type)
    }

    fn evaluate_super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.body_has_super = true;

        let call = node.borrow().as_super_statement().unwrap().call.clone();
        let args = rb(&call).as_call_expression().expect("ClassCastException").args.clone();
        if args.is_some() {
            self.eval(cx, &args);
        }
        None
    }

    fn evaluate_config_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (r, attrs, gpos) = {
            let b = node.borrow();
            let n = b.namespace_definition().unwrap();
            (n.ref_.clone(), n.def.attrs.clone(), b.get_position())
        };
        if let (false, Some(r)) = (self.first_pass, &r) {
            let (scopes, top) = {
                let c = cx.borrow();
                let n = c.get_scope_depth() as i32;
                (n, c.scope_at(n - 1))
            };
            if scopes != 1 || top.builder().map_or(false, |b| b.is_package_builder()) {
                return None;
            }

            if attrs_name_access(&attrs) {
                return None;
            }
            let ns = cx.borrow().scope_at(scopes - 2).get_printable_name() + ":";
            let input = Self::input(cx);
            self.warning2(gpos, &input, kWarning_MissingNamespaceDecl, &format!("namespace '{}'", r.name()), &ns);
        }

        None
    }

    fn evaluate_use_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_meta_data(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_empty_element(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_default_xml_namespace(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_default_xml_namespace().unwrap().expr.clone();
        if expr.is_some() {
            self.eval(cx, &expr);
        }
        val(cx.borrow().void_type())
    }

    fn evaluate_pragma(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_use_precision(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_use_numeric(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_use_rounding(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_pragma_expression(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_package_name(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_package_identifiers(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_untyped_variable_binding(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_doc_comment(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_import(&mut self, cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        val(cx.borrow().void_type())
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let qualifier = node.borrow().qualified_identifier().unwrap().qualifier.clone();
        if qualifier.is_some() {
            self.eval(cx, &qualifier);
        }
        val(cx.borrow().void_type())
    }

    fn evaluate_qualified_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (qualifier, expr) = {
            let b = node.borrow();
            let q = b.as_qualified_expression().unwrap();
            (q.qid.qualifier.clone(), q.expr.clone())
        };
        if qualifier.is_some() {
            self.eval(cx, &qualifier);
        }
        if expr.is_some() {
            self.eval(cx, &expr);
        }
        val(cx.borrow().void_type())
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().as_type_expression().unwrap().expr.clone();
        self.eval(cx, &expr)
    }
}

/// A statement list holding just an empty statement, as in "if (x);".
fn single_empty_statement(n: &Option<NodeRef>) -> bool {
    let b = rb(n);
    b.as_statement_list().map_or(false, |l| {
        l.items.len() == 1 && l.items[0].as_ref().map_or(false, |i| i.borrow().as_empty_statement().is_some())
    })
}
