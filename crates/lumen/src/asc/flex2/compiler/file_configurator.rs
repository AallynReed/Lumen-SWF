//! `flex2.compiler.config.FileConfigurator` over mxmlc's configuration variables, for the options
//! [`ApplicationOptions`] models: a `flex-config.xml` (or `-load-config` file) applied as mxmlc applies it after
//! `DefaultsConfigurator.loadDefaults`, with `append="true"`, `${flexlib}` and the `{locale}`,
//! `{targetPlayerMajorVersion}`, `{targetPlayerMinorVersion}`, `{playerglobalHome}` and `{airHome}` path tokens.
//! Library-path directories expand to their `.swc` files in `File.list()` order, as `SwcCache.getSwcs` does.

use std::path::{Path, PathBuf};

use crate::asc::flex2::compiler::flex_options::{FrameInfo, RslPathInfo};
use crate::asc::flex2::ApplicationOptions;

/// The mxmlc defaults `DefaultsConfigurator.loadDefaults` sets that differ from [`ApplicationOptions::default`]:
/// the Spark theme, the (JVM default, here `en_US`) locale and `use-resource-bundle-metadata`.
pub fn load_defaults(opts: &mut ApplicationOptions, flexlib: &Path) {
    opts.css.theme = vec![flexlib.join("themes").join("Spark").join("spark.css")];
    opts.flex.locales = vec!["en_US".to_string()];
    opts.flex.use_resource_bundle_metadata = true;
}

/// `mxmlc` with its default `-load-config` (`${flexlib}/flex-config.xml`).
pub fn mxmlc_defaults(flexlib: &Path) -> Result<ApplicationOptions, String> {
    let mut opts = ApplicationOptions::default();
    load_defaults(&mut opts, flexlib);
    load(&mut opts, &flexlib.join("flex-config.xml"), flexlib)?;
    Ok(opts)
}

struct Ctx<'a> {
    dir: PathBuf,
    flexlib: &'a Path,
}

impl Ctx<'_> {
    fn expand(&self, s: &str) -> String {
        s.replace("${flexlib}", &self.flexlib.to_string_lossy())
    }

    fn path(&self, s: &str) -> PathBuf {
        let s = self.expand(s.trim());
        let p = PathBuf::from(&s);
        if p.is_absolute() {
            p
        } else {
            self.dir.join(p)
        }
    }
}

fn text(n: roxmltree::Node) -> String {
    n.text().unwrap_or("").trim().to_string()
}

fn is_true(n: roxmltree::Node) -> bool {
    text(n).eq_ignore_ascii_case("true")
}

fn appends(n: roxmltree::Node) -> bool {
    n.attribute("append").is_some_and(|a| a.eq_ignore_ascii_case("true"))
}

fn children<'a>(n: roxmltree::Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> + 'a {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn values(n: roxmltree::Node, element: &str) -> Vec<String> {
    children(n, element).map(text).collect()
}

/// `expandRuntimeTokens` and `expandTargetPlayerToken`.
fn runtime_tokens(s: &str, opts: &ApplicationOptions) -> String {
    let mut tp = opts.target_player.split('.');
    let major = tp.next().unwrap_or("11").to_string();
    let minor = tp.next().unwrap_or("0").to_string();
    let mut s = s.replace("{targetPlayerMajorVersion}", &major).replace("{targetPlayerMinorVersion}", &minor);
    if let Ok(h) = std::env::var("PLAYERGLOBAL_HOME") {
        s = s.replace("{playerglobalHome}", &h);
    }
    if let Ok(h) = std::env::var("AIR_HOME") {
        s = s.replace("{airHome}", &h);
    }
    s
}

/// `expandLocaleToken`: a `{locale}` element becomes one element per locale.
fn locale_paths(elements: &[String], opts: &ApplicationOptions, ctx: &Ctx) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in elements {
        let e = runtime_tokens(e, opts);
        if e.contains("{locale}") {
            for l in &opts.flex.locales {
                out.push(ctx.path(&e.replace("{locale}", l)));
            }
        } else {
            out.push(ctx.path(&e));
        }
    }
    out
}

/// `SwcCache.getSwcs(path)`: a directory without a `catalog.xml` contributes its `.swc` files.
fn swc_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() && !p.join("catalog.xml").exists() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    let f = e.path();
                    let lower = e.file_name().to_string_lossy().to_lowercase();
                    if f.is_file() && lower.ends_with(".swc") && !lower.ends_with(".genswc") {
                        out.push(f);
                    }
                }
            }
        } else {
            out.push(p);
        }
    }
    out
}

fn set_list<T>(dst: &mut Vec<T>, append: bool, items: Vec<T>) {
    if !append {
        dst.clear();
    }
    dst.extend(items);
}

