//! Display lists and the character renderer: shapes, morph shapes, sprites, buttons,
//! texts, bitmaps and video placeholders, drawn into SVG.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::export::font::Font;
use crate::export::image::Bitmap;
use crate::filters::{read_filter_list, Filter};
use crate::io::Reader;
use crate::morph::MorphShape;
use crate::shape::{contours, ShapeTag};
use crate::tags::{code::*, Tag};
use crate::types::{ColorTransform, Matrix, Rect, Rgba};
use crate::Swf;

use super::paths::{self, Paths};
use super::svg::{self, blend_style, esc, filters_def, matrix_attr, Bitmaps, Doc, ShapeStyle};

const MAX_DEPTH: u8 = 24;

/// One object on a display list.
#[derive(Debug, Clone)]
pub struct Placed {
    pub depth: u16,
    pub id: u16,
    pub matrix: Matrix,
    pub cxform: ColorTransform,
    pub ratio: u16,
    pub clip_depth: u16,
    pub name: Option<String>,
    pub blend: u8,
    pub filters: Vec<Filter>,
    pub visible: bool,
    /// Frame of the parent timeline the object was placed on.
    pub since: usize,
}

pub type DisplayList = BTreeMap<u16, Placed>;

struct PlaceTag {
    depth: u16,
    id: Option<u16>,
    is_move: bool,
    matrix: Option<Matrix>,
    cxform: Option<ColorTransform>,
    ratio: Option<u16>,
    name: Option<String>,
    clip_depth: Option<u16>,
    filters: Option<Vec<Filter>>,
    blend: Option<u8>,
    visible: Option<bool>,
}

fn read_place(tag: &Tag) -> Result<PlaceTag> {
    let mut r = Reader::new(&tag.body);
    if tag.code == PLACE_OBJECT {
        let id = r.u16()?;
        let depth = r.u16()?;
        let matrix = Matrix::read(&mut r)?;
        let cxform = if !r.is_empty() { ColorTransform::read(&mut r, false).ok() } else { None };
        return Ok(PlaceTag {
            depth,
            id: Some(id),
            is_move: false,
            matrix: Some(matrix),
            cxform,
            ratio: None,
            name: None,
            clip_depth: None,
            filters: None,
            blend: None,
            visible: None,
        });
    }
    let f1 = r.u8()?;
    let f2 = if tag.code != PLACE_OBJECT2 { r.u8()? } else { 0 };
    let depth = r.u16()?;
    let has_char = f1 & 0x02 != 0;
    if f2 & 0x08 != 0 || (f2 & 0x10 != 0 && has_char) {
        r.string()?;
    }
    let id = if has_char { Some(r.u16()?) } else { None };
    let matrix = if f1 & 0x04 != 0 { Some(Matrix::read(&mut r)?) } else { None };
    let cxform = if f1 & 0x08 != 0 { Some(ColorTransform::read(&mut r, true)?) } else { None };
    let ratio = if f1 & 0x10 != 0 { Some(r.u16()?) } else { None };
    let name = if f1 & 0x20 != 0 { Some(r.string()?) } else { None };
    let clip_depth = if f1 & 0x40 != 0 { Some(r.u16()?) } else { None };
    let filters = if f2 & 0x01 != 0 { read_filter_list(&mut r).ok() } else { None };
    let blend = if f2 & 0x02 != 0 { r.u8().ok() } else { None };
    if f2 & 0x04 != 0 {
        let _ = r.u8();
    }
    let visible = if f2 & 0x20 != 0 { r.u8().ok().map(|v| v != 0) } else { None };
    Ok(PlaceTag { depth, id, is_move: f1 & 0x01 != 0, matrix, cxform, ratio, name, clip_depth, filters, blend, visible })
}

