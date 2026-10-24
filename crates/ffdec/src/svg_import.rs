//! SVG into a shape; CSS selectors are matched on the document directly.

use std::collections::HashMap;

use resvg::usvg::roxmltree::{self, Document, Node, NodeId, ParsingOptions};

use crate::io::JavaRound;
use crate::error::{Error, Result};
use crate::shape::{bounds, Fill, GradRecord, Gradient, LineStyle, Record, Shape, ShapeTag, Styles};
use crate::types::{Matrix as SwfMatrix, Rgba};
use crate::Swf;

const XLINK: &str = "http://www.w3.org/1999/xlink";

/// A double-precision matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
struct M {
    sx: f64,
    sy: f64,
    r0: f64,
    r1: f64,
    tx: f64,
    ty: f64,
}

impl M {
    const ID: M = M { sx: 1.0, sy: 1.0, r0: 0.0, r1: 0.0, tx: 0.0, ty: 0.0 };

    fn scale(x: f64, y: f64) -> M {
        M { sx: x, sy: y, ..M::ID }
    }

    fn translate(x: f64, y: f64) -> M {
        M { tx: x, ty: y, ..M::ID }
    }

    fn transform(&self, x: f64, y: f64) -> (f64, f64) {
        (self.sx * x + self.r1 * y + self.tx, self.r0 * x + self.sy * y + self.ty)
    }

    fn translate_by(&mut self, x: f64, y: f64) {
        let (tx, ty) = self.transform(x, y);
        (self.tx, self.ty) = (tx, ty);
    }

    fn concat(&self, m: &M) -> M {
        M {
            sx: self.sx * m.sx + self.r1 * m.r0,
            r0: self.r0 * m.sx + self.sy * m.r0,
            r1: self.sx * m.r1 + self.r1 * m.sy,
            sy: self.r0 * m.r1 + self.sy * m.sy,
            tx: self.sx * m.tx + self.r1 * m.ty + self.tx,
            ty: self.r0 * m.tx + self.sy * m.ty + self.ty,
        }
    }

    fn pre_concat(&self, m: &M) -> M {
        m.concat(self)
    }

    /// toMATRIX: scale and rotation through floats, translation truncated.
    fn to_swf(self) -> SwfMatrix {
        let f = |v: f64| v as f32 as f64;
        SwfMatrix { a: f(self.sx), d: f(self.sy), b: f(self.r0), c: f(self.r1), tx: self.tx as i32 as f64, ty: self.ty as i32 as f64 }
    }

    fn from_swf(m: &SwfMatrix) -> M {
        M { sx: m.a, sy: m.d, r0: m.b, r1: m.c, tx: m.tx, ty: m.ty }
    }
}

/// Java's Double.parseDouble.
fn java_double(s: &str) -> Option<f64> {
    let t = s.trim();
    let t = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    if !t.chars().all(|c| c.is_ascii_digit() || "+-.eE".contains(c)) {
        return match t {
            "Infinity" | "+Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "NaN" => Some(f64::NAN),
            _ => None,
        };
    }
    t.parse().ok()
}

fn number_list(params: &str) -> Vec<String> {
    let mut p = params.to_string();
    while p.contains("  ") {
        p = p.replace("  ", " ");
    }
    p.trim().replace(", ", ",").replace(' ', ",").split(',').map(str::to_string).collect()
}

/// Matrix.parseSvgMatrix; None where Java throws.
fn parse_transform(s: Option<&str>, translate_div: f64, unit_div: f64) -> Option<M> {
    let mut ret = M::ID;
    let mut rest = s.unwrap_or("").to_string();
    while !rest.is_empty() {
        let name = rest.split('(').next().unwrap_or("").to_string();
        rest = rest.get(name.len() + 1..)?.to_string();
        let params = rest.split(')').next().unwrap_or("").to_string();
        rest = rest.get(params.len() + 1..).unwrap_or("").trim().to_string();
        let args = number_list(&params);
        let a = |i: usize| java_double(&args[i]);
        let m = match (name.trim(), args.len()) {
            ("matrix", 6) => Some(M { sx: a(0)?, r0: a(1)?, r1: a(2)?, sy: a(3)?, tx: a(4)?, ty: a(5)? }),
            ("translate", 1 | 2) => Some(M::translate(a(0)?, if args.len() == 2 { a(1)? } else { 0.0 })),
            ("scale", 1 | 2) => {
                let x = a(0)?;
                Some(M::scale(x, if args.len() == 2 { a(1)? } else { x }))
            }
            ("skewX", 1) => Some(M { r1: (a(0)? * std::f64::consts::PI / 180.0).tan(), ..M::ID }),
            ("skewY", 1) => Some(M { r0: (a(0)? * std::f64::consts::PI / 180.0).tan(), ..M::ID }),
            ("rotate", 1 | 3) => {
                let angle = a(0)?;
                let (tx, ty) = if args.len() > 1 { (a(1)?, a(2)?) } else { (0.0, 0.0) };
                let rad = -angle * std::f64::consts::PI / 180.0;
                let r = M { r0: -rad.sin(), r1: rad.sin(), sx: rad.cos(), sy: rad.cos(), ..M::ID };
                Some(r.pre_concat(&M::translate(tx, ty)).concat(&M::translate(-tx, -ty)))
            }
            _ => None,
        };
        if let Some(m) = m {
            ret = ret.concat(&m);
        }
    }
    ret.tx *= translate_div;
    ret.ty *= translate_div;
    ret.r0 *= unit_div;
    ret.r1 *= unit_div;
    ret.sx *= unit_div;
    ret.sy *= unit_div;
    Some(ret)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Units {
    UserSpace,
    BoundingBox,
}

#[derive(Debug, Clone)]
struct Stop {
    color: Rgba,
    offset: f64,
}

#[derive(Debug, Clone)]
enum Paint {
    Color(Rgba),
    Transparent,
    Linear(Grad, [String; 4]),
    Radial(Grad, [String; 5]),
    Bitmap { id: u16, transform: Option<String>, smoothed: bool },
}

#[derive(Debug, Clone)]
struct Grad {
    stops: Vec<Stop>,
    units: Units,
    transform: Option<String>,
    spread: u8,
    interpolation: u8,
}

impl Paint {
    fn to_color(&self) -> Rgba {
        match self {
            Paint::Color(c) => *c,
            Paint::Transparent => Rgba { r: 0, g: 0, b: 0, a: 0 },
            Paint::Linear(g, _) | Paint::Radial(g, _) => g.stops.first().map_or(Rgba::BLACK, |s| s.color),
            Paint::Bitmap { .. } => Rgba::BLACK,
        }
    }
}

include!("svg_colors.rs");

fn rgb(r: i64, g: i64, b: i64) -> Option<Rgba> {
    let c = |v: i64| (0..=255).contains(&v).then_some(v as u8);
    Some(Rgba { r: c(r)?, g: c(g)?, b: c(b)?, a: 255 })
}

/// SvgColor.parse; Err where Java throws, Ok(None) where it returns null.
fn parse_color(s: &str) -> std::result::Result<Option<Rgba>, ()> {
    if let Some((_, [r, g, b])) = COLORS.iter().find(|(n, _)| *n == s) {
        return Ok(Some(Rgba { r: *r, g: *g, b: *b, a: 255 }));
    }
    if let Some(h) = s.strip_prefix('#') {
        let h: String = if h.chars().count() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
        let v = i32::from_str_radix(&h, 16).map_err(|_| ())?;
        return Ok(Some(Rgba { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8, a: 255 }));
    }
    if let Some(rest) = s.strip_prefix("rgb") {
        let rest = rest.trim();
        if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
            let args: Vec<&str> = inner.split(',').map(str::trim).collect();
            if args.len() == 3 {
                if args.iter().all(|a| a.ends_with('%')) {
                    let p = |a: &str| java_double(&a[..a.len() - 1]).map(|v| (v * 255.0 / 100.0).jround() as i64).ok_or(());
                    return Ok(rgb(p(args[0])?, p(args[1])?, p(args[2])?));
                }
                let p = |a: &str| a.parse::<i32>().map(|v| v as i64).map_err(|_| ());
                return rgb(p(args[0])?, p(args[1])?, p(args[2])?).ok_or(()).map(Some);
            }
        }
    }
    Ok(None)
}

fn unknown_fill() -> Paint {
    Paint::Color(Rgba { r: 128, g: 128, b: 128, a: 255 })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Join {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Color(Rgba),
    Paint(Box<Paint>),
    Num(f64),
    Str(String),
    Cap(Cap),
    Join(Join),
    /// 0 auto, 1 optimizeSpeed, 2 optimizeQuality.
    Rendering(u8),
    None,
}

impl PartialEq for Paint {
    fn eq(&self, _: &Paint) -> bool {
        false
    }
}

const PROPS: &[(&str, bool)] = &[
    ("color", true),
    ("fill", true),
    ("fill-opacity", true),
    ("fill-rule", true),
    ("stroke", true),
    ("stroke-width", true),
    ("stroke-opacity", true),
    ("stroke-linecap", true),
    ("stroke-linejoin", true),
    ("stroke-miterlimit", true),
    ("opacity", false),
    ("stop-color", false),
    ("stop-opacity", false),
    ("vector-effect", false),
    ("image-rendering", true),
];

fn initial(name: &str) -> Value {
    match name {
        "color" | "stop-color" => Value::Color(Rgba::BLACK),
        "fill" => Value::Paint(Box::new(Paint::Color(Rgba::BLACK))),
        "fill-opacity" | "stroke-width" | "stroke-opacity" | "opacity" | "stop-opacity" => Value::Num(1.0),
        "fill-rule" => Value::Str("nonzero".into()),
        "stroke-linecap" => Value::Cap(Cap::Butt),
        "stroke-linejoin" => Value::Join(Join::Miter),
        "stroke-miterlimit" => Value::Num(4.0),
        "vector-effect" => Value::Str("none".into()),
        "image-rendering" => Value::Rendering(0),
        _ => Value::None,
    }
}

struct Importer<'a, 'i, 's> {
    doc: &'a Document<'i>,
    ids: HashMap<String, NodeId>,
    css: HashMap<NodeId, String>,
    bitmaps: HashMap<String, u16>,
    view_box: (f64, f64, f64, f64),
    swf: &'s mut Swf,
    shape_path: Vec<usize>,
    shape_num: u8,
    flags: u8,
    records: Vec<Record>,
}

