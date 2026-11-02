//! Port of afe.jar `com.adobe.fontengine.font.cff.Index`.

use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug)]
pub struct Index {
    pub data: Rc<CFFByteArray>,
    pub offset: i32,
    pub size: i32,
    pub entry_count: i32,
    pub entry_offsets: Vec<i32>,
}

impl Index {
    pub fn new(data: Rc<CFFByteArray>, n: i32) -> FontResult<Index> {
        let entry_count = data.getcard16(n)?;
        let mut entry_offsets = vec![0; (entry_count + 1) as usize];
        if entry_count == 0 {
            return Ok(Index { data, offset: n, size: 2, entry_count, entry_offsets });
        }
        let n2 = data.get_off_size(n + 2)?;
        let n3 = n + 3 + n2 * (entry_count + 1);
        for i in 0..entry_count + 1 {
            entry_offsets[i as usize] = n3 + data.get_offset(n + 3 + n2 * i, n2, "INDEX offset too big")? - 1;
        }
        let size = entry_offsets[entry_count as usize] - n;
        Ok(Index { data, offset: n, size, entry_count, entry_offsets })
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn get_count(&self) -> i32 {
        self.entry_count
    }

    pub fn offset_of(&self, n: i32) -> i32 {
        self.entry_offsets[n as usize]
    }

    pub fn offset_following(&self, n: i32) -> i32 {
        self.entry_offsets[(n + 1) as usize]
    }

    pub fn size_of(&self, n: i32) -> i32 {
        self.entry_offsets[(n + 1) as usize] - self.entry_offsets[n as usize]
    }

    /// `offsetOf`/`offsetFollowing` with Java's `ArrayIndexOutOfBoundsException` as `None`.
    pub fn checked_range(&self, n: i32) -> Option<(i32, i32)> {
        if n < 0 || (n + 1) as usize >= self.entry_offsets.len() {
            return None;
        }
        Some((self.entry_offsets[n as usize], self.entry_offsets[(n + 1) as usize]))
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder) -> FontResult<()> {
        b.add_bytes_from(&self.data, self.offset, self.size)
    }

    pub fn start_index(b: &mut CFFByteArrayBuilder, n: i32) -> Option<Cursor> {
        if n == 0 {
            b.add_card16(0);
            return None;
        }
        b.add_card16(n);
        b.add_card8(4);
        b.add_card32(1);
        let offset = b.get_size();
        for _ in 0..n {
            b.add_card32(1);
        }
        Some(Cursor { base: b.get_size() - 1, offset })
    }

    pub fn element_entered(b: &mut CFFByteArrayBuilder, cursor: Option<Cursor>) -> Option<Cursor> {
        let mut c = cursor.expect("Index.elementEntered on an empty index");
        let v = b.get_size() - c.base;
        b.set_card32(c.offset, v);
        c.offset += 4;
        Some(c)
    }
}

/// `Index.Cursor`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cursor {
    pub base: i32,
    pub offset: i32,
}
