//! Port of `flex2.compiler.mxml.builder.ComponentBuilder`. Its subclasses (`DocumentBuilder`, `HTTPServiceBuilder`,
//! `RemoteObjectBuilder`, `WebServiceBuilder`) are [`ComponentKind`]s whose overrides live in their own files; the
//! inner `ComponentAttributeHandler`/`ComponentChildNodeHandler` borrow the builder while they run.

use super::abstract_builder::{err, AbstractBuilder, BindingHandlerKind, Builder, BuilderContext, FROM_ATTRIBUTE, FROM_CHILD_CDATA};
use super::document_builder::DocumentData;
use super::http_service_builder::HttpServiceData;
use super::remote_object_builder::RemoteObjectData;
use super::web_service_builder::WebServiceData;
use crate::asc::flex2::compiler::mxml::dom::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::dom::analyzer_adapter::AnalyzerAdapter;
use crate::asc::flex2::compiler::mxml::dom::node::{analyze, NodeClass, NodeRef};
use crate::asc::flex2::compiler::mxml::lang::attribute_handler::{AttributeHandler, AttributeHandlerState};
use crate::asc::flex2::compiler::mxml::lang::child_node_handler::{ChildNodeHandler, ChildNodeHandlerState};
use crate::asc::flex2::compiler::mxml::lang::declaration_handler::DeclarationHandler;
use crate::asc::flex2::compiler::mxml::lang::standard_defs as sd;
use crate::asc::flex2::compiler::mxml::lang::text_parser;
use crate::asc::flex2::compiler::mxml::lang::value_node_handler::is_value_node;
use crate::asc::flex2::compiler::mxml::mxml_namespaces::FXG_2008_NAMESPACE;
use crate::asc::flex2::compiler::mxml::reflect::{EffectRef, EventRef, PropertyRef, Stateful, StyleRef, TypeRef};
use crate::asc::flex2::compiler::mxml::rep::model::{Model, ModelRef, RValue};
use crate::asc::flex2::compiler::util::{mime_mappings, name_formatter};

pub enum ComponentKind {
    Component,
    Document(DocumentData),
    HttpService(HttpServiceData),
    RemoteObject(RemoteObjectData),
    WebService(WebServiceData),
}

pub struct ComponentBuilder {
    pub ab: AbstractBuilder,
    pub parent: Option<ModelRef>,
    pub top_level_child: bool,
    name: Option<String>,
    state: Option<String>,
    pub binding_handler: BindingHandlerKind,
    pub component: Option<ModelRef>,
    pub(crate) cnh: ChildNodeHandlerState,
    pub(crate) default_property_nodes: Vec<NodeRef>,
    pub kind: ComponentKind,
}

fn dot(t: &TypeRef) -> String {
    name_formatter::to_dot(&t.get_name())
}

impl ComponentBuilder {
    /// `new ComponentBuilder(unit, typeTable, mxmlConfiguration, document, parent, name, state, topLevelChild, bindingHandler)`.
    pub fn new(ctx: &BuilderContext, parent: Option<ModelRef>, name: Option<&str>, state: Option<&str>, top_level_child: bool, binding_handler: Option<BindingHandlerKind>) -> ComponentBuilder {
        ComponentBuilder::with_kind(ctx, parent, name, state, top_level_child, binding_handler, ComponentKind::Component)
    }

    pub(crate) fn with_kind(ctx: &BuilderContext, parent: Option<ModelRef>, name: Option<&str>, state: Option<&str>, top_level_child: bool, binding_handler: Option<BindingHandlerKind>, kind: ComponentKind) -> ComponentBuilder {
        let ab = AbstractBuilder::new(ctx);
        let is_fxg = ab.document.get_language_namespace().as_deref() == Some(FXG_2008_NAMESPACE);
        let cnh = ChildNodeHandlerState::new(ctx.type_table.clone(), is_fxg);
        ComponentBuilder {
            ab,
            parent,
            top_level_child,
            name: name.map(Into::into),
            state: state.map(Into::into),
            binding_handler: binding_handler.unwrap_or(BindingHandlerKind::ComponentDeclaration),
            component: None,
            cnh,
            default_property_nodes: Vec::new(),
            kind,
        }
    }

    pub fn component(&self) -> ModelRef {
        self.component.clone().expect("component")
    }

    pub fn is_document(&self) -> bool {
        matches!(self.kind, ComponentKind::Document(_))
    }

