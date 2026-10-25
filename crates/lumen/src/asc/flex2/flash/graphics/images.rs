//! Ports of `flash.graphics.images` (`JPEGImage`, `LosslessImage`, `ImageUtil`) and of the AWT pipeline they read
//! through: `Toolkit.createImage(bytes)` picks a `sun.awt.image` decoder by signature (`InputStreamImageSource`), and a
//! `PixelGrabber(image, 0, 0, -1, -1, true)` collects default-RGB pixels. `PNGImageDecoder`, `GifImageDecoder` (its
//! native LZW loop) and `IndexColorModel` are ported for the pixels; `JPEGImageDecoder` only as far as the frame
//! header, since mxmlc keeps JPEG bytes as they are and reads only the size.

use std::cell::RefCell;
use std::io::Read;

/// What `ImageUtil.getPixelGrabber` leaves: the size once `setDimensions` ran, and the pixels set by then.
pub struct Grabbed {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<i32>,
}

/// `PixelGrabber` with `forceRGB`: pixels outside what the decoder sent stay 0.
struct Grabber {
    width: i32,
    height: i32,
    pixels: Option<Vec<i32>>,
}

impl Grabber {
    fn set_dimensions(&mut self, w: i32, h: i32) {
        self.width = w;
        self.height = h;
        if w > 0 && h > 0 {
            self.pixels = Some(vec![0; (w as usize) * (h as usize)]);
        }
    }

    /// `setPixels` clipped to the image, as `PixelGrabber` clips to its rectangle.
    fn set_pixels(&mut self, x: i32, y: i32, w: i32, h: i32, src: &[i32], off: usize, scan: usize) {
        let Some(px) = self.pixels.as_mut() else { return };
        let (gw, gh) = (self.width, self.height);
        for row in 0..h {
            let dy = y + row;
            if dy < 0 || dy >= gh {
                continue;
            }
            for col in 0..w {
                let dx = x + col;
                if dx < 0 || dx >= gw {
                    continue;
                }
                px[(dy * gw + dx) as usize] = src[off + row as usize * scan + col as usize];
            }
        }
    }

    fn finish(self) -> Result<Grabbed, String> {
        match self.pixels {
            Some(p) => Ok(Grabbed { width: self.width, height: self.height, pixels: p }),
            None if self.width > 0 || self.height > 0 => Ok(Grabbed { width: self.width, height: self.height, pixels: Vec::new() }),
            None => Err(String::new()),
        }
    }
}

/// `IndexColorModel`'s `rgb` table and `getRGB(pixel)`.
#[derive(Clone)]
pub struct IndexColorModel {
    rgb: Vec<i32>,
    pixel_mask: i32,
}

impl IndexColorModel {
    /// `IndexColorModel(bits, size, r, g, b[, a])` with an optional transparent index.
    fn from_rgb(bits: i32, size: usize, r: &[u8], g: &[u8], b: &[u8], a: Option<&[u8]>, trans: i32) -> IndexColorModel {
        let real = (1usize << bits).max(size).max(256);
        let mut rgb = vec![0i32; real];
        for i in 0..size {
            let alpha = a.map_or(0xff, |a| a[i] as i32);
            rgb[i] = (alpha << 24) | ((r[i] as i32) << 16) | ((g[i] as i32) << 8) | b[i] as i32;
        }
        Self::finish(rgb, bits, size, trans)
    }

    /// `IndexColorModel(bits, size, cmap, start, hasalpha=false, trans)`.
    fn from_cmap(bits: i32, size: usize, cmap: &[u8], trans: i32) -> IndexColorModel {
        let real = (1usize << bits).max(size).max(256);
        let mut rgb = vec![0i32; real];
        for i in 0..size {
            let (r, g, b) = (cmap[i * 3] as i32, cmap[i * 3 + 1] as i32, cmap[i * 3 + 2] as i32);
            rgb[i] = (0xff << 24) | (r << 16) | (g << 8) | b;
        }
        Self::finish(rgb, bits, size, trans)
    }

