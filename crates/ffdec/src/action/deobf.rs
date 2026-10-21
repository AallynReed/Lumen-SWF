//! FFDec's automatic AS1/2 deobfuscation (`ActionDeobfuscator`): conditions and fake functions
//! that evaluate to constants are run on a small interpreter and replaced by jumps.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Action, ActionList, Charset, Op, Value};
use crate::abc::decomp::ecma::{self, Ty, V};

/// `as12DeobfuscatorExecutionLimit`.
const EXECUTION_LIMIT: usize = 10000;

/// A Java exception in FFDec: deobfuscation stops and keeps what it had stored.
struct Abort;

type R<T> = Result<T, Abort>;

/// `ActionItem`: an action in the linked list, with its jump target and container ends.
struct Item {
    a: Action,
    prev: usize,
    next: usize,
    jump: Option<usize>,
    jumps_here: Vec<usize>,
    last_of: Vec<usize>,
    lasts: Option<Vec<Option<usize>>>,
    reachable: u8,
    excluded: bool,
    /// The identity of a ConstantPool's list, which pushes refer to.
    pool: Option<Arc<Vec<String>>>,
}

/// `FastActionList`, over an arena; removed items keep their links as in Java.
struct Fast {
    it: Vec<Item>,
    first: Option<usize>,
    size: usize,
    cs: Charset,
}

fn new_item(a: Action) -> Item {
    let pool = match &a.op {
        Op::ConstantPool(c) => Some(Arc::new(c.clone())),
        _ => None,
    };
    Item { a, prev: 0, next: 0, jump: None, jumps_here: Vec::new(), last_of: Vec::new(), lasts: None, reachable: 0, excluded: false, pool }
}

fn get_by_address(actions: &[Action], addr: u64) -> Option<usize> {
    actions.binary_search_by_key(&addr, |a| a.address).ok()
}

impl Fast {
    fn new(list: &ActionList) -> Fast {
        let mut f = Fast { it: Vec::new(), first: None, size: 0, cs: list.charset };
        for a in &list.actions {
            let n = f.alloc(a.clone());
            f.insert_after(None, n);
        }
        let acts = &list.actions;
        for i in 0..acts.len() {
            if let Some(sizes) = acts[i].container_sizes() {
                let mut end = acts[i].address + acts[i].bytes_len(f.cs);
                let mut lasts = Vec::new();
                for s in sizes {
                    end += s;
                    let near = match acts.binary_search_by_key(&(end as i64 - 1), |a| a.address as i64) {
                        Ok(p) => Some(p),
                        Err(p) => p.checked_sub(1),
                    };
                    lasts.push(near);
                }
                f.set_lasts(i, lasts);
            }
        }
        for i in 0..acts.len() {
            let a = &acts[i];
            if let Some(t) = a.target() {
                let target = get_by_address(acts, t);
                f.set_jump(i, target);
            } else if let Some(store) = a.store_size() {
                let mut cur = Some(i);
                for _ in 0..=store {
                    let Some(c) = cur else { break };
                    cur = get_by_address(acts, acts[c].address + acts[c].total_len());
                }
                f.set_jump(i, cur);
            }
        }
        f
    }

    fn alloc(&mut self, a: Action) -> usize {
        self.it.push(new_item(a));
        self.it.len() - 1
    }

    fn last(&self) -> Option<usize> {
        self.first.map(|f| self.it[f].prev)
    }

    fn is_empty(&self) -> bool {
        self.size == 0
    }

    // ---- ActionItem ----

    fn remove_jump(&mut self, i: usize) {
        if let Some(t) = self.it[i].jump.take() {
            self.it[t].jumps_here.retain(|&x| x != i);
        }
    }

    fn set_jump(&mut self, i: usize, target: Option<usize>) {
        self.remove_jump(i);
        let Some(t) = target else { return };
        if !self.it[t].jumps_here.contains(&i) {
            self.it[t].jumps_here.push(i);
        }
        self.it[i].jump = Some(t);
    }

    fn remove_lasts(&mut self, i: usize) {
        let Some(lasts) = self.it[i].lasts.take() else { return };
        for l in lasts.into_iter().flatten() {
            self.it[l].last_of.retain(|&x| x != i);
        }
    }

    fn replace_last(&mut self, i: usize, old: usize, new: usize) {
        let Some(mut lasts) = self.it[i].lasts.take() else { return };
        for l in lasts.iter_mut() {
            if *l == Some(old) {
                *l = Some(new);
                self.it[old].last_of.retain(|&x| x != i);
                if !self.it[new].last_of.contains(&i) {
                    self.it[new].last_of.push(i);
                }
            }
        }
        self.it[i].lasts = Some(lasts);
    }

    fn set_lasts(&mut self, i: usize, lasts: Vec<Option<usize>>) {
        self.remove_lasts(i);
        for &l in lasts.iter().flatten() {
            if !self.it[l].last_of.contains(&i) {
                self.it[l].last_of.push(i);
            }
        }
        self.it[i].lasts = Some(lasts);
    }

    fn is_jump_target(&self, i: usize) -> bool {
        !self.it[i].jumps_here.is_empty()
    }

    fn is_container_last(&self, i: usize) -> bool {
        !self.it[i].last_of.is_empty()
    }

    fn first_last(&self, i: usize) -> R<usize> {
        self.it[i].lasts.as_ref().and_then(|l| l.first().copied().flatten()).ok_or(Abort)
    }

    // ---- FastActionList ----

    fn insert_before(&mut self, item: usize, new: usize) {
        let p = self.it[item].prev;
        self.insert_after(Some(p), new);
        if Some(item) == self.first {
            self.first = Some(new);
        }
    }

