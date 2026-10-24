//! SVG document assembly and the shape → SVG mapping.

use std::collections::HashMap;
use std::fmt::Write;

use crate::filters::Filter;
use crate::shape::{Fill, Gradient, LineStyle};
use crate::types::{ColorTransform, Matrix, Rgba};

use super::paths::{Edge, Paths, Seg};

/// Pixel value from twips, rounded to 1/100 px.
pub fn px(twips: f64) -> f64 {
    (twips / 20.0 * 100.0).round() / 100.0
}

fn num(v: f64) -> String {
    let v = (v * 10000.0).round() / 10000.0;
    if v == v.trunc() && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

/// `matrix(...)` with translation converted from twips; `scale_div` divides the linear part.
pub fn matrix_attr(m: &Matrix, scale_div: f64) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        num(m.a / scale_div),
        num(m.b / scale_div),
        num(m.c / scale_div),
        num(m.d / scale_div),
        num(m.tx / 20.0),
        num(m.ty / 20.0)
    )
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn color_hex(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn opacity(c: Rgba) -> Option<String> {
    (c.a != 255).then(|| num(c.a as f64 / 255.0))
}

/// Collects `<defs>` entries (deduplicated) while the body is written.
#[derive(Default)]
pub struct Doc {
    defs: String,
    def_ids: HashMap<String, String>,
    next_id: usize,
}

impl Doc {
    pub fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    /// Adds a definition whose markup contains `{id}`, returning the id; identical
    /// definitions share one id.
    pub fn def(&mut self, prefix: &str, markup: &str) -> String {
        if let Some(id) = self.def_ids.get(markup) {
            return id.clone();
        }
        let id = self.id(prefix);
        self.defs.push_str(&markup.replace("{id}", &id));
        self.defs.push('\n');
        self.def_ids.insert(markup.to_string(), id.clone());
        id
    }

    pub fn raw_def(&mut self, markup: &str) {
        self.defs.push_str(markup);
        self.defs.push('\n');
    }

    /// Wraps a body into a complete SVG of `width` × `height` px.
    pub fn finish(self, width: f64, height: f64, body: &str) -> String {
        let mut s = String::new();
        let _ = write!(
            s,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}px\" height=\"{}px\" viewBox=\"0 0 {} {}\">\n",
            num(width.max(1.0)),
            num(height.max(1.0)),
            num(width.max(1.0)),
            num(height.max(1.0))
        );
        if !self.defs.is_empty() {
            s.push_str("<defs>\n");
            s.push_str(&self.defs);
            s.push_str("</defs>\n");
        }
        s.push_str(body);
        s.push_str("</svg>\n");
        s
    }
}

/// Supplies bitmap fills: `(data URI, width, height)` per character id.
pub trait Bitmaps {
    fn bitmap(&self, id: u16) -> Option<(String, u32, u32)>;
}

pub struct ShapeStyle<'a> {
    pub cxform: &'a ColorTransform,
    pub nonzero: bool,
    pub bitmaps: &'a dyn Bitmaps,
    /// Scale of the shape on screen; strokes stay at least 1 px wide there.
    pub stroke_scale: f64,
}

fn seg_d(d: &mut String, s: &Seg, pos: &mut Option<(i32, i32)>, cmd: &mut char) {
    if *pos != Some(s.from) {
        let _ = write!(d, "M{} {} ", num(px(s.from.0 as f64)), num(px(s.from.1 as f64)));
        *cmd = 'M';
    }
    match s.ctrl {
        Some(c) => {
            if *cmd != 'Q' {
                d.push('Q');
                *cmd = 'Q';
            }
            let _ = write!(d, "{} {} {} {} ", num(px(c.0 as f64)), num(px(c.1 as f64)), num(px(s.to.0 as f64)), num(px(s.to.1 as f64)));
        }
        None => {
            if *cmd != 'L' {
                d.push('L');
                *cmd = 'L';
            }
            let _ = write!(d, "{} {} ", num(px(s.to.0 as f64)), num(px(s.to.1 as f64)));
        }
    }
    *pos = Some(s.to);
}

/// Path data for a run of edges; `alt` uses the edges' second (morph end) coordinates.
pub fn path_data(edges: &[Edge], alt: bool) -> String {
    let mut d = String::new();
    let mut pos = None;
    let mut cmd = ' ';
    for e in edges {
        let s = if alt { e.alt.unwrap_or(e.seg) } else { e.seg };
        seg_d(&mut d, &s, &mut pos, &mut cmd);
    }
    d.trim_end().to_string()
}

