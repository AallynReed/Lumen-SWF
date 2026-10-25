//! Port of `macromedia/asc/semantics/FlowAnalyzer.java`: binds the program's definitions into the object model (names,
//! slots, method ids, class frames, imports) and drives `FlowGraphEmitter` for each method's reaching definitions.

#![allow(non_upper_case_globals)]
#![allow(clippy::too_many_arguments)]

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;
use std::rc::Rc;

use crate::asc::emit::activation_builder::new_activation_builder;
use crate::asc::emit::catch_builder::new_catch_builder;
use crate::asc::emit::class_builder::new_class_builder;
use crate::asc::emit::constants::METHOD_Arguments;
use crate::asc::emit::function_builder::new_function_builder;
use crate::asc::emit::global_builder::new_global_builder;
use crate::asc::emit::import_builder::new_import_builder;
use crate::asc::emit::instance_builder::new_instance_builder;
use crate::asc::emit::runtime_constants::{IF_false, IF_true, SLOT_Global__cv, TYPE_none, TYPE_object, TYPE_void};
use crate::asc::emit::with_builder::new_with_builder;
use crate::asc::parser::evaluator::*;
use crate::asc::parser::node::{initializer_statement, new_node_list, Kind, NodeList, NodeRef, ProgramNode};
use crate::asc::parser::tokens::*;
use crate::asc::semantics::builder::Builder;
use crate::asc::semantics::emitter::{Emitter, EmitterBase, EmitterRef};
use crate::asc::semantics::flow_graph_emitter::FlowGraphEmitter;
use crate::asc::semantics::interface_walker::InterfaceWalker;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::object_value_walker::ObjectValueWalker;
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::slot::CALL_Method;
use crate::asc::semantics::type_value::{inherit_class_slots_static, TypeValue};
use crate::asc::semantics::value::Value;
use crate::asc::util::context::{Context, ContextRef, NS_EXPLICIT, NS_INTERNAL, NS_PRIVATE, NS_PROTECTED, NS_STATIC_PROTECTED};
use crate::asc::util::errors::*;
use crate::asc::util::graph::algorithms::topological_sort;
use crate::asc::util::graph::dependency_graph::DependencyGraph;
use crate::asc::util::graph::vertex::Vertex;
use crate::asc::util::multinames::Multinames;
use crate::asc::util::names::Names;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::number_usage::NumberUsage;

const INTRINSIC: &str = "intrinsic";
const INTERNAL: &str = "internal";
const PUBLIC: &str = "public";
const PRIVATE: &str = "private";
const PROTECTED: &str = "protected";
const PROTOTYPE: &str = "prototype";
const STATIC: &str = "static";

const NPE: &str = "NullPointerException";
const CCE: &str = "ClassCastException";

/// `FlowAnalyzer.CaseList`.
#[derive(Default)]
struct CaseList {
    items: Vec<Option<NodeRef>>,
    has_default: bool,
}

/// An entry of `importednames_sets`: Java aliases the `Multinames` a node owns, so this names the owner.
#[derive(Clone)]
enum ImportedNames {
    Own(Rc<RefCell<Multinames>>),
    Node(NodeRef),
}

impl ImportedNames {
    fn with<R>(&self, f: impl FnOnce(&mut Multinames) -> R) -> R {
        match self {
            ImportedNames::Own(m) => f(&mut m.borrow_mut()),
            ImportedNames::Node(n) => {
                let mut b = n.borrow_mut();
                if let Kind::PackageDefinition(p) = &mut b.kind {
                    return f(&mut p.imported_names);
                }
                if let Kind::FunctionCommon(fc) = &mut b.kind {
                    return f(fc.imported_names.as_mut().expect(NPE));
                }
                f(&mut b.class_definition_mut().expect(CCE).imported_names)
            }
        }
    }

    fn get(&self, name: &str) -> Option<Namespaces> {
        self.with(|m| m.get(name))
    }

    fn snapshot(&self) -> Multinames {
        self.with(|m| m.clone())
    }
}

/// The seven `*_unresolved_sets` stacks, which are always pushed and popped together with one `ProgramNode`.
#[derive(Clone, Copy)]
enum Unresolved {
    ImportDef,
    Package,
    Ns,
    Fa,
    Ce,
    Body,
    Rt,
}

fn unresolved_set(p: &mut ProgramNode, which: Unresolved) -> &mut HashSet<ReferenceValue> {
    match which {
        Unresolved::ImportDef => &mut p.import_def_unresolved,
        Unresolved::Package => &mut p.package_unresolved,
        Unresolved::Ns => &mut p.ns_unresolved,
        Unresolved::Fa => &mut p.fa_unresolved,
        Unresolved::Ce => &mut p.ce_unresolved,
        Unresolved::Body => &mut p.body_unresolved,
        Unresolved::Rt => &mut p.rt_unresolved,
    }
}

pub struct FlowAnalyzer {
    base: EmitterBase,
    flow: Rc<RefCell<FlowGraphEmitter>>,
    interface_methods: Option<Names>,
    define_cv: bool,
    error_node_seen: bool,
    endpoint_dominator_is_set: bool,
    found_circular_or_duplicate_class_definition: bool,
    endpoint_dominator: Option<NodeRef>,
    case_exprs: Vec<CaseList>,
    fexprs_sets: Vec<NodeList>,
    clsdefs_sets: Vec<NodeList>,
    instanceinits_sets: Vec<NodeList>,
    staticfexprs_sets: Vec<NodeList>,
    usednamespaces_sets: Vec<Namespaces>,
    used_def_namespaces_sets: Vec<Namespaces>,
    importednames_sets: Vec<ImportedNames>,
    private_namespaces: Vec<ObjectValue>,
    default_namespaces: Vec<ObjectValue>,
    public_namespaces: Vec<ObjectValue>,
    protected_namespaces: Vec<ObjectValue>,
    static_protected_namespaces: Vec<ObjectValue>,
    max_params_stack: Vec<i32>,
    max_locals_stack: Vec<i32>,
    max_temps_stack: Vec<i32>,
    fun_name_stack: Vec<String>,
    with_used_stack: Vec<i32>,
    exceptions_used_stack: Vec<i32>,
    number_usage_stack: Vec<NumberUsage>,
    region_name_stack: Vec<String>,
    package_name: String,
    import_context: Vec<bool>,
    strict_context: Vec<bool>,
    this_contexts: Vec<i32>,
    super_context: Vec<i32>,
    resolve_inheritance: bool,
    /// The program nodes whose unresolved sets are the tops of the `*_unresolved_sets` stacks.
    unresolved_sets: Vec<NodeRef>,
    unresolved: HashSet<ReferenceValue>,
    program_node_temp: i32,
    pub local_file_namespace: Option<ObjectValue>,
    loop_index: i32,
    inside_switch: i32,
    inside_loop: i32,
}

fn c(cx: &ContextRef) -> Ref<'_, Context> {
    cx.borrow()
}

fn nn<T>(o: Option<T>) -> T {
    o.expect(NPE)
}

fn pos(n: &NodeRef) -> i32 {
    n.borrow().pos()
}

fn pos_of(n: &Option<NodeRef>) -> i32 {
    nn(n.as_ref()).borrow().pos()
}

