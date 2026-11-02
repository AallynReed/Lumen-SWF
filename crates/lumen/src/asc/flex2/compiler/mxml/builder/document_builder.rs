//! Port of `flex2.compiler.mxml.builder.DocumentBuilder`: the [`ComponentKind::Document`] of a [`ComponentBuilder`],
//! with its `DocumentChildNodeHandler`, `NestedDeclarationNodeHandler` and `RootAttributeParser`. Its unused
//! `DocumentAttributeHandler` is not ported.

use std::collections::BTreeMap;
use std::rc::Rc;

use super::abstract_builder::{attr_str, err, BindingHandlerKind, Builder, BuilderContext, TextValueParser};
use super::array_builder::ArrayBuilder;
use super::component_builder::{is_service_node, ComponentBuilder, ComponentChildNodeHandler, ComponentKind};
use super::inline_component_builder::InlineComponentBuilder;
use super::model_builder::ModelBuilder;
use super::primitive_builder::PrimitiveBuilder;
use super::vector_builder::VectorBuilder;
use super::xml_builder::XmlBuilder;
use super::xml_list_builder::XmlListBuilder;
use crate::asc::flex2::compiler::compilation_unit::{AuxGenerateInfo, UnitRef};
use crate::asc::flex2::compiler::mxml::attribute::{AttrValue, Attribute};
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeClass, NodeRef};
use crate::asc::flex2::compiler::mxml::gen::text_gen;
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser;
use crate::asc::flex2::compiler::mxml::lang::value_node_handler::ValueNodeHandler;
use crate::asc::flex2::compiler::mxml::mxml_configuration::MxmlConfiguration;
use crate::asc::flex2::compiler::mxml::mxml_namespaces::MXML_2009_NAMESPACE;
use crate::asc::flex2::compiler::mxml::reflect::{Assignable, TypeTable};
use crate::asc::flex2::compiler::mxml::rep::at_embed;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpression;
use crate::asc::flex2::compiler::mxml::rep::model::RValue;
use crate::asc::flex2::compiler::mxml::rep::mxml_document::MxmlDocumentRef;
use crate::asc::flex2::compiler::mxml::rep::script::Script;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::{mime_mappings, name_formatter, QName};
use crate::asc::util::jhash::JHashMap;

const ROOT_ATTR_BACKGROUND_COLOR: &str = "backgroundColor";
const ROOT_ATTR_HEIGHT: &str = "height";
const ROOT_ATTR_STYLE_NAME: &str = "styleName";
const ROOT_ATTR_WIDTH: &str = "width";
const SPECIAL_ATTR_EXCLUDE: &str = "excludeFrom";
const SPECIAL_ATTR_FRAME_RATE: &str = "frameRate";
const SPECIAL_ATTR_IMPLEMENTS: &str = "implements";
const SPECIAL_ATTR_INCLUDE: &str = "includeIn";
const SPECIAL_ATTR_ITEM_CPOLICY: &str = "itemCreationPolicy";
const SPECIAL_ATTR_ITEM_DPOLICY: &str = "itemDestructionPolicy";
const SPECIAL_ATTR_LIB: &str = "lib";
const SPECIAL_ATTR_PAGE_TITLE: &str = "pageTitle";
const SPECIAL_ATTR_PRELOADER: &str = "preloader";
const SPECIAL_ATTR_RSL: &str = "rsl";
const SPECIAL_ATTR_RUNTIME_DPI_PROVIDER: &str = "runtimeDPIProvider";
const SPECIAL_ATTR_SCRIPT_RECURSION_LIMIT: &str = "scriptRecursionLimit";
const SPECIAL_ATTR_SCRIPT_TIME_LIMIT: &str = "scriptTimeLimit";
const SPECIAL_ATTR_THEME: &str = "theme";
const SPECIAL_ATTR_USE_PRELOADER: &str = "usePreloader";
const SPECIAL_ATTR_SPLASH_SCREEN_IMAGE: &str = "splashScreenImage";
const SPECIAL_ATTR_USE_GPU: &str = "useGPU";
const SPECIAL_ATTR_USE_DIRECT_BLIT: &str = "useDirectBlit";

