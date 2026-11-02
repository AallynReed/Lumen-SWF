//! Port of FreeType 2.14.2 `src/sfnt/sfobjs.c` (as built into JDK 17.0.19). Charmaps, kerning, color and SVG tables are not built.

use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::*;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtResult;
use crate::asc::flex2::fonts::java2d::freetype::ttload::*;
use crate::asc::flex2::fonts::java2d::freetype::ttmtx::*;
use crate::asc::flex2::fonts::java2d::freetype::ttsbit::*;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

fn tt_name_ascii_from_utf16(s: &[u8]) -> String {
    let len = s.len() / 2;
    let mut out = String::new();
    for n in 0..len {
        let mut code = u16::from_be_bytes([s[2 * n], s[2 * n + 1]]) as u32;
        if code == 0 {
            break;
        }
        if code < 32 || code > 127 {
            code = b'?' as u32;
        }
        out.push(code as u8 as char);
    }
    out
}

fn tt_name_ascii_from_other(s: &[u8]) -> String {
    let mut out = String::new();
    for &c in s {
        let mut code = c as u32;
        if code == 0 {
            break;
        }
        if code < 32 || code > 127 {
            code = b'?' as u32;
        }
        out.push(code as u8 as char);
    }
    out
}

pub fn tt_face_get_name(face: &mut TtFace, nameid: u16) -> FtResult<Option<String>> {
    let (mut found_apple, mut found_apple_roman, mut found_apple_english, mut found_win, mut found_unicode) =
        (-1i32, -1i32, -1i32, -1i32, -1i32);
    let mut is_english = false;
    for (n, rec) in face.names.iter().enumerate() {
        let n = n as i32;
        if rec.name_id == nameid && rec.string_length > 0 {
            match rec.platform_id {
                TT_PLATFORM_APPLE_UNICODE | TT_PLATFORM_ISO => found_unicode = n,
                TT_PLATFORM_MACINTOSH => {
                    if rec.language_id == TT_MAC_LANGID_ENGLISH {
                        found_apple_english = n;
                    } else if rec.encoding_id == TT_MAC_ID_ROMAN {
                        found_apple_roman = n;
                    }
                }
                TT_PLATFORM_MICROSOFT => {
                    if found_win == -1 || (rec.language_id & 0x3FF) == 0x009 {
                        match rec.encoding_id {
                            TT_MS_ID_SYMBOL_CS | TT_MS_ID_UNICODE_CS | TT_MS_ID_UCS_4 => {
                                is_english = (rec.language_id & 0x3FF) == 0x009;
                                found_win = n;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    found_apple = found_apple_roman;
    if found_apple_english >= 0 {
        found_apple = found_apple_english;
    }
    let mut pick: Option<(usize, bool)> = None;
    if found_win >= 0 && !(found_apple >= 0 && !is_english) {
        let rec = &face.names[found_win as usize];
        match rec.encoding_id {
            TT_MS_ID_UNICODE_CS | TT_MS_ID_SYMBOL_CS | TT_MS_ID_UCS_4 => pick = Some((found_win as usize, true)),
            _ => {}
        }
    } else if found_apple >= 0 {
        pick = Some((found_apple as usize, false));
    } else if found_unicode >= 0 {
        pick = Some((found_unicode as usize, true));
    }
    let Some((idx, utf16)) = pick else {
        return Ok(None);
    };
    if face.names[idx].string.is_none() {
        let (off, len) = (face.names[idx].string_offset, face.names[idx].string_length as u32);
        match face.stream.read_at(off, len) {
            Ok(v) => face.names[idx].string = Some(v),
            Err(_) => {
                face.names[idx].string_length = 0;
                return Ok(None);
            }
        }
    }
    let s = face.names[idx].string.as_ref().unwrap();
    Ok(Some(if utf16 { tt_name_ascii_from_utf16(s) } else { tt_name_ascii_from_other(s) }))
}

fn sfnt_open_font(face: &mut TtFace) -> FtResult<()> {
    face.ttc_version = 0;
    face.ttc_offsets.clear();
    let offset = face.stream.pos;
    let tag = face.stream.read_ulong()?;
    if tag != 0x0001_0000
        && tag != TTAG_TTCF
        && tag != TTAG_OTTO
        && tag != TTAG_TRUE
        && tag != TTAG_TYP1
        && tag != TTAG_0XA5KBD
        && tag != TTAG_0XA5LST
        && tag != 0x0002_0000
    {
        return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
    }
    if tag == TTAG_TTCF {
        let (version, count) = {
            let mut f = face.stream.enter_frame(8)?;
            (f.get_long(), f.get_long())
        };
        face.ttc_version = version;
        if count == 0 {
            return Err(FT_ERR_INVALID_TABLE);
        }
        if (count as u32) > face.stream.size / (28 + 4) {
            return Err(FT_ERR_ARRAY_TOO_LARGE);
        }
        let mut f = face.stream.enter_frame((count as u32).wrapping_mul(4))?;
        face.ttc_offsets = (0..count).map(|_| f.get_ulong()).collect();
    } else {
        face.ttc_version = 1 << 16;
        face.ttc_offsets = vec![offset];
    }
    Ok(())
}

pub fn sfnt_init_face(face: &mut FtFace, face_instance_index: i32) -> FtResult<()> {
    sfnt_open_font(&mut face.tt)?;
    let mut face_index = face_instance_index.wrapping_abs() & 0xFFFF;
    if face_instance_index < 0 && face_index > 0 {
        face_index -= 1;
    }
    if face_index >= face.tt.ttc_offsets.len() as i32 {
        if face_instance_index >= 0 {
            return Err(FT_ERR_INVALID_ARGUMENT);
        } else {
            face_index = 0;
        }
    }
    let off = face.tt.ttc_offsets[face_index as usize];
    face.tt.stream.seek(off)?;
    tt_face_load_font_dir(&mut face.tt)?;
    {
        let tt = &mut face.tt;
        let instance_index = face_instance_index.wrapping_abs() >> 16;
        let mut version = 0u32;
        let mut offset = 0u32;
        let mut num_axes = 0u16;
        let mut axis_size = 0u16;
        let mut num_instances = 0u16;
        let mut instance_size = 0u16;
        let mut fvar_len = 0u32;
        let r = (|| -> FtResult<()> {
            fvar_len = tt_face_goto_table(tt, TTAG_FVAR)?;
            if fvar_len < 20 {
                return Err(FT_ERR_INVALID_TABLE);
            }
            version = tt.stream.read_ulong()?;
            offset = tt.stream.read_ushort()? as u32;
            tt.stream.skip(2)?;
            num_axes = tt.stream.read_ushort()?;
            axis_size = tt.stream.read_ushort()?;
            num_instances = tt.stream.read_ushort()?;
            instance_size = tt.stream.read_ushort()?;
            Ok(())
        })();
        if r.is_err() {
            version = 0;
            offset = 0;
            num_axes = 0;
            axis_size = 0;
            num_instances = 0;
            instance_size = 0;
        }
        let mut num_instances = num_instances as u32;
        if version != 0x0001_0000
            || axis_size != 20
            || num_axes == 0
            || num_axes > 0x3FFE
            || !(instance_size as u32 == 4 + 4 * num_axes as u32 || instance_size as u32 == 6 + 4 * num_axes as u32)
            || num_instances > 0x7EFF
            || offset + axis_size as u32 * num_axes as u32 + instance_size as u32 * num_instances > fvar_len
        {
            num_instances = 0;
        } else {
            tt.variation_support |= TT_FACE_FLAG_VAR_FVAR;
        }
        if tt.variation_support & TT_FACE_FLAG_VAR_FVAR != 0 {
            let array_start = tt.stream.pos - 16 + offset;
            let mut default_value_offset = array_start + 8;
            let mut default_values = Vec::new();
            for _ in 0..num_axes {
                match tt.stream.read_at(default_value_offset, 4) {
                    Ok(v) => default_values.extend_from_slice(&v),
                    Err(_) => default_values.extend_from_slice(&[0; 4]),
                }
                default_value_offset += axis_size as u32;
            }
            let mut instance_offset = array_start + axis_size as u32 * num_axes as u32 + 4;
            let mut i = 0;
            while i < num_instances {
                let inst = tt.stream.read_at(instance_offset, num_axes as u32 * 4).unwrap_or_else(|_| vec![0; num_axes as usize * 4]);
                if inst == default_values {
                    break;
                }
                instance_offset += instance_size as u32;
                i += 1;
            }
            tt.var_default_named_instance = i + 1;
            if i == num_instances {
                num_instances += 1;
            }
        }
        if tt_face_goto_table(tt, TTAG_GLYF).is_err() && tt_face_goto_table(tt, TTAG_CFF2).is_err() && tt_face_goto_table(tt, TTAG_CFF).is_ok() {
            num_instances = 0;
        }
        if instance_index as u32 > num_instances {
            if face_instance_index >= 0 {
                return Err(FT_ERR_INVALID_ARGUMENT);
            } else {
                num_instances = 0;
            }
        }
        face.style_flags = (num_instances as i32) << 16;
    }
    face.num_faces = face.tt.ttc_offsets.len() as i32;
    face.face_index = face_instance_index;
    Ok(())
}

pub fn sfnt_load_face(face: &mut FtFace) -> FtResult<()> {
    let tt = &mut face.tt;
    let mut has_outline = tt_face_lookup_table(tt, TTAG_GLYF).is_some()
        || tt_face_lookup_table(tt, TTAG_CFF).is_some()
        || tt_face_lookup_table(tt, TTAG_CFF2).is_some();
    let has_cblc = tt_face_goto_table(tt, TTAG_CBLC).is_ok();
    let has_cbdt = tt_face_goto_table(tt, TTAG_CBDT).is_ok();
    let has_eblc = tt_face_goto_table(tt, TTAG_EBLC).is_ok();
    let has_bloc = tt_face_goto_table(tt, TTAG_BLOC).is_ok();
    let has_sbix = tt_face_goto_table(tt, TTAG_SBIX).is_ok();
    let mut is_apple_sbit = false;
    if !has_outline {
        is_apple_sbit = tt_face_load_bhed(tt).is_ok();
    }
    if !is_apple_sbit || has_sbix {
        tt_face_load_head(tt)?;
    }
    if has_cblc || has_cbdt {
        has_outline = false;
    }
    if tt.header.units_per_em < 16 || tt.header.units_per_em > 16384 {
        return Err(FT_ERR_INVALID_TABLE);
    }
    let _ = tt_face_load_maxp(tt);
    let _ = tt_face_load_cmap(tt);
    let _ = tt_face_load_name(tt);
    let psnames_error = tt_face_load_post(tt).err();
    if !is_apple_sbit {
        let mut error = tt_face_load_hhea(tt, false).err();
        if error.is_none() {
            error = tt_face_load_hmtx(tt, false).err();
            if error == Some(FT_ERR_TABLE_MISSING) {
                error = Some(FT_ERR_HMTX_TABLE_MISSING);
            }
        } else if error == Some(FT_ERR_TABLE_MISSING) {
            if tt.format_tag == TTAG_TRUE {
                has_outline = false;
                error = None;
            } else {
                error = Some(FT_ERR_HORIZ_HEADER_MISSING);
            }
        }
        if let Some(e) = error {
            return Err(e);
        }
        let mut error = tt_face_load_hhea(tt, true).err();
        if error.is_none() {
            error = tt_face_load_hmtx(tt, true).err();
            if error.is_none() {
                tt.vertical_info = true;
            }
        }
        if let Some(e) = error {
            if e != FT_ERR_TABLE_MISSING {
                return Err(e);
            }
        }
        if tt_face_load_os2(tt).is_err() {
            tt.os2.version = 0xFFFF;
        }
    }
    let mut sbit_table = Vec::new();
    if has_cblc || has_eblc || has_bloc || has_sbix {
        if let Ok(true) = tt_face_load_sbit(tt, &mut sbit_table) {
            face.face_flags |= FT_FACE_FLAG_SBIX_OVERLAY;
        }
    }
    let tt = &mut face.tt;
    face.num_glyphs = tt.max_profile.num_glyphs as i32;
    face.family_name = None;
    face.style_name = None;
    if tt.os2.version != 0xFFFF && tt.os2.fs_selection & 256 != 0 {
        face.family_name = tt_face_get_name(tt, TT_NAME_ID_TYPOGRAPHIC_FAMILY)?;
        if face.family_name.is_none() {
            face.family_name = tt_face_get_name(tt, TT_NAME_ID_FONT_FAMILY)?;
        }
        face.style_name = tt_face_get_name(tt, TT_NAME_ID_TYPOGRAPHIC_SUBFAMILY)?;
        if face.style_name.is_none() {
            face.style_name = tt_face_get_name(tt, TT_NAME_ID_FONT_SUBFAMILY)?;
        }
    } else {
        face.family_name = tt_face_get_name(tt, TT_NAME_ID_WWS_FAMILY)?;
        if face.family_name.is_none() {
            face.family_name = tt_face_get_name(tt, TT_NAME_ID_TYPOGRAPHIC_FAMILY)?;
        }
        if face.family_name.is_none() {
            face.family_name = tt_face_get_name(tt, TT_NAME_ID_FONT_FAMILY)?;
        }
        face.style_name = tt_face_get_name(tt, TT_NAME_ID_WWS_SUBFAMILY)?;
        if face.style_name.is_none() {
            face.style_name = tt_face_get_name(tt, TT_NAME_ID_TYPOGRAPHIC_SUBFAMILY)?;
        }
        if face.style_name.is_none() {
            face.style_name = tt_face_get_name(tt, TT_NAME_ID_FONT_SUBFAMILY)?;
        }
    }
    let mut flags = face.face_flags;
    if tt.sbit_table_type == TT_SBIT_TABLE_TYPE_CBLC || tt.sbit_table_type == TT_SBIT_TABLE_TYPE_SBIX {
        flags |= FT_FACE_FLAG_COLOR;
    }
    if has_outline {
        if has_sbix {
            flags |= FT_FACE_FLAG_SBIX;
        } else {
            flags |= FT_FACE_FLAG_SCALABLE;
        }
    }
    flags |= FT_FACE_FLAG_SFNT | FT_FACE_FLAG_HORIZONTAL;
    if psnames_error.is_none() && tt.postscript.format_type != 0x0003_0000 {
        flags |= FT_FACE_FLAG_GLYPH_NAMES;
    }
    if tt.postscript.is_fixed_pitch != 0 {
        flags |= FT_FACE_FLAG_FIXED_WIDTH;
    }
    if tt.vertical_info {
        flags |= FT_FACE_FLAG_VERTICAL;
    }
    if tt.variation_support & TT_FACE_FLAG_VAR_FVAR != 0 {
        flags |= FT_FACE_FLAG_MULTIPLE_MASTERS;
    }
    face.face_flags = flags;
    let mut sflags = 0;
    if has_outline && tt.os2.version != 0xFFFF {
        if tt.os2.fs_selection & 512 != 0 {
            sflags |= FT_STYLE_FLAG_ITALIC;
        } else if tt.os2.fs_selection & 1 != 0 {
            sflags |= FT_STYLE_FLAG_ITALIC;
        }
        if tt.os2.fs_selection & 32 != 0 {
            sflags |= FT_STYLE_FLAG_BOLD;
        }
    } else {
        if tt.header.mac_style & 1 != 0 {
            sflags |= FT_STYLE_FLAG_BOLD;
        }
        if tt.header.mac_style & 2 != 0 {
            sflags |= FT_STYLE_FLAG_ITALIC;
        }
    }
    face.style_flags |= sflags;
    let count = tt.sbit_num_strikes;
    if count > 0 {
        let mut em_size = tt.header.units_per_em as i32;
        let mut avgwidth = tt.os2.x_avg_char_width as i32;
        if em_size == 0 || tt.os2.version == 0xFFFF {
            avgwidth = 1;
            em_size = 1;
        }
        let mut sizes = Vec::new();
        let mut map = Vec::new();
        for strike_idx in 0..count {
            let Ok(metrics) = tt_face_load_strike_metrics(tt, &sbit_table, strike_idx) else {
                continue;
            };
            let bsize = FtBitmapSize {
                height: (metrics.height >> 6) as i16,
                width: ((avgwidth * metrics.x_ppem as i32 + em_size / 2) / em_size) as i16,
                x_ppem: (metrics.x_ppem as i32) << 6,
                y_ppem: (metrics.y_ppem as i32) << 6,
                size: (metrics.y_ppem as i32) << 6,
            };
            if bsize.x_ppem != 0 && bsize.y_ppem != 0 {
                sizes.push(bsize);
                map.push(strike_idx);
            }
        }
        if !map.is_empty() {
            tt.sbit_strike_map = map;
            face.face_flags |= FT_FACE_FLAG_FIXED_SIZES;
            face.num_fixed_sizes = sizes.len() as i32;
            face.available_sizes = sizes;
        }
    }
    face.tt.sbit_table = sbit_table;
    if !face.has_fixed_sizes() && !face.is_scalable() {
        face.face_flags |= FT_FACE_FLAG_SCALABLE;
    }
    if face.is_scalable() || face.has_sbix() {
        let tt = &face.tt;
        face.bbox = FtBBox { x_min: tt.header.x_min as i32, y_min: tt.header.y_min as i32, x_max: tt.header.x_max as i32, y_max: tt.header.y_max as i32 };
        face.units_per_em = tt.header.units_per_em;
        if tt.os2.version != 0xFFFF && tt.os2.fs_selection & 128 != 0 {
            face.ascender = tt.os2.s_typo_ascender;
            face.descender = tt.os2.s_typo_descender;
            face.height = (face.ascender as i32 - face.descender as i32 + tt.os2.s_typo_line_gap as i32) as i16;
        } else {
            face.ascender = tt.horizontal.ascender;
            face.descender = tt.horizontal.descender;
            face.height = (face.ascender as i32 - face.descender as i32 + tt.horizontal.line_gap as i32) as i16;
            if face.ascender == 0 && face.descender == 0 && tt.os2.version != 0xFFFF {
                if tt.os2.s_typo_ascender != 0 || tt.os2.s_typo_descender != 0 {
                    face.ascender = tt.os2.s_typo_ascender;
                    face.descender = tt.os2.s_typo_descender;
                    face.height = (face.ascender as i32 - face.descender as i32 + tt.os2.s_typo_line_gap as i32) as i16;
                } else {
                    face.ascender = tt.os2.us_win_ascent as i16;
                    face.descender = (tt.os2.us_win_descent as i16).wrapping_neg();
                    face.height = (face.ascender as i32 - face.descender as i32) as i16;
                }
            }
        }
        face.max_advance_width = tt.horizontal.advance_width_max as i16;
        face.max_advance_height = if tt.vertical_info { tt.vertical.advance_width_max as i16 } else { face.height };
        face.underline_position = (tt.postscript.underline_position as i32 - tt.postscript.underline_thickness as i32 / 2) as i16;
        face.underline_thickness = tt.postscript.underline_thickness;
    }
    Ok(())
}
