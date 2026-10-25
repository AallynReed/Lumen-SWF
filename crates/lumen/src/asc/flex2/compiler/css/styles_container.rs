//! Port of `flex2.compiler.css.StylesContainer`: an application's style declarations (SWC defaults, `-defaults-css-url`,
//! themes, and a document's `<fx:Style>`), validated and turned into the `_<App>_Styles` / `_FontFaceRules` sources.

use std::cell::RefCell;
use std::rc::Rc;

use super::at_embed::{AtEmbed, AtEmbedKey};
use super::css_file::{self, CssFile};
use super::document::{CssConfiguration, CssDocument, CssType, CssTypeTable, VERSION_3_0};
use super::import::Import;
use super::style_def::{dehyphenize, StyleDef, StyleDefRef, GLOBAL, UNIVERSAL};
use super::style_module::{StyleModule, StyleModuleOps};
use super::template_context;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::flash::css::style_declaration::{PropertyValue, StyleDeclarationRef};
use crate::asc::flex2::flash::css::style_selector::MediaList;
use crate::asc::flex2::flash::css::style_sheet::StyleSheet;
use crate::asc::util::context::ContextStatics;
use crate::asc::util::jhash::JHashSet;

const TEMPLATE_PATH: &str = "flex2/compiler/css/";
const FONT_FACE_RULES: &str = "_FontFaceRules";

/// A generated ActionScript source, for the caller to wrap in a `Source` (`createSource`): `new Source(new
/// TextFile(code, name, null, MimeMappings.AS, lastModified), "", shortName, null, false, false, false)` with the root's
/// path resolver and `includes` as its file includes.
#[derive(Clone, Debug)]
pub struct GeneratedSource {
    pub name: String,
    pub short_name: String,
    pub code: String,
    pub last_modified: i64,
    pub includes: Vec<String>,
}

/// A source `processDependencies` hands back: a new one, or the one already in the `ResourceContainer`.
#[derive(Clone, Debug)]
pub enum StyleSource {
    New(GeneratedSource),
    Existing(String),
}

/// `StylesContainer.StyleDefList`.
#[derive(Clone)]
pub struct StyleDefList {
    pub style_defs: Vec<StyleDefRef>,
}

impl StyleDefList {
    pub fn is_advanced(&self) -> bool {
        self.style_defs.iter().any(|d| d.borrow().is_advanced())
    }

    pub fn get_allow_duplicate_default_style_declarations(&self) -> bool {
        self.style_defs.iter().any(|d| d.borrow().get_allow_duplicate_default_style_declarations())
    }

    pub fn get_imports(&self) -> JHashSet<Import> {
        let mut result = JHashSet::new();
        for d in &self.style_defs {
            for i in d.borrow().get_imports().iter() {
                result.insert(i.clone());
            }
        }
        result
    }

    pub fn get_at_embeds(&self) -> JHashSet<AtEmbedKey> {
        let mut result = JHashSet::new();
        for d in &self.style_defs {
            for e in d.borrow().get_at_embeds().iter() {
                result.insert(e.clone());
            }
        }
        result
    }

    fn same_as(&self, other: &[StyleDefRef]) -> bool {
        self.style_defs.len() == other.len() && self.style_defs.iter().zip(other).all(|(a, b)| Rc::ptr_eq(a, b))
    }
}

pub struct StylesContainer {
    module: RefCell<StyleModule>,
    mxml_document: RefCell<Option<Rc<dyn CssDocument>>>,
    mxml_document_qname: RefCell<Option<String>>,
    mxml_configuration: Rc<CssConfiguration>,
    compilation_unit: UnitRef,
    local_style_type_names: RefCell<JHashSet<String>>,
    implicit_includes: RefCell<Vec<CssFile>>,
    last_style_def_list: RefCell<Option<Vec<StyleDefRef>>>,
    /// `CompilerConfiguration.defaultsCssFiles`, which `PreLink` prepends to on every pass.
    defaults_css_files: RefCell<Vec<CssFile>>,
}

