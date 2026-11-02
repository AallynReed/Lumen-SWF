//! Port of afe.jar `com.adobe.fontengine.fontmanagement.platform.PlatformFontResolverImpl`.

use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font::Font;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::font_resolution_priority::FontResolutionPriority;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_description::PlatformFontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_resolver::PlatformFontResolver;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform::platform_font_search_attributes::PlatformFontSearchAttributes;
use crate::asc::flex2::fonts::adobe::fontengine::inlineformatting::css20::family_name_normalizer::FamilyNameNormalizer;
use crate::asc::flex2::fonts::adobe::fontengine::inlineformatting::css20::pass_through_family_name_normalizer::PassThroughFamilyNameNormalizer;

#[derive(Clone)]
pub struct PlatformFontResolverImpl {
    platform_fonts: HashMap<String, Rc<dyn Font>>,
    resolution_priority: FontResolutionPriority,
    normalizer: Rc<dyn FamilyNameNormalizer>,
}

impl Default for PlatformFontResolverImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformFontResolverImpl {
    pub fn new() -> PlatformFontResolverImpl {
        Self::with_normalizer(Some(Rc::new(PassThroughFamilyNameNormalizer)))
    }

    pub fn with_normalizer(normalizer: Option<Rc<dyn FamilyNameNormalizer>>) -> PlatformFontResolverImpl {
        PlatformFontResolverImpl {
            platform_fonts: HashMap::new(),
            resolution_priority: FontResolutionPriority::First,
            normalizer: normalizer.unwrap_or_else(|| Rc::new(PassThroughFamilyNameNormalizer)),
        }
    }
}

impl PlatformFontResolver for PlatformFontResolverImpl {
    fn set_resolution_priority(&mut self, p: FontResolutionPriority) -> FontResolutionPriority {
        std::mem::replace(&mut self.resolution_priority, p)
    }

    fn add_font(&mut self, font: Rc<dyn Font>) -> FontResult<()> {
        let a = font.get_platform_font_description()?;
        for d in &a {
            self.add_font_with(d, font.clone())?;
        }
        Ok(())
    }

    /// Java looks the raw name up but stores the normalized one; `IntelligentResolver` is not ported.
    fn add_font_with(&mut self, d: &PlatformFontDescription, font: Rc<dyn Font>) -> FontResult<()> {
        if self.platform_fonts.contains_key(d.get_platform_name()) && self.resolution_priority == FontResolutionPriority::First {
            return Ok(());
        }
        self.platform_fonts.insert(self.normalizer.normalize(d.get_platform_name()), font);
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.platform_fonts.is_empty()
    }

    fn find_font(&self, a: &PlatformFontSearchAttributes) -> Option<Rc<dyn Font>> {
        self.platform_fonts.get(&self.normalizer.normalize(a.get_platform_name())).cloned()
    }
}
