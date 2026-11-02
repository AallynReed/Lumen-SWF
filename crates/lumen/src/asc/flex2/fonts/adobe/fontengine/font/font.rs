//! Port of afe.jar `com.adobe.fontengine.font.Font` (the members SWF embedding reaches).

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font4_description::SWFFont4Description;
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_description::FXGFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_description::PlatformFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform_type::Platform;

pub trait Font {
    fn get_units_per_em_x(&self) -> FontResult<f64>;
    fn get_units_per_em_y(&self) -> FontResult<f64>;
    fn is_symbolic(&self) -> FontResult<bool>;
    fn can_embed_for_print_and_preview(&self) -> FontResult<bool>;
    fn can_embed_for_editting(&self) -> FontResult<bool>;
    fn get_swf_font_description(&self) -> FontResult<Rc<dyn SWFFontDescription>>;
    fn get_swf_font4_description(&self) -> FontResult<Rc<dyn SWFFont4Description>>;
    /// `getFXGFontDescription()`.
    fn get_fxg_font_description(&self) -> FontResult<Vec<FXGFontDescription>>;
    /// `getFXGFontDescription(platform, locale)`; Java filters by object identity, so a locale matches nothing.
    fn get_fxg_font_description_for(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<FXGFontDescription>>;
    fn get_platform_font_description(&self) -> FontResult<Vec<PlatformFontDescription>>;
    fn get_platform_font_description_for(&self, platform: Option<Platform>, locale: Option<&str>) -> FontResult<Vec<PlatformFontDescription>>;
    /// `toString()`.
    fn to_java_string(&self) -> String;
}
