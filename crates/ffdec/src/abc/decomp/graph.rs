//! FFDec's `Graph` engine: control-flow graph construction, reachability and common parts.
//! Structuring (`printGraph`) lives in `structure.rs`, AVM2 specifics in `avm2graph.rs`.

use std::collections::VecDeque;

use super::item::*;
use super::item::PartId as P;
use super::ld::Ld;
use super::parts::*;
use super::stack::Stack;
use super::translate::Tr;

pub type St = std::rc::Rc<std::cell::RefCell<Stack>>;

pub fn st(s: Stack) -> St {
    std::rc::Rc::new(std::cell::RefCell::new(s))
}

/// `stack.clone()` of a shared stack.
pub fn st_clone(s: &St) -> St {
    st(s.borrow().duplicate())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    Other,
    BlockClose,
}

pub type Stops = Vec<(P, StopKind)>;

/// The state `printGraph` threads through every call besides parts, stacks and local data.
pub struct Pc {
    pub has_empty_stack_pops: bool,
    pub found_gotos: Vec<It>,
    pub part_codes: Vec<(P, List)>,
    pub part_code_pos: Vec<(P, usize)>,
    pub all_parts: PartSet,
    pub loops: Vec<usize>,
    pub throw_states: Vec<ThrowState>,
    pub static_operation: i32,
}

pub struct G<'t, 'c, 'a> {
    pub parts: Vec<Part>,
    pub arena: Vec<Loop>,
    pub heads: Vec<P>,
    pub tr: &'t Tr<'c, 'a>,
    pub do_not_process_ifs: bool,
    /// The AS1/2 translator; `tr` then only carries the action list as `Code`.
    pub act: Option<&'t crate::action::decomp::translate::ActTr>,
}

impl<'t, 'c, 'a> G<'t, 'c, 'a> {
    pub fn new(tr: &'t Tr<'c, 'a>) -> Self {
        G { parts: Vec::new(), arena: Vec::new(), heads: Vec::new(), tr, do_not_process_ifs: false, act: None }
    }

    pub fn as2(&self) -> bool {
        self.act.is_some()
    }

    /// `checkIp`.
    pub fn check_ip(&self, ip: i32) -> i32 {
        match self.act {
            Some(a) => a.check_ip(&self.tr.code, ip),
            None => ip,
        }
    }

    pub fn code_len(&self) -> i32 {
        self.tr.code.len() as i32
    }

    pub fn new_part(&mut self, start: i32, end: i32) -> P {
        self.parts.push(Part::new(start, end));
        self.parts.len() - 1
    }

    pub fn peq(&self, a: P, b: P) -> bool {
        peq(&self.parts, a, b)
    }

    /// What `GraphPart.equals` compares.
    pub fn part_key(&self, p: P) -> (i32, i32) {
        (self.parts[p].start, self.parts[p].end)
    }

    pub fn pcontains(&self, list: &[P], p: P) -> bool {
        pcontains(&self.parts, list, p)
    }

    pub fn next_parts(&self, p: P) -> Vec<P> {
        self.parts[p].next.clone()
    }

    pub fn lp(&self, l: usize) -> &Loop {
        &self.arena[l]
    }

    pub fn lpm(&mut self, l: usize) -> &mut Loop {
        &mut self.arena[l]
    }

    /// `searchPart`.
    pub fn search_part(&self, ip: i32, all: &[P]) -> Option<P> {
        if ip < 0 {
            return None;
        }
        all.iter().copied().find(|&p| ip >= self.parts[p].start && ip <= self.parts[p].end)
    }

    /// `populateParts`.
    pub fn populate_parts(&self, part: P, all: &mut PartSet) {
        if all.contains(&self.parts, part) {
            return;
        }
        all.insert(&self.parts, part);
        let mut q: VecDeque<P> = VecDeque::from([part]);
        while let Some(p) = q.pop_front() {
            for &n in &self.parts[p].next {
                if !all.contains(&self.parts, n) {
                    all.insert(&self.parts, n);
                    q.push_back(n);
                }
            }
        }
    }

    // Reachability.