impl StyleModuleOps for StylesContainer {
    fn module(&self) -> std::cell::Ref<'_, StyleModule> {
        self.module.borrow()
    }

    fn module_mut(&self) -> std::cell::RefMut<'_, StyleModule> {
        self.module.borrow_mut()
    }

    fn add_selector_to_style_def(&self, subject: &str, declaration: &StyleDeclarationRef, is_type_selector: bool, is_local: bool, line_number: i32) {
        if is_type_selector && self.has_non_root_type_selectors(subject, subject, line_number) {
            return;
        }
        if is_type_selector && is_local {
            self.local_style_type_names.borrow_mut().insert(subject.to_string());
        }
        let existing = self.module().get_style_def(subject);
        let style_def = match existing {
            Some(d) => d,
            None => {
                let (source, pcd) = self.source_and_data();
                let d = StyleDef::new_legacy(subject.to_string(), is_type_selector, self.mxml_document.borrow().clone(), Some(self.mxml_configuration.clone()), source, line_number, pcd);
                self.module_mut().put_style_def(subject.to_string(), d.clone());
                d
            }
        };
        style_def.borrow_mut().add_declaration(declaration);
        self.add_document_imports(&style_def);
    }

    fn add_advanced_selector_to_style_def(&self, declaration: &StyleDeclarationRef, media_list: Option<Rc<MediaList>>, is_local: bool, line_number: i32) {
        let subject = declaration.borrow().get_subject().unwrap_or("null").to_string();
        let selector = declaration.borrow().get_selector().expect("NullPointerException");
        let selector_string = selector.borrow().to_string();
        if self.has_non_root_type_selectors(&subject, &selector_string, line_number) {
            return;
        }
        let mut style_def_key = subject.clone();
        if subject == UNIVERSAL {
            style_def_key = GLOBAL.to_string();
            if selector.borrow().get_conditions().is_some_and(|c| !c.is_empty()) {
                selector.borrow_mut().set_value(Some(String::new()));
            }
        }
        let existing = self.module().get_style_def(&style_def_key);
        let style_def = match existing {
            Some(d) => d,
            None => {
                if is_local && style_def_key != GLOBAL {
                    self.local_style_type_names.borrow_mut().insert(subject.clone());
                }
                let (source, pcd) = self.source_and_data();
                let d = StyleDef::new_advanced(subject, self.mxml_document.borrow().clone(), Some(self.mxml_configuration.clone()), source, line_number, pcd);
                self.module_mut().put_style_def(style_def_key, d.clone());
                d
            }
        };
        style_def.borrow_mut().add_advanced_declaration(declaration, media_list);
        self.add_document_imports(&style_def);
    }

    fn add_at_embed(&self, at_embed: Rc<AtEmbed>) {
        if let Some(doc) = self.mxml_document.borrow().clone() {
            doc.add_at_embed(at_embed);
            return;
        }
        let mut m = self.module_mut();
        let key = at_embed.get_prop_name().to_string();
        if !m.at_embeds.iter().any(|(k, _)| *k == key) {
            m.at_embeds.push((key, at_embed));
        }
    }
}

impl StylesContainer {
    /// `new StylesContainer(mxmlConfiguration, compilationUnit, perCompileData)`.
    pub fn new(mxml_configuration: Rc<CssConfiguration>, compilation_unit: UnitRef, per_compile_data: Rc<RefCell<ContextStatics>>) -> StylesContainer {
        let source = compilation_unit.borrow().get_source();
        let mut module = StyleModule::new(source, per_compile_data);
        if mxml_configuration.compatibility_version <= VERSION_3_0 {
            module.set_advanced(false);
            module.set_qualified_type_selectors(false);
        } else {
            module.set_qualified_type_selectors(mxml_configuration.qualified_type_selectors);
        }
        StylesContainer {
            module: RefCell::new(module),
            mxml_document: RefCell::new(None),
            mxml_document_qname: RefCell::new(None),
            compilation_unit,
            local_style_type_names: RefCell::new(JHashSet::new()),
            implicit_includes: RefCell::new(Vec::new()),
            last_style_def_list: RefCell::new(None),
            defaults_css_files: RefCell::new(mxml_configuration.defaults_css_files.clone()),
            mxml_configuration,
        }
    }

