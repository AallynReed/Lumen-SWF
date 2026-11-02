//! Port of `flex2.compiler.as3.binding.TypeAnalyzer`: class and interface member/skin-part summaries, built from
//! syntax trees or from the registered class model. The symbol table owns it; calls pass the symbol table it reads.

use std::cell::RefCell;
use std::rc::Rc;

use super::class_info::{ClassInfo, ClassInfoRef};
use super::info::Info;
use super::interface_info::{InterfaceInfo, InterfaceInfoRef};
use crate::asc::flex2::compiler::as3::evaluator_adapter::{self, Adapter};
use crate::asc::flex2::compiler::as3::reflect::as3_class::AbcClass;
use crate::asc::flex2::compiler::as3::reflect::meta_data::MetaData;
use crate::asc::flex2::compiler::as3::reflect::node_magic;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::util::{MultiName, QName};
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::{JHashMap, JHashSet};

const REQUIRED: &str = "required";
const SKINPART: &str = "SkinPart";
const TRUE: &str = "true";

pub type TypeAnalyzerRef = Rc<RefCell<TypeAnalyzer>>;

/// `currentInfo`.
#[derive(Clone)]
enum InfoRef {
    Class(ClassInfoRef),
    Interface(InterfaceInfoRef),
}

impl InfoRef {
    fn with<R>(&self, f: impl FnOnce(&mut Info) -> R) -> R {
        match self {
            InfoRef::Class(c) => f(&mut c.borrow_mut()),
            InfoRef::Interface(i) => f(&mut i.borrow_mut()),
        }
    }
}

#[derive(Default)]
pub struct TypeAnalyzer {
    class_info_map: JHashMap<String, ClassInfoRef>,
    interface_info_map: JHashMap<String, InterfaceInfoRef>,
    current_info: Option<InfoRef>,
    current_package_name: Option<String>,
}

/// The evaluator pass: the analyzer plus the symbol table it consults.
struct Walk<'a> {
    ta: &'a mut TypeAnalyzer,
    st: &'a SymbolTable,
}

fn nn(n: &Option<NodeRef>) -> NodeRef {
    n.clone().expect("NullPointerException")
}

/// `EvaluatorAdapter.evaluate(Context, ClassDefinitionNode)`, whatever the node's own class.
fn walk_class_definition<A: Adapter + ?Sized>(a: &mut A, node: &NodeRef) {
    let (cx, children) = {
        let b = node.borrow();
        let n = b.class_definition().expect("ClassCastException");
        let mut c: Vec<NodeRef> = Vec::new();
        c.extend(n.def.attrs.clone());
        c.extend(n.name.clone());
        c.extend(n.baseclass.clone());
        c.extend(n.interfaces.clone());
        for l in [&n.fexprs, &n.staticfexprs, &n.instanceinits] {
            if let Some(l) = l {
                c.extend(l.borrow().iter().cloned());
            }
        }
        c.extend(n.statements.clone());
        (n.cx.clone().expect("NullPointerException"), c)
    };
    for n in children {
        evaluator_adapter::evaluate(a, &cx, &n);
    }
}

impl TypeAnalyzer {
    pub fn new() -> TypeAnalyzer {
        TypeAnalyzer::default()
    }

    /// `node.evaluate(context, typeAnalyzer)`.
    pub fn evaluate(&mut self, cx: &ContextRef, node: &NodeRef, symbol_table: &SymbolTable) {
        let mut w = Walk { ta: self, st: symbol_table };
        evaluator_adapter::evaluate(&mut w, cx, node);
    }

