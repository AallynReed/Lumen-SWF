//! Port of the style parts of `flex2.tools.PreLink`: `locateStyleDefaults`, the `StylesContainer` work of
//! `processMainUnit`, the root's validation and `lookupBackgroundColor` from `postRun`, and the `codegenFlexInit`
//! pieces that come from styles.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::compiler_api::CompilerApi;
use crate::asc::flex2::compiler::css::css_file::CssFile;
use crate::asc::flex2::compiler::css::document::{CssType, CssTypeTable};
use crate::asc::flex2::compiler::css::style_def::GLOBAL;
use crate::asc::flex2::compiler::css::styles_container::{GeneratedSource, StyleSource, StylesContainer};
use crate::asc::flex2::compiler::source::{Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::compiler::util::name_mappings::{get_name_mappings, NameMappings};
use crate::asc::flex2::flash::css::style_declaration::PropertyValue;
use crate::asc::flex2::flash::css::style_selector::CLASS_CONDITION;
use crate::asc::util::jhash::{JHashSet, JavaHash};

thread_local! {
    /// The style sources of the compile's `ResourceContainer`, by name.
    static RESOURCES: RefCell<HashMap<String, SourceRef>> = RefCell::new(HashMap::new());
    static NAME_MAPPINGS: RefCell<Option<Rc<NameMappings>>> = const { RefCell::new(None) };
}

/// A new compile's `ResourceContainer` and `NameMappings`.
pub fn reset() {
    RESOURCES.with(|r| r.borrow_mut().clear());
    NAME_MAPPINGS.with(|m| *m.borrow_mut() = None);
}

/// `CompilerAPI.getNameMappings(configuration)`, made once per compile.
pub fn name_mappings(api: &CompilerApi) -> Rc<NameMappings> {
    if let Some(m) = NAME_MAPPINGS.with(|m| m.borrow().clone()) {
        return m;
    }
    let m = Rc::new(get_name_mappings(&api.configuration.css.namespaces));
    NAME_MAPPINGS.with(|n| *n.borrow_mut() = Some(m.clone()));
    m
}

/// `mxml.reflect.TypeTable` as `StylesContainer.validate` makes it (`new TypeTable(symbolTable, nameMappings,
/// standardDefs, themeNames)`), for the `[Style]` and `[Exclude]` lookups of type selectors.
struct SymbolTableTypes<'a> {
    symbol_table: &'a crate::asc::flex2::compiler::symbol_table::SymbolTable,
    theme_names: &'a [String],
}

/// `TypeTable.TypeHelper`: a class's inherited `[Style]` and `[Exclude]` metadata.
struct TypeHelper {
    styles: Vec<crate::asc::semantics::meta_data::MetaData>,
    excludes: Vec<crate::asc::semantics::meta_data::MetaData>,
    theme_names: Vec<String>,
}

impl TypeHelper {
    /// `hasTheme(value)`: any of `value.split("[, ]")` is an active theme.
    fn has_theme(&self, value: &str) -> bool {
        value.split([',', ' ']).any(|t| self.theme_names.iter().any(|n| n == t))
    }
}

impl CssType for TypeHelper {
    fn has_style(&self, name: &str) -> bool {
        if self.is_excluded_style(name) {
            return false;
        }
        self.styles.iter().any(|md| md.get_value_key("name").as_deref() == Some(name) && md.get_value_key("theme").is_none_or(|t| self.has_theme(&t)))
    }

    fn get_style_themes(&self, name: &str) -> Option<String> {
        self.styles.iter().find(|md| md.get_value_key("name").as_deref() == Some(name)).and_then(|md| md.get_value_key("theme"))
    }

    fn is_excluded_style(&self, name: &str) -> bool {
        self.excludes.iter().any(|md| md.get_value_key("name").as_deref() == Some(name) && md.get_value_key("kind").as_deref() == Some("style"))
    }

    fn has_property(&self, _name: &str) -> bool {
        true
    }
}

impl CssTypeTable for SymbolTableTypes<'_> {
    fn get_type(&self, class_name: &str) -> Option<Rc<dyn CssType>> {
        let class_info = self.symbol_table.get_class(Some(class_name))?;
        let lookup = |n: &str| self.symbol_table.get_class(Some(n));
        Some(Rc::new(TypeHelper { styles: class_info.get_meta_data("Style", true, &lookup), excludes: class_info.get_meta_data("Exclude", true, &lookup), theme_names: self.theme_names.to_vec() }))
    }
}

/// A file a `HashSet<VirtualFile>` holds by its name's hash and its identity.
#[derive(Clone)]
struct FileKey(CssFile);