fn tag(n: Node) -> String {
    let local = n.tag_name().name();
    match n.tag_name().namespace().and_then(|ns| n.lookup_prefix(ns)) {
        Some(p) if !p.is_empty() => format!("{p}:{local}"),
        _ => local.to_string(),
    }
}

/// Element.getAttribute by qualified name.
fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    match name.split_once(':') {
        Some((p, local)) => {
            let ns = n.lookup_namespace_uri(Some(p)).or((p == "xlink").then_some(XLINK))?;
            n.attribute((ns, local))
        }
        None => n.attributes().find(|a| a.namespace().is_none() && a.name() == name).map(|a| a.value()),
    }
}

fn get<'a>(n: Node<'a, '_>, name: &str) -> &'a str {
    attr(n, name).unwrap_or("")
}

fn elements<'a, 'i>(n: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(|c| c.is_element())
}

fn parent_element<'a, 'i>(n: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    n.parent().filter(|p| p.is_element())
}

fn bad(m: impl Into<String>) -> Error {
    Error::Other(m.into())
}

impl<'a, 'i, 's> Importer<'a, 'i, 's> {
    fn length(&self, value: &str, relative: f64) -> Result<f64> {
        let v = value.to_lowercase();
        let (num, unit) = if ["em", "ex", "px", "in", "cm", "mm", "pt", "pc"].iter().any(|u| v.ends_with(u)) {
            (&v[..v.len() - 2], &v[v.len() - 2..])
        } else if let Some(n) = v.strip_suffix('%') {
            (n, "%")
        } else {
            (v.as_str(), "")
        };
        let r = java_double(num).ok_or_else(|| bad(format!("invalid length \"{value}\"")))?;
        Ok(match unit {
            "in" => r * 96.0,
            "pt" => r * 96.0 / 72.0,
            "pc" => r * 96.0 / 6.0,
            "cm" => r * 96.0 / 2.54,
            "mm" => r * 96.0 / 25.4,
            "%" => relative * r / 100.0,
            _ => r,
        })
    }

    fn attr_length(&self, n: Node, name: &str, relative: f64) -> Result<f64> {
        let a = get(n, name);
        if a.is_empty() { Ok(0.0) } else { self.length(a, relative) }
    }

