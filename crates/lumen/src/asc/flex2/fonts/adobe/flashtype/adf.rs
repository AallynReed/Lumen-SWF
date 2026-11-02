//! Port of flex-fontkit.jar `flash.fonts.flashtype.ADF`.

use crate::asc::flex2::fonts::adobe::flashtype::adf_math_utils::{
    adf_abs, adf_get_real_cubic_roots, adf_get_real_quadratic_roots, adf_max, adf_min, adf_pack_f32,
    adf_serialize_u16, adf_serialize_u32, adf_deserialize_u16, adf_deserialize_u32, adf_unpack_f32,
};
use crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::{ADFGenAttrs, ADFPath};

pub const ADF_VERSION_NUMBER: i64 = 1;
pub const ADF_X_ALGN_ZONE_BIT: i32 = 1;
pub const ADF_Y_ALGN_ZONE_BIT: i32 = 2;
pub const ADF_NON_LEAF_CELL: i32 = 0;
pub const ADF_ONE_SECTION_CELL: i32 = 1;
pub const ADF_TWO_SECTION_CELL: i32 = 2;
pub const ADF_INWARD_CORNER_CELL: i32 = 3;
pub const ADF_OUTWARD_CORNER_CELL: i32 = 4;
pub const ADF_MAX_OFFSET_SIZE: i32 = 0xFFFFFF;
pub const ADF_INVALID_DIST: f32 = -1.0;
pub const ADF_MAX_GEN_LEVEL: i32 = 15;
pub const ELM_POS_SLOPE: i16 = 1;
pub const ELM_NEG_SLOPE: i16 = -1;
pub const LINE_ELEMENT: i32 = 0;
pub const CURVE_ELEMENT: i32 = 1;
pub const STD_ORIENTATION: i32 = 0;
pub const INV_ORIENTATION: i32 = 1;
pub const VIRTUAL_CORNER: i32 = 0;
pub const INWARD_CORNER: i32 = 1;
pub const OUTWARD_CORNER: i32 = 2;
pub const CT_MAX_LEVELS: i32 = 3;
pub const CT_MAX_CHILDREN: i32 = 6;
pub const INVALID_FEATURE: i32 = 65535;
pub const LFCELL_ID_ONE_SECTION: i32 = 65534;
pub const LFCELL_ID_TWO_SECTION: i32 = 65533;
pub const LFCELL_ID_INWARD_CORNER: i32 = 65532;
pub const LFCELL_ID_OUTWARD_CORNER: i32 = 65531;
pub const SC_MAX_NUM_SCDATA: i32 = 16384;

pub fn adf_pack_cell_attrs(n: i32, n2: i32) -> i32 {
    n << 24 | n2
}

pub fn adf_unpack_cell_type(l: i64) -> i64 {
    l >> 24
}

pub fn adf_unpack_cell_offset(l: i64) -> i32 {
    l as i32 & 0xFFFFFF
}

pub fn adf_generate_adf(path: Option<&ADFPath>, attrs: &mut ADFGenAttrs) -> Option<ADFGlyph> {
    let path = path?;
    let l = attrs.max_level;
    attrs.max_level = adf_min(attrs.max_level as f32, 15.0) as i64;
    let glyph = sc_generate(path, attrs);
    attrs.max_level = l;
    glyph
}

pub fn get_dist_from_biquadratic_cell(a: &[f32], f: f32, f2: f32) -> f32 {
    let f3 = f - 0.5;
    let f4 = f - 1.0;
    let f5 = 2.0 * f3 * f4;
    let f6 = -4.0 * f * f4;
    let f7 = 2.0 * f * f3;
    let f8 = f2 - 0.5;
    let f9 = f2 - 1.0;
    let f10 = 2.0 * f8 * f9;
    let f11 = -4.0 * f2 * f9;
    let f12 = 2.0 * f2 * f8;
    f10 * (f5 * a[0] + f6 * a[1] + f7 * a[2]) + f11 * (f5 * a[3] + f6 * a[4] + f7 * a[5]) + f12 * (f5 * a[6] + f6 * a[7] + f7 * a[8])
}

pub fn adf_reconstruct_dist(g: &ADFGlyph, n: i32, f: f32, f2: f32, f3: f32, f4: f32, p: &[f32]) -> f32 {
    let l = adf_unpack_cell_type(n as i64);
    let n2 = adf_unpack_cell_offset(n as i64) as usize;
    let cd = &g.cells_and_dists;
    if l == 1 {
        let f7 = 3.0517578E-5f32;
        let mut a = [0.0f32; 9];
        for (i, v) in a.iter_mut().enumerate() {
            *v = cd[n2 + i] as f32 * f7;
        }
        return get_dist_from_biquadratic_cell(&a, p[0], p[1]);
    }
    if l == 2 {
        let f10 = 3.0517578E-5f32;
        let mut a = [0.0f32; 18];
        for (i, v) in a.iter_mut().enumerate() {
            *v = cd[n2 + i] as f32 * f10;
        }
        let f11 = get_dist_from_biquadratic_cell(&a, p[0], p[1]);
        let f12 = get_dist_from_biquadratic_cell(&a[9..], p[0], p[1]);
        if f11.abs() < f12.abs() {
            return f11;
        }
        return f12;
    }
    let f13 = p[0];
    let f14 = p[1];
    let mut n5 = 0usize;
    let mut f15 = 3.0517578E-5f32;
    let mut a7 = [0.0f32; 18];
    for v in a7.iter_mut() {
        *v = cd[n2 + n5] as f32 * f15;
        n5 += 1;
    }
    let f16 = f13 - (cd[n2 + n5] as f32 * f15 - f3) * f2;
    n5 += 1;
    let f17 = f14 - (cd[n2 + n5] as f32 * f15 - f4) * f2;
    n5 += 1;
    f15 = 6.1035156E-5f32;
    let a5 = [cd[n2 + n5] as f32 * f15, cd[n2 + n5 + 1] as f32 * f15];
    let a6 = [cd[n2 + n5 + 2] as f32 * f15, cd[n2 + n5 + 3] as f32 * f15];
    let n6 = if a5[0] * f16 + a5[1] * f17 > 0.0 { 1 } else { 0 };
    let n7 = if a6[0] * f16 + a6[1] * f17 > 0.0 { 1 } else { 0 };
    match n6 << 1 | n7 {
        3 => {
            let f18 = get_dist_from_biquadratic_cell(&a7, f13, f14);
            let f19 = get_dist_from_biquadratic_cell(&a7[9..], f13, f14);
            if f18 * f18 < f19 * f19 {
                f18
            } else {
                f19
            }
        }
        2 => get_dist_from_biquadratic_cell(&a7, f13, f14),
        1 => get_dist_from_biquadratic_cell(&a7[9..], f13, f14),
        _ => {
            let mut f20 = ((f16 * f16 + f17 * f17) as f64).sqrt() as f32 * f;
            if l == 4 {
                f20 *= -1.0;
            }
            f20
        }
    }
}

pub fn adf_reconstruct_dist_abs(g: &ADFGlyph, p: &[f32]) -> f32 {
    let f = p[0];
    let f2 = p[1];
    let mut f3 = 0.0f32;
    let mut f4 = 1.0f32;
    let mut f5 = 0.0f32;
    let mut f6 = 1.0f32;
    let mut f7 = 0.5f32;
    let mut f8 = 0.5f32;
    let cd = &g.cells_and_dists;
    let mut n = cd[0];
    while (n as u32 & 0xFF000000) == 0 {
        if f < f7 {
            if f2 < f8 {
                f4 = f7;
                f6 = f8;
                f7 = 0.5 * (f3 + f7);
                f8 = 0.5 * (f5 + f8);
                n = cd[n as usize];
                continue;
            }
            f4 = f7;
            f5 = f8;
            f7 = 0.5 * (f3 + f7);
            f8 = 0.5 * (f6 + f8);
            n = cd[n as usize + 2];
            continue;
        }
        if f2 < f8 {
            f3 = f7;
            f6 = f8;
            f7 = 0.5 * (f4 + f7);
            f8 = 0.5 * (f5 + f8);
            n = cd[n as usize + 1];
            continue;
        }
        f3 = f7;
        f5 = f8;
        f7 = 0.5 * (f4 + f7);
        f8 = 0.5 * (f6 + f8);
        n = cd[n as usize + 3];
    }
    let f9 = f4 - f3;
    let f10 = 1.0 / (f4 - f3);
    let a2 = [(f - f3) * f10, (f2 - f5) * f10];
    adf_reconstruct_dist(g, n, f9, f10, f3, f5, &a2)
}

