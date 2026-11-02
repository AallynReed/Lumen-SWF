//! Port of swfutils `flash.fonts.FontManager`: the manager chain, unicode-range parsing and its exceptions.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::asc::flex2::flash::swf::tags::TagRef;
use crate::asc::flex2::fonts::font_description::{FontDescription, JUrl};
use crate::asc::flex2::fonts::font_face::{FaceRef, BOLD, ITALIC};
use crate::asc::flex2::fonts::font_set::FontSet;

pub const LOCAL_FONT_PATHS: &str = "local-font-paths";
pub const PLAIN: i32 = 0;

/// A Java runtime exception thrown through the managers: its class, `getMessage()`, and the range of an
/// `InvalidUnicodeRangeException`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaException {
    pub class_name: String,
    pub message: Option<String>,
    pub range: Option<String>,
}

pub type FmResult<T> = Result<T, JavaException>;

impl JavaException {
    pub fn new(class_name: &str, message: Option<String>) -> JavaException {
        JavaException { class_name: class_name.to_string(), message, range: None }
    }
    pub fn runtime(message: impl Into<String>) -> JavaException {
        Self::new("java.lang.RuntimeException", Some(message.into()))
    }
    pub fn null_pointer() -> JavaException {
        Self::new("java.lang.NullPointerException", None)
    }
    pub fn font_not_found(message: String) -> JavaException {
        Self::new("flash.fonts.FontManager$FontNotFoundException", Some(message))
    }
    pub fn invalid_unicode_range(range: &str) -> JavaException {
        JavaException { class_name: "flash.fonts.FontManager$InvalidUnicodeRangeException".into(), message: None, range: Some(range.to_string()) }
    }
    pub fn is_invalid_unicode_range(&self) -> bool {
        self.class_name == "flash.fonts.FontManager$InvalidUnicodeRangeException"
    }
    pub fn get_message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    /// `Throwable.toString()`.
    pub fn to_java_string(&self) -> String {
        match &self.message {
            Some(m) => format!("{}: {}", self.class_name, m),
            None => self.class_name.clone(),
        }
    }
}

/// The `Map` `FontsConfiguration.getTopLevelManager` initializes the managers with.
#[derive(Clone, Debug, Default)]
pub struct InitMap {
    pub max_cached_fonts: Option<String>,
    pub max_glyphs_per_face: Option<String>,
    pub compatibility_version: Option<String>,
    pub local_fonts_snapshot: Option<String>,
    pub local_font_paths: Option<Vec<String>>,
}

pub type InitMapRef = Option<Rc<InitMap>>;

/// The fields of the abstract `FontManager`.
pub struct FontManagerBase {
    pub language_ranges: RefCell<Option<Rc<Vec<(String, String)>>>>,
    pub parent: RefCell<Option<Rc<dyn FontManager>>>,
    pub major_compatibility_version: Cell<i32>,
}

impl Default for FontManagerBase {
    fn default() -> Self {
        FontManagerBase { language_ranges: RefCell::new(None), parent: RefCell::new(None), major_compatibility_version: Cell::new(4) }
    }
}

impl FontManagerBase {
    /// `FontManager.initialize(map)`.
    pub fn initialize(&self, map: &InitMapRef) {
        if let Some(m) = map {
            if let Some(v) = &m.compatibility_version {
                if let Some(first) = v.split('.').next() {
                    if let Some(n) = java_parse_int(first, 10) {
                        self.major_compatibility_version.set(n);
                    }
                }
            }
        }
    }

    pub fn parent(&self) -> Option<Rc<dyn FontManager>> {
        self.parent.borrow().clone()
    }
}

pub trait FontManager {
    fn base(&self) -> &FontManagerBase;

    fn initialize(&self, map: &InitMapRef);

    fn set_parent(&self, parent: Rc<dyn FontManager>) {
        *self.base().parent.borrow_mut() = Some(parent);
    }

    fn set_language_range(&self, language_ranges: Rc<Vec<(String, String)>>) {
        *self.base().language_ranges.borrow_mut() = Some(language_ranges);
    }

    fn get_language_range(&self, lang: Option<&str>) -> Option<String> {
        let l = self.base().language_ranges.borrow();
        match (l.as_ref(), lang) {
            (Some(l), Some(lang)) => l.iter().find(|(k, _)| k == lang).map(|(_, v)| v.clone()),
            _ => None,
        }
    }

    /// `createDefineFont(tagCode, desc)`: `None` where Java returns null.
    fn create_define_font(&self, _tag_code: i32, _desc: &mut FontDescription) -> FmResult<Option<TagRef>> {
        Ok(None)
    }

    fn get_entry_from_location(&self, location: &JUrl, style: i32, use_twips: bool) -> FmResult<Option<FaceRef>>;

    fn get_entry_from_system(&self, family_name: &str, style: i32, use_twips: bool) -> FmResult<Option<FaceRef>>;

    /// `instanceof CachedFontManager`.
    fn is_cached(&self) -> bool {
        false
    }

    /// `CachedFontManager.createSetForSystemFont`, for the managers that are one.
    fn create_set_for_system_font(&self, _family: &str, _style: i32, _use_twips: bool) -> FmResult<Option<FontSet>> {
        Ok(None)
    }

