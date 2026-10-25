//! Ports of `flash.css.Rule`, `StyleRule`, `MediaRule`, `FontFaceRule`, `LocalSource` and `URLSource`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::style_declaration::{StyleDeclaration, StyleDeclarationRef};
use super::style_parser::StyleParser;
use super::style_selector::MediaList;
use crate::asc::flex2::batik::css::parser::css_selector_list::CSSSelectorList;
use crate::asc::flex2::batik::css::sac::lexical_unit::*;
use crate::asc::flex2::batik::css::sac::CSSException;
use crate::asc::flex2::compiler::css::at_embed::{EmbedParams, EmbedValue, FontSource};
use crate::asc::util::jstr;

pub const UNKNOWN_RULE: i16 = 0;
pub const STYLE_RULE: i16 = 1;
pub const CHARSET_RULE: i16 = 2;
pub const IMPORT_RULE: i16 = 3;
pub const MEDIA_RULE: i16 = 4;
pub const FONT_FACE_RULE: i16 = 5;
pub const PAGE_RULE: i16 = 6;

pub type RuleRef = Rc<RefCell<Rule>>;

pub enum Kind {
    Style { selector: Option<String>, selector_list: Rc<CSSSelectorList> },
    Media { media_list: Rc<MediaList>, rules: Vec<RuleRef> },
    FontFace(FontFaceRule),
}

pub struct Rule {
    css_text: Option<String>,
    parent_rule: Weak<RefCell<Rule>>,
    type_: i16,
    unique_id: i32,
    declaration: StyleDeclarationRef,
    pub kind: Kind,
}

impl Rule {
    fn new(type_: i16, path: &str, line_number: i32, kind: Kind) -> RuleRef {
        Rc::new(RefCell::new(Rule { css_text: None, parent_rule: Weak::new(), type_, unique_id: 0, declaration: StyleDeclaration::new(path.to_string(), line_number), kind }))
    }

    pub fn new_style_rule(selector_list: Rc<CSSSelectorList>, path: &str, line_number: i32) -> RuleRef {
        Rule::new(STYLE_RULE, path, line_number, Kind::Style { selector: None, selector_list })
    }

    pub fn new_media_rule(media_list: MediaList, path: &str, line_number: i32) -> RuleRef {
        Rule::new(MEDIA_RULE, path, line_number, Kind::Media { media_list: Rc::new(media_list), rules: Vec::new() })
    }

    pub fn new_font_face_rule(path: &str, line_number: i32) -> RuleRef {
        Rule::new(FONT_FACE_RULE, path, line_number, Kind::FontFace(FontFaceRule::default()))
    }

    pub fn get_style_declaration(&self) -> StyleDeclarationRef {
        self.declaration.clone()
    }

    pub fn get_css_text(&self) -> Option<&str> {
        self.css_text.as_deref()
    }

    pub fn get_parent_rule(&self) -> Option<RuleRef> {
        self.parent_rule.upgrade()
    }

    pub fn get_type(&self) -> i16 {
        self.type_
    }

    pub fn get_unique_id(&self) -> i32 {
        self.unique_id
    }

    pub fn set_css_text(&mut self, css_text: Option<String>) {
        self.css_text = css_text;
    }

    pub fn set_parent_rule(&mut self, parent_rule: &RuleRef) {
        self.parent_rule = Rc::downgrade(parent_rule);
    }

    pub fn set_unique_id(&mut self, unique_id: i32) {
        self.unique_id = unique_id;
    }

    pub fn unparse(&self) -> String {
        String::new()
    }

    pub fn is_style_rule(&self) -> bool {
        matches!(self.kind, Kind::Style { .. })
    }

    pub fn get_selector_list(&self) -> Option<Rc<CSSSelectorList>> {
        match &self.kind {
            Kind::Style { selector_list, .. } => Some(selector_list.clone()),
            _ => None,
        }
    }

    pub fn get_media_list(&self) -> Option<Rc<MediaList>> {
        match &self.kind {
            Kind::Media { media_list, .. } => Some(media_list.clone()),
            _ => None,
        }
    }

    /// `MediaRule.addRule`: only style rules are kept.
    pub fn add_rule(&mut self, rule: &RuleRef) {
        if let Kind::Media { rules, .. } = &mut self.kind {
            if rule.borrow().is_style_rule() {
                rules.push(rule.clone());
            }
        }
    }

    pub fn get_rules(&self) -> Vec<RuleRef> {
        match &self.kind {
            Kind::Media { rules, .. } => rules.clone(),
            _ => Vec::new(),
        }
    }

    pub fn font_face(&self) -> Option<&FontFaceRule> {
        match &self.kind {
            Kind::FontFace(f) => Some(f),
            _ => None,
        }
    }
}

const PLAIN: i32 = 0;
const BOLD: i32 = 1;
const ITALIC: i32 = 2;

/// `FontFaceRule`'s own state; its declaration is the `Rule`'s.
#[derive(Default)]
pub struct FontFaceRule {
    family: Option<String>,
    style: Option<i32>,
    locations: Option<Vec<FontSource>>,
    embed_params: Option<EmbedParams>,
}

