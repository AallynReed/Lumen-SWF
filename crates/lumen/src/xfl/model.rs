//! The tag fields the XFL converter reads.

use std::collections::HashMap;

use crate::filters::{read_filter_list, Filter};
use crate::io::Reader;
use crate::tags::{code::*, Tag, Timeline};
use crate::types::{Rect, Rgba};
use crate::Swf;

/// A MATRIX: float scale and skew, integer translation, and whether scale and skew
/// were present.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JMatrix {
    pub has_scale: bool,
    pub scale_x: f32,
    pub scale_y: f32,
    pub has_rotate: bool,
    pub rotate0: f32,
    pub rotate1: f32,
    pub tx: i32,
    pub ty: i32,
}

impl JMatrix {
    pub fn read(r: &mut Reader) -> crate::error::Result<JMatrix> {
        r.align();
        let mut m = JMatrix::default();
        if r.bit()? {
            let n = r.ub(5)?;
            m.has_scale = true;
            m.scale_x = r.sb(n)? as f32 / 65536.0;
            m.scale_y = r.sb(n)? as f32 / 65536.0;
        }
        if r.bit()? {
            let n = r.ub(5)?;
            m.has_rotate = true;
            m.rotate0 = r.sb(n)? as f32 / 65536.0;
            m.rotate1 = r.sb(n)? as f32 / 65536.0;
        }
        let n = r.ub(5)?;
        m.tx = r.sb(n)?;
        m.ty = r.sb(n)?;
        r.align();
        Ok(m)
    }

    pub fn is_empty(&self) -> bool {
        self.tx == 0 && self.ty == 0 && !self.has_rotate && !self.has_scale
    }
}

/// A matrix in doubles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DMatrix {
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotate0: f64,
    pub rotate1: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for DMatrix {
    fn default() -> Self {
        DMatrix { scale_x: 1.0, scale_y: 1.0, rotate0: 0.0, rotate1: 0.0, tx: 0.0, ty: 0.0 }
    }
}

impl DMatrix {
    pub fn of(m: &JMatrix) -> DMatrix {
        DMatrix {
            scale_x: if m.has_scale { m.scale_x as f64 } else { 1.0 },
            scale_y: if m.has_scale { m.scale_y as f64 } else { 1.0 },
            rotate0: if m.has_rotate { m.rotate0 as f64 } else { 0.0 },
            rotate1: if m.has_rotate { m.rotate1 as f64 } else { 0.0 },
            tx: m.tx as f64,
            ty: m.ty as f64,
        }
    }

    pub fn scale(s: f64) -> DMatrix {
        DMatrix { scale_x: s, scale_y: s, ..Default::default() }
    }

    pub fn scale_xy(x: f64, y: f64) -> DMatrix {
        DMatrix { scale_x: x, scale_y: y, ..Default::default() }
    }

    pub fn translate(x: f64, y: f64) -> DMatrix {
        DMatrix { tx: x, ty: y, ..Default::default() }
    }

    pub fn concatenate(&self, m: &DMatrix) -> DMatrix {
        DMatrix {
            scale_x: self.scale_x * m.scale_x + self.rotate1 * m.rotate0,
            rotate0: self.rotate0 * m.scale_x + self.scale_y * m.rotate0,
            rotate1: self.scale_x * m.rotate1 + self.rotate1 * m.scale_y,
            scale_y: self.rotate0 * m.rotate1 + self.scale_y * m.scale_y,
            tx: self.scale_x * m.tx + self.rotate1 * m.ty + self.tx,
            ty: self.rotate0 * m.tx + self.scale_y * m.ty + self.ty,
        }
    }

    pub fn pre_concatenate(&self, m: &DMatrix) -> DMatrix {
        m.concatenate(self)
    }

    pub fn transform(&self, x: f64, y: f64) -> (f64, f64) {
        (self.scale_x * x + self.rotate1 * y + self.tx, self.rotate0 * x + self.scale_y * y + self.ty)
    }

    /// `toMATRIX`.
    pub fn to_jmatrix(&self) -> JMatrix {
        JMatrix {
            has_scale: true,
            scale_x: self.scale_x as f32,
            scale_y: self.scale_y as f32,
            has_rotate: true,
            rotate0: self.rotate0 as f32,
            rotate1: self.rotate1 as f32,
            tx: self.tx as i32,
            ty: self.ty as i32,
        }
    }
}

