//! AS1/2 script sources of a SWF and their tree, as FFDec's `getFirstLevelASMNodes`/`getASMs`.

use std::collections::{BTreeMap, HashMap};

use crate::io::Reader;
use crate::names::{make_file_name, Names};
use crate::tags::code::*;
use crate::tags::{Tag, Timeline};
use crate::Swf;

use super::ActionList;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    DoAction,
    DoInitAction,
    /// The actions of a DefineButton.
    Button,
    ButtonCond,
    ClipAction,
}

/// One action record: a tag (by path) and the byte range of its actions in the tag body.
#[derive(Debug, Clone)]
pub struct Source {
    pub tag: Vec<usize>,
    pub kind: Kind,
    /// Position among the tag's action records (button conditions, clip events).
    pub index: usize,
    pub start: usize,
    pub end: usize,
    /// FFDec's `toString`.
    pub label: String,
    /// FFDec's `getExportFileName`.
    pub export_name: String,
    /// `on(...)`/`onClipEvent(...)` around the decompiled body.
    pub header: Option<String>,
}

impl Source {
    pub fn id(&self) -> String {
        self.index.to_string()
    }

    pub fn actions(&self, swf: &Swf) -> ActionList {
        let body = swf.tag(&self.tag).map_or(&[][..], |t| &t.body[..]);
        let end = self.end.min(body.len());
        super::read_action_list(body, self.start.min(end), end, swf.header.version)
    }

    /// Stores compiled actions and, for a button condition or clip event, the events of its
    /// `on(...)`/`onClipEvent(...)` header.
    pub fn set_compiled(&self, swf: &mut Swf, c: &super::compile::Compiled) -> crate::error::Result<()> {
        let Some(ev) = &c.events else { return self.set_actions(swf, &c.actions) };
        let version = swf.header.version;
        let tag = swf.tag(&self.tag).ok_or_else(|| crate::error::Error::Other("script tag not found".into()))?;
        let mut body = tag.body.clone();
        let bytes = c.actions.to_bytes();
        match self.kind {
            Kind::ButtonCond => {
                let at = self.start - 2;
                let flags = (ev.flags as u16) | ((ev.key as u16 & 0x7f) << 1);
                body[at..at + 2].copy_from_slice(&flags.to_be_bytes());
                let size_at = self.start - 4;
                if u16::from_le_bytes([body[size_at], body[size_at + 1]]) != 0 {
                    let size = u16::try_from(bytes.len() + 4).map_err(|_| crate::error::Error::Other("button actions are too large".into()))?;
                    body[size_at..size_at + 2].copy_from_slice(&size.to_le_bytes());
                }
                body.splice(self.start..self.end, bytes);
            }
            Kind::ClipAction => {
                let (_, _, recs, all_at) = clip_actions(tag, version).unwrap_or_default();
                let rec = recs.iter().find(|r| r.2 == self.start).ok_or_else(|| crate::error::Error::Other("clip event not found".into()))?;
                let flag_len = if version >= 6 { 4 } else { 2 };
                let old_key = (rec.0 & (1 << (31 - 22)) != 0) as usize;
                let key = ev.flags & (1 << (31 - 22)) != 0;
                let mut new = ev.flags.to_be_bytes()[..flag_len].to_vec();
                new.extend(((bytes.len() + key as usize) as u32).to_le_bytes());
                if key {
                    new.push(ev.key as u8);
                }
                new.extend(bytes);
                body.splice(self.start - old_key - 4 - flag_len..self.end, new);
                let mut all = 0u32;
                let probe = Tag { body: body.clone(), ..tag.clone() };
                for r in clip_actions(&probe, version).map(|c| c.2).unwrap_or_default() {
                    all |= r.0;
                }
                body[all_at..all_at + flag_len].copy_from_slice(&all.to_be_bytes()[..flag_len]);
            }
            _ => return self.set_actions(swf, &c.actions),
        }
        swf.set_tag_body(&self.tag, body)
    }

