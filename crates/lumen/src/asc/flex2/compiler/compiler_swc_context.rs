//! Port of `flex2.compiler.CompilerSwcContext`: the definitions of the libraries on the library and external library
//! paths, turned into `Source`s on demand.

use std::rc::Rc;

use super::source::{Owner, Source, SourceRef, VirtualFile};
use super::swc::{Swc, SwcGroup, SwcScript, EXPRESSION, INHERITANCE, NAMESPACE, SIGNATURE};
use super::toolkit::{self, Level};
use super::util::{mime_mappings, MultiName, Name, QName};
use crate::asc::util::jhash::{JHashMap, JHashSet};

const SWC_PLAYERGLOBAL: &str = "playerglobal.swc";
const SWC_AIRGLOBAL: &str = "airglobal.swc";
const SWC_AVMPLUS: &str = "avmplus.swc";

#[derive(Default)]
pub struct CompilerSwcContext {
    swc_group: Option<SwcGroup>,
    def2source: JHashMap<QName, SourceRef>,
    def2script: JHashMap<QName, Rc<SwcScript>>,
    package_names: JHashSet<String>,
    externs: JHashSet<String>,
    /// `rb2file`/`rb2source`: the libraries' `locale/<locale>/*.properties` bundles.
    pub resource_bundles: super::i18n::resource_file::SwcResourceBundles,
    /// `ts`: the modification times of the libraries and themes, for `checksum()`.
    ts: String,
    /// The group's libraries in `SwcGroup.swcs` order, the themes' last.
    file_swcs: Vec<Rc<Swc>>,
    /// `SwcGroup.files`: each catalog file's newest copy, as (library, entry) indices into `file_swcs`.
    files: JHashMap<String, ThemeStyle>,
    /// `themeStyles`: the CSS themes and the style sheets in theme libraries.
    theme_styles: Vec<ThemeStyle>,
}

/// A library file, or a theme style sheet: a CSS file or a `.css` entry of a theme library.
#[derive(Clone)]
pub enum ThemeStyle {
    Local(String),
    Zip(usize, usize),
}

impl CompilerSwcContext {
    pub fn new() -> CompilerSwcContext {
        CompilerSwcContext::default()
    }

    /// `load(libPath, rslPath, ...)`: `rsl_path` holds the external libraries, whose definitions become externs.
    pub fn load(&mut self, lib_path: &[std::path::PathBuf], rsl_path: &[std::path::PathBuf]) -> usize {
        self.load_with(lib_path, rsl_path, Vec::new())
    }

    /// `load`, plus libraries already in memory, external ones (`true`) contributing externs.
    pub fn load_with(&mut self, lib_path: &[std::path::PathBuf], rsl_path: &[std::path::PathBuf], extra: Vec<(Rc<Swc>, bool)>) -> usize {
        let mut swcs: Vec<Rc<Swc>> = Vec::new();
        let mut rsl_locations = Vec::new();
        for (swc, external) in extra {
            if external {
                rsl_locations.push(swc.location.clone());
                for lib in &swc.libraries {
                    for s in lib.get_scripts() {
                        for d in s.get_definitions() {
                            self.externs.insert(d.clone());
                        }
                    }
                }
            }
            swcs.push(swc);
        }
        for (paths, rsl) in [(lib_path, false), (rsl_path, true)] {
            for p in paths {
                match Swc::load(p) {
                    Ok(swc) => {
                        if rsl {
                            rsl_locations.push(swc.location.clone());
                            for lib in &swc.libraries {
                                for s in lib.get_scripts() {
                                    for d in s.get_definitions() {
                                        self.externs.insert(d.clone());
                                    }
                                }
                            }
                        }
                        if !swcs.iter().any(|s| s.location == swc.location) {
                            swcs.push(swc);
                        }
                    }
                    Err(e) => toolkit::log_plain(Level::Error, &e),
                }
            }
        }
        if swcs.is_empty() {
            return 0;
        }
        self.ts.clear();
        let mut groups: Vec<Rc<Swc>> = Vec::new();
        for paths in [lib_path, rsl_path] {
            let mut group: Vec<Rc<Swc>> = Vec::new();
            for p in paths {
                let loc = super::source::canonical_path(p);
                if let Some(swc) = swcs.iter().find(|s| s.location == loc) {
                    if !group.iter().any(|g| Rc::ptr_eq(g, swc)) {
                        group.push(swc.clone());
                    }
                }
            }
            for swc in &group {
                self.ts.push_str(&swc.last_modified.to_string());
                if !groups.iter().any(|g| Rc::ptr_eq(g, swc)) {
                    groups.push(swc.clone());
                }
            }
        }
        for swc in &swcs {
            if !groups.iter().any(|g| Rc::ptr_eq(g, swc)) {
                groups.push(swc.clone());
            }
        }
        self.file_swcs = groups;
        self.files = Self::swc_files(&self.file_swcs);
        let group = SwcGroup::new(swcs, &rsl_locations);
        crate::asc::flex2::compiler::util::name_mappings::set_swc_components(&group.swcs);
        super::css::css_file::set_swcs(group.swcs.clone());
        let files: Vec<(String, String, i64)> = self.files.iter().filter_map(|(n, f)| match f {
            ThemeStyle::Zip(i, e) => Some((n.clone(), self.file_swcs[*i].location.clone(), self.file_swcs[*i].entry_time(&self.file_swcs[*i].entries[*e]))),
            ThemeStyle::Local(_) => None,
        }).collect();
        self.resource_bundles.update(&files);
        for (def, script) in group.def2script.iter() {
            let q = QName::parse(def);
            self.package_names.insert(q.get_namespace().to_string());
            self.def2script.insert(q, script.clone());
        }
        let n = group.swcs.len();
        self.swc_group = Some(group);
        n
    }

