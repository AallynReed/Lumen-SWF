//! Fonts and texts in the XFL: `DOMFontItem`s, static, dynamic and input texts (with their
//! HTML text parser), and filters.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::convert::Converter;
use super::model::{DMatrix, EditText, JMatrix};
use super::num::{double_str, java_double};
use super::scripts::Accessibility;
use super::shapes::{convert_matrix, hex};
use super::xml::XmlWriter;
use crate::filters::Filter;
use crate::io::Reader;
use crate::tags::{code::*, Tag};
use crate::types::Rgba;

fn rad_to_deg(r: f64) -> f64 {
    r * 180.0 / std::f64::consts::PI
}

fn alpha(c: &Rgba) -> f32 {
    c.a as f32 / 255.0
}

/// `convertFilter`.
pub fn convert_filter(f: &Filter, w: &mut XmlWriter) {
    let ds = |v: f64| double_str(v);
    match f {
        Filter::DropShadow(s) => {
            w.start("DropShadowFilter");
            if s.color.a != 255 {
                w.attr("alpha", &ds(alpha(&s.color) as f64));
            }
            w.attr("angle", &ds(rad_to_deg(s.angle)));
            w.attr("blurX", &ds(s.blur_x));
            w.attr("blurY", &ds(s.blur_y));
            w.attr("color", &hex(&s.color));
            w.attr("distance", &ds(s.distance));
            if !s.composite_source {
                w.attr_bool("hideObject", true);
            }
            if s.inner {
                w.attr_bool("inner", true);
            }
            if s.knockout {
                w.attr_bool("knockout", true);
            }
            w.attr_i("quality", s.passes as i64);
            w.attr("strength", &ds(s.strength as f32 as f64));
            w.end();
        }
        Filter::Blur { blur_x, blur_y, passes } => {
            w.start("BlurFilter");
            w.attr("blurX", &ds(*blur_x));
            w.attr("blurY", &ds(*blur_y));
            w.attr_i("quality", *passes as i64);
            w.end();
        }
        Filter::Glow(g) => {
            w.start("GlowFilter");
            if g.color.a != 255 {
                w.attr_f("alpha", alpha(&g.color));
            }
            w.attr("blurX", &ds(g.blur_x));
            w.attr("blurY", &ds(g.blur_y));
            w.attr("color", &hex(&g.color));
            if g.inner {
                w.attr_bool("inner", true);
            }
            if g.knockout {
                w.attr_bool("knockout", true);
            }
            w.attr_i("quality", g.passes as i64);
            w.attr("strength", &ds(g.strength as f32 as f64));
            w.end();
        }
        Filter::Bevel(b) => {
            let black = Rgba { r: 0, g: 0, b: 0, a: 255 };
            let (shadow, highlight) = (b.colors.first().copied().unwrap_or(black), b.colors.last().copied().unwrap_or(black));
            w.start("BevelFilter");
            w.attr("blurX", &ds(b.blur_x));
            w.attr("blurY", &ds(b.blur_y));
            w.attr_i("quality", b.passes as i64);
            w.attr("angle", &ds(rad_to_deg(b.angle)));
            w.attr_d("distance", b.distance);
            if highlight.a != 255 {
                w.attr_f("highlightAlpha", alpha(&highlight));
            }
            w.attr("highlightColor", &hex(&highlight));
            if b.knockout {
                w.attr_bool("knockout", true);
            }
            if shadow.a != 255 {
                w.attr_f("shadowAlpha", alpha(&shadow));
            }
            w.attr("shadowColor", &hex(&shadow));
            w.attr("strength", &ds(b.strength as f32 as f64));
            if b.on_top && !b.inner {
                w.attr("type", "full");
            } else if !b.inner {
                w.attr("type", "outer");
            }
            w.end();
        }
        Filter::GradientGlow(b) | Filter::GradientBevel(b) => {
            w.start(if matches!(f, Filter::GradientGlow(_)) { "GradientGlowFilter" } else { "GradientBevelFilter" });
            w.attr("angle", &ds(rad_to_deg(b.angle)));
            w.attr("blurX", &ds(b.blur_x));
            w.attr("blurY", &ds(b.blur_y));
            w.attr_i("quality", b.passes as i64);
            w.attr("distance", &ds(b.distance));
            if b.knockout {
                w.attr_bool("knockout", true);
            }
            w.attr("strength", &ds(b.strength as f32 as f64));
            if b.on_top && !b.inner {
                w.attr("type", "full");
            } else if !b.inner {
                w.attr("type", "outer");
            }
            for (c, r) in b.colors.iter().zip(&b.ratios) {
                w.start_with("GradientEntry", &[("color", &hex(c))]);
                if c.a != 255 {
                    w.attr_f("alpha", alpha(c));
                }
                w.attr("ratio", &ds((*r as f32) as f64 / 255.0));
                w.end();
            }
            w.end();
        }
        Filter::ColorMatrix(m) => {
            let (b, c, s, h) = adjust_color(m);
            w.empty_with("AdjustColorFilter", &[("brightness", &b.to_string()), ("contrast", &c.to_string()), ("saturation", &s.to_string()), ("hue", &h.to_string())]);
        }
        Filter::Convolution { .. } => {}
    }
}

const CONTRAST_MAP: [f64; 101] = [
    0.0, 0.01, 0.02, 0.04, 0.05, 0.06, 0.07, 0.08, 0.1, 0.11, 0.12, 0.14, 0.15, 0.16, 0.17, 0.18, 0.20, 0.21, 0.22, 0.24, 0.25, 0.27, 0.28, 0.30, 0.32, 0.34, 0.36, 0.38, 0.40, 0.42, 0.44, 0.46, 0.48, 0.5, 0.53, 0.56,
    0.59, 0.62, 0.65, 0.68, 0.71, 0.74, 0.77, 0.80, 0.83, 0.86, 0.89, 0.92, 0.95, 0.98, 1.0, 1.06, 1.12, 1.18, 1.24, 1.30, 1.36, 1.42, 1.48, 1.54, 1.60, 1.66, 1.72, 1.78, 1.84, 1.90, 1.96, 2.0, 2.12, 2.25, 2.37, 2.50,
    2.62, 2.75, 2.87, 3.0, 3.2, 3.4, 3.6, 3.8, 4.0, 4.3, 4.7, 4.9, 5.0, 5.5, 6.0, 6.5, 6.8, 7.0, 7.3, 7.5, 7.8, 8.0, 8.4, 8.7, 9.0, 9.4, 9.6, 9.8, 10.0,
];

fn java_round_i(v: f64) -> i32 {
    super::num::java_round(v) as i32
}

