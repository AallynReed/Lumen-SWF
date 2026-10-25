//! The style options of `CompilerConfiguration` (`-defaults-css-url`, `-defaults-css-files`, `-theme`,
//! `-compatibility-version`, the namespace manifests and the CSS warnings), and `CompilerSwcContext.getThemeStyleSheets`.

use std::path::PathBuf;
use std::rc::Rc;

use super::css_file::{self, CssFile};
use super::document::CssConfiguration;
use crate::asc::flex2::compiler::swc::Swc;
use crate::asc::util::jhash::JHashSet;

#[derive(Clone, Debug)]
pub struct CssOptions {
    pub defaults_css_url: Option<PathBuf>,
    pub defaults_css_files: Vec<PathBuf>,
    /// `-theme`, as `cfgTheme` gets the paths.
    pub theme: Vec<PathBuf>,
    /// `-namespace` URI with its manifests, in configuration order.
    pub namespaces: Vec<(String, Vec<PathBuf>)>,
    /// `-compatibility-version` as major, minor, revision.
    pub compatibility_version: Option<(i32, i32, i32)>,
    pub qualified_type_selectors: bool,
    pub keep_all_type_selectors: bool,
    pub show_invalid_css_property_warnings: bool,
    pub show_unused_type_selector_warnings: bool,
    pub allow_duplicate_default_style_declarations: bool,
    pub isolate_styles: bool,
}

impl Default for CssOptions {
    fn default() -> Self {
        CssOptions {
            defaults_css_url: None,
            defaults_css_files: Vec::new(),
            theme: Vec::new(),
            namespaces: Vec::new(),
            compatibility_version: None,
            qualified_type_selectors: true,
            keep_all_type_selectors: false,
            show_invalid_css_property_warnings: true,
            show_unused_type_selector_warnings: true,
            allow_duplicate_default_style_declarations: false,
            isolate_styles: true,
        }
    }
}

impl CssOptions {
    /// `MxmlConfiguration.getCompatibilityVersion()`.
    pub fn compatibility_version(&self) -> i32 {
        match self.compatibility_version {
            Some((a, b, c)) if (a << 24) + (b << 16) + c != 0 => (a << 24) + (b << 16) + c,
            _ => super::document::VERSION_4_6,
        }
    }

    pub fn compatibility_version_string(&self) -> Option<String> {
        let (a, b, c) = self.compatibility_version.unwrap_or((4, 6, 0));
        (a != 0 || b != 0 || c != 0).then(|| format!("{a}.{b}.{c}"))
    }

    /// `getThemeFiles()`: a Flex 3 compile swaps the one default Spark theme for Halo.
    pub fn theme_files(&self) -> (Vec<PathBuf>, Vec<String>) {
        let mut names: Vec<String> = self.theme.iter().filter_map(|p| theme_name(p)).collect();
        let mut files = self.theme.clone();
        if self.compatibility_version() <= super::document::VERSION_3_0 && files.len() == 1 {
            let name = crate::asc::flex2::compiler::source::canonical_path(&files[0]);
            let tail = std::path::Path::new("/themes/Spark/spark.css").to_string_lossy().replace('/', std::path::MAIN_SEPARATOR_STR);
            if let Some(index) = name.find(&tail).filter(|_| name.ends_with(&tail)) {
                files[0] = PathBuf::from(format!("{}{}", &name[..index], "/themes/Halo/halo.swc"));
                names.retain(|n| n != "spark");
                names.push("halo".to_string());
            }
        }
        (files, names)
    }

    /// The `CssConfiguration` a compile's `StylesContainer` reads; `swcs` are the libraries `SwcPathResolver` sees.
    pub fn css_configuration(&self, debug: bool, generated_directory: Option<String>, swcs: Vec<Rc<Swc>>) -> CssConfiguration {
        css_file::set_swcs(swcs);
        let (theme_files, names) = self.theme_files();
        let mut theme_names = JHashSet::new();
        for n in names {
            theme_names.insert(n);
        }
        CssConfiguration {
            compatibility_version: self.compatibility_version(),
            compatibility_version_string: self.compatibility_version_string(),
            qualified_type_selectors: self.qualified_type_selectors,
            keep_all_type_selectors: self.keep_all_type_selectors,
            show_invalid_css_property_warnings: self.show_invalid_css_property_warnings,
            show_unused_type_selector_warnings: self.show_unused_type_selector_warnings,
            show_deprecation_warnings: true,
            allow_duplicate_default_style_declarations: self.allow_duplicate_default_style_declarations,
            archive_classes_and_assets: false,
            keep_generated_actionscript: generated_directory.is_some(),
            generated_directory,
            debug,
            defaults_css_url: self.defaults_css_url.as_ref().map(|p| CssFile::local(p)),
            // `cfgDefaultsCssFiles` inserts each at the same index: A,B,C become C,B,A.
            defaults_css_files: self.defaults_css_files.iter().rev().map(|p| CssFile::local(p)).collect(),
            theme_css_files: theme_style_sheets(&theme_files),
            theme_names: theme_names.iter().cloned().collect(),
        }
    }
}

/// `CompilerConfiguration.addThemeName(path)`.
fn theme_name(path: &std::path::Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    let end = file_name.find('-').or_else(|| file_name.rfind('.'))?;
    let name = &file_name[..end];
    (!name.is_empty()).then(|| name.to_string())
}

/// `CompilerSwcContext.getThemeStyleSheets()`: a `.css` theme as it is, a SWC theme's `.css` files in its file map's
/// order.
pub fn theme_style_sheets(theme_files: &[PathBuf]) -> Vec<CssFile> {
    let mut out = Vec::new();
    for t in theme_files {
        let name = t.to_string_lossy();
        if name.to_ascii_lowercase().ends_with(".css") {
            out.push(CssFile::local(t));
        } else if let Ok(swc) = Swc::load(t) {
            let mut files: JHashSet<String> = JHashSet::new();
            for f in catalog_files(&swc.location) {
                files.insert(f);
            }
            for f in files.iter() {
                if f.ends_with(".css") {
                    out.push(CssFile::Zip { swc: swc.location.clone(), entry: f.clone() });
                }
            }
        }
    }
    out
}

/// The `<files>` of a SWC's `catalog.xml`, which `SwcGroup.getFiles()` maps.
fn catalog_files(swc: &str) -> Vec<String> {
    let Ok(bytes) = CssFile::Zip { swc: swc.to_string(), entry: "catalog.xml".to_string() }.to_byte_array() else { return Vec::new() };
    let text = String::from_utf8_lossy(&bytes);
    let Ok(doc) = roxmltree::Document::parse(&text) else { return Vec::new() };
    doc.descendants().filter(|n| n.has_tag_name("file") && n.parent().is_some_and(|p| p.has_tag_name("files"))).filter_map(|n| n.attribute("path").map(str::to_string)).collect()
}

impl CssOptions {
    /// The flags `CssCompiler` reads, without resolving any style sheet.
    pub fn compiler_configuration(&self, debug: bool, generated_directory: Option<String>) -> CssConfiguration {
        CssConfiguration {
            compatibility_version: self.compatibility_version(),
            compatibility_version_string: self.compatibility_version_string(),
            qualified_type_selectors: self.qualified_type_selectors,
            keep_generated_actionscript: generated_directory.is_some(),
            generated_directory,
            debug,
            ..Default::default()
        }
    }
}
