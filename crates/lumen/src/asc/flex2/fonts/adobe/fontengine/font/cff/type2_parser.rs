//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2Parser`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_keyed_font::NameKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_consumer::Type2Consumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::postscript::seac_phase::SeacPhase;
use crate::asc::flex2::fonts::adobe::fontengine::font::postscript::standard_encoding;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

pub struct Type2Parser {
    stack: [f64; 48],
    storage: [f64; 32],
    nb_hints: i32,
    stack_depth: i32,
    call_depth: i32,
    width_may_be_there: bool,
    seac_phase: SeacPhase,
    seac_accent_x: f64,
    seac_accent_y: f64,
}

impl Default for Type2Parser {
    fn default() -> Self {
        Type2Parser::new()
    }
}

fn subr_bias(count: i32) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}

impl Type2Parser {
    pub fn new() -> Type2Parser {
        Type2Parser {
            stack: [0.0; 48],
            storage: [0.0; 32],
            nb_hints: 0,
            stack_depth: 0,
            call_depth: 0,
            width_may_be_there: false,
            seac_phase: SeacPhase::SeacNone,
            seac_accent_x: 0.0,
            seac_accent_y: 0.0,
        }
    }

    fn check(&self, bl: bool) -> FontResult<()> {
        if !bl {
            return Err(FontEngineException::invalid_glyph("invalid Type2 charstring"));
        }
        Ok(())
    }

    fn check_int(&self, d: f64) -> FontResult<i32> {
        let n = d as i32;
        if n as f64 != d {
            return Err(FontEngineException::invalid_glyph("unexpected non-integer value"));
        }
        Ok(n)
    }

    fn handle_width(&mut self, c: &mut dyn Type2Consumer) -> FontResult<bool> {
        let mut bl = true;
        if self.stack_depth == 1 {
            if self.width_may_be_there {
                bl = c.width(self.stack[0]);
            } else if self.seac_phase == SeacPhase::SeacNone {
                return Err(FontEngineException::invalid_glyph("stack is non empty"));
            }
            self.stack_depth = 0;
        }
        self.width_may_be_there = false;
        Ok(bl)
    }

    fn sd(&self) -> &[f64] {
        &self.stack[..]
    }

    fn push(&mut self, d: f64) {
        self.stack[self.stack_depth as usize] = d;
        self.stack_depth += 1;
    }

    fn pop(&mut self) -> f64 {
        self.stack_depth -= 1;
        self.stack[self.stack_depth as usize]
    }

    pub fn parse(&mut self, cs: &CharStrings, n: i32, lsubrs: Option<&CharStrings>, gsubrs: Option<&CharStrings>, c: &mut dyn Type2Consumer, nkf: Option<&NameKeyedFont>) -> FontResult<()> {
        self.width_may_be_there = true;
        self.stack_depth = 0;
        self.call_depth = 0;
        self.nb_hints = 0;
        self.seac_phase = SeacPhase::SeacNone;
        let (n3, n2) = match cs.checked_range(n) {
            Some(r) => r,
            None => {
                return Err(FontEngineException::new(ExceptionKind::InvalidGlyph, "Glyph past end of charstrings array"));
            }
        };
        let r = self.parse_range(&cs.data, n3, n2, lsubrs, gsubrs, c, nkf).and_then(|bl| {
            if bl {
                let d = self.stack_depth;
                c.endchar(self.sd(), d);
            }
            Ok(())
        });
        match r {
            Err(e) if matches!(e.kind, ExceptionKind::InvalidFont | ExceptionKind::InvalidGlyph) => Err(FontEngineException::from_cause(ExceptionKind::InvalidGlyph, e.to_java_string())),
            r => r,
        }
    }

