//! Port of `flex2.compiler.media.AbstractTranscoder`: attribute checking, the args-keyed transcoding cache, source
//! resolution, the `StandardDefs` asset base classes, and `EmbedClass.vm` code generation.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

use super::velocity_boundary::{self, EmbedClassContext};
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, TranscoderException, TranscodingResults, CORE_PACKAGE};
use crate::asc::flex2::flash::swf::tags::{Body, TagRef};
use crate::asc::util::jhash::JavaHash;

pub const SCALE9TOP: &str = "scaleGridTop";
pub const SCALE9LEFT: &str = "scaleGridLeft";
pub const SCALE9BOTTOM: &str = "scaleGridBottom";
pub const SCALE9RIGHT: &str = "scaleGridRight";
pub const FONTSTYLE: &str = "fontStyle";
pub const FONTWEIGHT: &str = "fontWeight";
pub const FONTNAME: &str = "fontName";
pub const FONTFAMILY: &str = "fontFamily";

/// `StandardDefs.PACKAGE_FLASH_DISPLAY + ".DisplayObject"`.
pub const ASSET_TYPE: &str = "flash.display.DisplayObject";

/// The `defineTag` class a transcoder was built with, for `isAssignableFrom`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefineTagClass {
    DefineSprite,
    DefineSound,
}

impl DefineTagClass {
    fn is_assignable_from(self, tag: &TagRef) -> bool {
        match self {
            DefineTagClass::DefineSprite => matches!(tag.borrow().body, Body::DefineSprite(_)),
            DefineTagClass::DefineSound => matches!(tag.borrow().body, Body::DefineSound(_)),
        }
    }
}

/// The state `AbstractTranscoder` keeps for its subclass.
pub struct AbstractTranscoder {
    pub mime_types: &'static [&'static str],
    pub define_tag: Option<DefineTagClass>,
    pub cache_tags: bool,
    pub transcoding_cache: HashMap<String, ResultsRef>,
    /// `getClass().getName()`, as `UnsupportedAttribute` reports it.
    pub java_class: &'static str,
}

impl AbstractTranscoder {
    pub fn new(mime_types: &'static [&'static str], define_tag: Option<DefineTagClass>, cache_tags: bool, java_class: &'static str) -> AbstractTranscoder {
        AbstractTranscoder { mime_types, define_tag, cache_tags, transcoding_cache: HashMap::new(), java_class }
    }

    pub fn is_supported(&self, mime_type: &str) -> bool {
        self.mime_types.iter().any(|m| m.eq_ignore_ascii_case(mime_type))
    }

    /// `transcode(...)` around the subclass's `doTranscode`.
    pub fn transcode(
        &mut self,
        args: &mut Args,
        is_supported_attribute: &dyn Fn(&str) -> bool,
        do_transcode: &mut dyn FnMut(&mut Args) -> Result<ResultsRef, TranscoderException>,
    ) -> Result<ResultsRef, TranscoderException> {
        for attr in args.keys() {
            let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
            if attr.starts_with('_') || eq(transcoder::SOURCE, attr) || eq(transcoder::MIMETYPE, attr) || eq(transcoder::NEWNAME, attr) {
                continue;
            }
            if transcoder::ORIGINAL != attr && !is_supported_attribute(attr) {
                return Err(TranscoderException::unsupported_attribute(attr, self.java_class));
            }
        }
        let mut results = None;
        if self.cache_tags {
            results = self.transcoding_cache.get(&get_cache_key(args)).cloned();
        }
        match results {
            Some(r) => Ok(r),
            None => {
                let r = do_transcode(args)?;
                if self.cache_tags {
                    self.transcoding_cache.insert(get_cache_key(args), r.clone());
                }
                Ok(r)
            }
        }
    }

    pub fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        associated_class(self.define_tag, tag)
    }

    pub fn clear(&mut self) {
        if !self.transcoding_cache.is_empty() {
            self.transcoding_cache = HashMap::new();
        }
    }
}

