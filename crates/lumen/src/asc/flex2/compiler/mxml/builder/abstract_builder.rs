//! Port of `flex2.compiler.mxml.builder.AbstractBuilder` (text path): the state every builder shares
//! ([`AbstractBuilder`]), its `TextValueParser`, its `RValueNodeHandler`, and the protected methods as the
//! [`Builder`] trait's defaults, which the Java subclasses' overrides replace per builder.

use std::rc::Rc;

use super::array_builder::ArrayBuilder;
use super::component_builder::ComponentBuilder;
use super::inline_component_builder::InlineComponentBuilder;
use super::model_builder::ModelBuilder;
use super::primitive_builder::PrimitiveBuilder;
use super::vector_builder::VectorBuilder;
use super::xml_builder::XmlBuilder;
use super::xml_list_builder::XmlListBuilder;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeClass, NodeRef};
use crate::asc::flex2::compiler::mxml::lang::framework_defs;
use crate::asc::flex2::compiler::mxml::lang::node_type_resolver::NodeTypeResolver;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::{self as sd, StandardDefs};
use crate::asc::flex2::compiler::mxml::lang::text_parser::{self, TextParser};
use crate::asc::flex2::compiler::mxml::lang::type_compatibility;
use crate::asc::flex2::compiler::mxml::lang::value_node_handler::ValueNodeHandler;
use crate::asc::flex2::compiler::mxml::mxml_configuration::MxmlConfiguration;
use crate::asc::flex2::compiler::mxml::mxml_namespaces::FXG_2008_NAMESPACE;
use crate::asc::flex2::compiler::mxml::reflect::{Assignable, DynamicPropertyRef, EffectRef, EventRef, PropertyRef, Stateful, StyleRef, TypeRef, TypeTable};
use crate::asc::flex2::compiler::mxml::rep::at_clear::AtClear;
use crate::asc::flex2::compiler::mxml::rep::at_embed;
use crate::asc::flex2::compiler::mxml::rep::at_resource::AtResource;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::{BindingExpression, BindingExpressionRef};
use crate::asc::flex2::compiler::mxml::rep::document_info::OUTER_DOCUMENT_PROP;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::util::jstr;

