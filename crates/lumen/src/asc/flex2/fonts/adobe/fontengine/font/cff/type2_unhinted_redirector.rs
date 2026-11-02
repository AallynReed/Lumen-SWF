//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2UnhintedRedirector`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_consumer::Type2Consumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;

/// The redirector's state; the `OutlineConsumer` field is passed to each call instead.
#[derive(Clone, Debug, Default)]
pub struct Type2UnhintedRedirector {
    pub x: f64,
    pub y: f64,
    path_started: bool,
    path_start_x: f64,
    path_start_y: f64,
}

impl Type2UnhintedRedirector {
    pub fn new() -> Type2UnhintedRedirector {
        Type2UnhintedRedirector::default()
    }

    pub fn reset(&mut self) {
        self.x = 0.0;
        self.y = 0.0;
        self.path_started = false;
    }

    fn start_path_if_needed(&mut self, d: f64, d2: f64) {
        if !self.path_started {
            self.path_started = true;
            self.path_start_x = d;
            self.path_start_y = d2;
        }
    }

    pub fn endchar<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C) {
        if self.path_started && (self.x.total_cmp(&self.path_start_x).is_ne() || self.y.total_cmp(&self.path_start_y).is_ne()) {
            oc.lineto(self.path_start_x, self.path_start_y);
        }
        oc.endchar();
        self.path_started = false;
    }

    pub fn rmoveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        self.path_started = false;
        self.x += a[(n - 2) as usize];
        self.y += a[(n - 1) as usize];
        oc.moveto(self.x, self.y);
    }

    pub fn hmoveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        self.path_started = false;
        self.x += a[(n - 1) as usize];
        oc.moveto(self.x, self.y);
    }

    pub fn vmoveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        self.path_started = false;
        self.y += a[(n - 1) as usize];
        oc.moveto(self.x, self.y);
    }

    pub fn moveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        self.path_started = false;
        self.x = a[(n - 2) as usize];
        self.y = a[(n - 1) as usize];
        oc.moveto(self.x, self.y);
    }

    pub fn rlineto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        let mut i = 0;
        while i < n as usize {
            self.start_path_if_needed(self.x, self.y);
            self.x += a[i];
            self.y += a[i + 1];
            oc.lineto(self.x, self.y);
            i += 2;
        }
    }

    pub fn hlineto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        for i in 0..n as usize {
            self.start_path_if_needed(self.x, self.y);
            if i % 2 == 0 {
                self.x += a[i];
            } else {
                self.y += a[i];
            }
            oc.lineto(self.x, self.y);
        }
    }

    pub fn vlineto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        for i in 0..n as usize {
            self.start_path_if_needed(self.x, self.y);
            if i % 2 == 1 {
                self.x += a[i];
            } else {
                self.y += a[i];
            }
            oc.lineto(self.x, self.y);
        }
    }

    pub fn rrcurveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        let mut i = 0;
        while i < n as usize {
            self.start_path_if_needed(self.x, self.y);
            self.x += a[i];
            self.y += a[i + 1];
            let (d, d2) = (self.x, self.y);
            self.x += a[i + 2];
            self.y += a[i + 3];
            let (d3, d4) = (self.x, self.y);
            self.x += a[i + 4];
            self.y += a[i + 5];
            oc.curveto(d, d2, d3, d4, self.x, self.y);
            i += 6;
        }
    }

    pub fn hhcurveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        let n = n as usize;
        let mut k = 0;
        let (mut d, mut d2) = (self.x, self.y);
        if n % 2 == 1 {
            self.y += a[k];
            k += 1;
        }
        while k < n {
            self.x += a[k];
            let (d3, d4) = (self.x, self.y);
            self.x += a[k + 1];
            self.y += a[k + 2];
            let (d5, d6) = (self.x, self.y);
            self.x += a[k + 3];
            k += 4;
            self.start_path_if_needed(d, d2);
            oc.curveto(d3, d4, d5, d6, self.x, self.y);
            d = self.x;
            d2 = self.y;
        }
    }

    pub fn vvcurveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        let n = n as usize;
        let mut k = 0;
        let (mut d, mut d2) = (self.x, self.y);
        if n % 2 == 1 {
            self.x += a[k];
            k += 1;
        }
        while k < n {
            self.y += a[k];
            let (d3, d4) = (self.x, self.y);
            self.x += a[k + 1];
            self.y += a[k + 2];
            let (d5, d6) = (self.x, self.y);
            self.y += a[k + 3];
            k += 4;
            self.start_path_if_needed(d, d2);
            oc.curveto(d3, d4, d5, d6, self.x, self.y);
            d = self.x;
            d2 = self.y;
        }
    }

    /// One curve that starts horizontal (`h`) or vertical; `last` adds the trailing coordinate.
    fn hv_step<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], k: &mut usize, n: usize, h: bool, last: bool, d: &mut f64, d2: &mut f64) {
        if h {
            self.x += a[*k];
        } else {
            self.y += a[*k];
        }
        *k += 1;
        let (d3, d4) = (self.x, self.y);
        self.x += a[*k];
        self.y += a[*k + 1];
        *k += 2;
        let (d5, d6) = (self.x, self.y);
        if h {
            self.y += a[*k];
        } else {
            self.x += a[*k];
        }
        *k += 1;
        if last && *k + 1 == n {
            if h {
                self.x += a[*k];
            } else {
                self.y += a[*k];
            }
            *k += 1;
        }
        self.start_path_if_needed(*d, *d2);
        oc.curveto(d3, d4, d5, d6, self.x, self.y);
        *d = self.x;
        *d2 = self.y;
    }

    /// `hvcurveto` (`h`) and `vhcurveto`.
    fn alternating<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32, h: bool) {
        let n = n as usize;
        let mut k = 0;
        let (mut d, mut d2) = (self.x, self.y);
        if n % 8 == 4 || n % 8 == 5 {
            self.hv_step(oc, a, &mut k, n, h, true, &mut d, &mut d2);
            while k + 8 <= n {
                self.hv_step(oc, a, &mut k, n, !h, false, &mut d, &mut d2);
                self.hv_step(oc, a, &mut k, n, h, true, &mut d, &mut d2);
            }
        } else {
            while k + 8 <= n {
                self.hv_step(oc, a, &mut k, n, h, false, &mut d, &mut d2);
                self.hv_step(oc, a, &mut k, n, !h, true, &mut d, &mut d2);
            }
        }
    }

    pub fn hvcurveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        self.alternating(oc, a, n, true);
    }

    pub fn vhcurveto<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        self.alternating(oc, a, n, false);
    }

    pub fn rcurveline<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        let n = n as usize;
        let mut k = 0;
        let (mut d, mut d2) = (self.x, self.y);
        while k + 6 <= n {
            self.x += a[k];
            self.y += a[k + 1];
            let (d3, d4) = (self.x, self.y);
            self.x += a[k + 2];
            self.y += a[k + 3];
            let (d5, d6) = (self.x, self.y);
            self.x += a[k + 4];
            self.y += a[k + 5];
            k += 6;
            self.start_path_if_needed(d, d2);
            oc.curveto(d3, d4, d5, d6, self.x, self.y);
            d = self.x;
            d2 = self.y;
        }
        self.x += a[k];
        self.y += a[k + 1];
        self.start_path_if_needed(d, d2);
        oc.lineto(self.x, self.y);
    }

    pub fn rlinecurve<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], n: i32) {
        let n = n as usize;
        let mut k = 0;
        let (mut d, mut d2) = (self.x, self.y);
        while k + 6 < n {
            self.x += a[k];
            self.y += a[k + 1];
            k += 2;
            self.start_path_if_needed(d, d2);
            oc.lineto(self.x, self.y);
            d = self.x;
            d2 = self.y;
        }
        self.x += a[k];
        self.y += a[k + 1];
        let (d3, d4) = (self.x, self.y);
        self.x += a[k + 2];
        self.y += a[k + 3];
        let (d5, d6) = (self.x, self.y);
        self.x += a[k + 4];
        self.y += a[k + 5];
        self.start_path_if_needed(d, d2);
        oc.curveto(d3, d4, d5, d6, self.x, self.y);
    }

    pub fn hflex<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], _n: i32) {
        let mut d2 = self.x;
        let mut d = self.y;
        let d3 = d;
        self.x += a[0];
        let (mut d4, mut d5) = (self.x, self.y);
        self.x += a[1];
        self.y += a[2];
        let (mut d6, mut d7) = (self.x, self.y);
        self.x += a[3];
        self.start_path_if_needed(d2, d);
        oc.curveto(d4, d5, d6, d7, self.x, self.y);
        d2 = self.x;
        d = self.y;
        self.x += a[4];
        d4 = self.x;
        d5 = self.y;
        self.x += a[5];
        d6 = self.x;
        d7 = d3;
        self.x += a[6];
        self.y = d3;
        self.start_path_if_needed(d2, d);
        oc.curveto(d4, d5, d6, d7, self.x, d3);
    }

    pub fn flex<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], _n: i32) {
        let (mut d, mut d2) = (self.x, self.y);
        let mut k = 0;
        for _ in 0..2 {
            self.x += a[k];
            self.y += a[k + 1];
            let (d3, d4) = (self.x, self.y);
            self.x += a[k + 2];
            self.y += a[k + 3];
            let (d5, d6) = (self.x, self.y);
            self.x += a[k + 4];
            self.y += a[k + 5];
            k += 6;
            self.start_path_if_needed(d, d2);
            oc.curveto(d3, d4, d5, d6, self.x, self.y);
            d = self.x;
            d2 = self.y;
        }
    }

    pub fn hflex1<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], _n: i32) {
        let mut d2 = self.x;
        let mut d = self.y;
        let d3 = d;
        self.x += a[0];
        self.y += a[1];
        let (mut d4, mut d5) = (self.x, self.y);
        self.x += a[2];
        self.y += a[3];
        let (mut d6, mut d7) = (self.x, self.y);
        self.x += a[4];
        self.start_path_if_needed(d2, d);
        oc.curveto(d4, d5, d6, d7, self.x, self.y);
        d2 = self.x;
        d = self.y;
        self.x += a[5];
        d4 = self.x;
        d5 = self.y;
        self.x += a[6];
        self.y += a[7];
        d6 = self.x;
        d7 = self.y;
        self.x += a[8];
        self.y = d3;
        self.start_path_if_needed(d2, d);
        oc.curveto(d4, d5, d6, d7, self.x, self.y);
    }

    pub fn flex1<C: OutlineConsumer + ?Sized>(&mut self, oc: &mut C, a: &[f64], _n: i32) {
        let (d, d2) = (self.x, self.y);
        let (mut d3, mut d4) = (self.x, self.y);
        self.x += a[0];
        self.y += a[1];
        let (mut d5, mut d6) = (self.x, self.y);
        self.x += a[2];
        self.y += a[3];
        let (mut d7, mut d8) = (self.x, self.y);
        self.x += a[4];
        self.y += a[5];
        self.start_path_if_needed(d3, d4);
        oc.curveto(d5, d6, d7, d8, self.x, self.y);
        d3 = self.x;
        d4 = self.y;
        self.x += a[6];
        self.y += a[7];
        d5 = self.x;
        d6 = self.y;
        self.x += a[8];
        self.y += a[9];
        d7 = self.x;
        d8 = self.y;
        if (self.x - d).abs() > (self.y - d2).abs() {
            self.x += a[10];
            self.y = d2;
        } else {
            self.y += a[10];
            self.x = d;
        }
        self.start_path_if_needed(d3, d4);
        oc.curveto(d5, d6, d7, d8, self.x, self.y);
    }
}

