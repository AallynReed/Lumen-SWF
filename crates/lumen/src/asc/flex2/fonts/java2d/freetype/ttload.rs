//! Port of FreeType 2.14.2 `src/sfnt/ttload.c` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ftstream::{FtResult, FtStream};
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

pub fn tt_face_lookup_table(face: &TtFace, tag: u32) -> Option<TtTable> {
    face.dir_tables.iter().find(|e| e.tag == tag && e.length != 0).copied()
}

/// Seeks to the table and returns its length.
pub fn tt_face_goto_table(face: &mut TtFace, tag: u32) -> FtResult<u32> {
    match tt_face_lookup_table(face, tag) {
        Some(t) => {
            face.stream.seek(t.offset)?;
            Ok(t.length)
        }
        None => Err(FT_ERR_TABLE_MISSING),
    }
}

fn read_table_entry(stream: &mut FtStream) -> FtResult<TtTable> {
    let mut f = stream.enter_frame(16)?;
    Ok(TtTable { tag: f.get_ulong(), check_sum: f.get_ulong(), offset: f.get_ulong(), length: f.get_ulong() })
}

struct SfntHeader {
    format_tag: u32,
    num_tables: u16,
    offset: u32,
}

fn check_table_dir(sfnt: &mut SfntHeader, stream: &mut FtStream) -> FtResult<u16> {
    let mut valid_entries: u16 = 0;
    let (mut has_head, mut has_sing, mut has_meta) = (false, false, false);
    let offset = sfnt.offset + 12;
    stream.seek(offset)?;
    let mut nn: u16 = 0;
    while nn < sfnt.num_tables {
        let table = match read_table_entry(stream) {
            Ok(t) => t,
            Err(_) => {
                sfnt.num_tables = nn;
                break;
            }
        };
        if table.offset > stream.size {
            nn += 1;
            continue;
        } else if table.length > stream.size - table.offset {
            if table.tag == TTAG_HMTX || table.tag == TTAG_VMTX {
                valid_entries += 1;
            } else {
                nn += 1;
                continue;
            }
        } else {
            valid_entries += 1;
        }
        if table.tag == TTAG_HEAD || table.tag == TTAG_BHED {
            has_head = true;
            if table.length < 0x36 {
                return Err(FT_ERR_TABLE_MISSING);
            }
            stream.seek(table.offset + 12)?;
            let _magic = stream.read_ulong()?;
            stream.seek(offset + (nn as u32 + 1) * 16)?;
        } else if table.tag == TTAG_SING {
            has_sing = true;
        } else if table.tag == TTAG_META {
            has_meta = true;
        }
        nn += 1;
    }
    if valid_entries == 0 {
        return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
    }
    if has_head || (has_sing && has_meta) {
        Ok(valid_entries)
    } else {
        Err(FT_ERR_TABLE_MISSING)
    }
}

pub fn tt_face_load_font_dir(face: &mut TtFace) -> FtResult<()> {
    let stream = &mut face.stream;
    let offset = stream.pos;
    let format_tag = stream.read_ulong()?;
    let num_tables = {
        let mut f = stream.enter_frame(8)?;
        f.get_ushort()
    };
    let mut sfnt = SfntHeader { format_tag, num_tables, offset };
    if sfnt.format_tag != TTAG_OTTO {
        check_table_dir(&mut sfnt, stream)?;
    } else if sfnt.num_tables == 0 {
        return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
    }
    face.format_tag = sfnt.format_tag;
    stream.seek(sfnt.offset + 12)?;
    let mut f = stream.enter_frame(sfnt.num_tables as u32 * 16)?;
    let size = stream.size;
    let mut tables: Vec<TtTable> = Vec::new();
    for _ in 0..sfnt.num_tables {
        let mut entry = TtTable { tag: f.get_ulong(), check_sum: f.get_ulong(), offset: f.get_ulong(), length: f.get_ulong() };
        if entry.offset > size {
            continue;
        } else if entry.length > size - entry.offset {
            if entry.tag == TTAG_HMTX || entry.tag == TTAG_VMTX {
                entry.length = (size - entry.offset) & !3;
            } else {
                continue;
            }
        }
        if tables.iter().any(|t| t.tag == entry.tag) {
            continue;
        }
        tables.push(entry);
    }
    face.dir_tables = tables;
    if face.dir_tables.is_empty() {
        return Err(FT_ERR_UNKNOWN_FILE_FORMAT);
    }
    Ok(())
}