impl PartialEq for FileKey {
    fn eq(&self, o: &FileKey) -> bool {
        self.0 == o.0
    }
}

impl Eq for FileKey {}

impl JavaHash for FileKey {
    fn java_hash(&self) -> i32 {
        self.0.get_name().java_hash()
    }
}

/// `locateStyleDefaults(units, compilerConfiguration)`.
fn locate_style_defaults(units: &[UnitRef], sc: &StylesContainer) {
    let mut defaults_css_files: JHashSet<FileKey> = JHashSet::new();
    let version_defaults_css_file_name = sc.get_mxml_configuration().compatibility_version_string.as_ref().map(|v| format!("defaults-{v}.css"));
    for unit in units {
        let source = unit.borrow().get_source();
        let location = source.borrow().swc_script().map(|s| s.get_swc_location());
        let Some(location) = location else { continue };
        let probe = |entry: &str| CssFile::Zip { swc: location.clone(), entry: entry.to_string() };
        let has = |entry: &str| zip_has(&location, entry);
        let mut f = None;
        if let Some(v) = &version_defaults_css_file_name {
            if has(v) {
                f = Some(probe(v));
            }
        }
        if f.is_none() && has("defaults.css") {
            f = Some(probe("defaults.css"));
        }
        if let Some(f) = f {
            defaults_css_files.insert(FileKey(f));
        }
    }
    sc.add_defaults_css_files(defaults_css_files.iter().map(|k| k.0.clone()).collect());
}

fn zip_has(swc: &str, entry: &str) -> bool {
    std::fs::File::open(swc).ok().and_then(|f| zip::ZipArchive::new(f).ok()).is_some_and(|mut z| z.by_name(entry).is_ok())
}

/// `createSource` then `resources.addResource(source)`.
fn add_resource(g: &GeneratedSource) -> SourceRef {
    let file = VirtualFile::InMemory { bytes: Rc::new(g.code.as_bytes().to_vec()), name: g.name.clone(), mime_type: crate::asc::flex2::compiler::util::mime_mappings::AS, last_modified: g.last_modified };
    let source = Source::new(file, None, "", &g.short_name, Owner::ResourceContainer, false, false, false);
    for i in &g.includes {
        source.borrow_mut().add_file_include(i.clone());
    }
    RESOURCES.with(|r| r.borrow_mut().insert(g.name.clone(), source.clone()));
    source
}

/// The styles half of `processMainUnit` for the root unit `u`; true when it generated a new style source.
pub fn process_main_unit_styles(api: &mut CompilerApi) -> bool {
    let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
    let Some(u) = units.iter().find(|u| u.borrow().is_root()).cloned() else { return false };
    if u.borrow().top_level_definitions.last().is_none() {
        return false;
    }
    let (loader_class, loader_class_base) = {
        let ub = u.borrow();
        (ub.loader_class.clone(), ub.loader_class_base.clone())
    };
    if loader_class.is_none() && loader_class_base.is_none() {
        return false;
    }
    let sc = match api.symbol_table.styles_container.clone() {
        Some(sc) => sc,
        None => {
            let swcs = api.swc_context.get_swcs();
            let config = api.configuration.css.css_configuration(api.configuration.debug, api.configuration.generated_directory.clone(), swcs);
            let sc = Rc::new(StylesContainer::new(Rc::new(config), u.clone(), api.symbol_table.per_compile_data.clone()));
            sc.set_name_mappings(Some(name_mappings(api)));
            api.symbol_table.styles_container = Some(sc.clone());
            sc
        }
    };
    let css_files_set: JHashSet<String> = {
        let mut s = JHashSet::new();
        for f in sc.get_defaults_css_files() {
            s.insert(f.get_name());
        }
        s
    };
    locate_style_defaults(&units, &sc);
    let added_css_files_set = if css_files_set.is_empty() {
        None
    } else {
        let mut added = JHashSet::new();
        for f in sc.get_defaults_css_files() {
            let n = f.get_name();
            if !css_files_set.contains(&n) {
                added.insert(n);
            }
        }
        Some(added)
    };
    sc.load_default_styles();
    let class_names = api.symbol_table.get_class_names();
    let theme_names = sc.get_mxml_configuration().theme_names.clone();
    let types = SymbolTableTypes { symbol_table: &api.symbol_table, theme_names: &theme_names };
    sc.validate(Some(&types), &class_names, &theme_names, added_css_files_set.as_ref());
    let mut def_names = Vec::new();
    for unit in &units {
        for d in unit.borrow().top_level_definitions.iter() {
            def_names.push(d.to_string());
        }
    }
    let (package_name, class_name) = {
        let s = u.borrow().get_source();
        let s = s.borrow();
        (s.get_relative_path().replace('/', "."), s.get_short_name().to_string())
    };
    let existing = |name: &str| -> Option<bool> { RESOURCES.with(|r| r.borrow().get(name).map(|s| s.borrow().get_compilation_unit().is_some())) };
    let (generated, style_sources) = sc.process_dependencies(&def_names, &existing, Some(&package_name), Some(&class_name));
    let style_sources: Vec<SourceRef> = style_sources
        .iter()
        .filter_map(|s| match s {
            StyleSource::New(g) => Some(add_resource(g)),
            StyleSource::Existing(name) => RESOURCES.with(|r| r.borrow().get(name).cloned()),
        })
        .collect();
    let names: Vec<String> = style_sources.iter().map(|s| s.borrow().get_name().to_string()).collect();
    let initial_source_count = api.sources.len();
    api.sources.retain(|s| !names.iter().any(|n| n == s.borrow().get_name()));
    api.sources.extend(style_sources);
    generated || api.sources.len() > initial_source_count
}

