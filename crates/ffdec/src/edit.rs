//! Script editing by tree id: `as2:<index>` for AS1/2 sources, `<script>:<trait>` for AS3
//! entries, whose methods are edited one at a time. Texts are edited by tag path.

use crate::abc::scripts::{entries, methods, parse_id, MethodRef};
use crate::abc::Abc;
use crate::action::asm::ParseError;
use crate::Swf;

fn fail(message: impl Into<String>) -> ParseError {
    ParseError { line: 0, message: message.into() }
}

fn as2_source(swf: &Swf, path: &[usize], script: &str) -> Option<Result<crate::action::scripts::Source, ParseError>> {
    let index = script.strip_prefix("as2:")?;
    let index = index.parse().map_err(|_| fail("bad script id"));
    Some(index.and_then(|i| crate::action::scripts::find(swf, path, i).ok_or_else(|| fail("script not found"))))
}

fn as3_entry(swf: &Swf, path: &[usize], script: &str) -> Result<(Abc, Vec<MethodRef>), ParseError> {
    let tag = swf.tag(path).ok_or_else(|| fail("tag not found"))?;
    let (data, _) = crate::abc::tag_abc(tag).ok_or_else(|| fail("not an ABC tag"))?;
    let abc = Abc::parse(data).map_err(|e| fail(e.to_string()))?;
    let (s, t) = parse_id(script).ok_or_else(|| fail("bad script id"))?;
    let e = entries(&abc).into_iter().find(|e| e.script == s && e.trait_index == t).ok_or_else(|| fail("script not found"))?;
    let m = methods(&abc, &e);
    Ok((abc, m))
}

/// The methods an AS3 script is edited by; empty for AS1/2, which is edited whole.
pub fn script_methods(swf: &Swf, path: &[usize], script: &str) -> Result<Vec<String>, ParseError> {
    if as2_source(swf, path, script).is_some() {
        return Ok(Vec::new());
    }
    Ok(as3_entry(swf, path, script)?.1.into_iter().map(|m| m.label).collect())
}

/// The editable p-code of an AS1/2 script, or of one AS3 method.
pub fn script_pcode(swf: &Swf, path: &[usize], script: &str, method: Option<&str>) -> Result<String, ParseError> {
    if let Some(src) = as2_source(swf, path, script) {
        return Ok(crate::action::pcode::to_pcode(&src?.actions(swf)));
    }
    let (abc, all) = as3_entry(swf, path, script)?;
    let m = all.iter().find(|m| Some(m.label.as_str()) == method).ok_or_else(|| fail("method not found"))?;
    Ok(m.pcode(&abc))
}

/// Assembles `text` in place of what `script_pcode` returned.
pub fn set_script_pcode(swf: &mut Swf, path: &[usize], script: &str, method: Option<&str>, text: &str) -> Result<(), ParseError> {
    if let Some(src) = as2_source(swf, path, script) {
        let src = src?;
        let list = crate::action::asm::parse_pcode(text, swf.header.version)?;
        return src.set_actions(swf, &list).map_err(|e| fail(e.to_string()));
    }
    let (mut abc, all) = as3_entry(swf, path, script)?;
    let m = all.into_iter().find(|m| Some(m.label.as_str()) == method).ok_or_else(|| fail("method not found"))?;
    crate::abc::asm::assemble_method(&mut abc, m.method, m.owner, text)?;
    swf.set_abc(path, &abc).map_err(|e| fail(e.to_string()))
}

/// Compiles ActionScript in place of a script, as FFDec's script editor does.
pub fn set_script_source(swf: &mut Swf, path: &[usize], script: &str, text: &str) -> Result<(), ParseError> {
    set_script_source_with(swf, path, script, text, &|_| None)
}

/// `set_script_source` for an AS3 script's own pack, whose `include "File.as";` lines are
/// read with `include` first and otherwise taken from the current decompile of that pack.
pub fn set_script_source_with(swf: &mut Swf, path: &[usize], script: &str, text: &str, include: &dyn Fn(&str) -> Option<String>) -> Result<(), ParseError> {
    if let Some((s, None)) = parse_id(script) {
        if crate::abc::tag_abc(swf.tag(path).ok_or_else(|| fail("tag not found"))?).is_some() {
            let expanded = expand_includes(swf, path, s, text, include)?;
            return set_as3_source(swf, path, script, &expanded);
        }
    }
    use crate::action::compile::{compile, swf_classes, Target};
    use crate::action::scripts::Kind;
    let Some(src) = as2_source(swf, path, script) else { return set_as3_source(swf, path, script, text) };
    let src = src?;
    let target = match src.kind {
        Kind::ButtonCond => Target::ButtonCond,
        Kind::ClipAction => Target::ClipAction,
        _ => Target::Actions,
    };
    let c = compile(text, swf.header.version, target, &swf_classes(swf), Some(swf)).map_err(|e| ParseError { line: e.line, message: e.message })?;
    src.set_compiled(swf, &c).map_err(|e| fail(e.to_string()))
}

