//! Port of Velocity's introspection (`UberspectImpl`, `Introspector`, `ClassMap`, `MethodMap`, `PropertyExecutor`,
//! `GetExecutor`, `BooleanPropertyExecutor`) over a model of the Java values templates see.
//!
//! Compiler objects reach templates as [`Value::Object`] holding a [`JavaObject`]: it names its Java class, lists the public
//! methods `Class.getMethods()` would give it (name, parameter types, return type; `java.lang.Object`'s own methods are added by
//! the engine) and invokes one. Velocity then resolves `$o.foo` exactly as Java does: `getfoo()`, `getFoo()` (case of the 4th
//! character flipped), `get("foo")`, then `isFoo()` returning primitive `boolean`; `$o.m(a, b)` picks the most specific
//! applicable method of that name and arity for the arguments' runtime classes (null fits any reference parameter); a `void`
//! method call yields `""`, a property yields `null`; a thrown exception is an `Err`. Strings, numbers, booleans, lists, maps,
//! entries and iterators are built-in values with `java.lang`/`java.util` behaviour, and every value renders with its Java
//! `toString()`.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::runtime::RuntimeInstance;
use crate::asc::util::jstr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JType {
    Void,
    Boolean,
    Int,
    Long,
    Double,
    Object,
    String,
    /// Any other reference type, by binary name (`java.util.List`, `flex2.compiler.mxml.rep.Model`).
    Class(Cow<'static, str>),
}

impl JType {
    pub fn is_primitive(&self) -> bool {
        matches!(self, JType::Void | JType::Boolean | JType::Int | JType::Long | JType::Double)
    }

    // ClassMap.makeMethodKey's spelling of a parameter type.
    fn key_name(&self) -> &str {
        match self {
            JType::Void => "void",
            JType::Boolean => "java.lang.Boolean",
            JType::Int => "java.lang.Integer",
            JType::Long => "java.lang.Long",
            JType::Double => "java.lang.Double",
            JType::Object => "java.lang.Object",
            JType::String => "java.lang.String",
            JType::Class(c) => c,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Method {
    pub name: Cow<'static, str>,
    pub params: Cow<'static, [JType]>,
    pub ret: JType,
}

impl Method {
    pub const fn new(name: &'static str, params: &'static [JType], ret: JType) -> Method {
        Method { name: Cow::Borrowed(name), params: Cow::Borrowed(params), ret }
    }

    fn key(&self) -> String {
        let mut k = self.name.to_string();
        for p in self.params.iter() {
            k.push_str(p.key_name());
        }
        k
    }
}

/// A Java exception thrown by an invoked method (Velocity sees it as the `InvocationTargetException`'s target).
#[derive(Clone, Debug)]
pub struct JavaException {
    pub class_name: String,
    pub message: Option<String>,
}

impl JavaException {
    pub fn new(class_name: &str, message: Option<&str>) -> JavaException {
        JavaException { class_name: class_name.to_string(), message: message.map(|m| m.to_string()) }
    }

    /// `Throwable.toString()`.
    pub fn to_java_string(&self) -> String {
        match &self.message {
            Some(m) => format!("{}: {}", self.class_name, m),
            None => self.class_name.clone(),
        }
    }
}

/// A Java object exposed to templates; see the module documentation.
pub trait JavaObject {
    /// `getClass().getName()`.
    fn class_name(&self) -> &str;
    /// `Class.isAssignableFrom` for a parameter of reference type `class_name`.
    fn instance_of(&self, class_name: &str) -> bool {
        class_name == self.class_name()
    }
    fn methods(&self) -> &[Method];
    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException>;
    /// `toString()`.
    fn to_java_string(&self) -> String {
        format!("{}@{:x}", self.class_name(), self as *const Self as *const u8 as usize)
    }
    /// `equals(Object)`.
    fn java_equals(&self, other: &Value) -> bool {
        match other {
            Value::Object(o) => std::ptr::addr_eq(Rc::as_ptr(o), self as *const Self),
            _ => false,
        }
    }
    fn java_hash_code(&self) -> i32 {
        (self as *const Self as *const u8 as usize) as i32
    }
    /// The elements, in iteration order, when the class implements `java.util.Collection`.
    fn as_collection(&self) -> Option<Vec<Value>> {
        None
    }
}

pub struct JList {
    pub class_name: Cow<'static, str>,
    pub items: RefCell<Vec<Value>>,
}

pub struct JMap {
    pub class_name: Cow<'static, str>,
    /// In the Java map's iteration order.
    pub entries: RefCell<Vec<(Value, Value)>>,
}

pub struct JEntry {
    pub key: Value,
    pub value: Value,
}

pub struct JIterator {
    items: Vec<Value>,
    pos: Cell<usize>,
}

impl JIterator {
    fn has_next(&self) -> bool {
        self.pos.get() < self.items.len()
    }

    fn next(&self) -> Result<Value, JavaException> {
        let p = self.pos.get();
        match self.items.get(p) {
            Some(v) => {
                self.pos.set(p + 1);
                Ok(v.clone())
            }
            None => Err(JavaException::new("java.util.NoSuchElementException", None)),
        }
    }
}

#[derive(Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i32),
    Long(i64),
    String(Rc<str>),
    List(Rc<JList>),
    Map(Rc<JMap>),
    Entry(Rc<JEntry>),
    Iterator(Rc<JIterator>),
    Class(Rc<str>),
    Object(Rc<dyn JavaObject>),
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::String(Rc::from(s))
    }

    /// A `java.util.ArrayList`.
    pub fn list(items: Vec<Value>) -> Value {
        Value::list_of("java.util.ArrayList", items)
    }

    pub fn list_of(class_name: &str, items: Vec<Value>) -> Value {
        Value::List(Rc::new(JList { class_name: Cow::Owned(class_name.to_string()), items: RefCell::new(items) }))
    }

    /// A map of class `class_name` whose entries iterate in the given order.
    pub fn map_of(class_name: &str, entries: Vec<(Value, Value)>) -> Value {
        Value::Map(Rc::new(JMap { class_name: Cow::Owned(class_name.to_string()), entries: RefCell::new(entries) }))
    }

    /// An `Iterator` over `items` (`list.iterator()`).
    pub fn iterator(items: Vec<Value>) -> Value {
        Value::Iterator(Rc::new(JIterator { items, pos: Cell::new(0) }))
    }

    pub fn object(o: Rc<dyn JavaObject>) -> Value {
        Value::Object(o)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// `getClass().getName()`; `None` for null.
    pub fn class_name(&self) -> Option<String> {
        Some(match self {
            Value::Null => return None,
            Value::Boolean(_) => "java.lang.Boolean".to_string(),
            Value::Integer(_) => "java.lang.Integer".to_string(),
            Value::Long(_) => "java.lang.Long".to_string(),
            Value::String(_) => "java.lang.String".to_string(),
            Value::List(l) => l.class_name.to_string(),
            Value::Map(m) => m.class_name.to_string(),
            Value::Entry(_) => "java.util.HashMap$Node".to_string(),
            Value::Iterator(_) => "java.util.ArrayList$Itr".to_string(),
            Value::Class(_) => "java.lang.Class".to_string(),
            Value::Object(o) => o.class_name().to_string(),
        })
    }

    /// `String.valueOf(this)`.
    pub fn to_java_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Long(l) => l.to_string(),
            Value::String(s) => s.to_string(),
            Value::List(l) => {
                let parts: Vec<String> = l.items.borrow().iter().map(|v| v.to_java_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Map(m) => {
                let parts: Vec<String> = m
                    .entries
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{}={}", k.to_java_string(), v.to_java_string()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Entry(e) => format!("{}={}", e.key.to_java_string(), e.value.to_java_string()),
            Value::Iterator(i) => format!("java.util.ArrayList$Itr@{:x}", Rc::as_ptr(i) as usize),
            Value::Class(c) => format!("class {c}"),
            Value::Object(o) => o.to_java_string(),
        }
    }

    /// `this.equals(other)` for a non-null receiver.
    pub fn java_equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Long(a), Value::Long(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::List(a), Value::List(b)) => {
                let (a, b) = (a.items.borrow(), b.items.borrow());
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| nullable_equals(x, y))
            }
            (Value::Map(a), Value::Map(b)) => {
                let (a, b) = (a.entries.borrow(), b.entries.borrow());
                a.len() == b.len()
                    && a.iter().all(|(k, v)| b.iter().any(|(k2, v2)| nullable_equals(k, k2) && nullable_equals(v, v2)))
            }
            (Value::Entry(a), Value::Entry(b)) => {
                nullable_equals(&a.key, &b.key) && nullable_equals(&a.value, &b.value)
            }
            (Value::Iterator(a), Value::Iterator(b)) => Rc::ptr_eq(a, b),
            (Value::Class(a), Value::Class(b)) => a == b,
            (Value::Object(o), _) => o.java_equals(other),
            _ => false,
        }
    }

    pub fn java_hash_code(&self) -> i32 {
        match self {
            Value::Null => 0,
            Value::Boolean(b) => {
                if *b {
                    1231
                } else {
                    1237
                }
            }
            Value::Integer(i) => *i,
            Value::Long(l) => (*l ^ (*l >> 32)) as i32,
            Value::String(s) => jstr::java_hash_code(s),
            Value::List(l) => {
                l.items.borrow().iter().fold(1i32, |h, v| h.wrapping_mul(31).wrapping_add(v.java_hash_code()))
            }
            Value::Map(m) => m
                .entries
                .borrow()
                .iter()
                .fold(0i32, |h, (k, v)| h.wrapping_add(k.java_hash_code() ^ v.java_hash_code())),
            Value::Entry(e) => e.key.java_hash_code() ^ e.value.java_hash_code(),
            Value::Iterator(i) => Rc::as_ptr(i) as usize as i32,
            Value::Class(c) => jstr::java_hash_code(c),
            Value::Object(o) => o.java_hash_code(),
        }
    }
}

fn nullable_equals(a: &Value, b: &Value) -> bool {
    if a.is_null() {
        b.is_null()
    } else {
        a.java_equals(b)
    }
}

// ---- built-in classes

use JType::{Boolean as Z, Int as I, Long as J, Object as O, String as S};

const fn c(n: &'static str) -> JType {
    JType::Class(Cow::Borrowed(n))
}

static CHAR_SEQUENCE: [JType; 1] = [c("java.lang.CharSequence")];
static INTEGER: [JType; 1] = [c("java.lang.Integer")];
static LONG: [JType; 1] = [c("java.lang.Long")];

static OBJECT_METHODS: &[Method] = &[
    Method::new("getClass", &[], c("java.lang.Class")),
    Method::new("hashCode", &[], I),
    Method::new("equals", &[O], Z),
    Method::new("toString", &[], S),
];

static STRING_METHODS: &[Method] = &[
    Method::new("length", &[], I),
    Method::new("isEmpty", &[], Z),
    Method::new("equalsIgnoreCase", &[S], Z),
    Method::new("compareTo", &[S], I),
    Method::new("indexOf", &[S], I),
    Method::new("indexOf", &[I], I),
    Method::new("indexOf", &[S, I], I),
    Method::new("lastIndexOf", &[S], I),
    Method::new("startsWith", &[S], Z),
    Method::new("endsWith", &[S], Z),
    Method::new("contains", &CHAR_SEQUENCE, Z),
    Method::new("substring", &[I], S),
    Method::new("substring", &[I, I], S),
    Method::new("concat", &[S], S),
    Method::new("toUpperCase", &[], S),
    Method::new("toLowerCase", &[], S),
    Method::new("trim", &[], S),
];

static INTEGER_METHODS: &[Method] =
    &[Method::new("intValue", &[], I), Method::new("longValue", &[], J), Method::new("compareTo", &INTEGER, I)];

static LONG_METHODS: &[Method] =
    &[Method::new("intValue", &[], I), Method::new("longValue", &[], J), Method::new("compareTo", &LONG, I)];

static BOOLEAN_METHODS: &[Method] = &[Method::new("booleanValue", &[], Z)];

static LIST_METHODS: &[Method] = &[
    Method::new("size", &[], I),
    Method::new("isEmpty", &[], Z),
    Method::new("get", &[I], O),
    Method::new("contains", &[O], Z),
    Method::new("indexOf", &[O], I),
    Method::new("add", &[O], Z),
    Method::new("iterator", &[], c("java.util.Iterator")),
];

static MAP_METHODS: &[Method] = &[
    Method::new("size", &[], I),
    Method::new("isEmpty", &[], Z),
    Method::new("get", &[O], O),
    Method::new("containsKey", &[O], Z),
    Method::new("containsValue", &[O], Z),
    Method::new("put", &[O, O], O),
    Method::new("keySet", &[], c("java.util.Set")),
    Method::new("values", &[], c("java.util.Collection")),
    Method::new("entrySet", &[], c("java.util.Set")),
];

static ENTRY_METHODS: &[Method] = &[Method::new("getKey", &[], O), Method::new("getValue", &[], O)];

static ITERATOR_METHODS: &[Method] = &[Method::new("hasNext", &[], Z), Method::new("next", &[], O)];

static CLASS_METHODS: &[Method] = &[Method::new("getName", &[], S)];

/// The class's public methods, as `ClassMap` collects them.
pub fn methods_of(v: &Value) -> Vec<Method> {
    let own: &[Method] = match v {
        Value::Null => return Vec::new(),
        Value::Boolean(_) => BOOLEAN_METHODS,
        Value::Integer(_) => INTEGER_METHODS,
        Value::Long(_) => LONG_METHODS,
        Value::String(_) => STRING_METHODS,
        Value::List(_) => LIST_METHODS,
        Value::Map(_) => MAP_METHODS,
        Value::Entry(_) => ENTRY_METHODS,
        Value::Iterator(_) => ITERATOR_METHODS,
        Value::Class(_) => CLASS_METHODS,
        Value::Object(o) => o.methods(),
    };
    let mut all: Vec<Method> = own.to_vec();
    for m in OBJECT_METHODS {
        if !all.iter().any(|o| o.name == m.name && o.params == m.params) {
            all.push(m.clone());
        }
    }
    all
}

fn arg_int(v: &Value) -> i32 {
    match v {
        Value::Integer(i) => *i,
        _ => 0,
    }
}

fn arg_str(v: &Value) -> Result<Rc<str>, JavaException> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Err(JavaException::new("java.lang.NullPointerException", None)),
        other => Ok(Rc::from(other.to_java_string().as_str())),
    }
}

