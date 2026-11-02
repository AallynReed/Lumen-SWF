//! Port of the option values of `flex2.compiler.common.FontsConfiguration` (`compiler.fonts.*`); the font manager
//! chain it builds lives with the font port (`fonts::font_manager::create`).

#[derive(Clone, Debug)]
pub struct FontsConfiguration {
    /// `compiler.fonts.managers`; `None` unless configured.
    pub managers: Option<Vec<String>>,
    pub max_cached_fonts: Option<String>,
    pub max_glyphs_per_face: Option<String>,
    /// `flash-type` / `advanced-anti-aliasing`.
    pub flash_type: bool,
    /// `languages.language-range`, a `Properties` (a later `setProperty` replaces).
    pub languages: Vec<(String, String)>,
    /// `local-fonts-snapshot`, as the resolved file's name.
    pub local_fonts_snapshot: Option<String>,
    /// `local-font-paths`, resolved; `None` unless configured.
    pub local_font_paths: Option<Vec<String>>,
}

impl Default for FontsConfiguration {
    fn default() -> Self {
        FontsConfiguration { managers: None, max_cached_fonts: None, max_glyphs_per_face: None, flash_type: true, languages: Vec::new(), local_fonts_snapshot: None, local_font_paths: None }
    }
}

impl FontsConfiguration {
    /// The `<fonts>` section of the SDK `flex-config.xml`.
    pub fn flex_config_defaults() -> FontsConfiguration {
        FontsConfiguration {
            managers: Some(["flash.fonts.JREFontManager", "flash.fonts.BatikFontManager", "flash.fonts.AFEFontManager", "flash.fonts.CFFFontManager"].iter().map(|s| s.to_string()).collect()),
            max_cached_fonts: Some("20".into()),
            max_glyphs_per_face: Some("1000".into()),
            ..FontsConfiguration::default()
        }
    }

    /// `Languages.cfgLanguageRange`.
    pub fn set_language_range(&mut self, lang: &str, range: &str) {
        match self.languages.iter_mut().find(|(l, _)| l == lang) {
            Some(e) => e.1 = range.to_string(),
            None => self.languages.push((lang.to_string(), range.to_string())),
        }
    }

    /// `Properties.getProperty(lang)`.
    pub fn get_language_range(&self, lang: &str) -> Option<&str> {
        self.languages.iter().find(|(l, _)| l == lang).map(|(_, r)| r.as_str())
    }
}

/// `CompilerConfiguration.getCompatibilityVersion()` from the configured `(major, minor, revision)`.
pub fn compatibility_version(v: Option<(i32, i32, i32)>) -> i32 {
    let (major, minor, revision) = v.unwrap_or((4, 6, 0));
    let version = (major << 24) + (minor << 16) + revision;
    if version != 0 {
        version
    } else {
        0x04060000
    }
}

/// `CompilerConfiguration.getCompatibilityVersionString()`.
pub fn compatibility_version_string(v: Option<(i32, i32, i32)>) -> Option<String> {
    let (major, minor, revision) = v.unwrap_or((4, 6, 0));
    if major == 0 && minor == 0 && revision == 0 {
        None
    } else {
        Some(format!("{major}.{minor}.{revision}"))
    }
}

pub const VERSION_4_0: i32 = 0x04000000;
