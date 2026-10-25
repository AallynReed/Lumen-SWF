//! Port of `flex2.compiler.as3.As3Compiler`, mxmlc's ActionScript `SubCompiler`, running its registered
//! compiler extensions in every phase.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::evaluator_adapter;
use super::inheritance_evaluator::InheritanceEvaluator;
use super::syntax_tree_evaluator::{normalize_class_name, SyntaxTreeEvaluator};
use super::type_table::AbcClass;
use crate::asc::embedding::lint_evaluator::LintEvaluator;
use crate::asc::emit::bytecode_emitter::new_bytecode_emitter_full;
use crate::asc::emit::global_builder::new_global_builder;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::source::{Source, SourceRef};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, Level};
use crate::asc::flex2::compiler::util::{mime_mappings, name_formatter, MultiName, MultiNameMap, Name, QName, QNameList};
use crate::asc::parser::evaluator::evaluate;
use crate::asc::parser::meta_data_evaluator::MetaDataEvaluator;
use crate::asc::parser::node::{NodeRef, ProgramNode};
use crate::asc::parser::parser::Parser;
use crate::asc::semantics::code_generator::CodeGenerator;
use crate::asc::semantics::configuration_evaluator::ConfigurationEvaluator;
use crate::asc::semantics::constant_evaluator::ConstantEvaluator;
use crate::asc::semantics::emitter::EmitterRef;
use crate::asc::semantics::flow_analyzer::{inherit_context_slots, FlowAnalyzer};
use crate::asc::semantics::flow_graph_emitter::FlowGraphEmitter;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::util::context::{CompilerHandler, Context, ContextRef, ContextStatics, NS_INTERNAL, NS_PUBLIC};
use crate::asc::util::jhash::JHashSet;

/// `flex2.compiler.SubCompiler`.
pub trait SubCompiler {
    fn get_name(&self) -> &'static str;
    fn is_supported(&self, mime_type: &str) -> bool;
    fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        Some(source)
    }
    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef>;
    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable);
    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable);
    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable);
    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable);
    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable);
    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable);
    fn postprocess(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}
}

/// `As3Compiler.CompilerHandler`: ASC's messages go to the current logger.
pub struct As3CompilerHandler;

impl CompilerHandler for As3CompilerHandler {
    fn error(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str, code: i32) {
        toolkit::log_asc(Level::Error, filename, ln, col, msg, source, code);
    }
    fn warning(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str, code: i32) {
        toolkit::log_asc(Level::Warning, filename, ln, col, msg, source, code);
    }
    fn error_no_code(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str) {
        toolkit::log_asc(Level::Error, filename, ln, col, msg, source, -1);
    }
    fn warning_no_code(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str) {
        toolkit::log_asc(Level::Warning, filename, ln, col, msg, source, -1);
    }
}

pub fn program_state(node: &NodeRef) -> i32 {
    node.borrow().program().expect("ClassCastException").state
}

pub fn program_frame(node: &NodeRef) -> ObjectValue {
    node.borrow().program().expect("ClassCastException").frame.clone().expect("NullPointerException")
}

/// A new `Context` sharing the symbol table's statics, set up as `parse1` does.
pub fn new_unit_context(source: &SourceRef, symbol_table: &SymbolTable, handler: Rc<RefCell<dyn CompilerHandler>>) -> ContextRef {
    let cx = Rc::new(RefCell::new(Context::new(symbol_table.per_compile_data.clone())));
    {
        let s = source.borrow();
        let c = cx.borrow();
        c.set_script_name(Some(s.get_name().to_string()));
        c.set_path(s.get_parent());
        c.set_emitter(Some(symbol_table.emitter.clone()));
    }
    cx.borrow_mut().set_handler(handler);
    symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
    cx
}

/// `As3Compiler.cleanNodeFactory`.
pub fn clean_node_factory(cx: &ContextRef) {
    let nf = Context::get_node_factory(cx);
    let mut nf = nf.borrow_mut();
    nf.pkg_defs.clear();
    nf.pkg_names.clear();
    nf.compound_names.clear();
    nf.current_package = None;
    nf.dxns = None;
    nf.use_stmts = None;
}