    fn source_and_data(&self) -> (crate::asc::flex2::compiler::source::SourceRef, Rc<RefCell<ContextStatics>>) {
        let m = self.module();
        (m.source.clone(), m.per_compile_data.clone())
    }

    fn add_document_imports(&self, style_def: &StyleDefRef) {
        if let Some(doc) = self.mxml_document.borrow().clone() {
            for i in style_def.borrow().get_imports().iter() {
                doc.add_import(i.get_value(), i.get_line_number());
            }
        }
    }

    pub fn set_name_mappings(&self, mappings: Option<Rc<NameMappings>>) {
        self.module_mut().set_name_mappings(mappings);
    }

    pub fn get_mxml_document(&self) -> Option<Rc<dyn CssDocument>> {
        self.mxml_document.borrow().clone()
    }

    /// `setMxmlDocument(doc)`: an MXML document's container, before its `<fx:Style>` blocks are extracted.
    pub fn set_mxml_document(&self, doc: Option<Rc<dyn CssDocument>>) {
        if let Some(d) = &doc {
            *self.mxml_document_qname.borrow_mut() = Some(d.get_qname());
        }
        *self.mxml_document.borrow_mut() = doc;
    }

    /// `extractStyles(styleSheet, isLocal)`, the `<fx:Style>` entry point.
    pub fn extract_styles(&self, style_sheet: Option<&StyleSheet>, is_local: bool) -> Result<(), Option<String>> {
        StyleModuleOps::extract_styles(self, style_sheet, is_local)
    }

    pub fn get_style_defs(&self) -> Vec<StyleDefRef> {
        self.module().get_style_defs()
    }

    pub fn get_style_def(&self, name: &str) -> Option<StyleDefRef> {
        self.module().get_style_def(name)
    }

    pub fn get_font_face_rules(&self) -> Vec<crate::asc::flex2::flash::css::rule::RuleRef> {
        self.module().get_font_face_rules().to_vec()
    }

    pub fn is_advanced(&self) -> bool {
        self.module().is_advanced()
    }

    pub fn get_qualified_type_selectors(&self) -> bool {
        self.module().get_qualified_type_selectors()
    }

    pub fn get_at_embeds(&self) -> JHashSet<AtEmbedKey> {
        self.module().get_at_embeds()
    }

    pub fn get_imports(&self) -> JHashSet<Import> {
        self.module().get_imports()
    }

    /// `processDependencies(extraSources, defNames, resources, packageName, className)`. `existing(name)` is
    /// `resources.findSource(name)`: `None` when absent, `Some(has_unit)` otherwise. Returns whether a new style
    /// source was generated, and the sources to add.
    pub fn process_dependencies(&self, def_names: &[String], existing: &dyn Fn(&str) -> Option<bool>, package_name: Option<&str>, class_name: Option<&str>) -> (bool, Vec<StyleSource>) {
        let mut extra_sources = Vec::new();
        let Some(class_name) = class_name else { return (false, extra_sources) };
        let mut regenerated_style_source = false;
        if !self.module().font_face_rules.is_empty() {
            {
                let mut u = self.compilation_unit.borrow_mut();
                u.extra_classes.insert(FONT_FACE_RULES.to_string());
                u.mixins.insert(FONT_FACE_RULES.to_string());
            }
            match self.generate_font_face_rules(existing) {
                Some(s) => extra_sources.push(StyleSource::New(s)),
                None if existing(&self.generate_font_face_rule_source_name()) == Some(true) => extra_sources.push(StyleSource::Existing(self.generate_font_face_rule_source_name())),
                None => {}
            }
        }
        let qualified = self.module().qualified_type_selectors;
        let mut processed_def_names = JHashSet::new();
        for def_name in def_names {
            if qualified {
                processed_def_names.insert(name_formatter::to_dot(def_name));
            } else {
                processed_def_names.insert(match def_name.rfind(':') {
                    Some(_) => replace_first_through_colon(def_name),
                    None => def_name.clone(),
                });
            }
        }
        let mut filtered = StyleDefList { style_defs: Vec::new() };
        for (style_name, style_def) in self.module().style_defs.clone() {
            let type_name = dehyphenize_key(&style_name);
            if !style_def.borrow().is_type_selector() || processed_def_names.contains(&type_name) || self.mxml_configuration.keep_all_type_selectors || style_name == GLOBAL {
                filtered.style_defs.push(style_def);
            }
        }
        if !filtered.style_defs.is_empty() {
            regenerated_style_source = true;
            let class_name = format!("_{class_name}_Styles");
            {
                let mut u = self.compilation_unit.borrow_mut();
                u.extra_classes.insert(class_name.clone());
                u.mixins.insert(class_name.clone());
            }
            let gen_file_name = self.generate_style_source_name(package_name, &class_name);
            if existing(&gen_file_name) == Some(true) {
                if let Some(last) = self.last_style_def_list.borrow().as_ref() {
                    if filtered.same_as(last) {
                        regenerated_style_source = false;
                    }
                }
            }
            *self.last_style_def_list.borrow_mut() = Some(filtered.style_defs.clone());
            if regenerated_style_source {
                if let Some(s) = self.generate_style_source(&filtered, &class_name, &gen_file_name) {
                    extra_sources.push(StyleSource::New(s));
                }
            } else {
                extra_sources.push(StyleSource::Existing(gen_file_name));
            }
        }
        (regenerated_style_source, extra_sources)
    }