fn same_node(a: &Option<NodeRef>, b: &Option<NodeRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn same_obj(a: &Option<ObjectValue>, b: &Option<ObjectValue>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn rv(v: Option<Value>) -> Option<ReferenceValue> {
    v.and_then(|v| v.as_reference_value())
}

/// `IntList.last()`.
fn ilast(v: &[i32]) -> i32 {
    v.last().copied().unwrap_or(0)
}

/// `IntList.set(size() - 1, x)`.
fn iset_last(v: &mut [i32], x: i32) {
    let n = v.len();
    if n == 0 {
        panic!("ArrayIndexOutOfBoundsException");
    }
    v[n - 1] = x;
}

/// `IntList.pop_back()`.
fn ipop_back(v: &mut Vec<i32>) -> i32 {
    v.pop().expect("ArrayIndexOutOfBoundsException")
}

fn list_contains(l: &NodeList, n: &NodeRef) -> bool {
    l.borrow().iter().any(|x| Rc::ptr_eq(x, n))
}

/// `Namespaces.addAll`, safe when both are the same list.
fn ns_add_all(dst: &Namespaces, src: &Namespaces) {
    for n in src.to_vec() {
        dst.push_back(n);
    }
}

fn builder_of(o: &ObjectValue) -> Builder {
    o.builder().expect(NPE)
}

fn is_instance_builder(o: &ObjectValue) -> bool {
    o.builder().map_or(false, |b| b.is_instance_builder())
}

fn is_class_builder(o: &ObjectValue) -> bool {
    o.builder().map_or(false, |b| b.is_class_builder())
}

impl Emitter for FlowAnalyzer {
    fn emitter_base(&self) -> &EmitterBase {
        &self.base
    }

    fn emitter_base_mut(&mut self) -> &mut EmitterBase {
        &mut self.base
    }
}

impl FlowAnalyzer {
    pub fn new(emitter: Rc<RefCell<FlowGraphEmitter>>) -> FlowAnalyzer {
        let impl_: EmitterRef = emitter.clone();
        FlowAnalyzer {
            base: EmitterBase::new(Some(impl_)),
            flow: emitter,
            interface_methods: None,
            define_cv: false,
            error_node_seen: false,
            endpoint_dominator_is_set: false,
            found_circular_or_duplicate_class_definition: false,
            endpoint_dominator: None,
            case_exprs: Vec::new(),
            fexprs_sets: Vec::new(),
            clsdefs_sets: Vec::new(),
            instanceinits_sets: Vec::new(),
            staticfexprs_sets: Vec::new(),
            usednamespaces_sets: Vec::new(),
            used_def_namespaces_sets: Vec::new(),
            importednames_sets: Vec::new(),
            private_namespaces: Vec::new(),
            default_namespaces: Vec::new(),
            public_namespaces: Vec::new(),
            protected_namespaces: Vec::new(),
            static_protected_namespaces: Vec::new(),
            max_params_stack: Vec::new(),
            max_locals_stack: Vec::new(),
            max_temps_stack: Vec::new(),
            fun_name_stack: Vec::new(),
            with_used_stack: Vec::new(),
            exceptions_used_stack: Vec::new(),
            number_usage_stack: Vec::new(),
            region_name_stack: Vec::new(),
            package_name: String::new(),
            import_context: vec![false],
            strict_context: Vec::new(),
            this_contexts: Vec::new(),
            super_context: Vec::new(),
            resolve_inheritance: true,
            unresolved_sets: Vec::new(),
            unresolved: HashSet::new(),
            program_node_temp: 0,
            local_file_namespace: None,
            loop_index: 0,
            inside_switch: 0,
            inside_loop: 0,
        }
    }

    /// `getEmitter()`.
    fn get_emitter(&self) -> RefMut<'_, FlowGraphEmitter> {
        self.flow.borrow_mut()
    }

    fn eval(&mut self, cx: &ContextRef, n: &NodeRef) -> Option<Value> {
        evaluate(n, cx, self)
    }

    fn eval_opt(&mut self, cx: &ContextRef, n: &Option<NodeRef>) -> Option<Value> {
        evaluate(nn(n.as_ref()), cx, self)
    }

    fn add_stmt(&self, node: &NodeRef) {
        let s = node.borrow().to_string();
        self.get_emitter().add_stmt_to_block(&s);
    }

    fn with_depth(cx: &ContextRef) -> i32 {
        c(cx).statics.borrow().with_depth
    }

    fn set_with_depth(cx: &ContextRef, d: i32) {
        c(cx).statics.borrow_mut().with_depth = d;
    }

    fn scope_count(cx: &ContextRef) -> usize {
        c(cx).get_scope_depth()
    }

    fn scope(cx: &ContextRef) -> ObjectValue {
        nn(c(cx).scope())
    }

    fn back_default(&self) -> ObjectValue {
        nn(self.default_namespaces.last().cloned())
    }

    fn back_used(&self) -> Namespaces {
        nn(self.usednamespaces_sets.last().cloned())
    }

    fn back_used_def(&self) -> Namespaces {
        nn(self.used_def_namespaces_sets.last().cloned())
    }

    fn back_imported(&self) -> ImportedNames {
        nn(self.importednames_sets.last().cloned())
    }

    fn back_fun_name(&self) -> String {
        self.fun_name_stack.last().cloned().unwrap_or_default()
    }

    fn back_region(&self) -> String {
        nn(self.region_name_stack.last().cloned())
    }

    /// `x_unresolved_sets.last().addAll(unresolved); unresolved.clear();`
    fn flush_unresolved(&mut self, which: Unresolved) {
        let items: Vec<ReferenceValue> = self.unresolved.drain().collect();
        let prog = nn(self.unresolved_sets.last().cloned());
        let mut p = prog.borrow_mut();
        unresolved_set(p.program_mut().expect(CCE), which).extend(items);
    }

    fn add_unresolved(&mut self, which: Unresolved, r: ReferenceValue) {
        let prog = nn(self.unresolved_sets.last().cloned());
        let mut p = prog.borrow_mut();
        unresolved_set(p.program_mut().expect(CCE), which).insert(r);
    }

    /// Pushes a package definition's access namespaces, as the toplevel definitions do.
    fn push_package_namespaces(&mut self, pkgdef: &NodeRef, with_used_def: bool) {
        let (public, internal, used, used_def) = {
            let p = pkgdef.borrow();
            let p = p.as_package_definition().expect(CCE);
            (p.public_namespace.clone(), p.internal_namespace.clone(), p.used_namespaces.clone(), p.used_def_namespaces.clone())
        };
        self.public_namespaces.push(nn(public));
        self.default_namespaces.push(nn(internal));
        self.usednamespaces_sets.push(used);
        if with_used_def {
            self.used_def_namespaces_sets.push(used_def);
        }
        self.importednames_sets.push(ImportedNames::Node(pkgdef.clone()));
    }

    fn pop_package_namespaces(&mut self, with_used_def: bool) {
        self.public_namespaces.pop();
        self.default_namespaces.pop();
        self.usednamespaces_sets.pop();
        if with_used_def {
            self.used_def_namespaces_sets.pop();
        }
        self.importednames_sets.pop();
    }

    fn set_global_in_package(cx: &ContextRef, v: bool) {
        let b = Self::scope(cx).builder();
        if let Some(b) = b {
            if b.is_global_builder() {
                b.set_is_in_package(v);
            }
        }
    }

    fn evaluate_identifier_impl(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (has_ref, name) = {
            let n = node.borrow();
            let id = n.identifier().expect(CCE);
            (id.ref_.is_some(), id.name.clone())
        };
        if !has_ref {
            let mut namespaces = self.back_used();
            let importednames = self.back_imported();
            if let Some(nss) = importednames.get(&name) {
                let copy = Namespaces::copy_of(&namespaces);
                ns_add_all(&copy, &nss);
                namespaces = copy;
            }
            let r = ReferenceValue::with_namespaces(&c(cx), None, &name, &namespaces);
            let (is_attr, p) = {
                let n = node.borrow();
                (n.is_attr(), n.pos())
            };
            r.set_is_attribute_identifier(is_attr);
            r.set_position(p);
            node.borrow_mut().identifier_mut().unwrap().ref_ = Some(r);
        }
        node.borrow().identifier().unwrap().ref_.clone().map(Value::Reference)
    }

    fn evaluate_qualified_identifier_impl(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if node.borrow().identifier().unwrap().ref_.is_none() {
            let mut ns: Option<ObjectValue> = None;
            let mut nss: Option<Namespaces> = None;
            let qualifier = node.borrow().qualified_identifier().unwrap().qualifier.clone();

            if let Some(q) = &qualifier {
                let attrs_namespaces = q.borrow().as_attribute_list().map(|a| a.namespaces.clone());
                if let Some(ans) = attrs_namespaces {
                    if ans.size() > 0 {
                        ns = Some(ans.get(0));
                    } else {
                        c(cx).internal_error_at(pos(node), "internal error: invalid qualifier in QualifiedIdentifierNode");
                        ns = Some(c(cx).public_namespace());
                    }
                } else {
                    let mut ref_: Option<Value> = None;
                    let is_memb = q.borrow().as_member_expression().is_some();
                    if is_memb {
                        if q.borrow().is_any() {
                            ref_ = Some(Value::Object(c(cx).any_namespace()));
                        } else {
                            let id_name = {
                                let qb = q.borrow();
                                let sel = nn(qb.as_member_expression().unwrap().selector.clone());
                                let id = sel.borrow().selector().expect(CCE).get_identifier();
                                id.map(|i| i.borrow().identifier().unwrap().name.clone())
                            };
                            if id_name.as_deref() == Some(INTERNAL) {
                                ns = self.default_namespaces.last().cloned();
                            } else if id_name.as_deref() == Some(PRIVATE) {
                                if self.private_namespaces.is_empty() {
                                    c(cx).error(pos(q), kError_InvalidPrivate);
                                } else {
                                    ns = self.private_namespaces.last().cloned();
                                }
                            } else if id_name.as_deref() == Some(PROTECTED) {
                                if self.static_protected_namespaces.is_empty() {
                                    c(cx).error(pos(q), kError_InvalidProtected);
                                } else {
                                    let n = Namespaces::new();
                                    n.push_back(self.static_protected_namespaces.last().cloned().unwrap());
                                    if !self.protected_namespaces.is_empty() {
                                        for u in self.back_used().to_vec() {
                                            if u.get_namespace_kind() == NS_PROTECTED {
                                                n.push_back(u);
                                            }
                                        }
                                    }
                                    nss = Some(n);
                                }
                            } else if id_name.as_deref() == Some(PUBLIC) {
                                ns = self.public_namespaces.last().cloned();
                            } else {
                                self.flush_unresolved(Unresolved::Rt);
                                self.eval(cx, q);
                                ref_ = q.borrow().as_member_expression().unwrap().ref_.clone().map(Value::Reference);
                                self.flush_unresolved(Unresolved::Ns);
                            }
                        }
                    }
                    let lsn = q.borrow().as_literal_string().map(|l| l.value.clone());
                    if let Some(value) = lsn {
                        ns = Some(c(cx).get_namespace(&value));
                    } else if let Some(r) = ref_ {
                        let value = r.get_value(&c(cx));
                        ns = value.and_then(|v| v.as_object_value());
                        if ns.is_none() {
                            return None;
                        }
                    } else if ns.is_none() && nss.is_none() {
                        c(cx).error(pos(q), kError_UndefinedNamespace);
                    }
                }
            } else {
                ns = self.default_namespaces.last().cloned();
            }

            let name = node.borrow().identifier().unwrap().name.clone();
            if !name.is_empty() {
                let r = match &nss {
                    Some(nss) => ReferenceValue::with_namespaces(&c(cx), None, &name, nss),
                    None => ReferenceValue::new(&c(cx), None, &name, ns.as_ref()),
                };
                let (is_attr, p) = {
                    let n = node.borrow();
                    (n.is_attr(), n.pos())
                };
                r.set_is_attribute_identifier(is_attr);
                r.set_position(p);
                node.borrow_mut().identifier_mut().unwrap().ref_ = Some(r);
            } else if node.borrow().as_qualified_expression().is_some() {
                let v = match ns {
                    Some(ns) => {
                        let n = Namespaces::new();
                        n.push_back(ns);
                        Some(n)
                    }
                    None => nss,
                };
                node.borrow_mut().as_qualified_expression_mut().unwrap().nss = v;
            }
        }
        node.borrow().identifier().unwrap().ref_.clone().map(Value::Reference)
    }

    /// `computeNamespaces`, after the attribute list has been evaluated.
    pub fn compute_namespaces(&mut self, cx: &ContextRef, attrs: &Option<NodeRef>, namespaces: &Namespaces, namespace_ids: &mut Vec<String>) {
        match attrs {
            Some(attrs) => {
                let (has_private, has_protected, has_public, has_internal, ans) = {
                    let a = attrs.borrow();
                    let a = a.as_attribute_list().expect(CCE);
                    (a.has_private, a.has_protected, a.has_public, a.has_internal, a.namespaces.clone())
                };
                let push = |ns: ObjectValue, id: String| {
                    ans.push_back(ns);
                    attrs.borrow_mut().as_attribute_list_mut().unwrap().namespace_ids.push(id);
                };
                let public = c(cx).public_namespace();
                if has_private {
                    if self.private_namespaces.is_empty() {
                        c(cx).error(pos(attrs), kError_InvalidPrivate);
                        push(public, PRIVATE.to_string());
                    } else {
                        push(self.private_namespaces.last().cloned().unwrap(), PRIVATE.to_string());
                    }
                } else if has_protected {
                    if is_instance_builder(&Self::scope(cx)) {
                        if self.protected_namespaces.is_empty() {
                            c(cx).error(pos(attrs), kError_InvalidProtected);
                            push(public, PROTECTED.to_string());
                        } else {
                            push(self.protected_namespaces.last().cloned().unwrap(), PROTECTED.to_string());
                        }
                    } else if self.static_protected_namespaces.is_empty() {
                        c(cx).error(pos(attrs), kError_InvalidProtected);
                        push(public, PROTECTED.to_string());
                    } else {
                        push(self.static_protected_namespaces.last().cloned().unwrap(), PROTECTED.to_string());
                    }
                } else if has_public {
                    if self.public_namespaces.is_empty() {
                        let n = public.name();
                        push(public, n);
                    } else {
                        let p = self.public_namespaces.last().cloned().unwrap();
                        let n = p.name();
                        push(p, n);
                    }
                } else if has_internal {
                    if self.public_namespaces.is_empty() {
                        let n = public.name();
                        push(public, n);
                    } else {
                        let d = self.back_default();
                        let n = d.name();
                        push(d, n);
                    }
                } else if ans.size() == 0 {
                    let d = self.back_default();
                    let n = d.name();
                    push(d, n);
                }
                ns_add_all(namespaces, &ans);
                namespace_ids.extend(attrs.borrow().as_attribute_list().unwrap().namespace_ids.iter().cloned());
            }
            None => {
                let d = self.back_default();
                namespace_ids.push(d.name());
                namespaces.push_back(d);
            }
        }
    }

    fn class_builder_on_scope_chain(&self, cx: &ContextRef) -> Option<Builder> {
        let n = Self::scope_count(cx);
        for i in (0..n).rev() {
            let bui = c(cx).scope_at(i as i32).builder();
            if let Some(b) = bui {
                if b.is_class_builder() {
                    return Some(b);
                }
            }
        }
        None
    }

    fn get_variable_definition_scope(&self, cx: &ContextRef) -> Option<ObjectValue> {
        let n = Self::scope_count(cx);
        for i in (0..n).rev() {
            let scope = c(cx).scope_at(i as i32);
            if builder_of(&scope).has_register_offset() {
                return Some(scope);
            }
        }
        None
    }

    fn adjust_protected_namespace(&mut self, cx: &ContextRef, r: &Option<ReferenceValue>) {
        let Some(r) = r else { return };
        let Some(class_builder) = self.class_builder_on_scope_chain(cx) else { return };
        let (basebui, protected_ns) = {
            let d = class_builder.class_data();
            (d.basebui.clone(), d.protected_namespace.clone())
        };
        let Some(basebui) = basebui else { return };
        let namespaces = r.get_immutable_namespaces();
        if let Some(pn) = &protected_ns {
            if namespaces.contains(pn) {
                let copy = Namespaces::copy_of(&namespaces);
                let i = copy.index_of(pn);
                copy.remove(i as usize);
                copy.add(nn(basebui.class_data().protected_namespace.clone()));
                let interned = c(cx).statics.borrow_mut().intern_namespaces.intern(&copy);
                r.set_immutable_namespaces(interned);
            }
        }
    }

    fn namespaces_contains(&self, outer: &Namespaces, inner: &Namespaces) -> bool {
        let set: Vec<ObjectValue> = outer.to_vec();
        inner.to_vec().iter().all(|ns| set.iter().any(|o| o.ptr_eq(ns)))
    }

    /// `getFullNameForInheritedSlot`.
    pub fn get_full_name_for_inherited_slot(&self, _cx: &ContextRef, declared_by: &ObjectValue, name: &str) -> String {
        let mut fullname = nn(declared_by.type_()).type_value().name().to_string();
        fullname.push('.');
        fullname.push_str(name);
        fullname
    }

    fn inherit_class_slots(&self, cframe: &TypeValue, iframe: &ObjectValue, base_type: &TypeValue, cx: &ContextRef) {
        inherit_class_slots_static(cframe, iframe, base_type, &c(cx));
    }
}

macro_rules! get {
    ($node:expr, $acc:ident . $($f:tt)+) => {{
        let v = $node.borrow().$acc().expect(CCE).$($f)+.clone();
        v
    }};
}

macro_rules! set {
    ($node:expr, $acc:ident . $($f:ident).+ = $v:expr) => {{
        let v = $v;
        $node.borrow_mut().$acc().expect(CCE).$($f).+ = v;
    }};
}

impl FlowAnalyzer {
    fn member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let base: Option<ObjectValue>;
        let selector = nn(get!(node, as_member_expression.selector));

        if get!(node, as_member_expression.base).is_none() {
            let value = self.eval(cx, &selector);
            let r = rv(value);
            set!(node, as_member_expression_mut.ref_ = r.clone());
            if let Some(r) = r {
                let kind = if r.get_kind() == SET_TOKEN { SET_TOKEN } else { GET_TOKEN };
                let slot = r.get_slot_kind(&c(cx), kind);
                r.set_slot(None);
                if slot.is_none() {
                    self.unresolved.insert(r);
                }
            }
        }

        let mut val: Option<Value>;
        let nbase = get!(node, as_member_expression.base);
        if let Some(b) = nbase {
            val = self.eval(cx, &b);
            val = val.and_then(|v| v.get_value(&c(cx)));
            base = val.as_ref().and_then(|v| v.as_object_value());
            let sel_base = Some(base.clone().unwrap_or_else(ObjectValue::undefined_value));
            selector.borrow_mut().selector_mut().expect(CCE).base = sel_base;
            val = self.eval(cx, &selector);
            let r = rv(val);
            set!(node, as_member_expression_mut.ref_ = r.clone());
            if let Some(r) = &r {
                if base.is_some() && same_obj(&base, &c(cx).global_scope()) {
                    r.set_scope_index(0);
                    set!(node, as_member_expression_mut.base = None);
                }
            }
        }

        let is_get = {
            let s = selector.borrow();
            s.is_get_expression() || s.is_apply_type_expression()
        };
        let r = get!(node, as_member_expression.ref_);
        val = if is_get { r.clone().map(Value::Reference) } else { None };

        let base_is_super = get!(node, as_member_expression.base).map_or(false, |b| b.borrow().as_super_expression().is_some());
        if base_is_super {
            self.adjust_protected_namespace(cx, &r);
        }
        val
    }

    fn statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let node_factory = Context::get_node_factory(cx);
        let has_pragma = get!(node, as_statement_list.has_pragma);
        if has_pragma {
            let block_usage = NumberUsage::copy_of(&nn(self.number_usage_stack.last().cloned()));
            self.number_usage_stack.push(block_usage);
        }
        let obj = Self::scope(cx);
        let inside_class = is_instance_builder(&obj) || is_class_builder(&obj);
        let inside_cinit = ilast(&self.this_contexts) == cinit_this;

        let mut hoisted_defs: Vec<Option<NodeRef>> = Vec::new();
        let instance_inits = self.instanceinits_sets.last().cloned();

        {
            let size = node.borrow().as_statement_list().unwrap().items.len();
            for i in (0..size).rev() {
                let item = node.borrow().as_statement_list().unwrap().items[i].clone();
                let Some(mut n) = item else { continue };
                let mut label: Option<NodeRef> = None;
                if n.borrow().as_labeled_statement().is_some() {
                    label = Some(n.clone());
                    let s = nn(get!(n, as_labeled_statement.statement));
                    n = s;
                }
                let replace = |with: NodeRef| match &label {
                    None => node.borrow_mut().as_statement_list_mut().unwrap().items[i] = Some(with),
                    Some(l) => l.borrow_mut().as_labeled_statement_mut().unwrap().statement = Some(with),
                };
                if n.borrow().is_definition() {
                    let def = if n.borrow().definition().is_some() { Some(n.clone()) } else { None };
                    let (is_include_or_import, is_const) = {
                        let b = n.borrow();
                        (matches!(b.kind, Kind::IncludeDirective(_) | Kind::ImportDirective(_)), b.is_const())
                    };
                    if is_include_or_import {
                        hoisted_defs.push(def);
                    } else if is_const || inside_class {
                        hoisted_defs.push(def);
                        let es = node_factory.borrow_mut().empty_statement();
                        replace(es);
                    } else {
                        hoisted_defs.push(def.clone());
                        let attrs = nn(def).borrow().definition().unwrap().attrs.clone();
                        if attrs.as_ref().map_or(false, |a| a.borrow().has_attribute(INTRINSIC)) {
                            let es = node_factory.borrow_mut().empty_statement();
                            replace(es);
                        } else {
                            let init = initializer_statement(&n, cx);
                            replace(init);
                        }
                    }
                } else if !self.endpoint_dominator_is_set
                    && self.loop_index == 0
                    && n.borrow().is_expression_statement()
                    && Self::scope_count(cx) == 1
                {
                    self.endpoint_dominator = Some(n.clone());
                    self.endpoint_dominator_is_set = true;
                }
            }
        }

        let mut inits: Vec<NodeRef> = Vec::new();
        for i in (0..hoisted_defs.len()).rev() {
            let def = nn(hoisted_defs[i].clone());
            let mut is_static = false;
            let mut is_intrinsic = false;
            let (is_include, is_namespace, is_use, is_class) = {
                let b = def.borrow();
                (
                    matches!(b.kind, Kind::IncludeDirective(_)),
                    b.namespace_definition().is_some(),
                    matches!(b.kind, Kind::UseDirective(_) | Kind::ImportDirective(_)),
                    b.class_definition().is_some(),
                )
            };
            let is_const = false;
            let attrs = def.borrow().definition().unwrap().attrs.clone();
            if is_namespace || is_const || is_use || is_include {
                if let Some(a) = &attrs {
                    self.eval(cx, a);
                    is_static = get!(a, as_attribute_list.has_static);
                    is_intrinsic = get!(a, as_attribute_list.has_intrinsic);
                }
                self.eval(cx, &def);
            } else if let Some(a) = &attrs {
                self.eval(cx, a);
                is_static = get!(a, as_attribute_list.has_static);
                is_intrinsic = get!(a, as_attribute_list.has_intrinsic);
            }

            let needs_init = (!is_use && !is_namespace && !is_intrinsic && def.borrow().is_const()) || inside_class;

            if inside_cinit && !is_static {
                let ii = nn(instance_inits.clone());
                ii.borrow_mut().push(def.clone());
                if needs_init {
                    let init = initializer_statement(&def, cx);
                    ii.borrow_mut().push(init);
                }
            } else {
                if !is_use && !(is_class && needs_init) {
                    inits.push(def.clone());
                }
                if needs_init {
                    inits.push(initializer_statement(&def, cx));
                }
            }
        }

        for i in (0..inits.len()).rev() {
            node.borrow_mut().as_statement_list_mut().unwrap().items.insert(0, Some(inits[i].clone()));
        }

        let mut i = 0;
        loop {
            let n = {
                let b = node.borrow();
                let items = &b.as_statement_list().unwrap().items;
                if i >= items.len() {
                    break;
                }
                items[i].clone()
            };
            i += 1;
            if same_node(&n, &self.endpoint_dominator) {
                self.define_cv = true;
            } else if !self.endpoint_dominator_is_set {
                self.define_cv = true;
                self.endpoint_dominator_is_set = true;
            }

            if let Some(n) = n {
                if !self.doing_method() && !n.borrow().is_definition() {
                    let name = self.back_fun_name();
                    let (p, l, t) = (ilast(&self.max_params_stack), ilast(&self.max_locals_stack), ilast(&self.max_temps_stack));
                    self.start_method(&name, p, l, t, false, 0);
                }
                self.eval(cx, &n);
            }
        }

        if !self.doing_method() {
            let name = self.back_fun_name();
            let (p, l, t) = (ilast(&self.max_params_stack), ilast(&self.max_locals_stack), ilast(&self.max_temps_stack));
            self.start_method(&name, p, l, t, false, 0);
        }

        if has_pragma {
            set!(node, as_statement_list_mut.number_usage = self.number_usage_stack.last().cloned());
            self.number_usage_stack.pop();
        }
        None
    }

    fn switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let t = self.allocate_temp();
        self.case_exprs.push(CaseList::default());
        self.switch_begin();
        let statements = get!(node, as_switch_statement.statements);
        if let Some(s) = &statements {
            self.inside_switch += 1;
            set!(node, as_switch_statement_mut.loop_index = self.loop_index);
            self.loop_index += 1;
            self.eval(cx, s);
            self.loop_index -= 1;
            self.inside_switch -= 1;
        }
        let node_loop_index = get!(node, as_switch_statement.loop_index);

        let has_default = nn(self.case_exprs.last()).has_default;
        if !has_default {
            self.case_exprs.last_mut().unwrap().items.push(None);
            self.case_label(true);
            self.break_(node_loop_index);
        }

        self.break_(node_loop_index);
        let ip = self.get_ip();
        self.patch_switch_begin(ip);
        self.eval_opt(cx, &get!(node, as_switch_statement.expr));

        {
            let mut case_index: i32 = 0;
            let mut default_index = 0;
            let case_expr = nn(self.case_exprs.pop());
            let case_expr_size = case_expr.items.len() as i32;
            if case_expr_size != 0 {
                case_index = 0;
                while case_index < case_expr_size {
                    let expr = case_expr.items[case_index as usize].clone();
                    match &expr {
                        Some(e) => {
                            self.eval(cx, e);
                        }
                        None => default_index = case_index,
                    }
                    self.if_(IF_false);
                    self.push_case_index(case_index);
                    self.else_();
                    let ip = self.get_ip();
                    self.patch_if(ip);
                    case_index += 1;
                }
            }
            self.push_case_index(default_index);
            while case_index != 0 {
                case_index -= 1;
                let ip = self.get_ip();
                self.patch_else(ip);
            }
        }
        self.switch_table();
        self.patch_continue(node_loop_index);
        self.patch_break(node_loop_index);
        self.free_temp(t);
        None
    }

    fn function_common(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = nn(get!(node, as_function_common.cx));
        let def = get!(node, as_function_common.def);
        let pkgdef = def.and_then(|d| d.borrow().definition().expect(CCE).pkgdef.clone());
        if let Some(p) = &pkgdef {
            if Self::scope_count(&cx) == 1 {
                self.push_package_namespaces(p, true);
            }
        }

        let saved_with_depth = Self::with_depth(&cx);
        let node_with_depth = get!(node, as_function_common.with_depth);
        if node_with_depth != -1 {
            Self::set_with_depth(&cx, node_with_depth);
        }

        let mut saved_scopes = None;
        if let Some(chain) = get!(node, as_function_common.scope_chain) {
            saved_scopes = Some(c(&cx).swap_scope_chain(chain));
        }

        if get!(node, as_function_common.fun).is_none() {
            let used = self.back_used();
            if get!(node, as_function_common.used_namespaces).is_none() {
                set!(node, as_function_common_mut.used_namespaces = Some(Namespaces::with_capacity(used.size())));
            }
            ns_add_all(&nn(get!(node, as_function_common.used_namespaces)), &used);
            if get!(node, as_function_common.imported_names).is_none() {
                set!(node, as_function_common_mut.imported_names = Some(Multinames::new()));
            }
            let imported = self.back_imported().snapshot();
            node.borrow_mut().as_function_common_mut().unwrap().imported_names.as_mut().unwrap().put_all(&imported);
            set!(node, as_function_common_mut.private_namespace = self.private_namespaces.last().cloned());
            set!(node, as_function_common_mut.default_namespace = self.default_namespaces.last().cloned());
            set!(node, as_function_common_mut.public_namespace = Some(c(&cx).public_namespace()));

            let fun = {
                let cxb = c(&cx);
                ObjectValue::with_builder(&cxb, new_function_builder(), Some(&cxb.function_type()))
            };
            set!(node, as_function_common_mut.fun = Some(fun.clone()));

            let mut is_named_anon = false;
            let (is_fundef, ident, user_body) = {
                let b = node.borrow();
                let f = b.as_function_common().unwrap();
                (f.is_function_definition(), f.identifier.clone(), f.is_user_defined_body())
            };
            let ident_name = ident.as_ref().map(|i| i.borrow().identifier().expect(CCE).name.clone());
            if !is_fundef && ident_name.is_some() && ident_name.as_deref() != Some("anonymous") && user_body {
                is_named_anon = true;
                node.borrow_mut().as_function_common_mut().unwrap().set_named_inner_func(true);
                let name = ident_name.unwrap();
                let cxb = c(&cx);
                let temp_ns = Namespaces::of(cxb.public_namespace());
                let fb = builder_of(&fun);
                let method_id = fb.method(&cxb, &fun, &name, &temp_ns, false);
                fb.explicit_call_nss(&cxb, &fun, &name, &temp_ns, Some(&cxb.no_type()), false, false, -1, method_id, -1);
                cxb.push_scope(fun.clone());
            }

            let wd = Self::with_depth(&cx);
            if wd != -1 {
                set!(node, as_function_common_mut.with_depth = wd);
            }

            let scope_chain = c(&cx).get_scopes();
            let mut s = scope_chain.len();
            while s > 0 {
                let b = builder_of(&scope_chain[s - 1]);
                if b.is_catch_builder() || b.is_with_builder() || b.is_activation_builder() {
                    set!(node, as_function_common_mut.scope_chain = Some(scope_chain.clone()));
                    break;
                }
                s -= 1;
            }

            if is_named_anon {
                c(&cx).pop_scope();
            }

            let scope = Self::scope(&cx);
            let set = if is_class_builder(&scope) { self.staticfexprs_sets.last().cloned() } else { self.fexprs_sets.last().cloned() };
            let set = nn(set);
            if !list_contains(&set, node) {
                set.borrow_mut().push(node.clone());
            }
        } else if self.doing_method() {
        } else if get!(node, as_function_common.ref_).is_none() {
            if let Some(u) = get!(node, as_function_common.used_namespaces) {
                self.usednamespaces_sets.push(u);
            }
            if get!(node, as_function_common.imported_names).is_some() {
                self.importednames_sets.push(ImportedNames::Node(node.clone()));
            }

            self.fexprs_sets.push(new_node_list());
            self.staticfexprs_sets.push(new_node_list());
            self.instanceinits_sets.push(new_node_list());

            let val = self.eval_opt(&cx, &get!(node, as_function_common.identifier));
            let r = rv(val);
            set!(node, as_function_common_mut.ref_ = r.clone());
            let r = nn(r);
            let fun = nn(get!(node, as_function_common.fun));
            let internal_name = get!(node, as_function_common.internal_name);
            fun.set_name(&internal_name);

            let (namespace_ids, kind) = (get!(node, as_function_common.namespace_ids), get!(node, as_function_common.kind));
            let region = c(&cx).debug_name(&self.back_region(), &r.name(), namespace_ids.as_deref(), kind);
            self.region_name_stack.push(region);

            let activation = {
                let cxb = c(&cx);
                ObjectValue::with_builder(&cxb, new_activation_builder(), Some(&cxb.no_type()))
            };
            fun.set_activation(Some(activation.clone()));
            {
                let n = Self::scope_count(&cx) as i32;
                let obj = c(&cx).scope_at(n - 1);
                let classname = builder_of(&obj).classname();
                builder_of(&activation).data_mut().classname = classname;
                activation.set_name(&internal_name);
            }

            c(&cx).push_scope(activation.clone());

            let signature = get!(node, as_function_common.signature);
            let (has_body, var_count, temp_count) =
                (get!(node, as_function_common.body).is_some(), get!(node, as_function_common.var_count), get!(node, as_function_common.temp_count));
            let sig_size = nn(signature.as_ref()).borrow().as_function_signature().expect(CCE).size();
            self.fun_name_stack.push(internal_name.clone());
            self.max_params_stack.push(sig_size);
            self.max_locals_stack.push(if has_body { var_count } else { 0 });
            self.max_temps_stack.push(if has_body { temp_count } else { 0 });
            self.with_used_stack.push(0);
            self.exceptions_used_stack.push(0);

            if let Some(u) = get!(node, as_function_common.use_stmts) {
                self.eval(&cx, &u);
            }

            self.flush_unresolved(Unresolved::Rt);
            self.eval_opt(&cx, &signature);
            self.flush_unresolved(Unresolved::Ce);

            let scope_index = Self::scope_count(&cx) as i32 - 1;
            let obj = c(&cx).scope_at(scope_index);
            if (get!(node, as_function_common.needs_arguments) & METHOD_Arguments) != 0 {
                let namespace = nn(get!(node, as_function_common.default_namespace));
                let cxb = c(&cx);
                if !obj.has_name(&cxb, GET_TOKEN, "arguments", &namespace) {
                    let bui = builder_of(&obj);
                    let var_id = bui.variable(&cxb, &obj);
                    bui.explicit_var(&cxb, &obj, "arguments", &namespace, Some(&cxb.array_type()), -1, -1, var_id);
                } else {
                    drop(cxb);
                    node.borrow_mut().as_function_common_mut().unwrap().needs_arguments ^= METHOD_Arguments;
                }
            }

            let is_constructor = r.name() == "$construct";
            if is_constructor && nn(signature.as_ref()).borrow().as_function_signature().unwrap().result.is_some() {
                c(&cx).error(pos(node), kError_CtorWithReturnType);
            }

            if has_body {
                if !is_fundef_of(node) {
                    self.this_contexts.push(global_this);
                }
                let state = if is_constructor { super_statement } else { super_error };
                self.super_context.push(state);

                let inits = nn(signature.as_ref()).borrow().as_function_signature().unwrap().inits.clone();
                if let Some(inits) = &inits {
                    let scope_depth = Self::scope_count(&cx) as i32;
                    let iframe = c(&cx).scope_at(scope_depth - 2);
                    if is_instance_builder(&iframe) {
                        iframe.set_init_only(true);
                    }
                    self.this_contexts.push(init_this);
                    self.eval(&cx, inits);
                    ipop_back(&mut self.this_contexts);
                    if is_instance_builder(&iframe) {
                        iframe.set_init_only(false);
                    }
                }

                if is_constructor && c(&cx).dialect(11) {
                    iset_last(&mut self.super_context, super_error_es4);
                }

                self.eval_opt(&cx, &get!(node, as_function_common.body));

                let tc = self.get_temp_count();
                set!(node, as_function_common_mut.temp_count = tc);
                let sig_size = nn(signature.as_ref()).borrow().as_function_signature().unwrap().size();
                set!(node, as_function_common_mut.var_count = activation.var_count() - sig_size);

                ipop_back(&mut self.super_context);
                if !is_fundef_of(node) {
                    ipop_back(&mut self.this_contexts);
                }
            } else {
                let name = self.back_fun_name();
                let p = ilast(&self.max_params_stack);
                self.start_method_counts(&name, p, 0);
            }

            let (needs_arguments, debug_name, is_native) =
                (get!(node, as_function_common.needs_arguments), get!(node, as_function_common.debug_name), node.borrow().as_function_common().unwrap().is_native());
            let name = self.back_fun_name();
            let depth = Self::scope_count(&cx) as i32;
            self.finish_method(&cx, &name, None, None, Some(&activation), needs_arguments, depth, &debug_name, is_native, false, None);

            if ilast(&self.with_used_stack) != 0 {
                node.borrow_mut().as_function_common_mut().unwrap().set_with_used(true);
            }
            self.with_used_stack.pop();

            if ilast(&self.exceptions_used_stack) != 0 {
                node.borrow_mut().as_function_common_mut().unwrap().set_exceptions_used(true);
            }
            self.exceptions_used_stack.pop();

            self.fun_name_stack.pop();

            let fexprs = self.fexprs_sets.last().cloned();
            set!(node, as_function_common_mut.fexprs = fexprs.clone());

            self.this_contexts.push(global_this);
            self.eval_list(&cx, &fexprs);
            ipop_back(&mut self.this_contexts);

            self.instanceinits_sets.pop();
            self.staticfexprs_sets.pop();
            self.fexprs_sets.pop();
            c(&cx).pop_scope();

            self.region_name_stack.pop();
            self.usednamespaces_sets.pop();
            self.importednames_sets.pop();
        }

        if let Some(saved) = saved_scopes {
            c(&cx).swap_scope_chain(saved);
        }
        Self::set_with_depth(&cx, saved_with_depth);

        if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
            self.pop_package_namespaces(true);
        }

        get!(node, as_function_common.fun).map(Value::Object)
    }

    /// `for (Node n : list) n.evaluate(cx, this)` over a list the node shares with an evaluator stack.
    fn eval_list(&mut self, cx: &ContextRef, list: &Option<NodeList>) {
        let list = nn(list.clone());
        let mut i = 0;
        loop {
            let n = {
                let l = list.borrow();
                if i >= l.len() {
                    break;
                }
                l[i].clone()
            };
            self.eval(cx, &n);
            i += 1;
        }
    }

    fn process_imports(&mut self, cx: &ContextRef, node: &NodeRef, imports: &[NodeRef]) {
        let frame = Self::scope(cx);
        let global_builder = new_global_builder();
        let import_frame = ObjectValue::with_builder(&c(cx), global_builder, None);
        node.borrow_mut().program_mut().unwrap().import_frame = Some(import_frame.clone());

        let nf = Context::get_node_factory(cx);
        let mut imported_program_nodes: Option<NodeRef> = None;
        for it in imports {
            let it_program = get!(it, as_import.program);
            match &imported_program_nodes {
                None => {
                    let ls = nf.borrow_mut().literal_string_pos("", 0);
                    imported_program_nodes = Some(nf.borrow_mut().import(Some(ls), it_program));
                }
                Some(ipn) => {
                    let target = nn(get!(ipn, as_import.program));
                    let src = nn(it_program);
                    let (items, pkgdefs) = {
                        let s = src.borrow();
                        let p = s.program().expect(CCE);
                        let st = nn(p.statements.clone());
                        let items = st.borrow().as_statement_list().expect(CCE).items.clone();
                        (items, p.pkgdefs.clone())
                    };
                    let tst = nn(target.borrow().program().expect(CCE).statements.clone());
                    tst.borrow_mut().as_statement_list_mut().expect(CCE).items.extend(items);
                    target.borrow_mut().program_mut().unwrap().pkgdefs.extend(pkgdefs);
                }
            }
        }

        if let Some(ipn) = imported_program_nodes {
            c(cx).pop_scope();
            c(cx).push_scope(import_frame);
            self.import_context.push(true);
            let prog = nn(get!(ipn, as_import.program));
            self.eval(cx, &prog);
            self.eval(cx, &prog);
            self.import_context.pop();
            c(cx).pop_scope();
            c(cx).push_scope(frame);
            self.eval(cx, &ipn);
        }
    }

    fn sort_class_definitions(&mut self, cx: &ContextRef, clsdefs: Option<NodeList>) -> Option<NodeList> {
        let list = match &clsdefs {
            None => return clsdefs,
            Some(l) if l.borrow().is_empty() => return clsdefs,
            Some(l) => l.borrow().clone(),
        };

        let mut g: DependencyGraph<NodeRef> = DependencyGraph::new();
        for clsdef in &list {
            let (cframe, interfaces) = {
                let b = clsdef.borrow();
                let cd = b.class_definition().expect(CCE);
                (nn(cd.cframe.clone()), cd.interfaces.clone())
            };
            let class_name = nn(builder_of(&cframe).classname()).to_string();
            g.put(&class_name, clsdef.clone());
            if !g.contains_vertex(&class_name) {
                g.add_vertex(Vertex::new(class_name.clone()));
            }
            if let Some(bc) = cframe.baseclass() {
                let bname = bc.name().to_string();
                if bname != "Class" {
                    g.add_dependency(&class_name, &bname);
                }
            }
            if let Some(interfaces) = &interfaces {
                let values = interfaces.borrow().list().expect(CCE).values.clone();
                for val in values {
                    if let Some(Value::Reference(r)) = val {
                        let v2 = r.get_value(&c(cx));
                        if let Some(t) = v2.and_then(|v| v.as_type_value()) {
                            g.add_dependency(&class_name, &nn(builder_of(&t).classname()).to_string());
                        }
                    }
                }
            }
        }

        let mut tsort: Vec<NodeRef> = Vec::new();
        {
            let mut visit = |v: &Vertex<String, ()>| {
                let name = v.get_weight().clone();
                if g.contains_key(&name) {
                    tsort.push(g.get(&name).unwrap().clone());
                }
            };
            topological_sort(&g, Some(&mut visit));
        }

        if list.len() > tsort.len() {
            for clsdef in &list {
                if !tsort.iter().any(|t| Rc::ptr_eq(t, clsdef)) {
                    let name = {
                        let b = clsdef.borrow();
                        let cf = nn(b.class_definition().unwrap().cframe.clone());
                        nn(builder_of(&cf).classname()).name().to_string()
                    };
                    c(cx).error1(pos(clsdef), kError_CircularReference, &name);
                    self.found_circular_or_duplicate_class_definition = true;
                }
            }
            clsdefs
        } else {
            Some(Rc::new(RefCell::new(tsort)))
        }
    }

    fn variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut is_intrinsic = false;
        let is_const = get!(node, as_variable_binding.kind) == CONST_TOKEN;
        let namespaces = Namespaces::new();
        let mut namespace_ids: Vec<String> = Vec::new();
        let attrs = get!(node, as_variable_binding.attrs);

        if let Some(a) = &attrs {
            let (hv, hf, hn, hi) = {
                let b = a.borrow();
                let a = b.as_attribute_list().expect(CCE);
                (a.has_virtual, a.has_final, a.has_native, a.has_intrinsic)
            };
            if hv && hf {
                c(cx).error(pos(node), kError_VarIsFinalAndVirtual);
            }
            if hn {
                c(cx).error(pos(node), kError_NativeVars);
            }
            if hv {
                c(cx).error(pos(node), kError_VirtualVars);
            }
            is_intrinsic = hi;
        }

        let variable = nn(get!(node, as_variable_binding.variable));
        let var_type = get!(variable, as_typed_identifier.type_);
        let ident = || nn(get!(variable, as_typed_identifier.identifier));

        if attrs.is_none() && var_type.is_none() {
            let ns = self.back_default();
            namespaces.push_back(ns.clone());
            namespace_ids.push(ns.name());
            let nf = Context::get_node_factory(cx);
            let is_public = ns.ptr_eq(&c(cx).public_namespace());
            let id = nf.borrow_mut().identifier_intern_pos(if is_public { PUBLIC } else { INTERNAL }, false, pos(&variable));
            let aln = nf.borrow_mut().attribute_list(Some(id), None);
            {
                let mut b = aln.borrow_mut();
                let a = b.as_attribute_list_mut().unwrap();
                if is_public {
                    a.has_public = true;
                } else {
                    a.has_internal = true;
                }
                a.namespaces.push_back(ns);
            }
            let identifier = ident();
            if identifier.borrow().qualified_identifier().is_some() {
                identifier.borrow_mut().qualified_identifier_mut().unwrap().qualifier = Some(aln);
            } else {
                let (name, p) = {
                    let b = identifier.borrow();
                    (b.identifier().expect(CCE).name.clone(), b.pos())
                };
                let q = nf.borrow_mut().qualified_identifier_pos(Some(aln), Some(&name), p);
                set!(variable, as_typed_identifier_mut.identifier = Some(q));
            }
            let val = self.eval(cx, &ident());
            set!(node, as_variable_binding_mut.ref_ = rv(val));
        } else {
            self.compute_namespaces(cx, &attrs, &namespaces, &mut namespace_ids);
            let val = self.eval(cx, &ident());
            set!(node, as_variable_binding_mut.ref_ = rv(val));
            if !node.borrow().in_package() && Self::scope_count(cx) == 1 {
                if let Some(a) = &attrs {
                    if a.borrow().has_attribute(PUBLIC) {
                        c(cx).error(pos(a), kError_InvalidPublic);
                    }
                }
            }
        }

        let mut val = self.eval(cx, &ident());
        set!(node, as_variable_binding_mut.ref_ = rv(val.clone()));
        let r = nn(get!(node, as_variable_binding.ref_));
        let ref_name = r.name();

        let initializer = get!(node, as_variable_binding.initializer);
        if let Some(init) = &initializer {
            let es4 = c(cx).statics.borrow().es4_nullability;
            if es4 && is_instance_builder(&Self::scope(cx)) {
                Self::scope(cx).set_init_only(true);
                self.this_contexts.push(init_this);
            }
            self.eval(cx, init);
            if es4 && is_instance_builder(&Self::scope(cx)) {
                Self::scope(cx).set_init_only(false);
                ipop_back(&mut self.this_contexts);
            }
        }

        let mut obj = nn(self.get_variable_definition_scope(cx));
        let bui = builder_of(&obj);

        if bui.is_instance_builder() && self.fun_name_stack.last().map_or(false, |f| *f == ref_name) {
            c(cx).error(pos(node), kError_ConstructorsMustBeInstanceMethods);
        }

        let has_user_ns = attrs.as_ref().map_or(false, |a| a.borrow().as_attribute_list().unwrap().has_user_namespace());
        let open_definition_namespaces = if has_user_ns { namespaces.clone() } else { self.back_used_def() };

        if let Some(t) = &var_type {
            self.flush_unresolved(Unresolved::Rt);
            val = self.eval(cx, t);
            obj = nn(self.get_variable_definition_scope(cx));
            if builder_of(&obj).is_activation_builder() {
                self.flush_unresolved(Unresolved::Body);
            } else {
                self.flush_unresolved(Unresolved::Ce);
            }
            let typeref = rv(val.clone());
            set!(node, as_variable_binding_mut.typeref = typeref.clone());
            match &typeref {
                Some(tr) => tr.set_type_annotation(true),
                None => c(cx).error1(pos(t), kError_UnknownType, ""),
            }
        }
        let typeref = get!(node, as_variable_binding.typeref);
        let block_or_no_init = node.borrow().block.is_some() || initializer.is_none();

        let mut kind = GET_TOKEN;
        let mut matching = obj.has_names(&c(cx), GET_TOKEN, &ref_name, &open_definition_namespaces);
        if matching.is_none() {
            matching = obj.has_names(&c(cx), SET_TOKEN, &ref_name, &open_definition_namespaces);
            kind = SET_TOKEN;
        }
        match matching {
            None => {
                let cxb = c(cx);
                let type_ = cxb.no_type();
                if bui.is_intrinsic() || is_intrinsic {
                    bui.explicit_var_nss(&cxb, &obj, &ref_name, &namespaces, Some(&type_), -1, -1, -1);
                } else {
                    let var_id = bui.variable(&cxb, &obj);
                    let slot_id = bui.explicit_var_nss(&cxb, &obj, &ref_name, &namespaces, Some(&type_), -1, -1, var_id);
                    let slot = nn(obj.get_slot(&cxb, slot_id));
                    slot.set_const(is_const);
                    slot.set_type_ref(typeref.clone());
                    if block_or_no_init {
                        slot.set_needs_init(true);
                    }
                }
            }
            Some(matching) => {
                let mut nsstr = String::new();
                for ns in matching.to_vec() {
                    if !nsstr.is_empty() {
                        nsstr.push(' ');
                    }
                    if ns.name_ref().is_empty() {
                        nsstr.push_str(PUBLIC);
                    } else {
                        match ns.get_namespace_kind() {
                            NS_PRIVATE => nsstr.push_str(PRIVATE),
                            NS_INTERNAL => nsstr.push_str(INTERNAL),
                            NS_PROTECTED => nsstr.push_str(PROTECTED),
                            _ => nsstr.push_str(&ns.name()),
                        }
                    }
                }

                let slot_index = obj.get_slot_index(&c(cx), kind, &ref_name, Some(&matching.back()));
                let orig = obj.get_slot(&c(cx), slot_index);

                let is_global_definition = bui.is_global_builder() && !node.borrow().in_package();
                let is_local_definition = bui.is_activation_builder();
                let is_global_or_local = is_global_definition || is_local_definition;
                let ident_pos = pos(&ident());

                if is_global_or_local && attrs.is_none() && var_type.is_none() {
                    let o = nn(orig.clone());
                    let tv = nn(o.get_type()).get_type_value();
                    if tv.map_or(false, |t| t.ptr_eq(&c(cx).type_type())) || o.is_const() {
                        c(cx).error2(ident_pos, kError_ConflictingNameInNamespace, &ref_name, "internal");
                    } else {
                        let nf = Context::get_node_factory(cx);
                        let id = nf.borrow_mut().identifier_pos(&matching.back().name(), pos(&variable));
                        let aln = nf.borrow_mut().attribute_list(Some(id), None);
                        {
                            let mut b = aln.borrow_mut();
                            let a = b.as_attribute_list_mut().unwrap();
                            a.items.clear();
                            ns_add_all(&a.namespaces, &matching);
                        }
                        let identifier = ident();
                        if identifier.borrow().qualified_identifier().is_some() {
                            identifier.borrow_mut().qualified_identifier_mut().unwrap().qualifier = Some(aln);
                        } else {
                            let (name, p) = {
                                let b = identifier.borrow();
                                (b.identifier().expect(CCE).name.clone(), b.pos())
                            };
                            let q = nf.borrow_mut().qualified_identifier_pos(Some(aln), Some(&name), p);
                            set!(variable, as_typed_identifier_mut.identifier = Some(q));
                        }
                        ident().borrow_mut().identifier_mut().expect(CCE).ref_ = None;
                        let val2 = self.eval(cx, &ident());
                        let r2 = if rv(val.clone()).is_some() { Some(val2.map(|v| v.as_reference_value().expect(CCE))).flatten() } else { None };
                        set!(node, as_variable_binding_mut.ref_ = r2);
                    }
                } else if !nn(orig.clone()).declared_by().map_or(false, |d| d.ptr_eq(&obj)) {
                    let fullname = self.get_full_name_for_inherited_slot(cx, &nn(nn(orig.clone()).declared_by()), &ref_name);
                    c(cx).error2(ident_pos, kError_ConflictingInheritedNameInNamespace, &fullname, &nsstr);
                } else {
                    let o = nn(orig.clone());
                    let compatible = is_global_or_local
                        && !o.is_const()
                        && (o.get_type_ref().is_none() || typeref.is_none() || o.get_type_ref().unwrap().name() == typeref.as_ref().unwrap().name());
                    if !compatible {
                        c(cx).error2(ident_pos, kError_ConflictingNameInNamespace, &ref_name, "internal");
                    }
                }
                if block_or_no_init {
                    nn(orig).set_needs_init(true);
                }
            }
        }

        let ref_name = nn(get!(node, as_variable_binding.ref_)).name();
        let dn = c(cx).debug_name(&self.back_region(), &ref_name, Some(&namespace_ids), VAR_TOKEN);
        set!(node, as_variable_binding_mut.debug_name = Some(dn));
        None
    }
}