    /// `analyze(Node)`.
    pub(crate) fn analyze_component_node(&mut self, node: &NodeRef) {
        let t = self.ab.resolve_type(node).expect("NullPointerException");
        self.construct_component(&t, node.borrow().begin_line);
        self.component().set_parent_index_state(self.name.as_deref(), self.state.as_deref());
        self.process_attributes(node, &t);
        self.process_children(node, &t);
        let c = self.component();
        let top = self.top_level_child;
        self.register_model(node, &c, top);
    }

    /// `constructComponent(type, line)`.
    pub fn construct_component(&mut self, t: &TypeRef, line: i32) {
        let doc = &self.ab.document;
        let c = if self.ab.standard_defs.is_iui_component(t) { Model::new_movie_clip(doc, t.clone(), self.parent.clone(), line) } else { Model::with_parent(doc, t.clone(), self.parent.clone(), line) };
        if **t == *self.ab.type_table.object_type() {
            c.set_inspectable(true);
        }
        self.component = Some(c);
    }

    /// `processAttributes(node, type)`.
    pub fn process_attributes(&mut self, node: &NodeRef, t: &TypeRef) {
        self.process_special_attributes(node);
        let names = node.borrow().get_attribute_names();
        for q in names {
            let mut h = ComponentAttributeHandler { b: self, st: AttributeHandlerState::default() };
            h.invoke_attribute(node, t, &q);
        }
    }

    /// `processLayerParent(node)`.
    pub fn process_layer_parent(&mut self, node: &NodeRef) {
        let mut layer_parent = node.borrow().get_layer_parent();
        if layer_parent.is_none() {
            return;
        }
        while let Some(lp) = layer_parent.clone() {
            let count = lp.borrow().get_attribute_count();
            if count > 0 {
                let has_stateful_attrs = has_state_specific_attributes(&lp);
                let (alpha_value, visible_value, id_value) = {
                    let n = lp.borrow();
                    (n.get_attribute_str("alpha").map(str::to_string), n.get_attribute_str("visible").map(str::to_string), n.get_attribute_str("id").map(str::to_string))
                };
                let opaque = alpha_value.as_deref().is_some_and(|a| a == "1.0" || a == "1") || alpha_value.is_none();
                let visible = visible_value.as_deref() == Some("true") || visible_value.is_none();
                if !(id_value.is_none() && opaque && visible) || has_stateful_attrs {
                    break;
                }
            }
            layer_parent = lp.borrow().get_layer_parent();
        }
        if let Some(lp) = layer_parent {
            let layer_model = match self.ab.document.get_layer_model(&lp) {
                Some(m) => m,
                None => {
                    let mut builder = ComponentBuilder::new(&self.ab.ctx, self.component.clone(), None, None, true, None);
                    analyze(&lp, &mut builder);
                    builder.component()
                }
            };
            *self.component().layer_parent.borrow_mut() = Some(layer_model);
        }
    }

    /// `unknownAttributeError(namespace, localPart, line)`.
    pub fn unknown_attribute_error(&self, namespace: Option<&str>, local_part: &str, line: i32) {
        let tname = dot(self.component().get_type());
        match namespace.filter(|n| !n.is_empty()) {
            Some(ns) => self.ab.log_line(line, err("Cannot resolve attribute '${localPart}' in namespace '${namespace}' for component type ${type}.", &[("namespace", ns), ("localPart", local_part), ("type", &tname)])),
            None => self.ab.log_line(line, err("Cannot resolve attribute '${name}' for component type ${type}.", &[("name", local_part), ("type", &tname)])),
        }
    }

    /// `processChildren(node, type)`.
    pub fn process_children(&mut self, node: &NodeRef, t: &TypeRef) {
        {
            let mut h = ComponentChildNodeHandler { b: self };
            h.scan_child_nodes(node, t);
        }
        if !self.default_property_nodes.is_empty() {
            let nodes = self.default_property_nodes.clone();
            let dp = t.get_default_property().expect("NullPointerException");
            let c = self.component();
            let line = node.borrow().begin_line;
            self.process_property_nodes_list(&nodes, &dp, &c, line);
        }
    }

