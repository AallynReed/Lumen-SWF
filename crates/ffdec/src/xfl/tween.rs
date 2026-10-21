//! FFDec's tween recovery: `ClassicTweenDetector` turns runs of keyframes into classic tweens
//! (or motion objects, see `motion`), `FilterTweenDetector` reads animated filters, and
//! `EasingDetector` finds a shape tween's ease.

use std::collections::{HashMap, HashSet};

use regex::Regex;

use super::convert::{KEY_MODE_CLASSIC_TWEEN, KEY_MODE_MOTION_TWEEN, KEY_MODE_NORMAL};
use super::geom::{p, BezierEdge};
use super::xml::{parse_fragment, Element, XmlWriter};

const COLOR_ATTRIBUTES: [&str; 8] = ["redMultiplier", "greenMultiplier", "blueMultiplier", "alphaMultiplier", "redOffset", "greenOffset", "blueOffset", "alphaOffset"];
pub const MATRIX_TOLERANCE: f64 = 2.0 / 65536.0;
pub const POSITION_TOLERANCE: f64 = 0.051;
const MULTIPLIER_TOLERANCE: f64 = 1.0 / 256.0;

#[derive(Debug, Clone)]
pub struct Filter {
    pub prefix: &'static str,
    pub numeric: Vec<&'static str>,
    pub colors: Vec<&'static str>,
    pub values: Vec<f64>,
    pub tolerances: Vec<f64>,
    pub offset: usize,
    pub gradient_stops: usize,
    pub settings: Vec<(String, i32)>,
}

#[derive(Debug, Clone, Default)]
pub struct Sample {
    pub xml: String,
    pub index: i32,
    pub duration: i32,
    pub identity: Option<Element>,
    /// Translation, two scales, two axis angles, color multipliers and offsets, then filters.
    pub values: Vec<f64>,
    pub matrix: [f64; 4],
    pub filters: Vec<Filter>,
}

