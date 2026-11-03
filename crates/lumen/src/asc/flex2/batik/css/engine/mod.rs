//! Ports of Batik's `css.engine.CSSEngine`, `SVGCSSEngine`, `StyleMap`, `StyleSheet` and its rules, and the
//! `css.engine.sac` selector and condition matching (`CSS*Selector.match`/`getSpecificity`).

pub mod value;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use value::{set_shorthand_values, svg_value_managers, PropertyHandler, ValueManager, Value, V, SHORTHANDS};

use crate::asc::flex2::batik::css::parser::conditions::Condition;
use crate::asc::flex2::batik::css::parser::css_lexical_unit::LexicalUnitRef;
use crate::asc::flex2::batik::css::parser::css_sac_media_list::CSSSACMediaList;
use crate::asc::flex2::batik::css::parser::css_selector_list::CSSSelectorList;
use crate::asc::flex2::batik::css::parser::parser::Parser;
use crate::asc::flex2::batik::css::parser::selectors::{Kind as SelKind, SelectorRef};
use crate::asc::flex2::batik::css::sac::{CssError, CssResult, DocumentHandler};
use crate::asc::flex2::batik::dom::{self, Node, NodeKind, NodeRef};
use crate::asc::util::jstr;

pub const USER_AGENT_ORIGIN: i32 = 0;
pub const USER_ORIGIN: i32 = 0x1000_0000;
pub const NON_CSS_ORIGIN: i32 = 0x2000_0000;
pub const AUTHOR_ORIGIN: i32 = 0x3000_0000;
pub const INLINE_AUTHOR_ORIGIN: i32 = 0x4000_0000;

pub const FILL_INDEX: usize = 15;
pub const FILL_OPACITY_INDEX: usize = 16;
pub const FILL_RULE_INDEX: usize = 17;
pub const FILTER_INDEX: usize = 18;
pub const FONT_FAMILY_INDEX: usize = 21;
pub const FONT_SIZE_INDEX: usize = 22;
pub const CLIP_INDEX: usize = 2;
pub const CLIP_PATH_INDEX: usize = 3;
pub const CLIP_RULE_INDEX: usize = 4;
pub const COLOR_INDEX: usize = 5;
pub const COLOR_INTERPOLATION_INDEX: usize = 6;
pub const COLOR_RENDERING_INDEX: usize = 9;
pub const DISPLAY_INDEX: usize = 12;
pub const ENABLE_BACKGROUND_INDEX: usize = 14;
pub const MARKER_END_INDEX: usize = 34;
pub const MARKER_MID_INDEX: usize = 35;
pub const MARKER_START_INDEX: usize = 36;
pub const MASK_INDEX: usize = 37;
pub const OPACITY_INDEX: usize = 38;
pub const OVERFLOW_INDEX: usize = 39;
pub const POINTER_EVENTS_INDEX: usize = 40;
pub const SHAPE_RENDERING_INDEX: usize = 42;
pub const STOP_COLOR_INDEX: usize = 43;
pub const STOP_OPACITY_INDEX: usize = 44;
pub const STROKE_INDEX: usize = 45;
pub const STROKE_DASHARRAY_INDEX: usize = 46;
pub const STROKE_DASHOFFSET_INDEX: usize = 47;
pub const STROKE_LINECAP_INDEX: usize = 48;
pub const STROKE_LINEJOIN_INDEX: usize = 49;
pub const STROKE_MITERLIMIT_INDEX: usize = 50;
pub const STROKE_OPACITY_INDEX: usize = 51;
pub const STROKE_WIDTH_INDEX: usize = 52;
pub const VISIBILITY_INDEX: usize = 57;

/// `CSSContext`: what the engine asks its `BridgeContext`.
pub trait CssContext {
    fn get_pixel_unit_to_millimeter(&self) -> f32;
    fn get_medium_font_size(&self) -> f32;
    fn get_lighter_font_weight(&self, f: f32) -> f32;
    fn get_bolder_font_weight(&self, f: f32) -> f32;
    fn get_block_width(&self, elt: &NodeRef) -> f32;
    fn get_block_height(&self, elt: &NodeRef) -> f32;
    /// `CSSEngineUserAgent.displayError(DOMException)`.
    fn display_error(&self, message: &str);
    /// The text of an external style sheet, or the exception message reading it failed with.
    fn load_style_sheet(&self, url: &str) -> Result<String, String>;
}

