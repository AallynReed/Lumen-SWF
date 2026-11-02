//! Port of swfutils `flash.fonts.CachedFontManager`: the font and font-file caches and the `createDefineFont` that
//! builds a DefineFont2/3 through `FontBuilder`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::asc::flex2::flash::swf::builder::font_builder::FontBuilder;
use crate::asc::flex2::flash::swf::tags::{codes, TagRef};
use crate::asc::flex2::fonts::font_description::{FontDescription, FontSource};
use crate::asc::flex2::fonts::font_manager::{java_parse_int, throw_font_not_found, FmResult, FontManager, InitMapRef, JavaException};
use crate::asc::flex2::fonts::font_set::FontSet;
use crate::asc::flex2::fonts::lru_cache::LRUCache;

pub const PURGE_SIZE: i32 = 2;
pub const DEFAULT_FONT_SIZE: f32 = 240.0;
pub const DEFAULT_FONT_SIZE_STRING: &str = "240";
pub const COMPATIBILITY_VERSION: &str = "CompatibilityVersion";
pub const MAX_CACHED_FONTS_KEY: &str = "max-cached-fonts";
pub const MAX_GLYPHS_PER_FACE_KEY: &str = "max-glyphs-per-face";

/// A `FontFileCache` key: the location string, or (as `JREFontManager` looks it up) the `URL` itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKey {
    Str(String),
    Url(String),
}

pub type FontSetRef = Rc<RefCell<FontSet>>;

/// The fields of the abstract `CachedFontManager`.
pub struct CachedFontManagerBase {
    pub max_entries: Cell<i16>,
    pub max_glyphs_per_face: Cell<i16>,
    pub max_faces_per_font: i16,
    font_cache: RefCell<Option<LRUCache<String, Option<FontSetRef>>>>,
    font_file_cache: RefCell<Option<LRUCache<FileKey, Option<String>>>>,
}

impl Default for CachedFontManagerBase {
    fn default() -> Self {
        CachedFontManagerBase { max_entries: Cell::new(20), max_glyphs_per_face: Cell::new(1000), max_faces_per_font: 4, font_cache: RefCell::new(None), font_file_cache: RefCell::new(None) }
    }
}

/// `Short.parseShort`.
fn parse_short(s: &str) -> Option<i16> {
    java_parse_int(s, 10).and_then(|v| i16::try_from(v).ok())
}

impl CachedFontManagerBase {
    /// `CachedFontManager.initialize(map)` after `super.initialize(map)`.
    pub fn initialize(&self, map: &InitMapRef) {
        if let Some(m) = map {
            if let Some(v) = m.max_cached_fonts.as_deref().and_then(parse_short) {
                self.max_entries.set(v);
            }
            if let Some(v) = m.max_glyphs_per_face.as_deref().and_then(parse_short) {
                self.max_glyphs_per_face.set(v);
            }
        }
        let n = self.max_entries.get() as i32;
        *self.font_cache.borrow_mut() = Some(LRUCache::new(n / 2, n, PURGE_SIZE));
        *self.font_file_cache.borrow_mut() = Some(LRUCache::new(n / 2, n, PURGE_SIZE));
    }

    fn ensure(&self) {
        if self.font_cache.borrow().is_none() || self.font_file_cache.borrow().is_none() {
            self.initialize(&None);
        }
    }

    /// `getFontCache().get(family)`: a miss caches `null`.
    pub fn font_cache_get(&self, family: &str) -> Option<FontSetRef> {
        self.ensure();
        self.font_cache.borrow_mut().as_mut().unwrap().get_or_fetch(&family.to_string(), || None)
    }

    pub fn font_cache_put(&self, family: &str, set: FontSetRef) {
        self.ensure();
        self.font_cache.borrow_mut().as_mut().unwrap().put(family.to_string(), Some(set));
    }

    /// `getFontFileCache().get(key)`: a miss caches `null`.
    pub fn font_file_cache_get(&self, key: &FileKey) -> Option<String> {
        self.ensure();
        self.font_file_cache.borrow_mut().as_mut().unwrap().get_or_fetch(key, || None)
    }

    pub fn font_file_cache_put(&self, key: FileKey, family: String) {
        self.ensure();
        self.font_file_cache.borrow_mut().as_mut().unwrap().put(key, Some(family));
    }
}

/// `CachedFontManager.createDefineFont(tagCode, font)` with `this` = `fm`.
pub fn create_define_font(fm: &dyn FontManager, tag_code: i32, font: &mut FontDescription) -> FmResult<Option<TagRef>> {
    let use_twips = tag_code == codes::DEFINE_FONT3;
    let face = match font.source.clone() {
        Some(FontSource::Url(location)) => {
            let face = fm.get_entry_from_location(&location, font.style, use_twips)?;
            if font.alias.is_none() {
                font.alias = Some(face.as_ref().ok_or_else(JavaException::null_pointer)?.get_family()?);
            }
            match face {
                Some(f) => f,
                None => return Err(throw_font_not_found(font.alias.as_deref(), None, font.style, Some(&location.external_form))),
            }
        }
        Some(FontSource::Name(font_family)) => {
            let face = fm.get_entry_from_system(&font_family, font.style, use_twips)?;
            if font.alias.is_none() {
                font.alias = Some(face.as_ref().ok_or_else(JavaException::null_pointer)?.get_family()?);
            }
            match face {
                Some(f) => f,
                None => return Err(throw_font_not_found(font.alias.as_deref(), Some(&font_family), font.style, None)),
            }
        }
        None => return Err(JavaException::null_pointer()),
    };
    let mut builder = FontBuilder::new(tag_code, face, font.alias.clone().unwrap_or_default(), font.advanced_anti_aliasing)?;
    match fm.get_unicode_ranges(font.unicode_ranges.as_deref())? {
        Some(ranges) => {
            for r in ranges {
                let count = r[1].wrapping_sub(r[0]).wrapping_add(1);
                builder.add_charset(r[0], count)?;
            }
        }
        None => builder.add_all_chars()?,
    }
    Ok(Some(builder.build()?))
}
