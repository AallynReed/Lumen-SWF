//! Port of afe.jar `com.adobe.fontengine.font.cff.Dict`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_byte_array::{CFFByteArray, CFFByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::string_index::{index_of, StringIndex};
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::orig_font_type::OrigFontType;
use crate::asc::flex2::fonts::adobe::fontengine::font::postscript::postscript_token_parser;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::ros::ROS;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};
use crate::xfl::num::java_double;

static FLOAT_NIBBLES: [&str; 15] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "E", "E-", "", "-"];

/// The map is a `LinkedHashMap`: insertion order, a re-put keeps its place.
#[derive(Clone, Debug, Default)]
pub struct Dict {
    pub m: Vec<(Key, Value)>,
}

impl Dict {
    fn put(&mut self, k: Key, v: Value) {
        if let Some(e) = self.m.iter_mut().find(|e| e.0 == k) {
            e.1 = v;
        } else {
            self.m.push((k, v));
        }
    }

    fn lookup(&self, k: &Key) -> Option<&Value> {
        self.m.iter().find(|e| e.0 == *k).map(|e| &e.1)
    }

    pub fn parse(data: &CFFByteArray, n: i32, n2: i32, string_index: Option<&StringIndex>) -> FontResult<Dict> {
        let mut d = Dict::default();
        let mut vs = ValueStack::new();
        let mut n3 = n;
        let n4 = n + n2;
        while n3 < n4 {
            let mut n5 = data.getcard8(n3)?;
            n3 += 1;
            match n5 {
                12 => {
                    n5 = data.getcard8(n3)?;
                    n3 += 1;
                    if let Some(k) = Key::extended_key(n5) {
                        let v = k.parse(&mut vs, string_index)?;
                        d.put(k, v);
                    } else {
                        vs.count = 0;
                    }
                    continue;
                }
                28 => {
                    vs.push_int(data.getint16(n3)?)?;
                    n3 += 2;
                    continue;
                }
                29 => {
                    vs.push_int(data.getint32(n3)?)?;
                    n3 += 4;
                    continue;
                }
                30 => {
                    let mut sb = String::new();
                    let mut bl = false;
                    while !bl {
                        let n6 = data.getcard8(n3)?;
                        n3 += 1;
                        if n6 == 255 {
                            bl = true;
                            continue;
                        }
                        if n6 & 0xf == 15 {
                            sb.push_str(FLOAT_NIBBLES[(n6 >> 4) as usize]);
                            bl = true;
                            continue;
                        }
                        // A 0xf high nibble indexes past the table in Java.
                        assert!(n6 >> 4 < 15, "java.lang.ArrayIndexOutOfBoundsException: Index 15 out of bounds for length 15");
                        sb.push_str(FLOAT_NIBBLES[(n6 >> 4) as usize]);
                        sb.push_str(FLOAT_NIBBLES[(n6 & 0xf) as usize]);
                    }
                    vs.push_double(sb)?;
                    continue;
                }
                _ => {}
            }
            if let Some(k) = Key::non_extended_key(n5) {
                let v = k.parse(&mut vs, string_index)?;
                d.put(k, v);
                continue;
            }
            if (32..=246).contains(&n5) {
                vs.push_int(n5 - 139)?;
                continue;
            }
            if (247..=250).contains(&n5) {
                let n7 = data.getcard8(n3)?;
                n3 += 1;
                vs.push_int((n5 - 247) * 256 + n7 + 108)?;
                continue;
            }
            if (241..=254).contains(&n5) {
                let n8 = data.getcard8(n3)?;
                n3 += 1;
                vs.push_int(-(n5 - 251) * 256 - n8 - 108)?;
                continue;
            }
            vs.count = 0;
        }
        if vs.count != 0 {
            return Err(FontEngineException::invalid_font("CFF/Dict stack not empty"));
        }
        Ok(d)
    }

    /// `new Dict(Map)` over a `LinkedHashMap` built by the caller.
    pub fn from_map(map: Vec<(Key, Value)>) -> FontResult<Dict> {
        let mut d = Dict::default();
        for (k, v) in map {
            if !k.compatible_value(&v) {
                return Err(FontEngineException::invalid_font(format!("Invalid value for {}", k.name)));
            }
            d.put(k, v);
        }
        Ok(d)
    }

    fn delta_encode(a: &[f64]) -> Vec<f64> {
        let mut r = vec![0.0; a.len()];
        let mut d = 0.0;
        for i in 0..r.len() {
            r[i] = a[i] - d;
            d = a[i];
        }
        r
    }

