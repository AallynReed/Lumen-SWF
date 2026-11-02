//! Port of `flex2.compiler.mxml.rep.init.ValueInitializer` (text codegen only). Its subclasses are
//! [`ValueInitializerKind`]s, each with its constructor and overrides in the subclass's own file.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use indexmap::IndexSet;

use super::initializer::{Initializer, InitializerRef};
use crate::asc::flex2::compiler::mxml::gen::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::mxml::gen::descriptor_generator;
use crate::asc::flex2::compiler::mxml::gen::text_gen;
use crate::asc::flex2::compiler::mxml::rep::deps::{self, type_eq, Effect, Property, StandardDefs, Style, Type, TypeTable};
use crate::asc::flex2::compiler::mxml::rep::model::{ModelKind, ModelRef, RValue};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::util::jstr;

pub enum ValueInitializerKind {
    StaticProperty { property: Property },
    DynamicProperty { type_: Type, name: String },
    Style { style: Style },
    Effect { effect: Effect, type_: Type },
    ArrayElement { type_: Type, index: usize },
    VisualChild,
    /// `decl.InitializedPropertyDeclaration`, a `DynamicPropertyInitializer`.
    InitializedPropertyDeclaration { type_: Type, name: String, top_level: bool },
}

impl Clone for ValueInitializerKind {
    fn clone(&self) -> Self {
        match self {
            ValueInitializerKind::StaticProperty { property } => ValueInitializerKind::StaticProperty { property: property.clone() },
            ValueInitializerKind::DynamicProperty { type_, name } => ValueInitializerKind::DynamicProperty { type_: type_.clone(), name: name.clone() },
            ValueInitializerKind::Style { style } => ValueInitializerKind::Style { style: style.clone() },
            ValueInitializerKind::Effect { effect, type_ } => ValueInitializerKind::Effect { effect: effect.clone(), type_: type_.clone() },
            ValueInitializerKind::ArrayElement { type_, index } => ValueInitializerKind::ArrayElement { type_: type_.clone(), index: *index },
            ValueInitializerKind::VisualChild => ValueInitializerKind::VisualChild,
            ValueInitializerKind::InitializedPropertyDeclaration { type_, name, top_level } => {
                ValueInitializerKind::InitializedPropertyDeclaration { type_: type_.clone(), name: name.clone(), top_level: *top_level }
            }
        }
    }
}

pub struct ValueInitializer {
    pub(crate) standard_defs: Rc<StandardDefs>,
    value: RefCell<RValue>,
    pub(crate) line: i32,
    state_specific: Cell<bool>,
    pub kind: ValueInitializerKind,
}

/// `StringUtils.formatString(s)`: `s` as a double-quoted ActionScript literal.
pub fn format_string(s: &str) -> String {
    let units: Vec<u16> = jstr::to_utf16(s);
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for (i, &c) in units.iter().enumerate() {
        match c {
            0x5c => {
                if i + 1 < units.len() && units[i + 1] == b'u' as u16 {
                    result.push('\\');
                } else {
                    result.push_str("\\\\");
                }
            }
            0x22 => result.push_str("\\\""),
            0x08 => result.push_str("\\b"),
            0x09 => result.push_str("\\t"),
            0x0c => result.push_str("\\f"),
            0x0d => result.push_str("\\r"),
            0x0a => result.push_str("\\n"),
            c if c < 0x20 => {
                result.push_str("\\x");
                result.push_str(&c.to_string());
                result.push('X');
            }
            c => jstr::push_unit(&mut result, c),
        }
    }
    result.push('"');
    result
}

impl ValueInitializer {
    pub(crate) fn create(value: RValue, line: i32, standard_defs: Rc<StandardDefs>, kind: ValueInitializerKind) -> InitializerRef {
        let v = ValueInitializer { standard_defs, value: RefCell::new(value.clone()), line, state_specific: Cell::new(false), kind };
        v.set_value(value);
        Rc::new(Initializer::Value(v))
    }

    pub fn get_value(&self) -> RValue {
        self.value.borrow().clone()
    }

