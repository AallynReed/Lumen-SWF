//! Port of `flex2.compiler.mxml.dom.Node` and its subclasses (`CDATANode`, `DocumentNode`, `LayeredNode`,
//! `DesignLayerNode`, `ScriptNode`, `StyleNode`, the `PrimitiveNode`s, `ModelNode`, `XMLNode`, ...): one struct, the
//! subclass and its fields in `class`; each Java class's `attributes` set is a `*_ATTRIBUTES` constant.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::analyzer::Analyzer;
use crate::asc::flex2::compiler::mxml::element::Element;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::flex2::flash::css::style_sheet::StyleSheet;
use crate::asc::util::jstr;

pub type NodeRef = Rc<RefCell<Node>>;

/// `String.trim()`.
pub fn java_trim(s: &str) -> String {
    s.trim_matches(|c: char| c <= ' ').to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveClass {
    String,
    Number,
    Int,
    UInt,
    Boolean,
    Class,
    Function,
}

#[derive(Default)]
pub struct DocumentData {
    outer_document_class_name: Option<String>,
    language_namespace: Option<String>,
    version: i32,
    local_class_mappings: Option<Rc<NameMappings>>,
    /// Declared `DesignLayer`s that would otherwise not be associated with any layer children.
    pub layer_declaration_nodes: Vec<NodeRef>,
}

pub enum NodeClass {
    Node,
    CData { in_cdata: bool },
    Document(DocumentData),
    /// The parent layer is held strongly, as in Java (a design layer's children point back at it).
    Layered { layer_parent: Option<NodeRef> },
    DesignLayer { layer_parent: Option<NodeRef> },
    Style { style_sheet: Option<Rc<RefCell<StyleSheet>>> },
    Script { source_file: Option<NodeRef> },
    MetaData,
    Model { source_file: Option<Vec<NodeRef>> },
    Xml { source_file: Option<Vec<NodeRef>> },
    XmlList,
    Array,
    Vector,
    Binding,
    Primitive { class: PrimitiveClass, source_file: Option<NodeRef> },
    WebService,
    HttpService,
    RemoteObject,
    Operation,
    Request,
    Method,
    Arguments,
    InlineComponent { class_qname: Option<QName> },
    Declarations,
    Library,
    Definition { name: Option<QName> },
    Reparent,
    Private,
    State,
}

pub struct Node {
    element: Element,
    index: i32,
    pub comment: Option<String>,
    /// SDK-22601: children are added one at a time so whitespace can be filtered.
    pub(crate) process_children_individually: bool,
    /// SDK-22601: whether pure-whitespace CDATA children are kept.
    pub(crate) preserve_whitespace: bool,
    pub class: NodeClass,
}

impl Deref for Node {
    type Target = Element;
    fn deref(&self) -> &Element {
        &self.element
    }
}

impl DerefMut for Node {
    fn deref_mut(&mut self) -> &mut Element {
        &mut self.element
    }
}

const fn q(local: &'static str) -> (&'static str, &'static str) {
    ("", local)
}

pub const DOCUMENT_NODE_ATTRIBUTES: &[(&str, &str)] = &[];
pub const PRIMITIVE_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id"), q("source"), q("includeIn"), q("excludeFrom"), q("itemCreationPolicy"), q("itemDestructionPolicy")];
pub const DESIGN_LAYER_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id"), q("visible"), q("alpha")];
pub const SCRIPT_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("source"), q("scope")];
pub const STYLE_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("source")];
pub const ARGUMENTS_NODE_ATTRIBUTES: &[(&str, &str)] = &[];
pub const ARRAY_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id"), q("includeIn"), q("excludeFrom"), q("itemCreationPolicy"), q("itemDestructionPolicy")];
pub const BINDING_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("destination"), q("source"), q("twoWay")];
pub const DECLARATIONS_NODE_ATTRIBUTES: &[(&str, &str)] = &[];
pub const DEFINITION_NAME_ATTR: &str = "name";
pub const DEFINITION_NODE_ATTRIBUTES: &[(&str, &str)] = &[q(DEFINITION_NAME_ATTR)];
pub const CLASS_NAME_ATTR: &str = "className";
pub const INLINE_COMPONENT_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id"), q(CLASS_NAME_ATTR)];
pub const LIBRARY_NODE_ATTRIBUTES: &[(&str, &str)] = &[];
pub const META_DATA_NODE_ATTRIBUTES: &[(&str, &str)] = &[];
pub const MODEL_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id"), q("source")];
pub const PRIVATE_NODE_ATTRIBUTES: &[(&str, &str)] = &[];
pub const REPARENT_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("target"), q("includeIn"), q("excludeFrom")];
pub const VECTOR_NODE_ATTRIBUTES: &[(&str, &str)] =
    &[q("id"), q("type"), q("fixed"), q("includeIn"), q("excludeFrom"), q("itemCreationPolicy"), q("itemDestructionPolicy")];