fn index_out_of_bounds(index: i32, size: usize) -> JavaException {
    JavaException::new(
        "java.lang.IndexOutOfBoundsException",
        Some(&format!("Index {index} out of bounds for length {size}")),
    )
}

fn view(map: &JMap, suffix: &str, items: Vec<Value>) -> Value {
    Value::list_of(&format!("{}{}", map.class_name, suffix), items)
}

fn invoke_builtin(obj: &Value, m: &Method, args: &[Value]) -> Result<Value, JavaException> {
    let name: &str = &m.name;
    if let Value::Object(o) = obj {
        if o.methods().iter().any(|x| x.name == m.name && x.params == m.params) {
            return o.invoke(m, args);
        }
    }
    match (name, args.len()) {
        ("getClass", 0) => return Ok(Value::Class(Rc::from(obj.class_name().unwrap_or_default().as_str()))),
        ("hashCode", 0) => return Ok(Value::Integer(obj.java_hash_code())),
        ("equals", 1) => return Ok(Value::Boolean(obj.java_equals(&args[0]))),
        ("toString", 0) => return Ok(Value::str(&obj.to_java_string())),
        _ => {}
    }
    match obj {
        Value::Object(_) => Ok(Value::Null),
        Value::String(s) => {
            let s: &str = s;
            Ok(match (name, args.len()) {
                ("length", 0) => Value::Integer(jstr::utf16_len(s)),
                ("isEmpty", 0) => Value::Boolean(s.is_empty()),
                ("equalsIgnoreCase", 1) => Value::Boolean(match &args[0] {
                    Value::String(o) => s.to_lowercase() == o.to_lowercase(),
                    _ => false,
                }),
                ("compareTo", 1) => Value::Integer(jstr::compare_to(s, &arg_str(&args[0])?)),
                ("indexOf", 1) => match &args[0] {
                    Value::Integer(c) => Value::Integer(jstr::index_of_unit(s, *c as u16, 0)),
                    a => Value::Integer(jstr::index_of(s, &arg_str(a)?, 0)),
                },
                ("indexOf", 2) => Value::Integer(jstr::index_of(s, &arg_str(&args[0])?, arg_int(&args[1]))),
                ("lastIndexOf", 1) => Value::Integer(jstr::last_index_of(s, &arg_str(&args[0])?)),
                ("startsWith", 1) => Value::Boolean(s.starts_with(&*arg_str(&args[0])?)),
                ("endsWith", 1) => Value::Boolean(s.ends_with(&*arg_str(&args[0])?)),
                ("contains", 1) => Value::Boolean(s.contains(&*arg_str(&args[0])?)),
                ("substring", 1) | ("substring", 2) => {
                    let len = jstr::utf16_len(s);
                    let b = arg_int(&args[0]);
                    let e = if args.len() == 2 { arg_int(&args[1]) } else { len };
                    if b < 0 || e > len || b > e {
                        return Err(JavaException::new(
                            "java.lang.StringIndexOutOfBoundsException",
                            Some(&format!(
                                "String index out of range: {}",
                                if b < 0 {
                                    b
                                } else if e > len {
                                    e
                                } else {
                                    e - b
                                }
                            )),
                        ));
                    }
                    Value::str(&jstr::substring(s, b, e))
                }
                ("concat", 1) => Value::str(&jstr::concat(s, &arg_str(&args[0])?)),
                ("toUpperCase", 0) => Value::str(&s.to_uppercase()),
                ("toLowerCase", 0) => Value::str(&s.to_lowercase()),
                ("trim", 0) => Value::str(s.trim_matches(|c: char| c <= ' ')),
                _ => Value::Null,
            })
        }
        Value::Integer(i) => Ok(match name {
            "intValue" => Value::Integer(*i),
            "longValue" => Value::Long(*i as i64),
            "compareTo" => Value::Integer((*i).cmp(&arg_int(&args[0])) as i32),
            _ => Value::Null,
        }),
        Value::Long(l) => Ok(match name {
            "intValue" => Value::Integer(*l as i32),
            "longValue" => Value::Long(*l),
            "compareTo" => Value::Integer(match &args[0] {
                Value::Long(o) => l.cmp(o) as i32,
                _ => 0,
            }),
            _ => Value::Null,
        }),
        Value::Boolean(b) => Ok(Value::Boolean(*b)),
        Value::List(l) => Ok(match name {
            "size" => Value::Integer(l.items.borrow().len() as i32),
            "isEmpty" => Value::Boolean(l.items.borrow().is_empty()),
            "get" => {
                let i = arg_int(&args[0]);
                let items = l.items.borrow();
                if i < 0 || i as usize >= items.len() {
                    return Err(index_out_of_bounds(i, items.len()));
                }
                items[i as usize].clone()
            }
            "contains" => Value::Boolean(l.items.borrow().iter().any(|v| nullable_equals(&args[0], v))),
            "indexOf" => Value::Integer(
                l.items.borrow().iter().position(|v| nullable_equals(&args[0], v)).map(|p| p as i32).unwrap_or(-1),
            ),
            "add" => {
                l.items.borrow_mut().push(args[0].clone());
                Value::Boolean(true)
            }
            "iterator" => Value::iterator(l.items.borrow().clone()),
            _ => Value::Null,
        }),
        Value::Map(mp) => Ok(match name {
            "size" => Value::Integer(mp.entries.borrow().len() as i32),
            "isEmpty" => Value::Boolean(mp.entries.borrow().is_empty()),
            "get" => mp
                .entries
                .borrow()
                .iter()
                .find(|(k, _)| nullable_equals(&args[0], k))
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Null),
            "containsKey" => Value::Boolean(mp.entries.borrow().iter().any(|(k, _)| nullable_equals(&args[0], k))),
            "containsValue" => Value::Boolean(mp.entries.borrow().iter().any(|(_, v)| nullable_equals(&args[0], v))),
            "put" => {
                let mut entries = mp.entries.borrow_mut();
                match entries.iter_mut().find(|(k, _)| nullable_equals(&args[0], k)) {
                    Some(e) => std::mem::replace(&mut e.1, args[1].clone()),
                    None => {
                        entries.push((args[0].clone(), args[1].clone()));
                        Value::Null
                    }
                }
            }
            "keySet" => view(mp, "$KeySet", mp.entries.borrow().iter().map(|(k, _)| k.clone()).collect()),
            "values" => view(mp, "$Values", mp.entries.borrow().iter().map(|(_, v)| v.clone()).collect()),
            "entrySet" => view(
                mp,
                "$EntrySet",
                mp.entries
                    .borrow()
                    .iter()
                    .map(|(k, v)| Value::Entry(Rc::new(JEntry { key: k.clone(), value: v.clone() })))
                    .collect(),
            ),
            _ => Value::Null,
        }),
        Value::Entry(e) => Ok(match name {
            "getKey" => e.key.clone(),
            "getValue" => e.value.clone(),
            _ => Value::Null,
        }),
        Value::Iterator(it) => match name {
            "hasNext" => Ok(Value::Boolean(it.has_next())),
            "next" => it.next(),
            _ => Ok(Value::Null),
        },
        Value::Class(cn) => Ok(match name {
            "getName" => Value::String(cn.clone()),
            _ => Value::Null,
        }),
        Value::Null => Ok(Value::Null),
    }
}

