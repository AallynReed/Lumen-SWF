//! Port of `flex2.compiler.media.SVGTranscoder`: an SVG (or gzipped SVGZ) file as a `DefineSprite` of shapes,
//! behind a `SpriteAsset`.

use super::abstract_transcoder::{self, new_results, resolve_source, AbstractTranscoder, DefineTagClass, SCALE9BOTTOM, SCALE9LEFT, SCALE9RIGHT, SCALE9TOP};
use super::mime;
use super::movie_transcoder::define_scaling_grid;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults};
use crate::asc::flex2::flash::svg::sprite_transcoder::{SpriteTranscoder, TranscodeFailure};
use crate::asc::flex2::flash::swf::tags::{new_sprite, TagRef};
use crate::asc::flex2::fonts::font_description::JUrl;

const MIME_TYPES: &[&str] = &[mime::SVG, mime::SVG_XML];

pub struct SvgTranscoder {
    base: AbstractTranscoder,
    inner: Inner,
}

struct Inner {
    deprecation_issued: bool,
    show_deprecation_warnings: bool,
}

/// `new SVGTranscoder(showDeprecationWarnings)`, for `media::SVG_TRANSCODER`.
pub fn factory(cfg: &Configuration) -> Box<dyn Transcoder> {
    Box::new(SvgTranscoder::new(cfg.show_deprecation_warnings))
}

fn is_supported_attribute(attr: &str) -> bool {
    SCALE9TOP == attr || SCALE9LEFT == attr || SCALE9BOTTOM == attr || SCALE9RIGHT == attr
}

/// `source.getURL()`: `File.toURL()` of the path as it was resolved.
fn get_url(f: &VirtualFile) -> String {
    match f {
        VirtualFile::Local { name, .. } => {
            let p = transcoder::raw_path(name).unwrap_or_else(|| std::path::PathBuf::from(name));
            JUrl::from_path(&p).external_form
        }
        _ => f.get_name().to_string(),
    }
}

impl SvgTranscoder {
    pub fn new(show_deprecation_warnings: bool) -> SvgTranscoder {
        SvgTranscoder { base: AbstractTranscoder::new(MIME_TYPES, Some(DefineTagClass::DefineSprite), true, "flex2.compiler.media.SVGTranscoder"), inner: Inner { deprecation_issued: false, show_deprecation_warnings } }
    }
}

impl Inner {
    fn do_transcode(&mut self, context: &PathResolver, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        if !self.deprecation_issued && self.show_deprecation_warnings {
            toolkit::log(CompilerMessage::warning("The use of SVG has been deprecated since Flex 4. Please use FXG."), None, -1, -1, None);
            self.deprecation_issued = true;
        }
        let results = new_results(TranscodingResults::with_source(resolve_source(context, args)?));
        let new_name = args.get(transcoder::NEWNAME).cloned();
        let source = results.borrow().asset_source.clone().unwrap();
        let tag = svg(&source, new_name, args)?;
        results.borrow_mut().define_tag = Some(tag);
        if generate_source {
            let base = abstract_transcoder::associated_class(Some(DefineTagClass::DefineSprite), results.borrow().define_tag.as_ref());
            abstract_transcoder::generate_source(&results, base, class_name, args, &[])?;
        }
        Ok(results)
    }
}

/// `svg(source, newName, args)`.
fn svg(source: &VirtualFile, new_name: Option<String>, args: &Args) -> Result<TagRef, TranscoderException> {
    let doc_uri = get_url(source);
    let raw = source.to_byte_array().map_err(|e| TranscoderException::exception_while_transcoding(&e.to_string()))?;
    let bytes = if is_gzip_compressed(&raw) {
        let mut out = Vec::new();
        match std::io::Read::read_to_end(&mut flate2::read::MultiGzDecoder::new(&raw[..]), &mut out) {
            Ok(_) => out,
            Err(_) => {
                let m = "Unexpected end of ZLIB input stream";
                crate::asc::flex2::batik::bridge::bridge_context::print_stderr(&format!("java.io.EOFException: {m}"));
                return Err(TranscoderException::exception_while_transcoding(&format!("null\nEnclosed Exception:\n{m}")));
            }
        }
    } else {
        raw
    };
    let sprite = transcode_svg(&doc_uri, bytes, new_name)?;
    if args.contains_key(SCALE9LEFT) || args.contains_key(SCALE9RIGHT) || args.contains_key(SCALE9TOP) || args.contains_key(SCALE9BOTTOM) {
        define_scaling_grid(&sprite, args)?;
    }
    Ok(sprite)
}

/// `transcodeSVG(ti, source, symbolName)`.
fn transcode_svg(uri: &str, bytes: Vec<u8>, symbol_name: Option<String>) -> Result<TagRef, TranscoderException> {
    let wrapped = |m: &str| TranscoderException::exception_while_transcoding(&format!("null\nEnclosed Exception:\n{m}"));
    match SpriteTranscoder::transcode(uri, bytes) {
        Ok(t) => {
            let sprite = new_sprite(symbol_name);
            {
                let mut s = sprite.borrow_mut();
                let d = s.sprite_mut().unwrap();
                d.tag_list = t.tags;
                d.framecount = 1;
            }
            Ok(sprite)
        }
        Err(TranscodeFailure::Io(m)) | Err(TranscodeFailure::Wrapped(m)) => Err(wrapped(&m)),
        Err(TranscodeFailure::Runtime(m)) => Err(TranscoderException::exception_while_transcoding(m.as_deref().unwrap_or("null"))),
        Err(TranscodeFailure::Unsupported(m)) => Err(TranscoderException::exception_while_transcoding(&format!("{m} is not supported by this SVG transcoder"))),
    }
}

/// `isGZIPCompressed(in)`; a stream shorter than the magic prints `EOFException`.
fn is_gzip_compressed(b: &[u8]) -> bool {
    if b.len() < 3 {
        if b.len() < 2 || u16::from_le_bytes([b[0], b[1]]) == 0x8b1f {
            crate::asc::flex2::batik::bridge::bridge_context::print_stderr("java.io.EOFException");
        }
        return false;
    }
    u16::from_le_bytes([b[0], b[1]]) == 0x8b1f && b[2] == 8
}

impl Transcoder for SvgTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let inner = &mut self.inner;
        self.base.transcode(args, &is_supported_attribute, &mut |args| inner.do_transcode(context, args, class_name, generate_source))
    }

    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        self.base.get_associated_class(tag)
    }

    fn clear(&mut self) {
        self.base.clear();
    }
}
