//! Ports of the parts of `flex2.compiler.swc` mxmlc reads a library through: `Swc` (catalog only), `SwcLibrary`,
//! `SwcScript`, `SwcDependencySet` and `SwcGroup`'s definition map.

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::io::Read;
use std::rc::{Rc, Weak};

use crate::asc::util::jhash::{java_compare, JHashMap, JHashSet};

pub const INHERITANCE: &str = "i";
pub const NAMESPACE: &str = "n";
pub const SIGNATURE: &str = "s";
pub const EXPRESSION: &str = "e";

#[derive(Default)]
pub struct SwcDependencySet {
    dep_type_map: JHashMap<String, JHashSet<String>>,
}

impl SwcDependencySet {
    pub fn add_dependency(&mut self, type_: &str, dep: &str) {
        if !self.dep_type_map.contains_key(type_) {
            self.dep_type_map.insert(type_.to_string(), JHashSet::new());
        }
        self.dep_type_map.get_mut(type_).unwrap().insert(dep.to_string());
    }

    pub fn get_dependencies(&self, type_: &str) -> Vec<String> {
        self.dep_type_map.get(type_).map(|s| s.iter().cloned().collect()).unwrap_or_default()
    }
}

pub struct SwcScript {
    library: Weak<SwcLibrary>,
    name: String,
    /// A `TreeSet`, in Java string order.
    defs: Vec<String>,
    deps: SwcDependencySet,
    modtime: i64,
    abc: RefCell<Option<Rc<Vec<u8>>>>,
    symbol_classes: RefCell<Option<JHashSet<String>>>,
}

impl SwcScript {
    pub fn get_library(&self) -> Rc<SwcLibrary> {
        self.library.upgrade().expect("SwcLibrary dropped")
    }

    pub fn get_swc_location(&self) -> String {
        self.get_library().swc_location.clone()
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_last_modified(&self) -> i64 {
        self.modtime
    }

    pub fn get_definitions(&self) -> &[String] {
        &self.defs
    }

    pub fn get_dependency_set(&self) -> &SwcDependencySet {
        &self.deps
    }

    /// `getSymbolClasses()`: the classes of the library symbols this script's symbols refer to.
    pub fn get_symbol_classes(&self) -> Vec<String> {
        if self.symbol_classes.borrow().is_none() {
            let mut set = JHashSet::new();
            let lib = self.get_library();
            for d in &self.defs {
                lib.get_symbol_classes(d, &mut set);
            }
            *self.symbol_classes.borrow_mut() = Some(set);
        }
        self.symbol_classes.borrow().as_ref().unwrap().iter().cloned().collect()
    }

    pub fn get_abc(&self) -> Rc<Vec<u8>> {
        self.get_library().parse();
        self.abc.borrow().clone().expect("no DoABC for the script")
    }

    pub fn to_string_name(&self) -> String {
        format!("{}({})", self.get_swc_location(), self.defs.join(", "))
    }
}

pub struct SwcLibrary {
    pub swc_location: String,
    pub path: String,
    zip_path: String,
    scripts: RefCell<Vec<Rc<SwcScript>>>,
    metadata: RefCell<JHashSet<String>>,
    parsed: Cell<bool>,
    def2symbol: RefCell<Option<JHashMap<String, crate::asc::flex2::flash::swf::tags::TagRef>>>,
}

impl SwcLibrary {
    pub fn get_scripts(&self) -> Vec<Rc<SwcScript>> {
        self.scripts.borrow().clone()
    }

    pub fn get_metadata(&self) -> Vec<String> {
        self.metadata.borrow().iter().cloned().collect()
    }