pub fn tt_face_load_generic_header(face: &mut TtFace, tag: u32) -> FtResult<()> {
    tt_face_goto_table(face, tag)?;
    let mut f = face.stream.enter_frame(54)?;
    let h = &mut face.header;
    h.table_version = f.get_ulong();
    h.font_revision = f.get_ulong();
    h.check_sum_adjust = f.get_long();
    h.magic_number = f.get_long();
    h.flags = f.get_ushort();
    h.units_per_em = f.get_ushort();
    h.created = [f.get_ulong(), f.get_ulong()];
    h.modified = [f.get_ulong(), f.get_ulong()];
    h.x_min = f.get_short();
    h.y_min = f.get_short();
    h.x_max = f.get_short();
    h.y_max = f.get_short();
    h.mac_style = f.get_ushort();
    h.lowest_rec_ppem = f.get_ushort();
    h.font_direction = f.get_short();
    h.index_to_loc_format = f.get_short();
    h.glyph_data_format = f.get_short();
    Ok(())
}

pub fn tt_face_load_head(face: &mut TtFace) -> FtResult<()> {
    tt_face_load_generic_header(face, TTAG_HEAD)
}

pub fn tt_face_load_bhed(face: &mut TtFace) -> FtResult<()> {
    tt_face_load_generic_header(face, TTAG_BHED)
}

pub fn tt_face_load_maxp(face: &mut TtFace) -> FtResult<()> {
    tt_face_goto_table(face, TTAG_MAXP)?;
    {
        let mut f = face.stream.enter_frame(6)?;
        face.max_profile.version = f.get_long();
        face.max_profile.num_glyphs = f.get_ushort();
    }
    let m = &mut face.max_profile;
    m.max_points = 0;
    m.max_contours = 0;
    m.max_composite_points = 0;
    m.max_composite_contours = 0;
    m.max_zones = 0;
    m.max_twilight_points = 0;
    m.max_storage = 0;
    m.max_function_defs = 0;
    m.max_instruction_defs = 0;
    m.max_stack_elements = 0;
    m.max_size_of_instructions = 0;
    m.max_component_elements = 0;
    m.max_component_depth = 0;
    if face.max_profile.version >= 0x10000 {
        let mut f = face.stream.enter_frame(26)?;
        let m = &mut face.max_profile;
        m.max_points = f.get_ushort();
        m.max_contours = f.get_ushort();
        m.max_composite_points = f.get_ushort();
        m.max_composite_contours = f.get_ushort();
        m.max_zones = f.get_ushort();
        m.max_twilight_points = f.get_ushort();
        m.max_storage = f.get_ushort();
        m.max_function_defs = f.get_ushort();
        m.max_instruction_defs = f.get_ushort();
        m.max_stack_elements = f.get_ushort();
        m.max_size_of_instructions = f.get_ushort();
        m.max_component_elements = f.get_ushort();
        m.max_component_depth = f.get_ushort();
        if m.max_function_defs < 64 {
            m.max_function_defs = 64;
        }
        if m.max_twilight_points > 0xFFFF - 4 {
            m.max_twilight_points = 0xFFFF - 4;
        }
    }
    Ok(())
}

pub fn tt_face_load_name(face: &mut TtFace) -> FtResult<()> {
    let table_len = tt_face_goto_table(face, TTAG_NAME)?;
    let stream = &mut face.stream;
    let table_pos = stream.pos;
    let (format, num_name_records, storage_offset) = {
        let mut f = stream.enter_frame(6)?;
        (f.get_ushort(), f.get_ushort(), f.get_ushort())
    };
    face.name_format = format;
    let mut storage_start = table_pos + 6 + 12 * num_name_records as u32;
    let storage_limit = table_pos + table_len;
    if storage_start > storage_limit {
        return Err(FT_ERR_NAME_TABLE_MISSING);
    }
    let mut num_lang_tag_records: u16 = 0;
    if format == 1 {
        stream.seek(storage_start)?;
        num_lang_tag_records = stream.read_ushort()?;
        storage_start += 2 + 4 * num_lang_tag_records as u32;
        let mut f = stream.enter_frame(num_lang_tag_records as u32 * 4)?;
        let mut tags = Vec::with_capacity(num_lang_tag_records as usize);
        for _ in 0..num_lang_tag_records {
            let mut e = TtLangTagRec { string_length: f.get_ushort(), string_offset: f.get_ushort() as u32 };
            e.string_offset = e.string_offset.wrapping_add(table_pos + storage_offset as u32);
            if e.string_offset < storage_start || e.string_offset + e.string_length as u32 > storage_limit {
                e.string_length = 0;
            }
            tags.push(e);
        }
        face.lang_tags = tags;
        let _ = stream.seek(table_pos + 6);
    }
    let mut f = stream.enter_frame(num_name_records as u32 * 12)?;
    let mut names = Vec::new();
    for _ in 0..num_name_records {
        let mut e = TtNameRec {
            platform_id: f.get_ushort(),
            encoding_id: f.get_ushort(),
            language_id: f.get_ushort(),
            name_id: f.get_ushort(),
            string_length: f.get_ushort(),
            string_offset: f.get_ushort() as u32,
            string: None,
        };
        if e.string_length == 0 {
            continue;
        }
        e.string_offset = e.string_offset.wrapping_add(table_pos + storage_offset as u32);
        if e.string_offset < storage_start || e.string_offset + e.string_length as u32 > storage_limit {
            continue;
        }
        if format == 1 && e.language_id >= 0x8000 {
            let li = (e.language_id - 0x8000) as usize;
            if li >= num_lang_tag_records as usize || face.lang_tags[li].string_length == 0 {
                continue;
            }
        }
        names.push(e);
    }
    face.names = names;
    Ok(())
}

