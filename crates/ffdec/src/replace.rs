//! Asset replacement: images, sounds, binary data, and tag bodies.

use std::io::{Cursor, Write};

use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::{DynamicImage, GenericImageView};

use crate::error::{Error, Result};
use crate::io::Reader;
use crate::tags::code::*;
use crate::Swf;

/// Replaces the image of an image character tag.
pub fn replace_image(swf: &mut Swf, path: &[usize], image_bytes: &[u8]) -> Result<()> {
    let tag = swf.tag(path).ok_or_else(|| Error::Other("tag not found".into()))?;
    let char_id = tag.character_id().ok_or_else(|| Error::Other("tag does not define a character".into()))?;
    let code = tag.code;

    let img = image::load_from_memory(image_bytes)
        .map_err(|e| Error::Other(format!("decoding replacement image: {e}")))?;
    let (_width, _height) = img.dimensions();

    let new_body = match code {
        DEFINE_BITS_LOSSLESS | DEFINE_BITS_LOSSLESS2 => {
            encode_lossless(char_id, code == DEFINE_BITS_LOSSLESS2, &img)?
        }
        DEFINE_BITS_JPEG2 => {
            let mut body = char_id.to_le_bytes().to_vec();
            if is_jpeg(image_bytes) {
                body.extend_from_slice(image_bytes);
            } else {
                body.extend_from_slice(&encode_jpeg(&img)?);
            }
            body
        }
        DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 => {
            encode_jpeg3_or_4(char_id, code == DEFINE_BITS_JPEG4, &img)?
        }
        DEFINE_BITS => {
            let mut body = char_id.to_le_bytes().to_vec();
            if is_jpeg(image_bytes) {
                body.extend_from_slice(image_bytes);
            } else {
                body.extend_from_slice(&encode_jpeg(&img)?);
            }
            body
        }
        _ => return Err(Error::Other(format!("tag 0x{code:x} is not an image tag"))),
    };

    swf.replace_tag_body(path, &new_body)
}

/// Replaces an image by its SWF character ID.
pub fn replace_image_by_id(swf: &mut Swf, character_id: u16, image_bytes: &[u8]) -> Result<()> {
    let path = swf
        .find_character(character_id)
        .ok_or_else(|| Error::Other(format!("character ID {character_id} not found")))?;
    replace_image(swf, &path, image_bytes)
}

/// Replaces a sound tag (DefineSound) with audio bytes (MP3, WAV, or raw stream).
pub fn replace_sound(swf: &mut Swf, path: &[usize], sound_bytes: &[u8]) -> Result<()> {
    let tag = swf.tag(path).ok_or_else(|| Error::Other("tag not found".into()))?;
    if tag.code != DEFINE_SOUND {
        return Err(Error::Other(format!("tag 0x{:x} is not a DefineSound tag", tag.code)));
    }
    let char_id = tag.character_id().ok_or_else(|| Error::Other("missing sound character ID".into()))?;

    let new_body = if is_mp3(sound_bytes) {
        encode_mp3_sound(char_id, sound_bytes)?
    } else if is_wav(sound_bytes) {
        encode_wav_sound(char_id, sound_bytes)?
    } else {
        // Fall back to preserving original flags and sample count, replacing sound data.
        let mut body = tag.body.clone();
        if body.len() >= 7 {
            body.truncate(7);
            body.extend_from_slice(sound_bytes);
            body
        } else {
            return Err(Error::Other("invalid DefineSound tag body".into()));
        }
    };

    swf.replace_tag_body(path, &new_body)
}

/// Replaces a sound by its SWF character ID.
pub fn replace_sound_by_id(swf: &mut Swf, character_id: u16, sound_bytes: &[u8]) -> Result<()> {
    let path = swf
        .find_character(character_id)
        .ok_or_else(|| Error::Other(format!("character ID {character_id} not found")))?;
    replace_sound(swf, &path, sound_bytes)
}