const SPECIAL_ATTRIBUTES_2006: &[&str] = &[
    SPECIAL_ATTR_FRAME_RATE,
    SPECIAL_ATTR_IMPLEMENTS,
    SPECIAL_ATTR_LIB,
    SPECIAL_ATTR_PAGE_TITLE,
    SPECIAL_ATTR_PRELOADER,
    SPECIAL_ATTR_RSL,
    SPECIAL_ATTR_SCRIPT_RECURSION_LIMIT,
    SPECIAL_ATTR_SCRIPT_TIME_LIMIT,
    SPECIAL_ATTR_THEME,
    SPECIAL_ATTR_USE_PRELOADER,
];

const SPECIAL_ATTRIBUTES_2009: &[&str] = &[
    SPECIAL_ATTR_FRAME_RATE,
    SPECIAL_ATTR_IMPLEMENTS,
    SPECIAL_ATTR_LIB,
    SPECIAL_ATTR_PAGE_TITLE,
    SPECIAL_ATTR_PRELOADER,
    SPECIAL_ATTR_RSL,
    SPECIAL_ATTR_RUNTIME_DPI_PROVIDER,
    SPECIAL_ATTR_SCRIPT_RECURSION_LIMIT,
    SPECIAL_ATTR_SCRIPT_TIME_LIMIT,
    SPECIAL_ATTR_THEME,
    SPECIAL_ATTR_USE_PRELOADER,
    SPECIAL_ATTR_SPLASH_SCREEN_IMAGE,
    SPECIAL_ATTR_USE_GPU,
    SPECIAL_ATTR_USE_DIRECT_BLIT,
    SPECIAL_ATTR_EXCLUDE,
    SPECIAL_ATTR_INCLUDE,
    SPECIAL_ATTR_ITEM_CPOLICY,
];

pub struct DocumentData {
    root_attribute_parser: TextValueParser,
    generate_loader: bool,
    pub(crate) in_declaration: bool,
}

/// `DocumentBuilder`'s constructor; the builder is a [`ComponentBuilder`] of kind `Document`.
pub struct DocumentBuilder;

impl DocumentBuilder {
    /// `new DocumentBuilder(unit, typeTable, mxmlConfiguration, document)`; run it with `dom::node::analyze(&root, &mut b)`.
    pub fn new(unit: UnitRef, type_table: Rc<TypeTable>, mxml_configuration: Rc<MxmlConfiguration>, document: MxmlDocumentRef) -> ComponentBuilder {
    let ctx = BuilderContext { unit, type_table, mxml_configuration, document };
    let mut root_attribute_parser = TextValueParser::new(&ctx, ctx.document.get_standard_defs());
    root_attribute_parser.root = true;
    ComponentBuilder::with_kind(&ctx, None, None, None, false, None, ComponentKind::Document(DocumentData { root_attribute_parser, generate_loader: true, in_declaration: false }))
}
}

fn doc_data(b: &mut ComponentBuilder) -> &mut DocumentData {
    match &mut b.kind {
        ComponentKind::Document(d) => d,
        _ => unreachable!(),
    }
}

/// `buildSwfMetadata(varmap)`.
fn build_swf_metadata(varmap: &BTreeMap<String, String>) -> Option<String> {
    if varmap.is_empty() {
        return None;
    }
    let mut buf = String::from("[SWF( ");
    let mut more = false;
    for (var, val) in varmap {
        if more {
            buf.push_str(", ");
        } else {
            more = true;
        }
        buf.push_str(var);
        buf.push_str("='");
        buf.push_str(val);
        buf.push('\'');
    }
    buf.push_str(")]");
    Some(buf)
}

fn attr_value_string(a: &Attribute) -> String {
    match a.get_value() {
        AttrValue::Str(s) => s.clone(),
        v => v.to_java_string(),
    }
}

