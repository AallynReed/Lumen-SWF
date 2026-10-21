//! `SwfToIggyConvertor`: a SWF movie written into an Iggy file, either a new one or the file
//! it was converted from, with FFDec's checks, unsupported-tag filtering and warnings.

use std::collections::{HashMap, HashSet};

use super::file::{IggyFile, Variant};
use super::model::*;
use super::stream::{err, field, Fields, Stream};
use super::swf::{Obj, Swf as Movie};
use super::texture::External;
use super::to_swf::{self, jround, Cx};
use crate::error::{Error, Result};
use crate::font_edit::Font2;
use crate::io::Reader;
use crate::morph::{MorphFill, MorphLine, MorphShape};
use crate::shape::{self, Fill, LineStyle, Record, ShapeTag};
use crate::tags::{code::*, defines_character, Tag};
use crate::types::{Matrix, Rect, Rgba};
use crate::Swf;

/// A failure, or an unsupported tag the caller may skip and retry without.
enum Fail {
    Tag(usize, String),
    Err(Error),
}

impl From<Error> for Fail {
    fn from(e: Error) -> Fail {
        Fail::Err(e)
    }
}

type R<T> = std::result::Result<T, Fail>;

fn fail<T>(m: impl Into<String>) -> R<T> {
    Err(Fail::Err(Error::Other(m.into())))
}

fn unsupported<T>(n: usize, m: impl Into<String>) -> R<T> {
    Err(Fail::Tag(n, m.into()))
}

fn into_result<T>(r: R<T>) -> Result<T> {
    r.map_err(|f| match f {
        Fail::Tag(_, m) => Error::Other(m),
        Fail::Err(e) => e,
    })
}

// ---- the SWF as FFDec's tag objects see it ----

#[derive(Clone)]
struct Node {
    tag: Tag,
    loc: String,
    children: Vec<usize>,
    no_visible: bool,
    no_background: bool,
}

#[derive(Clone)]
struct Model {
    nodes: Vec<Node>,
    root: Vec<usize>,
    version: u8,
    rect: Rect,
    rate: f32,
    frames: u16,
}

impl Model {
    fn new(swf: &Swf) -> Model {
        let mut m = Model { nodes: Vec::new(), root: Vec::new(), version: swf.header.version, rect: swf.frame_rect, rate: swf.frame_rate, frames: swf.frame_count };
        m.root = m.add(&swf.timeline.tags, "main");
        m
    }

    fn add(&mut self, tags: &[Tag], path: &str) -> Vec<usize> {
        let mut ids = Vec::new();
        for (i, t) in tags.iter().enumerate() {
            if t.code == END {
                continue;
            }
            let id = self.nodes.len();
            self.nodes.push(Node { tag: t.clone(), loc: format!("{path}/tag{i}"), children: Vec::new(), no_visible: false, no_background: false });
            if t.code == DEFINE_SPRITE {
                if let Some(s) = &t.sprite {
                    let sid = u16::from_le_bytes([t.body[0], t.body[1]]);
                    let c = self.add(&s.tags, &format!("{path}/sprite{sid}"));
                    self.nodes[id].children = c;
                }
            }
            ids.push(id);
        }
        ids
    }

    /// Locations as FFDec's filter numbers them on its reloaded copy.
    fn relocate(&mut self) {
        let root = self.root.clone();
        self.locate(&root, "main");
    }

    fn locate(&mut self, list: &[usize], path: &str) {
        for (i, &n) in list.iter().enumerate() {
            self.nodes[n].loc = format!("{path}/tag{i}");
            if self.nodes[n].tag.code == DEFINE_SPRITE {
                let id = u16_at(&self.nodes[n].tag.body, 0);
                let c = self.nodes[n].children.clone();
                self.locate(&c, &format!("{path}/sprite{id}"));
            }
        }
    }

    fn flat(&self, list: &[usize], out: &mut Vec<usize>) {
        for &n in list {
            out.push(n);
            self.flat(&self.nodes[n].children, out);
        }
    }

    /// `IggyDefinitionConvertor.tags`: every tag, sprites followed by their children.
    fn all(&self) -> Vec<usize> {
        let mut v = Vec::new();
        self.flat(&self.root, &mut v);
        v
    }

    fn tag(&self, n: usize) -> &Tag {
        &self.nodes[n].tag
    }

    fn remove(&mut self, n: usize) {
        self.root.retain(|&x| x != n);
        for node in &mut self.nodes {
            node.children.retain(|&x| x != n);
        }
    }

    fn characters(&self) -> HashMap<u16, usize> {
        let mut m = HashMap::new();
        for n in self.all() {
            if let Some(id) = char_id(self.tag(n)) {
                m.entry(id).or_insert(n);
            }
        }
        m
    }

    fn jpeg_tables(&self) -> Option<Vec<u8>> {
        self.root.iter().map(|&n| self.tag(n)).find(|t| t.code == JPEG_TABLES).map(|t| t.body.clone())
    }

    fn string(&self, b: &[u8]) -> String {
        if self.version >= 6 {
            String::from_utf8_lossy(b).into_owned()
        } else {
            b.iter().map(|&c| c as char).collect()
        }
    }
}

fn is_character(t: &Tag) -> bool {
    defines_character(t.code) || t.code == IGGY_EXTERNAL_IMAGE
}

fn char_id(t: &Tag) -> Option<u16> {
    if is_character(t) && t.body.len() >= 2 {
        if t.code == IGGY_EXTERNAL_IMAGE {
            return t.body.get(1..3).map(|b| u16::from_le_bytes([b[0], b[1]]));
        }
        Some(u16::from_le_bytes([t.body[0], t.body[1]]))
    } else {
        None
    }
}

fn is_modifier(code: u16) -> bool {
    matches!(code, DEFINE_BUTTON_CXFORM | DEFINE_BUTTON_SOUND | DEFINE_FONT_ALIGN_ZONES | DEFINE_FONT_NAME | DEFINE_SCALING_GRID | NAME_CHARACTER | DEFINE_FONT_INFO | DEFINE_FONT_INFO2)
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map_or(0, |v| u16::from_le_bytes([v[0], v[1]]))
}

fn cstring(r: &mut Reader) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    loop {
        let b = r.u8()?;
        if b == 0 {
            return Ok(v);
        }
        v.push(b);
    }
}

/// The Iggy definition type of a character tag (`IggyDefinitionConvertor.type`).
fn def_type(code: u16) -> Option<u32> {
    Some(match code {
        DEFINE_SPRITE => 3,
        DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3 | DEFINE_FONT4 => 22,
        DEFINE_EDIT_TEXT => 6,
        DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => 1,
        DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => 2,
        DEFINE_BITS | DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 | DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 | IGGY_EXTERNAL_IMAGE => 4,
        DEFINE_TEXT | DEFINE_TEXT2 => 5,
        DEFINE_BUTTON | DEFINE_BUTTON2 => 7,
        DEFINE_BINARY_DATA => 20,
        DEFINE_SOUND => 21,
        _ => return None,
    })
}

/// `SwfToIggyConvertor.supportsNewTag`.
fn supports_new_tag(t: &Tag) -> bool {
    if is_character(t) {
        return def_type(t.code).is_some() && t.code != DEFINE_FONT4;
    }
    matches!(t.code, 0 | 1 | 4 | 5 | 8 | 9 | 13 | 15 | 23 | 26 | 28 | 43 | 57 | 62 | 69 | 70 | 71 | 72 | 76 | 78 | 82 | 86 | 89)
}

// ---- tag fields ----

fn read_cx(r: &mut Reader, alpha: bool) -> Result<Cx> {
    r.align();
    let add = r.bit()?;
    let mult = r.bit()?;
    let n = r.ub(4)?;
    let mut c = Cx { add, mult, m: [256; 4], a: [0; 4] };
    let k = if alpha { 4 } else { 3 };
    if mult {
        for i in 0..k {
            c.m[i] = r.sb(n)?;
        }
    }
    if add {
        for i in 0..k {
            c.a[i] = r.sb(n)?;
        }
    }
    r.align();
    Ok(c)
}

fn filter_len(d: &[u8]) -> Result<usize> {
    let id = *d.first().ok_or_else(|| Error::Other("Truncated filter".into()))?;
    let n = d.get(1).copied().unwrap_or(0) as usize;
    let len = 1 + match id {
        0 => 23,
        1 => 9,
        2 => 15,
        3 => 27,
        4 | 7 => 1 + 5 * n + 19,
        5 => {
            let (x, y) = (d.get(1).copied().unwrap_or(0) as usize, d.get(2).copied().unwrap_or(0) as usize);
            2 + 8 + 4 * x * y + 4 + 1
        }
        6 => 80,
        _ => return Err(Error::Other(format!("Unknown filter {id}"))),
    };
    if len > d.len() {
        return Err(Error::Other("Truncated filter".into()));
    }
    Ok(len)
}

fn filter_list(r: &mut Reader) -> Result<Vec<Vec<u8>>> {
    let n = r.u8()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let rest = r.clone().rest();
        let len = filter_len(rest)?;
        out.push(r.bytes(len)?.to_vec());
    }
    Ok(out)
}

/// `PlaceObjectTypeTag`'s view of PlaceObject, PlaceObject2, PlaceObject3 and PlaceObject4.
#[derive(Clone, Default)]
struct Place {
    v3: bool,
    depth: u16,
    character: i32,
    matrix: Option<Matrix>,
    cx: Option<Cx>,
    ratio: i32,
    name: Option<String>,
    clip_depth: i32,
    filters: Option<Vec<Vec<u8>>>,
    blend: Option<u8>,
    cache: bool,
    visible: Option<u8>,
    background: bool,
    class: Option<String>,
    has_class: bool,
    image: bool,
    moves: bool,
    clip_actions: bool,
}

fn place(m: &Model, t: &Tag) -> Result<Place> {
    let mut r = Reader::new(&t.body);
    let mut p = Place { character: -1, ratio: -1, clip_depth: -1, ..Default::default() };
    if t.code == PLACE_OBJECT {
        p.character = r.u16()? as i32;
        p.depth = r.u16()?;
        p.matrix = Some(Matrix::read(&mut r)?);
        if !r.is_empty() {
            p.cx = Some(read_cx(&mut r, false)?);
        }
        return Ok(p);
    }
    let f1 = r.u8()?;
    let f2 = if t.code == PLACE_OBJECT2 { 0 } else { r.u8()? };
    p.v3 = t.code != PLACE_OBJECT2;
    p.clip_actions = f1 & 0x80 != 0;
    let (has_clip, has_name, has_ratio, has_cx, has_matrix, has_char) = (f1 & 0x40 != 0, f1 & 0x20 != 0, f1 & 0x10 != 0, f1 & 8 != 0, f1 & 4 != 0, f1 & 2 != 0);
    p.moves = f1 & 1 != 0;
    let (bg, has_visible, image, class, cache, blend, filters) = (f2 & 0x40 != 0, f2 & 0x20 != 0, f2 & 0x10 != 0, f2 & 8 != 0, f2 & 4 != 0, f2 & 2 != 0, f2 & 1 != 0);
    p.depth = r.u16()?;
    p.image = image;
    p.has_class = class;
    if class {
        p.class = Some(m.string(&cstring(&mut r)?));
    } else if has_char {
        p.character = r.u16()? as i32;
    }
    if has_matrix {
        p.matrix = Some(Matrix::read(&mut r)?);
    }
    if has_cx {
        p.cx = Some(read_cx(&mut r, true)?);
    }
    if has_ratio {
        p.ratio = r.u16()? as i32;
    }
    if has_name {
        p.name = Some(m.string(&cstring(&mut r)?));
    }
    if has_clip {
        p.clip_depth = r.u16()? as i32;
    }
    if filters {
        p.filters = Some(filter_list(&mut r)?);
    }
    if blend {
        p.blend = Some(r.u8()?);
    }
    if cache {
        let c = if r.is_empty() { 1 } else { r.u8()? };
        p.cache = c == 1;
    }
    if has_visible {
        p.visible = Some(r.u8()?);
    }
    p.background = bg;
    Ok(p)
}

#[derive(Clone, Default)]
struct SoundInfo {
    sync_stop: bool,
    no_multiple: bool,
    inp: Option<u32>,
    out: Option<u32>,
    loops: Option<u16>,
    envelope: Option<Vec<(u32, u16, u16)>>,
}

fn sound_info(r: &mut Reader) -> Result<SoundInfo> {
    let f = r.u8()?;
    let mut s = SoundInfo { sync_stop: f & 0x20 != 0, no_multiple: f & 0x10 != 0, ..Default::default() };
    if f & 1 != 0 {
        s.inp = Some(r.u32()?);
    }
    if f & 2 != 0 {
        s.out = Some(r.u32()?);
    }
    if f & 4 != 0 {
        s.loops = Some(r.u16()?);
    }
    if f & 8 != 0 {
        let n = r.u8()?;
        let mut e = Vec::new();
        for _ in 0..n {
            e.push((r.u32()?, r.u16()?, r.u16()?));
        }
        s.envelope = Some(e);
    }
    Ok(s)
}

struct EditText {
    id: u16,
    bounds: Rect,
    has_text: bool,
    flags: [bool; 16],
    font: Option<u16>,
    font_class: Option<String>,
    height: u16,
    color: Option<[u8; 4]>,
    max_length: Option<u16>,
    layout: Option<(u8, u16, u16, u16, i16)>,
    variable: String,
    text: String,
}

fn edit_text(m: &Model, t: &Tag) -> Result<EditText> {
    let mut r = Reader::new(&t.body);
    let id = r.u16()?;
    let bounds = Rect::read(&mut r)?;
    let mut flags = [false; 16];
    for f in flags.iter_mut() {
        *f = r.bit()?;
    }
    let [has_text, _, _, _, _, has_color, has_max, has_font, has_class, _, has_layout, ..] = flags;
    let font = if has_font { Some(r.u16()?) } else { None };
    let font_class = if has_class { Some(m.string(&cstring(&mut r)?)) } else { None };
    let height = if has_font || has_class { r.u16()? } else { 0 };
    let color = if has_color { Some(r.bytes(4)?.try_into().unwrap()) } else { None };
    let max_length = if has_max { Some(r.u16()?) } else { None };
    let layout = if has_layout { Some((r.u8()?, r.u16()?, r.u16()?, r.u16()?, r.i16()?)) } else { None };
    let variable = m.string(&cstring(&mut r)?);
    let text = if has_text { m.string(&cstring(&mut r)?) } else { String::new() };
    Ok(EditText { id, bounds, has_text, flags, font, font_class, height, color, max_length, layout, variable, text })
}

struct Symbols {
    ids: Vec<u16>,
    names: Vec<String>,
}

fn symbols(m: &Model, t: &Tag) -> Result<Symbols> {
    let mut r = Reader::new(&t.body);
    let n = r.u16()?;
    let mut s = Symbols { ids: Vec::new(), names: Vec::new() };
    for _ in 0..n {
        s.ids.push(r.u16()?);
        s.names.push(m.string(&cstring(&mut r)?));
    }
    Ok(s)
}

fn write_symbols(s: &Symbols) -> Vec<u8> {
    let mut w = crate::io::Writer::new();
    w.u16(s.ids.len() as u16);
    for (i, n) in s.ids.iter().zip(&s.names) {
        w.u16(*i);
        w.string(n);
    }
    w.data
}

/// ImportAssets/ImportAssets2: url, (downloadNow, hasDigest), assets.
fn imports(m: &Model, t: &Tag) -> Result<(String, u8, u8, Vec<(u16, String)>)> {
    let mut r = Reader::new(&t.body);
    let url = m.string(&cstring(&mut r)?);
    let (mut dl, mut dg) = (1, 0);
    if t.code == IMPORT_ASSETS2 {
        dl = r.u8()?;
        dg = r.u8()?;
        if dg == 1 {
            r.bytes(20)?;
        }
    }
    let n = r.u16()?;
    let mut a = Vec::new();
    for _ in 0..n {
        a.push((r.u16()?, m.string(&cstring(&mut r)?)));
    }
    Ok((url, dl, dg, a))
}

fn exports(m: &Model, t: &Tag) -> Result<Vec<(u16, String)>> {
    let mut r = Reader::new(&t.body);
    let n = r.u16()?;
    let mut a = Vec::new();
    for _ in 0..n {
        a.push((r.u16()?, m.string(&cstring(&mut r)?)));
    }
    Ok(a)
}

