//! Port of `macromedia/asc/parser/Node.java` and every `*Node.java` class of that package.

#![allow(non_upper_case_globals)]

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

use crate::asc::parser::tokens::*;
use crate::asc::semantics::value::*;
pub use crate::asc::util::context::ContextRef;
use crate::asc::util::context::Context;

pub type NodeRef = Rc<RefCell<Node>>;
/// An `ObjectList<...Node>` that Java shares between a node and an evaluator's stack.
pub type NodeList = Rc<RefCell<Vec<NodeRef>>>;

pub fn new_node_list() -> NodeList {
    Rc::new(RefCell::new(Vec::new()))
}

fn rb(n: &Option<NodeRef>) -> Ref<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow()
}

fn rbm(n: &Option<NodeRef>) -> RefMut<'_, Node> {
    n.as_ref().expect("NullPointerException").borrow_mut()
}

fn last_item(items: &[Option<NodeRef>]) -> Option<NodeRef> {
    items.last().cloned().flatten()
}

fn same_node(a: &Option<NodeRef>, b: &Option<NodeRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

pub const MAX_DEF_BITS: i32 = 1000;

const IS_ANY_FLAG: i32 = 1;
const IS_ATTR_FLAG: i32 = 2;
const POSITION_SHIFT: i32 = 2;
const POSITION_MASK: i32 = 0xFFFFFFC;

pub struct Node {
    pub flags: i32,
    pub block: Option<BlockRef>,
    pub kind: Kind,
}

pub enum Kind {
    ApplyTypeExpr(ApplyTypeExprNode),
    ArgumentList(ArgumentListNode),
    AttributeList(AttributeListNode),
    BinaryClassDef(BinaryClassDefNode),
    BinaryExpression(BinaryExpressionNode),
    BinaryFunctionDefinition(BinaryFunctionDefinitionNode),
    BinaryInterfaceDefinition(BinaryInterfaceDefinitionNode),
    BinaryProgram(BinaryProgramNode),
    Block(BlockNode),
    Box(BoxNode),
    BreakStatement(BreakStatementNode),
    CallExpression(CallExpressionNode),
    CaseLabel(CaseLabelNode),
    CatchClause(CatchClauseNode),
    ClassDefinition(ClassDefinitionNode),
    ClassName(ClassNameNode),
    Coerce(CoerceNode),
    Comment(CommentNode),
    ConditionalExpression(ConditionalExpressionNode),
    ConfigNamespaceDefinition(ConfigNamespaceDefinitionNode),
    ContinueStatement(ContinueStatementNode),
    DefaultXMLNamespace(DefaultXMLNamespaceNode),
    DeleteExpression(DeleteExpressionNode),
    DoStatement(DoStatementNode),
    DocComment(DocCommentNode),
    EmptyElement(EmptyElementNode),
    EmptyStatement(EmptyStatementNode),
    Error(ErrorNode),
    ExpressionStatement(ExpressionStatementNode),
    FinallyClause(FinallyClauseNode),
    ForStatement(ForStatementNode),
    FunctionCommon(FunctionCommonNode),
    FunctionDefinition(FunctionDefinitionNode),
    FunctionName(FunctionNameNode),
    FunctionSignature(FunctionSignatureNode),
    GetExpression(GetExpressionNode),
    HasNext(HasNextNode),
    IdentifierError(IdentifierErrorNode),
    Identifier(IdentifierNode),
    IfStatement(IfStatementNode),
    ImportDirective(ImportDirectiveNode),
    Import(ImportNode),
    IncludeDirective(IncludeDirectiveNode),
    Increment(IncrementNode),
    Inheritance(InheritanceNode),
    InterfaceDefinition(InterfaceDefinitionNode),
    Invoke(InvokeNode),
    LabeledStatement(LabeledStatementNode),
    ListError(ListErrorNode),
    List(ListNode),
    LiteralArray(LiteralArrayNode),
    LiteralBoolean(LiteralBooleanNode),
    LiteralField(LiteralFieldNode),
    LiteralNull(LiteralNullNode),
    LiteralNumber(LiteralNumberNode),
    LiteralObject(LiteralObjectNode),
    LiteralRegExp(LiteralRegExpNode),
    LiteralString(LiteralStringNode),
    LiteralVector(LiteralVectorNode),
    LiteralXML(LiteralXMLNode),
    LoadRegister(LoadRegisterNode),
    MemberExpression(MemberExpressionNode),
    MetaData(MetaDataNode),
    NamespaceDefinition(NamespaceDefinitionNode),
    PackageDefinition(PackageDefinitionNode),
    PackageIdentifiers(PackageIdentifiersNode),
    PackageName(PackageNameNode),
    ParameterList(ParameterListNode),
    Parameter(ParameterNode),
    ParenExpression(ParenExpressionNode),
    ParenListExpression(ParenListExpressionNode),
    PragmaExpression(PragmaExpressionNode),
    Pragma(PragmaNode),
    Program(ProgramNode),
    QualifiedExpression(QualifiedExpressionNode),
    QualifiedIdentifier(QualifiedIdentifierNode),
    Register(RegisterNode),
    RestExpression(RestExpressionNode),
    RestParameter(RestParameterNode),
    ReturnStatement(ReturnStatementNode),
    SetExpression(SetExpressionNode),
    StatementList(StatementListNode),
    StoreRegister(StoreRegisterNode),
    SuperExpression(SuperExpressionNode),
    SuperStatement(SuperStatementNode),
    SwitchStatement(SwitchStatementNode),
    ThisExpression(ThisExpressionNode),
    ThrowStatement(ThrowStatementNode),
    ToObject(ToObjectNode),
    TryStatement(TryStatementNode),
    TypeExpression(TypeExpressionNode),
    TypeIdentifier(TypeIdentifierNode),
    TypedIdentifier(TypedIdentifierNode),
    UnaryExpression(UnaryExpressionNode),
    UntypedVariableBinding(UntypedVariableBindingNode),
    UseDirective(UseDirectiveNode),
    UseNumeric(UseNumericNode),
    UsePragma(UsePragmaNode),
    UsePrecision(UsePrecisionNode),
    UseRounding(UseRoundingNode),
    VariableBinding(VariableBindingNode),
    VariableDefinition(VariableDefinitionNode),
    WhileStatement(WhileStatementNode),
    WithStatement(WithStatementNode),
}

impl Node {
    pub fn get_gen_bits(&self) -> Option<BitSet> {
        match &self.kind {
            Kind::ExpressionStatement(n) => BitSet::or(rb(&n.expr).get_gen_bits(), n.gen_bits.clone()),
            Kind::List(ListNode { items, .. })
            | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. })
            | Kind::StatementList(StatementListNode { items, .. }) => {
                let mut genbits = None;
                for n in items {
                    let n = rb(n);
                    genbits = BitSet::reset_set(genbits, n.get_kill_bits(), n.get_gen_bits());
                }
                genbits
            }
            Kind::MemberExpression(n) => rb(&n.selector).get_gen_bits(),
            Kind::NamespaceDefinition(n) | Kind::ConfigNamespaceDefinition(ConfigNamespaceDefinitionNode { ns: n }) => {
                n.gen_bits.clone()
            }
            Kind::SetExpression(n) => n.gen_bits.clone(),
            Kind::WhileStatement(n) => n.statement.as_ref().and_then(|s| s.borrow().get_gen_bits()),
            _ => None,
        }
    }

    pub fn get_kill_bits(&self) -> Option<BitSet> {
        match &self.kind {
            Kind::ExpressionStatement(n) => {
                let kb = rb(&n.expr).get_kill_bits();
                match n.ref_.as_ref().and_then(|r| r.slot()) {
                    Some(slot) => match slot.get_def_bits() {
                        Some(def_bits) => BitSet::or(kb, BitSet::xor(Some(def_bits), n.gen_bits.clone())),
                        None => BitSet::or(kb, n.gen_bits.clone()),
                    },
                    None => kb,
                }
            }
            Kind::List(ListNode { items, .. })
            | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. })
            | Kind::StatementList(StatementListNode { items, .. }) => {
                let mut killbits = None;
                for n in items {
                    let n = rb(n);
                    killbits = BitSet::reset_set(killbits, n.get_gen_bits(), n.get_kill_bits());
                }
                killbits
            }
            Kind::MemberExpression(n) => rb(&n.selector).get_kill_bits(),
            Kind::NamespaceDefinition(NamespaceDefinitionNode { ref_, gen_bits, .. })
            | Kind::ConfigNamespaceDefinition(ConfigNamespaceDefinitionNode {
                ns: NamespaceDefinitionNode { ref_, gen_bits, .. },
            })
            | Kind::SetExpression(SetExpressionNode { sel: SelectorNode { ref_, .. }, gen_bits, .. }) => {
                match ref_.as_ref().and_then(|r| r.slot()) {
                    Some(slot) => match slot.get_def_bits() {
                        Some(def_bits) => BitSet::xor(Some(def_bits), gen_bits.clone()),
                        None => gen_bits.clone(),
                    },
                    None => None,
                }
            }
            Kind::WhileStatement(n) => n.statement.as_ref().and_then(|s| s.borrow().get_kill_bits()),
            _ => None,
        }
    }

    pub fn get_ref(&mut self, cx: &ContextRef) -> Option<ReferenceValue> {
        match &mut self.kind {
            Kind::ExpressionStatement(n) => {
                if n.ref_.is_none() {
                    n.ref_ = Some(ReferenceValue::new(&cx.borrow(), None, "_cv", Some(&ObjectValue::internal_namespace())));
                }
                n.ref_.clone()
            }
            Kind::NamespaceDefinition(n) | Kind::ConfigNamespaceDefinition(ConfigNamespaceDefinitionNode { ns: n }) => {
                n.ref_.clone()
            }
            Kind::SetExpression(n) => n.sel.ref_.clone(),
            _ => None,
        }
    }

    pub fn new(position: i32, kind: Kind) -> Node {
        let mut node = Node { flags: 0, block: None, kind };
        node.set_position(position);
        node
    }

    pub fn into_ref(self) -> NodeRef {
        Rc::new(RefCell::new(self))
    }

    pub fn set_pos(&mut self, p: i32) {
        self.set_position(p);
    }

    pub fn pos(&self) -> i32 {
        match &self.kind {
            Kind::ArgumentList(ArgumentListNode { items, .. })
            | Kind::AttributeList(AttributeListNode { items, .. })
            | Kind::List(ListNode { items, .. })
            | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                if !items.is_empty() {
                    rb(&last_item(items)).pos()
                } else {
                    0
                }
            }
            Kind::EmptyStatement(_) => 0,
            Kind::PackageIdentifiers(n) => {
                if !n.list.is_empty() {
                    rb(&last_item(&n.list)).pos()
                } else {
                    0
                }
            }
            Kind::UntypedVariableBinding(n) => rb(&n.identifier).pos(),
            Kind::VariableBinding(n) => {
                let variable = rb(&n.variable);
                let identifier = &variable.as_typed_identifier().expect("ClassCastException").identifier;
                let pos = rb(identifier).pos();
                pos
            }
            _ => self.get_position(),
        }
    }

    pub fn to_canonical_string(&self, cx: &ContextRef, buf: &mut String) {
        match &self.kind {
            Kind::FunctionSignature(n) => n.to_canonical_string(cx, buf),
            Kind::MemberExpression(n) => {
                buf.push_str(&DocCommentNode::get_ref_name(cx, n.ref_.as_ref().expect("NullPointerException")));
            }
            Kind::ParameterList(n) => n.to_canonical_string(cx, buf),
            Kind::TypeExpression(n) => {
                if n.expr.is_some() {
                    rb(&n.expr).to_canonical_string(cx, buf);
                }
            }
            _ => {}
        }
    }

    pub fn is_expression_statement(&self) -> bool {
        matches!(self.kind, Kind::ExpressionStatement(_))
    }

    pub fn is_call_expression(&self) -> bool {
        matches!(self.kind, Kind::CallExpression(_))
    }

    pub fn is_apply_type_expression(&self) -> bool {
        matches!(self.kind, Kind::ApplyTypeExpr(_))
    }

    pub fn is_new_expression(&self) -> bool {
        false
    }

    pub fn is_full_super_expression(&self) -> bool {
        match &self.kind {
            Kind::SuperExpression(n) => n.expr.is_some(),
            _ => false,
        }
    }

    pub fn is_member_expression(&self) -> bool {
        matches!(self.kind, Kind::MemberExpression(_))
    }

    pub fn is_get_expression(&self) -> bool {
        matches!(self.kind, Kind::GetExpression(_))
    }

    pub fn is_set_expression(&self) -> bool {
        matches!(self.kind, Kind::SetExpression(_))
    }

    pub fn is_function_expression(&self) -> bool {
        matches!(self.kind, Kind::FunctionCommon(_))
    }

    pub fn is_boolean_expression(&self) -> bool {
        match &self.kind {
            Kind::BinaryExpression(n) => n.is_boolean_expression(),
            Kind::LiteralBoolean(_) => true,
            _ => false,
        }
    }

    pub fn is_list(&self) -> bool {
        matches!(self.kind, Kind::List(_) | Kind::ListError(_))
    }

    pub fn is_identifier(&self) -> bool {
        self.identifier().is_some()
    }

    pub fn is_label(&self) -> bool {
        match &self.kind {
            Kind::AttributeList(AttributeListNode { items, .. })
            | Kind::List(ListNode { items, .. })
            | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                items.len() == 1 && rb(&last_item(items)).is_label()
            }
            Kind::MemberExpression(n) => {
                if n.base.is_none() {
                    let selector = rb(&n.selector);
                    if selector.is_get_expression() {
                        let expr = &selector.selector().expect("ClassCastException").expr;
                        let is_qualified = expr.as_ref().map_or(false, |e| e.borrow().qualified_identifier().is_some());
                        if !is_qualified {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub fn is_definition(&self) -> bool {
        match &self.kind {
            Kind::ForStatement(n) => n.initialize.as_ref().map_or(false, |i| i.borrow().is_definition()),
            Kind::PackageDefinition(_) => false,
            Kind::PackageIdentifiers(_) => (self.flags & PackageIdentifiersNode::IS_DEFINITION_FLAG) != 0,
            _ => self.definition().is_some(),
        }
    }

    pub fn is_const(&self) -> bool {
        matches!(
            self.kind,
            Kind::ClassDefinition(_)
                | Kind::InterfaceDefinition(_)
                | Kind::BinaryClassDef(_)
                | Kind::BinaryInterfaceDefinition(_)
                | Kind::FunctionDefinition(_)
                | Kind::BinaryFunctionDefinition(_)
                | Kind::ImportDirective(_)
                | Kind::NamespaceDefinition(_)
                | Kind::ConfigNamespaceDefinition(_)
                | Kind::PackageDefinition(_)
        )
    }

    pub fn is_labeled_statement(&self) -> bool {
        matches!(self.kind, Kind::LabeledStatement(_))
    }

    pub fn is_literal(&self) -> bool {
        matches!(
            self.kind,
            Kind::LiteralArray(_)
                | Kind::LiteralBoolean(_)
                | Kind::LiteralField(_)
                | Kind::LiteralNull(_)
                | Kind::LiteralNumber(_)
                | Kind::LiteralObject(_)
                | Kind::LiteralRegExp(_)
                | Kind::LiteralString(_)
                | Kind::LiteralVector(_)
                | Kind::LiteralXML(_)
        )
    }

    pub fn is_literal_number(&self) -> bool {
        matches!(self.kind, Kind::LiteralNumber(_))
    }

    pub fn is_literal_integer(&self) -> bool {
        match &self.kind {
            Kind::ArgumentList(n) => {
                if n.items.len() == 1 && rb(&n.items.first().cloned().flatten()).is_literal_integer() {
                    true
                } else {
                    false
                }
            }
            Kind::Box(n) => rb(&n.expr).is_literal_integer(),
            Kind::LiteralNumber(_) => false,
            _ => false,
        }
    }

    pub fn is_statement_list(&self) -> bool {
        matches!(self.kind, Kind::StatementList(_))
    }

    pub fn is_attribute(&self) -> bool {
        match &self.kind {
            Kind::AttributeList(n) => {
                for n in &n.items {
                    if rb(n).is_attribute() {
                        return false;
                    }
                }
                true
            }
            Kind::ExpressionStatement(n) => rb(&n.expr).is_attribute(),
            Kind::GetExpression(_) => true,
            Kind::List(ListNode { items, .. }) | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                for n in items {
                    if !rb(n).is_attribute() {
                        return false;
                    }
                }
                true
            }
            Kind::MemberExpression(n) => rb(&n.selector).is_attribute(),
            Kind::QualifiedExpression(_) => true,
            _ => self.identifier().is_some(),
        }
    }

    pub fn is_delete_expression(&self) -> bool {
        matches!(self.kind, Kind::DeleteExpression(_))
    }

    pub fn has_attribute(&self, name: &str) -> bool {
        match &self.kind {
            Kind::AttributeList(n) => {
                for n in &n.items {
                    if rb(n).has_attribute(name) {
                        return true;
                    }
                }
                false
            }
            Kind::GetExpression(n) => {
                let expr = rb(&n.sel.expr);
                expr.identifier().expect("ClassCastException");
                if expr.has_attribute(name) {
                    return true;
                }
                false
            }
            Kind::List(ListNode { items, .. }) | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                for n in items {
                    if rb(n).has_attribute(name) {
                        return true;
                    }
                }
                false
            }
            Kind::MemberExpression(n) => {
                if n.base.is_none() && rb(&n.selector).has_attribute(name) {
                    return true;
                }
                false
            }
            Kind::TypeExpression(n) => rb(&n.expr).has_attribute(name),
            Kind::VariableDefinition(n) => {
                if n.def.attrs.is_some() && rb(&n.def.attrs).has_attribute(name) {
                    return true;
                }
                false
            }
            _ => match self.identifier() {
                Some(id) => id.name == name,
                None => false,
            },
        }
    }

    pub fn count_vars(&self) -> i32 {
        match &self.kind {
            Kind::CatchClause(n) => 1 + if n.statements.is_some() { rb(&n.statements).count_vars() } else { 0 },
            Kind::DoStatement(n) => rb(&n.statements).count_vars(),
            Kind::FinallyClause(n) => rb(&n.statements).count_vars(),
            Kind::ForStatement(n) => {
                (if n.initialize.is_some() { rb(&n.initialize).count_vars() } else { 0 })
                    + (if n.statement.is_some() { rb(&n.statement).count_vars() } else { 0 })
            }
            Kind::FunctionDefinition(n) | Kind::BinaryFunctionDefinition(BinaryFunctionDefinitionNode { fdef: n }) => {
                if rb(&n.name).as_function_name().expect("ClassCastException").kind == EMPTY_TOKEN {
                    1
                } else {
                    0
                }
            }
            Kind::IfStatement(n) => {
                (if n.thenactions.is_some() { rb(&n.thenactions).count_vars() } else { 0 })
                    + (if n.elseactions.is_some() { rb(&n.elseactions).count_vars() } else { 0 })
            }
            Kind::LabeledStatement(n) => rb(&n.statement).count_vars(),
            Kind::StatementList(n) => {
                let mut count = 0;
                for n in n.items.iter().flatten() {
                    count += n.borrow().count_vars();
                }
                count
            }
            Kind::SwitchStatement(n) => rb(&n.statements).count_vars(),
            Kind::TryStatement(n) => {
                (if n.tryblock.is_some() { rb(&n.tryblock).count_vars() } else { 0 })
                    + (if n.catchlist.is_some() { rb(&n.catchlist).count_vars() } else { 0 })
                    + (if n.finallyblock.is_some() { rb(&n.finallyblock).count_vars() } else { 0 })
            }
            Kind::VariableDefinition(n) => rb(&n.list).list().expect("ClassCastException").size(),
            Kind::WhileStatement(n) => if n.statement.is_some() { rb(&n.statement).count_vars() } else { 0 },
            Kind::WithStatement(n) => if n.statement.is_some() { rb(&n.statement).count_vars() } else { 0 },
            _ => 0,
        }
    }

    pub fn set_position_nonterminal(&mut self, expr: Option<&NodeRef>, pos: i32) {
        let expr_pos = expr.map(|e| e.borrow().pos());
        self.set_position_nonterminal_from(expr_pos, pos);
    }

    /// `setPositionNonterminal` with `expr.pos()` already read, for when `expr` is this node.
    pub fn set_position_nonterminal_from(&mut self, expr_pos: Option<i32>, pos: i32) {
        if let Kind::EmptyStatement(_) = self.kind {
            return;
        }
        if pos >= 0 {
            self.set_pos(pos);
        } else if let Some(expr_pos) = expr_pos {
            self.set_pos(expr_pos);
        } else {
            self.set_pos(0);
        }
        if self.get_position() <= 0 {
            self.set_pos(0);
        }
    }

    pub fn set_position_terminal(&mut self, pos: i32) {
        if let Kind::EmptyStatement(_) = self.kind {
            return;
        }
        if pos >= 0 {
            self.set_pos(pos);
        } else {
            self.set_pos(0);
        }
    }

    pub fn set_is_synthetic(&mut self) {
        self.set_pos(0);
    }

    pub fn in_terminal_block(&self) -> bool {
        self.block.as_ref().map_or(false, |b| b.borrow().succs_size() == 0)
    }

    pub fn is_synthetic(&self) -> bool {
        self.get_position() == 0
    }

    pub fn expected_type(&mut self, type_: Option<TypeValue>) {
        match &mut self.kind {
            Kind::ExpressionStatement(n) => {
                n.expected_type = type_.clone();
                rbm(&n.expr).expected_type(type_);
            }
            Kind::ForStatement(n) => rbm(&n.statement).expected_type(type_),
            _ => {}
        }
    }

    pub fn void_result(&mut self) {
        match &mut self.kind {
            Kind::BinaryExpression(n) => n.void_result = true,
            Kind::Box(n) => {
                n.void_result = true;
                rbm(&n.expr).void_result();
            }
            Kind::CallExpression(n) => n.void_result = true,
            Kind::Coerce(n) => {
                n.void_result = true;
                rbm(&n.expr).void_result();
            }
            Kind::ConditionalExpression(n) => {
                rbm(&n.thenexpr).void_result();
                rbm(&n.elseexpr).void_result();
            }
            Kind::DeleteExpression(n) => n.void_result = true,
            Kind::ExpressionStatement(n) => rbm(&n.expr).void_result(),
            Kind::FunctionCommon(n) => n.void_result = true,
            Kind::GetExpression(n) => {
                n.sel.void_result();
                rbm(&n.sel.expr).void_result();
            }
            Kind::Increment(n) => {
                n.void_result = true;
                rbm(&n.sel.expr).void_result();
            }
            Kind::List(ListNode { items, .. }) | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                rbm(&last_item(items)).void_result();
            }
            Kind::LiteralArray(LiteralArrayNode { void_result, .. })
            | Kind::LiteralBoolean(LiteralBooleanNode { void_result, .. })
            | Kind::LiteralNull(LiteralNullNode { void_result, .. })
            | Kind::LiteralNumber(LiteralNumberNode { void_result, .. })
            | Kind::LiteralObject(LiteralObjectNode { void_result, .. })
            | Kind::LiteralRegExp(LiteralRegExpNode { void_result, .. })
            | Kind::LiteralString(LiteralStringNode { void_result, .. })
            | Kind::LiteralVector(LiteralVectorNode { void_result, .. })
            | Kind::LiteralXML(LiteralXMLNode { void_result, .. }) => *void_result = true,
            Kind::LoadRegister(_) => {}
            Kind::MemberExpression(n) => rbm(&n.selector).void_result(),
            Kind::ParenExpression(n) => rbm(&n.expr).void_result(),
            Kind::ParenListExpression(n) => rbm(&n.expr).void_result(),
            Kind::Register(n) => n.void_result = true,
            Kind::SetExpression(n) => {
                n.void_result = true;
                rbm(&n.sel.expr).void_result();
            }
            Kind::StatementList(n) => {
                for n in &n.items {
                    rbm(n).void_result();
                }
                let last = last_item(&n.items);
                if let Some(last) = last {
                    if let Kind::LoadRegister(l) = &mut last.borrow_mut().kind {
                        l.void_result = true;
                    }
                }
            }
            Kind::StoreRegister(n) => n.void_result = true,
            Kind::TypeExpression(n) => rbm(&n.expr).void_result(),
            Kind::UnaryExpression(n) => n.void_result = true,
            Kind::ApplyTypeExpr(ApplyTypeExprNode { sel, .. }) | Kind::Invoke(InvokeNode { sel, .. }) => sel.void_result(),
            _ => {}
        }
    }

    pub fn set_position(&mut self, mut position: i32) {
        if position < 0 {
            position = 0;
        }
        self.flags &= !POSITION_MASK;
        self.flags |= position << POSITION_SHIFT;
    }

    pub fn get_position(&self) -> i32 {
        (self.flags & POSITION_MASK) >> POSITION_SHIFT
    }

    pub fn has_side_effect(&self) -> bool {
        match &self.kind {
            Kind::ArgumentList(n) => {
                for n in &n.items {
                    if rb(n).has_side_effect() {
                        return true;
                    }
                }
                false
            }
            Kind::Increment(_) => true,
            Kind::MemberExpression(n) => rb(&n.selector).has_side_effect(),
            Kind::SetExpression(_) => true,
            Kind::TypeExpression(n) => rb(&n.expr).has_side_effect(),
            _ => false,
        }
    }

    pub fn is_lvalue(&self) -> bool {
        match &self.kind {
            Kind::GetExpression(_) => true,
            Kind::List(ListNode { items, .. }) | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                items.len() == 1 && rb(&items[0]).is_lvalue()
            }
            Kind::MemberExpression(n) => rb(&n.selector).is_lvalue(),
            Kind::StoreRegister(_) => true,
            Kind::TypeExpression(n) => rb(&n.expr).is_lvalue(),
            _ => self.identifier().is_some(),
        }
    }

    pub fn is_configuration_name(&self) -> bool {
        match &self.kind {
            Kind::ExpressionStatement(n) => rb(&n.expr).is_configuration_name(),
            Kind::GetExpression(n) => rb(&n.sel.expr).is_configuration_name(),
            Kind::List(ListNode { items, .. }) | Kind::ListError(ListErrorNode { list: ListNode { items, .. }, .. }) => {
                items.len() == 1 && rb(&items[0]).is_configuration_name()
            }
            Kind::MemberExpression(n) => rb(&n.selector).is_configuration_name(),
            _ => match self.qualified_identifier() {
                Some(q) => q.is_config_name,
                None => false,
            },
        }
    }

    // Overrides of non-`Node` virtual methods, dispatched over the subclasses that declare them.

    /// `SelectorNode.isQualified()`.
    pub fn is_qualified(&self) -> bool {
        match &self.kind {
            Kind::CallExpression(CallExpressionNode { sel, .. })
            | Kind::DeleteExpression(DeleteExpressionNode { sel, .. })
            | Kind::GetExpression(GetExpressionNode { sel, .. })
            | Kind::Increment(IncrementNode { sel, .. })
            | Kind::SetExpression(SetExpressionNode { sel, .. }) => match &sel.expr {
                Some(e) => e.borrow().qualified_identifier().map_or(false, |q| q.qualifier.is_some()),
                None => false,
            },
            _ => false,
        }
    }

    /// `SelectorNode.isAttributeIdentifier()`.
    pub fn is_attribute_identifier(&self) -> bool {
        match &self.kind {
            Kind::CallExpression(CallExpressionNode { sel, .. })
            | Kind::DeleteExpression(DeleteExpressionNode { sel, .. })
            | Kind::Increment(IncrementNode { sel, .. })
            | Kind::SetExpression(SetExpressionNode { sel, .. }) => match &sel.expr {
                Some(e) if e.borrow().identifier().is_some() => e.borrow().is_attr(),
                _ => false,
            },
            Kind::GetExpression(GetExpressionNode { sel, .. }) => match &sel.expr {
                Some(e) if e.borrow().identifier().is_some() => e.borrow().is_attr(),
                _ => sel.is_attr(),
            },
            _ => false,
        }
    }

    /// `IdentifierNode.isAny()`, `SelectorNode.isAny()` and `MemberExpressionNode.isAny()`.
    pub fn is_any(&self) -> bool {
        match &self.kind {
            Kind::CallExpression(CallExpressionNode { sel, .. })
            | Kind::DeleteExpression(DeleteExpressionNode { sel, .. })
            | Kind::GetExpression(GetExpressionNode { sel, .. })
            | Kind::Increment(IncrementNode { sel, .. })
            | Kind::SetExpression(SetExpressionNode { sel, .. }) => match &sel.expr {
                Some(e) if e.borrow().identifier().is_some() => e.borrow().is_any(),
                _ => false,
            },
            Kind::MemberExpression(n) => rb(&n.selector).is_any(),
            _ if self.identifier().is_some() => (self.flags & IS_ANY_FLAG) != 0,
            _ => false,
        }
    }

    /// `IdentifierNode.setAny()`.
    pub fn set_any(&mut self, is_any: bool) {
        self.flags = if is_any { self.flags | IS_ANY_FLAG } else { self.flags & !IS_ANY_FLAG };
    }

    /// `IdentifierNode.setAttr()` and `SelectorNode.setAttr()`.
    pub fn set_attr(&mut self, is_attr: bool) {
        if let Some(sel) = self.selector_mut() {
            sel.set_attr(is_attr);
            return;
        }
        self.flags = if is_attr { self.flags | IS_ATTR_FLAG } else { self.flags & !IS_ATTR_FLAG };
    }

    /// `IdentifierNode.isAttr()` and `SelectorNode.isAttr()`.
    pub fn is_attr(&self) -> bool {
        if let Some(sel) = self.selector() {
            return sel.is_attr();
        }
        (self.flags & IS_ATTR_FLAG) != 0
    }

    /// `IdentifierNode.toIdentifierString()` and `PackageIdentifiersNode.toIdentifierString()`.
    pub fn to_identifier_string(&mut self) -> Option<String> {
        let is_definition = self.is_definition();
        match &mut self.kind {
            Kind::PackageIdentifiers(n) => n.to_identifier_string(is_definition),
            _ => Some(self.identifier().expect("ClassCastException").name.clone()),
        }
    }

    /// `VariableBindingNode.inPackage()`.
    pub fn in_package(&self) -> bool {
        (self.flags & VariableBindingNode::PACKAGE_FLAG) != 0
    }

    /// `ClassDefinitionNode.isInterface()`.
    pub fn is_interface(&self) -> bool {
        matches!(self.kind, Kind::InterfaceDefinition(_) | Kind::BinaryInterfaceDefinition(_))
    }

    /// `isBranch()`, declared by the statement classes.
    pub fn is_branch(&self) -> bool {
        matches!(
            self.kind,
            Kind::BreakStatement(_)
                | Kind::ContinueStatement(_)
                | Kind::DoStatement(_)
                | Kind::ForStatement(_)
                | Kind::IfStatement(_)
                | Kind::LabeledStatement(_)
                | Kind::ReturnStatement(_)
                | Kind::SwitchStatement(_)
                | Kind::ThrowStatement(_)
                | Kind::TryStatement(_)
                | Kind::WhileStatement(_)
        )
    }

    /// `skipNode()` of `DefinitionNode`, `ExpressionStatementNode`, `SelectorNode` and `TryStatementNode`.
    pub fn skip_node(&mut self, b: bool) {
        if let Some(def) = self.definition_mut() {
            def.skip = b;
            return;
        }
        if let Some(sel) = self.selector_mut() {
            sel.skip = b;
            return;
        }
        match &mut self.kind {
            Kind::ExpressionStatement(n) => n.skip = b,
            Kind::TryStatement(n) => n.skip = b,
            _ => panic!("skipNode on {self}"),
        }
    }

    pub fn skip(&self) -> bool {
        if let Some(def) = self.definition() {
            return def.skip;
        }
        if let Some(sel) = self.selector() {
            return sel.skip;
        }
        match &self.kind {
            Kind::ExpressionStatement(n) => n.skip,
            Kind::TryStatement(n) => n.skip,
            _ => panic!("skip on {self}"),
        }
    }

    /// `ParameterNode.size()` and `RestParameterNode.size()`.
    pub fn parameter_size(&self) -> i32 {
        match &self.kind {
            Kind::Parameter(_) => 1,
            Kind::RestParameter(_) => 0,
            _ => panic!("ClassCastException"),
        }
    }

    /// `MetaDataNode.count()` and `DocCommentNode.count()`.
    pub fn meta_data_count(&self) -> i32 {
        match &self.kind {
            Kind::DocComment(n) => n.meta.get_values().expect("NullPointerException").len() as i32,
            Kind::MetaData(n) => n.count(),
            _ => panic!("ClassCastException"),
        }
    }
}