impl ComponentBuilder {
    /// `DocumentBuilder.analyze(Node)`.
    pub(crate) fn document_analyze_node(&mut self, node: &NodeRef) {
        self.check_invalid_root_attributes(node);
        let comment = node.borrow().comment.clone();
        self.ab.document.set_comment(comment.as_deref());
        let t = self.ab.resolve_type(node).expect("NullPointerException");
        self.construct_component(&t, node.borrow().begin_line);
        self.ab.document.set_root(&self.component());
        self.process_attributes(node, &t);
        self.process_children(node, &t);
        self.ab.document.resolve_two_way_bindings();
        self.ab.document.post_process_states();
        self.post_process_design_layers();
        if self.ab.ctx.unit.borrow().is_root() {
            self.root_post_process(node);
            for bexpr in self.ab.document.get_binding_expressions() {
                self.ab.document.add_import(&bexpr.get_destination_type_name(), bexpr.get_xml_line_number());
            }
        }
    }

    pub(crate) fn document_analyze_meta_data_node(&mut self, node: &NodeRef) {
        let cdata = node.borrow().get_child_at(0);
        if cdata.is_some() {
            if node.borrow().get_text().unwrap_or_default().contains("[Frame") {
                doc_data(self).generate_loader = false;
            }
        }
    }

    pub(crate) fn document_analyze_style_node(&mut self, node: &NodeRef) {
        let sheet = node.borrow().get_style_sheet();
        if let Some(sheet) = sheet {
            let result = self.ab.document.get_styles_container().extract_styles(Some(&sheet.borrow()), true);
            if let Err(message) = result {
                let message = message.unwrap_or_else(|| "java.lang.Exception".to_string());
                self.ab.log_error_node(node, &message);
            }
        }
    }

    pub(crate) fn document_analyze_binding_node(&mut self, node: &NodeRef) {
        let mut is_two_way_bind = false;
        let (source, destination, two_way, line) = {
            let n = node.borrow();
            (n.get_attribute_str("source").map(str::to_string), n.get_attribute_str("destination").map(str::to_string), n.get_attribute_str("twoWay").map(str::to_string), n.begin_line)
        };
        let Some(source) = source else {
            self.ab.log_node(node, err("Missing ${attribute} attribute.", &[("attribute", "source")]));
            return;
        };
        let Some(destination) = destination else {
            self.ab.log_node(node, err("Missing ${attribute} attribute.", &[("attribute", "destination")]));
            return;
        };
        if let Some(two_way) = two_way {
            if let Some(RValue::Boolean(b)) = doc_data(self).root_attribute_parser.parse_boolean(&two_way, line, "twoWay") {
                is_two_way_bind = b;
            }
        }
        let st = self.ab.type_table.string_type();
        let value = self.ab.text_parser.parse_value_simple(&source, &st, 0, line, "source");
        let mut binding_expression = match value {
            Some(RValue::Binding(be)) => {
                if be.is_two_way_primary() {
                    self.ab.log_node(node, err("Initializer for '${desc}': two-way data binding expression not allowed here.", &[("desc", "source"), ("text", &source)]));
                } else if is_two_way_bind {
                    self.ab.log_node(node, err("Initializer for '${desc}': data binding expression not allowed here.", &[("desc", "source"), ("text", &source)]));
                }
                be
            }
            v => BindingExpression::new(&v.map(|v| v.to_java_string()).unwrap_or_else(|| "null".into()), line, &self.ab.document),
        };
        binding_expression.set_destination_property(Some(&destination));
        binding_expression.set_destination_lvalue(Some(&destination));
        binding_expression.set_from_binding_node(true);
        let value = self.ab.text_parser.parse_value_simple(&destination, &st, 0, line, "destination");
        if let Some(RValue::Binding(be)) = value {
            if be.is_two_way_primary() {
                self.ab.log_node(node, err("Initializer for '${desc}': two-way data binding expression not allowed here.", &[("desc", "destination"), ("text", &destination)]));
            } else {
                self.ab.log_node(node, err("Initializer for '${desc}': data binding expression not allowed here.", &[("desc", "destination"), ("text", &destination)]));
            }
            binding_expression = be;
        }
        if is_two_way_bind {
            let binding_expression2 = BindingExpression::new(&destination, line, &self.ab.document);
            let source = text_gen::strip_parens(&binding_expression.get_source_expression());
            binding_expression2.set_destination_property(Some(&source));
            binding_expression2.set_destination_lvalue(Some(&source));
            binding_expression2.set_from_binding_node(true);
        }
    }

