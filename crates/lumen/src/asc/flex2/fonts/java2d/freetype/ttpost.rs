//! Port of FreeType 2.14.2 `src/sfnt/ttpost.c` (as built into JDK 17.0.19), reduced to what `tt_check_single_notdef` asks: Macintosh standard names other than `.notdef` are returned as an empty placeholder.

use crate::asc::flex2::fonts::java2d::freetype::ftobjs::{FtFace, FT_FACE_FLAG_GLYPH_NAMES};
use crate::asc::flex2::fonts::java2d::freetype::ftstream::FtResult;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::ttload::tt_face_goto_table;
use crate::asc::flex2::fonts::java2d::freetype::tttypes::*;

fn mac_name(i: u32) -> Vec<u8> {
    if i == 0 {
        b".notdef".to_vec()
    } else {
        Vec::new()
    }
}

/// `(glyph name indices, names)` of a format 2.0 table, or the per-glyph offsets of 2.5 as indices.
fn load_post_names(face: &mut FtFace) -> FtResult<(Vec<u16>, Vec<Vec<u8>>)> {
    let tt = &mut face.tt;
    let mut post_len = tt_face_goto_table(tt, TTAG_POST)?;
    let format = tt.stream.read_long()?;
    tt.stream.skip(28)?;
    if format == 0x0002_0000 {
        post_len = post_len.wrapping_sub(32);
        let num_glyphs = tt.stream.read_ushort()?;
        post_len = post_len.wrapping_sub(2);
        if num_glyphs as u32 > tt.max_profile.num_glyphs as u32 || num_glyphs as u32 * 2 > post_len {
            return Err(FT_ERR_INVALID_FILE_FORMAT);
        }
        let mut f = tt.stream.enter_frame(num_glyphs as u32 * 2)?;
        let mut num_names: u16 = 0;
        let mut idx = Vec::with_capacity(num_glyphs as usize);
        for _ in 0..num_glyphs {
            let i = f.get_ushort();
            if i > num_names {
                num_names = i;
            }
            idx.push(i);
        }
        let num_names = if num_names > 257 { num_names - 257 } else { 0 };
        let mut names = Vec::new();
        if num_names != 0 {
            let rest = post_len - num_glyphs as u32 * 2;
            let p = tt.stream.extract(rest)?;
            let mut at = 0usize;
            while at < p.len() && names.len() < num_names as usize {
                let len = p[at] as usize;
                at += 1;
                let end = (at + len).min(p.len());
                names.push(p[at..end].to_vec());
                at += len;
            }
            while names.len() < num_names as usize {
                names.push(Vec::new());
            }
        }
        Ok((idx, names))
    } else {
        let num_glyphs = tt.stream.read_ushort()?;
        if num_glyphs as u32 > tt.max_profile.num_glyphs as u32 || num_glyphs > 258 + 128 {
            return Err(FT_ERR_INVALID_FILE_FORMAT);
        }
        let offsets = tt.stream.extract(num_glyphs as u32)?;
        let mut idx = Vec::new();
        for (n, &o) in offsets.iter().enumerate() {
            let v = n as i32 + o as i8 as i32;
            if !(0..258).contains(&v) {
                return Err(FT_ERR_INVALID_FILE_FORMAT);
            }
            idx.push(v as u16);
        }
        Ok((idx, Vec::new()))
    }
}

pub fn tt_face_get_ps_name(face: &mut FtFace, idx: u32) -> Option<Vec<u8>> {
    if face.face_flags & FT_FACE_FLAG_GLYPH_NAMES == 0 {
        return None;
    }
    if idx >= face.tt.max_profile.num_glyphs as u32 {
        return None;
    }
    let format = face.tt.postscript.format_type;
    if format == 0x0002_0000 || format == 0x0002_5000 {
        if let Ok((ind, names)) = load_post_names(face) {
            if (idx as usize) < ind.len() {
                let ni = ind[idx as usize] as u32;
                if ni < 258 {
                    return Some(mac_name(ni));
                }
                return Some(names.get((ni - 258) as usize).cloned().unwrap_or_default());
            }
        }
        return Some(mac_name(0));
    } else if format == 0x0001_0000 && face.tt.max_profile.num_glyphs == 258 {
        return Some(mac_name(idx));
    }
    Some(mac_name(0))
}