/// `As3Compiler.cleanSlots`.
pub fn clean_slots(ov: Option<&ObjectValue>, cx: Option<&ContextRef>, definitions: &QNameList) {
    if let Some(ov) = ov {
        ov.deferred_class_map_clear();
        for slot in ov.slots().unwrap_or_default() {
            slot.set_impl_node(None);
        }
    }
    if let Some(cx) = cx {
        for d in definitions.iter() {
            let value = cx.borrow().user_defined(&d.to_string());
            if let Some(value) = value {
                for slot in value.slots().unwrap_or_default() {
                    ContextStatics::clean_slot(Some(&slot));
                }
                if let Some(proto) = value.prototype() {
                    for slot in proto.slots().unwrap_or_default() {
                        ContextStatics::clean_slot(Some(&slot));
                    }
                }
            }
        }
    }
}

/// `inheritSlots(unit, types, symbolTable)`, shared with `AbcCompiler`.
pub fn inherit_slots(unit: &UnitRef, which: fn(&crate::asc::flex2::compiler::compilation_unit::CompilationUnit) -> &JHashSet<Name>, symbol_table: &SymbolTable) {
    let (node, cx, names) = {
        let u = unit.borrow();
        (u.get_syntax_tree().unwrap(), u.context.get_asc_context().unwrap(), which(&u).iter().cloned().collect::<Vec<Name>>())
    };
    let frame = program_frame(&node);
    let builder = frame.builder().expect("NullPointerException");
    for name in names {
        let Name::Q(q) = name else { continue };
        let s = symbol_table.find_source_by_qname(&q).expect("NullPointerException");
        let u = s.borrow().get_compilation_unit().expect("NullPointerException");
        if Rc::ptr_eq(&u, unit) {
            continue;
        }
        let other = u.borrow().type_info.clone();
        let sname = s.borrow().get_name().to_string();
        let processed = unit.borrow().context.processed.as_ref().is_some_and(|p| p.contains(&sname));
        if let (Some(other), false) = (other, processed) {
            inherit_context_slots(&other, &frame, &builder, &cx.borrow());
            unit.borrow_mut().context.processed.get_or_insert_with(HashSet::new).insert(sname);
        }
    }
}

/// Drains one of a `ProgramNode`'s unresolved sets. Java iterates them in identity-hash order, which mxmlc's output
/// depends on when two names share a bucket of the unit's dependency set. The most frequently referenced name comes
/// first, as it most often does in Java, then source order; references at position 0 (generated nodes, such as an
/// `[Embed]` variable's initializer) in creation order.
pub fn drain_ordered(set: &mut std::collections::HashSet<ReferenceValue>) -> Vec<ReferenceValue> {
    let key = |r: &ReferenceValue| (r.name(), r.data().namespaces.to_vec().iter().map(|n| n.name()).collect::<Vec<_>>());
    let mut v: Vec<(usize, i32, (String, Vec<String>), ReferenceValue)> = set.drain().map(|r| (0, r.get_position(), key(&r), r)).collect();
    let mut counts: std::collections::HashMap<(String, Vec<String>), usize> = std::collections::HashMap::new();
    for e in &v {
        *counts.entry(e.2.clone()).or_default() += 1;
    }
    for e in &mut v {
        e.0 = counts[&e.2];
    }
    v.sort_by(|a, b| {
        b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then_with(|| if a.1 == 0 && b.1 == 0 { a.3.data().seq.cmp(&b.3.data().seq) } else { a.2.cmp(&b.2) })
    });
    v.into_iter().map(|e| e.3).collect()
}

