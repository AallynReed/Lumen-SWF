//! Port of JDK 17 `sun.font.TrueTypeGlyphMapper`.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::asc::flex2::fonts::java2d::sun::font::c_map::{CMap, CMapError};
use crate::asc::flex2::fonts::java2d::sun::font::font2d::Font2D;
use crate::asc::flex2::fonts::java2d::sun::font::sun_font_manager::SunFontManager;
use crate::asc::flex2::fonts::java2d::sun::font::true_type_font::{CMAP_TAG, MAXP_TAG};

pub struct TrueTypeGlyphMapper {
    font: Weak<Font2D>,
    cmap: RefCell<CMap>,
    num_glyphs: Cell<i32>,
    pub missing_glyph: i32,
    /// Set when the cmap needed a converter map (platform 3 encodings 2..6), which is not ported.
    pub unsupported_cmap: bool,
}

impl TrueTypeGlyphMapper {
    pub fn new(font: &Rc<Font2D>) -> TrueTypeGlyphMapper {
        let ttf = font.ttf().unwrap();
        let mut unsupported_cmap = false;
        let mut bad = false;
        let cmap = match CMap::initialize(ttf.get_table_buffer(CMAP_TAG)) {
            Ok(c) => c,
            Err(e) => {
                unsupported_cmap = e == CMapError::Unsupported;
                bad = true;
                CMap::Null
            }
        };
        let m = TrueTypeGlyphMapper { font: Rc::downgrade(font), cmap: RefCell::new(cmap), num_glyphs: Cell::new(0), missing_glyph: 0, unsupported_cmap };
        if bad {
            m.handle_bad_cmap();
        }
        match ttf.get_table_buffer(MAXP_TAG) {
            Some(b) if b.capacity() >= 6 => m.num_glyphs.set(b.get_char_at(4).unwrap_or(0) as i32),
            _ => m.handle_bad_cmap(),
        }
        m
    }

    pub fn get_num_glyphs(&self) -> i32 {
        self.num_glyphs.get()
    }

    fn get_glyph_from_cmap(&self, char_code: i32) -> u16 {
        let r = self.cmap.borrow().get_glyph(char_code);
        match r {
            Ok(glyph_code) => {
                if (glyph_code as i32) < self.num_glyphs.get() || glyph_code as i32 >= 0xfffe {
                    glyph_code
                } else {
                    self.missing_glyph as u16
                }
            }
            Err(()) => {
                self.handle_bad_cmap();
                self.missing_glyph as u16
            }
        }
    }

    fn handle_bad_cmap(&self) {
        if let Some(font) = self.font.upgrade() {
            SunFontManager::get_instance().de_register_bad_font(&font);
        }
        *self.cmap.borrow_mut() = CMap::Null;
    }

    pub fn char_to_glyph(&self, unicode: i32) -> i32 {
        self.get_glyph_from_cmap(unicode) as i32
    }
}