fn number(e: Option<&Element>, name: &str, default: f64) -> f64 {
    e.and_then(|e| e.attr(name)).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

fn alpha_attribute(color: &str) -> String {
    if color == "color" {
        "alpha".into()
    } else {
        color.replace("Color", "Alpha")
    }
}

fn is_hex6(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// `FilterTweenDetector.read`: None when a filter can not be animated.
pub fn read_filters(container: Option<&Element>) -> Option<Vec<Filter>> {
    let Some(container) = container else { return Some(Vec::new()) };
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 14;
    for e in container.elements() {
        let prefix = match e.name.as_str() {
            "BlurFilter" => "Blur",
            "GlowFilter" => "Glow",
            "DropShadowFilter" => "DropShadow",
            "BevelFilter" => "Bevel",
            "GradientGlowFilter" => "GradientGlow",
            "GradientBevelFilter" => "GradientBevel",
            "AdjustColorFilter" => "AdjustColor",
            _ => return None,
        };
        if !seen.insert(prefix) || e.attr("isEnabled") == Some("false") {
            return None;
        }
        if prefix == "AdjustColor" {
            let numeric = vec!["brightness", "contrast", "saturation", "hue"];
            for (k, _) in &e.attrs {
                if !numeric.contains(&k.as_str()) && k != "isEnabled" {
                    return None;
                }
            }
            if e.elements().next().is_some() {
                return None;
            }
            let mut values = vec![0.0; 4];
            for c in 0..4 {
                let v = number(Some(e), numeric[c], 0.0);
                let limit = if c == 3 { 180.0 } else { 100.0 };
                if !v.is_finite() || v != v.round_ties_even() || v.abs() > limit {
                    return None;
                }
                values[c] = v;
            }
            result.push(Filter { prefix, numeric, colors: Vec::new(), values, tolerances: vec![1.0; 4], offset, gradient_stops: 0, settings: Vec::new() });
            offset += 4;
            continue;
        }
        let blur = prefix == "Blur";
        let gradient = prefix.starts_with("Gradient");
        let directional = gradient || prefix == "DropShadow" || prefix == "Bevel";
        let bevel = prefix == "Bevel";
        if bevel && e.has("type") && e.attr("type") != Some("inner") {
            return None;
        }
        let numeric: Vec<&'static str> = if blur {
            vec!["blurX", "blurY"]
        } else if directional {
            vec!["blurX", "blurY", "strength", "angle", "distance"]
        } else {
            vec!["blurX", "blurY", "strength"]
        };
        let colors: Vec<&'static str> = if blur || gradient {
            vec![]
        } else if bevel {
            vec!["shadowColor", "highlightColor"]
        } else {
            vec!["color"]
        };
        let mut allowed: HashSet<String> = numeric.iter().chain(&colors).map(|s| s.to_string()).collect();
        for a in ["quality", "isEnabled", "inner", "knockout", "hideObject", "type"] {
            allowed.insert(a.into());
        }
        if gradient {
            allowed.remove("inner");
            allowed.remove("hideObject");
        }
        for c in &colors {
            allowed.insert(alpha_attribute(c));
        }
        if e.attrs.iter().any(|(k, _)| !allowed.contains(k)) {
            return None;
        }
        let mut entries = Vec::new();
        for c in e.elements() {
            if !gradient || c.name != "GradientEntry" || c.elements().next().is_some() {
                return None;
            }
            if c.attrs.iter().any(|(k, _)| k != "color" && k != "alpha" && k != "ratio") {
                return None;
            }
            entries.push(c);
        }
        if gradient && (entries.is_empty() || entries.len() > 255) {
            return None;
        }
        let n = numeric.len() + 4 * colors.len() + 5 * entries.len();
        let mut values = vec![0.0; n];
        let mut tolerances = vec![0.0; n];
        for (i, name) in numeric.iter().enumerate() {
            let def = match *name {
                "strength" => 1.0,
                "angle" => 45.0,
                "distance" => 5.0,
                _ => 4.0,
            };
            let mut v = number(Some(e), name, def);
            let mut tol = 1.0 / 65536.0;
            if *name == "strength" {
                v *= 100.0;
                tol = 1.0;
            } else if *name == "angle" {
                tol = (1.0f64 / 65536.0).to_degrees();
            }
            if !v.is_finite() || (*name != "angle" && v < 0.0) {
                return None;
            }
            values[i] = v;
            tolerances[i] = tol;
        }
        for (i, name) in colors.iter().enumerate() {
            let hex = e.attr(name).map(str::to_string).unwrap_or_else(|| {
                if *name == "highlightColor" {
                    "#FFFFFF".into()
                } else if prefix == "Glow" {
                    "#FF0000".into()
                } else {
                    "#000000".into()
                }
            });
            if !is_hex6(&hex) {
                return None;
            }
            let rgb = i64::from_str_radix(&hex[1..], 16).ok()?;
            let alpha = number(Some(e), &alpha_attribute(name), 1.0);
            if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
                return None;
            }
            let o = numeric.len() + 4 * i;
            values[o] = (rgb >> 16) as f64;
            values[o + 1] = (rgb >> 8 & 255) as f64;
            values[o + 2] = (rgb & 255) as f64;
            values[o + 3] = super::num::java_round(alpha * 255.0) as f64;
            for c in 0..4 {
                tolerances[o + c] = 1.0;
            }
        }
        let mut previous = -1;
        for (stop, entry) in entries.iter().enumerate() {
            let hex = entry.attr("color").unwrap_or("");
            let alpha = number(Some(entry), "alpha", 1.0);
            let ratio = number(Some(entry), "ratio", 0.0);
            if !is_hex6(hex) || !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) || !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
                return None;
            }
            let index = super::num::java_round(ratio * 255.0) as i32;
            if index < previous {
                return None;
            }
            previous = index;
            let rgb = i64::from_str_radix(&hex[1..], 16).ok()?;
            let b = numeric.len() + 5 * stop;
            values[b] = (rgb >> 16) as f64;
            values[b + 1] = (rgb >> 8 & 255) as f64;
            values[b + 2] = (rgb & 255) as f64;
            values[b + 3] = super::num::java_round(alpha * 255.0) as f64;
            values[b + 4] = index as f64;
            for c in 0..5 {
                tolerances[b + c] = 1.0;
            }
        }
        let qf = number(Some(e), "quality", 1.0);
        let quality = qf as i32;
        if !(1..=3).contains(&quality) || quality as f64 != qf {
            return None;
        }
        let mut settings = vec![("Quality".to_string(), quality)];
        let flag = |n: &str| (e.attr(n) == Some("true")) as i32;
        if !blur {
            settings.push(("Knockout".into(), flag("knockout")));
            if gradient {
                let v = match e.attr("type").unwrap_or("inner") {
                    "inner" => 0,
                    "outer" => 1,
                    "full" => 2,
                    _ => return None,
                };
                settings.push(("Type".into(), v));
            } else if bevel {
                settings.push(("Type".into(), 0));
            } else {
                settings.push((if prefix == "Glow" { "InnerGlow" } else { "InnerShadow" }.into(), flag("inner")));
                if directional {
                    settings.push(("HideObject".into(), flag("hideObject")));
                }
            }
        }
        let len = values.len();
        result.push(Filter { prefix, numeric, colors, values, tolerances, offset, gradient_stops: entries.len(), settings });
        offset += len;
    }
    Some(result)
}