    /// `DocumentBuilder.isSpecialAttribute(namespace, localPart)`.
    pub(crate) fn document_is_special_attribute(&self, namespace: &str, local_part: &str) -> bool {
        if self.ab.document.get_version() < 4 {
            namespace.is_empty() && SPECIAL_ATTRIBUTES_2006.contains(&local_part)
        } else {
            let namespace = if namespace.is_empty() { MXML_2009_NAMESPACE } else { namespace };
            namespace == MXML_2009_NAMESPACE && SPECIAL_ATTRIBUTES_2009.contains(&local_part)
        }
    }

    /// `getSpecialAttribute(node, localPart)`.
    fn get_special_attribute(&self, node: &NodeRef, local_part: &str) -> Option<Attribute> {
        let n = node.borrow();
        let mut attr = n.get_attribute_uri("", local_part).cloned();
        if self.ab.document.get_version() >= 4 && attr.is_none() {
            attr = n.get_attribute_uri(&self.ab.document.get_language_namespace().unwrap_or_default(), local_part).cloned();
        }
        attr
    }

    /// `postProcessDesignLayers()`.
    pub fn post_process_design_layers(&mut self) {
        for node in self.ab.document.get_layer_declaration_nodes() {
            if self.ab.document.get_layer_model(&node).is_none() {
                let mut builder = ComponentBuilder::new(&self.ab.ctx, self.component.clone(), None, None, true, None);
                analyze(&node, &mut builder);
            }
        }
    }

    /// `NestedDeclarationNodeHandler.invoke(null, child, document)`.
    pub(crate) fn nested_declaration_node_handler(&mut self, child: &NodeRef) {
        let mut h = NestedDeclarationNodeHandler { b: self };
        let doc = h.b.ab.document.clone();
        h.invoke(None, child, Some(&doc));
    }

