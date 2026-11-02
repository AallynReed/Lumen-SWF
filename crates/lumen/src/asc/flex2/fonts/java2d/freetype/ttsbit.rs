//! Port of FreeType 2.14.2 `src/sfnt/ttsbit.c` (as built into JDK 17.0.19): strike directory and strike metrics only (bitmaps are never loaded, the JDK passes FT_LOAD_NO_BITMAP).

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::ft_div_fix;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::FtSizeMetrics;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtResult;
use crate::asc::flex2::fonts::java2d::freetype::ttload::tt_face_goto_table;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

pub const TT_SBIT_TABLE_TYPE_NONE: u32 = 0;
pub const TT_SBIT_TABLE_TYPE_EBLC: u32 = 1;
pub const TT_SBIT_TABLE_TYPE_CBLC: u32 = 2;
pub const TT_SBIT_TABLE_TYPE_SBIX: u32 = 3;

const TTAG_EBDT: u32 = tag(b"EBDT");
const TTAG_BDAT: u32 = tag(b"bdat");

/// Returns the `FT_FACE_FLAG_SBIX_OVERLAY` request as `Ok(true)`.
pub fn tt_face_load_sbit(face: &mut TtFace, sbit_table: &mut Vec<u8>) -> FtResult<bool> {
    let mut overlay = false;
    sbit_table.clear();
    face.sbit_table_type = TT_SBIT_TABLE_TYPE_NONE;
    face.sbit_num_strikes = 0;
    let r = (|| -> FtResult<()> {
        let mut res = tt_face_goto_table(face, TTAG_CBLC);
        if res.is_ok() {
            face.sbit_table_type = TT_SBIT_TABLE_TYPE_CBLC;
        } else {
            res = tt_face_goto_table(face, TTAG_EBLC);
            if res.is_err() {
                res = tt_face_goto_table(face, TTAG_BLOC);
            }
            if res.is_ok() {
                face.sbit_table_type = TT_SBIT_TABLE_TYPE_EBLC;
            }
        }
        if res.is_err() {
            res = tt_face_goto_table(face, TTAG_SBIX);
            if res.is_ok() {
                face.sbit_table_type = TT_SBIT_TABLE_TYPE_SBIX;
            }
        }
        let table_size = res?;
        if table_size < 8 {
            return Err(FT_ERR_INVALID_FILE_FORMAT);
        }
        match face.sbit_table_type {
            TT_SBIT_TABLE_TYPE_EBLC | TT_SBIT_TABLE_TYPE_CBLC => {
                *sbit_table = face.stream.extract(table_size)?;
                let p = &sbit_table[..];
                let version = u32::from_be_bytes([p[0], p[1], p[2], p[3]]);
                let num_strikes = u32::from_be_bytes([p[4], p[5], p[6], p[7]]);
                if (version & 0xFFFF_0000) != 0x0002_0000
                    && (version & 0x0000_FFFF) != 0x0000_0200
                    && (version & 0xFFFF_0000) != 0x0003_0000
                    && (version & 0x0000_FFFF) != 0x0000_0300
                {
                    return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
                }
                if num_strikes >= 0x10000 {
                    return Err(FT_ERR_INVALID_FILE_FORMAT);
                }
                let mut count = num_strikes;
                if 8 + 48 * count > table_size {
                    count = (table_size - 8) / 48;
                }
                face.sbit_num_strikes = count;
            }
            TT_SBIT_TABLE_TYPE_SBIX => {
                let (version, flags, num_strikes) = {
                    let mut f = face.stream.enter_frame(8)?;
                    (f.get_ushort(), f.get_ushort(), f.get_ulong())
                };
                if version < 1 {
                    return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
                }
                if !(flags == 1 || flags == 3) || num_strikes >= 0x10000 {
                    return Err(FT_ERR_INVALID_FILE_FORMAT);
                }
                if flags == 3 {
                    overlay = true;
                }
                let mut count = num_strikes;
                if 8 + 4 * count > table_size {
                    count = (table_size - 8) / 4;
                }
                let pos = face.stream.pos;
                face.stream.seek(pos - 8)?;
                *sbit_table = face.stream.extract(8 + count * 4)?;
                face.sbit_num_strikes = count;
            }
            _ => return Err(FT_ERR_UNKNOWN_FILE_FORMAT),
        }
        let mut ebdt_size = 0;
        if face.sbit_table_type == TT_SBIT_TABLE_TYPE_SBIX {
            ebdt_size = table_size;
        } else if face.sbit_table_type != TT_SBIT_TABLE_TYPE_NONE {
            let mut r = tt_face_goto_table(face, TTAG_CBDT);
            if r.is_err() {
                r = tt_face_goto_table(face, TTAG_EBDT);
            }
            if r.is_err() {
                r = tt_face_goto_table(face, TTAG_BDAT);
            }
            if let Ok(s) = r {
                ebdt_size = s;
            }
        }
        if ebdt_size == 0 {
            face.sbit_num_strikes = 0;
        }
        Ok(())
    })();
    if let Err(e) = r {
        sbit_table.clear();
        face.sbit_table_type = TT_SBIT_TABLE_TYPE_NONE;
        return Err(e);
    }
    Ok(overlay)
}

/// `tt_face_load_strike_metrics` before `sbit_strike_map` exists (raw strike index); sbix strikes are reported as unsupported.
pub fn tt_face_load_strike_metrics(face: &TtFace, sbit_table: &[u8], strike_index: u32) -> FtResult<FtSizeMetrics> {
    if strike_index >= face.sbit_num_strikes {
        return Err(FT_ERR_INVALID_ARGUMENT);
    }
    match face.sbit_table_type {
        TT_SBIT_TABLE_TYPE_EBLC | TT_SBIT_TABLE_TYPE_CBLC => {
            let strike = &sbit_table[8 + strike_index as usize * 48..];
            let mut m = FtSizeMetrics::default();
            m.x_ppem = strike[44] as u16;
            m.y_ppem = strike[45] as u16;
            m.ascender = (strike[16] as i8 as i32) * 64;
            m.descender = (strike[17] as i8 as i32) * 64;
            let max_before_bl = strike[24] as i8 as i32;
            let min_after_bl = strike[25] as i8 as i32;
            if m.descender > 0 {
                if min_after_bl < 0 {
                    m.descender = -m.descender;
                }
            } else if m.descender == 0 && m.ascender == 0 {
                if max_before_bl != 0 || min_after_bl != 0 {
                    m.ascender = max_before_bl * 64;
                    m.descender = min_after_bl * 64;
                } else {
                    m.ascender = m.y_ppem as i32 * 64;
                    m.descender = 0;
                }
            }
            m.height = m.ascender - m.descender;
            if m.height == 0 {
                m.height = m.y_ppem as i32 * 64;
                m.descender = m.ascender - m.height;
            }
            m.max_advance = ((strike[22] as i8 as i32) + strike[18] as i32 + (strike[23] as i8 as i32)) * 64;
            m.x_scale = ft_div_fix(m.x_ppem as i32 * 64, face.header.units_per_em as i32);
            m.y_scale = ft_div_fix(m.y_ppem as i32 * 64, face.header.units_per_em as i32);
            Ok(m)
        }
        _ => Err(FT_ERR_UNIMPLEMENTED_FEATURE),
    }
}
