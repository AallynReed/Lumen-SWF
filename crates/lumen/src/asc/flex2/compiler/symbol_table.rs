//! Port of `flex2.compiler.SymbolTable`.

use std::cell::RefCell;
use std::collections::HashMap;
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
    class_table: HashMap<String, Rc<AbcClass>>,
    pub per_compile_data: Rc<RefCell<ContextStatics>>,
    q_name_table: JHashMap<QName, SourceRef>,
    multi_names: JHashMap<MultiName, QName>,
    pub emitter: EmitterRef,
    styles: super::css::styles::Styles,
    /// `getContext().getAttribute(StylesContainer.class.getName())`: the application's styles, made by `PreLink`.
    pub styles_container: Option<Rc<super::css::styles_container::StylesContainer>>,
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
            class_table: HashMap::new(),
            per_compile_data,
            q_name_table: JHashMap::with_capacity(300),
            multi_names: JHashMap::with_capacity(1024),
            emitter,
            styles: super::css::styles::Styles::default(),
            styles_container: None,
        }
    }

    /// `registerStyles(newStyles)`.
    pub fn register_styles(&mut self, new_styles: &super::css::styles::Styles) -> Result<(), super::css::styles::StyleConflictException> {
        self.styles.add_styles(new_styles)
    }

    pub fn get_style(&self, style_name: Option<&str>) -> Option<&crate::asc::semantics::meta_data::MetaData> {
        self.styles.get_style(style_name?)
    }

    pub fn get_styles(&self) -> &super::css::styles::Styles {
        &self.styles
    }

    /// `getClassNames()`.
    pub fn get_class_names(&self) -> Vec<String> {
        self.class_table.keys().cloned().collect()
    }

    pub fn register_class(&mut self, class_name: &str, cls: Rc<AbcClass>) {
        self.class_table.insert(class_name.to_string(), cls);
    }

    pub fn get_class(&self, class_name: Option<&str>) -> Option<Rc<AbcClass>> {
        let class_name = class_name?;
        if class_name == NOTYPE {
            return Some(Rc::new(AbcClass::no_type()));
        }
        self.class_table.get(class_name).cloned()
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
}