fn is_fundef_of(node: &NodeRef) -> bool {
    node.borrow().as_function_common().unwrap().is_function_definition()
}

impl FlowAnalyzer {
    fn program(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let state = get!(node, program.state);
        if state == ProgramNode::Inheritance {
            if get!(node, program.frame).is_some() {
                return None;
            }
            let cx = nn(get!(node, program.cx));
            self.program_node_temp = self.allocate_temp();
            self.strict_context.push(false);
            set!(node, program_mut.frame = c(&cx).scope());

            let imports = get!(node, program.imports);
            self.process_imports(&cx, node, &imports);

            self.define_cv = false;
            self.endpoint_dominator_is_set = false;

            self.start_program("");

            self.this_contexts.push(global_this);
            self.super_context.push(super_error);
            self.number_usage_stack.push(NumberUsage::new());

            self.fexprs_sets.push(new_node_list());
            self.clsdefs_sets.push(new_node_list());
            self.staticfexprs_sets.push(new_node_list());
            self.instanceinits_sets.push(new_node_list());

            self.unresolved_sets.push(node.clone());

            set!(node, program_mut.fexprs = self.fexprs_sets.last().cloned());
            set!(node, program_mut.clsdefs = self.clsdefs_sets.last().cloned());

            self.fun_name_stack.push("$init".to_string());
            self.max_params_stack.push(0);
            self.max_locals_stack.push(get!(node, program.var_count));
            self.max_temps_stack.push(get!(node, program.temp_count));

            set!(node, program_mut.var_count = 0);

            let public = c(&cx).public_namespace();
            set!(node, program_mut.public_namespace = Some(public.clone()));
            let default_ns = {
                let cxb = c(&cx);
                let name = cxb.get_file_internal_namespace_name();
                cxb.get_namespace_kind(&name, NS_PRIVATE)
            };
            self.local_file_namespace = Some(default_ns.clone());
            set!(node, program_mut.default_namespace = Some(default_ns.clone()));

            self.public_namespaces.push(public.clone());
            self.default_namespaces.push(default_ns.clone());

            self.usednamespaces_sets.push(get!(node, program.used_def_namespaces));
            self.used_def_namespaces_sets.push(Namespaces::new());
            self.importednames_sets.push(ImportedNames::Own(Rc::new(RefCell::new(Multinames::new()))));
            self.back_used().push_back(public.clone());
            self.back_used().push_back(default_ns.clone());
            self.back_used_def().push_back(public);
            self.back_used_def().push_back(default_ns);
            self.region_name_stack.push(String::new());

            if get!(node, program.statements).is_some() {
                let pkgdefs = get!(node, program.pkgdefs);
                for it in &pkgdefs {
                    self.eval(&cx, it);
                }
                self.eval_opt(&cx, &get!(node, program.statements));
                self.define_cv = false;
            }

            let tc = self.get_temp_count();
            set!(node, program_mut.temp_count = tc);

            self.return_(TYPE_none);
            self.finish_method(&cx, "$init", None, None, None, 0, 1, "", false, false, None);

            let defs = {
                let f = self.flow.borrow();
                let out_bits = f.get_block().borrow().out_bits.clone();
                f.get_defs(&out_bits)
            };
            let no_type = c(&cx).no_type();
            for n in defs {
                nn(n).borrow_mut().expected_type(Some(no_type.clone()));
            }

            self.eval_list(&cx, &get!(node, program.fexprs));

            self.fexprs_sets.pop();

            self.resolve_inheritance = true;
            self.eval_list(&cx, &get!(node, program.clsdefs));
            self.resolve_inheritance = false;

            self.flush_unresolved(Unresolved::Rt);

            set!(node, program_mut.state = ProgramNode::Else);
        } else {
            let cx = nn(get!(node, program.cx));
            if !self.found_circular_or_duplicate_class_definition {
                let sorted = self.sort_class_definitions(&cx, get!(node, program.clsdefs));
                set!(node, program_mut.clsdefs = sorted);
            }
            if !self.found_circular_or_duplicate_class_definition {
                self.eval_list(&cx, &get!(node, program.clsdefs));
            }

            self.flush_unresolved(Unresolved::Rt);

            self.unresolved_sets.pop();

            self.clsdefs_sets.pop();
            self.instanceinits_sets.pop();
            self.staticfexprs_sets.pop();
            self.region_name_stack.pop();

            self.super_context.pop();
            self.this_contexts.pop();
            self.strict_context.pop();

            self.importednames_sets.pop();
            self.usednamespaces_sets.pop();
            self.used_def_namespaces_sets.pop();
            self.default_namespaces.pop();
            self.public_namespaces.pop();

            self.finish_program(&cx, "", 0);
            let t = self.program_node_temp;
            self.free_temp(t);

            set!(node, program_mut.state = ProgramNode::Done);
        }
        None
    }

