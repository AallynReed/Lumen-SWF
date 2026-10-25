//! Port of `flex2.compiler.media.XMLTranscoder`: a file as an `XML` literal in a generated class.

use super::abstract_transcoder::{new_results, resolve_source, AbstractTranscoder};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults};
use crate::asc::flex2::flash::swf::tags::TagRef;

pub const ENCODING: &str = "encoding";

pub struct XmlTranscoder {
    base: AbstractTranscoder,
}

impl Default for XmlTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

/// `windows-1252`, the `file.encoding` mxmlc's JVM runs with here.
fn decode_cp1252(b: &[u8]) -> String {
    const HIGH: [u16; 32] = [
        0x20ac, 0xfffd, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160, 0x2039, 0x0152, 0xfffd, 0x017d, 0xfffd, 0xfffd,
        0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0xfffd, 0x017e, 0x0178,
    ];
    let units: Vec<u16> = b.iter().map(|&c| if (0x80..0xa0).contains(&c) { HIGH[(c - 0x80) as usize] } else { c as u16 }).collect();
    String::from_utf16_lossy(&units)
}

fn decode_utf16(b: &[u8]) -> String {
    let (le, rest) = match b {
        [0xff, 0xfe, rest @ ..] => (true, rest),
        [0xfe, 0xff, rest @ ..] => (false, rest),
        _ => (false, b),
    };
    let units: Vec<u16> = rest.chunks(2).map(|c| if c.len() < 2 { 0xfffd } else if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
    String::from_utf16_lossy(&units)
}

/// `new InputStreamReader(in, FileUtils.consumeBOM(in, encoding))` read to the end.
fn read_text(bytes: &[u8], encoding: Option<&str>) -> String {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return decode_utf16(bytes);
    }
    let enc = encoding.filter(|e| !e.is_empty()).unwrap_or("Cp1252").to_ascii_lowercase().replace(['-', '_'], "");
    match enc.as_str() {
        "utf8" => String::from_utf8_lossy(bytes).into_owned(),
        "utf16" | "utf16be" | "unicodebig" => decode_utf16(bytes),
        "utf16le" | "unicodelittle" => {
            let units: Vec<u16> = bytes.chunks(2).map(|c| if c.len() < 2 { 0xfffd } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
            String::from_utf16_lossy(&units)
        }
        "iso88591" | "latin1" | "8859_1" => bytes.iter().map(|&c| c as char).collect(),
        "usascii" | "ascii" => bytes.iter().map(|&c| if c < 128 { c as char } else { '\u{fffd}' }).collect(),
        _ => decode_cp1252(bytes),
    }
}

impl XmlTranscoder {
    pub fn new() -> XmlTranscoder {
        XmlTranscoder { base: AbstractTranscoder::new(&[super::mime::XML], None, false, "flex2.compiler.media.XMLTranscoder") }
    }

    fn do_transcode(context: &PathResolver, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let source = resolve_source(context, args)?;
        let results = new_results(TranscodingResults::with_source(source.clone()));
        if generate_source {
            Self::generate_source(&results, class_name, args)?;
        } else {
            return Err(TranscoderException::embed_requires_codegen(source.get_name(), class_name));
        }
        Ok(results)
    }

    /// `generateSource(asset, fullClassName, embedMap)`.
    fn generate_source(asset: &ResultsRef, full_class_name: &str, embed_map: &Args) -> Result<(), TranscoderException> {
        let encoding = embed_map.get(ENCODING).cloned();
        let (package_name, class_name) = match full_class_name.rfind('.') {
            Some(dot) => (&full_class_name[..dot], &full_class_name[dot + 1..]),
            None => ("", full_class_name),
        };
        let src = asset.borrow().asset_source.clone().unwrap();
        let bytes = src.to_byte_array().map_err(|_| TranscoderException::unable_to_read_source(src.get_name()))?;
        let mut s = String::with_capacity(1024);
        s.push_str("package ");
        s.push_str(package_name);
        s.push_str(" { public class ");
        s.push_str(class_name);
        s.push_str(" { public static var data:XML = ");
        s.push_str(&read_text(&bytes, encoding.as_deref()));
        s.push_str("; } }");
        asset.borrow_mut().generated_code = Some(s);
        Ok(())
    }
}

impl Transcoder for XmlTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        self.base.transcode(args, &|a| a == ENCODING, &mut |args| Self::do_transcode(context, args, class_name, generate_source))
    }

    fn get_associated_class(&self, _tag: Option<&TagRef>) -> Option<String> {
        Some("Object".to_string())
    }

    fn clear(&mut self) {}
}