    fn get_unicode_ranges(&self, value: Option<&str>) -> FmResult<Option<Vec<[i32; 2]>>> {
        let mut value = value.map(str::to_string);
        if let Some(r) = self.get_language_range(value.as_deref()) {
            value = Some(r);
        }
        let Some(v) = value else { return Ok(None) };
        let v = v.replace([';', '\n', '\r', '\u{c}'], " ");
        let tokens: Vec<&str> = v.split(',').filter(|t| !t.is_empty()).collect();
        let mut ranges = vec![[0i32; 2]; tokens.len()];
        parse_ranges(&tokens, &mut ranges)?;
        Ok(Some(ranges))
    }
}

pub fn is_italic(style: i32) -> bool {
    style == ITALIC || style == BOLD + ITALIC
}

pub fn is_bold(style: i32) -> bool {
    style == BOLD || style == BOLD + ITALIC
}

/// `FontManager.create(managerClasses, map, languageRanges)`: unknown class names are skipped as Java skips the
/// classes it cannot load; each manager's parent is the one before it.
pub fn create(manager_classes: Option<&[String]>, map: InitMapRef, language_ranges: Option<Rc<Vec<(String, String)>>>) -> Option<Rc<dyn FontManager>> {
    let mut manager: Option<Rc<dyn FontManager>> = None;
    for class_name in manager_classes.unwrap_or(&[]) {
        let fm: Rc<dyn FontManager> = match class_name.as_str() {
            "flash.fonts.JREFontManager" => Rc::new(crate::asc::flex2::fonts::jre_font_manager::JREFontManager::new()),
            "flash.fonts.BatikFontManager" => Rc::new(crate::asc::flex2::fonts::batik_font_manager::BatikFontManager::new()),
            "flash.fonts.AFEFontManager" => Rc::new(crate::asc::flex2::fonts::adobe::afe_font_manager::AFEFontManager::new()),
            "flash.fonts.CFFFontManager" => Rc::new(crate::asc::flex2::fonts::adobe::cff_font_manager::CFFFontManager::new()),
            _ => continue,
        };
        fm.initialize(&map);
        if let Some(m) = &manager {
            fm.set_parent(m.clone());
        }
        if let Some(l) = &language_ranges {
            fm.set_language_range(l.clone());
        }
        manager = Some(fm);
    }
    manager
}

/// `throwFontNotFound(alias, fontFamily, style, location)`.
pub fn throw_font_not_found(alias: Option<&str>, font_family: Option<&str>, style: i32, location: Option<&str>) -> JavaException {
    let mut m = format!("Font for alias '{}' ", alias.unwrap_or("null"));
    if style == BOLD {
        m.push_str("with bold weight ");
    } else if style == ITALIC {
        m.push_str("with italic style ");
    } else if style == BOLD + ITALIC {
        m.push_str("with bold weight and italic style ");
    } else {
        m.push_str("with plain weight and style ");
    }
    match location {
        Some(l) => {
            m.push_str("was not found at: ");
            m.push_str(l);
        }
        None => {
            m.push_str("was not found by family name '");
            m.push_str(font_family.unwrap_or("null"));
            m.push('\'');
        }
    }
    JavaException::font_not_found(m)
}

/// `Integer.parseInt(s, radix)`.
pub fn java_parse_int(s: &str, radix: u32) -> Option<i32> {
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() {
        return None;
    }
    let mut v: i64 = 0;
    for c in digits.chars() {
        let d = c.to_digit(radix)? as i64;
        v = v * radix as i64 + d;
        if v > i32::MAX as i64 + 1 {
            return None;
        }
    }
    let v = if neg { -v } else { v };
    if v < i32::MIN as i64 || v > i32::MAX as i64 {
        return None;
    }
    Some(v as i32)
}

/// Java `String.trim()`.
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

fn parse_ranges(tokens: &[&str], ranges: &mut [[i32; 2]]) -> FmResult<()> {
    let mut i = 0;
    for t in tokens {
        let element = java_trim(t).to_uppercase();
        if let Some(rest) = element.strip_prefix("U+") {
            let range = java_trim(rest).to_string();
            let (low, high);
            if range.find('?').is_some_and(|p| p > 0) {
                low = range.replace('?', "0");
                high = range.replace('?', "F");
            } else if let Some(dash) = range.find('-').filter(|&p| p > 0) {
                low = range[..dash].to_string();
                let temp = java_trim(&range[dash + 1..]);
                high = match temp.strip_prefix("U+") {
                    Some(h) => java_trim(h).to_string(),
                    None => temp.to_string(),
                };
            } else if crate::asc::util::jstr::utf16_len(&range) <= 8 {
                low = range.clone();
                high = range.clone();
            } else {
                return Err(JavaException::invalid_unicode_range(&range));
            }
            match (java_parse_int(&low, 16), java_parse_int(&high, 16)) {
                (Some(l), Some(h)) => ranges[i] = [l, h],
                _ => return Err(JavaException::invalid_unicode_range(&range)),
            }
            i += 1;
        } else if element.is_empty() {
            continue;
        } else {
            return Err(JavaException::invalid_unicode_range(&element));
        }
    }
    Ok(())
}
