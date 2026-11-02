//! Port of FreeType 2.14.2 `src/truetype/ttobjs.c` (as built into JDK 17.0.19).

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::*;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::*;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtResult;
use crate::asc::flex2::fonts::java2d::freetype::sfobjs::{sfnt_init_face, sfnt_load_face};
use crate::asc::flex2::fonts::java2d::freetype::ttinterp::*;
use crate::asc::flex2::fonts::java2d::freetype::ttload::tt_face_goto_table;
use crate::asc::flex2::fonts::java2d::freetype::ttpload::*;
use crate::asc::flex2::fonts::java2d::freetype::ttpost::tt_face_get_ps_name;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct TtGraphicsState {
    pub rp0: u16,
    pub rp1: u16,
    pub rp2: u16,
    pub gep0: u16,
    pub gep1: u16,
    pub gep2: u16,
    pub dual_vector: FtUnitVector,
    pub proj_vector: FtUnitVector,
    pub free_vector: FtUnitVector,
    pub loop_: i32,
    pub round_state: i32,
    pub compensation: [FtF26Dot6; 4],
    pub minimum_distance: FtF26Dot6,
    pub control_value_cutin: FtF26Dot6,
    pub single_width_cutin: FtF26Dot6,
    pub single_width_value: FtF26Dot6,
    pub delta_base: u16,
    pub delta_shift: u16,
    pub auto_flip: bool,
    pub instruct_control: u8,
    pub scan_control: bool,
    pub scan_type: i32,
}

#[derive(Clone, Default, Debug)]
pub struct TtGlyphZone {
    pub n_points: u16,
    pub n_contours: u16,
    pub org: Vec<FtVector>,
    pub cur: Vec<FtVector>,
    pub orus: Vec<FtVector>,
    pub tags: Vec<u8>,
    pub contours: Vec<u16>,
    pub first_point: u16,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TtSizeMetrics {
    pub x_ratio: i32,
    pub y_ratio: i32,
    pub ratio: i32,
    pub scale: FtFixed,
    pub ppem: u16,
    pub rotated: bool,
    pub stretched: bool,
}

#[derive(Clone, Default)]
pub struct TtSize {
    pub root_metrics: FtSizeMetrics,
    /// `TT_Size.metrics` points at `hinted_metrics` when set, else at the root metrics.
    pub metrics_hinted: bool,
    pub hinted_metrics: FtSizeMetrics,
    pub ttmetrics: TtSizeMetrics,
    pub widthp: Option<usize>,
    pub strike_index: u32,
    pub point_size: i32,
    pub gs: TtGraphicsState,
    pub twilight_n_points: u16,
    pub context: Option<Box<TtExecContext>>,
    pub bytecode_ready: FtError,
    pub cvt_ready: FtError,
    pub autohint_x_scale: FtFixed,
}

impl TtSize {
    /// `FT_New_Size` + `tt_size_init`.
    pub fn new() -> TtSize {
        let mut s = TtSize::default();
        s.bytecode_ready = -1;
        s.cvt_ready = -1;
        s.strike_index = 0xFFFF_FFFF;
        s
    }

    pub fn metrics(&self) -> &FtSizeMetrics {
        if self.metrics_hinted {
            &self.hinted_metrics
        } else {
            &self.root_metrics
        }
    }
}

fn tt_skip_pdffont_random_tag(name: &[u8]) -> &[u8] {
    if name.len() >= 8 && name[..6].iter().all(|c| c.is_ascii_uppercase()) && name[6] == b'+' && name[7] != 0 {
        return &name[7..];
    }
    name
}

static TRICK_NAMES: [&str; 20] = [
    "cpop",
    "DFGirl-W6-WIN-BF",
    "DFGothic-EB",
    "DFGyoSho-Lt",
    "DFHei",
    "DFHSGothic-W5",
    "DFHSMincho-W3",
    "DFHSMincho-W7",
    "DFKaiSho-SB",
    "DFKaiShu",
    "DFKai-SB",
    "DFMing",
    "DLC",
    "HuaTianKaiTi?",
    "HuaTianSongTi?",
    "Ming(for ISO10646)",
    "MingLiU",
    "MingMedium",
    "PMingLiU",
    "MingLi43",
];

fn tt_check_trickyness_family(name: &str) -> bool {
    let n = tt_skip_pdffont_random_tag(name.as_bytes());
    TRICK_NAMES.iter().any(|t| n.windows(t.len()).any(|w| w == t.as_bytes()))
}

fn tt_synth_sfnt_checksum(data: &[u8]) -> u32 {
    let mut checksum: u32 = 0;
    let mut p = 0usize;
    let mut length = data.len();
    while length > 3 {
        checksum = checksum.wrapping_add(u32::from_be_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]));
        p += 4;
        length -= 4;
    }
    let mut shift = 24;
    while length > 0 {
        checksum = checksum.wrapping_add((data[p] as u32) << shift);
        p += 1;
        length -= 1;
        shift -= 8;
    }
    checksum
}

