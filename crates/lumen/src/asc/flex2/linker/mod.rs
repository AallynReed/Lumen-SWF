//! Ports of `flex2.linker`: `LinkerAPI.link`, `SimpleMovie`/`FlexMovie` (the frame and `DoABC` layout),
//! `DependencyWalker` (the unit order) and `CULinkable`, plus the parts of `flash.swf.Movie`/`Frame` they fill in.

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::Name;
use crate::asc::util::jhash::{JHashMap, JHashSet};

/// `flash.swf.tags.DoABC`.
#[derive(Clone)]
pub struct DoAbc {
    pub name: String,
    pub flag: i32,
    pub abc: Vec<u8>,
}

/// `flash.swf.Frame`, without assets.
#[derive(Default)]
pub struct Frame {
    pub label: Option<String>,
    pub do_abcs: Vec<DoAbc>,
    pub pos: i32,
}

/// `flash.swf.Movie` as `SimpleMovie` fills it in.
pub struct Movie {
    pub version: i32,
    /// Twips.
    pub width: i32,
    pub height: i32,
    pub framerate: i32,
    pub script_limits: Option<(i32, i32)>,
    pub bgcolor: i32,
    pub enable_debugger: Option<String>,
    pub debug_uuid: Option<[u8; 16]>,
    pub file_attributes: Option<FileAttributes>,
    pub metadata: Option<String>,
    pub product_info: ProductInfo,
    pub frames: Vec<Frame>,
    pub top_level_class: Option<String>,
    pub metadata_to_keep: Vec<String>,
}

#[derive(Clone, Copy, Default)]
pub struct FileAttributes {
    pub use_direct_blit: bool,
    pub use_gpu: bool,
    pub has_metadata: bool,
    pub action_script3: bool,
    pub use_network: bool,
}

pub struct ProductInfo {
    pub product: i32,
    pub edition: i32,
    pub major_version: u8,
    pub minor_version: u8,
    pub build: i64,
    pub compile_date: i64,
}

/// `SimpleMovie.formatSymbolClassName`.
fn format_symbol_class_name(class_name: Option<&str>) -> Option<String> {
    class_name.map(|c| c.replace(':', "."))
}

/// `CULinkable`.
struct CuLinkable {
    unit: UnitRef,
    name: String,
    defs: JHashSet<String>,
    prereqs: JHashSet<String>,
    deps: JHashSet<String>,
    native: bool,
}

impl CuLinkable {
    fn new(unit: &UnitRef) -> CuLinkable {
        let u = unit.borrow();
        let s = u.get_source();
        let (name, native) = {
            let s = s.borrow();
            (s.get_name().to_string(), s.is_internal())
        };
        let mut defs = JHashSet::new();
        for d in u.top_level_definitions.get_string_set() {
            defs.insert(d);
        }
        let mut prereqs = JHashSet::new();
        let mut deps = JHashSet::new();
        let add = |set: &mut JHashSet<String>, names: &JHashSet<Name>| {
            for n in names.iter() {
                if let Name::Q(q) = n {
                    set.insert(q.to_string());
                }
            }
        };
        add(&mut prereqs, &u.inheritance);
        add(&mut deps, &u.expressions);
        add(&mut deps, &u.namespaces);
        add(&mut deps, &u.types);
        for c in u.extra_classes.iter() {
            deps.insert(c.clone());
        }
        for c in u.resource_bundles.iter() {
            deps.insert(c.clone());
        }
        CuLinkable { unit: unit.clone(), name, defs, prereqs, deps, native }
    }
}

/// `DependencyWalker.LinkableContext`.
struct LinkableContext {
    l: CuLinkable,
    pi: Option<(Vec<String>, usize)>,
    di: Option<(Vec<String>, usize)>,
    visited: bool,
    progress: bool,
}

/// `DependencyWalker.LinkState`.
struct LinkState {
    contexts: Vec<LinkableContext>,
    defs: JHashMap<String, usize>,
    extdefs: JHashSet<String>,
    includes: Vec<String>,
    unresolved: JHashSet<String>,
}

