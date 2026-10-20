//! Bitmap tags: DefineBits (+JPEGTables), DefineBitsJPEG2/3/4 and DefineBitsLossless/2.

use std::io::{Cursor, Read};

use image::{ImageFormat, RgbaImage};

use crate::error::{Error, Result};
use crate::io::Reader;
use crate::tags::{code::*, Tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    Gif,
    Jxr,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Gif => "gif",
            Format::Jxr => "jxr",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Png => "image/png",
            Format::Gif => "image/gif",
            Format::Jxr => "image/vnd.ms-photo",
        }
    }
}

pub fn is_image(code: u16) -> bool {
    matches!(code, DEFINE_BITS | DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 | DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2)
}

fn has_error_header(d: &[u8]) -> bool {
    d.len() > 4 && d[..4] == [0xff, 0xd9, 0xff, 0xd8]
}

pub fn sniff(d: &[u8]) -> Option<Format> {
    if has_error_header(d) || (d.len() > 2 && d[0] == 0xff && d[1] == 0xd8) {
        Some(Format::Jpeg)
    } else if d.len() > 6 && &d[..4] == b"GIF8" && (d[4] == b'7' || d[4] == b'9') && d[5] == b'a' {
        Some(Format::Gif)
    } else if d.len() > 4 && d[..4] == [0x49, 0x49, 0xbc, 0x01] {
        Some(Format::Jxr)
    } else if d.len() > 8 && d[..8] == [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        Some(Format::Png)
    } else {
        None
    }
}

fn marker_has_length(m: u8) -> bool {
    m != 0 && m != 0xd8 && m != 0xd9 && !(0xd0..=0xd7).contains(&m)
}

/// Port of FFDec's `JpegFixer`: drops the erroneous `FFD9 FFD8` prefix and the extra
/// EOI/SOI pairs Flash allows between table and image data.
pub fn fix_jpeg(d: &[u8]) -> Vec<u8> {
    if d.len() < 2 || d[0] != 0xff || (d[1] != 0xd8 && d[1] != 0xd9) {
        return d.to_vec();
    }
    let mut i = 2;
    if d[1] == 0xd9 {
        if d.len() < 6 || d[2] != 0xff || d[3] != 0xd8 || d[4] != 0xff || d[5] != 0xd8 {
            return d.to_vec();
        }
        i = 6;
    }
    let mut out = Vec::with_capacity(d.len());
    out.extend_from_slice(&[0xff, 0xd8]);
    let mut prev_eoi = false;
    while i < d.len() {
        let b = d[i];
        i += 1;
        if b != 0xff {
            out.push(b);
            prev_eoi = false;
            continue;
        }
        let Some(&m) = d.get(i) else {
            break;
        };
        i += 1;
        if m == 0 {
            out.extend_from_slice(&[0xff, 0]);
            prev_eoi = false;
            continue;
        }
        if m == 0xd8 {
            // Repeated SOI, or EOI+SOI: drop.
        } else if prev_eoi {
            out.extend_from_slice(&[0xff, 0xd9, 0xff, m]);
        } else if m != 0xd9 {
            out.extend_from_slice(&[0xff, m]);
        }
        if marker_has_length(m) {
            if i + 2 > d.len() {
                out.extend_from_slice(&d[i..]);
                return out;
            }
            let len = ((d[i] as usize) << 8) | d[i + 1] as usize;
            let end = (i + len.max(2)).min(d.len());
            out.extend_from_slice(&d[i..end]);
            i = end;
        }
        prev_eoi = m == 0xd9;
    }
    if prev_eoi {
        out.extend_from_slice(&[0xff, 0xd9]);
    }
    out
}

fn inflate(d: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = flate2::read::ZlibDecoder::new(d).read_to_end(&mut out);
    out
}

/// A decoded bitmap: the original encoded bytes when they can be written as-is, and
/// the pixels when the image has to be (re)encoded.
pub struct Bitmap {
    pub id: u16,
    pub original: Option<(Format, Vec<u8>)>,
    alpha: Option<Vec<u8>>,
    lossless: Option<RgbaImage>,
}

impl Bitmap {
    pub fn read(tag: &Tag, jpeg_tables: Option<&[u8]>) -> Result<Bitmap> {
        let mut r = Reader::new(&tag.body);
        let id = r.u16()?;
        match tag.code {
            DEFINE_BITS => {
                let mut data = jpeg_tables.unwrap_or(&[]).to_vec();
                data.extend_from_slice(r.rest());
                Ok(Bitmap { id, original: Some((Format::Jpeg, fix_jpeg(&data))), alpha: None, lossless: None })
            }
            DEFINE_BITS_JPEG2 => Ok(Bitmap { id, original: Some(encoded(r.rest())), alpha: None, lossless: None }),
            DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 => {
                let alpha_offset = r.u32()? as usize;
                if tag.code == DEFINE_BITS_JPEG4 {
                    r.u16()?;
                }
                let rest = r.rest();
                let split = alpha_offset.min(rest.len());
                let (img, alpha) = rest.split_at(split);
                let (fmt, bytes) = encoded(img);
                let alpha = (fmt == Format::Jpeg && !alpha.is_empty()).then(|| inflate(alpha)).filter(|a| !a.is_empty());
                Ok(Bitmap { id, original: Some((fmt, bytes)), alpha, lossless: None })
            }
            DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => {
                let img = lossless(&mut r, tag.code == DEFINE_BITS_LOSSLESS2)?;
                Ok(Bitmap { id, original: None, alpha: None, lossless: Some(img) })
            }
            _ => Err(Error::Other(format!("{} is not an image tag", tag.name()))),
        }
    }

