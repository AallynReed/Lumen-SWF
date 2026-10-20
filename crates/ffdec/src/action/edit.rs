//! Action list edits that keep jumps and containers pointing at the same actions:
//! FFDec's `ActionListReader.removeAction`/`addAction`.

use std::collections::HashMap;

use super::{Action, ActionList, Op};

/// Jump targets and container ends as action ids, plus the ids of the current list.
struct Links {
    ids: Vec<usize>,
    next_id: usize,
    jumps: HashMap<usize, Option<usize>>,
    lasts: Vec<(usize, Vec<Option<usize>>)>,
}

fn near_address(actions: &[Action], address: u64) -> Option<usize> {
    match actions.binary_search_by_key(&address, |a| a.address) {
        Ok(i) => Some(i),
        Err(i) => i.checked_sub(1),
    }
}

fn by_address(actions: &[Action]) -> HashMap<u64, usize> {
    let mut m = HashMap::new();
    for (i, a) in actions.iter().enumerate() {
        m.entry(a.address).or_insert(i);
    }
    m
}

impl Links {
    fn of(list: &ActionList, extra: Option<&Action>) -> Links {
        let actions = &list.actions;
        let ids: Vec<usize> = (0..actions.len()).collect();
        let mut lasts = Vec::new();
        for (i, a) in actions.iter().enumerate() {
            if let Some(sizes) = a.container_sizes() {
                let mut end = a.address + a.bytes_len(list.charset);
                let mut l = Vec::new();
                for s in sizes {
                    end += s;
                    l.push(end.checked_sub(1).and_then(|e| near_address(actions, e)));
                }
                lasts.push((i, l));
            }
        }
        let mut all: Vec<&Action> = actions.iter().collect();
        if let Some(e) = extra {
            all.push(e);
        }
        let mut amap: HashMap<u64, usize> = HashMap::new();
        for (i, a) in all.iter().enumerate() {
            amap.entry(a.address).or_insert(i);
        }
        let mut jumps = HashMap::new();
        for (i, a) in all.iter().enumerate() {
            if let Some(t) = a.target() {
                jumps.insert(i, amap.get(&t).copied());
            } else if let Some(n) = a.store_size() {
                let mut cur = Some(i);
                for _ in 0..=n {
                    let Some(c) = cur else { break };
                    cur = amap.get(&(all[c].address + all[c].total_len())).copied();
                }
                jumps.insert(i, cur);
            }
        }
        Links { ids, next_id: all.len(), jumps, lasts }
    }

    fn pos(&self, id: usize) -> Option<usize> {
        self.ids.iter().position(|&x| x == id)
    }

    /// `updateActionLengths`, `updateAddresses`, `updateJumps`, `updateActionStores` and
    /// `updateContainerSizes`.
    fn apply(&self, list: &mut ActionList, start_ip: u64, end_address: u64) {
        let cs = list.charset;
        let n = list.actions.len();
        for a in list.actions.iter_mut() {
            a.length = a.content_len(cs);
        }
        let mut address = start_ip;
        for (i, a) in list.actions.iter_mut().enumerate() {
            a.address = address;
            address += if i != n - 1 && matches!(a.op, Op::End) { 5 } else { a.total_len() };
        }
        for i in 0..n {
            if i != n - 1 && matches!(list.actions[i].op, Op::End) {
                let addr = list.actions[i].address;
                let mut j = Action { code: 0x99, length: 2, address: addr, file_offset: list.actions[i].file_offset, ignored: false, op: Op::Jump { offset: 0, deobfuscate: true } };
                j.set_offset((end_address as i64 - addr as i64 - 5) as i32);
                list.actions[i] = j;
            } else if list.actions[i].target().is_some() {
                let base = list.actions[i].address as i64 + list.actions[i].total_len() as i64;
                let to = match self.jumps.get(&self.ids[i]).copied().flatten().and_then(|t| self.pos(t)) {
                    Some(t) => list.actions[t].address as i64,
                    None => end_address as i64,
                };
                list.actions[i].set_offset((to - base) as i32);
            }
        }
        let amap = by_address(&list.actions);
        for i in 0..n {
            if list.actions[i].store_size().is_some() {
                let stop = self.jumps.get(&self.ids[i]).copied().flatten().and_then(|t| self.pos(t));
                let mut cur = i;
                let mut count = 0;
                while let Some(&x) = amap.get(&(list.actions[cur].address + list.actions[cur].total_len())) {
                    if Some(x) == stop {
                        break;
                    }
                    count += 1;
                    cur = x;
                }
                list.actions[i].set_store_size(count);
            }
        }
        for (cid, lasts) in &self.lasts {
            let Some(p) = self.pos(*cid) else { continue };
            let mut start = list.actions[p].address + list.actions[p].bytes_len(cs);
            for (j, last) in lasts.iter().enumerate() {
                let Some(l) = last.and_then(|l| self.pos(l)) else { continue };
                let size = (list.actions[l].address + list.actions[l].total_len()).saturating_sub(start);
                list.actions[p].set_container_size(j, size);
                start += size;
            }
        }
    }
}

