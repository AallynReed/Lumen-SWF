//! Port of `flex2.compiler.as3.EmbedUtil`: resolves an embed's source the way mxmlc does, picks the transcoder by
//! mime type, and records the resulting assets on the compilation unit.

use crate::asc::flex2::compiler::assets::AssetInfo;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::media::mime;
use crate::asc::flex2::compiler::source::{SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, SinglePathResolver, TranscoderException, Transcoders};

/// A Java exception other than `TranscoderException`, which `EmbedUtil.transcode` does not catch.
pub struct Uncaught(pub String);

/// `transcode(transcoders, unit, symbolTable, className, args, line, col, generateCode)`.
#[allow(clippy::too_many_arguments)]
pub fn transcode(
    transcoders: &Transcoders,
    unit: &UnitRef,
    symbol_table: Option<&SymbolTable>,
    class_name: &str,
    args: &mut Args,
    line: i32,
    col: i32,
    generate_code: bool,
) -> Result<Option<ResultsRef>, Uncaught> {
    let mut context = PathResolver::new();
    let source = unit.borrow().get_source();
    if !args.contains_key(transcoder::RESOLVED_SOURCE) {
        let embed_source = args.get(transcoder::SOURCE).cloned();
        if let Some(es) = &embed_source {
            if es.is_empty() {
                return Err(Uncaught("String index out of range: 0".to_string()));
            }
        }
        if embed_source.as_deref().is_some_and(|s| s.starts_with('/')) {
            let s = source.borrow();
            if let Some(root) = s.get_path_root() {
                context.add_single_path_resolver(SinglePathResolver::LocalFile(root.to_string()));
            }
            if s.is_source_path_owner() {
                context.add_single_path_resolver(SinglePathResolver::SourcePath(transcoder::source_path_dirs()));
            }
        } else {
            if let Some(path) = args.get(transcoder::FILE) {
                let path = if args.get(transcoder::PATHSEP).map(String::as_str) == Some("true") { path.replace('/', "\\") } else { path.clone() };
                if let Some(context_file) = transcoder::resolve_local_file_path(&path) {
                    if context_file.get_name() != source.borrow().get_name() {
                        context.add_single_path_resolver(SinglePathResolver::LocalFile(context_file.get_name().to_string()));
                    }
                }
            }
            if let Some(backing) = backing_file_resolver(&source) {
                context.add_single_path_resolver(backing);
            }
        }
        context.add_single_path_resolver(SinglePathResolver::Toolkit);
    } else {
        context.add_single_path_resolver(SinglePathResolver::LocalFilePath);
    }
    let has = unit.borrow().assets.contains(class_name);
    if has {
        return Ok(None);
    }
    let results = transcode_with(transcoders, symbol_table, class_name, args, line, col, generate_code, Some(&source), &context)?;
    if let Some(r) = &results {
        let rb = r.borrow();
        if let Some(tag) = &rb.define_tag {
            unit.borrow_mut().assets.add(class_name, AssetInfo::new(tag.clone(), rb.asset_source.clone(), rb.modified, Some(args.clone())));
        }
        for asset in rb.additional_assets.iter().flatten() {
            let a = asset.borrow();
            if let Some(tag) = &a.define_tag {
                let name = a.class_name.clone().unwrap_or_default();
                unit.borrow_mut().assets.add(&name, AssetInfo::new(tag.clone(), rb.asset_source.clone(), rb.modified, Some(args.clone())));
            }
        }
    }
    Ok(results)
}

/// The backing file as a resolver: only a local file resolves relative paths (`TextFile.resolve` gives null).
fn backing_file_resolver(source: &SourceRef) -> Option<SinglePathResolver> {
    match source.borrow().get_backing_file() {
        VirtualFile::Local { name, .. } => Some(SinglePathResolver::LocalFile(name.clone())),
        VirtualFile::InMemory { .. } => None,
    }
}

/// `transcode(transcoders, symbolTable, className, args, line, col, generateCode, s, context)`.
#[allow(clippy::too_many_arguments)]
fn transcode_with(
    transcoders: &Transcoders,
    symbol_table: Option<&SymbolTable>,
    class_name: &str,
    args: &mut Args,
    line: i32,
    col: i32,
    generate_code: bool,
    s: Option<&SourceRef>,
    context: &PathResolver,
) -> Result<Option<ResultsRef>, Uncaught> {
    let request = format_transcode_request(args);
    let mut mime_type = args.get(transcoder::MIMETYPE).cloned();
    let mut origin = args.get(transcoder::FILE).cloned();
    if args.get(transcoder::PATHSEP).map(String::as_str) == Some("true") {
        origin = origin.map(|o| o.replace('/', "\\"));
    }
    let mut name_for_reporting = String::new();
    if let Some(s) = s {
        if origin.is_none() {
            origin = Some(s.borrow().get_name().to_string());
        }
        name_for_reporting = s.borrow().get_name_for_reporting().to_string();
    }
    if mime_type.is_none() {
        if let Some(source) = args.get(transcoder::SOURCE) {
            mime_type = mime::get_mime_type(source);
            if mime_type.is_none() {
                log_transcoder_exception(TranscoderException::unrecognized_extension(&request), &name_for_reporting, line, col);
                return Ok(None);
            }
        } else if args.contains_key(transcoder::SKINCLASS) {
            mime_type = Some(mime::SKIN.to_string());
        }
    }
    let mut ts = transcoders.borrow_mut();
    let found = mime_type.as_deref().and_then(|m| ts.iter().position(|t| t.is_supported(m)));
    let Some(i) = found else {
        log_transcoder_exception(TranscoderException::no_matching_transcoder(mime_type.as_deref().unwrap_or("null")), &name_for_reporting, line, col);
        return Ok(None);
    };
    if !args.contains_key(transcoder::SYMBOL) && !args.contains_key(transcoder::NEWNAME) {
        args.insert(transcoder::NEWNAME.to_string(), class_name.to_string());
    }
    match ts[i].transcode(context, symbol_table, args, class_name, generate_code) {
        Ok(r) => Ok(Some(r)),
        Err(e) if e.runtime => Err(Uncaught(e.message)),
        Err(e) => {
            log_transcoder_exception(e, origin.as_deref().unwrap_or(""), line, col);
            Ok(None)
        }
    }
}

/// `getTranscoder(transcoders, mimeType)`, as an index.
pub fn get_transcoder(transcoders: &Transcoders, mime_type: &str) -> Option<usize> {
    transcoders.borrow().iter().position(|t| t.is_supported(mime_type))
}

/// `formatTranscodeRequest(args)`.
pub fn format_transcode_request(args: &Args) -> String {
    if let Some(s) = args.get(transcoder::SOURCE) {
        return s.clone();
    }
    let parts: Vec<String> = args.iter().map(|(k, v)| format!("{k}='{v}'")).collect();
    format!("[{}]", parts.join(", "))
}

/// `logTranscoderException(e, path, line, column)`.
pub fn log_transcoder_exception(mut e: TranscoderException, path: &str, line: i32, column: i32) {
    e.path = Some(path.to_string());
    e.line = line;
    e.column = column;
    toolkit::log(CompilerMessage::error(e.message), Some(path), line, column, None);
}
