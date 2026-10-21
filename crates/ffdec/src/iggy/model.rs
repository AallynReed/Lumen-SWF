//! The structures of an Iggy main block: the movie header in its three layouts, fonts with
//! their glyph shapes, edit texts, timelines, ABC blocks and texture records.

use std::collections::{BTreeMap, HashSet};

use super::stream::{err, field, write_fields, Field, Stream, CONST_CHAR_OFFSET_SIZE, CONST_GENERAL_FONT_INFO2_SIZE, CONST_GENERAL_FONT_INFO_SIZE, CONST_KERNING_RECORD_SIZE, CONST_SHAPE_NODE_SIZE, CONST_SHAPE_SIZE, CONST_TEXT_DATA_SIZE};
use crate::error::Result;

pub fn check_range(s: &Stream, pos: u64, len: u64) -> Result<()> {
    if pos.checked_add(len).is_none_or(|e| e > s.len() as u64) {
        return err(format!("Iggy reference outside block: {pos}"));
    }
    Ok(())
}

// ---- movie headers ----

#[derive(Clone)]
pub struct Header64 {
    addr: [u64; 6],
    xmin: u64,
    ymin: u64,
    xmax: u64,
    ymax: u64,
    words: [u64; 6],
    frame_rate: f32,
    unk_5c: u64,
    imported_guid: u64,
    my_guid: u64,
    names: u64,
    unk78: u64,
    unk80: u64,
    last_section: u64,
    flash_filename: u64,
    decl_strings: u64,
    type_fonts: u64,
    flags: u64,
    font_count: u64,
    zero2: u64,
}

#[derive(Clone)]
pub struct Legacy {
    dictionary: u64,
    dictionary_size: u64,
    timeline: u64,
    other_timeline: u64,
    xmin: i64,
    ymin: i64,
    xmax: i64,
    ymax: i64,
    frame_count: u64,
    timeline_parameter: u64,
    color: u64,
    labels: u64,
    label_count: u64,
    scene_count: u64,
    scenes: u64,
    source: u64,
}

/// `IggyFlashHeaderInterface`: the 64-bit, 32-bit and 32-bit legacy movie headers.
#[derive(Clone)]
pub enum Header {
    H64(Header64),
    H32(Vec<u8>),
    Legacy(Legacy),
}

fn abs(s: &mut Stream) -> Result<u64> {
    let v = s.u64()?;
    Ok(if v == 1 { 0 } else { v.wrapping_add(s.position()).wrapping_sub(8) })
}

impl Header {
    pub fn read64(s: &mut Stream) -> Result<Header> {
        let mut addr = [0u64; 6];
        for a in addr.iter_mut() {
            *a = abs(s)?;
        }
        let xmin = s.u32()? as u32 as i32 as i64 as u64;
        let ymin = s.u32()? as u32 as i32 as i64 as u64;
        let xmax = s.u32()? as u32 as i32 as i64 as u64;
        let ymax = s.u32()? as u32 as i32 as i64 as u64;
        let mut words = [0u64; 6];
        for w in words.iter_mut() {
            *w = s.u32()?;
        }
        let frame_rate = s.f32()?;
        let unk_5c = s.u32()?;
        let imported_guid = s.u64()?;
        let my_guid = s.u64()?;
        let names = abs(s)?;
        let unk78 = abs(s)?;
        let unk80 = s.u64()?;
        let last_section = abs(s)?;
        let flash_filename = abs(s)?;
        let decl_strings = abs(s)?;
        let type_fonts = abs(s)?;
        let flags = s.u64()?;
        let font_count = s.u32()?;
        let zero2 = s.u32()?;
        Ok(Header::H64(Header64 { addr, xmin, ymin, xmax, ymax, words, frame_rate, unk_5c, imported_guid, my_guid, names, unk78, unk80, last_section, flash_filename, decl_strings, type_fonts, flags, font_count, zero2 }))
    }

    pub fn read32(s: &mut Stream) -> Result<Header> {
        check_range(s, s.position(), 128)?;
        Ok(Header::H32(s.bytes(128)?))
    }

    pub fn read_legacy(s: &mut Stream) -> Result<Header> {
        Ok(Header::Legacy(Legacy {
            dictionary: s.pointer(false)?,
            dictionary_size: s.u32()?,
            timeline: s.pointer(false)?,
            other_timeline: s.pointer(false)?,
            xmin: s.i32()?,
            ymin: s.i32()?,
            xmax: s.i32()?,
            ymax: s.i32()?,
            frame_count: s.u32()?,
            timeline_parameter: s.u32()?,
            color: s.u32()?,
            labels: s.pointer(false)?,
            label_count: s.u32()?,
            scene_count: s.u32()?,
            scenes: s.pointer(false)?,
            source: s.pointer(false)?,
        }))
    }

    pub fn write(&self, s: &mut Stream) {
        match self {
            Header::H64(h) => {
                let w = |s: &mut Stream, a: u64| {
                    let v = if a == 0 { 1 } else { a.wrapping_sub(s.position()) };
                    s.w64(v);
                };
                for &a in &h.addr {
                    w(s, a);
                }
                s.w32(h.xmin);
                s.w32(h.ymin);
                s.w32(h.xmax);
                s.w32(h.ymax);
                for &x in &h.words {
                    s.w32(x);
                }
                s.wf32(h.frame_rate);
                s.w32(h.unk_5c);
                s.w64(h.imported_guid);
                s.w64(h.my_guid);
                w(s, h.names);
                w(s, h.unk78);
                s.w64(h.unk80);
                w(s, h.last_section);
                w(s, h.flash_filename);
                w(s, h.decl_strings);
                w(s, h.type_fonts);
                s.w64(h.flags);
                s.w32(h.font_count);
                s.w32(h.zero2);
                s.idx().custom(184, &[0x00, 0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x34, 0x38, 0x3C, 0x40, 0x44, 0x48, 0x4C, 0x50, 0x58, 0x70, 0x78, 0x80, 0x84, 0x88, 0x90, 0x98, 0xA0, 0xB0], &[2, 2, 2, 2, 2, 2, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 2, 5, 5, 2, 2, 2, 2, 5]);
            }
            Header::H32(d) => {
                s.write_bytes(d);
                if let Some(i) = s.index.as_mut() {
                    i.custom(128, &[0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 64, 80, 84, 88, 96, 100, 104, 108, 120], &[2, 2, 2, 2, 2, 2, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 2, 2, 5, 2, 2, 2, 2, 5]);
                }
            }
            Header::Legacy(h) => {
                s.write_pointer(false, h.dictionary);
                s.w32(h.dictionary_size);
                s.write_pointer(false, h.timeline);
                s.write_pointer(false, h.other_timeline);
                s.w32(h.xmin as u64);
                s.w32(h.ymin as u64);
                s.w32(h.xmax as u64);
                s.w32(h.ymax as u64);
                s.w32(h.frame_count);
                s.w32(h.timeline_parameter);
                s.w32(h.color);
                s.write_pointer(false, h.labels);
                s.w32(h.label_count);
                s.w32(h.scene_count);
                s.write_pointer(false, h.scenes);
                s.write_pointer(false, h.source);
                if let Some(i) = s.index.as_mut() {
                    i.custom(64, &[0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 44, 48, 52, 56, 60], &[2, 5, 2, 2, 5, 5, 5, 5, 5, 5, 2, 5, 5, 2, 2]);
                }
            }
        }
    }

    fn word(d: &[u8], o: usize) -> u64 {
        u32::from_le_bytes(d[o..o + 4].try_into().unwrap()) as u64
    }

    fn ptr32(d: &[u8], o: usize) -> u64 {
        let v = Self::word(d, o);
        if v == 1 {
            0
        } else {
            (o as u64 + v) & 0xffff_ffff
        }
    }

    pub fn is_h32(&self) -> bool {
        matches!(self, Header::H32(_))
    }

    pub fn header_size(&self) -> usize {
        match self {
            Header::H64(_) => 184,
            Header::H32(_) => 128,
            Header::Legacy(_) => 64,
        }
    }