    /// The 17-argument private DICT constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn new_private(
        blue_values: Option<Vec<f64>>,
        other_blues: Option<Vec<f64>>,
        family_blues: Option<Vec<f64>>,
        family_other_blues: Option<Vec<f64>>,
        blue_scale: Option<f64>,
        blue_shift: Option<f64>,
        blue_fuzz: Option<i32>,
        std_hw: Option<f64>,
        std_vw: Option<f64>,
        stem_snap_h: Option<Vec<f64>>,
        stem_snap_v: Option<Vec<f64>>,
        force_bold: Option<bool>,
        language_group: Option<i32>,
        expansion_factor: Option<f64>,
        initial_random_seed: Option<i32>,
        nominal_width_x: Option<f64>,
        default_width_x: Option<f64>,
    ) -> Dict {
        let mut d = Dict::default();
        if let Some(d6) = nominal_width_x {
            if d6 != 0.0 {
                d.put(Key::NOMINAL_WIDTH_X, Value::Numbers(NumbersValue::from_double(d6)));
                if let Some(d7) = default_width_x {
                    if d7 != 0.0 {
                        d.put(Key::DEFAULT_WIDTH_X, Value::Numbers(NumbersValue::from_double(d7)));
                    }
                }
            }
        }
        if let Some(a) = &blue_values {
            d.put(Key::BLUE_VALUES, Value::Numbers(NumbersValue::from_doubles(&Self::delta_encode(a))));
        }
        if force_bold == Some(true) {
            d.put(Key::FORCE_BOLD, Value::Numbers(NumbersValue::from_int(1)));
        }
        if let Some(n2) = language_group {
            if n2 as f64 != 0.0 {
                d.put(Key::LANGUAGE_GROUP, Value::Numbers(NumbersValue::from_int(n2)));
            }
        }
        if let Some(d5) = expansion_factor {
            if d5 != 0.06 {
                d.put(Key::EXPANSION_FACTOR, Value::Numbers(NumbersValue::from_double(d5)));
            }
        }
        if let Some(n3) = initial_random_seed {
            if n3 as f64 != 0.0 {
                d.put(Key::INITIAL_RANDOM_SEED, Value::Numbers(NumbersValue::from_int(n3)));
            }
        }
        if let Some(a) = &other_blues {
            d.put(Key::OTHER_BLUES, Value::Numbers(NumbersValue::from_doubles(&Self::delta_encode(a))));
        }
        if let Some(a) = &family_blues {
            d.put(Key::FAMILY_BLUES, Value::Numbers(NumbersValue::from_doubles(&Self::delta_encode(a))));
        }
        if let Some(a) = &family_other_blues {
            d.put(Key::FAMILY_OTHER_BLUES, Value::Numbers(NumbersValue::from_doubles(&Self::delta_encode(a))));
        }
        if let Some(v) = blue_scale {
            d.put(Key::BLUE_SCALE, Value::Numbers(NumbersValue::from_double(v)));
        }
        if let Some(v) = blue_shift {
            if v != 7.0 {
                d.put(Key::BLUE_SHIFT, Value::Numbers(NumbersValue::from_double(v)));
            }
        }
        if let Some(n) = blue_fuzz {
            if n as f64 != 1.0 {
                d.put(Key::BLUE_FUZZ, Value::Numbers(NumbersValue::from_int(n)));
            }
        }
        if let Some(v) = std_hw {
            d.put(Key::STD_HW, Value::Numbers(NumbersValue::from_double(v)));
        }
        if let Some(v) = std_vw {
            d.put(Key::STD_VW, Value::Numbers(NumbersValue::from_double(v)));
        }
        if let Some(a) = &stem_snap_h {
            d.put(Key::STEM_SNAP_H, Value::Numbers(NumbersValue::from_doubles(&Self::delta_encode(a))));
        }
        if let Some(a) = &stem_snap_v {
            d.put(Key::STEM_SNAP_V, Value::Numbers(NumbersValue::from_doubles(&Self::delta_encode(a))));
        }
        d
    }

    /// `new Dict(Matrix)`.
    pub fn new_font_matrix(m: Matrix) -> Dict {
        let mut d = Dict::default();
        d.put(Key::FONT_MATRIX, Value::Numbers(NumbersValue::from_matrix(&m)));
        d
    }

    /// The 24-argument top DICT constructor (`isFixedPitch` is ignored, as in Java).
    #[allow(clippy::too_many_arguments)]
    pub fn new_top(
        registry: Option<String>,
        ordering: Option<String>,
        supplement: i32,
        version: Option<String>,
        notice: Option<String>,
        copyright: Option<String>,
        full_name: Option<String>,
        font_name: Option<String>,
        family_name: Option<String>,
        weight: Option<String>,
        _is_fixed_pitch: Option<bool>,
        italic_angle: Option<f64>,
        underline_position: Option<f64>,
        underline_thickness: Option<f64>,
        paint_type: Option<i32>,
        unique_id: Option<i32>,
        font_bbox: Option<Rect>,
        stroke_width: Option<f64>,
        xuid: Option<Vec<i32>>,
        postscript: Option<String>,
        fs_type: Option<i32>,
        base_font_blend: Option<Vec<i32>>,
        base_font_name: Option<String>,
        font_matrix: Option<Matrix>,
    ) -> FontResult<Dict> {
        let mut d = Dict::default();
        if let (Some(r), Some(o)) = (&registry, &ordering) {
            d.put(Key::ROS, Value::Ros(ROSValue::new(r.clone(), o.clone(), supplement)));
        }
        if let Some(s) = font_name {
            d.put(Key::FONT_NAME, Value::String(StringValue::new(s)));
        }
        if let Some(n4) = fs_type {
            let s12 = format!("/FSType {} def", n4);
            if let Some(s10) = &postscript {
                d.put(Key::POST_SCRIPT, Value::String(StringValue::new(format!("{} {}", s10, s12))));
            } else {
                d.put(Key::POST_SCRIPT, Value::String(StringValue::new(s12)));
            }
        } else if let Some(s10) = &postscript {
            d.put(Key::POST_SCRIPT, Value::String(StringValue::new(s10.clone())));
        }
        if let Some(s) = version {
            d.put(Key::VERSION, Value::String(StringValue::new(s)));
        }
        if let Some(s) = notice {
            d.put(Key::NOTICE, Value::String(StringValue::new(s)));
        }
        if let Some(s) = copyright {
            d.put(Key::COPYRIGHT, Value::String(StringValue::new(s)));
        }
        if let Some(a) = &xuid {
            d.put(Key::XUID, Value::Numbers(NumbersValue::from_ints(a)));
        }
        if let Some(a) = &base_font_blend {
            d.put(Key::BASE_FONT_BLEND, Value::Numbers(NumbersValue::from_ints(a)));
        }
        if let Some(r) = &font_bbox {
            d.put(Key::FONT_BBOX, Value::Numbers(NumbersValue::from_rect(r)));
        }
        if let Some(s) = full_name {
            d.put(Key::FULL_NAME, Value::String(StringValue::new(s)));
        }
        if let Some(s) = family_name {
            d.put(Key::FAMILY_NAME, Value::String(StringValue::new(s)));
        }
        if let Some(s) = weight {
            d.put(Key::WEIGHT, Value::String(StringValue::new(s)));
        }
        if let Some(v) = italic_angle {
            if v != 0.0 {
                d.put(Key::ITALIC_ANGLE, Value::Numbers(NumbersValue::from_double(v)));
            }
        }
        if let Some(v) = underline_position {
            if v != -100.0 {
                d.put(Key::UNDERLINE_POSITION, Value::Numbers(NumbersValue::from_double(v)));
            }
        }
        if let Some(v) = underline_thickness {
            if v != 50.0 {
                d.put(Key::UNDERLINE_THICKNESS, Value::Numbers(NumbersValue::from_double(v)));
            }
        }
        if let Some(n) = paint_type {
            if n as f64 != 0.0 {
                d.put(Key::PAINT_TYPE, Value::Numbers(NumbersValue::from_int(n)));
            }
        }
        if let Some(n) = unique_id {
            d.put(Key::UNIQUE_ID, Value::Numbers(NumbersValue::from_int(n)));
        }
        if let Some(v) = stroke_width {
            if v != 0.0 {
                d.put(Key::STROKE_WIDTH, Value::Numbers(NumbersValue::from_double(v)));
            }
        }
        if let Some(s) = base_font_name {
            d.put(Key::BASE_FONT_NAME, Value::String(StringValue::new(s)));
        }
        if let Some(m) = &font_matrix {
            d.put(Key::FONT_MATRIX, Value::Numbers(NumbersValue::from_matrix(m)));
        }
        Ok(d)
    }

    pub fn get_orig_font_type(&self) -> Option<OrigFontType> {
        let sv = self.get_string(&Key::POST_SCRIPT, true)?;
        postscript_token_parser::get_orig_font_type(Some(&sv.value))
    }

    pub fn get_fs_type(&self) -> Option<i32> {
        let sv = self.get_string(&Key::POST_SCRIPT, true)?;
        postscript_token_parser::get_fs_type(Some(&sv.value))
    }

    pub fn get_ros(&self) -> Option<ROS> {
        self.get_ros_value(&Key::ROS, true).map(|v| v.ros)
    }

    fn get_value(&self, k: &Key, bl: bool) -> Option<Value> {
        match self.lookup(k) {
            Some(v) => Some(v.clone()),
            None if bl => default_dict(k),
            None => None,
        }
    }

    pub fn get_integer(&self, k: &Key, bl: bool) -> Option<IntegerValue> {
        match self.get_value(k, bl)? {
            Value::Integer(v) => Some(v),
            _ => panic!("java.lang.ClassCastException: {} is not an IntegerValue", k.name),
        }
    }

    pub fn get_string(&self, k: &Key, bl: bool) -> Option<StringValue> {
        match self.get_value(k, bl)? {
            Value::String(v) => Some(v),
            _ => panic!("java.lang.ClassCastException: {} is not a StringValue", k.name),
        }
    }

    pub fn get_numbers(&self, k: &Key, bl: bool) -> Option<NumbersValue> {
        match self.get_value(k, bl)? {
            Value::Numbers(v) => Some(v),
            _ => panic!("java.lang.ClassCastException: {} is not a NumbersValue", k.name),
        }
    }

    pub fn get_ros_value(&self, k: &Key, bl: bool) -> Option<ROSValue> {
        match self.get_value(k, bl)? {
            Value::Ros(v) => Some(v),
            _ => panic!("java.lang.ClassCastException: {} is not a ROSValue", k.name),
        }
    }

    pub fn get_offset(&self, k: &Key, bl: bool) -> Option<OffsetValue> {
        match self.get_value(k, bl)? {
            Value::Offset(v) => Some(v),
            _ => panic!("java.lang.ClassCastException: {} is not an OffsetValue", k.name),
        }
    }

    pub fn get_offset_size(&self, k: &Key, bl: bool) -> Option<OffsetSizeValue> {
        match self.get_value(k, bl)? {
            Value::OffsetSize(v) => Some(v),
            _ => panic!("java.lang.ClassCastException: {} is not an OffsetSizeValue", k.name),
        }
    }

    pub fn collect_strings(&self, list: &mut Vec<String>) {
        for (_, v) in &self.m {
            v.collect_strings(list);
        }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder, list: &[String], keys: &[Key]) -> FontResult<()> {
        for k in keys {
            if let Some(v) = self.lookup(k) {
                v.stream(b, list)?;
                k.stream(b);
            }
        }
        Ok(())
    }

    pub fn stream_value(&self, b: &mut CFFByteArrayBuilder, list: &[String], k: &Key) -> FontResult<()> {
        if let Some(v) = self.lookup(k) {
            v.stream(b, list)?;
            k.stream(b);
        }
        Ok(())
    }

    /// `streamKeyVal(builder, IntegerKey, int)`.
    pub fn stream_key_val_int(b: &mut CFFByteArrayBuilder, k: &Key, n: i32) {
        Dict::stream_int(b, n);
        k.stream(b);
    }

    /// `streamKeyVal(builder, StringKey, String, List)`.
    pub fn stream_key_val_string(b: &mut CFFByteArrayBuilder, k: &Key, s: &str, list: &[String]) {
        Dict::stream_int(b, index_of(list, s));
        k.stream(b);
    }

    pub fn stream_int(b: &mut CFFByteArrayBuilder, n: i32) {
        if (-107..=107).contains(&n) {
            b.add_card8(n + 139);
        } else if (108..=1131).contains(&n) {
            b.add_card8((n - 108) / 256 + 247);
            b.add_card8((n - 108) % 256);
        } else if (-1131..=-108).contains(&n) {
            b.add_card8((-108 - n) / 256 + 251);
            b.add_card8((-108 - n) % 256);
        } else if (i16::MIN as i32..=i16::MAX as i32).contains(&n) {
            b.add_card8(28);
            b.add_card16(n);
        } else {
            b.add_card8(29);
            b.add_card32(n);
        }
    }

    /// `streamDouble(builder, Double)`.
    pub fn stream_double(b: &mut CFFByteArrayBuilder, d: f64) -> FontResult<()> {
        let n = d as i32;
        if n as f64 == d {
            Dict::stream_int(b, n);
            return Ok(());
        }
        Dict::stream_double_string(b, &java_double(d))
    }

    /// `streamDouble(builder, String)`.
    pub fn stream_double_string(b: &mut CFFByteArrayBuilder, s: &str) -> FontResult<()> {
        b.add_card8(30);
        let mut n: i32 = 0;
        let mut bl = true;
        let cs: Vec<u16> = s.encode_utf16().collect();
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            let sh = if bl { 4 } else { 0 };
            let nib = match c {
                0x30..=0x39 => (c - 0x30) as i32,
                0x2e => 10,
                0x45 => {
                    let mut n2 = 11;
                    if i < cs.len() - 1 && cs[i + 1] == '-' as u16 {
                        n2 = 12;
                        i += 1;
                    }
                    n2
                }
                0x2d => 14,
                _ => return Err(FontEngineException::invalid_font("CFF: invalid float number")),
            };
            n = (n | nib << sh) as i8 as i32;
            if !bl {
                b.add_card8(n);
                n = 0;
            }
            bl = !bl;
            i += 1;
        }
        if bl {
            b.add_card8(255);
        } else {
            b.add_card8(n | 0xf);
        }
        Ok(())
    }
}