// ---- assignability (Class.isAssignableFrom and MethodMap's conversions)

fn builtin_supertypes(v: &Value) -> &'static [&'static str] {
    match v {
        Value::Boolean(_) => &["java.lang.Boolean", "java.lang.Comparable", "java.io.Serializable"],
        Value::Integer(_) => &["java.lang.Integer", "java.lang.Number", "java.lang.Comparable", "java.io.Serializable"],
        Value::Long(_) => &["java.lang.Long", "java.lang.Number", "java.lang.Comparable", "java.io.Serializable"],
        Value::String(_) => {
            &["java.lang.String", "java.lang.CharSequence", "java.lang.Comparable", "java.io.Serializable"]
        }
        Value::List(_) => &[
            "java.util.ArrayList",
            "java.util.AbstractList",
            "java.util.AbstractCollection",
            "java.util.List",
            "java.util.Set",
            "java.util.Collection",
            "java.lang.Iterable",
            "java.util.RandomAccess",
            "java.lang.Cloneable",
            "java.io.Serializable",
        ],
        Value::Map(_) => &[
            "java.util.HashMap",
            "java.util.AbstractMap",
            "java.util.Map",
            "java.lang.Cloneable",
            "java.io.Serializable",
        ],
        Value::Entry(_) => &["java.util.Map$Entry"],
        Value::Iterator(_) => &["java.util.Iterator"],
        Value::Class(_) => &["java.lang.Class", "java.io.Serializable"],
        Value::Null | Value::Object(_) => &[],
    }
}

