//! Port of the batch compilation in `flex2.compiler.CompilerAPI`: `compile`, `batch`/`batch2`, the `nextSource`
//! scheduler and the multiname resolution that pulls dependencies in from the source path and the libraries.
//! Only `batch2` is ported (mxmlc never uses a `FileSpec`); incremental validation and persistence are left out.

use std::collections::HashMap;
use std::rc::Rc;

use super::as3::as3_compiler::SubCompiler;
use super::compilation_unit::{self, UnitRef};
use super::compiler_swc_context::CompilerSwcContext;
use super::configuration::Configuration;
use super::source::SourceRef;
use super::source_path::{SourceList, SourcePath};
use super::symbol_table::SymbolTable;
use super::toolkit::{self, CompilerMessage, LocalLogger};
use super::util::{MultiName, Name, QName};
use crate::asc::util::jhash::JHashSet;

thread_local! {
    /// Allows a source to define several public definitions, as one script of an existing SWF may.
    pub static SEVERAL_DEFINITIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

const PREPROCESS: i32 = 1 << 1;
const PARSE1: i32 = 1 << 2;
const PARSE2: i32 = 1 << 3;
const ANALYZE1: i32 = 1 << 4;
const ANALYZE2: i32 = 1 << 5;
const ANALYZE3: i32 = 1 << 6;
const ANALYZE4: i32 = 1 << 7;
const RESOLVE_TYPE: i32 = 1 << 10;
const GENERATE: i32 = 1 << 13;
const RESOLVE_IMPORT_STATEMENTS: i32 = 1 << 14;
const ADJUST_QNAMES: i32 = 1 << 15;
const EXTRA_SOURCES: i32 = 1 << 16;

const INHERITANCE: i32 = 1;
const NAMESPACES: i32 = 2;
const TYPES: i32 = 3;
const EXPRESSIONS: i32 = 4;

/// `flex2.compiler.util.graph.DependencyGraph<CompilationUnit>`.
#[derive(Default)]
pub struct DependencyGraph {
    names: Vec<String>,
    index: HashMap<String, usize>,
    preds: Vec<Vec<usize>>,
    succs: Vec<Vec<usize>>,
    weights: HashMap<String, Option<UnitRef>>,
}

impl DependencyGraph {
    pub fn get(&self, name: &str) -> Option<UnitRef> {
        self.weights.get(name).cloned().flatten()
    }

    pub fn put(&mut self, name: &str, unit: Option<UnitRef>) {
        self.weights.insert(name.to_string(), unit);
    }

    pub fn contains_vertex(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    pub fn add_vertex(&mut self, name: &str) -> usize {
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        let i = self.names.len();
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), i);
        self.preds.push(Vec::new());
        self.succs.push(Vec::new());
        i
    }

    pub fn dependency_exists(&self, name: &str, dep: &str) -> bool {
        match (self.index.get(name), self.index.get(dep)) {
            (Some(&h), Some(&t)) => self.preds[h].contains(&t),
            _ => false,
        }
    }

    pub fn add_dependency(&mut self, name: &str, dep: &str) {
        let h = self.add_vertex(name);
        let t = self.add_vertex(dep);
        if !self.preds[h].contains(&t) {
            self.preds[h].push(t);
            self.succs[t].push(h);
        }
    }

    /// `Algorithms.topologicalSort`: the vertices in visit order.
    pub fn topological_sort(&self) -> Vec<String> {
        let mut in_degree: Vec<usize> = self.preds.iter().map(|p| p.len()).collect();
        let mut queue: std::collections::VecDeque<usize> = (0..self.names.len()).filter(|&i| in_degree[i] == 0).collect();
        let mut out = Vec::new();
        while let Some(v) = queue.pop_front() {
            out.push(self.names[v].clone());
            for &h in &self.succs[v] {
                in_degree[h] -= 1;
                if in_degree[h] == 0 {
                    queue.push_back(h);
                }
            }
        }
        out
    }
}

pub struct CompilerApi {
    pub sources: Vec<SourceRef>,
    pub units: Vec<Option<UnitRef>>,
    pub igraph: DependencyGraph,
    pub symbol_table: SymbolTable,
    pub compilers: Vec<Box<dyn SubCompiler>>,
    pub source_list: Option<SourceList>,
    pub source_path: Option<SourcePath>,
    pub swc_context: CompilerSwcContext,
    pub configuration: Configuration,
}

fn workflow_of(s: &SourceRef) -> i32 {
    let s = s.borrow();
    if !s.is_preprocessed() {
        return 0;
    }
    match s.get_compilation_unit() {
        None => PREPROCESS,
        Some(u) => {
            let w = u.borrow().get_workflow();
            if w & PARSE1 == 0 {
                PREPROCESS
            } else {
                w
            }
        }
    }
}

fn unit_of(s: &SourceRef) -> Option<UnitRef> {
    s.borrow().get_compilation_unit()
}

fn logger_ok(s: &SourceRef) -> bool {
    s.borrow().get_logger().is_none_or(|l| l.error_count() == 0)
}

fn is_abc(s: &SourceRef) -> bool {
    s.borrow().get_mime_type() == Some(super::util::mime_mappings::ABC)
}

fn calculate_check_bits_mask(type_class: i32, workflow: i32) -> i32 {
    let base = (type_class - 1) * 4;
    let offset = match workflow {
        PARSE2 => 0,
        ANALYZE2 => 1,
        ANALYZE3 => 2,
        ANALYZE4 => 3,
        _ => 0,
    };
    1 << (base + offset)
}