    /// `isLegalLanguageNode(node)`, with `DocumentBuilder`'s override.
    pub fn is_legal_language_node(&self, node: &NodeRef) -> bool {
        let base = matches!(node.borrow().class, NodeClass::Script { .. } | NodeClass::Declarations | NodeClass::Library | NodeClass::Reparent);
        if self.is_document() {
            base || matches!(node.borrow().class, NodeClass::MetaData | NodeClass::Style { .. } | NodeClass::Binding) || is_service_node(node)
        } else {
            base
        }
    }

    /// `processVisualChild(node)`, with `DocumentBuilder`'s override.
    pub fn process_visual_child(&mut self, node: &NodeRef) {
        if let ComponentKind::Document(d) = &self.kind {
            if d.in_declaration {
                let mut builder = ComponentBuilder::new(&self.ab.ctx, self.component.clone(), None, None, true, None);
                analyze(node, &mut builder);
                return;
            }
        }
        let mut builder = ComponentBuilder::new(&self.ab.ctx, self.component.clone(), None, None, false, None);
        analyze(node, &mut builder);
        self.component().add_child(&builder.component());
    }

    /// `processTextInitializer(cdata)`.
    pub fn process_text_initializer(&mut self, cdata: &NodeRef) {
        let component = self.component();
        if !component.is_empty() {
            self.ab.log_line(cdata.borrow().begin_line, err("Both text and non-text initializers were specified.", &[]));
            return;
        }
        let (text, line, in_cdata) = {
            let c = cdata.borrow();
            (c.image.clone(), c.begin_line, c.in_cdata())
        };
        let t = component.get_type().clone();
        let flags = if in_cdata { text_parser::FLAG_IN_CDATA } else { 0 };
        let ot = self.ab.type_table.object_type();
        let value = self.ab.text_parser.parse_value_elem(&text, &t, Some(&ot), flags, line, &dot(&t));
        if let Some(value) = value {
            match &value {
                RValue::Binding(be) => {
                    self.binding_handler.invoke(be, &component);
                }
                _ => {
                    let prepped = component;
                    let c = match &value {
                        RValue::Model(m) => m.clone(),
                        v => Model::new_primitive_value(&self.ab.document, prepped.get_type().clone(), v, line),
                    };
                    c.set_id(prepped.get_id().as_deref(), prepped.get_id_is_autogenerated());
                    self.component = Some(c);
                }
            }
        }
    }

    /// `processSpecialAttributes(node)`, with `DocumentBuilder`'s override.
    pub fn process_special_attributes(&mut self, node: &NodeRef) {
        if self.is_document() {
            if self.ab.ctx.unit.borrow().get_source().borrow().is_root() {
                self.parse_root_attributes(node);
            }
        } else {
            let c = self.component();
            self.process_state_attributes(node, &c);
        }
    }

    fn is_mxml_source(&self) -> bool {
        self.ab.document.get_compilation_unit().borrow().get_source().borrow().get_mime_type() == Some(mime_mappings::MXML)
    }
}

fn has_state_specific_attributes(node: &NodeRef) -> bool {
    node.borrow().get_attribute_names().iter().any(|q| text_parser::is_scoped_name(q.get_local_part()))
}

/// `DocumentBuilder.isServiceNode(node)`.
pub fn is_service_node(node: &NodeRef) -> bool {
    matches!(node.borrow().class, NodeClass::RemoteObject | NodeClass::HttpService | NodeClass::WebService)
}

impl Builder for ComponentBuilder {
    fn ab(&mut self) -> &mut AbstractBuilder {
        &mut self.ab
    }

    fn abr(&self) -> &AbstractBuilder {
        &self.ab
    }

    /// `isAllowedProperty`, with `DocumentBuilder`'s override.
    fn is_allowed_property(&mut self, property: &PropertyRef) -> bool {
        if self.is_document() {
            return true;
        }
        !(property.get_name() == Some(sd::PROP_UICOMPONENT_STATES) && property.get_type().is_some_and(|t| *t == *self.ab.type_table.array_type()))
    }

    /// `isSpecialAttribute`, with `DocumentBuilder`'s override.
    fn is_special_attribute(&self, namespace: &str, local_part: &str) -> bool {
        if self.is_document() {
            return self.document_is_special_attribute(namespace, local_part);
        }
        if self.ab.document.get_version() < 4 {
            namespace.is_empty() && local_part == sd::PROP_ID
        } else if namespace.is_empty() || Some(namespace) == self.ab.document.get_language_namespace().as_deref() {
            matches!(local_part, sd::PROP_ID | sd::PROP_INCLUDE_STATES | sd::PROP_EXCLUDE_STATES | sd::PROP_ITEM_CREATION_POLICY | sd::PROP_ITEM_DESTRUCTION_POLICY)
        } else {
            false
        }
    }

