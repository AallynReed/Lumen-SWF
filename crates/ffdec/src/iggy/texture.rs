//! `IggyTextureDecoder` and `Bc7Decoder`: the raw textures of a `.raw.iggytex` companion file
//! as ARGB pixels, and an external bitmap cut out of its packed texture.

use std::sync::{Arc, Mutex};

use image::RgbaImage;

use super::stream::err;
use crate::error::Result;
use crate::io::Reader;

pub const RGBA_8888: u32 = 0;
pub const RGBA_4444_LE: u32 = 1;
pub const RGBA_5551_LE: u32 = 2;
pub const LA_88: u32 = 3;
pub const LA_44: u32 = 4;
pub const I_8: u32 = 5;
pub const I_4: u32 = 6;
pub const L_8: u32 = 7;
pub const L_4: u32 = 8;
pub const DXT1: u32 = 9;
pub const DXT3: u32 = 10;
pub const DXT5: u32 = 11;
pub const BC7: u32 = 13;

const MAX_BYTES: u64 = 268_435_456;

fn level_length(w: u64, h: u64, format: u32) -> Result<u64> {
    let n = match format {
        RGBA_8888 => w * h * 4,
        RGBA_4444_LE | RGBA_5551_LE | LA_88 => w * h * 2,
        LA_44 | I_8 | I_4 | L_8 | L_4 => w * h,
        DXT1 | DXT3 | DXT5 | BC7 => w.div_ceil(4) * h.div_ceil(4) * if format == DXT1 { 8 } else { 16 },
        f => return err(format!("Unsupported Iggy raw texture format {f}")),
    };
    if n > MAX_BYTES {
        return err("Iggy texture data exceeds 256 MiB");
    }
    Ok(n)
}

/// The bytes of a texture with its mip chain; the high byte of `format` is the level count.
pub fn data_length(width: u32, height: u32, format: u32) -> Result<u64> {
    let (mut w, mut h) = (width as u64, height as u64);
    if w == 0 || h == 0 || w * h > MAX_BYTES / 4 || format > 65535 {
        return err("Invalid Iggy texture dimensions or metadata");
    }
    let levels = (format >> 8).max(1);
    let maximum = 64 - w.max(h).leading_zeros();
    if levels > maximum {
        return err(format!("Invalid Iggy texture mip count {levels}"));
    }
    let mut n = 0;
    for _ in 0..levels {
        n += level_length(w, h, format & 255)?;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    if n > MAX_BYTES {
        return err("Iggy texture mip chain exceeds 256 MiB");
    }
    Ok(n)
}

fn u16_at(d: &[u8], at: usize) -> u32 {
    d[at] as u32 | (d[at + 1] as u32) << 8
}

fn rgb565(v: u32) -> u32 {
    let (r, g, b) = (v >> 11, (v >> 5) & 63, v & 31);
    0xff00_0000 | ((r << 3) | (r >> 2)) << 16 | ((g << 2) | (g >> 4)) << 8 | (b << 3) | (b >> 2)
}

fn mix(a: u32, b: u32, wa: u32, wb: u32) -> u32 {
    let mut r = 0xff00_0000;
    for s in [0, 8, 16] {
        r |= ((((a >> s) & 255) * wa + ((b >> s) & 255) * wb) / (wa + wb)) << s;
    }
    r
}

fn dxt(d: &[u8], width: usize, height: usize, format: u32, px: &mut [u32]) {
    let mut at = 0;
    let mut colors = [0u32; 4];
    let mut alpha = [0u32; 8];
    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            let ca = at + if format == DXT1 { 0 } else { 8 };
            let (first, second) = (u16_at(d, ca), u16_at(d, ca + 2));
            colors[0] = rgb565(first);
            colors[1] = rgb565(second);
            if format != DXT1 || first > second {
                colors[2] = mix(colors[0], colors[1], 2, 1);
                colors[3] = mix(colors[0], colors[1], 1, 2);
            } else {
                colors[2] = mix(colors[0], colors[1], 1, 1);
                colors[3] = 0;
            }
            let mut indices = 0u64;
            let mut alpha_indices = 0u64;
            for i in 0..4 {
                indices |= (d[ca + 4 + i] as u64) << (8 * i);
            }
            if format == DXT5 {
                alpha[0] = d[at] as u32;
                alpha[1] = d[at + 1] as u32;
                let den = if alpha[0] > alpha[1] { 7 } else { 5 };
                for i in 1..den {
                    alpha[i as usize + 1] = ((den - i) * alpha[0] + i * alpha[1]) / den;
                }
                if den == 5 {
                    alpha[6] = 0;
                    alpha[7] = 255;
                }
                for i in 0..6 {
                    alpha_indices |= (d[at + 2 + i] as u64) << (8 * i);
                }
            }
            for i in 0..16 {
                let mut p = colors[((indices >> (2 * i)) & 3) as usize];
                if format == DXT3 {
                    p = (p & 0xff_ffff) | (((d[at + i / 2] as u32) >> (4 * (i % 2))) & 15) * 17 << 24;
                } else if format == DXT5 {
                    p = (p & 0xff_ffff) | alpha[((alpha_indices >> (3 * i)) & 7) as usize] << 24;
                }
                let (qx, qy) = (x + i % 4, y + i / 4);
                if qx < width && qy < height {
                    px[qy * width + qx] = p;
                }
            }
            at += if format == DXT1 { 8 } else { 16 };
        }
    }
}