fn default_dict(k: &Key) -> Option<Value> {
    let num = |n: i32| Some(Value::Numbers(NumbersValue::from_int(n)));
    let strs = |a: &[&str]| Some(Value::Numbers(NumbersValue { values: a.iter().map(|s| DictNum::String(s.to_string())).collect() }));
    match k.name {
        "isFixedPitch" | "ItalicAngle" | "StrokeWidth" | "ForceBold" | "LanguageGroup" | "initialRandomSeed" | "defaultWidthX" | "nominalWidthX" | "CIDFontVersion" | "CIDFontRevision" | "CIDFontType"
        | "ForceBoldThreshold" => num(0),
        "UnderlinePosition" => num(-100),
        "UnderlineThickness" => num(50),
        "PainType" | "CharstringType" => num(2),
        "FontMatrix" => strs(&["0.001", "0", "0", "0.001", "0", "0"]),
        "FontBBox" => strs(&["0", "0", "0", "0"]),
        "charset" | "Encoding" => Some(Value::Offset(OffsetValue { offset: 0 })),
        "BlueScale" => Some(Value::Numbers(NumbersValue::from_string("0.039625".to_string()))),
        "BlueShift" => num(7),
        "BlueFuzz" => num(1),
        "ExpansionFactor" => Some(Value::Numbers(NumbersValue::from_string("0.06".to_string()))),
        // `defaultDict.put(Key.CIDCount, new NumbersValue(8720))` stores a NumbersValue under an IntegerKey.
        "CIDCount" => num(8720),
        _ => None,
    }
}

