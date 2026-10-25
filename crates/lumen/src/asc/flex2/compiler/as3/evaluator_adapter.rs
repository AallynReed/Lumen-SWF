//! Port of `flash.swf.tools.as3.EvaluatorAdapter`, the tree walk `SyntaxTreeEvaluator` and `InheritanceEvaluator`
//! extend. An adapter's overrides go in [`Adapter::visit`]; [`walk`] is `super.evaluate`.

use crate::asc::parser::evaluator::evaluate as asc_evaluate;
use crate::asc::parser::meta_data_evaluator::MetaDataEvaluator;
use crate::asc::parser::node::{Kind, NodeList, NodeRef};
use crate::asc::util::context::ContextRef;

pub trait Adapter {
    /// An overridden `evaluate`; returns false to fall back to the walk.
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool;
}

pub fn evaluate<A: Adapter + ?Sized>(a: &mut A, cx: &ContextRef, node: &NodeRef) {
    if !a.visit(cx, node) {
        walk(a, cx, node);
    }
}

fn list(l: &Option<NodeList>) -> Vec<NodeRef> {
    l.as_ref().map(|l| l.borrow().clone()).unwrap_or_default()
}

fn push(v: &mut Vec<NodeRef>, n: &Option<NodeRef>) {
    if let Some(n) = n {
        v.push(n.clone());
    }
}