fn names_of(u: &UnitRef, type_class: i32) -> Vec<Name> {
    let u = u.borrow();
    let set = match type_class {
        INHERITANCE => &u.inheritance,
        NAMESPACES => &u.namespaces,
        TYPES => &u.types,
        _ => &u.expressions,
    };
    set.iter().cloned().collect()
}

/// A unit's dependency sets, by which one a phase resolves.
#[derive(Clone, Copy)]
enum Deps {
    Inheritance,
    Namespaces,
    Types,
    Expressions,
}

fn deps_mut(u: &mut compilation_unit::CompilationUnit, d: Deps) -> (&mut JHashSet<Name>, &mut super::util::MultiNameMap) {
    match d {
        Deps::Inheritance => (&mut u.inheritance, &mut u.inheritance_history),
        Deps::Namespaces => (&mut u.namespaces, &mut u.namespace_history),
        Deps::Types => (&mut u.types, &mut u.type_history),
        Deps::Expressions => (&mut u.expressions, &mut u.expression_history),
    }
}

impl CompilerApi {
    fn get_compiler(&self, s: &SourceRef) -> Option<usize> {
        let mime = s.borrow().get_mime_type()?;
        self.compilers.iter().position(|c| c.is_supported(mime))
    }

    fn too_many_errors() -> bool {
        toolkit::error_count() > 100
    }

    /// `compile(...)` from `getCommonBuiltinClasses` through `getExtraSources`, with `PreLink` as the hook.
    pub fn compile(&mut self, pre_link: &mut dyn FnMut(&mut CompilerApi) -> bool) -> Result<Vec<UnitRef>, ()> {
        if let Some(sp) = &self.source_path {
            sp.display_warnings();
        }
        if let Some(sl) = &self.source_list {
            self.sources.extend(sl.retrieve_sources());
        }
        for i in 0..self.sources.len() {
            let s = self.sources[i].clone();
            let u = unit_of(&s);
            self.add_vertex_to_graphs(&s, u);
        }
        self.get_common_builtin_classes();
        self.batch()?;
        self.get_include_classes();
        if toolkit::error_count() > 0 {
            return Err(());
        }
        self.batch()?;
        let mut count = 0;
        let mut run_prelink = true;
        while run_prelink && count < 1000 {
            count += 1;
            run_prelink = pre_link(self);
            if toolkit::error_count() > 0 {
                return Err(());
            }
            self.batch()?;
        }
        let mut count = 0;
        while {
            count += 1;
            count < 1000
        } {
            let num_sources = self.sources.len();
            self.get_extra_sources();
            if toolkit::error_count() > 0 {
                return Err(());
            }
            self.batch()?;
            if self.sources.len() == num_sources {
                break;
            }
        }
        Ok(self.units.iter().flatten().cloned().collect())
    }

    fn batch(&mut self) -> Result<(), ()> {
        loop {
            self.units.clear();
            self.batch2();
            self.symbol_table.per_compile_data.borrow_mut().reuse();
            if toolkit::error_count() > 0 {
                return Err(());
            }
            if self.units_reset() == 0 {
                return Ok(());
            }
        }
    }

    fn units_reset(&mut self) -> i32 {
        let mut reset_count = 0;
        for u in self.units.iter().flatten() {
            let s = u.borrow().get_source();
            let (internal, compiled) = {
                let s = s.borrow();
                (s.is_internal(), s.is_compiled())
            };
            if !internal && !compiled {
                u.borrow_mut().reset();
                reset_count += 1;
            } else if !internal {
                u.borrow_mut().check_bits = 0;
            }
        }
        reset_count
    }

    fn batch2(&mut self) {
        let mut targets: Vec<Option<SourceRef>> = Vec::with_capacity(self.sources.len());
        self.units.clear();
        for s in &self.sources {
            if s.borrow().is_compiled() {
                self.units.push(unit_of(s));
            } else {
                self.units.push(None);
            }
        }
        while self.next_source(&mut targets) > 0 {
            let mut postprocess_count = 0;
            for i in 0..targets.len() {
                let Some(s) = targets[i].clone() else { continue };
                let w = workflow_of(&s);
                if w & PREPROCESS == 0 {
                    self.preprocess(i);
                } else if w & PARSE1 == 0 {
                    self.parse1(i);
                    self.resolve_inheritance(i);
                    self.add_generated_sources(i, i + 1);
                } else if w & PARSE2 == 0 {
                    self.parse2(i);
                    self.add_generated_sources(i, i + 1);
                } else if w & ANALYZE1 == 0 {
                    self.analyze(i, 1);
                    self.resolve_dependencies(i, Deps::Namespaces, false);
                    self.add_generated_sources(i, i + 1);
                } else if w & ANALYZE2 == 0 {
                    self.analyze(i, 2);
                    self.resolve_type(i);
                    if self.configuration.strict {
                        self.resolve_import_statements(i);
                    }
                    self.resolve_expression(i);
                } else if w & ANALYZE3 == 0 {
                    self.analyze(i, 3);
                } else if w & ANALYZE4 == 0 {
                    self.analyze(i, 4);
                } else if w & GENERATE == 0 {
                    self.generate(i);
                    self.add_generated_sources(i, i + 1);
                    self.resolve_expression(i);
                    self.mark_done(i);
                }
                if Self::too_many_errors() {
                    break;
                }
                if w & GENERATE != 0 {
                    self.postprocess(i);
                    self.resolve_expression(i);
                    postprocess_count += 1;
                }
                if Self::too_many_errors() {
                    break;
                }
            }
            if (postprocess_count == targets.len() && self.sources.len() == targets.len()) || Self::too_many_errors() {
                break;
            }
        }
        self.adjust_qnames();
    }