/// `Dict.ValueStack`: `Integer`s and the strings of real operands.
#[derive(Clone, Debug)]
pub struct ValueStack {
    pub count: i32,
    pub values: Vec<Option<DictNum>>,
}

impl ValueStack {
    fn new() -> ValueStack {
        ValueStack { count: 0, values: vec![None; 48] }
    }

    fn push_int(&mut self, n: i32) -> FontResult<()> {
        if self.count == self.values.len() as i32 {
            return Err(FontEngineException::invalid_font("CFF Dict stack overflow"));
        }
        self.values[self.count as usize] = Some(DictNum::Integer(n));
        self.count += 1;
        Ok(())
    }

    fn push_double(&mut self, s: String) -> FontResult<()> {
        if self.count == self.values.len() as i32 {
            return Err(FontEngineException::invalid_font("CFF Dict stack overflow"));
        }
        self.values[self.count as usize] = Some(DictNum::String(s));
        self.count += 1;
        Ok(())
    }

    fn pop_int(&mut self) -> FontResult<i32> {
        if self.count == 0 {
            return Err(FontEngineException::invalid_font("empty CFF Dict stack"));
        }
        self.count -= 1;
        match self.values[self.count as usize].clone() {
            Some(DictNum::Integer(n)) => Ok(n),
            Some(DictNum::String(s)) => match parse_double(&s) {
                Some(d) => Ok(d as i32),
                None => Err(FontEngineException::invalid_font("Invalid number on stack")),
            },
            other => panic!("ValueStack.popInt on {:?}", other),
        }
    }