/// ARGB pixels of the first mip level.
pub fn decode(width: u32, height: u32, format: u32, d: &[u8]) -> Result<Vec<u32>> {
    let len = data_length(width, height, format)?;
    if (d.len() as u64) < len {
        return err("Truncated Iggy external texture");
    }
    let f = format & 255;
    let (w, h) = (width as usize, height as usize);
    if f == BC7 {
        return bc7(w, h, d);
    }
    let mut px = vec![0u32; w * h];
    if matches!(f, DXT1 | DXT3 | DXT5) {
        dxt(d, w, h, f, &mut px);
        return Ok(px);
    }
    for (i, p) in px.iter_mut().enumerate() {
        let (r, g, b, a);
        if f == RGBA_8888 {
            (r, g, b, a) = (d[4 * i] as u32, d[4 * i + 1] as u32, d[4 * i + 2] as u32, d[4 * i + 3] as u32);
        } else if f == RGBA_4444_LE || f == RGBA_5551_LE {
            let v = u16_at(d, 2 * i);
            if f == RGBA_4444_LE {
                (a, r, g, b) = ((v >> 12) * 17, ((v >> 8) & 15) * 17, ((v >> 4) & 15) * 17, (v & 15) * 17);
            } else {
                let e = |c: u32| (c << 3) | (c >> 2);
                (a, r, g, b) = ((v >> 15) * 255, e((v >> 10) & 31), e((v >> 5) & 31), e(v & 31));
            }
        } else {
            let v = d[if f == LA_88 { 2 * i } else { i }] as u32;
            a = if f == LA_88 {
                d[2 * i + 1] as u32
            } else if f == LA_44 {
                (v >> 4) * 17
            } else if f == I_8 || f == I_4 {
                v
            } else {
                255
            };
            r = if f == LA_44 { (v & 15) * 17 } else { v };
            g = r;
            b = r;
        }
        *p = a << 24 | r << 16 | g << 8 | b;
    }
    Ok(px)
}

// ---- BC7 ----

const COLOR_BITS: [u32; 8] = [4, 6, 5, 7, 5, 7, 7, 5];
const ALPHA_BITS: [u32; 8] = [0, 0, 0, 0, 6, 8, 7, 5];
const WEIGHTS: [&[u32]; 3] = [&[0, 21, 43, 64], &[0, 9, 18, 27, 37, 46, 55, 64], &[0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64]];

include!("bc7_partitions.rs");

struct Bits<'a> {
    d: &'a [u8],
    offset: usize,
    pos: usize,
}

impl Bits<'_> {
    fn read(&mut self, n: u32) -> u32 {
        let i = self.offset + (self.pos >> 3);
        let shift = self.pos & 7;
        let mut v = (self.d[i] as u32) >> shift;
        if shift as u32 + n > 8 {
            v |= (self.d[i + 1] as u32) << (8 - shift);
        }
        self.pos += n as usize;
        v & ((1 << n) - 1)
    }
}

fn partition(subsets: u32, p: u32, pixel: usize) -> u32 {
    if subsets == 1 {
        if pixel == 0 {
            128
        } else {
            0
        }
    } else {
        PARTITIONS[subsets as usize - 2][p as usize][pixel] as u32
    }
}