    fn has_non_root_type_selectors(&self, subject: &str, selector: &str, line_number: i32) -> bool {
        if !self.compilation_unit.borrow().is_root() && subject != UNIVERSAL {
            let path = self.module().source.borrow().get_name().to_string();
            toolkit::log(CompilerMessage::warning(format!("CSS type selectors are not supported in components: '{selector}'")), Some(&path), line_number, -1, None);
            return true;
        }
        false
    }

    /// `validate(symbolTable, nameMappings, standardDefs, themeNames, addedCSSFiles)`. `class_names` is
    /// `symbolTable.getClassNames()`, `type_table` the MXML `TypeTable`.
    pub fn validate(&self, type_table: Option<&dyn CssTypeTable>, class_names: &[String], theme_names: &[String], added_css_files: Option<&JHashSet<String>>) {
        let qualified = self.module().qualified_type_selectors;
        let class_names: JHashSet<String> = if qualified {
            JHashSet::from_collection(class_names.iter().cloned(), class_names.len())
        } else {
            let mut s = JHashSet::new();
            for c in class_names {
                s.insert(replace_first_through_colon(c));
            }
            s
        };
        let theme_names_string = theme_names.join(", ");
        for (style_name, style_def) in self.module().style_defs.clone() {
            let type_name = dehyphenize_key(&style_name);
            let sd = style_def.borrow();
            let Some(declarations) = sd.get_declarations() else { continue };
            for (_, style_declaration) in declarations {
                let decl = style_declaration.borrow();
                for block in decl.get_declaration_blocks() {
                    let style_properties: Vec<_> = block.borrow().get_properties().values().cloned().collect();
                    if added_css_files.is_none_or(|a| a.contains(decl.get_path())) {
                        if sd.is_type_selector() && qualified && self.mxml_configuration.show_invalid_css_property_warnings {
                            if let Some(t) = type_table.and_then(|tt| tt.get_type(&name_formatter::to_colon(&type_name))) {
                                self.validate_type_selector_properties(&style_properties, t.as_ref(), &type_name, &theme_names_string);
                            }
                        }
                        if let Some(qname) = self.mxml_document_qname.borrow().as_ref() {
                            if let Some(t) = type_table.and_then(|tt| tt.get_type(qname)) {
                                validate_property_references(&style_properties, t.as_ref());
                            }
                        }
                    }
                }
            }
            if self.local_style_type_names.borrow().contains(&style_name) && !class_names.contains(&name_formatter::to_colon(&type_name)) && style_name != GLOBAL && self.mxml_configuration.show_unused_type_selector_warnings {
                let path = self.get_path_for_reporting(&sd);
                toolkit::log(CompilerMessage::warning(format!("The CSS type selector '{style_name}' was not processed, because the type was not used in the application.")), Some(&path), sd.get_line_number(), -1, None);
            }
        }
    }

