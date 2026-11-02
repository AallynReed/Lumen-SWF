//! Port of JDK 17 `sun.font.CompositeGlyphMapper`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::asc::flex2::fonts::java2d::sun::font::char_to_glyph_mapper::{CharToGlyphMapper, UNINITIALIZED_GLYPH};
use crate::asc::flex2::fonts::java2d::sun::font::font2d::Font2D;

pub const SLOTMASK: u32 = 0xff000000;
pub const GLYPHMASK: i32 = 0x00ffffff;
const NBLOCKS: usize = 216;
const BLOCKSZ: usize = 256;
const MAXUNICODE: i32 = (NBLOCKS * BLOCKSZ) as i32;

pub struct CompositeGlyphMapper {
    font: Weak<Font2D>,
    slot_mappers: RefCell<Vec<Option<Rc<CharToGlyphMapper>>>>,
    glyph_maps: RefCell<Vec<Option<Vec<i32>>>>,
    has_excludes: bool,
    pub missing_glyph: i32,
}

impl CompositeGlyphMapper {
    pub fn new(comp_font: &Rc<Font2D>) -> CompositeGlyphMapper {
        let c = comp_font.composite().unwrap();
        let missing = c.get_slot_font(comp_font, 0).get_missing_glyph_code();
        CompositeGlyphMapper {
            font: Rc::downgrade(comp_font),
            slot_mappers: RefCell::new(vec![None; c.num_slots]),
            glyph_maps: RefCell::new(vec![None; NBLOCKS]),
            has_excludes: c.exclusion_ranges.is_some() && c.max_indices.is_some(),
            missing_glyph: composite_glyph_code(0, missing),
        }
    }

    fn get_cached_glyph_code(&self, unicode: i32) -> i32 {
        if unicode >= MAXUNICODE {
            return UNINITIALIZED_GLYPH;
        }
        match &self.glyph_maps.borrow()[(unicode >> 8) as usize] {
            None => UNINITIALIZED_GLYPH,
            Some(g) => g[(unicode & 0xff) as usize],
        }
    }

    fn set_cached_glyph_code(&self, unicode: i32, glyph_code: i32) {
        if unicode >= MAXUNICODE {
            return;
        }
        let mut maps = self.glyph_maps.borrow_mut();
        let m = maps[(unicode >> 8) as usize].get_or_insert_with(|| vec![UNINITIALIZED_GLYPH; BLOCKSZ]);
        m[(unicode & 0xff) as usize] = glyph_code;
    }

    fn get_slot_mapper(&self, font: &Rc<Font2D>, slot: usize) -> Rc<CharToGlyphMapper> {
        if let Some(m) = &self.slot_mappers.borrow()[slot] {
            return m.clone();
        }
        let m = font.composite().unwrap().get_slot_font(font, slot).get_mapper();
        self.slot_mappers.borrow_mut()[slot] = Some(m.clone());
        m
    }

    fn convert_to_glyph(&self, unicode: i32) -> i32 {
        let font = self.font.upgrade().unwrap();
        let c = font.composite().unwrap();
        for slot in 0..c.num_slots {
            if !self.has_excludes || !c.is_excluded_char(slot, unicode) {
                let mapper = self.get_slot_mapper(&font, slot);
                let mut glyph_code = mapper.char_to_glyph(unicode);
                if glyph_code != mapper.get_missing_glyph_code() {
                    glyph_code = composite_glyph_code(slot as i32, glyph_code);
                    self.set_cached_glyph_code(unicode, glyph_code);
                    return glyph_code;
                }
            }
        }
        self.missing_glyph
    }

    pub fn get_num_glyphs(&self) -> i32 {
        let font = self.font.upgrade().unwrap();
        self.get_slot_mapper(&font, 0).get_num_glyphs()
    }

    pub fn char_to_glyph(&self, unicode: i32) -> i32 {
        let mut glyph_code = self.get_cached_glyph_code(unicode);
        if glyph_code == UNINITIALIZED_GLYPH {
            glyph_code = self.convert_to_glyph(unicode);
        }
        glyph_code
    }
}

pub fn composite_glyph_code(slot: i32, glyph_code: i32) -> i32 {
    (slot << 24) | (glyph_code & GLYPHMASK)
}
