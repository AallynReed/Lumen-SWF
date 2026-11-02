//! Port of afe.jar `com.adobe.fontengine.font.SubsetDefaultImpl`.

use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

#[derive(Clone, Debug)]
pub struct SubsetDefaultImpl {
    pub do_subset: bool,
    pub subset_glyph_count: i32,
    pub full_gid2subset_gid: Vec<i32>,
    pub subset_gid2full_gid: Vec<i32>,
}

impl SubsetDefaultImpl {
    pub fn new(num_glyphs: i32, do_subset: bool) -> SubsetDefaultImpl {
        if !do_subset {
            SubsetDefaultImpl { do_subset, subset_glyph_count: num_glyphs, full_gid2subset_gid: Vec::new(), subset_gid2full_gid: Vec::new() }
        } else {
            let n = num_glyphs.max(0) as usize;
            SubsetDefaultImpl { do_subset, subset_glyph_count: 0, full_gid2subset_gid: vec![-1; n], subset_gid2full_gid: vec![0; n] }
        }
    }

    pub fn do_subset(&self) -> bool {
        self.do_subset
    }

    /// The body of `getSubsetGid` with `pullComponentGlyphs` left to the caller: returns `(subset gid, newly added)`.
    pub fn add_gid(&mut self, n: i32) -> FontResult<(i32, bool)> {
        if !self.do_subset {
            return Ok((n, false));
        }
        let aioobe = |i: i32, len: usize| FontEngineException::from_cause(ExceptionKind::InvalidFont, format!("java.lang.ArrayIndexOutOfBoundsException: Index {} out of bounds for length {}", i, len));
        if n < 0 || n as usize >= self.full_gid2subset_gid.len() {
            return Err(aioobe(n, self.full_gid2subset_gid.len()));
        }
        let mut added = false;
        if self.full_gid2subset_gid[n as usize] == -1 {
            let c = self.subset_glyph_count;
            if c as usize >= self.subset_gid2full_gid.len() {
                return Err(aioobe(c, self.subset_gid2full_gid.len()));
            }
            self.full_gid2subset_gid[n as usize] = c;
            self.subset_gid2full_gid[c as usize] = n;
            self.subset_glyph_count += 1;
            added = true;
        }
        Ok((self.full_gid2subset_gid[n as usize], added))
    }
}

impl Subset for SubsetDefaultImpl {
    fn get_num_glyphs(&self) -> i32 {
        self.subset_glyph_count
    }

    fn get_subset_gid(&mut self, n: i32) -> FontResult<i32> {
        Ok(self.add_gid(n)?.0)
    }

    fn get_existing_subset_gid(&self, n: i32) -> FontResult<i32> {
        if self.do_subset {
            if n as usize >= self.full_gid2subset_gid.len() {
                return Ok(-1);
            }
            return Ok(self.full_gid2subset_gid[n as usize]);
        }
        Ok(n)
    }

    fn get_full_gid(&self, n: i32) -> i32 {
        if self.do_subset {
            self.subset_gid2full_gid[n as usize]
        } else {
            n
        }
    }
}
