//! Port of afe.jar `com.adobe.fontengine.font.opentype.LookupTableSubsetter`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table::LayoutTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::layout_table_subsetter::{write_byte_array_at_offset, CoverageGenerator, MAX_OFFSET};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::util::jhash::{JHashMap, JHashSet};

/// The `Map` of original coverage offset to `NewCoverage`.
pub type CoverageMap = JHashMap<i32, NewCoverage>;

/// `LookupTableSubsetter.NewCoverage`.
#[derive(Clone)]
pub struct NewCoverage {
    pub new_offset: i32,
    pub new_coverage: OTByteArray,
    pub glyph_count: i32,
    pub referring_orig_lookups: JHashMap<i32, JHashSet<i32>>,
}

impl NewCoverage {
    fn new() -> NewCoverage {
        NewCoverage { new_offset: -1, new_coverage: OTByteArray::default(), glyph_count: 0, referring_orig_lookups: JHashMap::new() }
    }

    pub fn get_max_referring_orig_lookup(&self) -> i32 {
        let mut n = 0;
        for &n2 in self.referring_orig_lookups.keys() {
            if n >= n2 {
                continue;
            }
            n = n2;
        }
        n
    }

    pub fn get_max_subtable(&self, n: i32) -> i32 {
        let set = self.referring_orig_lookups.get(&n).expect("java.lang.NullPointerException");
        let mut n2 = 0;
        for &n3 in set.iter() {
            if n2 >= n3 {
                continue;
            }
            n2 = n3;
        }
        n2
    }
}

/// `LookupTableSubsetter.LookupSubset`.
pub struct LookupSubset<'m> {
    pub base: SubsetDefaultImpl,
    pub lookups_to_keep: &'m BTreeMap<i32, Vec<i32>>,
}

impl<'m> LookupSubset<'m> {
    pub fn new(tree_map: &'m BTreeMap<i32, Vec<i32>>, n: i32) -> FontResult<LookupSubset<'m>> {
        let mut base = SubsetDefaultImpl::new(n, true);
        for &n2 in tree_map.keys() {
            base.get_subset_gid(n2)?;
        }
        Ok(LookupSubset { base, lookups_to_keep: tree_map })
    }

    pub fn get_subtables(&self, n: i32) -> &'m Vec<i32> {
        self.lookups_to_keep.get(&n).expect("java.lang.NullPointerException")
    }
}

impl Subset for LookupSubset<'_> {
    fn get_num_glyphs(&self) -> i32 {
        self.base.get_num_glyphs()
    }
    fn get_subset_gid(&mut self, n: i32) -> FontResult<i32> {
        self.base.get_subset_gid(n)
    }
    fn get_existing_subset_gid(&self, n: i32) -> FontResult<i32> {
        self.base.get_existing_subset_gid(n)
    }
    fn get_full_gid(&self, n: i32) -> i32 {
        self.base.get_full_gid(n)
    }
}

/// `LookupTableSubsetter.FeatureSubset`.
pub struct FeatureSubset {
    pub base: SubsetDefaultImpl,
}

impl FeatureSubset {
    fn new(must_keep_feature: &dyn Fn(i32) -> FontResult<bool>, data: &OTByteArray, n: i32, map: &BTreeMap<i32, Vec<i32>>) -> FontResult<FeatureSubset> {
        let mut base = SubsetDefaultImpl::new(data.getuint16(n)?, true);
        let n2 = data.getuint16(n)?;
        for i in 0..n2 {
            let n3 = data.get_offset(n, 2 + 6 * i + 4)?;
            if !Self::feature_contains_lookups(data, n3, map)? && !must_keep_feature(n + 2 + 6 * i)? {
                continue;
            }
            base.get_subset_gid(i)?;
        }
        Ok(FeatureSubset { base })
    }

