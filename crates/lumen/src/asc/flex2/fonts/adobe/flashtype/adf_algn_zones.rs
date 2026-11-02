//! Port of flex-fontkit.jar `flash.fonts.flashtype.ADFAlgnZones`.

use crate::asc::flex2::fonts::adobe::flashtype::adf::{adf_generate_adf, adf_pack_algn_zones, adf_reconstruct_dist_abs, ADFGlyph};
use crate::asc::flex2::fonts::adobe::flashtype::adf_math_utils::adf_max;
use crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::{ADFGenAttrs, ADFPath};

pub const AZ_ADF_MAXLEVEL: i32 = 5;
pub const AZ_ADF_MAXERROR: f32 = 2.0E-4;
pub const AZ_EDGE_THRESHOLD: f32 = 0.04;
pub const AZ_MAX_NSAMPLES: i32 = 220;
pub const AZ_MIN_NSAMPLES: i32 = 100;
pub const AZ_RANGE_CUTOFF: f32 = 0.13;
pub const AZ_MIN_CHAR_DIST_PCT: f32 = 0.7;
pub const AZ_LEFT_EDGE_CROSSING: i32 = 0;
pub const AZ_TOP_EDGE_CROSSING: i32 = 1;

#[allow(clippy::too_many_arguments)]
pub fn detect_edges(g: Option<&ADFGlyph>, n: i32, n2: i32, n3: i32, f: f32, f_array: &mut [f32], n_array: &mut [i32], int_value: &mut i32, f_array2: &mut [f32]) {
    let mut f_array3 = [0.0f32; 3];
    let n6 = adf_max(n2 as f32, n3 as f32) as i32;
    let g = match g {
        None => {
            *int_value = 0;
            return;
        }
        Some(g) => g,
    };
    let mut f_array4 = vec![0.0f32; (n2 * n3) as usize];
    let f2 = 1.0 / n2 as f32;
    let f3 = 1.0 / n3 as f32;
    let mut n7 = (g.glyph_min_x * n2 as f32) as i32;
    let mut n8 = (g.glyph_max_x * n2 as f32) as i32;
    let mut n9 = (g.glyph_min_y * n3 as f32) as i32;
    let mut n10 = (g.glyph_max_y * n3 as f32) as i32;
    n7 = if n7 < 2 { 0 } else { n7 - 2 };
    n9 = if n9 < 2 { 0 } else { n9 - 2 };
    n8 = if n8 > n2 - 3 { n2 - 1 } else { n8 + 2 };
    n10 = if n10 > n3 - 3 { n3 - 1 } else { n10 + 2 };
    let mut n11 = 0usize;
    for n5 in 0..n3 {
        if n5 < n9 || n5 > n10 {
            for _ in 0..n2 {
                f_array[n11] = -1.0;
                n11 += 1;
            }
            continue;
        }
        f_array3[1] = n5 as f32 * f3;
        for n4 in 0..n2 {
            if n4 < n7 || n4 > n8 {
                f_array[n11] = -1.0;
                n11 += 1;
                continue;
            }
            f_array3[0] = n4 as f32 * f2;
            let f4 = adf_reconstruct_dist_abs(g, &f_array3);
            f_array[n11] = f4;
            n11 += 1;
        }
    }
    let at = |n4: i32, n5: i32| (n4 + n5 * n2) as usize;
    match n {
        0 => {
            let l = (n10 - n9 + 1) as i64;
            let mut f5 = l as f32 * 2.0 * f2;
            if f5 != 0.0 {
                f5 = 1.0 / f5;
            }
            let mut n4 = n7 + 1;
            while n4 < n8 {
                for n5 in n9 + 1..n10 {
                    let f8 = f_array[at(n4 - 1, n5)];
                    let f9 = f_array[at(n4 + 1, n5)];
                    if !(f8 * f9 < 0.0) {
                        continue;
                    }
                    let f6 = f_array[at(n4, n5)];
                    let f7 = -3.0 * f6.abs() + f9 - f8;
                    if !(f7 > 0.0) {
                        continue;
                    }
                    f_array4[n4 as usize] += f7;
                }
                f_array4[n4 as usize] *= f5;
                n4 += 1;
            }
        }
        1 => {
            let l = (n8 - n7 + 1) as i64;
            let mut f10 = l as f32 * 2.0 * f3;
            if f10 != 0.0 {
                f10 = 1.0 / f10;
            }
            for n4 in n7 + 1..n8 {
                for n5 in n9 + 1..n10 {
                    let f13 = f_array[at(n4, n5 + 1)];
                    let f14 = f_array[at(n4, n5 - 1)];
                    if !(f13 * f14 < 0.0) {
                        continue;
                    }
                    let f11 = f_array[at(n4, n5)];
                    let f12 = -3.0 * f11.abs() + f14 - f13;
                    if !(f12 > 0.0) {
                        continue;
                    }
                    f_array4[n5 as usize] += f12;
                }
            }
            let mut n5 = n9 + 1;
            while n5 < n10 {
                f_array4[n5 as usize] *= f10;
                n5 += 1;
            }
        }
        _ => {
            *int_value = 0;
            return;
        }
    }
    determine_edges(&f_array4, n6, f, n_array, f_array2, int_value);
}

