//! Port of JDK 17 `sun.font.CMap` (the formats and lookups `TrueTypeGlyphMapper` uses).

use crate::asc::flex2::fonts::java2d::java::nio::byte_buffer::{ByteBuffer, JResult};
use crate::asc::flex2::fonts::java2d::sun::font::char_to_glyph_mapper::INVISIBLE_GLYPH_ID;

const INTMASK: i64 = 0x7fffffff;

pub enum CMap {
    Format0 { cmap: Vec<u8> },
    Format2 { sub_header_key: Vec<u16>, first_code_array: Vec<u16>, entry_count_array: Vec<u16>, id_delta_array: Vec<i16>, id_range_offset_array: Vec<u16>, glyph_index_array: Vec<u16> },
    Format4 { seg_count: i32, end_count: Vec<u16>, start_count: Vec<u16>, id_delta: Vec<i16>, id_range_offset: Vec<u16>, glyph_ids: Vec<u16> },
    Format6 { first_code: u16, entry_count: u16, glyph_id_array: Vec<u16> },
    Format8,
    Format10 { first_code: i64, entry_count: i32, glyph_id_array: Vec<u16> },
    Format12 { power: i32, extra: i32, start_char_code: Vec<i64>, end_char_code: Vec<i64>, start_glyph_id: Vec<i32> },
    Null,
}

/// What `CMap.initialize` threw; `Unsupported` marks the converter-map encodings (3,2..6) this port lacks.
#[derive(Debug, PartialEq)]
pub enum CMapError {
    Runtime,
    Unsupported,
}

impl From<()> for CMapError {
    fn from(_: ()) -> CMapError {
        CMapError::Runtime
    }
}

impl CMap {
    pub fn initialize(cmap_buffer: Option<ByteBuffer>) -> Result<CMap, CMapError> {
        let mut cmap_buffer = cmap_buffer.ok_or(CMapError::Runtime)?;
        let (mut three0, mut three1, mut three2, mut three3, mut three4, mut three5, mut three6, mut three10) = (0, 0, 0, 0, 0, 0, 0, 0);
        let mut zero5 = 0;
        let mut three_star = false;
        let number_sub_tables = cmap_buffer.get_short_at(2)?;
        for i in 0..number_sub_tables as i32 {
            cmap_buffer.set_position(i as i64 * 8 + 4)?;
            let platform_id = cmap_buffer.get_short()?;
            if platform_id == 3 {
                three_star = true;
                let encoding_id = cmap_buffer.get_short()?;
                let offset = cmap_buffer.get_int()?;
                match encoding_id {
                    0 => three0 = offset,
                    1 => three1 = offset,
                    2 => three2 = offset,
                    3 => three3 = offset,
                    4 => three4 = offset,
                    5 => three5 = offset,
                    6 => three6 = offset,
                    10 => three10 = offset,
                    _ => {}
                }
            } else if platform_id == 0 {
                let encoding_id = cmap_buffer.get_short()?;
                let offset = cmap_buffer.get_int()?;
                if encoding_id == 5 {
                    zero5 = offset;
                }
            }
        }
        let cmap = if three_star {
            if three10 != 0 {
                Some(Self::create_cmap(&mut cmap_buffer, three10)?)
            } else if three0 != 0 {
                Some(Self::create_cmap(&mut cmap_buffer, three0)?)
            } else if three1 != 0 {
                Some(Self::create_cmap(&mut cmap_buffer, three1)?)
            } else if three2 != 0 || three3 != 0 || three4 != 0 || three5 != 0 || three6 != 0 {
                return Err(CMapError::Unsupported);
            } else {
                None
            }
        } else {
            let o = cmap_buffer.get_int_at(8)?;
            Some(Self::create_cmap(&mut cmap_buffer, o)?)
        };
        let Some(cmap) = cmap else { return Err(CMapError::Runtime) };
        if zero5 != 0 {
            Self::create_uvs(&cmap_buffer, zero5)?;
        }
        Ok(cmap)
    }

    fn create_cmap(buffer: &mut ByteBuffer, offset: i32) -> JResult<CMap> {
        let offset = offset as i64;
        let subtable_format = buffer.get_char_at(offset)?;
        if subtable_format < 8 {
            buffer.get_char_at(offset + 2)?;
        } else {
            buffer.get_int_at(offset + 4)?;
        }
        match subtable_format {
            0 => Self::format0(buffer, offset),
            2 => Self::format2(buffer, offset),
            4 => Self::format4(buffer, offset),
            6 => Self::format6(buffer, offset),
            8 => Self::format8(buffer),
            10 => Self::format10(buffer, offset),
            12 => Self::format12(buffer, offset),
            _ => Err(()),
        }
    }

