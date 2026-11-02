//! Port of FreeType 2.14.2 `src/sfnt/ttmtx.c` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtResult;
use crate::asc::flex2::fonts::java2d::freetype::ttload::tt_face_goto_table;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

pub fn tt_face_load_hmtx(face: &mut TtFace, vertical: bool) -> FtResult<()> {
    let tag = if vertical { TTAG_VMTX } else { TTAG_HMTX };
    let table_size = tt_face_goto_table(face, tag)?;
    let pos = face.stream.pos;
    if vertical {
        face.vert_metrics_size = table_size;
        face.vert_metrics_offset = pos;
    } else {
        face.horz_metrics_size = table_size;
        face.horz_metrics_offset = pos;
    }
    Ok(())
}

pub fn tt_face_load_hhea(face: &mut TtFace, vertical: bool) -> FtResult<()> {
    tt_face_goto_table(face, if vertical { TTAG_VHEA } else { TTAG_HHEA })?;
    let mut f = face.stream.enter_frame(36)?;
    let h = if vertical { &mut face.vertical } else { &mut face.horizontal };
    h.version = f.get_ulong();
    h.ascender = f.get_short();
    h.descender = f.get_short();
    h.line_gap = f.get_short();
    h.advance_width_max = f.get_ushort();
    h.min_left_side_bearing = f.get_short();
    h.min_right_side_bearing = f.get_short();
    h.x_max_extent = f.get_short();
    h.caret_slope_rise = f.get_short();
    h.caret_slope_run = f.get_short();
    h.caret_offset = f.get_short();
    for i in 0..4 {
        h.reserved[i] = f.get_short();
    }
    h.metric_data_format = f.get_short();
    h.number_of_hmetrics = f.get_ushort();
    Ok(())
}

/// Returns `(bearing, advance)`; variation adjustments do not apply at the default instance.
pub fn tt_face_get_metrics(face: &mut TtFace, vertical: bool, gindex: u32) -> (i16, u16) {
    let (header, mut table_pos, table_size) = if vertical {
        (face.vertical, face.vert_metrics_offset, face.vert_metrics_size)
    } else {
        (face.horizontal, face.horz_metrics_offset, face.horz_metrics_size)
    };
    let table_end = table_pos.wrapping_add(table_size);
    let k = header.number_of_hmetrics as u32;
    let stream = &mut face.stream;
    if k > 0 {
        if gindex < k {
            table_pos = table_pos.wrapping_add(4 * gindex);
            if table_pos.wrapping_add(4) > table_end {
                return (0, 0);
            }
            let r = (|| -> FtResult<(i16, u16)> {
                stream.seek(table_pos)?;
                let a = stream.read_ushort()?;
                let b = stream.read_short()?;
                Ok((b, a))
            })();
            r.unwrap_or((0, 0))
        } else {
            table_pos = table_pos.wrapping_add(4 * (k - 1));
            if table_pos.wrapping_add(2) > table_end {
                return (0, 0);
            }
            let aadvance = match stream.seek(table_pos).and_then(|_| stream.read_ushort()) {
                Ok(a) => a,
                Err(_) => return (0, 0),
            };
            table_pos = table_pos.wrapping_add(4).wrapping_add(2u32.wrapping_mul(gindex - k));
            let abearing = if table_pos.wrapping_add(2) > table_end {
                0
            } else if stream.seek(table_pos).is_err() {
                0
            } else {
                stream.read_short().unwrap_or(0)
            };
            (abearing, aadvance)
        }
    } else {
        (0, 0)
    }
}
