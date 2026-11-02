//! Port of FreeType 2.14.2 `src/base/ftoutln.c` (as built into JDK 17.0.19).

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::*;
use crate::asc::flex2::fonts::java2d::freetype::fterrors::*;
use crate::asc::flex2::fonts::java2d::freetype::fttypes::*;

pub const FT_ORIENTATION_TRUETYPE: i32 = 0;
pub const FT_ORIENTATION_POSTSCRIPT: i32 = 1;
pub const FT_ORIENTATION_NONE: i32 = 2;

pub fn ft_outline_decompose(outline: &FtOutline, func_interface: &mut dyn FtOutlineFuncs) -> FtError {
    let shift = func_interface.shift();
    let delta = func_interface.delta();
    let scaled = |x: FtPos| x.wrapping_mul(1i32 << shift).wrapping_sub(delta);
    let pts = &outline.points;
    let tags = &outline.tags;
    let mut last: i32 = -1;
    for n in 0..outline.n_contours as usize {
        let first = last + 1;
        last = outline.contours[n] as i32;
        if last < first {
            return FT_ERR_INVALID_OUTLINE;
        }
        let mut limit = last as isize;
        let mut v_start = pts[first as usize];
        v_start.x = scaled(v_start.x);
        v_start.y = scaled(v_start.y);
        let mut v_last = pts[last as usize];
        v_last.x = scaled(v_last.x);
        v_last.y = scaled(v_last.y);
        let mut v_control = v_start;
        let mut point = first as isize;
        let mut tag = ft_curve_tag(tags[first as usize]);
        if tag == FT_CURVE_TAG_CUBIC {
            return FT_ERR_INVALID_OUTLINE;
        }
        if tag == FT_CURVE_TAG_CONIC {
            if ft_curve_tag(tags[last as usize]) == FT_CURVE_TAG_ON {
                v_start = v_last;
                limit -= 1;
            } else {
                v_start.x = (v_start.x.wrapping_add(v_last.x)) / 2;
                v_start.y = (v_start.y.wrapping_add(v_last.y)) / 2;
            }
            point -= 1;
        }
        let error = func_interface.move_to(&v_start);
        if error != 0 {
            return error;
        }
        let mut closed = false;
        'outer: while point < limit {
            point += 1;
            tag = ft_curve_tag(tags[point as usize]);
            match tag {
                FT_CURVE_TAG_ON => {
                    let vec = FtVector { x: scaled(pts[point as usize].x), y: scaled(pts[point as usize].y) };
                    let error = func_interface.line_to(&vec);
                    if error != 0 {
                        return error;
                    }
                    continue;
                }
                FT_CURVE_TAG_CONIC => {
                    v_control.x = scaled(pts[point as usize].x);
                    v_control.y = scaled(pts[point as usize].y);
                    loop {
                        if point < limit {
                            point += 1;
                            tag = ft_curve_tag(tags[point as usize]);
                            let vec = FtVector { x: scaled(pts[point as usize].x), y: scaled(pts[point as usize].y) };
                            if tag == FT_CURVE_TAG_ON {
                                let error = func_interface.conic_to(&v_control, &vec);
                                if error != 0 {
                                    return error;
                                }
                                continue 'outer;
                            }
                            if tag != FT_CURVE_TAG_CONIC {
                                return FT_ERR_INVALID_OUTLINE;
                            }
                            let v_middle = FtVector {
                                x: (v_control.x.wrapping_add(vec.x)) / 2,
                                y: (v_control.y.wrapping_add(vec.y)) / 2,
                            };
                            let error = func_interface.conic_to(&v_control, &v_middle);
                            if error != 0 {
                                return error;
                            }
                            v_control = vec;
                            continue;
                        }
                        let error = func_interface.conic_to(&v_control, &v_start);
                        if error != 0 {
                            return error;
                        }
                        closed = true;
                        break 'outer;
                    }
                }
                _ => {
                    if point + 1 > limit || ft_curve_tag(tags[point as usize + 1]) != FT_CURVE_TAG_CUBIC {
                        return FT_ERR_INVALID_OUTLINE;
                    }
                    point += 2;
                    let p2 = pts[point as usize - 2];
                    let p1 = pts[point as usize - 1];
                    let vec1 = FtVector { x: scaled(p2.x), y: scaled(p2.y) };
                    let vec2 = FtVector { x: scaled(p1.x), y: scaled(p1.y) };
                    if point <= limit {
                        let vec = FtVector { x: scaled(pts[point as usize].x), y: scaled(pts[point as usize].y) };
                        let error = func_interface.cubic_to(&vec1, &vec2, &vec);
                        if error != 0 {
                            return error;
                        }
                        continue;
                    }
                    let error = func_interface.cubic_to(&vec1, &vec2, &v_start);
                    if error != 0 {
                        return error;
                    }
                    closed = true;
                    break 'outer;
                }
            }
        }
        if !closed {
            let error = func_interface.line_to(&v_start);
            if error != 0 {
                return error;
            }
        }
    }
    FT_ERR_OK
}