fn normalize_identity(container: &mut Element, filters: &[Filter]) {
    let mut index = 0;
    for c in &mut container.children {
        let super::xml::Node::Element(e) = c else { continue };
        let Some(f) = filters.get(index) else { break };
        index += 1;
        for n in &f.numeric {
            e.remove_attr(n);
        }
        for n in &f.colors {
            e.remove_attr(n);
            e.remove_attr(&alpha_attribute(n));
        }
        if f.gradient_stops > 0 {
            for c in &mut e.children {
                if let super::xml::Node::Element(entry) = c {
                    entry.remove_attr("color");
                    entry.remove_attr("alpha");
                    entry.remove_attr("ratio");
                }
            }
        }
    }
}

pub fn filters_changing(samples: &[Sample], start: usize, end: usize) -> bool {
    let n = samples[start].values.len();
    (start + 1..=end).any(|f| (14..n).any(|c| samples[f].values.get(c) != samples[start].values.get(c)))
}

pub fn filter_tolerance(sample: &Sample, component: usize) -> f64 {
    for f in &sample.filters {
        if component >= f.offset && component < f.offset + f.values.len() {
            return f.tolerances[component - f.offset];
        }
    }
    1.0
}

pub fn strength_percent(v: f64) -> i32 {
    let fixed = super::num::java_round(v * 256.0 / 100.0);
    let percent = (fixed as f64 * 100.0 / 256.0).ceil() as i32;
    if (percent as f64 * 256.0 / 100.0).floor() as i64 == fixed {
        percent
    } else {
        -1
    }
}

pub fn native_strengths(samples: &[Sample], start: usize, end: usize) -> bool {
    for s in &samples[start..=end] {
        for f in &s.filters {
            for (p, n) in f.numeric.iter().enumerate() {
                if *n == "strength" && strength_percent(s.values[f.offset + p]) < 0 {
                    return false;
                }
            }
        }
    }
    true
}

/// `BigDecimal.valueOf(v).stripTrailingZeros().toPlainString()`.
pub fn plain(v: f64) -> String {
    let s = super::num::java_double(v);
    let (mant, exp) = match s.split_once('E') {
        Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
        None => (s.clone(), 0),
    };
    let neg = mant.starts_with('-');
    let mant = mant.trim_start_matches('-');
    let (ip, fp) = mant.split_once('.').unwrap_or((mant, ""));
    let mut digits: String = format!("{ip}{fp}");
    let mut point = ip.len() as i32 + exp;
    let lead = digits.len() - digits.trim_start_matches('0').len();
    digits = digits[lead..].to_string();
    point -= lead as i32;
    let digits = digits.trim_end_matches('0').to_string();
    if digits.is_empty() {
        return "0".into();
    }
    let out = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
    };
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

fn hex8(packed: i64) -> String {
    format!("0x{packed:08x}")
}