fn gradient_def(doc: &mut Doc, kind: &str, g: &Gradient, cx: &ColorTransform) -> String {
    let mut m = format!("<{kind}Gradient id=\"{{id}}\" gradientUnits=\"userSpaceOnUse\"");
    if kind == "linear" {
        m.push_str(" x1=\"-819.2\" y1=\"0\" x2=\"819.2\" y2=\"0\"");
    } else {
        m.push_str(" cx=\"0\" cy=\"0\" r=\"819.2\"");
        if g.focal != 0.0 {
            let _ = write!(m, " fx=\"{}\" fy=\"0\"", num(819.2 * g.focal));
        }
    }
    m.push_str(match g.spread {
        1 => " spreadMethod=\"reflect\"",
        2 => " spreadMethod=\"repeat\"",
        _ => " spreadMethod=\"pad\"",
    });
    let _ = write!(m, " gradientTransform=\"{}\">", matrix_attr(&g.matrix, 1.0));
    let stops: Vec<(f64, Rgba)> = g.records.iter().map(|r| (r.ratio as f64 / 255.0, cx.apply(r.color))).collect();
    // Renderers rarely honour color-interpolation="linearRGB", so bake it into extra stops.
    let stops = if g.interpolation == 1 { linear_rgb_stops(&stops) } else { stops };
    for (offset, c) in stops {
        let _ = write!(m, "<stop offset=\"{}\" stop-color=\"{}\"", num(offset), color_hex(c));
        if let Some(o) = opacity(c) {
            let _ = write!(m, " stop-opacity=\"{o}\"");
        }
        m.push_str("/>");
    }
    let _ = write!(m, "</{kind}Gradient>");
    doc.def("gradient", &m)
}

fn to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(l: f64) -> u8 {
    let c = if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

fn linear_rgb_stops(stops: &[(f64, Rgba)]) -> Vec<(f64, Rgba)> {
    const STEPS: usize = 16;
    let mut out = Vec::with_capacity(stops.len() * STEPS);
    for w in stops.windows(2) {
        let ((o0, a), (o1, b)) = (w[0], w[1]);
        for i in 0..STEPS {
            let t = i as f64 / STEPS as f64;
            let ch = |x: u8, y: u8| to_srgb(to_linear(x) + (to_linear(y) - to_linear(x)) * t);
            let alpha = (a.a as f64 + (b.a as f64 - a.a as f64) * t).round() as u8;
            out.push((o0 + (o1 - o0) * t, Rgba { r: ch(a.r, b.r), g: ch(a.g, b.g), b: ch(a.b, b.b), a: alpha }));
        }
    }
    if let Some(last) = stops.last() {
        out.push(*last);
    }
    out
}

fn pattern_def(doc: &mut Doc, id: u16, matrix: &Matrix, smooth: bool, bitmaps: &dyn Bitmaps) -> Option<String> {
    let (uri, w, h) = bitmaps.bitmap(id)?;
    let rendering = if smooth { "optimizeQuality" } else { "optimizeSpeed" };
    let m = format!(
        "<pattern id=\"{{id}}\" patternUnits=\"userSpaceOnUse\" overflow=\"visible\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" patternTransform=\"{}\"><image width=\"{w}\" height=\"{h}\" image-rendering=\"{rendering}\" xlink:href=\"{uri}\"/></pattern>",
        matrix_attr(matrix, 20.0)
    );
    Some(doc.def("pattern", &m))
}

/// The `fill="…"` (or `stroke`) paint for a fill style plus an optional filter for
/// colour-transformed bitmaps.
fn paint(doc: &mut Doc, fill: &Fill, st: &ShapeStyle, attr: &str) -> (String, Option<String>) {
    match fill {
        Fill::Solid(c) => {
            let c = st.cxform.apply(*c);
            let mut s = format!(" {attr}=\"{}\"", color_hex(c));
            if let Some(o) = opacity(c) {
                let _ = write!(s, " {attr}-opacity=\"{o}\"");
            }
            (s, None)
        }
        Fill::Linear(g) => (format!(" {attr}=\"url(#{})\"", gradient_def(doc, "linear", g, st.cxform)), None),
        Fill::Radial(g) | Fill::Focal(g) => (format!(" {attr}=\"url(#{})\"", gradient_def(doc, "radial", g, st.cxform)), None),
        Fill::Bitmap { id, matrix, smooth, .. } => match pattern_def(doc, *id, matrix, *smooth, st.bitmaps) {
            Some(p) => (format!(" {attr}=\"url(#{p})\""), color_filter(doc, st.cxform)),
            None => (format!(" {attr}=\"#ff0000\""), None),
        },
    }
}

/// An feColorMatrix filter equivalent to a colour transform, or None for identity.
pub fn color_filter(doc: &mut Doc, cx: &ColorTransform) -> Option<String> {
    if cx.is_identity() {
        return None;
    }
    let m = |i: usize| num(cx.mult[i] as f64 / 256.0);
    let a = |i: usize| num(cx.add[i] as f64 / 255.0);
    let values = format!(
        "{} 0 0 0 {} 0 {} 0 0 {} 0 0 {} 0 {} 0 0 0 {} {}",
        m(0),
        a(0),
        m(1),
        a(1),
        m(2),
        a(2),
        m(3),
        a(3)
    );
    Some(doc.def(
        "cxform",
        &format!("<filter id=\"{{id}}\" color-interpolation-filters=\"sRGB\"><feColorMatrix type=\"matrix\" values=\"{values}\"/></filter>"),
    ))
}

fn line_attrs(l: &LineStyle, scale: f64) -> String {
    let mut s = String::new();
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let width = (l.width as f64).max(20.0 / scale);
    let _ = write!(s, " stroke-width=\"{}\"", num(width / 20.0));
    s.push_str(match l.start_cap {
        1 => " stroke-linecap=\"butt\"",
        2 => " stroke-linecap=\"square\"",
        _ => " stroke-linecap=\"round\"",
    });
    match l.join {
        1 => s.push_str(" stroke-linejoin=\"bevel\""),
        2 => {
            let _ = write!(s, " stroke-linejoin=\"miter\" stroke-miterlimit=\"{}\"", num(l.miter_limit.max(1.0)));
        }
        _ => s.push_str(" stroke-linejoin=\"round\""),
    }
    if l.no_h_scale && l.no_v_scale {
        s.push_str(" vector-effect=\"non-scaling-stroke\"");
    }
    s
}

fn runs<F: Fn(&Edge) -> usize>(edges: &[Edge], key: F) -> Vec<(usize, &[Edge])> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=edges.len() {
        if i == edges.len() || key(&edges[i]) != key(&edges[start]) {
            out.push((key(&edges[start]), &edges[start..i]));
            start = i;
        }
    }
    out
}