fn fill_bitmaps(f: &Fill, out: &mut Vec<u16>) {
    if let Fill::Bitmap { id, .. } = f {
        out.push(*id);
    }
}

fn morph_fill_bitmaps(f: &MorphFill, out: &mut Vec<u16>) {
    if let MorphFill::Bitmap { id, .. } = f {
        out.push(*id);
    }
}

struct Ctx {
    chars: HashMap<u16, usize>,
    classes: HashMap<String, u16>,
    exports: HashMap<u16, String>,
    bound: HashMap<u16, Vec<String>>,
    parents: HashMap<usize, i32>,
}

impl Ctx {
    fn new(m: &Model) -> Ctx {
        let mut c = Ctx { chars: m.characters(), classes: HashMap::new(), exports: HashMap::new(), bound: HashMap::new(), parents: HashMap::new() };
        for n in m.all() {
            let t = m.tag(n);
            if t.code == DEFINE_SPRITE {
                for &ch in &m.nodes[n].children {
                    c.parents.insert(ch, u16_at(&t.body, 0) as i32);
                }
            }
            match t.code {
                SYMBOL_CLASS => {
                    if let Ok(s) = symbols(m, t) {
                        for (i, name) in s.ids.iter().zip(s.names) {
                            let v = c.bound.entry(*i).or_default();
                            if !v.contains(&name) {
                                v.push(name.clone());
                            }
                            c.classes.insert(name, *i);
                        }
                    }
                }
                IMPORT_ASSETS | IMPORT_ASSETS2 => {
                    if let Ok((_, _, _, a)) = imports(m, t) {
                        for (i, name) in a {
                            c.classes.insert(name, i);
                        }
                    }
                }
                EXPORT_ASSETS => {
                    if let Ok(a) = exports(m, t) {
                        for (i, name) in a {
                            c.exports.insert(i, name);
                        }
                    }
                }
                _ => {}
            }
        }
        c
    }

    fn is_image(&self, m: &Model, id: u16) -> bool {
        id != 65535 && self.chars.get(&id).is_some_and(|&n| def_type(m.tag(n).code) == Some(4))
    }

    fn class_id(&self, m: &Model, name: &str) -> i32 {
        match self.classes.get(name) {
            Some(&id) if self.chars.contains_key(&id) || id == 0 => id as i32,
            Some(&id) if m.all().iter().any(|&n| matches!(m.tag(n).code, IMPORT_ASSETS | IMPORT_ASSETS2) && imports(m, m.tag(n)).is_ok_and(|i| i.3.iter().any(|a| a.0 == id))) => id as i32,
            _ => -1,
        }
    }
}

/// `Tag.getNeededCharacters` and the character a modifier names.
fn needed(m: &Model, c: &Ctx, n: usize) -> (HashSet<i32>, HashSet<String>) {
    let t = m.tag(n);
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let b = &t.body;
    match t.code {
        SYMBOL_CLASS => {
            if let Ok(s) = symbols(m, t) {
                ids.extend(s.ids.iter().filter(|&&i| i != 0).map(|&i| i as i32));
            }
        }
        DEFINE_EDIT_TEXT => {
            if let Ok(e) = edit_text(m, t) {
                if let Some(f) = e.font {
                    ids.insert(f as i32);
                }
                if let Some(fc) = e.font_class {
                    names.insert(fc);
                }
            }
        }
        PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4 => {
            if let Ok(p) = place(m, t) {
                if t.code == PLACE_OBJECT || p.character >= 0 {
                    ids.insert(p.character);
                }
                if p.v3 && p.has_class {
                    let class = p.class.clone().unwrap_or_default();
                    let id = c.class_id(m, &class);
                    if id != -1 {
                        ids.insert(id);
                    } else {
                        names.insert(class);
                    }
                }
            }
        }
        START_SOUND2 => {
            let mut r = Reader::new(b);
            let class = cstring(&mut r).map(|v| m.string(&v)).unwrap_or_default();
            ids.insert(c.class_id(m, &class));
        }
        DEFINE_SPRITE => {
            for &ch in &m.nodes[n].children {
                if matches!(m.tag(ch).code, PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4 | START_SOUND | START_SOUND2 | VIDEO_FRAME) {
                    let (i, s) = needed(m, c, ch);
                    ids.extend(i);
                    names.extend(s);
                }
            }
        }
        DEFINE_BUTTON | DEFINE_BUTTON2 => {
            if let Ok(bt) = button(t) {
                ids.extend(bt.records.iter().map(|r| r.character as i32));
            }
        }
        DEFINE_TEXT | DEFINE_TEXT2 => {
            if let Ok(tx) = static_text(t) {
                for r in &tx.records {
                    if let Some(f) = r.font {
                        ids.insert(f as i32);
                    }
                }
            }
        }
        EXPORT_ASSETS => {
            if let Ok(a) = exports(m, t) {
                ids.extend(a.iter().map(|x| x.0 as i32));
            }
        }
        DO_INIT_ACTION | REMOVE_OBJECT | FREE_CHARACTER | NAME_CHARACTER | VIDEO_FRAME | START_SOUND | CSM_SETTINGS | DEFINE_FONT_INFO | DEFINE_FONT_INFO2 | DEFINE_FONT_ALIGN_ZONES | DEFINE_FONT_NAME | DEFINE_SCALING_GRID | DEFINE_BUTTON_CXFORM => {
            ids.insert(u16_at(b, 0) as i32);
        }
        DEFINE_BUTTON_SOUND => {
            for i in 0..4 {
                let s = u16_at(b, 2 + 9 * i);
                if s != 0 {
                    ids.insert(s as i32);
                }
            }
            ids.insert(u16_at(b, 0) as i32);
        }
        c2 if shape::is_shape(c2) => {
            if let Ok(s) = ShapeTag::read(t) {
                let mut bm = Vec::new();
                for f in &s.shape.styles.fills {
                    fill_bitmaps(f, &mut bm);
                }
                for l in &s.shape.styles.lines {
                    if let Some(f) = &l.fill {
                        fill_bitmaps(f, &mut bm);
                    }
                }
                for r in &s.shape.records {
                    if let Record::StyleChange { new_styles: Some(st), .. } = r {
                        for f in &st.fills {
                            fill_bitmaps(f, &mut bm);
                        }
                    }
                }
                ids.extend(bm.into_iter().filter(|&i| c.is_image(m, i)).map(|i| i as i32));
            }
        }
        DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => {
            if let Ok(s) = MorphShape::read(t) {
                let mut bm = Vec::new();
                for f in &s.fills {
                    morph_fill_bitmaps(f, &mut bm);
                }
                ids.extend(bm.into_iter().filter(|&i| c.is_image(m, i)).map(|i| i as i32));
            }
        }
        _ => {}
    }
    if is_modifier(t.code) && !is_character(t) {
        ids.insert(u16_at(b, 0) as i32);
    }
    (ids, names)
}

// ---- names in warnings ----

fn escape(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            _ => o.push(c),
        }
    }
    o
}

fn export_name(s: &str) -> String {
    if s.starts_with("__Packages.") {
        s.to_string()
    } else {
        format!("\"{}\"", escape(s))
    }
}

/// `Tag.getName`: the tag name and its identifying properties.
fn tag_name(m: &Model, c: &Ctx, n: usize) -> String {
    let t = m.tag(n);
    let mut props: Vec<(&str, String)> = Vec::new();
    let b = &t.body;
    let exp = |id: i32, props: &mut Vec<(&str, String)>| {
        if let Some(e) = c.exports.get(&(id as u16)).filter(|_| id >= 0) {
            props.push(("exp", export_name(e)));
        }
    };
    match t.code {
        PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4 => {
            if let Ok(p) = place(m, t) {
                let ch = if p.has_class { -1 } else { p.character };
                if ch == -1 && p.class.is_some() && p.has_class {
                    props.push(("cls", p.class.clone().unwrap()));
                } else {
                    if ch > -1 {
                        props.push(("chid", ch.to_string()));
                    }
                    exp(ch, &mut props);
                }
                props.push(("dpt", p.depth.to_string()));
                if p.clip_depth > -1 {
                    props.push(("cdp", p.clip_depth.to_string()));
                }
                if let Some(n) = &p.name {
                    props.push(("nm", format!("\"{}\"", escape(n))));
                }
            }
        }
        REMOVE_OBJECT | REMOVE_OBJECT2 => {
            let (ch, depth) = if t.code == REMOVE_OBJECT { (u16_at(b, 0) as i32, u16_at(b, 2)) } else { (-1, u16_at(b, 0)) };
            if ch != -1 {
                props.push(("chid", ch.to_string()));
            }
            exp(ch, &mut props);
            props.push(("dpt", depth.to_string()));
        }
        CSM_SETTINGS => props.push(("tid", u16_at(b, 0).to_string())),
        DEFINE_BUTTON_CXFORM | DEFINE_BUTTON_SOUND => props.push(("bid", u16_at(b, 0).to_string())),
        DEFINE_FONT_ALIGN_ZONES | DEFINE_FONT_NAME | DEFINE_FONT_INFO | DEFINE_FONT_INFO2 => props.push(("fid", u16_at(b, 0).to_string())),
        DEFINE_SCALING_GRID | FREE_CHARACTER => props.push(("chid", u16_at(b, 0).to_string())),
        DO_ABC2 => {
            let mut r = Reader::new(b);
            let _ = r.u32();
            if let Ok(n) = cstring(&mut r) {
                if !n.is_empty() {
                    props.push(("nm", m.string(&n)));
                }
            }
        }
        DO_INIT_ACTION => {
            let id = u16_at(b, 0);
            props.push(("sid", id.to_string()));
            exp(id as i32, &mut props);
        }
        EXPORT_ASSETS => {
            if let Ok(a) = exports(m, t) {
                if a.len() == 1 {
                    props.push(("chid", a[0].0.to_string()));
                    props.push(("exp", export_name(&a[0].1)));
                }
            }
        }
        IMPORT_ASSETS | IMPORT_ASSETS2 => {
            if let Ok((_, _, _, a)) = imports(m, t) {
                if a.len() == 1 {
                    props.push(("chid", a[0].0.to_string()));
                    props.push(("imp", if a[0].1.starts_with("__Packages.") { a[0].1.clone() } else { format!("\"{}\"", escape(&a[0].1)) }));
                }
            }
        }
        FRAME_LABEL => {
            let mut r = Reader::new(b);
            if let Ok(n) = cstring(&mut r) {
                if !n.is_empty() {
                    props.push(("name", format!("\"{}\"", escape(&m.string(&n)))));
                }
            }
        }
        START_SOUND => props.push(("cid", u16_at(b, 0).to_string())),
        START_SOUND2 => {
            let mut r = Reader::new(b);
            props.push(("cls", cstring(&mut r).map(|v| m.string(&v)).unwrap_or_default()));
        }
        SOUND_STREAM_HEAD | SOUND_STREAM_HEAD2 => props.push(("cid", c.parents.get(&n).copied().unwrap_or(-1).to_string())),
        SET_TAB_INDEX => {
            props.push(("dpt", u16_at(b, 0).to_string()));
            props.push(("ti", u16_at(b, 2).to_string()));
        }
        _ => {
            if let Some(id) = char_id(t) {
                props.push(("chid", id.to_string()));
                exp(id as i32, &mut props);
                if let Some(cls) = c.bound.get(&id) {
                    props.push(("cls", cls.join(", ")));
                }
                if matches!(t.code, DEFINE_FONT2 | DEFINE_FONT3) {
                    if let Ok(f) = Font2::read(t.code, b) {
                        props.push(("fn", format!("\"{}\"", escape(&m.string(&f.name)))));
                    }
                }
                if t.code == DEFINE_EDIT_TEXT {
                    if let Ok(e) = edit_text(m, t) {
                        if !e.variable.is_empty() {
                            props.push(("vn", format!("\"{}\"", escape(&e.variable))));
                        }
                    }
                }
            }
        }
    }
    let mut s = crate::tags::tag_name(t.code).map_or_else(|| format!("Unknown({})", t.code), str::to_string);
    if t.code == IGGY_EXTERNAL_IMAGE {
        s = "IggyExternalImage".into();
    }
    if t.code == PRODUCT_INFO && m.version < 7 {
        s = "SerialNumber".into();
    }
    if !props.is_empty() {
        s.push_str(" (");
        s.push_str(&props.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", "));
        s.push(')');
    }
    s
}

// ---- unsupported tag filter ----

/// `IggySwfInputFilter`: unsupported tags removed with a warning, then everything that
/// references what was removed.
struct Filter {
    m: Model,
    warnings: Vec<String>,
    characters: HashMap<usize, HashSet<i32>>,
    classes: HashMap<usize, HashSet<String>>,
    class_names: HashMap<i32, HashSet<String>>,
    omitted_ids: HashSet<i32>,
    omitted_classes: HashSet<String>,
    names: HashMap<usize, String>,
}