/// `transferDependencies(unresolved, target, history)`.
pub fn transfer_dependencies(unresolved: Vec<ReferenceValue>, target: &mut JHashSet<Name>, history: &MultiNameMap) {
    for r in unresolved {
        let mut ns_list: Vec<String> = Vec::new();
        let namespaces = r.data().namespaces.to_vec();
        for ov in namespaces {
            let mut ns: Option<String> = None;
            let mut ns_kind = 0u8;
            if ov.is_unresolved_namespace() {
                let (resolved, ref_name) = ov.with_unresolved(|u| (u.resolved, u.ref_.as_ref().map(|r| r.name())));
                ns = if resolved { Some(ov.name()) } else { ref_name };
                ns_kind = ov.get_namespace_kind();
            } else if ov.is_namespace_value() {
                ns = Some(ov.name());
                ns_kind = ov.get_namespace_kind();
            }
            if let Some(ns) = ns {
                if (ns_kind == NS_PUBLIC || ns_kind == NS_INTERNAL) && !ns_list.contains(&ns) {
                    ns_list.push(ns);
                }
            }
        }
        let name = r.name();
        if !name.contains('.') && !history.contains_key(&ns_list, &name) {
            target.insert(Name::M(MultiName::with_namespaces(ns_list, &name)));
        }
    }
}

/// `typeTable.createClasses(node.clsdefs, unit.topLevelDefinitions)`, registered as `analyze4` does.
pub fn register_classes(unit: &UnitRef, node: &NodeRef, symbol_table: &mut SymbolTable) {
    let clsdefs: Vec<NodeRef> = node.borrow().program().unwrap().clsdefs.as_ref().map(|l| l.borrow().clone()).unwrap_or_default();
    for clsdef in clsdefs {
        let q = {
            let b = clsdef.borrow();
            let c = b.class_definition().unwrap();
            c.cframe.as_ref().and_then(|f| f.builder()).and_then(|bu| bu.classname())
        };
        let Some(q) = q else { continue };
        let ns = q.ns().map(|n| n.name()).unwrap_or_default();
        if unit.borrow().top_level_definitions.contains(&ns, q.name()) {
            let cls = Rc::new(AbcClass::from_class_definition(&clsdef));
            symbol_table.register_class(&cls.name.clone(), cls.clone());
            unit.borrow_mut().class_table.insert(cls.name.clone(), cls);
        }
    }
}

/// `As3Compiler.evaluateLoaderClassBase`.
pub fn evaluate_loader_class_base(unit: &UnitRef, symbol_table: &SymbolTable) {
    let defs: Vec<QName> = unit.borrow().top_level_definitions.iter().cloned().collect();
    for q in defs {
        let Some(c) = symbol_table.get_class(Some(&q.to_string())) else { continue };
        if let Some(super_type_name) = &c.super_type_name {
            if let Some(sc) = symbol_table.get_class(Some(&name_formatter::to_colon(super_type_name))) {
                let lookup = |n: &str| symbol_table.get_class(Some(n));
                let inherited = sc.get_meta_data("Frame", true, &lookup);
                let lc = inherited.iter().find_map(|md| md.get_value_key("factoryClass"));
                unit.borrow_mut().loader_class_base = lc.map(|lc| normalize_class_name(&lc));
            }
        }
    }
}

pub struct As3Compiler {
    config: Rc<Configuration>,
    warn_map: std::collections::HashMap<i32, bool>,
    compiler_extensions: Vec<Box<dyn super::extension::Extension>>,
}

impl As3Compiler {
    pub fn new(config: Rc<Configuration>) -> As3Compiler {
        let warn_map = Self::process_coach_settings(&config);
        As3Compiler { config, warn_map, compiler_extensions: Vec::new() }
    }

    pub fn add_compiler_extension(&mut self, ext: Box<dyn super::extension::Extension>) {
        self.compiler_extensions.push(ext);
    }

    /// Runs one phase of every extension, stopping at the first error as each `As3Compiler` phase does.
    fn run_extensions(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable, phase: fn(&mut dyn super::extension::Extension, &UnitRef, &mut SymbolTable)) -> bool {
        for ext in self.compiler_extensions.iter_mut() {
            phase(ext.as_mut(), unit, symbol_table);
            if toolkit::error_count() > 0 {
                return false;
            }
        }
        true
    }