pub const XML_LIST_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id")];
pub const XML_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("id"), q("source"), q("format")];
pub const REQUEST_NODE_ATTRIBUTES: &[(&str, &str)] = &[q("format")];

impl Node {
    pub fn new(uri: &str, local_name: &str, size: usize) -> Node {
        Node::with_class(NodeClass::Node, uri, local_name, size)
    }

    pub fn with_class(class: NodeClass, uri: &str, local_name: &str, size: usize) -> Node {
        Node { element: Element::new(uri, local_name, size), index: 0, comment: None, process_children_individually: false, preserve_whitespace: false, class }
    }

    pub fn into_ref(self) -> NodeRef {
        Rc::new(RefCell::new(self))
    }

    /// `new CDATANode()`.
    pub fn new_cdata() -> Node {
        Node::with_class(NodeClass::CData { in_cdata: false }, "", "", 0)
    }

    /// `new DocumentNode(uri, localName, size)`.
    pub fn new_document(uri: &str, local_name: &str, size: usize) -> Node {
        Node::with_class(NodeClass::Document(DocumentData::default()), uri, local_name, size)
    }

    /// `DocumentNode.inlineDocumentNode(uri, localName, outerDocumentClassName)`.
    pub fn inline_document_node(uri: &str, local_name: &str, outer_document_class_name: &str) -> Node {
        let mut node = Node::new_document(uri, local_name, 0);
        if let NodeClass::Document(d) = &mut node.class {
            d.outer_document_class_name = Some(outer_document_class_name.to_string());
        }
        node
    }

    pub fn element(&self) -> &Element {
        &self.element
    }

    pub fn element_mut(&mut self) -> &mut Element {
        &mut self.element
    }

    pub fn set_index(&mut self, index: i32) {
        self.index = index;
    }

    pub fn get_index(&self) -> i32 {
        self.index
    }

    /// `toString()`.
    pub fn to_java_string(&self) -> String {
        match &self.class {
            NodeClass::CData { .. } => {
                let cdata = java_trim(&self.image.replace(['\r', '\n'], " "));
                let cdata = if jstr::utf16_len(&cdata) > 10 { format!("{}...", jstr::substring(&cdata, 0, 10)) } else { cdata };
                format!("<![[ {cdata} ]]>")
            }
            _ => format!("{} {}", self.image, self.begin_line),
        }
    }

    // ---- Element children, as the subclasses override them ----

    pub fn add_child(&mut self, child: NodeRef) {
        if self.process_children_individually && !self.preserve_whitespace && child.borrow().is_cdata_node() && child.borrow().is_whitespace() {
            return;
        }
        self.element.children.get_or_insert_with(Vec::new).push(child);
    }

    pub fn add_children(&mut self, children: Option<Vec<NodeRef>>) {
        if self.process_children_individually {
            if let Some(children) = children {
                for child in children {
                    self.add_child(child);
                }
                return;
            }
        }
        match (&mut self.element.children, children) {
            (None, c) => self.element.children = c,
            (Some(mine), Some(c)) => mine.extend(c),
            (Some(_), None) => panic!("NullPointerException"),
        }
    }

    pub fn get_child_count(&self) -> usize {
        match &self.class {
            NodeClass::Script { source_file: Some(_) } | NodeClass::Primitive { source_file: Some(_), .. } => 1,
            NodeClass::Model { source_file: Some(s) } | NodeClass::Xml { source_file: Some(s) } => s.len(),
            _ => self.element.children.as_ref().map_or(0, |c| c.len()),
        }
    }

    /// `getChildAt(index)`: `None` where Java returns null (no children list).
    pub fn get_child_at(&self, index: usize) -> Option<NodeRef> {
        match &self.class {
            NodeClass::Script { source_file: Some(s) } | NodeClass::Primitive { source_file: Some(s), .. } if index == 0 => Some(s.clone()),
            NodeClass::Model { source_file: Some(s) } | NodeClass::Xml { source_file: Some(s) } => Some(s[index].clone()),
            _ => self.element.children.as_ref().map(|c| c[index].clone()),
        }
    }

