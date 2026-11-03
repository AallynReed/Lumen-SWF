//! Ports of Batik's `css.engine.value` package for SVG: the `Value` kinds, `ValueConstants`/`SVGValueConstants`,
//! the 60 `SVGCSSEngine` value managers and the `font`/`marker` shorthand managers.

use std::rc::Rc;

use super::{CSSEngine, CssContext, StyleMap};
use crate::asc::flex2::batik::css::parser::css_lexical_unit::{self as clu, LexicalUnitRef};
use crate::asc::flex2::batik::css::sac::lexical_unit::*;
use crate::asc::flex2::batik::dom::NodeRef;

pub const CSS_INHERIT: i16 = 0;
pub const CSS_PRIMITIVE_VALUE: i16 = 1;
pub const CSS_VALUE_LIST: i16 = 2;
pub const CSS_CUSTOM: i16 = 3;

pub const CSS_UNKNOWN: i16 = 0;
pub const CSS_NUMBER: i16 = 1;
pub const CSS_PERCENTAGE: i16 = 2;
pub const CSS_EMS: i16 = 3;
pub const CSS_EXS: i16 = 4;
pub const CSS_PX: i16 = 5;
pub const CSS_CM: i16 = 6;
pub const CSS_MM: i16 = 7;
pub const CSS_IN: i16 = 8;
pub const CSS_PT: i16 = 9;
pub const CSS_PC: i16 = 10;
pub const CSS_DEG: i16 = 11;
pub const CSS_RAD: i16 = 12;
pub const CSS_GRAD: i16 = 13;
pub const CSS_STRING: i16 = 19;
pub const CSS_URI: i16 = 20;
pub const CSS_IDENT: i16 = 21;
pub const CSS_RECT: i16 = 24;
pub const CSS_RGBCOLOR: i16 = 25;

pub type V = Rc<Value>;

#[derive(Debug)]
pub enum Value {
    Inherit,
    /// `StringValue` (`CSS_IDENT` or `CSS_STRING`).
    Str { ptype: i16, s: String },
    /// `URIValue`: the text and the resolved URI `getStringValue` gives.
    Uri { css_text: String, value: String },
    Float { unit: i16, v: f32 },
    Rgb(V, V, V),
    Rect(V, V, V, V),
    List { sep: char, items: Vec<V> },
    /// `ICCColor`.
    Icc { profile: String, colors: Vec<f32> },
    /// `ComputedValue`: the cascaded value and what it computed to.
    Computed { cascaded: V, computed: V },
}

impl Value {
    fn inner(&self) -> &Value {
        match self {
            Value::Computed { computed, .. } => computed.inner(),
            v => v,
        }
    }

    pub fn css_value_type(&self) -> i16 {
        match self.inner() {
            Value::Inherit => CSS_INHERIT,
            Value::List { .. } => CSS_VALUE_LIST,
            Value::Icc { .. } => CSS_CUSTOM,
            _ => CSS_PRIMITIVE_VALUE,
        }
    }

    pub fn primitive_type(&self) -> i16 {
        match self.inner() {
            Value::Str { ptype, .. } => *ptype,
            Value::Uri { .. } => CSS_URI,
            Value::Float { unit, .. } => *unit,
            Value::Rgb(..) => CSS_RGBCOLOR,
            Value::Rect(..) => CSS_RECT,
            _ => CSS_UNKNOWN,
        }
    }

    pub fn float_value(&self) -> f32 {
        match self.inner() {
            Value::Float { v, .. } => *v,
            v => panic!("DOMException: getFloatValue on {v:?}"),
        }
    }

    pub fn string_value(&self) -> &str {
        match self.inner() {
            Value::Str { s, .. } => s,
            Value::Uri { value, .. } => value,
            v => panic!("DOMException: getStringValue on {v:?}"),
        }
    }

    pub fn length(&self) -> usize {
        match self.inner() {
            Value::List { items, .. } => items.len(),
            _ => 0,
        }
    }

    pub fn item(&self, i: usize) -> V {
        match self.inner() {
            Value::List { items, .. } => items[i].clone(),
            v => panic!("DOMException: item on {v:?}"),
        }
    }

    pub fn rgb(&self) -> (V, V, V) {
        match self.inner() {
            Value::Rgb(r, g, b) => (r.clone(), g.clone(), b.clone()),
            v => panic!("DOMException: getRed on {v:?}"),
        }
    }

    pub fn rect(&self) -> (V, V, V, V) {
        match self.inner() {
            Value::Rect(t, r, b, l) => (t.clone(), r.clone(), b.clone(), l.clone()),
            v => panic!("DOMException: getTop on {v:?}"),
        }
    }

    pub fn is_ident(&self, s: &str) -> bool {
        matches!(self.inner(), Value::Str { ptype: CSS_IDENT, s: v } if v == s)
    }

    pub fn is_inherit(&self) -> bool {
        matches!(self, Value::Inherit)
    }
}

pub fn ident(s: &str) -> V {
    Rc::new(Value::Str { ptype: CSS_IDENT, s: s.to_string() })
}

pub fn number(v: f32) -> V {
    Rc::new(Value::Float { unit: CSS_NUMBER, v })
}

pub fn float(unit: i16, v: f32) -> V {
    Rc::new(Value::Float { unit, v })
}

fn rgb_number(r: f32, g: f32, b: f32) -> V {
    Rc::new(Value::Rgb(number(r), number(g), number(b)))
}

include!("named_colors.rs");

/// `ShorthandManager.PropertyHandler`.
pub trait PropertyHandler {
    fn property(&mut self, engine: &CSSEngine, name: &str, lu: &LexicalUnitRef, important: bool) -> Result<(), String>;
}

/// A `DOMException` raised by a value manager, by its message.
pub type DomResult<T> = Result<T, String>;

