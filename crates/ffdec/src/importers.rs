//! Asset importers: FFDec's `ImageImporter`, `SoundImporter` (DefineSound), `BinaryDataImporter`,
//! `ShapeImporter` (images, and SVG through svg_import), `TextImporter` and font glyphs.

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::error::{Error, Result};
use crate::io::Reader;
use crate::shape::{Fill, Record, Shape, Styles};
use crate::tags::code::*;
use crate::Swf;

fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(Error::Other(m.into()))
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).expect("writing to memory");
    e.finish().expect("writing to memory")
}

fn tag_of(swf: &Swf, path: &[usize]) -> Result<(u16, Vec<u8>)> {
    swf.tag(path).map(|t| (t.code, t.body.clone())).ok_or_else(|| Error::Other(format!("no tag at {path:?}")))
}

fn is_jpeg(d: &[u8]) -> bool {
    d.starts_with(&[0xff, 0xd8]) || d.starts_with(&[0xff, 0xd9, 0xff, 0xd8])
}

fn decode(data: &[u8]) -> Result<image::DynamicImage> {
    image::load_from_memory(data).or_else(|e| err(format!("reading the image: {e}")))
}

/// `ImageImporter.importImage(tag, data, 0)`: DefineBits becomes DefineBitsJPEG2, other
/// image tags keep their type.
pub fn image(swf: &mut Swf, path: &[usize], data: &[u8]) -> Result<()> {
    let (code, old) = tag_of(swf, path)?;
    let data = to_png(data)?;
    let id = old.get(..2).ok_or_else(|| Error::Other("image tag without a character id".into()))?.to_vec();
    let mut body = id;
    let new_code = match code {
        DEFINE_BITS | DEFINE_BITS_JPEG2 => {
            decode(&data)?;
            body.extend_from_slice(&data);
            DEFINE_BITS_JPEG2
        }
        DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 => {
            let img = decode(&data)?;
            let alpha = if is_jpeg(&data) { zlib(&vec![255; (img.width() * img.height()) as usize]) } else { zlib(&[]) };
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            if code == DEFINE_BITS_JPEG4 {
                body.extend_from_slice(old.get(6..8).unwrap_or(&[0, 0]));
            }
            body.extend_from_slice(&data);
            body.extend_from_slice(&alpha);
            code
        }
        DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => {
            body.extend(lossless(&decode(&data)?, code == DEFINE_BITS_LOSSLESS2)?);
            code
        }
        _ => return err("not an image tag"),
    };
    swf.set_tag(path, new_code, body)
}

/// A lossless bitmap after its character id: format 5, premultiplied for DefineBitsLossless2.
fn lossless(img: &image::DynamicImage, alpha: bool) -> Result<Vec<u8>> {
    let has_alpha = img.color().has_alpha();
    let rgba = img.to_rgba8();
    let mut pixels = Vec::with_capacity(rgba.len());
    for p in rgba.pixels() {
        let [r, g, b, a] = p.0;
        if !alpha {
            pixels.extend_from_slice(&[0xff, r, g, b]);
        } else {
            let a = if has_alpha { a as u32 } else { 255 };
            let m = |c: u8| (c as u32 * a / 255) as u8;
            pixels.extend_from_slice(&[a as u8, m(r), m(g), m(b)]);
        }
    }
    let (w, h) = (u16::try_from(rgba.width()), u16::try_from(rgba.height()));
    let (Ok(w), Ok(h)) = (w, h) else { return err("the image is too large for a lossless bitmap") };
    let mut body = vec![5];
    body.extend_from_slice(&w.to_le_bytes());
    body.extend_from_slice(&h.to_le_bytes());
    body.extend_from_slice(&zlib(&pixels));
    Ok(body)
}

fn to_png(data: &[u8]) -> Result<Vec<u8>> {
    if !(data.starts_with(b"BM") || data.starts_with(b"RIFF")) {
        return Ok(data.to_vec());
    }
    let mut png = Vec::new();
    decode(data)?.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).or_else(|e| err(format!("converting the image: {e}")))?;
    Ok(png)
}

fn edge(records: &mut Vec<Record>, dx: i32, dy: i32) {
    let bits = crate::io::bits_s(&[dx, dy]).saturating_sub(2);
    if (dx != 0 || dy != 0) && bits > 15 {
        let (x1, y1) = (dx / 2, dy / 2);
        let (x2, y2) = (x1 + (2 * x1 < dx) as i32, y1 + (2 * y1 < dy) as i32);
        edge(records, x1, y1);
        edge(records, x2, y2);
        return;
    }
    records.push(Record::Straight { dx, dy });
}

