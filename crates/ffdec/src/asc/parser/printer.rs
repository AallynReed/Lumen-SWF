//! Port of `macromedia/asc/parser/NodePrinter.java`.

#![allow(non_upper_case_globals)]

use std::cell::Ref;
use std::io::Write;

use super::evaluator::{evaluate, Evaluator};
use super::meta_data_evaluator::MetaDataEvaluator;
use super::node::*;
use super::tokens::*;
use crate::asc::semantics::value::Value;
use crate::asc::util::number_usage::NumberUsage;
use crate::asc::util::context::ContextRef;

pub const man_mode: i32 = 0;
pub const machine_mode: i32 = 1;

static usageName: [&str; 5] = ["Number", "decimal", "double", "int", "uint"];

fn rb(n: &Option<NodeRef>) -> Ref<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow()
}

fn mode_name(mode: i32, dot_if_other: bool) -> &'static str {
    if mode == LEFTBRACKET_TOKEN {
        " bracket"
    } else if mode == LEFTPAREN_TOKEN {
        " filter"
    } else if mode == DOUBLEDOT_TOKEN {
        " descend"
    } else if mode == EMPTY_TOKEN {
        " lexical"
    } else if dot_if_other {
        " dot"
    } else {
        " error"
    }
}

pub struct NodePrinter<W: Write> {
    out: W,
    level: i32,
    mode: i32,
    /// Not in Java: print each node's position as it is visited, for comparing parse trees.
    pub show_positions: bool,
    /// Not in Java: text printed after each visited node's position, for comparing semantic annotations.
    pub annotate: Option<fn(&ContextRef, &NodeRef) -> String>,
}

impl NodePrinter<std::io::Stdout> {
    pub fn new() -> NodePrinter<std::io::Stdout> {
        Self::new_mode(man_mode, std::io::stdout())
    }
}

impl<W: Write> NodePrinter<W> {
    pub fn new_out(out: W) -> NodePrinter<W> {
        Self::new_mode(man_mode, out)
    }

    pub fn new_mode(mode: i32, out: W) -> NodePrinter<W> {
        NodePrinter { out, level: 0, mode, show_positions: false, annotate: None }
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    fn print(&mut self, s: &str) {
        let _ = self.out.write_all(&crate::asc::util::jstr::to_java_utf8_bytes(s));
    }

    fn println(&mut self) {
        self.print("\n");
    }

    fn separate(&mut self) {
        if self.mode == machine_mode {
            self.print("|");
        }
    }

    fn push_in(&mut self) {
        self.level += 1;
        if self.mode == machine_mode {
            self.print("[");
        } else {
            self.print(" ");
        }
    }

    fn pop_out(&mut self) {
        if self.mode == machine_mode {
            self.print("]");
        }
        self.level -= 1;
    }

    fn indent(&mut self) {
        if self.mode == man_mode {
            self.println();
            for _ in 0..self.level {
                self.print("  ");
            }
        }
    }

    fn eval(&mut self, cx: &ContextRef, node: &Option<NodeRef>) {
        evaluate(node.as_ref().expect("NullPointerException"), cx, self);
    }

    fn eval_if(&mut self, cx: &ContextRef, node: &Option<NodeRef>) {
        if node.is_some() {
            self.eval(cx, node);
        }
    }

    fn print_items(&mut self, cx: &ContextRef, items: Vec<Option<NodeRef>>) {
        for n in &items {
            self.eval(cx, n);
            self.separate();
        }
    }
}

macro_rules! fields {
    ($node:expr, $as:ident, $($f:ident),+) => {{
        let n = $node.borrow();
        let n = n.$as().expect("ClassCastException");
        ($(n.$f.clone()),+)
    }};
}

impl<W: Write> Evaluator for NodePrinter<W> {
    fn check_feature(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        if self.show_positions {
            let pos = node.borrow().get_position();
            self.print(&format!("@{}", pos));
        }
        if let Some(f) = self.annotate {
            let s = f(cx, node);
            self.print(&s);
        }
        true
    }