fn tt_get_sfnt_checksum(face: &mut TtFace, i: usize) -> u32 {
    let t = face.dir_tables[i];
    if tt_face_goto_table(face, t.tag).is_err() {
        return 0;
    }
    match face.stream.extract(t.length) {
        Ok(v) => tt_synth_sfnt_checksum(&v),
        Err(_) => 0,
    }
}

static SFNT_ID: [[(u32, u32); 3]; 31] = [
    [(0x05BCF058, 0x000002E4), (0x28233BF1, 0x000087C4), (0xA344A1EA, 0x000001E1)],
    [(0x05BCF058, 0x000002E4), (0x28233BF1, 0x000087C4), (0xA344A1EB, 0x000001E1)],
    [(0x12C3EBB2, 0x00000350), (0xB680EE64, 0x000087A7), (0xCE939563, 0x00000758)],
    [(0x11E5EAD4, 0x00000350), (0xCE5956E9, 0x0000BC85), (0x8272F416, 0x00000045)],
    [(0x1257EB46, 0x00000350), (0xF699D160, 0x0000715F), (0xD222F568, 0x000003BC)],
    [(0x1262EB4E, 0x00000350), (0xE86A5D64, 0x00007940), (0x7850F729, 0x000005FF)],
    [(0x122DEB0A, 0x00000350), (0x3D16328A, 0x0000859B), (0xA93FC33B, 0x000002CB)],
    [(0x125FEB26, 0x00000350), (0xA5ACC982, 0x00007EE1), (0x90999196, 0x0000041F)],
    [(0x11E5EAD4, 0x00000350), (0x5A30CA3B, 0x00009063), (0x13A42602, 0x0000007E)],
    [(0x11E5EAD4, 0x00000350), (0xA6E78C01, 0x00008998), (0x13A42602, 0x0000007E)],
    [(0x11E5EAD4, 0x00000360), (0x9DB282B2, 0x0000C06E), (0x53E6D7CA, 0x00000082)],
    [(0x1243EB18, 0x00000350), (0xBA0A8C30, 0x000074AD), (0xF3D83409, 0x0000037B)],
    [(0x07DCF546, 0x00000308), (0x40FE7C90, 0x00008E2A), (0x608174B5, 0x0000007A)],
    [(0xEB891238, 0x00000308), (0xD2E4DCD4, 0x0000676F), (0x8EA5F293, 0x000003B8)],
    [(0xFFFBFFFC, 0x00000008), (0x9C9E48B8, 0x0000BEA2), (0x70020112, 0x00000008)],
    [(0xFFFBFFFC, 0x00000008), (0x0A5A0483, 0x00017C39), (0x70020112, 0x00000008)],
    [(0x00000000, 0x00000000), (0x40C92555, 0x000000E5), (0xA39B58E3, 0x0000117C)],
    [(0x00000000, 0x00000000), (0x33C41652, 0x000000E5), (0x26D6C52A, 0x00000F6A)],
    [(0x00000000, 0x00000000), (0x6DB1651D, 0x0000019D), (0x6C6E4B03, 0x00002492)],
    [(0x00000000, 0x00000000), (0x40C92555, 0x000000E5), (0xDE51FAD0, 0x0000117C)],
    [(0x00000000, 0x00000000), (0x85E47664, 0x000000E5), (0xA6C62831, 0x00001CAA)],
    [(0x00000000, 0x00000000), (0x2D891CFD, 0x0000019D), (0xA0604633, 0x00001DE8)],
    [(0x00000000, 0x00000000), (0x40AA774C, 0x000001CB), (0x9B5CAA96, 0x00001F9A)],
    [(0x00000000, 0x00000000), (0x0D3DE9CB, 0x00000141), (0xD4127766, 0x00002280)],
    [(0x00000000, 0x00000000), (0x4A692698, 0x000001F0), (0x340D4346, 0x00001FCA)],
    [(0x00000000, 0x00000000), (0xCD34C604, 0x00000166), (0x6CF31046, 0x000022B0)],
    [(0x00000000, 0x00000000), (0x5DA75315, 0x0000019D), (0x40745A5F, 0x000022E0)],
    [(0x00000000, 0x00000000), (0xF055FC48, 0x000001C2), (0x3900DED3, 0x00001E18)],
    [(0x00170003, 0x00000060), (0xDBB4306E, 0x000058AA), (0xD643482A, 0x00000035)],
    [(0x1269EB58, 0x00000350), (0x5CD5957A, 0x00006A4E), (0xF758323A, 0x00000380)],
    [(0x122FEB0B, 0x00000350), (0x7F10919A, 0x000070A9), (0x7CD7E7B7, 0x0000025C)],
];