/// `EvaluatorAdapter`'s own `evaluate` for this node.
pub fn walk<A: Adapter + ?Sized>(a: &mut A, cx: &ContextRef, node: &NodeRef) {
    if node.borrow().as_statement_list().is_some() {
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
            if let Some(n) = n {
                evaluate(a, cx, &n);
            }
            i += 1;
        }
        return;
    }
    if node.borrow().as_meta_data().is_some() {
        let has_data = node.borrow().as_meta_data().unwrap().data.is_some();
        if has_data {
            asc_evaluate(node, cx, &mut MetaDataEvaluator::new());
        }
        return;
    }
    let mut own_cx: Option<ContextRef> = None;
    let mut c: Vec<NodeRef> = Vec::new();
    {
        let b = node.borrow();
        match &b.kind {
            Kind::Increment(n) => push(&mut c, &n.sel.expr),
            Kind::QualifiedIdentifier(n) => push(&mut c, &n.qualifier),
            Kind::QualifiedExpression(n) => push(&mut c, &n.expr),
            Kind::LiteralXML(n) => push(&mut c, &n.list),
            Kind::FunctionCommon(n) => {
                own_cx = n.cx.clone();
                push(&mut c, &n.signature);
                push(&mut c, &n.body);
            }
            Kind::ParenListExpression(n) => push(&mut c, &n.expr),
            Kind::LiteralObject(n) => push(&mut c, &n.fieldlist),
            Kind::LiteralField(n) => {
                push(&mut c, &n.name);
                push(&mut c, &n.value);
            }
            Kind::LiteralArray(n) => push(&mut c, &n.elementlist),
            Kind::LiteralVector(n) => {
                push(&mut c, &n.type_);
                push(&mut c, &n.elementlist);
            }
            Kind::SuperExpression(n) => push(&mut c, &n.expr),
            Kind::MemberExpression(n) => {
                push(&mut c, &n.base);
                push(&mut c, &n.selector);
            }
            Kind::Invoke(n) => push(&mut c, &n.args),
            Kind::CallExpression(n) => {
                push(&mut c, &n.sel.expr);
                push(&mut c, &n.args);
            }
            Kind::DeleteExpression(n) => push(&mut c, &n.sel.expr),
            Kind::GetExpression(n) => push(&mut c, &n.sel.expr),
            Kind::SetExpression(n) => {
                push(&mut c, &n.sel.expr);
                push(&mut c, &n.args);
            }
            Kind::UnaryExpression(n) => push(&mut c, &n.expr),
            Kind::BinaryExpression(n) => {
                push(&mut c, &n.lhs);
                push(&mut c, &n.rhs);
            }
            Kind::ConditionalExpression(n) => {
                push(&mut c, &n.condition);
                push(&mut c, &n.thenexpr);
                push(&mut c, &n.elseexpr);
            }
            Kind::ArgumentList(n) => c.extend(n.items.iter().flatten().cloned()),
            Kind::List(_) | Kind::ListError(_) => c.extend(b.list().unwrap().items.iter().flatten().cloned()),
            Kind::ExpressionStatement(n) => push(&mut c, &n.expr),
            Kind::SuperStatement(n) => {
                if let Some(call) = &n.call {
                    if let Some(ce) = call.borrow().as_call_expression() {
                        push(&mut c, &ce.args);
                    }
                }
            }
            Kind::LabeledStatement(n) => {
                push(&mut c, &n.label);
                push(&mut c, &n.statement);
            }
            Kind::IfStatement(n) => {
                push(&mut c, &n.condition);
                push(&mut c, &n.thenactions);
                push(&mut c, &n.elseactions);
            }
            Kind::SwitchStatement(n) => {
                push(&mut c, &n.expr);
                push(&mut c, &n.statements);
            }
            Kind::CaseLabel(n) => push(&mut c, &n.label),
            Kind::DoStatement(n) => {
                push(&mut c, &n.expr);
                push(&mut c, &n.statements);
            }
            Kind::WhileStatement(n) => {
                push(&mut c, &n.expr);
                push(&mut c, &n.statement);
            }
            Kind::ForStatement(n) => {
                push(&mut c, &n.initialize);
                push(&mut c, &n.test);
                push(&mut c, &n.increment);
                push(&mut c, &n.statement);
            }
            Kind::WithStatement(n) => {
                push(&mut c, &n.expr);
                push(&mut c, &n.statement);
            }
            Kind::ContinueStatement(n) => push(&mut c, &n.id),
            Kind::BreakStatement(n) => push(&mut c, &n.id),
            Kind::ReturnStatement(n) => push(&mut c, &n.expr),
            Kind::ThrowStatement(n) => push(&mut c, &n.expr),
            Kind::TryStatement(n) => {
                push(&mut c, &n.tryblock);
                push(&mut c, &n.catchlist);
                push(&mut c, &n.finallyblock);
            }
            Kind::CatchClause(n) => {
                push(&mut c, &n.parameter);
                push(&mut c, &n.statements);
            }
            Kind::FinallyClause(n) => push(&mut c, &n.statements),
            Kind::ImportDirective(n) => {
                own_cx = n.cx.clone();
                push(&mut c, &n.attrs);
                push(&mut c, &n.name);
            }
            Kind::AttributeList(n) => c.extend(n.items.iter().flatten().cloned()),
            Kind::VariableDefinition(n) => {
                push(&mut c, &n.def.attrs);
                push(&mut c, &n.list);
            }
            Kind::VariableBinding(n) => {
                push(&mut c, &n.variable);
                push(&mut c, &n.initializer);
            }
            Kind::UntypedVariableBinding(n) => {
                push(&mut c, &n.identifier);
                push(&mut c, &n.initializer);
            }
            Kind::TypedIdentifier(n) => {
                push(&mut c, &n.identifier);
                push(&mut c, &n.type_);
            }
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => {
                let n = b.function_definition().unwrap();
                own_cx = n.cx.clone();
                push(&mut c, &n.def.attrs);
                push(&mut c, &n.name);
                push(&mut c, &n.fexpr);
            }
            Kind::FunctionName(n) => push(&mut c, &n.identifier),
            Kind::FunctionSignature(n) => {
                push(&mut c, &n.parameter);
                push(&mut c, &n.result);
            }
            Kind::Parameter(_) | Kind::RestParameter(_) => {
                if let Kind::RestParameter(r) = &b.kind {
                    push(&mut c, &r.parameter);
                } else {
                    let n = b.parameter().unwrap();
                    push(&mut c, &n.identifier);
                    push(&mut c, &n.type_);
                }
            }
            Kind::RestExpression(n) => push(&mut c, &n.expr),
            Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) => {
                let n = b.class_definition().unwrap();
                own_cx = n.cx.clone();
                push(&mut c, &n.def.attrs);
                push(&mut c, &n.name);
                push(&mut c, &n.baseclass);
                push(&mut c, &n.interfaces);
                c.extend(list(&n.fexprs));
                c.extend(list(&n.staticfexprs));
                c.extend(list(&n.instanceinits));
                push(&mut c, &n.statements);
            }
            Kind::InterfaceDefinition(n) => {
                push(&mut c, &n.cls.def.attrs);
                push(&mut c, &n.cls.name);
                push(&mut c, &n.cls.interfaces);
                push(&mut c, &n.cls.statements);
            }
            Kind::ClassName(n) => {
                push(&mut c, &n.pkgname);
                push(&mut c, &n.ident);
            }
            Kind::Inheritance(n) => {
                push(&mut c, &n.baseclass);
                push(&mut c, &n.interfaces);
            }
            Kind::NamespaceDefinition(n) => {
                push(&mut c, &n.def.attrs);
                push(&mut c, &n.name);
                push(&mut c, &n.value);
            }
            Kind::PackageIdentifiers(n) => c.extend(n.list.iter().flatten().cloned()),
            Kind::PackageName(n) => push(&mut c, &n.id),
            Kind::Program(_) | Kind::BinaryProgram(_) => {
                let n = b.program().unwrap();
                own_cx = n.cx.clone();
                c.extend(n.pkgdefs.iter().cloned());
                push(&mut c, &n.statements);
                c.extend(list(&n.fexprs));
                c.extend(list(&n.clsdefs));
            }
            Kind::StoreRegister(n) => push(&mut c, &n.expr),
            Kind::Box(n) => push(&mut c, &n.expr),
            Kind::Coerce(n) => push(&mut c, &n.expr),
            Kind::Pragma(n) => push(&mut c, &n.list),
            Kind::PragmaExpression(n) => push(&mut c, &n.identifier),
            Kind::ParameterList(n) => c.extend(n.items.iter().flatten().cloned()),
            _ => {}
        }
    }
    let cx = own_cx.unwrap_or_else(|| cx.clone());
    for n in c {
        evaluate(a, &cx, &n);
    }
}
