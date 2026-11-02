//! Port of FreeType 2.14.2 `src/truetype/ttgload.c` (as built into JDK 17.0.19), without GX variations, incremental loading, SVG and bitmap loading.

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::*;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftgloadr::FtGlyphLoader;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::*;
use crate::asc::flex2::fonts::java2d::freetype::ftoutln::*;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::Frame;
use crate::asc::flex2::fonts::java2d::freetype::ttinterp::*;
use crate::asc::flex2::fonts::java2d::freetype::ttmtx::tt_face_get_metrics;
use crate::asc::flex2::fonts::java2d::freetype::ttobjs::*;
use crate::asc::flex2::fonts::java2d::freetype::ttpload::tt_face_get_location;

const ON_CURVE_POINT: u8 = 0x01;
const X_SHORT_VECTOR: u8 = 0x02;
const Y_SHORT_VECTOR: u8 = 0x04;
const REPEAT_FLAG: u8 = 0x08;
const X_POSITIVE: u8 = 0x10;
const SAME_X: u8 = 0x10;
const Y_POSITIVE: u8 = 0x20;
const SAME_Y: u8 = 0x20;
const OVERLAP_SIMPLE: u8 = 0x40;

const ARGS_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const ROUND_XY_TO_GRID: u16 = 0x0004;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_XY_SCALE: u16 = 0x0040;
const WE_HAVE_A_2X2: u16 = 0x0080;
const WE_HAVE_INSTR: u16 = 0x0100;
const USE_MY_METRICS: u16 = 0x0200;
const OVERLAP_COMPOUND: u16 = 0x0400;
const SCALED_COMPONENT_OFFSET: u16 = 0x0800;

#[inline]
fn is_hinted(flags: i32) -> bool {
    flags & FT_LOAD_NO_HINTING == 0
}

/// `TT_LoaderRec`; `gloader` is `glyph.loader`, and the execution context is borrowed from `size` while loading.
pub struct TtLoader<'a> {
    pub face: &'a mut FtFace,
    pub size: &'a mut TtSize,
    pub glyph: &'a mut FtGlyphSlot,
    pub exec: Option<Box<TtExecContext>>,
    pub load_flags: i32,
    pub glyph_index: u32,
    pub byte_len: u32,
    pub n_contours: i16,
    pub bbox: FtBBox,
    pub left_bearing: i32,
    pub advance: i32,
    pub linear: i32,
    pub linear_def: bool,
    pub pp1: FtVector,
    pub pp2: FtVector,
    pub ins_pos: u32,
    pub top_bearing: i32,
    pub vadvance: i32,
    pub pp3: FtVector,
    pub pp4: FtVector,
    frame: Option<Frame>,
    composites: Vec<u32>,
    pub widthp: Option<usize>,
}

/// `TT_GlyphZoneRec` as offsets into the glyph loader's arrays.
#[derive(Clone, Copy, Default)]
struct ZoneRef {
    n_points: u16,
    n_contours: u16,
    pt: usize,
    ct: usize,
    first_point: u16,
}

#[inline]
fn check_points(gl: &mut FtGlyphLoader, n_points: u32, n_contours: u32) -> FtError {
    let p_ok = n_points == 0 || gl.base_n_points as u32 + gl.cur_n_points as u32 + n_points <= gl.max_points;
    let c_ok = n_contours == 0 || gl.base_n_contours as u32 + gl.cur_n_contours as u32 + n_contours <= gl.max_contours;
    if p_ok && c_ok {
        FT_ERR_OK
    } else {
        gl.check_points(n_points, n_contours)
    }
}

pub fn tt_get_hmetrics(face: &mut FtFace, idx: u32) -> (i16, u16) {
    tt_face_get_metrics(&mut face.tt, false, idx)
}

pub fn tt_get_vmetrics(face: &mut FtFace, idx: u32, y_max: FtPos) -> (i16, u16) {
    let tt = &mut face.tt;
    if tt.vertical_info {
        tt_face_get_metrics(tt, true, idx)
    } else if tt.os2.version != 0xFFFF {
        let tsb = (tt.os2.s_typo_ascender as i32).wrapping_sub(y_max) as i16;
        let ah = ft_abs(tt.os2.s_typo_ascender as i32 - tt.os2.s_typo_descender as i32) as u16;
        (tsb, ah)
    } else {
        let tsb = (tt.horizontal.ascender as i32).wrapping_sub(y_max) as i16;
        let ah = ft_abs(tt.horizontal.ascender as i32 - tt.horizontal.descender as i32) as u16;
        (tsb, ah)
    }
}

