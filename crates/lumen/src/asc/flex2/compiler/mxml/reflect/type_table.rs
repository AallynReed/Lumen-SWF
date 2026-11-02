//! Port of `flex2.compiler.mxml.reflect.TypeTable` with its helper classes, which are the only implementations of the
//! `Type`, `Property`, `Event`, `Style`, `Effect`, `DynamicProperty`, `Inspectable` and `Deprecated` interfaces and so
//! become the structs of those names. The table shares the symbol table's class map and styles; types point back at
//! the table weakly, so the `TypeTableRef` must outlive them (Java keeps it in the symbol table's context).

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::{Rc, Weak};

use super::deprecated::{DEPRECATED, DEPRECATED_MESSAGE, DEPRECATED_REPLACEMENT, DEPRECATED_SINCE, MESSAGE, REPLACEMENT, SINCE};
use super::inspectable;
use crate::asc::flex2::compiler::as3::reflect::as3_class::{AbcClass, PUBLIC_NAMESPACE, VISIBILITY_NAMESPACES};
use crate::asc::flex2::compiler::as3::reflect::meta_data::MetaData;
use crate::asc::flex2::compiler::as3::reflect::method::Method;
use crate::asc::flex2::compiler::as3::reflect::type_table::{get_class_from, ClassTable};
use crate::asc::flex2::compiler::as3::reflect::variable::Variable;
use crate::asc::flex2::compiler::css::document::{CssType, CssTypeTable};
use crate::asc::flex2::compiler::css::styles::Styles;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{self, StandardDefs};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::util::context::ContextStatics;

pub const NOTYPE: &str = "*";
pub const STRING: &str = "String";
pub const BOOLEAN: &str = "Boolean";
pub const NUMBER: &str = "Number";
pub const INT: &str = "int";
pub const UINT: &str = "uint";
pub const FUNCTION: &str = "Function";
pub const CLASS: &str = "Class";
pub const ARRAY: &str = "Array";
pub const OBJECT: &str = "Object";
pub const XML: &str = "XML";
pub const XML_LIST: &str = "XMLList";
pub const REGEXP: &str = "RegExp";
pub const EVENT: &str = "flash.events:Event";
pub const VECTOR: &str = "__AS3__.vec:Vector";

pub type TypeTableRef = Rc<TypeTable>;
pub type TypeRef = Rc<Type>;
pub type PropertyRef = Rc<Property>;
pub type EventRef = Rc<Event>;
pub type StyleRef = Rc<Style>;
pub type EffectRef = Rc<Effect>;
pub type DynamicPropertyRef = Rc<DynamicProperty>;

/// `Stateful`.
pub trait Stateful {
    fn set_state_name(&self, name: Option<&str>);
    fn get_state_name(&self) -> Option<String>;
    fn is_state_specific(&self) -> bool {
        self.get_state_name().is_some()
    }
}

/// `StatefulHelper`'s `state`.
#[derive(Default)]
struct StateName(RefCell<Option<String>>);

impl StateName {
    fn set(&self, name: Option<&str>) {
        *self.0.borrow_mut() = name.map(str::to_string);
    }

    fn get(&self) -> Option<String> {
        self.0.borrow().clone()
    }
}

macro_rules! stateful {
    ($($t:ty),*) => {$(
        impl Stateful for $t {
            fn set_state_name(&self, name: Option<&str>) {
                self.state.set(name)
            }
            fn get_state_name(&self) -> Option<String> {
                self.state.get()
            }
        }
    )*};
}

struct Builtins {
    no_type: Option<TypeRef>,
    string_type: Option<TypeRef>,
    boolean_type: Option<TypeRef>,
    class_type: Option<TypeRef>,
    function_type: Option<TypeRef>,
    number_type: Option<TypeRef>,
    int_type: Option<TypeRef>,
    uint_type: Option<TypeRef>,
    array_type: Option<TypeRef>,
    object_type: Option<TypeRef>,
    xml_type: Option<TypeRef>,
    xml_list_type: Option<TypeRef>,
    reg_exp_type: Option<TypeRef>,
    vector_type: Option<TypeRef>,
}

pub struct TypeTable {
    this: Weak<TypeTable>,
    class_table: ClassTable,
    styles: Rc<RefCell<Styles>>,
    per_compile_data: Rc<RefCell<ContextStatics>>,
    standard_defs: Rc<StandardDefs>,
    manifest: Rc<NameMappings>,
    theme_names: Option<HashSet<String>>,
    builtins: OnceCell<Builtins>,
    type_map: RefCell<HashMap<String, TypeRef>>,
    non_repeaters: RefCell<HashSet<String>>,
}

fn expect_type(t: &Option<TypeRef>) -> TypeRef {
    t.clone().expect("NullPointerException")
}