macro_rules! kind_accessors {
    ($($variant:ident($ty:ident) => $get:ident, $get_mut:ident;)*) => {
        impl Node {
            $(
                pub fn $get(&self) -> Option<&$ty> {
                    if let Kind::$variant(n) = &self.kind { Some(n) } else { None }
                }
                pub fn $get_mut(&mut self) -> Option<&mut $ty> {
                    if let Kind::$variant(n) = &mut self.kind { Some(n) } else { None }
                }
            )*
        }
    };
}

kind_accessors! {
    ApplyTypeExpr(ApplyTypeExprNode) => as_apply_type_expr, as_apply_type_expr_mut;
    ArgumentList(ArgumentListNode) => as_argument_list, as_argument_list_mut;
    AttributeList(AttributeListNode) => as_attribute_list, as_attribute_list_mut;
    BinaryClassDef(BinaryClassDefNode) => as_binary_class_def, as_binary_class_def_mut;
    BinaryExpression(BinaryExpressionNode) => as_binary_expression, as_binary_expression_mut;
    BinaryFunctionDefinition(BinaryFunctionDefinitionNode) => as_binary_function_definition, as_binary_function_definition_mut;
    BinaryInterfaceDefinition(BinaryInterfaceDefinitionNode) => as_binary_interface_definition, as_binary_interface_definition_mut;
    BinaryProgram(BinaryProgramNode) => as_binary_program, as_binary_program_mut;
    Block(BlockNode) => as_block, as_block_mut;
    Box(BoxNode) => as_box, as_box_mut;
    BreakStatement(BreakStatementNode) => as_break_statement, as_break_statement_mut;
    CallExpression(CallExpressionNode) => as_call_expression, as_call_expression_mut;
    CaseLabel(CaseLabelNode) => as_case_label, as_case_label_mut;
    CatchClause(CatchClauseNode) => as_catch_clause, as_catch_clause_mut;
    ClassDefinition(ClassDefinitionNode) => as_class_definition, as_class_definition_mut;
    ClassName(ClassNameNode) => as_class_name, as_class_name_mut;
    Coerce(CoerceNode) => as_coerce, as_coerce_mut;
    Comment(CommentNode) => as_comment, as_comment_mut;
    ConditionalExpression(ConditionalExpressionNode) => as_conditional_expression, as_conditional_expression_mut;
    ConfigNamespaceDefinition(ConfigNamespaceDefinitionNode) => as_config_namespace_definition, as_config_namespace_definition_mut;
    ContinueStatement(ContinueStatementNode) => as_continue_statement, as_continue_statement_mut;
    DefaultXMLNamespace(DefaultXMLNamespaceNode) => as_default_xml_namespace, as_default_xml_namespace_mut;
    DeleteExpression(DeleteExpressionNode) => as_delete_expression, as_delete_expression_mut;
    DoStatement(DoStatementNode) => as_do_statement, as_do_statement_mut;
    DocComment(DocCommentNode) => as_doc_comment, as_doc_comment_mut;
    EmptyElement(EmptyElementNode) => as_empty_element, as_empty_element_mut;
    EmptyStatement(EmptyStatementNode) => as_empty_statement, as_empty_statement_mut;
    Error(ErrorNode) => as_error, as_error_mut;
    ExpressionStatement(ExpressionStatementNode) => as_expression_statement, as_expression_statement_mut;
    FinallyClause(FinallyClauseNode) => as_finally_clause, as_finally_clause_mut;
    ForStatement(ForStatementNode) => as_for_statement, as_for_statement_mut;
    FunctionCommon(FunctionCommonNode) => as_function_common, as_function_common_mut;
    FunctionDefinition(FunctionDefinitionNode) => as_function_definition, as_function_definition_mut;
    FunctionName(FunctionNameNode) => as_function_name, as_function_name_mut;
    FunctionSignature(FunctionSignatureNode) => as_function_signature, as_function_signature_mut;
    GetExpression(GetExpressionNode) => as_get_expression, as_get_expression_mut;
    HasNext(HasNextNode) => as_has_next, as_has_next_mut;
    IdentifierError(IdentifierErrorNode) => as_identifier_error, as_identifier_error_mut;
    Identifier(IdentifierNode) => as_identifier, as_identifier_mut;
    IfStatement(IfStatementNode) => as_if_statement, as_if_statement_mut;
    ImportDirective(ImportDirectiveNode) => as_import_directive, as_import_directive_mut;
    Import(ImportNode) => as_import, as_import_mut;
    IncludeDirective(IncludeDirectiveNode) => as_include_directive, as_include_directive_mut;
    Increment(IncrementNode) => as_increment, as_increment_mut;
    Inheritance(InheritanceNode) => as_inheritance, as_inheritance_mut;
    InterfaceDefinition(InterfaceDefinitionNode) => as_interface_definition, as_interface_definition_mut;
    Invoke(InvokeNode) => as_invoke, as_invoke_mut;
    LabeledStatement(LabeledStatementNode) => as_labeled_statement, as_labeled_statement_mut;
    ListError(ListErrorNode) => as_list_error, as_list_error_mut;
    List(ListNode) => as_list, as_list_mut;
    LiteralArray(LiteralArrayNode) => as_literal_array, as_literal_array_mut;
    LiteralBoolean(LiteralBooleanNode) => as_literal_boolean, as_literal_boolean_mut;
    LiteralField(LiteralFieldNode) => as_literal_field, as_literal_field_mut;
    LiteralNull(LiteralNullNode) => as_literal_null, as_literal_null_mut;
    LiteralNumber(LiteralNumberNode) => as_literal_number, as_literal_number_mut;
    LiteralObject(LiteralObjectNode) => as_literal_object, as_literal_object_mut;
    LiteralRegExp(LiteralRegExpNode) => as_literal_reg_exp, as_literal_reg_exp_mut;
    LiteralString(LiteralStringNode) => as_literal_string, as_literal_string_mut;
    LiteralVector(LiteralVectorNode) => as_literal_vector, as_literal_vector_mut;
    LiteralXML(LiteralXMLNode) => as_literal_xml, as_literal_xml_mut;
    LoadRegister(LoadRegisterNode) => as_load_register, as_load_register_mut;
    MemberExpression(MemberExpressionNode) => as_member_expression, as_member_expression_mut;
    MetaData(MetaDataNode) => as_meta_data, as_meta_data_mut;
    NamespaceDefinition(NamespaceDefinitionNode) => as_namespace_definition, as_namespace_definition_mut;
    PackageDefinition(PackageDefinitionNode) => as_package_definition, as_package_definition_mut;
    PackageIdentifiers(PackageIdentifiersNode) => as_package_identifiers, as_package_identifiers_mut;
    PackageName(PackageNameNode) => as_package_name, as_package_name_mut;
    ParameterList(ParameterListNode) => as_parameter_list, as_parameter_list_mut;
    Parameter(ParameterNode) => as_parameter, as_parameter_mut;
    ParenExpression(ParenExpressionNode) => as_paren_expression, as_paren_expression_mut;
    ParenListExpression(ParenListExpressionNode) => as_paren_list_expression, as_paren_list_expression_mut;
    PragmaExpression(PragmaExpressionNode) => as_pragma_expression, as_pragma_expression_mut;
    Pragma(PragmaNode) => as_pragma, as_pragma_mut;
    Program(ProgramNode) => as_program, as_program_mut;
    QualifiedExpression(QualifiedExpressionNode) => as_qualified_expression, as_qualified_expression_mut;
    QualifiedIdentifier(QualifiedIdentifierNode) => as_qualified_identifier, as_qualified_identifier_mut;
    Register(RegisterNode) => as_register, as_register_mut;
    RestExpression(RestExpressionNode) => as_rest_expression, as_rest_expression_mut;
    RestParameter(RestParameterNode) => as_rest_parameter, as_rest_parameter_mut;
    ReturnStatement(ReturnStatementNode) => as_return_statement, as_return_statement_mut;
    SetExpression(SetExpressionNode) => as_set_expression, as_set_expression_mut;
    StatementList(StatementListNode) => as_statement_list, as_statement_list_mut;
    StoreRegister(StoreRegisterNode) => as_store_register, as_store_register_mut;
    SuperExpression(SuperExpressionNode) => as_super_expression, as_super_expression_mut;
    SuperStatement(SuperStatementNode) => as_super_statement, as_super_statement_mut;
    SwitchStatement(SwitchStatementNode) => as_switch_statement, as_switch_statement_mut;
    ThisExpression(ThisExpressionNode) => as_this_expression, as_this_expression_mut;
    ThrowStatement(ThrowStatementNode) => as_throw_statement, as_throw_statement_mut;
    ToObject(ToObjectNode) => as_to_object, as_to_object_mut;
    TryStatement(TryStatementNode) => as_try_statement, as_try_statement_mut;
    TypeExpression(TypeExpressionNode) => as_type_expression, as_type_expression_mut;
    TypeIdentifier(TypeIdentifierNode) => as_type_identifier, as_type_identifier_mut;
    TypedIdentifier(TypedIdentifierNode) => as_typed_identifier, as_typed_identifier_mut;
    UnaryExpression(UnaryExpressionNode) => as_unary_expression, as_unary_expression_mut;
    UntypedVariableBinding(UntypedVariableBindingNode) => as_untyped_variable_binding, as_untyped_variable_binding_mut;
    UseDirective(UseDirectiveNode) => as_use_directive, as_use_directive_mut;
    UseNumeric(UseNumericNode) => as_use_numeric, as_use_numeric_mut;
    UsePragma(UsePragmaNode) => as_use_pragma, as_use_pragma_mut;
    UsePrecision(UsePrecisionNode) => as_use_precision, as_use_precision_mut;
    UseRounding(UseRoundingNode) => as_use_rounding, as_use_rounding_mut;
    VariableBinding(VariableBindingNode) => as_variable_binding, as_variable_binding_mut;
    VariableDefinition(VariableDefinitionNode) => as_variable_definition, as_variable_definition_mut;
    WhileStatement(WhileStatementNode) => as_while_statement, as_while_statement_mut;
    WithStatement(WithStatementNode) => as_with_statement, as_with_statement_mut;
}

