//! Port of afe.jar `com.adobe.fontengine.font.type1.FontFactory`: the format sniffing only (Type 1 fonts are not ported).

use crate::asc::flex2::fonts::adobe::fontengine::font::postscript::sub_arrays::array_compare;

const K_TYPE1_PFA_START1: &[u8] = b"%!PS-AdobeFont";
const K_TYPE1_PFA_START2: &[u8] = b"%!FontType1";
const K_TYPE1_PFA_START3: &[u8] = b"%!FontType 1";
const K_AFM_START: &[u8] = b"StartFontMetrics";
const K_TYPE1_PFB_START: &[u8] = &[0x80, 1];

pub struct FontFactory;

impl FontFactory {
    pub fn get_num_bytes_needed_to_identify() -> i32 {
        6 + K_TYPE1_PFA_START1.len() as i32
    }

    fn starts_with_type1(b: &[u8], n: i32) -> bool {
        array_compare(K_TYPE1_PFA_START1, 0, b, n, K_TYPE1_PFA_START1.len() as i32)
            || array_compare(K_TYPE1_PFA_START2, 0, b, n, K_TYPE1_PFA_START2.len() as i32)
            || array_compare(K_TYPE1_PFA_START3, 0, b, n, K_TYPE1_PFA_START3.len() as i32)
    }

    fn is_pfa(b: &[u8]) -> bool {
        Self::starts_with_type1(b, 0)
    }

    fn is_pfb(b: &[u8]) -> bool {
        if array_compare(K_TYPE1_PFB_START, 0, b, 0, K_TYPE1_PFB_START.len() as i32) {
            return Self::starts_with_type1(b, 6);
        }
        false
    }

    pub fn is_type1(b: &[u8]) -> bool {
        Self::is_pfb(b) || Self::is_pfa(b)
    }

    pub fn get_num_bytes_needed_to_identify_afm() -> i32 {
        K_AFM_START.len() as i32
    }

    pub fn is_afm(b: &[u8]) -> bool {
        if (b.len() as i32) < Self::get_num_bytes_needed_to_identify_afm() {
            return false;
        }
        array_compare(b, 0, K_AFM_START, 0, K_AFM_START.len() as i32)
    }

    pub fn get_num_bytes_needed_to_identify_pfm() -> i32 {
        68
    }

    pub fn is_pfm(b: &[u8]) -> bool {
        if (b.len() as i32) < Self::get_num_bytes_needed_to_identify_pfm() {
            return false;
        }
        b[0] == 0 && b[1] == 1 && b[66] == 129 && b[67] == 0 && ((b[3] as i32) << 8 | b[2] as i32) >= Self::get_num_bytes_needed_to_identify()
    }
}
