//! Port of afe.jar `com.adobe.fontengine.font.cff.AutoColor`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::non_overlapping_outline_consumer::java_round_f64;
use crate::asc::flex2::fonts::adobe::fontengine::font::hinted_outline_consumer::HintedOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;

pub const AC_ALLOWEDIT: i32 = 1;
pub const AC_HINTSUB: i32 = 2;
pub const AC_FIXWINDING: i32 = 4;
pub const AC_GENERATEVSTEMS: i32 = 8;
pub const ZN_VSTEM: i32 = 0;
pub const ZN_HSTEM: i32 = 1;
pub const ZN_CHAREXTREME: i32 = 2;
pub const ZN_CHARZONE: i32 = 3;
pub const ZN_ZONE: i32 = 4;
const S_LINE: i16 = 0;
const S_BEND: i16 = 1;
const S_CURVE: i16 = 2;
const S_GHOST: i16 = 3;
const MOVETO: i16 = 0;
const LINETO: i16 = 1;
const CURVETO: i16 = 2;
const CLOSEPATH: i16 = 3;
const BOTGHST: i32 = -21;
const TOPGHST: i32 = -20;
const THETA: f64 = 0.38;
const BEND_TAN: i32 = 577;
const MAXBLUES: usize = 20;
const MAXSERIFS: usize = 5;
const MAXSTEMS: usize = 20;
const MAXFIXES: usize = 100;
const SFACTOR: i32 = 20;
const MAXSTEMDIST: i32 = 150;
const MAXF: i32 = 32768;
const MAXCNT: i32 = 100;
const WD_CCW: i32 = 0;
const WD_CW: i32 = 1;
const FRP_NZWIND: i32 = 0;
const FRP_YEXTREME: i32 = 1;
const FRP_CHKDT: i32 = 2;
const FRP_CHKBBDT: i32 = 3;
const FRP_FPBBOXPT: i32 = 4;
const MAX_NUM_SUBPATHS: i32 = 200;
const MAX_NUM_PATH_ELEMENTS: i32 = 1000;
const AC_GC_NEW_HINTS: i32 = 8;
const NIL: usize = usize::MAX;

const V_COLOR_LIST: [i16; 5] = [109, 77, 84, 8230, 0];
const H_COLOR_LIST: [i16; 5] = [8712, 8801, 8713, 247, 0];
const UPPER_SPECIAL_CHARS: [i16; 4] = [191, 161, 59, 0];
const LOWER_SPECIAL_CHARS: [i16; 4] = [63, 33, 58, 0];
const NO_BLUE_LIST: [i16; 6] = [64, 8226, 169, 164, 174, 0];

/// `ACException`.
struct ACException;

fn fix_int(n: i32) -> i32 {
    n << 8
}

fn f_rnd(n: i32) -> i32 {
    n.wrapping_add(128) & (0xFFFF_FF00u32 as i32)
}

fn f_half_rnd(n: i32) -> i32 {
    n.wrapping_add(64) & (0xFFFF_FF80u32 as i32)
}

fn f_trunc(n: i32) -> i32 {
    n >> 8
}

fn fix_half_mul(n: i32) -> i32 {
    n >> 1
}

fn fix_two_mul(n: i32) -> i32 {
    n << 1
}

fn tfmx(n: i32) -> i32 {
    fix_half_mul(n)
}

fn tfmy(n: i32) -> i32 {
    fix_half_mul(n)
}

fn itfmx(n: i32) -> i32 {
    fix_two_mul(n)
}

fn itfmy(n: i32) -> i32 {
    fix_two_mul(n)
}

fn dtfmx(n: i32) -> i32 {
    fix_half_mul(n)
}

fn dtfmy(n: i32) -> i32 {
    fix_half_mul(n)
}

fn idtfmx(n: i32) -> i32 {
    fix_two_mul(n)
}

fn idtfmy(n: i32) -> i32 {
    fix_two_mul(n)
}

fn ps_dist(n: i32) -> i32 {
    dtfmx(fix_int(n))
}

fn fix_to_flt(n: i32) -> f64 {
    n as f64 / 256.0
}

fn flt_to_fix(d: f64) -> i32 {
    if d >= 8388607.99609375 {
        return i32::MAX;
    }
    if d <= -8388608.0 {
        return i32::MIN;
    }
    (d * 256.0) as i32
}

fn prod_lt0(n: i32, n2: i32) -> bool {
    n < 0 && n2 > 0 || n > 0 && n2 < 0
}

fn prod_ge0(n: i32, n2: i32) -> bool {
    !prod_lt0(n, n2)
}

fn sh_abs(s: i16) -> i16 {
    if s < 0 { s.wrapping_neg() } else { s }
}

fn mdpt(n: i32, n2: i32) -> i16 {
    (n.wrapping_add(n2) >> 1) as i16
}

fn in_range(n: i32, n2: i32, n3: i32) -> bool {
    n >= n2 && n <= n3
}

fn test_high_low(n: i32, n2: i32) -> i32 {
    if n == n2 {
        return 0;
    }
    if n < n2 {
        return 1;
    }
    -1
}

fn interpolate(d: f64, d2: f64, d3: f64, d4: f64, d5: f64) -> f64 {
    d2 + (d - d3) * ((d4 - d2) / (d5 - d3))
}

fn mfabs(d: f64) -> f64 {
    if d > 0.0 { d } else { -d }
}

fn find_code_in_list(n: i32, list: &[i16]) -> bool {
    let mut i = 0;
    loop {
        let s = list[i];
        i += 1;
        if s == 0 {
            return false;
        }
        if n == s as i32 {
            return true;
        }
    }
}

fn special_char_type(n: i32) -> i32 {
    if find_code_in_list(n, &UPPER_SPECIAL_CHARS) {
        return 1;
    }
    if find_code_in_list(n, &LOWER_SPECIAL_CHARS) {
        return -1;
    }
    0
}

fn h_color_char(n: i32) -> bool {
    find_code_in_list(n, &H_COLOR_LIST)
}

fn v_color_char(n: i32) -> bool {
    find_code_in_list(n, &V_COLOR_LIST)
}

fn no_blue_char(n: i32) -> bool {
    find_code_in_list(n, &NO_BLUE_LIST)
}

fn move_to_new_clrs(n: i32) -> bool {
    n == 37 || n == 8240
}

fn is_in(n: i32, n2: i32) -> bool {
    n == -1 && n2 == -1
}

fn is_ok(n: i32, n2: i32) -> bool {
    n != 0 && n2 != 0
}

fn hv_ness(d: f64) -> i32 {
    let d2 = if d < 0.25 {
        interpolate(d, 1.0, 0.0, 0.841, 0.25)
    } else if d < 0.5 {
        interpolate(d, 0.841, 0.25, 0.707, 0.5)
    } else if d < 1.0 {
        interpolate(d, 0.707, 0.5, 0.5, 1.0)
    } else if d < 2.0 {
        interpolate(d, 0.5, 1.0, 0.25, 2.0)
    } else if d < 4.0 {
        interpolate(d, 0.25, 2.0, 0.0, 4.0)
    } else {
        0.0
    };
    flt_to_fix(d2)
}

fn vert_quo(n: i32, n2: i32, n3: i32, n4: i32) -> i32 {
    let mut n5 = n.wrapping_sub(n3);
    if n5 < 0 {
        n5 = n5.wrapping_neg();
    }
    if n5 == 0 {
        return 256;
    }
    let mut n6 = n2.wrapping_sub(n4);
    if n6 < 0 {
        n6 = n6.wrapping_neg();
    }
    if n6 == 0 {
        return 0;
    }
    let d = fix_to_flt(n5);
    let d2 = fix_to_flt(n6);
    let d3 = 2.0 * d * d / (THETA * d2);
    hv_ness(d3)
}

fn horz_quo(n: i32, n2: i32, n3: i32, n4: i32) -> i32 {
    let mut n5 = n2.wrapping_sub(n4);
    if n5 < 0 {
        n5 = n5.wrapping_neg();
    }
    if n5 == 0 {
        return 256;
    }
    let mut n6 = n.wrapping_sub(n3);
    if n6 < 0 {
        n6 = n6.wrapping_neg();
    }
    if n6 == 0 {
        return 0;
    }
    let d = fix_to_flt(n6);
    let d2 = fix_to_flt(n5);
    let d3 = 2.0 * d2 * d2 / (THETA * d);
    hv_ness(d3)
}

fn is_vertical(n: i32, n2: i32, n3: i32, n4: i32) -> bool {
    vert_quo(n, n2, n3, n4) > 0
}

fn is_horizontal(n: i32, n2: i32, n3: i32, n4: i32) -> bool {
    horz_quo(n, n2, n3, n4) > 0
}

fn test_bend(n: i32, n2: i32, n3: i32, n4: i32, n5: i32, n6: i32) -> bool {
    let d3 = fix_to_flt(n3.wrapping_sub(n));
    let d4 = fix_to_flt(n4.wrapping_sub(n2));
    let d5 = fix_to_flt(n5.wrapping_sub(n3));
    let d2 = fix_to_flt(n6.wrapping_sub(n4));
    let d6 = d3 * d5 + d4 * d2;
    let d = (d3 * d3 + d4 * d4) * (d5 * d5 + d2 * d2);
    d6 * d6 / d <= 0.5
}

fn test_tan(n: i32, n2: i32) -> bool {
    n.wrapping_abs() > n2.wrapping_abs().wrapping_mul(BEND_TAN) / 1000
}

fn f_round(n: i32) -> i32 {
    f_trunc(f_rnd(n))
}

fn is_ccw(n: i32, n2: i32, n3: i32, n4: i32, n5: i32, n6: i32) -> bool {
    let n7 = f_round(n3.wrapping_sub(n));
    let n8 = f_round(n4.wrapping_sub(n2));
    let n9 = f_round(n5.wrapping_sub(n3));
    let n10 = f_round(n6.wrapping_sub(n4));
    n7.wrapping_mul(n10) >= n9.wrapping_mul(n8)
}

fn adj_dist(n: i32, n2: i32) -> i32 {
    if n2 == 256 {
        return n;
    }
    f_trunc(n.wrapping_mul(n2))
}

fn tst_flat(mut n: i32, mut n2: i32) -> bool {
    if n < 0 {
        n = n.wrapping_neg();
    }
    if n2 < 0 {
        n2 = n2.wrapping_neg();
    }
    n2 >= ps_dist(50) && n <= ps_dist(4)
}

fn tst_same_dir(n: i32, n2: i32, n3: i32, n4: i32, n5: i32, n6: i32) -> bool {
    if prod_lt0(n2.wrapping_sub(n4), n4.wrapping_sub(n6)) || prod_lt0(n.wrapping_sub(n3), n3.wrapping_sub(n5)) {
        return false;
    }
    !test_bend(n, n2, n3, n4, n5, n6)
}

#[allow(clippy::too_many_arguments)]
fn pick_v_spot(n: i32, n2: i32, n3: i32, n4: i32, n5: i32, n6: i32, n7: i32, n8: i32, n9: i32, n10: i32, n11: i32, n12: i32) -> i32 {
    if n == n5 && n3 != n7 {
        return n;
    }
    if n != n5 && n3 == n7 {
        return n3;
    }
    if n == n9 && n3 != n11 {
        return n;
    }
    if n != n9 && n3 == n11 {
        return n3;
    }
    if n6.wrapping_sub(n2).wrapping_abs() > n8.wrapping_sub(n4).wrapping_abs() {
        return n;
    }
    if n8.wrapping_sub(n4).wrapping_abs() > n6.wrapping_sub(n2).wrapping_abs() {
        return n3;
    }
    if n == n9 && n3 == n11 {
        if n2.wrapping_sub(n10).wrapping_abs() > n4.wrapping_sub(n12).wrapping_abs() {
            return n;
        }
        return n3;
    }
    fix_half_mul(n.wrapping_add(n3))
}

fn prune_lt(n: i32, n2: i32) -> bool {
    if n2 < 0x0CCC_CCCC && n < 0x2AAA_AAAA {
        return n.wrapping_mul(3) < n2.wrapping_mul(10);
    }
    n / 10 < n2 / 3
}

fn prune_le(n: i32, n2: i32) -> bool {
    if n < 0x2AAA_AAAA {
        return n2 <= n.wrapping_mul(3);
    }
    n2 / 3 <= n
}

fn prune_gt(n: i32, n2: i32) -> bool {
    if n < 0x2AAA_AAAA {
        return n2 > n.wrapping_mul(3);
    }
    n2 / 3 > n
}

fn prune_much_gt(n: i32, n2: i32) -> bool {
    if n < 42949672 {
        return n2 > n.wrapping_mul(50);
    }
    n2 / 50 > n
}

fn prune_very_much_gt(n: i32, n2: i32) -> bool {
    if n < 21474836 {
        return n2 > n.wrapping_mul(100);
    }
    n2 / 100 > n
}

fn calc_overlap_dist(n: i32, n2: i32, n3: i32) -> i32 {
    let mut d = n as f64;
    let d2 = n2 as f64;
    let d3 = n3 as f64;
    d *= 1.0 + 0.4 * (1.0 - d2 / d3);
    d as i32
}

fn gap_dist(n: i32) -> i32 {
    if n < fix_int(127) {
        return f_trunc(n.wrapping_mul(n) / 20);
    }
    f_trunc(n.wrapping_mul(n / 20))
}

#[derive(Clone, Copy, Default, Debug)]
struct Cd {
    x: i32,
    y: i32,
}

#[derive(Clone)]
struct PathElt {
    elt_sn: i32,
    prev: usize,
    next: usize,
    typ: i16,
    hs: usize,
    vs: usize,
    hcopy: bool,
    vcopy: bool,
    is_flex: bool,
    count: i16,
    newcolors: i16,
    x: i32,
    y: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    x3: i32,
    y3: i32,
}

impl PathElt {
    fn new() -> PathElt {
        PathElt { elt_sn: 0, prev: NIL, next: NIL, typ: 0, hs: NIL, vs: NIL, hcopy: false, vcopy: false, is_flex: false, count: 0, newcolors: 0, x: 0, y: 0, x1: 0, y1: 0, x2: 0, y2: 0, x3: 0, y3: 0 }
    }
}

#[derive(Clone)]
struct ClrVal {
    cvl_sn: i32,
    v_nxt: usize,
    v_val: i32,
    v_spc: i32,
    init_val: i32,
    v_loc1: i32,
    v_loc2: i32,
    v_ghst: bool,
    pruned: bool,
    merge: bool,
    v_seg1: usize,
    v_seg2: usize,
    v_bst: usize,
}

impl ClrVal {
    fn new() -> ClrVal {
        ClrVal { cvl_sn: 0, v_nxt: NIL, v_val: 0, v_spc: 0, init_val: 0, v_loc1: 0, v_loc2: 0, v_ghst: false, pruned: false, merge: false, v_seg1: NIL, v_seg2: NIL, v_bst: NIL }
    }
}

struct SegLnkLst {
    next: usize,
    lnk: usize,
}

struct SegLnk {
    seg: usize,
}

#[derive(Clone)]
struct ClrSeg {
    csg_sn: i32,
    s_nxt: usize,
    s_loc: i32,
    s_max: i32,
    s_min: i32,
    s_bonus: i32,
    s_lnk: usize,
    s_elt: usize,
    s_type: i16,
}

impl ClrSeg {
    fn new() -> ClrSeg {
        ClrSeg { csg_sn: 0, s_nxt: NIL, s_loc: 0, s_max: 0, s_min: 0, s_bonus: 0, s_lnk: NIL, s_elt: NIL, s_type: 0 }
    }
}

#[allow(dead_code)]
struct ClrPoint {
    cpt_sn: i32,
    next: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    p0: usize,
    p1: usize,
    c: u8,
    done: bool,
}

#[derive(Clone, Copy)]
struct ACBBox {
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    v_mn: i32,
    v_mx: i32,
    h_mn: i32,
    h_mx: i32,
    pxmn: usize,
    pxmx: usize,
    pymn: usize,
    pymx: usize,
    pe: usize,
    pv_mn: usize,
    pv_mx: usize,
    ph_mn: usize,
    ph_mx: usize,
}

impl ACBBox {
    fn new() -> ACBBox {
        ACBBox { xmin: 0, ymin: 0, xmax: 0, ymax: 0, v_mn: 0, v_mx: 0, h_mn: 0, h_mx: 0, pxmn: NIL, pxmx: NIL, pymn: NIL, pymx: NIL, pe: NIL, pv_mn: NIL, pv_mx: NIL, ph_mn: NIL, ph_mx: NIL }
    }
}

struct SubPathInfo {
    head: usize,
    tail: usize,
    start_pt: Cd,
    end_pt: Cd,
    bbox: ACBBox,
    current_wind: i32,
    filled: bool,
}

#[derive(Default)]
struct FindExParam {
    y_max: i32,
    wind_dir: i32,
    prerise_pt: Cd,
    rise_pt: Cd,
    last_pt: Cd,
    first_pt: Cd,
    before_first_sink_pt: Cd,
    first_sink_pt: Cd,
    has_prerise_pt: bool,
    has_rise_pt: bool,
    has_last_pt: bool,
    last_was_extreme: bool,
    first_was_extreme: bool,
    has_rise_pt_for_first: bool,
    plaeau_ended: bool,
}

#[derive(Default)]
struct NZWindParam {
    wind_count: i32,
    testx: f64,
    testy: f64,
    test_pt: Cd,
    last_pt: Cd,
    first_pt: Cd,
    on_line: bool,
    from_above: bool,
    has_last_pt: bool,
    test_to_right: bool,
}

/// The `CheckData` fields `checkBBoxEdge` uses (the smoothing ones belong to the never enabled `checkSmooth`).
#[derive(Default)]
struct CheckData {
    bbquit: bool,
    vert: bool,
    started: bool,
    loc: i32,
    frst: i32,
    lst: i32,
}

/// `FltnRec` without `param`, which travels beside it.
#[derive(Default)]
struct FltnRec {
    limit: i16,
    feps: i32,
    report: i32,
    ll: Cd,
    ur: Cd,
    llx: i32,
    lly: i32,
}