    pub fn base_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.addr[0],
            Header::H32(d) => Self::ptr32(d, 0),
            Header::Legacy(h) => h.dictionary,
        }
    }

    pub fn dictionary_size(&self) -> u64 {
        match self {
            Header::H64(h) => h.words[0],
            Header::H32(d) => Self::word(d, 0x28),
            Header::Legacy(h) => h.dictionary_size,
        }
    }

    pub fn timeline_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.addr[2],
            Header::H32(d) => Self::ptr32(d, 8),
            Header::Legacy(h) => h.timeline,
        }
    }

    pub fn source_name_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.addr[1],
            Header::H32(d) => Self::ptr32(d, 4),
            Header::Legacy(h) => h.source,
        }
    }

    pub fn frame_labels_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.addr[4],
            Header::H32(d) => Self::ptr32(d, 0x10),
            Header::Legacy(h) => h.labels,
        }
    }

    pub fn frame_label_count(&self) -> u64 {
        match self {
            Header::H64(h) => h.words[3],
            Header::H32(d) => Self::word(d, 0x34),
            Header::Legacy(h) => h.label_count,
        }
    }

    pub fn scenes_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.addr[5],
            Header::H32(d) => Self::ptr32(d, 0x14),
            Header::Legacy(h) => h.scenes,
        }
    }

    pub fn scene_count(&self) -> u64 {
        match self {
            Header::H64(h) => h.words[4],
            Header::H32(d) => Self::word(d, 0x38),
            Header::Legacy(h) => h.scene_count,
        }
    }

    pub fn frame_count(&self) -> u64 {
        match self {
            Header::H64(h) => h.words[1],
            Header::H32(d) => Self::word(d, 0x2c),
            Header::Legacy(h) => h.frame_count,
        }
    }

    pub fn background_color(&self) -> u64 {
        match self {
            Header::H64(h) => h.words[5],
            Header::H32(d) => Self::word(d, 0x3c),
            Header::Legacy(h) => h.color,
        }
    }

    /// (xmin, ymin, xmax, ymax) in pixels.
    pub fn bounds(&self) -> (i64, i64, i64, i64) {
        match self {
            Header::H64(h) => (h.xmin as i64, h.ymin as i64, h.xmax as i64, h.ymax as i64),
            Header::H32(d) => (Self::word(d, 0x18) as u32 as i32 as i64, Self::word(d, 0x1c) as u32 as i32 as i64, Self::word(d, 0x20) as u32 as i32 as i64, Self::word(d, 0x24) as u32 as i32 as i64),
            Header::Legacy(h) => (h.xmin, h.ymin, h.xmax, h.ymax),
        }
    }

    pub fn frame_rate(&self) -> f32 {
        match self {
            Header::H64(h) => h.frame_rate,
            Header::H32(d) => f32::from_bits(Self::word(d, 0x40) as u32),
            Header::Legacy(_) => 24.0,
        }
    }

    pub fn flash_filename_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.flash_filename,
            Header::H32(d) => Self::ptr32(d, 0x64),
            Header::Legacy(_) => 0,
        }
    }

    pub fn imports_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.names,
            Header::H32(d) => Self::ptr32(d, 0x50),
            Header::Legacy(_) => 0,
        }
    }

    pub fn import_count(&self) -> u64 {
        match self {
            Header::H64(h) => h.unk80 & 0xffff_ffff,
            Header::H32(d) => Self::word(d, 0x58),
            Header::Legacy(_) => 0,
        }
    }

    pub fn dictionary_fixups_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.last_section,
            Header::H32(d) => Self::ptr32(d, 0x60),
            Header::Legacy(_) => 0,
        }
    }

    pub fn abc_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.decl_strings,
            Header::H32(d) => Self::ptr32(d, 0x68),
            Header::Legacy(_) => 0,
        }
    }

    pub fn abc_pointer_offset(&self) -> i64 {
        match self {
            Header::H64(_) => 0x98,
            Header::H32(_) => 0x68,
            Header::Legacy(_) => -1,
        }
    }

    pub fn symbol_classes_address(&self) -> u64 {
        match self {
            Header::H64(h) => h.type_fonts,
            Header::H32(d) => Self::ptr32(d, 0x6c),
            Header::Legacy(_) => 0,
        }
    }

    pub fn symbol_class_count(&self) -> u64 {
        match self {
            Header::H64(h) => h.font_count,
            Header::H32(d) => Self::word(d, 0x78),
            Header::Legacy(_) => 0,
        }
    }

    pub fn has_as3(&self) -> bool {
        match self {
            Header::H64(h) => h.flags & 255 != 0,
            Header::H32(d) => d[0x70] != 0,
            Header::Legacy(_) => false,
        }
    }

    pub fn set_movie_properties(&mut self, b: (i64, i64, i64, i64), fps: f32, frames: u64, color: u64) -> Result<()> {
        match self {
            Header::H64(h) => {
                h.xmin = b.0 as u64;
                h.ymin = b.1 as u64;
                h.xmax = b.2 as u64;
                h.ymax = b.3 as u64;
                h.frame_rate = fps;
                h.words[1] = frames;
                h.words[5] = color;
            }
            Header::H32(d) => {
                for (i, v) in [b.0, b.1, b.2, b.3].iter().enumerate() {
                    d[0x18 + 4 * i..0x1c + 4 * i].copy_from_slice(&(*v as u32).to_le_bytes());
                }
                d[0x2c..0x30].copy_from_slice(&(frames as u32).to_le_bytes());
                d[0x3c..0x40].copy_from_slice(&(color as u32).to_le_bytes());
                d[0x40..0x44].copy_from_slice(&fps.to_bits().to_le_bytes());
            }
            Header::Legacy(h) => {
                if fps != 24.0 {
                    return err("The Iggy 32 legacy layout has a fixed frame rate of 24 FPS");
                }
                h.xmin = b.0;
                h.ymin = b.1;
                h.xmax = b.2;
                h.ymax = b.3;
                h.frame_count = frames;
                h.color = color;
            }
        }
        Ok(())
    }
}

// ---- glyphs ----

#[derive(Clone, Debug, PartialEq)]
pub struct ShapeNode {
    pub target_x: f32,
    pub target_y: f32,
    pub control_x: f32,
    pub control_y: f32,
    pub node_type: u32,
    pub node_subtype: u32,
    pub zer1: u32,
    pub zer2: u32,
    pub isstart: u64,
}

impl ShapeNode {
    pub fn new(tx: f32, ty: f32, cx: f32, cy: f32, node_type: u32, node_subtype: u32, first: bool) -> ShapeNode {
        ShapeNode { target_x: tx, target_y: ty, control_x: cx, control_y: cy, node_type, node_subtype, zer1: 0, zer2: 0, isstart: if first { 0 } else { 1 } }
    }

    fn read(s: &mut Stream) -> Result<ShapeNode> {
        Ok(ShapeNode { target_x: s.f32()?, target_y: s.f32()?, control_x: s.f32()?, control_y: s.f32()?, node_type: s.u8()?, node_subtype: s.u8()?, zer1: s.u8()?, zer2: s.u8()?, isstart: s.u32()? })
    }

    fn write(&self, s: &mut Stream) {
        s.wf32(self.target_x);
        s.wf32(self.target_y);
        s.wf32(self.control_x);
        s.wf32(self.control_y);
        s.w8(self.node_type);
        s.w8(self.node_subtype);
        s.w8(self.zer1);
        s.w8(self.zer2);
        s.w32(self.isstart);
    }
}

/// The 32-bit layout's extra glyph data (`IggyShape32`).
#[derive(Clone, Debug)]
pub struct Shape32 {
    pub header: Vec<u8>,
    pub address: u64,
    pub node_address: u64,
    pub auxiliary: [u64; 3],
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub minx: f32,
    pub miny: f32,
    pub maxx: f32,
    pub maxy: f32,
    pub unk: u64,
    pub count: u64,
    pub one: u64,
    pub one2: u64,
    pub one3: u64,
    pub one4: u64,
    pub two1: u64,
    pub nodes: Vec<ShapeNode>,
    pub x32: Option<Shape32>,
}