    fn feature_contains_lookups(data: &OTByteArray, n: i32, map: &BTreeMap<i32, Vec<i32>>) -> FontResult<bool> {
        let n2 = data.getuint16(n + 2)?;
        for i in 0..n2 {
            let n3 = data.getuint16(n + 4 + 2 * i)?;
            if map.contains_key(&n3) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn get_num_glyphs(&self) -> i32 {
        self.base.get_num_glyphs()
    }
    fn get_existing_subset_gid(&self, n: i32) -> FontResult<i32> {
        self.base.get_existing_subset_gid(n)
    }
    fn get_full_gid(&self, n: i32) -> i32 {
        self.base.get_full_gid(n)
    }
}

/// `LookupTableSubsetter.LookupSubtableGenerator`.
pub trait LookupSubtableGenerator {
    fn write_subtable(&mut self) -> FontResult<i32>;
}

/// The fields of `LayoutTableSubsetter` and `LookupTableSubsetter`.
pub struct LookupTableSubsetterBase<'a> {
    pub orig_table: &'a LookupTable,
    pub builder: OTByteArrayBuilder,
    pub orig_num_glyphs: i32,
    dummy_lookup_id: i32,
}

impl<'a> LookupTableSubsetterBase<'a> {
    pub fn new(orig_table: &'a LookupTable, builder: OTByteArrayBuilder, orig_num_glyphs: i32) -> Self {
        LookupTableSubsetterBase { orig_table, builder, orig_num_glyphs, dummy_lookup_id: -1 }
    }
}

/// `LookupTableSubsetter.addToCoveragesMap`.
pub fn add_to_coverages_map(t: &LayoutTable, n: i32, map: &mut CoverageMap, n2: i32, n3: i32, subset: &dyn Subset) -> FontResult<()> {
    if !map.contains_key(&n) {
        let mut nc = NewCoverage::new();
        let mut set = JHashSet::new();
        set.insert(n3);
        nc.referring_orig_lookups.insert(n2, set);
        let mut g = CoverageGenerator::new_instance(t, n, subset)?;
        nc.new_coverage = g.generate_coverage()?.to_ot_byte_array();
        nc.glyph_count = g.num_glyphs_found;
        map.insert(n, nc);
    } else {
        let nc = map.get_mut(&n).unwrap();
        if let Some(set) = nc.referring_orig_lookups.get_mut(&n2) {
            set.insert(n3);
        } else {
            let mut set = JHashSet::new();
            set.insert(n3);
            nc.referring_orig_lookups.insert(n2, set);
        }
    }
    Ok(())
}

/// `LookupTableSubsetter.patchCoverageAtOffset`.
pub fn patch_coverage_at_offset(t: &LayoutTable, builder: &mut OTByteArrayBuilder, n: i32, n2: i32, n3: i32, map: &CoverageMap) -> FontResult<()> {
    let n4 = t.data.get_offset(n, n3)?;
    let nc = map.get(&n4).expect("java.lang.NullPointerException");
    if nc.new_offset - n2 > 65535 {
        panic!("New coverage offset is out of range!");
    }
    if nc.new_offset < n2 {
        panic!("New coverage written out before subtable!");
    }
    builder.setuint16(n2 + n3, nc.new_offset - n2);
    Ok(())
}

pub trait LookupTableSubsetter<'a> {
    fn base(&self) -> &LookupTableSubsetterBase<'a>;
    fn base_mut(&mut self) -> &mut LookupTableSubsetterBase<'a>;