/// `StyleMap`.
#[derive(Clone, Default)]
pub struct StyleMap {
    values: Vec<Option<V>>,
    computed: Vec<bool>,
    important: Vec<bool>,
    origin: Vec<i32>,
    pub fixed_cascaded_style: bool,
}

impl StyleMap {
    pub fn new(n: usize) -> StyleMap {
        StyleMap { values: vec![None; n], computed: vec![false; n], important: vec![false; n], origin: vec![0; n], fixed_cascaded_style: false }
    }

    pub fn get_value(&self, i: usize) -> Option<V> {
        self.values[i].clone()
    }

    fn put(&mut self, i: usize, v: V, imp: bool, origin: i32) {
        self.values[i] = Some(v);
        self.important[i] = imp;
        self.origin[i] = origin;
    }
}

pub enum Rule {
    Style { selectors: CSSSelectorList, decl: Vec<(usize, V, bool)> },
    Media { media: CSSSACMediaList, rules: Vec<Rule> },
    Import { media: CSSSACMediaList, uri: Option<String>, rules: Vec<Rule> },
}

/// `StyleSheet` (its rules, media, and the alternate/title flags).
#[derive(Default)]
pub struct StyleSheet {
    pub rules: Vec<Rule>,
    pub media: Option<CSSSACMediaList>,
    pub alternate: bool,
    pub title: Option<String>,
}

pub struct CSSEngine {
    pub document: Weak<Node>,
    pub document_uri: Option<String>,
    pub value_managers: Vec<ValueManager>,
    pub font_size_index: usize,
    pub line_height_index: usize,
    pub color_index: usize,
    media: CSSSACMediaList,
    user_agent_style_sheet: StyleSheet,
    style_sheet_nodes: RefCell<Option<Vec<(NodeRef, Option<Rc<StyleSheet>>)>>>,
    css_base_uri: RefCell<Option<String>>,
    element: RefCell<Option<NodeRef>>,
}

/// `CSSEngine.getParentCSSStylableElement(elt)`.
pub fn get_parent_css_stylable_element(elt: &NodeRef) -> Option<NodeRef> {
    let mut e = dom::get_parent_element(elt);
    while let Some(n) = e {
        if n.elem().is_some_and(|d| d.stylable) {
            return Some(n);
        }
        e = dom::get_parent_element(&n);
    }
    None
}

