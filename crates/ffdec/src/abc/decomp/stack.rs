//! FFDec's `TranslateStack`: the expression stack plus deferred output of a translated block.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use super::item::*;

#[derive(Clone)]
pub struct Stack {
    pub items: Vec<It>,
    pop: Option<It>,
    has_path: bool,
    connected: Option<List>,
    pub output_queue: Vec<It>,
    pub max_temp: Rc<Cell<i32>>,
    marks: HashMap<String, It>,
    pub empty_pop_count: i32,
    pub no_push_to_output: bool,
}

impl Stack {
    pub fn new(has_path: bool, max_temp: Rc<Cell<i32>>) -> Stack {
        Stack { items: Vec::new(), pop: None, has_path, connected: None, output_queue: Vec::new(), max_temp, marks: HashMap::new(), empty_pop_count: 0, no_push_to_output: false }
    }

    /// Java's `clone()`: shares items, copies the output queue.
    pub fn duplicate(&self) -> Stack {
        self.clone()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.output_queue.clear();
    }

    pub fn set_connected_output(&mut self, output: Option<List>) {
        self.connected = output;
    }

    pub fn connected(&self) -> Option<List> {
        self.connected.clone()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn set_mark(&mut self, name: &str, v: It) {
        self.marks.insert(name.into(), v);
    }

    pub fn get_mark(&self, name: &str) -> Option<It> {
        self.marks.get(name).cloned()
    }

    fn get_pop(&mut self) -> It {
        self.pop.get_or_insert_with(|| mk(K::Pop, None, None, prec::PRIMARY, None)).clone()
    }

    pub fn push(&mut self, it: It) {
        if !self.output_queue.is_empty() {
            if let Some(c) = self.connected.clone() {
                self.finish_block(&c);
            }
        }
        self.items.push(it);
    }

    pub fn get(&mut self, i: usize) -> It {
        if self.has_path && i >= self.items.len() {
            return self.get_pop();
        }
        self.items[i].clone()
    }

    pub fn peek(&mut self) -> It {
        if self.has_path && self.items.is_empty() {
            return self.get_pop();
        }
        self.items.last().expect("peek on empty stack").clone()
    }

    /// `peek(index)`: the `index`-th item from the top, 1-based.
    pub fn peek_n(&mut self, n: usize) -> It {
        if self.has_path && n > self.items.len() {
            return self.get_pop();
        }
        self.items[self.items.len() - n].clone()
    }

    pub fn set(&mut self, i: usize, it: It) {
        self.items[i] = it;
    }

    pub fn remove(&mut self, i: usize) -> It {
        self.items.remove(i)
    }

    pub fn pop(&mut self) -> It {
        if !self.output_queue.is_empty() {
            let old = std::mem::take(&mut self.output_queue);
            let c = self.connected.clone().expect("connected output");
            self.finish_block(&c);
            c.borrow_mut().extend(old);
        }
        if self.items.is_empty() {
            if let Some(c) = self.connected.clone() {
                self.move_to_stack(&c);
                if !self.items.is_empty() {
                    return self.pop();
                }
                let n = c.borrow().len();
                for i in (0..n).rev() {
                    let item = c.borrow()[i].clone();
                    if is::block(&item) {
                        break;
                    }
                    if !is::push(&item) {
                        continue;
                    }
                    let pv = item.borrow().val();
                    let (src, ls) = (pv.borrow().src, pv.borrow().line_start);
                    let k = pv.borrow().k.clone();
                    match k {
                        K::SetTemporary { temp, .. } => {
                            c.borrow_mut()[i] = pv.clone();
                            return mk(K::Temporary { temp }, src, ls, prec::PRIMARY, Some(pv));
                        }
                        K::DuplicateSource { .. } => {
                            c.borrow_mut().remove(i);
                            return pv;
                        }
                        K::Duplicate { temp } => {
                            c.borrow_mut().remove(i);
                            return mk(K::Temporary { temp }, src, ls, prec::PRIMARY, Some(pv));
                        }
                        K::Temporary { .. } => {
                            c.borrow_mut().remove(i);
                            return pv;
                        }
                        _ => {
                            let temp = self.max_temp.get() + 1;
                            self.max_temp.set(temp);
                            c.borrow_mut()[i] = mk(K::SetTemporary { temp, declaration: false, ref_count: 1, suffix: "push".into() }, src, ls, prec::ASSIGNMENT, Some(pv.clone()));
                            return mk(K::Temporary { temp }, src, ls, prec::PRIMARY, Some(pv));
                        }
                    }
                }
            }
        }
        if self.has_path && self.items.is_empty() {
            let old = self.get_pop();
            self.pop = None;
            self.empty_pop_count += 1;
            return old;
        }
        self.items.pop().expect("pop on empty stack")
    }

    pub fn move_to_stack(&mut self, output: &List) {
        if !self.items.is_empty() {
            return;
        }
        let mut i = output.borrow().len();
        while i > 0 && is::push(&output.borrow()[i - 1]) {
            i -= 1;
        }
        while i < output.borrow().len() {
            let pi = output.borrow_mut().remove(i);
            let v = pi.borrow().val();
            self.push(v);
        }
    }

    pub fn add_to_output(&mut self, it: It) {
        let top_special = self.items.last().is_some_and(|p| matches!(p.borrow().k, K::ExceptionItem(_) | K::NewActivation));
        if self.items.is_empty() || top_special {
            self.connected.clone().expect("connected output").borrow_mut().push(it);
            return;
        }
        let exit = is::exit(&it);
        self.output_queue.push(it);
        if exit {
            let c = self.connected.clone().expect("connected output");
            self.finish_block(&c);
        }
    }

    pub fn finish_block(&mut self, output: &List) {
        if self.connected.is_none() {
            return;
        }
        if self.no_push_to_output {
            let q = std::mem::take(&mut self.output_queue);
            output.borrow_mut().extend(q);
            return;
        }
        let mut out = output.borrow_mut();
        let mut clen = out.len();
        let mut is_exit = self.output_queue.last().is_some_and(is::exit);
        if clen > 0 && is::script_end(&out[clen - 1]) {
            clen -= 1;
            is_exit = true;
        }
        if clen > 0 && is::exit(&out[clen - 1]) {
            is_exit = true;
            clen -= 1;
        }
        if clen > 0 && is::break_(&out[clen - 1]) {
            clen -= 1;
        }
        if clen > 0 && is::continue_(&out[clen - 1]) {
            clen -= 1;
        }
        while let Some(p) = self.items.pop() {
            if !is::pop(&p) {
                if is_exit {
                    out.insert(clen, p);
                } else {
                    out.insert(clen, push_item(p));
                }
            }
        }
        out.extend(self.output_queue.drain(..));
    }

    pub fn allow_swap(&mut self, output: &List) {
        if !self.items.is_empty() {
            return;
        }
        let (pi2v, tiv) = {
            let out = output.borrow();
            let n = out.len();
            if n < 3 || !is::push(&out[n - 1]) || !is::push(&out[n - 2]) || !is::set_temporary(&out[n - 3]) {
                return;
            }
            let pi1v = out[n - 1].borrow().val();
            let K::Temporary { temp: ti } = pi1v.borrow().k else { return };
            let K::SetTemporary { temp: st, ref suffix, .. } = out[n - 3].borrow().k else { return };
            if suffix != "swap" || st != ti {
                return;
            }
            let a = out[n - 2].borrow().val();
            let b = pi1v.borrow().val();
            (a, b)
        };
        {
            let mut out = output.borrow_mut();
            let n = out.len();
            out.truncate(n - 3);
        }
        self.push(pi2v);
        self.push(tiv);
    }
}
