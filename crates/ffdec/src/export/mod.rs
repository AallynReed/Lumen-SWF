//! Asset export: images, sounds, binary data, fonts, texts and video streams.

pub mod flv;
pub mod font;
pub mod image;
mod nellymoser;
pub mod sound;
pub mod text;
pub mod ttf;
pub mod video;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::names::{make_file_name, Names};
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
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Images, Kind::Sounds, Kind::BinaryData, Kind::Fonts, Kind::Texts, Kind::Movies];

    /// Folder name, matching FFDec's export layout.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Images => "images",
            Kind::Sounds => "sounds",
            Kind::BinaryData => "binaryData",
            Kind::Fonts => "fonts",
            Kind::Texts => "texts",
            Kind::Movies => "movies",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.dir().eq_ignore_ascii_case(s) || k.dir().trim_end_matches('s').eq_ignore_ascii_case(s))
            .or(match s.to_ascii_lowercase().as_str() {
                "image" => Some(Kind::Images),
                "sound" => Some(Kind::Sounds),
                "binary" | "binarydata" => Some(Kind::BinaryData),
                "font" => Some(Kind::Fonts),
                "text" => Some(Kind::Texts),
                "movie" | "video" => Some(Kind::Movies),
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
}

pub struct File {
    pub name: String,
    pub mime: &'static str,
    pub data: Vec<u8>,
}

/// A browser-friendly rendition of an asset for the preview pane.
#[derive(Serialize)]
pub struct Preview {
    /// `image`, `audio`, `font` or `text`.
    pub kind: &'static str,
    pub mime: &'static str,
    #[serde(skip)]
    pub data: Vec<u8>,
    pub text: Option<String>,
    pub note: Option<String>,
}

const NL: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Shared lookups for exporting from one SWF.
pub struct Exporter<'a> {
    swf: &'a Swf,
    names: Names,
    jpeg_tables: Option<&'a [u8]>,
    fonts: HashMap<u16, font::Font>,
}

