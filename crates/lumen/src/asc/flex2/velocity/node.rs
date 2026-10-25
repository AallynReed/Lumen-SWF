//! Port of `SimpleNode` and the `AST*` node classes' runtime behaviour (`init`, `render`, `value`, `evaluate`, `execute`,
//! `setValue`), over trees as `SerializedTemplateFactory` stores them: parse-time fields kept, tokens and transient state gone.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::context::{Ica, IcacheThingy, IntrospectionCacheData};
use super::directive::{self, DirectiveInst};
use super::introspection::{self, Info, InvokeError, JavaException, Value};
use super::parser::*;
use super::runtime::RuntimeInstance;
use super::VelocityError;

pub type Sink = String;

pub(crate) enum Kind {
    Plain,
    Text(String),
    Comment(String),
    Escape(String),
    Identifier { identifier: String, uber_info: RefCell<Option<Info>> },
    Method { method_name: String, param_count: Cell<i32> },
    NumberLiteral(i32),
    StringLiteral { image: String, interpolate: Cell<bool>, node_tree: RefCell<Option<Rc<Node>>> },
    Directive { directive_name: String, directive: RefCell<Option<Rc<DirectiveInst>>> },
    Reference { info: RefInfo, uber_info: RefCell<Option<Info>> },
    SetDirective { left_reference: String, blather: Cell<bool> },
}

pub struct Node {
    pub id: i32,
    pub children: Vec<Rc<Node>>,
    pub first_token_image: Option<String>,
    pub begin_line: i32,
    pub begin_column: i32,
    /// `SimpleNode.literal()` while the parse tokens still exist (the build-time init); `None` is a NullPointerException.
    literal: RefCell<Option<String>>,
    pub(crate) kind: Kind,
}

fn npe() -> VelocityError {
    VelocityError::Runtime("java.lang.NullPointerException".to_string())
}

fn method_invocation(method_name: &str, o: &Value, e: &JavaException) -> VelocityError {
    VelocityError::MethodInvocation {
        message: format!(
            "Invocation of method '{}' in  class {} threw exception class {} : {}",
            method_name,
            o.class_name().unwrap_or_default(),
            e.class_name,
            e.message.as_deref().unwrap_or("null")
        ),
        method_name: method_name.to_string(),
    }
}

/// Builds the node tree from a parse, with `literal()` still available.
pub fn from_parse(pr: &ParseResult) -> Rc<Node> {
    convert(pr, pr.root)
}

fn convert(pr: &ParseResult, n: usize) -> Rc<Node> {
    let p = &pr.nodes[n];
    let children = p.children.iter().map(|&c| convert(pr, c)).collect();
    let kind = match &p.data {
        PData::None => Kind::Plain,
        PData::Text(t) => Kind::Text(t.clone()),
        PData::Comment(c) => Kind::Comment(c.clone()),
        PData::Escape(v) => Kind::Escape(v.clone()),
        PData::Identifier(i) => Kind::Identifier { identifier: i.clone(), uber_info: RefCell::new(None) },
        PData::Method(m) => Kind::Method { method_name: m.clone(), param_count: Cell::new(0) },
        PData::NumberLiteral(v) => Kind::NumberLiteral(*v),
        PData::StringLiteral { image, interpolate } => Kind::StringLiteral {
            image: image.clone(),
            interpolate: Cell::new(*interpolate),
            node_tree: RefCell::new(None),
        },
        PData::Directive(name) => Kind::Directive { directive_name: name.clone(), directive: RefCell::new(None) },
        PData::Reference(info) => Kind::Reference { info: info.clone(), uber_info: RefCell::new(None) },
        PData::SetDirective(l) => Kind::SetDirective { left_reference: l.clone(), blather: Cell::new(false) },
    };
    Rc::new(Node {
        id: p.id,
        children,
        first_token_image: p.first_token_image.clone(),
        begin_line: p.begin_line,
        begin_column: p.begin_column,
        literal: RefCell::new(simple_literal(p, &pr.tokens)),
        kind,
    })
}