    fn next_source(&mut self, targets: &mut Vec<Option<SourceRef>>) -> i32 {
        let mut count = 0;
        let mut is_done = 0;
        let strict = self.configuration.strict;
        let warnings = self.configuration.warnings;
        let factor = 1000.0;
        targets.clear();
        targets.resize(self.sources.len(), None);
        let mut processed: Vec<String> = Vec::new();

        for i in (0..self.sources.len()).rev() {
            let s = self.sources[i].clone();
            let u = unit_of(&s);
            let w = workflow_of(&s);
            let okay = if w == 0 || (w & PREPROCESS) == 0 || (w & PARSE1) == 0 {
                Some(logger_ok(&s))
            } else if (w & PARSE2) == 0 {
                Some(logger_ok(&s) && self.check(u.as_ref().unwrap(), INHERITANCE, PARSE2))
            } else if (w & ANALYZE1) == 0 {
                Some(logger_ok(&s))
            } else if (w & ANALYZE2) == 0 {
                let u = u.as_ref().unwrap();
                let ok = logger_ok(&s) && self.check_inheritance(u, ANALYZE2, &mut processed) && self.check(u, NAMESPACES, ANALYZE2);
                processed.clear();
                Some(ok)
            } else if (w & ANALYZE3) == 0 {
                let u = u.as_ref().unwrap();
                let ok = logger_ok(&s)
                    && self.check_inheritance(u, ANALYZE3, &mut processed)
                    && self.check(u, TYPES, ANALYZE2)
                    && self.check(u, NAMESPACES, ANALYZE3)
                    && ((!strict && !warnings) || self.check(u, EXPRESSIONS, ANALYZE2));
                processed.clear();
                Some(ok)
            } else if (w & ANALYZE4) == 0 {
                let u = u.as_ref().unwrap();
                let ok = logger_ok(&s)
                    && self.check_inheritance(u, ANALYZE4, &mut processed)
                    && self.check(u, NAMESPACES, ANALYZE4)
                    && self.check_deep(u, TYPES, &mut processed)
                    && ((!strict && !warnings) || self.check_deep(u, EXPRESSIONS, &mut processed));
                processed.clear();
                Some(ok)
            } else if (w & GENERATE) == 0 {
                Some(logger_ok(&s))
            } else {
                if logger_ok(&s) {
                    is_done += 1;
                }
                None
            };
            if okay == Some(true) {
                targets[i] = Some(s);
                count += 1;
            }
        }

        if count > 0 {
            let n = targets.len();
            let mut bits = vec![false; n];
            let max_budget = 100.0;
            let mut budget = 0.0f64;
            let all = |w: i32, flags: &[i32]| w != 0 && flags.iter().all(|f| w & f != 0);
            type Rule = (fn(i32) -> bool, Option<bool>, bool);
            let rules: [Rule; 11] = [
                (|w| w != 0 && [PREPROCESS, PARSE1, PARSE2, ANALYZE1, ANALYZE2, ANALYZE3, ANALYZE4].iter().all(|f| w & f != 0) && w & GENERATE == 0, None, false),
                (|w| w != 0 && [PREPROCESS, PARSE1, PARSE2, ANALYZE1, ANALYZE2].iter().all(|f| w & f != 0) && w & ANALYZE3 == 0, None, false),
                (|w| w != 0 && [PREPROCESS, PARSE1, PARSE2, ANALYZE1, ANALYZE2, ANALYZE3].iter().all(|f| w & f != 0) && w & ANALYZE4 == 0, None, false),
                (|w| w != 0 && [PREPROCESS, PARSE1, PARSE2].iter().all(|f| w & f != 0) && w & ANALYZE1 == 0, None, false),
                (|w| w == 0, None, false),
                (|w| w != 0 && [PREPROCESS, PARSE1, PARSE2, ANALYZE1].iter().all(|f| w & f != 0) && w & ANALYZE2 == 0, Some(true), false),
                (|w| w != 0 && [PREPROCESS, PARSE1].iter().all(|f| w & f != 0) && w & PARSE2 == 0, Some(true), false),
                (|w| w != 0 && w & PREPROCESS != 0 && w & PARSE1 == 0, Some(true), false),
                (|w| w != 0 && [PREPROCESS, PARSE1, PARSE2, ANALYZE1].iter().all(|f| w & f != 0) && w & ANALYZE2 == 0, Some(false), true),
                (|w| w != 0 && [PREPROCESS, PARSE1].iter().all(|f| w & f != 0) && w & PARSE2 == 0, Some(false), true),
                (|w| w != 0 && w & PREPROCESS != 0 && w & PARSE1 == 0, Some(false), true),
            ];
            let _ = all;
            for (pred, abc, counts) in rules {
                let mut i = n;
                while i > 0 && budget < max_budget {
                    i -= 1;
                    let Some(s) = &targets[i] else { continue };
                    let w = workflow_of(s);
                    if pred(w) && abc.is_none_or(|a| a == is_abc(s)) {
                        if counts {
                            budget += calculate_budget(s, factor);
                        }
                        bits[i] = true;
                    }
                }
            }
            count = 0;
            for i in 0..n {
                if !bits[i] {
                    targets[i] = None;
                } else {
                    count += 1;
                }
            }
        } else if count == 0 && is_done == self.sources.len() {
            targets.clear();
            targets.extend(self.sources.iter().cloned().map(Some));
            count = targets.len() as i32;
        } else if count == 0 && is_done != self.sources.len() {
            self.detect_cycles();
        }
        count
    }

