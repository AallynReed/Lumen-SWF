//! Instruction translation: FFDec's `AVM2Code.toSourceOutput` and the per-instruction `translate`.

use std::collections::HashMap;
use std::rc::Rc;

use super::code::{Lit, op, Code, Ins};
use super::ctx::{Ctx, LocalData};
use super::graph::St;
use super::index;
use super::item::*;
use super::ld::{CodeStats, InsStats, Ld};
use super::names::Dotted;
use super::props::*;
use crate::abc::opcodes::Operand;
use crate::abc::{MethodBody, Multiname, TraitData};

/// Per-method translation context (the immutable parts of `AVM2GraphSource`/`AVM2LocalData`).
pub struct Tr<'c, 'a> {
    pub ctx: &'c Ctx<'a>,
    pub body: &'a MethodBody,
    pub code: Code,
    pub method: usize,
    pub is_static: bool,
    pub script_index: i32,
    pub class_index: i32,
    pub local_reg_names: HashMap<i32, String>,
    pub fqn: Vec<Dotted>,
    pub swf_version: u8,
    pub call_stack: Vec<usize>,
    pub path: String,
}

fn val(it: &It) -> Option<It> {
    it.borrow().value.clone()
}

fn set_val(it: &It, v: It) {
    it.borrow_mut().value = Some(v);
}

fn temp_of(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::Duplicate { temp } | K::DuplicateSource { temp, .. } | K::SetTemporary { temp, .. } | K::Temporary { temp } => Some(*temp),
        _ => None,
    }
}

fn st_temp(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::SetTemporary { temp, .. } => Some(*temp),
        _ => None,
    }
}

fn dup_temp(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::Duplicate { temp } => Some(*temp),
        _ => None,
    }
}

fn dup_src_temp(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::DuplicateSource { temp, .. } => Some(*temp),
        _ => None,
    }
}

fn is_inc(it: &It) -> bool {
    matches!(it.borrow().k, K::Increment)
}

fn is_dec(it: &It) -> bool {
    matches!(it.borrow().k, K::Decrement)
}

fn is_inc_dec(it: &It) -> bool {
    matches!(it.borrow().k, K::Increment | K::Decrement)
}

fn is_convert(it: &It) -> bool {
    matches!(it.borrow().k, K::Convert { .. })
}

fn is_coerce_or_convert(it: &It) -> bool {
    matches!(it.borrow().k, K::Coerce { .. } | K::Convert { .. })
}

fn is_getprop(it: &It) -> bool {
    matches!(it.borrow().k, K::GetProperty { .. })
}

fn reg_local(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::LocalReg { reg, .. } => Some(*reg),
        _ => None,
    }
}

fn reg_set(it: &It) -> Option<i32> {
    match &it.borrow().k {
        K::SetLocal { reg, .. } => Some(*reg),
        _ => None,
    }
}

fn gp_obj(it: &It) -> It {
    match &it.borrow().k {
        K::GetProperty { obj, .. } => obj.clone(),
        _ => panic!("not a getproperty"),
    }
}

fn gp_prop(it: &It) -> It {
    match &it.borrow().k {
        K::GetProperty { prop, .. } => prop.clone(),
        _ => panic!("not a getproperty"),
    }
}

fn gp_set_obj(it: &It, v: It) {
    if let K::GetProperty { obj, .. } = &mut it.borrow_mut().k {
        *obj = v;
    }
}

fn fm_index(it: &It) -> Option<usize> {
    match &it.borrow().k {
        K::FullMultiname { mn, .. } => Some(*mn),
        _ => None,
    }
}

fn fm_name(it: &It) -> Option<It> {
    match &it.borrow().k {
        K::FullMultiname { name, .. } => name.clone(),
        _ => None,
    }
}

fn fm_ns(it: &It) -> Option<It> {
    match &it.borrow().k {
        K::FullMultiname { ns, .. } => ns.clone(),
        _ => None,
    }
}

fn fm_set_name(it: &It, v: Option<It>) {
    if let K::FullMultiname { name, .. } = &mut it.borrow_mut().k {
        *name = v;
    }
}

fn fm_set_ns(it: &It, v: Option<It>) {
    if let K::FullMultiname { ns, .. } = &mut it.borrow_mut().k {
        *ns = v;
    }
}

fn opt_ptr_eq(a: &Option<It>, b: &Option<It>) -> bool {
    same_opt(a, b)
}

fn local_computed(it: &It) -> Option<Option<It>> {
    match &it.borrow().k {
        K::LocalReg { computed, .. } => Some(computed.clone()),
        _ => None,
    }
}

fn through_regs(mut t: Option<It>, stop_without_value: bool) -> Option<It> {
    while let Some(x) = t.clone() {
        match local_computed(&x) {
            Some(Some(c)) => t = Some(through_duplicate(&through_not_compilable(&c))),
            Some(None) if stop_without_value => break,
            Some(None) => break,
            None => break,
        }
    }
    t
}

/// `FullMultinameAVM2Item.compareSame`; like Java it rewrites `this.name` through duplicates.
pub fn compare_same(a: &It, b: &It) -> bool {
    if fm_index(a) != fm_index(b) {
        return false;
    }
    let ti_name = fm_name(a);
    if let Some(n) = &ti_name {
        fm_set_name(a, Some(through_duplicate(n)));
    }
    let ti_name = through_regs(ti_name, true);
    let ti_name2 = through_regs(fm_name(b).map(|n| through_duplicate(&n)), true);
    if !opt_ptr_eq(&ti_name, &ti_name2) {
        return false;
    }
    let ns1 = through_regs(fm_ns(a).map(|n| through_duplicate(&n)), false);
    let ns2 = through_regs(fm_ns(b).map(|n| through_duplicate(&n)), false);
    opt_ptr_eq(&ns1, &ns2)
}

/// `AVM2Item.mustStayIntact1`.
fn must_stay_intact1(t: &It) -> bool {
    matches!(not_coerced(t).borrow().k, K::ExceptionItem(_))
}

/// `AVM2Item.mustStayIntact2`.
fn must_stay_intact2(t: &It) -> bool {
    matches!(not_coerced(t).borrow().k, K::NextValue { .. } | K::NextName { .. })
}

/// `InstructionDefinition.getNumberType`.
fn number_type(it: &It) -> It {
    match &it.borrow().k {
        K::Convert { ty } | K::Coerce { ty } => ty.clone(),
        _ => ty("Number"),
    }
}

/// Left side, right side and operator of a `CompoundableBinaryOp`.
fn compound_parts(it: &It) -> Option<(It, It, String)> {
    match &it.borrow().k {
        K::Bin { op, l, r } if op.compoundable() => Some((l.clone(), r.clone(), op.op().to_string())),
        _ => None,
    }
}

fn set_info_mut(it: &It, f: impl FnOnce(&mut SetInfo)) {
    let mut b = it.borrow_mut();
    match &mut b.k {
        K::SetLocal { set, .. } | K::SetProperty { set, .. } | K::InitProperty { set, .. } | K::SetSlot { set, .. } | K::SetSuper { set, .. } => f(set),
        _ => {}
    }
}

fn set_compound(it: &It, value: It, op: String) {
    set_info_mut(it, |s| {
        s.compound_value = Some(value);
        s.compound_op = Some(op);
    });
}

/// `ConstructIns.walkXMLSub`.
fn walk_xml_sub(item: &It, list: &mut Vec<It>) -> bool {
    let k = item.borrow().k.clone();
    match k {
        K::Bin { op: Bin::Add, l, r } => {
            let a = walk_xml_sub(&l, list);
            let b = walk_xml_sub(&r, list);
            a || b
        }
        K::EscapeXElem | K::EscapeXAttr => {
            list.push(item.clone());
            true
        }
        _ => {
            list.push(item.clone());
            false
        }
    }
}

fn walk_xml(item: &It, list: &mut Vec<It>) -> bool {
    let ret = walk_xml_sub(item, list);
    if list.len() == 1 && matches!(list[0].borrow().k, K::Str(_)) {
        return true;
    }
    ret
}

/// One instruction's translation context.
struct X<'x, 'c, 'a> {
    tr: &'x Tr<'c, 'a>,
    ld: &'x mut Ld,
    out: &'x List,
    st: &'x St,
    ip: usize,
}

impl<'x, 'c, 'a> X<'x, 'c, 'a> {
    fn ls(&self) -> Option<usize> {
        self.ld.line_start
    }

    fn mk(&self, k: K, p: u8, v: Option<It>) -> It {
        mk(k, Some(self.ip), self.ld.line_start, p, v)
    }

    fn ins(&self) -> &Ins {
        &self.tr.code.ins[self.ip]
    }

    fn op(&self, i: usize) -> i32 {
        self.ins().op(i)
    }

    fn ctx(&self) -> &'c Ctx<'a> {
        self.tr.ctx
    }

    fn pop(&self) -> It {
        self.st.borrow_mut().pop()
    }

    fn push(&self, it: It) {
        self.st.borrow_mut().push(it);
    }

    fn peek(&self) -> It {
        self.st.borrow_mut().peek()
    }

    fn empty(&self) -> bool {
        self.st.borrow().is_empty()
    }

    fn add_out(&self, it: It) {
        self.st.borrow_mut().add_to_output(it);
    }

    fn finish(&self) {
        self.st.borrow_mut().finish_block(self.out);
    }

    fn move_to(&self) {
        self.st.borrow_mut().move_to_stack(self.out);
    }

    fn allow_swap(&self) {
        self.st.borrow_mut().allow_swap(self.out);
    }

    fn olen(&self) -> usize {
        self.out.borrow().len()
    }

    fn oat(&self, i: usize) -> It {
        self.out.borrow()[i].clone()
    }

    /// Item `n` places from the end (1 = last).
    fn oback(&self, n: usize) -> Option<It> {
        let o = self.out.borrow();
        if o.len() >= n {
            Some(o[o.len() - n].clone())
        } else {
            None
        }
    }

    fn orem_back(&self, n: usize) {
        let mut o = self.out.borrow_mut();
        let l = o.len();
        o.remove(l - n);
    }

    fn usages(&self, src: Option<usize>) -> usize {
        match src {
            Some(s) => self.ld.set_local_usages_of(s as i32).len(),
            None => 0,
        }
    }

    fn mn_name(&self, mn: usize) -> String {
        self.ctx().names().name(mn, &[], true, true)
    }

