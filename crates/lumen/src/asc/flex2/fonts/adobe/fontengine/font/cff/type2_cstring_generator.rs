//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2CStringGenerator`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::Key;
use crate::asc::flex2::fonts::adobe::fontengine::font::hinted_outline_consumer::HintedOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::postscript::sub_arrays::array_compare;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

const MAX_STACK: i32 = 48;
const MAX_STEMS: i32 = 96;
const STANDARD_FLEX_DEPTH: f64 = 50.0;
pub const DEFAULT_NOMINAL_WIDTH: f64 = 0.0;
pub const DEFAULT_DEFAULT_WIDTH: f64 = 0.0;
const BOTTOM_EDGE: f64 = -21.0;
const TOP_EDGE: f64 = -20.0;

/// `Math.round(double)`.
pub fn java_math_round(a: f64) -> i64 {
    let bits = a.to_bits() as i64;
    let biased_exp = (bits & 0x7ff0_0000_0000_0000) >> 52;
    let shift = (52 - 1 + 1023) - biased_exp;
    if shift & -64 == 0 {
        let mut r = (bits & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000;
        if bits < 0 {
            r = -r;
        }
        ((r >> shift) + 1) >> 1
    } else {
        a as i64
    }
}

/// Java `Collections.binarySearch(list, key, c)` (indexed): compares `(list[mid], key)`.
pub fn java_binary_search<T>(list: &[T], key: &T, cmp: impl Fn(&T, &T) -> i32) -> i32 {
    let mut low: i32 = 0;
    let mut high: i32 = list.len() as i32 - 1;
    while low <= high {
        let mid = ((low + high) as u32 >> 1) as i32;
        let c = cmp(&list[mid as usize], key);
        if c < 0 {
            low = mid + 1;
        } else if c > 0 {
            high = mid - 1;
        } else {
            return mid;
        }
    }
    -(low + 1)
}

pub struct Type2CStringGenerator {
    curr_cstring_index: i32,
    pub glyphs: Vec<Option<GlyphInfo>>,
    widths_analyzed: Vec<bool>,
    nominal_width: Vec<f64>,
    default_width: Vec<f64>,
    current_x: f64,
    current_y: f64,
    start_of_seq_op: i32,
    move_to_seen: bool,
    last_op_code: i32,
    stack_count: i32,
    stack: [f64; 48],
    hint_mask_index: i32,
    hint_mask_seen: bool,
    cntr_mask_seen: bool,
    hintmask: [u8; 12],
    cntrmask: [u8; 12],
    subrize: bool,
}

impl Type2CStringGenerator {
    /// The 2-argument constructor.
    pub fn new2(n: i32, n2: i32) -> Type2CStringGenerator {
        Type2CStringGenerator::new(n, n2, false)
    }

    pub fn new(n: i32, n2: i32, bl: bool) -> Type2CStringGenerator {
        let mut g = Type2CStringGenerator {
            curr_cstring_index: 0,
            glyphs: Vec::new(),
            widths_analyzed: Vec::new(),
            nominal_width: Vec::new(),
            default_width: Vec::new(),
            current_x: 0.0,
            current_y: 0.0,
            start_of_seq_op: 0,
            move_to_seen: false,
            last_op_code: 0,
            stack_count: 0,
            stack: [0.0; 48],
            hint_mask_index: 0,
            hint_mask_seen: false,
            cntr_mask_seen: false,
            hintmask: [0; 12],
            cntrmask: [0; 12],
            subrize: false,
        };
        g.init(n, n2, bl);
        g
    }

    fn init(&mut self, n: i32, n2: i32, bl: bool) {
        self.glyphs = (0..n).map(|_| None).collect();
        self.widths_analyzed = vec![false; n2 as usize];
        self.nominal_width = vec![0.0; n2 as usize];
        self.default_width = vec![0.0; n2 as usize];
        self.subrize = bl;
        self.reset_state();
    }

    fn reset_state(&mut self) {
        self.move_to_seen = false;
        self.last_op_code = 0;
        self.stack_count = 0;
        self.current_x = 0.0;
        self.current_y = 0.0;
        self.start_of_seq_op = 0;
        self.hint_mask_seen = false;
        self.cntr_mask_seen = false;
        self.hint_mask_index = -1;
        self.hintmask = [0; 12];
        self.cntrmask = [0; 12];
    }

    fn glyph(&mut self) -> &mut GlyphInfo {
        self.glyphs[self.curr_cstring_index as usize].as_mut().expect("java.lang.NullPointerException: no newGlyph")
    }

    fn push(&mut self, d: f64) {
        self.stack[self.stack_count as usize] = d;
        self.stack_count += 1;
    }

    fn stack_to_cstring(&mut self) {
        if self.stack_count != 0 {
            for i in 0..self.stack_count {
                let v = self.stack[i as usize];
                self.glyph().tmpcstr.num_to_cstring(v);
            }
            self.stack_count = 0;
        }
        let op = match self.last_op_code {
            6 | 7 | 30 | 31 => self.start_of_seq_op,
            o => o,
        };
        self.glyph().tmpcstr.add_byte(op);
        self.last_op_code = 0;
    }

    fn ensure_stack_space(&mut self, n: i32) {
        if self.stack_count + n + if self.subrize { 1 } else { 0 } > MAX_STACK {
            self.stack_to_cstring();
        }
    }

    pub fn new_glyph(&mut self, n: i32, n2: i32, d: f64, d2: f64) {
        self.reset_state();
        self.curr_cstring_index = n;
        self.glyphs[n as usize] = Some(GlyphInfo::new(n2, d));
        self.default_width[n2 as usize] = d;
        self.nominal_width[n2 as usize] = d2;
    }

    fn new_hint_mask(&mut self) {
        if self.last_op_code == 0 && self.hint_mask_index == self.glyph().tmpcstr.cstr_len {
            self.hintmask = [0; 12];
        } else {
            if self.last_op_code != 0 {
                self.stack_to_cstring();
            }
            if self.hint_mask_seen {
                let hm = HintMask::new(self.hint_mask_index, &self.hintmask);
                self.glyph().hint_masks.push(hm);
                self.hintmask = [0; 12];
            } else {
                self.hint_mask_seen = true;
            }
            self.hint_mask_index = self.glyph().tmpcstr.cstr_len;
        }
    }

    fn new_cntr_mask(&mut self) {
        if self.last_op_code != 0 {
            self.stack_to_cstring();
        }
        if self.cntr_mask_seen {
            let cm = CntrMask::new(&self.cntrmask);
            self.glyph().cntr_masks.push(cm);
            self.cntrmask = [0; 12];
        } else {
            self.cntr_mask_seen = true;
        }
    }

    fn add_id_to_mask(n: i32, a: &mut [u8; 12]) {
        if n < MAX_STEMS {
            let n2 = (n / 8) as usize;
            a[n2] |= (1 << (n % 8)) as u8;
        }
    }

    fn check_for_new_hint_mask(&mut self, bl: bool) {
        let hs = self.glyph().hints.len();
        if self.move_to_seen && (bl || hs == 0) || !self.move_to_seen && bl && hs > 0 {
            self.new_hint_mask();
        }
    }

    fn check_for_new_cntr_mask(&mut self, bl: bool) {
        if !self.cntr_mask_seen || bl {
            self.new_cntr_mask();
        }
    }

    fn cntr_hint(&mut self, d: f64, d2: f64, bl: bool) {
        let n = self.glyph().find_hint(d, d2, bl, true);
        Self::add_id_to_mask(n, &mut self.cntrmask);
        if self.hint_mask_seen {
            Self::add_id_to_mask(n, &mut self.hintmask);
        } else {
            Self::add_id_to_mask(n, &mut self.glyph().init_mask);
        }
    }

    fn add_vh_curve_to(&mut self, d: f64, d2: f64, d3: f64, d4: f64) {
        self.ensure_stack_space(4);
        match self.last_op_code {
            31 => {}
            0 => self.start_of_seq_op = 30,
            _ => {
                self.stack_to_cstring();
                self.start_of_seq_op = 30;
            }
        }
        self.push(d);
        self.push(d2);
        self.push(d3);
        self.push(d4);
        self.last_op_code = 30;
    }

    fn add_vh_curve_to5(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64) {
        self.ensure_stack_space(5);
        self.add_vh_curve_to(d, d2, d3, d4);
        self.push(d5);
        self.stack_to_cstring();
    }

    fn add_vv_curve_to(&mut self, d: f64, d2: f64, d3: f64, d4: f64) {
        self.ensure_stack_space(4);
        match self.last_op_code {
            0 | 26 => {}
            _ => self.stack_to_cstring(),
        }
        self.push(d);
        self.push(d2);
        self.push(d3);
        self.push(d4);
        self.last_op_code = 26;
    }

    fn add_vv_curve_to5(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64) {
        self.ensure_stack_space(5);
        if self.last_op_code != 0 {
            self.stack_to_cstring();
        }
        self.push(d);
        self.push(d2);
        self.push(d3);
        self.push(d4);
        self.push(d5);
        self.last_op_code = 26;
    }

    fn add_hv_curve_to(&mut self, d: f64, d2: f64, d3: f64, d4: f64) {
        self.ensure_stack_space(4);
        match self.last_op_code {
            30 => {}
            0 => self.start_of_seq_op = 31,
            _ => {
                self.stack_to_cstring();
                self.start_of_seq_op = 31;
            }
        }
        self.push(d);
        self.push(d2);
        self.push(d3);
        self.push(d4);
        self.last_op_code = 31;
    }

    fn add_hv_curve_to5(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64) {
        self.ensure_stack_space(5);
        self.add_hv_curve_to(d, d2, d3, d5);
        self.push(d4);
        self.stack_to_cstring();
    }

    fn add_hh_curve_to(&mut self, d: f64, d2: f64, d3: f64, d4: f64) {
        self.ensure_stack_space(4);
        match self.last_op_code {
            0 | 27 => {}
            _ => self.stack_to_cstring(),
        }
        self.push(d);
        self.push(d2);
        self.push(d3);
        self.push(d4);
        self.last_op_code = 27;
    }

    fn add_hh_curve_to5(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64) {
        self.ensure_stack_space(5);
        if self.last_op_code != 0 {
            self.stack_to_cstring();
        }
        self.push(d2);
        self.push(d);
        self.push(d3);
        self.push(d4);
        self.push(d5);
        self.last_op_code = 27;
    }

    fn add_rr_curve_to(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64) {
        self.ensure_stack_space(6);
        match self.last_op_code {
            5 => {
                self.push(d);
                self.push(d2);
                self.push(d3);
                self.push(d4);
                self.push(d5);
                self.push(d6);
                self.last_op_code = 25;
                self.stack_to_cstring();
            }
            o => {
                if o != 0 && o != 8 {
                    self.stack_to_cstring();
                }
                self.push(d);
                self.push(d2);
                self.push(d3);
                self.push(d4);
                self.push(d5);
                self.push(d6);
                self.last_op_code = 8;
            }
        }
    }

    fn clear_move_to(&mut self, d: f64, d2: f64) {
        self.current_x -= d;
        self.current_y -= d2;
        self.stack_count = 0;
        self.last_op_code = 0;
    }

    #[allow(clippy::too_many_arguments)]
    fn add_general_flex(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64, d7: f64, d8: f64, d9: f64, d10: f64, d11: f64, d12: f64, d13: f64) {
        let (cx, cy) = (self.current_x, self.current_y);
        self.push(d2 - cx);
        self.push(d3 - cy);
        self.push(d4 - d2);
        self.push(d5 - d3);
        self.push(d6 - d4);
        self.push(d7 - d5);
        self.push(d8 - d6);
        self.push(d9 - d7);
        self.push(d10 - d8);
        self.push(d11 - d9);
        self.push(d12 - d10);
        self.push(d13 - d11);
        self.push(d);
        self.last_op_code = 12;
        self.stack_to_cstring();
        self.last_op_code = 35;
        self.stack_to_cstring();
        self.current_x = d12;
        self.current_y = d13;
    }

    fn create_index_from_charstrings(&self) -> FontResult<CharStrings> {
        let mut n2: i32 = 1;
        let mut b = CFFByteArray::get_cff_byte_array_builder_instance();
        b.add_card16(self.glyphs.len() as i32);
        let g = |i: usize| self.glyphs[i].as_ref().expect("java.lang.NullPointerException: missing glyph");
        for i in 0..self.glyphs.len() {
            n2 += g(i).finalcstr.cstr_len;
        }
        let n3 = if n2 > 0xffffff {
            4
        } else if n2 > 65535 {
            3
        } else if n2 > 255 {
            2
        } else {
            1
        };
        b.add_card8(n3);
        n2 = 1;
        for i in 0..self.glyphs.len() {
            b.add_offset(n3, n2);
            n2 += g(i).finalcstr.cstr_len;
        }
        b.add_offset(n3, n2);
        for i in 0..self.glyphs.len() {
            let f = &g(i).finalcstr;
            b.add_bytes(&f.cstr, 0, f.cstr_len);
        }
        CharStrings::from_builder(b)
    }

    pub fn get_charstring_index(&mut self) -> FontResult<CharStrings> {
        for i in 0..self.glyphs.len() {
            let (dw, nw, s) = (&self.default_width, &self.nominal_width, self.subrize);
            self.glyphs[i].as_mut().expect("java.lang.NullPointerException: missing glyph").generate_full_charstring(dw, nw, s);
        }
        self.create_index_from_charstrings()
    }

    fn numsize(d: f64) -> i32 {
        let n = d as i32;
        if n as f64 == d {
            if (-107..=107).contains(&n) {
                return 1;
            }
            if (-1131..=1131).contains(&n) {
                return 2;
            }
            return 3;
        }
        5
    }

    fn analyze_widths(&mut self, n: i32) {
        let mut list: Vec<WidthFrequency> = Vec::new();
        let cmp = |a: &WidthFrequency, b: &WidthFrequency| (a.width - b.width) as i32;
        for g in self.glyphs.iter() {
            let g = g.as_ref().expect("java.lang.NullPointerException: missing glyph");
            if g.fd != n {
                continue;
            }
            let key = WidthFrequency { width: g.width, count: 0 };
            let n2 = java_binary_search(&list, &key, cmp);
            if n2 >= 0 {
                list[n2 as usize].count += 1;
                continue;
            }
            list.insert((-n2 - 1) as usize, WidthFrequency::new(g.width));
        }
        if !list.is_empty() {
            if list.len() == 1 {
                self.default_width[n as usize] = list[0].width;
            } else {
                let mut n2 = 0;
                let mut n4 = 0;
                let mut n5 = 0;
                for wf in &list {
                    if wf.width == 0.0 {
                        continue;
                    }
                    n2 += Self::numsize(wf.width - 0.0) * wf.count;
                }
                let mut n6 = n2;
                for n3 in 0..list.len() {
                    let d = list[n3].width + 107.0;
                    let mut n8 = 0;
                    for wf in &list {
                        n8 += Self::numsize(wf.width - d) * wf.count;
                    }
                    for (n7, wf) in list.iter().enumerate() {
                        let n9 = n8 - Self::numsize(wf.width - d) * wf.count;
                        if n9 >= n6 {
                            continue;
                        }
                        n6 = n9;
                        n4 = n3;
                        n5 = n7;
                    }
                }
                let d = list[n5].width;
                let d2 = list[n4].width + 107.0;
                let mut n10 = 0;
                if d != 0.0 {
                    n10 += Self::numsize(d) + Key::DEFAULT_WIDTH_X.op_code_length();
                }
                if d2 != 0.0 {
                    n10 += Self::numsize(d2) + Key::NOMINAL_WIDTH_X.op_code_length();
                }
                if n6 + n10 < n2 {
                    self.default_width[n as usize] = d;
                    self.nominal_width[n as usize] = d2;
                }
            }
        }
        self.widths_analyzed[n as usize] = true;
    }

    pub fn calculate_nominal_width(&mut self, n: i32) -> f64 {
        if !self.widths_analyzed[n as usize] {
            self.analyze_widths(n);
        }
        self.nominal_width[n as usize]
    }

    pub fn calculate_default_width(&mut self, n: i32) -> f64 {
        if !self.widths_analyzed[n as usize] {
            self.analyze_widths(n);
        }
        self.default_width[n as usize]
    }
}

impl OutlineConsumer for Type2CStringGenerator {
    fn set_matrix(&mut self, _m: &Matrix) {}

    fn moveto(&mut self, d: f64, d2: f64) {
        match self.last_op_code {
            0 => {}
            4 => {
                let s = self.stack[0];
                self.clear_move_to(0.0, s);
            }
            22 => {
                let s = self.stack[0];
                self.clear_move_to(s, 0.0);
            }
            21 => {
                let (s, s1) = (self.stack[0], self.stack[1]);
                self.clear_move_to(s, s1);
            }
            _ => self.stack_to_cstring(),
        }
        let d3 = d - self.current_x;
        let d4 = d2 - self.current_y;
        self.current_x = d;
        self.current_y = d2;
        if d3 == 0.0 {
            self.push(d4);
            self.last_op_code = 4;
        } else if d4 == 0.0 {
            self.push(d3);
            self.last_op_code = 22;
        } else {
            self.push(d3);
            self.push(d4);
            self.last_op_code = 21;
        }
        self.move_to_seen = true;
    }

    fn lineto(&mut self, d: f64, d2: f64) {
        if !self.move_to_seen {
            self.moveto(0.0, 0.0);
        }
        let d3 = d - self.current_x;
        let d4 = d2 - self.current_y;
        self.current_x = d;
        self.current_y = d2;
        if d3 == 0.0 {
            self.ensure_stack_space(1);
            match self.last_op_code {
                6 => {}
                0 => self.start_of_seq_op = 7,
                _ => {
                    self.stack_to_cstring();
                    self.start_of_seq_op = 7;
                }
            }
            self.push(d4);
            self.last_op_code = 7;
        } else if d4 == 0.0 {
            self.ensure_stack_space(1);
            match self.last_op_code {
                7 => {}
                0 => self.start_of_seq_op = 6,
                _ => {
                    self.stack_to_cstring();
                    self.start_of_seq_op = 6;
                }
            }
            self.push(d3);
            self.last_op_code = 6;
        } else {
            self.ensure_stack_space(2);
            match self.last_op_code {
                5 => {
                    self.push(d3);
                    self.push(d4);
                }
                8 => {
                    self.push(d3);
                    self.push(d4);
                    self.last_op_code = 24;
                    self.stack_to_cstring();
                }
                o => {
                    if o != 0 {
                        self.stack_to_cstring();
                    }
                    self.push(d3);
                    self.push(d4);
                    self.last_op_code = 5;
                }
            }
        }
    }

    fn curveto(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64) {
        if !self.move_to_seen {
            self.moveto(0.0, 0.0);
        }
        let d7 = d - self.current_x;
        let d8 = d3 - d;
        let d9 = d5 - d3;
        let d10 = d2 - self.current_y;
        let d11 = d4 - d2;
        let d12 = d6 - d4;
        self.current_x = d5;
        self.current_y = d6;
        if d7 == 0.0 {
            if d12 == 0.0 {
                self.add_vh_curve_to(d10, d8, d11, d9);
            } else if d9 == 0.0 {
                self.add_vv_curve_to(d10, d8, d11, d12);
            } else {
                self.add_vh_curve_to5(d10, d8, d11, d9, d12);
            }
        } else if d10 == 0.0 {
            if d9 == 0.0 {
                self.add_hv_curve_to(d7, d8, d11, d12);
            } else if d12 == 0.0 {
                self.add_hh_curve_to(d7, d8, d11, d9);
            } else {
                self.add_hv_curve_to5(d7, d8, d11, d9, d12);
            }
        } else if d9 == 0.0 {
            self.add_vv_curve_to5(d7, d10, d8, d11, d12);
        } else if d12 == 0.0 {
            self.add_hh_curve_to5(d7, d10, d8, d11, d9);
        } else {
            self.add_rr_curve_to(d7, d10, d8, d11, d9, d12);
        }
    }

    fn curveto_quad(&mut self, d: f64, d2: f64, d3: f64, d4: f64) {
        if !self.move_to_seen {
            self.moveto(0.0, 0.0);
        }
        let x1 = java_math_round((self.current_x + 2.0 * d) / 3.0) as f64;
        let y1 = java_math_round((self.current_y + 2.0 * d2) / 3.0) as f64;
        let x2 = java_math_round((2.0 * d + d3) / 3.0) as f64;
        let y2 = java_math_round((2.0 * d2 + d4) / 3.0) as f64;
        self.curveto(x1, y1, x2, y2, d3, d4);
    }

    fn endchar(&mut self) {
        match self.last_op_code {
            0 | 4 | 21 | 22 => {
                self.stack_count = 0;
                self.last_op_code = 0;
            }
            _ => self.stack_to_cstring(),
        }
        self.last_op_code = 14;
        self.stack_to_cstring();
        self.new_hint_mask();
        self.new_cntr_mask();
    }
}

impl HintedOutlineConsumer for Type2CStringGenerator {
    fn stem(&mut self, d: f64, d2: f64, bl: bool, bl2: bool, bl3: bool) {
        if !bl3 {
            self.check_for_new_hint_mask(bl);
        }
        let n = self.glyph().find_hint(d, d2, bl2, bl3);
        if bl3 {
            self.check_for_new_cntr_mask(bl);
            Self::add_id_to_mask(n, &mut self.cntrmask);
        } else if self.hint_mask_seen {
            Self::add_id_to_mask(n, &mut self.hintmask);
        } else {
            Self::add_id_to_mask(n, &mut self.glyph().init_mask);
        }
    }

    fn stem3(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64, bl: bool, bl2: bool) {
        self.check_for_new_hint_mask(bl);
        self.check_for_new_cntr_mask(false);
        self.cntr_hint(d, d2, bl2);
        self.cntr_hint(d3, d4, bl2);
        self.cntr_hint(d5, d6, bl2);
    }

    fn global_color_on(&mut self) {
        if self.last_op_code != 0 {
            self.stack_to_cstring();
        }
        self.glyph().tmpcstr.add_byte(12);
        self.glyph().tmpcstr.add_byte(38);
    }

    fn no_counters(&mut self) {
        self.new_cntr_mask();
    }

    fn no_hints(&mut self) {
        self.new_hint_mask();
    }

    fn width(&mut self, d: f64) -> bool {
        let i = self.curr_cstring_index as usize;
        let g = self.glyphs[i].as_mut().expect("java.lang.NullPointerException: no newGlyph");
        g.set_width(d, &self.nominal_width);
        true
    }

    fn closepath(&mut self) {}

    fn flex(&mut self, d: f64, d2: f64, d3: f64, d4: f64, d5: f64, d6: f64, d7: f64, d8: f64, d9: f64, d10: f64, d11: f64, d12: f64, d13: f64) {
        let mut bl = true;
        if self.last_op_code != 0 {
            self.stack_to_cstring();
        }
        if d != STANDARD_FLEX_DEPTH {
            self.add_general_flex(d, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13);
            return;
        }
        let (cx, cy) = (self.current_x, self.current_y);
        if cy == d13 {
            if d5 == d7 && d7 == d9 {
                if cy == d3 && d11 == d13 {
                    self.push(d2 - cx);
                    self.push(d4 - d2);
                    self.push(d5 - d3);
                    self.push(d6 - d4);
                    self.push(d8 - d6);
                    self.push(d10 - d8);
                    self.push(d12 - d10);
                    self.last_op_code = 12;
                    self.stack_to_cstring();
                    self.last_op_code = 34;
                    self.stack_to_cstring();
                } else {
                    self.push(d2 - cx);
                    self.push(d3 - cy);
                    self.push(d4 - d2);
                    self.push(d5 - d3);
                    self.push(d6 - d4);
                    self.push(d8 - d6);
                    self.push(d10 - d8);
                    self.push(d11 - d9);
                    self.push(d12 - d10);
                    self.last_op_code = 12;
                    self.stack_to_cstring();
                    self.last_op_code = 36;
                    self.stack_to_cstring();
                }
                self.current_x = d12;
                self.current_y = d13;
                return;
            }
        } else if cx == d12 {
            bl = false;
        } else {
            self.add_general_flex(d, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13);
            return;
        }
        if ((d10 - cx).abs() > (d11 - cy).abs()) != bl {
            self.add_general_flex(d, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13);
            return;
        }
        self.push(d2 - cx);
        self.push(d3 - cy);
        self.push(d4 - d2);
        self.push(d5 - d3);
        self.push(d6 - d4);
        self.push(d7 - d5);
        self.push(d8 - d6);
        self.push(d9 - d7);
        self.push(d10 - d8);
        self.push(d11 - d9);
        self.push(if bl { d12 - d10 } else { d13 - d11 });
        self.last_op_code = 12;
        self.stack_to_cstring();
        self.last_op_code = 37;
        self.stack_to_cstring();
        self.current_x = d12;
        self.current_y = d13;
    }
}

/// `Type2CStringGenerator.GlyphInfo`.
pub struct GlyphInfo {
    pub width: f64,
    pub fd: i32,
    hints: Vec<HintInfo>,
    hint_masks: Vec<HintMask>,
    cntr_masks: Vec<CntrMask>,
    init_mask: [u8; 12],
    pub tmpcstr: GrowableBuffer,
    pub finalcstr: GrowableBuffer,
}

impl GlyphInfo {
    fn new(n: i32, d: f64) -> GlyphInfo {
        GlyphInfo {
            width: d,
            fd: n,
            hints: Vec::new(),
            hint_masks: Vec::new(),
            cntr_masks: Vec::new(),
            init_mask: [0; 12],
            tmpcstr: GrowableBuffer::new(),
            finalcstr: GrowableBuffer::new(),
        }
    }

    fn find_hint(&mut self, d: f64, d2: f64, bl: bool, bl2: bool) -> i32 {
        let d5 = d2 - d;
        let (d4, d3) = if d5 < 0.0 && d5.total_cmp(&BOTTOM_EDGE).is_ne() && d5.total_cmp(&TOP_EDGE).is_ne() { (d2, d) } else { (d, d2) };
        let mut hi = HintInfo { top: d3, bottom: d4, flags: (if bl { 1 } else { 0 }) | (if bl2 { 2 } else { 0 }), id: 0 };
        let n = java_binary_search(&self.hints, &hi, HintInfo::compare);
        if n < 0 {
            let p = -n - 1;
            if !self.cntr_masks.is_empty() && !bl2 {
                if (p as usize) < self.hints.len() && hi.fuzzy_match(&self.hints[p as usize]) {
                    return self.hints[p as usize].id;
                }
                if p > 0 && hi.fuzzy_match(&self.hints[(p - 1) as usize]) {
                    return self.hints[(p - 1) as usize].id;
                }
            }
            hi.id = self.hints.len() as i32;
            let id = hi.id;
            if id < MAX_STEMS {
                self.hints.insert(p as usize, hi);
            }
            return id;
        }
        self.hints[n as usize].id
    }

    fn mask_byte_set(n: i32, a: &[u8]) -> bool {
        a[(n / 8) as usize] as i32 & (1 << (n % 8)) != 0
    }

    fn add_hint_to_mask(n: i32, a: &mut [u8]) {
        let n2 = (n / 8) as usize;
        a[n2] |= (1 << (7 - n % 8)) as u8;
    }

    fn write_stems(&mut self, n: i32, n2: i32, n3: i32, bl: bool) {
        let mut n4 = (n2 - n + 23 - 1) / 23;
        let mut n5 = n2 - n - (n4 - 1) * 23;
        let mut it = n as usize;
        while n4 > 0 {
            n4 -= 1;
            let mut d = 0.0;
            while n5 > 0 {
                n5 -= 1;
                let (bottom, top) = (self.hints[it].bottom, self.hints[it].top);
                it += 1;
                self.finalcstr.num_to_cstring(bottom - d);
                self.finalcstr.num_to_cstring(top - bottom);
                d = top;
            }
            if bl || n4 > 0 {
                self.finalcstr.add_byte(n3);
            }
            n5 = 23;
        }
    }

    fn save_start_of_cstring(&mut self, a: &[u8], d_arr: &[f64], d_arr2: &[f64], bl: bool) {
        let n2 = (self.hints.len() + 7) / 8;
        let mut a2 = vec![0u8; n2];
        let mut bl2 = false;
        let mut bl3 = false;
        let n3 = self.hints.len() as i32;
        if self.width != d_arr[self.fd as usize] {
            let v = self.width - d_arr2[self.fd as usize];
            self.finalcstr.num_to_cstring(v);
        }
        if n3 == 0 {
            return;
        }
        let mut n4 = 0;
        for h in &self.hints {
            if h.flags & 1 != 0 {
                break;
            }
            n4 += 1;
        }
        a2.fill(0xff);
        if n3 % 8 != 0 {
            let l = a2.len();
            a2[l - 1] = (255 >> (8 - n3 % 8)) as u8;
        }
        if !array_compare(&a2, 0, &self.init_mask, 0, a2.len() as i32) {
            bl2 = true;
            bl3 = true;
        } else if !self.hint_masks.is_empty() {
            if self.hint_masks[0].index_into_cstr == 0 {
                bl3 = true;
            }
            bl2 = true;
        }
        let mut object: Vec<u8> = if bl3 { self.init_mask.to_vec() } else { a2.clone() };
        let mut i = 0;
        while i < self.hint_masks.len() {
            let hm = &self.hint_masks[i];
            if hm.index_into_cstr != 0 && array_compare(&object, 0, &hm.mask, 0, object.len() as i32) {
                self.hint_masks.remove(i);
                continue;
            }
            object = hm.mask.to_vec();
            i += 1;
        }
        if self.hint_masks.is_empty() && !bl3 {
            bl2 = false;
        }
        self.write_stems(0, n4, if bl2 { 18 } else { 1 }, true);
        let last = !bl3 && self.cntr_masks.is_empty();
        self.write_stems(n4, n3, if bl2 { 23 } else { 3 }, last);
        for ci in 0..self.cntr_masks.len() {
            a2.fill(0);
            self.finalcstr.add_byte(20);
            for n in 0..n3 {
                if !GlyphInfo::mask_byte_set(n, &self.cntr_masks[ci].mask) {
                    continue;
                }
                GlyphInfo::add_hint_to_mask(a[n as usize] as i8 as i32, &mut a2);
            }
            if bl {
                self.finalcstr.add_byte(a2.len() as i32 + 2);
            }
            for &x in &a2 {
                self.finalcstr.add_byte(x as i8 as i32);
            }
        }
        if bl3 {
            a2.fill(0);
            self.finalcstr.add_byte(19);
            for n in 0..n3 {
                if !GlyphInfo::mask_byte_set(n, &self.init_mask) {
                    continue;
                }
                GlyphInfo::add_hint_to_mask(a[n as usize] as i8 as i32, &mut a2);
            }
            if bl {
                self.finalcstr.add_byte(a2.len() as i32 + 2);
            }
            for &x in &a2 {
                self.finalcstr.add_byte(x as i8 as i32);
            }
        }
    }

    fn save_paths_to_cstring(&mut self, a: &[u8], bl: bool) {
        let n = (self.hints.len() + 7) / 8;
        let mut n2 = 0;
        let mut a2 = vec![0u8; n];
        let n3 = self.hints.len() as i32;
        for hi in 0..self.hint_masks.len() {
            let n5 = self.hint_masks[hi].index_into_cstr;
            while n2 < n5 {
                let b = self.tmpcstr.cstr[n2 as usize];
                self.finalcstr.add_byte(b as i32);
                n2 += 1;
            }
            a2.fill(0);
            self.finalcstr.add_byte(19);
            for n4 in 0..n3 {
                if !GlyphInfo::mask_byte_set(n4, &self.hint_masks[hi].mask) {
                    continue;
                }
                GlyphInfo::add_hint_to_mask(a[n4 as usize] as i8 as i32, &mut a2);
            }
            if bl {
                self.finalcstr.add_byte(a2.len() as i32 + 2);
            }
            for &x in &a2 {
                self.finalcstr.add_byte(x as i32);
            }
        }
        while n2 < self.tmpcstr.cstr_len {
            let b = self.tmpcstr.cstr[n2 as usize];
            self.finalcstr.add_byte(b as i32);
            n2 += 1;
        }
    }

    fn generate_full_charstring(&mut self, d_arr: &[f64], d_arr2: &[f64], bl: bool) {
        let mut a = vec![0u8; self.hints.len()];
        let mut by: u8 = 0;
        for h in &self.hints {
            a[h.id as usize] = by;
            by = by.wrapping_add(1);
        }
        self.save_start_of_cstring(&a, d_arr, d_arr2, bl);
        self.save_paths_to_cstring(&a, bl);
    }

    fn set_width(&mut self, d: f64, d_arr: &[f64]) {
        self.width = d + d_arr[self.fd as usize];
    }
}

/// `Type2CStringGenerator.GrowableBuffer`.
pub struct GrowableBuffer {
    pub cstr: Vec<u8>,
    pub cstr_len: i32,
}

impl GrowableBuffer {
    fn new() -> GrowableBuffer {
        GrowableBuffer { cstr: vec![0; 200], cstr_len: 0 }
    }

    fn ensure_cstr_is_big_enough(&mut self, n: i32) {
        if self.cstr_len + n > self.cstr.len() as i32 {
            let new_len = self.cstr.len() + n.max(50) as usize;
            self.cstr.resize(new_len, 0);
        }
    }

    pub fn add_byte(&mut self, n: i32) {
        self.ensure_cstr_is_big_enough(1);
        self.cstr[self.cstr_len as usize] = (n & 0xff) as u8;
        self.cstr_len += 1;
    }

    fn int_to_cstring(&mut self, n: i32) {
        let mut s = n as i16 as i32;
        if (-107..=107).contains(&s) {
            self.add_byte(s + 139);
        } else if (108..=1131).contains(&s) {
            s = (s - 108) as i16 as i32;
            self.add_byte((s >> 8) + 247);
            self.add_byte(s);
        } else if (-1131..=-108).contains(&s) {
            s = (s + 108) as i16 as i32;
            self.add_byte((-s >> 8) + 251);
            self.add_byte(-s);
        } else {
            self.add_byte(28);
            self.add_byte(s >> 8);
            self.add_byte(s);
        }
    }

    pub fn num_to_cstring(&mut self, d: f64) {
        let n = d as i32;
        if n as f64 == d {
            self.int_to_cstring(n);
            return;
        }
        let n2 = (d * 65536.0 + if d < 0.0 { -0.5 } else { 0.5 }) as i32;
        self.add_byte(255);
        self.add_byte(n2 >> 24);
        self.add_byte(n2 >> 16);
        self.add_byte(n2 >> 8);
        self.add_byte(n2);
    }
}

struct CntrMask {
    mask: [u8; 12],
}

impl CntrMask {
    fn new(a: &[u8; 12]) -> CntrMask {
        CntrMask { mask: *a }
    }
}

struct HintMask {
    index_into_cstr: i32,
    mask: [u8; 12],
}

impl HintMask {
    fn new(n: i32, a: &[u8; 12]) -> HintMask {
        HintMask { index_into_cstr: n, mask: *a }
    }
}

#[derive(Clone, Copy)]
struct HintInfo {
    top: f64,
    bottom: f64,
    flags: i32,
    id: i32,
}

impl HintInfo {
    /// `HintComparator.compare`.
    fn compare(a: &HintInfo, b: &HintInfo) -> i32 {
        let n = (a.flags & 1) - (b.flags & 1);
        if n != 0 {
            return n;
        }
        if a.bottom < b.bottom {
            return -1;
        }
        if a.bottom > b.bottom {
            return 1;
        }
        if a.top < b.top {
            return -1;
        }
        if a.top > b.top {
            return 1;
        }
        0
    }

    fn fuzzy_match(&self, h: &HintInfo) -> bool {
        h.flags & 2 != 0 && self.flags & 1 == h.flags & 1 && (self.bottom - h.bottom).abs() < 2.0 && (self.top - h.top).abs() < 2.0
    }
}

#[derive(Clone, Copy)]
struct WidthFrequency {
    width: f64,
    count: i32,
}

impl WidthFrequency {
    fn new(d: f64) -> WidthFrequency {
        WidthFrequency { width: d, count: 1 }
    }
}

#[cfg(test)]
pub mod dump_tests {
    use super::*;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_subrize::CFFSubrize;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cid_keyed_font::CIDKeyedFont;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::dict::Dict;
    use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
    use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;

    /// Reader for the `CffHarness` dumps (Java `DataOutputStream`, big-endian).
    pub struct Rd<'a> {
        pub d: &'a [u8],
        pub p: usize,
    }

    impl<'a> Rd<'a> {
        pub fn u8(&mut self) -> u8 {
            self.p += 1;
            self.d[self.p - 1]
        }
        pub fn i(&mut self) -> i32 {
            self.p += 4;
            i32::from_be_bytes(self.d[self.p - 4..self.p].try_into().unwrap())
        }
        pub fn d(&mut self) -> f64 {
            self.p += 8;
            f64::from_bits(u64::from_be_bytes(self.d[self.p - 8..self.p].try_into().unwrap()))
        }
        pub fn b(&mut self) -> bool {
            self.u8() != 0
        }
        pub fn bytes(&mut self) -> Option<Vec<u8>> {
            let n = self.i();
            if n < 0 {
                return None;
            }
            self.p += n as usize;
            Some(self.d[self.p - n as usize..self.p].to_vec())
        }
        pub fn str(&mut self) -> Option<String> {
            let n = self.i();
            if n < 0 {
                return None;
            }
            let mut u = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let (a, b) = (self.u8(), self.u8());
                u.push(u16::from_be_bytes([a, b]));
            }
            Some(String::from_utf16_lossy(&u))
        }
        pub fn od(&mut self) -> Option<f64> {
            if self.b() {
                Some(self.d())
            } else {
                None
            }
        }
        pub fn oi(&mut self) -> Option<i32> {
            if self.b() {
                Some(self.i())
            } else {
                None
            }
        }
        pub fn ob(&mut self) -> Option<bool> {
            if self.b() {
                Some(self.b())
            } else {
                None
            }
        }
        pub fn oda(&mut self) -> Option<Vec<f64>> {
            if !self.b() {
                return None;
            }
            let n = self.i();
            Some((0..n).map(|_| self.d()).collect())
        }
        pub fn oia(&mut self) -> Option<Vec<i32>> {
            if !self.b() {
                return None;
            }
            let n = self.i();
            Some((0..n).map(|_| self.i()).collect())
        }
        pub fn ds<const N: usize>(&mut self) -> [f64; N] {
            let mut a = [0.0; N];
            for x in a.iter_mut() {
                *x = self.d();
            }
            a
        }
    }

    /// Replays one recorded consumer call into `c`.
    pub fn replay_op(r: &mut Rd, op: u8, c: &mut dyn HintedOutlineConsumer) {
        match op {
            2 => {
                let m = r.ds::<6>();
                c.set_matrix(&Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5]));
            }
            3 => {
                let a = r.ds::<2>();
                c.moveto(a[0], a[1]);
            }
            4 => {
                let a = r.ds::<2>();
                c.lineto(a[0], a[1]);
            }
            5 => {
                let a = r.ds::<6>();
                c.curveto(a[0], a[1], a[2], a[3], a[4], a[5]);
            }
            6 => {
                let a = r.ds::<4>();
                c.curveto_quad(a[0], a[1], a[2], a[3]);
            }
            7 => c.endchar(),
            8 => {
                let a = r.ds::<2>();
                let (x, y, z) = (r.b(), r.b(), r.b());
                c.stem(a[0], a[1], x, y, z);
            }
            9 => {
                let a = r.ds::<6>();
                let (x, y) = (r.b(), r.b());
                c.stem3(a[0], a[1], a[2], a[3], a[4], a[5], x, y);
            }
            10 => c.global_color_on(),
            11 => c.no_counters(),
            12 => c.no_hints(),
            13 => {
                let a = r.d();
                c.width(a);
            }
            14 => c.closepath(),
            15 => {
                let a = r.ds::<13>();
                c.flex(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8], a[9], a[10], a[11], a[12]);
            }
            _ => panic!("bad op {} at {}", op, r.p),
        }
    }

    pub fn dump_dir() -> Option<std::path::PathBuf> {
        let d = std::env::var("CFF_DUMP_DIR").unwrap_or_else(|_| "S:/Desktop/Projects/Rust-FFDec/target/fonts_ws/cff/dump".into());
        let p = std::path::PathBuf::from(d);
        if p.is_dir() {
            Some(p)
        } else {
            None
        }
    }

    pub fn dump_files() -> Vec<std::path::PathBuf> {
        let dir = match dump_dir() {
            Some(d) => d,
            None => return Vec::new(),
        };
        let filter = std::env::var("CFF_FILTER").unwrap_or_default();
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "bin") && p.file_name().unwrap().to_string_lossy().contains(&filter))
            .collect();
        files.sort();
        files
    }

    pub fn first_diff(a: &[u8], b: &[u8]) -> String {
        let i = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        format!("lengths {} vs {}, first difference at {}", a.len(), b.len(), i)
    }

    /// Replays a `G` record (Glyf.subsetAndStreamForCFF); returns the bytes compared.
    pub fn replay_glyf(r: &mut Rd, name: &str) -> usize {
        let num_glyphs = r.i();
        let bl3 = r.b();
        let mut g = Type2CStringGenerator::new(num_glyphs, 1, bl3);
        loop {
            let op = r.u8();
            match op {
                0 => break,
                1 => {
                    let (n, fd) = (r.i(), r.i());
                    let (d, d2) = (r.d(), r.d());
                    g.new_glyph(n, fd, d, d2);
                }
                _ => replay_op(r, op, &mut g),
            }
        }
        let (en, ed) = (r.d(), r.d());
        let d = g.calculate_nominal_width(0);
        let d2 = g.calculate_default_width(0);
        assert_eq!((d.to_bits(), d2.to_bits()), (en.to_bits(), ed.to_bits()), "{}: widths", name);
        let mut cs = g.get_charstring_index().unwrap();
        let exp = r.bytes().unwrap();
        let got = cs.data.base.get_bytes(cs.offset, cs.size());
        assert!(got == exp, "{}: charstrings {}", name, first_diff(&got, &exp));
        let mut total = got.len();
        let (mut l, mut gs) = (None, None);
        if bl3 {
            let mut s = CFFSubrize::new();
            cs = s.subrize(cs);
            l = s.get_l_subrs();
            gs = s.get_g_subrs();
            for (what, x) in [("main", Some(&cs)), ("lsubrs", l.as_ref()), ("gsubrs", gs.as_ref())] {
                let e = r.bytes();
                let got = x.map(|c| c.data.base.get_bytes(c.offset, c.size()));
                assert!(got == e, "{}: subrized {} {}", name, what, first_diff(got.as_deref().unwrap_or(&[]), e.as_deref().unwrap_or(&[])));
                total += got.map_or(0, |v| v.len());
            }
        }
        let (reg, ord, sup) = (r.str(), r.str(), r.i());
        let (ver, notice, copy, full, fname, family, weight) = (r.str(), r.str(), r.str(), r.str(), r.str(), r.str(), r.str());
        let (fixed, italic, upos, uthick, paint, uid) = (r.ob(), r.od(), r.od(), r.od(), r.oi(), r.oi());
        let bbox = if r.b() { Some(Rect::from_array(&r.ds::<4>())) } else { None };
        let (stroke, xuid, ps, fs, bfb, bfn) = (r.od(), r.oia(), r.str(), r.oi(), r.oia(), r.str());
        let fm = if r.b() { Some(Matrix::from_array(&r.ds::<6>())) } else { None };
        let top = Dict::new_top(reg, ord, sup, ver, notice, copy, full, fname, family, weight, fixed, italic, upos, uthick, paint, uid, bbox, stroke, xuid, ps, fs, bfb, bfn, fm).unwrap();
        let (a1, a2, a3, a4, bs, bsh, bf, hw, vw, sh, sv, fb, lg, ef, irs, nw, dw) =
            (r.oda(), r.oda(), r.oda(), r.oda(), r.od(), r.od(), r.oi(), r.od(), r.od(), r.oda(), r.oda(), r.ob(), r.oi(), r.od(), r.oi(), r.od(), r.od());
        let pd = Dict::new_private(a1, a2, a3, a4, bs, bsh, bf, hw, vw, sh, sv, fb, lg, ef, irs, nw, dw);
        let m = r.ds::<6>();
        let fd = Dict::new_font_matrix(Matrix::from_array(&m));
        let name2 = r.str().unwrap();
        let ck = CIDKeyedFont::new_synthetic(name2, top, cs, num_glyphs, fd, pd, l, gs).unwrap();
        let mut out = Vec::new();
        let mut sub = SubsetDefaultImpl::new(num_glyphs, false);
        ck.subset_and_stream(&mut sub, &mut out, false, None, false, false).unwrap();
        let exp = r.bytes().unwrap();
        assert!(out == exp, "{}: CFF {}", name, first_diff(&out, &exp));
        total + out.len()
    }

    /// Skips a `G` record.
    pub fn skip_glyf(r: &mut Rd) {
        r.i();
        let bl3 = r.b();
        loop {
            let op = r.u8();
            r.p += match op {
                0 => break,
                1 => 24,
                2 | 5 => 48,
                3 | 4 => 16,
                6 => 32,
                8 => 19,
                9 => 50,
                13 => 8,
                15 => 104,
                _ => 0,
            };
        }
        r.p += 16;
        r.bytes();
        if bl3 {
            for _ in 0..3 {
                r.bytes();
            }
        }
        r.str();
        r.str();
        r.i();
        for _ in 0..7 {
            r.str();
        }
        r.ob();
        r.od();
        r.od();
        r.od();
        r.oi();
        r.oi();
        if r.b() {
            r.p += 32;
        }
        r.od();
        r.oia();
        r.str();
        r.oi();
        r.oia();
        r.str();
        if r.b() {
            r.p += 48;
        }
        for _ in 0..4 {
            r.oda();
        }
        r.od();
        r.od();
        r.oi();
        r.od();
        r.od();
        r.oda();
        r.oda();
        r.ob();
        r.oi();
        r.od();
        r.oi();
        r.od();
        r.od();
        r.p += 48;
        r.str();
        r.bytes();
    }

    /// Skips an `R` record (a CFF afe wrote, read back).
    pub fn skip_raw(r: &mut Rd) {
        r.bytes();
        let n = r.i();
        for _ in 0..n {
            skip_outline(r);
        }
        let k = r.i();
        r.p += 4 * k as usize;
        for _ in 0..4 {
            r.bytes();
        }
    }

    /// Skips one recorded outline (plain consumer calls, then 0 or 255 and a message).
    pub fn skip_outline(r: &mut Rd) {
        loop {
            match r.u8() {
                0 => return,
                255 => {
                    r.str();
                    return;
                }
                2 | 5 => r.p += 48,
                3 | 4 => r.p += 16,
                6 => r.p += 32,
                7 => {}
                op => panic!("unexpected outline op {}", op),
            }
        }
    }

    #[test]
    fn glyf_to_cff_matches_java() {
        let (mut cases, mut bytes) = (0, 0);
        for p in dump_files() {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let data = std::fs::read(&p).unwrap();
            let mut r = Rd { d: &data, p: 4 };
            loop {
                match r.u8() {
                    b'K' => {
                        let c = r.str().unwrap();
                        let label = format!("{} {}", name, c);
                        match r.u8() {
                            b'G' => {
                                bytes += replay_glyf(&mut r, &label);
                                cases += 1;
                            }
                            b'C' => {
                                let n = r.i();
                                r.p += 4 * n as usize;
                                for _ in 0..r.i() {
                                    r.bytes();
                                }
                                r.bytes();
                            }
                            b'S' => {
                                r.bytes();
                            }
                            x => panic!("bad case kind {}", x),
                        }
                    }
                    b'O' => {
                        let n = r.i();
                        for _ in 0..n {
                            skip_outline(&mut r);
                        }
                    }
                    b'R' => skip_raw(&mut r),
                    b'E' => break,
                    x => panic!("bad record {} in {}", x, name),
                }
            }
        }
        eprintln!("glyf->CFF: {} cases, {} bytes matched", cases, bytes);
    }
}
