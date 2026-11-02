//! Port of afe.jar `com.adobe.fontengine.fontmanagement.platform.PlatformFontDescription`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_description::FontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform_type::Platform;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformFontDescription {
    platform: Platform,
    locale: String,
    platform_family_name: String,
}

impl FontDescription for PlatformFontDescription {}

impl PlatformFontDescription {
    pub fn new(platform: Platform, locale: String, platform_family_name: String) -> PlatformFontDescription {
        PlatformFontDescription { platform, locale, platform_family_name }
    }

    pub fn get_platform(&self) -> Platform {
        self.platform
    }

    pub fn get_locale(&self) -> &str {
        &self.locale
    }

    pub fn get_platform_name(&self) -> &str {
        &self.platform_family_name
    }

    pub fn to_java_string(&self) -> String {
        format!("[{}, {}] - {}", self.platform.to_java_string(), self.locale, self.platform_family_name)
    }
}
