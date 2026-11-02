//! Port of `flex2.compiler.mxml.rep.MxmlDocument`, the rep tree's root and `ClassDef.vm`'s `$doc`. `Rc` with
//! interior mutability; the template view is in `gen/template_values.rs`.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use super::at_embed::AtEmbed;
use super::at_resource::AtResource;
use super::binding_expression::{self, BindingExpression, BindingExpressionRef};
use super::decl::property_declaration::PropertyDeclaration;
use super::decl::uninitialized_property_declaration::UninitializedPropertyDeclaration;
use super::deps::{self, DesignLayerNode, MxmlConfiguration, StandardDefs, Type, TypeTable};
use super::document_info::{DocumentInfoRef, NameInfoSet, OUTER_DOCUMENT_PROP};
use super::init::initializer::InitializerRef;
use super::init::value_initializer::ValueInitializer;
use super::model::{add_definition_iterators, Model, ModelRef};
use super::script::Script;
use super::states_model::StatesModel;
use super::variable_declaration::VariableDeclaration;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::css::at_embed::AtEmbedKey;
use crate::asc::flex2::compiler::css::document::CssDocument;
use crate::asc::flex2::compiler::css::styles_container::StylesContainer;
use crate::asc::flex2::compiler::mxml::gen::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::mxml::gen::descriptor_generator;
use crate::asc::flex2::compiler::mxml::gen::line_number_map::DualModeLineNumberMap;
use crate::asc::flex2::compiler::mxml::gen::states_generator::StatesGenerator;
use crate::asc::flex2::compiler::mxml::gen::text_gen;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::{name_formatter, QName};
use crate::asc::util::jhash::{JHashMap, JHashSet};
use crate::asc::util::jstr;

pub type MxmlDocumentRef = Rc<MxmlDocument>;

/// A `TreeMap<String, ?>` key in `String.compareTo` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JKey(pub String);

impl PartialOrd for JKey {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for JKey {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        jstr::compare(&self.0, &o.0)
    }
}

pub struct MxmlDocument {
    self_ref: Weak<MxmlDocument>,
    unit: UnitRef,
    type_table: Rc<TypeTable>,
    info: DocumentInfoRef,
    standard_defs: Rc<StandardDefs>,
    root: RefCell<Option<ModelRef>>,
    declarations: RefCell<BTreeMap<JKey, PropertyDeclaration>>,
    binding_expressions: RefCell<Vec<BindingExpressionRef>>,
    layer_declarations: RefCell<Vec<PropertyDeclaration>>,
    at_embeds: RefCell<BTreeMap<JKey, Rc<AtEmbed>>>,
    at_resources: RefCell<BTreeMap<JKey, Rc<AtResource>>>,
    type_refs: RefCell<BTreeMap<JKey, ()>>,
    styles_container: Rc<StylesContainer>,
    preloader: RefCell<String>,
    use_preloader: Cell<bool>,
    line_number_map: RefCell<Option<Rc<DualModeLineNumberMap>>>,
    anon_id_counts: RefCell<JHashMap<String, i32>>,
    binding_imports_added: Cell<bool>,
    states_model: RefCell<Option<Rc<StatesModel>>>,
    stateful_event_initializers: RefCell<Vec<InitializerRef>>,
    comment: RefCell<Option<String>>,
    show_deprecation_warnings: bool,
    allow_duplicate_default_style_declarations: bool,
    design_layers: RefCell<Vec<(DesignLayerNode, ModelRef)>>,
}

