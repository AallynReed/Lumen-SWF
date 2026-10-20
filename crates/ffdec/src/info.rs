//! Human-readable fields for each tag, for the details panel and `ffdec-cli info`.

use serde::Serialize;

use crate::error::Result;
use crate::io::Reader;
use crate::tags::{code::*, Tag};
use crate::types::{Rect, Rgba};

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub name: String,
    pub value: String,
}

struct Out(Vec<Field>);

impl Out {
    fn f(&mut self, name: &str, value: impl ToString) {
        self.0.push(Field { name: name.into(), value: value.to_string() });
    }

    fn rect(&mut self, name: &str, r: Rect) {
        self.f(
            name,
            format!("{} × {} px at ({}, {})", r.width_px(), r.height_px(), r.x_min as f64 / 20.0, r.y_min as f64 / 20.0),
        );
    }

    fn flags(&mut self, name: &str, value: u32, names: &[(u32, &str)]) {
        let set: Vec<&str> = names.iter().filter(|(m, _)| value & m != 0).map(|(_, n)| *n).collect();
        self.f(name, if set.is_empty() { "none".to_string() } else { set.join(", ") });
    }
}

pub fn describe(tag: &Tag) -> Vec<Field> {
    let mut out = Out(Vec::new());
    out.f("Tag", format!("{} ({})", tag.name(), tag.code));
    out.f("Offset", format!("0x{:x}", tag.offset));
    out.f("Length", format!("{} bytes{}", tag.body.len(), if tag.long_header { ", long header" } else { "" }));
    if let Some(e) = &tag.error {
        out.f("Error", e);
    }
    let mut r = Reader::new(&tag.body);
    if let Err(e) = fields(tag, &mut r, &mut out) {
        out.f("Parse error", e);
    }
    out.0
}

fn image_kind(data: &[u8]) -> &'static str {
    let data = data.strip_prefix(&[0xff, 0xd9, 0xff, 0xd8]).unwrap_or(data);
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        "PNG"
    } else if data.starts_with(b"GIF8") {
        "GIF"
    } else if data.starts_with(&[0xff, 0xd8]) {
        "JPEG"
    } else {
        "unknown"
    }
}

pub fn sound_format(f: u32) -> &'static str {
    match f {
        0 => "Uncompressed (native endian)",
        1 => "ADPCM",
        2 => "MP3",
        3 => "Uncompressed (little endian)",
        4 => "Nellymoser 16 kHz",
        5 => "Nellymoser 8 kHz",
        6 => "Nellymoser",
        11 => "Speex",
        _ => "unknown",
    }
}

const SOUND_RATES: [u32; 4] = [5512, 11025, 22050, 44100];

fn skip_matrix(r: &mut Reader) -> Result<()> {
    r.align();
    if r.bit()? {
        let n = r.ub(5)?;
        r.ub(n * 2)?;
    }
    if r.bit()? {
        let n = r.ub(5)?;
        r.ub(n * 2)?;
    }
    let n = r.ub(5)?;
    r.ub(n * 2)?;
    r.align();
    Ok(())
}

fn skip_cxform(r: &mut Reader, alpha: bool) -> Result<()> {
    r.align();
    let has_add = r.bit()?;
    let has_mult = r.bit()?;
    let n = r.ub(4)?;
    let k = if alpha { 4 } else { 3 };
    if has_mult {
        r.ub(n * k)?;
    }
    if has_add {
        r.ub(n * k)?;
    }
    r.align();
    Ok(())
}

fn id_name_list(r: &mut Reader, out: &mut Out, label: &str) -> Result<()> {
    let n = r.u16()?;
    out.f("Count", n);
    for _ in 0..n {
        let id = r.u16()?;
        let name = r.string()?;
        out.f(label, format!("{id}: {name}"));
    }
    Ok(())
}