    /// `load`'s theme half: CSS themes and theme libraries, whose files join the group.
    pub fn load_themes(&mut self, theme_files: &[std::path::PathBuf]) {
        for t in theme_files {
            let name = super::source::canonical_path(t);
            if name.ends_with(".css") {
                self.ts.push_str(&VirtualFile::local(t).get_last_modified().to_string());
                self.theme_styles.push(ThemeStyle::Local(name));
            } else if let Ok(swc) = Swc::load(t) {
                let index = match self.file_swcs.iter().position(|s| s.location == swc.location) {
                    Some(i) => i,
                    None => {
                        self.file_swcs.push(swc);
                        self.file_swcs.len() - 1
                    }
                };
                let swc = self.file_swcs[index].clone();
                for (_, f) in Self::swc_files(&[swc.clone()]).iter() {
                    let ThemeStyle::Zip(_, e) = f else { continue };
                    self.ts.push_str(&swc.entry_time(&swc.entries[*e]).to_string());
                    if swc.entries[*e].name.ends_with(".css") {
                        self.theme_styles.push(ThemeStyle::Zip(index, *e));
                    }
                }
            }
        }
        self.files = Self::swc_files(&self.file_swcs);
    }

    /// `SwcGroup.updateFiles()`: per library, its archive's files in `HashMap` order; a name keeps its newest file.
    fn swc_files(swcs: &[Rc<Swc>]) -> JHashMap<String, ThemeStyle> {
        let mut files: JHashMap<String, ThemeStyle> = JHashMap::new();
        for (i, swc) in swcs.iter().enumerate() {
            let mut archive: JHashMap<String, usize> = JHashMap::new();
            for (e, entry) in swc.entries.iter().enumerate() {
                archive.insert(entry.name.clone(), e);
            }
            for (name, e) in archive.iter() {
                let t = swc.entry_time(&swc.entries[*e]);
                let newer = match files.get(name.as_str()) {
                    Some(&ThemeStyle::Zip(ci, ce)) => t > swcs[ci].entry_time(&swcs[ci].entries[ce]),
                    _ => true,
                };
                if newer {
                    files.insert(name.clone(), ThemeStyle::Zip(i, *e));
                }
            }
        }
        files
    }

    /// `getFiles()` with each file's `getLastModified()`.
    pub fn get_files(&self) -> Vec<(String, i64)> {
        self.files.iter().map(|(n, f)| (n.clone(), self.file_time(f).1)).collect()
    }

    fn file_time(&self, t: &ThemeStyle) -> (String, i64) {
        match t {
            ThemeStyle::Local(n) => (n.clone(), VirtualFile::local(std::path::Path::new(n)).get_last_modified()),
            ThemeStyle::Zip(i, e) => {
                let swc = &self.file_swcs[*i];
                (format!("{}${}", swc.location, swc.entries[*e].name), swc.entry_time(&swc.entries[*e]))
            }
        }
    }

