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
        let group = SwcGroup::new(swcs, &rsl_locations);
        for (def, script) in group.def2script.iter() {
            let q = QName::parse(def);
            self.package_names.insert(q.get_namespace().to_string());
            self.def2script.insert(q, script.clone());
        }
        let n = group.swcs.len();
        self.swc_group = Some(group);
        n
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
    }
    s
}
