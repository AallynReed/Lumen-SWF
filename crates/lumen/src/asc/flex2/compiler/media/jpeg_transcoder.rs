//! Port of `flex2.compiler.media.JPEGTranscoder`: the JPEG kept as it is in a `DefineBitsJPEG2`.

use super::abstract_transcoder::{AbstractTranscoder, DefineTagClass};
use super::image_transcoder::{self, ImageInfo};
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{Args, PathResolver, ResultsRef, Transcoder, TranscoderException};
use crate::asc::flex2::flash::graphics::images::JpegImage;
use crate::asc::flex2::flash::swf::builder::tags::build_jpeg;
use crate::asc::flex2::flash::swf::tags::TagRef;

pub struct JpegTranscoder {
    base: AbstractTranscoder,
}

impl JpegTranscoder {
    pub fn new() -> JpegTranscoder {
        JpegTranscoder { base: AbstractTranscoder::new(&[super::mime::JPG, super::mime::JPEG], Some(DefineTagClass::DefineSprite), true, "flex2.compiler.media.JPEGTranscoder") }
    }

    /// `getImage(source, args)`.
    pub fn get_image(source: &VirtualFile, _args: &Args) -> Result<ImageInfo, TranscoderException> {
        let data = source.to_byte_array().map_err(|e| TranscoderException::exception_while_transcoding(&e.to_string()))?;
        let image = JpegImage::new(source.get_name(), data).map_err(|e| TranscoderException::exception_while_transcoding(&e))?;
        Ok(ImageInfo { width: image.width, height: image.height, define_bits: build_jpeg(image.data) })
    }
}

impl Default for JpegTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Transcoder for JpegTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let dt = self.base.define_tag;
        self.base.transcode(args, &image_transcoder::is_supported_attribute, &mut |args| {
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