impl MxmlDocument {
    pub fn new(unit: &UnitRef, type_table: &Rc<TypeTable>, info: &DocumentInfoRef, mxml_configuration: &MxmlConfiguration) -> MxmlDocumentRef {
        let standard_defs = info.get_standard_defs();
        let css_configuration = deps::css_configuration(mxml_configuration);
        let styles_container = Rc::new(StylesContainer::new(css_configuration, unit.clone(), type_table.get_per_compile_data()));
        unit.borrow_mut().styles_container = Some(styles_container.clone());
        let doc = Rc::new_cyclic(|w| MxmlDocument {
            self_ref: w.clone(),
            unit: unit.clone(),
            type_table: type_table.clone(),
            info: info.clone(),
            standard_defs: standard_defs.clone(),
            root: RefCell::new(None),
            declarations: RefCell::new(BTreeMap::new()),
            binding_expressions: RefCell::new(Vec::new()),
            layer_declarations: RefCell::new(Vec::new()),
            at_embeds: RefCell::new(BTreeMap::new()),
            at_resources: RefCell::new(BTreeMap::new()),
            type_refs: RefCell::new(BTreeMap::new()),
            styles_container: styles_container.clone(),
            preloader: RefCell::new(name_formatter::to_dot(&standard_defs.class_downloadprogressbar)),
            use_preloader: Cell::new(true),
            line_number_map: RefCell::new(None),
            anon_id_counts: RefCell::new(JHashMap::new()),
            binding_imports_added: Cell::new(false),
            states_model: RefCell::new(None),
            stateful_event_initializers: RefCell::new(Vec::new()),
            comment: RefCell::new(None),
            show_deprecation_warnings: deps::show_deprecation_warnings(mxml_configuration),
            allow_duplicate_default_style_declarations: deps::allow_duplicate_default_style_declarations(mxml_configuration),
            design_layers: RefCell::new(Vec::new()),
        });
        styles_container.set_mxml_document(Some(doc.clone() as Rc<dyn CssDocument>));
        styles_container.set_name_mappings(Some(type_table.get_name_mappings()));
        *doc.states_model.borrow_mut() = Some(Rc::new(StatesModel::new(&doc, info, standard_defs)));
        doc
    }

    pub fn self_ref(&self) -> MxmlDocumentRef {
        self.self_ref.upgrade().expect("document dropped")
    }

    pub fn get_layer_model(&self, node: &DesignLayerNode) -> Option<ModelRef> {
        self.design_layers.borrow().iter().find(|(n, _)| deps::same_node(n, node)).map(|(_, m)| m.clone())
    }

    pub fn add_layer_model(&self, node: &DesignLayerNode, model: &ModelRef) {
        let mut d = self.design_layers.borrow_mut();
        match d.iter_mut().find(|(n, _)| deps::same_node(n, node)) {
            Some(e) => e.1 = model.clone(),
            None => d.push((node.clone(), model.clone())),
        }
    }

    pub fn get_compilation_unit(&self) -> UnitRef {
        self.unit.clone()
    }

    pub fn get_source_path(&self) -> String {
        self.unit.borrow().get_source().borrow().get_name().to_string()
    }

    pub fn get_standard_defs(&self) -> Rc<StandardDefs> {
        self.standard_defs.clone()
    }

    pub fn get_is_main(&self) -> bool {
        self.unit.borrow().is_root()
    }

    pub fn get_type_table(&self) -> Rc<TypeTable> {
        self.type_table.clone()
    }

    pub fn get_info(&self) -> DocumentInfoRef {
        self.info.clone()
    }

    pub fn get_class_name(&self) -> String {
        self.info.get_class_name()
    }

    pub fn get_converted_class_name(&self) -> String {
        format!("_{}", self.get_class_name().replace('.', "_"))
    }

    pub fn get_package_name(&self) -> String {
        self.info.get_package_name()
    }

    pub fn get_qname(&self) -> QName {
        self.info.get_qname()
    }

    pub fn get_skeleton_class(&self) -> Option<Type> {
        self.type_table.get_type(&self.get_qname().to_string())
    }

    pub fn get_super_class(&self) -> Type {
        self.get_root().get_type().clone()
    }

    pub fn get_super_class_name(&self) -> String {
        name_formatter::to_dot(&self.get_super_class().get_name().to_string())
    }

