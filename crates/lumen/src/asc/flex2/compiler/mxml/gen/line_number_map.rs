//! Ports of `flex2.compiler.util.LineNumberMap` and `DualModeLineNumberMap`, which the MXML codegen fills through
//! `VelocityUtil`.

use std::cell::RefCell;

#[derive(Clone, Copy, Debug)]
struct Range {
    start: i32,
    extent: i32,
}

impl Range {
    /// `Range.compareTo`: containment counts as equality.
    fn compare_to(&self, e: &Range) -> i32 {
        if self.start < e.start {
            -1
        } else if self.start + self.extent > e.start + e.extent {
            1
        } else {
            0
        }
    }
}

/// A node of the red-black tree `java.util.TreeMap` keeps.
#[derive(Clone, Debug)]
struct Entry {
    key: Range,
    value: i32,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    black: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LineNumberMap {
    old_name: Option<String>,
    new_name: Option<String>,
    /// `TreeMap<Range, Integer>`: its comparator treats containment as equality, so which entry a lookup finds depends on
    /// the tree's shape, which `put` builds as `TreeMap.put` and `fixAfterInsertion` do.
    entries: Vec<Entry>,
    root: Option<usize>,
}

impl LineNumberMap {
    pub fn new(name: Option<&str>, new_name: Option<&str>) -> LineNumberMap {
        LineNumberMap { old_name: name.map(Into::into), new_name: new_name.map(Into::into), entries: Vec::new(), root: None }
    }

    pub fn set_new_name(&mut self, new_name: &str) {
        self.new_name = Some(new_name.to_string());
    }

    pub fn get_old_name(&self) -> Option<&str> {
        self.old_name.as_deref()
    }

    pub fn get_new_name(&self) -> Option<&str> {
        self.new_name.as_deref()
    }

    /// `TreeMap.getEntry(key)`.
    fn get_entry(&self, key: &Range) -> Option<usize> {
        let mut p = self.root;
        while let Some(i) = p {
            let cmp = key.compare_to(&self.entries[i].key);
            p = match cmp {
                c if c < 0 => self.entries[i].left,
                c if c > 0 => self.entries[i].right,
                _ => return Some(i),
            };
        }
        None
    }

    pub fn put(&mut self, old_start: i32, new_start: i32, extent: i32) {
        let key = Range { start: new_start, extent };
        let value = new_start - old_start;
        let Some(mut t) = self.root else {
            self.entries.push(Entry { key, value, left: None, right: None, parent: None, black: true });
            self.root = Some(self.entries.len() - 1);
            return;
        };
        loop {
            let cmp = key.compare_to(&self.entries[t].key);
            let next = if cmp < 0 {
                self.entries[t].left
            } else if cmp > 0 {
                self.entries[t].right
            } else {
                self.entries[t].value = value;
                return;
            };
            match next {
                Some(n) => t = n,
                None => {
                    self.entries.push(Entry { key, value, left: None, right: None, parent: Some(t), black: true });
                    let e = self.entries.len() - 1;
                    if cmp < 0 {
                        self.entries[t].left = Some(e);
                    } else {
                        self.entries[t].right = Some(e);
                    }
                    self.fix_after_insertion(e);
                    return;
                }
            }
        }
    }

    fn color_of(&self, p: Option<usize>) -> bool {
        p.is_none_or(|i| self.entries[i].black)
    }

    fn parent_of(&self, p: Option<usize>) -> Option<usize> {
        p.and_then(|i| self.entries[i].parent)
    }

    fn left_of(&self, p: Option<usize>) -> Option<usize> {
        p.and_then(|i| self.entries[i].left)
    }

    fn right_of(&self, p: Option<usize>) -> Option<usize> {
        p.and_then(|i| self.entries[i].right)
    }

    fn set_color(&mut self, p: Option<usize>, black: bool) {
        if let Some(i) = p {
            self.entries[i].black = black;
        }
    }