    /// `parse()`: attaches each `DoABC2` of the library SWF to its script by name.
    fn parse(&self) {
        if self.parsed.get() {
            return;
        }
        self.parsed.set(true);
        let Ok(bytes) = read_zip_entry(&self.zip_path, &self.path) else { return };
        let Ok(swf) = crate::swf::Swf::parse(&bytes) else { return };
        let scripts = self.scripts.borrow();
        for t in &swf.timeline.tags {
            if t.code != crate::tags::code::DO_ABC2 || t.body.len() < 4 {
                continue;
            }
            let rest = &t.body[4..];
            let Some(nul) = rest.iter().position(|&b| b == 0) else { continue };
            let name = String::from_utf8_lossy(&rest[..nul]);
            if let Some(s) = scripts.iter().find(|s| s.name == name) {
                *s.abc.borrow_mut() = Some(Rc::new(rest[nul + 1..].to_vec()));
            }
        }
        let mut def2symbol = JHashMap::new();
        for (class_name, tag) in crate::asc::flex2::flash::swf::tag_decoder::decode_lenient(&bytes).symbol_classes {
            def2symbol.insert(class_name, tag);
        }
        *self.def2symbol.borrow_mut() = Some(def2symbol);
    }

    /// `getSymbol(name)`: the asset tag a `SymbolClass` gives the definition.
    pub fn get_symbol(&self, name: &str) -> Option<crate::asc::flex2::flash::swf::tags::TagRef> {
        self.def2symbol.borrow().as_ref().and_then(|m| m.get(&name.replace(':', ".")).cloned())
    }

    /// `getSymbolClasses(def, symbolClasses)` through `getReferencedSymbolClasses`.
    fn get_symbol_classes(&self, def: &str, out: &mut JHashSet<String>) {
        let t = self.get_symbol(def);
        let mut visited = crate::asc::flex2::flash::swf::dictionary::TagMap::<()>::default();
        self.referenced_symbol_classes(t, out, &mut visited);
    }

    fn referenced_symbol_classes(&self, t: Option<crate::asc::flex2::flash::swf::tags::TagRef>, out: &mut JHashSet<String>, visited: &mut crate::asc::flex2::flash::swf::dictionary::TagMap<()>) {
        let Some(t) = t else { return };
        if visited.contains(&t) {
            return;
        }
        visited.put(&t, ());
        let refs = t.borrow().get_references();
        for r in refs {
            if let Some(m) = self.def2symbol.borrow().as_ref() {
                for (class_name, tag) in m.iter() {
                    if Rc::ptr_eq(tag, &r) {
                        out.insert(super::util::name_formatter::to_colon(class_name));
                    }
                }
            }
            self.referenced_symbol_classes(Some(r), out, visited);
        }
    }
}

pub struct Swc {
    pub location: String,
    pub last_modified: i64,
    pub libraries: Vec<Rc<SwcLibrary>>,
}

fn read_zip_entry(zip_path: &str, name: &str) -> std::io::Result<Vec<u8>> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(std::io::Error::other)?;
    let mut e = z.by_name(name).map_err(std::io::Error::other)?;
    let mut v = Vec::new();
    e.read_to_end(&mut v)?;
    Ok(v)
}