    /// `processCoachSettings()` with `CompilerConfiguration`'s `warn-*` defaults.
    fn process_coach_settings(config: &Configuration) -> std::collections::HashMap<i32, bool> {
        use crate::asc::embedding::warning_constants::*;
        let mut m = LintEvaluator::get_warning_defaults();
        let dep = config.show_deprecation_warnings;
        for (k, v) in [
            (kWarning_ArrayToStringChanges, false),
            (kWarning_AssignmentWithinConditional, true),
            (kWarning_BadArrayCast, true),
            (kWarning_BadBoolAssignment, true),
            (kWarning_BadDateCast, true),
            (kWarning_BadES3TypeMethod, true),
            (kWarning_BadES3TypeProp, true),
            (kWarning_BadNaNComparision, true),
            (kWarning_BadNullAssignment, true),
            (kWarning_BadNullComparision, true),
            (kWarning_BadUndefinedComparision, true),
            (kWarning_BooleanConstructorWithNoArgs, false),
            (kWarning_ChangesInResolve, false),
            (kWarning_ClassIsSealed, false),
            (kWarning_ConstNotInitialized, true),
            (kWarning_ConstructorReturnsValue, false),
            (kWarning_Deprecated, dep),
            (kWarning_DeprecatedMessage, dep),
            (kWarning_DeprecatedUseReplacement, dep),
            (kWarning_DeprecatedSince, dep),
            (kWarning_DeprecatedSinceNoReplacement, dep),
            (kWarning_DepricatedEventHandlerError, false),
            (kWarning_DepricatedFunctionError, false),
            (kWarning_DepricatedPropertyError, false),
            (kWarning_DuplicateArgumentNames, true),
            (kWarning_DuplicateVariableDef, true),
            (kWarning_ForVarInChanges, false),
            (kWarning_ImportHidesClass, true),
            (kWarning_InstanceOfChanges, true),
            (kWarning_InternalError, true),
            (kWarning_LevelNotSupported, false),
            (kWarning_MissingNamespaceDecl, true),
            (kWarning_NegativeUintLiteral, true),
            (kWarning_NoConstructor, true),
            (kWarning_NoExplicitSuperCallInConstructor, false),
            (kWarning_NoTypeDecl, true),
            (kWarning_NumberFromStringChanges, false),
            (kWarning_ScopingChangeInThis, false),
            (kWarning_SlowTextFieldAddition, true),
            (kWarning_UnlikelyFunctionValue, true),
            (kWarning_XML_ClassHasChanged, false),
        ] {
            m.insert(k, v);
        }
        m
    }
}

fn is_not_private(attrs: &Option<NodeRef>) -> bool {
    let Some(attrs) = attrs else { return true };
    let items: Vec<NodeRef> = attrs.borrow().as_attribute_list().map(|l| l.items.iter().flatten().cloned().collect()).unwrap_or_default();
    !items.iter().any(|n| n.borrow().has_attribute("private"))
}

fn package_definition_name(pkgdef: &Option<NodeRef>) -> String {
    let Some(p) = pkgdef else { return String::new() };
    let p = p.borrow();
    let Some(name) = p.as_package_definition().and_then(|p| p.name.clone()) else { return String::new() };
    let name = name.borrow();
    let Some(id) = name.as_package_name().and_then(|n| n.id.clone()) else { return String::new() };
    let id = id.borrow();
    let list = &id.as_package_identifiers().unwrap().list;
    let parts: Vec<String> = list.iter().flatten().map(|n| n.borrow().identifier().map(|i| i.name.clone()).unwrap_or_default()).collect();
    parts.join(".")
}

fn ident_name(n: &Option<NodeRef>) -> String {
    n.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default()
}