/// The `<path>` elements of a shape. `animate_to` adds SMIL animation to the edges' alt
/// coordinates (morph shapes) using `end` styles.
pub fn shape_body(doc: &mut Doc, paths: &Paths, st: &ShapeStyle, morph_end: Option<(&Paths, f64)>) -> String {
    let rule = if st.nonzero { "nonzero" } else { "evenodd" };
    let mut out = String::new();
    for (li, layer) in paths.layers.iter().enumerate() {
        for (fill, edges) in runs(&layer.fills, |e| e.fill) {
            if edges.is_empty() {
                continue;
            }
            let (paint_attr, filter) = match paths.fills.get(fill.wrapping_sub(1)) {
                Some(f) => paint(doc, f, st, "fill"),
                None => (" fill=\"#000000\"".to_string(), None),
            };
            let d = path_data(edges, false);
            let _ = write!(out, "<path fill-rule=\"{rule}\" stroke=\"none\"{paint_attr}");
            if let Some(f) = filter {
                let _ = write!(out, " filter=\"url(#{f})\"");
            }
            match morph_end {
                Some((end, dur)) => {
                    let _ = write!(out, " d=\"{d}\">");
                    let _ = write!(out, "<animate attributeName=\"d\" dur=\"{dur}s\" repeatCount=\"indefinite\" values=\"{d};{}\"/>", path_data(edges, true));
                    if let (Some(Fill::Solid(a)), Some(Fill::Solid(b))) = (paths.fills.get(fill - 1), end.fills.get(fill - 1)) {
                        let (a, b) = (st.cxform.apply(*a), st.cxform.apply(*b));
                        if a != b {
                            let _ = write!(out, "<animate attributeName=\"fill\" dur=\"{dur}s\" repeatCount=\"indefinite\" values=\"{};{}\"/>", color_hex(a), color_hex(b));
                            let _ = write!(out, "<animate attributeName=\"fill-opacity\" dur=\"{dur}s\" repeatCount=\"indefinite\" values=\"{};{}\"/>", num(a.a as f64 / 255.0), num(b.a as f64 / 255.0));
                        }
                    }
                    out.push_str("</path>\n");
                }
                None => {
                    let _ = writeln!(out, " d=\"{d}\"/>");
                }
            }
        }
        for (line, edges) in runs(&layer.lines, |e| e.line) {
            let style = paths.lines.get(line.wrapping_sub(1)).cloned().unwrap_or_else(|| LineStyle::simple(20, Rgba::BLACK));
            let mut d = path_data(edges, false);
            let first = edges[0].seg.from;
            let last = edges[edges.len() - 1].seg.to;
            let close = !style.no_close && first == last && edges.iter().zip(edges.iter().skip(1)).all(|(a, b)| a.seg.to == b.seg.from);
            if close {
                d.push_str(" Z");
            }
            let (stroke, filter) = match &style.fill {
                Some(f) if !matches!(f, Fill::Solid(_)) => paint(doc, f, st, "stroke"),
                _ => paint(doc, &Fill::Solid(style.color), st, "stroke"),
            };
            let _ = write!(out, "<path fill=\"none\"{stroke}{}", line_attrs(&style, st.stroke_scale));
            if let Some(f) = filter {
                let _ = write!(out, " filter=\"url(#{f})\"");
            }
            match morph_end {
                Some((end, dur)) => {
                    let mut d2 = path_data(edges, true);
                    if close {
                        d2.push_str(" Z");
                    }
                    let _ = write!(out, " d=\"{d}\">");
                    let _ = write!(out, "<animate attributeName=\"d\" dur=\"{dur}s\" repeatCount=\"indefinite\" values=\"{d};{d2}\"/>");
                    if let Some(es) = end.lines.get(line.wrapping_sub(1)) {
                        if es.width != style.width {
                            let _ = write!(
                                out,
                                "<animate attributeName=\"stroke-width\" dur=\"{dur}s\" repeatCount=\"indefinite\" values=\"{};{}\"/>",
                                num(style.width as f64 / 20.0),
                                num(es.width as f64 / 20.0)
                            );
                        }
                    }
                    out.push_str("</path>\n");
                }
                None => {
                    let _ = writeln!(out, " d=\"{d}\"/>");
                }
            }
        }
        let _ = li;
    }
    out
}