    fn insert_after(&mut self, item: Option<usize>, new: usize) {
        match (item, self.first) {
            (None, None) => {
                self.first = Some(new);
                self.it[new].next = new;
                self.it[new].prev = new;
            }
            (item, Some(first)) => {
                let item = item.unwrap_or(self.it[first].prev);
                let old_next = self.it[item].next;
                self.it[new].prev = item;
                self.it[new].next = old_next;
                self.it[item].next = new;
                self.it[old_next].prev = new;
            }
            (Some(item), None) => {
                let old_next = self.it[item].next;
                self.it[new].prev = item;
                self.it[new].next = old_next;
                self.it[item].next = new;
                self.it[old_next].prev = new;
            }
        }
        self.size += 1;
    }

    /// `removeItem`: the next item, or None when the list became empty.
    fn remove(&mut self, item: usize) -> Option<usize> {
        let mut next = None;
        let (p, n) = (self.it[item].prev, self.it[item].next);
        if Some(item) == self.first {
            if n == item {
                self.first = None;
            } else {
                next = Some(n);
                self.first = Some(n);
                self.it[n].prev = p;
                self.it[p].next = n;
            }
        } else {
            next = Some(n);
            self.it[p].next = n;
            self.it[n].prev = p;
        }
        self.size -= 1;
        self.remove_jump(item);
        self.remove_lasts(item);
        for j in self.it[item].jumps_here.clone() {
            self.set_jump(j, Some(n));
        }
        for j in self.it[item].last_of.clone() {
            self.replace_last(j, item, p);
        }
        next
    }

    fn replace_jump_targets(&mut self, target: usize, new: usize) {
        for j in self.it[target].jumps_here.clone() {
            self.set_jump(j, Some(new));
        }
    }

    fn order(&self) -> Vec<usize> {
        let mut v = Vec::with_capacity(self.size);
        let Some(first) = self.first else { return v };
        let mut i = first;
        loop {
            v.push(i);
            i = self.it[i].next;
            if i == first {
                break;
            }
        }
        v
    }

    fn expand_pushes(&mut self) {
        let Some(first) = self.first else { return };
        let mut item = first;
        loop {
            if let Op::Push { values, pool } = &self.it[item].a.op {
                if values.len() > 1 {
                    let (values, pool) = (values.clone(), pool.clone());
                    let orig = item;
                    for v in &values[1..] {
                        let n = self.alloc(Action::new(0x96, Op::Push { values: vec![v.clone()], pool: pool.clone() }));
                        self.insert_after(Some(item), n);
                        item = self.it[item].next;
                    }
                    if let Op::Push { values: vs, .. } = &mut self.it[orig].a.op {
                        vs.truncate(1);
                    }
                }
            }
            item = self.it[item].next;
            if Some(item) == self.first {
                break;
            }
        }
    }

    fn remove_unknown(&mut self) {
        let Some(mut item) = self.first else { return };
        loop {
            if matches!(self.it[item].a.op, Op::Unknown(_)) {
                match self.remove(item) {
                    Some(n) => item = n,
                    None => return,
                }
                if Some(item) == self.first {
                    break;
                }
                continue;
            }
            item = self.it[item].next;
            if Some(item) == self.first {
                break;
            }
        }
    }

    fn remove_zero_jumps(&mut self) {
        let Some(mut item) = self.first else { return };
        loop {
            if self.it[item].a.is_jump() && self.it[item].jump == Some(self.it[item].next) && self.it[item].jump != self.first {
                match self.remove(item) {
                    Some(n) => item = n,
                    None => return,
                }
                if Some(item) == self.first {
                    break;
                }
                continue;
            }
            item = self.it[item].next;
            if Some(item) == self.first {
                break;
            }
        }
    }

    fn remove_unreachable(&mut self) {
        let Some(mut item) = self.first else { return };
        self.update_reachable(None, None);
        loop {
            if self.it[item].reachable == 0 {
                match self.remove(item) {
                    Some(n) => item = n,
                    None => return,
                }
                if Some(item) == self.first {
                    break;
                }
                continue;
            }
            item = self.it[item].next;
            if Some(item) == self.first {
                break;
            }
        }
    }

    fn remove_included(&mut self) -> R<()> {
        let Some(mut item) = self.first else { return Ok(()) };
        loop {
            if !self.it[item].excluded {
                item = self.remove(item).ok_or(Abort)?;
                continue;
            }
            item = self.it[item].next;
            if Some(item) == self.first {
                break;
            }
        }
        Ok(())
    }

    fn unreachable_count(&mut self, jump: usize, target: usize) -> usize {
        if self.first.is_none() {
            return 0;
        }
        self.update_reachable(Some(jump), Some(target));
        self.it[jump].reachable = 0;
        self.order().iter().filter(|&&i| self.it[i].reachable == 0).count()
    }

    fn set_excluded(&mut self, v: bool) {
        for i in self.order() {
            self.it[i].excluded = v;
        }
    }

    fn update_reachable(&mut self, jump: Option<usize>, target: Option<usize>) {
        let Some(first) = self.first else { return };
        for i in self.order() {
            self.it[i].reachable = 0;
        }
        self.it[first].reachable = 1;
        let mut first2 = first;
        let mut modified = true;
        while modified {
            modified = false;
            let mut item = first2;
            loop {
                let next = self.it[item].next;
                if self.it[item].reachable == 1 {
                    self.it[item].reachable = 2;
                    modified = true;
                    if item == first2 {
                        first2 = next;
                    }
                    if Some(item) == jump {
                        let t = target.unwrap();
                        if self.it[t].reachable == 0 {
                            self.it[t].reachable = 1;
                        }
                    } else {
                        let a = &self.it[item].a;
                        let (falls, container) = (!a.is_exit() && !a.is_jump(), a.container_sizes().is_some());
                        if falls && self.it[next].reachable == 0 {
                            self.it[next].reachable = 1;
                        }
                        if container {
                            for l in self.it[item].lasts.clone().unwrap_or_default().into_iter().flatten() {
                                let n = self.it[l].next;
                                if self.it[n].reachable == 0 {
                                    self.it[n].reachable = 1;
                                }
                            }
                        }
                        if let Some(t) = self.it[item].jump {
                            if self.it[t].reachable == 0 {
                                self.it[t].reachable = 1;
                            }
                        }
                    }
                }
                item = next;
                if item == first {
                    break;
                }
            }
        }
    }