/// `transferDefinitions(topLevelDefinitions, definitions)`.
fn transfer_definitions(top: &mut QNameList, definitions: &[NodeRef]) {
    for n in definitions {
        let b = n.borrow();
        if let Some(c) = b.class_definition() {
            if b.as_class_definition().is_some() || b.as_interface_definition().is_some() || b.binary_class_def().is_some() {
                if is_not_private(&c.def.attrs) {
                    top.add(QName::new(&package_definition_name(&c.def.pkgdef), &ident_name(&c.name)));
                }
            }
        } else if let Some(ns) = b.namespace_definition() {
            if is_not_private(&ns.def.attrs) && b.as_config_namespace_definition().is_none() {
                top.add(QName::new(&package_definition_name(&ns.def.pkgdef), &ident_name(&ns.name)));
            }
        } else if let Some(f) = b.function_definition() {
            if is_not_private(&f.def.attrs) {
                let name = f.name.as_ref().and_then(|n| n.borrow().as_function_name().and_then(|fname| fname.identifier.clone()));
                top.add(QName::new(&package_definition_name(&f.def.pkgdef), &ident_name(&name)));
            }
        } else if let Some(v) = b.as_variable_definition() {
            if is_not_private(&v.def.attrs) {
                let items: Vec<NodeRef> = v.list.as_ref().and_then(|l| l.borrow().list().map(|l| l.items.iter().flatten().cloned().collect())).unwrap_or_default();
                for binding in items {
                    let bb = binding.borrow();
                    let var = bb.as_variable_binding().and_then(|vb| vb.variable.clone());
                    let id = var.as_ref().and_then(|t| t.borrow().as_typed_identifier().and_then(|t| t.identifier.clone()));
                    top.add(QName::new(&package_definition_name(&v.def.pkgdef), &ident_name(&id)));
                }
            }
        }
    }
}