    fn style_values(&self, n: Node) -> HashMap<String, String> {
        let mut s = String::new();
        if let Some(c) = self.css.get(&n.id()) {
            s += c;
        }
        if let Some(st) = attr(n, "style") {
            s += "{1000}";
            s += st;
        }
        let mut out = HashMap::new();
        if s.is_empty() {
            return out;
        }
        let mut parts: Vec<&str> = s.split('{').collect();
        while parts.len() > 1 && parts.last() == Some(&"") {
            parts.pop();
        }
        let spec = |p: &str| -> i64 { p.find('}').and_then(|i| p[..i].parse().ok()).unwrap_or(0) };
        parts.sort_by(|a, b| match (a.is_empty(), b.is_empty()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, _) => std::cmp::Ordering::Less,
            (_, true) => std::cmp::Ordering::Greater,
            _ => spec(a).cmp(&spec(b)),
        });
        let joined: Vec<&str> = parts.iter().map(|p| p.find('}').map_or(*p, |i| &p[i + 1..])).collect();
        let all = joined.join(";") + ";";
        for def in all.split(';') {
            let Some((name, value)) = def.split_once(':') else { continue };
            let name = name.trim();
            if PROPS.iter().any(|p| p.0 == name) {
                out.insert(name.to_string(), value.trim().to_string());
            }
        }
        out
    }

    fn value(&mut self, n: Node<'a, 'i>, name: &str, inherit: bool) -> Value {
        let styles = self.style_values(n);
        for v in [styles.get(name).cloned(), attr(n, name).map(|a| a.trim().to_string())].into_iter().flatten() {
            if v == "inherit" {
                if let Some(p) = parent_element(n) {
                    return self.value(p, name, true);
                }
            } else if let Some(r) = self.style_value(n, name, &v) {
                return r;
            }
        }
        let inherited = PROPS.iter().find(|p| p.0 == name).is_some_and(|p| p.1);
        match parent_element(n) {
            Some(p) if inherit || inherited => self.value(p, name, false),
            _ => initial(name),
        }
    }

    fn num(&mut self, n: Node<'a, 'i>, name: &str) -> f64 {
        match self.value(n, name, false) {
            Value::Num(v) => v,
            _ => 1.0,
        }
    }

    fn color(&mut self, n: Node<'a, 'i>) -> Rgba {
        match self.value(n, "color", false) {
            Value::Color(c) => c,
            _ => Rgba::BLACK,
        }
    }

    fn paint(&mut self, n: Node<'a, 'i>, name: &str) -> Option<Paint> {
        match self.value(n, name, false) {
            Value::Paint(p) => Some(*p),
            _ => None,
        }
    }

    fn style_value(&mut self, n: Node<'a, 'i>, name: &str, v: &str) -> Option<Value> {
        if v.is_empty() {
            return None;
        }
        let num = || java_double(v).map(Value::Num);
        match name {
            "color" => parse_color(v).ok().flatten().map(Value::Color),
            "fill" | "stroke" => {
                if v == "currentColor" {
                    let c = self.color(n);
                    return Some(Value::Paint(Box::new(Paint::Color(c))));
                }
                self.parse_paint(v).ok().map(|p| Value::Paint(Box::new(p)))
            }
            "fill-opacity" | "stroke-opacity" | "opacity" | "stop-opacity" | "stroke-miterlimit" => num(),
            "fill-rule" | "vector-effect" => Some(Value::Str(v.to_string())),
            "stroke-width" => {
                let (_, _, w, h) = self.view_box;
                self.length(v, (w * w + h * h).sqrt() / 2f64.sqrt()).ok().map(Value::Num)
            }
            "stroke-linecap" => match v {
                "butt" => Some(Value::Cap(Cap::Butt)),
                "round" => Some(Value::Cap(Cap::Round)),
                "square" => Some(Value::Cap(Cap::Square)),
                _ => None,
            },
            "stroke-linejoin" => match v {
                "miter" => Some(Value::Join(Join::Miter)),
                "round" => Some(Value::Join(Join::Round)),
                "bevel" => Some(Value::Join(Join::Bevel)),
                _ => None,
            },
            "image-rendering" => match v {
                "optimizeSpeed" | "pixelated" => Some(Value::Rendering(1)),
                "optimizeQuality" => Some(Value::Rendering(2)),
                "auto" => Some(Value::Rendering(0)),
                _ => None,
            },
            "stop-color" => {
                if v == "currentColor" {
                    return Some(Value::Color(self.color(n)));
                }
                parse_color(v).ok().flatten().map(Value::Color)
            }
            _ => None,
        }
    }

    fn fill_with_opacity(&mut self, n: Node<'a, 'i>, name: &str, opacity_name: &str) -> Option<Paint> {
        let p = self.paint(n, name)?;
        let Paint::Color(c) = p else { return Some(p) };
        let o = ((self.num(n, "opacity") * self.num(n, opacity_name) * 255.0).jround() as i64).clamp(0, 255);
        Some(if o == 255 { p } else { Paint::Color(Rgba { a: o as u8, ..c }) })
    }

    fn pattern_child(&self, e: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
        let mut found = None;
        for c in elements(e) {
            if tag(c) == "animateTransform" {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(c);
        }
        found
    }

    fn smoothed(&mut self, e: Node<'a, 'i>, child: Option<Node<'a, 'i>>) -> bool {
        let rendering = child.map(|c| self.value(c, "image-rendering", true));
        match attr(e, "ffdec:smoothed").map(str::trim) {
            Some("true") => true,
            Some("false") => false,
            Some(_) => true,
            None => rendering != Some(Value::Rendering(1)),
        }
    }

    /// SvgStyle.parseFill; Err where Java throws.
    fn parse_paint(&mut self, s: &str) -> std::result::Result<Paint, ()> {
        if s == "none" {
            return Ok(Paint::Transparent);
        }
        let mut s = s.to_string();
        if let Some(rest) = s.strip_prefix("url(#") {
            if let Some(end) = rest.find(')') {
                let id = rest[..end].to_string();
                if let Some(&e) = self.ids.get(&id) {
                    let e = self.doc.get_node(e).unwrap();
                    if let Some(&bid) = self.bitmaps.get(&id) {
                        let child = self.pattern_child(e);
                        let smoothed = self.smoothed(e, child);
                        return Ok(Paint::Bitmap { id: bid, transform: attr(e, "patternTransform").map(str::to_string), smoothed });
                    }
                    match tag(e).as_str() {
                        "linearGradient" | "radialGradient" => return Ok(self.gradient(e).unwrap_or_else(unknown_fill)),
                        "pattern" => {
                            if let Some(img) = self.pattern_child(e).filter(|c| tag(*c) == "image") {
                                let href = get(img, "xlink:href").trim();
                                if let (true, Some(i)) = (href.starts_with("data:image/"), href.find("base64,")) {
                                    if let Some(data) = base64(&href[i + 7..]) {
                                        if let Ok(bid) = self.add_image(&data) {
                                            let smoothed = self.smoothed(e, Some(img));
                                            self.bitmaps.insert(id.clone(), bid);
                                            return Ok(Paint::Bitmap { id: bid, transform: attr(e, "patternTransform").map(str::to_string), smoothed });
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    return Ok(unknown_fill());
                }
                s = s.get(id.len() + 6..).unwrap_or("").trim().to_string();
            }
        }
        match parse_color(&s)? {
            Some(c) => Ok(Paint::Color(c)),
            None => Ok(unknown_fill()),
        }
    }

    fn add_image(&mut self, data: &[u8]) -> Result<u16> {
        let (id, code, body) = crate::importers::new_image(self.swf, data)?;
        self.swf.insert_tag(&self.shape_path, crate::tags::Tag::new(code, body))?;
        *self.shape_path.last_mut().unwrap() += 1;
        Ok(id)
    }

    /// Parses a gradient paint; None falls back to a random colour.
    fn gradient(&mut self, el: Node<'a, 'i>) -> Option<Paint> {
        let name = tag(el);
        let (mut units, mut transform, mut spread, mut interpolation) = (None, None, None, None);
        let mut coords: [Option<String>; 9] = Default::default();
        let mut stops = Vec::new();
        if let Some(href) = attr(el, "xlink:href") {
            let pid = href.strip_prefix('#')?;
            let parent = self.doc.get_node(*self.ids.get(pid)?).unwrap();
            let pname = tag(parent);
            if pname == "linearGradient" || pname == "radialGradient" {
                if let Some(p) = self.gradient(parent) {
                    let (g, c): (Grad, Vec<String>) = match p {
                        Paint::Linear(g, c) => (g, c.to_vec()),
                        Paint::Radial(g, c) => (g, c.to_vec()),
                        _ => return None,
                    };
                    units = Some(g.units);
                    transform = g.transform;
                    spread = Some(g.spread);
                    interpolation = Some(g.interpolation);
                    stops = g.stops;
                    if name == pname {
                        let off = if name == "linearGradient" { 0 } else { 4 };
                        for (i, v) in c.into_iter().enumerate() {
                            coords[off + i] = Some(v);
                        }
                    }
                }
            }
        }
        match attr(el, "gradientUnits") {
            Some("userSpaceOnUse") => units = Some(Units::UserSpace),
            Some("objectBoundingBox") => units = Some(Units::BoundingBox),
            Some(_) => return None,
            None => {}
        }
        if let Some(t) = attr(el, "gradientTransform") {
            transform = Some(t.to_string());
        }
        match attr(el, "spreadMethod") {
            Some("pad") => spread = Some(0),
            Some("reflect") => spread = Some(1),
            Some("repeat") => spread = Some(2),
            _ => {}
        }
        for (i, a) in ["x1", "y1", "x2", "y2", "cx", "cy", "fx", "fy", "r"].iter().enumerate() {
            if let Some(v) = attr(el, a) {
                coords[i] = Some(v.trim().to_string());
            }
        }
        if interpolation.is_none() {
            interpolation = match attr(el, "color-interpolation") {
                Some("linearRGB") => Some(1),
                Some("sRGB") | Some("auto") => Some(0),
                _ => None,
            };
        }
        let mut cleared = false;
        let stop_nodes: Vec<Node> = el.descendants().filter(|d| d.is_element() && d.id() != el.id() && tag(*d) == "stop").collect();
        for s in stop_nodes {
            let offset = attr(s, "offset").and_then(number_or_percent).unwrap_or(0.0);
            let color = match self.value(s, "stop-color", false) {
                Value::Color(c) => c,
                _ => Rgba::BLACK,
            };
            let alpha = (self.num(s, "stop-opacity") * 255.0).jround() as i64;
            if !cleared {
                cleared = true;
                stops = Vec::new();
            }
            stops.push(Stop { color: Rgba { a: alpha as u8, ..color }, offset });
        }
        let d = |i: usize, v: &str| coords[i].clone().unwrap_or_else(|| v.to_string());
        let (cx, cy) = (d(4, "50%"), d(5, "50%"));
        let g = Grad { stops: fix_stops(stops), units: units.unwrap_or(Units::BoundingBox), transform, spread: spread.unwrap_or(0), interpolation: interpolation.unwrap_or(0) };
        match name.as_str() {
            "linearGradient" => Some(Paint::Linear(g, [d(0, "0%"), d(1, "0%"), d(2, "100%"), d(3, "0%")])),
            "radialGradient" => Some(Paint::Radial(g, [cx.clone(), cy.clone(), coords[6].clone().unwrap_or(cx), coords[7].clone().unwrap_or(cy), d(8, "50%")])),
            _ => None,
        }
    }

    fn process_css(&mut self, n: Node<'a, 'i>) {
        let text: String = n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect();
        for (selector, spec, decls) in css_rules(text.trim()) {
            let matched: Vec<NodeId> = self.doc.descendants().filter(|e| e.is_element() && selector_matches(&selector, *e)).map(|e| e.id()).collect();
            for id in matched {
                let entry = self.css.entry(id).or_default();
                if !entry.is_empty() {
                    entry.push(';');
                }
                entry.push_str(&format!("{{{spec}}}{decls}"));
            }
        }
    }

    fn process_defs(&mut self, n: Node<'a, 'i>) {
        for c in elements(n) {
            match tag(c).as_str() {
                "style" => self.process_css(c),
                "defs" => self.process_defs(c),
                _ => {}
            }
        }
    }

    fn process_children(&mut self, n: Node<'a, 'i>, t: &M) -> Result<()> {
        for c in elements(n) {
            self.process(c, t)?;
        }
        Ok(())
    }

    fn process(&mut self, n: Node<'a, 'i>, t: &M) -> Result<()> {
        if attr(n, "requiredExtensions").is_some_and(|v| !v.is_empty()) {
            return Ok(());
        }
        let m = parse_transform(attr(n, "transform"), 1.0, 1.0).ok_or_else(|| bad("invalid transform"))?;
        let m2 = t.concat(&m);
        let (_, _, vw, vh) = self.view_box;
        match tag(n).as_str() {
            "switch" => {
                for c in elements(n) {
                    if attr(c, "requiredExtensions").is_some_and(|v| !v.is_empty()) {
                        continue;
                    }
                    if let Some(lang) = attr(c, "systemLanguage") {
                        if lang == "en-us" || lang == "en" {
                            return self.process(c, t);
                        }
                        continue;
                    }
                    return self.process(c, t);
                }
            }
            "style" => self.process_css(n),
            "g" => self.process_children(n, &m2)?,
            "path" => {
                let cmds = path_commands(get(n, "d"));
                self.commands(&cmds, &m2, n)?;
            }
            "circle" => {
                let cx = self.attr_length(n, "cx", vw)?;
                let cy = self.attr_length(n, "cy", vh)?;
                let r = self.attr_length(n, "r", (vw + vh) / 2.0)?;
                self.ellipse(cx, cy, r, r, &m2, n)?;
            }
            "ellipse" => {
                let cx = self.attr_length(n, "cx", vw)?;
                let cy = self.attr_length(n, "cy", vh)?;
                let rx = self.attr_length(n, "rx", vw)?;
                let ry = self.attr_length(n, "ry", vh)?;
                self.ellipse(cx, cy, rx, ry, &m2, n)?;
            }
            "rect" => self.rect(n, &m2)?,
            "line" => {
                let c = [self.attr_length(n, "x1", vw)?, self.attr_length(n, "y1", vh)?, self.attr_length(n, "x2", vw)?, self.attr_length(n, "y2", vh)?];
                self.commands(&[('M', vec![c[0], c[1]]), ('L', vec![c[2], c[3]])], &m2, n)?;
            }
            "polyline" | "polygon" => {
                let mut cmds = Vec::new();
                let mut r = PathReader::new(get(n, "points"));
                let mut cmd = 'M';
                while r.has_next() {
                    let Some(x) = r.double() else { break };
                    let Some(y) = r.double() else { break };
                    cmds.push((cmd, vec![x, y]));
                    cmd = 'L';
                }
                if tag(n) == "polygon" {
                    cmds.push(('Z', vec![]));
                }
                self.commands(&cmds, &m2, n)?;
            }
            "defs" => self.process_defs(n),
            _ => {}
        }
        Ok(())
    }

    fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, t: &M, n: Node<'a, 'i>) -> Result<()> {
        let s2 = 2f64.sqrt();
        let (hx, mx, hy, my) = (s2 * rx / 2.0, (s2 - 1.0) * rx, s2 * ry / 2.0, (s2 - 1.0) * ry);
        let p = [rx, -my, hx, -hy, mx, -ry, 0.0, -ry, -mx, -ry, -hx, -hy, -rx, -my, -rx, 0.0, -rx, my, -hx, hy, -mx, ry, 0.0, ry, mx, ry, hx, hy, rx, my, rx, 0.0];
        let mut cmds = vec![('M', vec![cx + rx, cy])];
        for q in p.chunks(4) {
            cmds.push(('Q', vec![cx + q[0], cy + q[1], cx + q[2], cy + q[3]]));
        }
        cmds.push(('Z', vec![]));
        self.commands(&cmds, t, n)
    }

    fn rect(&mut self, n: Node<'a, 'i>, t: &M) -> Result<()> {
        let (_, _, vw, vh) = self.view_box;
        let x = self.attr_length(n, "x", vw)?;
        let y = self.attr_length(n, "y", vh)?;
        let w = self.attr_length(n, "width", vw)?;
        let h = self.attr_length(n, "height", vh)?;
        let mut rx = self.attr_length(n, "rx", vw)?;
        let mut ry = self.attr_length(n, "ry", vh)?;
        if rx == 0.0 && ry != 0.0 {
            rx = ry;
        } else if rx != 0.0 && ry == 0.0 {
            ry = rx;
        }
        rx = rx.min(w / 2.0);
        ry = ry.min(h / 2.0);
        let mut cmds;
        if rx > 0.0 || ry > 0.0 {
            cmds = vec![('M', vec![x + w, y + ry])];
            let s2 = 2f64.sqrt();
            let (hx, mx, hy, my) = (s2 * rx / 2.0, (s2 - 1.0) * rx, s2 * ry / 2.0, (s2 - 1.0) * ry);
            let p = [
                x + w, y + ry - my, x + w - rx + hx, y + ry - hy, x + w - rx + mx, y, x + w - rx, y, x + rx, y,
                x + rx - mx, y, x + rx - hx, y + ry - hy, x, y + ry - my, x, y + ry, x, y + h - ry,
                x, y + h - ry + my, x + rx - hx, y + h - ry + hy, x + rx - mx, y + h, x + rx, y + h, x + w - rx, y + h,
                x + w - rx + mx, y + h, x + w - rx + hx, y + h - ry + hy, x + w, y + h - ry + my, x + w, y + h - ry, x + w, y + ry,
            ];
            let mut i = 0;
            while i < p.len() {
                if i % 10 == 8 {
                    cmds.push(('L', vec![p[i], p[i + 1]]));
                    i += 2;
                } else {
                    cmds.push(('Q', p[i..i + 4].to_vec()));
                    i += 4;
                }
            }
        } else {
            cmds = vec![('M', vec![x, y]), ('L', vec![x + w, y]), ('L', vec![x + w, y + h]), ('L', vec![x, y + h]), ('L', vec![x, y])];
        }
        cmds.push(('Z', vec![]));
        self.commands(&cmds, t, n)
    }

    fn rgb_for(&self, c: Rgba) -> Rgba {
        if self.shape_num >= 3 { c } else { Rgba { a: 255, ..c } }
    }

    /// getStyleChangeRecord: the path's own fill and line style, plus its fill and line indexes.
    fn style_record(&mut self, n: Node<'a, 'i>, t: &M) -> (Styles, u32, u32, Option<Paint>, Option<Paint>) {
        let mut styles = Styles::default();
        let fill = self.fill_with_opacity(n, "fill", "fill-opacity");
        let mut fill_index = 0;
        if let Some(f) = fill.as_ref().filter(|f| !matches!(f, Paint::Transparent)) {
            styles.fills.push(match f {
                Paint::Color(c) => Fill::Solid(self.rgb_for(*c)),
                _ => Fill::Solid(Rgba::BLACK),
            });
            fill_index = 1;
        }
        let stroke = self.fill_with_opacity(n, "stroke", "stroke-opacity");
        let mut line_index = 0;
        if let Some(s) = stroke.as_ref().filter(|s| !matches!(s, Paint::Transparent)) {
            let color = self.rgb_for(s.to_color());
            let scale = (t.sx * t.sy - t.r0 * t.r1).abs().sqrt();
            let width = (self.num(n, "stroke-width") * scale * 20.0).jround() as i64 as u16;
            let cap = match self.value(n, "stroke-linecap", false) {
                Value::Cap(c) => c,
                _ => Cap::Butt,
            };
            let join = match self.value(n, "stroke-linejoin", false) {
                Value::Join(j) => j,
                _ => Join::Miter,
            };
            let mut line = LineStyle::simple(width, color);
            if self.shape_num == 4 {
                let non_scaling = matches!(self.value(n, "vector-effect", false), Value::Str(v) if v == "non-scaling-stroke");
                if non_scaling {
                    line.no_h_scale = true;
                    line.no_v_scale = true;
                    self.flags |= 0x02;
                } else {
                    self.flags |= 0x01;
                }
                let swf_cap = match cap {
                    Cap::Butt => 1,
                    Cap::Round => 0,
                    Cap::Square => 2,
                };
                (line.start_cap, line.end_cap) = (swf_cap, swf_cap);
                if !matches!(s, Paint::Color(_)) {
                    line.fill = Some(Fill::Solid(Rgba::BLACK));
                }
                line.join = match join {
                    Join::Miter => 2,
                    Join::Round => 0,
                    Join::Bevel => 1,
                };
                line.miter_limit = self.num(n, "stroke-miterlimit") as f32 as f64;
            }
            styles.lines.push(line);
            line_index = 1;
        }
        (styles, fill_index, line_index, fill, stroke)
    }

    fn commands(&mut self, cmds: &[(char, Vec<f64>)], t: &M, n: Node<'a, 'i>) -> Result<()> {
        if matches!(self.value(n, "fill-rule", false), Value::Str(v) if v == "nonzero") {
            let f = self.paint(n, "fill");
            if f.is_some_and(|f| !matches!(f, Paint::Transparent)) && self.shape_num == 4 {
                self.flags |= 0x04;
            }
        }
        let t2 = t.pre_concat(&M::scale(20.0, 20.0));
        let (mut prev, mut start) = ((0.0, 0.0), (0.0, 0.0));
        let (mut x0, mut y0) = (0.0, 0.0);
        let (mut styles, fill_style, line_style, fill, stroke) = self.style_record(n, t);
        let has_line2 = self.shape_num == 4 && !styles.lines.is_empty();
        if has_line2 {
            styles.lines[0].no_close = true;
        }
        let mut recs: Vec<Record> = vec![Record::StyleChange { move_to: None, fill0: Some(0), fill1: Some(0), line: Some(0), new_styles: None }];
        let mut no_close = has_line2;
        let mut empty = true;
        let delta = |p: (f64, f64), prev: (f64, f64)| ((p.0 - prev.0).jround() as i32, (p.1 - prev.1).jround() as i32);
        for (cmd, params) in cmds {
            let (mut x, mut y) = (x0, y0);
            match cmd {
                'M' => {
                    auto_close(empty, &mut recs, fill_style, line_style, start, prev);
                    (x, y) = (params[0], params[1]);
                    let p = t2.transform(x, y);
                    let mv = (p.0.jround() as i32, p.1.jround() as i32);
                    prev = (mv.0 as f64, mv.1 as f64);
                    recs.push(Record::StyleChange {
                        move_to: Some(mv),
                        fill0: None,
                        fill1: (fill_style != 0).then_some(fill_style),
                        line: (line_style != 0).then_some(line_style),
                        new_styles: None,
                    });
                    start = prev;
                    empty = true;
                }
                'Z' => {
                    let (dx, dy) = delta(start, prev);
                    prev = (prev.0 + dx as f64, prev.1 + dy as f64);
                    straight(&mut recs, dx, dy);
                    no_close = false;
                    empty = true;
                }
                'L' | 'H' | 'V' => {
                    match cmd {
                        'L' => (x, y) = (params[0], params[1]),
                        'H' => x = params[0],
                        _ => y = params[0],
                    }
                    let (dx, dy) = delta(t2.transform(x, y), prev);
                    prev = (prev.0 + dx as f64, prev.1 + dy as f64);
                    straight(&mut recs, dx, dy);
                    empty = false;
                }
                'Q' => {
                    let (cdx, cdy) = delta(t2.transform(params[0], params[1]), prev);
                    prev = (prev.0 + cdx as f64, prev.1 + cdy as f64);
                    (x, y) = (params[2], params[3]);
                    let (adx, ady) = delta(t2.transform(x, y), prev);
                    prev = (prev.0 + adx as f64, prev.1 + ady as f64);
                    curved(&mut recs, cdx, cdy, adx, ady);
                    empty = false;
                }
                'C' => {
                    let c1 = t2.transform(params[0], params[1]);
                    let c2 = t2.transform(params[2], params[3]);
                    (x, y) = (params[4], params[5]);
                    let p = t2.transform(x, y);
                    let q = cubic_to_quad(prev.0, prev.1, c1.0, c1.1, c2.0, c2.1, p.0, p.1, 1.0);
                    let mut i = 2;
                    while i + 3 < q.len() {
                        let (cdx, cdy) = delta((q[i], q[i + 1]), prev);
                        prev = (prev.0 + cdx as f64, prev.1 + cdy as f64);
                        let (adx, ady) = delta((q[i + 2], q[i + 3]), prev);
                        prev = (prev.0 + adx as f64, prev.1 + ady as f64);
                        curved(&mut recs, cdx, cdy, adx, ady);
                        i += 4;
                    }
                    empty = false;
                }
                _ => return Ok(()),
            }
            (x0, y0) = (x, y);
        }
        auto_close(empty, &mut recs, fill_style, line_style, start, prev);
        if has_line2 {
            styles.lines[0].no_close = no_close;
        }
        if let Some(Record::StyleChange { new_styles, .. }) = recs.first_mut() {
            *new_styles = Some(Box::new(styles));
        }
        let b = bounds(&recs, &[], self.shape_num, false);
        let opacity = self.num(n, "opacity");
        if let Some(f) = fill.filter(|f| !matches!(f, Paint::Transparent)) {
            let o = opacity * self.num(n, "fill-opacity");
            if let Some(Record::StyleChange { new_styles: Some(s), .. }) = recs.first_mut() {
                if let Some(fs) = s.fills.first_mut() {
                    self.apply_paint(&f, fs, &b, &t2, o)?;
                }
            }
        }
        if let Some(s) = stroke {
            let o = opacity * self.num(n, "stroke-opacity");
            if let Some(Record::StyleChange { new_styles: Some(st), .. }) = recs.first_mut() {
                if let Some(Some(fs)) = st.lines.first_mut().filter(|_| self.shape_num == 4).map(|l| l.fill.as_mut()) {
                    self.apply_paint(&s, fs, &b, &t2, o)?;
                }
            }
        }
        self.records.extend(recs);
        Ok(())
    }

    /// applyFillGradients.
    fn apply_paint(&self, paint: &Paint, fs: &mut Fill, b: &crate::types::Rect, t: &M, opacity: f64) -> Result<()> {
        let coord = |s: &str| self.length(s, 1.0);
        let (bw, bh) = ((b.x_max - b.x_min) as f64, (b.y_max - b.y_min) as f64);
        let bbox = || M { sx: bw / 20.0, sy: bh / 20.0, r0: 0.0, r1: 0.0, tx: b.x_min as f64, ty: b.y_min as f64 };
        match paint {
            Paint::Linear(g, _) | Paint::Radial(g, _) => {
                let gm = parse_transform(g.transform.as_deref(), 20.0, 1.0).ok_or_else(|| bad("invalid gradientTransform"))?;
                let gm = t.concat(&M::scale(1.0 / 20.0, 1.0 / 20.0)).concat(&gm);
                let mut matrix = gm.to_swf();
                let mut kind = 0x10;
                let mut focal = 0.0;
                if let Paint::Linear(_, c) = paint {
                    let (mut x1, mut y1, x2, y2) = (coord(&c[0])?, coord(&c[1])?, coord(&c[2])?, coord(&c[3])?);
                    let tm = if g.units == Units::BoundingBox {
                        let xy = M { sx: (x2 - x1) * 20.0, r0: (y2 - y1) * 20.0, r1: -(y2 - y1) * 20.0, sy: (x2 - x1) * 20.0, tx: 0.0, ty: 0.0 };
                        x1 *= bw;
                        y1 *= bh;
                        M::ID.concat(&M::translate(x1, y1)).concat(&bbox()).concat(&xy).concat(&M::translate(0.5, 0.0)).concat(&M::scale(1.0 / 16384.0 / 2.0, 1.0 / 16384.0 / 2.0))
                    } else {
                        let (x1, y1, x2, y2) = (x1 * 20.0, y1 * 20.0, x2 * 20.0, y2 * 20.0);
                        let l = 16384.0;
                        let (ux, uy) = (x2 - x1, y2 - y1);
                        let du = (x2 - x1).hypot(y2 - y1);
                        let (mx, my) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                        let (ax, ay) = (ux / du, uy / du);
                        let k = du / (2.0 * l);
                        let (nx, ny) = (-ay, ax);
                        M {
                            sx: k * (gm.sx * ax + gm.r1 * ay),
                            r1: k * (gm.sx * nx + gm.r1 * ny),
                            tx: gm.sx * mx + gm.r1 * my + gm.tx,
                            r0: k * (gm.r0 * ax + gm.sy * ay),
                            sy: k * (gm.r0 * nx + gm.sy * ny),
                            ty: gm.r0 * mx + gm.sy * my + gm.ty,
                        }
                    };
                    matrix = tm.to_swf();
                } else if let Paint::Radial(_, rc) = paint {
                    let (cx, cy, r) = (coord(&rc[0])?, coord(&rc[1])?, self.length(&rc[4], 1.0)?);
                    let mut m = if g.units == Units::BoundingBox { bbox() } else { M::from_swf(&matrix) };
                    m.translate_by(20.0 * cx, 20.0 * cy);
                    let m = m.concat(&M::scale(r / 819.2, r / 819.2));
                    matrix = m.to_swf();
                    let (fx, fy) = (coord(&rc[2])?, coord(&rc[3])?);
                    if rc[2] != rc[0] || rc[3] != rc[1] {
                        kind = 0x13;
                        focal = (((fx - cx) * (fx - cx) + (fy - cy) * (fy - cy)).sqrt() / 819.2) as f32 as f64;
                    } else {
                        kind = 0x12;
                    }
                }
                let mut count = 0;
                let mut prev: i64 = -1;
                for (i, s) in g.stops.iter().enumerate() {
                    let mut ratio = ((s.offset * 255.0).jround() as i64).max(prev + 1);
                    count += 1;
                    if ratio == 255 && i + 1 < g.stops.len() {
                        if prev == 254 {
                            break;
                        }
                        ratio = 254;
                    }
                    prev = ratio;
                    if prev == 255 {
                        break;
                    }
                }
                prev = -1;
                let mut records = Vec::with_capacity(count);
                for (i, s) in g.stops.iter().take(count).enumerate() {
                    let c = Rgba { a: (s.color.a as f64 * opacity).jround() as i64 as u8, ..s.color };
                    let mut ratio = ((s.offset * 255.0).jround() as i64).max(prev + 1);
                    if ratio == 255 && i + 1 < count {
                        ratio = 254;
                    }
                    records.push(GradRecord { ratio: ratio as u8, color: self.rgb_for(c) });
                    prev = ratio;
                }
                let gr = Gradient { matrix, spread: g.spread, interpolation: g.interpolation, records, focal };
                *fs = match kind {
                    0x10 => Fill::Linear(gr),
                    0x12 => Fill::Radial(gr),
                    _ => Fill::Focal(gr),
                };
            }
            Paint::Bitmap { id, transform, smoothed } => {
                let m = parse_transform(transform.as_deref(), 20.0, 20.0).ok_or_else(|| bad("invalid patternTransform"))?;
                let m = t.concat(&M::scale(1.0 / 20.0, 1.0 / 20.0)).concat(&m);
                *fs = Fill::Bitmap { id: *id, matrix: m.to_swf(), repeat: true, smooth: *smoothed };
            }
            _ => {}
        }
        Ok(())
    }
}

fn number_or_percent(v: &str) -> Option<f64> {
    match v.strip_suffix('%') {
        Some(n) => java_double(n).map(|x| x / 100.0),
        None => java_double(v),
    }
}

fn fix_stops(mut stops: Vec<Stop>) -> Vec<Stop> {
    let clear = Rgba { r: 0, g: 0, b: 0, a: 0 };
    if stops.is_empty() {
        stops.push(Stop { color: clear, offset: 0.0 });
        stops.push(Stop { color: clear, offset: 1.0 });
    } else if stops.len() == 1 {
        stops[0].offset = 0.0;
        let c = stops[0].color;
        stops.push(Stop { color: c, offset: 1.0 });
    }
    let mut offset = 0.0;
    for s in stops.iter_mut() {
        if s.offset < offset {
            s.offset = offset;
        }
        if s.offset > 1.0 {
            s.offset = 1.0;
        }
        offset = s.offset;
    }
    if (offset - 1.0).abs() > 0.001 {
        let c = stops.last().unwrap().color;
        stops.push(Stop { color: c, offset: 1.0 });
    }
    stops
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn auto_close(empty: bool, recs: &mut Vec<Record>, fill: u32, line: u32, start: (f64, f64), prev: (f64, f64)) {
    if empty || start == prev || fill == 0 {
        return;
    }
    let style = |l: u32| Record::StyleChange { move_to: None, fill0: None, fill1: None, line: Some(l), new_styles: None };
    if line != 0 {
        recs.push(style(0));
    }
    recs.push(Record::Straight { dx: (start.0 - prev.0).jround() as i32, dy: (start.1 - prev.1).jround() as i32 });
    if line != 0 {
        recs.push(style(line));
    }
}

fn edge_bits(v: &[i32]) -> u32 {
    crate::io::bits_s(v).saturating_sub(2)
}

fn straight(recs: &mut Vec<Record>, dx: i32, dy: i32) {
    if edge_bits(&[dx, dy]) > 15 {
        let (x1, y1) = (dx / 2, dy / 2);
        let (x2, y2) = (x1 + (2 * x1 < dx) as i32, y1 + (2 * y1 < dy) as i32);
        straight(recs, x1, y1);
        straight(recs, x2, y2);
        return;
    }
    recs.push(Record::Straight { dx, dy });
}

fn curved(recs: &mut Vec<Record>, cdx: i32, cdy: i32, adx: i32, ady: i32) {
    if edge_bits(&[cdx, cdy, adx, ady]) > 15 {
        let p = [(0.0, 0.0), (cdx as f64, cdy as f64), ((cdx + adx) as f64, (cdy + ady) as f64)];
        let lerp = |a: (f64, f64), b: (f64, f64)| (0.5 * a.0 + 0.5 * b.0, 0.5 * a.1 + 0.5 * b.1);
        let (l1, r1) = (lerp(p[0], p[1]), lerp(p[1], p[2]));
        let mid = lerp(l1, r1);
        let c1 = (l1.0.jround() as i32, l1.1.jround() as i32);
        let a1 = (mid.0.jround() as i32 - c1.0, mid.1.jround() as i32 - c1.1);
        let c2 = (r1.0.jround() as i32 - c1.0 - a1.0, r1.1.jround() as i32 - c1.1 - a1.1);
        let a2 = (cdx + adx - c1.0 - a1.0 - c2.0, cdy + ady - c1.1 - a1.1 - c2.1);
        curved(recs, c1.0, c1.1, a1.0, a1.1);
        curved(recs, c2.0, c2.1, a2.0, a2.1);
        return;
    }
    recs.push(Record::Curved { cdx, cdy, adx, ady });
}

struct PathReader {
    s: Vec<char>,
    pos: usize,
}

impl PathReader {
    fn new(s: &str) -> PathReader {
        PathReader { s: s.chars().collect(), pos: 0 }
    }

    fn has_next(&self) -> bool {
        self.pos < self.s.len()
    }

    fn white(&mut self) {
        while self.has_next() && matches!(self.s[self.pos], ' ' | '\r' | '\n') {
            self.pos += 1;
        }
    }

    fn separators(&mut self) {
        while self.has_next() && matches!(self.s[self.pos], ' ' | ',' | '\r' | '\n') {
            self.pos += 1;
        }
    }

    fn command(&mut self) -> Option<char> {
        if !self.has_next() {
            return None;
        }
        self.white();
        let c = *self.s.get(self.pos)?;
        if c.is_ascii_alphabetic() {
            self.pos += 1;
            self.separators();
            return Some(c);
        }
        None
    }

    fn digits(&mut self) {
        while self.has_next() && self.s[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
    }

    /// readDouble; None where Java throws.
    fn double(&mut self) -> Option<f64> {
        let start = self.pos;
        self.white();
        let r = (|| {
            if *self.s.get(self.pos)? == '-' {
                self.pos += 1;
            }
            self.digits();
            if self.has_next() && self.s[self.pos] == '.' {
                self.pos += 1;
                self.digits();
            }
            if self.has_next() && self.s[self.pos] == 'e' {
                self.pos += 1;
                if *self.s.get(self.pos)? == '-' {
                    self.pos += 1;
                }
                self.digits();
            }
            let t: String = self.s[start..self.pos].iter().collect();
            java_double(&t)
        })();
        match r {
            Some(v) => {
                self.separators();
                Some(v)
            }
            None => {
                self.pos = start;
                None
            }
        }
    }
}

fn calc_angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
    let lu = (ux * ux + uy * uy).sqrt();
    let lv = (ux * ux + uy * uy).sqrt();
    let mut sign = (ux * vy - uy * vx).signum();
    if ux * vy - uy * vx == 0.0 {
        sign = 1.0;
    }
    sign * (ux * vx + uy * vy / (lu * lv)).acos()
}

/// processPathD: absolute M, L, H, V, Q and C commands, with arcs as quadratic curves.
fn path_commands(data: &str) -> Vec<(char, Vec<f64>)> {
    let mut out: Vec<(char, Vec<f64>)> = Vec::new();
    let mut r = PathReader::new(data);
    let mut command = '\0';
    let mut start = (0.0, 0.0);
    let (mut prev_c, mut prev_q): (Option<(f64, f64)>, Option<(f64, f64)>) = (None, None);
    let (mut x0, mut y0) = (0.0, 0.0);
    let pi = std::f64::consts::PI;
    'outer: while r.has_next() {
        if let Some(c) = r.command() {
            command = c;
        }
        let rel = command.is_ascii_lowercase();
        let (mut x, mut y) = (x0, y0);
        let cmd = command.to_ascii_uppercase();
        macro_rules! d {
            () => {
                match r.double() {
                    Some(v) => v,
                    None => break 'outer,
                }
            };
        }
        let abs = |v: f64, o: f64| if rel { v + o } else { v };
        match cmd {
            'M' => {
                x = abs(d!(), x0);
                y = abs(d!(), y0);
                out.push(('M', vec![x, y]));
                start = (x, y);
                command = if rel { 'l' } else { 'L' };
            }
            'Z' => {
                (x, y) = start;
                out.push(('Z', vec![]));
            }
            'L' => {
                x = abs(d!(), x0);
                y = abs(d!(), y0);
                out.push(('L', vec![x, y]));
            }
            'H' => {
                x = abs(d!(), x0);
                out.push(('H', vec![x]));
            }
            'V' => {
                y = abs(d!(), y0);
                out.push(('V', vec![y]));
            }
            'Q' | 'T' => {
                let c = if cmd == 'Q' {
                    let cx = abs(d!(), x0);
                    (cx, abs(d!(), y0))
                } else if let Some(p) = prev_q {
                    (2.0 * x0 - p.0, 2.0 * y0 - p.1)
                } else {
                    (x0, y0)
                };
                prev_q = Some(c);
                x = abs(d!(), x0);
                y = abs(d!(), y0);
                out.push(('Q', vec![c.0, c.1, x, y]));
            }
            'C' | 'S' => {
                let c1 = if cmd == 'C' {
                    let cx = abs(d!(), x0);
                    (cx, abs(d!(), y0))
                } else if let Some(p) = prev_c {
                    (2.0 * x0 - p.0, 2.0 * y0 - p.1)
                } else {
                    (x0, y0)
                };
                let c2x = abs(d!(), x0);
                let c2 = (c2x, abs(d!(), y0));
                prev_c = Some(c2);
                x = abs(d!(), x0);
                y = abs(d!(), y0);
                out.push(('C', vec![c1.0, c1.1, c2.0, c2.1, x, y]));
            }
            'A' => {
                let mut rx = d!();
                let mut ry = d!();
                let fi = d!() * pi / 180.0;
                let large = d!() as i32 != 0;
                let sweep = d!() as i32 != 0;
                x = abs(d!(), x0);
                y = abs(d!(), y0);
                if rx == 0.0 || ry == 0.0 {
                    out.push(('L', vec![x, y]));
                } else {
                    rx = rx.abs();
                    ry = ry.abs();
                    let (x1, y1, x2, y2) = (x0, y0, x, y);
                    let (d1, d2) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
                    let mut x1c = fi.cos() * d1 + fi.sin() * d2;
                    let mut y1c = -fi.sin() * d1 + fi.cos() * d2;
                    let lambda = x1c * x1c / (rx * rx) + y1c * y1c / (ry * ry);
                    if lambda > 1.0 {
                        rx *= lambda.sqrt();
                        ry *= lambda.sqrt();
                    }
                    let c = ((rx * rx * ry * ry - rx * rx * y1c * y1c - ry * ry * x1c * x1c) / (rx * rx * y1c * y1c + ry * ry * x1c * x1c)).sqrt();
                    let (mut cxc, mut cyc) = (c * rx * y1c / ry, c * -ry * x1c / rx);
                    if large == sweep {
                        cxc = -cxc;
                        cyc = -cyc;
                    }
                    let cx = fi.cos() * cxc - fi.sin() * cyc + (x1 + x2) / 2.0;
                    let cy = fi.sin() * cxc + fi.cos() * cyc + (y1 + y2) / 2.0;
                    let (px1, py1) = ((x1c - cxc) / rx, (y1c - cyc) / ry);
                    let theta1 = calc_angle(1.0, 0.0, px1, py1);
                    let (px2, py2) = ((-x1c - cxc) / rx, (-y1c - cyc) / ry);
                    let mut dt = calc_angle(px1, py1, px2, py2);
                    if sweep {
                        if dt < 0.0 {
                            dt += 2.0 * pi;
                        }
                    } else if dt > 0.0 {
                        dt -= 2.0 * pi;
                    }
                    let rcp = (4.0 - 2.0 * 2f64.sqrt()).sqrt();
                    let delta = java_signum(dt) * pi / 4.0;
                    let segments = (dt / delta).ceil() as i32;
                    let mut theta = theta1;
                    for _ in 0..segments - 1 {
                        theta += delta;
                        let (x12, y12) = (theta.cos() * rx, theta.sin() * ry);
                        x1c = fi.cos() * x12 - fi.sin() * y12;
                        y1c = fi.sin() * x12 + fi.cos() * y12;
                        let t2 = theta - delta / 2.0;
                        let (x12, y12) = (t2.cos() * rx * rcp, t2.sin() * ry * rcp);
                        let x1c2 = fi.cos() * x12 - fi.sin() * y12;
                        let y1c2 = fi.sin() * x12 + fi.cos() * y12;
                        out.push(('Q', vec![cx + x1c2, cy + y1c2, cx + x1c, cy + y1c]));
                    }
                    theta += delta;
                    let diff = -delta - (theta1 + dt - theta);
                    theta = theta - delta - diff / 2.0;
                    let rcpm = 1.0 + (rcp - 1.0) * (diff / delta) * (diff / delta);
                    let (x12, y12) = (theta.cos() * rx * rcpm, theta.sin() * ry * rcpm);
                    x1c = fi.cos() * x12 - fi.sin() * y12;
                    y1c = fi.sin() * x12 + fi.cos() * y12;
                    out.push(('Q', vec![cx + x1c, cy + y1c, x, y]));
                }
            }
            _ => return Vec::new(),
        }
        if cmd != 'C' && cmd != 'S' {
            prev_c = None;
        }
        if cmd != 'Q' && cmd != 'T' {
            prev_q = None;
        }
        x0 = x;
        y0 = y;
    }
    out
}

fn java_signum(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}

// CubicToQuad, with its quirks: its cube root raises to the power 1 / 3 in integers, so to 0.

type P = (f64, f64);

fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn mul(a: P, v: f64) -> P {
    (a.0 * v, a.1 * v)
}

fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

fn sqr(a: P) -> f64 {
    a.0 * a.0 + a.1 * a.1
}

fn power_coefficients(p1: P, c1: P, c2: P, p2: P) -> [P; 4] {
    [add(sub(p2, p1), mul(sub(c1, c2), 3.0)), sub(mul(add(p1, c2), 3.0), mul(c1, 6.0)), mul(sub(c1, p1), 3.0), p1]
}

fn point_at(a: P, b: P, c: P, d: P, t: f64) -> P {
    add(mul(add(mul(add(mul(a, t), b), t), c), t), d)
}

fn point_quad(a: P, b: P, c: P, t: f64) -> P {
    add(mul(add(mul(a, t), b), t), c)
}

fn derivative(a: P, b: P, c: P, t: f64) -> P {
    add(mul(add(mul(a, 3.0 * t), mul(b, 2.0)), t), c)
}

fn quad_solve(a: f64, b: f64, c: f64) -> Vec<f64> {
    if a == 0.0 {
        return if b == 0.0 { vec![] } else { vec![-c / b] };
    }
    let d = b * b - 4.0 * a * c;
    if d.abs() < 1e-16 {
        return vec![];
    } else if d == 0.0 {
        return vec![-b / (2.0 * a)];
    }
    let s = d.sqrt();
    vec![(-b - s) / (2.0 * a), (-b + s) / (2.0 * a)]
}

fn segment(a: P, b: P, c: P, d: P, t1: f64, t2: f64) -> [P; 3] {
    let (f1, f2) = (point_at(a, b, c, d, t1), point_at(a, b, c, d, t2));
    let (g1, g2) = (derivative(a, b, c, t1), derivative(a, b, c, t2));
    let det = -g1.0 * g2.1 + g2.0 * g1.1;
    if det.abs() < 1e-8 {
        return [f1, mul(add(f1, f2), 0.5), f2];
    }
    let cx = (g1.0 * (f2.1 * g2.0 - f2.0 * g2.1) + g2.0 * (f1.0 * g1.1 - f1.1 * g1.0)) / det;
    let cy = (g1.1 * (f2.1 * g2.0 - f2.0 * g2.1) + g2.1 * (f1.0 * g1.1 - f1.1 * g1.0)) / det;
    [f1, (cx, cy), f2]
}

fn dist_line_sq(p: P, p1: P, p2: P) -> f64 {
    let d = sub(p2, p1);
    let dotv = dot(sub(p, p1), d);
    let len = sqr(d);
    let param = if len != 0.0 { dotv / len } else { 0.0 };
    let diff = if param <= 0.0 {
        sub(p, p1)
    } else if param >= 1.0 {
        sub(p, p2)
    } else {
        sub(p, add(p1, mul(d, param)))
    };
    sqr(diff)
}

#[allow(clippy::too_many_arguments)]
fn segment_close(a: P, b: P, c: P, d: P, tmin: f64, tmax: f64, q: &[P; 3], bound: f64) -> bool {
    let n = 10;
    let qa = add(add(mul(q[1], -2.0), q[0]), q[2]);
    let qb = mul(sub(q[1], q[0]), 2.0);
    let qc = q[0];
    let bound_sq = bound * bound;
    let mut cubic = Vec::new();
    let mut quad = Vec::new();
    let dt = (tmax - tmin) / n as f64;
    let mut t = tmin;
    for _ in 0..=n {
        cubic.push(point_at(a, b, c, d, t));
        t += dt;
    }
    let dt = 1.0 / n as f64;
    let mut t = 0.0;
    for _ in 0..=n {
        quad.push(point_quad(qa, qb, qc, t));
        t += dt;
    }
    for i in 1..cubic.len() - 1 {
        let m = (0..quad.len() - 1).map(|j| dist_line_sq(cubic[i], quad[j], quad[j + 1])).fold(f64::MAX, f64::min);
        if m > bound_sq {
            return false;
        }
    }
    for i in 1..quad.len() - 1 {
        let m = (0..cubic.len() - 1).map(|j| dist_line_sq(quad[i], cubic[j], cubic[j + 1])).fold(f64::MAX, f64::min);
        if m > bound_sq {
            return false;
        }
    }
    true
}

fn approximation_close(a: P, b: P, c: P, d: P, quads: &[[P; 3]], bound: f64) -> bool {
    let dt = 1.0 / quads.len() as f64;
    quads.iter().enumerate().all(|(i, q)| segment_close(a, b, c, d, i as f64 * dt, (i + 1) as f64 * dt, q, bound))
}

fn flat(quads: &[[P; 3]]) -> Vec<f64> {
    let mut r = vec![quads[0][0].0, quads[0][0].1];
    for q in quads {
        r.extend([q[1].0, q[1].1, q[2].0, q[2].1]);
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn cubic_to_quad_one(p1x: f64, p1y: f64, c1x: f64, c1y: f64, c2x: f64, c2y: f64, p2x: f64, p2y: f64, bound: f64) -> Vec<f64> {
    let (p1, c1, c2, p2) = ((p1x, p1y), (c1x, c1y), (c2x, c2y), (p2x, p2y));
    let [a, b, c, d] = power_coefficients(p1, c1, c2, p2);
    let mut approx: Vec<[P; 3]> = Vec::new();
    for count in 1..=8 {
        approx.clear();
        let step = 1.0 / count as f64;
        let mut t = 0.0;
        while t < 1.0 {
            approx.push(segment(a, b, c, d, t, t + step));
            t += step;
        }
        if count == 1 && (dot(sub(approx[0][1], p1), sub(c1, p1)) < 0.0 || dot(sub(approx[0][1], p2), sub(c2, p2)) < 0.0) {
            continue;
        }
        if approximation_close(a, b, c, d, &approx, bound) {
            break;
        }
    }
    flat(&approx)
}

fn subdivide(c: [f64; 8], t: f64) -> ([f64; 8], [f64; 8]) {
    let (u, v) = (1.0 - t, t);
    let [x1, y1, x2, y2, x3, y3, x4, y4] = c;
    let (bx, sx, fx) = (x1 * u + x2 * v, x2 * u + x3 * v, x3 * u + x4 * v);
    let (cx, ex) = (bx * u + sx * v, sx * u + fx * v);
    let dx = cx * u + ex * v;
    let (by, sy, fy) = (y1 * u + y2 * v, y2 * u + y3 * v, y3 * u + y4 * v);
    let (cy, ey) = (by * u + sy * v, sy * u + fy * v);
    let dy = cy * u + ey * v;
    ([x1, y1, bx, by, cx, cy, dx, dy], [dx, dy, ex, ey, fx, fy, x4, y4])
}

#[allow(clippy::too_many_arguments)]
fn cubic_to_quad(p1x: f64, p1y: f64, c1x: f64, c1y: f64, c2x: f64, c2y: f64, p2x: f64, p2y: f64, bound: f64) -> Vec<f64> {
    let (x1, y1, x2, y2, x3, y3, x4, y4) = (p1x, p1y, c1x, c1y, c2x, c2y, p2x, p2y);
    let p = -(x4 * (y1 - 2.0 * y2 + y3)) + x3 * (2.0 * y1 - 3.0 * y2 + y4) + x1 * (y2 - 2.0 * y3 + y4) - x2 * (y1 - 3.0 * y3 + 2.0 * y4);
    let q = x4 * (y1 - y2) + 3.0 * x3 * (-y1 + y2) + x2 * (2.0 * y1 - 3.0 * y3 + y4) - x1 * (2.0 * y2 - 3.0 * y3 + y4);
    let r = x3 * (y1 - y2) + x1 * (y2 - y3) + x2 * (-y1 + y3);
    let mut inflections: Vec<f64> = quad_solve(p, q, r).into_iter().filter(|t| *t > 1e-8 && *t < 1.0 - 1e-8).collect();
    inflections.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if inflections.is_empty() {
        return cubic_to_quad_one(p1x, p1y, c1x, c1y, c2x, c2y, p2x, p2y, bound);
    }
    let mut result = Vec::new();
    let mut curve = [p1x, p1y, c1x, c1y, c2x, c2y, p2x, p2y];
    let mut prev = 0.0;
    for &inf in &inflections {
        let (l, r) = subdivide(curve, 1.0 - (1.0 - inf) / (1.0 - prev));
        let q = cubic_to_quad_one(l[0], l[1], l[2], l[3], l[4], l[5], l[6], l[7], bound);
        result.extend_from_slice(&q[..q.len() - 2]);
        curve = r;
        prev = inf;
    }
    result.extend(cubic_to_quad_one(curve[0], curve[1], curve[2], curve[3], curve[4], curve[5], curve[6], curve[7], bound));
    result
}

// CSS rulesets with specificity summed over a selector list; rules inside @media apply.

fn css_rules(text: &str) -> Vec<(String, i64, String)> {
    let mut s = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("/*") {
        s += &rest[..i];
        rest = rest[i + 2..].find("*/").map_or("", |j| &rest[i + 2 + j + 2..]);
    }
    s += rest;
    let s = s.replace("<!--", " ").replace("-->", " ");
    let mut out = Vec::new();
    let mut rest = s.as_str();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if rest.starts_with('@') {
            let semi = rest.find(';');
            let brace = rest.find('{');
            match (semi, brace) {
                (Some(a), b) if b.is_none_or(|b| a < b) => {
                    rest = &rest[a + 1..];
                    continue;
                }
                (_, Some(b)) => {
                    let media = rest.starts_with("@media");
                    let inner_start = b + 1;
                    let mut depth = 1;
                    let mut end = inner_start;
                    for (i, c) in rest[inner_start..].char_indices() {
                        match c {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = inner_start + i;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    if media {
                        out.extend(css_rules(&rest[inner_start..end]));
                    }
                    rest = rest.get(end + 1..).unwrap_or("");
                    continue;
                }
                _ => break,
            }
        }
        let Some(open) = rest.find('{') else { break };
        let Some(close) = rest[open..].find('}').map(|c| open + c) else { break };
        let selector = rest[..open].trim().to_string();
        let decls = rest[open + 1..close].to_string();
        let spec: i64 = selector.split(',').map(specificity).sum();
        out.push((selector, spec, decls));
        rest = &rest[close + 1..];
    }
    out
}

fn specificity(sel: &str) -> i64 {
    let mut total = 0;
    for part in compound_parts(sel) {
        let mut chars = part.chars().peekable();
        if chars.peek().is_some_and(|c| c.is_alphabetic() || *c == '_' || *c == '-') {
            total += 1;
        }
        let mut in_brackets = false;
        let mut in_paren = 0;
        for c in part.chars() {
            match c {
                '[' if in_paren == 0 => {
                    in_brackets = true;
                    total += 10;
                }
                ']' => in_brackets = false,
                '(' => in_paren += 1,
                ')' => in_paren -= 1,
                '#' if !in_brackets && in_paren == 0 => total += 100,
                '.' | ':' if !in_brackets && in_paren == 0 => total += 10,
                _ => {}
            }
        }
    }
    total
}

/// The compound selectors of one complex selector, with the combinators between them.
fn compound_parts(sel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in sel.chars() {
        match c {
            '[' | '(' => {
                depth += 1;
                cur.push(c);
            }
            ']' | ')' => {
                depth -= 1;
                cur.push(c);
            }
            ' ' | '>' | '+' | '~' if depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                if c != ' ' {
                    out.push(c.to_string());
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().filter(|p| !matches!(p.as_str(), ">" | "+" | "~")).collect()
}

fn selector_matches(list: &str, n: Node) -> bool {
    list.split(',').any(|sel| complex_matches(sel.trim(), n))
}

fn complex_matches(sel: &str, n: Node) -> bool {
    let mut parts: Vec<(char, String)> = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let mut comb = ' ';
    for c in sel.chars() {
        match c {
            '[' | '(' => {
                depth += 1;
                cur.push(c);
            }
            ']' | ')' => {
                depth -= 1;
                cur.push(c);
            }
            ' ' | '\t' | '\n' | '\r' | '>' | '+' | '~' if depth == 0 => {
                if !cur.is_empty() {
                    parts.push((comb, std::mem::take(&mut cur)));
                    comb = ' ';
                }
                if matches!(c, '>' | '+' | '~') {
                    comb = c;
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        parts.push((comb, cur));
    }
    if parts.is_empty() {
        return false;
    }
    fn go(parts: &[(char, String)], i: usize, n: Node) -> bool {
        if !compound_matches(&parts[i].1, n) {
            return false;
        }
        if i == 0 {
            return true;
        }
        let comb = parts[i].0;
        match comb {
            '>' => parent_element(n).is_some_and(|p| go(parts, i - 1, p)),
            '+' => n.prev_sibling_element().is_some_and(|p| go(parts, i - 1, p)),
            '~' => {
                let mut s = n.prev_sibling_element();
                while let Some(p) = s {
                    if go(parts, i - 1, p) {
                        return true;
                    }
                    s = p.prev_sibling_element();
                }
                false
            }
            _ => {
                let mut a = parent_element(n);
                while let Some(p) = a {
                    if go(parts, i - 1, p) {
                        return true;
                    }
                    a = parent_element(p);
                }
                false
            }
        }
    }
    go(&parts, parts.len() - 1, n)
}

fn compound_matches(c: &str, n: Node) -> bool {
    let chars: Vec<char> = c.chars().collect();
    let mut i = 0;
    let ident_len = |from: usize| chars[from..].iter().take_while(|c| c.is_alphanumeric() || **c == '-' || **c == '_').count();
    if i < chars.len() && chars[i] == '*' {
        i += 1;
    } else {
        let l = ident_len(0);
        if l > 0 {
            let name: String = chars[..l].iter().collect();
            if tag(n) != name {
                return false;
            }
            i = l;
        }
    }
    while i < chars.len() {
        match chars[i] {
            '#' | '.' => {
                let l = ident_len(i + 1);
                let v: String = chars[i + 1..i + 1 + l].iter().collect();
                let ok = if chars[i] == '#' { get(n, "id") == v } else { get(n, "class").split_whitespace().any(|c| c == v) };
                if !ok {
                    return false;
                }
                i += 1 + l;
            }
            '[' => {
                let end = chars[i..].iter().position(|c| *c == ']').map_or(chars.len(), |e| i + e);
                let inner: String = chars[i + 1..end].iter().collect();
                if !attr_matches(&inner, n) {
                    return false;
                }
                i = end + 1;
            }
            ':' => {
                let l = ident_len(i + 1);
                let name: String = chars[i + 1..i + 1 + l].iter().collect();
                let mut j = i + 1 + l;
                let mut arg = String::new();
                if j < chars.len() && chars[j] == '(' {
                    let end = chars[j..].iter().position(|c| *c == ')').map_or(chars.len(), |e| j + e);
                    arg = chars[j + 1..end].iter().collect();
                    j = end + 1;
                }
                let ok = match name.as_str() {
                    "first-child" => n.prev_sibling_element().is_none(),
                    "last-child" => n.next_sibling_element().is_none(),
                    "only-child" => n.prev_sibling_element().is_none() && n.next_sibling_element().is_none(),
                    "not" => !compound_matches(arg.trim(), n),
                    _ => false,
                };
                if !ok {
                    return false;
                }
                i = j;
            }
            _ => return false,
        }
    }
    true
}

fn attr_matches(inner: &str, n: Node) -> bool {
    let ops = ["~=", "|=", "^=", "$=", "*=", "!=", "="];
    for op in ops {
        if let Some((name, val)) = inner.split_once(op) {
            let val = val.trim().trim_matches(['"', '\'']);
            let Some(a) = attr(n, name.trim()) else { return op == "!=" };
            return match op {
                "~=" => a.split_whitespace().any(|w| w == val),
                "|=" => a == val || a.starts_with(&format!("{val}-")),
                "^=" => a.starts_with(val),
                "$=" => a.ends_with(val),
                "*=" => a.contains(val),
                "!=" => a != val,
                _ => a == val,
            };
        }
    }
    attr(n, inner.trim()).is_some()
}

/// `SvgImporter.importSvg` into the shape at `path`: stretched over the shape's bounds when
/// `fill`, else at the SVG's own size with bounds taken from the result.
pub fn import(swf: &mut Swf, path: &[usize], svg: &str, fill: bool) -> Result<()> {
    let tag_ref = swf.tag(path).ok_or_else(|| bad(format!("no tag at {path:?}")))?;
    let mut shape = ShapeTag::read(tag_ref)?;
    let doc = Document::parse_with_options(svg, ParsingOptions { allow_dtd: true, ..Default::default() }).map_err(|e: roxmltree::Error| bad(format!("SVG: {e}")))?;
    let root = doc.root_element();
    if tag(root) != "svg" {
        return Err(bad("SVG root element should be 'svg'"));
    }
    let mut ids = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = attr(n, "id") {
            ids.insert(id.to_string(), n.id());
        }
    }
    let rect = shape.bounds;
    let (mut width, mut height) = (800.0, 600.0);
    let mut vb = None;
    if let Some(v) = attr(root, "viewBox") {
        let a = number_list(v);
        let p = |i: usize| a.get(i).map_or(Some(0.0), |s| java_double(s)).ok_or_else(|| bad("invalid viewBox"));
        let b = (p(0)?, p(1)?, p(2)?, p(3)?);
        (width, height) = (b.2, b.3);
        vb = Some(b);
    }
    let mut imp = Importer {
        doc: &doc,
        ids,
        css: HashMap::new(),
        bitmaps: HashMap::new(),
        view_box: (0.0, 0.0, 0.0, 0.0),
        swf,
        shape_path: path.to_vec(),
        shape_num: shape.shape_num,
        flags: shape.flags & !0x07,
        records: Vec::new(),
    };
    if let Some(w) = attr(root, "width") {
        width = imp.length(w, width)?;
    }
    if let Some(h) = attr(root, "height") {
        height = imp.length(h, height)?;
    }
    let vb = vb.unwrap_or((0.0, 0.0, width, height));
    imp.view_box = vb;
    let mut t = M::ID;
    if fill {
        let (rx, ry) = ((rect.x_max - rect.x_min) as f64 / width, (rect.y_max - rect.y_min) as f64 / height);
        t = M::scale(rx / 20.0, ry / 20.0);
        t.translate_by(rect.x_min as f64 / rx, rect.y_min as f64 / ry);
    }
    t = t.pre_concat(&M::translate(-vb.0, -vb.1));
    if vb.2 != 0.0 && vb.3 != 0.0 {
        t = t.pre_concat(&M::scale(width / vb.2, height / vb.3));
    }
    imp.process_children(root, &t)?;
    let records = std::mem::take(&mut imp.records);
    let (flags, shape_path) = (imp.flags, imp.shape_path.clone());
    shape.shape = Shape { styles: Styles::default(), records };
    if shape.shape_num == 4 {
        shape.flags = flags;
    }
    if !fill {
        shape.bounds = bounds(&shape.shape.records, &[], shape.shape_num, false);
        if shape.shape_num == 4 {
            shape.edge_bounds = bounds(&shape.shape.records, &[], 4, true);
        }
    }
    swf.set_tag_body(&shape_path, shape.write())
}