    /// `updateActions`: re-addresses the list and writes jumps, stores and container sizes.
    fn to_actions(&mut self) -> R<Vec<Action>> {
        let order = self.order();
        let Some(&first) = order.first() else { return Ok(Vec::new()) };
        let mut address = self.it[first].a.address;
        for &i in &order {
            let a = &mut self.it[i].a;
            a.address = address;
            a.update_length(self.cs);
            address += a.total_len();
        }
        let end = self.it[*order.last().unwrap()].a.address;
        for &i in &order {
            if self.it[i].a.target().is_some() {
                let base = self.it[i].a.address as i64 + self.it[i].a.total_len() as i64;
                let to = self.it[i].jump.map_or(end as i64, |t| self.it[t].a.address as i64);
                self.it[i].a.set_offset((to - base) as i32);
            }
        }
        for (k, &i) in order.iter().enumerate() {
            if self.it[i].a.store_size().is_some() {
                let stop = self.it[i].jump;
                let n = order[k + 1..].iter().take_while(|&&x| Some(x) != stop).count();
                self.it[i].a.set_store_size(n);
            }
        }
        for &i in &order {
            if self.it[i].a.container_sizes().is_some() {
                let lasts = self.it[i].lasts.clone().unwrap_or_default();
                let mut start = self.it[i].a.address + self.it[i].a.bytes_len(self.cs);
                for (j, l) in lasts.iter().enumerate() {
                    let l = l.ok_or(Abort)?;
                    let la = &self.it[l].a;
                    let len = (la.address + la.total_len()) as i64 - start as i64;
                    self.it[i].a.set_container_size(j, len as u64);
                    start = (start as i64 + len) as u64;
                }
            }
        }
        Ok(order.iter().map(|&i| self.it[i].a.clone()).collect())
    }
}

/// `FastActionListIterator`.
struct Iter {
    item: Option<usize>,
    started: bool,
}

impl Iter {
    fn new(f: &Fast) -> Iter {
        Iter { item: f.first, started: false }
    }

    fn has_next(&self, f: &Fast) -> bool {
        self.item.is_some() && (!self.started || self.item != f.first)
    }

    fn next(&mut self, f: &Fast) -> usize {
        let r = self.item.unwrap();
        self.item = Some(f.it[r].next);
        self.started = true;
        r
    }

    fn prev(&mut self, f: &Fast) {
        let p = f.it[self.item.unwrap()].prev;
        self.item = Some(p);
        if self.item == f.first {
            self.started = false;
        }
    }

    fn set_current(&mut self, f: &Fast, item: usize) {
        self.item = Some(item);
        if self.item == f.first {
            self.started = false;
        }
    }

    fn remove(&mut self, f: &mut Fast) {
        let p = f.it[self.item.unwrap()].prev;
        self.item = f.remove(p);
    }

    fn add(&mut self, f: &mut Fast, new: usize) {
        let p = f.it[self.item.unwrap()].prev;
        f.insert_after(Some(p), new);
        self.item = Some(f.it[new].next);
    }

    fn add_before(&mut self, f: &mut Fast, new: usize) {
        let p = f.it[self.item.unwrap()].prev;
        f.insert_before(p, new);
    }

    fn peek(&self, f: &Fast, n: usize) -> usize {
        let mut i = self.item.unwrap();
        for _ in 0..n {
            i = f.it[i].next;
        }
        i
    }
}

// ---- the interpreter ----

/// `LocalDataArea` with a `FixItemCounterStack`.
struct Lda {
    pool: Option<Arc<Vec<String>>>,
    stack: Vec<V>,
    fix: usize,
    vars: Vec<(String, V)>,
    regs: HashMap<u8, V>,
    jump: bool,
    ret: Option<V>,
    check: bool,
}

impl Lda {
    fn new() -> Lda {
        Lda { pool: None, stack: Vec::new(), fix: usize::MAX, vars: Vec::new(), regs: HashMap::new(), jump: false, ret: None, check: true }
    }

    fn clear(&mut self) {
        self.pool = None;
        self.stack.clear();
        self.fix = usize::MAX;
        self.vars.clear();
        self.regs.clear();
        self.jump = false;
        self.ret = None;
    }

    fn all_fixed(&self) -> bool {
        self.stack.len() <= self.fix
    }

    fn raw_pop(&mut self) -> V {
        let v = self.stack.pop().unwrap();
        self.fix = self.fix.min(self.stack.len());
        v
    }

    fn pop(&mut self) -> V {
        if !self.check && self.stack.is_empty() {
            return V::Undefined;
        }
        self.raw_pop()
    }

    fn peek(&mut self) -> V {
        if !self.check && self.stack.is_empty() {
            self.stack.push(V::Undefined);
            return V::Undefined;
        }
        self.stack.last().unwrap().clone()
    }

    fn is_empty(&self) -> bool {
        self.check && self.stack.is_empty()
    }

    fn has(&self, n: usize) -> bool {
        !self.check || self.stack.len() >= n
    }

    fn num(&mut self) -> f64 {
        let v = self.pop();
        ecma::to_number_as2(&v)
    }

