//! The `media` library items (bitmaps, sounds, videos) with their `bin/*.dat` data, sound and
//! video instances on frames, and `PublishSettings.xml`.

use std::io::Write as _;

use super::convert::Converter;
use super::model::{SoundInfo, StreamRange};
use super::num::java_double;
use super::scripts::Pack;
use super::shapes::convert_matrix;
use super::xml::XmlWriter;
use crate::export::image::{Bitmap, Format};
use crate::export::sound::{format as sf, Sound};
use crate::io::Reader;
use crate::shape::{Fill, Record, ShapeTag};
use crate::tags::{code::*, Tag};

const RATE_NAMES: [u32; 4] = [5, 11, 22, 44];

/// Bitrate in kbit/s of the first MPEG audio frame, found the way JLayer syncs to it.
pub fn mp3_bitrate(data: &[u8]) -> Option<u32> {
    const V1: [[u32; 16]; 3] = [
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0],
    ];
    const V2: [[u32; 16]; 3] = [
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
    ];
    for i in 0..data.len().saturating_sub(3) {
        if data[i] != 0xff || data[i + 1] & 0xe0 != 0xe0 {
            continue;
        }
        let version = (data[i + 1] >> 3) & 3;
        let layer = (data[i + 1] >> 1) & 3;
        let br = (data[i + 2] >> 4) as usize;
        let sr = (data[i + 2] >> 2) & 3;
        if version == 1 || layer == 0 || br == 15 || sr == 3 {
            continue;
        }
        let l = 3 - layer as usize;
        return Some(if version == 3 { V1[l][br] } else { V2[l][br] });
    }
    None
}

fn bitrate_bits(kbps: u32) -> (i32, bool) {
    match kbps {
        8 => (6, false),
        16 => (7, false),
        20 => (8, false),
        24 => (9, false),
        32 => (10, false),
        48 => (11, false),
        56 => (12, false),
        64 => (13, false),
        80 => (14, false),
        112 => (15, false),
        128 => (16, false),
        160 => (17, false),
        _ => (17, true),
    }
}

/// `SoundTag.getExportFormat` of a DefineSound or stream head.
pub fn export_ext(format: u8, latency: i32, first_data: &[u8]) -> &'static str {
    match format {
        sf::MP3 => {
            if latency > 0 || mp3_bitrate(first_data).is_some_and(|b| b > 160) {
                "wav"
            } else {
                "mp3"
            }
        }
        sf::ADPCM | sf::PCM_LE | sf::PCM_NATIVE | sf::NELLY | sf::NELLY_16K | sf::NELLY_8K => "wav",
        _ => "flv",
    }
}

fn define_sound_parts(tag: &Tag) -> Option<(u8, u8, bool, bool, u32, &[u8])> {
    let mut r = Reader::new(&tag.body);
    r.u16().ok()?;
    let format = r.ub(4).ok()? as u8;
    let rate = r.ub(2).ok()? as u8;
    let size = r.bit().ok()?;
    let stereo = r.bit().ok()?;
    let count = r.u32().ok()?;
    Some((format, rate, size, stereo, count, r.rest()))
}

pub fn define_sound_ext(tag: &Tag) -> &'static str {
    match define_sound_parts(tag) {
        Some((f, _, _, _, _, data)) => {
            let latency = if f == sf::MP3 && data.len() >= 2 { i16::from_le_bytes([data[0], data[1]]) as i32 } else { 0 };
            export_ext(f, latency, data)
        }
        None => "flv",
    }
}

struct Head {
    format: u8,
    rate: u8,
    size: bool,
    stereo: bool,
    latency: i32,
}

fn read_head(t: &Tag) -> Option<Head> {
    let mut r = Reader::new(&t.body);
    r.u8().ok()?;
    let format = r.ub(4).ok()? as u8;
    let rate = r.ub(2).ok()? as u8;
    let size = r.bit().ok()?;
    let stereo = r.bit().ok()?;
    r.u16().ok()?;
    let latency = if format == sf::MP3 { r.i16().map(|v| v as i32).unwrap_or(0) } else { 0 };
    Some(Head { format, rate, size, stereo, latency })
}

