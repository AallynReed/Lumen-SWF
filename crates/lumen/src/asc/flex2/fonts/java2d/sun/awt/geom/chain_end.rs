//! Port of JDK 17 `sun.awt.geom.ChainEnd`: one open end of a partial outline built from `CurveLink`s.

use super::area_op::{ETAG_ENTER, ETAG_IGNORE};
use super::curve_link::CurveLink;

/// `ChainEnd`; `head`, `tail` and `partner` index into a `LinkArena`.
#[derive(Clone, Debug)]
pub struct ChainEnd {
    pub head: usize,
    pub tail: usize,
    pub partner: Option<usize>,
    pub etag: i32,
}

/// The `CurveLink` and `ChainEnd` objects one `AreaOp.calculate` allocates.
#[derive(Default)]
pub struct LinkArena {
    pub links: Vec<CurveLink>,
    pub ends: Vec<ChainEnd>,
}

impl LinkArena {
    pub fn add_link(&mut self, link: CurveLink) -> usize {
        self.links.push(link);
        self.links.len() - 1
    }

    /// `new ChainEnd(first, partner)`.
    pub fn new_end(&mut self, first: usize, partner: Option<usize>) -> usize {
        let etag = self.links[first].etag;
        self.ends.push(ChainEnd { head: first, tail: first, partner, etag });
        self.ends.len() - 1
    }

    /// `ChainEnd.linkTo`.
    pub fn link_to(&mut self, this: usize, that: usize) -> Option<usize> {
        if self.ends[this].etag == ETAG_IGNORE || self.ends[that].etag == ETAG_IGNORE {
            panic!("ChainEnd linked more than once!");
        }
        if self.ends[this].etag == self.ends[that].etag {
            panic!("Linking chains of the same type!");
        }
        let (enter, exit) = if self.ends[this].etag == ETAG_ENTER { (this, that) } else { (that, this) };
        self.ends[this].etag = ETAG_IGNORE;
        self.ends[that].etag = ETAG_IGNORE;
        let (enter_tail, exit_head) = (self.ends[enter].tail, self.ends[exit].head);
        self.links[enter_tail].next = Some(exit_head);
        self.ends[enter].tail = self.ends[exit].tail;
        if self.ends[this].partner == Some(that) {
            return Some(self.ends[enter].head);
        }
        let otherenter = self.ends[exit].partner.expect("partner");
        let otherexit = self.ends[enter].partner.expect("partner");
        self.ends[otherenter].partner = Some(otherexit);
        self.ends[otherexit].partner = Some(otherenter);
        let enter_head = self.ends[enter].head;
        if self.links[enter_head].get_y_top() < self.links[self.ends[otherenter].head].get_y_top() {
            let t = self.ends[enter].tail;
            self.links[t].next = Some(self.ends[otherenter].head);
            self.ends[otherenter].head = enter_head;
        } else {
            let t = self.ends[otherexit].tail;
            self.links[t].next = Some(enter_head);
            self.ends[otherexit].tail = self.ends[enter].tail;
        }
        None
    }

    /// `ChainEnd.addLink`.
    pub fn add_link_to(&mut self, this: usize, newlink: usize) {
        if self.ends[this].etag == ETAG_ENTER {
            let t = self.ends[this].tail;
            self.links[t].next = Some(newlink);
            self.ends[this].tail = newlink;
        } else {
            self.links[newlink].next = Some(self.ends[this].head);
            self.ends[this].head = newlink;
        }
    }

    /// `ChainEnd.getX`.
    pub fn get_x(&self, this: usize) -> f64 {
        let e = &self.ends[this];
        if e.etag == ETAG_ENTER {
            self.links[e.tail].get_x_bot()
        } else {
            self.links[e.head].get_x_bot()
        }
    }
}