    pub(crate) fn parse_root_attributes(&mut self, node: &NodeRef) {
        let mut swfvarmap: BTreeMap<String, String> = BTreeMap::new();
        let uint_attrs = [SPECIAL_ATTR_FRAME_RATE, SPECIAL_ATTR_SCRIPT_RECURSION_LIMIT, SPECIAL_ATTR_SCRIPT_TIME_LIMIT];
        for name in uint_attrs {
            if let Some(a) = self.get_special_attribute(node, name) {
                if let Some(v) = doc_data(self).root_attribute_parser.parse_uint(&attr_value_string(&a), a.get_line(), name) {
                    swfvarmap.insert(name.to_string(), v.to_java_string());
                }
            }
        }
        let attr = |n: &str| node.borrow().get_attribute_uri("", n).cloned();
        if let Some(bgcolor) = attr(ROOT_ATTR_BACKGROUND_COLOR) {
            if let Some(v) = doc_data(self).root_attribute_parser.parse_color(&attr_value_string(&bgcolor), bgcolor.get_line(), ROOT_ATTR_BACKGROUND_COLOR) {
                swfvarmap.insert(ROOT_ATTR_BACKGROUND_COLOR.to_string(), v.to_java_string());
            }
        }
        for name in [SPECIAL_ATTR_USE_DIRECT_BLIT, SPECIAL_ATTR_USE_GPU] {
            if let Some(a) = attr(name) {
                if let Some(v) = doc_data(self).root_attribute_parser.parse_boolean(&attr_value_string(&a), a.get_line(), name) {
                    swfvarmap.insert(name.to_string(), v.to_java_string());
                }
            }
        }
        if let Some(style_name) = attr(ROOT_ATTR_STYLE_NAME) {
            self.ab.document.get_compilation_unit().borrow_mut().style_name = Some(attr_value_string(&style_name));
        }
        if let Some(title) = self.get_special_attribute(node, SPECIAL_ATTR_PAGE_TITLE) {
            swfvarmap.insert(SPECIAL_ATTR_PAGE_TITLE.to_string(), attr_value_string(&title));
        }
        let node_type = self.ab.resolve_type(node);
        let is_application = node_type.as_ref().is_some_and(|t| sd::is_application(t));
        for (name, percent) in [(ROOT_ATTR_WIDTH, "widthPercent"), (ROOT_ATTR_HEIGHT, "heightPercent")] {
            let Some(a) = attr(name) else { continue };
            if !is_application {
                continue;
            }
            let s = attr_value_string(&a);
            let value = doc_data(self).root_attribute_parser.parse_number_or_percentage(&s, a.get_line(), name);
            if let Some(value) = value {
                if doc_data(self).root_attribute_parser.was_percentage() {
                    swfvarmap.insert(percent.to_string(), if s.ends_with('%') { s.clone() } else { format!("{s}%") });
                    node.borrow_mut().remove_attribute(&QName::new(a.get_namespace(), name));
                } else {
                    let value = match value {
                        RValue::Double(d) => RValue::Integer(d as i32),
                        v => v,
                    };
                    swfvarmap.insert(name.to_string(), value.to_java_string());
                }
            }
        }
        if let Some(use_preloader) = self.get_special_attribute(node, SPECIAL_ATTR_USE_PRELOADER) {
            if let Some(RValue::Boolean(b)) = doc_data(self).root_attribute_parser.parse_boolean(&attr_value_string(&use_preloader), use_preloader.get_line(), SPECIAL_ATTR_USE_PRELOADER) {
                self.ab.document.set_use_preloader(b);
            }
        }
        if let Some(preloader) = self.get_special_attribute(node, SPECIAL_ATTR_PRELOADER) {
            let preloader_string = attr_value_string(&preloader);
            if text_parser::parse_class_name(&preloader_string).is_some() {
                self.ab.document.set_preloader(&preloader_string);
            } else {
                self.ab.log_node(node, err("Invalid class name '${className}' specified for 'preloader' attribute.", &[("className", &preloader_string)]));
            }
        }
        if let Some(provider) = self.get_special_attribute(node, SPECIAL_ATTR_RUNTIME_DPI_PROVIDER) {
            let s = attr_value_string(&provider);
            if text_parser::parse_class_name(&s).is_none() {
                self.ab.log_node(node, err("Invalid class name '${className}' specified for 'runtimeDPIProvider' attribute.", &[("className", &s)]));
            }
        }
        if let Some(metadata) = build_swf_metadata(&swfvarmap) {
            self.ab.document.add_metadata(Script::from_text(&metadata));
        }
        if self.get_special_attribute(node, SPECIAL_ATTR_THEME).is_some() {
            self.ab.log_current(err("Please set your theme in the compiler configuration options; the 'theme' attribute is no longer supported in MXML documents.", &[]));
        }
        if self.get_special_attribute(node, SPECIAL_ATTR_RSL).is_some() {
            self.ab.log_current(err("Please specify runtime shared libraries in the compiler configuration options; the 'rsl' attribute is no longer supported in MXML documents.", &[]));
        }
        if self.get_special_attribute(node, SPECIAL_ATTR_LIB).is_some() {
            self.ab.log_current(err("Please specify libraries through the compiler configuration options; the 'lib' attribute is no longer supported in MXML documents.", &[]));
        }
    }

    fn root_post_process(&mut self, node: &NodeRef) {
        if doc_data(self).generate_loader {
            self.generate_loader_info(node);
        } else {
            self.ab.document.add_metadata(Script::from_text("[Frame(extraClass=\"FlexInit\")]\n"));
        }
    }