    fn finish(mut rgb: Vec<i32>, bits: i32, size: usize, trans: i32) -> IndexColorModel {
        if trans >= 0 && (trans as usize) < size {
            rgb[trans as usize] &= 0x00ffffff;
        }
        let maskbits = if bits == 3 {
            4
        } else if bits > 4 && bits < 8 {
            8
        } else {
            bits
        };
        IndexColorModel { rgb, pixel_mask: (1 << maskbits) - 1 }
    }

    fn get_rgb(&self, pixel: i32) -> i32 {
        self.rgb[(pixel & self.pixel_mask) as usize]
    }

    pub fn get_transparent_pixel(&self) -> i32 {
        -1
    }
}

thread_local! {
    /// `PNGImageDecoder.greyModels`: built once per JVM by the first grey image of each depth.
    static GREY_MODELS: RefCell<[Option<IndexColorModel>; 4]> = const { RefCell::new([None, None, None, None]) };
}

/// Forgets the grey models, as a new mxmlc process starts without them.
pub fn reset() {
    GREY_MODELS.with(|g| *g.borrow_mut() = [None, None, None, None]);
}

/// `Toolkit.createImage(bytes)` + `ImageUtil.getPixelGrabber(image, location)`.
pub fn grab(bytes: &[u8], location: &str) -> Result<Grabbed, String> {
    let fail = || format!("Failed to grab pixels for image {location}");
    let mut g = Grabber { width: -1, height: -1, pixels: None };
    if bytes.len() >= 4 && &bytes[..4] == b"GIF8" {
        let _ = decode_gif(bytes, &mut g);
    } else if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        if let Some((w, h)) = jpeg_size(bytes) {
            g.width = w;
            g.height = h;
            return Ok(Grabbed { width: w, height: h, pixels: Vec::new() });
        }
    } else if bytes.len() >= 8 && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10] {
        let _ = decode_png(bytes, &mut g);
    }
    g.finish().map_err(|_| fail())
}

/// The frame size `jpeg_read_header` reports before `JPEGImageDecoder` sends it, or `None` where libjpeg 6b gives up
/// first (no frame header, or a lossless/hierarchical one).
pub fn jpeg_size(b: &[u8]) -> Option<(i32, i32)> {
    let mut i = 2;
    loop {
        while i < b.len() && b[i] != 0xff {
            i += 1;
        }
        while i < b.len() && b[i] == 0xff {
            i += 1;
        }
        let m = *b.get(i)?;
        i += 1;
        match m {
            0xd8 | 0x01 | 0xd0..=0xd7 => continue,
            0xd9 | 0xda => return None,
            _ => {}
        }
        let len = u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as usize;
        match m {
            0xc0 | 0xc1 | 0xc2 | 0xc9 | 0xca => {
                let h = u16::from_be_bytes([*b.get(i + 3)?, *b.get(i + 4)?]) as i32;
                let w = u16::from_be_bytes([*b.get(i + 5)?, *b.get(i + 6)?]) as i32;
                return Some((w, h));
            }
            0xc3 | 0xc5..=0xc7 | 0xcb | 0xcd..=0xcf => return None,
            _ => {}
        }
        i += len;
    }
}

const PNG_GRAY: u8 = 0;
const PNG_PALETTE: u8 = 1;
const PNG_COLOR: u8 = 2;
const PNG_ALPHA: u8 = 4;

