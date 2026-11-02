//! Port of FreeType 2.14.2 `src/truetype/ttinterp.c` (as built into JDK 17.0.19), in the v35 mode the JDK selects (`backward_compatibility` is always 0, GX `blend` is never set up).

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::*;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::FtSizeMetrics;
use crate::asc::flex2::fonts::java2d::freetype::ttobjs::{TtGlyphZone, TtGraphicsState, TtSizeMetrics};
use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;

pub const TT_ROUND_OFF: i32 = 5;
pub const TT_ROUND_TO_HALF_GRID: i32 = 0;
pub const TT_ROUND_TO_GRID: i32 = 1;
pub const TT_ROUND_TO_DOUBLE_GRID: i32 = 2;
pub const TT_ROUND_UP_TO_GRID: i32 = 4;
pub const TT_ROUND_DOWN_TO_GRID: i32 = 3;
pub const TT_ROUND_SUPER: i32 = 6;
pub const TT_ROUND_SUPER_45: i32 = 7;

pub const TT_CODERANGE_NONE: i32 = 0;
pub const TT_CODERANGE_FONT: i32 = 1;
pub const TT_CODERANGE_CVT: i32 = 2;
pub const TT_CODERANGE_GLYPH: i32 = 3;

pub const TT_INTERPRETER_VERSION_35: i32 = 35;

const TT_CONFIG_OPTION_MAX_RUNNABLE_OPCODES: u32 = 1_000_000;

pub const TT_DEFAULT_GRAPHICS_STATE: TtGraphicsState = TtGraphicsState {
    rp0: 0,
    rp1: 0,
    rp2: 0,
    gep0: 1,
    gep1: 1,
    gep2: 1,
    dual_vector: FtUnitVector { x: 0x4000, y: 0 },
    proj_vector: FtUnitVector { x: 0x4000, y: 0 },
    free_vector: FtUnitVector { x: 0x4000, y: 0 },
    loop_: 1,
    round_state: 1,
    compensation: [0, 0, 0, 0],
    minimum_distance: 64,
    control_value_cutin: 68,
    single_width_cutin: 0,
    single_width_value: 0,
    delta_base: 9,
    delta_shift: 3,
    auto_flip: true,
    instruct_control: 0,
    scan_control: false,
    scan_type: 0,
};

macro_rules! pack {
    ($x:expr, $y:expr) => {
        (($x << 4) | $y) as u8
    };
}

static POP_PUSH_COUNT: [u8; 256] = [
    // 0x00
    pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(2, 0), pack!(2, 0),
    pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(0, 2), pack!(0, 2), pack!(0, 0), pack!(5, 0),
    // 0x10
    pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    pack!(0, 0), pack!(0, 0), pack!(1, 0), pack!(0, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    // 0x20
    pack!(1, 2), pack!(1, 0), pack!(0, 0), pack!(2, 2), pack!(0, 1), pack!(1, 1), pack!(1, 0), pack!(2, 0),
    pack!(0, 0), pack!(1, 0), pack!(2, 0), pack!(1, 0), pack!(1, 0), pack!(0, 0), pack!(1, 0), pack!(1, 0),
    // 0x30
    pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    pack!(1, 0), pack!(0, 0), pack!(2, 0), pack!(2, 0), pack!(0, 0), pack!(0, 0), pack!(2, 0), pack!(2, 0),
    // 0x40
    pack!(0, 0), pack!(0, 0), pack!(2, 0), pack!(1, 1), pack!(2, 0), pack!(1, 1), pack!(1, 1), pack!(1, 1),
    pack!(2, 0), pack!(2, 1), pack!(2, 1), pack!(0, 1), pack!(0, 1), pack!(0, 0), pack!(0, 0), pack!(1, 0),
    // 0x50
    pack!(2, 1), pack!(2, 1), pack!(2, 1), pack!(2, 1), pack!(2, 1), pack!(2, 1), pack!(1, 1), pack!(1, 1),
    pack!(1, 0), pack!(0, 0), pack!(2, 1), pack!(2, 1), pack!(1, 1), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    // 0x60
    pack!(2, 1), pack!(2, 1), pack!(2, 1), pack!(2, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1),
    pack!(1, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1), pack!(1, 1),
    // 0x70
    pack!(2, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    pack!(2, 0), pack!(2, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(1, 0), pack!(1, 0),
    // 0x80
    pack!(0, 0), pack!(2, 0), pack!(2, 0), pack!(0, 0), pack!(0, 0), pack!(1, 0), pack!(2, 0), pack!(2, 0),
    pack!(1, 1), pack!(1, 0), pack!(3, 3), pack!(2, 1), pack!(2, 1), pack!(1, 0), pack!(2, 0), pack!(0, 0),
    // 0x90
    pack!(0, 0), pack!(0, 0), pack!(0, 1), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0),
    pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0),
    // 0xA0
    pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0),
    pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0), pack!(0, 0),
    // 0xB0
    pack!(0, 1), pack!(0, 2), pack!(0, 3), pack!(0, 4), pack!(0, 5), pack!(0, 6), pack!(0, 7), pack!(0, 8),
    pack!(0, 1), pack!(0, 2), pack!(0, 3), pack!(0, 4), pack!(0, 5), pack!(0, 6), pack!(0, 7), pack!(0, 8),
    // 0xC0
    pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    // 0xD0
    pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0), pack!(1, 0),
    // 0xE0
    pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0),
    pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0),
    // 0xF0
    pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0),
    pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0), pack!(2, 0),
];

static OPCODE_LENGTH: [i8; 256] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    -1, -2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    2, 3, 4, 5, 6, 7, 8, 9, 3, 5, 7, 9, 11, 13, 15, 17, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, //
];

#[inline]
fn tt_mul_fix14(a: FtF26Dot6, b: i32) -> FtF26Dot6 {
    let mut ab = (a as i64).wrapping_mul(b as i64);
    ab = ab.wrapping_add(0x2000 + (ab >> 63));
    (ab >> 14) as FtF26Dot6
}

#[inline]
fn tt_dot_fix14(ax: FtF26Dot6, ay: FtF26Dot6, bx: FtF2Dot14, by: FtF2Dot14) -> FtF26Dot6 {
    let mut c = (ax as i64).wrapping_mul(bx as i64).wrapping_add((ay as i64).wrapping_mul(by as i64));
    c = c.wrapping_add(0x2000 + (c >> 63));
    (c >> 14) as FtF26Dot6
}