fn apply_tag(list: &mut DisplayList, tag: &Tag, frame: usize) {
    match tag.code {
        PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4 => {
            let Ok(p) = read_place(tag) else { return };
            let existing = list.get(&p.depth).cloned();
            // Like FFDec: a plain place needs a free depth and a move needs an occupied one.
            let mut obj = match (p.is_move, p.id, existing) {
                (false, Some(id), None) => Placed {
                    depth: p.depth,
                    id,
                    matrix: Matrix::IDENTITY,
                    cxform: ColorTransform::IDENTITY,
                    ratio: 0,
                    clip_depth: 0,
                    name: None,
                    blend: 0,
                    filters: Vec::new(),
                    visible: true,
                    since: frame,
                },
                (true, Some(id), Some(mut e)) => {
                    e.id = id;
                    e
                }
                (true, None, Some(e)) => e,
                _ => return,
            };
            if let Some(m) = p.matrix {
                obj.matrix = m;
            }
            if let Some(c) = p.cxform {
                obj.cxform = c;
            }
            if let Some(r) = p.ratio {
                obj.ratio = r;
            }
            if let Some(n) = p.name {
                obj.name = Some(n);
            }
            if let Some(c) = p.clip_depth {
                obj.clip_depth = c;
            }
            if let Some(f) = p.filters {
                obj.filters = f;
            }
            if let Some(b) = p.blend {
                obj.blend = b;
            }
            if let Some(v) = p.visible {
                obj.visible = v;
            }
            list.insert(p.depth, obj);
        }
        REMOVE_OBJECT => {
            if tag.body.len() >= 4 {
                list.remove(&u16::from_le_bytes([tag.body[2], tag.body[3]]));
            }
        }
        REMOVE_OBJECT2 => {
            if tag.body.len() >= 2 {
                list.remove(&u16::from_le_bytes([tag.body[0], tag.body[1]]));
            }
        }
        _ => {}
    }
}

/// Number of frames in a tag list (ShowFrames, plus a trailing partial frame).
pub fn frame_count(tags: &[Tag]) -> usize {
    let shows = tags.iter().filter(|t| t.code == SHOW_FRAME).count();
    let tail = tags.iter().rev().take_while(|t| t.code != SHOW_FRAME).any(|t| t.code != END);
    (shows + tail as usize).max(1)
}

/// Calls `f` with the display list of every frame in order.
pub fn for_each_frame(tags: &[Tag], mut f: impl FnMut(usize, &DisplayList)) {
    let mut list = DisplayList::new();
    let mut frame = 0;
    let mut pending = false;
    for t in tags {
        if t.code == SHOW_FRAME {
            f(frame, &list);
            frame += 1;
            pending = false;
        } else if t.code != END {
            apply_tag(&mut list, t, frame);
            pending = true;
        }
    }
    if pending || frame == 0 {
        f(frame, &list);
    }
}

pub fn display_list(tags: &[Tag], frame: usize) -> DisplayList {
    let mut list = DisplayList::new();
    let mut cur = 0;
    for t in tags {
        if t.code == SHOW_FRAME {
            if cur == frame {
                return list;
            }
            cur += 1;
        } else {
            apply_tag(&mut list, t, cur);
        }
    }
    list
}

struct Text {
    matrix: Matrix,
    records: Vec<TextRecord>,
}

struct TextRecord {
    font: Option<u16>,
    color: Rgba,
    y: i32,
    height: u16,
    glyphs: Vec<(u32, i32)>,
}

fn read_text(tag: &Tag) -> Result<Text> {
    let mut r = Reader::new(&tag.body);
    r.u16()?;
    Rect::read(&mut r)?;
    let matrix = Matrix::read(&mut r)?;
    let gb = r.u8()? as u32;
    let ab = r.u8()? as u32;
    let mut records = Vec::new();
    let (mut font, mut color, mut x, mut y, mut height) = (None, Rgba::BLACK, 0, 0, 0);
    loop {
        let flags = r.u8()?;
        if flags == 0 {
            break;
        }
        if flags & 0x08 != 0 {
            font = Some(r.u16()?);
        }
        if flags & 0x04 != 0 {
            color = if tag.code == DEFINE_TEXT2 { Rgba::read_rgba(&mut r)? } else { Rgba::read_rgb(&mut r)? };
        }
        if flags & 0x01 != 0 {
            x = r.i16()? as i32;
        }
        if flags & 0x02 != 0 {
            y = r.i16()? as i32;
        }
        if flags & 0x08 != 0 {
            height = r.u16()?;
        }
        let n = r.u8()?;
        let mut glyphs = Vec::with_capacity(n as usize);
        let mut gx = x;
        for _ in 0..n {
            let g = r.ub(gb)?;
            let adv = r.sb(ab)?;
            glyphs.push((g, gx));
            gx += adv;
        }
        r.align();
        records.push(TextRecord { font, color, y, height, glyphs });
        x = gx;
    }
    Ok(Text { matrix, records })
}

