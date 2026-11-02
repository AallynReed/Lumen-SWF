//! Ports of swfutils `flash.swf.builder.tags.FontBuilder` and `flash.swf.builder.types.ZoneRecordBuilder`, with
//! flex-fontkit `flash.fonts.flashtype.FlashTypeZoneRecordBuilder.build` (the zone records come from
//! `fonts::adobe::flashtype`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::asc::flex2::flash::swf::tags::{codes, Body, DefineFont2, DefineFontAlignZones, DefineFontName, Tag, TagRef};
use crate::asc::flex2::flash::swf::types::{Rect, ShapeRecord, ZoneRecord};
use crate::asc::flex2::fonts::adobe::flashtype::adf_algn_zones::AlgnZoneState;
use crate::asc::flex2::fonts::adobe::flashtype::flash_type_zone_record_builder::{create_zone_record, get_font_outline, initialize_align_zone_state, ShapeRecordIn, ZONE_STATE};
use crate::asc::flex2::fonts::font_face::FaceRef;
use crate::asc::flex2::fonts::font_manager::{FmResult, JavaException};
use crate::asc::flex2::fonts::fs_type::FSType;
use crate::asc::flex2::fonts::glyph_entry::GlyphEntryRef;

pub struct FontBuilder {
    pub code: i32,
    pub tag: DefineFont2,
    flash_type: bool,
    glyph_entry_map: BTreeMap<u16, GlyphEntryRef>,
    default_face: FaceRef,
    font_height: f64,
    /// The `ZoneRecordBuilder`, here only its `fontAlias`.
    zone_record_builder: Option<String>,
}

const USE_LICENSE_TAG: bool = true;

impl FontBuilder {
    /// `new FontBuilder(code, fontFace, alias, flashType)`.
    pub fn new(code: i32, font_face: FaceRef, alias: String, flash_type: bool) -> FmResult<FontBuilder> {
        if code != codes::DEFINE_FONT2 && code != codes::DEFINE_FONT3 {
            return Err(JavaException::new("flash.swf.builder.tags.FontBuilder$SWFFontNotSupportedException", Some(format!("Cannot build DefineFont for SWF tag code {code}"))));
        }
        let tag = DefineFont2 { has_layout: true, ..Default::default() };
        let mut b = FontBuilder { code, tag, flash_type, glyph_entry_map: BTreeMap::new(), default_face: font_face, font_height: 0.0, zone_record_builder: None };
        b.init(alias)?;
        Ok(b)
    }

    fn init(&mut self, alias: String) -> FmResult<()> {
        let face = self.default_face.clone();
        self.font_height = face.get_point_size();
        self.tag.font_name = Some(alias.clone());
        self.tag.bold = face.is_bold();
        self.tag.italic = face.is_italic();
        if self.tag.has_layout {
            self.tag.ascent = face.get_ascent();
            self.tag.descent = face.get_descent();
            self.tag.leading = face.get_line_gap();
        }
        if self.flash_type {
            for c in ['z', 'Z', 'l', 'L'] {
                if face.get_glyph_entry(c as u16)?.is_none() {
                    self.flash_type = false;
                }
            }
        }
        if self.flash_type {
            self.zone_record_builder = Some(alias);
        }
        self.add_char_default(' ' as u16)
    }

    /// `build()`: the DefineFont2/3, with its DefineFontAlignZones and DefineFontName.
    pub fn build(mut self) -> FmResult<TagRef> {
        let count = self.glyph_entry_map.len();
        let df3 = self.code == codes::DEFINE_FONT3;
        let mut zone_table = Vec::new();
        self.tag.glyph_shape_table = Vec::with_capacity(count);
        self.tag.code_table = Vec::with_capacity(count);
        self.tag.advance_table = Vec::with_capacity(count);
        self.tag.bounds_table = Vec::with_capacity(count);
        for (i, ge) in self.glyph_entry_map.values().enumerate() {
            let mut ge = ge.borrow_mut();
            if self.flash_type && df3 {
                zone_table.push(ge.zone_record.clone().unwrap_or_else(|| Rc::new(ZoneRecord::default())));
            }
            self.tag.glyph_shape_table.push(ge.shape.clone().unwrap_or_default());
            ge.set_index(i as i32);
            self.tag.code_table.push(ge.character);
            if self.tag.has_layout {
                self.tag.advance_table.push(ge.advance as i16);
                self.tag.bounds_table.push(Rect::default());
            }
        }
        if self.tag.has_layout {
            self.tag.kerning_table = Vec::new();
        }
        let name = self.get_name()?;
        let copyright = self.get_copyright();
        let fs = self.get_fs_type();
        let tag = Tag::new_ref(self.code, Body::DefineFont2(self.tag));
        if self.flash_type && df3 {
            let zones = Tag::new_ref(codes::DEFINE_FONT_ALIGN_ZONES, Body::DefineFontAlignZones(DefineFontAlignZones { font: Some(Rc::downgrade(&tag)), csm_table_hint: 1, zone_table }));
            if let Body::DefineFont2(f) = &mut tag.borrow_mut().body {
                f.zones = Some(zones);
            }
        }
        if USE_LICENSE_TAG && (fs.as_ref().is_some_and(|f| !f.installable) || copyright.is_some() || Some(&name).is_some()) {
            let license = Tag::new_ref(codes::DEFINE_FONT_NAME, Body::DefineFontName(DefineFontName { font: Some(Rc::downgrade(&tag)), font_name: Some(name), copyright }));
            if let Body::DefineFont2(f) = &mut tag.borrow_mut().body {
                f.license = Some(license);
            }
        }
        Ok(tag)
    }