/// `FilterTweenDetector.write`.
pub fn write_filters(w: &mut XmlWriter, samples: &[Sample], start: usize, end: usize) {
    w.start_with("PropertyContainer", &[("id", "Filters")]);
    for f in &samples[start].filters {
        w.start_with("PropertyContainer", &[("id", &format!("{}_Filter", f.prefix))]);
        for (p, attr) in f.numeric.iter().enumerate() {
            let mut name = attr[..1].to_uppercase();
            name += &attr[1..];
            let pp = if f.prefix == "AdjustColor" { "AdjColor" } else { f.prefix };
            write_keys(w, &format!("{pp}_{name}"), samples, start, end, f.offset + p, false);
        }
        for (n, v) in &f.settings {
            w.empty_with("Property", &[("id", &format!("{}_{}", f.prefix, n)), ("enabled", "1"), ("readonly", "0"), ("visible", "1"), ("value", &v.to_string())]);
        }
        for (p, name) in f.colors.iter().enumerate() {
            let name = if *name == "highlightColor" {
                "HilightColor".to_string()
            } else {
                format!("{}{}", name[..1].to_uppercase(), &name[1..])
            };
            write_keys(w, &format!("{}_{}", f.prefix, name), samples, start, end, f.offset + f.numeric.len() + 4 * p, true);
        }
        if f.gradient_stops > 0 {
            write_gradient(w, f, samples, start, end);
        }
        w.end();
    }
    w.end();
}

fn write_gradient(w: &mut XmlWriter, f: &Filter, samples: &[Sample], start: usize, end: usize) {
    let base = f.offset + f.numeric.len();
    let changing = (start + 1..=end).any(|i| (0..5 * f.gradient_stops).any(|c| samples[i].values[base + c] != samples[start].values[base + c]));
    w.start_with("Property", &[("id", &format!("{}_Gradient", f.prefix)), ("enabled", "1"), ("readonly", "0"), ("visible", "1"), ("ignoreTimeMap", "1")]);
    let stop_at = if changing { end } else { start };
    for fi in start..=stop_at {
        let s = &samples[fi];
        let last = if fi == end || !changing { s.index } else { s.index + s.duration - 1 };
        let mut colors = Vec::new();
        let mut indexes = Vec::new();
        for stop in 0..f.gradient_stops {
            let mut packed: i64 = 0;
            for c in 0..4 {
                packed = packed << 8 | s.values[base + 5 * stop + c] as i64;
            }
            colors.push(hex8(packed));
            indexes.push((s.values[base + 5 * stop + 4] as i32).to_string());
        }
        for frame in s.index..=last {
            w.empty_with("Keyframe", &[("timevalue", &((frame - samples[start].index) as i64 * 1000).to_string()), ("roving", "0"), ("colors", &colors.join(",")), ("indexes", &indexes.join(","))]);
        }
    }
    w.end();
}

fn write_keys(w: &mut XmlWriter, id: &str, samples: &[Sample], start: usize, end: usize, component: usize, color: bool) {
    w.start_with("Property", &[("id", id), ("enabled", "1"), ("readonly", "0"), ("visible", "1"), ("ignoreTimeMap", "1")]);
    let k = if color { 4 } else { 1 };
    let changing = (start + 1..=end).any(|f| (0..k).any(|c| samples[f].values[component + c] != samples[start].values[component + c]));
    let stop_at = if changing { end } else { start };
    for fi in start..=stop_at {
        let s = &samples[fi];
        let last = if fi == end || !changing { s.index } else { s.index + s.duration - 1 };
        for frame in s.index..=last {
            let time = ((frame - samples[start].index) as i64 * 1000).to_string();
            if color {
                let mut packed: i64 = 0;
                for c in 0..4 {
                    packed = packed << 8 | s.values[component + c] as i64;
                }
                w.empty_with("Keyframe", &[("timevalue", &time), ("roving", "0"), ("value", &hex8(packed))]);
            } else {
                let v = if id.ends_with("_Strength") { strength_percent(s.values[component]) as f64 } else { s.values[component] };
                let value = format!("0,{}", plain(v));
                w.empty_with("Keyframe", &[("timevalue", &time), ("roving", "0"), ("anchor", &value), ("previous", &value), ("next", &value)]);
            }
        }
    }
    w.end();
}

// ---- classic tweens ----