    fn package_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = nn(get!(node, as_package_definition.cx));
        if get!(node, as_package_definition.public_namespace).is_none() {
            if self.doing_package() && self.import_context.last().copied() != Some(true) {
                c(&cx).error(pos(node), kError_NestedPackage);
                return None;
            }
            if get!(node, as_package_definition.ref_).is_none() {
                let v = self.eval_opt(&cx, &get!(node, as_package_definition.name));
                let r = v.map(|v| v.as_reference_value().expect(CCE));
                set!(node, as_package_definition_mut.ref_ = r);
            }
            let name = nn(get!(node, as_package_definition.ref_)).name();
            let (public, internal) = {
                let cxb = c(&cx);
                let p = cxb.get_namespace(&name);
                p.set_package(true);
                (p, cxb.get_namespace_kind(&name, NS_INTERNAL))
            };
            set!(node, as_package_definition_mut.public_namespace = Some(public.clone()));
            set!(node, as_package_definition_mut.internal_namespace = Some(internal.clone()));
            let (used, used_def) = (get!(node, as_package_definition.used_namespaces), get!(node, as_package_definition.used_def_namespaces));
            let lfn = nn(self.local_file_namespace.clone());
            used.push_back(lfn.clone());
            used_def.push_back(lfn);
            let cpub = c(&cx).public_namespace();
            used.push_back(cpub.clone());
            used_def.push_back(cpub.clone());
            if !cpub.ptr_eq(&public) {
                used.push_back(public.clone());
                used_def.push_back(public);
            }
            used.push_back(internal.clone());
            used_def.push_back(internal);
        } else if !get!(node, as_package_definition.in_this_pkg) {
            set!(node, as_package_definition_mut.in_this_pkg = true);
            self.this_contexts.push(package_this);
            self.super_context.push(super_error);
            self.strict_context.push(true);
            self.push_package_namespaces(node, true);
        } else {
            set!(node, as_package_definition_mut.in_this_pkg = false);
            ipop_back(&mut self.this_contexts);
            ipop_back(&mut self.super_context);
            self.strict_context.pop();
            self.usednamespaces_sets.pop();
            self.used_def_namespaces_sets.pop();
            self.importednames_sets.pop();
            self.default_namespaces.pop();
            self.public_namespaces.pop();
        }
        None
    }

    fn variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = get!(node, as_variable_definition.cx).unwrap_or_else(|| cx.clone());
        let mut val = None;
        let pkgdef = get!(node, as_variable_definition.def.pkgdef);
        if let Some(p) = &pkgdef {
            if Self::scope_count(&cx) == 1 {
                self.push_package_namespaces(p, true);
            }
        }

        let attrs = get!(node, as_variable_definition.def.attrs);
        let (mut is_static, mut is_final, mut is_dynamic, mut is_override) = (false, false, false, false);
        if let Some(a) = &attrs {
            let b = a.borrow();
            let a = b.as_attribute_list().expect(CCE);
            is_static = a.has_static;
            is_final = a.has_final;
            is_dynamic = a.has_dynamic;
            is_override = a.has_override;
        }

        let class_builder = self.class_builder_on_scope_chain(&cx);
        if class_builder.as_ref().map_or(false, |b| b.class_data().is_interface) {
            c(&cx).error(pos(node), kError_VarInInterface);
        }
        if is_dynamic {
            c(&cx).error(pos(node), kError_InvalidDynamic);
        }
        if is_final {
            c(&cx).error(pos(node), kError_InvalidFinalUsage);
        }
        if is_override {
            c(&cx).error(pos(node), kError_InvalidOverrideUsage);
        }

        let list = get!(node, as_variable_definition.list);
        if is_static {
            if class_builder.is_some() {
                val = self.eval_opt(&cx, &list);
            } else {
                c(&cx).error(pos_of(&attrs), kError_InvalidStatic);
            }
        } else {
            val = self.eval_opt(&cx, &list);
        }

        if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
            self.pop_package_namespaces(true);
        }
        val
    }

    fn function_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = nn(get!(node, function_definition.cx));
        let pkgdef = get!(node, function_definition.def.pkgdef);
        if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
            self.push_package_namespaces(pkgdef.as_ref().unwrap(), true);
            Self::set_global_in_package(&cx, true);
        }

        let mut is_static = false;
        let mut is_intrinsic = false;
        let mut is_native = false;
        let mut is_ctor = false;
        let mut is_final = false;
        let mut is_override = false;
        let is_prototype = get!(node, function_definition.is_prototype);
        let mut is_dynamic = false;
        let namespaces = Namespaces::new();
        let mut namespace_ids: Vec<String> = Vec::with_capacity(1);

        let attrs = get!(node, function_definition.def.attrs);
        if let Some(a) = &attrs {
            let ap = pos(a);
            let (hv, hf, hs, ho, hd, hi, hn) = {
                let b = a.borrow();
                let a = b.as_attribute_list().expect(CCE);
                (a.has_virtual, a.has_final, a.has_static, a.has_override, a.has_dynamic, a.has_intrinsic, a.has_native)
            };
            if hv && hf {
                c(&cx).error(ap, kError_FuncIsVirtualAndFinal);
            }
            if hs && hv {
                c(&cx).error(ap, kError_FuncIsStaticAndVirtual);
            }
            if hs && ho {
                c(&cx).error(ap, kError_FuncIsStaticAndOverride);
            }
            if hs && hd {
                c(&cx).error(ap, kError_InvalidDynamic);
            }
            is_static = hs;
            is_intrinsic = hi;
            is_native = hn;
            is_dynamic = hd;
            is_final = if is_static { true } else { hf };
            is_override = ho;
        }

        self.compute_namespaces(&cx, &attrs, &namespaces, &mut namespace_ids);
        if pkgdef.is_none() && Self::scope_count(&cx) == 1 {
            if let Some(a) = &attrs {
                if a.borrow().has_attribute(PUBLIC) {
                    c(&cx).error(pos(a), kError_InvalidPublic);
                }
            }
        }

        let fname = nn(get!(node, function_definition.name));
        let name_kind = get!(fname, as_function_name.kind);
        let name_identifier = nn(get!(fname, as_function_name.identifier));
        let fexpr = get!(node, function_definition.fexpr);
        {
            let nf = Context::get_node_factory(&cx);
            let (iname, ipos) = {
                let b = name_identifier.borrow();
                (b.identifier().expect(CCE).name.clone(), b.pos())
            };
            let qid = nf.borrow_mut().qualified_identifier_pos(attrs.clone(), Some(&iname), ipos);
            let assign = nf.borrow_mut().assignment_expression(Some(qid), CONST_TOKEN, fexpr.clone());
            let init = nf.borrow_mut().expression_statement(Some(assign));
            init.borrow_mut().as_expression_statement_mut().unwrap().set_is_var_statement(true);
            set!(node, function_definition_mut.init = Some(init));
        }

        let is_first_time = get!(node, function_definition.ref_).is_none();
        let val = self.eval(&cx, &name_identifier);
        set!(node, function_definition_mut.ref_ = rv(val));
        let r = nn(get!(node, function_definition.ref_));

        let fexpr_node = nn(fexpr.clone());
        set!(fexpr_node, as_function_common_mut.namespace_ids = Some(namespace_ids.clone()));

        let obj = nn(self.get_variable_definition_scope(&cx));
        let bui = builder_of(&obj);

        let region = c(&cx).debug_name(&self.back_region(), &r.name(), Some(&namespace_ids), name_kind);
        self.region_name_stack.push(region);

        let cname = nn(self.fun_name_stack.last().cloned());
        if cname == r.name() {
            if bui.is_instance_builder() {
                is_ctor = true;
                r.set_name("$construct");
                is_final = true;
                namespaces.push_back(c(&cx).public_namespace());
            } else {
                c(&cx).error(pos(node), kError_ConstructorsMustBeInstanceMethods);
            }
            if name_kind == SET_TOKEN || name_kind == GET_TOKEN {
                c(&cx).error(pos(node), kError_ConstructorCannnotBeGetterSetter);
            }
            if let Some(a) = &attrs {
                let ans = get!(a, as_attribute_list.namespaces).to_vec();
                let public = c(&cx).public_namespace();
                for ns in ans {
                    if !ns.ptr_eq(&public) {
                        c(&cx).error(pos(node), kError_ConstructorMustBePublic);
                        break;
                    }
                }
            }
        }

        if bui.is_activation_builder() && (name_kind == SET_TOKEN || name_kind == GET_TOKEN) {
            c(&cx).error(pos(node), kError_InvalidNestedAccessor);
        }

        let mut is_interface_method = false;
        let mut is_instance_method = false;
        if bui.is_instance_builder() {
            is_instance_method = true;
            is_interface_method = obj.type_().map_or(false, |t| t.is_interface());
        } else if bui.is_class_builder() && bui.class_data().is_interface {
            is_interface_method = true;
        }
        let user_body = fexpr_node.borrow().as_function_common().expect(CCE).is_user_defined_body();
        if is_interface_method || is_native {
            if is_interface_method && is_native {
                c(&cx).error(pos(node), kError_InvalidInterfaceNative);
            }
            if user_body {
                c(&cx).error(pos(node), if is_interface_method { kError_InterfaceMethodWithBody } else { kError_NativeMethodWithBody });
            }
        } else if !user_body && !is_ctor && !is_native && !is_dynamic {
            c(&cx).error(pos(node), kError_FunctionWithoutBody);
        }

        if let Some(a) = &attrs {
            let (hf, hp, hpr, hi, hpu) = {
                let b = a.borrow();
                let a = b.as_attribute_list().unwrap();
                (a.has_final, a.has_private, a.has_protected, a.has_internal, a.has_public)
            };
            if hf && (!is_instance_method || is_interface_method) {
                c(&cx).error(pos(node), kError_InvalidFinalUsage);
            }
            if is_interface_method && (hp || hpr || hi || hpu) && r.name() != "$construct" {
                c(&cx).error(pos(node), kError_BadAccessInterfaceMember);
            }
        }

        if is_static && is_interface_method {
            c(&cx).error(pos(node), kError_InvalidStatic);
        }

        if is_interface_method {
            if attrs.as_ref().map_or(false, |a| a.borrow().as_attribute_list().unwrap().get_user_namespace().is_some()) {
                c(&cx).error(pos(node), kError_InterfaceNamespaceAttribute);
            }
        } else if self.interface_methods.as_ref().map_or(false, |m| m.size() > 0) && namespaces.contains(&c(&cx).public_namespace()) {
            let ty = if name_kind == SET_TOKEN { Names::SET_NAMES } else { Names::GET_NAMES };
            let q = self.interface_methods.as_ref().unwrap().get_qualifiers(&r.name(), ty);
            if let Some(q) = q {
                for ns in q.key_set() {
                    namespace_ids.push(ns.name());
                    namespaces.push_back(ns);
                }
            }
            set!(fexpr_node, as_function_common_mut.namespace_ids = Some(namespace_ids.clone()));
        }

        let mut slot_id = -1;
        let kind = if name_kind == SET_TOKEN { SET_TOKEN } else { GET_TOKEN };
        let has_user_ns = attrs.as_ref().map_or(false, |a| a.borrow().as_attribute_list().unwrap().has_user_namespace());
        let open_definition_namespaces = if has_user_ns { namespaces.clone() } else { self.back_used_def() };

        let ref_name = r.name();
        let has_namespaces = obj.has_names(&c(&cx), kind, &ref_name, &open_definition_namespaces);

        if let Some(hn) = has_namespaces {
            if bui.is_instance_builder() {
                slot_id = obj.get_slot_index(&c(&cx), kind, &ref_name, Some(&hn.back()));
                let implied_id = obj.get_implicit_index(&c(&cx), slot_id, EMPTY_TOKEN);
                if implied_id >= 0 {
                    if !is_ctor && !self.namespaces_contains(&namespaces, &hn) {
                        if !(namespaces.size() == 1 && hn.size() == 1 && namespaces.at(0).is_protected() && hn.at(0).is_protected()) {
                            c(&cx).error(pos(node), kError_IncompatibleOverride);
                        }
                    }
                    let overridden_kind = if slot_id == implied_id { kind } else { EMPTY_TOKEN };
                    if overridden_kind != name_kind && !is_ctor {
                        c(&cx).error(pos(node), kError_IncompatibleOverride);
                    }

                    let slot = nn(obj.get_slot(&c(&cx), implied_id));
                    is_dynamic = slot.is_intrinsic();
                    let method_id = slot.get_method_id();
                    if slot.is_final() && !is_ctor {
                        if name_kind == SET_TOKEN || name_kind == GET_TOKEN {
                            c(&cx).error(pos(node), kError_OverrideFinalAccessor);
                        } else {
                            c(&cx).error(pos(node), kError_FinalMethodRedefinition);
                        }
                    }
                    if slot.declared_by().map_or(false, |d| d.ptr_eq(&obj)) {
                        c(&cx).error(pos(node), kError_DuplicateFunction);
                    } else if is_prototype || is_dynamic {
                        is_override = true;
                    } else if !is_override && !is_ctor {
                        c(&cx).error(pos(node), kError_OverrideOfFuncNotMarkedForOverride);
                    }

                    let cxb = c(&cx);
                    let no_type = cxb.no_type();
                    slot_id = if name_kind == GET_TOKEN {
                        bui.explicit_get_nss(&cxb, &obj, &ref_name, &namespaces, Some(&no_type), is_final, is_override, -1, method_id, -1)
                    } else if name_kind == SET_TOKEN {
                        bui.explicit_set_nss(&cxb, &obj, &ref_name, &namespaces, Some(&no_type), is_final, is_override, -1, method_id, -1)
                    } else {
                        bui.explicit_call_nss(&cxb, &obj, &ref_name, &namespaces, Some(&no_type), is_final, is_override, -1, method_id, -1)
                    };

                    if !is_ctor {
                        let overridden_slot = slot;
                        let override_slot = nn(obj.get_slot(&cxb, obj.get_implicit_index(&cxb, slot_id, EMPTY_TOKEN)));
                        override_slot.set_overridden_slot(Some(overridden_slot));
                    }
                } else {
                    let slot = nn(obj.get_slot(&c(&cx), slot_id));
                    if slot.get_method_id() <= 0 {
                        c(&cx).error(pos(node), kError_OverrideFinalAccessor);
                    }
                }
            } else if bui.is_class_builder() {
                c(&cx).error(pos(node), kError_DuplicateFunction);
            } else if bui.is_global_builder() && (pkgdef.is_some() || !namespaces.at(0).ptr_eq(&hn.at(0))) && is_first_time {
                c(&cx).error(pos(node), kError_DuplicateFunction);
            } else if c(&cx).use_static_semantics() && is_first_time {
                c(&cx).error(pos(node), kError_DuplicateFunction);
            } else {
                slot_id = obj.get_slot_index(&c(&cx), GET_TOKEN, &ref_name, Some(&hn.at(0)));
            }
        } else {
            if is_override && !is_ctor {
                if is_interface_method {
                    c(&cx).error(pos(node), kError_InvalidOverrideUsage);
                } else {
                    let n = namespaces.at(0);
                    if !n.is_unresolved_namespace() || n.with_unresolved(|u| u.resolved) {
                        c(&cx).error(pos(node), kError_OverrideNotFound);
                    } else {
                        let un_pos = n.with_unresolved(|u| nn(u.node.clone())).borrow().pos();
                        c(&cx).error(un_pos, kError_Unknown_Namespace);
                    }
                }
            }

            let cxb = c(&cx);
            let no_type = cxb.no_type();
            if name_kind == GET_TOKEN {
                let method_id = bui.method(&cxb, &obj, &format!("{}$get", ref_name), &namespaces, is_intrinsic);
                slot_id = bui.explicit_get_nss(&cxb, &obj, &ref_name, &namespaces, Some(&no_type), is_final, is_override, -1, method_id, -1);
            } else if name_kind == SET_TOKEN {
                let method_id = bui.method(&cxb, &obj, &format!("{}$set", ref_name), &namespaces, is_intrinsic);
                slot_id = bui.explicit_set_nss(&cxb, &obj, &ref_name, &namespaces, Some(&no_type), is_final, is_override, -1, method_id, -1);
            } else {
                let method_id = bui.method(&cxb, &obj, &ref_name, &namespaces, is_intrinsic);
                slot_id = bui.explicit_call_nss(&cxb, &obj, &ref_name, &namespaces, Some(&no_type), is_final, is_override, -1, method_id, -1);
            }
        }

        if is_intrinsic {
        } else if slot_id >= 0 {
            {
                let mut b = fexpr_node.borrow_mut();
                let f = b.as_function_common_mut().unwrap();
                f.set_native(is_native);
                f.kind = name_kind;
            }
            let slot = nn(obj.get_slot(&c(&cx), slot_id));
            let val = self.eval(&cx, &fexpr_node);
            let val = val.and_then(|v| v.get_value(&c(&cx)));
            slot.set_object_value(val.and_then(|v| v.as_object_value()));

            if let Some(fun_obj) = slot.get_object_value() {
                let internal_name = get!(fexpr_node, as_function_common.internal_name);
                if is_ctor {
                    if bui.is_instance_builder() {
                        let mut d = bui.instance_data_mut();
                        d.has_ctor = true;
                        d.ctor_name = Some(internal_name.clone());
                    } else {
                        c(&cx).error(pos(node), kError_ConstructorsMustBeInstanceMethods);
                    }
                }
                let cxb = c(&cx);
                slot_id = obj.get_implicit_index(&cxb, slot_id, EMPTY_TOKEN);
                nn(obj.get_slot(&cxb, slot_id)).set_method_name(&internal_name);
                nn(obj.get_slot(&cxb, slot_id)).set_intrinsic(is_dynamic);
                fun_obj.set_name(&internal_name);
            }

            set!(fexpr_node, as_function_common_mut.debug_name = self.back_region());
        }
        self.region_name_stack.pop();

        if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
            self.pop_package_namespaces(true);
            Self::set_global_in_package(&cx, false);
        }

        if get!(node, function_definition.needs_init) {
            if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
                self.push_package_namespaces(pkgdef.as_ref().unwrap(), true);
                Self::set_global_in_package(&cx, true);
            }
            self.eval_opt(&cx, &get!(node, function_definition.init));
            if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
                self.pop_package_namespaces(true);
                Self::set_global_in_package(&cx, false);
            }
            return None;
        }
        None
    }

    fn rest_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let obj = Self::scope(cx);
        let bui = builder_of(&obj);
        let v = self.eval_opt(cx, &get!(node, parameter.identifier));
        set!(node, parameter_mut.ref_ = rv(v));
        if let Some(t) = get!(node, parameter.type_) {
            let v = self.eval(cx, &t);
            let tr = rv(v);
            if let Some(tr) = &tr {
                tr.set_type_annotation(true);
            }
            set!(node, parameter_mut.typeref = tr);
        }
        let cxb = c(cx);
        let namespaces = Namespaces::new();
        namespaces.push_back(cxb.public_namespace());
        let name = nn(get!(node, parameter.ref_)).name();
        let var_id = bui.variable(&cxb, &obj);
        bui.explicit_var_nss(&cxb, &obj, &name, &namespaces, Some(&cxb.array_type()), -1, -1, var_id);
        get!(node, parameter.ref_).map(Value::Reference)
    }

    fn parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let obj = Self::scope(cx);
        let bui = builder_of(&obj);
        let v = self.eval_opt(cx, &get!(node, parameter.identifier));
        set!(node, parameter_mut.ref_ = rv(v));
        if let Some(init) = get!(node, parameter.init) {
            self.eval(cx, &init);
        }
        if let Some(t) = get!(node, parameter.type_) {
            let v = self.eval(cx, &t);
            let tr = rv(v);
            match &tr {
                Some(tr) => tr.set_type_annotation(true),
                None => c(cx).error(pos(&t), kError_UnknownType),
            }
            set!(node, parameter_mut.typeref = tr);
        }
        let namespaces = Namespaces::new();
        let mut default_ns = self.back_default();
        if default_ns.is_interface() {
            default_ns = c(cx).public_namespace();
        }
        namespaces.push_back(default_ns);
        let cxb = c(cx);
        let name = nn(get!(node, parameter.ref_)).name();
        let var_id = bui.variable(&cxb, &obj);
        let slot_id = bui.explicit_var_nss(&cxb, &obj, &name, &namespaces, Some(&cxb.no_type()), -1, -1, var_id);
        let slot = nn(obj.get_slot(&cxb, slot_id));
        slot.set_type_ref(get!(node, parameter.typeref));
        get!(node, parameter.ref_).map(Value::Reference)
    }
}