pub fn ft_outline_check(outline: &FtOutline) -> FtError {
    let n_points = outline.n_points;
    let n_contours = outline.n_contours;
    if n_points == 0 && n_contours == 0 {
        return FT_ERR_OK;
    }
    if n_points == 0 || n_contours == 0 {
        return FT_ERR_INVALID_OUTLINE;
    }
    let mut end0: i32 = -1;
    for n in 0..n_contours as usize {
        let end = outline.contours[n] as i32;
        if end <= end0 || end >= n_points {
            return FT_ERR_INVALID_OUTLINE;
        }
        end0 = end;
    }
    if end0 != n_points - 1 {
        return FT_ERR_INVALID_OUTLINE;
    }
    FT_ERR_OK
}

pub fn ft_outline_get_cbox(outline: &FtOutline) -> FtBBox {
    ft_points_get_cbox(&outline.points[..outline.n_points as usize])
}

pub fn ft_points_get_cbox(points: &[FtVector]) -> FtBBox {
    if points.is_empty() {
        return FtBBox::default();
    }
    let mut b = FtBBox { x_min: points[0].x, x_max: points[0].x, y_min: points[0].y, y_max: points[0].y };
    for v in &points[1..] {
        if v.x < b.x_min {
            b.x_min = v.x;
        }
        if v.x > b.x_max {
            b.x_max = v.x;
        }
        if v.y < b.y_min {
            b.y_min = v.y;
        }
        if v.y > b.y_max {
            b.y_max = v.y;
        }
    }
    b
}

pub fn ft_outline_translate(outline: &mut FtOutline, x_offset: FtPos, y_offset: FtPos) {
    let n = outline.n_points as usize;
    ft_points_translate(&mut outline.points[..n], x_offset, y_offset);
}

pub fn ft_points_translate(points: &mut [FtVector], x_offset: FtPos, y_offset: FtPos) {
    for v in points {
        v.x = add_long(v.x, x_offset);
        v.y = add_long(v.y, y_offset);
    }
}

pub fn ft_vector_transform(vector: &mut FtVector, matrix: &FtMatrix) {
    let xz = ft_mul_fix(vector.x, matrix.xx).wrapping_add(ft_mul_fix(vector.y, matrix.xy));
    let yz = ft_mul_fix(vector.x, matrix.yx).wrapping_add(ft_mul_fix(vector.y, matrix.yy));
    vector.x = xz;
    vector.y = yz;
}

pub fn ft_outline_transform(outline: &mut FtOutline, matrix: &FtMatrix) {
    let n = outline.n_points as usize;
    ft_points_transform(&mut outline.points[..n], matrix);
}

pub fn ft_points_transform(points: &mut [FtVector], matrix: &FtMatrix) {
    for v in points {
        ft_vector_transform(v, matrix);
    }
}

pub fn ft_outline_embolden(outline: &mut FtOutline, strength: FtPos) -> FtError {
    ft_outline_embolden_xy(outline, strength, strength)
}