macro_rules! base_accessor {
    ($get:ident, $get_mut:ident, $ty:ident, $($variant:ident $(. $field:ident)*),+) => {
        impl Node {
            pub fn $get(&self) -> Option<&$ty> {
                match &self.kind {
                    $(Kind::$variant(n) => Some(&(*n) $(. $field)*),)+
                    _ => None,
                }
            }
            pub fn $get_mut(&mut self) -> Option<&mut $ty> {
                match &mut self.kind {
                    $(Kind::$variant(n) => Some(&mut (*n) $(. $field)*),)+
                    _ => None,
                }
            }
        }
    };
}

base_accessor!(selector, selector_mut, SelectorNode,
    ApplyTypeExpr.sel, CallExpression.sel, DeleteExpression.sel, GetExpression.sel, Increment.sel, Invoke.sel,
    SetExpression.sel);
base_accessor!(definition, definition_mut, DefinitionNode,
    ClassDefinition.def, InterfaceDefinition.cls.def, BinaryClassDef.cls.def, BinaryInterfaceDefinition.bcls.cls.def,
    FunctionDefinition.def, BinaryFunctionDefinition.fdef.def, ImportDirective.def, IncludeDirective.def,
    NamespaceDefinition.def, ConfigNamespaceDefinition.ns.def, PackageDefinition.def, UseDirective.def,
    VariableDefinition.def);
base_accessor!(identifier, identifier_mut, IdentifierNode,
    Identifier, IdentifierError.id, QualifiedIdentifier.id, QualifiedExpression.qid.id, TypeIdentifier.id);
base_accessor!(qualified_identifier, qualified_identifier_mut, QualifiedIdentifierNode,
    QualifiedIdentifier, QualifiedExpression.qid);
base_accessor!(class_definition, class_definition_mut, ClassDefinitionNode,
    ClassDefinition, InterfaceDefinition.cls, BinaryClassDef.cls, BinaryInterfaceDefinition.bcls.cls);
base_accessor!(binary_class_def, binary_class_def_mut, BinaryClassDefNode,
    BinaryClassDef, BinaryInterfaceDefinition.bcls);
base_accessor!(use_pragma, use_pragma_mut, UsePragmaNode,
    UsePragma, UsePrecision.pragma, UseNumeric.pragma, UseRounding.pragma);
base_accessor!(list, list_mut, ListNode, List, ListError.list);
base_accessor!(program, program_mut, ProgramNode, Program, BinaryProgram.prog);
base_accessor!(parameter, parameter_mut, ParameterNode, Parameter, RestParameter.param);
base_accessor!(function_definition, function_definition_mut, FunctionDefinitionNode,
    FunctionDefinition, BinaryFunctionDefinition.fdef);
base_accessor!(namespace_definition, namespace_definition_mut, NamespaceDefinitionNode,
    NamespaceDefinition, ConfigNamespaceDefinition.ns);
base_accessor!(meta_data, meta_data_mut, MetaDataNode, MetaData, DocComment.meta);

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match &self.kind {
            Kind::ApplyTypeExpr(_) => "ApplyTypeExpression",
            Kind::ArgumentList(_) => "ArgumentList",
            Kind::AttributeList(_) => "AttributeList",
            Kind::BinaryClassDef(_) => "BinaryClassDef",
            Kind::BinaryExpression(_) => "BinaryExpression",
            Kind::BinaryFunctionDefinition(_) => "BinaryFunctionDefinition",
            Kind::BinaryInterfaceDefinition(_) => "BinaryInterfaceDefinition",
            Kind::BinaryProgram(_) => "BinaryProgram",
            Kind::Block(_) => "Block",
            Kind::Box(_) => "Node",
            Kind::BreakStatement(_) => "BreakStatement",
            Kind::CallExpression(_) => "CallExpression",
            Kind::CaseLabel(_) => "CaseLabel",
            Kind::CatchClause(_) => "CatchClause",
            Kind::ClassDefinition(_) => "ClassDefinition",
            Kind::ClassName(_) => "ClassName",
            Kind::Coerce(_) => "Node",
            Kind::Comment(n) => return write!(f, "{}", n.comment),
            Kind::ConditionalExpression(_) => "ConditionalExpression",
            Kind::ConfigNamespaceDefinition(_) => "NamespaceDefinition",
            Kind::ContinueStatement(_) => "ContinueStatement",
            Kind::DefaultXMLNamespace(_) => "DefaultXMLNamespace",
            Kind::DeleteExpression(_) => "DeleteExpression",
            Kind::DoStatement(_) => "DoStatement",
            Kind::DocComment(_) => "DocComment",
            Kind::EmptyElement(_) => "EmptyElement",
            Kind::EmptyStatement(_) => "EmptyStatement",
            Kind::Error(_) => "Error",
            Kind::ExpressionStatement(_) => "ExpressionStatement",
            Kind::FinallyClause(_) => "FinallyClause",
            Kind::ForStatement(_) => "ForStatement",
            Kind::FunctionCommon(_) => "FunctionCommon",
            Kind::FunctionDefinition(_) => "FunctionDefinition",
            Kind::FunctionName(_) => "functionname",
            Kind::FunctionSignature(_) => "FunctionSignature",
            Kind::GetExpression(_) => "GetExpression",
            Kind::HasNext(_) => "HasNext",
            Kind::IdentifierError(_) => "IdentifierError",
            Kind::Identifier(_) => "Identifier",
            Kind::IfStatement(_) => "IfStatement",
            Kind::ImportDirective(_) => "importdirective",
            Kind::Import(_) => "ImportNode",
            Kind::IncludeDirective(_) => "IncludeDirective",
            Kind::Increment(_) => "IncrementNode",
            Kind::Inheritance(_) => "inheritance",
            Kind::InterfaceDefinition(_) => "InterfaceDefinition",
            Kind::Invoke(_) => "Invoke",
            Kind::LabeledStatement(_) => "LabeledStatement",
            Kind::ListError(_) => "ListError",
            Kind::List(_) => "List",
            Kind::LiteralArray(_) => "LiteralArray",
            Kind::LiteralBoolean(_) => "LiteralBoolean",
            Kind::LiteralField(_) => "LiteralField",
            Kind::LiteralNull(_) => "LiteralNull",
            Kind::LiteralNumber(_) => "LiteralNumber",
            Kind::LiteralObject(_) => "LiteralObject",
            Kind::LiteralRegExp(_) => "LiteralRegExp",
            Kind::LiteralString(_) => "LiteralString",
            Kind::LiteralVector(_) => "LiteralVector",
            Kind::LiteralXML(_) => "LiteralXML",
            Kind::LoadRegister(_) => "LoadRegister",
            Kind::MemberExpression(_) => "MemberExpression",
            Kind::MetaData(_) => "MetaData",
            Kind::NamespaceDefinition(_) => "NamespaceDefinition",
            Kind::PackageDefinition(_) => "PackageDefinition",
            Kind::PackageIdentifiers(_) => "PackageIdentifiers",
            Kind::PackageName(_) => "PackageName",
            Kind::ParameterList(_) => "ParameterList",
            Kind::Parameter(_) => "Parameter",
            Kind::ParenExpression(_) => "ParenExpression",
            Kind::ParenListExpression(_) => "ParenListExpression",
            Kind::PragmaExpression(_) => "PragmaExpression",
            Kind::Pragma(_) => "Pragma",
            Kind::Program(_) => "Program",
            Kind::QualifiedExpression(_) => "QualifiedExpression",
            Kind::QualifiedIdentifier(_) => "QualifiedIdentifier",
            Kind::Register(_) => "Register",
            Kind::RestExpression(_) => "RestExpression",
            Kind::RestParameter(_) => "RestParameter",
            Kind::ReturnStatement(_) => "ReturnStatement",
            Kind::SetExpression(_) => "SetExpression",
            Kind::StatementList(n) => return write!(f, "{}", rb(&last_item(&n.items))),
            Kind::StoreRegister(_) => "StoreRegister",
            Kind::SuperExpression(_) => "SuperExpression",
            Kind::SuperStatement(_) => "SuperStatement",
            Kind::SwitchStatement(_) => "SwitchStatement",
            Kind::ThisExpression(_) => "ThisExpression",
            Kind::ThrowStatement(_) => "ThrowStatement",
            Kind::ToObject(_) => "ToObject",
            Kind::TryStatement(_) => "TryStatement",
            Kind::TypeExpression(_) => "TypeEpression",
            Kind::TypeIdentifier(_) => "TypeIdentifier",
            Kind::TypedIdentifier(_) => "TypedIdentifier",
            Kind::UnaryExpression(_) => "UnaryExpression",
            Kind::UntypedVariableBinding(_) => "VariableBinding",
            Kind::UseDirective(_) => "UseDirective",
            Kind::UseNumeric(n) => {
                return write!(f, "UsePrecision {}", UseNumericNode::numeric_mode_names[n.numeric_mode as usize])
            }
            Kind::UsePragma(_) => "UsePragma",
            Kind::UsePrecision(n) => return write!(f, "UsePrecision {}", n.precision),
            Kind::UseRounding(n) => return write!(f, "UseRounding {}", NumberUsage::roundingModeName[n.mode as usize]),
            Kind::VariableBinding(_) => "VariableBinding",
            Kind::VariableDefinition(_) => "VarDefinition",
            Kind::WhileStatement(_) => "WhileStatement",
            Kind::WithStatement(_) => "WithStatement",
        };
        f.write_str(s)
    }
}

/// `Node.first()`.
pub fn first(this: &NodeRef) -> Option<NodeRef> {
    let statements = {
        let node = this.borrow();
        if let Some(c) = node.class_definition() {
            c.statements.clone()
        } else if let Some(f) = node.function_definition() {
            rb(&f.fexpr).as_function_common().expect("ClassCastException").body.clone()
        } else if let Some(s) = node.as_statement_list() {
            return s.items.first().cloned().flatten();
        } else {
            return Some(this.clone());
        }
    };
    first(statements.as_ref().expect("NullPointerException"))
}

/// `Node.last()`.
pub fn last(this: &NodeRef) -> Option<NodeRef> {
    let statements = {
        let node = this.borrow();
        if let Some(c) = node.class_definition() {
            c.statements.clone()
        } else if let Some(f) = node.function_definition() {
            rb(&f.fexpr).as_function_common().expect("ClassCastException").body.clone()
        } else if let Some(s) = node.as_statement_list() {
            return last_item(&s.items);
        } else {
            return Some(this.clone());
        }
    };
    last(statements.as_ref().expect("NullPointerException"))
}

/// `Node.initializerStatement(cx)`.
pub fn initializer_statement(this: &NodeRef, cx: &ContextRef) -> NodeRef {
    enum Which {
        This,
        Empty,
        FunctionDefinition,
        NamespaceDefinition,
        VariableDefinition,
    }
    let which = match &this.borrow().kind {
        Kind::ClassDefinition(_) | Kind::InterfaceDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) => {
            Which::This
        }
        Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => Which::FunctionDefinition,
        Kind::NamespaceDefinition(_) | Kind::ConfigNamespaceDefinition(_) => Which::NamespaceDefinition,
        Kind::VariableDefinition(_) => Which::VariableDefinition,
        _ => Which::Empty,
    };
    match which {
        Which::This => this.clone(),
        Which::Empty => Context::get_node_factory(cx).borrow_mut().empty_statement(),
        Which::NamespaceDefinition => {
            this.borrow_mut().namespace_definition_mut().unwrap().needs_init = true;
            this.clone()
        }
        Which::FunctionDefinition => {
            let obj = cx.borrow().scope().expect("NullPointerException");
            let bui = obj.builder().expect("NullPointerException");
            let (has_pkgdef, name_kind) = {
                let node = this.borrow();
                let fd = node.function_definition().unwrap();
                let name_kind = rb(&fd.name).as_function_name().expect("ClassCastException").kind;
                (fd.def.pkgdef.is_some(), name_kind)
            };
            if !bui.is_activation_builder()
                && (bui.is_class_builder()
                    || bui.is_instance_builder()
                    || has_pkgdef
                    || name_kind == GET_TOKEN
                    || name_kind == SET_TOKEN)
            {
                Context::get_node_factory(cx).borrow_mut().empty_statement()
            } else {
                this.borrow_mut().function_definition_mut().unwrap().needs_init = true;
                this.clone()
            }
        }
        Which::VariableDefinition => VariableDefinitionNode::initializer_statement(this, cx),
    }
}

pub struct ApplyTypeExprNode {
    pub sel: SelectorNode,
    pub type_args: Option<NodeRef>,
}

impl ApplyTypeExprNode {
    pub fn new(expr: Option<NodeRef>, type_args: Option<NodeRef>) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.expr = expr;
        sel.ref_ = None;
        Node::new(-1, Kind::ApplyTypeExpr(ApplyTypeExprNode { sel, type_args })).into_ref()
    }
}

pub struct ArgumentListNode {
    pub items: Vec<Option<NodeRef>>,
    pub expected_types: Option<Vec<TypeInfo>>,
    pub decl_styles: Option<ByteList>,
    pub is_bracket_selector: bool,
}