    pub fn get_children(&self) -> Vec<NodeRef> {
        match &self.class {
            NodeClass::Script { source_file: Some(s) } | NodeClass::Primitive { source_file: Some(s), .. } => vec![s.clone()],
            NodeClass::Model { source_file: Some(s) } | NodeClass::Xml { source_file: Some(s) } => s.clone(),
            _ => self.element.children.clone().unwrap_or_default(),
        }
    }

    pub fn to_start_element(&self, w: &mut String) {
        match &self.class {
            NodeClass::CData { in_cdata } => {
                if *in_cdata {
                    w.push_str("<![CDATA[");
                    w.push_str(&self.image);
                    w.push_str("]]>");
                } else {
                    w.push_str(&self.image);
                }
            }
            _ => self.element.to_start_element(w),
        }
    }

    pub fn to_end_element(&self, w: &mut String) {
        if !self.is_cdata_node() {
            self.element.to_end_element(w);
        }
    }

    // ---- instanceof ----

    pub fn is_cdata_node(&self) -> bool {
        matches!(self.class, NodeClass::CData { .. })
    }

    pub fn is_document_node(&self) -> bool {
        matches!(self.class, NodeClass::Document(_))
    }

    /// `instanceof LayeredNode` (also true of a `DesignLayerNode`).
    pub fn is_layered_node(&self) -> bool {
        matches!(self.class, NodeClass::Layered { .. } | NodeClass::DesignLayer { .. })
    }

    pub fn is_design_layer_node(&self) -> bool {
        matches!(self.class, NodeClass::DesignLayer { .. })
    }

    pub fn is_style_node(&self) -> bool {
        matches!(self.class, NodeClass::Style { .. })
    }

    pub fn is_script_node(&self) -> bool {
        matches!(self.class, NodeClass::Script { .. })
    }

    pub fn is_meta_data_node(&self) -> bool {
        matches!(self.class, NodeClass::MetaData)
    }

    pub fn is_model_node(&self) -> bool {
        matches!(self.class, NodeClass::Model { .. })
    }

    pub fn is_xml_node(&self) -> bool {
        matches!(self.class, NodeClass::Xml { .. })
    }

    pub fn is_xml_list_node(&self) -> bool {
        matches!(self.class, NodeClass::XmlList)
    }

    pub fn is_array_node(&self) -> bool {
        matches!(self.class, NodeClass::Array)
    }

    pub fn is_vector_node(&self) -> bool {
        matches!(self.class, NodeClass::Vector)
    }

    pub fn is_binding_node(&self) -> bool {
        matches!(self.class, NodeClass::Binding)
    }

    pub fn is_primitive_node(&self) -> bool {
        matches!(self.class, NodeClass::Primitive { .. })
    }

    pub fn primitive_class(&self) -> Option<PrimitiveClass> {
        match &self.class {
            NodeClass::Primitive { class, .. } => Some(*class),
            _ => None,
        }
    }