    fn check(&self, unit: &UnitRef, types_id: i32, workflow: i32) -> bool {
        let mask = calculate_check_bits_mask(types_id, workflow);
        if unit.borrow().check_bits & mask > 0 {
            return true;
        }
        for name in names_of(unit, types_id) {
            let Name::Q(q) = name else { continue };
            let s = self.symbol_table.find_source_by_qname(&q);
            let u = s.as_ref().and_then(unit_of);
            if u.as_ref().is_some_and(|u| Rc::ptr_eq(u, unit)) {
                continue;
            }
            let Some(u) = u else { return false };
            if u.borrow().get_workflow() & workflow == 0 {
                return false;
            }
            if u.borrow().type_info.is_none() {
                return false;
            }
            if !logger_ok(s.as_ref().unwrap()) {
                return false;
            }
        }
        unit.borrow_mut().check_bits |= mask;
        true
    }

    fn check_inheritance(&self, unit: &UnitRef, workflow: i32, processed: &mut Vec<String>) -> bool {
        processed.push(unit.borrow().get_source().borrow().get_name().to_string());
        if !self.check(unit, INHERITANCE, workflow) {
            return false;
        }
        for name in names_of(unit, INHERITANCE) {
            let Name::Q(q) = name else { continue };
            let s = self.symbol_table.find_source_by_qname(&q);
            let Some(u) = s.as_ref().and_then(unit_of) else { return false };
            let sname = s.as_ref().unwrap().borrow().get_name().to_string();
            if Rc::ptr_eq(&u, unit) || processed.contains(&sname) {
                continue;
            }
            if !self.check_inheritance(&u, workflow, processed) {
                return false;
            }
        }
        true
    }

    fn check_deep(&self, unit: &UnitRef, types_id: i32, processed: &mut Vec<String>) -> bool {
        let mask = calculate_check_bits_mask(types_id, ANALYZE3);
        if unit.borrow().check_bits & mask > 0 {
            return true;
        }
        processed.push(unit.borrow().get_source().borrow().get_name().to_string());
        if !self.check(unit, types_id, ANALYZE3) {
            return false;
        }
        for name in names_of(unit, types_id) {
            let Name::Q(q) = name else { continue };
            let s = self.symbol_table.find_source_by_qname(&q);
            let Some(u) = s.as_ref().and_then(unit_of) else { return false };
            let sname = s.as_ref().unwrap().borrow().get_name().to_string();
            if Rc::ptr_eq(&u, unit) || processed.contains(&sname) {
                continue;
            }
            if !self.check_deep(&u, INHERITANCE, processed) || !self.check_deep(&u, TYPES, processed) || !self.check_deep(&u, EXPRESSIONS, processed) {
                return false;
            }
        }
        unit.borrow_mut().check_bits |= mask;
        true
    }

    fn detect_cycles(&self) {
        let mut tsort: crate::asc::util::jhash::JHashMap<String, SourceRef> = crate::asc::util::jhash::JHashMap::with_capacity(self.sources.len());
        for s in &self.sources {
            let n = s.borrow().get_name().to_string();
            tsort.insert(n, s.clone());
        }
        for name in self.igraph.topological_sort() {
            tsort.remove(name.as_str());
        }
        for (name, s) in tsort.iter() {
            if !s.borrow().has_error() {
                toolkit::log_for(CompilerMessage::error("The definition is in circular inheritance."), name);
            }
        }
    }

    pub fn add_vertex_to_graphs(&mut self, s: &SourceRef, u: Option<UnitRef>) {
        let name = s.borrow().get_name().to_string();
        if u.is_some() || self.igraph.get(&name).is_none() {
            self.igraph.put(&name, u);
        }
        self.igraph.add_vertex(&name);
    }

    fn add_edge_to_graphs(&mut self, use_igraph: bool, head: &str, tail: &str) {
        if use_igraph && head != tail && !self.igraph.dependency_exists(head, tail) {
            self.igraph.add_dependency(head, tail);
        }
    }

    /// Runs one phase of a source under its own logger, as each `CompilerAPI` phase does.
    fn with_logger<R>(s: &SourceRef, f: impl FnOnce() -> R) -> (R, i32) {
        let original = toolkit::get_logger();
        let local = s.borrow().get_logger();
        toolkit::set_logger(local.clone());
        let r = f();
        toolkit::set_logger(original);
        (r, local.map_or(0, |l| l.error_count()))
    }

    fn preprocess(&mut self, i: usize) {
        let s = self.sources[i].clone();
        if s.borrow().is_preprocessed() || s.borrow().is_compiled() {
            return;
        }
        let local = LocalLogger::new();
        local.set_source(&s);
        s.borrow_mut().set_logger(Some(local.clone()));
        match self.get_compiler(&s) {
            Some(_) => {
                s.borrow_mut().set_preprocessed();
            }
            None => {
                let name = s.borrow().get_name_for_reporting().to_string();
                let original = toolkit::get_logger();
                toolkit::set_logger(Some(local));
                toolkit::log(CompilerMessage::error(format!("No compiler for '{name}'.")), None, -1, -1, None);
                toolkit::set_logger(original);
                s.borrow_mut().disconnect_logger();
            }
        }
    }