/// ImageBinDataGenerator: JPEG bytes plus the frame, or deflated premultiplied ARGB rows.
fn image_bin(bytes: &[u8], format: Format, img: &image::RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    if format == Format::Jpeg {
        out.extend_from_slice(bytes);
        for v in [0, 20 * w, 0, 20 * h] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        return out;
    }
    out.extend_from_slice(&[0x03, 0x05]);
    out.extend_from_slice(&((4 * w) as u16).to_le_bytes());
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    for v in [0, 20 * w, 0, 20 * h] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0x01, 0x01]);
    let mut raw = Vec::with_capacity((w * h * 4) as usize);
    for p in img.pixels() {
        let [mut r, mut g, mut b, mut a] = p.0.map(|c| c as u32);
        if a != 255 {
            r = (r as f32 * a as f32 / 256.0).floor() as u32;
            g = (g as f32 * a as f32 / 256.0).floor() as u32;
            b = (b as f32 * a as f32 / 256.0).floor() as u32;
        }
        if a != 0 && a != 255 {
            a += 1;
        }
        raw.extend_from_slice(&[a as u8, r as u8, g as u8, b as u8]);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(1));
    let _ = z.write_all(&raw);
    let data = z.finish().unwrap_or_default();
    for chunk in data.chunks(2048) {
        out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

const EMPTY_MOVIE_BIN: [u8; 101] = [
    0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xA0, 0, 0, 0, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0, 0, 0x01, 0x01, 0, 0,
    0, 0, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x59, 0x40, 0x18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFE, 0xFF, 0, 0, 0, 0, 0,
];

/// MovieBinDataGenerator over the FLV this crate writes (video only, one metadata tag first).
fn movie_bin(flv: &[u8], fps: f32) -> Vec<u8> {
    let mut tags: Vec<(u8, &[u8])> = Vec::new();
    let mut p = 13;
    while p + 11 <= flv.len() {
        let kind = flv[p];
        let size = u32::from_be_bytes([0, flv[p + 1], flv[p + 2], flv[p + 3]]) as usize;
        let Some(data) = flv.get(p + 11..p + 11 + size) else { break };
        tags.push((kind, data));
        p += 11 + size + 4;
    }
    let videos: Vec<&[u8]> = tags.iter().filter(|(k, d)| *k == crate::export::flv::VIDEO && !d.is_empty()).map(|(_, d)| *d).collect();
    let codec = videos.last().map_or(0, |d| d[0] & 0x0f);
    let (mut width, mut height) = (0u32, 0u32);
    let mut out = Vec::new();
    let mut last_offset: u32 = 0;
    let mut index = 0;
    for (k, d) in &tags {
        if *k == crate::export::flv::SCRIPT {
            if let Some(w) = flv_meta(d, "width") {
                width = w as u32;
            }
            if let Some(h) = flv_meta(d, "height") {
                height = h as u32;
            }
            out.extend_from_slice(&[0x03, codec, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x28, 0x40, 0, 0, 0, 0]);
            out.extend_from_slice(&[0, 0, 0, 0, 0]);
            out.extend_from_slice(&[0, 0, 0]);
            out.push(0);
            out.extend_from_slice(&[0, 0, 0]);
            out.extend_from_slice(&width.to_le_bytes());
            out.extend_from_slice(&height.to_le_bytes());
            out.extend_from_slice(&(videos.len() as f64 / fps as f64).to_bits().to_le_bytes());
            out.extend_from_slice(&[0, 0, 0, 0, 0x01, 0x01, 0, 0, 0, 0, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x59, 0x40, 0x18, 0, 0, 0, 0, 0, 0, 0, 0x01]);
        }
        if *k == crate::export::flv::VIDEO && !d.is_empty() {
            let len = d.len() as u32 - 1;
            out.extend_from_slice(&last_offset.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            last_offset += len;
            out.extend_from_slice(&[0x01, 0, 0, 0, 0x04 + (d[0] >> 4 == 1) as u8, 0, 0, 0]);
            out.push((index + 1 < videos.len()) as u8);
            index += 1;
        }
    }
    out.extend_from_slice(&last_offset.to_le_bytes());
    for d in &videos {
        out.extend_from_slice(&d[1..]);
    }
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(0);
    out.extend_from_slice(&[0, 0, 0, 0, 0xFF, 0xFE, 0xFF, 0, 0, 0, 0, 0]);
    out
}

/// A number in an FLV `onMetaData` ECMA array.
fn flv_meta(d: &[u8], key: &str) -> Option<f64> {
    let k = key.as_bytes();
    let pos = d.windows(k.len() + 2).position(|w| w[..2] == (k.len() as u16).to_be_bytes() && &w[2..] == k)?;
    let v = d.get(pos + 2 + k.len()..pos + 2 + k.len() + 9)?;
    (v[0] == 0).then(|| f64::from_be_bytes(v[1..9].try_into().unwrap()))
}

impl<'a> Converter<'a> {
    fn dat_name(&self, files: usize) -> String {
        format!("M {} {}.dat", files + 1, self.timestamp)
    }

    /// Whether a root shape fills with the bitmap smoothed (the first fill using it decides).
    fn bitmap_smoothing(&self, id: u16) -> bool {
        for t in &self.swf.timeline.tags {
            if !crate::shape::is_shape(t.code) {
                continue;
            }
            let Ok(s) = ShapeTag::read(t) else { continue };
            let mut fills: Vec<&Fill> = s.shape.styles.fills.iter().collect();
            for r in &s.shape.records {
                if let Record::StyleChange { new_styles: Some(ns), .. } = r {
                    fills.extend(ns.fills.iter());
                }
            }
            for f in fills {
                if let Fill::Bitmap { id: b, smooth, .. } = f {
                    if *b == id {
                        return *smooth;
                    }
                }
            }
        }
        false
    }

    fn convert_image_media(&mut self, id: u16, tag: &Tag, w: &mut XmlWriter) {
        let jt = self.swf.timeline.tags.iter().find(|t| t.code == JPEG_TABLES).map(|t| &t.body[..]);
        let Ok(bmp) = Bitmap::read(tag, jt) else { return };
        let Ok((format, bytes)) = bmp.encode(false) else { return };
        let Ok(img) = bmp.pixels() else { return };
        let smoothing = self.bitmap_smoothing(id);
        let file = format!("{}.{}", self.symbol_name(id, "Bitmap"), format.ext());
        let item_id = self.item_id();
        w.start_with("DOMBitmapItem", &[("name", &file), ("itemID", &item_id), ("sourceLastImported", &self.timestamp.to_string()), ("externalFileSize", &bytes.len().to_string())]);
        if smoothing {
            w.attr_bool("allowSmoothing", true);
        }
        match format {
            Format::Png | Format::Gif => {
                if bmp.original.as_ref().is_none_or(|(f, _)| *f != Format::Jpeg) {
                    w.attr_bool("useImportedJPEGData", false);
                    w.attr("compressionType", "lossless");
                }
                w.attr("originalCompressionType", "lossless");
            }
            Format::Jpeg => w.attr_bool("isJPEG", true),
            Format::Jxr => {}
        }
        let mut export = false;
        if let Some(cl) = self.classes.get(&id).cloned() {
            if self.ex.as3_extends(&cl, "flash.display.BitmapData") {
                export = true;
                w.attr("linkageClassName", &cl);
            }
        }
        if let Some(v) = self.variables.get(&id) {
            export = true;
            w.attr("linkageIdentifier", v);
        }
        if export {
            if !self.first_frame.contains(&id) {
                w.attr("linkageExportInFirstFrame", "false");
            }
            w.attr_bool("linkageExportForAS", true);
        }
        w.attr_i("quality", 50);
        w.attr("href", &file);
        let dat = self.dat_name(self.files.dat.len());
        w.attr("bitmapDataHRef", &dat);
        w.attr_i("frameRight", (img.width() * 20) as i64);
        w.attr_i("frameBottom", (img.height() * 20) as i64);
        w.end();
        self.files.dat.push((dat, image_bin(&bytes, format, &img)));
        self.files.library.insert(file, bytes);
    }

    /// `convertSoundMedia` for a DefineSound (`range` None) or a stream range.
    fn convert_sound_media(&mut self, tag: Option<&Tag>, range: Option<(&StreamRange, &'a [Tag])>, w: &mut XmlWriter) {
        let (format, rate, size, stereo, mut sample_count, first, sound, name) = if let Some((r, tags)) = range {
            let Some(h) = read_head(&tags[r.head]) else { return };
            let first: &[u8] = tags[r.head..].iter().find(|t| t.code == SOUND_STREAM_BLOCK).map_or(&[][..], |t| &t.body[..]);
            let chunks: Vec<&[u8]> = r.blocks.iter().map(|&b| &tags[b].body[..]).map(|d| if h.format == sf::MP3 { d.get(4..).unwrap_or(&[]) } else { d }).collect();
            let sound = Sound { format: h.format, rate_index: h.rate, sixteen_bit: h.size, stereo: h.stereo, chunks, latency: 0, stream: true, frame_rate: self.swf.frame_rate };
            (h.format, h.rate, h.size, h.stereo, 0u64, first, sound, r.fla_name())
        } else {
            let tag = tag.unwrap();
            let Some((f, rate, size, stereo, count, data)) = define_sound_parts(tag) else { return };
            let Ok(sound) = Sound::from_define_sound(tag) else { return };
            (f, rate, size, stereo, count as u64, data, sound, format!("sound{}", tag.character_id().unwrap_or(0)))
        };
        let mut ext = "wav";
        let mut fmt = 0;
        let mut bits = 0;
        if matches!(format, sf::ADPCM | sf::PCM_LE | sf::PCM_NATIVE) {
            if stereo {
                fmt += 1;
            }
            fmt += [2, 6, 10, 14][rate as usize & 3];
        }
        if format == sf::SPEEX {
            bits = 18;
        }
        if format == sf::ADPCM {
            if let Ok(c) = Reader::new(first).ub(2) {
                bits = 2 + c as i32;
            }
        }
        let mut seek = 0usize;
        let mut to_wav = false;
        if format == sf::MP3 {
            ext = "mp3";
            if !stereo {
                fmt += 1;
            }
            fmt += 4;
            if first.len() >= 2 {
                let s = i16::from_le_bytes([first[0], first[1]]);
                if s > 0 {
                    seek = s as usize;
                    ext = "wav";
                    to_wav = true;
                }
            }
            if let Some(kbps) = mp3_bitrate(first.get(2..).unwrap_or(&[])) {
                let (b, wav) = bitrate_bits(kbps);
                bits = b;
                if wav {
                    ext = "wav";
                    to_wav = true;
                }
            }
        }
        let data = if to_wav || format != sf::MP3 {
            let mut s2 = Sound { chunks: sound.chunks.clone(), latency: if range.is_none() { seek as u32 } else { 0 }, ..sound };
            s2.stream = sound.stream;
            if format == sf::MP3 || matches!(format, sf::ADPCM | sf::PCM_LE | sf::PCM_NATIVE | sf::NELLY | sf::NELLY_16K | sf::NELLY_8K) {
                s2.export(crate::export::sound::Kind::Wav).unwrap_or_default()
            } else {
                s2.export(crate::export::sound::Kind::Flv).unwrap_or_default()
            }
        } else {
            sound.export(crate::export::sound::Kind::Mp3).unwrap_or_default()
        };
        let mut dat = None;
        if let Ok((pcm, st)) = sound.decode() {
            let ch = if st { 2 } else { 1 };
            let skip = (seek * ch).min(pcm.len());
            let pcm = if seek > 0 { &pcm[skip..] } else { &pcm[..] };
            if sample_count == 0 {
                sample_count = (pcm.len() / ch) as u64;
            }
            let name = self.dat_name(self.files.dat.len());
            let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
            self.files.dat.push((name.clone(), bytes));
            dat = Some(name);
        }
        let file = format!("{name}.{ext}");
        let item_id = self.item_id();
        w.start_with("DOMSoundItem", &[("name", &file), ("itemID", &item_id), ("sourceLastImported", &self.timestamp.to_string()), ("externalFileSize", &data.len().to_string())]);
        w.attr("href", &file);
        if let Some(d) = &dat {
            w.attr("soundDataHRef", d);
        }
        w.attr("format", &format!("{}kHz {} {}", RATE_NAMES[rate as usize & 3], if size { "16bit" } else { "8bit" }, if stereo { "Stereo" } else { "Mono" }));
        w.attr_i("exportFormat", fmt);
        w.attr_i("exportBits", bits as i64);
        w.attr_i("sampleCount", sample_count as i64);
        self.files.library.insert(file, data);
    }

    pub fn convert_media(&mut self, w: &mut XmlWriter) {
        let mut started = false;
        let start = |w: &mut XmlWriter, s: &mut bool| {
            if !*s {
                w.start("media");
                *s = true;
            }
        };
        for id in self.characters.clone() {
            let Some(tag) = self.char_tag(id) else { continue };
            if crate::export::image::is_image(tag.code) {
                start(w, &mut started);
                self.convert_image_media(id, tag, w);
            } else if tag.code == DEFINE_SOUND {
                start(w, &mut started);
                self.convert_sound_media(Some(tag), None, w);
                self.write_linkage(w, id);
                w.end();
            } else if tag.code == DEFINE_VIDEO_STREAM {
                start(w, &mut started);
                self.convert_video_media(id, tag, w);
            }
        }
        let root: &'a [Tag] = &self.swf.timeline.tags;
        for t in root {
            if t.code == SOUND_STREAM_HEAD || t.code == SOUND_STREAM_HEAD2 {
                start(w, &mut started);
                for r in self.ranges_of(-1, t) {
                    self.convert_sound_media(None, Some((&r, root)), w);
                    w.end();
                }
            }
            if t.code == DEFINE_SPRITE {
                let id = t.character_id().map_or(-1, |i| i as i32);
                let sub: &'a [Tag] = &t.sprite.as_ref().unwrap().tags;
                if let Some(h) = sub.iter().find(|s| s.code == SOUND_STREAM_HEAD || s.code == SOUND_STREAM_HEAD2) {
                    start(w, &mut started);
                    for r in self.ranges_of(id, h) {
                        self.convert_sound_media(None, Some((&r, sub)), w);
                        w.end();
                    }
                }
            }
        }
        if started {
            w.end();
        }
    }

    /// `head.getRanges()`: the ranges of the container's stream, when `head` is its head.
    pub fn ranges_of(&self, container: i32, head: &Tag) -> Vec<StreamRange> {
        let (Some((hi, ranges)), Some(tags)) = (self.ranges.get(&container), self.range_tags.get(&container)) else { return Vec::new() };
        if std::ptr::eq(&tags[*hi], head) {
            ranges.clone()
        } else {
            Vec::new()
        }
    }

    pub fn head_ext(&self, container: i32, head: &Tag) -> &'static str {
        let Some(h) = read_head(head) else { return "flv" };
        let ranges = self.ranges_of(container, head);
        let tags = self.range_tags.get(&container);
        let first = match (ranges.first().and_then(|r| r.blocks.first()), tags) {
            (Some(&b), Some(t)) => t[b].body.get(2..).unwrap_or(&[]),
            _ => &[][..],
        };
        export_ext(h.format, h.latency, first)
    }

    pub fn stream_ext(&self, r: &StreamRange) -> &'static str {
        match self.range_tags.get(&r.container) {
            Some(tags) => self.head_ext(r.container, &tags[r.head]),
            None => "flv",
        }
    }

    fn convert_video_media(&mut self, id: u16, tag: &Tag, w: &mut XmlWriter) {
        let mut r = Reader::new(&tag.body);
        let _ = r.u16();
        let num_frames = r.u16().unwrap_or(0);
        let width = r.u16().unwrap_or(0);
        let height = r.u16().unwrap_or(0);
        let _ = r.u8();
        let codec = r.u8().unwrap_or(0);
        let video_type = match codec {
            2 => "h263 media",
            3 => "screen share media",
            4 => "vp6 media",
            5 => "vp6 alpha media",
            _ => "no media",
        };
        let has_frames = {
            let mut found = false;
            self.swf.walk(|_, t| {
                if t.code == VIDEO_FRAME && t.body.len() >= 2 && u16::from_le_bytes([t.body[0], t.body[1]]) == id {
                    found = true;
                }
            });
            found
        };
        let data = if has_frames { crate::export::video::export(self.swf, tag).unwrap_or_default() } else { Vec::new() };
        let file = format!("movie{id}.flv");
        let ts = self.timestamp.to_string();
        if data.is_empty() {
            let dat = self.dat_name(self.files.dat.len());
            w.start_with("DOMVideoItem", &[("name", &file), ("sourceExternalFilepath", &format!("./LIBRARY/{file}")), ("sourceLastImported", &ts), ("videoDataHRef", &dat), ("channels", "0"), ("isSpecial", "true")]);
            self.write_linkage(w, id);
            self.files.dat.push((dat, EMPTY_MOVIE_BIN.to_vec()));
            w.end();
            return;
        }
        let item_id = self.item_id();
        w.start_with("DOMVideoItem", &[("name", &file), ("itemID", &item_id), ("sourceLastImported", &ts), ("externalFileSize", &data.len().to_string())]);
        w.attr("href", &file);
        w.attr("videoType", video_type);
        w.attr_i("fps", self.swf.frame_rate as i64);
        w.attr_i("width", width as i64);
        w.attr_i("height", height as i64);
        w.attr("length", &java_double(num_frames as f64 / self.swf.frame_rate as f64));
        self.write_linkage(w, id);
        let dat = self.dat_name(self.files.dat.len());
        w.attr("videoDataHRef", &dat);
        self.files.dat.push((dat, movie_bin(&data, self.swf.frame_rate)));
        w.end();
        self.files.library.insert(file, data);
    }

    /// `convertSoundUsage`: the frame attributes of a StartSound.
    pub fn convert_sound_usage(&self, w: &mut XmlWriter, sound: &Tag, info: &SoundInfo) {
        let Some((_, rate, _, _, count, _)) = define_sound_parts(sound) else { return };
        w.attr("soundName", &format!("sound{}.{}", sound.character_id().unwrap_or(0), define_sound_ext(sound)));
        if let Some(p) = info.in_point {
            w.attr_i("inPoint44", p as i64);
        }
        if let Some(p) = info.out_point {
            w.attr_i("outPoint44", p as i64);
        }
        if let Some(l) = info.loops {
            if l == 32767 {
                w.attr("soundLoopMode", "loop");
            }
            w.attr_i("soundLoop", l as i64);
        }
        if info.sync_stop {
            w.attr("soundSync", "stop");
        } else if info.sync_no_multiple {
            w.attr("soundSync", "start");
        }
        let Some(env) = &info.envelope else {
            w.start("SoundEnvelope");
            w.empty_with("SoundEnvelopePoint", &[("level0", "32768"), ("level1", "32768")]);
            w.end();
            return;
        };
        let len = count as i64 * [8, 4, 2, 1][rate as usize & 3];
        let p = |i: usize| env.get(i).map(|e| (e.0 as i64, e.1 as i64, e.2 as i64));
        let effect = match (env.len(), p(0), p(1)) {
            (1, Some((0, 32768, 0)), _) => "left channel",
            (1, Some((0, 0, 32768)), _) => "right channel",
            (2, Some((0, 32768, 0)), Some((e, 0, 32768))) if e == len => "fade left to right",
            (2, Some((0, 0, 32768)), Some((e, 32768, 0))) if e == len => "fade right to left",
            (2, Some((0, 0, 0)), Some((e, 32768, 0))) if e == len / 4 => "fade in",
            (2, Some((s, 32768, 32768)), Some((e, 0, 0))) if s == len * 3 / 4 && e == len => "fade out",
            _ => "custom",
        };
        w.attr("soundEffect", effect);
        w.start("SoundEnvelope");
        for (pos, l, r) in env {
            w.empty_with("SoundEnvelopePoint", &[("mark44", &pos.to_string()), ("level0", &l.to_string()), ("level1", &r.to_string())]);
        }
        w.end();
    }

    pub fn convert_video_instance(&self, name: Option<&str>, m: &super::model::JMatrix, video: &Tag, w: &mut XmlWriter) {
        let mut r = Reader::new(&video.body);
        let id = r.u16().unwrap_or(0);
        let _ = r.u16();
        let width = r.u16().unwrap_or(0) as i64;
        let height = r.u16().unwrap_or(0) as i64;
        w.start_with("DOMVideoInstance", &[("libraryItemName", &format!("movie{id}.flv")), ("frameRight", &(20 * width).to_string()), ("frameBottom", &(20 * height).to_string())]);
        if let Some(n) = name {
            w.attr("name", n);
        }
        w.start("matrix");
        convert_matrix(m, w);
        w.end();
        w.start("transformationPoint");
        w.empty("Point");
        w.end();
        w.end();
    }

    pub fn convert_image_instance(&mut self, name: Option<&str>, m: &super::model::JMatrix, id: u16, w: &mut XmlWriter) {
        let item = self.bitmap_path(id).unwrap_or_default();
        w.start_with("DOMBitmapInstance", &[("libraryItemName", &item)]);
        if let Some(n) = name {
            w.attr("name", n);
        }
        w.start("matrix");
        convert_matrix(m, w);
        w.end();
        w.start("transformationPoint");
        w.empty("Point");
        w.end();
        w.end();
    }

    /// `convertSoundLayer`.
    pub fn convert_sound_layer(&mut self, scene_start: i32, items: &[super::convert::Item<'a>], w: &mut XmlWriter) {
        let mut index = 0;
        let mut starts: Vec<(&'a Tag, SoundInfo, i32)> = Vec::new();
        let mut ranges: Vec<StreamRange> = Vec::new();
        let mut frame = 0;
        for it in items {
            let Some(t) = it.tag() else {
                if it.code() == SHOW_FRAME {
                    frame += 1;
                }
                continue;
            };
            if t.code == START_SOUND {
                let mut r = Reader::new(&t.body);
                let id = r.u16().unwrap_or(0);
                let Some(s) = self.char_tag(id).filter(|s| s.code == DEFINE_SOUND) else { continue };
                let file = format!("sound{id}.{}", define_sound_ext(s));
                let info = SoundInfo::read(&mut r).unwrap_or_default();
                if self.files.library.contains_key(&file) {
                    starts.push((s, info, frame));
                }
            } else if t.code == SOUND_STREAM_HEAD || t.code == SOUND_STREAM_HEAD2 {
                let container = self.container_of(t);
                let ext = self.head_ext(container, t);
                for r in self.ranges_of(container, t) {
                    if self.files.library.contains_key(&format!("{}.{ext}", r.fla_name())) {
                        ranges.push(r);
                    }
                }
            } else if t.code == SHOW_FRAME {
                frame += 1;
            }
        }
        for r in &ranges {
            if r.start < scene_start || r.start >= scene_start + frame {
                continue;
            }
            let color = self.random_outline();
            w.start_with("DOMLayer", &[("name", &format!("Sound Layer {index}")), ("color", &color)]);
            index += 1;
            w.start("frames");
            let start = r.start - scene_start;
            if start != 0 {
                self.convert_frame(false, None, None, 0, start, "", "", w, None);
            }
            self.convert_frame(false, Some(r), None, start, frame - start, "", "", w, None);
            w.end();
            w.end();
        }
        for (s, info, start) in &starts {
            let color = self.random_outline();
            w.start_with("DOMLayer", &[("name", &format!("Sound Layer {index}")), ("color", &color)]);
            index += 1;
            w.start("frames");
            if *start != 0 {
                self.convert_frame(false, None, None, 0, *start, "", "", w, None);
            }
            self.convert_frame(false, None, Some((s, info)), *start, frame - start, "", "", w, None);
            w.end();
            w.end();
        }
    }

    /// The container id whose tag list holds `t` (-1 for the root timeline).
    fn container_of(&self, t: &Tag) -> i32 {
        for (&c, tags) in &self.range_tags {
            if tags.as_ptr_range().contains(&(t as *const Tag)) {
                return c;
            }
        }
        -1
    }
}

fn first_stream_head(tags: &[Tag]) -> Option<(&Tag, &[Tag])> {
    for t in tags {
        if (t.code == SOUND_STREAM_HEAD || t.code == SOUND_STREAM_HEAD2) && tags.iter().any(|b| b.code == SOUND_STREAM_BLOCK) {
            return Some((t, tags));
        }
        if let Some(s) = &t.sprite {
            if let Some(r) = first_stream_head(&s.tags) {
                return Some(r);
            }
        }
    }
    None
}

fn flash_player(v: u8) -> String {
    let s = match v {
        9 => "9.0",
        10 => "10.1",
        11 => "10.2",
        12 => "10.3",
        13..=22 => return format!("11.{}", v - 13),
        23 => "12.0",
        24..=43 => return format!("{}.0", v - 11),
        44 => "33.1",
        50 => "50.0",
        51.. => "51.0",
        _ => "null",
    };
    s.into()
}

pub fn publish_settings(c: &Converter, base: &str, width: f64, height: f64, network: bool, doc: &Option<Pack>) -> String {
    let cc = c.version >= super::convert::FlaVersion::Cc;
    let swf = c.swf;
    let wd = java_double(width);
    let ht = java_double(height);
    let mut w = XmlWriter::new();
    let v = |w: &mut XmlWriter, n: &str, x: &str| w.value(n, x);
    w.start("flash_profiles");
    w.start_with("flash_profile", &[("version", "1.0"), ("name", "Default"), ("current", "true")]);
    w.start_with("PublishFormatProperties", &[("enabled", "true")]);
    for (n, x) in [("defaultNames", "1"), ("flash", "1"), ("projectorWin", "0"), ("projectorMac", "0"), ("html", "1"), ("gif", "0"), ("jpeg", "0"), ("png", "0"), (if cc { "svg" } else { "qt" }, "0"), ("rnwk", "0"), ("swc", "0")] {
        v(&mut w, n, x);
    }
    for n in ["flash", "projectorWin", "projectorMac", "html", "gif", "jpeg", "png", if cc { "svg" } else { "qt" }, "rnwk", "swc"] {
        v(&mut w, &format!("{n}DefaultName"), "1");
    }
    for (n, e) in [("flash", "swf"), ("projectorWin", "exe"), ("projectorMac", "app"), ("html", "html"), ("gif", "gif"), ("jpeg", "jpg"), ("png", "png")] {
        v(&mut w, &format!("{n}FileName"), &format!("{base}.{e}"));
    }
    v(&mut w, if cc { "svgFileName" } else { "qtFileName" }, "1");
    v(&mut w, "rnwkFileName", &format!("{base}.smil"));
    v(&mut w, "swcFileName", &format!("{base}.swc"));
    w.end();
    w.start_with("PublishHtmlProperties", &[("enabled", "true")]);
    v(&mut w, "VersionDetectionIfAvailable", "0");
    v(&mut w, "VersionInfo", "12,0,0,0;11,2,0,0;11,1,0,0;10,3,0,0;10,2,153,0;10,1,52,0;9,0,124,0;8,0,24,0;7,0,14,0;6,0,79,0;5,0,58,0;4,0,32,0;3,0,8,0;2,0,1,12;1,0,0,1;");
    v(&mut w, "UsingDefaultContentFilename", "1");
    v(&mut w, "UsingDefaultAlternateFilename", "1");
    v(&mut w, "ContentFilename", &format!("{base}_content.html"));
    v(&mut w, "AlternateFilename", &format!("{base}_alternate.html"));
    v(&mut w, "UsingOwnAlternateFile", "0");
    v(&mut w, "OwnAlternateFilename", "");
    v(&mut w, "Width", &wd);
    v(&mut w, "Height", &ht);
    for (n, x) in [("Align", "0"), ("Units", "0"), ("Loop", "1"), ("StartPaused", "0"), ("Scale", "0"), ("HorizontalAlignment", "1"), ("VerticalAlignment", "1"), ("Quality", "4"), ("DeblockingFilter", "0"), ("WindowMode", "0"), ("DisplayMenu", "1"), ("DeviceFont", "0"), ("TemplateFileName", ""), ("showTagWarnMsg", "1")] {
        v(&mut w, n, x);
    }
    w.end();
    w.start_with("PublishFlashProperties", &[("enabled", "true")]);
    for (n, x) in [("TopDown", ""), ("FireFox", ""), ("Report", "0"), ("Protect", "0"), ("OmitTraceActions", "0"), ("Quality", "80"), ("DeblockingFilter", "0")] {
        v(&mut w, n, x);
    }
    match first_stream_head(&swf.timeline.tags) {
        None => {
            v(&mut w, "StreamFormat", "0");
            v(&mut w, "StreamCompress", "7");
        }
        Some((head, tags)) => {
            let h = read_head(head);
            let pos = tags.iter().position(|t| std::ptr::eq(t, head)).unwrap_or(0);
            let data: &[u8] = tags[pos..].iter().find(|t| t.code == SOUND_STREAM_BLOCK).map_or(&[][..], |t| &t.body[..]);
            let (mut format, mut compress) = (0, 0);
            if let Some(h) = h {
                if matches!(h.format, sf::ADPCM | sf::PCM_LE | sf::PCM_NATIVE) {
                    if h.stereo {
                        format += 1;
                    }
                    format += [0, 2, 4, 6][h.rate as usize & 3];
                }
                if h.format == sf::SPEEX {
                    compress = 18;
                }
                if h.format == sf::ADPCM {
                    if let Ok(c) = Reader::new(data).ub(2) {
                        compress = 2 + c as i32;
                    }
                }
                if h.format == sf::MP3 {
                    format = -1;
                    if !h.stereo {
                        format += 1;
                    }
                    format += 4;
                    if let Some(k) = mp3_bitrate(data.get(2..).unwrap_or(&[])) {
                        compress = bitrate_bits(k).0;
                    }
                }
            }
            v(&mut w, "StreamFormat", &format.to_string());
            v(&mut w, "StreamCompress", &compress.to_string());
        }
    }
    let version = swf.header.version.min(c.version.max_swf_version());
    let doc_class = crate::names::Names::new(swf).classes.get(&0).and_then(|v| v.first()).cloned();
    let _ = doc;
    let compression = swf.header.compression;
    for (n, x) in [
        ("EventFormat", "0".to_string()),
        ("EventCompress", "7".into()),
        ("OverrideSounds", "0".into()),
        ("Version", version.to_string()),
        ("ExternalPlayer", format!("FlashPlayer{}", flash_player(version))),
        ("ActionScriptVersion", if c.as3 { "3" } else { "2" }.into()),
        ("PackageExportFrame", "1".into()),
        ("PackagePaths", "".into()),
        ("AS3PackagePaths", ".".into()),
        ("AS3ConfigConst", "CONFIG::FLASH_AUTHORING=\"true\";".into()),
        ("DebuggingPermitted", "0".into()),
        ("DebuggingPassword", "".into()),
        ("CompressMovie", if compression == crate::Compression::None { "0" } else { "1" }.into()),
        ("CompressionType", if compression == crate::Compression::Lzma { "1" } else { "0" }.into()),
        ("InvisibleLayer", "1".into()),
        ("DeviceSound", "0".into()),
        ("StreamUse8kSampleRate", "0".into()),
        ("EventUse8kSampleRate", "0".into()),
        ("UseNetwork", if network { "1" } else { "0" }.into()),
        ("DocumentClass", if c.as3 { doc_class.unwrap_or_default() } else { String::new() }),
        ("AS3Strict", "2".into()),
        ("AS3Coach", "4".into()),
        ("AS3AutoDeclare", "4096".into()),
        ("AS3Dialect", "AS3".into()),
        ("AS3ExportFrame", "1".into()),
        ("AS3Optimize", "1".into()),
        ("ExportSwc", "0".into()),
        ("ScriptStuckDelay", "15".into()),
        ("IncludeXMP", "1".into()),
        ("HardwareAcceleration", "0".into()),
        ("AS3Flags", "4102".into()),
        ("DefaultLibraryLinkage", "rsl".into()),
        ("RSLPreloaderMethod", "wrap".into()),
        ("RSLPreloaderSWF", "$(AppConfig)/ActionScript 3.0/rsls/loader_animation.swf".into()),
    ] {
        v(&mut w, n, &x);
    }
    if cc {
        w.start("LibraryPath");
        w.start("library-path-entry");
        v(&mut w, "swc-path", "$(AppConfig)/ActionScript 3.0/libs");
        v(&mut w, "linkage", "merge");
        w.end();
        for p in ["flex", "core"] {
            w.start("library-path-entry");
            v(&mut w, "swc-path", &format!("$(FlexSDK)/frameworks/libs/{p}.swc"));
            v(&mut w, "linkage", "merge");
            v(&mut w, "rsl-url", "textLayout_2.0.0.232.swz");
            w.end();
        }
        w.end();
        w.value_raw("LibraryVersions", "\r\n      ");
    } else {
        w.start("LibraryPath");
        w.start("library-path-entry");
        v(&mut w, "swc-path", "$(AppConfig)/ActionScript 3.0/libs");
        v(&mut w, "linkage", "merge");
        w.end();
        w.start("library-path-entry");
        v(&mut w, "swc-path", "$(AppConfig)/ActionScript 3.0/libs/11.0/textLayout.swc");
        w.start_with("linkage", &[("usesDefault", "true")]);
        w.text("rsl");
        w.end();
        v(&mut w, "rsl-url", "http://fpdownload.adobe.com/pub/swz/tlf/2.0.0.232/textLayout_2.0.0.232.swz");
        v(&mut w, "policy-file-url", "http://fpdownload.adobe.com/pub/swz/crossdomain.xml");
        v(&mut w, "rsl-url", "textLayout_2.0.0.232.swz");
        w.end();
        w.end();
        w.start("LibraryVersions");
        w.start("library-version");
        v(&mut w, "swc-path", "$(AppConfig)/ActionScript 3.0/libs/11.0/textLayout.swc");
        w.empty_with("feature", &[("name", "tlfText"), ("majorVersion", "2"), ("minorVersion", "0"), ("build", "232")]);
        v(&mut w, "rsl-url", "http://fpdownload.adobe.com/pub/swz/tlf/2.0.0.232/textLayout_2.0.0.232.swz");
        v(&mut w, "policy-file-url", "http://fpdownload.adobe.com/pub/swz/crossdomain.xml");
        v(&mut w, "rsl-url", "textLayout_2.0.0.232.swz");
        w.end();
        w.end();
    }
    w.end();
    w.start_with("PublishJpegProperties", &[("enabled", "true")]);
    v(&mut w, "Width", &wd);
    v(&mut w, "Height", &ht);
    for (n, x) in [("Progressive", "0"), ("DPI", "4718592"), ("Size", "0"), ("Quality", "80"), ("MatchMovieDim", "1")] {
        v(&mut w, n, x);
    }
    w.end();
    w.start_with("PublishRNWKProperties", &[("enabled", "true")]);
    for (n, x) in [
        ("exportFlash", "1"),
        ("flashBitRate", "0"),
        ("exportAudio", "1"),
        ("audioFormat", "0"),
        ("singleRateAudio", "0"),
        ("realVideoRate", "100000"),
        ("speed28K", "1"),
        ("speed56K", "1"),
        ("speedSingleISDN", "0"),
        ("speedDualISDN", "0"),
        ("speedCorporateLAN", "0"),
        ("speed256K", "0"),
        ("speed384K", "0"),
        ("speed512K", "0"),
        ("exportSMIL", "1"),
    ] {
        v(&mut w, n, x);
    }
    w.end();
    w.start_with("PublishGifProperties", &[("enabled", "true")]);
    v(&mut w, "Width", &wd);
    v(&mut w, "Height", &ht);
    for (n, x) in [
        ("Animated", "0"),
        ("MatchMovieDim", "1"),
        ("Loop", "1"),
        ("LoopCount", ""),
        ("OptimizeColors", "1"),
        ("Interlace", "0"),
        ("Smooth", "1"),
        ("DitherSolids", "0"),
        ("RemoveGradients", "0"),
        ("TransparentOption", ""),
        ("TransparentAlpha", "128"),
        ("DitherOption", ""),
        ("PaletteOption", ""),
        ("MaxColors", "255"),
        ("PaletteName", ""),
    ] {
        v(&mut w, n, x);
    }
    w.end();
    w.start_with("PublishPNGProperties", &[("enabled", "true")]);
    v(&mut w, "Width", &wd);
    v(&mut w, "Height", &ht);
    for (n, x) in [
        ("OptimizeColors", "1"),
        ("Interlace", "0"),
        ("Transparent", "0"),
        ("Smooth", "1"),
        ("DitherSolids", "0"),
        ("RemoveGradients", "0"),
        ("MatchMovieDim", "1"),
        ("DitherOption", ""),
        ("FilterOption", ""),
        ("PaletteOption", ""),
        ("BitDepth", "24-bit with Alpha"),
        ("MaxColors", "255"),
        ("PaletteName", ""),
    ] {
        v(&mut w, n, x);
    }
    w.end();
    if !cc {
        w.start_with("PublishQTProperties", &[("enabled", "true")]);
        v(&mut w, "Width", &wd);
        v(&mut w, "Height", &ht);
        for (n, x) in [
            ("MatchMovieDim", "1"),
            ("UseQTSoundCompression", "0"),
            ("AlphaOption", ""),
            ("LayerOption", ""),
            ("QTSndSettings", "00000000"),
            ("ControllerOption", "0"),
            ("Looping", "0"),
            ("PausedAtStart", "0"),
            ("PlayEveryFrame", "0"),
            ("Flatten", "1"),
        ] {
            v(&mut w, n, x);
        }
        w.end();
    }
    w.end();
    w.end();
    w.to_string()
}