    pub fn set_value(&self, value: RValue) {
        if let RValue::Model(m) = &value {
            self.state_specific.set(m.is_state_specific());
        }
        *self.value.borrow_mut() = value;
    }

    /// The rvalue as a model, when it is one.
    pub fn get_model(&self) -> Option<ModelRef> {
        self.value.borrow().as_model().cloned()
    }

    /// `clone()` then `setValue(value)`.
    pub fn clone_with_value(&self, value: RValue) -> InitializerRef {
        let v = ValueInitializer {
            standard_defs: self.standard_defs.clone(),
            value: RefCell::new(self.get_value()),
            line: self.line,
            state_specific: Cell::new(self.state_specific.get()),
            kind: self.kind.clone(),
        };
        v.set_value(value);
        Rc::new(Initializer::Value(v))
    }

    pub fn get_line_ref(&self) -> i32 {
        self.line
    }

    pub fn is_binding(&self) -> bool {
        match &*self.value.borrow() {
            RValue::Binding(_) => true,
            RValue::Model(m) => m.is_primitive() && m.has_bindings(),
            _ => false,
        }
    }

    pub fn is_design_layer(&self) -> bool {
        matches!(&*self.value.borrow(), RValue::Model(m) if m.is_design_layer())
    }

    pub fn is_state_specific(&self) -> bool {
        self.state_specific.get()
    }

    pub fn set_state_specific(&self, value: bool) {
        self.state_specific.set(value);
    }

    /// `getLValueType()`, per subclass.
    pub fn get_lvalue_type(&self) -> Type {
        match &self.kind {
            ValueInitializerKind::StaticProperty { property } => property.get_type().expect("NullPointerException"),
            ValueInitializerKind::DynamicProperty { type_, .. } => type_.clone(),
            ValueInitializerKind::Style { style } => style.get_type().expect("NullPointerException"),
            ValueInitializerKind::Effect { type_, .. } => type_.clone(),
            ValueInitializerKind::ArrayElement { type_, .. } => type_.clone(),
            ValueInitializerKind::VisualChild => self.get_model().expect("ClassCastException").get_type().clone(),
            ValueInitializerKind::InitializedPropertyDeclaration { .. } => self.initialized_property_declaration_lvalue_type(),
        }
    }

    /// `NamedInitializer.getName()`; `None` for unnamed initializers.
    pub fn get_name(&self) -> Option<String> {
        match &self.kind {
            ValueInitializerKind::StaticProperty { property } => Some(property.get_name().unwrap_or("null").to_string()),
            ValueInitializerKind::DynamicProperty { name, .. } | ValueInitializerKind::InitializedPropertyDeclaration { name, .. } => Some(name.clone()),
            ValueInitializerKind::Style { style } => Some(style.get_name().to_string()),
            ValueInitializerKind::Effect { effect, .. } => Some(effect.get_name().to_string()),
            ValueInitializerKind::ArrayElement { .. } | ValueInitializerKind::VisualChild => None,
        }
    }

    /// `instanceof NamedInitializer`.
    pub fn is_named(&self) -> bool {
        self.get_name().is_some()
    }

    pub fn has_definition(&self) -> bool {
        match &*self.value.borrow() {
            RValue::Model(model) => model.is_declared() || !self.model_has_inline_rvalue() || self.is_instance_generator_over_definition(),
            _ => false,
        }
    }

    pub(crate) fn is_instance_generator_over_definition(&self) -> bool {
        let ltype = self.get_lvalue_type();
        self.standard_defs.is_ifactory(&ltype) || (self.standard_defs.is_ideferred_instance(&ltype) && !self.rvalue_is_class_ref())
    }

    pub(crate) fn rvalue_is_class_ref(&self) -> bool {
        matches!(&*self.value.borrow(), RValue::Model(m) if m.is_primitive() && type_eq(m.get_type(), &self.get_type_table().class_type()))
    }