    pub fn add_all_chars(&mut self) -> FmResult<()> {
        let face = self.default_face.clone();
        let min = face.get_first_char()?;
        let count = face.get_num_glyphs()?;
        self.add_charset(min, count)
    }

    pub fn add_charset(&mut self, from_char: i32, count: i32) -> FmResult<()> {
        let face = self.default_face.clone();
        let mut remaining = count;
        let mut i = from_char;
        while remaining > 0 && i < 0xFFFF {
            if self.add_char(&face, i as u16)?.is_some() {
                remaining -= 1;
            }
            i += 1;
        }
        Ok(())
    }

    pub fn add_char_default(&mut self, c: u16) -> FmResult<()> {
        let face = self.default_face.clone();
        self.add_char(&face, c).map(|_| ())
    }

    pub fn add_char(&mut self, face: &FaceRef, c: u16) -> FmResult<Option<GlyphEntryRef>> {
        let mut ge = self.glyph_entry_map.get(&c).cloned();
        if ge.is_none() {
            ge = face.get_glyph_entry(c)?;
            if let Some(g) = &ge {
                self.glyph_entry_map.insert(c, g.clone());
            }
        }
        if self.flash_type {
            if let Some(g) = &ge {
                if g.borrow().zone_record.is_none() && self.zone_record_builder.is_some() {
                    let z = self.zone_record_build(c)?;
                    g.borrow_mut().zone_record = Some(z);
                }
            }
        }
        Ok(ge)
    }

    pub fn get_copyright(&self) -> Option<String> {
        self.default_face.get_copyright()
    }

    pub fn get_name(&self) -> FmResult<String> {
        self.default_face.get_family()
    }

    pub fn get_fs_type(&self) -> Option<FSType> {
        self.default_face.get_fs_type()
    }

    pub fn get_glyph(&self, c: u16) -> Option<GlyphEntryRef> {
        self.glyph_entry_map.get(&c).cloned()
    }

    pub fn get_font_height(&self) -> f64 {
        self.font_height
    }

    pub fn size(&self) -> usize {
        self.glyph_entry_map.len()
    }

    /// `FlashTypeZoneRecordBuilder.getFontOutline`'s glyph lookup: this builder first, then the face.
    fn outline_records(&self, c: u16) -> FmResult<Option<Vec<ShapeRecordIn>>> {
        let ge = match self.get_glyph(c) {
            Some(g) => Some(g),
            None => self.default_face.get_glyph_entry(c)?,
        };
        Ok(ge.and_then(|g| g.borrow().shape.as_ref().map(|s| shape_records_in(s.shape_records.as_deref().unwrap_or(&[])))))
    }

    /// `FlashTypeZoneRecordBuilder.build(c)`.
    fn zone_record_build(&self, c: u16) -> FmResult<Rc<ZoneRecord>> {
        let face = self.default_face.clone();
        let alias = self.zone_record_builder.clone().unwrap_or_default();
        let state = match face.get_property(ZONE_STATE).and_then(|p| p.downcast::<RefCell<AlgnZoneState>>().ok()) {
            Some(s) => s,
            None => {
                let mut outline = |ch: u16| -> crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::ADFPath {
                    match self.outline_records(ch) {
                        Ok(Some(r)) => get_font_outline(&r, ch),
                        _ => panic!("FlashType outline for {ch} unavailable"),
                    }
                };
                let s = match initialize_align_zone_state(&mut outline) {
                    Ok(s) => Rc::new(RefCell::new(s)),
                    Err(()) => return Err(JavaException::runtime(format!("Could not initialize FlashType for font {}", face.get_family()?))),
                };
                face.set_property(ZONE_STATE, s.clone());
                s
            }
        };
        let path = if c == ' ' as u16 {
            None
        } else {
            match self.outline_records(c)? {
                Some(r) => Some(get_font_outline(&r, c)),
                None => return Err(JavaException::null_pointer()),
            }
        };
        let r = create_zone_record(&mut state.borrow_mut(), c, path)
            .map_err(|_| JavaException::runtime(format!("Could not create ZoneTable for character {} in font {}", String::from_utf16_lossy(&[c]), alias)))?;
        Ok(Rc::new(ZoneRecord { num_zone_data: r.num_zone_data, zone_data: r.zone_data, zone_mask: r.zone_mask }))
    }
}

fn shape_records_in(records: &[ShapeRecord]) -> Vec<ShapeRecordIn> {
    records
        .iter()
        .map(|r| match r {
            ShapeRecord::StyleChange(s) => ShapeRecordIn::StyleChange { move_delta_x: s.move_delta_x, move_delta_y: s.move_delta_y },
            ShapeRecord::StraightEdge { delta_x, delta_y } => ShapeRecordIn::StraightEdge { delta_x: *delta_x, delta_y: *delta_y },
            ShapeRecord::CurvedEdge { control_delta_x, control_delta_y, anchor_delta_x, anchor_delta_y } => {
                ShapeRecordIn::CurvedEdge { control_delta_x: *control_delta_x, control_delta_y: *control_delta_y, anchor_delta_x: *anchor_delta_x, anchor_delta_y: *anchor_delta_y }
            }
        })
        .collect()
}
