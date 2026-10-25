//! Code for names, properties, index and namespaced access, calls and construction.

use std::rc::Rc;

use super::ast::*;
use super::gen::{err, get_loc, resolved_of, set_loc, spec_node, Cx, Ld, M};
use super::index::mn_type;
use super::ins::{code_len, ins, op, Code, I};
use super::pool;
use super::resolve::Obj;
use super::R;
use crate::abc::{Multiname, TraitData};

/// `hasSideEffect`, looking into sub-items.
pub fn has_side_effect(n: &Node) -> bool {
    match &*n.borrow() {
        Item::Call(_) | Item::Construct(_) | Item::ConstructSuper { .. } | Item::PreInc { .. } | Item::PreDec { .. } | Item::PostInc { .. } | Item::PostDec { .. } => true,
        Item::NewArray(_) | Item::NewObject(_) | Item::ReturnValue(_) | Item::ReturnVoid | Item::Throw(_) | Item::Dup(_) | Item::Pop | Item::Swap => true,
        Item::Bin { l, r, .. } | Item::And { l, r } | Item::Or { l, r } => has_side_effect(l) || has_side_effect(r),
        Item::Un { v, .. } | Item::Paren(v) | Item::Coerce { v, .. } => has_side_effect(v),
        Item::Property(p) => p.object.as_ref().is_some_and(has_side_effect) || p.assigned.is_some(),
        Item::Index(x) => has_side_effect(&x.object) || has_side_effect(&x.index) || x.assigned.is_some(),
        Item::Name(x) => x.assigned.is_some(),
        Item::Unresolved(u) => u.resolved.as_ref().is_some_and(has_side_effect) || u.assigned.is_some(),
        Item::Ternar { cond, t, f } => has_side_effect(cond) || has_side_effect(t) || has_side_effect(f),
        Item::Comma(v) => v.iter().any(has_side_effect),
        _ => false,
    }
}

fn change_ins(dec: bool, is_int: bool, nc: Option<i64>) -> I {
    if is_int {
        ins(if dec { op::DECREMENT_I } else { op::INCREMENT_I }, &[])
    } else if let Some(c) = nc {
        ins(if dec { op::DECREMENT_P } else { op::INCREMENT_P }, &[c])
    } else {
        ins(if dec { op::DECREMENT } else { op::INCREMENT }, &[])
    }
}

impl Cx {
    fn obj_code(&mut self, ld: &mut Ld, o: Obj) -> R<Code> {
        match o {
            Obj::Item(n) => self.src(ld, &n),
            Obj::Ins(i) => Ok(vec![i]),
        }
    }

