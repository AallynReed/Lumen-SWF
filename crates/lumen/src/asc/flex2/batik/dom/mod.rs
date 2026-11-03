//! Ports of the parts of Batik's DOM the SVG transcoder reaches: `AbstractDocument`/`SVGOMDocument` nodes and
//! attributes, `SVGDOMImplementation.createElementNS`, `SVGOMCSSImportedElementRoot`, `importNode`,
//! `XMLBaseSupport` and `SAXDocumentFactory`/`SAXSVGDocumentFactory`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::asc::flex2::batik::css::engine::{CSSEngine, StyleMap};
use crate::asc::flex2::compiler::mxml::dom::xerces::{self, Locator, ParseFailure, SaxAttr, SaxHandler};
use crate::asc::util::jhash::JavaHash;

pub const SVG_NAMESPACE_URI: &str = "http://www.w3.org/2000/svg";
pub const XLINK_NAMESPACE_URI: &str = "http://www.w3.org/1999/xlink";
pub const XML_NAMESPACE_URI: &str = "http://www.w3.org/XML/1998/namespace";
pub const XMLNS_NAMESPACE_URI: &str = "http://www.w3.org/2000/xmlns/";

pub type NodeRef = Rc<Node>;

#[derive(Clone, Debug)]
pub struct Attr {
    pub ns: Option<String>,
    pub qname: String,
    pub local: String,
    pub value: String,
}

pub struct ElementData {
    pub ns: Option<String>,
    pub qname: String,
    /// `getLocalName()`: `None` for a DOM level 1 `GenericElement`.
    pub local: Option<String>,
    pub prefix: Option<String>,
    pub attrs: RefCell<Vec<Attr>>,
    /// `NamedNodeHashMap`'s buckets (indices into `attrs`), which fix the order `item(i)` walks.
    attr_table: RefCell<Vec<Vec<usize>>>,
    /// `CSSStylableElement`.
    pub stylable: bool,
    pub computed_style: RefCell<Option<Rc<RefCell<StyleMap>>>>,
    /// `SVGOMUseElement.getCSSImportedElementRoot()`.
    pub imported_root: RefCell<Option<NodeRef>>,
}

pub struct DocumentData {
    pub url: Option<String>,
    pub css_engine: RefCell<Option<Rc<CSSEngine>>>,
}

pub enum NodeKind {
    Document(DocumentData),
    Element(ElementData),
    Text(String),
    CData(String),
    Comment(String),
    Pi { target: String, data: String },
    /// `SVGOMCSSImportedElementRoot`, whose CSS parent is the `use` element.
    ImportedRoot { css_parent: Weak<Node> },
}

pub struct Node {
    pub kind: NodeKind,
    parent: RefCell<Weak<Node>>,
    owner: RefCell<Weak<Node>>,
    pub children: RefCell<Vec<NodeRef>>,
}

/// A `DOMException`, by its message.
#[derive(Debug, Clone)]
pub struct DomException(pub String);

impl Node {
    fn new(kind: NodeKind, owner: &Weak<Node>) -> NodeRef {
        Rc::new(Node { kind, parent: RefCell::new(Weak::new()), owner: RefCell::new(owner.clone()), children: RefCell::new(Vec::new()) })
    }

    pub fn elem(&self) -> Option<&ElementData> {
        match &self.kind {
            NodeKind::Element(e) => Some(e),
            _ => None,
        }
    }

    pub fn is_element(&self) -> bool {
        matches!(self.kind, NodeKind::Element(_))
    }

    pub fn get_parent_node(&self) -> Option<NodeRef> {
        self.parent.borrow().upgrade()
    }

    pub fn get_owner_document(&self) -> NodeRef {
        self.owner.borrow().upgrade().expect("owner document")
    }

    pub fn doc_data(&self) -> &DocumentData {
        match &self.kind {
            NodeKind::Document(d) => d,
            _ => panic!("not a document"),
        }
    }

    /// The owner document's `getURL()`.
    pub fn get_url(&self) -> Option<String> {
        let d = self.get_owner_document();
        d.doc_data().url.clone()
    }