impl FlowAnalyzer {
    fn class_definition(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = nn(get!(node, class_definition.cx));
        let pkgdef = get!(node, class_definition.def.pkgdef);
        if let Some(p) = &pkgdef {
            if Self::scope_count(&cx) == 1 {
                self.push_package_namespaces(p, true);
            }
        }

        if get!(node, class_definition.cframe).is_none() {
            ns_add_all(&get!(node, class_definition.used_namespaces), &self.back_used());
            let imported = self.back_imported().snapshot();
            node.borrow_mut().class_definition_mut().unwrap().imported_names.put_all(&imported);
            set!(node, class_definition_mut.public_namespace = Some(c(&cx).public_namespace()));
            set!(node, class_definition_mut.default_namespace = self.default_namespaces.last().cloned());

            if self.clsdefs_sets.len() == 1 {
                let last = self.clsdefs_sets.last().cloned().unwrap();
                if !list_contains(&last, node) {
                    if !self.package_name.is_empty() {
                        set!(node, class_definition_mut.package_name = Some(self.package_name.clone()));
                    }
                    last.borrow_mut().push(node.clone());
                }
            }

            let mut is_intrinsic = false;
            let attrs = get!(node, class_definition.def.attrs);
            if let Some(a) = &attrs {
                let (hi, hn) = {
                    let b = a.borrow();
                    let a = b.as_attribute_list().expect(CCE);
                    (a.has_intrinsic, a.has_native)
                };
                is_intrinsic = hi;
                if hn {
                    c(&cx).error(pos(node), kError_InvalidNative);
                }
            }

            if Self::scope_count(&cx) > 1 {
                if node.borrow().is_interface() {
                    c(&cx).error(pos(node), kError_InvalidInterfaceNesting);
                } else {
                    c(&cx).error(pos(node), kError_InvalidClassNesting);
                }
            }

            if get!(node, class_definition.ref_).is_none() {
                let ownerobj = Self::scope(&cx);
                let ownerbui = builder_of(&ownerobj);

                let mut region_name = self.back_region();
                if !region_name.is_empty() {
                    region_name.push('/');
                }

                let mut namespace_ids: Vec<String> = Vec::new();
                let node_namespaces = get!(node, class_definition.namespaces);
                self.compute_namespaces(&cx, &attrs, &node_namespaces, &mut namespace_ids);
                if pkgdef.is_none() && Self::scope_count(&cx) == 1 {
                    if let Some(a) = &attrs {
                        if a.borrow().has_attribute(PUBLIC) {
                            c(&cx).error(pos(a), kError_InvalidPublic);
                        }
                    }
                }

                let name_node = get!(node, class_definition.name);
                let val = self.eval_opt(&cx, &name_node);
                set!(node, class_definition_mut.ref_ = rv(val));
                let r = nn(get!(node, class_definition.ref_));

                let cxb = c(&cx);
                let fullname = cxb.compute_qualified_name(&region_name, &r.name(), &r.get_immutable_namespaces().back(), EMPTY_TOKEN);
                let fstr = fullname.to_string();
                let private_ns = cxb.get_namespace_kind(&fstr, NS_PRIVATE);
                let protected_ns = cxb.get_namespace_kind(&fstr, NS_PROTECTED);
                let static_protected_ns = cxb.get_namespace_kind(&fstr, NS_STATIC_PROTECTED);
                drop(cxb);
                set!(node, class_definition_mut.private_namespace = Some(private_ns));
                set!(node, class_definition_mut.protected_namespace = Some(protected_ns.clone()));
                set!(node, class_definition_mut.static_protected_namespace = Some(static_protected_ns.clone()));

                let (cframe, iframe) = if c(&cx).is_builtin(&fstr) {
                    let cframe = nn(c(&cx).builtin(&fstr));
                    let iframe = cframe.prototype();
                    (cframe, iframe)
                } else {
                    let cxb = c(&cx);
                    let cframe = TypeValue::define_type_value(
                        &cxb,
                        new_class_builder(fullname.clone(), Some(protected_ns), Some(static_protected_ns)),
                        &fullname,
                        TYPE_object,
                    );
                    if !get!(node, class_definition.is_default_nullable) {
                        cframe.set_is_nullable(false);
                    }
                    cframe.set_type(Some(cxb.type_type().get_default_type_info()));
                    let iframe = match cframe.prototype() {
                        Some(p) => {
                            p.clear_instance(&cxb, new_instance_builder(fullname.clone()), Some(&cframe), "", true);
                            p
                        }
                        None => {
                            let iframe = ObjectValue::with_builder(&cxb, new_instance_builder(fullname.clone()), Some(&cframe));
                            cframe.set_prototype(Some(iframe.clone()));
                            drop(cxb);
                            set!(node, class_definition_mut.owns_cframe = true);
                            iframe
                        }
                    };
                    (cframe, Some(iframe))
                };
                set!(node, class_definition_mut.cframe = Some(cframe.clone()));
                set!(node, class_definition_mut.iframe = iframe.clone());

                if matches!(node.borrow().kind, Kind::InterfaceDefinition(_)) {
                    set!(node, class_definition_mut.default_namespace = Some(cframe.as_object().clone()));
                }

                if let Some(a) = &attrs {
                    let (hd, hf) = {
                        let b = a.borrow();
                        let a = b.as_attribute_list().unwrap();
                        (a.has_dynamic, a.has_final)
                    };
                    let cb = builder_of(&cframe);
                    let ib = builder_of(&nn(iframe.clone()));
                    ib.set_is_dynamic(hd);
                    cb.set_is_dynamic(hd);
                    ib.set_is_final(hf);
                    cb.set_is_final(hf);
                    ib.set_is_intrinsic(is_intrinsic);
                    cb.set_is_intrinsic(is_intrinsic);
                }

                let has_user_ns = attrs.as_ref().map_or(false, |a| a.borrow().as_attribute_list().unwrap().has_user_namespace());
                let open_definition_namespaces = if has_user_ns { node_namespaces.clone() } else { self.back_used_def() };
                let has_namespaces = ownerobj.has_names(&c(&cx), GET_TOKEN, &r.name(), &open_definition_namespaces);
                match has_namespaces {
                    None => {
                        let cxb = c(&cx);
                        let mut var_id = -1;
                        if attrs.as_ref().map_or(true, |a| !a.borrow().as_attribute_list().unwrap().has_intrinsic) {
                            var_id = ownerbui.variable(&cxb, &ownerobj);
                        }
                        let slot_id = ownerbui.explicit_var_nss(&cxb, &ownerobj, &r.name(), &node_namespaces, Some(&cxb.type_type()), -1, -1, var_id);
                        let slot = nn(ownerobj.get_slot(&cxb, slot_id));
                        slot.set_value(Some(Value::Object(cframe.as_object().clone())));
                        slot.set_const(true);
                        ownerbui.implicit_call(&cxb, &ownerobj, slot_id, Some(&cframe), CALL_Method, -1, -1);
                        ownerbui.implicit_construct(&cxb, &ownerobj, slot_id, Some(&cframe), CALL_Method, -1, -1);
                    }
                    Some(_) => {
                        let name_pos = pos_of(&name_node);
                        if node.borrow().is_interface() {
                            c(&cx).error1(name_pos, kError_DuplicateInterfaceDefinition, &r.name());
                        } else {
                            c(&cx).error1(name_pos, kError_DuplicateClassDefinition, &r.name());
                            self.found_circular_or_duplicate_class_definition = true;
                        }
                    }
                }
            }

            let (used, used_def, private_ns, public_ns, default_ns, protected_ns, static_protected_ns) = {
                let b = node.borrow();
                let cd = b.class_definition().unwrap();
                (
                    cd.used_namespaces.clone(),
                    cd.used_def_namespaces.clone(),
                    nn(cd.private_namespace.clone()),
                    nn(cd.public_namespace.clone()),
                    nn(cd.default_namespace.clone()),
                    nn(cd.protected_namespace.clone()),
                    nn(cd.static_protected_namespace.clone()),
                )
            };
            used.push_back(private_ns.clone());
            used.push_back(protected_ns.clone());
            used.push_back(static_protected_ns.clone());

            used_def.push_back(private_ns);
            used_def.push_back(public_ns);
            used_def.push_back(default_ns);
            used_def.push_back(protected_ns);
            used_def.push_back(static_protected_ns);

            set!(node, class_definition_mut.state = crate::asc::parser::node::ClassDefinitionNode::INHERIT);

            {
                let nf = Context::get_node_factory(&cx);
                let attrs = get!(node, class_definition.def.attrs);
                let name_node = nn(get!(node, class_definition.name));
                let (iname, ipos) = {
                    let b = name_node.borrow();
                    (b.identifier().expect(CCE).name.clone(), b.pos())
                };
                let qid = nf.borrow_mut().qualified_identifier_pos(attrs, Some(&iname), ipos);
                let assign = nf.borrow_mut().assignment_expression(Some(qid), CONST_TOKEN, Some(node.clone()));
                let init = nf.borrow_mut().expression_statement(Some(assign));
                init.borrow_mut().as_expression_statement_mut().unwrap().set_is_var_statement(true);
                set!(node, class_definition_mut.init = Some(init));
            }

            self.clsdefs_sets.push(new_node_list());
            let cframe = nn(get!(node, class_definition.cframe));
            c(&cx).push_static_class_scopes(&cframe);
            let mut namespace_ids: Vec<String> = Vec::new();
            let nsl = get!(node, class_definition.namespaces);
            if nsl.size() != 0 {
                namespace_ids.push(nsl.back().name());
            } else {
                namespace_ids.push("error".to_string());
            }
            let r = nn(get!(node, class_definition.ref_));
            let region = c(&cx).debug_name(&self.back_region(), &r.name(), Some(&namespace_ids), EMPTY_TOKEN);
            self.region_name_stack.push(region);

            self.usednamespaces_sets.push(used);
            self.used_def_namespaces_sets.push(used_def);

            let clsdefs = nn(get!(node, class_definition.clsdefs));
            let mut i = 0;
            loop {
                let n = {
                    let l = clsdefs.borrow();
                    if i >= l.len() {
                        break;
                    }
                    l[i].clone()
                };
                i += 1;
                if let Some(a) = get!(n, class_definition.def.attrs) {
                    self.eval(&cx, &a);
                }
                self.eval(&cx, &n);
            }
            c(&cx).pop_static_class_scopes(Some(&cframe));
            self.region_name_stack.pop();
            self.usednamespaces_sets.pop();
            self.used_def_namespaces_sets.pop();
            self.clsdefs_sets.pop();
        } else if self.doing_class() || self.doing_method() {
        } else if self.resolve_inheritance {
            let cframe = nn(get!(node, class_definition.cframe));
            let iframe = nn(get!(node, class_definition.iframe));
            if get!(node, class_definition.baseclass).is_none() && !cframe.ptr_eq(&c(&cx).object_type()) {
                let nf = Context::get_node_factory(&cx);
                let id = nf.borrow_mut().identifier("Object");
                let ge = nf.borrow_mut().get_expression(Some(id));
                let me = nf.borrow_mut().member_expression(None, Some(ge));
                set!(node, class_definition_mut.baseclass = Some(me));
            }

            if get!(node, class_definition.baseref).is_none() {
                if let Some(bc) = get!(node, class_definition.baseclass) {
                    self.flush_unresolved(Unresolved::Rt);
                    let val2 = self.eval(&cx, &bc);
                    self.flush_unresolved(Unresolved::Fa);
                    let br = rv(val2);
                    set!(node, class_definition_mut.baseref = br.clone());
                    if br.is_none() {
                        c(&cx).error(pos(&bc), kError_InvalidBaseTypeExpression);
                    }
                }
            }

            if let Some(br) = get!(node, class_definition.baseref) {
                let val = br.get_value(&c(&cx));
                if let Some(t) = val.and_then(|v| v.as_type_value()) {
                    let tb = builder_of(&t);
                    if tb.is_final() {
                    } else if tb.is_class_builder() && tb.class_data().is_interface {
                    } else {
                        self.inherit_class_slots(&cframe, &iframe, &t, &cx);
                    }
                }
            }

            if let Some(interfaces) = get!(node, class_definition.interfaces) {
                self.flush_unresolved(Unresolved::Rt);
                self.eval(&cx, &interfaces);
                self.flush_unresolved(Unresolved::Fa);
            }

            set!(node, class_definition_mut.state = crate::asc::parser::node::ClassDefinitionNode::MAIN);
            self.eval_list(&cx, &get!(node, class_definition.clsdefs));
        } else if get!(node, class_definition.needs_init) {
            set!(node, class_definition_mut.needs_init = false);
            self.eval_opt(&cx, &get!(node, class_definition.init));
            set!(node, class_definition_mut.needs_init = true);
        } else {
            self.class_body(&cx, node);
        }

        if pkgdef.is_some() && Self::scope_count(&cx) == 1 {
            self.pop_package_namespaces(true);
        }

        set!(node, class_definition_mut.needs_init = true);
        get!(node, class_definition.ref_).map(Value::Reference)
    }

    /// The last branch of the `ClassDefinitionNode` evaluator: the class's static and instance bodies.
    fn class_body(&mut self, cx: &ContextRef, node: &NodeRef) {
        self.this_contexts.push(error_this);
        self.strict_context.push(true);

        let (used, used_def) = (get!(node, class_definition.used_namespaces), get!(node, class_definition.used_def_namespaces));
        self.usednamespaces_sets.push(used.clone());
        self.used_def_namespaces_sets.push(used_def.clone());
        self.importednames_sets.push(ImportedNames::Node(node.clone()));

        let cframe = nn(get!(node, class_definition.cframe));
        let iframe = nn(get!(node, class_definition.iframe));
        let r = nn(get!(node, class_definition.ref_));

        if let Some(baseref) = get!(node, class_definition.baseref) {
            let val = baseref.get_value(&c(cx));
            let t = val.and_then(|v| v.as_type_value());
            let bc_pos = pos_of(&get!(node, class_definition.baseclass));
            match t {
                None => c(cx).error1(bc_pos, kError_UnknownBaseClass, &baseref.name()),
                Some(t) => {
                    let tb = builder_of(&t);
                    if tb.is_final() {
                        c(cx).error(bc_pos, kError_BaseClassIsFinal);
                    } else if tb.is_class_builder() && tb.class_data().is_interface {
                        c(cx).error(bc_pos, kError_CannotExtendInterface);
                    } else {
                        self.inherit_class_slots(&cframe, &iframe, &t, cx);

                        let node_cx = nn(get!(node, class_definition.cx));
                        let base_slot = nn(baseref.get_slot_kind(&c(&node_cx), baseref.get_kind()));
                        if base_slot.is_imported() && !t.ptr_eq(&c(cx).no_type()) {
                            builder_of(&iframe).instance_data_mut().can_early_bind = false;
                        }

                        let mut ty = Some(t);
                        while let Some(t) = ty.clone() {
                            if t.ptr_eq(&cframe) || !t.resolved() {
                                break;
                            }
                            let sp = builder_of(&t).class_data().static_protected_namespace.clone();
                            if let Some(sp) = sp {
                                used.push_back(sp.clone());
                                used_def.push_back(sp);
                            }
                            ty = t.baseclass();
                        }
                    }
                }
            }
        }

        if let Some(interfaces) = get!(node, class_definition.interfaces) {
            let (values, items) = {
                let b = interfaces.borrow();
                let l = b.list().expect(CCE);
                (l.values.clone(), l.items.clone())
            };
            let ibui = builder_of(&iframe);
            let mut seen_interfs: Vec<TypeValue> = Vec::new();
            for (i, v) in values.iter().enumerate() {
                let item_pos = || pos_of(&items[i]);
                match v {
                    Some(Value::Reference(rf)) => {
                        let v2 = rf.get_value(&c(cx));
                        let t = v2.and_then(|v| v.as_type_value());
                        match t {
                            None => c(cx).error1(item_pos(), kError_UnknownInterface, &rf.name()),
                            Some(t) => {
                                let tb = builder_of(&t);
                                if tb.is_class_builder() {
                                    if !tb.class_data().is_interface {
                                        c(cx).error1(item_pos(), kError_CannotExtendClass, &rf.name());
                                    } else {
                                        if seen_interfs.iter().any(|s| s.ptr_eq(&t)) {
                                            c(cx).error2(item_pos(), kError_DuplicateImplements, &r.name(), &rf.name());
                                        } else {
                                            seen_interfs.push(t.clone());
                                        }
                                        ibui.instance_data_mut().interface_refs.push(rf.clone());
                                        if matches!(node.borrow().kind, Kind::InterfaceDefinition(_)) {
                                            self.inherit_class_slots(&cframe, &iframe, &t, cx);
                                        }
                                    }
                                } else {
                                    c(cx).error1(item_pos(), kError_UnknownInterface, &rf.name());
                                }
                            }
                        }
                    }
                    _ => c(cx).error(item_pos(), kError_InvalidInterfaceTypeExpression),
                }
            }
        }

        let last_interface_methods = self.interface_methods.take();

        self.scan_interface_methods(cx, node);
        self.process_interface_public_methods(cx, &iframe);

        self.start_class(&r.name());

        let mut namespace_ids: Vec<String> = Vec::new();
        let nsl = get!(node, class_definition.namespaces);
        if nsl.size() != 0 {
            namespace_ids.push(nsl.back().name());
        } else {
            namespace_ids.push("error".to_string());
        }
        let region = c(cx).debug_name(&self.back_region(), &r.name(), Some(&namespace_ids), EMPTY_TOKEN);
        self.region_name_stack.push(region);

        {
            let b = node.borrow();
            let cd = b.class_definition().unwrap();
            self.private_namespaces.push(nn(cd.private_namespace.clone()));
            self.default_namespaces.push(nn(cd.default_namespace.clone()));
            self.public_namespaces.push(nn(cd.public_namespace.clone()));
            self.protected_namespaces.push(nn(cd.protected_namespace.clone()));
            self.static_protected_namespaces.push(nn(cd.static_protected_namespace.clone()));
        }

        c(cx).push_static_class_scopes(&cframe);

        self.this_contexts.pop();
        self.this_contexts.push(cinit_this);

        self.fexprs_sets.push(new_node_list());
        self.staticfexprs_sets.push(new_node_list());
        self.instanceinits_sets.push(new_node_list());

        set!(node, class_definition_mut.fexprs = self.fexprs_sets.last().cloned());
        set!(node, class_definition_mut.instanceinits = self.instanceinits_sets.last().cloned());
        set!(node, class_definition_mut.staticfexprs = self.staticfexprs_sets.last().cloned());

        self.fun_name_stack.push(r.name());
        self.max_params_stack.push(0);
        self.max_locals_stack.push(get!(node, class_definition.var_count));
        self.max_temps_stack.push(get!(node, class_definition.temp_count));

        let name = self.back_fun_name();
        let (p, l) = (ilast(&self.max_params_stack), ilast(&self.max_locals_stack));
        self.start_method_counts(&name, p, l);

        if let Some(statements) = get!(node, class_definition.statements) {
            self.eval(cx, &statements);
            let tc = self.get_temp_count();
            set!(node, class_definition_mut.temp_count = tc);
            set!(node, class_definition_mut.var_count = cframe.var_count());
        } else {
            let name = self.back_fun_name();
            let (p, l) = (ilast(&self.max_params_stack), ilast(&self.max_locals_stack));
            self.start_method_counts(&name, p, l);
        }

        let tc = self.get_temp_count();
        set!(node, class_definition_mut.temp_count = tc);

        self.return_(TYPE_void);
        let name = self.back_fun_name();
        let depth = Self::scope_count(cx) as i32;
        self.finish_method(cx, &name, None, None, None, 0, depth, "", false, false, None);

        c(cx).push_scope(iframe.clone());

        self.this_contexts.pop();
        self.this_contexts.push(instance_this);

        {
            let instanceinits = nn(get!(node, class_definition.instanceinits));
            let mut i = 0;
            loop {
                let n = {
                    let l = instanceinits.borrow();
                    if i >= l.len() {
                        break;
                    }
                    l[i].clone()
                };
                i += 1;
                let es4 = c(cx).statics.borrow().es4_nullability;
                if es4 && !n.borrow().is_definition() {
                    iframe.set_init_only(true);
                }
                self.eval(cx, &n);
                if es4 && !n.borrow().is_definition() {
                    iframe.set_init_only(false);
                }
            }
        }

        let bui = iframe.builder().filter(|b| b.is_instance_builder());
        let bui = nn(bui);
        if !bui.is_intrinsic() && !bui.instance_data().has_ctor {
            let nf = Context::get_node_factory(cx);
            let id = nf.borrow_mut().identifier_pos(&r.name(), 0);
            let fname = nf.borrow_mut().function_name(EMPTY_TOKEN, Some(id));
            {
                let mut f = nf.borrow_mut();
                f.has_rest = false;
                f.has_arguments = false;
            }
            let fname_ident = get!(fname, as_function_name.identifier);
            let sig = nf.borrow_mut().function_signature_pos(None, None, 0);
            let fexpr = nf.borrow_mut().function_common_pos(cx, fname_ident, Some(sig), None, 0);
            let pid = nf.borrow_mut().identifier_intern_pos(PUBLIC, false, 0);
            let attrs = nf.borrow_mut().attribute_list(Some(pid), None);
            self.eval(cx, &attrs);
            let fdef = nf.borrow_mut().function_definition(cx, Some(attrs), Some(fname), Some(fexpr));
            let pkgdef = get!(node, class_definition.def.pkgdef);
            fdef.borrow_mut().definition_mut().unwrap().pkgdef = pkgdef;
            self.eval(cx, &fdef);
            let init = initializer_statement(&fdef, cx);
            self.eval(cx, &init);
            match get!(node, class_definition.statements) {
                None => {
                    let sl = nf.borrow_mut().statement_list(None, Some(init));
                    set!(node, class_definition_mut.statements = Some(sl));
                }
                Some(s) => s.borrow_mut().as_statement_list_mut().expect(CCE).items.insert(0, Some(init)),
            }
        }

        {
            self.this_contexts.push(error_this);
            c(cx).pop_scope();
            self.eval_list(cx, &get!(node, class_definition.staticfexprs));
            c(cx).push_scope(iframe.clone());
            self.this_contexts.pop();
        }

        self.fun_name_stack.pop();
        self.max_params_stack.pop();
        self.max_locals_stack.pop();
        self.max_temps_stack.pop();

        self.eval_list(cx, &get!(node, class_definition.fexprs));

        self.fexprs_sets.pop();

        self.private_namespaces.pop();
        self.default_namespaces.pop();
        self.public_namespaces.pop();
        self.protected_namespaces.pop();
        self.static_protected_namespaces.pop();
        self.usednamespaces_sets.pop();
        self.used_def_namespaces_sets.pop();
        self.importednames_sets.pop();

        let classname = nn(builder_of(&cframe).classname());
        let is_nullable = cframe.is_nullable();
        self.finish_class(cx, &classname, None, false, false, false, is_nullable);

        self.this_contexts.pop();

        c(cx).pop_scope();

        {
            let node_cx = nn(get!(node, class_definition.cx));
            let sorted = self.sort_class_definitions(&node_cx, get!(node, class_definition.clsdefs));
            set!(node, class_definition_mut.clsdefs = sorted);
            if !self.found_circular_or_duplicate_class_definition {
                self.eval_list(cx, &get!(node, class_definition.clsdefs));
            }
        }

        self.instanceinits_sets.pop();
        self.staticfexprs_sets.pop();

        c(cx).pop_static_class_scopes(Some(&cframe));

        let debug_name = self.back_region();
        set!(node, class_definition_mut.debug_name = Some(debug_name.clone()));
        let s = r.get_slot_kind(&c(cx), GET_TOKEN);
        if let Some(s) = s {
            s.set_debug_name(&debug_name);
            s.set_const(true);
        }

        self.region_name_stack.pop();
        self.strict_context.pop();

        self.interface_methods = last_interface_methods;
    }