/// `CompilerMessage` text from `compiler_en.properties`: `${name}` replaced by its parameter, or nothing.
pub fn msg(text: &str, params: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find('}') {
            Some(j) => {
                let name = &after[..j];
                out.push_str(params.iter().find(|(k, _)| *k == name).map_or("", |(_, v)| v));
                rest = &after[j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn err(text: &str, params: &[(&str, &str)]) -> CompilerMessage {
    CompilerMessage::error(msg(text, params))
}

pub fn warn(text: &str, params: &[(&str, &str)]) -> CompilerMessage {
    CompilerMessage::warning(msg(text, params))
}

fn dot(t: &TypeRef) -> String {
    name_formatter::to_dot(&t.get_name())
}

fn pname(p: &PropertyRef) -> String {
    p.get_name().unwrap_or("null").to_string()
}

/// `AbstractBuilder.TextOrigin`.
pub const FROM_ATTRIBUTE: i32 = 0;
pub const FROM_CHILD_TEXT: i32 = 1;
pub const FROM_CHILD_CDATA: i32 = 2;

pub fn from_child(cdata: bool) -> i32 {
    if cdata {
        FROM_CHILD_CDATA
    } else {
        FROM_CHILD_TEXT
    }
}

/// The constructor arguments every builder takes: `(unit, typeTable, mxmlConfiguration, document)`.
#[derive(Clone)]
pub struct BuilderContext {
    pub unit: UnitRef,
    pub type_table: Rc<TypeTable>,
    pub mxml_configuration: Rc<MxmlConfiguration>,
    pub document: MxmlDocumentRef,
}

/// The `BindingHandler`s of the builders.
#[derive(Clone)]
pub enum BindingHandlerKind {
    /// `ComponentBuilder.ComponentDeclarationBindingHandler`.
    ComponentDeclaration,
    /// `DocumentBuilder.PrimitiveDeclarationBindingHandler`.
    PrimitiveDeclaration,
    /// `ArrayBuilder`/`VectorBuilder.ElementBindingHandler` of this array.
    ArrayElement(ModelRef),
}

impl BindingHandlerKind {
    pub fn invoke(&self, binding_expression: &BindingExpressionRef, dest: &ModelRef) -> BindingExpressionRef {
        match self {
            BindingHandlerKind::ComponentDeclaration => binding_expression.set_destination(dest),
            BindingHandlerKind::PrimitiveDeclaration => {
                binding_expression.set_destination(dest);
                dest.set_value(None);
            }
            BindingHandlerKind::ArrayElement(array) => {
                binding_expression.set_destination(array);
                binding_expression.set_destination_lvalue(Some(&array.size().to_string()));
                binding_expression.set_destination_property_index(array.size() as i32);
            }
        }
        binding_expression.clone()
    }
}

/// `AbstractBuilder.TextValueParser`; `root` makes it `DocumentBuilder.RootAttributeParser`.
pub struct TextValueParser {
    ctx: BuilderContext,
    standard_defs: Rc<StandardDefs>,
    ignore_two_way_binding: bool,
    pub lvalue_name: Option<String>,
    pub line: i32,
    pub desc: Option<String>,
    pub was_percentage: bool,
    pub root: bool,
}

impl TextValueParser {
    pub fn new(ctx: &BuilderContext, standard_defs: Rc<StandardDefs>) -> TextValueParser {
        TextValueParser {
            ctx: ctx.clone(),
            standard_defs,
            ignore_two_way_binding: text_parser::ignore_two_way_binding_for(ctx.mxml_configuration.get_compatibility_version()),
            lvalue_name: None,
            line: 0,
            desc: None,
            was_percentage: false,
            root: false,
        }
    }

    fn path(&self) -> String {
        self.ctx.unit.borrow().get_source().borrow().get_name_for_reporting().to_string()
    }

    fn log(&self, line: i32, m: CompilerMessage) {
        toolkit::log(m, Some(&self.path()), line, -1, None);
    }

    /// `parseValue(text, type, flags, line, desc)`.
    pub fn parse_value_simple(&mut self, text: &str, t: &TypeRef, flags: i32, line: i32, desc: &str) -> Option<RValue> {
        let object_type = self.ctx.type_table.object_type();
        self.parse_value_elem(text, t, Some(&object_type), flags, line, desc)
    }

    /// `parseValue(text, type, elementType, flags, line, desc)`.
    pub fn parse_value_elem(&mut self, text: &str, t: &TypeRef, element_type: Option<&TypeRef>, mut flags: i32, line: i32, desc: &str) -> Option<RValue> {
        self.line = line;
        self.desc = Some(desc.to_string());
        self.was_percentage = false;
        if self.ctx.document.get_language_namespace().as_deref() == Some(FXG_2008_NAMESPACE) {
            flags |= text_parser::FLAG_IGNORE_BINDING;
            flags |= text_parser::FLAG_IGNORE_ARRAY_SYNTAX;
            flags |= text_parser::FLAG_IGNORE_AT_FUNCTION_ESCAPE;
        }
        self.parse(text, t, element_type, flags)
    }

    pub fn was_percentage(&self) -> bool {
        self.was_percentage
    }

    /// `RootAttributeParser.parseUInt` etc.
    pub fn parse_uint(&mut self, text: &str, line: i32, name: &str) -> Option<RValue> {
        let t = self.ctx.type_table.uint_type();
        self.parse_value_simple(text, &t, 0, line, name)
    }

    pub fn parse_color(&mut self, text: &str, line: i32, name: &str) -> Option<RValue> {
        let t = self.ctx.type_table.uint_type();
        self.parse_value_simple(text, &t, text_parser::FLAG_CONVERT_COLOR_NAMES, line, name)
    }

    pub fn parse_number_or_percentage(&mut self, text: &str, line: i32, name: &str) -> Option<RValue> {
        let t = self.ctx.type_table.number_type();
        self.parse_value_simple(text, &t, text_parser::FLAG_ALLOW_PERCENTAGES, line, name)
    }

    pub fn parse_boolean(&mut self, text: &str, line: i32, name: &str) -> Option<RValue> {
        let t = self.ctx.type_table.boolean_type();
        self.parse_value_simple(text, &t, 0, line, name)
    }
}

impl TextParser for TextValueParser {
    fn type_table(&self) -> Rc<TypeTable> {
        self.ctx.type_table.clone()
    }

    fn ignore_two_way_binding(&self) -> bool {
        self.ignore_two_way_binding
    }

    fn context_root(&mut self, text: &str) -> Option<RValue> {
        match self.ctx.mxml_configuration.get_context_root() {
            None => {
                self.error(text_parser::ERR_UNDEFINED_CONTEXT_ROOT, text, None, None);
                None
            }
            Some(root) => Some(RValue::Str(regex::Regex::new(r"@ContextRoot\(\)").unwrap().replace_all(text, regex::NoExpand(root)).into_owned())),
        }
    }

    fn clear(&mut self) -> Option<RValue> {
        if self.root {
            self.log(self.line, CompilerMessage::error("@Clear not allowed here."));
            return None;
        }
        Some(RValue::AtClear(Rc::new(AtClear::new(self.line))))
    }

    fn embed(&mut self, text: &str, t: &TypeRef) -> Option<RValue> {
        if self.root {
            self.log(self.line, CompilerMessage::error("@Embed not allowed here."));
            return None;
        }
        let tt = self.ctx.type_table.clone();
        let str_type = t.is_assignable_to(Some(&tt.string_type()));
        let source = self.ctx.unit.borrow().get_source();
        let at_embed = at_embed::create(&tt.get_per_compile_data(), &source, self.line, text, str_type)?;
        self.ctx.document.add_at_embed(&at_embed);
        if self.standard_defs.is_ifactory(t) {
            return Some(RValue::Model(factory_from_class(&self.ctx, &self.standard_defs, at_embed.get_prop_name(), self.line)));
        } else if self.standard_defs.is_ideferred_instance(t) {
            return Some(RValue::Model(instance_from_class(&self.ctx, at_embed.get_prop_name(), self.line, false)));
        }
        Some(RValue::AtEmbed(at_embed))
    }

    fn resource(&mut self, text: &str, t: &TypeRef) -> Option<RValue> {
        let source = self.ctx.unit.borrow().get_source();
        let at_resource = AtResource::create(&self.ctx.type_table, &source, self.line, text, t)?;
        self.ctx.document.add_at_resource(&at_resource);
        Some(RValue::AtResource(at_resource))
    }

    fn binding_expression(&mut self, converted: &str) -> Option<BindingExpressionRef> {
        if self.root {
            self.log(self.line, CompilerMessage::error("Data binding expression not allowed here."));
            return None;
        }
        self.binding_expression_two_way(converted, false)
    }

    fn binding_expression_two_way(&mut self, converted: &str, is_two_way: bool) -> Option<BindingExpressionRef> {
        let be = BindingExpression::new(converted, self.line, &self.ctx.document);
        be.set_two_way_primary(is_two_way);
        Some(be)
    }

    fn parse_binding_expression_line(&mut self, text: &str, line: i32) -> Option<BindingExpressionRef> {
        self.line = line;
        self.desc = None;
        self.parse_binding_expression(text)
    }

    fn percentage(&mut self, pct: &str) -> Option<RValue> {
        self.was_percentage = true;
        let n = jstr::substring(pct, 0, jstr::index_of_unit(pct, b'%' as u16, 0));
        text_parser::java_double_value_of(&n).map(RValue::Double)
    }

    fn array(&mut self, entries: Vec<RValue>, array_element_type: Option<&TypeRef>) -> Option<RValue> {
        let array = Model::new_array(&self.ctx.document, None, self.line, array_element_type.cloned());
        array.add_entries(entries, self.line);
        Some(RValue::Model(array))
    }

    fn function_text(&mut self, name: &str) -> Option<RValue> {
        Some(RValue::Str(name.to_string()))
    }

    fn class_name(&mut self, name: &str, lvalue_type: &TypeRef) -> Option<RValue> {
        self.ctx.document.add_import(name, self.line);
        if self.standard_defs.is_ifactory(lvalue_type) {
            Some(RValue::Model(factory_from_class(&self.ctx, &self.standard_defs, name, self.line)))
        } else if self.standard_defs.is_ideferred_instance(lvalue_type) {
            Some(RValue::Model(instance_from_class(&self.ctx, name, self.line, true)))
        } else {
            Some(RValue::Str(name.to_string()))
        }
    }

    fn error(&mut self, error: i32, text: &str, t: Option<&TypeRef>, element_type: Option<&TypeRef>) {
        let desc = self.desc.clone().unwrap_or_else(|| "null".into());
        let tname = t.map(dot).unwrap_or_else(|| "null".into());
        let line = self.line;
        match error {
            text_parser::ERR_TYPE_NOT_CONTEXT_ROOTABLE => {
                self.log(line, err("Initializer for '${desc}': @ContextRoot may not be used with type ${type}.", &[("desc", &desc), ("type", &tname)]))
            }
            text_parser::ERR_UNDEFINED_CONTEXT_ROOT => self.log(line, err("Undefined context root.", &[])),
            text_parser::ERR_TYPE_NOT_EMBEDDABLE => self.log(line, err("Initializer for '${desc}': @Embed may not be used with type ${type}.", &[("desc", &desc), ("type", &tname)])),
            text_parser::ERR_INVALID_TEXT_FOR_TYPE => {
                let array = match t {
                    Some(t) if **t == *self.ctx.type_table.array_type() => format!("[{}]", element_type.map(dot).unwrap_or_else(|| "null".into())),
                    _ => String::new(),
                };
                self.log(line, err("Initializer for '${desc}': cannot parse value of type ${type}${array} from text '${text}'.", &[("desc", &desc), ("type", &tname), ("array", &array), ("text", text)]))
            }
            text_parser::ERR_INVALID_PERCENTAGE => self.log(line, err("Initializer for '${desc}': invalid percentage expression: ${text}.", &[("desc", &desc), ("text", text)])),
            text_parser::ERR_TYPE_NOT_SERIALIZABLE => {
                self.log(line, err("Initializer for '${desc}': values of type ${type} cannot be represented in text.", &[("desc", &desc), ("type", &tname)]))
            }
            text_parser::ERR_PERCENTAGES_NOT_ALLOWED => self.log(line, err("Initializer for '${desc}': percentages are not allowed here.", &[("desc", &desc)])),
            text_parser::ERR_UNRECOGNIZED_AT_FUNCTION => self.log(line, err("Initializer for '${desc}': unrecognized compile-time directive.", &[("desc", &desc)])),
            text_parser::ERR_INVALID_TWO_WAY_BIND => {
                if self.desc.is_some() {
                    self.log(line, err("Initializer for '${desc}': invalid two-way binding expression: ${text}.", &[("desc", &desc), ("text", text)]));
                } else {
                    self.log(line, err("Invalid two-way binding expression: ${text}.", &[("text", text)]));
                }
            }
            _ => {}
        }
    }
}

/// `instanceFromClass(className, line, checkClass)`.
pub fn instance_from_class(ctx: &BuilderContext, class_name: &str, line: i32, check_class: bool) -> ModelRef {
    let tt = &ctx.type_table;
    if check_class && tt.get_type(&name_formatter::to_colon(class_name)).is_none() {
        let path = ctx.unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
        toolkit::log(err("Class '${className}' not available.", &[("className", class_name)]), Some(&path), line, -1, None);
    }
    Model::new_primitive_value(&ctx.document, tt.class_type(), &RValue::Str(class_name.to_string()), line)
}

/// `factoryFromClass(className, line)`.
pub fn factory_from_class(ctx: &BuilderContext, standard_defs: &StandardDefs, class_name: &str, line: i32) -> ModelRef {
    let tt = &ctx.type_table;
    let doc = &ctx.document;
    let Some(class_factory_type) = tt.get_type(&standard_defs.class_classfactory) else {
        let path = ctx.unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
        toolkit::log(err("Standard MXML type '${type}' not available.", &[("type", &standard_defs.class_classfactory)]), Some(&path), line, -1, None);
        return Model::new(doc, tt.object_type(), line);
    };
    let model = Model::new(doc, class_factory_type, line);
    model.set_property_model(sd::PROP_CLASSFACTORY_GENERATOR, &Model::new_primitive_value(doc, tt.class_type(), &RValue::Str(class_name.to_string()), line));
    if let Some(class_type) = tt.get_type(&name_formatter::to_colon(class_name)) {
        let mut prop_object = None;
        if let Some(outer_document_property) = class_type.get_property(OUTER_DOCUMENT_PROP) {
            let qual_name = doc.get_qname().to_string();
            let self_type = tt.get_type(&qual_name).expect("skeleton type not available");
            if self_type.is_assignable_to(outer_document_property.get_type().as_deref()) {
                let p = Model::new(doc, tt.object_type(), line);
                p.set_property_property(&outer_document_property, RValue::Model(Model::new_primitive_value(doc, tt.class_type(), &RValue::Str("this".into()), line)), line);
                prop_object = Some(p);
            }
        }
        if let Some(p) = prop_object {
            model.set_property_model(sd::PROP_CLASSFACTORY_PROPERTIES, &p);
        }
    }
    model
}

/// `checkDeprecation(property, path, line)`.
pub fn check_deprecation(property: &PropertyRef, path: &str, line: i32) -> bool {
    if let Some(deprecated) = property.get_deprecated() {
        let name = pname(property);
        if !check_log_deprecation_warning(path, line, &name, deprecated.get_since(), deprecated.get_message(), deprecated.get_replacement()) {
            toolkit::log(warn("'${name}' has been deprecated.", &[("name", &name)]), Some(path), line, -1, None);
        }
        return false;
    }
    true
}

fn check_log_deprecation_warning(path: &str, line: i32, name: &str, since: Option<&str>, message: Option<&str>, replacement: Option<&str>) -> bool {
    let has = |s: Option<&str>| s.is_some_and(|s| !s.is_empty());
    let log = |m: CompilerMessage| toolkit::log(m, Some(path), line, -1, None);
    if has(message) {
        log(warn("${deprecationMessage}", &[("deprecationMessage", message.unwrap())]));
    } else if has(replacement) {
        if has(since) {
            log(warn("'${name}' has been deprecated since ${since}.  Please use '${replacement}'.", &[("name", name), ("since", since.unwrap()), ("replacement", replacement.unwrap())]));
        } else {
            log(warn("'${name}' has been deprecated.  Please use '${replacement}'.", &[("name", name), ("replacement", replacement.unwrap())]));
        }
    } else if has(since) {
        log(warn("'${name}' has been deprecated since ${since}.", &[("name", name), ("since", since.unwrap())]));
    } else if message.is_some() || replacement.is_some() || since.is_some() {
        log(warn("'${name}' has been deprecated.", &[("name", name)]));
    } else {
        return false;
    }
    true
}

/// The fields of `AbstractBuilder` (and `AnalyzerAdapter`).
pub struct AbstractBuilder {
    pub adapter: AnalyzerAdapter,
    pub ctx: BuilderContext,
    pub standard_defs: Rc<StandardDefs>,
    pub type_table: Rc<TypeTable>,
    pub document: MxmlDocumentRef,
    pub mxml_configuration: Rc<MxmlConfiguration>,
    pub node_type_resolver: NodeTypeResolver,
    pub text_parser: TextValueParser,
}

impl AbstractBuilder {
    pub fn new(ctx: &BuilderContext) -> AbstractBuilder {
        let standard_defs = ctx.document.get_standard_defs();
        AbstractBuilder {
            adapter: AnalyzerAdapter::new(ctx.unit.clone()),
            ctx: ctx.clone(),
            standard_defs: standard_defs.clone(),
            type_table: ctx.type_table.clone(),
            document: ctx.document.clone(),
            mxml_configuration: ctx.mxml_configuration.clone(),
            node_type_resolver: NodeTypeResolver::new(ctx.type_table.clone()),
            text_parser: TextValueParser::new(ctx, standard_defs),
        }
    }

    pub fn path(&self) -> String {
        self.ctx.unit.borrow().get_source().borrow().get_name_for_reporting().to_string()
    }

    /// `log(line, msg)`: against the current node when `line` is 0.
    pub fn log_line(&self, line: i32, m: CompilerMessage) {
        let line = if line == 0 { self.adapter.current_node().map_or(0, |n| n.borrow().begin_line) } else { line };
        toolkit::log(m, Some(&self.path()), line, -1, None);
    }

    /// `log(node, msg)`.
    pub fn log_node(&self, node: &NodeRef, m: CompilerMessage) {
        let line = node.borrow().begin_line;
        toolkit::log(m, Some(&self.path()), line, -1, None);
    }

    /// `log(msg)`: against the current node.
    pub fn log_current(&self, m: CompilerMessage) {
        let line = self.adapter.current_node().map_or(0, |n| n.borrow().begin_line);
        toolkit::log(m, Some(&self.path()), line, -1, None);
    }

    /// `logError(node, message)`.
    pub fn log_error_node(&self, node: &NodeRef, message: &str) {
        toolkit::log_path_line(Level::Error, &self.path(), node.borrow().begin_line, message);
    }

    pub fn get_document_version(&self) -> i32 {
        self.document.get_version()
    }

    pub fn get_language_namespace(&self) -> Option<String> {
        self.document.get_language_namespace()
    }

    pub fn resolve_type(&mut self, node: &NodeRef) -> Option<TypeRef> {
        let doc = self.document.clone();
        self.node_type_resolver.resolve_type(node, Some(&doc))
    }

    pub fn get_is_color_property(&self, property: &PropertyRef) -> bool {
        property.get_inspectable().and_then(|i| i.get_format().map(|f| f == sd::MDPARAM_INSPECTABLE_FORMAT_COLOR)).unwrap_or(false)
    }

    pub fn get_is_color_style(&self, style: &StyleRef) -> bool {
        style.get_format().is_some_and(|f| f == sd::MDPARAM_STYLE_FORMAT_COLOR)
    }

    /// `getPercentProxy(type, property, line)`.
    pub fn get_percent_proxy(&self, t: &TypeRef, property: &PropertyRef, line: i32) -> Option<PropertyRef> {
        let percent_proxy_name = property.get_percent_proxy()?;
        match t.get_property(&percent_proxy_name) {
            Some(percent_proxy) => {
                percent_proxy.set_state_name(property.get_state_name().as_deref());
                Some(percent_proxy)
            }
            None => {
                self.log_line(
                    line,
                    warn("[PercentProxy(\"${percentProxyName}\")] on property '${property}' does not name a reachable property of class '${type}'.", &[("property", &pname(property)), ("type", &dot(t))]),
                );
                None
            }
        }
    }

    fn post_process_binding_expression(&self, value: &RValue, model: &ModelRef, name: &str) {
        if let RValue::Binding(be) = value {
            be.set_destination(model);
            be.set_destination_lvalue(Some(name));
            be.set_destination_property(Some(name));
        }
    }

    fn ensure_single_initializer(&self, model: &ModelRef, name: &str, line: i32, state: Option<&str>) {
        if model.has_property(name) && state.is_none() {
            let t = model.get_type().clone();
            match t.get_default_property() {
                Some(dp) if pname(&dp) == name => {
                    self.log_line(line, err("Multiple initializers for property '${name}'. (note: '${name}' is the default property of '${type}').", &[("name", name), ("type", &dot(&t))]))
                }
                _ => self.log_line(line, err("Multiple initializers for property '${name}'.", &[("name", name)])),
            }
        }
    }

    /// `processEventText(event, text, line, model)`.
    pub fn process_event_text(&self, event: &EventRef, text: &str, line: i32, model: &ModelRef) {
        if !text.is_empty() {
            let Some(event_type) = event.get_type() else {
                self.log_line(line, err("Event type '${type}' is unavailable.", &[("type", event.get_type_name())]));
                return;
            };
            self.document.add_import(&dot(&event_type), line);
            if text == "@Clear()" && !event.is_state_specific() {
                self.log_line(line, err("@Clear may only be used with state-specific properties.", &[]));
                return;
            }
            model.set_event(event, text, line);
        } else {
            self.log_line(line, warn("Event handler is empty.", &[]));
        }
    }

    pub fn check_style_usage(&self, style: &StyleRef, text: &str, line: i32) -> bool {
        self.check_image_type(style.get_format(), text, line);
        if !text_parser::is_binding_expression(text) && text_parser::get_at_function_name(text).is_none() && !self.check_enumeration(style.get_enumeration(), text, line) {
            return false;
        }
        if style.get_type().is_none() {
            self.log_line(line, err("Internal error: type for Style '{name}' is unreachable.", &[]));
            return false;
        }
        true
    }

    pub fn check_enumeration(&self, enums: Option<&[String]>, value: &str, line: i32) -> bool {
        match enums {
            Some(enums) => {
                if enums.iter().any(|e| e == value) {
                    return true;
                }
                self.log_line(line, err("Invalid value:  ${value}.  It must be one of ${values}.", &[("value", value), ("values", &enums.join(", "))]));
                false
            }
            None => true,
        }
    }

    pub fn check_image_type(&self, format: Option<&str>, value: &str, line: i32) -> bool {
        if format == Some("File") && value.ends_with(".svg") {
            self.log_line(line, warn("The Flash player does not support loading SVG at runtime.  To embed SVG use the @Embed() compile time function.", &[]));
            return false;
        }
        true
    }

    fn check_log_deprecation(&self, name: &str, since: Option<&str>, message: Option<&str>, replacement: Option<&str>, line: i32) {
        if self.mxml_configuration.show_deprecation_warnings() {
            check_log_deprecation_warning(&self.document.get_source_path(), line, name, since, message, replacement);
        }
    }

    pub fn check_event_deprecation(&self, event: &EventRef, line: i32) {
        self.check_log_deprecation(event.get_name().unwrap_or("null"), event.get_deprecated_since(), event.get_deprecated_message(), event.get_deprecated_replacement(), line);
    }

    pub fn check_effect_deprecation(&self, effect: &EffectRef, line: i32) {
        self.check_log_deprecation(effect.get_name(), effect.get_deprecated_since(), effect.get_deprecated_message(), effect.get_deprecated_replacement(), line);
    }

    pub fn check_style_deprecation(&self, style: &StyleRef, line: i32) {
        self.check_log_deprecation(style.get_name(), style.get_deprecated_since(), style.get_deprecated_message(), style.get_deprecated_replacement(), line);
    }

    /// `checkNonEmpty(node, type)`.
    pub fn check_non_empty(&self, node: &NodeRef, t: &TypeRef) -> bool {
        if node.borrow().get_child_count() == 0 {
            if !self.allow_empty_default(t) {
                self.log_line(node.borrow().begin_line, err("Child tag initializer for type '${type}' may not be empty.", &[("type", &dot(t))]));
            }
            false
        } else {
            true
        }
    }

    pub fn allow_empty_default(&self, t: &TypeRef) -> bool {
        self.type_table.string_type().is_assignable_to(Some(t)) || self.type_table.array_type().is_assignable_to(Some(t))
    }

    /// `registerModel(id, model, topLevel)`.
    pub fn register_model_id(&self, id: Option<&str>, model: &ModelRef, top_level: bool) {
        if let Some(id) = id {
            model.set_id(Some(id), false);
            self.document.add_declaration(model, top_level);
        } else if top_level {
            self.document.add_declaration(model, true);
        } else if model.is_declaration_ensured() && model.get_id_is_autogenerated() {
            self.document.add_declaration_named(&model.get_id().unwrap_or_default(), &model.get_type().get_name(), model.get_xml_line_number(), true, true, true, model.get_bindability_ensured(), None);
        }
    }
}

/// A node's attribute value as Java's `(String) getAttributeValue(...)`.
pub fn attr_str(v: Option<crate::asc::flex2::compiler::mxml::attribute::AttrValue>) -> Option<String> {
    v.and_then(|v| v.as_str().map(str::to_string))
}

/// `AbstractBuilder`'s protected methods; implementors supply the Java overrides.
pub trait Builder: Analyzer {
    fn ab(&mut self) -> &mut AbstractBuilder;
    fn abr(&self) -> &AbstractBuilder;

    fn is_allowed_property(&mut self, _property: &PropertyRef) -> bool {
        true
    }

    fn is_special_attribute(&self, _namespace: &str, _local_part: &str) -> bool {
        false
    }

    /// Whether `checkTypeCompatibility` is `DocumentBuilder`'s override.
    fn is_document_builder(&self) -> bool {
        false
    }

    /// `processPropertyText(property, text, origin, line, model)`.
    fn process_property_text(&mut self, property: &PropertyRef, text: &str, origin: i32, line: i32, model: &ModelRef) -> bool {
        let name = pname(property);
        self.abr().ensure_single_initializer(model, &name, line, property.get_state_name().as_deref());
        if !self.check_property_usage(property, text, line) {
            return false;
        }
        let ab = self.abr();
        let flags = (if origin == FROM_CHILD_CDATA { text_parser::FLAG_IN_CDATA } else { 0 })
            | (if ab.get_is_color_property(property) { text_parser::FLAG_CONVERT_COLOR_NAMES } else { 0 })
            | (if origin != FROM_ATTRIBUTE && property.collapse_white_space() { text_parser::FLAG_COLLAPSE_WHITE_SPACE } else { 0 })
            | (if ab.get_percent_proxy(model.get_type(), property, line).is_some() { text_parser::FLAG_ALLOW_PERCENTAGES } else { 0 })
            | (if property.rich_text_content() { text_parser::FLAG_RICH_TEXT_CONTENT } else { 0 });
        let t = property.get_type().expect("NullPointerException");
        let et = property.get_element_type();
        let value = self.ab().text_parser.parse_value_elem(text, &t, et.as_ref(), flags, line, &name);
        let Some(value) = value else { return false };
        self.abr().post_process_binding_expression(&value, model, &name);
        let mut property = property.clone();
        if self.abr().text_parser.was_percentage() {
            match self.abr().get_percent_proxy(model.get_type(), &property, line) {
                Some(p) => property = p,
                None => panic!("NullPointerException"),
            }
        } else if matches!(value, RValue::AtClear(_)) && !property.is_state_specific() {
            self.abr().log_line(line, err("@Clear may only be used with state-specific properties.", &[]));
            return false;
        }
        model.set_property_property(&property, value, line);
        true
    }

    /// `processPropertySyntheticArray(property, line, model)`.
    fn process_property_synthetic_array(&mut self, property: &PropertyRef, line: i32, model: &ModelRef) {
        let name = pname(property);
        self.abr().ensure_single_initializer(model, &name, line, property.get_state_name().as_deref());
        let mut builder = ArrayBuilder::new(&self.abr().ctx, None, None, true);
        builder.create_synthetic_array_model(line);
        model.set_property_property(property, RValue::Model(builder.array.clone().unwrap()), line);
    }

    /// `processDynamicPropertyText(name, text, origin, line, model, state)`.
    fn process_dynamic_property_text(&mut self, name: &str, text: &str, origin: i32, line: i32, model: &ModelRef, state: Option<&str>) -> bool {
        self.abr().ensure_single_initializer(model, name, line, state);
        let flags = if origin == FROM_CHILD_CDATA { text_parser::FLAG_IN_CDATA } else { 0 };
        let ot = self.abr().type_table.object_type();
        let value = self.ab().text_parser.parse_value_elem(text, &ot, Some(&ot), flags, line, name);
        match value {
            Some(value) => {
                self.abr().post_process_binding_expression(&value, model, name);
                model.set_dynamic_property(&ot, name, value, state, line);
                true
            }
            None => false,
        }
    }

    /// `processStateAttributes(node, model)`.
    fn process_state_attributes(&mut self, node: &NodeRef, model: &ModelRef) -> bool {
        let included_states = attr_str(self.get_language_attribute_value(node, sd::PROP_INCLUDE_STATES));
        let excluded_states = attr_str(self.get_language_attribute_value(node, sd::PROP_EXCLUDE_STATES));
        let item_creation_policy = attr_str(self.get_language_attribute_value(node, sd::PROP_ITEM_CREATION_POLICY));
        let item_destruction_policy = attr_str(self.get_language_attribute_value(node, sd::PROP_ITEM_DESTRUCTION_POLICY));
        let ab = self.abr();
        let not_allowed = "Initializer for '${desc}': data binding expression not allowed here.";
        if let Some(s) = included_states.as_deref().filter(|s| text_parser::is_binding_expression(s)) {
            ab.log_node(node, err(not_allowed, &[("desc", sd::PROP_INCLUDE_STATES), ("text", s)]));
        }
        if let Some(s) = excluded_states.as_deref().filter(|s| text_parser::is_binding_expression(s)) {
            ab.log_node(node, err(not_allowed, &[("desc", sd::PROP_EXCLUDE_STATES), ("text", s)]));
        }
        if included_states.is_some() || excluded_states.is_some() {
            let includes = text_parser::parse_string_list(included_states.as_deref());
            let excludes = text_parser::parse_string_list(excluded_states.as_deref());
            ab.document.register_state_specific_node(model, node, &includes, &excludes);
            if let Some(p) = &item_creation_policy {
                if p == "immediate" {
                    ab.document.register_early_init_node(model);
                } else if p != "deferred" {
                    ab.log_line(model.get_xml_line_number(), err("Invalid value for itemCreationPolicy attribute. ", &[]));
                }
            }
            if let Some(p) = &item_destruction_policy {
                if p == "auto" {
                    model.set_is_transient(true);
                } else if p != "never" {
                    ab.log_line(model.get_xml_line_number(), err("Invalid value for itemDestructionPolicy attribute. ", &[]));
                }
            }
            return true;
        }
        if item_creation_policy.is_some() {
            ab.log_line(model.get_xml_line_number(), err("itemCreationPolicy and itemDestructionPolicy attributes must accompany either the includeIn or excludeFrom attribute.", &[]));
        }
        if item_destruction_policy.is_some() {
            ab.log_line(model.get_xml_line_number(), err("itemDestructionPolicy must accompany either the includeIn or excludeFrom attribute.", &[]));
        }
        false
    }

    /// `processStyleText(style, text, origin, line, model)`.
    fn process_style_text(&mut self, style: &StyleRef, text: &str, origin: i32, line: i32, model: &ModelRef) -> bool {
        let name = style.get_name().to_string();
        let t = style.get_type();
        if !style.is_state_specific() && model.has_style(&name) {
            self.abr().log_line(line, err("Multiple initializers for style '${name}'.", &[("name", &name)]));
        }
        if !self.abr().check_style_usage(style, text, line) {
            return false;
        }
        let flags = (if origin == FROM_CHILD_CDATA { text_parser::FLAG_IN_CDATA } else { 0 }) | (if self.abr().get_is_color_style(style) { text_parser::FLAG_CONVERT_COLOR_NAMES } else { 0 });
        let t = t.expect("NullPointerException");
        let value = self.ab().text_parser.parse_value_simple(text, &t, flags, line, &name);
        let Some(value) = value else { return false };
        match &value {
            RValue::Binding(be) => {
                if be.is_two_way_primary() {
                    self.abr().log_line(line, err("Initializer for '${desc}': two-way data binding expression not allowed here.", &[("desc", &name), ("text", text)]));
                    return false;
                }
                be.set_destination(model);
                be.set_destination_lvalue(Some(&name));
                be.set_destination_style(Some(&name));
            }
            RValue::AtClear(_) => {
                if !style.is_state_specific() {
                    self.abr().log_line(line, err("@Clear directive may only be used for state-specific style properties.", &[]));
                    return false;
                }
            }
            _ => {}
        }
        model.set_style_style(style, value, line);
        true
    }

    /// `processEffectText(effect, text, origin, line, model)`.
    fn process_effect_text(&mut self, effect: &EffectRef, text: &str, origin: i32, line: i32, model: &ModelRef) -> bool {
        let name = effect.get_name().to_string();
        if !effect.is_state_specific() && model.has_effect(&name) {
            self.abr().log_line(line, err("Multiple initializers for effect '${name}'.", &[("name", &name)]));
        }
        let flags = if origin == FROM_CHILD_CDATA { text_parser::FLAG_IN_CDATA } else { 0 };
        let st = self.abr().type_table.string_type();
        let value = self.ab().text_parser.parse_value_simple(text, &st, flags, line, &name);
        let Some(value) = value else { return false };
        if let RValue::Binding(be) = &value {
            if be.is_two_way_primary() {
                self.abr().log_line(line, err("Initializer for '${desc}': two-way data binding expression not allowed here.", &[("desc", &name), ("text", text)]));
                return false;
            }
            be.set_destination(model);
            be.set_destination_style(Some(&name));
            be.set_destination_lvalue(Some(&name));
        } else if framework_defs::is_builtin_effect_name(text) {
            let ab = self.abr();
            ab.document.add_type_ref(&format!("{}.{text}", ab.standard_defs.get_effects_package()), line);
        }
        model.set_effect_effect(effect, value, &st, line);
        true
    }

    /// `processPropertyNodes(parent, property, model)`.
    fn process_property_nodes(&mut self, parent: &NodeRef, property: &PropertyRef, model: &ModelRef) -> bool {
        let (children, line) = {
            let p = parent.borrow();
            (p.get_children(), p.begin_line)
        };
        self.process_property_nodes_list(&children, property, model, line)
    }

    /// `processPropertyNodes(nodes, property, model, line)`.
    fn process_property_nodes_list(&mut self, nodes: &[NodeRef], property: &PropertyRef, model: &ModelRef, line: i32) -> bool {
        if let Some(cdata) = self.get_text_content(nodes, true) {
            let (image, in_cdata, begin_line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.in_cdata(), c.begin_line)
            };
            return self.process_property_text(property, &image, from_child(in_cdata), begin_line, model);
        }
        let name = pname(property);
        self.abr().ensure_single_initializer(model, &name, line, property.get_state_name().as_deref());
        let first_line = nodes[0].borrow().begin_line;
        if !self.check_property_usage(property, "", first_line) {
            return false;
        }
        match self.process_rvalue_nodes(&Assignable::Property(property.clone()), nodes, model) {
            Some(rvalue) => {
                model.set_property_property(property, rvalue, line);
                true
            }
            None => false,
        }
    }

    /// `processDynamicPropertyNodes(parent, property, model)`.
    fn process_dynamic_property_nodes(&mut self, parent: &NodeRef, property: &DynamicPropertyRef, model: &ModelRef) -> bool {
        let nodes = parent.borrow().get_children();
        let name = property.get_name().to_string();
        let state = property.get_state_name();
        if let Some(cdata) = self.get_text_content(&nodes, true) {
            let (image, in_cdata, begin_line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.in_cdata(), c.begin_line)
            };
            return self.process_dynamic_property_text(&name, &image, from_child(in_cdata), begin_line, model, state.as_deref());
        }
        if state.is_none() && model.has_property(&name) {
            self.abr().log_node(parent, err("Multiple initializers for property '${name}'.", &[("name", &name)]));
        }
        match self.process_rvalue_nodes(&Assignable::DynamicProperty(property.clone()), &nodes, model) {
            Some(rvalue) => {
                let ot = self.abr().type_table.object_type();
                model.set_dynamic_property(&ot, &name, rvalue, state.as_deref(), parent.borrow().begin_line);
                true
            }
            None => false,
        }
    }

    /// `processStyleNodes(parent, style, model)`.
    fn process_style_nodes(&mut self, parent: &NodeRef, style: &StyleRef, model: &ModelRef) -> bool {
        let nodes = parent.borrow().get_children();
        if let Some(cdata) = self.get_text_content(&nodes, true) {
            let (image, in_cdata, begin_line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.in_cdata(), c.begin_line)
            };
            return self.process_style_text(style, &image, from_child(in_cdata), begin_line, model);
        }
        let name = style.get_name().to_string();
        if !style.is_state_specific() && model.has_style(&name) {
            self.abr().log_node(parent, err("Multiple initializers for style '${name}'.", &[("name", &name)]));
        }
        if !self.abr().check_style_usage(style, "", nodes[0].borrow().begin_line) {
            return false;
        }
        match self.process_rvalue_nodes(&Assignable::Style(style.clone()), &nodes, model) {
            Some(rvalue) => {
                model.set_style_style(style, rvalue, parent.borrow().begin_line);
                true
            }
            None => false,
        }
    }

    /// `processEffectNodes(parent, effect, model)`.
    fn process_effect_nodes(&mut self, parent: &NodeRef, effect: &EffectRef, model: &ModelRef) -> bool {
        let nodes = parent.borrow().get_children();
        if let Some(cdata) = self.get_text_content(&nodes, true) {
            let (image, in_cdata, begin_line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.in_cdata(), c.begin_line)
            };
            return self.process_effect_text(effect, &image, from_child(in_cdata), begin_line, model);
        }
        let name = effect.get_name().to_string();
        if !effect.is_state_specific() && model.has_effect(&name) {
            self.abr().log_node(parent, err("Multiple initializers for effect '${name}'.", &[("name", &name)]));
        }
        match self.process_rvalue_nodes(&Assignable::Effect(effect.clone()), &nodes, model) {
            Some(rvalue) => {
                let et = effect.get_type().expect("NullPointerException");
                model.set_effect_effect(effect, rvalue, &et, parent.borrow().begin_line);
                true
            }
            None => false,
        }
    }

    /// `processRValueNodes(assignable, nodes, model)`.
    fn process_rvalue_nodes(&mut self, assignable: &Assignable, nodes: &[NodeRef], model: &ModelRef) -> Option<RValue> {
        let t = assignable.get_lvalue_type().expect("NullPointerException");
        let name = assignable.get_name().unwrap_or_else(|| "null".into());
        let element_store_type = assignable.get_element_type();
        let mut element_parse_type = element_store_type.clone();
        if let Assignable::Property(property) = assignable {
            if element_store_type.as_ref().is_some_and(|e| self.abr().standard_defs.is_ideferred_instance(e)) {
                element_parse_type = property.get_instance_type();
            }
        }
        let is_default_property = model.get_type().get_default_property().is_some_and(|dp| pname(&dp) == name);
        let compat = self.check_type_compatibility(nodes, &t, element_parse_type.as_ref(), &name, is_default_property);
        let ctx = self.abr().ctx.clone();
        let first_line = nodes[0].borrow().begin_line;
        match compat {
            type_compatibility::OK => {
                let mut handler = RValueNodeHandler { ctx, model: model.clone(), result: None };
                handler.process(Some(assignable), &nodes[0])
            }
            type_compatibility::OK_COERCE_TO_ARRAY => {
                let mut array_builder = ArrayBuilder::new(&ctx, Some(model.clone()), Some(assignable.clone()), false);
                array_builder.create_synthetic_array_model(first_line);
                array_builder.process_children(nodes);
                array_builder.array.clone().map(RValue::Model)
            }
            type_compatibility::OK_COERCE_TO_VECTOR => {
                let mut vector_builder = VectorBuilder::new(&ctx, Some(model.clone()), Some(assignable.clone()), None, false);
                vector_builder.create_synthetic_vector_model(first_line);
                vector_builder.process_children(nodes);
                vector_builder.vector.clone().map(RValue::Model)
            }
            _ => None,
        }
    }

    /// `checkTypeCompatibility(nodes, lvalueType, lvalueArrayElemType, lvalueDesc, isDefaultProperty)`.
    fn check_type_compatibility(&mut self, nodes: &[NodeRef], lvalue_type: &TypeRef, lvalue_array_elem_type: Option<&TypeRef>, lvalue_desc: &str, is_default_property: bool) -> i32 {
        match nodes.len() {
            0 => type_compatibility::ERR_RTYPE_NOT_ASSIGNABLE_TO_LTYPE,
            1 => self.check_type_compatibility_node(&nodes[0], lvalue_type, lvalue_array_elem_type, lvalue_desc, true, is_default_property),
            _ => {
                let mut compat = type_compatibility::OK;
                for node in nodes {
                    let element_compat = self.check_type_compatibility_node(node, lvalue_type, lvalue_array_elem_type, lvalue_desc, false, is_default_property);
                    if compat == type_compatibility::OK || compat == type_compatibility::OK_COERCE_TO_ARRAY || compat == type_compatibility::OK_COERCE_TO_VECTOR {
                        compat = element_compat;
                    }
                }
                compat
            }
        }
    }

    /// `checkTypeCompatibility(node, ...)`, with `DocumentBuilder`'s override.
    fn check_type_compatibility_node(&mut self, node: &NodeRef, lvalue_type: &TypeRef, lvalue_array_element_type: Option<&TypeRef>, lvalue_description: &str, rvalue_is_singleton: bool, is_default_property: bool) -> i32 {
        let rtype = self.ab().resolve_type(node);
        let rvalue_type_name = match &rtype {
            Some(t) => t.get_name(),
            None => node.borrow().get_local_part().to_string(),
        };
        let compat = type_compatibility::check(lvalue_type, lvalue_array_element_type, rtype.as_ref(), rvalue_is_singleton, &self.abr().standard_defs);
        let compat = self.coerce_stateful_nodes(node, lvalue_type, compat);
        let doc_builder = self.is_document_builder();
        let ab = self.abr();
        let line = node.borrow().begin_line;
        let local = node.borrow().get_local_part().to_string();
        let rname = name_formatter::to_dot(&rvalue_type_name);
        let lname = dot(lvalue_type);
        let ename = lvalue_array_element_type.map(dot).unwrap_or_else(|| "null".into());
        match compat {
            type_compatibility::ERR_RTYPE_NOT_ASSIGNABLE_TO_LTYPE => {
                if is_default_property {
                    ab.log_line(line, err("'${type}' is not assignable to the default property, '${defaultProperty}', of type '${targetType}'.", &[("defaultProperty", lvalue_description), ("type", &rname), ("targetType", &lname)]));
                } else {
                    ab.log_line(line, err("In initializer for '${lvalue}', type ${type} is not assignable to target type '${targetType}'.", &[("lvalue", lvalue_description), ("type", &rname), ("targetType", &lname)]));
                }
            }
            type_compatibility::ERR_LTYPE_NOT_MULTIPLE => {
                if is_default_property {
                    ab.log_line(line, err("Multiple initializer values for default property, '${defaultProperty}', of type '${targetType}'.", &[("defaultProperty", lvalue_description), ("targetType", &lname)]));
                } else {
                    ab.log_line(line, err("In initializer for '${lvalue}', multiple initializer values for target type ${targetType}.", &[("lvalue", lvalue_description), ("targetType", &lname)]));
                }
            }
            type_compatibility::ERR_SINGLE_RVALUE_NOT_ARRAY_OR_ARRAY_ELEM => {
                if doc_builder && is_default_property {
                    ab.log_line(
                        line,
                        err("'${declaration}' declaration must be contained within the <Declarations> tag, since it is not assignable to the default property's type '${targetType}' or element type '${targetElementType}'.", &[("declaration", &local), ("targetType", &lname), ("targetElementType", &ename)]),
                    );
                } else {
                    ab.log_line(
                        line,
                        err("In initializer for '${lvalue}', type ${type} is not assignable to target type ${targetType} or target element type ${targetElementType}.", &[("lvalue", lvalue_description), ("type", &rname), ("targetType", &lname), ("targetElementType", &ename)]),
                    );
                }
            }
            type_compatibility::ERR_MULTI_RVALUE_NOT_ARRAY_ELEM => {
                if doc_builder && is_default_property {
                    ab.log_line(
                        line,
                        err("'${declaration}' declaration must be contained within the <Declarations> tag, since it is not assignable to the default property's element type '${targetElementType}'.", &[("declaration", &local), ("targetType", &lname), ("targetElementType", &ename)]),
                    );
                } else {
                    ab.log_line(
                        line,
                        err("In initializer for '${lvalue}', type ${type} is not assignable to target ${targetType} element type ${targetElementType}.", &[("lvalue", lvalue_description), ("type", &rname), ("targetType", &lname), ("targetElementType", &ename)]),
                    );
                }
            }
            _ => {}
        }
        compat
    }

    /// `coerceStatefulNodes(node, lvalueType, compat)`.
    fn coerce_stateful_nodes(&mut self, node: &NodeRef, lvalue_type: &TypeRef, compat: i32) -> i32 {
        if node.borrow().is_reparent_node() {
            return type_compatibility::OK_COERCE_TO_ARRAY;
        }
        let n = lvalue_type.get_name();
        if compat == type_compatibility::OK
            && (n == "*" || n == "Object")
            && (self.get_language_attribute(node, sd::PROP_INCLUDE_STATES).is_some() || self.get_language_attribute(node, sd::PROP_EXCLUDE_STATES).is_some())
        {
            return type_compatibility::OK_COERCE_TO_ARRAY;
        }
        compat
    }

    /// `checkPropertyUsage(property, text, line)`.
    fn check_property_usage(&mut self, property: &PropertyRef, text: &str, line: i32) -> bool {
        if !self.is_allowed_property(property) {
            self.abr().log_line(line, err("Initializer for property '${name}' is not allowed here.", &[("name", &pname(property))]));
            return false;
        }
        let ab = self.abr();
        if ab.mxml_configuration.show_deprecation_warnings() {
            check_deprecation(property, &ab.document.get_source_path(), line);
        }
        if let Some(inspectable) = property.get_inspectable() {
            ab.check_image_type(inspectable.get_format(), text, line);
            if !text_parser::is_binding_expression(text) && !ab.check_enumeration(inspectable.get_enumeration(), text, line) {
                return false;
            }
        }
        if property.read_only() {
            ab.log_line(line, err("Property '${name}' is read-only.", &[("name", &pname(property))]));
            return false;
        }
        if property.get_type().is_none() {
            ab.log_line(line, err("Internal error: type for Property '{name}' is unreachable.", &[]));
            return false;
        }
        true
    }

    /// `hasAttributeInitializers(node)`.
    fn has_attribute_initializers(&self, node: &NodeRef) -> bool {
        let names = node.borrow().get_attribute_names();
        names.iter().any(|q| !self.is_special_attribute(q.get_namespace(), q.get_local_part()))
    }

    /// `registerModel(node, model, topLevel)`.
    fn register_model(&mut self, node: &NodeRef, model: &ModelRef, top_level: bool) {
        let id = attr_str(self.get_language_attribute_value(node, sd::PROP_ID));
        if let Some(comment) = node.borrow().comment.clone() {
            *model.comment.borrow_mut() = Some(comment);
        }
        self.abr().register_model_id(id.as_deref(), model, top_level);
    }
}