/// `PNGImageDecoder.produceImage`, feeding the grabber row by row (or pass by pass when interlaced).
fn decode_png(bytes: &[u8], g: &mut Grabber) -> Result<(), String> {
    let mut pos = 8;
    let (mut width, mut height, mut bit_depth, mut color_type, mut interlace) = (0i32, 0i32, 0u8, 0u8, 0u8);
    let mut red_map: Option<Vec<u8>> = None;
    let (mut green_map, mut blue_map) = (Vec::new(), Vec::new());
    let mut alpha_map: Option<Vec<u8>> = None;
    let mut transparent_pixel: i32 = -1;
    let mut transparent_pixel_16: Option<[u8; 6]> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_idat = false;
    let get_short = |b: &[u8], p: usize| -> i32 { (((b[p] as i32) << 8) | b[p + 1] as i32) as i16 as i32 };
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap());
        let key = &bytes[pos + 4..pos + 8];
        if (len as i32) < 0 {
            return Err("bogus length".into());
        }
        let len = len as usize;
        if pos + 12 + len > bytes.len() {
            break;
        }
        let st = pos + 8;
        let data = &bytes[st..st + len];
        let crc = u32::from_be_bytes(bytes[st + len..st + len + 4].try_into().unwrap());
        let mut h = crc32fast_hash(&bytes[pos + 4..st + len]);
        h ^= 0;
        if h != crc {
            return Err("crc corruption".into());
        }
        pos += len + 12;
        match key {
            b"IHDR" => {
                if len != 13 {
                    return Err("bogus IHDR".into());
                }
                width = u32::from_be_bytes(data[0..4].try_into().unwrap()) as i32;
                height = u32::from_be_bytes(data[4..8].try_into().unwrap()) as i32;
                if width == 0 || height == 0 {
                    return Err("bogus IHDR".into());
                }
                bit_depth = data[8];
                color_type = data[9];
                interlace = data[12];
            }
            b"PLTE" => {
                let n = len / 3;
                let (mut r, mut gm, mut b) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
                for i in 0..n {
                    r.push(data[i * 3]);
                    gm.push(data[i * 3 + 1]);
                    b.push(data[i * 3 + 2]);
                }
                red_map = Some(r);
                green_map = gm;
                blue_map = b;
            }
            b"tRNS" => match color_type {
                3 | 7 => {
                    let alen = red_map.as_ref().map_or(len, |r| r.len());
                    let mut a = vec![0u8; alen];
                    let n = len.min(alen);
                    a[..n].copy_from_slice(&data[..n]);
                    for x in a.iter_mut().skip(len) {
                        *x = 0xff;
                    }
                    alpha_map = Some(a);
                }
                2 | 6 => {
                    if len != 6 {
                        return Err("Broken file".into());
                    }
                    if bit_depth == 16 {
                        transparent_pixel_16 = Some(data[..6].try_into().unwrap());
                    } else {
                        transparent_pixel = ((get_short(data, 0) & 0xff) << 16) | ((get_short(data, 2) & 0xff) << 8) | (get_short(data, 4) & 0xff);
                    }
                }
                0 | 4 => {
                    if len != 2 {
                        return Err("Broken file".into());
                    }
                    let t = get_short(data, 0);
                    let t = 0xff & if bit_depth == 16 { t >> 8 } else { t };
                    transparent_pixel = (t << 16) | (t << 8) | t;
                }
                _ => {}
            },
            b"gAMA" => {
                if len != 4 {
                    return Err("bogus gAMA".into());
                }
            }
            b"IDAT" => {
                seen_idat = true;
                idat.extend_from_slice(data);
            }
            b"IEND" => break,
            _ => {}
        }
    }
    if !seen_idat {
        return Err("no data".into());
    }
    let log_depth = match bit_depth {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        _ => return Err("invalid depth".into()),
    };
    let w = width as usize;
    let mut pix_size = w;
    let row_stride = if interlace != 0 {
        pix_size *= height as usize;
        w
    } else {
        0
    };
    let combined = color_type as i32 | ((bit_depth as i32) << 3);
    let index_model: Option<IndexColorModel>;
    let mut b_pixels: Option<Vec<u8>> = None;
    let mut w_pixels: Option<Vec<i32>> = None;
    match color_type {
        3 | 7 => {
            let Some(r) = &red_map else { return Err("palette expected".into()) };
            index_model = Some(IndexColorModel::from_rgb(bit_depth as i32, r.len(), r, &green_map, &blue_map, alpha_map.as_deref(), -1));
            b_pixels = Some(vec![0; pix_size]);
        }
        PNG_GRAY => {
            let llog = if log_depth >= 4 { 3 } else { log_depth };
            let m = GREY_MODELS.with(|gm| {
                let mut gm = gm.borrow_mut();
                if gm[llog].is_none() {
                    let size = 1usize << (1 << llog);
                    let ramp: Vec<u8> = (0..size).map(|i| (255 * i / (size - 1)) as u8).collect();
                    let trans = if transparent_pixel == -1 { -1 } else { transparent_pixel & 0xff };
                    gm[llog] = Some(IndexColorModel::from_rgb(bit_depth as i32, size, &ramp, &ramp, &ramp, None, trans));
                }
                gm[llog].clone().unwrap()
            });
            index_model = Some(m);
            b_pixels = Some(vec![0; pix_size]);
        }
        2 | 6 | 4 => {
            index_model = None;
            w_pixels = Some(vec![0; pix_size]);
        }
        _ => return Err("invalid color type".into()),
    }
    g.set_dimensions(width, height);
    let samples_per_pixel = if color_type & PNG_PALETTE != 0 {
        1
    } else {
        (if color_type & PNG_COLOR != 0 { 3 } else { 1 }) + (if color_type & PNG_ALPHA != 0 { 1 } else { 0 })
    };
    let bits_per_pixel = samples_per_pixel * bit_depth as usize;
    let bytes_per_pixel = (bits_per_pixel + 7) >> 3;
    let mut stream = std::io::BufReader::new(flate2::read::ZlibDecoder::new(&idat[..]));
    const STARTING_ROW: [usize; 8] = [0, 0, 0, 4, 0, 2, 0, 1];
    const STARTING_COL: [usize; 8] = [0, 0, 4, 0, 2, 0, 1, 0];
    const ROW_INCREMENT: [usize; 8] = [1, 8, 8, 8, 4, 4, 2, 2];
    const COL_INCREMENT: [usize; 8] = [1, 8, 8, 4, 4, 2, 2, 1];
    let (first_pass, last_pass) = if interlace == 0 { (0, 0) } else { (1, 7) };
    let send = |g: &mut Grabber, y: i32, h: i32, b_pixels: &Option<Vec<u8>>, w_pixels: &Option<Vec<i32>>| {
        if let Some(wp) = w_pixels {
            g.set_pixels(0, y, width, h, wp, 0, w);
        } else if let Some(bp) = b_pixels {
            let m = index_model.as_ref().unwrap();
            let conv: Vec<i32> = bp.iter().map(|&p| m.get_rgb(p as i32 & 0xff)).collect();
            g.set_pixels(0, y, width, h, &conv, 0, w);
        }
    };
    for pass in first_pass..=last_pass {
        let mut row = STARTING_ROW[pass];
        let row_inc = ROW_INCREMENT[pass];
        let col_inc = COL_INCREMENT[pass];
        let s_col = STARTING_COL[pass];
        let row_pixel_width = (w as isize - s_col as isize + (col_inc as isize - 1)) / col_inc as isize;
        let row_byte_width = ((row_pixel_width.max(0) as usize * bits_per_pixel) + 7) >> 3;
        if row_byte_width == 0 {
            continue;
        }
        let mut row_offset = row_stride * row;
        let mut first_row = true;
        let mut cur = vec![0u8; row_byte_width];
        let mut prev = vec![0u8; row_byte_width];
        while row < height as usize {
            let mut f = [0u8; 1];
            let row_filter = match stream.read(&mut f) {
                Ok(1) => f[0] as i32,
                _ => -1,
            };
            if stream.read_exact(&mut cur).is_err() {
                return Err("missing data".into());
            }
            filter_row(&mut cur, if first_row { None } else { Some(&prev) }, row_filter, row_byte_width, bytes_per_pixel)?;
            let mut col = s_col;
            let mut spos = 0usize;
            while col < w {
                if let Some(wp) = w_pixels.as_mut() {
                    let b = &cur;
                    let v = match combined {
                        c if c == (6 | (8 << 3)) => {
                            let p = ((b[spos] as i32) << 16) | ((b[spos + 1] as i32) << 8) | b[spos + 2] as i32 | ((b[spos + 3] as i32) << 24);
                            spos += 4;
                            p
                        }
                        c if c == (6 | (16 << 3)) => {
                            let p = ((b[spos] as i32) << 16) | ((b[spos + 2] as i32) << 8) | b[spos + 4] as i32 | ((b[spos + 6] as i32) << 24);
                            spos += 8;
                            p
                        }
                        c if c == (2 | (8 << 3)) => {
                            let mut p = ((b[spos] as i32) << 16) | ((b[spos + 1] as i32) << 8) | b[spos + 2] as i32;
                            if p != transparent_pixel {
                                p |= 0xff000000u32 as i32;
                            }
                            spos += 3;
                            p
                        }
                        c if c == (2 | (16 << 3)) => {
                            let mut p = ((b[spos] as i32) << 16) | ((b[spos + 2] as i32) << 8) | b[spos + 4] as i32;
                            let transparent = transparent_pixel_16.is_some_and(|t| (0..6).all(|i| b[spos + i] == t[i]));
                            if !transparent {
                                p |= 0xff000000u32 as i32;
                            }
                            spos += 6;
                            p
                        }
                        c if c == (4 | (8 << 3)) => {
                            let tx = b[spos] as i32;
                            spos += 2;
                            (tx << 16) | (tx << 8) | tx | ((b[spos - 1] as i32) << 24)
                        }
                        c if c == (4 | (16 << 3)) => {
                            let tx = b[spos] as i32;
                            let a = b[spos + 2] as i32;
                            spos += 4;
                            (tx << 16) | (tx << 8) | tx | (a << 24)
                        }
                        _ => return Err("illegal type/depth".into()),
                    };
                    wp[col + row_offset] = v;
                } else {
                    let bp = b_pixels.as_mut().unwrap();
                    let b = &cur;
                    let v = match bit_depth {
                        1 => {
                            let v = (b[spos >> 3] >> (7 - (spos & 7))) & 1;
                            spos += 1;
                            v
                        }
                        2 => {
                            let v = (b[spos >> 2] >> ((3 - (spos & 3)) * 2)) & 3;
                            spos += 1;
                            v
                        }
                        4 => {
                            let v = (b[spos >> 1] >> ((1 - (spos & 1)) * 4)) & 15;
                            spos += 1;
                            v
                        }
                        8 => {
                            spos += 1;
                            b[spos - 1]
                        }
                        16 => {
                            let v = b[spos];
                            spos += 2;
                            v
                        }
                        _ => return Err("illegal type/depth".into()),
                    };
                    bp[col + row_offset] = v;
                }
                col += col_inc;
            }
            if interlace == 0 {
                send(g, row as i32, 1, &b_pixels, &w_pixels);
            }
            row += row_inc;
            row_offset += row_inc * row_stride;
            std::mem::swap(&mut cur, &mut prev);
            first_row = false;
        }
        if interlace != 0 {
            send(g, 0, height, &b_pixels, &w_pixels);
        }
    }
    Ok(())
}

