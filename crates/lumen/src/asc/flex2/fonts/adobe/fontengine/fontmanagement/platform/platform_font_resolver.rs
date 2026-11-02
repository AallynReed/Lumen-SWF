//! Port of afe.jar `com.adobe.fontengine.fontmanagement.platform.PlatformFontResolver`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_resolution_priority::FontResolutionPriority;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_description::PlatformFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_search_attributes::PlatformFontSearchAttributes;

pub trait PlatformFontResolver {
    fn set_resolution_priority(&mut self, p: FontResolutionPriority) -> FontResolutionPriority;
    fn add_font(&mut self, font: Rc<dyn Font>) -> FontResult<()>;
    fn add_font_with(&mut self, d: &PlatformFontDescription, font: Rc<dyn Font>) -> FontResult<()>;
    fn is_empty(&self) -> bool;
    fn find_font(&self, a: &PlatformFontSearchAttributes) -> Option<Rc<dyn Font>>;
}