    fn multiname(&self, mn: usize) -> Option<&'a Multiname> {
        self.ctx().abc.constants.multinames.get(mn).filter(|_| mn != 0)
    }

    fn local_data(&self) -> LocalData<'c, 'a> {
        let mut l = LocalData::new(self.tr.ctx);
        l.local_reg_names = self.tr.local_reg_names.clone();
        l.fqn = self.tr.fqn.clone();
        l.class_index = self.tr.class_index;
        l.call_stack = self.tr.call_stack.clone();
        l
    }

    /// `resolveMultiname`.
    fn resolve_multiname(&self, property: bool, mn: usize) -> It {
        self.allow_swap();
        let mut name = None;
        let mut ns = None;
        if let Some(m) = self.multiname(mn) {
            if m.needs_runtime_name() {
                name = Some(self.pop());
            }
            if m.needs_runtime_ns() {
                ns = Some(self.pop());
            }
        }
        let resolved = self.mn_name(mn);
        self.mk(K::FullMultiname { mn, name, ns, property, resolved: Some(resolved) }, prec::PRIMARY, None)
    }

    fn plain_fm(&self, mn: usize) -> It {
        let resolved = self.mn_name(mn);
        self.mk(K::FullMultiname { mn, name: None, ns: None, property: false, resolved: Some(resolved) }, prec::PRIMARY, None)
    }

    fn args(&self, n: i32) -> Vec<It> {
        let mut a: Vec<It> = Vec::new();
        for _ in 0..n.max(0) {
            a.insert(0, self.pop());
        }
        a
    }

    fn current_class_name(&self) -> Option<String> {
        if self.tr.class_index < 0 {
            return None;
        }
        let inst = self.ctx().abc.instances.get(self.tr.class_index as usize)?;
        Some(self.ctx().names().name_with_ns(inst.name, true).raw())
    }

    fn slot_trait_type_in_call_stack(&self, name: &str) -> Option<It> {
        let abc = self.ctx().abc;
        for &m in self.tr.call_stack.iter().rev() {
            let Some(body) = abc.body(m) else { continue };
            for t in &body.traits {
                if let TraitData::Slot { type_name, .. } | TraitData::Const { type_name, .. } = t.data {
                    if self.mn_name(t.name) == name {
                        return Some(index::mn_type(self.ctx(), type_name));
                    }
                }
            }
        }
        None
    }

    fn ns_raw_of(&self, mn: usize) -> Option<String> {
        match self.multiname(mn)? {
            Multiname::QName { ns, .. } => Some(self.ctx().names().ns_raw_name(*ns)),
            _ => None,
        }
    }

    fn own_ns_index(&self, mn: usize) -> usize {
        match self.multiname(mn) {
            Some(Multiname::QName { ns, .. }) => *ns,
            _ => 0,
        }
    }

    /// `GetLexIns.resolveLexType`: (is_static, type, call type).
    fn resolve_lex_type(&self, mn: usize) -> (bool, It, It) {
        let name = self.mn_name(mn);
        if let Some(t) = self.slot_trait_type_in_call_stack(&name) {
            return (false, t.clone(), t);
        }
        let mut t = unknown();
        let mut ct = unknown();
        if let Some(cls) = self.current_class_name() {
            if let Some(r) = index::find_property_type(self.ctx(), &ty(&cls), &name, self.own_ns_index(mn), true, true, true) {
                t = r.ty;
                ct = r.call_ty;
            }
        }
        if is_unknown(&t) {
            let ti = index::mn_type(self.ctx(), mn);
            if index::find_class(self.ctx(), &ti, self.tr.script_index) {
                return (true, ti, unbounded());
            }
            if let Some(raw) = self.ns_raw_of(mn) {
                if let Some((rt, crt)) = index::find_script_property(self.ctx(), &name, &Dotted::parse_with_suffix(&raw)) {
                    return (true, rt, crt);
                }
            }
        }
        (false, t, ct)
    }

    /// `GetPropertyIns.resolvePropertyType`: (is_static, type, call type).
    fn resolve_property_type(&self, obj: &It, fm: &It) -> (bool, It, It) {
        let mn = fm_index(fm).unwrap_or(0);
        let name = self.mn_name(mn);
        let fp = match &obj.borrow().k {
            K::FindProperty { prop } => Some(prop.clone()),
            _ => None,
        };
        if let Some(fprop) = fp {
            if super::eq::eq(&fprop, fm) {
                if let Some(t) = self.slot_trait_type_in_call_stack(&name) {
                    return (false, t.clone(), t);
                }
                let mut t = unknown();
                let mut ct = unknown();
                if let Some(cls) = self.current_class_name() {
                    if let Some(r) = index::find_property_type(self.ctx(), &ty(&cls), &name, self.own_ns_index(mn), true, true, true) {
                        t = r.ty;
                        ct = r.call_ty;
                    }
                }
                if is_unknown(&t) {
                    let ti = index::mn_type(self.ctx(), mn);
                    if index::find_class(self.ctx(), &ti, self.tr.script_index) {
                        return (true, ti.clone(), ti);
                    }
                    if let Some(raw) = self.ns_raw_of(mn) {
                        if let Some((rt, crt)) = index::find_script_property(self.ctx(), &name, &Dotted::parse_with_suffix(&raw)) {
                            return (true, rt, crt);
                        }
                    }
                }
                return (false, t, ct);
            }
            return (false, unknown(), unknown());
        }
        let mut receiver = return_type(obj);
        if is_unbounded(&receiver) || is_unknown(&receiver) {
            return (false, unknown(), unknown());
        }
        let parent_static = match &obj.borrow().k {
            K::GetLex { is_static, .. } | K::GetProperty { is_static, .. } => *is_static,
            _ => false,
        };
        let applied = match &receiver.borrow().k {
            K::ApplyType { obj, params } => Some((obj.clone(), params.borrow().first().cloned())),
            _ => None,
        };
        let m = self.multiname(mn);
        if let Some((aobj, p0)) = applied {
            if m.is_some_and(|m| m.needs_runtime_name()) {
                return (false, p0.unwrap_or_else(unbounded), unbounded());
            }
            receiver = aobj;
            if is_type(&receiver, "__AS3__.vec.Vector") || super::eq::eq(&receiver, &ty("__AS3__.vec.Vector")) {
                let ps = p0.map(|p| java_to_string(&p)).unwrap_or_default();
                receiver = match ps.as_str() {
                    "double" | "int" | "uint" => ty(&format!("__AS3__.vec.Vector${ps}")),
                    _ => ty("__AS3__.vec.Vector$object"),
                };
            }
        }
        if m.is_some_and(|m| m.is_attribute()) {
            return (false, ty("XMLList"), unknown());
        }
        if is_type(&receiver, "XMLList") {
            if let Some(n) = fm_name(fm) {
                if is_type(&return_type(&n), "int") {
                    return (false, ty("XML"), unknown());
                }
            }
        }
        let resolved = match &fm.borrow().k {
            K::FullMultiname { resolved, .. } => resolved.clone().unwrap_or_default(),
            _ => String::new(),
        };
        let mut t = unknown();
        let mut ct = unknown();
        if let Some(r) = index::find_property_type(self.ctx(), &receiver, &resolved, self.own_ns_index(mn), parent_static, !parent_static, false) {
            t = r.ty;
            ct = r.call_ty;
        }
        if is_type(&receiver, "XML") && !is_type(&t, "Function") {
            t = ty("XMLList");
        }
        (false, t, ct)
    }

    /// `FullMultinameAVM2Item.isTopLevel`.
    fn is_top_level(&self, fm: &It, tname: &str) -> bool {
        let Some(mn) = fm_index(fm) else { return false };
        let cname = match fm_name(fm) {
            Some(n) => super::print::to_str(&n, &self.local_data()),
            None => self.ctx().names().name(mn, &self.tr.fqn, true, true),
        };
        let cns = match fm_ns(fm) {
            Some(n) => super::print::to_str(&n, &self.local_data()),
            None => match self.multiname(mn) {
                Some(Multiname::QName { ns, .. }) if *ns != 0 && self.ctx().abc.constants.namespaces.get(*ns).is_some_and(|n| n.name != 0) => {
                    self.ctx().names().ns_name(*ns).printable(&self.ctx().ids)
                }
                _ => String::new(),
            },
        };
        cname == tname && cns.is_empty()
    }

    /// `checkDup2`.
    fn check_dup2(&self, item1: &It, item2: &It) -> i32 {
        if same(item1, item2) {
            return -1;
        }
        let ok = (dup_src_temp(item1).is_some() && dup_temp(item2).is_some()) || (dup_temp(item1).is_some() && dup_src_temp(item2).is_some());
        if !ok {
            return -2;
        }
        let t1 = temp_of(item1).unwrap();
        if t1 != temp_of(item2).unwrap() {
            return -2;
        }
        if let Some(last) = self.oback(1) {
            if st_temp(&last) == Some(t1) {
                self.orem_back(1);
                self.move_to();
            }
        }
        t1
    }

    /// `checkDup`.
    fn check_dup(&self, dup_source: &It, dup: &It) {
        let Some(last) = self.oback(1) else { return };
        let Some(stt) = st_temp(&last) else { return };
        let ds = not_coerced_no_dup(dup_source);
        let Some(dst) = dup_src_temp(&ds) else { return };
        let d = not_coerced_no_dup(dup);
        let Some(dt) = dup_temp(&d) else { return };
        if dst != dt || dt != stt {
            return;
        }
        self.orem_back(1);
        self.move_to();
    }

    /// `SetTypeIns.handleResult`.
    fn handle_result(&mut self, value: &It, result: &It, reg: i32) {
        let mut value = value.clone();
        let coerced = is_coerce_or_convert(&value);
        let not_coerced_value = if coerced { val(&value).unwrap() } else { value.clone() };
        self.move_to();
        if !self.empty() {
            let top = self.peek();
            if self.check_dup2(&not_coerced_value, &top) >= -1 {
                let new_value = through_duplicate(&not_coerced_value);
                if coerced {
                    set_val(&val(result).unwrap(), new_value.clone());
                } else {
                    set_val(result, new_value.clone());
                }
                if must_stay_intact2(&new_value) {
                    if let K::SetLocal { reg, ty: t, .. } = &result.borrow().k.clone() {
                        self.pop();
                        self.move_to();
                        self.add_out(result.clone());
                        let v = val(result);
                        self.push(mk(K::LocalReg { reg: *reg, computed: v, ty: t.clone(), is_ct: false }, None, None, prec::PRIMARY, None));
                        return;
                    }
                }
                self.pop();
                self.move_to();
                self.push(result.clone());
                return;
            }
            if dup_temp(&not_coerced_value).is_some() {
                let mut inside = val(&not_coerced_value).unwrap();
                if !must_stay_intact1(&not_coerced_no_dup(&inside)) && same(&through_duplicate(&self.peek()), &through_duplicate(&inside)) {
                    self.pop();
                    if dup_temp(&inside).is_some() && reg > -1 {
                        let mut num = 1;
                        while dup_temp(&inside).is_some() && !self.empty() && same(&through_duplicate(&self.peek()), &through_duplicate(&inside)) {
                            inside = val(&inside).unwrap();
                            self.pop();
                            num += 1;
                        }
                        if coerced {
                            set_val(&value, inside);
                        } else {
                            value = inside;
                        }
                        set_val(result, value.clone());
                        if self.oback(1).is_some_and(|l| st_temp(&l).is_some()) {
                            self.orem_back(1);
                        }
                        self.add_out(result.clone());
                        for _ in 0..num {
                            let t = self.ld.local_reg_types.borrow().get(&reg).cloned().unwrap_or_else(|| return_type(&value));
                            self.push(mk(K::LocalReg { reg, computed: Some(value.clone()), ty: t, is_ct: false }, None, self.ls(), prec::PRIMARY, None));
                        }
                        return;
                    }
                }
            }
        }
        self.finish();
        self.add_out(result.clone());
    }

    /// `InstructionDefinition.searchSlotName`: slot multiname and the object that owns it.
    fn search_slot_name(&self, slot: i32, obj: &It, mn_filter: i64) -> (Option<usize>, Option<It>) {
        let mut obj = obj.clone();
        let last = match &obj.borrow().k {
            K::Comma { cmds } => cmds.borrow().last().cloned(),
            _ => None,
        };
        if let Some(l) = last {
            obj = l;
        }
        let k = obj.borrow().k.clone();
        if let K::ExceptionItem(e) = &k {
            if mn_filter == -1 || e.var_name as i64 == mn_filter {
                return (Some(e.var_name), None);
            }
        }
        if let K::FindProperty { prop } = &k {
            let filter = fm_index(prop).map_or(-1, |m| m as i64);
            let items = self.ld.local_scope_stack.borrow().items.clone();
            for item in items {
                let r = self.search_slot_name(slot, &item, filter);
                if r.0.is_some() {
                    return r;
                }
            }
            return (None, None);
        }
        let abc = self.ctx().abc;
        let ci = self.tr.class_index;
        let traits = match &k {
            K::NewActivation => Some(&self.tr.body.traits),
            K::This { .. } if ci > -1 => abc.instances.get(ci as usize).map(|i| &i.traits),
            K::Class { .. } if ci > -1 => abc.classes.get(ci as usize).map(|c| &c.traits),
            K::Global => abc.scripts.get(self.tr.script_index.max(0) as usize).map(|s| &s.traits),
            _ => None,
        };
        if let Some(traits) = traits {
            for t in traits {
                let sid = match t.data {
                    TraitData::Slot { slot_id, .. } | TraitData::Const { slot_id, .. } | TraitData::Class { slot_id, .. } | TraitData::Function { slot_id, .. } => slot_id,
                    _ => continue,
                };
                if (mn_filter == -1 || t.name as i64 == mn_filter) && sid as i32 == slot {
                    return (Some(t.name), Some(obj));
                }
            }
        }
        (None, None)
    }

    fn activation_slot_type(&self, slot: i32) -> It {
        for t in &self.tr.body.traits {
            if let TraitData::Slot { slot_id, type_name, .. } | TraitData::Const { slot_id, type_name, .. } = t.data {
                if slot_id as i32 == slot {
                    return index::mn_type(self.ctx(), type_name);
                }
            }
        }
        unbounded()
    }

    fn temp_findprop(&self, obj: It) -> It {
        let tv = match &obj.borrow().k {
            K::Temporary { temp } => Some(*temp),
            _ => None,
        };
        if let Some(temp) = tv {
            let v = val(&obj).unwrap();
            if is::find_property(&v) {
                for i in (0..self.olen()).rev() {
                    if st_temp(&self.oat(i)) == Some(temp) {
                        self.out.borrow_mut().remove(i);
                        return v;
                    }
                }
            }
        }
        obj
    }

    fn post_inc(&self, src: Option<usize>, ls: Option<usize>, obj: It, t: It, inc: bool) -> It {
        let k = if inc { K::PostIncrement { obj, ty: t } } else { K::PostDecrement { obj, ty: t } };
        mk(k, src, ls, prec::POSTFIX, None)
    }

    fn pre_inc(&self, src: Option<usize>, ls: Option<usize>, obj: It, t: It, inc: bool) -> It {
        let op = if inc { Un::PreIncrement } else { Un::PreDecrement };
        mk(K::Un { op, ty: Some(t) }, src, ls, prec::UNARY, Some(obj))
    }

    fn src_ls(it: &It) -> (Option<usize>, Option<usize>) {
        let b = it.borrow();
        (b.src, b.line_start)
    }
}