/// `RuntimeInstance.parse`, as a node tree.
pub fn parse_tree(source: &str, template_name: &str, rsvc: &RuntimeInstance) -> Result<Rc<Node>, VelocityError> {
    let pr = parse(source, template_name, rsvc).map_err(|e| VelocityError::Parse(e.0))?;
    Ok(from_parse(&pr))
}

impl Node {
    pub(crate) fn key(&self) -> usize {
        self as *const Node as usize
    }

    /// Serialization: drops the tokens and every transient field.
    pub(crate) fn strip_transient(&self) {
        self.literal.replace(None);
        match &self.kind {
            Kind::Directive { directive, .. } => {
                directive.replace(None);
            }
            Kind::Method { param_count, .. } => param_count.set(0),
            Kind::SetDirective { blather, .. } => blather.set(false),
            Kind::StringLiteral { node_tree, .. } => {
                if let Some(t) = &*node_tree.borrow() {
                    t.strip_transient();
                }
            }
            _ => {}
        }
        for c in &self.children {
            c.strip_transient();
        }
    }

    pub fn literal(&self) -> Result<String, VelocityError> {
        if let Kind::Reference { info, .. } = &self.kind {
            if let Some(l) = &info.literal {
                return Ok(l.clone());
            }
        }
        self.literal.borrow().clone().ok_or_else(npe)
    }

