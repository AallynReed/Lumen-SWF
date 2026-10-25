//! Files holding several SWFs: ZIP archives, SWC libraries (the SWFs
//! their catalog lists) and any other file, searched for embedded SWF headers.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use crate::container;
use crate::error::{Error, Result};
use crate::swf::Swf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Zip,
    Swc,
    /// SWFs found inside another file, keyed by their offset as `[offset]`.
    Binary,
    /// An Iggy movie, converted to SWF and back.
    Iggy,
}

pub struct Bundle {
    pub kind: Kind,
    /// Openable files in archive order, by key.
    pub entries: Vec<(String, Vec<u8>)>,
}

impl Bundle {
    /// Only archives can take an edited SWF back.
    pub fn read_only(&self) -> bool {
        self.kind == Kind::Binary
    }
}

/// `OpenableSourceInfo.isBundle`: anything not named like a SWF or ABC file.
pub fn is_bundle(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    !matches!(ext.as_deref(), Some("swf" | "spl" | "swt" | "gfx" | "abc"))
}

pub fn open(path: &Path, data: &[u8]) -> Result<Bundle> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("swc") => Ok(Bundle { kind: Kind::Swc, entries: swc_entries(data)? }),
        Some("zip") => Ok(Bundle { kind: Kind::Zip, entries: zip_entries(data, openable)? }),
        Some("iggy") => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let file = crate::iggy::file::IggyFile::read(data, name.as_deref())?;
            let companion = path.with_file_name(&file.swf.texture_resource_name);
            if let Ok(t) = std::fs::read(&companion) {
                crate::iggy::texture::register_companion(&file.swf.texture_resource_name, t);
            }
            Ok(Bundle { kind: Kind::Iggy, entries: vec![(file.swf.name.clone(), crate::iggy::to_swf::swf_bytes(&file)?)] })
        }
        _ => Ok(Bundle { kind: Kind::Binary, entries: search(data) }),
    }
}

fn openable(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".swf", ".spl", ".gfx", ".abc"].iter().any(|e| n.ends_with(e))
}

fn zip_err(e: zip::result::ZipError) -> Error {
    Error::Other(format!("ZIP: {e}"))
}

fn zip_entries(data: &[u8], keep: impl Fn(&str) -> bool) -> Result<Vec<(String, Vec<u8>)>> {
    let mut z = zip::ZipArchive::new(Cursor::new(data)).map_err(zip_err)?;
    let mut out = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(zip_err)?;
        if f.is_file() && keep(f.name()) {
            let mut v = Vec::new();
            f.read_to_end(&mut v)?;
            out.push((f.name().to_string(), v));
        }
    }
    Ok(out)
}

/// `SWC.initBundle`: the `path` of every `library` in catalog.xml.
fn swc_entries(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let catalog = zip_entries(data, |n| n == "catalog.xml")?;
    let mut paths = Vec::new();
    if let Some((_, xml)) = catalog.first() {
        if let Ok(doc) = roxmltree::Document::parse(&String::from_utf8_lossy(xml)) {
            for n in doc.descendants().filter(|n| n.tag_name().name().eq_ignore_ascii_case("library")) {
                if let Some(p) = n.attribute("path") {
                    paths.push(p.to_string());
                }
            }
        }
    }
    zip_entries(data, |n| paths.iter().any(|p| p == n))
}

/// `putOpenable`: the file at `path` (bytes `data`) with entry `key` replaced by `swf`; an
/// Iggy file takes the SWF's changes like `IggySwfBundle`.
pub fn put(path: &Path, data: &[u8], key: &str, swf: &[u8]) -> Result<Vec<u8>> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iggy")) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let mut file = crate::iggy::file::IggyFile::read(data, name.as_deref())?;
        crate::iggy::from_swf::update(&mut file, &Swf::parse(swf)?)?;
        return file.save();
    }
    replace(data, key, swf)
}

/// `ZippedBundle.putOpenable`: the archive with entry `key` replaced by `swf`.
pub fn replace(archive: &[u8], key: &str, swf: &[u8]) -> Result<Vec<u8>> {
    let mut z = zip::ZipArchive::new(Cursor::new(archive)).map_err(zip_err)?;
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut found = false;
    for i in 0..z.len() {
        let f = z.by_index_raw(i).map_err(zip_err)?;
        if f.name() == key {
            found = true;
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            let name = f.name().to_string();
            drop(f);
            w.start_file(name, opts).map_err(zip_err)?;
            w.write_all(swf)?;
        } else {
            w.raw_copy_file(f).map_err(zip_err)?;
        }
    }
    if !found {
        return Err(Error::Other(format!("the archive has no entry {key}")));
    }
    Ok(w.finish().map_err(zip_err)?.into_inner())
}

/// `SwfHeaderStreamSearch` and `SWFSearch` in ALL mode: every offset where a SWF header
/// starts a file that parses with tags, keyed `[offset]`.
pub fn search(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for i in 0..data.len().saturating_sub(4) {
        let m = match &data[i..i + 3] {
            [b'F' | b'C' | b'Z', b'W', b'S'] | [b'G' | b'C', b'F', b'X'] => true,
            _ => false,
        };
        if !m {
            continue;
        }
        let Some(len) = container::encoded_len(&data[i..]) else { continue };
        let slice = &data[i..i + len];
        let Ok(swf) = Swf::parse(slice) else { continue };
        let ok = swf.header.file_size > 0
            && swf.header.version > 0
            && swf.header.version <= 64
            && !swf.timeline.tags.is_empty()
            && !swf.warnings.iter().any(|w| w.contains("ended early") || w.contains("decompressed") || w.contains("shorter"));
        if ok {
            out.push((format!("[{i}]"), slice.to_vec()));
        }
    }
    out
}
