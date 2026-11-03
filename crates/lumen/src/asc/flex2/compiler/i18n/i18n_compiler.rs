//! Port of `flex2.compiler.i18n.I18nCompiler`: a resource bundle `Source` becomes one `ResourceBundle` subclass per
//! locale, compiled by an embedded `As3Compiler` whose unit the bundle's unit mirrors (`Source.transfer*`).

use std::collections::HashMap;
use std::rc::Rc;

use super::i18n_utils::{bundle_name_from_class_name, CLASS_SUFFIX};
use super::property_text::PropertyValue;
use super::property_translation_format::{self, TranslationInfo};
use super::LINE_SEP;
use crate::asc::flex2::compiler::as3::as3_compiler::{As3Compiler, SubCompiler};
use crate::asc::flex2::compiler::compilation_unit::{CompilationUnit, UnitRef};
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::css::at_embed::AtEmbed;
use crate::asc::flex2::compiler::source::{Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, Level};
use crate::asc::flex2::compiler::transcoder::Transcoders;
use crate::asc::flex2::compiler::util::mime_mappings;

pub struct I18nCompiler {
    configuration: Rc<Configuration>,
    asc: As3Compiler,
    pub generated_dir: Option<String>,
    locales: Vec<String>,
    /// The `ascUnit` context attribute (with its `Source`), by unit.
    asc_units: HashMap<usize, (SourceRef, UnitRef)>,
}

/// The `As3Configuration` the constructor gives its `As3Compiler`.
fn asc_configuration(c: &Configuration) -> Configuration {
    let mut a = c.clone();
    a.optimize = true;
    a.verbose_stacktraces = false;
    a.debug = false;
    a.strict = true;
    a.warnings = false;
    a.doc = false;
    a.show_deprecation_warnings = false;
    a.defines = Vec::new();
    a.keep_embed_metadata = false;
    a
}

/// `AtEmbed.codegenEmbedVar()`.
fn codegen_embed_var(e: &AtEmbed) -> String {
    let attrs: Vec<String> = e.get_attributes().iter().map(|(k, v)| format!("{k}=\"{v}\"")).collect();
    format!("    [Embed({})]{LINE_SEP}    private static var {}:{};{LINE_SEP}", attrs.join(", "), e.get_prop_name(), e.get_type())
}

/// `escape(s)`: `StringUtils.formatString` without the quotes.
fn escape(s: &str) -> String {
    let units = crate::asc::util::jstr::to_utf16(s);
    let mut out = String::new();
    for (i, &c) in units.iter().enumerate() {
        match c {
            0x5c => {
                if units.get(i + 1) == Some(&(b'u' as u16)) {
                    out.push('\\');
                } else {
                    out.push_str("\\\\");
                }
            }
            0x22 => out.push_str("\\\""),
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0c => out.push_str("\\f"),
            0x0d => out.push_str("\\r"),
            0x0a => out.push_str("\\n"),
            c if c < 0x20 => out.push_str(&format!("\\x{c}X")),
            c => crate::asc::util::jstr::push_unit(&mut out, c),
        }
    }
    out
}

impl I18nCompiler {
    pub fn new(compiler_config: Rc<Configuration>, transcoders: Transcoders) -> I18nCompiler {
        let mut asc = As3Compiler::new(Rc::new(asc_configuration(&compiler_config)));
        let generated_dir = compiler_config.generated_directory.clone();
        asc.add_compiler_extension(Box::new(crate::asc::flex2::compiler::as3::embed_extension::EmbedExtension::new(transcoders, generated_dir.clone(), compiler_config.show_deprecation_warnings)));
        let locales = compiler_config.flex.locales.clone();
        I18nCompiler { configuration: compiler_config, asc, generated_dir, locales, asc_units: HashMap::new() }
    }

    fn asc_unit(&self, unit: &UnitRef) -> UnitRef {
        self.asc_units.get(&(Rc::as_ptr(unit) as usize)).map(|(_, u)| u.clone()).expect("ascUnit")
    }

