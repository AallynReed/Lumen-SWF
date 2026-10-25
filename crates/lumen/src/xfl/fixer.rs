//! Shape fixers: split a
//! shape into per-style paths, put fills on the side Animate expects, split crossing edges,
//! and join everything back into records with fractional coordinates.
//!
//! Java's `Area` is approximated: polygon containment and the morph path distance use the
//! flattened outlines instead of Java2D's area algebra.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use super::geom::{p, BezierEdge, Path, Seg, P, R};
use crate::render::paths;
use crate::shape::{Fill, LineStyle, Record, Shape, Styles};

/// `ShapeRecordAdvanced`.
#[derive(Debug, Clone)]
pub enum Adv {
    Style(StyleAdv),
    Straight { dx: f64, dy: f64 },
    Curved { cdx: f64, cdy: f64, adx: f64, ady: f64 },
}

#[derive(Debug, Clone, Default)]
pub struct StyleAdv {
    pub move_to: Option<(f64, f64)>,
    pub fill0: Option<i32>,
    pub fill1: Option<i32>,
    pub line: Option<i32>,
    pub new_styles: Option<Styles>,
}

impl Adv {
    pub fn change_x(&self, x: f64) -> f64 {
        match self {
            Adv::Style(s) => s.move_to.map_or(x, |m| m.0),
            Adv::Straight { dx, .. } => x + dx,
            Adv::Curved { cdx, adx, .. } => x + cdx + adx,
        }
    }

    pub fn change_y(&self, y: f64) -> f64 {
        match self {
            Adv::Style(s) => s.move_to.map_or(y, |m| m.1),
            Adv::Straight { dy, .. } => y + dy,
            Adv::Curved { cdy, ady, .. } => y + cdy + ady,
        }
    }
}

/// The paths of a shape, one per run of records with the same styles.
#[derive(Default)]
struct Split {
    shapes: Vec<Vec<BezierEdge>>,
    fs0: Vec<i32>,
    fs1: Vec<i32>,
    ls: Vec<i32>,
    layers: Vec<i32>,
    fill_layers: Vec<Vec<Fill>>,
    line_layers: Vec<Vec<LineStyle>>,
    styles_layers: Vec<Styles>,
}

impl Split {
    fn remove(&mut self, i: usize) {
        self.shapes.remove(i);
        self.fs0.remove(i);
        self.fs1.remove(i);
        self.ls.remove(i);
        self.layers.remove(i);
    }
}

fn split_to_layers(records: &[Record]) -> Split {
    let mut s = Split::default();
    let mut cur: Vec<BezierEdge> = Vec::new();
    let (mut f0, mut f1, mut ln, mut layer) = (0i32, 0i32, 0i32, -1i32);
    let (mut x, mut y) = (0i32, 0i32);
    let flush = |s: &mut Split, cur: &mut Vec<BezierEdge>, f0: i32, f1: i32, ln: i32, layer: i32| {
        if !cur.is_empty() {
            s.shapes.push(std::mem::take(cur));
            s.fs0.push(f0);
            s.fs1.push(f1);
            s.ls.push(ln);
            s.layers.push(layer);
        }
    };
    for rec in records {
        match rec {
            Record::StyleChange { move_to, fill0, fill1, line, new_styles } => {
                if move_to.is_some() || new_styles.is_some() || fill0.is_some() || fill1.is_some() || line.is_some() {
                    flush(&mut s, &mut cur, f0, f1, ln, layer);
                }
                if let Some(ns) = new_styles {
                    layer += 1;
                    f0 = 0;
                    f1 = 0;
                    ln = 0;
                    s.fill_layers.push(ns.fills.clone());
                    s.line_layers.push(ns.lines.clone());
                    s.styles_layers.push((**ns).clone());
                }
                if let Some(v) = fill0 {
                    f0 = *v as i32;
                }
                if let Some(v) = fill1 {
                    f1 = *v as i32;
                }
                if let Some(v) = line {
                    ln = *v as i32;
                }
                if let Some((mx, my)) = move_to {
                    x = *mx;
                    y = *my;
                }
            }
            Record::Straight { dx, dy } => {
                cur.push(BezierEdge::line(x as f64, y as f64, (x + dx) as f64, (y + dy) as f64));
                x += dx;
                y += dy;
            }
            Record::Curved { cdx, cdy, adx, ady } => {
                let (cx, cy) = (x + cdx, y + cdy);
                let (ax, ay) = (cx + adx, cy + ady);
                cur.push(BezierEdge::quad(x as f64, y as f64, cx as f64, cy as f64, ax as f64, ay as f64));
                x = ax;
                y = ay;
            }
        }
    }
    flush(&mut s, &mut cur, f0, f1, ln, layer);
    s
}

