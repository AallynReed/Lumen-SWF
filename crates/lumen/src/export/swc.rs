//! SWF to SWC: the SWF as library.swf, with a stub document class,
//! and a catalog of each DoABC2 tag's definitions and dependencies.

use std::io::{Cursor, Write};

use sha2::{Digest, Sha256};

use crate::abc::decomp::{self, names::Dotted};
use crate::abc::{tag_abc, Abc};
use crate::error::{Error, Result};
use crate::io::{Reader, Writer};
use crate::tags::{code::*, Tag};
use crate::{SaveOptions, Swf};

const STUB: &[u8] = include_bytes!("swc_main.bin");
const DOCUMENT_CLASS: &str = "_myhash_flash_display_Sprite";

/// `BigInteger.toString(16)`, padded to at least 32 digits.
fn sha256(data: &[u8]) -> String {
    let hex: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    let t = hex.trim_start_matches('0');
    format!("{t:0>32}")
}

fn dep_id(d: &Dotted) -> String {
    if d.without_last().is_empty() {
        d.last().to_string()
    } else {
        format!("{}:{}", d.without_last().raw(), d.last())
    }
}

/// The SWF as stored in an SWC: the stub DoABC2 tag before the first ABC tag, and
/// the stub as the document class.
pub fn library(swf: &Swf) -> Result<Swf> {
    let mut swf = swf.clone();
    if let Some(i) = swf.timeline.tags.iter().position(|t| t.code == DO_ABC || t.code == DO_ABC2) {
        swf.insert_tag(&[i], Tag::new(DO_ABC2, STUB.to_vec()))?;
    }
    for i in 0..swf.timeline.tags.len() {
        if swf.timeline.tags[i].code != SYMBOL_CLASS {
            continue;
        }
        let mut r = Reader::new(&swf.timeline.tags[i].body);
        let n = r.u16()?;
        let mut entries = Vec::new();
        for _ in 0..n {
            entries.push((r.u16()?, r.string()?));
        }
        if let Some(e) = entries.iter_mut().find(|e| e.0 == 0) {
            e.1 = DOCUMENT_CLASS.into();
            let mut w = Writer::new();
            w.u16(n);
            for (id, name) in &entries {
                w.u16(*id);
                w.string(name);
            }
            w.bytes(r.rest());
            swf.set_tag_body(&[i], w.data)?;
        }
    }
    Ok(swf)
}

/// `generateCatalog`, with `mod` set to `time` in milliseconds.
pub fn catalog(swf: &Swf, library: &[u8], time: u64, skip_dependencies: bool) -> String {
    let mut abcs: Vec<(Option<String>, Abc)> = Vec::new();
    swf.walk(|_, t| {
        if let Some((data, name)) = tag_abc(t) {
            if let Ok(abc) = Abc::parse(data) {
                abcs.push((name, abc));
            }
        }
    });
    let refs: Vec<&Abc> = abcs.iter().map(|a| &a.1).collect();
    let index = decomp::index_for(&refs);
    let packs: Vec<Vec<(Dotted, Vec<(Dotted, u8)>)>> = decomp::with_big_stack(|| abcs.iter().map(|(_, abc)| decomp::script_packs(&index, abc, !skip_dependencies)).collect());
    let defined: Vec<&Dotted> = packs.iter().flatten().map(|p| &p.0).collect();

    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding =\"utf-8\"?>\n<swc xmlns=\"http://www.adobe.com/flash/swccatalog/9\">\n  <versions>\n    <swc version=\"1.2\" />\n    <flex version=\"4.6.0\" build=\"23201\" minimumSupportedVersion=\"3.0.0\" />\n  </versions>\n  <features>\n    <feature-script-deps />\n    <feature-files />\n  </features>\n  <libraries>\n    <library path=\"library.swf\">\n");
    for ((name, _), packs) in abcs.iter().zip(&packs) {
        let Some(name) = name else { continue };
        s.push_str(&format!("      <script name=\"{name}\" mod=\"{time}\">\n"));
        for (def, deps) in packs {
            s.push_str(&format!("        <def id=\"{}\" />\n        <dep id=\"AS3\" type=\"n\" />\n", dep_id(def)));
            let mut all = vec![Dotted::new(&["AS3"])];
            for (id, ty) in deps {
                if id.last() == "*" || (id.without_last().is_empty() && !defined.contains(&id)) || all.contains(id) {
                    continue;
                }
                let ty = ["n", "i", "s", "e"][*ty as usize];
                s.push_str(&format!("        <dep id=\"{}\" type=\"{ty}\" />\n", dep_id(id)));
                all.push(id.clone());
            }
        }
        s.push_str("      </script>\n");
    }
    s.push_str(&format!("      <digests>\n        <digest type=\"SHA-256\" signed=\"false\" value=\"{}\" />\n      </digests>\n    </library>\n  </libraries>\n  <files>\n  </files>\n</swc>\n", sha256(library)));
    s
}

/// The SWC archive: library.swf, then catalog.xml.
pub fn swc(swf: &Swf, skip_dependencies: bool) -> Result<Vec<u8>> {
    let lib = library(swf)?;
    let bytes = lib.save(&SaveOptions { compression: lib.header.compression, encrypt: lib.header.encrypted })?;
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let cat = catalog(&lib, &bytes, time, skip_dependencies);
    let err = |e: zip::result::ZipError| Error::Other(format!("ZIP: {e}"));
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("library.swf", opts).map_err(err)?;
    z.write_all(&bytes)?;
    z.start_file("catalog.xml", opts).map_err(err)?;
    z.write_all(cat.as_bytes())?;
    Ok(z.finish().map_err(err)?.into_inner())
}