impl<'c, 'a> Tr<'c, 'a> {
    /// `code.toSourceOutput(...)` over `start..=end`.
    pub fn translate_range(&self, output: &List, ld: &mut Ld, stack: &St, start: usize, end: usize) {
        let code = &self.code.ins;
        let mut visited = vec![false; code.len()];
        let mut ip = start;
        while ip <= end && ip < code.len() {
            if visited[ip] {
                break;
            }
            visited[ip] = true;
            if stack.borrow().is_empty() {
                ld.line_start = Some(ip);
            }
            let c = code[ip].code;
            if c == op::NEWFUNCTION && ip < end && code[ip + 1].code == op::POP {
                ip += 2;
                continue;
            }
            if c == op::KILL && self.kill_set_property(output, stack, ip) {
                ip += 2;
                continue;
            }
            let mut x = X { tr: self, ld: &mut *ld, out: output, st: stack, ip };
            if matches!(c, op::RETURNVALUE | op::RETURNVOID | op::THROW) {
                x.translate();
                break;
            } else if c == op::NEWFUNCTION {
                let mut function_name = String::new();
                let (ipi, si, ei) = (ip as i64, start as i64, end as i64);
                if ipi >= si + 2 && ipi <= ei - 4 && code[ip - 2].code == op::NEWOBJECT && code[ip - 2].op(0) == 0 && code[ip - 1].code == op::PUSHWITH {
                    let plus = usize::from(code[ip + 1].code == op::DUP);
                    let psco = &code[ip + 1 + plus];
                    let lss = x.ld.local_scope_stack.borrow().len() as i32;
                    if psco.code == 0x65 && psco.op(0) == lss - 1 && code.get(ip + plus + 2).is_some_and(|i| i.code == op::SWAP) && code.get(ip + plus + 4).is_some_and(|i| i.code == op::POPSCOPE) && code.get(ip + plus + 3).is_some_and(|i| i.code == op::SETPROPERTY) {
                        function_name = self.ctx.names().name(code[ip + plus + 3].op(0) as usize, &self.fqn, true, true);
                        x.ld.local_scope_stack.borrow_mut().pop();
                        x.finish();
                        x.move_to();
                        output.borrow_mut().pop();
                        x.ip = ip + plus + 4;
                    }
                }
                let real_ip = x.ip;
                x.ip = ip;
                x.translate();
                let top = stack.borrow_mut().peek();
                if let K::NewFunction { name, .. } = &mut top.borrow_mut().k {
                    *name = function_name;
                }
                ip = real_ip + 1;
            } else {
                x.translate();
                let s = stack.borrow();
                if s.len() == 1 {
                    let top = s.items[0].clone();
                    drop(s);
                    if is::set_local(&top) {
                        let mut b = ld.bottom_set_locals.borrow_mut();
                        if !b.iter().any(|i| same(i, &top)) {
                            b.push(top);
                        }
                    }
                }
                ip += 1;
            }
        }
    }

    fn kill_set_property(&self, output: &List, stack: &St, ip: usize) -> bool {
        let killed = self.code.ins[ip].op(0);
        if output.borrow().len() < 2 || stack.borrow().is_empty() {
            return false;
        }
        let top = stack.borrow_mut().peek();
        if reg_local(&top) != Some(killed) {
            return false;
        }
        let (set_prop, set_loc) = {
            let o = output.borrow();
            (o[o.len() - 1].clone(), o[o.len() - 2].clone())
        };
        if !matches!(set_prop.borrow().k, K::SetProperty { .. }) || reg_set(&set_loc) != Some(killed) {
            return false;
        }
        if !self.code.ins.get(ip + 1).is_some_and(|i| i.code == op::POP) {
            return false;
        }
        let Some(v) = val(&set_prop) else { return false };
        if reg_local(&v) != Some(killed) {
            return false;
        }
        set_val(&set_prop, val(&set_loc).unwrap());
        {
            let mut o = output.borrow_mut();
            let l = o.len();
            o.remove(l - 2);
        }
        stack.borrow_mut().pop();
        true
    }

    /// `AVM2Code.getStats(abc, body, initScope, false)`.
    pub fn stats(&self) -> CodeStats {
        let n = self.code.len();
        let init_scope = self.body.init_scope_depth as i32;
        let mut stats = CodeStats { ins: vec![InsStats::default(); n], init_scope, ..Default::default() };
        if !self.walk_code(&mut stats, 0, 0, init_scope) {
            return stats;
        }
        let mut scope_pos = -1;
        for e in &self.code.exceptions {
            if scope_pos == -1 {
                match self.code.adr2pos(e.end as i64) {
                    Some(p) if p > 0 => scope_pos = stats.ins[p - 1].scopepos_after,
                    _ => continue,
                }
            }
            let visited: Vec<bool> = stats.ins.iter().map(|s| s.seen).collect();
            let Some(t) = self.code.adr2pos(e.target as i64) else { continue };
            if !self.walk_code(&mut stats, t, 1, scope_pos) {
                return stats;
            }
            let mut max_ip = 0;
            for (i, s) in stats.ins.iter().enumerate() {
                if s.seen && !visited[i] {
                    max_ip = i;
                }
            }
            scope_pos = stats.ins.get(max_ip).map_or(scope_pos, |s| s.scopepos_after);
        }
        stats
    }

    fn walk_code(&self, stats: &mut CodeStats, pos: usize, stack: i32, scope: i32) -> bool {
        let code = &self.code;
        let mut q: std::collections::VecDeque<(usize, i32, i32)> = std::collections::VecDeque::from([(pos, stack, scope)]);
        'q: while let Some((mut pos, mut stack, mut scope)) = q.pop_front() {
            while pos < code.len() {
                let ins = &code.ins[pos];
                if stats.ins[pos].seen {
                    continue 'q;
                }
                stats.ins[pos].seen = true;
                stats.ins[pos].stackpos = stack;
                stats.ins[pos].scopepos = scope;
                let (pop, push, sd) = self.stack_effect(ins);
                stack += push - pop;
                scope += sd;
                stats.ins[pos].stackpos_after = stack;
                stats.ins[pos].scopepos_after = scope;
                stats.max_stack = stats.max_stack.max(stack);
                stats.max_scope = stats.max_scope.max(scope);
                if ins.code == op::DXNS || ins.code == op::DXNSLATE {
                    stats.has_set_dxns = true;
                }
                if ins.code == op::NEWACTIVATION {
                    stats.has_activation = true;
                }
                if let Some(r) = ins.set_local_reg().or(ins.get_local_reg()) {
                    stats.max_local = stats.max_local.max(r + 1);
                } else {
                    for (i, k) in ins.kinds.iter().enumerate() {
                        if matches!(k, Operand::LocalRegIndex) {
                            stats.max_local = stats.max_local.max(ins.op(i) + 1);
                        }
                    }
                }
                if ins.is_exit() {
                    continue 'q;
                }
                if ins.is_jump() {
                    match code.adr2pos(ins.target_address()) {
                        Some(p) => {
                            pos = p;
                            continue;
                        }
                        None => return false,
                    }
                } else if ins.is_if_type() {
                    match code.adr2pos(ins.target_address()) {
                        Some(p) => q.push_back((p, stack, scope)),
                        None => return false,
                    }
                }
                if ins.code == op::LOOKUPSWITCH {
                    for i in 0..ins.ops.len() {
                        if i == 1 {
                            continue;
                        }
                        match code.adr2pos(code.pos2adr(pos) + ins.ops[i]) {
                            Some(p) => q.push_back((p, stack, scope)),
                            None => return false,
                        }
                    }
                }
                pos += 1;
            }
        }
        true
    }

    fn mn_stack(&self, mn: i32) -> i32 {
        match self.ctx.abc.constants.multinames.get(mn as usize).filter(|_| mn > 0) {
            Some(m) => i32::from(m.needs_runtime_name()) + i32::from(m.needs_runtime_ns()),
            None => 0,
        }
    }

    /// `ins.translate(localData, stack, output)` for one instruction, as the deobfuscator runs it.
    pub fn translate_one(&self, ld: &mut Ld, out: &List, st: &St, ip: usize) {
        st.borrow_mut().set_connected_output(Some(out.clone()));
        let mut x = X { tr: self, ld, out, st, ip };
        x.translate();
    }

    /// (pop count, push count, scope delta) of an instruction.
    pub fn stack_effect(&self, ins: &Ins) -> (i32, i32, i32) {
        let o = |i: usize| ins.op(i);
        match ins.name {
            "add_d" | "add_i" | "add" | "astypelate" | "bitand" | "bitor" | "bitxor" | "concat" | "divide" | "divide_p" | "equals" | "greaterequals" | "greaterthan" | "hasnext" | "in" | "instanceof"
            | "istypelate" | "lshift" | "lessequals" | "lessthan" | "modulo" | "modulo_p" | "multiply_i" | "multiply" | "multiply_p" | "nextname" | "nextvalue" | "rshift" | "strictequals" | "subtract_i"
            | "subtract" | "subtract_p" | "urshift" => (2, 1, 0),
            "applytype" => (o(0) + 1, 1, 0),
            "astype" | "bitnot" | "checkfilter" | "coerce_a" | "coerce_b" | "coerce_d" | "coerce_i" | "coerce" | "coerce_o" | "coerce_s" | "coerce_u" | "convert_b" | "convert_d" | "convert_f4" | "convert_f"
            | "convert_i" | "convert_m" | "convert_m_p" | "convert_o" | "convert_s" | "convert_u" | "decrement_i" | "decrement" | "decrement_p" | "esc_xattr" | "esc_xelem" | "getslot" | "increment_i"
            | "increment" | "increment_p" | "istype" | "lf32" | "lf32x4" | "lf64" | "li16" | "li32" | "li8" | "negate_i" | "negate" | "negate_p" | "newclass" | "not" | "sxi16" | "sxi1" | "sxi8" | "typeof"
            | "unplus" => (1, 1, 0),
            "call" => (o(0) + 2, 1, 0),
            "callmethod" | "callstatic" => (o(1) + 1, 1, 0),
            "callproplex" | "callproperty" | "callsuper" | "constructprop" => (o(1) + 1 + self.mn_stack(o(0)), 1, 0),
            "callpropvoid" | "callsupervoid" => (o(1) + 1 + self.mn_stack(o(0)), 0, 0),
            "construct" => (o(0) + 1, 1, 0),
            "constructsuper" => (o(0) + 1, 0, 0),
            "dxnslate" | "iffalse" | "iftrue" | "lookupswitch" | "pop" | "ffdec_deobfuscatepop" | "returnvalue" | "setglobalslot" | "throw" | "setlocal" | "setlocal0" | "setlocal1" | "setlocal2" | "setlocal3" => (1, 0, 0),
            "deleteproperty" | "getdescendants" | "getproperty" | "getsuper" => (1 + self.mn_stack(o(0)), 1, 0),
            "dup" => (1, 2, 0),
            "swap" => (2, 2, 0),
            "finddef" => (0, 0, 1),
            "findproperty" | "findpropstrict" => (self.mn_stack(o(0)), 1, 0),
            "getglobalscope" | "getglobalslot" | "getlex" | "getouterscope" | "getscopeobject" | "hasnext2" | "newactivation" | "newcatch" | "newfunction" | "pushbyte" | "pushdnan" | "pushdecimal"
            | "pushdouble" | "pushfalse" | "pushfloat4" | "pushfloat" | "pushint" | "pushnamespace" | "pushnan" | "pushnull" | "pushshort" | "pushstring" | "pushtrue" | "pushuint" | "pushundefined"
            | "getlocal" | "getlocal0" | "getlocal1" | "getlocal2" | "getlocal3" => (0, 1, 0),
            "ifeq" | "ifge" | "ifgt" | "ifle" | "iflt" | "ifnge" | "ifngt" | "ifnle" | "ifnlt" | "ifne" | "ifstricteq" | "ifstrictne" | "setslot" | "sf32" | "sf64" | "si16" | "si32" | "si8" => (2, 0, 0),
            "initproperty" | "setproperty" | "setsuper" => (2 + self.mn_stack(o(0)), 0, 0),
            "newarray" => (o(0), 1, 0),
            "newobject" => (o(0) * 2, 1, 0),
            "popscope" => (0, 0, -1),
            "pushscope" | "pushwith" => (1, 0, 1),
            _ => (0, 0, 0),
        }
    }
}

impl<'x, 'c, 'a> X<'x, 'c, 'a> {
    fn bin(&self, op: Bin) {
        let v2 = self.pop();
        let v1 = self.pop();
        self.push(bin(op, Some(self.ip), self.ls(), v1, v2));
    }

    fn un(&self, op: Un) {
        let v = self.pop();
        self.push(self.mk(K::Un { op, ty: None }, prec::UNARY, Some(v)));
    }