impl TypeTable {
    pub fn new(symbol_table: &SymbolTable, manifest: Rc<NameMappings>, standard_defs: Rc<StandardDefs>, theme_names: Option<HashSet<String>>) -> TypeTableRef {
        let table = Rc::new_cyclic(|this| TypeTable {
            this: this.clone(),
            class_table: symbol_table.class_table(),
            styles: symbol_table.styles_handle(),
            per_compile_data: symbol_table.per_compile_data.clone(),
            standard_defs,
            manifest,
            theme_names,
            builtins: OnceCell::new(),
            type_map: RefCell::new(HashMap::new()),
            non_repeaters: RefCell::new(HashSet::new()),
        });
        let t = |n: &str| table.get_type(n);
        let builtins = Builtins {
            no_type: t(NOTYPE),
            object_type: t(OBJECT),
            string_type: t(STRING),
            boolean_type: t(BOOLEAN),
            class_type: t(CLASS),
            function_type: t(FUNCTION),
            number_type: t(NUMBER),
            int_type: t(INT),
            uint_type: t(UINT),
            array_type: t(ARRAY),
            xml_type: t(XML),
            xml_list_type: t(XML_LIST),
            reg_exp_type: t(REGEXP),
            vector_type: t(VECTOR),
        };
        let _ = table.builtins.set(builtins);
        table
    }

    fn b(&self) -> &Builtins {
        self.builtins.get().expect("TypeTable used during construction")
    }

    pub fn no_type(&self) -> TypeRef {
        expect_type(&self.b().no_type)
    }
    pub fn string_type(&self) -> TypeRef {
        expect_type(&self.b().string_type)
    }
    pub fn boolean_type(&self) -> TypeRef {
        expect_type(&self.b().boolean_type)
    }
    pub fn class_type(&self) -> TypeRef {
        expect_type(&self.b().class_type)
    }
    pub fn function_type(&self) -> TypeRef {
        expect_type(&self.b().function_type)
    }
    pub fn number_type(&self) -> TypeRef {
        expect_type(&self.b().number_type)
    }
    pub fn int_type(&self) -> TypeRef {
        expect_type(&self.b().int_type)
    }
    pub fn uint_type(&self) -> TypeRef {
        expect_type(&self.b().uint_type)
    }
    pub fn array_type(&self) -> TypeRef {
        expect_type(&self.b().array_type)
    }
    pub fn object_type(&self) -> TypeRef {
        expect_type(&self.b().object_type)
    }
    pub fn xml_type(&self) -> TypeRef {
        expect_type(&self.b().xml_type)
    }
    pub fn xml_list_type(&self) -> TypeRef {
        expect_type(&self.b().xml_list_type)
    }
    pub fn reg_exp_type(&self) -> TypeRef {
        expect_type(&self.b().reg_exp_type)
    }
    pub fn vector_type(&self) -> TypeRef {
        expect_type(&self.b().vector_type)
    }

    fn is_no_type(&self, t: &Type) -> bool {
        self.b().no_type.as_ref().is_some_and(|n| std::ptr::eq(n.as_ref(), t))
    }

    fn get_class(&self, class_name: Option<&str>) -> Option<Rc<AbcClass>> {
        get_class_from(&self.class_table.borrow(), class_name)
    }

    pub fn get_name_mappings(&self) -> Rc<NameMappings> {
        self.manifest.clone()
    }

    /// `getType(namespaceURI, localPart)`.
    /// Java dereferences the null class name of an unmapped tag; so does this.
    pub fn get_type_ns(&self, namespace_uri: &str, local_part: &str) -> Option<TypeRef> {
        let class_name = self.manifest.resolve_class_name(namespace_uri, local_part);
        self.get_type(class_name.as_deref().expect("NullPointerException"))
    }

    /// `getType(QName)`.
    pub fn get_type_qname(&self, q_name: &QName) -> Option<TypeRef> {
        self.get_type_ns(q_name.get_namespace(), &q_name.local_part)
    }

    /// `getType(className)`, `className` in colon form.
    pub fn get_type(&self, class_name: &str) -> Option<TypeRef> {
        if let Some(t) = self.type_map.borrow().get(class_name) {
            return Some(t.clone());
        }
        let mut result = None;
        if let Some(class_info) = self.get_class(Some(class_name)) {
            let t = Rc::new(Type::new(self, class_info, None));
            self.type_map.borrow_mut().insert(class_name.to_string(), t.clone());
            result = Some(t);
        } else if let Some(less_than_index) = class_name.find('<') {
            if let Some(greater_than_index) = class_name.rfind('>') {
                let element_type_name = &class_name[less_than_index + 1..greater_than_index];
                if let Some(element_type) = self.get_type(element_type_name) {
                    let vector = self.get_class(Some(VECTOR)).expect("NullPointerException");
                    let t = Rc::new(Type::new(self, vector, Some(element_type)));
                    self.type_map.borrow_mut().insert(class_name.to_string(), t.clone());
                    result = Some(t);
                }
            }
        }
        result
    }

    pub fn get_vector_type(&self, element_type: &TypeRef) -> TypeRef {
        let class_name = format!("{}.<{}>", VECTOR, element_type.get_name());
        if let Some(t) = self.type_map.borrow().get(&class_name) {
            return t.clone();
        }
        let vector = self.get_class(Some(VECTOR)).expect("NullPointerException");
        let t = Rc::new(Type::new(self, vector, Some(element_type.clone())));
        self.type_map.borrow_mut().insert(class_name, t.clone());
        t
    }

    /// `getStyle(styleName)`: a globally declared style.
    pub fn get_style(&self, style_name: &str) -> Option<StyleRef> {
        let md = self.styles.borrow().get_style(style_name).cloned()?;
        let v = |k: &str| md.get_value_key(k);
        Some(Rc::new(Style::new(self, style_name, v("type"), v("enumeration"), v("format"), v("inherit"), v(DEPRECATED_MESSAGE), v(DEPRECATED_REPLACEMENT), v(DEPRECATED_SINCE))))
    }