#[derive(Debug)]
pub struct LinkerException(pub String);

impl LinkState {
    fn new(linkables: Vec<CuLinkable>, extdefs: JHashSet<String>, includes: Vec<String>, unresolved: JHashSet<String>) -> Result<LinkState, LinkerException> {
        let mut state = LinkState { contexts: Vec::new(), defs: JHashMap::new(), extdefs, includes, unresolved };
        let mut names: JHashSet<String> = JHashSet::new();
        for l in linkables {
            if names.contains(&l.name) {
                return Err(LinkerException(format!("Multiple symbols named '{}'.", l.name)));
            }
            names.insert(l.name.clone());
            let idx = state.contexts.len();
            let mut external: Option<String> = None;
            let defs: Vec<String> = l.defs.iter().cloned().collect();
            for def in defs {
                if let Some(&c) = state.defs.get(&def) {
                    return Err(LinkerException(format!(
                        "Symbol '{}' is multiply defined in {} and {}.",
                        def, l.name, state.contexts[c].l.name
                    )));
                }
                state.defs.insert(def.clone(), idx);
                if state.extdefs.contains(&def) {
                    external = Some(def);
                } else if let Some(ext) = &external {
                    return Err(LinkerException(format!(
                        "Inconsistant linkage in {} - '{}' is marked as extern, but '{}' is not.",
                        l.name, ext, def
                    )));
                }
            }
            state.contexts.push(LinkableContext { l, pi: None, di: None, visited: false, progress: false });
        }
        Ok(state)
    }

    fn resolve(&mut self, name: &str, allow_external: bool, export_includes: bool) -> Option<usize> {
        if allow_external && self.extdefs.contains(name) {
            self.unresolved.insert(name.to_string());
            return None;
        }
        if !export_includes {
            self.includes.retain(|i| i != name);
        }
        match self.defs.get(name).copied() {
            None => {
                self.unresolved.insert(name.to_string());
                Some(usize::MAX)
            }
            Some(lc) => {
                if self.contexts[lc].l.native {
                    self.unresolved.insert(name.to_string());
                    return None;
                }
                if !allow_external && self.extdefs.contains(name) {
                    self.extdefs.remove(name);
                }
                let c = &mut self.contexts[lc];
                if c.pi.is_none() {
                    c.pi = Some((c.l.prereqs.iter().cloned().collect(), 0));
                    c.di = Some((c.l.deps.iter().cloned().collect(), 0));
                }
                Some(lc)
            }
        }
    }
}

fn next_of(it: &mut Option<(Vec<String>, usize)>) -> Option<String> {
    let (v, i) = it.as_mut()?;
    let r = v.get(*i).cloned();
    if r.is_some() {
        *i += 1;
    }
    r
}

/// `DependencyWalker.traverse`; `resolve` returning `usize::MAX` is Java's null for an undefined name.
fn traverse(defs: &[String], state: &mut LinkState, allow_external: bool, export_includes: bool, visit: &mut dyn FnMut(&UnitRef)) -> Result<(), LinkerException> {
    let mut defs: Vec<String> = defs.to_vec();
    if export_includes {
        defs.extend(state.includes.iter().cloned());
    }
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for d in &defs {
        match state.resolve(d, allow_external, export_includes) {
            Some(lc) if lc != usize::MAX => queue.push_back(lc),
            _ => {}
        }
    }
    while let Some(qc) = queue.pop_front() {
        if state.contexts[qc].visited {
            continue;
        }
        state.contexts[qc].progress = true;
        let mut stack = vec![qc];
        while let Some(&c) = stack.last() {
            if state.contexts[c].visited {
                stack.pop();
                continue;
            }
            if let Some(p) = next_of(&mut state.contexts[c].pi) {
                if let Some(prereq) = state.resolve(&p, allow_external, export_includes).filter(|&x| x != usize::MAX) {
                    if state.contexts[prereq].progress {
                        return Err(LinkerException(format!("Prerequisites of {} contain a circular reference.", state.contexts[c].l.name)));
                    }
                    if !state.contexts[prereq].visited {
                        state.contexts[prereq].progress = true;
                        stack.push(prereq);
                    }
                }
                continue;
            }
            let unit = state.contexts[c].l.unit.clone();
            visit(&unit);
            state.contexts[c].visited = true;
            state.contexts[c].progress = false;
            stack.pop();
            while let Some(d) = next_of(&mut state.contexts[c].di) {
                if let Some(dc) = state.resolve(&d, allow_external, export_includes).filter(|&x| x != usize::MAX) {
                    if !state.contexts[dc].visited {
                        queue.push_back(dc);
                    }
                }
            }
        }
    }
    Ok(())
}