    pub fn get_has_interfaces(&self) -> bool {
        !self.info.get_interface_names().is_empty()
    }

    pub fn get_is_inline_component(&self) -> bool {
        deps::document_node_is_inline_component(&self.info.get_root_node())
    }

    pub fn get_allow_duplicate_default_style_declarations(&self) -> bool {
        self.allow_duplicate_default_style_declarations
    }

    pub fn set_root(&self, root: &ModelRef) {
        *self.root.borrow_mut() = Some(root.clone());
        if self.get_is_container() {
            self.add_import(&name_formatter::to_dot(&self.standard_defs.class_uicomponentdescriptor), root.get_xml_line_number());
        }
        if let Some(outer_doc_class_name) = deps::document_node_outer_document_class_name(&self.info.get_root_node()) {
            self.add_declaration_named(OUTER_DOCUMENT_PROP, &outer_doc_class_name, 0, false, true, false, false, None);
        }
    }

    pub fn get_root(&self) -> ModelRef {
        self.root.borrow().clone().expect("root component not set")
    }

    /// `addDeclaration(Model model, boolean topLevel)`.
    pub fn add_declaration(&self, model: &ModelRef, top_level: bool) {
        let id = model.get_id().unwrap_or_default();
        if !self.inherited_property_usage_error(&id, model.get_type(), model.get_xml_line_number()) {
            if model.is_design_layer() {
                self.layer_declarations.borrow_mut().push(PropertyDeclaration::Initialized(ValueInitializer::new_initialized_property_declaration(model, top_level, model.get_xml_line_number())));
            }
            let decl = PropertyDeclaration::Initialized(ValueInitializer::new_initialized_property_declaration(model, top_level, model.get_xml_line_number()));
            self.declarations.borrow_mut().insert(JKey(id), decl);
        }
    }