fn fields(tag: &Tag, r: &mut Reader, out: &mut Out) -> Result<()> {
    match tag.code {
        SET_BACKGROUND_COLOR => out.f("Color", Rgba::read_rgb(r)?.hex()),
        FILE_ATTRIBUTES => {
            let v = r.u32()?;
            out.flags(
                "Flags",
                v,
                &[
                    (0x40, "UseDirectBlit"),
                    (0x20, "UseGPU"),
                    (0x10, "HasMetadata"),
                    (0x08, "ActionScript3"),
                    (0x04, "NoCrossDomainCache"),
                    (0x02, "SwfRelativeUrls"),
                    (0x01, "UseNetwork"),
                ],
            );
        }
        FRAME_LABEL => {
            out.f("Name", r.string()?);
            if !r.is_empty() {
                out.f("Named anchor", r.u8()? != 0);
            }
        }
        EXPORT_ASSETS => id_name_list(r, out, "Export")?,
        SYMBOL_CLASS => id_name_list(r, out, "Symbol")?,
        IMPORT_ASSETS | IMPORT_ASSETS2 => {
            out.f("URL", r.string()?);
            if tag.code == IMPORT_ASSETS2 {
                r.skip(2)?;
            }
            id_name_list(r, out, "Import")?;
        }
        METADATA => out.f("XML", r.string()?),
        SCRIPT_LIMITS => {
            out.f("Max recursion depth", r.u16()?);
            out.f("Script timeout (s)", r.u16()?);
        }
        PRODUCT_INFO => {
            out.f("Product ID", r.u32()?);
            out.f("Edition", r.u32()?);
            out.f("Version", format!("{}.{}", r.u8()?, r.u8()?));
            let build = r.u32()? as u64 | ((r.u32()? as u64) << 32);
            out.f("Build", build);
            let date = r.u32()? as u64 | ((r.u32()? as u64) << 32);
            out.f("Compile date (ms since epoch)", date);
        }
        PROTECT | ENABLE_DEBUGGER => {
            if !r.is_empty() {
                out.f("Password hash", r.string()?);
            }
        }
        ENABLE_DEBUGGER2 => {
            r.skip(2)?;
            out.f("Password hash", r.string()?);
        }
        DEBUG_ID => out.f("UUID", r.rest().iter().map(|b| format!("{b:02x}")).collect::<String>()),
        DEFINE_BINARY_DATA => {
            out.f("Character ID", r.u16()?);
            r.skip(4)?;
            out.f("Data", format!("{} bytes", r.remaining()));
        }
        DO_ABC => out.f("ABC", format!("{} bytes", r.remaining())),
        DO_ABC2 => {
            out.flags("Flags", r.u32()?, &[(1, "LazyInitialize")]);
            out.f("Name", r.string()?);
            out.f("ABC", format!("{} bytes", r.remaining()));
        }
        DO_ACTION => out.f("Actions", format!("{} bytes", r.remaining())),
        DO_INIT_ACTION => {
            out.f("Sprite ID", r.u16()?);
            out.f("Actions", format!("{} bytes", r.remaining()));
        }
        DEFINE_SPRITE => {
            out.f("Character ID", r.u16()?);
            out.f("Frame count", r.u16()?);
            if let Some(s) = &tag.sprite {
                out.f("Child tags", s.tags.len());
            }
        }
        DEFINE_BITS => {
            out.f("Character ID", r.u16()?);
            out.f("Format", "JPEG (uses JPEGTables)");
        }
        DEFINE_BITS_JPEG2 => {
            out.f("Character ID", r.u16()?);
            out.f("Format", image_kind(r.rest()));
        }
        DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 => {
            out.f("Character ID", r.u16()?);
            let alpha_offset = r.u32()? as usize;
            if tag.code == DEFINE_BITS_JPEG4 {
                out.f("Deblocking", r.fixed8()?);
            }
            let rest = r.rest();
            out.f("Format", image_kind(rest));
            out.f("Image data", format!("{alpha_offset} bytes"));
            out.f("Alpha data (zlib)", format!("{} bytes", rest.len().saturating_sub(alpha_offset)));
        }
        DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => {
            out.f("Character ID", r.u16()?);
            let fmt = r.u8()?;
            out.f(
                "Format",
                match fmt {
                    3 => "8-bit colormapped",
                    4 => "15-bit RGB",
                    5 if tag.code == DEFINE_BITS_LOSSLESS => "24-bit RGB",
                    5 => "32-bit ARGB",
                    _ => "unknown",
                },
            );
            out.f("Size", format!("{} × {}", r.u16()?, r.u16()?));
            if fmt == 3 {
                out.f("Colors", r.u8()? as u32 + 1);
            }
        }
        JPEG_TABLES => out.f("Tables", format!("{} bytes", r.remaining())),
        DEFINE_SOUND => {
            out.f("Character ID", r.u16()?);
            let fmt = r.ub(4)?;
            out.f("Format", sound_format(fmt));
            out.f("Rate", format!("{} Hz", SOUND_RATES[r.ub(2)? as usize]));
            out.f("Sample size", if r.bit()? { "16-bit" } else { "8-bit" });
            out.f("Channels", if r.bit()? { "stereo" } else { "mono" });
            out.f("Sample count", r.u32()?);
        }
        SOUND_STREAM_HEAD | SOUND_STREAM_HEAD2 => {
            r.ub(4)?;
            out.f("Playback rate", format!("{} Hz", SOUND_RATES[r.ub(2)? as usize]));
            out.f("Playback size", if r.bit()? { "16-bit" } else { "8-bit" });
            out.f("Playback channels", if r.bit()? { "stereo" } else { "mono" });
            let fmt = r.ub(4)?;
            out.f("Stream format", sound_format(fmt));
            out.f("Stream rate", format!("{} Hz", SOUND_RATES[r.ub(2)? as usize]));
            out.f("Stream size", if r.bit()? { "16-bit" } else { "8-bit" });
            out.f("Stream channels", if r.bit()? { "stereo" } else { "mono" });
            out.f("Samples per block", r.u16()?);
            if fmt == 2 && r.remaining() >= 2 {
                out.f("Latency seek", r.i16()?);
            }
        }
        SOUND_STREAM_BLOCK => out.f("Data", format!("{} bytes", r.remaining())),
        START_SOUND => out.f("Sound ID", r.u16()?),
        START_SOUND2 => out.f("Sound class", r.string()?),
        DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => {
            out.f("Character ID", r.u16()?);
            out.rect("Bounds", Rect::read(r)?);
            if tag.code == DEFINE_SHAPE4 {
                out.rect("Edge bounds", Rect::read(r)?);
                out.flags("Flags", r.u8()? as u32, &[(4, "UsesFillWindingRule"), (2, "UsesNonScalingStrokes"), (1, "UsesScalingStrokes")]);
            }
        }
        DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => {
            out.f("Character ID", r.u16()?);
            out.rect("Start bounds", Rect::read(r)?);
            out.rect("End bounds", Rect::read(r)?);
        }
        DEFINE_TEXT | DEFINE_TEXT2 => {
            out.f("Character ID", r.u16()?);
            out.rect("Bounds", Rect::read(r)?);
        }
        DEFINE_EDIT_TEXT => edit_text(r, out)?,
        DEFINE_FONT => {
            out.f("Character ID", r.u16()?);
            let first = r.u16()?;
            out.f("Glyphs", first / 2);
        }
        DEFINE_FONT2 | DEFINE_FONT3 => {
            out.f("Character ID", r.u16()?);
            let flags = r.u8()?;
            out.flags(
                "Flags",
                flags as u32,
                &[(0x80, "HasLayout"), (0x40, "ShiftJIS"), (0x20, "SmallText"), (0x10, "ANSI"), (0x08, "WideOffsets"), (0x04, "WideCodes"), (0x02, "Italic"), (0x01, "Bold")],
            );
            out.f("Language", r.u8()?);
            let n = r.u8()? as usize;
            out.f("Name", String::from_utf8_lossy(r.bytes(n)?).trim_end_matches('\0'));
            out.f("Glyphs", r.u16()?);
        }
        DEFINE_FONT4 => {
            out.f("Character ID", r.u16()?);
            out.flags("Flags", r.u8()? as u32, &[(4, "HasFontData"), (2, "Italic"), (1, "Bold")]);
            out.f("Name", r.string()?);
        }
        DEFINE_FONT_NAME => {
            out.f("Font ID", r.u16()?);
            out.f("Name", r.string()?);
            out.f("Copyright", r.string()?);
        }
        DEFINE_FONT_INFO | DEFINE_FONT_INFO2 => {
            out.f("Font ID", r.u16()?);
            let n = r.u8()? as usize;
            out.f("Name", String::from_utf8_lossy(r.bytes(n)?));
        }
        DEFINE_FONT_ALIGN_ZONES => out.f("Font ID", r.u16()?),
        CSM_SETTINGS => {
            out.f("Text ID", r.u16()?);
            let flags = r.u8()?;
            out.f("Renderer", if flags & 0x40 != 0 { "Advanced" } else { "Normal" });
            out.f("Grid fit", ["None", "Pixel", "Subpixel", "?"][((flags >> 3) & 3) as usize]);
            out.f("Thickness", r.f32()?);
            out.f("Sharpness", r.f32()?);
        }
        DEFINE_SCALING_GRID => {
            out.f("Character ID", r.u16()?);
            out.rect("Splitter", Rect::read(r)?);
        }
        DEFINE_BUTTON => out.f("Character ID", r.u16()?),
        DEFINE_BUTTON2 => {
            out.f("Character ID", r.u16()?);
            out.f("Track as menu", r.u8()? & 1 != 0);
        }
        DEFINE_BUTTON_SOUND | DEFINE_BUTTON_CXFORM => out.f("Button ID", r.u16()?),
        DEFINE_VIDEO_STREAM => {
            out.f("Character ID", r.u16()?);
            out.f("Frames", r.u16()?);
            out.f("Size", format!("{} × {}", r.u16()?, r.u16()?));
            let flags = r.u8()?;
            out.f("Smoothing", flags & 1 != 0);
            out.f(
                "Codec",
                match r.u8()? {
                    2 => "Sorenson H.263",
                    3 => "Screen video",
                    4 => "VP6",
                    5 => "VP6 with alpha",
                    6 => "Screen video v2",
                    _ => "unknown",
                },
            );
        }
        VIDEO_FRAME => {
            out.f("Stream ID", r.u16()?);
            out.f("Frame", r.u16()?);
        }
        PLACE_OBJECT => {
            out.f("Character ID", r.u16()?);
            out.f("Depth", r.u16()?);
        }
        PLACE_OBJECT2 | PLACE_OBJECT3 | PLACE_OBJECT4 => place_object(tag.code, r, out)?,
        REMOVE_OBJECT => {
            out.f("Character ID", r.u16()?);
            out.f("Depth", r.u16()?);
        }
        REMOVE_OBJECT2 => out.f("Depth", r.u16()?),
        NAME_CHARACTER => {
            out.f("Character ID", r.u16()?);
            out.f("Name", r.string()?);
        }
        SET_TAB_INDEX => {
            out.f("Depth", r.u16()?);
            out.f("Tab index", r.u16()?);
        }
        DEFINE_SCENE_AND_FRAME_LABEL_DATA => {
            let scenes = r.encoded_u32()?;
            for _ in 0..scenes {
                let off = r.encoded_u32()?;
                out.f("Scene", format!("frame {}: {}", off + 1, r.string()?));
            }
            let labels = r.encoded_u32()?;
            for _ in 0..labels {
                let frame = r.encoded_u32()?;
                out.f("Frame label", format!("frame {}: {}", frame + 1, r.string()?));
            }
        }
        ENABLE_TELEMETRY => {
            r.skip(2)?;
            out.f("Password protected", r.remaining() >= 32);
        }
        GFX_EXPORTER_INFO => out.f("Exporter version", format!("0x{:x}", r.u16()?)),
        GFX_DEFINE_EXTERNAL_IMAGE | GFX_DEFINE_EXTERNAL_IMAGE2 | GFX_DEFINE_SUB_IMAGE | GFX_DEFINE_EXTERNAL_SOUND
        | GFX_DEFINE_EXTERNAL_STREAM_SOUND | GFX_DEFINE_COMPACTED_FONT => out.f("Character ID", r.u16()?),
        _ => {}
    }
    Ok(())
}