    fn pop_value(&mut self) -> FontResult<Option<DictNum>> {
        if self.count == 0 {
            return Err(FontEngineException::invalid_font("empty CFF Dict statck"));
        }
        self.count -= 1;
        Ok(self.values[self.count as usize].clone())
    }
}

/// `Double.parseDouble` on the strings a DICT real can produce.
pub fn parse_double(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let t = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    t.parse::<f64>().ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    String,
    Ros,
    OffsetSize,
    Offset,
    Numbers(i32),
    Integer,
}

/// `Dict.Key` and its subclasses; equality is name and opcodes, as in Java.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub name: &'static str,
    pub opcode1: i32,
    pub opcode2: i32,
    pub kind: KeyKind,
}

impl PartialEq for Key {
    fn eq(&self, o: &Key) -> bool {
        self.name == o.name && self.opcode1 == o.opcode1 && self.opcode2 == o.opcode2
    }
}

const fn key(n: i32, name: &'static str, kind: KeyKind) -> Key {
    Key { name, opcode1: n, opcode2: -1, kind }
}

const fn ekey(n: i32, n2: i32, name: &'static str, kind: KeyKind) -> Key {
    Key { name, opcode1: n, opcode2: n2, kind }
}

impl Key {
    pub const VERSION: Key = key(0, "version", KeyKind::String);
    pub const NOTICE: Key = key(1, "Notice", KeyKind::String);
    pub const FULL_NAME: Key = key(2, "FullName", KeyKind::String);
    pub const FAMILY_NAME: Key = key(3, "FamilyName", KeyKind::String);
    pub const WEIGHT: Key = key(4, "Weight", KeyKind::String);
    pub const FONT_BBOX: Key = key(5, "FontBBox", KeyKind::Numbers(4));
    pub const BLUE_VALUES: Key = key(6, "BlueValues", KeyKind::Numbers(-1));
    pub const OTHER_BLUES: Key = key(7, "OtherBlues", KeyKind::Numbers(-1));
    pub const FAMILY_BLUES: Key = key(8, "FamilyBlues", KeyKind::Numbers(-1));
    pub const FAMILY_OTHER_BLUES: Key = key(9, "FamilyOtherBlues", KeyKind::Numbers(-1));
    pub const STD_HW: Key = key(10, "StdHW", KeyKind::Numbers(1));
    pub const STD_VW: Key = key(11, "StdVW", KeyKind::Numbers(1));
    pub const UNIQUE_ID: Key = key(13, "UniqueID", KeyKind::Numbers(1));
    pub const XUID: Key = key(14, "XUID", KeyKind::Numbers(-1));
    pub const CHARSET: Key = key(15, "charset", KeyKind::Offset);
    pub const ENCODING: Key = key(16, "Encoding", KeyKind::Offset);
    pub const CHAR_STRINGS: Key = key(17, "CharStrings", KeyKind::Offset);
    pub const PRIVATE: Key = key(18, "Private", KeyKind::OffsetSize);
    pub const SUBRS: Key = key(19, "Subrs", KeyKind::Offset);
    pub const DEFAULT_WIDTH_X: Key = key(20, "defaultWidthX", KeyKind::Numbers(1));
    pub const NOMINAL_WIDTH_X: Key = key(21, "nominalWidthX", KeyKind::Numbers(1));
    pub const COPYRIGHT: Key = ekey(12, 0, "Copyright", KeyKind::String);
    pub const IS_FIXED_PITCH: Key = ekey(12, 1, "isFixedPitch", KeyKind::Numbers(1));
    pub const ITALIC_ANGLE: Key = ekey(12, 2, "ItalicAngle", KeyKind::Numbers(1));
    pub const UNDERLINE_POSITION: Key = ekey(12, 3, "UnderlinePosition", KeyKind::Numbers(1));
    pub const UNDERLINE_THICKNESS: Key = ekey(12, 4, "UnderlineThickness", KeyKind::Numbers(1));
    pub const PAINT_TYPE: Key = ekey(12, 5, "PainType", KeyKind::Numbers(1));
    pub const CHARSTRING_TYPE: Key = ekey(12, 6, "CharstringType", KeyKind::Numbers(1));
    pub const FONT_MATRIX: Key = ekey(12, 7, "FontMatrix", KeyKind::Numbers(6));
    pub const STROKE_WIDTH: Key = ekey(12, 8, "StrokeWidth", KeyKind::Numbers(1));
    pub const BLUE_SCALE: Key = ekey(12, 9, "BlueScale", KeyKind::Numbers(1));
    pub const BLUE_SHIFT: Key = ekey(12, 10, "BlueShift", KeyKind::Numbers(1));
    pub const BLUE_FUZZ: Key = ekey(12, 11, "BlueFuzz", KeyKind::Numbers(1));
    pub const STEM_SNAP_H: Key = ekey(12, 12, "StemSnapH", KeyKind::Numbers(-1));
    pub const STEM_SNAP_V: Key = ekey(12, 13, "StemSnapV", KeyKind::Numbers(-1));
    pub const FORCE_BOLD: Key = ekey(12, 14, "ForceBold", KeyKind::Numbers(1));
    pub const FORCE_BOLD_THRESHOLD: Key = ekey(12, 15, "ForceBoldThreshold", KeyKind::Numbers(1));
    pub const LANGUAGE_GROUP: Key = ekey(12, 17, "LanguageGroup", KeyKind::Numbers(1));
    pub const EXPANSION_FACTOR: Key = ekey(12, 18, "ExpansionFactor", KeyKind::Numbers(1));
    pub const INITIAL_RANDOM_SEED: Key = ekey(12, 19, "initialRandomSeed", KeyKind::Numbers(1));
    pub const SYNTHETIC_BASE: Key = ekey(12, 20, "SyntheticBase", KeyKind::Integer);
    pub const POST_SCRIPT: Key = ekey(12, 21, "PostScript", KeyKind::String);
    pub const BASE_FONT_NAME: Key = ekey(12, 22, "BaseFontName", KeyKind::String);
    pub const BASE_FONT_BLEND: Key = ekey(12, 23, "BaseFontBlend", KeyKind::Numbers(-1));
    pub const ROS: Key = ekey(12, 30, "ROS", KeyKind::Ros);
    pub const CID_FONT_VERSION: Key = ekey(12, 31, "CIDFontVersion", KeyKind::Numbers(1));
    pub const CID_FONT_REVISION: Key = ekey(12, 32, "CIDFontRevision", KeyKind::Numbers(1));
    pub const CID_FONT_TYPE: Key = ekey(12, 33, "CIDFontType", KeyKind::Numbers(1));
    pub const CID_COUNT: Key = ekey(12, 34, "CIDCount", KeyKind::Integer);
    pub const UID_BASE: Key = ekey(12, 35, "UIDBase", KeyKind::Numbers(1));
    pub const FD_ARRAY: Key = ekey(12, 36, "FDArray", KeyKind::Offset);
    pub const FD_SELECT: Key = ekey(12, 37, "FDSelect", KeyKind::Offset);
    pub const FONT_NAME: Key = ekey(12, 38, "FontName", KeyKind::String);

