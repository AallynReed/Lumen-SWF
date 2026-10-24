//! Shapes in XFL: `XFLConverter.convertShape`, its edge strings, fill and stroke styles.

use super::fixer::{self, Adv};
use super::model::{DMatrix, JMatrix};
use super::num::{java_double, java_float};
use super::xml::XmlWriter;
use super::Converter;
use crate::shape::{Fill, Gradient, LineStyle, Shape};
use crate::types::{Matrix, Rgba};

pub const KEY_MODE_SHAPE_LAYERS: i32 = 8192;
const SMALL_DIVISOR: f64 = 20.0;

pub fn hex(c: &Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

pub fn alpha_f(c: &Rgba) -> f32 {
    c.a as f32 / 255.0
}

/// A parsed matrix (float scale and skew, integer translation).
pub fn jmatrix(m: &Matrix) -> JMatrix {
    JMatrix {
        has_scale: m.a != 1.0 || m.d != 1.0,
        scale_x: m.a as f32,
        scale_y: m.d as f32,
        has_rotate: m.b != 0.0 || m.c != 0.0,
        rotate0: m.b as f32,
        rotate1: m.c as f32,
        tx: m.tx as i32,
        ty: m.ty as i32,
    }
}

/// `numEdgeToString`: integers as decimals, fractions as `#hex.ff`.
fn num_edge(v: f64) -> String {
    if v == v.floor() {
        return (v as i64).to_string();
    }
    let int = v.floor() as i64;
    let frac = v - int as f64;
    let f256 = (frac * 256.0).floor() as i32;
    let mut h = format!("{:X}", int);
    if int < 0 {
        h = format!("{:X}", int as u64);
    }
    if h.len() > 6 {
        h = h[h.len() - 6..].to_string();
    }
    format!("#{h}.{f256:02X}")
}

fn edge(m: &DMatrix, rec: &Adv, x: f64, y: f64) -> String {
    match rec {
        Adv::Style(s) => match s.move_to {
            Some((mx, my)) => {
                let (px, py) = m.transform(mx, my);
                format!("! {} {}", num_edge(px), num_edge(py))
            }
            None => String::new(),
        },
        Adv::Straight { dx, dy } => {
            let (px, py) = m.transform(x + dx, y + dy);
            format!("| {} {}", num_edge(px), num_edge(py))
        }
        Adv::Curved { cdx, cdy, adx, ady } => {
            let cx = cdx + x;
            let cy = cdy + y;
            let (ccx, ccy) = m.transform(cx, cy);
            let (ax, ay) = m.transform(adx + cx, ady + cy);
            format!("[ {} {} {} {}", num_edge(ccx), num_edge(ccy), num_edge(ax), num_edge(ay))
        }
    }
}

/// `convertShapeEdges`.
fn edges_str(close: bool, start_x: f64, start_y: f64, m: &DMatrix, recs: &[Adv]) -> String {
    let mut ret = String::new();
    let (mut x, mut y) = (start_x, start_y);
    let has_move = matches!(recs.first(), Some(Adv::Style(s)) if s.move_to.is_some());
    if !has_move {
        ret.push_str(&format!("! {} {}", num_edge(start_x), num_edge(start_y)));
    }
    let (mut lmx, mut lmy) = (start_x, start_y);
    let (mut f0, mut f1, mut ln) = (0, 0, 0);
    let mut seen: Vec<String> = Vec::new();
    for rec in recs {
        if let Adv::Style(s) = rec {
            if let Some((mx, my)) = s.move_to {
                lmx = mx;
                lmy = my;
            }
            if s.new_styles.is_some() {
                f0 = 0;
                f1 = 0;
                ln = 0;
            }
            if let Some(v) = s.line {
                ln = v;
            }
            if let Some(v) = s.fill0 {
                f0 = v;
            }
            if let Some(v) = s.fill1 {
                f1 = v;
            }
        }
        let e = edge(m, rec, x, y);
        let cur = format!("! {} {}{e}", num_edge(x), num_edge(y));
        if f0 == 0 && f1 == 0 && ln != 0 {
            if !seen.contains(&cur) {
                seen.push(cur);
                ret.push_str(&e);
            }
        } else {
            seen.push(cur);
            ret.push_str(&e);
        }
        x = rec.change_x(x);
        y = rec.change_y(y);
    }
    if close && (lmx.total_cmp(&x).is_ne() || lmy.total_cmp(&y).is_ne()) {
        ret.push_str(&edge(m, &Adv::Straight { dx: lmx - x, dy: lmy - y }, x, y));
    }
    ret
}

fn scale_mode(ls: &LineStyle, v2: bool) -> &'static str {
    if !v2 {
        return "normal";
    }
    match (ls.no_h_scale, ls.no_v_scale) {
        (true, true) => "none",
        (true, false) => "vertical",
        (false, true) => "horizontal",
        _ => "normal",
    }
}