    pub fn get_value_expr(&self) -> String {
        let lvalue_type = self.get_lvalue_type();
        if self.standard_defs.is_ideferred_instance(&lvalue_type) {
            if self.rvalue_is_class_ref() {
                format!("new {}({})", name_formatter::to_dot(&self.standard_defs.class_deferredinstancefromclass), self.get_inline_rvalue())
            } else {
                let transient = if self.standard_defs.is_itransient_deferred_instance(&lvalue_type) {
                    format!(",{}_r", self.get_model().expect("ClassCastException").get_definition_name().unwrap_or_default())
                } else {
                    String::new()
                };
                format!("new {}({}{})", name_formatter::to_dot(&self.standard_defs.class_deferredinstancefromfunction), self.get_definition_name(), transient)
            }
        } else if self.has_definition() {
            format!("{}()", self.get_definition_name())
        } else {
            self.get_inline_rvalue()
        }
    }

    pub(crate) fn model_has_inline_rvalue(&self) -> bool {
        let model = self.get_model().expect("assert value instanceof Model");
        match &model.kind {
            ModelKind::Array(a) if a.vector.is_some() => !a.is_fixed(),
            ModelKind::Xml(_) | ModelKind::XmlList(_) | ModelKind::Primitive(_) | ModelKind::Array(_) => true,
            _ => type_eq(model.get_type(), &self.get_type_table().object_type()),
        }
    }

    fn get_inline_rvalue(&self) -> String {
        let value = self.get_value();
        match &value {
            RValue::Model(model) => match &model.kind {
                ModelKind::Primitive(p) => self.format_expr(model.get_type(), &p.get_value().expect("assert value != null")),
                ModelKind::Array(a) if a.vector.is_some() => as_vector_literal(model),
                ModelKind::Array(_) => as_array_literal(model),
                ModelKind::Xml(_) => self.as_xml_literal(model),
                ModelKind::XmlList(_) => as_xml_list(model),
                _ if type_eq(model.get_type(), &self.get_type_table().object_type()) => as_object_literal(model),
                _ => panic!("can't generate inline expr for values of type {}", model.kind_class_simple_name()),
            },
            _ => self.format_expr(&self.get_lvalue_type(), &value),
        }
    }

    /// `getDefinitionName()`: `_i` when the rvalue is declared, `_c` otherwise.
    pub(crate) fn get_definition_name(&self) -> String {
        let model = self.get_model().expect("non-Model value has definition in getDefinitionName()");
        format!("{}{}", model.get_definition_name().unwrap_or_default(), if model.is_declared() { "_i" } else { "_c" })
    }

