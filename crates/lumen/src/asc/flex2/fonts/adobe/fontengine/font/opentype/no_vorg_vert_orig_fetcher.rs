//! Port of afe.jar `com.adobe.fontengine.font.opentype.NoVORGVertOrigFetcher`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vertical_origin_fetcher::VerticalOriginFetcher;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

pub struct NoVORGVertOrigFetcher<'a> {
    the_font: &'a OpenTypeFont,
}

impl<'a> NoVORGVertOrigFetcher<'a> {
    pub fn new(the_font: &'a OpenTypeFont) -> NoVORGVertOrigFetcher<'a> {
        NoVORGVertOrigFetcher { the_font }
    }
}

impl<'a> VerticalOriginFetcher for NoVORGVertOrigFetcher<'a> {
    fn get_v_origin(&self, n: i32) -> FontResult<i32> {
        let vmtx = self.the_font.vmtx.as_ref().ok_or_else(|| FontEngineException::new(ExceptionKind::FontEngine, "java.lang.NullPointerException"))?;
        let n2 = vmtx.get_top_side_bearing(n)?;
        let r = self.the_font.get_glyph_bbox(n)?;
        Ok((r.ymax as i32).wrapping_add(n2))
    }
}