    fn rotate_left(&mut self, p: Option<usize>) {
        let Some(p) = p else { return };
        let r = self.entries[p].right.unwrap();
        self.entries[p].right = self.entries[r].left;
        if let Some(rl) = self.entries[r].left {
            self.entries[rl].parent = Some(p);
        }
        self.entries[r].parent = self.entries[p].parent;
        match self.entries[p].parent {
            None => self.root = Some(r),
            Some(pp) if self.entries[pp].left == Some(p) => self.entries[pp].left = Some(r),
            Some(pp) => self.entries[pp].right = Some(r),
        }
        self.entries[r].left = Some(p);
        self.entries[p].parent = Some(r);
    }

    fn rotate_right(&mut self, p: Option<usize>) {
        let Some(p) = p else { return };
        let l = self.entries[p].left.unwrap();
        self.entries[p].left = self.entries[l].right;
        if let Some(lr) = self.entries[l].right {
            self.entries[lr].parent = Some(p);
        }
        self.entries[l].parent = self.entries[p].parent;
        match self.entries[p].parent {
            None => self.root = Some(l),
            Some(pp) if self.entries[pp].right == Some(p) => self.entries[pp].right = Some(l),
            Some(pp) => self.entries[pp].left = Some(l),
        }
        self.entries[l].right = Some(p);
        self.entries[p].parent = Some(l);
    }

    /// `TreeMap.fixAfterInsertion(x)`.
    fn fix_after_insertion(&mut self, x: usize) {
        let mut x = Some(x);
        self.set_color(x, false);
        while x.is_some() && x != self.root && !self.color_of(self.parent_of(x)) {
            let px = self.parent_of(x);
            let ppx = self.parent_of(px);
            if px == self.left_of(ppx) {
                let y = self.right_of(ppx);
                if !self.color_of(y) {
                    self.set_color(px, true);
                    self.set_color(y, true);
                    self.set_color(ppx, false);
                    x = ppx;
                } else {
                    if x == self.right_of(px) {
                        x = px;
                        self.rotate_left(x);
                    }
                    let px = self.parent_of(x);
                    let ppx = self.parent_of(px);
                    self.set_color(px, true);
                    self.set_color(ppx, false);
                    self.rotate_right(ppx);
                }
            } else {
                let y = self.left_of(ppx);
                if !self.color_of(y) {
                    self.set_color(px, true);
                    self.set_color(y, true);
                    self.set_color(ppx, false);
                    x = ppx;
                } else {
                    if x == self.left_of(px) {
                        x = px;
                        self.rotate_right(x);
                    }
                    let px = self.parent_of(x);
                    let ppx = self.parent_of(px);
                    self.set_color(px, true);
                    self.set_color(ppx, false);
                    self.rotate_left(ppx);
                }
            }
        }
        let r = self.root;
        self.set_color(r, true);
    }

    /// `get(newLine)`: the original line, 0 when unmapped.
    pub fn get(&self, new_line: i32) -> i32 {
        match self.get_entry(&Range { start: new_line, extent: 1 }) {
            Some(i) => new_line - self.entries[i].value,
            None => 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct DualModeLineNumberMap {
    pub map: RefCell<LineNumberMap>,
    temp: RefCell<LineNumberMap>,
}

impl DualModeLineNumberMap {
    pub fn new(name: Option<&str>, new_name: Option<&str>) -> DualModeLineNumberMap {
        DualModeLineNumberMap { map: RefCell::new(LineNumberMap::new(name, new_name)), temp: RefCell::new(LineNumberMap::new(None, None)) }
    }

    /// `put(oldStart, newStart, extent, temporary)`.
    pub fn put(&self, old_start: i32, new_start: i32, extent: i32, temporary: bool) {
        if temporary { &self.temp } else { &self.map }.borrow_mut().put(old_start, new_start, extent);
    }

    pub fn flush_temp(&self) {
        *self.temp.borrow_mut() = LineNumberMap::new(None, None);
    }

    pub fn get(&self, new_line: i32) -> i32 {
        let temp_line = self.temp.borrow().get(new_line);
        if temp_line > 0 {
            temp_line
        } else {
            self.map.borrow().get(new_line)
        }
    }

    pub fn get_old_name(&self) -> Option<String> {
        self.map.borrow().get_old_name().map(Into::into)
    }

    pub fn get_new_name(&self) -> Option<String> {
        self.map.borrow().get_new_name().map(Into::into)
    }

    pub fn set_new_name(&self, new_name: &str) {
        self.map.borrow_mut().set_new_name(new_name);
    }
}
