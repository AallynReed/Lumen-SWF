//! The 8-byte SWF header, compression and decompression of the body.

use std::io::{Read, Write};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::harman;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Compression {
    None,
    Zlib,
    Lzma,
    /// Non-standard LZMA container with the `ABC` signature.
    LzmaAbc,
}

impl Compression {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" | "fws" => Some(Self::None),
            "zlib" | "cws" => Some(Self::Zlib),
            "lzma" | "zws" => Some(Self::Lzma),
            "abc" | "lzma_abc" => Some(Self::LzmaAbc),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub signature: String,
    pub compression: Compression,
    pub gfx: bool,
    pub encrypted: bool,
    pub version: u8,
    pub file_size: u32,
    pub lzma_props: Option<[u8; 5]>,
}

pub struct Decoded {
    pub header: Header,
    /// Uncompressed body: everything after the 8-byte header.
    pub body: Vec<u8>,
    /// Bytes after the declared end of an uncompressed file, kept for byte-exact saving.
    pub trailer: Vec<u8>,
    pub warnings: Vec<String>,
}

const DEFAULT_LZMA_PROPS: [u8; 5] = [93, 0, 0, 32, 0];

pub fn decode(data: &[u8]) -> Result<Decoded> {
    if data.len() < 8 {
        return Err(Error::HeaderTooShort);
    }
    let mut hdr: [u8; 8] = data[..8].try_into().unwrap();
    let sig = String::from_utf8_lossy(&hdr[..3]).into_owned();
    let gfx = &hdr[1..3] == b"FX";
    let valid = matches!(&hdr[..3], b"FWS" | b"CWS" | b"ZWS" | b"GFX" | b"CFX" | b"ABC" | b"fWS" | b"cWS" | b"zWS");
    if !valid {
        return Err(Error::BadSignature(sig));
    }
    let mut warnings = Vec::new();
    let encrypted = harman::is_encrypted_signature(hdr[0]);
    let decrypted;
    let mut payload = &data[8..];
    if encrypted {
        let (h, d) = harman::decrypt(&hdr, payload)?;
        hdr = h;
        decrypted = d;
        payload = &decrypted;
    }
    let version = hdr[3];
    let file_size = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
    let expected = (file_size as usize).saturating_sub(8);
    let mut trailer = Vec::new();
    let mut lzma_props = None;

    let (compression, body) = match hdr[0] {
        b'C' => {
            let mut out = Vec::with_capacity(expected);
            if let Err(e) = flate2::read::ZlibDecoder::new(payload).take(expected as u64).read_to_end(&mut out) {
                warnings.push(format!("zlib stream ended early: {e}"));
            }
            (Compression::Zlib, out)
        }
        b'Z' => {
            if payload.len() < 9 {
                return Err(Error::Lzma("header too short".into()));
            }
            let props: [u8; 5] = payload[4..9].try_into().unwrap();
            lzma_props = Some(props);
            (Compression::Lzma, lzma_decode(&payload[9..], props, expected, &mut warnings)?)
        }
        b'A' => {
            if payload.len() < 13 {
                return Err(Error::Lzma("header too short".into()));
            }
            let props: [u8; 5] = payload[..5].try_into().unwrap();
            lzma_props = Some(props);
            (Compression::LzmaAbc, lzma_decode(&payload[13..], props, expected, &mut warnings)?)
        }
        _ => {
            let mut end = expected.min(payload.len());
            // Old GFx files store the size without the 8-byte header.
            if gfx && payload.len() == file_size as usize {
                end = payload.len();
            }
            if payload.len() < expected {
                warnings.push(format!("file is {} bytes shorter than its header says", expected - payload.len()));
            }
            trailer = payload[end..].to_vec();
            (Compression::None, payload[..end].to_vec())
        }
    };
    if body.len() < expected && compression != Compression::None {
        warnings.push(format!("decompressed {} of {} bytes", body.len(), expected));
    }

    Ok(Decoded {
        header: Header { signature: sig, compression, gfx, encrypted, version, file_size, lzma_props },
        body,
        trailer,
        warnings,
    })
}