    fn parse1(&mut self, i: usize) {
        let s = self.sources[i].clone();
        let u = if s.borrow().is_compiled() {
            unit_of(&s)
        } else {
            match self.get_compiler(&s) {
                Some(c) => {
                    let compiler = &mut self.compilers[c];
                    let symbol_table = &mut self.symbol_table;
                    let (u, errors) = Self::with_logger(&s, || compiler.parse1(&s, symbol_table));
                    if errors == 0 {
                        if let Some(u) = &u {
                            let defs: Vec<QName> = u.borrow().top_level_definitions.iter().cloned().collect();
                            self.symbol_table.register_qnames(&defs, &s);
                            let mut ub = u.borrow_mut();
                            ub.set_state(compilation_unit::SYNTAX_TREE);
                            ub.set_workflow(PREPROCESS);
                            ub.set_workflow(PARSE1);
                        }
                    }
                    u
                }
                None => None,
            }
        };
        if u.is_none() {
            s.borrow_mut().disconnect_logger();
        }
        while self.units.len() < i + 1 {
            self.units.push(None);
        }
        self.units[i] = u.clone();
        self.add_vertex_to_graphs(&s, u);
    }

    fn parse2(&mut self, i: usize) {
        let s = self.sources[i].clone();
        let u = unit_of(&s).unwrap();
        if u.borrow().get_workflow() & PARSE2 != 0 {
            return;
        }
        if s.borrow().is_compiled() {
            return;
        }
        let Some(c) = self.get_compiler(&s) else { return };
        let compiler = &mut self.compilers[c];
        let symbol_table = &mut self.symbol_table;
        let (_, errors) = Self::with_logger(&s, || {
            compiler.parse2(&u, symbol_table);
            u.borrow_mut().set_workflow(PARSE2);
        });
        if errors > 0 {
            s.borrow_mut().disconnect_logger();
        }
    }

    fn analyze(&mut self, i: usize, phase: i32) {
        let s = self.sources[i].clone();
        let u = unit_of(&s).unwrap();
        let flag = [0, ANALYZE1, ANALYZE2, ANALYZE3, ANALYZE4][phase as usize];
        if u.borrow().get_workflow() & flag != 0 {
            return;
        }
        if s.borrow().is_compiled() {
            return;
        }
        let Some(c) = self.get_compiler(&s) else {
            s.borrow_mut().disconnect_logger();
            return;
        };
        let compiler = &mut self.compilers[c];
        let symbol_table = &mut self.symbol_table;
        let (_, errors) = Self::with_logger(&s, || {
            match phase {
                1 => {
                    compiler.analyze1(&u, symbol_table);
                    if toolkit::error_count() == 0 {
                        Self::check_definitions(&s, &u);
                    }
                }
                2 => compiler.analyze2(&u, symbol_table),
                3 => compiler.analyze3(&u, symbol_table),
                _ => compiler.analyze4(&u, symbol_table),
            }
            u.borrow_mut().set_workflow(flag);
        });
        if errors > 0 {
            s.borrow_mut().disconnect_logger();
        }
    }

    /// The source-path checks `analyze(u, compilers, symbolTable, 1)` makes after `analyze1`. With
    /// `SEVERAL_DEFINITIONS` set, as when replacing one script of a SWF, a file may define several public definitions
    /// and the one named like the file is checked.
    fn check_definitions(s: &SourceRef, u: &UnitRef) {
        let (sp_owner, sl_owner, name, short_name, relative_path) = {
            let s = s.borrow();
            (s.is_source_path_owner(), s.is_source_list_owner(), s.get_name_for_reporting().to_string(), s.get_short_name().to_string(), s.get_relative_path().to_string())
        };
        if !(sp_owner || sl_owner) {
            return;
        }
        let mut defs = u.borrow().top_level_definitions.clone();
        if defs.len() > 1 && SEVERAL_DEFINITIONS.with(|d| d.get()) {
            if let Some(named) = defs.0.iter().find(|d| d.get_local_part() == short_name).cloned() {
                defs.0 = vec![named];
            }
        }
        if defs.len() > 1 {
            toolkit::log_for(
                CompilerMessage::error(format!("A file found in a source-path can not have more than one externally visible definition. {defs}")),
                &name,
            );
        } else if defs.is_empty() {
            toolkit::log_for(
                CompilerMessage::error("A file found in a source-path must have an externally visible definition. If a definition in the file is meant to be externally visible, please put the definition in a package."),
                &name,
            );
        } else {
            let def = defs.last().unwrap();
            if let Some((path_package, def_package)) = SourcePath::check_package_name_directory_name(def.get_namespace(), &relative_path) {
                toolkit::log_for(
                    CompilerMessage::error(format!(
                        "A file found in a source-path must have the same package structure '{path_package}', as the definition's package, '{def_package}'."
                    )),
                    &name,
                );
            }
            if let Some((path_name, def_name)) = SourcePath::check_class_name_file_name(def.get_local_part(), &short_name) {
                toolkit::log_for(
                    CompilerMessage::error(format!(
                        "A file found in a source-path '{path_name}' must have the same name as the class definition inside the file '{def_name}'."
                    )),
                    &name,
                );
            }
        }
    }