fn combine_layers(s: &Split) -> Vec<Adv> {
    let mut ret = Vec::new();
    let mut layer = -1;
    for (i, bes) in s.shapes.iter().enumerate() {
        if bes.is_empty() {
            continue;
        }
        let b0 = bes[0].points[0];
        let (mut dx, mut dy) = (b0.x, b0.y);
        let mut scr = StyleAdv { move_to: Some((dx, dy)), ..Default::default() };
        let nl = s.layers[i];
        if nl != layer {
            scr.new_styles = Some(s.styles_layers.get(nl as usize).cloned().unwrap_or_default());
        }
        layer = nl;
        scr.fill0 = Some(s.fs0[i]);
        scr.fill1 = Some(s.fs1[i]);
        scr.line = Some(s.ls[i]);
        ret.push(Adv::Style(scr));
        for be in bes {
            let b = be.begin();
            if !(b.x == dx && b.y == dy) {
                ret.push(Adv::Style(StyleAdv { move_to: Some((b.x, b.y)), ..Default::default() }));
            }
            let e = be.end();
            dx = e.x;
            dy = e.y;
            ret.push(to_adv(be));
        }
    }
    ret
}

fn to_adv(be: &BezierEdge) -> Adv {
    let q = &be.points;
    if q.len() == 2 {
        Adv::Straight { dx: q[1].x - q[0].x, dy: q[1].y - q[0].y }
    } else {
        Adv::Curved { cdx: q[1].x - q[0].x, cdy: q[1].y - q[0].y, adx: q[2].x - q[1].x, ady: q[2].y - q[1].y }
    }
}

struct Wrapper {
    be: BezierEdge,
    layer: i32,
    shape: usize,
    edge: usize,
}