fn tt_check_trickyness_sfnt_ids(face: &mut TtFace) -> bool {
    let mut num_matched_ids = [0i32; 31];
    let (mut has_cvt, mut has_fpgm, mut has_prep) = (false, false, false);
    for i in 0..face.dir_tables.len() {
        let mut checksum: u32 = 0;
        let k = match face.dir_tables[i].tag {
            TTAG_CVT => {
                has_cvt = true;
                0
            }
            TTAG_FPGM => {
                has_fpgm = true;
                1
            }
            TTAG_PREP => {
                has_prep = true;
                2
            }
            _ => continue,
        };
        for j in 0..31 {
            if face.dir_tables[i].length == SFNT_ID[j][k].1 {
                if checksum == 0 {
                    checksum = tt_get_sfnt_checksum(face, i);
                }
                if SFNT_ID[j][k].0 == checksum {
                    num_matched_ids[j] += 1;
                }
                if num_matched_ids[j] == 3 {
                    return true;
                }
            }
        }
    }
    for j in 0..31 {
        if !has_cvt && SFNT_ID[j][0].1 == 0 {
            num_matched_ids[j] += 1;
        }
        if !has_fpgm && SFNT_ID[j][1].1 == 0 {
            num_matched_ids[j] += 1;
        }
        if !has_prep && SFNT_ID[j][2].1 == 0 {
            num_matched_ids[j] += 1;
        }
        if num_matched_ids[j] == 3 {
            return true;
        }
    }
    false
}

fn tt_check_trickyness(face: &mut FtFace) -> bool {
    if let Some(n) = &face.family_name {
        if tt_check_trickyness_family(n) {
            return true;
        }
    }
    tt_check_trickyness_sfnt_ids(&mut face.tt)
}

fn tt_check_single_notdef(face: &mut FtFace) -> bool {
    let tt = &face.tt;
    let mut count = 0;
    let mut glyph_index = 0u32;
    for i in 0..tt.num_locations {
        let (_, asize) = tt_face_get_location(tt, i);
        if asize > 0 {
            count += 1;
            if count > 1 {
                break;
            }
            glyph_index = i;
        }
    }
    if count == 1 {
        if glyph_index == 0 {
            return true;
        }
        if let Some(name) = tt_face_get_ps_name(face, glyph_index) {
            // FT_Get_Glyph_Name into an 8-byte buffer truncates to 7 characters
            return name.len() >= 7 && &name[..7] == b".notdef";
        }
    }
    false
}