    pub fn get_styles(&self) -> std::cell::Ref<'_, Styles> {
        self.styles.borrow()
    }

    pub fn get_dynamic_property(&self, name: &str, state: Option<&str>) -> DynamicPropertyRef {
        let p = DynamicProperty { type_table: self.this.clone(), name: name.to_string(), state: StateName::default() };
        p.state.set(state);
        Rc::new(p)
    }

    pub fn get_per_compile_data(&self) -> Rc<RefCell<ContextStatics>> {
        self.per_compile_data.clone()
    }

    pub fn get_standard_defs(&self) -> Rc<StandardDefs> {
        self.standard_defs.clone()
    }

    /// Breaks the table's reference cycles (types cached here point at their element types and the class model).
    pub fn clear(&self) {
        self.type_map.borrow_mut().clear();
    }
}

impl CssTypeTable for TypeTable {
    fn get_type(&self, class_name: &str) -> Option<Rc<dyn CssType>> {
        TypeTable::get_type(self, class_name).map(|t| t as Rc<dyn CssType>)
    }
}

fn upgrade(t: &Weak<TypeTable>) -> TypeTableRef {
    t.upgrade().expect("TypeTable dropped while its types are in use")
}

/// `TypeHelper`.
pub struct Type {
    type_table: Weak<TypeTable>,
    class_info: Rc<AbcClass>,
    element_type: Option<TypeRef>,
    events: RefCell<Option<EventListHelper>>,
    effects: RefCell<Option<Vec<MetaData>>>,
    excludes: RefCell<Option<Vec<MetaData>>>,
    styles: RefCell<Option<Vec<MetaData>>>,
    standard_defs: Rc<StandardDefs>,
}

impl PartialEq for Type {
    /// `equals(Object)`: same name.
    fn eq(&self, other: &Type) -> bool {
        std::ptr::eq(self, other) || self.get_name() == other.get_name()
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Type {}", self.get_name())
    }
}

/// `Type.equals` on nullable references.
pub fn type_equals(a: Option<&TypeRef>, b: Option<&TypeRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => **a == **b,
        _ => false,
    }
}

impl Type {
    fn new(table: &TypeTable, class_info: Rc<AbcClass>, element_type: Option<TypeRef>) -> Type {
        Type {
            type_table: table.this.clone(),
            class_info,
            element_type,
            events: RefCell::new(None),
            effects: RefCell::new(None),
            excludes: RefCell::new(None),
            styles: RefCell::new(None),
            standard_defs: table.standard_defs.clone(),
        }
    }

    fn tt(&self) -> TypeTableRef {
        upgrade(&self.type_table)
    }

    pub fn get_type_table(&self) -> TypeTableRef {
        self.tt()
    }

    /// The `AbcClass` this type reflects.
    pub fn get_class_info(&self) -> Rc<AbcClass> {
        self.class_info.clone()
    }

    pub fn get_name(&self) -> String {
        match &self.element_type {
            Some(e) => format!("{}.<{}>", self.class_info.get_name(), e.get_name()),
            None => self.class_info.get_name().to_string(),
        }
    }

    pub fn get_super_type(&self) -> Option<TypeRef> {
        let s = self.class_info.get_super_type_name()?;
        self.tt().get_type(s)
    }

    pub fn get_element_type(&self) -> Option<TypeRef> {
        self.element_type.clone()
    }

    pub fn get_interfaces(&self) -> Option<Vec<Option<TypeRef>>> {
        let ifaces = self.class_info.get_interface_names()?;
        let tt = self.tt();
        Some(ifaces.iter().map(|i| tt.get_type(i.as_deref().expect("NullPointerException"))).collect())
    }

    /// `getProperty(name)`: public, protected, internal and private.
    pub fn get_property(&self, name: &str) -> Option<PropertyRef> {
        self.get_property_nss(&VISIBILITY_NAMESPACES, name)
    }

    /// `getProperty(namespace, name)`.
    pub fn get_property_ns(&self, namespace: &str, name: &str) -> Option<PropertyRef> {
        self.get_property_nss(&[namespace], name)
    }

    /// `getProperty(namespaces, name)`.
    pub fn get_property_nss(&self, namespaces: &[&str], name: &str) -> Option<PropertyRef> {
        let tt = self.tt();
        let mut cls = Some(self.class_info.clone());
        let mut super_class: Option<Rc<AbcClass>> = None;
        while let Some(c) = cls {
            if let Some(var) = c.get_variable(namespaces, name, false) {
                if !var.is_static() {
                    return Some(Rc::new(Property::from_variable(&tt, var)));
                } else {
                    super_class = tt.get_class(c.get_super_type_name());
                }
            } else {
                let setter = c.get_setter(namespaces, name, false);
                let getter = c.get_getter(namespaces, name, false);
                if setter.is_some() && getter.is_some() {
                    return Some(Rc::new(Property::from_accessors(&tt, setter, getter)));
                }
                super_class = tt.get_class(c.get_super_type_name());
                if setter.is_some() && super_class.is_some() {
                    let getter = self.find_getter(&tt, super_class.clone(), name);
                    return Some(Rc::new(Property::from_accessors(&tt, setter, getter)));
                } else if getter.is_some() && super_class.is_some() {
                    let setter = self.find_setter(&tt, super_class.clone(), name);
                    return Some(Rc::new(Property::from_accessors(&tt, setter, getter)));
                }
            }
            cls = super_class.clone();
        }
        None
    }