    pub fn local_name(&self) -> Option<&str> {
        self.elem().and_then(|e| e.local.as_deref())
    }

    pub fn namespace_uri(&self) -> Option<&str> {
        self.elem().and_then(|e| e.ns.as_deref())
    }

    /// `getNodeName()` of an element.
    pub fn node_name(&self) -> &str {
        self.elem().map_or("", |e| e.qname.as_str())
    }

    pub fn is_svg(&self, local: &str) -> bool {
        self.namespace_uri() == Some(SVG_NAMESPACE_URI) && self.local_name() == Some(local)
    }

    /// `getAttributeNS(ns, local)`, `""` when absent.
    pub fn get_attribute_ns(&self, ns: Option<&str>, local: &str) -> String {
        self.elem()
            .and_then(|e| e.attrs.borrow().iter().find(|a| a.ns.as_deref() == ns && a.local == local).map(|a| a.value.clone()))
            .unwrap_or_default()
    }

    pub fn has_attribute_ns(&self, ns: Option<&str>, local: &str) -> bool {
        self.elem().is_some_and(|e| e.attrs.borrow().iter().any(|a| a.ns.as_deref() == ns && a.local == local))
    }

    /// `getAttribute(qname)`.
    pub fn get_attribute(&self, qname: &str) -> String {
        self.elem().and_then(|e| e.attrs.borrow().iter().find(|a| a.qname == qname).map(|a| a.value.clone())).unwrap_or_default()
    }

    /// `setAttributeNS(ns, qname, value)`.
    pub fn set_attribute_ns(&self, ns: Option<&str>, qname: &str, value: &str) {
        let Some(e) = self.elem() else { return };
        let local = qname.rsplit(':').next().unwrap_or(qname).to_string();
        let ns = ns.filter(|n| !n.is_empty()).map(str::to_string);
        let mut attrs = e.attrs.borrow_mut();
        if let Some(a) = attrs.iter_mut().find(|a| a.ns == ns && a.local == local) {
            a.value = value.to_string();
        } else {
            let key = if ns.is_none() { qname } else { local.as_str() };
            let hash = (ns.as_deref().map_or(0, |n| n.java_hash()) ^ key.java_hash()) & 0x7FFF_FFFF;
            let mut table = e.attr_table.borrow_mut();
            let len = table.len();
            if attrs.len() >= (len * 3) >> 2 {
                let old = std::mem::take(&mut *table);
                *table = vec![Vec::new(); len * 2 + 1];
                for bucket in old.iter().rev() {
                    for &i in bucket {
                        let a: &Attr = &attrs[i];
                        let k = if a.ns.is_none() { a.qname.as_str() } else { a.local.as_str() };
                        let h = (a.ns.as_deref().map_or(0, |n| n.java_hash()) ^ k.java_hash()) & 0x7FFF_FFFF;
                        let n = table.len();
                        table[(h as usize) % n].insert(0, i);
                    }
                }
            }
            let n = table.len();
            table[(hash as usize) % n].insert(0, attrs.len());
            attrs.push(Attr { ns, qname: qname.to_string(), local, value: value.to_string() });
        }
    }

    /// The attributes in `getAttributes().item(i)` order.
    pub fn attributes_in_order(&self) -> Vec<Attr> {
        let Some(e) = self.elem() else { return Vec::new() };
        let attrs = e.attrs.borrow();
        e.attr_table.borrow().iter().flatten().map(|&i| attrs[i].clone()).collect()
    }

    pub fn first_child(&self) -> Option<NodeRef> {
        self.children.borrow().first().cloned()
    }

    pub fn child_elements(&self) -> Vec<NodeRef> {
        self.children.borrow().iter().filter(|c| c.is_element()).cloned().collect()
    }

    pub fn previous_sibling(self: &NodeRef) -> Option<NodeRef> {
        let p = self.get_parent_node()?;
        let ch = p.children.borrow();
        let i = ch.iter().position(|c| Rc::ptr_eq(c, self))?;
        if i == 0 {
            None
        } else {
            Some(ch[i - 1].clone())
        }
    }