impl ArgumentListNode {
    pub fn new(item: Option<NodeRef>, pos: i32) -> NodeRef {
        let n = ArgumentListNode { items: vec![item], expected_types: None, decl_styles: None, is_bracket_selector: false };
        Node::new(pos, Kind::ArgumentList(n)).into_ref()
    }

    pub fn size(&self) -> i32 {
        self.items.len() as i32
    }

    pub fn add_type(&mut self, type_: TypeInfo) {
        if self.expected_types.is_none() {
            self.expected_types = Some(Vec::with_capacity(2));
        }
        self.expected_types.as_mut().unwrap().push(type_);
    }

    pub fn add_decl_style(&mut self, style: i32) {
        if self.decl_styles.is_none() {
            self.decl_styles = Some(ByteList::with_capacity(2));
        }
        self.decl_styles.as_mut().unwrap().push(style as u8);
    }
}

pub struct AttributeListNode {
    pub items: Vec<Option<NodeRef>>,
    pub has_intrinsic: bool,
    pub has_static: bool,
    pub has_final: bool,
    pub has_virtual: bool,
    pub has_override: bool,
    pub has_dynamic: bool,
    pub has_native: bool,
    pub has_private: bool,
    pub has_protected: bool,
    pub has_public: bool,
    pub has_internal: bool,
    pub has_const: bool,
    pub has_false: bool,
    pub has_prototype: bool,
    pub compile_definition: bool,
    pub namespaces: Namespaces,
    pub namespace_ids: Vec<String>,
    user_namespace: Option<ObjectValue>,
}

impl AttributeListNode {
    pub fn new(item: Option<NodeRef>, pos: i32) -> NodeRef {
        let n = AttributeListNode {
            items: vec![item],
            has_intrinsic: false,
            has_static: false,
            has_final: false,
            has_virtual: false,
            has_override: false,
            has_dynamic: false,
            has_native: false,
            has_private: false,
            has_protected: false,
            has_public: false,
            has_internal: false,
            has_const: false,
            has_false: false,
            has_prototype: false,
            compile_definition: true,
            namespaces: Namespaces::with_capacity(3),
            namespace_ids: Vec::with_capacity(3),
            user_namespace: None,
        };
        Node::new(pos, Kind::AttributeList(n)).into_ref()
    }

    pub fn size(&self) -> i32 {
        self.items.len() as i32
    }

    pub fn get_user_namespace(&self) -> Option<ObjectValue> {
        self.user_namespace.clone()
    }

    pub fn set_user_namespace(&mut self, user_namespace: Option<ObjectValue>) {
        self.user_namespace = user_namespace;
    }

    pub fn has_user_namespace(&self) -> bool {
        self.user_namespace.is_some()
    }
}

pub struct BinaryClassDefNode {
    pub cls: ClassDefinitionNode,
}

impl BinaryClassDefNode {
    pub fn new(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        baseclass: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        statements: Option<NodeRef>,
    ) -> NodeRef {
        let cls = ClassDefinitionNode::base(cx, pkgdef, attrs, name, baseclass, interfaces, statements);
        Node::new(-1, Kind::BinaryClassDef(BinaryClassDefNode { cls })).into_ref()
    }
}

pub struct BinaryExpressionNode {
    pub lhs: Option<NodeRef>,
    pub rhs: Option<NodeRef>,
    pub op: i32,
    pub slot: Option<Slot>,
    pub lhstype: Option<TypeInfo>,
    pub rhstype: Option<TypeInfo>,
    pub number_usage: Option<NumberUsage>,
    pub void_result: bool,
}

impl BinaryExpressionNode {
    pub fn new(op: i32, lhs: Option<NodeRef>, rhs: Option<NodeRef>) -> NodeRef {
        let n = BinaryExpressionNode {
            lhs,
            rhs,
            op,
            slot: None,
            lhstype: None,
            rhstype: None,
            number_usage: None,
            void_result: false,
        };
        Node::new(-1, Kind::BinaryExpression(n)).into_ref()
    }

    pub fn is_boolean_expression(&self) -> bool {
        let op = self.op;
        if op == NOTEQUALS_TOKEN {
            true
        } else if op == STRICTNOTEQUALS_TOKEN {
            true
        } else if op == LOGICALAND_TOKEN {
            true
        } else if op == LOGICALXOR_TOKEN {
            true
        } else if op == LOGICALXORASSIGN_TOKEN {
            true
        } else if op == LOGICALOR_TOKEN {
            true
        } else if op == LESSTHAN_TOKEN {
            true
        } else if op == LESSTHANOREQUALS_TOKEN {
            true
        } else if op == EQUALS_TOKEN {
            true
        } else if op == STRICTEQUALS_TOKEN {
            true
        } else if op == GREATERTHAN_TOKEN {
            true
        } else {
            op == GREATERTHANOREQUALS_TOKEN
        }
    }
}

pub struct BinaryFunctionDefinitionNode {
    pub fdef: FunctionDefinitionNode,
}

impl BinaryFunctionDefinitionNode {
    pub fn new(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        fexpr: Option<NodeRef>,
    ) -> NodeRef {
        let fdef = FunctionDefinitionNode::base(cx, pkgdef, attrs, name, fexpr.clone());
        let node = Node::new(-1, Kind::BinaryFunctionDefinition(BinaryFunctionDefinitionNode { fdef })).into_ref();
        rbm(&fexpr).as_function_common_mut().expect("ClassCastException").def = Some(node.clone());
        node
    }
}

pub struct BinaryInterfaceDefinitionNode {
    pub bcls: BinaryClassDefNode,
}

impl BinaryInterfaceDefinitionNode {
    pub fn new(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        statements: Option<NodeRef>,
    ) -> NodeRef {
        let cls = ClassDefinitionNode::base(cx, pkgdef, attrs, name, None, interfaces, statements);
        Node::new(-1, Kind::BinaryInterfaceDefinition(BinaryInterfaceDefinitionNode { bcls: BinaryClassDefNode { cls } }))
            .into_ref()
    }
}

pub struct BinaryProgramNode {
    pub prog: ProgramNode,
    pub toplevel_definitions: Vec<QName>,
}

impl BinaryProgramNode {
    pub fn new(cx: Option<ContextRef>, statements: Option<NodeRef>) -> NodeRef {
        let prog = ProgramNode::base(cx, statements);
        Node::new(-1, Kind::BinaryProgram(BinaryProgramNode { prog, toplevel_definitions: Vec::new() })).into_ref()
    }
}

pub struct BlockNode {
    pub attributes: Option<NodeRef>,
    pub statements: Option<NodeRef>,
}

impl BlockNode {
    pub fn new(attributes: Option<NodeRef>, statements: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::Block(BlockNode { attributes, statements })).into_ref()
    }
}

pub struct BoxNode {
    pub expr: Option<NodeRef>,
    pub actual: Option<TypeValue>,
    pub void_result: bool,
}

impl BoxNode {
    pub fn new(expr: Option<NodeRef>, actual: Option<TypeValue>) -> NodeRef {
        Node::new(-1, Kind::Box(BoxNode { expr, actual, void_result: false })).into_ref()
    }
}

pub struct BreakStatementNode {
    pub id: Option<NodeRef>,
    pub loop_index: i32,
}

impl BreakStatementNode {
    pub fn new(id: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::BreakStatement(BreakStatementNode { id, loop_index: 0 })).into_ref()
    }
}

pub struct CallExpressionNode {
    pub sel: SelectorNode,
    pub args: Option<NodeRef>,
    pub is_new: bool,
    pub void_result: bool,
}

impl CallExpressionNode {
    pub fn new(expr: Option<NodeRef>, args: Option<NodeRef>) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.expr = expr;
        sel.ref_ = None;
        Node::new(-1, Kind::CallExpression(CallExpressionNode { sel, args, is_new: false, void_result: false })).into_ref()
    }

    pub fn is_rvalue(&self) -> bool {
        self.sel.is_rvalue()
    }
}

pub struct CaseLabelNode {
    pub label: Option<NodeRef>,
}

impl CaseLabelNode {
    pub fn new(label: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::CaseLabel(CaseLabelNode { label })).into_ref()
    }
}

pub struct CatchClauseNode {
    pub parameter: Option<NodeRef>,
    pub statements: Option<NodeRef>,
    pub typeref: Option<ReferenceValue>,
    pub finally_inserted: bool,
    pub default_namespace: Option<ObjectValue>,
    pub activation: Option<ObjectValue>,
}

impl CatchClauseNode {
    pub fn new(parameter: Option<NodeRef>, statements: Option<NodeRef>) -> NodeRef {
        let n = CatchClauseNode {
            parameter,
            statements,
            typeref: None,
            finally_inserted: false,
            default_namespace: None,
            activation: None,
        };
        Node::new(-1, Kind::CatchClause(n)).into_ref()
    }
}

pub struct ClassDefinitionNode {
    pub def: DefinitionNode,
    pub name: Option<NodeRef>,
    pub statements: Option<NodeRef>,
    pub baseclass: Option<NodeRef>,
    pub baseref: Option<ReferenceValue>,
    pub interfaces: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub fexprs: Option<NodeList>,
    pub clsdefs: Option<NodeList>,
    pub instanceinits: Option<NodeList>,
    pub staticfexprs: Option<NodeList>,
    pub var_count: i32,
    pub temp_count: i32,
    pub cframe: Option<TypeValue>,
    pub iframe: Option<ObjectValue>,
    pub is_dynamic: i32,
    pub cx: Option<ContextRef>,
    pub debug_name: Option<String>,
    pub package_name: Option<String>,
    pub owns_cframe: bool,
    pub needs_init: bool,
    pub deferred_subclasses: Option<NodeList>,
    pub init: Option<NodeRef>,
    pub needs_prototype_ns: bool,
    pub version: i32,
    pub imported_names: Multinames,
    pub used_namespaces: Namespaces,
    pub used_def_namespaces: Namespaces,
    pub private_namespace: Option<ObjectValue>,
    pub default_namespace: Option<ObjectValue>,
    pub public_namespace: Option<ObjectValue>,
    pub protected_namespace: Option<ObjectValue>,
    pub static_protected_namespace: Option<ObjectValue>,
    pub is_default_nullable: bool,
    pub state: i32,
    pub namespaces: Namespaces,
}

impl ClassDefinitionNode {
    pub const INIT: i32 = 0;
    pub const INHERIT: i32 = 1;
    pub const MAIN: i32 = 2;

    pub fn base(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        baseclass: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        statements: Option<NodeRef>,
    ) -> ClassDefinitionNode {
        ClassDefinitionNode {
            def: DefinitionNode::base(pkgdef, attrs),
            name,
            statements,
            baseclass,
            baseref: None,
            interfaces,
            ref_: None,
            fexprs: None,
            clsdefs: None,
            instanceinits: None,
            staticfexprs: None,
            var_count: 0,
            temp_count: 0,
            cframe: None,
            iframe: None,
            is_dynamic: 0,
            cx,
            debug_name: None,
            package_name: Some(String::new()),
            owns_cframe: false,
            needs_init: false,
            deferred_subclasses: None,
            init: None,
            needs_prototype_ns: true,
            version: -1,
            imported_names: Multinames::new(),
            used_namespaces: Namespaces::new(),
            used_def_namespaces: Namespaces::new(),
            private_namespace: None,
            default_namespace: None,
            public_namespace: None,
            protected_namespace: None,
            static_protected_namespace: None,
            is_default_nullable: true,
            state: Self::INIT,
            namespaces: Namespaces::new(),
        }
    }

    pub fn new(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        baseclass: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        statements: Option<NodeRef>,
    ) -> NodeRef {
        let n = Self::base(cx, pkgdef, attrs, name, baseclass, interfaces, statements);
        Node::new(-1, Kind::ClassDefinition(n)).into_ref()
    }
}

pub struct ClassNameNode {
    pub pkgname: Option<NodeRef>,
    pub ident: Option<NodeRef>,
    pub non_nullable: bool,
}

impl ClassNameNode {
    pub fn new(pkgname: Option<NodeRef>, ident: Option<NodeRef>, pos: i32) -> NodeRef {
        Node::new(pos, Kind::ClassName(ClassNameNode { pkgname, ident, non_nullable: false })).into_ref()
    }
}

pub struct CoerceNode {
    pub expr: Option<NodeRef>,
    pub actual: Option<TypeInfo>,
    pub expected: Option<TypeInfo>,
    pub void_result: bool,
    pub is_explicit: bool,
}

impl CoerceNode {
    pub fn new(expr: Option<NodeRef>, actual: Option<TypeInfo>, expected: Option<TypeInfo>, is_explicit: bool) -> NodeRef {
        Node::new(-1, Kind::Coerce(CoerceNode { expr, actual, expected, void_result: false, is_explicit })).into_ref()
    }
}

pub struct CommentNode {
    pub type_: i32,
    pub comment: String,
}

impl CommentNode {
    pub fn new(val: String, ctype: i32) -> NodeRef {
        Node::new(-1, Kind::Comment(CommentNode { type_: ctype, comment: val })).into_ref()
    }

    pub fn get_type(&self) -> i32 {
        self.type_
    }
}

pub struct ConditionalExpressionNode {
    pub condition: Option<NodeRef>,
    pub thenexpr: Option<NodeRef>,
    pub elseexpr: Option<NodeRef>,
    pub thenvalue: Option<Value>,
    pub elsevalue: Option<Value>,
}

impl ConditionalExpressionNode {
    pub fn new(condition: Option<NodeRef>, thenexpr: Option<NodeRef>, elseexpr: Option<NodeRef>) -> NodeRef {
        let n = ConditionalExpressionNode { condition, thenexpr, elseexpr, thenvalue: None, elsevalue: None };
        Node::new(-1, Kind::ConditionalExpression(n)).into_ref()
    }
}

pub struct ConfigNamespaceDefinitionNode {
    pub ns: NamespaceDefinitionNode,
}

impl ConfigNamespaceDefinitionNode {
    pub fn new(pkgdef: Option<NodeRef>, attrs: Option<NodeRef>, name: Option<NodeRef>) -> NodeRef {
        let ns = NamespaceDefinitionNode::base(pkgdef, attrs, name, None);
        Node::new(-1, Kind::ConfigNamespaceDefinition(ConfigNamespaceDefinitionNode { ns })).into_ref()
    }
}

pub struct ContinueStatementNode {
    pub id: Option<NodeRef>,
    pub loop_index: i32,
}

impl ContinueStatementNode {
    pub fn new(id: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::ContinueStatement(ContinueStatementNode { id, loop_index: 0 })).into_ref()
    }
}

pub struct DefaultXMLNamespaceNode {
    pub expr: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
}

impl DefaultXMLNamespaceNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::DefaultXMLNamespace(DefaultXMLNamespaceNode { expr, ref_: None })).into_ref()
    }
}

/// Abstract base `DefinitionNode`; its `pos` goes to the enclosing `Node`.
pub struct DefinitionNode {
    pub attrs: Option<NodeRef>,
    pub meta_data: Option<NodeRef>,
    pub pkgdef: Option<NodeRef>,
    skip: bool,
}

impl DefinitionNode {
    pub fn base(pkgdef: Option<NodeRef>, attrs: Option<NodeRef>) -> DefinitionNode {
        DefinitionNode { attrs, meta_data: None, pkgdef, skip: false }
    }

    pub fn skip_node(&mut self, b: bool) {
        self.skip = b;
    }

    pub fn skip(&self) -> bool {
        self.skip
    }

    pub fn add_meta_data_node(&mut self, node: Option<NodeRef>) {
        if self.meta_data.is_none() {
            self.meta_data = Some(StatementListNode::new(node));
        } else {
            rbm(&self.meta_data).as_statement_list_mut().unwrap().items.push(node);
        }
    }
}

pub struct DeleteExpressionNode {
    pub sel: SelectorNode,
    pub op: i32,
    pub slot: Option<Slot>,
    pub void_result: bool,
}

impl DeleteExpressionNode {
    pub fn new(op: i32, expr: Option<NodeRef>) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.expr = expr;
        sel.ref_ = None;
        Node::new(-1, Kind::DeleteExpression(DeleteExpressionNode { sel, op, slot: None, void_result: false })).into_ref()
    }
}

pub struct DoStatementNode {
    pub statements: Option<NodeRef>,
    pub expr: Option<NodeRef>,
    pub loop_index: i32,
}

impl DoStatementNode {
    pub fn new(statements: Option<NodeRef>, expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::DoStatement(DoStatementNode { statements, expr, loop_index: 0 })).into_ref()
    }
}

pub struct DocCommentNode {
    pub meta: MetaDataNode,
    pub meta_data: Option<NodeRef>,
    pub is_default: bool,
}

const NS_PUBLIC: u8 = 0x00;
const NS_INTERNAL: u8 = 0x01;
const NS_PRIVATE: u8 = 0x02;
const NS_PROTECTED: u8 = 0x03;

impl DocCommentNode {
    pub fn new(data: Option<NodeRef>) -> NodeRef {
        let meta = MetaDataNode::base(data);
        Node::new(-1, Kind::DocComment(DocCommentNode { meta, meta_data: None, is_default: false })).into_ref()
    }

    fn emit_meta_data_comment(&self, buf: &mut String, debug_name: &str, meta: &MetaDataNode, is_attribute_of_definition: bool) {
        buf.push_str("\n<metadata>\n");
        let tagname = meta.get_id().unwrap_or_else(|| "null".to_string());
        buf.push_str("\n\t<");
        buf.push_str(&tagname);
        buf.push(' ');
        buf.push_str("owner='");
        buf.push_str(debug_name);
        buf.push_str("' ");

        let mut has_name = false;
        if let Some(values) = meta.get_values() {
            if meta.get_id().as_deref() == Some("SkinStates") {
                let mut first = true;
                for v in &values {
                    if let Some(ov) = v.as_keyless_value() {
                        if first {
                            first = false;
                            buf.push_str("states='");
                        } else {
                            buf.push_str(", ");
                        }
                        buf.push_str(&ov.obj);
                    }
                }
                if !first {
                    buf.push('\'');
                }
            } else {
                for v in &values {
                    if let (Some(ov), false) = (v.as_keyless_value(), has_name) {
                        buf.push_str("name='");
                        buf.push_str(&ov.obj);
                        buf.push_str("' ");
                        has_name = true;
                        continue;
                    }
                    if let Some(kv) = v.as_key_value_pair() {
                        buf.push_str(&kv.key);
                        buf.push_str("='");
                        buf.push_str(&kv.obj);
                        buf.push_str("' ");
                        continue;
                    }
                }
            }
        } else if let Some(id) = meta.get_id() {
            buf.push_str("name='");
            buf.push_str(&id);
            buf.push_str("' ");
        }

        buf.push_str(">\n");

        if !is_attribute_of_definition {
            if let Some(values) = self.meta.get_values() {
                for v in &values {
                    if let Some(ov) = v.as_keyless_value() {
                        buf.push_str(&ov.obj);
                        continue;
                    }
                    if let Some(kv) = v.as_key_value_pair() {
                        buf.push_str("\n\t<");
                        buf.push_str(&kv.key);
                        buf.push('>');
                        buf.push_str(&kv.obj);
                        buf.push_str("</");
                        buf.push_str(&kv.key);
                        buf.push('>');
                        continue;
                    }
                }
            } else if let Some(id) = self.meta.get_id() {
                buf.push_str(&id);
            }
        }

        buf.push_str("\n\t</");
        buf.push_str(&tagname);
        buf.push_str(">\n");
        buf.push_str("</metadata>\n");
    }