impl<'a> Exporter<'a> {
    pub fn new(swf: &'a Swf) -> Exporter<'a> {
        let jpeg_tables = swf.timeline.tags.iter().find(|t| t.code == JPEG_TABLES).map(|t| &t.body[..]);
        let mut fonts = HashMap::new();
        swf.walk(|_, t| {
            if matches!(t.code, DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3) {
                if let Ok(f) = font::Font::read(swf, t) {
                    fonts.insert(f.id, f);
                }
            }
        });
        Exporter { swf, names: Names::new(swf), jpeg_tables, fonts }
    }

    fn file_name(&self, tag: &Tag, path: &[usize], ext: &str) -> String {
        let stem = match tag.character_id() {
            Some(id) if crate::tags::defines_character(tag.code) => self.names.export_file_name(id),
            _ => format!("stream_{}", path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("_")),
        };
        make_file_name(&format!("{stem}.{ext}"))
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

    /// Exports one tag in its FFDec format. `Ok(None)` when the tag is not an asset.
    pub fn export(&self, path: &[usize], opts: Options) -> Result<Option<File>> {
        let tag = self.swf.tag(path).ok_or_else(|| Error::Other("no such tag".into()))?;
        let Some(kind) = Kind::of(tag.code) else { return Ok(None) };
        let (ext, mime, data): (&str, &'static str, Vec<u8>) = match kind {
            Kind::Images => {
                let bmp = image::Bitmap::read(tag, self.jpeg_tables)?;
                let (fmt, data) = bmp.encode(opts.png_only)?;
                (fmt.ext(), fmt.mime(), data)
            }
            Kind::Sounds => {
                let s = self.sound(tag, path)?;
                let k = if opts.wav_only { sound::Kind::Wav } else { s.native_kind() };
                (k.ext(), k.mime(), s.export(k)?)
            }
            Kind::BinaryData => {
                let data = tag.body.get(6..).unwrap_or(&[]).to_vec();
                let inner = Swf::parse(&data).is_ok();
                (if inner { "swf" } else { "bin" }, if inner { "application/x-shockwave-flash" } else { "application/octet-stream" }, data)
            }
            Kind::Fonts => {
                if tag.code == DEFINE_FONT4 {
                    ("otf", "font/otf", font::font4_data(tag)?.to_vec())
                } else {
                    let id = tag.character_id().unwrap_or(0);
                    let f = self.fonts.get(&id).ok_or_else(|| Error::Other("font could not be read".into()))?;
                    // FFDec writes nothing for fonts without glyphs (device fonts).
                    if f.glyphs.iter().all(|g| g.shape.is_empty()) {
                        return Ok(None);
                    }
                    ("ttf", "font/ttf", f.to_ttf())
                }
            }
            Kind::Texts => {
                let sep = format!("{NL}--- RECORDSEPARATOR ---{NL}");
                ("txt", "text/plain", text::records(tag, &self.fonts)?.join(&sep).into_bytes())
            }
            Kind::Movies => ("flv", "video/x-flv", video::export(self.swf, tag)?),
        };
        let mut name = self.file_name(tag, path, ext);
        if kind == Kind::Fonts {
            let font_name = match tag.code {
                DEFINE_FONT4 => font::font4_name(tag),
                _ => self.fonts.get(&tag.character_id().unwrap_or(0)).map(|f| f.name.clone()).unwrap_or_default(),
            };
            let stem = name.strip_suffix(&format!(".{ext}")).unwrap_or(&name).to_string();
            name = make_file_name(&format!("{stem}_{font_name}.{ext}"));
        }
        Ok(Some(File { name, mime, data }))
    }

    pub fn preview(&self, path: &[usize]) -> Result<Option<Preview>> {
        let tag = self.swf.tag(path).ok_or_else(|| Error::Other("no such tag".into()))?;
        let p = |kind, mime, data, text: Option<String>| Preview { kind, mime, data, text, note: None };
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
                let id = tag.character_id().unwrap_or(0);
                let Some(f) = self.fonts.get(&id) else { return Ok(None) };
                let chars: String = f.glyphs.iter().filter_map(|g| char::from_u32(g.code as u32)).filter(|c| !c.is_control()).collect();
                let mut pv = p("font", "font/ttf", f.to_ttf(), Some(chars));
                pv.note = Some(format!("{} — {} glyphs", if f.name.is_empty() { "unnamed" } else { &f.name }, f.glyphs.len()));
                pv
            }
            Some(Kind::Fonts) => p("font", "font/otf", font::font4_data(tag)?.to_vec(), None),
            Some(Kind::Texts) => p("text", "text/plain", Vec::new(), Some(text::records(tag, &self.fonts)?.join("\n"))),
            _ => return Ok(None),
        }))
    }

    /// Every asset of the given kinds, as (kind, tag path, result).
    pub fn export_all(&self, kinds: &[Kind], opts: Options) -> Vec<(Kind, Vec<usize>, Result<File>)> {
        let mut out = Vec::new();
        self.swf.walk(|path, t| {
            if let Some(k) = Kind::of(t.code) {
                let empty_stream = k == Kind::Sounds && t.code != DEFINE_SOUND && self.sound(t, path).map_or(true, |s| s.chunks.is_empty());
                if kinds.contains(&k) && !empty_stream {
                    match self.export(path, opts) {
                        Ok(Some(f)) => out.push((k, path.to_vec(), Ok(f))),
                        Ok(None) => {}
                        Err(e) => out.push((k, path.to_vec(), Err(e))),
                    }
                }
            }
        });
        out
    }

    /// Writes `export_all` into `dir/<kind>/…`. Returns written paths and per-asset errors.
    pub fn write_all(&self, dir: &Path, kinds: &[Kind], opts: Options) -> (Vec<PathBuf>, Vec<String>) {
        let (mut written, mut errors) = (Vec::new(), Vec::new());
        for (kind, path, r) in self.export_all(kinds, opts) {
            let p = path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("/");
            match r {
                Ok(f) => {
                    let sub = dir.join(kind.dir());
                    let target = sub.join(&f.name);
                    match std::fs::create_dir_all(&sub).and_then(|_| std::fs::write(&target, &f.data)) {
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