pub fn create_intern_path(path: &ADFPath, attrs: &ADFGenAttrs) -> Option<InternPath> {
    let mut n = 0i32;
    let n2 = path.num_contours;
    let mut prev: Option<&crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::ADFPenCmd> = None;
    for c in &path.pen_cmds {
        if let Some(p) = prev {
            if c.op_code != 0 && (c.x != p.x || c.y != p.y) {
                n += 1;
            }
        }
        prev = Some(c);
    }
    if n == 0 {
        return None;
    }
    let n3 = (1i32 << (attrs.max_level + 1) as i32) + 1;
    let nu = n as usize;
    let n4 = (1 + nu) * n3 as usize;
    let mut ip = InternPath {
        num_path_elements: n,
        max_grid_lines: n3,
        elms: vec![None; nu + 1],
        contours: vec![0; n2 as usize],
        corners: vec![None; nu],
        crossings: vec![0.0; n4],
        crossing_types: vec![0; n4],
        cluster_tree_root: vec![None; 1 + nu * 3],
        ..Default::default()
    };
    for i in 0..n3 as usize {
        ip.crossings[i * (1 + nu)] = -1.0;
    }
    adf_set_glyph_scale_and_offset(path, &mut ip);
    init_intern_path(path, &mut ip, attrs);
    Some(ip)
}

pub fn adf_set_glyph_scale_and_offset(path: &ADFPath, ip: &mut InternPath) {
    let f2 = 1.4f32;
    let f3 = path.glyph_max_x - path.glyph_min_x;
    let f4 = path.glyph_max_y - path.glyph_min_y;
    let mut f5 = adf_max(f3, f4);
    if f5 == 0.0 {
        f5 = 1.0;
    }
    f5 *= f2;
    let f6 = -path.glyph_min_x + 0.5 * (f5 - f3);
    let f7 = -path.glyph_min_y + 0.5 * (f5 - f4);
    let f = 1.0 / f5;
    ip.fu_to_adf_scale = f;
    ip.glyph_offset_x = f6 * f;
    ip.glyph_offset_y = f7 * f;
    ip.adf_units_per_em = path.font_units_per_em * f;
}

pub fn init_intern_path(path: &ADFPath, ip: &mut InternPath, _attrs: &ADFGenAttrs) {
    let mut n = 0usize;
    let f = ip.fu_to_adf_scale;
    let f2 = ip.glyph_offset_x;
    let f3 = ip.glyph_offset_y;
    let f4 = 1.0E-5f32;
    if path.num_pen_cmds == 0 {
        ip.num_path_elements = 0;
        return;
    }
    let mut n2 = 0usize;
    let mut f5 = -1.0f32;
    let mut f6 = -1.0f32;
    let mut cur: Option<usize> = None;
    for k in 0..path.pen_cmds.len() {
        let prev = cur;
        cur = Some(k);
        let cmd = &path.pen_cmds[k];
        let f9 = cmd.x * f + f2;
        let mut f10 = cmd.y * f + f3;
        let f11 = f10 * 65536.0;
        f10 = f11 / 65536.0 + f4;
        if cmd.op_code == 0 {
            let l3: i64 = if n == 0 { 0 } else { (ip.contours[n - 1] + 1) as i64 };
            if n2 as i64 > l3 {
                ip.contours[n] = n2 as i32 - 1;
                n += 1;
            }
            f5 = f9;
            f6 = f10;
            continue;
        }
        if ip.elms[n2].is_none() {
            ip.elms[n2] = Some(InternPathElm::default());
        }
        if let Some(p) = prev {
            let p = &path.pen_cmds[p];
            if cmd.x == p.x && cmd.y == p.y {
                continue;
            }
        }
        let e = ip.elms[n2].as_mut().unwrap();
        e.e0 = [f5, f6];
        e.e1 = [f9, f10];
        if cmd.op_code == 1 {
            let mut f8 = f9 - f5;
            let mut f7 = f10 - f6;
            let f12 = ((f8 * f8 + f7 * f7) as f64).sqrt() as f32;
            if f12 != 0.0 {
                f8 /= f12;
                f7 /= f12;
            }
            e.c1 = f12 as f64;
            e.v01 = [f8 as f64, f7 as f64];
            e.v21 = [-f8 as f64, -f7 as f64];
            e.type_ = 0;
            e.x_min = adf_min(f5, f9);
            e.y_min = adf_min(f6, f10);
            e.x_max = adf_max(f5, f9);
            e.y_max = adf_max(f6, f10);
        } else {
            let f8 = cmd.cx * f + f2;
            let f7 = cmd.cy * f + f3;
            let d = [(f8 - f5) as f64, (f7 - f6) as f64];
            let d2 = [(f8 - f9) as f64, (f7 - f10) as f64];
            let d3 = [-d2[0] - d[0], -d2[1] - d[1]];
            e.v01 = d;
            e.v21 = d2;
            e.v012 = d3;
            e.c3 = d3[0] * d3[0] + d3[1] * d3[1];
            e.c2 = 3.0 * (d[0] * d3[0] + d[1] * d3[1]);
            e.c1 = 2.0 * (d[0] * d[0] + d[1] * d[1]);
            e.type_ = 1;
            e.x_min = adf_min(adf_min(f5, f8), f9);
            e.y_min = adf_min(adf_min(f6, f7), f10);
            e.x_max = adf_max(adf_max(f5, f8), f9);
            e.y_max = adf_max(adf_max(f6, f7), f10);
        }
        n2 += 1;
        f5 = f9;
        f6 = f10;
    }
    let l: i64 = if n == 0 { 0 } else { (ip.contours[n - 1] + 1) as i64 };
    if n2 as i64 > l {
        ip.contours[n] = n2 as i32 - 1;
        n += 1;
    }
    let l2 = n;
    ct_init_cluster_tree(ip);
    let mut n3 = 0usize;
    // -Math.cos(0.02 * 3.1415926535) and -Math.cos(0.75 * 3.1415926535) as HotSpot computes them.
    let d2 = f64::from_bits(0xbfefefd5bfe447f6);
    let d3 = f64::from_bits(0x3fe6a09e6678b051);
    let mut n4 = 0i32;
    for n in 0..l2 {
        let n5 = if n == 0 { 0 } else { (ip.contours[n - 1] + 1) as usize };
        let n6 = ip.contours[n] as usize;
        let mut ipe = n5;
        let l6 = get_contour_orientation(ip, n5 as i32, n6 as i64);
        for i in n5..=n6 {
            {
                let e3 = ip.elm_mut(i);
                e3.feature_id = n4;
                e3.orientation = l6;
            }
            let (e2i, n7) = if i < n6 { (i + 1, n4 + 2) } else { (n5, ip.elm(n5).feature_id) };
            let a4 = ip.elm(i).v21;
            let d4 = (a4[0] * a4[0] + a4[1] * a4[1]).sqrt();
            let a5 = ip.elm(e2i).v01;
            let mut d5 = (a5[0] * a5[0] + a5[1] * a5[1]).sqrt();
            let mut d6 = a5[0] * a4[0] + a5[1] * a4[1];
            if d6 != 0.0 {
                d6 /= d4 * d5;
            }
            if d6 > d2 {
                let (a, a6, d9, d8, n9, n8) = if l6 == 0 { (a4, a5, d4, d5, n4, n7) } else { (a5, a4, d5, d4, n7, n4) };
                let mut c = InternCorner::default();
                c.p = ip.elm(i).e1;
                c.t1 = [a[0] as f32, a[1] as f32];
                c.t2 = [a6[0] as f32, a6[1] as f32];
                if d9 > 0.0 {
                    c.t1[0] /= d9 as f32;
                    c.t1[1] /= d9 as f32;
                }
                if d8 > 0.0 {
                    c.t2[0] /= d8 as f32;
                    c.t2[1] /= d8 as f32;
                }
                let d7 = a6[0] * a[1] - a6[1] * a[0];
                c.type_ = if d7 > 0.0 { 1 } else { 2 };
                c.sec1_id = n9;
                c.sec2_id = n8;
                ip.corners[n3] = Some(c);
                ip.elm_mut(i).e1_id = n4 + 1;
                ip.elm_mut(e2i).e0_id = n4 + 1;
                n4 += 2;
                n3 += 1;
                ipe = e2i;
                continue;
            }
            let a = ip.elm(ipe).v01;
            let d10 = (a[0] * a[0] + a[1] * a[1]).sqrt();
            let a5 = ip.elm(e2i).v21;
            d5 = (a5[0] * a5[0] + a5[1] * a5[1]).sqrt();
            d6 = a5[0] * a[0] + a5[1] * a[1];
            if d6 != 0.0 {
                d6 /= d10 * d5;
            }
            if d6 > d3 || i == n6 {
                let mut c = InternCorner::default();
                c.sec1_id = 65535;
                c.sec2_id = 65535;
                c.type_ = 0;
                c.p = ip.elm(i).e1;
                ip.corners[n3] = Some(c);
                ip.elm_mut(i).e1_id = n4;
                ip.elm_mut(e2i).e0_id = n7;
                n4 += 2;
                n3 += 1;
                ipe = e2i;
                continue;
            }
            ip.elm_mut(e2i).e0_id = n4;
            ip.elm_mut(i).e1_id = n4;
        }
    }
}