    /// `getThemeStyleSheets()` as `getName()` and `getLastModified()`.
    pub fn get_theme_style_sheets(&self) -> Vec<(String, i64)> {
        self.theme_styles.iter().map(|t| self.file_time(t)).collect()
    }

    /// `SwcChecksums.areSwcFileChecksumsEqual` puts the theme style sheets into `getFiles()`'s map.
    pub fn add_theme_style_sheets_to_files(&mut self) {
        for t in self.theme_styles.clone() {
            let (name, _) = self.file_time(&t);
            self.files.insert(name, t);
        }
    }

    /// `close()`: the archives are read again on next use, with new clock milliseconds in their entry times.
    pub fn close(&self) {
        for swc in &self.file_swcs {
            swc.reopen();
        }
    }

    /// `checksum()`.
    pub fn checksum(&self) -> i32 {
        super::configuration_buffer::calculate_checksum(&self.ts)
    }

    /// `getChecksum(def)`: the defining script's signature checksum.
    pub fn get_checksum(&self, def: &QName) -> Option<i64> {
        self.def2script.get(def).and_then(|s| s.get_signature_checksum())
    }

    /// The defining script of `def`, as `def2script` has it.
    pub fn get_script(&self, def: &QName) -> Option<Rc<SwcScript>> {
        self.def2script.get(def).cloned()
    }

    pub fn get_source(&mut self, namespace_uri: &str, local_part: &str) -> Option<SourceRef> {
        let key = QName::new(namespace_uri, local_part);
        if let Some(s) = self.def2source.get(&key) {
            return Some(s.clone());
        }
        let script = self.def2script.get(&key)?.clone();
        let s = create_source(&script);
        for d in script.get_definitions() {
            self.def2source.insert(QName::parse(d), s.clone());
        }
        Some(s)
    }

    pub fn get_externs(&self) -> Vec<String> {
        self.externs.iter().cloned().collect()
    }

    pub fn has_package(&self, package_name: &str) -> bool {
        self.package_names.contains(package_name)
    }

    pub fn has_definition(&self, q_name: &QName) -> bool {
        self.def2script.contains_key(q_name)
    }

    pub fn get_swcs(&self) -> Vec<Rc<Swc>> {
        self.swc_group.as_ref().map(|g| g.swcs.clone()).unwrap_or_default()
    }
}

fn create_source(script: &Rc<SwcScript>) -> SourceRef {
    let loc = script.get_swc_location();
    let f = VirtualFile::InMemory {
        bytes: script.get_abc(),
        name: script.to_string_name(),
        mime_type: mime_mappings::ABC,
        last_modified: script.get_last_modified(),
    };
    let internal = loc.ends_with(SWC_PLAYERGLOBAL) || loc.ends_with(SWC_AIRGLOBAL) || loc.ends_with(SWC_AVMPLUS);
    let s = Source::new(f, None, "", "", Owner::SwcScript(script.clone()), internal, false, false);
    let u = Source::new_compilation_unit(&s, None);
    {
        let mut u = u.borrow_mut();
        u.signature_checksum = script.get_signature_checksum();
        for d in script.get_definitions() {
            u.top_level_definitions.add(QName::parse(d));
        }
        let set = script.get_dependency_set();
        for d in set.get_dependencies(INHERITANCE) {
            u.inheritance.insert(Name::M(MultiName::parse(&d)));
        }
        for d in set.get_dependencies(SIGNATURE) {
            u.types.insert(Name::M(MultiName::parse(&d)));
        }
        for d in set.get_dependencies(NAMESPACE) {
            u.namespaces.insert(Name::M(MultiName::parse(&d)));
        }
        for d in set.get_dependencies(EXPRESSION) {
            u.expressions.insert(Name::M(MultiName::parse(&d)));
        }
        for d in script.get_symbol_classes() {
            u.expressions.insert(Name::M(MultiName::parse(&d)));
        }
        let lib = script.get_library();
        for d in script.get_definitions() {
            if let Some(tag) = lib.get_symbol(d) {
                u.assets.add(d, super::assets::AssetInfo::from_tag(tag));
            }
        }
    }
    s
}