    fn is_document_builder(&self) -> bool {
        self.is_document()
    }
}

impl Analyzer for ComponentBuilder {
    fn adapter(&self) -> &AnalyzerAdapter {
        &self.ab.adapter
    }

    fn adapter_mut(&mut self) -> &mut AnalyzerAdapter {
        &mut self.ab.adapter
    }

    fn get_document_version(&self) -> i32 {
        self.ab.get_document_version()
    }

    fn get_language_namespace(&self) -> Option<String> {
        self.ab.get_language_namespace()
    }

    fn analyze_node(&mut self, node: &NodeRef) {
        match self.kind {
            ComponentKind::Document(_) => self.document_analyze_node(node),
            _ => self.analyze_component_node(node),
        }
    }

    fn analyze_layered_node(&mut self, node: &NodeRef) {
        self.analyze_node(node);
        if !self.is_document() {
            self.process_layer_parent(node);
        }
    }

    fn analyze_script_node(&mut self, _node: &NodeRef) {}

    fn analyze_design_layer_node(&mut self, node: &NodeRef) {
        let t = self.ab.resolve_type(node).expect("NullPointerException");
        let line = node.borrow().begin_line;
        self.component = Some(Model::new_design_layer(&self.ab.document, t.clone(), None, line));
        self.process_attributes(node, &t);
        self.process_layer_parent(node);
        self.ab.document.add_layer_model(node, &self.component());
        if node.borrow().get_attribute_count() > 0 {
            let c = self.component();
            self.register_model(node, &c, true);
        }
    }

    fn analyze_reparent_node(&mut self, node: &NodeRef) {
        let t = self.ab.resolve_type(node).expect("NullPointerException");
        let target = node.borrow().get_attribute_str("target").map(str::to_string).unwrap_or_else(|| "null".into());
        let line = node.borrow().begin_line;
        self.component = Some(Model::new_reparent_target(&self.ab.document, t.clone(), self.parent.clone(), &target, line));
        self.process_attributes(node, &t);
    }

    fn analyze_state_node(&mut self, node: &NodeRef) {
        let t = self.ab.resolve_type(node).expect("NullPointerException");
        self.construct_component(&t, node.borrow().begin_line);
        self.process_attributes(node, &t);
        let c = self.component();
        let top = self.top_level_child;
        self.register_model(node, &c, top);
        if self.ab.document.get_version() < 4 {
            self.process_children(node, &t);
        } else {
            self.ab.document.register_state(&c, node);
            if node.borrow().get_child_count() > 0 || node.borrow().get_attribute_value("overrides").is_some() {
                self.ab.log_line(node.borrow().begin_line, err("State overrides may no longer be explicitly declared. The legacy states syntax has been deprecated.", &[]));
            }
        }
    }

    fn analyze_meta_data_node(&mut self, node: &NodeRef) {
        if self.is_document() {
            self.document_analyze_meta_data_node(node);
        } else {
            self.traverse(node);
        }
    }

    fn analyze_style_node(&mut self, node: &NodeRef) {
        if self.is_document() {
            self.document_analyze_style_node(node);
        } else {
            self.traverse(node);
        }
    }

    fn analyze_binding_node(&mut self, node: &NodeRef) {
        if self.is_document() {
            self.document_analyze_binding_node(node);
        } else {
            self.traverse(node);
        }
    }

    fn analyze_web_service_node(&mut self, node: &NodeRef) {
        match self.kind {
            ComponentKind::Document(_) => {
                let mut builder = ComponentBuilder::new_web_service(&self.ab.ctx);
                analyze(node, &mut builder);
            }
            ComponentKind::WebService(_) => self.web_service_analyze(node),
            _ => self.traverse(node),
        }
    }

    fn analyze_http_service_node(&mut self, node: &NodeRef) {
        match self.kind {
            ComponentKind::Document(_) => {
                let mut builder = ComponentBuilder::new_http_service(&self.ab.ctx);
                analyze(node, &mut builder);
            }
            ComponentKind::HttpService(_) => self.http_service_analyze(node),
            _ => self.traverse(node),
        }
    }