    fn convert(&self, t: &str) {
        let v = self.pop();
        let p = prec_of(&v);
        self.push(self.mk(K::Convert { ty: ty(t) }, p, Some(v)));
    }

    fn coerce(&self, t: It) {
        let v = self.pop();
        let p = prec_of(&v);
        self.push(self.mk(K::Coerce { ty: t }, p, Some(v)));
    }

    fn alchemy_load(&self, t: &str, size: i32) {
        let ofs = self.pop();
        self.push(self.mk(K::AlchemyLoad { ty: t.into(), size, ofs }, prec::PRIMARY, None));
    }

    fn alchemy_store(&self, t: &str, size: i32) {
        let ofs = self.pop();
        let v = self.pop();
        self.add_out(self.mk(K::AlchemyStore { ty: t.into(), size, ofs }, prec::PRIMARY, Some(v)));
    }

    fn translate(&mut self) {
        let name = self.ins().name;
        let c = self.ins().code;
        match name {
            "add" | "add_i" | "add_p" => self.bin(Bin::Add),
            "subtract" | "subtract_i" | "subtract_p" => self.bin(Bin::Sub),
            "multiply" | "multiply_i" | "multiply_p" => self.bin(Bin::Mul),
            "divide" | "divide_p" => self.bin(Bin::Div),
            "modulo" | "modulo_p" => self.bin(Bin::Mod),
            "lshift" => self.bin(Bin::LShift),
            "rshift" => self.bin(Bin::RShift),
            "urshift" => self.bin(Bin::URShift),
            "bitand" => self.bin(Bin::BitAnd),
            "bitor" => self.bin(Bin::BitOr),
            "bitxor" => self.bin(Bin::BitXor),
            "equals" | "ifeq" => self.bin(Bin::Eq),
            "ifne" => self.bin(Bin::Neq),
            "strictequals" | "ifstricteq" => self.bin(Bin::StrictEq),
            "ifstrictne" => self.bin(Bin::StrictNeq),
            "lessthan" | "iflt" | "ifnge" => self.bin(Bin::Lt),
            "lessequals" | "ifle" | "ifngt" => self.bin(Bin::Le),
            "greaterthan" | "ifgt" | "ifnle" => self.bin(Bin::Gt),
            "greaterequals" | "ifge" | "ifnlt" => self.bin(Bin::Ge),
            "instanceof" => self.bin(Bin::InstanceOf),
            "istypelate" => self.bin(Bin::IsType),
            "astypelate" => self.bin(Bin::AsType),
            "in" => {
                let obj = self.pop();
                let name = self.pop();
                self.push(bin(Bin::In, Some(self.ip), self.ls(), name, obj));
            }
            "istype" | "astype" => {
                let v = self.pop();
                let fm = self.plain_fm(self.op(0) as usize);
                self.push(bin(if name == "istype" { Bin::IsType } else { Bin::AsType }, Some(self.ip), self.ls(), v, fm));
            }
            "iffalse" => {
                let v = self.pop();
                self.push(invert(&v, Some(self.ip)));
            }
            "not" => {
                let v = self.pop();
                self.push(invert(&v, Some(self.ip)));
            }
            "negate" | "negate_i" | "negate_p" => self.un(Un::Neg),
            "bitnot" => self.un(Un::BitNot),
            "typeof" => self.un(Un::TypeOf),
            "increment" | "increment_i" | "increment_p" => {
                let v = self.pop();
                self.push(self.mk(K::Increment, prec::ADDITIVE, Some(v)));
            }
            "decrement" | "decrement_i" | "decrement_p" => {
                let v = self.pop();
                self.push(self.mk(K::Decrement, prec::ADDITIVE, Some(v)));
            }
            "convert_b" => self.convert("Boolean"),
            "convert_d" => self.convert("Number"),
            "convert_f4" => self.convert("float4"),
            "convert_f" => self.convert("float"),
            "convert_i" => self.convert("int"),
            "convert_m" | "convert_m_p" => self.convert("decimal"),
            "convert_o" => self.convert("Object"),
            "convert_s" => self.convert("String"),
            "convert_u" => self.convert("uint"),
            "coerce_a" => self.coerce(unbounded()),
            "coerce_s" => self.coerce(ty("String")),
            "coerce" => self.coerce(index::mn_type(self.ctx(), self.op(0) as usize)),
            "esc_xattr" | "esc_xelem" => {
                let v = self.pop();
                self.push(self.mk(if name == "esc_xattr" { K::EscapeXAttr } else { K::EscapeXElem }, prec::NOPRECEDENCE, Some(v)));
            }
            "checkfilter" => {
                let obj = self.pop();
                self.push(self.mk(K::FilteredCheck { obj }, prec::NOPRECEDENCE, None));
            }
            "lf32" => self.alchemy_load("f", 32),
            "lf32x4" => self.alchemy_load("f4", 32),
            "lf64" => self.alchemy_load("f", 64),
            "li8" => self.alchemy_load("i", 8),
            "li16" => self.alchemy_load("i", 16),
            "li32" => self.alchemy_load("i", 32),
            "sf32" => self.alchemy_store("f", 32),
            "sf64" => self.alchemy_store("f", 64),
            "si8" => self.alchemy_store("i", 8),
            "si16" => self.alchemy_store("i", 16),
            "si32" => self.alchemy_store("i", 32),
            "sxi1" | "sxi8" | "sxi16" => {
                let size = match name {
                    "sxi1" => 1,
                    "sxi8" => 8,
                    _ => 16,
                };
                let v = self.pop();
                self.push(self.mk(K::AlchemySignExtend { size }, prec::PRIMARY, Some(v)));
            }
            "pushint" | "pushbyte" | "pushshort" if matches!(self.ins().lit, Some(Lit::Int(_))) => {
                let Some(Lit::Int(v)) = self.ins().lit else { unreachable!() };
                self.push(integer_a(Some(self.ip), self.ls(), v));
            }
            "pushdouble" if matches!(self.ins().lit, Some(Lit::Double(_))) => {
                let Some(Lit::Double(v)) = self.ins().lit else { unreachable!() };
                self.push(self.mk(K::DoubleValue(v), prec::PRIMARY, None));
            }
            "pushstring" if matches!(self.ins().lit, Some(Lit::Str(_))) => {
                let Some(Lit::Str(s)) = self.ins().lit.clone() else { unreachable!() };
                self.push(string_item(Some(self.ip), self.ls(), s));
            }
            "ffdec_deobfuscatepop" => self.deobfuscate_pop(),
            "pushbyte" => self.push(integer_a(Some(self.ip), self.ls(), self.op(0) as i8 as i32)),
            "pushshort" => self.push(integer_a(Some(self.ip), self.ls(), self.op(0) as i16 as i32)),
            "pushint" => {
                let v = self.ctx().abc.constants.ints.get(self.op(0) as usize).copied().unwrap_or(0);
                self.push(integer_a(Some(self.ip), self.ls(), v));
            }
            "pushuint" => {
                let v = self.ctx().abc.constants.uints.get(self.op(0) as usize).copied().unwrap_or(0);
                self.push(self.mk(K::DoubleValue(v as f64), prec::PRIMARY, None));
            }
            "pushdouble" => {
                let v = self.ctx().abc.constants.doubles.get(self.op(0) as usize).copied().unwrap_or(0.0);
                self.push(self.mk(K::DoubleValue(v), prec::PRIMARY, None));
            }
            "pushfloat" => {
                let v = self.ctx().abc.constants.floats.get(self.op(0) as usize).copied().unwrap_or(0.0);
                self.push(self.mk(K::FloatValue(v), prec::PRIMARY, None));
            }
            "pushdecimal" => {
                let v = self.ctx().abc.constants.decimals.get(self.op(0) as usize).copied().unwrap_or([0; 16]);
                self.push(self.mk(K::DecimalValue(v), prec::PRIMARY, None));
            }
            "pushfloat4" => self.push(self.mk(K::Float4Value([0.0; 4]), prec::PRIMARY, None)),
            "pushstring" => {
                let s = self.ctx().abc.str(self.op(0) as usize).to_string();
                self.push(string_item(Some(self.ip), self.ls(), s));
            }
            "pushnamespace" => self.push(self.mk(K::NameSpace(self.op(0) as usize), prec::NOPRECEDENCE, None)),
            "pushnan" => self.push(self.mk(K::Nan, prec::NOPRECEDENCE, None)),
            "pushdnan" => self.push(self.mk(K::DNan, prec::NOPRECEDENCE, None)),
            "pushnull" => self.push(null(Some(self.ip), self.ls())),
            "pushundefined" => self.push(undefined(Some(self.ip), self.ls())),
            "pushtrue" => self.push(true_item(Some(self.ip), self.ls())),
            "pushfalse" => self.push(false_item(Some(self.ip), self.ls())),
            "pop" => {
                let top = self.pop();
                if matches!(top.borrow().k, K::Any | K::FindProperty { .. }) {
                    return;
                }
                self.add_out(top);
            }
            "dup" => self.dup(),
            "swap" => self.swap(),
            "returnvalue" => {
                let mut v = self.pop();
                if is::set_local(&v) {
                    v = val(&v).unwrap();
                }
                self.add_out(self.mk(K::ReturnValue, prec::NOPRECEDENCE, Some(v)));
            }
            "returnvoid" => self.add_out(self.mk(K::ReturnVoid, prec::NOPRECEDENCE, None)),
            "throw" => {
                let v = self.pop();
                self.add_out(self.mk(K::Throw, prec::NOPRECEDENCE, Some(v)));
            }
            "dxns" => {
                let s = string_item(Some(self.ip), self.ls(), self.ctx().abc.str(self.op(0) as usize).to_string());
                self.add_out(self.mk(K::DefaultXmlNs { ns: s }, prec::NOPRECEDENCE, None));
            }
            "dxnslate" => {
                let ns = self.pop();
                self.add_out(self.mk(K::DefaultXmlNs { ns }, prec::NOPRECEDENCE, None));
            }
            "getglobalscope" => self.push(self.mk(K::Global, prec::PRIMARY, None)),
            "getscopeobject" => {
                let v = self.ld.local_scope_stack.borrow_mut().get(self.op(0) as usize);
                self.push(v);
            }
            "getouterscope" => {
                let item = self.ld.scope_stack.borrow_mut().get(self.op(0) as usize);
                let global = matches!(&item.borrow().k, K::This { class_name: Some(c), .. } if *c == Dotted::parse_no_suffix("global"));
                if global {
                    self.push(mk(K::Global, None, None, prec::PRIMARY, None));
                    return;
                }
                self.push(item);
            }
            "pushscope" => {
                let top = self.pop();
                if is::local_reg(&top) {
                    if let Some(gip) = top.borrow().src {
                        for (_, s) in self.ld.set_local_usages.borrow_mut().iter_mut() {
                            s.remove(&(gip as i32));
                        }
                    }
                }
                self.ld.local_scope_stack.borrow_mut().push(top);
            }
            "pushwith" => {
                let w = self.pop();
                let wot = self.mk(K::WithObject { scope: w.clone() }, prec::PRIMARY, None);
                self.ld.local_scope_stack.borrow_mut().push(wot);
                self.add_out(self.mk(K::With { scope: w, items: empty_list() }, prec::NOPRECEDENCE, None));
            }
            "popscope" => {
                let scope = self.ld.local_scope_stack.borrow_mut().pop();
                let inner = match &scope.borrow().k {
                    K::WithObject { scope } => Some(scope.clone()),
                    _ => None,
                };
                if let Some(s) = inner {
                    self.add_out(self.mk(K::WithEnd { scope: s }, prec::PRIMARY, None));
                }
            }
            "newactivation" => self.push(self.mk(K::NewActivation, prec::NOPRECEDENCE, None)),
            "newcatch" => {
                let e = self.tr.code.exceptions.get(self.op(0) as usize).cloned();
                if let Some(e) = e {
                    self.push(mk(K::ExceptionItem(e), None, None, prec::NOPRECEDENCE, None));
                }
            }
            "newfunction" => {
                let mut scope = self.ld.scope_stack.borrow().items.clone();
                scope.extend(self.ld.local_scope_stack.borrow().items.iter().cloned());
                let f = K::NewFunction { name: String::new(), path: self.tr.path.clone(), is_static: false, script: self.tr.script_index, class: self.tr.class_index, method: self.op(0) as usize, scope };
                self.push(self.mk(f, prec::PRIMARY, None));
            }
            "newclass" => self.new_class(),
            "newarray" => {
                let mut args = Vec::new();
                for _ in 0..self.op(0) {
                    let item = self.pop();
                    if is::pop(&item) {
                        self.push(null(Some(self.ip), self.ls()));
                        return;
                    }
                    args.insert(0, item);
                }
                self.push(self.mk(K::NewArray { values: new_list(args) }, prec::PRIMARY, None));
            }
            "newobject" => {
                let mut args = Vec::new();
                for _ in 0..self.op(0) {
                    let value = self.pop();
                    if is::pop(&value) {
                        self.push(null(Some(self.ip), self.ls()));
                        return;
                    }
                    let name = self.pop();
                    args.insert(0, mk(K::NameValuePair { name }, None, None, prec::NOPRECEDENCE, Some(value)));
                }
                self.push(self.mk(K::NewObject { pairs: new_list(args) }, prec::PRIMARY, None));
            }
            "nextname" | "nextvalue" => {
                let index = self.pop();
                let obj = self.pop();
                let k = if name == "nextname" { K::NextName { index, obj, local_reg: None } } else { K::NextValue { index, obj, local_reg: None } };
                self.push(self.mk(k, prec::NOPRECEDENCE, None));
            }
            "hasnext" => {
                let index = self.pop();
                let obj = self.pop();
                self.push(self.mk(K::HasNext { index, obj }, prec::NOPRECEDENCE, None));
            }
            "hasnext2" => {
                let (obj_reg, index_reg) = (self.op(0), self.op(1));
                let regs = self.ld.local_regs.borrow();
                let index = self.mk(K::LocalReg { reg: index_reg, computed: regs.get(&index_reg).cloned(), ty: unbounded(), is_ct: false }, prec::PRIMARY, None);
                let obj = self.mk(K::LocalReg { reg: obj_reg, computed: regs.get(&obj_reg).cloned(), ty: unbounded(), is_ct: false }, prec::PRIMARY, None);
                drop(regs);
                self.push(self.mk(K::HasNext { index, obj }, prec::NOPRECEDENCE, None));
            }
            "applytype" => {
                let params = self.args(self.op(0));
                let obj = self.pop();
                self.push(self.mk(K::ApplyType { obj, params: new_list(params) }, prec::PRIMARY, None));
            }
            "finddef" => self.push(self.mk(K::FindDef { mn: self.op(0) as usize }, prec::PRIMARY, None)),
            "findproperty" | "findpropstrict" => {
                let fm = self.resolve_multiname(true, self.op(0) as usize);
                self.push(self.mk(K::FindProperty { prop: fm }, prec::PRIMARY, None));
            }
            "getlex" => {
                let mn = self.op(0) as usize;
                let (is_static, t, ct) = self.resolve_lex_type(mn);
                let full = self.ctx().names().name_with_ns(mn, true);
                self.push(self.mk(K::GetLex { mn, ty: t, call_ty: ct, is_static, full }, prec::PRIMARY, None));
            }
            "getdescendants" => {
                let fm = self.resolve_multiname(true, self.op(0) as usize);
                let obj = self.pop();
                self.push(self.mk(K::GetDescendants { obj, mn: fm }, prec::PRIMARY, None));
            }
            "deleteproperty" => {
                let fm = self.resolve_multiname(true, self.op(0) as usize);
                let obj = self.pop();
                let (is_static, _, _) = self.resolve_property_type(&obj, &fm);
                let it = self.mk(K::DeleteProperty { obj, prop: fm, is_static }, prec::UNARY, None);
                self.st.borrow_mut().items.push(it);
            }
            "getproperty" => self.get_property(),
            "getsuper" => {
                let fm = self.resolve_multiname(true, self.op(0) as usize);
                let obj = self.pop();
                let (is_static, t, ct) = self.resolve_property_type(&obj, &fm);
                self.push(self.mk(K::GetSuper { obj, prop: fm, ty: t, call_ty: ct, is_static }, prec::PRIMARY, None));
            }
            "setsuper" => self.set_super(),
            "setproperty" => self.handle_set_property(false),
            "initproperty" => self.handle_set_property(true),
            "getslot" => {
                let slot = self.op(0);
                let objinreg = self.pop();
                let mut obj = through_register(&objinreg);
                let (slot_name, real) = self.search_slot_name(slot, &obj, -1);
                if let Some(r) = real {
                    obj = r;
                }
                let slot_ty = if matches!(obj.borrow().k, K::NewActivation) { self.activation_slot_type(slot) } else { unbounded() };
                self.push(self.mk(K::GetSlot { slot_name, scope: obj, slot_obj: objinreg, idx: slot, slot_ty }, prec::PRIMARY, None));
            }
            "getglobalslot" => {
                let slot = self.op(0);
                let obj = self.mk(K::Global, prec::PRIMARY, None);
                let (slot_name, _) = self.search_slot_name(slot, &obj, -1);
                self.push(self.mk(K::GetSlot { slot_name, scope: obj.clone(), slot_obj: obj, idx: slot, slot_ty: unbounded() }, prec::PRIMARY, None));
            }
            "setslot" => {
                let slot = self.op(0);
                self.allow_swap();
                let value = self.pop();
                let obj = self.pop();
                self.handle_set_slot(slot, obj, value);
            }
            "setglobalslot" => {
                let slot = self.op(0);
                self.allow_swap();
                let value = self.pop();
                let obj = self.mk(K::Global, prec::PRIMARY, None);
                self.handle_set_slot(slot, obj, value);
            }
            "call" => self.call(),
            "callmethod" | "callstatic" => {
                let mi = self.op(0) as usize;
                let args = self.args(self.op(1));
                let receiver = self.pop();
                let abc = self.ctx().abc;
                let mname = abc.methods.get(mi).map(|m| abc.str(m.name).to_string()).unwrap_or_default();
                let k = if name == "callmethod" { K::CallMethod { receiver, name: mname, args: new_list(args) } } else { K::CallStatic { receiver, name: mname, args: new_list(args) } };
                self.push(self.mk(k, prec::PRIMARY, None));
            }
            "callproperty" | "callpropvoid" | "callproplex" => {
                let mn = self.op(0) as usize;
                let args = self.args(self.op(1));
                let fm = self.resolve_multiname(true, mn);
                let mut obj = self.pop();
                if name != "callproplex" {
                    obj = self.temp_findprop(obj);
                }
                let (is_static, _, ct) = self.resolve_property_type(&obj, &fm);
                let is_void = name == "callpropvoid";
                let it = self.mk(K::CallProperty { receiver: obj, prop: fm, args: new_list(args), is_void, ty: ct, is_static }, prec::PRIMARY, None);
                if is_void {
                    self.add_out(it);
                } else {
                    self.push(it);
                }
            }
            "callsuper" | "callsupervoid" => {
                let mn = self.op(0) as usize;
                let args = self.args(self.op(1));
                let fm = self.resolve_multiname(true, mn);
                let receiver = self.pop();
                let is_void = name == "callsupervoid";
                let it = self.mk(K::CallSuper { receiver, mn: fm, args: new_list(args), is_void }, prec::PRIMARY, None);
                if is_void {
                    self.add_out(it);
                } else {
                    self.push(it);
                }
            }
            "construct" => self.construct(),
            "constructsuper" => {
                let args = self.args(self.op(0));
                let obj = self.pop();
                self.add_out(self.mk(K::ConstructSuper { obj, args: new_list(args) }, prec::PRIMARY, None));
            }
            "constructprop" => self.construct_prop(),
            "getlocal" | "getlocal0" | "getlocal1" | "getlocal2" | "getlocal3" => {
                let r = self.ins().get_local_reg().unwrap();
                self.get_local(r);
            }
            "setlocal" | "setlocal0" | "setlocal1" | "setlocal2" | "setlocal3" => {
                let r = self.ins().set_local_reg().unwrap();
                self.set_local(r);
            }
            "inclocal" | "inclocal_i" | "inclocal_p" | "declocal" | "declocal_i" | "declocal_p" => {
                let p = name.ends_with("_p");
                let reg = if p { self.op(1) } else { self.op(0) };
                let t = if p {
                    ty("decimal")
                } else if name.ends_with("_i") {
                    ty("int")
                } else {
                    ty("Number")
                };
                self.inc_dec_local(reg, t, name.starts_with("inc"));
            }
            _ => {
                let _ = c;
            }
        }
    }