fn message(key: &str, prop: &str, arg: &str) -> String {
    match key {
        "invalid.identifier" => format!("The \"{arg}\" identifier is not a valid value for the \"{prop}\" property. "),
        "invalid.lexical.unit" => {
            let t: i32 = arg.parse().unwrap_or(-1);
            let name = match t {
                13 => "integer",
                14 => "real",
                15 => "em",
                16 => "ex",
                17 => "pixel",
                18 => "inch",
                19 => "centimeter",
                20 => "millimeter",
                21 => "point",
                22 => "pica",
                23 => "percentage",
                24 => "URI",
                25 => "counter function",
                26 => "counters function",
                27 => "RGB color",
                28 => "degree",
                29 => "gradian",
                30 => "radian",
                31 => "millisecond",
                32 => "second",
                33 => "hertz",
                34 => "kilohertz",
                35 => "identifier",
                36 => "string",
                37 => "attr function",
                38 => "rect function",
                39 => "unicode range",
                40 => "identifier",
                41 => "function",
                42 => "dimension",
                _ if t < 13 => "integer",
                _ => "dimension",
            };
            format!("The \"{prop}\" property does not support {name} values.")
        }
        "malformed.rect" => format!("The '{prop}' property value is malformed."),
        "invalid.float.value" => format!("The number '{arg}' represents an invalid value for the '{prop}' property."),
        "invalid.rgb.component.unit" => format!("The \"{prop}\" property does not support RGB values with type #{arg} components. "),
        "malformed.lexical.unit" => format!("A malformed value was assigned to a \"{prop}\" property."),
        _ => key.to_string(),
    }
}

/// How a manager parses and computes its property.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Ident(&'static [&'static str]),
    BaselineShift,
    Clip,
    UriOrNone,
    Color,
    SvgColor,
    SvgPaint,
    ColorProfile,
    Cursor,
    EnableBackground,
    Opacity,
    FontFamily,
    FontSize,
    FontSizeAdjust,
    FontStretch,
    FontWeight,
    GlyphOrientation(bool),
    Kerning,
    Spacing,
    Src,
    StrokeDasharray,
    StrokeLength,
    StrokeMiterlimit,
    TextDecoration,
}

pub struct ValueManager {
    pub name: &'static str,
    pub inherited: bool,
    pub kind: Kind,
    default: fn() -> V,
}

const SYSTEM_COLORS: &[(&str, u32)] = &[
    ("activeborder", 0x000000),
    ("activecaption", 0x000080),
    ("appworkspace", 0x005C5C),
    ("background", 0x005C5C),
    ("buttonface", 0xC0C0C0),
    ("buttonhighlight", 0xE0E0E0),
    ("buttonshadow", 0x000000),
    ("buttontext", 0x000000),
    ("captiontext", 0xFFFFFF),
    ("graytext", 0x808080),
    ("highlight", 0x000080),
    ("highlighttext", 0xFFFFFF),
    ("inactiveborder", 0x000000),
    ("inactivecaption", 0x808080),
    ("inactivecaptiontext", 0xC0C0C0),
    ("infobackground", 0xE0E000),
    ("infotext", 0x000000),
    ("menu", 0xC0C0C0),
    ("menutext", 0x000000),
    ("scrollbar", 0xE0E0E0),
    ("threeddarkshadow", 0x000000),
    ("threedface", 0xC0C0C0),
    ("threedhighlight", 0xFFFFFF),
    ("threedlightshadow", 0xE0E0E0),
    ("threedshadow", 0x808080),
    ("window", 0xFFFFFF),
    ("windowframe", 0x000000),
    ("windowtext", 0x000000),
];

/// `SystemColorSupport.getSystemColor(ident)`, with headless JDK 17's `SystemColor` defaults.
pub fn get_system_color(ident: &str) -> V {
    let v = SYSTEM_COLORS.iter().find(|c| c.0 == ident.to_lowercase()).map_or(0, |c| c.1);
    rgb_number(((v >> 16) & 0xFF) as f32, ((v >> 8) & 0xFF) as f32, (v & 0xFF) as f32)
}

const CSS2_COLORS: &[&str] = &["aqua", "black", "blue", "fuchsia", "gray", "green", "lime", "maroon", "navy", "olive", "purple", "red", "silver", "teal", "white", "yellow"];

fn is_color_ident(s: &str) -> bool {
    NAMED_COLORS.iter().any(|c| c.0 == s) || CSS2_COLORS.contains(&s) || SYSTEM_COLORS.iter().any(|c| c.0 == s)
}

const FONT_SIZE_IDS: &[&str] = &["all", "large", "larger", "medium", "small", "smaller", "x-large", "x-small", "xx-large", "xx-small"];
const FONT_STRETCH_IDS: &[&str] = &["all", "condensed", "expanded", "extra-condensed", "extra-expanded", "narrower", "normal", "semi-condensed", "semi-expanded", "ultra-condensed", "ultra-expanded", "wider"];
const FONT_STYLE_IDS: &[&str] = &["all", "italic", "normal", "oblique"];
const FONT_VARIANT_IDS: &[&str] = &["normal", "small-caps"];
const FONT_WEIGHT_IDS: &[&str] = &["all", "bold", "bolder", "lighter", "normal"];

macro_rules! vm {
    ($name:expr, $inh:expr, $kind:expr, $def:expr) => {
        ValueManager { name: $name, inherited: $inh, kind: $kind, default: || $def }
    };
}