    pub fn has_static_member(&self, name: &str) -> bool {
        let tt = self.tt();
        let mut cls = Some(self.class_info.clone());
        while let Some(c) = cls {
            if let Some(var) = c.get_variable(&VISIBILITY_NAMESPACES, name, false) {
                if var.is_static() {
                    return true;
                }
            } else if let Some(method) = c.get_method(&[PUBLIC_NAMESPACE], name, false) {
                if method.is_static() {
                    return true;
                }
            }
            cls = tt.get_class(c.get_super_type_name());
        }
        false
    }

    pub fn get_event(&self, name: &str) -> Option<EventRef> {
        if self.events.borrow().is_none() {
            let list = EventListHelper::new(&self.tt(), &self.class_info.get_meta_data("Event", false));
            *self.events.borrow_mut() = Some(list);
        }
        let e = self.events.borrow().as_ref().unwrap().get_event(name);
        if e.is_some() {
            return e;
        }
        self.get_super_type().and_then(|st| st.get_event(name))
    }

    fn cached(&self, cell: &RefCell<Option<Vec<MetaData>>>, id: &str) -> Vec<MetaData> {
        if cell.borrow().is_none() {
            *cell.borrow_mut() = Some(self.class_info.get_meta_data(id, true));
        }
        cell.borrow().clone().unwrap()
    }

    pub fn get_effect(&self, name: &str) -> Option<EffectRef> {
        for md in self.cached(&self.effects, "Effect") {
            if md.get_value_index(0).as_deref() == Some(name) {
                return Some(Rc::new(Effect {
                    type_table: self.type_table.clone(),
                    name: name.to_string(),
                    event: md.get_value_key("event"),
                    message: md.get_value_key(DEPRECATED_MESSAGE),
                    replacement: md.get_value_key(DEPRECATED_REPLACEMENT),
                    since: md.get_value_key(DEPRECATED_SINCE),
                    state: StateName::default(),
                }));
            }
        }
        None
    }

    pub fn get_style(&self, name: &str) -> Option<StyleRef> {
        let styles = self.cached(&self.styles, "Style");
        if !self.is_excluded_style(name) {
            let tt = self.tt();
            for md in &styles {
                if md.get_value_key("name").as_deref() == Some(name) {
                    let theme = md.get_value_key("theme");
                    if theme.is_none() || tt.theme_names.is_none() || self.has_theme(&tt, theme.as_deref().unwrap()) {
                        let v = |k: &str| md.get_value_key(k);
                        return Some(Rc::new(Style::new(&tt, name, v("type"), v("enumeration"), v("format"), v("inherit"), v(DEPRECATED_MESSAGE), v(DEPRECATED_REPLACEMENT), v(DEPRECATED_SINCE))));
                    }
                }
            }
        }
        None
    }

    pub fn is_excluded_style(&self, name: &str) -> bool {
        let mut result = false;
        for md in self.cached(&self.excludes, "Exclude") {
            if md.get_value_key("name").as_deref() == Some(name) && md.get_value_key("kind").as_deref() == Some("style") {
                result = true;
            }
        }
        result
    }

    pub fn get_style_themes(&self, name: &str) -> Option<String> {
        for md in self.cached(&self.styles, "Style") {
            if md.get_value_key("name").as_deref() == Some(name) {
                return md.get_value_key("theme");
            }
        }
        None
    }

    fn has_theme(&self, tt: &TypeTable, value: &str) -> bool {
        let names = tt.theme_names.as_ref().unwrap();
        java_split_comma_space(value).iter().any(|t| names.contains(t.as_str()))
    }

    pub fn has_metadata(&self, name: &str, inheriting: bool) -> bool {
        !self.class_info.get_meta_data(name, inheriting).is_empty()
    }

    pub fn get_default_property(&self) -> Option<PropertyRef> {
        let metadata = self.class_info.get_meta_data("DefaultProperty", true);
        let md = metadata.first()?;
        let default_property_name = md.get_value_index(0)?;
        self.get_property(&default_property_name)
    }

    pub fn has_obsolete(&self, name: &str) -> bool {
        self.class_info.get_meta_data("Obsolete", false).iter().any(|md| md.get_value_index(0).as_deref() == Some(name))
    }

    pub fn get_max_children(&self) -> i32 {
        let metadata = self.class_info.get_meta_data("MaxChildren", true);
        match metadata.first() {
            Some(md) => md.get_value_index(0).as_deref().unwrap_or("null").parse::<i32>().expect("NumberFormatException"),
            None => 0,
        }
    }

    pub fn get_loader_class(&self) -> Option<String> {
        self.class_info.get_meta_data("Frame", true).first().and_then(|md| md.get_value_key("factoryClass"))
    }

    pub fn has_dynamic(&self) -> bool {
        self.class_info.is_dynamic()
    }

    /// `isSubclassOf(Type)`.
    pub fn is_subclass_of(&self, base_type: Option<&Type>) -> bool {
        match base_type {
            Some(b) if std::ptr::eq(b, self) => true,
            Some(b) if self.tt().is_no_type(b) => false,
            Some(b) => self.is_subclass_of_name(Some(&b.get_name())),
            None => false,
        }
    }