    fn inc_dec_local(&self, reg: i32, t: It, inc: bool) {
        let mut post = false;
        if !self.empty() {
            let top = self.peek();
            if reg_local(&top) == Some(reg) {
                self.pop();
                self.push(self.post_inc(Some(self.ip), self.ls(), top, t.clone(), inc));
                post = true;
            }
        }
        if !post {
            let k = if inc { K::IncLocal { reg, ty: t } } else { K::DecLocal { reg, ty: t } };
            self.add_out(self.mk(k, prec::POSTFIX, None));
        }
        let cur = self.ld.local_regs.borrow().get(&reg).cloned();
        if let Some(cur) = cur {
            let one = integer_a(Some(self.ip), self.ls(), 1);
            let nv = bin(if inc { Bin::Add } else { Bin::Sub }, Some(self.ip), self.ls(), cur, one);
            self.ld.local_regs.borrow_mut().insert(reg, nv);
        }
        *self.ld.local_reg_assignment_ips.borrow_mut().entry(reg).or_insert(0) += 1;
    }

    fn dup(&self) {
        let v = self.pop();
        if is::simple_value(&v) {
            self.push(v.clone());
            self.push(v);
            return;
        }
        let temp;
        if matches!(v.borrow().k, K::NewActivation | K::ExceptionItem(_)) {
            self.push(v.clone());
            temp = 0;
        } else if let Some(t) = temp_of(&v) {
            temp = t;
            self.push(v.clone());
        } else {
            temp = self.ld.max_temp.get() + 1;
            self.ld.max_temp.set(temp);
            self.finish();
            self.add_out(self.mk(K::SetTemporary { temp, declaration: false, ref_count: 2, suffix: "dup".into() }, prec::ASSIGNMENT, Some(v.clone())));
            self.finish();
            self.push(self.mk(K::DuplicateSource { temp, declaration: false }, prec::PRIMARY, Some(v.clone())));
        }
        self.push(self.mk(K::Duplicate { temp }, prec::PRIMARY, Some(v)));
    }

    /// `DeobfuscatePopIns.translate`: drops the value without a statement, releasing the
    /// temporaries its duplicates hold.
    fn deobfuscate_pop(&self) {
        let item = self.pop();
        let out = self.out.clone();
        let st = self.st.clone();
        let mut visit = |sub: &It| -> bool {
            let Some(temp) = (match &sub.borrow().k {
                K::Duplicate { temp } | K::DuplicateSource { temp, .. } => Some(*temp),
                _ => None,
            }) else {
                return true;
            };
            let last = out.borrow().last().cloned();
            let Some(last) = last else { return false };
            let (lt, rc) = match &last.borrow().k {
                K::SetTemporary { temp, ref_count, .. } => (*temp, *ref_count),
                _ => return false,
            };
            if lt != temp {
                return false;
            }
            let set_rc = |v: i32| {
                if let K::SetTemporary { ref_count, .. } = &mut last.borrow_mut().k {
                    *ref_count = v;
                }
            };
            let mut rc = rc - 1;
            set_rc(rc);
            if rc <= 0 {
                out.borrow_mut().pop();
                st.borrow_mut().move_to_stack(&out);
            } else if rc == 1 {
                let n = st.borrow().len();
                for i in 0..n {
                    let s = st.borrow_mut().get(i);
                    if temp_of(&s) == Some(temp) {
                        let v = last.borrow().value.clone().expect("value");
                        st.borrow_mut().set(i, v);
                        rc -= 1;
                        set_rc(rc);
                        out.borrow_mut().pop();
                        st.borrow_mut().move_to_stack(&out);
                    }
                }
            }
            false
        };
        visit(&item);
        visit_recursively(&item, &mut visit);
    }

    fn swap(&self) {
        let o1 = self.pop();
        let o2 = self.pop();
        let simple = |o: &It| (is::simple_value(o) && !is::duplicate(o)) || matches!(o.borrow().k, K::EscapeXAttr);
        let both_ex = matches!(o1.borrow().k, K::ExceptionItem(_)) && matches!(o2.borrow().k, K::ExceptionItem(_));
        if both_ex || (simple(&o1) && simple(&o2)) || matches!(o2.borrow().k, K::Temporary { .. } | K::Duplicate { .. } | K::DuplicateSource { .. }) {
            self.push(o1);
            self.push(o2);
            return;
        }
        let temp = self.ld.max_temp.get() + 1;
        self.ld.max_temp.set(temp);
        self.finish();
        self.add_out(self.mk(K::SetTemporary { temp, declaration: false, ref_count: 1, suffix: "swap".into() }, prec::ASSIGNMENT, Some(o2.clone())));
        self.push(o1);
        self.push(self.mk(K::Temporary { temp }, prec::PRIMARY, Some(o2)));
        self.finish();
    }