pub fn get_contour_orientation(ip: &mut InternPath, n: i32, l: i64) -> i64 {
    let n2 = ip.num_path_elements;
    let mut f = 1.0f32;
    let mut f2 = 0.0f32;
    let mut n3 = n;
    while n3 as i64 <= l {
        let e = ip.elm(n3 as usize);
        f = adf_min(f, e.y_min);
        f2 = adf_max(f2, e.y_max);
        n3 += 1;
    }
    if f2 <= f {
        return 0;
    }
    let l2: i64 = 65536;
    let l3 = (f * l2 as f32) as i64;
    let l4 = (f2 * l2 as f32) as i64;
    let mut l5 = l2 >> 1;
    let mut l6 = l2 >> 1;
    while l6 < l3 || l6 > l4 {
        l5 >>= 1;
        if l6 < l3 {
            l6 += l5;
            continue;
        }
        l6 -= l5;
    }
    let f3 = l6 as f32 / l2 as f32;
    let n4 = ((f3 * (ip.max_grid_lines - 1) as f32) as i32).wrapping_mul(n2 + 1) as usize;
    let n5 = n4 + 1;
    let mut n6 = 0usize;
    let mut f4 = 1.0f32;
    let elms = &ip.elms;
    let fa = &mut ip.crossings;
    let sa = &mut ip.crossing_types;
    for n3 in 0..n2 {
        let e = elms[n3 as usize].as_ref().unwrap();
        if f3 < e.y_min || f3 > e.y_max {
            continue;
        }
        match e.type_ {
            0 => {
                let f6 = (f3 - e.e0[1]) / (e.e1[1] - e.e0[1]);
                let f5 = e.e0[0] * (1.0 - f6) + e.e1[0] * f6;
                fa[n5 + n6] = f5;
                if n <= n3 && n3 as i64 <= l && f5 < f4 {
                    f4 = f5;
                }
                if e.v01[1] > 0.0 {
                    sa[n5 + n6] = 1;
                } else {
                    sa[n5 + n6] = -1;
                }
                n6 += 1;
            }
            1 => {
                let d = [(e.e0[1] - f3) as f64, 2.0 * e.v01[1], e.v012[1]];
                let rv = adf_get_real_quadratic_roots(&d);
                let mut n7 = rv.num;
                while n7 > 0 {
                    n7 -= 1;
                    let f8 = rv.roots.as_ref().unwrap()[n7 as usize] as f32;
                    if !(f8 >= 0.0) || !(f8 <= 1.0) {
                        continue;
                    }
                    let f9 = 1.0 - f8;
                    let f10 = (e.v01[0] + e.e0[0] as f64) as f32;
                    let f7 = e.e0[0] * f9 * f9 + 2.0 * f10 * f9 * f8 + e.e1[0] * f8 * f8;
                    fa[n5 + n6] = f7;
                    let f9 = -2.0 * f8 * (e.v01[1] + e.v21[1]) as f32 + (2.0 * e.v01[1]) as f32;
                    if f9 > 0.0 {
                        sa[n5 + n6] = 1;
                        n6 += 1;
                    } else if f9 < 0.0 {
                        sa[n5 + n6] = -1;
                        n6 += 1;
                    }
                    if f9 == 0.0 || n3 < n || n3 as i64 > l || !(f7 < f4) {
                        continue;
                    }
                    f4 = f7;
                }
            }
            _ => {}
        }
    }
    fa[n4] = n6 as f32;
    if n6 > 1 {
        loop {
            let mut bl = true;
            for i in 0..n6 - 1 {
                if !(fa[n5 + i] > fa[n5 + i + 1]) {
                    continue;
                }
                fa.swap(n5 + i, n5 + i + 1);
                sa.swap(n5 + i, n5 + i + 1);
                bl = false;
            }
            if bl {
                break;
            }
        }
    }
    let mut n8 = 0i32;
    let mut n3 = 0usize;
    while n3 < n6 && !(f4 <= fa[n5 + n3]) {
        n8 += sa[n5 + n3] as i32;
        n3 += 1;
    }
    if n8 < 0 {
        return 1;
    }
    if n8 > 0 {
        return 0;
    }
    if sa[n5 + n3] == 1 {
        return 0;
    }
    1
}

#[allow(clippy::too_many_arguments)]
pub fn exclude_path_element(f: f32, f2: f32, f3: f32, f4: f32, f5: f32, f6: f32, f7: f32) -> bool {
    if f2 <= f4 {
        if f3 <= f6 {
            let a = f2 - f4;
            let b = f3 - f6;
            a * a + b * b > f
        } else if f3 >= f7 {
            let a = f2 - f4;
            let b = f3 - f7;
            a * a + b * b > f
        } else {
            let a = f2 - f4;
            a * a > f
        }
    } else if f2 <= f5 {
        if f3 < f6 {
            let b = f3 - f6;
            b * b > f
        } else {
            f3 >= f7 && {
                let b = f3 - f7;
                b * b > f
            }
        }
    } else if f3 <= f6 {
        let a = f2 - f5;
        let b = f3 - f6;
        a * a + b * b > f
    } else if f3 >= f7 {
        let a = f2 - f5;
        let b = f3 - f7;
        a * a + b * b > f
    } else {
        let a = f2 - f5;
        a * a > f
    }
}

pub fn dist_from_curv(p: &[f32], e: &InternPathElm) -> f32 {
    let a4 = e.v01;
    let a5 = e.v012;
    let a2 = [(e.e0[0] - p[0]) as f64, (e.e0[1] - p[1]) as f64];
    let a3 = [(e.e1[0] - p[0]) as f64, (e.e1[1] - p[1]) as f64];
    let a = [a4[0] * a2[0] + a4[1] * a2[1], e.c1 + a5[0] * a2[0] + a5[1] * a2[1], e.c2, e.c3];
    let mut d2 = a2[0] * a2[0] + a2[1] * a2[1];
    let mut f: f32 = if a2[0] * a4[1] - a2[1] * a4[0] > 0.0 { -1.0 } else { 1.0 };
    let d3 = a3[0] * a3[0] + a3[1] * a3[1];
    if d3 < d2 {
        d2 = d3;
        f = if a3[0] * e.v21[1] - a3[1] * e.v21[0] < 0.0 { -1.0 } else { 1.0 };
    }
    let rv = adf_get_real_cubic_roots(&a);
    let l = rv.num as i64;
    let mut n = 0i64;
    while n < l {
        let d9 = rv.roots.as_ref().unwrap()[n as usize];
        if 0.0 <= d9 && d9 <= 1.0 {
            let d6 = d9 * d9;
            let d5 = 2.0 * d9;
            let d7 = d6 * a5[0] + d5 * a4[0] + a2[0];
            let d4 = d6 * a5[1] + d5 * a4[1] + a2[1];
            let d8 = d7 * d7 + d4 * d4;
            if d8 <= d2 {
                let d10 = d9 * a5[0] + a4[0];
                let d11 = d9 * a5[1] + a4[1];
                f = if d10 * d4 - d11 * d7 < 0.0 { -1.0 } else { 1.0 };
                d2 = d8;
            }
        }
        n += 1;
    }
    f * d2.sqrt() as f32
}

pub fn dist_from_line(p: &[f32], e: &InternPathElm) -> f32 {
    let mut f = p[0] - e.e0[0];
    let mut f2 = p[1] - e.e0[1];
    let f3 = (f as f64 * e.v01[0] + f2 as f64 * e.v01[1]) as f32;
    let f4 = (f as f64 * e.v01[1] - f2 as f64 * e.v01[0]) as f32;
    if f3 <= 0.0 {
        let f5 = ((f * f + f2 * f2) as f64).sqrt() as f32;
        if f4 < 0.0 {
            return -f5;
        }
        return f5;
    }
    if f3 as f64 >= e.c1 {
        f = p[0] - e.e1[0];
        f2 = p[1] - e.e1[1];
        let f6 = ((f * f + f2 * f2) as f64).sqrt() as f32;
        if f4 < 0.0 {
            return -f6;
        }
        return f6;
    }
    f4
}