/// `SVGCSSEngine.SVG_VALUE_MANAGERS`, in its order.
pub fn svg_value_managers() -> Vec<ValueManager> {
    use Kind::*;
    vec![
        vm!("alignment-baseline", false, Ident(&["after-edge", "alphabetic", "auto", "baseline", "before-edge", "hanging", "ideographic", "mathematical", "middle", "text-after-edge", "text-before-edge"]), ident("auto")),
        vm!("baseline-shift", false, BaselineShift, ident("baseline")),
        vm!("clip", false, Clip, ident("auto")),
        vm!("clip-path", false, UriOrNone, ident("none")),
        vm!("clip-rule", true, Ident(&["evenodd", "nonzero"]), ident("nonzero")),
        vm!("color", true, Color, rgb_number(0.0, 0.0, 0.0)),
        vm!("color-interpolation", true, Ident(&["auto", "linearrgb", "srgb"]), ident("srgb")),
        vm!("color-interpolation-filters", true, Ident(&["auto", "linearrgb", "srgb"]), ident("linearrgb")),
        vm!("color-profile", true, ColorProfile, ident("auto")),
        vm!("color-rendering", true, Ident(&["auto", "optimizequality", "optimizespeed"]), ident("auto")),
        vm!("cursor", true, Cursor, ident("auto")),
        vm!("direction", true, Ident(&["ltr", "rtl"]), ident("ltr")),
        vm!(
            "display",
            false,
            Ident(&["block", "compact", "inline", "inline-table", "list-item", "marker", "none", "run-in", "table", "table-caption", "table-cell", "table-column", "table-column-group", "table-footer-group", "table-header-group", "table-row", "table-row-group"]),
            ident("inline")
        ),
        vm!(
            "dominant-baseline",
            false,
            Ident(&["auto", "alphabetic", "central", "hanging", "ideographic", "mathematical", "middle", "no-change", "reset-size", "text-after-edge", "text-before-edge", "text-bottom", "text-top", "use-script"]),
            ident("auto")
        ),
        vm!("enable-background", false, EnableBackground, ident("accumulate")),
        vm!("fill", true, SvgPaint, rgb_number(0.0, 0.0, 0.0)),
        vm!("fill-opacity", true, Opacity, number(1.0)),
        vm!("fill-rule", true, Ident(&["evenodd", "nonzero"]), ident("nonzero")),
        vm!("filter", false, UriOrNone, ident("none")),
        vm!("flood-color", false, SvgColor, rgb_number(0.0, 0.0, 0.0)),
        vm!("flood-opacity", false, Opacity, number(1.0)),
        vm!("font-family", true, FontFamily, Rc::new(Value::List { sep: ',', items: vec![Rc::new(Value::Str { ptype: CSS_STRING, s: "Arial".into() })] })),
        vm!("font-size", true, FontSize, ident("medium")),
        vm!("font-size-adjust", true, FontSizeAdjust, ident("none")),
        vm!("font-stretch", true, FontStretch, ident("normal")),
        vm!("font-style", true, Ident(FONT_STYLE_IDS), ident("normal")),
        vm!("font-variant", true, Ident(FONT_VARIANT_IDS), ident("normal")),
        vm!("font-weight", true, FontWeight, ident("normal")),
        vm!("glyph-orientation-horizontal", true, GlyphOrientation(false), float(CSS_DEG, 0.0)),
        vm!("glyph-orientation-vertical", true, GlyphOrientation(true), ident("auto")),
        vm!("image-rendering", true, Ident(&["auto", "optimizequality", "optimizespeed"]), ident("auto")),
        vm!("kerning", true, Kerning, ident("auto")),
        vm!("letter-spacing", true, Spacing, ident("normal")),
        vm!("lighting-color", false, SvgColor, rgb_number(255.0, 255.0, 255.0)),
        vm!("marker-end", true, UriOrNone, ident("none")),
        vm!("marker-mid", true, UriOrNone, ident("none")),
        vm!("marker-start", true, UriOrNone, ident("none")),
        vm!("mask", false, UriOrNone, ident("none")),
        vm!("opacity", false, Opacity, number(1.0)),
        vm!("overflow", false, Ident(&["auto", "hidden", "scroll", "visible"]), ident("visible")),
        vm!("pointer-events", true, Ident(&["all", "fill", "fillstroke", "none", "painted", "stroke", "visible", "visiblefill", "visiblefillstroke", "visiblepainted", "visiblestroke"]), ident("visiblepainted")),
        vm!("src", false, Src, ident("none")),
        vm!("shape-rendering", true, Ident(&["auto", "crispedges", "geometricprecision", "optimizespeed"]), ident("auto")),
        vm!("stop-color", false, SvgColor, rgb_number(0.0, 0.0, 0.0)),
        vm!("stop-opacity", false, Opacity, number(1.0)),
        vm!("stroke", true, SvgPaint, ident("none")),
        vm!("stroke-dasharray", true, StrokeDasharray, ident("none")),
        vm!("stroke-dashoffset", true, StrokeLength, number(0.0)),
        vm!("stroke-linecap", true, Ident(&["butt", "round", "square"]), ident("butt")),
        vm!("stroke-linejoin", true, Ident(&["miter", "round", "bevel"]), ident("miter")),
        vm!("stroke-miterlimit", true, StrokeMiterlimit, number(4.0)),
        vm!("stroke-opacity", true, Opacity, number(1.0)),
        vm!("stroke-width", true, StrokeLength, number(1.0)),
        vm!("text-anchor", true, Ident(&["start", "middle", "end"]), ident("start")),
        vm!("text-decoration", false, TextDecoration, ident("none")),
        vm!("text-rendering", true, Ident(&["auto", "optimizespeed", "geometricprecision", "optimizelegibility"]), ident("auto")),
        vm!("unicode-bidi", false, Ident(&["bidi-override", "embed", "normal"]), ident("normal")),
        vm!("visibility", true, Ident(&["visible", "hidden", "collapse"]), ident("visible")),
        vm!("word-spacing", true, Spacing, ident("normal")),
        vm!("writing-mode", true, Ident(&["lr", "lr-tb", "rl", "rl-tb", "tb", "tb-rl"]), ident("lr-tb")),
    ]
}

pub const SHORTHANDS: &[&str] = &["font", "marker"];

fn ty(lu: &LexicalUnitRef) -> i16 {
    lu.get_lexical_unit_type()
}

fn sval(lu: &LexicalUnitRef) -> String {
    lu.get_string_value().unwrap_or_default()
}

fn fval(lu: &LexicalUnitRef) -> f32 {
    lu.get_float_value().unwrap_or(0.0)
}

fn ival(lu: &LexicalUnitRef) -> i32 {
    lu.get_integer_value().unwrap_or(0)
}

/// `String.toLowerCase()`.
fn lower(s: &str) -> String {
    s.to_lowercase()
}

impl ValueManager {
    pub fn get_default_value(&self) -> V {
        (self.default)()
    }

    fn invalid_ident(&self, s: &str) -> String {
        message("invalid.identifier", self.name, s)
    }

    fn invalid_lu(&self, t: i16) -> String {
        message("invalid.lexical.unit", self.name, &t.to_string())
    }

    /// `LengthManager.createValue(lu, engine)`.
    fn create_length(&self, lu: &LexicalUnitRef) -> DomResult<V> {
        let u = match ty(lu) {
            SAC_EM => CSS_EMS,
            SAC_EX => CSS_EXS,
            SAC_PIXEL => CSS_PX,
            SAC_CENTIMETER => CSS_CM,
            SAC_MILLIMETER => CSS_MM,
            SAC_INCH => CSS_IN,
            SAC_POINT => CSS_PT,
            SAC_PICA => CSS_PC,
            SAC_INTEGER => return Ok(number(ival(lu) as f32)),
            SAC_REAL => CSS_NUMBER,
            SAC_PERCENTAGE => CSS_PERCENTAGE,
            t => return Err(self.invalid_lu(t)),
        };
        Ok(float(u, fval(lu)))
    }