    pub(crate) fn get_definition_body(&self) -> CodeFragmentList {
        let var_name = "temp";
        let self_model = self.get_model().expect("non-Model value has definition in getDefinitionBody()");
        let self_type = self_model.get_type().clone();
        let self_type_name = match &self_model.kind {
            ModelKind::Array(a) if a.vector.is_some() => format!("{}.<{}>", deps::CLASS_VECTOR, a.get_element_type_name()),
            _ => name_formatter::to_dot(&self_type.get_name().to_string()),
        };
        let is_declared = self_model.is_declared();
        let id = if is_declared { self_model.get_id().unwrap_or_default() } else { var_name.to_string() };
        let line = self.get_line_ref();
        let sd = &self.standard_defs;
        let mut list = CodeFragmentList::new();

        list.add4("private function ", &self.get_definition_name(), "() : ", &self_type_name, line);
        list.add("{", line);

        let mut sb = format!("\tvar {var_name} : {self_type_name} = ");
        if self.model_has_inline_rvalue() {
            sb.push_str(&self.get_inline_rvalue());
        } else if let ModelKind::Array(a) = &self_model.kind {
            if a.vector.is_some() {
                sb.push_str(&format!("new {self_type_name}({}", a.size()));
                sb.push_str(if a.is_fixed() { ", true)" } else { ")" });
            } else {
                sb.push_str(&format!("new {self_type_name}()"));
            }
        } else {
            sb.push_str(&format!("new {self_type_name}()"));
        }
        sb.push(';');
        list.add(&sb, line);

        if !self.model_has_inline_rvalue() {
            match &self_model.kind {
                ModelKind::Array(a) if a.vector.is_some() => add_assign_exprs(&mut list, &a.get_element_initializer_iterator(), var_name),
                _ => add_assign_exprs(&mut list, &self_model.get_property_initializer_iterator_bindings(self_model.get_type().has_dynamic()), var_name),
            }
        }
        add_assign_exprs(&mut list, &self_model.get_style_initializer_iterator(), var_name);
        add_assign_exprs(&mut list, &self_model.get_effect_initializer_iterator(), var_name);
        add_assign_exprs(&mut list, &self_model.get_event_initializer_iterator(), var_name);

        let effect_event_names = self_model.get_effect_names();
        if !effect_event_names.is_empty() {
            list.add5("\t", var_name, ".registerEffects([ ", &effect_event_names, " ]);", line);
        }

        if is_declared && sd.is_iui_component_with_id_property(&self_type) {
            list.add5("\t", var_name, ".id = \"", &id, "\";", line);
        }

        let layer_parent = self_model.layer_parent.borrow().clone();
        if let Some(layer_parent) = layer_parent {
            let layer_id = layer_parent.get_id().unwrap_or_default();
            if self_model.is_design_layer() {
                list.add5("\t", &layer_id, ".addLayer(", var_name, ");", line);
            } else if self_model.get_type().is_assignable_to_name(Some(&sd.interface_ivisualelement)) {
                list.add5("\t", var_name, ".designLayer = ", &layer_id, ";", line);
            }
        }

        if sd.is_iui_component(&self_type) {
            let ModelKind::MovieClip(movie_clip) = &self_model.kind else { panic!("isIUIComponent(selfType) but !(self instanceof MovieClip)") };
            list.add(&format!("\tif (!{var_name}.document) {var_name}.document = this;"), line);
            if !sd.is_repeater(&self_type) {
                if sd.is_container(&self_type) {
                    if movie_clip.has_children() {
                        list.add3("\t", var_name, ".mx_internal::_documentDescriptor = ", line);
                        descriptor_generator::add_descriptor_initializer_fragments_props(&mut list, &self_model, &HashSet::new(), false, "\t\t");
                        list.add("\t;", line);
                        list.add3("\t", var_name, ".mx_internal::_documentDescriptor.document = this;", line);
                    }
                } else {
                    for init in movie_clip.get_child_initializer_iterator() {
                        let child = init.as_value().and_then(|v| v.get_model()).expect("ClassCastException");
                        if !child.is_state_specific() {
                            list.add3("\t", &init.get_assign_expr(var_name), ";", init.get_line_ref());
                        }
                    }
                }
            } else {
                list.add3("\tvar cd:Array = ", var_name, ".childDescriptors = [", line);
                let children = movie_clip.children();
                for (i, init) in children.iter().enumerate() {
                    let child = init.as_value().and_then(|v| v.get_model()).expect("ClassCastException");
                    descriptor_generator::add_descriptor_initializer_fragments(&mut list, &child, "\t\t");
                    if i + 1 < children.len() {
                        list.add(",", 0);
                    }
                }
                list.add("\t];", line);
                list.add("\tfor (var i:int = 0; i < cd.length; i++) cd[i].document = this;", line);
            }
        }

        if self_model.get_type().is_assignable_to_name(Some(&sd.interface_imxmlobject)) || self_model.get_type().has_metadata(deps::MD_IMXMLOBJECT, true) {
            let id_param = if is_declared { text_gen::quote_word(&id) } else { "null".to_string() };
            list.add5("\t", var_name, ".initialized(this, ", &id_param, ")", line);
        }

        if is_declared {
            if self_model.get_repeater_level() == 0 {
                list.add5("\t", &id, " = ", var_name, ";", line);
            } else {
                toolkit::log(
                    CompilerMessage::error("Unable to generate initialization code within Repeater, due to id or data binding on a component that is not a visual child."),
                    Some(&self_model.get_document().get_source_path()),
                    line,
                    -1,
                    None,
                );
            }
            list.add5("\t", &name_formatter::to_dot(&sd.class_bindingmanager), ".executeBindings(this, ", &text_gen::quote_word(&id), &format!(", {id});"), line);
        }

        if sd.is_container(&self_type) && self_model.is_early_init() {
            list.add3("\t", var_name, ".initialize();", line);
        }

        list.add3("\treturn ", var_name, ";", line);
        list.add("}", line);

        let lvalue_type = self.get_lvalue_type();
        if sd.is_itransient_deferred_instance(&lvalue_type) || self_model.get_is_transient() {
            self.get_destructor_body(&mut list, line);
        }
        list
    }

