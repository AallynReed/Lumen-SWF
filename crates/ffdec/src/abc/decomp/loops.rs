//! FFDec's `LoopDetector` and `GraphPrecontinueDetector`.

use std::collections::{HashMap, HashSet, VecDeque};

use super::item::PartId as P;
use super::parts::*;

/// `PrevNextWalker`: plain edges, optionally extended with exception edges.
pub enum Walker {
    Basic,
    Throw { map: Vec<(P, Vec<P>)>, rev: Vec<(P, Vec<P>)> },
}

fn map_get<'m>(parts: &[Part], m: &'m [(P, Vec<P>)], k: P) -> Option<&'m Vec<P>> {
    m.iter().find(|(x, _)| peq(parts, *x, k)).map(|(_, v)| v)
}

fn map_entry<'m>(parts: &[Part], m: &'m mut Vec<(P, Vec<P>)>, k: P) -> &'m mut Vec<P> {
    match m.iter().position(|(x, _)| peq(parts, *x, k)) {
        Some(i) => &mut m[i].1,
        None => {
            m.push((k, Vec::new()));
            &mut m.last_mut().unwrap().1
        }
    }
}

impl Walker {
    pub fn throw(parts: &[Part], states: &[&ThrowState]) -> Walker {
        let mut map = Vec::new();
        let mut rev = Vec::new();
        for ts in states {
            let target = ts.target.expect("throw target");
            for t in ts.throwing.iter() {
                map_entry(parts, &mut map, t).push(target);
            }
            map_entry(parts, &mut rev, target).extend(ts.throwing.iter());
        }
        Walker::Throw { map, rev }
    }

    pub fn prev(&self, parts: &[Part], node: P) -> Vec<P> {
        match self {
            Walker::Basic => parts[node].refs.clone(),
            Walker::Throw { rev, .. } => {
                let mut r = parts[node].refs.clone();
                if let Some(v) = map_get(parts, rev, node) {
                    r.extend(v.iter().copied());
                }
                r.retain(|&p| parts[p].start >= 0);
                r
            }
        }
    }

    pub fn next(&self, parts: &[Part], node: P) -> Vec<P> {
        match self {
            Walker::Basic => parts[node].next.clone(),
            Walker::Throw { map, .. } => {
                let mut r = parts[node].next.clone();
                if let Some(v) = map_get(parts, map, node) {
                    r.extend(v.iter().copied());
                }
                r
            }
        }
    }
}

pub struct LoopCtx<'a> {
    pub parts: &'a mut Vec<Part>,
    pub arena: &'a mut Vec<Loop>,
}

fn order_part(arena: &[Loop], l: usize) -> P {
    arena[l].order_part.or(arena[l].cont).expect("loop order part")
}