fn is_space_char(c: char) -> bool {
    matches!(c, ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

fn condition_match(c: &Condition, e: &NodeRef) -> bool {
    match c {
        Condition::And(a, b) => condition_match(a, e) && condition_match(b, e),
        Condition::Attribute { local_name, value, .. } => {
            let attr = e.get_attribute(local_name.as_deref().unwrap_or(""));
            match value {
                None => !attr.is_empty(),
                Some(v) => &attr == v,
            }
        }
        Condition::OneOfAttribute { local_name, value, .. } => {
            let attr: Vec<char> = e.get_attribute(local_name.as_deref().unwrap_or("")).chars().collect();
            let val: Vec<char> = value.as_deref().unwrap_or("").chars().collect();
            let Some(i) = find_chars(&attr, &val, 0) else { return false };
            if i != 0 && !is_space_char(attr[i - 1]) {
                return false;
            }
            let j = i + val.len();
            j == attr.len() || (j < attr.len() && is_space_char(attr[j]))
        }
        Condition::BeginHyphenAttribute { local_name, value, .. } => e.get_attribute(local_name.as_deref().unwrap_or("")).starts_with(value.as_deref().unwrap_or("")),
        Condition::Class { value, .. } => {
            if !e.elem().is_some_and(|d| d.stylable) {
                return false;
            }
            let attr: Vec<char> = e.get_attribute_ns(None, "class").chars().collect();
            let val: Vec<char> = value.as_deref().unwrap_or("").chars().collect();
            let mut i = find_chars(&attr, &val, 0);
            while let Some(p) = i {
                if (p == 0 || is_space_char(attr[p - 1])) && (p + val.len() == attr.len() || is_space_char(attr[p + val.len()])) {
                    return true;
                }
                i = find_chars(&attr, &val, p + val.len());
            }
            false
        }
        Condition::Id { value } => value.as_deref().unwrap_or("") == e.get_attribute("id"),
        Condition::Lang { lang } => {
            let s = e.get_attribute("lang");
            let l = lang.as_deref().unwrap_or("");
            s == l || (s.starts_with(l) && s[l.len()..].starts_with('-'))
        }
        Condition::PseudoClass { value, .. } => {
            if !e.elem().is_some_and(|d| d.stylable) {
                return false;
            }
            if value.as_deref() == Some("first-child") {
                let mut n = e.previous_sibling();
                while let Some(p) = n.clone() {
                    if p.is_element() {
                        break;
                    }
                    n = p.previous_sibling();
                }
                return n.is_none();
            }
            false
        }
    }
}

fn find_chars(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return (from <= hay.len()).then_some(from);
    }
    (from..hay.len().saturating_sub(needle.len() - 1)).find(|&i| hay[i..i + needle.len()] == *needle)
}

fn condition_specificity(c: &Condition) -> i32 {
    match c {
        Condition::And(a, b) => condition_specificity(a) + condition_specificity(b),
        Condition::Id { .. } => 1 << 16,
        _ => 1 << 8,
    }
}

pub fn selector_match(s: &SelectorRef, e: &NodeRef, pseudo: Option<&str>) -> bool {
    match &s.kind {
        SelKind::Element { local_name, .. } => {
            let Some(name) = local_name else { return true };
            let ed = e.elem().unwrap();
            let ename = if ed.prefix.is_none() { ed.qname.as_str() } else { ed.local.as_deref().unwrap_or("") };
            ename == name
        }
        SelKind::PseudoElement { local_name, .. } => pseudo.is_some_and(|p| local_name.as_deref().unwrap_or("").eq_ignore_ascii_case(p)),
        SelKind::Conditional { simple_selector, condition } => selector_match(simple_selector, e, pseudo) && condition_match(condition, e),
        SelKind::Descendant { ancestor_selector, simple_selector } => {
            if !selector_match(simple_selector, e, pseudo) {
                return false;
            }
            let mut n = e.get_parent_node();
            while let Some(p) = n {
                if p.is_element() && selector_match(ancestor_selector, &p, None) {
                    return true;
                }
                n = p.get_parent_node();
            }
            false
        }
        SelKind::Child { ancestor_selector, simple_selector } => match e.get_parent_node() {
            Some(p) if p.is_element() => selector_match(ancestor_selector, &p, None) && selector_match(simple_selector, e, pseudo),
            _ => false,
        },
        SelKind::DirectAdjacent { selector, simple_selector, .. } => {
            if !selector_match(simple_selector, e, pseudo) {
                return false;
            }
            let mut n = e.previous_sibling();
            while let Some(p) = n.clone() {
                if p.is_element() {
                    break;
                }
                n = p.previous_sibling();
            }
            n.is_some_and(|p| selector_match(selector, &p, None))
        }
    }
}

pub fn selector_specificity(s: &SelectorRef) -> i32 {
    match &s.kind {
        SelKind::Element { local_name, .. } => i32::from(local_name.is_some()),
        SelKind::PseudoElement { .. } => 0,
        SelKind::Conditional { simple_selector, condition } => selector_specificity(simple_selector) + condition_specificity(condition),
        SelKind::Descendant { ancestor_selector, simple_selector } | SelKind::Child { ancestor_selector, simple_selector } => selector_specificity(ancestor_selector) + selector_specificity(simple_selector),
        SelKind::DirectAdjacent { selector, simple_selector, .. } => selector_specificity(selector) + selector_specificity(simple_selector),
    }
}

fn selector_list(sl: &CSSSelectorList) -> Vec<SelectorRef> {
    (0..sl.get_length()).filter_map(|i| sl.item(i).cloned()).collect()
}

fn css_message(e: &CssError) -> String {
    e.get_localized_message().unwrap_or_default()
}

/// The parser's `DocumentHandler` while a style sheet is parsed (`StyleSheetDocumentHandler`).
struct SheetHandler {
    engine: *const CSSEngine,
    out: Rc<RefCell<Vec<Vec<Rule>>>>,
    in_style: bool,
}

impl SheetHandler {
    fn engine(&self) -> &CSSEngine {
        unsafe { &*self.engine }
    }

    fn push(&mut self, r: Rule) {
        self.out.borrow_mut().last_mut().unwrap().push(r);
    }
}

struct DeclCollector<'a> {
    decl: &'a mut Vec<(usize, V, bool)>,
}

impl PropertyHandler for DeclCollector<'_> {
    fn property(&mut self, engine: &CSSEngine, name: &str, lu: &LexicalUnitRef, important: bool) -> Result<(), String> {
        match engine.get_property_index(name) {
            Some(i) => {
                let v = engine.value_managers[i].create_value(lu, engine)?;
                self.decl.push((i, v, important));
                Ok(())
            }
            None => {
                if SHORTHANDS.contains(&name) {
                    set_shorthand_values(engine, self, name, lu, important)?;
                }
                Ok(())
            }
        }
    }
}

