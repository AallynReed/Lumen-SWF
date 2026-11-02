//! Port of FreeType 2.14.2 `src/truetype/ttpload.c` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::FtFace;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::{peek_ulong, peek_ushort, FtResult};
use crate::asc::flex2::fonts::java2d::freetype::ttload::tt_face_goto_table;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

pub fn tt_face_load_loca(face: &mut FtFace) -> FtResult<()> {
    let tt = &mut face.tt;
    match tt_face_goto_table(tt, TTAG_GLYF) {
        Err(FT_ERR_TABLE_MISSING) => {
            tt.glyf_len = 0;
            tt.glyf_offset = 0;
        }
        Err(e) => return Err(e),
        Ok(len) => {
            tt.glyf_len = len;
            tt.glyf_offset = tt.stream.pos;
        }
    }
    let mut table_len = match tt_face_goto_table(tt, TTAG_LOCA) {
        Ok(l) => l,
        Err(_) => return Err(FT_ERR_LOCATIONS_MISSING),
    };
    let shift = if tt.header.index_to_loc_format != 0 { 2 } else { 1 };
    if table_len > 0x10000u32 << shift {
        table_len = 0x10000u32 << shift;
    }
    tt.num_locations = table_len >> shift;
    if tt.num_locations != (face.num_glyphs as u32).wrapping_add(1) {
        if tt.num_locations < (face.num_glyphs as u32).wrapping_add(1) {
            let new_loca_len = ((face.num_glyphs as u32).wrapping_add(1)) << shift;
            let pos = tt.stream.pos as i32;
            let mut dist: i32 = 0x7FFF_FFFF;
            let mut found = false;
            for entry in &tt.dir_tables {
                let diff = (entry.offset as i32).wrapping_sub(pos);
                if diff > 0 && diff < dist {
                    dist = diff;
                    found = true;
                }
            }
            if !found {
                dist = (tt.stream.size as i32).wrapping_sub(pos);
            }
            if new_loca_len <= dist as u32 {
                tt.num_locations = (face.num_glyphs as u32).wrapping_add(1);
                table_len = new_loca_len;
            } else {
                face.num_glyphs = if tt.num_locations != 0 { tt.num_locations as i32 - 1 } else { 0 };
            }
        }
    }
    tt.glyph_locations = tt.stream.extract(table_len)?;
    Ok(())
}

/// Returns `(offset, size)` of the glyph in the `glyf` table.
pub fn tt_face_get_location(tt: &TtFace, gindex: u32) -> (u32, u32) {
    let mut pos1: u32 = 0;
    let mut pos2: u32 = 0;
    if gindex < tt.num_locations {
        let loca = &tt.glyph_locations;
        if tt.header.index_to_loc_format != 0 {
            let p = gindex as usize * 4;
            let p_limit = tt.num_locations as usize * 4;
            pos1 = peek_ulong(loca, p);
            pos2 = pos1;
            if p + 8 <= p_limit {
                pos2 = peek_ulong(loca, p + 4);
            }
        } else {
            let p = gindex as usize * 2;
            let p_limit = tt.num_locations as usize * 2;
            pos1 = peek_ushort(loca, p) as u32;
            pos2 = pos1;
            if p + 4 <= p_limit {
                pos2 = peek_ushort(loca, p + 2) as u32;
            }
            pos1 <<= 1;
            pos2 <<= 1;
        }
    }
    if pos1 > tt.glyf_len {
        return (0, 0);
    }
    if pos2 > tt.glyf_len {
        if gindex == tt.num_locations.wrapping_sub(2) {
            pos2 = tt.glyf_len;
        } else {
            return (0, 0);
        }
    }
    let asize = if pos2 >= pos1 { pos2 - pos1 } else { tt.glyf_len - pos1 };
    (pos1, asize)
}

pub fn tt_face_load_cvt(tt: &mut TtFace) -> FtResult<()> {
    let table_len = match tt_face_goto_table(tt, TTAG_CVT) {
        Ok(l) => l,
        Err(_) => {
            tt.cvt = Vec::new();
            return Ok(());
        }
    };
    let cvt_size = table_len / 2;
    let mut f = tt.stream.enter_frame(cvt_size * 2)?;
    tt.cvt = (0..cvt_size).map(|_| f.get_short() as i32 * 64).collect();
    Ok(())
}

pub fn tt_face_load_fpgm(tt: &mut TtFace) -> FtResult<()> {
    match tt_face_goto_table(tt, TTAG_FPGM) {
        Err(_) => {
            tt.font_program = Vec::new();
            Ok(())
        }
        Ok(len) => {
            tt.font_program = tt.stream.extract(len)?;
            Ok(())
        }
    }
}

pub fn tt_face_load_prep(tt: &mut TtFace) -> FtResult<()> {
    match tt_face_goto_table(tt, TTAG_PREP) {
        Err(_) => {
            tt.cvt_program = Vec::new();
            Ok(())
        }
        Ok(len) => {
            tt.cvt_program = tt.stream.extract(len)?;
            Ok(())
        }
    }
}

pub fn tt_face_load_hdmx(face: &mut FtFace) -> FtResult<()> {
    let tt = &mut face.tt;
    let table_size = match tt_face_goto_table(tt, TTAG_HDMX) {
        Ok(s) if s >= 8 => s,
        _ => return Ok(()),
    };
    tt.hdmx_table = tt.stream.extract(table_size)?;
    let p = &tt.hdmx_table;
    let num_records = peek_ushort(p, 2) as u32;
    let mut record_size = peek_ulong(p, 4);
    if record_size >= 0xFFFF_0000 {
        record_size &= 0xFFFF;
    }
    let fail = num_records > 255
        || num_records == 0
        || record_size as i32 != ((face.num_glyphs.wrapping_add(2 + 3)) & !3);
    if fail {
        tt.hdmx_table = Vec::new();
        return Ok(());
    }
    let mut records = Vec::new();
    let mut at = 8usize;
    for _ in 0..num_records {
        if at + record_size as usize > table_size as usize {
            break;
        }
        records.push(at);
        at += record_size as usize;
    }
    let tbl = &tt.hdmx_table;
    records.sort_by(|a, b| tbl[*a].cmp(&tbl[*b]));
    tt.hdmx_record_count = records.len() as u32;
    tt.hdmx_records = records;
    tt.hdmx_record_size = record_size;
    Ok(())
}

/// Offset into `hdmx_table` of the widths for `ppem` (glyph 0), or None.
pub fn tt_face_get_device_metrics(tt: &TtFace, ppem: u32) -> Option<usize> {
    let mut min = 0u32;
    let mut max = tt.hdmx_record_count;
    while min < max {
        let mid = (min + max) >> 1;
        let rec = tt.hdmx_records[mid as usize];
        let v = tt.hdmx_table[rec] as u32;
        if v > ppem {
            max = mid;
        } else if v < ppem {
            min = mid + 1;
        } else {
            return Some(rec + 2);
        }
    }
    None
}