/// The redirector bound to its consumer for one parse.
pub struct UnhintedBinding<'a> {
    pub r: &'a mut Type2UnhintedRedirector,
    pub oc: &'a mut dyn OutlineConsumer,
}

impl Type2Consumer for UnhintedBinding<'_> {
    fn endchar(&mut self, _a: &[f64], _n: i32) {
        self.r.endchar(self.oc);
    }
    fn rmoveto(&mut self, a: &[f64], n: i32) {
        self.r.rmoveto(self.oc, a, n);
    }
    fn hmoveto(&mut self, a: &[f64], n: i32) {
        self.r.hmoveto(self.oc, a, n);
    }
    fn vmoveto(&mut self, a: &[f64], n: i32) {
        self.r.vmoveto(self.oc, a, n);
    }
    fn moveto(&mut self, a: &[f64], n: i32) {
        self.r.moveto(self.oc, a, n);
    }
    fn rlineto(&mut self, a: &[f64], n: i32) {
        self.r.rlineto(self.oc, a, n);
    }
    fn hlineto(&mut self, a: &[f64], n: i32) {
        self.r.hlineto(self.oc, a, n);
    }
    fn vlineto(&mut self, a: &[f64], n: i32) {
        self.r.vlineto(self.oc, a, n);
    }
    fn rrcurveto(&mut self, a: &[f64], n: i32) {
        self.r.rrcurveto(self.oc, a, n);
    }
    fn hhcurveto(&mut self, a: &[f64], n: i32) {
        self.r.hhcurveto(self.oc, a, n);
    }
    fn hvcurveto(&mut self, a: &[f64], n: i32) {
        self.r.hvcurveto(self.oc, a, n);
    }
    fn rcurveline(&mut self, a: &[f64], n: i32) {
        self.r.rcurveline(self.oc, a, n);
    }
    fn rlinecurve(&mut self, a: &[f64], n: i32) {
        self.r.rlinecurve(self.oc, a, n);
    }
    fn vvcurveto(&mut self, a: &[f64], n: i32) {
        self.r.vvcurveto(self.oc, a, n);
    }
    fn vhcurveto(&mut self, a: &[f64], n: i32) {
        self.r.vhcurveto(self.oc, a, n);
    }
    fn hflex(&mut self, a: &[f64], n: i32) {
        self.r.hflex(self.oc, a, n);
    }
    fn flex(&mut self, a: &[f64], n: i32) {
        self.r.flex(self.oc, a, n);
    }
    fn hflex1(&mut self, a: &[f64], n: i32) {
        self.r.hflex1(self.oc, a, n);
    }
    fn flex1(&mut self, a: &[f64], n: i32) {
        self.r.flex1(self.oc, a, n);
    }
}