fn is_instance(class: &str, v: &Value) -> bool {
    if class == "java.lang.Object" {
        return true;
    }
    match v {
        Value::Object(o) => o.instance_of(class),
        Value::List(l) if l.class_name == class => true,
        Value::Map(m) if m.class_name == class => true,
        _ => builtin_supertypes(v).contains(&class),
    }
}

/// `MethodMap.isMethodInvocationConvertible(formal, actual.getClass())`.
fn is_method_invocation_convertible(formal: &JType, actual: &Value) -> bool {
    match (formal, actual) {
        (f, Value::Null) => !f.is_primitive(),
        (JType::Object, _) => true,
        (JType::String, a) => matches!(a, Value::String(_)),
        (JType::Class(c), a) => is_instance(c, a),
        (JType::Boolean, Value::Boolean(_)) => true,
        (JType::Int, Value::Integer(_)) => true,
        (JType::Long, Value::Long(_) | Value::Integer(_)) => true,
        (JType::Double, Value::Long(_) | Value::Integer(_)) => true,
        _ => false,
    }
}

const TYPE_SUPERS: &[(&str, &[&str])] = &[
    ("java.lang.String", &["java.lang.CharSequence", "java.lang.Comparable", "java.io.Serializable"]),
    ("java.lang.Integer", &["java.lang.Number", "java.lang.Comparable", "java.io.Serializable"]),
    ("java.lang.Long", &["java.lang.Number", "java.lang.Comparable", "java.io.Serializable"]),
    ("java.lang.Boolean", &["java.lang.Comparable", "java.io.Serializable"]),
    ("java.util.List", &["java.util.Collection", "java.lang.Iterable"]),
    ("java.util.Set", &["java.util.Collection", "java.lang.Iterable"]),
    ("java.util.Collection", &["java.lang.Iterable"]),
    (
        "java.util.ArrayList",
        &["java.util.List", "java.util.Collection", "java.lang.Iterable", "java.util.AbstractList"],
    ),
    ("java.util.HashMap", &["java.util.Map", "java.util.AbstractMap"]),
];