    fn var(&self, name: &str) -> Option<&V> {
        self.vars.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    fn set_var(&mut self, name: String, v: V) {
        match self.vars.iter_mut().find(|(k, _)| *k == name) {
            Some(e) => e.1 = v,
            None => self.vars.push((name, v)),
        }
    }
}

fn value_to_v(v: &Value, pool: Option<&Arc<Vec<String>>>, regs: &HashMap<u8, V>) -> V {
    match v {
        Value::Str(s) => V::Str(s.clone()),
        Value::Float(f) => V::Float(*f),
        Value::Null => V::Null,
        Value::Undefined => V::Undefined,
        Value::Register(r) => regs.get(r).cloned().unwrap_or(V::Undefined),
        Value::Bool(b) => V::Bool(*b),
        Value::Double(d) => V::Double(*d),
        Value::Int(i) => match i32::try_from(*i) {
            Ok(i) => V::Int(i),
            Err(_) => V::Long(*i),
        },
        Value::Constant(i) => match pool.filter(|p| (*i as usize) < p.len()) {
            Some(p) => V::Str(p[*i as usize].clone()),
            None => V::Str(format!("constant{i}")),
        },
    }
}

fn v_to_value(v: &V) -> Value {
    match v {
        V::Str(s) => Value::Str(s.clone()),
        V::Float(f) => Value::Float(*f),
        V::Null | V::None => Value::Null,
        V::Undefined | V::Obj(_) => Value::Undefined,
        V::Bool(b) => Value::Bool(*b),
        V::Double(d) => Value::Double(*d),
        V::Int(i) => Value::Int(*i as i64),
        V::Long(l) => Value::Int(*l),
    }
}

/// `ActionPush.isValidValue`.
fn valid_value(v: &V) -> bool {
    match v {
        V::Str(s) => !s.contains('\0'),
        V::Long(l) => (-0x8000_0000..=0x7fff_ffff).contains(l),
        _ => true,
    }
}

fn float_point(v: &V) -> f64 {
    match v {
        V::Double(d) => *d,
        V::Int(i) => *i as f64,
        V::Long(l) => *l as f64,
        V::Null | V::Undefined => f64::NAN,
        V::Bool(b) => *b as i32 as f64,
        V::Str(s) => ecma::java_parse_double(s).unwrap_or(f64::NAN),
        _ => 0.0,
    }
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn ascii_to_char(v: &V) -> V {
    let c = ecma::to_int32(v) as u16;
    if c == 0 {
        return V::Str(String::new());
    }
    V::Str(String::from_utf16_lossy(&[c]))
}

fn str_len(v: &V) -> V {
    V::Double(utf16(&ecma::to_string(v)).len() as f64)
}

/// `Action.execute` for the actions the deobfuscator runs; others only set the return value.
fn execute(lda: &mut Lda, a: &Action) -> bool {
    let two = |lda: &mut Lda| -> Option<(V, V)> {
        if !lda.has(2) {
            return None;
        }
        let r = lda.pop();
        let l = lda.pop();
        Some((r, l))
    };
    let two_num = |lda: &mut Lda| -> Option<(f64, f64)> {
        if !lda.has(2) {
            return None;
        }
        let r = lda.num();
        let l = lda.num();
        Some((r, l))
    };
    macro_rules! one {
        ($f:expr) => {{
            if lda.is_empty() {
                return false;
            }
            let v = lda.pop();
            let r = $f(&v);
            lda.stack.push(r);
            true
        }};
    }
    macro_rules! bin {
        ($f:expr) => {{
            let Some((r, l)) = two(lda) else { return false };
            let res = $f(&l, &r);
            lda.stack.push(res);
            true
        }};
    }
    macro_rules! bin_num {
        ($f:expr) => {{
            let Some((r, l)) = two_num(lda) else { return false };
            let res = $f(l, r);
            lda.stack.push(res);
            true
        }};
    }
    match &a.op {
        Op::Push { values, pool } => {
            let p = lda.pool.clone().or_else(|| pool.clone());
            for v in values {
                let x = value_to_v(v, p.as_ref(), &lda.regs);
                lda.stack.push(x);
            }
            true
        }
        Op::Jump { .. } => {
            lda.jump = true;
            true
        }
        Op::If { .. } => {
            if lda.is_empty() {
                return false;
            }
            let v = lda.pop();
            if ecma::to_boolean(&v) {
                lda.jump = true;
            }
            true
        }
        Op::ConstantPool(c) => {
            lda.pool = Some(Arc::new(c.clone()));
            true
        }
        Op::StoreRegister(r) => {
            if !lda.has(1) {
                return false;
            }
            let v = lda.pop();
            lda.regs.insert(*r, v);
            true
        }
        Op::Simple => match a.code {
            0x33 | 0x37 => one!(ascii_to_char),
            0x32 => one!(|v: &V| {
                let s = utf16(&ecma::java_to_string(v));
                V::Int(s.first().copied().unwrap_or(0) as i32)
            }),
            0x51 => {
                if lda.is_empty() {
                    return false;
                }
                let n = lda.num();
                lda.stack.push(V::Double(n - 1.0));
                true
            }
            0x50 => {
                if lda.is_empty() {
                    return false;
                }
                let n = lda.num();
                lda.stack.push(V::Double(n + 1.0));
                true
            }
            0x12 => one!(|v: &V| V::Bool(!ecma::to_boolean(v))),
            0x18 => one!(|v: &V| V::Long(ecma::to_int32(v) as i64)),
            0x4a => {
                if lda.is_empty() {
                    return false;
                }
                let n = lda.num();
                lda.stack.push(V::Double(n));
                true
            }
            0x4b => one!(|v: &V| V::Str(ecma::to_string(v))),
            0x44 => one!(|v: &V| V::Str(
                match ecma::ty(v) {
                    Ty::Str => "string",
                    Ty::Boolean => "boolean",
                    Ty::Num => "number",
                    Ty::Undefined => "undefined",
                    Ty::Null => "null",
                    _ => "object",
                }
                .into()
            )),
            0x14 | 0x31 => one!(str_len),
            0x10 => bin!(|l: &V, r: &V| V::Bool(ecma::to_boolean(l) && ecma::to_boolean(r))),
            0x11 => bin!(|l: &V, r: &V| V::Bool(ecma::to_boolean(l) || ecma::to_boolean(r))),
            0x0a => bin!(|l: &V, r: &V| V::Double(float_point(l) + float_point(r))),
            0x47 => bin!(|l: &V, r: &V| {
                if ecma::ty(l) == Ty::Str || ecma::ty(r) == Ty::Str {
                    V::Str(ecma::to_string(l) + &ecma::to_string(r))
                } else {
                    V::Double(ecma::to_number_as2(l) + ecma::to_number_as2(r))
                }
            }),
            0x60 => bin!(|l: &V, r: &V| V::Long((ecma::to_int32(l) & ecma::to_int32(r)) as i64)),
            0x61 => bin!(|l: &V, r: &V| V::Long((ecma::to_int32(l) | ecma::to_int32(r)) as i64)),
            0x62 => bin!(|l: &V, r: &V| V::Long((ecma::to_int32(l) ^ ecma::to_int32(r)) as i64)),
            0x63 => bin!(|l: &V, r: &V| V::Int(ecma::to_int32(l).wrapping_shl(ecma::to_int32(r) as u32))),
            0x64 => bin!(|l: &V, r: &V| V::Long((ecma::to_int32(l) >> (ecma::to_int32(r) & 0x1f)) as i64)),
            0x65 => bin_num!(|l: f64, r: f64| V::Long(((l as i64) & 0xffff_ffff) >> ((r as i64) & 0x1f))),
            0x0d => bin_num!(|l: f64, r: f64| V::Double(if r == 0.0 && r.is_sign_positive() {
                if l < 0.0 {
                    f64::NEG_INFINITY
                } else if l > 0.0 {
                    f64::INFINITY
                } else {
                    f64::NAN
                }
            } else {
                l / r
            })),
            0x0e => bin!(|l: &V, r: &V| V::Bool(float_point(l) == float_point(r))),
            0x49 => bin!(|l: &V, r: &V| V::Bool(ecma::equals_in(true, l, r))),
            0x67 => bin!(|l: &V, r: &V| match ecma::compare_in(l, r, true) {
                None => V::Undefined,
                Some(c) => V::Bool(c == 1),
            }),
            0x0f => bin!(|l: &V, r: &V| V::Bool(float_point(l) < float_point(r))),
            0x48 => bin!(|l: &V, r: &V| match ecma::compare_in(l, r, true) {
                None => V::Undefined,
                Some(c) => V::Bool(c == -1),
            }),
            0x3f => bin_num!(|l: f64, r: f64| V::Double(if l.is_nan() || r.is_nan() || (r == 0.0 && r.is_sign_positive()) { f64::NAN } else { l % r })),
            0x0c => bin_num!(|l: f64, r: f64| V::Double(l * r)),
            0x0b => bin_num!(|l: f64, r: f64| V::Double(l - r)),
            0x21 => bin!(|l: &V, r: &V| V::Str(ecma::to_string(l) + &ecma::to_string(r))),
            0x13 => bin!(|l: &V, r: &V| V::Bool(ecma::to_string(l) == ecma::to_string(r))),
            0x68 => bin!(|l: &V, r: &V| V::Bool(utf16(&ecma::to_string(l)) > utf16(&ecma::to_string(r)))),
            0x29 => bin!(|l: &V, r: &V| V::Bool(utf16(&ecma::to_string(l)) < utf16(&ecma::to_string(r)))),
            0x4c => {
                if lda.is_empty() {
                    return false;
                }
                let v = lda.peek();
                lda.stack.push(v);
                true
            }
            0x17 => {
                if lda.is_empty() {
                    return false;
                }
                lda.pop();
                true
            }
            0x3c | 0x1d => {
                if !lda.has(2) {
                    return false;
                }
                let v = lda.pop();
                let name = lda.pop();
                lda.set_var(ecma::to_string(&name), v);
                true
            }
            0x1c => {
                if lda.is_empty() {
                    return false;
                }
                let name = lda.pop();
                let v = lda.var(&ecma::to_string(&name)).cloned().unwrap_or(V::Undefined);
                lda.stack.push(v);
                true
            }
            0x3e => {
                if lda.is_empty() {
                    return false;
                }
                lda.ret = Some(lda.pop());
                true
            }
            _ => {
                lda.ret = Some(V::Undefined);
                true
            }
        },
        _ => {
            lda.ret = Some(V::Undefined);
            true
        }
    }
}

/// Actions `executeActions` may continue through (besides a Pop right after PushDuplicate).
fn runnable(a: &Action) -> bool {
    match &a.op {
        Op::Push { .. } | Op::If { .. } | Op::Jump { .. } | Op::ConstantPool(_) | Op::End => true,
        Op::Simple => matches!(
            a.code,
            0x4c | 0x33 | 0x32 | 0x51 | 0x50 | 0x12 | 0x18 | 0x4a | 0x4b | 0x44 | 0x14 | 0x37 | 0x31 | 0x10 | 0x0a | 0x47 | 0x60 | 0x63 | 0x61 | 0x64 | 0x65 | 0x62 | 0x0d | 0x0e | 0x49 | 0x67 | 0x0f | 0x48 | 0x3f
                | 0x0c | 0x11 | 0x21 | 0x13 | 0x68 | 0x29 | 0x0b | 0x3c | 0x1c | 0x1d | 0x3d | 0x3e
        ),
        _ => false,
    }
}

fn uses_variables(a: &Action) -> bool {
    matches!(a.op, Op::ConstantPool(_) | Op::End) || (matches!(a.op, Op::Simple) && matches!(a.code, 0x3c | 0x1c | 0x1d | 0x3d | 0x3e))
}

#[derive(Default)]
struct Exec {
    item: Option<usize>,
    min_skipped: usize,
    max_skipped: usize,
    stack: Vec<V>,
    result: Option<V>,
    pool: Option<usize>,
    vars: Vec<(String, V)>,
    defines: Vec<String>,
}

fn is_fake_name(name: &str) -> bool {
    !super::decomp::print::is_valid_name(name, &[])
}

#[allow(clippy::too_many_arguments)]
fn execute_actions(f: &Fast, mut item: usize, lda: &mut Lda, pool: Option<&Arc<Vec<String>>>, res: &mut Exec, fakes: Option<&HashMap<String, V>>, use_vars: bool, allow_uninit: bool) -> R<()> {
    if f.it[item].a.is_exit() {
        return Ok(());
    }
    let mut processed = 0usize;
    let mut skipped = 0usize;
    let mut last_pool = None;
    let mut unknown = false;
    let mut jumped_here = true;
    let mut jump_found = false;
    let mut dup_before = false;
    loop {
        if f.it[item].excluded || processed > EXECUTION_LIMIT {
            break;
        }
        let a = &f.it[item].a;
        let simple = |c: u8| matches!(a.op, Op::Simple) && a.code == c;
        if matches!(a.op, Op::ConstantPool(_)) {
            last_pool = Some(item);
        } else if simple(0x3c) {
            if lda.stack.len() < 2 {
                break;
            }
            let name = ecma::to_string(&lda.stack[lda.stack.len() - 2]);
            res.defines.push(name);
        } else if simple(0x1c) {
            let Some(top) = lda.stack.last() else { break };
            let name = ecma::java_to_string(top);
            if lda.var(&name).is_none() && (!allow_uninit || !is_fake_name(&name)) {
                break;
            }
        }
        if simple(0x3d) {
            if lda.stack.len() < 2 {
                break;
            }
            let name = ecma::java_to_string(&lda.raw_pop());
            let args = ecma::to_uint32(&lda.raw_pop());
            match fakes.and_then(|m| m.get(&name)).filter(|_| args == 0) {
                Some(v) => lda.stack.push(v.clone()),
                None => break,
            }
        } else if !execute(lda, a) {
            break;
        }
        if !use_vars && uses_variables(a) {
            break;
        }
        if !(runnable(a) || (simple(0x17) && dup_before)) {
            break;
        }
        if let Op::Push { values, .. } = &a.op {
            processed += values.len().saturating_sub(1);
            if values.iter().any(|v| (pool.is_none() && matches!(v, Value::Constant(_))) || matches!(v, Value::Register(_))) {
                break;
            }
        }
        if simple(0x4c) {
            dup_before = true;
        } else if !matches!(a.op, Op::If { .. } | Op::Jump { .. }) && !simple(0x12) {
            dup_before = false;
        }
        let is_end = matches!(a.op, Op::End);
        if !is_end {
            let prev_item = item;
            let prev_jumped = jumped_here;
            if lda.jump {
                item = f.it[item].jump.ok_or(Abort)?;
                jumped_here = true;
                lda.jump = false;
            } else {
                item = f.it[item].next;
                jumped_here = false;
            }
            processed += 1;
            if !unknown {
                let mut target = f.is_jump_target(prev_item) || f.is_container_last(f.it[prev_item].prev);
                if target && prev_jumped && f.it[prev_item].jumps_here.len() == 1 {
                    target = false;
                }
                if target {
                    unknown = true;
                } else {
                    skipped += 1;
                    if matches!(a.op, Op::If { .. }) {
                        unknown = true;
                    }
                }
                if jumped_here {
                    unknown = true;
                }
            }
        }
        let fixed_stop = lda.all_fixed() && (simple(0x17) || matches!(a.op, Op::If { .. }) || (jump_found && a.is_jump()) || simple(0x1d));
        if is_end || fixed_stop {
            res.item = Some(item);
            res.min_skipped = skipped;
            res.max_skipped = if unknown { usize::MAX } else { skipped };
            res.pool = last_pool;
            res.vars = lda.vars.clone();
            res.stack = lda.stack.clone();
            if is_end {
                break;
            }
        }
        if jumped_here && f.is_container_last(f.it[item].prev) {
            break;
        }
        if simple(0x3e) {
            res.result = lda.ret.clone();
            break;
        }
        if a.is_jump() {
            jump_found = true;
        }
    }
    Ok(())
}

struct Deob {
    f: Fast,
    /// What FFDec's `ActionList` holds: the last list stored by `setActions`.
    stored: Vec<Action>,
}

impl Deob {
    fn store(&mut self) -> R<()> {
        self.stored = self.f.to_actions()?;
        Ok(())
    }

    /// `getConstantPool`: the only ConstantPool of the list.
    fn single_pool(&self) -> Option<usize> {
        let mut found = None;
        for i in self.f.order() {
            if matches!(self.f.it[i].a.op, Op::ConstantPool(_)) {
                if found.is_some() {
                    return None;
                }
                found = Some(i);
            }
        }
        found
    }

    fn fake_functions(&mut self) -> R<HashMap<String, V>> {
        let mut results = HashMap::new();
        let mut lda = Lda::new();
        let mut it = Iter::new(&self.f);
        while it.has_next(&self.f) {
            let item = it.next(&self.f);
            let Op::DefineFunction { name, params, .. } = &self.f.it[item].a.op else { continue };
            if !params.is_empty() || name.is_empty() || !is_fake_name(name) {
                continue;
            }
            let name = name.clone();
            let mut res = Exec::default();
            if self.mark_container(item)? {
                lda.clear();
                let next = self.f.it[item].next;
                execute_actions(&self.f, next, &mut lda, None, &mut res, None, true, false)?;
                if let Some(v) = res.result.take() {
                    results.insert(name, v);
                    self.f.remove_included()?;
                }
            }
        }
        self.f.set_excluded(false);
        Ok(results)
    }

    fn mark_container(&mut self, item: usize) -> R<bool> {
        let last = self.f.it[item].lasts.as_ref().and_then(|l| l.first().copied()).ok_or(Abort)?;
        if last == Some(item) {
            return Ok(false);
        }
        self.f.set_excluded(true);
        let end = self.f.last();
        let mut i = item;
        loop {
            self.f.it[i].excluded = false;
            i = self.f.it[i].next;
            if Some(i) == last || Some(i) == end {
                break;
            }
        }
        self.f.it[i].excluded = false;
        Ok(true)
    }

    fn remove_get_times(&mut self) -> bool {
        if self.f.is_empty() {
            return false;
        }
        let mut ret = false;
        let mut changed = true;
        let mut count = 1;
        while changed && count > 0 {
            changed = false;
            self.f.remove_unreachable();
            self.f.remove_zero_jumps();
            count = 0;
            let f = &mut self.f;
            let mut it = Iter::new(f);
            while it.has_next(f) {
                let a = it.next(f);
                let a2 = it.peek(f, 0);
                let is_time = matches!(f.it[a].a.op, Op::Simple) && f.it[a].a.code == 0x34;
                if is_time {
                    count += 1;
                }
                if is_time && matches!(f.it[a2].a.op, Op::If { .. }) {
                    let target = f.it[a2].jump;
                    let j = f.alloc(Action::new(0x99, Op::Jump { offset: 0, deobfuscate: false }));
                    f.set_jump(j, target);
                    it.next(f);
                    it.add(f, j);
                    it.prev(f);
                    it.prev(f);
                    it.remove(f);
                    it.next(f);
                    it.remove(f);
                    it.next(f);
                    changed = true;
                    ret = true;
                    count -= 1;
                }
            }
            if !changed && count > 0 {
                let mut it = Iter::new(f);
                while it.has_next(f) {
                    let a = it.next(f);
                    let a1 = it.peek(f, 0);
                    let a2 = it.peek(f, 1);
                    let simple = |i: usize, c: u8| matches!(f.it[i].a.op, Op::Simple) && f.it[i].a.code == c;
                    if simple(a, 0x34) && simple(a1, 0x50) && matches!(f.it[a2].a.op, Op::If { .. }) {
                        let target = f.it[a2].jump;
                        let j = f.alloc(Action::new(0x99, Op::Jump { offset: 0, deobfuscate: false }));
                        f.set_jump(j, target);
                        it.next(f);
                        it.next(f);
                        it.add(f, j);
                        it.prev(f);
                        it.prev(f);
                        it.prev(f);
                        it.remove(f);
                        it.next(f);
                        it.remove(f);
                        it.next(f);
                        it.remove(f);
                        it.next(f);
                        changed = true;
                        ret = true;
                    }
                }
            }
        }
        ret
    }

    fn remove_obfuscation_ifs(&mut self, fakes: &HashMap<String, V>, use_vars: bool) -> R<bool> {
        if self.f.is_empty() {
            return Ok(false);
        }
        let mut ret = false;
        self.f.remove_unreachable();
        self.f.remove_zero_jumps();
        let pool = self.single_pool().map(|i| match &self.f.it[i].a.op {
            Op::ConstantPool(c) => Arc::new(c.clone()),
            _ => unreachable!(),
        });
        let mut lda = Lda::new();
        let mut it = Iter::new(&self.f);
        let mut first = true;
        while it.has_next(&self.f) {
            let item = it.next(&self.f);
            let mut res = Exec::default();
            lda.clear();
            lda.check = !first;
            execute_actions(&self.f, item, &mut lda, pool.as_ref(), &mut res, Some(fakes), use_vars, first)?;
            if let (Some(target), None) = (res.item, &res.result) {
                let mut count = 1 + res.stack.len();
                if res.pool.is_some() {
                    count += 1;
                }
                count += 3 * res.vars.len();
                if res.vars.iter().all(|(_, v)| valid_value(v)) && count < res.max_skipped {
                    let mut unreachable = res.min_skipped;
                    if count >= res.min_skipped {
                        unreachable = self.f.unreachable_count(item, target);
                    }
                    if count < unreachable {
                        let f = &mut self.f;
                        if res.stack.is_empty() && res.vars.is_empty() && res.pool.is_none() && f.it[item].a.is_jump() {
                            f.set_jump(item, Some(target));
                        } else {
                            let prev = f.it[item].prev;
                            if let Some(p) = res.pool {
                                let Op::ConstantPool(c) = &f.it[p].a.op else { unreachable!() };
                                let n = f.alloc(Action::new(0x88, Op::ConstantPool(c.clone())));
                                it.add_before(f, n);
                            }
                            for (name, v) in &res.vars {
                                let n = f.alloc(push(Value::Str(name.clone())));
                                it.add_before(f, n);
                                let n = f.alloc(push(v_to_value(v)));
                                it.add_before(f, n);
                                let code = if res.defines.contains(name) { 0x3c } else { 0x1d };
                                let n = f.alloc(Action::new(code, Op::Simple));
                                it.add_before(f, n);
                            }
                            for v in &res.stack {
                                let n = f.alloc(push(v_to_value(v)));
                                it.add_before(f, n);
                            }
                            let j = f.alloc(Action::new(0x99, Op::Jump { offset: 0, deobfuscate: false }));
                            f.set_jump(j, Some(target));
                            it.add_before(f, j);
                            let to = f.it[prev].next;
                            f.replace_jump_targets(item, to);
                        }
                        let prev = self.f.it[item].prev;
                        self.f.remove_unreachable();
                        self.f.remove_zero_jumps();
                        let cur = self.f.it[self.f.it[prev].next].next;
                        it.set_current(&self.f, cur);
                        ret = true;
                    }
                }
            }
            first = false;
        }
        Ok(ret)
    }

    fn jumps_after_function(&mut self) -> R<bool> {
        let mut ends: Vec<usize> = Vec::new();
        let mut changed = false;
        let mut it = Iter::new(&self.f);
        while it.has_next(&self.f) {
            let item = it.next(&self.f);
            let addr = self.f.it[item].a.address;
            let mut end = None;
            while let Some(&e) = ends.last() {
                end = Some(e);
                if addr >= self.f.it[e].a.address {
                    ends.pop();
                    end = None;
                } else {
                    break;
                }
            }
            if matches!(self.f.it[item].a.op, Op::DefineFunction { .. } | Op::DefineFunction2(_)) {
                let l = self.f.first_last(item)?;
                ends.push(self.f.it[l].next);
            }
            if let Some(e) = end {
                if let Some(t) = self.f.it[item].a.target() {
                    if t > self.f.it[e].a.address {
                        self.f.set_jump(item, Some(e));
                        changed = true;
                    }
                }
            }
        }
        Ok(changed)
    }

    /// `ActionListReader.fixConstantPools` over the stored list.
    fn fix_constant_pools(&mut self, restore: bool) -> bool {
        let order = self.f.order();
        if order.is_empty() {
            return false;
        }
        let end_ip = self.f.it[*order.last().unwrap()].a.address as i64;
        let start_ip = self.f.it[order[0]].a.address as i64;
        let mut by_addr: HashMap<i64, usize> = HashMap::new();
        for &i in &order {
            by_addr.insert(self.f.it[i].a.address as i64, i);
        }
        let mut seen: HashMap<i64, usize> = HashMap::new();
        let mut visited = Vec::new();
        let empty = Arc::new(Vec::new());
        let ret = self.fix_pools(&by_addr, &mut seen, empty, start_ip, start_ip, end_ip, &mut visited);
        if restore {
            self.stored = order.iter().map(|&i| self.f.it[i].a.clone()).collect();
        }
        ret
    }

    #[allow(clippy::too_many_arguments)]
    fn fix_pools(&mut self, by_addr: &HashMap<i64, usize>, seen: &mut HashMap<i64, usize>, mut pool: Arc<Vec<String>>, ip: i64, start_ip: i64, end_ip: i64, visited: &mut Vec<i64>) -> bool {
        if visited.contains(&ip) {
            return false;
        }
        visited.push(ip);
        let mut queue = std::collections::VecDeque::from([ip]);
        let mut ret = false;
        while let Some(q) = queue.pop_front() {
            let mut ip = q;
            if ip < start_ip {
                continue;
            }
            while end_ip == -1 || end_ip > ip {
                let Some(&i) = by_addr.get(&ip) else { break };
                let mut len = self.f.it[i].a.total_len() as i64;
                if seen.contains_key(&ip) {
                    break;
                }
                seen.insert(ip, i);
                let item_pool = self.f.it[i].pool.clone();
                let a = &mut self.f.it[i].a;
                let sizes = a.container_sizes();
                let exit = a.is_exit();
                let mut stop = false;
                match &mut a.op {
                    Op::Push { pool: p, .. } => {
                        if !p.as_ref().is_some_and(|x| Arc::ptr_eq(x, &pool)) {
                            *p = if pool.is_empty() { None } else { Some(pool.clone()) };
                            if p.is_some() {
                                ret = true;
                            }
                        }
                    }
                    Op::ConstantPool(_) => pool = item_pool.unwrap(),
                    Op::If { offset } => {
                        let n = ip + len + *offset as i64;
                        if n >= 0 {
                            queue.push_back(n);
                        }
                    }
                    Op::Jump { offset, .. } => {
                        let n = ip + len + *offset as i64;
                        if n >= 0 {
                            queue.push_back(n);
                        }
                        stop = true;
                    }
                    _ => {}
                }
                if stop {
                    break;
                }
                if let Some(sizes) = sizes {
                    for size in sizes {
                        if size != 0 {
                            let ip2 = ip + len;
                            let end2 = ip + len + size as i64;
                            ret |= self.fix_pools(by_addr, seen, pool.clone(), ip2, start_ip, end2, visited);
                            len += size as i64;
                        }
                    }
                }
                ip += len;
                if exit {
                    break;
                }
            }
        }
        ret
    }

    /// `actionListParsed`.
    fn run(&mut self) -> R<()> {
        // The reader gave each push its constant pool; that is the identity FFDec compares.
        self.fix_constant_pools(false);
        self.f.remove_unknown();
        self.f.expand_pushes();
        // FFDec's ActionList shares the pushes that expandPushes cut down to their first value.
        for (i, stored) in self.stored.iter_mut().enumerate() {
            if let (Op::Push { values, .. }, Op::Push { values: sv, .. }) = (&self.f.it[i].a.op, &mut stored.op) {
                sv.truncate(values.len());
            }
        }
        let fakes = self.fake_functions()?;
        let mut changed = true;
        let mut use_vars = false;
        while changed {
            while changed {
                while changed {
                    changed = self.remove_get_times();
                    changed |= self.remove_obfuscation_ifs(&fakes, use_vars)?;
                    self.store()?;
                    changed |= self.fix_constant_pools(true);
                    if !changed && !use_vars {
                        use_vars = true;
                        changed = true;
                    }
                }
            }
            changed = self.jumps_after_function()?;
            if changed {
                use_vars = false;
            }
        }
        Ok(())
    }
}

fn push(v: Value) -> Action {
    Action::new(0x96, Op::Push { values: vec![v], pool: None })
}

/// Runs FFDec's AS1/2 deobfuscator on a freshly read action list. When it fails part way, the
/// list is left as FFDec leaves it: as last stored.
pub fn deobfuscate(list: &mut ActionList) {
    if list.actions.is_empty() {
        return;
    }
    let mut d = Deob { f: Fast::new(list), stored: list.actions.clone() };
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.run()));
    list.actions = d.stored;
}

/// `BrokenScriptDetector.codeIsBroken`: decompiled code FFDec could not express as ActionScript.
pub fn code_is_broken(code: &str) -> bool {
    code.contains("\u{a7}\u{a7}")
}