impl Filter {
    fn new(mut m: Model, variant: Variant) -> Filter {
        m.relocate();
        let ctx = Ctx::new(&m);
        let mut f = Filter { m, warnings: Vec::new(), characters: HashMap::new(), classes: HashMap::new(), class_names: HashMap::new(), omitted_ids: HashSet::new(), omitted_classes: HashSet::new(), names: HashMap::new() };
        for n in f.m.all() {
            let (ids, names) = if f.m.tag(n).code == DEFINE_SPRITE { Default::default() } else { needed(&f.m, &ctx, n) };
            f.characters.insert(n, ids);
            f.classes.insert(n, names);
            f.names.insert(n, tag_name(&f.m, &ctx, n));
        }
        for n in f.m.all() {
            let t = f.m.tag(n);
            if t.code == SYMBOL_CLASS {
                if let Ok(s) = symbols(&f.m, t) {
                    for (i, name) in s.ids.iter().zip(s.names) {
                        f.class_names.entry(*i as i32).or_default().insert(name);
                    }
                }
            }
            if matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
                if let Ok((_, _, _, a)) = imports(&f.m, t) {
                    for (i, name) in a {
                        f.class_names.entry(i as i32).or_default().insert(name);
                    }
                }
            }
        }
        for n in f.m.all() {
            let t = f.m.tag(n).clone();
            if matches!(t.code, PLACE_OBJECT3 | PLACE_OBJECT4) {
                if let Ok(p) = place(&f.m, &t) {
                    if p.visible.is_some() {
                        f.omit_attribute(n, "visible");
                        f.m.nodes[n].no_visible = true;
                    }
                    if p.background {
                        f.omit_attribute(n, "opaqueBackground");
                        f.m.nodes[n].no_background = true;
                    }
                }
            }
            let unsupported = !supports_new_tag(&t);
            let legacy = variant == Variant::X86Legacy && (matches!(t.code, DO_ABC | DO_ABC2 | IMPORT_ASSETS | IMPORT_ASSETS2 | SYMBOL_CLASS) || (t.code == FILE_ATTRIBUTES && t.body.first().is_some_and(|b| b & 8 != 0)));
            if unsupported || legacy {
                f.discard(n, if legacy { "not supported by 32 legacy" } else { "no supported native Iggy representation" });
            }
        }
        f.prune();
        f
    }

    fn warn(&mut self, n: usize, reason: &str) {
        let w = format!("Skipping {} at {}: {reason}", self.names[&n], self.m.nodes[n].loc);
        self.warnings.push(w);
    }

    fn omit_attribute(&mut self, n: usize, attribute: &str) {
        let w = format!("Ignoring unsupported {attribute} attribute of {} at {}", self.names[&n], self.m.nodes[n].loc);
        self.warnings.push(w);
    }

    fn omit_id(&mut self, id: i32) {
        self.omitted_ids.insert(id);
        if let Some(names) = self.class_names.get(&id) {
            self.omitted_classes.extend(names.iter().cloned());
        }
    }

    fn discard(&mut self, n: usize, reason: &str) {
        self.warn(n, reason);
        let t = self.m.tag(n).clone();
        if let Some(id) = char_id(&t) {
            self.omit_id(id as i32);
        }
        if matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            if let Ok((_, _, _, a)) = imports(&self.m, &t) {
                for (i, _) in a {
                    self.omit_id(i as i32);
                }
            }
        }
        if t.code == SYMBOL_CLASS {
            if let Ok(s) = symbols(&self.m, &t) {
                self.omitted_classes.extend(s.names);
            }
        }
        self.m.remove(n);
    }

    fn prune(&mut self) {
        loop {
            let mut changed = false;
            for n in self.m.all() {
                let t = self.m.tag(n).clone();
                if t.code == SYMBOL_CLASS {
                    let Ok(mut s) = symbols(&self.m, &t) else { continue };
                    let mut edited = false;
                    for i in (0..s.ids.len()).rev() {
                        if self.omitted_ids.contains(&(s.ids[i] as i32)) {
                            let w = format!("omitted SymbolClass binding for character {}", s.ids[i]);
                            self.warn(n, &w);
                            s.ids.remove(i);
                            s.names.remove(i);
                            edited = true;
                        }
                    }
                    if edited {
                        self.m.nodes[n].tag.body = write_symbols(&s);
                    }
                    continue;
                }
                let mut ids: Vec<i32> = self.characters[&n].intersection(&self.omitted_ids).copied().collect();
                let mut names: Vec<String> = self.classes[&n].intersection(&self.omitted_classes).cloned().collect();
                if !ids.is_empty() || !names.is_empty() {
                    ids.sort_unstable();
                    names.sort();
                    let list = if !ids.is_empty() { format!("[{}]", ids.iter().map(i32::to_string).collect::<Vec<_>>().join(", ")) } else { format!("[{}]", names.join(", ")) };
                    self.discard(n, &format!("references an omitted definition {list}"));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn skip(&mut self, n: usize, reason: &str) -> Result<()> {
        if !self.m.all().contains(&n) {
            return err(format!("Cannot locate unsupported SWF tag: {reason}"));
        }
        self.discard(n, reason);
        self.prune();
        Ok(())
    }
}

// ---- the Iggy side: object buffers ----

/// `IggyTimelineBuffer`: a block of new objects with their fixups and character references.
struct Buf {
    wide: bool,
    data: Stream,
    fields: Fields,
    characters: Vec<(u64, i32)>,
}

impl Buf {
    fn new(wide: bool) -> Buf {
        let mut data = Stream::new();
        data.w64(0);
        Buf { wide, data, fields: Fields::default(), characters: Vec::new() }
    }

    fn allocate(&mut self, size: u64) -> u64 {
        self.data.pad8();
        let r = self.data.position();
        self.data.write_bytes(&vec![0u8; size as usize]);
        r
    }

    fn value(&mut self, at: u64, v: u64, width: u32, op: u32) {
        let end = self.data.pos;
        let _ = self.data.seek(at);
        match width {
            8 => self.data.w64(v),
            4 => self.data.w32(v & 0xffff_ffff),
            2 => self.data.w16(v as u32 & 0xffff),
            _ => self.data.w8(v as u32 & 0xff),
        }
        self.data.pos = end;
        if op != 0 {
            self.fields.put(field(at, op));
        }
    }

    fn u8(&mut self, at: u64, v: u32) {
        self.value(at, v as u64, 1, 0);
    }

    fn u16(&mut self, at: u64, v: i64) {
        self.value(at, v as u64, 2, 4);
    }

    fn u32(&mut self, at: u64, v: i64) {
        self.value(at, v as u64, 4, 5);
    }

    fn f32(&mut self, at: u64, v: f64) {
        self.u32(at, (v as f32).to_bits() as i64);
    }

    fn pointer(&mut self, at: u64, target: i64) {
        let v = if target < 0 { 1 } else { (target as u64).wrapping_sub(at) };
        self.value(at, v, if self.wide { 8 } else { 4 }, 2);
    }

    fn string(&mut self, at: u64, text: Option<&str>) {
        let Some(t) = text else { return self.pointer(at, -1) };
        let units: Vec<u16> = t.encode_utf16().collect();
        let start = self.allocate((units.len() as u64 + 1) * 2);
        for (i, &u) in units.iter().enumerate() {
            self.u16(start + 2 * i as u64, u as i64);
        }
        self.u16(start + 2 * units.len() as u64, 0);
        self.pointer(at, start as i64);
    }

    fn character(&mut self, at: u64, id: i32) {
        match self.characters.iter_mut().find(|c| c.0 == at) {
            Some(c) => c.1 = id,
            None => self.characters.push((at, id)),
        }
    }

    fn object(self, at: u64, size: u32, ty: u32) -> ObjectData {
        ObjectData { data: self.data.data, fields: self.fields.values(), characters: self.characters, address: at, header_size: size, ty }
    }
}

fn header(b: &mut Buf, id: i32, ty: u32, size: u64) -> R<u64> {
    if id <= 0 || id > 65535 {
        return fail(format!("Invalid Iggy asset character ID {id}"));
    }
    let at = b.allocate(size);
    b.u8(at, ty);
    b.value(at + 1, 255, 1, 12);
    b.u16(at + 2, id as i64);
    b.u32(at + if b.wide { 8 } else { 4 }, 0);
    b.pointer(at + if b.wide { 16 } else { 8 }, -1);
    Ok(at)
}

fn color(b: &mut Buf, at: u64, c: Rgba) {
    b.u8(at, c.r as u32);
    b.u8(at + 1, c.g as u32);
    b.u8(at + 2, c.b as u32);
    b.u8(at + 3, c.a as u32);
}

/// FFDec's MATRIX fields as floats, as `readMatrix` gives them.
fn mx(m: &Matrix) -> (f32, f32, f32, f32, i32, i32) {
    (m.a as f32, m.d as f32, m.b as f32, m.c as f32, m.tx as i32, m.ty as i32)
}

fn matrix(b: &mut Buf, at: u64, m: &Matrix, linear: f64) {
    let (sx, sy, r0, r1, tx, ty) = mx(m);
    b.f32(at, sx as f64 / linear);
    b.f32(at + 4, r1 as f64 / linear);
    b.f32(at + 8, r0 as f64 / linear);
    b.f32(at + 12, sy as f64 / linear);
    b.f32(at + 16, tx as f64 / 20.0);
    b.f32(at + 20, ty as f64 / 20.0);
}

fn fill_type(f: &Fill) -> u32 {
    match f {
        Fill::Solid(_) => 0,
        Fill::Linear(_) => 0x10,
        Fill::Radial(_) => 0x12,
        Fill::Focal(_) => 0x13,
        Fill::Bitmap { repeat, smooth, .. } => match (repeat, smooth) {
            (true, true) => 0x40,
            (false, true) => 0x41,
            (true, false) => 0x42,
            (false, false) => 0x43,
        },
    }
}

/// `IggyShapeStyles.writeFill` from a SWF fill.
fn write_fill(b: &mut Buf, at: u64, f: &Fill) -> R<()> {
    let ty = fill_type(f);
    let value = if b.wide { 16 } else { 8 };
    let refer = at + if b.wide { 64 } else { 56 };
    b.u8(at, ty);
    b.pointer(at + if b.wide { 8 } else { 4 }, -1);
    b.pointer(refer, -1);
    match f {
        Fill::Solid(c) => color(b, at + value, *c),
        Fill::Linear(g) | Fill::Radial(g) | Fill::Focal(g) => {
            matrix(b, at + value, &g.matrix, 20.0);
            let n = g.records.len() as u64;
            if n > 15 {
                return fail("Too many Iggy gradient stops");
            }
            let ga = b.allocate(14 + n * 10);
            b.pointer(refer, ga as i64);
            b.u8(ga, if g.spread == 3 { 0 } else { g.spread as u32 });
            b.u8(ga + 1, g.interpolation as u32);
            b.u8(ga + 2, n as u32);
            for (i, r) in g.records.iter().enumerate() {
                let ra = ga + 12 + 10 * i as u64;
                b.u8(ra + 8, r.ratio as u32);
                color(b, ra, r.color);
            }
            if ty == 0x13 {
                b.f32(ga + 4, g.focal);
            }
        }
        Fill::Bitmap { id, matrix: m, .. } => {
            if *id != 65535 {
                b.character(refer, *id as i32);
            }
            matrix(b, at + value, m, 20.0);
        }
    }
    Ok(())
}

fn write_morph_fill(b: &mut Buf, at: u64, f: &MorphFill) -> R<()> {
    let value = if b.wide { 16 } else { 8 };
    let refer = at + if b.wide { 64 } else { 56 };
    let ty = match f {
        MorphFill::Solid(..) => 0,
        MorphFill::Gradient { kind, .. } => *kind as u32,
        MorphFill::Bitmap { repeat, smooth, .. } => match (repeat, smooth) {
            (true, true) => 0x40,
            (false, true) => 0x41,
            (true, false) => 0x42,
            (false, false) => 0x43,
        },
    };
    b.u8(at, ty);
    b.pointer(at + if b.wide { 8 } else { 4 }, -1);
    b.pointer(refer, -1);
    match f {
        MorphFill::Solid(s, e) => {
            color(b, at + value, *s);
            color(b, at + value + 4, *e);
        }
        MorphFill::Gradient { kind, start, end, gradient: g } => {
            matrix(b, at + value, start, 20.0);
            matrix(b, at + value + 24, end, if *kind == 0x13 { 1.0 } else { 20.0 });
            let n = g.records.len() as u64;
            if n > 15 {
                return fail("Too many Iggy gradient stops");
            }
            let ga = b.allocate(14 + n * 10);
            b.pointer(refer, ga as i64);
            b.u8(ga, if g.spread == 3 { 0 } else { g.spread as u32 });
            b.u8(ga + 1, g.interpolation as u32);
            b.u8(ga + 2, n as u32);
            for (i, &(sr, sc, er, ec)) in g.records.iter().enumerate() {
                let ra = ga + 12 + 10 * i as u64;
                b.u8(ra + 8, sr as u32);
                color(b, ra, sc);
                b.u8(ra + 9, er as u32);
                color(b, ra + 4, ec);
            }
            if *kind == 0x13 {
                b.f32(ga + 4, g.focal.0);
                b.f32(ga + 8, g.focal.1);
            }
        }
        MorphFill::Bitmap { id, start, end, .. } => {
            if *id != 65535 {
                b.character(refer, *id as i32);
            }
            matrix(b, at + value, start, 20.0);
            matrix(b, at + value + 24, end, 20.0);
        }
    }
    Ok(())
}

fn ufixed8(v: f64) -> f64 {
    ((v as f32 * 256.0) as i32 as u16) as f32 as f64 / 256.0
}

fn write_line(b: &mut Buf, at: u64, l: &LineStyle) -> R<()> {
    let width = if b.wide { 76 } else { 64 };
    b.f32(at + width, l.width as f64 / 20.0);
    b.u8(at + width + 8, l.start_cap as u32);
    b.u8(at + width + 10, l.join as u32);
    let flags = l.no_h_scale as u32 | (l.no_v_scale as u32) << 1 | (l.pixel_hinting as u32) << 2 | (l.no_close as u32) << 3;
    b.u8(at + width + 9, l.end_cap as u32);
    b.value(at + width + 11, flags as u64, 1, 3);
    if l.join == 2 {
        b.f32(at + width - 4, ufixed8(l.miter_limit));
    }
    match &l.fill {
        Some(f) => write_fill(b, at, f)?,
        None => {
            b.u8(at, 0);
            b.pointer(at + if b.wide { 8 } else { 4 }, -1);
            b.pointer(at + if b.wide { 64 } else { 56 }, -1);
            color(b, at + if b.wide { 16 } else { 8 }, l.color);
        }
    }
    Ok(())
}

fn write_morph_line(b: &mut Buf, at: u64, l: &MorphLine) -> R<()> {
    let width = if b.wide { 76 } else { 64 };
    b.f32(at + width, l.widths.0 as f64 / 20.0);
    b.f32(at + width + 4, l.widths.1 as f64 / 20.0);
    b.u8(at + width + 8, l.start_cap as u32);
    b.u8(at + width + 10, l.join as u32);
    let flags = l.no_h_scale as u32 | (l.no_v_scale as u32) << 1 | (l.pixel_hinting as u32) << 2 | (l.no_close as u32) << 3;
    b.u8(at + width + 9, l.end_cap as u32);
    b.value(at + width + 11, flags as u64, 1, 3);
    if l.join == 2 {
        b.f32(at + width - 4, ufixed8(l.miter_limit));
    }
    match &l.fill {
        Some(f) => write_morph_fill(b, at, f)?,
        None => {
            b.u8(at, 0);
            b.pointer(at + if b.wide { 8 } else { 4 }, -1);
            b.pointer(at + if b.wide { 64 } else { 56 }, -1);
            color(b, at + if b.wide { 16 } else { 8 }, l.colors.0);
            color(b, at + if b.wide { 20 } else { 12 }, l.colors.1);
        }
    }
    Ok(())
}

// ---- vectors ----

#[derive(Clone, Copy)]
struct GNode {
    x: i32,
    y: i32,
    cx: i32,
    cy: i32,
    ty: u32,
    f0: u32,
    f1: u32,
    line: u32,
}

#[derive(Clone)]
struct Group {
    nodes: Vec<GNode>,
    fills: Vec<Fill>,
    lines: Vec<LineStyle>,
}

fn groups(records: &[Record], fills: Vec<Fill>, lines: Vec<LineStyle>) -> Vec<Group> {
    let mut out = Vec::new();
    let mut g = Group { nodes: Vec::new(), fills, lines };
    let (mut x, mut y, mut f0, mut f1, mut line) = (0, 0, 0, 0, 0);
    let node = |ty, x, y, cx, cy, f0, f1, line| GNode { x, y, cx, cy, ty, f0, f1, line };
    for r in records {
        match r {
            Record::StyleChange { move_to, fill0, fill1, line: l, new_styles } => {
                if let Some((mx_, my)) = move_to {
                    x = *mx_;
                    y = *my;
                    g.nodes.push(node(1, x, y, 0, 0, f0, f1, line));
                }
                if let Some(v) = fill0 {
                    f0 = *v;
                }
                if let Some(v) = fill1 {
                    f1 = *v;
                }
                if let Some(v) = l {
                    line = *v;
                }
                if let Some(s) = new_styles {
                    if !g.nodes.is_empty() {
                        out.push(g);
                    }
                    g = Group { nodes: Vec::new(), fills: s.fills.clone(), lines: s.lines.clone() };
                }
            }
            Record::Straight { dx, dy } => {
                if g.nodes.is_empty() {
                    g.nodes.push(node(1, x, y, 0, 0, f0, f1, line));
                }
                x += dx;
                y += dy;
                g.nodes.push(node(2, x, y, 0, 0, f0, f1, line));
            }
            Record::Curved { cdx, cdy, adx, ady } => {
                if g.nodes.is_empty() {
                    g.nodes.push(node(1, x, y, 0, 0, f0, f1, line));
                }
                let cx = x + cdx;
                let cy = y + cdy;
                x = cx + adx;
                y = cy + ady;
                g.nodes.push(node(3, x, y, cx, cy, f0, f1, line));
            }
        }
    }
    if !g.nodes.is_empty() {
        out.push(g);
    }
    out
}

fn include(b: &mut [f32; 4], n: &GNode, p: &GNode) {
    b[0] = b[0].min(n.x.min(p.x) as f32 / 20.0);
    b[1] = b[1].min(n.y.min(p.y) as f32 / 20.0);
    b[2] = b[2].max(n.x.max(p.x) as f32 / 20.0);
    b[3] = b[3].max(n.y.max(p.y) as f32 / 20.0);
    if n.ty == 3 {
        b[0] = b[0].min(n.cx as f32 / 20.0);
        b[1] = b[1].min(n.cy as f32 / 20.0);
        b[2] = b[2].max(n.cx as f32 / 20.0);
        b[3] = b[3].max(n.cy as f32 / 20.0);
    }
}

fn write_groups(b: &mut Buf, collection: u64, groups: &[Group], morph: Option<&MorphShape>, end: bool) -> R<()> {
    if groups.len() > 32767 {
        return fail("Too many Iggy shape groups");
    }
    let wide = b.wide;
    let count_at = if end { 0 } else if wide { 8 } else { 4 };
    let pointer_at = if end { if wide { 8 } else { 4 } } else if wide { 24 } else { 12 };
    b.u16(collection + count_at, groups.len() as i64);
    b.u8(collection + count_at + 2, 0);
    if !end {
        b.pointer(collection, -1);
        b.value(collection + if wide { 16 } else { 8 }, 0, if wide { 8 } else { 4 }, 8);
    }
    let gs = if wide { 64 } else { 40 };
    let array = if groups.is_empty() { -1 } else { b.allocate(groups.len() as u64 * gs) as i64 };
    b.pointer(collection + pointer_at, array);
    for (gi, g) in groups.iter().enumerate() {
        let at = array as u64 + gs * gi as u64;
        let fc = if end { 0 } else { morph.map_or(g.fills.len(), |m| m.fills.len()) };
        let lc = if end { 0 } else { morph.map_or(g.lines.len(), |m| m.lines.len()) };
        if fc > 32767 || lc > 32767 {
            return fail("Too many Iggy shape styles");
        }
        let fs = if wide { 72 } else { 60 };
        let fill_table = b.allocate(if wide { 8 } else { 4 } + fc as u64 * fs);
        b.pointer(at + if wide { 32 } else { 24 }, fill_table as i64);
        b.u32(fill_table, fc as i64);
        for i in 0..fc {
            let s = fill_table + if wide { 8 } else { 4 } + fs * i as u64;
            match morph {
                None => write_fill(b, s, &g.fills[i])?,
                Some(m) => write_morph_fill(b, s, &m.fills[i])?,
            }
        }
        let ls = if wide { 88 } else { 76 };
        let line_table = b.allocate(if wide { 8 } else { 4 } + lc as u64 * ls);
        b.pointer(at + if wide { 40 } else { 28 }, line_table as i64);
        b.u32(line_table, lc as i64);
        for i in 0..lc {
            let s = line_table + if wide { 8 } else { 4 } + ls * i as u64;
            match morph {
                None => write_line(b, s, &g.lines[i])?,
                Some(m) => write_morph_line(b, s, &m.lines[i])?,
            }
        }
        let nodes = b.allocate(g.nodes.len() as u64 * 24);
        b.pointer(at + 16, nodes as i64);
        b.u32(at + if wide { 24 } else { 20 }, g.nodes.len() as i64);
        let mut total = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        let mut fb = vec![[1e15f32, 1e15, -1e15, -1e15]; fc];
        let mut no_lines = true;
        let mut prev = g.nodes[0];
        for (i, n) in g.nodes.iter().enumerate() {
            let node = nodes + 24 * i as u64;
            if !end && n.ty != 1 && (n.f0 as usize > fc || n.f1 as usize > fc || n.line as usize > lc) {
                return fail("Invalid SWF shape style index");
            }
            b.f32(node, n.x as f64 / 20.0);
            b.f32(node + 4, n.y as f64 / 20.0);
            b.f32(node + 8, n.cx as f64 / 20.0);
            b.f32(node + 12, n.cy as f64 / 20.0);
            b.u8(node + 16, n.ty);
            let (a, c) = if morph.is_none() { (n.f0, n.f1) } else { (n.f1, n.f0) };
            b.u16(node + 18, if n.ty == 1 { 0 } else { a as i64 });
            b.u16(node + 20, if n.ty == 1 { 0 } else { c as i64 });
            b.u16(node + 22, if n.ty == 1 { 0 } else { n.line as i64 });
            no_lines &= n.ty == 1 || n.line == 0;
            include(&mut total, n, n);
            if n.ty == 3 {
                b.u8(collection + count_at + 2, 1);
            }
            if !end && n.ty != 1 && n.f0 != n.f1 {
                if n.f0 != 0 {
                    include(&mut fb[n.f0 as usize - 1], n, &prev);
                }
                if n.f1 != 0 {
                    include(&mut fb[n.f1 as usize - 1], n, &prev);
                }
            }
            prev = *n;
        }
        for (i, v) in total.iter().enumerate() {
            b.f32(at + 4 * i as u64, *v as f64);
        }
        let fba = if fc == 0 { -1 } else { b.allocate(fc as u64 * 16) as i64 };
        b.pointer(at + if wide { 48 } else { 32 }, fba);
        for (i, r) in fb.iter().enumerate() {
            for (j, v) in r.iter().enumerate() {
                b.f32(fba as u64 + 16 * i as u64 + 4 * j as u64, *v as f64);
            }
        }
        b.u8(at + if wide { 56 } else { 36 }, no_lines as u32);
    }
    Ok(())
}

fn bounds(b: &mut Buf, at: u64, r: &Rect) {
    b.f32(at, r.x_min as f64 / 20.0);
    b.f32(at + 4, r.y_min as f64 / 20.0);
    b.f32(at + 8, r.x_max as f64 / 20.0);
    b.f32(at + 12, r.y_max as f64 / 20.0);
}

fn normalize_morph(start: &mut [Group], end: &mut [Group]) -> R<()> {
    let a: Vec<u32> = start.iter().flat_map(|g| g.nodes.iter().filter(|n| n.ty != 1).map(|n| n.ty)).collect();
    let z: Vec<u32> = end.iter().flat_map(|g| g.nodes.iter().filter(|n| n.ty != 1).map(|n| n.ty)).collect();
    if a.len() != z.len() {
        return fail("Morph shape edge counts differ");
    }
    for side in [start, end] {
        let mut edge = 0;
        for g in side.iter_mut() {
            let mut prev = g.nodes[0];
            for n in g.nodes.iter_mut() {
                if n.ty != 1 {
                    if (a[edge] == 3 || z[edge] == 3) && n.ty == 2 {
                        n.ty = 3;
                        n.cx = (prev.x + n.x) / 2;
                        n.cy = (prev.y + n.y) / 2;
                    }
                    edge += 1;
                }
                prev = *n;
            }
        }
    }
    Ok(())
}

fn vector(n: usize, wide: bool, t: &Tag) -> R<ObjectData> {
    let morph = MorphShape::is_morph_code(t.code);
    let ty = if morph { 2 } else { 1 };
    let size = if morph { if wide { 128 } else { 88 } } else if wide { 80 } else { 48 };
    let mut b = Buf::new(wide);
    if !morph {
        let s = ShapeTag::read(t)?;
        let at = header(&mut b, s.id as i32, ty, size)?;
        if s.shape_num == 4 && s.nonzero {
            return unsupported(n, "Iggy does not retain the SWF nonzero fill winding flag");
        }
        bounds(&mut b, at + if wide { 32 } else { 16 }, &s.bounds);
        let gs = groups(&s.shape.records, s.shape.styles.fills.clone(), s.shape.styles.lines.clone());
        write_groups(&mut b, at + if wide { 48 } else { 32 }, &gs, None, false)?;
        return Ok(b.object(at, size as u32, ty));
    }
    let s = MorphShape::read(t)?;
    let at = header(&mut b, s.id as i32, ty, size)?;
    let mut start = groups(&s.start, Vec::new(), Vec::new());
    let mut end = groups(&s.end, Vec::new(), Vec::new());
    normalize_morph(&mut start, &mut end)?;
    bounds(&mut b, at + if wide { 32 } else { 16 }, &s.start_bounds);
    bounds(&mut b, at + if wide { 112 } else { 72 }, &s.end_bounds);
    let u = Rect { x_min: s.start_bounds.x_min.min(s.end_bounds.x_min), x_max: s.start_bounds.x_max.max(s.end_bounds.x_max), y_min: s.start_bounds.y_min.min(s.end_bounds.y_min), y_max: s.start_bounds.y_max.max(s.end_bounds.y_max) };
    bounds(&mut b, at + if wide { 96 } else { 56 }, &u);
    write_groups(&mut b, at + if wide { 48 } else { 32 }, &start, Some(&s), false)?;
    write_groups(&mut b, at + if wide { 80 } else { 48 }, &end, Some(&s), true)?;
    Ok(b.object(at, size as u32, ty))
}

// ---- bitmaps, binary data, sounds ----

fn dims(data: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok()?.into_dimensions().ok()
}

fn zlib(data: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(data).map_err(|e| Error::Other(e.to_string()))?;
    z.finish().map_err(|e| Error::Other(e.to_string()))
}

/// `IggyBitmapConvertor.lossless`: premultiplied ARGB, zlib-compressed.
fn lossless_payload(img: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut d = Vec::with_capacity(img.len());
    for p in img.pixels() {
        let a = p[3] as u32;
        d.push(a as u8);
        for c in [p[0], p[1], p[2]] {
            d.push(((c as u32 * a + 127) / 255) as u8);
        }
    }
    let mut out = vec![5];
    out.extend((img.width() as u16).to_le_bytes());
    out.extend((img.height() as u16).to_le_bytes());
    out.extend(zlib(&d)?);
    Ok(out)
}

fn bitmap(n: usize, m: &Model, wide: bool, t: &Tag) -> R<ObjectData> {
    let id = char_id(t).unwrap() as i32;
    let mut b = Buf::new(wide);
    let size = if wide { 88 } else { 56 };
    let at = header(&mut b, id, 4, size)?;
    let h = if wide { 32 } else { 16 };
    let (mut sx, mut sy, mut ox, mut oy) = (1f32, 1f32, 0f32, 0f32);
    let (width, height, pixel, kind, format, payload);
    let ext = if t.code == IGGY_EXTERNAL_IMAGE { Some(External::read(&t.body)?) } else { None };
    let external = ext.as_ref().is_some_and(|e| e.replacement.is_empty());
    if let (true, Some(e)) = (external, &ext) {
        width = e.width;
        height = e.height;
        pixel = e.pixel;
        kind = e.kind;
        format = e.format;
        [sx, sy, ox, oy] = e.scale;
        payload = Vec::new();
        b.u32(at + if wide { 80 } else { 52 }, e.length as i64);
    } else {
        let mut code = t.code;
        let mut body = t.body.clone();
        if let Some(e) = &ext {
            let img = image::load_from_memory(&e.replacement).map_err(|e| Error::Other(e.to_string()))?.to_rgba8();
            body = (id as u16).to_le_bytes().to_vec();
            body.extend(lossless_payload(&img)?);
            code = DEFINE_BITS_LOSSLESS2;
        }
        if code == DEFINE_BITS {
            let Some(tables) = m.jpeg_tables() else { return unsupported(n, format!("DefineBits character {id} requires JPEGTables")) };
            let bm = crate::export::image::Bitmap::read(t, Some(&tables))?;
            let jpeg = bm.original.map(|o| o.1).unwrap_or_default();
            if crate::export::image::sniff(&jpeg) != Some(crate::export::image::Format::Jpeg) || image::load_from_memory(&jpeg).is_err() {
                return unsupported(n, format!("Cannot convert DefineBits character {id}: missing or invalid JPEG tables/image data"));
            }
            code = DEFINE_BITS_JPEG2;
            body = (id as u16).to_le_bytes().to_vec();
            body.extend(jpeg);
        }
        let jpeg_kind = matches!(code, DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3);
        let image_bytes = |code: u16, body: &[u8]| -> Vec<u8> {
            if code == DEFINE_BITS_JPEG3 {
                let off = u32::from_le_bytes(body[2..6].try_into().unwrap()) as usize;
                body[6..(6 + off).min(body.len())].to_vec()
            } else {
                body[2..].to_vec()
            }
        };
        let is_jpeg = jpeg_kind && crate::export::image::sniff(&image_bytes(code, &body)) == Some(crate::export::image::Format::Jpeg);
        if !matches!(code, DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2) && !is_jpeg {
            let tag = Tag::new(code, body.clone());
            let img = crate::export::image::Bitmap::read(&tag, None)?.pixels()?;
            let mut nb = (id as u16).to_le_bytes().to_vec();
            nb.extend(lossless_payload(&img)?);
            body = nb;
            code = DEFINE_BITS_LOSSLESS2;
        }
        kind = code as u32;
        let (w, hh) = match code {
            DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => (u16_at(&body, 3) as u32, u16_at(&body, 5) as u32),
            _ => dims(&crate::export::image::fix_jpeg(&image_bytes(code, &body))).unwrap_or((0, 0)),
        };
        width = w;
        height = hh;
        pixel = if kind == 20 || kind == 21 { 3 } else { 4 };
        let skip = if kind == 20 || kind == 36 { 7 } else { 2 };
        format = if kind == 20 || kind == 36 { body[2] as u32 } else { 0 };
        payload = body[skip..].to_vec();
        b.u32(at + if wide { 80 } else { 52 }, payload.len() as i64);
    }
    if width > 65535 || height > 65535 || (!external && (width == 0 || height == 0)) {
        return fail("Invalid Iggy bitmap dimensions");
    }
    b.u16(at + h, width as i64);
    b.u16(at + h + 2, height as i64);
    b.u8(at + h + 4, pixel);
    b.u8(at + h + 5, kind);
    b.u8(at + h + 6, format);
    b.value(at + h + 7, 0, 1, 3);
    b.f32(at + if wide { 48 } else { 28 }, sx as f64);
    b.f32(at + if wide { 52 } else { 32 }, sy as f64);
    b.f32(at + if wide { 56 } else { 36 }, ox as f64);
    b.f32(at + if wide { 60 } else { 40 }, oy as f64);
    b.pointer(at + if wide { 64 } else { 44 }, -1);
    let body = if payload.is_empty() { -1 } else { b.allocate(payload.len() as u64) as i64 };
    if body >= 0 {
        for (i, &v) in payload.iter().enumerate() {
            b.u8(body as u64 + i as u64, v as u32);
        }
    }
    b.pointer(at + if wide { 72 } else { 48 }, body);
    Ok(b.object(at, size as u32, 4))
}

fn binary(wide: bool, t: &Tag) -> R<ObjectData> {
    let mut r = Reader::new(&t.body);
    let id = r.u16()? as i32;
    if r.u32()? != 0 {
        return fail("Iggy binary data requires reserved=0");
    }
    let data = r.rest().to_vec();
    let mut b = Buf::new(wide);
    let size = if wide { 48 } else { 24 };
    let at = header(&mut b, id, 20, size)?;
    b.value(at + if wide { 32 } else { 16 }, data.len() as u64, if wide { 8 } else { 4 }, if wide { 6 } else { 5 });
    let body = if data.is_empty() { -1 } else { b.allocate(data.len() as u64) as i64 };
    if !data.is_empty() {
        let end = b.data.pos;
        b.data.pos = body as usize;
        b.data.write_bytes(&data);
        b.data.pos = end;
    }
    b.pointer(at + if wide { 40 } else { 20 }, body);
    Ok(b.object(at, size as u32, 20))
}

const RATES: [i64; 4] = [5500, 11025, 22050, 44100];

fn sound(wide: bool, t: &Tag) -> R<ObjectData> {
    let mut r = Reader::new(&t.body);
    let id = r.u16()? as i32;
    let f = r.u8()?;
    let (format, rate, size16, stereo) = (f >> 4, (f >> 2) & 3, f & 2 != 0, f & 1 != 0);
    let samples = r.u32()?;
    let mut data = r.rest().to_vec();
    if format == 0 && size16 {
        if data.len() % 2 != 0 {
            return fail("Odd Iggy 16-bit PCM payload size");
        }
        for p in data.chunks_mut(2) {
            p.swap(0, 1);
        }
    }
    let mut b = Buf::new(wide);
    let size = if wide { 64 } else { 40 };
    let at = header(&mut b, id, 21, size)?;
    let h = if wide { 32 } else { 16 };
    b.u32(at + h, data.len() as i64 + 7);
    b.u32(at + h + 4, samples as i64);
    let body = if data.is_empty() { -1 } else { b.allocate(data.len() as u64) as i64 };
    if body >= 0 {
        for (i, &v) in data.iter().enumerate() {
            b.u8(body as u64 + i as u64, v as u32);
        }
    }
    b.pointer(at + if wide { 40 } else { 24 }, body);
    b.u32(at + if wide { 48 } else { 28 }, data.len() as i64);
    b.u16(at + if wide { 52 } else { 32 }, RATES[rate as usize]);
    b.u8(at + if wide { 54 } else { 34 }, format as u32);
    b.u8(at + if wide { 55 } else { 35 }, if size16 { 16 } else { 8 });
    b.u8(at + if wide { 56 } else { 36 }, if stereo { 2 } else { 1 });
    Ok(b.object(at, size as u32, 21))
}

// ---- static texts ----

struct TextRecord {
    font: Option<u16>,
    color: Option<[u8; 4]>,
    x: Option<i16>,
    y: Option<i16>,
    height: u16,
    glyphs: Vec<(u32, i32)>,
}

struct StaticText {
    id: u16,
    v2: bool,
    bounds: Rect,
    matrix: Matrix,
    records: Vec<TextRecord>,
}

fn static_text(t: &Tag) -> Result<StaticText> {
    let mut r = Reader::new(&t.body);
    let id = r.u16()?;
    let bounds = Rect::read(&mut r)?;
    let matrix = Matrix::read(&mut r)?;
    let gb = r.u8()? as u32;
    let ab = r.u8()? as u32;
    let v2 = t.code == DEFINE_TEXT2;
    let mut records = Vec::new();
    loop {
        let f = r.u8()?;
        if f == 0 {
            break;
        }
        let font = if f & 8 != 0 { Some(r.u16()?) } else { None };
        let color = if f & 4 != 0 { Some(if v2 { r.bytes(4)?.try_into().unwrap() } else { let c = r.bytes(3)?; [c[0], c[1], c[2], 255] }) } else { None };
        let x = if f & 1 != 0 { Some(r.i16()?) } else { None };
        let y = if f & 2 != 0 { Some(r.i16()?) } else { None };
        let height = if f & 8 != 0 { r.u16()? } else { 0 };
        let n = r.u8()?;
        let mut glyphs = Vec::new();
        for _ in 0..n {
            glyphs.push((r.ub(gb)?, r.sb(ab)?));
        }
        r.align();
        records.push(TextRecord { font, color, x, y, height, glyphs });
    }
    Ok(StaticText { id, v2, bounds, matrix, records })
}

fn static_text_object(wide: bool, t: &Tag) -> R<ObjectData> {
    let s = static_text(t)?;
    let mut b = Buf::new(wide);
    let size = if wide { 80 } else { 60 };
    let at = header(&mut b, s.id as i32, 5, size)?;
    let off = if wide { 32 } else { 16 };
    bounds(&mut b, at + off, &s.bounds);
    matrix(&mut b, at + if wide { 48 } else { 32 }, &s.matrix, 1.0);
    let mut link = at + if wide { 72 } else { 56 };
    let (mut font, mut height, mut x, mut y) = (-1i32, 0u16, 0i32, 0i32);
    let mut c = [255u8; 4];
    for r in &s.records {
        if let Some(f) = r.font {
            font = f as i32;
            height = r.height;
        }
        if let Some(v) = r.x {
            x = v as i32;
        }
        if let Some(v) = r.y {
            y = v as i32;
        }
        if let Some(v) = r.color {
            c = v;
        }
        let n = r.glyphs.len();
        if n > 255 || (font != -1 && font <= 0) || (n != 0 && font == -1) {
            return fail("Invalid Iggy static text record");
        }
        let rec = b.allocate(if wide { 32 } else { 20 } + 4 * n as u64);
        b.pointer(link, rec as i64);
        link = rec + if wide { 24 } else { 16 };
        b.u16(rec, height as i64);
        b.u16(rec + 2, x as i64);
        b.u16(rec + 4, y as i64);
        b.u8(rec + 6, n as u32);
        b.pointer(rec + 8, -1);
        if font >= 0 {
            b.character(rec + 8, font);
        }
        color(&mut b, rec + if wide { 16 } else { 12 }, Rgba { r: c[0], g: c[1], b: c[2], a: c[3] });
        let g = rec + if wide { 32 } else { 20 };
        for (i, &(gi, adv)) in r.glyphs.iter().enumerate() {
            if gi > 65535 || !(-32768..=32767).contains(&adv) {
                return fail("Iggy static text glyph exceeds 16-bit range");
            }
            b.u16(g + 4 * i as u64, gi as i64);
            b.u16(g + 4 * i as u64 + 2, adv as i64);
            x = (x + adv) as i16 as i32;
        }
    }
    b.pointer(link, -1);
    let _ = s.v2;
    Ok(b.object(at, size as u32, 5))
}

// ---- buttons ----

struct ButtonRecord {
    states: u32,
    character: u16,
    depth: u16,
    matrix: Matrix,
    cx: Option<Cx>,
    filters: Option<Vec<Vec<u8>>>,
    blend: Option<u8>,
}

struct Button {
    id: u16,
    v2: bool,
    track: bool,
    records: Vec<ButtonRecord>,
    actions: bool,
}

fn has_actions(b: &[u8]) -> bool {
    !b.is_empty() && !(b.len() == 1 && b[0] == 0)
}

fn button(t: &Tag) -> Result<Button> {
    let mut r = Reader::new(&t.body);
    let id = r.u16()?;
    let v2 = t.code == DEFINE_BUTTON2;
    let mut track = false;
    let mut offset = 0;
    if v2 {
        track = r.u8()? & 1 != 0;
        offset = r.u16()?;
    }
    let mut records = Vec::new();
    loop {
        let f = r.u8()?;
        if f == 0 {
            break;
        }
        let character = r.u16()?;
        let depth = r.u16()?;
        let matrix = Matrix::read(&mut r)?;
        let cx = if v2 { Some(read_cx(&mut r, true)?) } else { None };
        let filters = if f & 0x10 != 0 { Some(filter_list(&mut r)?) } else { None };
        let blend = if f & 0x20 != 0 { Some(r.u8()?) } else { None };
        records.push(ButtonRecord { states: (f & 8 != 0) as u32 | ((f & 1 != 0) as u32) << 1 | ((f & 2 != 0) as u32) << 2 | ((f & 4 != 0) as u32) << 3, character, depth, matrix, cx, filters, blend });
    }
    let mut actions = false;
    if !v2 {
        actions = has_actions(r.rest());
    } else if offset > 0 {
        loop {
            let size = r.u16()? as usize;
            let rest = r.clone().rest();
            let body = if size == 0 { rest } else { &rest[..size.saturating_sub(2).min(rest.len())] };
            if body.len() > 2 && has_actions(&body[2..]) {
                actions = true;
            }
            if size == 0 {
                break;
            }
            r.bytes(size - 2)?;
        }
    }
    Ok(Button { id, v2, track, records, actions })
}

fn write_cx(b: &mut Buf, at: u64, c: Option<&Cx>, alpha: bool) {
    b.u8(at, alpha as u32);
    b.u8(at + 1, c.is_some() as u32);
    b.u8(at + 2, c.is_some() as u32);
    for i in 0..4 {
        let (m, a) = c.map_or((256, 0), |c| (c.multi(i), c.addi(i)));
        b.f32(at + 4 + 4 * i as u64, m as f64 / 256.0);
        b.u16(at + 20 + 2 * i as u64, a as i64);
    }
}

fn button_object(n: usize, m: &Model, wide: bool, t: &Tag) -> R<ObjectData> {
    let bt = button(t)?;
    if bt.actions {
        return unsupported(n, "Iggy does not store AVM1 button actions");
    }
    let legacy = !bt.v2;
    let mut records: Vec<&ButtonRecord> = bt.records.iter().collect();
    if records.len() > 32767 {
        return fail("Too many Iggy button records");
    }
    records.sort_by_key(|r| r.depth);
    let mut b = Buf::new(wide);
    let size = if wide { 88 } else { 60 };
    let at = header(&mut b, bt.id as i32, 7, size)?;
    let mut global = None;
    if legacy {
        for &x in &m.root {
            let tt = m.tag(x);
            if tt.code == DEFINE_BUTTON_CXFORM && u16_at(&tt.body, 0) == bt.id {
                let mut r = Reader::new(&tt.body[2..]);
                global = Some(read_cx(&mut r, false)?);
            }
        }
    }
    write_cx(&mut b, at + if wide { 40 } else { 20 }, global.as_ref(), false);
    b.u16(at + if wide { 68 } else { 48 }, records.len() as i64);
    b.u8(at + if wide { 70 } else { 50 }, (!legacy && bt.track) as u32);
    b.u8(at + if wide { 71 } else { 51 }, global.is_some() as u32);
    let stride = if wide { 72 } else { 64 };
    let array = if records.is_empty() { -1 } else { b.allocate(stride * records.len() as u64) as i64 };
    b.pointer(at + if wide { 72 } else { 52 }, array);
    let mut max_depth = 0;
    for (i, rec) in records.iter().enumerate() {
        if rec.character == 0 {
            return fail("Invalid Iggy button character ID or depth");
        }
        if rec.states == 0 {
            return fail("Iggy button record has no states");
        }
        let r = array as u64 + stride * i as u64;
        b.value(r, rec.states as u64, 1, 3);
        let blend = if legacy { None } else { rec.blend };
        if blend.is_some_and(|v| v as usize >= to_swf::BLEND.len()) {
            return fail("Invalid SWF button blend mode");
        }
        b.u8(r + 1, blend.map_or(15, |v| to_swf::BLEND[v as usize]));
        let filters = if legacy { None } else { rec.filters.as_ref() };
        if filters.is_some_and(|f| f.len() > 255) {
            return fail("Invalid Iggy button filter list");
        }
        let count = filters.map_or(0, Vec::len);
        b.u8(r + 2, count as u32);
        b.u16(r + 4, rec.character as i64);
        b.u16(r + 6, rec.depth as i64);
        write_cx(&mut b, r + 8, if legacy { None } else { rec.cx.as_ref() }, true);
        matrix(&mut b, r + 36, &rec.matrix, 1.0);
        let fs = if wide { 56 } else { 44 };
        let fa = if count == 0 { -1 } else { b.allocate(count as u64 * fs) as i64 };
        b.pointer(r + if wide { 64 } else { 60 }, fa);
        if let Some(f) = filters {
            for (j, d) in f.iter().enumerate() {
                write_filter(&mut b, fa as u64 + fs * j as u64, d)?;
            }
        }
        if rec.states & 14 != 0 {
            max_depth = max_depth.max(rec.depth as i64);
        }
    }
    b.u32(at + if wide { 80 } else { 56 }, max_depth + 1);
    Ok(b.object(at, size as u32, 7))
}

/// `IggyTimelineFilters.write` from a SWF FILTER.
fn write_filter(b: &mut Buf, at: u64, d: &[u8]) -> R<()> {
    let mut r = Reader::new(d);
    let id = r.u8()?;
    b.u8(at, id as u32);
    let fixed = |r: &mut Reader| -> Result<f64> { Ok(r.i32()? as f64 / 65536.0) };
    let uf8 = |r: &mut Reader| -> Result<f64> { Ok((r.u16()? as f32 / 256.0) as f64) };
    let rgba = |r: &mut Reader| -> Result<Rgba> { Rgba::read_rgba(r) };
    match id {
        0 | 2 | 4 => {
            if id == 4 {
                let n = r.u8()? as u64;
                b.u8(at + 3, n as u32);
                let colors = b.allocate(4 * n);
                let ratios = b.allocate(n);
                b.pointer(at + if b.wide { 32 } else { 28 }, colors as i64);
                b.pointer(at + if b.wide { 40 } else { 32 }, ratios as i64);
                for i in 0..n {
                    color(b, colors + 4 * i, rgba(&mut r)?);
                }
                for i in 0..n {
                    b.u8(ratios + i, r.u8()? as u32);
                }
            } else {
                color(b, at + 4, rgba(&mut r)?);
            }
            b.f32(at + 8, fixed(&mut r)?);
            b.f32(at + 12, fixed(&mut r)?);
            if id != 2 {
                b.f32(at + if id == 0 { 16 } else { 20 }, fixed(&mut r)?);
                b.f32(at + if id == 0 { 20 } else { 24 }, fixed(&mut r)?);
            }
            b.f32(at + if id == 0 { 24 } else { 16 }, uf8(&mut r)?);
            let mut flags = r.ub(1)? | r.ub(1)? << 1;
            r.ub(1)?;
            if id == 4 {
                flags |= r.ub(1)? << 2;
            }
            b.value(at + 2, flags as u64, 1, 3);
            b.u8(at + 1, r.ub(if id == 4 { 4 } else { 5 })?);
        }
        1 => {
            b.f32(at + 4, fixed(&mut r)?);
            b.f32(at + 8, fixed(&mut r)?);
            b.u8(at + 1, r.ub(5)?);
        }
        3 | 7 => {
            if id == 3 {
                color(b, at + 8, rgba(&mut r)?);
                color(b, at + 12, rgba(&mut r)?);
            } else {
                let n = r.u8()? as u64;
                b.u8(at + 5, n as u32);
                let colors = b.allocate(4 * n);
                let ratios = b.allocate(n);
                b.pointer(at + if b.wide { 40 } else { 36 }, colors as i64);
                b.pointer(at + if b.wide { 48 } else { 40 }, ratios as i64);
                for i in 0..n {
                    color(b, colors + 4 * i, rgba(&mut r)?);
                }
                for i in 0..n {
                    b.u8(ratios + i, r.u8()? as u32);
                }
            }
            for i in 0..4 {
                b.f32(at + 16 + 4 * i, fixed(&mut r)?);
            }
            b.f32(at + 32, uf8(&mut r)?);
            b.u8(at + 2, r.ub(1)?);
            b.u8(at + 3, r.ub(1)?);
            r.ub(1)?;
            b.u8(at + 4, r.ub(1)?);
            b.u8(at + 1, r.ub(4)?);
        }
        5 => {
            let x = r.u8()? as u64;
            let y = r.u8()? as u64;
            b.u8(at + 1, x as u32);
            b.u8(at + 2, y as u32);
            b.f32(at + 4, r.f32()? as f64);
            b.f32(at + 8, r.f32()? as f64);
            let mx_ = b.allocate(4 * x * y);
            b.pointer(at + 16, mx_ as i64);
            for i in 0..x * y {
                b.f32(mx_ + 4 * i, r.f32()? as f64);
            }
            color(b, at + 12, rgba(&mut r)?);
            r.ub(6)?;
            let conv = r.ub(1)? | r.ub(1)? << 1;
            b.value(at + 3, conv as u64, 1, 3);
        }
        6 => {
            const ORDER: [u64; 20] = [0, 1, 2, 3, 16, 4, 5, 6, 7, 17, 8, 9, 10, 11, 18, 12, 13, 14, 15, 19];
            let cm = b.allocate(80);
            b.pointer(at + if b.wide { 8 } else { 4 }, cm as i64);
            for &i in &ORDER {
                let v = r.f32()?;
                b.f32(cm + 4 * i, (v / if i >= 16 { 255.0 } else { 1.0 }) as f64);
            }
        }
        _ => return fail(format!("Unsupported SWF timeline filter: {id}")),
    }
    Ok(())
}

// ---- timelines ----

fn is_command(code: u16) -> bool {
    matches!(code, PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4 | REMOVE_OBJECT | REMOVE_OBJECT2 | SHOW_FRAME | FRAME_LABEL | START_SOUND | START_SOUND2)
}

/// `IggyTimelineConvertor.fromSwf`.
fn timeline(m: &Model, nodes: &[usize], wide: bool, frames: u64) -> R<Replacement> {
    let mut b = Buf::new(wide);
    let p = if wide { 8 } else { 4 };
    let mut actual = 0u64;
    let mut max_depth = 0i64;
    let (mut first, mut previous) = (-1i64, -1i64);
    let mut placements = Vec::new();
    let mut owners: Vec<Option<u64>> = Vec::new();
    let mut lifetimes: HashMap<u16, u64> = HashMap::new();
    let mut matrices: HashMap<u16, Matrix> = HashMap::new();
    let mut labels = Vec::new();
    for &n in nodes {
        let t = m.tag(n);
        if !is_command(t.code) {
            continue;
        }
        let ty = match t.code {
            SHOW_FRAME => 1,
            REMOVE_OBJECT | REMOVE_OBJECT2 => 3,
            FRAME_LABEL => 5,
            START_SOUND | START_SOUND2 => 6,
            _ => 2,
        };
        let size = match ty {
            2 => if wide { 96 } else { 64 },
            6 => if wide { 56 } else { 40 },
            3 | 5 => if wide { 24 } else { 12 },
            _ => if wide { 16 } else { 8 },
        };
        let at = b.allocate(size);
        if first < 0 {
            first = at as i64;
        }
        if previous >= 0 {
            b.pointer(previous as u64, at as i64);
        }
        previous = at as i64;
        b.pointer(at, -1);
        b.u8(at + p, ty);
        match ty {
            1 => actual += 1,
            2 => {
                let mut pl = place(m, t)?;
                if m.nodes[n].no_visible {
                    pl.visible = None;
                }
                if m.nodes[n].no_background {
                    pl.background = false;
                }
                if pl.clip_actions {
                    return unsupported(n, "Iggy does not store SWF clip actions");
                }
                if pl.visible.is_some_and(|v| v != 1) || pl.background {
                    return unsupported(n, "Iggy does not store timeline visibility or opaque background");
                }
                let depth = pl.depth;
                max_depth = max_depth.max(depth as i64);
                let character = if pl.v3 && pl.has_class { -1 } else { pl.character };
                let has_char = character >= 0;
                let class = if pl.v3 && pl.has_class { pl.class.clone() } else { None };
                let explicit = pl.v3 && pl.has_class;
                let mut mtx = pl.matrix;
                if let Some(mm) = mtx {
                    matrices.insert(depth, mm);
                } else if has_char || explicit {
                    mtx = matrices.get(&depth).copied();
                }
                let cache = pl.cache;
                let flags = mtx.is_some() as u32 | (pl.cx.is_some() as u32) << 1 | (pl.moves as u32) << 2 | (pl.image as u32) << 3 | (cache as u32) << 4 | (explicit as u32) << 5 | (pl.name.is_some() as u32) << 6 | ((pl.clip_depth >= 0) as u32) << 7;
                b.value(at + p + 1, flags as u64, 1, 3);
                b.u16(at + if wide { 40 } else { 36 }, depth as i64);
                b.u32(at + p + 4, 65536);
                let cf = at + if wide { 48 } else { 40 };
                b.pointer(cf, -1);
                if has_char {
                    b.character(cf, character);
                }
                if has_char || class.is_some() {
                    if let Some(old) = lifetimes.insert(depth, at) {
                        b.u32(old + p + 4, actual as i64);
                    }
                }
                placements.push(at);
                owners.push(lifetimes.get(&depth).copied());
                if let Some(mm) = &mtx {
                    let (sx, sy, r0, r1, tx, ty_) = mx(mm);
                    let o = if wide { 16 } else { 12 };
                    b.f32(at + o, sx as f64);
                    b.f32(at + o + 4, r1 as f64);
                    b.f32(at + o + 8, r0 as f64);
                    b.f32(at + o + 12, sy as f64);
                    b.f32(at + o + 16, tx as f64 / 20.0);
                    b.f32(at + o + 20, ty_ as f64 / 20.0);
                }
                b.string(at + if wide { 64 } else { 48 }, pl.name.as_deref());
                b.string(at + if wide { 80 } else { 56 }, class.as_deref());
                b.pointer(at + if wide { 72 } else { 52 }, -1);
                if let Some(c) = &pl.cx {
                    let ca = b.allocate(28);
                    b.pointer(at + if wide { 72 } else { 52 }, ca as i64);
                    b.u8(ca, 1);
                    b.u8(ca + 1, 1);
                    b.u8(ca + 2, 1);
                    for i in 0..4 {
                        b.f32(ca + 4 + 4 * i as u64, c.multi(i) as f64 / 256.0);
                        b.u16(ca + 20 + 2 * i as u64, c.addi(i) as i64);
                    }
                }
                b.u16(at + if wide { 56 } else { 44 }, pl.ratio.max(0) as i64);
                b.u16(at + if wide { 58 } else { 46 }, pl.clip_depth.max(0) as i64);
                if pl.clip_depth >= 0 {
                    max_depth = max_depth.max(pl.clip_depth as i64);
                }
                let blend = if pl.v3 { pl.blend.map_or(-1, |v| v as i32) } else { -1 };
                if blend >= to_swf::BLEND.len() as i32 {
                    return fail("Invalid SWF blend mode");
                }
                b.u8(at + p + 3, if blend < 0 { 15 } else { to_swf::BLEND[blend as usize] });
                let filters = if pl.v3 { pl.filters.as_ref() } else { None };
                let count = filters.map_or(127, Vec::len);
                if count > 126 && filters.is_some() {
                    return fail("Too many Iggy filters");
                }
                b.u8(at + p + 2, count as u32);
                b.pointer(at + if wide { 88 } else { 60 }, -1);
                if let Some(f) = filters.filter(|f| !f.is_empty()) {
                    let fs = if wide { 56 } else { 44 };
                    let da = b.allocate(f.len() as u64 * fs);
                    b.pointer(at + if wide { 88 } else { 60 }, da as i64);
                    for (i, d) in f.iter().enumerate() {
                        write_filter(&mut b, da + fs * i as u64, d)?;
                    }
                }
            }
            3 => {
                let depth = if t.code == REMOVE_OBJECT { u16_at(&t.body, 2) } else { u16_at(&t.body, 0) };
                matrices.remove(&depth);
                max_depth = max_depth.max(depth as i64);
                b.u16(at + p + 2, depth as i64);
                b.pointer(at + if wide { 16 } else { 8 }, -1);
                if let Some(old) = lifetimes.remove(&depth) {
                    b.u32(old + p + 4, actual as i64);
                }
            }
            5 => {
                let mut r = Reader::new(&t.body);
                let name = m.string(&cstring(&mut r)?);
                let anchor = !r.is_empty() && r.u8()? == 1;
                labels.push(NamedFrame { name: name.clone(), frame: actual, reserved: 0 });
                b.u8(at + p + 1, anchor as u32);
                b.string(at + if wide { 16 } else { 8 }, Some(&name));
            }
            _ => {
                let mut r = Reader::new(&t.body);
                let by_class = t.code == START_SOUND2;
                let (id, class) = if by_class { (0, Some(m.string(&cstring(&mut r)?))) } else { (r.u16()?, None) };
                let info = sound_info(&mut r)?;
                let start = if wide { 12 } else { 8 };
                b.u16(at + start, id as i64);
                b.u8(at + start + 2, info.sync_stop as u32);
                b.u8(at + start + 3, info.no_multiple as u32);
                b.string(at + if wide { 16 } else { 12 }, class.as_deref());
                b.u32(at + if wide { 24 } else { 16 }, info.inp.unwrap_or(0) as i64);
                b.u32(at + if wide { 28 } else { 20 }, info.out.unwrap_or(0x7fff_ffff) as i64);
                b.u32(at + if wide { 36 } else { 28 }, info.loops.map_or(1, |v| v as i64));
                let env = info.envelope.unwrap_or_default();
                b.u32(at + if wide { 40 } else { 32 }, env.len() as i64);
                b.pointer(at + if wide { 48 } else { 36 }, -1);
                if !env.is_empty() {
                    let da = b.allocate(env.len() as u64 * 8);
                    b.pointer(at + if wide { 48 } else { 36 }, da as i64);
                    for (i, &(pos, l, rr)) in env.iter().enumerate() {
                        b.u32(da + 8 * i as u64, pos as i64);
                        b.u16(da + 8 * i as u64 + 4, l as i64);
                        b.u16(da + 8 * i as u64 + 6, rr as i64);
                    }
                }
            }
        }
    }
    while actual < frames {
        let at = b.allocate(if wide { 16 } else { 8 });
        if first < 0 {
            first = at as i64;
        }
        if previous >= 0 {
            b.pointer(previous as u64, at as i64);
        }
        previous = at as i64;
        b.pointer(at, -1);
        b.u8(at + p, 1);
        actual += 1;
    }
    let snapshot = b.data.data.clone();
    for (i, &at) in placements.iter().enumerate() {
        if let Some(o) = owners[i] {
            if o != at {
                let o = (o + p + 4) as usize;
                b.u32(at + p + 4, u32::from_le_bytes(snapshot[o..o + 4].try_into().unwrap()) as i64);
            }
        }
    }
    Ok(Replacement { data: b.data.data, fields: b.fields.values(), characters: b.characters, first, last: previous, max_depth: (max_depth + 1) as i32, frame_count: frames as i32, labels })
}

/// `IggyTimelineConvertor.restoreClassReferences`.
fn restore_classes(movie: &Movie, original: &Timeline, r: Replacement) -> Result<Replacement> {
    let wide = original.is64;
    let p = if wide { 8 } else { 4 };
    let src = movie.original_data();
    let mut classes: HashMap<String, i32> = HashMap::new();
    let mut images: HashMap<i32, Option<String>> = HashMap::new();
    for c in &original.commands {
        if c.ty != 2 {
            continue;
        }
        let at = c.address;
        let flags = c.data[p as usize + 1];
        let f = at + if wide { 48 } else { 40 };
        let id = movie.dictionary_id(f, reference(src, f, wide)?)?;
        let class_ref = at + if wide { 80 } else { 56 };
        if flags & 32 != 0 && id >= 0 && !movie.is_definition_removed(id) {
            let name = wstring_at(src, reference(src, class_ref, wide)?)?.unwrap_or_default();
            if classes.get(&name).is_some_and(|&v| v != id) {
                classes.insert(name, -1);
            } else {
                classes.insert(name, id);
            }
        } else if flags & 8 != 0 && id >= 0 {
            let name = wstring_at(src, reference(src, class_ref, wide)?)?;
            match images.get(&id) {
                Some(v) if *v != name => {
                    images.insert(id, None);
                }
                _ => {
                    images.insert(id, name);
                }
            }
        }
    }
    for l in movie.imports() {
        for s in &l.symbols {
            classes.insert(s.name.clone(), s.character_id as i32);
        }
    }
    for s in movie.symbol_classes() {
        if s.character_id != 0 {
            classes.insert(s.name.clone(), s.character_id as i32);
        }
    }
    let mut characters = r.characters.clone();
    let mut fields = r.fields.clone();
    let mut data = Stream::new();
    data.write_bytes(&r.data);
    let rebuilt = Timeline::read(&r.data, if r.first < 0 { 0 } else { r.first as u64 }, wide)?;
    for c in &rebuilt.commands {
        if c.ty != 2 {
            continue;
        }
        let at = c.address;
        let flags = c.data[p as usize + 1];
        let f = at + if wide { 48 } else { 40 };
        let class_field = at + if wide { 80 } else { 56 };
        if flags & 32 != 0 {
            let name = wstring_at(&r.data, reference(&r.data, class_field, wide)?)?.unwrap_or_default();
            if let Some(&id) = classes.get(&name) {
                if !characters.iter().any(|x| x.0 == f) && id >= 0 {
                    characters.push((f, id));
                }
            }
        } else if flags & 8 != 0 {
            let ch = characters.iter().find(|x| x.0 == f).map(|x| x.1);
            if let Some(Some(name)) = ch.and_then(|id| images.get(&id)) {
                data.pad8();
                let target = data.position();
                data.write_wchar(name);
                for i in 0..=name.encode_utf16().count() as u64 {
                    fields.push(field(target + 2 * i, 4));
                }
                let end = data.pos;
                data.pos = class_field as usize;
                if wide {
                    data.w64(target - class_field);
                } else {
                    data.w32(target - class_field);
                }
                data.pos = end;
            }
        }
    }
    Ok(Replacement { data: data.data, fields, characters, ..r })
}

fn update_timeline(m: &Model, movie: &mut Movie, id: i32, nodes: &[usize], frames: u64) -> R<()> {
    let wide = movie.is64;
    let original = if id == 0 { Some(movie.timeline.clone()) } else { movie.sprite_timelines.iter().find(|e| e.0 == id).map(|e| e.1.clone()) };
    let Some(original) = original else {
        let r = timeline(m, nodes, wide, frames)?;
        let r = restore_classes(movie, &Timeline { commands: Vec::new(), is64: wide }, r)?;
        movie.replace_timeline(id, Some(r))?;
        return Ok(());
    };
    let r = timeline(m, nodes, original.is64, frames)?;
    let old_frames = movie.timeline_frame_count(id)?;
    let baseline = baseline_timeline(movie, &original, old_frames)?;
    if r.frame_count != baseline.frame_count || r.data != baseline.data || r.characters != baseline.characters {
        let r = restore_classes(movie, &original, r)?;
        movie.replace_timeline(id, Some(r))?;
    } else {
        movie.replace_timeline(id, None)?;
    }
    Ok(())
}

/// The original timeline taken through SWF and back, the domain edits are compared in.
fn baseline_timeline(movie: &Movie, original: &Timeline, frames: u64) -> R<Replacement> {
    let tags = to_swf::timeline_tags(movie, original, frames)?;
    let mut m = Model { nodes: Vec::new(), root: Vec::new(), version: 10, rect: Rect::default(), rate: 0.0, frames: 0 };
    let ids = m.add(&tags, "main");
    timeline(&m, &ids, original.is64, frames)
}

// ---- fonts and texts ----

fn glyph_from_records(records: &[Record], units: f32) -> Shape {
    let b = shape::bounds(records, &[], 1, false);
    let mut nodes = Vec::new();
    let mut first = true;
    let (mut x, mut y) = (0i32, 0i32);
    for r in records {
        match r {
            Record::Straight { dx, dy } => {
                x += dx;
                y += dy;
                nodes.push(ShapeNode::new(x as f32 / units, y as f32 / units, 0.0, 0.0, 2, 0, first));
            }
            Record::Curved { cdx, cdy, adx, ady } => {
                let (cx, cy) = (x + cdx, y + cdy);
                x = cx + adx;
                y = cy + ady;
                nodes.push(ShapeNode::new(x as f32 / units, y as f32 / units, cx as f32 / units, cy as f32 / units, 3, 0, first));
            }
            Record::StyleChange { move_to, .. } => {
                if let Some((mx_, my)) = move_to {
                    x = *mx_;
                    y = *my;
                    nodes.push(ShapeNode::new(x as f32 / units, y as f32 / units, 0.0, 0.0, 1, 0, first));
                }
            }
        }
        first = false;
    }
    Shape::new(b.x_min as f32 / units, b.y_min as f32 / units, b.x_max as f32 / units, b.y_max as f32 / units, nodes)
}

struct FontData {
    v: u8,
    glyphs: Vec<Vec<Record>>,
    codes: Option<Vec<u16>>,
    layout: Option<crate::font_edit::Layout>,
    name: String,
    language: u8,
    shift_jis: bool,
    small: bool,
    ansi: bool,
    bold: bool,
    italic: bool,
}

fn font_data(m: &Model, n: usize) -> R<FontData> {
    let t = m.tag(n);
    match t.code {
        DEFINE_FONT2 | DEFINE_FONT3 => {
            let f = Font2::read(t.code, &t.body)?;
            Ok(FontData {
                v: if t.code == DEFINE_FONT3 { 3 } else { 2 },
                glyphs: f.glyphs,
                codes: Some(f.codes),
                layout: if f.flags & 0x80 != 0 { f.layout } else { None },
                name: m.string(&f.name),
                language: f.language,
                shift_jis: f.flags & 0x40 != 0,
                small: f.flags & 0x20 != 0,
                ansi: f.flags & 0x10 != 0,
                bold: f.flags & 1 != 0,
                italic: f.flags & 2 != 0,
            })
        }
        DEFINE_FONT => {
            let mut r = Reader::new(&t.body);
            let id = r.u16()?;
            let base = 2;
            let first = r.u16()? as usize;
            let count = first / 2;
            let mut offsets = vec![first];
            for _ in 1..count {
                offsets.push(r.u16()? as usize);
            }
            let mut glyphs = Vec::new();
            for o in offsets {
                glyphs.push(shape::read_glyph(t.body.get(base + o..).unwrap_or(&[]))?);
            }
            let mut fd = FontData { v: 1, glyphs, codes: None, layout: None, name: String::new(), language: 0, shift_jis: false, small: false, ansi: false, bold: false, italic: false };
            let mut info = None;
            for x in m.all() {
                let it = m.tag(x);
                if matches!(it.code, DEFINE_FONT_INFO | DEFINE_FONT_INFO2) && u16_at(&it.body, 0) == id {
                    if info.is_some() {
                        return fail(format!("Duplicate font info for Iggy font {id}"));
                    }
                    info = Some(it.clone());
                }
            }
            if let Some(it) = info {
                let mut r = Reader::new(&it.body);
                r.u16()?;
                let len = r.u8()? as usize;
                fd.name = m.string(r.bytes(len)?);
                let f = r.u8()?;
                fd.small = f & 0x20 != 0;
                fd.shift_jis = f & 0x10 != 0;
                fd.ansi = f & 8 != 0;
                fd.italic = f & 4 != 0;
                fd.bold = f & 2 != 0;
                let wide = f & 1 != 0;
                if it.code == DEFINE_FONT_INFO2 {
                    fd.language = r.u8()?;
                }
                let mut codes = Vec::new();
                while !r.is_empty() {
                    codes.push(if wide { r.u16()? } else { r.u8()? as u16 });
                }
                fd.codes = Some(codes);
            }
            Ok(fd)
        }
        _ => unsupported(n, format!("Unsupported Iggy embedded font tag {}", t.code)),
    }
}

fn update_font(m: &Model, n: usize, font: &mut Font) -> R<()> {
    let fd = font_data(m, n)?;
    let count = fd.glyphs.len();
    let legacy = fd.v == 1;
    let layout = !legacy && fd.layout.is_some();
    let units = if fd.v == 3 { 20480.0 } else { 1024.0 };
    if count > 65535 || fd.codes.as_ref().is_some_and(|c| c.len() != count) || (!legacy && fd.codes.is_none()) {
        return fail("Invalid Iggy font code table size");
    }
    if let Some(l) = &fd.layout {
        if layout && l.advances.len() != count {
            return fail("Invalid Iggy font layout table size");
        }
    }
    if font.char_offsets.is_some() && !legacy {
        let original = to_swf::font_tag(font)?;
        let t = m.tag(n);
        if original.code == t.code && original.body == t.body {
            return Ok(());
        }
    }
    let mut glyphs = Vec::new();
    let mut offsets = Vec::new();
    let mut bounds = [0f32; 5];
    let mut have = false;
    for (i, g) in fd.glyphs.iter().enumerate() {
        let glyph = glyph_from_records(g, units);
        let glyph = if glyph.nodes.is_empty() { None } else { Some(glyph) };
        let curve = glyph.as_ref().is_some_and(|g| g.nodes.iter().any(|n| n.node_type == 3));
        offsets.push(CharOffset { ischar1: glyph.is_some() as u32, ischar2: curve as u32, ..Default::default() });
        if let Some(g) = &glyph {
            if !have {
                bounds[..4].copy_from_slice(&[g.minx, g.miny, g.maxx, g.maxy]);
                have = true;
            } else {
                bounds[0] = bounds[0].min(g.minx);
                bounds[1] = bounds[1].min(g.miny);
                bounds[2] = bounds[2].max(g.maxx);
                bounds[3] = bounds[3].max(g.maxy);
            }
        }
        glyphs.push(glyph);
        let _ = i;
    }
    font.set_font_flags(fd.v as u32, fd.language as u32, layout, fd.shift_jis, fd.small, fd.ansi, fd.bold, fd.italic);
    font.char_count = count as u64;
    font.char_count2 = count as u32;
    let l = fd.layout.as_ref().filter(|_| layout);
    font.ascent = l.map_or(0, |l| l.ascent as u32);
    font.descent = l.map_or(0, |l| l.descent as u32);
    font.leading = l.map_or(0, |l| l.leading as i32);
    font.name = Some(fd.name.clone());
    font.unk_float = bounds;
    font.glyphs = glyphs;
    font.char_offsets = Some(offsets);
    font.code_points = fd.codes.map(|c| (c, 0));
    font.advances = l.map(|l| l.advances.iter().map(|&a| a as f32 / units).collect());
    font.set_kernings(l.filter(|l| !l.kerning.is_empty()).map(|l| l.kerning.clone()));
    Ok(())
}

fn update_text(text: &mut Text, e: &EditText) {
    text.bounds = [(e.bounds.x_min as f64 / 20.0) as f32, (e.bounds.y_min as f64 / 20.0) as f32, (e.bounds.x_max as f64 / 20.0) as f32, (e.bounds.y_max as f64 / 20.0) as f32];
    let f = e.flags;
    let mut flags = text.flags & !SWF_BEHAVIOR_MASK;
    for (on, bit) in [(f[1], FLAG_WORD_WRAP), (f[2], FLAG_MULTILINE), (f[3], FLAG_PASSWORD), (f[4], FLAG_READ_ONLY), (f[9], FLAG_AUTO_SIZE), (f[11], FLAG_NO_SELECT), (f[12], FLAG_BORDER), (f[13], FLAG_WAS_STATIC), (f[14], FLAG_HTML), (f[15], FLAG_USE_OUTLINES)] {
        if on {
            flags |= bit;
        }
    }
    text.flags = flags;
    text.initial_text = if e.has_text { Some(e.text.clone()) } else { None };
    text.font_index = e.font.map_or(0, |f| f as u32);
    text.set_font_class_name(e.font_class.clone());
    if e.font.is_some() || e.font_class.is_some() {
        text.font_height = e.height as u32;
    }
    text.color = e.color.unwrap_or([255; 4]);
    text.max_length = e.max_length.map_or(0, |v| v as u32);
    let (al, lm, rm, ind, ld) = e.layout.unwrap_or((0, 0, 0, 0, 0));
    text.align = al as u32;
    text.left_margin = lm as u32;
    text.right_margin = rm as u32;
    text.indent = ind as u32;
    text.leading = ld as i32;
}

// ---- the update ----

fn validate_alchemy(m: &Model) -> Result<()> {
    let mut index = 0;
    for n in m.all() {
        let t = m.tag(n);
        if !matches!(t.code, DO_ABC | DO_ABC2) {
            continue;
        }
        let Some((data, _)) = crate::abc::tag_abc(t) else { continue };
        let Ok(abc) = crate::abc::Abc::parse(data) else { continue };
        for body in &abc.bodies {
            let Ok(code) = crate::abc::opcodes::decode(&body.code, Default::default()) else { continue };
            for i in &code {
                if crate::abc::opcodes::def(i.code).is_some_and(|d| d.flags & crate::abc::opcodes::DOMAIN_MEMORY != 0) {
                    return err(format!("Cannot convert SWF to IGGY: Alchemy instruction {} in {} (ABC {index}, method {}, offset {}) is not supported", i.name, t.name(), body.method, i.offset));
                }
            }
        }
        index += 1;
    }
    Ok(())
}

fn validate_frames(m: &Model, nodes: &[usize], frames: u16) -> Result<()> {
    let mut count = 0;
    for &n in nodes {
        let t = m.tag(n);
        if t.code == SHOW_FRAME {
            count += 1;
        }
        if t.code == DEFINE_SPRITE {
            validate_frames(m, &m.nodes[n].children, u16_at(&t.body, 2))?;
        }
    }
    if count > frames as u32 {
        return err("Invalid timeline frame count in a new Iggy");
    }
    Ok(())
}

fn validate_new_movie(m: &Model) -> R<()> {
    if !m.rate.is_finite() || m.rate <= 0.0 {
        return fail("Invalid movie properties for a new Iggy");
    }
    validate_frames(m, &m.root, m.frames)?;
    let ctx = Ctx::new(m);
    let mut available: HashSet<i32> = HashSet::new();
    for n in m.all() {
        let t = m.tag(n);
        if let Some(id) = char_id(t) {
            available.insert(id as i32);
        }
        if matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            if let Ok((_, _, _, a)) = imports(m, t) {
                available.extend(a.iter().map(|x| x.0 as i32));
            }
        }
    }
    for n in m.all() {
        let t = m.tag(n);
        let (mut ids, _) = needed(m, &ctx, n);
        ids.remove(&0);
        if let Some(id) = ids.iter().filter(|i| !available.contains(i)).min() {
            return fail(format!("Missing character in a new Iggy: {id}"));
        }
        if is_character(t) || supports_new_tag(t) {
            continue;
        }
        if !matches!(t.code, 24 | 41 | 58 | 63 | 64 | 65 | 77) {
            let name = tag_name(m, &ctx, n);
            return unsupported(n, format!("Unsupported tag in a new Iggy: {name}"));
        }
    }
    Ok(())
}

fn validate_definitions(m: &Model, movie: &Movie) -> R<Vec<(u16, usize)>> {
    let mut defs: Vec<(u16, usize)> = Vec::new();
    let mut available: HashSet<i32> = HashSet::new();
    let mut classes: HashMap<String, u16> = HashMap::new();
    for n in m.all() {
        let t = m.tag(n);
        if let Some(id) = char_id(t) {
            if id == 0 || defs.iter().any(|d| d.0 == id) {
                return fail(format!("Invalid or duplicate Iggy character ID {id}"));
            }
            defs.push((id, n));
            let Some(kind) = def_type(t.code) else { return unsupported(n, format!("Unsupported Iggy character definition: {}", t.name())) };
            if let Some(before) = movie.dictionary.get(id as usize) {
                let bt = before.ty as u32;
                if !before.is_null() && kind != bt && (matches!(kind, 3 | 6 | 22) || matches!(bt, 3 | 6 | 22)) {
                    return fail(format!("Iggy character ID is occupied by another definition type: {id}"));
                }
            }
            available.insert(id as i32);
        }
        if matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            if let Ok((_, _, _, a)) = imports(m, t) {
                for (i, name) in a {
                    available.insert(i as i32);
                    classes.insert(name, i);
                }
            }
        }
        if t.code == SYMBOL_CLASS {
            if let Ok(s) = symbols(m, t) {
                for (i, name) in s.ids.iter().zip(s.names) {
                    classes.insert(name, *i);
                }
            }
        }
    }
    let mut removed: HashSet<i32> = HashSet::new();
    for e in &movie.dictionary {
        if e.id > 0 && !e.is_null() && !available.contains(&e.id) {
            if matches!(e.ty, 1..=7 | 20..=22) {
                removed.insert(e.id);
            } else {
                available.insert(e.id);
            }
        }
    }
    let mut removed_classes: HashSet<String> = HashSet::new();
    for s in movie.original_symbol_classes() {
        let id = s.character_id as i32;
        if id != 0 && !available.contains(&id) && !classes.contains_key(&s.name) {
            removed_classes.insert(s.name.clone());
        }
    }
    let src = movie.original_data();
    for &id in &removed {
        let f = movie.dictionary[id as usize].address + if movie.is64 { 16 } else { 8 };
        let v = read_ptr(src, f, movie.is64);
        if v != 0 && v != 1 {
            if let Some(name) = wstring_at(src, reference(src, f, movie.is64)?)? {
                if !name.is_empty() && !classes.contains_key(&name) {
                    removed_classes.insert(name);
                }
            }
        }
    }
    let ctx = Ctx::new(m);
    for n in m.all() {
        let (mut ids, names) = needed(m, &ctx, n);
        let t = m.tag(n);
        if is_modifier(t.code) && !is_character(t) {
            ids.insert(u16_at(&t.body, 0) as i32);
        }
        ids.remove(&0);
        if let Some(id) = ids.iter().find(|i| removed.contains(i)) {
            return fail(format!("Missing or removed Iggy character is still referenced: {id}"));
        }
        if let Some(name) = names.iter().find(|s| removed_classes.contains(*s)) {
            return fail(format!("Removed Iggy class is still referenced: {name}"));
        }
    }
    Ok(defs)
}

fn read_ptr(src: &[u8], f: u64, wide: bool) -> i64 {
    let n = if wide { 8 } else { 4 };
    let Some(b) = src.get(f as usize..f as usize + n) else { return 0 };
    if wide {
        i64::from_le_bytes(b.try_into().unwrap())
    } else {
        i32::from_le_bytes(b.try_into().unwrap()) as i64
    }
}

fn update_imports(m: &Model, movie: &mut Movie) -> R<()> {
    let mut libs = Vec::new();
    let mut imported: HashSet<i32> = HashSet::new();
    for &n in &m.root {
        let t = m.tag(n);
        if !matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            continue;
        }
        let (url, dl, dg, a) = imports(m, t)?;
        if t.code == IMPORT_ASSETS2 && (dl != 1 || dg != 0) {
            return unsupported(n, "Iggy imports require downloadNow=1 and hasDigest=0");
        }
        let mut symbols = Vec::new();
        for (id, name) in a {
            if id == 0 || !imported.insert(id as i32) {
                return fail(format!("Invalid or duplicate Iggy import character ID {id}"));
            }
            symbols.push(SymbolClass { object_address: 0, name, character_id: id as u64, reserved: 0 });
        }
        libs.push(Import { library: url, symbols });
    }
    let mut removed: HashSet<i32> = HashSet::new();
    for l in movie.original_imports() {
        for s in &l.symbols {
            if !imported.contains(&(s.character_id as i32)) {
                removed.insert(s.character_id as i32);
            }
        }
    }
    let ctx = Ctx::new(m);
    validate_import_tags(m, &ctx, &m.root, &imported, &removed, false)?;
    movie.set_imports(libs)?;
    Ok(())
}

