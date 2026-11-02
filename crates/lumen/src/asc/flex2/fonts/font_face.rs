//! Port of swfutils `flash.fonts.FontFace`.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::font_manager::FmResult;
use crate::asc::flex2::fonts::fs_type::FSType;
use crate::asc::flex2::fonts::glyph_entry::GlyphEntryRef;

pub const PLAIN: i32 = 0;
pub const BOLD: i32 = 1;
pub const ITALIC: i32 = 2;
pub const SWF_EM_SQUARE: i32 = 1024;
pub const TTF_EM_SQUARE: i32 = 2048;

/// The methods that can throw a Java runtime exception return `FmResult`.
pub trait FontFace {
    fn get_glyph_entry(&self, c: u16) -> FmResult<Option<GlyphEntryRef>>;
    fn get_missing_glyph_code(&self) -> i32;
    fn get_point_size(&self) -> f64;
    fn get_family(&self) -> FmResult<String>;
    fn get_ascent(&self) -> i32;
    fn get_descent(&self) -> i32;
    fn get_line_gap(&self) -> i32;
    fn get_first_char(&self) -> FmResult<i32>;
    fn get_num_glyphs(&self) -> FmResult<i32>;
    fn can_display(&self, c: u16) -> FmResult<bool>;
    fn get_advance(&self, c: u16) -> FmResult<i32>;
    fn is_bold(&self) -> bool;
    fn is_italic(&self) -> bool;
    fn get_em_scale(&self) -> f64;
    fn get_copyright(&self) -> Option<String>;
    fn set_copyright(&self, c: Option<String>);
    fn get_trademark(&self) -> Option<String>;
    fn set_trademark(&self, t: Option<String>);
    fn get_fs_type(&self) -> Option<FSType>;
    fn set_fs_type(&self, t: Option<FSType>);
    fn get_postscript_name(&self) -> FmResult<String>;
    /// The `properties` map behind `setProperty`/`getProperty`.
    fn properties(&self) -> &RefCell<HashMap<String, Rc<dyn Any>>>;

    fn set_property(&self, name: &str, value: Rc<dyn Any>) {
        self.properties().borrow_mut().insert(name.to_string(), value);
    }

    fn get_property(&self, name: &str) -> Option<Rc<dyn Any>> {
        self.properties().borrow().get(name).cloned()
    }
}

pub type FaceRef = Rc<dyn FontFace>;