    /// `evaluate(Context, ClassDefinitionNode)` called directly, as the extensions do.
    pub fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef, symbol_table: &SymbolTable) {
        let mut w = Walk { ta: self, st: symbol_table };
        w.class_definition(cx, node);
    }

    pub fn analyze_interface(&mut self, cx: &ContextRef, multi_name: &MultiName, info: &ClassInfoRef, symbol_table: &SymbolTable) -> Option<InterfaceInfoRef> {
        let mut w = Walk { ta: self, st: symbol_table };
        w.analyze_interface(cx, multi_name, &InfoRef::Class(info.clone()))
    }

    pub fn analyze_class(&mut self, cx: &ContextRef, multi_name: &MultiName, symbol_table: &SymbolTable) -> Option<ClassInfoRef> {
        let mut w = Walk { ta: self, st: symbol_table };
        w.analyze_class(cx, multi_name)
    }

    pub fn get_class_info(&self, class_name: &str) -> Option<ClassInfoRef> {
        self.class_info_map.get(class_name).cloned()
    }

    pub fn get_class_info_iterator(&self) -> Vec<ClassInfoRef> {
        self.class_info_map.values().cloned().collect()
    }

    pub fn remove_class_info(&mut self, class_name: &str) {
        self.class_info_map.remove(class_name);
    }
}

