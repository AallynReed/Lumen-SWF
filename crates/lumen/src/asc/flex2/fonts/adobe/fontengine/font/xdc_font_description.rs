//! Port of afe.jar `com.adobe.fontengine.font.XDCFontDescription`.

use crate::asc::flex2::fonts::adobe::fontengine::font::pdf_font_description::PDFFontDescription;

pub trait XDCFontDescription: PDFFontDescription {}