    fn init_children(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<(), VelocityError> {
        for c in &self.children {
            c.init(rsvc, cx)?;
        }
        Ok(())
    }

    pub fn init(self: &Rc<Node>, rsvc: &RuntimeInstance, cx: Ica) -> Result<(), VelocityError> {
        match &self.kind {
            Kind::Text(_) | Kind::Escape(_) => Ok(()),
            Kind::Reference { uber_info, .. } | Kind::Identifier { uber_info, .. } => {
                self.init_children(rsvc, cx)?;
                if uber_info.borrow().is_none() {
                    uber_info.replace(Some(Info::new(&cx.current_template_name(), self.begin_line, self.begin_column)));
                }
                Ok(())
            }
            Kind::Method { param_count, .. } => {
                self.init_children(rsvc, cx)?;
                param_count.set(self.children.len() as i32 - 1);
                Ok(())
            }
            Kind::SetDirective { blather, .. } => {
                self.init_children(rsvc, cx)?;
                blather.set(true);
                Ok(())
            }
            Kind::StringLiteral { image, interpolate, node_tree } => {
                self.init_children(rsvc, cx)?;
                let tree = node_tree.borrow().clone();
                if let Some(t) = tree {
                    return t.init(rsvc, cx);
                }
                if interpolate.get() {
                    let t = parse_tree(image, "StringLiteral", rsvc)
                        .map_err(|e| VelocityError::Runtime(format!("java.lang.RuntimeException: {e}")))?;
                    node_tree.replace(Some(t));
                }
                Ok(())
            }
            Kind::Directive { directive_name, directive } => {
                self.init_children(rsvc, cx)?;
                let d = directive::create(rsvc, cx, self, directive_name)?;
                directive.replace(d.map(Rc::new));
                self.init_children(rsvc, cx)
            }
            _ => self.init_children(rsvc, cx),
        }
    }

    pub fn render(self: &Rc<Node>, rsvc: &RuntimeInstance, cx: Ica, w: &mut Sink) -> Result<bool, VelocityError> {
        #[cfg(test)]
        super::tests::RENDERED.with(|r| r.borrow_mut().insert(self.key()));
        match &self.kind {
            Kind::Text(t) | Kind::Comment(t) | Kind::Escape(t) => {
                w.push_str(t);
                Ok(true)
            }
            Kind::Directive { directive_name, directive } => {
                let d = directive.borrow().clone();
                match d {
                    Some(d) => {
                        d.render(rsvc, cx, w, self)?;
                    }
                    None => {
                        w.push('#');
                        w.push_str(directive_name);
                    }
                }
                Ok(true)
            }
            Kind::Reference { info, .. } => self.render_reference(info, rsvc, cx, w),
            Kind::SetDirective { left_reference, blather } => {
                let right = &self.children[1];
                let left = &self.children[0];
                let value = right.value(rsvc, cx)?;
                if value.is_null() {
                    if blather.get() {
                        let mut doit = true;
                        if let Some(ec) = cx.event_cartridge() {
                            let (l, r) = (left.literal()?, right.literal()?);
                            doit = ec.should_log_on_null_set(&l, &r);
                        }
                        if doit {
                            rsvc.error(&format!(
                                "RHS of #set statement is null. Context will not be modified. {} [line {}, column {}]",
                                cx.current_template_name(),
                                self.begin_line,
                                self.begin_column
                            ));
                        }
                    }
                    return Ok(false);
                }
                if left.children.is_empty() {
                    cx.put(left_reference, value)?;
                } else {
                    left.set_value(rsvc, cx, value)?;
                }
                Ok(true)
            }
            _ => match self.id {
                JJTESCAPEDDIRECTIVE => {
                    w.push_str(self.first_token_image.as_deref().unwrap_or("null"));
                    Ok(true)
                }
                JJTIFSTATEMENT => {
                    if self.children[0].evaluate(rsvc, cx)? {
                        self.children[1].render(rsvc, cx, w)?;
                        return Ok(true);
                    }
                    for c in &self.children[2..] {
                        if c.evaluate(rsvc, cx)? {
                            c.render(rsvc, cx, w)?;
                            return Ok(true);
                        }
                    }
                    Ok(true)
                }
                JJTELSEIFSTATEMENT => self.children[1].render(rsvc, cx, w),
                _ => {
                    for c in &self.children {
                        c.render(rsvc, cx, w)?;
                    }
                    Ok(true)
                }
            },
        }
    }

    fn render_reference(
        &self,
        info: &RefInfo,
        rsvc: &RuntimeInstance,
        cx: Ica,
        w: &mut Sink,
    ) -> Result<bool, VelocityError> {
        if info.reference_type == RUNT {
            w.push_str(&info.root_string);
            return Ok(true);
        }
        let mut value = self.execute(&Value::Null, rsvc, cx)?;
        if info.escaped {
            w.push_str(&info.esc_prefix);
            if value.is_null() {
                w.push('\\');
            }
            w.push_str(&info.null_string);
            return Ok(true);
        }
        if let Some(ec) = cx.event_cartridge() {
            value = ec.reference_insert(info.literal.as_deref().unwrap_or("null"), value);
        }
        if value.is_null() {
            w.push_str(&info.esc_prefix);
            w.push_str(&info.esc_prefix);
            w.push_str(&info.more_prefix);
            w.push_str(&info.null_string);
            if info.reference_type != QUIET_REFERENCE {
                rsvc.warn(&format!(
                    "org.apache.flex.forks.velocity.runtime.exception.ReferenceException: reference : template = {} [line {},column {}] : {} is not a valid reference.",
                    cx.current_template_name(),
                    self.begin_line,
                    self.begin_column,
                    self.literal()?
                ));
            }
        } else {
            w.push_str(&info.esc_prefix);
            w.push_str(&info.more_prefix);
            w.push_str(&value.to_java_string());
        }
        Ok(true)
    }

    pub fn execute(&self, o: &Value, rsvc: &RuntimeInstance, cx: Ica) -> Result<Value, VelocityError> {
        match &self.kind {
            Kind::Reference { info, .. } => {
                if info.reference_type == RUNT {
                    return Ok(Value::Null);
                }
                let mut result = cx.get(&info.root_string)?;
                if result.is_null() {
                    return Ok(Value::Null);
                }
                for c in &self.children {
                    match c.execute(&result, rsvc, cx) {
                        Ok(r) => result = r,
                        Err(VelocityError::MethodInvocation { message, method_name }) => {
                            rsvc.error(&format!(
                                "Method {} threw exception for reference ${} in template {} at  [{},{}]",
                                method_name,
                                info.root_string,
                                cx.current_template_name(),
                                self.begin_line,
                                self.begin_column
                            ));
                            return Err(VelocityError::MethodInvocation { message, method_name });
                        }
                        Err(e) => return Err(e),
                    }
                    if result.is_null() {
                        return Ok(Value::Null);
                    }
                }
                Ok(result)
            }
            Kind::Identifier { identifier, uber_info } => self.execute_identifier(o, identifier, uber_info, rsvc, cx),
            Kind::Method { method_name, param_count } => {
                self.execute_method(o, method_name, param_count.get(), rsvc, cx)
            }
            _ => Ok(Value::Null),
        }
    }

    fn execute_identifier(
        &self,
        o: &Value,
        identifier: &str,
        uber_info: &RefCell<Option<Info>>,
        rsvc: &RuntimeInstance,
        cx: Ica,
    ) -> Result<Value, VelocityError> {
        let c = o.class_name().unwrap_or_default();
        let vg = match cx.icache_get(self.key()) {
            Some(IntrospectionCacheData { context_data, thingy: IcacheThingy::Getter(g) }) if context_data == c => g,
            _ => {
                let g = introspection::get_property_get(rsvc, o, identifier);
                if g.method.is_none() {
                    let info = uber_info.borrow().clone().unwrap_or_else(|| Info::new("", 0, 0));
                    rsvc.strict_get_miss(&info, Some(identifier), &c);
                }
                cx.icache_put(
                    self.key(),
                    IntrospectionCacheData { context_data: c.clone(), thingy: IcacheThingy::Getter(g.clone()) },
                );
                g
            }
        };
        match vg.invoke(o) {
            Ok(v) => Ok(v),
            Err(InvokeError::IllegalArgument(_)) => Ok(Value::Null),
            Err(InvokeError::Target(e)) => {
                let name = vg.method_name().unwrap_or_default();
                match cx.event_cartridge() {
                    Some(ec) => ec.method_exception(&c, &name, &e).map_err(|e2| method_invocation(&name, o, &e2)),
                    None => Err(method_invocation(&name, o, &e)),
                }
            }
        }
    }

    fn execute_method(
        &self,
        o: &Value,
        method_name: &str,
        param_count: i32,
        rsvc: &RuntimeInstance,
        cx: Ica,
    ) -> Result<Value, VelocityError> {
        let mut params = Vec::new();
        for j in 0..param_count.max(0) as usize {
            match self.children[j + 1].value(rsvc, cx) {
                Ok(v) => params.push(v),
                Err(e @ VelocityError::MethodInvocation { .. }) => return Err(e),
                Err(e) => {
                    rsvc.error(&format!("ASTMethod.execute() : exception from introspection : {e}"));
                    return Ok(Value::Null);
                }
            }
        }
        let c = o.class_name().unwrap_or_default();
        let method = match cx.icache_get(self.key()) {
            Some(IntrospectionCacheData { context_data, thingy: IcacheThingy::Method(m) }) if context_data == c => {
                Some(m)
            }
            _ => {
                let m = introspection::get_method(rsvc, o, method_name, &params);
                if let Some(m) = &m {
                    cx.icache_put(
                        self.key(),
                        IntrospectionCacheData { context_data: c.clone(), thingy: IcacheThingy::Method(m.clone()) },
                    );
                }
                m
            }
        };
        let Some(method) = method else { return Ok(Value::Null) };
        match introspection::invoke(o, &method, &params) {
            Ok(v) => {
                if v.is_null() && method.ret == introspection::JType::Void {
                    return Ok(Value::str(""));
                }
                Ok(v)
            }
            Err(InvokeError::Target(e)) => match cx.event_cartridge() {
                Some(ec) => {
                    ec.method_exception(&c, method_name, &e).map_err(|e2| method_invocation(method_name, o, &e2))
                }
                None => Err(method_invocation(method_name, o, &e)),
            },
            Err(InvokeError::IllegalArgument(e)) => {
                rsvc.error(&format!(
                    "ASTMethod.execute() : exception invoking method '{method_name}' in class {c} : {e}"
                ));
                Ok(Value::Null)
            }
        }
    }

    /// `ASTReference.setValue`.
    fn set_value(&self, rsvc: &RuntimeInstance, cx: Ica, value: Value) -> Result<bool, VelocityError> {
        let Kind::Reference { info, uber_info } = &self.kind else { return Ok(false) };
        if self.children.is_empty() {
            cx.put(&info.root_string, value)?;
            return Ok(true);
        }
        let reference_set_error = |cx: Ica| -> Result<(), VelocityError> {
            rsvc.error(&format!(
                "org.apache.flex.forks.velocity.runtime.exception.ReferenceException: reference set : template = {} [line {},column {}] : {} is not a valid reference.",
                cx.current_template_name(),
                self.begin_line,
                self.begin_column,
                self.literal()?
            ));
            Ok(())
        };
        let mut result = cx.get(&info.root_string)?;
        if result.is_null() {
            reference_set_error(cx)?;
            return Ok(false);
        }
        for c in &self.children[..info.num_children as usize - 1] {
            result = c.execute(&result, rsvc, cx)?;
            if result.is_null() {
                reference_set_error(cx)?;
                return Ok(false);
            }
        }
        let vs = introspection::get_property_set(rsvc, &result, info.identifier.as_deref(), &value);
        let Some(vs) = vs else {
            let ui = uber_info.borrow().clone().unwrap_or_else(|| Info::new("", 0, 0));
            rsvc.strict_set_miss(&ui, info.identifier.as_deref(), &result.class_name().unwrap_or_default());
            return Ok(false);
        };
        match vs.invoke(&result, value) {
            Ok(_) => Ok(true),
            Err(InvokeError::Target(e)) => Err(VelocityError::MethodInvocation {
                message: format!(
                    "ASTReference : Invocation of method '{}' in  class {} threw exception class {}",
                    info.identifier.as_deref().unwrap_or("null"),
                    result.class_name().unwrap_or_default(),
                    e.class_name
                ),
                method_name: info.identifier.clone().unwrap_or_else(|| "null".to_string()),
            }),
            Err(InvokeError::IllegalArgument(e)) => {
                rsvc.error(&format!(
                    "ASTReference setValue() : exception : {} template = {} [{},{}]",
                    e,
                    cx.current_template_name(),
                    self.begin_line,
                    self.begin_column
                ));
                Ok(false)
            }
        }
    }

    pub fn evaluate(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<bool, VelocityError> {
        match &self.kind {
            Kind::Reference { .. } => {
                let v = self.execute(&Value::Null, rsvc, cx)?;
                Ok(match v {
                    Value::Null => false,
                    Value::Boolean(b) => b,
                    _ => true,
                })
            }
            _ => match self.id {
                JJTEXPRESSION | JJTELSEIFSTATEMENT => self.children[0].evaluate(rsvc, cx),
                JJTELSESTATEMENT | JJTTRUE => Ok(true),
                JJTNOTNODE => Ok(!self.children[0].evaluate(rsvc, cx)?),
                JJTANDNODE => Ok(self.children[0].evaluate(rsvc, cx)? && self.children[1].evaluate(rsvc, cx)?),
                JJTORNODE => Ok(self.children[0].evaluate(rsvc, cx)? || self.children[1].evaluate(rsvc, cx)?),
                JJTEQNODE | JJTNENODE => self.equality(rsvc, cx),
                JJTLTNODE | JJTGTNODE | JJTLENODE | JJTGENODE => self.relational(rsvc, cx),
                _ => Ok(false),
            },
        }
    }

    fn null_side(&self, left: &Value) -> Result<String, VelocityError> {
        self.children[if left.is_null() { 0 } else { 1 }].literal()
    }

    fn equality(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<bool, VelocityError> {
        let eq = self.id == JJTEQNODE;
        let left = self.children[0].value(rsvc, cx)?;
        let right = self.children[1].value(rsvc, cx)?;
        let side = if left.is_null() { "Left" } else { "Right" };
        if left.is_null() || right.is_null() {
            let lit = self.null_side(&left)?;
            if eq {
                rsvc.error(&format!(
                    "{side} side ({lit}) of '==' operation has null value. If a reference, it may not be in the context. Operation not possible. {} [line {}, column {}]",
                    cx.current_template_name(), self.begin_line, self.begin_column
                ));
            } else {
                rsvc.error(&format!(
                    "{side} side ({lit}) of '!=' operation has null value. Operation not possible. {} [line {}, column {}]",
                    cx.current_template_name(), self.begin_line, self.begin_column
                ));
            }
            return Ok(false);
        }
        let (lc, rc) = (left.class_name().unwrap(), right.class_name().unwrap());
        if lc == rc {
            let e = left.java_equals(&right);
            return Ok(if eq { e } else { !e });
        }
        rsvc.error(&format!(
            "Error in evaluation of {} expression. Both arguments must be of the same Class. Currently left = class {lc}, right = class {rc}. {} [line {}, column {}] (ASTEQNode)",
            if eq { "==" } else { "!=" },
            cx.current_template_name(),
            self.begin_line,
            self.begin_column
        ));
        Ok(false)
    }

    fn relational(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<bool, VelocityError> {
        let op = match self.id {
            JJTLTNODE => "<",
            JJTGTNODE => ">",
            JJTLENODE => "<=",
            _ => ">=",
        };
        let left = self.children[0].value(rsvc, cx)?;
        let right = self.children[1].value(rsvc, cx)?;
        if left.is_null() || right.is_null() {
            let lit = self.null_side(&left)?;
            rsvc.error(&format!(
                "{} side ({lit}) of '{op}' operation has null value. Operation not possible. {} [line {}, column {}]",
                if left.is_null() { "Left" } else { "Right" },
                cx.current_template_name(),
                self.begin_line,
                self.begin_column
            ));
            return Ok(false);
        }
        let (Value::Integer(l), Value::Integer(r)) = (&left, &right) else {
            let bad = if matches!(left, Value::Integer(_)) { &right } else { &left };
            rsvc.error(&format!(
                "{} side of '{op}' operation is not a valid type.  It is a class {}. Currently only integers (1,2,3...) and Integer type is supported. {} [line {}, column {}]",
                if matches!(left, Value::Integer(_)) { "Right" } else { "Left" },
                bad.class_name().unwrap_or_default(),
                cx.current_template_name(),
                self.begin_line,
                self.begin_column
            ));
            return Ok(false);
        };
        Ok(match self.id {
            JJTLTNODE => l < r,
            JJTGTNODE => l > r,
            JJTLENODE => l <= r,
            _ => l >= r,
        })
    }

    fn arithmetic(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<Value, VelocityError> {
        let what = match self.id {
            JJTADDNODE => "addition",
            JJTSUBTRACTNODE => "subtraction",
            JJTMULNODE => "multiplication",
            JJTDIVNODE => "division",
            _ => "modulus",
        };
        let left = self.children[0].value(rsvc, cx)?;
        let right = self.children[1].value(rsvc, cx)?;
        let name = cx.current_template_name();
        let (line, col) = (self.begin_line, self.begin_column);
        if left.is_null() || right.is_null() {
            let lit = self.null_side(&left)?;
            rsvc.error(&format!(
                "{} side ({lit}) of {what} operation has null value. Operation not possible. {name} [line {line}, column {col}]",
                if left.is_null() { "Left" } else { "Right" }
            ));
            return Ok(Value::Null);
        }
        let (Value::Integer(l), Value::Integer(r)) = (&left, &right) else {
            rsvc.error(&format!(
                "{} side of {what} operation is not a valid type. Currently only integers (1,2,3...) and Integer type is supported. {name} [line {line}, column {col}]",
                if matches!(left, Value::Integer(_)) { "Right" } else { "Left" }
            ));
            return Ok(Value::Null);
        };
        if (self.id == JJTDIVNODE || self.id == JJTMODNODE) && *r == 0 {
            rsvc.error(&format!(
                "Right side of {what} operation is zero. Must be non-zero. {name} [line {line}, column {col}]"
            ));
            return Ok(Value::Null);
        }
        Ok(Value::Integer(match self.id {
            JJTADDNODE => l.wrapping_add(*r),
            JJTSUBTRACTNODE => l.wrapping_sub(*r),
            JJTMULNODE => l.wrapping_mul(*r),
            JJTDIVNODE => l.wrapping_div(*r),
            _ => l.wrapping_rem(*r),
        }))
    }

    pub fn value(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<Value, VelocityError> {
        match &self.kind {
            Kind::Reference { info, .. } => {
                if info.computable_reference {
                    self.execute(&Value::Null, rsvc, cx)
                } else {
                    Ok(Value::Null)
                }
            }
            Kind::NumberLiteral(v) => Ok(Value::Integer(*v)),
            Kind::StringLiteral { image, interpolate, node_tree } => {
                if interpolate.get() {
                    let tree = node_tree.borrow().clone();
                    let mut writer = String::new();
                    let r = match &tree {
                        Some(t) => t.render(rsvc, cx, &mut writer).map(|_| ()),
                        None => Err(npe()),
                    };
                    match r {
                        Ok(()) => return Ok(Value::str(&writer)),
                        Err(e) => rsvc.error(&format!("Error in interpolating string literal : {e}")),
                    }
                }
                Ok(Value::str(image))
            }
            _ => match self.id {
                JJTEXPRESSION => self.children[0].value(rsvc, cx),
                JJTTRUE => Ok(Value::Boolean(true)),
                JJTFALSE => Ok(Value::Boolean(false)),
                JJTNOTNODE => Ok(Value::Boolean(!self.children[0].evaluate(rsvc, cx)?)),
                JJTANDNODE | JJTORNODE | JJTEQNODE | JJTNENODE | JJTLTNODE | JJTGTNODE | JJTLENODE | JJTGENODE => {
                    Ok(Value::Boolean(self.evaluate(rsvc, cx)?))
                }
                JJTADDNODE | JJTSUBTRACTNODE | JJTMULNODE | JJTDIVNODE | JJTMODNODE => self.arithmetic(rsvc, cx),
                JJTOBJECTARRAY => {
                    let mut items = Vec::new();
                    for c in &self.children {
                        items.push(c.value(rsvc, cx)?);
                    }
                    Ok(Value::list(items))
                }
                JJTINTEGERRANGE => self.integer_range(rsvc, cx),
                _ => Ok(Value::Null),
            },
        }
    }

    fn integer_range(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<Value, VelocityError> {
        let left = self.children[0].value(rsvc, cx)?;
        let right = self.children[1].value(rsvc, cx)?;
        let name = cx.current_template_name();
        let (line, col) = (self.begin_line, self.begin_column);
        if left.is_null() || right.is_null() {
            rsvc.error(&format!(
                "{} side of range operator [n..m] has null value. Operation not possible. {name} [line {line}, column {col}]",
                if left.is_null() { "Left" } else { "Right" }
            ));
            return Ok(Value::Null);
        }
        let (Value::Integer(l), Value::Integer(r)) = (&left, &right) else {
            rsvc.error(&format!(
                "{} side of range operator is not a valid type. Currently only integers (1,2,3...) and Integer type is supported. {name} [line {line}, column {col}]",
                if matches!(left, Value::Integer(_)) { "Right" } else { "Left" }
            ));
            return Ok(Value::Null);
        };
        let num = l.wrapping_sub(*r).wrapping_abs().wrapping_add(1);
        let delta = if l >= r { -1 } else { 1 };
        let mut items = Vec::new();
        let mut val = *l;
        for _ in 0..num {
            items.push(Value::Integer(val));
            val = val.wrapping_add(delta);
        }
        Ok(Value::list(items))
    }
}