pub fn ft_outline_embolden_xy(outline: &mut FtOutline, xstrength: FtPos, ystrength: FtPos) -> FtError {
    let xstrength = xstrength / 2;
    let ystrength = ystrength / 2;
    if xstrength == 0 && ystrength == 0 {
        return FT_ERR_OK;
    }
    let orientation = ft_outline_get_orientation(outline);
    if orientation == FT_ORIENTATION_NONE {
        if outline.n_contours != 0 {
            return FT_ERR_INVALID_ARGUMENT;
        } else {
            return FT_ERR_OK;
        }
    }
    let points = &mut outline.points;
    let mut last: i32 = -1;
    for c in 0..outline.n_contours as usize {
        let first = last + 1;
        last = outline.contours[c] as i32;
        let mut l_in: FtFixed = 0;
        let mut l_out: FtFixed;
        let mut l_anchor: FtFixed = 0;
        let mut in_ = FtVector::default();
        let mut out = FtVector::default();
        let mut anchor = FtVector::default();
        let mut i = last;
        let mut j = first;
        let mut k: i32 = -1;
        while j != i && i != k {
            if j != k {
                out.x = points[j as usize].x.wrapping_sub(points[i as usize].x);
                out.y = points[j as usize].y.wrapping_sub(points[i as usize].y);
                l_out = ft_vector_norm_len(&mut out) as FtFixed;
                if l_out == 0 {
                    j = if j < last { j + 1 } else { first };
                    continue;
                }
            } else {
                out = anchor;
                l_out = l_anchor;
            }
            if l_in != 0 {
                if k < 0 {
                    k = i;
                    anchor = in_;
                    l_anchor = l_in;
                }
                let mut d = ft_mul_fix(in_.x, out.x).wrapping_add(ft_mul_fix(in_.y, out.y));
                let mut shift = FtVector::default();
                if d > -0xF000 {
                    d = d.wrapping_add(0x10000);
                    shift.x = in_.y.wrapping_add(out.y);
                    shift.y = in_.x.wrapping_add(out.x);
                    if orientation == FT_ORIENTATION_TRUETYPE {
                        shift.x = shift.x.wrapping_neg();
                    } else {
                        shift.y = shift.y.wrapping_neg();
                    }
                    let mut q = ft_mul_fix(out.x, in_.y).wrapping_sub(ft_mul_fix(out.y, in_.x));
                    if orientation == FT_ORIENTATION_TRUETYPE {
                        q = q.wrapping_neg();
                    }
                    let l = l_in.min(l_out);
                    if ft_mul_fix(xstrength, q) <= ft_mul_fix(l, d) {
                        shift.x = ft_mul_div(shift.x, xstrength, d);
                    } else {
                        shift.x = ft_mul_div(shift.x, l, q);
                    }
                    if ft_mul_fix(ystrength, q) <= ft_mul_fix(l, d) {
                        shift.y = ft_mul_div(shift.y, ystrength, d);
                    } else {
                        shift.y = ft_mul_div(shift.y, l, q);
                    }
                }
                while i != j {
                    let p = &mut points[i as usize];
                    p.x = p.x.wrapping_add(xstrength.wrapping_add(shift.x));
                    p.y = p.y.wrapping_add(ystrength.wrapping_add(shift.y));
                    i = if i < last { i + 1 } else { first };
                }
            } else {
                i = j;
            }
            in_ = out;
            l_in = l_out;
            j = if j < last { j + 1 } else { first };
        }
    }
    FT_ERR_OK
}

pub fn ft_outline_get_orientation(outline: &FtOutline) -> i32 {
    if outline.n_points <= 0 {
        return FT_ORIENTATION_TRUETYPE;
    }
    let cbox = ft_outline_get_cbox(outline);
    if cbox.x_min == cbox.x_max || cbox.y_min == cbox.y_max {
        return FT_ORIENTATION_NONE;
    }
    if cbox.x_min < -0x100_0000 || cbox.y_min < -0x100_0000 || cbox.x_max > 0x100_0000 || cbox.y_max > 0x100_0000 {
        return FT_ORIENTATION_NONE;
    }
    let xshift = (ft_msb((ft_abs(cbox.x_max) | ft_abs(cbox.x_min)) as u32) - 14).max(0);
    let yshift = (ft_msb((cbox.y_max - cbox.y_min) as u32) - 14).max(0);
    let points = &outline.points;
    let mut area: FtPos = 0;
    let mut last: i32 = -1;
    for c in 0..outline.n_contours as usize {
        let first = last + 1;
        last = outline.contours[c] as i32;
        let mut v_prev = FtVector { x: points[last as usize].x >> xshift, y: points[last as usize].y >> yshift };
        for n in first..=last {
            let v_cur = FtVector { x: points[n as usize].x >> xshift, y: points[n as usize].y >> yshift };
            area = add_long(area, mul_long(v_cur.y.wrapping_sub(v_prev.y), v_cur.x.wrapping_add(v_prev.x)));
            v_prev = v_cur;
        }
    }
    if area > 0 {
        FT_ORIENTATION_POSTSCRIPT
    } else if area < 0 {
        FT_ORIENTATION_TRUETYPE
    } else {
        FT_ORIENTATION_NONE
    }
}