/// `AbstractBuilder.RValueNodeHandler`.
pub struct RValueNodeHandler {
    pub ctx: BuilderContext,
    pub model: ModelRef,
    pub result: Option<RValue>,
}

fn get_name(property: Option<&Assignable>) -> Option<String> {
    property.and_then(|p| p.get_name())
}

fn get_state_name(property: Option<&Assignable>) -> Option<String> {
    property.and_then(|p| p.get_state_name())
}

impl RValueNodeHandler {
    pub fn process(&mut self, property: Option<&Assignable>, node: &NodeRef) -> Option<RValue> {
        let doc = self.ctx.document.clone();
        self.invoke(property, node, Some(&doc));
        self.result.take()
    }

    fn component(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ComponentBuilder::new(&self.ctx, Some(self.model.clone()), get_name(property).as_deref(), get_state_name(property).as_deref(), false, None);
        analyze(node, &mut builder);
        self.result = builder.component.clone().map(RValue::Model);
    }
}

impl ValueNodeHandler for RValueNodeHandler {
    fn component_node(&mut self, property: Option<&Assignable>, node: &NodeRef, _document: Option<&MxmlDocumentRef>) {
        self.component(property, node);
    }

    fn array_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ArrayBuilder::new(&self.ctx, Some(self.model.clone()), property.cloned(), false);
        analyze(node, &mut builder);
        self.result = builder.array.clone().map(RValue::Model);
    }

    fn vector_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let type_attribute_value = node.borrow().get_attribute_str(sd::PROP_TYPE).map(str::to_string).expect("NullPointerException");
        let element_type = self.ctx.type_table.get_type(&name_formatter::to_colon(&type_attribute_value));
        let mut builder = VectorBuilder::new(&self.ctx, Some(self.model.clone()), property.cloned(), element_type, false);
        analyze(node, &mut builder);
        self.result = builder.vector.clone().map(RValue::Model);
    }

    fn primitive_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = PrimitiveBuilder::new(&self.ctx, Some(self.model.clone()), false, property.cloned(), None);
        analyze(node, &mut builder);
        self.result = builder.value.clone().map(RValue::Model);
    }

    fn xml_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = XmlBuilder::new_with_parent(&self.ctx, Some(self.model.clone()));
        analyze(node, &mut builder);
        let xml = builder.xml.clone().expect("xml");
        xml.set_parent_index_state(get_name(property).as_deref(), get_state_name(property).as_deref());
        if xml.get_id().is_some() {
            self.ctx.document.ensure_declaration(&xml);
        }
        self.result = Some(RValue::Model(xml));
    }

    fn xml_list_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = XmlListBuilder::new_with_parent(&self.ctx, Some(self.model.clone()));
        analyze(node, &mut builder);
        let xml_list = builder.xml_list.clone().expect("xmlList");
        xml_list.set_parent_index_state(get_name(property).as_deref(), get_state_name(property).as_deref());
        self.result = Some(RValue::Model(xml_list));
    }

    fn model_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ModelBuilder::new(&self.ctx, Some(self.model.clone()));
        analyze(node, &mut builder);
        self.result = builder.aog.graph.clone().map(RValue::Model);
    }

    fn inline_component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = InlineComponentBuilder::new(&self.ctx, false);
        analyze(node, &mut builder);
        self.result = builder.get_rvalue().map(RValue::Model);
    }

    fn reparent_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.component(property, node);
    }

    fn cdata_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.primitive_node(property, node);
    }

    fn state_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.component(property, node);
    }

    fn unknown(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {
        self.result = None;
    }
}

/// Whether the node's Java class is exactly the given one.
pub fn node_class_is(node: &NodeRef, f: impl Fn(&NodeClass) -> bool) -> bool {
    f(&node.borrow().class)
}

/// The required `Analyzer` methods of a builder whose `AbstractBuilder` is its `ab` field.
macro_rules! analyzer_basics {
    () => {
        fn adapter(&self) -> &crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter {
            &self.ab.adapter
        }
        fn adapter_mut(&mut self) -> &mut crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter {
            &mut self.ab.adapter
        }
        fn get_document_version(&self) -> i32 {
            self.ab.get_document_version()
        }
        fn get_language_namespace(&self) -> Option<String> {
            self.ab.get_language_namespace()
        }
    };
}
pub(crate) use analyzer_basics;