/// Replaces a binary data tag (DefineBinaryData).
pub fn replace_binary(swf: &mut Swf, path: &[usize], data: &[u8]) -> Result<()> {
    let tag = swf.tag(path).ok_or_else(|| Error::Other("tag not found".into()))?;
    if tag.code != DEFINE_BINARY_DATA {
        return Err(Error::Other(format!("tag 0x{:x} is not a DefineBinaryData tag", tag.code)));
    }
    let char_id = tag.character_id().ok_or_else(|| Error::Other("missing character ID".into()))?;
    let mut body = char_id.to_le_bytes().to_vec();
    body.extend_from_slice(&[0, 0, 0, 0]); // reserved
    body.extend_from_slice(data);
    swf.replace_tag_body(path, &body)
}

/// Replaces a binary data tag by its SWF character ID.
pub fn replace_binary_by_id(swf: &mut Swf, character_id: u16, data: &[u8]) -> Result<()> {
    let path = swf
        .find_character(character_id)
        .ok_or_else(|| Error::Other(format!("character ID {character_id} not found")))?;
    replace_binary(swf, &path, data)
}

/// Replaces the initial text in a DefineEditText tag.
pub fn replace_edit_text(swf: &mut Swf, path: &[usize], new_text: &str) -> Result<()> {
    let tag = swf.tag(path).ok_or_else(|| Error::Other("tag not found".into()))?;
    if tag.code != DEFINE_EDIT_TEXT {
        return Err(Error::Other(format!("tag 0x{:x} is not a DefineEditText tag", tag.code)));
    }
    let mut r = Reader::new(&tag.body);
    let char_id = r.u16()?;
    let bounds_start = r.pos();
    let _ = crate::types::Rect::read(&mut r)?;
    let bounds_end = r.pos();
    let bounds_bytes = tag.body[bounds_start..bounds_end].to_vec();
    let mut f1 = r.u8()?;
    let f2 = r.u8()?;
    let has_font = f1 & 0x01 != 0;
    let mut font_id = 0u16;
    let mut font_class = String::new();
    let mut font_height = 0u16;
    if has_font {
        font_id = r.u16()?;
    }
    if f2 & 0x80 != 0 {
        font_class = r.string()?.to_string();
    }
    if has_font {
        font_height = r.u16()?;
    }
    let color = if f1 & 0x04 != 0 { Some(r.bytes(4)?.to_vec()) } else { None };
    let max_length = if f1 & 0x02 != 0 { Some(r.u16()?) } else { None };
    let layout = if f2 & 0x20 != 0 { Some(r.bytes(9)?.to_vec()) } else { None };
    let variable = r.string()?.to_string();

    f1 |= 0x80; // set HasText flag

    let mut w = crate::io::Writer::new();
    w.u16(char_id);
    w.bytes(&bounds_bytes);
    w.u8(f1);
    w.u8(f2);
    if has_font {
        w.u16(font_id);
    }
    if f2 & 0x80 != 0 {
        w.string(&font_class);
    }
    if has_font {
        w.u16(font_height);
    }
    if let Some(c) = color {
        w.bytes(&c);
    }
    if let Some(m) = max_length {
        w.u16(m);
    }
    if let Some(l) = layout {
        w.bytes(&l);
    }
    w.string(&variable);
    w.string(new_text);

    swf.replace_tag_body(path, &w.data)
}

fn is_jpeg(data: &[u8]) -> bool {
    data.len() > 2 && data[0] == 0xff && data[1] == 0xd8
}

fn is_mp3(data: &[u8]) -> bool {
    let d = skip_id3(data);
    d.len() > 2 && d[0] == 0xff && (d[1] & 0xe0) == 0xe0
}

fn is_wav(data: &[u8]) -> bool {
    data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE"
}

fn skip_id3(data: &[u8]) -> &[u8] {
    if data.len() >= 10 && &data[..3] == b"ID3" {
        let size = ((data[6] as usize & 0x7f) << 21)
            | ((data[7] as usize & 0x7f) << 14)
            | ((data[8] as usize & 0x7f) << 7)
            | (data[9] as usize & 0x7f);
        if 10 + size <= data.len() {
            return &data[10 + size..];
        }
    }
    data
}

