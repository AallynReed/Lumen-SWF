//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2HintedRedirector`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_consumer::Type2Consumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_unhinted_redirector::Type2UnhintedRedirector;
use crate::asc::flex2::fonts::adobe::fontengine::font::hinted_outline_consumer::HintedOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

#[derive(Clone, Debug)]
struct Stem {
    e1: f64,
    e2: f64,
    is_vertical: bool,
}

/// `seacHintBase` is not reset between glyphs, as in Java.
#[derive(Clone, Debug)]
pub struct Type2HintedRedirector {
    pub base: Type2UnhintedRedirector,
    a: Vec<Stem>,
    last_hint_redirected: i32,
    seac_hint_base: i32,
    hintmask_seen: bool,
    new_hints: bool,
}

impl Default for Type2HintedRedirector {
    fn default() -> Self {
        Type2HintedRedirector::new()
    }
}

impl Type2HintedRedirector {
    pub fn new() -> Type2HintedRedirector {
        Type2HintedRedirector { base: Type2UnhintedRedirector::new(), a: Vec::new(), last_hint_redirected: -1, seac_hint_base: 0, hintmask_seen: false, new_hints: false }
    }

    pub fn reset(&mut self) {
        self.base.reset();
        self.a.clear();
        self.hintmask_seen = false;
        self.new_hints = false;
        self.last_hint_redirected = -1;
    }

    fn add_mask(&mut self, oc: &mut dyn HintedOutlineConsumer) {
        if !self.hintmask_seen || self.new_hints {
            let mut bl = self.hintmask_seen;
            let n = (self.last_hint_redirected + 1) as usize;
            for i in n..self.a.len() {
                let s = &self.a[i];
                oc.stem(s.e1, s.e2, bl, s.is_vertical, false);
                self.last_hint_redirected += 1;
                bl = false;
            }
            self.hintmask_seen = true;
            self.new_hints = false;
        }
    }

    fn process_mask(&mut self, oc: &mut dyn HintedOutlineConsumer, data: &CFFByteArray, n: i32, n2: i32, bl: bool) -> FontResult<()> {
        let mut bl2 = false;
        let mut bl3 = bl || self.hintmask_seen;
        for n3 in 0..n2 {
            if data.getcard8(n + n3)? == 0 {
                continue;
            }
            bl2 = true;
        }
        if bl2 {
            for n3 in 0..n2 {
                let n4 = data.getcard8(n + n3)?;
                if n4 == 0 {
                    continue;
                }
                for i in (0..=7).rev() {
                    if n4 & (1 << i) == 0 {
                        continue;
                    }
                    let n5 = n3 * 8 + (7 - i) + self.seac_hint_base;
                    if n5 >= self.a.len() as i32 {
                        return Err(FontEngineException::invalid_glyph("Invalid hint mask"));
                    }
                    let s = &self.a[n5 as usize];
                    oc.stem(s.e1, s.e2, bl3, s.is_vertical, bl);
                    bl3 = false;
                }
            }
        } else if bl {
            oc.no_counters();
        } else {
            oc.no_hints();
        }
        Ok(())
    }

    fn add_stem(&mut self, a: &[f64], n: i32, bl: bool) {
        let mut d = 0.0;
        if !self.new_hints {
            self.seac_hint_base = self.a.len() as i32;
            self.last_hint_redirected = self.seac_hint_base - 1;
            self.new_hints = true;
        }
        let mut k = if n % 2 != 0 { 1 } else { 0 };
        while k < n as usize {
            d += a[k];
            let d2 = d + a[k + 1];
            k += 2;
            self.a.push(Stem { e1: d, e2: d2, is_vertical: bl });
            d = d2;
        }
    }
}

/// The redirector bound to its consumer for one parse.
pub struct HintedBinding<'a> {
    pub r: &'a mut Type2HintedRedirector,
    pub oc: &'a mut dyn HintedOutlineConsumer,
}