struct EditText {
    bounds: Rect,
    font: Option<u16>,
    font_class: Option<String>,
    height: u16,
    color: Rgba,
    align: u8,
    left: u16,
    right: u16,
    leading: i16,
    text: String,
    border: bool,
    html: bool,
}

fn read_edit_text(tag: &Tag) -> Result<EditText> {
    let mut r = Reader::new(&tag.body);
    r.u16()?;
    let bounds = Rect::read(&mut r)?;
    let f1 = r.u8()?;
    let f2 = r.u8()?;
    let font = if f1 & 0x01 != 0 { Some(r.u16()?) } else { None };
    let font_class = if f2 & 0x80 != 0 { Some(r.string()?) } else { None };
    let height = if f1 & 0x01 != 0 || f2 & 0x80 != 0 { r.u16()? } else { 240 };
    let color = if f1 & 0x04 != 0 { Rgba::read_rgba(&mut r)? } else { Rgba::BLACK };
    if f1 & 0x02 != 0 {
        r.u16()?;
    }
    let (mut align, mut left, mut right, mut leading) = (0, 0, 0, 0);
    if f2 & 0x20 != 0 {
        align = r.u8()?;
        left = r.u16()?;
        right = r.u16()?;
        r.u16()?;
        leading = r.i16()?;
    }
    r.string()?;
    let text = if f1 & 0x80 != 0 { r.string()? } else { String::new() };
    Ok(EditText { bounds, font, font_class, height, color, align, left, right, leading, text, border: f2 & 0x08 != 0, html: f2 & 0x02 != 0 })
}

struct ButtonRecord {
    states: u8,
    id: u16,
    depth: u16,
    matrix: Matrix,
    cxform: ColorTransform,
    filters: Vec<Filter>,
    blend: u8,
}

pub const BUTTON_UP: u8 = 0x01;
pub const BUTTON_OVER: u8 = 0x02;
pub const BUTTON_DOWN: u8 = 0x04;
pub const BUTTON_HIT: u8 = 0x08;

fn read_button(tag: &Tag) -> Result<Vec<ButtonRecord>> {
    let mut r = Reader::new(&tag.body);
    r.u16()?;
    if tag.code == DEFINE_BUTTON2 {
        r.u8()?;
        r.u16()?;
    }
    let mut out = Vec::new();
    loop {
        let flags = r.u8()?;
        if flags == 0 {
            break;
        }
        let id = r.u16()?;
        let depth = r.u16()?;
        let matrix = Matrix::read(&mut r)?;
        let mut rec = ButtonRecord { states: flags & 0x0f, id, depth, matrix, cxform: ColorTransform::IDENTITY, filters: Vec::new(), blend: 0 };
        if tag.code == DEFINE_BUTTON2 {
            rec.cxform = ColorTransform::read(&mut r, true)?;
            if flags & 0x10 != 0 {
                rec.filters = read_filter_list(&mut r)?;
            }
            if flags & 0x20 != 0 {
                rec.blend = r.u8()?;
            }
        }
        out.push(rec);
    }
    Ok(out)
}

/// Renders characters and timelines of one SWF.
pub struct Renderer<'a> {
    pub swf: &'a Swf,
    chars: HashMap<u16, &'a Tag>,
    jpeg_tables: Option<&'a [u8]>,
    fonts: HashMap<u16, Font>,
    shapes: RefCell<HashMap<u16, Option<Arc<(ShapeTag, Paths)>>>>,
    morphs: RefCell<HashMap<u16, Option<Arc<MorphShape>>>>,
    bitmaps: RefCell<HashMap<u16, Option<(String, u32, u32)>>>,
    lists: RefCell<HashMap<(u16, usize), Arc<DisplayList>>>,
    bounds: RefCell<HashMap<u16, Rect>>,
}

impl Bitmaps for Renderer<'_> {
    fn bitmap(&self, id: u16) -> Option<(String, u32, u32)> {
        if let Some(b) = self.bitmaps.borrow().get(&id) {
            return b.clone();
        }
        let v = self.chars.get(&id).and_then(|t| {
            let bmp = Bitmap::read(t, self.jpeg_tables).ok()?;
            let px = bmp.pixels().ok()?;
            let (fmt, data) = bmp.encode(false).ok()?;
            Some((format!("data:{};base64,{}", fmt.mime(), base64(&data)), px.width(), px.height()))
        });
        self.bitmaps.borrow_mut().insert(id, v.clone());
        v
    }
}