fn validate_import_tags(m: &Model, ctx: &Ctx, nodes: &[usize], imported: &HashSet<i32>, removed: &HashSet<i32>, nested: bool) -> R<()> {
    for &n in nodes {
        let t = m.tag(n);
        if nested && matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            return fail("Iggy library imports must be on the root timeline");
        }
        if let Some(id) = char_id(t) {
            if imported.contains(&(id as i32)) {
                return fail(format!("Iggy import character ID collides with a local definition: {id}"));
            }
        }
        if !removed.is_empty() {
            let (ids, _) = needed(m, ctx, n);
            if let Some(id) = ids.iter().find(|i| removed.contains(i)) {
                return fail(format!("Removed Iggy import is still referenced: {id}"));
            }
        }
        if t.code == DEFINE_SPRITE {
            validate_import_tags(m, ctx, &m.nodes[n].children, imported, removed, true)?;
        }
    }
    Ok(())
}

fn asset_object(n: usize, m: &Model, wide: bool, t: &Tag) -> R<ObjectData> {
    match t.code {
        c if shape::is_shape(c) || MorphShape::is_morph_code(c) => vector(n, wide, t),
        DEFINE_BUTTON | DEFINE_BUTTON2 => button_object(n, m, wide, t),
        DEFINE_TEXT | DEFINE_TEXT2 => static_text_object(wide, t),
        DEFINE_BINARY_DATA => binary(wide, t),
        DEFINE_SOUND => sound(wide, t),
        _ => bitmap(n, m, wide, t),
    }
}