/// Fill geometry only, for use inside `<clipPath>`; each path carries `transform`.
pub fn clip_body(paths: &Paths, nonzero: bool, transform: &Matrix) -> String {
    let rule = if nonzero { "nonzero" } else { "evenodd" };
    let mut out = String::new();
    for layer in &paths.layers {
        for (_, edges) in runs(&layer.fills, |e| e.fill) {
            let _ = writeln!(out, "<path clip-rule=\"{rule}\" transform=\"{}\" d=\"{}\"/>", matrix_attr(transform, 1.0), path_data(edges, false));
        }
    }
    out
}

/// An SVG filter chain for Flash filters.
pub fn filters_def(doc: &mut Doc, filters: &[Filter]) -> Option<String> {
    let mut body = String::new();
    let mut n = 0;
    let res = |n: &mut usize| {
        *n += 1;
        format!("r{n}")
    };
    let mut input = "SourceGraphic".to_string();
    for f in filters {
        match f {
            Filter::Blur { blur_x, blur_y, passes } => {
                input = blur(&mut body, &input, *blur_x, *blur_y, *passes, &mut || res(&mut n));
            }
            Filter::DropShadow(s) | Filter::Glow(s) => {
                let is_glow = matches!(f, Filter::Glow(_));
                let (dx, dy) = if is_glow { (0.0, 0.0) } else { (s.distance * s.angle.cos(), s.distance * s.angle.sin()) };
                let off = res(&mut n);
                let _ = write!(body, "<feOffset in=\"{input}\" dx=\"{}\" dy=\"{}\" result=\"{off}\"/>", num(dx), num(dy));
                let mut cur;
                let c = s.color;
                if s.inner {
                    let flood = res(&mut n);
                    let _ = write!(body, "<feFlood flood-color=\"{}\" flood-opacity=\"{}\" result=\"{flood}\"/>", color_hex(c), num(c.a as f64 / 255.0));
                    cur = res(&mut n);
                    let _ = write!(body, "<feComposite in=\"{flood}\" in2=\"{off}\" operator=\"out\" result=\"{cur}\"/>");
                } else {
                    cur = res(&mut n);
                    let _ = write!(
                        body,
                        "<feColorMatrix in=\"{off}\" type=\"matrix\" values=\"0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} 0 0 0 {} 0\" result=\"{cur}\"/>",
                        num(c.r as f64 / 255.0),
                        num(c.g as f64 / 255.0),
                        num(c.b as f64 / 255.0),
                        num(c.a as f64 / 255.0)
                    );
                }
                cur = blur(&mut body, &cur, s.blur_x, s.blur_y, s.passes, &mut || res(&mut n));
                if s.strength != 1.0 {
                    let r = res(&mut n);
                    let _ = write!(body, "<feColorMatrix in=\"{cur}\" type=\"matrix\" values=\"1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 {} 0\" result=\"{r}\"/>", num(s.strength));
                    cur = r;
                }
                let r = res(&mut n);
                // Glows always composite with the source.
                let composite = is_glow || s.composite_source;
                let op = if s.inner {
                    if s.knockout || !composite { ("in", &cur, &input) } else { ("atop", &cur, &input) }
                } else if s.knockout {
                    ("out", &cur, &input)
                } else {
                    ("over", &input, &cur)
                };
                if s.inner || s.knockout || composite {
                    let _ = write!(body, "<feComposite in=\"{}\" in2=\"{}\" operator=\"{}\" result=\"{r}\"/>", op.1, op.2, op.0);
                    input = r;
                } else {
                    input = cur;
                }
            }
            Filter::ColorMatrix(m) => {
                let r = res(&mut n);
                let vals: Vec<String> = m.iter().enumerate().map(|(i, v)| num(if i % 5 == 4 { *v as f64 / 255.0 } else { *v as f64 })).collect();
                let _ = write!(body, "<feColorMatrix in=\"{input}\" type=\"matrix\" values=\"{}\" result=\"{r}\"/>", vals.join(" "));
                input = r;
            }
            Filter::Convolution { cols, rows, divisor, bias, matrix, preserve_alpha, .. } => {
                if *cols == 0 || *rows == 0 {
                    continue;
                }
                let r = res(&mut n);
                let vals: Vec<String> = matrix.iter().map(|v| num(*v as f64)).collect();
                let _ = write!(
                    body,
                    "<feConvolveMatrix in=\"{input}\" order=\"{cols} {rows}\" kernelMatrix=\"{}\" divisor=\"{}\" bias=\"{}\" preserveAlpha=\"{preserve_alpha}\" result=\"{r}\"/>",
                    vals.join(" "),
                    num(if *divisor == 0.0 { 1.0 } else { *divisor as f64 }),
                    num(*bias as f64 / 255.0)
                );
                input = r;
            }
            Filter::Bevel(b) | Filter::GradientBevel(b) => {
                let mut next = || {
                    n += 1;
                    format!("r{n}")
                };
                input = gradient_bevel(&mut body, &input, b, &mut next);
            }
            Filter::GradientGlow(b) => {
                let mut next = || {
                    n += 1;
                    format!("r{n}")
                };
                input = gradient_glow(&mut body, &input, b, &mut next);
            }
        }
    }
    if body.is_empty() {
        return None;
    }
    Some(doc.def(
        "filter",
        &format!("<filter id=\"{{id}}\" x=\"-100%\" y=\"-100%\" width=\"300%\" height=\"300%\" color-interpolation-filters=\"sRGB\">{body}</filter>"),
    ))
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Inner,
    Outer,
    Full,
}