    pub fn is_string_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::String)
    }

    pub fn is_number_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::Number)
    }

    pub fn is_int_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::Int)
    }

    pub fn is_uint_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::UInt)
    }

    pub fn is_boolean_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::Boolean)
    }

    pub fn is_class_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::Class)
    }

    pub fn is_function_node(&self) -> bool {
        self.primitive_class() == Some(PrimitiveClass::Function)
    }

    pub fn is_web_service_node(&self) -> bool {
        matches!(self.class, NodeClass::WebService)
    }

    pub fn is_http_service_node(&self) -> bool {
        matches!(self.class, NodeClass::HttpService)
    }

    pub fn is_remote_object_node(&self) -> bool {
        matches!(self.class, NodeClass::RemoteObject)
    }

    pub fn is_operation_node(&self) -> bool {
        matches!(self.class, NodeClass::Operation)
    }

    pub fn is_request_node(&self) -> bool {
        matches!(self.class, NodeClass::Request)
    }

    pub fn is_method_node(&self) -> bool {
        matches!(self.class, NodeClass::Method)
    }

    pub fn is_arguments_node(&self) -> bool {
        matches!(self.class, NodeClass::Arguments)
    }

    pub fn is_inline_component_node(&self) -> bool {
        matches!(self.class, NodeClass::InlineComponent { .. })
    }

    pub fn is_declarations_node(&self) -> bool {
        matches!(self.class, NodeClass::Declarations)
    }

    pub fn is_library_node(&self) -> bool {
        matches!(self.class, NodeClass::Library)
    }

    pub fn is_definition_node(&self) -> bool {
        matches!(self.class, NodeClass::Definition { .. })
    }

    pub fn is_reparent_node(&self) -> bool {
        matches!(self.class, NodeClass::Reparent)
    }

    pub fn is_private_node(&self) -> bool {
        matches!(self.class, NodeClass::Private)
    }

    pub fn is_state_node(&self) -> bool {
        matches!(self.class, NodeClass::State)
    }

    /// The simple name of the node's Java class.
    pub fn class_name(&self) -> &'static str {
        match &self.class {
            NodeClass::Node => "Node",
            NodeClass::CData { .. } => "CDATANode",
            NodeClass::Document(_) => "DocumentNode",
            NodeClass::Layered { .. } => "LayeredNode",
            NodeClass::DesignLayer { .. } => "DesignLayerNode",
            NodeClass::Style { .. } => "StyleNode",
            NodeClass::Script { .. } => "ScriptNode",
            NodeClass::MetaData => "MetaDataNode",
            NodeClass::Model { .. } => "ModelNode",
            NodeClass::Xml { .. } => "XMLNode",
            NodeClass::XmlList => "XMLListNode",
            NodeClass::Array => "ArrayNode",
            NodeClass::Vector => "VectorNode",
            NodeClass::Binding => "BindingNode",
            NodeClass::Primitive { class, .. } => match class {
                PrimitiveClass::String => "StringNode",
                PrimitiveClass::Number => "NumberNode",
                PrimitiveClass::Int => "IntNode",
                PrimitiveClass::UInt => "UIntNode",
                PrimitiveClass::Boolean => "BooleanNode",
                PrimitiveClass::Class => "ClassNode",
                PrimitiveClass::Function => "FunctionNode",
            },
            NodeClass::WebService => "WebServiceNode",
            NodeClass::HttpService => "HTTPServiceNode",
            NodeClass::RemoteObject => "RemoteObjectNode",
            NodeClass::Operation => "OperationNode",
            NodeClass::Request => "RequestNode",
            NodeClass::Method => "MethodNode",
            NodeClass::Arguments => "ArgumentsNode",
            NodeClass::InlineComponent { .. } => "InlineComponentNode",
            NodeClass::Declarations => "DeclarationsNode",
            NodeClass::Library => "LibraryNode",
            NodeClass::Definition { .. } => "DefinitionNode",
            NodeClass::Reparent => "ReparentNode",
            NodeClass::Private => "PrivateNode",
            NodeClass::State => "StateNode",
        }
    }

    // ---- CDATANode ----

    pub fn in_cdata(&self) -> bool {
        matches!(self.class, NodeClass::CData { in_cdata: true })
    }

    pub fn set_in_cdata(&mut self, value: bool) {
        if let NodeClass::CData { in_cdata } = &mut self.class {
            *in_cdata = value;
        }
    }

    /// `CDATANode.isWhitespace()`.
    pub fn is_whitespace(&self) -> bool {
        java_trim(&self.image).is_empty()
    }

    // ---- DocumentNode ----

    fn document(&self) -> &DocumentData {
        match &self.class {
            NodeClass::Document(d) => d,
            _ => panic!("ClassCastException: not a DocumentNode"),
        }
    }

    pub fn document_mut(&mut self) -> &mut DocumentData {
        match &mut self.class {
            NodeClass::Document(d) => d,
            _ => panic!("ClassCastException: not a DocumentNode"),
        }
    }

    pub fn get_outer_document_class_name(&self) -> Option<&str> {
        self.document().outer_document_class_name.as_deref()
    }

    pub fn is_inline_component(&self) -> bool {
        self.document().outer_document_class_name.is_some()
    }

    pub fn get_local_class(&self, namespace: &str, local_part: &str) -> Option<String> {
        self.document().local_class_mappings.as_ref()?.lookup_class_name(namespace, local_part)
    }

    pub fn add_local_class(&mut self, namespace: &str, local_part: &str, class_name: &str) {
        let d = self.document_mut();
        d.local_class_mappings.get_or_insert_with(|| Rc::new(NameMappings::new())).add_class(namespace, local_part, class_name);
    }

    pub fn get_local_class_mappings(&self) -> Option<Rc<NameMappings>> {
        self.document().local_class_mappings.clone()
    }

    pub fn set_local_class_mappings(&mut self, mappings: Option<Rc<NameMappings>>) {
        self.document_mut().local_class_mappings = mappings;
    }

    pub fn get_language_namespace(&self) -> Option<&str> {
        self.document().language_namespace.as_deref()
    }

    pub fn set_language_namespace(&mut self, namespace: Option<&str>) {
        self.document_mut().language_namespace = namespace.map(str::to_string);
    }

    pub fn get_version(&self) -> i32 {
        self.document().version
    }

    pub fn set_version(&mut self, version: i32) {
        self.document_mut().version = version;
    }

    pub fn layer_declaration_nodes(&self) -> &[NodeRef] {
        &self.document().layer_declaration_nodes
    }

    // ---- LayeredNode ----

    pub fn set_layer_parent(&mut self, node: Option<&NodeRef>) {
        if let NodeClass::Layered { layer_parent } | NodeClass::DesignLayer { layer_parent } = &mut self.class {
            *layer_parent = node.cloned();
        }
    }

    pub fn get_layer_parent(&self) -> Option<NodeRef> {
        match &self.class {
            NodeClass::Layered { layer_parent } | NodeClass::DesignLayer { layer_parent } => layer_parent.clone(),
            _ => None,
        }
    }

    // ---- StyleNode ----

    pub fn get_style_sheet(&self) -> Option<Rc<RefCell<StyleSheet>>> {
        match &self.class {
            NodeClass::Style { style_sheet } => style_sheet.clone(),
            _ => None,
        }
    }

    pub fn set_style_sheet(&mut self, sheet: Option<Rc<RefCell<StyleSheet>>>) {
        if let NodeClass::Style { style_sheet } = &mut self.class {
            *style_sheet = sheet;
        }
    }

    // ---- ScriptNode, PrimitiveNode ----

    pub fn set_source_file(&mut self, cdata: NodeRef) {
        match &mut self.class {
            NodeClass::Script { source_file } | NodeClass::Primitive { source_file, .. } => *source_file = Some(cdata),
            _ => panic!("setSourceFile(CDATANode) on {}", self.class_name()),
        }
    }

    pub fn get_source_file(&self) -> Option<NodeRef> {
        match &self.class {
            NodeClass::Script { source_file } | NodeClass::Primitive { source_file, .. } => source_file.clone(),
            _ => None,
        }
    }

    /// `ScriptNode.getText()` / `MetaDataNode.getText()`.
    pub fn get_text(&self) -> Option<String> {
        self.get_child_at(0).map(|c| c.borrow().image.clone())
    }

    // ---- ModelNode, XMLNode ----

    pub fn set_source_file_nodes(&mut self, nodes: Vec<NodeRef>) {
        match &mut self.class {
            NodeClass::Model { source_file } | NodeClass::Xml { source_file } => *source_file = Some(nodes),
            _ => panic!("setSourceFile(Node[]) on {}", self.class_name()),
        }
    }

    pub fn get_source_file_nodes(&self) -> Option<Vec<NodeRef>> {
        match &self.class {
            NodeClass::Model { source_file } | NodeClass::Xml { source_file } => source_file.clone(),
            _ => None,
        }
    }

    /// `XMLNode.isE4X()`.
    pub fn is_e4x(&self) -> bool {
        match self.get_attribute_str("format") {
            None => self.get_attribute_value("format").is_none(),
            Some(f) => f.eq_ignore_ascii_case("E4X"),
        }
    }

    // ---- InlineComponentNode, DefinitionNode ----

    pub fn set_class_qname(&mut self, qname: QName) {
        if let NodeClass::InlineComponent { class_qname } = &mut self.class {
            *class_qname = Some(qname);
        }
    }

    pub fn get_class_qname(&self) -> Option<QName> {
        match &self.class {
            NodeClass::InlineComponent { class_qname } => class_qname.clone(),
            _ => None,
        }
    }

    pub fn set_name(&mut self, qname: QName) {
        if let NodeClass::Definition { name } = &mut self.class {
            *name = Some(qname);
        }
    }

    pub fn get_name(&self) -> Option<QName> {
        match &self.class {
            NodeClass::Definition { name } => name.clone(),
            _ => None,
        }
    }
}