    pub fn emit(&self, cx: &ContextRef, buf: &mut String) {
        let mut tagname = String::new();
        let mut debug_name = String::new();
        let def = self.meta.def.clone();

        if let Some(def) = &def {
            let def = def.borrow();
            if let Some(fd) = def.function_definition() {
                let fexpr = rb(&fd.fexpr);
                let fexpr = fexpr.as_function_common().expect("ClassCastException");
                debug_name = fexpr.debug_name.clone();
                tagname = "method".to_string();
                buf.push_str("\n<method name='");
                buf.push_str(&rb(&rb(&fd.name).as_function_name().unwrap().identifier).identifier().unwrap().name);
                buf.push_str("' fullname='");
                buf.push_str(&fexpr.debug_name);
                buf.push_str("' ");
                if fd.def.attrs.is_some() {
                    let attrs = rb(&fd.def.attrs);
                    let attrs = attrs.as_attribute_list().unwrap();
                    buf.push_str("isStatic='");
                    buf.push_str(if attrs.has_static { "true" } else { "false" });
                    buf.push_str("' ");
                    buf.push_str("isFinal='");
                    buf.push_str(if attrs.has_final { "true" } else { "false" });
                    buf.push_str("' ");
                    buf.push_str("isOverride='");
                    buf.push_str(if attrs.has_override { "true" } else { "false" });
                    buf.push_str("' ");
                } else {
                    buf.push_str("isStatic='false' ");
                    buf.push_str("isFinal='false' ");
                    buf.push_str("isOverride='false' ");
                }
                rb(&fexpr.signature).to_canonical_string(cx, buf);
                buf.push('>');
            }
        }

        if let Some(def) = &def {
            let def = def.borrow();
            if let Some(vd) = def.as_variable_definition() {
                let list = rb(&vd.list);
                let vb = rb(&list.list().unwrap().items[0]);
                let vb = vb.as_variable_binding().expect("ClassCastException");
                debug_name = vb.debug_name.clone().unwrap_or_else(|| "null".to_string());
                tagname = "field".to_string();
                buf.push_str("\n<");
                buf.push_str(&tagname);
                buf.push_str(" name='");
                buf.push_str(&rb(&rb(&vb.variable).as_typed_identifier().unwrap().identifier).identifier().unwrap().name);
                buf.push_str("' fullname='");
                buf.push_str(vb.debug_name.as_deref().unwrap_or("null"));
                buf.push_str("' type='");
                if let Some(typeref) = &vb.typeref {
                    buf.push_str(&Self::get_ref_name(cx, typeref));
                }
                buf.push_str("' ");
                if vd.def.attrs.is_some() {
                    let attrs = rb(&vd.def.attrs);
                    buf.push_str("isStatic='");
                    buf.push_str(if attrs.as_attribute_list().unwrap().has_static { "true" } else { "false" });
                    buf.push_str("' ");
                } else {
                    buf.push_str("isStatic='false' ");
                }
                let vb_ref = vb.ref_.as_ref().expect("NullPointerException");
                if let Some(s) = vb_ref.get_slot(&mut cx.borrow_mut()) {
                    buf.push_str("isConst='");
                    buf.push_str(if s.is_const() { "true" } else { "false" });
                    buf.push_str("' ");
                }
                if vb.initializer.is_some() {
                    buf.push_str("defaultValue='");
                    let init = rb(&vb.initializer);
                    if let Some(n) = init.as_literal_number() {
                        buf.push_str(&n.value);
                    } else if let Some(n) = init.as_literal_string() {
                        buf.push_str(&Self::escape_xml(&n.value));
                    } else if init.as_literal_null().is_some() {
                        buf.push_str("null");
                    } else if let Some(n) = init.as_literal_boolean() {
                        buf.push_str(if n.value { "true" } else { "false" });
                    } else if let Some(mb) = init.as_member_expression() {
                        let vs = match &mb.ref_ {
                            Some(r) => r.get_slot_kind(&mut cx.borrow_mut(), GET_TOKEN),
                            None => None,
                        };
                        let v = vs.and_then(|vs| vs.get_value());
                        let ov = v.and_then(|v| v.as_object_value());
                        buf.push_str(&ov.map_or_else(|| "unknown".to_string(), |ov| ov.get_value()));
                    } else {
                        let vs = vb_ref.get_slot_kind(&mut cx.borrow_mut(), GET_TOKEN);
                        let v = vs.and_then(|vs| vs.get_value());
                        let ov = v.and_then(|v| v.as_object_value());
                        buf.push_str(&ov.map_or_else(|| "unknown".to_string(), |ov| ov.get_value()));
                    }
                    buf.push_str("' ");
                }
                buf.push('>');
            }
        }

        if let Some(def) = &def {
            let def = def.borrow();
            if let Some(pd) = def.as_package_definition() {
                tagname = "packageRec".to_string();
                debug_name = String::new();
                let name = rb(&pd.name);
                let name = name.as_package_name().unwrap();
                let (pkg_part, def_part) = match &name.id {
                    Some(id) => {
                        let id = id.borrow();
                        let id = id.as_package_identifiers().unwrap();
                        (
                            id.pkg_part.clone().unwrap_or_else(|| "null".to_string()),
                            id.def_part.clone().unwrap_or_else(|| "null".to_string()),
                        )
                    }
                    None => (String::new(), String::new()),
                };
                buf.push_str("\n<");
                buf.push_str(&tagname);
                buf.push_str(" name='");
                buf.push_str(&pkg_part);
                buf.push('.');
                buf.push_str(&def_part);
                buf.push_str("' fullname='");
                buf.push_str(&pkg_part);
                buf.push('.');
                buf.push_str(&def_part);
                buf.push_str("'>\n");
            }
        }

        if let Some(def_ref) = &def {
            let def = def_ref.borrow();
            if let Some(cd) = def.class_definition() {
                debug_name = cd.debug_name.clone().unwrap_or_else(|| "null".to_string());
                let is_interface = def.as_interface_definition().is_some();
                tagname = if is_interface { "interfaceRec".to_string() } else { "classRec".to_string() };
                buf.push_str("\n<");
                buf.push_str(&tagname);
                buf.push_str(" name='");
                buf.push_str(&rb(&cd.name).identifier().unwrap().name);
                buf.push_str("' fullname='");
                buf.push_str(&debug_name);
                {
                    let cd_cx = cd.cx.as_ref().expect("NullPointerException").borrow();
                    if let Some(input) = &cd_cx.input {
                        let origin = input.borrow().origin.clone().expect("NullPointerException");
                        if !origin.is_empty() {
                            buf.push_str("' sourcefile='");
                            buf.push_str(&origin);
                        }
                    }
                }
                let ns = cd.cframe.as_ref().expect("NullPointerException").builder().expect("NullPointerException").classname().expect("NullPointerException").ns().expect("NullPointerException");
                buf.push_str("' namespace='");
                buf.push_str(&ns.name());
                buf.push_str("' access='");
                buf.push_str(Self::get_access_kind_from_ns(&ns));

                let append_interfaces = |cx: &ContextRef, buf: &mut String, interfaces: &Option<NodeRef>| {
                    let interfaces = rb(interfaces);
                    let values = &interfaces.list().unwrap().values;
                    let first_v = values[0].clone();
                    for v in values {
                        let rv = v.as_reference_value().expect("ClassCastException");
                        if !v.ptr_eq(&first_v) {
                            buf.push(';');
                        }
                        let s = rv.get_slot_kind(&mut cx.borrow_mut(), GET_TOKEN);
                        match s {
                            Some(s) if !s.get_debug_name().is_empty() => buf.push_str(&s.get_debug_name()),
                            _ => buf.push_str(&rv.name()),
                        }
                    }
                };

                if is_interface {
                    buf.push_str("' baseClasses='");
                    if cd.interfaces.is_some() {
                        append_interfaces(cx, buf, &cd.interfaces);
                    } else {
                        buf.push_str("Object");
                    }
                    buf.push_str("' ");
                } else {
                    buf.push_str("' baseclass='");
                    if let Some(baseref) = &cd.baseref {
                        let s = baseref.get_slot_kind(&mut cx.borrow_mut(), GET_TOKEN);
                        match s {
                            Some(s) if !s.get_debug_name().is_empty() => buf.push_str(&s.get_debug_name()),
                            _ => buf.push_str("Object"),
                        }
                    } else {
                        buf.push_str("Object");
                    }
                    buf.push_str("' ");
                    if cd.interfaces.is_some() {
                        buf.push_str("interfaces='");
                        append_interfaces(cx, buf, &cd.interfaces);
                        buf.push_str("' ");
                    }
                }
                if cd.def.attrs.is_some() {
                    let attrs = rb(&cd.def.attrs);
                    let attrs = attrs.as_attribute_list().unwrap();
                    buf.push_str("isFinal='");
                    buf.push_str(if attrs.has_final { "true" } else { "false" });
                    buf.push_str("' ");
                    buf.push_str("isDynamic='");
                    buf.push_str(if attrs.has_dynamic { "true" } else { "false" });
                    buf.push_str("' ");
                } else {
                    buf.push_str("isFinal='false' ");
                    buf.push_str("isDynamic='false' ");
                }
                buf.push('>');
            }
        }

        if let Some(values) = self.meta.get_values() {
            for v in &values {
                if let Some(ov) = v.as_keyless_value() {
                    buf.push_str(&ov.obj);
                    continue;
                }
                if let Some(kv) = v.as_key_value_pair() {
                    buf.push_str("\n<");
                    buf.push_str(&kv.key);
                    buf.push('>');
                    buf.push_str(&kv.obj);
                    buf.push_str("</");
                    buf.push_str(&kv.key);
                    buf.push('>');
                    continue;
                }
            }
        } else if let Some(id) = self.meta.get_id() {
            buf.push_str(&id);
        }

        if let Some(def_ref) = &def {
            let meta_data = def_ref.borrow().definition().and_then(|d| d.meta_data.clone());
            if let Some(meta_data) = meta_data {
                let items: Vec<Option<NodeRef>> = meta_data.borrow().as_statement_list().unwrap().items.clone();
                let num_items = items.len();
                let mut x = 0;
                while x < num_items {
                    let md = items[x].clone();
                    let md_node = md.as_ref().map(|m| m.borrow());
                    let mdi = md_node.as_ref().and_then(|m| m.meta_data());
                    if let Some(mdi) = mdi {
                        if let Some(id) = mdi.get_id() {
                            if id == "Style" || id == "Event" || id == "Effect" {
                                if x + 1 < num_items {
                                    let next = &items[x + 1];
                                    let next_node = next.as_ref().map(|n| n.borrow());
                                    let mut meta_data_comment = next_node.as_ref().and_then(|n| n.as_doc_comment());
                                    if let Some(c) = meta_data_comment {
                                        if !same_node(&c.meta_data, &md) {
                                            meta_data_comment = None;
                                        }
                                    }
                                    if let Some(c) = meta_data_comment {
                                        c.emit_meta_data_comment(buf, &debug_name, mdi, false);
                                        x += 1;
                                    } else {
                                        self.emit_meta_data_comment(buf, &debug_name, mdi, true);
                                    }
                                } else {
                                    self.emit_meta_data_comment(buf, &debug_name, mdi, true);
                                }
                            } else if id == "Bindable"
                                || id == "Deprecated"
                                || id == "Exclude"
                                || id == "DefaultProperty"
                                || id == "SkinStates"
                            {
                                self.emit_meta_data_comment(buf, &debug_name, mdi, true);
                            }
                        }
                    }
                    x += 1;
                }
            }
        }

        if !tagname.is_empty() {
            buf.push_str("\n</");
            buf.push_str(&tagname);
            buf.push('>');
        } else if let Some(def_ref) = &def {
            let def = def_ref.borrow();
            if let Some(nd) = def.namespace_definition() {
                buf.push_str("<!-- Namespace comments not supported yet: ");
                buf.push_str(nd.debug_name.as_deref().unwrap_or("null"));
                buf.push_str("-->");
            }
        }
    }

    fn get_access_kind_from_ns(ns: &ObjectValue) -> &'static str {
        match ns.get_namespace_kind() {
            NS_PUBLIC => "public",
            NS_INTERNAL => "internal",
            NS_PROTECTED => "protected",
            NS_PRIVATE => "private",
            _ => "public",
        }
    }

    pub fn get_ref_name(cx: &ContextRef, ref_: &ReferenceValue) -> String {
        let s = ref_.get_slot_kind(&mut cx.borrow_mut(), GET_TOKEN);
        match s {
            Some(s) if !s.get_debug_name().is_empty() => s.get_debug_name(),
            s => {
                let mut name = ref_.name();
                if ref_.type_params().is_some() {
                    if let Some(t) = s.as_ref().and_then(|s| s.get_value()).and_then(|v| v.as_type_value()) {
                        name += &Self::get_indexed_type_name(cx, t.indexed_type());
                    }
                }
                name
            }
        }
    }

    fn get_indexed_type_name(cx: &ContextRef, t: Option<TypeValue>) -> String {
        let t = t.expect("NullPointerException");
        let mut name = "$".to_string();
        if t.name().is_parameterized_name() {
            name += &t.name().name();
            if t.indexed_type().is_some() {
                name += &Self::get_indexed_type_name(cx, t.indexed_type());
            }
        } else {
            name += &t.to_string();
        }
        name
    }

    pub fn escape_xml(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 16);
        for c in s.chars() {
            match c {
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '&' => out.push_str("&amp;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                _ => out.push(c),
            }
        }
        out
    }
}

pub struct EmptyElementNode;

impl EmptyElementNode {
    pub fn new() -> NodeRef {
        Node::new(-1, Kind::EmptyElement(EmptyElementNode)).into_ref()
    }
}

pub struct EmptyStatementNode;

thread_local! {
    static EMPTY_STATEMENT_SINGLETON: NodeRef = Node::new(1, Kind::EmptyStatement(EmptyStatementNode)).into_ref();
}

impl EmptyStatementNode {
    pub fn get_instance() -> NodeRef {
        EMPTY_STATEMENT_SINGLETON.with(|n| n.clone())
    }
}

pub struct ErrorNode {
    pub error_arg: String,
    pub error_code: i32,
}

impl ErrorNode {
    pub fn new(error_code: i32, value: String) -> NodeRef {
        Node::new(-1, Kind::Error(ErrorNode { error_arg: value, error_code })).into_ref()
    }
}

pub struct ExpressionStatementNode {
    pub expr: Option<NodeRef>,
    pub gen_bits: Option<BitSet>,
    pub ref_: Option<ReferenceValue>,
    pub expected_type: Option<TypeValue>,
    pub is_var_stmt: bool,
    skip: bool,
}

impl ExpressionStatementNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        let n = ExpressionStatementNode { expr, gen_bits: None, ref_: None, expected_type: None, is_var_stmt: false, skip: false };
        Node::new(-1, Kind::ExpressionStatement(n)).into_ref()
    }

    pub fn set_is_var_statement(&mut self, b: bool) {
        self.is_var_stmt = b;
    }

    pub fn is_var_statement(&self) -> bool {
        self.is_var_stmt
    }

    pub fn skip_node(&mut self, b: bool) {
        self.skip = b;
    }

    pub fn skip(&self) -> bool {
        self.skip
    }
}

pub struct FinallyClauseNode {
    pub statements: Option<NodeRef>,
    pub default_catch: Option<NodeRef>,
}

impl FinallyClauseNode {
    pub fn new(statements: Option<NodeRef>, default_catch: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::FinallyClause(FinallyClauseNode { statements, default_catch })).into_ref()
    }
}

pub struct ForStatementNode {
    pub initialize: Option<NodeRef>,
    pub test: Option<NodeRef>,
    pub increment: Option<NodeRef>,
    pub statement: Option<NodeRef>,
    pub is_forin: bool,
    pub loop_index: i32,
}

impl ForStatementNode {
    pub fn new(
        initialize: Option<NodeRef>,
        test: Option<NodeRef>,
        increment: Option<NodeRef>,
        statement: Option<NodeRef>,
        is_forin: bool,
    ) -> NodeRef {
        let n = ForStatementNode { initialize, test, increment, statement, is_forin, loop_index: 0 };
        Node::new(-1, Kind::ForStatement(n)).into_ref()
    }
}

pub struct FunctionCommonNode {
    pub def: Option<NodeRef>,
    pub identifier: Option<NodeRef>,
    pub signature: Option<NodeRef>,
    pub body: Option<NodeRef>,
    pub fixed_count: i32,
    pub fun: Option<ObjectValue>,
    pub ref_: Option<ReferenceValue>,
    pub fexprs: Option<NodeList>,
    pub var_count: i32,
    pub temp_count: i32,
    pub internal_name: String,
    pub blocks: Option<Vec<BlockRef>>,
    pub with_depth: i32,
    pub scope_chain: Option<Vec<ObjectValue>>,
    pub debug_name: String,
    pub needs_arguments: i32,
    pub cx: Option<ContextRef>,
    pub kind: i32,
    pub namespace_ids: Option<Vec<String>>,
    pub use_stmts: Option<NodeRef>,
    pub default_dxns: Option<NodeRef>,
    pub imported_names: Option<Multinames>,
    pub used_namespaces: Option<Namespaces>,
    pub private_namespace: Option<ObjectValue>,
    pub default_namespace: Option<ObjectValue>,
    pub public_namespace: Option<ObjectValue>,
    flags: i32,
    void_result: bool,
}

impl FunctionCommonNode {
    const USER_DEFINED_BODY_Flag: i32 = 1;
    const IS_FUNDEF_Flag: i32 = 2;
    const WITH_USED_Flag: i32 = 4;
    const IS_NATIVE_Flag: i32 = 8;
    const EXCEPTIONS_USED_Flag: i32 = 16;
    const NAMED_INNER_Flag: i32 = 32;