/// `NameFormatter.nameFromSource`.
fn name_from_source(unit: &UnitRef) -> String {
    let u = unit.borrow();
    let s = u.get_source();
    let s = s.borrow();
    if let Some(script) = s.swc_script() {
        return script.get_name().to_string();
    }
    u.top_level_definitions.first().map(|q| q.to_string().replace(':', "/").replace('.', "/")).unwrap_or_default()
}

/// `new SimpleMovie(configuration)` + `FlexMovie`'s fields.
fn new_movie(configuration: &Configuration) -> Movie {
    let width = configuration.width.as_deref().and_then(|w| w.parse().ok()).unwrap_or(configuration.default_width);
    let height = configuration.height.as_deref().and_then(|h| h.parse().ok()).unwrap_or(configuration.default_height);
    let version = configuration.swf_version;
    let file_attributes = (version >= 8).then(|| FileAttributes {
        use_direct_blit: configuration.use_direct_blit,
        use_gpu: configuration.use_gpu,
        has_metadata: configuration.metadata.is_some(),
        action_script3: version >= 9,
        use_network: configuration.use_network,
    });
    let compile_date = configuration.compile_date.unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
    });
    Movie {
        version,
        width: width * 20,
        height: height * 20,
        framerate: configuration.frame_rate,
        script_limits: Some((configuration.script_recursion_limit, configuration.script_time_limit)),
        bgcolor: configuration.background_color,
        enable_debugger: configuration.debug.then(|| "NO-PASSWORD".to_string()),
        debug_uuid: configuration.debug.then(random_uuid),
        file_attributes,
        metadata: if version >= 8 { configuration.metadata.clone() } else { None },
        product_info: ProductInfo {
            product: 3,
            edition: 6,
            major_version: configuration.flex_major,
            minor_version: configuration.flex_minor,
            build: configuration.flex_build,
            compile_date,
        },
        frames: Vec::new(),
        top_level_class: None,
        metadata_to_keep: Vec::new(),
    }
}

/// `FlashUUID`: random bytes, which comparisons mask.
fn random_uuid() -> [u8; 16] {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b
}

/// `LinkerAPI.link(units, postLink, configuration)` with a `FlexMovie`.
pub fn link(units: &[UnitRef], configuration: &Configuration) -> Result<Movie, ()> {
    let mut movie = new_movie(configuration);
    let root_class = configuration.root_class_name.clone().or_else(|| configuration.main_definition.clone());
    movie.top_level_class = format_symbol_class_name(root_class.as_deref());
    match flex_movie_generate(&mut movie, units, configuration) {
        Ok(()) => {}
        Err(e) => {
            toolkit::log(CompilerMessage::error(e.0), None, -1, -1, None);
            return Err(());
        }
    }
    if configuration.optimize && !configuration.debug {
        crate::asc::flex2::tools::post_link::run(&mut movie, configuration);
    }
    Ok(movie)
}