fn block(bits: &mut Bits, out: &mut [u32; 16]) {
    let mut mode = 0;
    while mode < 8 && bits.read(1) == 0 {
        mode += 1;
    }
    if mode == 8 {
        *out = [0; 16];
        return;
    }
    let subsets = match mode {
        0 | 2 => 3,
        1 | 3 | 7 => 2,
        _ => 1,
    };
    let part = if subsets == 1 { 0 } else { bits.read(if mode == 0 { 4 } else { 6 }) };
    let rotation = if mode == 4 || mode == 5 { bits.read(2) } else { 0 };
    let selection = if mode == 4 { bits.read(1) } else { 0 };
    let n = subsets as usize * 2;
    let mut ep = [[0u32; 4]; 6];
    for c in 0..4 {
        let p = if c == 3 { ALPHA_BITS[mode] } else { COLOR_BITS[mode] };
        for e in ep.iter_mut().take(n) {
            e[c] = if p == 0 { 0 } else { bits.read(p) };
        }
    }
    let pbits = 0xcb & (1 << mode) != 0;
    if pbits {
        let mut pb = 0;
        for (i, e) in ep.iter_mut().take(n).enumerate() {
            if mode != 1 || i & 1 == 0 {
                pb = bits.read(1);
            }
            for v in e.iter_mut() {
                *v = (*v << 1) | pb;
            }
        }
    }
    for e in ep.iter_mut().take(n) {
        for (c, v) in e.iter_mut().enumerate() {
            if c == 3 && ALPHA_BITS[mode] == 0 {
                *v = 255;
            } else {
                let p = if c == 3 { ALPHA_BITS[mode] } else { COLOR_BITS[mode] } + pbits as u32;
                let x = *v << (8 - p);
                *v = x | (x >> p);
            }
        }
    }
    let index_bits = match mode {
        0 | 1 => 3,
        6 => 4,
        _ => 2,
    };
    let secondary = match mode {
        4 => 3,
        5 => 2,
        _ => 0,
    };
    let mut idx = [0u32; 16];
    for (p, v) in idx.iter_mut().enumerate() {
        let s = partition(subsets, part, p);
        *v = bits.read(index_bits - if s & 128 != 0 { 1 } else { 0 });
    }
    let lerp = |a: u32, b: u32, w: u32| ((64 - w) * a + w * b + 32) >> 6;
    for p in 0..16 {
        let s = (partition(subsets, part, p) & 3) as usize;
        let pw = WEIGHTS[index_bits as usize - 2][idx[p] as usize];
        let sw = if secondary == 0 { pw } else { WEIGHTS[secondary as usize - 2][bits.read(secondary - if p == 0 { 1 } else { 0 }) as usize] };
        let (cw, aw) = if selection == 0 { (pw, sw) } else { (sw, pw) };
        let (a0, a1) = (ep[s * 2], ep[s * 2 + 1]);
        let (mut r, mut g, mut b, mut a) = (lerp(a0[0], a1[0], cw), lerp(a0[1], a1[1], cw), lerp(a0[2], a1[2], cw), lerp(a0[3], a1[3], aw));
        match rotation {
            1 => std::mem::swap(&mut r, &mut a),
            2 => std::mem::swap(&mut g, &mut a),
            3 => std::mem::swap(&mut b, &mut a),
            _ => {}
        }
        out[p] = a << 24 | r << 16 | g << 8 | b;
    }
}

fn bc7(w: usize, h: usize, d: &[u8]) -> Result<Vec<u32>> {
    let blocks = w.div_ceil(4) * h.div_ceil(4);
    if w == 0 || h == 0 || blocks * 16 > d.len() {
        return err("Invalid BC7 dimensions or truncated blocks");
    }
    let mut px = vec![0u32; w * h];
    let mut bits = Bits { d, offset: 0, pos: 0 };
    let mut b = [0u32; 16];
    for y in (0..h).step_by(4) {
        for x in (0..w).step_by(4) {
            block(&mut bits, &mut b);
            bits.offset += 16;
            bits.pos = 0;
            for row in 0..4 {
                if y + row >= h {
                    break;
                }
                let n = 4.min(w - x);
                px[(y + row) * w + x..(y + row) * w + x + n].copy_from_slice(&b[row * 4..row * 4 + n]);
            }
        }
    }
    Ok(px)
}