    /// Java picks `map` or `map2` (extension subtables) itself.
    #[allow(clippy::too_many_arguments)]
    fn gather_coverages_for_subtable(&mut self, n: i32, n2: i32, n3: i32, n4: i32, map: &mut CoverageMap, map2: &mut CoverageMap, subset: &dyn Subset) -> FontResult<()>;
    fn get_extension_subtable_size(&self) -> i32;
    /// `map`, `subset` and `lookup_subset` are null when an extension subtable is copied as is.
    fn write_subtable(&mut self, n: i32, n2: i32, map: Option<&CoverageMap>, n3: i32, subset: Option<&dyn Subset>, lookup_subset: Option<&LookupSubset>) -> FontResult<i32>;
    fn patch_subtable_coverage(&mut self, n: i32, n2: i32, map: &CoverageMap, n3: i32) -> FontResult<()>;
    fn get_extension_lookup_type(&self) -> i32;
    fn write_extension_subtable(&mut self, n: i32, n2: i32) -> i32;
    fn patch_extension_offset(&mut self, n: i32, n2: i32);
    fn get_referenced_lookup_type(&self, n: i32) -> FontResult<i32>;
    fn get_referenced_subtable_offset(&self, n: i32) -> FontResult<i32>;
    fn must_keep_feature(&self, n: i32) -> FontResult<bool>;

    fn gather_coverages(&mut self, n: i32, map: &mut CoverageMap, map2: &mut CoverageMap, subset: &dyn Subset, list: &[i32]) -> FontResult<()> {
        let orig = self.base().orig_table;
        let n2 = orig.get_lookup_list_offset()?;
        let n3 = orig.data.get_offset(n2, 2 + 2 * n)?;
        for &n5 in list {
            let n4 = orig.data.getuint16(n3)?;
            let n6 = orig.data.get_offset(n3, 6 + 2 * n5)?;
            self.gather_coverages_for_subtable(n6, n4, n, n5, map, map2, subset)?;
        }
        Ok(())
    }

    fn move_coverages_for_lookup(&mut self, n: i32, map: &mut CoverageMap, map2: &mut CoverageMap) {
        let keys: Vec<i32> = map.keys().copied().collect();
        for n2 in keys {
            let nc2 = map.get_mut(&n2).unwrap();
            if !nc2.referring_orig_lookups.contains_key(&n) {
                continue;
            }
            let set = nc2.referring_orig_lookups.get(&n).unwrap().clone();
            if let Some(nc) = map2.get_mut(&n2) {
                nc.referring_orig_lookups.insert(n, set);
            } else {
                let mut nc = NewCoverage::new();
                nc.glyph_count = nc2.glyph_count;
                nc.new_coverage = nc2.new_coverage.clone();
                nc.referring_orig_lookups.insert(n, set);
                map2.insert(n2, nc);
            }
            map.get_mut(&n2).unwrap().referring_orig_lookups.remove(&n);
        }
    }

    fn get_lookup_offset(&self, n: i32, n2: i32) -> FontResult<i32> {
        self.base().orig_table.data.get_offset(n, 2 + 2 * n2)
    }

    fn patch_coverages(&mut self, n: i32, n2: i32, list: &[i32], list2: &[i32], map: &CoverageMap) -> FontResult<()> {
        let orig = self.base().orig_table;
        let n3 = self.get_lookup_offset(n, n2)?;
        let n4 = orig.data.getuint16(n3)?;
        for i in 0..list.len() {
            let n5 = list[i];
            self.patch_subtable_coverage(orig.data.get_offset(n3, 6 + 2 * n5)?, list2[i], map, n4)?;
        }
        Ok(())
    }

