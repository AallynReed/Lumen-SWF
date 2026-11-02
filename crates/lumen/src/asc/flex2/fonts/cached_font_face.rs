//! Port of swfutils `flash.fonts.CachedFontFace`: the state its subclasses share, and the glyph cache (an
//! `IntMapLRUCache` whose `get` misses are stored outside its LRU list, so it never purges).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::font_face::{BOLD, ITALIC, PLAIN};
use crate::asc::flex2::fonts::font_manager::FmResult;
use crate::asc::flex2::fonts::fs_type::FSType;
use crate::asc::flex2::fonts::glyph_entry::GlyphEntryRef;

pub const GLYPH_CACHE_PURGE: i32 = 10;

pub struct CachedFontFace {
    pub style: Cell<i32>,
    pub fs_type: RefCell<Option<FSType>>,
    pub copyright: RefCell<Option<String>>,
    pub trademark: RefCell<Option<String>>,
    pub use_twips: bool,
    pub glyph_cache: RefCell<HashMap<u16, Option<GlyphEntryRef>>>,
    pub properties: RefCell<HashMap<String, Rc<dyn Any>>>,
}

impl CachedFontFace {
    pub fn new(_max_cached_glyphs: i32) -> CachedFontFace {
        CachedFontFace {
            style: Cell::new(0),
            fs_type: RefCell::new(None),
            copyright: RefCell::new(None),
            trademark: RefCell::new(None),
            use_twips: false,
            glyph_cache: RefCell::new(HashMap::new()),
            properties: RefCell::new(HashMap::new()),
        }
    }

    pub fn with(max_cached_glyphs: i32, style: i32, fs_type: Option<FSType>, copyright: Option<String>, trademark: Option<String>, use_twips: bool) -> CachedFontFace {
        let mut f = CachedFontFace::new(max_cached_glyphs);
        f.style.set(style);
        *f.fs_type.get_mut() = fs_type;
        *f.copyright.get_mut() = copyright;
        *f.trademark.get_mut() = trademark;
        f.use_twips = use_twips;
        f
    }

    pub fn is_bold(&self) -> bool {
        is_bold(self.style.get())
    }

    pub fn is_italic(&self) -> bool {
        is_italic(self.style.get())
    }

    /// `glyphCache.get(c)`: on a miss, `canDisplay(c) ? createGlyphEntry(c, c) : null`, cached either way.
    pub fn cached_glyph(&self, c: u16, can_display: impl FnOnce() -> FmResult<bool>, create: impl FnOnce() -> FmResult<GlyphEntryRef>) -> FmResult<Option<GlyphEntryRef>> {
        if let Some(e) = self.glyph_cache.borrow().get(&c) {
            return Ok(e.clone());
        }
        let v = if can_display()? { Some(create()?) } else { None };
        self.glyph_cache.borrow_mut().insert(c, v.clone());
        Ok(v)
    }
}

pub fn is_bold(style: i32) -> bool {
    style == BOLD || style == BOLD + ITALIC
}

pub fn is_italic(style: i32) -> bool {
    style == ITALIC || style == ITALIC + BOLD
}

pub fn guess_style_from_sub_family_name(sub_family_name: Option<&str>) -> i32 {
    let mut style = PLAIN;
    if let Some(s) = sub_family_name {
        let s = s.to_lowercase();
        if s.contains("regular") {
            style = PLAIN;
        }
        if s.contains("bold") {
            style += BOLD;
        }
        if s.contains("italic") || s.contains("oblique") {
            style += ITALIC;
        }
    }
    style
}