    /// `GraphPart.hasPathTo`; with `collect`, gathers every reachable part instead.
    fn has_path_to(&self, ld: &Ld, from: P, target: Option<P>, loops: &[usize], throws: &[ThrowState], first_can_be_continue: bool, mut collect: Option<&mut Vec<P>>) -> bool {
        let mut todo: Vec<P> = vec![from];
        let mut visited: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
        let mut collected: std::collections::HashSet<P> = std::collections::HashSet::new();
        let mut first = first_can_be_continue;
        'todo: while let Some(this) = todo.pop() {
            super::method::check_deadline();
            let Some(tpart) = self.check_part(None, ld, None, this) else { continue };
            if tpart != this {
                todo.push(tpart);
                continue;
            }
            for &l in loops {
                let lp = &self.arena[l];
                if lp.phase == 1 && (lp.cont == Some(this) || lp.pre_cont == Some(this)) && !first {
                    continue 'todo;
                }
            }
            first = false;
            if !visited.insert(self.part_key(this)) {
                continue;
            }
            for &p in &self.parts[this].next {
                match collect.as_deref_mut() {
                    Some(c) => {
                        if collected.insert(p) {
                            c.push(p);
                        }
                    }
                    None => {
                        if Some(p) == target {
                            return true;
                        }
                    }
                }
                if visited.contains(&self.part_key(p)) {
                    continue;
                }
                todo.push(p);
            }
            for ts in throws {
                if ts.state != 1 && ts.throwing.contains(&self.parts, this) {
                    let p = ts.target.expect("throw target");
                    match collect.as_deref_mut() {
                        Some(c) => {
                            if collected.insert(p) {
                                c.push(p);
                            }
                        }
                        None => {
                            if Some(p) == target {
                                return true;
                            }
                        }
                    }
                    if visited.contains(&self.part_key(p)) {
                        continue;
                    }
                    todo.push(p);
                }
            }
        }
        false
    }

    /// `GraphPart.leadsTo`.
    pub fn leads_to(&self, ld: &Ld, from: P, to: P, pc: &Pc, first_can_be_continue: bool) -> bool {
        self.has_path_to(ld, from, Some(to), &pc.loops, &pc.throw_states, first_can_be_continue, None)
    }

    pub fn leads_to_with(&self, ld: &Ld, from: P, to: P, loops: &[usize], throws: &[ThrowState], first_can_be_continue: bool) -> bool {
        self.has_path_to(ld, from, Some(to), loops, throws, first_can_be_continue, None)
    }

    /// `GraphPart.getReachableParts` (identity bitset).
    pub fn reachable_bits(&self, ld: &Ld, from: P, pc: &Pc, first_can_be_continue: bool) -> Vec<P> {
        let mut c = Vec::new();
        self.has_path_to(ld, from, None, &pc.loops, &pc.throw_states, first_can_be_continue, Some(&mut c));
        c
    }

    /// `Graph.getReachableParts`: the loop-aware reachable set used by common-part searches.
    pub fn get_reachable_parts(&mut self, ld: &Ld, part: P, ret: &mut PartList, loops: &[usize], throws: &[ThrowState]) {
        struct Q {
            items: VecDeque<P>,
            current_loop: Option<usize>,
        }
        let mut stack: Vec<Q> = vec![Q { items: VecDeque::from([part]), current_loop: None }];
        'next: while !stack.is_empty() {
            let part;
            {
                let q = stack.last_mut().unwrap();
                if let Some(p) = q.items.pop_front() {
                    part = p;
                } else if let Some(cl) = q.current_loop {
                    q.current_loop = None;
                    let b = self.arena[cl].brk.expect("loop break");
                    part = b;
                    if ret.contains(&self.parts, part) {
                        continue;
                    }
                    ret.0.push(part);
                    self.arena[cl].reachable_mark = 2;
                } else {
                    stack.pop();
                    continue;
                }
            }
            for &l in loops {
                self.arena[l].reachable_mark = 0;
            }
            let mut current_loop: Option<usize> = None;
            for &l in loops {
                let lp = &self.arena[l];
                if lp.phase == 1 || lp.reachable_mark == 1 {
                    if lp.cont == Some(part) || lp.brk == Some(part) || lp.pre_cont == Some(part) {
                        continue 'next;
                    }
                }
                if lp.reachable_mark == 0 && lp.cont == Some(part) {
                    self.arena[l].reachable_mark = 1;
                    current_loop = Some(l);
                }
            }
            let mut new_parts: VecDeque<P> = VecDeque::new();
            let nexts = self.next_parts(part);
            for ts in throws {
                if ts.state != 1 && ts.throwing.contains(&self.parts, part) {
                    new_parts.push_back(ts.target.expect("throw target"));
                }
            }
            'nx: for raw in nexts {
                let Some(next) = self.check_part(None, ld, Some(part), raw) else { continue };
                if self.parts[next].start >= self.code_len() {
                    continue;
                }
                for &l in loops {
                    let lp = &self.arena[l];
                    if (lp.phase == 1 || lp.reachable_mark == 1) && (lp.cont == Some(next) || lp.brk == Some(next) || lp.pre_cont == Some(next)) {
                        continue 'nx;
                    }
                }
                if !ret.contains(&self.parts, next) {
                    new_parts.push_back(next);
                }
            }
            for &p in &new_parts {
                ret.insert(&self.parts, p);
            }
            let cl = match current_loop {
                Some(l) if self.arena[l].brk.is_some() => Some(l),
                _ => None,
            };
            if !new_parts.is_empty() || cl.is_some() {
                stack.push(Q { items: new_parts, current_loop: cl });
            }
        }
    }

    /// `getCommonPart`.
    pub fn get_common_part(&mut self, ld: &Ld, _prev: Option<P>, parts: &[P], pc: &Pc) -> Option<P> {
        self.get_common_part_with(ld, parts, &pc.loops.clone(), &pc.throw_states)
    }

    pub fn get_common_part_with(&mut self, ld: &Ld, parts: &[P], loops: &[usize], throws: &[ThrowState]) -> Option<P> {
        if parts.is_empty() {
            return None;
        }
        let mut conts: Vec<P> = Vec::new();
        let mut brks: Vec<P> = Vec::new();
        for &l in loops {
            let lp = &self.arena[l];
            if lp.phase == 1 {
                if let Some(c) = lp.cont {
                    conts.push(c);
                }
                if let Some(pc) = lp.pre_cont {
                    conts.push(pc);
                }
                if let Some(b) = lp.brk {
                    brks.push(b);
                }
            }
        }
        for &p in parts {
            if self.pcontains(&conts, p) || self.pcontains(&brks, p) {
                break;
            }
            let mut common = true;
            for &q in parts {
                if q == p {
                    continue;
                }
                if !self.leads_to_with(ld, q, p, loops, throws, false) {
                    common = false;
                    break;
                }
            }
            if common {
                return Some(p);
            }
        }
        let mut reachable: Vec<PartList> = Vec::new();
        for &p in parts {
            let mut r1 = PartList::default();
            self.get_reachable_parts(ld, p, &mut r1, loops, throws);
            r1.insert(&self.parts, p);
            reachable.push(r1);
        }
        let first = reachable[0].0.clone();
        for p in first {
            if reachable.iter().all(|r| r.contains(&self.parts, p)) {
                if self.pcontains(&conts, p) || self.pcontains(&brks, p) {
                    return None;
                }
                return Some(p);
            }
        }
        None
    }

    /// `getMostCommonPart`.
    pub fn get_most_common_part(&mut self, ld: &Ld, parts: &[P], pc: &Pc) -> Option<P> {
        if parts.is_empty() {
            return None;
        }
        let parts: Vec<P> = PartSet::from_parts(&self.parts, parts).iter();
        let loops = pc.loops.clone();
        let mut conts: Vec<Option<P>> = Vec::new();
        for &l in &loops {
            let lp = &self.arena[l];
            if lp.phase == 1 {
                conts.push(lp.cont);
                conts.push(lp.pre_cont);
            }
        }
        let in_conts = |g: &Self, p: P| conts.iter().any(|c| c.is_some_and(|c| g.peq(c, p)));
        let mut reachable: Vec<(P, PartList)> = Vec::new();
        let mut all_reachable = PartList::default();
        for &p in &parts {
            let mut r1 = PartList::default();
            self.get_reachable_parts(ld, p, &mut r1, &loops, &pc.throw_states);
            let mut r2 = PartList::default();
            r2.insert(&self.parts, p);
            for &x in &r1.0 {
                r2.insert(&self.parts, x);
            }
            reachable.push((p, r2));
            all_reachable.insert(&self.parts, p);
            for &x in &r1.0 {
                all_reachable.insert(&self.parts, x);
            }
        }
        let reach_of = |g: &Self, p: P| -> &PartList { &reachable.iter().find(|(x, _)| g.peq(*x, p)).unwrap().1 };
        let mut common_set: Vec<(P, i32)> = Vec::new();
        for &r in &all_reachable.0 {
            if in_conts(self, r) {
                continue;
            }
            let mut common = true;
            let mut level = 0;
            for &p in &parts {
                if p == r {
                    level += 1;
                    continue;
                }
                if !reach_of(self, p).contains(&self.parts, r) {
                    common = false;
                } else {
                    level += 1;
                }
            }
            if common {
                let mut to_process: Vec<P> = parts.clone();
                let mut visited: Vec<P> = Vec::new();
                'proc: while let Some(p) = to_process.pop() {
                    if p == r || in_conts(self, p) || self.pcontains(&visited, p) {
                        continue;
                    }
                    visited.push(p);
                    for n in self.next_parts(p) {
                        if n == r || in_conts(self, n) || self.pcontains(&visited, n) {
                            continue;
                        }
                        if !self.leads_to(ld, n, r, pc, false) {
                            common = false;
                            break 'proc;
                        }
                    }
                    to_process.extend(self.next_parts(p));
                }
                if common {
                    return Some(r);
                }
            }
            let key = |g: &Self, x: &(P, i32)| (-x.1, g.parts[x.0].closed_time, g.parts[x.0].start);
            let k = key(self, &(r, level));
            if !common_set.iter().any(|x| key(self, x) == k) {
                common_set.push((r, level));
            }
        }
        common_set.sort_by_key(|x| (-x.1, self.parts[x.0].closed_time, self.parts[x.0].start));
        match common_set.first() {
            Some(&(p, level)) if level > 1 => Some(p),
            _ => None,
        }
    }

    // Graph construction.

    /// `makeGraph`.
    pub fn make_graph(&mut self, alternate: &[i32]) -> Vec<P> {
        let code = &self.tr.code;
        let n = code.len() as i32;
        let refs = code.graph_refs(alternate);
        let mut visited = vec![false; n as usize];
        let mut all: Vec<P> = Vec::new();
        let mut owner: Vec<P> = vec![P::MAX; n as usize + 1];
        struct W {
            parent: Option<P>,
            path: GraphPath,
            start: i32,
            last: i32,
        }
        let mut q: VecDeque<W> = VecDeque::new();
        let start_ip = code.start_ip;
        q.push_back(W { parent: None, path: GraphPath::default(), start: start_ip, last: 0 });
        for &pos in alternate {
            let e1 = self.new_part(-1, -1);
            self.parts[e1].path = GraphPath::root("e");
            q.push_back(W { parent: Some(e1), path: GraphPath::root("e"), start: pos, last: pos });
        }
        'q: while let Some(w) = q.pop_front() {
            let (parent, path, start, mut last) = (w.parent, w.path, w.start, w.last);
            let mut ip = start;
            if let Some(ex) = find_owner(&owner, ip) {
                if let Some(par) = parent {
                    self.parts[ex].refs.push(par);
                    self.parts[par].next.push(ex);
                }
                continue;
            }
            let ret = self.new_part(ip, -1);
            self.parts[ret].path = path.clone();
            let mut part = ret;
            if let Some(par) = parent {
                self.parts[ret].refs.push(par);
                self.parts[par].next.push(ret);
            }
            while ip < n {
                let aip = self.check_ip(ip);
                if aip >= n {
                    ip = aip;
                    break;
                }
                if visited[ip as usize] || (ip != start && refs[ip as usize].len() > 1) {
                    self.parts[part].end = last;
                    let found = find_owner(&owner, aip);
                    push_part(&mut all, &mut owner, &self.parts, part);
                    match found {
                        Some(f) => {
                            self.parts[part].next.push(f);
                            self.parts[f].refs.push(part);
                            break;
                        }
                        None => {
                            let np = self.new_part(aip, -1);
                            self.parts[np].path = path.clone();
                            self.parts[part].next.push(np);
                            self.parts[np].refs.push(part);
                            part = np;
                        }
                    }
                }
                visited[ip as usize] = true;
                ip = aip;
                last = ip;
                let ins = &self.tr.code.ins[ip as usize];
                if ins.ignored {
                    ip += 1;
                    continue;
                }
                if let Some(cend) = ins.container_end {
                    let define = ins.define_function;
                    ip = self.tr.code.adr2pos(cend).map_or(n, |x| x as i32);
                    if define {
                        self.parts[part].end = last;
                        push_part(&mut all, &mut owner, &self.parts, part);
                        let np = self.new_part(ip, -1);
                        self.parts[np].path = path.clone();
                        self.parts[part].next.push(np);
                        self.parts[np].refs.push(part);
                        part = np;
                    }
                    continue;
                }
                if ins.is_exit() {
                    self.parts[part].end = ip;
                    push_part(&mut all, &mut owner, &self.parts, part);
                    break;
                } else if ins.is_jump() {
                    self.parts[part].end = ip;
                    push_part(&mut all, &mut owner, &self.parts, part);
                    let target = self.check_ip(self.tr.code.branches(ip as usize)[0]);
                    q.push_back(W { parent: Some(part), path: path.clone(), start: target, last });
                    break;
                } else if ins.is_branch() {
                    self.parts[part].end = ip;
                    push_part(&mut all, &mut owner, &self.parts, part);
                    let branches = self.tr.code.branches(ip as usize);
                    for (i, &b) in branches.iter().enumerate() {
                        q.push_back(W { parent: Some(part), path: path.sub(i as i32, ip), start: self.check_ip(b), last: ip });
                    }
                    break;
                }
                ip += 1;
            }
            if self.parts[part].end == -1 && ip >= n {
                if self.parts[part].start == n {
                    self.parts[part].end = n;
                    push_part(&mut all, &mut owner, &self.parts, part);
                } else {
                    self.parts[part].end = ip - 1;
                    if let Some(&p) = all.iter().find(|&&p| self.parts[p].start == ip) {
                        self.parts[p].refs.push(part);
                        self.parts[part].next.push(p);
                        push_part(&mut all, &mut owner, &self.parts, part);
                        continue 'q;
                    }
                    let gp = self.new_part(ip, ip);
                    push_part(&mut all, &mut owner, &self.parts, gp);
                    self.parts[gp].refs.push(part);
                    self.parts[part].next.push(gp);
                    push_part(&mut all, &mut owner, &self.parts, part);
                }
            }
        }
        let mut heads = Vec::new();
        if let Some(h) = self.search_part(start_ip, &all) {
            heads.push(h);
        }
        for &pos in alternate {
            if let Some(h) = self.search_part(pos, &all) {
                heads.push(h);
            }
        }
        if self.tr.ctx.cfg.deobfuscate {
            self.flatten_jumps(&heads, &mut all);
        }
        heads
    }

    /// `flattenJumps`: blocks holding a single jump are bypassed.
    fn flatten_jumps(&mut self, heads: &[P], all: &mut Vec<P>) {
        let n = self.code_len();
        loop {
            let mut modified = false;
            let mut i = 0;
            while i < all.len() {
                let part = all[i];
                i += 1;
                let (start, height) = (self.parts[part].start, self.parts[part].height());
                if heads.contains(&part) || height != 1 || start < 0 || start >= n || self.tr.code.ins[start as usize].code != super::code::op::JUMP || self.parts[part].next.len() != 1 {
                    continue;
                }
                let next = self.parts[part].next[0];
                if next == part {
                    continue;
                }
                for r in self.parts[part].refs.clone() {
                    for x in self.parts[r].next.iter_mut() {
                        if *x == part {
                            *x = next;
                        }
                    }
                }
                if let Some(k) = self.parts[next].refs.iter().position(|&x| x == part) {
                    self.parts[next].refs.remove(k);
                }
                let refs = self.parts[part].refs.clone();
                self.parts[next].refs.extend(refs);
                i -= 1;
                all.remove(i);
                i += 1;
                modified = true;
            }
            if !modified {
                break;
            }
        }
    }

    // Small helpers shared by the structuring code.

    /// `translatePart`.
    pub fn translate_part(&mut self, output: &List, ld: &mut Ld, part: P, stack: &St) {
        let n = self.code_len();
        if self.parts[part].end == -1 {
            self.parts[part].end = n - 1;
        }
        if self.parts[part].start == n {
            return;
        } else if self.parts[part].end == n {
            self.parts[part].end -= 1;
        }
        stack.borrow_mut().set_connected_output(Some(output.clone()));
        let (s, e) = (self.parts[part].start, self.parts[part].end);
        self.translate_range(output, ld, stack, s as usize, e as usize);
    }

    /// `GraphSource.translatePart`; AS1/2 can report the ip a container ran past.
    pub fn translate_range(&self, output: &List, ld: &mut Ld, stack: &St, s: usize, e: usize) -> Option<i32> {
        match self.act {
            Some(a) => a.translate_range(self, output, ld, stack, s, e),
            None => {
                self.tr.translate_range(output, ld, stack, s, e);
                None
            }
        }
    }

    /// `translatePartGetStack` with an output list.
    pub fn translate_part_get_stack(&mut self, ld: &mut Ld, part: P, stack: &St, output: &List) -> It {
        let stack = st_clone(stack);
        output.borrow_mut().clear();
        self.translate_part(output, ld, part, &stack);
        let mut p = part;
        while self.parts[p].next.len() == 1 && self.parts[self.parts[p].next[0]].refs.len() == 1 {
            p = self.parts[p].next[0];
            self.translate_part(output, ld, p, &stack);
        }
        let v = stack.borrow_mut().pop();
        v
    }

    /// `getLastPartSourceItem`: position of the last instruction of a straight-line run.
    pub fn last_part_ip(&self, part: P) -> usize {
        let mut p = part;
        while self.parts[p].next.len() == 1 && self.parts[self.parts[p].next[0]].refs.len() == 1 {
            p = self.parts[p].next[0];
        }
        let mut end = self.parts[p].end;
        if end >= self.code_len() {
            end = self.code_len() - 1;
        }
        end as usize
    }

    /// `checkLoop(output, part, stopPart, loops, throwStates)`.
    pub fn check_loop_part(&self, part: Option<P>, stop: &Stops, pc: &Pc) -> Option<It> {
        let part = part?;
        if stop.iter().any(|(s, _)| self.peq(*s, part)) {
            return None;
        }
        let first = if self.parts[part].start >= 0 && self.parts[part].start < self.code_len() { Some(self.parts[part].start as usize) } else { None };
        for &l in &pc.loops {
            let lp = &self.arena[l];
            if lp.phase != 1 {
                continue;
            }
            if lp.cont == Some(part) {
                return Some(continue_item(None, first, lp.id));
            }
            if lp.brk == Some(part) {
                return Some(break_item(None, first, lp.id));
            }
        }
        None
    }

    pub fn part_is_loop_cont_bre_pre(&self, part: P, pc: &Pc) -> bool {
        pc.loops.iter().any(|&l| {
            let lp = &self.arena[l];
            lp.phase == 1 && (lp.brk == Some(part) || lp.cont == Some(part) || lp.pre_cont == Some(part))
        })
    }

    /// `makeAllCommands`: AVM2 drops pending function literals, then the stack becomes pushes.
    pub fn make_all_commands(&self, commands: &List, stack: &St) {
        if self.as2() {
            return self.as2_make_all_commands(commands, stack);
        }
        {
            let mut s = stack.borrow_mut();
            s.items.retain(|i| !is::new_function(i));
        }
        stack.borrow_mut().finish_block(commands);
    }
}

/// `isEmpty(List)`: empty or only a decompiler mark.
pub fn list_is_empty(l: &List) -> bool {
    l.borrow().is_empty()
}

pub fn last_of(l: &List) -> Option<It> {
    l.borrow().last().cloned()
}

pub fn pop_last(l: &List) -> Option<It> {
    l.borrow_mut().pop()
}

/// Records which part first covers each ip, so `searchPart` during graph building is a lookup.
fn push_part(all: &mut Vec<P>, owner: &mut [P], parts: &[Part], p: P) {
    all.push(p);
    let (s, e) = (parts[p].start.max(0), parts[p].end.min(owner.len() as i32 - 1));
    for ip in s..=e {
        if owner[ip as usize] == P::MAX {
            owner[ip as usize] = p;
        }
    }
}

fn find_owner(owner: &[P], ip: i32) -> Option<P> {
    if ip < 0 {
        return None;
    }
    owner.get(ip as usize).copied().filter(|&p| p != P::MAX)
}