fn encode_jpeg(img: &DynamicImage) -> Result<Vec<u8>> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    let mut cursor = Cursor::new(&mut out);
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| Error::Other(format!("encoding JPEG: {e}")))?;
    Ok(out)
}

fn encode_lossless(char_id: u16, alpha: bool, img: &DynamicImage) -> Result<Vec<u8>> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut raw_pixels = Vec::with_capacity((w * h * 4) as usize);

    for p in rgba.pixels() {
        if alpha {
            let a = p[3];
            let r = ((p[0] as u32 * a as u32) / 255) as u8;
            let g = ((p[1] as u32 * a as u32) / 255) as u8;
            let b = ((p[2] as u32 * a as u32) / 255) as u8;
            raw_pixels.extend_from_slice(&[a, r, g, b]);
        } else {
            raw_pixels.extend_from_slice(&[0, p[0], p[1], p[2]]);
        }
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&raw_pixels)
        .map_err(|e| Error::Other(format!("compressing lossless pixels: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| Error::Other(format!("finishing lossless compression: {e}")))?;

    let mut body = Vec::with_capacity(7 + compressed.len());
    body.extend_from_slice(&char_id.to_le_bytes());
    body.push(5); // 32-bit ARGB format
    body.extend_from_slice(&(w as u16).to_le_bytes());
    body.extend_from_slice(&(h as u16).to_le_bytes());
    body.extend_from_slice(&compressed);
    Ok(body)
}

fn encode_jpeg3_or_4(char_id: u16, is_jpeg4: bool, img: &DynamicImage) -> Result<Vec<u8>> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let mut alpha_bytes = Vec::with_capacity((w * h) as usize);
    for p in rgba.pixels() {
        alpha_bytes.push(p[3]);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&alpha_bytes)
        .map_err(|e| Error::Other(format!("compressing JPEG3 alpha: {e}")))?;
    let compressed_alpha = encoder
        .finish()
        .map_err(|e| Error::Other(format!("finishing JPEG3 alpha: {e}")))?;

    let jpeg_bytes = encode_jpeg(img)?;
    let alpha_offset = jpeg_bytes.len() as u32;

    let mut body = Vec::with_capacity(6 + (if is_jpeg4 { 2 } else { 0 }) + jpeg_bytes.len() + compressed_alpha.len());
    body.extend_from_slice(&char_id.to_le_bytes());
    body.extend_from_slice(&alpha_offset.to_le_bytes());
    if is_jpeg4 {
        body.extend_from_slice(&[0, 0]); // deblock parameter
    }
    body.extend_from_slice(&jpeg_bytes);
    body.extend_from_slice(&compressed_alpha);
    Ok(body)
}

fn encode_mp3_sound(char_id: u16, mp3_data: &[u8]) -> Result<Vec<u8>> {
    let clean = skip_id3(mp3_data);
    // Parse first MP3 frame header to detect sample rate and channels
    let (rate_flag, channels_flag, sample_rate) = parse_mp3_header(clean)?;
    let sound_format = 2u8; // MP3
    let sound_size = 1u8; // 16-bit
    let flags = (sound_format << 4) | (rate_flag << 2) | (sound_size << 1) | channels_flag;

    // Estimate sample count: approx 1152 samples per frame or total duration * sample_rate
    let sample_count = estimate_mp3_samples(clean, sample_rate);

    let mut body = Vec::with_capacity(9 + clean.len());
    body.extend_from_slice(&char_id.to_le_bytes());
    body.push(flags);
    body.extend_from_slice(&sample_count.to_le_bytes());
    // SWF MP3 stream begins with 2 bytes SeekSamples (0)
    body.extend_from_slice(&[0, 0]);
    body.extend_from_slice(clean);
    Ok(body)
}

fn parse_mp3_header(data: &[u8]) -> Result<(u8, u8, u32)> {
    if data.len() < 4 {
        return Ok((3, 1, 44100)); // Default 44kHz stereo
    }
    for i in 0..data.len().saturating_sub(3) {
        if data[i] == 0xff && (data[i + 1] & 0xe0) == 0xe0 {
            let b1 = data[i + 1];
            let b2 = data[i + 2];
            let b3 = data[i + 3];

            let version = (b1 >> 3) & 3; // 3 = MPEG 1, 2 = MPEG 2, 0 = MPEG 2.5
            let rate_idx = ((b2 >> 2) & 3) as usize;
            let channel_mode = (b3 >> 6) & 3;
            let stereo = if channel_mode == 3 { 0 } else { 1 };

            let sr = match version {
                3 => [44100, 48000, 32000, 44100][rate_idx],
                2 => [22050, 24000, 16000, 22050][rate_idx],
                _ => [11025, 12000, 8000, 11025][rate_idx],
            };

            let rate_flag = match sr {
                s if s >= 44000 => 3,
                s if s >= 22000 => 2,
                s if s >= 11000 => 1,
                _ => 0,
            };
            return Ok((rate_flag, stereo, sr));
        }
    }
    Ok((3, 1, 44100))
}

fn estimate_mp3_samples(data: &[u8], sample_rate: u32) -> u32 {
    let mut frames = 0u32;
    let mut i = 0;
    while i + 4 <= data.len() {
        if data[i] == 0xff && (data[i + 1] & 0xe0) == 0xe0 {
            frames += 1;
            // approximate frame length jump
            let b2 = data[i + 2];
            let bitrate_idx = ((b2 >> 4) & 0x0f) as usize;
            let padding = (b2 >> 1) & 1;
            let bitrates = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];
            let br = bitrates.get(bitrate_idx).copied().unwrap_or(128) * 1000;
            let frame_len = if sample_rate > 0 && br > 0 {
                ((144 * br / sample_rate) as usize + padding as usize).max(4)
            } else {
                4
            };
            i += frame_len;
        } else {
            i += 1;
        }
    }
    frames * 1152
}

