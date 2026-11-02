//! Port of the `flex2.compiler.mxml.MxmlConfiguration` interface as the values `CompilerConfiguration` gives it.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::css::document::CssConfiguration;

#[derive(Clone)]
pub struct MxmlConfiguration {
    pub debug: bool,
    pub accessible: bool,
    pub keep_generated_action_script: bool,
    pub enable_runtime_design_layers: bool,
    pub enable_swc_version_filtering: bool,
    pub keep_all_type_selectors: bool,
    pub generated_directory: Option<String>,
    pub report_invalid_styles_as_warnings: bool,
    pub show_deprecation_warnings: bool,
    pub show_binding_warnings: bool,
    pub show_invalid_css_property_warnings: bool,
    pub show_shadowed_device_font_warnings: bool,
    pub show_unused_type_selector_warnings: bool,
    pub context_root: Option<String>,
    pub compatibility_version_string: Option<String>,
    pub compatibility_version: i32,
    pub qualified_type_selectors: bool,
    pub source_path: Vec<PathBuf>,
    pub library_path: Vec<PathBuf>,
    pub compiling_for_air: bool,
    pub generate_abstract_syntax_tree: bool,
    pub theme_names: HashSet<String>,
    pub allow_duplicate_default_style_declarations: bool,
    pub report_missing_required_skin_parts_as_warnings: bool,
    /// The style flags `StylesContainer`/`StyleDef` read.
    pub css: Rc<CssConfiguration>,
}

impl MxmlConfiguration {
    pub fn new(c: &Configuration) -> MxmlConfiguration {
        let (_, theme_names) = c.css.theme_files();
        let mut css = c.css.compiler_configuration(c.debug, c.generated_directory.clone());
        css.keep_all_type_selectors = c.css.keep_all_type_selectors;
        css.show_invalid_css_property_warnings = c.css.show_invalid_css_property_warnings;
        css.show_unused_type_selector_warnings = c.css.show_unused_type_selector_warnings;
        css.show_deprecation_warnings = c.show_deprecation_warnings;
        css.allow_duplicate_default_style_declarations = c.css.allow_duplicate_default_style_declarations;
        css.theme_names = theme_names.clone();
        MxmlConfiguration {
            debug: c.debug,
            accessible: c.flex.accessible,
            keep_generated_action_script: c.generated_directory.is_some(),
            enable_runtime_design_layers: c.flex.enable_runtime_design_layers,
            enable_swc_version_filtering: c.flex.enable_swc_version_filtering,
            keep_all_type_selectors: c.css.keep_all_type_selectors,
            generated_directory: c.generated_directory.clone(),
            report_invalid_styles_as_warnings: c.flex.report_invalid_styles_as_warnings,
            show_deprecation_warnings: c.show_deprecation_warnings,
            show_binding_warnings: c.flex.show_binding_warnings,
            show_invalid_css_property_warnings: c.css.show_invalid_css_property_warnings,
            show_shadowed_device_font_warnings: c.flex.show_shadowed_device_font_warnings,
            show_unused_type_selector_warnings: c.css.show_unused_type_selector_warnings,
            context_root: c.flex.context_root.clone(),
            compatibility_version_string: c.css.compatibility_version_string(),
            compatibility_version: c.css.compatibility_version(),
            qualified_type_selectors: c.css.qualified_type_selectors,
            source_path: c.source_path.clone(),
            library_path: c.library_path.clone(),
            compiling_for_air: c.flex.compiling_for_air,
            generate_abstract_syntax_tree: c.get_generate_abstract_syntax_tree(),
            theme_names: theme_names.into_iter().collect(),
            allow_duplicate_default_style_declarations: c.css.allow_duplicate_default_style_declarations,
            report_missing_required_skin_parts_as_warnings: c.flex.report_missing_required_skin_parts_as_warnings,
            css: Rc::new(css),
        }
    }

    pub fn debug(&self) -> bool {
        self.debug
    }
    pub fn accessible(&self) -> bool {
        self.accessible
    }
    pub fn keep_generated_action_script(&self) -> bool {
        self.keep_generated_action_script
    }
    pub fn enable_runtime_design_layers(&self) -> bool {
        self.enable_runtime_design_layers
    }
    pub fn get_generated_directory(&self) -> Option<&str> {
        self.generated_directory.as_deref()
    }
    pub fn show_deprecation_warnings(&self) -> bool {
        self.show_deprecation_warnings
    }
    pub fn show_binding_warnings(&self) -> bool {
        self.show_binding_warnings
    }
    pub fn get_context_root(&self) -> Option<&str> {
        self.context_root.as_deref()
    }
    pub fn get_compatibility_version(&self) -> i32 {
        self.compatibility_version
    }
    pub fn get_compatibility_version_string(&self) -> Option<&str> {
        self.compatibility_version_string.as_deref()
    }
    pub fn get_qualified_type_selectors(&self) -> bool {
        self.qualified_type_selectors
    }
    pub fn get_source_path(&self) -> &[PathBuf] {
        &self.source_path
    }
    pub fn get_compiling_for_air(&self) -> bool {
        self.compiling_for_air
    }
    pub fn get_generate_abstract_syntax_tree(&self) -> bool {
        self.generate_abstract_syntax_tree
    }
    pub fn get_theme_names(&self) -> &HashSet<String> {
        &self.theme_names
    }
    pub fn get_allow_duplicate_default_style_declarations(&self) -> bool {
        self.allow_duplicate_default_style_declarations
    }
    pub fn report_missing_required_skin_parts_as_warnings(&self) -> bool {
        self.report_missing_required_skin_parts_as_warnings
    }
}