/// `LoopDetector.LoopComparator`.
pub fn sort_loops(parts: &[Part], arena: &[Loop], loops: &mut [usize]) {
    loops.sort_by(|&a, &b| {
        let ct = |p: P| parts[p].closed_time;
        let r = ct(order_part(arena, a)) - ct(order_part(arena, b));
        if r != 0 {
            return r.cmp(&0);
        }
        let nb = |l: usize| if arena[l].brk.is_none() { 1 } else { 0 };
        let r = nb(a) - nb(b);
        if r != 0 {
            return r.cmp(&0);
        }
        match (arena[a].brk, arena[b].brk) {
            (None, _) => std::cmp::Ordering::Equal,
            (Some(x), Some(y)) => (ct(y) - ct(x)).cmp(&0),
            (Some(_), None) => std::cmp::Ordering::Equal,
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub fn detect_loops(c: &mut LoopCtx, heads: &[P], loops: &mut Vec<usize>, throw_states: &[ThrowState], all_parts: &PartSet, switch_parts: &[Vec<P>], switch_breaks: &[Option<P>], detect_breaks: bool) {
    let ts_refs: Vec<&ThrowState> = throw_states.iter().collect();
    let pnw = Walker::throw(c.parts, &ts_refs);
    let back_edges = get_back_edges(c.parts, heads[0], throw_states);
    let mut by_header: Vec<(P, usize)> = Vec::new();
    for e in &back_edges {
        let from = e.from.expect("back edge source");
        let body = build_natural_loop(c.parts, from, e.to, &pnw);
        if let Some(&(_, l)) = by_header.iter().find(|(h, _)| peq(c.parts, *h, e.to)) {
            c.arena[l].back_edges.insert(c.parts, from);
            for p in body.0 {
                c.arena[l].body.insert(c.parts, p);
            }
        } else {
            let mut lp = Loop::new(loops.len() as i64, Some(e.to), None);
            lp.order_part = Some(e.to);
            lp.body = body;
            lp.back_edges.insert(c.parts, from);
            c.arena.push(lp);
            let id = c.arena.len() - 1;
            by_header.push((e.to, id));
            loops.push(id);
        }
    }
    compute_parent_loops(c, loops);
    calculate_closed_time(c, heads, loops);
    sort_loops(c.parts, c.arena, loops);
    for (i, &l) in loops.iter().enumerate() {
        c.arena[l].id = i as i64;
    }
    if !detect_breaks {
        return;
    }
    let mut ignored: Vec<Edge> = Vec::new();
    let mut multiple_same: PartList = PartList::default();
    for cases in switch_parts {
        for i in 1..cases.len() {
            let (c1, c2) = (cases[i - 1], cases[i]);
            if c1 == c2 {
                multiple_same.insert(c.parts, c1);
                continue;
            }
            if let Some(prev) = part_leads_to_get_prev(c.parts, c1, c2, &back_edges, &pnw) {
                edges_insert(c.parts, &mut ignored, Edge { from: Some(prev), to: c2 });
            }
        }
    }
    for &part in &multiple_same.0 {
        let prev = pnw.prev(c.parts, part);
        for r in 0..prev.len().saturating_sub(1) {
            edges_insert(c.parts, &mut ignored, Edge { from: Some(prev[r]), to: part });
        }
    }
    detect_precontinues(c, heads, all_parts, loops, throw_states);
    find_breaks(c, loops, &back_edges, throw_states, &ignored, switch_breaks);
    sort_loops(c.parts, c.arena, loops);
    for (i, &l) in loops.iter().enumerate() {
        c.arena[l].id = i as i64;
    }
    detect_precontinues(c, heads, all_parts, loops, throw_states);
}

fn body_contains_all(parts: &[Part], big: &PartList, small: &PartList) -> bool {
    small.0.iter().all(|&p| big.contains(parts, p))
}

fn compute_parent_loops(c: &mut LoopCtx, loops: &[usize]) {
    let n = loops.len();
    for i in 0..n {
        let child = &c.arena[loops[i]].body;
        let mut best: Option<usize> = None;
        let mut best_size = usize::MAX;
        for j in 0..n {
            if i == j {
                continue;
            }
            let cand = &c.arena[loops[j]].body;
            if cand.len() > child.len() && body_contains_all(c.parts, cand, child) && cand.len() < best_size {
                best_size = cand.len();
                best = Some(j);
            }
        }
        if let Some(b) = best {
            let parent = loops[b];
            c.arena[loops[i]].parent = Some(parent);
        }
    }
}

pub fn calculate_closed_time(c: &mut LoopCtx, heads: &[P], loops: &[usize]) {
    let mut opened: VecDeque<P> = VecDeque::new();
    let mut closed: Vec<P> = Vec::new();
    let mut visited: Vec<(P, P)> = Vec::new();
    for &h in heads {
        for &r in &c.parts[h].refs {
            visited.push((r, h));
        }
    }
    for &l in loops {
        let cont = c.arena[l].cont.expect("loop continue");
        for be in c.arena[l].back_edges.iter() {
            visited.push((be, cont));
        }
    }
    let mut time = 1;
    for &h in heads {
        opened.push_back(h);
        'opened: while let Some(part) = opened.pop_front() {
            if closed.iter().any(|&x| peq(c.parts, x, part)) {
                continue;
            }
            for &r in &c.parts[part].refs {
                if !visited.contains(&(r, part)) {
                    continue 'opened;
                }
            }
            for n in c.parts[part].next.clone() {
                opened.push_back(n);
                visited.push((part, n));
            }
            closed.push(part);
            c.parts[part].closed_time = time;
            time += 1;
        }
    }
}

fn set_contains(parts: &[Part], v: &[P], p: P) -> bool {
    pcontains(parts, v, p)
}

fn set_insert(parts: &[Part], v: &mut Vec<P>, p: P) {
    if !pcontains(parts, v, p) {
        v.push(p);
    }
}

fn set_remove_all(parts: &[Part], v: &mut Vec<P>, rm: &[P]) {
    v.retain(|&x| !pcontains(parts, rm, x));
}

fn find_breaks(c: &mut LoopCtx, loops: &[usize], back_edges: &[Edge], throw_states: &[ThrowState], ignored: &[Edge], switch_breaks: &[Option<P>]) {
    let mut walkers: Vec<Walker> = Vec::new();
    let mut sub_states: Vec<Vec<usize>> = Vec::new();
    for &l in loops {
        let cont = c.arena[l].cont.expect("loop continue");
        let subs: Vec<usize> = (0..throw_states.len()).filter(|&i| !throw_states[i].throwing.contains(c.parts, cont)).collect();
        let refs: Vec<&ThrowState> = subs.iter().map(|&i| &throw_states[i]).collect();
        let walker = Walker::throw(c.parts, &refs);
        let body = c.arena[l].body.0.clone();
        for part in body {
            for next in walker.next(c.parts, part) {
                if !c.arena[l].body.contains(c.parts, next) {
                    let mut eo = std::mem::take(&mut c.arena[l].edges_outside);
                    edges_insert(c.parts, &mut eo, Edge { from: Some(part), to: next });
                    c.arena[l].edges_outside = eo;
                }
            }
        }
        walkers.push(walker);
        sub_states.push(subs);
    }
    for (li, &l) in loops.iter().enumerate() {
        if c.arena[l].brk.is_some() {
            continue;
        }
        let parts: &[Part] = c.parts;
        let cont = c.arena[l].cont.expect("loop continue");
        let mut parent_continues: Vec<P> = Vec::new();
        let mut parent_breaks: Vec<P> = Vec::new();
        let mut parent = c.arena[l].parent;
        while let Some(pl) = parent {
            if let Some(b) = c.arena[pl].brk {
                set_insert(parts, &mut parent_breaks, b);
            }
            if let Some(cc) = c.arena[pl].cont {
                set_insert(parts, &mut parent_continues, cc);
            }
            if let Some(pc) = c.arena[pl].pre_cont {
                set_insert(parts, &mut parent_continues, pc);
            }
            parent = c.arena[pl].parent;
        }
        for b in switch_breaks.iter().flatten() {
            set_insert(parts, &mut parent_breaks, *b);
        }
        let walker = &walkers[li];
        let mut exception_targets: Vec<P> = Vec::new();
        for &ti in &sub_states[li] {
            let ts = &throw_states[ti];
            let mut catch: Vec<P> = if ts.catch_parts.is_empty() { vec![ts.target.expect("target")] } else { ts.catch_parts.iter() };
            catch = PartSet::from_parts(parts, &catch).iter();
            if pcontains(parts, &catch, cont) {
                continue;
            }
            set_remove_all(parts, &mut catch, &parent_continues);
            set_remove_all(parts, &mut catch, &parent_breaks);
            for p in catch {
                set_insert(parts, &mut exception_targets, p);
            }
        }
        let mut abrupt: Vec<P> = Vec::new();
        for &p in parent_breaks.iter().chain(parent_continues.iter()) {
            set_insert(parts, &mut abrupt, p);
        }
        let mut all_out_reachable: Vec<P> = Vec::new();
        let mut all_reachable: Vec<Vec<P>> = Vec::new();
        let mut out_parts: Vec<P> = Vec::new();
        let mut natural_out: Vec<P> = Vec::new();
        let mut considered: Vec<Edge> = Vec::new();
        for e in &c.arena[l].edges_outside {
            if edges_contain(parts, ignored, e) {
                continue;
            }
            considered.push(*e);
            set_insert(parts, &mut out_parts, e.to);
            if e.from == Some(cont) {
                set_insert(parts, &mut natural_out, e.to);
            }
        }
        let mut classified = abrupt.clone();
        if !natural_out.is_empty() && !abrupt.is_empty() {
            let mut boundary = abrupt.clone();
            for &p in natural_out.iter().chain(exception_targets.iter()) {
                set_insert(parts, &mut boundary, p);
            }
            for &op in &out_parts {
                if set_contains(parts, &natural_out, op) || set_contains(parts, &exception_targets, op) || set_contains(parts, &abrupt, op) {
                    continue;
                }
                if leads_only_to_abrupt(parts, op, &boundary, &abrupt, back_edges, walker, ignored) {
                    set_insert(parts, &mut classified, op);
                }
            }
        }
        let mut excluded = classified.clone();
        for &p in &exception_targets {
            set_insert(parts, &mut excluded, p);
        }
        let mut local_out = out_parts.clone();
        set_remove_all(parts, &mut local_out, &excluded);
        for e in &considered {
            let out = e.to;
            let mut reachable = get_reachable(parts, out, back_edges, &parent_breaks, walker, ignored);
            if set_contains(parts, &parent_continues, out) {
                reachable = vec![out];
            }
            for &r in &reachable {
                set_insert(parts, &mut all_out_reachable, r);
            }
            all_reachable.push(reachable);
        }
        set_remove_all(parts, &mut all_out_reachable, &exception_targets);
        if all_out_reachable.is_empty() {
            continue;
        }
        // Code that only returns or throws is no break when the loop also leaves another way
        // (FFDec 26 takes it, and then moves code that follows the loop's normal exit).
        let dead_end = |p: P| {
            let (mut seen, mut stack) = (vec![p], vec![p]);
            while let Some(q) = stack.pop() {
                for n in walker.next(parts, q) {
                    let e = Edge { from: Some(q), to: n };
                    if edges_contain(parts, back_edges, &e) || set_contains(parts, &abrupt, n) || set_contains(parts, &out_parts, n) || seen.len() > 64 {
                        return false;
                    }
                    if !set_contains(parts, &seen, n) {
                        seen.push(n);
                        stack.push(n);
                    }
                }
            }
            true
        };
        if let Some(exit) = nearest_common_post_dominator(parts, &local_out, back_edges, &classified, walker, ignored, &excluded, &out_parts).filter(|&e| !dead_end(e) || out_parts.iter().all(|&o| dead_end(o))) {
            c.arena[l].brk = Some(exit);
            continue;
        }
        let mut cands: Vec<(P, usize)> = all_out_reachable.iter().map(|&p| (p, all_reachable.iter().filter(|r| set_contains(parts, r, p)).count())).collect();
        cands.sort_by(|a, b| {
            use std::cmp::Ordering::*;
            let (a1, a2) = (set_contains(parts, &parent_breaks, a.0), set_contains(parts, &parent_breaks, b.0));
            if a1 != a2 {
                return if a1 { Greater } else { Less };
            }
            let (m1, m2) = (a.1 > 1, b.1 > 1);
            if m1 != m2 {
                return if m1 { Less } else { Greater };
            }
            let (o1, o2) = (set_contains(parts, &out_parts, a.0), set_contains(parts, &out_parts, b.0));
            if o1 != o2 {
                return if o1 { Less } else { Greater };
            }
            (parts[a.0].closed_time - parts[b.0].closed_time).cmp(&0)
        });
        c.arena[l].brk = Some(cands[0].0);
    }
}

fn successors_get<'m>(parts: &[Part], m: &'m [(P, Vec<P>)], k: P) -> Option<&'m Vec<P>> {
    map_get(parts, m, k)
}

fn leads_only_to_abrupt(parts: &[Part], start: P, boundary: &[P], abrupt: &[P], back_edges: &[Edge], pnw: &Walker, ignored: &[Edge]) -> bool {
    super::method::check_deadline();
    let mut succ: Vec<(P, Vec<P>)> = Vec::new();
    let mut q: VecDeque<P> = VecDeque::from([start]);
    let mut found = false;
    while let Some(part) = q.pop_front() {
        if successors_get(parts, &succ, part).is_some() {
            continue;
        }
        let mut nexts = Vec::new();
        let mut has = false;
        for next in pnw.next(parts, part) {
            let e = Edge { from: Some(part), to: next };
            if edges_contain(parts, ignored, &e) || edges_contain(parts, back_edges, &e) {
                continue;
            }
            has = true;
            if set_contains(parts, boundary, next) {
                if !set_contains(parts, abrupt, next) {
                    return false;
                }
                found = true;
                continue;
            }
            nexts.push(next);
            q.push_back(next);
        }
        succ.push((part, nexts));
        if !has {
            return false;
        }
    }
    if !found {
        return false;
    }
    let mut incoming: Vec<i32> = vec![0; succ.len()];
    let idx = |p: P| succ.iter().position(|(x, _)| peq(parts, *x, p));
    for (_, ns) in &succ {
        for &n in ns {
            if let Some(i) = idx(n) {
                incoming[i] += 1;
            }
        }
    }
    let mut zero: VecDeque<usize> = (0..succ.len()).filter(|&i| incoming[i] == 0).collect();
    let mut processed = 0;
    while let Some(i) = zero.pop_front() {
        processed += 1;
        for &n in &succ[i].1 {
            if let Some(j) = idx(n) {
                incoming[j] -= 1;
                if incoming[j] == 0 {
                    zero.push_back(j);
                }
            }
        }
    }
    processed == succ.len()
}

#[allow(clippy::too_many_arguments)]
fn nearest_common_post_dominator(parts: &[Part], starts: &[P], back_edges: &[Edge], stops: &[P], pnw: &Walker, ignored: &[Edge], excluded: &[P], direct_out: &[P]) -> Option<P> {
    super::method::check_deadline();
    if starts.is_empty() {
        return None;
    }
    let mut succ: Vec<(P, Vec<P>)> = Vec::new();
    let mut q: VecDeque<P> = starts.iter().copied().collect();
    while let Some(part) = q.pop_front() {
        if successors_get(parts, &succ, part).is_some() {
            continue;
        }
        let mut nexts = Vec::new();
        if !set_contains(parts, stops, part) {
            for next in pnw.next(parts, part) {
                let e = Edge { from: Some(part), to: next };
                if edges_contain(parts, ignored, &e) || edges_contain(parts, back_edges, &e) {
                    continue;
                }
                nexts.push(next);
                q.push_back(next);
            }
        }
        succ.push((part, nexts));
    }
    let n = succ.len();
    let index = |p: P| succ.iter().position(|(x, _)| peq(parts, *x, p));
    let next_idx: Vec<Vec<usize>> = succ.iter().map(|(_, ns)| ns.iter().map(|&x| index(x).unwrap()).collect()).collect();
    let all: Vec<usize> = (0..n).collect();
    let mut pd: Vec<Vec<bool>> = Vec::with_capacity(n);
    let mut order: Vec<Vec<usize>> = Vec::with_capacity(n);
    for i in 0..n {
        if next_idx[i].is_empty() {
            let mut b = vec![false; n];
            b[i] = true;
            pd.push(b);
            order.push(vec![i]);
        } else {
            pd.push(vec![true; n]);
            order.push(all.clone());
        }
    }
    loop {
        let mut changed = false;
        for i in 0..n {
            let nx = &next_idx[i];
            let mut upd = vec![false; n];
            if !nx.is_empty() {
                upd.clone_from(&pd[nx[0]]);
                for &j in &nx[1..] {
                    for k in 0..n {
                        upd[k] = upd[k] && pd[j][k];
                    }
                }
            }
            upd[i] = true;
            if upd != pd[i] {
                let mut ord = vec![i];
                if !nx.is_empty() {
                    for &cand in &order[nx[0]] {
                        if cand != i && upd[cand] {
                            ord.push(cand);
                        }
                    }
                }
                order[i] = ord;
                pd[i] = upd;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut common: Option<Vec<bool>> = None;
    let mut common_order: Vec<usize> = Vec::new();
    for &s in starts {
        let i = index(s)?;
        match &mut common {
            None => {
                common = Some(pd[i].clone());
                common_order = order[i].clone();
            }
            Some(cm) => {
                for k in 0..n {
                    cm[k] = cm[k] && pd[i][k];
                }
            }
        }
    }
    let common = common?;
    let mut cpd: Vec<P> = Vec::new();
    for &cand in &common_order {
        if common[cand] {
            set_insert(parts, &mut cpd, succ[cand].0);
        }
    }
    set_remove_all(parts, &mut cpd, excluded);
    if cpd.is_empty() {
        return None;
    }
    let mut sums: Vec<i64> = vec![0; cpd.len()];
    for &s in starts {
        let mut dist: Vec<(P, i32)> = vec![(s, 0)];
        let mut dq: VecDeque<P> = VecDeque::from([s]);
        while let Some(p) = dq.pop_front() {
            let nd = dist.iter().find(|(x, _)| peq(parts, *x, p)).unwrap().1 + 1;
            for &next in successors_get(parts, &succ, p).unwrap() {
                if !dist.iter().any(|(x, _)| peq(parts, *x, next)) {
                    dist.push((next, nd));
                    dq.push_back(next);
                }
            }
        }
        for (ci, &cand) in cpd.iter().enumerate() {
            let d = dist.iter().find(|(x, _)| peq(parts, *x, cand))?.1;
            sums[ci] += d as i64;
        }
    }
    let mut result: Option<usize> = None;
    for ci in 0..cpd.len() {
        let Some(r) = result else {
            result = Some(ci);
            continue;
        };
        let cand = cpd[ci];
        let res = cpd[r];
        let dc = sums[ci].cmp(&sums[r]);
        let (cd, rd) = (set_contains(parts, direct_out, cand), set_contains(parts, direct_out, res));
        if dc.is_lt() || (dc.is_eq() && cd && !rd) || (dc.is_eq() && cd == rd && parts[cand].closed_time < parts[res].closed_time) {
            result = Some(ci);
        }
    }
    result.map(|r| cpd[r])
}

fn build_natural_loop(parts: &[Part], tail: P, header: P, pnw: &Walker) -> PartList {
    super::method::check_deadline();
    let mut lp = PartList::default();
    if tail == header {
        lp.0.push(header);
        return lp;
    }
    lp.insert(parts, header);
    lp.insert(parts, tail);
    let mut stack = vec![tail];
    while let Some(x) = stack.pop() {
        for p in pnw.prev(parts, x) {
            if lp.insert(parts, p) {
                stack.push(p);
            }
        }
    }
    lp
}

fn get_reachable(parts: &[Part], start: P, back_edges: &[Edge], ignored_parts: &[P], pnw: &Walker, ignored_edges: &[Edge]) -> Vec<P> {
    super::method::check_deadline();
    let mut visited: Vec<P> = Vec::new();
    let mut q: VecDeque<P> = VecDeque::from([start]);
    while let Some(part) = q.pop_front() {
        if set_contains(parts, &visited, part) {
            continue;
        }
        visited.push(part);
        for next in pnw.next(parts, part) {
            let e = Edge { from: Some(part), to: next };
            if edges_contain(parts, ignored_edges, &e) || set_contains(parts, ignored_parts, part) {
                continue;
            }
            if edges_contain(parts, back_edges, &e) {
                set_insert(parts, &mut visited, next);
                continue;
            }
            q.push_back(next);
        }
    }
    visited
}

fn part_leads_to_get_prev(parts: &[Part], p1: P, p2: P, back_edges: &[Edge], pnw: &Walker) -> Option<P> {
    super::method::check_deadline();
    if p1 == p2 {
        return None;
    }
    let mut stack = vec![p1];
    let mut visited: Vec<P> = Vec::new();
    while let Some(part) = stack.pop() {
        if set_contains(parts, &visited, part) {
            continue;
        }
        visited.push(part);
        for next in pnw.next(parts, part) {
            if edges_contain(parts, back_edges, &Edge { from: Some(part), to: next }) {
                continue;
            }
            if next == p2 {
                return Some(part);
            }
            stack.push(next);
        }
    }
    None
}

fn get_back_edges(parts: &[Part], first: P, throw_states: &[ThrowState]) -> Vec<Edge> {
    // A back edge is one whose target is on the DFS-tree path to its source; checked afterwards
    // with entry/exit times instead of copying the path for every part.
    let mut canon: HashMap<(i32, i32), P> = HashMap::new();
    let mut key = |p: P| *canon.entry((parts[p].start, parts[p].end)).or_insert(p);
    let mut stack: Vec<Edge> = vec![Edge { from: None, to: first }];
    let mut visited: HashSet<P> = HashSet::new();
    let mut children: HashMap<P, Vec<P>> = HashMap::new();
    let mut candidates: Vec<(Edge, P, P)> = Vec::new();
    let mut root = None;
    while let Some(edge) = stack.pop() {
        let to = key(edge.to);
        if let Some(f) = edge.from {
            candidates.push((edge, key(f), to));
        }
        let part = edge.to;
        if !visited.insert(to) {
            continue;
        }
        match edge.from {
            Some(f) => children.entry(key(f)).or_default().push(to),
            None => root = Some(to),
        }
        for &n in &parts[part].next {
            stack.push(Edge { from: Some(part), to: n });
        }
        for ts in throw_states {
            if ts.throwing.contains(parts, part) {
                stack.push(Edge { from: Some(part), to: ts.target.expect("target") });
            }
        }
    }
    let mut tin: HashMap<P, (usize, usize)> = HashMap::new();
    let mut time = 0;
    if let Some(r) = root {
        let mut dfs: Vec<(P, usize)> = vec![(r, 0)];
        tin.insert(r, (time, usize::MAX));
        while let Some(top) = dfs.last_mut() {
            let n = top.0;
            let kids = children.get(&n).map_or(&[][..], |v| &v[..]);
            if top.1 < kids.len() {
                let c = kids[top.1];
                top.1 += 1;
                time += 1;
                tin.insert(c, (time, usize::MAX));
                dfs.push((c, 0));
            } else {
                time += 1;
                tin.get_mut(&n).unwrap().1 = time;
                dfs.pop();
            }
        }
    }
    let mut back: Vec<Edge> = Vec::new();
    for (edge, from, to) in candidates {
        if let (Some(a), Some(b)) = (tin.get(&to), tin.get(&from)) {
            if a.0 <= b.0 && b.1 <= a.1 {
                edges_insert(parts, &mut back, edge);
            }
        }
    }
    back
}

// GraphPrecontinueDetector.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NodeKind {
    Plain,
    If,
    While,
    DoWhile,
    Joined,
}

struct Node {
    next: Vec<usize>,
    prev: Vec<usize>,
    part: Option<P>,
    parent: Option<usize>,
    _kind: NodeKind,
}

struct Nodes(Vec<Node>);

impl Nodes {
    fn add(&mut self, kind: NodeKind, part: Option<P>) -> usize {
        self.0.push(Node { next: Vec::new(), prev: Vec::new(), part, parent: None, _kind: kind });
        self.0.len() - 1
    }

    fn replace_prevs(&mut self, this: usize, new: usize) {
        for p in self.0[this].prev.clone() {
            for x in self.0[p].next.iter_mut() {
                if *x == this {
                    *x = new;
                }
            }
        }
    }

    fn replace_nexts(&mut self, this: usize, new: usize) {
        for n in self.0[this].next.clone() {
            for x in self.0[n].prev.iter_mut() {
                if *x == this {
                    *x = new;
                }
            }
        }
    }

    fn remove_from_graph(&mut self, this: usize) {
        for p in self.0[this].prev.clone() {
            self.0[p].next.retain(|&x| x != this);
        }
        for n in self.0[this].next.clone() {
            self.0[n].prev.retain(|&x| x != this);
        }
        self.0[this].next.clear();
        self.0[this].prev.clear();
    }
}

fn detect_precontinues(c: &mut LoopCtx, heads: &[P], all_parts: &PartSet, loops: &[usize], throw_states: &[ThrowState]) {
    if !loops.iter().any(|&l| c.arena[l].back_edges.len() == 1) {
        return;
    }
    let parts: &[Part] = c.parts;
    let mut nodes = Nodes(Vec::new());
    let all = all_parts.iter();
    let mut part_to_node: Vec<(P, usize)> = Vec::new();
    for &p in &all {
        let n = nodes.add(NodeKind::Plain, Some(p));
        part_to_node.push((p, n));
    }
    let node_of = |p: P| part_to_node.iter().find(|(x, _)| peq(parts, *x, p)).map(|(_, n)| *n);
    for &p in &all {
        let n = node_of(p).unwrap();
        for &r in &parts[p].refs {
            if parts[r].start < 0 {
                continue;
            }
            if let Some(rn) = node_of(r) {
                nodes.0[n].prev.push(rn);
            }
        }
        for &x in &parts[p].next {
            if let Some(xn) = node_of(x) {
                nodes.0[n].next.push(xn);
            }
        }
    }
    let mut head_nodes: Vec<usize> = heads.iter().filter_map(|&h| node_of(h)).collect();
    loop {
        let mut changed = false;
        if join_nodes_all(&mut nodes, &mut head_nodes) {
            changed = true;
        }
        if check_ifs_all(&mut nodes, &mut head_nodes) {
            changed = true;
        }
        if handle_while_all(&mut nodes, &mut head_nodes) {
            changed = true;
        }
        if !changed {
            break;
        }
    }
    let targets: Vec<P> = throw_states.iter().filter_map(|t| t.target).collect();
    for &l in loops {
        if c.arena[l].back_edges.len() != 1 {
            continue;
        }
        let be = c.arena[l].back_edges.first().unwrap();
        let (cont, brk) = (c.arena[l].cont, c.arena[l].brk);
        let mut node = node_of(be).unwrap();
        let (mut whole, mut in_try, mut more, mut use_pre) = (false, false, false, false);
        if nodes.0[node].parent.is_none() && nodes.0[node].prev.len() == 1 {
            let prev = nodes.0[node].prev[0];
            if nodes.0[prev].next.len() == 2 {
                let other = if nodes.0[prev].next[0] == node { nodes.0[prev].next[1] } else { nodes.0[prev].next[0] };
                if brk.is_some() && nodes.0[other].part == brk {
                    node = prev;
                    use_pre = true;
                }
            }
        }
        if !use_pre && nodes.0[node].next.len() > 1 {
            if nodes.0[node].next.len() == 2 {
                let other = if nodes.0[nodes.0[node].next[0]].part == cont { nodes.0[node].next[1] } else { nodes.0[node].next[0] };
                if nodes.0[other].part != brk && brk.is_some() {
                    more = true;
                }
            } else {
                more = true;
            }
        }
        if let Some(gp) = nodes.0[node].part {
            if pcontains(parts, &targets, gp) {
                in_try = true;
            }
        }
        if !in_try && !more {
            while let Some(pn) = nodes.0[node].parent {
                node = pn;
                let gp = nodes.0[node].part.unwrap();
                if cont.is_some_and(|cc| peq(parts, gp, cc)) {
                    whole = true;
                    break;
                }
                if pcontains(parts, &targets, gp) {
                    in_try = true;
                    break;
                }
            }
        }
        if !whole && !in_try && !more {
            c.arena[l].pre_cont = nodes.0[node].part;
        }
    }
}

fn handle_while_all(nodes: &mut Nodes, heads: &mut [usize]) -> bool {
    let mut visited = Vec::new();
    let mut n = 0;
    for h in heads.iter_mut() {
        *h = handle_while(nodes, *h, &mut visited, &mut n);
    }
    n > 0
}

fn handle_while(nodes: &mut Nodes, node: usize, visited: &mut Vec<usize>, num: &mut i32) -> usize {
    if visited.contains(&node) {
        return node;
    }
    visited.push(node);
    let mut result = node;
    let nd = &nodes.0[node];
    if nd.prev.contains(&node) && nd.next.contains(&node) && nd.next.len() == 2 {
        let part = nd.part;
        let prev: Vec<usize> = nd.prev.iter().copied().filter(|&x| x != node).collect();
        let next: Vec<usize> = nd.next.iter().copied().filter(|&x| x != node).collect();
        let dw = nodes.add(NodeKind::DoWhile, part);
        nodes.0[dw].prev = prev;
        nodes.0[dw].next = next;
        nodes.replace_prevs(node, dw);
        nodes.replace_nexts(node, dw);
        nodes.remove_from_graph(node);
        nodes.0[node].parent = Some(dw);
        result = dw;
        *num += 1;
    } else {
        let nx = nd.next.clone();
        let mut body = None;
        let mut brk = None;
        if nx.len() == 2 && nodes.0[nx[0]].next.len() == 1 && nodes.0[nx[0]].next[0] == node && nodes.0[nx[0]].prev.len() == 1 {
            brk = Some(nx[1]);
            body = Some(nx[0]);
        } else if nx.len() == 2 && nodes.0[nx[1]].next.len() == 1 && nodes.0[nx[1]].next[0] == node && nodes.0[nx[1]].prev.len() == 1 {
            brk = Some(nx[0]);
            body = Some(nx[1]);
        }
        if let (Some(b), Some(br)) = (body, brk) {
            let part = nodes.0[node].part;
            let wn = nodes.add(NodeKind::While, part);
            nodes.0[b].parent = Some(wn);
            nodes.remove_from_graph(b);
            nodes.0[wn].prev = nodes.0[node].prev.clone();
            nodes.replace_prevs(node, wn);
            nodes.replace_nexts(node, wn);
            nodes.0[wn].next.push(br);
            result = wn;
            *num += 1;
        }
    }
    for n in nodes.0[result].next.clone() {
        handle_while(nodes, n, visited, num);
    }
    result
}

fn join_nodes_all(nodes: &mut Nodes, heads: &mut [usize]) -> bool {
    let mut visited = Vec::new();
    let mut n = 0;
    for h in heads.iter_mut() {
        *h = join_nodes(nodes, *h, &mut visited, &mut n);
    }
    n > 0
}

fn join_nodes(nodes: &mut Nodes, node: usize, visited: &mut Vec<usize>, num: &mut i32) -> usize {
    if visited.contains(&node) {
        return node;
    }
    visited.push(node);
    let mut cur = node;
    let mut list = vec![cur];
    while nodes.0[cur].next.len() == 1 && nodes.0[nodes.0[cur].next[0]].prev.len() == 1 && !visited.contains(&nodes.0[cur].next[0]) {
        cur = nodes.0[cur].next[0];
        visited.push(cur);
        list.push(cur);
    }
    let mut result = node;
    if list.len() > 1 {
        let part = nodes.0[node].part;
        let jn = nodes.add(NodeKind::Joined, part);
        let mut next = nodes.0[cur].next.clone();
        for x in next.iter_mut() {
            if *x == node {
                *x = jn;
            }
        }
        let mut prev = nodes.0[node].prev.clone();
        for x in prev.iter_mut() {
            if *x == cur {
                *x = jn;
            }
        }
        nodes.0[jn].next = next;
        nodes.0[jn].prev = prev;
        nodes.replace_prevs(node, jn);
        nodes.replace_nexts(cur, jn);
        for &n in &list {
            nodes.0[n].parent = Some(jn);
            nodes.remove_from_graph(n);
        }
        result = jn;
        *num += 1;
    }
    for n in nodes.0[result].next.clone() {
        join_nodes(nodes, n, visited, num);
    }
    result
}

fn check_ifs_all(nodes: &mut Nodes, heads: &mut [usize]) -> bool {
    let mut visited = Vec::new();
    let mut n = 0;
    for h in heads.iter_mut() {
        *h = check_ifs(nodes, *h, &mut visited, &mut n);
    }
    n > 0
}

fn make_if(nodes: &mut Nodes, node: usize, branches: &[usize], after: usize, visited: &mut Vec<usize>, num: &mut i32) -> usize {
    let part = nodes.0[node].part;
    let ifn = nodes.add(NodeKind::If, part);
    for &b in branches {
        nodes.0[b].parent = Some(ifn);
    }
    for &b in branches {
        nodes.remove_from_graph(b);
    }
    nodes.0[ifn].prev = nodes.0[node].prev.clone();
    nodes.replace_prevs(node, ifn);
    nodes.remove_from_graph(node);
    nodes.0[ifn].next.push(after);
    nodes.0[after].prev.push(ifn);
    *num += 1;
    check_ifs(nodes, after, visited, num);
    ifn
}

fn check_ifs(nodes: &mut Nodes, node: usize, visited: &mut Vec<usize>, num: &mut i32) -> usize {
    if visited.contains(&node) {
        return node;
    }
    visited.push(node);
    let nx = nodes.0[node].next.clone();
    if nx.len() == 2 {
        let (a, b) = (nx[0], nx[1]);
        let (an, bn) = (nodes.0[a].next.clone(), nodes.0[b].next.clone());
        if an.len() == 1 && an[0] == b && nodes.0[a].prev.len() == 1 {
            return make_if(nodes, node, &[a], b, visited, num);
        }
        if bn.len() == 1 && bn[0] == a && nodes.0[b].prev.len() == 1 {
            return make_if(nodes, node, &[b], a, visited, num);
        }
        if an.len() == 1 && bn.len() == 1 && an[0] == bn[0] && nodes.0[a].prev.len() == 1 && nodes.0[b].prev.len() == 1 {
            let after = an[0];
            return make_if(nodes, node, &[a, b], after, visited, num);
        }
    }
    for n in nx {
        check_ifs(nodes, n, visited, num);
    }
    node
}