/// `node.analyze(analyzer)`: `prepare`, then the `analyze` overload of the node's class.
pub fn analyze<A: Analyzer + ?Sized>(node: &NodeRef, analyzer: &mut A) {
    analyzer.prepare(node);
    enum K {
        Node,
        CData,
        Layered,
        DesignLayer,
        Style,
        Script,
        MetaData,
        Model,
        Xml,
        XmlList,
        Array,
        Vector,
        Binding,
        Primitive(PrimitiveClass),
        WebService,
        HttpService,
        RemoteObject,
        Operation,
        Request,
        Method,
        Arguments,
        InlineComponent,
        Declarations,
        Library,
        Definition,
        Reparent,
        Private,
        State,
    }
    let k = match &node.borrow().class {
        NodeClass::Node | NodeClass::Document(_) => K::Node,
        NodeClass::CData { .. } => K::CData,
        NodeClass::Layered { .. } => K::Layered,
        NodeClass::DesignLayer { .. } => K::DesignLayer,
        NodeClass::Style { .. } => K::Style,
        NodeClass::Script { .. } => K::Script,
        NodeClass::MetaData => K::MetaData,
        NodeClass::Model { .. } => K::Model,
        NodeClass::Xml { .. } => K::Xml,
        NodeClass::XmlList => K::XmlList,
        NodeClass::Array => K::Array,
        NodeClass::Vector => K::Vector,
        NodeClass::Binding => K::Binding,
        NodeClass::Primitive { class, .. } => K::Primitive(*class),
        NodeClass::WebService => K::WebService,
        NodeClass::HttpService => K::HttpService,
        NodeClass::RemoteObject => K::RemoteObject,
        NodeClass::Operation => K::Operation,
        NodeClass::Request => K::Request,
        NodeClass::Method => K::Method,
        NodeClass::Arguments => K::Arguments,
        NodeClass::InlineComponent { .. } => K::InlineComponent,
        NodeClass::Declarations => K::Declarations,
        NodeClass::Library => K::Library,
        NodeClass::Definition { .. } => K::Definition,
        NodeClass::Reparent => K::Reparent,
        NodeClass::Private => K::Private,
        NodeClass::State => K::State,
    };
    match k {
        K::Node => analyzer.analyze_node(node),
        K::CData => analyzer.analyze_cdata_node(node),
        K::Layered => analyzer.analyze_layered_node(node),
        K::DesignLayer => analyzer.analyze_design_layer_node(node),
        K::Style => analyzer.analyze_style_node(node),
        K::Script => analyzer.analyze_script_node(node),
        K::MetaData => analyzer.analyze_meta_data_node(node),
        K::Model => analyzer.analyze_model_node(node),
        K::Xml => analyzer.analyze_xml_node(node),
        K::XmlList => analyzer.analyze_xml_list_node(node),
        K::Array => analyzer.analyze_array_node(node),
        K::Vector => analyzer.analyze_vector_node(node),
        K::Binding => analyzer.analyze_binding_node(node),
        K::Primitive(PrimitiveClass::String) => analyzer.analyze_string_node(node),
        K::Primitive(PrimitiveClass::Number) => analyzer.analyze_number_node(node),
        K::Primitive(PrimitiveClass::Int) => analyzer.analyze_int_node(node),
        K::Primitive(PrimitiveClass::UInt) => analyzer.analyze_uint_node(node),
        K::Primitive(PrimitiveClass::Boolean) => analyzer.analyze_boolean_node(node),
        K::Primitive(PrimitiveClass::Class) => analyzer.analyze_class_node(node),
        K::Primitive(PrimitiveClass::Function) => analyzer.analyze_function_node(node),
        K::WebService => analyzer.analyze_web_service_node(node),
        K::HttpService => analyzer.analyze_http_service_node(node),
        K::RemoteObject => analyzer.analyze_remote_object_node(node),
        K::Operation => analyzer.analyze_operation_node(node),
        K::Request => analyzer.analyze_request_node(node),
        K::Method => analyzer.analyze_method_node(node),
        K::Arguments => analyzer.analyze_arguments_node(node),
        K::InlineComponent => analyzer.analyze_inline_component_node(node),
        K::Declarations => analyzer.analyze_declarations_node(node),
        K::Library => analyzer.analyze_library_node(node),
        K::Definition => analyzer.analyze_definition_node(node),
        K::Reparent => analyzer.analyze_reparent_node(node),
        K::Private => analyzer.analyze_private_node(node),
        K::State => analyzer.analyze_state_node(node),
    }
}
