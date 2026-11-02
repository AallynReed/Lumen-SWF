//! The Flex values of `flex2.compiler.common.Configuration`, `CompilerConfiguration`, `FramesConfiguration`,
//! `MetadataConfiguration` and `RuntimeSharedLibrarySettingsConfiguration` that `PreLink`, the i18n compiler and the
//! MXML compiler read, with `CompilerConfiguration`'s own defaults (`file_configurator::load_defaults` applies
//! mxmlc's `DefaultsConfigurator` ones).

/// `FramesConfiguration.FrameInfo`.
#[derive(Clone, Debug, Default)]
pub struct FrameInfo {
    pub label: String,
    pub frame_classes: Vec<String>,
}

/// `Configuration.RslPathInfo`.
#[derive(Clone, Debug, Default)]
pub struct RslPathInfo {
    pub swc_path: String,
    pub rsl_urls: Vec<String>,
    pub policy_file_urls: Vec<String>,
    pub signed_flags: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct FlexOptions {
    /// `compiler.locale`.
    pub locales: Vec<String>,
    pub accessible: bool,
    pub use_resource_bundle_metadata: bool,
    pub resource_hack: bool,
    pub archive_classes_and_assets: bool,
    /// `getFrameList()`.
    pub frame_list: Vec<FrameInfo>,
    /// `runtime-shared-libraries`.
    pub runtime_shared_libraries: Vec<String>,
    /// `getRslPathInfo()`: empty while `static_link_rsl`, as `cfgRuntimeSharedLibraryPath` ignores the option then.
    pub rsl_path_info: Vec<RslPathInfo>,
    pub static_link_rsl: bool,
    pub remove_unused_rsls: bool,
    pub verify_digests: bool,
    pub force_rsls: Vec<String>,
    pub preloader: Option<String>,
    pub enable_swc_version_filtering: bool,
    pub include_resource_bundles: Vec<String>,
    pub framework: String,
    pub mobile: bool,
    pub show_binding_warnings: bool,
    pub show_shadowed_device_font_warnings: bool,
    pub report_invalid_styles_as_warnings: bool,
    pub report_missing_required_skin_parts_as_warnings: bool,
    pub enable_runtime_design_layers: bool,
    pub context_root: Option<String>,
    pub compiling_for_air: bool,
    /// `compiler.generate-abstract-syntax-tree`; `setKeepGeneratedActionScript(true)` turns it off.
    pub generate_abstract_syntax_tree: bool,
    /// Values [`super::configuration::Configuration`] already has, set from a configuration file.
    pub use_network: bool,
    pub allow_source_path_overlap: bool,
    pub verbose_stacktraces: bool,
    pub show_deprecation_warnings: bool,
    pub keep_as3_metadata: Option<Vec<String>>,
    pub includes: Vec<String>,
    pub include_libraries: Vec<std::path::PathBuf>,
    pub metadata: MetadataConfiguration,
    /// `compiler.fonts`.
    pub fonts: super::fonts_configuration::FontsConfiguration,
    /// `compiler.warn-*`, by option name without the prefix, in configuration order.
    pub warn: Vec<(String, bool)>,
}

impl Default for FlexOptions {
    fn default() -> Self {
        FlexOptions {
            locales: Vec::new(),
            accessible: false,
            use_resource_bundle_metadata: false,
            resource_hack: false,
            archive_classes_and_assets: false,
            frame_list: Vec::new(),
            runtime_shared_libraries: Vec::new(),
            rsl_path_info: Vec::new(),
            static_link_rsl: true,
            remove_unused_rsls: false,
            verify_digests: true,
            force_rsls: Vec::new(),
            preloader: None,
            enable_swc_version_filtering: true,
            include_resource_bundles: Vec::new(),
            framework: "halo".to_string(),
            mobile: false,
            show_binding_warnings: true,
            show_shadowed_device_font_warnings: true,
            report_invalid_styles_as_warnings: false,
            report_missing_required_skin_parts_as_warnings: false,
            enable_runtime_design_layers: true,
            context_root: None,
            compiling_for_air: false,
            generate_abstract_syntax_tree: true,
            use_network: true,
            allow_source_path_overlap: false,
            verbose_stacktraces: false,
            show_deprecation_warnings: true,
            keep_as3_metadata: None,
            includes: Vec::new(),
            include_libraries: Vec::new(),
            metadata: MetadataConfiguration::default(),
            fonts: Default::default(),
            warn: Vec::new(),
        }
    }
}

/// `MetadataConfiguration`: the SWF `Metadata` tag's RDF.
#[derive(Clone, Debug, Default)]
pub struct MetadataConfiguration {
    /// `localizedTitles`, a `TreeMap` by language.
    pub localized_titles: Vec<(String, String)>,
    pub localized_descriptions: Vec<(String, String)>,
    /// `TreeSet`s.
    pub publishers: Vec<String>,
    pub creators: Vec<String>,
    pub contributors: Vec<String>,
    pub langs: Vec<String>,
    pub date: Option<String>,
}

fn f(s: &str) -> String {
    let s = s.trim_matches(|c: char| c <= ' ');
    if !s.contains('<') && !s.contains('>') {
        s.to_string()
    } else {
        format!("<![CDATA[{s}]]>")
    }
}

fn tree_insert(v: &mut Vec<String>, s: String) {
    if let Err(i) = v.binary_search_by(|x| crate::asc::util::jstr::compare(x, &s)) {
        v.insert(i, s);
    }
}

fn tree_put(v: &mut Vec<(String, String)>, k: String, val: String) {
    match v.binary_search_by(|x| crate::asc::util::jstr::compare(&x.0, &k)) {
        Ok(i) => v[i].1 = val,
        Err(i) => v.insert(i, (k, val)),
    }
}

impl MetadataConfiguration {
    pub fn add_title(&mut self, lang: &str, title: &str) {
        tree_put(&mut self.localized_titles, lang.to_string(), title.to_string());
    }
    pub fn add_description(&mut self, lang: &str, text: &str) {
        tree_put(&mut self.localized_descriptions, lang.to_string(), text.to_string());
    }
    pub fn add_publisher(&mut self, s: &str) {
        tree_insert(&mut self.publishers, s.to_string());
    }
    pub fn add_creator(&mut self, s: &str) {
        tree_insert(&mut self.creators, s.to_string());
    }
    pub fn add_contributor(&mut self, s: &str) {
        tree_insert(&mut self.contributors, s.to_string());
    }
    pub fn add_language(&mut self, s: &str) {
        tree_insert(&mut self.langs, s.to_string());
    }