fn asset_supported(ty: i32) -> bool {
    matches!(ty, 1 | 2 | 4 | 5 | 7 | 20 | 21)
}

/// The original asset taken through SWF and back, to tell edits from round trips.
fn baseline_asset(file: Option<&IggyFile>, movie: &Movie, entry: &super::swf::DictEntry) -> R<ObjectData> {
    let tag = to_swf::asset_tag(file, movie, entry)?;
    let mut m = Model { nodes: Vec::new(), root: Vec::new(), version: 10, rect: Rect::default(), rate: 0.0, frames: 0 };
    let ids = m.add(std::slice::from_ref(&tag), "main");
    asset_object(ids[0], &m, movie.is64, &tag)
}

fn update_assets(m: &Model, movie: &mut Movie) -> R<()> {
    movie.clear_asset_changes();
    let mut seen = HashSet::new();
    for n in m.all() {
        let t = m.tag(n);
        if t.code == DEFINE_BUTTON_SOUND {
            return fail("Iggy does not store DefineButtonSound; use timeline or AS3 sounds");
        }
        let Some(id) = char_id(t) else { continue };
        if !matches!(def_type(t.code), Some(1 | 2 | 4 | 5 | 7 | 20 | 21)) {
            continue;
        }
        if !seen.insert(id) {
            return fail(format!("Duplicate Iggy asset ID {id}"));
        }
        if matches!(t.code, DEFINE_BUTTON | DEFINE_BUTTON2) {
            let chars = m.characters();
            let imported = imported_ids(m);
            for r in &button(t)?.records {
                if !chars.contains_key(&r.character) && !imported.contains(&r.character) {
                    return fail(format!("Missing Iggy button character {}", r.character));
                }
            }
        }
        if matches!(t.code, DEFINE_TEXT | DEFINE_TEXT2) {
            let fonts: HashSet<u16> = m.all().iter().filter(|&&x| matches!(m.tag(x).code, DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3 | DEFINE_FONT4)).filter_map(|&x| char_id(m.tag(x))).chain(imported_ids(m)).collect();
            let mut font = -1i32;
            for r in &static_text(t)?.records {
                if let Some(f) = r.font {
                    font = f as i32;
                }
                if (r.font.is_some() || !r.glyphs.is_empty()) && (font < 0 || !fonts.contains(&(font as u16))) {
                    return fail(format!("Missing Iggy static text font {font}"));
                }
            }
        }
        let entry = movie.dictionary.get(id as usize).copied().filter(|_| id > 0);
        if let Some(e) = entry {
            if !e.is_null() && !asset_supported(e.ty) {
                return fail(format!("Iggy asset ID is occupied by another object type: {id}"));
            }
        }
        let replacement = asset_object(n, m, movie.is64, t)?;
        let mut changed = true;
        if let Some(e) = entry.filter(|e| asset_supported(e.ty)) {
            let base = baseline_asset(None, movie, &e)?;
            changed = replacement.data != base.data || replacement.characters != base.characters;
        }
        let rty = replacement.ty;
        if changed {
            movie.replace_object(id as i32, replacement)?;
        }
        if changed && entry.is_some_and(|e| e.ty == 4) && rty != 4 {
            movie.replace_texture_record(id as u32, None);
        }
        let ext = if t.code == IGGY_EXTERNAL_IMAGE { Some(External::read(&t.body)?) } else { None };
        if let Some(e) = ext.filter(|e| e.replacement.is_empty()) {
            if m.string(&e.file_name) != movie.texture_resource_name {
                return fail("Iggy stores external texture IDs, not filenames; the companion filename cannot be changed in the movie");
            }
            let (ty, offset, f, w, h) = e.resource;
            if ty == 8 && offset != u32::MAX {
                let mut rec = offset.to_le_bytes().to_vec();
                for v in [f, w, h, id] {
                    rec.extend(v.to_le_bytes());
                }
                movie.replace_texture_record(id as u32, Some(rec));
            }
        } else if def_type(t.code) == Some(4) && changed {
            movie.replace_texture_record(id as u32, None);
        }
    }
    Ok(())
}