impl Walk<'_> {
    fn analyze_interfaces(&mut self, cx: &ContextRef, multi_names: &[MultiName], info: &InfoRef) {
        for m in multi_names {
            self.analyze_interface(cx, m, info);
        }
    }

    fn analyze_interface(&mut self, cx: &ContextRef, multi_name: &MultiName, info: &InfoRef) -> Option<InterfaceInfoRef> {
        let q_name = self.find_qname(multi_name)?;
        let source = self.st.find_source_by_qname(&q_name);
        let key = q_name.to_string();
        let mut interface_info = self.ta.interface_info_map.get(&key).cloned();
        if interface_info.is_none() {
            let unit = source.expect("NullPointerException").borrow().get_compilation_unit();
            if let Some(unit) = unit {
                let abc_class = unit.borrow().class_table.get(&key).cloned();
                if let Some(abc_class) = abc_class {
                    self.build_interface_info(cx, &q_name, &abc_class);
                } else if let Some(node) = get_node(&unit) {
                    let old = self.ta.current_info.take();
                    evaluator_adapter::evaluate(self, cx, &node);
                    self.ta.current_info = old;
                }
            }
            interface_info = self.ta.interface_info_map.get(&key).cloned();
        }
        if let Some(i) = &interface_info {
            info.with(|inf| inf.add_interface_info(i.clone()));
        }
        interface_info
    }

    fn analyze_base_class(&mut self, cx: &ContextRef, multi_name: &MultiName, class_info: &ClassInfoRef) {
        if let Some(base) = self.analyze_class(cx, multi_name) {
            ClassInfo::set_base_class_info(class_info, base);
        }
    }

    fn analyze_class(&mut self, cx: &ContextRef, multi_name: &MultiName) -> Option<ClassInfoRef> {
        let q_name = self.find_qname(multi_name)?;
        let key = q_name.to_string();
        let mut class_info = self.ta.class_info_map.get(&key).cloned();
        if class_info.is_none() {
            let source = self.st.find_source_by_qname(&q_name).expect("NullPointerException");
            let unit = source.borrow().get_compilation_unit().expect("NullPointerException");
            let abc_class = unit.borrow().class_table.get(&key).cloned();
            if let Some(abc_class) = abc_class {
                self.build_class_info(cx, &q_name, &abc_class);
            } else if let Some(node) = get_node(&unit) {
                let old = self.ta.current_info.take();
                evaluator_adapter::evaluate(self, cx, &node);
                self.ta.current_info = old;
            }
            class_info = self.ta.class_info_map.get(&key).cloned();
        }
        class_info
    }

    fn analyze_base_interface(&mut self, cx: &ContextRef, multi_name: &MultiName, interface_info: &InterfaceInfoRef) {
        let Some(q_name) = self.find_qname(multi_name) else { return };
        let source = self.st.find_source_by_qname(&q_name);
        let key = q_name.to_string();
        let mut base = self.ta.interface_info_map.get(&key).cloned();
        if base.is_none() {
            let unit = source.expect("NullPointerException").borrow().get_compilation_unit();
            let mut abc_class = None;
            if let Some(unit) = &unit {
                abc_class = unit.borrow().class_table.get(&key).cloned();
                if let Some(c) = &abc_class {
                    self.build_interface_info(cx, &q_name, c);
                }
            }
            if abc_class.is_none() {
                if let Some(node) = get_node(&unit.expect("NullPointerException")) {
                    let old = self.ta.current_info.take();
                    evaluator_adapter::evaluate(self, cx, &node);
                    self.ta.current_info = old;
                }
            }
            base = self.ta.interface_info_map.get(&key).cloned();
        }
        if let Some(b) = base {
            interface_info.borrow_mut().set_base_interface_info(b);
        }
    }

    fn build_class_info(&mut self, cx: &ContextRef, q_name: &QName, abc_class: &AbcClass) {
        let class_info = Rc::new(RefCell::new(ClassInfo::new(abc_class.get_name())));
        if let Some(p) = &self.ta.current_package_name {
            self.ta.current_info.as_ref().expect("NullPointerException").with(|i| i.add_import(p));
        }
        self.ta.class_info_map.insert(q_name.to_string(), class_info.clone());
        if let Some(super_type_name) = abc_class.get_super_type_name() {
            class_info.borrow_mut().set_base_class_name(super_type_name);
            let m = class_info.borrow_mut().get_base_class_multi_name();
            self.analyze_base_class(cx, &m, &class_info);
        }
        if let Some(interface_names) = abc_class.get_interface_names() {
            for n in &interface_names {
                class_info.borrow_mut().add_interface_name(n.as_deref().expect("NullPointerException"));
            }
            let m = class_info.borrow_mut().get_interface_multi_names();
            self.analyze_interfaces(cx, &m, &InfoRef::Class(class_info.clone()));
        }
        process_members(abc_class, &class_info);
    }

    fn build_interface_info(&mut self, cx: &ContextRef, q_name: &QName, abc_class: &AbcClass) {
        let interface_info = Rc::new(RefCell::new(InterfaceInfo::new(abc_class.get_name())));
        self.ta.interface_info_map.insert(q_name.to_string(), interface_info.clone());
        if let Some(super_type_name) = abc_class.get_super_type_name() {
            interface_info.borrow_mut().set_base_interface_name(super_type_name);
            let m = interface_info.borrow_mut().get_base_interface_multi_name();
            self.analyze_base_interface(cx, &m, &interface_info);
        }
        if let Some(interface_names) = abc_class.get_interface_names() {
            for n in &interface_names {
                interface_info.borrow_mut().add_interface_name(n.as_deref().expect("NullPointerException"));
            }
            let m = interface_info.borrow_mut().get_interface_multi_names();
            self.analyze_interfaces(cx, &m, &InfoRef::Interface(interface_info.clone()));
        }
        for m in abc_class.get_method_iterator() {
            interface_info.borrow_mut().add_function(m.get_qname());
        }
    }

    fn package_name_of_frame(node: &NodeRef) -> Option<String> {
        let cframe = node.borrow().class_definition()?.cframe.clone()?;
        let name = cframe.name_opt()?;
        name.ns().map(|ns| ns.name())
    }

    fn binary_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        if let Some(p) = Self::package_name_of_frame(node) {
            self.ta.current_package_name = Some(p);
        }
        self.evaluate_interface(cx, node);
        self.ta.current_package_name = None;
    }

    fn binary_class_def(&mut self, cx: &ContextRef, node: &NodeRef) {
        if let Some(p) = Self::package_name_of_frame(node) {
            self.ta.current_package_name = Some(p);
        }
        self.class_definition(cx, node);
        self.ta.current_package_name = None;
    }

    fn base_name_of(node: &NodeRef) -> Option<Option<String>> {
        let baseclass = node.borrow().class_definition().unwrap().baseclass.clone()?;
        let b = baseclass.borrow();
        if let Some(m) = b.as_member_expression() {
            Some(m.selector.as_ref().map(|sel| {
                let s = sel.borrow();
                let id = s.selector().expect("ClassCastException").get_identifier().expect("NullPointerException");
                let name = to_string(&id);
                name
            }))
        } else if let Some(ls) = b.as_literal_string() {
            Some(Some(ls.value.clone()))
        } else {
            debug_assert!(false);
            Some(None)
        }
    }

    fn class_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        let class_name = node_magic::get_class_name(node);
        if self.ta.class_info_map.get(&class_name).is_some() {
            return;
        }
        let old = self.ta.current_info.take();
        let class_info = Rc::new(RefCell::new(ClassInfo::new(&class_name)));
        let info = InfoRef::Class(class_info.clone());
        self.ta.current_info = Some(info.clone());
        self.ta.class_info_map.insert(class_name, class_info.clone());
        if let Some(p) = self.ta.current_package_name.clone() {
            info.with(|i| i.add_import(&p));
        }
        let (pkgdef, statements, is_binary) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (c.def.pkgdef.clone(), c.statements.clone(), b.as_binary_class_def().is_some() || b.as_binary_interface_definition().is_some())
        };
        if let Some(pkgdef) = pkgdef {
            let stmts = nn(&pkgdef.borrow().as_package_definition().expect("ClassCastException").statements);
            process_imports(&stmts, &info);
        }
        if let Some(stmts) = statements {
            process_imports(&stmts, &info);
        }
        if let Some(Some(base_class_name)) = Self::base_name_of(node) {
            class_info.borrow_mut().set_base_class_name(&base_class_name);
            let m = class_info.borrow_mut().get_base_class_multi_name();
            self.analyze_base_class(cx, &m, &class_info);
        }
        self.process_interfaces(cx, node);
        if is_binary {
            process_members(&AbcClass::from_class_definition(node), &class_info);
        }
        walk_class_definition(self, node);
        self.ta.current_info = old;
    }

    fn evaluate_interface(&mut self, cx: &ContextRef, node: &NodeRef) {
        let interface_name = node_magic::get_class_name(node);
        if self.ta.interface_info_map.get(&interface_name).is_some() {
            return;
        }
        let old = self.ta.current_info.take();
        let interface_info = Rc::new(RefCell::new(InterfaceInfo::new(&interface_name)));
        let info = InfoRef::Interface(interface_info.clone());
        self.ta.current_info = Some(info.clone());
        self.ta.interface_info_map.insert(interface_name, interface_info.clone());
        let (pkgdef, statements) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            (c.def.pkgdef.clone(), c.statements.clone())
        };
        if let Some(pkgdef) = pkgdef {
            let stmts = nn(&pkgdef.borrow().as_package_definition().expect("ClassCastException").statements);
            process_imports(&stmts, &info);
        }
        if let Some(stmts) = statements {
            process_imports(&stmts, &info);
        }
        let base = node.borrow().class_definition().unwrap().baseclass.clone();
        if let Some(base) = base {
            let sel = base.borrow().as_member_expression().map(|m| m.selector.clone());
            match sel {
                Some(Some(sel)) => {
                    let id = sel.borrow().selector().expect("ClassCastException").get_identifier().expect("NullPointerException");
                    let base_interface_name = to_string(&id);
                    interface_info.borrow_mut().set_base_interface_name(&base_interface_name);
                    let m = interface_info.borrow_mut().get_base_interface_multi_name();
                    self.analyze_base_interface(cx, &m, &interface_info);
                }
                Some(None) => {}
                None => debug_assert!(false),
            }
        }
        self.process_interfaces(cx, node);
        walk_class_definition(self, node);
        self.ta.current_info = old;
    }

    fn function_definition(&mut self, node: &NodeRef) {
        let has_name = {
            let b = node.borrow();
            let f = b.function_definition().unwrap();
            f.name.as_ref().is_some_and(|n| n.borrow().as_function_name().is_some_and(|fname| fname.identifier.is_some()))
        };
        if !has_name {
            return;
        }
        let function_name = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_function_name(node).unwrap_or_default());
        if let Some(info) = &self.ta.current_info {
            if node_magic::function_is_getter(node) {
                info.with(|i| i.add_getter(function_name));
            } else if node_magic::function_is_setter(node) {
                info.with(|i| i.add_setter(function_name));
            } else {
                info.with(|i| i.add_function(function_name));
            }
        }
    }

    fn package_definition(&mut self, node: &NodeRef) {
        let pkg_part = {
            let b = node.borrow();
            let p = b.as_package_definition().unwrap();
            p.name.as_ref().and_then(|n| n.borrow().as_package_name().and_then(|pn| pn.id.clone())).and_then(|id| id.borrow().as_package_identifiers().and_then(|i| i.pkg_part.clone()))
        };
        if let Some(p) = pkg_part {
            self.ta.current_package_name = Some(p);
        }
        self.ta.current_package_name = None;
    }

    fn variable_definition(&mut self, node: &NodeRef) {
        let ok = {
            let b = node.borrow();
            let v = b.as_variable_definition().unwrap();
            v.list.as_ref().is_some_and(|l| l.borrow().list().is_some_and(|l| l.items.first().cloned().flatten().is_some_and(|i| i.borrow().as_variable_binding().is_some())))
        };
        if !ok {
            return;
        }
        let variable_name = QName::new(&node_magic::get_user_namespace(node), &node_magic::get_variable_name(node));
        if let Some(InfoRef::Class(c)) = &self.ta.current_info {
            c.borrow_mut().add_variable(variable_name);
        }
    }

    fn meta_data(&mut self, node: &NodeRef) {
        let (id, def) = {
            let b = node.borrow();
            let m = b.meta_data().unwrap();
            (m.get_id(), m.def.clone())
        };
        if id.as_deref() != Some(SKINPART) {
            return;
        }
        let Some(def) = def else { return };
        let (is_var, is_fun) = {
            let d = def.borrow();
            (d.as_variable_definition().is_some(), d.function_definition().is_some())
        };
        if is_var || is_fun {
            let field_name = if is_var { node_magic::get_variable_name(&def) } else { node_magic::get_function_name(&def).unwrap_or_default() };
            let value = node.borrow().meta_data().unwrap().get_value_key(REQUIRED);
            match &self.ta.current_info {
                Some(InfoRef::Class(c)) => c.borrow_mut().add_skin_part(&field_name, value.is_some_and(|v| v.eq_ignore_ascii_case(TRUE))),
                _ => panic!("ClassCastException"),
            }
        }
    }

    fn find_qname(&self, multi_name: &MultiName) -> Option<QName> {
        let local_part = multi_name.get_local_part();
        for ns in multi_name.get_namespace() {
            if self.st.find_source_by_qname_parts(ns, local_part).is_some() {
                return Some(QName::new(ns, local_part));
            }
        }
        None
    }

    fn process_interfaces(&mut self, cx: &ContextRef, node: &NodeRef) {
        let interfaces = node.borrow().class_definition().unwrap().interfaces.clone();
        let Some(interfaces) = interfaces else { return };
        let info = self.ta.current_info.clone().expect("NullPointerException");
        let items: Vec<NodeRef> = interfaces.borrow().list().expect("ClassCastException").items.iter().flatten().cloned().collect();
        for member_expression in items {
            let sel = member_expression.borrow().as_member_expression().expect("ClassCastException").selector.clone();
            let Some(sel) = sel else { continue };
            let identifier = sel.borrow().selector().expect("ClassCastException").get_identifier().expect("NullPointerException");
            let interface_name = to_string(&identifier);
            let namespaces = identifier.borrow().identifier().and_then(|i| i.ref_.clone()).map(|r| r.data().namespaces.to_vec());
            match namespaces {
                Some(nss) => {
                    if nss.is_empty() {
                        let ns_name = nss.first().expect("IndexOutOfBoundsException").name();
                        if !ns_name.is_empty() {
                            info.with(|i| i.add_interface_multi_name(&ns_name, &interface_name));
                        } else {
                            info.with(|i| i.add_interface_name(&interface_name));
                        }
                    } else {
                        let mut set: JHashSet<String> = JHashSet::new();
                        for ns in &nss {
                            let n = ns.name();
                            if !n.is_empty() {
                                set.insert(n);
                            }
                        }
                        let namespaces: Vec<String> = set.iter().cloned().collect();
                        info.with(|i| i.add_interface_multi_name_namespaces(namespaces, &interface_name));
                    }
                }
                None => info.with(|i| i.add_interface_name(&interface_name)),
            }
        }
        let m = info.with(|i| i.get_interface_multi_names());
        self.analyze_interfaces(cx, &m, &info);
    }
}