fn read_sample(frame: &Element, xml: &str) -> Sample {
    let mut s = Sample { xml: xml.to_string(), values: vec![0.0; 14], ..Default::default() };
    s.index = frame.attr("index").and_then(|v| v.parse().ok()).unwrap_or(0);
    s.duration = frame.attr("duration").and_then(|v| v.parse().ok()).unwrap_or(1);
    if frame.attr("keyMode") != Some(&KEY_MODE_NORMAL.to_string()) || frame.attrs.len() > if frame.has("duration") { 3 } else { 2 } {
        return s;
    }
    let Some(elements) = frame.child("elements") else { return s };
    let Some(instance) = elements.child("DOMSymbolInstance") else { return s };
    if elements.elements().count() != 1 {
        return s;
    }
    if instance.child("Actionscript").is_some() || instance.child("persistentData").is_some() {
        return s;
    }
    let matrix = instance.child("matrix").and_then(|m| m.child("Matrix"));
    let a = number(matrix, "a", 1.0);
    let b = number(matrix, "b", 0.0);
    let c = number(matrix, "c", 0.0);
    let d = number(matrix, "d", 1.0);
    if a * d - b * c == 0.0 {
        return s;
    }
    s.matrix = [a, b, c, d];
    s.values[0] = number(matrix, "tx", 0.0);
    s.values[1] = number(matrix, "ty", 0.0);
    s.values[2] = a.hypot(b);
    s.values[3] = c.hypot(d).copysign(a * d - b * c);
    s.values[4] = b.atan2(a);
    s.values[5] = (-c / s.values[3]).atan2(d / s.values[3]);
    let color = instance.child("color").and_then(|c| c.child("Color"));
    for (i, n) in COLOR_ATTRIBUTES.iter().enumerate() {
        s.values[6 + i] = number(color, n, if i < 4 { 1.0 } else { 0.0 });
    }
    let filters = read_filters(instance.child("filters"));
    if let Some(fl) = filters {
        if !fl.is_empty() {
            let last = fl.last().unwrap();
            s.values.resize(last.offset + last.values.len(), 0.0);
            for f in &fl {
                s.values[f.offset..f.offset + f.values.len()].copy_from_slice(&f.values);
            }
            s.filters = fl;
        }
    }
    let mut identity = instance.clone();
    identity.remove_child("matrix");
    identity.remove_child("color");
    if !s.filters.is_empty() {
        if let Some(fc) = identity.child_mut("filters") {
            normalize_identity(fc, &s.filters);
        }
    }
    identity.remove_attr("centerPoint3DX");
    identity.remove_attr("centerPoint3DY");
    identity.remove_whitespace();
    s.identity = Some(identity);
    s
}

fn fit_classic(samples: &[Sample], start: usize, end: usize) -> Option<i32> {
    if filters_changing(samples, start, end) {
        return None;
    }
    for s in &samples[start..=end] {
        if s.matrix[1] != 0.0 || s.matrix[2] != 0.0 {
            return None;
        }
    }
    for s in &samples[start..end] {
        if s.duration != 1 {
            return None;
        }
    }
    let first = &samples[start].values;
    let last = &samples[end].values;
    if (last[4] - first[4]).abs() >= std::f64::consts::PI || (last[5] - first[5]).abs() >= std::f64::consts::PI {
        return None;
    }
    let tol_of = |i: usize| {
        if i < 2 {
            POSITION_TOLERANCE
        } else if i < 6 {
            MATRIX_TOLERANCE
        } else if i < 10 {
            MULTIPLIER_TOLERANCE
        } else {
            1.0
        }
    };
    let mut largest: f64 = 0.0;
    for i in 0..first.len() {
        let change = (last.get(i).copied().unwrap_or(0.0) - first[i]).abs() / tol_of(i);
        largest = largest.max(change);
    }
    if largest < 4.0 {
        return None;
    }
    let mut min_ease: f64 = -100.0;
    let mut max_ease: f64 = 100.0;
    for f in start + 1..end {
        let t = (f - start) as f64 / (end - start) as f64;
        let q = t * (1.0 - t) / 100.0;
        for i in 0..first.len() {
            let tol = if i < 2 {
                POSITION_TOLERANCE
            } else if i < 4 {
                2.0 * MATRIX_TOLERANCE
            } else if i < 6 {
                2.0 * MATRIX_TOLERANCE / samples[f].values[i - 2]
            } else if i < 10 {
                MULTIPLIER_TOLERANCE
            } else {
                1.0
            };
            let delta = last[i] - first[i];
            let diff = samples[f].values[i] - first[i] - delta * t;
            if delta.abs() < 1e-12 {
                if diff.abs() > tol {
                    return None;
                }
                continue;
            }
            let lower = (diff - tol) / (delta * q);
            let upper = (diff + tol) / (delta * q);
            min_ease = min_ease.max(lower.min(upper));
            max_ease = max_ease.min(lower.max(upper));
            if min_ease > max_ease {
                return None;
            }
        }
    }
    let ease = super::num::java_round((min_ease + max_ease) / 2.0) as i32;
    if (ease as f64) < min_ease - 1e-7 || (ease as f64) > max_ease + 1e-7 {
        return None;
    }
    for f in start + 1..end {
        let t = (f - start) as f64 / (end - start) as f64;
        let progress = t + ease as f64 / 100.0 * t * (1.0 - t);
        let expected: Vec<f64> = (0..first.len()).map(|i| first[i] + (last[i] - first[i]) * progress).collect();
        let s = &samples[f];
        for i in 0..expected.len() {
            if (2..6).contains(&i) {
                continue;
            }
            let tol = if i < 2 {
                POSITION_TOLERANCE
            } else if i < 10 {
                MULTIPLIER_TOLERANCE
            } else {
                1.0
            };
            if (s.values[i] - expected[i]).abs() > tol {
                return None;
            }
        }
        let m = [expected[2] * expected[4].cos(), expected[2] * expected[4].sin(), -expected[3] * expected[5].sin(), expected[3] * expected[5].cos()];
        for i in 0..4 {
            if (s.matrix[i] - m[i]).abs() > MATRIX_TOLERANCE {
                return None;
            }
        }
    }
    Some(-ease)
}