    fn generate(&mut self, i: usize) {
        let s = self.sources[i].clone();
        let u = unit_of(&s).unwrap();
        if u.borrow().get_workflow() & GENERATE != 0 {
            return;
        }
        u.borrow_mut().set_workflow(GENERATE);
        if u.borrow().is_bytecode_available() || s.borrow().is_compiled() {
            return;
        }
        let Some(c) = self.get_compiler(&s) else {
            s.borrow_mut().disconnect_logger();
            return;
        };
        let compiler = &mut self.compilers[c];
        let symbol_table = &mut self.symbol_table;
        let (_, errors) = Self::with_logger(&s, || {
            compiler.generate(&u, symbol_table);
            if !u.borrow().bytes.is_empty() {
                u.borrow_mut().set_state(compilation_unit::ABC);
            }
        });
        if errors > 0 {
            s.borrow_mut().disconnect_logger();
        }
    }

    fn postprocess(&mut self, i: usize) {
        let s = self.sources[i].clone();
        if s.borrow().is_compiled() {
            return;
        }
        let u = unit_of(&s).unwrap();
        let Some(c) = self.get_compiler(&s) else {
            s.borrow_mut().disconnect_logger();
            return;
        };
        let compiler = &mut self.compilers[c];
        let symbol_table = &mut self.symbol_table;
        let (_, errors) = Self::with_logger(&s, || compiler.postprocess(&u, symbol_table));
        if errors > 0 {
            s.borrow_mut().disconnect_logger();
        }
    }

    fn mark_done(&mut self, i: usize) {
        let s = self.sources[i].clone();
        if s.borrow().is_compiled() {
            unit_of(&s).unwrap().borrow_mut().set_state(compilation_unit::DONE);
        }
    }

    /// `resolveInheritance` / `resolveNamespace` and the two halves of `resolveType`: resolved multinames are replaced
    /// by their `QName`s.
    fn resolve_dependencies(&mut self, i: usize, which: Deps, use_igraph: bool) {
        let s = self.sources[i].clone();
        let Some(u) = unit_of(&s) else { return };
        let names: Vec<Name> = {
            let mut ub = u.borrow_mut();
            let (set, _) = deps_mut(&mut ub, which);
            if set.is_empty() {
                return;
            }
            set.iter().cloned().collect()
        };
        let head = s.borrow().get_name().to_string();
        let name = s.borrow().get_name_for_reporting().to_string();
        let mut q_names: JHashSet<QName> = JHashSet::new();
        let mut removed: Vec<Name> = Vec::new();
        for unresolved in names {
            let Name::M(m_name) = &unresolved else { continue };
            if let Some(q_name) = self.resolve_multi_name(Some(&name), m_name) {
                let tail_source = self.symbol_table.find_source_by_qname(&q_name).unwrap();
                let tail = tail_source.borrow().get_name().to_string();
                let tu = unit_of(&tail_source);
                self.add_vertex_to_graphs(&tail_source, tu);
                self.add_edge_to_graphs(use_igraph, &head, &tail);
                q_names.insert(q_name.clone());
                let mut ub = u.borrow_mut();
                let (_, history) = deps_mut(&mut ub, which);
                history.put(m_name.clone(), q_name);
                removed.push(unresolved.clone());
            }
        }
        let mut ub = u.borrow_mut();
        let (set, _) = deps_mut(&mut ub, which);
        for r in removed {
            set.remove(&r);
        }
        for q in q_names.iter() {
            set.insert(Name::Q(q.clone()));
        }
    }

    /// `addGeneratedSources(sources, igraph, ..., start, end)`: the sources a unit generated (embed classes, ...)
    /// join the batch, owned by the `ResourceContainer`, and become expression dependencies of the unit.
    fn add_generated_sources(&mut self, start: usize, end: usize) {
        for i in start..end {
            let Some(u) = unit_of(&self.sources[i]) else { continue };
            let generated: Option<Vec<(QName, SourceRef)>> = u.borrow().get_generated_sources().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            let Some(generated) = generated else { continue };
            for (q_name, g_source) in generated {
                let m_name = MultiName::new(q_name.get_namespace(), q_name.get_local_part());
                let g_name = g_source.borrow().get_name().to_string();
                if !self.igraph.contains_vertex(&g_name) {
                    g_source.borrow_mut().set_owner(super::source::Owner::ResourceContainer);
                    let gu = unit_of(&g_source);
                    self.add_vertex_to_graphs(&g_source, gu);
                    self.sources.push(g_source.clone());
                    self.symbol_table.register_multi_name(&m_name, &q_name);
                    self.symbol_table.register_qname(&q_name, &g_source);
                }
                u.borrow_mut().expressions.insert(Name::M(m_name));
            }
            u.borrow_mut().clear_generated_sources();
        }
    }

    fn resolve_inheritance(&mut self, i: usize) {
        let has = unit_of(&self.sources[i]).is_some_and(|u| !u.borrow().inheritance.is_empty());
        if has {
            self.resolve_dependencies(i, Deps::Inheritance, true);
        }
    }

    fn resolve_type(&mut self, i: usize) {
        let s = self.sources[i].clone();
        let u = unit_of(&s).unwrap();
        if u.borrow().get_workflow() & RESOLVE_TYPE != 0 {
            return;
        }
        if !u.borrow().types.is_empty() {
            self.resolve_dependencies(i, Deps::Types, false);
        }
        u.borrow_mut().set_workflow(RESOLVE_TYPE);
        if !u.borrow().namespaces.is_empty() {
            self.resolve_dependencies(i, Deps::Namespaces, false);
        }
    }

