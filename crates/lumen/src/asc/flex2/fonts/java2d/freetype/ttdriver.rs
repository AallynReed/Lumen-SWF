//! Port of FreeType 2.14.2 `src/truetype/ttdriver.c` (as built into JDK 17.0.19): the size and glyph-loading driver methods.

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::ft_mul_div;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::*;
use crate::asc::flex2::fonts::java2d::freetype::ttgload::tt_load_glyph;
use crate::asc::flex2::fonts::java2d::freetype::ttobjs::{tt_size_reset, TtSize};
use crate::asc::flex2::fonts::java2d::freetype::ttsbit::tt_face_load_strike_metrics;

pub fn tt_size_select(face: &FtFace, size: &mut TtSize, strike_index: u32) -> FtError {
    size.strike_index = strike_index;
    if face.is_scalable() {
        ft_select_metrics(face, &mut size.root_metrics, strike_index);
        let _ = tt_size_reset(face, size);
        FT_ERR_OK
    } else {
        let tt = &face.tt;
        match tt_face_load_strike_metrics(tt, &tt.sbit_table, tt.sbit_strike_map[strike_index as usize]) {
            Ok(m) => {
                size.root_metrics = m;
                FT_ERR_OK
            }
            Err(e) => {
                size.strike_index = 0xFFFF_FFFF;
                e
            }
        }
    }
}

pub fn tt_size_request(face: &FtFace, size: &mut TtSize, req: &FtSizeRequest) -> FtError {
    if face.has_fixed_sizes() {
        match ft_match_size(face, req, false) {
            Ok(strike_index) => return tt_size_select(face, size, strike_index),
            Err(_) => size.strike_index = 0xFFFF_FFFF,
        }
    }
    let err = ft_request_metrics(face, &mut size.root_metrics, req);
    if err != 0 {
        return err;
    }
    let mut error = FT_ERR_OK;
    if face.is_scalable() {
        error = tt_size_reset(face, size);
        if error == 0 {
            let m = size.metrics();
            let mut resolution = if m.x_ppem > m.y_ppem { req.hori_resolution } else { req.vert_resolution };
            if req.type_ == FT_SIZE_REQUEST_TYPE_SCALES || resolution == 0 {
                resolution = 72;
            }
            size.point_size = ft_mul_div(size.ttmetrics.ppem as i32, 64 * 72, resolution as i32);
        }
    }
    error
}

pub fn tt_glyph_load(face: &mut FtFace, size: &mut TtSize, slot: &mut FtGlyphSlot, glyph_index: u32, load_flags: i32) -> FtError {
    let mut load_flags = load_flags;
    if glyph_index >= face.num_glyphs as u32 {
        return FT_ERR_INVALID_ARGUMENT;
    }
    if load_flags & FT_LOAD_NO_HINTING != 0 {
        if face.is_tricky() {
            load_flags &= !FT_LOAD_NO_HINTING;
        }
        if load_flags & FT_LOAD_NO_AUTOHINT != 0 {
            load_flags |= FT_LOAD_NO_HINTING;
        }
    }
    if load_flags & (FT_LOAD_NO_RECURSE | FT_LOAD_NO_SCALE) != 0 {
        load_flags |= FT_LOAD_NO_BITMAP | FT_LOAD_NO_SCALE;
        if !face.is_tricky() {
            load_flags |= FT_LOAD_NO_HINTING;
        }
    }
    size.metrics_hinted = load_flags & FT_LOAD_NO_HINTING == 0;
    tt_load_glyph(face, size, slot, glyph_index, load_flags)
}