/// `ShapeImporter.addImage` without the insertion: the next character id, and a
/// DefineBitsJPEG2 for JPEG data or a DefineBitsLossless2 for anything else.
pub fn new_image(swf: &Swf, data: &[u8]) -> Result<(u16, u16, Vec<u8>)> {
    let data = to_png(data)?;
    let id = swf.next_character_id();
    let mut body = id.to_le_bytes().to_vec();
    let code = if is_jpeg(&data) {
        decode(&data)?;
        body.extend_from_slice(&data);
        DEFINE_BITS_JPEG2
    } else {
        body.extend(lossless(&decode(&data)?, true)?);
        DEFINE_BITS_LOSSLESS2
    };
    Ok((id, code, body))
}

/// `ShapeImporter.importImage`: the image goes into a new DefineBitsJPEG2 (JPEG) or
/// DefineBitsLossless2 tag before the shape, which becomes a rectangle filled with it, stretched
/// over the shape's bounds when `fill`, else at the image's own size.
pub fn shape_image(swf: &mut Swf, path: &[usize], data: &[u8], fill: bool) -> Result<()> {
    let tag = swf.tag(path).ok_or_else(|| Error::Other(format!("no tag at {path:?}")))?;
    let mut shape = crate::shape::ShapeTag::read(tag)?;
    let (id, code, body) = new_image(swf, data)?;
    let img = decode(&to_png(data)?)?;
    let (iw, ih) = (img.width() as i32 * 20, img.height() as i32 * 20);
    let mut rect = shape.bounds;
    if !fill {
        rect.x_max = rect.x_min + iw;
        rect.y_max = rect.y_min + ih;
        shape.bounds = rect;
    }
    let (w, h) = (rect.x_max - rect.x_min, rect.y_max - rect.y_min);
    let translated = rect.x_min != 0 || rect.y_min != 0;
    let scale = |a: i32, b: i32| if fill { (20.0 * a as f64 / b as f64) as f32 as f64 } else { 20.0 };
    let mut matrix = crate::types::Matrix::scale(scale(w, iw), scale(h, ih));
    if translated {
        (matrix.tx, matrix.ty) = (rect.x_min as f64, rect.y_min as f64);
    }
    let mut records = vec![Record::StyleChange { move_to: Some((rect.x_min, rect.y_min)), fill0: Some(1), fill1: None, line: None, new_styles: None }];
    edge(&mut records, w, 0);
    edge(&mut records, 0, h);
    edge(&mut records, -w, 0);
    edge(&mut records, 0, -h);
    shape.shape = Shape { styles: Styles { fills: vec![Fill::Bitmap { id, matrix, repeat: false, smooth: true }], lines: vec![] }, records };
    swf.insert_tag(path, crate::tags::Tag::new(code, body))?;
    let mut moved = path.to_vec();
    *moved.last_mut().unwrap() += 1;
    swf.set_tag_body(&moved, shape.write())
}

/// MPEG audio frames of `data` as (sampling rate, stereo, samples per channel).
fn mp3_info(data: &[u8]) -> Option<(u32, bool, u32)> {
    const BITRATES: [[u32; 15]; 5] = [
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    ];
    let mut first = None;
    let mut samples = 0u32;
    let mut i = 0;
    while i + 4 <= data.len() {
        let h = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        let version = (h >> 19) & 3;
        let layer = 4 - ((h >> 17) & 3);
        let bitrate = ((h >> 12) & 15) as usize;
        let rate = ((h >> 10) & 3) as usize;
        if h >> 21 != 0x7ff || version == 1 || layer == 4 || bitrate == 0 || bitrate == 15 || rate == 3 {
            i += 1;
            continue;
        }
        let mpeg1 = version == 3;
        let sr = [44100, 48000, 32000][rate] >> if mpeg1 { 0 } else if version == 2 { 1 } else { 2 };
        let table = if mpeg1 { layer as usize - 1 } else if layer == 1 { 3 } else { 4 };
        let br = BITRATES[table][bitrate] * 1000;
        let pad = (h >> 9) & 1;
        let (len, count) = match layer {
            1 => ((12 * br / sr + pad) * 4, 384),
            3 if !mpeg1 => (72 * br / sr + pad, 576),
            _ => (144 * br / sr + pad, 1152),
        };
        first.get_or_insert((sr, (h >> 6) & 3 != 3));
        samples += count;
        i += len.max(4) as usize;
    }
    first.map(|(sr, stereo)| (sr, stereo, samples))
}

fn strip_id3(mut d: &[u8]) -> &[u8] {
    if d.len() >= 10 && d.starts_with(b"ID3") {
        let size = (d[6] as usize) << 21 | (d[7] as usize) << 14 | (d[8] as usize) << 7 | d[9] as usize;
        d = d.get(size + 10..).unwrap_or(&[]);
    }
    if d.len() > 128 && d[d.len() - 128..].starts_with(b"TAG") {
        d = &d[..d.len() - 128];
        if d.len() > 227 && d[d.len() - 227..].starts_with(b"TAG+") {
            d = &d[..d.len() - 227];
        }
    }
    d
}