/// Applies the configuration file `file` to `opts`.
pub fn load(opts: &mut ApplicationOptions, file: &Path, flexlib: &Path) -> Result<(), String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let text_s = String::from_utf8_lossy(&bytes).into_owned();
    let doc = roxmltree::Document::parse(&text_s).map_err(|e| format!("{}: {e}", file.display()))?;
    let root = doc.root_element();
    let ctx = Ctx { dir: file.parent().map(Path::to_path_buf).unwrap_or_default(), flexlib };
    // target-player and locale first: the path variables depend on them.
    for n in root.children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "target-player" => opts.target_player = text(n),
            "compiler" => {
                for c in children(n, "locale") {
                    let l: Vec<String> = values(c, "locale-element").into_iter().filter(|l| !l.is_empty()).collect();
                    set_list(&mut opts.flex.locales, appends(c), l);
                }
            }
            _ => {}
        }
    }
    for n in root.children().filter(|n| n.is_element()) {
        let name = n.tag_name().name();
        match name {
            "target-player" | "include-resource-bundles" => {
                if name == "include-resource-bundles" {
                    set_list(&mut opts.flex.include_resource_bundles, appends(n), values(n, "bundle"));
                }
            }
            "swf-version" => opts.swf_version = text(n).parse().unwrap_or(opts.swf_version),
            "compiler" => load_compiler(opts, n, &ctx),
            "remove-unused-rsls" => opts.flex.remove_unused_rsls = is_true(n),
            "static-link-runtime-shared-libraries" => {
                opts.flex.static_link_rsl = is_true(n);
                if opts.flex.static_link_rsl {
                    opts.flex.rsl_path_info.clear();
                }
            }
            "runtime-shared-libraries" => set_list(&mut opts.flex.runtime_shared_libraries, appends(n), values(n, "url")),
            "runtime-shared-library-path" => {
                if !opts.flex.static_link_rsl {
                    let urls = values(n, "rsl-url");
                    let policies = values(n, "policy-file-url");
                    let info = RslPathInfo {
                        swc_path: values(n, "path-element").first().map(|p| crate::asc::flex2::compiler::source::canonical_path(&ctx.path(p))).unwrap_or_default(),
                        policy_file_urls: (0..urls.len()).map(|i| policies.get(i).cloned().unwrap_or_default()).collect(),
                        signed_flags: urls.iter().map(|u| u.ends_with(".swz")).collect(),
                        rsl_urls: urls,
                    };
                    opts.flex.rsl_path_info.push(info);
                }
            }
            "force-rsls" => set_list(&mut opts.flex.force_rsls, appends(n), values(n, "path-element")),
            "verify-digests" => opts.flex.verify_digests = is_true(n),
            "use-network" => opts.flex.use_network = is_true(n),
            "default-frame-rate" => opts.default_frame_rate = text(n).parse().unwrap_or(opts.default_frame_rate),
            "default-background-color" => {
                if let Some(v) = crate::asc::flex2::tools::pre_link::integer_decode(&text(n)) {
                    opts.default_background_color = v;
                }
            }
            "default-size" => {
                let w = children(n, "width").next().map(text).and_then(|t| t.parse().ok());
                let h = children(n, "height").next().map(text).and_then(|t| t.parse().ok());
                if let (Some(w), Some(h)) = (w, h) {
                    opts.default_size = (w, h);
                }
            }
            "frames" => {
                let frames: Vec<FrameInfo> = children(n, "frame").map(|f| FrameInfo { label: children(f, "label").next().map(text).unwrap_or_default(), frame_classes: values(f, "classname") }).collect();
                set_list(&mut opts.flex.frame_list, appends(n), frames);
            }
            "metadata" => {
                let m = &mut opts.flex.metadata;
                for c in n.children().filter(|c| c.is_element()) {
                    match c.tag_name().name() {
                        "title" => m.add_title("x-default", &text(c)),
                        "description" => m.add_description("x-default", &text(c)),
                        "publisher" => m.add_publisher(&text(c)),
                        "creator" => m.add_creator(&text(c)),
                        "contributor" => m.add_contributor(&text(c)),
                        "language" => m.add_language(&text(c)),
                        "date" => m.date = Some(text(c)),
                        "localized-title" => m.add_title(&children(c, "lang").next().map(text).unwrap_or_default(), &children(c, "title").next().map(text).unwrap_or_default()),
                        "localized-description" => m.add_description(&children(c, "lang").next().map(text).unwrap_or_default(), &children(c, "text").next().map(text).unwrap_or_default()),
                        _ => {}
                    }
                }
            }
            "framework" => opts.flex.framework = text(n),
            "includes" => set_list(&mut opts.flex.includes, appends(n), values(n, "symbol")),
            _ => {}
        }
    }
    Ok(())
}

