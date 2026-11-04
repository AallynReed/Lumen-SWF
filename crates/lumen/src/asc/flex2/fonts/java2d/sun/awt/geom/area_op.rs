//! Port of JDK 17 `sun.awt.geom.AreaOp` and its `CAGOp` (`AddOp`, `SubOp`, `IntOp`, `XorOp`), `NZWindOp` and
//! `EOWindOp` subclasses: the scanline sweep that classifies edges and stitches the result outline.

use std::cmp::Ordering;
use std::sync::Arc;

use super::chain_end::LinkArena;
use super::curve::Curve;
use super::curve_link::CurveLink;
use super::edge::{self, Edge};

pub const CTAG_LEFT: i32 = 0;
pub const CTAG_RIGHT: i32 = 1;
pub const ETAG_IGNORE: i32 = 0;
pub const ETAG_ENTER: i32 = 1;
pub const ETAG_EXIT: i32 = -1;
pub const RSTAG_INSIDE: i32 = 1;
pub const RSTAG_OUTSIDE: i32 = -1;
pub const MAX_LINK_COUNT: usize = 1024;

/// Which `AreaOp` subclass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Add,
    Sub,
    Int,
    Xor,
    NZWind,
    EOWind,
}

/// An `AreaOp` instance with the state its subclass keeps.
#[derive(Clone, Debug)]
pub struct AreaOp {
    kind: OpKind,
    in_left: bool,
    in_right: bool,
    in_result: bool,
    inside: bool,
    count: i32,
}

impl AreaOp {
    pub fn new(kind: OpKind) -> AreaOp {
        AreaOp { kind, in_left: false, in_right: false, in_result: false, inside: false, count: 0 }
    }

    /// `newRow`.
    fn new_row(&mut self) {
        match self.kind {
            OpKind::NZWind => self.count = 0,
            OpKind::EOWind => self.inside = false,
            _ => {
                self.in_left = false;
                self.in_right = false;
                self.in_result = false;
            }
        }
    }

    /// `CAGOp.newClassification`.
    fn new_classification(&self, in_left: bool, in_right: bool) -> bool {
        match self.kind {
            OpKind::Add => in_left || in_right,
            OpKind::Sub => in_left && !in_right,
            OpKind::Int => in_left && in_right,
            _ => in_left != in_right,
        }
    }

    /// `classify`.
    fn classify(&mut self, e: &Edge) -> i32 {
        match self.kind {
            OpKind::NZWind => {
                let mut new_count = self.count;
                let type_ = if new_count == 0 { ETAG_ENTER } else { ETAG_IGNORE };
                new_count += e.curve.get_direction();
                self.count = new_count;
                if new_count == 0 {
                    ETAG_EXIT
                } else {
                    type_
                }
            }
            OpKind::EOWind => {
                let new_inside = !self.inside;
                self.inside = new_inside;
                if new_inside {
                    ETAG_ENTER
                } else {
                    ETAG_EXIT
                }
            }
            _ => {
                if e.ctag == CTAG_LEFT {
                    self.in_left = !self.in_left;
                } else {
                    self.in_right = !self.in_right;
                }
                let new_class = self.new_classification(self.in_left, self.in_right);
                if self.in_result == new_class {
                    return ETAG_IGNORE;
                }
                self.in_result = new_class;
                if new_class {
                    ETAG_ENTER
                } else {
                    ETAG_EXIT
                }
            }
        }
    }

    /// `getState`.
    fn get_state(&self) -> i32 {
        let inside = match self.kind {
            OpKind::NZWind => self.count != 0,
            OpKind::EOWind => self.inside,
            _ => self.in_result,
        };
        if inside {
            RSTAG_INSIDE
        } else {
            RSTAG_OUTSIDE
        }
    }

