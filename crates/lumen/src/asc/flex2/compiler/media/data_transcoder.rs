//! Port of `flex2.compiler.media.DataTranscoder`: a file as a `DefineBinaryData` behind a `ByteArrayAsset`.

use super::abstract_transcoder::{self, new_results, resolve_source, AbstractTranscoder};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults, CORE_PACKAGE};
use crate::asc::flex2::flash::swf::tags::{codes, Body, Tag, TagRef};

pub struct DataTranscoder {
    base: AbstractTranscoder,
}

impl Default for DataTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTranscoder {
    pub fn new() -> DataTranscoder {
        DataTranscoder { base: AbstractTranscoder::new(&["application/octet-stream"], None, false, "flex2.compiler.media.DataTranscoder") }
    }

    pub fn do_transcode(context: &PathResolver, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let results = new_results(TranscodingResults::with_source(resolve_source(context, args)?));
        load_data(&results)?;
        if generate_source {
            abstract_transcoder::generate_source(&results, Some(format!("{CORE_PACKAGE}.ByteArrayAsset")), class_name, args, &[])?;
        }
        Ok(results)
    }
}

/// `loadData(asset)`.
pub fn load_data(asset: &ResultsRef) -> Result<(), TranscoderException> {
    let src = asset.borrow().asset_source.clone().unwrap();
    let data = src.to_byte_array().map_err(|_| TranscoderException::unable_to_read_source(src.get_name()))?;
    asset.borrow_mut().define_tag = Some(Tag::new_ref(codes::DEFINE_BINARY_DATA, Body::DefineBinaryData { reserved: 0, data }));
    Ok(())
}

impl Transcoder for DataTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        self.base.transcode(args, &|_| false, &mut |args| Self::do_transcode(context, args, class_name, generate_source))
    }

    fn get_associated_class(&self, _tag: Option<&TagRef>) -> Option<String> {
        Some(format!("{CORE_PACKAGE}.ByteArrayAsset"))
    }

    fn clear(&mut self) {}
}