/// Replaces `include "File.as";` lines with the text of that file.
fn expand_includes(swf: &Swf, path: &[usize], script: usize, text: &str, include: &dyn Fn(&str) -> Option<String>) -> Result<String, ParseError> {
    let mut out = String::new();
    for (n, line) in text.lines().enumerate() {
        let t = line.trim();
        let Some(name) = t.strip_prefix("include").map(str::trim).and_then(|r| r.trim_end_matches(';').trim().strip_prefix('"')).and_then(|r| r.strip_suffix('"')) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let body = match include(name) {
            Some(b) => b,
            None => pack_source_named(swf, path, script, name).ok_or_else(|| ParseError { line: n + 1, message: format!("cannot include \"{name}\"") })?,
        };
        out.push_str(&body);
        out.push('\n');
    }
    Ok(out)
}

/// The decompiled text of the pack of `script` that FFDec exports as `name`.
fn pack_source_named(swf: &Swf, path: &[usize], script: usize, name: &str) -> Option<String> {
    let tag = swf.tag(path)?;
    let (data, _) = crate::abc::tag_abc(tag)?;
    let abc = Abc::parse(data).ok()?;
    let want = name.replace('\\', "/");
    let e = entries(&abc).into_iter().find(|e| {
        e.script == script && e.trait_index.is_some() && {
            let mut parts: Vec<&str> = if e.package.is_empty() { Vec::new() } else { e.package.split('.').collect() };
            let file = format!("{}.as", e.name);
            parts.push(&file);
            parts.join("/") == want
        }
    })?;
    let index = crate::abc::decomp::index_for(&[&abc]);
    Some(crate::abc::decomp::entry_source(&index, &abc, &e, &crate::abc::decomp::SwfContext::of(swf)))
}

/// Compiles an AS3 script pack, with every ABC of the SWF in scope.
fn set_as3_source(swf: &mut Swf, path: &[usize], script: &str, text: &str) -> Result<(), ParseError> {
    let (s, t) = parse_id(script).ok_or_else(|| fail("bad script id"))?;
    let mut abcs = Vec::new();
    let mut paths: Vec<Vec<usize>> = Vec::new();
    swf.walk(|p, tag| {
        if let Some((d, _)) = crate::abc::tag_abc(tag) {
            if let Ok(a) = Abc::parse(d) {
                abcs.push(a);
                paths.push(p.to_vec());
            }
        }
    });
    let sel = paths.iter().position(|p| p == path).ok_or_else(|| fail("not an ABC tag"))?;
    let doc = crate::names::Names::new(swf).classes.get(&0).and_then(|v| v.first().cloned());
    let entry = entries(&abcs[sel]).into_iter().find(|e| e.script == s && e.trait_index == t).ok_or_else(|| fail("script not found"))?;
    let index = crate::abc::decomp::index::AbcIndex::for_abcs(&abcs.iter().collect::<Vec<_>>());
    let renames = crate::abc::decomp::entry_renames(&index, &abcs[sel], &entry, &crate::abc::decomp::SwfContext::of(swf));
    let abc = crate::abc::decomp::with_big_stack(|| crate::abc::compile::replace_script_renamed(&abcs, sel, s, t, text, doc.as_deref(), &renames)).map_err(|e| ParseError { line: e.line, message: e.message })?;
    swf.set_abc(path, &abc).map_err(|e| fail(e.to_string()))
}

/// The editable text of a text tag: FFDec's formatted text for DefineText/2, the initial text
/// (HTML when the field is HTML) for DefineEditText. None for other tags.
pub fn text_source(swf: &Swf, path: &[usize]) -> Result<Option<String>, ParseError> {
    use crate::tags::code::*;
    let tag = swf.tag(path).ok_or_else(|| fail("tag not found"))?;
    match tag.code {
        DEFINE_TEXT | DEFINE_TEXT2 => {
            let t = crate::text::StaticText::read(tag).map_err(|e| fail(e.to_string()))?;
            Ok(Some(crate::text::formatted(swf, &t, false)))
        }
        DEFINE_EDIT_TEXT => Ok(Some(crate::export::text::edit_text_raw(tag).map_err(|e| fail(e.to_string()))?.0)),
        _ => Ok(None),
    }
}