impl<'a> TtLoader<'a> {
    fn tt_get_metrics(&mut self, glyph_index: u32) -> FtError {
        let pos = self.face.tt.stream.pos;
        let (left_bearing, advance_width) = tt_get_hmetrics(self.face, glyph_index);
        let (top_bearing, advance_height) = tt_get_vmetrics(self.face, glyph_index, self.bbox.y_max);
        if let Err(e) = self.face.tt.stream.seek(pos) {
            return e;
        }
        self.left_bearing = left_bearing as i32;
        self.advance = advance_width as i32;
        self.top_bearing = top_bearing as i32;
        self.vadvance = advance_height as i32;
        if !self.linear_def {
            self.linear_def = true;
            self.linear = advance_width as i32;
        }
        FT_ERR_OK
    }

    fn access_glyph_frame(&mut self, offset: u32, byte_count: u32) -> FtError {
        let stream = &mut self.face.tt.stream;
        if let Err(e) = stream.seek(offset) {
            return e;
        }
        match stream.enter_frame(byte_count) {
            Ok(f) => {
                self.frame = Some(f);
                FT_ERR_OK
            }
            Err(e) => e,
        }
    }

    fn forget_glyph_frame(&mut self) {
        self.frame = None;
    }

    fn load_glyph_header(&mut self) -> FtError {
        let f = self.frame.as_mut().unwrap();
        if f.cursor + 10 > f.limit {
            return FT_ERR_INVALID_OUTLINE;
        }
        self.n_contours = f.get_short();
        self.bbox.x_min = f.get_short() as i32;
        self.bbox.y_min = f.get_short() as i32;
        self.bbox.x_max = f.get_short() as i32;
        self.bbox.y_max = f.get_short() as i32;
        FT_ERR_OK
    }

    fn load_simple_glyph(&mut self) -> FtError {
        let mut f = self.frame.take().unwrap();
        let e = self.load_simple_glyph_in(&mut f);
        self.frame = Some(f);
        e
    }

    fn load_simple_glyph_in(&mut self, f: &mut Frame) -> FtError {
        let limit = f.limit;
        let n_contours = self.n_contours as i32;
        let hinted = is_hinted(self.load_flags);
        let gl = &mut self.glyph.loader;
        let error = check_points(gl, 0, n_contours as u32);
        if error != 0 {
            return error;
        }
        if n_contours >= 0xFFF || f.cursor + 2 * n_contours as usize + 2 > limit {
            return FT_ERR_INVALID_OUTLINE;
        }
        let ct = gl.base_n_contours as usize + gl.cur_n_contours as usize;
        let mut last: i32 = -1;
        for i in 0..n_contours as usize {
            let c = f.get_ushort();
            gl.contours[ct + i] = c;
            if c as i32 <= last {
                return FT_ERR_INVALID_OUTLINE;
            }
            last = c as i32;
        }
        let n_points = last + 1;
        let error = check_points(gl, (n_points + 4) as u32, 0);
        if error != 0 {
            return error;
        }
        let n_ins = f.get_ushort() as usize;
        if f.cursor + n_ins > limit {
            return FT_ERR_TOO_MANY_HINTS;
        }
        if hinted {
            let exec = self.exec.as_mut().unwrap();
            exec.glyph_ins = Rc::new(Vec::new());
            exec.glyph_size = 0;
            if n_ins != 0 {
                exec.glyph_ins = Rc::new(f.base[f.cursor..f.cursor + n_ins].to_vec());
                exec.glyph_size = n_ins as u32;
            }
        }
        f.cursor += n_ins;
        let pt = gl.base_n_points as usize + gl.cur_n_points as usize;
        let np = n_points as usize;
        let mut i = 0usize;
        while i < np {
            if f.cursor + 1 > limit {
                return FT_ERR_INVALID_OUTLINE;
            }
            let c = f.get_byte();
            gl.tags[pt + i] = c;
            i += 1;
            if c & REPEAT_FLAG != 0 {
                if f.cursor + 1 > limit {
                    return FT_ERR_INVALID_OUTLINE;
                }
                let count = f.get_byte() as usize;
                if i + count > np {
                    return FT_ERR_INVALID_OUTLINE;
                }
                for _ in 0..count {
                    gl.tags[pt + i] = c;
                    i += 1;
                }
            }
        }
        if np != 0 && gl.tags[pt] & OVERLAP_SIMPLE != 0 {
            gl.base_flags |= FT_OUTLINE_OVERLAP;
        }
        let mut x: FtPos = 0;
        for i in 0..np {
            let fl = gl.tags[pt + i];
            let mut delta: FtPos = 0;
            if fl & X_SHORT_VECTOR != 0 {
                if f.cursor + 1 > limit {
                    return FT_ERR_INVALID_OUTLINE;
                }
                delta = f.get_byte() as FtPos;
                if fl & X_POSITIVE == 0 {
                    delta = -delta;
                }
            } else if fl & SAME_X == 0 {
                if f.cursor + 2 > limit {
                    return FT_ERR_INVALID_OUTLINE;
                }
                delta = f.get_short() as FtPos;
            }
            x = x.wrapping_add(delta);
            gl.points[pt + i].x = x;
        }
        let mut y: FtPos = 0;
        for i in 0..np {
            let fl = gl.tags[pt + i];
            let mut delta: FtPos = 0;
            if fl & Y_SHORT_VECTOR != 0 {
                if f.cursor + 1 > limit {
                    return FT_ERR_INVALID_OUTLINE;
                }
                delta = f.get_byte() as FtPos;
                if fl & Y_POSITIVE == 0 {
                    delta = -delta;
                }
            } else if fl & SAME_Y == 0 {
                if f.cursor + 2 > limit {
                    return FT_ERR_INVALID_OUTLINE;
                }
                delta = f.get_short() as FtPos;
            }
            y = y.wrapping_add(delta);
            gl.points[pt + i].y = y;
            gl.tags[pt + i] = fl & ON_CURVE_POINT;
        }
        gl.cur_n_points = n_points as u16;
        gl.cur_n_contours = n_contours as u16;
        FT_ERR_OK
    }