/// `formal.isAssignableFrom(actual)` between declared types (java.* hierarchy only; other classes by name).
fn is_assignable_type(formal: &JType, actual: &JType) -> bool {
    if formal == actual {
        return true;
    }
    if formal.is_primitive() || actual.is_primitive() {
        return false;
    }
    let f = formal.key_name();
    if f == "java.lang.Object" {
        return true;
    }
    let a = actual.key_name();
    TYPE_SUPERS.iter().any(|(t, s)| *t == a && s.contains(&f))
}

/// `MethodMap.isStrictMethodInvocationConvertible`.
fn is_strict_convertible(formal: &JType, actual: &JType) -> bool {
    if is_assignable_type(formal, actual) {
        return true;
    }
    matches!((formal, actual), (JType::Long, JType::Int) | (JType::Double, JType::Int) | (JType::Double, JType::Long))
}

const MORE_SPECIFIC: i32 = 0;
const LESS_SPECIFIC: i32 = 1;
const INCOMPARABLE: i32 = 2;

fn more_specific(c1: &[JType], c2: &[JType]) -> i32 {
    let mut c1_more = false;
    let mut c2_more = false;
    for i in 0..c1.len() {
        if c1[i] != c2[i] {
            c1_more = c1_more || is_strict_convertible(&c2[i], &c1[i]);
            c2_more = c2_more || is_strict_convertible(&c1[i], &c2[i]);
        }
    }
    if c1_more {
        if c2_more {
            return INCOMPARABLE;
        }
        return MORE_SPECIFIC;
    }
    if c2_more {
        return LESS_SPECIFIC;
    }
    INCOMPARABLE
}