    fn create_ident(&self, ids: &[&str], lu: &LexicalUnitRef) -> DomResult<V> {
        match ty(lu) {
            SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
            SAC_IDENT => {
                let s = lower(&sval(lu));
                if ids.contains(&s.as_str()) {
                    Ok(ident(&s))
                } else {
                    Err(self.invalid_ident(&sval(lu)))
                }
            }
            t => Err(self.invalid_lu(t)),
        }
    }

    fn create_uri(&self, engine: &CSSEngine, lu: &LexicalUnitRef) -> V {
        let s = sval(lu);
        let uri = crate::asc::flex2::batik::dom::resolve_url(engine.get_css_base_uri().as_deref(), &s);
        Rc::new(Value::Uri { css_text: s, value: uri })
    }

    /// `AbstractColorManager.createValue`, after `SVGColorManager`'s `currentcolor`.
    fn create_color(&self, lu: &LexicalUnitRef) -> DomResult<V> {
        if ty(lu) == SAC_RGBCOLOR {
            let comp = |lu: &LexicalUnitRef| -> DomResult<V> {
                match ty(lu) {
                    SAC_INTEGER => Ok(number(ival(lu) as f32)),
                    SAC_REAL => Ok(number(fval(lu))),
                    SAC_PERCENTAGE => Ok(float(CSS_PERCENTAGE, fval(lu))),
                    t => Err(message("invalid.rgb.component.unit", self.name, &t.to_string())),
                }
            };
            let p = lu.get_parameters().ok().flatten().ok_or_else(|| self.invalid_lu(SAC_RGBCOLOR))?;
            let red = comp(&p)?;
            let p = p.get_next_lexical_unit().and_then(|n| n.get_next_lexical_unit()).ok_or_else(|| "NullPointerException".to_string())?;
            let green = comp(&p)?;
            let p = p.get_next_lexical_unit().and_then(|n| n.get_next_lexical_unit()).ok_or_else(|| "NullPointerException".to_string())?;
            let blue = comp(&p)?;
            return Ok(Rc::new(Value::Rgb(red, green, blue)));
        }
        match ty(lu) {
            SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
            SAC_IDENT => {
                let s = lower(&sval(lu));
                if is_color_ident(&s) {
                    Ok(ident(&s))
                } else {
                    Err(self.invalid_ident(&sval(lu)))
                }
            }
            t => Err(self.invalid_lu(t)),
        }
    }

    /// `SVGColorManager.createValue`.
    fn create_svg_color(&self, lu: &LexicalUnitRef) -> DomResult<V> {
        if ty(lu) == SAC_IDENT && sval(lu).eq_ignore_ascii_case("currentcolor") {
            return Ok(ident("currentcolor"));
        }
        let v = self.create_color(lu)?;
        let Some(next) = lu.get_next_lexical_unit() else { return Ok(v) };
        if ty(&next) != SAC_FUNCTION || !next.get_function_name().unwrap_or_default().eq_ignore_ascii_case("icc-color") {
            return Err(self.invalid_lu(ty(&next)));
        }
        let p = next.get_parameters().ok().flatten().ok_or_else(|| self.invalid_lu(-1))?;
        if ty(&p) != SAC_IDENT {
            return Err(self.invalid_lu(ty(&p)));
        }
        let mut colors = Vec::new();
        let mut l = p.get_next_lexical_unit();
        while let Some(c) = l {
            if ty(&c) != SAC_OPERATOR_COMMA {
                return Err(self.invalid_lu(ty(&c)));
            }
            let Some(n) = c.get_next_lexical_unit() else { return Err(self.invalid_lu(-1)) };
            colors.push(match ty(&n) {
                SAC_INTEGER => ival(&n) as f32,
                SAC_REAL => fval(&n),
                t => return Err(self.invalid_lu(t)),
            });
            l = n.get_next_lexical_unit();
        }
        Ok(Rc::new(Value::List { sep: ' ', items: vec![v, Rc::new(Value::Icc { profile: sval(&p), colors })] }))
    }