    pub fn append_child(self: &NodeRef, child: NodeRef) {
        if let Some(old) = child.get_parent_node() {
            old.children.borrow_mut().retain(|c| !Rc::ptr_eq(c, &child));
        }
        *child.parent.borrow_mut() = Rc::downgrade(self);
        self.children.borrow_mut().push(child);
    }

    /// The concatenated text and CDATA children (`getNodeValue` of each).
    pub fn text_content_children(&self) -> String {
        let mut s = String::new();
        for c in self.children.borrow().iter() {
            match &c.kind {
                NodeKind::Text(t) | NodeKind::CData(t) => s.push_str(t),
                _ => {}
            }
        }
        s
    }

    pub fn get_document_element(&self) -> Option<NodeRef> {
        self.children.borrow().iter().find(|c| c.is_element()).cloned()
    }

    /// `getElementById(id)`: the first element in document order with that `id`.
    pub fn get_element_by_id(&self, id: &str) -> Option<NodeRef> {
        fn find(n: &NodeRef, id: &str) -> Option<NodeRef> {
            if n.is_element() && n.has_attribute_ns(None, "id") && n.get_attribute_ns(None, "id") == id {
                return Some(n.clone());
            }
            for c in n.children.borrow().iter() {
                if c.is_element() {
                    if let Some(r) = find(c, id) {
                        return Some(r);
                    }
                }
            }
            None
        }
        let root = self.get_document_element()?;
        find(&root, id)
    }

    pub fn get_css_engine(&self) -> Option<Rc<CSSEngine>> {
        self.get_owner_document().doc_data().css_engine.borrow().clone()
    }

    /// `CSSImportedElementRoot.getCSSParentElement()` for an imported root.
    pub fn css_parent(&self) -> Option<NodeRef> {
        match &self.kind {
            NodeKind::ImportedRoot { css_parent } => css_parent.upgrade(),
            _ => None,
        }
    }
}

/// `new SVGOMDocument(...)` with its URL.
pub fn new_document(url: Option<String>) -> NodeRef {
    Rc::new_cyclic(|w: &Weak<Node>| Node {
        kind: NodeKind::Document(DocumentData { url, css_engine: RefCell::new(None) }),
        parent: RefCell::new(Weak::new()),
        owner: RefCell::new(w.clone()),
        children: RefCell::new(Vec::new()),
    })
}

/// The SVG 1.1 element names `SVGDOMImplementation` has factories for.
const SVG11_ELEMENTS: &[&str] = &[
    "a", "altGlyph", "altGlyphDef", "altGlyphItem", "animate", "animateColor", "animateMotion", "animateTransform", "circle", "clipPath", "color-profile", "cursor", "definition-src", "defs", "desc", "ellipse",
    "feBlend", "feColorMatrix", "feComponentTransfer", "feComposite", "feConvolveMatrix", "feDiffuseLighting", "feDisplacementMap", "feDistantLight", "feFlood", "feFuncA", "feFuncR", "feFuncG", "feFuncB",
    "feGaussianBlur", "feImage", "feMerge", "feMergeNode", "feMorphology", "feOffset", "fePointLight", "feSpecularLighting", "feSpotLight", "feTile", "feTurbulence", "filter", "font", "font-face",
    "font-face-format", "font-face-name", "font-face-src", "font-face-uri", "foreignObject", "g", "glyph", "glyphRef", "hkern", "image", "line", "linearGradient", "marker", "mask", "metadata",
    "missing-glyph", "mpath", "path", "pattern", "polygon", "polyline", "radialGradient", "rect", "set", "script", "stop", "style", "svg", "switch", "symbol", "text", "textPath", "title", "tref", "tspan",
    "use", "view", "vkern",
];