/// `ColorMatrixConvertor.convertFromMatrix`: brightness, contrast, saturation and hue.
pub fn adjust_color(m: &[f32; 20]) -> (i32, i32, i32, i32) {
    let mut m2 = [[0f32; 5]; 5];
    let mut index = 0;
    for i in 0..4 {
        for row in m2.iter_mut() {
            row[i] = m[index];
            index += 1;
        }
    }
    let a11 = m2[0][0] as f64;
    let a12 = m2[0][1] as f64;
    let a13 = m2[0][2] as f64;
    let a21 = m2[1][0] as f64;
    let a22 = m2[1][1] as f64;
    let a23 = m2[1][2] as f64;
    let a31 = m2[2][0] as f64;
    let a32 = m2[2][1] as f64;
    let a33 = m2[2][2] as f64;
    let a41 = m2[4][0] as f64;
    let sq = (216225.0 * a11 * a11 + 332369.0 * a12 * a12 - 397828.0 * a12 * a13 + 281684.0 * a13 * a13 - 930.0 * a11 * (287.0 * a12 + 178.0 * a13)).sqrt();
    let b = (24872168661075.0 * a11 * a11 - 151430415740925.0 * a12 + 341095051289483.0 * a12 * a12 - 15302094789450.0 * a13 + 82428663495404.0 * a12 * a13 - 4592294873812.0 * a13 * a13
        + 43556251470.0 * sq
        + 2384730956550.0 * a12 * a41
        + 240977870700.0 * a13 * a41
        - 685925220.0 * sq * a41
        + 465.0 * a11 * (466201717582.0 * a12 + 55756962908.0 * a13 + 764132175.0 * (-127.0 + 2.0 * a41)))
        / (391687695450.0 * a11 * a11 + 5371575610858.0 * a12 * a12 + 1298089188904.0 * a12 * a13 - 72319604312.0 * a13 * a13 + 1860.0 * a11 * (1835439833.0 * a12 + 219515602.0 * a13));
    let c = (127.0 * (495225.0 * a11 + 1661845.0 * a12 + 167930.0 * a13 + 478.0 * sq)) / 717495.0;
    let mut h = 2.0 * (((-465.0 * a11 + 287.0 * a12 + 178.0 * a13 + sq) / (500.0 * (a12 - a13))).atan() + std::f64::consts::PI);
    let s = (1543.0 * (-103355550.0 * a11 * a11 - 158872382.0 * a12 * a12 + 190161784.0 * a12 * a13 - 134644952.0 * a13 * a13 + 1661845.0 * a12 * sq + 167930.0 * a13 * sq + 465.0 * a11 * (274372.0 * a12 + 170168.0 * a13 + 1065.0 * sq)))
        / (195843847725.0 * a11 * a11 + 2685787805429.0 * a12 * a12 + 649044594452.0 * a12 * a13 - 36159802156.0 * a13 * a13 + 930.0 * a11 * (1835439833.0 * a12 + 219515602.0 * a13));
    let same = |a: f64, b: f64| a == b || (a - b).abs() < 0.00001;
    if same(a12, 1543.0 * a31 / 410.0)
        && same(a12, 1543.0 * a32 / 410.0)
        && same(a21, 3047.0 * a12 / 1543.0)
        && same(a23, 3047.0 * a12 / 1543.0)
        && same(a22, a11 + (1504.0 * a12) / 1543.0)
        && same((1133.0 * a12) / 1543.0 + a33, a11)
        && !same(a11, a12)
        && !same(a11 + 3457.0 * a12 / 1543.0, 0.0)
    {
        h = 0.0;
    }
    let brightness = java_round_i(if b.is_nan() { -100.0 } else { b });
    let contrast = if c == 127.0 {
        0
    } else if c - 127.0 < 0.0 {
        java_round_i((c - 127.0) * 100.0 / 127.0)
    } else {
        let mut c = (c - 127.0) / 127.0;
        c = super::num::java_round(c * 100.0) as f64 / 100.0;
        CONTRAST_MAP.iter().position(|&v| v >= c).unwrap_or(CONTRAST_MAP.len() - 1) as i32
    };
    let saturation = if s.is_nan() {
        -100
    } else if s == 1.0 {
        0
    } else if s - 1.0 < 0.0 {
        java_round_i((s - 1.0) * 100.0)
    } else {
        java_round_i(((s - 1.0) * 100.0) / 3.0)
    };
    if h.is_nan() {
        h = -std::f64::consts::PI;
    }
    let mut hue = java_round_i(h * 180.0 / std::f64::consts::PI);
    while hue > 180 {
        hue -= 360;
    }
    while hue < -180 {
        hue += 360;
    }
    (brightness, contrast, saturation, hue)
}

// ---- installed fonts, as Java's font environment names them ----

struct SysFace {
    id: resvg::usvg::fontdb::ID,
    ps: String,
    bold: bool,
    italic: bool,
    full: String,
    ascent: f64,
    descent: f64,
    gap: f64,
}

struct SysFonts {
    families: HashMap<String, Vec<SysFace>>,
}

fn sys_fonts() -> &'static SysFonts {
    static S: OnceLock<SysFonts> = OnceLock::new();
    S.get_or_init(|| {
        let db = crate::render::raster::fonts();
        let mut families: HashMap<String, Vec<SysFace>> = HashMap::new();
        for f in db.faces() {
            db.with_face_data(f.id, |data, index| {
                let Ok(face) = ttf_parser::Face::parse(data, index) else { return };
                let name = |id: u16| {
                    let names: Vec<_> = face.names().into_iter().filter(|n| n.name_id == id).collect();
                    names.iter().find(|n| n.is_unicode() && n.language_id == 0x409).or_else(|| names.iter().find(|n| n.is_unicode())).and_then(|n| n.to_string())
                };
                let Some(family) = name(ttf_parser::name_id::FAMILY) else { return };
                let upem = face.units_per_em() as f64;
                families.entry(family).or_default().push(SysFace {
                    id: f.id,
                    ps: name(ttf_parser::name_id::POST_SCRIPT_NAME).unwrap_or_default(),
                    bold: face.is_bold(),
                    italic: face.is_italic(),
                    full: name(ttf_parser::name_id::FULL_NAME).unwrap_or_default(),
                    ascent: face.ascender() as f64 / upem,
                    descent: -(face.descender() as f64) / upem,
                    gap: face.line_gap() as f64 / upem,
                });
            });
        }
        SysFonts { families }
    })
}

/// `FontTag.isFontFamilyInstalled`.
pub fn installed_family(name: &str) -> Option<String> {
    let f = &sys_fonts().families;
    if f.contains_key(name) {
        return Some(name.to_string());
    }
    let before = name.split('_').next()?;
    (name.contains('_') && f.contains_key(before)).then(|| before.to_string())
}

fn face_of(family: &str, bold: bool, italic: bool) -> Option<&'static SysFace> {
    let faces = sys_fonts().families.get(family)?;
    faces.iter().find(|f| f.bold == bold && f.italic == italic).or_else(|| faces.iter().find(|f| !f.bold && !f.italic)).or_else(|| faces.first())
}

/// The face `new Font(name, style, size)` resolves to: a family, else a face name, else Dialog.
fn resolve_face(name: &str, bold: bool, italic: bool) -> Option<&'static SysFace> {
    if let Some(f) = face_of(name, bold, italic) {
        return Some(f);
    }
    if let Some(f) = sys_fonts().families.values().flatten().find(|f| f.full == name) {
        return Some(f);
    }
    face_of("Arial", bold, italic)
}

/// Height of a glyph outline at 1024 px (control points included, as JDK 17 bounds are).
fn glyph_height(face: &SysFace, c: char) -> Option<f64> {
    struct B(f32, f32, bool);
    impl ttf_parser::OutlineBuilder for B {
        fn move_to(&mut self, _: f32, y: f32) {
            self.add(y);
        }
        fn line_to(&mut self, _: f32, y: f32) {
            self.add(y);
        }
        fn quad_to(&mut self, _: f32, y1: f32, _: f32, y: f32) {
            self.add(y1);
            self.add(y);
        }
        fn curve_to(&mut self, _: f32, y1: f32, _: f32, y2: f32, _: f32, y: f32) {
            self.add(y1);
            self.add(y2);
            self.add(y);
        }
        fn close(&mut self) {}
    }
    impl B {
        fn add(&mut self, y: f32) {
            self.0 = self.0.min(y);
            self.1 = self.1.max(y);
            self.2 = true;
        }
    }
    let db = crate::render::raster::fonts();
    db.with_face_data(face.id, |data, index| {
        let f = ttf_parser::Face::parse(data, index).ok()?;
        let g = f.glyph_index(c)?;
        let mut b = B(f32::MAX, f32::MIN, false);
        f.outline_glyph(g, &mut b);
        let k = 1024.0 / f.units_per_em() as f32;
        Some(if b.2 { ((b.1 - b.0) * k) as f64 } else { 0.0 })
    })
    .flatten()
}

fn can_display(face: &SysFace, c: char) -> bool {
    let db = crate::render::raster::fonts();
    db.with_face_data(face.id, |data, index| ttf_parser::Face::parse(data, index).ok().and_then(|f| f.glyph_index(c)).is_some()).unwrap_or(false)
}

/// `new Font(family, style, size).getPSName()`.
pub fn ps_name(family: &str, bold: bool, italic: bool) -> String {
    face_of(family, bold, italic).map_or_else(|| family.to_string(), |f| f.ps.clone())
}

fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// `FontTag.getDefaultFontName`: the first face name of Times New Roman (or Arial) in a
/// Java HashMap's order.
pub fn default_font_name() -> String {
    let f = &sys_fonts().families;
    let fam = ["Times New Roman", "Arial"].into_iter().find(|n| f.contains_key(*n)).map(str::to_string).or_else(|| f.keys().next().cloned()).unwrap_or_else(|| "Times New Roman".into());
    let Some(faces) = f.get(&fam) else { return fam };
    let bucket = |s: &str| {
        let h = java_string_hash(s);
        ((h ^ ((h as u32) >> 16) as i32) & 15) as u32
    };
    faces.iter().map(|x| x.full.clone()).min_by_key(|n| bucket(n)).unwrap_or(fam)
}