/// `Integer.decode`.
fn integer_decode(s: &str) -> Option<i32> {
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (radix, digits) = if let Some(r) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")).or_else(|| rest.strip_prefix('#')) {
        (16, r)
    } else if rest.starts_with('0') && rest.len() > 1 {
        (8, &rest[1..])
    } else {
        (10, rest)
    };
    if digits.is_empty() || digits.starts_with(['-', '+']) {
        return None;
    }
    let v = i64::from_str_radix(digits, radix).ok()?;
    i32::try_from(if neg { -v } else { v }).ok()
}

fn decode_background_color(value: &PropertyValue, path: &str, line: i32) -> Option<i32> {
    let PropertyValue::Str(s) = value else { return None };
    match integer_decode(s) {
        Some(v) => Some(v),
        None => {
            toolkit::log(CompilerMessage::error(format!("Invalid background color: {s}")), Some(path), line, -1, None);
            None
        }
    }
}

/// `lookupTypeSelectorBackgroundColor(stylesContainer, className)`.
fn lookup_type_selector_background_color(sc: &StylesContainer, class_name: &str) -> i32 {
    let mut result = -1;
    let Some(style_def) = sc.get_style_def(class_name) else { return result };
    let sd = style_def.borrow();
    for (_, decl) in sd.get_declarations().into_iter().flatten() {
        let d = decl.borrow();
        for block in d.get_declaration_blocks() {
            if let Some(p) = block.borrow().get_properties().get("backgroundColor") {
                if let Some(v) = decode_background_color(p.get_value(), d.get_path(), p.get_line_number()) {
                    result = v;
                }
            }
        }
    }
    result
}

/// `lookupClassSelectorBackgroundColor(stylesContainer, styleName)`.
fn lookup_class_selector_background_color(sc: &StylesContainer, style_name: &str) -> i32 {
    let mut result = -1;
    let Some(style_def) = sc.get_style_def(GLOBAL) else { return result };
    let sd = style_def.borrow();
    for (_, decl) in sd.get_declarations().into_iter().flatten() {
        let d = decl.borrow();
        let Some(selector) = d.get_selector() else { continue };
        let conditions = selector.borrow().get_conditions().cloned().unwrap_or_default();
        for c in conditions {
            if c.get_kind() == CLASS_CONDITION && c.get_value() == Some(style_name) {
                for block in d.get_declaration_blocks() {
                    if let Some(p) = block.borrow().get_properties().get("backgroundColor") {
                        if let Some(v) = decode_background_color(p.get_value(), d.get_path(), p.get_line_number()) {
                            result = v;
                        }
                    }
                }
            }
        }
    }
    result
}

/// `lookupStyleName(stylesContainer, className)`.
fn lookup_style_name(sc: &StylesContainer, class_name: &str) -> Option<String> {
    let mut result = None;
    let style_def = sc.get_style_def(class_name)?;
    let sd = style_def.borrow();
    for (_, decl) in sd.get_declarations().expect("NullPointerException") {
        for block in decl.borrow().get_declaration_blocks() {
            if let Some(p) = block.borrow().get_properties().get("styleName") {
                if let PropertyValue::Str(s) = p.get_value() {
                    result = Some(s.clone());
                }
            }
        }
    }
    result
}