impl Adapter for Walk<'_> {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let kind = match &node.borrow().kind {
            Kind::BinaryInterfaceDefinition(_) => 1,
            Kind::BinaryClassDef(_) => 2,
            Kind::ClassDefinition(_) => 3,
            Kind::InterfaceDefinition(_) => 4,
            Kind::FunctionCommon(_) => 5,
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => 6,
            Kind::PackageDefinition(_) => 7,
            Kind::VariableDefinition(_) => 8,
            Kind::MetaData(_) => 9,
            _ => 0,
        };
        match kind {
            1 => self.binary_interface_definition(cx, node),
            2 => self.binary_class_def(cx, node),
            3 => self.class_definition(cx, node),
            4 => self.evaluate_interface(cx, node),
            5 => {}
            6 => self.function_definition(node),
            7 => self.package_definition(node),
            8 => self.variable_definition(node),
            9 => self.meta_data(node),
            _ => return false,
        }
        true
    }
}

fn process_members(abc_class: &AbcClass, class_info: &ClassInfoRef) {
    let mut c = class_info.borrow_mut();
    for m in abc_class.get_method_iterator() {
        c.add_function(m.get_qname());
    }
    for getter in abc_class.get_getter_iterator() {
        let getter_name = getter.get_qname();
        c.add_getter(getter_name.clone());
        process_skin_part_meta_data(getter.get_meta_data(SKINPART), &mut c, &getter_name);
    }
    for s in abc_class.get_setter_iterator() {
        c.add_setter(s.get_qname());
    }
    for variable in abc_class.get_var_iterator() {
        let var_name = variable.get_qname();
        c.add_variable(var_name.clone());
        process_skin_part_meta_data(variable.get_meta_data(SKINPART), &mut c, &var_name);
    }
}