/// The `Object param` handed to `flatReportProc`.
enum Param<'a> {
    NzWind(&'a mut NZWindParam),
    FindEx(&'a mut FindExParam),
    Check(&'a mut CheckData),
    BBox(&'a mut ACBBox),
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct HintData {
    flags: i32,
    v1: i32,
    v2: i32,
}

struct WriteData {
    first_flex: bool,
    wrt_color_info: bool,
    bst: usize,
    bch: u8,
    bx: i32,
    by: i32,
    bst_b: bool,
    hint_array: Option<Vec<HintData>>,
    prev_hint_array: Option<Vec<HintData>>,
}

#[allow(dead_code)]
pub struct AutoColor {
    m_path_start: usize,
    m_path_end: usize,
    m_orig_em_square: f64,
    m_generate_v_stems: bool,
    m_use_v: bool,
    m_use_h: bool,
    m_auto_v_fix: bool,
    m_auto_h_fix: bool,
    m_edit_char: bool,
    m_scalinghints: bool,
    m_extracolor: bool,
    m_fix_winding: bool,
    m_flex_strict: bool,
    m_flex_ok: bool,
    m_do_smoothing: bool,
    m_do_counters: bool,
    m_h_big_dist: i32,
    m_v_big_dist: i32,
    m_init_big_dist: i32,
    m_min_dist: i32,
    m_ghost_width: i32,
    m_ghost_length: i32,
    m_bend_length: i32,
    m_band_margin: i32,
    m_max_flare: i32,
    m_max_bend_merge: i32,
    m_max_merge: i32,
    m_min_color_element_length: i32,
    m_flex_cand: i32,
    m_bluefuzz: i32,
    m_unicode: i32,
    m_prune_a: i32,
    m_prune_b: i32,
    m_prune_c: i32,
    m_prune_d: i32,
    m_prune_value: i32,
    m_bonus: i32,
    m_h_big_dist_r: f64,
    m_v_big_dist_r: f64,
    m_max_val: f64,
    m_min_val: f64,
    m_dmin: i32,
    m_cp_percent: i32,
    m_hcoloring: usize,
    m_hprimary: usize,
    m_val_list: usize,
    m_vcoloring: usize,
    m_vprimary: usize,
    m_seg_lists: [usize; 4],
    m_point_list: usize,
    m_pt_lst_array: Vec<usize>,
    m_pt_lst_index: i32,
    m_num_pt_lsts: i32,
    m_max_pt_lsts: i32,
    m_top_bands: [i32; MAXBLUES],
    m_bot_bands: [i32; MAXBLUES],
    m_serifs: [i32; MAXSERIFS],
    m_len_top_bands: i32,
    m_len_bot_bands: i32,
    m_num_serifs: i32,
    m_v_stems: [i32; MAXSTEMS],
    m_h_stems: [i32; MAXSTEMS],
    m_num_v_stems: i32,
    m_num_h_stems: i32,
    m_do_aligns: bool,
    m_do_write_glyph: bool,
    m_counter_failed: bool,
    m_hlnks: usize,
    m_vlnks: usize,
    m_cp_from: i32,
    m_cp_to: i32,
    m_h_fix_ys: [i32; MAXFIXES],
    m_h_fix_d_ys: [i32; MAXFIXES],
    m_h_fix_count: i32,
    m_v_fix_xs: [i32; MAXFIXES],
    m_v_fix_d_xs: [i32; MAXFIXES],
    m_v_fix_count: i32,
    m_clr_b_box: bool,
    m_clr_h_bounds: bool,
    m_clr_v_bounds: bool,
    m_have_h_bnds: bool,
    m_merge_main: bool,
    m_have_v_bnds: bool,
    m_b_box: ACBBox,
    m_links: Option<Vec<u8>>,
    m_row_cnt: i32,
    m_sub_path_open: bool,
    m_input_path_start: usize,
    m_input_path_end: usize,
    m_glyph_width: f64,
    m_num_sub_paths: i32,
    m_sub_paths: Option<Vec<SubPathInfo>>,
    m_path_b_box: Option<ACBBox>,
    m_all_stems: bool,
    m_top_zones: Vec<i32>,
    m_bottom_zones: Vec<i32>,
    m_outline_consumer: Option<Rc<RefCell<dyn HintedOutlineConsumer>>>,
    m_elt_sn: i32,
    m_cpt_sn: i32,
    m_cvl_sn: i32,
    m_csg_sn: i32,
    m_cur_x: f64,
    m_cur_y: f64,
    m_zone_data: Vec<i32>,
    pe: Vec<PathElt>,
    cv: Vec<ClrVal>,
    cs: Vec<ClrSeg>,
    sll: Vec<SegLnkLst>,
    sl: Vec<SegLnk>,
    cp: Vec<ClrPoint>,
}

impl AutoColor {
    /// Java ctor order; `consumer` is null for `ZoneHint`'s instance, and null zone arrays are empty ones.
    #[allow(clippy::too_many_arguments)]
    pub fn new(consumer: Option<Rc<RefCell<dyn HintedOutlineConsumer>>>, units_per_em: f64, n: i32, b1: bool, b2: bool, top_zones: &[i32], bottom_zones: &[i32]) -> AutoColor {
        let mut a = AutoColor {
            m_path_start: NIL,
            m_path_end: NIL,
            m_orig_em_square: units_per_em,
            m_generate_v_stems: false,
            m_use_v: false,
            m_use_h: false,
            m_auto_v_fix: false,
            m_auto_h_fix: false,
            m_edit_char: false,
            m_scalinghints: false,
            m_extracolor: false,
            m_fix_winding: false,
            m_flex_strict: false,
            m_flex_ok: false,
            m_do_smoothing: false,
            m_do_counters: false,
            m_h_big_dist: 0,
            m_v_big_dist: 0,
            m_init_big_dist: 0,
            m_min_dist: 0,
            m_ghost_width: 0,
            m_ghost_length: 0,
            m_bend_length: 0,
            m_band_margin: 0,
            m_max_flare: 0,
            m_max_bend_merge: 0,
            m_max_merge: 0,
            m_min_color_element_length: 0,
            m_flex_cand: 0,
            m_bluefuzz: 0,
            m_unicode: 0,
            m_prune_a: 0,
            m_prune_b: 0,
            m_prune_c: 0,
            m_prune_d: 0,
            m_prune_value: 0,
            m_bonus: 0,
            m_h_big_dist_r: 0.0,
            m_v_big_dist_r: 0.0,
            m_max_val: 0.0,
            m_min_val: 0.0,
            m_dmin: 0,
            m_cp_percent: 0,
            m_hcoloring: NIL,
            m_hprimary: NIL,
            m_val_list: NIL,
            m_vcoloring: NIL,
            m_vprimary: NIL,
            m_seg_lists: [NIL; 4],
            m_point_list: NIL,
            m_pt_lst_array: Vec::new(),
            m_pt_lst_index: 0,
            m_num_pt_lsts: 0,
            m_max_pt_lsts: 0,
            m_top_bands: [0; MAXBLUES],
            m_bot_bands: [0; MAXBLUES],
            m_serifs: [0; MAXSERIFS],
            m_len_top_bands: 0,
            m_len_bot_bands: 0,
            m_num_serifs: 0,
            m_v_stems: [0; MAXSTEMS],
            m_h_stems: [0; MAXSTEMS],
            m_num_v_stems: 0,
            m_num_h_stems: 0,
            m_do_aligns: b2,
            m_do_write_glyph: b1,
            m_counter_failed: false,
            m_hlnks: NIL,
            m_vlnks: NIL,
            m_cp_from: 0,
            m_cp_to: 0,
            m_h_fix_ys: [0; MAXFIXES],
            m_h_fix_d_ys: [0; MAXFIXES],
            m_h_fix_count: 0,
            m_v_fix_xs: [0; MAXFIXES],
            m_v_fix_d_xs: [0; MAXFIXES],
            m_v_fix_count: 0,
            m_clr_b_box: false,
            m_clr_h_bounds: false,
            m_clr_v_bounds: false,
            m_have_h_bnds: false,
            m_merge_main: false,
            m_have_v_bnds: false,
            m_b_box: ACBBox::new(),
            m_links: None,
            m_row_cnt: 0,
            m_sub_path_open: false,
            m_input_path_start: NIL,
            m_input_path_end: NIL,
            m_glyph_width: 0.0,
            m_num_sub_paths: 0,
            m_sub_paths: None,
            m_path_b_box: None,
            m_all_stems: false,
            m_top_zones: top_zones.to_vec(),
            m_bottom_zones: bottom_zones.to_vec(),
            m_outline_consumer: consumer,
            m_elt_sn: 0,
            m_cpt_sn: 0,
            m_cvl_sn: 0,
            m_csg_sn: 0,
            m_cur_x: 0.0,
            m_cur_y: 0.0,
            m_zone_data: Vec::new(),
            pe: Vec::new(),
            cv: Vec::new(),
            cs: Vec::new(),
            sll: Vec::new(),
            sl: Vec::new(),
            cp: Vec::new(),
        };
        a.m_scalinghints = true;
        a.m_all_stems = true;
        a.m_auto_h_fix = false;
        a.m_auto_v_fix = false;
        a.m_sub_path_open = false;
        a.m_extracolor = (n & AC_HINTSUB) != 0;
        a.m_edit_char = (n & AC_ALLOWEDIT) != 0;
        a.m_fix_winding = (n & AC_FIXWINDING) != 0;
        a.m_generate_v_stems = (n & AC_GENERATEVSTEMS) != 0;
        a.copy_bands();
        a.init_all();
        a
    }

    pub fn new_glyph(&mut self, _gid: i32, width: f64, n: i32) {
        self.m_glyph_width = width;
        self.m_unicode = n;
        self.m_input_path_start = NIL;
        self.m_input_path_end = NIL;
        self.m_zone_data = Vec::new();
        // Nothing of an earlier glyph is read again once its input path is dropped.
        self.m_path_start = NIL;
        self.m_path_end = NIL;
        self.pe.clear();
        self.cv.clear();
        self.cs.clear();
        self.sll.clear();
        self.sl.clear();
        self.cp.clear();
    }

    pub fn report_zones(&self) -> Vec<i32> {
        self.m_zone_data.clone()
    }

    fn init_data(&mut self) {
        self.m_dmin = 50;
        self.m_init_big_dist = ps_dist(MAXSTEMDIST);
        self.m_min_dist = ps_dist(7);
        self.m_ghost_width = ps_dist(20);
        self.m_ghost_length = ps_dist(4);
        self.m_bend_length = ps_dist(2);
        self.m_prune_a = fix_int(50);
        self.m_prune_c = 100;
        self.m_prune_d = 256;
        let d = 10.24;
        self.m_prune_b = flt_to_fix(d);
        self.m_prune_value = self.m_prune_b;
        self.m_cp_percent = 40;
        self.m_band_margin = ps_dist(30);
        self.m_max_flare = ps_dist(10);
        self.m_max_bend_merge = ps_dist(6);
        self.m_max_merge = ps_dist(2);
        self.m_min_color_element_length = ps_dist(12);
        self.m_flex_cand = ps_dist(4);
        self.m_max_val = 8000000.0;
        self.m_min_val = 0.00390625;
        self.m_auto_v_fix = false;
        self.m_auto_h_fix = false;
        self.m_flex_ok = false;
        self.m_flex_strict = false;
        self.m_do_smoothing = false;
        self.m_do_counters = false;
        self.m_bluefuzz = (self.m_orig_em_square / 2000.0) as i32;
        self.m_point_list = NIL;
        self.m_max_pt_lsts = 5;
        self.m_pt_lst_array = vec![NIL; self.m_max_pt_lsts as usize];
        self.m_pt_lst_index = 0;
        self.m_pt_lst_array[0] = NIL;
        self.m_num_pt_lsts = 1;
        self.m_num_sub_paths = 0;
        self.m_sub_paths = None;
        self.m_path_b_box = None;
        self.m_elt_sn = 0;
        self.m_cpt_sn = 0;
        self.m_cvl_sn = 0;
        self.m_csg_sn = 0;
    }

    fn scale_abs(&self, n: i32) -> i32 {
        if !self.m_scalinghints {
            return n;
        }
        (1000.0 / self.m_orig_em_square * n as f64) as i32
    }

    fn un_scale_abs(&self, n: i32) -> i32 {
        if !self.m_scalinghints {
            return n;
        }
        let n2 = (self.m_orig_em_square / 1000.0 * n as f64) as i32;
        f_rnd(n2)
    }

    fn init_auto(&mut self) {
        self.m_have_h_bnds = false;
        self.m_clr_h_bounds = false;
        self.m_clr_b_box = false;
        self.m_have_v_bnds = false;
        self.m_clr_v_bounds = false;
    }

    fn get_sub_path_nxt(&self, p: usize) -> usize {
        if self.pe[p].typ == CLOSEPATH {
            return self.get_dest(p);
        }
        self.pe[p].next
    }

    fn get_sub_path_prv(&self, mut p: usize) -> usize {
        if self.pe[p].typ == MOVETO {
            p = self.get_closed_by(p);
        }
        self.pe[p].prev
    }

    fn find_closest_val(&self, mut v: usize, n: i32) -> usize {
        let mut n2 = fix_int(10000);
        let mut v2 = NIL;
        while v != NIL {
            let mut n4 = self.cv[v].v_loc1;
            let mut n3 = self.cv[v].v_loc2;
            if n4 > n3 {
                std::mem::swap(&mut n4, &mut n3);
            }
            if n >= n4 && n <= n3 {
                v2 = v;
                break;
            }
            let n6 = if n < n4 { n4.wrapping_sub(n) } else { n.wrapping_sub(n3) };
            if n6 < n2 {
                n2 = n6;
                v2 = v;
            }
            v = self.cv[v].v_nxt;
        }
        v2
    }

    fn cpy_h_clr(&mut self, p: usize) {
        let cd = self.get_end_point(p);
        let v = self.find_closest_val(self.m_hprimary, cd.y);
        if v != NIL {
            self.add_h_pair(v, b'b');
        }
    }

    fn cpy_v_clr(&mut self, p: usize) {
        let cd = self.get_end_point(p);
        let v = self.find_closest_val(self.m_vprimary, cd.x);
        if v != NIL {
            self.add_v_pair(v, b'y');
        }
    }

    fn hs_vs(&self, p: usize, bl: bool) -> usize {
        if bl { self.pe[p].hs } else { self.pe[p].vs }
    }

    fn set_hs_vs(&mut self, p: usize, bl: bool, l: usize) {
        if bl {
            self.pe[p].hs = l;
        } else {
            self.pe[p].vs = l;
        }
    }

    fn lst_seg(&self, l: usize) -> usize {
        self.sl[self.sll[l].lnk].seg
    }

    fn prune_color_segs(&mut self, p: usize, bl: bool) {
        let mut l = self.hs_vs(p, bl);
        let mut l2 = NIL;
        while l != NIL {
            let mut v = NIL;
            let lnk = self.sll[l].lnk;
            if lnk != NIL {
                let s = self.sl[lnk].seg;
                if s != NIL {
                    v = self.cs[s].s_lnk;
                }
            }
            let l3 = self.sll[l].next;
            if v == NIL {
                if l2 == NIL {
                    self.set_hs_vs(p, bl, l3);
                } else {
                    self.sll[l2].next = l3;
                }
                l = l3;
                continue;
            }
            l2 = l;
            l = l3;
        }
    }

    fn prune_element_color_segs(&mut self) {
        let mut p = self.m_path_start;
        while p != NIL {
            self.prune_color_segs(p, true);
            self.prune_color_segs(p, false);
            p = self.pe[p].next;
        }
    }

    fn elmnt_clr_seg_lst(&self, p: usize, bl: bool) -> usize {
        self.hs_vs(p, bl)
    }

    fn rem_lnk(&mut self, p: usize, bl: bool, l: usize) {
        let mut l2 = self.hs_vs(p, bl);
        let mut l3 = NIL;
        while l2 != NIL {
            let l4 = self.sll[l2].next;
            if l2 == l {
                if l3 == NIL {
                    self.set_hs_vs(p, bl, l4);
                } else {
                    self.sll[l3].next = l4;
                }
                return;
            }
            l3 = l2;
            l2 = l4;
        }
    }

    fn already_on_list(&self, v: usize, mut v2: usize) -> bool {
        while v2 != NIL {
            if v == v2 {
                return true;
            }
            v2 = self.cv[v2].v_nxt;
        }
        false
    }

    fn auto_v_seg(&mut self, v: usize) {
        self.add_v_pair(v, b'y');
    }

    fn auto_h_seg(&mut self, v: usize) {
        self.add_h_pair(v, b'b');
    }

    fn add_h_coloring(&mut self, v: usize) {
        if self.m_use_h || self.already_on_list(v, self.m_hcoloring) {
            return;
        }
        self.cv[v].v_nxt = self.m_hcoloring;
        self.m_hcoloring = v;
        self.auto_h_seg(v);
    }

    fn add_v_coloring(&mut self, v: usize) {
        if self.m_use_v || self.already_on_list(v, self.m_vcoloring) {
            return;
        }
        self.cv[v].v_nxt = self.m_vcoloring;
        self.m_vcoloring = v;
        self.auto_v_seg(v);
    }

    fn test_color(&self, s: usize, mut v: usize, bl: bool, bl2: bool) -> i32 {
        if s == NIL {
            return -1;
        }
        let v2 = self.cs[s].s_lnk;
        let mut n3 = self.cs[s].s_loc;
        if v2 == NIL {
            return -1;
        }
        let mut n2 = self.cv[v2].v_loc2;
        let n4 = n2;
        let mut n = self.cv[v2].v_loc1;
        let n5 = n;
        if self.cv[v2].v_ghst {
            if self.cs[self.cv[v2].v_seg1].s_type == S_GHOST {
                n = n2;
            } else {
                n2 = n;
            }
        }
        if self.cv[v2].v_ghst {
            let bl3;
            let mut v3 = v;
            if n3.wrapping_sub(n4).wrapping_abs() < n3.wrapping_sub(n5).wrapping_abs() {
                bl3 = false;
                n3 = n4;
            } else {
                bl3 = true;
                n3 = n5;
            }
            while v3 != NIL {
                if (if bl3 { self.cv[v3].v_loc1 } else { self.cv[v3].v_loc2 }) == n3 {
                    return -1;
                }
                v3 = self.cv[v3].v_nxt;
                if !bl2 {
                    break;
                }
            }
        }
        if bl {
            n2 = n2.wrapping_add(self.m_band_margin);
            n = n.wrapping_sub(self.m_band_margin);
        } else {
            n2 = n2.wrapping_sub(self.m_band_margin);
            n = n.wrapping_add(self.m_band_margin);
        }
        while v != NIL {
            let mut n6 = self.cv[v].v_loc2;
            let mut n7 = self.cv[v].v_loc1;
            if n5 == n7 && n4 == n6 {
                return -1;
            }
            if self.cv[v].v_ghst {
                if self.cs[self.cv[v].v_seg1].s_type == S_GHOST {
                    n7 = n6;
                } else {
                    n6 = n7;
                }
            }
            if bl && n7 <= n2 && n6 >= n || !bl && n7 >= n2 && n6 <= n {
                return 0;
            }
            v = self.cv[v].v_nxt;
            if bl2 {
                continue;
            }
            break;
        }
        1
    }

    fn test_color_lst(&self, mut l: usize, v: usize, bl: bool, bl2: bool) -> i32 {
        let mut n = -1;
        let mut n2 = 0;
        while l != NIL {
            let n3 = self.test_color(self.lst_seg(l), v, bl, bl2);
            if n3 == 0 {
                n = 0;
                break;
            }
            if n3 == 1 {
                n = 1;
            }
            l = self.sll[l].next;
            n2 += 1;
            if n2 <= MAXCNT {
                continue;
            }
            return 0;
        }
        n
    }

    /// `resolveConflictBySplit`, only reachable when `mEditChar` is set, which neither caller of this class does.
    fn resolve_conflict_by_split(&mut self, _p: usize, _bl: bool, _l: usize, _l2: usize) -> bool {
        unreachable!("AutoColor.resolveConflictBySplit needs AC_ALLOWEDIT")
    }

    fn rem_dup_lnks(&mut self, p: usize, bl: bool) {
        let mut l = self.hs_vs(p, bl);
        while l != NIL {
            let mut l3 = self.sll[l].next;
            while l3 != NIL {
                let l4 = self.sll[l3].next;
                if self.lst_seg(l) == self.lst_seg(l3) {
                    self.rem_lnk(p, bl, l3);
                }
                l3 = l4;
            }
            l = self.sll[l].next;
        }
    }

    fn ok_to_rem_lnk(&self, n: i32, bl: bool, n2: i32) -> bool {
        if !bl || n2 == 0 {
            return true;
        }
        if self.in_blue_band(n, self.m_len_top_bands, &self.m_top_bands) {
            return false;
        }
        !self.in_blue_band(n, self.m_len_bot_bands, &self.m_bot_bands)
    }

    fn try_resolve_conflict(&mut self, p: usize, bl: bool) -> bool {
        self.rem_dup_lnks(p, bl);
        let s = self.pe[p].typ;
        let (cd, cd2) = if s == MOVETO {
            self.get_end_points(self.get_closed_by(p))
        } else if s == CURVETO {
            (Cd { x: self.pe[p].x1, y: self.pe[p].y1 }, Cd { x: self.pe[p].x3, y: self.pe[p].y3 })
        } else {
            self.get_end_points(p)
        };
        let n3 = if bl { cd.y } else { cd.x };
        let n4 = if bl { cd2.y } else { cd2.x };
        let mut l = self.hs_vs(p, bl);
        let mut seg = self.lst_seg(l);
        let n5 = self.cs[seg].s_loc;
        let mut l2 = l;
        l = self.sll[l].next;
        let mut seg2 = self.lst_seg(l);
        let n6 = self.cs[seg2].s_loc;
        let mut l3 = l;
        if n5 != n3 && n6 != n4 && (n5.wrapping_sub(n3).wrapping_abs() > n5.wrapping_sub(n4).wrapping_abs() || n6.wrapping_sub(n4).wrapping_abs() > n6.wrapping_sub(n3).wrapping_abs()) {
            std::mem::swap(&mut seg, &mut seg2);
            l = l2;
            l2 = l3;
            l3 = l;
        }
        let v = self.cs[seg].s_lnk;
        let v2 = self.cs[seg2].s_lnk;
        if self.cv[v].v_val < fix_int(50) && self.ok_to_rem_lnk(n3, bl, self.cv[v].v_spc) {
            self.rem_lnk(p, bl, l2);
            return true;
        }
        if self.cv[v2].v_val < fix_int(50) && self.cv[v].v_val > self.cv[v2].v_val.wrapping_mul(SFACTOR) && self.ok_to_rem_lnk(n4, bl, self.cv[v2].v_spc) {
            self.rem_lnk(p, bl, l3);
            return true;
        }
        if s != CURVETO || (bl && is_horizontal(cd.x, cd.y, cd2.x, cd2.y) || !bl && is_vertical(cd.x, cd.y, cd2.x, cd2.y)) && self.ok_to_rem_lnk(n3, bl, self.cv[v].v_spc) {
            self.rem_lnk(p, bl, l2);
            return true;
        }
        let (cd, _) = self.get_end_points(self.get_sub_path_prv(p));
        let n2 = if bl { cd.y } else { cd.x };
        if prod_lt0(n4.wrapping_sub(n3), n2.wrapping_sub(n3)) {
            self.rem_lnk(p, bl, l2);
            return true;
        }
        let cd2 = self.get_end_point(self.get_sub_path_nxt(p));
        let n = if bl { cd2.y } else { cd2.x };
        if prod_lt0(n.wrapping_sub(n4), n3.wrapping_sub(n4)) {
            self.rem_lnk(p, bl, l3);
            return true;
        }
        if (n4 == self.cv[v2].v_loc1 || n4 == self.cv[v2].v_loc2) && n3 != self.cv[v].v_loc1 && n3 != self.cv[v].v_loc2 {
            self.rem_lnk(p, bl, l2);
            return true;
        }
        if (n3 == self.cv[v].v_loc1 || n3 == self.cv[v].v_loc2) && n4 != self.cv[v2].v_loc1 && n4 != self.cv[v2].v_loc2 {
            self.rem_lnk(p, bl, l3);
            return true;
        }
        self.m_edit_char && self.resolve_conflict_by_split(p, bl, l2, l3)
    }

    fn check_color_segs(&mut self, p: usize, bl: bool, bl2: bool) -> bool {
        let mut l = self.hs_vs(p, bl2);
        while l != NIL {
            let l3 = self.sll[l].next;
            if l3 != NIL {
                let v = self.cs[self.lst_seg(l)].s_lnk;
                if v != NIL && self.test_color_lst(l3, v, bl, false) == 0 {
                    if self.try_resolve_conflict(p, bl2) {
                        return self.check_color_segs(p, bl, bl2);
                    }
                    self.set_hs_vs(p, bl2, NIL);
                    return true;
                }
            }
            l = l3;
        }
        false
    }

    fn check_elmnt_clr_segs(&mut self) {
        let mut p = self.m_path_start;
        while p != NIL {
            if !self.check_color_segs(p, true, true) {
                self.check_color_segs(p, true, false);
            }
            p = self.pe[p].next;
        }
    }

    fn clr_lsts_clash(&self, mut l: usize, l2: usize, bl: bool) -> bool {
        while l != NIL {
            let v = self.cs[self.lst_seg(l)].s_lnk;
            if v != NIL {
                let mut l3 = l2;
                while l3 != NIL {
                    if self.test_color_lst(l3, v, bl, false) == 0 {
                        return true;
                    }
                    l3 = self.sll[l3].next;
                }
            }
            l = self.sll[l].next;
        }
        false
    }

    fn best_from_lsts(&self, l: usize, l2: usize) -> usize {
        let mut l3 = NIL;
        let mut n = 0;
        for i in 0..2 {
            let mut l4 = if i != 0 { l } else { l2 };
            while l4 != NIL {
                let v = self.cs[self.lst_seg(l4)].s_lnk;
                if v != NIL && self.cv[v].v_val > n {
                    l3 = l4;
                    n = self.cv[v].v_val;
                }
                l4 = self.sll[l4].next;
            }
        }
        l3
    }

    fn new_sll(&mut self, next: usize, lnk: usize) -> usize {
        self.sll.push(SegLnkLst { next, lnk });
        self.sll.len() - 1
    }

    fn clrs_clash(&mut self, p: usize, p2: usize, a: &mut usize, a2: &mut usize, a3: &mut usize, a4: &mut usize) -> bool {
        let mut bl = false;
        if self.clr_lsts_clash(*a, *a3, true) {
            bl = true;
            let l2 = self.best_from_lsts(*a, *a3);
            let l = if l2 != NIL {
                let lnk = self.sll[l2].lnk;
                self.new_sll(NIL, lnk)
            } else {
                NIL
            };
            *a3 = l;
            *a = l;
            self.pe[p2].hs = *a3;
            self.pe[p].hs = *a3;
        }
        if self.clr_lsts_clash(*a2, *a4, true) {
            bl = true;
            let l2 = self.best_from_lsts(*a2, *a4);
            let l = if l2 != NIL {
                let lnk = self.sll[l2].lnk;
                self.new_sll(NIL, lnk)
            } else {
                NIL
            };
            *a4 = l;
            *a2 = l;
            self.pe[p2].vs = *a4;
            self.pe[p].vs = *a4;
        }
        bl
    }

    /// `getColorLsts`, returning (H list, V list, H test, V test).
    fn get_color_lsts(&self, p: usize) -> (usize, usize, i32, i32) {
        let (l2, n2) = if self.m_use_h {
            (NIL, -1)
        } else {
            let l2 = self.pe[p].hs;
            (l2, if l2 == NIL { -1 } else { self.test_color_lst(l2, self.m_hcoloring, true, true) })
        };
        let (l, n) = if self.m_use_v {
            (NIL, -1)
        } else {
            let l = self.pe[p].vs;
            (l, if l == NIL { -1 } else { self.test_color_lst(l, self.m_vcoloring, true, true) })
        };
        (l2, l, n2, n)
    }

    fn re_clr_bounds(&mut self, p: usize) {
        if !self.m_use_h {
            if self.m_clr_h_bounds && self.m_hcoloring == NIL && !self.m_have_h_bnds {
                let b = self.m_b_box;
                self.re_clr_h_bnds(&b);
            } else if !self.m_clr_b_box {
                if self.m_hcoloring == NIL {
                    self.cpy_h_clr(p);
                }
                if self.m_merge_main {
                    self.merge_from_main_colors(b'b');
                }
            }
        }
        if !self.m_use_v {
            if self.m_clr_v_bounds && self.m_vcoloring == NIL && !self.m_have_v_bnds {
                let b = self.m_b_box;
                self.re_clr_v_bnds(&b);
            } else if !self.m_clr_b_box {
                if self.m_vcoloring == NIL {
                    self.cpy_v_clr(p);
                }
                if self.m_merge_main {
                    self.merge_from_main_colors(b'y');
                }
            }
        }
    }

    fn add_color_lst(&mut self, mut l: usize, bl: bool) {
        while l != NIL {
            let v = self.cs[self.lst_seg(l)].s_lnk;
            if bl {
                self.add_v_coloring(v);
            } else {
                self.add_h_coloring(v);
            }
            l = self.sll[l].next;
        }
    }

    fn start_new_coloring(&mut self, p: usize, l: usize, l2: usize) {
        self.re_clr_bounds(p);
        self.xtra_clrs(p);
        self.m_clr_b_box = false;
        if self.m_do_counters && self.m_use_v {
            self.copy_main_v();
        }
        self.m_vcoloring = NIL;
        if self.m_do_counters && self.m_use_h {
            self.copy_main_h();
        }
        self.m_hcoloring = NIL;
        if !self.m_use_h {
            self.add_color_lst(l, false);
        }
        if !self.m_use_v {
            self.add_color_lst(l2, true);
        }
    }

    fn add_if_need_v(&mut self, n: i32, l: usize) {
        if !self.m_use_v && n == 1 {
            self.add_color_lst(l, true);
        }
    }

    fn add_if_need_h(&mut self, n: i32, l: usize) {
        if !self.m_use_h && n == 1 {
            self.add_color_lst(l, false);
        }
    }

    fn set_h_colors(&mut self, mut v: usize) {
        if self.m_use_h {
            return;
        }
        self.m_hcoloring = v;
        while v != NIL {
            self.auto_h_seg(v);
            v = self.cv[v].v_nxt;
        }
    }

    fn set_v_colors(&mut self, mut v: usize) {
        if self.m_use_v {
            return;
        }
        self.m_vcoloring = v;
        while v != NIL {
            self.auto_v_seg(v);
            v = self.cv[v].v_nxt;
        }
    }

    fn copy_clrs(&mut self, mut v: usize) -> usize {
        let mut v2 = NIL;
        let mut n = 0;
        while v != NIL {
            let mut c = self.cv[v].clone();
            c.v_nxt = v2;
            self.cv.push(c);
            v2 = self.cv.len() - 1;
            n += 1;
            if n > MAXCNT {
                return v2;
            }
            v = self.cv[v].v_nxt;
        }
        v2
    }

    fn is_flare(&self, n: i32, mut p: usize, p2: usize, bl: bool) -> bool {
        while p != p2 {
            let cd = self.get_end_point(p);
            if bl && cd.y.wrapping_sub(n).wrapping_abs() > self.m_max_flare || !bl && cd.x.wrapping_sub(n).wrapping_abs() > self.m_max_flare {
                return false;
            }
            p = self.get_sub_path_nxt(p);
        }
        true
    }

    fn is_top_seg_of_val(n: i32, n2: i32, n3: i32) -> bool {
        let n4 = n2.wrapping_sub(n);
        let n5 = n3.wrapping_sub(n);
        n4.wrapping_abs() <= n5.wrapping_abs()
    }

    fn rem_flare_lnk(&mut self, p: usize, bl: bool, l: usize, _p2: usize, _n: i32) {
        self.rem_lnk(p, bl, l);
    }

    fn compare_values(&self, v: usize, v2: usize, n: i32, n2: i32) -> bool {
        let mut n4 = self.cv[v].v_val;
        let mut n3 = self.cv[v2].v_val;
        let mut n5 = if n4 > n3 { n4 } else { n3 };
        n5 <<= 1;
        while n5 > 0 {
            n5 <<= 1;
            n4 <<= 1;
            n3 <<= 1;
        }
        if n2 > 0 && self.cv[v].v_ghst != self.cv[v2].v_ghst {
            if self.cv[v].v_ghst {
                n4 >>= n2;
            }
            if self.cv[v2].v_ghst {
                n3 >>= n2;
            }
        }
        let (s1, s2) = (self.cv[v].v_spc, self.cv[v2].v_spc);
        if s1 > 0 && s2 > 0 || s1 == 0 && s2 == 0 {
            return n4 > n3;
        }
        if s1 > 0 {
            return if n4 < i32::MAX / n { n4.wrapping_mul(n) > n3 } else { n4 > n3 / n };
        }
        if n3 < i32::MAX / n { n4 > n3.wrapping_mul(n) } else { n4 / n > n3 }
    }

    fn rem_flares(&mut self, bl: bool) {
        let (bl3, bl2) = if bl { (true, false) } else { (false, true) };
        let mut p = self.m_path_start;
        while p != NIL {
            if if bl3 { self.pe[p].hs == NIL } else { self.pe[p].vs == NIL } {
                p = self.pe[p].next;
                continue;
            }
            let mut p2 = self.get_sub_path_nxt(p);
            let mut bl4 = false;
            while p2 != p && !bl4 {
                if if bl3 { self.pe[p2].hs != NIL } else { self.pe[p2].vs != NIL } {
                    let mut l = self.elmnt_clr_seg_lst(p, bl3);
                    while l != NIL {
                        let s = self.lst_seg(l);
                        let l2 = self.sll[l].next;
                        let mut l3 = self.elmnt_clr_seg_lst(p2, bl3);
                        while l3 != NIL {
                            let s2 = self.lst_seg(l3);
                            let l4 = self.sll[l3].next;
                            if s != NIL && s2 != NIL {
                                let n = self.cs[s].s_loc.wrapping_sub(self.cs[s2].s_loc);
                                if n.wrapping_abs() > self.m_max_flare || !self.is_flare(self.cs[s].s_loc, p, p2, bl) {
                                    bl4 = true;
                                    l3 = l4;
                                    continue;
                                }
                                let v = self.cs[s].s_lnk;
                                let v2 = self.cs[s2].s_lnk;
                                if n != 0 && Self::is_top_seg_of_val(self.cs[s].s_loc, self.cv[v].v_loc2, self.cv[v].v_loc1) == Self::is_top_seg_of_val(self.cs[s2].s_loc, self.cv[v2].v_loc2, self.cv[v2].v_loc1) {
                                    if self.compare_values(v, v2, 1000, 0) {
                                        if self.cv[v2].v_spc == 0 && self.cv[v2].v_val < fix_int(1000) {
                                            self.rem_flare_lnk(p2, bl3, l3, p, 1);
                                        }
                                    } else if self.cv[v].v_spc == 0 && self.cv[v].v_val < fix_int(1000) {
                                        self.rem_flare_lnk(p, bl3, l, p2, 2);
                                        break;
                                    }
                                }
                            }
                            l3 = l4;
                        }
                        l = l2;
                    }
                }
                if if bl2 { self.pe[p2].hs != NIL } else { self.pe[p2].vs != NIL } {
                    break;
                }
                p2 = self.get_sub_path_nxt(p2);
            }
            p = self.pe[p].next;
        }
    }

    fn carry_if_need(&mut self, n: i32, bl: bool, mut v: usize) {
        if bl && self.m_use_v || !bl && self.m_use_h {
            return;
        }
        let mut n2 = fix_half_mul(self.m_band_margin);
        if n2 > fix_int(10) {
            n2 = fix_int(10);
        }
        while v != NIL {
            let mut s = self.cv[v].v_seg1;
            if self.cv[v].v_ghst && self.cs[s].s_type == S_GHOST {
                s = self.cv[v].v_seg2;
            }
            if s != NIL {
                let mut n4 = self.cv[v].v_loc2;
                let mut n3 = self.cv[v].v_loc1;
                if n4 > n3 {
                    std::mem::swap(&mut n4, &mut n3);
                }
                n4 = n4.wrapping_sub(n2);
                n3 = n3.wrapping_add(n2);
                if n > n4 && n < n3 {
                    let v2 = self.cs[s].s_lnk;
                    self.cs[s].s_lnk = v;
                    if bl {
                        if self.test_color(s, self.m_vcoloring, true, true) == 1 {
                            self.add_v_coloring(v);
                            self.cs[s].s_lnk = v2;
                            break;
                        }
                    } else if self.test_color(s, self.m_hcoloring, true, true) == 1 {
                        self.add_h_coloring(v);
                        self.cs[s].s_lnk = v2;
                        break;
                    }
                    self.cs[s].s_lnk = v2;
                }
            }
            v = self.cv[v].v_nxt;
        }
    }

    fn pro_clrs(&mut self, p: usize, bl: bool, n: i32) {
        let l = self.elmnt_clr_seg_lst(p, bl);
        if l == NIL {
            return;
        }
        if if bl { self.pe[p].hcopy } else { self.pe[p].vcopy } {
            return;
        }
        let mut p2 = p;
        loop {
            p2 = self.get_sub_path_prv(p2);
            if self.elmnt_clr_seg_lst(p2, bl) != NIL {
                break;
            }
            let cd = self.get_end_point(p2);
            let n2 = (if bl { cd.y } else { cd.x }).wrapping_sub(n);
            if n2.wrapping_abs() > fix_int(50) {
                return;
            }
            if bl {
                self.pe[p2].hs = l;
                self.pe[p2].hcopy = true;
                continue;
            }
            self.pe[p2].vs = l;
            self.pe[p2].vcopy = true;
        }
    }

    fn promote_colors(&mut self) {
        let mut p = self.m_path_start;
        while p != NIL {
            let cd = self.get_end_point(p);
            self.pro_clrs(p, true, cd.y);
            self.pro_clrs(p, false, cd.x);
            p = self.pe[p].next;
        }
    }

    fn rem_promoted_clrs(&mut self) {
        let mut p = self.m_path_start;
        while p != NIL {
            if self.pe[p].hcopy {
                self.pe[p].hs = NIL;
                self.pe[p].hcopy = false;
            }
            if self.pe[p].vcopy {
                self.pe[p].vs = NIL;
                self.pe[p].vcopy = false;
            }
            p = self.pe[p].next;
        }
    }

    fn rem_short_colors(&mut self) {
        let mut cd = Cd::default();
        let mut p = self.m_path_start;
        while p != NIL {
            let cd2 = self.get_end_point(p);
            if cd.x.wrapping_sub(cd2.x).wrapping_abs() < self.m_min_color_element_length && cd.y.wrapping_sub(cd2.y).wrapping_abs() < self.m_min_color_element_length {
                self.pe[p].hs = NIL;
                self.pe[p].vs = NIL;
            }
            p = self.pe[p].next;
            cd.x = cd2.x;
            cd.y = cd2.y;
        }
    }

    fn auto_extra_colors(&mut self, bl: bool) {
        let (mut a_n, mut a_n2, mut a_n3, mut a_n4): (i32, i32, i32, i32);
        let (mut a_l, mut a_l2, mut a_l3, mut a_l4): (usize, usize, usize, usize);
        let mut bl2 = true;
        self.m_clr_h_bounds = false;
        self.m_clr_v_bounds = false;
        self.m_clr_b_box = false;
        self.m_merge_main = self.count_sub_paths().0 <= 5;
        let mut p = self.m_path_start;
        self.rem_flares(true);
        self.rem_flares(false);
        self.check_elmnt_clr_segs();
        self.promote_colors();
        self.rem_short_colors();
        self.m_have_v_bnds = self.m_clr_v_bounds;
        self.m_have_h_bnds = self.m_clr_h_bounds;
        let mut p2 = NIL;
        let mut bl3 = false;
        let mut v = NIL;
        let mut v2 = NIL;
        while p != NIL {
            let s = self.pe[p].typ;
            if bl && s == MOVETO {
                self.start_new_coloring(p, NIL, NIL);
                bl3 = false;
            }
            if bl2 && p == p2 {
                self.start_new_coloring(p, NIL, NIL);
                self.set_h_colors(v2);
                if self.m_generate_v_stems {
                    self.set_v_colors(v);
                }
                bl3 = true;
            }
            (a_l, a_l2, a_n, a_n2) = self.get_color_lsts(p);
            let p3 = if s == MOVETO { self.get_closed_by(p) } else { NIL };
            if s == MOVETO && self.is_short(p3) {
                p2 = self.pe[p3].prev;
                (a_l3, a_l4, a_n3, a_n4) = self.get_color_lsts(p2);
                if self.clrs_clash(p, p2, &mut a_l, &mut a_l2, &mut a_l3, &mut a_l4) {
                    (a_l, a_l2, a_n, a_n2) = self.get_color_lsts(p);
                    (a_l3, a_l4, a_n3, a_n4) = self.get_color_lsts(p2);
                }
                let bl4 = if bl3 { !is_in(a_n3, a_n4) || !is_in(a_n, a_n2) } else { !is_ok(a_n3, a_n4) || !is_ok(a_n, a_n2) };
                if bl4 {
                    self.start_new_coloring(p, a_l, a_l2);
                    bl3 = false;
                    a_n4 = 1;
                    a_n3 = 1;
                } else {
                    self.add_if_need_h(a_n, a_l);
                    self.add_if_need_v(a_n2, a_l2);
                }
                self.add_if_need_h(a_n3, a_l3);
                self.add_if_need_v(a_n4, a_l4);
                bl2 = false;
            } else {
                let bl4 = if bl3 { !is_in(a_n, a_n2) } else { !is_ok(a_n, a_n2) };
                if bl4 {
                    if s == CLOSEPATH {
                        p = self.pe[p].prev;
                        (a_l, a_l2, _, _) = self.get_color_lsts(p);
                    }
                    let mut v3 = NIL;
                    let v4 = self.copy_clrs(self.m_hcoloring);
                    if self.m_generate_v_stems {
                        v3 = self.copy_clrs(self.m_vcoloring);
                    }
                    if !bl2 {
                        bl2 = true;
                        if self.m_generate_v_stems {
                            v = self.copy_clrs(v3);
                        }
                        v2 = self.copy_clrs(v4);
                    }
                    self.start_new_coloring(p, a_l, a_l2);
                    bl3 = false;
                    let cd = if s == CURVETO { Cd { x: self.pe[p].x1, y: self.pe[p].y1 } } else { self.get_end_point(p) };
                    self.carry_if_need(cd.y, false, v4);
                    if self.m_generate_v_stems {
                        self.carry_if_need(cd.x, true, v3);
                    }
                } else {
                    self.add_if_need_h(a_n, a_l);
                    self.add_if_need_v(a_n2, a_l2);
                }
            }
            p = self.pe[p].next;
        }
        let e = self.m_path_end;
        self.re_clr_bounds(e);
        self.rem_promoted_clrs();
    }

    fn init_gen(&mut self) {
        for i in 0..4 {
            self.m_seg_lists[i] = NIL;
        }
        self.m_hlnks = NIL;
        self.m_vlnks = NIL;
    }

    fn link_segment(&mut self, p: usize, bl: bool, s: usize) {
        self.sl.push(SegLnk { seg: s });
        let lnk = self.sl.len() - 1;
        if bl {
            let l = self.new_sll(self.pe[p].hs, lnk);
            self.pe[p].hs = l;
            let l2 = self.new_sll(self.m_hlnks, lnk);
            self.m_hlnks = l2;
        } else {
            let l = self.new_sll(self.pe[p].vs, lnk);
            self.pe[p].vs = l;
            let l2 = self.new_sll(self.m_vlnks, lnk);
            self.m_vlnks = l2;
        }
    }

    fn copy_segment_link(&mut self, p: usize, p2: usize, bl: bool) {
        if bl {
            let lnk = self.sll[self.pe[p].hs].lnk;
            let l = self.new_sll(self.pe[p2].hs, lnk);
            self.pe[p2].hs = l;
        } else {
            let lnk = self.sll[self.pe[p].vs].lnk;
            let l = self.new_sll(self.pe[p2].vs, lnk);
            self.pe[p2].vs = l;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_segment(&mut self, n: i32, n2: i32, n3: i32, n4: usize, n5: usize, mut p: usize, mut p2: usize, bl: bool, n6: i32) {
        let mut seg = ClrSeg::new();
        self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
        seg.csg_sn = self.m_csg_sn;
        if seg.csg_sn == 0 {
            self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
        }
        seg.s_loc = n3;
        if n > n2 {
            seg.s_max = n;
            seg.s_min = n2;
        } else {
            seg.s_max = n2;
            seg.s_min = n;
        }
        seg.s_bonus = self.m_bonus;
        seg.s_type = n6 as i16;
        self.cs.push(seg);
        let s = self.cs.len() - 1;
        if p != NIL {
            if self.pe[p].typ == CLOSEPATH {
                p = self.get_dest(p);
            }
            self.link_segment(p, bl, s);
            self.cs[s].s_elt = p;
        }
        if p2 != NIL {
            if self.pe[p2].typ == CLOSEPATH {
                p2 = self.get_dest(p2);
            }
            self.copy_segment_link(p, p2, bl);
            if p == NIL || p2 == self.pe[p].prev {
                self.cs[s].s_elt = p2;
            }
        }
        let n7 = if n > n2 { n4 } else { n5 };
        let mut s2 = self.m_seg_lists[n7];
        let mut s3 = NIL;
        loop {
            if s2 == NIL {
                if s3 == NIL {
                    self.m_seg_lists[n7] = s;
                    break;
                }
                self.cs[s3].s_nxt = s;
                break;
            }
            if self.cs[s2].s_loc >= n3 {
                if s3 == NIL {
                    self.m_seg_lists[n7] = s;
                } else {
                    self.cs[s3].s_nxt = s;
                }
                self.cs[s].s_nxt = s2;
                break;
            }
            s3 = s2;
            s2 = self.cs[s2].s_nxt;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_v_segment(&mut self, n: i32, n2: i32, n3: i32, p: usize, p2: usize, n4: i16, _n5: i32) {
        self.add_segment(n, n2, n3, 0, 1, p, p2, false, n4 as i32);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_h_segment(&mut self, n: i32, n2: i32, n3: i32, p: usize, p2: usize, n4: i16, _n5: i32) {
        self.add_segment(n, n2, n3, 2, 3, p, p2, true, n4 as i32);
    }

    fn cp_from(&self, n: i32, n2: i32) -> i32 {
        n2.wrapping_sub(n).wrapping_mul(self.m_cp_from) / 100 + n
    }

    fn cp_to(&self, n: i32, n2: i32) -> i32 {
        n2.wrapping_sub(n).wrapping_mul(self.m_cp_to) / 100 + n
    }

    fn do_h_bends_nxt(&mut self, n: i32, n2: i32, n3: i32, n4: i32, p: usize) {
        if n2 == n4 {
            return;
        }
        let (_, cd, _) = self.nxt_for_bend(p);
        let bl = prod_lt0(cd.y.wrapping_sub(n4), n4.wrapping_sub(n2));
        if bl || test_tan(n3.wrapping_sub(cd.x), n4.wrapping_sub(cd.y)) && (prod_lt0(cd.x.wrapping_sub(n3), n3.wrapping_sub(n)) || is_vertical(n, n2, n3, n4) && test_bend(n, n2, n3, n4, cd.x, cd.y)) {
            let mut n5 = fix_half_mul(self.m_bend_length);
            let mut bl2 = false;
            if !(n <= n3 && n3 < cd.x || n < n3 && n3 <= cd.x) {
                if cd.x < n3 && n3 <= n || cd.x <= n3 && n3 < n {
                    n5 = -n5;
                } else if bl {
                    let bl4 = n2 > n4;
                    if bl4 != is_ccw(n, n2, n3, n4, cd.x, cd.y) {
                        n5 = -n5;
                    }
                } else {
                    bl2 = true;
                }
            }
            let n6 = n3.wrapping_sub(n5);
            let n7 = n3.wrapping_add(n5);
            self.add_h_segment(n6, n7, n4, p, NIL, S_BEND, 0);
            if bl2 {
                self.add_h_segment(n7, n6, n4, p, NIL, S_BEND, 1);
            }
        }
    }

    fn do_h_bends_prv(&mut self, n: i32, n2: i32, n3: i32, n4: i32, p: usize) {
        if n2 == n4 {
            return;
        }
        let (_, cd) = self.prv_for_bend(p);
        let bl = prod_lt0(cd.y.wrapping_sub(n2), n2.wrapping_sub(n4));
        if bl || test_tan(n.wrapping_sub(cd.x), n2.wrapping_sub(cd.y)) && (prod_lt0(cd.x.wrapping_sub(n), n.wrapping_sub(n3)) || is_vertical(n, n2, n3, n4) && test_bend(cd.x, cd.y, n, n2, n3, n4)) {
            let mut n5 = fix_half_mul(self.m_bend_length);
            let bl2 = false;
            if !(cd.x < n && n <= n3 || cd.x <= n && n < n3) {
                if n3 < n && n <= cd.x || n3 <= n && n < cd.x {
                    n5 = -n5;
                } else if bl && (cd.y > n2) != is_ccw(cd.x, cd.y, n, n2, n3, n4) {
                    n5 = -n5;
                }
            }
            let n6 = n.wrapping_sub(n5);
            let n7 = n.wrapping_add(n5);
            let pv = self.pe[p].prev;
            self.add_h_segment(n6, n7, n2, pv, NIL, S_BEND, 2);
            if bl2 {
                self.add_h_segment(n7, n6, n2, pv, NIL, S_BEND, 3);
            }
        }
    }

    fn do_v_bends_nxt(&mut self, n: i32, n2: i32, n3: i32, n4: i32, p: usize) {
        if n == n3 {
            return;
        }
        let (_, cd, _) = self.nxt_for_bend(p);
        let bl = prod_lt0(cd.x.wrapping_sub(n3), n3.wrapping_sub(n));
        if bl || test_tan(n4.wrapping_sub(cd.y), n3.wrapping_sub(cd.x)) && (prod_lt0(cd.y.wrapping_sub(n4), n4.wrapping_sub(n2)) || is_horizontal(n, n2, n3, n4) && test_bend(n, n2, n3, n4, cd.x, cd.y)) {
            let mut n5 = fix_half_mul(self.m_bend_length);
            let mut bl2 = false;
            if !(n2 <= n4 && n4 < cd.y || n2 < n4 && n4 <= cd.y) {
                if cd.y < n4 && n4 <= n2 || cd.y <= n4 && n4 < n2 {
                    n5 = -n5;
                } else if bl {
                    let bl4 = n > n3;
                    if bl4 != is_ccw(n, n2, n3, n4, cd.x, cd.y) {
                        n5 = -n5;
                    }
                } else {
                    bl2 = true;
                }
            }
            let n6 = n4.wrapping_sub(n5);
            let n7 = n4.wrapping_add(n5);
            self.add_v_segment(n6, n7, n3, p, NIL, S_BEND, 0);
            if bl2 {
                self.add_v_segment(n7, n6, n3, p, NIL, S_BEND, 1);
            }
        }
    }

    fn do_v_bends_prv(&mut self, n: i32, n2: i32, n3: i32, n4: i32, p: usize) {
        if n == n3 {
            return;
        }
        let (_, cd) = self.prv_for_bend(p);
        let bl = prod_lt0(cd.x.wrapping_sub(n), n.wrapping_sub(n3));
        if bl || test_tan(n2.wrapping_sub(cd.y), n.wrapping_sub(cd.x)) && (prod_lt0(cd.y.wrapping_sub(n2), n2.wrapping_sub(n4)) || is_horizontal(n, n2, n3, n4) && test_bend(cd.x, cd.y, n, n2, n3, n4)) {
            let mut n5 = fix_half_mul(self.m_bend_length);
            let bl2 = false;
            if !(cd.y < n2 && n2 <= n4 || cd.y <= n2 && n2 < n4) {
                if n4 < n2 && n2 <= cd.y || n4 <= n2 && n2 < cd.y {
                    n5 = -n5;
                } else if bl && (n > n3) != is_ccw(cd.x, cd.y, n, n2, n3, n4) {
                    n5 = -n5;
                }
            }
            let n6 = n2.wrapping_sub(n5);
            let n7 = n2.wrapping_add(n5);
            let pv = self.pe[p].prev;
            self.add_v_segment(n6, n7, n, pv, NIL, S_BEND, 2);
            if bl2 {
                self.add_v_segment(n7, n6, n, pv, NIL, S_BEND, 3);
            }
        }
    }

    fn merge_lnk_segs(&mut self, s: usize, s2: usize, mut l: usize) {
        while l != NIL {
            let lnk = self.sll[l].lnk;
            if self.sl[lnk].seg == s {
                self.sl[lnk].seg = s2;
            }
            l = self.sll[l].next;
        }
    }

    fn merge_h_segs(&mut self, s: usize, s2: usize) {
        let l = self.m_hlnks;
        self.merge_lnk_segs(s, s2, l);
    }

    fn merge_v_segs(&mut self, s: usize, s2: usize) {
        let l = self.m_vlnks;
        self.merge_lnk_segs(s, s2, l);
    }

    fn rem_extra_bends(&mut self, n: usize, n2: usize) {
        let mut s = self.m_seg_lists[n];
        let mut s2 = NIL;
        while s != NIL {
            let s3 = self.cs[s].s_nxt;
            let n3 = self.cs[s].s_loc;
            let mut s4 = self.m_seg_lists[n2];
            let mut s5 = NIL;
            while s4 != NIL {
                let s6 = self.cs[s4].s_nxt;
                let n4 = self.cs[s4].s_loc;
                if n4 > n3 {
                    break;
                }
                let (a, b) = (self.cs[s].clone(), self.cs[s4].clone());
                if n4 == n3 && b.s_min < a.s_max && b.s_max > a.s_min {
                    if a.s_type == S_BEND && b.s_type != S_BEND && b.s_type != S_GHOST && b.s_max.wrapping_sub(b.s_min) > a.s_max.wrapping_sub(a.s_min).wrapping_mul(3) {
                        if s2 == NIL {
                            self.m_seg_lists[n] = s3;
                        } else {
                            self.cs[s2].s_nxt = s3;
                        }
                        s = s2;
                        break;
                    }
                    if b.s_type == S_BEND && a.s_type != S_BEND && a.s_type != S_GHOST && a.s_max.wrapping_sub(a.s_min) > b.s_max.wrapping_sub(b.s_min).wrapping_mul(3) {
                        if s5 == NIL {
                            self.m_seg_lists[n2] = s6;
                        } else {
                            self.cs[s5].s_nxt = s6;
                        }
                        s4 = s5;
                    }
                }
                s5 = s4;
                s4 = s6;
            }
            s2 = s;
            s = s3;
        }
    }

    fn compact_list(&mut self, n: usize, bl: bool) {
        let mut s = self.m_seg_lists[n];
        let mut s2 = NIL;
        while s != NIL {
            let bl2;
            let mut s3 = self.cs[s].s_nxt;
            let mut s4 = s;
            loop {
                if s3 == NIL || self.cs[s3].s_loc > self.cs[s].s_loc {
                    bl2 = true;
                    break;
                }
                let n2 = self.cs[s].s_min;
                let n3 = self.cs[s].s_max;
                let n4 = self.cs[s3].s_min;
                let n5 = self.cs[s3].s_max;
                if n3 >= n4 && n2 <= n5 {
                    if n3.wrapping_sub(n2).wrapping_abs() > n5.wrapping_sub(n4).wrapping_abs() {
                        if bl {
                            self.merge_v_segs(s3, s);
                        } else {
                            self.merge_h_segs(s3, s);
                        }
                        self.cs[s].s_min = n2.min(n4);
                        self.cs[s].s_max = n3.max(n5);
                        self.cs[s].s_bonus = self.cs[s].s_bonus.max(self.cs[s3].s_bonus);
                        self.cs[s4].s_nxt = self.cs[s3].s_nxt;
                    } else {
                        if bl {
                            self.merge_v_segs(s, s3);
                        } else {
                            self.merge_h_segs(s, s3);
                        }
                        self.cs[s3].s_min = n2.min(n4);
                        self.cs[s3].s_max = n3.max(n5);
                        self.cs[s3].s_bonus = self.cs[s].s_bonus.max(self.cs[s3].s_bonus);
                        s = self.cs[s].s_nxt;
                        if s2 == NIL {
                            self.m_seg_lists[n] = s;
                        } else {
                            self.cs[s2].s_nxt = s;
                        }
                    }
                    bl2 = false;
                    break;
                }
                s4 = s3;
                s3 = self.cs[s3].s_nxt;
            }
            if !bl2 {
                continue;
            }
            s2 = s;
            s = self.cs[s].s_nxt;
        }
    }

    fn nxt_horz(&self, n: i32, n2: i32, p: usize) -> bool {
        let (_, cd, _) = self.nxt_for_bend(p);
        tst_flat(cd.y.wrapping_sub(n2), cd.x.wrapping_sub(n))
    }

    fn prv_horz(&self, n: i32, n2: i32, p: usize) -> bool {
        let (_, cd) = self.prv_for_bend(p);
        tst_flat(cd.y.wrapping_sub(n2), cd.x.wrapping_sub(n))
    }

    fn nxt_vert(&self, n: i32, n2: i32, p: usize) -> bool {
        let (_, cd, _) = self.nxt_for_bend(p);
        tst_flat(cd.x.wrapping_sub(n), cd.y.wrapping_sub(n2))
    }

    fn prv_vert(&self, n: i32, n2: i32, p: usize) -> bool {
        let (_, cd) = self.prv_for_bend(p);
        tst_flat(cd.x.wrapping_sub(n), cd.y.wrapping_sub(n2))
    }

    fn prv_same_dir(&self, n: i32, n2: i32, n3: i32, n4: i32, p: usize) -> bool {
        let (p, mut cd) = self.prv_for_bend(p);
        if p != NIL && self.pe[p].typ == CURVETO && self.pe[p].prev != NIL {
            cd = self.get_end_point(self.pe[p].prev);
        }
        tst_same_dir(n, n2, n3, n4, cd.x, cd.y)
    }

    fn nxt_same_dir(&self, n: i32, n2: i32, n3: i32, n4: i32, p: usize) -> bool {
        let (p, mut cd, _) = self.nxt_for_bend(p);
        if p != NIL && self.pe[p].typ == CURVETO {
            cd.x = self.pe[p].x3;
            cd.y = self.pe[p].y3;
        }
        tst_same_dir(n, n2, n3, n4, cd.x, cd.y)
    }

    fn gen_v_pts(&mut self, n: i32) {
        let mut p = self.m_path_start;
        let mut bl: bool;
        let mut bl2 = false;
        self.m_cp_to = self.m_cp_percent;
        self.m_cp_from = 100 - self.m_cp_to;
        let mut n2 = 0;
        let mut n3 = 0;
        let mut p2 = NIL;
        while p != NIL {
            let (cd6, cd7) = self.get_end_points(p);
            let typ = self.pe[p].typ;
            if typ == CURVETO {
                let mut bl3 = false;
                if self.pe[p].is_flex {
                    if bl2 {
                        if is_vertical(n3, n2, cd7.x, cd7.y) {
                            let pv = self.pe[p2].prev;
                            self.add_v_segment(n2, cd7.y, cd7.x, pv, p, S_LINE, 4);
                        }
                        bl2 = false;
                        bl = true;
                    } else {
                        bl2 = true;
                        bl = false;
                        n3 = cd6.x;
                        n2 = cd6.y;
                        p2 = p;
                    }
                } else {
                    bl = false;
                    bl2 = false;
                }
                let (n9, n10, n11, n12) = (self.pe[p].x1, self.pe[p].y1, self.pe[p].x2, self.pe[p].y2);
                if !bl {
                    let n6 = vert_quo(n9, n10, cd6.x, cd6.y);
                    if n6 == 0 {
                        self.do_v_bends_prv(cd6.x, cd6.y, n9, n10, p);
                    } else {
                        bl3 = true;
                        if n9 == cd6.x || n11 != cd7.x && (self.prv_vert(n9, n10, p) || !self.prv_same_dir(cd7.x, cd7.y, cd6.x, cd6.y, p)) {
                            let n8 = vert_quo(n11, n12, cd6.x, cd6.y);
                            let pv = self.pe[p].prev;
                            if n8 > 0 && prod_ge0(n10.wrapping_sub(cd6.y), n12.wrapping_sub(cd6.y)) && n12.wrapping_sub(cd6.y).wrapping_abs() > n10.wrapping_sub(cd6.y).wrapping_abs() {
                                let mut n5 = adj_dist(self.cp_to(n10, n12).wrapping_sub(cd6.y), n8);
                                let n7 = adj_dist(self.cp_to(cd6.y, n10).wrapping_sub(cd6.y), n6);
                                if n7.wrapping_abs() > n5.wrapping_abs() {
                                    n5 = n7;
                                }
                                self.add_v_segment(cd6.y, cd6.y.wrapping_add(n5), cd6.x, pv, p, S_CURVE, 5);
                            } else {
                                let to = self.cp_to(cd6.y, n10);
                                self.add_v_segment(cd6.y, to, cd6.x, pv, p, S_CURVE, 6);
                            }
                        }
                    }
                }
                if !bl2 {
                    let n6 = vert_quo(n11, n12, cd7.x, cd7.y);
                    if n6 == 0 {
                        self.do_v_bends_nxt(n11, n12, cd7.x, cd7.y, p);
                    } else if n11 == cd7.x || n9 != cd6.x && (self.nxt_vert(n11, n12, p) || !self.nxt_same_dir(cd6.x, cd6.y, cd7.x, cd7.y, p)) {
                        let mut n5 = adj_dist(cd7.y.wrapping_sub(self.cp_from(n12, cd7.y)), n6);
                        bl3 = true;
                        let mut n8 = vert_quo(cd6.x, cd6.y, cd7.x, cd7.y);
                        let n13 = if n8 > 0 { adj_dist(cd7.y.wrapping_sub(cd6.y), n8) } else { 0 };
                        if bl3 && n8 > 0 && n13.wrapping_abs() > n5.wrapping_abs() {
                            n5 = fix_half_mul(n13);
                            let n4 = fix_half_mul(cd6.y.wrapping_add(cd7.y));
                            let (_, cd) = self.prv_for_bend(p);
                            let (_, cd2, _) = self.nxt_for_bend(p);
                            let spot = pick_v_spot(cd6.x, cd6.y, cd7.x, cd7.y, n9, n10, n11, n12, cd.x, cd.y, cd2.x, cd2.y);
                            self.add_v_segment(n4.wrapping_sub(n5), n4.wrapping_add(n5), spot, p, NIL, S_CURVE, 7);
                        } else {
                            n8 = vert_quo(n9, n10, cd7.x, cd7.y);
                            if n8 > 0 && prod_ge0(n10.wrapping_sub(cd7.y), n12.wrapping_sub(cd7.y)) && n12.wrapping_sub(cd7.y).wrapping_abs() < n10.wrapping_sub(cd7.y).wrapping_abs() {
                                let n7 = adj_dist(cd7.y.wrapping_sub(self.cp_from(n10, n12)), n8);
                                if n7.wrapping_abs() > n5.wrapping_abs() {
                                    n5 = n7;
                                }
                                self.add_v_segment(cd7.y.wrapping_sub(n5), cd7.y, cd7.x, p, NIL, S_CURVE, 8);
                            } else {
                                self.add_v_segment(cd7.y.wrapping_sub(n5), cd7.y, cd7.x, p, NIL, S_CURVE, 9);
                            }
                        }
                    }
                }
                let _ = bl3;
                if !bl2 && !bl {
                    let n15 = cd6.x.max(cd7.x);
                    let n16 = cd6.x.min(cd7.x);
                    if n9.wrapping_sub(n15) >= 256 || n11.wrapping_sub(n15) >= 256 || n9.wrapping_sub(n16) <= 256 || n11.wrapping_sub(n16) <= 256 {
                        let (cd4, cd5) = self.find_curve_b_box(cd6.x, cd6.y, n9, n10, n11, n12, cd7.x, cd7.y);
                        if cd5.x.wrapping_sub(n15) > 256 || n16.wrapping_sub(cd4.x) > 256 {
                            let n17 = if n16.wrapping_sub(cd4.x) > cd5.x.wrapping_sub(n15) { cd4.x } else { cd5.x };
                            let (a0, a1) = self.check_b_box_edge(p, true, n17);
                            let n4 = fix_half_mul(a0.wrapping_add(a1));
                            let mut n5 = if a0 == a1 { cd7.y.wrapping_sub(cd6.y) / 10 } else { fix_half_mul(a1.wrapping_sub(a0)) };
                            if n5.wrapping_abs() < self.m_bend_length {
                                n5 = if n5 > 0 { fix_half_mul(self.m_bend_length) } else { fix_half_mul(-self.m_bend_length) };
                            }
                            self.add_v_segment(n4.wrapping_sub(n5), n4.wrapping_add(n5), n17, p, NIL, S_CURVE, 10);
                        }
                    }
                }
            } else if typ == MOVETO {
                self.m_bonus = 0;
                if n == -1 {
                    if self.is_lower(p) {
                        self.m_bonus = fix_int(200);
                    }
                } else if n == 1 && self.is_upper(p) {
                    self.m_bonus = fix_int(200);
                }
            } else if !self.is_tiny(p) {
                let mut n6 = vert_quo(cd6.x, cd6.y, cd7.x, cd7.y);
                if n6 > 0 {
                    if cd6.x == cd7.x {
                        let pv = self.pe[p].prev;
                        self.add_v_segment(cd6.y, cd7.y, cd6.x, pv, p, S_LINE, 11);
                    } else {
                        if n6 < 64 {
                            n6 = 64;
                        }
                        let n5 = fix_half_mul(adj_dist(cd7.y.wrapping_sub(cd6.y), n6));
                        let n4 = fix_half_mul(cd6.y.wrapping_add(cd7.y));
                        let (_, cd) = self.prv_for_bend(p);
                        let (_, cd2, _) = self.nxt_for_bend(p);
                        let spot = pick_v_spot(cd6.x, cd6.y, cd7.x, cd7.y, cd6.x, cd6.y, cd7.x, cd7.y, cd.x, cd.y, cd2.x, cd2.y);
                        self.add_v_segment(n4.wrapping_sub(n5), n4.wrapping_add(n5), spot, p, NIL, S_LINE, 12);
                    }
                } else {
                    self.do_v_bends_nxt(cd6.x, cd6.y, cd7.x, cd7.y, p);
                    self.do_v_bends_prv(cd6.x, cd6.y, cd7.x, cd7.y, p);
                }
            }
            p = self.pe[p].next;
        }
        self.compact_list(0, true);
        self.compact_list(1, true);
        self.rem_extra_bends(0, 1);
    }

    fn in_blue_band(&self, n: i32, n2: i32, arr: &[i32]) -> bool {
        if n2 <= 0 {
            return false;
        }
        let n3 = itfmy(n);
        let mut i = 0;
        while i < n2 as usize {
            if !(arr[i].wrapping_sub(self.m_bluefuzz) > n3 || arr[i + 1].wrapping_add(self.m_bluefuzz) < n3) {
                return true;
            }
            i += 2;
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn pick_h_spot(&self, n: i32, n2: i32, n3: i32, n4: i32, n5: i32, n6: i32, n7: i32, n8: i32, n9: i32, n10: i32, n11: i32, n12: i32, n13: i32) -> i32 {
        let bl3 = n5 < 0;
        let (bl2, bl) = if bl3 {
            (self.in_blue_band(n2, self.m_len_top_bands, &self.m_top_bands), self.in_blue_band(n4, self.m_len_top_bands, &self.m_top_bands))
        } else {
            (self.in_blue_band(n2, self.m_len_bot_bands, &self.m_bot_bands), self.in_blue_band(n4, self.m_len_bot_bands, &self.m_bot_bands))
        };
        if bl2 && !bl {
            return n2;
        }
        if bl && !bl2 {
            return n4;
        }
        if n2 == n7 && n4 != n9 {
            return n2;
        }
        if n2 != n7 && n4 == n9 {
            return n4;
        }
        if n2 == n11 && n4 != n13 {
            return n2;
        }
        if n2 != n11 && n4 == n13 {
            return n4;
        }
        if bl2 && bl {
            let (n15, n14) = if n2 > n4 { (n2, n4) } else { (n4, n2) };
            return if bl3 { n15 } else { n14 };
        }
        if n6.wrapping_sub(n).wrapping_abs() > n8.wrapping_sub(n3).wrapping_abs() {
            return n2;
        }
        if n8.wrapping_sub(n3).wrapping_abs() > n6.wrapping_sub(n).wrapping_abs() {
            return n4;
        }
        if n2 == n11 && n4 == n13 {
            if n.wrapping_sub(n10).wrapping_abs() > n3.wrapping_sub(n12).wrapping_abs() {
                return n2;
            }
            return n4;
        }
        fix_half_mul(n2.wrapping_add(n4))
    }

    fn gen_h_pts(&mut self) {
        let mut p = self.m_path_start;
        self.m_bonus = 0;
        let mut n = 0;
        let mut n2 = 0;
        let mut p2 = NIL;
        let mut bl: bool;
        let mut bl2 = false;
        self.m_cp_to = self.m_cp_percent;
        self.m_cp_from = 100 - self.m_cp_to;
        while p != NIL {
            let (cd6, cd7) = self.get_end_points(p);
            let typ = self.pe[p].typ;
            if typ == CURVETO {
                let mut bl3 = false;
                if self.pe[p].is_flex {
                    if bl2 {
                        bl2 = false;
                        bl = true;
                        if is_horizontal(n2, n, cd7.x, cd7.y) {
                            let pv = self.pe[p2].prev;
                            self.add_h_segment(n2, cd7.x, cd7.y, pv, p, S_LINE, 4);
                        }
                    } else {
                        bl2 = true;
                        bl = false;
                        n2 = cd6.x;
                        n = cd6.y;
                        p2 = p;
                    }
                } else {
                    bl = false;
                    bl2 = false;
                }
                let (n9, n10, n11, n12) = (self.pe[p].x1, self.pe[p].y1, self.pe[p].x2, self.pe[p].y2);
                if !bl {
                    let n5 = horz_quo(n9, n10, cd6.x, cd6.y);
                    if n5 == 0 {
                        self.do_h_bends_prv(cd6.x, cd6.y, n9, n10, p);
                    } else {
                        bl3 = true;
                        if n10 == cd6.y || n12 != cd7.y && (self.prv_horz(n9, n10, p) || !self.prv_same_dir(cd7.x, cd7.y, cd6.x, cd6.y, p)) {
                            let n8 = horz_quo(n11, n12, cd6.x, cd6.y);
                            let pv = self.pe[p].prev;
                            if n8 > 0 && prod_ge0(n9.wrapping_sub(cd6.x), n11.wrapping_sub(cd6.x)) && n11.wrapping_sub(cd6.x).wrapping_abs() > n9.wrapping_sub(cd6.x).wrapping_abs() {
                                let mut n4 = adj_dist(self.cp_to(n9, n11).wrapping_sub(cd6.x), n8);
                                let n7 = adj_dist(self.cp_to(cd6.x, n9).wrapping_sub(cd6.x), n5);
                                if n7.wrapping_abs() > n4.wrapping_abs() {
                                    n4 = n7;
                                }
                                self.add_h_segment(cd6.x, cd6.x.wrapping_add(n4), cd6.y, pv, p, S_CURVE, 5);
                            } else {
                                let n4 = adj_dist(self.cp_to(cd6.x, n9).wrapping_sub(cd6.x), n5);
                                self.add_h_segment(cd6.x, cd6.x.wrapping_add(n4), cd6.y, pv, p, S_CURVE, 6);
                            }
                        }
                    }
                }
                if !bl2 {
                    let n5 = horz_quo(n11, n12, cd7.x, cd7.y);
                    if n5 == 0 {
                        self.do_h_bends_nxt(n11, n12, cd7.x, cd7.y, p);
                    } else if n12 == cd7.y || n10 != cd6.y && (self.nxt_horz(n11, n12, p) || !self.nxt_same_dir(cd6.x, cd6.y, cd7.x, cd7.y, p)) {
                        let mut n4 = adj_dist(cd7.x.wrapping_sub(self.cp_from(n11, cd7.x)), n5);
                        let mut n8 = horz_quo(cd6.x, cd6.y, cd7.x, cd7.y);
                        bl3 = true;
                        let n13 = if n8 > 0 { adj_dist(cd7.x.wrapping_sub(cd6.x), n8) } else { 0 };
                        if bl3 && n8 > 0 && n13.wrapping_abs() > n4.wrapping_abs() {
                            let (_, cd) = self.prv_for_bend(p);
                            let (_, cd2, _) = self.nxt_for_bend(p);
                            n4 = fix_half_mul(n13);
                            let n3 = fix_half_mul(cd6.x.wrapping_add(cd7.x));
                            let n6 = self.pick_h_spot(cd6.x, cd6.y, cd7.x, cd7.y, n4, n9, n10, n11, n12, cd.x, cd.y, cd2.x, cd2.y);
                            self.add_h_segment(n3.wrapping_sub(n4), n3.wrapping_add(n4), n6, p, NIL, S_CURVE, 7);
                        } else {
                            n8 = horz_quo(n9, n10, cd7.x, cd7.y);
                            if n8 > 0 && prod_ge0(n9.wrapping_sub(cd7.x), n11.wrapping_sub(cd7.x)) && n11.wrapping_sub(cd7.x).wrapping_abs() < n9.wrapping_sub(cd7.x).wrapping_abs() {
                                let n7 = adj_dist(cd7.x.wrapping_sub(self.cp_from(n9, n11)), n8);
                                if n7.wrapping_abs() > n4.wrapping_abs() {
                                    n4 = n7;
                                }
                                self.add_h_segment(cd7.x.wrapping_sub(n4), cd7.x, cd7.y, p, NIL, S_CURVE, 8);
                            } else {
                                self.add_h_segment(cd7.x.wrapping_sub(n4), cd7.x, cd7.y, p, NIL, S_CURVE, 9);
                            }
                        }
                    }
                }
                let _ = bl3;
                if !bl2 && !bl {
                    let n15 = cd6.y.max(cd7.y);
                    let n6 = cd6.y.min(cd7.y);
                    if n10.wrapping_sub(n15) >= 256 || n12.wrapping_sub(n15) >= 256 || n10.wrapping_sub(n6) <= 256 || n12.wrapping_sub(n6) <= 256 {
                        let (cd4, cd5) = self.find_curve_b_box(cd6.x, cd6.y, n9, n10, n11, n12, cd7.x, cd7.y);
                        if cd5.y.wrapping_sub(n15) > 256 || n6.wrapping_sub(cd4.y) > 256 {
                            let n16 = if n6.wrapping_sub(cd4.y) > cd5.y.wrapping_sub(n15) { cd4.y } else { cd5.y };
                            let (a0, a1) = self.check_b_box_edge(p, false, n16);
                            let n3 = fix_half_mul(a0.wrapping_add(a1));
                            let mut n4 = if a0 == a1 { cd7.x.wrapping_sub(cd6.x) / 10 } else { fix_half_mul(a1.wrapping_sub(a0)) };
                            if n4.wrapping_abs() < self.m_bend_length {
                                n4 = if n4 as f64 > 0.0 { fix_half_mul(self.m_bend_length) } else { fix_half_mul(-self.m_bend_length) };
                            }
                            self.add_h_segment(n3.wrapping_sub(n4), n3.wrapping_add(n4), n16, p, NIL, S_CURVE, 10);
                        }
                    }
                }
            } else if typ != MOVETO && !self.is_tiny(p) {
                let mut n5 = horz_quo(cd6.x, cd6.y, cd7.x, cd7.y);
                if n5 > 0 {
                    let pv = self.pe[p].prev;
                    if cd6.y == cd7.y {
                        self.add_h_segment(cd6.x, cd7.x, cd6.y, pv, p, S_LINE, 11);
                    } else {
                        if n5 < 64 {
                            n5 = 64;
                        }
                        let n4 = fix_half_mul(adj_dist(cd7.x.wrapping_sub(cd6.x), n5));
                        let n3 = fix_half_mul(cd6.x.wrapping_add(cd7.x));
                        let (_, cd) = self.prv_for_bend(p);
                        let (_, cd2, _) = self.nxt_for_bend(p);
                        let y = self.pick_h_spot(cd6.x, cd6.y, cd7.x, cd7.y, n4, cd6.x, cd6.y, cd7.x, cd7.y, cd.x, cd.y, cd2.x, cd2.y);
                        self.add_h_segment(n3.wrapping_sub(n4), n3.wrapping_add(n4), y, pv, p, S_LINE, 12);
                    }
                } else {
                    self.do_h_bends_nxt(cd6.x, cd6.y, cd7.x, cd7.y, p);
                    self.do_h_bends_prv(cd6.x, cd6.y, cd7.x, cd7.y, p);
                }
            }
            p = self.pe[p].next;
        }
        self.compact_list(2, false);
        self.compact_list(3, false);
        self.rem_extra_bends(2, 3);
    }

    fn pre_gen_pts(&mut self) {
        self.m_hlnks = NIL;
        self.m_seg_lists[2] = NIL;
        self.m_seg_lists[3] = NIL;
        self.m_vlnks = NIL;
        self.m_seg_lists[0] = NIL;
        self.m_seg_lists[1] = NIL;
    }

    fn init_all(&mut self) {
        self.init_data();
        self.init_auto();
        self.init_fix();
        self.init_gen();
        self.init_pick();
    }

    fn pt_lst_len(&self, mut c: usize) -> i32 {
        let mut n = 0;
        while c != NIL {
            n += 1;
            c = self.cp[c].next;
        }
        n
    }

    fn point_list_check(&self, c1: usize, mut c2: usize) -> i32 {
        let c = self.cp[c1].c;
        let n6 = fix_half_mul(self.m_band_margin);
        let (mut n4, mut n5) = match c {
            b'm' | b'y' => (self.cp[c1].x0, self.cp[c1].x1),
            b'b' | b'v' => (self.cp[c1].y0, self.cp[c1].y1),
            _ => (0, 0),
        };
        if n4 > n5 {
            std::mem::swap(&mut n4, &mut n5);
        }
        while c2 != NIL {
            if self.cp[c2].c == c {
                let (mut n2, mut n3) = match c {
                    b'm' | b'y' => (self.cp[c2].x0, self.cp[c2].x1),
                    b'b' | b'v' => (self.cp[c2].y0, self.cp[c2].y1),
                    _ => (0, 0),
                };
                if n2 > n3 {
                    std::mem::swap(&mut n2, &mut n3);
                }
                if n2 == n4 && n3 == n5 {
                    return 1;
                }
                n2 = n2.wrapping_sub(n6);
                n3 = n3.wrapping_add(n6);
                if n2 <= n5 && n4 <= n3 {
                    return 0;
                }
            }
            c2 = self.cp[c2].next;
        }
        -1
    }

    fn same_color_lists(&self, mut c: usize, c2: usize) -> bool {
        if self.pt_lst_len(c) != self.pt_lst_len(c2) {
            return false;
        }
        while c != NIL {
            if self.point_list_check(c, c2) != 1 {
                return false;
            }
            c = self.cp[c].next;
        }
        true
    }

    fn same_colors(&self, n: i32, n2: i32) -> bool {
        if n == n2 {
            return true;
        }
        self.same_color_lists(self.m_pt_lst_array[n as usize], self.m_pt_lst_array[n2 as usize])
    }

    fn merge_from_main_colors(&mut self, c: u8) {
        let mut p = self.m_pt_lst_array[0];
        while p != NIL {
            if self.cp[p].c == c && self.point_list_check(p, self.m_point_list) == -1 {
                let (x0, y0, x1, y1, p0, p1) = (self.cp[p].x0, self.cp[p].y0, self.cp[p].x1, self.cp[p].y1, self.cp[p].p0, self.cp[p].p1);
                if c == b'b' {
                    self.add_color_point(0, y0, 0, y1, c, p0, p1);
                } else {
                    self.add_color_point(x0, 0, x1, 0, c, p0, p1);
                }
            }
            p = self.cp[p].next;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_color_point(&mut self, n: i32, n2: i32, n3: i32, n4: i32, c: u8, p: usize, p2: usize) {
        self.m_cpt_sn = self.m_cpt_sn.wrapping_add(1);
        let mut sn = self.m_cpt_sn;
        if sn == 0 {
            sn += 1;
        }
        self.cp.push(ClrPoint { cpt_sn: sn, next: NIL, x0: n, y0: n2, x1: n3, y1: n4, p0: p, p1: p2, c, done: false });
        let id = self.cp.len() - 1;
        let n5 = self.point_list_check(id, self.m_point_list);
        if n5 == -1 {
            self.cp[id].next = self.m_point_list;
            self.m_point_list = id;
        }
    }

    fn copy_clr_from_lst(&mut self, c: u8, mut p: usize) {
        let bl = c == b'b' || c == b'v';
        while p != NIL {
            if self.cp[p].c == c {
                let (x0, y0, x1, y1, p0, p1) = (self.cp[p].x0, self.cp[p].y0, self.cp[p].x1, self.cp[p].y1, self.cp[p].p0, self.cp[p].p1);
                if bl {
                    self.add_color_point(0, y0, 0, y1, c, p0, p1);
                } else {
                    self.add_color_point(x0, 0, x1, 0, c, p0, p1);
                }
            }
            p = self.cp[p].next;
        }
    }

    fn copy_main_v(&mut self) {
        let p = self.m_pt_lst_array[0];
        self.copy_clr_from_lst(b'm', p);
    }

    fn copy_main_h(&mut self) {
        let p = self.m_pt_lst_array[0];
        self.copy_clr_from_lst(b'v', p);
    }

    fn add_h_pair(&mut self, v: usize, c: u8) {
        let mut n = itfmy(self.cv[v].v_loc1);
        let mut n2 = itfmy(self.cv[v].v_loc2);
        let bst = self.cv[v].v_bst;
        let mut p = self.cs[self.cv[bst].v_seg1].s_elt;
        let mut p2 = self.cs[self.cv[bst].v_seg2].s_elt;
        if n2 < n {
            std::mem::swap(&mut n, &mut n2);
            std::mem::swap(&mut p, &mut p2);
        }
        if self.cv[v].v_ghst {
            if self.cs[self.cv[v].v_seg1].s_type == S_GHOST {
                n = n2;
                p = p2;
                p2 = NIL;
                n2 = n.wrapping_add(fix_int(TOPGHST));
            } else {
                n2 = n;
                p2 = p;
                p = NIL;
                n = n2.wrapping_sub(fix_int(BOTGHST));
            }
        }
        self.add_color_point(0, n, 0, n2, c, p, p2);
    }

    fn add_v_pair(&mut self, v: usize, c: u8) {
        let mut n = itfmx(self.cv[v].v_loc1);
        let mut n2 = itfmx(self.cv[v].v_loc2);
        let bst = self.cv[v].v_bst;
        let mut p = self.cs[self.cv[bst].v_seg1].s_elt;
        let mut p2 = self.cs[self.cv[bst].v_seg2].s_elt;
        if n > n2 {
            std::mem::swap(&mut n, &mut n2);
            std::mem::swap(&mut p, &mut p2);
        }
        self.add_color_point(n, 0, n2, 0, c, p, p2);
    }

    /// `useCounter`, only reachable with `mDoCounters`, which nothing sets.
    fn use_counter(&mut self, _v: usize, _bl: bool) -> bool {
        unreachable!("AutoColor.useCounter needs mDoCounters")
    }

    fn get_new_pt_lst(&mut self) {
        if self.m_num_pt_lsts >= self.m_max_pt_lsts {
            self.m_max_pt_lsts += 5;
            let mut arr = vec![NIL; self.m_max_pt_lsts as usize];
            for i in 0..(self.m_max_pt_lsts - 5) as usize {
                arr[i] = self.m_pt_lst_array[i];
            }
            self.m_pt_lst_array = arr;
        }
        self.m_pt_lst_index = self.m_num_pt_lsts;
        self.m_num_pt_lsts += 1;
        self.m_point_list = NIL;
        self.m_pt_lst_array[self.m_pt_lst_index as usize] = NIL;
    }

    fn xtra_clrs(&mut self, p: usize) {
        self.m_pt_lst_array[self.m_pt_lst_index as usize] = self.m_point_list;
        if self.pe[p].newcolors == 0 {
            self.get_new_pt_lst();
            self.pe[p].newcolors = self.m_pt_lst_index as i16;
        }
        self.m_pt_lst_index = self.pe[p].newcolors as i32;
        self.m_point_list = self.m_pt_lst_array[self.m_pt_lst_index as usize];
    }

    fn blues(&mut self) {
        let (mut n, mut n2, mut n3, mut n4, mut n5) = (0, 0, 0, 0, 0);
        if no_blue_char(self.m_unicode) {
            self.m_len_bot_bands = 0;
            self.m_len_top_bands = 0;
        }
        self.gen_h_pts();
        if self.m_do_counters && !self.m_counter_failed && h_color_char(self.m_unicode) {
            n = self.m_prune_value;
            self.m_prune_value = flt_to_fix(self.m_min_val);
            n5 = self.m_prune_a;
            self.m_prune_a = flt_to_fix(self.m_min_val);
            n2 = self.m_prune_d;
            self.m_prune_d = flt_to_fix(self.m_min_val);
            n3 = self.m_prune_c;
            self.m_prune_c = flt_to_fix(self.m_max_val);
            n4 = self.m_prune_b;
            self.m_prune_b = flt_to_fix(self.m_min_val);
        }
        self.eval_h();
        self.prune_h_vals();
        self.find_best_h_vals();
        self.merge_vals(false);
        let vl = self.m_val_list;
        self.mark_links(vl, true);
        self.check_vals(vl, false);
        self.do_h_stems(vl);
        self.pick_h_vals(vl);
        if self.m_do_counters && !self.m_counter_failed && h_color_char(self.m_unicode) {
            self.m_prune_value = n;
            self.m_prune_d = n2;
            self.m_prune_c = n3;
            self.m_prune_b = n4;
            self.m_prune_a = n5;
            self.m_use_h = self.use_counter(self.m_hcoloring, false);
            if !self.m_use_h {
                self.add_b_box_hv(true, true);
                self.m_use_h = self.use_counter(self.m_hcoloring, false);
                if !self.m_use_h {
                    self.m_counter_failed = true;
                }
            }
        } else {
            self.m_use_h = false;
        }
        if self.m_hcoloring == NIL {
            self.add_b_box_hv(true, false);
        }
        let mut v = self.m_hcoloring;
        while v != NIL {
            let c = if self.m_use_h { b'v' } else { b'b' };
            self.add_h_pair(v, c);
            v = self.cv[v].v_nxt;
        }
    }

    fn do_h_stems(&mut self, mut v: usize) {
        let mut n = i32::MIN;
        let mut n2 = i32::MAX;
        let mut bb = ACBBox::new();
        if !self.m_do_aligns {
            return;
        }
        self.find_path_b_box(&mut bb);
        let (lo, hi) = (self.un_scale_abs(itfmy(bb.ymin)), self.un_scale_abs(itfmy(bb.ymax)));
        self.add_char_extreme(lo, hi);
        while v != NIL {
            let mut n3 = itfmy(self.cv[v].v_loc1);
            let mut n4 = itfmy(self.cv[v].v_loc2);
            if n4 < n3 {
                std::mem::swap(&mut n3, &mut n4);
            }
            if n4 > n {
                n = n4;
            }
            if n3 < n2 {
                n2 = n3;
            }
            if self.cv[v].v_ghst {
                v = self.cv[v].v_nxt;
                continue;
            }
            let bl = !self.find_line_seg(self.cv[v].v_loc1, self.bot_list()) && !self.find_line_seg(self.cv[v].v_loc2, self.top_list());
            let (a, b) = (self.un_scale_abs(n4), self.un_scale_abs(n3));
            self.add_h_stem(a, b, bl);
            v = self.cv[v].v_nxt;
            if n4 == i32::MIN && n3 == i32::MAX {
                continue;
            }
            let (a, b) = (self.un_scale_abs(n3), self.un_scale_abs(n4));
            self.add_zone(a, b);
        }
        if n != i32::MIN || n2 != i32::MAX {
            let (a, b) = (self.un_scale_abs(n2), self.un_scale_abs(n));
            self.add_char_zone(a, b);
        }
    }

    fn yellows(&mut self) {
        let (mut n, mut n2, mut n3, mut n4, mut n5) = (0, 0, 0, 0, 0);
        self.gen_v_pts(special_char_type(self.m_unicode));
        if self.m_do_counters && !self.m_counter_failed && v_color_char(self.m_unicode) {
            n = self.m_prune_value;
            self.m_prune_value = flt_to_fix(self.m_min_val);
            n5 = self.m_prune_a;
            self.m_prune_a = flt_to_fix(self.m_min_val);
            n2 = self.m_prune_d;
            self.m_prune_d = flt_to_fix(self.m_min_val);
            n3 = self.m_prune_c;
            self.m_prune_c = flt_to_fix(self.m_max_val);
            n4 = self.m_prune_b;
            self.m_prune_b = flt_to_fix(self.m_min_val);
        }
        self.eval_v();
        self.prune_v_vals();
        self.find_best_v_vals();
        self.merge_vals(true);
        let vl = self.m_val_list;
        self.mark_links(vl, false);
        self.check_vals(vl, true);
        self.do_v_stems(vl);
        self.pick_v_vals(vl);
        if self.m_do_counters && !self.m_counter_failed && v_color_char(self.m_unicode) {
            self.m_prune_value = n;
            self.m_prune_d = n2;
            self.m_prune_c = n3;
            self.m_prune_b = n4;
            self.m_prune_a = n5;
            self.m_use_v = self.use_counter(self.m_vcoloring, true);
            if !self.m_use_v {
                self.add_b_box_hv(false, true);
                self.m_use_v = self.use_counter(self.m_vcoloring, true);
                if !self.m_use_v {
                    self.m_counter_failed = true;
                }
            }
        } else {
            self.m_use_v = false;
        }
        if self.m_vcoloring == NIL {
            self.add_b_box_hv(false, false);
        }
        let mut v = self.m_vcoloring;
        while v != NIL {
            let c = if self.m_use_v { b'm' } else { b'y' };
            self.add_v_pair(v, c);
            v = self.cv[v].v_nxt;
        }
    }

    fn do_v_stems(&mut self, mut v: usize) {
        if !self.m_do_aligns {
            return;
        }
        while v != NIL {
            let bl = !self.find_line_seg(self.cv[v].v_loc1, self.left_list()) && !self.find_line_seg(self.cv[v].v_loc2, self.right_list());
            let mut n2 = itfmx(self.cv[v].v_loc1);
            let mut n = itfmx(self.cv[v].v_loc2);
            if n2 > n {
                std::mem::swap(&mut n2, &mut n);
            }
            let (a, b) = (self.un_scale_abs(n), self.un_scale_abs(n2));
            self.add_v_stem(a, b, bl);
            v = self.cv[v].v_nxt;
        }
    }

    fn remove_redundant_first_colors(&mut self) {
        if self.m_num_pt_lsts < 2 {
            return;
        }
        if !self.same_colors(0, 1) {
            return;
        }
        let mut p = self.m_path_start;
        while p != NIL {
            if self.pe[p].newcolors == 1 {
                self.pe[p].newcolors = 0;
                return;
            }
            p = self.pe[p].next;
        }
    }

    fn add_colors_setup(&mut self) -> Result<(), ACException> {
        self.m_v_big_dist = 0;
        for n in 0..self.m_num_v_stems as usize {
            if self.m_v_stems[n] <= self.m_v_big_dist {
                continue;
            }
            self.m_v_big_dist = self.m_v_stems[n];
        }
        self.m_v_big_dist = dtfmx(self.m_v_big_dist);
        if self.m_v_big_dist < self.m_init_big_dist {
            self.m_v_big_dist = self.m_init_big_dist;
        }
        self.m_v_big_dist = self.m_v_big_dist.wrapping_mul(23) / 20;
        self.m_v_big_dist_r = fix_to_flt(self.m_v_big_dist);
        self.m_h_big_dist = 0;
        for n in 0..self.m_num_h_stems as usize {
            if self.m_h_stems[n] <= self.m_h_big_dist {
                continue;
            }
            self.m_h_big_dist = self.m_h_stems[n];
        }
        self.m_h_big_dist = dtfmy(self.m_h_big_dist).wrapping_abs();
        if self.m_h_big_dist < self.m_init_big_dist {
            self.m_h_big_dist = self.m_init_big_dist;
        }
        self.m_h_big_dist = self.m_h_big_dist.wrapping_mul(23) / 20;
        self.m_h_big_dist_r = fix_to_flt(self.m_h_big_dist);
        if !self.m_scalinghints {
            self.round_path_coords();
        }
        self.check_for_multi_move_to();
        self.collect_sub_path_info()?;
        if self.m_fix_winding {
            self.fix_path_wind(true);
        }
        Ok(())
    }

    fn add_colors_inner_loop(&mut self) -> Result<(), ACException> {
        let mut n = 0;
        loop {
            self.pre_gen_pts();
            if self.m_do_smoothing {
                self.check_smooth();
            }
            self.init_shuffle_subpaths();
            self.blues();
            if self.m_generate_v_stems {
                self.yellows();
            }
            if !self.m_do_write_glyph {
                return Ok(());
            }
            if self.m_edit_char {
                self.do_shuffle_subpaths();
            }
            self.m_hprimary = self.copy_clrs(self.m_hcoloring);
            if self.m_generate_v_stems {
                self.m_vprimary = self.copy_clrs(self.m_vcoloring);
            }
            self.prune_element_color_segs();
            if self.m_extracolor {
                self.auto_extra_colors(move_to_new_clrs(self.m_unicode));
            }
            self.m_pt_lst_array[self.m_pt_lst_index as usize] = self.m_point_list;
            let again = self.m_counter_failed && {
                n += 1;
                n == 1
            } || self.do_fixes() && n <= 2;
            if !again {
                break;
            }
            self.init_all();
            self.reset_glyph();
            self.add_colors_setup()?;
            if !self.pre_check_for_coloring() {
                break;
            }
            if !self.m_flex_ok {
                continue;
            }
            self.auto_add_flex();
        }
        Ok(())
    }

    fn add_colors_cleanup(&mut self) {
        self.remove_redundant_first_colors();
        if self.m_path_start != NIL && self.m_path_start != self.m_path_end {
            self.write_glyph();
        }
        self.init_all();
    }

    fn add_colors(&mut self) -> Result<(), ACException> {
        if self.m_path_start == NIL || self.m_path_start == self.m_path_end {
            self.write_glyph();
            return Ok(());
        }
        self.m_counter_failed = false;
        self.add_colors_setup()?;
        if !self.pre_check_for_coloring() {
            self.write_glyph();
            return Ok(());
        }
        if self.m_flex_ok {
            self.auto_add_flex();
        }
        self.add_colors_inner_loop()?;
        self.add_colors_cleanup();
        Ok(())
    }

    fn do_glyph(&mut self) -> Result<bool, ACException> {
        self.reset_glyph();
        self.add_colors()?;
        Ok(true)
    }

    fn le_prune_value(&self, n: i32) -> bool {
        n < 256 && n << 10 <= self.m_prune_value
    }

    fn adjust_val(&self, mut n: i32, mut n2: i32, mut n3: i32, n4: i32, bl: bool) -> i32 {
        if n3 < 256 {
            n3 = 256;
        }
        if n < 256 {
            n = 256;
        }
        if n2 < 256 {
            n2 = 256;
        }
        let d3 = if n.wrapping_abs() < MAXF { n.wrapping_mul(n) as f64 } else { (n as f64) * (n as f64) };
        let d2 = if n2.wrapping_abs() < MAXF { n2.wrapping_mul(n2) as f64 } else { (n2 as f64) * (n2 as f64) };
        let mut d = if n3.wrapping_abs() < MAXF { n3.wrapping_mul(n3) as f64 } else { (n3 as f64) * (n3 as f64) };
        let mut d4 = 1000.0 * d3 * d2 / (d * d);
        if n4 > (if bl { self.m_h_big_dist } else { self.m_v_big_dist }) {
            let d5 = fix_to_flt(n4);
            d = if bl { self.m_h_big_dist_r } else { self.m_v_big_dist_r };
            if d + d <= d5 {
                d4 = 0.0;
            } else {
                d /= d5;
                d *= d;
                d *= d;
                d *= d;
                d4 *= d;
            }
        }
        if d4 > self.m_max_val {
            d4 = self.m_max_val;
        } else if d4 > 0.0 && d4 < self.m_min_val {
            d4 = self.m_min_val;
        }
        flt_to_fix(d4)
    }

    /// `evalHPair`, returning (spc, val).
    fn eval_h_pair(&self, s: usize, s2: usize) -> (i32, i32) {
        let mut spc = 0;
        let (n3, n4, n5, n6) = (self.cs[s].s_max, self.cs[s].s_min, self.cs[s2].s_max, self.cs[s2].s_min);
        let n7 = self.cs[s].s_loc;
        let n2 = self.cs[s2].s_loc;
        let n8 = n7.wrapping_sub(n2).wrapping_abs();
        if n8 < self.m_min_dist {
            return (spc, 0);
        }
        let bl = self.in_blue_band(n7, self.m_len_bot_bands, &self.m_bot_bands);
        let bl2 = self.in_blue_band(n2, self.m_len_top_bands, &self.m_top_bands);
        if bl && bl2 {
            return (spc, 0);
        }
        if bl || bl2 {
            spc = fix_int(2);
        }
        let mut n;
        if n6 <= n3 && n5 >= n4 {
            let n9 = n5.min(n3).wrapping_sub(n6.max(n4));
            let n10 = n5.wrapping_sub(n6).min(n3.wrapping_sub(n4));
            n = if n10 == n9 { n8 } else { calc_overlap_dist(n8, n9, n10) };
        } else {
            let n11 = n6.wrapping_sub(n3).wrapping_abs();
            let n12 = n5.wrapping_sub(n4).wrapping_abs();
            let n13 = n11.min(n12);
            n = gap_dist(n13).wrapping_add(7i32.wrapping_mul(n8) / 5);
            if n13 > n8 {
                n = n.wrapping_mul(n13 / n8);
            }
        }
        let n14 = fix_two_mul(self.m_min_dist);
        n = n.max(n14);
        if self.m_num_h_stems > 0 {
            let n15 = idtfmy(n8).wrapping_abs();
            for i in 0..self.m_num_h_stems as usize {
                if n15 != self.m_h_stems[i] {
                    continue;
                }
                spc = spc.wrapping_add(256);
                break;
            }
        }
        (spc, self.adjust_val(n3.wrapping_sub(n4), n5.wrapping_sub(n6), n, n8, true))
    }

    /// `evalVPair`, returning (spc, val).
    fn eval_v_pair(&self, s: usize, s2: usize) -> (i32, i32) {
        let (n3, n4, n5, n6) = (self.cs[s].s_max, self.cs[s].s_min, self.cs[s2].s_max, self.cs[s2].s_min);
        let n7 = self.cs[s].s_loc;
        let n2 = self.cs[s2].s_loc;
        let n8 = n7.wrapping_sub(n2).wrapping_abs();
        if n8 < self.m_min_dist {
            return (0, 0);
        }
        let mut n;
        if n3 >= n6 && n4 <= n5 {
            let n9 = n3.min(n5).wrapping_sub(n4.max(n6));
            let n10 = n3.wrapping_sub(n4).min(n5.wrapping_sub(n6));
            n = if n10 == n9 { n8 } else { calc_overlap_dist(n8, n9, n10) };
        } else {
            let n11 = n3.wrapping_sub(n6).wrapping_abs();
            let n12 = n4.wrapping_sub(n5).wrapping_abs();
            let n13 = n11.min(n12);
            n = (7i32.wrapping_mul(n8) / 5).wrapping_add(gap_dist(n13));
            if n13 > n8 {
                n = n.wrapping_mul(n13 / n8);
            }
        }
        let n14 = fix_two_mul(self.m_min_dist);
        n = n.max(n14);
        let n17 = self.cs[s].s_bonus.min(self.cs[s2].s_bonus);
        let mut spc = if n17 > 0 { fix_int(2) } else { 0 };
        if self.m_num_v_stems > 0 {
            let n19 = idtfmx(n8).wrapping_abs();
            for i in 0..self.m_num_v_stems as usize {
                if n19 != self.m_v_stems[i] {
                    continue;
                }
                spc = spc.wrapping_add(256);
                break;
            }
        }
        (spc, self.adjust_val(n3.wrapping_sub(n4), n5.wrapping_sub(n6), n, n8, false))
    }

    fn insert_v_value(&mut self, n: i32, n2: i32, n3: i32, n4: i32, s: usize, s2: usize) {
        let mut v = ClrVal::new();
        self.m_cvl_sn = self.m_cvl_sn.wrapping_add(1);
        v.cvl_sn = self.m_cvl_sn;
        if v.cvl_sn == 0 {
            v.cvl_sn += 1;
        }
        v.v_val = n3;
        v.init_val = n3;
        v.v_loc1 = n;
        v.v_loc2 = n2;
        v.v_spc = n4;
        v.v_seg1 = s;
        v.v_seg2 = s2;
        v.v_ghst = false;
        self.cv.push(v);
        let id = self.cv.len() - 1;
        let mut v2 = self.m_val_list;
        let mut v3 = NIL;
        while v2 != NIL && self.cv[v2].v_loc1 < n {
            v3 = v2;
            v2 = self.cv[v2].v_nxt;
        }
        while v2 != NIL && self.cv[v2].v_loc1 == n && self.cv[v2].v_loc2 < n2 {
            v3 = v2;
            v2 = self.cv[v2].v_nxt;
        }
        if v3 == NIL {
            self.m_val_list = id;
        } else {
            self.cv[v3].v_nxt = id;
        }
        self.cv[id].v_nxt = v2;
    }

    fn add_v_value(&mut self, n: i32, n2: i32, n3: i32, n4: i32, s: usize, s2: usize) {
        if n3 == 0 {
            return;
        }
        if self.le_prune_value(n3) && n4 <= 0 {
            return;
        }
        if s != NIL && self.cs[s].s_type == S_BEND && s2 != NIL && self.cs[s2].s_type == S_BEND {
            return;
        }
        if !(n3 > self.m_prune_d || n4 > 0 || s == NIL || s2 == NIL || self.cs[s].s_type != S_BEND && self.cs[s2].s_type != S_BEND && self.check_b_boxes(self.cs[s].s_elt, self.cs[s2].s_elt)) {
            return;
        }
        self.insert_v_value(n, n2, n3, n4, s, s2);
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_h_value(&mut self, n: i32, n2: i32, n3: i32, n4: i32, s: usize, s2: usize, bl: bool) {
        let mut v = self.m_val_list;
        let mut v2 = NIL;
        while v != NIL && self.cv[v].v_loc2 < n2 {
            v2 = v;
            v = self.cv[v].v_nxt;
        }
        while v != NIL && self.cv[v].v_loc2 == n2 && self.cv[v].v_loc1 < n {
            v2 = v;
            v = self.cv[v].v_nxt;
        }
        let mut v3 = v;
        while bl && v3 != NIL && self.cv[v3].v_loc2 == n2 && self.cv[v3].v_loc1 == n {
            let c = &self.cv[v3];
            if !(c.v_ghst || c.v_seg1 != s && c.v_seg2 != s2 || c.v_val <= n3) {
                return;
            }
            v3 = self.cv[v3].v_nxt;
        }
        let mut v4 = ClrVal::new();
        self.m_cvl_sn = self.m_cvl_sn.wrapping_add(1);
        v4.cvl_sn = self.m_cvl_sn;
        v4.v_val = n3;
        v4.init_val = n3;
        v4.v_spc = n4;
        v4.v_loc1 = n;
        v4.v_loc2 = n2;
        v4.v_seg1 = s;
        v4.v_seg2 = s2;
        v4.v_ghst = bl;
        self.cv.push(v4);
        let id = self.cv.len() - 1;
        if v2 == NIL {
            self.m_val_list = id;
        } else {
            self.cv[v2].v_nxt = id;
        }
        self.cv[id].v_nxt = v;
    }

    fn add_h_value(&mut self, n: i32, n2: i32, n3: i32, n4: i32, s: usize, s2: usize) {
        if n3 == 0 {
            return;
        }
        if self.le_prune_value(n3) && n4 <= 0 {
            return;
        }
        if self.cs[s].s_type == S_BEND && self.cs[s2].s_type == S_BEND {
            return;
        }
        let bl = self.cs[s].s_type == S_GHOST || self.cs[s2].s_type == S_GHOST;
        if !(bl || n3 > self.m_prune_d || n4 > 0 || self.cs[s].s_type != S_BEND && self.cs[s2].s_type != S_BEND && self.check_b_boxes(self.cs[s].s_elt, self.cs[s2].s_elt)) {
            return;
        }
        self.insert_h_value(n, n2, n3, n4, s, s2, bl);
    }

    fn comb_vals(&self, n: i32, n2: i32) -> i32 {
        let mut d2 = 0.0;
        let mut d3 = fix_to_flt(n);
        let d4 = fix_to_flt(n2);
        let d = d3 * d4;
        let mut d5 = d;
        for i in 0..16 {
            d2 = 0.5 * (d5 + d / d5);
            if i >= 8 && mfabs(d2 - d5) <= mfabs(d2) * 1.0E-7 {
                break;
            }
            d5 = d2;
        }
        d3 += d4 + 2.0 * d2;
        if d3 > self.m_max_val {
            d3 = self.m_max_val;
        } else if d3 > 0.0 && d3 < self.m_min_val {
            d3 = self.m_min_val;
        }
        flt_to_fix(d3)
    }

    fn combine_values(&mut self) {
        let mut v = self.m_val_list;
        while v != NIL {
            let mut v2 = self.cv[v].v_nxt;
            let n = self.cv[v].v_loc1;
            let n2 = self.cv[v].v_loc2;
            let mut n3 = self.cv[v].v_val;
            let mut bl = false;
            while v2 != NIL && self.cv[v2].v_loc1 == n && self.cv[v2].v_loc2 == n2 {
                n3 = if self.cv[v2].v_ghst { self.cv[v2].v_val } else { self.comb_vals(n3, self.cv[v2].v_val) };
                bl = true;
                v2 = self.cv[v2].v_nxt;
            }
            if bl {
                while v != v2 {
                    self.cv[v].v_val = n3;
                    v = self.cv[v].v_nxt;
                }
                continue;
            }
            v = v2;
        }
    }

    fn eval_v(&mut self) {
        self.m_val_list = NIL;
        let mut s = self.left_list();
        while s != NIL {
            let mut s2 = self.right_list();
            while s2 != NIL {
                let n2 = self.cs[s].s_loc;
                let n = self.cs[s2].s_loc;
                if n2 < n {
                    let (spc, val) = self.eval_v_pair(s, s2);
                    self.add_v_value(n2, n, val, spc, s, s2);
                }
                s2 = self.cs[s2].s_nxt;
            }
            s = self.cs[s].s_nxt;
        }
        self.combine_values();
    }

    fn new_seg(&mut self, s: ClrSeg) -> usize {
        self.cs.push(s);
        self.cs.len() - 1
    }

    fn eval_h(&mut self) {
        self.m_val_list = NIL;
        let mut s = self.bot_list();
        while s != NIL {
            let mut s2 = self.top_list();
            while s2 != NIL {
                let n4 = self.cs[s].s_loc;
                let n3 = self.cs[s2].s_loc;
                if n4 < n3 {
                    let (n2, n) = self.eval_h_pair(s, s2);
                    self.add_h_value(n4, n3, n, n2, s, s2);
                }
                s2 = self.cs[s2].s_nxt;
            }
            s = self.cs[s].s_nxt;
        }
        let mut g = ClrSeg::new();
        self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
        g.csg_sn = self.m_csg_sn;
        g.s_type = S_GHOST;
        g.s_elt = NIL;
        let s3 = self.new_seg(g);
        if self.m_len_bot_bands >= 2 || self.m_len_top_bands >= 2 {
            let mut s4 = self.bot_list();
            while s4 != NIL {
                let n7 = self.cs[s4].s_loc;
                if self.in_blue_band(n7, self.m_len_bot_bands, &self.m_bot_bands) {
                    let n6 = n7.wrapping_add(self.m_ghost_width);
                    self.cs[s3].s_loc = n6;
                    let n5 = self.cs[s4].s_max.wrapping_add(self.cs[s4].s_min) / 2;
                    self.cs[s3].s_max = n5.wrapping_add(self.m_ghost_length / 2);
                    self.cs[s3].s_min = n5.wrapping_sub(self.m_ghost_length / 2);
                    self.add_h_value(n7, n6, fix_int(20), fix_int(2), s4, s3);
                }
                s4 = self.cs[s4].s_nxt;
            }
            s4 = self.top_list();
            while s4 != NIL {
                let n7 = self.cs[s4].s_loc;
                if self.in_blue_band(n7, self.m_len_top_bands, &self.m_top_bands) {
                    let n6 = n7.wrapping_sub(self.m_ghost_width);
                    self.cs[s3].s_loc = n6;
                    let n5 = self.cs[s4].s_min.wrapping_add(self.cs[s4].s_max) / 2;
                    self.cs[s3].s_max = n5.wrapping_add(self.m_ghost_length / 2);
                    self.cs[s3].s_min = n5.wrapping_sub(self.m_ghost_length / 2);
                    self.add_h_value(n6, n7, fix_int(20), fix_int(2), s3, s4);
                }
                s4 = self.cs[s4].s_nxt;
            }
            if self.m_len_top_bands != 0 || self.m_len_bot_bands != 0 {
                let mut bb = ACBBox::new();
                self.find_path_b_box(&mut bb);
                let n3 = bb.ymax;
                let n8 = bb.ymin;
                let mut o = ClrSeg::new();
                self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
                o.csg_sn = self.m_csg_sn;
                o.s_type = S_LINE;
                o.s_elt = NIL;
                o.s_min = i32::MIN;
                o.s_max = i32::MAX;
                let object = self.new_seg(o);
                self.cs[s3].s_max = self.m_ghost_length / 2;
                self.cs[s3].s_min = -self.m_ghost_length / 2;
                let n2 = fix_int(2);
                let n = fix_int(20);
                if self.in_blue_band(n8, self.m_len_bot_bands, &self.m_bot_bands) {
                    self.cs[object].s_loc = n8;
                    let n6 = n8.wrapping_add(self.m_ghost_width);
                    self.cs[s3].s_loc = n6;
                    self.add_h_value(n8, n6, n, n2, object, s3);
                }
                if self.in_blue_band(n3, self.m_len_top_bands, &self.m_top_bands) {
                    self.cs[object].s_loc = n3;
                    let n6 = n3.wrapping_sub(self.m_ghost_width);
                    self.cs[s3].s_loc = n6;
                    self.add_h_value(n6, n3, n, n2, s3, object);
                }
            }
        }
        self.combine_values();
    }

    fn walk_sub_path(&self, p: usize, bl: bool, param: &mut Param) {
        let mut p2 = p;
        let mut cd = Cd::default();
        let mut fr = FltnRec::default();
        let rep = if bl { FRP_YEXTREME } else { FRP_NZWIND };
        while p2 != NIL {
            let e = &self.pe[p2];
            match e.typ {
                CURVETO => {
                    flat_report_proc(rep, param, cd);
                    let cd2 = Cd { x: e.x1, y: e.y1 };
                    let cd3 = Cd { x: e.x2, y: e.y2 };
                    let cd4 = Cd { x: e.x3, y: e.y3 };
                    let bl2 = if bl {
                        fr.report = FRP_YEXTREME;
                        y_extreme_short_cut_proc(cd, cd2, cd3, cd4, param, fr.report)
                    } else {
                        fr.report = FRP_NZWIND;
                        nz_wind_short_cut_proc(cd, cd2, cd3, cd4, param, fr.report)
                    };
                    if !bl2 {
                        fltn_curve(cd, cd2, cd3, cd4, &mut fr, param);
                    }
                    cd = cd4;
                }
                LINETO => {
                    flat_report_proc(rep, param, cd);
                    cd = Cd { x: e.x, y: e.y };
                }
                MOVETO => {
                    cd = Cd { x: e.x, y: e.y };
                }
                CLOSEPATH => {
                    flat_report_proc(rep, param, cd);
                    return;
                }
                _ => {}
            }
            p2 = self.pe[p2].next;
        }
    }

    fn collect_sub_path_info(&mut self) -> Result<(), ACException> {
        let (n_sub, n_elts) = self.count_sub_paths();
        self.m_num_sub_paths = n_sub;
        if self.m_num_sub_paths > MAX_NUM_SUBPATHS || n_elts > MAX_NUM_PATH_ELEMENTS {
            return Err(ACException);
        }
        let mut subs: Vec<SubPathInfo> = Vec::with_capacity(n_sub as usize);
        let mut n = 0;
        let mut p = self.m_path_end;
        while n < self.m_num_sub_paths && p != NIL {
            p = self.get_dest(p);
            if p == NIL {
                continue;
            }
            let mut bb = ACBBox::new();
            self.calc_subpath_b_box(&mut bb, p);
            n += 1;
            let tail = self.get_closed_by(p);
            subs.push(SubPathInfo { head: p, tail, start_pt: Cd { x: self.pe[p].x, y: self.pe[p].y }, end_pt: Cd::default(), bbox: bb, current_wind: 0, filled: false });
            if tail == NIL {
                continue;
            }
            let cd = self.get_end_point(self.pe[tail].prev);
            subs.last_mut().unwrap().end_pt = cd;
            p = self.pe[p].prev;
        }
        self.m_sub_paths = Some(subs);
        Ok(())
    }

    fn determine_current_wind(&self, subs: &mut [SubPathInfo], n: i32) {
        for i in 0..n as usize {
            let p = subs[i].head;
            let mut fe = FindExParam { y_max: i32::MIN, ..Default::default() };
            self.walk_sub_path(p, true, &mut Param::FindEx(&mut fe));
            let s = &mut subs[i];
            if fe.first_was_extreme {
                if fe.rise_pt.x != fe.before_first_sink_pt.x {
                    s.current_wind = if fe.rise_pt.x > fe.before_first_sink_pt.x { WD_CCW } else { WD_CW };
                    continue;
                }
                s.current_wind = wind_test(fe.prerise_pt, fe.before_first_sink_pt, fe.first_sink_pt);
                continue;
            }
            if fe.last_was_extreme {
                if fe.rise_pt.x != fe.last_pt.x {
                    s.current_wind = if fe.rise_pt.x > fe.last_pt.x { WD_CCW } else { WD_CW };
                    continue;
                }
                s.current_wind = wind_test(fe.prerise_pt, fe.last_pt, fe.first_sink_pt);
                continue;
            }
            s.current_wind = fe.wind_dir;
        }
    }

    fn nz_wind_test(&self, cd: Cd, s: &SubPathInfo) -> i32 {
        let mut nz = NZWindParam { test_pt: cd, testx: fix_to_flt(cd.x), testy: fix_to_flt(cd.y), test_to_right: s.start_pt.x < cd.x, ..Default::default() };
        self.walk_sub_path(s.head, false, &mut Param::NzWind(&mut nz));
        if nz.has_last_pt {
            let fp = nz.first_pt;
            nz_wind_proc(&mut nz, fp);
        }
        nz.wind_count
    }

    fn determine_fill_or_unfill(&self, subs: &mut [SubPathInfo], n: i32) {
        for i in 0..n as usize {
            let mut n2: i32 = 0;
            let cd = subs[i].start_pt;
            for j in 0..n as usize {
                let b = &subs[j].bbox;
                if i == j || !in_range(cd.x, b.xmin, b.xmax) || !in_range(cd.y, b.ymin, b.ymax) {
                    continue;
                }
                n2 = n2.wrapping_add(self.nz_wind_test(cd, &subs[j]));
            }
            n2 = if subs[i].current_wind == WD_CCW { n2.wrapping_add(1) } else { n2.wrapping_sub(1) };
            subs[i].filled = n2 != 0;
        }
    }

    fn flip_sub_path(&mut self, s: &mut SubPathInfo) {
        let mut n = 0;
        let mut n2 = 0;
        let mut p = s.head;
        let p2 = self.pe[p].prev;
        let p3 = self.pe[s.tail].next;
        let mut p4 = NIL;
        let mut p5 = NIL;
        loop {
            if p == NIL {
                s.head = p4;
                s.tail = p5;
                return;
            }
            let p6 = self.pe[p].next;
            let p7 = self.pe[p].prev;
            let mut skip_tail = false;
            match self.pe[p].typ {
                CURVETO => {
                    let e = &mut self.pe[p];
                    let (n3, n4, n5, n6) = (e.x1, e.y1, e.x3, e.y3);
                    e.x1 = e.x2;
                    e.y1 = e.y2;
                    e.x2 = n3;
                    e.y2 = n4;
                    e.x3 = n;
                    e.y3 = n2;
                    n = n5;
                    n2 = n6;
                }
                LINETO => {
                    let e = &mut self.pe[p];
                    let (n5, n6) = (e.x, e.y);
                    e.x = n;
                    e.y = n2;
                    n = n5;
                    n2 = n6;
                }
                MOVETO => {
                    p5 = p;
                    n = self.pe[p].x;
                    n2 = self.pe[p].y;
                    self.pe[p].typ = CLOSEPATH;
                    self.pe[p].next = p3;
                    if p3 != NIL {
                        self.pe[p3].prev = p;
                    } else {
                        self.m_path_end = p;
                    }
                    self.pe[p].prev = p6;
                    self.pe[p].next = p3;
                    skip_tail = true;
                }
                CLOSEPATH => {
                    p4 = p;
                    self.pe[p].typ = MOVETO;
                    self.pe[p].x = n;
                    self.pe[p].y = n2;
                    self.pe[p].x = self.pe[s.tail].x;
                    self.pe[p].y = self.pe[s.tail].y;
                    if p2 != NIL {
                        self.pe[p2].next = p;
                    } else {
                        self.m_path_start = p;
                    }
                    self.pe[p].next = p7;
                    self.pe[p].prev = p2;
                    s.head = p4;
                    s.tail = p5;
                    return;
                }
                _ => {}
            }
            if !skip_tail {
                self.pe[p].next = p7;
                self.pe[p].prev = p6;
            }
            p = p6;
        }
    }

    fn flip_sub_paths(&mut self, subs: &mut [SubPathInfo], n: i32, bl: bool) {
        for i in 0..n as usize {
            let bl2 = ((subs[i].current_wind == WD_CCW) ^ subs[i].filled) ^ bl;
            if bl2 {
                continue;
            }
            self.flip_sub_path(&mut subs[i]);
        }
    }

    fn fix_path_wind(&mut self, bl: bool) {
        let mut subs = self.m_sub_paths.take().unwrap_or_default();
        let n = self.m_num_sub_paths;
        self.determine_current_wind(&mut subs, n);
        self.determine_fill_or_unfill(&mut subs, n);
        self.flip_sub_paths(&mut subs, n, bl);
        self.m_sub_paths = Some(subs);
    }

    fn close_elements(&self, p: usize, p2: usize, mut n: i32, mut n2: i32, bl: bool) -> bool {
        if p == p2 {
            return true;
        }
        let n4 = ps_dist(20);
        if n < n2 {
            n = n.wrapping_sub(n4);
            n2 = n2.wrapping_add(n4);
        } else {
            let n3 = n;
            n = n2.wrapping_sub(n4);
            n2 = n3.wrapping_add(n4);
        }
        let mut p3 = p;
        loop {
            if p3 == p2 {
                return true;
            }
            let cd = self.get_end_point(p3);
            let n3 = if bl { cd.x } else { cd.y };
            if !(n3 <= n2 && n3 >= n) {
                return false;
            }
            p3 = if self.pe[p3].typ == CLOSEPATH { self.get_dest(p3) } else { self.pe[p3].next };
            if p3 == p {
                break;
            }
        }
        false
    }

    fn close_segs(&self, s: usize, s2: usize, bl: bool) -> bool {
        if s == s2 {
            return true;
        }
        let p = self.cs[s].s_elt;
        let p2 = self.cs[s2].s_elt;
        if p == NIL || p2 == NIL {
            return true;
        }
        let n2 = self.cs[s].s_loc;
        let n = self.cs[s2].s_loc;
        self.close_elements(p, p2, n2, n, bl) || self.close_elements(p2, p, n, n2, bl)
    }

    fn do_prune(&mut self) {
        let mut v = self.m_val_list;
        while v != NIL && self.cv[v].pruned {
            v = self.cv[v].v_nxt;
        }
        self.m_val_list = v;
        if v == NIL {
            return;
        }
        let mut v2 = v;
        v = self.cv[v].v_nxt;
        while v != NIL {
            if self.cv[v].pruned {
                v = self.cv[v].v_nxt;
                self.cv[v2].v_nxt = v;
                continue;
            }
            v2 = v;
            v = self.cv[v].v_nxt;
        }
    }

    fn prune_one(&mut self, v: usize, _bl: bool, _v2: usize, _n: i32) -> usize {
        self.cv[v].pruned = true;
        self.cv[v].v_nxt
    }

    fn prune_v_vals(&mut self) {
        let mut v = self.m_val_list;
        let n = ps_dist(10);
        while v != NIL {
            let mut bl = true;
            let mut bl2 = false;
            let mut bl3 = false;
            let n2 = self.cv[v].v_val;
            let n3 = self.cv[v].v_loc1;
            let n4 = self.cv[v].v_loc2;
            let s = self.cv[v].v_seg1;
            let s2 = self.cv[v].v_seg2;
            let mut v2 = self.m_val_list;
            while v2 != NIL {
                let n5 = self.cv[v2].v_val;
                let s3 = self.cv[v2].v_seg1;
                let s4 = self.cv[v2].v_seg2;
                let n6 = self.cv[v2].v_loc1;
                let n7 = self.cv[v2].v_loc2;
                if !(n6 == n3 && n7 == n4 || prune_le(n2, n5)) {
                    let close = if n2 < fix_int(100) && prune_much_gt(n2, n5) {
                        self.close_segs(s, s3, true) || self.close_segs(s2, s4, true)
                    } else {
                        self.close_segs(s, s3, true) && self.close_segs(s2, s4, true)
                    };
                    if n4.wrapping_add(n) >= n7 && n3.wrapping_sub(n) <= n6 && close {
                        v = self.prune_one(v, false, v2, 1);
                        bl = false;
                        break;
                    }
                    if s != NIL && s2 != NIL {
                        if n6.wrapping_sub(n3).wrapping_abs() < 256 {
                            if !bl3 && prune_lt(n2, n5) && n6.wrapping_sub(n7).wrapping_abs() < n3.wrapping_sub(n4).wrapping_abs() && self.close_segs(s, s3, true) {
                                bl3 = true;
                            }
                            if self.cs[s2].s_type == S_BEND && self.close_segs(s, s3, true) {
                                v = self.prune_one(v, false, v2, 2);
                                bl = false;
                                break;
                            }
                        }
                        if n7.wrapping_sub(n4).wrapping_abs() < 256 {
                            if !bl2 && prune_lt(n2, n5) && n6.wrapping_sub(n7).wrapping_abs() < n3.wrapping_sub(n4).wrapping_abs() && self.close_segs(s2, s4, true) {
                                bl2 = true;
                            }
                            if self.cs[s].s_type == S_BEND && self.close_segs(s2, s4, true) {
                                v = self.prune_one(v, false, v2, 3);
                                bl = false;
                                break;
                            }
                        }
                        if bl3 && bl2 {
                            v = self.prune_one(v, false, v2, 4);
                            bl = false;
                            break;
                        }
                    }
                }
                v2 = self.cv[v2].v_nxt;
            }
            if !bl {
                continue;
            }
            v = self.cv[v].v_nxt;
        }
        self.do_prune();
    }

    fn prune_h_vals(&mut self) {
        let mut v = self.m_val_list;
        let n = ps_dist(10);
        while v != NIL {
            let mut bl = true;
            let mut bl2 = false;
            let mut bl3 = false;
            let s = self.cv[v].v_seg1;
            let s2 = self.cv[v].v_seg2;
            let bl4 = self.cv[v].v_ghst;
            let n2 = self.cv[v].v_val;
            let n3 = self.cv[v].v_loc1;
            let n4 = self.cv[v].v_loc2;
            let bl5 = self.in_blue_band(n4, self.m_len_top_bands, &self.m_top_bands);
            let bl6 = self.in_blue_band(n3, self.m_len_bot_bands, &self.m_bot_bands);
            let mut v2 = self.m_val_list;
            while v2 != NIL {
                let s3 = self.cv[v2].v_seg1;
                let s4 = self.cv[v2].v_seg2;
                let n5 = self.cv[v2].v_val;
                if !bl4 && self.cv[v2].v_ghst && !prune_very_much_gt(n2, n5) {
                    v2 = self.cv[v2].v_nxt;
                    continue;
                }
                let n6 = self.cv[v2].v_loc1;
                let n7 = self.cv[v2].v_loc2;
                if n7 == n4 && n6 == n3 {
                    v2 = self.cv[v2].v_nxt;
                    continue;
                }
                let cond = !(!prune_gt(n2, n5)
                    || n4.wrapping_add(n) < n7
                    || n3.wrapping_sub(n) > n6
                    || !(if n2 < fix_int(100) && prune_much_gt(n2, n5) {
                        self.close_segs(s, s3, false) || self.close_segs(s2, s4, false)
                    } else {
                        self.close_segs(s, s3, false) && self.close_segs(s2, s4, false)
                    })
                    || n2 >= 4096 && (bl5 && n4 != n7 || bl6 && n3 != n6));
                if cond {
                    v = self.prune_one(v, true, v2, 5);
                    bl = false;
                    break;
                }
                if s != NIL && s2 != NIL {
                    if n6.wrapping_sub(n3).wrapping_abs() < 256 {
                        if prune_gt(n2, n5) && !bl5 && self.cs[s2].s_type == S_BEND && self.close_segs(s, s3, false) {
                            v = self.prune_one(v, true, v2, 6);
                            bl = false;
                            break;
                        }
                        if !bl2 && prune_lt(n2, n5) && n7.wrapping_sub(n6).wrapping_abs() < n4.wrapping_sub(n3).wrapping_abs() && self.close_segs(s, s3, false) {
                            bl2 = true;
                        }
                    }
                    if n7.wrapping_sub(n4).wrapping_abs() < 256 {
                        if prune_gt(n2, n5) && !bl6 && self.cs[s2].s_type == S_BEND && self.close_segs(s, s3, false) {
                            v = self.prune_one(v, true, v2, 7);
                            bl = false;
                            break;
                        }
                        if !bl3 && prune_lt(n2, n5) && n7.wrapping_sub(n6).wrapping_abs() < n4.wrapping_sub(n3).wrapping_abs() && self.close_segs(s2, s4, false) {
                            bl3 = true;
                        }
                    }
                    if bl2 && bl3 {
                        v = self.prune_one(v, true, v2, 8);
                        bl = false;
                        break;
                    }
                }
                v2 = self.cv[v2].v_nxt;
            }
            if !bl {
                continue;
            }
            v = self.cv[v].v_nxt;
        }
        self.do_prune();
    }

    fn find_best_vals(&mut self, mut v: usize) {
        while v != NIL {
            if self.cv[v].v_bst == NIL {
                let mut n = self.cv[v].v_val;
                let mut n2 = self.cv[v].v_spc;
                let mut v2 = v;
                let n3 = self.cv[v].v_loc1;
                let n4 = self.cv[v].v_loc2;
                let mut v3 = self.cv[v].v_nxt;
                let mut v4 = v;
                while v3 != NIL {
                    if self.cv[v3].v_bst == NIL && self.cv[v3].v_loc1 == n3 && self.cv[v3].v_loc2 == n4 {
                        if self.cv[v3].v_spc == n2 && self.cv[v3].v_val > n || self.cv[v3].v_spc > n2 {
                            n2 = self.cv[v3].v_spc;
                            n = self.cv[v3].v_val;
                            v2 = v3;
                        }
                        self.cv[v3].v_bst = v4;
                        v4 = v3;
                    }
                    v3 = self.cv[v3].v_nxt;
                }
                while v4 != NIL {
                    v3 = self.cv[v4].v_bst;
                    self.cv[v4].v_bst = v2;
                    v4 = v3;
                }
            }
            v = self.cv[v].v_nxt;
        }
    }

    fn replace_vals(&mut self, n: i32, n2: i32, n3: i32, n4: i32, v: usize, _bl: bool) {
        let mut v2 = self.m_val_list;
        while v2 != NIL {
            if self.cv[v2].v_loc1 == n && self.cv[v2].v_loc2 == n2 && !self.cv[v2].merge {
                self.cv[v2].v_loc1 = n3;
                self.cv[v2].v_loc2 = n4;
                self.cv[v2].v_val = self.cv[v].v_val;
                self.cv[v2].v_spc = self.cv[v].v_spc;
                self.cv[v2].v_bst = v;
                self.cv[v2].merge = true;
            }
            v2 = self.cv[v2].v_nxt;
        }
    }

    fn merge_vals(&mut self, bl: bool) {
        let vl = self.m_val_list;
        self.find_best_vals(vl);
        let mut v = self.m_val_list;
        while v != NIL {
            self.cv[v].merge = false;
            v = self.cv[v].v_nxt;
        }
        let (tb, lt) = (self.m_top_bands, self.m_len_top_bands);
        let (bb, lb) = (self.m_bot_bands, self.m_len_bot_bands);
        loop {
            let mut v2 = self.m_val_list;
            v = NIL;
            while v2 != NIL {
                if !self.cv[v2].merge && (v == NIL || self.compare_values(self.cv[v2].v_bst, self.cv[v].v_bst, 20, 0)) {
                    v = v2;
                }
                v2 = self.cv[v2].v_nxt;
            }
            if v == NIL {
                break;
            }
            self.cv[v].merge = true;
            let bl2 = self.cv[v].v_ghst;
            let n = self.cv[v].v_loc1;
            let n2 = self.cv[v].v_loc2;
            let s = self.cv[v].v_seg1;
            let s2 = self.cv[v].v_seg2;
            let v3 = self.cv[v].v_bst;
            let n3 = self.cv[v3].v_val;
            let n4 = self.cv[v3].v_spc;
            v2 = self.m_val_list;
            while v2 != NIL {
                if !self.cv[v2].merge && bl2 == self.cv[v2].v_ghst {
                    let n5 = self.cv[v2].v_loc1;
                    let n6 = self.cv[v2].v_loc2;
                    if n5 != n || n6 != n2 {
                        let v4 = self.cv[v2].v_bst;
                        let n7 = self.cv[v4].v_val;
                        let n8 = self.cv[v4].v_spc;
                        let ib = |x: i32, top: bool| if top { self.in_blue_band(x, lt, &tb) } else { self.in_blue_band(x, lb, &bb) };
                        let c = n6 == n2 && self.close_segs(s2, self.cv[v2].v_seg2, bl) && (bl || !ib(n2, true) && !ib(n5, false) && !ib(n, false))
                            || n5 == n && self.close_segs(s, self.cv[v2].v_seg1, bl) && (bl || !ib(n, false) && !ib(n2, true) && !ib(n6, true))
                            || n6.wrapping_sub(n2).wrapping_abs() <= self.m_max_merge && n5.wrapping_sub(n).wrapping_abs() <= self.m_max_merge && (bl || n2 == n6 || !ib(n6, true)) && (bl || n == n5 || !ib(n5, false));
                        if c {
                            if n4 == n8 && n7 == n3 && !bl {
                                if ib(n2, true) {
                                    if n2 > n6 {
                                        self.replace_vals(n5, n6, n, n2, v3, bl);
                                    }
                                } else if ib(n, false) && n < n5 {
                                    self.replace_vals(n5, n6, n, n2, v3, bl);
                                }
                            } else {
                                self.replace_vals(n5, n6, n, n2, v3, bl);
                            }
                        } else if n4 == n8 && s != NIL && s2 != NIL {
                            let s3 = self.cv[v2].v_seg1;
                            let s4 = self.cv[v2].v_seg2;
                            if s3 != NIL && s4 != NIL {
                                if n5.wrapping_sub(n).wrapping_abs() <= 256 && n6.wrapping_sub(n2).wrapping_abs() <= self.m_max_bend_merge {
                                    if self.cs[s4].s_type == S_BEND && (bl || !ib(n6, true)) {
                                        self.replace_vals(n5, n6, n, n2, v3, bl);
                                    }
                                } else if n6.wrapping_sub(n2).wrapping_abs() <= 256 && n5.wrapping_sub(n).wrapping_abs() <= self.m_max_bend_merge && n3 > n7 && self.cs[s3].s_type == S_BEND && (bl || !ib(n5, false)) {
                                    self.replace_vals(n5, n6, n, n2, v3, bl);
                                }
                            }
                        }
                    }
                }
                v2 = self.cv[v2].v_nxt;
            }
        }
    }

    /// `checkSmooth`, only reachable with `mDoSmoothing`, which nothing sets.
    fn check_smooth(&mut self) {
        unreachable!("AutoColor.checkSmooth needs mDoSmoothing")
    }

    fn check_for_multi_move_to(&mut self) {
        let mut p = self.m_path_start;
        let mut bl = false;
        while p != NIL {
            if self.pe[p].typ != MOVETO {
                bl = false;
            } else if !bl {
                bl = true;
            } else {
                let pv = self.pe[p].prev;
                self.delete(pv);
            }
            p = self.pe[p].next;
        }
    }

    fn delete(&mut self, p: usize) {
        let p2 = self.pe[p].next;
        let p3 = self.pe[p].prev;
        if p2 != NIL {
            self.pe[p2].prev = p3;
        } else {
            self.m_path_end = p3;
        }
        if p3 != NIL {
            self.pe[p3].next = p2;
        } else {
            self.m_path_start = p2;
        }
    }

    /// `checkBBoxEdge`, returning (frst, lst).
    fn check_b_box_edge(&self, p: usize, bl: bool, n: i32) -> (i32, i32) {
        let mut cdat = CheckData::default();
        let mut fr = FltnRec::default();
        let cd = self.get_end_point(self.pe[p].prev);
        fr.report = FRP_CHKBBDT;
        cdat.bbquit = false;
        let e = &self.pe[p];
        let cd2 = Cd { x: e.x1, y: e.y1 };
        let cd3 = Cd { x: e.x2, y: e.y2 };
        let cd4 = Cd { x: e.x3, y: e.y3 };
        cdat.loc = n;
        cdat.vert = bl;
        cdat.started = false;
        chk_bb_dt(&mut cdat, cd);
        fltn_curve(cd, cd2, cd3, cd4, &mut fr, &mut Param::Check(&mut cdat));
        (cdat.frst, cdat.lst)
    }

    fn init_pick(&mut self) {}

    fn lt_prune_b(&self, n: i32) -> bool {
        n < 256 && n << 10 < self.m_prune_b
    }

    fn consider_picking(&self, n: i32, n2: i32, v: usize, n3: i32) -> bool {
        if n > 0 {
            return true;
        }
        if v == NIL {
            return n2 >= self.m_prune_d;
        }
        if n2 > self.m_prune_a {
            return true;
        }
        if self.lt_prune_b(n2) {
            return false;
        }
        if n2 < i32::MAX / self.m_prune_c { n3 <= n2.wrapping_mul(self.m_prune_c) } else { n3 / self.m_prune_c <= n2 }
    }

    fn pick_v_vals(&mut self, mut v: usize) {
        let mut n = 0;
        let mut v4 = NIL;
        let mut v5 = NIL;
        let mut n2 = 0;
        'block0: loop {
            let mut v3 = v;
            let mut v6 = NIL;
            let mut v7 = NIL;
            let mut v8 = NIL;
            while v3 != NIL {
                if (v6 == NIL || self.compare_values(v3, v6, 1000, 0)) && self.consider_picking(self.cv[v3].v_spc, self.cv[v3].v_val, v5, n2) {
                    v6 = v3;
                    v7 = v8;
                    n = self.cv[v3].v_val;
                }
                v8 = v3;
                v3 = self.cv[v3].v_nxt;
            }
            if v6 == NIL {
                break;
            }
            if v7 == NIL {
                v = self.cv[v6].v_nxt;
            } else {
                self.cv[v7].v_nxt = self.cv[v6].v_nxt;
            }
            self.cv[v6].v_nxt = v5;
            v5 = v6;
            n2 = n;
            let n3 = self.cv[v6].v_loc1.wrapping_sub(self.m_band_margin);
            let n4 = self.cv[v6].v_loc2.wrapping_add(self.m_band_margin);
            v3 = v;
            v8 = NIL;
            loop {
                if v3 == NIL {
                    continue 'block0;
                }
                let n5 = self.cv[v3].v_loc1;
                let n6 = self.cv[v3].v_loc2;
                if n5 <= n4 && n6 >= n3 {
                    let v2 = self.cv[v3].v_nxt;
                    self.cv[v3].v_nxt = v4;
                    v4 = v3;
                    v3 = v2;
                    if v8 == NIL {
                        v = v3;
                        continue;
                    }
                    self.cv[v8].v_nxt = v3;
                    continue;
                }
                v8 = v3;
                v3 = self.cv[v3].v_nxt;
            }
        }
        let mut v3 = v;
        while v3 != NIL {
            let v2 = self.cv[v3].v_nxt;
            self.cv[v3].v_nxt = v4;
            v4 = v3;
            v3 = v2;
        }
        if v5 == NIL {
            let mut b = self.m_b_box;
            self.clr_v_bnds(&mut b);
            self.m_b_box = b;
        }
        self.m_vcoloring = v5;
    }

    fn in_serif_band(&self, mut n: i32, mut n2: i32, n3: i32, arr: &[i32]) -> bool {
        if n3 <= 0 {
            return false;
        }
        n = itfmy(n);
        n2 = itfmy(n2);
        if n > n2 {
            std::mem::swap(&mut n, &mut n2);
        }
        let mut i = 0;
        while i < n3 as usize {
            if !(arr[i] > n || arr[i + 1] < n2) {
                return true;
            }
            i += 2;
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn consider_val_for_seg(&self, v: usize, s: usize, n: i32, n2: i32, arr: &[i32], n3: i32, arr2: &[i32], bl: bool) -> bool {
        if bl && self.cv[v].v_spc as f64 > 0.0 {
            return true;
        }
        if self.in_blue_band(n, n2, arr) {
            return true;
        }
        if self.cv[v].v_spc as f64 <= 0.0 && self.in_serif_band(self.cs[s].s_max, self.cs[s].s_min, n3, arr2) {
            return false;
        }
        !self.lt_prune_b(self.cv[v].v_val)
    }

    #[allow(clippy::too_many_arguments)]
    fn fnd_bst_val(&self, s: usize, bl: bool, v: usize, v2: usize, n: i32, arr: &[i32], n2: i32, arr2: &[i32], bl2: bool, bl3: bool) -> usize {
        let mut v3 = NIL;
        let n3 = self.cs[s].s_loc;
        let mut v4 = v;
        loop {
            let v5 = v4;
            while v4 != NIL {
                let c = &self.cv[v4];
                let (s2, n4) = if bl { (c.v_seg1, c.v_loc1) } else { (c.v_seg2, c.v_loc2) };
                if n3.wrapping_sub(n4).wrapping_abs() <= self.m_max_merge
                    && (if bl2 { !c.v_ghst } else { s2 == s || self.close_segs(s, s2, !bl3) })
                    && (v3 == NIL || c.v_val == self.cv[v3].v_val && c.v_spc == self.cv[v3].v_spc && c.init_val > self.cv[v3].init_val || self.compare_values(v4, v3, 1000, 3))
                    && self.consider_val_for_seg(v4, s, n3, n, arr, n2, arr2, true)
                {
                    v3 = v4;
                }
                v4 = self.cv[v4].v_nxt;
            }
            if v5 == v2 {
                break;
            }
            v4 = v2;
        }
        v3
    }

    #[allow(clippy::too_many_arguments)]
    fn find_best_val_for_seg(&mut self, s: usize, bl: bool, v: usize, v2: usize, n: i32, arr: &[i32], n2: i32, arr2: &[i32], bl2: bool) -> usize {
        let mut v4 = NIL;
        let mut v5 = self.fnd_bst_val(s, bl, v, v2, n, arr, n2, arr2, false, bl2);
        if v5 != NIL && self.cv[v5].v_ghst {
            let v3 = self.fnd_bst_val(s, bl, v, v2, n, arr, n2, arr2, true, bl2);
            if v3 != NIL && self.cv[v3].v_val >= fix_int(2) {
                v4 = v5;
                v5 = v3;
            }
        }
        if v5 != NIL {
            if self.cv[v5].v_val < 16 && (v4 == NIL || self.cv[v4].v_val < 16) {
                v5 = NIL;
            } else {
                self.cv[v5].pruned = false;
            }
        }
        v5
    }

    fn memb_val_list(&self, v: usize, mut v2: usize) -> bool {
        while v2 != NIL {
            if v == v2 {
                return true;
            }
            v2 = self.cv[v2].v_nxt;
        }
        false
    }

    fn prev_val(&self, v: usize, mut v2: usize) -> usize {
        if v == v2 {
            return NIL;
        }
        let mut v3 = v2;
        loop {
            v2 = self.cv[v2].v_nxt;
            if v2 == v {
                break;
            }
            v3 = v2;
        }
        v3
    }

    fn pick_h_vals(&mut self, mut v: usize) {
        let mut n = 0;
        let mut v4 = NIL;
        let mut v5 = NIL;
        let mut n2 = 0;
        'block0: loop {
            let mut v3 = v;
            let mut v6 = NIL;
            let mut v7 = NIL;
            let mut v8 = NIL;
            while v3 != NIL {
                if (v6 == NIL || self.compare_values(v3, v6, 1000, 0)) && self.consider_picking(self.cv[v3].v_spc, self.cv[v3].v_val, v5, n2) {
                    v6 = v3;
                    v7 = v8;
                    n = self.cv[v3].v_val;
                }
                v8 = v3;
                v3 = self.cv[v3].v_nxt;
            }
            if v6 != NIL {
                let mut s = self.cv[v6].v_seg1;
                let mut s2 = self.cv[v6].v_seg2;
                if self.cv[v6].v_ghst {
                    v3 = v;
                    while v3 != NIL {
                        if self.cv[v3].v_loc2 == self.cv[v6].v_loc2 && self.cv[v3].v_loc1 == self.cv[v6].v_loc1 && !self.cv[v3].v_ghst {
                            s = self.cv[v3].v_seg1;
                            s2 = self.cv[v3].v_seg2;
                            break;
                        }
                        v3 = self.cv[v3].v_nxt;
                    }
                }
                if self.cs[s].s_type == S_GHOST {
                    let v9 = self.cs[s2].s_lnk;
                    if v9 != NIL && v9 != v6 && self.memb_val_list(v9, v) {
                        v6 = v9;
                        v7 = self.prev_val(v6, v);
                    }
                } else if self.cs[s2].s_type == S_GHOST {
                    let v9 = self.cs[s2].s_lnk;
                    if v9 != NIL && v9 != v6 && self.memb_val_list(v9, v) {
                        v6 = v9;
                        v7 = self.prev_val(v6, v);
                    }
                }
            }
            if v6 == NIL {
                break;
            }
            n2 = n;
            if v7 == NIL {
                v = self.cv[v6].v_nxt;
            } else {
                self.cv[v7].v_nxt = self.cv[v6].v_nxt;
            }
            self.cv[v6].v_nxt = v5;
            v5 = v6;
            let mut n3 = self.cv[v6].v_loc1;
            let mut n4 = self.cv[v6].v_loc2;
            if self.cv[v6].v_ghst {
                if self.cs[self.cv[v6].v_seg1].s_type == S_GHOST {
                    n3 = n4;
                } else {
                    n4 = n3;
                }
            }
            n3 = n3.wrapping_sub(self.m_band_margin);
            n4 = n4.wrapping_add(self.m_band_margin);
            v3 = v;
            v8 = NIL;
            loop {
                if v3 == NIL {
                    continue 'block0;
                }
                let mut n5 = self.cv[v3].v_loc1;
                let mut n6 = self.cv[v3].v_loc2;
                if self.cv[v3].v_ghst {
                    if self.cs[self.cv[v3].v_seg1].s_type == S_GHOST {
                        n5 = n6;
                    } else {
                        n6 = n5;
                    }
                }
                if n5 <= n4 && n6 >= n3 {
                    let v2 = self.cv[v3].v_nxt;
                    self.cv[v3].v_nxt = v4;
                    v4 = v3;
                    v3 = v2;
                    if v8 == NIL {
                        v = v3;
                        continue;
                    }
                    self.cv[v8].v_nxt = v3;
                    continue;
                }
                v8 = v3;
                v3 = self.cv[v3].v_nxt;
            }
        }
        let mut v3 = v;
        while v3 != NIL {
            let v2 = self.cv[v3].v_nxt;
            self.cv[v3].v_nxt = v4;
            v4 = v3;
            v3 = v2;
        }
        if v5 == NIL {
            let mut b = self.m_b_box;
            self.clr_h_bnds(&mut b);
            self.m_b_box = b;
        }
        self.m_hcoloring = v5;
    }

    #[allow(clippy::too_many_arguments)]
    fn find_best_val_for_segs(&mut self, mut s: usize, bl: bool, v: usize, v2: usize, n: i32, arr: &[i32], n2: i32, arr2: &[i32], bl2: bool) {
        while s != NIL {
            let v3 = self.find_best_val_for_seg(s, bl, v, v2, n, arr, n2, arr2, bl2);
            self.cs[s].s_lnk = v3;
            s = self.cs[s].s_nxt;
        }
    }

    fn set_pruned(&mut self) {
        let mut v = self.m_val_list;
        while v != NIL {
            self.cv[v].pruned = true;
            v = self.cv[v].v_nxt;
        }
    }

    fn find_best_h_vals(&mut self) {
        self.set_pruned();
        let (tb, bb) = (self.m_top_bands, self.m_bot_bands);
        let (t, b, vl) = (self.top_list(), self.bot_list(), self.m_val_list);
        self.find_best_val_for_segs(t, false, vl, NIL, self.m_len_top_bands, &tb, 0, &[], true);
        self.find_best_val_for_segs(b, true, vl, NIL, self.m_len_bot_bands, &bb, 0, &[], true);
        self.do_prune();
    }

    fn find_best_v_vals(&mut self) {
        self.set_pruned();
        let sf = self.m_serifs;
        let (l, r, vl) = (self.left_list(), self.right_list(), self.m_val_list);
        self.find_best_val_for_segs(l, true, vl, NIL, 0, &[], self.m_num_serifs, &sf, false);
        self.find_best_val_for_segs(r, false, vl, NIL, 0, &[], self.m_num_serifs, &sf, false);
        self.do_prune();
    }

    fn find_path_b_box(&mut self, bb: &mut ACBBox) {
        if let Some(p) = self.m_path_b_box {
            *bb = p;
            return;
        }
        let mut fr = FltnRec::default();
        let mut cd = Cd::default();
        if self.m_path_start == NIL {
            bb.xmin = 0;
            bb.ymin = 0;
            bb.xmax = 0;
            bb.ymax = 0;
            bb.pxmn = NIL;
            bb.pxmx = NIL;
            bb.pymn = NIL;
            bb.pymx = NIL;
            return;
        }
        fr.report = FRP_FPBBOXPT;
        bb.xmin = fix_int(10000);
        bb.ymin = bb.xmin;
        bb.xmax = -bb.xmin;
        bb.ymax = bb.xmax;
        let mut p = self.m_path_start;
        while p != NIL {
            let e = &self.pe[p];
            match e.typ {
                MOVETO | LINETO => {
                    cd = Cd { x: e.x, y: e.y };
                    bb.pe = p;
                    fp_b_box_pt(bb, cd);
                }
                CURVETO => {
                    let cd2 = Cd { x: e.x1, y: e.y1 };
                    let cd3 = Cd { x: e.x2, y: e.y2 };
                    let cd4 = Cd { x: e.x3, y: e.y3 };
                    bb.pe = p;
                    if !fp_b_box_curve_short_cut(cd, cd2, cd3, cd4, bb) {
                        fltn_curve(cd, cd2, cd3, cd4, &mut fr, &mut Param::BBox(&mut *bb));
                    }
                    cd = cd4;
                }
                _ => {}
            }
            p = self.pe[p].next;
        }
        bb.xmin = f_half_rnd(bb.xmin);
        bb.ymin = f_half_rnd(bb.ymin);
        bb.xmax = f_half_rnd(bb.xmax);
        bb.ymax = f_half_rnd(bb.ymax);
        self.m_path_b_box = Some(*bb);
    }

    fn calc_subpath_b_box(&self, bb: &mut ACBBox, mut p: usize) -> usize {
        let mut fr = FltnRec::default();
        let mut cd = Cd::default();
        if p == NIL {
            bb.xmin = 0;
            bb.ymin = 0;
            bb.xmax = 0;
            bb.ymax = 0;
            bb.pxmn = NIL;
            bb.pxmx = NIL;
            bb.pymn = NIL;
            bb.pymx = NIL;
            return NIL;
        }
        fr.report = FRP_FPBBOXPT;
        bb.xmin = fix_int(10000);
        bb.ymin = bb.xmin;
        bb.xmax = -bb.xmin;
        bb.ymax = bb.xmax;
        if self.pe[p].typ != MOVETO {
            p = self.get_dest(p);
        }
        while p != NIL {
            let e = &self.pe[p];
            match e.typ {
                MOVETO | LINETO => {
                    cd = Cd { x: e.x, y: e.y };
                    bb.pe = p;
                    fp_b_box_pt(bb, cd);
                }
                CURVETO => {
                    let cd2 = Cd { x: e.x1, y: e.y1 };
                    let cd3 = Cd { x: e.x2, y: e.y2 };
                    let cd4 = Cd { x: e.x3, y: e.y3 };
                    bb.pe = p;
                    if !fp_b_box_curve_short_cut(cd, cd2, cd3, cd4, bb) {
                        fltn_curve(cd, cd2, cd3, cd4, &mut fr, &mut Param::BBox(&mut *bb));
                    }
                    cd = cd4;
                }
                CLOSEPATH => {
                    p = self.pe[p].next;
                    bb.xmin = f_half_rnd(bb.xmin);
                    bb.ymin = f_half_rnd(bb.ymin);
                    bb.xmax = f_half_rnd(bb.xmax);
                    bb.ymax = f_half_rnd(bb.ymax);
                    return p;
                }
                _ => {}
            }
            p = self.pe[p].next;
        }
        bb.xmin = f_half_rnd(bb.xmin);
        bb.ymin = f_half_rnd(bb.ymin);
        bb.xmax = f_half_rnd(bb.xmax);
        bb.ymax = f_half_rnd(bb.ymax);
        p
    }

    fn find_subpath_b_box(&self, bb: &mut ACBBox, mut p: usize) -> usize {
        if let Some(subs) = &self.m_sub_paths {
            for i in 0..self.m_num_sub_paths as usize {
                if subs[i].head != p {
                    continue;
                }
                *bb = subs[i].bbox;
                p = subs[i].tail;
                if p != NIL {
                    p = self.pe[p].next;
                }
                return p;
            }
        }
        self.calc_subpath_b_box(bb, p)
    }

    /// `findCurveBBox`, returning (min, max).
    #[allow(clippy::too_many_arguments)]
    fn find_curve_b_box(&self, n: i32, n2: i32, n3: i32, n4: i32, n5: i32, n6: i32, n7: i32, n8: i32) -> (Cd, Cd) {
        let mut fr = FltnRec::default();
        let mut bb = ACBBox::new();
        fr.report = FRP_FPBBOXPT;
        bb.xmin = fix_int(10000);
        bb.ymin = bb.xmin;
        bb.xmax = -bb.xmin;
        bb.ymax = bb.xmax;
        let cd3 = Cd { x: n, y: n2 };
        let cd4 = Cd { x: n3, y: n4 };
        let cd5 = Cd { x: n5, y: n6 };
        let cd6 = Cd { x: n7, y: n8 };
        fp_b_box_pt(&mut bb, cd3);
        fltn_curve(cd3, cd4, cd5, cd6, &mut fr, &mut Param::BBox(&mut bb));
        (Cd { x: f_half_rnd(bb.xmin), y: f_half_rnd(bb.ymin) }, Cd { x: f_half_rnd(bb.xmax), y: f_half_rnd(bb.ymax) })
    }

    fn clr_v_bnds(&mut self, bb: &mut ACBBox) {
        if self.m_path_start == NIL || self.m_do_counters && v_color_char(self.m_unicode) {
            return;
        }
        self.find_path_b_box(bb);
        bb.v_mn = itfmx(bb.xmin);
        bb.v_mx = itfmx(bb.xmax);
        bb.pv_mn = bb.pxmn;
        bb.pv_mx = bb.pxmx;
        if bb.v_mn > bb.v_mx {
            std::mem::swap(&mut bb.v_mn, &mut bb.v_mx);
            std::mem::swap(&mut bb.pv_mn, &mut bb.pv_mx);
        }
        self.add_color_point(bb.v_mn, 0, bb.v_mx, 0, b'y', bb.pv_mn, bb.pv_mx);
    }

    fn re_clr_v_bnds(&mut self, bb: &ACBBox) {
        self.add_color_point(bb.v_mn, 0, bb.v_mx, 0, b'y', bb.pv_mn, bb.pv_mx);
    }

    fn clr_h_bnds(&mut self, bb: &mut ACBBox) {
        if self.m_path_start == NIL || self.m_do_counters && h_color_char(self.m_unicode) {
            return;
        }
        self.find_path_b_box(bb);
        bb.h_mn = itfmy(bb.ymin);
        bb.h_mx = itfmy(bb.ymax);
        bb.ph_mn = bb.pymn;
        bb.ph_mx = bb.pymx;
        if bb.h_mn > bb.h_mx {
            std::mem::swap(&mut bb.h_mn, &mut bb.h_mx);
            std::mem::swap(&mut bb.ph_mn, &mut bb.ph_mx);
        }
        self.add_color_point(0, bb.h_mn, 0, bb.h_mx, b'b', bb.ph_mn, bb.ph_mx);
    }

    fn re_clr_h_bnds(&mut self, bb: &ACBBox) {
        self.add_color_point(0, bb.h_mn, 0, bb.h_mx, b'b', bb.ph_mn, bb.ph_mx);
    }

    fn check_val_overlaps(&self, mut n: i32, mut n2: i32, mut v: usize, bl: bool) -> bool {
        if bl {
            n = itfmx(n);
            n2 = itfmx(n2);
        } else {
            n = itfmy(n);
            n2 = itfmy(n2);
        }
        if n > n2 {
            std::mem::swap(&mut n, &mut n2);
        }
        while v != NIL {
            let mut n4 = self.cv[v].v_loc1;
            let mut n5 = self.cv[v].v_loc2;
            if bl {
                n4 = itfmx(n4);
                n5 = itfmx(n5);
            } else {
                n4 = itfmy(n4);
                n5 = itfmy(n5);
            }
            if n4 > n5 {
                std::mem::swap(&mut n4, &mut n5);
            }
            if n4 <= n2 && n <= n5 {
                return true;
            }
            v = self.cv[v].v_nxt;
        }
        false
    }

    fn add_b_box_hv(&mut self, bl: bool, bl2: bool) {
        let mut p = self.m_path_start;
        while p != NIL {
            let mut bb = ACBBox::new();
            if bl2 {
                p = self.find_subpath_b_box(&mut bb, p);
            } else {
                self.find_path_b_box(&mut bb);
                p = NIL;
            }
            if !bl {
                if self.check_val_overlaps(bb.xmin, bb.xmax, self.m_vcoloring, true) {
                    continue;
                }
                let mut v = ClrVal::new();
                self.m_cvl_sn = self.m_cvl_sn.wrapping_add(1);
                v.cvl_sn = self.m_cvl_sn;
                let mut s2 = ClrSeg::new();
                self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
                s2.csg_sn = self.m_csg_sn;
                s2.s_loc = bb.xmin;
                s2.s_elt = bb.pxmn;
                s2.s_type = S_LINE;
                s2.s_min = bb.ymin;
                s2.s_max = bb.ymax;
                let s2 = self.new_seg(s2);
                let mut s = ClrSeg::new();
                self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
                s.csg_sn = self.m_csg_sn;
                s.s_loc = bb.xmax;
                s.s_elt = bb.pxmx;
                s.s_type = S_LINE;
                s.s_min = bb.ymin;
                s.s_max = bb.ymax;
                let s = self.new_seg(s);
                v.v_val = 100;
                v.v_spc = 0;
                v.v_loc1 = bb.xmin;
                v.v_loc2 = bb.xmax;
                v.v_seg1 = s2;
                v.v_seg2 = s;
                v.v_ghst = false;
                v.v_nxt = self.m_vcoloring;
                self.cv.push(v);
                let id = self.cv.len() - 1;
                self.cv[id].v_bst = id;
                self.m_vcoloring = id;
                continue;
            }
            if self.check_val_overlaps(bb.ymin, bb.ymax, self.m_hcoloring, false) {
                continue;
            }
            let mut v = ClrVal::new();
            self.m_cvl_sn = self.m_cvl_sn.wrapping_add(1);
            v.cvl_sn = self.m_cvl_sn;
            let mut s2 = ClrSeg::new();
            self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
            s2.csg_sn = self.m_csg_sn;
            s2.s_loc = bb.ymax;
            s2.s_elt = bb.pymx;
            s2.s_type = S_LINE;
            s2.s_min = bb.xmin;
            s2.s_max = bb.xmax;
            let s2 = self.new_seg(s2);
            let mut s = ClrSeg::new();
            self.m_csg_sn = self.m_csg_sn.wrapping_add(1);
            s.csg_sn = self.m_csg_sn;
            s.s_loc = bb.ymin;
            s.s_elt = bb.pymn;
            s.s_type = S_LINE;
            s.s_min = bb.xmin;
            s.s_max = bb.xmax;
            let s = self.new_seg(s);
            v.v_val = 100;
            v.v_spc = 0;
            v.v_loc1 = bb.ymax;
            v.v_loc2 = bb.ymin;
            v.v_seg1 = s2;
            v.v_seg2 = s;
            v.v_ghst = false;
            v.v_nxt = self.m_hcoloring;
            self.cv.push(v);
            let id = self.cv.len() - 1;
            self.cv[id].v_bst = id;
            self.m_hcoloring = id;
        }
    }

    fn check_b_boxes(&self, mut p: usize, mut p2: usize) -> bool {
        let mut bb = ACBBox::new();
        p = self.get_dest(p);
        p2 = self.get_dest(p2);
        if p == p2 {
            return true;
        }
        self.find_subpath_b_box(&mut bb, p);
        let (n, n2, n3, n4) = (bb.xmin, bb.xmax, bb.ymin, bb.ymax);
        self.find_subpath_b_box(&mut bb, p2);
        n <= bb.xmin && bb.xmax <= n2 && n3 <= bb.ymin && bb.ymax <= n4 || n >= bb.xmin && bb.xmax >= n2 && n3 >= bb.ymin && bb.ymax >= n4
    }

    fn new_best(&self, wd: &mut WriteData, c: usize) {
        let p = &self.cp[c];
        wd.bst = c;
        wd.bch = p.c;
        if wd.bch == b'y' || wd.bch == b'm' {
            wd.bst_b = true;
            wd.bx = p.x0.min(p.x1);
        } else {
            wd.bst_b = false;
            wd.by = p.y0.min(p.y1);
        }
    }

    fn write_point_item(&self, wd: &mut WriteData, c: usize) {
        let p = &self.cp[c];
        let (n3, mut n2, mut n) = match p.c {
            b'v' => (4, p.y0, p.y1),
            b'b' => (0, p.y0, p.y1),
            b'm' => (5, p.x0, p.x1),
            b'y' => (1, p.x0, p.x1),
            _ => return,
        };
        let n4 = n.wrapping_sub(n2);
        if n4 == fix_int(TOPGHST) {
            n2 = self.un_scale_abs(n2);
            n = n2.wrapping_add(n4);
        } else if n4 == fix_int(BOTGHST) {
            n = self.un_scale_abs(n);
            n2 = n.wrapping_sub(n4);
        } else {
            n2 = self.un_scale_abs(n2);
            n = self.un_scale_abs(n);
        }
        wd.hint_array.get_or_insert_with(Vec::new).push(HintData { flags: n3, v1: n2, v2: n });
    }

    fn flush_hint_array(&self, list: &Option<Vec<HintData>>) {
        if let Some(oc) = &self.m_outline_consumer {
            let list = list.as_ref().expect("java.lang.NullPointerException in AutoColor.flushHintArray");
            for (i, h) in list.iter().enumerate() {
                let mut n = h.flags;
                if i == 0 {
                    n |= AC_GC_NEW_HINTS;
                }
                let d = fix_to_flt(h.v1);
                let d2 = fix_to_flt(h.v2);
                oc.borrow_mut().stem(d, d2, (n & AC_GC_NEW_HINTS) != 0, (n & 1) != 0, false);
            }
        }
    }

    fn compare_hint_array(a: &Option<Vec<HintData>>, b: &Option<Vec<HintData>>) -> bool {
        let n2 = a.as_ref().map_or(0, |v| v.len());
        let n = b.as_ref().map_or(0, |v| v.len());
        if n2 != n {
            return false;
        }
        for i in 0..n2 {
            if a.as_ref().unwrap()[i] != b.as_ref().unwrap()[i] {
                return false;
            }
        }
        true
    }

    fn wrt_pnt_lst(&mut self, wd: &mut WriteData, c: usize) {
        let c2 = c;
        let mut q = c;
        while q != NIL {
            self.cp[q].done = false;
            q = self.cp[q].next;
        }
        loop {
            q = c2;
            wd.bst = NIL;
            while q != NIL {
                if !self.cp[q].done {
                    self.new_best(wd, q);
                    break;
                }
                q = self.cp[q].next;
            }
            if wd.bst == NIL {
                break;
            }
            q = self.cp[wd.bst].next;
            while q != NIL {
                if !self.cp[q].done {
                    let ch = self.cp[q].c;
                    if ch > wd.bch {
                        self.new_best(wd, q);
                    } else if ch == wd.bch {
                        if wd.bst_b {
                            if self.cp[q].x0.min(self.cp[q].x1) < wd.bx {
                                self.new_best(wd, q);
                            }
                        } else if self.cp[q].y0.min(self.cp[q].y1) < wd.by {
                            self.new_best(wd, q);
                        }
                    }
                }
                q = self.cp[q].next;
            }
            let b = wd.bst;
            self.cp[b].done = true;
            self.write_point_item(wd, b);
        }
    }

    fn wrt_new_clrs(&mut self, wd: &mut WriteData, p: usize) {
        if !wd.wrt_color_info {
            return;
        }
        wd.hint_array = None;
        let l = self.m_pt_lst_array[self.pe[p].newcolors as usize];
        self.wrt_pnt_lst(wd, l);
        if !Self::compare_hint_array(&wd.prev_hint_array, &wd.hint_array) {
            self.flush_hint_array(&wd.hint_array);
            wd.prev_hint_array = Some(wd.hint_array.clone().expect("java.lang.NullPointerException in AutoColor.wrtNewClrs"));
        }
    }

    fn mt(&mut self, wd: &mut WriteData, cd: Cd, p: usize) {
        if let Some(oc) = self.m_outline_consumer.clone() {
            if self.pe[p].newcolors != 0 {
                self.wrt_new_clrs(wd, p);
            }
            oc.borrow_mut().moveto(fix_to_flt(cd.x), fix_to_flt(cd.y));
        }
    }

    fn dt(&mut self, wd: &mut WriteData, cd: Cd, p: usize) {
        if let Some(oc) = self.m_outline_consumer.clone() {
            if self.pe[p].newcolors != 0 {
                self.wrt_new_clrs(wd, p);
            }
            oc.borrow_mut().lineto(fix_to_flt(cd.x), fix_to_flt(cd.y));
        }
    }

    fn ct(&mut self, wd: &mut WriteData, cd: Cd, cd2: Cd, cd3: Cd, p: usize) {
        if let Some(oc) = self.m_outline_consumer.clone() {
            if self.pe[p].newcolors != 0 {
                self.wrt_new_clrs(wd, p);
            }
            if self.pe[p].is_flex {
                unreachable!("AutoColor flex output needs mFlexOK");
            }
            oc.borrow_mut().curveto(fix_to_flt(cd.x), fix_to_flt(cd.y), fix_to_flt(cd2.x), fix_to_flt(cd2.y), fix_to_flt(cd3.x), fix_to_flt(cd3.y));
        }
    }

    fn cp_(&mut self, wd: &mut WriteData, p: usize) {
        if self.pe[p].newcolors != 0 {
            self.wrt_new_clrs(wd, p);
        }
    }

    fn begin_glyph(&mut self) {}

    fn end_glyph(&mut self) {
        if let Some(oc) = &self.m_outline_consumer {
            oc.borrow_mut().endchar();
        }
    }

    fn write_width(&mut self) -> bool {
        if let Some(oc) = &self.m_outline_consumer {
            return oc.borrow_mut().width(self.m_glyph_width);
        }
        true
    }

    fn number_path(&mut self) {
        let mut p = self.m_path_start;
        let mut s: i16 = 1;
        while p != NIL {
            self.pe[p].count = s;
            s = s.wrapping_add(1);
            p = self.pe[p].next;
        }
    }

    fn write_glyph(&mut self) {
        if !self.m_do_write_glyph {
            return;
        }
        let mut p = self.m_path_start;
        let mut wd = WriteData { first_flex: false, wrt_color_info: false, bst: NIL, bch: 0, bx: 0, by: 0, bst_b: false, hint_array: None, prev_hint_array: None };
        wd.wrt_color_info = self.m_path_start != NIL && self.m_path_start != self.m_path_end;
        self.number_path();
        self.begin_glyph();
        if !self.write_width() {
            return;
        }
        if wd.wrt_color_info && self.pe[p].newcolors == 0 {
            let l = self.m_pt_lst_array[0];
            self.wrt_pnt_lst(&mut wd, l);
            self.flush_hint_array(&wd.hint_array);
            wd.prev_hint_array = Some(wd.hint_array.clone().expect("java.lang.NullPointerException in AutoColor.writeGlyph"));
        }
        wd.first_flex = true;
        while p != NIL {
            let e = self.pe[p].clone();
            match e.typ {
                CURVETO => {
                    let cd = Cd { x: self.un_scale_abs(itfmx(e.x1)), y: self.un_scale_abs(itfmy(e.y1)) };
                    let cd2 = Cd { x: self.un_scale_abs(itfmx(e.x2)), y: self.un_scale_abs(itfmy(e.y2)) };
                    let cd3 = Cd { x: self.un_scale_abs(itfmx(e.x3)), y: self.un_scale_abs(itfmy(e.y3)) };
                    self.ct(&mut wd, cd, cd2, cd3, p);
                }
                LINETO => {
                    let cd = Cd { x: self.un_scale_abs(itfmx(e.x)), y: self.un_scale_abs(itfmy(e.y)) };
                    self.dt(&mut wd, cd, p);
                }
                MOVETO => {
                    let cd = Cd { x: self.un_scale_abs(itfmx(e.x)), y: self.un_scale_abs(itfmy(e.y)) };
                    self.mt(&mut wd, cd, p);
                }
                CLOSEPATH => self.cp_(&mut wd, p),
                _ => {}
            }
            p = self.pe[p].next;
        }
        self.end_glyph();
    }

    fn init_fix(&mut self) {
        self.m_v_fix_count = 0;
        self.m_h_fix_count = 0;
    }

    /// `recordForFix`, only reachable with `mAutoHFix`/`mAutoVFix`, which nothing sets.
    fn record_for_fix(&mut self, _bl: bool, _n: i32, _n2: i32, _n3: i32, _n4: i32) {
        unreachable!("AutoColor.recordForFix needs mAutoHFix or mAutoVFix")
    }

    fn find_line_seg(&self, n: i32, mut s: usize) -> bool {
        while s != NIL {
            if self.cs[s].s_loc == n && self.cs[s].s_type == S_LINE {
                return true;
            }
            s = self.cs[s].s_nxt;
        }
        false
    }

    fn check_val(&mut self, v: usize, bl: bool) {
        let (arr, n3, n2, n) = if bl {
            (self.m_v_stems, self.m_num_v_stems, itfmx(self.cv[v].v_loc1), itfmx(self.cv[v].v_loc2))
        } else {
            (self.m_h_stems, self.m_num_h_stems, itfmy(self.cv[v].v_loc1), itfmy(self.cv[v].v_loc2))
        };
        let n4 = n.wrapping_sub(n2).wrapping_abs();
        let mut n5 = fix_int(1000);
        let mut n6 = 0;
        for i in 0..n3 as usize {
            let n7 = arr[i];
            let n8 = n7.wrapping_sub(n4).wrapping_abs();
            if n8 >= n5 {
                continue;
            }
            n5 = n8;
            n6 = n7;
            if n5 == 0 {
                break;
            }
        }
        if n5 == 0 || n5 > fix_int(2) {
            return;
        }
        if bl && self.m_auto_v_fix || !bl && self.m_auto_h_fix {
            self.record_for_fix(bl, n4, n6, n2, n);
        }
    }

    fn check_vals(&mut self, mut v: usize, bl: bool) {
        while v != NIL {
            self.check_val(v, bl);
            v = self.cv[v].v_nxt;
        }
    }

    fn do_fixes(&mut self) -> bool {
        if self.m_h_fix_count > 0 && self.m_auto_h_fix || self.m_v_fix_count > 0 && self.m_auto_v_fix {
            unreachable!("AutoColor.fixHs/fixVs need mAutoHFix or mAutoVFix");
        }
        false
    }

    fn get_dest(&self, mut p: usize) -> usize {
        if p == NIL {
            return NIL;
        }
        loop {
            p = self.pe[p].prev;
            if p == NIL {
                return self.m_path_start;
            }
            if self.pe[p].typ == MOVETO {
                return p;
            }
        }
    }

    fn get_closed_by(&self, mut p: usize) -> usize {
        if p == NIL {
            return NIL;
        }
        if self.pe[p].typ == CLOSEPATH {
            return p;
        }
        loop {
            p = self.pe[p].next;
            if p == NIL {
                return NIL;
            }
            if self.pe[p].typ == MOVETO {
                return NIL;
            }
            if self.pe[p].typ == CLOSEPATH {
                return p;
            }
        }
    }

    fn get_end_point(&self, mut p: usize) -> Cd {
        if p == NIL {
            return Cd { x: 0, y: 0 };
        }
        loop {
            let e = &self.pe[p];
            match e.typ {
                MOVETO | LINETO => return Cd { x: e.x, y: e.y },
                CURVETO => return Cd { x: e.x3, y: e.y3 },
                CLOSEPATH => p = self.get_dest(p),
                _ => {}
            }
        }
    }

    /// `getEndPoints`: (end of `p.prev`, end of `p`).
    fn get_end_points(&self, p: usize) -> (Cd, Cd) {
        let cd2 = self.get_end_point(p);
        let cd = self.get_end_point(self.pe[p].prev);
        (cd, cd2)
    }

    fn is_tiny(&self, p: usize) -> bool {
        let (cd, cd2) = self.get_end_points(p);
        cd.x.wrapping_sub(cd2.x).wrapping_abs() < 256 && cd.y.wrapping_sub(cd2.y).wrapping_abs() < 256
    }

    fn is_short(&self, p: usize) -> bool {
        let (cd, cd2) = self.get_end_points(p);
        let n3 = cd.x.wrapping_sub(cd2.x).wrapping_abs();
        let n4 = cd.y.wrapping_sub(cd2.y).wrapping_abs();
        let (n2, n) = if n3 > n4 { (n4, n3) } else { (n3, n4) };
        n.wrapping_add(n2.wrapping_mul(42) / 125) < fix_int(3)
    }

    /// `nxtForBend`, returning (element, cd, cd2).
    fn nxt_for_bend(&self, p: usize) -> (usize, Cd, Cd) {
        let mut p2 = NIL;
        let cd3 = self.get_end_point(p);
        let mut p3 = p;
        loop {
            if self.pe[p3].typ == CLOSEPATH {
                p3 = self.get_dest(p3);
                if p2 != NIL && p2 == p3 {
                    p3 = NIL;
                } else {
                    p2 = p3;
                    p3 = self.pe[p3].next;
                }
            } else {
                p3 = self.pe[p3].next;
            }
            if p3 == NIL {
                let s = fix_int(-9999);
                return (NIL, Cd { x: s, y: s }, Cd { x: s, y: s });
            }
            if !self.is_tiny(p3) {
                break;
            }
        }
        let cd = if self.pe[p3].typ == CURVETO {
            let e = &self.pe[p3];
            let (mut n, mut n2) = (e.x1, e.y1);
            if n == cd3.x && n2 == cd3.y {
                n = e.x2;
                n2 = e.y2;
            }
            Cd { x: n, y: n2 }
        } else {
            self.get_end_point(p3)
        };
        let cd2 = self.get_end_point(p3);
        (p3, cd, cd2)
    }

    /// `prvForBend`, returning (element, cd).
    fn prv_for_bend(&self, p: usize) -> (usize, Cd) {
        let s = fix_int(-9999);
        let mut p2 = NIL;
        let mut p3 = p;
        loop {
            p3 = self.pe[p3].prev;
            if p3 == NIL {
                return (NIL, Cd { x: s, y: s });
            }
            if self.pe[p3].typ == MOVETO {
                p3 = self.get_closed_by(p3);
                if p3 == NIL || p2 != NIL && p2 == p3 {
                    return (p3, Cd { x: s, y: s });
                }
                p2 = p3;
            }
            if !self.is_tiny(p3) {
                break;
            }
        }
        if self.pe[p3].typ == CURVETO {
            let e = &self.pe[p3];
            let (mut n, mut n2) = (e.x2, e.y2);
            if n == e.x3 && n2 == e.y3 {
                n = e.x1;
                n2 = e.y1;
            }
            return (p3, Cd { x: n, y: n2 });
        }
        let pv = self.pe[p3].prev;
        if pv == NIL {
            return (p3, Cd { x: s, y: s });
        }
        (p3, self.get_end_point(pv))
    }

    fn check_height(&self, bl: bool, p: usize) -> bool {
        let mut p2 = self.m_path_start;
        let n = itfmy(self.pe[p].y);
        while p2 != NIL {
            if self.pe[p2].typ == MOVETO && p2 != p {
                let n2 = itfmy(self.pe[p2].y);
                if bl && n2 > n || !bl && n2 < n {
                    return false;
                }
            }
            p2 = self.pe[p2].next;
        }
        true
    }

    fn is_lower(&self, p: usize) -> bool {
        self.check_height(false, p)
    }

    fn is_upper(&self, p: usize) -> bool {
        self.check_height(true, p)
    }

    /// `countSubPaths`, returning (subpaths, elements).
    fn count_sub_paths(&self) -> (i32, i32) {
        let mut n = 0;
        let mut n2 = 0;
        let mut p = self.m_path_end;
        while p != NIL {
            loop {
                p = self.pe[p].prev;
                n2 += 1;
                if !(p != NIL && self.pe[p].typ != MOVETO) {
                    break;
                }
            }
            n += 1;
            if p == NIL {
                continue;
            }
            p = self.pe[p].prev;
        }
        (n, n2)
    }

    fn round_path_coords(&mut self) {
        let mut p = self.m_path_start;
        while p != NIL {
            let e = &mut self.pe[p];
            if e.typ == CURVETO {
                e.x1 = f_half_rnd(e.x1);
                e.y1 = f_half_rnd(e.y1);
                e.x2 = f_half_rnd(e.x2);
                e.y2 = f_half_rnd(e.y2);
                e.x3 = f_half_rnd(e.x3);
                e.y3 = f_half_rnd(e.y3);
            } else if e.typ == LINETO || e.typ == MOVETO {
                e.x = f_half_rnd(e.x);
                e.y = f_half_rnd(e.y);
            }
            p = self.pe[p].next;
        }
    }

    fn check_for_clr(&self) -> i32 {
        let mut p = self.m_path_start;
        while p != NIL {
            let p2 = self.get_closed_by(p);
            p = self.pe[p2].next;
        }
        0
    }

    fn pre_check_for_coloring(&mut self) -> bool {
        let mut n2 = 0;
        while self.m_path_end != NIL {
            if self.pe[self.m_path_end].typ == MOVETO {
                let e = self.m_path_end;
                self.delete(e);
                continue;
            }
            if self.pe[self.m_path_end].typ == CLOSEPATH {
                break;
            }
            return false;
        }
        let mut p = self.m_path_start;
        while p != NIL {
            if self.pe[p].typ == CLOSEPATH {
                if p == self.m_path_end {
                    break;
                }
                let p2 = self.pe[p].next;
                if self.pe[p2].typ == MOVETO {
                    p = p2;
                    continue;
                }
                if self.pe[p2].typ == CLOSEPATH {
                    self.delete(p2);
                    continue;
                }
            }
            p = self.pe[p].next;
        }
        loop {
            let n = self.check_for_clr();
            if n == -1 {
                return false;
            }
            if n == 0 {
                break;
            }
            n2 += 1;
            if n2 > 10 {
                break;
            }
        }
        true
    }

    /// `autoAddFlex`, only reachable with `mFlexOK`, which nothing sets.
    fn auto_add_flex(&mut self) {
        unreachable!("AutoColor.autoAddFlex needs mFlexOK")
    }

    fn init_shuffle_subpaths(&mut self) {
        let mut n: i32 = -1;
        let mut p = self.m_path_start;
        while p != NIL {
            if self.pe[p].typ == MOVETO {
                n += 1;
            }
            self.pe[p].count = n as i16;
            p = self.pe[p].next;
        }
        n += 1;
        self.m_row_cnt = n;
        self.m_links = if !(4..100).contains(&n) { None } else { Some(vec![0u8; (n * n) as usize]) };
    }

    fn mark_links(&mut self, mut v: usize, _bl: bool) {
        if self.m_links.is_none() {
            return;
        }
        while v != NIL {
            let s = self.cv[v].v_seg1;
            if s != NIL {
                let p = self.cs[s].s_elt;
                if p != NIL {
                    let s2c = self.pe[p].count as i32;
                    let sg = self.cv[v].v_seg2;
                    if sg != NIL {
                        let p2 = self.cs[sg].s_elt;
                        if p2 != NIL {
                            let sc = self.pe[p2].count as i32;
                            if s2c != sc {
                                let rc = self.m_row_cnt;
                                let links = self.m_links.as_mut().unwrap();
                                links[(rc * s2c + sc) as usize] = 1;
                                links[(rc * sc + s2c) as usize] = 1;
                            }
                        }
                    }
                }
            }
            v = self.cv[v].v_nxt;
        }
    }

    /// `doShuffleSubpaths`, only reachable with `mEditChar`, which neither caller sets.
    fn do_shuffle_subpaths(&mut self) {
        unreachable!("AutoColor.doShuffleSubpaths needs AC_ALLOWEDIT")
    }

    fn add_v_stem(&mut self, n: i32, n2: i32, bl: bool) {
        if !bl || self.m_all_stems {
            self.m_zone_data.push(ZN_VSTEM);
            self.m_zone_data.push(f_trunc(f_rnd(n)));
            self.m_zone_data.push(f_trunc(f_rnd(n2)));
        }
    }

    fn add_h_stem(&mut self, n: i32, n2: i32, bl: bool) {
        if !bl || self.m_all_stems {
            self.m_zone_data.push(ZN_HSTEM);
            self.m_zone_data.push(f_trunc(f_rnd(n)));
            self.m_zone_data.push(f_trunc(f_rnd(n2)));
        }
    }

    fn add_char_extreme(&mut self, n: i32, n2: i32) {
        self.m_zone_data.push(ZN_CHAREXTREME);
        self.m_zone_data.push(f_trunc(f_rnd(n2)));
        self.m_zone_data.push(f_trunc(f_rnd(n)));
    }

    fn add_char_zone(&mut self, n: i32, n2: i32) {
        self.m_zone_data.push(ZN_CHARZONE);
        self.m_zone_data.push(f_trunc(f_rnd(n2)));
        self.m_zone_data.push(f_trunc(f_rnd(n)));
    }

    fn add_zone(&mut self, n: i32, n2: i32) {
        self.m_zone_data.push(ZN_ZONE);
        self.m_zone_data.push(f_trunc(f_rnd(n2)));
        self.m_zone_data.push(f_trunc(f_rnd(n)));
    }

    fn append_element(&mut self, n: i16) -> usize {
        let mut e = PathElt::new();
        self.m_elt_sn = self.m_elt_sn.wrapping_add(1);
        e.elt_sn = self.m_elt_sn;
        if e.elt_sn == 0 {
            e.elt_sn += 1;
        }
        e.typ = n;
        self.pe.push(e);
        let id = self.pe.len() - 1;
        if self.m_input_path_end != NIL {
            let end = self.m_input_path_end;
            self.pe[end].next = id;
            self.pe[id].prev = end;
        } else {
            self.m_input_path_start = id;
        }
        self.m_input_path_end = id;
        id
    }

    fn close_sub_path(&mut self) {
        if self.m_sub_path_open {
            self.m_sub_path_open = false;
            self.append_element(CLOSEPATH);
        }
    }

    fn emit_raw_path(&mut self) {
        if let Some(oc) = self.m_outline_consumer.clone() {
            let mut p = self.m_path_start;
            while p != NIL {
                let e = self.pe[p].clone();
                match e.typ {
                    MOVETO => {
                        let d = fix_to_flt(self.un_scale_abs(itfmx(e.x)));
                        let d2 = fix_to_flt(self.un_scale_abs(itfmx(e.y)));
                        oc.borrow_mut().moveto(d, d2);
                    }
                    LINETO => {
                        let d = fix_to_flt(self.un_scale_abs(itfmx(e.x)));
                        let d2 = fix_to_flt(self.un_scale_abs(itfmx(e.y)));
                        oc.borrow_mut().lineto(d, d2);
                    }
                    CURVETO => {
                        let d = fix_to_flt(self.un_scale_abs(itfmx(e.x1)));
                        let d2 = fix_to_flt(self.un_scale_abs(itfmy(e.y1)));
                        let d3 = fix_to_flt(self.un_scale_abs(itfmx(e.x2)));
                        let d4 = fix_to_flt(self.un_scale_abs(itfmy(e.y2)));
                        let d5 = fix_to_flt(self.un_scale_abs(itfmx(e.x3)));
                        let d6 = fix_to_flt(self.un_scale_abs(itfmy(e.y3)));
                        oc.borrow_mut().curveto(d, d2, d3, d4, d5, d6);
                    }
                    _ => {}
                }
                p = self.pe[p].next;
            }
            oc.borrow_mut().endchar();
        }
    }

    fn reset_glyph(&mut self) {
        self.m_path_end = NIL;
        self.m_path_start = NIL;
        self.m_cur_y = 0.0;
        self.m_cur_x = 0.0;
        let mut p = self.m_input_path_start;
        while p != NIL {
            let e = self.pe[p].clone();
            self.pe.push(e);
            let p2 = self.pe.len() - 1;
            if self.m_path_end != NIL {
                let end = self.m_path_end;
                self.pe[end].next = p2;
                self.pe[p2].prev = end;
            } else {
                self.m_path_start = p2;
            }
            self.m_path_end = p2;
            p = self.pe[p].next;
        }
    }

    fn copy_bands(&mut self) {
        self.m_len_top_bands = self.m_top_zones.len().min(MAXBLUES) as i32;
        for n in 0..self.m_len_top_bands as usize {
            self.m_top_bands[n] = self.scale_abs(fix_int(self.m_top_zones[n]));
        }
        self.m_len_bot_bands = self.m_bottom_zones.len().min(MAXBLUES) as i32;
        for n in 0..self.m_len_bot_bands as usize {
            self.m_bot_bands[n] = self.scale_abs(fix_int(self.m_bottom_zones[n]));
        }
    }

    fn left_list(&self) -> usize {
        self.m_seg_lists[0]
    }

    fn right_list(&self) -> usize {
        self.m_seg_lists[1]
    }

    fn top_list(&self) -> usize {
        self.m_seg_lists[2]
    }

    fn bot_list(&self) -> usize {
        self.m_seg_lists[3]
    }
}

impl OutlineConsumer for AutoColor {
    fn set_matrix(&mut self, m: &Matrix) {
        if let Some(oc) = &self.m_outline_consumer {
            oc.borrow_mut().set_matrix(m);
        }
    }

    fn moveto(&mut self, x: f64, y: f64) {
        self.close_sub_path();
        let p = self.append_element(MOVETO);
        self.pe[p].x = tfmx(self.scale_abs(flt_to_fix(x)));
        self.pe[p].y = tfmy(self.scale_abs(flt_to_fix(y)));
        self.m_cur_x = x;
        self.m_cur_y = y;
        self.m_sub_path_open = true;
    }

    fn lineto(&mut self, x: f64, y: f64) {
        let p = self.append_element(LINETO);
        self.pe[p].x = tfmx(self.scale_abs(flt_to_fix(x)));
        self.pe[p].y = tfmy(self.scale_abs(flt_to_fix(y)));
        self.m_cur_x = x;
        self.m_cur_y = y;
    }

    fn curveto(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        let p = self.append_element(CURVETO);
        self.pe[p].x1 = tfmx(self.scale_abs(flt_to_fix(x1)));
        self.pe[p].y1 = tfmy(self.scale_abs(flt_to_fix(y1)));
        self.pe[p].x2 = tfmx(self.scale_abs(flt_to_fix(x2)));
        self.pe[p].y2 = tfmy(self.scale_abs(flt_to_fix(y2)));
        self.pe[p].x3 = tfmx(self.scale_abs(flt_to_fix(x3)));
        self.pe[p].y3 = tfmy(self.scale_abs(flt_to_fix(y3)));
        self.m_cur_x = x3;
        self.m_cur_y = y3;
    }

    fn curveto_quad(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let (cx, cy) = (self.m_cur_x, self.m_cur_y);
        self.curveto(
            java_round_f64((cx + 2.0 * x1) / 3.0) as f64,
            java_round_f64((cy + 2.0 * y1) / 3.0) as f64,
            java_round_f64((2.0 * x1 + x2) / 3.0) as f64,
            java_round_f64((2.0 * y1 + y2) / 3.0) as f64,
            java_round_f64(x2) as f64,
            java_round_f64(y2) as f64,
        );
    }

    fn endchar(&mut self) {
        self.close_sub_path();
        if self.do_glyph().is_err() {
            self.emit_raw_path();
        }
    }
}

fn flat_report_proc(n: i32, param: &mut Param, cd: Cd) {
    match (n, param) {
        (FRP_NZWIND, Param::NzWind(p)) => nz_wind_proc(p, cd),
        (FRP_YEXTREME, Param::FindEx(p)) => find_y_extreme_proc(p, cd),
        (FRP_CHKDT, _) => unreachable!("AutoColor.chkDT needs mDoSmoothing"),
        (FRP_CHKBBDT, Param::Check(p)) => chk_bb_dt(p, cd),
        (FRP_FPBBOXPT, Param::BBox(p)) => fp_b_box_pt(p, cd),
        _ => panic!("java.lang.ClassCastException in AutoColor.flatReportProc"),
    }
}

fn wind_test(cd: Cd, cd2: Cd, cd3: Cd) -> i32 {
    if cd.x > cd2.x && cd2.x > cd3.x {
        return WD_CCW;
    }
    if cd.x < cd2.x && cd2.x < cd3.x {
        return WD_CW;
    }
    if cd.y == cd2.y {
        return if cd.x >= cd2.x { WD_CCW } else { WD_CW };
    }
    if cd2.y == cd3.y {
        return if cd2.x >= cd3.x { WD_CCW } else { WD_CW };
    }
    let d3 = fix_to_flt(cd.x);
    let d4 = fix_to_flt(cd.y);
    let d5 = fix_to_flt(cd2.x);
    let d6 = fix_to_flt(cd2.y);
    let d7 = fix_to_flt(cd3.x);
    let d = fix_to_flt(cd3.y);
    let d2 = ((d5 - d3) * d + (d3 * d6 - d5 * d4)) / (d6 - d4);
    if d7 <= d2 { WD_CCW } else { WD_CW }
}

fn find_y_extreme_proc(fe: &mut FindExParam, cd: Cd) {
    if fe.has_last_pt && fe.last_pt.x == cd.x && fe.last_pt.y == cd.y {
        return;
    }
    if fe.last_was_extreme && cd.y < fe.y_max {
        fe.plaeau_ended = true;
        if fe.has_rise_pt {
            if fe.rise_pt.x != fe.last_pt.x {
                fe.wind_dir = if fe.rise_pt.x > fe.last_pt.x { WD_CCW } else { WD_CW };
            } else if fe.has_prerise_pt {
                fe.wind_dir = wind_test(fe.prerise_pt, fe.rise_pt, cd);
                fe.first_was_extreme = false;
            }
        } else if !fe.first_was_extreme {
            fe.first_was_extreme = true;
            fe.rise_pt = fe.first_pt;
            fe.before_first_sink_pt = fe.last_pt;
            fe.first_sink_pt = cd;
        }
    }
    if fe.has_last_pt {
        if cd.y > fe.y_max || fe.plaeau_ended && cd.y == fe.y_max {
            fe.plaeau_ended = false;
            fe.rise_pt = cd;
            fe.has_rise_pt = true;
            fe.prerise_pt = fe.last_pt;
            fe.has_prerise_pt = true;
            fe.first_was_extreme = false;
        }
    } else {
        fe.first_sink_pt = cd;
        fe.first_pt = cd;
    }
    if fe.first_was_extreme {
        if cd.y < fe.y_max {
            fe.prerise_pt = cd;
            fe.has_prerise_pt = true;
        }
        if fe.has_prerise_pt && cd.y == fe.y_max && !fe.has_rise_pt_for_first {
            fe.rise_pt = cd;
            fe.has_rise_pt_for_first = true;
        }
    }
    fe.last_pt = cd;
    fe.has_last_pt = true;
    fe.last_was_extreme = cd.y >= fe.y_max;
    if fe.last_was_extreme {
        fe.y_max = cd.y;
    }
}

fn y_extreme_short_cut_proc(cd: Cd, cd2: Cd, cd3: Cd, cd4: Cd, param: &mut Param, n: i32) -> bool {
    let n2 = match param {
        Param::FindEx(p) => p.y_max,
        _ => panic!("java.lang.ClassCastException in AutoColor.yExtremeShortCutProc"),
    };
    if cd.y < n2 && cd2.y < n2 && cd3.y < n2 && cd4.y < n2 {
        flat_report_proc(n, param, cd4);
        return true;
    }
    false
}

fn nz_wind_proc(nz: &mut NZWindParam, cd: Cd) {
    if nz.has_last_pt {
        let n2 = test_high_low(nz.test_pt.y, nz.last_pt.y);
        let n = test_high_low(nz.test_pt.y, cd.y);
        if n2.wrapping_mul(n) <= 0
            && (nz.test_to_right && (nz.last_pt.x > nz.test_pt.x || cd.x > nz.test_pt.x) || !nz.test_to_right && (nz.last_pt.x < nz.test_pt.x || cd.x < nz.test_pt.x))
            && nz.last_pt.y != cd.y
        {
            if cd.y == nz.test_pt.y && !(nz.test_to_right ^ (cd.x > nz.test_pt.x)) {
                nz.on_line = true;
                nz.from_above = nz.last_pt.y > nz.test_pt.y;
                nz.last_pt = cd;
                nz.has_last_pt = true;
                return;
            }
            if nz.on_line {
                if nz.from_above && n < 0 || !nz.from_above && n > 0 {
                    nz.wind_count = nz.wind_count.wrapping_add(if nz.test_to_right ^ (n < 0) { 1 } else { -1 });
                }
                nz.on_line = false;
                nz.last_pt = cd;
                nz.has_last_pt = true;
                return;
            }
            let d = fix_to_flt(nz.last_pt.x);
            let d2 = fix_to_flt(nz.last_pt.y);
            let d3 = fix_to_flt(cd.x);
            let d4 = fix_to_flt(cd.y);
            let d5 = (nz.testy * (d3 - d) + d * d4 - d3 * d2) / (d4 - d2);
            if !(nz.test_to_right ^ (d5 > nz.testx)) {
                nz.wind_count = nz.wind_count.wrapping_add(if nz.test_to_right ^ (d2 > d4) { 1 } else { -1 });
            }
        }
    } else {
        nz.first_pt = cd;
    }
    nz.last_pt = cd;
    nz.has_last_pt = true;
}

fn nz_wind_short_cut_proc(cd: Cd, cd2: Cd, cd3: Cd, cd4: Cd, param: &mut Param, n: i32) -> bool {
    let n2 = match param {
        Param::NzWind(p) => p.test_pt.y,
        _ => panic!("java.lang.ClassCastException in AutoColor.nzWindShortCutProc"),
    };
    if cd.y > n2 && cd2.y > n2 && cd3.y > n2 && cd4.y > n2 || cd.y < n2 && cd2.y < n2 && cd3.y < n2 && cd4.y < n2 {
        flat_report_proc(n, param, cd4);
        return true;
    }
    false
}

fn chk_bb_dt(c: &mut CheckData, cd: Cd) {
    let n = cd.x;
    let n2 = cd.y;
    if c.bbquit {
        return;
    }
    if c.vert {
        c.lst = n2;
        if !c.started && n.wrapping_sub(c.loc).wrapping_abs() <= fix_int(10) {
            c.started = true;
            c.frst = n2;
        } else if c.started && n.wrapping_sub(c.loc).wrapping_abs() > fix_int(10) {
            c.bbquit = true;
        }
    } else {
        c.lst = n;
        if !c.started && n2.wrapping_sub(c.loc).wrapping_abs() <= fix_int(10) {
            c.started = true;
            c.frst = n;
        } else if c.started && n2.wrapping_sub(c.loc).wrapping_abs() > fix_int(10) {
            c.bbquit = true;
        }
    }
}

fn fp_b_box_pt(b: &mut ACBBox, cd: Cd) {
    if cd.x < b.xmin {
        b.xmin = cd.x;
        b.pxmn = b.pe;
    }
    if cd.x > b.xmax {
        b.xmax = cd.x;
        b.pxmx = b.pe;
    }
    if cd.y < b.ymin {
        b.ymin = cd.y;
        b.pymn = b.pe;
    }
    if cd.y > b.ymax {
        b.ymax = cd.y;
        b.pymx = b.pe;
    }
}

fn fp_b_box_curve_short_cut(cd: Cd, cd2: Cd, cd3: Cd, cd4: Cd, b: &mut ACBBox) -> bool {
    fp_b_box_pt(b, cd);
    fp_b_box_pt(b, cd4);
    in_range(cd2.x, b.xmin, b.xmax) && in_range(cd2.y, b.ymin, b.ymax) && in_range(cd3.x, b.xmin, b.xmax) && in_range(cd3.y, b.ymin, b.ymax)
}

fn f_mini_fltn(cd: Cd, cd2: Cd, cd3: Cd, cd4: Cd, fr: &mut FltnRec, param: &mut Param, bl: bool) {
    let mut a = [0i16; 60];
    let (mut s, mut s2, mut s3, mut s4): (i32, i32, i32, i32) = (0, 0, 0, 0);
    let mut n2: usize = 0;
    let mut n3: i32 = 1;
    a[n2] = if bl { 1 } else { 0 };
    n2 += 1;
    a[n2] = 0;
    n2 += 1;
    let mut n4 = fr.llx;
    let mut n5 = fr.lly;
    for c in [cd, cd2, cd3, cd4] {
        a[n2] = c.x.wrapping_sub(n4) as i16;
        n2 += 1;
        a[n2] = c.y.wrapping_sub(n5) as i16;
        n2 += 1;
    }
    if a[n2 - 10] == 0 {
        n4 = fr.ll.x;
        s = if n4 <= 0 { 0 } else { n4 as i16 as i32 };
        n4 = fr.ll.y;
        s2 = if n4 <= 0 { 0 } else { n4 as i16 as i32 };
        n5 = fix_int(128);
        n4 = fr.ur.x;
        s3 = if n4 >= n5 { 32767 } else { n4 as i16 as i32 };
        n4 = fr.ur.y;
        s4 = if n4 >= n5 { 32767 } else { n4 as i16 as i32 };
    }
    let mut n = fr.feps as i16 as i32;
    if n < 8 {
        n = 8;
    }
    let g = |a: &[i16; 60], i: usize| a[i] as i32;
    'outer: loop {
        'b38: {
            'b40: {
                'b39: {
                    if n3 == 6 {
                        break 'b38;
                    }
                    if a[n2 - 10] != 0 {
                        break 'b39;
                    }
                    let mut n9 = g(&a, n2 - 6);
                    let mut n8 = g(&a, n2 - 8);
                    n4 = n8;
                    if n9 < n4 {
                        n4 = n9;
                    } else if n9 > n8 {
                        n8 = n9;
                    }
                    n9 = g(&a, n2 - 4);
                    if n9 < n4 {
                        n4 = n9;
                    } else if n9 > n8 {
                        n8 = n9;
                    }
                    n9 = g(&a, n2 - 2);
                    if n9 < n4 {
                        n4 = n9;
                    } else if n9 > n8 {
                        n8 = n9;
                    }
                    if n8 < s || n4 > s3 {
                        break 'b38;
                    }
                    n9 = g(&a, n2 - 5);
                    let mut n7 = g(&a, n2 - 7);
                    n5 = n7;
                    if n9 < n5 {
                        n5 = n9;
                    } else if n9 > n7 {
                        n7 = n9;
                    }
                    n9 = g(&a, n2 - 3);
                    if n9 < n5 {
                        n5 = n9;
                    } else if n9 > n7 {
                        n7 = n9;
                    }
                    n9 = g(&a, n2 - 1);
                    if n9 < n5 {
                        n5 = n9;
                    } else if n9 > n7 {
                        n7 = n9;
                    }
                    if n7 < s2 || n5 > s4 {
                        break 'b38;
                    }
                    if n8 <= s3 && n7 <= s4 && n4 >= s && n5 >= s2 {
                        a[n2 - 10] = 1;
                    }
                }
                if a[n2 - 9] == 0 {
                    n4 = n;
                    n5 = g(&a, n2 - 8);
                    let mut n8 = g(&a, n2 - 2);
                    let (mut n7, mut n9) = if n5 < n8 { ((n5 - n4) as i16 as i32, (n8 + n4) as i16 as i32) } else { ((n8 - n4) as i16 as i32, (n5 + n4) as i16 as i32) };
                    if n9 < 0 {
                        n9 = 32767;
                    }
                    let n6 = g(&a, n2 - 6);
                    if n6 > n7 && n6 < n9 && {
                        let n6 = g(&a, n2 - 4);
                        n6 > n7 && n6 < n9
                    } {
                        n5 = g(&a, n2 - 7);
                        n8 = g(&a, n2 - 1);
                        if n5 < n8 {
                            n7 = (n5 - n4) as i16 as i32;
                            n9 = (n8 + n4) as i16 as i32;
                        } else {
                            n7 = (n8 - n4) as i16 as i32;
                            n9 = (n5 + n4) as i16 as i32;
                        }
                        if n9 < 0 {
                            n9 = 32767;
                        }
                        let n6 = g(&a, n2 - 5);
                        if n6 > n7 && n6 < n9 && {
                            let n6 = g(&a, n2 - 3);
                            n6 > n7 && n6 < n9
                        } {
                            a[n2 - 9] = 1;
                        }
                    }
                }
                if a[n2 - 9] == 0 {
                    break 'b40;
                }
                let n8 = g(&a, n2 - 8);
                let n7 = g(&a, n2 - 7);
                n4 = (g(&a, n2 - 1) - n7) as i16 as i32;
                n5 = (n8 - g(&a, n2 - 2)) as i16 as i32;
                if n4 == 0 && n5 == 0 {
                    break 'b38;
                }
                let mut n9 = (if sh_abs(n4 as i16) > sh_abs(n5 as i16) { n4 } else { n5 }).wrapping_mul(n);
                if n9 < 0 {
                    n9 = n9.wrapping_neg();
                }
                let mut n6 = n4.wrapping_mul(g(&a, n2 - 6) - n8);
                n6 = n6.wrapping_add(n5.wrapping_mul(g(&a, n2 - 5) - n7));
                if n6.wrapping_abs() >= n9 {
                    break 'b40;
                }
                n6 = n4.wrapping_mul(g(&a, n2 - 4) - n8);
                n6 = n6.wrapping_add(n5.wrapping_mul(g(&a, n2 - 3) - n7));
                if n6.wrapping_abs() < n9 {
                    break 'b38;
                }
            }
            let x4 = g(&a, n2 - 8);
            a[n2 + 2] = x4 as i16;
            let x5 = g(&a, n2 - 6);
            let mut x8 = g(&a, n2 - 4);
            let x7 = mdpt(x4, x5) as i32;
            a[n2 + 4] = x7 as i16;
            let x6 = mdpt(x5, x8) as i32;
            let x9 = mdpt(x7, x6);
            a[n2 + 6] = x9;
            let s7 = mdpt(x8, g(&a, n2 - 2));
            x8 = s7 as i32;
            a[n2 - 4] = s7;
            let s8 = mdpt(x6, x8);
            a[n2 - 6] = s8;
            let s9 = mdpt(x9 as i32, s8 as i32);
            a[n2 + 8] = s9;
            a[n2 - 8] = s9;
            let y4 = g(&a, n2 - 7);
            a[n2 + 3] = y4 as i16;
            let y5 = g(&a, n2 - 5);
            let mut y8 = g(&a, n2 - 3);
            let y7 = mdpt(y4, y5) as i32;
            a[n2 + 5] = y7 as i16;
            let y6 = mdpt(y5, y8) as i32;
            let y9 = mdpt(y7, y6);
            a[n2 + 7] = y9;
            let s12 = mdpt(y8, g(&a, n2 - 1));
            y8 = s12 as i32;
            a[n2 - 3] = s12;
            let s13 = mdpt(y6, y8);
            a[n2 - 5] = s13;
            let s14 = mdpt(y9 as i32, s13 as i32);
            a[n2 + 9] = s14;
            a[n2 - 7] = s14;
            a[n2 + 1] = a[n2 - 9];
            a[n2] = a[n2 - 10];
            n2 += 10;
            n3 = (n3 + 1) as i16 as i32;
            continue 'outer;
        }
        n3 = (n3 - 1) as i16 as i32;
        let cd5 = if n3 == 0 { cd4 } else { Cd { x: g(&a, n2 - 2).wrapping_add(fr.llx), y: g(&a, n2 - 1).wrapping_add(fr.lly) } };
        flat_report_proc(fr.report, param, cd5);
        if n3 == 0 {
            return;
        }
        n2 -= 10;
    }
}

fn f_fltn_curve(cd: Cd, cd2: Cd, cd3: Cd, cd4: Cd, fr: &mut FltnRec, param: &mut Param, mut bl: bool) {
    let (mut cd, mut cd2, mut cd3, mut cd4) = (cd, cd2, cd3, cd4);
    if cd.x == cd2.x && cd.y == cd2.y && cd3.x == cd4.x && cd3.y == cd4.y || fr.limit <= 0 {
        flat_report_proc(fr.report, param, cd4);
        return;
    }
    let mut n3 = cd.x;
    let mut n2 = cd.x;
    for v in [cd2.x, cd3.x, cd4.x] {
        if v < n3 {
            n3 = v;
        } else if v > n2 {
            n2 = v;
        }
    }
    let mut n5 = cd.y;
    let mut n = cd.y;
    for v in [cd2.y, cd3.y, cd4.y] {
        if v < n5 {
            n5 = v;
        } else if v > n {
            n = v;
        }
    }
    if !bl {
        if n2 < fr.ll.x || n3 > fr.ur.x || n < fr.ll.y || n5 > fr.ur.y {
            flat_report_proc(fr.report, param, cd4);
            return;
        }
        if n2 <= fr.ur.x && n <= fr.ur.y && n3 >= fr.ll.x && n5 >= fr.ll.y {
            bl = true;
        }
    }
    let n4 = fix_int(127);
    if n2.wrapping_sub(n3) >= n4 || n.wrapping_sub(n5) >= n4 {
        let cd8 = cd4;
        let cd7 = Cd { x: cd3.x.wrapping_add(cd4.x) >> 1, y: cd3.y.wrapping_add(cd4.y) >> 1 };
        cd4 = Cd { x: cd2.x.wrapping_add(cd3.x) >> 1, y: cd2.y.wrapping_add(cd3.y) >> 1 };
        cd2 = Cd { x: cd.x.wrapping_add(cd2.x) >> 1, y: cd.y.wrapping_add(cd2.y) >> 1 };
        cd3 = Cd { x: cd2.x.wrapping_add(cd4.x) >> 1, y: cd2.y.wrapping_add(cd4.y) >> 1 };
        let cd6 = Cd { x: cd4.x.wrapping_add(cd7.x) >> 1, y: cd4.y.wrapping_add(cd7.y) >> 1 };
        let cd5 = Cd { x: cd3.x.wrapping_add(cd6.x) >> 1, y: cd3.y.wrapping_add(cd6.y) >> 1 };
        cd4 = cd5;
        fr.limit = fr.limit.wrapping_sub(1);
        f_fltn_curve(cd, cd2, cd3, cd4, fr, param, bl);
        f_fltn_curve(cd5, cd6, cd7, cd8, fr, param, bl);
        fr.limit = fr.limit.wrapping_add(1);
        return;
    }
    fr.llx = n3;
    fr.lly = n5;
    if !bl {
        fr.ll.x = fr.ll.x.wrapping_sub(n3);
        fr.ur.x = fr.ur.x.wrapping_sub(n3);
        fr.ll.y = fr.ll.y.wrapping_sub(n5);
        fr.ur.y = fr.ur.y.wrapping_sub(n5);
    }
    f_mini_fltn(cd, cd2, cd3, cd4, fr, param, bl);
    if !bl {
        fr.ll.x = fr.ll.x.wrapping_add(n3);
        fr.ur.x = fr.ur.x.wrapping_add(n3);
        fr.ll.y = fr.ll.y.wrapping_add(n5);
        fr.ur.y = fr.ur.y.wrapping_add(n5);
    }
    let _ = &mut cd;
}

fn fltn_curve(cd: Cd, cd2: Cd, cd3: Cd, cd4: Cd, fr: &mut FltnRec, param: &mut Param) {
    fr.limit = 6;
    fr.feps = 128;
    f_fltn_curve(cd, cd2, cd3, cd4, fr, param, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::non_overlapping_outline_consumer::test_support::*;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::non_overlapping_outline_consumer::NonOverlappingOutlineConsumer;

    fn zones(z: &[i32]) -> String {
        let mut s = z.len().to_string();
        for v in z {
            s.push(' ');
            s.push_str(&v.to_string());
        }
        s
    }

    #[test]
    fn auto_color_matches_java() {
        let mut bad = 0;
        let mut shown = 0;
        for name in FONTS {
            let Some(faces) = load(name) else { return };
            let (mut ok, mut n, mut zok, mut zn) = (0, 0, 0, 0);
            for face in &faces {
                let rec = Rc::new(RefCell::new(Rec { prefix: "A", lines: Vec::new() }));
                let ac = Rc::new(RefCell::new(AutoColor::new(Some(rec.clone()), face.upem_y, 6, true, false, &face.top, &face.bot)));
                let mut chain = NonOverlappingOutlineConsumer::new(ac.clone(), face.upem_y);
                let zac = Rc::new(RefCell::new(AutoColor::new(None, face.upem_x, 14, false, true, &[], &[])));
                let mut zchain = NonOverlappingOutlineConsumer::new(zac.clone(), face.upem_x);
                for g in face.glyphs.iter().filter(|g| !g.err) {
                    rec.borrow_mut().lines.clear();
                    ac.borrow_mut().new_glyph(g.idx, g.adv, 0);
                    replay(&g.input, &mut chain);
                    n += 1;
                    if rec.borrow().lines == g.a {
                        ok += 1;
                    } else if shown < 4 {
                        shown += 1;
                        eprintln!("{} gid {}:
  java {:?}
  rust {:?}", name, g.gid, g.a, rec.borrow().lines);
                    }
                    zac.borrow_mut().new_glyph(g.gid, g.adv, 0);
                    replay(&g.input, &mut zchain);
                    zn += 1;
                    let z = zones(&zac.borrow().report_zones());
                    if z == g.z {
                        zok += 1;
                    } else if shown < 4 {
                        shown += 1;
                        eprintln!("{} gid {} zones:
  java {}
  rust {}", name, g.gid, g.z, z);
                    }
                }
                let zc = Rc::new(RefCell::new(AutoColor::new(None, face.upem_x, 14, false, true, &[], &[])));
                let mut zcc = NonOverlappingOutlineConsumer::new(zc.clone(), face.upem_x);
                for g in &face.zchars {
                    zc.borrow_mut().new_glyph(g.gid, g.adv, g.code);
                    replay(&g.input, &mut zcc);
                    zn += 1;
                    let z = zones(&zc.borrow().report_zones());
                    if z == g.z {
                        zok += 1;
                    } else if shown < 4 {
                        shown += 1;
                        eprintln!("{} char {} zones:
  java {}
  rust {}", name, g.code, g.z, z);
                    }
                }
            }
            eprintln!("{}: hinted {}/{} glyphs, zones {}/{}", name, ok, n, zok, zn);
            bad += n - ok + zn - zok;
        }
        assert_eq!(bad, 0);
    }

    #[test]
    fn auto_color_synthetic_matches_java() {
        let mut counts = [0usize; 4];
        let mut oks = [0usize; 4];
        let mut shown = 0;
        let faces: Vec<Face> = ["synth1", "synth2", "synth3"].iter().filter_map(|n| load(n)).flatten().collect();
        for face in &faces {
            let rec = Rc::new(RefCell::new(Rec { prefix: "A", lines: Vec::new() }));
            let ac = Rc::new(RefCell::new(AutoColor::new(Some(rec.clone()), face.upem_y, 6, true, false, &face.top, &face.bot)));
            let mut chain = NonOverlappingOutlineConsumer::new(ac.clone(), face.upem_y);
            let drec = Rc::new(RefCell::new(Rec { prefix: "D", lines: Vec::new() }));
            let mut dac = AutoColor::new(Some(drec.clone()), face.upem_y, 6, true, false, &face.top, &face.bot);
            let zac = Rc::new(RefCell::new(AutoColor::new(None, face.upem_x, 14, false, true, &[], &[])));
            let mut zchain = NonOverlappingOutlineConsumer::new(zac.clone(), face.upem_x);
            let mut eac = AutoColor::new(None, face.upem_x, 14, false, true, &[], &[]);
            for g in &face.glyphs {
                assert!(g.xs.is_empty());
                rec.borrow_mut().lines.clear();
                ac.borrow_mut().new_glyph(g.idx, g.adv, g.code);
                replay(&g.input, &mut chain);
                drec.borrow_mut().lines.clear();
                dac.new_glyph(g.idx, g.adv, g.code);
                replay(&g.input, &mut dac);
                zac.borrow_mut().new_glyph(g.gid, g.adv, g.code);
                replay(&g.input, &mut zchain);
                eac.new_glyph(g.gid, g.adv, g.code);
                replay(&g.input, &mut eac);
                let res = [rec.borrow().lines == g.a, drec.borrow().lines == g.d, zones(&zac.borrow().report_zones()) == g.z, zones(&eac.report_zones()) == g.e];
                for i in 0..4 {
                    counts[i] += 1;
                    if res[i] {
                        oks[i] += 1;
                    } else if shown < 3 {
                        shown += 1;
                        eprintln!("gid {} stream {}:
  in {:?}
  java A {:?}
  rust A {:?}
  java D {:?}
  rust D {:?}", g.gid, i, g.input, g.a, rec.borrow().lines, g.d, drec.borrow().lines);
                    }
                }
            }
        }
        eprintln!("synthetic: chained {}/{}, direct {}/{}, zones chained {}/{}, zones direct {}/{}", oks[0], counts[0], oks[1], counts[1], oks[2], counts[2], oks[3], counts[3]);
        assert_eq!(oks, counts);
    }
}