pub fn tt_face_init(face: &mut FtFace, face_index: i32) -> FtResult<()> {
    face.tt.stream.seek(0)?;
    sfnt_init_face(face, face_index)?;
    let ft = face.tt.format_tag;
    if ft != 0x0001_0000 && ft != 0x0002_0000 && ft != TTAG_TRUE && ft != TTAG_0XA5KBD && ft != TTAG_0XA5LST {
        return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
    }
    face.face_flags |= FT_FACE_FLAG_HINTER;
    if face_index < 0 {
        return Ok(());
    }
    sfnt_load_face(face)?;
    if tt_check_trickyness(face) {
        face.face_flags |= FT_FACE_FLAG_TRICKY;
    }
    tt_face_load_hdmx(face)?;
    if face.is_scalable() || face.has_sbix() {
        match tt_face_load_loca(face) {
            Ok(()) => {}
            Err(e) => return Err(e),
        }
        if let Err(e) = tt_face_load_cvt(&mut face.tt) {
            if e != FT_ERR_TABLE_MISSING {
                return Err(e);
            }
        }
        if let Err(e) = tt_face_load_fpgm(&mut face.tt) {
            if e != FT_ERR_TABLE_MISSING {
                return Err(e);
            }
        }
        if let Err(e) = tt_face_load_prep(&mut face.tt) {
            if e != FT_ERR_TABLE_MISSING {
                return Err(e);
            }
        }
        if face.num_fixed_sizes != 0 && !face.tt.glyph_locations.is_empty() && tt_check_single_notdef(face) {
            face.face_flags &= !FT_FACE_FLAG_SCALABLE;
        }
    }
    Ok(())
}

pub fn tt_size_run_fpgm(face: &FtFace, size: &mut TtSize) -> FtError {
    let exec = size.context.as_mut().unwrap();
    exec.load_context(size.point_size, size.ttmetrics, *if size.metrics_hinted { &size.hinted_metrics } else { &size.root_metrics }, size.twilight_n_points);
    exec.clear_code_range(TT_CODERANGE_CVT);
    exec.clear_code_range(TT_CODERANGE_GLYPH);
    let error;
    if !face.tt.font_program.is_empty() {
        let fp = Rc::new(face.tt.font_program.clone());
        let len = fp.len() as i32;
        exec.set_code_range(TT_CODERANGE_FONT, fp, len);
        exec.pts.n_points = 0;
        exec.pts.n_contours = 0;
        error = exec.run_context(&size.gs);
    } else {
        error = FT_ERR_OK;
    }
    size.bytecode_ready = error;
    if error == 0 {
        exec.save_context(&mut size.gs);
    }
    error
}

pub fn tt_size_run_prep(face: &FtFace, size: &mut TtSize) -> FtError {
    size.gs = TT_DEFAULT_GRAPHICS_STATE;
    let metrics = *size.metrics();
    let exec = size.context.as_mut().unwrap();
    for v in exec.twilight.org.iter_mut() {
        *v = FtVector::default();
    }
    for v in exec.twilight.cur.iter_mut() {
        *v = FtVector::default();
    }
    exec.load_context(size.point_size, size.ttmetrics, metrics, size.twilight_n_points);
    for v in exec.storage_base.iter_mut() {
        *v = 0;
    }
    for i in 0..exec.cvt_size as usize {
        exec.cvt_base[i] = ft_mul_fix(face.tt.cvt[i] / 64, size.ttmetrics.scale);
    }
    exec.clear_code_range(TT_CODERANGE_GLYPH);
    let error;
    if !face.tt.cvt_program.is_empty() {
        let cp = Rc::new(face.tt.cvt_program.clone());
        let len = cp.len() as i32;
        exec.set_code_range(TT_CODERANGE_CVT, cp, len);
        exec.pts.n_points = 0;
        exec.pts.n_contours = 0;
        error = exec.run_context(&size.gs);
    } else {
        error = FT_ERR_OK;
    }
    size.cvt_ready = error;
    if error == 0 {
        exec.save_context(&mut size.gs);
    }
    error
}