impl DocumentHandler for SheetHandler {
    fn start_document(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn end_document(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn comment(&mut self, _text: &str) -> CssResult<()> {
        Ok(())
    }
    fn ignorable_at_rule(&mut self, _at_rule: &str) -> CssResult<()> {
        Ok(())
    }
    fn namespace_declaration(&mut self, _prefix: Option<&str>, _uri: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn import_style(&mut self, uri: Option<&str>, media: &CSSSACMediaList, _ns: Option<&str>) -> CssResult<()> {
        let base = self.engine().css_base_uri.borrow().clone();
        let u = uri.map(|u| dom::resolve_url(base.as_deref(), u));
        self.push(Rule::Import { media: media.clone(), uri: u, rules: Vec::new() });
        Ok(())
    }
    fn start_media(&mut self, _media: &CSSSACMediaList) -> CssResult<()> {
        self.out.borrow_mut().push(Vec::new());
        Ok(())
    }
    fn end_media(&mut self, media: &CSSSACMediaList) -> CssResult<()> {
        let rules = self.out.borrow_mut().pop().unwrap_or_default();
        self.push(Rule::Media { media: media.clone(), rules });
        Ok(())
    }
    fn start_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn end_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
        Ok(())
    }
    fn start_font_face(&mut self) -> CssResult<()> {
        self.in_style = false;
        Ok(())
    }
    fn end_font_face(&mut self) -> CssResult<()> {
        Ok(())
    }
    fn start_selector(&mut self, selectors: &CSSSelectorList) -> CssResult<()> {
        self.push(Rule::Style { selectors: selectors.clone(), decl: Vec::new() });
        self.in_style = true;
        Ok(())
    }
    fn end_selector(&mut self, _selectors: &CSSSelectorList) -> CssResult<()> {
        self.in_style = false;
        Ok(())
    }
    fn property(&mut self, name: &str, value: &LexicalUnitRef, important: bool) -> CssResult<()> {
        let engine = self.engine;
        let mut tmp = Vec::new();
        let r = DeclCollector { decl: &mut tmp }.property(unsafe { &*engine }, name, value, important);
        if self.in_style {
            if let Some(Rule::Style { decl, .. }) = self.out.borrow_mut().last_mut().unwrap().last_mut() {
                decl.extend(tmp);
            }
        }
        r.map_err(|m| CssError::Runtime(Some(m)))
    }
}

/// The handler `parseStyleDeclaration` sends a `style` attribute's properties to.
struct InlineHandler {
    engine: *const CSSEngine,
    sm: Rc<RefCell<StyleMap>>,
}

impl PropertyHandler for InlineHandler {
    fn property(&mut self, engine: &CSSEngine, name: &str, lu: &LexicalUnitRef, important: bool) -> Result<(), String> {
        match engine.get_property_index(name) {
            Some(i) => {
                let v = engine.value_managers[i].create_value(lu, engine)?;
                put_author_property(&mut self.sm.borrow_mut(), i, v, important, INLINE_AUTHOR_ORIGIN);
                Ok(())
            }
            None => {
                if SHORTHANDS.contains(&name) {
                    set_shorthand_values(engine, self, name, lu, important)?;
                }
                Ok(())
            }
        }
    }
}

macro_rules! ignore_rest {
    () => {
        fn start_document(&mut self) -> CssResult<()> {
            Ok(())
        }
        fn end_document(&mut self) -> CssResult<()> {
            Ok(())
        }
        fn comment(&mut self, _text: &str) -> CssResult<()> {
            Ok(())
        }
        fn ignorable_at_rule(&mut self, _at_rule: &str) -> CssResult<()> {
            Ok(())
        }
        fn namespace_declaration(&mut self, _prefix: Option<&str>, _uri: Option<&str>) -> CssResult<()> {
            Ok(())
        }
        fn import_style(&mut self, _uri: Option<&str>, _media: &CSSSACMediaList, _ns: Option<&str>) -> CssResult<()> {
            Ok(())
        }
        fn start_media(&mut self, _media: &CSSSACMediaList) -> CssResult<()> {
            Ok(())
        }
        fn end_media(&mut self, _media: &CSSSACMediaList) -> CssResult<()> {
            Ok(())
        }
        fn start_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
            Ok(())
        }
        fn end_page(&mut self, _name: Option<&str>, _pseudo_page: Option<&str>) -> CssResult<()> {
            Ok(())
        }
        fn start_font_face(&mut self) -> CssResult<()> {
            Ok(())
        }
        fn end_font_face(&mut self) -> CssResult<()> {
            Ok(())
        }
        fn start_selector(&mut self, _selectors: &CSSSelectorList) -> CssResult<()> {
            Ok(())
        }
        fn end_selector(&mut self, _selectors: &CSSSelectorList) -> CssResult<()> {
            Ok(())
        }
    };
}

impl DocumentHandler for InlineHandler {
    ignore_rest!();
    fn property(&mut self, name: &str, value: &LexicalUnitRef, important: bool) -> CssResult<()> {
        let engine = self.engine;
        PropertyHandler::property(self, unsafe { &*engine }, name, value, important).map_err(|m| CssError::Runtime(Some(m)))
    }
}

/// `putAuthorProperty(dest, idx, sval, imp, origin)`.
fn put_author_property(dest: &mut StyleMap, idx: usize, sval: V, imp: bool, origin: i32) {
    let cond = match dest.values[idx] {
        None => true,
        Some(_) => match dest.origin[idx] {
            USER_ORIGIN => !dest.important[idx],
            AUTHOR_ORIGIN => !dest.important[idx] || imp,
            _ => true,
        },
    };
    if cond {
        dest.put(idx, sval, imp, origin);
    }
}

struct HintHandler {
    sm: Rc<RefCell<StyleMap>>,
}

impl PropertyHandler for HintHandler {
    fn property(&mut self, engine: &CSSEngine, name: &str, lu: &LexicalUnitRef, important: bool) -> Result<(), String> {
        match engine.get_property_index(name) {
            Some(i) => {
                let v = engine.value_managers[i].create_value(lu, engine)?;
                put_author_property(&mut self.sm.borrow_mut(), i, v, important, NON_CSS_ORIGIN);
                Ok(())
            }
            None => {
                if SHORTHANDS.contains(&name) {
                    set_shorthand_values(engine, self, name, lu, important)?;
                }
                Ok(())
            }
        }
    }
}

impl CSSEngine {
    /// `SVGDOMImplementation.createCSSEngine(doc, ctx)` with the user agent style sheet and `screen` media.
    pub fn new_svg(doc: &NodeRef, ctx: &dyn CssContext) -> Rc<CSSEngine> {
        let vms = svg_value_managers();
        let font_size_index = 22;
        let mut e = CSSEngine {
            document: Rc::downgrade(doc),
            document_uri: doc.doc_data().url.clone(),
            value_managers: vms,
            font_size_index,
            line_height_index: font_size_index,
            color_index: 5,
            media: CSSSACMediaList::new(),
            user_agent_style_sheet: StyleSheet::default(),
            style_sheet_nodes: RefCell::new(None),
            css_base_uri: RefCell::new(None),
            element: RefCell::new(None),
        };
        e.media = Parser::new().parse_media("screen");
        let ua = e.parse_style_sheet_text("svg, symbol, image, marker, pattern, foreignObject { overflow: hidden }", Some("jar:file:batik.jar!/org/apache/flex/forks/batik/dom/svg/resources/UserAgentStyleSheet.css"), "all", ctx);
        e.user_agent_style_sheet = ua;
        Rc::new(e)
    }

