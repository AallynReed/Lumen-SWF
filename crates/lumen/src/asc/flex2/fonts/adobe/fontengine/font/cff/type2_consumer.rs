//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2Consumer`, with `Type2ConsumerDefaultImpl`'s empty bodies as defaults.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::CFFByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

#[allow(unused_variables)]
pub trait Type2Consumer {
    fn hstem(&mut self, stack: &[f64], n: i32) {}
    fn vstem(&mut self, stack: &[f64], n: i32) {}
    fn vmoveto(&mut self, stack: &[f64], n: i32) {}
    fn rlineto(&mut self, stack: &[f64], n: i32) {}
    fn hlineto(&mut self, stack: &[f64], n: i32) {}
    fn vlineto(&mut self, stack: &[f64], n: i32) {}
    fn rrcurveto(&mut self, stack: &[f64], n: i32) {}
    fn callsubr(&mut self, stack: &[f64], n: i32, n2: i32) {}
    fn return_op(&mut self, stack: &[f64], n: i32) {}
    fn and(&mut self, stack: &[f64], n: i32) {}
    fn or(&mut self, stack: &[f64], n: i32) {}
    fn not(&mut self, stack: &[f64], n: i32) {}
    fn abs(&mut self, stack: &[f64], n: i32) {}
    fn add(&mut self, stack: &[f64], n: i32) {}
    fn sub(&mut self, stack: &[f64], n: i32) {}
    fn div(&mut self, stack: &[f64], n: i32) {}
    fn neg(&mut self, stack: &[f64], n: i32) {}
    fn eq(&mut self, stack: &[f64], n: i32) {}
    fn drop(&mut self, stack: &[f64], n: i32) {}
    fn put(&mut self, stack: &[f64], n: i32) {}
    fn get(&mut self, stack: &[f64], n: i32) {}
    fn ifelse(&mut self, stack: &[f64], n: i32) {}
    fn random(&mut self, stack: &[f64], n: i32) {}
    fn mul(&mut self, stack: &[f64], n: i32) {}
    fn sqrt(&mut self, stack: &[f64], n: i32) {}
    fn dup(&mut self, stack: &[f64], n: i32) {}
    fn exch(&mut self, stack: &[f64], n: i32) {}
    fn index(&mut self, stack: &[f64], n: i32) {}
    fn roll(&mut self, stack: &[f64], n: i32) {}
    fn hflex(&mut self, stack: &[f64], n: i32) {}
    fn flex(&mut self, stack: &[f64], n: i32) {}
    fn hflex1(&mut self, stack: &[f64], n: i32) {}
    fn flex1(&mut self, stack: &[f64], n: i32) {}
    fn endchar(&mut self, stack: &[f64], n: i32) {}
    fn hstemhm(&mut self, stack: &[f64], n: i32) {}
    fn hintmask(&mut self, stack: &[f64], n: i32, data: &CFFByteArray, n2: i32, n3: i32) -> FontResult<()> {
        Ok(())
    }
    fn cntrmask(&mut self, stack: &[f64], n: i32, data: &CFFByteArray, n2: i32, n3: i32) -> FontResult<()> {
        Ok(())
    }
    fn rmoveto(&mut self, stack: &[f64], n: i32) {}
    fn hmoveto(&mut self, stack: &[f64], n: i32) {}
    fn moveto(&mut self, stack: &[f64], n: i32) {}
    fn vstemhm(&mut self, stack: &[f64], n: i32) {}
    fn implicit_vstemhm(&mut self, stack: &[f64], n: i32) {}
    fn rcurveline(&mut self, stack: &[f64], n: i32) {}
    fn rlinecurve(&mut self, stack: &[f64], n: i32) {}
    fn vvcurveto(&mut self, stack: &[f64], n: i32) {}
    fn hhcurveto(&mut self, stack: &[f64], n: i32) {}
    fn callgsubr(&mut self, stack: &[f64], n: i32, n2: i32) {}
    fn vhcurveto(&mut self, stack: &[f64], n: i32) {}
    fn hvcurveto(&mut self, stack: &[f64], n: i32) {}
    fn integer(&mut self, stack: &[f64], n: i32, n2: i32) {}
    fn real(&mut self, stack: &[f64], n: i32, d: f64) {}
    fn width(&mut self, d: f64) -> bool {
        true
    }
    fn seac(&mut self, stack: &[f64], n: i32) -> bool {
        true
    }
    fn global_color_me(&mut self, stack: &[f64], n: i32) {}
}