pub fn determine_edges(f_array: &[f32], n: i32, f: f32, n_array: &mut [i32], f_array2: &mut [f32], int_value: &mut i32) {
    let mut n3 = 0i32;
    let mut n4 = 0usize;
    let mut f2 = 0.0f32;
    let f3 = 1.0 / n as f32;
    for n2 in 0..n {
        let f4 = f_array[n2 as usize];
        if f2 == 0.0 && f4 > f {
            n3 = n2;
            f2 = f4;
            continue;
        }
        if f2 == 0.0 {
            continue;
        }
        if f4 >= f2 {
            n3 = n2;
            f2 = f4;
            continue;
        }
        if !((n2 - n3) as f32 * f3 > 0.13) {
            continue;
        }
        n_array[n4] = n3;
        n4 += 1;
        f2 = 0.0;
    }
    if n4 as i32 <= *int_value {
        *int_value = n4 as i32;
    } else {
        for _ in 0..*int_value {
            let mut n6 = -1i32;
            let mut n7 = 0usize;
            for i in 0..n4 {
                let n5 = n_array[i];
                if n5 >= n {
                    continue;
                }
                if n6 < 0 {
                    n6 = n5;
                    n7 = i;
                    continue;
                }
                if !(f_array[n5 as usize] > f_array[n6 as usize]) {
                    continue;
                }
                n6 = n5;
                n7 = i;
            }
            n_array[n7] += n;
        }
        let mut n5 = 0usize;
        for n2 in 0..n4 {
            if n_array[n2] < n {
                continue;
            }
            n_array[n5] = n_array[n2] - n;
            n5 += 1;
        }
    }
    for n2 in 0..*int_value as usize {
        f_array2[n2] = f3 * n_array[n2] as f32;
    }
}

/// `get_outline` stands for the reflective `getFontOutline` callback; it is asked for 'Z', 'z', 'L' and 'l' in that order.
pub fn adf_init_algn_zone_detection(get_outline: &mut dyn FnMut(u16) -> ADFPath) -> AlgnZoneState {
    let mut f_array = [0.0f32; 2];
    let f = 0.04f32;
    let mut attrs = ADFGenAttrs::default();
    let mut st = AlgnZoneState::default();
    attrs.max_level = 5;
    attrs.max_error = 2.0E-4;
    attrs.dist_eps = 0.0;
    st.p_dist = vec![0.0; 22000];
    st.edge_pts_idx = vec![0; 220];

    let mut iv = 2i32;
    let path = get_outline('Z' as u16);
    let g = adf_generate_adf(Some(&path), &mut attrs);
    detect_edges(g.as_ref(), 1, 100, 220, f, &mut st.p_dist, &mut st.edge_pts_idx, &mut iv, &mut f_array);
    if iv == 2 {
        let g = g.as_ref().unwrap();
        st.cap_height = (f_array[1] - g.glyph_origin_y) / g.fu_to_adf_scale;
        if st.cap_height < 0.7 * (path.glyph_max_y - path.glyph_min_y) {
            st.cap_height = 0.0;
        }
    } else {
        st.cap_height = 0.0;
    }

    iv = 2;
    let path = get_outline('z' as u16);
    let g = adf_generate_adf(Some(&path), &mut attrs);
    detect_edges(g.as_ref(), 1, 100, 220, f, &mut st.p_dist, &mut st.edge_pts_idx, &mut iv, &mut f_array);
    if iv == 2 {
        let g = g.as_ref().unwrap();
        st.x_height = (f_array[1] - g.glyph_origin_y) / g.fu_to_adf_scale;
        if st.x_height < 0.7 * (path.glyph_max_y - path.glyph_min_y) {
            st.x_height = 0.0;
        }
    } else {
        st.x_height = 0.0;
    }

    iv = 1;
    let path = get_outline('L' as u16);
    let g = adf_generate_adf(Some(&path), &mut attrs);
    detect_edges(g.as_ref(), 0, 220, 100, f, &mut st.p_dist, &mut st.edge_pts_idx, &mut iv, &mut f_array);
    st.left_cap_align_pt = if iv == 1 {
        let g = g.as_ref().unwrap();
        (f_array[0] - g.glyph_origin_x) / g.fu_to_adf_scale
    } else {
        0.0
    };

    iv = 1;
    let path = get_outline('l' as u16);
    let g = adf_generate_adf(Some(&path), &mut attrs);
    detect_edges(g.as_ref(), 0, 220, 100, f, &mut st.p_dist, &mut st.edge_pts_idx, &mut iv, &mut f_array);
    st.left_x_align_pt = if iv == 1 {
        let g = g.as_ref().unwrap();
        (f_array[0] - g.glyph_origin_x) / g.fu_to_adf_scale
    } else {
        0.0
    };
    st
}