    fn analyze_remote_object_node(&mut self, node: &NodeRef) {
        match self.kind {
            ComponentKind::Document(_) => {
                let mut builder = ComponentBuilder::new_remote_object(&self.ab.ctx);
                analyze(node, &mut builder);
            }
            ComponentKind::RemoteObject(_) => self.remote_object_analyze(node),
            _ => self.traverse(node),
        }
    }
}

/// `ComponentBuilder.ComponentAttributeHandler`.
pub(crate) struct ComponentAttributeHandler<'a> {
    pub b: &'a mut ComponentBuilder,
    pub st: AttributeHandlerState,
}

impl ComponentAttributeHandler<'_> {
    fn text(&self) -> String {
        self.st.text.clone().unwrap_or_default()
    }
}

impl DeclarationHandler for ComponentAttributeHandler<'_> {
    fn event(&mut self, event: &EventRef) {
        let line = self.st.line;
        self.b.ab.check_event_deprecation(event, line);
        let c = self.b.component();
        self.b.ab.process_event_text(event, &self.text(), line, &c);
    }

    fn states(&mut self, property: &PropertyRef) {
        self.property(property);
    }

    fn property(&mut self, property: &PropertyRef) {
        let c = self.b.component();
        let (text, line) = (self.text(), self.st.line);
        self.b.process_property_text(property, &text, FROM_ATTRIBUTE, line, &c);
    }

    fn effect(&mut self, effect: &EffectRef) {
        let line = self.st.line;
        self.b.ab.check_effect_deprecation(effect, line);
        let c = self.b.component();
        let text = self.text();
        self.b.process_effect_text(effect, &text, FROM_ATTRIBUTE, line, &c);
    }

    fn style(&mut self, style: &StyleRef) {
        let line = self.st.line;
        self.b.ab.check_style_deprecation(style, line);
        let c = self.b.component();
        let text = self.text();
        self.b.process_style_text(style, &text, FROM_ATTRIBUTE, line, &c);
    }

    fn dynamic_property(&mut self, name: &str, state: Option<&str>) {
        let c = self.b.component();
        let (text, line) = (self.text(), self.st.line);
        self.b.process_dynamic_property_text(name, &text, FROM_ATTRIBUTE, line, &c, state);
    }

    fn unknown(&mut self, namespace: &str, local_part: &str) {
        let line = self.st.line;
        let t = self.st.type_.clone().expect("type");
        let style_themes = t.get_style_themes(local_part);
        let cname = dot(self.b.component().get_type());
        if t.is_excluded_style(local_part) {
            self.b.ab.log_line(line, err("The style '${stylePropertyName}' is excluded by type '${typeName}'.", &[("stylePropertyName", local_part), ("typeName", &cname)]));
        } else if let Some(themes) = style_themes {
            let text = "The style '${stylePropertyName}' is only supported by type '${typeName}' with the theme(s) '${styleThemes}'.";
            let params = [("stylePropertyName", local_part), ("typeName", cname.as_str()), ("styleThemes", themes.as_str())];
            if self.b.ab.mxml_configuration.report_invalid_styles_as_warnings {
                self.b.ab.log_line(line, super::abstract_builder::warn(text, &params));
            } else {
                self.b.ab.log_line(line, err(text, &params));
            }
        } else {
            self.b.unknown_attribute_error(Some(namespace), local_part, line);
        }
    }

    /// `ComponentAttributeHandler.invoke(type, namespace, localPart)`: state-scoped names first.
    fn invoke(&mut self, t: &TypeRef, namespace: &str, local_part: &str) {
        let line = self.st.line;
        if text_parser::is_scoped_name(local_part) {
            if self.b.ab.document.get_version() >= 4 {
                let stateful_name = text_parser::analyze_scoped_name(local_part);
                match &stateful_name {
                    Some(sn) if self.b.ab.document.validate_state(&sn[1], line) => {
                        if self.is_special(namespace, &sn[0]) {
                            self.b.ab.log_line(line, err("Cannot specify state-specific value for MXML language attribute '${name}'.", &[("name", &sn[0])]));
                        } else {
                            let c = self.b.component();
                            c.ensure_declaration();
                            c.ensure_bindable();
                            self.invoke_state(t, namespace, &sn[0], Some(&sn[1]));
                        }
                    }
                    None => self.b.unknown_attribute_error(Some(namespace), local_part, line),
                    _ => {}
                }
            } else {
                self.b.ab.log_line(line, err("State-specific property syntax '${name}' is unsupported for MXML language versions earlier than 2009.", &[("name", local_part)]));
            }
        } else {
            self.invoke_state(t, namespace, local_part, None);
        }
    }
}