    fn patch_extension_coverages(&mut self, n: i32, list: &[i32], list2: &[i32], map: &CoverageMap) -> FontResult<()> {
        let orig = self.base().orig_table;
        for i in 0..list.len() {
            let n2 = list[i];
            let mut n3 = orig.data.get_offset(n, 6 + 2 * n2)?;
            let mut n4 = orig.data.getuint16(n)?;
            if n4 == self.get_extension_lookup_type() {
                n4 = self.get_referenced_lookup_type(n3)?;
                n3 = self.get_referenced_subtable_offset(n3)?;
            }
            self.patch_subtable_coverage(n3, list2[i], map, n4)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_lookup(&mut self, n: i32, map: &CoverageMap, n2: i32, list: &[i32], subset: &dyn Subset, list2: &mut Vec<i32>, ls: &LookupSubset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let n3 = list.len() as i32;
        let mut n4 = 6 + 2 * n3;
        self.base_mut().builder.ensure_capacity(n + n4);
        let n5 = orig.data.getuint16(n2)?;
        let b = &mut self.base_mut().builder;
        b.setuint16(n, n5);
        b.setuint16(n + 2, orig.data.getuint16(n2 + 2)?);
        b.setuint16(n + 4, n3);
        for i in 0..n3 {
            let n6 = list[i as usize];
            self.base_mut().builder.setuint16(n + 6 + 2 * i, n4);
            list2.insert(i as usize, n + n4);
            n4 += self.write_subtable(orig.data.get_offset(n2, 6 + 2 * n6)?, n + n4, Some(map), n5, Some(subset), Some(ls))?;
        }
        Ok(n4)
    }

    fn write_lookup_as_extension(&mut self, n: i32, n2: i32, list: &[i32], list2: &mut Vec<i32>) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let n3 = list.len() as i32;
        let mut n4 = 6 + 2 * n3;
        let n5 = orig.data.getuint16(n2)?;
        let ext = self.get_extension_lookup_type();
        let bl = n5 == ext;
        let flags = orig.data.getuint16(n2 + 2)?;
        let b = &mut self.base_mut().builder;
        b.setuint16(n, ext);
        b.setuint16(n + 2, flags);
        b.setuint16(n + 4, n3);
        for i in 0..n3 {
            let n6 = list[i as usize];
            self.base_mut().builder.setuint16(n + 6 + 2 * i, n4);
            list2.insert(i as usize, n + n4);
            if bl {
                n4 += self.write_subtable(orig.data.get_offset(n2, 6 + 2 * n6)?, n + n4, None, n5, None, None)?;
                continue;
            }
            n4 += self.write_extension_subtable(n + n4, n5);
        }
        Ok(n4)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_extension_subtables(&mut self, n: i32, list: &mut [i32], map: &mut CoverageMap, n2: i32, list2: &[i32], subset: &dyn Subset, ls: &LookupSubset, n3: i32, n4: i32) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let mut n5 = 0;
        for i in 0..list2.len() {
            let n6 = list[i];
            let mut n7 = orig.data.get_offset(n2, 6 + 2 * list2[i])?;
            self.patch_extension_offset(n6, n + n5);
            list[i] = n + n5;
            let mut n8 = n3;
            if n3 == self.get_extension_lookup_type() {
                n8 = self.get_referenced_lookup_type(n7)?;
                n7 = self.get_referenced_subtable_offset(n7)?;
            }
            n5 += self.write_subtable(n7, n + n5, Some(map), n8, Some(subset), Some(ls))?;
            let keys: Vec<i32> = map.keys().copied().collect();
            for n9 in keys {
                let nc = map.get(&n9).unwrap();
                if nc.get_max_referring_orig_lookup() != n4 || nc.get_max_subtable(n4) != list2[i] {
                    continue;
                }
                let cov = nc.new_coverage.clone();
                map.get_mut(&n9).unwrap().new_offset = n + n5;
                n5 += write_byte_array_at_offset(&mut self.base_mut().builder, n + n5, &cov)?;
            }
        }
        Ok(n5)
    }

    fn write_dummy_lookup(&mut self, n: i32) -> i32 {
        let b = &mut self.base_mut().builder;
        b.ensure_capacity(n + 6);
        b.setuint16(n, 0);
        b.setuint16(n + 2, 0);
        b.setuint16(n + 2, 0);
        6
    }

    fn build_lookup_list(&mut self, n: i32, n2: i32, ls: &LookupSubset, subset: &dyn Subset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let dummy = self.base().dummy_lookup_id;
        let num = ls.get_num_glyphs();
        let mut n9 = 2 + 2 * num + if dummy == -1 { 0 } else { 2 };
        let mut hash_map: CoverageMap = JHashMap::new();
        let mut hash_map2: CoverageMap = JHashMap::new();
        self.base_mut().builder.ensure_capacity(n2 + n9);
        self.base_mut().builder.setuint16(n2, num + if dummy == -1 { 0 } else { 1 });
        let mut n10 = 0;
        for n8 in 0..num {
            let list = ls.get_subtables(ls.get_full_gid(n8));
            n10 += list.len() as i32;
            self.gather_coverages(ls.get_full_gid(n8), &mut hash_map, &mut hash_map2, subset, list)?;
        }
        let mut n8 = 0;
        for nc in hash_map.values() {
            n8 += nc.new_coverage.get_size();
        }
        let mut n11 = 0;
        let mut n12 = 0;
        let mut list_array: Vec<Vec<i32>> = vec![Vec::new(); num.max(0) as usize];
        let mut array_list: Vec<i32> = Vec::new();
        let ext = self.get_extension_lookup_type();
        let ext_size = self.get_extension_subtable_size();
        let mut n7 = 0;
        while n7 < num {
            list_array[n7 as usize] = Vec::new();
            let list = ls.get_subtables(ls.get_full_gid(n7));
            let n6 = self.get_lookup_offset(n, ls.get_full_gid(n7))?;
            self.base_mut().builder.setuint16(n2 + 2 + 2 * n7, n9);
            if orig.data.getuint16(n6)? == ext {
                array_list.push(n7);
            }
            let mut la = std::mem::take(&mut list_array[n7 as usize]);
            let n5 = self.write_lookup(n2 + n9, &hash_map, n6, list, subset, &mut la, ls)?;
            list_array[n7 as usize] = la;
            let n4 = list.len() as i32;
            let n3 = n12 + n5 + (num - (n7 + 1)) * 6 + if dummy == -1 { 0 } else { 6 } + (n10 - (n11 + n4)) * (ext_size + 2) + n8;
            if n3 > MAX_OFFSET {
                break;
            }
            n11 += n4;
            n12 += n5;
            n9 += n5;
            n7 += 1;
        }
        if n7 < num {
            while n7 < num {
                list_array[n7 as usize] = Vec::new();
                let n13 = self.get_lookup_offset(n, ls.get_full_gid(n7))?;
                let list = ls.get_subtables(ls.get_full_gid(n7));
                self.base_mut().builder.setuint16(n2 + 2 + 2 * n7, n9);
                let mut la = Vec::new();
                n9 += self.write_lookup_as_extension(n2 + n9, n13, list, &mut la)?;
                list_array[n7 as usize] = la;
                array_list.push(n7);
                self.move_coverages_for_lookup(ls.get_full_gid(n7), &mut hash_map, &mut hash_map2);
                n7 += 1;
            }
        }
        let keys: Vec<i32> = hash_map.keys().copied().collect();
        for n14 in keys {
            let nc = hash_map.get_mut(&n14).unwrap();
            if nc.referring_orig_lookups.is_empty() {
                continue;
            }
            nc.new_offset = n2 + n9;
            let cov = nc.new_coverage.clone();
            n9 += write_byte_array_at_offset(&mut self.base_mut().builder, n2 + n9, &cov)?;
        }
        for n7 in 0..list_array.len() as i32 {
            if array_list.contains(&n7) {
                continue;
            }
            let la = list_array[n7 as usize].clone();
            self.patch_coverages(n, ls.get_full_gid(n7), ls.get_subtables(ls.get_full_gid(n7)), &la, &hash_map)?;
        }
        let dummy = self.base().dummy_lookup_id;
        if dummy != -1 {
            self.base_mut().builder.setuint16(n2 + 2 + 2 * dummy, n9);
            n9 += self.write_dummy_lookup(n2 + n9);
        }
        for k in 0..array_list.len() {
            let n6 = array_list[k];
            let list = ls.get_subtables(ls.get_full_gid(n6));
            let n4 = self.get_lookup_offset(n, ls.get_full_gid(n6))?;
            let n3 = orig.data.getuint16(n4)?;
            let mut la = std::mem::take(&mut list_array[n6 as usize]);
            let r = self.write_extension_subtables(n2 + n9, &mut la, &mut hash_map2, n4, list, subset, ls, n3, ls.get_full_gid(n6));
            list_array[n6 as usize] = la;
            n9 += r?;
        }
        for k in 0..array_list.len() {
            let n6 = array_list[k];
            let n5 = self.get_lookup_offset(n, ls.get_full_gid(n6))?;
            let list = ls.get_subtables(ls.get_full_gid(n6));
            let la = list_array[n6 as usize].clone();
            self.patch_extension_coverages(n5, list, &la, &hash_map2)?;
        }
        Ok(n9)
    }

    /// `subsetAndStream(TreeMap, Subset)`: returns the finished builder.
    fn subset_and_stream(&mut self, tree_map: &BTreeMap<i32, Vec<i32>>, subset: &dyn Subset) -> FontResult<OTByteArrayBuilder> {
        let orig = self.base().orig_table;
        let n = orig.data.get_offset(0, 6)?;
        let ls = LookupSubset::new(tree_map, orig.data.getuint16(orig.get_lookup_list_offset()?)?)?;
        let fs = FeatureSubset::new(&|x| self.must_keep_feature(x), &orig.data, n, tree_map)?;
        self.base_mut().builder.setuint32(0, 65536);
        let mut n2 = orig.data.get_offset(0, 4)?;
        let mut n3 = 10;
        self.base_mut().builder.setuint16(4, n3);
        n3 += self.build_script_list(n2, n3, &fs, &ls)?;
        self.base_mut().builder.setuint16(6, n3);
        n3 += self.build_feature_list(n, n3, &fs, &ls)?;
        n2 = orig.data.get_offset(0, 8)?;
        self.base_mut().builder.setuint16(8, n3);
        self.build_lookup_list(n2, n3, &ls, subset)?;
        Ok(std::mem::take(&mut self.base_mut().builder))
    }

    fn build_script_list(&mut self, n: i32, n2: i32, fs: &FeatureSubset, ls: &LookupSubset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let n3 = orig.data.getuint16(n)?;
        let mut n4 = 2;
        n4 += n3 * 6;
        self.base_mut().builder.ensure_capacity(n2 + n4);
        self.base_mut().builder.setuint16(n2, n3);
        for i in 0..n3 {
            for j in 0..4 {
                let v = orig.data.getint8(n + 2 + 6 * i + j)?;
                self.base_mut().builder.setuint8(n2 + 2 + 6 * i + j, v);
            }
            self.base_mut().builder.setuint16(n2 + 2 + 6 * i + 4, n4);
            n4 += self.write_script(orig.data.get_offset(n, 2 + 6 * i + 4)?, fs, n2 + n4, ls)?;
        }
        Ok(n4)
    }

    fn write_script(&mut self, n: i32, fs: &FeatureSubset, n2: i32, ls: &LookupSubset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let mut n3 = 0;
        let n4 = orig.data.getuint16(n + 2)?;
        n3 += 4 + n4 * 6;
        self.base_mut().builder.ensure_capacity(n2 + n3);
        self.base_mut().builder.setuint16(n2 + 2, n4);
        let n5 = orig.data.get_offset(n, 0)?;
        if n5 != 0 {
            self.base_mut().builder.setuint16(n2, n3);
            n3 += self.write_lang_sys(n5, fs, n2 + n3, ls)?;
        } else {
            self.base_mut().builder.setuint16(n2, 0);
        }
        for i in 0..n4 {
            self.base_mut().builder.setuint16(n2 + 4 + 6 * i + 4, n3);
            for j in 0..4 {
                let v = orig.data.getint8(n + 4 + 6 * i + j)?;
                self.base_mut().builder.setuint8(n2 + 4 + 6 * i + j, v);
            }
            n3 += self.write_lang_sys(orig.data.get_offset(n, 4 + 6 * i + 4)?, fs, n2 + n3, ls)?;
        }
        Ok(n3)
    }

    fn write_lang_sys(&mut self, n: i32, fs: &FeatureSubset, n2: i32, _ls: &LookupSubset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let n5 = orig.data.getuint16(n + 4)?;
        let mut array_list: Vec<i32> = Vec::new();
        let mut n6 = 0;
        for n4 in 0..n5 {
            let n3 = orig.data.getuint16(n + 6 + 2 * n4)?;
            if fs.get_existing_subset_gid(n3)? == -1 {
                continue;
            }
            array_list.push(n4);
        }
        n6 += 6 + 2 * array_list.len() as i32;
        self.base_mut().builder.ensure_capacity(n2 + n6);
        self.base_mut().builder.setuint16(n2, 0);
        let n4 = orig.data.getuint16(n + 2)?;
        if n4 != 65535 && fs.get_existing_subset_gid(n4)? != -1 {
            let v = fs.get_existing_subset_gid(n4)?;
            self.base_mut().builder.setuint16(n2 + 2, v);
        } else {
            self.base_mut().builder.setuint16(n2 + 2, 65535);
        }
        self.base_mut().builder.setuint16(n2 + 4, array_list.len() as i32);
        for (n3, &n7) in array_list.iter().enumerate() {
            let n8 = orig.data.getuint16(n + 6 + 2 * n7)?;
            let n9 = fs.get_existing_subset_gid(n8)?;
            self.base_mut().builder.setuint16(n2 + 6 + 2 * n3 as i32, n9);
        }
        Ok(n6)
    }

    fn build_feature_list(&mut self, n: i32, n2: i32, fs: &FeatureSubset, ls: &LookupSubset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let mut n3 = 2 + 6 * fs.get_num_glyphs();
        self.base_mut().builder.ensure_capacity(n2 + n3);
        self.base_mut().builder.setuint16(n2, fs.get_num_glyphs());
        for i in 0..fs.get_num_glyphs() {
            let n4 = fs.get_full_gid(i);
            for j in 0..4 {
                let v = orig.data.getint8(n + 2 + 6 * n4 + j)?;
                self.base_mut().builder.setuint8(n2 + 2 + 6 * i + j, v);
            }
            self.base_mut().builder.setuint16(n2 + 2 + 6 * i + 4, n3);
            n3 += self.write_feature(orig.data.get_offset(n, 2 + 6 * n4 + 4)?, n2 + n3, ls)?;
        }
        Ok(n3)
    }

    fn write_feature(&mut self, n: i32, n2: i32, ls: &LookupSubset) -> FontResult<i32> {
        let orig = self.base().orig_table;
        let mut n5 = 0;
        let n6 = orig.data.getuint16(n + 2)?;
        let mut n7 = 0;
        for n4 in 0..n6 {
            let n3 = orig.data.getuint16(n + 4 + 2 * n4)?;
            if ls.get_existing_subset_gid(n3)? == -1 {
                continue;
            }
            n7 += 1;
        }
        if n6 > 0 && n7 == 0 {
            n7 += 1;
            if self.base().dummy_lookup_id == -1 {
                self.base_mut().dummy_lookup_id = ls.get_num_glyphs();
            }
        }
        n5 += 4 + 2 * n7;
        self.base_mut().builder.ensure_capacity(n2 + n5);
        self.base_mut().builder.setuint16(n2, 0);
        self.base_mut().builder.setuint16(n2 + 2, n7);
        let mut n4 = 0;
        for n3 in 0..n6 {
            let n8 = orig.data.getuint16(n + 4 + 2 * n3)?;
            let n9 = ls.get_existing_subset_gid(n8)?;
            if n9 == -1 {
                continue;
            }
            self.base_mut().builder.setuint16(n2 + 4 + 2 * n4, n9);
            n4 += 1;
        }
        // Java tests the incremented count, so the dummy lookup index is never written here.
        if n6 > 0 && n7 == 0 {
            let d = self.base().dummy_lookup_id;
            self.base_mut().builder.setuint16(n2 + 4, d);
        }
        Ok(n5)
    }
}
