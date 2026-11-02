//! Port of afe.jar `com.adobe.fontengine.font.FontData` (the members SWF embedding reaches).

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font4_description::SWFFont4Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_description::FXGFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_description::PlatformFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform_type::Platform;

pub trait FontData {
    fn get_container_fingerprint(&self) -> Vec<u8>;
    fn get_num_glyphs(&self) -> FontResult<i32>;
    fn is_symbolic(&self) -> FontResult<bool> {
        Ok(false)
    }
    fn get_units_per_em_x(&self) -> FontResult<f64>;
    fn get_units_per_em_y(&self) -> FontResult<f64>;
    fn get_font_bbox(&self) -> FontResult<Rect>;
    fn get_glyph_for_char(&self, n: i32) -> FontResult<i32>;
    fn get_horizontal_advance(&self, n: i32) -> FontResult<f64>;
    fn get_glyph_outline(&self, n: i32, consumer: &mut dyn OutlineConsumer) -> FontResult<()>;
    fn get_glyph_bbox(&self, n: i32) -> FontResult<Rect>;
    fn get_swf_font_description(self: Rc<Self>, was_embedded: bool) -> FontResult<Rc<dyn SWFFontDescription>>;
    fn get_swf_font4_description(self: Rc<Self>, was_embedded: bool) -> FontResult<Rc<dyn SWFFont4Description>>;
    fn get_embedding_permission(&self, was_embedded: bool) -> FontResult<Permission>;
    /// The locale is a `ULocale` name.
    fn get_fxg_font_description(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<FXGFontDescription>>;
    fn get_platform_font_description(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<PlatformFontDescription>>;
}