    fn resolve_import_statements(&mut self, i: usize) {
        let s = self.sources[i].clone();
        let u = unit_of(&s).unwrap();
        if u.borrow().get_workflow() & RESOLVE_IMPORT_STATEMENTS != 0 {
            return;
        }
        u.borrow_mut().set_workflow(RESOLVE_IMPORT_STATEMENTS);
        let packages: Vec<String> = u.borrow().import_package_statements.iter().cloned().collect();
        for p in packages {
            let has = self.source_path.as_mut().is_some_and(|sp| sp.has_package(&p)) || self.swc_context.has_package(&p);
            if !has {
                u.borrow_mut().import_package_statements.remove(&p);
            }
        }
        let defs: Vec<QName> = u.borrow().import_definition_statements.iter().cloned().collect();
        for d in defs {
            let has = self.source_path.as_mut().is_some_and(|sp| sp.has_definition(d.get_namespace(), d.get_local_part())) || self.swc_context.has_definition(&d);
            if !has {
                u.borrow_mut().import_definition_statements.remove(&d);
            }
        }
    }

    fn resolve_expression(&mut self, i: usize) {
        let s = self.sources[i].clone();
        let Some(u) = unit_of(&s) else { return };
        let names: Vec<Name> = u.borrow().expressions.iter().cloned().collect();
        if names.is_empty() {
            return;
        }
        let head = s.borrow().get_name().to_string();
        let name = s.borrow().get_name_for_reporting().to_string();
        let mut q_names: JHashSet<QName> = JHashSet::new();
        let mut removed = Vec::new();
        for unresolved in names {
            let Name::M(m_name) = &unresolved else { continue };
            match self.resolve_multi_name(Some(&name), m_name) {
                Some(q_name) => {
                    let tail_source = self.symbol_table.find_source_by_qname(&q_name).unwrap();
                    let tail = tail_source.borrow().get_name().to_string();
                    let tu = unit_of(&tail_source);
                    self.add_vertex_to_graphs(&tail_source, tu);
                    self.add_edge_to_graphs(false, &head, &tail);
                    q_names.insert(q_name.clone());
                    u.borrow_mut().expression_history.put(m_name.clone(), q_name);
                }
                None => {
                    if self.configuration.show_dependency_warnings {
                        toolkit::log_for(CompilerMessage::warning(format!("Unable to resolve dependency on {}.", m_name.local_part)), &name);
                    }
                }
            }
            removed.push(unresolved.clone());
        }
        let mut ub = u.borrow_mut();
        for r in removed {
            ub.expressions.remove(&r);
        }
        for q in q_names.iter() {
            ub.expressions.insert(Name::Q(q.clone()));
        }
    }

    fn adjust_qnames(&mut self) {
        for u in self.units.iter().flatten() {
            let (done, w) = {
                let ub = u.borrow();
                (ub.is_done(), ub.get_workflow())
            };
            if !done || w & ADJUST_QNAMES != 0 {
                continue;
            }
            let mut ub = u.borrow_mut();
            for set in [Deps::Inheritance, Deps::Namespaces, Deps::Types, Deps::Expressions] {
                let (set, _) = deps_mut(&mut ub, set);
                for name in set.keys_mut_in_place() {
                    if let Name::Q(q) = name {
                        Self::adjust_qname(q, &self.igraph, &self.symbol_table);
                    }
                }
            }
            ub.set_workflow(ADJUST_QNAMES);
        }
    }

    fn adjust_qname(q_name: &mut QName, igraph: &DependencyGraph, symbol_table: &SymbolTable) {
        let s = symbol_table.find_source_by_qname(q_name);
        let Some(u) = s.as_ref().and_then(|s| igraph.get(s.borrow().get_name())) else { return };
        let Ok(ub) = u.try_borrow() else { return };
        let src = ub.get_source();
        let owner_ok = {
            let src = src.borrow();
            src.is_source_path_owner() || src.is_source_list_owner()
        };
        if owner_ok && ub.top_level_definitions.len() == 1 {
            let def = ub.top_level_definitions.last().unwrap();
            if q_name.get_local_part() == def.get_local_part() && q_name.get_namespace() != def.get_namespace() {
                q_name.set_namespace(def.get_namespace());
            }
        }
    }

    pub fn resolve_multi_name(&mut self, name_for_reporting: Option<&str>, multi_name: &MultiName) -> Option<QName> {
        if let Some(q) = self.symbol_table.is_multi_name_resolved(multi_name) {
            return Some(q);
        }
        let mut q_name: Option<QName> = None;
        let mut source: Option<SourceRef> = None;
        let mut ambiguity: Option<(QName, SourceRef)> = None;
        let local_part = multi_name.local_part.clone();
        for ns in &multi_name.namespace_uri {
            let mut s = self.symbol_table.find_source_by_qname_parts(ns, &local_part);
            if s.is_none() {
                let where_ = self.find_definition(ns, &local_part);
                if let Some(w) = where_ {
                    s = Some(self.sources[w].clone());
                }
            }
            if let Some(s) = s {
                match &q_name {
                    None => {
                        q_name = Some(QName::new(ns, &local_part));
                        source = Some(s);
                    }
                    Some(q) if !q.equals(ns, &local_part) => {
                        ambiguity = Some((QName::new(ns, &local_part), s));
                        break;
                    }
                    _ => {}
                }
            }
        }
        if let Some((q2, s2)) = ambiguity {
            let q = q_name.unwrap();
            let msg = format!(
                "Can not resolve a multiname reference unambiguously. {} (from {}) and {} (from {}) are available.",
                q,
                source.unwrap().borrow().get_name(),
                q2,
                s2.borrow().get_name()
            );
            match name_for_reporting {
                Some(n) => toolkit::log_for(CompilerMessage::error(msg), n),
                None => toolkit::log(CompilerMessage::error(msg), None, -1, -1, None),
            }
            return None;
        } else if let Some(source) = &source {
            let q = q_name.clone().unwrap();
            self.symbol_table.register_multi_name(multi_name, &q);
            self.symbol_table.register_qname(&q, source);
        }
        q_name
    }