fn imported_ids(m: &Model) -> Vec<u16> {
    let mut v = Vec::new();
    for n in m.all() {
        let t = m.tag(n);
        if matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            if let Ok((_, _, _, a)) = imports(m, t) {
                v.extend(a.iter().map(|x| x.0));
            }
        }
    }
    v
}

fn update_definitions(m: &Model, movie: &mut Movie, defs: &[(u16, usize)]) -> R<()> {
    let ids: HashSet<i32> = defs.iter().map(|d| d.0 as i32).collect();
    movie.set_active_definitions(&ids);
    let wide = movie.is64;
    for &(id, n) in defs {
        if m.tag(n).code != DEFINE_SPRITE || movie.sprite_timelines.iter().any(|e| e.0 == id as i32) {
            continue;
        }
        let mut b = Buf::new(wide);
        let size = if wide { 72 } else { 36 };
        let at = header(&mut b, id as i32, 3, size)?;
        for o in if wide { [32, 48, 56, 64] } else { [16, 24, 28, 32] } {
            b.pointer(at + o, -1);
        }
        movie.replace_object(id as i32, b.object(at, size as u32, 3))?;
    }
    Ok(())
}

fn update_movie_tags(m: &Model, movie: &mut Movie) -> R<()> {
    let mut grids: Vec<(i32, [f32; 4])> = Vec::new();
    for n in m.all() {
        let t = m.tag(n);
        if t.code == DEFINE_SCALING_GRID {
            let id = u16_at(&t.body, 0) as i32;
            if grids.iter().any(|g| g.0 == id) {
                return fail("Duplicate Iggy scaling grid");
            }
            let mut r = Reader::new(&t.body[2..]);
            let rc = Rect::read(&mut r)?;
            grids.push((id, [rc.x_min as f32 / 20.0, rc.y_min as f32 / 20.0, rc.x_max as f32 / 20.0, rc.y_max as f32 / 20.0]));
        }
    }
    let mut available: HashSet<i32> = HashSet::new();
    for n in m.all() {
        let t = m.tag(n);
        if let Some(id) = char_id(t) {
            available.insert(id as i32);
        }
    }
    available.extend(imported_ids(m).into_iter().map(|i| i as i32));
    movie.set_scaling_grids(&grids)?;
    let mut scenes_tag = None;
    let mut names: Vec<(u16, String)> = Vec::new();
    let mut as3 = false;
    for &n in &m.root {
        let t = m.tag(n);
        if matches!(t.code, DO_ABC | DO_ABC2) || (t.code == FILE_ATTRIBUTES && t.body.first().is_some_and(|b| b & 8 != 0)) {
            as3 = true;
        }
        if t.code == DEFINE_SCENE_AND_FRAME_LABEL_DATA {
            if scenes_tag.is_some() {
                return fail("Duplicate Iggy scene metadata tag");
            }
            scenes_tag = Some(t.clone());
        }
        if t.code == SYMBOL_CLASS {
            let s = symbols(m, t)?;
            for (id, name) in s.ids.iter().zip(s.names) {
                if (*id != 0 && !available.contains(&(*id as i32))) || names.iter().any(|x| x.0 == *id) {
                    return fail(format!("Invalid or duplicate Iggy SymbolClass character ID {id}"));
                }
                names.push((*id, name));
            }
        }
    }
    movie.set_as3(as3)?;
    let original_ids: HashSet<u64> = movie.original_symbol_classes().iter().map(|s| s.character_id).collect();
    let mut intrinsic = HashSet::new();
    let texts: Vec<u32> = movie.texts.iter().map(|t| t.text_index).collect();
    for (i, ti) in texts.iter().enumerate() {
        if movie.has_original_text_symbol_class(*ti) {
            if !original_ids.contains(&(*ti as u64)) {
                intrinsic.insert(*ti as u16);
            }
            let name = names.iter().find(|x| x.0 as u32 == *ti).map(|x| x.1.clone());
            movie.texts[i].set_symbol_class_name(name);
        }
    }
    let symbols: Vec<SymbolClass> = names.iter().filter(|x| !intrinsic.contains(&x.0)).map(|(id, name)| SymbolClass { object_address: 0, name: name.clone(), character_id: *id as u64, reserved: 0 }).collect();
    movie.set_symbol_classes(symbols)?;
    let (mut scenes, mut labels) = (Vec::new(), Vec::new());
    if let Some(t) = scenes_tag {
        let mut r = Reader::new(&t.body);
        let n = r.encoded_u32()?;
        for _ in 0..n {
            let frame = r.encoded_u32()? as u64;
            scenes.push(NamedFrame { frame, name: m.string(&cstring(&mut r)?), reserved: 0 });
        }
        let n = r.encoded_u32()?;
        for _ in 0..n {
            let frame = r.encoded_u32()? as u64;
            labels.push(NamedFrame { frame, name: m.string(&cstring(&mut r)?), reserved: 0 });
        }
    }
    movie.set_named_frames(scenes, labels)?;
    Ok(())
}

