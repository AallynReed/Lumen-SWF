//! Port of `macromedia/asc/parser/NodeFactory.java`.

#![allow(non_upper_case_globals)]

use std::cell::{Ref, RefCell, RefMut};
use std::rc::{Rc, Weak};
use std::collections::{BTreeMap, HashSet};

use crate::asc::parser::node::*;
use crate::asc::parser::tokens::*;
use crate::asc::semantics::value::*;
use crate::asc::util::context::Context;
use crate::asc::util::errors::*;

// `embedding.avmplus.Features`.
const SPECIAL_FUNCTION_SYNTAX: bool = false;
const TRANSLATE_COMPOUND_NAMES: bool = true;
const COMPILE_TIME_IF: bool = false;
const USE_DEFINED_NAME_AS_INTERNAL_NAME: bool = false;

// `embedding.avmplus.ActionBlockConstants`.
const METHOD_Arguments: i32 = 0x1;
const METHOD_Needrest: i32 = 0x4;

const ARGUMENTS: &str = "arguments";

fn rb(n: &Option<NodeRef>) -> Ref<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow()
}

fn rbm(n: &Option<NodeRef>) -> RefMut<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow_mut()
}

fn is_kind(n: &Option<NodeRef>, f: impl Fn(&Node) -> bool) -> bool {
    n.as_ref().map_or(false, |n| f(&n.borrow()))
}