    fn compatibility_version(&self) -> i32 {
        self.configuration.css.compatibility_version()
    }

    /// `transform(source, translations, packageName, className, locale)`.
    fn transform_one(&self, translations: &TranslationInfo, class_name: &str, locale: &str) -> String {
        let bundle_name = bundle_name_from_class_name(class_name);
        let l = LINE_SEP;
        [
            self.codegen_imports(&translations.class_references),
            "[ExcludeClass]".into(), l.into(), l.into(),
            "public class ".into(), class_name.into(), " extends ResourceBundle".into(), l.into(),
            "{".into(), l.into(),
            self.codegen_at_embeds(&translations.embeds),
            "    public function ".into(), class_name.into(), "()".into(), l.into(),
            "    {".into(), l.into(),
            self.codegen_super(locale, &bundle_name), l.into(),
            "    }".into(), l.into(), l.into(),
            "    override protected function getContent():Object".into(), l.into(),
            "    {".into(), l.into(),
            "        var content:Object =".into(), l.into(),
            "        {".into(), l.into(),
            self.codegen_content(&translations.translation_set),
            "        };".into(), l.into(),
            "        return content;".into(), l.into(),
            "    }".into(), l.into(),
            "}".into(), l.into(), l.into(),
        ]
        .concat()
    }

    /// `transform(source, translations)`.
    fn transform(&self, source: &SourceRef, translations: &[Option<TranslationInfo>]) -> SourceRef {
        let s = source.borrow();
        let name = s.get_name().to_string();
        let relative_path = s.get_relative_path().to_string();
        let short_name = s.get_short_name().to_string();
        let package_name = relative_path.replace('/', ".");
        let mut code = String::new();
        code.push_str("package ");
        code.push_str(&package_name);
        code.push_str(LINE_SEP);
        code.push('{');
        code.push_str(LINE_SEP);
        code.push_str(LINE_SEP);
        for (i, t) in translations.iter().enumerate() {
            let class_name = format!("{}${short_name}{CLASS_SUFFIX}", self.locales[i]);
            match t {
                Some(t) => code.push_str(&self.transform_one(t, &class_name, &self.locales[i])),
                None => panic!("NullPointerException"),
            }
            code.push_str(LINE_SEP);
            code.push_str(LINE_SEP);
        }
        code.push('}');
        code.push_str(LINE_SEP);
        if let Some(dir) = &self.generated_dir {
            let mut p = std::path::PathBuf::from(dir);
            for part in package_name.split('.').filter(|p| !p.is_empty()) {
                p.push(part);
            }
            let _ = std::fs::create_dir_all(&p);
            p.push(format!("{short_name}{CLASS_SUFFIX}.as"));
            if let Err(e) = std::fs::write(&p, &code) {
                toolkit::log_plain(Level::Error, &e.to_string());
            }
        }
        let last_modified = s.get_last_modified();
        let file = VirtualFile::InMemory { bytes: Rc::new(code.into_bytes()), name, mime_type: mime_mappings::AS, last_modified };
        Source::new(file, s.get_path_root().map(str::to_string), &relative_path, &short_name, s.get_owner().clone(), s.is_internal(), s.is_root(), s.is_debuggable())
    }

    fn codegen_imports(&self, imports: &[String]) -> String {
        let items: Vec<String> = imports.iter().map(|i| format!("import {i};")).collect();
        format!("{}{LINE_SEP}{LINE_SEP}", items.join(LINE_SEP))
    }

    fn codegen_at_embeds(&self, at_embeds: &[Rc<AtEmbed>]) -> String {
        if self.configuration.flex.archive_classes_and_assets {
            return String::new();
        }
        let items: Vec<String> = at_embeds.iter().map(|e| codegen_embed_var(e)).collect();
        format!("{}{LINE_SEP}", items.join(LINE_SEP))
    }

    fn codegen_super(&self, locale: &str, bundle_name: &str) -> String {
        if self.configuration.flex.resource_hack {
            "\t\t super();".to_string()
        } else {
            format!("\t\t super(\"{locale}\", \"{bundle_name}\");")
        }
    }