/// The SVG elements whose classes are not `SVGStylableElement`s.
const NON_STYLABLE: &[&str] = &[
    "altGlyphDef", "altGlyphItem", "animate", "animateColor", "animateMotion", "animateTransform", "color-profile", "cursor", "definition-src", "feDistantLight", "feFuncA", "feFuncR", "feFuncG", "feFuncB",
    "feMergeNode", "fePointLight", "feSpotLight", "font-face", "font-face-format", "font-face-name", "font-face-src", "font-face-uri", "hkern", "metadata", "mpath", "script", "set", "style", "view", "vkern",
];

/// `document.createElementNS(namespaceURI, qualifiedName)`.
pub fn create_element_ns(doc: &NodeRef, ns: Option<&str>, qname: &str) -> Result<NodeRef, DomException> {
    let owner = Rc::downgrade(doc);
    let (prefix, local) = match qname.find(':') {
        Some(i) => (Some(qname[..i].to_string()), qname[i + 1..].to_string()),
        None => (None, qname.to_string()),
    };
    let ns = ns.filter(|n| !n.is_empty());
    let (local, stylable) = match ns {
        None => (None, false),
        Some(SVG_NAMESPACE_URI) => {
            if !SVG11_ELEMENTS.contains(&local.as_str()) {
                return Err(DomException(format!("The current document is unable to create an element of the requested type (namespace: {SVG_NAMESPACE_URI}, name: {qname}).")));
            }
            let st = !NON_STYLABLE.contains(&local.as_str());
            (Some(local), st)
        }
        Some(_) => (Some(local), false),
    };
    let is_svg = ns == Some(SVG_NAMESPACE_URI);
    let defaults = if is_svg { attribute_initializer(local.as_deref().unwrap_or("")) } else { &[] };
    let e = Node::new(
        NodeKind::Element(ElementData {
            ns: ns.map(str::to_string),
            qname: qname.to_string(),
            local,
            prefix,
            attrs: RefCell::new(Vec::new()),
            attr_table: RefCell::new(vec![Vec::new(); 3]),
            stylable,
            computed_style: RefCell::new(None),
            imported_root: RefCell::new(None),
        }),
        &owner,
    );
    for (ans, aq, av) in defaults.iter().rev() {
        e.set_attribute_ns(*ans, aq, av);
    }
    Ok(e)
}

const XLINK_DEFAULTS_OTHER: &[(Option<&str>, &str, &str)] = &[
    (Some(XMLNS_NAMESPACE_URI), "xmlns:xlink", XLINK_NAMESPACE_URI),
    (Some(XLINK_NAMESPACE_URI), "xlink:type", "simple"),
    (Some(XLINK_NAMESPACE_URI), "xlink:show", "other"),
    (Some(XLINK_NAMESPACE_URI), "xlink:actuate", "onLoad"),
];
const XLINK_DEFAULTS_EMBED: &[(Option<&str>, &str, &str)] = &[
    (Some(XMLNS_NAMESPACE_URI), "xmlns:xlink", XLINK_NAMESPACE_URI),
    (Some(XLINK_NAMESPACE_URI), "xlink:type", "simple"),
    (Some(XLINK_NAMESPACE_URI), "xlink:show", "embed"),
    (Some(XLINK_NAMESPACE_URI), "xlink:actuate", "onLoad"),
];