    pub fn get_property_index(&self, name: &str) -> Option<usize> {
        self.value_managers.iter().position(|vm| vm.name == name)
    }

    pub fn get_css_base_uri(&self) -> Option<String> {
        if self.css_base_uri.borrow().is_none() {
            let e = self.element.borrow().clone();
            if let Some(e) = e {
                *self.css_base_uri.borrow_mut() = dom::get_cascaded_xml_base(&e);
            }
        }
        self.css_base_uri.borrow().clone()
    }

    fn uri_text(&self) -> String {
        self.document_uri.clone().unwrap_or_else(|| "<unknown>".into())
    }

    /// `parseStyleSheet(String rules, URL uri, String media)`.
    pub fn parse_style_sheet_text(&self, rules: &str, uri: Option<&str>, media: &str, ctx: &dyn CssContext) -> StyleSheet {
        let mut ss = StyleSheet { media: Some(Parser::new().parse_media(media)), ..Default::default() };
        if let Err(m) = self.parse_into(&mut ss, rules, uri, ctx) {
            ctx.display_error(&format!("{}:\nThe following stylesheet represents an invalid \nCSS document.\n{}\nOriginal message:\n{}", uri.unwrap_or("null"), rules, m));
        }
        ss
    }

    /// `parseStyleSheet(URL uri, String media)` for an `xml-stylesheet` reference.
    pub fn parse_style_sheet_url(&self, uri: &str, media: &str, ctx: &dyn CssContext) -> StyleSheet {
        let mut ss = StyleSheet { media: Some(Parser::new().parse_media(media)), ..Default::default() };
        match ctx.load_style_sheet(uri) {
            Ok(text) => {
                if let Err(m) = self.parse_into(&mut ss, &text, Some(uri), ctx) {
                    ctx.display_error(&format!("{uri}:\nInvalid CSS document.\n{m}"));
                }
            }
            Err(m) => ctx.display_error(&format!("{uri}:\nInvalid CSS document.\n{m}")),
        }
        ss
    }