impl Shape {
    pub fn new(minx: f32, miny: f32, maxx: f32, maxy: f32, nodes: Vec<ShapeNode>) -> Shape {
        Shape { minx, miny, maxx, maxy, unk: if nodes.is_empty() { 1 } else { 48 }, count: nodes.len() as u64, one: 1, one2: 1, one3: 1, one4: 1, two1: 2, nodes, x32: None }
    }

    fn read(s: &mut Stream) -> Result<Shape> {
        let minx = s.f32()?;
        let miny = s.f32()?;
        let maxx = s.f32()?;
        let maxy = s.f32()?;
        let unk = s.u64()?;
        let count = s.u64()?;
        let one = s.u64()?;
        let one2 = s.u64()?;
        let one3 = s.u64()?;
        let one4 = s.u32()?;
        let two1 = s.u32()?;
        let mut nodes = Vec::new();
        for _ in 0..count {
            nodes.push(ShapeNode::read(s)?);
        }
        Ok(Shape { minx, miny, maxx, maxy, unk, count, one, one2, one3, one4, two1, nodes, x32: None })
    }

    fn read32(s: &mut Stream) -> Result<Shape> {
        let start = s.position();
        check_range(s, start, 40)?;
        let header = s.bytes(40)?;
        s.seek(start)?;
        let minx = s.f32()?;
        let miny = s.f32()?;
        let maxx = s.f32()?;
        let maxy = s.f32()?;
        let node_address = s.pointer(false)?;
        let count = s.u32()?;
        let mut auxiliary = [0u64; 3];
        for a in auxiliary.iter_mut() {
            *a = s.pointer(false)?;
        }
        let mut nodes = Vec::new();
        if count != 0 {
            if node_address == 0 {
                return err("Missing Iggy glyph nodes");
            }
            check_range(s, node_address, count * 24)?;
            s.seek(node_address)?;
            for _ in 0..count {
                nodes.push(ShapeNode::read(s)?);
            }
        }
        Ok(Shape { minx, miny, maxx, maxy, unk: 0, count, one: 0, one2: 0, one3: 0, one4: 0, two1: 0, nodes, x32: Some(Shape32 { header, address: start, node_address, auxiliary }) })
    }