struct Ambiguous;

/// `MethodMap.getMostSpecific`.
fn get_most_specific(methods: &[Method], args: &[Value]) -> Result<Option<Method>, Ambiguous> {
    let applicables: Vec<&Method> = methods
        .iter()
        .filter(|m| {
            m.params.len() == args.len()
                && m.params.iter().zip(args).all(|(f, a)| is_method_invocation_convertible(f, a))
        })
        .collect();
    if applicables.is_empty() {
        return Ok(None);
    }
    if applicables.len() == 1 {
        return Ok(Some(applicables[0].clone()));
    }
    let mut maximals: Vec<&Method> = Vec::new();
    for app in applicables {
        let mut less_specific = false;
        let mut i = 0;
        while !less_specific && i < maximals.len() {
            match more_specific(&app.params, &maximals[i].params) {
                MORE_SPECIFIC => {
                    maximals.remove(i);
                    continue;
                }
                LESS_SPECIFIC => less_specific = true,
                _ => {}
            }
            i += 1;
        }
        if !less_specific {
            maximals.push(app);
        }
    }
    if maximals.len() > 1 {
        return Err(Ambiguous);
    }
    Ok(Some(maximals[0].clone()))
}

/// `Introspector.getMethod(obj.getClass(), name, params)`: `ClassMap.findMethod` (exact-signature cache hit first) then
/// `MethodMap.find`.
pub fn get_method(rt: &RuntimeInstance, obj: &Value, name: &str, params: &[Value]) -> Option<Method> {
    let methods = methods_of(obj);
    let mut key = name.to_string();
    for p in params {
        key.push_str(&p.class_name().unwrap_or_else(|| "java.lang.Object".to_string()));
    }
    if let Some(m) = methods.iter().rev().find(|m| m.key() == key) {
        return Some(m.clone());
    }
    let named: Vec<Method> = methods.into_iter().filter(|m| m.name == name).collect();
    if named.is_empty() {
        return None;
    }
    match get_most_specific(&named, params) {
        Ok(m) => m,
        Err(Ambiguous) => {
            let mut msg = format!("Introspection Error : Ambiguous method invocation {name}( ");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    msg.push_str(", ");
                }
                msg.push_str(&p.class_name().unwrap_or_default());
            }
            msg.push_str(&format!(") for class class {}", obj.class_name().unwrap_or_default()));
            rt.error(&msg);
            None
        }
    }
}