    fn validate_type_selector_properties(&self, style_properties: &[Rc<crate::asc::flex2::flash::css::style_declaration::StyleProperty>], t: &dyn CssType, type_name: &str, _theme_names_string: &str) {
        for p in style_properties {
            let name = p.get_name();
            if !t.has_style(name) {
                let style_themes = t.get_style_themes(name);
                if t.is_excluded_style(name) {
                    toolkit::log(CompilerMessage::warning(format!("The style '{name}' is excluded by type '{type_name}'.")), Some(p.get_path()), p.get_line_number(), -1, None);
                } else if let Some(themes) = style_themes {
                    toolkit::log(CompilerMessage::warning(format!("The style '{name}' is only supported by type '{type_name}' with the theme(s) '{themes}'.")), Some(p.get_path()), p.get_line_number(), -1, None);
                } else if self.mxml_document.borrow().is_some() {
                    toolkit::log(CompilerMessage::warning(format!("The style '{name}' was not declared by type '{type_name}'.")), Some(p.get_path()), p.get_line_number(), -1, None);
                }
            }
        }
    }

    fn get_path_for_reporting(&self, style_def: &StyleDef) -> String {
        if style_def.is_advanced() {
            if let Some(d) = style_def.get_declarations().and_then(|d| d.first()) {
                return d.1.borrow().get_path().to_string();
            }
        }
        self.compilation_unit.borrow().get_source().borrow().get_name().to_string()
    }

    fn generate_font_face_rule_source_name(&self) -> String {
        match &self.mxml_configuration.generated_directory {
            Some(d) => format!("{d}{}_FontFaceRules.as", std::path::MAIN_SEPARATOR),
            None => "_FontFaceRules.as".to_string(),
        }
    }

    fn generate_font_face_rules(&self, existing: &dyn Fn(&str) -> Option<bool>) -> Option<GeneratedSource> {
        let gen_file_name = self.generate_font_face_rule_source_name();
        if existing(&gen_file_name) == Some(true) {
            return None;
        }
        let template = format!("{TEMPLATE_PATH}FontFaceRules.vm");
        let entries = self.module().at_embeds.clone();
        let code = template_context::merge(&template, self.mxml_configuration.debug, vec![("atEmbeds", template_context::at_embed_map(&entries))], &self.relative_path())?;
        Some(self.create_source(&gen_file_name, code, i64::MAX))
    }

    fn relative_path(&self) -> String {
        self.compilation_unit.borrow().get_source().borrow().get_relative_path().to_string()
    }

    fn create_source(&self, file_name: &str, code: String, last_modified: i64) -> GeneratedSource {
        if self.mxml_configuration.keep_generated_actionscript {
            if let Err(e) = std::fs::write(file_name, &code) {
                toolkit::log(CompilerMessage::error(format!("Unable to write generated file ({file_name}): {e}.")), None, -1, -1, None);
            }
        }
        let short_name = match file_name.rfind('.') {
            Some(i) => file_name[..i].to_string(),
            None => file_name.to_string(),
        };
        let includes = self.implicit_includes.borrow().iter().map(|f| f.get_name()).collect();
        GeneratedSource { name: file_name.to_string(), short_name, code, last_modified, includes }
    }

    fn generate_style_source(&self, style_def_list: &StyleDefList, class_name: &str, gen_file_name: &str) -> Option<GeneratedSource> {
        let template = format!("{TEMPLATE_PATH}StyleDef.vm");
        let code = template_context::merge(
            &template,
            self.mxml_configuration.debug,
            vec![("styleDefList", template_context::style_def_list(style_def_list)), ("className", template_context::string(class_name))],
            &self.relative_path(),
        )?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        Some(self.create_source(gen_file_name, code, now))
    }

    fn generate_style_source_name(&self, _package_name: Option<&str>, class_name: &str) -> String {
        match &self.mxml_configuration.generated_directory {
            Some(d) => format!("{d}{}{class_name}.as", std::path::MAIN_SEPARATOR),
            None => format!("{class_name}.as"),
        }
    }