/// Stores edited text in place of what `text_source` returned.
pub fn set_text_source(swf: &mut Swf, path: &[usize], text: &str) -> Result<(), ParseError> {
    use crate::tags::code::*;
    match swf.tag(path).map(|t| t.code) {
        Some(DEFINE_TEXT | DEFINE_TEXT2) => crate::text::import(swf, path, text),
        Some(DEFINE_EDIT_TEXT) => crate::importers::edit_text(swf, path, text).map_err(|e| fail(e.to_string())),
        _ => Err(fail("not a text tag")),
    }
}

/// The header fields the app edits: version, frame rate, stage size in pixels and the
/// SetBackgroundColor colour.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeaderEdit {
    pub version: u8,
    pub frame_rate: f32,
    pub width: f64,
    pub height: f64,
    /// `#rrggbb`, or None without a SetBackgroundColor tag.
    pub background: Option<String>,
}

pub fn header(swf: &Swf) -> HeaderEdit {
    let bg = swf.timeline.tags.iter().find(|t| t.code == crate::tags::code::SET_BACKGROUND_COLOR).and_then(|t| t.body.get(..3)).map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
    HeaderEdit { version: swf.header.version, frame_rate: swf.frame_rate, width: swf.frame_rect.width_px(), height: swf.frame_rect.height_px(), background: bg }
}

/// Stores edited header fields. The stage keeps its origin; a new background colour goes into
/// the existing SetBackgroundColor tag, or a new one after FileAttributes.
pub fn set_header(swf: &mut Swf, h: &HeaderEdit) -> Result<(), String> {
    use crate::tags::code::{FILE_ATTRIBUTES, METADATA, SET_BACKGROUND_COLOR};
    if h.version == 0 {
        return Err("the SWF version must be at least 1".into());
    }
    if !(0.0..256.0).contains(&h.frame_rate) {
        return Err("the frame rate must be from 0 to 255.99".into());
    }
    let twips = |px: f64| -> Result<i32, String> {
        let t = (px * 20.0).round();
        if px > 0.0 && t < (1 << 30) as f64 { Ok(t as i32) } else { Err("the stage size must be positive".into()) }
    };
    let (w, hgt) = (twips(h.width)?, twips(h.height)?);
    let color = match h.background.as_deref() {
        Some(c) => {
            let hex = c.trim().trim_start_matches('#');
            let v = (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten().ok_or_else(|| format!("\"{c}\" is not a #rrggbb colour"))?;
            Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
        }
        None => None,
    };
    swf.header.version = h.version;
    swf.frame_rate = (h.frame_rate * 256.0).round() / 256.0;
    swf.frame_rect.x_max = swf.frame_rect.x_min + w;
    swf.frame_rect.y_max = swf.frame_rect.y_min + hgt;
    if let Some(c) = color {
        match swf.timeline.tags.iter().position(|t| t.code == SET_BACKGROUND_COLOR) {
            Some(i) => swf.set_tag_body(&[i], c.to_vec()).map_err(|e| e.to_string())?,
            None => {
                let at = swf.timeline.tags.iter().take_while(|t| t.code == FILE_ATTRIBUTES || t.code == METADATA).count();
                swf.insert_tag(&[at], crate::tags::Tag::new(SET_BACKGROUND_COLOR, c.to_vec())).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// What `deobfuscate` rewrites AS3 code with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deobfuscation {
    DeadCode,
    Traps,
    /// FFDec's `AVM2DeobfuscatorGroupParts`, which its own menu does not offer.
    GroupParts,
}

/// `SWF.deobfuscate`: rewrites every AS3 method. Returns how many methods were changed; a
/// method whose deobfuscation fails is left alone.
pub fn deobfuscate(swf: &mut Swf, level: Deobfuscation) -> Result<usize, String> {
    let mut paths = Vec::new();
    swf.walk(|p, t| {
        if crate::abc::tag_abc(t).is_some() {
            paths.push(p.to_vec());
        }
    });
    let mut n = 0;
    for p in paths {
        let Some((data, _)) = swf.tag(&p).and_then(crate::abc::tag_abc) else { continue };
        let mut abc = Abc::parse(data).map_err(|e| e.to_string())?;
        match level {
            Deobfuscation::Traps => n += crate::abc::decomp::deobf::remove_traps_all(&mut abc),
            Deobfuscation::GroupParts => n += crate::abc::decomp::deobf::group_parts_all(&mut abc),
            Deobfuscation::DeadCode => {
                crate::abc::decomp::deobf::remove_dead_code_all(&mut abc);
                n += abc.bodies.len();
            }
        }
        swf.set_abc(&p, &abc).map_err(|e| e.to_string())?;
    }
    Ok(n)
}