/// `getFontNameWithFallback`.
fn name_with_fallback(name: &str) -> String {
    let f = &sys_fonts().families;
    if f.contains_key(name) {
        return name.to_string();
    }
    for n in ["Times New Roman", "Arial"] {
        if f.contains_key(n) {
            return n.into();
        }
    }
    f.keys().next().cloned().unwrap_or_default()
}

/// Java `LineMetrics` (ascent, descent, leading) of a system font at an integer size.
fn line_metrics(family: &str, bold: bool, italic: bool, size: i32) -> (f64, f64, f64) {
    match face_of(family, bold, italic) {
        Some(f) => ((f.ascent * size as f64) as f32 as f64, (f.descent * size as f64) as f32 as f64, (f.gap * size as f64) as f32 as f64),
        None => (0.0, 0.0, 0.0),
    }
}

// ---- SWF fonts ----

/// `SHAPERECORD.getBounds` height: maxima start at 0, minima only after the first move.
fn glyph_bounds_height(records: &[crate::shape::Record]) -> i32 {
    use crate::shape::Record;
    let (mut x, mut y) = (0i32, 0i32);
    let (mut max_y, mut min_y) = (0i32, i32::MAX);
    let mut started = false;
    for r in records {
        match *r {
            Record::StyleChange { move_to, .. } => {
                if let Some((mx, my)) = move_to {
                    x = mx;
                    y = my;
                    started = true;
                }
            }
            Record::Straight { dx, dy } => {
                x += dx;
                y += dy;
            }
            Record::Curved { cdx, cdy, adx, ady } => {
                let cy = y + cdy;
                max_y = max_y.max(cy);
                if started {
                    min_y = min_y.min(cy);
                }
                x += cdx + adx;
                y += cdy + ady;
            }
        }
        let _ = x;
        max_y = max_y.max(y);
        if started {
            min_y = min_y.min(y);
        }
    }
    if min_y == i32::MAX {
        min_y = 0;
    }
    max_y - min_y
}

/// `FontNormalizer`: per-font scales that make embedded glyphs match the system font's size.
#[derive(Default)]
pub struct Norm {
    pub scale: HashMap<u16, f64>,
    pub inverted: std::collections::HashSet<u16>,
}

/// What the converter reads from a DefineFont/2/3 (`FontTag`).
pub struct FontData {
    pub id: u16,
    /// Glyph heights (`SHAPE.getBounds(1)` height, in glyph units).
    pub heights: Vec<i32>,
    pub name_in_tag: Option<String>,
    pub name_tag: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub small: bool,
    pub has_layout: bool,
    pub divider: f64,
    pub ascent: i32,
    pub descent: i32,
    pub leading: i32,
    pub codes: Vec<u16>,
    pub advances: Vec<f64>,
    pub kerning: Vec<(u16, u16, i16)>,
}

impl FontData {
    pub fn read(swf: &crate::Swf, tag: &Tag) -> Option<FontData> {
        if !matches!(tag.code, DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3) {
            return None;
        }
        let font = crate::export::font::Font::read(swf, tag).ok()?;
        let id = font.id;
        let mut small = false;
        let mut name_in_tag = None;
        let mut r = Reader::new(&tag.body);
        r.u16().ok()?;
        if tag.code == DEFINE_FONT {
            swf.walk(|_, t| {
                if (t.code == DEFINE_FONT_INFO || t.code == DEFINE_FONT_INFO2) && t.character_id() == Some(id) && name_in_tag.is_none() {
                    let mut r = Reader::new(&t.body);
                    let _ = r.u16();
                    let len = r.u8().unwrap_or(0) as usize;
                    let name = r.bytes(len).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
                    small = r.u8().is_ok_and(|f| f & 0x20 != 0);
                    name_in_tag = Some(name);
                }
            });
        } else {
            let flags = r.u8().ok()?;
            small = flags & 0x20 != 0;
            r.u8().ok()?;
            let len = r.u8().ok()? as usize;
            let mut name = String::from_utf8_lossy(r.bytes(len).ok()?).into_owned();
            if let Some(p) = name.find('\0') {
                name.truncate(p);
            }
            name_in_tag = Some(name);
        }
        let mut name_tag = None;
        swf.walk(|_, t| {
            if t.code == DEFINE_FONT_NAME && t.character_id() == Some(id) && name_tag.is_none() {
                let mut r = Reader::new(&t.body);
                let _ = r.u16();
                name_tag = r.string().ok();
            }
        });
        let has_layout = font.ascent.is_some();
        let heights = font.glyphs.iter().map(|g| glyph_bounds_height(&g.shape)).collect();
        let codes = if tag.code == DEFINE_FONT && name_in_tag.is_none() { Vec::new() } else { font.glyphs.iter().map(|g| g.code).collect() };
        Some(FontData {
            id,
            heights,
            name_in_tag,
            name_tag,
            bold: font.bold,
            italic: font.italic,
            small,
            has_layout,
            divider: font.divider,
            ascent: font.ascent.unwrap_or(0) as i32,
            descent: font.descent.unwrap_or(0) as i32,
            leading: font.leading.unwrap_or(0) as i32,
            codes,
            advances: font.glyphs.iter().map(|g| g.advance.map_or(-1.0, |a| a as f64)).collect(),
            kerning: font.kerning.clone(),
        })
    }

    /// The name the converter starts from: DefineFontName, the tag's own, else the default.
    pub fn name(&self) -> String {
        self.name_tag.clone().or_else(|| self.name_in_tag.clone()).unwrap_or_else(default_font_name)
    }

    /// The font as FontNormalizer leaves it after scaling (and flipping, when inverted).
    pub fn normalized(&self, scale: f64, inverted: bool) -> FontData {
        let r = |v: f64| super::num::java_round(v) as f64;
        let mut advances = self.advances.clone();
        let (mut ascent, mut descent, mut leading) = (self.ascent, self.descent, self.leading);
        let mut kerning = self.kerning.clone();
        if self.has_layout {
            for a in advances.iter_mut() {
                *a = r(*a * scale);
            }
            ascent = r(ascent as f64 * scale) as i32;
            descent = r(descent as f64 * scale) as i32;
            leading = r(leading as f64 * scale) as i32;
            for k in kerning.iter_mut() {
                k.2 = r(k.2 as f64 * scale) as i16;
            }
            if inverted {
                std::mem::swap(&mut ascent, &mut descent);
            }
            for c in [0x20u16, 0xa0] {
                if let Some(g) = self.codes.iter().position(|&x| x == c) {
                    if self.advances[g] <= 1.0 {
                        advances[g] = 512.0;
                    }
                }
            }
        }
        FontData {
            id: self.id,
            heights: self.heights.clone(),
            name_in_tag: self.name_in_tag.clone(),
            name_tag: self.name_tag.clone(),
            bold: self.bold,
            italic: self.italic,
            small: self.small,
            has_layout: self.has_layout,
            divider: self.divider,
            ascent,
            descent,
            leading,
            codes: self.codes.clone(),
            advances,
            kerning,
        }
    }

    /// `getSystemFontName`: the installed face named like the font, else the default.
    fn system_font_name(&self) -> String {
        let name = self.name_tag.clone().or_else(|| self.name_in_tag.clone()).unwrap_or_default();
        let fonts = sys_fonts();
        let by_name = |n: &str| fonts.families.values().flatten().any(|f| f.full == n);
        if by_name(&name) {
            return name;
        }
        if let Some(before) = name.split_once('_').map(|p| p.0) {
            if by_name(before) {
                return before.to_string();
            }
        }
        default_font_name()
    }

    /// FontNormalizer's scale: system glyph height over embedded glyph height.
    fn system_scale(&self) -> f64 {
        let sys = self.system_font_name();
        let face = resolve_face(&sys, self.bold, self.italic);
        let n = self.heights.len();
        let mut h = 0.0;
        let mut sum = 0.0;
        let mut system = None;
        'pass: for alpha_only in [true, false] {
            sum = 0.0;
            for i in 0..n {
                h = self.heights[i] as f64 / self.divider;
                if h <= 0.0 {
                    continue;
                }
                sum += h;
                let c = self.glyph_char(i);
                if alpha_only && !c.is_alphabetic() {
                    continue;
                }
                let Some(f) = face else { continue };
                if !can_display(f, c) {
                    continue;
                }
                system = glyph_height(f, c);
                break 'pass;
            }
        }
        let system = match system {
            Some(s) => s,
            None => {
                h = sum / n as f64;
                face_of("Times New Roman", self.bold, self.italic).and_then(|f| glyph_height(f, 'H')).unwrap_or(h)
            }
        };
        system / h
    }

    pub fn chars(&self) -> String {
        self.codes.iter().map(|&c| char::from_u32(c as u32).unwrap_or('\u{fffd}')).collect()
    }

    fn glyph_char(&self, i: usize) -> char {
        self.codes.get(i).and_then(|&c| char::from_u32(c as u32)).unwrap_or('\u{fffd}')
    }

    fn kerning_adjustment(&self, a: usize, b: usize) -> i32 {
        let (Some(&c1), Some(&c2)) = (self.codes.get(a), self.codes.get(b)) else { return 0 };
        self.kerning.iter().find(|k| k.0 == c1 && k.1 == c2).map_or(0, |k| k.2 as i32)
    }

    /// The face XFL text attributes name: the installed font's PostScript name when the
    /// family is installed.
    pub fn face(&self, bold: bool, italic: bool) -> String {
        let name = self.name();
        match installed_family(&name) {
            Some(f) => ps_name(&f, bold, italic),
            None => name,
        }
    }
}

