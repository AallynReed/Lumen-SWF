//! Port of afe.jar `com.adobe.fontengine.font.opentype.Cmap` (all but the PDF `subsetAndStream(SubsetSimpleTrueType)`).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::asc::flex2::fonts::adobe::fontengine::charset_util::CharsetUtil;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

pub const TAG_CMAP: i32 = 1668112752;

fn aioobe(i: i32, len: usize) -> FontEngineException {
    FontEngineException::new(ExceptionKind::FontEngine, format!("java.lang.ArrayIndexOutOfBoundsException: Index {} out of bounds for length {}", i, len))
}

fn put(a: &mut [i32], i: i32, v: i32) -> FontResult<()> {
    if i < 0 || i as usize >= a.len() {
        return Err(aioobe(i, a.len()));
    }
    a[i as usize] = v;
    Ok(())
}

/// `Cmap.CmapSelector`.
pub trait CmapSelector {
    fn cmap_found(&mut self, cmap: &Cmap, platform: i32, encoding: i32, index: i32) -> FontResult<()>;
}

/// `Cmap.Format14Consumer`.
pub trait Format14Consumer {
    fn default_uv(&mut self, vs: i32, uv: i32) -> bool;
    fn non_default_uv(&mut self, vs: i32, uv: i32, gid: i32) -> FontResult<bool>;
}

/// `Cmap.NonUnicodeCmap`.
#[derive(Clone, Debug)]
pub struct NonUnicodeCmap {
    pub subtable_offset: i32,
    pub charset_name: &'static str,
}

/// `Cmap.SegmentInfo`.
#[derive(Clone, Debug)]
struct SegmentInfo {
    can_use_id_delta: bool,
    id_delta: i32,
    start_code: i32,
    end_code: i32,
}

/// `SWFCmapSelector.CmapInfo`.
#[derive(Clone, Debug)]
struct CmapInfo {
    platform_id: i32,
    platform_encoding: i32,
    offset: i32,
}

/// `Cmap.SWFCmapSelector`.
#[derive(Default)]
struct SWFCmapSelector {
    found_required_cmap: bool,
    cmaps_to_keep: Vec<CmapInfo>,
}

impl SWFCmapSelector {
    fn already_one_with_this_offset(&self, n: i32) -> bool {
        self.cmaps_to_keep.iter().any(|c| c.offset == n)
    }
}

impl CmapSelector for SWFCmapSelector {
    fn cmap_found(&mut self, cmap: &Cmap, n: i32, n2: i32, n3: i32) -> FontResult<()> {
        let n4 = cmap.get_offset(n3)?;
        match (n, n2) {
            (0, 3) | (0, 4) | (3, 0) | (3, 1) | (3, 10) => {
                if !self.already_one_with_this_offset(n4) {
                    self.found_required_cmap = true;
                    self.cmaps_to_keep.push(CmapInfo { platform_id: n, platform_encoding: n2, offset: n4 });
                }
            }
            (0, 5) => self.cmaps_to_keep.push(CmapInfo { platform_id: n, platform_encoding: n2, offset: n4 }),
            _ => {}
        }
        Ok(())
    }
}

/// `Cmap.Format14Harvester`; `variation_uvs` is a Java `HashSet<Integer>`, iterated in its order.
struct Format14Harvester<'a> {
    harvested_cps: &'a BTreeSet<i32>,
    harvested_gids: &'a mut dyn Subset,
    variation_uvs: crate::asc::util::jhash::JHashSet<i32>,
    variation_sequences_found: bool,
}

impl<'a> Format14Consumer for Format14Harvester<'a> {
    fn default_uv(&mut self, n: i32, n2: i32) -> bool {
        if self.harvested_cps.contains(&n2) {
            self.variation_uvs.insert(n);
            self.variation_sequences_found = true;
            return false;
        }
        true
    }

    fn non_default_uv(&mut self, n: i32, n2: i32, n3: i32) -> FontResult<bool> {
        if self.harvested_cps.contains(&n2) {
            if !self.variation_uvs.contains(&n) {
                self.variation_uvs.insert(n);
                self.variation_sequences_found = true;
            }
            self.harvested_gids.get_subset_gid(n3)?;
        }
        Ok(true)
    }
}

/// `Cmap.Format14Writer`; `vsToUnicode` is a `LinkedHashMap`.
struct Format14Writer<'a> {
    subset: &'a dyn Subset,
    codepoints: &'a BTreeSet<i32>,
    builder: &'a mut OTByteArrayBuilder,
    new_st_offset: i32,
    last_vs: i32,
    vs_to_unicode: Vec<(i32, Vec<(i32, i32)>)>,
    st_size: i32,
}

impl<'a> Format14Writer<'a> {
    fn add(&mut self, n: i32, uv: i32, gid: i32) {
        if self.last_vs != n {
            self.last_vs = n;
        }
        let vs = self.last_vs;
        match self.vs_to_unicode.iter_mut().find(|(k, _)| *k == vs) {
            Some((_, l)) => l.push((uv, gid)),
            None => self.vs_to_unicode.push((vs, vec![(uv, gid)])),
        }
    }

    fn write_subtable(&mut self) -> FontResult<()> {
        self.st_size = 10 + 11 * self.vs_to_unicode.len() as i32;
        self.builder.ensure_capacity(self.new_st_offset + self.st_size);
        self.builder.setuint16(self.new_st_offset, 14);
        self.builder.setuint32(self.new_st_offset + 6, self.vs_to_unicode.len() as i32);
        let entries = self.vs_to_unicode.clone();
        for (n, (vs, list)) in entries.iter().enumerate() {
            let n = n as i32;
            let mut n3 = self.write_default_uvs_table(self.new_st_offset + self.st_size, list);
            self.builder.setuint24(self.new_st_offset + 10 + 11 * n, *vs);
            self.builder.setuint32(self.new_st_offset + 10 + 11 * n + 3, if n3 == 0 { 0 } else { self.st_size });
            self.st_size += n3;
            n3 = self.write_non_default_uvs_table(self.new_st_offset + self.st_size, list)?;
            self.builder.setuint32(self.new_st_offset + 10 + 11 * n + 7, if n3 == 0 { 0 } else { self.st_size });
            self.st_size += n3;
        }
        self.builder.setuint32(self.new_st_offset + 2, self.st_size);
        Ok(())
    }

    fn write_default_uvs_table(&mut self, n: i32, list: &[(i32, i32)]) -> i32 {
        let n2 = -1;
        let mut n3 = 0;
        let mut n4 = 0;
        for &(uv, gid) in list {
            if gid != -1 {
                break;
            }
            if n2 + 1 != uv {
                self.builder.ensure_capacity(n + 4 + 4 * (n3 + 1));
                if n3 > 0 {
                    self.builder.setuint8(n + 4 + 4 * (n3 - 1) + 3, n4);
                }
                self.builder.setuint24(n + 4 + 4 * n3, uv);
                n4 = 0;
                n3 += 1;
                continue;
            }
            n4 += 1;
        }
        if n3 > 0 {
            self.builder.setuint8(n + 4 + 4 * (n3 - 1) + 3, n4);
            self.builder.setuint32(n, n3);
            return 4 + 4 * n3;
        }
        0
    }