    /// `isSubclassOf(String)`.
    pub fn is_subclass_of_name(&self, base_name: Option<&str>) -> bool {
        if base_name == Some(NOTYPE) {
            false
        } else {
            self.is_assignable_to_name(base_name)
        }
    }

    /// `isAssignableTo(Type)`.
    pub fn is_assignable_to(&self, base_type: Option<&Type>) -> bool {
        match base_type {
            Some(b) if std::ptr::eq(b, self) || self.tt().is_no_type(b) => true,
            Some(b) => self.is_assignable_to_name(Some(&b.get_name())),
            None => false,
        }
    }

    /// `isAssignableTo(String)`.
    pub fn is_assignable_to_name(&self, base_name: Option<&str>) -> bool {
        let tt = self.tt();
        let name = self.get_name();
        let is_repeater = base_name == Some(self.standard_defs.class_repeater.as_str());
        if is_repeater && tt.non_repeaters.borrow().contains(&name) {
            return false;
        }
        if base_name == Some(NOTYPE) || base_name == Some(name.as_str()) {
            return true;
        }
        let super_type = self.get_super_type();
        if let Some(st) = &super_type {
            if base_name == Some(st.get_name().as_str()) {
                return true;
            }
        }
        let interfaces = self.get_interfaces();
        for i in interfaces.iter().flatten().flatten() {
            if base_name.is_some() && base_name == Some(i.get_name().as_str()) {
                return true;
            }
        }
        if let Some(st) = &super_type {
            if st.is_assignable_to_name(base_name) {
                return true;
            }
        }
        for i in interfaces.iter().flatten().flatten() {
            if i.is_assignable_to_name(base_name) {
                return true;
            }
        }
        if is_repeater {
            tt.non_repeaters.borrow_mut().insert(name);
        }
        false
    }

    fn find_getter(&self, tt: &TypeTable, cls: Option<Rc<AbcClass>>, name: &str) -> Option<Method> {
        let mut cls = cls;
        while let Some(c) = cls {
            if let Some(g) = c.get_getter(&[PUBLIC_NAMESPACE], name, true) {
                return Some(g);
            }
            cls = tt.get_class(c.get_super_type_name());
        }
        None
    }

    fn find_setter(&self, tt: &TypeTable, cls: Option<Rc<AbcClass>>, name: &str) -> Option<Method> {
        let mut cls = cls;
        while let Some(c) = cls {
            if let Some(s) = c.get_setter(&[PUBLIC_NAMESPACE], name, true) {
                return Some(s);
            }
            cls = tt.get_class(c.get_super_type_name());
        }
        None
    }
}

impl CssType for Type {
    fn has_style(&self, name: &str) -> bool {
        self.get_style(name).is_some()
    }
    fn get_style_themes(&self, name: &str) -> Option<String> {
        Type::get_style_themes(self, name)
    }
    fn is_excluded_style(&self, name: &str) -> bool {
        Type::is_excluded_style(self, name)
    }
    fn has_property(&self, name: &str) -> bool {
        self.get_property(name).is_some()
    }
}

/// `String.split("[, ]")`: empty strings kept, trailing ones dropped.
fn java_split_comma_space(value: &str) -> Vec<String> {
    let mut parts: Vec<String> = value.split([',', ' ']).map(str::to_string).collect();
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

/// `new StringTokenizer(s, ",")` collected.
fn tokenize_commas(s: &str) -> Vec<String> {
    s.split(',').filter(|t| !t.is_empty()).map(str::to_string).collect()
}

struct EventListHelper {
    event_types: Option<HashMap<String, EventRef>>,
}

impl EventListHelper {
    fn new(tt: &TypeTable, events: &[MetaData]) -> EventListHelper {
        if events.is_empty() {
            return EventListHelper { event_types: None };
        }
        let mut event_types = HashMap::new();
        for md in events {
            let mut name = md.get_value_key("name");
            let mut type_name = md.get_value_key("type");
            if name.is_some() {
                type_name = Some(match type_name {
                    None => EVENT.to_string(),
                    Some(t) => name_formatter::to_colon(&t),
                });
            } else {
                name = md.get_value_index(0);
                type_name = Some(EVENT.to_string());
            }
            if let Some(type_name) = type_name {
                let name = name.clone();
                let e = Event {
                    type_table: tt.this.clone(),
                    name: name.clone(),
                    type_name,
                    type_: RefCell::new(None),
                    message: md.get_value_key(DEPRECATED_MESSAGE),
                    replacement: md.get_value_key(DEPRECATED_REPLACEMENT),
                    since: md.get_value_key(DEPRECATED_SINCE),
                    state: StateName::default(),
                };
                event_types.insert(name.unwrap_or_else(|| "null".to_string()), Rc::new(e));
            }
        }
        EventListHelper { event_types: Some(event_types) }
    }

    fn get_event(&self, name: &str) -> Option<EventRef> {
        self.event_types.as_ref().and_then(|m| m.get(name).cloned())
    }
}

/// `PropertyHelper`.
pub struct Property {
    type_table: Weak<TypeTable>,
    standard_defs: Rc<StandardDefs>,
    read_only: bool,
    name: Option<String>,
    var: Option<Variable>,
    setter: Option<Method>,
    getter: Option<Method>,
    type_: RefCell<Option<TypeRef>>,
    instance_type: RefCell<Option<TypeRef>>,
    lvalue_type: RefCell<Option<TypeRef>>,
    state: StateName,
}

stateful!(Property, Event, Style, Effect, DynamicProperty);

impl PartialEq for Property {
    fn eq(&self, other: &Property) -> bool {
        self.get_name() == other.get_name()
    }
}

impl fmt::Display for Property {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Property {}", self.get_name().unwrap_or("null"))
    }
}