/// Why `Method.invoke` failed.
pub enum InvokeError {
    /// `InvocationTargetException`.
    Target(JavaException),
    /// `IllegalArgumentException`, by its `toString()`.
    IllegalArgument(&'static str),
}

/// `Method.invoke(obj, args)`.
pub fn invoke(obj: &Value, m: &Method, args: &[Value]) -> Result<Value, InvokeError> {
    if m.params.len() != args.len() {
        return Err(InvokeError::IllegalArgument("java.lang.IllegalArgumentException: wrong number of arguments"));
    }
    if !m.params.iter().zip(args).all(|(f, a)| is_method_invocation_convertible(f, a)) {
        return Err(InvokeError::IllegalArgument("java.lang.IllegalArgumentException: argument type mismatch"));
    }
    invoke_builtin(obj, m, args).map_err(InvokeError::Target)
}

#[derive(Clone)]
enum ExecutorKind {
    Property,
    Get(String),
    Boolean,
}

/// `VelGetterImpl` over a property executor; `method` is `None` when no executor matched (StrictUberspectImpl's case).
#[derive(Clone)]
pub struct VelPropertyGet {
    kind: ExecutorKind,
    pub method: Option<Method>,
}

impl VelPropertyGet {
    pub fn invoke(&self, o: &Value) -> Result<Value, InvokeError> {
        match &self.method {
            None => Ok(Value::Null),
            Some(m) => match &self.kind {
                ExecutorKind::Get(key) => invoke(o, m, &[Value::str(key)]),
                _ => invoke(o, m, &[]),
            },
        }
    }

