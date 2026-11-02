//! Port of afe.jar `com.adobe.fontengine.font.opentype.VerticalOriginFetcher`.

use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub trait VerticalOriginFetcher {
    fn get_v_origin(&self, n: i32) -> FontResult<i32>;
}