/// The `AttributeInitializer` defaults an `SVGOM*Element` is created with.
fn attribute_initializer(local: &str) -> &'static [(Option<&'static str>, &'static str, &'static str)] {
    match local {
        "svg" => &[
            (Some(XMLNS_NAMESPACE_URI), "xmlns", SVG_NAMESPACE_URI),
            (Some(XMLNS_NAMESPACE_URI), "xmlns:xlink", XLINK_NAMESPACE_URI),
            (None, "preserveAspectRatio", "xMidYMid meet"),
            (None, "zoomAndPan", "magnify"),
            (None, "version", "1.0"),
            (None, "contentScriptType", "text/ecmascript"),
            (None, "contentStyleType", "text/css"),
        ],
        "symbol" | "marker" => &[(None, "preserveAspectRatio", "xMidYMid meet")],
        "view" => &[(None, "preserveAspectRatio", "xMidYMid meet"), (None, "zoomAndPan", "magnify")],
        "image" => &[
            (None, "preserveAspectRatio", "xMidYMid meet"),
            (Some(XMLNS_NAMESPACE_URI), "xmlns:xlink", XLINK_NAMESPACE_URI),
            (Some(XLINK_NAMESPACE_URI), "xlink:type", "simple"),
            (Some(XLINK_NAMESPACE_URI), "xlink:show", "embed"),
            (Some(XLINK_NAMESPACE_URI), "xlink:actuate", "onLoad"),
        ],
        "pattern" => &[
            (None, "preserveAspectRatio", "xMidYMid meet"),
            (Some(XMLNS_NAMESPACE_URI), "xmlns:xlink", XLINK_NAMESPACE_URI),
            (Some(XLINK_NAMESPACE_URI), "xlink:type", "simple"),
            (Some(XLINK_NAMESPACE_URI), "xlink:show", "other"),
            (Some(XLINK_NAMESPACE_URI), "xlink:actuate", "onLoad"),
        ],
        "a" => &[
            (Some(XMLNS_NAMESPACE_URI), "xmlns:xlink", XLINK_NAMESPACE_URI),
            (Some(XLINK_NAMESPACE_URI), "xlink:type", "simple"),
            (Some(XLINK_NAMESPACE_URI), "xlink:show", "replace"),
            (Some(XLINK_NAMESPACE_URI), "xlink:actuate", "onRequest"),
        ],
        "use" | "feImage" => XLINK_DEFAULTS_EMBED,
        "altGlyph" | "color-profile" | "cursor" | "filter" | "glyphRef" | "linearGradient" | "radialGradient" | "mpath" | "script" | "textPath" => XLINK_DEFAULTS_OTHER,
        "style" => &[(Some(XML_NAMESPACE_URI), "xml:space", "preserve")],
        _ => &[],
    }
}

/// `new SVGOMCSSImportedElementRoot(document, cssParent, isLocal)`.
pub fn create_imported_root(doc: &NodeRef, css_parent: &NodeRef) -> NodeRef {
    Node::new(NodeKind::ImportedRoot { css_parent: Rc::downgrade(css_parent) }, &Rc::downgrade(doc))
}

/// `document.importNode(node, true, true)`: a deep copy owned by `doc`.
pub fn import_node(doc: &NodeRef, n: &NodeRef) -> NodeRef {
    let owner = Rc::downgrade(doc);
    if let NodeKind::Element(e) = &n.kind {
        let copy = create_element_ns(doc, e.ns.as_deref(), &e.qname).unwrap_or_else(|_| unreachable!());
        for a in n.attributes_in_order() {
            copy.set_attribute_ns(a.ns.as_deref(), &a.qname, &a.value);
        }
        for c in n.children.borrow().iter() {
            copy.append_child(import_node(doc, c));
        }
        return copy;
    }
    let kind = match &n.kind {
        NodeKind::Element(_) => unreachable!(),
        NodeKind::Text(t) => NodeKind::Text(t.clone()),
        NodeKind::CData(t) => NodeKind::CData(t.clone()),
        NodeKind::Comment(t) => NodeKind::Comment(t.clone()),
        NodeKind::Pi { target, data } => NodeKind::Pi { target: target.clone(), data: data.clone() },
        NodeKind::Document(_) | NodeKind::ImportedRoot { .. } => NodeKind::Comment(String::new()),
    };
    let copy = Node::new(kind, &owner);
    for c in n.children.borrow().iter() {
        copy.append_child(import_node(doc, c));
    }
    copy
}

/// `CSSEngine.getParentElement`: the parent element, crossing imported roots to their `use` element.
pub fn get_parent_element(e: &NodeRef) -> Option<NodeRef> {
    let mut n = e.get_parent_node();
    while let Some(node) = n {
        let logical = node.css_parent().unwrap_or(node);
        if logical.is_element() {
            return Some(logical);
        }
        n = logical.get_parent_node();
    }
    None
}