    /// Stores `list` in place of these actions, updating the record's size field.
    pub fn set_actions(&self, swf: &mut Swf, list: &ActionList) -> crate::error::Result<()> {
        let tag = swf.tag(&self.tag).ok_or_else(|| crate::error::Error::Other("script tag not found".into()))?;
        let bytes = list.to_bytes();
        let mut body = tag.body.clone();
        if self.end > body.len() || self.start > self.end {
            return Err(crate::error::Error::Other("script range is outside its tag".into()));
        }
        match self.kind {
            Kind::ButtonCond => {
                let at = self.start - 4;
                if u16::from_le_bytes([body[at], body[at + 1]]) != 0 {
                    let size = u16::try_from(bytes.len() + 4).map_err(|_| crate::error::Error::Other("button actions are too large".into()))?;
                    body[at..at + 2].copy_from_slice(&size.to_le_bytes());
                }
            }
            Kind::ClipAction => {
                let (_, _, recs, _) = clip_actions(tag, swf.header.version).unwrap_or_default();
                let key = recs.iter().find(|r| r.2 == self.start).is_some_and(|r| r.0 & (1 << (31 - 22)) != 0) as usize;
                let at = self.start - key - 4;
                body[at..at + 4].copy_from_slice(&((bytes.len() + key) as u32).to_le_bytes());
            }
            _ => {}
        }
        body.splice(self.start..self.end, bytes);
        swf.set_tag_body(&self.tag, body)
    }
}

/// A script tree node: a folder (package, sprite, frame, button) or a source.
#[derive(Debug, Clone)]
pub struct Node {
    pub label: String,
    /// Path segment in exported file names (`getASMPath(true, ..)`).
    pub file: String,
    pub source: Option<Source>,
    pub children: Vec<Node>,
    /// Default packages add no folder level.
    pub transparent: bool,
}

impl Node {
    fn folder(label: String, file: String, children: Vec<Node>) -> Node {
        Node { label, file, source: None, children, transparent: false }
    }

    fn leaf(s: Source) -> Node {
        Node { label: s.label.clone(), file: make_file_name(&s.export_name), source: Some(s), children: vec![], transparent: false }
    }
}