fn edit_text(r: &mut Reader, out: &mut Out) -> Result<()> {
    out.f("Character ID", r.u16()?);
    out.rect("Bounds", Rect::read(r)?);
    let f1 = r.u8()?;
    let f2 = r.u8()?;
    out.flags(
        "Flags",
        ((f1 as u32) << 8) | f2 as u32,
        &[
            (0x4000, "WordWrap"),
            (0x2000, "Multiline"),
            (0x1000, "Password"),
            (0x0800, "ReadOnly"),
            (0x0040, "AutoSize"),
            (0x0010, "NoSelect"),
            (0x0008, "Border"),
            (0x0004, "WasStatic"),
            (0x0002, "HTML"),
            (0x0001, "UseOutlines"),
        ],
    );
    let has_font = f1 & 0x01 != 0;
    if has_font {
        out.f("Font ID", r.u16()?);
    }
    if f2 & 0x80 != 0 {
        out.f("Font class", r.string()?);
    }
    if has_font {
        out.f("Font height", r.u16()? as f64 / 20.0);
    }
    if f1 & 0x04 != 0 {
        out.f("Text color", Rgba::read_rgba(r)?.hex());
    }
    if f1 & 0x02 != 0 {
        out.f("Max length", r.u16()?);
    }
    if f2 & 0x20 != 0 {
        out.f("Align", ["left", "right", "center", "justify"].get(r.u8()? as usize).copied().unwrap_or("?"));
        r.skip(8)?;
    }
    out.f("Variable", r.string()?);
    if f1 & 0x80 != 0 {
        out.f("Initial text", r.string()?);
    }
    Ok(())
}