fn crc32fast_hash(b: &[u8]) -> u32 {
    let mut c: u32 = 0xffff_ffff;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    c ^ 0xffff_ffff
}

fn filter_row(row: &mut [u8], prev: Option<&[u8]>, filter: i32, width: usize, bpp: usize) -> Result<(), String> {
    match filter {
        0 => {}
        1 => {
            for x in bpp..width {
                row[x] = row[x].wrapping_add(row[x - bpp]);
            }
        }
        2 => {
            if let Some(p) = prev {
                for x in 0..width {
                    row[x] = row[x].wrapping_add(p[x]);
                }
            }
        }
        3 => {
            if let Some(p) = prev {
                for x in 0..bpp.min(width) {
                    row[x] = row[x].wrapping_add(p[x] >> 1);
                }
                for x in bpp..width {
                    row[x] = row[x].wrapping_add(((p[x] as u32 + row[x - bpp] as u32) >> 1) as u8);
                }
            } else {
                for x in bpp..width {
                    row[x] = row[x].wrapping_add(row[x - bpp] >> 1);
                }
            }
        }
        4 => {
            if let Some(p) = prev {
                for x in 0..bpp.min(width) {
                    row[x] = row[x].wrapping_add(p[x]);
                }
                for x in bpp..width {
                    let a = row[x - bpp] as i32;
                    let b = p[x] as i32;
                    let c = p[x - bpp] as i32;
                    let pp = a + b - c;
                    let pa = (pp - a).abs();
                    let pb = (pp - b).abs();
                    let pc = (pp - c).abs();
                    let v = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    row[x] = row[x].wrapping_add(v as u8);
                }
            } else {
                for x in bpp..width {
                    row[x] = row[x].wrapping_add(row[x - bpp]);
                }
            }
        }
        _ => return Err("Illegal filter".into()),
    }
    Ok(())
}