    fn generate_loader_info(&mut self, node: &NodeRef) {
        let doc = self.ab.document.clone();
        let Some(base_loader_class) = doc.get_super_class().get_loader_class() else { return };
        let non_ident = regex::Regex::new("[^A-Za-z0-9]").unwrap();
        let generate_init_class = non_ident.replace_all(&format!("_{}_FlexInit", doc.get_class_name()), "_").into_owned();
        doc.add_metadata(Script::from_text(&format!("[Frame(extraClass=\"{generate_init_class}\")]\n")));
        let base_loader_class = base_loader_class.replace(':', ".");
        let generate_loader_class = non_ident.replace_all(&format!("_{}_{base_loader_class}", doc.get_class_name()), "_").into_owned();
        doc.add_metadata(Script::from_text(&format!("[Frame(factoryClass=\"{generate_loader_class}\")]\n")));

        let mut root_attribute_map: JHashMap<String, String> = JHashMap::new();
        let mut root_attribute_embed_vars_map: JHashMap<String, String> = JHashMap::new();
        let mut root_attribute_embed_names_map: JHashMap<String, String> = JHashMap::new();
        let names = node.borrow().get_attribute_names();
        for qname in names {
            let local_part = qname.get_local_part().to_string();
            let (value, line) = {
                let n = node.borrow();
                (n.get_attribute_value_qname(&qname).map(|v| v.to_java_string()).unwrap_or_default(), n.get_line_number_qname(&qname))
            };
            let value = value.replace('"', "\\\"");
            if !text_parser::is_scoped_name(&local_part) {
                root_attribute_map.insert(local_part.clone(), value.clone());
                if local_part == "splashScreenImage" {
                    let at_function_name = text_parser::get_at_function_name(&value);
                    if at_function_name.as_deref() == Some("Embed") {
                        let source = self.ab.ctx.unit.borrow().get_source();
                        let path = source.borrow().get_name().to_string();
                        let per = self.ab.type_table.get_per_compile_data();
                        let at_embed = at_embed::AtEmbed::create_resolved(&per, &source, &value, &path, line, "_embed_mxml_", false);
                        match at_embed {
                            Some(e) => {
                                root_attribute_embed_vars_map.insert(local_part.clone(), at_embed::codegen_embed_var(&e));
                                root_attribute_embed_names_map.insert(local_part.clone(), e.get_prop_name().to_string());
                            }
                            None => toolkit::log(err("unable to resolve '${source}' for transcoding", &[("source", &value)]), Some(&path), line, -1, None),
                        }
                    } else if text_parser::parse_class_name(&value).is_none() {
                        self.ab.log_node(node, err("Invalid class name '${className}' specified for 'splashScreenImage' attribute.", &[("className", &value)]));
                    }
                }
            }
        }
        let package_name = doc.get_package_name();
        let window_class = if package_name.is_empty() { doc.get_class_name() } else { format!("{package_name}.{}", doc.get_class_name()) };
        let to_vec = |m: &JHashMap<String, String>| Some(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        self.ab.ctx.unit.borrow_mut().aux_generate_info = Some(AuxGenerateInfo {
            generate_loader_class,
            base_loader_class,
            window_class,
            root_attributes: to_vec(&root_attribute_map),
            root_attribute_embed_vars: to_vec(&root_attribute_embed_vars_map),
            root_attribute_embed_names: to_vec(&root_attribute_embed_names_map),
        });
    }

    fn check_invalid_root_attributes(&mut self, node: &NodeRef) {
        if let Some(id_attr) = self.get_language_attribute(node, sd::PROP_ID) {
            let line = if id_attr.get_line() == 0 { node.borrow().begin_line } else { id_attr.get_line() };
            toolkit::log(err("id attribute is not allowed on the root tag of a component.", &[]), Some(&self.ab.path()), line, -1, None);
        }
        if self.get_language_attribute(node, SPECIAL_ATTR_INCLUDE).is_some() || self.get_language_attribute(node, SPECIAL_ATTR_EXCLUDE).is_some() {
            self.ab.log_current(err("The includeIn and excludeFrom attributes are not allowed on the root tag of a component.", &[]));
        }
        if self.get_language_attribute(node, SPECIAL_ATTR_ITEM_CPOLICY).is_some() || self.get_language_attribute(node, SPECIAL_ATTR_ITEM_DPOLICY).is_some() {
            self.ab.log_current(err("The itemCreationPolicy and itemDestructionPolicy attributes are not allowed on the root tag of a component.", &[]));
        }
    }
}

fn nested_declaration_msg(local: &str, b: &ComponentBuilder) -> CompilerMessage {
    err(
        "'${declaration}' declaration must be contained within the <Declarations> tag since it does not implement '${targetType}'.",
        &[("declaration", local), ("targetType", &name_formatter::to_dot(&b.ab.standard_defs.interface_iuicomponent))],
    )
}

impl ComponentChildNodeHandler<'_> {
    pub(crate) fn document_language_node(&mut self) {
        let child = self.b.cnh.child();
        if self.b.ab.document.get_version() >= 4 && is_service_node(&child) {
            let local = child.borrow().get_local_part().to_string();
            let m = nested_declaration_msg(&local, self.b);
            self.b.ab.log_node(&child, m);
        } else {
            self.component_language_node();
        }
    }