    fn find_definition(&mut self, namespace_uri: &str, local_part: &str) -> Option<usize> {
        let mut s = self.source_list.as_mut().and_then(|sl| sl.find_source(namespace_uri, local_part));
        if s.is_none() {
            if let Some(sp) = self.source_path.as_mut() {
                match sp.find_source(namespace_uri, local_part) {
                    Ok(found) => s = found,
                    Err(e) => toolkit::log_plain(toolkit::Level::Error, &e.message),
                }
            }
        }
        let swc_source = self.swc_context.get_source(namespace_uri, local_part);
        if let Some(swc) = swc_source {
            let take = match &s {
                None => true,
                Some(src) => {
                    let sb = src.borrow();
                    sb.get_last_modified() == swc.borrow().get_last_modified()
                        && sb.get_compilation_unit().is_none_or(|u| !u.borrow().has_type_info)
                }
            };
            if take {
                s = Some(swc);
            }
        }
        let s = s?;
        match self.sources.iter().position(|x| Rc::ptr_eq(x, &s)) {
            Some(w) => Some(w),
            None => {
                self.sources.push(s);
                Some(self.sources.len() - 1)
            }
        }
    }

    fn get_common_builtin_classes(&mut self) {
        for n in ["Object", "Class", "Function", "Boolean", "Number", "String", "Array", "int", "uint", "Namespace", "RegExp", "XML", "XMLList"] {
            if let Some(q) = self.resolve_multi_name(Some("builtin"), &MultiName::parse(n)) {
                let s = self.symbol_table.find_source_by_qname(&q).unwrap();
                let u = unit_of(&s);
                self.add_vertex_to_graphs(&s, u);
            }
        }
    }

    fn get_include_classes(&mut self) {
        let includes = self.configuration.includes.clone();
        for class_name in includes {
            let m = MultiName::parse(&super::util::name_formatter::to_colon(&class_name));
            match self.resolve_multi_name(Some("include"), &m) {
                Some(q) => {
                    let s = self.symbol_table.find_source_by_qname(&q).unwrap();
                    let u = unit_of(&s);
                    self.add_vertex_to_graphs(&s, u);
                }
                None => toolkit::log(CompilerMessage::error(format!("Unable to resolve a class for include: {class_name}.")), None, -1, -1, None),
            }
        }
    }

    fn get_extra_sources(&mut self) {
        for i in 0..self.sources.len() {
            let Some(u) = unit_of(&self.sources[i]) else { continue };
            if u.borrow().get_workflow() & EXTRA_SOURCES != 0 {
                continue;
            }
            let reporting = u.borrow().get_source().borrow().get_name_for_reporting().to_string();
            let loader_class = u.borrow().loader_class.clone();
            if let Some(class_name) = loader_class {
                match self.resolve_multi_name(Some(&reporting), &MultiName::parse(&class_name)) {
                    Some(q) => {
                        let s = self.symbol_table.find_source_by_qname(&q).unwrap();
                        let tu = unit_of(&s);
                        self.add_vertex_to_graphs(&s, tu);
                    }
                    None => toolkit::log(CompilerMessage::error(format!("Unable to resolve a class for factoryClass: {class_name}.")), None, -1, -1, None),
                }
            }
            let history: Vec<String> = u.borrow().resource_bundle_history.iter().cloned().collect();
            for rb in history {
                if !self.configuration.resource_bundles.contains(&rb) {
                    self.configuration.resource_bundles.push(rb);
                }
            }
            let extra: Vec<String> = u.borrow().extra_classes.iter().cloned().collect();
            for class_name in extra {
                match self.resolve_multi_name(Some(&reporting), &MultiName::parse(&class_name)) {
                    Some(q) => {
                        let s = self.symbol_table.find_source_by_qname(&q).unwrap();
                        let tu = unit_of(&s);
                        self.add_vertex_to_graphs(&s, tu);
                    }
                    None => toolkit::log(CompilerMessage::error(format!("Unable to resolve needed class: {class_name}.")), None, -1, -1, None),
                }
            }
            u.borrow_mut().set_workflow(EXTRA_SOURCES);
        }
    }
}

/// `calculateBudget`.
fn calculate_budget(s: &SourceRef, factor: f64) -> f64 {
    let s = s.borrow();
    match s.get_mime_type() {
        Some(super::util::mime_mappings::MXML) => s.size() as f64 * 4.5 / factor,
        Some(super::util::mime_mappings::AS) => (s.size() / factor as i64) as f64,
        _ => 0.0,
    }
}