fn lzma_decode(data: &[u8], props: [u8; 5], size: usize, warnings: &mut Vec<String>) -> Result<Vec<u8>> {
    let dict = u32::from_le_bytes(props[1..5].try_into().unwrap());
    let mut reader = lzma_rust2::LzmaReader::new_with_props(data, size as u64, props[0], dict, None)
        .map_err(|e| Error::Lzma(e.to_string()))?;
    reader.set_relaxed_end_condition(true);
    let mut out = Vec::with_capacity(size);
    if let Err(e) = reader.read_to_end(&mut out) {
        warnings.push(format!("LZMA stream ended early: {e}"));
    }
    Ok(out)
}

fn lzma_encode(body: &[u8], props: Option<[u8; 5]>) -> Result<([u8; 5], Vec<u8>)> {
    let props = props.unwrap_or(DEFAULT_LZMA_PROPS);
    let mut opts = lzma_rust2::LzmaOptions::with_preset(6);
    let p = props[0] as u32;
    opts.lc = p % 9;
    opts.lp = (p / 9) % 5;
    opts.pb = p / 45;
    opts.dict_size = u32::from_le_bytes(props[1..5].try_into().unwrap()).max(4096);
    let mut w = lzma_rust2::LzmaWriter::new_no_header(Vec::new(), &opts, true).map_err(|e| Error::Lzma(e.to_string()))?;
    w.write_all(body)?;
    let out = w.finish().map_err(|e| Error::Lzma(e.to_string()))?;
    let mut used = [0u8; 5];
    used[0] = opts.get_props();
    used[1..].copy_from_slice(&opts.dict_size.to_le_bytes());
    Ok((used, out))
}

pub struct EncodeOptions {
    pub compression: Compression,
    pub gfx: bool,
    pub encrypt: bool,
    pub version: u8,
    pub file_size: u32,
    pub lzma_props: Option<[u8; 5]>,
}

pub fn encode(body: &[u8], trailer: &[u8], o: &EncodeOptions) -> Result<Vec<u8>> {
    let sig: [u8; 3] = match o.compression {
        Compression::LzmaAbc => *b"ABC",
        c => {
            let first = match c {
                Compression::Lzma => b'Z',
                Compression::Zlib => b'C',
                _ if o.gfx => b'G',
                _ => b'F',
            };
            if o.gfx { [first, b'F', b'X'] } else { [first, b'W', b'S'] }
        }
    };
    let mut header = [0u8; 8];
    header[..3].copy_from_slice(&sig);
    header[3] = o.version;
    header[4..].copy_from_slice(&o.file_size.to_le_bytes());

    let mut payload = Vec::new();
    match o.compression {
        Compression::None => {
            payload.extend_from_slice(body);
            payload.extend_from_slice(trailer);
        }
        Compression::Zlib => {
            let mut enc = flate2::write::ZlibEncoder::new(payload, flate2::Compression::best());
            enc.write_all(body)?;
            payload = enc.finish()?;
        }
        Compression::Lzma => {
            let (props, data) = lzma_encode(body, o.lzma_props)?;
            payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
            payload.extend_from_slice(&props);
            payload.extend_from_slice(&data);
        }
        Compression::LzmaAbc => {
            let (props, data) = lzma_encode(body, o.lzma_props)?;
            payload.extend_from_slice(&props);
            payload.extend_from_slice(&(body.len() as u64).to_le_bytes());
            payload.extend_from_slice(&data);
        }
    }

    if o.encrypt && !o.gfx && o.compression != Compression::LzmaAbc {
        return harman::encrypt(&header, &payload);
    }
    let mut out = header.to_vec();
    out.extend_from_slice(&payload);
    Ok(out)
}