    pub fn new(
        cx: Option<ContextRef>,
        use_stmts: Option<NodeRef>,
        internal_name: String,
        identifier: Option<NodeRef>,
        signature: Option<NodeRef>,
        body: Option<NodeRef>,
        has_user_defined_body: bool,
    ) -> NodeRef {
        let mut n = FunctionCommonNode {
            def: None,
            identifier,
            signature,
            body,
            fixed_count: 0,
            fun: None,
            ref_: None,
            fexprs: None,
            var_count: 0,
            temp_count: 0,
            internal_name,
            blocks: None,
            with_depth: -1,
            scope_chain: None,
            debug_name: String::new(),
            needs_arguments: 0,
            cx,
            kind: EMPTY_TOKEN,
            namespace_ids: None,
            use_stmts,
            default_dxns: None,
            imported_names: None,
            used_namespaces: None,
            private_namespace: None,
            default_namespace: None,
            public_namespace: None,
            flags: 0,
            void_result: false,
        };
        n.set_native(false);
        n.set_function_definition(false);
        n.set_user_defined_body(has_user_defined_body);
        Node::new(-1, Kind::FunctionCommon(n)).into_ref()
    }

    pub fn set_function_definition(&mut self, is_fundef: bool) {
        self.flags = if is_fundef { self.flags | Self::IS_FUNDEF_Flag } else { self.flags & !Self::IS_FUNDEF_Flag };
    }

    pub fn is_function_definition(&self) -> bool {
        (self.flags & Self::IS_FUNDEF_Flag) != 0
    }

    pub fn set_user_defined_body(&mut self, user_defined_body: bool) {
        self.flags = if user_defined_body {
            self.flags | Self::USER_DEFINED_BODY_Flag
        } else {
            self.flags & !Self::USER_DEFINED_BODY_Flag
        };
    }

    pub fn is_user_defined_body(&self) -> bool {
        (self.flags & Self::USER_DEFINED_BODY_Flag) != 0
    }

    pub fn set_native(&mut self, is_native: bool) {
        self.flags = if is_native { self.flags | Self::IS_NATIVE_Flag } else { self.flags & !Self::IS_NATIVE_Flag };
    }

    pub fn is_native(&self) -> bool {
        (self.flags & Self::IS_NATIVE_Flag) != 0
    }

    pub fn set_with_used(&mut self, with_used: bool) {
        self.flags = if with_used { self.flags | Self::WITH_USED_Flag } else { self.flags & !Self::WITH_USED_Flag };
    }

    pub fn is_with_used(&self) -> bool {
        (self.flags & Self::WITH_USED_Flag) != 0
    }

    pub fn set_named_inner_func(&mut self, is_named_inner_func: bool) {
        self.flags =
            if is_named_inner_func { self.flags | Self::NAMED_INNER_Flag } else { self.flags & !Self::NAMED_INNER_Flag };
    }

    pub fn is_named_inner_func(&self) -> bool {
        (self.flags & Self::NAMED_INNER_Flag) != 0
    }

    pub fn set_exceptions_used(&mut self, exceptions_used: bool) {
        self.flags = if exceptions_used {
            self.flags | Self::EXCEPTIONS_USED_Flag
        } else {
            self.flags & !Self::EXCEPTIONS_USED_Flag
        };
    }

    pub fn is_exceptions_used(&self) -> bool {
        (self.flags & Self::EXCEPTIONS_USED_Flag) != 0
    }

    pub fn is_void_result(&self) -> bool {
        self.void_result
    }
}

pub struct FunctionDefinitionNode {
    pub def: DefinitionNode,
    pub name: Option<NodeRef>,
    pub fexpr: Option<NodeRef>,
    pub fixed_count: i32,
    pub fun: Option<ObjectValue>,
    pub ref_: Option<ReferenceValue>,
    pub cx: Option<ContextRef>,
    pub init: Option<NodeRef>,
    pub needs_init: bool,
    pub is_prototype: bool,
    pub version: i32,
}

impl FunctionDefinitionNode {
    /// The fields only; the caller sets `fexpr.def` once the node exists.
    pub fn base(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        fexpr: Option<NodeRef>,
    ) -> FunctionDefinitionNode {
        FunctionDefinitionNode {
            def: DefinitionNode::base(pkgdef, attrs),
            name,
            fexpr,
            fixed_count: 0,
            fun: None,
            ref_: None,
            cx,
            init: None,
            needs_init: false,
            is_prototype: false,
            version: -1,
        }
    }

    pub fn new(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        fexpr: Option<NodeRef>,
    ) -> NodeRef {
        let n = Self::base(cx, pkgdef, attrs, name, fexpr.clone());
        let node = Node::new(-1, Kind::FunctionDefinition(n)).into_ref();
        rbm(&fexpr).as_function_common_mut().expect("ClassCastException").def = Some(node.clone());
        node
    }
}

pub struct FunctionNameNode {
    pub identifier: Option<NodeRef>,
    pub kind: i32,
}

impl FunctionNameNode {
    pub fn new(kind: i32, identifier: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::FunctionName(FunctionNameNode { identifier, kind })).into_ref()
    }
}

pub struct FunctionSignatureNode {
    pub parameter: Option<NodeRef>,
    pub result: Option<NodeRef>,
    pub type_: Option<TypeInfo>,
    pub typeref: Option<ReferenceValue>,
    pub inits: Option<NodeRef>,
    pub no_anno: bool,
    pub void_anno: bool,
}

impl FunctionSignatureNode {
    pub fn new(parameter: Option<NodeRef>, result: Option<NodeRef>) -> NodeRef {
        let n = FunctionSignatureNode {
            parameter,
            result,
            type_: None,
            typeref: None,
            inits: None,
            no_anno: false,
            void_anno: false,
        };
        Node::new(-1, Kind::FunctionSignature(n)).into_ref()
    }

    /// The `(ParameterListNode, ListNode initializers)` constructor.
    pub fn new_with_inits(parameter: Option<NodeRef>, initializers: Option<NodeRef>) -> NodeRef {
        let n = FunctionSignatureNode {
            parameter,
            result: None,
            type_: None,
            typeref: None,
            inits: initializers,
            no_anno: false,
            void_anno: false,
        };
        Node::new(-1, Kind::FunctionSignature(n)).into_ref()
    }

    pub fn size(&self) -> i32 {
        match &self.parameter {
            Some(p) => p.borrow_mut().as_parameter_list_mut().expect("ClassCastException").size(),
            None => 0,
        }
    }

    pub fn to_canonical_string(&self, cx: &ContextRef, buff: &mut String) {
        if self.parameter.is_some() {
            rb(&self.parameter).to_canonical_string(cx, buff);
        }
        buff.push_str(" result_type='");
        if self.result.is_some() {
            rb(&self.result).to_canonical_string(cx, buff);
        } else if self.void_anno {
            buff.push_str("void");
        } else {
            buff.push_str(&cx.borrow().no_type().name().to_string());
        }
        buff.push('\'');
    }
}

pub struct GetExpressionNode {
    pub sel: SelectorNode,
}

impl GetExpressionNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.expr = expr;
        sel.ref_ = None;
        Node::new(-1, Kind::GetExpression(GetExpressionNode { sel })).into_ref()
    }
}

pub struct HasNextNode {
    pub object_register: Option<NodeRef>,
    pub index_register: Option<NodeRef>,
}

impl HasNextNode {
    pub fn new(object_register: Option<NodeRef>, index_register: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::HasNext(HasNextNode { object_register, index_register })).into_ref()
    }
}

pub struct IdentifierErrorNode {
    pub id: IdentifierNode,
    pub value: String,
}

impl IdentifierErrorNode {
    pub fn new() -> NodeRef {
        let id = IdentifierNode::base("");
        let node = Node::new(0, Kind::IdentifierError(IdentifierErrorNode { id, value: "Expecting an identifier".to_string() }));
        node.into_ref()
    }
}

pub struct IdentifierNode {
    pub name: String,
    pub ref_: Option<ReferenceValue>,
    pub auth_orig_type_token: i32,
}

impl IdentifierNode {
    const ASTERISK: &'static str = "*";

    pub fn base(name: &str) -> IdentifierNode {
        IdentifierNode { name: name.to_string(), ref_: None, auth_orig_type_token: -1 }
    }

    /// Applies the `IdentifierNode` constructor's `setAny` to the finished node.
    fn init(mut node: Node, name: &str) -> NodeRef {
        if name == Self::ASTERISK {
            node.set_any(true);
        }
        node.into_ref()
    }

    pub fn new(name: &str, pos: i32) -> NodeRef {
        Self::new_intern(name, true, pos)
    }

    pub fn new_intern(name: &str, _intern: bool, pos: i32) -> NodeRef {
        Self::init(Node::new(pos, Kind::Identifier(Self::base(name))), name)
    }

    pub fn set_orig_type_token(&mut self, token: i32) {
        self.auth_orig_type_token = token;
    }

    pub fn get_orig_type_token(&self) -> i32 {
        self.auth_orig_type_token
    }

    pub fn to_identifier_string(&self) -> String {
        self.name.clone()
    }
}

pub struct IfStatementNode {
    pub condition: Option<NodeRef>,
    pub thenactions: Option<NodeRef>,
    pub elseactions: Option<NodeRef>,
    pub is_true: bool,
    pub is_false: bool,
}

impl IfStatementNode {
    pub fn new(condition: Option<NodeRef>, thenactions: Option<NodeRef>, elseactions: Option<NodeRef>) -> NodeRef {
        let n = IfStatementNode { condition, thenactions, elseactions, is_true: false, is_false: false };
        Node::new(-1, Kind::IfStatement(n)).into_ref()
    }
}

pub struct ImportDirectiveNode {
    pub def: DefinitionNode,
    /// Hides `DefinitionNode.attrs`, as the Java field does.
    pub attrs: Option<NodeRef>,
    pub name: Option<NodeRef>,
    pub pkg_node: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub package_retrieved: bool,
    pub cx: Option<ContextRef>,
}

impl ImportDirectiveNode {
    pub fn new(
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        pkg_node: Option<NodeRef>,
        cx: Option<ContextRef>,
    ) -> NodeRef {
        let n = ImportDirectiveNode {
            def: DefinitionNode::base(pkgdef, attrs.clone()),
            attrs,
            name,
            pkg_node,
            ref_: None,
            package_retrieved: false,
            cx,
        };
        Node::new(-1, Kind::ImportDirective(n)).into_ref()
    }
}

pub struct ImportNode {
    pub filespec: Option<NodeRef>,
    pub program: Option<NodeRef>,
}

impl ImportNode {
    pub fn new(spec: Option<NodeRef>, prog: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::Import(ImportNode { filespec: spec, program: prog })).into_ref()
    }
}

pub struct IncludeDirectiveNode {
    pub def: DefinitionNode,
    pub filespec: Option<NodeRef>,
    pub program: Option<NodeRef>,
    pub cx: Option<ContextRef>,
    pub in_this_include: bool,
    pub prev_cx: Option<ContextRef>,
}

impl IncludeDirectiveNode {
    pub fn new(cx: Option<ContextRef>, filespec: Option<NodeRef>, program: Option<NodeRef>) -> NodeRef {
        let n = IncludeDirectiveNode {
            def: DefinitionNode::base(None, None),
            filespec,
            program,
            cx,
            in_this_include: false,
            prev_cx: None,
        };
        Node::new(-1, Kind::IncludeDirective(n)).into_ref()
    }
}

pub struct IncrementNode {
    pub sel: SelectorNode,
    pub op: i32,
    pub is_postfix: bool,
    pub slot: Option<Slot>,
    pub number_usage: Option<NumberUsage>,
    pub void_result: bool,
}

impl IncrementNode {
    pub fn new(op: i32, expr: Option<NodeRef>, is_postfix: bool) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.expr = expr;
        sel.ref_ = None;
        let n = IncrementNode { sel, op, is_postfix, slot: None, number_usage: None, void_result: false };
        Node::new(-1, Kind::Increment(n)).into_ref()
    }
}

pub struct InheritanceNode {
    pub baseclass: Option<NodeRef>,
    pub interfaces: Option<NodeRef>,
}

impl InheritanceNode {
    pub fn new(baseclass: Option<NodeRef>, interfaces: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::Inheritance(InheritanceNode { baseclass, interfaces })).into_ref()
    }
}

pub struct InterfaceDefinitionNode {
    pub cls: ClassDefinitionNode,
}

impl InterfaceDefinitionNode {
    pub fn new(
        cx: Option<ContextRef>,
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        name: Option<NodeRef>,
        interfaces: Option<NodeRef>,
        statements: Option<NodeRef>,
    ) -> NodeRef {
        let cls = ClassDefinitionNode::base(cx, pkgdef, attrs, name, None, interfaces, statements);
        Node::new(-1, Kind::InterfaceDefinition(InterfaceDefinitionNode { cls })).into_ref()
    }
}

pub struct InvokeNode {
    pub sel: SelectorNode,
    pub name: String,
    pub args: Option<NodeRef>,
    pub index: i32,
}

impl InvokeNode {
    pub fn new(name: &str, args: Option<NodeRef>) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.ref_ = None;
        Node::new(-1, Kind::Invoke(InvokeNode { sel, name: name.to_string(), args, index: 0 })).into_ref()
    }
}

pub struct LabeledStatementNode {
    pub label: Option<NodeRef>,
    pub statement: Option<NodeRef>,
    pub loop_index: i32,
    pub is_loop_label: bool,
}

impl LabeledStatementNode {
    pub fn new(label: Option<NodeRef>, is_loop_label: bool, statement: Option<NodeRef>) -> NodeRef {
        let n = LabeledStatementNode { label, statement, loop_index: 0, is_loop_label };
        Node::new(-1, Kind::LabeledStatement(n)).into_ref()
    }
}

pub struct ListErrorNode {
    pub list: ListNode,
    pub value: String,
}

impl ListErrorNode {
    pub fn new(str: String) -> NodeRef {
        Node::new(0, Kind::ListError(ListErrorNode { list: ListNode::base(None), value: str })).into_ref()
    }
}

pub struct ListNode {
    pub items: Vec<Option<NodeRef>>,
    pub values: Vec<Value>,
}

impl ListNode {
    pub fn base(item: Option<NodeRef>) -> ListNode {
        ListNode { items: vec![item], values: Vec::with_capacity(1) }
    }

    pub fn new(_list: Option<NodeRef>, item: Option<NodeRef>, pos: i32) -> NodeRef {
        Node::new(pos, Kind::List(Self::base(item))).into_ref()
    }

    pub fn size(&self) -> i32 {
        self.items.len() as i32
    }
}

pub struct LiteralArrayNode {
    pub elementlist: Option<NodeRef>,
    pub value: Option<Value>,
    pub void_result: bool,
}

impl LiteralArrayNode {
    pub fn new(elementlist: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::LiteralArray(LiteralArrayNode { elementlist, value: None, void_result: false })).into_ref()
    }
}

pub struct LiteralBooleanNode {
    pub value: bool,
    pub void_result: bool,
}

impl LiteralBooleanNode {
    pub fn new(value: bool) -> NodeRef {
        Node::new(-1, Kind::LiteralBoolean(LiteralBooleanNode { value, void_result: false })).into_ref()
    }
}

pub struct LiteralFieldNode {
    pub name: Option<NodeRef>,
    pub value: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
}

impl LiteralFieldNode {
    pub fn new(name: Option<NodeRef>, value: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::LiteralField(LiteralFieldNode { name, value, ref_: None })).into_ref()
    }
}

pub struct LiteralNullNode {
    pub void_result: bool,
}

impl LiteralNullNode {
    pub fn new() -> NodeRef {
        Node::new(-1, Kind::LiteralNull(LiteralNullNode { void_result: false })).into_ref()
    }
}

pub struct LiteralNumberNode {
    pub type_: Option<TypeValue>,
    pub value: String,
    pub numeric_value: Option<NumberConstant>,
    pub number_usage: Option<NumberUsage>,
    pub void_result: bool,
}

impl LiteralNumberNode {
    pub fn new(value: &str) -> NodeRef {
        let n = LiteralNumberNode {
            type_: None,
            value: value.to_string(),
            numeric_value: None,
            number_usage: None,
            void_result: false,
        };
        Node::new(-1, Kind::LiteralNumber(n)).into_ref()
    }

    pub fn int_value(&self) -> i32 {
        self.value.parse::<i32>().expect("NumberFormatException")
    }

    pub fn negate(&mut self) {
        if self.value.chars().next().expect("StringIndexOutOfBoundsException") == '-' {
            self.value = self.value[1..].to_string();
        } else {
            self.value = format!("-{}", self.value);
        }
    }
}

pub struct LiteralObjectNode {
    pub fieldlist: Option<NodeRef>,
    pub void_result: bool,
}

impl LiteralObjectNode {
    pub fn new(fieldlist: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::LiteralObject(LiteralObjectNode { fieldlist, void_result: false })).into_ref()
    }
}

pub struct LiteralRegExpNode {
    pub value: String,
    pub void_result: bool,
}

impl LiteralRegExpNode {
    pub fn new(value: String) -> NodeRef {
        Node::new(-1, Kind::LiteralRegExp(LiteralRegExpNode { value, void_result: false })).into_ref()
    }
}

pub struct LiteralStringNode {
    pub value: String,
    delimiter_type: i32,
    pub void_result: bool,
}

impl LiteralStringNode {
    const SINGLE_QUOTE_DELIMITER: i32 = 2;
    const DOUBLE_QUOTE_DELIMITER: i32 = 1;
    const OTHER_DELIMITER: i32 = 0;

    pub fn is_single_quote(&self) -> bool {
        self.delimiter_type == Self::SINGLE_QUOTE_DELIMITER
    }

    pub fn is_double_quote(&self) -> bool {
        self.delimiter_type == Self::DOUBLE_QUOTE_DELIMITER
    }

    pub fn new(value: &str) -> NodeRef {
        let n = LiteralStringNode { value: value.to_string(), delimiter_type: Self::OTHER_DELIMITER, void_result: false };
        Node::new(-1, Kind::LiteralString(n)).into_ref()
    }

    pub fn new_quoted(value: &str, single_quoted: bool) -> NodeRef {
        Self::new_quoted_intern(value, single_quoted, true)
    }

    pub fn new_quoted_intern(value: &str, single_quoted: bool, _intern: bool) -> NodeRef {
        let delimiter_type = if single_quoted { Self::SINGLE_QUOTE_DELIMITER } else { Self::DOUBLE_QUOTE_DELIMITER };
        let n = LiteralStringNode { value: value.to_string(), delimiter_type, void_result: false };
        Node::new(-1, Kind::LiteralString(n)).into_ref()
    }
}