    /// The format FFDec's default "PNG/GIF/JPEG" mode writes.
    pub fn native_format(&self) -> Format {
        match &self.original {
            Some((f, _)) if self.alpha.is_none() => *f,
            _ => Format::Png,
        }
    }

    /// Encoded bytes in the native format; `force_png` re-encodes everything as PNG.
    pub fn encode(&self, force_png: bool) -> Result<(Format, Vec<u8>)> {
        if !force_png || self.native_format() == Format::Png {
            if let (Some((f, b)), None) = (&self.original, &self.alpha) {
                return Ok((*f, b.clone()));
            }
        }
        let img = self.pixels()?;
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).map_err(|e| Error::Other(e.to_string()))?;
        Ok((Format::Png, out.into_inner()))
    }

    pub fn pixels(&self) -> Result<RgbaImage> {
        if let Some(img) = &self.lossless {
            return Ok(img.clone());
        }
        let (fmt, bytes) = self.original.as_ref().unwrap();
        if *fmt == Format::Jxr {
            return Err(Error::Other("JPEG XR images can only be exported as-is".into()));
        }
        let mut img = image::load_from_memory(bytes).map_err(|e| Error::Other(e.to_string()))?.to_rgba8();
        if let Some(alpha) = &self.alpha {
            for (px, &a) in img.pixels_mut().zip(alpha.iter()) {
                // Colour channels of JPEG3 data are premultiplied by the separate alpha.
                for c in &mut px.0[..3] {
                    *c = unpremultiply(*c, a);
                }
                px.0[3] = a;
            }
        }
        Ok(img)
    }
}

fn unpremultiply(c: u8, a: u8) -> u8 {
    if a == 0 {
        if c == 0 { 0 } else { 255 }
    } else {
        ((c as u32 * 255) / a as u32).min(255) as u8
    }
}

fn encoded(d: &[u8]) -> (Format, Vec<u8>) {
    match sniff(d) {
        Some(Format::Jpeg) | None => (Format::Jpeg, fix_jpeg(d)),
        Some(f) => (f, d.to_vec()),
    }
}

fn lossless(r: &mut Reader, alpha: bool) -> Result<RgbaImage> {
    let format = r.u8()?;
    let w = r.u16()? as usize;
    let h = r.u16()? as usize;
    let colors = if format == 3 { r.u8()? as usize + 1 } else { 0 };
    let data = inflate(r.rest());
    let mut img = RgbaImage::new(w as u32, h as u32);
    let px = |i: usize| data.get(i).copied().unwrap_or(0);
    match format {
        3 => {
            let entry = if alpha { 4 } else { 3 };
            let table = colors * entry;
            let stride = (w + 3) & !3;
            for y in 0..h {
                for x in 0..w {
                    let idx = px(table + y * stride + x) as usize;
                    let c = if idx < colors {
                        let o = idx * entry;
                        [px(o), px(o + 1), px(o + 2), if alpha { px(o + 3) } else { 255 }]
                    } else {
                        [0, 0, 0, 0]
                    };
                    img.put_pixel(x as u32, y as u32, image::Rgba(straight(c, alpha)));
                }
            }
        }
        4 => {
            let stride = (w * 2 + 3) & !3;
            for y in 0..h {
                for x in 0..w {
                    let o = y * stride + x * 2;
                    let v = ((px(o) as u16) << 8) | px(o + 1) as u16;
                    let s = |c: u16| ((c & 0x1f) << 3 | (c & 0x1f) >> 2) as u8;
                    img.put_pixel(x as u32, y as u32, image::Rgba([s(v >> 10), s(v >> 5), s(v), 255]));
                }
            }
        }
        5 => {
            for y in 0..h {
                for x in 0..w {
                    let o = (y * w + x) * 4;
                    let c = [px(o + 1), px(o + 2), px(o + 3), if alpha { px(o) } else { 255 }];
                    img.put_pixel(x as u32, y as u32, image::Rgba(straight(c, alpha)));
                }
            }
        }
        _ => return Err(Error::Other(format!("unknown lossless bitmap format {format}"))),
    }
    Ok(img)
}

/// DefineBitsLossless2 stores premultiplied colour.
fn straight(c: [u8; 4], premultiplied: bool) -> [u8; 4] {
    if !premultiplied || c[3] == 255 {
        return c;
    }
    [unpremultiply(c[0], c[3]), unpremultiply(c[1], c[3]), unpremultiply(c[2], c[3]), c[3]]
}

#[cfg(test)]
mod tests {
    use super::fix_jpeg;

    #[test]
    fn fixes_error_header_and_inner_eoi_soi() {
        let tables = [0xff, 0xd8, 0xff, 0xdb, 0x00, 0x03, 0x01, 0xff, 0xd9];
        let image = [0xff, 0xd8, 0xff, 0xc0, 0x00, 0x02, 0x55, 0xff, 0xd9];
        let mut joined = vec![0xff, 0xd9, 0xff, 0xd8];
        joined.extend_from_slice(&tables);
        joined.extend_from_slice(&image);
        let fixed = fix_jpeg(&joined);
        assert_eq!(fixed, [0xff, 0xd8, 0xff, 0xdb, 0x00, 0x03, 0x01, 0xff, 0xc0, 0x00, 0x02, 0x55, 0xff, 0xd9]);
    }
}