    /// `calculate`.
    pub fn calculate(&mut self, left: &[Arc<Curve>], right: &[Arc<Curve>]) -> Vec<Arc<Curve>> {
        let mut edges = Vec::new();
        add_edges(&mut edges, left, CTAG_LEFT);
        add_edges(&mut edges, right, CTAG_RIGHT);
        self.prune_edges(edges)
    }

    /// `consumeSubCurves`.
    fn consume_sub_curves(arena: &mut LinkArena, subcurves: &mut Vec<usize>, chains: &mut Vec<usize>, curves: &mut Vec<Arc<Curve>>) {
        finalize_sub_curves(arena, subcurves, chains);
        for &first in subcurves.iter() {
            let mut link = first;
            curves.push(arena.links[link].get_moveto());
            let mut nextlink = link;
            while let Some(n) = arena.links[nextlink].next {
                nextlink = n;
                let (c, ytop, ybot, etag) = {
                    let l = &arena.links[nextlink];
                    (l.curve.clone(), l.ytop, l.ybot, l.etag)
                };
                if !arena.links[link].absorb(&c, ytop, ybot, etag) {
                    curves.push(arena.links[link].get_sub_curve());
                    link = nextlink;
                }
            }
            curves.push(arena.links[link].get_sub_curve());
        }
    }

    /// `pruneEdges`.
    fn prune_edges(&mut self, mut edges: Vec<Edge>) -> Vec<Arc<Curve>> {
        let numedges = edges.len();
        if numedges < 2 {
            return Vec::new();
        }
        let mut edgelist: Vec<usize> = (0..numedges).collect();
        stable_sort(&mut edgelist, |&a, &b| yx_top_compare(&edges[a], &edges[b]));
        let mut left = 0usize;
        let mut right = 0usize;
        let mut cur: usize;
        let mut next: isize;
        let mut yrange = [0.0f64; 2];
        let mut arena = LinkArena::default();
        let mut subcurves: Vec<usize> = Vec::new();
        let mut chains: Vec<usize> = Vec::new();
        let mut links: Vec<usize> = Vec::new();
        let mut ret: Vec<Arc<Curve>> = Vec::new();
        let mut link_count = 0usize;
        while left < numedges {
            let mut y = yrange[0];
            let mut c = right as isize - 1;
            next = c;
            while c >= left as isize {
                let e = edgelist[c as usize];
                if edges[e].curve.get_y_bot() > y {
                    if next > c {
                        edgelist[next as usize] = e;
                    }
                    next -= 1;
                }
                c -= 1;
            }
            left = (next + 1) as usize;
            if left >= right {
                if right >= numedges {
                    break;
                }
                y = edges[edgelist[right]].curve.get_y_top();
                if y > yrange[0] {
                    finalize_sub_curves(&mut arena, &mut subcurves, &mut chains);
                }
                yrange[0] = y;
            }
            while right < numedges {
                let e = edgelist[right];
                if edges[e].curve.get_y_top() > y {
                    break;
                }
                right += 1;
            }
            yrange[1] = edges[edgelist[left]].curve.get_y_bot();
            if right < numedges {
                y = edges[edgelist[right]].curve.get_y_top();
                if yrange[1] > y {
                    yrange[1] = y;
                }
            }
            let mut nexteq = 1;
            cur = left;
            while cur < right {
                let e = edgelist[cur];
                edges[e].equivalence = 0;
                let mut nx = cur;
                while nx > left {
                    let prevedge = edgelist[nx - 1];
                    let ordering = edge::compare_to(&mut edges, e, prevedge, &mut yrange);
                    if yrange[1] <= yrange[0] {
                        panic!("backstepping to {} from {}", yrange[1], yrange[0]);
                    }
                    if ordering >= 0 {
                        if ordering == 0 {
                            let mut eq = edges[prevedge].equivalence;
                            if eq == 0 {
                                eq = nexteq;
                                nexteq += 1;
                                edges[prevedge].equivalence = eq;
                            }
                            edges[e].equivalence = eq;
                        }
                        break;
                    }
                    edgelist[nx] = prevedge;
                    nx -= 1;
                }
                edgelist[nx] = e;
                cur += 1;
            }
            self.new_row();
            let ystart = yrange[0];
            let yend = yrange[1];
            cur = left;
            while cur < right {
                let mut e = edgelist[cur];
                let mut etag;
                let eq = edges[e].equivalence;
                if eq != 0 {
                    let origstate = self.get_state();
                    etag = if origstate == RSTAG_INSIDE { ETAG_EXIT } else { ETAG_ENTER };
                    let mut activematch: Option<usize> = None;
                    let mut longestmatch = e;
                    let mut furthesty = yend;
                    loop {
                        self.classify(&edges[e]);
                        if activematch.is_none() && edges[e].is_active_for(ystart, etag) {
                            activematch = Some(e);
                        }
                        y = edges[e].curve.get_y_bot();
                        if y > furthesty {
                            longestmatch = e;
                            furthesty = y;
                        }
                        cur += 1;
                        if cur >= right {
                            break;
                        }
                        e = edgelist[cur];
                        if edges[e].equivalence != eq {
                            break;
                        }
                    }
                    cur -= 1;
                    if self.get_state() == origstate {
                        etag = ETAG_IGNORE;
                    } else {
                        e = activematch.unwrap_or(longestmatch);
                    }
                } else {
                    etag = self.classify(&edges[e]);
                }
                if etag != ETAG_IGNORE {
                    edges[e].record(yend, etag);
                    let l = arena.add_link(CurveLink::new(edges[e].curve.clone(), ystart, yend, etag));
                    links.push(l);
                }
                cur += 1;
            }
            if link_count >= MAX_LINK_COUNT {
                Self::consume_sub_curves(&mut arena, &mut subcurves, &mut chains, &mut ret);
                link_count = 0;
                chains.clear();
                subcurves.clear();
            }
            link_count += links.len();
            resolve_links(&mut arena, &mut subcurves, &mut chains, &links);
            links.clear();
            yrange[0] = yend;
        }
        Self::consume_sub_curves(&mut arena, &mut subcurves, &mut chains, &mut ret);
        ret
    }
}