pub struct LiteralVectorNode {
    pub elementlist: Option<NodeRef>,
    pub type_: Option<NodeRef>,
    pub void_result: bool,
    pub value: Option<Value>,
}

impl LiteralVectorNode {
    pub fn new(elementlist: Option<NodeRef>, type_: Option<NodeRef>) -> NodeRef {
        let n = LiteralVectorNode { elementlist, type_, void_result: false, value: None };
        Node::new(-1, Kind::LiteralVector(n)).into_ref()
    }
}

pub struct LiteralXMLNode {
    pub list: Option<NodeRef>,
    pub is_xmllist: bool,
    pub void_result: bool,
}

impl LiteralXMLNode {
    pub fn new(list: Option<NodeRef>, is_xmllist: bool) -> NodeRef {
        Node::new(-1, Kind::LiteralXML(LiteralXMLNode { list, is_xmllist, void_result: false })).into_ref()
    }
}

pub struct LoadRegisterNode {
    pub reg: Option<NodeRef>,
    pub type_: Option<TypeValue>,
    pub void_result: bool,
}

impl LoadRegisterNode {
    pub fn new(reg: Option<NodeRef>, type_: Option<TypeValue>) -> NodeRef {
        Node::new(-1, Kind::LoadRegister(LoadRegisterNode { reg, type_, void_result: false })).into_ref()
    }
}

pub struct MemberExpressionNode {
    pub base: Option<NodeRef>,
    pub selector: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub auth_orig_token: i32,
}

impl MemberExpressionNode {
    pub fn set_orig_token(&mut self, token: i32) {
        self.auth_orig_token = token;
    }

    pub fn get_orig_token(&self) -> i32 {
        self.auth_orig_token
    }

    pub fn new(base: Option<NodeRef>, selector: Option<NodeRef>, pos: i32) -> NodeRef {
        let n = MemberExpressionNode { base, selector, ref_: None, auth_orig_token: -1 };
        Node::new(pos, Kind::MemberExpression(n)).into_ref()
    }

    pub fn is_indexed_member_expression(&self) -> bool {
        rb(&self.selector).selector().expect("ClassCastException").get_mode() == LEFTBRACKET_TOKEN
    }
}

pub struct MetaDataNode {
    pub data: Option<NodeRef>,
    md: Option<MetaData>,
    pub def: Option<NodeRef>,
}

impl MetaDataNode {
    pub fn base(data: Option<NodeRef>) -> MetaDataNode {
        MetaDataNode { data, md: None, def: None }
    }

    pub fn new(data: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::MetaData(Self::base(data))).into_ref()
    }

    pub fn set_metadata(&mut self, md: Option<MetaData>) {
        self.md = md;
    }

    pub fn get_value_key(&self, key: &str) -> Option<String> {
        self.md.as_ref().and_then(|md| md.get_value_key(key))
    }

    pub fn get_value_index(&self, index: i32) -> Option<String> {
        self.md.as_ref().and_then(|md| md.get_value_index(index))
    }

    pub fn count(&self) -> i32 {
        self.get_values().map_or(0, |v| v.len() as i32)
    }

    pub fn get_id(&self) -> Option<String> {
        self.md.as_ref().and_then(|md| md.id())
    }

    pub fn set_id(&mut self, id: Option<String>) {
        self.md.get_or_insert_with(MetaData::new).set_id(id);
    }

    pub fn get_values(&self) -> Option<Vec<Value>> {
        self.md.as_ref().and_then(|md| md.values())
    }

    pub fn set_values(&mut self, values: Option<Vec<Value>>) {
        self.md.get_or_insert_with(MetaData::new).set_values(values);
    }

    pub fn get_metadata(&self) -> Option<MetaData> {
        self.md.clone()
    }
}

pub struct NamespaceDefinitionNode {
    pub def: DefinitionNode,
    pub name: Option<NodeRef>,
    pub value: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub debug_name: Option<String>,
    pub qualifiedname: Option<QName>,
    pub needs_init: bool,
    pub gen_bits: Option<BitSet>,
}

impl NamespaceDefinitionNode {
    pub fn base(pkgdef: Option<NodeRef>, attrs: Option<NodeRef>, name: Option<NodeRef>, value: Option<NodeRef>) -> NamespaceDefinitionNode {
        NamespaceDefinitionNode {
            def: DefinitionNode::base(pkgdef, attrs),
            name,
            value,
            ref_: None,
            debug_name: None,
            qualifiedname: None,
            needs_init: false,
            gen_bits: None,
        }
    }

    pub fn new(pkgdef: Option<NodeRef>, attrs: Option<NodeRef>, name: Option<NodeRef>, value: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::NamespaceDefinition(Self::base(pkgdef, attrs, name, value))).into_ref()
    }
}

pub struct PackageDefinitionNode {
    pub def: DefinitionNode,
    pub ref_: Option<ReferenceValue>,
    pub name: Option<NodeRef>,
    pub statements: Option<NodeRef>,
    pub fexprs: Option<NodeList>,
    pub clsdefs: Option<NodeList>,
    pub default_namespace: Option<ObjectValue>,
    pub public_namespace: Option<ObjectValue>,
    pub internal_namespace: Option<ObjectValue>,
    pub used_namespaces: Namespaces,
    pub used_def_namespaces: Namespaces,
    pub imported_names: Multinames,
    pub var_count: i32,
    pub temp_count: i32,
    pub cx: Option<ContextRef>,
    pub package_retrieved: bool,
    pub in_this_pkg: bool,
}

impl PackageDefinitionNode {
    pub fn new(cx: Option<ContextRef>, attrs: Option<NodeRef>, name: Option<NodeRef>, statements: Option<NodeRef>) -> NodeRef {
        let n = PackageDefinitionNode {
            def: DefinitionNode::base(None, attrs),
            ref_: None,
            name,
            statements,
            fexprs: Some(new_node_list()),
            clsdefs: Some(new_node_list()),
            default_namespace: None,
            public_namespace: None,
            internal_namespace: None,
            used_namespaces: Namespaces::new(),
            used_def_namespaces: Namespaces::new(),
            imported_names: Multinames::new(),
            var_count: 0,
            temp_count: 0,
            cx,
            package_retrieved: false,
            in_this_pkg: false,
        };
        Node::new(0, Kind::PackageDefinition(n)).into_ref()
    }
}

pub struct PackageIdentifiersNode {
    pub list: Vec<Option<NodeRef>>,
    pub pkg_part: Option<String>,
    pub def_part: Option<String>,
}

impl PackageIdentifiersNode {
    const IS_DEFINITION_FLAG: i32 = 1;
    const ASTERISK: &'static str = "*";

    pub fn new(item: Option<NodeRef>, pos: i32, is_definition: bool) -> NodeRef {
        let mut node = Node::new(pos, Kind::PackageIdentifiers(PackageIdentifiersNode { list: vec![item], pkg_part: None, def_part: None }));
        if is_definition {
            node.flags |= Self::IS_DEFINITION_FLAG;
        }
        node.into_ref()
    }

    pub fn size(&self) -> i32 {
        self.list.len() as i32
    }

    pub fn clear_identifier_string(&mut self) {
        if self.pkg_part.is_some() {
            self.pkg_part = None;
        }
        if self.def_part.is_some() {
            self.def_part = None;
        }
    }

    /// `toIdentifierString()`; `is_definition` is the node's `isDefinition()`.
    pub fn to_identifier_string(&mut self, is_definition: bool) -> Option<String> {
        if self.pkg_part.is_none() {
            let mut buf = String::new();
            let len = self.list.len();
            for x in 0..len {
                let item = rb(&self.list[x]);
                let item = item.identifier().expect("ClassCastException");
                if x == len - 1 && is_definition {
                    self.def_part = Some(String::new());
                    if Self::ASTERISK != item.name {
                        self.def_part = Some(item.name.clone());
                    }
                } else {
                    if !buf.is_empty() {
                        buf.push('.');
                    }
                    buf.push_str(&item.to_identifier_string());
                }
            }
            self.pkg_part = Some(buf);
        }
        self.pkg_part.clone()
    }
}

pub struct PackageNameNode {
    pub url: Option<NodeRef>,
    pub id: Option<NodeRef>,
}

impl PackageNameNode {
    pub fn new_url(url: Option<NodeRef>, pos: i32) -> NodeRef {
        Node::new(pos, Kind::PackageName(PackageNameNode { url, id: None })).into_ref()
    }

    pub fn new(id: Option<NodeRef>, pos: i32) -> NodeRef {
        Node::new(pos, Kind::PackageName(PackageNameNode { url: None, id })).into_ref()
    }
}

pub struct ParameterListNode {
    pub items: Vec<Option<NodeRef>>,
    pub types: Vec<TypeInfo>,
    pub decl_styles: ByteList,
    pub count: i32,
}

impl ParameterListNode {
    pub fn new(_list: Option<NodeRef>, item: Option<NodeRef>, _pos: i32) -> NodeRef {
        let n = ParameterListNode { items: vec![item], types: Vec::with_capacity(1), decl_styles: ByteList::with_capacity(1), count: -1 };
        Node::new(-1, Kind::ParameterList(n)).into_ref()
    }

    pub fn size(&mut self) -> i32 {
        if self.count < 0 {
            self.count = 0;
            for item in &self.items {
                self.count += rb(item).parameter_size();
            }
        }
        self.count
    }

    pub fn to_canonical_string(&self, cx: &ContextRef, buf: &mut String) {
        {
            buf.push_str(" param_names='");
            let first_item = self.items[0].clone();
            for item in &self.items {
                if !same_node(item, &first_item) {
                    buf.push(';');
                }
                let item = rb(item);
                let param = item.parameter().expect("ClassCastException");
                buf.push_str(&param.ref_.as_ref().map_or_else(String::new, |r| r.name()));
            }
            buf.push('\'');
        }
        {
            buf.push_str(" param_types='");
            let first_item = self.items[0].clone();
            for item in &self.items {
                if !same_node(item, &first_item) {
                    buf.push(';');
                }
                let item = rb(item);
                let param = item.parameter().expect("ClassCastException");
                if item.as_rest_parameter().is_some() {
                    buf.push_str("restParam");
                } else if let Some(typeref) = &param.typeref {
                    buf.push_str(&DocCommentNode::get_ref_name(cx, typeref));
                }
            }
            buf.push('\'');
        }
        {
            buf.push_str(" param_defaults='");
            let first_item = self.items[0].clone();
            for item in &self.items {
                if !same_node(item, &first_item) {
                    buf.push(';');
                }
                let item = rb(item);
                let param = item.parameter().expect("ClassCastException");
                if param.init.is_none() {
                    buf.push_str("undefined");
                } else {
                    let init = rb(&param.init);
                    if let Some(n) = init.as_literal_number() {
                        buf.push_str(&n.value);
                    } else if let Some(n) = init.as_literal_string() {
                        buf.push_str(&DocCommentNode::escape_xml(&n.value));
                    } else if init.as_literal_null().is_some() {
                        buf.push_str("null");
                    } else if let Some(n) = init.as_literal_boolean() {
                        buf.push_str(if n.value { "true" } else { "false" });
                    } else {
                        buf.push_str("unknown");
                    }
                }
            }
            buf.push('\'');
        }
    }
}

pub struct ParameterNode {
    pub kind: i32,
    pub identifier: Option<NodeRef>,
    pub type_: Option<NodeRef>,
    pub init: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub typeref: Option<ReferenceValue>,
    pub attrs: Option<NodeRef>,
    pub no_anno: bool,
}

impl ParameterNode {
    pub fn base(kind: i32, identifier: Option<NodeRef>, type_: Option<NodeRef>, init: Option<NodeRef>) -> ParameterNode {
        ParameterNode { kind, identifier, type_, init, ref_: None, typeref: None, attrs: None, no_anno: false }
    }

    pub fn new(kind: i32, identifier: Option<NodeRef>, type_: Option<NodeRef>, init: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::Parameter(Self::base(kind, identifier, type_, init))).into_ref()
    }
}

pub struct ParenExpressionNode {
    pub expr: Option<NodeRef>,
}

impl ParenExpressionNode {
    pub fn new(expr: Option<NodeRef>, pos: i32) -> NodeRef {
        Node::new(pos, Kind::ParenExpression(ParenExpressionNode { expr })).into_ref()
    }
}

pub struct ParenListExpressionNode {
    pub expr: Option<NodeRef>,
}

impl ParenListExpressionNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::ParenListExpression(ParenListExpressionNode { expr })).into_ref()
    }

    pub fn is_list_expression(&self) -> bool {
        self.expr.as_ref().map_or(false, |e| e.borrow().list().is_some())
    }
}

pub struct PragmaExpressionNode {
    pub identifier: Option<NodeRef>,
    pub arg: Option<NodeRef>,
}

impl PragmaExpressionNode {
    pub fn new(identifier: Option<NodeRef>, arg: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::PragmaExpression(PragmaExpressionNode { identifier, arg })).into_ref()
    }
}

pub struct PragmaNode {
    pub list: Option<NodeRef>,
}

impl PragmaNode {
    pub fn new(list: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::Pragma(PragmaNode { list })).into_ref()
    }
}

pub struct ProgramNode {
    pub statements: Option<NodeRef>,
    pub fexprs: Option<NodeList>,
    pub clsdefs: Option<NodeList>,
    pub pkgdefs: Vec<NodeRef>,
    pub imports: Vec<NodeRef>,
    pub temp_count: i32,
    pub var_count: i32,
    pub blocks: Option<Vec<BlockRef>>,
    pub cx: Option<ContextRef>,
    pub has_unnamed_package: bool,
    pub frame: Option<ObjectValue>,
    pub import_frame: Option<ObjectValue>,
    pub default_namespace: Option<ObjectValue>,
    pub public_namespace: Option<ObjectValue>,
    pub state: i32,
    pub used_def_namespaces: Namespaces,
    pub import_def_unresolved: HashSet<ReferenceValue>,
    pub package_unresolved: HashSet<ReferenceValue>,
    pub ns_unresolved: HashSet<ReferenceValue>,
    pub fa_unresolved: HashSet<ReferenceValue>,
    pub ce_unresolved: HashSet<ReferenceValue>,
    pub body_unresolved: HashSet<ReferenceValue>,
    pub rt_unresolved: HashSet<ReferenceValue>,
}

impl ProgramNode {
    pub const Inheritance: i32 = 1;
    pub const Else: i32 = 2;
    pub const Done: i32 = 3;

    pub fn base(cx: Option<ContextRef>, statements: Option<NodeRef>) -> ProgramNode {
        ProgramNode {
            statements,
            fexprs: None,
            clsdefs: None,
            pkgdefs: Vec::new(),
            imports: Vec::new(),
            temp_count: 0,
            var_count: 0,
            blocks: None,
            cx,
            has_unnamed_package: false,
            frame: None,
            import_frame: None,
            default_namespace: None,
            public_namespace: None,
            state: Self::Inheritance,
            used_def_namespaces: Namespaces::new(),
            import_def_unresolved: HashSet::new(),
            package_unresolved: HashSet::new(),
            ns_unresolved: HashSet::new(),
            fa_unresolved: HashSet::new(),
            ce_unresolved: HashSet::new(),
            body_unresolved: HashSet::new(),
            rt_unresolved: HashSet::new(),
        }
    }

    pub fn new(cx: Option<ContextRef>, statements: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::Program(Self::base(cx, statements))).into_ref()
    }
}

pub struct QualifiedExpressionNode {
    pub qid: QualifiedIdentifierNode,
    pub expr: Option<NodeRef>,
    pub nss: Option<Namespaces>,
}

impl QualifiedExpressionNode {
    pub fn new(qualifier: Option<NodeRef>, expr: Option<NodeRef>, pos: i32) -> NodeRef {
        let qid = QualifiedIdentifierNode::base(qualifier, None);
        let node = Node::new(pos, Kind::QualifiedExpression(QualifiedExpressionNode { qid, expr, nss: None }));
        IdentifierNode::init(node, "")
    }
}

pub struct QualifiedIdentifierNode {
    pub id: IdentifierNode,
    pub qualifier: Option<NodeRef>,
    pub is_config_name: bool,
}

impl QualifiedIdentifierNode {
    pub fn base(qualifier: Option<NodeRef>, name: Option<&str>) -> QualifiedIdentifierNode {
        QualifiedIdentifierNode { id: IdentifierNode::base(name.unwrap_or("")), qualifier, is_config_name: false }
    }

    pub fn new(qualifier: Option<NodeRef>, name: Option<&str>, pos: i32) -> NodeRef {
        let node = Node::new(pos, Kind::QualifiedIdentifier(Self::base(qualifier, name)));
        IdentifierNode::init(node, name.unwrap_or(""))
    }
}

pub struct RegisterNode {
    pub index: i32,
    pub type_: Option<TypeValue>,
    pub void_result: bool,
}

impl RegisterNode {
    pub fn new() -> NodeRef {
        Node::new(-1, Kind::Register(RegisterNode { index: -1, type_: None, void_result: false })).into_ref()
    }
}

pub struct RestExpressionNode {
    pub expr: Option<NodeRef>,
}

impl RestExpressionNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::RestExpression(RestExpressionNode { expr })).into_ref()
    }
}

pub struct RestParameterNode {
    pub param: ParameterNode,
    pub parameter: Option<NodeRef>,
}

impl RestParameterNode {
    pub fn new(parameter: NodeRef) -> NodeRef {
        let param = {
            let p = parameter.borrow();
            let p = p.parameter().expect("ClassCastException");
            ParameterNode::base(p.kind, p.identifier.clone(), p.type_.clone(), p.init.clone())
        };
        Node::new(-1, Kind::RestParameter(RestParameterNode { param, parameter: Some(parameter) })).into_ref()
    }
}

pub struct ReturnStatementNode {
    pub expr: Option<NodeRef>,
    pub finally_inserted: bool,
}

impl ReturnStatementNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::ReturnStatement(ReturnStatementNode { expr, finally_inserted: false })).into_ref()
    }

    pub fn is_finally_inserted(&self) -> bool {
        self.finally_inserted
    }

    pub fn set_finally_inserted(&mut self) {
        self.finally_inserted = true;
    }
}

/// Abstract base `SelectorNode`; its own `flags` are separate from `Node.flags`.
pub struct SelectorNode {
    pub base: Option<ObjectValue>,
    flags: i32,
    pub ref_: Option<ReferenceValue>,
    pub expr: Option<NodeRef>,
    pub is_package: bool,
    skip: bool,
}

