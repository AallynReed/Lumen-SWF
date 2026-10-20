//! Turns shape records into per-style paths, porting FFDec's `ShapeExporterBase`:
//! fill0 edges are reversed, edges are grouped by style and chained end to start.
//! Edges can carry a second coordinate set (a morph shape's end state) that follows the
//! same ordering, so start and end paths line up command for command.

use std::collections::HashMap;

use crate::shape::{Fill, LineStyle, Record, Shape};

pub type Pt = (i32, i32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub from: Pt,
    pub ctrl: Option<Pt>,
    pub to: Pt,
}

impl Seg {
    fn reversed(&self) -> Seg {
        Seg { from: self.to, ctrl: self.ctrl, to: self.from }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub seg: Seg,
    pub alt: Option<Seg>,
    pub line: usize,
    pub fill: usize,
}

impl Edge {
    fn reversed(&self, fill: usize) -> Edge {
        Edge { seg: self.seg.reversed(), alt: self.alt.map(|s| s.reversed()), line: self.line, fill }
    }
}

/// One group of fills and lines; a shape starts a new group whenever every style is reset.
#[derive(Debug, Default)]
pub struct Layer {
    pub fills: Vec<Edge>,
    pub lines: Vec<Edge>,
}

pub struct Paths {
    pub fills: Vec<Fill>,
    pub lines: Vec<LineStyle>,
    pub layers: Vec<Layer>,
}

type EdgeMap = HashMap<usize, Vec<Edge>>;

/// Builds paths for `shape`; `end`, when given, must have the same record structure
/// (as `MorphShape::shape_at(0)` and `shape_at(65535)` do).
pub fn build(shape: &Shape, end: Option<&Shape>) -> Paths {
    let mut fills = shape.styles.fills.clone();
    let mut lines = shape.styles.lines.clone();
    let mut layers = Vec::new();
    let (mut fill_off, mut line_off) = (0usize, 0usize);
    let (mut f0, mut f1, mut ln) = (0usize, 0usize, 0usize);
    let (mut x, mut y) = (0i32, 0i32);
    let (mut ex, mut ey) = (0i32, 0i32);
    let mut sub: Vec<Edge> = Vec::new();
    let mut fill_map: EdgeMap = HashMap::new();
    let mut line_map: EdgeMap = HashMap::new();
    let end_records = end.map(|e| &e.records);

    for (i, rec) in shape.records.iter().enumerate() {
        let end_rec = end_records.and_then(|e| e.get(i));
        match rec {
            Record::StyleChange { move_to, fill0, fill1, line, new_styles } => {
                if line.is_some() || fill0.is_some() || fill1.is_some() {
                    process(&mut sub, ln, f0, f1, &mut fill_map, &mut line_map);
                }
                if let Some(ns) = new_styles {
                    fill_off = fills.len();
                    line_off = lines.len();
                    fills.extend(ns.fills.iter().cloned());
                    lines.extend(ns.lines.iter().cloned());
                }
                if *line == Some(0) && *fill0 == Some(0) && *fill1 == Some(0) {
                    layers.push(finish(&mut fill_map, &mut line_map));
                    ln = 0;
                    f0 = 0;
                    f1 = 0;
                } else {
                    let idx = |v: u32, off: usize| if v > 0 { v as usize + off } else { 0 };
                    if let Some(v) = line {
                        ln = idx(*v, line_off);
                    }
                    if let Some(v) = fill0 {
                        f0 = idx(*v, fill_off);
                    }
                    if let Some(v) = fill1 {
                        f1 = idx(*v, fill_off);
                    }
                }
                if let Some((mx, my)) = move_to {
                    x = *mx;
                    y = *my;
                }
                if let Some(Record::StyleChange { move_to: Some((mx, my)), .. }) = end_rec {
                    ex = *mx;
                    ey = *my;
                } else if end_rec.is_some() && move_to.is_some() {
                    ex = x;
                    ey = y;
                }
            }
            Record::Straight { .. } | Record::Curved { .. } => {
                let seg = advance(rec, &mut x, &mut y);
                let alt = end_rec.map(|r| advance(r, &mut ex, &mut ey));
                sub.push(Edge { seg, alt, line: ln, fill: f1 });
            }
        }
    }
    process(&mut sub, ln, f0, f1, &mut fill_map, &mut line_map);
    layers.push(finish(&mut fill_map, &mut line_map));
    layers.retain(|l| !l.fills.is_empty() || !l.lines.is_empty());
    Paths { fills, lines, layers }
}

fn advance(rec: &Record, x: &mut i32, y: &mut i32) -> Seg {
    let from = (*x, *y);
    match *rec {
        Record::Straight { dx, dy } => {
            *x += dx;
            *y += dy;
            Seg { from, ctrl: None, to: (*x, *y) }
        }
        Record::Curved { cdx, cdy, adx, ady } => {
            let ctrl = (*x + cdx, *y + cdy);
            *x = ctrl.0 + adx;
            *y = ctrl.1 + ady;
            Seg { from, ctrl: Some(ctrl), to: (*x, *y) }
        }
        _ => Seg { from, ctrl: None, to: from },
    }
}

fn process(sub: &mut Vec<Edge>, ln: usize, f0: usize, f1: usize, fill_map: &mut EdgeMap, line_map: &mut EdgeMap) {
    if f0 != 0 {
        let p = fill_map.entry(f0).or_default();
        p.extend(sub.iter().rev().map(|e| e.reversed(f0)));
    }
    if f1 != 0 {
        fill_map.entry(f1).or_default().extend(sub.iter().copied());
    }
    if ln != 0 {
        line_map.entry(ln).or_default().extend(sub.iter().copied());
    }
    sub.clear();
}

fn finish(fill_map: &mut EdgeMap, line_map: &mut EdgeMap) -> Layer {
    Layer { fills: flatten(std::mem::take(fill_map)), lines: flatten(std::mem::take(line_map)) }
}

fn flatten(map: EdgeMap) -> Vec<Edge> {
    let mut keys: Vec<usize> = map.keys().copied().collect();
    keys.sort_unstable();
    let mut map = map;
    let mut out = Vec::new();
    for k in keys {
        out.extend(chain(map.remove(&k).unwrap()));
    }
    out
}

const NONE: usize = usize::MAX;

/// FFDec's `cleanEdgeMap` for one style: reorders edges so each starts where the
/// previous ended, reversing an edge when only its end point connects.
fn chain(mut edges: Vec<Edge>) -> Vec<Edge> {
    let n = edges.len();
    let mut next: Vec<usize> = (1..=n).map(|i| if i == n { NONE } else { i }).collect();
    let mut prev: Vec<usize> = (0..n).map(|i| if i == 0 { NONE } else { i - 1 }).collect();
    let mut head = if n == 0 { NONE } else { 0 };
    let mut starts: HashMap<Pt, Vec<usize>> = HashMap::new();
    let mut ends: HashMap<Pt, Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        starts.entry(e.seg.from).or_default().push(i);
        ends.entry(e.seg.to).or_default().push(i);
    }
    let drop = |m: &mut HashMap<Pt, Vec<usize>>, k: Pt, id: usize| {
        if let Some(v) = m.get_mut(&k) {
            if let Some(p) = v.iter().position(|&x| x == id) {
                v.remove(p);
            }
            if v.is_empty() {
                m.remove(&k);
            }
        }
    };
    let mut out = Vec::with_capacity(n);
    let mut cursor = head;
    let mut last: Option<Pt> = None;
    while head != NONE {
        if cursor == NONE {
            cursor = head;
        }
        let e = edges[cursor];
        if let Some(to) = last {
            if to != e.seg.from {
                if let Some(&id) = starts.get(&to).and_then(|v| v.first()) {
                    cursor = id;
                } else if let Some(&id) = ends.get(&to).and_then(|v| v.first()) {
                    let old = edges[id];
                    let r = old.reversed(old.fill);
                    drop(&mut starts, old.seg.from, id);
                    drop(&mut ends, old.seg.to, id);
                    starts.entry(r.seg.from).or_default().push(id);
                    ends.entry(r.seg.to).or_default().push(id);
                    edges[id] = r;
                    cursor = id;
                } else {
                    cursor = head;
                    last = None;
                }
                continue;
            }
        }
        let after = next[cursor];
        if prev[cursor] != NONE {
            next[prev[cursor]] = after;
        } else {
            head = after;
        }
        if after != NONE {
            prev[after] = prev[cursor];
        }
        drop(&mut starts, e.seg.from, cursor);
        drop(&mut ends, e.seg.to, cursor);
        out.push(e);
        last = Some(e.seg.to);
        cursor = after;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::Styles;
    use crate::types::Rgba;

    fn sc(mv: Option<Pt>, f0: Option<u32>, f1: Option<u32>) -> Record {
        Record::StyleChange { move_to: mv, fill0: f0, fill1: f1, line: None, new_styles: None }
    }

    #[test]
    fn fill0_and_fill1_edges_close_into_one_path() {
        // A square drawn as two halves: top/right with fill1, then bottom/left with fill0 reversed.
        let shape = Shape {
            styles: Styles { fills: vec![Fill::Solid(Rgba::BLACK)], lines: vec![] },
            records: vec![
                sc(Some((0, 0)), None, Some(1)),
                Record::Straight { dx: 100, dy: 0 },
                Record::Straight { dx: 0, dy: 100 },
                sc(Some((0, 0)), Some(1), Some(0)),
                Record::Straight { dx: 0, dy: 100 },
                Record::Straight { dx: 100, dy: 0 },
            ],
        };
        let p = build(&shape, None);
        assert_eq!(p.layers.len(), 1);
        let f = &p.layers[0].fills;
        assert_eq!(f.len(), 4);
        for w in f.windows(2) {
            assert_eq!(w[0].seg.to, w[1].seg.from);
        }
        assert_eq!(f[3].seg.to, f[0].seg.from);
    }
}