/// `toLowerCase()` and `trim()` on a Java string.
fn lower_trim(s: &str) -> String {
    let t: String = s.trim_matches(|c: char| c <= ' ').to_string();
    jstr::from_utf16(&jstr::units(&t).map(java_lower).collect::<Vec<u16>>())
}

fn java_lower(c: u16) -> u16 {
    crate::asc::flex2::batik::css::parser::scanner::java_to_lower(c)
}

/// `FontFaceRule.isItalic(String)`.
pub fn is_italic_value(value: Option<&str>) -> bool {
    let Some(v) = value else { return false };
    let mut ital = lower_trim(v);
    if let Some(r) = ital.strip_prefix('"') {
        ital = r.to_string();
        if let Some(r) = ital.strip_suffix('"') {
            ital = r.to_string();
        }
    }
    ital == "italic" || ital == "oblique"
}

/// `FontFaceRule.isBold(String)`.
pub fn is_bold_value(value: Option<&str>) -> bool {
    let Some(v) = value else { return false };
    let mut b = lower_trim(v);
    if let Some(r) = b.strip_prefix('"') {
        b = r.to_string();
        if let Some(r) = b.strip_suffix('"') {
            b = r.to_string();
        }
    }
    if b.starts_with("bold") {
        return true;
    }
    b.parse::<i32>().is_ok_and(|w| w >= 700)
}

/// The first `StringTokenizer(s, ",")` token, trimmed and lower-cased; Java throws if there is none.
fn first_token(s: &str) -> Option<String> {
    s.split(',').find(|t| !t.is_empty()).map(lower_trim)
}

impl Rule {
    fn ff(&self) -> &FontFaceRule {
        self.font_face().expect("ClassCastException")
    }

    fn ff_mut(&mut self) -> &mut FontFaceRule {
        match &mut self.kind {
            Kind::FontFace(f) => f,
            _ => panic!("ClassCastException"),
        }
    }

    pub fn get_embed_params(&self) -> Option<&EmbedParams> {
        self.ff().embed_params.as_ref()
    }

    pub fn get_embed_params_mut(&mut self) -> Option<&mut EmbedParams> {
        self.ff_mut().embed_params.as_mut()
    }

    /// `FontFaceRule.initialize(parser)`.
    pub fn initialize(&mut self, parser: &StyleParser) -> Result<(), Option<String>> {
        let alias = self.get_family();
        let locations = self.get_locations()?;
        let mut embed_params = EmbedParams::new();
        embed_params.insert("mimeType".into(), EmbedValue::Str("application/x-font".into()));
        match (&locations, &alias) {
            (None, Some(a)) => {
                embed_params.insert("fontName".into(), EmbedValue::Str(a.clone()));
                embed_params.insert("systemFont".into(), EmbedValue::Str(a.clone()));
            }
            (Some(locs), Some(a)) => {
                embed_params.insert("fontName".into(), EmbedValue::Str(a.clone()));
                if locs.len() == 1 {
                    match &locs[0] {
                        FontSource::Url(v) => {
                            embed_params.insert("source".into(), EmbedValue::Str(v.clone()));
                            if lower_trim_keep(v).ends_with(".swf") {
                                embed_params.insert("mimeType".into(), EmbedValue::Str("application/x-shockwave-flash".into()));
                            }
                        }
                        FontSource::Local(v) => {
                            embed_params.insert("systemFont".into(), EmbedValue::Str(v.clone()));
                        }
                    }
                } else {
                    embed_params.insert("sourceList".into(), EmbedValue::Sources(locs.clone()));
                }
            }
            _ => {}
        }
        if let Some(r) = self.get_unicode_range() {
            embed_params.insert("unicodeRange".into(), EmbedValue::Str(r));
        }
        if let Some(t) = self.get_flash_type() {
            embed_params.insert("advancedAntiAliasing".into(), EmbedValue::Str(t));
        }
        if let Some(c) = self.get_compact_font_format() {
            embed_params.insert("embedAsCFF".into(), EmbedValue::Str(c));
        }
        self.check_for_ignored_descriptors(parser);
        if self.is_bold()? {
            embed_params.insert("fontWeight".into(), EmbedValue::Str("bold".into()));
        }
        if self.is_italic()? {
            embed_params.insert("fontStyle".into(), EmbedValue::Str("italic".into()));
        }
        self.ff_mut().embed_params = Some(embed_params);
        Ok(())
    }

    fn descriptor(&self, a: &str, b: &str) -> Option<std::rc::Rc<super::descriptor::Descriptor>> {
        let d = self.declaration.borrow();
        d.get_descriptor_value(a).or_else(|| d.get_descriptor_value(b))
    }

    pub fn get_family(&mut self) -> Option<String> {
        if self.ff().family.is_none() {
            if let Some(f) = self.descriptor("font-family", "fontFamily") {
                self.ff_mut().family = Some(f.get_ident_as_string());
            }
        }
        self.ff().family.clone()
    }