fn load_compiler(opts: &mut ApplicationOptions, n: roxmltree::Node, ctx: &Ctx) {
    for c in n.children().filter(|c| c.is_element()) {
        let append = appends(c);
        match c.tag_name().name() {
            "accessible" => opts.flex.accessible = is_true(c),
            "report-missing-required-skin-parts-as-warnings" => opts.flex.report_missing_required_skin_parts_as_warnings = is_true(c),
            "allow-source-path-overlap" => opts.flex.allow_source_path_overlap = is_true(c),
            "show-actionscript-warnings" => opts.warnings = is_true(c),
            "debug" => opts.debug = is_true(c),
            "optimize" => opts.optimize = is_true(c),
            "omit-trace-statements" => opts.omit_trace_statements = is_true(c),
            "strict" => opts.strict = is_true(c),
            "verbose-stacktraces" => opts.flex.verbose_stacktraces = is_true(c),
            "show-binding-warnings" => opts.flex.show_binding_warnings = is_true(c),
            "show-shadowed-device-font-warnings" => opts.flex.show_shadowed_device_font_warnings = is_true(c),
            "show-unused-type-selector-warnings" => opts.css.show_unused_type_selector_warnings = is_true(c),
            "show-invalid-css-property-warnings" => opts.css.show_invalid_css_property_warnings = is_true(c),
            "show-deprecation-warnings" => opts.flex.show_deprecation_warnings = is_true(c),
            "keep-all-type-selectors" => opts.css.keep_all_type_selectors = is_true(c),
            "isolate-styles" => opts.css.isolate_styles = is_true(c),
            "qualified-type-selectors" => opts.css.qualified_type_selectors = is_true(c),
            "mobile" => opts.flex.mobile = is_true(c),
            "use-resource-bundle-metadata" => opts.flex.use_resource_bundle_metadata = is_true(c),
            "source-path" => {
                let p = locale_paths(&values(c, "path-element"), opts, ctx);
                set_list(&mut opts.source_path, append, p);
            }
            "library-path" => {
                let p = swc_files(locale_paths(&values(c, "path-element"), opts, ctx));
                set_list(&mut opts.library_path, append, p);
            }
            "external-library-path" => {
                let p = swc_files(locale_paths(&values(c, "path-element"), opts, ctx));
                set_list(&mut opts.external_library_path, append, p);
            }
            "include-libraries" => {
                let p = swc_files(locale_paths(&values(c, "library"), opts, ctx));
                set_list(&mut opts.flex.include_libraries, append, p);
            }
            "namespaces" => {
                let ns: Vec<(String, Vec<PathBuf>)> = children(c, "namespace").map(|e| (children(e, "uri").next().map(text).unwrap_or_default(), values(e, "manifest").iter().map(|m| ctx.path(m)).collect())).collect();
                set_list(&mut opts.css.namespaces, append, ns);
            }
            "theme" => set_list(&mut opts.css.theme, append, values(c, "filename").iter().map(|f| ctx.path(f)).collect()),
            "defaults-css-url" => opts.css.defaults_css_url = Some(ctx.path(&text(c))),
            "keep-as3-metadata" => {
                let names = values(c, "name");
                let k = opts.flex.keep_as3_metadata.get_or_insert_with(Vec::new);
                set_list(k, append, names);
            }
            "compatibility-version" => {
                let v: Vec<i32> = text(c).split('.').map(|p| p.parse().unwrap_or(0)).collect();
                opts.css.compatibility_version = Some((v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0), v.get(2).copied().unwrap_or(0)));
                if v.first().copied().unwrap_or(0) <= 3 {
                    opts.css.qualified_type_selectors = false;
                }
            }
            "define" => {
                let name = children(c, "name").next().map(text).unwrap_or_default();
                let value = children(c, "value").next().map(text).unwrap_or_default();
                if !append {
                    opts.defines.clear();
                }
                opts.defines.push((name, value));
            }
            "context-root" => opts.flex.context_root = Some(text(c)),
            "fonts" => load_fonts(opts, c, ctx),
            "preloader" => opts.flex.preloader = Some(text(c)),
            "locale" => {}
            n if n.starts_with("warn-") => opts.flex.warn.push((n["warn-".len()..].to_string(), is_true(c))),
            _ => {}
        }
    }
}

/// `compiler.fonts.*` into `FontsConfiguration`.
fn load_fonts(opts: &mut ApplicationOptions, n: roxmltree::Node, ctx: &Ctx) {
    let f = &mut opts.flex.fonts;
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "flash-type" | "advanced-anti-aliasing" => f.flash_type = is_true(c),
            "max-cached-fonts" => f.max_cached_fonts = Some(text(c)),
            "max-glyphs-per-face" => f.max_glyphs_per_face = Some(text(c)),
            "managers" => {
                let v = values(c, "manager-class");
                set_list(f.managers.get_or_insert_with(Vec::new), appends(c), v);
            }
            "languages" => {
                for r in children(c, "language-range") {
                    let lang = children(r, "lang").next().map(text).unwrap_or_default();
                    let range = children(r, "range").next().map(text).unwrap_or_default();
                    f.set_language_range(&lang, &range);
                }
            }
            "local-fonts-snapshot" => {
                let p = ctx.path(&text(c));
                if p.exists() {
                    f.local_fonts_snapshot = Some(crate::asc::flex2::compiler::transcoder::local_name(&p));
                }
            }
            "local-font-paths" => {
                let v: Vec<String> = values(c, "path-element").iter().map(|e| ctx.path(e)).filter(|p| p.exists()).map(|p| crate::asc::flex2::compiler::transcoder::local_name(&p)).collect();
                let l = f.local_font_paths.get_or_insert_with(Vec::new);
                if !appends(c) {
                    l.clear();
                }
                l.extend(v);
            }
            _ => {}
        }
    }
}
