//! `MotionTweenDetector`: motion-object tweens (CS5+) fitted to sampled keyframes, with
//! `NativeTimeMap` eases and `PresetEasing` curves.

use super::num::{java_double, java_round};
use super::tween::{filter_tolerance, native_strengths, plain, write_filters, Sample, MATRIX_TOLERANCE, POSITION_TOLERANCE};
use super::xml::XmlWriter;

const COLOR_PROPERTIES: [&str; 8] = ["AdvClr_R_Pct", "AdvClr_G_Pct", "AdvClr_B_Pct", "AdvClr_A_Pct", "AdvClr_R_Offset", "AdvClr_G_Offset", "AdvClr_B_Offset", "AdvClr_A_Offset"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapType {
    Quadratic,
    DualQuadratic,
    Cubic,
    Quartic,
    Quintic,
    DualCubic,
    DualQuartic,
    DualQuintic,
    Spring,
    Bounce,
    BounceIn,
}

impl MapType {
    fn xml(self) -> &'static str {
        match self {
            MapType::Quadratic => "Quadratic",
            MapType::DualQuadratic => "DualQuadratic",
            MapType::Cubic => "Cubic",
            MapType::Quartic => "Quartic",
            MapType::Quintic => "Quintic",
            MapType::DualCubic => "DualCubic",
            MapType::DualQuartic => "DualQuartic",
            MapType::DualQuintic => "DualQuintic",
            MapType::Spring => "Spring",
            MapType::Bounce => "Bounce",
            MapType::BounceIn => "BounceIn",
        }
    }

    fn power(self) -> i32 {
        match self {
            MapType::Cubic | MapType::DualCubic => 3,
            MapType::Quartic | MapType::DualQuartic => 4,
            MapType::Quintic | MapType::DualQuintic => 5,
            _ => 0,
        }
    }

    fn dual(self) -> bool {
        matches!(self, MapType::DualCubic | MapType::DualQuartic | MapType::DualQuintic)
    }
}

/// `NativeTimeMap`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TimeMap {
    ty: MapType,
    strength: i32,
    intervals: i32,
}

impl TimeMap {
    fn new(ty: MapType, strength: i32) -> TimeMap {
        TimeMap { ty, strength, intervals: 0 }
    }

    fn wave(ty: MapType, strength: i32, intervals: i32) -> TimeMap {
        TimeMap { ty, strength, intervals: if matches!(ty, MapType::Bounce | MapType::BounceIn) { intervals } else { 0 } }
    }

    fn is_wave(&self) -> bool {
        matches!(self.ty, MapType::Spring | MapType::Bounce | MapType::BounceIn)
    }

    fn is_polynomial(&self) -> bool {
        self.ty.power() >= 3
    }

    fn progress(&self, t: f64) -> f64 {
        let s = self.strength;
        if self.is_polynomial() {
            let p = self.ty.power();
            let q = if self.ty.dual() {
                if t <= 0.5 {
                    0.5 * (1.0 - (1.0 - 2.0 * t).powi(p))
                } else {
                    0.5 * (1.0 + (2.0 * t - 1.0).powi(p))
                }
            } else {
                1.0 - (1.0 - t).powi(p)
            };
            return t + s as f64 / 100.0 * (q - t);
        }
        if self.ty == MapType::Spring {
            let first = 0.4 / s as f64;
            if t <= first {
                return (std::f64::consts::PI * t / (2.0 * first)).sin();
            }
            let u = 2.0 * s as f64 * (t - first) / (1.0 - first);
            return 0.7 + 0.3 * (-u / 2.0).exp() * (std::f64::consts::PI * u).cos();
        }
        if matches!(self.ty, MapType::Bounce | MapType::BounceIn) {
            let mut harmonic = 0.0;
            for j in 1..=s {
                harmonic += 1.0 / j as f64;
            }
            let frame = t * self.intervals as f64;
            let mut start = 0;
            for j in 1..=s {
                let width = (java_round(self.intervals as f64 / (j as f64 * harmonic)) as i32).min(self.intervals - start);
                if width <= 0 {
                    break;
                }
                if frame <= (start + width) as f64 + 1e-9 {
                    let u = ((frame - start as f64) / width as f64).clamp(0.0, 1.0);
                    if self.ty == MapType::BounceIn && j == 1 {
                        return u * u;
                    }
                    let parabola = (1.0 - j as f64).exp() * 4.0 * u * (1.0 - u);
                    return if self.ty == MapType::BounceIn { 1.0 - parabola } else { parabola };
                }
                start += width;
            }
            return if self.ty == MapType::BounceIn { 1.0 } else { 0.0 };
        }
        let basis = if self.ty == MapType::Quadratic { t * (1.0 - t) } else { (if t < 0.5 { t } else { 1.0 - t }) * (1.0 - 2.0 * t) };
        t + s as f64 / 100.0 * basis
    }
}

// ---- PresetEasing ----

fn bounce_out(t: f64) -> f64 {
    if t < 4.0 / 11.0 {
        return 121.0 / 16.0 * t * t;
    }
    if t < 8.0 / 11.0 {
        let u = t - 6.0 / 11.0;
        return 121.0 / 16.0 * u * u + 0.75;
    }
    if t < 10.0 / 11.0 {
        let u = t - 9.0 / 11.0;
        return 121.0 / 16.0 * u * u + 0.9375;
    }
    let u = t - 21.0 / 22.0;
    121.0 / 16.0 * u * u + 0.984375
}