/// `CXFORM`/`CXFORMWITHALPHA` with the multipliers in 1/256.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cx {
    pub mult: [i32; 4],
    pub add: [i32; 4],
}

impl Cx {
    pub fn read(r: &mut Reader, alpha: bool) -> crate::error::Result<Cx> {
        r.align();
        let has_add = r.bit()?;
        let has_mult = r.bit()?;
        let n = r.ub(4)?;
        let k = if alpha { 4 } else { 3 };
        let mut c = Cx { mult: [256; 4], add: [0; 4] };
        if has_mult {
            for i in 0..k {
                c.mult[i] = r.sb(n)?;
            }
        }
        if has_add {
            for i in 0..k {
                c.add[i] = r.sb(n)?;
            }
        }
        r.align();
        Ok(c)
    }
}

#[derive(Debug, Clone)]
pub struct Place {
    pub num: u8,
    pub depth: i32,
    pub is_move: bool,
    pub char_id: i32,
    pub class_name: Option<String>,
    pub matrix: Option<JMatrix>,
    pub cxform: Option<Cx>,
    pub ratio: i32,
    pub name: Option<String>,
    pub clip_depth: i32,
    pub filters: Option<Vec<Filter>>,
    pub blend: i32,
    pub cache_as_bitmap: bool,
    pub visible: Option<u8>,
    pub background: Option<Rgba>,
    /// The clip action records, as byte ranges of the tag body.
    pub has_clip_actions: bool,
    pub amf: Option<(crate::amf::Amf, crate::amf::Value)>,
}

impl Place {
    pub fn is_visible(&self) -> bool {
        self.visible.is_none_or(|v| v != 0)
    }

    pub fn read(tag: &Tag) -> Option<Place> {
        let mut r = Reader::new(&tag.body);
        if tag.code == PLACE_OBJECT {
            let id = r.u16().ok()?;
            let depth = r.u16().ok()?;
            let matrix = JMatrix::read(&mut r).ok()?;
            let cxform = if !r.is_empty() { Cx::read(&mut r, false).ok() } else { None };
            return Some(Place {
                num: 1,
                depth: depth as i32,
                is_move: false,
                char_id: id as i32,
                class_name: None,
                matrix: Some(matrix),
                cxform,
                ratio: -1,
                name: None,
                clip_depth: -1,
                filters: None,
                blend: 0,
                cache_as_bitmap: false,
                visible: None,
                background: None,
                has_clip_actions: false,
                amf: None,
            });
        }
        let num = match tag.code {
            PLACE_OBJECT2 => 2,
            PLACE_OBJECT3 => 3,
            _ => 4,
        };
        let f1 = r.u8().ok()?;
        let f2 = if num >= 3 { r.u8().ok()? } else { 0 };
        let depth = r.u16().ok()? as i32;
        let class_name = if f2 & 0x08 != 0 || (f2 & 0x10 != 0 && f1 & 0x02 != 0) { Some(r.string().ok()?) } else { None };
        let char_id = if f1 & 0x02 != 0 { r.u16().ok()? as i32 } else { -1 };
        let matrix = if f1 & 0x04 != 0 { Some(JMatrix::read(&mut r).ok()?) } else { None };
        let cxform = if f1 & 0x08 != 0 { Some(Cx::read(&mut r, true).ok()?) } else { None };
        let ratio = if f1 & 0x10 != 0 { r.u16().ok()? as i32 } else { -1 };
        let name = if f1 & 0x20 != 0 { Some(r.string().ok()?) } else { None };
        let clip_depth = if f1 & 0x40 != 0 { r.u16().ok()? as i32 } else { -1 };
        let filters = if f2 & 0x01 != 0 { read_filter_list(&mut r).ok() } else { None };
        let blend = if f2 & 0x02 != 0 { r.u8().ok()? as i32 } else { 0 };
        let cache_as_bitmap = f2 & 0x04 != 0;
        if cache_as_bitmap {
            let _ = r.u8();
        }
        let visible = if f2 & 0x20 != 0 { r.u8().ok() } else { None };
        let background = if f2 & 0x40 != 0 { Rgba::read_rgba(&mut r).ok() } else { None };
        let amf = crate::action::scripts::place_object_amf(tag, 10).and_then(|d| crate::amf::read_amf3(d).ok());
        Some(Place {
            num,
            depth,
            is_move: f1 & 0x01 != 0,
            char_id,
            class_name,
            matrix,
            cxform,
            ratio,
            name,
            clip_depth,
            filters,
            blend,
            cache_as_bitmap,
            visible,
            background,
            has_clip_actions: f1 & 0x80 != 0,
            amf,
        })
    }
}

