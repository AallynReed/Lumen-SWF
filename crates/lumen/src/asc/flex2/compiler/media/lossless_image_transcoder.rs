//! Port of `flex2.compiler.media.LosslessImageTranscoder`: GIF and PNG pixels as a `DefineBitsLossless2`, or with
//! `compression` as an ImageIO JPEG in a `DefineBitsJPEG3` with the alpha channel beside it.

use super::abstract_transcoder::{AbstractTranscoder, DefineTagClass};
use super::image_transcoder::{self, ImageInfo};
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{Args, PathResolver, ResultsRef, Transcoder, TranscoderException};
use crate::asc::flex2::flash::graphics::{imageio_jpeg, images::LosslessImage};
use crate::asc::flex2::flash::swf::builder::tags::build_lossless;
use crate::asc::flex2::flash::swf::tags::{codes, BitsKind, Body, DefineBits, Tag, TagRef};

pub const COMPRESSION: &str = "compression";
pub const QUALITY: &str = "quality";

pub struct LosslessImageTranscoder {
    base: AbstractTranscoder,
}

fn invalid_quality() -> TranscoderException {
    TranscoderException::new("Quality must be a double between 0 and 100.")
}

/// `Double.parseDouble` for the decimal forms an Embed attribute holds.
pub fn parse_double(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let t = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    if t.is_empty() || t.contains(['x', 'X', '_']) {
        return None;
    }
    match t {
        "NaN" | "+NaN" | "-NaN" => Some(f64::NAN),
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ if t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') => None,
        _ => t.parse::<f64>().ok(),
    }
}

impl LosslessImageTranscoder {
    pub fn new() -> LosslessImageTranscoder {
        LosslessImageTranscoder { base: AbstractTranscoder::new(&[super::mime::GIF, super::mime::PNG], Some(DefineTagClass::DefineSprite), true, "flex2.compiler.media.LosslessImageTranscoder") }
    }

    pub fn is_supported_attribute(attr: &str) -> bool {
        attr == COMPRESSION || attr == QUALITY || image_transcoder::is_supported_attribute(attr)
    }

    /// `getImage(sourceFile, args)`.
    pub fn get_image(source: &VirtualFile, args: &Args) -> Result<ImageInfo, TranscoderException> {
        let bytes = source.to_byte_array().map_err(|e| TranscoderException::exception_while_transcoding(&e.to_string()))?;
        let image = LosslessImage::new(source.get_name(), &bytes).map_err(|e| TranscoderException::exception_while_transcoding(&e))?;
        let define_bits = if args.contains_key(COMPRESSION) && image_transcoder::parse_boolean(args.get(COMPRESSION)) {
            let size = (image.width * image.height) as usize;
            let pixels = &image.pixels;
            let alpha: Vec<u8> = (0..size).map(|i| ((pixels.get(i).copied().unwrap_or(0) >> 24) & 0xff) as u8).collect();
            let mut quality: f32 = 1.0;
            if let Some(q) = args.get(QUALITY) {
                let p = parse_double(q).ok_or_else(invalid_quality)?;
                if p < 0.0 || p > 100.0 {
                    return Err(invalid_quality());
                }
                quality = (p / 100.0) as f32;
            }
            let mut b = DefineBits::new(BitsKind::Jpeg3);
            b.data = imageio_jpeg::buffered_image_to_jpeg(pixels, image.width, image.height, quality).map_err(|e| TranscoderException::exception_while_transcoding(&e))?;
            b.alpha_data_offset = alpha.len() as i64;
            b.alpha_data = alpha;
            Tag::new_ref(codes::DEFINE_BITS_JPEG3, Body::DefineBits(b))
        } else {
            if args.contains_key(QUALITY) {
                return Err(TranscoderException::new("Quality requires compress=\"true\"."));
            }
            build_lossless(&image.pixels, image.width, image.height)
        };
        Ok(ImageInfo { define_bits, width: image.width, height: image.height })
    }
}

impl Default for LosslessImageTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Transcoder for LosslessImageTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let dt = self.base.define_tag;
        self.base.transcode(args, &Self::is_supported_attribute, &mut |args| {
            image_transcoder::do_transcode(dt, context, args, class_name, generate_source, &Self::get_image)
        })
    }

    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        image_transcoder::get_associated_class(self.base.define_tag, tag)
    }

    fn clear(&mut self) {
        self.base.clear();
    }
}