pub fn tt_size_init_bytecode(face: &FtFace, size: &mut TtSize, pedantic: bool) -> FtError {
    let maxp = &face.tt.max_profile;
    let mut exec = Box::new(TtExecContext::new());
    exec.num_glyphs = face.num_glyphs;
    exec.pedantic_hinting = pedantic;
    exec.max_fdefs = maxp.max_function_defs as u32;
    exec.max_idefs = maxp.max_instruction_defs as u32;
    exec.fdefs = vec![TtDefRecord::default(); exec.max_fdefs as usize];
    exec.idefs = vec![TtDefRecord::default(); exec.max_idefs as usize];
    exec.num_fdefs = 0;
    exec.num_idefs = 0;
    exec.max_func = 0;
    exec.max_ins = 0;
    exec.stack_size = maxp.max_stack_elements as i32 + 32;
    exec.store_size = maxp.max_storage;
    exec.cvt_size = face.tt.cvt.len() as u32;
    exec.stack = vec![0; exec.stack_size as usize];
    exec.storage_base = vec![0; exec.store_size as usize];
    exec.cvt_base = vec![0; exec.cvt_size as usize];
    let n_twilight = maxp.max_twilight_points + 4;
    exec.twilight = TtGlyphZone {
        n_points: n_twilight,
        n_contours: 0,
        org: vec![FtVector::default(); n_twilight as usize],
        cur: vec![FtVector::default(); n_twilight as usize],
        orus: vec![FtVector::default(); n_twilight as usize],
        tags: vec![0; n_twilight as usize],
        contours: Vec::new(),
        first_point: 0,
    };
    size.twilight_n_points = n_twilight;
    size.context = Some(exec);
    size.gs = TT_DEFAULT_GRAPHICS_STATE;
    size.cvt_ready = -1;
    size.ttmetrics.rotated = false;
    size.ttmetrics.stretched = false;
    tt_size_run_fpgm(face, size)
}

pub fn tt_size_reset_height(face: &FtFace, size: &mut TtSize) {
    let m = &mut size.hinted_metrics;
    if face.tt.header.flags & 8 != 0 {
        m.ascender = ft_pix_round(ft_mul_fix(face.ascender as i32, m.y_scale));
        m.descender = ft_pix_round(ft_mul_fix(face.descender as i32, m.y_scale));
        m.height = ft_pix_round(ft_mul_fix(face.height as i32, m.y_scale));
    }
}

pub fn tt_size_reset(face: &FtFace, size: &mut TtSize) -> FtError {
    size.ttmetrics.ppem = 0;
    if size.root_metrics.x_ppem == 0 || size.root_metrics.y_ppem == 0 {
        return FT_ERR_INVALID_PPEM;
    }
    size.hinted_metrics = size.root_metrics;
    tt_size_reset_height(face, size);
    let m = &mut size.hinted_metrics;
    if face.tt.header.flags & 8 != 0 {
        m.x_scale = ft_div_fix((m.x_ppem as i32) << 6, face.units_per_em as i32);
        m.y_scale = ft_div_fix((m.y_ppem as i32) << 6, face.units_per_em as i32);
        m.max_advance = ft_pix_round(ft_mul_fix(face.max_advance_width as i32, m.x_scale));
    }
    if m.x_ppem >= m.y_ppem {
        size.ttmetrics.scale = m.x_scale;
        size.ttmetrics.ppem = m.x_ppem;
        size.ttmetrics.x_ratio = 0x10000;
        size.ttmetrics.y_ratio = ft_div_fix(m.y_ppem as i32, m.x_ppem as i32);
    } else {
        size.ttmetrics.scale = m.y_scale;
        size.ttmetrics.ppem = m.y_ppem;
        size.ttmetrics.x_ratio = ft_div_fix(m.x_ppem as i32, m.y_ppem as i32);
        size.ttmetrics.y_ratio = 0x10000;
    }
    size.widthp = tt_face_get_device_metrics(&face.tt, m.x_ppem as u32);
    size.metrics_hinted = true;
    size.cvt_ready = -1;
    FT_ERR_OK
}
