//! Port of afe.jar `com.adobe.fontengine.font.opentype.OTFSWFFont4Description`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub_harvester::GsubHarvester;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_subset::OTSubset;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::otf_swf_font3_description::{missing_required_table, OTFSWFFont3Description};
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font4_description::SWFFont4Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct OTFSWFFont4Description {
    define_font3_desc: OTFSWFFont3Description,
    the_font: Rc<OpenTypeFont>,
}

/// What `streamFontData(iterator, out)` decided before streaming, for inspection.
#[derive(Clone, Debug, Default)]
pub struct SWFSubsetTrace {
    pub code_points: Vec<i32>,
    pub variation_sequences: bool,
    pub gids: Vec<i32>,
}

impl OTFSWFFont4Description {
    pub fn new(the_font: Rc<OpenTypeFont>, was_embedded: bool) -> OTFSWFFont4Description {
        OTFSWFFont4Description { define_font3_desc: OTFSWFFont3Description::new(the_font.clone(), was_embedded), the_font }
    }

    /// `streamFontData(iterator, out)`, also returning the mapped code points and the subset's gids.
    pub fn stream_font_data_traced(&self, it: &mut dyn Iterator<Item = i32>, out: &mut dyn Write) -> FontResult<SWFSubsetTrace> {
        let font = &*self.the_font;
        let mut subset = OTSubset::new(font, true)?;
        let mut tree_set = BTreeSet::new();
        let mut bl = false;
        if let Some(cmap) = &font.cmap {
            let mut list: Vec<i32> = it.collect();
            list.sort();
            bl = cmap.gather_possible_mappings(&mut list.into_iter(), &mut tree_set, &mut subset)?;
        }
        let mut lookups: Option<BTreeMap<i32, Vec<i32>>> = None;
        if let Some(gsub) = &font.gsub {
            let mut h = GsubHarvester::new(gsub, font.get_num_glyphs()?);
            lookups = Some(h.gather_possible_lookups(&mut subset)?);
        }
        font.subset_and_stream_for_swf_embedding(&mut subset, out, lookups.as_ref(), &tree_set, bl)?;
        Ok(SWFSubsetTrace { code_points: tree_set.into_iter().collect(), variation_sequences: bl, gids: subset.gids() })
    }
}

impl SWFFont4Description for OTFSWFFont4Description {
    fn can_display(&self, n: i32) -> FontResult<bool> {
        let cmap = self.the_font.cmap.as_ref().ok_or_else(missing_required_table)?;
        Ok(cmap.unicode_char2glyph(n)? != 0)
    }

    fn get_first_char(&self) -> FontResult<i32> {
        self.define_font3_desc.get_first_char()
    }

    fn get_last_char(&self) -> FontResult<i32> {
        let cmap = self.the_font.cmap.as_ref().ok_or_else(missing_required_table)?;
        if cmap.symbol_subtable_offset != -1 {
            return cmap.get_highest_mapped_code(cmap.symbol_subtable_offset, false);
        }
        if cmap.unicode_subtable_offset != -1 {
            return cmap.get_highest_mapped_code(cmap.unicode_subtable_offset, false);
        }
        Ok(65535)
    }

    fn get_permissions(&self) -> FontResult<Permission> {
        self.define_font3_desc.get_permissions()
    }

    fn get_family(&self) -> FontResult<Option<String>> {
        self.define_font3_desc.get_family()
    }

    fn get_sub_family(&self) -> FontResult<Option<String>> {
        self.define_font3_desc.get_sub_family()
    }

    fn is_bold(&self) -> FontResult<bool> {
        self.define_font3_desc.is_bold()
    }

    fn is_italic(&self) -> FontResult<bool> {
        self.define_font3_desc.is_italic()
    }

    fn stream_font_data(&self, it: &mut dyn Iterator<Item = i32>, out: &mut dyn Write) -> FontResult<()> {
        self.stream_font_data_traced(it, out)?;
        Ok(())
    }

    fn stream_font_data_all(&self, out: &mut dyn Write) -> FontResult<()> {
        self.the_font.stream_sfnt_for_swf_editting(out)
    }
}