fn encode_wav_sound(char_id: u16, wav_data: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(wav_data);
    let riff = r.bytes(4)?;
    let _riff_size = r.u32()?;
    let wave = r.bytes(4)?;
    if riff != b"RIFF" || wave != b"WAVE" {
        return Err(Error::Other("invalid WAV header".into()));
    }

    let mut channels = 1u16;
    let mut sample_rate = 44100u32;
    let mut bits_per_sample = 16u16;
    let mut pcm_data = &[][..];

    while !r.is_empty() {
        let chunk_id = r.bytes(4)?;
        let chunk_size = r.u32()? as usize;
        let chunk_bytes = r.bytes(chunk_size)?;
        if chunk_id == b"fmt " && chunk_size >= 14 {
            let mut cr = Reader::new(chunk_bytes);
            let _format = cr.u16()?;
            channels = cr.u16()?;
            sample_rate = cr.u32()?;
            let _byte_rate = cr.u32()?;
            let _block_align = cr.u16()?;
            if chunk_size >= 16 {
                bits_per_sample = cr.u16()?;
            }
        } else if chunk_id == b"data" {
            pcm_data = chunk_bytes;
        }
    }

    let rate_flag = match sample_rate {
        s if s >= 44000 => 3,
        s if s >= 22000 => 2,
        s if s >= 11000 => 1,
        _ => 0,
    };
    let sound_format = 3u8; // Uncompressed little-endian
    let sound_size = if bits_per_sample == 16 { 1u8 } else { 0u8 };
    let stereo = if channels > 1 { 1u8 } else { 0u8 };
    let flags = (sound_format << 4) | (rate_flag << 2) | (sound_size << 1) | stereo;

    let bytes_per_sample = (channels as usize) * (bits_per_sample as usize / 8).max(1);
    let sample_count = if bytes_per_sample > 0 {
        (pcm_data.len() / bytes_per_sample) as u32
    } else {
        0
    };

    let mut body = Vec::with_capacity(7 + pcm_data.len());
    body.extend_from_slice(&char_id.to_le_bytes());
    body.push(flags);
    body.extend_from_slice(&sample_count.to_le_bytes());
    body.extend_from_slice(pcm_data);
    Ok(body)
}
