//! Port of afe.jar `com.adobe.fontengine.font.SWFFontDescription`.

use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub trait SWFFontDescription {
    fn get_outline(&self, c: u16, consumer: &mut dyn OutlineConsumer) -> FontResult<()>;
    fn get_horizontal_advance(&self, c: u16) -> FontResult<f64>;
    fn can_display(&self, c: u16) -> FontResult<bool>;
    fn get_permissions(&self) -> FontResult<Permission>;
    fn get_family(&self) -> FontResult<Option<String>>;
    fn get_sub_family(&self) -> FontResult<Option<String>>;
    fn get_ascent(&self) -> FontResult<f64>;
    fn get_descent(&self) -> FontResult<f64>;
    fn get_line_gap(&self) -> FontResult<f64>;
    fn get_first_char(&self) -> FontResult<i32>;
    fn get_last_char(&self) -> FontResult<i32>;
    fn get_num_glyphs(&self) -> FontResult<i32>;
    fn is_bold(&self) -> FontResult<bool>;
    fn is_italic(&self) -> FontResult<bool>;
    fn get_em_scale(&self) -> FontResult<f64>;
    fn get_copyright(&self) -> FontResult<Option<String>>;
    fn get_trademark(&self) -> FontResult<Option<String>>;
    fn get_postscript_name(&self) -> FontResult<Option<String>>;
    fn get_full_name(&self) -> FontResult<Option<String>>;
}