fn same_type_info(a: &Option<TypeInfo>, b: &Option<TypeInfo>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

pub struct NodeFactory {
    cx: Weak<RefCell<Context>>,
    pub compound_names: HashSet<String>,
    pub has_arguments: bool,
    pub has_rest: bool,
    pub has_unnamed_package: bool,
    pub has_dxns: bool,
    pub current_package: Option<NodeRef>,
    pub use_stmts: Option<NodeRef>,
    pub dxns: Option<NodeRef>,
    create_default_doc_comments: bool,
    clsdefs_sets: Vec<NodeList>,
    needs_prototype_ns: bool,
    fun_names: BTreeMap<String, i32>,
    pub pkg_names: BTreeMap<String, NodeRef>,
    pub pkg_defs: Vec<NodeRef>,
    namespace_names: BTreeMap<String, i32>,
}

impl NodeFactory {
    pub fn new(cx: &ContextRef) -> NodeFactory {
        NodeFactory {
            cx: Rc::downgrade(cx),
            compound_names: HashSet::new(),
            has_arguments: false,
            has_rest: false,
            has_unnamed_package: false,
            has_dxns: false,
            current_package: None,
            use_stmts: None,
            dxns: None,
            create_default_doc_comments: false,
            clsdefs_sets: Vec::new(),
            needs_prototype_ns: false,
            fun_names: BTreeMap::new(),
            pkg_names: BTreeMap::new(),
            pkg_defs: Vec::new(),
            namespace_names: BTreeMap::new(),
        }
    }

    pub fn get_context(&self) -> ContextRef {
        self.cx.upgrade().expect("NullPointerException")
    }

    pub fn set_context(&mut self, cx: &ContextRef) {
        self.cx = Rc::downgrade(cx);
    }

    pub fn init(&mut self, names: &[String]) {
        self.compound_names.extend(names.iter().cloned());
    }

    pub fn create_default_doc_comments(&mut self, val: bool) {
        self.create_default_doc_comments = val;
    }

    pub fn argument_list(&mut self, list: Option<NodeRef>, item: Option<NodeRef>) -> NodeRef {
        self.argument_list_pos(list, item, -1)
    }

    pub fn argument_list_pos(&mut self, list: Option<NodeRef>, mut item: Option<NodeRef>, pos: i32) -> NodeRef {
        if is_kind(&item, Node::is_identifier) {
            let get = self.get_expression(item);
            item = Some(self.member_expression_pos(None, Some(get), pos));
        }

        if let Some(list) = list {
            list.borrow_mut().as_argument_list_mut().unwrap().items.push(item);
            list
        } else {
            let node = ArgumentListNode::new(item.clone(), 0);
            node.borrow_mut().set_position_nonterminal(item.as_ref(), pos);
            node
        }
    }

    pub fn assignment_expression(&mut self, lhs: Option<NodeRef>, op: i32, rhs: Option<NodeRef>) -> NodeRef {
        self.assignment_expression_pos(lhs, op, rhs, -1)
    }

    pub fn assignment_expression_pos(
        &mut self,
        mut lhs: Option<NodeRef>,
        mut op: i32,
        mut rhs: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let node: NodeRef;
        let mut is_constinit = false;
        if op == CONST_TOKEN {
            op = ASSIGN_TOKEN;
            is_constinit = true;
        }

        let prev_op = op;
        if op != ASSIGN_TOKEN {
            loop {
                let inner = {
                    let l = rb(&lhs);
                    match l.list() {
                        Some(list) if list.items.len() == 1 => list.items[0].clone(),
                        _ => break,
                    }
                };
                lhs = inner;
            }
            op = if op == MULTASSIGN_TOKEN {
                MULT_TOKEN
            } else if op == DIVASSIGN_TOKEN {
                DIV_TOKEN
            } else if op == MODULUSASSIGN_TOKEN {
                MODULUS_TOKEN
            } else if op == PLUSASSIGN_TOKEN {
                PLUS_TOKEN
            } else if op == MINUSASSIGN_TOKEN {
                MINUS_TOKEN
            } else if op == LEFTSHIFTASSIGN_TOKEN {
                LEFTSHIFT_TOKEN
            } else if op == RIGHTSHIFTASSIGN_TOKEN {
                RIGHTSHIFT_TOKEN
            } else if op == UNSIGNEDRIGHTSHIFTASSIGN_TOKEN {
                UNSIGNEDRIGHTSHIFT_TOKEN
            } else if op == BITWISEANDASSIGN_TOKEN {
                BITWISEAND_TOKEN
            } else if op == BITWISEXORASSIGN_TOKEN {
                BITWISEXOR_TOKEN
            } else if op == BITWISEORASSIGN_TOKEN {
                BITWISEOR_TOKEN
            } else if op == LOGICALANDASSIGN_TOKEN {
                LOGICALAND_TOKEN
            } else if op == LOGICALXORASSIGN_TOKEN {
                LOGICALXOR_TOKEN
            } else if op == LOGICALORASSIGN_TOKEN {
                LOGICALOR_TOKEN
            } else {
                ERROR_TOKEN
            };
            rhs = Some(self.binary_expression(op, lhs.clone(), rhs));
        }

        if rb(&lhs).is_member_expression() {
            let (base, selector, is_get, is_indexed) = {
                let l = rb(&lhs);
                let member = l.as_member_expression().unwrap();
                let is_get = rb(&member.selector).is_get_expression();
                let is_indexed = is_get && member.is_indexed_member_expression();
                (member.base.clone(), member.selector.clone(), is_get, is_indexed)
            };
            if is_get {
                let get = selector;
                if is_indexed {
                    let args = self.argument_list(None, rhs.clone());
                    let set = self.set_expression_get(get, Some(args), is_constinit);
                    node = self.indexed_member_expression(base, Some(set));
                } else {
                    let args = self.argument_list(None, rhs.clone());
                    let rhs_pos = rb(&rhs).pos();
                    let set = self.set_expression_get_pos(get, Some(args), is_constinit, rhs_pos);
                    node = self.member_expression(base, Some(set));
                }
            } else {
                node = self.error(pos, kError_AssignmentTargetNotRefVal);
            }
        } else {
            let args = self.argument_list(None, rhs.clone());
            let set = self.set_expression(lhs.clone(), Some(args), is_constinit);
            node = self.member_expression(None, Some(set));
        }

        if self.get_context().borrow().script_assist_parsing {
            let mut n = node.borrow_mut();
            let expr_node = n.as_member_expression_mut().expect("ClassCastException");
            if prev_op != op {
                expr_node.set_orig_token(prev_op);
            }
        }

        node.borrow_mut().set_position_nonterminal(lhs.as_ref(), pos);
        node
    }

    pub fn attribute_list(&mut self, item: Option<NodeRef>, list: Option<NodeRef>) -> NodeRef {
        self.attribute_list_pos(item, list, -1)
    }

    pub fn attribute_list_pos(&mut self, item: Option<NodeRef>, list: Option<NodeRef>, pos: i32) -> NodeRef {
        if let Some(list) = list {
            list.borrow_mut().as_attribute_list_mut().unwrap().items.push(item);
            list
        } else {
            let item_pos = rb(&item).pos();
            let node = AttributeListNode::new(item.clone(), item_pos);
            node.borrow_mut().set_position_nonterminal(item.as_ref(), pos);
            node
        }
    }

    pub fn binary_expression(&mut self, op: i32, lhs: Option<NodeRef>, rhs: Option<NodeRef>) -> NodeRef {
        self.binary_expression_pos(op, lhs, rhs, -1)
    }

    pub fn binary_expression_pos(&mut self, op: i32, lhs: Option<NodeRef>, rhs: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = BinaryExpressionNode::new(op, lhs.clone(), rhs);
        node.borrow_mut().set_position_nonterminal(lhs.as_ref(), pos);
        node
    }

    pub fn block(&mut self, attributes: Option<NodeRef>, statements: Option<NodeRef>) -> NodeRef {
        self.block_pos(attributes, statements, -1)
    }

    pub fn block_pos(&mut self, attributes: Option<NodeRef>, statements: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = BlockNode::new(attributes.clone(), statements);
        node.borrow_mut().set_position_nonterminal(attributes.as_ref(), pos);
        node
    }

    pub fn break_statement(&mut self, id: Option<NodeRef>) -> NodeRef {
        self.break_statement_pos(id, -1)
    }

    pub fn break_statement_pos(&mut self, id: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = BreakStatementNode::new(id);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn call_expression(&mut self, expr: Option<NodeRef>, args: Option<NodeRef>) -> NodeRef {
        self.call_expression_pos(expr, args, -1)
    }

    pub fn call_expression_pos(&mut self, expr: Option<NodeRef>, args: Option<NodeRef>, pos: i32) -> NodeRef {
        if omit_trace() && expr.is_some() {
            let is_trace = {
                let e = rb(&expr);
                match e.as_member_expression() {
                    Some(men) if men.base.is_none() && men.selector.is_some() => {
                        let sel = rb(&men.selector);
                        match sel.as_get_expression() {
                            Some(gen) if gen.sel.expr.is_some() => {
                                let ge = rb(&gen.sel.expr);
                                ge.identifier().map_or(false, |iden| iden.name == "trace")
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            };
            if is_trace {
                return self.empty_statement();
            }
        }

        let node: NodeRef;
        if is_kind(&expr, Node::is_member_expression) {
            let memb = expr.clone().unwrap();
            let get = {
                let m = memb.borrow();
                let selector = m.as_member_expression().unwrap().selector.clone();
                if is_kind(&selector, |s| s.as_get_expression().is_some()) { selector } else { None }
            };
            if let Some(get) = get {
                let (get_expr, mode, is_package) = {
                    let g = get.borrow();
                    let g = g.as_get_expression().unwrap();
                    (g.sel.expr.clone(), g.sel.get_mode(), g.sel.is_package)
                };
                let call = CallExpressionNode::new(get_expr, args);
                {
                    let mut c = call.borrow_mut();
                    let sel = c.selector_mut().unwrap();
                    sel.set_mode(mode);
                    sel.is_package = is_package;
                    c.set_position_nonterminal(expr.as_ref(), pos);
                }
                memb.borrow_mut().as_member_expression_mut().unwrap().selector = Some(call);
                node = memb;
            } else {
                let call = CallExpressionNode::new(expr.clone(), args);
                {
                    let mut c = call.borrow_mut();
                    c.selector_mut().unwrap().set_rvalue(true);
                    c.set_position_nonterminal(expr.as_ref(), pos);
                }
                node = call;
            }
        } else {
            let call = CallExpressionNode::new(expr.clone(), args);
            {
                let mut c = call.borrow_mut();
                c.selector_mut().unwrap().set_rvalue(true);
                c.set_position_nonterminal(expr.as_ref(), pos);
            }
            node = call;
        }

        if SPECIAL_FUNCTION_SYNTAX {}

        node
    }

    pub fn case_label(&mut self, label: Option<NodeRef>) -> NodeRef {
        self.case_label_pos(label, -1)
    }

    pub fn case_label_pos(&mut self, label: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = CaseLabelNode::new(label);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn catch_clause(&mut self, parameter: Option<NodeRef>, block: Option<NodeRef>) -> NodeRef {
        self.catch_clause_pos(parameter, block, -1)
    }

    pub fn catch_clause_pos(&mut self, parameter: Option<NodeRef>, block: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = CatchClauseNode::new(parameter.clone(), block);
        node.borrow_mut().set_position_nonterminal(parameter.as_ref(), pos);
        node
    }

    pub fn start_class_defs(&mut self) {
        self.clsdefs_sets.push(new_node_list());
        self.needs_prototype_ns = true;
    }

    pub fn finish_class_defs(&mut self) {
        self.clsdefs_sets.pop();
    }

    pub fn class_needs_prototype_namespace(&mut self) -> bool {
        let temp = self.needs_prototype_ns;
        self.needs_prototype_ns = false;
        temp
    }

    fn qualify_identifier(&mut self, attrs: &Option<NodeRef>, identifier: &Option<NodeRef>) -> NodeRef {
        let (name, pos) = {
            let i = rb(identifier);
            (i.identifier().expect("ClassCastException").name.clone(), i.pos())
        };
        self.qualified_identifier_pos(attrs.clone(), Some(&name), pos)
    }

    fn inheritance_parts(inheritance: &Option<NodeRef>) -> (Option<NodeRef>, Option<NodeRef>) {
        match inheritance {
            Some(i) => {
                let i = i.borrow();
                let i = i.as_inheritance().expect("ClassCastException");
                (i.baseclass.clone(), i.interfaces.clone())
            }
            None => (None, None),
        }
    }

    pub fn class_definition(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        inheritance: Option<NodeRef>,
        block: Option<NodeRef>,
    ) -> NodeRef {
        self.class_definition_non_nullable(cx, attrs, identifier, inheritance, block, false)
    }

    pub fn class_definition_non_nullable(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        inheritance: Option<NodeRef>,
        block: Option<NodeRef>,
        non_nullable: bool,
    ) -> NodeRef {
        self.class_definition_pos(cx, attrs, identifier, inheritance, block, non_nullable, -1)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn class_definition_pos(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        inheritance: Option<NodeRef>,
        block: Option<NodeRef>,
        non_nullable: bool,
        pos: i32,
    ) -> NodeRef {
        let identifier = Some(self.qualify_identifier(&attrs, &identifier));
        let (baseclass, interfaces) = Self::inheritance_parts(&inheritance);
        let node =
            ClassDefinitionNode::new(Some(cx.clone()), self.current_package.clone(), attrs, identifier.clone(), baseclass, interfaces, block);
        {
            let mut n = node.borrow_mut();
            n.as_class_definition_mut().unwrap().is_default_nullable = !non_nullable;
            n.set_position_nonterminal(identifier.as_ref(), pos);
            n.as_class_definition_mut().unwrap().clsdefs = self.clsdefs_sets.last().cloned();
        }
        self.clsdefs_sets.pop();
        if let Some(last) = self.clsdefs_sets.last() {
            last.borrow_mut().push(node.clone());
        }
        node
    }

    pub fn binary_class_definition(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        inheritance: Option<NodeRef>,
        block: Option<NodeRef>,
    ) -> NodeRef {
        self.binary_class_definition_pos(cx, attrs, identifier, inheritance, block, -1)
    }

    pub fn binary_class_definition_pos(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        inheritance: Option<NodeRef>,
        block: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let identifier = Some(self.qualify_identifier(&attrs, &identifier));
        let (baseclass, interfaces) = Self::inheritance_parts(&inheritance);
        let node =
            BinaryClassDefNode::new(Some(cx.clone()), self.current_package.clone(), attrs, identifier.clone(), baseclass, interfaces, block);
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn interface_definition(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        block: Option<NodeRef>,
    ) -> NodeRef {
        self.interface_definition_pos(cx, attrs, identifier, interfaces, block, -1)
    }

    pub fn interface_definition_pos(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        block: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let identifier = Some(self.qualify_identifier(&attrs, &identifier));
        let node = InterfaceDefinitionNode::new(Some(cx.clone()), self.current_package.clone(), attrs, identifier.clone(), interfaces, block);
        {
            let mut n = node.borrow_mut();
            n.set_position_nonterminal(identifier.as_ref(), pos);
            n.class_definition_mut().unwrap().clsdefs = Some(new_node_list());
        }
        if let Some(last) = self.clsdefs_sets.last() {
            last.borrow_mut().push(node.clone());
        }
        node
    }

    pub fn binary_interface_definition(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        block: Option<NodeRef>,
    ) -> NodeRef {
        self.binary_interface_definition_pos(cx, attrs, identifier, interfaces, block, -1)
    }

    pub fn binary_interface_definition_pos(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        identifier: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        block: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let identifier = Some(self.qualify_identifier(&attrs, &identifier));
        let node =
            BinaryInterfaceDefinitionNode::new(Some(cx.clone()), self.current_package.clone(), attrs, identifier.clone(), interfaces, block);
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn class_name(&mut self, pkgname: Option<NodeRef>, ident: Option<NodeRef>) -> NodeRef {
        self.class_name_pos(pkgname, ident, -1)
    }

    pub fn class_name_pos(&mut self, pkgname: Option<NodeRef>, ident: Option<NodeRef>, pos: i32) -> NodeRef {
        let ident_pos = rb(&ident).pos();
        let node = ClassNameNode::new(pkgname, ident.clone(), ident_pos);
        node.borrow_mut().set_position_nonterminal(ident.as_ref(), pos);
        node
    }

    pub fn conditional_expression(&mut self, cond: Option<NodeRef>, thenexpr: Option<NodeRef>, elseexpr: Option<NodeRef>) -> NodeRef {
        self.conditional_expression_pos(cond, thenexpr, elseexpr, -1)
    }

    pub fn conditional_expression_pos(
        &mut self,
        cond: Option<NodeRef>,
        thenexpr: Option<NodeRef>,
        elseexpr: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let node = ConditionalExpressionNode::new(cond.clone(), thenexpr, elseexpr);
        node.borrow_mut().set_position_nonterminal(cond.as_ref(), pos);
        node
    }

    pub fn continue_statement(&mut self, id: Option<NodeRef>) -> NodeRef {
        self.continue_statement_pos(id, -1)
    }

    pub fn continue_statement_pos(&mut self, id: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ContinueStatementNode::new(id);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    /// Wraps `stmt` in a statement list unless it already is one, as several statement factories do.
    fn ensure_statement_list(&mut self, stmt: Option<NodeRef>) -> Option<NodeRef> {
        if stmt.is_some() && rb(&stmt).as_statement_list().is_none() {
            return Some(self.statement_list(None, stmt));
        }
        stmt
    }

    pub fn do_statement(&mut self, stmt: Option<NodeRef>, expr: Option<NodeRef>) -> NodeRef {
        self.do_statement_pos(stmt, expr, -1)
    }

    pub fn do_statement_pos(&mut self, stmt: Option<NodeRef>, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let stmt = self.ensure_statement_list(stmt);
        let node = DoStatementNode::new(stmt.clone(), expr);
        node.borrow_mut().set_position_nonterminal(stmt.as_ref(), pos);
        node
    }

    pub fn empty_element(&mut self) -> NodeRef {
        self.empty_element_pos(-1)
    }

    pub fn empty_element_pos(&mut self, pos: i32) -> NodeRef {
        let node = EmptyElementNode::new();
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn empty_statement(&mut self) -> NodeRef {
        EmptyStatementNode::get_instance()
    }

    pub fn expression_statement(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.expression_statement_pos(expr, -1)
    }

    pub fn expression_statement_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ExpressionStatementNode::new(expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn for_in_statement(
        &mut self,
        is_each: bool,
        expr1: Option<NodeRef>,
        expr2: Option<NodeRef>,
        stmt: Option<NodeRef>,
    ) -> NodeRef {
        self.for_in_statement_pos(is_each, expr1, expr2, stmt, -1)
    }

    pub fn for_in_statement_pos(
        &mut self,
        is_each: bool,
        mut expr1: Option<NodeRef>,
        expr2: Option<NodeRef>,
        mut stmt: Option<NodeRef>,
        mut pos: i32,
    ) -> NodeRef {
        if !self.get_context().borrow().script_assist_parsing {
            if stmt.is_some() && !rb(&stmt).is_statement_list() {
                stmt = Some(self.statement_list(None, stmt));
            }

            let mut vard: Option<NodeRef> = None;
            if rb(&expr1).is_definition() {
                vard = expr1.clone();
                let identifier = {
                    let v = rb(&vard);
                    let v = v.as_variable_definition().expect("ClassCastException");
                    let list = rb(&v.list);
                    let varb = list.list().expect("ClassCastException").items.last().cloned().flatten();
                    let varb = rb(&varb);
                    let var = rb(&varb.as_variable_binding().expect("ClassCastException").variable);
                    var.as_typed_identifier().expect("ClassCastException").identifier.clone()
                };
                expr1 = identifier;
            } else {
                let last = rb(&expr1).list().expect("ClassCastException").items.last().cloned().flatten();
                expr1 = last;
            }

            let ndx_reg = self.register(pos);
            let obj_reg = self.register(pos);

            let untyped_expr = self.coerce_pos(expr2, None, Some(self.get_context().borrow().no_type().get_default_type_info()), true, pos);
            let zero = self.literal_number_pos("0", pos);
            let store_ndx = self.store_register(Some(ndx_reg.clone()), Some(zero), Some(self.get_context().borrow().int_type()), pos);
            let init0 = self.list(None, Some(store_ndx));
            let store_obj = self.store_register(Some(obj_reg.clone()), Some(untyped_expr), Some(self.get_context().borrow().no_type()), pos);
            let init = self.list(Some(init0), Some(store_obj));
            let test = self.has_next(Some(obj_reg.clone()), Some(ndx_reg.clone()), pos);

            let incr = if is_each {
                let load_obj = self.load_register(Some(obj_reg.clone()), Some(self.get_context().borrow().no_type()), pos);
                let load_ndx = self.load_register(Some(ndx_reg.clone()), Some(self.get_context().borrow().int_type()), pos);
                let args = self.argument_list(None, Some(load_ndx));
                let invoke = self.invoke("[[NextValue]]", Some(args), pos);
                let member = self.member_expression(Some(load_obj), Some(invoke));
                let assign = self.assignment_expression(expr1.clone(), ASSIGN_TOKEN, Some(member));
                self.expression_statement(Some(assign))
            } else {
                let load_obj = self.load_register(Some(obj_reg.clone()), Some(self.get_context().borrow().no_type()), pos);
                let load_ndx = self.load_register(Some(ndx_reg.clone()), Some(self.get_context().borrow().int_type()), pos);
                let args = self.argument_list(None, Some(load_ndx));
                let expr1_pos = rb(&expr1).pos();
                let invoke = self.invoke("[[NextName]]", Some(args), expr1_pos);
                let member = self.member_expression(Some(load_obj), Some(invoke));
                let assign = self.assignment_expression(expr1.clone(), ASSIGN_TOKEN, Some(member));
                self.expression_statement(Some(assign))
            };

            let incr_list = self.statement_list(None, Some(incr));
            stmt = Some(self.statement_list(Some(incr_list), stmt));
            let for_pos = if pos == -1 { rb(&expr1).pos() } else { pos };
            let mut node = self.for_statement_pos(Some(init), Some(test), None, stmt, true, for_pos);
            if vard.is_some() {
                let vard_list = self.statement_list(None, vard);
                let stmtlist = self.statement_list(Some(vard_list), Some(node));
                stmtlist.borrow_mut().as_statement_list_mut().unwrap().is_loop = true;
                node = stmtlist;
            }
            node
        } else {
            if stmt.is_some() && !rb(&stmt).is_statement_list() {
                stmt = Some(self.statement_list(None, stmt));
            }

            let mut vard: Option<NodeRef> = None;
            if is_kind(&expr1, Node::is_definition) {
                vard = expr1.clone();
                rb(&vard).as_variable_definition().expect("ClassCastException");
                expr1 = None;
            }

            if expr1.is_some() {
                pos = rb(&expr1).pos();
            } else if expr2.is_some() {
                pos = rb(&expr2).pos();
            }

            let mut node = ForStatementNode::new(expr1.clone(), expr2, None, stmt.clone(), true);
            node.borrow_mut().set_position_nonterminal(expr1.as_ref(), pos);
            if vard.is_some() {
                let vard_list = self.statement_list(None, vard);
                let stmtlist = self.statement_list(Some(vard_list), Some(node));
                node = stmtlist.clone();
                stmtlist.borrow_mut().as_statement_list_mut().unwrap().is_loop = true;
            } else if let Some(stmt) = &stmt {
                if let Some(stmtlist) = stmt.borrow_mut().as_statement_list_mut() {
                    stmtlist.is_loop = true;
                }
            }
            node
        }
    }

    pub fn for_statement(
        &mut self,
        expr1: Option<NodeRef>,
        expr2: Option<NodeRef>,
        expr3: Option<NodeRef>,
        stmt: Option<NodeRef>,
    ) -> NodeRef {
        self.for_statement_pos(expr1, expr2, expr3, stmt, false, -1)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn for_statement_pos(
        &mut self,
        mut expr1: Option<NodeRef>,
        expr2: Option<NodeRef>,
        expr3: Option<NodeRef>,
        mut stmt: Option<NodeRef>,
        is_forin: bool,
        pos: i32,
    ) -> NodeRef {
        if stmt.is_some() && !rb(&stmt).is_statement_list() {
            stmt = Some(self.statement_list(None, stmt));
        }

        let mut vard: Option<NodeRef> = None;
        if is_kind(&expr1, Node::is_definition) {
            vard = expr1.clone();
            rb(&vard).as_variable_definition().expect("ClassCastException");
            expr1 = None;
        }

        let mut node = ForStatementNode::new(expr1.clone(), expr2, expr3, stmt.clone(), is_forin);
        node.borrow_mut().set_position_nonterminal(expr1.as_ref(), pos);
        if vard.is_some() {
            let vard_list = self.statement_list(None, vard);
            let stmtlist = self.statement_list(Some(vard_list), Some(node));
            node = stmtlist.clone();
            stmtlist.borrow_mut().as_statement_list_mut().unwrap().is_loop = true;
        } else if let Some(stmt) = &stmt {
            if let Some(stmtlist) = stmt.borrow_mut().as_statement_list_mut() {
                stmtlist.is_loop = true;
            }
        }
        node
    }

    pub fn function_definition(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        fexpr: Option<NodeRef>,
    ) -> NodeRef {
        self.function_definition_pos(cx, attrs, name, fexpr, -1)
    }

    fn fix_up_function_name(&mut self, attrs: &Option<NodeRef>, name: &Option<NodeRef>, fexpr: &Option<NodeRef>) {
        let old = rb(name).as_function_name().expect("ClassCastException").identifier.clone();
        let qid = self.qualify_identifier(attrs, &old);
        rbm(name).as_function_name_mut().unwrap().identifier = Some(qid.clone());
        let mut f = rbm(fexpr);
        let f = f.as_function_common_mut().expect("ClassCastException");
        f.identifier = Some(qid);
        f.set_function_definition(true);
    }

    pub fn function_definition_pos(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        fexpr: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        self.fix_up_function_name(&attrs, &name, &fexpr);
        let node = FunctionDefinitionNode::new(Some(cx.clone()), self.current_package.clone(), attrs, name.clone(), fexpr);
        node.borrow_mut().set_position_nonterminal(name.as_ref(), pos);
        node
    }

    pub fn binary_function_definition(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        fexpr: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        self.fix_up_function_name(&attrs, &name, &fexpr);
        let node = BinaryFunctionDefinitionNode::new(Some(cx.clone()), self.current_package.clone(), attrs, name.clone(), fexpr);
        node.borrow_mut().set_position_nonterminal(name.as_ref(), pos);
        node
    }

    pub fn function_common(
        &mut self,
        cx: &ContextRef,
        identifier: Option<NodeRef>,
        signature: Option<NodeRef>,
        body: Option<NodeRef>,
    ) -> NodeRef {
        self.function_common_pos(cx, identifier, signature, body, -1)
    }

    pub fn function_common_pos(
        &mut self,
        cx: &ContextRef,
        mut identifier: Option<NodeRef>,
        signature: Option<NodeRef>,
        mut body: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        if identifier.is_none() {
            if cx.borrow().script_assist_parsing {
                identifier = Some(self.identifier_pos("", pos));
            } else {
                identifier = Some(self.identifier_pos("anonymous", pos));
            }
        }

        let name = rb(&identifier).identifier().expect("ClassCastException").name.clone();
        let mut internal_name = String::with_capacity(name.len() + 6);
        if !self.fun_names.contains_key(&name) {
            self.fun_names.insert(name.clone(), 0);
        }
        let mut num = self.fun_names[&name];
        if USE_DEFINED_NAME_AS_INTERNAL_NAME {
            if num == 0 {
                internal_name.push_str(&name);
                num += 1;
                self.fun_names.insert(name.clone(), num);
            } else {
                internal_name.push_str(&name);
                internal_name.push('$');
                internal_name.push_str(&num.to_string());
                num += 1;
                self.fun_names.insert(name.clone(), num);
            }
        } else {
            internal_name.push_str(&name);
            internal_name.push('$');
            internal_name.push_str(&num.to_string());
            num += 1;
            self.fun_names.insert(name.clone(), num);
        }

        let mut user_body = false;
        if cx.borrow().input.is_some() {
            let rtn;
            if cx.borrow().script_assist_parsing {
                let zero = self.literal_number_pos("0", 0);
                let void0 = self.unary_expression_pos(VOID_TOKEN, Some(zero), 0);
                rtn = self.return_statement_pos(Some(void0), 0);
                rtn.borrow_mut().set_is_synthetic();
            } else {
                rtn = self.return_statement_pos(None, 0);
                rtn.borrow_mut().set_is_synthetic();
                let mark = cx.borrow().input.as_ref().unwrap().borrow().position_of_mark();
                rtn.borrow_mut().set_position_terminal(mark);
            }

            if body.is_none() {
                body = Some(self.statement_list(None, Some(rtn)));
                user_body = false;
            } else {
                rbm(&body).as_statement_list_mut().expect("ClassCastException").items.push(Some(rtn));
                user_body = true;
            }
        }

        let fexpr = FunctionCommonNode::new(Some(cx.clone()), None, internal_name, identifier, signature.clone(), body, user_body);
        {
            let mut f = fexpr.borrow_mut();
            f.as_function_common_mut().unwrap().needs_arguments = if self.has_rest {
                METHOD_Needrest
            } else if self.has_arguments {
                METHOD_Arguments
            } else {
                0
            };
            f.set_position_nonterminal(signature.as_ref(), pos);
        }
        fexpr
    }

    pub fn function_name(&mut self, kind: i32, name: Option<NodeRef>) -> NodeRef {
        self.function_name_pos(kind, name, -1)
    }

    pub fn function_name_pos(&mut self, kind: i32, name: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = FunctionNameNode::new(kind, name.clone());
        node.borrow_mut().set_position_nonterminal(name.as_ref(), pos);
        node
    }

    pub fn function_signature(&mut self, parameter: Option<NodeRef>, result: Option<NodeRef>) -> NodeRef {
        self.function_signature_pos(parameter, result, -1)
    }

    pub fn function_signature_pos(&mut self, parameter: Option<NodeRef>, result: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = FunctionSignatureNode::new(parameter, result);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn constructor_signature(&mut self, parameter: Option<NodeRef>, initializers: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = FunctionSignatureNode::new_with_inits(parameter, initializers);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    /// Both Java overloads, `getExpression(Node)` and `getExpression(IdentifierNode)`.
    pub fn get_expression(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.get_expression_pos(expr, -1)
    }

    pub fn get_expression_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = GetExpressionNode::new(expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn identifier(&mut self, name: &str) -> NodeRef {
        self.identifier_pos(name, -1)
    }

    pub fn identifier_intern(&mut self, name: &str, intern: bool) -> NodeRef {
        self.identifier_intern_pos(name, intern, -1)
    }

    pub fn identifier_intern_pos(&mut self, name: &str, intern: bool, pos: i32) -> NodeRef {
        let node = IdentifierNode::new_intern(name, intern, pos);
        if ARGUMENTS == node.borrow().identifier().unwrap().name {
            self.has_arguments = true;
        }
        node
    }

    pub fn identifier_pos(&mut self, name: &str, pos: i32) -> NodeRef {
        let node = IdentifierNode::new(name, pos);
        node.borrow_mut().set_position_terminal(pos);
        if ARGUMENTS == node.borrow().identifier().unwrap().name {
            self.has_arguments = true;
        }
        node
    }

    /// `identifier(IdentifierNode ident, int pos)`.
    pub fn identifier_from(&mut self, ident: &NodeRef, pos: i32) -> NodeRef {
        let (qualifier, is_qualified, name, is_attr) = {
            let i = ident.borrow();
            let id = i.identifier().expect("ClassCastException");
            match i.qualified_identifier() {
                Some(q) => (q.qualifier.clone(), true, id.name.clone(), i.is_attr()),
                None => (None, false, id.name.clone(), i.is_attr()),
            }
        };
        let node = if is_qualified {
            self.qualified_identifier_pos(qualifier, Some(&name), pos)
        } else {
            self.identifier_pos(&name, pos)
        };
        node.borrow_mut().set_attr(is_attr);
        node
    }

    pub fn apply_type_expr(&mut self, expr: Option<NodeRef>, type_args: Option<NodeRef>, pos: i32) -> NodeRef {
        let node: NodeRef;
        if rb(&expr).is_member_expression() {
            let memb = expr.clone().unwrap();
            let get = {
                let m = memb.borrow();
                let selector = m.as_member_expression().unwrap().selector.clone();
                if is_kind(&selector, |s| s.as_get_expression().is_some()) { selector } else { None }
            };
            if let Some(get) = get {
                let (get_expr, mode, is_package) = {
                    let g = get.borrow();
                    let g = g.as_get_expression().unwrap();
                    (g.sel.expr.clone(), g.sel.get_mode(), g.sel.is_package)
                };
                let apply = ApplyTypeExprNode::new(get_expr, type_args);
                {
                    let mut a = apply.borrow_mut();
                    let sel = a.selector_mut().unwrap();
                    sel.set_mode(mode);
                    sel.is_package = is_package;
                    a.set_position_nonterminal(expr.as_ref(), pos);
                }
                memb.borrow_mut().as_member_expression_mut().unwrap().selector = Some(apply);
                node = memb;
            } else {
                let apply = ApplyTypeExprNode::new(expr.clone(), type_args);
                {
                    let mut a = apply.borrow_mut();
                    a.selector_mut().unwrap().set_rvalue(true);
                    a.set_position_nonterminal(expr.as_ref(), pos);
                }
                node = apply;
            }
        } else if rb(&expr).is_identifier() {
            let apply = ApplyTypeExprNode::new(expr.clone(), type_args);
            {
                let mut a = apply.borrow_mut();
                a.selector_mut().unwrap().set_rvalue(true);
                a.set_position_nonterminal(expr.as_ref(), pos);
            }
            node = apply;
        } else {
            let s = rb(&expr).to_string();
            node = self.error_arg(pos, kError_Parser_keywordInsteadOfTypeExpr, &s);
        }
        node
    }

    pub fn if_statement(&mut self, test: Option<NodeRef>, tblock: Option<NodeRef>, eblock: Option<NodeRef>) -> NodeRef {
        self.if_statement_pos(test, tblock, eblock, -1)
    }

    pub fn if_statement_pos(
        &mut self,
        test: Option<NodeRef>,
        tblock: Option<NodeRef>,
        eblock: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let mut node: Option<NodeRef> = None;
        if COMPILE_TIME_IF {}

        let tblock = self.ensure_statement_list(tblock);
        let eblock = self.ensure_statement_list(eblock);

        if node.is_none() {
            let n = IfStatementNode::new(test.clone(), tblock, eblock);
            n.borrow_mut().set_position_nonterminal(test.as_ref(), pos);
            node = Some(n);
        }
        node.unwrap()
    }

    pub fn import_directive(
        &mut self,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        init: Option<NodeRef>,
        cx: &ContextRef,
    ) -> NodeRef {
        self.import_directive_pos(attrs, name, init, -1, cx)
    }

    pub fn import_directive_pos(
        &mut self,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        _init: Option<NodeRef>,
        pos: i32,
        cx: &ContextRef,
    ) -> NodeRef {
        let mut pkg_node = None;
        let id_node = rb(&name).as_package_name().expect("ClassCastException").id.clone();
        let id = rbm(&id_node).to_identifier_string().unwrap_or_default();
        if self.pkg_names.contains_key(&id) {
            pkg_node = self.pkg_names.get(&id).cloned();
        }
        let node = ImportDirectiveNode::new(self.current_package.clone(), attrs, name.clone(), pkg_node, Some(cx.clone()));
        node.borrow_mut().set_position_nonterminal(name.as_ref(), pos);
        node
    }

    pub fn include_directive(&mut self, cx: &ContextRef, filespec: Option<NodeRef>, program: Option<NodeRef>) -> NodeRef {
        self.include_directive_pos(cx, filespec, program, -1)
    }

    pub fn include_directive_pos(
        &mut self,
        cx: &ContextRef,
        filespec: Option<NodeRef>,
        program: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let node = IncludeDirectiveNode::new(Some(cx.clone()), filespec.clone(), program);
        node.borrow_mut().set_position_nonterminal(filespec.as_ref(), pos);
        node
    }

    /// `Import(cx, filespec, program)`; the Java `cx` is unused.
    pub fn import(&mut self, filespec: Option<NodeRef>, program: Option<NodeRef>) -> NodeRef {
        self.import_pos(filespec, program, -1)
    }

    pub fn import_pos(&mut self, filespec: Option<NodeRef>, program: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ImportNode::new(filespec.clone(), program);
        node.borrow_mut().set_position_nonterminal(filespec.as_ref(), pos);
        node
    }

    pub fn increment(&mut self, op: i32, expr: Option<NodeRef>, is_postfix: bool) -> NodeRef {
        self.increment_pos(op, expr, is_postfix, -1)
    }

    pub fn increment_pos(&mut self, op: i32, expr: Option<NodeRef>, is_postfix: bool, pos: i32) -> NodeRef {
        let node = IncrementNode::new(op, expr.clone(), is_postfix);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn inheritance(&mut self, baseclass: Option<NodeRef>, interfaces: Option<NodeRef>) -> NodeRef {
        self.inheritance_pos(baseclass, interfaces, -1)
    }

    pub fn inheritance_pos(&mut self, baseclass: Option<NodeRef>, interfaces: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = InheritanceNode::new(baseclass.clone(), interfaces);
        node.borrow_mut().set_position_nonterminal(baseclass.as_ref(), pos);
        node
    }

    pub fn labeled_statement(&mut self, label: Option<NodeRef>, statement: Option<NodeRef>) -> NodeRef {
        self.labeled_statement_pos(label, statement, -1)
    }

    pub fn labeled_statement_pos(
        &mut self,
        label: Option<NodeRef>,
        statement: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let stmt_is_loop = is_kind(&statement, |s| {
            s.as_for_statement().is_some()
                || s.as_while_statement().is_some()
                || s.as_do_statement().is_some()
                || s.as_statement_list().map_or(false, |l| l.is_loop)
        });
        let is_statement_list = is_kind(&statement, |s| s.as_statement_list().is_some());
        let node = if !self.get_context().borrow().script_assist_parsing && is_statement_list {
            let t = self.literal_boolean(true);
            let test = self.list(None, Some(t));
            let if_stmt = self.if_statement(Some(test), statement, None);
            LabeledStatementNode::new(label.clone(), stmt_is_loop, Some(if_stmt))
        } else {
            let statement = if statement.is_some() { statement } else { Some(self.empty_statement()) };
            LabeledStatementNode::new(label.clone(), stmt_is_loop, statement)
        };
        node.borrow_mut().set_position_nonterminal(label.as_ref(), pos);
        node
    }

    pub fn list(&mut self, list: Option<NodeRef>, item: Option<NodeRef>) -> NodeRef {
        self.list_pos(list, item, -1)
    }

    pub fn list_pos(&mut self, list: Option<NodeRef>, mut item: Option<NodeRef>, pos: i32) -> NodeRef {
        if is_kind(&item, Node::is_identifier) {
            let get = self.get_expression(item);
            item = Some(self.member_expression(None, Some(get)));
        }

        if let Some(list) = list {
            list.borrow_mut().list_mut().expect("ClassCastException").items.push(item);
            list
        } else {
            let node = ListNode::new(None, item.clone(), 0);
            node.borrow_mut().set_position_nonterminal(item.as_ref(), pos);
            node
        }
    }

    pub fn literal_array(&mut self, elementlist: Option<NodeRef>) -> NodeRef {
        self.literal_array_pos(elementlist, -1)
    }

    pub fn literal_array_pos(&mut self, elementlist: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = LiteralArrayNode::new(elementlist.clone());
        node.borrow_mut().set_position_nonterminal(elementlist.as_ref(), pos);
        node
    }

    pub fn literal_boolean(&mut self, value: bool) -> NodeRef {
        self.literal_boolean_pos(value, -1)
    }

    pub fn literal_boolean_pos(&mut self, value: bool, pos: i32) -> NodeRef {
        let node = LiteralBooleanNode::new(value);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn literal_field(&mut self, name: Option<NodeRef>, value: Option<NodeRef>) -> NodeRef {
        self.literal_field_pos(name, value, -1)
    }

    pub fn literal_field_pos(&mut self, name: Option<NodeRef>, value: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = LiteralFieldNode::new(name.clone(), value);
        node.borrow_mut().set_position_nonterminal(name.as_ref(), pos);
        node
    }

    /// `literalNumber(int i)`.
    pub fn literal_number_int(&mut self, i: i32) -> NodeRef {
        self.literal_number_int_pos(i, -1)
    }

    pub fn literal_number_int_pos(&mut self, i: i32, pos: i32) -> NodeRef {
        self.literal_number_pos(&i.to_string(), pos)
    }

    pub fn literal_null(&mut self) -> NodeRef {
        self.literal_null_pos(-1)
    }

    pub fn literal_null_pos(&mut self, pos: i32) -> NodeRef {
        let node = LiteralNullNode::new();
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn literal_number(&mut self, value: &str) -> NodeRef {
        self.literal_number_pos(value, -1)
    }

    pub fn literal_number_pos(&mut self, value: &str, pos: i32) -> NodeRef {
        let node = LiteralNumberNode::new(value);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn literal_object(&mut self, fieldlist: Option<NodeRef>) -> NodeRef {
        self.literal_object_pos(fieldlist, -1)
    }

    pub fn literal_object_pos(&mut self, fieldlist: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = LiteralObjectNode::new(fieldlist.clone());
        node.borrow_mut().set_position_nonterminal(fieldlist.as_ref(), pos);
        node
    }

    pub fn literal_reg_exp(&mut self, value: &str, pos: i32) -> NodeRef {
        let node = LiteralRegExpNode::new(value.to_string());
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn literal_vector(&mut self, vector_type: Option<NodeRef>, initializer_list: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = LiteralVectorNode::new(initializer_list, vector_type);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn literal_xml(&mut self, list: Option<NodeRef>, is_xmllist: bool, pos: i32) -> NodeRef {
        let node = LiteralXMLNode::new(list, is_xmllist);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn literal_string(&mut self, value: &str) -> NodeRef {
        self.literal_string_pos(value, 0)
    }

    pub fn literal_string_intern(&mut self, value: &str, intern: bool) -> NodeRef {
        LiteralStringNode::new_quoted_intern(value, false, intern)
    }

    pub fn literal_string_pos(&mut self, value: &str, pos: i32) -> NodeRef {
        let node = LiteralStringNode::new(value);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    /// `literalString(String value, int pos, boolean is_single_quoted)`.
    pub fn literal_string_quoted(&mut self, value: &str, pos: i32, is_single_quoted: bool) -> NodeRef {
        let node = LiteralStringNode::new_quoted(value, is_single_quoted);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn indexed_member_expression(&mut self, base: Option<NodeRef>, selector: Option<NodeRef>) -> NodeRef {
        self.indexed_member_expression_pos(base, selector, -1)
    }

    pub fn indexed_member_expression_pos(&mut self, base: Option<NodeRef>, selector: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = self.member_expression_pos(base, selector, pos);
        let selector = node.borrow().as_member_expression().unwrap().selector.clone();
        rbm(&selector).selector_mut().expect("ClassCastException").set_mode(LEFTBRACKET_TOKEN);
        node
    }

    pub fn type_expression(&mut self, expr: Option<NodeRef>, nullable: bool, explicit: bool, pos: i32) -> NodeRef {
        let typeexpr = TypeExpressionNode::new(expr.clone(), nullable, explicit);
        typeexpr.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        typeexpr
    }

    pub fn member_expression(&mut self, base: Option<NodeRef>, selector: Option<NodeRef>) -> NodeRef {
        self.member_expression_pos(base, selector, -1)
    }

    pub fn member_expression_pos(&mut self, mut base: Option<NodeRef>, mut selector: Option<NodeRef>, pos: i32) -> NodeRef {
        if TRANSLATE_COMPOUND_NAMES && is_kind(&base, Node::is_member_expression) {
            let mut memb = base.clone().unwrap();
            let memb_selector = memb.borrow().as_member_expression().unwrap().selector.clone();
            if rb(&memb_selector).is_get_expression() {
                let mut getx = memb_selector;
                if rb(&selector).is_get_expression() {
                    let pname = rb(&selector).selector().unwrap().get_identifier();
                    let mut bname: Option<NodeRef> = None;
                    let mut base_name: Option<String> = None;
                    while getx.is_some() {
                        bname = rb(&getx).selector().unwrap().get_identifier();
                        if let Some(b) = &bname {
                            let b = b.borrow();
                            let bname_name = &b.identifier().unwrap().name;
                            base_name = Some(match &base_name {
                                None => bname_name.clone(),
                                Some(base_name) => format!("{}.{}", bname_name, base_name),
                            });
                            if let Some(q) = b.qualified_identifier() {
                                if let Some(s) = q.qualifier.as_ref().and_then(|q| q.borrow().as_literal_string().map(|s| s.value.clone())) {
                                    base_name = Some(format!("{}.{}", s, base_name.as_ref().unwrap()));
                                }
                            }
                        }
                        let memb_base = memb.borrow().as_member_expression().unwrap().base.clone();
                        if is_kind(&memb_base, |b| b.as_member_expression().is_some()) {
                            memb = memb_base.unwrap();
                            let sel = memb.borrow().as_member_expression().unwrap().selector.clone();
                            getx = if rb(&sel).is_get_expression() { sel } else { None };
                        } else {
                            getx = None;
                        }
                    }

                    if let (Some(base_name), Some(pname)) = (&base_name, &pname) {
                        let pname_name = pname.borrow().identifier().unwrap().name.clone();
                        let mut compound_name = String::with_capacity(base_name.len() + pname_name.len() + 1);
                        compound_name.push_str(base_name);
                        compound_name.push('.');
                        compound_name.push_str(&pname_name);
                        if self.compound_names.contains(&compound_name) {
                            let selector_pos = rb(&selector).pos();
                            let id = self.identifier_pos(&compound_name, selector_pos);
                            selector = Some(self.get_expression(Some(id)));
                            base = memb.borrow().as_member_expression().unwrap().base.clone();
                        } else if self.compound_names.contains(base_name) {
                            let bname_name = rb(&bname).identifier().unwrap().name.clone();
                            let memb_pos = memb.borrow().pos();
                            let nsn = self.literal_string_pos(&bname_name, memb_pos);
                            let selector_pos = rb(&selector).pos();
                            let qid = self.qualified_identifier_pos(Some(nsn), Some(&pname_name), selector_pos);
                            selector = Some(self.get_expression(Some(qid)));
                            base = memb.borrow().as_member_expression().unwrap().base.clone();
                        }
                    }
                }
            }
        }

        if is_kind(&base, Node::is_identifier) {
            let get = self.get_expression(base);
            base = Some(self.member_expression(None, Some(get)));
        }

        if base.is_none() {
            let mut s = rbm(&selector);
            let sel = s.selector_mut().expect("ClassCastException");
            sel.set_mode(EMPTY_TOKEN);
            let ident_name = sel.expr.as_ref().and_then(|e| e.borrow().identifier().map(|i| i.name.clone()));
            if let Some(name) = ident_name {
                if self.compound_names.contains(&name) {
                    sel.is_package = true;
                }
            }
        }

        let node = MemberExpressionNode::new(base, selector.clone(), 0);
        node.borrow_mut().set_position_nonterminal(selector.as_ref(), pos);
        node
    }

    pub fn namespace_definition(&mut self, attributes: Option<NodeRef>, identifier: Option<NodeRef>, value: Option<NodeRef>) -> NodeRef {
        self.namespace_definition_pos(attributes, identifier, value, -1)
    }

    pub fn namespace_definition_pos(
        &mut self,
        attributes: Option<NodeRef>,
        identifier: Option<NodeRef>,
        value: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let name = rb(&identifier).identifier().expect("ClassCastException").name.clone();
        if !self.namespace_names.contains_key(&name) {
            self.namespace_names.insert(name, 0);
        }
        let node = NamespaceDefinitionNode::new(self.current_package.clone(), attributes, identifier.clone(), value);
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn config_namespace_definition(&mut self, attributes: Option<NodeRef>, identifier: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ConfigNamespaceDefinitionNode::new(self.current_package.clone(), attributes, identifier.clone());
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn new_expression(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.new_expression_pos(expr, -1)
    }

    /// The Java `assert(false)` branches run on as with assertions disabled.
    pub fn new_expression_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node: NodeRef;
        if rb(&expr).is_member_expression() {
            let memb = expr.clone().unwrap();
            let selector = memb.borrow().as_member_expression().unwrap().selector.clone();
            if rb(&selector).is_call_expression() {
                rbm(&selector).as_call_expression_mut().unwrap().is_new = true;
                node = memb;
            } else {
                let get = if is_kind(&selector, |s| s.as_get_expression().is_some()) { selector } else { None };
                let (get_expr, mode) = {
                    let g = rb(&get);
                    let g = g.as_get_expression().unwrap();
                    (g.sel.expr.clone(), g.sel.get_mode())
                };
                let call = CallExpressionNode::new(get_expr, None);
                {
                    let mut c = call.borrow_mut();
                    c.selector_mut().unwrap().set_mode(mode);
                    c.set_position_nonterminal(expr.as_ref(), pos);
                }
                memb.borrow_mut().as_member_expression_mut().unwrap().selector = Some(call);
                node = memb;
            }
        } else if rb(&expr).is_call_expression() {
            let call = expr.clone().unwrap();
            call.borrow_mut().as_call_expression_mut().unwrap().is_new = true;
            node = call;
        } else {
            let call = CallExpressionNode::new(expr.clone(), None);
            {
                let mut c = call.borrow_mut();
                c.as_call_expression_mut().unwrap().is_new = true;
                c.selector_mut().unwrap().set_mode(EMPTY_TOKEN);
                c.set_position_nonterminal(expr.as_ref(), pos);
            }
            node = call;
        }
        node
    }

    pub fn start_package(&mut self, cx: &ContextRef, attrs: Option<NodeRef>, name: Option<NodeRef>) -> NodeRef {
        if cx.borrow().script_assist_parsing {
            let pos = cx.borrow().input.as_ref().expect("NullPointerException").borrow().position_of_next();
            self.start_package_pos(cx, attrs, name, pos)
        } else {
            self.start_package_pos(cx, attrs, name, -1)
        }
    }

    pub fn start_package_pos(
        &mut self,
        cx: &ContextRef,
        attrs: Option<NodeRef>,
        mut name: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        if name.is_none() {
            let id = self.identifier_pos("", 0);
            let ids = self.package_identifiers(None, Some(id), false);
            name = Some(self.package_name(Some(ids)));
        }
        let id_node = rb(&name).as_package_name().expect("ClassCastException").id.clone();
        let id = rbm(&id_node).to_identifier_string().unwrap_or_default();
        let node = PackageDefinitionNode::new(Some(cx.clone()), attrs, name.clone(), None);
        node.borrow_mut().set_position_nonterminal(name.as_ref(), pos);
        self.pkg_names.insert(id, node.clone());
        self.pkg_defs.push(node.clone());

        if self.current_package.is_some() {
            let parser = cx.borrow().parser.clone();
            parser.expect("NullPointerException").error(kError_NestedPackage);
        }

        self.current_package = Some(node.clone());
        node
    }

    pub fn finish_package(&mut self, cx: &ContextRef, block: Option<NodeRef>) -> Option<NodeRef> {
        self.finish_package_pos(cx, block, -1)
    }

    pub fn finish_package_pos(&mut self, _cx: &ContextRef, mut statements: Option<NodeRef>, _pos: i32) -> Option<NodeRef> {
        self.current_package.as_ref()?;
        let node = self.current_package.take().unwrap();

        if statements.is_none() {
            statements = Some(self.statement_list_concat(None, None));
        }
        {
            let mut s = rbm(&statements);
            let s = s.as_statement_list_mut().unwrap();
            s.items.insert(0, Some(node.clone()));
            s.items.push(Some(node.clone()));
        }
        node.borrow_mut().as_package_definition_mut().unwrap().statements = statements;
        self.use_stmts = None;
        Some(node)
    }

    /// `packageName(LiteralStringNode url)`.
    pub fn package_name_url(&mut self, url: Option<NodeRef>) -> NodeRef {
        self.package_name_url_pos(url, -1)
    }

    pub fn package_name_url_pos(&mut self, url: Option<NodeRef>, pos: i32) -> NodeRef {
        let url_pos = rb(&url).pos();
        let node = PackageNameNode::new_url(url.clone(), url_pos);
        node.borrow_mut().set_position_nonterminal(url.as_ref(), pos);
        node
    }

    /// `packageName(PackageIdentifiersNode id)`.
    pub fn package_name(&mut self, id: Option<NodeRef>) -> NodeRef {
        self.package_name_pos(id, -1)
    }

    pub fn package_name_pos(&mut self, id: Option<NodeRef>, pos: i32) -> NodeRef {
        let compound_name = {
            let mut i = rbm(&id);
            i.as_package_identifiers_mut().expect("ClassCastException").clear_identifier_string();
            i.to_identifier_string().unwrap_or_default()
        };
        self.compound_names.insert(compound_name);
        let id_pos = rb(&id).pos();
        let node = PackageNameNode::new(id.clone(), id_pos);
        node.borrow_mut().set_position_nonterminal(id.as_ref(), pos);
        node
    }

    pub fn package_identifiers(&mut self, node: Option<NodeRef>, item: Option<NodeRef>, is_definition: bool) -> NodeRef {
        self.package_identifiers_pos(node, item, is_definition, -1)
    }

    pub fn package_identifiers_pos(&mut self, node: Option<NodeRef>, item: Option<NodeRef>, is_definition: bool, pos: i32) -> NodeRef {
        if let Some(node) = node {
            {
                let mut n = node.borrow_mut();
                let ids = n.as_package_identifiers_mut().expect("ClassCastException");
                ids.list.push(item);
                ids.clear_identifier_string();
                let _compound_name = n.to_identifier_string();
            }
            node
        } else {
            let node = PackageIdentifiersNode::new(item, 0, is_definition);
            {
                let mut n = node.borrow_mut();
                let self_pos = n.pos();
                n.set_position_nonterminal_from(Some(self_pos), pos);
            }
            node
        }
    }

    pub fn parameter(&mut self, kind: i32, identifier: Option<NodeRef>, type_: Option<NodeRef>) -> NodeRef {
        self.parameter_pos(kind, identifier, type_, -1)
    }

    pub fn parameter_pos(&mut self, kind: i32, identifier: Option<NodeRef>, type_: Option<NodeRef>, pos: i32) -> NodeRef {
        self.parameter_init_pos(kind, identifier, type_, None, pos)
    }

    /// `parameter(int kind, IdentifierNode identifier, Node type, Node init)`.
    pub fn parameter_init(&mut self, kind: i32, identifier: Option<NodeRef>, type_: Option<NodeRef>, init: Option<NodeRef>) -> NodeRef {
        self.parameter_init_pos(kind, identifier, type_, init, -1)
    }

    pub fn parameter_init_pos(
        &mut self,
        kind: i32,
        identifier: Option<NodeRef>,
        type_: Option<NodeRef>,
        init: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let node = ParameterNode::new(kind, identifier.clone(), type_, init);
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn parameter_list(&mut self, list: Option<NodeRef>, item: Option<NodeRef>) -> NodeRef {
        self.parameter_list_pos(list, item, -1)
    }

    pub fn parameter_list_pos(&mut self, list: Option<NodeRef>, item: Option<NodeRef>, pos: i32) -> NodeRef {
        if let Some(list) = list {
            list.borrow_mut().as_parameter_list_mut().expect("ClassCastException").items.push(item);
            list
        } else {
            let node = ParameterListNode::new(None, item.clone(), 0);
            node.borrow_mut().set_position_nonterminal(item.as_ref(), pos);
            node
        }
    }

    pub fn paren_expression(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.paren_expression_pos(expr, -1)
    }

    pub fn paren_expression_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ParenExpressionNode::new(expr.clone(), 0);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn paren_list_expression(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.paren_list_expression_pos(expr, -1)
    }

    pub fn paren_list_expression_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ParenListExpressionNode::new(expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn postfix_expression(&mut self, op: i32, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node: NodeRef;
        let mut member: Option<NodeRef> = None;
        if rb(&expr).is_list() {
            let last = rb(&expr).list().unwrap().items.last().cloned().flatten();
            if rb(&last).is_member_expression() {
                member = last;
            }
        } else if rb(&expr).is_member_expression() {
            member = expr.clone();
        }

        let get = member.as_ref().and_then(|m| {
            let selector = m.borrow().as_member_expression().unwrap().selector.clone();
            if rb(&selector).is_get_expression() { selector } else { None }
        });
        if let (Some(member), Some(get)) = (&member, get) {
            let (get_expr, mode) = {
                let g = get.borrow();
                let g = g.as_get_expression().unwrap();
                (g.sel.expr.clone(), g.sel.get_mode())
            };
            let inc = IncrementNode::new(op, get_expr, true);
            {
                let mut i = inc.borrow_mut();
                i.selector_mut().unwrap().set_mode(mode);
                i.set_position_terminal(pos);
            }
            let (base, is_indexed) = {
                let mut m = member.borrow_mut();
                let m = m.as_member_expression_mut().unwrap();
                m.selector = Some(inc.clone());
                (m.base.clone(), m.is_indexed_member_expression())
            };
            if is_indexed {
                node = self.indexed_member_expression(base, Some(inc));
            } else {
                node = self.member_expression(base, Some(inc));
            }
        } else {
            let expr_position = rb(&expr).get_position();
            node = self.error(expr_position, kError_IncrementOperatorNotAReference);
            let p = node.borrow().get_position();
            node.borrow_mut().set_position(p);
        }
        node
    }

    pub fn pragma_expression(&mut self, identifier: Option<NodeRef>, arg: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = PragmaExpressionNode::new(identifier, arg);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn pragma(&mut self, list: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = PragmaNode::new(list);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn program(&mut self, cx: &ContextRef, statements: Option<NodeRef>) -> NodeRef {
        self.program_pos(cx, statements, -1)
    }

    pub fn program_pos(&mut self, cx: &ContextRef, mut statements: Option<NodeRef>, pos: i32) -> NodeRef {
        if statements.is_none() {
            let s = self.statement_list_concat(None, None);
            s.borrow_mut().as_statement_list_mut().unwrap().was_empty = true;
            statements = Some(s);
        }
        let node = ProgramNode::new(Some(cx.clone()), statements.clone());
        {
            let mut n = node.borrow_mut();
            n.set_position_nonterminal(statements.as_ref(), pos);
            let p = n.as_program_mut().unwrap();
            p.pkgdefs.extend(self.pkg_defs.iter().cloned());
            p.has_unnamed_package = self.has_unnamed_package;
        }
        self.use_stmts = None;
        node
    }

    pub fn binary_program(&mut self, cx: &ContextRef, statements: Option<NodeRef>) -> NodeRef {
        self.binary_program_pos(cx, statements, -1)
    }

    pub fn binary_program_pos(&mut self, cx: &ContextRef, mut statements: Option<NodeRef>, pos: i32) -> NodeRef {
        if statements.is_none() {
            let s = self.statement_list_concat(None, None);
            s.borrow_mut().as_statement_list_mut().unwrap().was_empty = true;
            statements = Some(s);
        }
        let node = BinaryProgramNode::new(Some(cx.clone()), statements.clone());
        {
            let mut n = node.borrow_mut();
            n.set_position_nonterminal(statements.as_ref(), pos);
            let p = n.program_mut().unwrap();
            p.pkgdefs.extend(self.pkg_defs.iter().cloned());
            p.has_unnamed_package = self.has_unnamed_package;
        }
        node
    }

    pub fn qualified_identifier(&mut self, qualifier: Option<NodeRef>, name: Option<&str>) -> NodeRef {
        self.qualified_identifier_pos(qualifier, name, -1)
    }

    pub fn qualified_identifier_pos(&mut self, qualifier: Option<NodeRef>, name: Option<&str>, pos: i32) -> NodeRef {
        let node = QualifiedIdentifierNode::new(qualifier, name, pos);
        node.borrow_mut().set_position_terminal(pos);
        if name == Some(ARGUMENTS) {
            self.has_arguments = true;
        }
        node
    }

    pub fn qualified_expression(&mut self, qualifier: Option<NodeRef>, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let expr_pos = rb(&expr).pos();
        let node = QualifiedExpressionNode::new(qualifier, expr.clone(), expr_pos);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn rest_parameter(&mut self, expr: Option<NodeRef>) -> Option<NodeRef> {
        self.rest_parameter_pos(expr, -1)
    }

    pub fn rest_parameter_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> Option<NodeRef> {
        self.has_rest = true;
        let mut node = None;
        match &expr {
            None => self.get_context().borrow().error(pos - 1, kError_RestParameterNotNamed),
            Some(e) => {
                let n = RestParameterNode::new(e.clone());
                n.borrow_mut().set_position_nonterminal(Some(e), pos);
                node = Some(n);
            }
        }
        node
    }

    pub fn rest_expression(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.rest_expression_pos(expr, -1)
    }

    pub fn rest_expression_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = RestExpressionNode::new(expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn return_statement(&mut self, expr: Option<NodeRef>) -> NodeRef {
        self.return_statement_pos(expr, -1)
    }

    pub fn return_statement_pos(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ReturnStatementNode::new(expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    /// `setExpression(Node expr, ...)`.
    pub fn set_expression(&mut self, expr: Option<NodeRef>, args: Option<NodeRef>, is_constinit: bool) -> NodeRef {
        self.set_expression_pos(expr, args, is_constinit, -1)
    }

    pub fn set_expression_pos(&mut self, expr: Option<NodeRef>, args: Option<NodeRef>, is_constinit: bool, pos: i32) -> NodeRef {
        let node = SetExpressionNode::new(expr.clone(), args);
        {
            let mut n = node.borrow_mut();
            n.set_position_nonterminal(expr.as_ref(), pos);
            n.as_set_expression_mut().unwrap().is_constinit = is_constinit;
        }
        node
    }

    /// `setExpression(GetExpressionNode get, ...)`.
    pub fn set_expression_get(&mut self, get: Option<NodeRef>, args: Option<NodeRef>, is_constinit: bool) -> NodeRef {
        self.set_expression_get_pos(get, args, is_constinit, -1)
    }

    pub fn set_expression_get_pos(
        &mut self,
        get: Option<NodeRef>,
        args: Option<NodeRef>,
        is_constinit: bool,
        pos: i32,
    ) -> NodeRef {
        let (has_identifier, get_expr) = {
            let g = rb(&get);
            let sel = g.selector().expect("ClassCastException");
            (sel.get_identifier().is_some(), sel.expr.clone())
        };
        if has_identifier {
            self.set_expression_pos(get_expr, args, is_constinit, pos)
        } else if rb(&get_expr).has_side_effect() {
            let reg = self.register(-1);
            let store = self.store_register(Some(reg.clone()), get_expr, Some(self.get_context().borrow().no_type()), -1);
            let first = self.list(None, Some(store));
            let load = self.load_register(Some(reg.clone()), Some(self.get_context().borrow().no_type()), -1);
            let expr = self.list(Some(first), Some(load));
            let load = self.load_register(Some(reg), Some(self.get_context().borrow().no_type()), -1);
            rbm(&get).selector_mut().unwrap().expr = Some(load);
            self.set_expression_pos(Some(expr), args, is_constinit, pos)
        } else {
            self.set_expression_pos(get_expr, args, is_constinit, pos)
        }
    }

    pub fn statement_list(&mut self, list: Option<NodeRef>, item: Option<NodeRef>) -> NodeRef {
        self.statement_list_pos(list, item, -1)
    }

    pub fn statement_list_pos(&mut self, list: Option<NodeRef>, item: Option<NodeRef>, pos: i32) -> NodeRef {
        let item_is = |f: fn(&Node) -> bool| is_kind(&item, f);
        let node: NodeRef;
        if let Some(list) = list {
            if item_is(|n| n.as_meta_data().is_some()) {
                let items = list.borrow().as_statement_list().expect("ClassCastException").items.clone();
                for n in items.iter().rev() {
                    if is_kind(n, |n| n.as_doc_comment().is_some()) {
                        let mut d = rbm(n);
                        let doc = d.as_doc_comment_mut().unwrap();
                        if doc.meta_data.is_none() {
                            doc.meta_data = item.clone();
                        }
                        break;
                    } else if is_kind(n, |n| n.meta_data().is_some() || n.definition().is_some()) {
                        break;
                    }
                }
            }

            let mut has_doc = false;
            if item_is(|n| n.definition().is_some() && n.as_include_directive().is_none()) {
                let items = list.borrow().as_statement_list().unwrap().items.clone();
                for n in items.iter().rev() {
                    if is_kind(n, |n| n.meta_data().is_some()) {
                        let def_is_null = rb(n).meta_data().unwrap().def.is_none();
                        if def_is_null {
                            rbm(n).meta_data_mut().unwrap().def = item.clone();
                            rbm(&item).definition_mut().unwrap().add_meta_data_node(n.clone());
                            if let Some(doc) = rb(n).as_doc_comment() {
                                if doc.meta_data.is_none() {
                                    has_doc = true;
                                }
                            }
                        }
                    } else if !is_kind(n, |n| n.as_include_directive().is_some()) {
                        break;
                    }
                }
                if !has_doc && self.create_default_doc_comments {
                    let data = self.literal_array(None);
                    let dcn = self.doc_comment(Some(data), pos);
                    {
                        let mut d = dcn.borrow_mut();
                        let d = d.as_doc_comment_mut().unwrap();
                        d.meta.def = item.clone();
                        d.is_default = true;
                    }
                    let meta_data = rb(&item).definition().unwrap().meta_data.clone();
                    if meta_data.is_none() {
                        rbm(&item).definition_mut().unwrap().add_meta_data_node(Some(dcn.clone()));
                    } else {
                        rbm(&meta_data).as_statement_list_mut().unwrap().items.insert(0, Some(dcn.clone()));
                    }
                    list.borrow_mut().as_statement_list_mut().unwrap().items.push(Some(dcn));
                }
            }

            if !self.get_context().borrow().script_assist_parsing && item_is(|n| n.as_include_directive().is_some()) {
                let included = Self::included_statements(&item);
                let mut l = list.borrow_mut();
                let l = l.as_statement_list_mut().unwrap();
                l.items.push(item.clone());
                l.items.extend(included);
                l.items.push(item.clone());
            } else {
                list.borrow_mut().as_statement_list_mut().expect("ClassCastException").items.push(item.clone());
            }
            node = list;
        } else {
            let item_pos = if item.is_some() { rb(&item).pos() } else { -1 };
            if item_is(|n| n.definition().is_some()) && self.create_default_doc_comments {
                let data = self.literal_array(None);
                let dcn = self.doc_comment(Some(data), pos);
                {
                    let mut d = dcn.borrow_mut();
                    let d = d.as_doc_comment_mut().unwrap();
                    d.meta.def = item.clone();
                    d.is_default = true;
                }
                rbm(&item).definition_mut().unwrap().add_meta_data_node(Some(dcn.clone()));
                node = StatementListNode::new(Some(dcn));
                let mut n = node.borrow_mut();
                n.set_position_nonterminal(item.as_ref(), item_pos);
                n.as_statement_list_mut().unwrap().items.push(item.clone());
            } else {
                node = StatementListNode::new(item.clone());
                node.borrow_mut().set_position_nonterminal(item.as_ref(), item_pos);
            }

            if !self.get_context().borrow().script_assist_parsing && item_is(|n| n.as_include_directive().is_some()) {
                let included = Self::included_statements(&item);
                let mut n = node.borrow_mut();
                let n = n.as_statement_list_mut().unwrap();
                n.items.extend(included);
                n.items.push(item.clone());
            }
        }
        node
    }

    fn included_statements(idn: &Option<NodeRef>) -> Vec<Option<NodeRef>> {
        let program = rb(idn).as_include_directive().unwrap().program.clone();
        let statements = rb(&program).program().expect("ClassCastException").statements.clone();
        let items = rb(&statements).as_statement_list().expect("ClassCastException").items.clone();
        items
    }

    /// `statementList(StatementListNode first, StatementListNode second)`.
    pub fn statement_list_concat(&mut self, first: Option<NodeRef>, second: Option<NodeRef>) -> NodeRef {
        let first = match first {
            Some(first) => first,
            None => {
                let first = StatementListNode::new(None);
                let second_pos = if second.is_some() { rb(&second).pos() } else { -1 };
                first.borrow_mut().set_position_nonterminal(second.as_ref(), second_pos);
                first
            }
        };
        if second.is_some() {
            let items = rb(&second).as_statement_list().expect("ClassCastException").items.clone();
            first.borrow_mut().as_statement_list_mut().expect("ClassCastException").items.extend(items);
        }
        first
    }

    pub fn super_expression(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = SuperExpressionNode::new(expr);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn super_statement(&mut self, call: Option<NodeRef>) -> NodeRef {
        self.super_statement_pos(call, -1)
    }

    pub fn super_statement_pos(&mut self, call: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = SuperStatementNode::new(call.clone());
        node.borrow_mut().set_position_nonterminal(call.as_ref(), pos);
        node
    }

    pub fn switch_statement(&mut self, expr: Option<NodeRef>, statements: Option<NodeRef>) -> NodeRef {
        self.switch_statement_pos(expr, statements, -1)
    }

    pub fn switch_statement_pos(&mut self, expr: Option<NodeRef>, statements: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = SwitchStatementNode::new(expr.clone(), statements);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn this_expression(&mut self, pos: i32) -> NodeRef {
        let node = ThisExpressionNode::new();
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn throw_statement(&mut self, list: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ThrowStatementNode::new(list);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn try_statement(
        &mut self,
        tryblock: Option<NodeRef>,
        catchlist: Option<NodeRef>,
        finallyblock: Option<NodeRef>,
    ) -> NodeRef {
        self.try_statement_pos(tryblock, catchlist, finallyblock, -1)
    }

    pub fn try_statement_pos(
        &mut self,
        mut tryblock: Option<NodeRef>,
        catchlist: Option<NodeRef>,
        finallyblock: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        if tryblock.is_none() {
            let empty = self.empty_statement();
            tryblock = Some(self.statement_list(None, Some(empty)));
        }
        let node = TryStatementNode::new(tryblock.clone(), catchlist, finallyblock);
        node.borrow_mut().set_position_nonterminal(tryblock.as_ref(), pos);
        node
    }

    pub fn finally_clause(&mut self, block: Option<NodeRef>) -> NodeRef {
        self.finally_clause_pos(block, -1)
    }

    pub fn finally_clause_pos(&mut self, mut block: Option<NodeRef>, pos: i32) -> NodeRef {
        if block.is_none() {
            let empty = self.empty_statement();
            block = Some(self.statement_list(None, Some(empty)));
        }
        let throw = self.throw_statement(None, 0);
        let throw_list = self.statement_list(None, Some(throw));
        let catch_clause = self.catch_clause(None, Some(throw_list));
        let node = FinallyClauseNode::new(block.clone(), Some(catch_clause));
        node.borrow_mut().set_position_nonterminal(block.as_ref(), pos);
        node
    }

    pub fn typed_identifier(&mut self, identifier: Option<NodeRef>, type_: Option<NodeRef>) -> NodeRef {
        self.typed_identifier_pos(identifier, type_, -1)
    }

    pub fn typed_identifier_pos(&mut self, identifier: Option<NodeRef>, type_: Option<NodeRef>, pos: i32) -> NodeRef {
        let node_pos = if type_.is_some() { rb(&type_).pos() } else { rb(&identifier).pos() };
        let node = TypedIdentifierNode::new(identifier.clone(), type_, node_pos);
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn unary_expression(&mut self, op: i32, expr: Option<NodeRef>) -> NodeRef {
        self.unary_expression_pos(op, expr, -1)
    }

    pub fn unary_expression_pos(&mut self, op: i32, mut expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node: NodeRef;
        if op == DELETE_TOKEN {
            let mut member: Option<NodeRef> = None;
            if rb(&expr).is_member_expression() {
                member = expr.clone();
            } else if rb(&expr).is_list() {
                let item = rb(&expr).list().unwrap().items.last().cloned().flatten();
                if is_kind(&item, |i| i.as_member_expression().is_some()) {
                    member = item;
                }
            }

            let get = member.as_ref().and_then(|m| {
                let selector = m.borrow().as_member_expression().unwrap().selector.clone();
                if rb(&selector).is_get_expression() { selector } else { None }
            });
            if let (Some(member), Some(get)) = (&member, get) {
                let (get_expr, mode) = {
                    let g = get.borrow();
                    let g = g.as_get_expression().unwrap();
                    (g.sel.expr.clone(), g.sel.get_mode())
                };
                let del = DeleteExpressionNode::new(op, get_expr);
                {
                    let mut d = del.borrow_mut();
                    d.selector_mut().unwrap().set_mode(mode);
                    d.set_position_nonterminal(expr.as_ref(), pos);
                }
                let (base, is_indexed) = {
                    let mut m = member.borrow_mut();
                    let m = m.as_member_expression_mut().unwrap();
                    m.selector = Some(del.clone());
                    (m.base.clone(), m.is_indexed_member_expression())
                };
                if is_indexed {
                    node = self.indexed_member_expression(base, Some(del));
                } else {
                    node = self.member_expression(base, Some(del));
                }
            } else {
                node = DeleteExpressionNode::new(op, expr.clone());
                node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
            }
        } else if op == PLUSPLUS_TOKEN || op == MINUSMINUS_TOKEN {
            let mut member: Option<NodeRef> = None;
            while rb(&expr).is_list() {
                let inner = {
                    let e = rb(&expr);
                    let list = e.list().unwrap();
                    if list.size() != 1 {
                        None
                    } else {
                        Some(list.items[0].clone())
                    }
                };
                match inner {
                    None => {
                        if op == PLUSPLUS_TOKEN {
                            return self.error(kError_InvalidIncrementOperand, pos);
                        } else {
                            return self.error(kError_InvalidDecrementOperand, pos);
                        }
                    }
                    Some(inner) => expr = inner,
                }
            }

            if rb(&expr).is_member_expression() {
                member = expr.clone();
            }

            let get = member.as_ref().and_then(|m| {
                let selector = m.borrow().as_member_expression().unwrap().selector.clone();
                if rb(&selector).is_get_expression() { selector } else { None }
            });
            if let (Some(member), Some(get)) = (&member, get) {
                let get_expr = get.borrow().as_get_expression().unwrap().sel.expr.clone();
                let inc = IncrementNode::new(op, get_expr, false);
                inc.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
                let (base, is_indexed) = {
                    let mut m = member.borrow_mut();
                    let m = m.as_member_expression_mut().unwrap();
                    m.selector = Some(inc.clone());
                    (m.base.clone(), m.is_indexed_member_expression())
                };
                if is_indexed {
                    node = self.indexed_member_expression(base, Some(inc));
                } else {
                    node = self.member_expression(base, Some(inc));
                }
            } else {
                node = self.error(pos, kError_IncrementOperatorNotAReference);
                node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
            }
        } else if rb(&expr).is_literal_number() && (op == PLUS_TOKEN || op == MINUS_TOKEN) {
            let numb = expr.clone().unwrap();
            {
                let mut n = numb.borrow_mut();
                let n = n.as_literal_number_mut().unwrap();
                if op == MINUS_TOKEN {
                    n.negate();
                } else if self.get_context().borrow().script_assist_parsing {
                    n.value = format!("+{}", n.value);
                }
            }
            node = numb;
        } else {
            node = UnaryExpressionNode::new(op, expr.clone());
            node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        }
        node
    }

    pub fn use_directive(&mut self, attrs: Option<NodeRef>, expr: Option<NodeRef>) -> NodeRef {
        self.use_directive_pos(attrs, expr, -1)
    }

    pub fn use_directive_pos(&mut self, attrs: Option<NodeRef>, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = UseDirectiveNode::new(self.current_package.clone(), attrs, expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        let use_stmts = self.use_stmts.take();
        self.use_stmts = Some(self.statement_list(use_stmts, Some(node.clone())));
        node
    }

    pub fn use_pragma(&mut self, id: Option<NodeRef>, argument: Option<NodeRef>) -> NodeRef {
        self.use_pragma_pos(id, argument, -1)
    }

    pub fn use_pragma_pos(&mut self, id: Option<NodeRef>, argument: Option<NodeRef>, pos: i32) -> NodeRef {
        let mut node: Option<NodeRef> = None;
        if is_kind(&id, Node::is_identifier) && self.get_context().borrow().statics.borrow().es4_numerics {
            let idval = rbm(&id).to_identifier_string().unwrap_or_default();
            if idval == "precision" {
                node = Some(UsePrecisionNode::new(id.clone(), argument.clone()));
            } else if idval == "rounding" {
                node = Some(UseRoundingNode::new(id.clone(), argument.clone()));
            } else if idval == "decimal" {
                node = Some(UseNumericNode::new(id.clone(), argument.clone(), NumberUsage::use_decimal));
            } else if idval == "double" {
                node = Some(UseNumericNode::new(id.clone(), argument.clone(), NumberUsage::use_double));
            } else if idval == "int" {
                node = Some(UseNumericNode::new(id.clone(), argument.clone(), NumberUsage::use_int));
            } else if idval == "uint" {
                node = Some(UseNumericNode::new(id.clone(), argument.clone(), NumberUsage::use_uint));
            } else if idval == "Number" {
                node = Some(UseNumericNode::new(id.clone(), argument.clone(), NumberUsage::use_Number));
            }
        }
        let node = match node {
            Some(node) => node,
            None => UsePragmaNode::new(id, argument),
        };
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn variable_binding(
        &mut self,
        attrs: Option<NodeRef>,
        kind: i32,
        identifier: Option<NodeRef>,
        initializer: Option<NodeRef>,
    ) -> NodeRef {
        self.variable_binding_pos(attrs, kind, identifier, initializer, -1)
    }

    pub fn variable_binding_pos(
        &mut self,
        attrs: Option<NodeRef>,
        kind: i32,
        identifier: Option<NodeRef>,
        initializer: Option<NodeRef>,
        pos: i32,
    ) -> NodeRef {
        let old = rb(&identifier).as_typed_identifier().expect("ClassCastException").identifier.clone();
        let qid = self.qualify_identifier(&attrs, &old);
        rbm(&identifier).as_typed_identifier_mut().unwrap().identifier = Some(qid);
        let node = VariableBindingNode::new(self.current_package.clone(), attrs, kind, identifier.clone(), initializer);
        node.borrow_mut().set_position_nonterminal(identifier.as_ref(), pos);
        node
    }

    pub fn variable_definition(&mut self, attrs: Option<NodeRef>, kind: i32, list: Option<NodeRef>) -> NodeRef {
        self.variable_definition_pos(attrs, kind, list, -1)
    }

    pub fn variable_definition_pos(&mut self, attrs: Option<NodeRef>, kind: i32, list: Option<NodeRef>, pos: i32) -> NodeRef {
        let list_pos = rb(&list).pos();
        let node = VariableDefinitionNode::new(self.current_package.clone(), attrs, kind, list.clone(), list_pos);
        node.borrow_mut().set_position_nonterminal(list.as_ref(), pos);
        node
    }

    pub fn with_statement(&mut self, expr: Option<NodeRef>, stmt: Option<NodeRef>) -> NodeRef {
        self.with_statement_pos(expr, stmt, -1)
    }

    pub fn with_statement_pos(&mut self, expr: Option<NodeRef>, stmt: Option<NodeRef>, pos: i32) -> NodeRef {
        let stmt = self.ensure_statement_list(stmt);
        let node = WithStatementNode::new(expr.clone(), stmt);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn while_statement(&mut self, expr: Option<NodeRef>, stmt: Option<NodeRef>) -> NodeRef {
        self.while_statement_pos(expr, stmt, -1)
    }

    pub fn while_statement_pos(&mut self, expr: Option<NodeRef>, stmt: Option<NodeRef>, pos: i32) -> NodeRef {
        let stmt = self.ensure_statement_list(stmt);
        let node = WhileStatementNode::new(expr.clone(), stmt);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn invoke(&mut self, name: &str, args: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = InvokeNode::new(name, args);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    /// `error(int errCode)`.
    pub fn error_code(&mut self, err_code: i32) -> NodeRef {
        self.error(-1, err_code)
    }

    /// `error(int pos, int errCode, String arg)`.
    pub fn error_arg(&mut self, pos: i32, err_code: i32, arg: &str) -> NodeRef {
        let node = ErrorNode::new(err_code, arg.to_string());
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn error(&mut self, pos: i32, err_code: i32) -> NodeRef {
        let node = ErrorNode::new(err_code, String::new());
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn has_next(&mut self, object_register: Option<NodeRef>, index_register: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = HasNextNode::new(object_register, index_register);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn load_register(&mut self, reg: Option<NodeRef>, type_: Option<TypeValue>, pos: i32) -> NodeRef {
        let node = LoadRegisterNode::new(reg, type_);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn store_register(&mut self, reg: Option<NodeRef>, expr: Option<NodeRef>, type_: Option<TypeValue>, pos: i32) -> NodeRef {
        let node = StoreRegisterNode::new(reg, expr.clone(), type_);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn to_object(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = ToObjectNode::new(expr.clone());
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    /// `box(expr, actual)`.
    pub fn box_(&mut self, expr: Option<NodeRef>, actual: Option<TypeValue>) -> NodeRef {
        self.box_pos(expr, actual, -1)
    }

    pub fn box_pos(&mut self, expr: Option<NodeRef>, actual: Option<TypeValue>, pos: i32) -> NodeRef {
        let node = BoxNode::new(expr.clone(), actual);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn coerce(&mut self, expr: Option<NodeRef>, actual: Option<TypeInfo>, expected: Option<TypeInfo>, is_explicit: bool) -> NodeRef {
        self.coerce_pos(expr, actual, expected, is_explicit, -1)
    }

    pub fn coerce_pos(
        &mut self,
        expr: Option<NodeRef>,
        actual: Option<TypeInfo>,
        expected: Option<TypeInfo>,
        is_explicit: bool,
        pos: i32,
    ) -> NodeRef {
        if let Some(e) = &expr {
            let redundant = match e.borrow().as_coerce() {
                Some(cn) => {
                    same_type_info(&cn.actual, &actual)
                        && same_type_info(&cn.expected, &expected)
                        && (cn.is_explicit || !is_explicit)
                }
                None => false,
            };
            if redundant {
                return e.clone();
            }
        }
        let node = CoerceNode::new(expr.clone(), actual, expected, is_explicit);
        node.borrow_mut().set_position_nonterminal(expr.as_ref(), pos);
        node
    }

    pub fn meta_data(&mut self, data: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = MetaDataNode::new(data);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn doc_comment(&mut self, data: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = DocCommentNode::new(data);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn comment(&mut self, val: &str, ctype: i32, pos: i32) -> NodeRef {
        let node = CommentNode::new(val.to_string(), ctype);
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn default_xml_namespace(&mut self, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let node = DefaultXMLNamespaceNode::new(expr);
        node.borrow_mut().set_position_terminal(pos);
        self.dxns = Some(node.clone());
        self.has_dxns = true;
        node
    }

    pub fn register(&mut self, pos: i32) -> NodeRef {
        let node = RegisterNode::new();
        node.borrow_mut().set_position_terminal(pos);
        node
    }

    pub fn filter_operator(&mut self, expr1: Option<NodeRef>, expr2: Option<NodeRef>, pos: i32) -> NodeRef {
        if self.get_context().borrow().script_assist_parsing {
            let get = self.get_expression(expr2);
            return self.member_expression(expr1, Some(get));
        }

        let var_reg = self.register(pos);
        let tmp_reg = self.register(pos);
        let ndx_reg = self.register(pos);
        let obj_reg = self.register(pos);

        let check_args = self.argument_list(None, expr1);
        let check_filter = self.invoke("[[CheckFilterOperand]]", Some(check_args), pos);
        let untyped_expr = self.coerce_pos(Some(check_filter), None, Some(self.get_context().borrow().no_type().get_default_type_info()), true, pos);

        let zero = self.literal_number_pos("0", pos);
        let store_ndx = self.store_register(Some(ndx_reg.clone()), Some(zero), Some(self.get_context().borrow().int_type()), pos);
        let init0 = self.list(None, Some(store_ndx));
        let store_obj = self.store_register(Some(obj_reg.clone()), Some(untyped_expr), Some(self.get_context().borrow().no_type()), pos);
        let init1 = self.list(Some(init0), Some(store_obj));
        let empty = self.literal_string_pos("", pos);
        let xml_list = self.list(None, Some(empty));
        let xml = self.literal_xml(Some(xml_list), true, pos);
        let store_tmp = self.store_register(Some(tmp_reg.clone()), Some(xml), Some(self.get_context().borrow().no_type()), pos);
        let init = self.list(Some(init1), Some(store_tmp));

        let test = self.has_next(Some(obj_reg.clone()), Some(ndx_reg.clone()), pos);

        let load_obj = self.load_register(Some(obj_reg), Some(self.get_context().borrow().no_type()), pos);
        let load_ndx = self.load_register(Some(ndx_reg.clone()), Some(self.get_context().borrow().int_type()), pos);
        let next_args = self.argument_list(None, Some(load_ndx));
        let next_value = self.invoke("[[NextValue]]", Some(next_args), pos);
        let member = self.member_expression(Some(load_obj), Some(next_value));
        let store_var = self.store_register(Some(var_reg.clone()), Some(member), Some(self.get_context().borrow().no_type()), pos);
        let incr = self.expression_statement(Some(store_var));
        incr.borrow_mut().as_expression_statement_mut().unwrap().set_is_var_statement(true);

        let load_ndx = self.load_register(Some(ndx_reg), Some(self.get_context().borrow().int_type()), pos);
        let load_var = self.load_register(Some(var_reg.clone()), Some(self.get_context().borrow().no_type()), pos);
        let set_args = self.argument_list(None, Some(load_var));
        let setx = self.set_expression(Some(load_ndx), Some(set_args), false);
        setx.borrow_mut().selector_mut().unwrap().set_mode(EMPTY_TOKEN);

        let load_tmp = self.load_register(Some(tmp_reg.clone()), Some(self.get_context().borrow().no_type()), pos);
        let tmp_member = self.member_expression_pos(Some(load_tmp), Some(setx), pos);
        let tmp_stmt = self.expression_statement_pos(Some(tmp_member), pos);
        let stmt2 = self.statement_list_pos(None, Some(tmp_stmt), pos);

        let mut stmt = self.statement_list(None, Some(incr));
        let load_var = self.load_register(Some(var_reg), Some(self.get_context().borrow().no_type()), pos);
        let filter_test = self.list(None, expr2);
        let if_stmt = self.if_statement(Some(filter_test), Some(stmt2), None);
        let with_stmt = self.with_statement(Some(load_var), Some(if_stmt));
        stmt = self.statement_list(Some(stmt), Some(with_stmt));

        let node = self.for_statement_pos(Some(init), Some(test), None, Some(stmt), false, pos);
        let tmp_alloc = self.statement_list_pos(None, Some(tmp_reg.clone()), pos);
        let with_loop = self.statement_list(Some(tmp_alloc), Some(node));
        let load_tmp = self.load_register(Some(tmp_reg), Some(self.get_context().borrow().no_type()), pos);
        self.statement_list(Some(with_loop), Some(load_tmp))
    }
}