    fn new_class(&self) {
        let cls = self.op(0) as usize;
        let obj = self.pop();
        if let Some(t) = dup_src_temp(&obj) {
            if self.oback(1).is_some_and(|l| st_temp(&l) == Some(t)) {
                self.orem_back(1);
                self.move_to();
            }
        }
        let mut ld = self.local_data();
        ld.class_index = cls as i32;
        let base = super::print::to_str(&obj, &ld);
        let abc = self.ctx().abc;
        let cname = abc.instances.get(cls).map(|i| self.ctx().names().name(i.name, &self.tr.fqn, false, true)).unwrap_or_default();
        self.push(self.mk(K::Unparsed(format!("\u{a7}\u{a7}newclass({cname},{base})")), prec::NOPRECEDENCE, None));
    }

    fn call(&self) {
        let args = self.args(self.op(0));
        self.allow_swap();
        let mut receiver = self.pop();
        let function = self.pop();
        if is_getprop(&function) {
            let gobj = gp_obj(&function);
            if let Some(dt) = dup_temp(&gobj) {
                if opt_ptr_eq(&val(&gobj), &Some(through_duplicate(&receiver))) {
                    gp_set_obj(&function, through_duplicate(&receiver));
                }
                if dup_src_temp(&receiver).is_some() {
                    receiver = through_duplicate(&receiver);
                }
                if self.oback(1).is_some_and(|l| st_temp(&l) == Some(dt)) {
                    self.orem_back(1);
                    self.move_to();
                }
            } else if let Some(sr) = reg_set(&gobj) {
                if reg_local(&receiver) == Some(sr) {
                    let v = val(&gobj).unwrap();
                    gp_set_obj(&function, v.clone());
                    receiver = v;
                }
            }
        }
        self.push(self.mk(K::Call { receiver, function, args: new_list(args) }, prec::PRIMARY, None));
    }

    fn construct(&self) {
        let args = self.args(self.op(0));
        let obj = self.pop();
        let names = self.ctx().names();
        let mut is_xml = false;
        let mut is_regexp = false;
        if is_getprop(&obj) {
            let gobj = gp_obj(&obj);
            let fp = match &gobj.borrow().k {
                K::FindProperty { prop } => Some(prop.clone()),
                _ => None,
            };
            if let Some(fpm) = fp {
                let gpm = gp_prop(&obj);
                is_xml = self.is_top_level(&fpm, "XML") && self.is_top_level(&gpm, "XML");
                is_regexp = self.is_top_level(&fpm, "RegExp") && self.is_top_level(&gpm, "RegExp");
            }
        }
        if let K::GetLex { mn, .. } = &obj.borrow().k {
            let n = names.name(*mn, &self.tr.fqn, true, true);
            is_xml = n == "XML";
            is_regexp = n == "RegExp";
        }
        if self.xml_or_regexp(is_xml, is_regexp, &args) {
            return;
        }
        self.push(self.mk(K::Construct { obj, args: new_list(args) }, prec::PRIMARY, None));
    }

    fn xml_or_regexp(&self, is_xml: bool, is_regexp: bool, args: &[It]) -> bool {
        if is_xml && args.len() == 1 {
            let mut lines = Vec::new();
            if walk_xml(&args[0], &mut lines) {
                self.push(self.mk(K::Xml { parts: new_list(lines) }, prec::NOPRECEDENCE, None));
                return true;
            }
        }
        let s = |i: usize| match &args[i].borrow().k {
            K::Str(s) => Some(s.clone()),
            _ => None,
        };
        if is_regexp && !args.is_empty() && s(0).is_some() && (args.len() == 1 || (args.len() == 2 && s(1).is_some())) {
            let pattern = s(0).unwrap();
            let modifier = if args.len() == 2 { s(1).unwrap() } else { String::new() };
            self.push(self.mk(K::RegExp { pattern, modifier }, prec::PRIMARY, None));
            return true;
        }
        false
    }

    fn construct_prop(&self) {
        let mn = self.op(0) as usize;
        let args = self.args(self.op(1));
        let fm = self.resolve_multiname(true, mn);
        let obj = self.pop();
        let is_xml = self.is_top_level(&fm, "XML");
        let is_regexp = self.is_top_level(&fm, "RegExp");
        if self.xml_or_regexp(is_xml, is_regexp, &args) {
            return;
        }
        let (is_static, t, _) = self.resolve_property_type(&obj, &fm);
        if is::find_property(&obj) {
            if let K::FullMultiname { property, .. } = &mut fm.borrow_mut().k {
                *property = false;
            }
        }
        self.push(self.mk(K::ConstructProp { obj, prop: fm, args: new_list(args), ty: t, is_static }, prec::PRIMARY, None));
    }

    fn get_property(&self) {
        let fm = self.resolve_multiname(true, self.op(0) as usize);
        let obj = self.pop();
        let fp = match &obj.borrow().k {
            K::FindProperty { prop } => Some(prop.clone()),
            _ => None,
        };
        if let Some(fpn) = fp {
            if fm_index(&fpn).is_some() {
                if let (Some(n1), Some(n2)) = (fm_name(&fpn), fm_name(&fm)) {
                    if let (Some(r1), Some(r2)) = (reg_local(&n1), reg_local(&n2)) {
                        if let Some(last) = self.oback(1) {
                            if let Some(sr) = reg_set(&last) {
                                if sr == r1 && sr == r2 && self.usages(last.borrow().src) == 2 {
                                    fm_set_name(&fpn, val(&last));
                                    self.orem_back(1);
                                }
                            }
                        }
                    }
                }
                if let Some(n1) = fm_name(&fpn) {
                    if let Some(t) = temp_of(&n1).filter(|_| is::duplicate(&n1) || is::duplicate_source(&n1)) {
                        let n2 = fm_name(&fm);
                        if n2.as_ref().is_some_and(|n2| same(&through_duplicate(&n1), &through_duplicate(n2))) {
                            fm_set_name(&fpn, val(&n1));
                            fm_set_name(&fm, n2.map(|n| through_duplicate(&n)));
                            if self.oback(1).is_some_and(|l| st_temp(&l) == Some(t)) {
                                self.orem_back(1);
                            }
                        }
                    }
                }
                if let Some(ns1) = fm_ns(&fpn) {
                    if let Some(sr) = reg_set(&ns1) {
                        if let Some(ns2) = fm_ns(&fm) {
                            if reg_local(&ns2) == Some(sr) && self.usages(ns1.borrow().src) == 1 {
                                fm_set_ns(&fpn, val(&ns1));
                            }
                        }
                    }
                }
                if let Some(ns1) = fm_ns(&fpn) {
                    if let Some(t) = temp_of(&ns1).filter(|_| is::duplicate(&ns1) || is::duplicate_source(&ns1)) {
                        let ns2 = fm_ns(&fm);
                        if ns2.as_ref().is_some_and(|n2| same(&through_duplicate(&ns1), &through_duplicate(n2))) {
                            fm_set_ns(&fpn, Some(through_duplicate(&ns1)));
                            fm_set_ns(&fm, ns2.map(|n| through_duplicate(&n)));
                            if self.oback(1).is_some_and(|l| st_temp(&l) == Some(t)) {
                                self.orem_back(1);
                            }
                        }
                    }
                }
                self.move_to();
            }
        }
        let (is_static, t, ct) = self.resolve_property_type(&obj, &fm);
        self.push(self.mk(K::GetProperty { obj, prop: fm, ty: t, call_ty: ct, is_static, null_cond: false }, prec::PRIMARY, None));
    }

    fn set_super(&mut self) {
        self.allow_swap();
        let mn = self.op(0) as usize;
        let value = self.pop();
        let fm = self.resolve_multiname(true, mn);
        let obj = self.pop();
        let (is_static, t, ct) = self.resolve_property_type(&obj, &fm);
        let result = self.mk(K::SetSuper { obj: obj.clone(), prop: fm.clone(), set: SetInfo::default(), ty: t, call_ty: ct, is_static }, prec::ASSIGNMENT, Some(value.clone()));
        if let Some((l, r, o)) = compound_parts(&not_coerced_no_dup(&value)) {
            if !has_side_effect(&obj) && !has_side_effect(&fm) {
                let ln = not_coerced(&l);
                let gs = match &ln.borrow().k {
                    K::GetSuper { obj, prop, .. } => Some((obj.clone(), prop.clone())),
                    _ => None,
                };
                if let Some((gobj, gprop)) = gs {
                    if super::eq::eq(&obj, &through_duplicate(&gobj)) && super::eq::eq(&fm, &gprop) {
                        set_compound(&result, r, o);
                    }
                }
            }
        }
        self.handle_result(&value, &result, -1);
    }