/// `convertMatrix`.
pub fn convert_matrix(m: &JMatrix, w: &mut XmlWriter) {
    let d = DMatrix::of(m);
    w.start("Matrix");
    w.attr_d("tx", (d.tx as f32) as f64 / 20.0);
    w.attr_d("ty", (d.ty as f32) as f64 / 20.0);
    if d.scale_x != 1.0 || d.scale_y != 1.0 {
        w.attr_d("a", d.scale_x);
        w.attr_d("d", d.scale_y);
    }
    if d.rotate0 != 0.0 || d.rotate1 != 0.0 {
        w.attr_d("b", d.rotate0);
        w.attr_d("c", d.rotate1);
    }
    w.end();
}

impl Converter<'_> {
    fn convert_fill(&mut self, mat: &JMatrix, fs: &Fill, shape_num: u8, w: &mut XmlWriter) {
        match fs {
            Fill::Solid(c) => {
                w.start_with("SolidColor", &[("color", &hex(c))]);
                if shape_num >= 3 {
                    w.attr_f("alpha", alpha_f(c));
                }
                w.end();
            }
            Fill::Bitmap { id, matrix, repeat, smooth } => {
                let Some(path) = self.bitmap_path(*id) else {
                    w.empty_with("SolidColor", &[("color", "#ffffff")]);
                    return;
                };
                w.start("BitmapFill");
                w.attr("bitmapPath", &path);
                if !*repeat {
                    w.attr_bool("bitmapIsClipped", true);
                }
                let _ = smooth;
                w.start("matrix");
                let bm = DMatrix::of(mat).concatenate(&DMatrix::of(&jmatrix(matrix))).to_jmatrix();
                convert_matrix(&bm, w);
                w.end();
                w.end();
            }
            Fill::Linear(g) | Fill::Radial(g) | Fill::Focal(g) => {
                let linear = matches!(fs, Fill::Linear(_));
                if linear {
                    w.start("LinearGradient");
                } else {
                    w.start("RadialGradient");
                    let fp = if matches!(fs, Fill::Focal(_)) { java_float(g.focal as f32) } else { "0".into() };
                    w.attr("focalPointRatio", &fp);
                }
                self.gradient_body(mat, g, shape_num, w);
                w.end();
            }
        }
    }

    fn gradient_body(&mut self, mat: &JMatrix, g: &Gradient, shape_num: u8, w: &mut XmlWriter) {
        if g.interpolation == 1 {
            w.attr("interpolationMethod", "linearRGB");
        }
        match g.spread {
            1 => w.attr("spreadMethod", "reflect"),
            2 => w.attr("spreadMethod", "repeat"),
            _ => {}
        }
        w.start("matrix");
        let gm = DMatrix::of(mat).concatenate(&DMatrix::of(&jmatrix(&g.matrix))).to_jmatrix();
        convert_matrix(&gm, w);
        w.end();
        for rec in &g.records {
            w.start("GradientEntry");
            w.attr("color", &hex(&rec.color));
            if shape_num >= 3 {
                w.attr_f("alpha", alpha_f(&rec.color));
            }
            w.attr_f("ratio", rec.ratio as f32 / 255.0);
            w.end();
        }
    }

    fn convert_line(&mut self, mat: &JMatrix, ls: &LineStyle, shape_num: u8, w: &mut XmlWriter) {
        if shape_num <= 3 {
            w.start_with("SolidStroke", &[("scaleMode", "normal"), ("weight", &java_double(ls.width as f32 as f64 / 20.0))]);
            w.start("fill");
            w.start_with("SolidColor", &[("color", &hex(&ls.color))]);
            if shape_num >= 3 {
                w.attr_f("alpha", alpha_f(&ls.color));
            }
            w.end();
            w.end();
            w.end();
            return;
        }
        w.start_with("SolidStroke", &[("weight", &java_double(ls.width as f32 as f64 / 20.0))]);
        if ls.pixel_hinting {
            w.attr_bool("pixelHinting", true);
        }
        if ls.width == 1 {
            w.attr("solidStyle", "hairline");
        }
        w.attr("scaleMode", scale_mode(ls, true));
        match ls.end_cap {
            1 => w.attr("caps", "none"),
            2 => w.attr("caps", "square"),
            _ => {}
        }
        match ls.join {
            1 => w.attr("joints", "bevel"),
            2 => {
                w.attr("joints", "miter");
                let ml = ls.miter_limit as f32;
                if ml != 3.0 {
                    w.attr_f("miterLimit", ml);
                }
            }
            _ => {}
        }
        w.start("fill");
        match &ls.fill {
            None => {
                w.start_with("SolidColor", &[("color", &hex(&ls.color))]);
                let a = alpha_f(&ls.color);
                if a != 1.0 {
                    w.attr("alpha", &java_float(a));
                }
                w.end();
            }
            Some(f) => self.convert_fill(mat, f, shape_num, w),
        }
        w.end();
        w.end();
    }

    /// `getShapeLayers`: one `DOMShape` per style group.
    #[allow(clippy::too_many_arguments)]
    fn shape_layers(&mut self, mat: Option<&JMatrix>, shape_num: u8, shape: &Shape, morph: bool, small: bool) -> Vec<String> {
        let mat = mat.copied().unwrap_or_default();
        let mut shape = shape.clone();
        if small {
            super::transform::scale_shape(&mut shape, SMALL_DIVISOR, shape_num);
        }
        let recs = fixer::fix(&shape, shape_num, morph, small);
        let dm = DMatrix::of(&mat);
        let mut fills = XmlWriter::new();
        let mut strokes = XmlWriter::new();
        fills.start("fills");
        strokes.start("strokes");
        let mut layers = Vec::new();
        let mut fill_count = 0;
        for fs in &shape.styles.fills {
            fills.start_with("FillStyle", &[("index", &(fill_count + 1).to_string())]);
            self.convert_fill(&mat, fs, shape_num, &mut fills);
            fills.end();
            fill_count += 1;
        }
        let mut line_count = 0;
        for ls in &shape.styles.lines {
            strokes.start_with("StrokeStyle", &[("index", &(line_count + 1).to_string())]);
            self.convert_line(&mat, ls, shape_num, &mut strokes);
            strokes.end();
            line_count += 1;
        }
        fills.end();
        strokes.end();
        let mut has_edge = false;
        let mut cur = XmlWriter::new();
        if fill_count > 0 || line_count > 0 {
            cur.start_with("DOMShape", &[("isFloating", "true")]);
            cur.raw(fills.as_str());
            cur.raw(strokes.as_str());
            cur.start("edges");
        }
        let (mut x, mut y, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
        let (mut f0, mut f1, mut st) = (-1i32, -1i32, -1i32);
        let mut edges: Vec<Adv> = Vec::new();
        let mut fill_count = shape.styles.fills.len();
        let write_edge = |cur: &mut XmlWriter, f0: i32, f1: i32, st: i32, close: bool, sx: f64, sy: f64, edges: &[Adv]| {
            cur.start("Edge");
            if f0 > -1 {
                cur.attr_i("fillStyle0", f0 as i64);
            }
            if f1 > -1 {
                cur.attr_i("fillStyle1", f1 as i64);
            }
            if st > -1 {
                cur.attr_i("strokeStyle", st as i64);
            }
            cur.attr("edges", &edges_str(close, sx, sy, &dm, edges));
            cur.end();
        };
        for rec in &recs {
            if let Adv::Style(s) = rec {
                let (lf0, lf1, lst) = (f0, f1, st);
                if let Some(ns) = &s.new_styles {
                    let mut fills_new = XmlWriter::new();
                    let mut strokes_new = XmlWriter::new();
                    fills_new.start("fills");
                    strokes_new.start("strokes");
                    if (fill_count > 0 || line_count > 0) && (f0 > 0 || f1 > 0 || st > 0) {
                        write_edge(&mut cur, f0, f1, st, (f0 > 0 || f1 > 0) && morph, sx, sy, &edges);
                        has_edge = true;
                        edges.clear();
                    }
                    if !cur.is_empty() {
                        cur.end();
                        cur.end();
                    }
                    if !cur.is_empty() && has_edge {
                        layers.push(cur.to_string());
                    }
                    cur = XmlWriter::new();
                    has_edge = false;
                    cur.start_with("DOMShape", &[("isFloating", "true")]);
                    for (fi, f) in ns.fills.iter().enumerate() {
                        fills_new.start_with("FillStyle", &[("index", &(fi + 1).to_string())]);
                        self.convert_fill(&mat, f, shape_num, &mut fills_new);
                        fills_new.end();
                        fill_count += 1;
                    }
                    line_count = 0;
                    for l in &ns.lines {
                        strokes_new.start_with("StrokeStyle", &[("index", &(line_count + 1).to_string())]);
                        self.convert_line(&mat, l, shape_num, &mut strokes_new);
                        strokes_new.end();
                        line_count += 1;
                    }
                    fills_new.end();
                    strokes_new.end();
                    cur.raw(fills_new.as_str());
                    cur.raw(strokes_new.as_str());
                    cur.start("edges");
                }
                if let Some(v) = s.fill0 {
                    if morph {
                        f1 = v;
                    } else {
                        f0 = v;
                    }
                }
                if let Some(v) = s.fill1 {
                    if morph {
                        f0 = v;
                    } else {
                        f1 = v;
                    }
                }
                if let Some(v) = s.line {
                    st = v;
                }
                if !edges.is_empty() {
                    if f0 > 0 || f1 > 0 || st > 0 {
                        write_edge(&mut cur, lf0, lf1, lst, (lf0 > 0 || lf1 > 0) && morph, sx, sy, &edges);
                        has_edge = true;
                        sx = x;
                        sy = y;
                    }
                    edges.clear();
                }
            }
            edges.push(rec.clone());
            x = rec.change_x(x);
            y = rec.change_y(y);
        }
        if !edges.is_empty() && (f0 > 0 || f1 > 0 || st > 0) {
            write_edge(&mut cur, f0, f1, st, (f0 > 0 || f1 > 0) && morph, sx, sy, &edges);
            has_edge = true;
        }
        if !cur.is_empty() {
            cur.end();
            cur.end();
            if has_edge {
                layers.push(cur.to_string());
            }
        }
        layers
    }

    /// `convertShape`.
    #[allow(clippy::too_many_arguments)]
    pub fn convert_shape(&mut self, mat: Option<&JMatrix>, shape_num: u8, shape: &Shape, morph: bool, use_layers: bool, w: &mut XmlWriter, small: bool) {
        let layers = self.shape_layers(mat, shape_num, shape, morph, small);
        if !use_layers {
            for l in layers.iter().rev() {
                w.raw(l);
            }
        } else {
            for (n, l) in layers.iter().rev().enumerate() {
                w.start_with("DOMLayer", &[("name", &format!("Layer {}", n + 1))]);
                w.start("frames");
                w.start_with("DOMFrame", &[("index", "0"), ("motionTweenScale", "false"), ("keyMode", &KEY_MODE_SHAPE_LAYERS.to_string())]);
                w.start("elements");
                w.raw(l);
                w.end();
                w.end();
                w.end();
                w.end();
            }
        }
    }
}