    /// `getLocations()`; an `Err` is the `RuntimeException` `local()` without a string parameter throws.
    pub fn get_locations(&mut self) -> Result<Option<Vec<FontSource>>, Option<String>> {
        if self.ff().locations.is_none() {
            let src = self.declaration.borrow().get_descriptor_value("src");
            let mut result = None;
            if let Some(src) = src {
                let mut r = Vec::with_capacity(2);
                let mut lu = Some(src.get_value().clone());
                while let Some(u) = lu {
                    if u.get_lexical_unit_type() == SAC_FUNCTION {
                        let p = u.get_parameters().ok().flatten().ok_or(None)?;
                        let s = p.get_string_value().map_err(|_| None)?;
                        r.push(FontSource::Local(s.trim_matches(|c: char| c <= ' ').to_string()));
                    } else if u.get_lexical_unit_type() == SAC_URI {
                        r.push(FontSource::Url(u.get_string_value().unwrap_or_default()));
                    }
                    lu = u.get_next_lexical_unit();
                }
                result = Some(r);
            }
            self.ff_mut().locations = result;
        }
        Ok(self.ff().locations.clone())
    }

    pub fn get_font_style(&mut self) -> Result<i32, Option<String>> {
        if self.ff().style.is_none() {
            let mut style = PLAIN;
            let s = self.descriptor("font-style", "fontStyle");
            let w = self.descriptor("font-weight", "fontWeight");
            if let Some(s) = s {
                let t = first_token(&s.get_ident_as_string()).ok_or(None)?;
                if is_italic_value(Some(&t)) {
                    style += ITALIC;
                }
            }
            if let Some(w) = w {
                let t = first_token(&w.get_ident_as_string()).ok_or(None)?;
                if is_bold_value(Some(&t)) {
                    style += BOLD;
                }
            }
            self.ff_mut().style = Some(style);
        }
        Ok(self.ff().style.unwrap())
    }

    pub fn is_bold(&mut self) -> Result<bool, Option<String>> {
        let style = self.get_font_style()?;
        Ok(style == BOLD || style == BOLD + ITALIC)
    }

    pub fn is_italic(&mut self) -> Result<bool, Option<String>> {
        let style = self.get_font_style()?;
        Ok(style == ITALIC || style == BOLD + ITALIC)
    }

    pub fn get_unicode_range(&self) -> Option<String> {
        let property = self.descriptor("unicode-range", "unicodeRange")?;
        let mut r = property.get_ident_as_string().trim_matches(|c: char| c <= ' ').to_string();
        if let Some(i) = r.find(';') {
            r.truncate(i);
        }
        Some(r)
    }

    fn get_flash_type(&self) -> Option<String> {
        let d = self.descriptor("advanced-anti-aliasing", "advancedAntiAliasing").or_else(|| self.descriptor("flash-type", "flashType"))?;
        Some(d.get_ident_as_string())
    }

    fn get_compact_font_format(&self) -> Option<String> {
        let d = self.descriptor("embedAsCFF", "embed-as-cff")?;
        Some(d.get_ident_as_string())
    }

    fn check_for_ignored_descriptors(&self, parser: &StyleParser) {
        const IGNORED: [&str; 20] = [
            "font-size",
            "fontSize",
            "panose-1",
            "stemv",
            "stemh",
            "slope",
            "cap-height",
            "capHeight",
            "x-height",
            "xHeight",
            "ascent",
            "descent",
            "widths",
            "bbox",
            "definition-src",
            "definitionSrc",
            "baseline",
            "centerline",
            "mathline",
            "topline",
        ];
        let d = self.declaration.borrow();
        if let Some(descriptor) = IGNORED.iter().find(|n| d.get_descriptor_value(n).is_some()) {
            let text = format!("'{descriptor}' is ignored inside @font-face.");
            parser.warning(&CSSException::with(0, Some(text)));
        }
    }
}

/// `toLowerCase()` without trimming.
fn lower_trim_keep(s: &str) -> String {
    jstr::from_utf16(&jstr::units(s).map(java_lower).collect::<Vec<u16>>())
}

/// `FontFaceRule.getRule(rules, family, bold, italic)`.
pub fn get_rule(rules: &[RuleRef], family: Option<&str>, bold: bool, italic: bool) -> Result<Option<RuleRef>, Option<String>> {
    for r in rules {
        let mut ffr = r.borrow_mut();
        let f = ffr.get_family();
        let family = family.ok_or_else(|| Some("Cannot invoke \"String.equals(Object)\" because \"family\" is null".to_string()))?;
        if Some(family) == f.as_deref() {
            if bold != ffr.is_bold()? {
                continue;
            }
            if italic != ffr.is_italic()? {
                continue;
            }
            return Ok(Some(r.clone()));
        }
    }
    Ok(None)
}

/// `FontFaceRule.hasFamily(rules, family)`.
pub fn has_family(rules: &[RuleRef], family: &str) -> bool {
    rules.iter().any(|r| r.borrow_mut().get_family().as_deref() == Some(family))
}