    pub(crate) fn document_default_property_element(&mut self, loc_error: bool) {
        let mime_type = self.b.ab.document.get_compilation_unit().borrow().get_source().borrow().get_mime_type().unwrap_or("").to_string();
        if self.b.ab.document.get_is_inline_component() || (mime_type == mime_mappings::MXML && self.b.ab.document.get_version() >= 4) || mime_type == mime_mappings::FXG {
            self.component_default_property_element(loc_error);
        } else {
            self.nested_declaration_check(false);
        }
    }

    pub(crate) fn document_nested_declaration(&mut self) {
        let mime_type = self.b.ab.document.get_compilation_unit().borrow().get_source().borrow().get_mime_type().unwrap_or("").to_string();
        if mime_type == mime_mappings::MXML && self.b.ab.document.get_version() < 4 {
            self.nested_declaration_check(true);
            return;
        }
        let child = self.b.cnh.child();
        let parent_type = self.b.cnh.parent_type();
        let child_type = self.b.ab.resolve_type(&child).expect("nested declaration node type == null");
        let sdefs = self.b.ab.standard_defs.clone();
        if sdefs.is_container(&parent_type) && sdefs.is_iui_component(&child_type) && ((mime_type == mime_mappings::MXML && self.b.ab.document.get_version() < 4) || parent_type.get_default_property().is_none()) {
            self.b.process_visual_child(&child);
        } else if child.borrow().is_reparent_node() || (sdefs.is_container(&parent_type) && (child_type.is_assignable_to_name(Some(&sdefs.class_radiobuttongroup)) || child_type.is_assignable_to_name(Some(&sdefs.class_spark_radiobuttongroup)))) {
            let mut builder = ComponentBuilder::new(&self.b.ab.ctx, self.b.component.clone(), None, None, true, None);
            analyze(&child, &mut builder);
        } else if sdefs.is_spark_graphic(&child_type) {
            let parent_local = self.b.cnh.parent().borrow().get_local_part().to_string();
            let local = child.borrow().get_local_part().to_string();
            self.b.ab.log_node(&child, err("'${type}' is a graphic primitive that should be wrapped in a Group in order to be added to '${parentType}'.", &[("type", &local), ("parentType", &parent_local)]));
        } else {
            let local = child.borrow().get_local_part().to_string();
            let m = nested_declaration_msg(&local, self.b);
            self.b.ab.log_node(&child, m);
        }
    }

    /// `validateNestedDeclarationAttrs(node)`.
    fn validate_nested_declaration_attrs(&mut self, node: &NodeRef) -> bool {
        let include = attr_str(self.b.get_language_attribute_value(node, SPECIAL_ATTR_INCLUDE));
        let exclude = attr_str(self.b.get_language_attribute_value(node, SPECIAL_ATTR_EXCLUDE));
        let icpolicy = attr_str(self.b.get_language_attribute_value(node, SPECIAL_ATTR_ITEM_CPOLICY));
        let idpolicy = attr_str(self.b.get_language_attribute_value(node, SPECIAL_ATTR_ITEM_DPOLICY));
        if include.is_some() || exclude.is_some() {
            self.b.ab.log_node(node, err("The includeIn and excludeFrom attributes are not allowed on immediate children of the <Declarations> tag.", &[]));
            return false;
        }
        if icpolicy.is_some() || idpolicy.is_some() {
            self.b.ab.log_node(node, err("The itemCreationPolicy and itemDestructionPolicy attributes are not allowed on immediate children of the <Declarations> tag.", &[]));
            return false;
        }
        true
    }