    /// `addDeclaration(id, typeName, line, inspectable, topLevel, idIsAutogenerated, isBindable[, comment])`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_declaration_named(&self, id: &str, type_name: &str, line: i32, inspectable: bool, top_level: bool, id_is_autogenerated: bool, is_bindable: bool, comment: Option<&str>) {
        let root_type = self.get_root().get_type().clone();
        let t = root_type.get_type_table().get_type(&name_formatter::to_colon(type_name));
        if !self.inherited_property_usage_error_opt(id, t.as_ref(), line) {
            let decl = UninitializedPropertyDeclaration::new(id, type_name, line, inspectable, top_level, id_is_autogenerated, is_bindable, comment);
            self.declarations.borrow_mut().insert(JKey(id.to_string()), PropertyDeclaration::Uninitialized(Rc::new(decl)));
        }
    }

    pub fn get_declaration(&self, id: &str) -> Option<PropertyDeclaration> {
        self.declarations.borrow().get(&JKey(id.to_string())).cloned()
    }

    pub fn add_state_specific_event_initializer(&self, initializer: &InitializerRef) {
        if initializer.as_event().is_some_and(|e| e.get_handler_text() != "@Clear()") {
            self.stateful_event_initializers.borrow_mut().push(initializer.clone());
        }
    }

    pub fn ensure_declaration(&self, model: &ModelRef) {
        if !self.is_declared(model) {
            self.add_declaration(model, false);
        }
    }

    pub fn is_declared(&self, model: &Model) -> bool {
        model.get_id().is_some_and(|id| self.is_declared_id(&id))
    }

    pub fn is_declared_id(&self, id: &str) -> bool {
        self.declarations.borrow().contains_key(&JKey(id.to_string()))
    }

    pub fn show_deprecation_warnings(&self) -> bool {
        self.show_deprecation_warnings
    }

    /// `getDeclarationIterator()`: declarations the superclass doesn't already have as properties.
    pub fn get_declaration_iterator(&self) -> Vec<PropertyDeclaration> {
        let super_type = self.get_super_class();
        let decls: Vec<PropertyDeclaration> = self.declarations.borrow().values().cloned().collect();
        decls.into_iter().filter(|d| super_type.get_property(&d.get_name()).is_none()).collect()
    }

    fn get_top_level_declaration_iterator(&self) -> Vec<PropertyDeclaration> {
        let decls: Vec<PropertyDeclaration> = self.declarations.borrow().values().cloned().collect();
        decls.into_iter().filter(|d| d.get_top_level()).collect()
    }

    fn root_and_top_level_property_initializers(&self) -> Vec<InitializerRef> {
        let mut v = self.get_root().get_property_initializer_iterator_bindings(false);
        v.extend(self.get_top_level_initializer_iterator());
        v
    }

    pub fn get_non_stage_property_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.root_and_top_level_property_initializers()
            .into_iter()
            .filter(|i| !i.is_named() || (!deps::is_stage_property(&i.get_name()) && !i.is_design_layer()))
            .collect()
    }

    pub fn get_design_layer_property_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.layer_declarations.borrow().iter().filter_map(|d| d.as_initializer().cloned()).collect()
    }

    pub fn get_stage_property_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.root_and_top_level_property_initializers().into_iter().filter(|i| i.is_named() && deps::is_stage_property(&i.get_name())).collect()
    }

    pub fn get_has_stage_property_initializers(&self) -> bool {
        !self.get_stage_property_initializer_iterator().is_empty()
    }

    pub fn get_top_level_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.get_top_level_declaration_iterator().iter().filter_map(|d| d.as_initializer().cloned()).collect()
    }

    pub fn get_property_initializer_iterator(&self) -> Vec<InitializerRef> {
        self.root_and_top_level_property_initializers()
    }

    pub fn get_stateful_event_iterator(&self) -> Vec<InitializerRef> {
        self.stateful_event_initializers.borrow().clone()
    }

    /// `getProceduralVisualChildInitializerIterator()`: the root's visual children not described or state-specific.
    pub fn get_procedural_visual_child_initializer_iterator(&self) -> Vec<InitializerRef> {
        let root = self.get_root();
        if root.is_movie_clip() {
            root.get_child_initializer_iterator()
                .into_iter()
                .filter(|init| match init.as_value().and_then(|v| v.get_model()) {
                    Some(m) => !m.is_described() && !m.is_state_specific(),
                    None => true,
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn get_describe_visual_children(&self) -> bool {
        true
    }

    pub fn get_definition_iterator(&self) -> Vec<CodeFragmentList> {
        let mut iter_list = Vec::new();
        add_definition_iterators(&mut iter_list, &self.get_top_level_initializer_iterator());
        let stateful = self.stateful_event_initializers.borrow().clone();
        for i in &stateful {
            iter_list.extend(i.get_definitions_iterator());
        }
        iter_list.extend(self.get_root().get_sub_definitions_iterator());
        if self.get_version() >= 4 {
            iter_list.extend(self.get_stateful_model().get_sub_definition_iterators());
        }
        iter_list
    }

    pub fn add_binding_expression(&self, expr: &BindingExpressionRef) {
        let size = self.binding_expressions.borrow().len() as i32;
        expr.set_id(size);
        self.binding_expressions.borrow_mut().push(expr.clone());
        self.info.add_interface_name(&self.standard_defs.interface_ibindingclient_dot, -1);
    }

    pub fn remove_binding_expression(&self, expr: &BindingExpressionRef) {
        let index = self.binding_expressions.borrow().iter().position(|b| b.get_id() == expr.get_id());
        if let Some(index) = index {
            let list = self.binding_expressions.borrow().clone();
            for (i, b) in list.iter().enumerate().skip(index) {
                b.set_id(i as i32 - 1);
            }
            self.binding_expressions.borrow_mut().remove(index);
        }
    }

    pub fn get_binding_expressions(&self) -> Vec<BindingExpressionRef> {
        self.binding_expressions.borrow().clone()
    }

    pub fn add_at_embed(&self, at_embed: &Rc<AtEmbed>) {
        let key = JKey(at_embed.get_prop_name().to_string());
        self.at_embeds.borrow_mut().entry(key).or_insert_with(|| at_embed.clone());
    }

    /// `getAtEmbeds()`: ours and the styles container's, in `HashSet` order.
    pub fn get_at_embeds(&self) -> Vec<Rc<AtEmbed>> {
        let values: Vec<AtEmbedKey> = self.at_embeds.borrow().values().map(|e| AtEmbedKey(e.clone())).collect();
        let n = values.len();
        let mut result = JHashSet::from_collection(values, n);
        for e in self.styles_container.get_at_embeds().iter() {
            result.insert(e.clone());
        }
        result.iter().map(|k| k.0.clone()).collect()
    }

    pub fn add_at_resource(&self, at_resource: &Rc<AtResource>) -> bool {
        self.add_import(&self.standard_defs.class_resourcemanager_dot, at_resource.get_xml_line_number());
        self.at_resources.borrow_mut().insert(JKey(at_resource.get_bundle().to_string()), at_resource.clone());
        true
    }

    pub fn get_at_resources(&self) -> Vec<Rc<AtResource>> {
        self.at_resources.borrow().values().cloned().collect()
    }

    pub fn add_type_ref(&self, type_ref: &str, line: i32) {
        self.add_import(type_ref, line);
        self.type_refs.borrow_mut().insert(JKey(type_ref.to_string()), ());
    }

    pub fn get_type_refs(&self) -> Vec<String> {
        self.type_refs.borrow().keys().map(|k| k.0.clone()).collect()
    }

    pub fn add_import(&self, name: &str, line: i32) {
        self.info.add_import_name(name, line);
    }

    pub fn get_imports(&self) -> NameInfoSet {
        self.ensure_binding_imports();
        self.info.get_import_names()
    }

    pub fn get_split_imports(&self) -> Vec<Vec<String>> {
        self.info.get_split_import_names()
    }

    fn ensure_binding_imports(&self) {
        if !self.binding_imports_added.get() {
            for expr in self.get_binding_expressions() {
                self.add_import(&expr.get_destination_type_name(), expr.get_xml_line_number());
            }
            self.binding_imports_added.set(true);
        }
    }

    pub fn add_script(&self, script: Script) {
        self.info.add_script(script);
    }

    pub fn get_scripts(&self) -> Vec<Script> {
        self.info.get_scripts()
    }

    pub fn get_local_class(&self, namespace: &str, local_part: &str) -> Option<String> {
        self.info.get_local_class(namespace, local_part)
    }

    pub fn add_metadata(&self, meta_data_source: Script) {
        if !meta_data_source.get_text().starts_with('[') {
            self.info.add_metadata_first(meta_data_source);
        } else {
            self.info.add_metadata(meta_data_source);
        }
    }

    pub fn get_metadata(&self) -> Vec<Script> {
        self.info.get_metadata()
    }

    pub fn get_styles_container(&self) -> Rc<StylesContainer> {
        self.styles_container.clone()
    }

    /// `getInheritingStyleNameIterator()`.
    pub fn get_inheriting_style_name_iterator(&self) -> Vec<String> {
        let styles = self.type_table.get_styles();
        styles.get_style_names().into_iter().filter(|n| styles.is_inheriting_style(n)).collect()
    }

    pub fn get_is_container(&self) -> bool {
        self.standard_defs.is_container(self.get_root().get_type())
    }

    pub fn get_is_visual_element_container(&self) -> bool {
        self.get_root().get_type().is_assignable_to_name(Some(&self.standard_defs.interface_ivisualelementcontainer))
    }

    pub fn get_is_iflex_module(&self) -> bool {
        self.standard_defs.is_iflex_module(self.get_root().get_type())
    }

    pub fn get_is_iui_component(&self) -> bool {
        self.standard_defs.is_iui_component(self.get_root().get_type())
    }

    pub fn get_is_flex_application(&self) -> bool {
        self.get_is_main() && (self.get_is_container() || (self.get_version() >= 4 && self.get_is_simple_style_component()))
    }

    pub fn get_is_simple_style_component(&self) -> bool {
        self.standard_defs.is_simple_style_component(self.get_root().get_type())
    }

    pub fn set_line_number_map(&self, line_number_map: Option<Rc<DualModeLineNumberMap>>) {
        *self.line_number_map.borrow_mut() = line_number_map;
    }

    pub fn get_line_number_map(&self) -> Option<Rc<DualModeLineNumberMap>> {
        self.line_number_map.borrow().clone()
    }

    pub fn set_preloader(&self, preloader: &str) {
        *self.preloader.borrow_mut() = preloader.to_string();
    }

    pub fn get_preloader(&self) -> String {
        self.preloader.borrow().clone()
    }

    pub fn set_use_preloader(&self, use_preloader: bool) {
        self.use_preloader.set(use_preloader);
    }

    pub fn get_use_preloader(&self) -> bool {
        self.use_preloader.get()
    }

    pub fn get_language_namespace(&self) -> Option<String> {
        self.info.get_language_namespace()
    }

    pub fn get_version(&self) -> i32 {
        self.info.get_version()
    }

    pub fn super_has_public_property(&self, name: &str) -> bool {
        self.get_super_class().get_property(name).is_some()
    }

    /// Package-private `ensureId(model)`: an anonymous model's `_<Class>_<Type><n>` id.
    pub(crate) fn ensure_id(&self, model: &Model) {
        if model.get_id().is_none() {
            let type_ = model.get_type();
            let i = self.get_anon_index(type_);
            let type_name = type_.get_name().to_string().replace('<', "_").replace('>', "_");
            let id = format!("_{}_{}{}", name_formatter::to_dot(&self.info.get_class_name()).replace('.', "_"), name_formatter::retrieve_class_name(&type_name), i);
            model.set_id(Some(&id), true);
        }
    }

    fn get_anon_index(&self, type_: &Type) -> i32 {
        let type_name = name_formatter::retrieve_class_name(&type_.get_name().to_string());
        let mut counts = self.anon_id_counts.borrow_mut();
        let i = counts.get(&type_name).copied().unwrap_or(1);
        counts.insert(type_name, i + 1);
        i
    }

    pub fn resolve_two_way_bindings(&self) {
        self.complete_two_way_bindings();
        let mut destination_map: JHashMap<String, BindingExpressionRef> = JHashMap::new();
        for binding_expression in self.get_binding_expressions() {
            let source_expression = text_gen::strip_parens(&binding_expression.get_source_expression());
            let destination_path = binding_expression.get_destination_path(false);
            let matched = destination_map.get(&source_expression).cloned();
            match matched {
                Some(m) if destination_path == text_gen::strip_parens(&m.get_source_expression()) => binding_expression.set_two_way_counterpart(Some(&m)),
                _ => {
                    destination_map.insert(destination_path, binding_expression.clone());
                }
            }
        }
    }

    fn complete_two_way_bindings(&self) {
        let array = self.get_binding_expressions();
        for binding_expression in array {
            if binding_expression.is_two_way_primary() {
                if let Some(destination) = binding_expression.get_destination() {
                    destination.ensure_bindable();
                }
                let source2 = binding_expression.get_destination_path(false);
                let binding_expression2 = BindingExpression::new(&source2, binding_expression.get_xml_line_number(), &self.self_ref());
                let destination2 = text_gen::strip_parens(&binding_expression.get_source_expression());
                binding_expression2.set_destination_property(Some(&destination2));
                binding_expression2.set_destination_lvalue(Some(&destination2));
            }
        }
    }

    /// `getAllBindingNamespaces()`, in insertion order (Java's `HashMap` order is applied when printed).
    pub fn get_all_binding_namespaces(&self) -> Vec<(i32, String)> {
        let mut all = Vec::new();
        for be in self.get_binding_expressions() {
            if let Some(ns) = be.get_namespaces() {
                binding_expression::merge_namespaces(&mut all, &ns);
            }
        }
        all
    }

    pub fn has_binding_tags(&self) -> bool {
        self.get_binding_expressions().iter().any(|b| b.get_destination().is_none())
    }

    pub fn get_all_binding_namespace_declarations(&self) -> String {
        BindingExpression::get_namespace_declarations_of(Some(&self.get_all_binding_namespaces()))
    }

    pub fn post_process_states(&self) {
        let states_model = self.get_stateful_model();
        states_model.apply_metadata();
        if states_model.process_reparents() {
            states_model.process_stateful_models();
            states_model.set_initial_state();
        }
    }

    pub fn get_interface_list(&self) -> String {
        text_gen::to_comma_list(self.info.get_interface_names().iter().map(|n| n.get_name().to_string()))
    }

    pub fn get_interface_names(&self) -> NameInfoSet {
        self.info.get_interface_names()
    }

    pub fn get_watcher_setup_util_class_name(&self) -> String {
        let mut s = String::from("_");
        let package_name = self.get_package_name();
        if !package_name.is_empty() {
            s.push_str(&package_name.replace('.', "_"));
            s.push('_');
        }
        s.push_str(&self.get_class_name());
        s.push_str("WatcherSetupUtil");
        s
    }

    pub fn get_descriptor_declaration(&self, name: &str) -> CodeFragmentList {
        let mut frag_list = CodeFragmentList::new();
        descriptor_generator::add_descriptor_initializer_fragments_props(&mut frag_list, &self.get_root(), &deps::required_top_level_descriptor_properties(), true, "");
        frag_list.add_at(0, &format!("private var {name} : {} = ", name_formatter::to_dot(&self.standard_defs.class_uicomponentdescriptor)), 0);
        frag_list
    }

    pub fn get_states_declaration(&self) -> CodeFragmentList {
        let mut generator = StatesGenerator::new(self.standard_defs.clone());
        if self.get_version() >= 4 {
            generator.get_states_initializer_fragments(&self.get_stateful_model())
        } else {
            CodeFragmentList::new()
        }
    }

    fn inherited_property_usage_error(&self, name: &str, type_: &Type, line: i32) -> bool {
        self.inherited_property_usage_error_opt(name, Some(type_), line)
    }

    /// `inheritedPropertyUsageError(name, type, line)`; Java dereferences a null `type` only when the property exists.
    fn inherited_property_usage_error_opt(&self, name: &str, type_: Option<&Type>, line: i32) -> bool {
        let root = self.get_root();
        if let Some(prop) = root.get_type().get_property(name) {
            let path = self.get_source_path();
            if !prop.has_public() {
                toolkit::log(CompilerMessage::error(format!("Attempting to initialize non-public inherited property '{name}' from MXML.")), Some(&path), line, -1, None);
                return true;
            }
            if prop.read_only() {
                toolkit::log(CompilerMessage::error(format!("Attempting to initialize read-only inherited property '{name}' from MXML.")), Some(&path), line, -1, None);
                return true;
            }
            let type_ = type_.expect("NullPointerException");
            let prop_type = prop.get_type().expect("NullPointerException");
            if !type_.is_assignable_to(Some(&prop_type)) {
                toolkit::log(
                    CompilerMessage::error(format!(
                        "Attempting to initialize inherited property '{name}' of type '{}' with value of incompatible type '{}'.",
                        name_formatter::to_dot(&prop_type.get_name()),
                        name_formatter::to_dot(&type_.get_name().to_string())
                    )),
                    Some(&path),
                    line,
                    -1,
                    None,
                );
                return true;
            }
        }
        false
    }

    fn unresolved_state_identifier(&self, states: &[String], line: i32) -> bool {
        states.iter().any(|s| !self.validate_state(s, line))
    }

    pub fn validate_state(&self, state: &str, line: i32) -> bool {
        if !self.get_stateful_model().validate_state(Some(state)) {
            toolkit::log(CompilerMessage::error(format!("State '{state}' was referenced without being declared.")), Some(&self.get_source_path()), line, -1, None);
            return false;
        }
        true
    }

    pub fn validate_state_filters(&self, include: &[String], exclude: &[String], line: i32) -> bool {
        if !include.is_empty() && !exclude.is_empty() {
            toolkit::log(CompilerMessage::error("The includeIn and excludeFrom keywords may not be specified concurrently."), Some(&self.get_source_path()), line, -1, None);
            return false;
        }
        !(self.unresolved_state_identifier(include, line) || self.unresolved_state_identifier(exclude, line))
    }

    pub fn get_layer_declaration_nodes(&self) -> Vec<DesignLayerNode> {
        deps::document_node_layer_declaration_nodes(&self.info.get_root_node())
    }

    pub fn get_binding_management_vars() -> Vec<VariableDeclaration> {
        deps::binding_management_vars()
    }

    pub fn register_state_specific_node(&self, model: &ModelRef, node: &deps::Node, included_states: &[String], excluded_states: &[String]) {
        if self.validate_state_filters(included_states, excluded_states, model.get_xml_line_number()) {
            self.get_stateful_model().register_state_specific_node(model, node, included_states, excluded_states);
        }
    }

    pub fn register_state_specific_property(&self, model: Option<&ModelRef>, property: &str, value: &InitializerRef, state_name: Option<&str>) {
        self.get_stateful_model().register_state_specific_property(model, property, value, state_name);
    }

    pub fn register_state_specific_style(&self, model: Option<&ModelRef>, property: &str, value: &InitializerRef, state_name: Option<&str>) {
        self.get_stateful_model().register_state_specific_style(model, property, value, state_name);
    }

    pub fn register_state_specific_event_handler(&self, model: Option<&ModelRef>, event: &str, value: &InitializerRef, state_name: Option<&str>) {
        self.get_stateful_model().register_state_specific_event_handler(model, event, value, state_name);
    }

    pub fn register_state(&self, model: &ModelRef, node: &deps::Node) {
        self.get_stateful_model().register_state(model, node);
    }

    pub fn get_stateful_model(&self) -> Rc<StatesModel> {
        self.states_model.borrow().clone().expect("statesModel")
    }

    pub fn register_early_init_node(&self, model: &ModelRef) {
        self.get_stateful_model().register_early_init_node(model);
    }

    pub fn get_comment(&self) -> Option<String> {
        self.comment.borrow().clone()
    }

    pub fn set_comment(&self, comment: Option<&str>) {
        *self.comment.borrow_mut() = comment.map(Into::into);
    }
}

impl CssDocument for MxmlDocument {
    fn get_qname(&self) -> String {
        MxmlDocument::get_qname(self).to_string()
    }

    fn add_import(&self, name: &str, line: i32) {
        MxmlDocument::add_import(self, name, line)
    }

    fn add_at_embed(&self, at_embed: Rc<AtEmbed>) {
        MxmlDocument::add_at_embed(self, &at_embed)
    }

    fn root_type_has_property(&self, name: &str) -> bool {
        self.root.borrow().as_ref().is_some_and(|r| r.get_type().get_property(name).is_some())
    }

    fn add_type_ref(&self, name: &str, line: i32) {
        MxmlDocument::add_type_ref(self, name, line)
    }

    fn effects_package(&self) -> String {
        self.standard_defs.get_effects_package().to_string()
    }
}