fn update_fonts(m: &Model, movie: &mut Movie) -> R<()> {
    let mut fonts: Vec<(u16, usize)> = Vec::new();
    for n in m.all() {
        let t = m.tag(n);
        if matches!(t.code, DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3 | DEFINE_FONT4) {
            let id = char_id(t).unwrap();
            if fonts.iter().any(|f| f.0 == id) {
                return fail(format!("Duplicate Iggy font ID {id}"));
            }
            fonts.push((id, n));
        }
    }
    let h32 = movie.hdr.is_h32();
    let keys: Vec<i32> = fonts.iter().map(|f| f.0 as i32).collect();
    let fonts: Vec<(u16, usize)> = java_hash_order(&keys).into_iter().map(|i| fonts[i]).collect();
    for (id, n) in fonts {
        match movie.fonts.iter().position(|f| f.font_id == id as u32) {
            Some(i) => {
                let mut f = std::mem::replace(&mut movie.fonts[i], Font::create(true, false, 0)?);
                let r = update_font(m, n, &mut f);
                movie.fonts[i] = f;
                r?;
            }
            None => {
                let mut f = Font::create(movie.is64, h32, id as u32)?;
                update_font(m, n, &mut f)?;
                movie.replace_definition(Obj::Font(&mut f))?;
            }
        }
    }
    Ok(())
}