/// `getAssociatedClass(tag)` for a transcoder built with `define_tag`.
pub fn associated_class(define_tag: Option<DefineTagClass>, tag: Option<&TagRef>) -> Option<String> {
    {
        let tag = tag?;
        let skin_sprite = format!("{CORE_PACKAGE}.SpriteAsset");
        let cls = {
            let t = tag.borrow();
            match &t.body {
                Body::DefineButton(_) => Some(format!("{CORE_PACKAGE}.ButtonAsset")),
                Body::DefineFont1(_) | Body::DefineFont2(_) | Body::DefineFont4(_) => Some(format!("{CORE_PACKAGE}.FontAsset")),
                Body::DefineText(_) => Some(format!("{CORE_PACKAGE}.TextFieldAsset")),
                Body::DefineSound(_) => Some(format!("{CORE_PACKAGE}.SoundAsset")),
                Body::DefineBits(_) => Some(format!("{CORE_PACKAGE}.BitmapAsset")),
                Body::DefineSprite(_) => Some(skin_sprite.clone()),
                _ => None,
            }
        };
        if let Some(cls) = cls {
            if define_tag.is_none_or(|d| d.is_assignable_from(tag)) {
                let frames = tag.borrow().sprite().map_or(0, |s| s.framecount);
                if frames > 1 && cls == skin_sprite {
                    return Some(format!("{CORE_PACKAGE}.MovieClipAsset"));
                }
                return Some(cls);
            }
        }
        if define_tag.is_none() {
            Some(skin_sprite)
        } else {
            None
        }
    }
}

/// `getCacheKey(args)`: the `TreeMap`'s `hashCode`, the sum of `key.hashCode() ^ value.hashCode()`.
pub fn get_cache_key(args: &Args) -> String {
    let mut h: i32 = 0;
    for (k, v) in args.iter() {
        h = h.wrapping_add(k.java_hash() ^ v.java_hash());
    }
    h.to_string()
}

/// `resolve(context, path)`.
pub fn resolve(context: &PathResolver, path: &str) -> Result<VirtualFile, TranscoderException> {
    let p = path.strip_prefix("file:").unwrap_or(path);
    match context.resolve(p) {
        Some(f) => Ok(f),
        None => {
            if p.contains("://") {
                return Err(TranscoderException::network_transcoding_source(path));
            }
            Err(TranscoderException::unable_to_resolve(path))
        }
    }
}

/// `resolveSource(context, args)`.
pub fn resolve_source(context: &PathResolver, args: &Args) -> Result<VirtualFile, TranscoderException> {
    if let Some(rs) = args.get(transcoder::RESOLVED_SOURCE) {
        if let Some(f) = transcoder::get_resolved_path(rs) {
            return Ok(f);
        }
    }
    match args.get(transcoder::SOURCE) {
        None => Err(TranscoderException::missing_source()),
        Some(source) => resolve(context, source),
    }
}

pub fn new_results(r: TranscodingResults) -> ResultsRef {
    Rc::new(RefCell::new(r))
}

/// `generateSource(asset, fullClassName, embedMap, embedProps)` with `baseClassName` already looked up.
pub fn generate_source(asset: &ResultsRef, base_class_name: Option<String>, full_class_name: &str, embed_map: &mut Args, embed_props: &[(String, String)]) -> Result<(), TranscoderException> {
    let (package_name, class_name) = match full_class_name.rfind('.') {
        Some(dot) => (&full_class_name[..dot], &full_class_name[dot + 1..]),
        None => ("", full_class_name),
    };
    if let Some(src) = asset.borrow().asset_source.clone() {
        let path = src.get_name().replace('\\', "/");
        embed_map.insert(transcoder::RESOLVED_SOURCE.to_string(), path.clone());
        transcoder::add_resolved_path(&path, &src);
    }
    let map: Vec<(String, String)> = embed_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let ctx = EmbedClassContext {
        package_name: package_name.to_string(),
        base_class: base_class_name,
        asset_type: (!embed_props.is_empty()).then(|| ASSET_TYPE.to_string()),
        embed_class: class_name.to_string(),
        embed_map: map,
        embed_props: embed_props.to_vec(),
    };
    let code = velocity_boundary::merge_embed_class(&ctx).ok_or_else(|| TranscoderException::unable_to_generate_source(full_class_name))?;
    asset.borrow_mut().generated_code = Some(code);
    Ok(())
}

/// The `SymbolTable` a transcoder gets; most ignore it.
pub type SymbolTableRef<'a> = Option<&'a SymbolTable>;