    const NON_EXTENDED: [Key; 21] = [
        Key::VERSION,
        Key::NOTICE,
        Key::FULL_NAME,
        Key::FAMILY_NAME,
        Key::WEIGHT,
        Key::FONT_BBOX,
        Key::BLUE_VALUES,
        Key::OTHER_BLUES,
        Key::FAMILY_BLUES,
        Key::FAMILY_OTHER_BLUES,
        Key::STD_HW,
        Key::STD_VW,
        Key::UNIQUE_ID,
        Key::XUID,
        Key::CHARSET,
        Key::ENCODING,
        Key::CHAR_STRINGS,
        Key::PRIVATE,
        Key::SUBRS,
        Key::DEFAULT_WIDTH_X,
        Key::NOMINAL_WIDTH_X,
    ];

    const EXTENDED: [Key; 32] = [
        Key::COPYRIGHT,
        Key::IS_FIXED_PITCH,
        Key::ITALIC_ANGLE,
        Key::UNDERLINE_POSITION,
        Key::UNDERLINE_THICKNESS,
        Key::PAINT_TYPE,
        Key::CHARSTRING_TYPE,
        Key::FONT_MATRIX,
        Key::STROKE_WIDTH,
        Key::BLUE_SCALE,
        Key::BLUE_SHIFT,
        Key::BLUE_FUZZ,
        Key::STEM_SNAP_H,
        Key::STEM_SNAP_V,
        Key::FORCE_BOLD,
        Key::FORCE_BOLD_THRESHOLD,
        Key::LANGUAGE_GROUP,
        Key::EXPANSION_FACTOR,
        Key::INITIAL_RANDOM_SEED,
        Key::SYNTHETIC_BASE,
        Key::POST_SCRIPT,
        Key::BASE_FONT_NAME,
        Key::BASE_FONT_BLEND,
        Key::ROS,
        Key::CID_FONT_VERSION,
        Key::CID_FONT_REVISION,
        Key::CID_FONT_TYPE,
        Key::CID_COUNT,
        Key::UID_BASE,
        Key::FD_ARRAY,
        Key::FD_SELECT,
        Key::FONT_NAME,
    ];