    fn binary_class_def(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cframe = get!(node, class_definition.cframe);
        let iframe = get!(node, class_definition.iframe);
        let baseref = get!(node, class_definition.baseref);
        if self.resolve_inheritance {
            if let Some(br) = baseref {
                let val = br.get_value(&c(cx));
                match val {
                    None => self.add_unresolved(Unresolved::Fa, br),
                    Some(v) => {
                        if let Some(t) = v.as_type_value() {
                            let tb = builder_of(&t);
                            if tb.is_final() {
                            } else if tb.is_class_builder() && tb.class_data().is_interface {
                            } else {
                                self.inherit_class_slots(&nn(cframe), &nn(iframe), &t, cx);
                            }
                        }
                    }
                }
            }
        } else {
            let node_cx = nn(get!(node, class_definition.cx));
            let cframe = nn(cframe);
            if let Some(br) = &baseref {
                let base_type = br.get_value(&c(&node_cx)).map(|v| v.as_type_value().expect(CCE));
                if let Some(bt) = base_type {
                    self.inherit_class_slots(&cframe, &nn(cframe.prototype()), &bt, &node_cx);
                }
            }
            if let Some(interfaces) = get!(node, class_definition.interfaces) {
                let iframe = nn(iframe.clone());
                let (size, values) = {
                    let b = interfaces.borrow();
                    let l = b.list().expect(CCE);
                    (l.size(), l.values.clone())
                };
                for i in 0..size as usize {
                    let v = values.get(i).cloned().expect("IndexOutOfBoundsException");
                    if let Some(Value::Reference(rf)) = &v {
                        if rf.slot().is_none() {
                            c(cx).error1(pos(node), kError_UnknownType, &rf.to_multi_name());
                            continue;
                        }
                        builder_of(&iframe).instance_data_mut().interface_refs.push(rf.clone());
                        let v2 = rf.get_value(&c(&node_cx));
                        if let Some(t) = v2.and_then(|v| v.as_type_value()) {
                            if t.is_interface() && matches!(node.borrow().kind, Kind::BinaryInterfaceDefinition(_)) {
                                self.inherit_class_slots(&cframe, &iframe, &t, &node_cx);
                            }
                        }
                    }
                }
            }
            let r = nn(get!(node, class_definition.ref_));
            let s = r.get_slot_kind(&c(&node_cx), GET_TOKEN);
            if let Some(s) = s {
                s.set_debug_name(&get!(node, class_definition.debug_name).unwrap_or_default());
                s.set_const(true);
            }

            let last_interface_methods = self.interface_methods.take();
            self.scan_interface_methods(&node_cx, node);
            self.process_interface_public_methods(&node_cx, &nn(iframe));
            self.interface_methods = last_interface_methods;

            cframe.set_type(Some(c(&node_cx).type_type().get_default_type_info()));
        }
        None
    }

    fn interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let val = self.class_definition(cx, node);
        let r = rv(val);
        let slot = r.and_then(|r| r.get_slot(&c(cx)));
        let Some(slot) = slot else {
            c(cx).internal_error_at(pos(node), "internal error in FA::InterfaceDefinitionNode has no slot");
            return None;
        };
        slot.set_impl_node(Some(node.clone()));
        let cframe = nn(get!(node, class_definition.cframe));
        builder_of(&cframe).class_data_mut().is_interface = true;