fn flex_movie_generate(movie: &mut Movie, units: &[UnitRef], configuration: &Configuration) -> Result<(), LinkerException> {
    let main_def = configuration.main_definition.clone().unwrap_or_default();
    let mut def2unit: std::collections::HashMap<String, UnitRef> = std::collections::HashMap::new();
    for u in units {
        for d in u.borrow().top_level_definitions.get_string_set() {
            def2unit.insert(d, u.clone());
        }
    }
    let mut frame_info_list: Vec<(String, Vec<String>)> = Vec::new();
    build_frames(&def2unit, &main_def, &mut Vec::new(), &mut frame_info_list)?;
    if let Some((_, classes)) = frame_info_list.first() {
        movie.top_level_class = format_symbol_class_name(Some(&classes[0]));
    }

    let mut externs: JHashSet<String> = JHashSet::new();
    for e in &configuration.externs {
        externs.insert(e.clone());
    }
    let mut linkables = Vec::new();
    for u in units {
        let l = CuLinkable::new(u);
        if l.native {
            for d in u.borrow().top_level_definitions.get_string_set() {
                externs.insert(d);
            }
        }
        linkables.push(l);
    }
    let includes = configuration.includes.clone();
    let mut state = LinkState::new(linkables, externs, includes, JHashSet::new())?;
    let lazy_init = configuration.lazy_init;
    let mut metadata: Vec<String> = Vec::new();
    let mut libraries_processed: Vec<String> = Vec::new();
    let count = frame_info_list.len();
    for (n, (label, frame_classes)) in frame_info_list.iter().enumerate() {
        let mut f = Frame { label: Some(label.clone()), do_abcs: Vec::new(), pos: n as i32 + 1 };
        let last = n + 1 == count;
        traverse(frame_classes, &mut state, last, last, &mut |unit: &UnitRef| {
            let (internal, swc) = {
                let u = unit.borrow();
                let s = u.get_source();
                let s = s.borrow();
                (s.is_internal(), s.swc_script().cloned())
            };
            if internal {
                return;
            }
            let name = name_from_source(unit);
            let flag = if lazy_init { 1 } else { 0 };
            f.do_abcs.push(DoAbc { name, flag, abc: unit.borrow().get_byte_codes().to_vec() });
            if let Some(script) = swc {
                let lib = script.get_library();
                let key = format!("{}|{}", lib.swc_location, lib.path);
                if !libraries_processed.contains(&key) {
                    libraries_processed.push(key);
                    for m in lib.get_metadata() {
                        if !metadata.contains(&m) {
                            metadata.push(m);
                        }
                    }
                }
            }
        })?;
        movie.frames.push(f);
    }
    let mut fatal = false;
    for u in state.unresolved.iter() {
        if !state.extdefs.contains(u) {
            toolkit::log(CompilerMessage::error(format!("'{u}' is undefined.")), None, -1, -1, None);
            fatal = true;
        }
    }
    if fatal {
        return Err(LinkerException("Encountered fatal link errors.".to_string()));
    }
    movie.metadata_to_keep = metadata;
    Ok(())
}

fn build_frames(
    def2unit: &std::collections::HashMap<String, UnitRef>,
    class_name: &str,
    progress: &mut Vec<String>,
    frame_info_list: &mut Vec<(String, Vec<String>)>,
) -> Result<(), LinkerException> {
    if frame_info_list.iter().any(|(_, classes)| classes.iter().any(|c| c == class_name)) {
        return Ok(());
    }
    if progress.iter().any(|p| p == class_name) {
        return Ok(());
    }
    progress.push(class_name.to_string());
    let Some(unit) = def2unit.get(class_name) else {
        return Err(LinkerException(format!("'{class_name}' is undefined.")));
    };
    let loader = unit.borrow().loader_class.clone();
    if let Some(loader) = loader {
        build_frames(def2unit, &loader, progress, frame_info_list)?;
    }
    let label: String = class_name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let mut classes = vec![class_name.to_string()];
    let u = unit.borrow();
    classes.extend(u.resource_bundles.iter().cloned());
    classes.extend(u.extra_classes.iter().cloned());
    frame_info_list.push((label, classes));
    Ok(())
}