pub fn remove_depth(tag: &Tag) -> Option<i32> {
    let mut r = Reader::new(&tag.body);
    match tag.code {
        REMOVE_OBJECT => {
            r.u16().ok()?;
            Some(r.u16().ok()? as i32)
        }
        REMOVE_OBJECT2 => Some(r.u16().ok()? as i32),
        _ => None,
    }
}

pub fn is_place(code: u16) -> bool {
    matches!(code, PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4)
}

pub fn is_remove(code: u16) -> bool {
    matches!(code, REMOVE_OBJECT | REMOVE_OBJECT2)
}

#[derive(Debug, Clone)]
pub struct ButtonRecord {
    pub up: bool,
    pub over: bool,
    pub down: bool,
    pub hit: bool,
    pub char_id: u16,
    pub depth: i32,
    pub matrix: JMatrix,
    pub cxform: Option<Cx>,
    pub filters: Vec<Filter>,
    pub has_filters: bool,
    pub blend: Option<u8>,
}

pub struct Button {
    pub v2: bool,
    pub track_as_menu: bool,
    pub records: Vec<ButtonRecord>,
}

impl Button {
    pub fn read(tag: &Tag) -> Option<Button> {
        let mut r = Reader::new(&tag.body);
        r.u16().ok()?;
        let v2 = tag.code == DEFINE_BUTTON2;
        let mut track = false;
        if v2 {
            track = r.u8().ok()? & 1 != 0;
            r.u16().ok()?;
        }
        let mut records = Vec::new();
        loop {
            let flags = r.u8().ok()?;
            if flags == 0 {
                break;
            }
            let char_id = r.u16().ok()?;
            let depth = r.u16().ok()? as i32;
            let matrix = JMatrix::read(&mut r).ok()?;
            let cxform = if v2 { Some(Cx::read(&mut r, true).ok()?) } else { None };
            let has_filters = v2 && flags & 0x10 != 0;
            let filters = if has_filters { read_filter_list(&mut r).ok()? } else { Vec::new() };
            let blend = if v2 && flags & 0x20 != 0 { Some(r.u8().ok()?) } else { None };
            records.push(ButtonRecord { up: flags & 1 != 0, over: flags & 2 != 0, down: flags & 4 != 0, hit: flags & 8 != 0, char_id, depth, matrix, cxform, filters, has_filters, blend });
        }
        Some(Button { v2, track_as_menu: track, records })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoundInfo {
    pub sync_stop: bool,
    pub sync_no_multiple: bool,
    pub in_point: Option<u32>,
    pub out_point: Option<u32>,
    pub loops: Option<u16>,
    pub envelope: Option<Vec<(u32, u16, u16)>>,
}

impl SoundInfo {
    pub fn read(r: &mut Reader) -> Option<SoundInfo> {
        let f = r.u8().ok()?;
        let mut s = SoundInfo { sync_stop: f & 0x20 != 0, sync_no_multiple: f & 0x10 != 0, ..Default::default() };
        if f & 0x01 != 0 {
            s.in_point = Some(r.u32().ok()?);
        }
        if f & 0x02 != 0 {
            s.out_point = Some(r.u32().ok()?);
        }
        if f & 0x04 != 0 {
            s.loops = Some(r.u16().ok()?);
        }
        if f & 0x08 != 0 {
            let n = r.u8().ok()?;
            let mut env = Vec::new();
            for _ in 0..n {
                env.push((r.u32().ok()?, r.u16().ok()?, r.u16().ok()?));
            }
            s.envelope = Some(env);
        }
        Some(s)
    }
}

#[derive(Clone)]
pub struct EditText {
    pub bounds: Rect,
    pub has_text: bool,
    pub word_wrap: bool,
    pub multiline: bool,
    pub password: bool,
    pub read_only: bool,
    pub has_color: bool,
    pub has_max_length: bool,
    pub has_font: bool,
    pub has_font_class: bool,
    pub has_layout: bool,
    pub no_select: bool,
    pub border: bool,
    pub was_static: bool,
    pub html: bool,
    pub use_outlines: bool,
    pub font_id: i32,
    pub font_class: String,
    pub font_height: i32,
    pub color: Rgba,
    pub max_length: i32,
    pub align: u8,
    pub left_margin: i32,
    pub right_margin: i32,
    pub indent: i32,
    pub leading: i32,
    pub variable: String,
    pub text: String,
}

impl EditText {
    pub fn read(tag: &Tag) -> Option<EditText> {
        let mut r = Reader::new(&tag.body);
        r.u16().ok()?;
        let bounds = Rect::read(&mut r).ok()?;
        r.align();
        let b = |r: &mut Reader| r.bit().ok();
        let has_text = b(&mut r)?;
        let word_wrap = b(&mut r)?;
        let multiline = b(&mut r)?;
        let password = b(&mut r)?;
        let read_only = b(&mut r)?;
        let has_color = b(&mut r)?;
        let has_max_length = b(&mut r)?;
        let has_font = b(&mut r)?;
        let has_font_class = b(&mut r)?;
        let _auto_size = b(&mut r)?;
        let has_layout = b(&mut r)?;
        let no_select = b(&mut r)?;
        let border = b(&mut r)?;
        let was_static = b(&mut r)?;
        let html = b(&mut r)?;
        let use_outlines = b(&mut r)?;
        let mut e = EditText {
            bounds,
            has_text,
            word_wrap,
            multiline,
            password,
            read_only,
            has_color,
            has_max_length,
            has_font,
            has_font_class,
            has_layout,
            no_select,
            border,
            was_static,
            html,
            use_outlines,
            font_id: 0,
            font_class: String::new(),
            font_height: 0,
            color: Rgba { r: 0, g: 0, b: 0, a: 255 },
            max_length: 0,
            align: 0,
            left_margin: 0,
            right_margin: 0,
            indent: 0,
            leading: 0,
            variable: String::new(),
            text: String::new(),
        };
        if has_font {
            e.font_id = r.u16().ok()? as i32;
        }
        if has_font_class {
            e.font_class = r.string().ok()?;
        }
        if has_font || has_font_class {
            e.font_height = r.u16().ok()? as i32;
        }
        if has_color {
            e.color = Rgba::read_rgba(&mut r).ok()?;
        }
        if has_max_length {
            e.max_length = r.u16().ok()? as i32;
        }
        if has_layout {
            e.align = r.u8().ok()?;
            e.left_margin = r.u16().ok()? as i32;
            e.right_margin = r.u16().ok()? as i32;
            e.indent = r.u16().ok()? as i32;
            e.leading = r.i16().ok()? as i32;
        }
        e.variable = r.string().ok()?;
        if has_text {
            e.text = r.string().ok()?;
        }
        Some(e)
    }
}

/// `SoundStreamFrameRange`: a run of stream blocks.
#[derive(Debug, Clone)]
pub struct StreamRange {
    /// Character id of the containing sprite, or -1 for the root timeline.
    pub container: i32,
    pub start: i32,
    pub end: i32,
    pub head: usize,
    pub blocks: Vec<usize>,
}

impl StreamRange {
    pub fn fla_name(&self) -> String {
        format!("sound{}_{}-{}", self.container, self.start + 1, self.end + 1)
    }
}

/// `Timeline.populateSoundStreamBlocks` for one timeline: block indices into `tl.tags`.
pub fn stream_ranges(container: i32, tags: &[Tag], scene_offsets: &[u32], out: &mut HashMap<i32, (usize, Vec<StreamRange>)>) {
    let mut ranges: Option<Vec<StreamRange>> = None;
    let mut range: Option<StreamRange> = None;
    let mut no_sound = 2;
    let mut has_sound = false;
    let mut frame = 0i32;
    let mut head = 0usize;
    for (i, t) in tags.iter().enumerate() {
        if t.code == SOUND_STREAM_HEAD || t.code == SOUND_STREAM_HEAD2 {
            if let (Some(rs), Some(r)) = (ranges.take(), range.take()) {
                let mut rs = rs;
                if r.end > -1 {
                    rs.push(r);
                }
                out.insert(container, (head, rs));
            }
            head = i;
            ranges = Some(Vec::new());
            range = Some(StreamRange { container, start: -1, end: -1, head, blocks: Vec::new() });
            no_sound = 2;
            out.insert(container, (head, Vec::new()));
            continue;
        }
        if t.code == SHOW_FRAME {
            frame += 1;
            if has_sound {
                no_sound = 0;
            } else {
                no_sound += 1;
            }
            has_sound = false;
            if container == -1 && scene_offsets.contains(&(frame as u32)) {
                if let (Some(rs), Some(r)) = (ranges.as_mut(), range.as_mut()) {
                    if r.end > -1 {
                        rs.push(r.clone());
                    }
                    *r = StreamRange { container, start: -1, end: -1, head, blocks: Vec::new() };
                }
            }
        }
        let (Some(rs), Some(r)) = (ranges.as_mut(), range.as_mut()) else { continue };
        if t.code == SOUND_STREAM_BLOCK {
            if no_sound >= 2 && r.end > -1 {
                rs.push(r.clone());
                *r = StreamRange { container, start: -1, end: -1, head, blocks: Vec::new() };
            }
            r.blocks.push(i);
            if r.start == -1 {
                r.start = frame;
            }
            r.end = frame;
            has_sound = true;
        }
    }
    if let (Some(mut rs), Some(r)) = (ranges, range) {
        if r.end > -1 {
            rs.push(r);
        }
        out.insert(container, (head, rs));
    }
}

/// Character ids of every DefineX tag, with the path to the tag.
pub struct Chars {
    pub by_id: HashMap<u16, Vec<usize>>,
    /// Ids in the order a Java `HashMap` iterates them.
    pub order: Vec<u16>,
}

/// Iteration order of a Java `HashMap<Integer, _>` filled with `keys` in this order.
pub fn java_hash_order(keys: &[i32]) -> Vec<i32> {
    let mut cap = 16usize;
    let mut uniq: Vec<i32> = Vec::new();
    for &k in keys {
        if !uniq.contains(&k) {
            uniq.push(k);
            if uniq.len() > cap * 3 / 4 {
                cap *= 2;
            }
        }
    }
    let bucket = |k: i32| {
        let h = k as u32;
        ((h ^ (h >> 16)) as usize) & (cap - 1)
    };
    let mut idx: Vec<(usize, usize, i32)> = uniq.iter().enumerate().map(|(i, &k)| (bucket(k), i, k)).collect();
    idx.sort();
    idx.into_iter().map(|t| t.2).collect()
}

pub fn character_id(tag: &Tag) -> Option<u16> {
    if crate::tags::defines_character(tag.code) {
        tag.body.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    } else {
        None
    }
}

impl Chars {
    pub fn of(swf: &Swf) -> Chars {
        let mut by_id: HashMap<u16, Vec<usize>> = HashMap::new();
        let mut keys = Vec::new();
        fn walk(tl: &Timeline, prefix: &mut Vec<usize>, by_id: &mut HashMap<u16, Vec<usize>>, keys: &mut Vec<i32>) {
            for (i, t) in tl.tags.iter().enumerate() {
                prefix.push(i);
                if let Some(id) = character_id(t) {
                    if !by_id.contains_key(&id) {
                        by_id.insert(id, prefix.clone());
                        keys.push(id as i32);
                    }
                }
                if let Some(s) = &t.sprite {
                    walk(s, prefix, by_id, keys);
                }
                prefix.pop();
            }
        }
        walk(&swf.timeline, &mut Vec::new(), &mut by_id, &mut keys);
        let order = java_hash_order(&keys).into_iter().map(|k| k as u16).collect();
        Chars { by_id, order }
    }

    pub fn get<'a>(&self, swf: &'a Swf, id: u16) -> Option<&'a Tag> {
        self.by_id.get(&id).and_then(|p| swf.tag(p))
    }
}