fn preset_in(family: usize, t: f64, dual: bool) -> f64 {
    match family {
        0 => t * t,
        1 => t * t * t,
        2 => t * t * t * t,
        3 => t * t * t * t * t,
        4 => 1.0 - (std::f64::consts::PI * t / 2.0).cos(),
        5 => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        6 => {
            if t == 0.0 {
                0.0
            } else {
                2f64.powf(10.0 * (t - 1.0))
            }
        }
        7 => {
            let o = 1.70158 * if dual { 1.525 } else { 1.0 };
            t * t * ((o + 1.0) * t - o)
        }
        8 => 1.0 - bounce_out(1.0 - t),
        _ => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let period = if dual { 0.45 } else { 0.3 };
            -2f64.powf(10.0 * (t - 1.0)) * ((t - 1.0 - period / 4.0) * 2.0 * std::f64::consts::PI / period).sin()
        }
    }
}

fn preset_progress(family: usize, direction: i32, t: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    match direction {
        1 => 1.0 - preset_in(family, 1.0 - t, false),
        2 => {
            if t < 0.5 {
                preset_in(family, 2.0 * t, true) / 2.0
            } else {
                1.0 - preset_in(family, 2.0 - 2.0 * t, true) / 2.0
            }
        }
        _ => preset_in(family, t, false),
    }
}

// ---- fitting ----

#[derive(Debug, Clone)]
struct Segment {
    start: f64,
    end: f64,
    curve: [f64; 4],
}

fn evaluate(c: &[f64; 4], t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * c[0] + 3.0 * u * u * t * c[1] + 3.0 * u * t * t * c[2] + t * t * t * c[3]
}

fn evaluate_channel(curve: &[f64; 4], map: Option<&TimeMap>, segments: Option<&Vec<Segment>>, t: f64) -> f64 {
    let Some(segs) = segments else { return evaluate(curve, map.map_or(t, |m| m.progress(t))) };
    for s in segs {
        if t <= s.end + 1e-12 {
            return evaluate(&s.curve, (t - s.start) / (s.end - s.start));
        }
    }
    segs.last().unwrap().curve[3]
}

fn channel(s: &Sample, c: usize) -> f64 {
    if c == 5 {
        s.values[5] - s.values[4]
    } else {
        s.values[c]
    }
}

fn tolerance(c: usize, s: &Sample) -> f64 {
    if c < 2 {
        POSITION_TOLERANCE
    } else if c < 4 {
        MATRIX_TOLERANCE
    } else if c < 6 {
        if c == 4 {
            MATRIX_TOLERANCE / s.values[2]
        } else {
            MATRIX_TOLERANCE * (1.0 / s.values[2] + 1.0 / s.values[3].abs())
        }
    } else if c >= 14 {
        filter_tolerance(s, c)
    } else if c < 10 {
        1.0 / 256.0
    } else {
        1.0
    }
}

fn matches_position(actual: f64, predicted: f64, map: Option<&TimeMap>) -> bool {
    if map.is_some_and(|m| m.is_wave() || m.is_polynomial()) {
        return (java_round(actual * 20.0) - java_round(predicted * 20.0)).abs() <= 1;
    }
    (actual - predicted).abs() <= POSITION_TOLERANCE
}

fn dual_progress(t: f64, strength: i32) -> f64 {
    t + strength as f64 / 100.0 * (if t < 0.5 { t } else { 1.0 - t }) * (1.0 - 2.0 * t)
}

/// Every frame of a run: (sample, frame) up to the run's last keyframe.
fn frames(samples: &[Sample], start: usize, end: usize) -> Vec<(usize, i32)> {
    let mut out = Vec::new();
    for f in start..=end {
        let s = &samples[f];
        let last = if f == end { s.index } else { s.index + s.duration - 1 };
        for frame in s.index..=last {
            out.push((f, frame));
        }
    }
    out
}

fn matches_channel(samples: &[Sample], start: usize, end: usize, c: usize, curve: &[f64; 4], strength: i32) -> bool {
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    for (f, frame) in frames(samples, start, end) {
        let t = (frame - first) as f64 / duration;
        if (channel(&samples[f], c) - evaluate(curve, dual_progress(t, strength))).abs() > tolerance(c, &samples[f]) {
            return false;
        }
    }
    true
}