        let attrs = get!(node, class_definition.def.attrs);
        if let Some(a) = &attrs {
            if get!(node, class_definition.state) == ProgramNode::Inheritance {
                let (hf, hd, hn, hp, hpr) = {
                    let b = a.borrow();
                    let a = b.as_attribute_list().expect(CCE);
                    (a.has_final, a.has_dynamic, a.has_native, a.has_private, a.has_protected)
                };
                if hf {
                    c(cx).error1(pos(node), kError_InvalidInterfaceAttribute, "final");
                }
                if hd {
                    c(cx).error1(pos(node), kError_InvalidInterfaceAttribute, "dynamic");
                }
                if hn {
                    c(cx).error1(pos(node), kError_InvalidInterfaceAttribute, "native");
                }
                if hp {
                    c(cx).error1(pos(node), kError_InvalidInterfaceAttribute, PRIVATE);
                }
                if hpr {
                    c(cx).error1(pos(node), kError_InvalidInterfaceAttribute, PROTECTED);
                }
            }
        }
        None
    }

    fn attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        {
            let b = node.borrow();
            let a = b.as_attribute_list().unwrap();
            if !a.namespace_ids.is_empty() || a.namespaces.size() != 0 {
                return None;
            }
        }

        let mut set_private = false;
        let mut set_protected = false;
        let mut set_public = false;
        let mut set_internal = false;
        let mut set_dynamic = false;
        let mut set_virtual = false;
        let mut set_final = false;
        let mut set_override = false;
        let mut set_static = false;
        let mut set_native = false;
        let mut set_prototype = false;

        macro_rules! attr {
            ($f:ident) => {
                node.borrow_mut().as_attribute_list_mut().unwrap().$f = true
            };
        }
        let in_class = |cx: &ContextRef| {
            let s = Self::scope(cx);
            is_class_builder(&s) || is_instance_builder(&s)
        };

        let size = node.borrow().as_attribute_list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().as_attribute_list().unwrap().items.get(i).cloned().flatten();
            let Some(n) = n else { continue };
            let np = pos(&n);
            if n.borrow().has_attribute(PRIVATE) {
                if set_private {
                    c(cx).error1(np, kError_DuplicateAttribute, PRIVATE);
                }
                set_private = true;
                attr!(has_private);
            } else if n.borrow().has_attribute(PROTECTED) {
                if set_protected {
                    c(cx).error1(np, kError_DuplicateAttribute, PROTECTED);
                }
                set_protected = true;
                attr!(has_protected);
            } else if n.borrow().has_attribute(PUBLIC) {
                if set_public {
                    c(cx).error1(np, kError_DuplicateAttribute, PUBLIC);
                }
                set_public = true;
                attr!(has_public);
            } else if n.borrow().has_attribute(INTERNAL) {
                if set_internal {
                    c(cx).error1(np, kError_DuplicateAttribute, INTERNAL);
                }
                set_internal = true;
                attr!(has_internal);
            } else {
                if n.borrow().has_attribute(PROTOTYPE) {
                    if set_prototype {
                        c(cx).error1(np, kError_DuplicateAttribute, PROTOTYPE);
                    }
                    set_prototype = true;
                    attr!(has_prototype);
                }

                self.flush_unresolved(Unresolved::Rt);
                let val1 = self.eval(cx, &n);
                self.flush_unresolved(Unresolved::Ns);

                match rv(val1) {
                    Some(r) => {
                        let val2 = r.get_value(&c(cx));
                        let obj = val2.and_then(|v| v.as_object_value());
                        match obj {
                            Some(obj) => {
                                if obj.ptr_eq(&ObjectValue::intrinsic_attribute()) {
                                    attr!(has_intrinsic);
                                    c(cx).error(np, kError_Unsupported_Intrinsic);
                                } else if obj.ptr_eq(&ObjectValue::static_attribute()) {
                                    if set_static {
                                        c(cx).error1(np, kError_DuplicateAttribute, STATIC);
                                    }
                                    set_static = true;
                                    attr!(has_static);
                                    if !in_class(cx) {
                                        c(cx).error(np, kError_InvalidStatic);
                                    }
                                } else if obj.ptr_eq(&ObjectValue::dynamic_attribute()) {
                                    if set_dynamic {
                                        c(cx).error1(np, kError_DuplicateAttribute, "dynamic");
                                    }
                                    set_dynamic = true;
                                    attr!(has_dynamic);
                                } else if obj.ptr_eq(&ObjectValue::virtual_attribute()) {
                                    if set_virtual {
                                        c(cx).error1(np, kError_DuplicateAttribute, "virtual");
                                    }
                                    set_virtual = true;
                                    attr!(has_virtual);
                                    if !in_class(cx) {
                                        c(cx).error(np, kError_InvalidVirtual);
                                    }
                                } else if obj.ptr_eq(&ObjectValue::final_attribute()) {
                                    if set_final {
                                        c(cx).error1(np, kError_DuplicateAttribute, "final");
                                    }
                                    set_final = true;
                                    attr!(has_final);
                                } else if obj.ptr_eq(&ObjectValue::override_attribute()) {
                                    if set_override {
                                        c(cx).error1(np, kError_DuplicateAttribute, "override");
                                    }
                                    set_override = true;
                                    attr!(has_override);
                                    if !in_class(cx) {
                                        c(cx).error(np, kError_InvalidOverride);
                                    }
                                } else if obj.ptr_eq(&ObjectValue::native_attribute()) {
                                    if set_native {
                                        c(cx).error1(np, kError_DuplicateAttribute, "native");
                                    }
                                    set_native = true;
                                    attr!(has_native);
                                } else {
                                    if *obj.name_ref() == *"false" {
                                        attr!(has_false);
                                    }
                                    if !in_class(cx) {
                                        c(cx).error(pos(node), kError_InvalidNamespace);
                                    } else {
                                        let ans = get!(node, as_attribute_list.namespaces);
                                        ans.push_back(obj);
                                        node.borrow_mut().as_attribute_list_mut().unwrap().namespace_ids.push(r.name());
                                    }
                                }
                            }
                            None => {
                                if !in_class(cx) {
                                    c(cx).error(pos(node), kError_InvalidNamespace);
                                } else {
                                    let un = {
                                        let cxb = c(cx);
                                        cxb.get_unresolved_namespace(&cxb, Some(node.clone()), &r)
                                    };
                                    get!(node, as_attribute_list.namespaces).push_back(un);
                                    node.borrow_mut().as_attribute_list_mut().unwrap().namespace_ids.push(r.name());
                                }
                            }
                        }
                    }
                    None => c(cx).error(pos(node), kError_InvalidAttribute),
                }
            }
        }

        let ans = get!(node, as_attribute_list.namespaces);
        if ans.size() != 0 {
            let mut found_user_namespace = false;
            for ns in ans.to_vec() {
                if !ns.is_namespace_value() {
                    c(cx).error(pos(node), kError_InvalidAttribute);
                } else if !found_user_namespace {
                    found_user_namespace = true;
                    node.borrow_mut().as_attribute_list_mut().unwrap().set_user_namespace(Some(ns));
                } else {
                    c(cx).error(pos(node), kError_MultipleNamespaceAttributes);
                    break;
                }
            }
        }

        let (hp, hpu, hpr, hi, hun) = {
            let b = node.borrow();
            let a = b.as_attribute_list().unwrap();
            (a.has_private, a.has_public, a.has_protected, a.has_internal, a.has_user_namespace())
        };
        if (hp as i32 + hpu as i32 + hpr as i32 + hi as i32) > 1 {
            c(cx).error(pos(node), kError_ConflictingAccessSpecifiers);
        }
        if hun && (hp || hpu || hpr || hi) {
            c(cx).error(pos(node), kError_NamespaceAccessSpecifiers);
        }
        None
    }

    fn include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if !get!(node, as_include_directive.in_this_include) {
            set!(node, as_include_directive_mut.in_this_include = true);
            let statics = c(cx).statics.clone();
            let mut prev = Context::new(statics);
            prev.switch_to_context(&c(cx));
            set!(node, as_include_directive_mut.prev_cx = Some(Rc::new(RefCell::new(prev))));
            let ncx = nn(get!(node, as_include_directive.cx));
            if !Rc::ptr_eq(&ncx, cx) {
                cx.borrow_mut().switch_to_context(&ncx.borrow());
            }
        } else {
            set!(node, as_include_directive_mut.in_this_include = false);
            let prev = nn(get!(node, as_include_directive.prev_cx));
            cx.borrow_mut().switch_to_context(&prev.borrow());
            set!(node, as_include_directive_mut.prev_cx = None);
        }
        None
    }

    fn import(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let program = nn(get!(node, as_import.program));
        let baseobj = nn(get!(program, program.frame));
        let obj = Self::scope(cx);
        let filespec = nn(get!(node, as_import.filespec));
        let id = get!(filespec, as_literal_string.value);
        let qname = QName::new(Some(c(cx).public_namespace()), &id);
        let bui = new_import_builder(qname);
        inherit_slots(&baseobj, &obj, &bui, &c(cx));
        None
    }

    fn import_directive(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let cx = nn(get!(node, as_import_directive.cx));
        let Some(name) = get!(node, as_import_directive.name) else { return None };
        let val = self.eval(&cx, &name);
        match rv(val) {
            Some(r) => {
                let pkg_name = r.name();
                let is_package = {
                    let cxb = c(&cx);
                    cxb.is_namespace_name(&pkg_name) && cxb.get_namespace(&pkg_name).is_package()
                };
                if !is_package {
                    self.add_unresolved(Unresolved::Package, r.clone());
                }

                let id = nn(get!(name, as_package_name.id));
                let def_name = nn(get!(id, as_package_identifiers.def_part));
                let pkgdef = get!(node, as_import_directive.def.pkgdef);
                let in_pkg = pkgdef.is_some() && Self::scope_count(&cx) == 1;
                let pkg_ns = c(&cx).get_namespace(&pkg_name);
                if !def_name.is_empty() {
                    let importednames = if in_pkg { ImportedNames::Node(pkgdef.clone().unwrap()) } else { self.back_imported() };
                    if importednames.get(&def_name).is_none() {
                        importednames.with(|m| m.put(&def_name, Namespaces::new()));
                    }
                    let prop = nn(importednames.get(&def_name));
                    if !prop.contains(&pkg_ns) {
                        prop.push_back(pkg_ns.clone());
                        let r2 = ReferenceValue::new(&c(&cx), None, &def_name, Some(&pkg_ns));
                        self.add_unresolved(Unresolved::ImportDef, r2);
                    }
                } else if in_pkg {
                    get!(pkgdef.unwrap(), as_package_definition.used_namespaces).push_back(pkg_ns);
                } else {
                    self.back_used().push_back(pkg_ns);
                }
            }
            None => c(&cx).error(pos(&name), kError_Unknown_Namespace),
        }
        None
    }

    fn super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        match ilast(&self.super_context) {
            super_statement => {
                let index = Self::scope_count(cx) as i32 - 2;
                let obj = c(cx).scope_at(index);
                let bui = obj.builder().filter(|b| b.is_instance_builder());
                match &bui {
                    None => c(cx).internal_error("internal error: super statement outside of instance constructor"),
                    Some(b) => b.instance_data_mut().calls_super_ctor = true,
                }
                let call = nn(get!(node, as_super_statement.call));
                if let Some(args) = get!(call, as_call_expression.args) {
                    self.eval(cx, &args);
                }
                let type_ = nn(obj.type_()).type_value();
                let basecls = type_.baseclass();
                let baseobj = match basecls {
                    Some(b) => b.prototype(),
                    None => c(cx).no_type().prototype(),
                };
                set!(node, as_super_statement_mut.baseobj = baseobj);
                iset_last(&mut self.super_context, super_error2);
            }
            super_error2 => c(cx).error(pos(node), kError_IllegalSuperStatement),
            super_error_es4 => c(cx).error(pos(node), kError_InvalidES4SuperStatement),
            _ => c(cx).error(pos(node), kError_InvalidSuperStatement),
        }
        None
    }

    fn namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if get!(node, namespace_definition.needs_init) {
            self.add_stmt(node);
            let gen_bits = self.get_emitter().new_def(node);
            set!(node, namespace_definition_mut.gen_bits = gen_bits.clone());
            let slot = nn(get!(node, namespace_definition.ref_)).get_slot(&c(cx));
            if let Some(slot) = slot {
                slot.add_def_bits(gen_bits);
            }
            set!(node, namespace_definition_mut.needs_init = false);
            return None;
        }

        if get!(node, namespace_definition.ref_).is_some() {
            return None;
        }

        let attrs = get!(node, namespace_definition.def.attrs);
        if let Some(a) = &attrs {
            if a.borrow().has_attribute(STATIC) {
                c(cx).error(pos(a), kError_StaticModifiedNamespace);
            }
        }

        let pkgdef = get!(node, namespace_definition.def.pkgdef);
        if let Some(p) = &pkgdef {
            if Self::scope_count(cx) == 1 {
                self.push_package_namespaces(p, false);
            }
        }

        let class_builder = self.class_builder_on_scope_chain(cx);
        if class_builder.as_ref().map_or(false, |b| b.class_data().is_interface) {
            c(cx).error(pos(node), kError_NamespaceInInterface);
        }
        let namespaces = Namespaces::new();
        let mut namespace_ids: Vec<String> = Vec::new();
        self.compute_namespaces(cx, &attrs, &namespaces, &mut namespace_ids);

        let v = self.eval_opt(cx, &get!(node, namespace_definition.name));
        set!(node, namespace_definition_mut.ref_ = rv(v));
        let r = nn(get!(node, namespace_definition.ref_));

        let obj = Self::scope(cx);
        let bui = builder_of(&obj);
        let is_intrinsic = false;
        let mut slot_id = -1;

        let has_namespaces = obj.has_names(&c(cx), GET_TOKEN, &r.name(), &namespaces);
        if has_namespaces.is_some() {
            c(cx).error(pos(node), kError_DuplicateNamespaceDefinition);
        } else {
            let cxb = c(cx);
            if bui.is_intrinsic() || is_intrinsic {
                slot_id = bui.explicit_var_nss(&cxb, &obj, &r.name(), &namespaces, Some(&cxb.no_type()), -1, -1, -1);
            } else {
                let var_id = bui.variable(&cxb, &obj);
                slot_id = bui.explicit_var_nss(&cxb, &obj, &r.name(), &namespaces, Some(&cxb.no_type()), -1, -1, var_id);
            }
        }

        let debug_name = self.back_region();
        set!(node, namespace_definition_mut.debug_name = Some(debug_name.clone()));
        if let Some(s) = obj.get_slot(&c(cx), slot_id) {
            s.set_debug_name(&debug_name);
        }

        let qn = c(cx).compute_qualified_name(&debug_name, &r.name(), &namespaces.back(), EMPTY_TOKEN);
        set!(node, namespace_definition_mut.qualifiedname = Some(qn));

        if !is_intrinsic && slot_id >= 0 {
            let slot = nn(obj.get_slot(&c(cx), slot_id));
            let value = get!(node, namespace_definition.value);
            match &value {
                Some(value) => {
                    let lsn = value.borrow().as_literal_string().map(|l| l.value.clone());
                    if let Some(s) = lsn {
                        slot.set_object_value(Some(c(cx).get_namespace_kind(&s, NS_EXPLICIT)));
                    } else {
                        let val = self.eval(cx, value);
                        if let Some(Value::Reference(vr)) = val {
                            let ns_slot = vr.get_slot(&c(cx));
                            if let Some(ns_slot) = ns_slot {
                                if c(cx).is_namespace(ns_slot.get_object_value().as_ref()) {
                                    slot.set_object_value(ns_slot.get_object_value());
                                }
                            }
                        }
                    }
                }
                None => {
                    let cxb = c(cx);
                    let name = cxb.debug_name(&self.back_region(), &r.name(), Some(&namespace_ids), EMPTY_TOKEN);
                    let ns = cxb.get_namespace_kind(&name, NS_INTERNAL);
                    slot.set_object_value(Some(ns));
                }
            }
            slot.set_const(true);
            if slot.get_object_value().is_none() {
                c(cx).error(pos_of(&value), kError_InvalidNamespaceInitializer);
            }
            set!(node, namespace_definition_mut.needs_init = true);
        }

        if pkgdef.is_some() && Self::scope_count(cx) == 1 {
            self.pop_package_namespaces(false);
        }
        None
    }

    fn use_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if get!(node, as_use_directive.ref_).is_some() {
            return None;
        }
        let pkgdef = get!(node, as_use_directive.def.pkgdef);
        if let Some(p) = &pkgdef {
            if Self::scope_count(cx) == 1 {
                self.push_package_namespaces(p, false);
            }
        }

        let mut obj: Option<ObjectValue> = None;
        if let Some(expr) = get!(node, as_use_directive.expr) {
            self.flush_unresolved(Unresolved::Rt);
            let v = self.eval(cx, &expr);
            let r = v.map(|v| v.as_reference_value().expect(CCE));
            set!(node, as_use_directive_mut.ref_ = r.clone());
            let value = nn(r).get_value(&c(cx));
            obj = value.and_then(|v| v.as_object_value());
            self.flush_unresolved(Unresolved::Ns);
        }

        let in_pkg = pkgdef.is_some() && Self::scope_count(cx) == 1;
        let ns = match obj {
            Some(o) => o,
            None => {
                let r = nn(get!(node, as_use_directive.ref_));
                let cxb = c(cx);
                cxb.get_unresolved_namespace(&cxb, Some(node.clone()), &r)
            }
        };
        if in_pkg {
            get!(pkgdef.clone().unwrap(), as_package_definition.used_namespaces).push_back(ns);
        } else {
            self.back_used().push_back(ns);
        }

        if pkgdef.is_some() && Self::scope_count(cx) == 1 {
            self.pop_package_namespaces(false);
        }
        None
    }

    fn process_interface_public_methods(&mut self, cx: &ContextRef, iframe: &ObjectValue) {
        let ibui = iframe.builder().filter(|b| b.is_instance_builder()).expect(CCE);
        let Some(interface_methods) = self.interface_methods.clone() else { return };
        let cxb = c(cx);
        let public = cxb.public_namespace();
        let mut i = 0;
        loop {
            i = interface_methods.has_next(i);
            if i == -1 {
                break;
            }
            let idx = i as usize;
            let name = interface_methods.get_name(idx);
            let ty = interface_methods.get_type(idx);
            let interface_namespace = interface_methods.get_namespace(idx);

            let kind = if ty == Names::GET_NAMES {
                GET_TOKEN
            } else if ty == Names::SET_NAMES {
                SET_TOKEN
            } else {
                EMPTY_TOKEN
            };

            let slot_id = iframe.get_slot_index(&cxb, if kind == EMPTY_TOKEN { GET_TOKEN } else { kind }, &name, Some(&public));
            if slot_id <= 0 {
                i += 1;
                continue;
            }
            let implied_id = iframe.get_implicit_index(&cxb, slot_id, EMPTY_TOKEN);
            if nn(ibui.object_value()).has_name(&cxb, kind, &name, &interface_namespace) {
                i += 1;
                continue;
            }
            match ty {
                Names::GET_NAMES => {
                    iframe.define_name(&cxb, GET_TOKEN, &name, &interface_namespace, slot_id);
                    ibui.name(&cxb, GET_TOKEN, &name, &interface_namespace);
                }
                Names::SET_NAMES => {
                    iframe.define_name(&cxb, SET_TOKEN, &name, &interface_namespace, slot_id);
                    ibui.name(&cxb, SET_TOKEN, &name, &interface_namespace);
                }
                Names::METHOD_NAMES => {
                    iframe.define_name(&cxb, GET_TOKEN, &name, &interface_namespace, slot_id);
                    ibui.name(&cxb, GET_TOKEN, &name, &interface_namespace);
                    iframe.define_name(&cxb, EMPTY_TOKEN, &name, &interface_namespace, implied_id);
                    ibui.name(&cxb, EMPTY_TOKEN, &name, &interface_namespace);
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn scan_interface_methods(&mut self, _cx: &ContextRef, node: &NodeRef) {
        let iframe = nn(get!(node, class_definition.iframe));
        let mut interface_walker = InterfaceWalker::new(&iframe);
        if interface_walker.has_next() {
            self.interface_methods = Some(Names::new());
        }
        let used_def = get!(node, class_definition.used_def_namespaces);
        while interface_walker.has_next() {
            let interface_iframe = nn(interface_walker.next());
            let interface_cframe = nn(interface_iframe.type_()).type_value();
            used_def.push_back(interface_cframe.as_object().clone());

            let interface_builder = interface_iframe.builder().filter(|b| b.is_instance_builder()).expect(CCE);
            if let Some(names) = interface_builder.get_names() {
                let mut i = 0;
                loop {
                    i = names.has_next(i);
                    if i == -1 {
                        break;
                    }
                    let idx = i as usize;
                    if names.get_namespace(idx).compare_to(interface_cframe.as_object()) != 0 {
                        i += 1;
                        continue;
                    }
                    let name = names.get_name(idx);
                    let ty = names.get_type(idx);
                    self.interface_methods.as_mut().unwrap().put(&name, interface_cframe.as_object(), ty, 0);
                    i += 1;
                }
            }
        }
    }
}

/// `inheritSlots(baseobj, obj, bui, cx)`.
pub fn inherit_slots(baseobj: &ObjectValue, obj: &ObjectValue, bui: &Builder, cx: &Context) {
    inherit_slots_limit(baseobj, obj, bui, cx, false);
}

/// `inheritContextSlots`: only the slots the base object's own context declared.
pub fn inherit_context_slots(baseobj: &ObjectValue, obj: &ObjectValue, bui: &Builder, cx: &Context) {
    inherit_slots_limit(baseobj, obj, bui, cx, true);
}

/// `inheritSlots(baseobj, obj, bui, cx, limitToContext)`.
pub fn inherit_slots_limit(baseobj: &ObjectValue, obj: &ObjectValue, bui: &Builder, cx: &Context, limit_to_context: bool) {
    let basebui = builder_of(baseobj);
    let Some(names) = basebui.get_names() else { return };
    let public = cx.public_namespace();
    let mut i = 0;
    loop {
        i = names.has_next(i);
        if i == -1 {
            break;
        }
        let idx = i as usize;
        let name = names.get_name(idx);
        let namespace = names.get_namespace(idx);
        match names.get_type(idx) {
            Names::METHOD_NAMES => {
                let inherited_get_id = baseobj.get_slot_index(cx, GET_TOKEN, &name, Some(&namespace));
                let inherited_call_id = baseobj.get_implicit_index(cx, inherited_get_id, EMPTY_TOKEN);
                let inherited_slot = baseobj.get_slot(cx, inherited_get_id);
                let inherited_call_slot = baseobj.get_slot(cx, inherited_call_id);
                bui.inherit_call(cx, obj, &name, &namespace, nn(inherited_slot), nn(inherited_call_slot));
            }
            Names::GET_NAMES => 'get: {
                let slot_id = baseobj.get_slot_index(cx, GET_TOKEN, &name, Some(&namespace));
                let slot = baseobj.get_slot(cx, slot_id);
                let Some(slot) = slot else { break 'get };
                if limit_to_context {
                    if let Some(db) = slot.declared_by() {
                        if builder_of(&db).data().context_id != basebui.data().context_id {
                            break 'get;
                        }
                    }
                }
                let implicit_id = baseobj.get_implicit_index(cx, slot_id, EMPTY_TOKEN);
                if slot_id != implicit_id && slot.is_method_slot() {
                    let inherited_call_slot = baseobj.get_slot(cx, implicit_id);
                    bui.inherit_call(cx, obj, &name, &namespace, slot, nn(inherited_call_slot));
                    break 'get;
                }
                if slot.is_variable_slot() {
                    slot.set_var_index(-1);
                    let index = slot.implies(cx, EMPTY_TOKEN);
                    let inherited_call_slot = baseobj.get_slot(cx, index);
                    let index = slot.implies(cx, NEW_TOKEN);
                    let inherited_constructor_slot = baseobj.get_slot(cx, index);
                    bui.inherit_var(cx, obj, &name, &namespace, slot.clone());
                    if let Some(s) = inherited_call_slot {
                        obj.add_slot(s);
                    }
                    if let Some(s) = inherited_constructor_slot {
                        obj.add_slot(s);
                    }
                }
                if !namespace.ptr_eq(&public) {
                    break 'get;
                }
                bui.inherit_get(cx, obj, &name, &namespace, slot);
            }
            Names::SET_NAMES => {
                if namespace.ptr_eq(&public) {
                    let slot_id = baseobj.get_slot_index(cx, SET_TOKEN, &name, Some(&namespace));
                    let slot = baseobj.get_slot(cx, slot_id);
                    bui.inherit_set(cx, obj, &name, &namespace, nn(slot));
                }
            }
            Names::VAR_NAMES => {
                let slot_id = baseobj.get_slot_index(cx, VAR_TOKEN, &name, Some(&namespace));
                let inherited_slot = nn(baseobj.get_slot(cx, slot_id));
                inherited_slot.set_var_index(-1);
                let index = inherited_slot.implies(cx, EMPTY_TOKEN);
                let inherited_call_slot = baseobj.get_slot(cx, index);
                let index = inherited_slot.implies(cx, NEW_TOKEN);
                let inherited_constructor_slot = baseobj.get_slot(cx, index);
                bui.inherit_var(cx, obj, &name, &namespace, inherited_slot);
                if let Some(s) = inherited_call_slot {
                    obj.add_slot(s);
                }
                if let Some(s) = inherited_constructor_slot {
                    obj.add_slot(s);
                }
            }
            _ => {}
        }
        i += 1;
    }
}

impl Evaluator for FlowAnalyzer {
    fn check_feature(&mut self, _cx: &ContextRef, node: &NodeRef) -> bool {
        if node.borrow().block.is_none() && self.doing_method() {
            let b = self.flow.borrow().get_block();
            node.borrow_mut().block = Some(b);
        }
        true
    }

    fn evaluate_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_identifier_impl(cx, node)
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_qualified_identifier_impl(cx, node)
    }

    fn evaluate_qualified_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if node.borrow().identifier().unwrap().ref_.is_none() {
            self.evaluate_qualified_identifier_impl(cx, node);
            self.eval_opt(cx, &get!(node, as_qualified_expression.expr));
        }
        node.borrow().identifier().unwrap().ref_.clone().map(Value::Reference)
    }

    fn evaluate_this_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        match ilast(&self.this_contexts) {
            error_this | cinit_this | package_this => c(cx).error(pos(node), kError_ThisUsedInStaticFunction),
            init_this => c(cx).error(pos(node), kError_ThisUsedInInitializer),
            _ => {}
        }
        None
    }

    fn evaluate_literal_boolean(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        let v = get!(node, as_literal_boolean.value);
        Some(Value::Object(if v { c(cx).boolean_true() } else { c(cx).boolean_false() }))
    }

    fn evaluate_literal_null(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        None
    }

    fn evaluate_literal_number(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        set!(node, as_literal_number_mut.number_usage = self.number_usage_stack.last().cloned());
        self.add_stmt(node);
        None
    }

    fn evaluate_literal_string(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        None
    }

    fn evaluate_literal_reg_exp(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        None
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        if let Some(f) = get!(node, as_literal_object.fieldlist) {
            self.eval(cx, &f);
        }
        None
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let t = self.allocate_temp();
        self.add_stmt(node);
        if let Some(name) = get!(node, as_literal_field.name) {
            let ident_name = name.borrow().identifier().map(|i| i.name.clone());
            match ident_name {
                Some(iname) => {
                    let namespaces = Namespaces::new();
                    let r = ReferenceValue::with_namespaces(&c(cx), None, &iname, &namespaces);
                    r.set_is_attribute_identifier(false);
                    r.set_position(pos(node));
                    set!(node, as_literal_field_mut.ref_ = Some(r));
                }
                None => {
                    let value = self.eval(cx, &name);
                    set!(node, as_literal_field_mut.ref_ = rv(value));
                }
            }
        }
        self.eval_opt(cx, &get!(node, as_literal_field.value));
        self.free_temp(t);
        get!(node, as_literal_field.ref_).map(Value::Reference)
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        if let Some(e) = get!(node, as_literal_array.elementlist) {
            self.eval(cx, &e);
        }
        None
    }

    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        self.eval_opt(cx, &get!(node, as_literal_vector.type_));
        if let Some(e) = get!(node, as_literal_vector.elementlist) {
            self.eval(cx, &e);
        }
        None
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.member_expression(cx, node)
    }

    fn evaluate_apply_type_expr(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(r) = get!(node, as_apply_type_expr.sel.ref_) {
            return Some(Value::Reference(r));
        }
        self.add_stmt(node);
        let type_args = get!(node, as_apply_type_expr.type_args);
        if let Some(ta) = &type_args {
            self.eval(cx, ta);
        }
        let expr = nn(get!(node, as_apply_type_expr.sel.expr));
        let value = self.eval(cx, &expr);
        if expr.borrow().identifier().is_some() {
            let mut r = rv(value);
            if let Some(ta) = &type_args {
                let values = ta.borrow().list().expect(CCE).values.clone();
                let typerefs: Vec<ReferenceValue> = values.iter().filter_map(|v| v.as_ref().and_then(|v| v.as_reference_value())).collect();
                if typerefs.len() != values.len() {
                    r = None;
                }
                if let Some(r) = &r {
                    r.add_type_param(typerefs.first().cloned().expect("IndexOutOfBoundsException"));
                }
            }
            set!(node, as_apply_type_expr_mut.sel.ref_ = r);
        } else {
            set!(node, as_apply_type_expr_mut.sel.ref_ = None);
        }
        get!(node, as_apply_type_expr.sel.ref_).map(Value::Reference)
    }

    fn evaluate_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(r) = get!(node, as_call_expression.sel.ref_) {
            return Some(Value::Reference(r));
        }
        let t = self.allocate_temp();
        self.add_stmt(node);
        if let Some(args) = get!(node, as_call_expression.args) {
            self.eval(cx, &args);
        }
        let expr = nn(get!(node, as_call_expression.sel.expr));
        let value = self.eval(cx, &expr);
        let r = if expr.borrow().identifier().is_some() { rv(value) } else { None };
        set!(node, as_call_expression_mut.sel.ref_ = r.clone());

        if let Some(r) = &r {
            if r.name() == "Vector$object" && r.get_immutable_namespaces().contains(&c(cx).public_namespace()) {
                let tin_args = expr.borrow().as_type_identifier().map(|t| t.type_args.clone());
                if let Some(type_args) = tin_args {
                    let first = nn(type_args).borrow().list().expect(CCE).items.first().cloned().expect("IndexOutOfBoundsException");
                    let args = get!(node, as_call_expression.args);
                    let mut new_args: Vec<Option<NodeRef>> = vec![first];
                    if let Some(a) = &args {
                        new_args.extend(a.borrow().as_argument_list().expect(CCE).items.iter().cloned());
                    }
                    let nl = Context::get_node_factory(cx).borrow_mut().argument_list(None, None);
                    nl.borrow_mut().as_argument_list_mut().unwrap().items = new_args;
                    set!(node, as_call_expression_mut.args = Some(nl));
                }
            }
        }

        if node.borrow().is_attribute_identifier() {
            c(cx).error(pos(node) - 2, kError_AttributesAreNotCallable);
        }
        if let Some(r) = &r {
            r.set_base(get!(node, as_call_expression.sel.base));
        }
        self.free_temp(t);
        r.map(Value::Reference)
    }

    fn evaluate_invoke(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        let name = get!(node, as_invoke.name);
        let r = ReferenceValue::new(&c(cx), None, &name, Some(&c(cx).public_namespace()));
        r.set_kind(EMPTY_TOKEN);
        r.set_base(get!(node, as_invoke.sel.base));
        r.set_position(pos(node));
        set!(node, as_invoke_mut.sel.ref_ = Some(r));
        if let Some(args) = get!(node, as_invoke.args) {
            self.eval(cx, &args);
        }
        None
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        let t = self.allocate_temp();
        let value = self.eval_opt(cx, &get!(node, as_set_expression.sel.expr));
        let r = rv(value);
        set!(node, as_set_expression_mut.sel.ref_ = r.clone());
        if let Some(r) = &r {
            let base = get!(node, as_set_expression.sel.base);
            if base.is_some() {
                r.set_base(base);
            }
            if Self::with_depth(cx) == -1 {
                r.get_slot_kind(&c(cx), SET_TOKEN);
                let gen_bits = self.get_emitter().new_def(node);
                set!(node, as_set_expression_mut.gen_bits = gen_bits);
            }
        }
        let args = get!(node, as_set_expression.args);
        match &r {
            None => {
                self.eval_opt(cx, &args);
            }
            Some(r) => {
                self.eval_opt(cx, &args);
                if Self::with_depth(cx) == -1 {
                    r.set_base(get!(node, as_set_expression.sel.base));
                    let slot = r.get_slot_kind(&c(cx), GET_TOKEN);
                    r.set_kind(SET_TOKEN);
                    if let Some(slot) = slot {
                        slot.add_def_bits(get!(node, as_set_expression.gen_bits));
                    }
                }
            }
        }
        self.free_temp(t);
        r.map(Value::Reference)
    }

    fn evaluate_delete_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        let val = self.eval_opt(cx, &get!(node, as_delete_expression.sel.expr));
        let r = rv(val);
        set!(node, as_delete_expression_mut.sel.ref_ = r.clone());
        if let Some(r) = &r {
            r.set_base(get!(node, as_delete_expression.sel.base));
            r.set_kind(GET_TOKEN);
        }
        r.map(Value::Reference)
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        let val = self.eval_opt(cx, &get!(node, as_get_expression.sel.expr));
        let r = rv(val);
        set!(node, as_get_expression_mut.sel.ref_ = r.clone());
        let base = get!(node, as_get_expression.sel.base);
        if let Some(r) = &r {
            if base.is_some() {
                r.set_base(base);
                r.set_kind(GET_TOKEN);
            }
        }
        r.map(Value::Reference)
    }

    fn evaluate_increment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let t1 = self.allocate_temp();
        let t2 = self.allocate_temp();
        let t3 = self.allocate_temp();
        set!(node, as_increment_mut.number_usage = self.number_usage_stack.last().cloned());
        self.add_stmt(node);
        let val = self.eval_opt(cx, &get!(node, as_increment.sel.expr));
        let r = rv(val);
        set!(node, as_increment_mut.sel.ref_ = r.clone());
        if let Some(r) = &r {
            r.set_base(get!(node, as_increment.sel.base));
            r.get_slot_kind(&c(cx), GET_TOKEN);
        }
        self.free_temp(t3);
        self.free_temp(t2);
        self.free_temp(t1);
        r.map(Value::Reference)
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        self.eval_opt(cx, &get!(node, as_unary_expression.expr));
        set!(node, as_unary_expression_mut.number_usage = self.number_usage_stack.last().cloned());
        None
    }

    fn evaluate_binary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.add_stmt(node);
        self.eval_opt(cx, &get!(node, as_binary_expression.lhs));
        self.eval_opt(cx, &get!(node, as_binary_expression.rhs));
        set!(node, as_binary_expression_mut.number_usage = self.number_usage_stack.last().cloned());
        None
    }

    fn evaluate_conditional_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.eval_opt(cx, &get!(node, as_conditional_expression.condition));
        self.if_(IF_false);
        self.eval_opt(cx, &get!(node, as_conditional_expression.thenexpr));
        self.else_();
        let ip = self.get_ip();
        self.patch_if(ip);
        self.eval_opt(cx, &get!(node, as_conditional_expression.elseexpr));
        let ip = self.get_ip();
        self.patch_else(ip);
        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let size = node.borrow().as_argument_list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().as_argument_list().unwrap().items.get(i).cloned().expect("IndexOutOfBoundsException");
            if let Some(n) = n {
                self.eval(cx, &n);
            }
        }
        None
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;
        let size = node.borrow().list().unwrap().items.len();
        for i in 0..size {
            let n = node.borrow().list().unwrap().items.get(i).cloned().expect("IndexOutOfBoundsException");
            val = self.eval_opt(cx, &n);
            node.borrow_mut().list_mut().unwrap().values.push(val.clone());
        }
        val
    }

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.statement_list(cx, node)
    }

    fn evaluate_empty_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = get!(node, as_expression_statement.expr);
        if let Some(e) = &expr {
            self.eval(cx, e);
        }
        let is_var = node.borrow().as_expression_statement().unwrap().is_var_statement();
        if self.define_cv && !is_var {
            let gen_bits = self.get_emitter().new_def(node);
            set!(node, as_expression_statement_mut.gen_bits = gen_bits.clone());
            let cv = {
                let cxb = c(cx);
                nn(nn(cxb.global_scope()).get_slot(&cxb, SLOT_Global__cv))
            };
            cv.add_def_bits(gen_bits);
        } else if expr.is_some() {
            let void = c(cx).void_type();
            let mut b = node.borrow_mut();
            b.void_result();
            b.as_expression_statement_mut().unwrap().expected_type = Some(void);
        }
        None
    }

    fn evaluate_labeled_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let label = nn(get!(node, as_labeled_statement.label));
        self.eval(cx, &label);
        let mnode = label.borrow().as_list().expect(CCE).items.last().cloned().flatten();
        let r = nn(mnode).borrow().as_member_expression().expect(CCE).ref_.clone();
        let obj = nn(c(cx).get_scopes().last().cloned());

        let label_name = match &r {
            None => {
                c(cx).error(pos(node), kError_InvalidLabel);
                None
            }
            Some(r) => Some(r.name()),
        };

        let is_loop_label = get!(node, as_labeled_statement.is_loop_label);
        if let Some(name) = &label_name {
            if obj.define_name(&c(cx), GET_TOKEN, name, &ObjectValue::label_namespace(), self.loop_index) == 0 {
                c(cx).error(pos(node), kError_DuplicateLabel);
            }
            if is_loop_label && obj.define_name(&c(cx), GET_TOKEN, name, &ObjectValue::loop_label_namespace(), self.loop_index) == 0 {
                c(cx).error(pos(node), kError_DuplicateLabel);
            }
        }

        if let Some(statement) = get!(node, as_labeled_statement.statement) {
            set!(node, as_labeled_statement_mut.loop_index = self.loop_index);
            if !is_loop_label {
                self.loop_index += 1;
                self.label_statement_begin();
            }
            self.eval(cx, &statement);
            if !is_loop_label {
                let li = get!(node, as_labeled_statement.loop_index);
                self.label_statement_end(li);
                self.loop_index -= 1;
            }
        }

        if let Some(name) = &label_name {
            obj.remove_name(&c(cx), GET_TOKEN, name, &ObjectValue::label_namespace());
        }
        None
    }

    fn evaluate_if_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let val = self.eval_opt(cx, &get!(node, as_if_statement.condition));
        let val = val.and_then(|v| v.get_value(&c(cx)));
        let is_boolean = val.as_ref().map_or(false, |v| {
            let cxb = c(cx);
            nn(v.get_type(&cxb)).get_type_value().map_or(false, |t| t.ptr_eq(&cxb.boolean_type()))
        });
        if is_boolean {
            let b = val.unwrap().boolean_value();
            set!(node, as_if_statement_mut.is_true = b);
            set!(node, as_if_statement_mut.is_false = !b);
            if b {
                if let Some(t) = get!(node, as_if_statement.thenactions) {
                    self.eval(cx, &t);
                }
            } else if let Some(e) = get!(node, as_if_statement.elseactions) {
                self.eval(cx, &e);
            }
        } else {
            self.if_(IF_false);
            if let Some(t) = get!(node, as_if_statement.thenactions) {
                self.eval(cx, &t);
            }
            self.else_();
            let ip = self.get_ip();
            self.patch_if(ip);
            if let Some(e) = get!(node, as_if_statement.elseactions) {
                self.eval(cx, &e);
            }
            let ip = self.get_ip();
            self.patch_else(ip);
        }
        None
    }

    fn evaluate_switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.switch_statement(cx, node)
    }

    fn evaluate_case_label(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let label = get!(node, as_case_label.label);
        let mut duplicate = false;
        {
            let case_list = nn(self.case_exprs.last_mut());
            case_list.items.push(label.clone());
            if label.is_none() {
                if case_list.has_default {
                    duplicate = true;
                } else {
                    case_list.has_default = true;
                }
            }
        }
        if duplicate {
            c(cx).error(pos(node), kError_MultipleSwitchDefaults);
        }
        self.case_label(label.is_none());
        node.borrow_mut().block = None;
        self.check_feature(cx, node);
        None
    }

    fn evaluate_do_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.loop_begin();
        let ip = self.get_ip();
        self.patch_loop_begin(ip);
        if let Some(s) = get!(node, as_do_statement.statements) {
            self.inside_loop += 1;
            set!(node, as_do_statement_mut.loop_index = self.loop_index);
            self.loop_index += 1;
            self.eval(cx, &s);
            self.loop_index -= 1;
            self.inside_loop -= 1;
        }
        let li = get!(node, as_do_statement.loop_index);
        self.patch_continue(li);
        if let Some(e) = get!(node, as_do_statement.expr) {
            self.eval(cx, &e);
        }
        self.loop_end(IF_true);
        self.patch_break(li);
        None
    }

    fn evaluate_while_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.loop_begin();
        if let Some(s) = get!(node, as_while_statement.statement) {
            self.inside_loop += 1;
            set!(node, as_while_statement_mut.loop_index = self.loop_index);
            self.loop_index += 1;
            self.eval(cx, &s);
            self.loop_index -= 1;
            self.inside_loop -= 1;
        }
        let ip = self.get_ip();
        self.patch_loop_begin(ip);
        let li = get!(node, as_while_statement.loop_index);
        self.patch_continue(li);
        if let Some(e) = get!(node, as_while_statement.expr) {
            self.eval(cx, &e);
        }
        self.loop_end(IF_true);
        self.patch_break(li);
        None
    }

    fn evaluate_for_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(init) = get!(node, as_for_statement.initialize) {
            let mut init = init;
            if init.borrow().is_definition() {
                let es = initializer_statement(&init, cx);
                let e = es.borrow().as_expression_statement().expect(CCE).expr.clone();
                set!(node, as_for_statement_mut.initialize = e.clone());
                init = nn(e);
            }
            self.eval(cx, &init);
        }
        self.loop_begin();
        if let Some(s) = get!(node, as_for_statement.statement) {
            self.inside_loop += 1;
            set!(node, as_for_statement_mut.loop_index = self.loop_index);
            self.loop_index += 1;
            self.eval(cx, &s);
            self.loop_index -= 1;
            self.inside_loop -= 1;
        }
        let li = get!(node, as_for_statement.loop_index);
        self.patch_continue(li);
        if let Some(inc) = get!(node, as_for_statement.increment) {
            self.eval(cx, &inc);
        }
        let ip = self.get_ip();
        self.patch_loop_begin(ip);
        if let Some(t) = get!(node, as_for_statement.test) {
            self.eval(cx, &t);
        }
        self.loop_end(IF_true);
        self.patch_break(li);
        None
    }

    fn evaluate_with_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(e) = get!(node, as_with_statement.expr) {
            self.eval(cx, &e);
        }
        self.push_scope();
        let activation = ObjectValue::with_builder(&c(cx), new_with_builder(), None);
        set!(node, as_with_statement_mut.activation = Some(activation.clone()));
        c(cx).push_scope(activation);
        if !self.with_used_stack.is_empty() {
            self.with_used_stack.pop();
            self.with_used_stack.push(1);
        }
        let save_with_depth = Self::with_depth(cx);
        Self::set_with_depth(cx, Self::scope_count(cx) as i32 - 1);
        if let Some(s) = get!(node, as_with_statement.statement) {
            self.eval(cx, &s);
        }
        Self::set_with_depth(cx, save_with_depth);
        self.pop_scope();
        c(cx).pop_scope();
        None
    }

    fn evaluate_continue_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(id) = get!(node, as_continue_statement.id) {
            let val = self.eval(cx, &id);
            if let Some(r) = rv(val) {
                r.set_qualifier(&c(cx), Some(&ObjectValue::loop_label_namespace()));
                r.set_base(c(cx).get_scopes().last().cloned());
                r.set_kind(GET_TOKEN);
                r.get_slot_kind(&c(cx), GET_TOKEN);
                set!(node, as_continue_statement_mut.loop_index = r.get_slot_index(GET_TOKEN));
            }
            if self.inside_loop < 0 {
                set!(node, as_continue_statement_mut.loop_index = -1);
            }
        } else if self.inside_loop > 0 {
            set!(node, as_continue_statement_mut.loop_index = self.loop_index - 1);
        } else {
            set!(node, as_continue_statement_mut.loop_index = -1);
        }
        let li = get!(node, as_continue_statement.loop_index);
        if li < 0 {
            c(cx).error(pos(node), kError_ContinueHasNoTarget);
        }
        self.continue_(li);
        None
    }

    fn evaluate_break_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(id) = get!(node, as_break_statement.id) {
            let val = self.eval(cx, &id);
            if let Some(r) = rv(val) {
                r.set_qualifier(&c(cx), Some(&ObjectValue::label_namespace()));
                r.set_base(c(cx).get_scopes().last().cloned());
                r.set_kind(GET_TOKEN);
                r.get_slot_kind(&c(cx), GET_TOKEN);
                set!(node, as_break_statement_mut.loop_index = r.get_slot_index(GET_TOKEN));
            }
        } else if self.inside_loop > 0 || self.inside_switch > 0 {
            set!(node, as_break_statement_mut.loop_index = self.loop_index - 1);
        } else {
            set!(node, as_break_statement_mut.loop_index = -1);
        }
        let li = get!(node, as_break_statement.loop_index);
        if li < 0 {
            c(cx).error(pos(node), kError_BreakHasNoTarget);
        }
        self.break_(li);
        None
    }

    fn evaluate_return_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let tc = ilast(&self.this_contexts);
        if tc == cinit_this {
            c(cx).error(pos(node), kError_CannotReturnFromStaticInit);
        } else if tc == package_this {
            c(cx).error(pos(node), kError_CannotReturnFromPackageInit);
        } else if tc == global_this && Self::scope(cx).builder().map_or(false, |b| b.is_global_builder()) {
            c(cx).error(pos(node), kError_CannotReturnFromGlobalInit);
        }
        if let Some(e) = get!(node, as_return_statement.expr) {
            self.eval(cx, &e);
        }
        self.return_(TYPE_none);
        if ilast(&self.super_context) == super_statement {
            iset_last(&mut self.super_context, super_error2);
        }
        None
    }

    fn evaluate_throw_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(e) = get!(node, as_throw_statement.expr) {
            self.eval(cx, &e);
        }
        if !node.borrow().is_synthetic() {
            self.throw();
        }
        if ilast(&self.super_context) == super_statement {
            iset_last(&mut self.super_context, super_error2);
        }
        None
    }

    fn evaluate_try_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let finallyblock = get!(node, as_try_statement.finallyblock);
        self.try_(finallyblock.is_some());
        if let Some(t) = get!(node, as_try_statement.tryblock) {
            self.eval(cx, &t);
        }
        self.catch_clauses_begin();
        if let Some(cl) = get!(node, as_try_statement.catchlist) {
            self.eval(cx, &cl);
        }
        self.catch_clauses_end();
        if let Some(f) = &finallyblock {
            self.eval(cx, f);
        }
        self.finally_clause_end();
        None
    }

    fn evaluate_catch_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let catch_builder = new_catch_builder();
        let activation = {
            let cxb = c(cx);
            ObjectValue::with_builder(&cxb, catch_builder.clone(), Some(&cxb.no_type()))
        };
        set!(node, as_catch_clause_mut.activation = Some(activation.clone()));
        c(cx).push_scope(activation);

        if let Some(parameter) = get!(node, as_catch_clause.parameter) {
            catch_builder.set_has_parameter(true);
            self.eval(cx, &parameter);
            let (ptyperef, ptype) = {
                let b = parameter.borrow();
                let p = b.parameter().expect(CCE);
                (p.typeref.clone(), p.type_.clone())
            };
            if ptyperef.is_some() {
                set!(node, as_catch_clause_mut.typeref = ptyperef);
            }
            if let Some(t) = &ptype {
                self.flush_unresolved(Unresolved::Rt);
                let val = self.eval(cx, t);
                let tr = rv(val);
                if let Some(tr) = &tr {
                    tr.set_type_annotation(true);
                }
                set!(node, as_catch_clause_mut.typeref = tr);
                let obj = Self::scope(cx);
                if obj.builder().map_or(false, |b| b.is_activation_builder()) {
                    self.flush_unresolved(Unresolved::Body);
                } else {
                    self.flush_unresolved(Unresolved::Ce);
                }
            }
        }

        self.catch(None, None);

        node.borrow_mut().block = None;
        self.check_feature(cx, node);

        if let Some(s) = get!(node, as_catch_clause.statements) {
            self.eval(cx, &s);
            self.exceptions_used_stack.pop();
            self.exceptions_used_stack.push(1);
        }

        c(cx).pop_scope();
        None
    }

    fn evaluate_finally_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.eval_opt(cx, &get!(node, as_finally_clause.default_catch));
        if let Some(s) = get!(node, as_finally_clause.statements) {
            self.eval(cx, &s);
        }
        None
    }

    fn evaluate_function_common(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.function_common(cx, node)
    }

    fn evaluate_binary_program(&mut self, _unused_cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (frame, ncx, clsdefs, ce) = {
            let b = node.borrow();
            let p = b.program().unwrap();
            (p.frame.clone(), p.cx.clone(), p.clsdefs.clone(), p.ce_unresolved.clone())
        };
        if let Some(frame) = frame {
            let ncx = nn(ncx);
            let cxb = c(&ncx);
            let scope = nn(cxb.scope());
            let scope_builder = builder_of(&scope);
            scope_builder.add_names(builder_of(&frame).get_names().as_ref());
            inherit_slots(&frame, &scope, &scope_builder, &cxb);
            builder_of(&frame).clear_names();
            drop(cxb);
            let target = nn(self.clsdefs_sets.last().cloned());
            for cdn in nn(clsdefs).borrow().iter() {
                target.borrow_mut().push(cdn.clone());
            }
        }
        let prog = nn(self.unresolved_sets.last().cloned());
        prog.borrow_mut().program_mut().expect(CCE).ce_unresolved.extend(ce);
        None
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.program(cx, node)
    }

    fn evaluate_package_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.package_definition(cx, node)
    }

    fn evaluate_package_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.eval_opt(cx, &get!(node, as_package_name.id))
    }

    fn evaluate_package_identifiers(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let r = ReferenceValue::new(&c(cx), None, "", Some(&c(cx).public_namespace()));
        r.set_position(pos(node));
        r.set_name(&nn(get!(node, as_package_identifiers.pkg_part)));
        Some(Value::Reference(r))
    }

    fn evaluate_node(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let msg = format!("Feature not supported: {}", node.borrow());
        c(cx).internal_error_at(pos(node), &msg);
        None
    }

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.variable_definition(cx, node)
    }

    fn evaluate_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.variable_binding(cx, node)
    }

    fn evaluate_function_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.function_definition(cx, node)
    }

    fn evaluate_binary_function_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_function_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let val = self.eval_opt(cx, &get!(node, as_function_name.identifier));
        rv(val).map(Value::Reference)
    }

    fn evaluate_function_signature(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(p) = get!(node, as_function_signature.parameter) {
            self.eval(cx, &p);
        }
        if let Some(result) = get!(node, as_function_signature.result) {
            let val = self.eval(cx, &result);
            let tr = rv(val);
            match &tr {
                None => c(cx).error(pos(&result), kError_UnknownType),
                Some(tr) => tr.set_type_annotation(true),
            }
            set!(node, as_function_signature_mut.typeref = tr);
        }
        None
    }

    fn evaluate_rest_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.rest_parameter(cx, node)
    }

    fn evaluate_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.parameter(cx, node)
    }

    fn evaluate_parameter_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().as_parameter_list().unwrap().items.clone();
        for it in &items {
            self.eval_opt(cx, it);
        }
        None
    }

    fn evaluate_to_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.eval_opt(cx, &get!(node, as_to_object.expr));
        None
    }

    fn evaluate_load_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(r) = get!(node, as_load_register.reg) {
            self.eval(cx, &r);
        }
        None
    }

    fn evaluate_store_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(r) = get!(node, as_store_register.reg) {
            self.eval(cx, &r);
        }
        self.eval_opt(cx, &get!(node, as_store_register.expr));
        None
    }

    fn evaluate_register(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_has_next(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_box(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_coerce(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(e) = get!(node, as_coerce.expr) {
            self.eval(cx, &e);
        }
        None
    }

    fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.class_definition(cx, node)
    }

    fn evaluate_binary_class_def(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.binary_class_def(cx, node)
    }

    fn evaluate_binary_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.binary_class_def(cx, node)
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.interface_definition(cx, node)
    }

    fn evaluate_class_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let mut val = None;
        if let Some(p) = get!(node, as_class_name.pkgname) {
            self.eval(cx, &p);
        }
        if let Some(i) = get!(node, as_class_name.ident) {
            val = self.eval(cx, &i);
        }
        val
    }

    fn evaluate_inheritance(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(b) = get!(node, as_inheritance.baseclass) {
            self.eval(cx, &b);
        }
        if let Some(i) = get!(node, as_inheritance.interfaces) {
            self.eval(cx, &i);
        }
        None
    }

    fn evaluate_attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.attribute_list(cx, node)
    }

    fn evaluate_include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.include_directive(cx, node)
    }

    fn evaluate_import(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.import(cx, node)
    }

    fn evaluate_import_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.import_directive(cx, node)
    }

    fn evaluate_super_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        match ilast(&self.this_contexts) {
            global_this | error_this => c(cx).error(pos(node), kError_InvalidSuperExpression),
            _ => {}
        }
        if let Some(e) = get!(node, as_super_expression.expr) {
            self.eval(cx, &e);
        }
        if ilast(&self.super_context) == super_statement {
            iset_last(&mut self.super_context, super_error2);
        }
        None
    }

    fn evaluate_super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.super_statement(cx, node)
    }

    fn evaluate_config_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.namespace_definition(cx, node)
    }

    fn evaluate_use_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.use_directive(cx, node)
    }

    fn evaluate_rest_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        c(cx).internal_error_at(pos(node), "RestExpressionNode not yet implemented");
        None
    }

    fn evaluate_error(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if !self.error_node_seen {
            self.error_node_seen = true;
            let (code, arg) = (get!(node, as_error.error_code), get!(node, as_error.error_arg));
            c(cx).error1(pos(node), code, &arg);
        }
        None
    }

    fn evaluate_pragma(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(l) = get!(node, as_pragma.list) {
            self.eval(cx, &l);
        }
        None
    }

    fn evaluate_use_precision(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let current = nn(self.number_usage_stack.last().cloned());
        let precision = get!(node, as_use_precision.precision);
        if (1..=34).contains(&precision) {
            current.set_precision(precision);
        } else {
            c(cx).error(pos(node), kError_InvalidPrecision);
        }
        None
    }

    fn evaluate_use_numeric(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let current = nn(self.number_usage_stack.last().cloned());
        current.set_usage(get!(node, as_use_numeric.numeric_mode));
        None
    }

    fn evaluate_use_rounding(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let current = nn(self.number_usage_stack.last().cloned());
        current.set_rounding(get!(node, as_use_rounding.mode));
        None
    }

    fn evaluate_pragma_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        c(cx).internal_error_at(pos(node), "PragmaExpressionNode not yet implemented");
        None
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        c(cx).internal_error_at(pos(node), "TypedIdentifierNode not yet implemented");
        None
    }

    fn evaluate_untyped_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        c(cx).internal_error_at(pos(node), "UntypedVariableBindingNode not yet implemented");
        None
    }

    fn evaluate_paren_list_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        c(cx).internal_error_at(pos(node), "ParenListExpressionNode not yet implemented");
        None
    }

    fn evaluate_paren_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        c(cx).internal_error_at(pos(node), "ParenExpressionNode not yet implemented");
        None
    }

    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(l) = get!(node, as_literal_xml.list) {
            self.eval(cx, &l);
        }
        self.add_stmt(node);
        None
    }

    fn evaluate_meta_data(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_empty_element(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_default_xml_namespace(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if let Some(e) = get!(node, as_default_xml_namespace.expr) {
            self.eval(cx, &e);
        }
        None
    }

    fn evaluate_doc_comment(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let v = self.eval_opt(cx, &get!(node, as_type_expression.expr));
        if let Some(Value::Reference(r)) = &v {
            let (na, n) = (get!(node, as_type_expression.nullable_annotation), get!(node, as_type_expression.is_nullable));
            r.set_nullable_annotation(na, n);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc::abc::abc_parser::tests::{mxmlc_context, playerglobal_abcs};
    use crate::asc::abc::abc_parser::AbcParser;
    use crate::asc::parser::parser::Parser;
    use crate::asc::semantics::configuration_evaluator::ConfigurationEvaluator;

    const SOURCE: &str = "package p { public class A { public var x:int; public function f(a:String, ...r):void { var y = a; } } }";

    #[test]
    fn builds_class_model() {
        let Some(abcs) = playerglobal_abcs() else { return };
        let cx = mxmlc_context();
        let statics = c(&cx).statics.clone();
        let mut imports = Vec::new();
        for abc in abcs {
            let cx2 = Rc::new(RefCell::new(Context::new(statics.clone())));
            let program = AbcParser::new(&cx2, abc).parse_abc();
            let nf = Context::get_node_factory(&cx2);
            let ls = nf.borrow_mut().literal_string_pos("", 0);
            imports.push(nf.borrow_mut().import(Some(ls), program));
            nf.borrow_mut().pkg_defs.clear();
        }
        let node = Parser::new_stream(&cx, SOURCE.as_bytes(), "A.as").parse_program().unwrap();
        node.borrow_mut().program_mut().unwrap().imports = imports;
        let global = ObjectValue::with_builder(&c(&cx), new_global_builder(), None);
        c(&cx).push_scope(global);
        let pkg_defs = Context::get_node_factory(&cx).borrow().pkg_defs.clone();
        node.borrow_mut().program_mut().unwrap().pkgdefs = pkg_defs;
        evaluate(&node, &cx, &mut ConfigurationEvaluator::new());
        let mut fa = FlowAnalyzer::new(Rc::new(RefCell::new(FlowGraphEmitter::new(&cx, "A.as", false))));
        evaluate(&node, &cx, &mut fa);
        evaluate(&node, &cx, &mut fa);
        assert_eq!(c(&cx).error_count(), 0);
        assert_eq!(node.borrow().program().unwrap().state, ProgramNode::Done);

        let cxb = c(&cx);
        let a = cxb.user_defined("p:A").expect("p:A");
        assert_eq!(a.baseclass().unwrap().to_string(), "Object");
        let iframe = a.prototype().unwrap();
        let public = cxb.public_namespace();
        assert!(iframe.has_name(&cxb, GET_TOKEN, "x", &public));
        let f = ReferenceValue::new(&cxb, Some(iframe.clone()), "f", Some(&public));
        let call = f.get_slot_kind(&cxb, EMPTY_TOKEN).expect("f call slot");
        assert_eq!(call.get_method_name(), "f$0");
        let ctor = builder_of(&iframe).instance_data().ctor_name.clone();
        assert_eq!(ctor.as_deref(), Some("A$0"));
        let clsdefs = node.borrow().program().unwrap().clsdefs.clone().unwrap();
        let fexprs = clsdefs.borrow()[0].borrow().class_definition().unwrap().fexprs.clone().unwrap();
        let activation = fexprs.borrow()[0].borrow().as_function_common().unwrap().fun.clone().unwrap().activation().unwrap();
        for name in ["a", "r", "y"] {
            assert!(activation.has_name_unqualified(&cxb, name, GET_TOKEN), "{name}");
        }
    }
}