    fn evaluate_node(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("error:undefined printer method");
        None
    }

    fn evaluate_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();

        let (is_type_identifier, is_attr, name, type_args) = {
            let n = node.borrow();
            let type_args = n.as_type_identifier().map(|t| t.type_args.clone());
            (type_args.is_some(), n.is_attr(), n.identifier().unwrap().name.clone(), type_args.flatten())
        };

        if is_type_identifier {
            self.print("typeidentifier ");
        } else if is_attr {
            self.print("attributeidentifier ");
        } else {
            self.print("identifier ");
        }
        self.print(&name);
        if is_type_identifier {
            self.print("types ");
            self.push_in();
            self.eval(cx, &type_args);
            self.pop_out();
        }
        None
    }

    fn evaluate_increment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("increment");
        let (mode, is_postfix, op, expr) = {
            let n = node.borrow();
            let i = n.as_increment().unwrap();
            (i.sel.get_mode(), i.is_postfix, i.op, i.sel.expr.clone())
        };
        self.print(if mode == DOT_TOKEN { " dot" } else { mode_name(mode, false) });
        self.print(&format!("{}{}", if is_postfix { " postfix " } else { " prefix " }, get_token_class_name(op)));
        self.push_in();
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_this_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("this");
        None
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let (is_attr, name, qualifier) = {
            let n = node.borrow();
            let q = n.as_qualified_identifier().unwrap();
            (n.is_attr(), q.id.name.clone(), q.qualifier.clone())
        };
        if is_attr {
            self.print("qualifiedattributeidentifier ");
        } else {
            self.print("qualifiedidentifier ");
        }
        self.print(&name);
        if qualifier.is_some() {
            self.push_in();
            self.indent();
            self.print("qualifier");
            self.push_in();
            self.eval(cx, &qualifier);
            self.pop_out();
            self.pop_out();
        }
        None
    }

    fn evaluate_qualified_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let (is_attr, name, qualifier, expr) = {
            let n = node.borrow();
            let q = n.as_qualified_expression().unwrap();
            (n.is_attr(), q.qid.id.name.clone(), q.qid.qualifier.clone(), q.expr.clone())
        };
        if is_attr {
            self.print("qualifiedattributeexpression ");
        } else {
            self.print("qualifiedexpression ");
        }
        self.print(&name);
        if qualifier.is_some() {
            self.push_in();
            self.indent();
            self.print("qualifier");
            self.push_in();
            self.eval(cx, &qualifier);
            self.pop_out();
            self.pop_out();
        }
        if expr.is_some() {
            self.push_in();
            self.indent();
            self.print("expr");
            self.push_in();
            self.eval(cx, &expr);
            self.pop_out();
            self.pop_out();
        }
        None
    }

    fn evaluate_literal_boolean(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalboolean ");
        let value = node.borrow().as_literal_boolean().unwrap().value;
        self.print(if value { "1" } else { "0" });
        None
    }

    fn evaluate_literal_number(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalnumber:");
        let value = node.borrow().as_literal_number().unwrap().value.clone();
        self.print(&value);
        None
    }

    fn evaluate_literal_string(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalstring:");
        let value = node.borrow().as_literal_string().unwrap().value.clone();
        self.print(&value);
        None
    }

    fn evaluate_literal_null(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalnull");
        None
    }

    fn evaluate_literal_reg_exp(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalregexp:");
        let value = node.borrow().as_literal_reg_exp().unwrap().value.clone();
        self.print(&value);
        None
    }

    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalxml");
        self.push_in();
        let list = fields!(node, as_literal_xml, list);
        self.eval_if(cx, &list);
        self.pop_out();
        None
    }

    fn evaluate_function_common(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("functioncommon");
        self.push_in();
        let (signature, body) = fields!(node, as_function_common, signature, body);
        self.eval_if(cx, &signature);
        self.separate();
        self.eval_if(cx, &body);
        self.pop_out();
        None
    }

    fn evaluate_paren_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("paren");
        None
    }

    fn evaluate_paren_list_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("parenlist");
        let expr = fields!(node, as_paren_list_expression, expr);
        self.eval_if(cx, &expr);
        None
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalobject");
        self.push_in();
        let fieldlist = fields!(node, as_literal_object, fieldlist);
        self.eval_if(cx, &fieldlist);
        self.pop_out();
        None
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalfield");
        self.push_in();
        let (name, value) = fields!(node, as_literal_field, name, value);
        self.eval_if(cx, &name);
        self.separate();
        self.eval_if(cx, &value);
        self.pop_out();
        None
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("literalarray");
        self.push_in();
        let elementlist = fields!(node, as_literal_array, elementlist);
        self.eval_if(cx, &elementlist);
        self.pop_out();
        None
    }

    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("new<");
        let (type_, elementlist) = fields!(node, as_literal_vector, type_, elementlist);
        self.eval(cx, &type_);
        self.print(">");

        self.push_in();
        self.eval_if(cx, &elementlist);
        self.pop_out();
        None
    }

    fn evaluate_super_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("superexpression");
        self.push_in();
        let expr = fields!(node, as_super_expression, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("member");
        self.push_in();
        let (base, selector) = fields!(node, as_member_expression, base, selector);
        self.eval_if(cx, &base);
        self.separate();
        self.eval_if(cx, &selector);
        self.pop_out();

        None
    }

    fn evaluate_invoke(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("invoke");
        let (mode, name, args) = {
            let n = node.borrow();
            let i = n.as_invoke().unwrap();
            (i.sel.get_mode(), i.name.clone(), i.args.clone())
        };
        self.print(mode_name(mode, true));
        self.push_in();
        self.print(&name);
        self.separate();
        self.eval_if(cx, &args);
        self.pop_out();
        None
    }

    fn evaluate_call_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let (is_new, mode, expr, args) = {
            let n = node.borrow();
            let c = n.as_call_expression().unwrap();
            (c.is_new, c.sel.get_mode(), c.sel.expr.clone(), c.args.clone())
        };
        self.print(if is_new { "construct" } else { "call" });
        self.print(mode_name(mode, true));
        self.push_in();
        self.eval_if(cx, &expr);
        self.separate();

        self.eval_if(cx, &args);
        self.pop_out();
        None
    }

    fn evaluate_delete_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("delete");
        let (mode, expr) = {
            let n = node.borrow();
            let d = n.as_delete_expression().unwrap();
            (d.sel.get_mode(), d.sel.expr.clone())
        };
        self.print(mode_name(mode, true));
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_apply_type_expr(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("applytype");
        self.push_in();
        let type_args = fields!(node, as_apply_type_expr, type_args);
        self.eval(cx, &type_args);
        self.pop_out();
        None
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("get");
        let (mode, expr) = {
            let n = node.borrow();
            let g = n.as_get_expression().unwrap();
            (g.sel.get_mode(), g.sel.expr.clone())
        };
        self.print(mode_name(mode, true));
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_set_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("set");
        let (mode, expr, args) = {
            let n = node.borrow();
            let s = n.as_set_expression().unwrap();
            (s.sel.get_mode(), s.sel.expr.clone(), s.args.clone())
        };
        self.print(mode_name(mode, true));
        self.push_in();
        self.eval_if(cx, &expr);
        self.separate();
        self.eval_if(cx, &args);
        self.pop_out();
        None
    }

    fn evaluate_unary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("unary");
        let (op, expr) = fields!(node, as_unary_expression, op, expr);
        self.push_in();
        self.print(get_token_class_name(op));
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_binary_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("binary");
        let (op, lhs, rhs) = fields!(node, as_binary_expression, op, lhs, rhs);
        self.push_in();
        self.print(get_token_class_name(op));
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &lhs);
        self.separate();
        self.eval_if(cx, &rhs);
        self.pop_out();
        None
    }

    fn evaluate_conditional_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("cond");
        let (condition, thenexpr, elseexpr) = fields!(node, as_conditional_expression, condition, thenexpr, elseexpr);
        self.push_in();
        self.eval_if(cx, &condition);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &thenexpr);
        self.separate();
        self.eval_if(cx, &elseexpr);
        self.pop_out();
        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("argumentlist");

        self.push_in();

        let items = fields!(node, as_argument_list, items);
        self.print_items(cx, items);

        self.pop_out();

        None
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("list");

        self.push_in();

        let items = fields!(node, list, items);
        self.print_items(cx, items);

        self.pop_out();

        None
    }

    fn evaluate_statement_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("statementlist");
        self.push_in();

        let items = fields!(node, as_statement_list, items);
        for n in &items {
            self.eval_if(cx, n);
        }

        self.pop_out();

        None
    }

    fn evaluate_empty_element(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("empty");
        None
    }

    fn evaluate_empty_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("empty");
        None
    }

    fn evaluate_expression_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("expression");
        self.push_in();
        let expr = fields!(node, as_expression_statement, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_super_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("super");
        self.push_in();
        let call = fields!(node, as_super_statement, call);
        let args = rb(&call).as_call_expression().expect("ClassCastException").args.clone();
        self.eval_if(cx, &args);
        self.pop_out();
        None
    }

    fn evaluate_labeled_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("labeled");
        let (label, statement) = fields!(node, as_labeled_statement, label, statement);
        self.push_in();
        self.eval_if(cx, &label);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statement);
        self.pop_out();
        None
    }

    fn evaluate_if_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("if");
        let (condition, thenactions, elseactions) = fields!(node, as_if_statement, condition, thenactions, elseactions);
        self.push_in();
        self.eval_if(cx, &condition);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &thenactions);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &elseactions);
        self.pop_out();
        None
    }

    fn evaluate_switch_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("switch");
        let (expr, statements) = fields!(node, as_switch_statement, expr, statements);
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statements);
        self.pop_out();
        None
    }

    fn evaluate_case_label(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("case");
        self.push_in();
        let label = fields!(node, as_case_label, label);
        if label.is_some() {
            self.eval(cx, &label);
        } else {
            self.print("default");
        }
        self.pop_out();
        None
    }

    fn evaluate_do_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("do");
        let (expr, statements) = fields!(node, as_do_statement, expr, statements);
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statements);
        self.pop_out();
        None
    }

    fn evaluate_while_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("while");
        let (expr, statement) = fields!(node, as_while_statement, expr, statement);
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statement);
        self.pop_out();
        None
    }

    fn evaluate_for_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("for");
        let (initialize, test, increment, statement) = fields!(node, as_for_statement, initialize, test, increment, statement);
        self.push_in();
        self.eval_if(cx, &initialize);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &test);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &increment);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statement);
        self.pop_out();
        None
    }

    fn evaluate_with_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("with");
        let (expr, statement) = fields!(node, as_with_statement, expr, statement);
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statement);
        self.pop_out();
        None
    }

    fn evaluate_continue_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("continue");
        self.push_in();
        let id = fields!(node, as_continue_statement, id);
        self.eval_if(cx, &id);
        self.pop_out();
        None
    }

    fn evaluate_break_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("break");
        self.push_in();
        let id = fields!(node, as_break_statement, id);
        self.eval_if(cx, &id);
        self.pop_out();
        None
    }

    fn evaluate_return_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("return");
        self.push_in();
        let expr = fields!(node, as_return_statement, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_throw_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("throw");
        self.push_in();
        let expr = fields!(node, as_throw_statement, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_try_statement(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("try");
        let (tryblock, catchlist, finallyblock) = fields!(node, as_try_statement, tryblock, catchlist, finallyblock);
        self.push_in();
        self.eval_if(cx, &tryblock);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &catchlist);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &finallyblock);
        self.pop_out();
        None
    }

    fn evaluate_catch_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("catch");
        let (parameter, statements) = fields!(node, as_catch_clause, parameter, statements);
        self.push_in();
        self.eval_if(cx, &parameter);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &statements);
        self.pop_out();
        None
    }

    fn evaluate_finally_clause(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("finally");
        self.push_in();
        let statements = fields!(node, as_finally_clause, statements);
        self.eval_if(cx, &statements);
        self.pop_out();
        None
    }

    fn evaluate_use_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("use");
        let expr = fields!(node, as_use_directive, expr);
        self.eval_if(cx, &expr);
        None
    }

    fn evaluate_include_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("include");
        self.push_in();
        let filespec = fields!(node, as_include_directive, filespec);
        self.eval_if(cx, &filespec);
        self.separate();
        self.pop_out();

        None
    }

    fn evaluate_import_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("import");
        let (attrs, name) = fields!(node, as_import_directive, attrs, name);
        self.push_in();
        self.eval_if(cx, &attrs);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &name);
        self.pop_out();

        None
    }

    fn evaluate_attribute_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("attributelist");

        self.push_in();

        let items = fields!(node, as_attribute_list, items);
        self.print_items(cx, items);

        self.pop_out();

        None
    }

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let (kind, attrs, list) = {
            let n = node.borrow();
            let v = n.as_variable_definition().unwrap();
            (v.kind, v.def.attrs.clone(), v.list.clone())
        };
        if kind == CONST_TOKEN {
            self.print("const");
        } else {
            self.print("var");
        }
        self.push_in();
        self.eval_if(cx, &attrs);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &list);
        self.pop_out();
        None
    }

    fn evaluate_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("variablebinding");
        self.push_in();
        let (variable, initializer) = fields!(node, as_variable_binding, variable, initializer);
        self.eval_if(cx, &variable);
        self.separate();
        self.eval_if(cx, &initializer);
        self.pop_out();
        None
    }

    fn evaluate_untyped_variable_binding(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("untypedvariablebinding");
        self.push_in();
        let (identifier, initializer) = fields!(node, as_untyped_variable_binding, identifier, initializer);
        self.eval_if(cx, &identifier);
        self.separate();
        self.eval_if(cx, &initializer);
        self.pop_out();
        None
    }

    fn evaluate_typed_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("typedidentifier");
        self.push_in();
        let (identifier, type_) = fields!(node, as_typed_identifier, identifier, type_);
        self.eval_if(cx, &identifier);
        self.separate();
        self.eval_if(cx, &type_);
        self.pop_out();
        None
    }

    fn evaluate_binary_function_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_function_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("function");
        self.push_in();
        let (attrs, name, fexpr) = {
            let n = node.borrow();
            let f = n.as_function_definition().unwrap();
            (f.def.attrs.clone(), f.name.clone(), f.fexpr.clone())
        };
        self.eval_if(cx, &attrs);
        self.separate();
        self.eval_if(cx, &name);
        self.separate();
        self.eval_if(cx, &fexpr);
        self.pop_out();
        None
    }

    fn evaluate_function_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("functionname");
        let (kind, identifier) = fields!(node, as_function_name, kind, identifier);
        self.print(if kind == GET_TOKEN {
            " get"
        } else if kind == SET_TOKEN {
            " set"
        } else {
            ""
        });
        self.push_in();
        self.eval_if(cx, &identifier);
        self.pop_out();
        None
    }

    fn evaluate_function_signature(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let (parameter, result, inits) = fields!(node, as_function_signature, parameter, result, inits);
        self.print(if inits.is_some() { "constructorsignature" } else { "functionsignature" });
        self.push_in();
        self.eval_if(cx, &parameter);
        self.separate();
        self.eval_if(cx, &result);
        self.eval_if(cx, &inits);
        self.pop_out();
        None
    }

    fn evaluate_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("parameter");
        let (kind, identifier, init, type_) = fields!(node, as_parameter, kind, identifier, init, type_);
        if kind == CONST_TOKEN {
            self.print(" const");
        }
        self.push_in();
        self.eval_if(cx, &identifier);
        self.separate();
        self.eval_if(cx, &init);
        self.separate();
        self.eval_if(cx, &type_);
        self.pop_out();
        None
    }

    fn evaluate_rest_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("restexpression");
        self.push_in();
        let expr = fields!(node, as_rest_expression, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_rest_parameter(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("restparameter");
        self.push_in();
        let parameter = fields!(node, as_rest_parameter, parameter);
        self.eval_if(cx, &parameter);
        self.pop_out();
        None
    }

    fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("class");
        self.push_in();
        let (attrs, name, baseclass, interfaces, statements) = {
            let n = node.borrow();
            let c = n.class_definition().unwrap();
            (c.def.attrs.clone(), c.name.clone(), c.baseclass.clone(), c.interfaces.clone(), c.statements.clone())
        };
        self.eval_if(cx, &attrs);
        self.separate();
        self.eval_if(cx, &name);
        self.separate();
        self.eval_if(cx, &baseclass);
        self.separate();
        self.eval_if(cx, &interfaces);
        self.separate();
        self.eval_if(cx, &statements);
        self.pop_out();
        None
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("interface");
        self.push_in();
        let (attrs, name, interfaces, statements) = {
            let n = node.borrow();
            let c = n.class_definition().unwrap();
            (c.def.attrs.clone(), c.name.clone(), c.interfaces.clone(), c.statements.clone())
        };
        self.eval_if(cx, &attrs);
        self.separate();
        self.eval_if(cx, &name);
        self.separate();
        self.eval_if(cx, &interfaces);
        self.separate();
        self.eval_if(cx, &statements);
        self.pop_out();
        None
    }

    fn evaluate_class_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("classname");
        let (pkgname, ident) = fields!(node, as_class_name, pkgname, ident);
        self.push_in();
        self.eval_if(cx, &pkgname);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &ident);
        self.pop_out();
        None
    }

    fn evaluate_inheritance(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (baseclass, interfaces) = fields!(node, as_inheritance, baseclass, interfaces);
        if baseclass.is_some() {
            self.indent();
            self.print("extends");
            self.push_in();
            self.eval(cx, &baseclass);
            self.pop_out();
        }
        self.separate();
        if interfaces.is_some() {
            self.indent();
            self.print("implements");
            self.push_in();
            self.eval(cx, &interfaces);
            self.pop_out();
        }
        None
    }

    fn evaluate_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("namespace");
        self.print_namespace_definition(cx, node);
        None
    }

    fn evaluate_config_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("config namespace");
        self.print_namespace_definition(cx, node);
        None
    }

    fn evaluate_package_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("package");
        self.push_in();
        let name = fields!(node, as_package_definition, name);
        self.eval_if(cx, &name);
        self.pop_out();
        None
    }

    fn evaluate_package_identifiers(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("packageidentifiers");
        self.push_in();

        let list = fields!(node, as_package_identifiers, list);
        self.print_items(cx, list);

        self.pop_out();
        None
    }

    fn evaluate_package_name(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("packagename");
        self.push_in();
        let id = fields!(node, as_package_name, id);
        self.eval_if(cx, &id);
        self.pop_out();
        None
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("program");
        self.push_in();
        let statements = node.borrow().program().unwrap().statements.clone();
        self.eval_if(cx, &statements);
        self.pop_out();
        self.println();
        None
    }

    fn evaluate_error(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("error");
        let error_code = node.borrow().as_error().unwrap().error_code;
        self.print(&error_code.to_string());
        None
    }

    fn evaluate_to_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("toobject");
        self.push_in();
        let expr = fields!(node, as_to_object, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        self.println();
        None
    }

    fn evaluate_load_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("loadregister");
        self.push_in();
        let reg = fields!(node, as_load_register, reg);
        self.eval_if(cx, &reg);
        self.pop_out();
        None
    }

    fn evaluate_store_register(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("storeregister");
        let (reg, expr) = fields!(node, as_store_register, reg, expr);
        self.push_in();
        self.eval_if(cx, &reg);
        self.pop_out();
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        self.println();
        None
    }

    fn evaluate_register(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("register");

        self.push_in();
        let index = node.borrow().as_register().unwrap().index;
        self.print(&index.to_string());
        None
    }

    fn evaluate_has_next(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("hasNext");

        let (object_register, index_register) = fields!(node, as_has_next, object_register, index_register);
        self.push_in();
        self.eval_if(cx, &object_register);
        self.pop_out();
        self.push_in();
        self.eval_if(cx, &index_register);
        self.pop_out();
        self.println();
        None
    }

    fn evaluate_box(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("box");
        let (actual, expr) = fields!(node, as_box, actual, expr);
        self.print(&actual.map_or_else(|| "null".to_string(), |t| t.to_string()));
        self.push_in();
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_coerce(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("coerce");
        self.push_in();
        let expr = fields!(node, as_coerce, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_pragma(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("pragma");
        self.push_in();
        let list = fields!(node, as_pragma, list);
        self.eval_if(cx, &list);
        self.pop_out();
        None
    }

    fn evaluate_use_precision(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let precision = node.borrow().as_use_precision().unwrap().precision;
        self.print(&format!("usePrecision({})", precision));
        None
    }

    fn evaluate_use_numeric(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let numeric_mode = node.borrow().as_use_numeric().unwrap().numeric_mode;
        self.print(&format!("useNumeric({})", usageName[numeric_mode as usize]));
        None
    }

    fn evaluate_use_rounding(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        let mode = node.borrow().as_use_rounding().unwrap().mode;
        self.print(&format!("useRounding({})", NumberUsage::roundingModeName[mode as usize]));
        None
    }

    fn evaluate_pragma_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("pragmaitem");
        self.push_in();
        let identifier = fields!(node, as_pragma_expression, identifier);
        self.eval_if(cx, &identifier);
        self.pop_out();
        None
    }

    fn evaluate_parameter_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("parameterlist");

        self.push_in();

        let items = fields!(node, as_parameter_list, items);
        let size = items.len();
        for (i, param) in items.iter().enumerate() {
            self.eval_if(cx, param);

            if i + 1 < size {
                self.separate();
            }
        }

        self.pop_out();

        None
    }

    fn evaluate_meta_data(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let has_data = node.borrow().meta_data().unwrap().data.is_some();
        if has_data {
            let mut mde = MetaDataEvaluator::new();
            mde.evaluate_meta_data(cx, node);
        }

        self.indent();
        self.print("metadata:");
        let (id, values) = {
            let n = node.borrow();
            let m = n.meta_data().unwrap();
            (m.get_id(), m.get_values())
        };
        self.print(id.as_deref().unwrap_or(""));
        self.print(" ");
        for v in values.iter().flatten() {
            match v {
                Value::KeyValuePair(pair) => self.print(&format!("[{},{}]", pair.key, pair.obj)),
                Value::KeylessValue(val) => self.print(&format!("[{}]", val.obj)),
                _ => {}
            }
        }
        None
    }

    fn evaluate_default_xml_namespace(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("dxns");
        self.push_in();
        let expr = fields!(node, as_default_xml_namespace, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }

    fn evaluate_doc_comment(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.evaluate_meta_data(cx, node);
        None
    }

    fn evaluate_import(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_binary_program(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_binary_class_def(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_binary_interface_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        None
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.indent();
        self.print("typeexpr");
        self.push_in();
        let expr = fields!(node, as_type_expression, expr);
        self.eval_if(cx, &expr);
        self.pop_out();
        None
    }
}

impl<W: Write> NodePrinter<W> {
    /// The shared body of the `NamespaceDefinitionNode` and `ConfigNamespaceDefinitionNode` printers.
    fn print_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        let (attrs, name, value) = {
            let n = node.borrow();
            let ns = n.namespace_definition().unwrap();
            (ns.def.attrs.clone(), ns.name.clone(), ns.value.clone())
        };
        self.push_in();
        self.eval_if(cx, &attrs);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &name);
        self.pop_out();
        self.separate();
        self.push_in();
        self.eval_if(cx, &value);
        self.pop_out();
    }
}
