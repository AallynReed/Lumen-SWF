//! Port of afe.jar `com.adobe.fontengine.font.Subset`.

use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub trait Subset {
    fn get_num_glyphs(&self) -> i32;
    /// Adds `full_gid` to the subset if it is not there yet (and, for some subsets, its components).
    fn get_subset_gid(&mut self, full_gid: i32) -> FontResult<i32>;
    fn get_existing_subset_gid(&self, full_gid: i32) -> FontResult<i32>;
    fn get_full_gid(&self, subset_gid: i32) -> i32;
}