    fn codegen_content(&self, resources: &[(String, PropertyValue)]) -> String {
        let flex2_compatible = self.compatibility_version() < crate::asc::flex2::compiler::css::document::VERSION_3_0;
        let items: Vec<String> = resources
            .iter()
            .map(|(key, value)| {
                let key = if flex2_compatible { key.clone() } else { escape(key) };
                let value = match value {
                    PropertyValue::ClassReference(c) => c.to_string(),
                    PropertyValue::Embed(e) => {
                        if self.configuration.flex.archive_classes_and_assets {
                            "\"\"".to_string()
                        } else {
                            e.get_prop_name().to_string()
                        }
                    }
                    PropertyValue::Str(s) => format!("\"{}\"", if flex2_compatible { s.clone() } else { escape(s) }),
                };
                format!("            \"{key}\": {value}")
            })
            .collect();
        format!("{}{LINE_SEP}", items.join(&format!(",{LINE_SEP}")))
    }
}

/// `Source.transferInheritance(from, to)`.
fn transfer_inheritance(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.inheritance.clear();
    for n in from.inheritance.iter() {
        to.inheritance.insert(n.clone());
    }
    for (k, v) in from.inheritance_history.iter() {
        to.inheritance_history.put(k.clone(), v.clone());
    }
}

/// `Source.transferDependencies(from, to)`.
fn transfer_dependencies(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.inheritance.clear();
    to.types.clear();
    to.expressions.clear();
    to.namespaces.clear();
    to.import_package_statements.clear();
    to.import_definition_statements.clear();
    for n in from.inheritance.iter() {
        to.inheritance.insert(n.clone());
    }
    for n in from.types.iter() {
        to.types.insert(n.clone());
    }
    for n in from.expressions.iter() {
        to.expressions.insert(n.clone());
    }
    for n in from.namespaces.iter() {
        to.namespaces.insert(n.clone());
    }
    for n in from.import_package_statements.iter() {
        to.import_package_statements.insert(n.clone());
    }
    for n in from.import_definition_statements.iter() {
        to.import_definition_statements.insert(n.clone());
    }
    for (k, v) in from.inheritance_history.iter() {
        to.inheritance_history.put(k.clone(), v.clone());
    }
    for (k, v) in from.type_history.iter() {
        to.type_history.put(k.clone(), v.clone());
    }
    for (k, v) in from.expression_history.iter() {
        to.expression_history.put(k.clone(), v.clone());
    }
    for (k, v) in from.namespace_history.iter() {
        to.namespace_history.put(k.clone(), v.clone());
    }
}

/// `Source.transferMetaData(from, to)`.
fn transfer_meta_data(from: &CompilationUnit, to: &mut CompilationUnit) {
    to.metadata.extend(from.metadata.iter().cloned());
    to.swf_meta_data = from.swf_meta_data.clone();
    to.icon = from.icon.clone();
    to.icon_file = from.icon_file.clone();
    to.signature_checksum = from.signature_checksum;
    to.loader_class = from.loader_class.clone();
    for c in from.extra_classes.iter() {
        to.extra_classes.insert(c.clone());
    }
    if let Some(a) = &from.accessibility_classes {
        let set = to.accessibility_classes.get_or_insert_with(Default::default);
        for c in a.iter() {
            set.insert(c.clone());
        }
    }
    for (k, v) in &from.licensed_class_reqs {
        to.licensed_class_reqs.insert(k.clone(), v.clone());
    }
    for (k, v) in &from.remote_class_aliases {
        to.remote_class_aliases.insert(k.clone(), v.clone());
    }
    for (k, v) in &from.effect_triggers {
        to.effect_triggers.insert(k.clone(), v.clone());
    }
    for m in from.mixins.iter() {
        to.mixins.insert(m.clone());
    }
    for r in from.resource_bundles.iter() {
        to.resource_bundles.insert(r.clone());
    }
    for r in from.resource_bundle_history.iter() {
        to.resource_bundle_history.insert(r.clone());
    }
}

