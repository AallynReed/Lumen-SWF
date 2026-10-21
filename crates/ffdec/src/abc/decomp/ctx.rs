//! Shared decompilation state: per-ABC context and per-method `LocalData`.

use std::collections::{HashMap, HashSet};

use super::names::{Dotted, Idents, Names};
use crate::abc::Abc;

#[derive(Debug, Clone)]
pub struct Config {
    pub display_dup_instructions: bool,
    pub qname_obfuscated_in_brackets: bool,
    pub register_name_format: String,
    pub local_names_from_debug: bool,
    pub param_names: bool,
    /// FFDec's `autoDeobfuscate`.
    pub deobfuscate: bool,
}

/// FFDec's global `autoDeobfuscate` setting, which new configs start from.
pub static AUTO_DEOBFUSCATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Default for Config {
    fn default() -> Self {
        Config { display_dup_instructions: false, qname_obfuscated_in_brackets: false, register_name_format: "_loc%d_".into(), local_names_from_debug: true, param_names: false, deobfuscate: AUTO_DEOBFUSCATE.load(std::sync::atomic::Ordering::Relaxed) }
    }
}

impl Config {
    /// The settings FFDec's decompiler tests run with.
    pub fn testing() -> Config {
        Config { display_dup_instructions: true, qname_obfuscated_in_brackets: true, ..Config::default() }
    }

    pub fn reg_name(&self, reg: i32) -> String {
        self.register_name_format.replace("%d", &reg.to_string())
    }
}

/// Everything shared by the methods of one ABC.
pub struct Ctx<'a> {
    pub abc: &'a Abc,
    pub ids: Idents,
    pub custom_ns: HashMap<String, Dotted>,
    pub display_ns: HashSet<usize>,
    pub cfg: Config,
    pub swf_version: u8,
    pub index: Option<std::sync::Arc<super::index::AbcIndex>>,
    pub doc: Option<DocClass>,
    /// `[Embed(...)]` lines of classes bound to SWF characters, by raw class name.
    pub embeds: HashMap<String, Vec<String>>,
    /// FFDec's FLA export mode: frame scripts and generated accessibility code are left out.
    pub fla: bool,
}

/// The SWF's document class and stage settings, printed as `[SWF(...)]` on that class.
#[derive(Debug, Clone)]
pub struct DocClass {
    pub name: String,
    pub width: i64,
    pub height: i64,
    pub background: Option<String>,
    pub frame_rate: f32,
}

impl<'a> Ctx<'a> {
    pub fn new(abc: &'a Abc, cfg: Config) -> Ctx<'a> {
        Ctx { abc, ids: Idents::default(), custom_ns: HashMap::new(), display_ns: crate::abc::usages::colliding_multinames(abc), cfg, swf_version: 10, index: None, doc: None, embeds: HashMap::new(), fla: false }
    }

    /// Attaches type information about the ABC's classes and the player library.
    pub fn with_index(mut self, index: std::sync::Arc<super::index::AbcIndex>) -> Self {
        self.custom_ns = index.custom_namespaces(self.abc);
        self.index = Some(index);
        self
    }

    pub fn names(&self) -> Names<'_> {
        Names { abc: self.abc, ids: &self.ids, custom_ns: &self.custom_ns, display_ns: &self.display_ns }
    }
}

/// FFDec's `LocalData` for AS3.
pub struct LocalData<'c, 'a> {
    pub ctx: &'c Ctx<'a>,
    pub local_reg_names: HashMap<i32, String>,
    pub fqn: Vec<Dotted>,
    pub seen_methods: HashSet<usize>,
    pub class_index: i32,
    pub call_stack: Vec<usize>,
    /// Printing AS1/2 (`localData.abc == null` in FFDec).
    pub as2: bool,
    /// `localData.as2ClassMemberScope`.
    pub as2_scope: std::cell::RefCell<Option<std::rc::Rc<crate::action::decomp::print::Scope>>>,
}

impl<'c, 'a> LocalData<'c, 'a> {
    pub fn new(ctx: &'c Ctx<'a>) -> Self {
        LocalData { ctx, local_reg_names: HashMap::new(), fqn: Vec::new(), seen_methods: HashSet::new(), class_index: -1, call_stack: Vec::new(), as2: false, as2_scope: Default::default() }
    }

    pub fn abc(&self) -> &'a Abc {
        self.ctx.abc
    }

    pub fn names(&self) -> Names<'_> {
        self.ctx.names()
    }

    /// `AVM2Item.localRegName`.
    pub fn reg_name(&self, reg: i32) -> String {
        match self.local_reg_names.get(&reg) {
            Some(n) => self.ctx.ids.print(n),
            None if reg == 0 => "this".into(),
            None => self.ctx.cfg.reg_name(reg),
        }
    }
}