pub fn dist_sqr_from_curv(p: &[f32], e: &InternPathElm, iv: &mut i32) -> f32 {
    let a4 = e.v01;
    let a5 = e.v012;
    let a2 = [(e.e0[0] - p[0]) as f64, (e.e0[1] - p[1]) as f64];
    let a3 = [(e.e1[0] - p[0]) as f64, (e.e1[1] - p[1]) as f64];
    let a = [a4[0] * a2[0] + a4[1] * a2[1], e.c1 + a5[0] * a2[0] + a5[1] * a2[1], e.c2, e.c3];
    let mut d2 = a2[0] * a2[0] + a2[1] * a2[1];
    *iv = e.e0_id;
    let d3 = a3[0] * a3[0] + a3[1] * a3[1];
    if d3 < d2 {
        d2 = d3;
        *iv = e.e1_id;
    }
    let rv = adf_get_real_cubic_roots(&a);
    for i in 0..rv.num.max(0) as usize {
        let d9 = rv.roots.as_ref().unwrap()[i];
        if !(d9 > 0.0) || !(d9 < 1.0) {
            continue;
        }
        let d6 = d9 * d9;
        let d5 = 2.0 * d9;
        let d7 = d6 * a5[0] + d5 * a4[0] + a2[0];
        let d4 = d6 * a5[1] + d5 * a4[1] + a2[1];
        let d8 = d7 * d7 + d4 * d4;
        if !(d8 < d2) {
            continue;
        }
        *iv = e.feature_id;
        d2 = d8;
    }
    d2 as f32
}

pub fn dist_sqr_from_line(p: &[f32], e: &InternPathElm, iv: &mut i32) -> f32 {
    let mut d = (p[0] - e.e0[0]) as f64;
    let mut d2 = (p[1] - e.e0[1]) as f64;
    let d3 = d * e.v01[0] + d2 * e.v01[1];
    if d3 <= 0.0 {
        let d4 = d * d + d2 * d2;
        *iv = e.e0_id;
        return d4 as f32;
    }
    if d3 >= e.c1 {
        d = (p[0] - e.e1[0]) as f64;
        d2 = (p[1] - e.e1[1]) as f64;
        let d5 = d * d + d2 * d2;
        *iv = e.e1_id;
        return d5 as f32;
    }
    let d6 = d * e.v01[1] - d2 * e.v01[0];
    *iv = e.feature_id;
    (d6 * d6) as f32
}

pub fn dist_from_2d_path(ip: &mut InternPath, p: &[f32], iv: &mut i32) -> f32 {
    let f = p[1];
    let l = ip.num_path_elements as i64;
    if l == 0 {
        return -1.0;
    }
    let f2 = ct_dist_from_cluster_tree(ip, p, iv);
    let f3 = (f2 as f64).sqrt() as f32;
    let n2 = (f * (ip.max_grid_lines - 1) as f32 * (l + 1) as f32) as i32 as usize;
    let mut n3 = ip.crossings[n2] as i32;
    if n3 < 0 {
        n3 = 0;
        let elms = &ip.elms;
        let cr = &mut ip.crossings;
        let ct = &mut ip.crossing_types;
        for n in 0..l as usize {
            let e = elms[n].as_ref().unwrap();
            if f < e.y_min || f > e.y_max {
                continue;
            }
            match e.type_ {
                0 => {
                    let f5 = (f - e.e0[1]) / (e.e1[1] - e.e0[1]);
                    let f4 = e.e0[0] * (1.0 - f5) + e.e1[0] * f5;
                    cr[n2 + 1 + n3 as usize] = f4;
                    if e.v01[1] > 0.0 {
                        ct[n2 + 1 + n3 as usize] = 1;
                    } else {
                        ct[n2 + 1 + n3 as usize] = -1;
                    }
                    n3 += 1;
                }
                1 => {
                    let d = [(e.e0[1] - f) as f64, 2.0 * e.v01[1], e.v012[1]];
                    let rv = adf_get_real_quadratic_roots(&d);
                    let mut n4 = rv.num;
                    while n4 > 0 {
                        n4 -= 1;
                        let f7 = rv.roots.as_ref().unwrap()[n4 as usize] as f32;
                        if !(f7 >= 0.0) || !(f7 <= 1.0) {
                            continue;
                        }
                        let f8 = 1.0 - f7;
                        let f9 = (e.v01[0] + e.e0[0] as f64) as f32;
                        let f6 = e.e0[0] * f8 * f8 + 2.0 * f9 * f8 * f7 + e.e1[0] * f7 * f7;
                        cr[n2 + 1 + n3 as usize] = f6;
                        let f8 = -2.0 * f7 * (e.v01[1] + e.v21[1]) as f32 + (2.0 * e.v01[1]) as f32;
                        if f8 > 0.0 {
                            ct[n2 + 1 + n3 as usize] = 1;
                            n3 += 1;
                            continue;
                        }
                        if !(f8 < 0.0) {
                            continue;
                        }
                        ct[n2 + 1 + n3 as usize] = -1;
                        n3 += 1;
                    }
                }
                _ => {}
            }
        }
        cr[n2] = n3 as f32;
        if n3 > 1 {
            loop {
                let mut bl = true;
                for i in 0..(n3 - 1) as usize {
                    if !(cr[n2 + 1 + i] > cr[n2 + 1 + i + 1]) {
                        continue;
                    }
                    cr.swap(n2 + 1 + i, n2 + 1 + i + 1);
                    ct.swap(n2 + 1 + i, n2 + 1 + i + 1);
                    bl = false;
                }
                if bl {
                    break;
                }
            }
        }
    }
    let mut n5 = 0i32;
    let mut n = 0i32;
    while n < n3 && !(p[0] < ip.crossings[n2 + 1 + n as usize]) {
        n5 += ip.crossing_types[n2 + 1 + n as usize] as i32;
        n += 1;
    }
    if n5 == 0 {
        return -f3;
    }
    f3
}

pub fn dist_from_2d_path_section(ip: &InternPath, p: &[f32], l: i64) -> f32 {
    let mut n: i32 = -1;
    let f4 = p[0];
    let f5 = p[1];
    let f6 = 4.0 * adf_abs(-1.0);
    let mut f7 = -1.0f32;
    let l2 = ip.num_path_elements as i64;
    if l2 == 0 {
        return -1.0;
    }
    let mut f8 = f6;
    for n2 in 0..l2 as usize {
        let e = ip.elm(n2);
        if e.feature_id as i64 == l {
            let f2 = 0.5 * (e.x_min + e.x_max) - f4;
            let f = 0.5 * (e.y_min + e.y_max) - f5;
            let f3 = f2 * f2 + f * f;
            if f3 < f8 {
                n = n2 as i32;
                f8 = f3;
            }
        }
    }
    if f8 == f6 {
        return -1.0;
    }
    let e = ip.elm(n as usize);
    let l3 = e.orientation;
    match e.type_ {
        0 => {
            f7 = dist_from_line(p, e);
            f8 = f7 * f7;
        }
        1 => {
            f7 = dist_from_curv(p, e);
            f8 = f7 * f7;
        }
        _ => {}
    }
    for n2 in 0..l2 as usize {
        if n2 as i32 == n {
            continue;
        }
        let e = ip.elm(n2);
        if e.feature_id as i64 != l {
            continue;
        }
        let (f3, f2, f, f9) = (e.x_min, e.x_max, e.y_min, e.y_max);
        match e.type_ {
            0 => {
                if exclude_path_element(f8, f4, f5, f3, f2, f, f9) {
                    continue;
                }
                let f10 = dist_from_line(p, e);
                if f10 * f10 < f8 {
                    f7 = f10;
                    f8 = f7 * f7;
                }
            }
            1 => {
                if exclude_path_element(f8, f4, f5, f3, f2, f, f9) {
                    continue;
                }
                let f10 = dist_from_curv(p, e);
                if f10 * f10 < f8 {
                    f7 = f10;
                    f8 = f7 * f7;
                }
            }
            _ => {}
        }
    }
    if l3 == 0 {
        return f7;
    }
    -f7
}

pub fn ct_min_sqr_dist_to_bbox(p: &[f32], f: f32, f2: f32, f3: f32, f4: f32) -> f32 {
    if p[0] <= f {
        if p[1] <= f3 {
            let a = p[0] - f;
            let b = p[1] - f3;
            a * a + b * b
        } else if p[1] >= f4 {
            let a = p[0] - f;
            let b = p[1] - f4;
            a * a + b * b
        } else {
            let a = p[0] - f;
            a * a
        }
    } else if p[0] <= f2 {
        if p[1] < f3 {
            let b = p[1] - f3;
            b * b
        } else if p[1] >= f4 {
            let b = p[1] - f4;
            b * b
        } else {
            0.0
        }
    } else if p[1] <= f3 {
        let a = p[0] - f2;
        let b = p[1] - f3;
        a * a + b * b
    } else if p[1] >= f4 {
        let a = p[0] - f2;
        let b = p[1] - f4;
        a * a + b * b
    } else {
        let a = p[0] - f2;
        a * a
    }
}