    fn parse_into(&self, ss: &mut StyleSheet, text: &str, uri: Option<&str>, ctx: &dyn CssContext) -> Result<(), String> {
        *self.css_base_uri.borrow_mut() = uri.map(str::to_string);
        let out = Rc::new(RefCell::new(vec![Vec::new()]));
        let mut parser = Parser::new();
        parser.set_document_handler(Box::new(SheetHandler { engine: self as *const CSSEngine, out: out.clone(), in_style: false }));
        let r = parser.parse_style_sheet(jstr::to_utf16(text));
        ss.rules = out.borrow_mut().pop().unwrap_or_default();
        let mut res = r.map_err(|e| css_message(&e));
        if res.is_ok() {
            for r in ss.rules.iter_mut() {
                let Rule::Import { uri: Some(u), rules, .. } = r else { break };
                match ctx.load_style_sheet(u) {
                    Ok(t) => {
                        let mut sub = StyleSheet::default();
                        if let Err(m) = self.parse_into(&mut sub, &t, Some(u), ctx) {
                            res = Err(m);
                        }
                        *rules = sub.rules;
                    }
                    Err(m) => ctx.display_error(&format!("{u}:\nInvalid CSS document.\n{m}")),
                }
            }
        }
        *self.css_base_uri.borrow_mut() = None;
        res
    }

    fn media_match(&self, ml: Option<&CSSSACMediaList>) -> bool {
        let Some(ml) = ml else { return true };
        if self.media.get_length() == 0 || ml.get_length() == 0 {
            return true;
        }
        for i in 0..ml.get_length() {
            let m = ml.item(i).unwrap_or_default();
            if m.eq_ignore_ascii_case("all") {
                return true;
            }
            for j in 0..self.media.get_length() {
                let n = self.media.item(j).unwrap_or_default();
                if n.eq_ignore_ascii_case("all") || m.eq_ignore_ascii_case(&n) {
                    return true;
                }
            }
        }
        false
    }

    fn add_matching_rules<'a>(&self, out: &mut Vec<(&'a CSSSelectorList, &'a Vec<(usize, V, bool)>)>, rules: &'a [Rule], elt: &NodeRef) {
        for r in rules {
            match r {
                Rule::Style { selectors, decl } => {
                    for s in selector_list(selectors) {
                        if selector_match(&s, elt, None) {
                            out.push((selectors, decl));
                        }
                    }
                }
                Rule::Media { media, rules } | Rule::Import { media, rules, .. } => {
                    if self.media_match(Some(media)) {
                        self.add_matching_rules(out, rules, elt);
                    }
                }
            }
        }
    }

    /// `addRules(...)`, with `sortRules`' selection sort by specificity.
    fn add_rules(&self, elt: &NodeRef, sm: &mut StyleMap, mut rules: Vec<(&CSSSelectorList, &Vec<(usize, V, bool)>)>, origin: i32) {
        let len = rules.len();
        for i in 0..len.saturating_sub(1) {
            let mut idx = i;
            let mut min = i32::MAX;
            for (j, r) in rules.iter().enumerate().skip(i) {
                let mut spec = 0;
                for s in selector_list(r.0) {
                    if selector_match(&s, elt, None) {
                        let sp = selector_specificity(&s);
                        if sp > spec {
                            spec = sp;
                        }
                    }
                }
                if spec < min {
                    min = spec;
                    idx = j;
                }
            }
            if i != idx {
                rules.swap(i, idx);
            }
        }
        for (_, decl) in rules {
            for (i, v, imp) in decl.iter() {
                if origin == AUTHOR_ORIGIN {
                    put_author_property(sm, *i, v.clone(), *imp, origin);
                } else {
                    sm.put(*i, v.clone(), *imp, origin);
                }
            }
        }
    }