/// `GifImageDecoder.produceImage` up to the first frame, which is all a `PixelGrabber` waits for.
fn decode_gif(b: &[u8], g: &mut Grabber) -> Result<(), String> {
    if b.len() < 13 {
        return Err("short".into());
    }
    let word = |p: usize| -> i32 { b[p] as i32 | ((b[p + 1] as i32) << 8) };
    let mut global_width = word(6);
    let mut global_height = word(8);
    let ch = b[10];
    let mut pos = 13;
    let (mut num_global_colors, mut global_colormap) = if ch & 0x80 == 0 {
        (2usize, vec![0u8, 0, 0, 255, 255, 255])
    } else {
        let n = 1usize << ((ch & 7) + 1);
        let cm = b.get(pos..pos + n * 3).ok_or("short")?.to_vec();
        pos += n * 3;
        (n, cm)
    };
    let mut trans_pixel: i32 = -1;
    loop {
        let code = *b.get(pos).ok_or("eof")?;
        pos += 1;
        match code {
            0x21 => {
                let ext = *b.get(pos).ok_or("eof")?;
                pos += 1;
                if ext == 0xf9 {
                    let buf = b.get(pos..pos + 6).ok_or("eof")?;
                    pos += 6;
                    if buf[0] != 4 || buf[5] != 0 {
                        return Err("corrupt GIF file (GCE size)".into());
                    }
                    trans_pixel = if buf[1] & 1 != 0 { buf[4] as i32 } else { -1 };
                } else {
                    loop {
                        let n = *b.get(pos).ok_or("eof")? as usize;
                        pos += 1;
                        if n == 0 {
                            break;
                        }
                        pos += n;
                    }
                }
            }
            0x2c => {
                let block = b.get(pos..pos + 10).ok_or("eof")?;
                pos += 10;
                let x = block[0] as i32 | ((block[1] as i32) << 8);
                let y = block[2] as i32 | ((block[3] as i32) << 8);
                let width = block[4] as i32 | ((block[5] as i32) << 8);
                let height = block[6] as i32 | ((block[7] as i32) << 8);
                let interlace = block[8] & 0x40 != 0;
                let mut init_code_size = block[9] as i32;
                let model = if block[8] & 0x80 != 0 {
                    let mut num_local = 1usize << ((block[8] & 7) + 1);
                    let mut cm = vec![0u8; num_local * 3];
                    cm[0] = block[9];
                    cm[1..].copy_from_slice(b.get(pos..pos + num_local * 3 - 1).ok_or("eof")?);
                    pos += num_local * 3 - 1;
                    init_code_size = *b.get(pos).ok_or("eof")? as i32;
                    pos += 1;
                    if trans_pixel >= num_local as i32 {
                        num_local = trans_pixel as usize + 1;
                        cm.resize(num_local * 3, 0);
                    }
                    IndexColorModel::from_cmap(8, num_local, &cm, trans_pixel)
                } else {
                    if trans_pixel >= num_global_colors as i32 {
                        num_global_colors = trans_pixel as usize + 1;
                        global_colormap.resize(num_global_colors * 3, 0);
                    }
                    IndexColorModel::from_cmap(8, num_global_colors, &global_colormap, trans_pixel)
                };
                if global_width == 0 {
                    global_width = width;
                }
                if global_height == 0 {
                    global_height = height;
                }
                g.set_dimensions(global_width, global_height);
                if init_code_size >= 12 {
                    return Err("Invalid initial code size".into());
                }
                let mut data = Vec::new();
                loop {
                    let n = *b.get(pos).ok_or("eof")? as usize;
                    pos += 1;
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(b.get(pos..pos + n).ok_or("eof")?);
                    pos += n;
                }
                let indices = lzw_decode(&data, init_code_size, (width as usize) * (height as usize));
                let rows_in_order: Vec<i32> = if interlace {
                    let mut v = Vec::new();
                    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                        let mut r = start;
                        while r < height {
                            v.push(r);
                            r += step;
                        }
                    }
                    v
                } else {
                    (0..height).collect()
                };
                for (i, &r) in rows_in_order.iter().enumerate() {
                    let start = i * width as usize;
                    if start >= indices.len() {
                        break;
                    }
                    let line: Vec<i32> = (0..width as usize).map(|c| model.get_rgb(*indices.get(start + c).unwrap_or(&0) as i32)).collect();
                    let (mut sx, mut sw, mut off) = (x, width, 0usize);
                    if sx < 0 {
                        off = (-sx) as usize;
                        sw += sx;
                        sx = 0;
                    }
                    if sx + sw > global_width {
                        sw = global_width - sx;
                    }
                    let sy = y + r;
                    if sw > 0 && sy >= 0 && sy < global_height {
                        g.set_pixels(sx, sy, sw, 1, &line, off, width as usize);
                    }
                }
                return Ok(());
            }
            _ => return Err("corrupt GIF file".into()),
        }
    }
}

