//! Port of `flex2.compiler.SymbolTable`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use super::as3::type_table::AbcClass;
use super::configuration::Configuration;
use super::source::SourceRef;
use super::util::{MultiName, QName};
use crate::asc::emit::bytecode_emitter::new_bytecode_emitter;
use crate::asc::semantics::emitter::EmitterRef;
use crate::asc::util::context::{Context, ContextStatics, OMIT_TRACE, USE_VERBOSE_ERRORS};
use crate::asc::util::jhash::JHashMap;

pub const NOTYPE: &str = "*";

pub struct SymbolTable {
    class_table: super::as3::reflect::type_table::ClassTable,
    pub per_compile_data: Rc<RefCell<ContextStatics>>,
    q_name_table: JHashMap<QName, SourceRef>,
    multi_names: JHashMap<MultiName, QName>,
    pub emitter: EmitterRef,
    styles: Rc<RefCell<super::css::styles::Styles>>,
    /// `getContext().getAttribute(StylesContainer.class.getName())`: the application's styles, made by `PreLink`.
    pub styles_container: Option<Rc<super::css::styles_container::StylesContainer>>,
    rb_names: std::collections::HashMap<String, Option<Vec<QName>>>,
    rb_name_table: std::collections::HashMap<String, SourceRef>,
    type_analyzer: super::as3::binding::type_analyzer::TypeAnalyzerRef,
}

impl SymbolTable {
    pub fn new(configuration: &Configuration) -> SymbolTable {
        Self::with_statics(configuration, Rc::new(RefCell::new(ContextStatics::new())))
    }

    pub fn with_statics(configuration: &Configuration, per_compile_data: Rc<RefCell<ContextStatics>>) -> SymbolTable {
        {
            let mut s = per_compile_data.borrow_mut();
            s.use_static_semantics = configuration.strict;
            s.dialect = configuration.dialect();
            s.language_id = Context::get_language_id("US");
            s.set_abc_version(ContextStatics::get_target_avm(configuration.target_player_major));
            s.use_namespaces.extend(ContextStatics::get_required_use_namespaces(configuration.target_player_major));
        }
        OMIT_TRACE.store(!configuration.debug && configuration.omit_trace_statements, Ordering::Relaxed);
        USE_VERBOSE_ERRORS.store(false, Ordering::Relaxed);
        let cx = Rc::new(RefCell::new(Context::new(per_compile_data.clone())));
        let emitter: EmitterRef = Rc::new(RefCell::new(new_bytecode_emitter(&cx, None, false, false)));
        cx.borrow().set_emitter(Some(emitter.clone()));
        SymbolTable {
            class_table: super::as3::reflect::type_table::new_class_table(),
            per_compile_data,
            q_name_table: JHashMap::with_capacity(300),
            multi_names: JHashMap::with_capacity(1024),
            emitter,
            styles: Rc::new(RefCell::new(super::css::styles::Styles::default())),
            styles_container: None,
            rb_names: std::collections::HashMap::new(),
            rb_name_table: std::collections::HashMap::new(),
            type_analyzer: Rc::new(RefCell::new(super::as3::binding::type_analyzer::TypeAnalyzer::new())),
        }
    }

    /// `registerStyles(newStyles)`.
    pub fn register_styles(&mut self, new_styles: &super::css::styles::Styles) -> Result<(), super::css::styles::StyleConflictException> {
        self.styles.borrow_mut().add_styles(new_styles)
    }

    pub fn get_style(&self, style_name: Option<&str>) -> Option<crate::asc::semantics::meta_data::MetaData> {
        self.styles.borrow().get_style(style_name?).cloned()
    }

    pub fn get_styles(&self) -> std::cell::Ref<'_, super::css::styles::Styles> {
        self.styles.borrow()
    }

    /// The registered styles, shared with the MXML `TypeTable`.
    pub fn styles_handle(&self) -> Rc<RefCell<super::css::styles::Styles>> {
        self.styles.clone()
    }

    /// `getTypeAnalyzer()`; its calls take this symbol table.
    pub fn get_type_analyzer(&self) -> super::as3::binding::type_analyzer::TypeAnalyzerRef {
        self.type_analyzer.clone()
    }

    /// The class map, shared with the type tables.
    pub fn class_table(&self) -> super::as3::reflect::type_table::ClassTable {
        self.class_table.clone()
    }

    /// `getClassNames()`.
    pub fn get_class_names(&self) -> Vec<String> {
        self.class_table.borrow().keys().cloned().collect()
    }

    pub fn register_class(&mut self, class_name: &str, cls: Rc<AbcClass>) {
        self.class_table.borrow_mut().insert(class_name.to_string(), cls);
    }

    pub fn get_class(&self, class_name: Option<&str>) -> Option<Rc<AbcClass>> {
        super::as3::reflect::type_table::get_class_from(&self.class_table.borrow(), class_name)
    }

    /// `registerQNames(qNames, source)`.
    pub fn register_qnames(&mut self, q_names: &[QName], source: &SourceRef) {
        for q in q_names {
            self.q_name_table.insert(q.clone(), source.clone());
        }
    }

    pub fn register_qname(&mut self, q_name: &QName, source: &SourceRef) {
        if self.q_name_table.get(q_name).is_none() {
            self.q_name_table.insert(q_name.clone(), source.clone());
        }
    }

    pub fn find_source_by_qname(&self, q_name: &QName) -> Option<SourceRef> {
        self.q_name_table.get(q_name).cloned()
    }

    pub fn find_source_by_qname_parts(&self, namespace_uri: &str, local_part: &str) -> Option<SourceRef> {
        self.q_name_table.get(&QName::new(namespace_uri, local_part)).cloned()
    }

    pub fn register_multi_name(&mut self, multi_name: &MultiName, q_name: &QName) {
        self.multi_names.insert(multi_name.clone(), q_name.clone());
    }

    pub fn is_multi_name_resolved(&self, multi_name: &MultiName) -> Option<QName> {
        self.multi_names.get(multi_name).cloned()
    }

    pub fn find_source_by_resource_bundle_name(&self, rb_name: &str) -> Option<SourceRef> {
        self.rb_name_table.get(rb_name).cloned()
    }

    /// `isResourceBundleResolved(rbName)`.
    pub fn is_resource_bundle_resolved(&self, rb_name: &str) -> Option<Vec<QName>> {
        self.rb_names.get(rb_name).cloned().flatten()
    }

    /// `register(rbName, qNames, source)`.
    pub fn register_resource_bundle_name(&mut self, rb_name: &str, q_names: Option<Vec<QName>>, source: Option<&SourceRef>) {
        let Some(source) = source else { return };
        self.rb_names.insert(rb_name.to_string(), q_names.clone());
        self.rb_name_table.insert(rb_name.to_string(), source.clone());
        for q in q_names.iter().flatten() {
            self.register_qname(q, source);
        }
    }
}