pub fn tt_face_load_cmap(face: &mut TtFace) -> FtResult<()> {
    let len = tt_face_goto_table(face, TTAG_CMAP)?;
    match face.stream.extract(len) {
        Ok(v) => face.cmap_table = v,
        Err(_) => face.cmap_table = Vec::new(),
    }
    Ok(())
}

pub fn tt_face_load_os2(face: &mut TtFace) -> FtResult<()> {
    tt_face_goto_table(face, TTAG_OS2)?;
    {
        let mut f = face.stream.enter_frame(78)?;
        let o = &mut face.os2;
        o.version = f.get_ushort();
        o.x_avg_char_width = f.get_short();
        o.us_weight_class = f.get_ushort();
        o.us_width_class = f.get_ushort();
        o.fs_type = f.get_short();
        o.y_subscript_x_size = f.get_short();
        o.y_subscript_y_size = f.get_short();
        o.y_subscript_x_offset = f.get_short();
        o.y_subscript_y_offset = f.get_short();
        o.y_superscript_x_size = f.get_short();
        o.y_superscript_y_size = f.get_short();
        o.y_superscript_x_offset = f.get_short();
        o.y_superscript_y_offset = f.get_short();
        o.y_strikeout_size = f.get_short();
        o.y_strikeout_position = f.get_short();
        o.s_family_class = f.get_short();
        for i in 0..10 {
            o.panose[i] = f.get_byte();
        }
        for i in 0..4 {
            o.ul_unicode_range[i] = f.get_ulong();
        }
        for i in 0..4 {
            o.ach_vend_id[i] = f.get_byte();
        }
        o.fs_selection = f.get_ushort();
        o.us_first_char_index = f.get_ushort();
        o.us_last_char_index = f.get_ushort();
        o.s_typo_ascender = f.get_short();
        o.s_typo_descender = f.get_short();
        o.s_typo_line_gap = f.get_short();
        o.us_win_ascent = f.get_ushort();
        o.us_win_descent = f.get_ushort();
    }
    let o = &mut face.os2;
    o.ul_code_page_range1 = 0;
    o.ul_code_page_range2 = 0;
    o.sx_height = 0;
    o.s_cap_height = 0;
    o.us_default_char = 0;
    o.us_break_char = 0;
    o.us_max_context = 0;
    o.us_lower_optical_point_size = 0;
    o.us_upper_optical_point_size = 0xFFFF;
    if face.os2.version >= 1 {
        {
            let mut f = face.stream.enter_frame(8)?;
            face.os2.ul_code_page_range1 = f.get_ulong();
            face.os2.ul_code_page_range2 = f.get_ulong();
        }
        if face.os2.version >= 2 {
            {
                let mut f = face.stream.enter_frame(10)?;
                let o = &mut face.os2;
                o.sx_height = f.get_short();
                o.s_cap_height = f.get_short();
                o.us_default_char = f.get_ushort();
                o.us_break_char = f.get_ushort();
                o.us_max_context = f.get_ushort();
            }
            if face.os2.version >= 5 {
                let mut f = face.stream.enter_frame(4)?;
                face.os2.us_lower_optical_point_size = f.get_ushort();
                face.os2.us_upper_optical_point_size = f.get_ushort();
            }
        }
    }
    Ok(())
}

pub fn tt_face_load_post(face: &mut TtFace) -> FtResult<()> {
    tt_face_goto_table(face, TTAG_POST)?;
    {
        let mut f = face.stream.enter_frame(32)?;
        let p = &mut face.postscript;
        p.format_type = f.get_long();
        p.italic_angle = f.get_long();
        p.underline_position = f.get_short();
        p.underline_thickness = f.get_short();
        p.is_fixed_pitch = f.get_ulong();
        p.min_mem_type42 = f.get_ulong();
        p.max_mem_type42 = f.get_ulong();
        p.min_mem_type1 = f.get_ulong();
        p.max_mem_type1 = f.get_ulong();
    }
    let ft = face.postscript.format_type;
    if ft != 0x0003_0000 && ft != 0x0002_5000 && ft != 0x0002_0000 && ft != 0x0001_0000 {
        return Err(FT_ERR_INVALID_POST_TABLE_FORMAT);
    }
    Ok(())
}