fn fit_quadratic(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<i32> {
    let begin = channel(&samples[start], c);
    let delta = channel(&samples[end], c) - begin;
    if delta.abs() < 4.0 * tolerance(c, &samples[start]) {
        return None;
    }
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    let (mut lower, mut upper) = (-100.0f64, 100.0f64);
    let (mut shh, mut shy) = (0.0, 0.0);
    for (f, frame) in frames(samples, start, end) {
        let t = (frame - first) as f64 / duration;
        let h = delta * t * (1.0 - t) / 100.0;
        let residual = channel(&samples[f], c) - begin - delta * t;
        let error = tolerance(c, &samples[f]);
        if h.abs() < 1e-12 {
            if residual.abs() > error {
                return None;
            }
            continue;
        }
        let a = (residual - error) / h;
        let b = (residual + error) / h;
        lower = lower.max(a.min(b));
        upper = upper.min(a.max(b));
        if lower > upper {
            return None;
        }
        shh += h * h;
        shy += h * residual;
    }
    let (mn, mx) = (lower.ceil() as i32, upper.floor() as i32);
    if mn > mx || shh == 0.0 {
        return None;
    }
    Some(mn.max(mx.min(java_round(shy / shh) as i32)))
}

fn fit_dual_quadratic(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<i32> {
    let begin = channel(&samples[start], c);
    let finish = channel(&samples[end], c);
    if (finish - begin).abs() < tolerance(c, &samples[start]) {
        return None;
    }
    let (mut mn, mut mx) = (-100.0f64, 100.0f64);
    let (mut shh, mut shy) = (0.0, 0.0);
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    for (f, frame) in frames(samples, start, end) {
        let t = (frame - first) as f64 / duration;
        let h = (finish - begin) * (if t < 0.5 { t } else { 1.0 - t }) * (1.0 - 2.0 * t) / 100.0;
        let residual = channel(&samples[f], c) - begin - (finish - begin) * t;
        let tol = tolerance(c, &samples[f]);
        if h.abs() < 1e-12 {
            if residual.abs() > tol {
                return None;
            }
            continue;
        }
        let a = (residual - tol) / h;
        let b = (residual + tol) / h;
        mn = mn.max(a.min(b));
        mx = mx.min(a.max(b));
        if mn > mx {
            return None;
        }
        shh += h * h;
        shy += h * residual;
    }
    let (lo, hi) = (mn.ceil() as i32, mx.floor() as i32);
    if lo > hi || shh == 0.0 {
        return None;
    }
    let strength = lo.max(hi.min(java_round(shy / shh) as i32));
    let curve = [begin, begin + (finish - begin) / 3.0, begin + 2.0 * (finish - begin) / 3.0, finish];
    matches_channel(samples, start, end, c, &curve, strength).then_some(strength)
}

fn fit_native_polynomial(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<TimeMap> {
    if end - start < 10 {
        return None;
    }
    let first = samples[start].index;
    let intervals = (samples[end].index - first) as f64;
    let begin = channel(&samples[start], c);
    let delta = channel(&samples[end], c) - begin;
    if delta.abs() < 4.0 * tolerance(c, &samples[start]) {
        return None;
    }
    for ty in [MapType::Cubic, MapType::Quartic, MapType::Quintic, MapType::DualCubic, MapType::DualQuartic, MapType::DualQuintic] {
        let map = TimeMap::new(ty, 50);
        let valid = frames(samples, start, end).into_iter().all(|(f, frame)| {
            let predicted = begin + delta * map.progress((frame - first) as f64 / intervals);
            let actual = channel(&samples[f], c);
            if c < 2 {
                matches_position(actual, predicted, Some(&map))
            } else {
                (actual - predicted).abs() <= tolerance(c, &samples[f])
            }
        });
        if valid {
            return Some(map);
        }
    }
    None
}

fn fit_native_wave(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<(TimeMap, f64)> {
    if end - start < 10 {
        return None;
    }
    let first = samples[start].index;
    let intervals = samples[end].index - first;
    let begin = channel(&samples[start], c);
    for ty in [MapType::Spring, MapType::Bounce, MapType::BounceIn] {
        for strength in [1, 2, 3, 4, 5, 6, 8] {
            let map = TimeMap::wave(ty, strength, intervals);
            let (mut lower, mut upper) = (f64::NEG_INFINITY, f64::INFINITY);
            let (mut spv, mut spp) = (0.0, 0.0);
            let mut valid = true;
            'outer: for f in start..=end {
                let s = &samples[f];
                let last = if f == end { s.index } else { s.index + s.duration - 1 };
                let value = channel(s, c) - begin;
                let error = if c < 2 { 0.075 } else { tolerance(c, s) };
                for frame in s.index..=last {
                    let p = map.progress((frame - first) as f64 / intervals as f64);
                    if p.abs() < 1e-12 {
                        if value.abs() > error {
                            valid = false;
                            break 'outer;
                        }
                    } else {
                        let a = (value - error) / p;
                        let b = (value + error) / p;
                        lower = lower.max(a.min(b));
                        upper = upper.min(a.max(b));
                        if lower > upper {
                            valid = false;
                            break 'outer;
                        }
                    }
                    spv += p * value;
                    spp += p * p;
                }
            }
            if valid && spp > 0.0 {
                let mut delta = lower.max(upper.min(spv / spp));
                let unit = if c < 2 {
                    20.0
                } else if c < 4 {
                    65536.0
                } else {
                    0.0
                };
                if unit != 0.0 {
                    let rounded = ((begin + delta) * unit).round_ties_even() / unit - begin;
                    if rounded >= lower && rounded <= upper {
                        delta = rounded;
                    }
                }
                return Some((map, delta));
            }
        }
    }
    None
}

fn fit_preset(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<Vec<Segment>> {
    if end - start < 10 {
        return None;
    }
    let begin = channel(&samples[start], c);
    let delta = channel(&samples[end], c) - begin;
    if delta.abs() < 4.0 * tolerance(c, &samples[start]) {
        return None;
    }
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    for family in 0..10 {
        for direction in 0..3 {
            let (mut lower, mut upper) = (0.01f64, 1.0f64);
            let mut matches = true;
            for (f, frame) in frames(samples, start, end) {
                let s = &samples[f];
                let t = (frame - first) as f64 / duration;
                let h = delta * (preset_progress(family, direction, t) - t);
                let residual = channel(s, c) - begin - delta * t;
                let error = tolerance(c, s);
                if h.abs() < 1e-12 {
                    matches = residual.abs() <= error;
                } else {
                    let a = (residual - error) / h;
                    let b = (residual + error) / h;
                    lower = lower.max(a.min(b));
                    upper = upper.min(a.max(b));
                    matches = lower <= upper;
                }
                if !matches {
                    break;
                }
            }
            if !matches {
                continue;
            }
            let mut segments = Vec::new();
            for f in start..end {
                let s = &samples[f];
                let value = channel(s, c);
                for frame in s.index..s.index + s.duration {
                    let next = if frame + 1 < s.index + s.duration { value } else { channel(&samples[f + 1], c) };
                    segments.push(Segment {
                        start: (frame - first) as f64 / duration,
                        end: (frame + 1 - first) as f64 / duration,
                        curve: [value, value + (next - value) / 3.0, value + 2.0 * (next - value) / 3.0, next],
                    });
                }
            }
            return Some(segments);
        }
    }
    None
}

/// The cubic of a least-squares fit, as `fitCubicSegment` and the main loop compute it.
fn least_squares(samples: &[Sample], start: usize, end: usize, c: usize, begin: f64, finish: f64, largest: Option<&mut f64>) -> [f64; 4] {
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    let (mut saa, mut sbb, mut sab, mut say, mut sby) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut largest = largest;
    for f in start..end {
        let s = &samples[f];
        let value = channel(s, c);
        if let Some(l) = largest.as_deref_mut() {
            *l = l.max((value - begin).abs() / tolerance(c, s));
        }
        for frame in s.index..s.index + s.duration {
            let t = (frame - first) as f64 / duration;
            let a = t * (1.0 - t);
            let b = a * (2.0 * t - 1.0);
            let residual = value - begin - (finish - begin) * t;
            saa += a * a;
            sbb += b * b;
            sab += a * b;
            say += a * residual;
            sby += b * residual;
        }
    }
    let det = saa * sbb - sab * sab;
    let q = (say * sbb - sby * sab) / det;
    let cu = (sby * saa - say * sab) / det;
    [begin, begin + (finish - begin + q - cu) / 3.0, finish - (finish - begin - q - cu) / 3.0, finish]
}

/// Whether the three-point cubic through the run's thirds stays within 4 tolerances near both ends.
fn cubic_candidate(samples: &[Sample], start: usize, end: usize, c: usize) -> bool {
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    let left = start + (end - start) / 3;
    let right = start + 2 * (end - start) / 3;
    let ta = (samples[left].index - first) as f64 / duration;
    let tb = (samples[right].index - first) as f64 / duration;
    let aa = ta * (1.0 - ta);
    let ab = aa * (2.0 * ta - 1.0);
    let ba = tb * (1.0 - tb);
    let bb = ba * (2.0 * tb - 1.0);
    let det = aa * bb - ab * ba;
    let begin = channel(&samples[start], c);
    let delta = channel(&samples[end], c) - begin;
    let ya = channel(&samples[left], c) - begin - delta * ta;
    let yb = channel(&samples[right], c) - begin - delta * tb;
    let q = (ya * bb - yb * ab) / det;
    let cu = (yb * aa - ya * ba) / det;
    let mut ok = true;
    for offset in 1..=4.min(end - start - 1) {
        for f in [start + offset, end - offset] {
            let t = (samples[f].index - first) as f64 / duration;
            let expected = begin + delta * t + t * (1.0 - t) * (q + cu * (2.0 * t - 1.0));
            if (channel(&samples[f], c) - expected).abs() > 4.0 * tolerance(c, &samples[f]) {
                ok = false;
            }
        }
    }
    ok
}

fn fit_cubic_segment(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<[f64; 4]> {
    let begin = channel(&samples[start], c);
    let finish = channel(&samples[end], c);
    if !cubic_candidate(samples, start, end, c) {
        return None;
    }
    let curve = least_squares(samples, start, end, c, begin, finish, None);
    matches_channel(samples, start, end, c, &curve, 0).then_some(curve)
}

fn fit_segments(samples: &[Sample], start: usize, end: usize, c: usize) -> Option<Vec<Segment>> {
    if end - start < 10 {
        return None;
    }
    let first = samples[start].index;
    let duration = (samples[end].index - first) as f64;
    let mut result = Vec::new();
    let mut current = start;
    while current < end && result.len() < 8 {
        let mut best = None;
        let mut finish = end;
        while finish >= current + 5 {
            if finish != end && end - finish < 5 {
                finish -= 1;
                continue;
            }
            best = fit_cubic_segment(samples, current, finish, c);
            if best.is_some() {
                break;
            }
            finish -= 1;
        }
        let best = best?;
        result.push(Segment { start: (samples[current].index - first) as f64 / duration, end: (samples[finish].index - first) as f64 / duration, curve: best });
        current = finish;
    }
    (current == end && result.len() > 1).then_some(result)
}

fn sampled_transform(s: &Sample) -> Option<[f64; 4]> {
    let mut r = [0.0; 4];
    for axis in 0..2 {
        let mut scale = s.values[2 + axis];
        let degrees = (s.values[4 + axis].to_degrees() * 4.0).round_ties_even() / 4.0;
        let angle = degrees.to_radians();
        let first = if axis == 0 { angle.cos() } else { -angle.sin() };
        let second = if axis == 0 { angle.sin() } else { angle.cos() };
        let snapped = (scale * 100.0).round_ties_even() / 100.0;
        let o = axis * 2;
        if (snapped - scale).abs() < 0.00001 && (snapped * first - s.matrix[o]).abs() <= MATRIX_TOLERANCE && (snapped * second - s.matrix[o + 1]).abs() <= MATRIX_TOLERANCE {
            scale = snapped;
        }
        if (scale * first - s.matrix[o]).abs() > MATRIX_TOLERANCE || (scale * second - s.matrix[o + 1]).abs() > MATRIX_TOLERANCE {
            return None;
        }
        r[axis] = scale * 100.0;
        r[2 + axis] = degrees;
    }
    Some(r)
}

fn sampled_value(s: &Sample, first: &Sample, c: usize) -> f64 {
    if c < 2 {
        return s.values[c] - first.values[c];
    }
    if c < 6 {
        return sampled_transform(s).map_or(0.0, |t| t[c - 2]);
    }
    if c < 10 {
        let percent = s.values[c] * 100.0;
        let lower = percent.floor();
        let upper = percent.ceil();
        let expected = (s.values[c] * 256.0).round_ties_even();
        let le = (java_round(lower * 256.0 / 100.0) as f64 - expected).abs();
        let ue = (java_round(upper * 256.0 / 100.0) as f64 - expected).abs();
        return (if le <= ue { lower } else { upper }) + 0.001;
    }
    s.values[c]
}

fn point(time: f64, value: f64) -> String {
    format!("{},{}", plain(time), plain(value))
}

fn constant(c: &[f64; 4]) -> bool {
    c[0] == c[1] && c[0] == c[2] && c[0] == c[3]
}

fn linear(c: &[f64; 4]) -> bool {
    let d = c[3] - c[0];
    (c[1] - c[0] - d / 3.0).abs() <= POSITION_TOLERANCE && (c[2] - c[0] - 2.0 * d / 3.0).abs() <= POSITION_TOLERANCE
}

fn property_start(w: &mut XmlWriter, id: &str, ignore: &str) {
    w.start_with("Property", &[("id", id), ("enabled", "1"), ("readonly", "0"), ("visible", "1"), ("ignoreTimeMap", ignore)]);
}

fn write_property(w: &mut XmlWriter, id: &str, curve: &[f64; 4], last_time: i32, map_index: usize) {
    property_start(w, id, if map_index == 0 { "1" } else { "0" });
    if map_index != 0 {
        w.attr("TimeMapIndex", &map_index.to_string());
    }
    let handle = if constant(curve) { 0.0 } else { last_time as f64 / 3.0 };
    w.empty_with("Keyframe", &[("timevalue", "0"), ("roving", "0"), ("anchor", &point(0.0, curve[0])), ("previous", &point(0.0, curve[0])), ("next", &point(handle, curve[1]))]);
    if !constant(curve) {
        w.empty_with("Keyframe", &[("timevalue", &last_time.to_string()), ("roving", "0"), ("anchor", &point(0.0, curve[3])), ("previous", &point(-handle, curve[2])), ("next", &point(0.0, curve[3]))]);
    }
    w.end();
}

fn write_channel(w: &mut XmlWriter, id: &str, curve: &[f64; 4], last_time: i32, map_index: usize, segments: Option<&Vec<Segment>>, scale: f64) {
    let Some(segs) = segments else {
        write_property(w, id, curve, last_time, map_index);
        return;
    };
    property_start(w, id, "1");
    for key in 0..=segs.len() {
        let prev = if key == 0 { None } else { Some(&segs[key - 1]) };
        let next = segs.get(key);
        let t = next.map_or_else(|| prev.unwrap().end, |n| n.start);
        let anchor = next.map_or_else(|| prev.unwrap().curve[3], |n| n.curve[0]) * scale;
        let prev_time = prev.map_or(0.0, |p| -(p.end - p.start) * last_time as f64 / 3.0);
        let next_time = next.map_or(0.0, |n| (n.end - n.start) * last_time as f64 / 3.0);
        let prev_value = prev.map_or(anchor, |p| p.curve[2] * scale);
        let next_value = next.map_or(anchor, |n| n.curve[1] * scale);
        w.empty_with(
            "Keyframe",
            &[("timevalue", &java_round(t * last_time as f64).to_string()), ("roving", "0"), ("anchor", &point(0.0, anchor)), ("previous", &point(prev_time, prev_value)), ("next", &point(next_time, next_value))],
        );
    }
    w.end();
}

fn write_sampled(w: &mut XmlWriter, id: &str, samples: &[Sample], start: usize, end: usize, c: usize) {
    property_start(w, id, "1");
    let begin = sampled_value(&samples[start], &samples[start], c);
    let changing = (start + 1..=end).any(|f| sampled_value(&samples[f], &samples[start], c) != begin);
    let stop = if changing { end } else { start };
    for f in start..=stop {
        let s = &samples[f];
        let last = if f == end || !changing { s.index } else { s.index + s.duration - 1 };
        for frame in s.index..=last {
            let anchor = point(0.0, sampled_value(s, &samples[start], c));
            w.empty_with("Keyframe", &[("timevalue", &((frame - samples[start].index) as i64 * 1000).to_string()), ("roving", "0"), ("anchor", &anchor), ("previous", &anchor), ("next", &anchor)]);
        }
    }
    w.end();
}

fn write_head(w: &mut XmlWriter, frame_rate: f64, last_time: i32) {
    w.start("motionObjectXML");
    w.start_with("AnimationCore", &[("TimeScale", &java_round(frame_rate * 1000.0).to_string()), ("Version", "1"), ("duration", &(last_time as i64 + 1000).to_string())]);
    w.empty_with("TimeMap", &[("strength", "0"), ("type", "Quadratic")]);
}

fn write_metadata(w: &mut XmlWriter, reg: [f64; 2]) {
    w.start("metadata");
    w.start("names");
    w.empty_with("name", &[("langID", "en_US"), ("value", "")]);
    w.end();
    w.empty_with("Settings", &[("orientToPath", "0"), ("xformPtXOffsetPct", &java_double(reg[0])), ("xformPtYOffsetPct", &java_double(reg[1])), ("xformPtZOffsetPixels", "0")]);
    w.end();
}

struct AlphaCurve {
    strength: i32,
    begin: f64,
    finish: f64,
    error: f64,
}

fn fit_alpha(samples: &[Sample], start: usize, end: usize) -> Option<AlphaCurve> {
    let first = &samples[start];
    let duration = samples[end].index - first.index;
    if duration < 9 || end - start < 7 {
        return None;
    }
    let change = samples[end].values[9] - first.values[9];
    if change.abs() < 0.1 {
        return None;
    }
    let mut offsets = Vec::new();
    let mut percentages = Vec::new();
    let mut previous = first.values[9];
    for i in start..=end {
        let s = &samples[i];
        for c in 0..first.values.len() {
            if c != 9 && (s.values.get(c).copied().unwrap_or(0.0) - first.values[c]).abs() > 1e-9 {
                return None;
            }
        }
        let value = s.values[9];
        if (value - previous) * change < 0.0 || !(0.0..=1.0).contains(&value) {
            return None;
        }
        previous = value;
        let percent = java_round(value * 100.0) as i32;
        if java_round(percent as f64 * 256.0 / 100.0) != java_round(value * 256.0) {
            return None;
        }
        let ff = s.index - first.index;
        let lf = if i == end { ff } else { ff + s.duration - 1 };
        offsets.push(ff);
        percentages.push(percent);
        if lf != ff {
            offsets.push(lf);
            percentages.push(percent);
        }
    }
    let count = offsets.len();
    let lower: Vec<f64> = percentages.iter().map(|&p| p as f64).collect();
    let upper: Vec<f64> = lower.iter().map(|&l| if l == 100.0 { 100.0 } else { l + 1.0 - 0.001 }).collect();
    let mut best: Option<AlphaCurve> = None;
    for strength in -100i32..=100 {
        if strength.abs() < 10 {
            continue;
        }
        let progress: Vec<f64> = offsets.iter().map(|&o| dual_progress(o as f64 / duration as f64, strength)).collect();
        let (mut min_slope, mut max_slope) = (-f64::MAX, f64::MAX);
        let mut feasible = true;
        'outer: for i in 0..count {
            for j in 0..count {
                let delta = progress[j] - progress[i];
                let bound = upper[j] - lower[i];
                if delta > 1e-12 {
                    max_slope = max_slope.min(bound / delta);
                } else if delta < -1e-12 {
                    min_slope = min_slope.max(bound / delta);
                } else if bound < 0.0 {
                    feasible = false;
                    break 'outer;
                }
                if min_slope > max_slope + 1e-9 {
                    feasible = false;
                    break 'outer;
                }
            }
        }
        if !feasible {
            continue;
        }
        let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
        for f in 0..count {
            let x = progress[f];
            let y = (lower[f] + upper[f]) / 2.0;
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
        }
        let n = count as f64;
        let mut slope = (sxy - sx * sy / n) / (sxx - sx * sx / n);
        slope = min_slope.max(max_slope.min(slope));
        let (mut min_i, mut max_i) = (-f64::MAX, f64::MAX);
        for f in 0..count {
            min_i = min_i.max(lower[f] - slope * progress[f]);
            max_i = max_i.min(upper[f] - slope * progress[f]);
        }
        let mut intercept = min_i.max(max_i.min((sy - slope * sx) / n));
        let rb = intercept.round_ties_even();
        let rf = (intercept + slope).round_ties_even();
        let whole = (0..count).all(|f| {
            let v = rb + (rf - rb) * progress[f];
            v >= lower[f] - 1e-8 && v <= upper[f] + 1e-8
        });
        if whole {
            intercept = rb;
            slope = rf - rb;
        }
        let mut error = 0.0;
        for f in 0..count {
            let v = intercept + slope * progress[f];
            let multiplier = java_round((v + 1e-8).floor() * 256.0 / 100.0);
            let expected = java_round(lower[f] * 256.0 / 100.0);
            if multiplier != expected {
                feasible = false;
                break;
            }
            let r = v - (lower[f] + upper[f]) / 2.0;
            error += r * r;
        }
        if feasible && best.as_ref().is_none_or(|b| error < b.error) {
            best = Some(AlphaCurve { strength, begin: intercept, finish: intercept + slope, error });
        }
    }
    best
}

fn write_alpha_motion(samples: &[Sample], start: usize, end: usize, frame_rate: f64, reg: [f64; 2], alpha: &AlphaCurve) -> String {
    let initial = &samples[start].values;
    let last_time = (samples[end].index - samples[start].index) * 1000;
    let mut w = XmlWriter::new();
    write_head(&mut w, frame_rate, last_time);
    w.empty_with("TimeMap", &[("strength", &alpha.strength.to_string()), ("type", "DualQuadratic")]);
    write_metadata(&mut w, reg);
    w.start_with("PropertyContainer", &[("id", "headContainer")]);
    w.start_with("PropertyContainer", &[("id", "Basic_Motion")]);
    write_property(&mut w, "Motion_X", &[0.0; 4], last_time, 0);
    write_property(&mut w, "Motion_Y", &[0.0; 4], last_time, 0);
    let rotation = initial[4].to_degrees();
    write_property(&mut w, "Rotation_Z", &[rotation; 4], last_time, 0);
    w.end();
    w.start_with("PropertyContainer", &[("id", "Transformation")]);
    let skew = (initial[5] - initial[4]).to_degrees();
    write_property(&mut w, "Skew_X", &[skew; 4], last_time, 0);
    write_property(&mut w, "Skew_Y", &[0.0; 4], last_time, 0);
    write_property(&mut w, "Scale_X", &[initial[2] * 100.0; 4], last_time, 0);
    write_property(&mut w, "Scale_Y", &[initial[3] * 100.0; 4], last_time, 0);
    w.end();
    w.start_with("PropertyContainer", &[("id", "Colors")]);
    w.start_with("PropertyContainer", &[("id", "Alpha_ColorXform")]);
    property_start(&mut w, "Alpha_Amount", "0");
    w.attr("TimeMapIndex", "1");
    let b = format!("0,{}", java_double(alpha.begin));
    let f = format!("0,{}", java_double(alpha.finish));
    w.empty_with("Keyframe", &[("timevalue", "0"), ("roving", "0"), ("anchor", &b), ("previous", &b), ("next", &b)]);
    w.empty_with("Keyframe", &[("timevalue", &last_time.to_string()), ("roving", "0"), ("anchor", &f), ("previous", &f), ("next", &f)]);
    w.end();
    w.end();
    w.end();
    write_filters(&mut w, samples, start, end);
    w.end();
    w.end();
    w.end();
    w.to_string()
}

/// `MotionTweenDetector.fit`: the `motionObjectXML` of a run, or None.
pub fn fit(samples: &[Sample], start: usize, end: usize, frame_rate: f64, reg: [f64; 2]) -> Option<String> {
    if end - start < 5 || !frame_rate.is_finite() || frame_rate <= 0.0 {
        return None;
    }
    let endpoint = &samples[end];
    if endpoint.duration > 1 {
        let mut expanded: Vec<Sample> = samples[..=end].to_vec();
        let mut hold = endpoint.clone();
        hold.duration -= 1;
        expanded[end] = hold;
        let mut last = endpoint.clone();
        last.index += endpoint.duration - 1;
        last.duration = 1;
        expanded.push(last);
        if let Some(full) = fit(&expanded, start, end + 1, frame_rate, reg) {
            if full.contains("type=\"Spring\"") || full.contains("type=\"Bounce\"") || full.contains("type=\"BounceIn\"") {
                return Some(full);
            }
        }
    }
    if !native_strengths(samples, start, end) {
        return None;
    }
    let sampled_tf = (start..=end).any(|f| samples[f].matrix[1] != 0.0 || samples[f].matrix[2] != 0.0);
    if sampled_tf && (start..=end).any(|f| sampled_transform(&samples[f]).is_none()) {
        return None;
    }
    if let Some(alpha) = fit_alpha(samples, start, end) {
        if !sampled_tf {
            return Some(write_alpha_motion(samples, start, end, frame_rate, reg, &alpha));
        }
    }
    let first_frame = samples[start].index;
    let frame_count = (samples[end].index - first_frame) as f64;
    let first = samples[start].values.clone();
    let n = first.len();
    let mut curves = vec![[0.0f64; 4]; n];
    let mut maps: Vec<Option<TimeMap>> = vec![None; n];
    let mut sampled_colors = [false; 4];
    let mut pieces: Vec<Option<Vec<Segment>>> = vec![None; n];
    let mut largest: f64 = 0.0;
    let candidates: Vec<bool> = (0..n).map(|c| cubic_candidate(samples, start, end, c)).collect();
    for c in 0..n {
        let begin = channel(&samples[start], c);
        let finish = channel(&samples[end], c);
        let mut is_const = true;
        for f in start..=end {
            let v = channel(&samples[f], c);
            is_const &= v == begin;
            largest = largest.max((v - begin).abs() / tolerance(c, &samples[f]));
        }
        if is_const {
            curves[c] = [begin; 4];
            continue;
        }
        let thirds = [begin, begin + (finish - begin) / 3.0, begin + 2.0 * (finish - begin) / 3.0, finish];
        if c < 14 {
            if let Some(s) = fit_quadratic(samples, start, end, c).filter(|&s| s != 0) {
                maps[c] = Some(TimeMap::new(MapType::Quadratic, s));
                curves[c] = thirds;
                continue;
            }
            if let Some(m) = fit_native_polynomial(samples, start, end, c) {
                maps[c] = Some(m);
                curves[c] = thirds;
                continue;
            }
            if let Some((m, d)) = fit_native_wave(samples, start, end, c) {
                maps[c] = Some(m);
                curves[c] = [begin, begin + d / 3.0, begin + 2.0 * d / 3.0, begin + d];
                continue;
            }
        }
        let fallback = |maps: &mut Vec<Option<TimeMap>>, pieces: &mut Vec<Option<Vec<Segment>>>| -> Option<()> {
            match fit_dual_quadratic(samples, start, end, c) {
                Some(s) => maps[c] = Some(TimeMap::new(MapType::DualQuadratic, s)),
                None => {
                    let segs = fit_preset(samples, start, end, c).or_else(|| fit_segments(samples, start, end, c))?;
                    pieces[c] = Some(segs);
                }
            }
            Some(())
        };
        if !candidates[c] {
            fallback(&mut maps, &mut pieces)?;
            curves[c] = thirds;
            continue;
        }
        curves[c] = least_squares(samples, start, end, c, begin, finish, Some(&mut largest));
        if !matches_channel(samples, start, end, c, &curves[c], 0) {
            fallback(&mut maps, &mut pieces)?;
            curves[c] = thirds;
        }
    }
    if largest < 4.0 {
        return None;
    }
    let x_keys = pieces[0].as_ref().map_or(2, |p| p.len() + 1);
    let y_keys = pieces[1].as_ref().map_or(2, |p| p.len() + 1);
    let sampled_position = x_keys == y_keys && !constant(&curves[0]) && !constant(&curves[1]) && (pieces[0].is_some() || pieces[1].is_some() || !linear(&curves[0]) || !linear(&curves[1]));
    for (f, frame) in frames(samples, start, end) {
        let s = &samples[f];
        let t = (frame - first_frame) as f64 / frame_count;
        for c in 6..n {
            let predicted = evaluate_channel(&curves[c], maps[c].as_ref(), pieces[c].as_ref(), t);
            if (s.values[c] - predicted).abs() > tolerance(c, s) {
                return None;
            }
            let changing = pieces[c].is_some() || !constant(&curves[c]);
            if c < 10 && changing && frame != first_frame && frame != samples[end].index {
                let percent = (predicted * 100.0).floor().clamp(0.0, 100.0);
                let published = java_round(percent * 256.0 / 100.0) as f64 / 256.0;
                if (s.values[c] - published).abs() > 1.0 / 256.0 {
                    sampled_colors[c - 6] = true;
                }
            }
        }
        if !matches_position(s.values[0], evaluate_channel(&curves[0], maps[0].as_ref(), pieces[0].as_ref(), t), maps[0].as_ref())
            || !matches_position(s.values[1], evaluate_channel(&curves[1], maps[1].as_ref(), pieces[1].as_ref(), t), maps[1].as_ref())
        {
            return None;
        }
        let sx = evaluate_channel(&curves[2], maps[2].as_ref(), pieces[2].as_ref(), t);
        let sy = evaluate_channel(&curves[3], maps[3].as_ref(), pieces[3].as_ref(), t);
        let rotation = evaluate_channel(&curves[4], maps[4].as_ref(), pieces[4].as_ref(), t);
        let y_rotation = rotation + evaluate_channel(&curves[5], maps[5].as_ref(), pieces[5].as_ref(), t);
        let m = [sx * rotation.cos(), sx * rotation.sin(), -sy * y_rotation.sin(), sy * y_rotation.cos()];
        if sx <= 0.0 || sy == 0.0 {
            return None;
        }
        for i in 0..4 {
            if (s.matrix[i] - m[i]).abs() > MATRIX_TOLERANCE {
                return None;
            }
        }
    }
    for i in 0..4 {
        curves[0][i] -= first[0];
        curves[1][i] -= first[1];
        curves[2][i] *= 100.0;
        curves[3][i] *= 100.0;
        curves[5][i] = curves[5][i].to_degrees();
        curves[4][i] = curves[4][i].to_degrees();
    }
    for (p, piece) in pieces.iter_mut().enumerate().take(6) {
        let Some(segs) = piece else { continue };
        for s in segs.iter_mut() {
            for v in s.curve.iter_mut() {
                if p < 2 {
                    *v -= first[p];
                } else if p < 4 {
                    *v *= 100.0;
                } else {
                    *v = v.to_degrees();
                }
            }
        }
    }
    let last_time = (samples[end].index - samples[start].index) * 1000;
    let mut w = XmlWriter::new();
    write_head(&mut w, frame_rate, last_time);
    let mut map_indices = vec![0usize; n];
    let mut time_maps: Vec<TimeMap> = Vec::new();
    for p in 0..n.min(14) {
        let Some(m) = maps[p] else { continue };
        if (p < 2 && sampled_position) || ((2..6).contains(&p) && sampled_tf) || ((6..10).contains(&p) && sampled_colors[p - 6]) {
            continue;
        }
        let index = match time_maps.iter().position(|x| *x == m) {
            Some(i) => i,
            None => {
                time_maps.push(m);
                w.empty_with("TimeMap", &[("strength", &m.strength.to_string()), ("type", m.ty.xml())]);
                time_maps.len() - 1
            }
        };
        map_indices[p] = index + 1;
    }
    write_metadata(&mut w, reg);
    w.start_with("PropertyContainer", &[("id", "headContainer")]);
    w.start_with("PropertyContainer", &[("id", "Basic_Motion")]);
    if sampled_position {
        write_sampled(&mut w, "Motion_X", samples, start, end, 0);
        write_sampled(&mut w, "Motion_Y", samples, start, end, 1);
    } else {
        write_channel(&mut w, "Motion_X", &curves[0], last_time, map_indices[0], pieces[0].as_ref(), 1.0);
        write_channel(&mut w, "Motion_Y", &curves[1], last_time, map_indices[1], pieces[1].as_ref(), 1.0);
    }
    if sampled_tf {
        write_property(&mut w, "Rotation_Z", &[0.0; 4], last_time, 0);
    } else {
        write_channel(&mut w, "Rotation_Z", &curves[4], last_time, map_indices[4], pieces[4].as_ref(), 1.0);
    }
    w.end();
    w.start_with("PropertyContainer", &[("id", "Transformation")]);
    if sampled_tf {
        write_sampled(&mut w, "Skew_X", samples, start, end, 5);
        write_sampled(&mut w, "Scale_X", samples, start, end, 2);
        write_sampled(&mut w, "Scale_Y", samples, start, end, 3);
        write_sampled(&mut w, "Skew_Y", samples, start, end, 4);
    } else {
        write_channel(&mut w, "Skew_X", &curves[5], last_time, map_indices[5], pieces[5].as_ref(), 1.0);
        write_channel(&mut w, "Scale_X", &curves[2], last_time, map_indices[2], pieces[2].as_ref(), 1.0);
        write_channel(&mut w, "Scale_Y", &curves[3], last_time, map_indices[3], pieces[3].as_ref(), 1.0);
        write_property(&mut w, "Skew_Y", &[0.0; 4], last_time, 0);
    }
    w.end();
    w.start_with("PropertyContainer", &[("id", "Colors")]);
    w.start_with("PropertyContainer", &[("id", "Advanced_ColorXform")]);
    for (p, name) in COLOR_PROPERTIES.iter().enumerate() {
        let mut curve = curves[p + 6];
        if p < 4 {
            for v in curve.iter_mut() {
                *v *= 100.0;
            }
        }
        if p < 4 && sampled_colors[p] {
            write_sampled(&mut w, name, samples, start, end, p + 6);
        } else {
            write_channel(&mut w, name, &curve, last_time, map_indices[p + 6], pieces[p + 6].as_ref(), if p < 4 { 100.0 } else { 1.0 });
        }
    }
    w.end();
    w.end();
    write_filters(&mut w, samples, start, end);
    w.end();
    w.end();
    w.end();
    Some(w.to_string())
}
