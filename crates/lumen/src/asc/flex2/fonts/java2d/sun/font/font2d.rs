//! Port of JDK 17 `sun.font.Font2D` and `Font2DHandle`; the subclasses (`TrueTypeFont`, `CompositeFont`) are the
//! `Font2DKind` variants.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::java::util::locale::Locale;
use crate::asc::flex2::fonts::java2d::sun::font::char_to_glyph_mapper::CharToGlyphMapper;
use crate::asc::flex2::fonts::java2d::sun::font::composite_font::CompositeFont;
use crate::asc::flex2::fonts::java2d::sun::font::composite_glyph_mapper::CompositeGlyphMapper;
use crate::asc::flex2::fonts::java2d::sun::font::composite_strike::CompositeStrike;
use crate::asc::flex2::fonts::java2d::sun::font::file_font_strike::FileFontStrike;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike::FontStrike;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike_desc::FontStrikeDesc;
use crate::asc::flex2::fonts::java2d::sun::font::true_type_font::TrueTypeFont;
use crate::asc::flex2::fonts::java2d::sun::font::true_type_glyph_mapper::TrueTypeGlyphMapper;

pub const FONT_PLAIN: i32 = 0;
pub const FONT_BOLD: i32 = 1;
pub const FONT_ITALIC: i32 = 2;

pub const FONT_CONFIG_RANK: i32 = 2;
pub const JRE_RANK: i32 = 2;
pub const TTF_RANK: i32 = 3;
pub const TYPE1_RANK: i32 = 4;
pub const NATIVE_RANK: i32 = 5;
pub const UNKNOWN_RANK: i32 = 6;
pub const DEFAULT_RANK: i32 = 4;

pub const FWIDTH_NORMAL: i32 = 5;
pub const FWEIGHT_NORMAL: i32 = 400;
pub const FWEIGHT_BOLD: i32 = 700;

const BOLD_NAMES: [&str; 6] = ["bold", "demibold", "demi-bold", "demi bold", "negreta", "demi"];
const ITALIC_NAMES: [&str; 4] = ["italic", "cursiva", "oblique", "inclined"];
const BOLD_ITALIC_NAMES: [&str; 9] = ["bolditalic", "bold-italic", "bold italic", "boldoblique", "bold-oblique", "bold oblique", "demibold italic", "negreta cursiva", "demi oblique"];

/// `Font2D.setStyle()`: the style the full name implies, else `style` unchanged.
pub fn style_from_name(full_name: &str, style: i32) -> i32 {
    let f_name = full_name.to_lowercase();
    if BOLD_ITALIC_NAMES.iter().any(|n| f_name.contains(n)) {
        return FONT_BOLD | FONT_ITALIC;
    }
    if ITALIC_NAMES.iter().any(|n| f_name.contains(n)) {
        return FONT_ITALIC;
    }
    if BOLD_NAMES.iter().any(|n| f_name.contains(n)) {
        return FONT_BOLD;
    }
    style
}

pub struct Font2DHandle {
    pub font2d: RefCell<Rc<Font2D>>,
}

impl Font2DHandle {
    pub fn get(&self) -> Rc<Font2D> {
        self.font2d.borrow().clone()
    }
}

pub enum Font2DKind {
    TrueType(TrueTypeFont),
    Composite(CompositeFont),
}

pub struct Font2D {
    handle: OnceCell<Rc<Font2DHandle>>,
    pub family_name: String,
    pub full_name: String,
    pub style: Cell<i32>,
    font_rank: Cell<i32>,
    mapper: RefCell<Option<Rc<CharToGlyphMapper>>>,
    strike_cache: RefCell<Vec<(FontStrikeDesc, Rc<FontStrike>)>>,
    pub kind: Font2DKind,
}

impl Font2D {
    fn wrap(family_name: String, full_name: String, style: i32, rank: i32, kind: Font2DKind) -> Rc<Font2D> {
        let f = Rc::new(Font2D { handle: OnceCell::new(), family_name, full_name, style: Cell::new(style), font_rank: Cell::new(rank), mapper: RefCell::new(None), strike_cache: RefCell::new(Vec::new()), kind });
        let _ = f.handle.set(Rc::new(Font2DHandle { font2d: RefCell::new(f.clone()) }));
        f
    }

    pub fn new_true_type(ttf: TrueTypeFont, rank: i32) -> Rc<Font2D> {
        let (family, full, style) = (ttf.family_name.clone().unwrap_or_default(), ttf.full_name.clone().unwrap_or_default(), ttf.style);
        Font2D::wrap(family, full, style, rank, Font2DKind::TrueType(ttf))
    }

    pub fn new_composite(family_name: String, full_name: String, style: i32, rank: i32, comp: CompositeFont) -> Rc<Font2D> {
        Font2D::wrap(family_name, full_name, style, rank, Font2DKind::Composite(comp))
    }

    pub fn handle(&self) -> Rc<Font2DHandle> {
        self.handle.get().unwrap().clone()
    }

    pub fn ttf(&self) -> Option<&TrueTypeFont> {
        match &self.kind {
            Font2DKind::TrueType(t) => Some(t),
            _ => None,
        }
    }