impl Property {
    fn from_variable(tt: &TypeTable, var: Variable) -> Property {
        let name = Some(var.get_qname().local_part.clone());
        let read_only = var.is_const();
        Property {
            type_table: tt.this.clone(),
            standard_defs: tt.standard_defs.clone(),
            read_only,
            name,
            var: Some(var),
            setter: None,
            getter: None,
            type_: RefCell::new(None),
            instance_type: RefCell::new(None),
            lvalue_type: RefCell::new(None),
            state: StateName::default(),
        }
    }

    fn from_accessors(tt: &TypeTable, setter: Option<Method>, getter: Option<Method>) -> Property {
        let name = match (&setter, &getter) {
            (Some(s), _) => Some(s.get_qname().local_part.clone()),
            (None, Some(g)) => Some(g.get_qname().local_part.clone()),
            _ => None,
        };
        let read_only = setter.is_none() && getter.is_some();
        Property {
            type_table: tt.this.clone(),
            standard_defs: tt.standard_defs.clone(),
            read_only,
            name,
            var: None,
            setter,
            getter,
            type_: RefCell::new(None),
            instance_type: RefCell::new(None),
            lvalue_type: RefCell::new(None),
            state: StateName::default(),
        }
    }

    fn tt(&self) -> TypeTableRef {
        upgrade(&self.type_table)
    }

    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn get_type(&self) -> Option<TypeRef> {
        if self.type_.borrow().is_none() {
            let class_name = if let Some(v) = &self.var {
                v.get_type_name()
            } else if let Some(s) = &self.setter {
                s.get_parameter_type_names().expect("NullPointerException")[0].clone()
            } else {
                self.getter.as_ref().expect("NullPointerException").get_return_type_name()
            };
            *self.type_.borrow_mut() = self.tt().get_type(class_name.as_deref().expect("NullPointerException"));
        }
        self.type_.borrow().clone()
    }

    pub fn get_lvalue_type(&self) -> Option<TypeRef> {
        if self.lvalue_type.borrow().is_none() {
            let mut lvalue_type = self.get_type();
            if self.standard_defs.is_ideferred_instance(lvalue_type.as_deref().expect("NullPointerException")) {
                lvalue_type = self.get_instance_type();
            }
            *self.lvalue_type.borrow_mut() = lvalue_type;
        }
        self.lvalue_type.borrow().clone()
    }

    pub fn get_element_type(&self) -> Option<TypeRef> {
        let tt = self.tt();
        let t = self.get_type().expect("NullPointerException");
        let mut element_type = t.get_element_type();
        if element_type.is_none() {
            let mut element_type_name = None;
            if let Some(md_list) = self.get_meta_data_list(standard_defs::MD_ARRAYELEMENTTYPE) {
                let meta_data = &md_list[0];
                if meta_data.count() > 0 {
                    if let Some(value) = meta_data.get_value_index(0) {
                        element_type_name = Some(name_formatter::to_colon(&value));
                    }
                }
            }
            if let Some(element_type_name) = &element_type_name {
                element_type = tt.get_type(element_type_name);
                if *t == *tt.array_type() && element_type.is_none() {
                    let text = format!("[{}({element_type_name})]: type {element_type_name} not found.", standard_defs::MD_ARRAYELEMENTTYPE);
                    toolkit::log(CompilerMessage::error(text), None, -1, -1, None);
                }
            }
            if element_type.is_none() {
                element_type = Some(tt.object_type());
            }
        }
        element_type
    }