    /// `nonExtendedKeys[n]` (27 slots).
    pub fn non_extended_key(n: i32) -> Option<Key> {
        if !(0..27).contains(&n) {
            return None;
        }
        Key::NON_EXTENDED.iter().find(|k| k.opcode1 == n).copied()
    }

    /// `extendedKeys[n]` (39 slots).
    pub fn extended_key(n: i32) -> Option<Key> {
        if !(0..39).contains(&n) {
            return None;
        }
        Key::EXTENDED.iter().find(|k| k.opcode2 == n).copied()
    }

    pub fn op_code_length(&self) -> i32 {
        if self.opcode2 == -1 {
            1
        } else {
            2
        }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder) {
        b.add_card8(self.opcode1);
        if self.opcode2 != -1 {
            b.add_card8(self.opcode2);
        }
    }

    pub fn parse(&self, vs: &mut ValueStack, si: Option<&StringIndex>) -> FontResult<Value> {
        Ok(match self.kind {
            KeyKind::String => Value::String(StringValue { value: si.expect("StringIndex").get_string(vs.pop_int()?)?.unwrap_or_default() }),
            KeyKind::Ros => Value::Ros(ROSValue::parse(vs, si.expect("StringIndex"))?),
            KeyKind::OffsetSize => {
                let offset = vs.pop_int()?;
                let size = vs.pop_int()?;
                Value::OffsetSize(OffsetSizeValue { size, offset })
            }
            KeyKind::Offset => Value::Offset(OffsetValue { offset: vs.pop_int()? }),
            KeyKind::Numbers(depth) => Value::Numbers(NumbersValue::parse(vs, depth)),
            KeyKind::Integer => Value::Integer(IntegerValue { value: vs.pop_int()? }),
        })
    }

    pub fn compatible_value(&self, v: &Value) -> bool {
        match (self.kind, v) {
            (KeyKind::String, Value::String(_)) => true,
            (KeyKind::Ros, Value::Ros(_)) => true,
            (KeyKind::OffsetSize, Value::OffsetSize(_)) => true,
            (KeyKind::Offset, Value::Offset(_)) => true,
            (KeyKind::Integer, Value::Integer(_)) => true,
            (KeyKind::Numbers(d), Value::Numbers(nv)) => d == -1 || nv.values.len() as i32 == d,
            _ => false,
        }
    }

    /// `OffsetKey.streamDummyValue` / `OffsetSizeKey.streamDummyValue`.
    pub fn stream_dummy_value(&self, b: &mut CFFByteArrayBuilder) -> i32 {
        let n = b.get_size();
        b.add_card8(29);
        b.add_card32(0);
        if self.kind == KeyKind::OffsetSize {
            b.add_card8(29);
            b.add_card32(0);
        }
        self.stream(b);
        n
    }

    pub fn fix_offset(&self, b: &mut CFFByteArrayBuilder, n: i32, v: i32) {
        if self.kind == KeyKind::OffsetSize {
            b.set_card32(n + 6, v);
        } else {
            b.set_card32(n + 1, v);
        }
    }

    /// `OffsetSizeKey.fixSize`.
    pub fn fix_size(&self, b: &mut CFFByteArrayBuilder, n: i32, v: i32) {
        b.set_card32(n + 1, v);
    }
}

/// An element of `NumbersValue.values`: `Integer`, `String` or `Double`; `Null` is the slot a short stack leaves.
#[derive(Clone, Debug, PartialEq)]
pub enum DictNum {
    Integer(i32),
    String(String),
    Double(f64),
    Null,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Integer(IntegerValue),
    Ros(ROSValue),
    OffsetSize(OffsetSizeValue),
    Offset(OffsetValue),
    Numbers(NumbersValue),
    String(StringValue),
}

