//! Asset export: images, sounds, binary data, fonts, texts, video streams, and rendered
//! shapes, morph shapes, frames, sprites and buttons.

pub mod flv;
pub mod font;
pub mod image;
mod nellymoser;
pub mod sound;
pub mod text;
pub mod ttf;
pub mod video;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::names::{make_file_name, Names};
use crate::render::display::{BUTTON_DOWN, BUTTON_HIT, BUTTON_OVER, BUTTON_UP};
use crate::render::{raster, Renderer};
use crate::tags::{code::*, Tag, Timeline};
use crate::Swf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Images,
    Sounds,
    BinaryData,
    Fonts,
    Texts,
    Movies,
    Shapes,
    MorphShapes,
    Frames,
    Sprites,
    Buttons,
    Scripts,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Images,
        Kind::Sounds,
        Kind::BinaryData,
        Kind::Fonts,
        Kind::Texts,
        Kind::Movies,
        Kind::Shapes,
        Kind::MorphShapes,
        Kind::Frames,
        Kind::Sprites,
        Kind::Buttons,
        Kind::Scripts,
    ];

    /// Folder name, matching FFDec's export layout.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Images => "images",
            Kind::Sounds => "sounds",
            Kind::BinaryData => "binaryData",
            Kind::Fonts => "fonts",
            Kind::Texts => "texts",
            Kind::Movies => "movies",
            Kind::Shapes => "shapes",
            Kind::MorphShapes => "morphshapes",
            Kind::Frames => "frames",
            Kind::Sprites => "sprites",
            Kind::Buttons => "buttons",
            Kind::Scripts => "scripts",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        let s = s.to_ascii_lowercase();
        Kind::ALL.into_iter().find(|k| k.dir().eq_ignore_ascii_case(&s) || k.dir().trim_end_matches('s').eq_ignore_ascii_case(&s)).or(match s.as_str() {
            "binary" => Some(Kind::BinaryData),
            "video" => Some(Kind::Movies),
            "morph" | "morphshape" => Some(Kind::MorphShapes),
            _ => None,
        })
    }

    pub fn of(code: u16) -> Option<Kind> {
        Some(match code {
            c if image::is_image(c) => Kind::Images,
            DEFINE_SOUND | SOUND_STREAM_HEAD | SOUND_STREAM_HEAD2 => Kind::Sounds,
            DEFINE_BINARY_DATA => Kind::BinaryData,
            c if font::is_font(c) => Kind::Fonts,
            c if text::is_text(c) => Kind::Texts,
            DEFINE_VIDEO_STREAM => Kind::Movies,
            c if crate::shape::is_shape(c) => Kind::Shapes,
            c if crate::morph::is_morph(c) => Kind::MorphShapes,
            DEFINE_SPRITE => Kind::Sprites,
            DEFINE_BUTTON | DEFINE_BUTTON2 => Kind::Buttons,
            DO_ABC | DO_ABC2 => Kind::Scripts,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Write every image as PNG instead of keeping JPEG/GIF data as-is.
    pub png_only: bool,
    /// Decode every sound to WAV instead of keeping MP3 and wrapping other codecs in FLV.
    pub wav_only: bool,
    /// Shapes and morph shapes as PNG (default SVG; morph shapes then as start/end images).
    pub shapes_png: bool,
    /// Frames, sprites and buttons as SVG (default PNG).
    pub frames_svg: bool,
    /// Texts as SVG drawings instead of plain text.
    pub texts_svg: bool,
    /// Raster scale; 0 means 1.
    pub zoom: f32,
    /// AS3 scripts as p-code instead of decompiled ActionScript.
    pub scripts_pcode: bool,
}

impl Options {
    fn zoom(&self) -> f32 {
        if self.zoom > 0.0 { self.zoom } else { 1.0 }
    }
}

pub struct File {
    /// File name, possibly under a sub-folder (`DefineSprite_3/1.png`).
    pub name: String,
    pub mime: &'static str,
    pub data: Vec<u8>,
}

/// A browser-friendly rendition of an asset for the preview pane.
#[derive(Serialize)]
pub struct Preview {
    /// `image`, `audio`, `font`, `text` or `vector` (SVG).
    pub kind: &'static str,
    pub mime: &'static str,
    #[serde(skip)]
    pub data: Vec<u8>,
    pub text: Option<String>,
    /// A second rendition of `text`, such as p-code next to decompiled ActionScript.
    pub alt_text: Option<String>,
    pub note: Option<String>,
    /// Frames the preview can step through (sprites, buttons, the root timeline).
    pub frames: usize,
    pub frame: usize,
    pub frame_rate: f32,
}

const NL: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Shared lookups for exporting from one SWF.
pub struct Exporter<'a> {
    swf: &'a Swf,
    names: Names,
    jpeg_tables: Option<&'a [u8]>,
    renderer: Renderer<'a>,
    as3: OnceLock<As3>,
}

/// The SWF's parsed ABCs with their type index, built on first use.
struct As3 {
    abcs: Vec<(Vec<usize>, crate::abc::Abc)>,
    index: Arc<crate::abc::decomp::index::AbcIndex>,
}

const BUTTON_FRAMES: [(u8, &str); 4] = [(BUTTON_UP, "1_up"), (BUTTON_OVER, "2_over"), (BUTTON_DOWN, "3_down"), (BUTTON_HIT, "4_hittest")];

impl<'a> Exporter<'a> {
    pub fn new(swf: &'a Swf) -> Exporter<'a> {
        let jpeg_tables = swf.timeline.tags.iter().find(|t| t.code == JPEG_TABLES).map(|t| &t.body[..]);
        Exporter { swf, names: Names::new(swf), jpeg_tables, renderer: Renderer::new(swf), as3: OnceLock::new() }
    }

    fn as3(&self) -> &As3 {
        self.as3.get_or_init(|| {
            let mut abcs = Vec::new();
            self.swf.walk(|path, t| {
                if let Some((data, _)) = crate::abc::tag_abc(t) {
                    if let Ok(abc) = crate::abc::Abc::parse(data) {
                        abcs.push((path.to_vec(), abc));
                    }
                }
            });
            let refs: Vec<&crate::abc::Abc> = abcs.iter().map(|(_, a)| a).collect();
            let index = crate::abc::decomp::index_for(&refs);
            As3 { abcs, index }
        })
    }

    fn abc_at(&self, path: &[usize]) -> Result<&crate::abc::Abc> {
        self.as3().abcs.iter().find(|(p, _)| p == path).map(|(_, a)| a).ok_or_else(|| Error::Other("not an ABC tag".into()))
    }

    /// Decompiled ActionScript of a script entry of the ABC at `path`.
    pub fn script_source(&self, path: &[usize], e: &crate::abc::scripts::Entry) -> Result<String> {
        let abc = self.abc_at(path)?;
        Ok(crate::abc::decomp::entry_source(&self.as3().index, abc, e, self.swf.header.version))
    }

    pub fn renderer(&self) -> &Renderer<'a> {
        &self.renderer
    }

    fn stem(&self, tag: &Tag, path: &[usize]) -> String {
        match tag.character_id() {
            Some(id) if crate::tags::defines_character(tag.code) => self.names.export_file_name(id),
            _ => format!("stream_{}", path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("_")),
        }
    }

    fn file_name(&self, tag: &Tag, path: &[usize], ext: &str) -> String {
        make_file_name(&format!("{}.{ext}", self.stem(tag, path)))
    }

    /// FFDec's folder name for a timeline character: `DefineSprite_12_ExportName`.
    fn folder_name(&self, tag: &Tag) -> String {
        let id = tag.character_id().unwrap_or(0);
        let mut s = format!("{}_{id}", tag.name());
        if let Some(e) = self.names.export.get(&id) {
            s.push('_');
            s.push_str(e);
        }
        if let Some([c]) = self.names.classes.get(&id).map(Vec::as_slice) {
            s.push('_');
            s.push_str(c);
        }
        make_file_name(&s)
    }

    fn timeline_of(&self, path: &[usize]) -> Option<&'a Timeline> {
        let (_, parents) = path.split_last()?;
        let mut tl = &self.swf.timeline;
        for &i in parents {
            tl = tl.tags.get(i)?.sprite.as_ref()?;
        }
        Some(tl)
    }

    fn sound(&self, tag: &'a Tag, path: &[usize]) -> Result<sound::Sound<'a>> {
        if tag.code == DEFINE_SOUND {
            sound::Sound::from_define_sound(tag)
        } else {
            let tl = self.timeline_of(path).ok_or_else(|| Error::Other("bad tag path".into()))?;
            sound::Sound::from_stream(tl, *path.last().unwrap(), self.swf.frame_rate)
        }
    }

    fn vector(&self, svg: String, png: bool, zoom: f32) -> Result<(&'static str, &'static str, Vec<u8>)> {
        if png {
            Ok(("png", "image/png", raster::png(&svg, zoom)?))
        } else {
            Ok(("svg", "image/svg+xml", svg.into_bytes()))
        }
    }

    /// Exports one tag in its FFDec format: usually one file, several for sprites,
    /// buttons and PNG morph shapes; none when the tag is not an asset.
    pub fn export(&self, path: &[usize], opts: Options) -> Result<Vec<File>> {
        let tag = self.swf.tag(path).ok_or_else(|| Error::Other("no such tag".into()))?;
        let Some(kind) = Kind::of(tag.code) else { return Ok(Vec::new()) };
        let id = tag.character_id().unwrap_or(0);
        let r = &self.renderer;
        let one = |ext: &str, mime: &'static str, data: Vec<u8>| Ok(vec![File { name: self.file_name(tag, path, ext), mime, data }]);
        match kind {
            Kind::Images => {
                let bmp = image::Bitmap::read(tag, self.jpeg_tables)?;
                let (fmt, data) = bmp.encode(opts.png_only)?;
                one(fmt.ext(), fmt.mime(), data)
            }
            Kind::Sounds => {
                let s = self.sound(tag, path)?;
                let k = if opts.wav_only { sound::Kind::Wav } else { s.native_kind() };
                one(k.ext(), k.mime(), s.export(k)?)
            }
            Kind::BinaryData => {
                let data = tag.body.get(6..).unwrap_or(&[]).to_vec();
                if Swf::parse(&data).is_ok() {
                    one("swf", "application/x-shockwave-flash", data)
                } else {
                    one("bin", "application/octet-stream", data)
                }
            }
            Kind::Fonts => {
                let (ext, mime, data, font_name) = if tag.code == DEFINE_FONT4 {
                    ("otf", "font/otf", font::font4_data(tag)?.to_vec(), font::font4_name(tag))
                } else {
                    let f = r.fonts().get(&id).ok_or_else(|| Error::Other("font could not be read".into()))?;
                    // FFDec writes nothing for fonts without glyphs (device fonts).
                    if f.glyphs.iter().all(|g| g.shape.is_empty()) {
                        return Ok(Vec::new());
                    }
                    ("ttf", "font/ttf", f.to_ttf(), f.name.clone())
                };
                Ok(vec![File { name: make_file_name(&format!("{}_{font_name}.{ext}", self.stem(tag, path))), mime, data }])
            }
            Kind::Texts if opts.texts_svg => one("svg", "image/svg+xml", r.character_svg(id, 0, BUTTON_UP)?.into_bytes()),
            Kind::Texts => {
                let sep = format!("{NL}--- RECORDSEPARATOR ---{NL}");
                one("txt", "text/plain", text::records(tag, r.fonts())?.join(&sep).into_bytes())
            }
            Kind::Movies => one("flv", "video/x-flv", video::export(self.swf, tag)?),
            Kind::Shapes => {
                let (ext, mime, data) = self.vector(r.character_svg(id, 0, BUTTON_UP)?, opts.shapes_png, opts.zoom())?;
                one(ext, mime, data)
            }
            Kind::MorphShapes if opts.shapes_png => {
                let stem = self.stem(tag, path);
                let mut out = Vec::new();
                for (suffix, ratio) in [("start", 0u16), ("end", 65535)] {
                    let data = raster::png(&r.morph_svg_at(id, ratio)?, opts.zoom())?;
                    out.push(File { name: make_file_name(&format!("{stem}.{suffix}.png")), mime: "image/png", data });
                }
                Ok(out)
            }
            Kind::MorphShapes => one("svg", "image/svg+xml", r.morph_svg(id)?.into_bytes()),
            Kind::Sprites => {
                let dir = self.folder_name(tag);
                let mut out = Vec::new();
                for f in 0..r.frame_count_of(id) {
                    let (ext, mime, data) = self.vector(r.character_svg(id, f, BUTTON_UP)?, !opts.frames_svg, opts.zoom())?;
                    out.push(File { name: format!("{dir}/{}.{ext}", f + 1), mime, data });
                }
                Ok(out)
            }
            Kind::Buttons => {
                let dir = self.folder_name(tag);
                let mut out = Vec::new();
                for (state, name) in BUTTON_FRAMES {
                    let (ext, mime, data) = self.vector(r.character_svg(id, 0, state)?, !opts.frames_svg, opts.zoom())?;
                    out.push(File { name: format!("{dir}/{name}.{ext}"), mime, data });
                }
                Ok(out)
            }
            Kind::Scripts => {
                if crate::abc::tag_abc(tag).is_none() {
                    return Ok(Vec::new());
                }
                let abc = self.abc_at(path)?;
                let ext = if opts.scripts_pcode { "pcode" } else { "as" };
                crate::abc::scripts::entries(abc)
                    .iter()
                    .map(|e| {
                        let mut name: Vec<String> = if e.package.is_empty() { Vec::new() } else { e.package.split('.').map(make_file_name).collect() };
                        name.push(make_file_name(&format!("{}.{ext}", e.name)));
                        let text = if opts.scripts_pcode { crate::abc::scripts::entry_pcode(abc, e) } else { self.script_source(path, e)? };
                        Ok(File { name: name.join("/"), mime: "text/plain", data: text.into_bytes() })
                    })
                    .collect()
            }
            Kind::Frames => Ok(Vec::new()),
        }
    }

    /// Root timeline frames as `frames/<n>.png` (or SVG).
    pub fn export_frames(&self, opts: Options) -> Vec<Result<File>> {
        (0..self.renderer.root_frame_count())
            .map(|f| {
                let (ext, mime, data) = self.vector(self.renderer.frame_svg(f), !opts.frames_svg, opts.zoom())?;
                Ok(File { name: format!("{}.{ext}", f + 1), mime, data })
            })
            .collect()
    }

    /// A preview of a tag, or of the root timeline when `path` is empty.
    pub fn preview(&self, path: &[usize], frame: usize) -> Result<Option<Preview>> {
        let r = &self.renderer;
        let p = |kind, mime, data, text: Option<String>| Preview { kind, mime, data, text, alt_text: None, note: None, frames: 1, frame: 0, frame_rate: self.swf.frame_rate };
        if path.is_empty() {
            let n = r.root_frame_count();
            let f = frame.min(n - 1);
            let mut pv = p("vector", "image/svg+xml", r.frame_svg(f).into_bytes(), None);
            pv.frames = n;
            pv.frame = f;
            return Ok(Some(pv));
        }
        let tag = self.swf.tag(path).ok_or_else(|| Error::Other("no such tag".into()))?;
        let id = tag.character_id().unwrap_or(0);
        Ok(Some(match Kind::of(tag.code) {
            Some(Kind::Images) => {
                let bmp = image::Bitmap::read(tag, self.jpeg_tables)?;
                let native = bmp.native_format();
                let (fmt, data) = bmp.encode(native == image::Format::Jxr)?;
                let mut pv = p("image", fmt.mime(), data, None);
                if let Ok(px) = bmp.pixels() {
                    pv.note = Some(format!("{} × {} px, exports as {}", px.width(), px.height(), native.ext().to_uppercase()));
                }
                pv
            }
            Some(Kind::Sounds) => {
                let s = self.sound(tag, path)?;
                let k = s.playable_kind();
                let mut pv = p("audio", k.mime(), s.export(k)?, None);
                pv.note = Some(format!("{}, {} Hz, exports as {}", crate::info::sound_format(s.format as u32), s.rate(), s.native_kind().ext().to_uppercase()));
                pv
            }
            Some(Kind::Fonts) if tag.code != DEFINE_FONT4 => {
                let Some(f) = r.fonts().get(&id) else { return Ok(None) };
                let chars: String = f.glyphs.iter().filter_map(|g| char::from_u32(g.code as u32)).filter(|c| !c.is_control()).collect();
                let mut pv = p("font", "font/ttf", f.to_ttf(), Some(chars));
                pv.note = Some(format!("{} — {} glyphs", if f.name.is_empty() { "unnamed" } else { &f.name }, f.glyphs.len()));
                pv
            }
            Some(Kind::Fonts) => p("font", "font/otf", font::font4_data(tag)?.to_vec(), None),
            Some(Kind::Texts) => {
                let mut pv = p("vector", "image/svg+xml", r.character_svg(id, 0, BUTTON_UP)?.into_bytes(), None);
                pv.text = Some(text::records(tag, r.fonts())?.join("\n"));
                pv
            }
            Some(Kind::Shapes) => p("vector", "image/svg+xml", r.character_svg(id, 0, BUTTON_UP)?.into_bytes(), None),
            Some(Kind::MorphShapes) => {
                let mut pv = p("vector", "image/svg+xml", r.morph_svg(id)?.into_bytes(), None);
                pv.note = Some("Morphs from the start to the end shape every 2 seconds".into());
                pv
            }
            Some(Kind::Sprites) => {
                let n = r.frame_count_of(id);
                let f = frame.min(n - 1);
                let mut pv = p("vector", "image/svg+xml", r.character_svg(id, f, BUTTON_UP)?.into_bytes(), None);
                pv.frames = n;
                pv.frame = f;
                pv
            }
            Some(Kind::Buttons) => {
                let f = frame.min(3);
                let mut pv = p("vector", "image/svg+xml", r.character_svg(id, 0, BUTTON_FRAMES[f].0)?.into_bytes(), None);
                pv.frames = 4;
                pv.frame = f;
                pv.note = Some(["up", "over", "down", "hit test"][f].to_string() + " state");
                pv
            }
            _ => return Ok(None),
        }))
    }

    /// One AS3 script entry as an `.as` file, or `.pcode` with `pcode`.
    pub fn export_script(&self, path: &[usize], script: &str, pcode: bool) -> Result<File> {
        let pv = self.script_preview(path, script)?;
        let name = pv.note.as_deref().and_then(|n| n.split(' ').nth(1)).unwrap_or("script").rsplit('.').next().unwrap_or("script").to_string();
        let (ext, text) = if pcode { ("pcode", pv.alt_text) } else { ("as", pv.text) };
        Ok(File { name: make_file_name(&format!("{name}.{ext}")), mime: "text/plain", data: text.unwrap_or_default().into_bytes() })
    }

    /// Decompiled ActionScript, with p-code as `alt_text`, of one AS3 script entry (`script` as
    /// `script:trait`) in the DoABC tag at `path`.
    pub fn script_preview(&self, path: &[usize], script: &str) -> Result<Preview> {
        let abc = self.abc_at(path)?;
        let (s, t) = crate::abc::scripts::parse_id(script).ok_or_else(|| Error::Other("bad script id".into()))?;
        let e = crate::abc::scripts::entries(&abc)
            .into_iter()
            .find(|e| e.script == s && e.trait_index == t)
            .ok_or_else(|| Error::Other("script not found".into()))?;
        Ok(Preview {
            kind: "code",
            mime: "text/plain",
            data: Vec::new(),
            text: Some(self.script_source(path, &e)?),
            alt_text: Some(crate::abc::scripts::entry_pcode(abc, &e)),
            note: Some(format!("{} {}, ABC {}.{}", e.kind, e.full_name(), abc.major, abc.minor)),
            frames: 1,
            frame: 0,
            frame_rate: self.swf.frame_rate,
        })
    }

    /// Every asset of the given kinds, as (kind, tag path, result).
    pub fn export_all(&self, kinds: &[Kind], opts: Options) -> Vec<(Kind, Vec<usize>, Result<File>)> {
        let mut out = Vec::new();
        self.swf.walk(|path, t| {
            if let Some(k) = Kind::of(t.code) {
                let empty_stream = k == Kind::Sounds && t.code != DEFINE_SOUND && self.sound(t, path).map_or(true, |s| s.chunks.is_empty());
                if kinds.contains(&k) && !empty_stream {
                    match self.export(path, opts) {
                        Ok(files) => out.extend(files.into_iter().map(|f| (k, path.to_vec(), Ok(f)))),
                        Err(e) => out.push((k, path.to_vec(), Err(e))),
                    }
                }
            }
        });
        if kinds.contains(&Kind::Frames) {
            out.extend(self.export_frames(opts).into_iter().map(|r| (Kind::Frames, Vec::new(), r)));
        }
        out
    }

    /// Writes `export_all` into `dir/<kind>/…`. Returns written paths and per-asset errors.
    pub fn write_all(&self, dir: &Path, kinds: &[Kind], opts: Options) -> (Vec<PathBuf>, Vec<String>) {
        let (mut written, mut errors) = (Vec::new(), Vec::new());
        for (kind, path, r) in self.export_all(kinds, opts) {
            let p = path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("/");
            match r {
                Ok(f) => {
                    let target = dir.join(kind.dir()).join(&f.name);
                    let res = target.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&target, &f.data));
                    match res {
                        Ok(()) => written.push(target),
                        Err(e) => errors.push(format!("{}: {e}", target.display())),
                    }
                }
                Err(e) => errors.push(format!("tag {p}: {e}")),
            }
        }
        (written, errors)
    }
}