    /// `getStyleSheetNodes()`: the `style` elements and `xml-stylesheet` instructions, with their sheets.
    fn get_style_sheet_nodes(&self, ctx: &dyn CssContext) {
        if self.style_sheet_nodes.borrow().is_some() {
            return;
        }
        let mut nodes = Vec::new();
        fn find(n: &NodeRef, out: &mut Vec<NodeRef>) {
            let is_sheet = match &n.kind {
                NodeKind::Pi { target, .. } => target == "xml-stylesheet",
                NodeKind::Element(_) => n.is_svg("style"),
                _ => false,
            };
            if is_sheet {
                out.push(n.clone());
            }
            for c in n.children.borrow().iter() {
                find(c, out);
            }
        }
        let doc = self.document.upgrade().unwrap();
        find(&doc, &mut nodes);
        *self.style_sheet_nodes.borrow_mut() = Some(nodes.iter().map(|n| (n.clone(), None)).collect());
        let mut sheets = Vec::new();
        for n in &nodes {
            sheets.push(self.get_css_style_sheet(n, ctx).map(Rc::new));
        }
        let mut b = self.style_sheet_nodes.borrow_mut();
        for (i, s) in sheets.into_iter().enumerate() {
            b.as_mut().unwrap()[i].1 = s;
        }
    }

    /// `CSSStyleSheetNode.getCSSStyleSheet()` of a `style` element or an `xml-stylesheet` instruction.
    fn get_css_style_sheet(&self, n: &NodeRef, ctx: &dyn CssContext) -> Option<StyleSheet> {
        match &n.kind {
            NodeKind::Pi { data, .. } => {
                let attrs = parse_pseudo_attributes(data);
                let get = |k: &str| attrs.iter().find(|a| a.0 == k).map(|a| a.1.clone());
                if get("type").as_deref() != Some("text/css") {
                    return None;
                }
                let href = get("href").unwrap_or_default();
                let url = dom::resolve_url(self.document_uri.as_deref(), &href);
                let mut ss = self.parse_style_sheet_url(&url, get("media").as_deref().unwrap_or("all"), ctx);
                ss.alternate = get("alternate").as_deref().unwrap_or("no") == "yes";
                ss.title = get("title");
                Some(ss)
            }
            _ => {
                let t = n.get_attribute_ns(None, "type");
                if t != "text/css" {
                    return None;
                }
                let text = n.text_content_children();
                let base = dom::get_cascaded_xml_base(n);
                let media = n.get_attribute_ns(None, "media");
                Some(self.parse_style_sheet_text(&text, base.as_deref(), &media, ctx))
            }
        }
    }

    /// `getCascadedStyleMap(elt, null)`.
    pub fn get_cascaded_style_map(&self, elt: &NodeRef, ctx: &dyn CssContext) -> StyleMap {
        let n = self.value_managers.len();
        let result = Rc::new(RefCell::new(StyleMap::new(n)));
        {
            let mut rules = Vec::new();
            self.add_matching_rules(&mut rules, &self.user_agent_style_sheet.rules, elt);
            self.add_rules(elt, &mut result.borrow_mut(), rules, USER_AGENT_ORIGIN);
        }
        *self.element.borrow_mut() = Some(elt.clone());
        let attrs = elt.attributes_in_order();
        for a in &attrs {
            let an = &a.qname;
            if self.get_property_index(an).is_some() || SHORTHANDS.contains(&an.as_str()) {
                let mut parser = Parser::new();
                let r = parser.parse_property_value_string(&a.value).map_err(|e| css_message(&e)).and_then(|lu| match lu {
                    Some(lu) => HintHandler { sm: result.clone() }.property(self, an, &lu, false),
                    None => Err(String::new()),
                });
                if let Err(m) = r {
                    ctx.display_error(&format!("{}:\nThe attribute \"{}\" represents an invalid CSS value (\"{}\").\nOriginal message:\n{}", self.uri_text(), an, a.value, m));
                }
            }
        }
        self.get_style_sheet_nodes(ctx);
        let nodes = self.style_sheet_nodes.borrow();
        let nodes = nodes.as_ref().unwrap();
        if !nodes.is_empty() {
            let mut rules = Vec::new();
            for (_, ss) in nodes.iter() {
                if let Some(ss) = ss {
                    if (!ss.alternate || ss.title.is_none()) && self.media_match(ss.media.as_ref()) {
                        self.add_matching_rules(&mut rules, &ss.rules, elt);
                    }
                }
            }
            self.add_rules(elt, &mut result.borrow_mut(), rules, AUTHOR_ORIGIN);
        }
        let style = elt.get_attribute_ns(None, "style");
        if !style.is_empty() {
            let mut parser = Parser::new();
            parser.set_document_handler(Box::new(InlineHandler { engine: self as *const CSSEngine, sm: result.clone() }));
            if let Err(e) = parser.parse_style_declaration_string(&style) {
                ctx.display_error(&format!("{}:\nThe attribute \"style\" represents an invalid CSS declaration (\"{}\").\nOriginal message:\n{}", self.uri_text(), style, css_message(&e)));
            }
        }
        *self.element.borrow_mut() = None;
        *self.css_base_uri.borrow_mut() = None;
        let r = result.borrow().clone();
        r
    }