fn process_skin_part_meta_data(meta_data: Option<Vec<MetaData>>, class_info: &mut ClassInfo, qname: &QName) {
    for skin_part in meta_data.iter().flatten() {
        let s_required = skin_part.get_value_key(REQUIRED);
        let required = s_required.is_some_and(|r| r.eq_ignore_ascii_case(TRUE));
        class_info.add_skin_part(&qname.local_part, required);
    }
}

/// `getNode(compilationUnit)`.
fn get_node(unit: &UnitRef) -> Option<NodeRef> {
    let u = unit.borrow();
    if let Some(t) = u.get_syntax_tree() {
        return Some(t);
    }
    // An MXML unit's syntax tree is its DocumentNode (kept as an attribute until the second pass); use its delegate's.
    u.context.get_attribute("DocumentNode")?;
    let delegate = u.context.get_attribute_as::<UnitRef>("DelegateUnit")?;
    let tree = delegate.borrow().get_syntax_tree();
    tree
}

fn process_imports(statements: &NodeRef, info: &InfoRef) {
    let items: Vec<NodeRef> = statements.borrow().as_statement_list().expect("ClassCastException").items.iter().flatten().cloned().collect();
    for node in items {
        let parts = {
            let b = node.borrow();
            b.as_import_directive().map(|imp| {
                let name = nn(&imp.name);
                let nb = name.borrow();
                let id = nn(&nb.as_package_name().expect("ClassCastException").id);
                let ib = id.borrow();
                let ids = ib.as_package_identifiers().expect("ClassCastException");
                (ids.def_part.clone().unwrap_or_default(), ids.pkg_part.clone().unwrap_or_default())
            })
        };
        if let Some((def_part, pkg_part)) = parts {
            if def_part.is_empty() {
                info.with(|i| i.add_import(&pkg_part));
            } else {
                info.with(|i| i.add_qualified_import(&def_part, &pkg_part));
            }
        }
    }
}

fn to_string(identifier: &NodeRef) -> String {
    let b = identifier.borrow();
    if let Some(q) = b.as_qualified_identifier() {
        match q.qualifier.as_ref().and_then(|ql| ql.borrow().as_literal_string().map(|l| l.value.clone())) {
            Some(v) => format!("{v}:{}", q.id.name),
            None => {
                debug_assert!(false, "Unhandled QualifiedIdentifierNode qualifier type");
                String::new()
            }
        }
    } else {
        b.identifier().expect("ClassCastException").name.clone()
    }
}