/// `XMLBaseSupport.getCascadedXMLBase(elt)`.
pub fn get_cascaded_xml_base(elt: &NodeRef) -> Option<String> {
    let mut base = None;
    let mut n = elt.get_parent_node();
    while let Some(node) = n {
        if node.is_element() {
            base = get_cascaded_xml_base(&node);
            break;
        }
        n = match node.css_parent() {
            Some(p) => Some(p),
            None => node.get_parent_node(),
        };
    }
    if base.is_none() {
        base = elt.get_url();
    }
    if elt.has_attribute_ns(Some(XML_NAMESPACE_URI), "base") {
        let b = elt.get_attribute_ns(Some(XML_NAMESPACE_URI), "base");
        base = Some(resolve_url(base.as_deref(), &b));
    }
    base
}

/// `new ParsedURL(base, url).toString()` for the `file:`/`#fragment` forms SVG files use.
pub fn resolve_url(base: Option<&str>, url: &str) -> String {
    let Some(base) = base else { return url.to_string() };
    if url.is_empty() {
        return base.to_string();
    }
    if url.starts_with('#') {
        let b = base.split('#').next().unwrap_or(base);
        return format!("{b}{url}");
    }
    if let Some(i) = url.find(':') {
        if i > 1 && url[..i].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
            return url.to_string();
        }
    }
    let b = base.split('#').next().unwrap_or(base);
    let (scheme, path) = match b.find(':') {
        Some(i) => (&b[..=i], &b[i + 1..]),
        None => ("", b),
    };
    if url.starts_with('/') {
        return format!("{scheme}{url}");
    }
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    };
    let mut segs: Vec<&str> = Vec::new();
    let joined = format!("{dir}{url}");
    let lead = joined.starts_with('/');
    for s in joined.split('/') {
        match s {
            "." => {}
            ".." if segs.last().is_some_and(|l| *l != "..") && !segs.is_empty() => {
                segs.pop();
            }
            _ => segs.push(s),
        }
    }
    let mut out = segs.join("/");
    if lead && !out.starts_with('/') {
        out.insert(0, '/');
    }
    format!("{scheme}{out}")
}

/// What `SAXDocumentFactory.createDocument` can throw.
pub enum DocError {
    /// An `IOException` (a SAX error's message, or a root element mismatch).
    Io(String),
    Dom(DomException),
}

const SKIPPABLE_PUBLIC_IDS: &[&str] = &[
    "-//W3C//DTD SVG 1.0//EN",
    "-//W3C//DTD SVG 20010904//EN",
    "-//W3C//DTD SVG 20001102//EN",
    "-//W3C//DTD SVG 20000802//EN",
    "-//W3C//DTD SVG 20000303 Stylable//EN",
    "-//W3C//DTD SVG 1.1//EN",
    "-//W3C//DTD SVG 1.1 Basic//EN",
    "-//W3C//DTD SVG 1.1 Tiny//EN",
    "-//W3C//DTD SVG 1.2//EN",
];
const SKIP_DTD: &str = "<!ELEMENT svg (desc|title|metadata|defs)*> <!ATTLIST svg xmlns CDATA #FIXED \"http://www.w3.org/2000/svg\"               xmlns:xlink CDATA #FIXED \"http://www.w3.org/1999/xlink\" >";

enum PreInfo {
    Text(String),
    CData(String),
    Comment(String),
    Pi(String, String),
}

/// `SAXSVGDocumentFactory` as the SAX handler.
struct SaxSvgDocumentFactory {
    url: Option<String>,
    document: Option<NodeRef>,
    current: Option<NodeRef>,
    namespaces: Vec<Vec<(String, Option<String>)>>,
    pre_info: Vec<PreInfo>,
    in_dtd: bool,
    in_cdata: bool,
    buf: String,
    string_content: bool,
}

impl SaxSvgDocumentFactory {
    fn ns_get(&self, prefix: &str) -> Option<String> {
        for frame in self.namespaces.iter().rev() {
            if let Some((_, u)) = frame.iter().rev().find(|(p, _)| p == prefix) {
                return u.clone();
            }
        }
        None
    }

