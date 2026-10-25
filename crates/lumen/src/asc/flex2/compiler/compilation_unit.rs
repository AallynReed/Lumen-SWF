//! Port of `flex2.compiler.CompilationUnit` and `flex2.compiler.CompilerContext`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::source::{Source, SourceRef};
use super::util::{MultiNameMap, Name, QName, QNameList};
use crate::asc::flex2::compiler::as3::type_table::AbcClass;
use crate::asc::parser::node::NodeRef;
use crate::asc::semantics::constant_evaluator::ConstantEvaluator;
use crate::asc::semantics::flow_analyzer::FlowAnalyzer;
use crate::asc::semantics::meta_data::MetaData;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::JHashSet;

pub type UnitRef = Rc<RefCell<CompilationUnit>>;

pub const EMPTY: i32 = 0;
pub const SYNTAX_TREE: i32 = 1;
pub const ABC: i32 = 2;
pub const DONE: i32 = 4;

/// `CompilerContext`, with its attribute map as typed fields.
#[derive(Default)]
pub struct CompilerContext {
    asc_context: Option<ContextRef>,
    pub flow_analyzer: Option<FlowAnalyzer>,
    pub constant_evaluator: Option<ConstantEvaluator>,
    pub processed: Option<std::collections::HashSet<String>>,
    pub main_definition: Option<String>,
}

impl CompilerContext {
    pub fn get_asc_context(&self) -> Option<ContextRef> {
        self.asc_context.clone()
    }

    pub fn set_asc_context(&mut self, cx: Option<ContextRef>) {
        self.asc_context = cx;
    }

    pub fn remove_asc_context(&mut self) -> Option<ContextRef> {
        self.asc_context.take()
    }

    pub fn clear(&mut self) {
        *self = CompilerContext::default();
    }
}

pub struct CompilationUnit {
    source: Weak<RefCell<Source>>,
    syntax_tree: Option<NodeRef>,
    pub context: CompilerContext,
    state: i32,
    workflow: i32,
    pub bytes: Vec<u8>,
    pub metadata: Vec<NodeRef>,
    pub swf_meta_data: Option<MetaData>,
    pub icon: Option<String>,
    pub loader_class: Option<String>,
    pub loader_class_base: Option<String>,
    pub extra_classes: JHashSet<String>,
    pub remote_class_aliases: HashMap<String, String>,
    pub licensed_class_reqs: HashMap<String, String>,
    pub effect_triggers: HashMap<String, String>,
    pub mixins: JHashSet<String>,
    pub resource_bundles: JHashSet<String>,
    pub resource_bundle_history: JHashSet<String>,
    pub top_level_definitions: QNameList,
    pub inheritance: JHashSet<Name>,
    pub types: JHashSet<Name>,
    pub expressions: JHashSet<Name>,
    pub namespaces: JHashSet<Name>,
    pub import_package_statements: JHashSet<String>,
    pub import_definition_statements: JHashSet<QName>,
    pub inheritance_history: MultiNameMap,
    pub type_history: MultiNameMap,
    pub namespace_history: MultiNameMap,
    pub expression_history: MultiNameMap,
    pub skin_states: Option<std::collections::HashSet<String>>,
    pub host_component_meta_data: Option<NodeRef>,
    pub has_type_info: bool,
    pub type_info: Option<ObjectValue>,
    pub class_table: HashMap<String, Rc<AbcClass>>,
    pub check_bits: i32,
    pub assets: super::assets::Assets,
    generated_sources: Option<crate::asc::util::jhash::JHashMap<QName, SourceRef>>,
    pub styles: super::css::styles::Styles,
    /// Only MXML components set it (`setStylesContainer`).
    pub styles_container: Option<Rc<super::css::styles_container::StylesContainer>>,
    pub style_name: Option<String>,
}

