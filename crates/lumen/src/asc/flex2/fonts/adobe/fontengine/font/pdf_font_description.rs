//! Port of afe.jar `com.adobe.fontengine.font.PDFFontDescription` (the members SWF embedding reaches).

use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub trait PDFFontDescription {
    fn get_postscript_name(&self) -> FontResult<Option<String>>;
    fn get_font_family(&self) -> FontResult<Option<String>>;
    fn get_font_bbox(&self) -> FontResult<Option<Rect>>;
    fn get_num_glyphs(&self) -> FontResult<i32>;
    fn get_advance(&self, n: i32) -> FontResult<f64>;
    fn get_base14_name(&self) -> Option<String>;
    fn pdf_font_is_true_type(&self) -> bool;
}