impl Type2Consumer for HintedBinding<'_> {
    fn cntrmask(&mut self, _s: &[f64], _n: i32, data: &CFFByteArray, n2: i32, n3: i32) -> FontResult<()> {
        self.r.process_mask(self.oc, data, n2, n3, true)
    }

    fn endchar(&mut self, _a: &[f64], _n: i32) {
        self.oc.endchar();
    }

    fn rmoveto(&mut self, a: &[f64], n: i32) {
        self.r.add_mask(self.oc);
        self.r.base.rmoveto(self.oc, a, n);
    }

    fn hmoveto(&mut self, a: &[f64], n: i32) {
        self.r.add_mask(self.oc);
        self.r.base.hmoveto(self.oc, a, n);
    }

    fn vmoveto(&mut self, a: &[f64], n: i32) {
        self.r.add_mask(self.oc);
        self.r.base.vmoveto(self.oc, a, n);
    }

    fn moveto(&mut self, a: &[f64], n: i32) {
        self.r.add_mask(self.oc);
        self.r.base.moveto(self.oc, a, n);
    }

    fn rlineto(&mut self, a: &[f64], n: i32) {
        self.r.base.rlineto(self.oc, a, n);
    }
    fn hlineto(&mut self, a: &[f64], n: i32) {
        self.r.base.hlineto(self.oc, a, n);
    }
    fn vlineto(&mut self, a: &[f64], n: i32) {
        self.r.base.vlineto(self.oc, a, n);
    }
    fn rrcurveto(&mut self, a: &[f64], n: i32) {
        self.r.base.rrcurveto(self.oc, a, n);
    }
    fn hhcurveto(&mut self, a: &[f64], n: i32) {
        self.r.base.hhcurveto(self.oc, a, n);
    }
    fn hvcurveto(&mut self, a: &[f64], n: i32) {
        self.r.base.hvcurveto(self.oc, a, n);
    }
    fn rcurveline(&mut self, a: &[f64], n: i32) {
        self.r.base.rcurveline(self.oc, a, n);
    }
    fn rlinecurve(&mut self, a: &[f64], n: i32) {
        self.r.base.rlinecurve(self.oc, a, n);
    }
    fn vvcurveto(&mut self, a: &[f64], n: i32) {
        self.r.base.vvcurveto(self.oc, a, n);
    }
    fn vhcurveto(&mut self, a: &[f64], n: i32) {
        self.r.base.vhcurveto(self.oc, a, n);
    }

    fn flex(&mut self, a: &[f64], _n: i32) {
        let b = &mut self.r.base;
        let mut p = [0.0f64; 12];
        for i in 0..6 {
            b.x += a[2 * i];
            b.y += a[2 * i + 1];
            p[2 * i] = b.x;
            p[2 * i + 1] = b.y;
        }
        self.oc.flex(a[12], p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9], b.x, b.y);
    }

    fn flex1(&mut self, a: &[f64], _n: i32) {
        let b = &mut self.r.base;
        let (d, d2) = (b.x, b.y);
        let mut p = [0.0f64; 10];
        for i in 0..5 {
            b.x += a[2 * i];
            b.y += a[2 * i + 1];
            p[2 * i] = b.x;
            p[2 * i + 1] = b.y;
        }
        if (b.x - d).abs() > (b.y - d2).abs() {
            b.x += a[10];
            b.y = d2;
        } else {
            b.y += a[10];
            b.x = d;
        }
        self.oc.flex(50.0, p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9], b.x, b.y);
    }

    fn hflex(&mut self, a: &[f64], _n: i32) {
        let b = &mut self.r.base;
        let d = b.y;
        b.x += a[0];
        let d2 = b.x;
        b.x += a[1];
        b.y += a[2];
        let d3 = b.x;
        b.x += a[3];
        let d4 = b.x;
        b.x += a[4];
        let d5 = b.x;
        b.x += a[5];
        let d6 = b.x;
        b.x += a[6];
        let y = b.y;
        self.oc.flex(50.0, d2, d, d3, y, d4, y, d5, y, d6, d, b.x, d);
        b.y = d;
    }

    fn hflex1(&mut self, a: &[f64], _n: i32) {
        let b = &mut self.r.base;
        let d = b.y;
        b.x += a[0];
        b.y += a[1];
        let (d2, d3) = (b.x, b.y);
        b.x += a[2];
        b.y += a[3];
        let (d4, d5) = (b.x, b.y);
        b.x += a[4];
        let d6 = b.x;
        b.x += a[5];
        let d7 = b.x;
        b.x += a[6];
        b.y += a[7];
        let (d8, d9) = (b.x, b.y);
        b.x += a[8];
        b.y = d;
        self.oc.flex(50.0, d2, d3, d4, d5, d6, d5, d7, d5, d8, d9, b.x, b.y);
    }

    fn hintmask(&mut self, _s: &[f64], _n: i32, data: &CFFByteArray, n2: i32, n3: i32) -> FontResult<()> {
        self.r.process_mask(self.oc, data, n2, n3, false)?;
        self.r.hintmask_seen = true;
        self.r.new_hints = false;
        Ok(())
    }

    fn hstem(&mut self, a: &[f64], n: i32) {
        self.r.add_stem(a, n, false);
    }

    fn hstemhm(&mut self, a: &[f64], n: i32) {
        self.r.add_stem(a, n, false);
    }

    fn implicit_vstemhm(&mut self, a: &[f64], n: i32) {
        self.r.add_stem(a, n, true);
    }

    fn vstem(&mut self, a: &[f64], n: i32) {
        self.r.add_stem(a, n, true);
    }

    fn vstemhm(&mut self, a: &[f64], n: i32) {
        self.r.add_stem(a, n, true);
    }

    fn width(&mut self, d: f64) -> bool {
        self.oc.width(d)
    }

    fn global_color_me(&mut self, _a: &[f64], _n: i32) {
        self.oc.global_color_on();
    }
}