pub fn ct_remove_child(nodes: &mut [Option<CTNode>], parent: usize, child: usize) {
    if Some(child) == node(nodes, parent).first_child {
        let ns = node(nodes, child).next_sibling;
        node_mut(nodes, parent).first_child = ns;
    } else {
        let mut c3 = node(nodes, parent).first_child.unwrap();
        while node(nodes, c3).next_sibling != Some(child) {
            c3 = node(nodes, c3).next_sibling.unwrap();
        }
        let ns = node(nodes, child).next_sibling;
        node_mut(nodes, c3).next_sibling = ns;
    }
    node_mut(nodes, child).next_sibling = None;
}

fn node(nodes: &[Option<CTNode>], i: usize) -> &CTNode {
    nodes[i].as_ref().unwrap()
}

fn node_mut(nodes: &mut [Option<CTNode>], i: usize) -> &mut CTNode {
    nodes[i].as_mut().unwrap()
}

pub fn ct_init_cluster_tree(ip: &mut InternPath) {
    let n = ip.num_path_elements as usize;
    let mut f = 0.0f32;
    let mut root = CTNode::default();
    root.first_child = None;
    root.next_sibling = None;
    root.x_min = 1.0;
    root.x_max = 0.0;
    root.y_min = 1.0;
    root.y_max = 0.0;
    ip.cluster_tree_root[0] = Some(root);
    let mut n2 = 1usize;
    for i in 0..n {
        let e = ip.elms[i].as_ref().unwrap();
        let mut c2 = CTNode::default();
        c2.p_elm = Some(i);
        c2.x_min = e.x_min;
        c2.y_min = e.y_min;
        c2.x_max = e.x_max;
        c2.y_max = e.y_max;
        c2.first_child = None;
        let nodes = &mut ip.cluster_tree_root;
        c2.next_sibling = node(nodes, 0).first_child;
        let (cx0, cx1, cy0, cy1) = (c2.x_min, c2.x_max, c2.y_min, c2.y_max);
        nodes[n2] = Some(c2);
        let r = node_mut(nodes, 0);
        r.first_child = Some(n2);
        n2 += 1;
        r.x_min = adf_min(r.x_min, cx0);
        r.x_max = adf_max(r.x_max, cx1);
        r.y_min = adf_min(r.y_min, cy0);
        r.y_max = adf_max(r.y_max, cy1);
        f += cx1 - cx0 + cy1 - cy0;
    }
    if n > 6 {
        let f2 = f / (n as i32 * 3) as f32;
        ct_build_cluster_tree(&mut ip.cluster_tree_root, n2, 1, f2);
    }
}

pub fn ct_build_cluster_tree(nodes: &mut [Option<CTNode>], mut n: usize, l: i64, f: f32) {
    let f2 = f * l as f32;
    let root = 0usize;
    let mut c2 = node(nodes, root).first_child;
    while let Some(c) = c2 {
        node_mut(nodes, c).is_new = false;
        c2 = node(nodes, c).next_sibling;
    }
    c2 = node(nodes, root).first_child;
    while let Some(c) = c2 {
        let cn = node(nodes, c);
        let f3 = (cn.x_max + cn.x_min) * 0.5;
        let f4 = (cn.y_max + cn.y_min) * 0.5;
        let mut c3: Option<usize> = None;
        let mut c4 = node(nodes, root).first_child;
        let mut f5 = f2;
        while let Some(o) = c4 {
            let on = node(nodes, o);
            if o != c {
                let f8 = (on.x_max + on.x_min) * 0.5;
                let f9 = f3 - f8;
                let f6 = (on.y_max + on.y_min) * 0.5;
                let f7 = f4 - f6;
                let f10 = f9 * f9 + f7 * f7;
                if f10 < f5 {
                    f5 = f10;
                    c3 = Some(o);
                }
            }
            c4 = on.next_sibling;
        }
        let mut c5 = node(nodes, c).next_sibling;
        if f5 < f2 {
            let t = c3.unwrap();
            if node(nodes, t).is_new {
                ct_remove_child(nodes, root, c);
                let fc = node(nodes, t).first_child;
                node_mut(nodes, c).next_sibling = fc;
                let (cx0, cx1, cy0, cy1) = {
                    let cn = node(nodes, c);
                    (cn.x_min, cn.x_max, cn.y_min, cn.y_max)
                };
                let tn = node_mut(nodes, t);
                tn.first_child = Some(c);
                tn.x_min = adf_min(tn.x_min, cx0);
                tn.x_max = adf_max(tn.x_max, cx1);
                tn.y_min = adf_min(tn.y_min, cy0);
                tn.y_max = adf_max(tn.y_max, cy1);
            } else {
                let c6 = n;
                let mut nn = CTNode::default();
                nn.is_new = true;
                nodes[c6] = Some(nn);
                n += 1;
                ct_remove_child(nodes, root, c);
                if c5 == Some(t) {
                    c5 = node(nodes, t).next_sibling;
                }
                ct_remove_child(nodes, root, t);
                node_mut(nodes, t).next_sibling = None;
                node_mut(nodes, c).next_sibling = Some(t);
                let (a, b) = (node(nodes, c).clone(), node(nodes, t).clone());
                let rfc = node(nodes, root).first_child;
                let nn = node_mut(nodes, c6);
                nn.first_child = Some(c);
                nn.x_min = adf_min(a.x_min, b.x_min);
                nn.x_max = adf_max(a.x_max, b.x_max);
                nn.y_min = adf_min(a.y_min, b.y_min);
                nn.y_max = adf_max(a.y_max, b.y_max);
                nn.next_sibling = rfc;
                node_mut(nodes, root).first_child = Some(c6);
            }
        }
        c2 = c5;
    }
    let mut l2 = 0i64;
    c2 = node(nodes, root).first_child;
    while let Some(c) = c2 {
        l2 += 1;
        c2 = node(nodes, c).next_sibling;
    }
    if l + 1 < 3 && l2 > 6 {
        ct_build_cluster_tree(nodes, n, l + 1, f);
    }
}

pub fn ct_dist_from_cluster_tree(ip: &mut InternPath, p: &[f32], iv: &mut i32) -> f32 {
    let mut f = 4.0 * adf_abs(-1.0);
    let dflt = InternPathElm::default();
    let elms = &ip.elms;
    let nodes = &mut ip.cluster_tree_root;
    let elm_of = |pe: Option<usize>| -> &InternPathElm {
        match pe {
            Some(i) => elms[i].as_ref().unwrap(),
            None => &dflt,
        }
    };
    let mut object: Option<usize> = Some(0);
    node_mut(nodes, 0).sort_thread = None;
    node_mut(nodes, 0).min_dist_sqr = 0.0;
    while let Some(o) = object {
        if !(node(nodes, o).min_dist_sqr < f) {
            break;
        }
        let c = o;
        object = node(nodes, o).sort_thread;
        if node(nodes, c).first_child.is_some() {
            let mut o2 = node(nodes, c).first_child;
            while let Some(ch) = o2 {
                let chn = node(nodes, ch);
                if chn.first_child.is_none() && elm_of(chn.p_elm).type_ == 0 {
                    let mut iv2 = 0i32;
                    let f2 = dist_sqr_from_line(p, elm_of(chn.p_elm), &mut iv2);
                    if f2 < f {
                        f = f2;
                        *iv = iv2;
                    }
                } else {
                    let f3 = ct_min_sqr_dist_to_bbox(p, chn.x_min, chn.x_max, chn.y_min, chn.y_max);
                    node_mut(nodes, ch).min_dist_sqr = f3;
                    if f3 < f {
                        if object.is_none() || f3 < node(nodes, object.unwrap()).min_dist_sqr {
                            node_mut(nodes, ch).sort_thread = object;
                            object = Some(ch);
                        } else {
                            let mut o3 = object.unwrap();
                            while let Some(st) = node(nodes, o3).sort_thread {
                                if !(node(nodes, st).min_dist_sqr < f3) {
                                    break;
                                }
                                o3 = st;
                            }
                            let st = node(nodes, o3).sort_thread;
                            node_mut(nodes, ch).sort_thread = st;
                            node_mut(nodes, o3).sort_thread = Some(ch);
                        }
                    }
                }
                o2 = node(nodes, ch).next_sibling;
            }
            continue;
        }
        let mut iv2 = 0i32;
        let f4 = dist_sqr_from_curv(p, elm_of(node(nodes, c).p_elm), &mut iv2);
        if !(f4 < f) {
            continue;
        }
        f = f4;
        *iv = iv2;
    }
    f
}