    fn get_local(&self, reg: i32) {
        if reg == 0 {
            let abc = self.ctx().abc;
            let ci = self.tr.class_index;
            let this = |cn: Dotted, basic: bool| self.mk(K::This { class_name: Some(cn), basic, show_class: false }, prec::PRIMARY, None);
            if ci == -1 {
                self.push(this(Dotted::parse_no_suffix("global"), false));
                return;
            }
            if ci as usize >= abc.instances.len() {
                self.push(this(Dotted::new(&["Object"]), false));
                return;
            }
            let inst = &abc.instances[ci as usize];
            if self.tr.is_static {
                self.push(mk(K::Class { mn: Some(inst.name), name: None }, None, None, prec::PRIMARY, None));
            } else {
                let cn = self.ctx().names().name_with_ns(inst.name, true);
                self.push(this(cn, self.ld.this_has_default_to_primitive));
            }
            return;
        }
        let computed = self.ld.local_regs.borrow().get(&reg).cloned();
        let t = match self.ld.local_reg_types.borrow().get(&reg) {
            Some(t) => t.clone(),
            None => computed.as_ref().map(return_type).unwrap_or_else(unbounded),
        };
        let result = self.mk(K::LocalReg { reg, computed, ty: t, is_ct: false }, prec::PRIMARY, None);
        self.finish();
        if let Some(last) = self.oback(1) {
            if reg_set(&last) == Some(reg) && last.borrow().src.is_some() && self.usages(last.borrow().src) == 1 && self.oback(2).is_some_and(|p| is::push(&p)) {
                self.orem_back(1);
                self.move_to();
                self.push(val(&last).unwrap());
                return;
            }
        }
        if let Some(last) = self.oback(1) {
            let lk = last.borrow().k.clone();
            let inc = matches!(lk, K::IncLocal { .. });
            match lk {
                K::IncLocal { reg: r, ty: t } | K::DecLocal { reg: r, ty: t } if r == reg => {
                    self.orem_back(1);
                    self.move_to();
                    let (s, l) = X::src_ls(&last);
                    self.push(self.pre_inc(s, l, result, t, inc));
                    return;
                }
                _ => {}
            }
        }
        if let Some(set_prop) = self.oback(1).filter(|l| matches!(l.borrow().k, K::SetProperty { .. })) {
            if self.post_from_set_property(&set_prop, reg) {
                return;
            }
        }
        self.move_to();
        if let Some(set_item) = self.oback(1) {
            if super::avm2graph::is_set_type(&set_item) {
                let sv = val(&set_item).unwrap();
                let set_local = not_coerced_no_dup(&sv);
                if reg_set(&set_local) == Some(reg) {
                    let sl_ip = set_local.borrow().src;
                    if self.usages(sl_ip) == 1 {
                        if matches!(set_item.borrow().k, K::SetProperty { .. }) {
                            let slv = val(&set_local).unwrap();
                            if is_inc_dec(&slv) {
                                let inc = is_inc(&slv);
                                let gp = val(&slv).unwrap();
                                if is_getprop(&gp) {
                                    let gobj = gp_obj(&gp);
                                    let sobj = match &set_item.borrow().k {
                                        K::SetProperty { obj, .. } => obj.clone(),
                                        _ => unreachable!(),
                                    };
                                    if let (Some(d), Some(ds)) = (dup_temp(&gobj), dup_src_temp(&sobj)) {
                                        if ds == d {
                                            if let Some(st) = self.oback(2).filter(|s| st_temp(s) == Some(d)) {
                                                gp_set_obj(&gp, val(&st).unwrap());
                                                self.orem_back(1);
                                                self.orem_back(1);
                                                self.move_to();
                                                let (s, l) = X::src_ls(&slv);
                                                self.push(self.pre_inc(s, l, gp, ty("Number"), inc));
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let slv = val(&set_local).unwrap();
                        if is_coerce_or_convert(&sv) {
                            set_val(&sv, slv);
                        } else {
                            set_val(&set_item, slv);
                        }
                        self.orem_back(1);
                        self.move_to();
                        self.push(set_item);
                        return;
                    }
                }
            }
            self.finish();
        }
        self.move_to();
        self.push(result);
    }

    fn post_from_set_property(&self, set_prop: &It, reg: i32) -> bool {
        let Some(spv) = val(set_prop) else { return false };
        if !is_inc_dec(&spv) {
            return false;
        }
        let inc = is_inc(&spv);
        let set_loc = val(&spv).unwrap();
        if reg_set(&set_loc) != Some(reg) {
            return false;
        }
        let Some(src) = set_loc.borrow().src else { return false };
        if self.usages(Some(src)) != 1 {
            return false;
        }
        let slv = val(&set_loc).unwrap();
        if !is_convert(&slv) {
            return false;
        }
        let gp = val(&slv).unwrap();
        if !is_getprop(&gp) {
            return false;
        }
        let sobj = match &set_prop.borrow().k {
            K::SetProperty { obj, .. } => obj.clone(),
            _ => return false,
        };
        if dup_src_temp(&sobj).is_none() {
            return false;
        }
        let Some(d) = dup_temp(&gp_obj(&gp)) else { return false };
        let Some(st) = self.oback(2).filter(|s| st_temp(s) == Some(d)) else { return false };
        gp_set_obj(&gp, val(&st).unwrap());
        self.orem_back(1);
        self.orem_back(1);
        self.move_to();
        let (s, l) = X::src_ls(&spv);
        self.push(self.post_inc(s, l, gp, slv, inc));
        true
    }

    fn set_local(&mut self, reg: i32) {
        let value = self.pop();
        let last_cmd = match &value.borrow().k {
            K::Comma { cmds } => Some(cmds.borrow().last().cloned()),
            _ => None,
        };
        match last_cmd {
            Some(Some(l)) => {
                self.ld.local_regs.borrow_mut().insert(reg, l);
            }
            Some(None) => {}
            None if !is::pop(&value) => {
                self.ld.local_regs.borrow_mut().insert(reg, value.clone());
            }
            None => {}
        }
        *self.ld.local_reg_assignment_ips.borrow_mut().entry(reg).or_insert(0) += 1;
        if matches!(through_duplicate(&value).borrow().k, K::NewActivation) {
            self.add_out(self.mk(K::StoreNewActivation { reg }, prec::PRIMARY, None));
            return;
        }
        if is::find_property(&value) {
            return;
        }
        let nc = not_coerced(&value);
        if is_inc_dec(&nc) {
            let inc = is_inc(&nc);
            let inside = through_duplicate(&not_coerced(&val(&nc).unwrap()));
            if reg_local(&inside) == Some(reg) {
                let nt = number_type(&value);
                if !self.empty() {
                    let top = through_duplicate(&not_coerced(&self.peek()));
                    let top_is_same_incdec = (if inc { is_inc(&top) } else { is_dec(&top) }) && opt_ptr_eq(&val(&top), &Some(inside.clone()));
                    if same(&top, &inside) {
                        let p = self.pop();
                        self.check_dup(&p, &val(&nc).unwrap());
                        self.push(self.post_inc(Some(self.ip), self.ls(), inside, nt, inc));
                    } else if top_is_same_incdec {
                        let p = self.pop();
                        self.check_dup(&p, &value);
                        self.push(self.pre_inc(Some(self.ip), self.ls(), inside, nt, inc));
                    } else {
                        self.add_out(self.post_inc(Some(self.ip), self.ls(), inside, nt, inc));
                    }
                } else {
                    self.add_out(self.post_inc(Some(self.ip), self.ls(), inside, nt, inc));
                }
                return;
            }
        }
        let rt = return_type(&value);
        let result = self.mk(K::SetLocal { reg, set: SetInfo::default(), ty: rt.clone(), hide_value: false, caused_by_dup: false, directly_caused_by_dup: false }, prec::ASSIGNMENT, Some(value.clone()));
        if let Some((l, r, o)) = compound_parts(&nc) {
            if reg_local(&not_coerced(&l)) == Some(reg) {
                set_compound(&result, r, o);
            }
        }
        if is::duplicate(&value) {
            if let K::SetLocal { directly_caused_by_dup, .. } = &mut result.borrow_mut().k {
                *directly_caused_by_dup = true;
            }
        }
        self.handle_result(&value, &result, reg);
    }

    fn handle_set_slot(&mut self, slot: i32, obj: It, value: It) {
        let objnoreg = obj.clone();
        let mut obj = through_register(&obj);
        let (slot_name, real) = self.search_slot_name(slot, &obj, -1);
        if let Some(r) = real {
            obj = r;
        }
        if let Some(sn) = slot_name {
            if let Some(r) = reg_local(&value) {
                let sname = self.ctx().names().name(sn, &self.tr.fqn, true, true);
                if self.tr.local_reg_names.get(&r).is_some_and(|n| *n == sname) {
                    return;
                }
            }
        }
        let vnc = not_coerced(&value);
        let vtd = through_duplicate(&vnc);
        if is_inc_dec(&vtd) && is_inc_dec(&vnc) {
            let inc = is_inc(&vtd);
            if let Some(vv) = val(&vnc) {
                let inside = through_duplicate(&not_coerced(&through_register(&vv)));
                let gs = match &inside.borrow().k {
                    K::GetSlot { scope, slot_name: sn, .. } => Some((scope.clone(), *sn)),
                    _ => None,
                };
                if let Some((scope, sn)) = gs {
                    if same(&through_register(&scope), &through_register(&obj)) && sn == slot_name {
                        self.move_to();
                        let nt = number_type(&value);
                        if !self.empty() {
                            let top = through_duplicate(&not_coerced(&self.peek()));
                            let same_incdec = (if inc { is_inc(&top) } else { is_dec(&top) }) && opt_ptr_eq(&val(&top), &Some(inside.clone()));
                            if same(&top, &inside) {
                                let p = self.pop();
                                let d = val(&not_coerced_no_dup(&value)).unwrap();
                                self.check_dup(&p, &d);
                                self.push(self.post_inc(Some(self.ip), self.ls(), inside, nt, inc));
                            } else if same_incdec {
                                let p = self.pop();
                                self.check_dup(&p, &not_coerced_no_dup(&value));
                                self.push(self.pre_inc(Some(self.ip), self.ls(), inside, nt, inc));
                            } else {
                                self.add_out(self.post_inc(Some(self.ip), self.ls(), inside, nt, inc));
                            }
                        } else {
                            self.add_out(self.post_inc(Some(self.ip), self.ls(), inside, nt, inc));
                        }
                        return;
                    }
                }
            }
        }
        let slot_ty = if matches!(obj.borrow().k, K::NewActivation) { self.activation_slot_type(slot) } else { unbounded() };
        let result = self.mk(K::SetSlot { slot_name, scope: obj.clone(), slot_obj: objnoreg, idx: slot, set: SetInfo::default(), ty: slot_ty.clone() }, prec::ASSIGNMENT, Some(value.clone()));
        if let Some((l, r, o)) = compound_parts(&vnc) {
            if !has_side_effect(&obj) {
                let ln = not_coerced(&l);
                let gs = match &ln.borrow().k {
                    K::GetSlot { scope, idx, .. } => Some((scope.clone(), *idx)),
                    _ => None,
                };
                if let Some((scope, idx)) = gs {
                    if super::eq::eq(&obj, &through_duplicate(&scope)) && idx == slot {
                        set_compound(&result, r, o);
                    }
                }
            }
        }
        self.handle_result(&value, &result, -1);
    }

    /// `InstructionDefinition.handleSetProperty`.
    fn handle_set_property(&mut self, init: bool) {
        self.allow_swap();
        let mn = self.op(0) as usize;
        let value = self.pop();
        let fm = self.resolve_multiname(true, mn);
        let mut obj = self.pop();
        if self.set_property_patterns(&value, &fm, &obj, mn) {
            return;
        }
        if let Some(n) = fm_name(&fm) {
            let cmds = match &n.borrow().k {
                K::Comma { cmds } => Some(cmds.borrow().clone()),
                _ => None,
            };
            if let Some(c) = cmds {
                if c.len() == 2 {
                    if let (Some(r1), Some(r2)) = (reg_set(&c[0]), reg_local(&c[1])) {
                        if r1 == r2 {
                            let src = c[0].borrow().src;
                            if src.is_some() && self.usages(src) == 1 {
                                fm_set_name(&fm, val(&c[0]));
                            }
                        }
                    }
                }
            }
        }
        let otd = through_duplicate(&obj);
        let cons = match &otd.borrow().k {
            K::Construct { obj, args } => Some((obj.clone(), args.clone())),
            _ => None,
        };
        if let Some((cobj, cargs)) = cons {
            let at = match &cobj.borrow().k {
                K::ApplyType { params, .. } => Some(params.borrow().first().cloned()),
                _ => None,
            };
            let iv = match &cobj.borrow().k {
                K::InitVector { args, .. } => Some(args.clone()),
                _ => None,
            };
            if let Some(p0) = at {
                cargs.borrow_mut().clear();
                let (s, l) = X::src_ls(&otd);
                let ivi = mk(K::InitVector { subtype: p0.unwrap_or_else(unbounded), args: new_list(vec![value.clone()]) }, s, l, prec::PRIMARY, None);
                if let K::Construct { obj: o, .. } = &mut otd.borrow_mut().k {
                    *o = ivi;
                }
                if is::duplicate(&obj) && !self.empty() {
                    let top = self.peek();
                    if is::duplicate_source(&top) && same(&through_duplicate(&top), &otd) {
                        if let Some(last) = self.oback(1) {
                            if st_temp(&last).is_some() && st_temp(&last) == dup_src_temp(&top) {
                                self.orem_back(1);
                                self.move_to();
                            }
                        }
                        let p = self.pop();
                        self.push(val(&p).unwrap());
                    }
                }
                return;
            } else if let Some(ivargs) = iv {
                ivargs.borrow_mut().push(value.clone());
                if is::duplicate(&obj) && !self.empty() {
                    let top = self.peek();
                    if is::duplicate_source(&top) && same(&through_duplicate(&top), &otd) {
                        if let Some(last) = self.oback(1) {
                            if st_temp(&last).is_some() && st_temp(&last) == dup_src_temp(&top) {
                                self.orem_back(1);
                            }
                        }
                        let p = self.pop();
                        self.push(val(&p).unwrap());
                    }
                }
                return;
            }
        }
        obj = self.temp_findprop(obj);
        let (is_static, t, ct) = self.resolve_property_type(&obj, &fm);
        let k = if init {
            K::InitProperty { obj: obj.clone(), prop: fm.clone(), set: SetInfo::default(), ty: t.clone(), call_ty: ct, is_static }
        } else {
            K::SetProperty { obj: obj.clone(), prop: fm.clone(), set: SetInfo::default(), ty: t.clone(), call_ty: ct, is_static }
        };
        let result = self.mk(k, prec::ASSIGNMENT, Some(value.clone()));
        self.handle_compound(&obj, &fm, &value, &result);
        self.handle_result(&value, &result, -1);
    }

    /// `SetPropertyIns.handleCompound`.
    fn handle_compound(&self, obj: &It, fm: &It, value: &It, result: &It) {
        if let (Some(loc_val), Some(loc_name)) = (reg_local(value), fm_name(fm).and_then(|n| reg_local(&n))) {
            if self.olen() > 2 {
                let (l1, l2) = (self.oback(1).unwrap(), self.oback(2).unwrap());
                if reg_set(&l1) == Some(loc_val) && reg_set(&l2) == Some(loc_name) {
                    if let Some((l, r, o)) = compound_parts(&val(&l1).unwrap()) {
                        let ln = not_coerced(&l);
                        if is_getprop(&ln) && compare_same(&gp_prop(&ln), fm) && super::eq::eq(&gp_obj(&ln), obj) {
                            fm_set_name(fm, val(&l2));
                            set_compound(result, r, o);
                            self.orem_back(2);
                            self.orem_back(1);
                        }
                    }
                }
            }
        }
        if let Some((l, r, o)) = compound_parts(&not_coerced(value)) {
            if !has_side_effect(obj) && !has_side_effect(fm) {
                let lk = l.borrow().k.clone();
                if let K::GetLex { mn: lmn, .. } = lk {
                    let m = fm_index(fm).unwrap_or(0);
                    let cs = &self.ctx().abc.constants.multinames;
                    if is::find_property(&through_duplicate(obj)) && cs.get(m).is_some() && cs.get(m) == cs.get(lmn) {
                        set_compound(result, r, o);
                    }
                } else if let K::GetProperty { obj: pobj, prop: pprop, .. } = lk {
                    if super::eq::eq(&through_duplicate(obj), &through_duplicate(&pobj)) && super::eq::eq(fm, &pprop) {
                        set_compound(result, r, o);
                        if let Some(d) = dup_temp(&pobj) {
                            if self.oback(1).is_some_and(|x| st_temp(&x) == Some(d)) {
                                self.orem_back(1);
                                self.move_to();
                            }
                        }
                        let sobj = match &result.borrow().k {
                            K::SetProperty { obj, .. } => Some(obj.clone()),
                            _ => None,
                        };
                        if let Some(so) = sobj {
                            if is::duplicate_source(&so) {
                                let v = val(&so).unwrap();
                                if let K::SetProperty { obj, .. } = &mut result.borrow_mut().k {
                                    *obj = v;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The increment/decrement rewrites at the start of `handleSetProperty`. True when handled.
    fn set_property_patterns(&self, value: &It, fm: &It, obj: &It, mn: usize) -> bool {
        let ip = self.ip;
        let _ = ip;
        // value = ++/-- of obj.prop through a dup
        if is_inc_dec(value) {
            let inc = is_inc(value);
            let vv = val(value).unwrap();
            if is_getprop(&vv) {
                let go = gp_obj(&vv);
                if let Some(d) = dup_temp(&go) {
                    if is::duplicate_source(obj) && same(&through_duplicate(&go), &through_duplicate(obj)) {
                        if let Some(st) = self.oback(1).filter(|l| st_temp(l) == Some(d)) {
                            self.orem_back(1);
                            gp_set_obj(&vv, val(&st).unwrap());
                            let (s, l) = X::src_ls(value);
                            self.add_out(self.pre_inc(s, l, vv.clone(), return_type(&vv), inc));
                            return true;
                        }
                    }
                }
            }
            if is_convert(&vv) {
                let gp = val(&vv).unwrap();
                if is_getprop(&gp) {
                    let go = gp_obj(&gp);
                    if let Some(d) = dup_temp(&go) {
                        if is::duplicate_source(obj) && same(&through_duplicate(&go), &through_duplicate(obj)) {
                            if let Some(st) = self.oback(1).filter(|l| st_temp(l) == Some(d)) {
                                self.orem_back(1);
                                gp_set_obj(&gp, val(&st).unwrap());
                                let (s, l) = X::src_ls(value);
                                self.add_out(self.post_inc(s, l, gp, ty("Number"), inc));
                                return true;
                            }
                        }
                    }
                }
            }
        }
        if let Some(value_reg) = reg_local(value) {
            let name_reg = fm_name(fm).and_then(|n| reg_local(&n));
            if let Some(obj_reg) = reg_local(obj) {
                if self.loc_patterns(value_reg, name_reg, obj_reg, fm) {
                    return true;
                }
            }
        }
        if let Some(d) = dup_temp(value) {
            if !self.empty() {
                let top = self.peek();
                if dup_src_temp(&top) == Some(d) {
                    if let Some(st) = self.oback(1).filter(|l| st_temp(l) == Some(d)) {
                        let stv = val(&st).unwrap();
                        if is_inc_dec(&stv) {
                            let inc = is_inc(&stv);
                            let inner = val(&stv).unwrap();
                            let ok = match &inner.borrow().k {
                                K::GetLex { mn: lmn, .. } => {
                                    let cs = &self.ctx().abc.constants.multinames;
                                    cs.get(mn).is_some() && cs.get(mn) == cs.get(*lmn)
                                }
                                K::GetProperty { prop, .. } => fm_index(prop).is_some() && compare_same(prop, fm),
                                _ => false,
                            };
                            if ok {
                                self.orem_back(1);
                                self.pop();
                                self.move_to();
                                let (s, l) = X::src_ls(&stv);
                                self.push(self.pre_inc(s, l, inner, ty("Number"), inc));
                                return true;
                            }
                        }
                    }
                }
            }
        }
        if is_inc_dec(value) {
            let inc = is_inc(value);
            let vv = val(value).unwrap();
            if is_getprop(&vv) && compare_same(&gp_prop(&vv), fm) {
                let (s, l) = X::src_ls(value);
                self.add_out(self.pre_inc(s, l, vv, ty("Number"), inc));
                return true;
            }
            if let Some(d) = dup_temp(&vv) {
                if let Some(pushed) = self.oback(1).filter(|p| is::push(p)) {
                    let ds = val(&pushed).unwrap();
                    if dup_src_temp(&ds) == Some(d) {
                        if let Some(st) = self.oback(2).filter(|s| st_temp(s) == Some(d)) {
                            let stv = val(&st).unwrap();
                            if is_convert(&stv) {
                                let inner = val(&stv).unwrap();
                                let ok = match &inner.borrow().k {
                                    K::GetLex { mn: lmn, .. } => {
                                        let cs = &self.ctx().abc.constants.multinames;
                                        cs.get(mn).is_some() && cs.get(mn) == cs.get(*lmn)
                                    }
                                    K::GetProperty { prop, .. } => fm_index(prop).is_some() && compare_same(prop, fm),
                                    _ => false,
                                };
                                if ok {
                                    self.orem_back(1);
                                    self.orem_back(1);
                                    self.move_to();
                                    let (s, l) = X::src_ls(value);
                                    self.push(self.post_inc(s, l, inner, ty("Number"), inc));
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            if is_convert(&vv) {
                let gp = val(&vv).unwrap();
                if is_getprop(&gp) && compare_same(&gp_prop(&gp), fm) {
                    let (s, l) = X::src_ls(value);
                    self.add_out(self.post_inc(s, l, gp, ty("Number"), inc));
                    return true;
                }
            }
        }
        false
    }

    /// The register-based `obj[name] += 1` patterns of `handleSetProperty`.
    fn loc_patterns(&self, value_reg: i32, name_reg: Option<i32>, obj_reg: i32, fm: &It) -> bool {
        // (getProp, setLocObj, setLocName) for an inc/dec of a getproperty whose object and name are setlocals
        let named = |slv_val: &It| -> Option<(It, It, It)> {
            if !is_getprop(slv_val) {
                return None;
            }
            let gobj = gp_obj(slv_val);
            reg_set(&gobj)?;
            let prop = gp_prop(slv_val);
            let n = fm_name(&prop)?;
            reg_set(&n)?;
            Some((slv_val.clone(), gobj, n))
        };
        let apply_named = |gp: &It, set_loc_obj: &It, set_loc_name: &It| {
            gp_set_obj(gp, val(set_loc_obj).unwrap());
            fm_set_name(&gp_prop(gp), val(set_loc_name));
        };
        let regs_match = |slv: &It, slo: &It, sln: &It| reg_set(slv) == Some(value_reg) && reg_set(slo) == Some(obj_reg) && name_reg.is_some() && reg_set(sln) == name_reg;
        // 1: stack top setlocal ++obj[name]
        if !self.empty() {
            let slv = self.peek();
            if reg_set(&slv).is_some() {
                let v = val(&slv).unwrap();
                if is_inc_dec(&v) {
                    let inc = is_inc(&v);
                    if let Some((gp, slo, sln)) = named(&val(&v).unwrap()) {
                        if regs_match(&slv, &slo, &sln) {
                            apply_named(&gp, &slo, &sln);
                            self.pop();
                            let (s, l) = X::src_ls(&v);
                            self.push(self.pre_inc(s, l, gp, ty("Number"), inc));
                            return true;
                        }
                    }
                }
            }
        }
        // 2: output last
        if let Some(slv) = self.oback(1).filter(|x| reg_set(x).is_some()) {
            let v = val(&slv).unwrap();
            if is_inc_dec(&v) {
                let inc = is_inc(&v);
                if let Some((gp, slo, sln)) = named(&val(&v).unwrap()) {
                    if regs_match(&slv, &slo, &sln) {
                        apply_named(&gp, &slo, &sln);
                        self.orem_back(1);
                        let (s, l) = X::src_ls(&v);
                        self.add_out(self.pre_inc(s, l, gp, ty("Number"), inc));
                        return true;
                    }
                }
            }
        }
        // 3: dup of convert(getprop) named
        if let Some(slv) = self.oback(1).filter(|x| reg_set(x).is_some()) {
            let v = val(&slv).unwrap();
            if is_inc_dec(&v) {
                let inc = is_inc(&v);
                let vv = val(&v).unwrap();
                if let Some(d) = dup_temp(&vv) {
                    if let Some(st) = self.dup_chain(d) {
                        let stv = val(&st).unwrap();
                        if is_convert(&stv) {
                            if let Some((gp, slo, sln)) = named(&val(&stv).unwrap()) {
                                if regs_match(&slv, &slo, &sln) {
                                    apply_named(&gp, &slo, &sln);
                                    self.orem_back(1);
                                    self.orem_back(1);
                                    self.orem_back(1);
                                    self.move_to();
                                    let (s, l) = X::src_ls(&v);
                                    self.push(self.post_inc(s, l, gp, ty("Number"), inc));
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        // 4: convert(getprop) named, standalone
        if let Some(slv) = self.oback(1).filter(|x| reg_set(x).is_some()) {
            let v = val(&slv).unwrap();
            if is_inc_dec(&v) {
                let inc = is_inc(&v);
                let vv = val(&v).unwrap();
                if is_convert(&vv) {
                    if let Some((gp, slo, sln)) = named(&val(&vv).unwrap()) {
                        if regs_match(&slv, &slo, &sln) {
                            apply_named(&gp, &slo, &sln);
                            self.orem_back(1);
                            self.move_to();
                            let (s, l) = X::src_ls(&v);
                            self.add_out(self.post_inc(s, l, gp, ty("Number"), inc));
                            return true;
                        }
                    }
                }
            }
        }
        // getprop with a setlocal object and the same multiname
        let same_mn = |slv_val: &It| -> Option<(It, It)> {
            if !is_getprop(slv_val) {
                return None;
            }
            let gobj = gp_obj(slv_val);
            reg_set(&gobj)?;
            let prop = gp_prop(slv_val);
            fm_index(&prop)?;
            if !compare_same(&prop, fm) {
                return None;
            }
            Some((slv_val.clone(), gobj))
        };
        // 5: stack top
        if !self.empty() {
            let slv = self.peek();
            if reg_set(&slv).is_some() {
                let v = val(&slv).unwrap();
                if is_inc_dec(&v) {
                    let inc = is_inc(&v);
                    let vv = val(&v).unwrap();
                    if is_getprop(&vv) && reg_set(&gp_obj(&vv)).is_some() {
                        if let Some((gp, slo)) = same_mn(&vv) {
                            if reg_set(&slv) == Some(value_reg) && reg_set(&slo) == Some(obj_reg) {
                                gp_set_obj(&gp, val(&slo).unwrap());
                                self.pop();
                                let (s, l) = X::src_ls(&v);
                                self.push(self.pre_inc(s, l, gp, ty("Number"), inc));
                                return true;
                            }
                        }
                    }
                }
            }
        }
        // 6: output last
        if let Some(slv) = self.oback(1).filter(|x| reg_set(x).is_some()) {
            let v = val(&slv).unwrap();
            if is_inc_dec(&v) {
                let inc = is_inc(&v);
                let vv = val(&v).unwrap();
                if is_getprop(&vv) && reg_set(&gp_obj(&vv)).is_some() {
                    if let Some((gp, slo)) = same_mn(&vv) {
                        if reg_set(&slv) == Some(value_reg) && reg_set(&slo) == Some(obj_reg) {
                            gp_set_obj(&gp, val(&slo).unwrap());
                            self.orem_back(1);
                            self.move_to();
                            let (s, l) = X::src_ls(&v);
                            self.add_out(self.pre_inc(s, l, gp, ty("Number"), inc));
                            return true;
                        }
                    }
                }
            }
        }
        // 7 and 8: dup chains of convert(getprop) or getprop
        for with_convert in [true, false] {
            if let Some(slv) = self.oback(1).filter(|x| reg_set(x).is_some()) {
                let v = val(&slv).unwrap();
                if is_inc_dec(&v) {
                    let inc = is_inc(&v);
                    let vv = val(&v).unwrap();
                    if let Some(d) = dup_temp(&vv) {
                        if let Some(st) = self.dup_chain(d) {
                            let mut stv = val(&st).unwrap();
                            if with_convert {
                                if !is_convert(&stv) {
                                    continue;
                                }
                                stv = val(&stv).unwrap();
                            }
                            if is_getprop(&stv) && reg_set(&gp_obj(&stv)).is_some() {
                                if let Some((gp, slo)) = same_mn(&stv) {
                                    if reg_set(&slo) == Some(obj_reg) && reg_set(&slv) == Some(value_reg) {
                                        gp_set_obj(&gp, val(&slo).unwrap());
                                        self.orem_back(1);
                                        self.orem_back(1);
                                        self.orem_back(1);
                                        self.move_to();
                                        let (s, l) = X::src_ls(&v);
                                        self.push(self.post_inc(s, l, gp, ty("Number"), inc));
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Output ends with `SetTemporary(d), Push(DuplicateSource(d)), <last>`: the SetTemporary.
    fn dup_chain(&self, d: i32) -> Option<It> {
        let p = self.oback(2)?;
        if !is::push(&p) || dup_src_temp(&val(&p)?) != Some(d) {
            return None;
        }
        self.oback(3).filter(|s| st_temp(s) == Some(d))
    }
}

/// `Rc` identity helper kept for symmetry with Java's `==` on items.
#[allow(dead_code)]
fn ptr(it: &It) -> *const std::cell::RefCell<Item> {
    Rc::as_ptr(it)
}