/// An external bitmap: its texture decoded from the companion file and the region
/// `(offset, scale)` drawn at the bitmap's own size, like `IggyBitmapConvertor.readExternal`.
pub fn external_image(texture: &[u8], resource: (u32, u16, u16, u16), size: (u32, u32), scale: [f32; 4]) -> Result<RgbaImage> {
    let (offset, format, rw, rh) = resource;
    let (w, h) = (rw as u32, rh as u32);
    let len = data_length(w, h, format as u32)?;
    let start = offset as u64;
    if start + len > texture.len() as u64 {
        return err("Iggy texture resource outside companion file");
    }
    let px = decode(w, h, format as u32, &texture[start as usize..(start + len) as usize])?;
    let (tw, th) = size;
    if tw == 0 || th == 0 || tw as u64 * th as u64 > 67_108_864 {
        return err("Invalid Iggy bitmap dimensions");
    }
    let [sx, sy, ox, oy] = scale;
    let round = |v: f32| super::swf::java_round(v);
    let x0 = round(ox * w as f32);
    let y0 = round(oy * h as f32);
    let x1 = round((ox + sx) * w as f32);
    let y1 = round((oy + sy) * h as f32);
    let mut img = RgbaImage::new(tw, th);
    for dy in 0..th {
        for dx in 0..tw {
            let sxp = x0 as f64 + (dx as f64 + 0.5) * (x1 - x0) as f64 / tw as f64;
            let syp = y0 as f64 + (dy as f64 + 0.5) * (y1 - y0) as f64 / th as f64;
            let (qx, qy) = (sxp.floor() as i64, syp.floor() as i64);
            if qx < 0 || qy < 0 || qx >= w as i64 || qy >= h as i64 {
                continue;
            }
            let p = px[qy as usize * w as usize + qx as usize];
            img.put_pixel(dx, dy, image::Rgba([(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]));
        }
    }
    Ok(img)
}

/// `IggyExternalImage`: a bitmap whose pixels live in the companion texture file, or a
/// replacement image set in the editor.
pub struct External {
    pub width: u32,
    pub height: u32,
    pub pixel: u32,
    pub kind: u32,
    pub format: u32,
    pub length: u32,
    pub scale: [f32; 4],
    pub file_name: Vec<u8>,
    /// Resource type (8 for a platform texture record), offset, format, width and height.
    pub resource: (u8, u32, u16, u16, u16),
    pub replacement: Vec<u8>,
}

fn cstring(r: &mut Reader) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    loop {
        match r.u8()? {
            0 => return Ok(v),
            b => v.push(b),
        }
    }
}

impl External {
    pub fn read(body: &[u8]) -> Result<External> {
        let mut r = Reader::new(body);
        if r.u8()? != 1 {
            return err("Unsupported IggyExternalImage version");
        }
        r.u16()?;
        let (width, height) = (r.u16()? as u32, r.u16()? as u32);
        let (pixel, kind, format) = (r.u8()? as u32, r.u8()? as u32, r.u8()? as u32);
        let length = r.u32()?;
        let scale = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        let file_name = cstring(&mut r)?;
        let resource = (r.u8()?, r.u32()?, r.u16()?, r.u16()?, r.u16()?);
        let n = r.u32()? as usize;
        let replacement = r.bytes(n)?.to_vec();
        Ok(External { width, height, pixel, kind, format, length, scale, file_name, resource, replacement })
    }

    /// The pixels from the registered companion file, or a red placeholder.
    pub fn image(&self) -> RgbaImage {
        let name = String::from_utf8_lossy(&self.file_name).to_ascii_lowercase();
        let texture = COMPANIONS.lock().unwrap().iter().rev().find(|c| c.0 == name).map(|c| c.1.clone());
        let (ty, offset, format, w, h) = self.resource;
        if let Some(t) = texture.filter(|_| ty == 8 && offset != u32::MAX) {
            if let Ok(img) = external_image(&t, (offset, format, w, h), (self.width, self.height), self.scale) {
                return img;
            }
        }
        let ok = self.width > 0 && self.height > 0 && self.width as u64 * self.height as u64 <= 67_108_864;
        let (w, h) = if ok { (self.width, self.height) } else { (1, 1) };
        RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 0, 255]))
    }
}

static COMPANIONS: Mutex<Vec<(String, Arc<Vec<u8>>)>> = Mutex::new(Vec::new());

/// Makes a companion texture file available to the external bitmaps naming it.
pub fn register_companion(file_name: &str, data: Vec<u8>) {
    COMPANIONS.lock().unwrap().push((file_name.to_ascii_lowercase(), Arc::new(data)));
}