    /// `createUVS`: only the reads outside its `try` can fail the cmap.
    fn create_uvs(buffer: &ByteBuffer, offset: i32) -> JResult<()> {
        let subtable_format = buffer.get_char_at(offset as i64)?;
        if subtable_format == 14 {
            buffer.get_int_at(offset as i64 + 2)?;
        }
        Ok(())
    }

    fn format0(buffer: &mut ByteBuffer, offset: i64) -> JResult<CMap> {
        let len = buffer.get_char_at(offset + 2)? as i32;
        if len - 6 < 0 {
            return Err(());
        }
        buffer.set_position(offset + 6)?;
        let n = (len - 6) as usize;
        let cmap = buffer.get_bytes(n, n)?;
        Ok(CMap::Format0 { cmap })
    }

    fn format2(bb: &mut ByteBuffer, offset: i64) -> JResult<CMap> {
        let table_len = bb.get_char_at(offset + 2)? as i32;
        bb.set_position(offset + 6)?;
        let mut c = bb.view(2);
        let mut sub_header_key = vec![0u16; 256];
        let mut max_sub_header = 0u16;
        for k in sub_header_key.iter_mut() {
            *k = c.get_char()?;
            if *k > max_sub_header {
                max_sub_header = *k;
            }
        }
        let num_sub_headers = ((max_sub_header >> 3) + 1) as usize;
        let (mut first_code_array, mut entry_count_array, mut id_delta_array, mut id_range_offset_array) = (vec![0u16; num_sub_headers], vec![0u16; num_sub_headers], vec![0i16; num_sub_headers], vec![0u16; num_sub_headers]);
        for i in 0..num_sub_headers {
            first_code_array[i] = c.get_char()?;
            entry_count_array[i] = c.get_char()?;
            id_delta_array[i] = c.get_char()? as i16;
            id_range_offset_array[i] = c.get_char()?;
        }
        let glyph_index_arr_size = (table_len - 518 - num_sub_headers as i32 * 8) / 2;
        if glyph_index_arr_size < 0 {
            return Err(());
        }
        let mut glyph_index_array = vec![0u16; glyph_index_arr_size as usize];
        for g in glyph_index_array.iter_mut() {
            *g = c.get_char()?;
        }
        Ok(CMap::Format2 { sub_header_key, first_code_array, entry_count_array, id_delta_array, id_range_offset_array, glyph_index_array })
    }

    fn format4(bb: &mut ByteBuffer, offset: i64) -> JResult<CMap> {
        bb.set_position(offset)?;
        let mut buffer = bb.view(2);
        buffer.get_char()?;
        let mut subtable_length = buffer.get_char()? as i64;
        if offset + subtable_length > bb.capacity() as i64 {
            subtable_length = bb.capacity() as i64 - offset;
        }
        buffer.get_char()?;
        let seg_count = (buffer.get_char()? / 2) as i32;
        buffer.get_char()?;
        buffer.get_char()?;
        buffer.get_char()?;
        let n = seg_count as usize;
        let (mut start_count, mut end_count, mut id_delta, mut id_range_offset) = (vec![0u16; n], vec![0u16; n], vec![0i16; n], vec![0u16; n]);
        for e in end_count.iter_mut() {
            *e = buffer.get_char()?;
        }
        buffer.get_char()?;
        for s in start_count.iter_mut() {
            *s = buffer.get_char()?;
        }
        for d in id_delta.iter_mut() {
            *d = buffer.get_char()? as i16;
        }
        for r in id_range_offset.iter_mut() {
            let ctmp = buffer.get_char()?;
            *r = (ctmp >> 1) & 0xffff;
        }
        let pos = (seg_count * 8 + 16) / 2;
        buffer.set_position(pos as i64 * 2)?;
        let num_glyph_ids = subtable_length / 2 - pos as i64;
        if num_glyph_ids < 0 {
            return Err(());
        }
        let mut glyph_ids = vec![0u16; num_glyph_ids as usize];
        for g in glyph_ids.iter_mut() {
            *g = buffer.get_char()?;
        }
        Ok(CMap::Format4 { seg_count, end_count, start_count, id_delta, id_range_offset, glyph_ids })
    }