    pub fn composite(&self) -> Option<&CompositeFont> {
        match &self.kind {
            Font2DKind::Composite(c) => Some(c),
            _ => None,
        }
    }

    pub fn is_physical(&self) -> bool {
        self.ttf().is_some()
    }

    pub fn plat_name(&self) -> Option<&str> {
        self.ttf().map(|t| t.plat_name.as_str())
    }

    pub fn get_style(&self) -> i32 {
        self.style.get()
    }

    pub fn get_rank(&self) -> i32 {
        self.font_rank.get()
    }

    pub fn set_rank(&self, rank: i32) {
        self.font_rank.set(rank);
    }

    pub fn get_width(&self) -> i32 {
        match self.ttf() {
            Some(t) if t.font_width > 0 => t.font_width,
            _ => FWIDTH_NORMAL,
        }
    }

    pub fn get_weight(&self) -> i32 {
        if let Some(t) = self.ttf() {
            if t.font_weight > 0 {
                return t.font_weight;
            }
        }
        if (self.style.get() & FONT_BOLD) != 0 {
            FWEIGHT_BOLD
        } else {
            FWEIGHT_NORMAL
        }
    }

    pub fn get_mapper(self: &Rc<Self>) -> Rc<CharToGlyphMapper> {
        if let Some(m) = self.mapper.borrow().as_ref() {
            return m.clone();
        }
        let m = Rc::new(match &self.kind {
            Font2DKind::TrueType(_) => CharToGlyphMapper::TrueType(TrueTypeGlyphMapper::new(self)),
            Font2DKind::Composite(_) => CharToGlyphMapper::Composite(CompositeGlyphMapper::new(self)),
        });
        let mut slot = self.mapper.borrow_mut();
        if let Some(existing) = slot.as_ref() {
            return existing.clone();
        }
        *slot = Some(m.clone());
        m
    }

    pub fn get_validated_glyph_code(self: &Rc<Self>, glyph_code: i32) -> i32 {
        match &self.kind {
            Font2DKind::Composite(c) => c.get_validated_glyph_code(self, glyph_code),
            _ => {
                let m = self.get_mapper();
                if glyph_code < 0 || glyph_code >= m.get_num_glyphs() {
                    m.get_missing_glyph_code()
                } else {
                    glyph_code
                }
            }
        }
    }

    pub fn get_strike(self: &Rc<Self>, desc: &FontStrikeDesc) -> Rc<FontStrike> {
        if let Some((_, s)) = self.strike_cache.borrow().iter().find(|(d, _)| d == desc) {
            return s.clone();
        }
        let s = Rc::new(match &self.kind {
            Font2DKind::TrueType(_) => FontStrike::File(FileFontStrike::new(self.clone(), desc.clone())),
            Font2DKind::Composite(_) => FontStrike::Composite(CompositeStrike::new(self.clone(), desc.clone())),
        });
        self.strike_cache.borrow_mut().push((desc.clone(), s.clone()));
        s
    }

    pub fn can_do_style(&self, style: i32) -> bool {
        match &self.kind {
            Font2DKind::TrueType(_) => true,
            Font2DKind::Composite(_) => style == self.style.get(),
        }
    }

    pub fn get_postscript_name(&self) -> String {
        match &self.kind {
            Font2DKind::TrueType(t) => t.get_postscript_name(),
            _ => self.full_name.clone(),
        }
    }

    pub fn get_font_name(&self, l: Option<&Locale>) -> String {
        match &self.kind {
            Font2DKind::TrueType(t) => t.get_font_name(l),
            _ => self.full_name.clone(),
        }
    }

    pub fn get_family_name(&self, l: Option<&Locale>) -> String {
        match &self.kind {
            Font2DKind::TrueType(t) => t.get_family_name(l),
            _ => self.family_name.clone(),
        }
    }

    pub fn get_num_glyphs(self: &Rc<Self>) -> i32 {
        match &self.kind {
            Font2DKind::Composite(c) => {
                if c.num_glyphs.get() == 0 {
                    c.num_glyphs.set(self.get_mapper().get_num_glyphs());
                }
                c.num_glyphs.get()
            }
            _ => self.get_mapper().get_num_glyphs(),
        }
    }

    pub fn get_missing_glyph_code(self: &Rc<Self>) -> i32 {
        self.get_mapper().get_missing_glyph_code()
    }

    pub fn can_display(self: &Rc<Self>, c: u16) -> bool {
        self.get_mapper().can_display(c)
    }

    pub fn supports_encoding(&self, encoding: Option<&str>) -> bool {
        match &self.kind {
            Font2DKind::TrueType(t) => t.supports_encoding(encoding),
            _ => false,
        }
    }

    pub fn use_aa_for_pt_size(self: &Rc<Self>, ptsize: i32) -> bool {
        match &self.kind {
            Font2DKind::TrueType(t) => t.use_aa_for_pt_size(ptsize, self.style.get()),
            Font2DKind::Composite(c) => c.use_aa_for_pt_size(self, ptsize),
        }
    }
}