fn bevel_kind(b: &crate::filters::Bevel) -> Kind {
    if b.on_top && !b.inner {
        Kind::Full
    } else if !b.inner {
        Kind::Outer
    } else {
        Kind::Inner
    }
}

/// `SvgFiltering.compose`.
fn compose(body: &mut String, src: &str, result: String, kind: Kind, knockout: bool, composite_source: bool, res: &mut dyn FnMut() -> String) -> String {
    let (a, b, op) = match kind {
        Kind::Full if !knockout && composite_source => (result.as_str(), src, "over"),
        Kind::Inner => (result.as_str(), src, if knockout || !composite_source { "in" } else { "atop" }),
        Kind::Outer if knockout => (result.as_str(), src, "out"),
        Kind::Outer if composite_source => (src, result.as_str(), "over"),
        Kind::Outer => ("", "", ""),
        _ => return result,
    };
    composite(body, a, b, op, res)
}

fn flood_black(body: &mut String, res: &mut dyn FnMut() -> String) -> String {
    let r = res();
    let _ = write!(body, "<feFlood flood-color=\"black\" flood-opacity=\"1\" result=\"{r}\"/>");
    r
}

fn composite(body: &mut String, a: &str, b: &str, op: &str, res: &mut dyn FnMut() -> String) -> String {
    let r = res();
    let _ = write!(body, "<feComposite in=\"{a}\" in2=\"{b}\" operator=\"{op}\" result=\"{r}\"/>");
    r
}