    /// `DocumentChildNodeHandler.invoke(parent, parentType, child)`.
    pub(crate) fn document_invoke_child(&mut self, parent: &crate::asc::flex2::compiler::mxml::dom::node::NodeRef, parent_type: &crate::asc::flex2::compiler::mxml::reflect::TypeRef, child: &NodeRef) {
        let class = match &child.borrow().class {
            NodeClass::Declarations => 1,
            NodeClass::Library => 2,
            NodeClass::Private => 3,
            _ => 0,
        };
        match class {
            1 => {
                self.b.cnh.parent = Some(parent.clone());
                self.b.cnh.parent_type = Some(parent_type.clone());
                doc_data(self.b).in_declaration = true;
                for c in child.borrow().get_children() {
                    self.b.cnh.child = Some(c.clone());
                    if self.validate_nested_declaration_attrs(&c) {
                        if is_service_node(&c) {
                            self.component_language_node();
                        } else if self.b.is_legal_language_node(&c) {
                            let image = c.borrow().image.clone();
                            self.b.ab.log_node(&c, err("'${image}' must be a child of the root tag.", &[("image", &image)]));
                        } else {
                            self.nested_declaration_check(false);
                        }
                    }
                }
                doc_data(self.b).in_declaration = false;
            }
            2 | 3 => {}
            _ => self.component_invoke_child(parent, parent_type, child),
        }
    }
}

/// `DocumentBuilder.NestedDeclarationNodeHandler`.
struct NestedDeclarationNodeHandler<'a> {
    b: &'a mut ComponentBuilder,
}

impl ValueNodeHandler for NestedDeclarationNodeHandler<'_> {
    fn component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef, _document: Option<&MxmlDocumentRef>) {
        let mut builder = ComponentBuilder::new(&self.b.ab.ctx, self.b.component.clone(), None, None, true, Some(BindingHandlerKind::ComponentDeclaration));
        analyze(node, &mut builder);
    }

    fn array_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ArrayBuilder::new_top(&self.b.ab.ctx);
        analyze(node, &mut builder);
    }

    fn vector_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let type_attribute_value = node.borrow().get_attribute_str(sd::PROP_TYPE).map(str::to_string).expect("NullPointerException");
        let element_type = self.b.ab.type_table.get_type(&name_formatter::to_colon(&type_attribute_value));
        let mut builder = VectorBuilder::new(&self.b.ab.ctx, None, None, element_type, true);
        analyze(node, &mut builder);
    }

    fn primitive_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = PrimitiveBuilder::new(&self.b.ab.ctx, self.b.component.clone(), true, None, Some(BindingHandlerKind::PrimitiveDeclaration));
        analyze(node, &mut builder);
    }

    fn xml_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = XmlBuilder::new(&self.b.ab.ctx);
        analyze(node, &mut builder);
        let xml = builder.xml.clone().expect("xml");
        self.b.register_model(node, &xml, true);
    }

    fn xml_list_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = XmlListBuilder::new(&self.b.ab.ctx);
        analyze(node, &mut builder);
        let xml_list = builder.xml_list.clone().expect("xmlList");
        self.b.register_model(node, &xml_list, true);
    }

    fn model_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ModelBuilder::new(&self.b.ab.ctx, None);
        analyze(node, &mut builder);
    }

    fn inline_component_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = InlineComponentBuilder::new(&self.b.ab.ctx, true);
        analyze(node, &mut builder);
    }

    fn reparent_node(&mut self, _property: Option<&Assignable>, node: &NodeRef) {
        let mut builder = ComponentBuilder::new(&self.b.ab.ctx, self.b.component.clone(), None, None, true, None);
        analyze(node, &mut builder);
    }

    fn state_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.reparent_node(property, node);
    }

    fn cdata_node(&mut self, property: Option<&Assignable>, node: &NodeRef) {
        self.primitive_node(property, node);
    }

    fn unknown(&mut self, _property: Option<&Assignable>, _node: &NodeRef) {}
}