    pub(crate) fn collect_declarations(initializers: &[InitializerRef], ids: &mut IndexSet<String>) {
        for initializer in initializers {
            if let Some(model) = initializer.as_value().and_then(|v| v.get_model()) {
                if model.is_declared() {
                    ids.insert(model.get_id().unwrap_or_default());
                }
                Self::collect_declarations(&model.get_sub_initializer_iterator(), ids);
            }
        }
    }

    fn get_destructor_body(&self, list: &mut CodeFragmentList, line: i32) {
        let model = self.get_model().expect("ClassCastException");
        let mut ids = IndexSet::new();
        if model.is_declared() {
            ids.insert(model.get_id().unwrap_or_default());
        }
        Self::collect_declarations(&model.get_sub_initializer_iterator(), &mut ids);
        list.add3("\nprivate function ", &format!("{}_r", model.get_definition_name().unwrap_or_default()), "() : void", line);
        list.add("{", line);
        for id in &ids {
            list.add3("\t", id, " = null;", 0);
        }
        list.add("}", line);
    }

    /// `getDefinitionsIterator()`: our definition, if any, then our rvalue's sub-definitions.
    pub fn get_definitions_iterator(&self) -> Vec<CodeFragmentList> {
        let mut iter_list = Vec::new();
        if self.has_definition() {
            let described = self.get_model().is_some_and(|m| m.is_described());
            if !described {
                iter_list.push(self.get_definition_body());
            }
        }
        if let Some(model) = self.get_model() {
            iter_list.extend(model.get_sub_definitions_iterator());
        }
        iter_list
    }

    pub(crate) fn get_type_table(&self) -> Rc<TypeTable> {
        self.get_lvalue_type().get_type_table()
    }

    /// `formatExpr(targetType, value)`.
    pub(crate) fn format_expr(&self, target_type: &Type, value: &RValue) -> String {
        let type_table = self.get_type_table();
        let tt = &*type_table;
        let is_scalar = |t: &Type| type_eq(t, &tt.boolean_type()) || type_eq(t, &tt.number_type()) || type_eq(t, &tt.int_type()) || type_eq(t, &tt.uint_type());
        match value {
            RValue::Binding(_) => return if is_scalar(target_type) { deps::UNDEFINED.to_string() } else { deps::NULL.to_string() },
            RValue::AtEmbed(e) => return e.get_prop_name().to_string(),
            RValue::AtResource(r) => return r.get_value_expression(),
            _ => {}
        }
        if type_eq(target_type, &tt.string_type()) {
            format_string(&value.to_java_string())
        } else if is_scalar(target_type) {
            value.to_java_string()
        } else if type_eq(target_type, &tt.object_type()) || type_eq(target_type, &tt.no_type()) {
            match value {
                RValue::Str(s) => format_string(s),
                RValue::Integer(_) | RValue::Long(_) | RValue::Double(_) | RValue::Boolean(_) => value.to_java_string(),
                _ => panic!("formatExpr: unsupported rvalue type for lvalue type 'Object'"),
            }
        } else if type_eq(target_type, &tt.class_type()) || type_eq(target_type, &tt.function_type()) || type_eq(target_type, &tt.reg_exp_type()) {
            value.to_java_string()
        } else if type_eq(target_type, &tt.xml_type()) {
            self.as_xml_literal(value.as_model().expect("ClassCastException"))
        } else if type_eq(target_type, &tt.xml_list_type()) {
            as_xml_list(value.as_model().expect("ClassCastException"))
        } else {
            panic!("formatExpr: unsupported lvalue type: {}", target_type.get_name())
        }
    }