    fn load_composite_glyph(&mut self) -> FtError {
        let mut f = self.frame.take().unwrap();
        let e = self.load_composite_glyph_in(&mut f);
        self.frame = Some(f);
        e
    }

    fn load_composite_glyph_in(&mut self, f: &mut Frame) -> FtError {
        let limit = f.limit;
        let num_glyphs = self.face.num_glyphs;
        let gl = &mut self.glyph.loader;
        let mut num_subglyphs: u32 = 0;
        loop {
            let error = gl.check_subglyphs(num_subglyphs + 1);
            if error != 0 {
                return error;
            }
            if f.cursor + 4 > limit {
                return FT_ERR_INVALID_COMPOSITE;
            }
            let si = (gl.base_num_subglyphs + num_subglyphs) as usize;
            let sg = &mut gl.subglyphs[si];
            sg.arg1 = 0;
            sg.arg2 = 0;
            sg.flags = f.get_ushort();
            sg.index = f.get_ushort() as i32;
            if sg.index >= num_glyphs {
                return FT_ERR_INVALID_COMPOSITE;
            }
            let mut count = 2usize;
            if sg.flags & ARGS_ARE_WORDS != 0 {
                count += 2;
            }
            if sg.flags & WE_HAVE_A_SCALE != 0 {
                count += 2;
            } else if sg.flags & WE_HAVE_AN_XY_SCALE != 0 {
                count += 4;
            } else if sg.flags & WE_HAVE_A_2X2 != 0 {
                count += 8;
            }
            if f.cursor + count > limit {
                return FT_ERR_INVALID_COMPOSITE;
            }
            if sg.flags & ARGS_ARE_XY_VALUES != 0 {
                if sg.flags & ARGS_ARE_WORDS != 0 {
                    sg.arg1 = f.get_short() as i32;
                    sg.arg2 = f.get_short() as i32;
                } else {
                    sg.arg1 = f.get_char() as i32;
                    sg.arg2 = f.get_char() as i32;
                }
            } else if sg.flags & ARGS_ARE_WORDS != 0 {
                sg.arg1 = f.get_ushort() as i32;
                sg.arg2 = f.get_ushort() as i32;
            } else {
                sg.arg1 = f.get_byte() as i32;
                sg.arg2 = f.get_byte() as i32;
            }
            let (mut xx, mut xy, mut yx, mut yy): (FtFixed, FtFixed, FtFixed, FtFixed) = (0x10000, 0, 0, 0x10000);
            if sg.flags & WE_HAVE_A_SCALE != 0 {
                xx = f.get_short() as i32 * 4;
                yy = xx;
            } else if sg.flags & WE_HAVE_AN_XY_SCALE != 0 {
                xx = f.get_short() as i32 * 4;
                yy = f.get_short() as i32 * 4;
            } else if sg.flags & WE_HAVE_A_2X2 != 0 {
                xx = f.get_short() as i32 * 4;
                yx = f.get_short() as i32 * 4;
                xy = f.get_short() as i32 * 4;
                yy = f.get_short() as i32 * 4;
            }
            sg.transform = FtMatrix { xx, xy, yx, yy };
            num_subglyphs += 1;
            if sg.flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
        gl.cur_num_subglyphs = num_subglyphs;
        self.ins_pos = f.cursor as u32;
        FT_ERR_OK
    }

    fn prepare_zone(gl: &FtGlyphLoader, current: bool, start_point: u32, start_contour: u32) -> ZoneRef {
        let (pt0, ct0, n_points, n_contours) = if current {
            (gl.base_n_points as usize, gl.base_n_contours as usize, gl.cur_n_points, gl.cur_n_contours)
        } else {
            (0, 0, gl.base_n_points, gl.base_n_contours)
        };
        ZoneRef {
            n_points: n_points.wrapping_add(4).wrapping_sub(start_point as u16),
            n_contours: n_contours.wrapping_sub(start_contour as u16),
            pt: pt0 + start_point as usize,
            ct: ct0 + start_contour as usize,
            first_point: start_point as u16,
        }
    }

    fn hint_glyph(&mut self, zone: ZoneRef, is_composite: bool) -> FtError {
        let size_metrics = *self.size.metrics();
        let mut exec = self.exec.take().unwrap();
        let gl = &mut self.glyph.loader;
        let n = zone.n_points as usize;
        let (pt, ct) = (zone.pt, zone.ct);
        let n_ins = exec.glyph_size;
        if n_ins > 0 {
            gl.extra_points[pt..pt + n].copy_from_slice(&gl.points[pt..pt + n]);
        }
        if is_composite {
            exec.metrics.x_scale = 1 << 16;
            exec.metrics.y_scale = 1 << 16;
            gl.extra_points2[pt..pt + n].copy_from_slice(&gl.points[pt..pt + n]);
        } else {
            exec.metrics.x_scale = size_metrics.x_scale;
            exec.metrics.y_scale = size_metrics.y_scale;
        }
        gl.points[pt + n - 4].x = ft_pix_round(gl.points[pt + n - 4].x);
        gl.points[pt + n - 3].x = ft_pix_round(gl.points[pt + n - 3].x);
        gl.points[pt + n - 2].y = ft_pix_round(gl.points[pt + n - 2].y);
        gl.points[pt + n - 1].y = ft_pix_round(gl.points[pt + n - 1].y);
        if n_ins > 0 {
            let ins = exec.glyph_ins.clone();
            exec.set_code_range(TT_CODERANGE_GLYPH, ins, n_ins as i32);
            exec.is_composite = is_composite;
            exec.pts = TtGlyphZone {
                n_points: zone.n_points,
                n_contours: zone.n_contours,
                org: gl.extra_points[pt..pt + n].to_vec(),
                cur: gl.points[pt..pt + n].to_vec(),
                orus: gl.extra_points2[pt..pt + n].to_vec(),
                tags: gl.tags[pt..pt + n].to_vec(),
                contours: gl.contours[ct..ct + zone.n_contours as usize].to_vec(),
                first_point: zone.first_point,
            };
            let error = exec.run_context(&self.size.gs);
            gl.extra_points[pt..pt + n].copy_from_slice(&exec.pts.org[..n]);
            gl.points[pt..pt + n].copy_from_slice(&exec.pts.cur[..n]);
            gl.extra_points2[pt..pt + n].copy_from_slice(&exec.pts.orus[..n]);
            gl.tags[pt..pt + n].copy_from_slice(&exec.pts.tags[..n]);
            if error != 0 && exec.pedantic_hinting {
                self.exec = Some(exec);
                return error;
            }
            let t0 = gl.base_n_points as usize;
            gl.tags[t0] |= ((exec.gs.scan_type << 5) as u8) | FT_CURVE_TAG_HAS_SCANMODE;
        }
        self.exec = Some(exec);
        self.pp1 = gl.points[pt + n - 4];
        self.pp2 = gl.points[pt + n - 3];
        self.pp3 = gl.points[pt + n - 2];
        self.pp4 = gl.points[pt + n - 1];
        FT_ERR_OK
    }

    fn process_simple_glyph(&mut self) -> FtError {
        let mut error = FT_ERR_OK;
        let hinted = is_hinted(self.load_flags);
        let gl = &mut self.glyph.loader;
        let pt = gl.base_n_points as usize;
        let mut n_points = gl.cur_n_points as usize;
        gl.points[pt + n_points] = self.pp1;
        gl.points[pt + n_points + 1] = self.pp2;
        gl.points[pt + n_points + 2] = self.pp3;
        gl.points[pt + n_points + 3] = self.pp4;
        n_points += 4;
        let mut zone = ZoneRef::default();
        if hinted {
            zone = Self::prepare_zone(gl, true, 0, 0);
            let zn = zone.n_points as usize;
            gl.extra_points2[zone.pt..zone.pt + zn].copy_from_slice(&gl.points[zone.pt..zone.pt + zn]);
        }
        if self.load_flags & FT_LOAD_NO_SCALE == 0 {
            let m = self.size.metrics();
            let (x_scale, y_scale) = (m.x_scale, m.y_scale);
            for v in &mut gl.points[pt..pt + n_points] {
                v.x = ft_mul_fix(v.x, x_scale);
                v.y = ft_mul_fix(v.y, y_scale);
            }
        }
        self.pp1 = gl.points[pt + n_points - 4];
        self.pp2 = gl.points[pt + n_points - 3];
        self.pp3 = gl.points[pt + n_points - 2];
        self.pp4 = gl.points[pt + n_points - 1];
        if hinted {
            error = self.hint_glyph(zone, false);
        }
        error
    }

    fn process_composite_component(&mut self, subglyph_index: usize, start_point: u32, num_base_points: u32) -> FtError {
        let sg = self.glyph.loader.subglyphs[subglyph_index];
        let have_scale = sg.flags & (WE_HAVE_A_SCALE | WE_HAVE_AN_XY_SCALE | WE_HAVE_A_2X2) != 0;
        let gl = &mut self.glyph.loader;
        let num_points = gl.base_n_points as u32;
        let cur = num_base_points as usize..num_points as usize;
        if have_scale {
            ft_points_transform(&mut gl.points[cur.clone()], &sg.transform);
        }
        let (mut x, mut y);
        if sg.flags & ARGS_ARE_XY_VALUES == 0 {
            let k = (sg.arg1 as u32).wrapping_add(start_point);
            let l = (sg.arg2 as u32).wrapping_add(num_base_points);
            if k >= num_base_points || l >= num_points {
                return FT_ERR_INVALID_COMPOSITE;
            }
            let p1 = gl.points[k as usize];
            let p2 = gl.points[l as usize];
            x = sub_long(p1.x, p2.x);
            y = sub_long(p1.y, p2.y);
        } else {
            x = sg.arg1;
            y = sg.arg2;
            if x == 0 && y == 0 {
                return FT_ERR_OK;
            }
            if have_scale && sg.flags & SCALED_COMPONENT_OFFSET != 0 {
                let mac_xscale = ft_hypot(sg.transform.xx, sg.transform.xy);
                let mac_yscale = ft_hypot(sg.transform.yy, sg.transform.yx);
                x = ft_mul_fix(x, mac_xscale);
                y = ft_mul_fix(y, mac_yscale);
            }
            if self.load_flags & FT_LOAD_NO_SCALE == 0 {
                let m = self.size.metrics();
                x = ft_mul_fix(x, m.x_scale);
                y = ft_mul_fix(y, m.y_scale);
                if sg.flags & ROUND_XY_TO_GRID != 0 && is_hinted(self.load_flags) {
                    x = ft_pix_round(x);
                    y = ft_pix_round(y);
                }
            }
        }
        if x != 0 || y != 0 {
            ft_points_translate(&mut self.glyph.loader.points[cur], x, y);
        }
        FT_ERR_OK
    }

    fn process_composite_glyph(&mut self, start_point: u32, start_contour: u32) -> FtError {
        let gl = &mut self.glyph.loader;
        let n = gl.base_n_points as u32;
        let error = check_points(gl, n + 4, 0);
        if error != 0 {
            return error;
        }
        let n = n as usize;
        gl.points[n] = self.pp1;
        gl.points[n + 1] = self.pp2;
        gl.points[n + 2] = self.pp3;
        gl.points[n + 3] = self.pp4;
        {
            let exec = self.exec.as_mut().unwrap();
            exec.glyph_ins = Rc::new(Vec::new());
            exec.glyph_size = 0;
            let stream = &mut self.face.tt.stream;
            if let Err(e) = stream.seek(self.ins_pos) {
                return e;
            }
            let n_ins = match stream.read_ushort() {
                Ok(v) => v,
                Err(e) => return e,
            };
            if n_ins == 0 {
                return FT_ERR_OK;
            }
            if n_ins as u32 > self.byte_len {
                return FT_ERR_TOO_MANY_HINTS;
            }
            let ins = match stream.extract(n_ins as u32) {
                Ok(v) => v,
                Err(e) => return e,
            };
            exec.glyph_ins = Rc::new(ins);
            exec.glyph_size = n_ins as u32;
        }
        let gl = &mut self.glyph.loader;
        let zone = Self::prepare_zone(gl, false, start_point, start_contour);
        for i in 0..(zone.n_points as u32).wrapping_sub(4) as usize {
            gl.tags[zone.pt + i] &= !FT_CURVE_TAG_TOUCH_BOTH;
        }
        self.hint_glyph(zone, true)
    }

    fn set_pp(&mut self) {
        self.pp1.x = self.bbox.x_min.wrapping_sub(self.left_bearing);
        self.pp1.y = 0;
        self.pp2.x = self.pp1.x.wrapping_add(self.advance);
        self.pp2.y = 0;
        self.pp3.x = 0;
        self.pp3.y = self.bbox.y_max.wrapping_add(self.top_bearing);
        self.pp4.x = 0;
        self.pp4.y = self.pp3.y.wrapping_sub(self.vadvance);
    }

    fn scale_phantoms(&mut self, x_scale: FtFixed, y_scale: FtFixed) {
        if self.load_flags & FT_LOAD_NO_SCALE == 0 {
            self.pp1.x = ft_mul_fix(self.pp1.x, x_scale);
            self.pp2.x = ft_mul_fix(self.pp2.x, x_scale);
            self.pp3.x = ft_mul_fix(self.pp3.x, x_scale);
            self.pp3.y = ft_mul_fix(self.pp3.y, y_scale);
            self.pp4.x = ft_mul_fix(self.pp4.x, x_scale);
            self.pp4.y = ft_mul_fix(self.pp4.y, y_scale);
        }
    }

    fn load_truetype_glyph(&mut self, glyph_index: u32, recurse_count: u32, header_only: bool) -> FtError {
        let error = self.load_truetype_glyph_in(glyph_index, recurse_count, header_only);
        self.frame = None;
        error
    }

    fn load_truetype_glyph_in(&mut self, glyph_index: u32, recurse_count: u32, header_only: bool) -> FtError {
        let mp = &mut self.face.tt.max_profile;
        if recurse_count > mp.max_component_depth as u32 {
            mp.max_component_depth = recurse_count as u16;
        }
        self.glyph_index = glyph_index;
        let (x_scale, y_scale) = if self.load_flags & FT_LOAD_NO_SCALE != 0 {
            (0x10000, 0x10000)
        } else {
            let m = self.size.metrics();
            (m.x_scale, m.y_scale)
        };
        let (offset, len) = tt_face_get_location(&self.face.tt, glyph_index);
        self.byte_len = len;
        let glyf_offset = self.face.tt.glyf_offset;
        if self.byte_len > 0 {
            if glyf_offset == 0 {
                return FT_ERR_INVALID_TABLE;
            }
            let error = self.access_glyph_frame(glyf_offset.wrapping_add(offset), self.byte_len);
            if error != 0 {
                return error;
            }
            let error = self.load_glyph_header();
            self.forget_glyph_frame();
            if error != 0 {
                return error;
            }
        }
        if self.byte_len == 0 || self.n_contours == 0 {
            self.bbox = FtBBox::default();
        }
        let error = self.tt_get_metrics(glyph_index);
        if error != 0 {
            return error;
        }
        if header_only {
            return FT_ERR_OK;
        }
        self.set_pp();
        if self.byte_len == 0 || self.n_contours == 0 {
            self.scale_phantoms(x_scale, y_scale);
            return FT_ERR_OK;
        }
        let error = self.access_glyph_frame(glyf_offset.wrapping_add(offset).wrapping_add(10), self.byte_len.wrapping_sub(10));
        if error != 0 {
            return error;
        }
        if self.n_contours > 0 {
            let error = self.load_simple_glyph();
            if error != 0 {
                return error;
            }
            self.forget_glyph_frame();
            let error = self.process_simple_glyph();
            if error != 0 {
                return error;
            }
            self.glyph.loader.add();
        } else {
            self.n_contours = -1;
            let rc = recurse_count as usize;
            for d in self.composites.iter_mut().skip(rc) {
                *d = u32::MAX;
            }
            if self.composites.contains(&glyph_index) {
                return FT_ERR_INVALID_COMPOSITE;
            } else if rc < self.composites.len() {
                self.composites[rc] = glyph_index;
            } else {
                self.composites.push(glyph_index);
            }
            let start_point = self.glyph.loader.base_n_points as u32;
            let start_contour = self.glyph.loader.base_n_contours as u32;
            let error = self.load_composite_glyph();
            if error != 0 {
                return error;
            }
            let ins_pos = self.ins_pos;
            self.forget_glyph_frame();
            self.scale_phantoms(x_scale, y_scale);
            if self.load_flags & FT_LOAD_NO_RECURSE != 0 {
                self.glyph.loader.add();
                self.glyph.format = FT_GLYPH_FORMAT_COMPOSITE;
                return FT_ERR_OK;
            }
            let mut num_points = start_point;
            let num_subglyphs = self.glyph.loader.cur_num_subglyphs;
            let num_base_subgs = self.glyph.loader.base_num_subglyphs;
            let old_byte_len = self.byte_len;
            let mut last_flags: Option<u16> = None;
            self.glyph.loader.add();
            for n in 0..num_subglyphs {
                let si = (num_base_subgs + n) as usize;
                let pp = [self.pp1, self.pp2, self.pp3, self.pp4];
                let linear_hadvance = self.linear;
                let linear_vadvance = self.vadvance;
                let num_base_points = self.glyph.loader.base_n_points as u32;
                let sub_index = self.glyph.loader.subglyphs[si].index as u32;
                let error = self.load_truetype_glyph(sub_index, recurse_count + 1, false);
                if error != 0 {
                    return error;
                }
                let flags = self.glyph.loader.subglyphs[si].flags;
                last_flags = Some(flags);
                if flags & USE_MY_METRICS == 0 {
                    self.pp1 = pp[0];
                    self.pp2 = pp[1];
                    self.pp3 = pp[2];
                    self.pp4 = pp[3];
                    self.linear = linear_hadvance;
                    self.vadvance = linear_vadvance;
                }
                num_points = self.glyph.loader.base_n_points as u32;
                if num_points == num_base_points {
                    continue;
                }
                let error = self.process_composite_component(si, start_point, num_base_points);
                if error != 0 {
                    return error;
                }
            }
            self.byte_len = old_byte_len;
            self.ins_pos = ins_pos;
            if is_hinted(self.load_flags) && last_flags.map_or(false, |f| f & WE_HAVE_INSTR != 0) && num_points > start_point {
                let error = self.process_composite_glyph(start_point, start_contour);
                if error != 0 {
                    return error;
                }
            }
            let gl = &mut self.glyph.loader;
            if gl.base_num_subglyphs != 0 && gl.subglyphs[0].flags & OVERLAP_COMPOUND != 0 {
                gl.base_flags |= FT_OUTLINE_OVERLAP;
            }
        }
        FT_ERR_OK
    }

    fn compute_glyph_metrics(&mut self, glyph_index: u32) -> FtError {
        let y_scale = if self.load_flags & FT_LOAD_NO_SCALE == 0 { self.size.metrics().y_scale } else { 0x10000 };
        let bbox = if self.glyph.format != FT_GLYPH_FORMAT_COMPOSITE { ft_outline_get_cbox(&self.glyph.outline) } else { self.bbox };
        let tt = &self.face.tt;
        let glyph = &mut *self.glyph;
        glyph.linear_hori_advance = self.linear;
        glyph.metrics.hori_bearing_x = bbox.x_min;
        glyph.metrics.hori_bearing_y = bbox.y_max;
        glyph.metrics.hori_advance = match self.widthp {
            Some(w) => tt.hdmx_table[w + glyph_index as usize] as i32 * 64,
            None => sub_long(self.pp2.x, self.pp1.x),
        };
        glyph.metrics.width = sub_long(bbox.x_max, bbox.x_min);
        glyph.metrics.height = sub_long(bbox.y_max, bbox.y_min);
        let mut top: FtPos;
        let mut advance: FtPos;
        if tt.vertical_info && tt.vertical.number_of_hmetrics > 0 {
            top = ft_div_fix(sub_long(self.pp3.y, bbox.y_max), y_scale) as i16 as FtPos;
            if self.pp3.y <= self.pp4.y {
                advance = 0;
            } else {
                advance = ft_div_fix(sub_long(self.pp3.y, self.pp4.y), y_scale) as u16 as FtPos;
            }
        } else {
            let height = ft_div_fix(sub_long(bbox.y_max, bbox.y_min), y_scale) as i16 as FtPos;
            if tt.os2.version != 0xFFFF {
                advance = tt.os2.s_typo_ascender as FtPos - tt.os2.s_typo_descender as FtPos;
            } else {
                advance = tt.horizontal.ascender as FtPos - tt.horizontal.descender as FtPos;
            }
            top = (advance - height) / 2;
        }
        glyph.linear_vert_advance = advance;
        if self.load_flags & FT_LOAD_NO_SCALE == 0 {
            top = ft_mul_fix(top, y_scale);
            advance = ft_mul_fix(advance, y_scale);
        }
        glyph.metrics.vert_bearing_x = sub_long(glyph.metrics.hori_bearing_x, glyph.metrics.hori_advance / 2);
        glyph.metrics.vert_bearing_y = top;
        glyph.metrics.vert_advance = advance;
        FT_ERR_OK
    }

    fn init(face: &'a mut FtFace, size: &'a mut TtSize, glyph: &'a mut FtGlyphSlot, load_flags: i32, glyf_table_only: bool) -> Result<TtLoader<'a>, FtError> {
        let mut load_flags = load_flags;
        let mut exec_out = None;
        let mut widthp = None;
        if is_hinted(load_flags) && !glyf_table_only {
            let mode = ft_load_target_mode(load_flags);
            let grayscale = mode != FT_RENDER_MODE_MONO;
            let mut reexecute = false;
            if size.bytecode_ready > 0 {
                return Err(size.bytecode_ready);
            }
            if size.bytecode_ready < 0 {
                let pedantic = load_flags & FT_LOAD_PEDANTIC != 0;
                let error = tt_size_init_bytecode(face, size, pedantic);
                if error != 0 {
                    return Err(error);
                }
            }
            {
                let exec = size.context.as_mut().unwrap();
                exec.backward_compatibility = 0;
                if grayscale != exec.grayscale {
                    exec.grayscale = grayscale;
                    reexecute = true;
                }
            }
            if size.cvt_ready > 0 {
                return Err(size.cvt_ready);
            }
            if size.cvt_ready < 0 || reexecute {
                let error = tt_size_run_prep(face, size);
                if error != 0 {
                    return Err(error);
                }
            }
            let metrics = *size.metrics();
            let (ps, ttm, tw) = (size.point_size, size.ttmetrics, size.twilight_n_points);
            let mut exec = size.context.take().unwrap();
            exec.load_context(ps, ttm, metrics, tw);
            if size.gs.instruct_control & 1 != 0 {
                load_flags |= FT_LOAD_NO_HINTING;
            }
            if size.gs.instruct_control & 2 != 0 {
                size.gs = TT_DEFAULT_GRAPHICS_STATE;
            }
            if is_hinted(load_flags) && load_flags & FT_LOAD_COMPUTE_METRICS == 0 && face.tt.postscript.is_fixed_pitch == 0 {
                widthp = size.widthp;
            }
            exec_out = Some(exec);
        }
        if !glyf_table_only {
            glyph.loader.rewind();
        }
        Ok(TtLoader {
            face,
            size,
            glyph,
            exec: exec_out,
            load_flags,
            glyph_index: 0,
            byte_len: 0,
            n_contours: 0,
            bbox: FtBBox::default(),
            left_bearing: 0,
            advance: 0,
            linear: 0,
            linear_def: false,
            pp1: FtVector::default(),
            pp2: FtVector::default(),
            ins_pos: 0,
            top_bearing: 0,
            vadvance: 0,
            pp3: FtVector::default(),
            pp4: FtVector::default(),
            frame: None,
            composites: Vec::new(),
            widthp,
        })
    }

    fn done(mut self) {
        if let Some(exec) = self.exec.take() {
            self.size.context = Some(exec);
        }
    }
}

/// `TT_Load_Glyph`.
pub fn tt_load_glyph(face: &mut FtFace, size: &mut TtSize, glyph: &mut FtGlyphSlot, glyph_index: u32, load_flags: i32) -> FtError {
    if size.strike_index != 0xFFFF_FFFF && !(load_flags & FT_LOAD_NO_BITMAP != 0 && face.is_scalable()) {
        // embedded bitmaps are not ported; the JDK's anti-aliased contexts never reach them
        if !face.is_scalable() {
            return FT_ERR_UNIMPLEMENTED_FEATURE;
        }
    }
    if load_flags & FT_LOAD_SBITS_ONLY != 0 {
        return FT_ERR_INVALID_ARGUMENT;
    }
    if load_flags & FT_LOAD_NO_SCALE == 0 && size.ttmetrics.ppem == 0 {
        return FT_ERR_INVALID_SIZE_HANDLE;
    }
    let mut loader = match TtLoader::init(face, size, glyph, load_flags, false) {
        Ok(l) => l,
        Err(e) => return e,
    };
    if load_flags & FT_LOAD_ADVANCE_ONLY != 0 && load_flags & FT_LOAD_VERTICAL_LAYOUT == 0 {
        if let Some(w) = loader.widthp {
            loader.glyph.metrics.hori_advance = loader.face.tt.hdmx_table[w + glyph_index as usize] as i32 * 64;
            loader.done();
            return FT_ERR_OK;
        }
    }
    let mut error = loader.load_truetype_glyph(glyph_index, 0, false);
    if error == 0 {
        if loader.glyph.format == FT_GLYPH_FORMAT_COMPOSITE {
            loader.glyph.num_subglyphs = loader.glyph.loader.base_num_subglyphs;
        } else {
            let gl = &loader.glyph.loader;
            let np = gl.base_n_points as usize;
            let nc = gl.base_n_contours as usize;
            let mut outline = FtOutline {
                n_contours: nc as i32,
                n_points: np as i32,
                points: gl.points[..np].to_vec(),
                tags: gl.tags[..np].to_vec(),
                contours: gl.contours[..nc].to_vec(),
                flags: gl.base_flags,
            };
            outline.flags &= !FT_OUTLINE_SINGLE_PASS;
            if load_flags & FT_LOAD_NO_SCALE == 0 && loader.size.metrics().y_ppem < 24 {
                outline.flags |= FT_OUTLINE_HIGH_PRECISION;
            }
            if loader.pp1.x != 0 {
                ft_outline_translate(&mut outline, loader.pp1.x.wrapping_neg(), 0);
            }
            loader.glyph.format = FT_GLYPH_FORMAT_OUTLINE;
            loader.glyph.outline = outline;
        }
        if is_hinted(load_flags) {
            let exec = loader.exec.as_ref().unwrap();
            loader.glyph.control_len = exec.glyph_size;
            let flags = &mut loader.glyph.outline.flags;
            if exec.gs.scan_control {
                match exec.gs.scan_type {
                    0 => *flags |= FT_OUTLINE_INCLUDE_STUBS,
                    1 => {}
                    4 => *flags |= FT_OUTLINE_SMART_DROPOUTS | FT_OUTLINE_INCLUDE_STUBS,
                    5 => *flags |= FT_OUTLINE_SMART_DROPOUTS,
                    _ => *flags |= FT_OUTLINE_IGNORE_DROPOUTS,
                }
            } else {
                *flags |= FT_OUTLINE_IGNORE_DROPOUTS;
            }
        }
        error = loader.compute_glyph_metrics(glyph_index);
    }
    loader.done();
    error
}