    pub fn get_instance_type(&self) -> Option<TypeRef> {
        if self.instance_type.borrow().is_none() {
            let tt = self.tt();
            let mut instance_type_name = None;
            if let Some(md_list) = self.get_meta_data_list(standard_defs::MD_INSTANCETYPE) {
                let meta_data = &md_list[0];
                if meta_data.count() > 0 {
                    instance_type_name = Some(name_formatter::to_colon(&meta_data.get_value_index(0).expect("NullPointerException")));
                }
            }
            let instance_type = match instance_type_name {
                None => tt.object_type(),
                Some(n) => match tt.get_type(&n) {
                    Some(t) => t,
                    None => {
                        let text = format!("[{}]({n}): type {n} is unavailable.", standard_defs::MD_INSTANCETYPE);
                        toolkit::log(CompilerMessage::warning(text), None, -1, -1, None);
                        tt.object_type()
                    }
                },
            };
            *self.instance_type.borrow_mut() = Some(instance_type);
        }
        self.instance_type.borrow().clone()
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn has_public(&self) -> bool {
        match (&self.var, &self.setter, &self.getter) {
            (Some(v), _, _) => v.is_public(),
            (None, Some(s), _) => s.is_public(),
            (None, None, g) => g.as_ref().expect("NullPointerException").is_public(),
        }
    }

    pub fn get_inspectable(&self) -> Option<Inspectable> {
        let md_list = self.get_meta_data_list(inspectable::INSPECTABLE)?;
        let md = &md_list[0];
        let v = |k: &str| md.get_value_key(k);
        Some(Inspectable::new(
            v(inspectable::ENUMERATION),
            v(inspectable::DEFAULT_VALUE),
            v(inspectable::IS_DEFAULT),
            v(inspectable::CATEGORY),
            v(inspectable::IS_VERBOSE),
            v(inspectable::TYPE),
            v(inspectable::OBJECT_TYPE),
            v(inspectable::ARRAY_TYPE),
            v(inspectable::ENVIRONMENT),
            v(inspectable::FORMAT),
        ))
    }

    pub fn get_deprecated(&self) -> Option<Deprecated> {
        let md_list = self.get_meta_data_list(DEPRECATED)?;
        let md = &md_list[0];
        let replacement = md.get_value_key(REPLACEMENT);
        let mut message = md.get_value_key(MESSAGE);
        let since = md.get_value_key(SINCE);
        if replacement.is_none() && message.is_none() && since.is_none() && md.count() > 0 {
            message = md.get_value_index(0);
        }
        Some(Deprecated { replacement, message, since })
    }

    pub fn has_change_event(&self, name: &str) -> bool {
        if let Some(v) = &self.var {
            if let Some(md_list) = v.get_meta_data(standard_defs::MD_CHANGEEVENT) {
                return md_list.iter().any(|md| md.get_value_index(0).as_deref() == Some(name));
            }
        }
        false
    }

    pub fn get_percent_proxy(&self) -> Option<String> {
        let md_list = self.get_meta_data_list(standard_defs::MD_PERCENTPROXY)?;
        let md = &md_list[0];
        if md.count() > 0 {
            md.get_value_index(0)
        } else {
            None
        }
    }

    pub fn rich_text_content(&self) -> bool {
        self.get_meta_data_list(standard_defs::MD_RICHTEXTCONTENT).is_some()
    }

    pub fn collapse_white_space(&self) -> bool {
        self.get_meta_data_list(standard_defs::MD_COLLAPSEWHITESPACE).is_some()
    }

    fn get_meta_data_list(&self, name: &str) -> Option<Vec<MetaData>> {
        let mut md_list = None;
        if let Some(v) = &self.var {
            md_list = v.get_meta_data(name);
        } else {
            if let Some(g) = &self.getter {
                md_list = g.get_meta_data(name);
            }
            if md_list.is_none() {
                if let Some(s) = &self.setter {
                    md_list = s.get_meta_data(name);
                }
            }
        }
        md_list.filter(|l| !l.is_empty())
    }

    pub fn get_variable(&self) -> Option<&Variable> {
        self.var.as_ref()
    }

    pub fn get_setter(&self) -> Option<&Method> {
        self.setter.as_ref()
    }

    pub fn get_getter(&self) -> Option<&Method> {
        self.getter.as_ref()
    }
}

/// `EventHelper`.
pub struct Event {
    type_table: Weak<TypeTable>,
    name: Option<String>,
    type_name: String,
    type_: RefCell<Option<TypeRef>>,
    message: Option<String>,
    replacement: Option<String>,
    since: Option<String>,
    state: StateName,
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Event {}", self.name.as_deref().unwrap_or("null"))
    }
}

impl Event {
    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn get_type_name(&self) -> &str {
        &self.type_name
    }

    pub fn get_type(&self) -> Option<TypeRef> {
        if self.type_.borrow().is_none() {
            *self.type_.borrow_mut() = upgrade(&self.type_table).get_type(&self.type_name);
        }
        self.type_.borrow().clone()
    }

    pub fn get_deprecated_message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn get_deprecated_replacement(&self) -> Option<&str> {
        self.replacement.as_deref()
    }

    pub fn get_deprecated_since(&self) -> Option<&str> {
        self.since.as_deref()
    }
}

/// `StyleHelper`.
pub struct Style {
    type_table: Weak<TypeTable>,
    standard_defs: Rc<StandardDefs>,
    name: String,
    type_name: String,
    enumeration: Option<Vec<String>>,
    format: Option<String>,
    is_inherit: bool,
    message: Option<String>,
    replacement: Option<String>,
    since: Option<String>,
    type_: RefCell<Option<TypeRef>>,
    lvalue_type: RefCell<Option<TypeRef>>,
    state: StateName,
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Style {}", self.name)
    }
}