const KEYNAMES: [Option<&str>; 33] = [
    None,
    Some("<Left>"),
    Some("<Right>"),
    Some("<Home>"),
    Some("<End>"),
    Some("<Insert>"),
    Some("<Delete>"),
    None,
    Some("<Backspace>"),
    None,
    None,
    None,
    None,
    Some("<Enter>"),
    Some("<Up>"),
    Some("<Down>"),
    Some("<PageUp>"),
    Some("<PageDown>"),
    Some("<Tab>"),
    Some("<Escape>"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some("<Space>"),
];

pub fn key_to_string(key: u32) -> String {
    match KEYNAMES.get(key as usize).copied().flatten() {
        Some(k) if key > 0 => k.to_string(),
        _ => char::from_u32(key).map(String::from).unwrap_or_default(),
    }
}

fn key_part(key: u32, file: bool) -> String {
    let k = key_to_string(key);
    if file {
        format!("keyPress {}", make_file_name(&k.replace(['<', '>'], "")))
    } else {
        format!("keyPress \"{}\"", crate::abc::names::escape_as(&k))
    }
}

/// BUTTONCONDACTION `getHeader`.
fn button_header(flags: u16, key: u32, file: bool) -> String {
    let bit = |n: u32| flags & (1 << (15 - n)) != 0;
    // Bit order: IdleToOverDown, OutDownToIdle, OutDownToOverDown, OverDownToOutDown,
    // OverDownToOverUp, OverUpToOverDown, OverUpToIdle, IdleToOverUp, KeyPress(7), OverDownToIdle.
    let mut ev: Vec<String> = Vec::new();
    if bit(5) {
        ev.push("press".into());
    }
    if bit(4) {
        ev.push("release".into());
    }
    if bit(1) {
        ev.push("releaseOutside".into());
    }
    if bit(7) {
        ev.push("rollOver".into());
    }
    if bit(6) {
        ev.push("rollOut".into());
    }
    if bit(3) || bit(15) {
        ev.push("dragOut".into());
    }
    if bit(2) || bit(0) {
        ev.push("dragOver".into());
    }
    if key > 0 {
        ev.push(key_part(key, file));
    }
    format!("on({})", ev.join(", "))
}

/// CLIPEVENTFLAGS `getHeader`; `flags` holds the 32 flag bits from the high bit down.
fn clip_header(flags: u32, key: u32, file: bool) -> String {
    let bit = |n: u32| flags & (1 << (31 - n)) != 0;
    let mut ret = "";
    for (n, name) in [(0, "keyUp"), (1, "keyDown"), (2, "mouseUp"), (3, "mouseDown"), (4, "mouseMove"), (5, "unload"), (6, "enterFrame"), (7, "load"), (15, "data")] {
        if bit(n) {
            ret = name;
        }
    }
    let mut ret = if ret.is_empty() { String::new() } else { format!("onClipEvent({ret})") };
    let mut on: Vec<String> = Vec::new();
    for (n, name) in [(8, "dragOver"), (9, "rollOut"), (10, "rollOver"), (11, "releaseOutside"), (12, "release"), (13, "press"), (14, "initialize"), (21, "construct")] {
        if bit(n) {
            on.push(name.into());
        }
    }
    if bit(22) {
        on.push(key_part(key, file));
    }
    if bit(23) {
        on.push("dragOut".into());
    }
    if !on.is_empty() {
        ret = format!("on({})", on.join(","));
    }
    ret.trim().to_string()
}

const CLIP_FLAG_MASK: u32 = 0xffff_0700;

fn skip_matrix(r: &mut Reader) -> crate::error::Result<()> {
    r.align();
    for _ in 0..2 {
        if r.bit()? {
            let n = r.ub(5)?;
            r.ub(n * 2)?;
        }
    }
    let n = r.ub(5)?;
    r.ub(n * 2)?;
    r.align();
    Ok(())
}

fn skip_cxform_alpha(r: &mut Reader) -> crate::error::Result<()> {
    r.align();
    let has_add = r.bit()?;
    let has_mult = r.bit()?;
    let n = r.ub(4)?;
    if has_mult {
        r.ub(n * 4)?;
    }
    if has_add {
        r.ub(n * 4)?;
    }
    r.align();
    Ok(())
}

fn clip_flags(r: &mut Reader, version: u8) -> crate::error::Result<u32> {
    let mut f = (r.u8()? as u32) << 24;
    f |= (r.u8()? as u32) << 16;
    if version >= 6 && r.remaining() > 0 {
        f |= ((r.u8()? as u32) << 8) | r.u8()? as u32;
    }
    Ok(f)
}

/// The clip action records of a PlaceObject2/3 body as (flags, key, start, end), with the
/// placed character, depth and the offset of AllEventFlags.
fn clip_actions(tag: &Tag, version: u8) -> Option<(i32, u16, Vec<(u32, u32, usize, usize)>, usize)> {
    let mut r = Reader::new(&tag.body);
    let f1 = r.u8().ok()?;
    let f2 = if tag.code == PLACE_OBJECT3 { r.u8().ok()? } else { 0 };
    let depth = r.u16().ok()?;
    let mut character = -1;
    if tag.code == PLACE_OBJECT3 && f2 & 0x08 != 0 {
        r.string().ok()?;
    } else if f1 & 0x02 != 0 {
        character = r.u16().ok()? as i32;
    }
    if f1 & 0x80 == 0 {
        return Some((character, depth, vec![], 0));
    }
    if f1 & 0x04 != 0 {
        skip_matrix(&mut r).ok()?;
    }
    if f1 & 0x08 != 0 {
        skip_cxform_alpha(&mut r).ok()?;
    }
    if f1 & 0x10 != 0 {
        r.u16().ok()?;
    }
    if f1 & 0x20 != 0 {
        r.string().ok()?;
    }
    if f1 & 0x40 != 0 {
        r.u16().ok()?;
    }
    if tag.code == PLACE_OBJECT3 {
        if f2 & 0x01 != 0 {
            crate::filters::read_filter_list(&mut r).ok()?;
        }
        if f2 & 0x02 != 0 {
            r.u8().ok()?;
        }
        if f2 & 0x04 != 0 {
            let _ = r.u8();
        }
        if f2 & 0x20 != 0 {
            r.u8().ok()?;
        }
        if f2 & 0x40 != 0 {
            r.u32().ok()?;
        }
    }
    r.u16().ok()?;
    let all_at = r.pos();
    clip_flags(&mut r, version).ok()?;
    let mut out = Vec::new();
    loop {
        let Ok(flags) = clip_flags(&mut r, version) else { break };
        if flags & CLIP_FLAG_MASK == 0 {
            break;
        }
        let Ok(mut size) = r.u32() else { break };
        let mut key = 0;
        if flags & (1 << (31 - 22)) != 0 {
            let Ok(k) = r.u8() else { break };
            key = k as u32;
            size = size.wrapping_sub(1);
        }
        let start = r.pos();
        let end = start + size as usize;
        if end > tag.body.len() {
            break;
        }
        out.push((flags, key, start, end));
        if r.skip(size as usize).is_err() {
            break;
        }
    }
    Some((character, depth, out, all_at))
}

fn skip_button_records(r: &mut Reader, v2: bool) -> crate::error::Result<()> {
    loop {
        let flags = r.u8()?;
        if flags == 0 {
            return Ok(());
        }
        r.u16()?;
        r.u16()?;
        skip_matrix(r)?;
        if v2 {
            skip_cxform_alpha(r)?;
        }
        if flags & 0x10 != 0 {
            crate::filters::read_filter_list(r)?;
        }
        if flags & 0x20 != 0 {
            r.u8()?;
        }
    }
}

/// DefineButton2 condition records as (flags, key, start, end).
fn button_conds(tag: &Tag) -> Vec<(u16, u32, usize, usize)> {
    let mut r = Reader::new(&tag.body);
    let mut out = Vec::new();
    let ok = (|| -> crate::error::Result<bool> {
        r.u16()?;
        r.u8()?;
        let offset = r.u16()?;
        skip_button_records(&mut r, true)?;
        Ok(offset > 0)
    })();
    if !matches!(ok, Ok(true)) {
        return out;
    }
    loop {
        let (Ok(size), Ok(flags)) = (r.u16(), r.u16()) else { break };
        let flags = flags.swap_bytes();
        let key = ((flags >> 1) & 0x7f) as u32;
        let start = r.pos();
        let end = if size == 0 { tag.body.len() } else { (start + size as usize).saturating_sub(4).max(start) };
        if end > tag.body.len() {
            break;
        }
        out.push((flags, key, start, end));
        if size == 0 || r.skip(end - start).is_err() {
            break;
        }
    }
    out
}

fn button_actions_start(tag: &Tag) -> Option<usize> {
    let mut r = Reader::new(&tag.body);
    r.u16().ok()?;
    skip_button_records(&mut r, false).ok()?;
    Some(r.pos())
}

struct Ctx<'a> {
    swf: &'a Swf,
    names: Names,
    labels: HashMap<u16, String>,
}