#[inline]
fn bounds(x: u32, n: u32) -> bool {
    x >= n
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ZoneId {
    Twilight,
    #[default]
    Pts,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtDefRecord {
    pub range: i32,
    pub start: i32,
    pub end: i32,
    pub opc: u32,
    pub active: bool,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtCallRec {
    pub caller_range: i32,
    pub caller_ip: i32,
    pub cur_count: i32,
    pub def_is_idef: bool,
    pub def_index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum RoundFunc {
    None,
    #[default]
    ToGrid,
    ToHalfGrid,
    ToDoubleGrid,
    DownToGrid,
    UpToGrid,
    Super,
    Super45,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ProjFunc {
    #[default]
    X,
    Y,
    Proj,
    Dual,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum MoveFunc {
    #[default]
    X,
    Y,
    Any,
}

#[derive(Clone, Default)]
pub struct TtExecContext {
    pub num_glyphs: i32,
    pub interpreter_version: i32,
    pub error: FtError,
    pub top: i32,
    pub stack_size: i32,
    pub stack: Vec<i32>,
    pub args: i32,
    pub new_top: i32,
    pub zp0: ZoneId,
    pub zp1: ZoneId,
    pub zp2: ZoneId,
    pub pts: TtGlyphZone,
    pub twilight: TtGlyphZone,
    pub point_size: i32,
    pub metrics: FtSizeMetrics,
    pub tt_metrics: TtSizeMetrics,
    pub gs: TtGraphicsState,
    pub ini_range: i32,
    pub cur_range: i32,
    pub code: Rc<Vec<u8>>,
    pub ip: i32,
    pub code_size: i32,
    pub opcode: u8,
    pub length: i32,
    pub cvt_size: u32,
    pub cvt_base: Vec<i32>,
    pub glyf_cvt: Vec<i32>,
    pub cvt_is_glyf: bool,
    pub glyph_size: u32,
    pub glyph_ins: Rc<Vec<u8>>,
    pub num_fdefs: u32,
    pub max_fdefs: u32,
    pub fdefs: Vec<TtDefRecord>,
    pub num_idefs: u32,
    pub max_idefs: u32,
    pub idefs: Vec<TtDefRecord>,
    pub max_func: u32,
    pub max_ins: u32,
    pub call_top: i32,
    pub call_size: i32,
    pub call_stack: Vec<TtCallRec>,
    pub code_range_table: [(Option<Rc<Vec<u8>>>, i32); 3],
    pub store_size: u16,
    pub storage_base: Vec<i32>,
    pub glyf_storage: Vec<i32>,
    pub storage_is_glyf: bool,
    pub period: FtF26Dot6,
    pub phase: FtF26Dot6,
    pub threshold: FtF26Dot6,
    pub instruction_trap: bool,
    pub is_composite: bool,
    pub pedantic_hinting: bool,
    func_round: RoundFunc,
    pub move_vector: FtVector,
    func_project: ProjFunc,
    func_dualproj: ProjFunc,
    func_move: MoveFunc,
    stretched: bool,
    pub grayscale: bool,
    pub backward_compatibility: i32,
    pub mode: i32,
    pub loopcall_counter: u32,
    pub loopcall_counter_max: u32,
    pub neg_jump_counter: u32,
    pub neg_jump_counter_max: u32,
}

impl TtExecContext {
    pub fn new() -> TtExecContext {
        let mut e = TtExecContext::default();
        e.interpreter_version = TT_INTERPRETER_VERSION_35;
        e.call_size = 32;
        e.call_stack = vec![TtCallRec::default(); 32];
        e
    }

    pub fn set_code_range(&mut self, range: i32, base: Rc<Vec<u8>>, length: i32) {
        self.code_range_table[(range - 1) as usize] = (Some(base.clone()), length);
        self.code = base;
        self.code_size = length;
        self.ip = 0;
        self.cur_range = range;
        self.ini_range = range;
    }

    pub fn clear_code_range(&mut self, range: i32) {
        self.code_range_table[(range - 1) as usize] = (None, 0);
    }

    /// `TT_Load_Context`: the size's point size, metrics and the full twilight zone.
    pub fn load_context(&mut self, point_size: i32, tt_metrics: TtSizeMetrics, metrics: FtSizeMetrics, twilight_n_points: u16) {
        self.cvt_is_glyf = false;
        self.storage_is_glyf = false;
        self.glyph_ins = Rc::new(Vec::new());
        self.glyph_size = 0;
        self.point_size = point_size;
        self.tt_metrics = tt_metrics;
        self.metrics = metrics;
        self.twilight.n_points = twilight_n_points;
    }

    pub fn save_context(&self, gs: &mut TtGraphicsState) {
        gs.minimum_distance = self.gs.minimum_distance;
        gs.control_value_cutin = self.gs.control_value_cutin;
        gs.single_width_cutin = self.gs.single_width_cutin;
        gs.single_width_value = self.gs.single_width_value;
        gs.delta_base = self.gs.delta_base;
        gs.delta_shift = self.gs.delta_shift;
        gs.auto_flip = self.gs.auto_flip;
        gs.instruct_control = self.gs.instruct_control;
        gs.scan_control = self.gs.scan_control;
        gs.scan_type = self.gs.scan_type;
    }

    #[inline]
    fn zone(&self, z: ZoneId) -> &TtGlyphZone {
        match z {
            ZoneId::Twilight => &self.twilight,
            ZoneId::Pts => &self.pts,
        }
    }

    #[inline]
    fn zone_mut(&mut self, z: ZoneId) -> &mut TtGlyphZone {
        match z {
            ZoneId::Twilight => &mut self.twilight,
            ZoneId::Pts => &mut self.pts,
        }
    }

    #[inline]
    fn zn(&self, z: ZoneId) -> u32 {
        self.zone(z).n_points as u32
    }

    #[inline]
    fn cur(&self, z: ZoneId, i: usize) -> FtVector {
        self.zone(z).cur[i]
    }

    #[inline]
    fn org(&self, z: ZoneId, i: usize) -> FtVector {
        self.zone(z).org[i]
    }

    #[inline]
    fn orus(&self, z: ZoneId, i: usize) -> FtVector {
        self.zone(z).orus[i]
    }

    #[inline]
    fn cvt(&self) -> &Vec<i32> {
        if self.cvt_is_glyf {
            &self.glyf_cvt
        } else {
            &self.cvt_base
        }
    }

    #[inline]
    fn cvt_mut(&mut self) -> &mut Vec<i32> {
        if self.cvt_is_glyf {
            &mut self.glyf_cvt
        } else {
            &mut self.cvt_base
        }
    }

    #[inline]
    fn storage(&self) -> &Vec<i32> {
        if self.storage_is_glyf {
            &self.glyf_storage
        } else {
            &self.storage_base
        }
    }

    fn current_ratio(&mut self) -> i32 {
        if self.tt_metrics.ratio == 0 {
            if self.gs.proj_vector.y == 0 {
                self.tt_metrics.ratio = self.tt_metrics.x_ratio;
            } else if self.gs.proj_vector.x == 0 {
                self.tt_metrics.ratio = self.tt_metrics.y_ratio;
            } else {
                let x = tt_mul_fix14(self.tt_metrics.x_ratio, self.gs.proj_vector.x as i32);
                let y = tt_mul_fix14(self.tt_metrics.y_ratio, self.gs.proj_vector.y as i32);
                self.tt_metrics.ratio = ft_hypot(x, y);
            }
        }
        self.tt_metrics.ratio
    }

    fn func_cur_ppem(&mut self) -> i32 {
        if self.stretched {
            let r = self.current_ratio();
            ft_mul_fix(self.tt_metrics.ppem as i32, r)
        } else {
            self.tt_metrics.ppem as i32
        }
    }

    fn func_read_cvt(&mut self, idx: usize) -> i32 {
        if self.stretched {
            let r = self.current_ratio();
            ft_mul_fix(self.cvt()[idx], r)
        } else {
            self.cvt()[idx]
        }
    }

    fn modify_cvt_check(&mut self) {
        if self.ini_range == TT_CODERANGE_GLYPH && !self.cvt_is_glyf {
            let mut g = self.cvt_base.clone();
            g.resize(self.cvt_size as usize, 0);
            self.glyf_cvt = g;
            self.cvt_is_glyf = true;
        }
    }

    fn func_write_cvt(&mut self, idx: usize, value: i32) {
        self.modify_cvt_check();
        if self.error != 0 {
            return;
        }
        if self.stretched {
            let r = self.current_ratio();
            self.cvt_mut()[idx] = ft_div_fix(value, r);
        } else {
            self.cvt_mut()[idx] = value;
        }
    }

    fn func_move_cvt(&mut self, idx: usize, value: i32) {
        self.modify_cvt_check();
        if self.error != 0 {
            return;
        }
        if self.stretched {
            let r = self.current_ratio();
            let v = ft_div_fix(value, r);
            let c = &mut self.cvt_mut()[idx];
            *c = add_long(*c, v);
        } else {
            let c = &mut self.cvt_mut()[idx];
            *c = add_long(*c, value);
        }
    }

    fn ins_goto_code_range(&mut self, a_range: i32, a_ip: i32) -> bool {
        if !(1..=3).contains(&a_range) {
            self.error = FT_ERR_BAD_ARGUMENT;
            return false;
        }
        let (base, size) = self.code_range_table[(a_range - 1) as usize].clone();
        let Some(base) = base else {
            self.error = FT_ERR_INVALID_CODERANGE;
            return false;
        };
        if a_ip > size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return false;
        }
        self.code = base;
        self.code_size = size;
        self.ip = a_ip;
        self.length = 0;
        self.cur_range = a_range;
        true
    }

    fn func_move_z(&mut self, z: ZoneId, point: usize, distance: FtF26Dot6) {
        match self.func_move {
            MoveFunc::X => {
                let zone = self.zone_mut(z);
                zone.cur[point].x = add_long(zone.cur[point].x, distance);
                zone.tags[point] |= FT_CURVE_TAG_TOUCH_X;
            }
            MoveFunc::Y => {
                let zone = self.zone_mut(z);
                zone.cur[point].y = add_long(zone.cur[point].y, distance);
                zone.tags[point] |= FT_CURVE_TAG_TOUCH_Y;
            }
            MoveFunc::Any => {
                let mv = self.move_vector;
                let zone = self.zone_mut(z);
                if mv.x != 0 {
                    zone.cur[point].x = add_long(zone.cur[point].x, ft_mul_fix(distance, mv.x));
                    zone.tags[point] |= FT_CURVE_TAG_TOUCH_X;
                }
                if mv.y != 0 {
                    zone.cur[point].y = add_long(zone.cur[point].y, ft_mul_fix(distance, mv.y));
                    zone.tags[point] |= FT_CURVE_TAG_TOUCH_Y;
                }
            }
        }
    }

    fn func_move_orig_z(&mut self, z: ZoneId, point: usize, distance: FtF26Dot6) {
        match self.func_move {
            MoveFunc::X => {
                let zone = self.zone_mut(z);
                zone.org[point].x = add_long(zone.org[point].x, distance);
            }
            MoveFunc::Y => {
                let zone = self.zone_mut(z);
                zone.org[point].y = add_long(zone.org[point].y, distance);
            }
            MoveFunc::Any => {
                let mv = self.move_vector;
                let zone = self.zone_mut(z);
                if mv.x != 0 {
                    zone.org[point].x = add_long(zone.org[point].x, ft_mul_fix(distance, mv.x));
                }
                if mv.y != 0 {
                    zone.org[point].y = add_long(zone.org[point].y, ft_mul_fix(distance, mv.y));
                }
            }
        }
    }

    fn round_none(distance: FtF26Dot6, compensation: FtF26Dot6) -> FtF26Dot6 {
        if distance >= 0 {
            let val = add_long(distance, compensation);
            if val < 0 {
                0
            } else {
                val
            }
        } else {
            let val = sub_long(distance, compensation);
            if val > 0 {
                0
            } else {
                val
            }
        }
    }

    fn round(&self, f: RoundFunc, distance: FtF26Dot6, compensation: FtF26Dot6) -> FtF26Dot6 {
        match f {
            RoundFunc::None => Self::round_none(distance, compensation),
            RoundFunc::ToGrid => {
                if distance >= 0 {
                    let val = ft_pix_round_long(add_long(distance, compensation));
                    if val < 0 {
                        0
                    } else {
                        val
                    }
                } else {
                    let val = neg_long(ft_pix_round_long(sub_long(compensation, distance)));
                    if val > 0 {
                        0
                    } else {
                        val
                    }
                }
            }
            RoundFunc::ToHalfGrid => {
                if distance >= 0 {
                    let val = add_long(ft_pix_floor(add_long(distance, compensation)), 32);
                    if val < 0 {
                        32
                    } else {
                        val
                    }
                } else {
                    let val = neg_long(add_long(ft_pix_floor(sub_long(compensation, distance)), 32));
                    if val > 0 {
                        -32
                    } else {
                        val
                    }
                }
            }
            RoundFunc::DownToGrid => {
                if distance >= 0 {
                    let val = ft_pix_floor(add_long(distance, compensation));
                    if val < 0 {
                        0
                    } else {
                        val
                    }
                } else {
                    let val = neg_long(ft_pix_floor(sub_long(compensation, distance)));
                    if val > 0 {
                        0
                    } else {
                        val
                    }
                }
            }
            RoundFunc::UpToGrid => {
                if distance >= 0 {
                    let val = ft_pix_ceil_long(add_long(distance, compensation));
                    if val < 0 {
                        0
                    } else {
                        val
                    }
                } else {
                    let val = neg_long(ft_pix_ceil_long(sub_long(compensation, distance)));
                    if val > 0 {
                        0
                    } else {
                        val
                    }
                }
            }
            RoundFunc::ToDoubleGrid => {
                if distance >= 0 {
                    let val = ft_pad_round_long(add_long(distance, compensation), 32);
                    if val < 0 {
                        0
                    } else {
                        val
                    }
                } else {
                    let val = neg_long(ft_pad_round_long(sub_long(compensation, distance), 32));
                    if val > 0 {
                        0
                    } else {
                        val
                    }
                }
            }
            RoundFunc::Super => {
                if distance >= 0 {
                    let mut val = add_long(distance, self.threshold.wrapping_sub(self.phase).wrapping_add(compensation)) & self.period.wrapping_neg();
                    val = add_long(val, self.phase);
                    if val < 0 {
                        self.phase
                    } else {
                        val
                    }
                } else {
                    let mut val = neg_long(
                        sub_long(self.threshold.wrapping_sub(self.phase).wrapping_add(compensation), distance) & self.period.wrapping_neg(),
                    );
                    val = sub_long(val, self.phase);
                    if val > 0 {
                        self.phase.wrapping_neg()
                    } else {
                        val
                    }
                }
            }
            RoundFunc::Super45 => {
                if distance >= 0 {
                    let mut val = c_div(add_long(distance, self.threshold.wrapping_sub(self.phase).wrapping_add(compensation)), self.period)
                        .wrapping_mul(self.period);
                    val = add_long(val, self.phase);
                    if val < 0 {
                        self.phase
                    } else {
                        val
                    }
                } else {
                    let mut val = neg_long(
                        c_div(sub_long(self.threshold.wrapping_sub(self.phase).wrapping_add(compensation), distance), self.period)
                            .wrapping_mul(self.period),
                    );
                    val = sub_long(val, self.phase);
                    if val > 0 {
                        self.phase.wrapping_neg()
                    } else {
                        val
                    }
                }
            }
        }
    }

    #[inline]
    fn func_round(&self, distance: FtF26Dot6, compensation: FtF26Dot6) -> FtF26Dot6 {
        self.round(self.func_round, distance, compensation)
    }

    fn set_super_round(&mut self, grid_period: i32, selector: i32) {
        match selector & 0xC0 {
            0 => self.period = grid_period / 2,
            0x40 => self.period = grid_period,
            0x80 => self.period = grid_period * 2,
            _ => self.period = grid_period,
        }
        match selector & 0x30 {
            0 => self.phase = 0,
            0x10 => self.phase = self.period / 4,
            0x20 => self.phase = self.period / 2,
            _ => self.phase = self.period * 3 / 4,
        }
        if (selector & 0x0F) == 0 {
            self.threshold = self.period - 1;
        } else {
            self.threshold = ((selector & 0x0F) - 4) * self.period / 8;
        }
        self.period >>= 8;
        self.phase >>= 8;
        self.threshold >>= 8;
    }

    #[inline]
    fn project_with(&self, f: ProjFunc, dx: FtPos, dy: FtPos) -> FtF26Dot6 {
        match f {
            ProjFunc::X => dx,
            ProjFunc::Y => dy,
            ProjFunc::Proj => tt_dot_fix14(dx, dy, self.gs.proj_vector.x, self.gs.proj_vector.y),
            ProjFunc::Dual => tt_dot_fix14(dx, dy, self.gs.dual_vector.x, self.gs.dual_vector.y),
        }
    }

    #[inline]
    fn project(&self, v1: FtVector, v2: FtVector) -> FtF26Dot6 {
        self.project_with(self.func_project, sub_long(v1.x, v2.x), sub_long(v1.y, v2.y))
    }

    #[inline]
    fn dualproj(&self, v1: FtVector, v2: FtVector) -> FtF26Dot6 {
        self.project_with(self.func_dualproj, sub_long(v1.x, v2.x), sub_long(v1.y, v2.y))
    }

    #[inline]
    fn fast_project(&self, v: FtVector) -> FtF26Dot6 {
        self.project_with(self.func_project, v.x, v.y)
    }

    #[inline]
    fn fast_dualproj(&self, v: FtVector) -> FtF26Dot6 {
        self.project_with(self.func_dualproj, v.x, v.y)
    }

    fn compute_funcs(&mut self) {
        let pv = self.gs.proj_vector;
        let fv = self.gs.free_vector;
        let f_dot_p = ((pv.x as i32) * (fv.x as i32) + (pv.y as i32) * (fv.y as i32) + 0x2000) >> 14;
        if f_dot_p >= 0x3FFE {
            self.move_vector.x = fv.x as i32 * 4;
            self.move_vector.y = fv.y as i32 * 4;
        } else if -0x400 < f_dot_p && f_dot_p < 0x400 {
            self.move_vector.x = 0;
            self.move_vector.y = 0;
        } else {
            self.move_vector.x = (fv.x as i32 * 0x10000) / f_dot_p;
            self.move_vector.y = (fv.y as i32 * 0x10000) / f_dot_p;
        }
        if f_dot_p >= 0x3FFE && fv.x == 0x4000 {
            self.func_move = MoveFunc::X;
        } else if f_dot_p >= 0x3FFE && fv.y == 0x4000 {
            self.func_move = MoveFunc::Y;
        } else {
            self.func_move = MoveFunc::Any;
        }
        self.func_project = if pv.x == 0x4000 {
            ProjFunc::X
        } else if pv.y == 0x4000 {
            ProjFunc::Y
        } else {
            ProjFunc::Proj
        };
        let dv = self.gs.dual_vector;
        self.func_dualproj = if dv.x == 0x4000 {
            ProjFunc::X
        } else if dv.y == 0x4000 {
            ProjFunc::Y
        } else {
            ProjFunc::Dual
        };
        self.tt_metrics.ratio = 0;
    }

    fn normalize(vx: FtF26Dot6, vy: FtF26Dot6, r: &mut FtUnitVector) {
        if vx == 0 && vy == 0 {
            return;
        }
        let mut v = FtVector { x: vx, y: vy };
        ft_vector_norm_len(&mut v);
        r.x = (v.x / 4) as FtF2Dot14;
        r.y = (v.y / 4) as FtF2Dot14;
    }

    #[inline]
    fn arg(&self, a: usize, k: usize) -> i32 {
        self.stack[a + k]
    }

    #[inline]
    fn set_arg(&mut self, a: usize, k: usize, v: i32) {
        self.stack[a + k] = v;
    }

    fn skip_code(&mut self) -> bool {
        self.ip += self.length;
        if self.ip < self.code_size {
            self.opcode = self.code[self.ip as usize];
            self.length = OPCODE_LENGTH[self.opcode as usize] as i32;
            if self.length < 0 {
                if self.ip + 1 >= self.code_size {
                    self.error = FT_ERR_CODE_OVERFLOW;
                    return false;
                }
                self.length = 2 - self.length * self.code[self.ip as usize + 1] as i32;
            }
            return true;
        }
        self.error = FT_ERR_CODE_OVERFLOW;
        false
    }

    fn ins_if(&mut self, a: usize) {
        if self.arg(a, 0) != 0 {
            return;
        }
        let mut n_ifs = 1;
        let mut out = false;
        loop {
            if !self.skip_code() {
                return;
            }
            match self.opcode {
                0x58 => n_ifs += 1,
                0x1B => out = n_ifs == 1,
                0x59 => {
                    n_ifs -= 1;
                    out = n_ifs == 0;
                }
                _ => {}
            }
            if out {
                break;
            }
        }
    }

    fn ins_else(&mut self) {
        let mut n_ifs = 1;
        loop {
            if !self.skip_code() {
                return;
            }
            match self.opcode {
                0x58 => n_ifs += 1,
                0x59 => n_ifs -= 1,
                _ => {}
            }
            if n_ifs == 0 {
                break;
            }
        }
    }

    fn ins_jmpr(&mut self, a: usize) {
        let a0 = self.arg(a, 0);
        if a0 == 0 && self.args == 0 {
            self.error = FT_ERR_BAD_ARGUMENT;
            return;
        }
        self.ip = add_long(self.ip, a0);
        if self.ip < 0 || (self.call_top > 0 && self.ip > self.def_of(self.call_stack[(self.call_top - 1) as usize]).end) {
            self.error = FT_ERR_BAD_ARGUMENT;
            return;
        }
        self.length = 0;
        if a0 < 0 {
            self.neg_jump_counter += 1;
            if self.neg_jump_counter > self.neg_jump_counter_max {
                self.error = FT_ERR_EXECUTION_TOO_LONG;
            }
        }
    }

    fn def_of(&self, c: TtCallRec) -> TtDefRecord {
        if c.def_is_idef {
            self.idefs[c.def_index]
        } else {
            self.fdefs[c.def_index]
        }
    }

    fn ins_fdef(&mut self, a: usize) {
        if self.ini_range == TT_CODERANGE_GLYPH {
            self.error = FT_ERR_DEF_IN_GLYF_BYTECODE;
            return;
        }
        let n = self.arg(a, 0) as u32;
        let mut rec = 0usize;
        let limit = self.num_fdefs as usize;
        while rec < limit {
            if self.fdefs[rec].opc == n {
                break;
            }
            rec += 1;
        }
        if rec == limit {
            if self.num_fdefs >= self.max_fdefs {
                self.error = FT_ERR_TOO_MANY_FUNCTION_DEFS;
                return;
            }
            self.num_fdefs += 1;
        }
        if n > 0xFFFF {
            self.error = FT_ERR_TOO_MANY_FUNCTION_DEFS;
            return;
        }
        self.fdefs[rec].range = self.cur_range;
        self.fdefs[rec].opc = n as u16 as u32;
        self.fdefs[rec].start = self.ip + 1;
        self.fdefs[rec].active = true;
        if n > self.max_func {
            self.max_func = n as u16 as u32;
        }
        while self.skip_code() {
            match self.opcode {
                0x89 | 0x2C => {
                    self.error = FT_ERR_NESTED_DEFS;
                    return;
                }
                0x2D => {
                    self.fdefs[rec].end = self.ip;
                    return;
                }
                _ => {}
            }
        }
    }

    fn ins_endf(&mut self) {
        if self.call_top <= 0 {
            self.error = FT_ERR_ENDF_IN_EXEC_STREAM;
            return;
        }
        self.call_top -= 1;
        let idx = self.call_top as usize;
        self.call_stack[idx].cur_count -= 1;
        let rec = self.call_stack[idx];
        if rec.cur_count > 0 {
            self.call_top += 1;
            self.ip = self.def_of(rec).start;
            self.length = 0;
        } else {
            self.ins_goto_code_range(rec.caller_range, rec.caller_ip);
        }
    }

    fn find_fdef(&self, f: u32) -> Option<usize> {
        if bounds(f, self.max_func.wrapping_add(1)) {
            return None;
        }
        let mut def = f as usize;
        if self.max_func + 1 != self.num_fdefs || self.fdefs[def].opc != f {
            def = 0;
            let limit = self.num_fdefs as usize;
            while def < limit && self.fdefs[def].opc != f {
                def += 1;
            }
            if def == limit {
                return None;
            }
        }
        Some(def)
    }

    fn ins_call(&mut self, a: usize) {
        let f = self.arg(a, 0) as u32;
        if self.fdefs.is_empty() && !bounds(f, self.max_func.wrapping_add(1)) {
            self.error = FT_ERR_INVALID_REFERENCE;
            return;
        }
        let Some(def) = self.find_fdef(f) else {
            self.error = FT_ERR_INVALID_REFERENCE;
            return;
        };
        if !self.fdefs[def].active {
            self.error = FT_ERR_INVALID_REFERENCE;
            return;
        }
        if self.call_top >= self.call_size {
            self.error = FT_ERR_STACK_OVERFLOW;
            return;
        }
        let top = self.call_top as usize;
        self.call_stack[top] = TtCallRec { caller_range: self.cur_range, caller_ip: self.ip + 1, cur_count: 1, def_is_idef: false, def_index: def };
        self.call_top += 1;
        let d = self.fdefs[def];
        self.ins_goto_code_range(d.range, d.start);
    }

    fn ins_loopcall(&mut self, a: usize) {
        let f = self.arg(a, 1) as u32;
        let Some(def) = self.find_fdef(f) else {
            self.error = FT_ERR_INVALID_REFERENCE;
            return;
        };
        if !self.fdefs[def].active {
            self.error = FT_ERR_INVALID_REFERENCE;
            return;
        }
        if self.call_top >= self.call_size {
            self.error = FT_ERR_STACK_OVERFLOW;
            return;
        }
        let a0 = self.arg(a, 0);
        if a0 > 0 {
            let top = self.call_top as usize;
            self.call_stack[top] = TtCallRec { caller_range: self.cur_range, caller_ip: self.ip + 1, cur_count: a0, def_is_idef: false, def_index: def };
            self.call_top += 1;
            let d = self.fdefs[def];
            self.ins_goto_code_range(d.range, d.start);
            self.loopcall_counter = self.loopcall_counter.wrapping_add(a0 as u32);
            if self.loopcall_counter > self.loopcall_counter_max {
                self.error = FT_ERR_EXECUTION_TOO_LONG;
            }
        }
    }

    fn ins_idef(&mut self, a: usize) {
        if self.ini_range == TT_CODERANGE_GLYPH {
            self.error = FT_ERR_DEF_IN_GLYF_BYTECODE;
            return;
        }
        let a0 = self.arg(a, 0);
        let mut def = 0usize;
        let limit = self.num_idefs as usize;
        while def < limit {
            if self.idefs[def].opc == a0 as u32 {
                break;
            }
            def += 1;
        }
        if def == limit {
            if self.num_idefs >= self.max_idefs {
                self.error = FT_ERR_TOO_MANY_INSTRUCTION_DEFS;
                return;
            }
            self.num_idefs += 1;
        }
        if 0 > a0 || a0 > 0xFF {
            self.error = FT_ERR_TOO_MANY_INSTRUCTION_DEFS;
            return;
        }
        self.idefs[def].opc = a0 as u8 as u32;
        self.idefs[def].start = self.ip + 1;
        self.idefs[def].range = self.cur_range;
        self.idefs[def].active = true;
        if a0 as u32 > self.max_ins {
            self.max_ins = a0 as u8 as u32;
        }
        while self.skip_code() {
            match self.opcode {
                0x89 | 0x2C => {
                    self.error = FT_ERR_NESTED_DEFS;
                    return;
                }
                0x2D => {
                    self.idefs[def].end = self.ip;
                    return;
                }
                _ => {}
            }
        }
    }

    fn ins_npushb(&mut self, a: usize) {
        let mut ip = self.ip;
        ip += 1;
        if ip >= self.code_size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return;
        }
        let l = self.code[ip as usize] as i32;
        if ip + l >= self.code_size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return;
        }
        if bounds(l as u32, (self.stack_size + 1 - self.top) as u32) {
            self.error = FT_ERR_STACK_OVERFLOW;
            return;
        }
        for k in 0..l as usize {
            ip += 1;
            self.stack[a + k] = self.code[ip as usize] as i32;
        }
        self.new_top += l;
        self.ip = ip;
    }

    fn ins_npushw(&mut self, a: usize) {
        let mut ip = self.ip;
        ip += 1;
        if ip >= self.code_size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return;
        }
        let l = self.code[ip as usize] as i32;
        if ip + 2 * l >= self.code_size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return;
        }
        if bounds(l as u32, (self.stack_size + 1 - self.top) as u32) {
            self.error = FT_ERR_STACK_OVERFLOW;
            return;
        }
        for k in 0..l as usize {
            let v = (((self.code[ip as usize + 1] as u16) << 8) | self.code[ip as usize + 2] as u16) as i16;
            self.stack[a + k] = v as i32;
            ip += 2;
        }
        self.new_top += l;
        self.ip = ip;
    }

    fn ins_pushb(&mut self, a: usize) {
        let mut ip = self.ip;
        let l = self.opcode as i32 - 0xB0 + 1;
        if ip + l >= self.code_size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return;
        }
        if bounds(l as u32, (self.stack_size + 1 - self.top) as u32) {
            self.error = FT_ERR_STACK_OVERFLOW;
            return;
        }
        for k in 0..l as usize {
            ip += 1;
            self.stack[a + k] = self.code[ip as usize] as i32;
        }
        self.ip = ip;
    }

    fn ins_pushw(&mut self, a: usize) {
        let mut ip = self.ip;
        let l = self.opcode as i32 - 0xB8 + 1;
        if ip + 2 * l >= self.code_size {
            self.error = FT_ERR_CODE_OVERFLOW;
            return;
        }
        if bounds(l as u32, (self.stack_size + 1 - self.top) as u32) {
            self.error = FT_ERR_STACK_OVERFLOW;
            return;
        }
        for k in 0..l as usize {
            let v = (((self.code[ip as usize + 1] as u16) << 8) | self.code[ip as usize + 2] as u16) as i16;
            self.stack[a + k] = v as i32;
            ip += 2;
        }
        self.ip = ip;
    }

    fn ins_sxvtl(&mut self, a_idx1: u16, a_idx2: u16) -> Option<(i32, i32)> {
        let mut opcode = self.opcode;
        if bounds(a_idx1 as u32, self.zn(self.zp2)) || bounds(a_idx2 as u32, self.zn(self.zp1)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return None;
        }
        let p1 = self.cur(self.zp1, a_idx2 as usize);
        let p2 = self.cur(self.zp2, a_idx1 as usize);
        let mut a = sub_long(p1.x, p2.x);
        let mut b = sub_long(p1.y, p2.y);
        if a == 0 && b == 0 {
            a = 0x4000;
            opcode = 0;
        }
        if (opcode & 1) != 0 {
            let c = b;
            b = a;
            a = neg_long(c);
        }
        Some((a, b))
    }

    fn ins_sxy_tca(&mut self) {
        let opcode = self.opcode;
        let aa = ((opcode as i32 & 1) << 14) as i16;
        let bb = aa ^ 0x4000;
        if opcode < 4 {
            self.gs.proj_vector = FtUnitVector { x: aa, y: bb };
            self.gs.dual_vector = FtUnitVector { x: aa, y: bb };
        }
        if (opcode & 2) == 0 {
            self.gs.free_vector = FtUnitVector { x: aa, y: bb };
        }
        self.compute_funcs();
    }

    fn ins_szp(&mut self, which: u8, a: usize) {
        let a0 = self.arg(a, 0);
        let z = match a0 {
            0 => ZoneId::Twilight,
            1 => ZoneId::Pts,
            _ => {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                }
                return;
            }
        };
        match which {
            0 => {
                self.zp0 = z;
                self.gs.gep0 = a0 as u16;
            }
            1 => {
                self.zp1 = z;
                self.gs.gep1 = a0 as u16;
            }
            2 => {
                self.zp2 = z;
                self.gs.gep2 = a0 as u16;
            }
            _ => {
                self.zp0 = z;
                self.zp1 = z;
                self.zp2 = z;
                self.gs.gep0 = a0 as u16;
                self.gs.gep1 = a0 as u16;
                self.gs.gep2 = a0 as u16;
            }
        }
    }

    fn ins_gc(&mut self, a: usize) {
        let l = self.arg(a, 0) as u32;
        let r = if bounds(l, self.zn(self.zp2)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            0
        } else if self.opcode & 1 != 0 {
            self.fast_dualproj(self.org(self.zp2, l as usize))
        } else {
            self.fast_project(self.cur(self.zp2, l as usize))
        };
        self.set_arg(a, 0, r);
    }

    fn ins_scfs(&mut self, a: usize) {
        let l = self.arg(a, 0) as u16;
        if bounds(l as u32, self.zn(self.zp2)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let k = self.fast_project(self.cur(self.zp2, l as usize));
        let d = sub_long(self.arg(a, 1), k);
        self.func_move_z(self.zp2, l as usize, d);
        if self.gs.gep2 == 0 {
            let z = self.zone_mut(self.zp2);
            z.org[l as usize] = z.cur[l as usize];
        }
    }

    fn ins_md(&mut self, a: usize) {
        let k = self.arg(a, 1) as u16 as usize;
        let l = self.arg(a, 0) as u16 as usize;
        let d;
        if bounds(l as u32, self.zn(self.zp0)) || bounds(k as u32, self.zn(self.zp1)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            d = 0;
        } else if self.opcode & 1 != 0 {
            d = self.project(self.cur(self.zp0, l), self.cur(self.zp1, k));
        } else if self.gs.gep0 == 0 || self.gs.gep1 == 0 {
            d = self.dualproj(self.org(self.zp0, l), self.org(self.zp1, k));
        } else {
            let vec1 = self.orus(self.zp0, l);
            let vec2 = self.orus(self.zp1, k);
            if self.metrics.x_scale == self.metrics.y_scale {
                d = ft_mul_fix(self.dualproj(vec1, vec2), self.metrics.x_scale);
            } else {
                let vec = FtVector {
                    x: ft_mul_fix(vec1.x.wrapping_sub(vec2.x), self.metrics.x_scale),
                    y: ft_mul_fix(vec1.y.wrapping_sub(vec2.y), self.metrics.y_scale),
                };
                d = self.fast_dualproj(vec);
            }
        }
        self.set_arg(a, 0, d);
    }

    fn ins_sdpvtl(&mut self, a: usize) {
        let mut opcode = self.opcode;
        let p1 = self.arg(a, 1) as u16 as usize;
        let p2 = self.arg(a, 0) as u16 as usize;
        if bounds(p2 as u32, self.zn(self.zp1)) || bounds(p1 as u32, self.zn(self.zp2)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let v1 = self.org(self.zp1, p2);
        let v2 = self.org(self.zp2, p1);
        let mut aa = sub_long(v1.x, v2.x);
        let mut bb = sub_long(v1.y, v2.y);
        if aa == 0 && bb == 0 {
            aa = 0x4000;
            opcode = 0;
        }
        if (opcode & 1) != 0 {
            let c = bb;
            bb = aa;
            aa = neg_long(c);
        }
        let mut dv = self.gs.dual_vector;
        Self::normalize(aa, bb, &mut dv);
        self.gs.dual_vector = dv;
        let v1 = self.cur(self.zp1, p2);
        let v2 = self.cur(self.zp2, p1);
        aa = sub_long(v1.x, v2.x);
        bb = sub_long(v1.y, v2.y);
        if aa == 0 && bb == 0 {
            aa = 0x4000;
            opcode = 0;
        }
        if (opcode & 1) != 0 {
            let c = bb;
            bb = aa;
            aa = neg_long(c);
        }
        let mut pv = self.gs.proj_vector;
        Self::normalize(aa, bb, &mut pv);
        self.gs.proj_vector = pv;
        self.compute_funcs();
    }

    fn ins_instctrl(&mut self, a: usize) {
        let k = self.arg(a, 1) as u32;
        let l = self.arg(a, 0) as u32;
        if !(1..=3).contains(&k) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let kf = 1u32 << (k - 1);
        if l != 0 && l != kf {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        if self.ini_range == TT_CODERANGE_CVT {
            self.gs.instruct_control &= !(kf as u8);
            self.gs.instruct_control |= l as u8;
        } else if self.ini_range == TT_CODERANGE_GLYPH && k == 3 {
        } else if self.pedantic_hinting {
            self.error = FT_ERR_INVALID_REFERENCE;
        }
    }

    fn ins_scanctrl(&mut self, a: usize) {
        let a0 = self.arg(a, 0);
        let aa = a0 & 0xFF;
        if aa == 0xFF {
            self.gs.scan_control = true;
            return;
        } else if aa == 0 {
            self.gs.scan_control = false;
            return;
        }
        let ppem = self.tt_metrics.ppem as i32;
        if (a0 & 0x100) != 0 && ppem <= aa {
            self.gs.scan_control = true;
        }
        if (a0 & 0x200) != 0 && self.tt_metrics.rotated {
            self.gs.scan_control = true;
        }
        if (a0 & 0x400) != 0 && self.tt_metrics.stretched {
            self.gs.scan_control = true;
        }
        if (a0 & 0x800) != 0 && ppem > aa {
            self.gs.scan_control = false;
        }
        if (a0 & 0x1000) != 0 && self.tt_metrics.rotated {
            self.gs.scan_control = false;
        }
        if (a0 & 0x2000) != 0 && self.tt_metrics.stretched {
            self.gs.scan_control = false;
        }
    }

    fn ins_flippt(&mut self, a: usize) {
        let mut lp = self.gs.loop_;
        if self.new_top < lp {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            self.gs.loop_ = 1;
            return;
        }
        self.new_top -= lp;
        let mut args = a;
        while lp > 0 {
            lp -= 1;
            args -= 1;
            let point = self.stack[args] as u16;
            if bounds(point as u32, self.pts.n_points as u32) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
            } else {
                self.pts.tags[point as usize] ^= FT_CURVE_TAG_ON;
            }
        }
        self.gs.loop_ = 1;
    }

    fn ins_fliprg(&mut self, a: usize, on: bool) {
        let k = self.arg(a, 1) as u16;
        let l = self.arg(a, 0) as u16;
        if bounds(k as u32, self.pts.n_points as u32) || bounds(l as u32, self.pts.n_points as u32) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let mut i = l;
        while i <= k {
            if on {
                self.pts.tags[i as usize] |= FT_CURVE_TAG_ON;
            } else {
                self.pts.tags[i as usize] &= !FT_CURVE_TAG_ON;
            }
            if i == u16::MAX {
                break;
            }
            i += 1;
        }
    }

    fn compute_point_displacement(&mut self) -> Option<(FtF26Dot6, FtF26Dot6, ZoneId, u16)> {
        let (zp, p) = if self.opcode & 1 != 0 { (self.zp0, self.gs.rp1) } else { (self.zp1, self.gs.rp2) };
        if bounds(p as u32, self.zn(zp)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return None;
        }
        let d = self.project(self.cur(zp, p as usize), self.org(zp, p as usize));
        let x = ft_mul_fix(d, self.move_vector.x);
        let y = ft_mul_fix(d, self.move_vector.y);
        Some((x, y, zp, p))
    }

    fn move_zp2_point(&mut self, point: usize, dx: FtF26Dot6, dy: FtF26Dot6, touch: bool) {
        let fv = self.gs.free_vector;
        let z = self.zone_mut(self.zp2);
        if fv.x != 0 {
            z.cur[point].x = add_long(z.cur[point].x, dx);
            if touch {
                z.tags[point] |= FT_CURVE_TAG_TOUCH_X;
            }
        }
        if fv.y != 0 {
            z.cur[point].y = add_long(z.cur[point].y, dy);
            if touch {
                z.tags[point] |= FT_CURVE_TAG_TOUCH_Y;
            }
        }
    }

    fn ins_shp(&mut self, a: usize) {
        let mut lp = self.gs.loop_;
        if self.new_top < lp {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            self.gs.loop_ = 1;
            return;
        }
        self.new_top -= lp;
        let Some((dx, dy, _, _)) = self.compute_point_displacement() else {
            return;
        };
        let mut args = a;
        while lp > 0 {
            lp -= 1;
            args -= 1;
            let point = self.stack[args] as u16;
            if bounds(point as u32, self.zn(self.zp2)) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
            } else {
                self.move_zp2_point(point as usize, dx, dy, true);
            }
        }
        self.gs.loop_ = 1;
    }

    fn ins_shc(&mut self, a: usize) {
        let contour = self.arg(a, 0) as u16;
        let bnds: u16 = if self.gs.gep2 == 0 { 1 } else { self.zone(self.zp2).n_contours };
        if bounds(contour as u32, bnds as u32) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let Some((dx, dy, zp, refp)) = self.compute_point_displacement() else {
            return;
        };
        let z2 = self.zone(self.zp2);
        let start: u16 = if contour == 0 {
            0
        } else {
            z2.contours[contour as usize - 1].wrapping_add(1).wrapping_sub(z2.first_point)
        };
        let limit: u16 = if self.gs.gep2 == 0 {
            z2.n_points
        } else {
            z2.contours[contour as usize].wrapping_add(1).wrapping_sub(z2.first_point)
        };
        let mut i = start;
        while i < limit {
            if zp != self.zp2 || refp != i {
                self.move_zp2_point(i as usize, dx, dy, true);
            }
            i += 1;
        }
    }

    fn ins_shz(&mut self, a: usize) {
        if bounds(self.arg(a, 0) as u32, 2) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let Some((dx, dy, zp, refp)) = self.compute_point_displacement() else {
            return;
        };
        let z2 = self.zone(self.zp2);
        let limit: u16 = if self.gs.gep2 == 0 {
            z2.n_points
        } else if self.gs.gep2 == 1 && z2.n_contours > 0 {
            z2.contours[z2.n_contours as usize - 1].wrapping_add(1)
        } else {
            0
        };
        let mut i: u16 = 0;
        while i < limit {
            if zp != self.zp2 || refp != i {
                self.move_zp2_point(i as usize, dx, dy, false);
            }
            i += 1;
        }
    }

    fn ins_shpix(&mut self, a: usize) {
        let mut lp = self.gs.loop_;
        if self.new_top < lp {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            self.gs.loop_ = 1;
            return;
        }
        self.new_top -= lp;
        let a0 = self.arg(a, 0);
        let dx = tt_mul_fix14(a0, self.gs.free_vector.x as i32);
        let dy = tt_mul_fix14(a0, self.gs.free_vector.y as i32);
        let mut args = a;
        while lp > 0 {
            lp -= 1;
            args -= 1;
            let point = self.stack[args] as u16;
            if bounds(point as u32, self.zn(self.zp2)) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
            } else {
                self.move_zp2_point(point as usize, dx, dy, true);
            }
        }
        self.gs.loop_ = 1;
    }

    fn ins_msirp(&mut self, a: usize) {
        let point = self.arg(a, 0) as u16 as usize;
        let rp0 = self.gs.rp0 as usize;
        if bounds(point as u32, self.zn(self.zp1)) || bounds(rp0 as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let a1 = self.arg(a, 1);
        if self.gs.gep1 == 0 {
            let o = self.org(self.zp0, rp0);
            self.zone_mut(self.zp1).org[point] = o;
            self.func_move_orig_z(self.zp1, point, a1);
            let z = self.zone_mut(self.zp1);
            z.cur[point] = z.org[point];
        }
        let distance = self.project(self.cur(self.zp1, point), self.cur(self.zp0, rp0));
        self.func_move_z(self.zp1, point, sub_long(a1, distance));
        self.gs.rp1 = self.gs.rp0;
        self.gs.rp2 = point as u16;
        if (self.opcode & 1) != 0 {
            self.gs.rp0 = point as u16;
        }
    }

    fn ins_mdap(&mut self, a: usize) {
        let point = self.arg(a, 0) as u16 as usize;
        if bounds(point as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let distance = if (self.opcode & 1) != 0 {
            let cur_dist = self.fast_project(self.cur(self.zp0, point));
            sub_long(self.func_round(cur_dist, 0), cur_dist)
        } else {
            0
        };
        self.func_move_z(self.zp0, point, distance);
        self.gs.rp0 = point as u16;
        self.gs.rp1 = point as u16;
    }

    fn ins_miap(&mut self, a: usize) {
        let cvt_entry = self.arg(a, 1) as u32;
        let point = self.arg(a, 0) as u16;
        if bounds(point as u32, self.zn(self.zp0)) || bounds(cvt_entry, self.cvt_size) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
        } else {
            let p = point as usize;
            let mut distance = self.func_read_cvt(cvt_entry as usize);
            if self.gs.gep0 == 0 {
                let fv = self.gs.free_vector;
                let z = self.zone_mut(self.zp0);
                z.org[p].x = tt_mul_fix14(distance, fv.x as i32);
                z.org[p].y = tt_mul_fix14(distance, fv.y as i32);
                z.cur[p] = z.org[p];
            }
            let org_dist = self.fast_project(self.cur(self.zp0, p));
            if (self.opcode & 1) != 0 {
                let control_value_cutin = self.gs.control_value_cutin;
                let mut delta = sub_long(distance, org_dist);
                if delta < 0 {
                    delta = neg_long(delta);
                }
                if delta > control_value_cutin {
                    distance = org_dist;
                }
                distance = self.func_round(distance, 0);
            }
            self.func_move_z(self.zp0, p, sub_long(distance, org_dist));
        }
        self.gs.rp0 = point;
        self.gs.rp1 = point;
    }

    fn ins_mdrp(&mut self, a: usize) {
        let point = self.arg(a, 0) as u16;
        let rp0 = self.gs.rp0 as usize;
        if bounds(point as u32, self.zn(self.zp1)) || bounds(rp0 as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
        } else {
            let p = point as usize;
            let mut org_dist;
            if self.gs.gep0 == 0 || self.gs.gep1 == 0 {
                org_dist = self.dualproj(self.org(self.zp1, p), self.org(self.zp0, rp0));
            } else {
                let vec1 = self.orus(self.zp1, p);
                let vec2 = self.orus(self.zp0, rp0);
                if self.metrics.x_scale == self.metrics.y_scale {
                    org_dist = ft_mul_fix(self.dualproj(vec1, vec2), self.metrics.x_scale);
                } else {
                    let vec = FtVector {
                        x: ft_mul_fix(sub_long(vec1.x, vec2.x), self.metrics.x_scale),
                        y: ft_mul_fix(sub_long(vec1.y, vec2.y), self.metrics.y_scale),
                    };
                    org_dist = self.fast_dualproj(vec);
                }
            }
            let swv = self.gs.single_width_value;
            let swc = self.gs.single_width_cutin;
            if swc > 0 && org_dist < add_long(swv, swc) && org_dist > sub_long(swv, swc) {
                org_dist = if org_dist >= 0 { swv } else { swv.wrapping_neg() };
            }
            let compensation = self.gs.compensation[(self.opcode & 3) as usize];
            let mut distance =
                if (self.opcode & 4) != 0 { self.func_round(org_dist, compensation) } else { Self::round_none(org_dist, compensation) };
            if (self.opcode & 8) != 0 {
                let md = self.gs.minimum_distance;
                if org_dist >= 0 {
                    if distance < md {
                        distance = md;
                    }
                } else if distance > neg_long(md) {
                    distance = neg_long(md);
                }
            }
            let od = self.project(self.cur(self.zp1, p), self.cur(self.zp0, rp0));
            self.func_move_z(self.zp1, p, sub_long(distance, od));
        }
        self.gs.rp1 = self.gs.rp0;
        self.gs.rp2 = point;
        if (self.opcode & 16) != 0 {
            self.gs.rp0 = point;
        }
    }

    fn ins_mirp(&mut self, a: usize) {
        let point = self.arg(a, 0) as u16;
        let cvt_entry = add_long(self.arg(a, 1), 1) as u32;
        let rp0 = self.gs.rp0 as usize;
        if bounds(point as u32, self.zn(self.zp1)) || bounds(cvt_entry, self.cvt_size.wrapping_add(1)) || bounds(rp0 as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
        } else {
            let p = point as usize;
            let mut cvt_dist = if cvt_entry == 0 { 0 } else { self.func_read_cvt(cvt_entry as usize - 1) };
            let mut delta = sub_long(cvt_dist, self.gs.single_width_value);
            if delta < 0 {
                delta = neg_long(delta);
            }
            if delta < self.gs.single_width_cutin {
                cvt_dist = if cvt_dist >= 0 { self.gs.single_width_value } else { self.gs.single_width_value.wrapping_neg() };
            }
            if self.gs.gep1 == 0 {
                let fv = self.gs.free_vector;
                let o0 = self.org(self.zp0, rp0);
                let z = self.zone_mut(self.zp1);
                z.org[p].x = add_long(o0.x, tt_mul_fix14(cvt_dist, fv.x as i32));
                z.org[p].y = add_long(o0.y, tt_mul_fix14(cvt_dist, fv.y as i32));
                z.cur[p] = z.org[p];
            }
            let org_dist = self.dualproj(self.org(self.zp1, p), self.org(self.zp0, rp0));
            let cur_dist = self.project(self.cur(self.zp1, p), self.cur(self.zp0, rp0));
            if self.gs.auto_flip && (org_dist ^ cvt_dist) < 0 {
                cvt_dist = neg_long(cvt_dist);
            }
            let compensation = self.gs.compensation[(self.opcode & 3) as usize];
            let mut distance;
            if (self.opcode & 4) != 0 {
                if self.gs.gep0 == self.gs.gep1 {
                    let control_value_cutin = self.gs.control_value_cutin;
                    let mut delta = sub_long(cvt_dist, org_dist);
                    if delta < 0 {
                        delta = neg_long(delta);
                    }
                    if delta > control_value_cutin {
                        cvt_dist = org_dist;
                    }
                }
                distance = self.func_round(cvt_dist, compensation);
            } else {
                distance = Self::round_none(cvt_dist, compensation);
            }
            if (self.opcode & 8) != 0 {
                let md = self.gs.minimum_distance;
                if org_dist >= 0 {
                    if distance < md {
                        distance = md;
                    }
                } else if distance > neg_long(md) {
                    distance = neg_long(md);
                }
            }
            self.func_move_z(self.zp1, p, sub_long(distance, cur_dist));
        }
        self.gs.rp1 = self.gs.rp0;
        self.gs.rp2 = point;
        if (self.opcode & 16) != 0 {
            self.gs.rp0 = point;
        }
    }

    fn ins_alignrp(&mut self, a: usize) {
        let mut lp = self.gs.loop_;
        if self.new_top < lp {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            self.gs.loop_ = 1;
            return;
        }
        self.new_top -= lp;
        let rp0 = self.gs.rp0 as usize;
        if bounds(rp0 as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            self.gs.loop_ = 1;
            return;
        }
        let mut args = a;
        while lp > 0 {
            lp -= 1;
            args -= 1;
            let point = self.stack[args] as u16 as usize;
            if bounds(point as u32, self.zn(self.zp1)) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
            } else {
                let distance = self.project(self.cur(self.zp1, point), self.cur(self.zp0, rp0));
                self.func_move_z(self.zp1, point, neg_long(distance));
            }
        }
        self.gs.loop_ = 1;
    }

    fn ins_isect(&mut self, a: usize) {
        let point = self.arg(a, 0) as u16 as usize;
        let a0 = self.arg(a, 1) as u16 as usize;
        let a1 = self.arg(a, 2) as u16 as usize;
        let b0 = self.arg(a, 3) as u16 as usize;
        let b1 = self.arg(a, 4) as u16 as usize;
        if bounds(b0 as u32, self.zn(self.zp0))
            || bounds(b1 as u32, self.zn(self.zp0))
            || bounds(a0 as u32, self.zn(self.zp1))
            || bounds(a1 as u32, self.zn(self.zp1))
            || bounds(point as u32, self.zn(self.zp2))
        {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let pb0 = self.cur(self.zp0, b0);
        let pb1 = self.cur(self.zp0, b1);
        let pa0 = self.cur(self.zp1, a0);
        let pa1 = self.cur(self.zp1, a1);
        let dbx = sub_long(pb1.x, pb0.x);
        let dby = sub_long(pb1.y, pb0.y);
        let dax = sub_long(pa1.x, pa0.x);
        let day = sub_long(pa1.y, pa0.y);
        let dx = sub_long(pb0.x, pa0.x);
        let dy = sub_long(pb0.y, pa0.y);
        let discriminant = add_long(ft_mul_div(dax, neg_long(dby), 0x40), ft_mul_div(day, dbx, 0x40));
        let dotproduct = add_long(ft_mul_div(dax, dbx, 0x40), ft_mul_div(day, dby, 0x40));
        let z2 = self.zp2;
        if mul_long(19, ft_abs(discriminant)) > ft_abs(dotproduct) {
            let val = add_long(ft_mul_div(dx, neg_long(dby), 0x40), ft_mul_div(dy, dbx, 0x40));
            let rx = ft_mul_div(val, dax, discriminant);
            let ry = ft_mul_div(val, day, discriminant);
            let z = self.zone_mut(z2);
            z.cur[point].x = add_long(pa0.x, rx);
            z.cur[point].y = add_long(pa0.y, ry);
        } else {
            let z = self.zone_mut(z2);
            z.cur[point].x = add_long(add_long(pa0.x, pa1.x), add_long(pb0.x, pb1.x)) / 4;
            z.cur[point].y = add_long(add_long(pa0.y, pa1.y), add_long(pb0.y, pb1.y)) / 4;
        }
        self.zone_mut(z2).tags[point] |= FT_CURVE_TAG_TOUCH_BOTH;
    }

    fn ins_alignpts(&mut self, a: usize) {
        let p1 = self.arg(a, 0) as u16 as usize;
        let p2 = self.arg(a, 1) as u16 as usize;
        if bounds(p1 as u32, self.zn(self.zp1)) || bounds(p2 as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let distance = self.project(self.cur(self.zp0, p2), self.cur(self.zp1, p1)) / 2;
        self.func_move_z(self.zp1, p1, distance);
        self.func_move_z(self.zp0, p2, neg_long(distance));
    }

    fn ins_ip(&mut self, a: usize) {
        let mut lp = self.gs.loop_;
        if self.new_top < lp {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            self.gs.loop_ = 1;
            return;
        }
        self.new_top -= lp;
        let rp1 = self.gs.rp1 as usize;
        if bounds(rp1 as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            self.gs.loop_ = 1;
            return;
        }
        let twilight = self.gs.gep0 == 0 || self.gs.gep1 == 0 || self.gs.gep2 == 0;
        let orus_base = if twilight { self.org(self.zp0, rp1) } else { self.orus(self.zp0, rp1) };
        let cur_base = self.cur(self.zp0, rp1);
        let rp2 = self.gs.rp2 as usize;
        let (old_range, cur_range);
        if bounds(rp2 as u32, self.zn(self.zp1)) {
            old_range = 0;
            cur_range = 0;
        } else {
            if twilight {
                old_range = self.dualproj(self.org(self.zp1, rp2), orus_base);
            } else if self.metrics.x_scale == self.metrics.y_scale {
                old_range = self.dualproj(self.orus(self.zp1, rp2), orus_base);
            } else {
                let o = self.orus(self.zp1, rp2);
                let vec = FtVector {
                    x: ft_mul_fix(sub_long(o.x, orus_base.x), self.metrics.x_scale),
                    y: ft_mul_fix(sub_long(o.y, orus_base.y), self.metrics.y_scale),
                };
                old_range = self.fast_dualproj(vec);
            }
            cur_range = self.project(self.cur(self.zp1, rp2), cur_base);
        }
        let mut args = a;
        while lp > 0 {
            lp -= 1;
            args -= 1;
            let point = self.stack[args] as u32;
            if bounds(point, self.zn(self.zp2)) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
                continue;
            }
            let p = point as usize;
            let org_dist = if twilight {
                self.dualproj(self.org(self.zp2, p), orus_base)
            } else if self.metrics.x_scale == self.metrics.y_scale {
                self.dualproj(self.orus(self.zp2, p), orus_base)
            } else {
                let o = self.orus(self.zp2, p);
                let vec = FtVector {
                    x: ft_mul_fix(sub_long(o.x, orus_base.x), self.metrics.x_scale),
                    y: ft_mul_fix(sub_long(o.y, orus_base.y), self.metrics.y_scale),
                };
                self.fast_dualproj(vec)
            };
            let cur_dist = self.project(self.cur(self.zp2, p), cur_base);
            let new_dist = if org_dist != 0 {
                if old_range != 0 {
                    ft_mul_div(org_dist, cur_range, old_range)
                } else {
                    org_dist
                }
            } else {
                0
            };
            self.func_move_z(self.zp2, point as u16 as usize, sub_long(new_dist, cur_dist));
        }
        self.gs.loop_ = 1;
    }

    fn ins_utp(&mut self, a: usize) {
        let point = self.arg(a, 0) as u16 as usize;
        if bounds(point as u32, self.zn(self.zp0)) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
            return;
        }
        let mut mask: u8 = 0xFF;
        if self.gs.free_vector.x != 0 {
            mask &= !FT_CURVE_TAG_TOUCH_X;
        }
        if self.gs.free_vector.y != 0 {
            mask &= !FT_CURVE_TAG_TOUCH_Y;
        }
        self.zone_mut(self.zp0).tags[point] &= mask;
    }

    fn ins_iup(&mut self) {
        if self.pts.n_contours == 0 {
            return;
        }
        let (mask, yaxis) = if self.opcode & 1 != 0 { (FT_CURVE_TAG_TOUCH_X, false) } else { (FT_CURVE_TAG_TOUCH_Y, true) };
        let max_points = self.pts.n_points as u32;
        let pts = &mut self.pts;
        let mut w = IupWorker { pts, yaxis, max_points };
        let mut contour: u16 = 0;
        let mut point: u32 = 0;
        loop {
            let mut end_point = (w.pts.contours[contour as usize] as u32).wrapping_sub(w.pts.first_point as u32);
            let first_point = point;
            if bounds(end_point, w.pts.n_points as u32) {
                end_point = w.pts.n_points as u32 - 1;
            }
            while point <= end_point && (w.pts.tags[point as usize] & mask) == 0 {
                point += 1;
            }
            if point <= end_point {
                let first_touched = point;
                let mut cur_touched = point;
                point += 1;
                while point <= end_point {
                    if (w.pts.tags[point as usize] & mask) != 0 {
                        w.interpolate(cur_touched + 1, point - 1, cur_touched, point);
                        cur_touched = point;
                    }
                    point += 1;
                }
                if cur_touched == first_touched {
                    w.shift(first_point, end_point, cur_touched);
                } else {
                    w.interpolate((cur_touched + 1) as u16 as u32, end_point, cur_touched, first_touched);
                    if first_touched > 0 {
                        w.interpolate(first_point, first_touched - 1, cur_touched, first_touched);
                    }
                }
            }
            contour += 1;
            if contour as i32 >= w.pts.n_contours as i32 {
                break;
            }
        }
    }

    fn ins_deltap(&mut self, a: usize) {
        let mut nump = self.arg(a, 0);
        if nump < 0 || nump > self.new_top / 2 {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            nump = self.new_top / 2;
        }
        self.new_top -= 2 * nump;
        let mut p = self.func_cur_ppem() - self.gs.delta_base as i32;
        match self.opcode {
            0x71 => p -= 16,
            0x72 => p -= 32,
            _ => {}
        }
        if p & !0xF != 0 {
            return;
        }
        p <<= 4;
        let f = 1i32 << (6 - self.gs.delta_shift as i32);
        let mut args = a;
        while nump > 0 {
            nump -= 1;
            args -= 1;
            let aa = self.stack[args] as u16 as usize;
            args -= 1;
            let mut b = self.stack[args];
            if bounds(aa as u32, self.zn(self.zp0)) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
            } else if (b & 0xF0) == p {
                b = (b & 0xF) - 8;
                if b >= 0 {
                    b += 1;
                }
                b = b.wrapping_mul(f);
                self.func_move_z(self.zp0, aa, b);
            }
        }
    }

    fn ins_deltac(&mut self, a: usize) {
        let mut nump = self.arg(a, 0);
        if nump < 0 || nump > self.new_top / 2 {
            if self.pedantic_hinting {
                self.error = FT_ERR_TOO_FEW_ARGUMENTS;
            }
            nump = self.new_top / 2;
        }
        self.new_top -= 2 * nump;
        let mut p = self.func_cur_ppem() - self.gs.delta_base as i32;
        match self.opcode {
            0x74 => p -= 16,
            0x75 => p -= 32,
            _ => {}
        }
        if p & !0xF != 0 {
            return;
        }
        p <<= 4;
        let f = 1i32 << (6 - self.gs.delta_shift as i32);
        let mut args = a;
        while nump > 0 {
            nump -= 1;
            args -= 1;
            let aa = self.stack[args] as u32;
            args -= 1;
            let mut b = self.stack[args];
            if bounds(aa, self.cvt_size) {
                if self.pedantic_hinting {
                    self.error = FT_ERR_INVALID_REFERENCE;
                    return;
                }
            } else if (b & 0xF0) == p {
                b = (b & 0xF) - 8;
                if b >= 0 {
                    b += 1;
                }
                b = b.wrapping_mul(f);
                self.func_move_cvt(aa as usize, b);
            }
        }
    }

    fn ins_getinfo(&mut self, a: usize) {
        let a0 = self.arg(a, 0);
        let mut k = 0;
        if (a0 & 1) != 0 {
            k = self.interpreter_version;
        }
        if (a0 & 2) != 0 && self.tt_metrics.rotated {
            k |= 1 << 8;
        }
        if (a0 & 4) != 0 && self.tt_metrics.stretched {
            k |= 1 << 9;
        }
        if (a0 & 32) != 0 && self.grayscale {
            k |= 1 << 12;
        }
        self.set_arg(a, 0, k);
    }

    fn ins_unknown(&mut self) {
        for def in 0..self.num_idefs as usize {
            let d = self.idefs[def];
            if d.opc as u8 == self.opcode && d.active {
                if self.call_top >= self.call_size {
                    self.error = FT_ERR_STACK_OVERFLOW;
                    return;
                }
                let top = self.call_top as usize;
                self.call_stack[top] = TtCallRec { caller_range: self.cur_range, caller_ip: self.ip + 1, cur_count: 1, def_is_idef: true, def_index: def };
                self.call_top += 1;
                self.ins_goto_code_range(d.range, d.start);
                return;
            }
        }
        self.error = FT_ERR_INVALID_OPCODE;
    }

    fn ins_ws(&mut self, a: usize) {
        let i = self.arg(a, 0) as u32;
        if bounds(i, self.store_size as u32) {
            if self.pedantic_hinting {
                self.error = FT_ERR_INVALID_REFERENCE;
            }
        } else {
            if self.ini_range == TT_CODERANGE_GLYPH && !self.storage_is_glyf {
                let mut g = self.storage_base.clone();
                g.resize(self.store_size as usize, 0);
                self.glyf_storage = g;
                self.storage_is_glyf = true;
            }
            let v = self.arg(a, 1);
            if self.storage_is_glyf {
                self.glyf_storage[i as usize] = v;
            } else {
                self.storage_base[i as usize] = v;
            }
        }
    }

    /// `TT_RunIns`.
    pub fn run_ins(&mut self) -> FtError {
        let mut ins_counter: u32 = 0;
        loop {
            ins_counter += 1;
            if ins_counter > TT_CONFIG_OPTION_MAX_RUNNABLE_OPCODES {
                self.error = FT_ERR_EXECUTION_TOO_LONG;
                return self.error;
            }
            self.error = FT_ERR_OK;
            self.opcode = self.code[self.ip as usize];
            self.length = 1;
            let pops = (POP_PUSH_COUNT[self.opcode as usize] >> 4) as i32;
            self.args = self.top - pops;
            if self.args < 0 {
                if self.pedantic_hinting {
                    self.error = FT_ERR_TOO_FEW_ARGUMENTS;
                    return self.error;
                }
                for i in 0..pops as usize {
                    self.stack[i] = 0;
                }
                self.args = 0;
            }
            self.new_top = self.args + (POP_PUSH_COUNT[self.opcode as usize] & 15) as i32;
            if self.new_top > self.stack_size {
                self.error = FT_ERR_STACK_OVERFLOW;
                return self.error;
            }
            let a = self.args as usize;
            let opcode = self.opcode;
            match opcode {
                0x00..=0x05 => self.ins_sxy_tca(),
                0x06 | 0x07 => {
                    if let Some((x, y)) = self.ins_sxvtl(self.arg(a, 1) as u16, self.arg(a, 0) as u16) {
                        let mut pv = self.gs.proj_vector;
                        Self::normalize(x, y, &mut pv);
                        self.gs.proj_vector = pv;
                        self.gs.dual_vector = self.gs.proj_vector;
                        self.compute_funcs();
                    }
                }
                0x08 | 0x09 => {
                    if let Some((x, y)) = self.ins_sxvtl(self.arg(a, 1) as u16, self.arg(a, 0) as u16) {
                        let mut fv = self.gs.free_vector;
                        Self::normalize(x, y, &mut fv);
                        self.gs.free_vector = fv;
                        self.compute_funcs();
                    }
                }
                0x0A => {
                    let y = self.arg(a, 1) as i16 as i32;
                    let x = self.arg(a, 0) as i16 as i32;
                    let mut pv = self.gs.proj_vector;
                    Self::normalize(x, y, &mut pv);
                    self.gs.proj_vector = pv;
                    self.gs.dual_vector = self.gs.proj_vector;
                    self.compute_funcs();
                }
                0x0B => {
                    let y = self.arg(a, 1) as i16 as i32;
                    let x = self.arg(a, 0) as i16 as i32;
                    let mut fv = self.gs.free_vector;
                    Self::normalize(x, y, &mut fv);
                    self.gs.free_vector = fv;
                    self.compute_funcs();
                }
                0x0C => {
                    self.set_arg(a, 0, self.gs.proj_vector.x as i32);
                    self.set_arg(a, 1, self.gs.proj_vector.y as i32);
                }
                0x0D => {
                    self.set_arg(a, 0, self.gs.free_vector.x as i32);
                    self.set_arg(a, 1, self.gs.free_vector.y as i32);
                }
                0x0E => {
                    self.gs.free_vector = self.gs.proj_vector;
                    self.compute_funcs();
                }
                0x0F => self.ins_isect(a),
                0x10 => self.gs.rp0 = self.arg(a, 0) as u16,
                0x11 => self.gs.rp1 = self.arg(a, 0) as u16,
                0x12 => self.gs.rp2 = self.arg(a, 0) as u16,
                0x13 => self.ins_szp(0, a),
                0x14 => self.ins_szp(1, a),
                0x15 => self.ins_szp(2, a),
                0x16 => self.ins_szp(3, a),
                0x17 => {
                    let a0 = self.arg(a, 0);
                    if a0 < 0 {
                        self.error = FT_ERR_BAD_ARGUMENT;
                    } else {
                        self.gs.loop_ = if a0 > 0xFFFF { 0xFFFF } else { a0 };
                    }
                }
                0x18 => {
                    self.gs.round_state = TT_ROUND_TO_GRID;
                    self.func_round = RoundFunc::ToGrid;
                }
                0x19 => {
                    self.gs.round_state = TT_ROUND_TO_HALF_GRID;
                    self.func_round = RoundFunc::ToHalfGrid;
                }
                0x1A => self.gs.minimum_distance = self.arg(a, 0),
                0x1B => self.ins_else(),
                0x1C => self.ins_jmpr(a),
                0x1D => self.gs.control_value_cutin = self.arg(a, 0),
                0x1E => self.gs.single_width_cutin = self.arg(a, 0),
                0x1F => self.gs.single_width_value = ft_mul_fix(self.arg(a, 0), self.tt_metrics.scale),
                0x20 => self.set_arg(a, 1, self.arg(a, 0)),
                0x21 => {}
                0x22 => self.new_top = 0,
                0x23 => {
                    let l = self.arg(a, 0);
                    self.set_arg(a, 0, self.arg(a, 1));
                    self.set_arg(a, 1, l);
                }
                0x24 => self.set_arg(a, 0, self.top),
                0x25 => {
                    let l = self.arg(a, 0);
                    if l <= 0 || l > self.args {
                        if self.pedantic_hinting {
                            self.error = FT_ERR_INVALID_REFERENCE;
                        }
                        self.set_arg(a, 0, 0);
                    } else {
                        self.set_arg(a, 0, self.stack[a - l as usize]);
                    }
                }
                0x26 => {
                    let l = self.arg(a, 0);
                    if l <= 0 || l > self.args {
                        if self.pedantic_hinting {
                            self.error = FT_ERR_INVALID_REFERENCE;
                        }
                    } else {
                        let l = l as usize;
                        let k = self.stack[a - l];
                        self.stack.copy_within(a - l + 1..a, a - l);
                        self.stack[a - 1] = k;
                    }
                }
                0x27 => self.ins_alignpts(a),
                0x28 => self.ins_unknown(),
                0x29 => self.ins_utp(a),
                0x2A => self.ins_loopcall(a),
                0x2B => self.ins_call(a),
                0x2C => self.ins_fdef(a),
                0x2D => self.ins_endf(),
                0x2E | 0x2F => self.ins_mdap(a),
                0x30 | 0x31 => self.ins_iup(),
                0x32 | 0x33 => self.ins_shp(a),
                0x34 | 0x35 => self.ins_shc(a),
                0x36 | 0x37 => self.ins_shz(a),
                0x38 => self.ins_shpix(a),
                0x39 => self.ins_ip(a),
                0x3A | 0x3B => self.ins_msirp(a),
                0x3C => self.ins_alignrp(a),
                0x3D => {
                    self.gs.round_state = TT_ROUND_TO_DOUBLE_GRID;
                    self.func_round = RoundFunc::ToDoubleGrid;
                }
                0x3E | 0x3F => self.ins_miap(a),
                0x40 => self.ins_npushb(a),
                0x41 => self.ins_npushw(a),
                0x42 => self.ins_ws(a),
                0x43 => {
                    let i = self.arg(a, 0) as u32;
                    if bounds(i, self.store_size as u32) {
                        if self.pedantic_hinting {
                            self.error = FT_ERR_INVALID_REFERENCE;
                        } else {
                            self.set_arg(a, 0, 0);
                        }
                    } else {
                        let v = self.storage()[i as usize];
                        self.set_arg(a, 0, v);
                    }
                }
                0x44 => {
                    let i = self.arg(a, 0) as u32;
                    if bounds(i, self.cvt_size) {
                        if self.pedantic_hinting {
                            self.error = FT_ERR_INVALID_REFERENCE;
                        }
                    } else {
                        let v = self.arg(a, 1);
                        self.func_write_cvt(i as usize, v);
                    }
                }
                0x45 => {
                    let i = self.arg(a, 0) as u32;
                    if bounds(i, self.cvt_size) {
                        if self.pedantic_hinting {
                            self.error = FT_ERR_INVALID_REFERENCE;
                        } else {
                            self.set_arg(a, 0, 0);
                        }
                    } else {
                        let v = self.func_read_cvt(i as usize);
                        self.set_arg(a, 0, v);
                    }
                }
                0x46 | 0x47 => self.ins_gc(a),
                0x48 => self.ins_scfs(a),
                0x49 | 0x4A => self.ins_md(a),
                0x4B => {
                    let v = self.func_cur_ppem();
                    self.set_arg(a, 0, v);
                }
                0x4C => {
                    let v = self.func_cur_ppem();
                    self.set_arg(a, 0, v);
                }
                0x4D => self.gs.auto_flip = true,
                0x4E => self.gs.auto_flip = false,
                0x4F => self.error = FT_ERR_DEBUG_OPCODE,
                0x50 => self.set_arg(a, 0, (self.arg(a, 0) < self.arg(a, 1)) as i32),
                0x51 => self.set_arg(a, 0, (self.arg(a, 0) <= self.arg(a, 1)) as i32),
                0x52 => self.set_arg(a, 0, (self.arg(a, 0) > self.arg(a, 1)) as i32),
                0x53 => self.set_arg(a, 0, (self.arg(a, 0) >= self.arg(a, 1)) as i32),
                0x54 => self.set_arg(a, 0, (self.arg(a, 0) == self.arg(a, 1)) as i32),
                0x55 => self.set_arg(a, 0, (self.arg(a, 0) != self.arg(a, 1)) as i32),
                0x56 => {
                    let r = self.func_round(self.arg(a, 0), 0);
                    self.set_arg(a, 0, ((r & 64) == 64) as i32);
                }
                0x57 => {
                    let r = self.func_round(self.arg(a, 0), 0);
                    self.set_arg(a, 0, ((r & 64) == 0) as i32);
                }
                0x58 => self.ins_if(a),
                0x59 => {}
                0x5A => self.set_arg(a, 0, (self.arg(a, 0) != 0 && self.arg(a, 1) != 0) as i32),
                0x5B => self.set_arg(a, 0, (self.arg(a, 0) != 0 || self.arg(a, 1) != 0) as i32),
                0x5C => self.set_arg(a, 0, (self.arg(a, 0) == 0) as i32),
                0x5D => self.ins_deltap(a),
                0x5E => self.gs.delta_base = self.arg(a, 0) as u16,
                0x5F => {
                    let a0 = self.arg(a, 0) as u32;
                    if a0 > 6 {
                        self.error = FT_ERR_BAD_ARGUMENT;
                    } else {
                        self.gs.delta_shift = a0 as u16;
                    }
                }
                0x60 => self.set_arg(a, 0, add_long(self.arg(a, 0), self.arg(a, 1))),
                0x61 => self.set_arg(a, 0, sub_long(self.arg(a, 0), self.arg(a, 1))),
                0x62 => {
                    if self.arg(a, 1) == 0 {
                        self.error = FT_ERR_DIVIDE_BY_ZERO;
                    } else {
                        self.set_arg(a, 0, ft_mul_div_no_round(self.arg(a, 0), 64, self.arg(a, 1)));
                    }
                }
                0x63 => self.set_arg(a, 0, ft_mul_div(self.arg(a, 0), self.arg(a, 1), 64)),
                0x64 => {
                    if self.arg(a, 0) < 0 {
                        self.set_arg(a, 0, neg_long(self.arg(a, 0)));
                    }
                }
                0x65 => self.set_arg(a, 0, neg_long(self.arg(a, 0))),
                0x66 => self.set_arg(a, 0, ft_pix_floor(self.arg(a, 0))),
                0x67 => self.set_arg(a, 0, ft_pix_ceil_long(self.arg(a, 0))),
                0x68..=0x6B => {
                    let c = self.gs.compensation[(self.opcode & 3) as usize];
                    let r = self.func_round(self.arg(a, 0), c);
                    self.set_arg(a, 0, r);
                }
                0x6C..=0x6F => {
                    let c = self.gs.compensation[(self.opcode & 3) as usize];
                    self.set_arg(a, 0, Self::round_none(self.arg(a, 0), c));
                }
                0x70 => {
                    let i = self.arg(a, 0) as u32;
                    if bounds(i, self.cvt_size) {
                        if self.pedantic_hinting {
                            self.error = FT_ERR_INVALID_REFERENCE;
                        }
                    } else {
                        let v = ft_mul_fix(self.arg(a, 1), self.tt_metrics.scale);
                        self.cvt_mut()[i as usize] = v;
                    }
                }
                0x71 | 0x72 => self.ins_deltap(a),
                0x73..=0x75 => self.ins_deltac(a),
                0x76 => {
                    self.set_super_round(0x4000, self.arg(a, 0));
                    self.gs.round_state = TT_ROUND_SUPER;
                    self.func_round = RoundFunc::Super;
                }
                0x77 => {
                    self.set_super_round(0x2D41, self.arg(a, 0));
                    self.gs.round_state = TT_ROUND_SUPER_45;
                    self.func_round = RoundFunc::Super45;
                }
                0x78 => {
                    if self.arg(a, 1) != 0 {
                        self.ins_jmpr(a);
                    }
                }
                0x79 => {
                    if self.arg(a, 1) == 0 {
                        self.ins_jmpr(a);
                    }
                }
                0x7A => {
                    self.gs.round_state = TT_ROUND_OFF;
                    self.func_round = RoundFunc::None;
                }
                0x7B => self.ins_unknown(),
                0x7C => {
                    self.gs.round_state = TT_ROUND_UP_TO_GRID;
                    self.func_round = RoundFunc::UpToGrid;
                }
                0x7D => {
                    self.gs.round_state = TT_ROUND_DOWN_TO_GRID;
                    self.func_round = RoundFunc::DownToGrid;
                }
                0x7E | 0x7F => {}
                0x80 => self.ins_flippt(a),
                0x81 => self.ins_fliprg(a, true),
                0x82 => self.ins_fliprg(a, false),
                0x83 | 0x84 => self.ins_unknown(),
                0x85 => self.ins_scanctrl(a),
                0x86 | 0x87 => self.ins_sdpvtl(a),
                0x88 => self.ins_getinfo(a),
                0x89 => self.ins_idef(a),
                0x8A => {
                    let (aa, bb, cc) = (self.arg(a, 2), self.arg(a, 1), self.arg(a, 0));
                    self.set_arg(a, 2, cc);
                    self.set_arg(a, 1, aa);
                    self.set_arg(a, 0, bb);
                }
                0x8B => {
                    if self.arg(a, 1) > self.arg(a, 0) {
                        self.set_arg(a, 0, self.arg(a, 1));
                    }
                }
                0x8C => {
                    if self.arg(a, 1) < self.arg(a, 0) {
                        self.set_arg(a, 0, self.arg(a, 1));
                    }
                }
                0x8D => {
                    if self.arg(a, 0) >= 0 {
                        self.gs.scan_type = self.arg(a, 0) & 0xFFFF;
                    }
                }
                0x8E => self.ins_instctrl(a),
                0x8F | 0x90 | 0x91 | 0x92 => self.ins_unknown(),
                _ => {
                    if opcode >= 0xE0 {
                        self.ins_mirp(a);
                    } else if opcode >= 0xC0 {
                        self.ins_mdrp(a);
                    } else if opcode >= 0xB8 {
                        self.ins_pushw(a);
                    } else if opcode >= 0xB0 {
                        self.ins_pushb(a);
                    } else {
                        self.ins_unknown();
                    }
                }
            }
            let mut suite = false;
            if self.error != 0 {
                if self.error == FT_ERR_INVALID_OPCODE {
                    let mut handled = false;
                    for def in 0..self.num_idefs as usize {
                        let d = self.idefs[def];
                        if d.active && self.opcode == d.opc as u8 {
                            if self.call_top >= self.call_size {
                                self.error = FT_ERR_INVALID_REFERENCE;
                                return self.error;
                            }
                            let top = self.call_top as usize;
                            self.call_stack[top] = TtCallRec { caller_range: self.cur_range, caller_ip: self.ip + 1, cur_count: 1, def_is_idef: true, def_index: def };
                            if !self.ins_goto_code_range(d.range, d.start) {
                                return self.error;
                            }
                            handled = true;
                            break;
                        }
                    }
                    if !handled {
                        return self.error;
                    }
                    suite = true;
                } else {
                    return self.error;
                }
            }
            if !suite {
                self.top = self.new_top;
                self.ip += self.length;
            }
            if self.ip >= self.code_size {
                if self.call_top > 0 {
                    self.error = FT_ERR_CODE_OVERFLOW;
                    return self.error;
                } else {
                    return FT_ERR_OK;
                }
            }
            if self.instruction_trap {
                return FT_ERR_OK;
            }
        }
    }

    /// `TT_Run_Context`.
    pub fn run_context(&mut self, size_gs: &TtGraphicsState) -> FtError {
        self.zp0 = ZoneId::Pts;
        self.zp1 = ZoneId::Pts;
        self.zp2 = ZoneId::Pts;
        let mut num_twilight_points: u32 = 30.max(2 * (self.pts.n_points as u32 + self.cvt_size));
        if self.twilight.n_points as u32 > num_twilight_points {
            if num_twilight_points > 0xFFFF {
                num_twilight_points = 0xFFFF;
            }
            self.twilight.n_points = num_twilight_points as u16;
        }
        self.loopcall_counter = 0;
        self.neg_jump_counter = 0;
        if self.pts.n_points != 0 {
            self.loopcall_counter_max = 50.max(10 * self.pts.n_points as u32) + 50.max(self.cvt_size / 10);
        } else {
            self.loopcall_counter_max = 300u32.wrapping_add(22u32.wrapping_mul(self.cvt_size));
        }
        if self.loopcall_counter_max > 100u32.wrapping_mul(self.num_glyphs as u32) {
            self.loopcall_counter_max = 100u32.wrapping_mul(self.num_glyphs as u32);
        }
        self.neg_jump_counter_max = self.loopcall_counter_max;
        self.stretched = self.metrics.x_ppem != self.metrics.y_ppem;
        self.gs = *size_gs;
        self.func_round = RoundFunc::ToGrid;
        self.compute_funcs();
        self.backward_compatibility &= !0x3;
        self.top = 0;
        self.call_top = 0;
        self.instruction_trap = false;
        self.run_ins()
    }
}