pub fn adf_detect_algn_zones(state: Option<&mut AlgnZoneState>, path: Option<&mut ADFPath>) {
    let mut f_array = [0.0f32; 10];
    let f5 = 0.04f32;
    let mut iv = 0i32;
    let mut iv2 = 0i32;
    let path = match path {
        None => return,
        Some(p) => p,
    };
    let st = match state {
        None => {
            path.algn_zones_mask = 0;
            return;
        }
        Some(s) => s,
    };
    let mut bv = false;
    set_glyph_specific_algn_zone_data(path.char_code, &mut bv, &mut iv, &mut iv2);
    let mut attrs = ADFGenAttrs { max_error: 2.0E-4, dist_eps: 0.0, max_level: if iv == 0 && iv2 == 0 { 0 } else { 5 } };
    let mut g = match adf_generate_adf(Some(path), &mut attrs) {
        None => {
            path.algn_zones_mask = 0;
            return;
        }
        Some(g) => g,
    };
    if iv != 0 {
        detect_edges(Some(&g), 0, 220, 100, f5, &mut st.p_dist, &mut st.edge_pts_idx, &mut iv, &mut f_array);
    }
    let (mut f4, mut f3);
    if iv == 0 {
        f4 = if bv { st.left_cap_align_pt } else { st.left_x_align_pt };
        f3 = 0.0;
    } else if iv == 1 {
        f4 = (f_array[0] - g.glyph_origin_x) / g.fu_to_adf_scale;
        f3 = 0.0;
    } else {
        f4 = (f_array[0] - g.glyph_origin_x) / g.fu_to_adf_scale;
        f3 = (f_array[1] - f_array[0]) / g.fu_to_adf_scale;
        if f3 * g.fu_to_adf_scale < 0.13 {
            f3 = 0.0;
        }
    }
    if iv2 != 0 {
        detect_edges(Some(&g), 1, 100, 220, f5, &mut st.p_dist, &mut st.edge_pts_idx, &mut iv2, &mut f_array);
    }
    let (mut f2, mut f);
    if iv2 == 0 {
        f2 = 0.0f32;
        f = if bv { st.cap_height } else { st.x_height };
    } else {
        f2 = 0.0;
        f = (f_array[(iv2 - 1) as usize] - g.glyph_origin_y) / g.fu_to_adf_scale;
        if f < 0.0 {
            f = -f;
        }
        if f * g.fu_to_adf_scale < 0.13 && path.char_code != '_' as u16 {
            f = 0.0;
        }
    }
    let f6 = 1.0 / path.font_units_per_em;
    f4 *= f6;
    g.x_algn_coord = f4;
    f2 *= f6;
    g.y_algn_coord = f2;
    f3 *= f6;
    g.x_range = f3;
    f *= f6;
    g.y_range = f;
    g.algn_zones_mask = 3;
    adf_pack_algn_zones(&g, path);
}