/// `lookupBackgroundColor(global, local, inlineStyleName, className, symbolTable, configuration)`.
pub fn lookup_background_color(api: &mut CompilerApi, global: Option<&StylesContainer>, local: Option<&StylesContainer>, inline_style_name: Option<String>, class_name: &str) {
    let mut style_name = inline_style_name;
    if style_name.is_none() {
        if let Some(l) = local {
            style_name = lookup_style_name(l, class_name);
        }
    }
    if style_name.is_none() {
        if let Some(g) = global {
            style_name = lookup_style_name(g, class_name);
        }
    }
    let mut background_color = -1;
    if let (Some(n), Some(l)) = (&style_name, local) {
        background_color = lookup_class_selector_background_color(l, n);
    }
    if background_color == -1 {
        if let (Some(n), Some(g)) = (&style_name, global) {
            background_color = lookup_class_selector_background_color(g, n);
        }
    }
    if background_color == -1 {
        if let Some(l) = local {
            background_color = lookup_type_selector_background_color(l, class_name);
        }
    }
    if background_color == -1 {
        if let Some(g) = global {
            background_color = lookup_type_selector_background_color(g, class_name);
        }
    }
    if background_color == -1 {
        if let Some(abc_class) = api.symbol_table.get_class(Some(&name_formatter::to_colon(class_name))) {
            match &abc_class.super_type_name {
                Some(super_type_name) => {
                    let s = name_formatter::to_dot(super_type_name);
                    lookup_background_color(api, global, local, None, &s);
                }
                None => {
                    if let Some(l) = local {
                        background_color = lookup_type_selector_background_color(l, GLOBAL);
                    }
                    if background_color == -1 {
                        if let Some(g) = global {
                            background_color = lookup_type_selector_background_color(g, GLOBAL);
                        }
                    }
                }
            }
        }
    }
    if background_color != -1 {
        api.configuration.background_color = background_color;
    }
}

/// The root unit's styles part of `postRun`: the background color from CSS when `[SWF]` gave none, and the root's own
/// container validated and dropped.
pub fn post_run_styles(api: &mut CompilerApi) {
    let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
    let Some(u) = units.iter().find(|u| u.borrow().is_root()).cloned() else { return };
    let global = api.symbol_table.styles_container.clone();
    let root_container = u.borrow().styles_container.clone();
    let has_bg = u.borrow().swf_meta_data.as_ref().is_some_and(|md| md.get_value_key("backgroundColor").is_some());
    if !has_bg {
        let last = u.borrow().top_level_definitions.last().map(|q| q.to_string());
        if let Some(def) = last {
            let style_name = u.borrow().style_name.clone();
            lookup_background_color(api, global.as_deref(), root_container.as_deref(), style_name, &name_formatter::to_dot(&def));
        }
    }
    if let Some(rc) = &root_container {
        let class_names = api.symbol_table.get_class_names();
        let theme_names = rc.get_mxml_configuration().theme_names.clone();
        let types = SymbolTableTypes { symbol_table: &api.symbol_table, theme_names: &theme_names };
        rc.validate(Some(&types), &class_names, &theme_names, None);
    }
    u.borrow_mut().styles_container = None;
}

/// `codegenInheritingStyleRegistration(inheritingStyles)`, over `symbolTable` styles as `processInitClass` gathers them.
pub fn codegen_inheriting_style_registration(inheriting_styles: &[String]) -> String {
    let mut sb = String::from("       var styleNames:Array = [");
    let quoted: Vec<String> = inheriting_styles.iter().map(|s| format!("\"{s}\"")).collect();
    sb.push_str(&quoted.join(", "));
    sb.push_str("];\n\n");
    sb.push_str("       for (var i:int = 0; i < styleNames.length; i++)\n");
    sb.push_str("       {\n");
    sb.push_str("          styleManager.registerInheritingStyle(styleNames[i]);\n");
    sb.push_str("       }\n");
    sb
}

/// The `inheritingStyles` `processInitClass` collects: every unit's inheriting styles, in a `HashSet`.
pub fn inheriting_styles(units: &[UnitRef]) -> Vec<String> {
    let mut set: JHashSet<String> = JHashSet::new();
    for u in units {
        for s in u.borrow().styles.get_inheriting_styles().iter() {
            set.insert(s.clone());
        }
    }
    set.iter().cloned().collect()
}

/// `codegenQualifiedTypeSelectors(configuration)`.
pub fn codegen_qualified_type_selectors(qualified: bool) -> &'static str {
    if qualified {
        ""
    } else {
        "       styleManager.qualifiedTypeSelectors = false;\n"
    }
}