    pub fn create_value(&self, lu: &LexicalUnitRef, engine: &CSSEngine) -> DomResult<V> {
        use Kind::*;
        match self.kind {
            Ident(ids) => self.create_ident(ids, lu),
            BaselineShift => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT => {
                    let s = lower(&sval(lu));
                    if ["baseline", "sub", "super"].contains(&s.as_str()) {
                        Ok(ident(&s))
                    } else {
                        Err(self.invalid_ident(&sval(lu)))
                    }
                }
                _ => self.create_length(lu),
            },
            Clip => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT if sval(lu).eq_ignore_ascii_case("auto") => Ok(ident("auto")),
                _ => self.create_rect(lu),
            },
            UriOrNone => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_URI => Ok(self.create_uri(engine, lu)),
                SAC_IDENT if sval(lu).eq_ignore_ascii_case("none") => Ok(ident("none")),
                t => Err(self.invalid_lu(t)),
            },
            Color => self.create_color(lu),
            SvgColor => self.create_svg_color(lu),
            SvgPaint => {
                if ty(lu) == SAC_IDENT && sval(lu).eq_ignore_ascii_case("none") {
                    return Ok(ident("none"));
                }
                if ty(lu) != SAC_URI {
                    return self.create_svg_color(lu);
                }
                let u = self.create_uri(engine, lu);
                let Some(next) = lu.get_next_lexical_unit() else { return Ok(u) };
                let mut items = vec![u];
                if ty(&next) == SAC_IDENT && sval(&next).eq_ignore_ascii_case("none") {
                    items.push(ident("none"));
                    return Ok(Rc::new(Value::List { sep: ' ', items }));
                }
                let v = self.create_svg_color(&next)?;
                if v.css_value_type() == CSS_CUSTOM || v.css_value_type() == CSS_VALUE_LIST {
                    for i in 0..v.length() {
                        items.push(v.item(i));
                    }
                } else {
                    items.push(v);
                }
                Ok(Rc::new(Value::List { sep: ' ', items }))
            }
            ColorProfile => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT => {
                    let s = lower(&sval(lu));
                    Ok(ident(&s))
                }
                SAC_URI => Ok(self.create_uri(engine, lu)),
                t => Err(self.invalid_lu(t)),
            },
            Cursor => {
                let mut items = Vec::new();
                let mut l = Some(lu.clone());
                if ty(lu) == SAC_INHERIT {
                    return Ok(Rc::new(Value::Inherit));
                }
                while let Some(c) = l.clone() {
                    match ty(&c) {
                        SAC_URI => {
                            items.push(self.create_uri(engine, &c));
                            let n = c.get_next_lexical_unit().ok_or_else(|| message("malformed.lexical.unit", self.name, ""))?;
                            if ty(&n) != SAC_OPERATOR_COMMA {
                                return Err(self.invalid_lu(ty(&n)));
                            }
                            l = n.get_next_lexical_unit();
                            if l.is_none() {
                                return Err(message("malformed.lexical.unit", self.name, ""));
                            }
                        }
                        SAC_IDENT => {
                            let s = lower(&sval(&c));
                            let ids = ["auto", "crosshair", "default", "e-resize", "help", "move", "n-resize", "ne-resize", "nw-resize", "pointer", "s-resize", "se-resize", "sw-resize", "text", "w-resize", "wait"];
                            if !ids.contains(&s.as_str()) {
                                return Err(self.invalid_ident(&sval(&c)));
                            }
                            items.push(ident(&s));
                            l = c.get_next_lexical_unit();
                            if l.is_some() {
                                return Err(self.invalid_lu(ty(l.as_ref().unwrap())));
                            }
                        }
                        t => return Err(self.invalid_lu(t)),
                    }
                }
                Ok(Rc::new(Value::List { sep: ',', items }))
            }
            EnableBackground => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT => {
                    let id = lower(&sval(lu));
                    if id == "accumulate" {
                        return Ok(ident("accumulate"));
                    }
                    if id != "new" {
                        return Err(self.invalid_ident(&id));
                    }
                    let mut items = vec![ident("new")];
                    let mut l = lu.get_next_lexical_unit();
                    if l.is_none() {
                        return Ok(Rc::new(Value::List { sep: ' ', items }));
                    }
                    for i in 0..4 {
                        let c = l.clone().ok_or_else(|| message("malformed.lexical.unit", self.name, ""))?;
                        items.push(self.create_length(&c)?);
                        if i < 3 {
                            l = c.get_next_lexical_unit();
                        }
                    }
                    Ok(Rc::new(Value::List { sep: ' ', items }))
                }
                t => Err(self.invalid_lu(t)),
            },
            Opacity | StrokeMiterlimit => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_INTEGER => Ok(number(ival(lu) as f32)),
                SAC_REAL => Ok(number(fval(lu))),
                t => Err(self.invalid_lu(t)),
            },
            FontFamily => {
                match ty(lu) {
                    SAC_INHERIT => return Ok(Rc::new(Value::Inherit)),
                    SAC_IDENT | SAC_STRING_VALUE => {}
                    t => return Err(self.invalid_lu(t)),
                }
                let mut items = Vec::new();
                let mut l = Some(lu.clone());
                loop {
                    let c = l.clone().unwrap();
                    match ty(&c) {
                        SAC_STRING_VALUE => {
                            items.push(Rc::new(Value::Str { ptype: CSS_STRING, s: sval(&c) }));
                            l = c.get_next_lexical_unit();
                        }
                        SAC_IDENT => {
                            let mut sb = sval(&c);
                            l = c.get_next_lexical_unit();
                            if l.as_ref().is_some_and(|n| ty(n) == SAC_IDENT) {
                                while let Some(n) = l.clone().filter(|n| ty(n) == SAC_IDENT) {
                                    sb.push(' ');
                                    sb.push_str(&sval(&n));
                                    l = n.get_next_lexical_unit();
                                }
                                items.push(Rc::new(Value::Str { ptype: CSS_STRING, s: sb }));
                            } else {
                                let s = lower(&sb);
                                if ["cursive", "fantasy", "monospace", "serif", "sans-serif"].contains(&s.as_str()) {
                                    items.push(ident(&s));
                                } else {
                                    items.push(Rc::new(Value::Str { ptype: CSS_STRING, s: sb }));
                                }
                            }
                        }
                        _ => {}
                    }
                    let Some(c) = l.clone() else { return Ok(Rc::new(Value::List { sep: ',', items })) };
                    if ty(&c) != SAC_OPERATOR_COMMA {
                        return Err(self.invalid_lu(ty(&c)));
                    }
                    l = c.get_next_lexical_unit();
                    if l.is_none() {
                        return Err(message("malformed.lexical.unit", self.name, ""));
                    }
                }
            }
            FontSize => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT => {
                    let s = lower(&sval(lu));
                    if FONT_SIZE_IDS.contains(&s.as_str()) {
                        Ok(ident(&s))
                    } else {
                        Err(self.invalid_ident(&s))
                    }
                }
                _ => self.create_length(lu),
            },
            FontSizeAdjust => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_INTEGER => Ok(number(ival(lu) as f32)),
                SAC_REAL => Ok(number(fval(lu))),
                SAC_IDENT if sval(lu).eq_ignore_ascii_case("none") => Ok(ident("none")),
                SAC_IDENT => Err(self.invalid_ident(&sval(lu))),
                t => Err(self.invalid_lu(t)),
            },
            FontStretch => self.create_ident(FONT_STRETCH_IDS, lu),
            FontWeight => {
                if ty(lu) == SAC_INTEGER {
                    let i = ival(lu);
                    return match i {
                        100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900 => Ok(number(i as f32)),
                        _ => Err(message("invalid.float.value", self.name, &format!("{}.0", i))),
                    };
                }
                self.create_ident(FONT_WEIGHT_IDS, lu)
            }
            GlyphOrientation(vertical) => {
                if vertical && ty(lu) == SAC_IDENT {
                    if sval(lu).eq_ignore_ascii_case("auto") {
                        return Ok(ident("auto"));
                    }
                    return Err(self.invalid_ident(&sval(lu)));
                }
                match ty(lu) {
                    SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                    SAC_DEGREE => Ok(float(CSS_DEG, fval(lu))),
                    SAC_GRADIAN => Ok(float(CSS_GRAD, fval(lu))),
                    SAC_RADIAN => Ok(float(CSS_RAD, fval(lu))),
                    SAC_INTEGER => Ok(float(CSS_DEG, ival(lu) as f32)),
                    SAC_REAL => Ok(float(CSS_DEG, fval(lu))),
                    t => Err(self.invalid_lu(t)),
                }
            }
            Kerning | Spacing => {
                let kw = if matches!(self.kind, Kerning) { "auto" } else { "normal" };
                match ty(lu) {
                    SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                    SAC_IDENT if sval(lu).eq_ignore_ascii_case(kw) => Ok(ident(kw)),
                    SAC_IDENT => Err(self.invalid_ident(&sval(lu))),
                    _ => self.create_length(lu),
                }
            }
            Src => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT if sval(lu).eq_ignore_ascii_case("none") => Ok(ident("none")),
                SAC_URI | SAC_STRING_VALUE | SAC_IDENT | SAC_FUNCTION => Ok(Rc::new(Value::List { sep: ',', items: vec![self.create_uri(engine, lu)] })),
                t => Err(self.invalid_lu(t)),
            },
            StrokeDasharray => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT if sval(lu).eq_ignore_ascii_case("none") => Ok(ident("none")),
                SAC_IDENT => Err(self.invalid_ident(&sval(lu))),
                _ => {
                    let mut items = Vec::new();
                    let mut l = Some(lu.clone());
                    while let Some(c) = l {
                        items.push(self.create_length(&c)?);
                        l = c.get_next_lexical_unit();
                        if let Some(n) = l.clone() {
                            if ty(&n) == SAC_OPERATOR_COMMA {
                                l = n.get_next_lexical_unit();
                            }
                        }
                    }
                    Ok(Rc::new(Value::List { sep: ' ', items }))
                }
            },
            StrokeLength => {
                if ty(lu) == SAC_INHERIT {
                    return Ok(Rc::new(Value::Inherit));
                }
                self.create_length(lu)
            }
            TextDecoration => match ty(lu) {
                SAC_INHERIT => Ok(Rc::new(Value::Inherit)),
                SAC_IDENT => {
                    if sval(lu).eq_ignore_ascii_case("none") {
                        return Ok(ident("none"));
                    }
                    let mut items = Vec::new();
                    let mut l = Some(lu.clone());
                    while let Some(c) = l {
                        if ty(&c) != SAC_IDENT {
                            return Err(self.invalid_lu(ty(&c)));
                        }
                        let s = lower(&sval(&c));
                        if !["blink", "line-through", "overline", "underline"].contains(&s.as_str()) {
                            return Err(self.invalid_ident(&sval(&c)));
                        }
                        items.push(ident(&s));
                        l = c.get_next_lexical_unit();
                    }
                    Ok(Rc::new(Value::List { sep: ' ', items }))
                }
                t => Err(self.invalid_lu(t)),
            },
        }
    }

    /// `RectManager.createValue`.
    fn create_rect(&self, lu: &LexicalUnitRef) -> DomResult<V> {
        let malformed = || message("malformed.rect", self.name, "");
        let is_rect = match ty(lu) {
            SAC_FUNCTION => lu.get_function_name().unwrap_or_default().eq_ignore_ascii_case("rect"),
            SAC_RECT_FUNCTION => true,
            _ => false,
        };
        if !is_rect {
            return Err(malformed());
        }
        let comp = |lu: &LexicalUnitRef| -> DomResult<V> {
            match ty(lu) {
                SAC_IDENT if sval(lu).eq_ignore_ascii_case("auto") => Ok(ident("auto")),
                SAC_IDENT => Err(malformed()),
                _ => self.create_length(lu).map_err(|_| malformed()),
            }
        };
        let mut l = lu.get_parameters().ok().flatten().ok_or_else(malformed)?;
        let mut parts = Vec::new();
        for i in 0..4 {
            parts.push(comp(&l)?);
            if i < 3 {
                let n = l.get_next_lexical_unit().filter(|n| ty(n) == SAC_OPERATOR_COMMA).ok_or_else(malformed)?;
                l = n.get_next_lexical_unit().ok_or_else(malformed)?;
            }
        }
        Ok(Rc::new(Value::Rect(parts[0].clone(), parts[1].clone(), parts[2].clone(), parts[3].clone())))
    }

    /// `LengthManager.computeValue` with the given orientation (0 horizontal, 1 vertical, 2 both).
    fn compute_length(&self, elt: &NodeRef, engine: &CSSEngine, ctx: &dyn CssContext, value: &V, orientation: i32) -> V {
        if value.css_value_type() != CSS_PRIMITIVE_VALUE {
            return value.clone();
        }
        let pmm = ctx.get_pixel_unit_to_millimeter();
        match value.primitive_type() {
            CSS_NUMBER | CSS_PX => value.clone(),
            CSS_MM => number(value.float_value() / pmm),
            CSS_CM => number(value.float_value() * 10.0f32 / pmm),
            CSS_IN => number(value.float_value() * 25.4f32 / pmm),
            CSS_PT => number(value.float_value() * 25.4f32 / (72.0f32 * pmm)),
            CSS_PC => number(value.float_value() * 25.4f32 / (6.0f32 * pmm)),
            CSS_EMS => {
                let v = value.float_value();
                let fs = engine.get_computed_style(elt, engine.font_size_index, ctx).float_value();
                number(v * fs)
            }
            CSS_EXS => {
                let v = value.float_value();
                let fs = engine.get_computed_style(elt, engine.font_size_index, ctx).float_value();
                number(v * fs * 0.5f32)
            }
            CSS_PERCENTAGE => {
                let fs = match orientation {
                    0 => value.float_value() * ctx.get_block_width(elt) / 100.0f32,
                    1 => value.float_value() * ctx.get_block_height(elt) / 100.0f32,
                    _ => {
                        let w = ctx.get_block_width(elt) as f64;
                        let h = ctx.get_block_height(elt) as f64;
                        (value.float_value() as f64 * ((w * w + h * h).sqrt() / 2f64.sqrt()) / 100.0) as f32
                    }
                };
                number(fs)
            }
            _ => value.clone(),
        }
    }

    fn compute_color(&self, value: &V) -> V {
        if value.primitive_type() == CSS_IDENT {
            let id = value.string_value();
            if let Some(c) = NAMED_COLORS.iter().find(|c| c.0 == id) {
                return rgb_number(c.1 as f32, c.2 as f32, c.3 as f32);
            }
            return get_system_color(id);
        }
        value.clone()
    }

    fn compute_svg_color(&self, elt: &NodeRef, engine: &CSSEngine, ctx: &dyn CssContext, value: &V) -> V {
        if value.is_ident("currentcolor") {
            return engine.get_computed_style(elt, engine.color_index, ctx);
        }
        if value.css_value_type() == CSS_VALUE_LIST {
            let v = value.item(0);
            let t = self.compute_color(&v);
            if !Rc::ptr_eq(&t, &v) {
                return Rc::new(Value::List { sep: ' ', items: vec![t, value.item(1)] });
            }
            return value.clone();
        }
        self.compute_color(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn compute_value(&self, elt: &NodeRef, engine: &CSSEngine, idx: usize, sm: &mut StyleMap, value: &V, ctx: &dyn CssContext) -> V {
        use Kind::*;
        let _ = (idx, &sm);
        let uri_copy = |value: &V| -> V {
            if value.css_value_type() == CSS_PRIMITIVE_VALUE && value.primitive_type() == CSS_URI {
                let s = value.string_value().to_string();
                return Rc::new(Value::Uri { css_text: s.clone(), value: s });
            }
            value.clone()
        };
        match self.kind {
            Ident(_) | UriOrNone | ColorProfile | Cursor | Opacity | StrokeMiterlimit | FontSizeAdjust | GlyphOrientation(_) | Src | TextDecoration => uri_copy(value),
            BaselineShift => {
                if value.primitive_type() == CSS_PERCENTAGE {
                    let parent = elt.get_parent_node().filter(|p| p.is_element()).unwrap_or_else(|| elt.clone());
                    let fs = engine.get_computed_style(&parent, engine.line_height_index, ctx);
                    return number((fs.float_value() * value.float_value()) / 100.0f32);
                }
                self.compute_length(elt, engine, ctx, value, 2)
            }
            Clip => {
                if value.css_value_type() != CSS_PRIMITIVE_VALUE || value.primitive_type() != CSS_RECT {
                    return value.clone();
                }
                let (t, r, b, l) = value.rect();
                let top = self.compute_length(elt, engine, ctx, &t, 1);
                let bottom = self.compute_length(elt, engine, ctx, &b, 1);
                let left = self.compute_length(elt, engine, ctx, &l, 0);
                let right = self.compute_length(elt, engine, ctx, &r, 0);
                if !Rc::ptr_eq(&top, &t) || !Rc::ptr_eq(&right, &r) || !Rc::ptr_eq(&bottom, &b) || !Rc::ptr_eq(&left, &l) {
                    Rc::new(Value::Rect(top, right, bottom, left))
                } else {
                    value.clone()
                }
            }
            Color => self.compute_color(value),
            SvgColor => self.compute_svg_color(elt, engine, ctx, value),
            SvgPaint => {
                if value.is_ident("none") {
                    return value.clone();
                }
                if value.css_value_type() == CSS_VALUE_LIST {
                    let v = value.item(0);
                    if v.primitive_type() == CSS_URI {
                        let v1 = value.item(1);
                        if v1.is_ident("none") {
                            return value.clone();
                        }
                        let t = self.compute_svg_color(elt, engine, ctx, &v1);
                        if !Rc::ptr_eq(&t, &v1) {
                            let mut items = vec![value.item(0), t];
                            if value.length() == 3 {
                                items.push(value.item(1));
                            }
                            return Rc::new(Value::List { sep: ' ', items });
                        }
                        return value.clone();
                    }
                }
                self.compute_svg_color(elt, engine, ctx, value)
            }
            EnableBackground => {
                if value.css_value_type() == CSS_VALUE_LIST && value.length() == 5 {
                    let o = [0, 1, 0, 1];
                    let mut items = vec![value.item(0)];
                    let mut changed = false;
                    for i in 1..5 {
                        let lv = value.item(i);
                        let v = self.compute_length(elt, engine, ctx, &lv, o[i - 1]);
                        changed |= !Rc::ptr_eq(&lv, &v);
                        items.push(v);
                    }
                    if changed {
                        return Rc::new(Value::List { sep: ' ', items });
                    }
                }
                value.clone()
            }
            FontFamily => value.clone(),
            FontSize => self.compute_font_size(elt, engine, idx, ctx, value),
            FontStretch => {
                if value.is_ident("narrower") || value.is_ident("wider") {
                    const ORDER: [&str; 9] = ["ultra-condensed", "extra-condensed", "condensed", "semi-condensed", "normal", "semi-expanded", "expanded", "extra-expanded", "ultra-expanded"];
                    let p = super::get_parent_css_stylable_element(elt);
                    let base = match p {
                        None => "normal".to_string(),
                        Some(p) => engine.get_computed_style(&p, idx, ctx).string_value().to_string(),
                    };
                    let i = ORDER.iter().position(|s| *s == base).unwrap_or(4);
                    let j = if value.is_ident("narrower") { i.saturating_sub(1) } else { (i + 1).min(8) };
                    return ident(ORDER[j]);
                }
                value.clone()
            }
            FontWeight => {
                if value.is_ident("bolder") || value.is_ident("lighter") {
                    let p = super::get_parent_css_stylable_element(elt);
                    let fw = match p {
                        None => 400.0,
                        Some(p) => engine.get_computed_style(&p, idx, ctx).float_value(),
                    };
                    let f = if value.is_ident("bolder") { ctx.get_bolder_font_weight(fw) } else { ctx.get_lighter_font_weight(fw) };
                    let w = match f as i32 {
                        100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 => f as i32,
                        _ => 900,
                    };
                    return number(w as f32);
                }
                if value.is_ident("normal") {
                    return number(400.0);
                }
                if value.is_ident("bold") {
                    return number(700.0);
                }
                value.clone()
            }
            Kerning => self.compute_length(elt, engine, ctx, value, 0),
            Spacing | StrokeLength => self.compute_length(elt, engine, ctx, value, 2),
            StrokeDasharray => {
                if value.css_value_type() == CSS_PRIMITIVE_VALUE {
                    return value.clone();
                }
                let items = (0..value.length()).map(|i| self.compute_length(elt, engine, ctx, &value.item(i), 2)).collect();
                Rc::new(Value::List { sep: ' ', items })
            }
        }
    }

    fn compute_font_size(&self, elt: &NodeRef, engine: &CSSEngine, idx: usize, ctx: &dyn CssContext, value: &V) -> V {
        let pmm = ctx.get_pixel_unit_to_millimeter();
        let mut scale = 1.0f32;
        let mut parent_relative = false;
        match value.primitive_type() {
            CSS_NUMBER | CSS_PX => return value.clone(),
            CSS_MM => return number(value.float_value() / pmm),
            CSS_CM => return number(value.float_value() * 10.0f32 / pmm),
            CSS_IN => return number(value.float_value() * 25.4f32 / pmm),
            CSS_PT => return number(value.float_value() * 25.4f32 / (72.0f32 * pmm)),
            CSS_PC => return number(value.float_value() * 25.4f32 / (6.0f32 * pmm)),
            CSS_EMS => {
                parent_relative = true;
                scale = value.float_value();
            }
            CSS_EXS => {
                parent_relative = true;
                scale = value.float_value() * 0.5f32;
            }
            CSS_PERCENTAGE => {
                parent_relative = true;
                scale = value.float_value() * 0.01f32;
            }
            _ => {}
        }
        if value.is_ident("larger") {
            parent_relative = true;
            scale = 1.2f32;
        } else if value.is_ident("smaller") {
            parent_relative = true;
            scale = 1.0f32 / 1.2f32;
        }
        if parent_relative {
            let fs = match super::get_parent_css_stylable_element(elt) {
                None => ctx.get_medium_font_size(),
                Some(p) => engine.get_computed_style(&p, idx, ctx).float_value(),
            };
            return number(fs * scale);
        }
        let mut fs = ctx.get_medium_font_size();
        let s = value.string_value().as_bytes().to_vec();
        let f = fs as f64;
        match s[0] {
            b'm' => {}
            b's' => fs = (f / 1.2) as f32,
            b'l' => fs = (f * 1.2) as f32,
            _ => {
                if s[1] == b'x' {
                    fs = if s[3] == b's' { (((f / 1.2) / 1.2) / 1.2) as f32 } else { (f * 1.2 * 1.2 * 1.2) as f32 };
                } else {
                    fs = if s[2] == b's' { ((f / 1.2) / 1.2) as f32 } else { (f * 1.2 * 1.2) as f32 };
                }
            }
        }
        number(fs)
    }
}

/// `MarkerShorthandManager.setValues` and `FontShorthandManager.setValues`.
pub fn set_shorthand_values(engine: &CSSEngine, ph: &mut dyn PropertyHandler, name: &str, lu: &LexicalUnitRef, imp: bool) -> Result<(), String> {
    if name == "marker" {
        ph.property(engine, "marker-end", lu, imp)?;
        ph.property(engine, "marker-mid", lu, imp)?;
        return ph.property(engine, "marker-start", lu, imp);
    }
    let invalid = |t: i16| message("invalid.lexical.unit", "font", &t.to_string());
    let malformed = || message("malformed.lexical.unit", "font", "");
    match ty(lu) {
        SAC_INHERIT => return Ok(()),
        SAC_IDENT => {
            let s = lower(&sval(lu));
            if ["caption", "icon", "menu", "message-box", "small-caption", "status-bar"].contains(&s.as_str()) {
                let fam = clu::create_string(SAC_STRING_VALUE, "Dialog".into(), None, 0);
                let normal = clu::create_string(SAC_IDENT, "normal".into(), None, 0);
                let size = clu::create_float(SAC_POINT, 12.0, None, 0);
                ph.property(engine, "font-family", &fam, imp)?;
                ph.property(engine, "font-style", &normal, imp)?;
                ph.property(engine, "font-weight", &normal, imp)?;
                ph.property(engine, "font-size", &size, imp)?;
                return Ok(());
            }
        }
        _ => {}
    }
    let normal = clu::create_string(SAC_IDENT, "normal".into(), None, 0);
    let (mut font_style, mut font_variant, mut font_weight, mut font_size, mut line_height) = (None, None, None, None, None);
    let mut int_lu: Option<LexicalUnitRef> = None;
    let mut l = Some(lu.clone());
    let mut done = false;
    while !done {
        let Some(c) = l.clone() else { break };
        match ty(&c) {
            SAC_IDENT => {
                let s = lower(&sval(&c));
                if font_style.is_none() && FONT_STYLE_IDS.contains(&s.as_str()) {
                    font_style = Some(c.clone());
                    if let Some(i) = int_lu.take() {
                        if font_weight.is_none() {
                            font_weight = Some(i);
                        } else {
                            return Err(invalid(ty(&i)));
                        }
                    }
                } else if font_variant.is_none() && FONT_VARIANT_IDS.contains(&s.as_str()) {
                    font_variant = Some(c.clone());
                    if let Some(i) = int_lu.take() {
                        if font_weight.is_none() {
                            font_weight = Some(i);
                        } else {
                            return Err(invalid(ty(&i)));
                        }
                    }
                } else if int_lu.is_none() && font_weight.is_none() && FONT_WEIGHT_IDS.contains(&s.as_str()) {
                    font_weight = Some(c.clone());
                } else {
                    done = true;
                }
            }
            SAC_INTEGER => {
                if int_lu.is_none() && font_weight.is_none() {
                    int_lu = Some(c.clone());
                } else {
                    done = true;
                }
            }
            _ => done = true,
        }
        if !done {
            l = c.get_next_lexical_unit();
        }
    }
    let Some(c0) = l else { return Err(malformed()) };
    let mut cur = Some(c0.clone());
    match ty(&c0) {
        SAC_IDENT => {
            let s = lower(&sval(&c0));
            if FONT_SIZE_IDS.contains(&s.as_str()) {
                font_size = Some(c0.clone());
                cur = c0.get_next_lexical_unit();
            }
        }
        SAC_EM | SAC_EX | SAC_PIXEL | SAC_CENTIMETER | SAC_MILLIMETER | SAC_INCH | SAC_POINT | SAC_PICA | SAC_INTEGER | SAC_REAL | SAC_PERCENTAGE => {
            font_size = Some(c0.clone());
            cur = c0.get_next_lexical_unit();
        }
        _ => {}
    }
    if font_size.is_none() {
        match int_lu.take() {
            Some(i) => font_size = Some(i),
            None => return Err(invalid(ty(&c0))),
        }
    }
    if let Some(i) = int_lu.take() {
        if font_weight.is_none() {
            font_weight = Some(i);
        } else {
            return Err(invalid(ty(&i)));
        }
    }
    let mut c = cur.ok_or_else(malformed)?;
    if ty(&c) == SAC_OPERATOR_SLASH {
        let n = c.get_next_lexical_unit().ok_or_else(malformed)?;
        line_height = Some(n.clone());
        c = n.get_next_lexical_unit().ok_or_else(malformed)?;
    }
    let _ = line_height;
    let font_family = c;
    ph.property(engine, "font-family", &font_family, imp)?;
    ph.property(engine, "font-style", font_style.as_ref().unwrap_or(&normal), imp)?;
    ph.property(engine, "font-variant", font_variant.as_ref().unwrap_or(&normal), imp)?;
    ph.property(engine, "font-weight", font_weight.as_ref().unwrap_or(&normal), imp)?;
    ph.property(engine, "font-size", font_size.as_ref().unwrap(), imp)?;
    Ok(())
}