impl AttributeHandler for ComponentAttributeHandler<'_> {
    fn ah(&mut self) -> &mut AttributeHandlerState {
        &mut self.st
    }

    fn qualified_attribute(&mut self, node: &NodeRef, t: &TypeRef, namespace: &str, local_part: &str) {
        let version = self.b.ab.document.get_version();
        if version < 4 {
            self.unknown_namespace(namespace, local_part);
        } else if Some(namespace) == self.b.ab.document.get_language_namespace().as_deref() {
            self.unknown(namespace, local_part);
        } else if namespace == node.borrow().get_namespace() {
            self.invoke(t, namespace, local_part);
        }
    }

    fn is_special(&mut self, namespace: &str, local_part: &str) -> bool {
        self.b.is_special_attribute(namespace, local_part)
    }

    fn special(&mut self, _t: &TypeRef, _namespace: &str, _local_part: &str) {}

    fn unknown_namespace(&mut self, namespace: &str, _local_part: &str) {
        let text = self.text();
        self.b.ab.log_line(self.st.line, err("Unknown namespace ${namespace} on attribute ${text} (only default attribute namespaces are supported).", &[("namespace", namespace), ("text", &text)]));
    }
}

/// `ComponentBuilder.ComponentChildNodeHandler` (and `DocumentChildNodeHandler`, `HTTPServiceChildNodeHandler`, ...).
pub(crate) struct ComponentChildNodeHandler<'a> {
    pub b: &'a mut ComponentBuilder,
}