#[inline]
fn c_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

struct IupWorker<'a> {
    pts: &'a mut TtGlyphZone,
    yaxis: bool,
    max_points: u32,
}

impl<'a> IupWorker<'a> {
    #[inline]
    fn get(v: &FtVector, y: bool) -> i32 {
        if y {
            v.y
        } else {
            v.x
        }
    }

    #[inline]
    fn set(v: &mut FtVector, y: bool, val: i32) {
        if y {
            v.y = val;
        } else {
            v.x = val;
        }
    }

    fn shift(&mut self, p1: u32, p2: u32, p: u32) {
        let y = self.yaxis;
        let dx = sub_long(Self::get(&self.pts.cur[p as usize], y), Self::get(&self.pts.org[p as usize], y));
        if dx != 0 {
            for i in p1..p {
                let c = &mut self.pts.cur[i as usize];
                let v = add_long(Self::get(c, y), dx);
                Self::set(c, y, v);
            }
            for i in p + 1..=p2 {
                let c = &mut self.pts.cur[i as usize];
                let v = add_long(Self::get(c, y), dx);
                Self::set(c, y, v);
            }
        }
    }

    fn interpolate(&mut self, p1: u32, p2: u32, ref1: u32, ref2: u32) {
        let y = self.yaxis;
        if p1 > p2 {
            return;
        }
        if bounds(ref1, self.max_points) || bounds(ref2, self.max_points) {
            return;
        }
        let (mut ref1, mut ref2) = (ref1 as usize, ref2 as usize);
        let mut orus1 = Self::get(&self.pts.orus[ref1], y);
        let mut orus2 = Self::get(&self.pts.orus[ref2], y);
        if orus1 > orus2 {
            std::mem::swap(&mut orus1, &mut orus2);
            std::mem::swap(&mut ref1, &mut ref2);
        }
        let org1 = Self::get(&self.pts.org[ref1], y);
        let org2 = Self::get(&self.pts.org[ref2], y);
        let cur1 = Self::get(&self.pts.cur[ref1], y);
        let cur2 = Self::get(&self.pts.cur[ref2], y);
        let delta1 = sub_long(cur1, org1);
        let delta2 = sub_long(cur2, org2);
        if cur1 == cur2 || orus1 == orus2 {
            for i in p1..=p2 {
                let mut x = Self::get(&self.pts.org[i as usize], y);
                if x <= org1 {
                    x = add_long(x, delta1);
                } else if x >= org2 {
                    x = add_long(x, delta2);
                } else {
                    x = cur1;
                }
                Self::set(&mut self.pts.cur[i as usize], y, x);
            }
        } else {
            let mut scale: FtFixed = 0;
            let mut scale_valid = false;
            for i in p1..=p2 {
                let mut x = Self::get(&self.pts.org[i as usize], y);
                if x <= org1 {
                    x = add_long(x, delta1);
                } else if x >= org2 {
                    x = add_long(x, delta2);
                } else {
                    if !scale_valid {
                        scale_valid = true;
                        scale = ft_div_fix(sub_long(cur2, cur1), sub_long(orus2, orus1));
                    }
                    x = add_long(cur1, ft_mul_fix(sub_long(Self::get(&self.pts.orus[i as usize], y), orus1), scale));
                }
                Self::set(&mut self.pts.cur[i as usize], y, x);
            }
        }
    }
}