fn place_object(code: u16, r: &mut Reader, out: &mut Out) -> Result<()> {
    let f1 = r.u8()?;
    let f2 = if code != PLACE_OBJECT2 { r.u8()? } else { 0 };
    out.f("Depth", r.u16()?);
    out.f("Mode", if f1 & 0x01 != 0 { if f1 & 0x02 != 0 { "replace" } else { "modify" } } else { "place" });
    if f2 & 0x08 != 0 || (f2 & 0x10 != 0 && f1 & 0x02 != 0) {
        out.f("Class name", r.string()?);
    }
    if f1 & 0x02 != 0 {
        out.f("Character ID", r.u16()?);
    }
    if f1 & 0x04 != 0 {
        skip_matrix(r)?;
        out.f("Matrix", "yes");
    }
    if f1 & 0x08 != 0 {
        skip_cxform(r, true)?;
        out.f("Color transform", "yes");
    }
    if f1 & 0x10 != 0 {
        out.f("Ratio", r.u16()?);
    }
    if f1 & 0x20 != 0 {
        out.f("Instance name", r.string()?);
    }
    if f1 & 0x40 != 0 {
        out.f("Clip depth", r.u16()?);
    }
    if f2 & 0x01 != 0 {
        out.f("Filters", r.u8()?);
        return Ok(());
    }
    if f2 & 0x02 != 0 {
        out.f("Blend mode", r.u8()?);
    }
    if f1 & 0x80 != 0 {
        out.f("Clip actions", "yes");
    }
    Ok(())
}

/// Names attached to characters by SymbolClass and ExportAssets.
pub fn character_names(swf: &crate::Swf) -> std::collections::HashMap<u16, String> {
    let mut names = std::collections::HashMap::new();
    for t in &swf.timeline.tags {
        if t.code == SYMBOL_CLASS || t.code == EXPORT_ASSETS {
            let mut r = Reader::new(&t.body);
            let Ok(n) = r.u16() else { continue };
            for _ in 0..n {
                let (Ok(id), Ok(name)) = (r.u16(), r.string()) else { break };
                names.entry(id).or_insert(name);
            }
        }
    }
    names
}
