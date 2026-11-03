//! Port of Batik's `bridge.BridgeContext` with the `SpriteTranscoder.SwfTranscoderUserAgent` it is given, its
//! `CSSContext` side, `URIResolver` and `DocumentLoader`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::{bridge_exception, BResult, ERR_URI_BAD_TARGET, ERR_URI_IO, ERR_URI_REFERENCE_A_DOCUMENT};
use crate::asc::flex2::batik::css::engine::{CSSEngine, CssContext};
use crate::asc::flex2::batik::dom::{self, NodeRef};

/// `UserAgent.getPixelUnitToMillimeter()` (96 dpi).
pub const PIXEL_UNIT_TO_MILLIMETER: f32 = 0.264_583_33;

pub struct BridgeContext {
    pub document: NodeRef,
    viewport_stack: RefCell<Option<Vec<(f32, f32)>>>,
    viewport_map: RefCell<Vec<(NodeRef, (f32, f32))>>,
    pub document_size: Cell<Option<(i32, i32)>>,
    /// `DocumentLoader`'s cache of the other documents loaded.
    loaded: RefCell<Vec<(String, NodeRef)>>,
    /// The `ERROR:` lines `DefaultErrorHandler.error` printed, in order.
    pub errors: RefCell<Vec<String>>,
}

impl BridgeContext {
    pub fn new(document: NodeRef) -> BridgeContext {
        BridgeContext {
            document,
            viewport_stack: RefCell::new(Some(Vec::new())),
            viewport_map: RefCell::new(Vec::new()),
            document_size: Cell::new(None),
            loaded: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
        }
    }

    /// `initializeDocument(document)`: the document's CSS engine.
    pub fn initialize_document(&self, doc: &NodeRef) {
        if doc.doc_data().css_engine.borrow().is_none() {
            let eng = CSSEngine::new_svg(doc, self);
            *doc.doc_data().css_engine.borrow_mut() = Some(eng);
        }
    }

    /// `getViewport(e)`.
    pub fn get_viewport(&self, e: &NodeRef) -> (f32, f32) {
        if let Some(stack) = self.viewport_stack.borrow().as_ref() {
            return stack.first().copied().unwrap_or((400.0, 400.0));
        }
        let map = self.viewport_map.borrow();
        let mut p = dom::get_parent_element(e);
        while let Some(n) = p {
            if let Some((_, v)) = map.iter().find(|(k, _)| Rc::ptr_eq(k, &n)) {
                return *v;
            }
            p = dom::get_parent_element(&n);
        }
        (400.0, 400.0)
    }

    pub fn open_viewport(&self, e: &NodeRef, v: (f32, f32)) {
        self.viewport_map.borrow_mut().push((e.clone(), v));
        let mut s = self.viewport_stack.borrow_mut();
        s.get_or_insert_with(Vec::new).insert(0, v);
    }

    pub fn close_viewport(&self) {
        let mut s = self.viewport_stack.borrow_mut();
        if let Some(st) = s.as_mut() {
            if !st.is_empty() {
                st.remove(0);
            }
            if st.is_empty() {
                *s = None;
            }
        }
    }

    /// `UserAgent.displayError(e)` through `DefaultErrorHandler.error`: printed on standard error.
    pub fn display_error_message(&self, message: &str) {
        let line = format!("ERROR: null\nEnclosed Exception:\n{message}");
        print_stderr(&line);
        self.errors.borrow_mut().push(line);
    }

    /// `DocumentLoader.loadDocument(uri)` for a `file:` URI.
    fn load_document(&self, uri: &str) -> Result<NodeRef, ()> {
        if let Some((_, d)) = self.loaded.borrow().iter().find(|(u, _)| u == uri) {
            return Ok(d.clone());
        }
        let path = uri.strip_prefix("file:").ok_or(())?;
        let bytes = std::fs::read(file_path(path)).map_err(|_| ())?;
        let doc = dom::create_svg_document(Some(uri), bytes).map_err(|_| ())?;
        self.initialize_document(&doc);
        self.loaded.borrow_mut().push((uri.to_string(), doc.clone()));
        Ok(doc)
    }

    /// `new URIResolver(document, loader).getElement(uri, e)`: `Ok(None)` when no element has the id.
    pub fn uri_get_element(&self, e: &NodeRef, uri: &str) -> BResult<Option<NodeRef>> {
        let base = dom::get_cascaded_xml_base(e);
        let doc = e.get_owner_document();
        if base.is_none() && uri.starts_with('#') {
            return Ok(doc.get_element_by_id(&uri[1..]));
        }
        let purl = dom::resolve_url(base.as_deref(), uri);
        let (file, frag) = match purl.find('#') {
            Some(i) => (&purl[..i], Some(&purl[i + 1..])),
            None => (purl.as_str(), None),
        };
        let doc_uri = doc.doc_data().url.clone();
        if let (Some(f), Some(d)) = (frag, doc_uri.as_deref()) {
            if d.split('#').next() == Some(file) {
                return Ok(doc.get_element_by_id(f));
            }
        }
        let other = self.load_document(file).map_err(|_| bridge_exception(e, ERR_URI_IO, &[uri]))?;
        match frag {
            Some(f) => Ok(other.get_element_by_id(f)),
            None => Err(bridge_exception(e, ERR_URI_REFERENCE_A_DOCUMENT, &[uri])),
        }
    }

    /// `getReferencedElement(e, uri)`.
    pub fn get_referenced_element(&self, e: &NodeRef, uri: &str) -> BResult<NodeRef> {
        self.uri_get_element(e, uri)?.ok_or_else(|| bridge_exception(e, ERR_URI_BAD_TARGET, &[uri]))
    }
}

/// A line the Java side writes to `System.err`, kept in order with the compiler's own messages.
pub fn print_stderr(text: &str) {
    crate::asc::flex2::compiler::toolkit::log_plain(crate::asc::flex2::Level::Info, text);
}

/// A `file:` URL's path as a local path (the `/C:/...` form on Windows).
pub fn file_path(p: &str) -> std::path::PathBuf {
    let p = if cfg!(windows) && p.len() > 2 && p.as_bytes()[0] == b'/' && p.as_bytes()[2] == b':' { &p[1..] } else { p };
    std::path::PathBuf::from(p)
}

impl CssContext for BridgeContext {
    fn get_pixel_unit_to_millimeter(&self) -> f32 {
        PIXEL_UNIT_TO_MILLIMETER
    }
    fn get_medium_font_size(&self) -> f32 {
        14.0
    }
    fn get_lighter_font_weight(&self, _f: f32) -> f32 {
        100.0
    }
    fn get_bolder_font_weight(&self, _f: f32) -> f32 {
        700.0
    }
    fn get_block_width(&self, elt: &NodeRef) -> f32 {
        self.get_viewport(elt).0
    }
    fn get_block_height(&self, elt: &NodeRef) -> f32 {
        self.get_viewport(elt).1
    }
    fn display_error(&self, message: &str) {
        self.display_error_message(message);
    }
    fn load_style_sheet(&self, url: &str) -> Result<String, String> {
        let Some(path) = url.strip_prefix("file:") else { return Err(format!("unknown protocol: {}", url.split(':').next().unwrap_or(""))) };
        let p = file_path(path);
        match std::fs::read(&p) {
            Ok(b) => Ok(String::from_utf8_lossy(&b).into_owned()),
            Err(_) => Err(format!("{} (No such file or directory)", p.display())),
        }
    }
}