impl SubCompiler for As3Compiler {
    fn get_name(&self) -> &'static str {
        "as3"
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        mime_type == mime_mappings::AS
    }

    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let existing = source.borrow().get_compilation_unit();
        if let Some(u) = &existing {
            if u.borrow().has_type_info {
                return existing;
            }
        }
        let path = source.borrow().get_name().to_string();
        let cx = new_unit_context(source, symbol_table, Rc::new(RefCell::new(As3CompilerHandler)));
        cx.borrow_mut().config_vars.extend(self.config.defines.iter().cloned());
        let bytes = match source.borrow().to_byte_array() {
            Ok(b) => b,
            Err(e) => {
                toolkit::log_path(Level::Error, &path, &e.to_string(), -1);
                return None;
            }
        };
        let node = Parser::new_stream(&cx, &bytes, &path).parse_program();
        clean_node_factory(&cx);
        if toolkit::error_count() > 0 {
            return None;
        }
        let node = node?;
        evaluate(&node, &cx, &mut ConfigurationEvaluator::new());
        if toolkit::error_count() > 0 {
            return None;
        }
        let unit = match existing {
            Some(u) => u,
            None => Source::new_compilation_unit(source, Some(node.clone())),
        };
        unit.borrow_mut().context.set_asc_context(Some(cx.clone()));

        let mut tree_evaluator = SyntaxTreeEvaluator::new(unit.clone());
        evaluator_adapter::evaluate(&mut tree_evaluator, &cx, &node);
        if toolkit::error_count() > 0 {
            return None;
        }

        let statements: Vec<NodeRef> = {
            let b = node.borrow();
            let p = b.program().unwrap();
            p.statements.as_ref().and_then(|s| s.borrow().as_statement_list().map(|l| l.items.iter().flatten().cloned().collect())).unwrap_or_default()
        };
        let mut definitions = Vec::new();
        let mut in_package = false;
        for n in statements {
            if n.borrow().as_package_definition().is_some() {
                in_package = !in_package;
            } else if n.borrow().is_definition() && in_package {
                definitions.push(n);
            }
        }
        transfer_definitions(&mut unit.borrow_mut().top_level_definitions, &definitions);

        let mut inheritance_evaluator = InheritanceEvaluator::new();
        evaluator_adapter::evaluate(&mut inheritance_evaluator, &cx, &node);
        let inheritance = inheritance_evaluator.get_inheritance();
        let mut u = unit.borrow_mut();
        for name in inheritance.iter() {
            u.inheritance.insert(name.clone());
        }
        drop(u);
        if !self.run_extensions(&unit, symbol_table, |e, u, st| e.parse1(u, st)) {
            return None;
        }
        Some(unit)
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.run_extensions(unit, symbol_table, |e, u, st| e.parse2(u, st));
    }

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        if program_state(&node) != ProgramNode::Inheritance {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        let global = ObjectValue::with_builder(&cx.borrow(), new_global_builder(), None);
        cx.borrow().push_scope(global);
        let name = unit.borrow().get_source().borrow().get_name().to_string();
        let flowem = Rc::new(RefCell::new(FlowGraphEmitter::new(&cx, &name, false)));
        let mut flower = FlowAnalyzer::new(flowem);
        evaluate(&node, &cx, &mut flower);
        cx.borrow().pop_scope();
        unit.borrow_mut().context.flow_analyzer = Some(flower);
        if toolkit::error_count() > 0 {
            return;
        }
        let ns_unresolved: Vec<ReferenceValue> = {
            let mut b = node.borrow_mut();
            let p = b.program_mut().unwrap();
            p.fa_unresolved.clear();
            drain_ordered(&mut p.ns_unresolved)
        };
        {
            let mut u = unit.borrow_mut();
            let u = &mut *u;
            transfer_dependencies(ns_unresolved, &mut u.namespaces, &u.namespace_history);
            u.type_info = node.borrow().program().unwrap().frame.clone();
        }
        self.run_extensions(unit, symbol_table, |e, u, st| e.analyze1(u, st));
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        if program_state(&node) != ProgramNode::Else {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        let mut flower = unit.borrow_mut().context.flow_analyzer.take().expect("NullPointerException");
        unit.borrow_mut().context.processed = Some(HashSet::with_capacity(15));
        inherit_slots(unit, |u| &u.inheritance, symbol_table);
        inherit_slots(unit, |u| &u.namespaces, symbol_table);
        cx.borrow().push_scope(program_frame(&node));
        evaluate(&node, &cx, &mut flower);
        cx.borrow().pop_scope();
        unit.borrow_mut().context.flow_analyzer = Some(flower);
        if toolkit::error_count() > 0 {
            return;
        }
        let (ce, body, ns, rt, pkg, idef) = {
            let mut b = node.borrow_mut();
            let p = b.program_mut().unwrap();
            (
                drain_ordered(&mut p.ce_unresolved),
                drain_ordered(&mut p.body_unresolved),
                drain_ordered(&mut p.ns_unresolved),
                drain_ordered(&mut p.rt_unresolved),
                drain_ordered(&mut p.package_unresolved),
                drain_ordered(&mut p.import_def_unresolved),
            )
        };
        {
            let mut u = unit.borrow_mut();
            let u = &mut *u;
            transfer_dependencies(ce, &mut u.types, &u.type_history);
            transfer_dependencies(body, &mut u.types, &u.type_history);
            transfer_dependencies(ns, &mut u.namespaces, &u.namespace_history);
            transfer_dependencies(rt, &mut u.expressions, &u.expression_history);
            if self.config.strict {
                for r in pkg {
                    u.import_package_statements.insert(r.name());
                }
                for r in idef {
                    let namespaces = r.data().namespaces.to_vec();
                    if let Some(ov) = namespaces.into_iter().find(|ov| ov.is_namespace_value()) {
                        let kind = ov.get_namespace_kind();
                        if kind == NS_PUBLIC || kind == NS_INTERNAL {
                            let q = QName::new(&ov.name(), &r.name());
                            if !u.import_definition_statements.contains(&q) {
                                u.import_definition_statements.insert(q);
                            }
                        }
                    }
                }
            }
        }
        self.run_extensions(unit, symbol_table, |e, u, st| e.analyze2(u, st));
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        let state = program_state(&node);
        if state == ProgramNode::Inheritance || state == ProgramNode::Else {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        inherit_slots(unit, |u| &u.types, symbol_table);
        inherit_slots(unit, |u| &u.namespaces, symbol_table);
        if self.config.strict || self.config.warnings {
            inherit_slots(unit, |u| &u.expressions, symbol_table);
        }
        if self.config.strict {
            let (packages, defs): (Vec<String>, Vec<QName>) = {
                let u = unit.borrow();
                (u.import_package_statements.iter().cloned().collect(), u.import_definition_statements.iter().cloned().collect())
            };
            for p in packages {
                let ns = cx.borrow().get_namespace(&p);
                ns.set_package(true);
            }
            for q in defs {
                cx.borrow().add_valid_import(&q.to_string());
            }
        }
        let internal = unit.borrow().get_source().borrow().is_internal();
        if !internal {
            cx.borrow().push_scope(program_frame(&node));
            let mut printer = MetaDataEvaluator::new_help(self.config.debug, !self.config.optimize);
            evaluate(&node, &cx, &mut printer);
            cx.borrow().pop_scope();
        }
        if toolkit::error_count() > 0 {
            return;
        }
        cx.borrow().push_scope(program_frame(&node));
        let mut analyzer = ConstantEvaluator::new(&cx);
        analyzer.preprocess_definition_type_info_program(&cx, &node);
        cx.borrow().pop_scope();
        unit.borrow_mut().context.constant_evaluator = Some(analyzer);
        self.run_extensions(unit, symbol_table, |e, u, st| e.analyze3(u, st));
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            let classes: Vec<(String, Rc<AbcClass>)> = unit.borrow().class_table.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            for (k, c) in classes {
                symbol_table.register_class(&k, c);
            }
            super::style_evaluator::register_unit_styles(unit, symbol_table);
            evaluate_loader_class_base(unit, symbol_table);
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        let state = program_state(&node);
        if state == ProgramNode::Inheritance || state == ProgramNode::Else {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        cx.borrow().push_scope(program_frame(&node));
        let mut analyzer = unit.borrow_mut().context.constant_evaluator.take().expect("NullPointerException");
        evaluate(&node, &cx, &mut analyzer);
        cx.borrow().pop_scope();
        if toolkit::error_count() > 0 {
            return;
        }
        if self.config.warnings {
            cx.borrow().push_scope(program_frame(&node));
            let name = unit.borrow().get_source().borrow().get_name().to_string();
            let mut lint = LintEvaluator::new(&cx, &name, self.warn_map.clone());
            evaluate(&node, &cx, &mut lint);
            cx.borrow().pop_scope();
            lint.simple_log_warnings(&cx, false);
            lint.clear();
        }
        if toolkit::error_count() > 0 {
            return;
        }
        register_classes(unit, &node, symbol_table);
        super::style_evaluator::register_unit_styles(unit, symbol_table);
        evaluate_loader_class_base(unit, symbol_table);
        self.run_extensions(unit, symbol_table, |e, u, st| e.analyze4(u, st));
    }

    fn generate(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        _symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        let node = unit.borrow().get_syntax_tree().unwrap();
        let source = unit.borrow().get_source();
        let view = source.borrow_mut().emitter_view();
        let emitter = Rc::new(RefCell::new(new_bytecode_emitter_full(
            &cx,
            Some(view),
            self.config.debug || self.config.verbose_stacktraces,
            !self.config.optimize,
            self.config.keep_embed_metadata,
            None,
        )));
        let emitter_ref: EmitterRef = emitter.clone();
        cx.borrow().push_scope(program_frame(&node));
        let mut generator = CodeGenerator::new(emitter_ref);
        evaluate(&node, &cx, &mut generator);
        cx.borrow().pop_scope();
        if toolkit::error_count() > 0 {
            return;
        }
        let mut bytes = Vec::new();
        crate::asc::semantics::emitter::Emitter::emit(&mut *emitter.borrow_mut(), &mut bytes);
        unit.borrow_mut().bytes = bytes;
        if !self.run_extensions(unit, _symbol_table, |e, u, st| e.generate(u, st)) {
            return;
        }
        let (type_info, defs) = {
            let u = unit.borrow();
            (u.type_info.clone(), u.top_level_definitions.clone())
        };
        clean_slots(type_info.as_ref(), Some(&cx), &defs);
        unit.borrow_mut().context.remove_asc_context();
        cx.borrow_mut().handler = None;
    }
}