    /// `loadDefaultStyles()`: the SWCs' `defaults.css`, then `-defaults-css-url`, then the themes.
    pub fn load_default_styles(&self) {
        let defaults_css_file = self.resolve_defaults_css_file();
        let defaults_css_files = self.defaults_css_files.borrow().clone();
        for swc_defaults_css_file in &defaults_css_files {
            css_file::push_resolver(swc_defaults_css_file);
            self.process_style_sheet(swc_defaults_css_file);
            css_file::remove_resolver(swc_defaults_css_file);
        }
        match &defaults_css_file {
            Some(f) => {
                if !f.is_zip() {
                    self.process_style_sheet(f);
                }
            }
            None => toolkit::log(CompilerMessage::warning("Default CSS file not found."), None, -1, -1, None),
        }
        for theme_css_file in &self.mxml_configuration.theme_css_files {
            css_file::push_resolver(theme_css_file);
            self.process_style_sheet(theme_css_file);
            css_file::remove_resolver(theme_css_file);
        }
    }

    pub fn get_defaults_css_files(&self) -> Vec<CssFile> {
        self.defaults_css_files.borrow().clone()
    }

    /// `CompilerConfiguration.addDefaultsCssFiles(files)`: inserted at the front, duplicates and all.
    pub fn add_defaults_css_files(&self, files: Vec<CssFile>) {
        let mut d = self.defaults_css_files.borrow_mut();
        let rest = std::mem::take(&mut *d);
        *d = files;
        d.extend(rest);
    }

    pub fn get_mxml_configuration(&self) -> &Rc<CssConfiguration> {
        &self.mxml_configuration
    }

    fn resolve_defaults_css_file(&self) -> Option<CssFile> {
        if let Some(f) = &self.mxml_configuration.defaults_css_url {
            return Some(f.clone());
        }
        let mut f = None;
        if let Some(v) = &self.mxml_configuration.compatibility_version_string {
            f = css_file::toolkit_resolve(&format!("defaults-{v}.css"));
        }
        f.or_else(|| css_file::toolkit_resolve("defaults.css"))
    }

    /// `processStyleSheet(cssFile)`.
    pub fn process_style_sheet(&self, css_file: &CssFile) {
        self.implicit_includes.borrow_mut().push(css_file.clone());
        let name = css_file.get_name();
        match css_file.to_byte_array() {
            Ok(bytes) => {
                let mut style_sheet = StyleSheet::new();
                style_sheet.check_deprecation(self.mxml_configuration.show_deprecation_warnings);
                style_sheet.parse(&name, &bytes);
                if let Err(m) = StyleModuleOps::extract_styles(self, Some(&style_sheet), false) {
                    toolkit::log(CompilerMessage::error(m.unwrap_or_else(|| "null".to_string())), Some(&name), -1, -1, None);
                }
            }
            Err(e) => toolkit::log(CompilerMessage::error(e.to_string()), Some(&name), -1, -1, None),
        }
    }
}

/// `StyleDef.dehyphenize(styleName)`, which throws on the `null` key.
fn dehyphenize_key(style_name: &str) -> String {
    if style_name == super::style_module::NULL_KEY {
        panic!("Cannot invoke \"String.indexOf(int)\" because \"string\" is null");
    }
    dehyphenize(style_name)
}

/// `s.replaceFirst(".*:", "")`: everything through the last colon goes.
fn replace_first_through_colon(s: &str) -> String {
    match s.rfind(':') {
        Some(i) => s[i + 1..].to_string(),
        None => s.to_string(),
    }
}

fn validate_property_references(style_properties: &[Rc<crate::asc::flex2::flash::css::style_declaration::StyleProperty>], mxml_document_type: &dyn CssType) {
    for p in style_properties {
        if let PropertyValue::Reference(r) = p.get_value() {
            if !r.is_class_reference() && !mxml_document_type.has_property(r.get_value()) {
                toolkit::log(CompilerMessage::error(format!("{} is not a document property reference.", r.get_value())), Some(p.get_path()), p.get_line_number(), -1, None);
            }
        }
    }
}