impl ActionList {
    /// `ActionListReader.removeAction(actions, index, true)`.
    pub fn remove_action(&mut self, index: usize) {
        if index >= self.actions.len() {
            return;
        }
        let start_ip = self.actions[0].address;
        let last = self.actions.last().unwrap();
        let end_address = last.address + last.total_len();
        let mut links = Links::of(self, None);
        let prev = index.checked_sub(1);
        let next = (index + 1 < self.actions.len()).then_some(index + 1);
        for (_, lasts) in links.lasts.iter_mut() {
            for l in lasts.iter_mut() {
                if *l == Some(index) {
                    *l = prev;
                }
            }
        }
        for t in links.jumps.values_mut() {
            if *t == Some(index) {
                *t = next;
            }
        }
        links.lasts.retain(|(c, _)| *c != index);
        links.jumps.remove(&index);
        self.actions.remove(index);
        links.ids.remove(index);
        links.apply(self, start_ip, end_address);
    }

    /// `ActionList.removeAction(index, count)`.
    pub fn remove_actions(&mut self, index: usize, count: usize) {
        let count = count.min(self.actions.len().saturating_sub(index));
        for _ in 0..count {
            self.remove_action(index);
        }
    }

    /// `ActionListReader.addAction(actions, index, action, false, false)`.
    pub fn add_action(&mut self, index: usize, action: Action) {
        if index > self.actions.len() || self.actions.is_empty() {
            return;
        }
        let start_ip = self.actions[0].address;
        let last = self.actions.last().unwrap();
        if !matches!(last.op, Op::End) {
            let mut e = Action::new(0, Op::End);
            e.address = last.address + last.total_len();
            self.actions.push(e);
        }
        let end_address = self.actions.last().unwrap().address;
        let mut links = Links::of(self, Some(&action));
        let new_id = links.next_id - 1;
        self.actions.insert(index, action);
        links.ids.insert(index, new_id);
        links.apply(self, start_ip, end_address);
    }

    /// `ActionList.getReferencesFor`: jumps, stores and containers that point at `target`.
    pub fn references_for(&self, target: usize) -> Vec<usize> {
        let ta = self.actions[target].address;
        let mut out = Vec::new();
        for (i, a) in self.actions.iter().enumerate() {
            match &a.op {
                Op::Jump { .. } | Op::If { .. } => {
                    if a.target() == Some(ta) {
                        out.push(i);
                    }
                }
                _ if a.store_size().is_some() => {
                    if target == i + a.store_size().unwrap() {
                        out.push(i);
                    }
                }
                _ => {
                    if let Some(sizes) = a.container_sizes() {
                        let mut r = a.address + a.total_len();
                        for s in sizes {
                            r += s;
                            if r == ta {
                                out.push(i);
                                break;
                            }
                        }
                    }
                }
            }
        }
        out
    }
}