/// `ShapeFixer.Sweep`: candidate pairs by overlapping x and y ranges, tested exactly.
fn sweep(list: &[&Wrapper], ignore: &HashSet<usize>) -> IndexMap<usize, Vec<f64>> {
    let mut events: Vec<(f64, u8, usize)> = Vec::new();
    for (i, w) in list.iter().enumerate() {
        if ignore.contains(&i) {
            continue;
        }
        let b = w.be.bbox();
        events.push((b.min_x(), 0, i));
        events.push((b.max_x(), 1, i));
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut status: Vec<usize> = Vec::new();
    let mut splits: IndexMap<usize, Vec<f64>> = IndexMap::new();
    for (_, kind, i) in events {
        if kind == 0 {
            let max_y = list[i].be.bbox().max_y();
            let mut cands: Vec<usize> = status.iter().copied().filter(|&j| list[j].be.bbox().min_y() <= max_y).collect();
            cands.sort_by(|&a, &b| list[a].be.bbox().min_y().total_cmp(&list[b].be.bbox().min_y()).then(a.cmp(&b)));
            for j in cands {
                let (mut t1, mut t2, mut pts) = (Vec::new(), Vec::new(), Vec::new());
                if list[i].be.intersects(&list[j].be, &mut t1, &mut t2, &mut pts) {
                    splits.entry(i).or_default().extend(t1);
                    splits.entry(j).or_default().extend(t2);
                }
            }
            status.push(i);
        } else {
            status.retain(|&j| j != i);
        }
    }
    splits
}

fn handle_list(all: &[Wrapper], idx: &[usize], shapes: &mut [Vec<BezierEdge>], was_small: bool) {
    let mut by_layer: IndexMap<i32, Vec<&Wrapper>> = IndexMap::new();
    for &i in idx {
        by_layer.entry(all[i].layer).or_default().push(&all[i]);
    }
    for (_, list) in by_layer {
        let mut ignore: HashSet<usize> = HashSet::new();
        let mut existing: HashMap<Vec<(u64, u64)>, usize> = HashMap::new();
        for (n, w) in list.iter().enumerate() {
            let k = w.be.key();
            let rk = w.be.reverse().key();
            if let Some(prev) = existing.insert(k, n) {
                ignore.insert(prev);
            }
            if let Some(prev) = existing.insert(rk, n) {
                ignore.insert(prev);
            }
        }
        let splits = sweep(&list, &ignore);
        let mut order: Vec<usize> = splits.keys().copied().collect();
        order.sort_by(|&a, &b| list[a].shape.cmp(&list[b].shape).then(list[a].edge.cmp(&list[b].edge)));
        for &n in order.iter().rev() {
            let w = list[n];
            let mut ts = splits[&n].clone();
            ts.sort_by(|a, b| a.total_cmp(b));
            let real: Vec<f64> = ts.into_iter().filter(|&t| t != 0.0 && t != 1.0).collect();
            if real.is_empty() {
                continue;
            }
            let pieces = w.be.split(&real);
            let shape = &mut shapes[w.shape];
            shape.remove(w.edge);
            let mut pos = 0;
            for mut piece in pieces {
                piece.round_n(if was_small { 2.0 } else { 100.0 });
                if piece.is_empty() {
                    continue;
                }
                shape.insert(w.edge + pos, piece);
                pos += 1;
            }
        }
    }
}

fn detect_overlapping(s: &mut Split, was_small: bool) {
    let mut all = Vec::new();
    for (i, sh) in s.shapes.iter().enumerate() {
        for (j, be) in sh.iter().enumerate() {
            all.push(Wrapper { be: be.clone(), layer: s.layers[i], shape: i, edge: j });
        }
    }
    let (strokes, fills): (Vec<usize>, Vec<usize>) = (0..all.len()).partition(|&i| s.fs0[all[i].shape] == 0 && s.fs1[all[i].shape] == 0);
    handle_list(&all, &strokes, &mut s.shapes, was_small);
    handle_list(&all, &fills, &mut s.shapes, was_small);
    for sh in &mut s.shapes {
        for be in sh.iter_mut() {
            be.shrink_to_line();
        }
    }
}

/// `ShapeFixer.fix` (or `MorphShapeFixer` when `morph`).
pub fn fix(shape: &Shape, shape_num: u8, morph: bool, was_small: bool) -> Vec<Adv> {
    let mut s = split_to_layers(&shape.records);
    if morph {
        morph_before(shape_num, &mut s, &shape.styles);
    }
    fix_switched_fills(shape, &mut s);
    detect_overlapping(&mut s, was_small);
    combine_layers(&s)
}

// ---- SwitchedFillSidesFixer ----

struct Polygon {
    edges: Vec<paths::Edge>,
    fill: usize,
    ccw: bool,
    area: f64,
    bbox: R,
    path: Path,
    flat: Vec<P>,
    empty: bool,
    children: Vec<usize>,
    parent: Option<usize>,
    filled: bool,
}

fn edge_bezier(e: &paths::Edge) -> BezierEdge {
    let s = e.seg;
    match s.ctrl {
        Some(c) => BezierEdge::quad(s.from.0 as f64, s.from.1 as f64, c.0 as f64, c.1 as f64, s.to.0 as f64, s.to.1 as f64),
        None => BezierEdge::line(s.from.0 as f64, s.from.1 as f64, s.to.0 as f64, s.to.1 as f64),
    }
}

/// `PathArea.orientationSingleClosed`: (counter-clockwise, area) of the first closed ring.
fn orientation(path: &Path) -> (Option<bool>, f64) {
    let mut cur: Vec<P> = Vec::new();
    for s in path.flatten(0.5, 10) {
        match s {
            Seg::Move(x, y) => {
                if !cur.is_empty() {
                    cur.clear();
                }
                cur.push(p(x, y));
            }
            Seg::Line(x, y) => cur.push(p(x, y)),
            Seg::Close => {
                if !cur.is_empty() {
                    if cur.len() < 3 {
                        return (None, 0.0);
                    }
                    let mut a2 = 0.0;
                    for i in 0..cur.len() {
                        let a = cur[i];
                        let b = cur[(i + 1) % cur.len()];
                        a2 += a.x * b.y - b.x * a.y;
                    }
                    if a2.abs() <= 1e-9 {
                        return (None, 0.0);
                    }
                    return (Some(a2 <= 0.0), (a2 / 2.0).abs());
                }
            }
            Seg::Quad(..) => {}
        }
    }
    (None, 0.0)
}

fn exact_bounds(edges: &[paths::Edge]) -> R {
    let mut r: Option<R> = None;
    for e in edges {
        let b = edge_bezier(e).bbox();
        r = Some(match r {
            None => b,
            Some(a) => {
                let x0 = a.min_x().min(b.min_x());
                let y0 = a.min_y().min(b.min_y());
                let x1 = a.max_x().max(b.max_x());
                let y1 = a.max_y().max(b.max_y());
                R { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
            }
        });
    }
    r.unwrap_or_default()
}

fn polygon(edges: Vec<paths::Edge>, fill: usize) -> Polygon {
    let mut path = Path::default();
    let mut last: Option<(i32, i32)> = None;
    for e in &edges {
        if last != Some(e.seg.from) {
            path.move_to(e.seg.from.0 as f64, e.seg.from.1 as f64);
        }
        match e.seg.ctrl {
            Some(c) => path.quad_to(c.0 as f64, c.1 as f64, e.seg.to.0 as f64, e.seg.to.1 as f64),
            None => path.line_to(e.seg.to.0 as f64, e.seg.to.1 as f64),
        }
        last = Some(e.seg.to);
    }
    if last == Some(edges[0].seg.from) {
        path.close();
    }
    let (o, area) = orientation(&path);
    let flat: Vec<P> = path
        .flatten(0.1, 10)
        .into_iter()
        .filter_map(|s| match s {
            Seg::Move(x, y) | Seg::Line(x, y) => Some(p(x, y)),
            _ => None,
        })
        .collect();
    let bbox = exact_bounds(&edges);
    Polygon { edges, fill, ccw: o == Some(true), area, bbox, path, flat, empty: area == 0.0, children: Vec::new(), parent: None, filled: true }
}

fn on_boundary(poly: &Polygon, q: &P) -> bool {
    poly.flat.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        let cross = (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
        cross.abs() < 1e-6 && q.x >= a.x.min(b.x) - 1e-9 && q.x <= a.x.max(b.x) + 1e-9 && q.y >= a.y.min(b.y) - 1e-9 && q.y <= a.y.max(b.y) + 1e-9
    })
}

fn poly_contains(outer: &Polygon, inner: &Polygon) -> bool {
    if inner.empty {
        return false;
    }
    inner.flat.iter().all(|q| outer.path.contains(q.x, q.y) || on_boundary(outer, q))
}

fn build_containment(polys: &mut [Polygon]) {
    let mut by_style: IndexMap<usize, Vec<usize>> = IndexMap::new();
    for (i, pl) in polys.iter().enumerate() {
        by_style.entry(pl.fill).or_default().push(i);
    }
    for (_, mut group) in by_style {
        group.sort_by(|&a, &b| polys[b].area.total_cmp(&polys[a].area));
        for gi in (0..group.len()).rev() {
            let inner = group[gi];
            let mut best: Option<usize> = None;
            let mut best_area = f64::INFINITY;
            for &outer in &group {
                if outer == inner || polys[outer].area <= polys[inner].area || !polys[outer].bbox.contains(&polys[inner].bbox) {
                    continue;
                }
                if poly_contains(&polys[outer], &polys[inner]) && polys[outer].area < best_area {
                    best_area = polys[outer].area;
                    best = Some(outer);
                }
            }
            if let Some(b) = best {
                polys[b].children.push(inner);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fix_sides_in_layer(fill_list: &[Vec<paths::Edge>], s: &mut Split, layer: i32, start: usize, end: usize, g2l: &HashMap<usize, i32>) {
    let layer = (layer + 1) as usize;
    if layer >= fill_list.len() {
        return;
    }
    let mut fill_idx = usize::MAX;
    let mut cur: Vec<paths::Edge> = Vec::new();
    let mut lists: Vec<(Vec<paths::Edge>, usize)> = Vec::new();
    let (mut last_to, mut last_move) = ((i32::MAX, i32::MAX), (i32::MAX, i32::MAX));
    for e in &fill_list[layer] {
        if fill_idx != e.fill || e.seg.from != last_to || e.seg.from == last_move {
            if fill_idx != usize::MAX {
                lists.push((std::mem::take(&mut cur), fill_idx));
            }
            fill_idx = e.fill;
            last_move = e.seg.from;
        }
        cur.push(*e);
        last_to = e.seg.to;
    }
    if !cur.is_empty() {
        lists.push((cur, fill_idx));
    }
    let mut polys: Vec<Polygon> = lists.into_iter().map(|(l, f)| polygon(l, f)).collect();
    build_containment(&mut polys);
    for i in 0..polys.len() {
        for c in polys[i].children.clone() {
            polys[c].parent = Some(i);
        }
    }
    for i in 0..polys.len() {
        let mut depth = 0;
        let mut par = polys[i].parent;
        while let Some(pi) = par {
            par = polys[pi].parent;
            depth += 1;
        }
        polys[i].filled = depth % 2 == 0;
    }
    type Key = Vec<(u64, u64)>;
    let mut fs0_list: IndexMap<Key, Vec<i32>> = IndexMap::new();
    let mut fs1_list: IndexMap<Key, Vec<i32>> = IndexMap::new();
    for pl in &polys {
        let clockwise = !pl.ccw;
        let local = g2l.get(&pl.fill).copied().unwrap_or(0);
        for e in &pl.edges {
            let be = edge_bezier(e);
            let k = be.key();
            let rk = be.reverse().key();
            if pl.filled == clockwise {
                fs1_list.entry(k).or_default().push(local);
                fs0_list.entry(rk).or_default().push(local);
            } else {
                fs0_list.entry(k).or_default().push(local);
                fs1_list.entry(rk).or_default().push(local);
            }
        }
    }
    let rev = |k: &Key| -> Key { k.iter().rev().copied().collect() };
    let mut fs0: HashMap<Key, i32> = HashMap::new();
    let mut fs1: HashMap<Key, i32> = HashMap::new();
    for (k, l) in &fs0_list {
        let f = if l.len() == 1 { l[0] } else { -1 };
        if fs0.get(k).is_none_or(|&v| v > 0 || f == -1) {
            fs0.insert(k.clone(), f);
        }
        let r = rev(k);
        if fs1.get(&r).is_none_or(|&v| v > 0 || f == -1) {
            fs1.insert(r, f);
        }
    }
    for (k, l) in &fs1_list {
        let f = if l.len() == 1 { l[0] } else { -1 };
        if fs1.get(k).is_none_or(|&v| v > 0 || f == -1) {
            fs1.insert(k.clone(), f);
        }
        let r = rev(k);
        if fs0.get(&r).is_none_or(|&v| v > 0 || f == -1) {
            fs0.insert(r, f);
        }
    }
    for i in start..end {
        for be in &s.shapes[i] {
            if be.is_empty() {
                continue;
            }
            let (b0, b1) = (s.fs0[i], s.fs1[i]);
            if b0 == 0 && b1 == 0 {
                break;
            }
            let k = be.key();
            let mut a0 = fs0.get(&k).copied().unwrap_or(0);
            let mut a1 = fs1.get(&k).copied().unwrap_or(0);
            if a0 == -1 || a1 == -1 {
                break;
            }
            if a0 == 0 && a1 == b1 {
                a0 = b0;
            } else if a1 == 0 && a0 == b0 {
                a1 = b1;
            }
            s.fs0[i] = a0;
            s.fs1[i] = a1;
            break;
        }
    }
}

fn fix_switched_fills(shape: &Shape, s: &mut Split) {
    let fill_list: Vec<Vec<paths::Edge>> = paths::build_layers(shape).into_iter().map(|l| l.fills).collect();
    let mut g2l: HashMap<usize, i32> = HashMap::new();
    g2l.insert(0, 0);
    let mut last = 0usize;
    for _ in &shape.styles.fills {
        last += 1;
        g2l.insert(last, last as i32);
    }
    for rec in &shape.records {
        if let Record::StyleChange { new_styles: Some(ns), .. } = rec {
            for i in 0..ns.fills.len() {
                last += 1;
                g2l.insert(last, i as i32 + 1);
            }
        }
    }
    let mut from = 0;
    for i in 1..s.layers.len() {
        if s.layers[i] != s.layers[i - 1] {
            let l = s.layers[i - 1];
            fix_sides_in_layer(&fill_list, s, l, from, i, &g2l);
            from = i;
        }
    }
    if let Some(&l) = s.layers.last() {
        let n = s.layers.len();
        fix_sides_in_layer(&fill_list, s, l, from, n, &g2l);
    }
}

// ---- MorphShapeFixer ----

fn morph_before(shape_num: u8, s: &mut Split, base: &Styles) {
    for sh in &mut s.shapes {
        sh.retain(|b| !b.is_empty());
    }
    merge_similar(shape_num, s, base);
    merge_with_same_prefix(s);
    clear_duplicates_next_to_each_other(s);
    fix_holes(s);
    merge_opposite_fills(s);
}

/// Closed sub-paths of every path: (path index, first edge index, edges).
fn closed_paths(s: &Split) -> Vec<(usize, usize, Vec<BezierEdge>)> {
    let mut out = Vec::new();
    for (i, sh) in s.shapes.iter().enumerate() {
        let mut last_move: Option<P> = None;
        let mut last_edge: Option<&BezierEdge> = None;
        let mut move_idx = 0;
        let mut batch = Vec::new();
        for (j, be) in sh.iter().enumerate() {
            batch.push(be.clone());
            let e = be.end();
            if let Some(m) = last_move {
                if e.x == m.x && e.y == m.y {
                    out.push((i, move_idx, std::mem::take(&mut batch)));
                    move_idx = j + 1;
                    last_move = Some(e);
                }
            }
            match last_edge {
                Some(le) => {
                    let (a, b) = (le.end(), be.begin());
                    if !(a.x == b.x && a.y == b.y) {
                        last_move = Some(b);
                        move_idx = j;
                    }
                }
                None => last_move = Some(be.begin()),
            }
            last_edge = Some(be);
        }
    }
    out
}

fn is_empty_path(path: &[BezierEdge]) -> bool {
    path.iter().all(|b| b.is_empty())
}

fn batch_path(batch: &[BezierEdge]) -> Path {
    let mut path = Path::default();
    if batch.is_empty() {
        return path;
    }
    let b = batch[0].begin();
    path.move_to(b.x, b.y);
    for be in batch {
        if be.points.len() == 3 {
            path.quad_to(be.points[1].x, be.points[1].y, be.points[2].x, be.points[2].y);
        } else {
            let e = be.end();
            path.line_to(e.x, e.y);
        }
    }
    path.close();
    path
}

fn line_points(points: &mut Vec<P>, x1: f64, y1: f64, x2: f64, y2: f64) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let steps = ((dx * dx + dy * dy).sqrt().ceil() as i32).max(1);
    for step in 1..=steps {
        points.push(p(x1 + step as f64 * dx / steps as f64, y1 + step as f64 * dy / steps as f64));
    }
}

fn area_points(path: &Path) -> Vec<P> {
    let mut points = Vec::new();
    let (mut px, mut py, mut bx, mut by) = (0.0, 0.0, 0.0, 0.0);
    for s in path.flatten(0.1, 10) {
        match s {
            Seg::Move(x, y) => {
                bx = x;
                by = y;
                px = x;
                py = y;
                points.push(p(x, y));
            }
            Seg::Line(x, y) => {
                line_points(&mut points, px, py, x, y);
                px = x;
                py = y;
            }
            Seg::Close => {
                line_points(&mut points, px, py, bx, by);
                px = bx;
                py = by;
            }
            Seg::Quad(..) => {}
        }
    }
    points
}

/// `Distances.getBatchDistance`.
fn batch_distance(a: &[BezierEdge], b: &[BezierEdge]) -> f64 {
    let pa = area_points(&batch_path(a));
    let pb = area_points(&batch_path(b));
    let dist = |x: &[P], y: &[P]| {
        let mut max = 0.0;
        for q in x {
            let d = y.iter().fold(f64::MAX, |m, r| m.min(q.distance(r)));
            if d > max {
                max = d;
            }
        }
        max
    };
    dist(&pa, &pb).max(dist(&pb, &pa))
}

fn merge_similar(shape_num: u8, s: &mut Split, base: &Styles) {
    let closed = closed_paths(s);
    let fill = |i: usize| s.fs0[closed[i].0];
    let line = |i: usize| s.ls[closed[i].0];
    let layer = |i: usize| s.layers[closed[i].0];
    let mut repl: IndexMap<usize, usize> = IndexMap::new();
    let mut removed: HashSet<usize> = HashSet::new();
    let fills_of = |l: i32| if l == -1 { &base.fills } else { &s.fill_layers[l as usize] };
    let lines_of = |l: i32| if l == -1 { &base.lines } else { &s.line_layers[l as usize] };
    let _ = shape_num;
    for i1 in 0..closed.len() {
        if repl.contains_key(&i1) {
            continue;
        }
        for i2 in 0..closed.len() {
            if i1 == i2 || repl.contains_key(&i2) || layer(i1) != layer(i2) {
                continue;
            }
            if fill(i1) > 0 && fill(i2) > 0 && fill(i1) != fill(i2) {
                let fa = fills_of(layer(i1));
                if fa.get(fill(i1) as usize - 1) != fa.get(fill(i2) as usize - 1) {
                    continue;
                }
            }
            if line(i1) > 0 && line(i2) > 0 && line(i1) != line(i2) {
                let la = lines_of(layer(i1));
                if la.get(line(i1) as usize - 1) != la.get(line(i2) as usize - 1) {
                    continue;
                }
            }
            if closed[i1].2.len() <= 1 || closed[i2].2.len() <= 1 {
                continue;
            }
            if line(i1) > 0 && line(i2) == 0 {
                continue;
            }
            if is_empty_path(&closed[i1].2) || is_empty_path(&closed[i2].2) {
                continue;
            }
            if batch_distance(&closed[i1].2, &closed[i2].2) <= 10.0 {
                repl.insert(i2, i1);
                removed.insert(i2);
            }
        }
    }
    for i in 0..closed.len() {
        let ri = repl.get(&i).copied().unwrap_or(i);
        let li = closed[ri].2.clone();
        for j in i + 1..closed.len() {
            if removed.contains(&j) && !repl.contains_key(&j) {
                continue;
            }
            let rj = repl.get(&j).copied().unwrap_or(j);
            if fill(i) != fill(j) || line(i) != line(j) {
                continue;
            }
            if li == closed[rj].2 {
                repl.shift_remove(&j);
                removed.insert(j);
            }
        }
    }
    for i in (0..closed.len()).rev() {
        let (si, from, ref edges) = closed[i];
        let to = from as i64 + edges.len() as i64 - 1;
        let list = &mut s.shapes[si];
        if removed.contains(&i) {
            let mut j = to;
            while j >= from as i64 {
                if (j as usize) < list.len() {
                    list.remove(j as usize);
                }
                j -= 1;
            }
        }
        if let Some(&r) = repl.get(&i) {
            let at = from.min(list.len());
            for (k, be) in closed[r].2.iter().enumerate() {
                list.insert(at + k, be.clone());
            }
        }
        if !removed.contains(&i) {
            let mut j = to;
            while j >= from as i64 {
                if (j as usize) < list.len() && list[j as usize].is_empty() {
                    list.remove(j as usize);
                }
                j -= 1;
            }
        }
    }
}

fn merge_with_same_prefix(s: &mut Split) {
    let mut prev: Option<usize> = None;
    for i in 0..s.shapes.len() {
        if s.shapes[i].is_empty() {
            continue;
        }
        if let Some(pi) = prev {
            let (pf0, pf1, pl) = (s.fs0[i - 1], s.fs1[i - 1], s.ls[i - 1]);
            let (l, f0, f1) = (s.ls[i], s.fs0[i], s.fs1[i]);
            let prev_list = &s.shapes[pi];
            let list = &s.shapes[i];
            if f0 == 0 && f1 == 0 && l != 0 && l == pl {
                if prev_list.len() >= list.len() && list.iter().zip(prev_list).all(|(a, b)| a == b) {
                    s.shapes[i].clear();
                    continue;
                }
            } else if pf0 == 0 && pf1 == 0 && pl != 0 && l == pl && list.len() >= prev_list.len() && prev_list.iter().zip(list).all(|(a, b)| a == b) {
                s.shapes[pi].clear();
            }
        }
        prev = Some(i);
    }
}

fn clear_duplicates_next_to_each_other(s: &mut Split) {
    let mut prev: Option<usize> = None;
    for i in 0..s.shapes.len() {
        if s.shapes[i].is_empty() {
            continue;
        }
        if let Some(pi) = prev {
            if s.fs0[i] == s.fs0[pi] && s.ls[i] == s.ls[pi] && s.shapes[i] == s.shapes[pi] {
                s.shapes[pi].clear();
            }
        }
        prev = Some(i);
    }
}

fn fix_holes(s: &mut Split) {
    let mut closed = closed_paths(s);
    for c in closed.iter_mut() {
        let (si, sj, ref list) = *c;
        if list.is_empty() {
            continue;
        }
        let mut pts = vec![list[0].begin()];
        for be in list {
            if be.points.len() == 3 {
                pts.push(be.points[1]);
            }
            pts.push(be.end());
        }
        let mut sum = 0.0;
        for j in 0..pts.len() {
            let a = pts[j];
            let b = pts[(j + 1) % pts.len()];
            sum += a.x * b.y - b.x * a.y;
        }
        if sum < 0.0 {
            let rev: Vec<BezierEdge> = list.iter().rev().map(|b| b.reverse()).collect();
            for (k, r) in rev.into_iter().enumerate() {
                s.shapes[si][sj + k] = r;
            }
        }
    }
    let mut by_fill: IndexMap<i32, Vec<usize>> = IndexMap::new();
    for (i, c) in closed.iter().enumerate() {
        let fs = s.fs0[c.0];
        if fs != 0 {
            by_fill.entry(fs).or_default().push(i);
        }
    }
    let mut holes: indexmap::IndexSet<usize> = indexmap::IndexSet::new();
    for (_, ids) in &by_fill {
        let mut all = Path::default();
        let mut paths_of: Vec<(usize, Path)> = Vec::new();
        for &i in ids {
            let c = &closed[i].2;
            if c.is_empty() {
                continue;
            }
            let mut cp = Path::default();
            let b = c[0].begin();
            cp.move_to(b.x, b.y);
            all.move_to(b.x, b.y);
            let mut empty = true;
            for be in c {
                if be.is_empty() {
                    continue;
                }
                empty = false;
                if be.points.len() == 3 {
                    cp.quad_to(be.points[1].x, be.points[1].y, be.end().x, be.end().y);
                    all.quad_to(be.points[1].x, be.points[1].y, be.end().x, be.end().y);
                } else {
                    cp.line_to(be.end().x, be.end().y);
                    all.line_to(be.end().x, be.end().y);
                }
            }
            cp.close();
            all.close();
            if !empty {
                paths_of.push((i, cp));
            }
        }
        for (i, region) in &paths_of {
            let b = region.bounds();
            let (cx, cy) = (b.center_x(), b.center_y());
            let (mut n, mut yes, mut no) = (0, 0, 0);
            let (mut x, mut y) = (0.0, 0.0);
            for sg in &region.segs {
                match *sg {
                    Seg::Move(px, py) | Seg::Line(px, py) => {
                        x = px;
                        y = py;
                    }
                    Seg::Quad(_, _, px, py) => {
                        x = px;
                        y = py;
                    }
                    Seg::Close => {}
                }
                n += 1;
                let d = ((cx - x) * (cx - x) + (cy - y) * (cy - y)).sqrt();
                let x1 = (cx - x) * 0.1 / d;
                let y1 = (cy - y) * 0.1 / d;
                if all.contains(x + x1, y + y1) {
                    yes += 1;
                } else {
                    no += 1;
                }
                if n == 4 {
                    break;
                }
            }
            if no > yes {
                holes.insert(*i);
            }
        }
    }
    for i in (0..closed.len()).rev() {
        if !holes.contains(&i) {
            continue;
        }
        let (si, from, ref edges) = closed[i];
        let to = from + edges.len() - 1;
        for j in (from..=to).rev() {
            if j < s.shapes[si].len() {
                s.shapes[si].remove(j);
            }
        }
        let edges = std::mem::take(&mut closed[i].2);
        s.shapes.insert(si + 1, edges);
        s.fs0.insert(si + 1, 0);
        let f = s.fs0[si];
        s.fs1.insert(si + 1, f);
        let l = s.ls[si];
        s.ls.insert(si + 1, l);
        let la = s.layers[si];
        s.layers.insert(si + 1, la);
    }
}

fn merge_opposite_fills(s: &mut Split) {
    let mut i1 = 0;
    while i1 < s.shapes.len() {
        let mut i2 = 0;
        while i2 < s.shapes.len() {
            if i1 == i2 || s.layers[i1] != s.layers[i2] || s.ls[i1] != s.ls[i2] || s.shapes[i1] != s.shapes[i2] {
                i2 += 1;
                continue;
            }
            let mut remove = false;
            if s.fs0[i1] != 0 && s.fs1[i1] == 0 && s.fs1[i2] != 0 && s.fs0[i2] == 0 {
                s.fs1[i1] = s.fs1[i2];
                remove = true;
            } else if s.fs1[i1] != 0 && s.fs0[i1] == 0 && s.fs0[i2] != 0 && s.fs1[i2] == 0 {
                s.fs0[i1] = s.fs0[i2];
                remove = true;
            }
            if remove {
                s.remove(i2);
                continue;
            }
            i2 += 1;
        }
        i1 += 1;
    }
}
