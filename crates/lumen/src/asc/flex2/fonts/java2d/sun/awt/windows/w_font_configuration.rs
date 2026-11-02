//! Port of JDK 17 `sun.awt.windows.WFontConfiguration` (the overrides the 2D font code reaches).

use crate::asc::flex2::fonts::java2d::sun::awt::font_configuration::FontConfiguration;

impl FontConfiguration {
    pub fn get_fallback_family_name(&self, font_name: &str, default_fallback: Option<&str>) -> Option<String> {
        if self.use_compatibility_fallbacks {
            if let Some(c) = self.get_compatibility_family_name(font_name) {
                return Some(c);
            }
        }
        default_fallback.map(|s| s.to_string())
    }

    pub fn get_face_name_from_component_font_name(&self, component_font_name: &str) -> String {
        component_font_name.to_string()
    }
}
