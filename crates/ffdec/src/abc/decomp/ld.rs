//! FFDec's `BaseLocalData`/`AVM2LocalData`: translation state threaded through the graph.
//!
//! Java copies these objects with a copy constructor that shares most maps, so the maps are
//! reference counted and a `clone()` behaves like `new AVM2LocalData(other)`.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::item::{It, PartId as P};
use super::stack::Stack;

pub type Shared<T> = Rc<RefCell<T>>;

pub fn shared<T>(v: T) -> Shared<T> {
    Rc::new(RefCell::new(v))
}

#[derive(Debug, Clone, Default)]
pub struct SecondPass {
    pub switch_cases: Vec<Vec<P>>,
    pub switch_breaks: Vec<Option<P>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InsStats {
    pub seen: bool,
    pub stackpos: i32,
    pub scopepos: i32,
    pub stackpos_after: i32,
    pub scopepos_after: i32,
}

#[derive(Debug, Clone, Default)]
pub struct CodeStats {
    pub ins: Vec<InsStats>,
    pub max_stack: i32,
    pub max_scope: i32,
    pub max_local: i32,
    pub init_scope: i32,
    pub has_activation: bool,
    pub has_set_dxns: bool,
}

#[derive(Clone)]
pub struct Ld {
    pub line_start: Option<usize>,
    pub second_pass: Option<Rc<SecondPass>>,
    pub max_temp: Rc<Cell<i32>>,
    pub gotos_used: Rc<Cell<bool>>,
    pub switch_cases: Shared<Vec<Vec<P>>>,
    pub switch_breaks: Shared<Vec<Option<P>>>,
    pub local_regs: Shared<HashMap<i32, It>>,
    pub scope_stack: Shared<Stack>,
    pub local_scope_stack: Shared<Stack>,
    pub local_reg_types: Shared<HashMap<i32, It>>,
    pub parsed_exceptions: Shared<Vec<usize>>,
    pub parsed_exception_ids: Shared<Vec<i32>>,
    pub finally_jumps: Shared<Vec<(P, P)>>,
    pub finally_jumps_to_index: Shared<Vec<(P, i32)>>,
    pub finally_index_to_default: Shared<HashMap<i32, Option<P>>>,
    pub ignored_switches: Shared<BTreeMap<i32, P>>,
    pub default_parts: Shared<BTreeMap<i32, P>>,
    pub switched_regs: Shared<HashMap<i32, i32>>,
    pub push_default_part: Shared<HashMap<i32, P>>,
    pub finally_kinds: Shared<HashMap<i32, i32>>,
    pub finally_throw_parts: Shared<BTreeMap<i32, P>>,
    pub finally_target_parts: Shared<HashMap<i32, Option<P>>>,
    pub default_ways: Shared<Vec<(P, Option<i32>)>>,
    pub set_local_usages: Shared<BTreeMap<i32, BTreeSet<i32>>>,
    pub code_stats: Shared<Option<Rc<CodeStats>>>,
    pub finally_double_push: Shared<HashSet<i32>>,
    pub in_get_loops: bool,
    pub bottom_set_locals: Shared<Vec<It>>,
    pub this_has_default_to_primitive: bool,
}

impl Ld {
    pub fn new(max_temp: Rc<Cell<i32>>) -> Ld {
        Ld {
            line_start: None,
            second_pass: None,
            max_temp: max_temp.clone(),
            gotos_used: Rc::new(Cell::new(false)),
            switch_cases: shared(Vec::new()),
            switch_breaks: shared(Vec::new()),
            local_regs: shared(HashMap::new()),
            scope_stack: shared(Stack::new(true, max_temp.clone())),
            local_scope_stack: shared(Stack::new(true, max_temp)),
            local_reg_types: shared(HashMap::new()),
            parsed_exceptions: shared(Vec::new()),
            parsed_exception_ids: shared(Vec::new()),
            finally_jumps: shared(Vec::new()),
            finally_jumps_to_index: shared(Vec::new()),
            finally_index_to_default: shared(HashMap::new()),
            ignored_switches: shared(BTreeMap::new()),
            default_parts: shared(BTreeMap::new()),
            switched_regs: shared(HashMap::new()),
            push_default_part: shared(HashMap::new()),
            finally_kinds: shared(HashMap::new()),
            finally_throw_parts: shared(BTreeMap::new()),
            finally_target_parts: shared(HashMap::new()),
            default_ways: shared(Vec::new()),
            set_local_usages: shared(BTreeMap::new()),
            code_stats: shared(None),
            finally_double_push: shared(HashSet::new()),
            in_get_loops: false,
            bottom_set_locals: shared(Vec::new()),
            this_has_default_to_primitive: false,
        }
    }

    /// `new AVM2LocalData(this)`: shares the maps, drops the line start.
    pub fn copy(&self) -> Ld {
        let mut c = self.clone();
        c.line_start = None;
        c
    }

    pub fn new_local_scope(&mut self) {
        let mt = self.max_temp.clone();
        self.local_scope_stack = shared(Stack::new(true, mt));
    }

    pub fn set_local_usages_of(&self, ip: i32) -> BTreeSet<i32> {
        self.set_local_usages.borrow().get(&ip).cloned().unwrap_or_default()
    }

    pub fn stats(&self) -> Rc<CodeStats> {
        self.code_stats.borrow().clone().expect("code stats")
    }
}
