//! Port of afe.jar `com.adobe.fontengine.fontmanagement.fxg.FXGFontResolver`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_resolution_priority::FontResolutionPriority;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_description::FXGFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_search_attributes::FXGFontSearchAttributes;

pub trait FXGFontResolver {
    fn set_resolution_priority(&mut self, p: FontResolutionPriority) -> FontResolutionPriority;
    fn add_font(&mut self, font: Rc<dyn Font>) -> FontResult<()>;
    fn add_font_with(&mut self, d: FXGFontDescription, font: Rc<dyn Font>) -> FontResult<()>;
    fn is_empty(&self) -> bool;
    fn find_font(&self, a: &FXGFontSearchAttributes) -> Option<Rc<dyn Font>>;
}
