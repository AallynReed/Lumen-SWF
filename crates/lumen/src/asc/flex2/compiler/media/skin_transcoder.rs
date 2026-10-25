//! Port of `flex2.compiler.media.SkinTranscoder`: a `skinClass` embed as a generated `IBorder`/`IFlexDisplayObject`
//! subclass. Which members it needs comes from `TypeAnalyzer.analyzeClass`, part of the MXML reflection that is not
//! ported yet; [`ClassInfoProvider`] is where it plugs in.

use super::abstract_transcoder::AbstractTranscoder;
use super::velocity_boundary::{merge_skin_class, SkinClassContext};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults, CORE_PACKAGE};
use crate::asc::flex2::compiler::util::{name_formatter, QName};
use crate::asc::flex2::flash::swf::tags::TagRef;

/// The `ClassInfo` questions `generateSource` asks.
pub trait ClassInfo {
    fn implements_interface(&self, namespace: &str, name: &str) -> bool;
    fn defines_getter(&self, name: &str, inherited: bool) -> bool;
    fn defines_function(&self, name: &str, inherited: bool) -> bool;
    fn extends_class(&self, name: &str) -> bool;
}

/// `symbolTable.getTypeAnalyzer().analyzeClass(null, multiName)`.
pub type ClassInfoProvider = fn(&SymbolTable, &str) -> Option<Box<dyn ClassInfo>>;

/// The `TypeAnalyzer` the MXML port installs, once.
pub static CLASS_INFO_PROVIDER: std::sync::OnceLock<ClassInfoProvider> = std::sync::OnceLock::new();

pub struct SkinTranscoder {
    base: AbstractTranscoder,
}

impl Default for SkinTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SkinTranscoder {
    pub fn new() -> SkinTranscoder {
        SkinTranscoder { base: AbstractTranscoder::new(&[super::mime::SKIN], None, false, "flex2.compiler.media.SkinTranscoder") }
    }

    fn do_transcode(symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        if !generate_source {
            return Err(TranscoderException::embed_requires_codegen(args.get(transcoder::ORIGINAL).map_or("null", |s| s.as_str()), class_name));
        }
        let skin_class_name = args.get(transcoder::SKINCLASS).cloned().unwrap_or_default();
        let colon = name_formatter::to_colon(&skin_class_name);
        let found = symbol_table.and_then(|st| st.find_source_by_qname(&QName::parse(&colon)));
        if found.is_none() {
            return Err(TranscoderException::new(format!("class for skin '{skin_class_name}' not found")));
        }
        let info = CLASS_INFO_PROVIDER.get().and_then(|p| p(symbol_table.unwrap(), &colon));
        let Some(info) = info else { return Err(TranscoderException::unable_to_generate_source(class_name)) };
        let code = generate_skin_source(class_name, &skin_class_name, info.as_ref()).ok_or_else(|| TranscoderException::unable_to_generate_source(class_name))?;
        Ok(super::abstract_transcoder::new_results(TranscodingResults { generated_code: Some(code), ..Default::default() }))
    }
}

/// `generateSource(fullClassName, baseClassName, classInfo)`.
fn generate_skin_source(full_class_name: &str, base_class_name: &str, ci: &dyn ClassInfo) -> Option<String> {
    let flex_movie_clip_or_sprite = ci.extends_class(&format!("{CORE_PACKAGE}:FlexMovieClip")) || ci.extends_class(&format!("{CORE_PACKAGE}:FlexSprite"));
    let (package_name, class_name) = match full_class_name.rfind('.') {
        Some(dot) => (&full_class_name[..dot], &full_class_name[dot + 1..]),
        None => ("", full_class_name),
    };
    merge_skin_class(&SkinClassContext {
        package_name: package_name.to_string(),
        base_class_name: base_class_name.to_string(),
        class_name: class_name.to_string(),
        needs_iborder: !ci.implements_interface(CORE_PACKAGE, "IBorder"),
        needs_border_metrics: !ci.defines_getter("borderMetrics", true),
        needs_iflex_display_object: !ci.implements_interface(CORE_PACKAGE, "IFlexDisplayObject"),
        needs_measured_height: !ci.defines_getter("measuredHeight", true),
        needs_measured_width: !ci.defines_getter("measuredWidth", true),
        needs_move: !ci.defines_function("move", true),
        needs_set_actual_size: !ci.defines_function("setActualSize", true),
        needs_name: !flex_movie_clip_or_sprite,
        needs_to_string: !flex_movie_clip_or_sprite,
    })
}

impl Transcoder for SkinTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, _context: &PathResolver, symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        self.base.transcode(args, &|a| a == transcoder::SKINCLASS, &mut |args| Self::do_transcode(symbol_table, args, class_name, generate_source))
    }

    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        self.base.get_associated_class(tag)
    }

    fn clear(&mut self) {}
}