impl Swc {
    /// Reads `catalog.xml` (`CatalogReader`); the library SWFs are parsed on first use.
    pub fn load(path: &std::path::Path) -> Result<Rc<Swc>, String> {
        let location = super::source::canonical_path(path);
        let last_modified = super::source::VirtualFile::local(path).get_last_modified();
        let catalog = read_zip_entry(&location, "catalog.xml").map_err(|e| format!("{location}: {e}"))?;
        let text = String::from_utf8_lossy(&catalog);
        let doc = roxmltree::Document::parse(&text).map_err(|e| format!("{location}: {e}"))?;
        let version_1_0 = doc
            .descendants()
            .find(|n| n.has_tag_name("swc") && n.parent().is_some_and(|p| p.has_tag_name("versions")))
            .and_then(|n| n.attribute("version"))
            .is_some_and(|v| v == "1.0");
        let mut libraries = Vec::new();
        for lib in doc.descendants().filter(|n| n.has_tag_name("library")) {
            let path = lib.attribute("path").unwrap_or("library.swf").to_string();
            let library = Rc::new(SwcLibrary {
                swc_location: location.clone(),
                path,
                zip_path: location.clone(),
                scripts: RefCell::new(Vec::new()),
                metadata: RefCell::new(JHashSet::new()),
                parsed: Cell::new(false),
                def2symbol: RefCell::new(None),
            });
            for child in lib.children().filter(|n| n.is_element()) {
                if child.has_tag_name("script") {
                    let name = child.attribute("name").unwrap_or("").to_string();
                    let modtime = child.attribute("mod").and_then(|m| m.parse().ok()).unwrap_or(0);
                    let mut defs: Vec<String> = Vec::new();
                    let mut deps = SwcDependencySet::default();
                    for d in child.children().filter(|n| n.is_element()) {
                        if d.has_tag_name("def") {
                            let id = d.attribute("id").unwrap_or("");
                            let id = if version_1_0 { super::util::name_formatter::to_colon(id) } else { id.to_string() };
                            if !defs.contains(&id) {
                                defs.push(id);
                            }
                        } else if d.has_tag_name("dep") {
                            deps.add_dependency(d.attribute("type").unwrap_or(""), d.attribute("id").unwrap_or(""));
                        }
                    }
                    defs.sort_by(|a, b| java_compare(a, b));
                    let script = Rc::new(SwcScript {
                        library: Rc::downgrade(&library),
                        name,
                        defs,
                        deps,
                        modtime,
                        abc: RefCell::new(None),
                        symbol_classes: RefCell::new(None),
                    });
                    library.scripts.borrow_mut().push(script);
                } else if child.has_tag_name("keep-as3-metadata") {
                    for m in child.children().filter(|n| n.has_tag_name("metadata")) {
                        if let Some(n) = m.attribute("name") {
                            library.metadata.borrow_mut().insert(n.to_string());
                        }
                    }
                }
            }
            libraries.push(library);
        }
        Ok(Rc::new(Swc { location, last_modified, libraries }))
    }
}

impl Swc {
    /// A library whose scripts are given directly, with their ABC already attached.
    pub fn from_abc_blocks(location: &str, scripts: Vec<(String, Vec<String>, SwcDependencySet, Vec<u8>)>) -> Rc<Swc> {
        let library = Rc::new(SwcLibrary {
            swc_location: location.to_string(),
            path: String::new(),
            zip_path: String::new(),
            scripts: RefCell::new(Vec::new()),
            metadata: RefCell::new(JHashSet::new()),
            parsed: Cell::new(true),
            def2symbol: RefCell::new(None),
        });
        for (name, mut defs, deps, abc) in scripts {
            defs.sort_by(|a, b| java_compare(a, b));
            let script = Rc::new(SwcScript { library: Rc::downgrade(&library), name, defs, deps, modtime: 0, abc: RefCell::new(Some(Rc::new(abc))), symbol_classes: RefCell::new(None) });
            library.scripts.borrow_mut().push(script);
        }
        Rc::new(Swc { location: location.to_string(), last_modified: 0, libraries: vec![library] })
    }
}

/// `SwcGroup`: a definition maps to the newest script that defines it.
pub struct SwcGroup {
    pub swcs: Vec<Rc<Swc>>,
    pub def2script: JHashMap<String, Rc<SwcScript>>,
}

impl SwcGroup {
    pub fn new(swcs: Vec<Rc<Swc>>, rsl_locations: &[String]) -> SwcGroup {
        let mut scripts: Vec<Rc<SwcScript>> = Vec::new();
        for swc in &swcs {
            for lib in &swc.libraries {
                scripts.extend(lib.get_scripts());
            }
        }
        scripts.sort_by(|a, b| {
            if a.modtime == b.modtime {
                let a_rsl = rsl_locations.contains(&a.get_swc_location());
                let b_rsl = rsl_locations.contains(&b.get_swc_location());
                match (a_rsl, b_rsl) {
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    _ => Ordering::Equal,
                }
            } else if a.modtime < b.modtime {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        });
        let mut def2script: JHashMap<String, Rc<SwcScript>> = JHashMap::new();
        for s in scripts {
            if s.defs.iter().any(|d| def2script.contains_key(d.as_str())) {
                continue;
            }
            for d in &s.defs {
                def2script.insert(d.clone(), s.clone());
            }
        }
        SwcGroup { swcs, def2script }
    }
}