    fn append_string_data(&mut self) {
        if !self.string_content {
            return;
        }
        let s = std::mem::take(&mut self.buf);
        self.string_content = false;
        match &self.current {
            None => self.pre_info.push(if self.in_cdata { PreInfo::CData(s) } else { PreInfo::Text(s) }),
            Some(c) => {
                let doc = self.document.as_ref().unwrap();
                let n = Node::new(if self.in_cdata { NodeKind::CData(s) } else { NodeKind::Text(s) }, &Rc::downgrade(doc));
                c.append_child(n);
            }
        }
    }
}

impl SaxHandler for SaxSvgDocumentFactory {
    type Error = DomException;

    fn start_document(&mut self, _loc: Locator) -> Result<(), DomException> {
        self.namespaces = vec![vec![
            ("xml".into(), Some(XML_NAMESPACE_URI.into())),
            ("xmlns".into(), Some(XMLNS_NAMESPACE_URI.into())),
            (String::new(), None),
            (String::new(), Some(SVG_NAMESPACE_URI.into())),
            ("xlink".into(), Some(XLINK_NAMESPACE_URI.into())),
        ]];
        Ok(())
    }

    fn end_document(&mut self, _loc: Locator) -> Result<(), DomException> {
        Ok(())
    }

    fn start_prefix_mapping(&mut self, _prefix: &str, _uri: &str, _loc: Locator) -> Result<(), DomException> {
        Ok(())
    }

    fn end_prefix_mapping(&mut self, _prefix: &str, _loc: Locator) -> Result<(), DomException> {
        Ok(())
    }

    fn start_element(&mut self, _uri: &str, _local: &str, qname: &str, attrs: &[SaxAttr], _loc: Locator) -> Result<(), DomException> {
        let mut frame = Vec::new();
        for a in attrs {
            let an = &a.qname;
            if an.chars().count() < 5 || an == "version" || !an.starts_with("xmlns") {
                continue;
            }
            let v = if a.value.is_empty() { None } else { Some(a.value.clone()) };
            if an.len() == 5 {
                frame.push((String::new(), v));
            } else if an.as_bytes()[5] == b':' {
                frame.push((an[6..].to_string(), v));
            }
        }
        self.namespaces.push(frame);
        self.append_string_data();
        let idx = qname.find(':');
        let nsp = match idx {
            Some(i) if i != qname.len() - 1 => &qname[..i],
            _ => "",
        };
        let ns_uri = self.ns_get(nsp);
        let e;
        if self.current.is_none() {
            let doc = new_document(self.url.clone());
            e = create_element_ns(&doc, ns_uri.as_deref(), qname)?;
            for pi in std::mem::take(&mut self.pre_info) {
                let w = Rc::downgrade(&doc);
                let n = match pi {
                    PreInfo::Text(s) => Node::new(NodeKind::Text(s), &w),
                    PreInfo::CData(s) => Node::new(NodeKind::CData(s), &w),
                    PreInfo::Comment(s) => Node::new(NodeKind::Comment(s), &w),
                    PreInfo::Pi(t, d) => Node::new(NodeKind::Pi { target: t, data: d }, &w),
                };
                doc.append_child(n);
            }
            doc.append_child(e.clone());
            self.document = Some(doc);
        } else {
            e = create_element_ns(self.document.as_ref().unwrap(), ns_uri.as_deref(), qname)?;
            self.current.as_ref().unwrap().append_child(e.clone());
        }
        self.current = Some(e.clone());
        for a in attrs {
            let an = &a.qname;
            if an == "xmlns" {
                e.set_attribute_ns(Some(XMLNS_NAMESPACE_URI), an, &a.value);
            } else {
                let ns = an.find(':').and_then(|i| self.ns_get(&an[..i]));
                e.set_attribute_ns(ns.as_deref(), an, &a.value);
            }
        }
        Ok(())
    }

    fn end_element(&mut self, _uri: &str, _local: &str, _qname: &str, _loc: Locator) -> Result<(), DomException> {
        self.append_string_data();
        if let Some(c) = self.current.take() {
            self.current = c.get_parent_node().filter(|p| p.is_element());
        }
        self.namespaces.pop();
        Ok(())
    }