impl Ctx<'_> {
    fn export_name(&self, id: u16) -> Option<&String> {
        self.names.export.get(&id)
    }

    /// CharacterTag `getExportFileName`.
    fn character_file(&self, t: &Tag) -> String {
        let id = t.character_id().unwrap_or(0);
        let mut s = format!("{}_{id}", t.name());
        if let Some(e) = self.export_name(id) {
            s.push('_');
            s.push_str(e);
        }
        if let Some([c]) = self.names.classes.get(&id).map(Vec::as_slice) {
            s.push('_');
            s.push_str(c);
        }
        s
    }

    fn source(&self, path: &[usize], kind: Kind, index: usize, range: (usize, usize), label: String, export_name: String, header: Option<String>) -> Source {
        Source { tag: path.to_vec(), kind, index, start: range.0, end: range.1, label, export_name, header }
    }

    /// The sources of an ASMSourceContainer tag (buttons, placements with clip actions).
    fn container_items(&self, t: &Tag, path: &[usize]) -> Vec<Node> {
        let version = self.swf.header.version;
        match t.code {
            DEFINE_BUTTON => {
                let start = button_actions_start(t).unwrap_or(t.body.len());
                let name = self.character_file(t);
                let label = crate::tree::tag_label(t, &self.labels);
                vec![Node::leaf(self.source(path, Kind::Button, 0, (start, t.body.len()), label, name, None))]
            }
            DEFINE_BUTTON2 => button_conds(t)
                .into_iter()
                .enumerate()
                .map(|(i, (flags, key, s, e))| {
                    let h = button_header(flags, key, false);
                    Node::leaf(self.source(path, Kind::ButtonCond, i, (s, e), format!("BUTTONCONDACTION {h}"), format!("BUTTONCONDACTION {}", button_header(flags, key, true)), Some(h)))
                })
                .collect(),
            PLACE_OBJECT2 | PLACE_OBJECT3 => {
                let Some((_, _, recs, _)) = clip_actions(t, version) else { return vec![] };
                recs.into_iter()
                    .enumerate()
                    .map(|(i, (flags, key, s, e))| {
                        let h = clip_header(flags, key, false);
                        Node::leaf(self.source(path, Kind::ClipAction, i, (s, e), format!("CLIPACTIONRECORD {h}"), format!("CLIPACTIONRECORD {}", clip_header(flags, key, true)), Some(h)))
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    fn place_file(&self, t: &Tag) -> String {
        let (ch, depth, _, _) = clip_actions(t, self.swf.header.version).unwrap_or((-1, 0, vec![], 0));
        let mut s = format!("{}_{ch}", t.name());
        if ch >= 0 {
            if let Some(e) = self.export_name(ch as u16) {
                s.push('_');
                s.push_str(e);
            }
        }
        format!("{s}_{depth}")
    }

    /// FrameScripts of a timeline: frames with DoAction tags or nested containers.
    fn frames(&self, tl: &Timeline, base: &[usize]) -> Vec<Node> {
        let mut out = Vec::new();
        let mut frame = 0;
        let mut labels: Vec<String> = Vec::new();
        let mut containers: Vec<Node> = Vec::new();
        let mut actions: Vec<Node> = Vec::new();
        let mut pending = false;
        let flush = |frame: usize, labels: &mut Vec<String>, containers: &mut Vec<Node>, actions: &mut Vec<Node>, out: &mut Vec<Node>| {
            if !containers.is_empty() || !actions.is_empty() {
                let mut label = format!("frame {}", frame + 1);
                if !labels.is_empty() {
                    label.push_str(&format!(" (name: {})", labels.join(", ")));
                }
                let mut children = std::mem::take(containers);
                children.append(actions);
                out.push(Node::folder(label, format!("frame_{}", frame + 1), children));
            }
            labels.clear();
            containers.clear();
            actions.clear();
        };
        for (i, t) in tl.tags.iter().enumerate() {
            pending = true;
            let mut p = base.to_vec();
            p.push(i);
            match t.code {
                FRAME_LABEL => {
                    let name = Reader::new(&t.body).string().unwrap_or_default();
                    labels.push(format!("\"{}\"", crate::abc::names::escape_pcode(&name)));
                }
                PLACE_OBJECT2 | PLACE_OBJECT3 => {
                    let items = self.container_items(t, &p);
                    if !items.is_empty() {
                        containers.push(Node::folder(crate::tree::tag_label(t, &self.labels), make_file_name(&self.place_file(t)), items));
                    }
                }
                DO_ACTION => {
                    let s = self.source(&p, Kind::DoAction, 0, (0, t.body.len()), "DoAction".into(), "DoAction".into(), None);
                    actions.push(Node::leaf(s));
                }
                SHOW_FRAME => {
                    flush(frame, &mut labels, &mut containers, &mut actions, &mut out);
                    frame += 1;
                    pending = false;
                }
                _ => {}
            }
        }
        if pending {
            flush(frame, &mut labels, &mut containers, &mut actions, &mut out);
        }
        out
    }

    fn init_action(&self, t: &Tag, path: &[usize], label: String, export_name: String) -> Source {
        self.source(path, Kind::DoInitAction, 0, (2.min(t.body.len()), t.body.len()), label, export_name, None)
    }
}

#[derive(Default)]
struct Package {
    subs: BTreeMap<String, Package>,
    scripts: BTreeMap<String, Source>,
    flat: bool,
    default: bool,
}

impl Package {
    fn into_nodes(self) -> Vec<Node> {
        let mut out = Vec::new();
        for (name, p) in self.subs {
            let file = if p.flat && !p.default { name.split('.').map(make_file_name).collect::<Vec<_>>().join("/") } else { make_file_name(&name) };
            let transparent = p.default;
            let mut n = Node::folder(name, file, p.into_nodes());
            n.transparent = transparent;
            out.push(n);
        }
        out.extend(self.scripts.into_values().map(Node::leaf));
        out
    }
}

/// The AS1/2 script tree of a SWF, in FFDec's order. Empty for SWFs without AS1/2 code.
pub fn tree(swf: &Swf) -> Vec<Node> {
    let ctx = Ctx { swf, names: Names::new(swf), labels: crate::info::character_names(swf) };
    let tl = &swf.timeline;
    let mut root = Package::default();
    let mut unnamed = Vec::new();
    for (i, t) in tl.tags.iter().enumerate() {
        if t.code != DO_INIT_ACTION {
            continue;
        }
        let id = t.character_id().unwrap_or(0);
        let Some(exp) = ctx.export_name(id) else {
            unnamed.push(Node::leaf(ctx.init_action(t, &[i], crate::tree::tag_label(t, &ctx.labels), "DoInitAction".into())));
            continue;
        };
        let path = if exp.is_empty() { "DoInitAction".to_string() } else { exp.clone() };
        let parts: Vec<&str> = path.split('.').collect();
        let last = parts[parts.len() - 1].to_string();
        let named = parts[0] == "__Packages";
        let mut pkg = &mut root;
        for pos in 0..parts.len() - 1 {
            if !(named && pos == 0) {
                let start = if named { parts[0].len() + 1 } else { 0 };
                let full = path[start..path.len() - last.len() - 1].to_string();
                pkg = pkg.subs.entry(full).or_insert_with(|| Package { flat: true, ..Default::default() });
                break;
            }
            pkg = pkg.subs.entry(parts[pos].to_string()).or_default();
        }
        if (parts.len() == 2 && named) || parts.len() == 1 {
            pkg = pkg.subs.entry("<default package>".into()).or_insert_with(|| Package { flat: true, default: true, ..Default::default() });
        }
        pkg.scripts.insert(last.clone(), ctx.init_action(t, &[i], last.clone(), last));
    }
    let mut out = root.into_nodes();
    out.extend(unnamed);
    let mut with_frames = Vec::new();
    for (i, t) in tl.tags.iter().enumerate() {
        let path = [i];
        if matches!(t.code, DO_INIT_ACTION | DO_ACTION | SHOW_FRAME | PLACE_OBJECT | PLACE_OBJECT2 | PLACE_OBJECT3 | REMOVE_OBJECT | REMOVE_OBJECT2 | FRAME_LABEL | START_SOUND | START_SOUND2) {
            continue;
        }
        let mut subs = Vec::new();
        let mut inner = false;
        if let Some(s) = &t.sprite {
            let f = ctx.frames(s, &path);
            inner = !f.is_empty();
            subs.extend(f);
        }
        subs.extend(ctx.container_items(t, &path));
        if subs.is_empty() {
            continue;
        }
        let node = Node::folder(crate::tree::tag_label(t, &ctx.labels), make_file_name(&ctx.character_file(t)), subs);
        if inner { with_frames.push(node) } else { out.push(node) }
    }
    out.extend(with_frames);
    out.extend(ctx.frames(tl, &[]));
    out
}

/// Every source with its export path (`dir/name`, no extension), deduplicated like FFDec.
pub fn export_paths(nodes: &[Node]) -> Vec<(String, Source)> {
    fn go(nodes: &[Node], dir: &str, used: &mut HashMap<String, Vec<String>>, out: &mut Vec<(String, Source)>) {
        for n in nodes {
            if let Some(s) = &n.source {
                let names = used.entry(dir.to_string()).or_default();
                let base = make_file_name(&s.export_name);
                let mut name = base.clone();
                let mut i = 1;
                while names.contains(&name) {
                    i += 1;
                    name = format!("{base}_{i}");
                }
                names.push(name.clone());
                let full = if dir.is_empty() { name } else { format!("{dir}/{name}") };
                out.push((full, s.clone()));
            }
            if !n.children.is_empty() {
                let sub = if n.transparent {
                    dir.to_string()
                } else if dir.is_empty() {
                    n.file.clone()
                } else {
                    format!("{dir}/{}", n.file)
                };
                go(&n.children, &sub, used, out);
            }
        }
    }
    let mut out = Vec::new();
    go(nodes, "", &mut HashMap::new(), &mut out);
    out
}

/// Finds a source by tag path and record index.
pub fn find(swf: &Swf, path: &[usize], index: usize) -> Option<Source> {
    fn go(nodes: &[Node], path: &[usize], index: usize) -> Option<Source> {
        for n in nodes {
            if let Some(s) = &n.source {
                if s.tag == path && s.index == index {
                    return Some(s.clone());
                }
            }
            if let Some(s) = go(&n.children, path, index) {
                return Some(s);
            }
        }
        None
    }
    go(&tree(swf), path, index)
}