pub fn set_glyph_specific_algn_zone_data(c: u16, bv: &mut bool, iv: &mut i32, iv2: &mut i32) {
    *bv = true;
    *iv = 0;
    *iv2 = 0;
    let c = c as u32;
    let ch = |s: &str| s.chars().any(|x| x as u32 == c);
    let in_list = |l: &[u32]| l.contains(&c);
    // Java falls through from each "booleanValue = false" group into the next group.
    if ch("lsvwxyz") || in_list(&[0xff, 0x15b, 0x15d, 0x15f, 0x161, 0x175, 0x177, 0x17a, 0x17c, 0x17e]) {
        *bv = false;
        return;
    }
    if ch("&'/236789<>ALSVWXZ\\")
        || in_list(&[
            0xaa, 0xab, 0xb0, 0xb2, 0xb3, 0xb8, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xd7, 0x100, 0x102, 0x104, 0x15a, 0x15c,
            0x15e, 0x160, 0x174, 0x179, 0x17b, 0x17d, 0x1cd, 0x1fa,
        ])
    {
        return;
    }
    if ch("abcdefgijkpqrt")
        || in_list(&[
            0xa2, 0xde, 0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe7, 0xe8, 0xe9, 0xea, 0xeb, 0xec, 0xed, 0xee, 0xef, 0xf0, 0xfd, 0xfe, 0x101, 0x103, 0x105,
            0x107, 0x109, 0x10b, 0x10d, 0x10f, 0x111, 0x113, 0x115, 0x117, 0x119, 0x11b, 0x11d, 0x11f, 0x121, 0x123, 0x129, 0x12b, 0x12d, 0x12f, 0x131,
            0x135, 0x137, 0x138, 0x13a, 0x13c, 0x13e, 0x140, 0x142, 0x155, 0x157, 0x159, 0x163, 0x165, 0x167, 0x17f, 0x192, 0x1ce, 0x1d0, 0x1fb,
        ])
    {
        *bv = false;
        *iv = 1;
        return;
    }
    if ch("!$()*,.145?@CEFGIJKPRTY[]{|}")
        || in_list(&[
            0xa1, 0xa3, 0xa5, 0xa6, 0xa9, 0xae, 0xb4, 0xb6, 0xb9, 0xbf, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xcb, 0xcc, 0xcd, 0xce, 0xcf, 0xdd, 0x106, 0x108,
            0x10a, 0x10c, 0x112, 0x114, 0x116, 0x118, 0x11a, 0x11c, 0x11e, 0x120, 0x122, 0x128, 0x12a, 0x12c, 0x12e, 0x130, 0x134, 0x136, 0x139, 0x13b,
            0x13d, 0x13f, 0x141, 0x154, 0x156, 0x158, 0x162, 0x164, 0x166, 0x176, 0x178, 0x18f, 0x1cf, 0x1fc,
        ])
    {
        *iv = 1;
        return;
    }
    if ch("hnou")
        || in_list(&[
            0xb5, 0xe6, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0x125, 0x127, 0x133, 0x144, 0x146, 0x148, 0x149, 0x14b, 0x14d,
            0x14f, 0x151, 0x153, 0x169, 0x16b, 0x16d, 0x16f, 0x171, 0x173, 0x1a1, 0x1b0, 0x1d2, 0x1d4, 0x1d6, 0x1d8, 0x1da, 0x1dc, 0x1fd, 0x1ff,
        ])
    {
        *bv = false;
        *iv = 2;
        return;
    }
    if ch("%0BDHMNOQU")
        || in_list(&[
            0xa4, 0xa7, 0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd8, 0xd9, 0xda, 0xdb, 0xdc, 0xdf, 0x10e, 0x110, 0x124, 0x126, 0x132, 0x143, 0x145,
            0x147, 0x14a, 0x14c, 0x14e, 0x150, 0x152, 0x168, 0x16a, 0x16c, 0x16e, 0x170, 0x172, 0x1a0, 0x1af, 0x1d1, 0x1d3, 0x1d5, 0x1d7, 0x1d9, 0x1db,
            0x1fe,
        ])
    {
        *iv = 2;
        return;
    }
    if ch("m") {
        *bv = false;
        *iv = 3;
        return;
    }
    if ch("-^_~") || in_list(&[0xad, 0xaf]) {
        *iv2 = 1;
        return;
    }
    if ch("#=") {
        *iv2 = 2;
        return;
    }
    if ch("+`") || in_list(&[0xac, 0xb7, 0xf7]) {
        *iv = 1;
        *iv2 = 1;
        return;
    }
    if ch(":;") || in_list(&[0xb1]) {
        *iv = 1;
        *iv2 = 2;
        return;
    }
    if ch("\"") || in_list(&[0xa8]) {
        *iv = 2;
        *iv2 = 1;
    }
}

#[derive(Clone, Debug, Default)]
pub struct AlgnZoneState {
    pub left_x_align_pt: f32,
    pub left_cap_align_pt: f32,
    pub x_height: f32,
    pub cap_height: f32,
    pub p_dist: Vec<f32>,
    pub edge_pts_idx: Vec<i32>,
}