impl Style {
    #[allow(clippy::too_many_arguments)]
    fn new(tt: &TypeTable, name: &str, type_name: Option<String>, enumeration: Option<String>, format: Option<String>, inherit: Option<String>, message: Option<String>, replacement: Option<String>, since: Option<String>) -> Style {
        let type_name = match type_name {
            None => if enumeration.is_none() { OBJECT } else { STRING }.to_string(),
            Some(t) => {
                if tt.get_type(&name_formatter::to_colon(&t)).is_none() {
                    OBJECT.to_string()
                } else {
                    t
                }
            }
        };
        Style {
            type_table: tt.this.clone(),
            standard_defs: tt.standard_defs.clone(),
            name: name.to_string(),
            type_name: name_formatter::to_colon(&type_name),
            enumeration: enumeration.as_deref().map(tokenize_commas),
            format,
            is_inherit: inherit.is_some_and(|i| i.eq_ignore_ascii_case("yes")),
            message,
            replacement,
            since,
            type_: RefCell::new(None),
            lvalue_type: RefCell::new(None),
            state: StateName::default(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_type(&self) -> Option<TypeRef> {
        if self.type_.borrow().is_none() {
            *self.type_.borrow_mut() = upgrade(&self.type_table).get_type(&self.type_name);
        }
        self.type_.borrow().clone()
    }

    pub fn get_lvalue_type(&self) -> Option<TypeRef> {
        if self.lvalue_type.borrow().is_none() {
            let mut lvalue_type = self.get_type();
            if self.standard_defs.is_ideferred_instance(lvalue_type.as_deref().expect("NullPointerException")) {
                lvalue_type = Some(upgrade(&self.type_table).object_type());
            }
            *self.lvalue_type.borrow_mut() = lvalue_type;
        }
        self.lvalue_type.borrow().clone()
    }

    pub fn get_element_type(&self) -> Option<TypeRef> {
        Some(upgrade(&self.type_table).object_type())
    }

    pub fn get_enumeration(&self) -> Option<&[String]> {
        self.enumeration.as_deref()
    }

    pub fn get_format(&self) -> Option<&str> {
        self.format.as_deref()
    }

    pub fn is_inherit(&self) -> bool {
        self.is_inherit
    }

    pub fn get_deprecated_message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn get_deprecated_replacement(&self) -> Option<&str> {
        self.replacement.as_deref()
    }

    pub fn get_deprecated_since(&self) -> Option<&str> {
        self.since.as_deref()
    }
}

/// `EffectHelper`.
pub struct Effect {
    type_table: Weak<TypeTable>,
    name: String,
    event: Option<String>,
    message: Option<String>,
    replacement: Option<String>,
    since: Option<String>,
    state: StateName,
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Effect {}", self.name)
    }
}

impl Effect {
    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_type(&self) -> Option<TypeRef> {
        let tt = upgrade(&self.type_table);
        let class_effect = tt.standard_defs.class_effect.clone();
        tt.get_type(&class_effect)
    }

    pub fn get_lvalue_type(&self) -> Option<TypeRef> {
        self.get_type()
    }

    pub fn get_element_type(&self) -> Option<TypeRef> {
        Some(upgrade(&self.type_table).object_type())
    }

    pub fn get_event(&self) -> Option<&str> {
        self.event.as_deref()
    }

    pub fn get_deprecated_message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn get_deprecated_replacement(&self) -> Option<&str> {
        self.replacement.as_deref()
    }

    pub fn get_deprecated_since(&self) -> Option<&str> {
        self.since.as_deref()
    }
}

/// `DynamicPropertyHelper`.
pub struct DynamicProperty {
    type_table: Weak<TypeTable>,
    name: String,
    state: StateName,
}

impl fmt::Display for DynamicProperty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DynamicProperty {}", self.name)
    }
}

impl DynamicProperty {
    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_type(&self) -> Option<TypeRef> {
        Some(upgrade(&self.type_table).object_type())
    }

    pub fn get_lvalue_type(&self) -> Option<TypeRef> {
        self.get_type()
    }

    pub fn get_element_type(&self) -> Option<TypeRef> {
        Some(upgrade(&self.type_table).object_type())
    }
}

/// `InspectableHelper`.
#[derive(Clone, Debug)]
pub struct Inspectable {
    enumeration: Option<Vec<String>>,
    default_value: Option<String>,
    is_default: bool,
    category: Option<String>,
    is_verbose: bool,
    type_: Option<String>,
    object_type: Option<String>,
    array_type: Option<String>,
    environment: Option<String>,
    format: Option<String>,
}

impl Inspectable {
    #[allow(clippy::too_many_arguments)]
    fn new(enumeration: Option<String>, default_value: Option<String>, is_default: Option<String>, category: Option<String>, is_verbose: Option<String>, type_: Option<String>, object_type: Option<String>, array_type: Option<String>, environment: Option<String>, format: Option<String>) -> Inspectable {
        Inspectable {
            enumeration: enumeration.as_deref().map(tokenize_commas),
            default_value,
            is_default: is_default.is_some_and(|v| v.eq_ignore_ascii_case("yes")),
            category,
            is_verbose: is_verbose.is_some_and(|v| v.eq_ignore_ascii_case("yes")),
            type_,
            object_type,
            array_type,
            environment,
            format,
        }
    }

    pub fn get_enumeration(&self) -> Option<&[String]> {
        self.enumeration.as_deref()
    }
    pub fn get_default_value(&self) -> Option<&str> {
        self.default_value.as_deref()
    }
    pub fn is_default(&self) -> bool {
        self.is_default
    }
    pub fn get_category(&self) -> Option<&str> {
        self.category.as_deref()
    }
    pub fn is_verbose(&self) -> bool {
        self.is_verbose
    }
    pub fn get_type(&self) -> Option<&str> {
        self.type_.as_deref()
    }
    pub fn get_object_type(&self) -> Option<&str> {
        self.object_type.as_deref()
    }
    pub fn get_array_type(&self) -> Option<&str> {
        self.array_type.as_deref()
    }
    pub fn get_environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }
    pub fn get_format(&self) -> Option<&str> {
        self.format.as_deref()
    }
}

/// `DeprecatedHelper`.
#[derive(Clone, Debug)]
pub struct Deprecated {
    replacement: Option<String>,
    message: Option<String>,
    since: Option<String>,
}

impl Deprecated {
    pub fn get_replacement(&self) -> Option<&str> {
        self.replacement.as_deref()
    }
    pub fn get_message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn get_since(&self) -> Option<&str> {
        self.since.as_deref()
    }
}