impl SelectorNode {
    const RVALUE_Flag: i32 = 1;
    const ATTR_Flag: i32 = 2;
    const SUPER_Flag: i32 = 4;
    const VOID_RESULT_Flag: i32 = 8;
    const THIS_Flag: i32 = 16;
    const MODE_Shift: i32 = 16;
    const MODE_Mask: i32 = 0xFFFF0000u32 as i32;

    pub fn new() -> SelectorNode {
        let mut sel = SelectorNode { base: None, flags: 0, ref_: None, expr: None, is_package: false, skip: false };
        sel.set_mode(DOT_TOKEN);
        sel
    }

    pub fn set_base(&mut self, base: Option<ObjectValue>) {
        self.base = base;
    }

    pub fn set_rvalue(&mut self, is_rvalue: bool) {
        self.flags = if is_rvalue { self.flags | Self::RVALUE_Flag } else { self.flags & !Self::RVALUE_Flag };
    }

    pub fn is_rvalue(&self) -> bool {
        (self.flags & Self::RVALUE_Flag) != 0
    }

    pub fn set_attr(&mut self, is_attr: bool) {
        self.flags = if is_attr { self.flags | Self::ATTR_Flag } else { self.flags & !Self::ATTR_Flag };
    }

    pub fn is_attr(&self) -> bool {
        (self.flags & Self::ATTR_Flag) != 0
    }

    pub fn set_mode(&mut self, mode: i32) {
        self.flags &= !Self::MODE_Mask;
        self.flags |= mode << Self::MODE_Shift;
    }

    pub fn get_mode(&self) -> i32 {
        self.flags >> Self::MODE_Shift
    }

    pub fn set_super(&mut self, is_super: bool) {
        self.flags = if is_super { self.flags | Self::SUPER_Flag } else { self.flags & !Self::SUPER_Flag };
    }

    pub fn is_super(&self) -> bool {
        (self.flags & Self::SUPER_Flag) != 0
    }

    pub fn set_this(&mut self, is_this: bool) {
        self.flags = if is_this { self.flags | Self::THIS_Flag } else { self.flags & !Self::THIS_Flag };
    }

    pub fn is_this(&self) -> bool {
        (self.flags & Self::THIS_Flag) != 0
    }

    /// `SelectorNode.voidResult()`, which only the subclasses that do not override it reach.
    pub fn void_result(&mut self) {
        self.flags |= Self::VOID_RESULT_Flag;
    }

    pub fn is_void_result(&self) -> bool {
        (self.flags & Self::VOID_RESULT_Flag) != 0
    }

    pub fn get_identifier(&self) -> Option<NodeRef> {
        match &self.expr {
            Some(e) if e.borrow().identifier().is_some() => Some(e.clone()),
            _ => None,
        }
    }

    pub fn skip_node(&mut self, b: bool) {
        self.skip = b;
    }

    pub fn skip(&self) -> bool {
        self.skip
    }
}

pub struct SetExpressionNode {
    pub sel: SelectorNode,
    pub args: Option<NodeRef>,
    pub value_type: Option<TypeInfo>,
    pub is_constinit: bool,
    pub is_initializer: bool,
    pub gen_bits: Option<BitSet>,
    pub void_result: bool,
}

impl SetExpressionNode {
    pub fn new(expr: Option<NodeRef>, args: Option<NodeRef>) -> NodeRef {
        let mut sel = SelectorNode::new();
        sel.expr = expr;
        sel.ref_ = None;
        let n = SetExpressionNode {
            sel,
            args,
            value_type: None,
            is_constinit: false,
            is_initializer: false,
            gen_bits: None,
            void_result: false,
        };
        Node::new(-1, Kind::SetExpression(n)).into_ref()
    }
}

pub struct StatementListNode {
    pub items: Vec<Option<NodeRef>>,
    pub dominates_program_endpoint: bool,
    pub was_empty: bool,
    pub is_loop: bool,
    pub is_block: bool,
    pub has_pragma: bool,
    pub number_usage: Option<NumberUsage>,
    pub default_namespace: Option<ObjectValue>,
    pub config_attrs: Option<NodeRef>,
}

impl StatementListNode {
    pub fn new(item: Option<NodeRef>) -> NodeRef {
        let mut n = StatementListNode {
            items: Vec::with_capacity(5),
            dominates_program_endpoint: false,
            was_empty: false,
            is_loop: false,
            is_block: false,
            has_pragma: false,
            number_usage: None,
            default_namespace: None,
            config_attrs: None,
        };
        if item.is_some() {
            n.items.push(item);
        }
        Node::new(-1, Kind::StatementList(n)).into_ref()
    }

    pub fn defines_cv(&self) -> bool {
        for n in &self.items {
            let n = rb(n);
            if n.is_expression_statement() {
                let expr = n.as_expression_statement().unwrap();
                if !expr.is_var_statement() {
                    return true;
                }
            }
        }
        false
    }
}

pub struct StoreRegisterNode {
    pub reg: Option<NodeRef>,
    pub expr: Option<NodeRef>,
    pub type_: Option<TypeValue>,
    pub void_result: bool,
}

impl StoreRegisterNode {
    pub fn new(reg: Option<NodeRef>, expr: Option<NodeRef>, type_: Option<TypeValue>) -> NodeRef {
        Node::new(-1, Kind::StoreRegister(StoreRegisterNode { reg, expr, type_, void_result: false })).into_ref()
    }
}

pub struct SuperExpressionNode {
    pub expr: Option<NodeRef>,
}

impl SuperExpressionNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::SuperExpression(SuperExpressionNode { expr })).into_ref()
    }
}

pub struct SuperStatementNode {
    pub call: Option<NodeRef>,
    pub baseobj: Option<ObjectValue>,
}

impl SuperStatementNode {
    pub fn new(call: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::SuperStatement(SuperStatementNode { call, baseobj: None })).into_ref()
    }
}

pub struct SwitchStatementNode {
    pub expr: Option<NodeRef>,
    pub statements: Option<NodeRef>,
    pub loop_index: i32,
}

impl SwitchStatementNode {
    pub fn new(expr: Option<NodeRef>, statements: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::SwitchStatement(SwitchStatementNode { expr, statements, loop_index: 0 })).into_ref()
    }
}

pub struct ThisExpressionNode;

impl ThisExpressionNode {
    pub fn new() -> NodeRef {
        Node::new(-1, Kind::ThisExpression(ThisExpressionNode)).into_ref()
    }
}

pub struct ThrowStatementNode {
    pub expr: Option<NodeRef>,
    pub finally_inserted: bool,
}

impl ThrowStatementNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::ThrowStatement(ThrowStatementNode { expr, finally_inserted: false })).into_ref()
    }

    pub fn is_finally_inserted(&self) -> bool {
        self.finally_inserted
    }

    pub fn set_finally_inserted(&mut self) {
        self.finally_inserted = true;
    }
}

pub struct ToObjectNode {
    pub expr: Option<NodeRef>,
}

impl ToObjectNode {
    pub fn new(expr: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::ToObject(ToObjectNode { expr })).into_ref()
    }
}

pub struct TryStatementNode {
    pub tryblock: Option<NodeRef>,
    pub catchlist: Option<NodeRef>,
    pub finallyblock: Option<NodeRef>,
    pub finally_inserted: bool,
    pub loop_index: i32,
    skip: bool,
}

impl TryStatementNode {
    pub fn new(tryblock: Option<NodeRef>, catchlist: Option<NodeRef>, finallyblock: Option<NodeRef>) -> NodeRef {
        let n = TryStatementNode { tryblock, catchlist, finallyblock, finally_inserted: false, loop_index: 0, skip: false };
        Node::new(-1, Kind::TryStatement(n)).into_ref()
    }

    pub fn skip_node(&mut self, b: bool) {
        self.skip = b;
    }

    pub fn skip(&self) -> bool {
        self.skip
    }
}

pub struct TypeExpressionNode {
    pub nullable_annotation: bool,
    pub is_nullable: bool,
    pub expr: Option<NodeRef>,
}

impl TypeExpressionNode {
    pub fn new(expr: Option<NodeRef>, is_nullable: bool, is_explicit: bool) -> NodeRef {
        let n = TypeExpressionNode { nullable_annotation: is_explicit, is_nullable, expr };
        Node::new(-1, Kind::TypeExpression(n)).into_ref()
    }
}

pub struct TypeIdentifierNode {
    pub id: IdentifierNode,
    pub base: Option<NodeRef>,
    pub type_args: Option<NodeRef>,
}

impl TypeIdentifierNode {
    pub fn new(base: Option<NodeRef>, type_args: Option<NodeRef>, pos: i32) -> NodeRef {
        let n = TypeIdentifierNode { id: IdentifierNode::base(""), base, type_args };
        IdentifierNode::init(Node::new(pos, Kind::TypeIdentifier(n)), "")
    }
}

pub struct TypedIdentifierNode {
    pub identifier: Option<NodeRef>,
    pub type_: Option<NodeRef>,
    pub no_anno: bool,
}

impl TypedIdentifierNode {
    pub fn new(identifier: Option<NodeRef>, type_: Option<NodeRef>, pos: i32) -> NodeRef {
        if let Some(i) = &identifier {
            i.borrow().identifier().expect("ClassCastException");
        }
        Node::new(pos, Kind::TypedIdentifier(TypedIdentifierNode { identifier, type_, no_anno: false })).into_ref()
    }
}

pub struct UnaryExpressionNode {
    pub expr: Option<NodeRef>,
    pub op: i32,
    pub ref_: Option<ReferenceValue>,
    pub slot: Option<Slot>,
    pub number_usage: Option<NumberUsage>,
    pub void_result: bool,
}

impl UnaryExpressionNode {
    pub fn new(op: i32, expr: Option<NodeRef>) -> NodeRef {
        let n = UnaryExpressionNode { expr, op, ref_: None, slot: None, number_usage: None, void_result: false };
        Node::new(-1, Kind::UnaryExpression(n)).into_ref()
    }

    /// Misspelled in Java, so it never overrides `isBooleanExpression()`.
    pub fn isbooleanean_expression(&self) -> bool {
        self.op == NOT_TOKEN
    }
}

pub struct UntypedVariableBindingNode {
    pub identifier: Option<NodeRef>,
    pub initializer: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
}

impl UntypedVariableBindingNode {
    pub fn new(identifier: Option<NodeRef>, initializer: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::UntypedVariableBinding(UntypedVariableBindingNode { identifier, initializer, ref_: None })).into_ref()
    }
}

pub struct UseDirectiveNode {
    pub def: DefinitionNode,
    pub expr: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
}

impl UseDirectiveNode {
    pub fn new(pkgdef: Option<NodeRef>, attrs: Option<NodeRef>, expr: Option<NodeRef>) -> NodeRef {
        let n = UseDirectiveNode { def: DefinitionNode::base(pkgdef, attrs), expr, ref_: None };
        Node::new(-1, Kind::UseDirective(n)).into_ref()
    }
}

pub struct UseNumericNode {
    pub pragma: UsePragmaNode,
    pub numeric_mode: i32,
}

impl UseNumericNode {
    const numeric_mode_names: [&'static str; 5] = ["Number", "decimal", "double", "int", "uint"];

    pub fn new(id_node: Option<NodeRef>, argument: Option<NodeRef>, n_mode: i32) -> NodeRef {
        let pragma = UsePragmaNode::base(id_node, argument);
        Node::new(-1, Kind::UseNumeric(UseNumericNode { pragma, numeric_mode: n_mode })).into_ref()
    }
}

pub struct UsePragmaNode {
    pub identifier: Option<NodeRef>,
    pub argument: Option<NodeRef>,
}

impl UsePragmaNode {
    pub fn base(id_node: Option<NodeRef>, arg_node: Option<NodeRef>) -> UsePragmaNode {
        UsePragmaNode { identifier: id_node, argument: arg_node }
    }

    pub fn new(id_node: Option<NodeRef>, arg_node: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::UsePragma(Self::base(id_node, arg_node))).into_ref()
    }
}

pub struct UsePrecisionNode {
    pub pragma: UsePragmaNode,
    pub precision: i32,
}

impl UsePrecisionNode {
    pub fn new(id_node: Option<NodeRef>, precision_node: Option<NodeRef>) -> NodeRef {
        let mut precision = 34;
        if let Some(p) = &precision_node {
            if let Some(n) = p.borrow().as_literal_number() {
                precision = n.int_value();
            }
        }
        let pragma = UsePragmaNode::base(id_node, precision_node);
        Node::new(-1, Kind::UsePrecision(UsePrecisionNode { pragma, precision })).into_ref()
    }
}

pub struct UseRoundingNode {
    pub pragma: UsePragmaNode,
    pub mode: i32,
}

impl UseRoundingNode {
    pub fn new(id: Option<NodeRef>, argument: Option<NodeRef>) -> NodeRef {
        let mut mode = NumberUsage::round_HALF_EVEN;
        if let Some(a) = &argument {
            let is_identifier = a.borrow().identifier().is_some();
            if is_identifier {
                let arg = a.borrow_mut().to_identifier_string().unwrap_or_default();
                if arg == "HALF_EVEN" {
                    mode = NumberUsage::round_HALF_EVEN;
                } else if arg == "DOWN" {
                    mode = NumberUsage::round_DOWN;
                } else if arg == "FLOOR" {
                    mode = NumberUsage::round_FLOOR;
                } else if arg == "UP" {
                    mode = NumberUsage::round_UP;
                } else if arg == "CEILING" {
                    mode = NumberUsage::round_CEILING;
                } else if arg == "HALF_UP" {
                    mode = NumberUsage::round_HALF_UP;
                } else if arg == "HALF_DOWN" {
                    mode = NumberUsage::round_HALF_DOWN;
                }
            }
        }
        let pragma = UsePragmaNode::base(id, argument);
        Node::new(-1, Kind::UseRounding(UseRoundingNode { pragma, mode })).into_ref()
    }
}

pub struct VariableBindingNode {
    pub variable: Option<NodeRef>,
    pub initializer: Option<NodeRef>,
    pub ref_: Option<ReferenceValue>,
    pub typeref: Option<ReferenceValue>,
    pub attrs: Option<NodeRef>,
    pub debug_name: Option<String>,
    pub kind: i32,
}

impl VariableBindingNode {
    pub const PACKAGE_FLAG: i32 = 1;

    pub fn new(
        pkgdef: Option<NodeRef>,
        attrs: Option<NodeRef>,
        kind: i32,
        variable: Option<NodeRef>,
        initializer: Option<NodeRef>,
    ) -> NodeRef {
        let n = VariableBindingNode { variable, initializer, ref_: None, typeref: None, attrs, debug_name: None, kind };
        let mut node = Node::new(-1, Kind::VariableBinding(n));
        if pkgdef.is_some() {
            node.flags |= Self::PACKAGE_FLAG;
        }
        node.into_ref()
    }
}

pub struct VariableDefinitionNode {
    pub def: DefinitionNode,
    pub kind: i32,
    pub list: Option<NodeRef>,
    pub cx: Option<ContextRef>,
}

impl VariableDefinitionNode {
    pub fn new(pkgdef: Option<NodeRef>, attrs: Option<NodeRef>, kind: i32, list: Option<NodeRef>, pos: i32) -> NodeRef {
        let n = VariableDefinitionNode { def: DefinitionNode::base(pkgdef, attrs), kind, list, cx: None };
        Node::new(pos, Kind::VariableDefinition(n)).into_ref()
    }

    fn initializer_statement(this: &NodeRef, cx: &ContextRef) -> NodeRef {
        let node_factory = Context::get_node_factory(cx);
        let mut node_factory = node_factory.borrow_mut();
        let (is_const, items) = {
            let node = this.borrow();
            let list = rb(&node.as_variable_definition().unwrap().list);
            (node.is_const(), list.list().expect("ClassCastException").items.clone())
        };
        let mut init_list: Option<NodeRef> = None;

        if !is_const {
            for n in &items {
                let binding_ref = n.clone().expect("NullPointerException");
                let (attrs_null, variable, initializer, kind) = {
                    let binding = binding_ref.borrow();
                    let binding = binding.as_variable_binding().expect("ClassCastException");
                    (binding.attrs.is_none(), binding.variable.clone(), binding.initializer.clone(), binding.kind)
                };
                let type_null = rb(&variable).as_typed_identifier().unwrap().type_.is_none();
                if attrs_null && type_null {
                    let (name, pos) = {
                        let var = rb(&variable);
                        let id = rb(&var.as_typed_identifier().unwrap().identifier);
                        (id.identifier().expect("ClassCastException").name.clone(), id.pos())
                    };
                    let aln = None;
                    let qid = node_factory.qualified_identifier_pos(aln, Some(&name), pos);
                    rbm(&variable).as_typed_identifier_mut().unwrap().identifier = Some(qid);
                }
                if initializer.is_some() {
                    let identifier = rb(&variable).as_typed_identifier().unwrap().identifier.clone();
                    let assign_node = node_factory.assignment_expression(
                        identifier,
                        if kind == CONST_TOKEN { CONST_TOKEN } else { ASSIGN_TOKEN },
                        initializer,
                    );
                    if let Some(m) = assign_node.borrow().as_member_expression() {
                        if let Some(s) = rbm(&m.selector).as_set_expression_mut() {
                            s.is_initializer = true;
                        }
                    }
                    init_list = Some(node_factory.list(init_list, Some(assign_node)));
                }
            }
        }

        if init_list.is_some() {
            let init = node_factory.expression_statement(init_list);
            init.borrow_mut().as_expression_statement_mut().unwrap().set_is_var_statement(true);
            init
        } else {
            node_factory.empty_statement()
        }
    }

    pub fn set_context(&mut self, cx: Option<ContextRef>) {
        self.cx = cx;
    }

    pub fn get_context(&self) -> Option<ContextRef> {
        self.cx.clone()
    }
}

pub struct WhileStatementNode {
    pub expr: Option<NodeRef>,
    pub statement: Option<NodeRef>,
    pub loop_index: i32,
}

impl WhileStatementNode {
    pub fn new(expr: Option<NodeRef>, statement: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::WhileStatement(WhileStatementNode { expr, statement, loop_index: 0 })).into_ref()
    }
}

pub struct WithStatementNode {
    pub expr: Option<NodeRef>,
    pub statement: Option<NodeRef>,
    pub activation: Option<ObjectValue>,
}

impl WithStatementNode {
    pub fn new(expr: Option<NodeRef>, statement: Option<NodeRef>) -> NodeRef {
        Node::new(-1, Kind::WithStatement(WithStatementNode { expr, statement, activation: None })).into_ref()
    }
}