impl CompilationUnit {
    pub(super) fn new(source: &SourceRef, syntax_tree: Option<NodeRef>) -> UnitRef {
        Rc::new(RefCell::new(CompilationUnit {
            source: Rc::downgrade(source),
            syntax_tree,
            context: CompilerContext::default(),
            state: EMPTY,
            workflow: 0,
            bytes: Vec::new(),
            metadata: Vec::new(),
            swf_meta_data: None,
            icon: None,
            loader_class: None,
            loader_class_base: None,
            extra_classes: JHashSet::new(),
            remote_class_aliases: HashMap::new(),
            licensed_class_reqs: HashMap::new(),
            effect_triggers: HashMap::new(),
            mixins: JHashSet::with_capacity(2),
            resource_bundles: JHashSet::with_capacity(1),
            resource_bundle_history: JHashSet::with_capacity(1),
            top_level_definitions: QNameList::default(),
            inheritance: JHashSet::with_capacity(2),
            types: JHashSet::with_capacity(8),
            expressions: JHashSet::with_capacity(8),
            namespaces: JHashSet::with_capacity(2),
            import_package_statements: JHashSet::with_capacity(16),
            import_definition_statements: JHashSet::with_capacity(16),
            inheritance_history: MultiNameMap::with_capacity(2),
            type_history: MultiNameMap::with_capacity(8),
            namespace_history: MultiNameMap::with_capacity(2),
            expression_history: MultiNameMap::with_capacity(8),
            skin_states: None,
            host_component_meta_data: None,
            has_type_info: false,
            type_info: None,
            class_table: HashMap::new(),
            check_bits: 0,
            assets: Default::default(),
            generated_sources: None,
            styles: super::css::styles::Styles::new(2),
            styles_container: None,
            style_name: None,
        }))
    }

    pub fn get_source(&self) -> SourceRef {
        self.source.upgrade().expect("CompilationUnit outlived its Source")
    }

    /// `resetKeepTypeInfo()` + `removeTypeInfo()`.
    pub fn reset(&mut self) {
        self.state = EMPTY;
        self.workflow = 0;
        self.bytes.clear();
        self.metadata.clear();
        self.swf_meta_data = None;
        self.loader_class = None;
        self.extra_classes.clear();
        self.remote_class_aliases.clear();
        self.licensed_class_reqs.clear();
        self.effect_triggers.clear();
        self.mixins = JHashSet::with_capacity(2);
        self.resource_bundles.clear();
        self.resource_bundle_history.clear();
        self.top_level_definitions.clear();
        self.inheritance.clear();
        self.types.clear();
        self.expressions.clear();
        self.namespaces.clear();
        self.import_package_statements.clear();
        self.import_definition_statements.clear();
        self.inheritance_history.clear();
        self.type_history.clear();
        self.expression_history.clear();
        self.namespace_history.clear();
        self.check_bits = 0;
        self.assets = Default::default();
        self.generated_sources = None;
        self.styles.clear();
        self.has_type_info = false;
        self.type_info = None;
        self.class_table.clear();
    }

    pub fn is_root(&self) -> bool {
        self.get_source().borrow().is_root()
    }

    pub(super) fn set_state(&mut self, flag: i32) {
        self.state |= flag;
        if flag == ABC {
            self.syntax_tree = None;
            if !self.is_root() {
                self.get_source().borrow_mut().disconnect_logger();
            }
        } else if flag == DONE {
            self.has_type_info = self.type_info.is_some();
            self.context.clear();
            self.metadata.clear();
        }
    }

    pub fn is_bytecode_available(&self) -> bool {
        (self.state & ABC) != 0
    }

    pub fn is_done(&self) -> bool {
        (self.state & DONE) != 0
    }

    pub(super) fn set_workflow(&mut self, flag: i32) {
        self.workflow |= flag;
    }

    pub fn get_workflow(&self) -> i32 {
        self.workflow
    }

    pub fn get_syntax_tree(&self) -> Option<NodeRef> {
        self.syntax_tree.clone()
    }

    pub fn set_syntax_tree(&mut self, node: Option<NodeRef>) {
        self.syntax_tree = node;
    }

    pub fn get_byte_codes(&self) -> &[u8] {
        &self.bytes
    }

    /// `addGeneratedSources(generatedSources)`: a `putAll` into the unit's own map.
    pub fn add_generated_sources(&mut self, sources: crate::asc::util::jhash::JHashMap<QName, SourceRef>) {
        let n = sources.len();
        let m = self.generated_sources.get_or_insert_with(Default::default);
        m.put_all(sources.iter().map(|(k, v)| (k.clone(), v.clone())), n);
    }

    pub fn add_generated_source(&mut self, def_name: QName, source: SourceRef) {
        self.generated_sources.get_or_insert_with(Default::default).insert(def_name, source);
    }

    pub fn get_generated_sources(&self) -> Option<&crate::asc::util::jhash::JHashMap<QName, SourceRef>> {
        self.generated_sources.as_ref()
    }

    pub fn clear_generated_sources(&mut self) {
        self.generated_sources = None;
    }

    pub fn has_assets(&self) -> bool {
        self.assets.count() > 0
    }
}