fn read_utf16_chars(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `CharacterRanges`: the embed ranges FLA fonts list.
fn character_ranges() -> &'static Vec<Vec<u16>> {
    static R: OnceLock<Vec<Vec<u16>>> = OnceLock::new();
    R.get_or_init(|| {
        include_str!("character_ranges.txt")
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let parts = l.split_once(':').map_or("", |p| p.1);
                let mut v = Vec::new();
                for r in parts.split(',') {
                    let (a, b) = r.split_once('-').unwrap_or((r, r));
                    let (Ok(a), Ok(b)) = (u32::from_str_radix(a.trim(), 16), u32::from_str_radix(b.trim(), 16)) else { continue };
                    for c in a..=b {
                        v.push(c as u16);
                    }
                }
                v
            })
            .collect()
    })
}

fn twip(v: f64) -> f64 {
    v / 20.0
}

/// CSMTextSettings of a text: thickness and sharpness.
fn csm_settings(swf: &crate::Swf, id: u16) -> Option<(f32, f32)> {
    for t in &swf.timeline.tags {
        if t.code == CSM_SETTINGS {
            let mut r = Reader::new(&t.body);
            if r.u16().ok() == Some(id) {
                r.u8().ok()?;
                let th = f32::from_bits(r.u32().ok()?);
                let sh = f32::from_bits(r.u32().ok()?);
                return Some((th, sh));
            }
        }
    }
    None
}

/// A token of the edit text HTML lexer.
enum Tok {
    Open(String, Vec<(String, String)>, bool),
    Close(String),
    Entity(String),
    Numeric(String),
    Chars(String),
}

/// `XmlLexer`: None where it throws (a lone `&` or `<`, an unterminated tag).
fn lex_html(src: &str) -> Option<Vec<Tok>> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    let name_start = |c: char| c == ':' || c == '_' || c.is_ascii_alphabetic() || (c as u32) >= 0xc0;
    let name_char = |c: char| name_start(c) || c == '-' || c == '.' || c.is_ascii_digit() || c == '\u{b7}';
    while i < b.len() {
        let c = b[i];
        if c == '&' {
            let mut j = i + 1;
            let numeric = b.get(j) == Some(&'#');
            if numeric {
                j += 1;
            }
            let st = j;
            while j < b.len() && (if numeric { b[j].is_ascii_digit() } else { b[j].is_ascii_lowercase() }) {
                j += 1;
            }
            if j == st || b.get(j) != Some(&';') {
                return None;
            }
            let v: String = b[st..j].iter().collect();
            out.push(if numeric { Tok::Numeric(v) } else { Tok::Entity(v) });
            i = j + 1;
            continue;
        }
        if c == '<' {
            let rest: String = b[i..b.len().min(i + 9)].iter().collect();
            if rest.starts_with("<!--") {
                let s: String = b[i..].iter().collect();
                let e = s.find("-->")?;
                i += s[..e].chars().count() + 3;
                continue;
            }
            if rest.starts_with("<![CDATA[") {
                let s: String = b[i..].iter().collect();
                let e = s.find("]]>")?;
                i += s[..e].chars().count() + 3;
                continue;
            }
            if b.get(i + 1) == Some(&'/') {
                let mut j = i + 2;
                if !b.get(j).is_some_and(|&c| name_start(c)) {
                    return None;
                }
                while j < b.len() && name_char(b[j]) {
                    j += 1;
                }
                let name: String = b[i + 2..j].iter().collect();
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                if b.get(j) != Some(&'>') {
                    return None;
                }
                out.push(Tok::Close(name));
                i = j + 1;
                continue;
            }
            let mut j = i + 1;
            if b.get(j) == Some(&'?') {
                let s: String = b[i..].iter().collect();
                let e = s.find("?>")?;
                i += s[..e].chars().count() + 2;
                continue;
            }
            if !b.get(j).is_some_and(|&c| name_start(c)) {
                return None;
            }
            while j < b.len() && name_char(b[j]) {
                j += 1;
            }
            let name: String = b[i + 1..j].iter().collect();
            let mut attrs: Vec<(String, String)> = Vec::new();
            let mut pending: Option<String> = None;
            loop {
                let Some(&c) = b.get(j) else { return None };
                if c.is_whitespace() {
                    j += 1;
                } else if c == '>' {
                    out.push(Tok::Open(name, attrs, false));
                    j += 1;
                    break;
                } else if c == '/' && b.get(j + 1) == Some(&'>') {
                    out.push(Tok::Open(name, attrs, true));
                    j += 2;
                    break;
                } else if c == '"' || c == '\'' {
                    let st = j + 1;
                    let mut k = st;
                    while k < b.len() && b[k] != c && b[k] != '\r' && b[k] != '\n' {
                        k += 1;
                    }
                    if b.get(k) != Some(&c) {
                        return None;
                    }
                    let v: String = b[st..k].iter().collect();
                    let n = pending.take()?;
                    match attrs.iter_mut().find(|a| a.0 == n) {
                        Some(a) => a.1 = v,
                        None => attrs.push((n, v)),
                    }
                    j = k + 1;
                } else if name_start(c) {
                    let st = j;
                    while j < b.len() && name_char(b[j]) {
                        j += 1;
                    }
                    if b.get(j) != Some(&'=') {
                        return None;
                    }
                    pending = Some(b[st..j].iter().collect());
                    j += 1;
                } else {
                    return None;
                }
            }
            i = j;
            continue;
        }
        if c == '>' {
            return None;
        }
        let st = i;
        while i < b.len() && !matches!(b[i], '<' | '>' | '&') {
            i += 1;
        }
        out.push(Tok::Chars(b[st..i].iter().collect()));
    }
    Some(out)
}

impl<'a> Converter<'a> {
    pub fn norm(&self) -> &Norm {
        self.norm.get_or_init(|| self.compute_norm())
    }

    fn font_by_name_in_tag(&self, name: &str, bold: bool, italic: bool) -> Option<u16> {
        self.characters.iter().copied().find(|&id| self.font_data(id).is_some_and(|f| f.name_in_tag.as_deref() == Some(name) && f.bold == bold && f.italic == italic))
    }

    fn font_by_export_name(&self, name: &str) -> Option<u16> {
        self.variables.iter().find(|(id, n)| n.as_str() == name && self.font_data(**id).is_some()).map(|(&id, _)| id)
    }