/// `Source.transferGeneratedSources(from, to)`.
fn transfer_generated_sources(from: &mut CompilationUnit, to: &mut CompilationUnit) {
    if let Some(g) = from.get_generated_sources() {
        let g = g.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>();
        let mut m = crate::asc::util::jhash::JHashMap::new();
        for (k, v) in g {
            m.insert(k, v);
        }
        to.add_generated_sources(m);
    }
    from.clear_generated_sources();
}

impl SubCompiler for I18nCompiler {
    fn get_name(&self) -> &'static str {
        "i18n"
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        property_translation_format::is_supported(mime_type)
    }

    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let mut translations: Vec<Option<TranslationInfo>> = Vec::new();
        for l in self.locales.clone() {
            match property_translation_format::get_translation_set(self.compatibility_version(), symbol_table.per_compile_data.clone(), source, &l) {
                Ok(t) => translations.push(Some(t)),
                Err(e) => {
                    toolkit::log_plain(Level::Error, &e);
                    translations.push(None);
                }
            }
        }
        if toolkit::error_count() > 0 {
            return None;
        }
        let trans_source = self.transform(source, &translations);
        if toolkit::error_count() > 0 {
            return None;
        }
        let asc_unit = self.asc.parse1(&trans_source, symbol_table);
        if toolkit::error_count() > 0 {
            return None;
        }
        let asc_unit = asc_unit?;
        let unit = Source::new_compilation_unit(source, None);
        {
            let mut a = asc_unit.borrow_mut();
            let mut u = unit.borrow_mut();
            transfer_meta_data(&a, &mut u);
            transfer_generated_sources(&mut a, &mut u);
            for q in a.top_level_definitions.iter() {
                u.top_level_definitions.add(q.clone());
            }
            transfer_inheritance(&a, &mut u);
        }
        self.asc_units.insert(Rc::as_ptr(&unit) as usize, (trans_source, asc_unit));
        Some(unit)
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = self.asc_unit(unit);
        transfer_inheritance(&unit.borrow(), &mut asc_unit.borrow_mut());
        self.asc.parse2(&asc_unit, symbol_table);
        let mut a = asc_unit.borrow_mut();
        let mut u = unit.borrow_mut();
        if a.has_assets() {
            let assets = std::mem::take(&mut a.assets);
            u.assets.add_all(&assets);
        }
        transfer_generated_sources(&mut a, &mut u);
    }

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = self.asc_unit(unit);
        self.asc.analyze1(&asc_unit, symbol_table);
        let a = asc_unit.borrow();
        let mut u = unit.borrow_mut();
        u.type_info = a.type_info.clone();
        u.namespaces.clear();
        for n in a.namespaces.iter() {
            u.namespaces.insert(n.clone());
        }
        for (k, v) in a.namespace_history.iter() {
            u.namespace_history.put(k.clone(), v.clone());
        }
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = self.asc_unit(unit);
        transfer_dependencies(&unit.borrow(), &mut asc_unit.borrow_mut());
        self.asc.analyze2(&asc_unit, symbol_table);
        transfer_dependencies(&asc_unit.borrow(), &mut unit.borrow_mut());
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = self.asc_unit(unit);
        transfer_dependencies(&unit.borrow(), &mut asc_unit.borrow_mut());
        self.asc.analyze3(&asc_unit, symbol_table);
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = self.asc_unit(unit);
        self.asc.analyze4(&asc_unit, symbol_table);
        let a = asc_unit.borrow();
        let mut u = unit.borrow_mut();
        u.expressions.clear();
        for n in a.expressions.iter() {
            u.expressions.insert(n.clone());
        }
        for (k, v) in a.expression_history.iter() {
            u.expression_history.put(k.clone(), v.clone());
        }
    }

    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let asc_unit = self.asc_unit(unit);
        self.asc.generate(&asc_unit, symbol_table);
        let bytes = asc_unit.borrow().bytes.clone();
        unit.borrow_mut().bytes = bytes;
    }

    fn postprocess(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        self.asc_units.remove(&(Rc::as_ptr(unit) as usize));
    }
}