    fn format6(bb: &mut ByteBuffer, offset: i64) -> JResult<CMap> {
        bb.set_position(offset + 6)?;
        let mut buffer = bb.view(2);
        let first_code = buffer.get_char()?;
        let entry_count = buffer.get_char()?;
        let mut glyph_id_array = vec![0u16; entry_count as usize];
        for g in glyph_id_array.iter_mut() {
            *g = buffer.get_char()?;
        }
        Ok(CMap::Format6 { first_code, entry_count, glyph_id_array })
    }

    fn format8(bb: &mut ByteBuffer) -> JResult<CMap> {
        bb.set_position(12)?;
        bb.get_bytes(8192, 8192)?;
        let n_groups = bb.get_int()? as i64 & INTMASK;
        if (bb.remaining() as i64) < 12 * n_groups {
            return Err(());
        }
        Ok(CMap::Format8)
    }

    fn format10(bb: &mut ByteBuffer, offset: i64) -> JResult<CMap> {
        bb.set_position(offset + 12)?;
        let first_code = bb.get_int()? as i64 & INTMASK;
        let entry_count = (bb.get_int()? as i64 & INTMASK) as i32;
        if (bb.remaining() as i64) < 2 * entry_count as i64 {
            return Err(());
        }
        let mut buffer = bb.view(2);
        let mut glyph_id_array = vec![0u16; entry_count as usize];
        for g in glyph_id_array.iter_mut() {
            *g = buffer.get_char()?;
        }
        Ok(CMap::Format10 { first_code, entry_count, glyph_id_array })
    }

    fn format12(bb: &mut ByteBuffer, offset: i64) -> JResult<CMap> {
        bb.set_position(offset + 12)?;
        let num_groups = (bb.get_int()? as i64 & INTMASK) as i32;
        if (bb.remaining() as i64) < 12 * num_groups as i64 {
            return Err(());
        }
        let n = num_groups as usize;
        let (mut start_char_code, mut end_char_code, mut start_glyph_id) = (vec![0i64; n], vec![0i64; n], vec![0i32; n]);
        let mut ib = bb.slice().view(4);
        for i in 0..n {
            start_char_code[i] = ib.get_int()? as i64 & INTMASK;
            end_char_code[i] = ib.get_int()? as i64 & INTMASK;
            start_glyph_id[i] = (ib.get_int()? as i64 & INTMASK) as i32;
        }
        let mut value = num_groups;
        let mut high_bit = 0;
        for s in [16, 8, 4, 2, 1] {
            if value >= 1 << s {
                value >>= s;
                high_bit += s;
            }
        }
        let power = 1 << high_bit;
        let extra = num_groups - power;
        Ok(CMap::Format12 { power, extra, start_char_code, end_char_code, start_glyph_id })
    }

