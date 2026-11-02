//! Port of afe.jar `com.adobe.fontengine.fontmanagement.fxg.FXGFontDescription`.

use crate::asc::flex2::fonts::adobe::fontengine::font::font_description::FontDescription;
use crate::asc::flex2::fonts::adobe::fontengine::fontmanagement::platform_type::Platform;

/// The locale is the `ULocale`'s name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FXGFontDescription {
    platform: Platform,
    locale: String,
    family_name: String,
    is_bold: bool,
    is_italic: bool,
}

impl FontDescription for FXGFontDescription {}

impl FXGFontDescription {
    pub fn new(platform: Platform, locale: String, family_name: String, is_bold: bool, is_italic: bool) -> FXGFontDescription {
        FXGFontDescription { platform, locale, family_name, is_bold, is_italic }
    }

    pub fn get_platform(&self) -> Platform {
        self.platform
    }

    pub fn get_locale(&self) -> &str {
        &self.locale
    }

    pub fn get_family_name(&self) -> &str {
        &self.family_name
    }

    pub fn is_bold(&self) -> bool {
        self.is_bold
    }

    pub fn is_italic(&self) -> bool {
        self.is_italic
    }

    pub fn to_java_string(&self) -> String {
        format!(
            "[{}, {}] - {}{}{}",
            self.platform.to_java_string(),
            self.locale,
            self.family_name,
            if self.is_bold { ", bold" } else { "" },
            if self.is_italic { ", italic" } else { "" }
        )
    }
}