pub fn sc_generate(path: &ADFPath, attrs: &ADFGenAttrs) -> Option<ADFGlyph> {
    let l = attrs.max_level;
    let ip = create_intern_path(path, attrs)?;
    let n = (1i32 << (l + 1) as i32) + 1;
    let n2 = (n * n) as usize;
    let mut d = SCGenData {
        max_num_sc_data: 16384,
        sc_data: vec![0.0; 16384],
        intern_path: ip,
        gen_attrs: attrs.clone(),
        dist_map: vec![-1.0; n2],
        feature_map: vec![0; n2],
        map_side_len: n,
        num_sc_data: 0,
        num_dists: 0,
        num_cells: 0,
    };
    let mut cell = SCGenCellData::default();
    cell.level = 0;
    cell.x_idx = 0;
    cell.y_idx = 0;
    cell.dx_idx = d.map_side_len - 1 >> 1;
    cell.dy_idx = cell.dx_idx * n;
    cell.half_cell_side = 0.5;
    d.num_cells = 0;
    d.num_dists = 0;
    sc_subdiv_dist_field(&mut d, &mut cell);
    let mut g = ADFGlyph {
        cells_and_dists: vec![0; (d.num_cells + d.num_dists + 1) as usize],
        num_cells: d.num_cells as i64,
        char_code: path.char_code as i64,
        adf_units_per_em: d.intern_path.adf_units_per_em,
        fu_to_adf_scale: d.intern_path.fu_to_adf_scale,
        glyph_origin_x: d.intern_path.glyph_offset_x,
        glyph_origin_y: d.intern_path.glyph_offset_y,
        ..Default::default()
    };
    g.glyph_min_x = path.glyph_min_x * g.fu_to_adf_scale + g.glyph_origin_x;
    g.glyph_min_y = path.glyph_min_y * g.fu_to_adf_scale + g.glyph_origin_y;
    g.glyph_max_x = path.glyph_max_x * g.fu_to_adf_scale + g.glyph_origin_x;
    g.glyph_max_y = path.glyph_max_y * g.fu_to_adf_scale + g.glyph_origin_y;
    g.version = ADF_VERSION_NUMBER;
    adf_unpack_algn_zones(path, &mut g);
    let n3 = 0usize;
    let mut n4 = (n3 + 1) as i32;
    let mut n5 = n3 as i32 + d.num_cells;
    sc_build_adf_quadtree(&d, &cell, &mut g, n3, &mut n4, &mut n5);
    Some(g)
}

pub fn sc_subdiv_dist_field(d: &mut SCGenData, cell: &mut SCGenCellData) {
    let n = cell.x_idx;
    let n2 = cell.y_idx;
    let n3 = cell.dx_idx;
    let n4 = cell.dy_idx;
    let n5 = (n + n3 + n2 + n4) as usize;
    sc_init_cell_dvs(d, cell);
    d.num_cells += 1;
    if cell.level >= d.gen_attrs.max_level {
        d.feature_map[n5] = 65534;
        d.num_dists += 9;
        return;
    }
    let f = 1.4142137f32;
    let f2 = f * cell.half_cell_side;
    let f3 = d.dist_map[n5];
    if f3.abs() > f2 + d.gen_attrs.dist_eps {
        d.feature_map[n5] = 65534;
        d.num_dists += 9;
        return;
    }
    let f = sc_get_cell_error(d, cell);
    if f < d.gen_attrs.max_error {
        d.feature_map[n5] = 65534;
        d.num_dists += 9;
        return;
    }
    if d.num_sc_data + 24 < d.max_num_sc_data {
        let l = sc_is_special_cell(d, cell);
        if l != 0 {
            d.num_dists += if l == 2 { 18 } else { 24 };
            return;
        }
    }
    let mut c2 = SCGenCellData::default();
    c2.level = cell.level + 1;
    c2.dx_idx = n3 >> 1;
    c2.dy_idx = n4 >> 1;
    c2.half_cell_side = 0.5 * cell.half_cell_side;
    c2.y_idx = n2;
    for _ in 0..2 {
        c2.x_idx = n;
        for _ in 0..2 {
            sc_subdiv_dist_field(d, &mut c2);
            c2.x_idx += n3;
        }
        c2.y_idx += n4;
    }
}

pub fn sc_init_cell_dvs(d: &mut SCGenData, cell: &mut SCGenCellData) {
    let mut p = [0.0f32; 2];
    let n = cell.x_idx;
    let n2 = cell.y_idx;
    let n3 = cell.dx_idx;
    let n4 = cell.dy_idx;
    let f3 = cell.half_cell_side;
    let f4 = d.map_side_len as f32;
    let f2 = n as f32 / (f4 - 1.0);
    cell.x0 = f2;
    let f = n2 as f32 / ((f4 - 1.0) * f4);
    cell.y0 = f;
    let mut n5 = 0usize;
    let mut n6 = n2;
    p[1] = f;
    for _ in 0..=2 {
        let mut n7 = n;
        p[0] = f2;
        for _ in 0..=2 {
            let n8 = (n7 + n6) as usize;
            if d.dist_map[n8] == -1.0 {
                let mut iv = 0i32;
                let f5 = dist_from_2d_path(&mut d.intern_path, &p, &mut iv);
                cell.dist_vals[n5] = f5;
                n5 += 1;
                d.dist_map[n8] = f5;
                d.feature_map[n8] = iv as u16;
            } else {
                cell.dist_vals[n5] = d.dist_map[n8];
                n5 += 1;
            }
            n7 += n3;
            p[0] += f3;
        }
        n6 += n4;
        p[1] += f3;
    }
}

pub fn sc_get_cell_error(d: &mut SCGenData, cell: &SCGenCellData) -> f32 {
    let mut p = [0.0f32; 2];
    let n = cell.x_idx;
    let n2 = cell.y_idx;
    let n3 = cell.dx_idx;
    let n4 = cell.dy_idx;
    let f = cell.half_cell_side;
    let f2 = cell.x0;
    let f3 = cell.y0;
    let mut f4 = 0.0f32;
    let bl = n3 > 1 && n4 > 1;
    let mut f5 = 0.25f32;
    let mut n5 = n2 + n4 / 2;
    p[1] = f3 + 0.5 * f;
    for _ in 0..2 {
        let mut f6 = 0.25f32;
        let mut n6 = n + n3 / 2;
        p[0] = f2 + 0.5 * f;
        for _ in 0..2 {
            let mut iv = 0i32;
            let f8 = get_dist_from_biquadratic_cell(&cell.dist_vals, f6, f5);
            let f9 = dist_from_2d_path(&mut d.intern_path, &p, &mut iv);
            if bl {
                let n7 = (n6 + n5) as usize;
                d.dist_map[n7] = f9;
                d.feature_map[n7] = iv as u16;
            }
            let f7 = if f8 > f9 { f8 - f9 } else { f9 - f8 };
            if f7 > f4 {
                f4 = f7;
            }
            f6 += 0.5;
            n6 += n3;
            p[0] += f;
        }
        f5 += 0.5;
        n5 += n4;
        p[1] += f;
    }
    f4
}