impl ComponentChildNodeHandler<'_> {
    fn child(&self) -> NodeRef {
        self.b.cnh.child()
    }

    fn parent_type(&self) -> TypeRef {
        self.b.cnh.parent_type()
    }

    /// `nestedDeclaration(checkHaloNavigatorRequirements)`.
    pub(crate) fn nested_declaration_check(&mut self, check_halo_navigator_requirements: bool) {
        let child = self.child();
        let parent_type = self.parent_type();
        let child_type = self.b.ab.resolve_type(&child).expect("nested declaration node type == null");
        let sdefs = self.b.ab.standard_defs.clone();
        let is_reparent = child.borrow().is_reparent_node();
        if check_halo_navigator_requirements && sdefs.is_halo_navigator(&parent_type) && !sdefs.is_navigator_content(&child_type) && !is_reparent {
            self.b.ab.log_node(&child, err("The children of Halo navigators must implement INavigatorContent.", &[]));
        } else if sdefs.is_container(&parent_type) && sdefs.is_iui_component(&child_type) && ((self.b.is_mxml_source() && self.b.ab.document.get_version() < 4) || parent_type.get_default_property().is_none()) {
            self.b.process_visual_child(&child);
        } else if (sdefs.is_container(&parent_type) && (child_type.is_assignable_to_name(Some(&sdefs.class_radiobuttongroup)) || child_type.is_assignable_to_name(Some(&sdefs.class_spark_radiobuttongroup)))) || is_reparent {
            let mut builder = ComponentBuilder::new(&self.b.ab.ctx, self.b.component.clone(), None, None, true, None);
            analyze(&child, &mut builder);
        } else {
            self.process_nested_declaration(&child_type);
        }
    }

    /// `processNestedDeclaration(childType)`, with `DocumentChildNodeHandler`'s override.
    fn process_nested_declaration(&mut self, child_type: &TypeRef) {
        if self.b.is_document() {
            let child = self.child();
            self.b.nested_declaration_node_handler(&child);
            return;
        }
        let child = self.child();
        let sdefs = self.b.ab.standard_defs.clone();
        if sdefs.is_spark_graphic(child_type) {
            let parent_local = self.b.cnh.parent().borrow().get_local_part().to_string();
            let local = child.borrow().get_local_part().to_string();
            self.b.ab.log_node(&child, err("'${type}' is a graphic primitive that should be wrapped in a Group in order to be added to '${parentType}'.", &[("type", &local), ("parentType", &parent_local)]));
        } else if sdefs.is_container(&self.parent_type()) {
            self.b.ab.log_node(&child, err("Component declarations are not allowed here. (Note: visual children must implement ${interfaceName})", &[("interfaceName", &name_formatter::to_dot(&sdefs.interface_iuicomponent))]));
        } else {
            self.b.ab.log_node(&child, err("Component declarations are not allowed here.", &[]));
        }
    }

    /// `ComponentChildNodeHandler.defaultPropertyElement(locError)`.
    pub(crate) fn component_default_property_element(&mut self, loc_error: bool) {
        let child = self.child();
        let parent_type = self.parent_type();
        if loc_error {
            let dp = parent_type.get_default_property().map(|p| p.get_name().unwrap_or("null").to_string()).unwrap_or_default();
            self.b.ab.log_node(
                &child,
                err("Child elements of '${parentTypeName}' serving as the default property value for '${propertyName}' must be contiguous.", &[("parentTypeName", &name_formatter::retrieve_class_name(&parent_type.get_name())), ("propertyName", &dp)]),
            );
        }
        let child_type = self.b.ab.resolve_type(&child);
        let sdefs = self.b.ab.standard_defs.clone();
        if child_type.as_ref().is_some_and(|t| sdefs.is_repeater(t)) && !sdefs.is_container(&parent_type) {
            self.b.ab.log_node(&child, err("Repeaters require a Halo container based parent.", &[]));
        } else {
            self.b.default_property_nodes.push(child);
        }
    }

    /// `ComponentChildNodeHandler.languageNode()`.
    pub(crate) fn component_language_node(&mut self) {
        let child = self.child();
        if self.b.is_legal_language_node(&child) {
            analyze(&child, self.b);
        } else {
            let image = child.borrow().image.clone();
            self.b.ab.log_node(&child, err("${image} is not allowed here.", &[("image", &image)]));
        }
    }

    /// `ComponentChildNodeHandler.invoke(parent, parentType, child)`.
    pub(crate) fn component_invoke_child(&mut self, parent: &NodeRef, parent_type: &TypeRef, child: &NodeRef) {
        self.b.cnh.parent = Some(parent.clone());
        self.b.cnh.parent_type = Some(parent_type.clone());
        self.b.cnh.child = Some(child.clone());
        if is_value_node(child) {
            let (namespace, local_part, count, line) = {
                let c = child.borrow();
                (c.get_namespace().to_string(), c.get_local_part().to_string(), c.get_attribute_count(), c.begin_line)
            };
            if count == 0 && namespace == parent.borrow().get_namespace() {
                if text_parser::is_scoped_name(&local_part) {
                    let stateful_name = text_parser::analyze_scoped_name(&local_part);
                    if self.b.ab.document.get_version() >= 4 {
                        if let Some(sn) = stateful_name.filter(|sn| self.b.ab.document.validate_state(&sn[1], line)) {
                            let c = self.b.component();
                            c.ensure_declaration();
                            c.ensure_bindable();
                            self.invoke_state(parent_type, &namespace, &sn[0], Some(&sn[1]));
                        }
                    } else {
                        self.b.ab.log_node(child, err("State-specific property syntax '${name}' is unsupported for MXML language versions earlier than 2009.", &[("name", &local_part)]));
                    }
                } else {
                    self.core_invoke(parent_type, &namespace, &local_part, None);
                }
            } else {
                self.unknown("", "");
            }
        } else {
            let st = &mut self.b.cnh;
            st.parent = Some(parent.clone());
            st.parent_type = Some(parent_type.clone());
            st.child = Some(child.clone());
            if child.borrow().is_cdata_node() {
                use crate::asc::flex2::compiler::mxml::lang::child_node_handler::{ERR_DP_LOCATION, INPUT_DP_CHILD, INPUT_NON_DP_CHILD};
                if parent_type.get_default_property().is_some() {
                    self.b.cnh.transition(INPUT_DP_CHILD);
                    let loc_error = self.b.cnh.get_error() == ERR_DP_LOCATION;
                    self.default_property_element(loc_error);
                } else {
                    self.b.cnh.transition(INPUT_NON_DP_CHILD);
                    self.text_content();
                }
            } else {
                self.language_node();
            }
        }
    }
}

