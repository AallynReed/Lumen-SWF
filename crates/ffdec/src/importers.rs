//! Asset importers: FFDec's `ImageImporter`, `SoundImporter` (DefineSound),
//! `BinaryDataImporter` and the plain-text path of `TextImporter` for edit texts.

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::error::{Error, Result};
use crate::io::Reader;
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
    let mut data = data.to_vec();
    if data.starts_with(b"BM") || data.starts_with(b"RIFF") {
        let mut png = Vec::new();
        decode(&data)?.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).or_else(|e| err(format!("converting the image: {e}")))?;
        data = png;
    }
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
            let img = decode(&data)?;
            let has_alpha = img.color().has_alpha();
            let rgba = img.to_rgba8();
            let mut pixels = Vec::with_capacity(rgba.len());
            for p in rgba.pixels() {
                let [r, g, b, a] = p.0;
                if code == DEFINE_BITS_LOSSLESS {
                    pixels.extend_from_slice(&[0xff, r, g, b]);
                } else {
                    let a = if has_alpha { a as u32 } else { 255 };
                    let m = |c: u8| (c as u32 * a / 255) as u8;
                    pixels.extend_from_slice(&[a as u8, m(r), m(g), m(b)]);
                }
            }
            let (w, h) = (u16::try_from(rgba.width()), u16::try_from(rgba.height()));
            let (Ok(w), Ok(h)) = (w, h) else { return err("the image is too large for a lossless bitmap") };
            body.push(5);
            body.extend_from_slice(&w.to_le_bytes());
            body.extend_from_slice(&h.to_le_bytes());
            body.extend_from_slice(&zlib(&pixels));
            code
        }
        _ => return err("not an image tag"),
    };
    swf.set_tag(path, new_code, body)
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