/// `addEdges`.
fn add_edges(edges: &mut Vec<Edge>, curves: &[Arc<Curve>], curvetag: i32) {
    for c in curves {
        if c.get_order() > 0 {
            edges.push(Edge::new(c.clone(), curvetag));
        }
    }
}

/// `YXTopComparator`.
fn yx_top_compare(o1: &Edge, o2: &Edge) -> Ordering {
    let c1 = &o1.curve;
    let c2 = &o2.curve;
    let (mut v1, mut v2) = (c1.get_y_top(), c2.get_y_top());
    if v1 == v2 {
        v1 = c1.get_x_top();
        v2 = c2.get_x_top();
        if v1 == v2 {
            return Ordering::Equal;
        }
    }
    if v1 < v2 {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// A stable merge sort standing in for `Arrays.sort(Object[], Comparator)`; any stable sort gives TimSort's order
/// for a consistent comparator, and this one never panics on an inconsistent (NaN) one.
fn stable_sort<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut cmp: F) {
    fn merge_sort<T: Copy, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], buf: &mut Vec<T>, cmp: &mut F) {
        let n = v.len();
        if n <= 1 {
            return;
        }
        let mid = n / 2;
        merge_sort(&mut v[..mid], buf, cmp);
        merge_sort(&mut v[mid..], buf, cmp);
        buf.clear();
        let (mut i, mut j) = (0, mid);
        while i < mid && j < n {
            if cmp(&v[j], &v[i]) == Ordering::Less {
                buf.push(v[j]);
                j += 1;
            } else {
                buf.push(v[i]);
                i += 1;
            }
        }
        buf.extend_from_slice(&v[i..mid]);
        buf.extend_from_slice(&v[j..n]);
        v.copy_from_slice(buf);
    }
    let mut buf = Vec::with_capacity(v.len());
    merge_sort(v, &mut buf, &mut cmp);
}