fn rate_code(rate: u32, allowed: &[(u32, u8)]) -> Result<u8> {
    match allowed.iter().find(|(r, _)| *r == rate) {
        Some(&(_, c)) => Ok(c),
        None => err(format!("unsupported sampling rate {rate} Hz; use {}", allowed.iter().map(|(r, _)| r.to_string()).collect::<Vec<_>>().join(", "))),
    }
}

/// `SoundImporter.importDefineSound`: MP3 data, or WAV stored uncompressed.
pub fn sound(swf: &mut Swf, path: &[usize], data: &[u8]) -> Result<()> {
    let (code, old) = tag_of(swf, path)?;
    if code != DEFINE_SOUND {
        return err("not a DefineSound tag");
    }
    let (format, rate, size16, stereo, count, payload) = if data.starts_with(b"RIFF") {
        let (rate, channels, bits, pcm) = wav(data)?;
        let rate = rate_code(rate, &[(5512, 0), (11025, 1), (22050, 2), (44100, 3)])?;
        let frame = channels as usize * (bits as usize / 8).max(1);
        (3u8, rate, bits == 16, channels == 2, (pcm.len() / frame) as u32, pcm.to_vec())
    } else {
        let mp3 = strip_id3(data);
        let Some((sr, stereo, samples)) = mp3_info(mp3) else { return err("not an MP3 or WAV file") };
        let rate = rate_code(sr, &[(11025, 1), (22050, 2), (44100, 3)])?;
        let mut payload = vec![0, 0];
        payload.extend_from_slice(mp3);
        (2u8, rate, true, stereo, samples, payload)
    };
    let mut body = old.get(..2).ok_or_else(|| Error::Other("sound tag without a character id".into()))?.to_vec();
    body.push(format << 4 | rate << 2 | (size16 as u8) << 1 | stereo as u8);
    body.extend_from_slice(&count.to_le_bytes());
    body.extend_from_slice(&payload);
    swf.set_tag(path, code, body)
}

/// A PCM WAV file as (rate, channels, bits per sample, sample data).
fn wav(data: &[u8]) -> Result<(u32, u16, u16, &[u8])> {
    let mut r = Reader::new(data);
    r.skip(8)?;
    if r.bytes(4)? != b"WAVE" {
        return err("not a WAV file");
    }
    let mut fmt = None;
    while r.remaining() >= 8 {
        let id = r.bytes(4)?;
        let len = r.u32()? as usize;
        let chunk = r.bytes(len.min(r.remaining()))?;
        if len % 2 == 1 && r.remaining() > 0 {
            r.skip(1)?;
        }
        match id {
            b"fmt " => {
                let mut c = Reader::new(chunk);
                let tag = c.u16()?;
                let channels = c.u16()?;
                let rate = c.u32()?;
                c.skip(6)?;
                let bits = c.u16()?;
                if tag != 1 || !(1..=2).contains(&channels) || !(bits == 8 || bits == 16) {
                    return err("only 8 or 16-bit mono or stereo PCM WAV files can be imported");
                }
                fmt = Some((rate, channels, bits));
            }
            b"data" => {
                let Some((rate, channels, bits)) = fmt else { return err("WAV data before its format") };
                return Ok((rate, channels, bits, chunk));
            }
            _ => {}
        }
    }
    err("WAV file without sound data")
}

/// `BinaryDataImporter.importData`.
pub fn binary(swf: &mut Swf, path: &[usize], data: &[u8]) -> Result<()> {
    let (code, old) = tag_of(swf, path)?;
    if code != DEFINE_BINARY_DATA || old.len() < 6 {
        return err("not a DefineBinaryData tag");
    }
    let mut body = old[..6].to_vec();
    body.extend_from_slice(data);
    swf.set_tag(path, code, body)
}

/// Sets the initial text of a DefineEditText, keeping everything else.
pub fn edit_text(swf: &mut Swf, path: &[usize], text: &str) -> Result<()> {
    let (code, old) = tag_of(swf, path)?;
    if code != DEFINE_EDIT_TEXT {
        return err("not a DefineEditText tag");
    }
    let mut r = Reader::new(&old);
    r.u16()?;
    crate::types::Rect::read(&mut r)?;
    let flags_at = r.pos();
    let (f1, f2) = (r.u8()?, r.u8()?);
    if f1 & 0x01 != 0 {
        r.u16()?;
    }
    if f2 & 0x80 != 0 {
        r.string()?;
    }
    if f1 & 0x01 != 0 || f2 & 0x80 != 0 {
        r.u16()?;
    }
    if f1 & 0x04 != 0 {
        r.skip(4)?;
    }
    if f1 & 0x02 != 0 {
        r.u16()?;
    }
    if f2 & 0x20 != 0 {
        r.skip(9)?;
    }
    r.string()?;
    let mut body = old[..r.pos()].to_vec();
    body[flags_at] |= 0x80;
    body.extend_from_slice(&crate::action::write::encode(crate::action::Charset::for_version(swf.header.version), text));
    body.push(0);
    swf.set_tag(path, code, body)
}