fn stroke_scale(m: &Matrix) -> f64 {
    (m.a * m.d - m.b * m.c).abs().sqrt()
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

impl<'a> Renderer<'a> {
    pub fn new(swf: &'a Swf) -> Renderer<'a> {
        let mut chars = HashMap::new();
        let mut fonts = HashMap::new();
        swf.walk(|_, t| {
            if crate::tags::defines_character(t.code) {
                if let Some(id) = t.character_id() {
                    chars.entry(id).or_insert(t);
                }
            }
            if matches!(t.code, DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3) {
                if let Ok(f) = Font::read(swf, t) {
                    fonts.insert(f.id, f);
                }
            }
        });
        let jpeg_tables = swf.timeline.tags.iter().find(|t| t.code == JPEG_TABLES).map(|t| &t.body[..]);
        Renderer {
            swf,
            chars,
            jpeg_tables,
            fonts,
            shapes: RefCell::default(),
            morphs: RefCell::default(),
            bitmaps: RefCell::default(),
            lists: RefCell::default(),
            bounds: RefCell::default(),
        }
    }

    pub fn fonts(&self) -> &HashMap<u16, Font> {
        &self.fonts
    }

    /// A morph shape frozen at `ratio`, cropped to the bounds of both states.
    pub fn morph_svg_at(&self, id: u16, ratio: u16) -> Result<String> {
        let m = self.morph(id).ok_or_else(|| Error::Other(format!("morph shape {id} not found")))?;
        let b = m.start_bounds.union(&m.end_bounds);
        let mut doc = Doc::default();
        let p = paths::build(&m.shape_at(ratio), None);
        let body = svg::shape_body(&mut doc, &p, &ShapeStyle { cxform: &ColorTransform::IDENTITY, nonzero: false, bitmaps: self, stroke_scale: 1.0 }, None);
        let wrapped = format!("<g transform=\"translate({} {})\">\n{body}</g>\n", -(b.x_min as f64) / 20.0, -(b.y_min as f64) / 20.0);
        Ok(doc.finish(b.width_px(), b.height_px(), &wrapped))
    }

    pub fn character(&self, id: u16) -> Option<&'a Tag> {
        self.chars.get(&id).copied()
    }

    fn shape(&self, id: u16) -> Option<Arc<(ShapeTag, Paths)>> {
        if let Some(s) = self.shapes.borrow().get(&id) {
            return s.clone();
        }
        let v = self.chars.get(&id).and_then(|t| ShapeTag::read(t).ok()).map(|s| {
            let p = paths::build(&s.shape, None);
            Arc::new((s, p))
        });
        self.shapes.borrow_mut().insert(id, v.clone());
        v
    }

    fn morph(&self, id: u16) -> Option<Arc<MorphShape>> {
        if let Some(m) = self.morphs.borrow().get(&id) {
            return m.clone();
        }
        let v = self.chars.get(&id).and_then(|t| MorphShape::read(t).ok()).map(Arc::new);
        self.morphs.borrow_mut().insert(id, v.clone());
        v
    }

    fn sprite_tags(&self, id: u16) -> Option<&'a [Tag]> {
        let t = self.chars.get(&id)?;
        if t.code != DEFINE_SPRITE {
            return None;
        }
        t.sprite.as_ref().map(|s| &s.tags[..])
    }

    fn list(&self, id: u16, tags: &[Tag], frame: usize) -> Arc<DisplayList> {
        if let Some(l) = self.lists.borrow().get(&(id, frame)) {
            return l.clone();
        }
        let l = Arc::new(display_list(tags, frame));
        self.lists.borrow_mut().insert((id, frame), l.clone());
        l
    }

    pub fn frame_count_of(&self, id: u16) -> usize {
        match self.chars.get(&id) {
            Some(t) if t.code == DEFINE_SPRITE => self.sprite_tags(id).map(frame_count).unwrap_or(1),
            _ => 1,
        }
    }

    /// Bounds of a character in twips (sprites: over every frame).
    pub fn bounds(&self, id: u16) -> Rect {
        self.bounds_inner(id, 0)
    }

    fn bounds_inner(&self, id: u16, guard: u8) -> Rect {
        if let Some(b) = self.bounds.borrow().get(&id) {
            return *b;
        }
        if guard > MAX_DEPTH {
            return Rect::default();
        }
        let Some(tag) = self.chars.get(&id) else { return Rect::default() };
        let mut r = Reader::new(&tag.body);
        let _ = r.u16();
        let b = match tag.code {
            DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 | DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT => Rect::read(&mut r).unwrap_or_default(),
            DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => {
                let s = Rect::read(&mut r).unwrap_or_default();
                let e = Rect::read(&mut r).unwrap_or_default();
                s.union(&e)
            }
            DEFINE_SPRITE => {
                let mut acc = Rect::default();
                if let Some(tags) = self.sprite_tags(id) {
                    for_each_frame(tags, |_, list| {
                        for p in list.values() {
                            acc = acc.union(&p.matrix.transform_rect(&self.bounds_inner(p.id, guard + 1)));
                        }
                    });
                }
                acc
            }
            DEFINE_BUTTON | DEFINE_BUTTON2 => {
                let mut acc = Rect::default();
                for rec in read_button(tag).unwrap_or_default() {
                    if rec.states & (BUTTON_UP | BUTTON_OVER | BUTTON_DOWN) != 0 {
                        acc = acc.union(&rec.matrix.transform_rect(&self.bounds_inner(rec.id, guard + 1)));
                    }
                }
                acc
            }
            DEFINE_VIDEO_STREAM => {
                let _ = r.u16();
                let w = r.u16().unwrap_or(0) as i32 * 20;
                let h = r.u16().unwrap_or(0) as i32 * 20;
                Rect { x_min: 0, y_min: 0, x_max: w, y_max: h }
            }
            c if crate::export::image::is_image(c) => match self.bitmap(id) {
                Some((_, w, h)) => Rect { x_min: 0, y_min: 0, x_max: w as i32 * 20, y_max: h as i32 * 20 },
                None => Rect::default(),
            },
            _ => Rect::default(),
        };
        self.bounds.borrow_mut().insert(id, b);
        b
    }

    /// The content of character `id` in its own coordinate space (px).
    fn draw_char(&self, doc: &mut Doc, id: u16, ratio: u16, frame: usize, cx: &ColorTransform, world: &Matrix, button_state: u8, out: &mut String, guard: u8) {
        if guard > MAX_DEPTH {
            return;
        }
        let Some(tag) = self.chars.get(&id) else { return };
        match tag.code {
            DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => {
                if let Some(s) = self.shape(id) {
                    out.push_str(&svg::shape_body(doc, &s.1, &ShapeStyle { cxform: cx, nonzero: s.0.nonzero, bitmaps: self, stroke_scale: stroke_scale(world) }, None));
                }
            }
            DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => {
                if let Some(m) = self.morph(id) {
                    let p = paths::build(&m.shape_at(ratio), None);
                    out.push_str(&svg::shape_body(doc, &p, &ShapeStyle { cxform: cx, nonzero: false, bitmaps: self, stroke_scale: stroke_scale(world) }, None));
                }
            }
            DEFINE_SPRITE => {
                if let Some(tags) = self.sprite_tags(id) {
                    // `frame` counts frames since placement; children keep running across loops.
                    let n = frame_count(tags);
                    let local = frame % n;
                    let list = self.list(id, tags, local);
                    self.draw_list(doc, &list, local, frame - local, cx, world, out, guard + 1);
                }
            }
            DEFINE_BUTTON | DEFINE_BUTTON2 => {
                let mut recs = read_button(tag).unwrap_or_default();
                recs.sort_by_key(|r| r.depth);
                for rec in recs.iter().filter(|r| r.states & button_state != 0) {
                    let p = Placed {
                        depth: rec.depth,
                        id: rec.id,
                        matrix: rec.matrix,
                        cxform: rec.cxform,
                        ratio: 0,
                        clip_depth: 0,
                        name: None,
                        blend: rec.blend,
                        filters: rec.filters.clone(),
                        visible: true,
                        since: 0,
                    };
                    self.draw_placed(doc, &p, 0, frame, cx, world, out, guard + 1);
                }
            }
            DEFINE_TEXT | DEFINE_TEXT2 => {
                if let Ok(t) = read_text(tag) {
                    self.draw_text(&t, cx, None, out);
                }
            }
            DEFINE_EDIT_TEXT => {
                if let Ok(t) = read_edit_text(tag) {
                    self.draw_edit_text(&t, cx, out);
                }
            }
            DEFINE_VIDEO_STREAM => {
                let b = self.bounds(id);
                let _ = writeln!(out, "<rect width=\"{}\" height=\"{}\" fill=\"#808080\" fill-opacity=\"0.5\"/>", b.width_px(), b.height_px());
            }
            c if crate::export::image::is_image(c) => {
                if let Some((uri, w, h)) = self.bitmap(id) {
                    let mut img = format!("<image width=\"{w}\" height=\"{h}\" xlink:href=\"{uri}\"");
                    if let Some(f) = svg::color_filter(doc, cx) {
                        let _ = write!(img, " filter=\"url(#{f})\"");
                    }
                    let _ = writeln!(out, "{img}/>");
                }
            }
            _ => {}
        }
    }

    fn draw_placed(&self, doc: &mut Doc, p: &Placed, parent_frame: usize, carry: usize, cx: &ColorTransform, world: &Matrix, out: &mut String, guard: u8) {
        let cx = cx.concat(&p.cxform);
        let world = world.concat(&p.matrix);
        let mut inner = String::new();
        let child_frame = carry + parent_frame.saturating_sub(p.since);
        self.draw_char(doc, p.id, p.ratio, child_frame, &cx, &world, BUTTON_UP, &mut inner, guard);
        if inner.is_empty() {
            return;
        }
        let _ = write!(out, "<g transform=\"{}\"", matrix_attr(&p.matrix, 1.0));
        if !p.filters.is_empty() {
            if let Some(f) = filters_def(doc, &p.filters) {
                let _ = write!(out, " filter=\"url(#{f})\"");
            }
        }
        if let Some(b) = blend_style(p.blend) {
            let _ = write!(out, " style=\"mix-blend-mode:{b}\"");
        }
        out.push_str(">\n");
        out.push_str(&inner);
        out.push_str("</g>\n");
    }

    fn draw_list(&self, doc: &mut Doc, list: &DisplayList, frame: usize, carry: usize, cx: &ColorTransform, world: &Matrix, out: &mut String, guard: u8) {
        let mut clips: Vec<u16> = Vec::new();
        for p in list.values() {
            while clips.last().is_some_and(|&end| p.depth > end) {
                clips.pop();
                out.push_str("</g>\n");
            }
            if p.clip_depth > 0 {
                let mut body = String::new();
                self.clip_char(p.id, p.ratio, carry + frame.saturating_sub(p.since), &p.matrix, &mut body, guard);
                let id = doc.def("clip", &format!("<clipPath id=\"{{id}}\">{body}</clipPath>"));
                let _ = writeln!(out, "<g clip-path=\"url(#{id})\">");
                clips.push(p.clip_depth);
                continue;
            }
            if p.visible && !self.chars.get(&p.id).is_some_and(|t| crate::export::font::is_font(t.code)) {
                self.draw_placed(doc, p, frame, carry, cx, world, out, guard);
            }
        }
        for _ in clips {
            out.push_str("</g>\n");
        }
    }

    /// Fill geometry of a character for a clip mask, every path transformed by `m`.
    fn clip_char(&self, id: u16, ratio: u16, frame: usize, m: &Matrix, out: &mut String, guard: u8) {
        if guard > MAX_DEPTH {
            return;
        }
        let Some(tag) = self.chars.get(&id) else { return };
        match tag.code {
            DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => {
                if let Some(s) = self.shape(id) {
                    out.push_str(&svg::clip_body(&s.1, s.0.nonzero, m));
                }
            }
            DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => {
                if let Some(mo) = self.morph(id) {
                    out.push_str(&svg::clip_body(&paths::build(&mo.shape_at(ratio), None), false, m));
                }
            }
            DEFINE_SPRITE => {
                if let Some(tags) = self.sprite_tags(id) {
                    let n = frame_count(tags);
                    let local = frame % n;
                    let list = self.list(id, tags, local);
                    for p in list.values().filter(|p| p.clip_depth == 0) {
                        self.clip_char(p.id, p.ratio, frame - local + local.saturating_sub(p.since), &m.concat(&p.matrix), out, guard + 1);
                    }
                }
            }
            DEFINE_BUTTON | DEFINE_BUTTON2 => {
                for rec in read_button(tag).unwrap_or_default().iter().filter(|r| r.states & BUTTON_UP != 0) {
                    self.clip_char(rec.id, 0, 0, &m.concat(&rec.matrix), out, guard + 1);
                }
            }
            DEFINE_TEXT | DEFINE_TEXT2 => {
                if let Ok(t) = read_text(tag) {
                    self.draw_text(&t, &ColorTransform::IDENTITY, Some(m), out);
                }
            }
            _ => {
                let b = self.bounds(id);
                if !b.is_empty() {
                    let _ = writeln!(
                        out,
                        "<rect transform=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                        matrix_attr(m, 1.0),
                        b.x_min as f64 / 20.0,
                        b.y_min as f64 / 20.0,
                        b.width_px(),
                        b.height_px()
                    );
                }
            }
        }
    }

    /// Static text from font glyphs; `clip` emits bare transformed paths for masks.
    fn draw_text(&self, t: &Text, cx: &ColorTransform, clip: Option<&Matrix>, out: &mut String) {
        let mut body = String::new();
        for rec in &t.records {
            let Some(font) = rec.font.and_then(|f| self.fonts.get(&f)) else { continue };
            let scale = rec.height as f64 / (1024.0 * font.divider);
            let mut d = String::new();
            for &(g, x) in &rec.glyphs {
                let Some(glyph) = font.glyphs.get(g as usize) else { continue };
                for c in contours(&glyph.shape) {
                    let p = |pt: &crate::shape::Point| {
                        (svg::px(x as f64 + pt.x as f64 * scale), svg::px(rec.y as f64 + pt.y as f64 * scale))
                    };
                    let (x0, y0) = p(&c[0]);
                    let _ = write!(d, "M{x0} {y0}");
                    let mut i = 1;
                    while i < c.len() {
                        if c[i].on {
                            let (x1, y1) = p(&c[i]);
                            let _ = write!(d, "L{x1} {y1}");
                            i += 1;
                        } else {
                            let (cx1, cy1) = p(&c[i]);
                            let (x1, y1) = c.get(i + 1).map(p).unwrap_or((cx1, cy1));
                            let _ = write!(d, "Q{cx1} {cy1} {x1} {y1}");
                            i += 2;
                        }
                    }
                    d.push('Z');
                }
            }
            if d.is_empty() {
                continue;
            }
            match clip {
                Some(m) => {
                    let _ = writeln!(body, "<path transform=\"{}\" d=\"{d}\"/>", matrix_attr(&m.concat(&t.matrix), 1.0));
                }
                None => {
                    let c = cx.apply(rec.color);
                    let _ = write!(body, "<path fill=\"#{:02x}{:02x}{:02x}\"", c.r, c.g, c.b);
                    if c.a != 255 {
                        let _ = write!(body, " fill-opacity=\"{}\"", c.a as f64 / 255.0);
                    }
                    let _ = writeln!(body, " d=\"{d}\"/>");
                }
            }
        }
        if body.is_empty() {
            return;
        }
        match clip {
            Some(_) => out.push_str(&body),
            None => {
                let _ = writeln!(out, "<g transform=\"{}\">", matrix_attr(&t.matrix, 1.0));
                out.push_str(&body);
                out.push_str("</g>\n");
            }
        }
    }

    /// Dynamic text, approximated with SVG `<text>` in the field's font and alignment.
    fn draw_edit_text(&self, t: &EditText, cx: &ColorTransform, out: &mut String) {
        let b = t.bounds;
        if t.border {
            let _ = writeln!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#000000\" stroke-width=\"1\"/>",
                b.x_min as f64 / 20.0,
                b.y_min as f64 / 20.0,
                b.width_px(),
                b.height_px()
            );
        }
        let text = if t.html { crate::export::text::html_to_plain(&t.text) } else { t.text.replace('\r', "\n") };
        if text.trim().is_empty() {
            return;
        }
        let family = t
            .font
            .and_then(|f| self.fonts.get(&f))
            .map(|f| f.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| t.font_class.clone())
            .unwrap_or_else(|| "Times New Roman".to_string());
        let size = t.height as f64 / 20.0;
        let c = cx.apply(t.color);
        let (anchor, x) = match t.align {
            1 => ("end", (b.x_max - t.right as i32) as f64 / 20.0 - 2.0),
            2 => ("middle", (b.x_min + t.left as i32 + b.x_max - t.right as i32) as f64 / 40.0),
            _ => ("start", (b.x_min + t.left as i32) as f64 / 20.0 + 2.0),
        };
        let line_h = size * 1.15 + t.leading as f64 / 20.0;
        let mut y = b.y_min as f64 / 20.0 + 2.0 + size * 0.9;
        let _ = write!(
            out,
            "<text xml:space=\"preserve\" font-family=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\" fill=\"#{:02x}{:02x}{:02x}\"",
            esc(&family),
            size,
            c.r,
            c.g,
            c.b
        );
        if c.a != 255 {
            let _ = write!(out, " fill-opacity=\"{}\"", c.a as f64 / 255.0);
        }
        out.push('>');
        for line in text.split('\n') {
            let _ = write!(out, "<tspan x=\"{x}\" y=\"{y}\">{}</tspan>", esc(line));
            y += line_h;
        }
        out.push_str("</text>\n");
    }

    /// A character as a standalone SVG cropped to its bounds.
    pub fn character_svg(&self, id: u16, frame: usize, button_state: u8) -> Result<String> {
        if !self.chars.contains_key(&id) {
            return Err(Error::Other(format!("character {id} not found")));
        }
        let b = self.bounds(id);
        let mut doc = Doc::default();
        let mut body = String::new();
        self.draw_char(&mut doc, id, 0, frame, &ColorTransform::IDENTITY, &Matrix::IDENTITY, button_state, &mut body, 0);
        let wrapped = format!("<g transform=\"translate({} {})\">\n{body}</g>\n", -(b.x_min as f64) / 20.0, -(b.y_min as f64) / 20.0);
        Ok(doc.finish(b.width_px(), b.height_px(), &wrapped))
    }

    /// An animated SVG morphing between the start and end shapes, like FFDec's default.
    pub fn morph_svg(&self, id: u16) -> Result<String> {
        let m = self.morph(id).ok_or_else(|| Error::Other(format!("morph shape {id} not found")))?;
        let start = m.shape_at(0);
        let end = m.shape_at(65535);
        let paired = paths::build(&start, Some(&end));
        let end_paths = paths::build(&end, None);
        let b = m.start_bounds.union(&m.end_bounds);
        let mut doc = Doc::default();
        let body = svg::shape_body(&mut doc, &paired, &ShapeStyle { cxform: &ColorTransform::IDENTITY, nonzero: false, bitmaps: self, stroke_scale: 1.0 }, Some((&end_paths, 2.0)));
        let wrapped = format!("<g transform=\"translate({} {})\">\n{body}</g>\n", -(b.x_min as f64) / 20.0, -(b.y_min as f64) / 20.0);
        Ok(doc.finish(b.width_px(), b.height_px(), &wrapped))
    }

    /// One frame of the root timeline at stage size, with the background colour.
    pub fn frame_svg(&self, frame: usize) -> String {
        let r = self.swf.frame_rect;
        let mut doc = Doc::default();
        let mut body = String::new();
        if let Some(bg) = self.background() {
            let _ = writeln!(body, "<rect width=\"{}\" height=\"{}\" fill=\"#{:02x}{:02x}{:02x}\"/>", r.width_px(), r.height_px(), bg.r, bg.g, bg.b);
        }
        let list = display_list(&self.swf.timeline.tags, frame);
        let mut inner = String::new();
        self.draw_list(&mut doc, &list, frame, 0, &ColorTransform::IDENTITY, &Matrix::IDENTITY, &mut inner, 0);
        let _ = write!(body, "<g transform=\"translate({} {})\">\n{inner}</g>\n", -(r.x_min as f64) / 20.0, -(r.y_min as f64) / 20.0);
        doc.finish(r.width_px(), r.height_px(), &body)
    }

    pub fn background(&self) -> Option<Rgba> {
        let t = self.swf.timeline.tags.iter().find(|t| t.code == SET_BACKGROUND_COLOR)?;
        Rgba::read_rgb(&mut Reader::new(&t.body)).ok()
    }

    pub fn root_frame_count(&self) -> usize {
        frame_count(&self.swf.timeline.tags)
    }
}