    /// The fonts of an edit text: its own, and with outlines those `<font face size>` selects.
    fn edit_text_fonts(&self, det: &EditText) -> Vec<u16> {
        let main = if det.has_font_class { self.char_by_class(&det.font_class) } else { Some(det.font_id as u16) };
        let mut out: Vec<u16> = main.into_iter().collect();
        if !det.html {
            return out;
        }
        let Some(toks) = lex_html(&det.text) else { return out };
        let mut styles: Vec<(bool, bool, Option<String>)> = vec![(false, false, None)];
        for t in toks {
            match t {
                Tok::Open(name, attrs, _) => {
                    let (mut bold, mut italic, mut face) = styles.last().cloned().unwrap();
                    match name.as_str() {
                        "b" => bold = true,
                        "i" => italic = true,
                        "u" => {}
                        "font" => {
                            if let Some(f) = attrs.iter().find(|a| a.0 == "face").map(|a| a.1.clone()).filter(|f| !f.is_empty()) {
                                face = Some(f);
                            }
                            if attrs.iter().any(|a| a.0 == "size" && !a.1.is_empty()) && det.use_outlines {
                                if let Some(f) = &face {
                                    match self.font_by_export_name(f).or_else(|| self.font_by_name_in_tag(f, bold, italic)) {
                                        Some(id) => {
                                            if !out.contains(&id) {
                                                out.push(id);
                                            }
                                        }
                                        None => face = None,
                                    }
                                }
                            }
                        }
                        _ => continue,
                    }
                    styles.push((bold, italic, face));
                }
                Tok::Close(name) => {
                    if matches!(name.as_str(), "b" | "i" | "u" | "font") && styles.len() > 1 {
                        styles.pop();
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn compute_norm(&self) -> Norm {
        let mut ids: Vec<u16> = Vec::new();
        let mut inverted: Vec<u16> = Vec::new();
        let mut upright: Vec<u16> = Vec::new();
        let push = |v: &mut Vec<u16>, id: u16| {
            if !v.contains(&id) {
                v.push(id);
            }
        };
        for &id in &self.characters {
            let Some(tag) = self.char_tag(id) else { continue };
            if tag.code == DEFINE_EDIT_TEXT {
                if let Some(det) = EditText::read(tag) {
                    for f in self.edit_text_fonts(&det) {
                        push(&mut ids, f);
                    }
                }
            } else if tag.code == DEFINE_TEXT || tag.code == DEFINE_TEXT2 {
                let Ok(t) = crate::text::StaticText::read(tag) else { continue };
                let inv = t.matrix.scale.is_some_and(|s| s.1 < 0.0);
                for r in &t.records {
                    if let Some((f, _)) = r.font {
                        push(if inv { &mut inverted } else { &mut upright }, f);
                        push(&mut ids, f);
                    }
                }
            }
        }
        let mut n = Norm::default();
        for id in ids {
            if upright.contains(&id) {
                inverted.retain(|&x| x != id);
            }
            let Some(f) = self.font_data(id) else { continue };
            n.scale.insert(id, f.system_scale());
        }
        n.inverted = inverted.into_iter().collect();
        n
    }

    /// A font as the normalizer leaves it.
    pub fn norm_font(&self, id: u16) -> Option<std::rc::Rc<FontData>> {
        if let Some(f) = self.norm_cache.borrow().get(&id) {
            return f.clone();
        }
        let f = self.font_data(id).map(|f| match self.norm().scale.get(&id) {
            Some(&s) => std::rc::Rc::new(f.normalized(s, self.norm().inverted.contains(&id))),
            None => f,
        });
        self.norm_cache.borrow_mut().insert(id, f.clone());
        f
    }

    /// The static text as the normalizer leaves it: rescaled heights, flipped offsets of
    /// inverted fonts, and a real advance for a zero last advance.
    fn normalized_static(&self, text: &crate::text::StaticText) -> crate::text::StaticText {
        let mut t2 = text.clone();
        let norm = self.norm();
        let mut font_id: Option<u16> = None;
        let mut height = 12 * 20;
        for i in 0..text.records.len() {
            let rec = &text.records[i];
            if let Some((f, h)) = rec.font {
                font_id = Some(f);
                height = h as i32;
                if let Some(&s) = norm.scale.get(&f) {
                    height = super::num::java_round(h as f64 / s) as i32;
                    t2.records[i].font = Some((f, height.clamp(0, u16::MAX as i32) as u16));
                }
            }
            let inv = font_id.is_some_and(|f| norm.inverted.contains(&f));
            if inv {
                if let Some(y) = rec.y {
                    t2.records[i].y = Some(y.wrapping_neg());
                }
            }
            if rec.glyphs.last().is_some_and(|g| g.1 == 0) {
                let Some(f) = font_id.and_then(|f| self.norm_font(f)) else { continue };
                let gi = rec.glyphs.len() - 1;
                let adv = f.advances.get(rec.glyphs[gi].0 as usize).copied().unwrap_or(-1.0);
                let a = super::num::java_round(adv * height as f64 / (1024.0 * f.divider)) as i32;
                t2.records[i].glyphs[gi].1 = a;
                if i + 1 < text.records.len() && text.records[i + 1].x.is_none() && !t2.records[i + 1].glyphs.is_empty() {
                    t2.records[i + 1].glyphs[0].1 -= a;
                }
            }
        }
        t2
    }

    /// The edit text as the normalizer leaves it: font height, leading and HTML sizes rescaled.
    fn normalized_edit(&self, det: &EditText) -> EditText {
        let mut d2 = det.clone();
        let norm = self.norm();
        let main = if det.has_font_class { self.char_by_class(&det.font_class) } else { Some(det.font_id as u16) };
        let mut changed = false;
        if let Some(&s) = main.and_then(|m| norm.scale.get(&m)) {
            d2.font_height = super::num::java_round(det.font_height as f64 / s) as i32;
            d2.leading = super::num::java_round(det.leading as f64 / s) as i32;
            changed = true;
        }
        if !det.html {
            return d2;
        }
        let Some(toks) = lex_html(&det.text) else { return d2 };
        let mut styles: Vec<(bool, bool, Option<String>)> = vec![(false, false, None)];
        let mut sb = String::new();
        for t in toks {
            match t {
                Tok::Open(name, mut attrs, closed) => {
                    let (mut bold, mut italic, mut face) = styles.last().cloned().unwrap();
                    let mut push = true;
                    match name.as_str() {
                        "b" => bold = true,
                        "i" => italic = true,
                        "u" => {}
                        "font" => {
                            if let Some(f) = attrs.iter().find(|a| a.0 == "face").map(|a| a.1.clone()).filter(|f| !f.is_empty()) {
                                face = Some(f);
                            }
                            let size = attrs.iter().find(|a| a.0 == "size").map(|a| a.1.clone()).filter(|s| !s.is_empty());
                            if let (Some(size), true) = (size, det.use_outlines) {
                                if let Some(f) = face.clone() {
                                    match self.font_by_export_name(&f).or_else(|| self.font_by_name_in_tag(&f, bold, italic)) {
                                        None => face = None,
                                        Some(id) => {
                                            if let Some(&sc) = norm.scale.get(&id) {
                                                changed = true;
                                                let first = size.chars().next().unwrap();
                                                let new = if first != '+' && first != '-' {
                                                    size.parse::<i32>().ok().map(|v| super::num::java_round(v as f64 / sc).to_string())
                                                } else {
                                                    size[1..].parse::<i32>().ok().map(|v| format!("{first}{}", super::num::java_round((v as f64 * 20.0) as i32 as f64 / sc)))
                                                };
                                                if let Some(new) = new {
                                                    if let Some(a) = attrs.iter_mut().find(|a| a.0 == "size") {
                                                        a.1 = new;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => push = false,
                    }
                    if push && !closed {
                        styles.push((bold, italic, face));
                    }
                    sb.push('<');
                    sb += &name;
                    for (k, v) in &attrs {
                        sb += &format!(" {k}=\"{v}\"");
                    }
                    sb += if closed { "/>" } else { ">" };
                }
                Tok::Close(name) => {
                    if matches!(name.as_str(), "b" | "i" | "u" | "font") && styles.len() > 1 {
                        styles.pop();
                    }
                    sb += &format!("</{name}>");
                }
                Tok::Entity(e) => sb += &format!("&{e};"),
                Tok::Numeric(e) => sb += &format!("&#{e};"),
                Tok::Chars(c) => sb += &c,
            }
        }
        if changed {
            d2.text = sb;
        }
        d2
    }

    pub fn font_data(&self, id: u16) -> Option<std::rc::Rc<FontData>> {
        if let Some(f) = self.font_cache.borrow().get(&id) {
            return f.clone();
        }
        let f = self.char_tag(id).and_then(|t| FontData::read(self.swf, t)).map(std::rc::Rc::new);
        self.font_cache.borrow_mut().insert(id, f.clone());
        f
    }

    pub fn convert_fonts(&mut self, w: &mut XmlWriter) {
        let fonts: Vec<u16> = self.characters.iter().copied().filter(|&id| self.font_data(id).is_some()).collect();
        if !fonts.iter().any(|&id| self.font_data(id).is_some_and(|f| !f.codes.is_empty())) {
            return;
        }
        w.start("fonts");
        let mut counter = 0;
        for id in fonts {
            let f = self.font_data(id).unwrap();
            let chars = f.chars();
            if chars.is_empty() {
                continue;
            }
            let font_name = f.face(f.bold, f.italic);
            let set: std::collections::HashSet<u16> = read_utf16_chars(&chars).into_iter().collect();
            let mut embedded: Vec<u16> = read_utf16_chars(&chars);
            let mut ranges = String::new();
            for (r, codes) in character_ranges().iter().enumerate() {
                if codes.iter().all(|c| set.contains(c)) {
                    let rm: std::collections::HashSet<u16> = codes.iter().copied().collect();
                    embedded.retain(|c| !rm.contains(c));
                    if !ranges.is_empty() {
                        ranges.push('|');
                    }
                    ranges += &(r + 1).to_string();
                }
            }
            embedded.retain(|&c| c != 0xa0 && (c >= 32 || c == 9 || c == 10 || c == 13));
            counter += 1;
            let item_id = self.item_id();
            let name = self.symbol_name(id, "Font");
            w.start_with("DOMFontItem", &[("name", &name), ("itemID", &item_id), ("font", &font_name), ("size", "0"), ("id", &counter.to_string()), ("embedRanges", &ranges)]);
            self.write_linkage(w, id);
            if !embedded.is_empty() {
                w.attr("embeddedCharacters", &String::from_utf16_lossy(&embedded));
            }
            w.end();
        }
        w.end();
    }

    /// `convertText` for DefineText/DefineText2/DefineEditText.
    #[allow(clippy::too_many_arguments)]
    pub fn convert_text(&mut self, frame: i32, acc: &Accessibility, instance: Option<&str>, tag: &Tag, m: &JMatrix, filters: Option<&[Filter]>, w: &mut XmlWriter) {
        let mut filter_str = XmlWriter::new();
        if let Some(fl) = filters {
            filter_str.start("filters");
            for f in fl {
                convert_filter(f, &mut filter_str);
            }
            filter_str.end();
        }
        let id = tag.character_id().unwrap_or(0);
        let csm = csm_settings(self.swf, id);
        let mut rendering = Some("standard");
        let mut aa: Option<(String, String)> = None;
        if let Some((th, sh)) = csm {
            rendering = if th == 0.0 && sh == 0.0 { None } else { Some("customThicknessSharpness") };
            aa = Some((double_str(sh as f64), double_str(th as f64)));
        }
        if tag.code == DEFINE_TEXT || tag.code == DEFINE_TEXT2 {
            let Ok(text) = crate::text::StaticText::read(tag) else { return };
            let text = self.normalized_static(&text);
            self.convert_static_text(&text, rendering, aa, instance, m, w);
            w.raw(filter_str.as_str());
            w.end();
            return;
        }
        let Some(det) = EditText::read(tag) else { return };
        let det = self.normalized_edit(&det);
        let ft = if det.has_font {
            self.font_data(det.font_id as u16)
        } else if det.has_font_class {
            self.char_by_class(&det.font_class).and_then(|i| self.font_data(i))
        } else {
            None
        };
        if ft.as_ref().is_some_and(|f| f.small) {
            rendering = Some("bitmap");
        }
        if !det.use_outlines {
            rendering = Some("device");
        }
        let name = if det.was_static {
            "DOMStaticText"
        } else if det.read_only {
            "DOMDynamicText"
        } else {
            "DOMInputText"
        };
        w.start(name);
        if let Some(r) = rendering {
            w.attr("fontRenderingMode", r);
        }
        if let Some(n) = instance {
            w.attr("name", n);
            let a = acc.attributes(n, frame + 1);
            if !a.is_empty() {
                w.attr("hasAccessibleData", "true");
                for (k, v) in &a {
                    w.attr(k, v);
                }
            }
        }
        if let Some((s, t)) = &aa {
            w.attr("antiAliasSharpness", s);
            w.attr("antiAliasThickness", t);
        }
        let b = det.bounds;
        w.attr_d("width", twip((b.x_max - b.x_min) as f64) - 4.0);
        w.attr_d("height", twip((b.y_max - b.y_min) as f64) - 4.0);
        if det.border {
            w.attr_bool("border", true);
        }
        if det.html {
            w.attr_bool("renderAsHTML", true);
        }
        if det.no_select {
            w.attr_bool("isSelectable", false);
        }
        if det.multiline && det.word_wrap {
            w.attr("lineType", "multiline");
        } else if det.multiline {
            w.attr("lineType", "multiline no wrap");
        } else if det.password {
            w.attr("lineType", "password");
        }
        if det.has_max_length {
            w.attr_i("maxCharacters", det.max_length as i64);
        }
        if !det.variable.is_empty() {
            w.attr("variableName", &det.variable);
        }
        w.start("matrix");
        let m2 = DMatrix::of(m).pre_concatenate(&DMatrix::translate((b.x_min + 40) as f64, (b.y_min + 40) as f64));
        convert_matrix(&m2.to_jmatrix(), w);
        w.end();
        w.start("textRuns");
        let txt = if det.has_text { det.text.clone() } else { String::new() };
        if det.html {
            let runs = self.html_text(&det, ft.as_deref(), &txt);
            w.raw(&runs);
        } else {
            w.start("DOMTextRun");
            w.value("characters", &txt);
            let mut face = None;
            let mut size = -1;
            if det.has_font || det.has_font_class {
                if let Some(f) = &ft {
                    size = det.font_height;
                    face = Some(f.face(f.bold, f.italic));
                }
            }
            w.start("textAttrs");
            w.start("DOMTextAttrs");
            if det.has_layout {
                w.attr("alignment", ["left", "right", "center", "justify"].get(det.align as usize).copied().unwrap_or("unknown"));
            }
            w.attr_bool("rotation", true);
            if det.has_layout {
                if det.indent > -1 {
                    w.attr_d("indent", twip(det.indent as f64));
                }
                if det.left_margin > -1 {
                    w.attr_d("leftMargin", twip(det.left_margin as f64));
                }
                if det.leading > -1 {
                    w.attr_d("lineSpacing", twip(det.leading as f64));
                }
                if det.right_margin > -1 {
                    w.attr_d("rightMargin", twip(det.right_margin as f64));
                }
            }
            w.attr("autoKern", "false");
            if size > -1 {
                w.attr_d("size", twip(size as f64));
                w.attr_i("bitmapSize", size as i64);
            }
            if let Some(f) = &face {
                w.attr("face", f);
            }
            if det.has_color {
                w.attr("fillColor", &hex(&det.color));
                w.attr_f("alpha", alpha(&det.color));
            }
            w.end();
            w.end();
            w.end();
        }
        w.end();
        w.raw(filter_str.as_str());
        w.end();
    }

    fn convert_static_text(&mut self, text: &crate::text::StaticText, mut rendering: Option<&str>, aa: Option<(String, String)>, instance: Option<&str>, m: &JMatrix, w: &mut XmlWriter) {
        let left = double_str(text.matrix.tx as f64 / 20.0);
        for rec in &text.records {
            if let Some((fid, _)) = rec.font {
                if self.font_data(fid).is_some_and(|f| f.small) {
                    rendering = Some("bitmap");
                    break;
                }
            }
        }
        w.start("DOMStaticText");
        w.attr("left", &left);
        if let Some(r) = rendering {
            w.attr("fontRenderingMode", r);
        }
        if let Some(n) = instance {
            w.attr("instanceName", n);
        }
        if let Some((s, t)) = &aa {
            w.attr("antiAliasSharpness", s);
            w.attr("antiAliasThickness", t);
        }
        let (margins, spacings, line_spacing) = self.text_records_attributes(&text.records);
        let b = text.bounds;
        w.attr_i("width", ((b.x_max - b.x_min) / 2) as i64);
        w.attr_i("height", (b.y_max - b.y_min) as i64);
        w.attr_bool("autoExpand", true);
        w.attr_bool("isSelectable", false);
        w.start("matrix");
        convert_matrix(m, w);
        w.end();
        w.start("textRuns");
        let mut font: Option<std::rc::Rc<FontData>> = None;
        let mut face = String::new();
        let mut height = -1i32;
        let mut color: Option<[u8; 4]> = None;
        let mut first = true;
        for (r, rec) in text.records.iter().enumerate() {
            if rec.color.is_some() {
                color = rec.color;
            }
            if let Some((fid, h)) = rec.font {
                height = h as i32;
                font = self.font_data(fid);
                face = match &font {
                    Some(f) => f.face(f.bold, f.italic),
                    None => {
                        let n = default_font_name();
                        installed_family(&n).map_or(n.clone(), |f| ps_name(&f, false, false))
                    }
                };
            }
            let newline = !first && rec.y.is_some();
            first = false;
            let Some(f) = &font else { continue };
            w.start("DOMTextRun");
            w.start("characters");
            let mut s = String::new();
            if newline {
                s.push('\r');
            }
            for (g, _) in &rec.glyphs {
                s.push(f.glyph_char(*g as usize));
            }
            w.text(&s);
            w.end();
            w.start("textAttrs");
            let sp = spacings.get(r).copied().unwrap_or(0);
            let lm = margins.get(r).copied().unwrap_or(0);
            w.start_with(
                "DOMTextAttrs",
                &[
                    ("aliasText", "false"),
                    ("rotation", "true"),
                    ("size", &java_double(twip(height as f64))),
                    ("bitmapSize", &height.to_string()),
                    ("letterSpacing", &double_str(twip(sp as f64))),
                    ("indent", &double_str(0.0)),
                    ("leftMargin", &double_str(twip(lm as f64))),
                    ("lineSpacing", &double_str(twip(line_spacing as f64))),
                    ("rightMargin", &double_str(0.0)),
                ],
            );
            if let Some(c) = color {
                let c = Rgba { r: c[0], g: c[1], b: c[2], a: c[3] };
                w.attr("fillColor", &hex(&c));
                if text.rgba {
                    w.attr_f("alpha", alpha(&c));
                }
            }
            w.attr("face", &face);
            w.end();
            w.end();
            w.end();
        }
        w.end();
    }

    /// `TextTag.getTextRecordsAttributes`: left margins, letter spacings and line spacing.
    fn text_records_attributes(&self, records: &[crate::text::Record]) -> (Vec<i32>, Vec<i32>, i32) {
        let mut font: Option<std::rc::Rc<FontData>> = None;
        let mut text_height = 12;
        let mut line_spacing = 0;
        let (mut ascent, mut descent) = (0.0, 0.0);
        let mut first_line = true;
        let mut top = 0.0;
        let mut margins = Vec::new();
        let mut spacings = Vec::new();
        for rec in records {
            if let Some((fid, h)) = rec.font {
                if let Some(f) = self.norm_font(fid) {
                    font = Some(f);
                }
                text_height = h as i32;
                let Some(f) = &font else { continue };
                if !f.has_layout {
                    let fam = name_with_fallback(f.name_in_tag.as_deref().unwrap_or(""));
                    let (a, d, _) = line_metrics(&fam, f.bold, f.italic, text_height / 20);
                    ascent = a;
                    descent = d;
                } else {
                    let k = text_height as f64 / 1024.0 / f.divider / 20.0;
                    ascent = f.ascent as f64 * k;
                    descent = f.descent as f64 * k;
                }
            }
            let mut current_left = 0;
            if let Some(x) = rec.x {
                current_left = x as i32;
            }
            if let Some(y) = rec.y {
                if !first_line {
                    top += ascent + descent;
                    let topint = (super::num::java_round(top) as f64 * 20.0) as i32;
                    line_spacing = y as i32 - topint;
                    top += line_spacing as f64 / 20.0;
                } else {
                    top = ascent;
                }
            }
            first_line = false;
            margins.push(current_left);
            let Some(f) = &font else { continue };
            let mut spacing = i32::MAX;
            let n = rec.glyphs.len();
            for (e, &(g, adv)) in rec.glyphs.iter().enumerate() {
                if !f.has_layout {
                    spacing = 0;
                    continue;
                }
                let kern = if e + 1 < n { f.kerning_adjustment(g as usize, rec.glyphs[e + 1].0 as usize) } else { 0 };
                let gadv = f.advances.get(g as usize).copied().unwrap_or(0.0);
                let default = super::num::java_round(text_height as f64 * (gadv + kern as f64) / (1024.0 * f.divider)) as i32;
                let new_spacing = adv - default;
                if n == 1 {
                    spacing = 0;
                } else if e == n - 1 {
                } else if new_spacing < spacing {
                    spacing = new_spacing;
                }
            }
            spacings.push(spacing);
        }
        (margins, spacings, line_spacing)
    }

    /// `HTMLTextParser`: text runs of an HTML edit text.
    fn html_text(&self, det: &EditText, ft: Option<&FontData>, html: &str) -> String {
        let mut p = HtmlState::new();
        if det.has_font || det.has_font_class {
            if let Some(f) = ft {
                p.italic = f.italic;
                p.bold = f.bold;
                p.size = det.font_height / 20;
                p.face = Some(f.face(p.bold, p.italic));
                p.face_stack.push(p.face.clone());
                p.size_stack.push(p.size);
            }
        }
        if det.has_layout {
            p.left_margin = det.left_margin;
            p.right_margin = det.right_margin;
            p.indent = det.indent;
            p.line_spacing = det.leading;
            p.alignment = Some(["left", "right", "center", "justify"].get(det.align as usize).copied().unwrap_or("unknown").to_string());
        }
        if det.has_color {
            p.color = Some(hex(&det.color));
            p.color_alpha = det.color.a as i32;
        }
        let fonts: Vec<std::rc::Rc<FontData>> = self.characters.iter().filter_map(|&id| self.font_data(id)).collect();
        let doc = format!("<html>{html}</html>");
        let complete = sax(&doc, &mut |ev| match ev {
            Sax::Start(name, attrs) => p.start(name, attrs, &fonts),
            Sax::End(name) => p.end(name),
            Sax::Text(t) => p.put(t),
        });
        if complete && p.out.is_empty() {
            p.put("");
        }
        p.out.to_string()
    }
}

struct HtmlState {
    out: XmlWriter,
    face: Option<String>,
    color: Option<String>,
    color_alpha: i32,
    auto_kern: bool,
    size: i32,
    indent: i32,
    left_margin: i32,
    right_margin: i32,
    line_spacing: i32,
    letter_spacing: f64,
    alignment: Option<String>,
    bold: bool,
    italic: bool,
    url: Option<String>,
    target: Option<String>,
    spacing_stack: Vec<f64>,
    size_stack: Vec<i32>,
    face_stack: Vec<Option<String>>,
    color_stack: Vec<Option<String>>,
    alpha_stack: Vec<i32>,
    kern_stack: Vec<bool>,
}

impl HtmlState {
    fn new() -> HtmlState {
        HtmlState {
            out: XmlWriter::new(),
            face: Some(String::new()),
            color: Some(String::new()),
            color_alpha: 255,
            auto_kern: false,
            size: -1,
            indent: -1,
            left_margin: -1,
            right_margin: -1,
            line_spacing: -1,
            letter_spacing: 0.0,
            alignment: None,
            bold: false,
            italic: false,
            url: None,
            target: None,
            spacing_stack: Vec::new(),
            size_stack: Vec::new(),
            face_stack: Vec::new(),
            color_stack: Vec::new(),
            alpha_stack: Vec::new(),
            kern_stack: Vec::new(),
        }
    }

    fn start(&mut self, name: &str, attrs: &[(String, String)], fonts: &[std::rc::Rc<FontData>]) {
        let get = |k: &str| attrs.iter().find(|a| a.0 == k).map(|a| a.1.clone());
        match name {
            "a" => {
                if let Some(h) = get("href") {
                    self.url = Some(h);
                }
                if let Some(t) = get("target") {
                    self.target = Some(t);
                }
            }
            "b" => self.bold = true,
            "i" => self.italic = true,
            "p" => {
                if let Some(a) = get("align") {
                    self.alignment = Some(a);
                }
                if !self.out.is_empty() {
                    self.put("\r\n");
                }
            }
            "font" => {
                if let Some(k) = get("kerning") {
                    self.auto_kern = k == "1";
                }
                if let Some(ls) = get("letterSpacing") {
                    if let Ok(v) = ls.trim().parse::<f64>() {
                        self.letter_spacing = v;
                    }
                }
                if let Some(s) = get("size") {
                    if let Some(d) = s.strip_prefix('+') {
                        if let Ok(v) = d.parse::<i32>() {
                            self.size += v;
                        }
                    } else if let Some(d) = s.strip_prefix('-') {
                        if let Ok(v) = d.parse::<i32>() {
                            self.size -= v;
                        }
                    } else if let Ok(v) = s.parse::<i32>() {
                        self.size = v;
                    }
                }
                if let Some(c) = get("color") {
                    let hexd = |s: &str| s.chars().all(|c| c.is_ascii_hexdigit());
                    if c.len() == 9 && c.starts_with('#') && hexd(&c[1..]) {
                        self.color = Some(format!("#{}", &c[3..]));
                    } else if c.len() == 7 && c.starts_with('#') && hexd(&c[1..]) {
                        self.color = Some(c);
                        self.color_alpha = 255;
                    }
                }
                if let Some(f) = get("face") {
                    if let Some(ft) = fonts.iter().find(|ft| ft.name_in_tag.as_deref() == Some(f.as_str())) {
                        let name = ft.name_tag.clone().or_else(|| ft.name_in_tag.clone()).unwrap_or_default();
                        self.face = Some(match installed_family(&name) {
                            Some(fam) => ps_name(&fam, self.bold, self.italic),
                            None => name,
                        });
                    }
                }
                self.color_stack.push(self.color.clone());
                self.alpha_stack.push(self.color_alpha);
                self.face_stack.push(self.face.clone());
                self.size_stack.push(self.size);
                self.spacing_stack.push(self.letter_spacing);
                self.kern_stack.push(self.auto_kern);
            }
            _ => {}
        }
    }

    fn end(&mut self, name: &str) {
        match name {
            "a" => {
                self.url = None;
                self.target = None;
            }
            "b" => self.bold = false,
            "i" => self.italic = false,
            "font" => {
                self.color_stack.pop();
                self.alpha_stack.pop();
                self.face_stack.pop();
                self.kern_stack.pop();
                self.spacing_stack.pop();
                self.size_stack.pop();
                self.color = self.color_stack.last().cloned().flatten();
                self.color_alpha = self.alpha_stack.last().copied().unwrap_or(255);
                self.face = self.face_stack.last().cloned().flatten();
                self.auto_kern = self.kern_stack.last().copied().unwrap_or(false);
                self.letter_spacing = self.spacing_stack.last().copied().unwrap_or(0.0);
                self.size = self.size_stack.last().copied().unwrap_or(10);
            }
            _ => {}
        }
    }

    fn put(&mut self, txt: &str) {
        let w = &mut self.out;
        w.start("DOMTextRun");
        w.value("characters", txt);
        w.start("textAttrs");
        w.start("DOMTextAttrs");
        if let Some(a) = &self.alignment {
            w.attr("alignment", a);
        }
        w.attr_bool("rotation", true);
        if self.indent > -1 {
            w.attr_d("indent", twip(self.indent as f64));
        }
        if self.left_margin > -1 {
            w.attr_d("leftMargin", twip(self.left_margin as f64));
        }
        w.attr("autoKern", if self.auto_kern { "true" } else { "false" });
        if self.letter_spacing != 0.0 {
            w.attr_d("letterSpacing", self.letter_spacing);
        }
        if self.line_spacing > -1 {
            w.attr_d("lineSpacing", twip(self.line_spacing as f64));
        }
        if self.right_margin > -1 {
            w.attr_d("rightMargin", twip(self.right_margin as f64));
        }
        if self.size > -1 {
            w.attr_i("size", self.size as i64);
            w.attr_i("bitmapSize", (self.size as f64 * 20.0) as i64);
        }
        if let Some(f) = &self.face {
            w.attr("face", f);
        }
        if let Some(c) = &self.color {
            if !c.is_empty() {
                w.attr("fillColor", c);
                if self.color_alpha != 255 {
                    w.attr_d("alpha", self.color_alpha as f64 / 255.0);
                }
            }
        }
        if let Some(u) = &self.url {
            w.attr("url", u);
        }
        if let Some(t) = &self.target {
            w.attr("target", t);
        }
        w.end();
        w.end();
        w.end();
    }
}

enum Sax<'s> {
    Start(&'s str, &'s [(String, String)]),
    End(&'s str),
    Text(&'s str),
}

/// A small SAX parser with Xerces' `characters` chunking: text splits at references and before
/// line breaks that follow other text. Returns false where Xerces stops with a fatal error.
fn sax(doc: &str, f: &mut dyn FnMut(Sax)) -> bool {
    let doc = doc.replace("\r\n", "\n").replace('\r', "\n");
    let b: Vec<char> = doc.chars().collect();
    let mut i = 0;
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, f: &mut dyn FnMut(Sax)| {
        if !text.is_empty() {
            f(Sax::Text(text));
            text.clear();
        }
    };
    let is_name = |c: char| c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.');
    let entity = |name: &str| -> Option<char> {
        match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => {
                let n = name.strip_prefix('#')?;
                let v = if let Some(h) = n.strip_prefix('x') { u32::from_str_radix(h, 16).ok()? } else { n.parse().ok()? };
                char::from_u32(v)
            }
        }
    };
    let read_ref = |i: &mut usize| -> Option<char> {
        let start = *i + 1;
        let mut j = start;
        while j < b.len() && b[j] != ';' && j - start < 12 {
            j += 1;
        }
        if j >= b.len() || b[j] != ';' {
            return None;
        }
        let name: String = b[start..j].iter().collect();
        *i = j + 1;
        entity(&name)
    };
    while i < b.len() {
        let c = b[i];
        if c == '<' {
            flush(&mut text, f);
            if b.get(i + 1) == Some(&'/') {
                let mut j = i + 2;
                while j < b.len() && is_name(b[j]) {
                    j += 1;
                }
                let name: String = b[i + 2..j].iter().collect();
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                if b.get(j) != Some(&'>') || stack.last() != Some(&name) {
                    return false;
                }
                stack.pop();
                f(Sax::End(&name));
                i = j + 1;
                continue;
            }
            if b.get(i + 1) == Some(&'!') || b.get(i + 1) == Some(&'?') {
                let s: String = b[i..].iter().collect();
                if let Some(rest) = s.strip_prefix("<!--") {
                    let Some(e) = rest.find("-->") else { return false };
                    i += 4 + rest[..e].chars().count() + 3;
                    continue;
                }
                if let Some(rest) = s.strip_prefix("<![CDATA[") {
                    let Some(e) = rest.find("]]>") else { return false };
                    let content = &rest[..e];
                    if !content.is_empty() {
                        f(Sax::Text(content));
                    }
                    i += 9 + content.chars().count() + 3;
                    continue;
                }
                return false;
            }
            let mut j = i + 1;
            while j < b.len() && is_name(b[j]) {
                j += 1;
            }
            if j == i + 1 {
                return false;
            }
            let name: String = b[i + 1..j].iter().collect();
            let mut attrs: Vec<(String, String)> = Vec::new();
            loop {
                let ws = j;
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                match b.get(j) {
                    Some('>') => {
                        j += 1;
                        stack.push(name.clone());
                        f(Sax::Start(&name, &attrs));
                        break;
                    }
                    Some('/') if b.get(j + 1) == Some(&'>') => {
                        j += 2;
                        f(Sax::Start(&name, &attrs));
                        f(Sax::End(&name));
                        break;
                    }
                    Some(&c) if is_name(c) && j > ws => {
                        let ns = j;
                        while j < b.len() && is_name(b[j]) {
                            j += 1;
                        }
                        let an: String = b[ns..j].iter().collect();
                        while j < b.len() && b[j].is_whitespace() {
                            j += 1;
                        }
                        if b.get(j) != Some(&'=') {
                            return false;
                        }
                        j += 1;
                        while j < b.len() && b[j].is_whitespace() {
                            j += 1;
                        }
                        let Some(&q) = b.get(j) else { return false };
                        if q != '"' && q != '\'' {
                            return false;
                        }
                        j += 1;
                        let mut v = String::new();
                        loop {
                            let Some(&c) = b.get(j) else { return false };
                            if c == q {
                                j += 1;
                                break;
                            }
                            if c == '<' {
                                return false;
                            }
                            if c == '&' {
                                let Some(ch) = read_ref(&mut j) else { return false };
                                v.push(ch);
                                continue;
                            }
                            v.push(if c == '\n' || c == '\t' { ' ' } else { c });
                            j += 1;
                        }
                        if attrs.iter().any(|a| a.0 == an) {
                            return false;
                        }
                        attrs.push((an, v));
                    }
                    _ => return false,
                }
            }
            i = j;
            continue;
        }
        if c == '&' {
            flush(&mut text, f);
            let Some(ch) = read_ref(&mut i) else { return false };
            let mut s = [0u8; 4];
            f(Sax::Text(ch.encode_utf8(&mut s)));
            continue;
        }
        if c == '\n' && !text.is_empty() && !text.ends_with('\n') {
            flush(&mut text, f);
        }
        if c == ']' {
            flush(&mut text, f);
            f(Sax::Text("]"));
            i += 1;
            continue;
        }
        text.push(c);
        i += 1;
    }
    flush(&mut text, f);
    stack.is_empty()
}