    pub fn is_set(&self) -> bool {
        !self.localized_titles.is_empty() || !self.localized_descriptions.is_empty() || !self.publishers.is_empty() || !self.creators.is_empty() || !self.contributors.is_empty() || !self.langs.is_empty() || self.date.is_some()
    }

    /// `toString()`; `today` is `DateFormat.getDateInstance().format(new Date())` for when no date is set.
    pub fn to_rdf(&self, today: &str) -> Option<String> {
        if !self.is_set() {
            return None;
        }
        let mut sb = String::new();
        sb.push_str("<rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>");
        sb.push_str("<rdf:Description rdf:about='' xmlns:dc='http://purl.org/dc/elements/1.1'>");
        sb.push_str("<dc:format>application/x-shockwave-flash</dc:format>");
        for (tag, list) in [("title", &self.localized_titles), ("description", &self.localized_descriptions)] {
            if list.is_empty() {
                continue;
            }
            if list.len() == 1 && list[0].0 == "x-default" {
                sb.push_str(&format!("<dc:{tag}>{}</dc:{tag}>", f(&list[0].1)));
            } else {
                sb.push_str(&format!("<dc:{tag}><rdf:Alt>"));
                for (k, v) in list {
                    sb.push_str(&format!("<rdf:li xml:lang='{k}'>{}</rdf:li>", f(v)));
                }
                sb.push_str(&format!("</rdf:Alt></dc:{tag}>"));
            }
        }
        for (tag, list) in [("publisher", &self.publishers), ("creator", &self.creators), ("contributor", &self.contributors), ("language", &self.langs)] {
            for s in list {
                sb.push_str(&format!("<dc:{tag}>{}</dc:{tag}>", f(s)));
            }
        }
        let date = self.date.clone().unwrap_or_else(|| today.to_string());
        sb.push_str(&format!("<dc:date>{}</dc:date>", f(&date)));
        sb.push_str("</rdf:Description></rdf:RDF>");
        Some(sb)
    }
}

/// `DateFormat.getDateInstance().format(date)` for the `en_US` locale (`MEDIUM`: `Oct 25, 2026`), in local time.
pub fn java_medium_date(millis: i64) -> String {
    let days = (millis + local_offset_millis()).div_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {}, {}", MONTHS[(m - 1) as usize], d, y)
}

fn local_offset_millis() -> i64 {
    0
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