pub fn sc_is_special_cell(d: &mut SCGenData, cell: &SCGenCellData) -> i64 {
    let mut ca = [0u16; 9];
    let mut n4: i32 = 65535;
    let mut n5: i32 = 65535;
    let mut n6: i32 = 65535;
    let mut p = [0.0f32; 2];
    let mut a2 = [0.0f32, 0.0f32];
    let mut a3 = [0.0f32, 0.0f32];
    let mut a4 = [0.0f32; 9];
    let mut a5 = [0.0f32; 9];
    let mut n7 = 0usize;
    let mut n8 = cell.y_idx;
    for _ in 0..=2 {
        let mut n2 = cell.x_idx;
        for _ in 0..=2 {
            let n9 = d.feature_map[(n2 + n8) as usize] as i32;
            if n9 % 2 != 0 {
                if n6 == 65535 {
                    n6 = n9;
                } else if n9 != n6 {
                    return 0;
                }
            } else if n4 == 65535 {
                n4 = n9;
            } else if n9 != n4 {
                if n5 == 65535 {
                    n5 = n9;
                } else if n9 != n5 {
                    return 0;
                }
            }
            ca[n7] = n9 as u16;
            n7 += 1;
            n2 += cell.dx_idx;
        }
        n8 += cell.dy_idx;
    }
    let ip = &d.intern_path;
    let corner_at = |idx: i32| -> Option<InternCorner> { ip.corners[idx as usize].clone() };
    let corner: Option<InternCorner>;
    if n6 != 65535 {
        let c = corner_at((n6 - 1) / 2).unwrap();
        if n5 == 65535 {
            if n4 == 65535 {
                n4 = c.sec1_id;
                n5 = c.sec2_id;
            } else if n4 == c.sec1_id {
                n5 = c.sec2_id;
            } else {
                if n4 != c.sec2_id {
                    return 0;
                }
                n4 = c.sec1_id;
                n5 = c.sec2_id;
            }
        } else if n4 == c.sec1_id {
            if n5 != c.sec2_id {
                return 0;
            }
        } else {
            if n4 != c.sec2_id {
                return 0;
            }
            if n5 != c.sec1_id {
                return 0;
            }
            n4 = c.sec1_id;
            n5 = c.sec2_id;
        }
        corner = Some(c);
    } else {
        if n5 == 65535 {
            return 0;
        }
        let c = corner_at(n4 / 2).unwrap();
        if c.sec1_id == n4 && c.sec2_id == n5 {
            n6 = n4 + 1;
            corner = Some(c);
        } else if c.sec1_id == n5 && c.sec2_id == n4 {
            n6 = n4 + 1;
            n4 = c.sec1_id;
            n5 = c.sec2_id;
            corner = Some(c);
        } else {
            let c = corner_at(n5 / 2).unwrap();
            if c.sec1_id == n4 && c.sec2_id == n5 {
                n6 = n4 + 1;
                corner = Some(c);
            } else if c.sec1_id == n5 && c.sec2_id == n4 {
                n6 = n5 + 1;
                n4 = c.sec1_id;
                n5 = c.sec2_id;
                corner = Some(c);
            } else {
                corner = None;
            }
        }
    }
    let (l2, l): (i64, i64);
    if let Some(c) = &corner {
        if c.type_ == 1 {
            l2 = 3;
            l = 65532;
        } else {
            l2 = 4;
            l = 65531;
        }
        a2 = c.t1;
        a3 = c.t2;
    } else {
        l2 = 2;
        l = 65533;
    }
    let f4 = cell.x0;
    let f5 = cell.y0;
    let a6 = cell.dist_vals;
    let f3 = cell.half_cell_side;
    if corner.is_none() {
        p[1] = f5;
        n7 = 0;
        for _ in 0..=2 {
            p[0] = f4;
            for _ in 0..=2 {
                if ca[n7] as i32 == n4 {
                    a4[n7] = a6[n7];
                    a5[n7] = dist_from_2d_path_section(ip, &p, n5 as i64);
                } else {
                    a4[n7] = dist_from_2d_path_section(ip, &p, n4 as i64);
                    a5[n7] = a6[n7];
                }
                p[0] += f3;
                n7 += 1;
            }
            p[1] += f3;
        }
    } else {
        let c = corner.as_ref().unwrap();
        p[1] = f5;
        n7 = 0;
        for _ in 0..=2 {
            p[0] = f4;
            for _ in 0..=2 {
                let l3 = ca[n7] as i64;
                if ca[n7] as i32 == n4 {
                    a4[n7] = a6[n7];
                    a5[n7] = dist_from_2d_path_section(ip, &p, n5 as i64);
                } else if ca[n7] as i32 == n5 {
                    a4[n7] = dist_from_2d_path_section(ip, &p, n4 as i64);
                    a5[n7] = a6[n7];
                } else {
                    a4[n7] = a6[n7];
                    a5[n7] = a6[n7];
                }
                let f2 = p[0] - c.p[0];
                let f = p[1] - c.p[1];
                if f2 * a2[0] + f * a2[1] < 0.0 {
                    let f6 = f * a2[0] - f2 * a2[1];
                    if adf_abs(f6) < adf_abs(a4[n7]) {
                        a4[n7] = f6;
                    }
                }
                if f2 * a3[0] + f * a3[1] < 0.0 {
                    let f6 = f2 * a3[1] - f * a3[0];
                    if adf_abs(f6) < adf_abs(a5[n7]) {
                        a5[n7] = f6;
                    }
                }
                let n10 = if a2[0] * f2 + a2[1] * f > 0.0 { 1 } else { 0 };
                let n11 = if a3[0] * f2 + a3[1] * f > 0.0 { 1 } else { 0 };
                match n10 << 1 | n11 {
                    3 => {
                        let cond = if l3 == n4 as i64 {
                            adf_abs(a5[n7]) < adf_abs(a4[n7])
                        } else {
                            l3 == n5 as i64 && adf_abs(a4[n7]) < adf_abs(a5[n7])
                        };
                        if cond {
                            return 0;
                        }
                    }
                    2 => {
                        if l3 != n4 as i64 {
                            return 0;
                        }
                    }
                    1 => {
                        if l3 != n5 as i64 {
                            return 0;
                        }
                    }
                    _ => {
                        if l3 != n6 as i64 {
                            return 0;
                        }
                    }
                }
                p[0] += f3;
                n7 += 1;
            }
            p[1] += f3;
        }
    }
    if cell.dx_idx > 1 {
        let f7 = d.gen_attrs.max_error;
        let mut f8 = 0.25f32;
        n8 = cell.y_idx + cell.dy_idx / 2;
        p[1] = f5 + 0.5 * f3;
        for _ in 0..2 {
            let mut f2 = 0.25f32;
            let mut n2 = cell.x_idx + cell.dx_idx / 2;
            p[0] = f4 + 0.5 * f3;
            for _ in 0..2 {
                match &corner {
                    None => {
                        let f = dist_from_2d_path_section(ip, &p, n4 as i64);
                        let mut f11 = get_dist_from_biquadratic_cell(&a4, f2, f8);
                        if (f - f11).abs() > f7 {
                            return 0;
                        }
                        let f = dist_from_2d_path_section(ip, &p, n5 as i64);
                        let f9 = get_dist_from_biquadratic_cell(&a5, f2, f8);
                        if (f - f9).abs() > f7 {
                            return 0;
                        }
                        let f = d.dist_map[(n2 + n8) as usize];
                        if adf_abs(f9) < adf_abs(f11) {
                            f11 = f9;
                        }
                        if (f - f11).abs() > f7 {
                            return 0;
                        }
                    }
                    Some(c) => {
                        let f13 = p[0] - c.p[0];
                        let f14 = p[1] - c.p[1];
                        let f15 = a2[0] * f13 + a2[1] * f14;
                        let f16 = a3[0] * f13 + a3[1] * f14;
                        let mut f = dist_from_2d_path_section(ip, &p, n4 as i64);
                        if f15 <= 0.0 {
                            let f12 = f14 * a2[0] - f13 * a2[1];
                            if adf_abs(f12) < adf_abs(f) {
                                f = f12;
                            }
                        }
                        let mut f11 = get_dist_from_biquadratic_cell(&a4, f2, f8);
                        if (f - f11).abs() > f7 {
                            return 0;
                        }
                        f = dist_from_2d_path_section(ip, &p, n5 as i64);
                        if f16 <= 0.0 {
                            let f12 = f13 * a3[1] - f14 * a3[0];
                            if adf_abs(f12) < adf_abs(f) {
                                f = f12;
                            }
                        }
                        let f9 = get_dist_from_biquadratic_cell(&a5, f2, f8);
                        if (f - f9).abs() > f7 {
                            return 0;
                        }
                        f = d.dist_map[(n2 + n8) as usize];
                        let n13 = if f15 > 0.0 { 1 } else { 0 };
                        let n14 = if f16 > 0.0 { 1 } else { 0 };
                        match n13 << 1 | n14 {
                            3 => {
                                if f9 * f9 - f11 * f11 < 0.0 {
                                    f11 = f9;
                                }
                            }
                            2 => {}
                            1 => {
                                f11 = f9;
                            }
                            _ => {
                                f11 = ((f13 * f13 + f14 * f14) as f64).sqrt() as f32;
                                if l == 65531 {
                                    f11 *= -1.0;
                                }
                            }
                        }
                        if (f - f11).abs() > f7 {
                            return 0;
                        }
                    }
                }
                f2 += 0.5;
                n2 += cell.dx_idx;
                p[0] += f3;
            }
            f8 += 0.5;
            n8 += cell.dy_idx;
            p[1] += f3;
        }
    }
    let n16 = (cell.x_idx + cell.dx_idx + cell.y_idx + cell.dy_idx) as usize;
    d.dist_map[n16] = d.num_sc_data as f32;
    d.feature_map[n16] = l as u16;
    let mut n17 = d.num_sc_data as usize;
    for v in a4 {
        d.sc_data[n17] = v;
        n17 += 1;
    }
    for v in a5 {
        d.sc_data[n17] = v;
        n17 += 1;
    }
    d.num_sc_data += 18;
    let c = match &corner {
        None => return l2,
        Some(c) => c,
    };
    for v in [c.p[0], c.p[1], a2[0], a2[1], a3[0], a3[1]] {
        d.sc_data[n17] = v;
        n17 += 1;
    }
    d.num_sc_data += 6;
    l2
}