    pub fn name_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool) -> R<Code> {
        let nm = match &*n.borrow() {
            Item::Name(x) => x.clone(),
            _ => unreachable!(),
        };
        if needs_ret {
            if let Some(r) = &nm.redirect {
                return self.src(ld, r);
            }
        }
        if nm.reg == -1 && nm.slot == -1 && nm.ns.is_none() {
            return err(nm.line, format!("No register or slot set for {}", nm.var));
        }
        if nm.definition && nm.assigned.is_none() {
            return Ok(Vec::new());
        }
        let ty = nm.ty.clone().unwrap_or_else(unbounded);
        if let Some(av) = &nm.assigned {
            if nm.slot > -1 {
                let at = self.ret_type(av)?;
                let tt = text(&ty);
                let skip = text(&at) == tt && (tt == "int" || tt == "Number");
                let coerce = if skip { None } else { Some(self.generate_coerce(Some(ld), &ty)?) };
                let mut m = M::new();
                m.op(op::GET_SCOPE_OBJECT, &[nm.slot_scope]).c(self.src(ld, av)?).opt(coerce);
                let mut rt = -1;
                if needs_ret {
                    m.c(self.dup_set_temp(ld, &mut rt));
                }
                m.opt(set_loc(nm.reg)).op(op::SET_SLOT, &[nm.slot]);
                if needs_ret {
                    m.c(Cx::get_temp(rt));
                }
                m.c(self.kill_temp(ld, &[rt]));
                return Ok(m.0);
            }
            let coerce = self.generate_coerce(Some(ld), &ty)?;
            let mut m = M::new();
            m.c(self.src(ld, av)?).i(coerce);
            if needs_ret {
                m.op(op::DUP, &[]);
            }
            m.opt(set_loc(nm.reg));
            return Ok(m.0);
        }
        let mut m = M::new();
        m.opt(get_loc(nm.reg));
        if nm.slot != -1 {
            m.op(op::GET_SCOPE_OBJECT, &[nm.slot_scope]).op(op::GET_SLOT, &[nm.slot]);
        }
        if !needs_ret {
            m.op(op::POP, &[]);
        }
        Ok(m.0)
    }

    /// `toSourceChange`: `++`/`--` on an assignable item.
    pub fn to_source_change(&mut self, ld: &mut Ld, n: &Node, post: bool, dec: bool, needs_ret: bool) -> R<Code> {
        let it = n.borrow().clone();
        match it {
            Item::Unresolved(u) => match u.resolved {
                None => err(u.line, format!("Undefined variable or property: {}", u.name.raw())),
                Some(r) if is_assignable(&r) => self.to_source_change(ld, &r, post, dec, needs_ret),
                Some(_) => err(u.line, "Cannot assign"),
            },
            Item::Name(nm) => {
                if let Some(r) = &nm.redirect {
                    return self.to_source_change(ld, r, post, dec, needs_ret);
                }
                let ty = nm.ty.clone().unwrap_or_else(unbounded);
                let is_int = text(&ty) == "int";
                if !needs_ret {
                    if nm.slot > -1 {
                        let ci = change_ins(dec, is_int, ld.number_context);
                        return Ok(vec![ins(op::GET_SCOPE_OBJECT, &[nm.slot_scope]), ins(op::GET_SCOPE_OBJECT, &[nm.slot_scope]), ins(op::GET_SLOT, &[nm.slot]), ci, ins(op::SET_SLOT, &[nm.slot])]);
                    }
                    let ci = if is_int {
                        ins(if dec { op::DEC_LOCAL_I } else { op::INC_LOCAL_I }, &[nm.reg])
                    } else if let Some(c) = ld.number_context {
                        ins(if dec { op::DEC_LOCAL_P } else { op::INC_LOCAL_P }, &[c, nm.reg])
                    } else {
                        ins(if dec { op::DEC_LOCAL } else { op::INC_LOCAL }, &[nm.reg])
                    };
                    return Ok(vec![ci]);
                }
                let step = || ins(if dec { if is_int { op::DECREMENT_I } else { op::DECREMENT } } else if is_int { op::INCREMENT_I } else { op::INCREMENT }, &[]);
                let coerce = self.generate_coerce(Some(ld), &ty)?;
                let mut m = M::new();
                m.opt(get_loc(nm.reg));
                if nm.slot != -1 {
                    m.op(op::GET_SCOPE_OBJECT, &[nm.slot_scope]).op(op::GET_SLOT, &[nm.slot]);
                }
                if !post {
                    m.i(step());
                }
                m.op(op::DUP, &[]);
                if post {
                    m.i(step());
                }
                m.i(coerce).opt(set_loc(nm.reg));
                if nm.slot > -1 {
                    m.op(op::GET_SCOPE_OBJECT, &[nm.slot_scope]).op(op::SWAP, &[]).op(op::SET_SLOT, &[nm.slot]);
                }
                Ok(m.0)
            }
            Item::Property(_) => {
                let r = self.property_resolve(n, false, ld)?;
                let pid = r.prop_index as i64;
                let obj = self.resolve_object(n, ld, false)?;
                let is_int = r.prop_type.as_ref().is_some_and(|t| text(t) == "int");
                let ci = change_ins(dec, is_int, ld.number_context);
                let mut m = M::new();
                m.c(self.obj_code(ld, obj)?);
                let mut ot = -1;
                let mut rt = -1;
                m.c(self.dup_set_temp(ld, &mut ot)).op(op::GET_PROPERTY, &[pid]);
                if !is_int && post {
                    m.op(op::CONVERT_D, &[]);
                }
                if !post {
                    m.i(ci.clone());
                }
                if needs_ret {
                    m.op(op::DUP, &[]);
                }
                if post {
                    m.i(ci);
                }
                m.c(self.set_temp(ld, &mut rt)).c(Cx::get_temp(ot)).c(Cx::get_temp(rt)).op(op::SET_PROPERTY, &[pid]);
                m.c(self.kill_temp(ld, &[rt, ot]));
                Ok(m.0)
            }
            Item::Index(x) => {
                let ip = self.multiname_l(x.attr, &x.opened)? as i64;
                let ci = match ld.number_context {
                    Some(c) => ins(if dec { op::DECREMENT_P } else { op::INCREMENT_P }, &[c]),
                    None => ins(if dec { op::DECREMENT } else { op::INCREMENT }, &[]),
                };
                let (mut ot, mut it_, mut vt) = (-1, -1, -1);
                let mut m = M::new();
                m.c(self.src(ld, &x.object)?).c(self.dup_set_temp(ld, &mut ot));
                m.c(self.src(ld, &x.index)?).c(self.dup_set_temp(ld, &mut it_));
                m.op(op::GET_PROPERTY, &[ip]);
                if post {
                    m.op(op::CONVERT_D, &[]);
                }
                if !post {
                    m.i(ci.clone());
                }
                if needs_ret {
                    m.op(op::DUP, &[]);
                }
                if post {
                    m.i(ci);
                }
                m.c(self.set_temp(ld, &mut vt)).c(Cx::get_temp(ot)).c(Cx::get_temp(it_)).c(Cx::get_temp(vt)).op(op::SET_PROPERTY, &[ip]);
                m.c(self.kill_temp(ld, &[vt, ot, it_]));
                Ok(m.0)
            }
            Item::Namespaced(x) => {
                let Some(name) = &x.name else { return Ok(Vec::new()) };
                let is_int = false;
                let ci = change_ins(dec, is_int, ld.number_context);
                let ns_ty = ty_parts(&["Namespace"]);
                let c1 = self.generate_coerce(Some(ld), &ns_ty)?;
                let s = self.str(name);
                let rtq = pool::mn(&mut self.abc, Multiname::RtqName { attr: false, name: s }) as i64;
                let c2 = self.generate_coerce(Some(ld), &ns_ty)?;
                let ml = self.multiname_l(false, &x.opened)? as i64;
                let (mut nt, mut st, mut rt) = (-1, -1, -1);
                let mut m = M::new();
                m.c(self.src(ld, &x.ns)?).i(c1).op(op::FIND_PROPERTY_STRICT, &[rtq]).c(self.dup_set_temp(ld, &mut nt));
                m.c(self.src(ld, &x.ns)?).i(c2).c(self.dup_set_temp(ld, &mut st)).op(op::GET_PROPERTY, &[ml]);
                if !is_int {
                    m.op(op::CONVERT_D, &[]);
                }
                if !post {
                    m.i(ci.clone());
                }
                if needs_ret {
                    m.op(op::DUP, &[]);
                }
                if post {
                    m.i(ci);
                }
                m.c(self.set_temp(ld, &mut rt)).c(Cx::get_temp(nt)).c(Cx::get_temp(st)).c(Cx::get_temp(rt)).op(op::SET_PROPERTY, &[ml]);
                m.c(self.kill_temp(ld, &[rt, nt, st]));
                Ok(m.0)
            }
            Item::ScriptProperty { ty, .. } => {
                let pid = self.type_name(Some(ld), &ty)? as i64;
                let ci = change_ins(dec, false, ld.number_context);
                let (mut ot, mut rt) = (-1, -1);
                let mut m = M::new();
                m.c(self.src(ld, &node(Item::FindProperty(ty.clone())))?).c(self.dup_set_temp(ld, &mut ot)).op(op::GET_PROPERTY, &[pid]);
                if post {
                    m.op(op::CONVERT_D, &[]);
                }
                if !post {
                    m.i(ci.clone());
                }
                if needs_ret {
                    m.op(op::DUP, &[]);
                }
                if post {
                    m.i(ci);
                }
                m.c(self.set_temp(ld, &mut rt)).c(Cx::get_temp(ot)).c(Cx::get_temp(rt)).op(op::SET_PROPERTY, &[pid]);
                m.c(self.kill_temp(ld, &[rt, ot]));
                Ok(m.0)
            }
            _ => Ok(Vec::new()),
        }
    }

    pub fn property_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool) -> R<Code> {
        let r = self.property_resolve(n, false, ld)?;
        let pid = r.prop_index as i64;
        let (assigned, nullish) = match &*n.borrow() {
            Item::Property(p) => (p.assigned.clone(), p.nullish),
            _ => unreachable!(),
        };
        let obj = self.resolve_object(n, ld, assigned.is_none())?;
        let is_super = matches!(&obj, Obj::Item(o) if matches!(&*o.borrow(), Item::Name(x) if x.var == "super"));
        if let Some(av) = assigned {
            let pt = r.prop_type.clone().unwrap_or_else(unbounded);
            handle_and_or_coerce(&av, &pt);
            self.ret_type(&av)?;
            let mut m = M::new();
            if is_super {
                // `this` is the receiver of setsuper, as mxmlc pushes it.
                m.op(op::GET_LOCAL0, &[]);
            } else {
                m.c(self.obj_code(ld, obj)?);
            }
            m.c(self.src(ld, &av)?);
            let mut rt = -1;
            if needs_ret {
                m.c(self.dup_set_temp(ld, &mut rt));
            }
            m.op(if is_super { op::SET_SUPER } else { op::SET_PROPERTY }, &[pid]);
            if needs_ret {
                m.c(Cx::get_temp(rt));
            }
            m.c(self.kill_temp(ld, &[rt]));
            return Ok(m.0);
        }
        if let Obj::Ins(i) = &obj {
            if i.borrow().code == op::FIND_PROPERTY_STRICT {
                let mut c = vec![ins(op::GET_LEX, &[pid])];
                if !needs_ret {
                    c.push(ins(op::POP, &[]));
                }
                return Ok(c);
            }
        }
        let obj_item = match &obj {
            Obj::Item(o) => Some(o.clone()),
            Obj::Ins(_) => None,
        };
        let mut on_false = M::new();
        if is_super {
            on_false.op(op::FIND_PROPERTY_STRICT, &[pid]);
        } else {
            on_false.c(self.obj_code(ld, obj)?);
        }
        on_false.op(if is_super { op::GET_SUPER } else { op::GET_PROPERTY }, &[pid]);
        if !needs_ret {
            on_false.op(op::POP, &[]);
        }
        let on_false = on_false.0;
        if !nullish {
            return Ok(on_false);
        }
        let if_false = ins(op::IF_FALSE, &[0]);
        let mut result = match obj_item {
            Some(o) => self.src(ld, &o)?,
            None => Vec::new(),
        };
        result.push(ins(op::PUSH_NULL, &[]));
        result.push(ins(op::EQUALS, &[]));
        result.push(if_false.clone());
        let jump = ins(op::JUMP, &[0]);
        let on_true = vec![ins(op::PUSH_NULL, &[]), jump.clone()];
        if_false.borrow_mut().ops[0] = code_len(&on_true);
        jump.borrow_mut().ops[0] = code_len(&on_false);
        result.extend(on_true);
        result.extend(on_false);
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn index_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool, call: bool, callargs: &[Node], delete: bool, construct: bool) -> R<Code> {
        let x = match &*n.borrow() {
            Item::Index(x) => x.clone(),
            _ => unreachable!(),
        };
        let ip = self.multiname_l(x.attr, &x.opened)? as i64;
        let mut rt = -1;
        if let Some(av) = &x.assigned {
            let comp = match &*av.borrow() {
                Item::Bin { op: o, l, r } if o.compoundable() => Some((l.clone(), r.clone())),
                _ => None,
            };
            if let Some((left, right)) = comp {
                let same = match &*left.borrow() {
                    Item::Index(li) => li.assigned.is_none() && Rc::ptr_eq(&li.object, &x.object) && Rc::ptr_eq(&li.index, &x.index),
                    _ => false,
                };
                if same && has_side_effect(&x.index) {
                    let opi = self.compound_op(ld, av).unwrap();
                    let (mut it_, mut vt) = (-1, -1);
                    let mut m = M::new();
                    m.c(self.src(ld, &x.index)?).c(self.set_temp(ld, &mut it_));
                    m.c(self.src(ld, &x.object)?).c(Cx::get_temp(it_)).op(op::GET_PROPERTY, &[ip]);
                    m.c(self.src(ld, &right)?).i(opi).c(self.set_temp(ld, &mut vt));
                    m.c(self.src(ld, &x.object)?).c(Cx::get_temp(it_)).c(Cx::get_temp(vt));
                    if needs_ret {
                        m.c(self.dup_set_temp(ld, &mut rt));
                    }
                    m.op(op::SET_PROPERTY, &[ip]);
                    if needs_ret {
                        m.c(Cx::get_temp(rt));
                    }
                    m.c(self.kill_temp(ld, &[it_, vt, rt]));
                    return Ok(m.0);
                }
            }
            let mut m = M::new();
            m.c(self.src(ld, &x.object)?).c(self.src(ld, &x.index)?).c(self.src(ld, av)?);
            if needs_ret {
                m.c(self.dup_set_temp(ld, &mut rt));
            }
            m.op(op::SET_PROPERTY, &[ip]);
            if needs_ret {
                m.c(Cx::get_temp(rt));
            }
            m.c(self.kill_temp(ld, &[rt]));
            return Ok(m.0);
        }
        let mut m = M::new();
        m.c(self.src(ld, &x.object)?);
        if call {
            m.c(self.dup_set_temp(ld, &mut rt));
        }
        m.c(self.src(ld, &x.index)?);
        if construct {
            m.c(Cx::get_temp(rt));
            for a in callargs {
                m.c(self.src(ld, a)?);
            }
        }
        let code = if construct {
            op::CONSTRUCT_PROP
        } else if delete {
            op::DELETE_PROPERTY
        } else {
            op::GET_PROPERTY
        };
        if construct {
            m.op(code, &[ip, callargs.len() as i64]);
        } else {
            m.op(code, &[ip]);
        }
        if call {
            m.c(Cx::get_temp(rt));
            for a in callargs {
                m.c(self.src(ld, a)?);
            }
            m.op(op::CALL, &[callargs.len() as i64]);
        }
        if !needs_ret {
            m.op(op::POP, &[]);
        }
        if call || construct {
            m.c(self.kill_temp(ld, &[rt]));
        }
        Ok(m.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn namespaced_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool, call: bool, callargs: &[Node], delete: bool, construct: bool) -> R<Code> {
        let x = match &*n.borrow() {
            Item::Namespaced(x) => x.clone(),
            _ => unreachable!(),
        };
        let ns_ty = ty_parts(&["Namespace"]);
        let (mut rt, mut ot) = (-1, -1);
        let mut m = M::new();
        let get_code = if construct {
            op::CONSTRUCT_PROP
        } else if delete {
            op::DELETE_PROPERTY
        } else {
            op::GET_PROPERTY
        };
        match &x.name {
            None => {
                let name_item = x.name_item.clone().unwrap_or_else(|| node(Item::Undefined));
                let c1 = if x.obj.is_none() { Some(self.generate_coerce(Some(ld), &ns_ty)?) } else { None };
                let early = if x.obj.is_none() { Some(pool::mn(&mut self.abc, Multiname::RtqNameL { attr: false }) as i64) } else { None };
                let c2 = self.generate_coerce(Some(ld), &ns_ty)?;
                let rtql = match early {
                    Some(r) => r,
                    None => pool::mn(&mut self.abc, Multiname::RtqNameL { attr: false }) as i64,
                };
                if x.obj.is_none() {
                    m.c(self.src(ld, &x.ns)?);
                }
                m.opt(c1).c(self.src(ld, &name_item)?).op(op::CONVERT_S, &[]);
                match &x.obj {
                    Some(o) => m.c(self.src(ld, o)?),
                    None => m.op(op::FIND_PROPERTY_STRICT, &[rtql]),
                };
                if let Some(av) = &x.assigned {
                    m.c(self.src(ld, &x.ns)?).i(c2).c(self.src(ld, &name_item)?).op(op::CONVERT_S, &[]).c(self.src(ld, av)?);
                    if needs_ret {
                        m.c(self.dup_set_temp(ld, &mut rt));
                    }
                    m.op(op::SET_PROPERTY, &[rtql]);
                    if needs_ret {
                        m.c(Cx::get_temp(rt));
                    }
                    m.c(self.kill_temp(ld, &[rt]));
                } else {
                    if call {
                        m.c(self.dup_set_temp(ld, &mut ot));
                    }
                    m.c(self.src(ld, &x.ns)?).i(c2).c(self.src(ld, &name_item)?).op(op::CONVERT_S, &[]);
                    if construct {
                        for a in callargs {
                            m.c(self.src(ld, a)?);
                        }
                        m.op(get_code, &[rtql, callargs.len() as i64]);
                    } else {
                        m.op(get_code, &[rtql]);
                    }
                    if call {
                        m.c(Cx::get_temp(ot));
                        for a in callargs {
                            m.c(self.src(ld, a)?);
                        }
                        m.op(op::CALL, &[callargs.len() as i64]);
                    }
                    if !needs_ret {
                        m.op(op::POP, &[]);
                    }
                    m.c(self.kill_temp(ld, &[ot]));
                }
            }
            Some(name) => {
                let c1 = if x.obj.is_none() { Some(self.generate_coerce(Some(ld), &ns_ty)?) } else { None };
                let mut rtq_of = |cx: &mut Cx| {
                    let s = cx.str(name);
                    pool::mn(&mut cx.abc, Multiname::RtqName { attr: x.attr, name: s }) as i64
                };
                let early = if x.obj.is_none() { Some(rtq_of(self)) } else { None };
                let c2 = self.generate_coerce(Some(ld), &ns_ty)?;
                let rtq = match early {
                    Some(r) => r,
                    None => rtq_of(self),
                };
                if x.obj.is_none() {
                    m.c(self.src(ld, &x.ns)?);
                }
                m.opt(c1);
                match &x.obj {
                    Some(o) => m.c(self.src(ld, o)?),
                    None => m.op(op::FIND_PROPERTY_STRICT, &[rtq]),
                };
                if let Some(av) = &x.assigned {
                    m.c(self.src(ld, &x.ns)?).i(c2).c(self.src(ld, av)?);
                    if needs_ret {
                        m.c(self.dup_set_temp(ld, &mut rt));
                    }
                    m.op(op::SET_PROPERTY, &[rtq]);
                    if needs_ret {
                        m.c(Cx::get_temp(rt));
                    }
                    m.c(self.kill_temp(ld, &[rt]));
                } else {
                    if call {
                        m.c(self.dup_set_temp(ld, &mut ot));
                    }
                    m.c(self.src(ld, &x.ns)?).i(c2);
                    if construct {
                        for a in callargs {
                            m.c(self.src(ld, a)?);
                        }
                        m.op(get_code, &[rtq, callargs.len() as i64]);
                    } else {
                        m.op(get_code, &[rtq]);
                    }
                    if call {
                        m.c(Cx::get_temp(ot));
                        for a in callargs {
                            m.c(self.src(ld, a)?);
                        }
                        m.op(op::CALL, &[callargs.len() as i64]);
                    }
                    if !needs_ret {
                        m.op(op::POP, &[]);
                    }
                    m.c(self.kill_temp(ld, &[ot]));
                }
            }
        }
        Ok(m.0)
    }

    pub fn script_property_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool) -> R<Code> {
        let (ty, assigned) = match &*n.borrow() {
            Item::ScriptProperty { ty, assigned } => (ty.clone(), assigned.clone()),
            _ => unreachable!(),
        };
        let pid = self.type_name(Some(ld), &ty)? as i64;
        let obj = node(Item::FindProperty(ty));
        let mut m = M::new();
        let mut rt = -1;
        if let Some(av) = assigned {
            m.c(self.src(ld, &obj)?).c(self.src(ld, &av)?);
            if needs_ret {
                m.c(self.dup_set_temp(ld, &mut rt));
            }
            m.op(op::SET_PROPERTY, &[pid]);
            if needs_ret {
                m.c(Cx::get_temp(rt));
            }
            m.c(self.kill_temp(ld, &[rt]));
            return Ok(m.0);
        }
        m.c(self.src(ld, &obj)?).op(op::GET_PROPERTY, &[pid]);
        if !needs_ret {
            m.op(op::POP, &[]);
        }
        Ok(m.0)
    }

    pub fn call_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool) -> R<Code> {
        let c = match &*n.borrow() {
            Item::Call(c) => c.clone(),
            _ => unreachable!(),
        };
        let mut callable = resolved_of(&c.name);
        let name_info = match &*callable.borrow() {
            Item::Name(nm) => Some(nm.clone()),
            _ => None,
        };
        if let Some(nm) = name_info {
            if !ld.has_reg(&nm.var) {
                let p = property(None, nm.attribute, &nm.var, &nm.suffix, nm.opened.clone(), empty_stack(), false, None, c.line, c.this_type.clone());
                set_assigned(&p, nm.assigned.clone());
                callable = p;
            }
        }
        let mut prop_index: i64 = -1;
        if matches!(&*callable.borrow(), Item::Type { .. }) {
            prop_index = self.resolve_type(Some(ld), &callable)? as i64;
        }
        let sp_ty = match &*callable.borrow() {
            Item::ScriptProperty { ty, .. } => Some(ty.clone()),
            _ => None,
        };
        if let Some(t) = sp_ty {
            prop_index = self.resolve_type(Some(ld), &t)? as i64;
        }
        let mut args = c.args.clone();
        let mut obj: Option<Obj> = None;
        let prop_object = match &*callable.borrow() {
            Item::Property(p) => Some(p.object.clone()),
            _ => None,
        };
        if let Some(po) = prop_object {
            let r = self.property_resolve(&callable, false, ld)?;
            if let Some((t, aid)) = &r.tr {
                if let TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } = t.data {
                    let a = self.abc_of(*aid);
                    let pts: Vec<usize> = a.methods.get(method).map_or(Vec::new(), |m| m.param_types.clone());
                    let tys: Vec<Node> = pts.iter().map(|&p| if p == 0 { unbounded() } else { ty(super::index::raw_names(a).name_with_ns(p, true)) }).collect();
                    for (i, t) in tys.iter().enumerate() {
                        if i >= args.len() {
                            break;
                        }
                        handle_and_or_coerce(&args[i], t);
                    }
                }
            }
            obj = po.map(Obj::Item);
            prop_index = self.resolve_property_index(&callable, ld)? as i64;
        }
        if let Item::Call(cc) = &mut *n.borrow_mut() {
            cc.args = args.clone();
        }
        if matches!(&*callable.borrow(), Item::Type { .. }) && prop_index != -1 && args.len() == 1 {
            if let Some(i) = self.generate_convert(&callable) {
                let mut m = M::new();
                m.c(self.src(ld, &args[0])?).i(i);
                return Ok(m.0);
            }
        }
        if prop_index != -1 {
            let obj = obj.unwrap_or_else(|| Obj::Ins(ins(op::FIND_PROPERTY_STRICT, &[prop_index])));
            let is_super = matches!(&obj, Obj::Item(o) if matches!(&*o.borrow(), Item::Name(x) if x.var == "super"));
            let code = match (is_super, needs_ret) {
                (true, true) => op::CALL_SUPER,
                (true, false) => op::CALL_SUPER_VOID,
                (false, true) => op::CALL_PROPERTY,
                (false, false) => op::CALL_PROP_VOID,
            };
            let mut m = M::new();
            m.c(self.obj_code(ld, obj)?);
            for a in &args {
                m.c(self.src(ld, a)?);
            }
            m.op(code, &[prop_index, args.len() as i64]);
            return Ok(m.0);
        }
        if matches!(&*callable.borrow(), Item::Index(_)) {
            return self.index_src(ld, &callable, needs_ret, true, &args, false, false);
        }
        if matches!(&*callable.borrow(), Item::Namespaced(_)) {
            return self.namespaced_src(ld, &callable, needs_ret, true, &args, false, false);
        }
        let mut m = M::new();
        m.c(self.src(ld, &callable)?).op(op::GET_GLOBAL_SCOPE, &[]);
        for a in &args {
            m.c(self.src(ld, a)?);
        }
        m.op(op::CALL, &[args.len() as i64]);
        if !needs_ret {
            m.op(op::POP, &[]);
        }
        Ok(m.0)
    }

    pub fn construct_src(&mut self, ld: &mut Ld, n: &Node, needs_ret: bool) -> R<Code> {
        let c = match &*n.borrow() {
            Item::Construct(c) => c.clone(),
            _ => unreachable!(),
        };
        let res = resolved_of(&c.name);
        let nargs = c.args.len() as i64;
        let tail = |m: &mut M| {
            if !needs_ret {
                m.op(op::POP, &[]);
            }
        };
        let type_full = match &*res.borrow() {
            Item::Type { full, .. } => Some(full.clone()),
            _ => None,
        };
        let sp_ty = match &*res.borrow() {
            Item::ScriptProperty { ty, .. } => Some(ty.clone()),
            _ => None,
        };
        if let Some(full) = type_full {
            let cur = ld.pkg.as_ref().map(|p| match &ld.current_class {
                Some(cn) => p.add_with_suffix(cn),
                None => p.clone(),
            });
            if ld.is_static && cur.as_ref() == Some(&full) {
                let mut m = M::new();
                m.op(op::GET_LOCAL0, &[]);
                for a in &c.args {
                    m.c(self.src(ld, a)?);
                }
                m.op(op::CONSTRUCT, &[nargs]);
                tail(&mut m);
                return Ok(m.0);
            }
            let ti = self.resolve_type(Some(ld), &res)? as i64;
            return self.construct_prop(ld, ti, &c.args, needs_ret);
        }
        if let Some(t) = sp_ty {
            let ti = self.resolve_type(Some(ld), &t)? as i64;
            return self.construct_prop(ld, ti, &c.args, needs_ret);
        }
        if matches!(&*res.borrow(), Item::Property(_)) {
            let obj = self.resolve_object(&res, ld, true)?;
            let pi = self.resolve_property_index(&res, ld)? as i64;
            let mut m = M::new();
            m.c(self.obj_code(ld, obj)?);
            for a in &c.args {
                m.c(self.src(ld, a)?);
            }
            m.op(op::CONSTRUCT_PROP, &[pi, nargs]);
            tail(&mut m);
            return Ok(m.0);
        }
        if matches!(&*res.borrow(), Item::Index(_)) {
            return self.index_src(ld, &res, needs_ret, false, &c.args, false, true);
        }
        if matches!(&*res.borrow(), Item::Namespaced(_)) {
            return self.namespaced_src(ld, &res, needs_ret, false, &c.args, false, true);
        }
        let mut m = M::new();
        m.c(self.src(ld, &res)?);
        for a in &c.args {
            m.c(self.src(ld, a)?);
        }
        m.op(op::CONSTRUCT, &[nargs]);
        tail(&mut m);
        Ok(m.0)
    }

    fn construct_prop(&mut self, ld: &mut Ld, ti: i64, args: &[Node], needs_ret: bool) -> R<Code> {
        let mut m = M::new();
        m.op(op::FIND_PROPERTY_STRICT, &[ti, args.len() as i64]);
        for a in args {
            m.c(self.src(ld, a)?);
        }
        m.op(op::CONSTRUCT_PROP, &[ti, args.len() as i64]);
        if !needs_ret {
            m.op(op::POP, &[]);
        }
        Ok(m.0)
    }

    pub fn delete_src(&mut self, ld: &mut Ld, obj: &Node, line: usize) -> R<Code> {
        let p = resolved_of(obj);
        if matches!(&*p.borrow(), Item::Property(_)) {
            let o = self.resolve_object(&p, ld, true)?;
            let pi = self.resolve_property_index(&p, ld)? as i64;
            let mut c = self.obj_code(ld, o)?;
            c.push(ins(op::DELETE_PROPERTY, &[pi]));
            return Ok(c);
        }
        if matches!(&*p.borrow(), Item::Index(_)) {
            return self.index_src(ld, &p, true, false, &[], true, false);
        }
        if matches!(&*p.borrow(), Item::Namespaced(_)) {
            return self.namespaced_src(ld, &p, true, false, &[], true, false);
        }
        err(line, "Not a property")
    }

    /// Types of a method's parameters as written in its ABC.
    pub fn trait_type(&self, abc: &crate::abc::Abc, mn: usize) -> Node {
        spec_node(&mn_type(abc, mn))
    }
}
