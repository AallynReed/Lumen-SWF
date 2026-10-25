//! The values of `flex2.compiler.common.Configuration`, `CompilerConfiguration` and `LinkerConfiguration` that a
//! pure-ActionScript mxmlc build reads, with `DefaultsConfigurator.loadDefaults` and the `-load-config=` defaults.

use std::path::PathBuf;

use crate::asc::util::context::ConfigVar;

#[derive(Clone)]
pub struct Configuration {
    pub strict: bool,
    pub warnings: bool,
    pub debug: bool,
    pub optimize: bool,
    pub omit_trace_statements: bool,
    pub as3: bool,
    pub verbose_stacktraces: bool,
    pub keep_embed_metadata: bool,
    pub show_deprecation_warnings: bool,
    pub show_dependency_warnings: bool,
    pub allow_source_path_overlap: bool,
    pub doc: bool,
    pub defines: Vec<ConfigVar>,
    pub keep_as3_metadata: Option<Vec<String>>,
    pub source_path: Vec<PathBuf>,
    pub library_path: Vec<PathBuf>,
    pub external_library_path: Vec<PathBuf>,
    pub target_player_major: i32,
    pub target_player_minor: i32,
    pub swf_version: i32,
    pub default_width: i32,
    pub default_height: i32,
    pub width: Option<String>,
    pub height: Option<String>,
    pub width_percent: Option<String>,
    pub height_percent: Option<String>,
    pub frame_rate: i32,
    pub background_color: i32,
    pub script_recursion_limit: i32,
    pub script_time_limit: i32,
    pub use_network: bool,
    pub use_direct_blit: bool,
    pub use_gpu: bool,
    pub lazy_init: bool,
    pub generate_frame_loader: bool,
    pub metadata: Option<String>,
    pub debug_password: String,
    pub page_title: Option<String>,
    pub main_definition: Option<String>,
    pub root_class_name: Option<String>,
    pub externs: Vec<String>,
    pub includes: Vec<String>,
    pub resource_bundles: Vec<String>,
    /// `System.currentTimeMillis()` as `ProductInfo` stores it; `None` reads the clock.
    pub compile_date: Option<i64>,
    /// `VersionInfo` of the mxmlc being reproduced (Flex 4.6.0 build 23188).
    pub flex_major: u8,
    pub flex_minor: u8,
    pub flex_build: i64,
    /// `compiler.keep-generated-actionscript` with its `generated-directory`.
    pub generated_directory: Option<String>,
    /// The style options (`-defaults-css-url`, `-theme`, namespaces, ...).
    pub css: super::css::options::CssOptions,
}

impl Default for Configuration {
    fn default() -> Self {
        Configuration {
            strict: true,
            warnings: true,
            debug: false,
            optimize: true,
            omit_trace_statements: true,
            as3: true,
            verbose_stacktraces: false,
            keep_embed_metadata: false,
            show_deprecation_warnings: true,
            show_dependency_warnings: false,
            allow_source_path_overlap: false,
            doc: false,
            defines: Vec::new(),
            keep_as3_metadata: None,
            source_path: Vec::new(),
            library_path: Vec::new(),
            external_library_path: Vec::new(),
            target_player_major: 11,
            target_player_minor: 1,
            swf_version: 14,
            default_width: 500,
            default_height: 375,
            width: None,
            height: None,
            width_percent: None,
            height_percent: None,
            frame_rate: 24,
            background_color: 0xFFFFFF,
            script_recursion_limit: 1000,
            script_time_limit: 60,
            use_network: true,
            use_direct_blit: false,
            use_gpu: false,
            lazy_init: true,
            generate_frame_loader: true,
            metadata: None,
            debug_password: String::new(),
            page_title: None,
            main_definition: None,
            root_class_name: None,
            externs: Vec::new(),
            includes: Vec::new(),
            resource_bundles: Vec::new(),
            compile_date: None,
            flex_major: 4,
            flex_minor: 6,
            flex_build: 23188,
            generated_directory: None,
            css: Default::default(),
        }
    }
}

impl Configuration {
    /// `CompilerConfiguration.dialect()`.
    pub fn dialect(&self) -> i32 {
        if self.as3 {
            crate::asc::emit::features::DIALECT_AS3
        } else {
            crate::asc::emit::features::DIALECT_ES3
        }
    }

    pub fn add_externs(&mut self, externs: impl IntoIterator<Item = String>) {
        for e in externs {
            if !self.externs.contains(&e) {
                self.externs.push(e);
            }
        }
    }
}