fn replace_first(s: &str, re: &Regex, rep: &str) -> String {
    re.replacen(s, 1, regex::NoExpand(rep)).into_owned()
}

/// `ClassicTweenDetector.detect`.
pub fn detect(frames: &str, starts: &HashSet<i32>, frame_rate: f64, reg_points: &HashMap<String, [f64; 2]>) -> String {
    if !frames.contains("<DOMSymbolInstance") || frames.contains("<Actionscript") {
        return frames.to_string();
    }
    let frame_re = Regex::new(r"(?s)<DOMFrame\b[^>]*>.*?</DOMFrame>").unwrap();
    let root = parse_fragment(frames);
    let mut matches = frame_re.find_iter(frames);
    let mut samples = Vec::new();
    for e in root.elements() {
        let Some(m) = matches.next() else { return frames.to_string() };
        samples.push(read_sample(e, m.as_str()));
    }
    for i in 1..samples.len() {
        let (a, b) = samples.split_at_mut(i);
        let prev = &a[i - 1];
        let next = &mut b[0];
        if let (Some(pi), Some(ni)) = (&prev.identity, &next.identity) {
            if pi.equal_node(ni) {
                for axis in 4..6 {
                    next.values[axis] += ((prev.values[axis] - next.values[axis]) / (2.0 * std::f64::consts::PI)).round_ties_even() * 2.0 * std::f64::consts::PI;
                }
            }
        }
    }
    let n = samples.len();
    let mut run_ends = vec![0usize; n];
    for i in (0..n).rev() {
        run_ends[i] = i;
        if i + 1 < n {
            let (c, x) = (&samples[i], &samples[i + 1]);
            if (c.duration == 1 || frame_rate > 0.0)
                && c.identity.is_some()
                && x.identity.is_some()
                && x.index == c.index + c.duration
                && !starts.contains(&x.index)
                && c.identity.as_ref().unwrap().equal_node(x.identity.as_ref().unwrap())
            {
                run_ends[i] = run_ends[i + 1];
            }
        }
    }
    let duration_re = Regex::new(r#" duration="[0-9]+""#).unwrap();
    let keymode_re = Regex::new(r#"keyMode="[0-9]+""#).unwrap();
    let index_re = Regex::new(r#"index="[0-9]+""#).unwrap();
    let core_re = Regex::new(r#"<AnimationCore\b[^>]*\bduration="([0-9]+)""#).unwrap();
    let mut changed = false;
    let mut result = String::new();
    let mut start = 0;
    while start < n {
        let first = &samples[start];
        let limit = run_ends[start];
        let mut accel = None;
        let mut motion = None;
        let mut end = limit;
        while end >= start + 3 {
            accel = fit_classic(&samples, start, end);
            if accel.is_some() {
                break;
            }
            let reg = first.identity.as_ref().and_then(|i| i.attr("libraryItemName")).and_then(|n| reg_points.get(n));
            if frame_rate > 0.0 {
                if let Some(reg) = reg {
                    motion = super::motion::fit(&samples, start, end, frame_rate, *reg);
                    if motion.is_some() {
                        break;
                    }
                }
            }
            end -= 1;
        }
        if let Some(motion) = motion {
            changed = true;
            let endpoint = &samples[end];
            let mut duration = endpoint.index - first.index + 1;
            if let Some(c) = core_re.captures(&motion) {
                duration = c[1].parse::<i32>().unwrap_or(0) / 1000;
            }
            let mut xml = replace_first(&first.xml, &duration_re, "");
            xml = replace_first(&xml, &keymode_re, &format!("keyMode=\"{KEY_MODE_MOTION_TWEEN}\""));
            xml = xml.replacen(
                "<DOMFrame",
                &format!("<DOMFrame tweenType=\"motion object\" isMotionObject=\"true\" visibleAnimationKeyframes=\"2097151\" motionTweenRotate=\"none\" motionTweenScale=\"false\" duration=\"{duration}\""),
                1,
            );
            let es = xml.find("<elements").unwrap_or(xml.len());
            xml = format!("{}{}{}", &xml[..es], motion, &xml[es..]);
            result += &xml;
            result.push('\n');
            let remaining = endpoint.index + endpoint.duration - first.index - duration;
            if remaining > 0 {
                let hold = replace_first(&endpoint.xml, &index_re, &format!("index=\"{}\"", first.index + duration));
                let hold = replace_first(&hold, &duration_re, &format!(" duration=\"{remaining}\""));
                result += &hold;
                result.push('\n');
            }
            start = end + 1;
        } else if let Some(a) = accel {
            changed = true;
            let duration = samples[end].index - first.index;
            let mut xml = replace_first(&first.xml, &duration_re, "");
            xml = replace_first(&xml, &keymode_re, &format!("keyMode=\"{KEY_MODE_CLASSIC_TWEEN}\""));
            xml = xml.replacen("<DOMFrame", &format!("<DOMFrame tweenType=\"motion\" motionTweenScale=\"true\" acceleration=\"{a}\" duration=\"{duration}\""), 1);
            result += &xml;
            result.push('\n');
            start = end;
        } else {
            result += &first.xml;
            result.push('\n');
            start += 1;
        }
    }
    if changed {
        result
    } else {
        frames.to_string()
    }
}

// ---- shape tween easing ----

fn frame_percent_to_ratio(pct: f64, ease: i32) -> f64 {
    let be = BezierEdge::quad(0.0, 0.0, 50.0, (65535 / 2 + ease * 65535 / 100 / 2) as f64, 100.0, 65535.0);
    let line = BezierEdge::line(pct, 0.0, pct, 65535.0);
    let mut t1 = Vec::new();
    let mut t2 = Vec::new();
    let mut pts = Vec::new();
    be.intersects(&line, &mut t1, &mut t2, &mut pts);
    pts.first().map_or(0.0, |q| q.y)
}

/// `EasingDetector.getEaseFromShapeRatios`.
pub fn ease_from_shape_ratios(ratios: &mut Vec<i32>) -> Option<i32> {
    if ratios.is_empty() {
        return None;
    }
    if ratios[0] != 0 {
        ratios.insert(0, 0);
    }
    if *ratios.last().unwrap() != 65535 {
        ratios.push(65535);
    }
    let mut ease = 100;
    loop {
        let mut min_dist = f64::MAX;
        let mut max_dist = f64::MIN_POSITIVE;
        for (f, &r) in ratios.iter().enumerate() {
            let pct = f as f64 * 100.0 / (ratios.len() - 1) as f64;
            let should = super::num::java_round(frame_percent_to_ratio(pct, ease)) as f64;
            let dist = should - r as f64;
            if dist > max_dist {
                max_dist = dist;
            }
            if dist < min_dist {
                min_dist = dist;
            }
        }
        if min_dist > -5.0 && max_dist < 5.0 {
            break;
        }
        if ease == -100 {
            return None;
        }
        ease -= 1;
    }
    let _ = p;
    Some(ease)
}