impl ChildNodeHandler for ComponentChildNodeHandler<'_> {
    fn cnh(&mut self) -> &mut ChildNodeHandlerState {
        &mut self.b.cnh
    }

    fn event(&mut self, event: &EventRef) {
        let child = self.child();
        let children = child.borrow().get_children();
        if let Some(cdata) = self.b.get_text_content(&children, false) {
            let (image, line) = {
                let c = cdata.borrow();
                (c.image.clone(), c.begin_line)
            };
            let c = self.b.component();
            self.b.ab.process_event_text(event, &image, line, &c);
        }
    }

    fn states(&mut self, property: &PropertyRef) {
        self.property(property);
        if self.b.ab.document.get_version() >= 4 {
            self.b.component().remove_property(property.get_name().unwrap_or("null"));
            if property.is_state_specific() {
                let child = self.child();
                self.b.ab.log_node(&child, err("Cannot specify state-specific value for MXML language attribute '${name}'.", &[("name", sd::PROP_UICOMPONENT_STATES)]));
            }
        }
    }

    fn property(&mut self, property: &PropertyRef) {
        let child = self.child();
        let t = property.get_type().expect("NullPointerException");
        let c = self.b.component();
        let line = child.borrow().begin_line;
        if self.b.ab.check_non_empty(&child, &t) {
            self.b.process_property_nodes(&child, property, &c);
        } else if self.b.ab.type_table.string_type().is_assignable_to(Some(&t)) {
            self.b.process_property_text(property, "", FROM_CHILD_CDATA, line, &c);
        } else if self.b.ab.type_table.array_type().is_assignable_to(Some(&t)) {
            self.b.process_property_synthetic_array(property, line, &c);
        }
    }

    fn effect(&mut self, effect: &EffectRef) {
        let child = self.child();
        let ct = self.b.ab.type_table.class_type();
        if self.b.ab.check_non_empty(&child, &ct) {
            let c = self.b.component();
            self.b.process_effect_nodes(&child, effect, &c);
        }
    }

    fn style(&mut self, style: &StyleRef) {
        let child = self.child();
        let t = style.get_type().expect("NullPointerException");
        let c = self.b.component();
        if self.b.ab.check_non_empty(&child, &t) {
            self.b.process_style_nodes(&child, style, &c);
        } else if self.b.ab.allow_empty_default(&t) {
            let line = child.borrow().begin_line;
            self.b.process_style_text(style, "", FROM_CHILD_CDATA, line, &c);
        }
    }

    fn dynamic_property(&mut self, name: &str, state: Option<&str>) {
        if self.b.is_document() {
            self.nested_declaration_check(false);
            return;
        }
        let child = self.child();
        let t = self.b.ab.type_table.object_type();
        let c = self.b.component();
        if self.b.ab.check_non_empty(&child, &t) {
            let dynamic_property = self.b.ab.type_table.get_dynamic_property(name, state);
            self.b.process_dynamic_property_nodes(&child, &dynamic_property, &c);
        } else if self.b.ab.allow_empty_default(&t) {
            let line = child.borrow().begin_line;
            self.b.process_dynamic_property_text(name, "", FROM_CHILD_CDATA, line, &c, state);
        }
    }

    fn default_property_element(&mut self, loc_error: bool) {
        if self.b.is_document() {
            self.document_default_property_element(loc_error);
        } else {
            self.component_default_property_element(loc_error);
        }
    }

    fn nested_declaration(&mut self) {
        if matches!(self.b.kind, ComponentKind::WebService(_)) && self.b.web_service_in_operation() {
            self.web_service_nested_declaration();
        } else if self.b.is_document() {
            self.document_nested_declaration();
        } else {
            self.nested_declaration_check(true);
        }
    }

    fn text_content(&mut self) {
        let child = self.child();
        let parent = self.b.cnh.parent();
        if parent.borrow().get_child_count() > 1 {
            self.b.ab.log_node(&child, err("Mixed content not allowed here.", &[]));
        } else if self.b.has_attribute_initializers(&parent) {
            self.b.ab.log_node(&child, err("Component may not have both a text initializer and attributes.", &[]));
        } else {
            self.b.process_text_initializer(&child);
        }
    }

    fn language_node(&mut self) {
        match &mut self.b.kind {
            ComponentKind::Document(_) => self.document_language_node(),
            ComponentKind::HttpService(_) => self.http_service_language_node(),
            ComponentKind::RemoteObject(_) | ComponentKind::WebService(_) => self.service_language_node(),
            ComponentKind::Component => self.component_language_node(),
        }
    }

    fn invoke_child(&mut self, parent: &NodeRef, parent_type: &TypeRef, child: &NodeRef) {
        if self.b.is_document() {
            self.document_invoke_child(parent, parent_type, child);
        } else {
            self.component_invoke_child(parent, parent_type, child);
        }
    }
}