    fn seac_stem_adjust(&mut self, d: f64) {
        if self.seac_phase == SeacPhase::SeacAccentPreMove {
            let i = if self.stack_depth % 2 != 0 { 1 } else { 0 };
            self.stack[i] += d;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_range(&mut self, data: &CFFByteArray, mut n: i32, n2: i32, lsubrs: Option<&CharStrings>, gsubrs: Option<&CharStrings>, c: &mut dyn Type2Consumer, nkf: Option<&NameKeyedFont>) -> FontResult<bool> {
        while n < n2 {
            let n3 = data.getcard8(n)?;
            n += 1;
            match n3 {
                1 | 3 | 18 | 23 => {
                    self.check(self.stack_depth >= 2)?;
                    self.nb_hints += self.stack_depth / 2;
                    if n3 == 18 {
                        if self.seac_phase == SeacPhase::SeacAccentPreMove {
                            let i = (self.stack_depth % 2) as usize;
                            self.stack[i] += self.seac_accent_y;
                        }
                    } else {
                        let d = if n3 == 1 { self.seac_accent_y } else { self.seac_accent_x };
                        self.seac_stem_adjust(d);
                    }
                    let sd = self.stack_depth;
                    match n3 {
                        1 => c.hstem(self.sd(), sd),
                        3 => c.vstem(self.sd(), sd),
                        18 => c.hstemhm(self.sd(), sd),
                        _ => c.vstemhm(self.sd(), sd),
                    }
                    self.stack_depth %= 2;
                    if !self.handle_width(c)? {
                        return Ok(false);
                    }
                }
                4 => {
                    self.check(self.stack_depth == 1 || self.stack_depth == 2)?;
                    if self.seac_phase == SeacPhase::SeacAccentPreMove {
                        if self.stack_depth == 2 {
                            self.stack[2] = self.stack[1] + self.seac_accent_y;
                            self.stack[1] = self.seac_accent_x;
                        } else {
                            self.stack[1] = self.stack[0] + self.seac_accent_y;
                            self.stack[0] = self.seac_accent_x;
                        }
                        self.stack_depth += 1;
                        let sd = self.stack_depth;
                        c.moveto(self.sd(), sd);
                        self.stack_depth -= 2;
                        self.seac_phase = SeacPhase::SeacAccentPostMove;
                    } else {
                        let sd = self.stack_depth;
                        c.vmoveto(self.sd(), sd);
                        self.stack_depth -= 1;
                    }
                    if !self.handle_width(c)? {
                        return Ok(false);
                    }
                }
                5 => {
                    self.check(self.stack_depth >= 2 && self.stack_depth % 2 == 0)?;
                    let sd = self.stack_depth;
                    c.rlineto(self.sd(), sd);
                    self.stack_depth = 0;
                }
                6 => {
                    self.check(self.stack_depth > 0)?;
                    let sd = self.stack_depth;
                    c.hlineto(self.sd(), sd);
                    self.stack_depth = 0;
                }
                7 => {
                    self.check(self.stack_depth > 0)?;
                    let sd = self.stack_depth;
                    c.vlineto(self.sd(), sd);
                    self.stack_depth = 0;
                }
                8 => {
                    self.check(self.stack_depth > 0 && self.stack_depth % 6 == 0)?;
                    let sd = self.stack_depth;
                    c.rrcurveto(self.sd(), sd);
                    self.stack_depth = 0;
                }
                10 | 29 => {
                    self.check(self.stack_depth >= 1)?;
                    let subrs = if n3 == 10 {
                        self.check(lsubrs.is_some())?;
                        lsubrs.unwrap()
                    } else {
                        gsubrs.expect("java.lang.NullPointerException: global subrs")
                    };
                    let v = self.pop();
                    let mut n8 = self.check_int(v)?;
                    n8 += subr_bias(subrs.get_count());
                    let sd = self.stack_depth;
                    if n3 == 10 {
                        c.callsubr(self.sd(), sd, n8);
                    } else {
                        c.callgsubr(self.sd(), sd, n8);
                    }
                    self.call_depth += 1;
                    let (a, b) = subrs.checked_range(n8).unwrap_or_else(|| panic!("java.lang.ArrayIndexOutOfBoundsException: subr {}", n8));
                    let bl = self.parse_range(&subrs.data, a, b, lsubrs, gsubrs, c, nkf)?;
                    self.call_depth -= 1;
                    if !bl {
                        return Ok(bl);
                    }
                }
                11 => {
                    self.check(self.call_depth > 0)?;
                    let sd = self.stack_depth;
                    c.return_op(self.sd(), sd);
                    return Ok(true);
                }
                12 => {
                    let n9 = data.getcard8(n)?;
                    n += 1;
                    self.escape(n9, c)?;
                }
                14 => {
                    if self.seac_phase != SeacPhase::SeacNone {
                        return Ok(false);
                    }
                    if self.stack_depth == 4 || self.stack_depth == 5 {
                        let mut n17 = 0usize;
                        if self.stack_depth == 5 {
                            self.stack_depth = 1;
                            if !self.handle_width(c)? {
                                return Ok(false);
                            }
                            self.stack_depth = 5;
                            n17 = 1;
                        }
                        let sd = self.stack_depth;
                        if c.seac(self.sd(), sd) {
                            self.seac_phase = SeacPhase::SeacBase;
                            self.seac_accent_x = self.stack[n17];
                            self.seac_accent_y = self.stack[n17 + 1];
                            let mut n18 = self.stack[n17 + 2] as i32;
                            let mut n19 = self.stack[n17 + 3] as i32;
                            let nk = match nkf {
                                None => return Err(FontEngineException::invalid_glyph("Seac found in non-nk font")),
                                Some(f) => f,
                            };
                            n18 = nk.glyph_name2gid(standard_encoding::NAMES[n18 as usize])?;
                            n19 = nk.glyph_name2gid(standard_encoding::NAMES[n19 as usize])?;
                            self.stack_depth = 0;
                            let ncs = &nk.char_strings;
                            self.parse_range(&ncs.data, ncs.offset_of(n18), ncs.offset_following(n18), lsubrs, gsubrs, c, nkf)?;
                            self.seac_phase = SeacPhase::SeacAccentPreMove;
                            self.nb_hints = 0;
                            self.stack_depth = 0;
                            self.parse_range(&ncs.data, ncs.offset_of(n19), ncs.offset_following(n19), lsubrs, gsubrs, c, nkf)?;
                        }
                        self.stack_depth = 0;
                    }
                    self.handle_width(c)?;
                    let sd = self.stack_depth;
                    c.endchar(self.sd(), sd);
                    return Ok(false);
                }
                19 | 20 => {
                    if self.stack_depth > 1 {
                        self.nb_hints += self.stack_depth / 2;
                        let d = self.seac_accent_x;
                        self.seac_stem_adjust(d);
                        let sd = self.stack_depth;
                        c.implicit_vstemhm(self.sd(), sd);
                        self.stack_depth %= 2;
                    }
                    if !self.handle_width(c)? {
                        return Ok(false);
                    }
                    let n23 = (self.nb_hints + 7) / 8;
                    let sd = self.stack_depth;
                    if n3 == 19 {
                        c.hintmask(self.sd(), sd, data, n, n23)?;
                    } else {
                        c.cntrmask(self.sd(), sd, data, n, n23)?;
                    }
                    n += n23;
                    self.stack_depth = 0;
                }
                21 => {
                    self.check(self.stack_depth == 2 || self.stack_depth == 3)?;
                    let sd = self.stack_depth;
                    if self.seac_phase == SeacPhase::SeacAccentPreMove {
                        if self.stack_depth == 2 {
                            self.stack[0] += self.seac_accent_x;
                            self.stack[1] += self.seac_accent_y;
                        } else {
                            self.stack[1] += self.seac_accent_x;
                            self.stack[2] += self.seac_accent_y;
                        }
                        self.seac_phase = SeacPhase::SeacAccentPostMove;
                        c.moveto(self.sd(), sd);
                    } else {
                        c.rmoveto(self.sd(), sd);
                    }
                    self.stack_depth -= 2;
                    if !self.handle_width(c)? {
                        return Ok(false);
                    }
                }
                22 => {
                    self.check(self.stack_depth == 1 || self.stack_depth == 2)?;
                    if self.seac_phase == SeacPhase::SeacAccentPreMove {
                        if self.stack_depth == 2 {
                            self.stack[2] = self.seac_accent_y;
                            self.stack[1] += self.seac_accent_x;
                        } else {
                            self.stack[1] = self.seac_accent_y;
                            self.stack[0] += self.seac_accent_x;
                        }
                        self.stack_depth += 1;
                        let sd = self.stack_depth;
                        c.moveto(self.sd(), sd);
                        self.stack_depth -= 2;
                        self.seac_phase = SeacPhase::SeacAccentPostMove;
                    } else {
                        let sd = self.stack_depth;
                        c.hmoveto(self.sd(), sd);
                        self.stack_depth -= 1;
                    }
                    if !self.handle_width(c)? {
                        return Ok(false);
                    }
                }
                24 => {
                    self.check(self.stack_depth >= 8 && self.stack_depth % 6 == 2)?;
                    let sd = self.stack_depth;
                    c.rcurveline(self.sd(), sd);
                    self.stack_depth = 0;
                }
                25 => {
                    self.check(self.stack_depth >= 8 && self.stack_depth % 2 == 0)?;
                    let sd = self.stack_depth;
                    c.rlinecurve(self.sd(), sd);
                    self.stack_depth = 0;
                }
                26 | 27 => {
                    self.check(self.stack_depth >= 4 && self.stack_depth % 4 <= 1)?;
                    let sd = self.stack_depth;
                    if n3 == 26 {
                        c.vvcurveto(self.sd(), sd);
                    } else {
                        c.hhcurveto(self.sd(), sd);
                    }
                    self.stack_depth = 0;
                }
                28 => {
                    self.check(self.stack_depth < 48)?;
                    let n29 = data.getint16(n)?;
                    n += 2;
                    let sd = self.stack_depth;
                    c.integer(self.sd(), sd, n29);
                    self.push(n29 as f64);
                }
                30 | 31 => {
                    let m = self.stack_depth % 8;
                    self.check(self.stack_depth >= 4 && (m == 0 || m == 1 || m == 4 || m == 5))?;
                    let sd = self.stack_depth;
                    if n3 == 30 {
                        c.vhcurveto(self.sd(), sd);
                    } else {
                        c.hvcurveto(self.sd(), sd);
                    }
                    self.stack_depth = 0;
                }
                255 => {
                    self.check(self.stack_depth < 48)?;
                    let d = data.getint32(n)? as f64 / 65536.0;
                    n += 4;
                    let sd = self.stack_depth;
                    c.real(self.sd(), sd, d);
                    self.push(d);
                }
                32..=246 => {
                    self.check(self.stack_depth < 48)?;
                    let v = n3 - 139;
                    let sd = self.stack_depth;
                    c.integer(self.sd(), sd, v);
                    self.push(v as f64);
                }
                247..=250 => {
                    self.check(self.stack_depth < 48)?;
                    let n32 = data.getcard8(n)?;
                    n += 1;
                    let v = (n3 - 247) * 256 + n32 + 108;
                    let sd = self.stack_depth;
                    c.integer(self.sd(), sd, v);
                    self.push(v as f64);
                }
                251..=254 => {
                    self.check(self.stack_depth < 48)?;
                    let n34 = data.getcard8(n)?;
                    n += 1;
                    let v = -(n3 - 251) * 256 - n34 - 108;
                    let sd = self.stack_depth;
                    c.integer(self.sd(), sd, v);
                    self.push(v as f64);
                }
                _ => return Err(FontEngineException::invalid_glyph(format!("invalid Type2 operator ({})", n3))),
            }
        }
        self.check(false)?;
        Ok(false)
    }

    fn escape(&mut self, n9: i32, c: &mut dyn Type2Consumer) -> FontResult<()> {
        let sd = self.stack_depth;
        match n9 {
            0 => {}
            3 | 4 | 10 | 11 | 12 | 15 | 24 | 28 => {
                self.check(self.stack_depth >= 2)?;
                match n9 {
                    3 => c.and(self.sd(), sd),
                    4 => c.or(self.sd(), sd),
                    10 => c.add(self.sd(), sd),
                    11 => c.sub(self.sd(), sd),
                    12 => c.div(self.sd(), sd),
                    15 => c.eq(self.sd(), sd),
                    24 => c.mul(self.sd(), sd),
                    _ => c.exch(self.sd(), sd),
                }
                let d = self.pop();
                let d2 = self.pop();
                match n9 {
                    3 => self.push(if d2 == 0.0 || d == 0.0 { 0.0 } else { 1.0 }),
                    4 => self.push(if d2 == 0.0 && d == 0.0 { 0.0 } else { 1.0 }),
                    10 => self.push(d2 + d),
                    11 => self.push(d2 - d),
                    12 => self.push(d2 / d),
                    15 => self.push(if d2 == d { 1.0 } else { 0.0 }),
                    24 => self.push(d2 * d),
                    _ => {
                        self.push(d);
                        self.push(d2);
                    }
                }
            }
            5 | 9 | 14 | 26 => {
                self.check(self.stack_depth >= 1)?;
                match n9 {
                    5 => c.not(self.sd(), sd),
                    9 => c.abs(self.sd(), sd),
                    14 => c.neg(self.sd(), sd),
                    _ => c.sqrt(self.sd(), sd),
                }
                let d = self.pop();
                match n9 {
                    5 => self.push(if d == 0.0 { 1.0 } else { 0.0 }),
                    9 => self.push(if d < 0.0 { -d } else { d }),
                    14 => self.push(-d),
                    _ => self.push(d.sqrt()),
                }
            }
            18 => {
                self.check(self.stack_depth >= 1)?;
                c.drop(self.sd(), sd);
                self.stack_depth -= 1;
            }
            20 => {
                self.check(self.stack_depth >= 2)?;
                c.put(self.sd(), sd);
                let v = self.pop();
                let n10 = self.check_int(v)?;
                let d = self.pop();
                self.storage[n10 as usize] = d;
            }
            21 => {
                self.check(self.stack_depth >= 1)?;
                c.get(self.sd(), sd);
                let v = self.pop();
                let n11 = self.check_int(v)?;
                let d = self.storage[n11 as usize];
                self.push(d);
            }
            22 => {
                self.check(self.stack_depth >= 4)?;
                c.ifelse(self.sd(), sd);
                let d = self.pop();
                let d8 = self.pop();
                let d9 = self.pop();
                let d10 = self.pop();
                self.push(if d8 <= d { d10 } else { d9 });
            }
            23 => {
                self.check(self.stack_depth < 48)?;
                c.random(self.sd(), sd);
                self.push(0.0);
            }
            27 => {
                self.check(self.stack_depth >= 1)?;
                self.check(self.stack_depth < 48)?;
                c.dup(self.sd(), sd);
                let d = self.pop();
                self.push(d);
                self.push(d);
            }
            29 => {
                self.check(self.stack_depth >= 1)?;
                let n12 = self.check_int(self.stack[self.stack_depth as usize])?;
                self.check(self.stack_depth >= n12 + 1)?;
                c.index(self.sd(), sd);
                self.stack_depth -= 1;
                let i = self.stack_depth;
                self.stack_depth += 1;
                self.stack[i as usize] = self.stack[(self.stack_depth - n12 - 1) as usize];
            }
            30 => {
                self.check(self.stack_depth >= 2)?;
                self.check(self.stack[(self.stack_depth - 2) as usize] > 0.0)?;
                self.check(self.stack_depth as f64 >= self.stack[(self.stack_depth - 2) as usize] + 2.0)?;
                c.roll(self.sd(), sd);
                let v = self.pop();
                let mut n13 = self.check_int(v)?;
                let v = self.pop();
                let n14 = self.check_int(v)?;
                if n13 < 0 {
                    n13 = n14 - -n13 % n14;
                }
                let n15 = self.stack_depth - 1;
                let n16 = self.stack_depth - n14;
                n13 %= n14;
                self.reverse_stack_range(n15 - n13 + 1, n15);
                self.reverse_stack_range(n16, n15 - n13);
                self.reverse_stack_range(n16, n15);
            }
            34 | 35 | 36 | 37 => {
                let need = match n9 {
                    34 => 7,
                    35 => 13,
                    36 => 9,
                    _ => 11,
                };
                self.check(self.stack_depth == need)?;
                match n9 {
                    34 => c.hflex(self.sd(), sd),
                    35 => c.flex(self.sd(), sd),
                    36 => c.hflex1(self.sd(), sd),
                    _ => c.flex1(self.sd(), sd),
                }
                self.stack_depth = 0;
            }
            38 => c.global_color_me(self.sd(), sd),
            _ => return Err(FontEngineException::invalid_glyph(format!("Invalid Type2 operator (12, {})", n9))),
        }
        Ok(())
    }

    fn reverse_stack_range(&mut self, mut n: i32, mut n2: i32) {
        while n < n2 {
            self.stack.swap(n as usize, n2 as usize);
            n += 1;
            n2 -= 1;
        }
    }
}
