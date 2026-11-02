//! Port of afe.jar `com.adobe.fontengine.font.opentype.OTFSWFFont3Description`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::cmap::Cmap;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

/// `new InvalidFontException("Missing required table", npe)`.
pub fn missing_required_table() -> FontEngineException {
    FontEngineException::invalid_font("Missing required table")
}

/// An uncaught `NullPointerException`.
pub fn npe() -> FontEngineException {
    FontEngineException::new(ExceptionKind::FontEngine, "java.lang.NullPointerException")
}

/// Java's `catch (NullPointerException)` around a call.
pub fn catch_npe<T>(r: FontResult<T>) -> FontResult<T> {
    match r {
        Err(e) if e.kind == ExceptionKind::FontEngine && e.get_message() == Some("java.lang.NullPointerException") => Err(missing_required_table()),
        r => r,
    }
}

pub struct OTFSWFFont3Description {
    pub the_font: Rc<OpenTypeFont>,
    pub was_embedded: bool,
}

impl OTFSWFFont3Description {
    pub fn new(the_font: Rc<OpenTypeFont>, was_embedded: bool) -> OTFSWFFont3Description {
        OTFSWFFont3Description { the_font, was_embedded }
    }

    fn cmap(&self) -> FontResult<&Cmap> {
        self.the_font.cmap.as_deref().ok_or_else(missing_required_table)
    }
}

impl SWFFontDescription for OTFSWFFont3Description {
    fn can_display(&self, c: u16) -> FontResult<bool> {
        Ok(self.cmap()?.unicode_char2glyph(c as i32)? != 0)
    }

    fn get_ascent(&self) -> FontResult<f64> {
        if let Some(os2) = &self.the_font.os2 {
            return Ok(os2.get_win_ascent()? as f64);
        }
        if let Some(hhea) = &self.the_font.hhea {
            return Ok(hhea.get_ascender()? as f64);
        }
        Ok(0.0)
    }

    fn get_descent(&self) -> FontResult<f64> {
        if let Some(os2) = &self.the_font.os2 {
            return Ok(os2.get_win_descent()? as f64);
        }
        if let Some(hhea) = &self.the_font.hhea {
            return Ok(hhea.get_descender()?.wrapping_neg() as f64);
        }
        Ok(0.0)
    }

    fn get_line_gap(&self) -> FontResult<f64> {
        if let Some(os2) = &self.the_font.os2 {
            return Ok(os2.get_typo_line_gap()? as f64);
        }
        if let Some(hhea) = &self.the_font.hhea {
            return Ok(hhea.get_line_gap()? as f64);
        }
        Ok(0.0)
    }

    fn get_em_scale(&self) -> FontResult<f64> {
        match &self.the_font.head {
            Some(head) => Ok(head.get_units_per_em()? as f64),
            None => Err(FontEngineException::invalid_font("Cannot determine em")),
        }
    }

    fn get_family(&self) -> FontResult<Option<String>> {
        self.the_font.name.as_ref().ok_or_else(npe)?.get_first_name(1)
    }

    fn get_first_char(&self) -> FontResult<i32> {
        if let Some(os2) = &self.the_font.os2 {
            let n = os2.get_first_char()?;
            return Ok(self.cmap()?.remove_symbol_modifier(n));
        }
        Ok(0)
    }

    fn get_last_char(&self) -> FontResult<i32> {
        let cmap = self.cmap()?;
        if cmap.symbol_subtable_offset != -1 {
            return cmap.get_highest_mapped_code(cmap.symbol_subtable_offset, true);
        }
        if cmap.unicode_subtable_offset != -1 {
            return cmap.get_highest_mapped_code(cmap.unicode_subtable_offset, true);
        }
        Ok(65535)
    }

    fn get_horizontal_advance(&self, c: u16) -> FontResult<f64> {
        let hmtx = self.the_font.hmtx.as_ref().ok_or_else(missing_required_table)?;
        let g = self.cmap()?.unicode_char2glyph(c as i32)?;
        Ok(hmtx.get_horizontal_advance(g)? as f64)
    }

    fn get_num_glyphs(&self) -> FontResult<i32> {
        self.the_font.get_num_glyphs()
    }

    fn get_postscript_name(&self) -> FontResult<Option<String>> {
        match &self.the_font.name {
            None => Ok(None),
            Some(n) => n.get_first_name(6),
        }
    }

    fn get_copyright(&self) -> FontResult<Option<String>> {
        match &self.the_font.name {
            None => Ok(None),
            Some(n) => n.get_first_name(0),
        }
    }

    fn get_trademark(&self) -> FontResult<Option<String>> {
        match &self.the_font.name {
            None => Ok(None),
            Some(n) => n.get_first_name(7),
        }
    }

    fn get_full_name(&self) -> FontResult<Option<String>> {
        match &self.the_font.name {
            None => Ok(None),
            Some(n) => match n.get_name(1, 0, 0, 4) {
                Err(e) if e.kind == ExceptionKind::Io => Ok(None),
                r => r,
            },
        }
    }

    fn is_bold(&self) -> FontResult<bool> {
        Ok(match &self.the_font.os2 {
            Some(os2) => os2.get_selection()? & 0x20 != 0,
            None => false,
        })
    }

    fn is_italic(&self) -> FontResult<bool> {
        Ok(match &self.the_font.os2 {
            Some(os2) => os2.get_selection()? & 1 != 0,
            None => false,
        })
    }

    fn get_permissions(&self) -> FontResult<Permission> {
        let p = self.the_font.get_embedding_permission(self.was_embedded)?;
        if p == Permission::IllegalValue {
            return Ok(Permission::Editable);
        }
        Ok(p)
    }

    fn get_outline(&self, c: u16, consumer: &mut dyn OutlineConsumer) -> FontResult<()> {
        catch_npe((|| {
            let g = self.cmap()?.unicode_char2glyph(c as i32)?;
            self.the_font.get_glyph_outline(g, consumer)
        })())
    }

    fn get_sub_family(&self) -> FontResult<Option<String>> {
        match &self.the_font.name {
            None => Ok(None),
            Some(n) => n.get_first_name(2),
        }
    }
}
