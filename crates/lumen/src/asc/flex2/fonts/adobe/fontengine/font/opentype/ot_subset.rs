//! Port of afe.jar `com.adobe.fontengine.font.opentype.OTSubset`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub struct OTSubset<'a> {
    pub base: SubsetDefaultImpl,
    pub font: &'a OpenTypeFont,
    pub targetting_cff: bool,
}

impl<'a> OTSubset<'a> {
    pub fn new(font: &'a OpenTypeFont, bl: bool) -> FontResult<OTSubset<'a>> {
        let mut s = OTSubset { base: SubsetDefaultImpl::new(font.get_num_glyphs()?, true), font, targetting_cff: bl };
        if !bl {
            for g in 0..4 {
                s.get_subset_gid(g)?;
            }
        } else {
            s.get_subset_gid(0)?;
        }
        Ok(s)
    }

    fn pull_component_glyphs(&mut self, n: i32) -> FontResult<()> {
        let font = self.font;
        if !self.targetting_cff {
            if let Some(glyf) = &font.glyf {
                glyf.pull_component_glyphs(self, n)?;
            }
        }
        Ok(())
    }

    /// The full gids of the subset, in subset order.
    pub fn gids(&self) -> Vec<i32> {
        (0..self.get_num_glyphs()).map(|i| self.get_full_gid(i)).collect()
    }

    pub fn to_java_string(&self) -> String {
        (0..self.get_num_glyphs()).map(|i| format!("{} -> {}; ", self.get_full_gid(i), i)).collect()
    }
}

impl<'a> Subset for OTSubset<'a> {
    fn get_num_glyphs(&self) -> i32 {
        self.base.get_num_glyphs()
    }

    fn get_subset_gid(&mut self, n: i32) -> FontResult<i32> {
        let (g, added) = self.base.add_gid(n)?;
        if added {
            self.pull_component_glyphs(n)?;
        }
        Ok(g)
    }

    fn get_existing_subset_gid(&self, n: i32) -> FontResult<i32> {
        self.base.get_existing_subset_gid(n)
    }

    fn get_full_gid(&self, n: i32) -> i32 {
        self.base.get_full_gid(n)
    }
}
