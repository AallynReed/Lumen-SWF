//! Ports of `flash.swf.tools.as3.PrettyPrinter` and its subclass `flex2.compiler.as3.binding.PrefixedPrettyPrinter`,
//! which reprint watcher arguments as AS3 with unqualified references prefixed (`target.`).

use crate::asc::flex2::compiler::as3::evaluator_adapter::{evaluate, walk, Adapter};
use crate::asc::flex2::compiler::mxml::parser::LINE_SEPARATOR;
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::tokens::*;
use crate::asc::util::context::ContextRef;

pub struct PrefixedPrettyPrinter {
    prefix: String,
    out: String,
    indent: i32,
}

impl PrefixedPrettyPrinter {
    pub fn new(prefix: &str) -> PrefixedPrettyPrinter {
        PrefixedPrettyPrinter { prefix: prefix.to_string(), out: String::new(), indent: 0 }
    }

    pub fn print(&mut self, cx: &ContextRef, node: &NodeRef) {
        evaluate(self, cx, node);
    }

    pub fn into_string(self) -> String {
        self.out
    }

    fn p(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn println(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push_str(LINE_SEPARATOR);
    }

    fn ev(&mut self, cx: &ContextRef, n: &Option<NodeRef>) {
        if let Some(n) = n {
            evaluate(self, cx, n);
        }
    }

    fn do_indent(&mut self) {
        for _ in 0..self.indent {
            self.p("    ");
        }
    }

    fn items(&mut self, cx: &ContextRef, items: Vec<Option<NodeRef>>, sep: &str, trailing: bool) {
        let size = items.len();
        for (i, n) in items.into_iter().enumerate() {
            self.ev(cx, &n);
            if trailing || i + 1 < size {
                self.p(sep);
            }
        }
    }

    /// `isStatic(cx, node)`: the selector refers to a class.
    fn is_static(&self, cx: &ContextRef, node: &NodeRef) -> bool {
        let r = node.borrow().selector().and_then(|s| s.ref_.clone());
        match r {
            Some(r) => {
                let type_name = r.get_type(&cx.borrow()).get_name().to_string();
                type_name == "Class" && r.slot().is_some_and(|s| s.get_object_value().is_some())
            }
            None => false,
        }
    }

    fn op(&mut self, op: i32) {
        let s = match op {
            NOTEQUALS_TOKEN => " != ",
            STRICTNOTEQUALS_TOKEN => " !== ",
            LESSTHAN_TOKEN => " < ",
            LESSTHANOREQUALS_TOKEN => " <= ",
            EQUALS_TOKEN => " == ",
            STRICTEQUALS_TOKEN => " === ",
            GREATERTHAN_TOKEN => " > ",
            GREATERTHANOREQUALS_TOKEN => " >= ",
            MULTASSIGN_TOKEN => " *= ",
            MULT_TOKEN => " * ",
            DIVASSIGN_TOKEN => " /= ",
            DIV_TOKEN => " / ",
            MODULUSASSIGN_TOKEN => " %= ",
            MODULUS_TOKEN => " % ",
            PLUSASSIGN_TOKEN => " += ",
            PLUS_TOKEN => " + ",
            MINUSASSIGN_TOKEN => " -= ",
            MINUS_TOKEN => " - ",
            LEFTSHIFTASSIGN_TOKEN => " <<= ",
            LEFTSHIFT_TOKEN => " << ",
            RIGHTSHIFTASSIGN_TOKEN => " >>= ",
            RIGHTSHIFT_TOKEN => " >> ",
            UNSIGNEDRIGHTSHIFTASSIGN_TOKEN => " >>>= ",
            UNSIGNEDRIGHTSHIFT_TOKEN => " >>> ",
            BITWISEANDASSIGN_TOKEN => " &= ",
            BITWISEAND_TOKEN => " & ",
            BITWISEXORASSIGN_TOKEN => " |= ",
            BITWISEXOR_TOKEN => " ^ ",
            BITWISEORASSIGN_TOKEN => " ^= ",
            BITWISEOR_TOKEN => " | ",
            LOGICALANDASSIGN_TOKEN => " &&= ",
            LOGICALAND_TOKEN => " && ",
            LOGICALXORASSIGN_TOKEN => " ^^= ",
            LOGICALXOR_TOKEN => " ^^ ",
            LOGICALORASSIGN_TOKEN => " ||= ",
            LOGICALOR_TOKEN => " || ",
            _ => "",
        };
        self.p(s);
    }

    fn block(&mut self, cx: &ContextRef, body: &Option<NodeRef>) {
        self.do_indent();
        self.println("{");
        self.indent += 1;
        self.ev(cx, body);
        self.indent -= 1;
        self.do_indent();
        self.println("}");
    }
}

impl Adapter for PrefixedPrettyPrinter {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let b = node.borrow();
        match &b.kind {
            Kind::AttributeList(n) => {
                let items = n.items.clone();
                drop(b);
                self.items(cx, items, " ", true);
            }
            Kind::ArgumentList(n) => {
                let items = n.items.clone();
                drop(b);
                self.items(cx, items, ", ", false);
            }
            Kind::List(_) => {
                let items = b.list().unwrap().items.clone();
                drop(b);
                self.items(cx, items, ", ", false);
            }
            Kind::BinaryExpression(n) => {
                let (lhs, op, rhs) = (n.lhs.clone(), n.op, n.rhs.clone());
                drop(b);
                self.ev(cx, &lhs);
                self.op(op);
                self.ev(cx, &rhs);
            }
            Kind::CallExpression(n) => {
                let (is_new, expr, args) = (n.is_new, n.sel.expr.clone(), n.args.clone());
                drop(b);
                if !is_new {
                    let prefix = format!("{}.", self.prefix);
                    self.p(&prefix);
                }
                self.ev(cx, &expr);
                self.p("(");
                self.ev(cx, &args);
                self.p(")");
            }
            Kind::GetExpression(n) => {
                let (base_none, expr) = (n.sel.base.is_none(), n.sel.expr.clone());
                drop(b);
                if base_none && !self.is_static(cx, node) {
                    let prefix = format!("{}.", self.prefix);
                    self.p(&prefix);
                }
                if let Some(expr) = expr {
                    let is_args = expr.borrow().as_argument_list().is_some();
                    if is_args {
                        self.p("[");
                        evaluate(self, cx, &expr);
                        self.p("]");
                    } else {
                        evaluate(self, cx, &expr);
                    }
                }
            }
            Kind::SetExpression(n) => {
                let base_none = n.sel.base.is_none();
                drop(b);
                if base_none && !self.is_static(cx, node) {
                    let prefix = format!("{}.", self.prefix);
                    self.p(&prefix);
                }
                walk(self, cx, node);
            }
            Kind::ThisExpression(_) => {
                drop(b);
                let prefix = self.prefix.clone();
                self.p(&prefix);
            }
            Kind::ClassDefinition(n) => {
                let (attrs, name, baseclass, interfaces, statements) = (n.def.attrs.clone(), n.name.clone(), n.baseclass.clone(), n.interfaces.clone(), n.statements.clone());
                let builder_name = n.cframe.as_ref().and_then(|f| f.builder()).and_then(|bu| bu.classname()).map(|q| q.to_string());
                drop(b);
                self.ev(cx, &attrs);
                self.println("");
                self.p("class ");
                let has_name = name.as_ref().is_some_and(|n| n.borrow().identifier().is_some());
                if has_name {
                    self.ev(cx, &name);
                } else if let Some(bn) = builder_name {
                    self.p(&bn);
                }
                if baseclass.is_some() {
                    self.p(" extends ");
                    self.ev(cx, &baseclass);
                }
                self.println("");
                if interfaces.is_some() {
                    self.p("\timplements ");
                    self.ev(cx, &interfaces);
                    self.println("");
                }
                self.println("{");
                self.indent += 1;
                self.ev(cx, &statements);
                self.indent -= 1;
                self.println("}");
            }
            Kind::InterfaceDefinition(n) => {
                let (name, baseclass, statements) = (n.cls.name.clone(), n.cls.baseclass.clone(), n.cls.statements.clone());
                let builder_name = n.cls.cframe.as_ref().and_then(|f| f.builder()).and_then(|bu| bu.classname()).map(|q| q.to_string());
                drop(b);
                self.do_indent();
                self.p("interface ");
                let has_name = name.as_ref().is_some_and(|n| n.borrow().identifier().is_some());
                if has_name {
                    self.ev(cx, &name);
                } else if let Some(bn) = builder_name {
                    self.p(&bn);
                }
                if baseclass.is_some() {
                    self.p(" extends ");
                    self.ev(cx, &baseclass);
                }
                self.println("");
                self.block(cx, &statements);
            }
            Kind::ForStatement(n) => {
                let (i, t, inc, s) = (n.initialize.clone(), n.test.clone(), n.increment.clone(), n.statement.clone());
                drop(b);
                self.do_indent();
                self.p("for (");
                self.ev(cx, &i);
                self.p("; ");
                self.ev(cx, &t);
                self.p("; ");
                self.ev(cx, &inc);
                self.println(")");
                self.block(cx, &s);
            }
            Kind::FunctionCommon(n) => {
                let (sig, body) = (n.signature.clone(), n.body.clone());
                drop(b);
                self.p("(");
                self.ev(cx, &sig);
                self.println(")");
                self.block(cx, &body);
                self.println("");
            }
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => {
                let f = b.function_definition().unwrap();
                let (attrs, name, fexpr) = (f.def.attrs.clone(), f.name.clone(), f.fexpr.clone());
                drop(b);
                self.do_indent();
                self.ev(cx, &attrs);
                self.p("function ");
                self.ev(cx, &name);
                self.ev(cx, &fexpr);
            }
            Kind::Identifier(n) => {
                let name = n.name.clone();
                drop(b);
                self.p(&name);
            }
            Kind::QualifiedIdentifier(n) => {
                let (qualifier, name) = (n.qualifier.clone(), n.id.name.clone());
                drop(b);
                self.ev(cx, &qualifier);
                self.p(&name);
            }
            Kind::IfStatement(n) => {
                let (c, t, e) = (n.condition.clone(), n.thenactions.clone(), n.elseactions.clone());
                drop(b);
                self.do_indent();
                self.p("if (");
                self.ev(cx, &c);
                self.println(")");
                self.block(cx, &t);
                if e.is_some() {
                    self.do_indent();
                    self.println("else");
                    self.block(cx, &e);
                }
            }
            Kind::LiteralArray(_) => {
                drop(b);
                self.p("[");
                walk(self, cx, node);
                self.p("]");
            }
            Kind::LiteralVector(n) => {
                let (t, el) = (n.type_.clone(), n.elementlist.clone());
                drop(b);
                self.p("new<");
                self.ev(cx, &t);
                self.p(">");
                self.p("[");
                if let Some(el) = el {
                    walk(self, cx, &el);
                }
                self.p("]");
            }
            Kind::LiteralBoolean(n) => {
                let v = n.value;
                drop(b);
                self.p(if v { "true" } else { "false" });
            }
            Kind::LiteralField(n) => {
                let (name, value) = (n.name.clone(), n.value.clone());
                drop(b);
                self.ev(cx, &name);
                self.p(": ");
                self.ev(cx, &value);
            }
            Kind::LiteralNumber(n) => {
                let v = n.value.clone();
                drop(b);
                self.p(&v);
            }
            Kind::LiteralObject(_) => {
                drop(b);
                self.p("{");
                walk(self, cx, node);
                self.p("}");
            }
            Kind::LiteralString(n) => {
                let v = format!("\"{}\"", n.value);
                drop(b);
                self.p(&v);
            }
            Kind::LiteralNull(_) => {
                drop(b);
                self.p("null");
            }
            Kind::LiteralRegExp(n) => {
                let v = n.value.clone();
                drop(b);
                self.p(&v);
            }
            Kind::LiteralXML(_) => {}
            Kind::MemberExpression(n) => {
                let (base, selector) = (n.base.clone(), n.selector.clone());
                drop(b);
                if let Some(base) = &base {
                    evaluate(self, cx, base);
                    let dot = selector.as_ref().is_some_and(|s| {
                        let sb = s.borrow();
                        sb.as_get_expression().is_some_and(|g| !g.sel.expr.as_ref().is_some_and(|e| e.borrow().as_argument_list().is_some()))
                    });
                    if dot {
                        self.p(".");
                    }
                }
                self.ev(cx, &selector);
            }
            Kind::ReturnStatement(n) => {
                let e = n.expr.clone();
                drop(b);
                self.do_indent();
                self.p("return ");
                self.ev(cx, &e);
                self.println(";");
            }
            Kind::TypedIdentifier(n) => {
                let (i, t) = (n.identifier.clone(), n.type_.clone());
                drop(b);
                self.ev(cx, &i);
                if t.is_some() {
                    self.p(":");
                    self.ev(cx, &t);
                }
            }
            Kind::VariableDefinition(n) => {
                let (attrs, list) = (n.def.attrs.clone(), n.list.clone());
                drop(b);
                self.do_indent();
                self.ev(cx, &attrs);
                self.p("var ");
                self.ev(cx, &list);
                self.println(";");
            }
            Kind::WhileStatement(n) => {
                let (e, s) = (n.expr.clone(), n.statement.clone());
                drop(b);
                self.do_indent();
                self.p("while (");
                self.ev(cx, &e);
                self.println(")");
                self.block(cx, &s);
            }
            Kind::WithStatement(n) => {
                let (e, s) = (n.expr.clone(), n.statement.clone());
                drop(b);
                self.do_indent();
                self.p("with (");
                self.ev(cx, &e);
                self.println(")");
                self.block(cx, &s);
            }
            _ => return false,
        }
        true
    }
}