/// The LZW loop of the native `parseImage`; a code stream that ends early leaves the rest of the frame unset.
fn lzw_decode(data: &[u8], init_code_size: i32, npix: usize) -> Vec<u8> {
    let clear = 1i32 << init_code_size;
    let eoi = clear + 1;
    let mut code_size = init_code_size + 1;
    let mut code_mask = (1i32 << code_size) - 1;
    let mut avail = clear + 2;
    let mut old_code: i32 = -1;
    let mut prefix = [0i32; 4096];
    let mut suffix = [0u8; 4096];
    for i in 0..clear.min(4096) {
        suffix[i as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(npix);
    let mut first: u8 = 0;
    let (mut bits, mut acc, mut p) = (0i32, 0u32, 0usize);
    let mut stack: Vec<u8> = Vec::with_capacity(4097);
    while out.len() < npix {
        while bits < code_size {
            if p >= data.len() {
                return out;
            }
            acc |= (data[p] as u32) << bits;
            p += 1;
            bits += 8;
        }
        let code = (acc as i32) & code_mask;
        acc >>= code_size;
        bits -= code_size;
        if code == clear {
            code_size = init_code_size + 1;
            code_mask = (1 << code_size) - 1;
            avail = clear + 2;
            old_code = -1;
            continue;
        }
        if code == eoi {
            break;
        }
        if old_code == -1 {
            first = suffix[code as usize];
            out.push(first);
            old_code = code;
            continue;
        }
        let in_code = code;
        let mut c = code;
        stack.clear();
        if c >= avail {
            stack.push(first);
            c = old_code;
        }
        while c > clear {
            stack.push(suffix[c as usize]);
            c = prefix[c as usize];
        }
        first = suffix[c as usize];
        stack.push(first);
        if avail < 4096 {
            prefix[avail as usize] = old_code;
            suffix[avail as usize] = first;
            avail += 1;
            if (avail & code_mask) == 0 && avail < 4096 {
                code_size += 1;
                code_mask += avail;
            }
        }
        old_code = in_code;
        while let Some(v) = stack.pop() {
            out.push(v);
        }
    }
    out.truncate(npix);
    out
}

/// `JPEGImage`: the bytes, and the size the AWT decoder reported.
pub struct JpegImage {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

impl JpegImage {
    pub fn new(location: &str, data: Vec<u8>) -> Result<JpegImage, String> {
        let g = grab(&data, location)?;
        Ok(JpegImage { width: g.width, height: g.height, data })
    }
}

/// `LosslessImage`: the default-RGB pixels of a GIF or PNG.
pub struct LosslessImage {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<i32>,
}

impl LosslessImage {
    pub fn new(location: &str, bytes: &[u8]) -> Result<LosslessImage, String> {
        let g = grab(bytes, location)?;
        Ok(LosslessImage { width: g.width, height: g.height, pixels: g.pixels })
    }
}