    /// `getGlyph(charCode)`; `Err` is the exception `TrueTypeGlyphMapper` turns into `handleBadCMAP`.
    pub fn get_glyph(&self, char_code: i32) -> JResult<u16> {
        match self {
            CMap::Format0 { cmap } => {
                if char_code < 256 {
                    if char_code < 0x0010 {
                        if let 0x09 | 0x0a | 0x0d = char_code {
                            return Ok(INVISIBLE_GLYPH_ID as u16);
                        }
                    }
                    cmap.get(char_code as usize).map(|&b| b as u16).ok_or(())
                } else {
                    Ok(0)
                }
            }
            CMap::Format2 { sub_header_key, first_code_array, entry_count_array, id_delta_array, id_range_offset_array, glyph_index_array } => {
                let orig_char_code = char_code;
                let control_glyph = get_control_code_glyph(char_code, true);
                if control_glyph >= 0 {
                    return Ok(control_glyph as u16);
                }
                let high_byte = (char_code >> 8) as u16;
                let low_byte = (char_code & 0xff) as u16;
                let key = (*sub_header_key.get(high_byte as usize).ok_or(())? >> 3) as usize;
                let mut map_me;
                if key != 0 {
                    map_me = low_byte;
                } else {
                    map_me = high_byte;
                    if map_me == 0 {
                        map_me = low_byte;
                    }
                }
                let first_code = *first_code_array.get(key).ok_or(())?;
                if map_me < first_code {
                    return Ok(0);
                }
                map_me -= first_code;
                if map_me < entry_count_array[key] {
                    let glyph_array_offset = ((id_range_offset_array.len() as i32 - key as i32) * 8) - 6;
                    let glyph_sub_array_start = (id_range_offset_array[key] as i32 - glyph_array_offset) / 2;
                    let idx = glyph_sub_array_start + map_me as i32;
                    if idx < 0 {
                        return Err(());
                    }
                    let mut glyph_code = *glyph_index_array.get(idx as usize).ok_or(())?;
                    if glyph_code != 0 {
                        glyph_code = glyph_code.wrapping_add(id_delta_array[key] as u16);
                        return Ok(glyph_code);
                    }
                }
                Ok(get_format_char_glyph(orig_char_code))
            }
            CMap::Format4 { end_count, start_count, id_delta, id_range_offset, glyph_ids, seg_count } => {
                let orig_char_code = char_code;
                let mut glyph_code: u16 = 0;
                let control_glyph = get_control_code_glyph(char_code, true);
                if control_glyph >= 0 {
                    return Ok(control_glyph as u16);
                }
                let (mut left, mut right) = (0usize, start_count.len());
                let mut index = start_count.len() >> 1;
                while left < right {
                    if (end_count[index] as i32) < char_code {
                        left = index + 1;
                    } else {
                        right = index;
                    }
                    index = (left + right) >> 1;
                }
                let sc = *start_count.get(index).ok_or(())? as i32;
                if char_code >= sc && char_code <= *end_count.get(index).ok_or(())? as i32 {
                    let range_offset = id_range_offset[index] as i32;
                    if range_offset == 0 {
                        glyph_code = (char_code + id_delta[index] as i32) as u16;
                    } else {
                        let glyph_id_index = range_offset - seg_count + index as i32 + (char_code - sc);
                        if glyph_id_index < 0 {
                            return Err(());
                        }
                        glyph_code = *glyph_ids.get(glyph_id_index as usize).ok_or(())?;
                        if glyph_code != 0 {
                            glyph_code = (glyph_code as i32 + id_delta[index] as i32) as u16;
                        }
                    }
                }
                if glyph_code == 0 {
                    glyph_code = get_format_char_glyph(orig_char_code);
                }
                Ok(glyph_code)
            }
            CMap::Format6 { first_code, entry_count, glyph_id_array } => {
                let orig_char_code = char_code;
                let control_glyph = get_control_code_glyph(char_code, true);
                if control_glyph >= 0 {
                    return Ok(control_glyph as u16);
                }
                let c = char_code - *first_code as i32;
                if c < 0 || c >= *entry_count as i32 {
                    Ok(get_format_char_glyph(orig_char_code))
                } else {
                    Ok(glyph_id_array[c as usize])
                }
            }
            CMap::Format8 | CMap::Null => Ok(0),
            CMap::Format10 { first_code, entry_count, glyph_id_array } => {
                let code = (char_code as i64 - first_code) as i32;
                if code < 0 || code >= *entry_count {
                    Ok(0)
                } else {
                    Ok(glyph_id_array[code as usize])
                }
            }
            CMap::Format12 { power, extra, start_char_code, end_char_code, start_glyph_id } => {
                let orig_char_code = char_code;
                let control_glyph = get_control_code_glyph(char_code, false);
                if control_glyph >= 0 {
                    return Ok(control_glyph as u16);
                }
                let cc = char_code as i64;
                let sc = |i: i32| -> JResult<i64> { if i < 0 { Err(()) } else { start_char_code.get(i as usize).copied().ok_or(()) } };
                let mut probe = *power;
                let mut range = 0;
                if sc(*extra)? <= cc {
                    range = *extra;
                }
                while probe > 1 {
                    probe >>= 1;
                    if sc(range + probe)? <= cc {
                        range += probe;
                    }
                }
                if sc(range)? <= cc && end_char_code[range as usize] >= cc {
                    return Ok((start_glyph_id[range as usize] as i64 + (cc - start_char_code[range as usize])) as u16);
                }
                Ok(get_format_char_glyph(orig_char_code))
            }
        }
    }
}

fn get_control_code_glyph(char_code: i32, no_surrogates: bool) -> i32 {
    if char_code < 0x0010 {
        if let 0x09 | 0x0a | 0x0d = char_code {
            return INVISIBLE_GLYPH_ID;
        }
    } else if no_surrogates && char_code >= 0xFFFF {
        return 0;
    }
    -1
}

fn get_format_char_glyph(char_code: i32) -> u16 {
    if char_code >= 0x200c && (char_code <= 0x200f || (0x2028..=0x202e).contains(&char_code) || (0x206a..=0x206f).contains(&char_code)) {
        return INVISIBLE_GLYPH_ID as u16;
    }
    0
}