fn solid_matrix(body: &mut String, input: &str, c: (f64, f64, f64, f64), res: &mut dyn FnMut() -> String) -> String {
    let r = res();
    let _ = write!(body, "<feColorMatrix type=\"matrix\" in=\"{input}\" values=\"0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} 0 0 0 {} 0\" result=\"{r}\"/>", num(c.0), num(c.1), num(c.2), num(c.3));
    r
}

/// `SvgFiltering.dropShadow` without blur, as the bevels use it.
#[allow(clippy::too_many_arguments)]
fn drop_shadow(body: &mut String, input: &str, distance: f64, angle: f64, color: (f64, f64, f64, f64), inner: bool, strength: f64, res: &mut dyn FnMut() -> String) -> String {
    let off = res();
    let _ = write!(body, "<feOffset dx=\"{}\" dy=\"{}\" in=\"{input}\" result=\"{off}\"/>", num(distance * angle.cos()), num(distance * angle.sin()));
    let mut cur = if inner {
        let flood = res();
        let hex = format!("#{:02x}{:02x}{:02x}", (color.0 * 255.0) as u8, (color.1 * 255.0) as u8, (color.2 * 255.0) as u8);
        let _ = write!(body, "<feFlood flood-color=\"{hex}\" flood-opacity=\"{}\" result=\"{flood}\"/>", num(color.3));
        composite(body, &flood, &off, "out", res)
    } else {
        solid_matrix(body, &off, color, res)
    };
    if strength != 1.0 {
        let r = res();
        let _ = write!(body, "<feColorMatrix type=\"matrix\" in=\"{cur}\" values=\"1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 {} 0\" result=\"{r}\"/>", num(strength));
        cur = r;
    }
    compose(body, input, cur, if inner { Kind::Inner } else { Kind::Outer }, true, true, res)
}

/// `GradientUtil.colorAt` in sRGB.
fn color_at(colors: &[crate::types::Rgba], ratios: &[u8], k: f32) -> [u8; 4] {
    let c = |x: &crate::types::Rgba| [x.r, x.g, x.b, x.a];
    let n = ratios.len();
    if n == 0 || colors.len() != n {
        return [0; 4];
    }
    if k <= ratios[0] as f32 {
        return c(&colors[0]);
    }
    if k >= ratios[n - 1] as f32 {
        return c(&colors[n - 1]);
    }
    let mut i = 0;
    while i < n - 1 && k > ratios[i + 1] as f32 {
        i += 1;
    }
    let (p0, p1) = (ratios[i] as f32, ratios[i + 1] as f32);
    if p1 == p0 {
        return c(&colors[i + 1]);
    }
    let t = (k - p0) / (p1 - p0);
    let (a, b) = (c(&colors[i]), c(&colors[i + 1]));
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t + 0.5).floor().clamp(0.0, 255.0) as u8;
    [lerp(a[0], b[0]), lerp(a[1], b[1]), lerp(a[2], b[2]), lerp(a[3], b[3])]
}

/// `prepareFeComponentTransfer`: maps the red channel through the gradient.
fn gradient_transfer(body: &mut String, input: &str, b: &crate::filters::Bevel, res: &mut dyn FnMut() -> String) -> String {
    let table: Vec<[u8; 4]> = (0..256).map(|i| color_at(&b.colors, &b.ratios, i as f32)).collect();
    let vals = |ch: usize| table.iter().map(|c| num(c[ch] as f64 / 255.0)).collect::<Vec<_>>().join(" ");
    let r = res();
    let _ = write!(
        body,
        "<feComponentTransfer in=\"{input}\" result=\"{r}\"><feFuncR type=\"table\" tableValues=\"{}\"/><feFuncG type=\"table\" tableValues=\"{}\"/><feFuncB type=\"table\" tableValues=\"{}\"/><feFuncA type=\"table\" tableValues=\"{}\"/></feComponentTransfer>",
        vals(0),
        vals(1),
        vals(2),
        vals(3)
    );
    r
}

