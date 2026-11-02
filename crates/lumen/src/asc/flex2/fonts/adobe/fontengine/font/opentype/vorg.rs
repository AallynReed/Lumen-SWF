//! Port of afe.jar `com.adobe.fontengine.font.opentype.Vorg`.

use std::collections::BTreeMap;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::data_as_byte_array;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::vertical_origin_fetcher::VerticalOriginFetcher;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[derive(Clone, Debug, Default)]
pub struct Vorg {
    pub data: OTByteArray,
}

impl Vorg {
    pub fn new(a: FontByteArray) -> Vorg {
        Vorg { data: OTByteArray::from_font_byte_array(a) }
    }

    pub fn get_default_v_origin(&self) -> FontResult<i32> {
        self.data.getint16(4)
    }

    pub fn subset_and_stream(subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>, fetcher: &dyn VerticalOriginFetcher) -> FontResult<()> {
        let n4 = subset.get_num_glyphs();
        let mut a = vec![0i32; n4.max(0) as usize];
        for n3 in 0..n4 {
            let n2 = subset.get_full_gid(n3);
            a[n3 as usize] = fetcher.get_v_origin(n2)?;
        }
        let a2 = a.clone();
        a.sort_unstable();
        let mut n3 = a[0];
        let mut n2 = 1;
        let mut n5 = 1;
        for n in 1..a.len() {
            if a[n] == a[n - 1] {
                n5 += 1;
                if n5 <= n2 {
                    continue;
                }
                n2 = n5;
                n3 = a[n];
                continue;
            }
            n5 = 1;
        }
        let n = a.iter().filter(|&&v| v != n3).count() as i32;
        let mut b = OTByteArray::get_ot_byte_array_builder_instance_sized(8 + 4 * n);
        b.setuint16(0, 1);
        b.setuint16(2, 0);
        b.setint16(4, n3);
        b.setuint16(6, n);
        let mut n6 = 0;
        let mut i = 0;
        while i < a2.len() && n6 < n {
            if a2[i] != n3 {
                b.setuint16(8 + n6 * 4, i as i32);
                b.setuint16(8 + n6 * 4 + 2, a2[i]);
                n6 += 1;
            }
            i += 1;
        }
        map.insert(1448038983, b);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(1448038983, data_as_byte_array(&self.data));
    }
}

impl VerticalOriginFetcher for Vorg {
    fn get_v_origin(&self, n: i32) -> FontResult<i32> {
        let mut n2 = self.data.getuint16(6)?;
        let mut n3 = 0;
        while n3 < n2 {
            let n4 = (n3 + n2) / 2;
            let n5 = self.data.getuint16(8 + 4 * n4)?;
            if n < n5 {
                n2 = n4;
                continue;
            }
            if n5 < n {
                n3 = n4 + 1;
                continue;
            }
            return self.data.getint16(8 + 4 * n4 + 2);
        }
        self.get_default_v_origin()
    }
}