/// The iteration order of a `java.util.HashMap<Integer, _>` filled with `keys` in order.
fn java_hash_order(keys: &[i32]) -> Vec<usize> {
    let mut cap = 16usize;
    while keys.len() > cap * 3 / 4 {
        cap *= 2;
    }
    let bucket = |k: i32| ((k ^ ((k as u32) >> 16) as i32) as usize) & (cap - 1);
    let mut idx: Vec<usize> = (0..keys.len()).collect();
    idx.sort_by_key(|&i| bucket(keys[i]));
    idx
}

/// `SwfToIggyConvertor.updateIggy` on the movie block.
fn update_movie(m: &Model, movie: &mut Movie) -> R<()> {
    validate_alchemy(m)?;
    let m = &expand_aliases(Some(movie), m.clone())?;
    validate_definitions(m, movie)?;
    update_imports(m, movie)?;
    update_assets(m, movie)?;
    let defs = validate_definitions(m, movie)?;
    update_definitions(m, movie, &defs)?;
    update_movie_tags(m, movie)?;
    update_timeline(m, movie, 0, &m.root, m.frames as u64)?;
    let mut sprites: Vec<i32> = Vec::new();
    for n in m.all() {
        let t = m.tag(n);
        if t.code == DEFINE_SPRITE {
            let id = u16_at(&t.body, 0) as i32;
            if sprites.contains(&id) {
                return fail("Duplicate Iggy sprite ID");
            }
            sprites.push(id);
            update_timeline(m, movie, id, &m.nodes[n].children, u16_at(&t.body, 2) as u64)?;
        }
    }
    let mut a: Vec<i32> = sprites.clone();
    let mut b = movie.sprite_ids();
    a.sort_unstable();
    b.sort_unstable();
    if a != b {
        return fail("Missing original Iggy sprite definitions");
    }
    let mut color = movie.hdr.background_color();
    for &n in &m.root {
        let t = m.tag(n);
        if t.code == SET_BACKGROUND_COLOR && t.body.len() >= 3 {
            color = (color & 0xff00_0000) | t.body[0] as u64 | (t.body[1] as u64) << 8 | (t.body[2] as u64) << 16;
        }
    }
    let r = |v: i32| jround(v as f64 / 20.0);
    movie.hdr.set_movie_properties((r(m.rect.x_min), r(m.rect.y_min), r(m.rect.x_max), r(m.rect.y_max)), m.rate, m.frames as u64, color)?;
    update_fonts(m, movie)?;
    let mut abcs = Vec::new();
    for n in m.all() {
        let t = m.tag(n).clone();
        if t.code == DEFINE_EDIT_TEXT {
            let e = edit_text(m, &t)?;
            match movie.texts.iter().position(|x| x.text_index == e.id as u32) {
                Some(i) => update_text(&mut movie.texts[i], &e),
                None => {
                    let mut text = Text::create(movie.is64, e.id as u32)?;
                    update_text(&mut text, &e);
                    movie.replace_definition(Obj::Text(&text))?;
                }
            }
        }
        if matches!(t.code, DO_ABC | DO_ABC2) {
            if let Some((data, _)) = crate::abc::tag_abc(&t) {
                abcs.push(match crate::abc::Abc::parse(data) {
                    Ok(a) => a.to_bytes(),
                    Err(_) => data.to_vec(),
                });
            }
        }
    }
    for (i, a) in abcs.iter().enumerate() {
        if i < movie.abc_blocks.len() {
            movie.abc_blocks[i].data = a.clone();
        } else {
            movie.abc_blocks.push(DeclStrings::new(a));
        }
    }
    movie.abc_blocks.truncate(abcs.len());
    Ok(())
}

/// `IggyDefinitionConvertor.expandSymbolAliases`: a definition bound to several classes is
/// copied under a new ID for each extra binding.
fn expand_aliases(movie: Option<&Movie>, mut m: Model) -> R<Model> {
    let all = m.all();
    let eligible: HashSet<u16> = all.iter().filter(|&&n| is_character(m.tag(n)) && supports_new_tag(m.tag(n))).filter_map(|&n| char_id(m.tag(n))).collect();
    let mut bound = HashSet::new();
    let mut aliases = false;
    for &n in &m.root {
        if m.tag(n).code == SYMBOL_CLASS {
            for id in symbols(&m, m.tag(n))?.ids {
                if id != 0 && !bound.insert(id) && eligible.contains(&id) {
                    aliases = true;
                }
            }
        }
    }
    if !aliases {
        return Ok(m);
    }
    let defs = m.characters();
    let mut used: HashSet<u16> = HashSet::from([0]);
    for &n in &all {
        let t = m.tag(n);
        if let Some(id) = char_id(t) {
            used.insert(id);
        }
        if matches!(t.code, IMPORT_ASSETS | IMPORT_ASSETS2) {
            used.extend(imports(&m, t)?.3.iter().map(|x| x.0));
        }
        if t.code == SYMBOL_CLASS {
            used.extend(symbols(&m, t)?.ids);
        }
    }
    let mut previous: HashMap<String, u16> = HashMap::new();
    let mut occupied = used.clone();
    if let Some(mv) = movie {
        for e in &mv.dictionary {
            if !e.is_null() {
                occupied.insert(e.id as u16);
            }
        }
        for s in mv.symbol_classes() {
            previous.insert(s.name.clone(), s.character_id as u16);
        }
    }
    bound.clear();
    let mut next = 1u32;
    for n in m.root.clone() {
        if m.tag(n).code != SYMBOL_CLASS {
            continue;
        }
        let mut s = symbols(&m, m.tag(n))?;
        for i in 0..s.ids.len() {
            let id = s.ids[i];
            if bound.insert(id) || !eligible.contains(&id) {
                continue;
            }
            let original = defs[&id];
            let reuse = previous.get(&s.names[i]).copied();
            let alias = match (reuse, movie) {
                (Some(r), Some(mv)) if !used.contains(&r) && r > 0 && (r as usize) < mv.dictionary.len() && mv.dictionary[r as usize].ty as u32 == def_type(m.tag(original).code).unwrap_or(0) => r,
                _ => {
                    while next <= 65535 && occupied.contains(&(next as u16)) {
                        next += 1;
                    }
                    if next > 65535 {
                        return fail(format!("No free Iggy character ID for SymbolClass alias {id}"));
                    }
                    next += 1;
                    (next - 1) as u16
                }
            };
            let pos = m.root.iter().position(|&x| x == n).unwrap();
            let copy = clone_definition(&mut m, original, alias);
            m.root.insert(pos, copy);
            for &x in &all {
                let t = m.tag(x).clone();
                if is_modifier(t.code) && !is_character(&t) && u16_at(&t.body, 0) == id {
                    let mut d = t.clone();
                    d.body[0..2].copy_from_slice(&alias.to_le_bytes());
                    let nid = m.nodes.len();
                    m.nodes.push(Node { tag: d, loc: m.nodes[x].loc.clone(), children: Vec::new(), no_visible: false, no_background: false });
                    let pos = m.root.iter().position(|&y| y == n).unwrap();
                    m.root.insert(pos, nid);
                }
            }
            s.ids[i] = alias;
            used.insert(alias);
            occupied.insert(alias);
        }
        m.nodes[n].tag.body = write_symbols(&s);
    }
    Ok(m)
}

fn clone_definition(m: &mut Model, n: usize, id: u16) -> usize {
    let mut node = m.nodes[n].clone();
    node.tag.body[0..2].copy_from_slice(&id.to_le_bytes());
    if node.tag.code == DEFINE_SPRITE {
        let kids: Vec<usize> = node.children.iter().copied().filter(|&c| is_command(m.tag(c).code)).collect();
        let mut copies = Vec::new();
        for c in kids {
            let k = m.nodes[c].clone();
            m.nodes.push(Node { children: Vec::new(), ..k });
            copies.push(m.nodes.len() - 1);
        }
        node.children = copies;
    }
    m.nodes.push(node);
    m.nodes.len() - 1
}

fn create_once(m: &Model, variant: Variant, name: &str) -> R<IggyFile> {
    validate_new_movie(m)?;
    let mut file = IggyFile::create(variant, name)?;
    file.header.swf_version = m.version as u64;
    update_movie(m, &mut file.swf)?;
    let bytes = file.save()?;
    Ok(IggyFile::read(&bytes, None)?)
}

/// `createIggy` with a warning handler: unsupported tags are skipped and reported.
pub fn create(swf: &Swf, variant: Variant, name: &str) -> Result<(IggyFile, Vec<String>)> {
    let m = Model::new(swf);
    into_result(validate_alchemy(&m).map_err(Fail::from))?;
    let m = into_result(expand_aliases(None, m))?;
    let mut filter = Filter::new(m, variant);
    loop {
        match create_once(&filter.m, variant, name) {
            Ok(f) => return Ok((f, filter.warnings)),
            Err(Fail::Tag(n, reason)) => filter.skip(n, &reason)?,
            Err(Fail::Err(e)) => return Err(e),
        }
    }
}

/// `updateIggy`: the SWF's changes written back into the Iggy file it came from.
pub fn update(file: &mut IggyFile, swf: &Swf) -> Result<()> {
    let m = Model::new(swf);
    let previous = file.swf_version();
    into_result(update_movie(&m, &mut file.swf))?;
    if m.version != previous {
        file.header.swf_version = m.version as u64;
    }
    Ok(())
}

impl MorphShape {
    fn is_morph_code(code: u16) -> bool {
        crate::morph::is_morph(code)
    }
}