/// What an importer replaces a tag with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Image,
    Sound,
    Binary,
    Text,
    Shape,
    Font,
}

impl Asset {
    pub fn of(code: u16) -> Option<Asset> {
        Some(match code {
            DEFINE_BITS | DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 | DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => Asset::Image,
            DEFINE_SOUND => Asset::Sound,
            DEFINE_BINARY_DATA => Asset::Binary,
            DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT => Asset::Text,
            DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => Asset::Shape,
            DEFINE_FONT2 | DEFINE_FONT3 | DEFINE_FONT4 => Asset::Font,
            _ => return None,
        })
    }

    /// File extensions the importer reads.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Asset::Image => &["png", "jpg", "jpeg", "gif", "bmp", "webp"],
            Asset::Shape => &["svg", "png", "jpg", "jpeg", "gif", "bmp", "webp"],
            Asset::Sound => &["mp3", "wav"],
            Asset::Binary => &[],
            Asset::Text => &["txt"],
            Asset::Font => &["ttf", "otf"],
        }
    }
}

/// Replaces a font from a TrueType or OpenType file: a DefineFont4 takes the file as its font
/// data; a DefineFont2/3 gets new glyphs for the characters it has and for `extra`, as FFDec's
/// font panel adds them. Returns how many glyphs were set.
pub fn font(swf: &mut Swf, path: &[usize], data: &[u8], extra: &[char]) -> Result<usize> {
    let (code, old) = tag_of(swf, path)?;
    if code == DEFINE_FONT4 {
        let mut r = Reader::new(&old);
        r.u16()?;
        let flags = r.u8()?;
        r.string()?;
        let mut body = old[..r.pos()].to_vec();
        body[2] = flags | 0x04;
        body.extend_from_slice(data);
        swf.set_tag(path, code, body)?;
        return Ok(0);
    }
    let mut chars = crate::font_edit::characters(swf, path)?;
    chars.extend(extra.iter().filter(|c| !chars.contains(c)).copied().collect::<Vec<_>>());
    let id = swf.tag(path).and_then(|t| t.character_id()).unwrap_or(0);
    let n = crate::font_edit::add_characters(swf, path, data, &chars)?;
    relayout_texts(swf, id)?;
    Ok(n)
}

/// Lays the static texts using `font` out again from their formatted text without letter
/// spacing, so their advances follow the font's new glyphs (FFDec's font panel offers this).
fn relayout_texts(swf: &mut Swf, font: u16) -> Result<()> {
    let paths: Vec<usize> = swf.timeline.tags.iter().enumerate().filter(|(_, t)| t.code == DEFINE_TEXT || t.code == DEFINE_TEXT2).map(|(i, _)| i).collect();
    for i in paths {
        let mut t = crate::text::StaticText::read(&swf.timeline.tags[i])?;
        if !t.records.iter().any(|r| r.font.is_some_and(|f| f.0 == font)) {
            continue;
        }
        let text = crate::text::formatted(swf, &t, true);
        crate::text::set_formatted(swf, &mut t, &text, None).map_err(|e| Error::Other(e.to_string()))?;
        let body = t.write();
        swf.set_tag_body(&[i], body)?;
    }
    Ok(())
}

fn is_svg(data: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&data[..data.len().min(4096)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with('<') && head.contains("<svg")
}

/// Replaces the asset at `path` with the contents of a file.
pub fn import(swf: &mut Swf, path: &[usize], data: &[u8]) -> Result<()> {
    let code = swf.tag(path).map(|t| t.code).ok_or_else(|| Error::Other(format!("no tag at {path:?}")))?;
    match Asset::of(code) {
        Some(Asset::Image) => image(swf, path, data),
        Some(Asset::Sound) => sound(swf, path, data),
        Some(Asset::Binary) => binary(swf, path, data),
        Some(Asset::Shape) if is_svg(data) => crate::svg_import::import(swf, path, &String::from_utf8_lossy(data), true),
        Some(Asset::Shape) => shape_image(swf, path, data, true),
        Some(Asset::Font) => font(swf, path, data, &[]).map(|_| ()),
        Some(Asset::Text) if code == DEFINE_EDIT_TEXT => edit_text(swf, path, &String::from_utf8_lossy(data)),
        Some(Asset::Text) => crate::text::import(swf, path, &String::from_utf8_lossy(data)).map_err(|e| Error::Other(e.to_string())),
        None => err(format!("{} tags cannot be replaced yet", crate::tags::display_name(code))),
    }
}
