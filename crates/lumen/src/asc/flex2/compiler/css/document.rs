//! What the CSS classes need from the MXML side and from `MxmlConfiguration`, as interfaces the MXML port provides:
//! `MxmlDocument`, `mxml.reflect.TypeTable`/`Type`, and the configuration getters `StylesContainer`, `StyleDef` and
//! `CssCompiler` call.

use std::rc::Rc;

use super::at_embed::AtEmbed;
use super::css_file::CssFile;

/// The `flex2.compiler.mxml.rep.MxmlDocument` calls CSS makes.
pub trait CssDocument {
    /// `getQName().toString()`.
    fn get_qname(&self) -> String;
    fn add_import(&self, name: &str, line: i32);
    fn add_at_embed(&self, at_embed: Rc<AtEmbed>);
    /// `getRoot().getType()` exists and `getProperty(name) != null`.
    fn root_type_has_property(&self, name: &str) -> bool;
    fn add_type_ref(&self, name: &str, line: i32);
    /// `getStandardDefs().getEffectsPackage()`.
    fn effects_package(&self) -> String;
}

/// The `flex2.compiler.mxml.reflect.Type` calls `StylesContainer.validate` makes.
pub trait CssType {
    fn has_style(&self, name: &str) -> bool;
    fn get_style_themes(&self, name: &str) -> Option<String>;
    fn is_excluded_style(&self, name: &str) -> bool;
    fn has_property(&self, name: &str) -> bool;
}

/// `TypeTable.getType(String)`.
pub trait CssTypeTable {
    fn get_type(&self, class_name: &str) -> Option<Rc<dyn CssType>>;
}

pub const VERSION_3_0: i32 = 0x03000000;
pub const VERSION_4_5: i32 = 0x04050000;
pub const VERSION_4_6: i32 = 0x04060000;

/// The `MxmlConfiguration` / `CompilerConfiguration` values CSS reads.
#[derive(Clone, Debug)]
pub struct CssConfiguration {
    pub compatibility_version: i32,
    pub compatibility_version_string: Option<String>,
    pub qualified_type_selectors: bool,
    pub keep_all_type_selectors: bool,
    pub show_invalid_css_property_warnings: bool,
    pub show_unused_type_selector_warnings: bool,
    pub show_deprecation_warnings: bool,
    pub allow_duplicate_default_style_declarations: bool,
    pub archive_classes_and_assets: bool,
    pub keep_generated_actionscript: bool,
    pub generated_directory: Option<String>,
    pub debug: bool,
    /// `-defaults-css-url`.
    pub defaults_css_url: Option<CssFile>,
    /// `getDefaultsCssFiles()`: the SWCs' `defaults.css`, as `PreLink.locateStyleDefaults` adds them.
    pub defaults_css_files: Vec<CssFile>,
    /// `getThemeCssFiles()`.
    pub theme_css_files: Vec<CssFile>,
    /// `getThemeNames()`, in its `HashSet` order.
    pub theme_names: Vec<String>,
}

impl Default for CssConfiguration {
    fn default() -> Self {
        CssConfiguration {
            compatibility_version: 0x04060000,
            compatibility_version_string: None,
            qualified_type_selectors: true,
            keep_all_type_selectors: false,
            show_invalid_css_property_warnings: true,
            show_unused_type_selector_warnings: true,
            show_deprecation_warnings: true,
            allow_duplicate_default_style_declarations: false,
            archive_classes_and_assets: false,
            keep_generated_actionscript: false,
            generated_directory: None,
            debug: false,
            defaults_css_url: None,
            defaults_css_files: Vec::new(),
            theme_css_files: Vec::new(),
            theme_names: Vec::new(),
        }
    }
}

/// `FrameworkDefs.isBuiltinEffectName`.
pub fn is_builtin_effect_name(name: &str) -> bool {
    matches!(name, "Dissolve" | "Fade" | "WipeLeft" | "WipeRight" | "WipeUp" | "WipeDown" | "Zoom" | "Resize" | "Move" | "Pause" | "Rotate" | "Iris" | "Blur" | "Glow")
}