/// `finalizeSubCurves`.
pub fn finalize_sub_curves(arena: &mut LinkArena, subcurves: &mut Vec<usize>, chains: &mut Vec<usize>) {
    let numchains = chains.len();
    if numchains == 0 {
        return;
    }
    if numchains & 1 != 0 {
        panic!("Odd number of chains!");
    }
    let mut i = 1;
    while i < numchains {
        let open = chains[i - 1];
        let close = chains[i];
        if let Some(subcurve) = arena.link_to(open, close) {
            subcurves.push(subcurve);
        }
        i += 2;
    }
    chains.clear();
}

/// `resolveLinks`.
pub fn resolve_links(arena: &mut LinkArena, subcurves: &mut Vec<usize>, chains: &mut Vec<usize>, links: &[usize]) {
    let numlinks = links.len();
    if numlinks & 1 != 0 {
        panic!("Odd number of new curves!");
    }
    let mut linklist: Vec<Option<usize>> = links.iter().map(|&l| Some(l)).collect();
    linklist.resize(numlinks + 2, None);
    let numchains = chains.len();
    if numchains & 1 != 0 {
        panic!("Odd number of chains!");
    }
    let mut endlist: Vec<Option<usize>> = chains.iter().map(|&c| Some(c)).collect();
    endlist.resize(numchains + 2, None);
    let mut curchain = 0usize;
    let mut curlink = 0usize;
    chains.clear();
    let mut chain = endlist[0];
    let mut nextchain = endlist[1];
    let mut link = linklist[0];
    let mut nextlink = linklist[1];
    while chain.is_some() || link.is_some() {
        let mut connectchains = link.is_none();
        let mut connectlinks = chain.is_none();
        if !connectchains && !connectlinks {
            let (ch, li) = (chain.unwrap(), link.unwrap());
            connectchains = curchain & 1 == 0 && arena.get_x(ch) == arena.get_x(nextchain.expect("chain pair"));
            connectlinks = curlink & 1 == 0 && arena.links[li].get_x() == arena.links[nextlink.expect("link pair")].get_x();
            if !connectchains && !connectlinks {
                let cx = arena.get_x(ch);
                let lx = arena.links[li].get_x();
                connectchains = match nextchain {
                    Some(nc) => cx < lx && obstructs(arena.get_x(nc), lx, curchain as i32),
                    None => false,
                };
                connectlinks = match nextlink {
                    Some(nl) => lx < cx && obstructs(arena.links[nl].get_x(), cx, curlink as i32),
                    None => false,
                };
            }
        }
        if connectchains {
            if let Some(subcurve) = arena.link_to(chain.expect("chain"), nextchain.expect("chain pair")) {
                subcurves.push(subcurve);
            }
            curchain += 2;
            chain = endlist[curchain];
            nextchain = endlist[curchain + 1];
        }
        if connectlinks {
            let openend = arena.new_end(link.expect("link"), None);
            let closeend = arena.new_end(nextlink.expect("link pair"), Some(openend));
            arena.ends[openend].partner = Some(closeend);
            chains.push(openend);
            chains.push(closeend);
            curlink += 2;
            link = linklist[curlink];
            nextlink = linklist[curlink + 1];
        }
        if !connectchains && !connectlinks {
            let ch = chain.expect("chain");
            arena.add_link_to(ch, link.expect("link"));
            chains.push(ch);
            curchain += 1;
            chain = nextchain;
            nextchain = endlist[curchain + 1];
            curlink += 1;
            link = nextlink;
            nextlink = linklist[curlink + 1];
        }
    }
}

/// `obstructs`.
pub fn obstructs(v1: f64, v2: f64, phase: i32) -> bool {
    if phase & 1 == 0 {
        v1 <= v2
    } else {
        v1 < v2
    }
}