/// `SvgFiltering.gradientGlow`.
fn gradient_glow(body: &mut String, input: &str, b: &crate::filters::Bevel, res: &mut dyn FnMut() -> String) -> String {
    let black = flood_black(body, res);
    let off = res();
    let _ = write!(body, "<feOffset dx=\"{}\" dy=\"{}\" in=\"{input}\" result=\"{off}\"/>", num(b.distance * b.angle.cos()), num(b.distance * b.angle.sin()));
    let red = solid_matrix(body, &off, (1.0, 0.0, 0.0, 1.0), res);
    let mut cur = composite(body, &red, &black, "over", res);
    cur = blur(body, &cur, b.blur_x, b.blur_y, b.passes, res);
    let row = format!("{} 0 0 0 0", num(b.strength));
    let r = res();
    let _ = write!(body, "<feColorMatrix type=\"matrix\" in=\"{cur}\" values=\"{row} {row} {row} {row}\" result=\"{r}\"/>");
    let cur = gradient_transfer(body, &r, b, res);
    compose(body, input, cur, bevel_kind(b), b.knockout, b.composite_source, res)
}

/// `SvgFiltering.gradientBevel`, which also draws the plain bevel.
fn gradient_bevel(body: &mut String, input: &str, b: &crate::filters::Bevel, res: &mut dyn FnMut() -> String) -> String {
    let (red, blue) = ((1.0, 0.0, 0.0, 1.0), (0.0, 0.0, 1.0, 1.0));
    let kind = bevel_kind(b);
    let pi = std::f64::consts::PI;
    let pair = |body: &mut String, inner: bool, res: &mut dyn FnMut() -> String| {
        let (ha, sa) = if inner { (b.angle, b.angle + pi) } else { (b.angle + pi, b.angle) };
        let hilight = drop_shadow(body, input, b.distance, ha, red, inner, b.strength, res);
        let shadow = drop_shadow(body, input, b.distance, sa, blue, inner, b.strength, res);
        let h = composite(body, &hilight, &shadow, "out", res);
        let s = composite(body, &shadow, &hilight, "out", res);
        (h, s)
    };
    let inner = (kind != Kind::Outer).then(|| pair(body, true, res));
    let outer = (kind != Kind::Inner).then(|| pair(body, false, res));
    let (hilight, shadow) = match (inner, outer) {
        (Some((hi, si)), Some((ho, so))) => (composite(body, &hi, &ho, "over", res), composite(body, &si, &so, "over", res)),
        (Some(p), None) | (None, Some(p)) => p,
        (None, None) => unreachable!(),
    };
    let black = flood_black(body, res);
    let cur = composite(body, &shadow, &black, "over", res);
    let mut cur = composite(body, &hilight, &cur, "over", res);
    cur = blur(body, &cur, b.blur_x, b.blur_y, b.passes, res);
    let half = b.strength / 2.0;
    let row = format!("{} 0 {} 0 0.5", num(half), num(-half));
    let r = res();
    let _ = write!(body, "<feColorMatrix type=\"matrix\" in=\"{cur}\" values=\"{row} {row} {row} {row}\" result=\"{r}\"/>");
    let cur = gradient_transfer(body, &r, b, res);
    compose(body, input, cur, kind, b.knockout, b.composite_source, res)
}

fn blur(body: &mut String, input: &str, bx: f64, by: f64, passes: u32, res: &mut dyn FnMut() -> String) -> String {
    if passes == 0 || (bx == 0.0 && by == 0.0) {
        return input.to_string();
    }
    let dev = |b: f64| ((b * b - 1.0).max(0.0) / 12.0).sqrt();
    let mut cur = input.to_string();
    for _ in 0..passes {
        let r = res();
        let _ = write!(body, "<feGaussianBlur in=\"{cur}\" stdDeviation=\"{} {}\" result=\"{r}\"/>", num(dev(bx)), num(dev(by)));
        cur = r;
    }
    cur
}

pub fn blend_style(mode: u8) -> Option<&'static str> {
    Some(match mode {
        3 => "multiply",
        4 => "screen",
        5 => "lighten",
        6 => "darken",
        7 => "difference",
        13 => "overlay",
        14 => "hard-light",
        _ => return None,
    })
}