    fn write_non_default_uvs_table(&mut self, n: i32, list: &[(i32, i32)]) -> FontResult<i32> {
        let mut n2 = 0;
        for &(uv, gid) in list {
            if gid == -1 {
                continue;
            }
            self.builder.ensure_capacity(n + 4 + 5 * (n2 + 1));
            self.builder.setuint24(n + 4 + 5 * n2, uv);
            self.builder.setuint16(n + 4 + 5 * n2 + 3, self.subset.get_existing_subset_gid(gid)?);
            n2 += 1;
        }
        if n2 > 0 {
            self.builder.setuint32(n, n2);
            return Ok(4 + 5 * n2);
        }
        Ok(0)
    }
}

impl<'a> Format14Consumer for Format14Writer<'a> {
    fn default_uv(&mut self, n: i32, n2: i32) -> bool {
        if self.codepoints.contains(&n2) {
            self.add(n, n2, -1);
        }
        true
    }

    fn non_default_uv(&mut self, n: i32, n2: i32, n3: i32) -> FontResult<bool> {
        if self.codepoints.contains(&n2) {
            self.add(n, n2, n3);
        }
        Ok(true)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cmap {
    pub data: OTByteArray,
    pub unicode_subtable_offset: i32,
    pub old_unicode_subtable_offset: i32,
    pub non_unicode_subtable_indices: Vec<NonUnicodeCmap>,
    pub symbol_subtable_offset: i32,
    pub padding_byte: i32,
}

impl Cmap {
    pub fn new(a: FontByteArray) -> FontResult<Cmap> {
        let mut c = Cmap { data: OTByteArray::from_font_byte_array(a), ..Default::default() };
        let mut n = c.probe(3, 10)?;
        c.unicode_subtable_offset = if n != -1 {
            n
        } else {
            n = c.probe(0, 4)?;
            if n != -1 {
                n
            } else {
                n = c.probe(3, 1)?;
                if n != -1 {
                    n
                } else {
                    c.probe(0, 3)?
                }
            }
        };
        n = c.probe(0, 2)?;
        c.old_unicode_subtable_offset = if n != -1 {
            n
        } else {
            n = c.probe(0, 1)?;
            if n != -1 {
                n
            } else {
                c.probe(0, 0)?
            }
        };
        n = c.probe(3, 0)?;
        if n != -1 {
            c.symbol_subtable_offset = n;
            c.padding_byte = c.compute_symbol_pad(n)?;
        } else {
            c.symbol_subtable_offset = -1;
        }
        let candidates: [(i32, i32, &'static str); 16] = [
            (3, 2, "windows-932"),
            (3, 3, "windows-936"),
            (3, 4, "windows-950"),
            (3, 5, "windows-949"),
            (3, 6, "ms1361"),
            (1, 0, "MacRoman"),
            (1, 1, "MacJapanese"),
            (1, 2, "MacChineseTraditional"),
            (1, 3, "MacKorean"),
            (1, 4, "MacArabic"),
            (1, 5, "MacHebrew"),
            (1, 6, "MacGreek"),
            (1, 7, "MacCyrillic"),
            (1, 8, "MacSymbol"),
            (1, 21, "MacThai"),
            (1, 25, "MacChineseSimplified"),
        ];
        for (p, e, name) in candidates {
            let n = c.probe(p, e)?;
            if n != -1 {
                c.non_unicode_subtable_indices.push(NonUnicodeCmap { subtable_offset: n, charset_name: name });
            }
        }
        Ok(c)
    }

    fn compute_symbol_pad(&self, n: i32) -> FontResult<i32> {
        for pad in [61440, 61696, 61952, 0] {
            if self.get_mapping(n, 97 + pad)? != 0 {
                return Ok(pad);
            }
        }
        let n2 = self.get_lowest_mapped_code(n)?;
        if (61440..62208).contains(&n2) {
            return Ok(n2 & 0xFF00);
        }
        Ok(0)
    }

    pub fn probe(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint16(2)?;
        for i in 0..n3 {
            let n4 = self.data.getuint16(4 + 8 * i)?;
            let n5 = self.data.getuint16(4 + 8 * i + 2)?;
            if n4 != n || n5 != n2 {
                continue;
            }
            return self.data.getuint32asint(4 + 8 * i + 4, "Offset to cmap subtable is big");
        }
        Ok(-1)
    }

    pub fn is_symbolic(&self) -> bool {
        self.symbol_subtable_offset != -1
    }

    pub fn enumerate_cmaps(&self, sel: &mut dyn CmapSelector) -> FontResult<()> {
        let n = self.data.getuint16(2)?;
        for i in 0..n {
            let n2 = self.data.getuint16(4 + 8 * i)?;
            let n3 = self.data.getuint16(4 + 8 * i + 2)?;
            let wanted = match n2 {
                1 => matches!(n3, 0..=8 | 21 | 25),
                3 => matches!(n3, 0..=6 | 10),
                0 => matches!(n3, 0..=5),
                _ => false,
            };
            if wanted {
                sel.cmap_found(self, n2, n3, i)?;
            }
        }
        Ok(())
    }

    pub fn get_cmap_subtable_index(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint16(2)?;
        for i in 0..n3 {
            if self.data.getuint16(4 + 8 * i)? == n && self.data.getuint16(4 + 8 * i + 2)? == n2 {
                return Ok(i);
            }
        }
        Ok(-1)
    }

    pub fn remove_symbol_modifier(&self, n: i32) -> i32 {
        if self.symbol_subtable_offset != -1 && n & 0xFF00 == self.padding_byte {
            return n & 0xFF;
        }
        n
    }

    pub fn unicode_char2glyph(&self, n: i32) -> FontResult<i32> {
        if self.symbol_subtable_offset != -1 {
            let mut n2 = self.get_mapping(self.symbol_subtable_offset, n)?;
            if n2 == 0 && (0..=255).contains(&n) {
                n2 = self.get_mapping(self.symbol_subtable_offset, self.padding_byte | n)?;
            }
            return Ok(n2);
        }
        if self.unicode_subtable_offset != -1 {
            if n == 65535 {
                return Ok(0);
            }
            return self.get_mapping(self.unicode_subtable_offset, n);
        }
        self.get_non_unicode_mapping(n)
    }

    pub fn cool_type_unicode_char2glyph(&self, n: i32) -> FontResult<i32> {
        if self.unicode_subtable_offset != -1 {
            return self.get_mapping(self.unicode_subtable_offset, n);
        }
        if self.old_unicode_subtable_offset != -1 {
            return self.get_mapping(self.old_unicode_subtable_offset, n);
        }
        if self.symbol_subtable_offset != -1 {
            let mut n2 = self.get_mapping(self.symbol_subtable_offset, n)?;
            if n2 == 0 && (0..=255).contains(&n) {
                n2 = self.get_mapping(self.symbol_subtable_offset, self.padding_byte | n)?;
            }
            return Ok(n2);
        }
        self.get_non_unicode_mapping(n)
    }

    /// Java shares one char buffer and one byte buffer across the subtables, so the first charset that can encode the
    /// character decides; the encoders are the inverse of the ICU decode tables.
    pub fn get_non_unicode_mapping(&self, n: i32) -> FontResult<i32> {
        let units: Vec<u16> = match char::from_u32(n as u32) {
            Some(c) if n > 0xFFFF => {
                let mut b = [0u16; 2];
                c.encode_utf16(&mut b).to_vec()
            }
            _ => vec![n as u16],
        };
        let units = if units.len() == 2 && units[1] == 0 { vec![units[0]] } else { units };
        let mut consumed = false;
        let mut bb: Vec<u8> = Vec::new();
        for nu in &self.non_unicode_subtable_indices {
            let Some(cs) = CharsetUtil::for_name_icu(Some(nu.charset_name)) else {
                let n2 = self.get_mapping(nu.subtable_offset, n)?;
                if n2 == 0 {
                    continue;
                }
                return Ok(n2);
            };
            if !CharsetUtil::can_encode(cs, n as u16) {
                continue;
            }
            if !consumed {
                consumed = true;
                match CharsetUtil::encode(cs, &units) {
                    Some(b) => bb.extend_from_slice(&b[..b.len().min(10 - bb.len())]),
                    None => continue,
                }
            }
            if bb.len() == 1 {
                return self.get_mapping(nu.subtable_offset, bb[0] as i32);
            }
            if bb.len() != 2 {
                continue;
            }
            return self.get_mapping(nu.subtable_offset, (bb[0] as i32) << 8 | bb[1] as i32);
        }
        Ok(0)
    }

    pub fn char2glyph(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint32asint(4 + 8 * n2 + 4, "Offset to cmap subtable is big")?;
        self.get_mapping(n3, n)
    }

    pub fn get_lowest_mapped_code(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(n)?;
        match n2 {
            0 => self.get_lowest_mapped_code_format0(n),
            2 => self.get_lowest_mapped_code_format2(n),
            4 => self.get_lowest_mapped_code_format4(n),
            6 => self.get_lowest_mapped_code_format6(n),
            8 => self.get_lowest_mapped_code_format8(n),
            10 => self.get_lowest_mapped_code_format10(n),
            12 => self.get_lowest_mapped_code_format12(n),
            _ => Err(FontEngineException::unsupported_font(format!("cmap subtable format {}", n2))),
        }
    }

    pub fn get_highest_mapped_code(&self, n: i32, bl: bool) -> FontResult<i32> {
        let n2 = self.data.getuint16(n)?;
        match n2 {
            0 => self.get_highest_mapped_code_format0(n),
            2 => self.get_highest_mapped_code_format2(n),
            4 => self.get_highest_mapped_code_format4(n),
            6 => self.get_highest_mapped_code_format6(n),
            8 => self.get_highest_mapped_code_format8(n, bl),
            10 => self.get_highest_mapped_code_format10(n, bl),
            12 => self.get_highest_mapped_code_format12(n, bl),
            _ => Err(FontEngineException::unsupported_font(format!("cmap subtable format {}", n2))),
        }
    }

    pub fn get_mapping(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint16(n)?;
        match n3 {
            0 => self.get_mapping_format0(n, n2),
            2 => self.get_mapping_format2(n, n2),
            4 => Ok(self.get_mapping_format4(n, n2)),
            6 => self.get_mapping_format6(n, n2),
            8 => self.get_mapping_format8(n, n2),
            10 => self.get_mapping_format10(n, n2),
            12 => self.get_mapping_format12(n, n2),
            _ => Err(FontEngineException::unsupported_font(format!("cmap subtable format {}", n3))),
        }
    }

    pub fn get_offset(&self, n: i32) -> FontResult<i32> {
        self.data.getuint32asint(4 + n * 8 + 4, "Offset to cmap subtable is big")
    }

    pub fn glyph2char(&self, n: i32, n2: i32) -> FontResult<Vec<i32>> {
        let mut a = vec![-1i32; n.max(0) as usize];
        let n3 = self.data.getuint32asint(4 + 8 * n2 + 4, "Offset to cmap subtable is big")?;
        let n4 = self.data.getuint16(n3)?;
        match n4 {
            0 => self.get_inverted_mapping_format0(&mut a, n3)?,
            2 => self.get_inverted_mapping_format2(&mut a, n3)?,
            4 => self.get_inverted_mapping_format4(&mut a, n3)?,
            6 => self.get_inverted_mapping_format6(&mut a, n3)?,
            8 => self.get_inverted_mapping_format8(&mut a, n3)?,
            10 => self.get_inverted_mapping_format10(&mut a, n3)?,
            12 => self.get_inverted_mapping_format12(&mut a, n3)?,
            _ => return Err(FontEngineException::unsupported_font(format!("cmap subtable format {}", n4))),
        }
        Ok(a)
    }

    fn get_lowest_mapped_code_format0(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(n + 2)? - 6 - 1;
        for i in 0..n2 {
            if self.data.getuint8(n + 6 + i)? > 0 {
                return Ok(i);
            }
        }
        Ok(n2)
    }

    fn get_highest_mapped_code_format0(&self, n: i32) -> FontResult<i32> {
        let mut i = self.data.getuint16(n + 2)? - 6 - 1;
        while i >= 0 {
            if self.data.getuint8(n + 6 + i)? > 0 {
                return Ok(i);
            }
            i -= 1;
        }
        Ok(0)
    }

    fn get_mapping_format0(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint16(n + 2)? - 6;
        if 0 <= n2 && n2 < n3 {
            return self.data.getuint8(n + 6 + n2);
        }
        Ok(0)
    }

    fn get_inverted_mapping_format0(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        let n2 = self.data.getuint16(n + 2)? - 6;
        for n3 in 0..n2 {
            let n4 = self.data.getuint8(n + 6 + n3)?;
            put(a, n4, n3)?;
        }
        Ok(())
    }

    fn get_lowest_mapped_code_format2(&self, n: i32) -> FontResult<i32> {
        for n10 in 0..=255 {
            let n9 = self.data.getuint16(n + 6 + 2 * n10)? / 8;
            let n8 = n + 518 + 8 * n9;
            let n7 = self.data.getuint16(n8)?;
            let n6 = self.data.getuint16(n8 + 2)? + n7 - 1;
            if n7 > n10 || n10 > n6 {
                continue;
            }
            let n5 = self.data.getuint16(n8 + 4)?;
            let n4 = self.data.getuint16(n8 + 6)?;
            let n3 = n8 + 6 + n4;
            let n2 = self.data.getuint16(n3 + 2 * (n10 - n7))?;
            if n2 == 0 || (n2 + n5) % 65536 == 0 {
                continue;
            }
            return Ok(n10);
        }
        for n10 in 1..=255 {
            let n9 = self.data.getuint16(n + 6 + 2 * n10)? / 8;
            let n8 = n + 518 + 8 * n9;
            let n6 = self.data.getuint16(n8)?;
            let mut n7 = n6;
            let n5 = self.data.getuint16(n8 + 2)? + n7 - 1;
            while n7 <= n5 {
                let n4 = self.data.getuint16(n8 + 4)?;
                let n3 = self.data.getuint16(n8 + 6)?;
                let n2 = n8 + 6 + n3;
                let n11 = self.data.getuint16(n2 + 2 * (n7 - n6))?;
                if n11 != 0 && (n11 + n4) % 65536 != 0 {
                    return Ok(n10 << 8 | n7);
                }
                n7 += 1;
            }
        }
        Ok(0)
    }

    fn get_highest_mapped_code_format2(&self, n: i32) -> FontResult<i32> {
        let mut n10 = 255;
        while n10 > 0 {
            let n9 = self.data.getuint16(n + 6 + 2 * n10)? / 8;
            let n8 = n + 518 + 8 * n9;
            let n7 = self.data.getuint16(n8)?;
            let mut n6 = self.data.getuint16(n8 + 2)? + n7 - 1;
            while n7 <= n6 {
                let n5 = self.data.getuint16(n8 + 4)?;
                let n4 = self.data.getuint16(n8 + 6)?;
                let n3 = n8 + 6 + n4;
                let n2 = self.data.getuint16(n3 + 2 * (n6 - n7))?;
                if n2 != 0 && (n2 + n5) % 65536 != 0 {
                    return Ok(n10 << 8 | n6);
                }
                n6 -= 1;
            }
            n10 -= 1;
        }
        let mut n10 = 255;
        while n10 >= 0 {
            let n9 = self.data.getuint16(n + 6 + 2 * n10)? / 8;
            let n8 = n + 518 + 8 * n9;
            let n7 = self.data.getuint16(n8)?;
            let n6 = self.data.getuint16(n8 + 2)? + n7 - 1;
            if !(n7 > n10 || n10 > n6) {
                let n5 = self.data.getuint16(n8 + 4)?;
                let n4 = self.data.getuint16(n8 + 6)?;
                let n3 = n8 + 6 + n4;
                let n2 = self.data.getuint16(n3 + 2 * (n10 - n7))?;
                if n2 != 0 && (n2 + n5) % 65536 != 0 {
                    return Ok(n10);
                }
            }
            n10 -= 1;
        }
        Ok(0)
    }

    fn get_mapping_format2(&self, n: i32, n2: i32) -> FontResult<i32> {
        let mut n3 = n2 >> 8 & 0xFF;
        let mut n4 = n2 & 0xFF;
        let n5 = if n3 == 0 && self.data.getuint16(n + 6 + 2 * n4)? == 0 { n4 } else { n3 };
        let n6 = n4;
        n3 = self.data.getuint16(n + 6 + 2 * n5)? / 8;
        n4 = n + 518 + 8 * n3;
        let n7 = self.data.getuint16(n4)?;
        let n8 = self.data.getuint16(n4 + 2)?;
        if n6 < n7 || n7 + n8 <= n6 {
            return Ok(0);
        }
        let n9 = self.data.getuint16(n4 + 4)?;
        let n10 = self.data.getuint16(n4 + 6)?;
        let n11 = n4 + 6 + n10;
        let n12 = self.data.getuint16(n11 + 2 * (n6 - n7))?;
        if n12 == 0 {
            return Ok(0);
        }
        Ok((n12 + n9) % 65536)
    }

    fn get_inverted_mapping_format2(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        for i in 0..=255 {
            let n3 = self.data.getuint16(n + 6 + 2 * i)? / 8;
            let n4 = n + 518 + n3 * 8;
            let n5 = self.data.getuint16(n4)?;
            let n6 = self.data.getuint16(n4 + 2)?;
            let n7 = self.data.getuint16(n4 + 4)?;
            let n8 = self.data.getuint16(n4 + 6)?;
            let n9 = n4 + 6 + n8;
            if n3 == 0 {
                let n2 = self.data.getuint16(n9 + 2 * (i - n5))?;
                if n2 == 0 {
                    continue;
                }
                put(a, (n2 + n7) % 65536, i)?;
                continue;
            }
            for n2 in n5..n5 + n6 {
                let n10 = self.data.getuint16(n9 + 2 * (n2 - n5))?;
                if n10 == 0 {
                    continue;
                }
                put(a, (n10 + n7) % 65536, (i << 8) + n2)?;
            }
        }
        Ok(())
    }

    /// Java swallows `InvalidFontException` around each segment and each code.
    fn get_lowest_mapped_code_format4(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(n + 6)? / 2;
        for n3 in 0..n2 {
            let Ok(start) = self.data.getuint16(n + 16 + 2 * n2 + 2 * n3) else { continue };
            let n5 = start;
            let Ok(n6) = self.data.getuint16(n + 14 + 2 * n3) else { continue };
            let mut n4 = start;
            while n4 <= n6 {
                let r: FontResult<Option<i32>> = (|| {
                    let n9 = self.data.getuint16(n + 16 + 6 * n2 + 2 * n3)?;
                    let n10 = self.data.getint16(n + 16 + 4 * n2 + 2 * n3)?;
                    if n9 == 0 && (n4 + n10) & 0xFFFF != 0 {
                        return Ok(Some(n4));
                    }
                    let n8 = n + 16 + 6 * n2 + 2 * n3 + n9 + 2 * (n4 - n5);
                    let n11 = self.data.getuint16(n8)?;
                    if n11 != 0 && (n11 + n10) & 0xFFFF != 0 {
                        return Ok(Some(n4));
                    }
                    Ok(None)
                })();
                if let Ok(Some(v)) = r {
                    return Ok(v);
                }
                n4 += 1;
            }
        }
        Ok(0)
    }

    fn get_highest_mapped_code_format4(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(n + 6)? / 2;
        let mut i = n2 - 1;
        while 0 <= i {
            if let (Ok(n3), Ok(end)) = (self.data.getuint16(n + 16 + 2 * n2 + 2 * i), self.data.getuint16(n + 14 + 2 * i)) {
                let mut j = end;
                while j >= n3 {
                    let r: FontResult<Option<i32>> = (|| {
                        let n6 = self.data.getuint16(n + 16 + 6 * n2 + 2 * i)?;
                        let n7 = self.data.getint16(n + 16 + 4 * n2 + 2 * i)?;
                        if n6 == 0 && (j + n7) & 0xFFFF != 0 {
                            return Ok(Some(j));
                        }
                        let n5 = n + 16 + 6 * n2 + 2 * i + n6 + 2 * (j - n3);
                        let n8 = self.data.getuint16(n5)?;
                        if n8 == 0 || (n8 + n7) & 0xFFFF == 0 {
                            return Ok(None);
                        }
                        Ok(Some(j))
                    })();
                    if let Ok(Some(v)) = r {
                        return Ok(v);
                    }
                    j -= 1;
                }
            }
            i -= 1;
        }
        Ok(0)
    }

    pub fn get_mapping_format4(&self, n: i32, n2: i32) -> i32 {
        let r: FontResult<i32> = (|| {
            let n3 = self.data.getuint16(n + 6)? / 2;
            let mut n4 = 0;
            let mut n5 = n3 - 1;
            while n4 <= n5 {
                let n6 = (n4 + n5) / 2;
                let n7 = self.data.getuint16(n + 16 + 2 * n3 + 2 * n6)?;
                let n8 = self.data.getuint16(n + 14 + 2 * n6)?;
                if n2 < n7 {
                    n5 = n6 - 1;
                    continue;
                }
                if n8 < n2 {
                    n4 = n6 + 1;
                    continue;
                }
                let n9 = self.data.getuint16(n + 16 + 6 * n3 + 2 * n6)?;
                let n10 = self.data.getint16(n + 16 + 4 * n3 + 2 * n6)?;
                if n9 == 0 {
                    return Ok((n2 + n10) & 0xFFFF);
                }
                let n11 = n + 16 + 6 * n3 + 2 * n6 + n9 + 2 * (n2 - n7);
                let n12 = self.data.getuint16(n11)?;
                if n12 == 0 {
                    return Ok(0);
                }
                return Ok((n12 + n10) & 0xFFFF);
            }
            Ok(0)
        })();
        r.unwrap_or(0)
    }

    fn get_inverted_mapping_format4(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        let n2 = self.data.getuint16(n + 6)? / 2;
        for i in 0..n2 {
            let n3 = self.data.getuint16(n + 16 + 2 * n2 + 2 * i)?;
            let n4 = self.data.getuint16(n + 14 + 2 * i)?;
            let n5 = n + 16 + 6 * n2 + 2 * i;
            let n6 = self.data.getuint16(n5)?;
            let n7 = self.data.getint16(n + 16 + 4 * n2 + 2 * i)?;
            for j in n3..=n4 {
                if n6 == 0 {
                    let n9 = (j + n7) & 0xFFFF;
                    if n9 == 0 || n9 >= a.len() as i32 {
                        continue;
                    }
                    a[n9 as usize] = j;
                    continue;
                }
                let n9 = n + 16 + 6 * n2 + 2 * i + n6 + 2 * (j - n3);
                let n10 = self.data.getuint16(n9)?;
                let n8 = (n10 + n7) & 0xFFFF;
                if n10 == 0 || n8 == 0 || n8 >= a.len() as i32 {
                    continue;
                }
                a[n8 as usize] = j;
            }
        }
        Ok(())
    }

    fn get_lowest_mapped_code_format6(&self, n: i32) -> FontResult<i32> {
        let n3 = self.data.getuint16(n + 6)?;
        let mut n2 = n3;
        let n4 = self.data.getuint16(n + 8)? + n2 - 1;
        while n2 <= n4 {
            if self.data.getuint16(n + 10 + (n2 - n3) * 2)? != 0 {
                return Ok(n2);
            }
            n2 += 1;
        }
        Ok(0)
    }

    fn get_highest_mapped_code_format6(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(n + 6)?;
        let mut i = self.data.getuint16(n + 8)? + n2 - 1;
        while n2 <= i {
            if self.data.getuint16(n + 10 + (i - n2) * 2)? != 0 {
                return Ok(i);
            }
            i -= 1;
        }
        Ok(0)
    }

    fn get_mapping_format6(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint16(n + 6)?;
        let n4 = self.data.getuint16(n + 8)?;
        if n3 <= n2 && n2 < n3 + n4 {
            return self.data.getuint16(n + 10 + (n2 - n3) * 2);
        }
        Ok(0)
    }

    fn get_inverted_mapping_format6(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        let n2 = self.data.getuint16(n + 6)?;
        let n3 = self.data.getuint16(n + 8)?;
        for i in 0..n3 {
            let g = self.data.getuint16(n + 10 + i * 2)?;
            put(a, g, n2 + i)?;
        }
        Ok(())
    }

    fn get_lowest_mapped_code_format8(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint32asint(n + 8204, "cmap subtable, format 8, nGroups is big")?;
        let mut n3 = 65535;
        for i in 0..n2 {
            let n4 = self.data.getuint32asint(n + 8208 + 12 * i, "cmap subtable, format 8, startCharCode is big")?;
            n3 = self.data.getuint32asint(n + 8208 + 12 * i + 4, "cmap subtable, format 8, endCharCode is big")?;
            let mut j = n4;
            while j <= n3 {
                let n5 = self.data.getuint32asint(n + 8208 + 12 * i + 8, "cmap subtable, format 8, startGlyphID is big")?.wrapping_add(j).wrapping_sub(n4);
                if n5 != 0 && j <= 65535 {
                    return Ok(j);
                }
                j += 1;
            }
        }
        Ok(n3)
    }

    fn get_highest_mapped_code_format8(&self, n: i32, bl: bool) -> FontResult<i32> {
        let n2 = self.data.getuint32asint(n + 8204, "cmap subtable, format 8, nGroups is big")?;
        let mut i = n2 - 1;
        while 0 <= i {
            let n3 = self.data.getuint32asint(n + 8208 + 12 * i, "cmap subtable, format 8, startCharCode is big")?;
            let mut j = self.data.getuint32asint(n + 8208 + 12 * i + 4, "cmap subtable, format 8, endCharCode is big")?;
            while j >= n3 {
                let n4 = self.data.getuint32asint(n + 8208 + 12 * i + 8, "cmap subtable, format 8, startGlyphID is big")?.wrapping_add(j).wrapping_sub(n3);
                if !(n4 == 0 || bl && j > 65535) {
                    return Ok(j);
                }
                j -= 1;
            }
            i -= 1;
        }
        Ok(0)
    }

    fn get_mapping_format8(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint32asint(n + 8204, "cmap subtable, format 8, nGroups is big")?;
        let mut n4 = 0;
        let mut n5 = n3 - 1;
        while n4 <= n5 {
            let n6 = (n4 + n5) / 2;
            let n7 = self.data.getuint32asint(n + 8208 + 12 * n6, "cmap subtable, format 8, startCharCode is big")?;
            let n8 = self.data.getuint32asint(n + 8208 + 12 * n6 + 4, "cmap subtable, format 8, endCharCode is big")?;
            if n2 < n7 {
                n5 = n6 - 1;
                continue;
            }
            if n8 < n2 {
                n4 = n6 + 1;
                continue;
            }
            let n9 = self.data.getuint32asint(n + 8208 + 12 * n6 + 8, "cmap subtable, format 8, startGlyphID is big")?;
            return Ok(n2.wrapping_sub(n7).wrapping_add(n9));
        }
        Ok(0)
    }

    fn get_inverted_mapping_format8(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        let n2 = self.data.getuint32asint(n + 8204, "cmap subtable, format 8, nGroups is big")?;
        for i in 0..n2 {
            let n3 = self.data.getuint32asint(n + 8208 + 12 * i, "cmap subtable, format 8, startCharCode is big")?;
            let n4 = self.data.getuint32asint(n + 8208 + 12 * i + 4, "cmap subtable, format 8, endCharCode is big")?;
            let n5 = self.data.getuint32asint(n + 8208 + 12 * i + 8, "cmap subtable, format 8, startGlyphID is big")?;
            for j in n3..=n4 {
                put(a, n5 + j - n3, j)?;
            }
        }
        Ok(())
    }

    fn get_lowest_mapped_code_format10(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint32asint(n + 12, "cmap subtable, format 10, startCharCode is big")?;
        let n3 = self.data.getuint32asint(n + 16, "cmap subtable, format 10, numChars is big")?.wrapping_add(n2).wrapping_sub(1);
        let mut i = n2;
        while i <= n3 {
            if self.data.getuint16(n + 20 + 2 * (i - n2))? > 0 {
                return Ok(i);
            }
            i += 1;
        }
        Ok(n3)
    }

    fn get_highest_mapped_code_format10(&self, n: i32, bl: bool) -> FontResult<i32> {
        let n2 = self.data.getuint32asint(n + 12, "cmap subtable, format 10, startCharCode is big")?;
        let n3 = self.data.getuint32asint(n + 16, "cmap subtable, format 10, numChars is big")?;
        let mut i = n2.wrapping_add(n3).wrapping_sub(1);
        while n2 <= i {
            let n4 = self.data.getuint16(n + 20 + 2 * (i - n2))?;
            if !(n4 <= 0 || bl && i > 65535) {
                return Ok(i);
            }
            i -= 1;
        }
        Ok(0)
    }

    fn get_mapping_format10(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint32asint(n + 12, "cmap subtable, format 10, startCharCode is big")?;
        let n4 = self.data.getuint32asint(n + 16, "cmap subtable, format 10, numChars is big")?;
        if n3 <= n2 && n2 < n3.wrapping_add(n4) {
            return self.data.getuint16(n + 20 + 2 * (n2 - n3));
        }
        Ok(0)
    }

    fn get_inverted_mapping_format10(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        let n2 = self.data.getuint32asint(n + 12, "cmap subtable, format 10, startCharCode is big")?;
        let n3 = self.data.getuint32asint(n + 16, "cmap subtable, format 10, numChars is big")?;
        let mut i = n2;
        while i < n2.wrapping_add(n3) {
            let g = self.data.getuint16(n + 20 + 2 * (i - n2))?;
            put(a, g, i)?;
            i += 1;
        }
        Ok(())
    }

    fn get_lowest_mapped_code_format12(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint32(n + 12)? as i32;
        let mut n3 = 65535;
        for i in 0..n2 {
            let n4 = self.data.getuint32(n + 16 + 12 * i)? as i32;
            n3 = self.data.getuint32(n + 16 + 12 * i + 4)? as i32;
            let mut j = n4;
            while j <= n3 {
                let n5 = (self.data.getuint32(n + 16 + 12 * i + 8)? as i32).wrapping_add(j.wrapping_sub(n4));
                if n5 > 0 {
                    return Ok(j);
                }
                j += 1;
            }
        }
        Ok(n3)
    }

    fn get_highest_mapped_code_format12(&self, n: i32, bl: bool) -> FontResult<i32> {
        let n2 = self.data.getuint32(n + 12)? as i32;
        let mut i = n2 - 1;
        while 0 <= i {
            let n3 = self.data.getuint32(n + 16 + 12 * i)? as i32;
            let mut j = self.data.getuint32(n + 16 + 12 * i + 4)? as i32;
            while j >= n3 {
                let n4 = (self.data.getuint32(n + 16 + 12 * i + 8)? as i32).wrapping_add(j.wrapping_sub(n3));
                if !(n4 <= 0 || bl && j > 65535) {
                    return Ok(j);
                }
                j -= 1;
            }
            i -= 1;
        }
        Ok(0)
    }

    fn get_mapping_format12(&self, n: i32, n2: i32) -> FontResult<i32> {
        let n3 = self.data.getuint32(n + 12)? as i32;
        let mut n4 = 0;
        let mut n5 = n3 - 1;
        while n4 <= n5 {
            let n6 = (n4 + n5) / 2;
            let n7 = self.data.getuint32(n + 16 + 12 * n6)? as i32;
            let n8 = self.data.getuint32(n + 16 + 12 * n6 + 4)? as i32;
            if n2 < n7 {
                n5 = n6 - 1;
                continue;
            }
            if n8 < n2 {
                n4 = n6 + 1;
                continue;
            }
            let n9 = self.data.getuint32(n + 16 + 12 * n6 + 8)? as i32;
            return Ok(n9.wrapping_add(n2.wrapping_sub(n7)));
        }
        Ok(0)
    }

    fn get_inverted_mapping_format12(&self, a: &mut [i32], n: i32) -> FontResult<()> {
        let n2 = self.data.getuint32(n + 12)? as i32;
        for i in 0..n2 {
            let n3 = self.data.getuint32(n + 16 + 12 * i)? as i32;
            let n4 = self.data.getuint32(n + 16 + 12 * i + 4)? as i32;
            let n5 = self.data.getuint32(n + 16 + 12 * i + 8)? as i32;
            let mut j = n3;
            while j <= n4 {
                put(a, n5.wrapping_add(j - n3), j)?;
                j += 1;
            }
        }
        Ok(())
    }

    pub fn subset_and_stream_for_swf(&self, tree_set: &BTreeSet<i32>, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>, bl: bool) -> FontResult<()> {
        let bmp_only = tree_set.iter().all(|&c| c <= 65535);
        let n = if bl { 2 } else { 1 };
        let mut b = OTByteArray::get_ot_byte_array_builder_instance();
        b.ensure_capacity(4 + 8 * n);
        b.setuint16(0, 0);
        b.setuint16(2, n);
        let n3 = 4 + 8 * n;
        let n2 = if bmp_only {
            b.setuint16(4, 3);
            b.setuint16(6, if self.symbol_subtable_offset != -1 { 0 } else { 1 });
            b.setuint32(8, n3);
            self.write_format4_subtable(&mut b, n3, tree_set, subset, self.padding_byte)?
        } else {
            b.setuint16(4, 3);
            b.setuint16(6, 10);
            b.setuint32(8, n3);
            self.write_format12_subtable(&mut b, n3, tree_set, subset)?
        };
        if bl {
            b.setuint16(12, 0);
            b.setuint16(14, 5);
            b.setuint32(16, n3 + n2);
            let p = self.probe(0, 5)?;
            self.write_format14_subtable(&mut b, n3 + n2, tree_set, subset, p)?;
        }
        map.insert(TAG_CMAP, b);
        Ok(())
    }

    fn compute_code_point_to_emit(n: i32, n2: i32) -> i32 {
        if n2 == 0 {
            return n;
        }
        n2 | n & 0xFF
    }

    fn write_format4_subtable(&self, b: &mut OTByteArrayBuilder, n: i32, tree_set: &BTreeSet<i32>, subset: &dyn Subset, n2: i32) -> FontResult<i32> {
        let mut it = tree_set.iter();
        let mut n5 = 0;
        let mut segs: Vec<SegmentInfo> = Vec::new();
        let mut bl = true;
        if let Some(&c) = it.next() {
            n5 = c;
            let g = subset.get_existing_subset_gid(self.unicode_char2glyph(n5)?)?;
            segs.push(SegmentInfo { can_use_id_delta: true, id_delta: g - Cmap::compute_code_point_to_emit(n5, n2), start_code: n5, end_code: 0 });
            if n5 == 65535 {
                bl = false;
            }
        }
        for &n4 in it {
            if n4 == 65535 {
                bl = false;
            }
            if n4 != n5 + 1 {
                segs.last_mut().unwrap().end_code = n5;
                let g = subset.get_existing_subset_gid(self.unicode_char2glyph(n4)?)?;
                segs.push(SegmentInfo { can_use_id_delta: true, id_delta: g - Cmap::compute_code_point_to_emit(n4, n2), start_code: n4, end_code: 0 });
            } else {
                let s = segs.last_mut().unwrap();
                if s.can_use_id_delta {
                    let g = subset.get_existing_subset_gid(self.unicode_char2glyph(n4)?)?;
                    if g - Cmap::compute_code_point_to_emit(n4, n2) != s.id_delta {
                        s.can_use_id_delta = false;
                    }
                }
            }
            n5 = n4;
        }
        if let Some(s) = segs.last_mut() {
            s.end_code = n5;
        }
        let mut n4 = segs.len() as i32;
        if bl {
            n4 += 1;
        }
        let mut n3 = 16 + 8 * n4;
        b.ensure_capacity(n + n3);
        b.setuint16(n, 4);
        b.setuint16(n + 4, 0);
        b.setuint16(n + 6, n4 * 2);
        let n6 = Cmap::log2(n4 as f64).floor() as i32;
        let n7 = 2 * java_pow2(n6);
        b.setuint16(n + 8, n7);
        b.setuint16(n + 10, n6);
        b.setuint16(n + 12, 2 * n4 - n7);
        b.setuint16(n + 14 + n4 * 2, 0);
        let mut n8 = 0;
        for (i, s) in segs.iter().enumerate() {
            let i = i as i32;
            b.setuint16(n + 14 + 2 * i, Cmap::compute_code_point_to_emit(s.end_code, n2));
            b.setuint16(n + 16 + 2 * n4 + 2 * i, Cmap::compute_code_point_to_emit(s.start_code, n2));
            b.setint16(n + 16 + 4 * n4 + 2 * i, if s.can_use_id_delta { s.id_delta } else { 0 });
            b.setuint16(n + 16 + 6 * n4 + 2 * i, if s.can_use_id_delta { 0 } else { (n8 + n4 - i) * 2 });
            if s.can_use_id_delta {
                continue;
            }
            b.ensure_capacity(n + 16 + 8 * n4 + 2 * (n8 + s.end_code - s.start_code + 1));
            for n9 in s.start_code..=s.end_code {
                let n10 = subset.get_existing_subset_gid(self.unicode_char2glyph(n9)?)?;
                b.setuint16(n + 16 + 8 * n4 + 2 * n8, n10);
                n8 += 1;
            }
        }
        if bl {
            b.setuint16(n + 14 + 2 * segs.len() as i32, 65535);
            b.setuint16(n + 16 + 4 * n4 - 2, 65535);
            b.setuint16(n + 16 + 6 * n4 - 2, 1);
            b.setuint16(n + 16 + 8 * n4 - 2, 0);
        }
        n3 += 2 * n8;
        b.setuint16(n + 2, n3);
        if n3 >= 65536 {
            return Err(FontEngineException::unsupported_font("Format 4 cmap subtable is too big for format"));
        }
        Ok(n3)
    }

    /// `Math.log(d) / Math.log(2.0)`.
    pub fn log2(d: f64) -> f64 {
        d.ln() / 2.0f64.ln()
    }

    fn write_format12_subtable(&self, b: &mut OTByteArrayBuilder, n: i32, tree_set: &BTreeSet<i32>, subset: &dyn Subset) -> FontResult<i32> {
        let mut it = tree_set.iter();
        let mut n2 = 0;
        let mut n3 = 0;
        let mut n4 = 0;
        let mut n5 = 16;
        b.ensure_capacity(n + n5);
        if let Some(&c) = it.next() {
            n2 = c;
            n3 = subset.get_existing_subset_gid(self.unicode_char2glyph(n2)?)?;
            n5 += 12;
            b.ensure_capacity(n + n5);
            b.setuint32(n + 16 + 12 * n4, n2);
            b.setuint32(n + 16 + 12 * n4 + 8, n3);
            n4 += 1;
        }
        for &n6 in it {
            let n7 = subset.get_existing_subset_gid(self.unicode_char2glyph(n6)?)?;
            if n6 != n2 + 1 || n7 != n3 + 1 {
                b.setuint32(n + 16 + 12 * (n4 - 1) + 4, n2);
                n5 += 12;
                b.ensure_capacity(n + n5);
                b.setuint32(n + 16 + 12 * n4, n6);
                b.setuint32(n + 16 + 12 * n4 + 8, n7);
                n4 += 1;
            }
            n2 = n6;
            n3 = n7;
        }
        if n4 > 0 {
            b.setuint32(n + 16 + 12 * (n4 - 1) + 4, n2);
        }
        b.setuint16(n, 12);
        b.setuint16(n + 2, 0);
        b.setuint32(n + 4, n5);
        b.setuint32(n + 8, 0);
        b.setuint32(n + 12, n4);
        Ok(n5)
    }

    pub fn iterate_format14(&self, n: i32, c: &mut dyn Format14Consumer) -> FontResult<()> {
        let n2 = self.data.getuint32asint(n + 6, "Only 2^31 selectors supported")?;
        for i in 0..n2 {
            let n7 = self.data.getuint24(n + 10 + 11 * i)?;
            let mut n8 = self.data.getuint32asint(n + 10 + 11 * i + 3, "Only 2^31 offsets supported")?;
            if n8 != 0 {
                n8 += n;
                let n6 = self.data.getuint32asint(n8, "Only 2^31 UV ranges supported")?;
                let mut stop = false;
                for n5 in 0..n6 {
                    let n4 = self.data.getuint24(n8 + 4 + 4 * n5)?;
                    let n3 = self.data.getuint8(n8 + 4 + 4 * n5 + 3)?;
                    for j in 0..=n3 {
                        if c.default_uv(n7, n4 + j) {
                            continue;
                        }
                        stop = true;
                        break;
                    }
                    if stop {
                        break;
                    }
                }
            }
            n8 = self.data.getuint32asint(n + 10 + 11 * i + 7, "Only 2^31 offsets supported")?;
            if n8 == 0 {
                continue;
            }
            n8 += n;
            let n6 = self.data.getuint32asint(n8, "Only 2^31 UV ranges supported")?;
            let mut n5 = 0;
            while n5 < n6 {
                let n4 = self.data.getuint24(n8 + 4 + 5 * n5)?;
                let n3 = self.data.getuint16(n8 + 4 + 5 * n5 + 3)?;
                if !c.non_default_uv(n7, n4, n3)? {
                    break;
                }
                n5 += 1;
            }
        }
        Ok(())
    }

    fn write_format14_subtable(&self, b: &mut OTByteArrayBuilder, n: i32, tree_set: &BTreeSet<i32>, subset: &dyn Subset, n2: i32) -> FontResult<i32> {
        let mut w = Format14Writer { subset, codepoints: tree_set, builder: b, new_st_offset: n, last_vs: -1, vs_to_unicode: Vec::new(), st_size: 0 };
        self.iterate_format14(n2, &mut w)?;
        w.write_subtable()?;
        Ok(w.st_size)
    }

    /// `gatherPossibleMappings(iterator, set, subset)` over the sorted code points.
    pub fn gather_possible_mappings(&self, it: &mut dyn Iterator<Item = i32>, set: &mut BTreeSet<i32>, subset: &mut dyn Subset) -> FontResult<bool> {
        for n in it {
            let n2 = self.unicode_char2glyph(n)?;
            if n2 == 0 {
                continue;
            }
            set.insert(n);
            subset.get_subset_gid(n2)?;
        }
        let n = self.probe(0, 5)?;
        if n != -1 {
            let snapshot = set.clone();
            let mut h = Format14Harvester { harvested_cps: &snapshot, harvested_gids: subset, variation_uvs: crate::asc::util::jhash::JHashSet::new(), variation_sequences_found: false };
            self.iterate_format14(n, &mut h)?;
            let uvs: Vec<i32> = h.variation_uvs.iter().cloned().collect();
            let found = h.variation_sequences_found;
            for n3 in uvs {
                let n4 = self.unicode_char2glyph(n3)?;
                if n4 == 0 {
                    continue;
                }
                set.insert(n3);
                subset.get_subset_gid(n4)?;
            }
            return Ok(found);
        }
        Ok(false)
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(TAG_CMAP, data_as_byte_array(&self.data));
    }

    /// The ICU decoder's state between codes is not carried over (only bites on malformed codes).
    fn generate_required_cmap(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        if self.non_unicode_subtable_indices.is_empty() {
            return Err(FontEngineException::unsupported_font("Could not generate required cmap subtables"));
        }
        let mut tree_set = BTreeSet::new();
        for nu in &self.non_unicode_subtable_indices {
            if nu.charset_name == "MacSymbol" {
                if self.non_unicode_subtable_indices.len() != 1 {
                    continue;
                }
                return Err(FontEngineException::unsupported_font("Cannot generate a required cmap"));
            }
            let cs = CharsetUtil::for_name_icu(Some(nu.charset_name)).unwrap();
            let n3 = self.get_highest_mapped_code(nu.subtable_offset, false)?;
            let n4 = self.get_lowest_mapped_code(nu.subtable_offset)?;
            let n5 = self.offset_to_index(nu.subtable_offset)?;
            let mut bb = [0u8; 2];
            let mut last = 0u16;
            for j in n4..=n3 {
                let n6 = self.char2glyph(j, n5)?;
                if n6 == 0 {
                    continue;
                }
                if j < 256 {
                    bb[0] = (j & 0xFF) as u8;
                } else if j < 65536 {
                    bb[0] = (j >> 8) as u8;
                    bb[1] = (j & 0xFF) as u8;
                } else {
                    return Err(FontEngineException::unsupported_font("Only 2-byte encodings supported"));
                }
                // The one-char buffer keeps its previous char when nothing was written.
                match CharsetUtil::decode_first_unit(cs, &bb) {
                    Err(()) => continue,
                    Ok(Some(c)) => last = c,
                    Ok(None) => {}
                }
                tree_set.insert(last as i32);
            }
        }
        self.write_format4_subtable(b, n, &tree_set, &SubsetDefaultImpl::new(n2, false), 0)
    }

    fn copy_subtable(&self, b: &mut OTByteArrayBuilder, n: i32, n2: i32) -> FontResult<i32> {
        let n4 = self.data.getuint16(n2)?;
        let n3 = match n4 {
            0 | 2 | 4 | 6 => self.data.getuint16(n2 + 2)?,
            8 | 10 => self.data.getuint32asint(n2 + 4, "Only signed int length cmap subtables supported")?,
            12 => 16 + 12 * self.data.getuint32asint(n2 + 12, "Only signed int length cmap subtables supported")?,
            14 => self.data.getuint32asint(n2 + 2, "Only signed int length cmap subtables supported")?,
            _ => return Err(FontEngineException::unsupported_font(format!("Unsupported cmap format: {}", n4))),
        };
        if n3 > 0 && n2 as i64 + n3 as i64 > self.data.base.size as i64 {
            b.replace_from(n, &self.data.base, n2, self.data.get_size() - n2)?;
        } else {
            b.replace_from(n, &self.data.base, n2, n3)?;
        }
        Ok(n3)
    }

    pub fn stream_for_swf(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>, n: i32) -> FontResult<()> {
        let mut sel = SWFCmapSelector::default();
        self.enumerate_cmaps(&mut sel)?;
        let mut n2 = 4;
        let mut n3 = sel.cmaps_to_keep.len() as i32 + if sel.found_required_cmap { 0 } else { 1 };
        let mut n4 = n2 + 8 * n3;
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(n4);
        b.setuint16(0, 0);
        if !sel.found_required_cmap {
            b.setuint16(n2, 3);
            b.setuint16(n2 + 2, 1);
            b.setuint32(n2 + 4, n4);
            n2 += 8;
            n4 += self.generate_required_cmap(&mut b, n4, n)?;
        }
        for c in &sel.cmaps_to_keep {
            let n5 = self.copy_subtable(&mut b, n4, c.offset)?;
            if n5 > 0 {
                b.setuint16(n2, c.platform_id);
                b.setuint16(n2 + 2, c.platform_encoding);
                b.setuint32(n2 + 4, n4);
                n4 += n5;
                n2 += 8;
                continue;
            }
            n3 -= 1;
        }
        b.setuint16(2, n3);
        map.insert(TAG_CMAP, b);
        Ok(())
    }

    pub fn offset_to_index(&self, n: i32) -> FontResult<i32> {
        let n2 = self.data.getuint16(2)?;
        for i in 0..n2 {
            if n as i64 == self.data.getuint32(4 + i * 8 + 4)? {
                return Ok(i);
            }
        }
        Ok(-1)
    }
}

/// `(int)Math.pow(2.0, n)` for an int `n`.
fn java_pow2(n: i32) -> i32 {
    if n < -1074 {
        return 0;
    }
    let d = 2.0f64.powi(n);
    d as i32
}

#[allow(dead_code)]
fn unused(_: HashSet<i32>) {}
