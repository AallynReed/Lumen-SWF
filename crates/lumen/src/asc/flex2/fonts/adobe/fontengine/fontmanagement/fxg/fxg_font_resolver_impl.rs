//! Port of afe.jar `com.adobe.fontengine.fontmanagement.fxg.FXGFontResolverImpl`.

use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_resolution_priority::FontResolutionPriority;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_description::FXGFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_resolver::FXGFontResolver;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::fxg::fxg_font_search_attributes::FXGFontSearchAttributes;

/// `FXGKey`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FXGKey {
    family_name: String,
    is_bold: bool,
    is_italic: bool,
}

/// `FXGValue`.
#[derive(Clone)]
struct FXGValue {
    fxg_description: FXGFontDescription,
    font: Rc<dyn Font>,
}

#[derive(Clone)]
pub struct FXGFontResolverImpl {
    fxg_fonts: HashMap<FXGKey, FXGValue>,
    resolution_priority: FontResolutionPriority,
}

impl Default for FXGFontResolverImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl FXGFontResolverImpl {
    pub fn new() -> FXGFontResolverImpl {
        FXGFontResolverImpl { fxg_fonts: HashMap::new(), resolution_priority: FontResolutionPriority::First }
    }
}

impl FXGFontResolver for FXGFontResolverImpl {
    fn set_resolution_priority(&mut self, p: FontResolutionPriority) -> FontResolutionPriority {
        std::mem::replace(&mut self.resolution_priority, p)
    }

    fn add_font(&mut self, font: Rc<dyn Font>) -> FontResult<()> {
        let a = font.get_fxg_font_description()?;
        for d in a {
            self.add_font_with(d, font.clone())?;
        }
        Ok(())
    }

    /// `IntelligentResolver` is not ported: the INTELLIGENT priorities replace like LAST.
    fn add_font_with(&mut self, d: FXGFontDescription, font: Rc<dyn Font>) -> FontResult<()> {
        let key = FXGKey { family_name: d.get_family_name().to_string(), is_bold: d.is_bold(), is_italic: d.is_italic() };
        if self.fxg_fonts.contains_key(&key) && self.resolution_priority == FontResolutionPriority::First {
            return Ok(());
        }
        self.fxg_fonts.insert(key, FXGValue { fxg_description: d, font });
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.fxg_fonts.is_empty()
    }

    fn find_font(&self, a: &FXGFontSearchAttributes) -> Option<Rc<dyn Font>> {
        let key = FXGKey { family_name: a.get_family_name().to_string(), is_bold: a.is_bold(), is_italic: a.is_italic() };
        self.fxg_fonts.get(&key).map(|v| v.font.clone())
    }
}