    pub fn method_name(&self) -> Option<String> {
        self.method.as_ref().map(|m| m.name.to_string())
    }
}

fn flip_case_at(s: &str, i: i32) -> String {
    let mut u = jstr::to_utf16(s);
    let c = u[i as usize];
    let ch = char::from_u32(c as u32).unwrap_or('\0');
    let flipped = if ch.is_lowercase() { ch.to_uppercase().next() } else { ch.to_lowercase().next() };
    if let Some(f) = flipped {
        if (f as u32) < 0x10000 {
            u[i as usize] = f as u16;
        }
    }
    jstr::from_utf16(&u)
}

/// `UberspectImpl.getPropertyGet`: PropertyExecutor, then GetExecutor, then BooleanPropertyExecutor.
pub fn get_property_get(rt: &RuntimeInstance, obj: &Value, identifier: &str) -> VelPropertyGet {
    let mut name = format!("get{identifier}");
    if let Some(m) = get_method(rt, obj, &name, &[]) {
        return VelPropertyGet { kind: ExecutorKind::Property, method: Some(m) };
    }
    if jstr::utf16_len(&name) > 3 {
        name = flip_case_at(&name, 3);
        if let Some(m) = get_method(rt, obj, &name, &[]) {
            return VelPropertyGet { kind: ExecutorKind::Property, method: Some(m) };
        }
    }
    if let Some(m) = get_method(rt, obj, "get", &[Value::str(identifier)]) {
        return VelPropertyGet { kind: ExecutorKind::Get(identifier.to_string()), method: Some(m) };
    }
    let mut is = format!("is{identifier}");
    if jstr::utf16_len(&is) > 2 {
        let c = jstr::char_at(&is, 2);
        if char::from_u32(c as u32).is_some_and(|ch| ch.is_lowercase()) {
            is = flip_case_at(&is, 2);
        }
        if let Some(m) = get_method(rt, obj, &is, &[]) {
            if m.ret == JType::Boolean {
                return VelPropertyGet { kind: ExecutorKind::Boolean, method: Some(m) };
            }
        }
    }
    VelPropertyGet { kind: ExecutorKind::Boolean, method: None }
}

/// `VelSetterImpl`.
pub struct VelPropertySet {
    method: Method,
    put_key: Option<String>,
}

impl VelPropertySet {
    pub fn invoke(&self, o: &Value, value: Value) -> Result<Value, InvokeError> {
        match &self.put_key {
            Some(k) => invoke(o, &self.method, &[Value::str(k), value]),
            None => invoke(o, &self.method, &[value]),
        }
    }
}

/// `UberspectImpl.getPropertySet`; a null identifier concatenates as `"null"`, as Java's `"set" + identifier` does.
pub fn get_property_set(
    rt: &RuntimeInstance,
    obj: &Value,
    identifier: Option<&str>,
    arg: &Value,
) -> Option<VelPropertySet> {
    let id = identifier.unwrap_or("null");
    let params = [arg.clone()];
    let name = format!("set{id}");
    if let Some(m) = get_method(rt, obj, &name, &params) {
        return Some(VelPropertySet { method: m, put_key: None });
    }
    if jstr::utf16_len(&name) > 3 {
        if let Some(m) = get_method(rt, obj, &flip_case_at(&name, 3), &params) {
            return Some(VelPropertySet { method: m, put_key: None });
        }
    }
    if is_instance("java.util.Map", obj) {
        let probe = [Value::Object(Rc::new(PlainObject)), Value::Object(Rc::new(PlainObject))];
        if let Some(m) = get_method(rt, obj, "put", &probe) {
            return Some(VelPropertySet { method: m, put_key: identifier.map(|s| s.to_string()) });
        }
    }
    None
}

/// `new Object()`, as getPropertySet probes `put(Object, Object)` with.
struct PlainObject;

impl JavaObject for PlainObject {
    fn class_name(&self) -> &str {
        "java.lang.Object"
    }

    fn methods(&self) -> &[Method] {
        &[]
    }

    fn invoke(&self, _: &Method, _: &[Value]) -> Result<Value, JavaException> {
        Ok(Value::Null)
    }
}

/// The iteration `UberspectImpl.getIterator` yields for `#foreach`.
pub enum ForeachIter {
    Snapshot(Vec<Value>),
    Live(Rc<JIterator>),
}

impl ForeachIter {
    pub fn next(&mut self, pos: &mut usize) -> Option<Value> {
        match self {
            ForeachIter::Snapshot(v) => {
                let r = v.get(*pos).cloned();
                *pos += 1;
                r
            }
            ForeachIter::Live(it) => {
                if it.has_next() {
                    it.next().ok()
                } else {
                    None
                }
            }
        }
    }
}

/// `UberspectImpl.getIterator`; `info` is (template, line, column) for the warnings.
pub fn get_iterator(rt: &RuntimeInstance, obj: &Value, info: &Info) -> Option<ForeachIter> {
    let warn_reset = |what: &str| {
        rt.warn(&format!(
            "Warning! The iterative  is an {} in the #foreach() loop at [{},{}] in template {}. Because it's not resetable, if used in more than once, this may lead to unexpected results.",
            what, info.line, info.column, info.template_name
        ))
    };
    match obj {
        Value::List(l) => Some(ForeachIter::Snapshot(l.items.borrow().clone())),
        Value::Map(m) => Some(ForeachIter::Snapshot(m.entries.borrow().iter().map(|(_, v)| v.clone()).collect())),
        Value::Iterator(it) => {
            warn_reset("Iterator");
            Some(ForeachIter::Live(it.clone()))
        }
        Value::Object(o) if o.as_collection().is_some() => Some(ForeachIter::Snapshot(o.as_collection().unwrap())),
        _ => {
            rt.warn(&format!(
                "Could not determine type of iterator in #foreach loop  at [{},{}] in template {}",
                info.line, info.column, info.template_name
            ));
            None
        }
    }
}

/// `org.apache.flex.forks.velocity.util.introspection.Info`.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub template_name: String,
    pub line: i32,
    pub column: i32,
}

impl Info {
    pub fn new(template_name: &str, line: i32, column: i32) -> Info {
        Info { template_name: template_name.to_string(), line, column }
    }
}