impl Value {
    pub fn collect_strings(&self, list: &mut Vec<String>) {
        match self {
            Value::Ros(v) => v.collect_strings(list),
            Value::String(v) => v.collect_strings(list),
            _ => {}
        }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder, list: &[String]) -> FontResult<()> {
        match self {
            Value::Integer(v) => v.stream(b),
            Value::Ros(v) => v.stream(b, list),
            Value::OffsetSize(_) | Value::Offset(_) => {}
            Value::Numbers(v) => v.stream(b)?,
            Value::String(v) => v.stream(b, list),
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntegerValue {
    pub value: i32,
}

impl IntegerValue {
    pub fn new(n: i32) -> IntegerValue {
        IntegerValue { value: n }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder) {
        Dict::stream_int(b, self.value);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ROSValue {
    pub ros: ROS,
}

impl ROSValue {
    fn parse(vs: &mut ValueStack, si: &StringIndex) -> FontResult<ROSValue> {
        let n = vs.pop_int()?;
        let s = si.get_string(vs.pop_int()?)?.unwrap_or_default();
        let s2 = si.get_string(vs.pop_int()?)?.unwrap_or_default();
        Ok(ROSValue { ros: ROS::new(s2, s, n) })
    }

    pub fn new(registry: String, ordering: String, n: i32) -> ROSValue {
        ROSValue { ros: ROS::new(registry, ordering, n) }
    }

    pub fn collect_strings(&self, list: &mut Vec<String>) {
        if index_of(list, &self.ros.registry) == -1 {
            list.push(self.ros.registry.clone());
        }
        if index_of(list, &self.ros.ordering) == -1 {
            list.push(self.ros.ordering.clone());
        }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder, list: &[String]) {
        Dict::stream_int(b, index_of(list, &self.ros.registry));
        Dict::stream_int(b, index_of(list, &self.ros.ordering));
        Dict::stream_int(b, self.ros.supplement);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OffsetSizeValue {
    pub size: i32,
    pub offset: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OffsetValue {
    pub offset: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumbersValue {
    pub values: Vec<DictNum>,
}

impl NumbersValue {
    fn parse(vs: &mut ValueStack, n: i32) -> NumbersValue {
        let n = if n == -1 { vs.count } else { n };
        let mut values = vec![DictNum::Null; n.max(0) as usize];
        let mut i = n - 1;
        while i >= 0 {
            match vs.pop_value() {
                Ok(v) => values[i as usize] = v.unwrap_or(DictNum::Null),
                Err(_) => break,
            }
            i -= 1;
        }
        NumbersValue { values }
    }

    pub fn from_ints(a: &[i32]) -> NumbersValue {
        NumbersValue { values: a.iter().map(|&n| DictNum::Integer(n)).collect() }
    }

    pub fn from_doubles(a: &[f64]) -> NumbersValue {
        NumbersValue { values: a.iter().map(|&d| DictNum::Double(d)).collect() }
    }

    pub fn from_double(d: f64) -> NumbersValue {
        NumbersValue { values: vec![DictNum::Double(d)] }
    }

    pub fn from_int(n: i32) -> NumbersValue {
        NumbersValue { values: vec![DictNum::Integer(n)] }
    }

    pub fn from_string(s: String) -> NumbersValue {
        NumbersValue { values: vec![DictNum::String(s)] }
    }

    pub fn from_matrix(m: &Matrix) -> NumbersValue {
        NumbersValue::from_doubles(&[m.a, m.b, m.c, m.d, m.tx, m.ty])
    }

    pub fn from_rect(r: &Rect) -> NumbersValue {
        NumbersValue::from_doubles(&[r.xmin, r.ymin, r.xmax, r.ymax])
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder) -> FontResult<()> {
        for v in &self.values {
            match v {
                DictNum::Integer(n) => Dict::stream_int(b, *n),
                DictNum::String(s) => Dict::stream_double_string(b, s)?,
                DictNum::Double(d) => {
                    let n = *d as i32;
                    if n as f64 == *d {
                        Dict::stream_int(b, n);
                    } else {
                        Dict::stream_double(b, *d)?;
                    }
                }
                DictNum::Null => panic!("java.lang.NullPointerException in NumbersValue.stream"),
            }
        }
        Ok(())
    }

    pub fn get_count(&self) -> i32 {
        self.values.len() as i32
    }

    fn num_as_double(v: &DictNum) -> f64 {
        match v {
            DictNum::Integer(n) => *n as f64,
            DictNum::String(s) => parse_double(s).expect("java.lang.NumberFormatException"),
            DictNum::Double(d) => *d,
            DictNum::Null => panic!("java.lang.NullPointerException in NumbersValue"),
        }
    }

    pub fn get_values_as_double(&self) -> Vec<f64> {
        self.values.iter().map(NumbersValue::num_as_double).collect()
    }

    pub fn get_values_as_int(&self) -> Vec<i32> {
        self.values
            .iter()
            .map(|v| match v {
                DictNum::Integer(n) => *n,
                DictNum::String(s) => s.parse::<i32>().expect("java.lang.NumberFormatException"),
                DictNum::Double(d) => *d as i32,
                DictNum::Null => panic!("java.lang.NullPointerException in NumbersValue"),
            })
            .collect()
    }

    pub fn get_first_value_as_double(&self) -> f64 {
        NumbersValue::num_as_double(&self.values[0])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StringValue {
    pub value: String,
}

impl StringValue {
    pub fn new(s: impl Into<String>) -> StringValue {
        StringValue { value: s.into() }
    }

    pub fn collect_strings(&self, list: &mut Vec<String>) {
        if index_of(list, &self.value) == -1 {
            list.push(self.value.clone());
        }
    }

    pub fn stream(&self, b: &mut CFFByteArrayBuilder, list: &[String]) {
        Dict::stream_int(b, index_of(list, &self.value));
    }
}