pub fn sc_build_adf_quadtree(d: &SCGenData, cell: &SCGenCellData, g: &mut ADFGlyph, n: usize, iv: &mut i32, iv2: &mut i32) {
    let n2 = cell.x_idx;
    let n3 = cell.y_idx;
    let n4 = cell.dx_idx;
    let n5 = cell.dy_idx;
    let n6 = (n2 + n4 + n3 + n5) as usize;
    let c = d.feature_map[n6];
    match c {
        0xFFFE => {
            g.cells_and_dists[n] = adf_pack_cell_attrs(1, *iv2);
            let mut n7 = cell.y_idx;
            let f = 32768.0f32;
            let mut n8 = 0usize;
            for _ in 0..=2 {
                let mut n10 = cell.x_idx;
                for _ in 0..=2 {
                    g.cells_and_dists[*iv2 as usize + n8] = (d.dist_map[(n10 + n7) as usize] * f) as i32;
                    n8 += 1;
                    n10 += cell.dx_idx;
                }
                n7 += cell.dy_idx;
            }
            *iv2 += 9;
            return;
        }
        0xFFFD => {
            g.cells_and_dists[n] = adf_pack_cell_attrs(2, *iv2);
            let n12 = d.dist_map[n6] as i32 as usize;
            let f = 32768.0f32;
            for i in 0..18 {
                g.cells_and_dists[*iv2 as usize + i] = (d.sc_data[n12 + i] * f) as i32;
            }
            *iv2 += 18;
            return;
        }
        0xFFFB | 0xFFFC => {
            let n15 = if c == 0xFFFC { 3 } else { 4 };
            g.cells_and_dists[n] = adf_pack_cell_attrs(n15, *iv2);
            let n16 = d.dist_map[n6] as i32 as usize;
            let mut f = 32768.0f32;
            let mut n17 = 0usize;
            for n14 in 0..20 {
                g.cells_and_dists[*iv2 as usize + n17] = (d.sc_data[n16 + n14] * f) as i32;
                n17 += 1;
            }
            f = 16384.0;
            for n14 in 20..24 {
                g.cells_and_dists[*iv2 as usize + n17] = (d.sc_data[n16 + n14] * f) as i32;
                n17 += 1;
            }
            *iv2 += 24;
            return;
        }
        _ => {}
    }
    let mut n18 = *iv;
    g.cells_and_dists[n] = adf_pack_cell_attrs(0, n18);
    *iv += 4;
    let mut c2 = SCGenCellData::default();
    c2.level = cell.level + 1;
    c2.dx_idx = n4 >> 1;
    c2.dy_idx = n5 >> 1;
    c2.half_cell_side = 0.5 * cell.half_cell_side;
    c2.y_idx = n3;
    for _ in 0..2 {
        c2.x_idx = n2;
        for _ in 0..2 {
            sc_build_adf_quadtree(d, &c2, g, n18 as usize, iv, iv2);
            n18 += 1;
            c2.x_idx += n4;
        }
        c2.y_idx += n5;
    }
}

pub fn adf_pack_algn_zones(g: &ADFGlyph, path: &mut ADFPath) {
    let mut z = vec![0i64; 2];
    let mut r = adf_pack_f32(g.x_algn_coord);
    if r.too_large {
        r.packed_value = 0;
    }
    adf_serialize_u16(&mut z, 0, 0, 8, r.packed_value);
    r = adf_pack_f32(g.y_algn_coord);
    if r.too_large {
        r.packed_value = 0;
    }
    adf_serialize_u16(&mut z, 0, 16, 24, r.packed_value);
    r = adf_pack_f32(g.x_range);
    if r.too_large {
        r.packed_value = 0;
    }
    adf_serialize_u16(&mut z, 1, 0, 8, r.packed_value);
    r = adf_pack_f32(g.y_range);
    if r.too_large {
        r.packed_value = 0;
    }
    adf_serialize_u16(&mut z, 1, 16, 24, r.packed_value);
    path.algn_zones = Some(z);
    path.algn_zones_mask = adf_serialize_u32(g.algn_zones_mask);
}

pub fn adf_unpack_algn_zones(path: &ADFPath, g: &mut ADFGlyph) {
    g.algn_zones_mask = adf_deserialize_u32(path.algn_zones_mask);
    let l3 = (g.algn_zones_mask & 1) as i64;
    let l4 = (g.algn_zones_mask & 2) as i64;
    if l3 == 0 {
        g.x_range = 0.0;
    } else {
        let z = path.algn_zones.as_ref().unwrap();
        let l2 = adf_deserialize_u16(z, 0, 0, 8);
        let l = adf_deserialize_u16(z, 1, 0, 8);
        g.x_algn_coord = adf_unpack_f32(l2);
        g.x_range = adf_unpack_f32(l);
    }
    if l4 == 0 {
        g.y_range = 0.0;
    } else {
        let z = path.algn_zones.as_ref().unwrap();
        let l2 = adf_deserialize_u16(z, 0, 16, 24);
        let l = adf_deserialize_u16(z, 1, 16, 24);
        g.y_algn_coord = adf_unpack_f32(l2);
        g.y_range = adf_unpack_f32(l);
    }
}

#[derive(Clone, Debug, Default)]
pub struct SCGenCellData {
    pub level: i64,
    pub x_idx: i32,
    pub y_idx: i32,
    pub dx_idx: i32,
    pub dy_idx: i32,
    pub half_cell_side: f32,
    pub dist_vals: [f32; 9],
    pub x0: f32,
    pub y0: f32,
}

#[derive(Clone, Debug, Default)]
pub struct SCGenData {
    pub intern_path: InternPath,
    pub gen_attrs: ADFGenAttrs,
    pub map_side_len: i32,
    pub dist_map: Vec<f32>,
    pub feature_map: Vec<u16>,
    pub sc_data: Vec<f32>,
    pub num_sc_data: i32,
    pub max_num_sc_data: i32,
    pub num_dists: i32,
    pub num_cells: i32,
}

#[derive(Clone, Debug, Default)]
pub struct InternPath {
    pub fu_to_adf_scale: f32,
    pub glyph_offset_x: f32,
    pub glyph_offset_y: f32,
    pub adf_units_per_em: f32,
    pub num_path_elements: i32,
    pub elms: Vec<Option<InternPathElm>>,
    pub contours: Vec<i32>,
    pub corners: Vec<Option<InternCorner>>,
    pub max_grid_lines: i32,
    pub crossings: Vec<f32>,
    pub crossing_types: Vec<i16>,
    pub cluster_tree_root: Vec<Option<CTNode>>,
}

impl InternPath {
    fn elm(&self, i: usize) -> &InternPathElm {
        self.elms[i].as_ref().unwrap()
    }

    fn elm_mut(&mut self, i: usize) -> &mut InternPathElm {
        self.elms[i].as_mut().unwrap()
    }
}

/// Node of the cluster tree; links are indices into `InternPath::cluster_tree_root`, `p_elm` into `elms`.
#[derive(Clone, Debug, Default)]
pub struct CTNode {
    pub is_new: bool,
    pub min_dist_sqr: f32,
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
    pub first_child: Option<usize>,
    pub next_sibling: Option<usize>,
    pub sort_thread: Option<usize>,
    pub p_elm: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct InternCorner {
    pub sec1_id: i32,
    pub sec2_id: i32,
    pub p: [f32; 2],
    pub t1: [f32; 2],
    pub t2: [f32; 2],
    pub type_: i64,
}

#[derive(Clone, Debug, Default)]
pub struct InternPathElm {
    pub type_: i32,
    pub x_min: f32,
    pub x_max: f32,
    pub y_min: f32,
    pub y_max: f32,
    pub feature_id: i32,
    pub e0_id: i32,
    pub e1_id: i32,
    pub e0: [f32; 2],
    pub e1: [f32; 2],
    pub v01: [f64; 2],
    pub v21: [f64; 2],
    pub v012: [f64; 2],
    pub c1: f64,
    pub c2: f64,
    pub c3: f64,
    pub orientation: i64,
}

#[derive(Clone, Debug, Default)]
pub struct ADFGlyph {
    pub version: i64,
    pub num_cells: i64,
    pub cells_and_dists: Vec<i32>,
    pub char_code: i64,
    pub fu_to_adf_scale: f32,
    pub adf_units_per_em: f32,
    pub glyph_origin_x: f32,
    pub glyph_origin_y: f32,
    pub glyph_min_x: f32,
    pub glyph_min_y: f32,
    pub glyph_max_x: f32,
    pub glyph_max_y: f32,
    pub x_algn_coord: f32,
    pub y_algn_coord: f32,
    pub x_range: f32,
    pub y_range: f32,
    pub algn_zones_mask: i32,
}