    fn characters(&mut self, text: &str, _loc: Locator) -> Result<(), DomException> {
        self.buf.push_str(text);
        self.string_content = true;
        Ok(())
    }

    fn processing_instruction(&mut self, target: &str, data: &str, _loc: Locator) -> Result<(), DomException> {
        if self.in_dtd {
            return Ok(());
        }
        self.append_string_data();
        match &self.current {
            None => self.pre_info.push(PreInfo::Pi(target.into(), data.into())),
            Some(c) => {
                let n = Node::new(NodeKind::Pi { target: target.into(), data: data.into() }, &Rc::downgrade(self.document.as_ref().unwrap()));
                c.append_child(n);
            }
        }
        Ok(())
    }

    fn start_cdata(&mut self, _loc: Locator) -> Result<(), DomException> {
        self.append_string_data();
        self.in_cdata = true;
        self.string_content = true;
        Ok(())
    }

    fn end_cdata(&mut self, _loc: Locator) -> Result<(), DomException> {
        self.append_string_data();
        self.in_cdata = false;
        Ok(())
    }

    fn comment(&mut self, text: &str, _loc: Locator) -> Result<(), DomException> {
        if self.in_dtd {
            return Ok(());
        }
        self.append_string_data();
        match &self.current {
            None => self.pre_info.push(PreInfo::Comment(text.into())),
            Some(c) => {
                let n = Node::new(NodeKind::Comment(text.into()), &Rc::downgrade(self.document.as_ref().unwrap()));
                c.append_child(n);
            }
        }
        Ok(())
    }

    fn start_dtd(&mut self, _name: &str, _loc: Locator) -> Result<(), DomException> {
        self.append_string_data();
        self.in_dtd = true;
        Ok(())
    }

    fn end_dtd(&mut self, _loc: Locator) -> Result<(), DomException> {
        self.in_dtd = false;
        Ok(())
    }

    fn fatal_error(&mut self, _message: &str, _loc: Locator) -> Result<(), DomException> {
        Ok(())
    }

    fn resolve_entity(&mut self, public_id: Option<&str>, _system_id: Option<&str>) -> Option<String> {
        let p = public_id?;
        SKIPPABLE_PUBLIC_IDS.join("").contains(p).then(|| SKIP_DTD.to_string())
    }
}

/// `new SAXSVGDocumentFactory(parser).createDocument(SVG_NAMESPACE_URI, "svg", uri, inputStream)`.
pub fn create_svg_document(uri: Option<&str>, bytes: Vec<u8>) -> Result<NodeRef, DocError> {
    let mut f = SaxSvgDocumentFactory { url: uri.map(str::to_string), document: None, current: None, namespaces: Vec::new(), pre_info: Vec::new(), in_dtd: false, in_cdata: false, buf: String::new(), string_content: false };
    let opts = xerces::Options { namespace_prefixes: true, dtd: true };
    match xerces::parse_with(bytes, &mut f, opts) {
        Ok(()) => {}
        Err(ParseFailure::Fatal(m)) | Err(ParseFailure::Io(m)) => return Err(DocError::Io(m)),
        Err(ParseFailure::Handler(e)) => return Err(DocError::Dom(e)),
    }
    let doc = f.document.ok_or_else(|| DocError::Io("Premature end of file.".into()))?;
    let root = doc.get_document_element().unwrap();
    let doc_ns = root.namespace_uri().map(str::to_string);
    if doc_ns.as_deref() != Some(SVG_NAMESPACE_URI) {
        return Err(DocError::Io(format!("Root element namespace does not match that requested:\nRequested: {SVG_NAMESPACE_URI}\nFound: {}", doc_ns.as_deref().unwrap_or("null"))));
    }
    if root.local_name() != Some("svg") {
        return Err(DocError::Io(format!("Root element does not match that requested:\nRequested: svg\nFound: {}", root.local_name().unwrap_or("null"))));
    }
    Ok(doc)
}
