//! The `flex2.compiler.util.MimeMappings` table `EmbedUtil` reads, with `findMimeType`'s `HashMap` order.

use crate::asc::util::jhash::JHashMap;

pub const MXML: &str = "text/mxml";
pub const AS: &str = "text/as";
pub const FXG: &str = "text/fxg";
pub const ABC: &str = "application/x-actionscript-bytecode";
pub const CSS: &str = "text/css";
pub const PROPERTIES: &str = "text/properties";
pub const JPEG: &str = "image/jpeg";
pub const JPG: &str = "image/jpg";
pub const PNG: &str = "image/png";
pub const GIF: &str = "image/gif";
pub const SVG: &str = "image/svg";
pub const SVG_XML: &str = "image/svg-xml";
pub const MP3: &str = "audio/mpeg";
pub const FLASH: &str = "application/x-shockwave-flash";
pub const XML: &str = "text/xml";
pub const TTF: &str = "application/x-font-truetype";
pub const TTC: &str = "application/x-font-truetype-collection";
pub const OTF: &str = "application/x-font-opentype";
pub const FONT: &str = "application/x-font";
pub const DFONT: &str = "application/x-dfont";
pub const PBJ: &str = "application/x-pbj";
pub const TIFF: &str = "image/tiff";
pub const SKIN: &str = "skin";

/// `new MimeMappings()`.
fn mappings() -> JHashMap<String, Vec<&'static str>> {
    let mut m = JHashMap::new();
    for (mime, ext) in [
        (MXML, &[".mxml"][..]),
        (AS, &[".as"]),
        (FXG, &[".fxg"]),
        (ABC, &[".abc"]),
        (CSS, &[".css"]),
        (PROPERTIES, &[".properties"]),
        (JPEG, &[".jpg", ".jpeg"]),
        (JPG, &[".jpg", ".jpeg"]),
        (PNG, &[".png"]),
        (GIF, &[".gif"]),
        (SVG, &[".svg", ".svgz"]),
        (MP3, &[".mp3"]),
        (FLASH, &[".swf"]),
        (XML, &[".xml"]),
        (TTF, &[".ttf"]),
        (TTC, &[".ttc"]),
        (OTF, &[".otf"]),
        (DFONT, &[".dfont"]),
        (PBJ, &[".pbj"]),
        (TIFF, &[".tiff", ".tif"]),
    ] {
        m.insert(mime.to_string(), ext.to_vec());
    }
    m
}

/// `MimeMappings.getMimeType(name)`: the first mime type, in map order, with an extension `name` ends with.
pub fn get_mime_type(name: &str) -> Option<String> {
    let m = mappings();
    let units: Vec<u16> = crate::asc::util::jstr::to_utf16(name);
    for (mime, exts) in m.iter() {
        for e in exts {
            let eu: Vec<u16> = e.encode_utf16().collect();
            if units.len() > eu.len() {
                let tail = &units[units.len() - eu.len()..];
                if tail.iter().zip(&eu).all(|(a, b)| a == b || (*a < 128 && (*a as u8).eq_ignore_ascii_case(&(*b as u8)))) {
                    return Some(mime.clone());
                }
            }
        }
    }
    None
}