    /// `getComputedStyle(elt, null, propidx)`.
    pub fn get_computed_style(&self, elt: &NodeRef, propidx: usize, ctx: &dyn CssContext) -> V {
        let ed = elt.elem().expect("CSSStylableElement");
        let sm = ed.computed_style.borrow().clone();
        let sm = match sm {
            Some(sm) => sm,
            None => {
                let sm = Rc::new(RefCell::new(self.get_cascaded_style_map(elt, ctx)));
                *ed.computed_style.borrow_mut() = Some(sm.clone());
                sm
            }
        };
        let value = sm.borrow().values[propidx].clone();
        if sm.borrow().computed[propidx] {
            return value.unwrap();
        }
        let vm = &self.value_managers[propidx];
        let p = get_parent_css_stylable_element(elt);
        let mut result = value.clone();
        match &value {
            None => {
                if p.is_none() || !vm.inherited {
                    result = Some(vm.get_default_value());
                }
            }
            Some(v) => {
                if p.is_some() && v.is_inherit() {
                    result = None;
                }
            }
        }
        let result = match result {
            None => {
                let p = p.unwrap();
                p.get_css_engine().unwrap().get_computed_style(&p, propidx, ctx)
            }
            Some(r) => {
                let mut tmp = StyleMap::new(0);
                vm.compute_value(elt, self, propidx, &mut tmp, &r, ctx)
            }
        };
        let mut smb = sm.borrow_mut();
        let stored = match &value {
            None => result.clone(),
            Some(v) if !Rc::ptr_eq(v, &result) => Rc::new(Value::Computed { cascaded: v.clone(), computed: result.clone() }),
            Some(_) => result.clone(),
        };
        smb.values[propidx] = Some(stored.clone());
        smb.computed[propidx] = true;
        stored
    }

    /// `importCascadedStyleMaps(src, srceng, dest)`.
    pub fn import_cascaded_style_maps(src: &NodeRef, srceng: &CSSEngine, dest: &NodeRef, ctx: &dyn CssContext) {
        if src.elem().is_some_and(|d| d.stylable) {
            let mut sm = srceng.get_cascaded_style_map(src, ctx);
            sm.fixed_cascaded_style = true;
            *dest.elem().unwrap().computed_style.borrow_mut() = Some(Rc::new(RefCell::new(sm)));
        }
        let sc = src.children.borrow().clone();
        let dc = dest.children.borrow().clone();
        for (s, d) in sc.iter().zip(dc.iter()) {
            if s.is_element() {
                CSSEngine::import_cascaded_style_maps(s, srceng, d, ctx);
            }
        }
    }
}

/// `DOMUtilities.parseStyleSheetPIData`: the `name="value"` pairs of an `xml-stylesheet` instruction.
fn parse_pseudo_attributes(data: &str) -> Vec<(String, String)> {
    let c: Vec<char> = data.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        while i < c.len() && c[i].is_whitespace() {
            i += 1;
        }
        let st = i;
        while i < c.len() && c[i] != '=' && !c[i].is_whitespace() {
            i += 1;
        }
        let name: String = c[st..i].iter().collect();
        while i < c.len() && (c[i].is_whitespace() || c[i] == '=') {
            i += 1;
        }
        if i >= c.len() {
            break;
        }
        let q = c[i];
        if q != '"' && q != '\'' {
            break;
        }
        i += 1;
        let st = i;
        while i < c.len() && c[i] != q {
            i += 1;
        }
        out.push((name, c[st..i.min(c.len())].iter().collect()));
        i += 1;
    }
    out
}
