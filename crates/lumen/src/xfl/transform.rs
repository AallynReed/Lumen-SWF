//! `ShapeTransformer` for a uniform scale, as the converter applies to small shapes.

use crate::shape::{Fill, Record, Shape, Styles};
use crate::types::Matrix;

fn scale_matrix(m: &mut Matrix, s: f64) {
    m.a = (m.a * s) as f32 as f64;
    m.b = (m.b * s) as f32 as f64;
    m.c = (m.c * s) as f32 as f64;
    m.d = (m.d * s) as f32 as f64;
    m.tx = (m.tx * s) as i32 as f64;
    m.ty = (m.ty * s) as i32 as f64;
}

fn scale_fill(f: &mut Fill, s: f64) {
    match f {
        Fill::Bitmap { matrix, .. } => scale_matrix(matrix, s),
        Fill::Linear(g) | Fill::Radial(g) | Fill::Focal(g) => scale_matrix(&mut g.matrix, s),
        Fill::Solid(_) => {}
    }
}

fn scale_styles(st: &mut Styles, s: f64, shape_num: u8) {
    for f in &mut st.fills {
        scale_fill(f, s);
    }
    for l in &mut st.lines {
        if shape_num >= 4 {
            if let Some(f) = &mut l.fill {
                scale_fill(f, s);
            }
        }
        l.width = ((l.width as f64) * s).min(u16::MAX as f64) as u16;
    }
}

fn round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

/// `transformShapeRecords` and `transformStyles` with `Matrix.getScaleInstance(s)`.
pub fn scale_shape(shape: &mut Shape, s: f64, shape_num: u8) {
    scale_styles(&mut shape.styles, s, shape_num);
    let t = |x: i32, y: i32| (round(x as f64 * s), round(y as f64 * s));
    let (mut x, mut y) = (0i32, 0i32);
    let mut last_style: Option<usize> = None;
    let mut was_move = false;
    for i in 0..shape.records.len() {
        let rec = shape.records[i].clone();
        match rec {
            Record::StyleChange { move_to, .. } => {
                last_style = Some(i);
                if let Record::StyleChange { new_styles: Some(ns), .. } = &mut shape.records[i] {
                    scale_styles(ns, s, shape_num);
                }
                if let Some((mx, my)) = move_to {
                    x = mx;
                    y = my;
                    if let Record::StyleChange { move_to, .. } = &mut shape.records[i] {
                        *move_to = Some(t(mx, my));
                    }
                    was_move = true;
                }
            }
            Record::Straight { dx, dy } => {
                if !was_move {
                    if let Some(li) = last_style {
                        let p = t(x, y);
                        if p != (0, 0) {
                            if let Record::StyleChange { move_to, .. } = &mut shape.records[li] {
                                *move_to = Some(p);
                            }
                            was_move = true;
                        }
                    }
                }
                let a = t(x, y);
                let b = t(x + dx, y + dy);
                x += dx;
                y += dy;
                shape.records[i] = Record::Straight { dx: b.0 - a.0, dy: b.1 - a.1 };
            }
            Record::Curved { cdx, cdy, adx, ady } => {
                if !was_move {
                    if let Some(li) = last_style {
                        let p = t(x, y);
                        if p != (0, 0) {
                            if let Record::StyleChange { move_to, .. } = &mut shape.records[li] {
                                *move_to = Some(p);
                            }
                            was_move = true;
                        }
                    }
                }
                let a = t(x, y);
                let c = t(x + cdx, y + cdy);
                let e = t(x + cdx + adx, y + cdy + ady);
                x += cdx + adx;
                y += cdy + ady;
                shape.records[i] = Record::Curved { cdx: c.0 - a.0, cdy: c.1 - a.1, adx: e.0 - c.0, ady: e.1 - c.1 };
            }
        }
    }
}