    fn write(&self, s: &mut Stream) {
        s.idx().const_length(CONST_SHAPE_SIZE);
        s.wf32(self.minx);
        s.wf32(self.miny);
        s.wf32(self.maxx);
        s.wf32(self.maxy);
        s.w64(self.unk);
        s.w64(self.count);
        s.w64(self.one);
        s.w64(self.one2);
        s.w64(self.one3);
        s.w32(self.one4);
        s.w32(self.two1);
        s.idx().const_length_array(CONST_SHAPE_NODE_SIZE, self.nodes.len() as u64);
        for n in &self.nodes {
            n.write(s);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CharOffset {
    pub zero: u64,
    pub ischar1: u32,
    pub ischar2: u32,
    pub zero2: u64,
    pub xscale: u32,
    pub yscale: u32,
    pub zero3: u64,
    pub offset: u64,
}

impl CharOffset {
    fn read(s: &mut Stream) -> Result<CharOffset> {
        Ok(CharOffset { zero: s.u64()?, ischar1: s.u16()?, ischar2: s.u16()?, zero2: s.u32()?, xscale: s.u16()?, yscale: s.u16()?, zero3: s.u32()?, offset: s.u64()? })
    }

    fn write(&self, s: &mut Stream) {
        s.w64(self.zero);
        s.w16(self.ischar1);
        s.w16(self.ischar2);
        s.w32(self.zero2);
        s.w16(self.xscale);
        s.w16(self.yscale);
        s.w32(self.zero3);
        s.w64(self.offset);
    }
}

/// `setOlderOffsetToThisPos`: a 64-bit offset from `saved` to the current position.
fn older_offset(s: &mut Stream, saved: u64) {
    let cur = s.position();
    s.pos = saved as usize;
    s.w64(cur - saved);
    s.pos = cur as usize;
}

/// The 32-bit font layout's state (`IggyFont32`).
#[derive(Clone, Debug, Default)]
pub struct Font32 {
    pub header: Vec<u8>,
    pub header_size: usize,
    pub external_references: Vec<(u64, u64)>,
    pub relocated_parts: Vec<(u64, u64)>,
    pub part_sizes: BTreeMap<u64, u64>,
    original_descriptors: u64,
    original_count: u64,
}

/// `IggyFont`: a font definition (type 22).
#[derive(Clone, Debug)]
pub struct Font {
    pub ty: u32,
    pub font_id: u32,
    pub zeroone: Vec<u8>,
    pub char_count2: u32,
    pub ascent: u32,
    pub descent: u32,
    pub leading: i32,
    pub flags: u64,
    pub kern_count: u64,
    pub unk_float: [f32; 5],
    pub zero_padd: u64,
    pub what_2: u64,
    pub zero_padd_2: u64,
    pub one_padd: u64,
    pub xscale: u32,
    pub yscale: u32,
    pub zero_padd_3: u64,
    pub ssr1: f32,
    pub ssr2: f32,
    pub char_count: u64,
    pub zero_padd_4: u64,
    pub what_3: u64,
    pub sub_name: Vec<u16>,
    pub zeroes48a: Vec<u8>,
    pub zeroes48b: Vec<u8>,
    pub sss: [f32; 4],
    pub one_padd_n: [u64; 4],
    pub name: Option<String>,
    pub char_offsets: Option<Vec<CharOffset>>,
    pub glyphs: Vec<Option<Shape>>,
    pub code_points: Option<(Vec<u16>, u64)>,
    pub advances: Option<Vec<f32>>,
    pub kernings: Option<Vec<(u16, u16, i16)>>,
    pub x32: Option<Font32>,
}

pub const FONT_ID: u32 = 0xFF16;

impl Font {
    /// A new native font header (`IggyFont.create`).
    pub fn create(wide: bool, extended32: bool, id: u32) -> Result<Font> {
        let mut d = Stream::new();
        d.write_bytes(&vec![0u8; if wide { 352 } else if extended32 { 216 } else { 208 }]);
        d.seek(0)?;
        d.w16(FONT_ID);
        d.w16(id);
        let offsets: &[u64] = if wide { &[16, 48, 56, 64, 96, 128] } else { &[8, 28, 32, 36, 60, 76] };
        for &o in offsets {
            d.seek(o)?;
            if wide {
                d.w64(1);
            } else {
                d.w32(1);
            }
        }
        let mut r = Stream::reader(&d.data);
        if wide {
            Font::read(&mut r)
        } else {
            Font::read32(&mut r, extended32)
        }
    }

    pub fn read(s: &mut Stream) -> Result<Font> {
        let ty = s.u16()?;
        let font_id = s.u16()?;
        let zeroone = s.bytes(28)?;
        let char_count2 = s.u16()?;
        let ascent = s.u16()?;
        let descent = s.u16()?;
        let leading = s.i16()?;
        let flags = s.u64()?;
        let a_char = abs(s)?;
        let a_index = abs(s)?;
        let a_scale = abs(s)?;
        let kern_count = s.u32()?;
        let mut unk_float = [0f32; 5];
        for f in unk_float.iter_mut() {
            *f = s.f32()?;
        }
        let a_kern = abs(s)?;
        let zero_padd = s.u64()?;
        let what_2 = s.u64()?;
        let zero_padd_2 = s.u64()?;
        let a_name = abs(s)?;
        let one_padd = s.u64()?;
        let xscale = s.u16()?;
        let yscale = s.u16()?;
        let zero_padd_3 = s.u64()?;
        let ssr1 = s.f32()?;
        let ssr2 = s.f32()?;
        let char_count = s.u32()?;
        let zero_padd_4 = s.u64()?;
        let what_3 = s.u64()?;
        let mut sub_name = Vec::new();
        let mut done = false;
        for _ in 0..20 {
            let c = s.u16()? as u16;
            if c == 0 {
                done = true;
            }
            if !done {
                sub_name.push(c);
            }
        }
        let zeroes48a = s.bytes(48)?;
        let mut sss = [0f32; 4];
        let mut one_padd_n = [0u64; 4];
        for i in 0..4 {
            sss[i] = s.f32()?;
            one_padd_n[i] = s.u32()?;
        }
        let zeroes48b = s.bytes(48)?;
        let mut name = None;
        if a_name != 0 {
            s.seek(a_name)?;
            name = Some(s.wchar()?);
            s.pad8();
        }
        let mut char_offsets = None;
        let mut glyphs = Vec::new();
        if a_char != 0 {
            s.seek(a_char)?;
            let mut offs = Vec::new();
            let mut addrs = Vec::new();
            for _ in 0..char_count {
                let o = CharOffset::read(s)?;
                addrs.push(if o.offset == 1 { 0 } else { o.offset.wrapping_add(s.position()).wrapping_sub(8) });
                offs.push(o);
            }
            for a in addrs {
                if a != 0 {
                    s.seek(a)?;
                    glyphs.push(Some(Shape::read(s)?));
                } else {
                    glyphs.push(None);
                }
            }
            char_offsets = Some(offs);
        }
        let mut code_points = None;
        if a_index != 0 {
            s.seek(a_index)?;
            let mut v = Vec::new();
            for _ in 0..char_count {
                v.push(s.u16()? as u16);
            }
            code_points = Some((v, s.u32()?));
        }
        let mut advances = None;
        if a_scale != 0 {
            s.seek(a_scale)?;
            let mut v = Vec::new();
            for _ in 0..char_count {
                v.push(s.f32()?);
            }
            advances = Some(v);
        }
        let mut kernings = None;
        if a_kern != 0 {
            s.seek(a_kern)?;
            let mut v = Vec::new();
            for _ in 0..kern_count {
                v.push((s.u16()? as u16, s.u16()? as u16, s.i16()? as i16));
            }
            kernings = Some(v);
        }
        Ok(Font {
            ty,
            font_id,
            zeroone,
            char_count2,
            ascent,
            descent,
            leading,
            flags,
            kern_count,
            unk_float,
            zero_padd,
            what_2,
            zero_padd_2,
            one_padd,
            xscale,
            yscale,
            zero_padd_3,
            ssr1,
            ssr2,
            char_count,
            zero_padd_4,
            what_3,
            sub_name,
            zeroes48a,
            zeroes48b,
            sss,
            one_padd_n,
            name,
            char_offsets,
            glyphs,
            code_points,
            advances,
            kernings,
            x32: None,
        })
    }

    fn reference32(s: &mut Stream, f: u64) -> Result<u64> {
        check_range(s, f, 4)?;
        s.seek(f)?;
        s.pointer(false)
    }

    fn table32(s: &mut Stream, a: u64, size: u64) -> Result<()> {
        if a == 0 && size != 0 {
            return err("Missing Iggy font table");
        }
        check_range(s, a, size)?;
        s.seek(a)
    }

    pub fn read32(s: &mut Stream, extended: bool) -> Result<Font> {
        let header_size = if extended { 216 } else { 208 };
        let start = s.position();
        check_range(s, start, header_size as u64)?;
        let header = s.bytes(header_size)?;
        let mut h = Stream::reader(&header);
        let ty = h.u16()?;
        let font_id = h.u16()?;
        let zeroone = h.bytes(12)?;
        let char_count = h.u16()? as u64;
        let ascent = h.u16()?;
        let descent = h.u16()?;
        let leading = h.i16()?;
        let flags = h.u32()?;
        let zero_padd_3 = (header[88] as u64) << 32 | (header[89] as u64) << 40;
        h.seek(40)?;
        let kern_count = h.u32()?;
        let mut unk_float = [0f32; 5];
        for f in unk_float.iter_mut().take(4) {
            *f = h.f32()?;
        }
        let descriptors = Self::reference32(s, start + 28)?;
        let codes = Self::reference32(s, start + 32)?;
        let adv = Self::reference32(s, start + 36)?;
        let kerns = Self::reference32(s, start + 60)?;
        let name_address = Self::reference32(s, start + 76)?;
        let mut name = String::new();
        if name_address != 0 {
            Self::table32(s, name_address, 2)?;
            name = s.wchar()?;
        }
        let mut offs = Vec::new();
        let mut shapes = Vec::new();
        Self::table32(s, descriptors, char_count * 16)?;
        for i in 0..char_count {
            s.seek(descriptors + 16 * i)?;
            let zero = s.u32()?;
            let flag = s.u16()?;
            let flag2 = s.u8()?;
            let flag3 = s.u8()?;
            let context = s.u32()?;
            let offset = s.u32()?;
            offs.push(CharOffset { zero, ischar1: flag, ischar2: flag2, zero2: flag3 as u64, xscale: 0, yscale: 0, zero3: context, offset });
            shapes.push(if offset == 1 { 0 } else { (descriptors + 16 * i + 12 + offset) & 0xffff_ffff });
        }
        let mut glyphs = Vec::new();
        for sh in shapes {
            if sh == 0 {
                glyphs.push(None);
            } else {
                Self::table32(s, sh, 40)?;
                glyphs.push(Some(Shape::read32(s)?));
            }
        }
        let mut code_points = None;
        if codes != 0 {
            Self::table32(s, codes, char_count * 2)?;
            let mut v = Vec::new();
            for _ in 0..char_count {
                v.push(s.u16()? as u16);
            }
            code_points = Some((v, 0));
        }
        let mut advances = None;
        if adv != 0 {
            Self::table32(s, adv, char_count * 4)?;
            let mut v = Vec::new();
            for _ in 0..char_count {
                v.push(s.f32()?);
            }
            advances = Some(v);
        }
        let mut kernings = None;
        if kerns != 0 {
            Self::table32(s, kerns, kern_count * 6)?;
            let mut v = Vec::new();
            for _ in 0..kern_count {
                v.push((s.u16()? as u16, s.u16()? as u16, s.i16()? as i16));
            }
            kernings = Some(v);
        } else if kern_count != 0 {
            return err("Missing Iggy kerning table");
        }
        Ok(Font {
            ty,
            font_id,
            zeroone,
            char_count2: char_count as u32,
            ascent,
            descent,
            leading,
            flags,
            kern_count,
            unk_float,
            zero_padd: 0,
            what_2: 0,
            zero_padd_2: 0,
            one_padd: 0,
            xscale: 0,
            yscale: 0,
            zero_padd_3,
            ssr1: 0.0,
            ssr2: 0.0,
            char_count,
            zero_padd_4: 0,
            what_3: 0,
            sub_name: Vec::new(),
            zeroes48a: Vec::new(),
            zeroes48b: Vec::new(),
            sss: [0.0; 4],
            one_padd_n: [0; 4],
            name: Some(name),
            char_offsets: Some(offs),
            glyphs,
            code_points,
            advances,
            kernings,
            x32: Some(Font32 { header, header_size, original_descriptors: descriptors, original_count: char_count, ..Default::default() }),
        })
    }

    pub fn is_bold(&self) -> bool {
        self.zero_padd_3 & 0xff_0000_0000 != 0
    }

    pub fn is_italic(&self) -> bool {
        self.zero_padd_3 & 0xff00_0000_0000 != 0
    }

    pub fn has_layout(&self) -> bool {
        self.flags & 0xff0000 != 0
    }

    pub fn is_small_text(&self) -> bool {
        self.flags & 0x0200_0000 != 0
    }

    pub fn is_shift_jis(&self) -> bool {
        self.flags & 0x0100_0000 != 0
    }

    pub fn is_ansi(&self) -> bool {
        self.flags & 0x0400_0000 != 0
    }

    pub fn font_type(&self) -> u32 {
        (self.flags & 255) as u32
    }

    pub fn language(&self) -> u32 {
        ((self.flags >> 8) & 255) as u32
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_font_flags(&mut self, font_type: u32, language: u32, layout: bool, shift_jis: bool, small: bool, ansi: bool, bold: bool, italic: bool) {
        self.flags = (self.flags & !0x07ff_ffff) | (font_type & 255) as u64 | ((language & 255) as u64) << 8 | if layout { 0x10000 } else { 0 } | if shift_jis { 0x100_0000 } else { 0 } | if small { 0x200_0000 } else { 0 } | if ansi { 0x400_0000 } else { 0 };
        self.zero_padd_3 = (self.zero_padd_3 & !0xffff_0000_0000) | if bold { 0x1_0000_0000 } else { 0 } | if italic { 0x100_0000_0000 } else { 0 };
    }

    pub fn set_kernings(&mut self, k: Option<Vec<(u16, u16, i16)>>) {
        self.kern_count = k.as_ref().map_or(0, |v| v.len() as u64);
        self.kernings = k;
    }

    pub fn write(&mut self, s: &mut Stream) -> Result<()> {
        if self.x32.is_some() {
            return self.write32(s);
        }
        s.idx().const_length(CONST_GENERAL_FONT_INFO_SIZE);
        s.w16(self.ty);
        s.w16(self.font_id);
        s.write_bytes(&self.zeroone);
        s.w16(self.char_count2);
        s.w16(self.ascent);
        s.w16(self.descent);
        s.w16(self.leading as u32);
        s.w64(self.flags);
        let p_char = s.position();
        s.w64(1);
        let p_index = s.position();
        s.w64(1);
        let p_scale = s.position();
        s.w64(1);
        s.w32(self.kern_count);
        for &f in &self.unk_float {
            s.wf32(f);
        }
        let p_kern = s.position();
        s.w64(1);
        s.w64(self.zero_padd);
        s.idx().const_length(CONST_GENERAL_FONT_INFO2_SIZE);
        s.w64(self.what_2);
        s.w64(self.zero_padd_2);
        let p_name = s.position();
        s.w64(1);
        s.w64(self.one_padd);
        s.w16(self.xscale);
        s.w16(self.yscale);
        s.w64(self.zero_padd_3);
        s.wf32(self.ssr1);
        s.wf32(self.ssr2);
        s.w32(self.char_count);
        s.w64(self.zero_padd_4);
        s.w64(self.what_3);
        for i in 0..20 {
            s.w16(self.sub_name.get(i).copied().unwrap_or(0) as u32);
        }
        s.write_bytes(&self.zeroes48a);
        for i in 0..4 {
            s.wf32(self.sss[i]);
            s.w32(self.one_padd_n[i]);
        }
        s.write_bytes(&self.zeroes48b);
        if let Some(name) = &self.name {
            older_offset(s, p_name);
            let units: Vec<u16> = name.encode_utf16().collect();
            for &u in &units {
                s.w16(u as u32);
            }
            s.w16(0);
            let len = units.len() * 2 + 2;
            s.idx().array16(units.len() as u64 + 1);
            s.idx().pad8();
            let pad = 8 - len % 8;
            if pad < 8 {
                for _ in 0..pad {
                    s.write(0);
                }
            }
        }
        if let Some(offs) = &mut self.char_offsets {
            older_offset(s, p_char);
            s.idx().const_length_array(CONST_CHAR_OFFSET_SIZE, offs.len() as u64);
            let mut fix = Vec::new();
            for o in offs.iter_mut() {
                o.offset = 1;
                o.write(s);
                fix.push(s.position() - 8);
            }
            for (i, g) in self.glyphs.iter().enumerate() {
                if let Some(g) = g {
                    older_offset(s, fix[i]);
                    g.write(s);
                }
            }
        }
        if let Some((chars, _)) = &self.code_points {
            older_offset(s, p_index);
            for &c in chars {
                s.w16(c as u32);
            }
            s.idx().array16(chars.len() as u64);
            s.idx().pad8();
            s.pad8();
        }
        if let Some(adv) = &self.advances {
            older_offset(s, p_scale);
            for &a in adv {
                s.wf32(a);
            }
            s.idx().array32(adv.len() as u64);
            s.idx().pad8();
            s.pad8();
        }
        if let Some(k) = &self.kernings {
            older_offset(s, p_kern);
            s.idx().const_length_array(CONST_KERNING_RECORD_SIZE, self.kern_count);
            for i in 0..self.kern_count as usize {
                let (a, b, o) = k[i];
                s.w16(a as u32);
                s.w16(b as u32);
                s.w16(o as u16 as u32);
            }
            s.idx().pad8();
            s.pad8();
        }
        Ok(())
    }

    fn set_ref32(s: &mut Stream, f: u64, target: u64) {
        let saved = s.pos;
        s.pos = f as usize;
        s.write_pointer(false, target);
        s.pos = saved;
    }

    fn scalar32(s: &mut Stream, f: u64, v: u64, word: bool) {
        let saved = s.pos;
        s.pos = f as usize;
        if word {
            s.w16(v as u32);
        } else {
            s.w32(v);
        }
        s.pos = saved;
    }

    fn write_shape32(x: &mut Font32, s: &mut Stream, g: &Shape, fields: &mut Vec<Field>) {
        let start = s.position();
        if let Some(o) = &g.x32 {
            x.relocated_parts.push((o.address, start));
            x.part_sizes.insert(o.address, 40);
        }
        match &g.x32 {
            Some(o) => s.write_bytes(&o.header),
            None => s.write_bytes(&[0u8; 40]),
        }
        s.pos = start as usize;
        s.wf32(g.minx);
        s.wf32(g.miny);
        s.wf32(g.maxx);
        s.wf32(g.maxy);
        s.write_pointer(false, if g.nodes.is_empty() { 0 } else { start + 40 });
        s.w32(g.nodes.len() as u64);
        for i in 0..3 {
            let p = s.position();
            s.write_pointer(false, 0);
            if let Some(o) = &g.x32 {
                if o.auxiliary[i] != 0 {
                    x.external_references.push((p, o.auxiliary[i]));
                }
            }
            fields.push(field(p, 2));
        }
        s.pos = (start + 40) as usize;
        if let Some(o) = &g.x32 {
            if o.node_address != 0 {
                x.relocated_parts.push((o.node_address, s.position()));
                x.part_sizes.insert(o.node_address, g.nodes.len() as u64 * 24);
            }
        }
        for o in [0, 4, 8, 12, 20] {
            fields.push(field(start + o, 5));
        }
        fields.push(field(start + 16, 2));
        for n in &g.nodes {
            let p = s.position();
            n.write(s);
            for o in [0, 4, 8, 12] {
                fields.push(field(p + o, 5));
            }
            for o in [18, 20, 22] {
                fields.push(field(p + o, 4));
            }
        }
    }

    fn write32(&mut self, s: &mut Stream) -> Result<()> {
        if s.position() != 0 {
            return err("Serialize x86 fonts in a separate block");
        }
        let bold = self.is_bold();
        let italic = self.is_italic();
        let mut x = self.x32.take().unwrap();
        x.external_references.clear();
        x.relocated_parts.clear();
        x.part_sizes.clear();
        let offs = self.char_offsets.clone().unwrap_or_default();
        if self.char_count > 65535 || self.glyphs.len() as u64 != self.char_count || offs.len() as u64 != self.char_count {
            self.x32 = Some(x);
            return err("Invalid Iggy glyph count");
        }
        x.header[88] = bold as u8;
        x.header[89] = italic as u8;
        s.write_bytes(&x.header);
        for i in 0..4 {
            Self::scalar32(s, 44 + 4 * i as u64, self.unk_float[i].to_bits() as u64, false);
        }
        Self::scalar32(s, 0, self.ty as u64, true);
        Self::scalar32(s, 2, self.font_id as u64, true);
        Self::scalar32(s, 16, self.char_count, true);
        Self::scalar32(s, 18, self.ascent as u64, true);
        Self::scalar32(s, 20, self.descent as u64, true);
        Self::scalar32(s, 22, self.leading as u16 as u64, true);
        Self::scalar32(s, 24, self.flags, false);
        let kc = self.kernings.as_ref().map_or(0, |k| k.len() as u64);
        Self::scalar32(s, 40, kc, false);
        Self::scalar32(s, 100, self.char_count, false);
        let mut fields = Vec::new();
        for o in [2, 16, 18, 20, 22] {
            fields.push(field(o, 4));
        }
        for o in [40, 44, 48, 52, 56, 100] {
            fields.push(field(o, 5));
        }
        fields.push(field(27, 3));
        for o in [28, 32, 36, 60, 76] {
            Self::set_ref32(s, o, 0);
            fields.push(field(o, 2));
        }
        if let Some(name) = &self.name {
            let p = s.position();
            Self::set_ref32(s, 76, p);
            s.write_wchar(name);
            for i in 0..=name.encode_utf16().count() as u64 {
                fields.push(field(p + 2 * i, 4));
            }
        }
        s.pad8();
        let descriptors = s.position();
        Self::set_ref32(s, 28, if self.char_count == 0 { 0 } else { descriptors });
        for (i, d) in offs.iter().enumerate() {
            let p = s.position();
            if (i as u64) < x.original_count && x.original_descriptors != 0 {
                let old = x.original_descriptors + 16 * i as u64;
                x.relocated_parts.push((old, p));
                x.part_sizes.insert(old, 16);
            }
            s.w32(d.zero);
            s.w16(d.ischar1);
            s.w8(d.ischar2);
            s.w8(d.zero2 as u32);
            s.w32(d.zero3);
            s.w32(1);
            fields.push(field(p + 4, 4));
            if i as u64 >= x.original_count || x.original_descriptors == 0 {
                fields.push(field(p + 8, 8));
            }
            fields.push(field(p + 12, 2));
        }
        for (i, g) in self.glyphs.iter().enumerate() {
            if let Some(g) = g {
                s.pad8();
                let p = s.position();
                Self::set_ref32(s, descriptors + 16 * i as u64 + 12, p);
                Self::write_shape32(&mut x, s, g, &mut fields);
            }
        }
        if let Some((chars, _)) = &self.code_points {
            if chars.len() as u64 != self.char_count {
                self.x32 = Some(x);
                return err("Invalid Iggy code table size");
            }
            s.pad8();
            let p = s.position();
            Self::set_ref32(s, 32, p);
            for &c in chars {
                s.w16(c as u32);
            }
            for i in 0..self.char_count {
                fields.push(field(p + 2 * i, 4));
            }
        }
        if let Some(adv) = &self.advances {
            if adv.len() as u64 != self.char_count {
                self.x32 = Some(x);
                return err("Invalid Iggy advance table size");
            }
            s.pad8();
            let p = s.position();
            Self::set_ref32(s, 36, p);
            for &a in adv {
                s.wf32(a);
            }
            for i in 0..self.char_count {
                fields.push(field(p + 4 * i, 5));
            }
        }
        if let Some(k) = &self.kernings {
            s.pad8();
            let p = s.position();
            Self::set_ref32(s, 60, p);
            for i in 0..self.kern_count as usize {
                let (a, b, o) = k[i];
                s.w16(a as u32);
                s.w16(b as u32);
                s.w16(o as u16 as u32);
            }
            for i in 0..kc * 3 {
                fields.push(field(p + 2 * i, 4));
            }
        }
        let size = s.position();
        write_fields(s.idx(), &fields, size)?;
        self.x32 = Some(x);
        Ok(())
    }
}

// ---- texts ----

pub const TEXT_ID: u32 = 0xFF06;

/// `IggyText`: an edit text definition (type 6).
#[derive(Clone, Debug)]
pub struct Text {
    pub ty: u32,
    pub text_index: u32,
    pub is64: bool,
    pub common_pad: u64,
    pub unknown_word: u64,
    pub unknown_upper: u64,
    pub symbol_class_offset: u64,
    pub runtime_class_pointer: u64,
    pub symbol_class_name: Option<String>,
    pub font_class_name: Option<String>,
    symbol_changed: bool,
    font_changed: bool,
    pub bounds: [f32; 4],
    pub flags: u32,
    pub font_index: u32,
    pub font_class_pad: u64,
    pub font_class_offset: u64,
    pub color: [u8; 4],
    pub font_height: u32,
    pub max_length: u32,
    pub left_margin: u32,
    pub right_margin: u32,
    pub indent: u32,
    pub leading: i32,
    pub align: u32,
    pub layout_rest: Vec<u8>,
    pub initial_text: Option<String>,
}

pub const FLAG_WORD_WRAP: u32 = 0x001;
pub const FLAG_MULTILINE: u32 = 0x002;
pub const FLAG_PASSWORD: u32 = 0x004;
pub const FLAG_READ_ONLY: u32 = 0x008;
pub const FLAG_AUTO_SIZE: u32 = 0x010;
pub const FLAG_NO_SELECT: u32 = 0x020;
pub const FLAG_BORDER: u32 = 0x040;
pub const FLAG_WAS_STATIC: u32 = 0x080;
pub const FLAG_HTML: u32 = 0x100;
pub const FLAG_USE_OUTLINES: u32 = 0x200;
pub const SWF_BEHAVIOR_MASK: u32 = 0x3ff;

impl Text {
    pub fn create(wide: bool, id: u32) -> Result<Text> {
        let mut d = Stream::new();
        d.write_bytes(&vec![0u8; if wide { 104 } else { 68 }]);
        d.seek(0)?;
        d.w16(TEXT_ID);
        d.w16(id);
        let offsets: &[u64] = if wide { &[16, 56, 96] } else { &[8, 36, 64] };
        for &o in offsets {
            d.seek(o)?;
            if wide {
                d.w64(1);
            } else {
                d.w32(1);
            }
        }
        let mut r = Stream::reader(&d.data);
        if wide {
            Text::read(&mut r)
        } else {
            Text::read32(&mut r)
        }
    }

    fn read_style(&mut self, s: &mut Stream) -> Result<()> {
        let c = s.bytes(4)?;
        self.color = [c[0], c[1], c[2], c[3]];
        self.font_height = s.u16()?;
        self.max_length = s.u16()?;
        self.left_margin = s.u16()?;
        self.right_margin = s.u16()?;
        self.indent = s.u16()?;
        self.leading = s.i16()?;
        self.align = s.u8()?;
        self.layout_rest = s.bytes(if self.is64 { 15 } else { 7 })?;
        Ok(())
    }

    fn write_style(&self, s: &mut Stream) {
        s.write_bytes(&self.color);
        s.w16(self.font_height);
        s.w16(self.max_length);
        s.w16(self.left_margin);
        s.w16(self.right_margin);
        s.w16(self.indent);
        s.w16(self.leading as u16 as u32);
        s.w8(self.align);
        s.write_bytes(&self.layout_rest);
    }

    fn empty(is64: bool) -> Text {
        Text {
            ty: 0,
            text_index: 0,
            is64,
            common_pad: 0,
            unknown_word: 0,
            unknown_upper: 0,
            symbol_class_offset: 0,
            runtime_class_pointer: 0,
            symbol_class_name: None,
            font_class_name: None,
            symbol_changed: false,
            font_changed: false,
            bounds: [0.0; 4],
            flags: 0,
            font_index: 0,
            font_class_pad: 0,
            font_class_offset: 0,
            color: [0; 4],
            font_height: 0,
            max_length: 0,
            left_margin: 0,
            right_margin: 0,
            indent: 0,
            leading: 0,
            align: 0,
            layout_rest: Vec::new(),
            initial_text: None,
        }
    }

    fn read_name(&self, s: &mut Stream, pos: u64) -> Result<Option<String>> {
        s.seek(pos)?;
        let a = s.pointer(self.is64)?;
        if a == 0 {
            return Ok(None);
        }
        check_range(s, a, 2)?;
        s.seek(a)?;
        Ok(Some(s.wchar()?))
    }

    fn read_class_names(&mut self, s: &mut Stream, start: u64) -> Result<()> {
        let saved = s.pos;
        self.symbol_class_name = self.read_name(s, start + if self.is64 { 16 } else { 8 })?;
        self.font_class_name = self.read_name(s, start + if self.is64 { 56 } else { 36 })?;
        self.symbol_changed = false;
        self.font_changed = false;
        s.pos = saved;
        Ok(())
    }

    pub fn read(s: &mut Stream) -> Result<Text> {
        let start = s.position();
        let mut t = Text::empty(true);
        t.ty = s.u16()?;
        t.text_index = s.u16()?;
        t.common_pad = s.u32()?;
        t.unknown_word = s.u32()?;
        t.unknown_upper = s.u32()?;
        t.symbol_class_offset = s.u64()?;
        t.runtime_class_pointer = s.u64()?;
        for b in t.bounds.iter_mut() {
            *b = s.f32()?;
        }
        t.flags = s.u16()?;
        t.font_index = s.u16()?;
        t.font_class_pad = s.u32()?;
        t.font_class_offset = s.u64()?;
        t.read_style(s)?;
        let ofs = s.u64()?;
        let name_address = if ofs == 1 { 0 } else { ofs.wrapping_add(s.position()).wrapping_sub(8) };
        if name_address != 0 {
            s.seek(name_address)?;
            t.initial_text = Some(s.wchar()?);
        }
        s.pad8();
        t.read_class_names(s, start)?;
        Ok(t)
    }

    pub fn read32(s: &mut Stream) -> Result<Text> {
        let start = s.position();
        check_range(s, start, 68)?;
        let mut h = Stream::reader(&s.bytes(68)?);
        let mut t = Text::empty(false);
        t.ty = h.u16()?;
        t.text_index = h.u16()?;
        t.unknown_word = h.u32()?;
        t.symbol_class_offset = h.u32()?;
        t.runtime_class_pointer = h.u32()?;
        for b in t.bounds.iter_mut() {
            *b = h.f32()?;
        }
        t.flags = h.u16()?;
        t.font_index = h.u16()?;
        t.font_class_offset = h.u32()?;
        t.read_style(&mut h)?;
        s.seek(start + 64)?;
        let text = s.pointer(false)?;
        if text != 0 {
            check_range(s, text, 2)?;
            s.seek(text)?;
            t.initial_text = Some(s.wchar()?);
        }
        t.read_class_names(s, start)?;
        Ok(t)
    }

    pub fn set_symbol_class_name(&mut self, name: Option<String>) {
        if self.symbol_class_name != name {
            self.symbol_class_name = name;
            self.symbol_changed = true;
        }
    }

    pub fn set_font_class_name(&mut self, name: Option<String>) {
        if self.font_class_name != name {
            self.font_class_name = name;
            self.font_changed = true;
        }
    }

    pub fn is_generated_pointer(&self, offset: u64) -> bool {
        offset == if self.is64 { 96 } else { 64 } || (self.symbol_changed && offset == if self.is64 { 16 } else { 8 }) || (self.font_changed && offset == if self.is64 { 56 } else { 36 })
    }

    fn write_class_name(s: &mut Stream, f: u64, name: &Option<String>, wide: bool) {
        let end = s.position();
        s.pos = f as usize;
        s.write_pointer(wide, if name.is_none() { 0 } else { end });
        s.pos = end as usize;
        if let Some(n) = name {
            s.write_wchar(n);
            if wide {
                s.idx().array16(n.encode_utf16().count() as u64 + 1);
                s.pad8();
                s.idx().pad8();
            }
        }
    }

    fn write_changed_names(&self, s: &mut Stream, start: u64, wide: bool) {
        if self.symbol_changed {
            Self::write_class_name(s, start + if wide { 16 } else { 8 }, &self.symbol_class_name, wide);
        }
        if self.font_changed {
            Self::write_class_name(s, start + if wide { 56 } else { 36 }, &self.font_class_name, wide);
        }
    }

    pub fn write(&self, s: &mut Stream) -> Result<()> {
        if !self.is64 {
            return self.write32(s);
        }
        let start = s.position();
        s.idx().const_length(CONST_TEXT_DATA_SIZE);
        s.w16(self.ty);
        s.w16(self.text_index);
        s.w32(self.common_pad);
        s.w32(self.unknown_word);
        s.w32(self.unknown_upper);
        s.w64(self.symbol_class_offset);
        s.w64(self.runtime_class_pointer);
        for &b in &self.bounds {
            s.wf32(b);
        }
        s.w16(self.flags);
        s.w16(self.font_index);
        s.w32(self.font_class_pad);
        s.w64(self.font_class_offset);
        self.write_style(s);
        let p = s.position();
        s.w64(1);
        if let Some(t) = &self.initial_text {
            older_offset(s, p);
            s.write_wchar(t);
            s.pad8();
            s.idx().array16(t.encode_utf16().count() as u64 + 1);
            s.idx().pad8();
        }
        self.write_changed_names(s, start, true);
        Ok(())
    }

    fn write32(&self, s: &mut Stream) -> Result<()> {
        let start = s.position();
        s.w16(self.ty);
        s.w16(self.text_index);
        s.w32(self.unknown_word);
        s.w32(self.symbol_class_offset);
        s.w32(self.runtime_class_pointer);
        for &b in &self.bounds {
            s.wf32(b);
        }
        s.w16(self.flags);
        s.w16(self.font_index);
        s.w32(self.font_class_offset);
        self.write_style(s);
        s.seek(start + 64)?;
        s.write_pointer(false, if self.initial_text.is_none() { 0 } else { start + 68 });
        let mut fields = vec![field(2, 4)];
        for o in [16, 20, 24, 28] {
            fields.push(field(o, 5));
        }
        fields.push(field(32, 4));
        fields.push(field(34, 4));
        fields.push(field(8, 2));
        fields.push(field(36, 2));
        for o in (44..=54).step_by(2) {
            fields.push(field(o, 4));
        }
        fields.push(field(64, 2));
        if let Some(t) = &self.initial_text {
            s.write_wchar(t);
            for i in 0..=t.encode_utf16().count() as u64 {
                fields.push(field(68 + 2 * i, 4));
            }
        }
        let names_start = s.position();
        self.write_changed_names(s, start, false);
        let mut o = names_start - start;
        while o < s.position() - start {
            fields.push(field(o, 4));
            o += 2;
        }
        let size = s.position() - start;
        write_fields(s.idx(), &fields, size)
    }
}

// ---- timelines ----

#[derive(Clone, Debug)]
pub struct Command {
    pub address: u64,
    pub ty: u32,
    pub data: Vec<u8>,
}

/// `IggyTimeline`: a linked list of display commands.
#[derive(Clone, Debug)]
pub struct Timeline {
    pub commands: Vec<Command>,
    pub is64: bool,
}

/// A rebuilt timeline to splice into the main block (`IggyTimeline.Replacement`).
#[derive(Clone, Debug, Default)]
pub struct Replacement {
    pub data: Vec<u8>,
    pub fields: Vec<Field>,
    pub characters: Vec<(u64, i32)>,
    pub first: i64,
    pub last: i64,
    pub max_depth: i32,
    pub frame_count: i32,
    pub labels: Vec<NamedFrame>,
}

pub fn bytes_at(src: &[u8], a: u64, n: u64) -> Result<&[u8]> {
    if a.checked_add(n).is_none_or(|e| e > src.len() as u64) {
        return err("Iggy timeline reference outside main block");
    }
    Ok(&src[a as usize..(a + n) as usize])
}

pub fn reference(src: &[u8], f: u64, is64: bool) -> Result<u64> {
    let v = bytes_at(src, f, if is64 { 8 } else { 4 })?;
    let off = if is64 { i64::from_le_bytes(v.try_into().unwrap()) } else { i32::from_le_bytes(v.try_into().unwrap()) as i64 };
    if off == 1 {
        return Ok(0);
    }
    let t = if is64 { (f as i64).wrapping_add(off) as u64 } else { ((f as i64).wrapping_add(off) as u64) & 0xffff_ffff };
    bytes_at(src, t, 1)?;
    Ok(t)
}

pub fn wstring_at(src: &[u8], mut a: u64) -> Result<Option<String>> {
    if a == 0 {
        return Ok(None);
    }
    let mut units = Vec::new();
    loop {
        let b = bytes_at(src, a, 2)?;
        let v = u16::from_le_bytes([b[0], b[1]]);
        if v == 0 {
            return Ok(Some(String::from_utf16_lossy(&units)));
        }
        units.push(v);
        a += 2;
    }
}

impl Timeline {
    pub fn read(src: &[u8], mut address: u64, is64: bool) -> Result<Timeline> {
        let mut commands = Vec::new();
        let mut visited = HashSet::new();
        while address != 0 {
            if address.checked_add(if is64 { 16 } else { 8 }).is_none_or(|e| e > src.len() as u64) || !visited.insert(address) {
                return err("Invalid Iggy timeline link");
            }
            let next = reference(src, address, is64)?;
            let ty = bytes_at(src, address + if is64 { 8 } else { 4 }, 1)?[0] as u32;
            let size = match ty {
                1 => {
                    if is64 {
                        16
                    } else {
                        8
                    }
                }
                2 => {
                    let f = bytes_at(src, address + if is64 { 9 } else { 5 }, 2)?;
                    let (value, filters) = (f[0], f[1]);
                    if filters != 127 && filters > 0 {
                        if is64 {
                            96
                        } else {
                            64
                        }
                    } else if value & 32 != 0 || (value & 8 != 0 && reference(src, address + if is64 { 48 } else { 40 }, is64)? != 0) {
                        if is64 {
                            88
                        } else {
                            60
                        }
                    } else if value & 2 != 0 {
                        if is64 {
                            80
                        } else {
                            56
                        }
                    } else if value & 64 != 0 {
                        if is64 {
                            72
                        } else {
                            52
                        }
                    } else if is64 {
                        64
                    } else {
                        48
                    }
                }
                3 | 5 => {
                    if is64 {
                        24
                    } else {
                        12
                    }
                }
                4 => {
                    if is64 {
                        24
                    } else {
                        16
                    }
                }
                6 => {
                    if is64 {
                        56
                    } else {
                        40
                    }
                }
                t => return err(format!("Unsupported Iggy timeline command: {t}")),
            };
            if address + size as u64 > src.len() as u64 {
                return err("Truncated Iggy timeline command");
            }
            commands.push(Command { address, ty, data: src[address as usize..address as usize + size].to_vec() });
            address = next;
        }
        Ok(Timeline { commands, is64 })
    }
}

// ---- small records ----

#[derive(Clone, Debug, PartialEq)]
pub struct NamedFrame {
    pub name: String,
    pub frame: u64,
    pub reserved: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SymbolClass {
    pub object_address: u64,
    pub name: String,
    pub character_id: u64,
    pub reserved: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub library: String,
    pub symbols: Vec<SymbolClass>,
}

/// A replacement definition: its bytes, fixups and the character references to resolve.
#[derive(Clone, Debug)]
pub struct ObjectData {
    pub data: Vec<u8>,
    pub fields: Vec<Field>,
    pub characters: Vec<(u64, i32)>,
    pub address: u64,
    pub header_size: u32,
    pub ty: u32,
}

/// `IggyDeclStrings`: one ABC block of the linked list.
#[derive(Clone, Debug)]
pub struct DeclStrings {
    pub address: u64,
    pub next_address: u64,
    pub data: Vec<u8>,
    pub is64: bool,
}

impl DeclStrings {
    pub fn new(abc: &[u8]) -> DeclStrings {
        DeclStrings { address: 0, next_address: 0, data: abc.to_vec(), is64: true }
    }

    pub fn read(s: &mut Stream, is64: bool) -> Result<DeclStrings> {
        let address = s.position();
        let next_address = s.pointer(is64)?;
        let size = s.u32()?;
        if size < 4 || size > i32::MAX as u64 || size as usize > s.available() {
            return err("Invalid Iggy ABC length");
        }
        let data = s.bytes(size as usize)?;
        let d = DeclStrings { address, next_address, data, is64 };
        d.validate()?;
        Ok(d)
    }

    pub fn validate(&self) -> Result<()> {
        if self.data.len() < 4 {
            return err("Truncated Iggy ABC header");
        }
        let minor = self.data[0] as u32 | (self.data[1] as u32) << 8;
        let major = self.data[2] as u32 | (self.data[3] as u32) << 8;
        if major != 46 || (minor != 16 && minor != 18) {
            return err(format!("Unsupported Iggy ABC version: {major}.{minor}"));
        }
        Ok(())
    }
}

/// `IggyTextureMetadata`: 12-byte texture records after a header.
pub struct TextureMetadata {
    pub records: Vec<(Vec<u8>, u32)>,
}

impl TextureMetadata {
    pub fn parse(data: &[u8], is64: bool) -> Result<TextureMetadata> {
        let mut records = Vec::new();
        if data.is_empty() {
            return Ok(TextureMetadata { records });
        }
        let hs = if is64 { 16 } else { 12 };
        if data.len() < hs {
            return err("Invalid Iggy texture metadata");
        }
        let count = data[0] as usize | (data[1] as usize) << 8;
        if data.len() < hs + 12 * count {
            return err("Invalid Iggy texture metadata");
        }
        for i in 0..count {
            let r = data[hs + 12 * i..hs + 12 * i + 12].to_vec();
            let id = r[10] as u32 | (r[11] as u32) << 8;
            records.push((r, id));
        }
        Ok(TextureMetadata { records })
    }

    pub fn update(source: &[u8], is64: bool, changes: &[(u32, Option<Vec<u8>>)]) -> Result<Vec<u8>> {
        let size = if is64 { 16 } else { 12 };
        let original = TextureMetadata::parse(source, is64)?;
        let get = |id: u32| changes.iter().find(|c| c.0 == id).map(|c| &c.1);
        let mut last: Vec<(u32, usize)> = Vec::new();
        for (i, r) in original.records.iter().enumerate() {
            match last.iter_mut().find(|l| l.0 == r.1) {
                Some(l) => l.1 = i,
                None => last.push((r.1, i)),
            }
        }
        let last_of = |id: u32| last.iter().find(|l| l.0 == id).map(|l| l.1);
        let mut records: Vec<Vec<u8>> = Vec::new();
        for (i, r) in original.records.iter().enumerate() {
            match get(r.1) {
                Some(None) => continue,
                Some(Some(c)) if last_of(r.1) == Some(i) => records.push(c.clone()),
                _ => records.push(r.0.clone()),
            }
        }
        for (id, v) in changes {
            if let Some(v) = v {
                if v.len() != 12 {
                    return err("Invalid Iggy texture record");
                }
                if last_of(*id).is_none() {
                    records.push(v.clone());
                }
            }
        }
        if records.len() > 65535 {
            return err("Too many Iggy textures");
        }
        if source.is_empty() && records.is_empty() {
            return Ok(source.to_vec());
        }
        let old_end = if source.is_empty() { 0 } else { size + original.records.len() * 12 };
        let tail = source.len() - old_end;
        let mut out = vec![0u8; size + records.len() * 12 + tail];
        if !source.is_empty() {
            out[..size].copy_from_slice(&source[..size]);
        }
        out[0] = records.len() as u8;
        out[1] = (records.len() >> 8) as u8;
        let mut o = size;
        for r in &records {
            out[o..o + 12].copy_from_slice(r);
            o += 12;
        }
        if tail > 0 {
            out[o..].copy_from_slice(&source[old_end..]);
        }
        Ok(out)
    }
}