    fn as_xml_literal(&self, component: &ModelRef) -> String {
        let ModelKind::Xml(x) = &component.kind else { panic!("ClassCastException") };
        let xml = x.get_literal_xml().unwrap_or_default();
        if x.get_is_e4x() {
            xml
        } else {
            format!("{}.createXMLDocument(\"{}\").firstChild", name_formatter::to_dot(&self.standard_defs.class_xmlutil), fixup_xml_string(&xml))
        }
    }

    pub fn get_comment(&self) -> Option<String> {
        self.get_model().and_then(|m| m.comment.borrow().clone())
    }

    /// `getAssignExpr(lvalueBase)`, per subclass.
    pub fn get_assign_expr(&self, lvalue_base: &str) -> String {
        match &self.kind {
            ValueInitializerKind::Style { .. } | ValueInitializerKind::Effect { .. } => {
                format!("{lvalue_base}.setStyle({}, {})", text_gen::quote_word(&self.get_name().unwrap_or_default()), self.get_value_expr())
            }
            ValueInitializerKind::ArrayElement { index, .. } => format!("{lvalue_base}[{index}] = {}", self.get_value_expr()),
            ValueInitializerKind::VisualChild => self.visual_child_assign_expr(lvalue_base),
            ValueInitializerKind::InitializedPropertyDeclaration { .. } => self.initialized_property_declaration_assign_expr(lvalue_base),
            _ => self.named_assign_expr(lvalue_base),
        }
    }

    /// `NamedInitializer.getAssignExpr(lvalueBase)`.
    pub(crate) fn named_assign_expr(&self, lvalue_base: &str) -> String {
        let name = self.get_name().unwrap_or_default();
        let lvalue = if deps::is_valid_identifier(&name) { format!("{lvalue_base}.{name}") } else { format!("{lvalue_base}[{}]", text_gen::quote_word(&name)) };
        format!("{lvalue} = {}", self.get_value_expr())
    }
}

/// `addAssignExprs(list, initIter, name)`.
fn add_assign_exprs(list: &mut CodeFragmentList, inits: &[InitializerRef], name: &str) {
    for init in inits {
        if !init.is_state_specific() {
            list.add3("\t", &init.get_assign_expr(name), ";", init.get_line_ref());
        }
    }
}

fn as_array_literal(array: &ModelRef) -> String {
    let ModelKind::Array(a) = &array.kind else { unreachable!() };
    let elements: Vec<String> = a.get_element_initializer_iterator().iter().filter(|i| !i.is_state_specific()).map(|i| i.get_value_expr()).collect();
    format!("[{}]", text_gen::to_comma_list(elements))
}

fn as_object_literal(model: &ModelRef) -> String {
    let pairs: Vec<String> = model.get_property_initializer_iterator().iter().map(|i| format!("{}: {}", i.get_name(), i.get_value_expr())).collect();
    format!("{{{}}}", text_gen::to_comma_list(pairs))
}

fn as_vector_literal(vector: &ModelRef) -> String {
    let ModelKind::Array(a) = &vector.kind else { unreachable!() };
    let elements: Vec<String> = a.get_element_initializer_iterator().iter().filter(|i| !i.is_state_specific()).map(|i| i.get_value_expr()).collect();
    format!("new <{}>[{}]", a.get_element_type_name(), text_gen::to_comma_list(elements))
}

fn fixup_xml_string(orig: &str) -> String {
    let mut result = String::with_capacity(orig.len());
    for c in orig.chars() {
        match c {
            '\r' => continue,
            '\n' => {
                result.push_str("\\n");
                continue;
            }
            '"' => result.push('\\'),
            _ => {}
        }
        result.push(c);
    }
    result
}

fn as_xml_list(component: &ModelRef) -> String {
    let ModelKind::XmlList(x) = &component.kind else { panic!("ClassCastException") };
    format!("<>{}</>", x.get_literal_xml().unwrap_or_default())
}
