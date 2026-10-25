//! Port of `flex2.compiler.media.MovieTranscoder`: a symbol or font taken from an existing SWF (with its AS2 code
//! purged), or a whole SWF as a `MovieClipLoaderAsset` over its bytes.

use std::collections::HashMap;

use super::abstract_transcoder::{self, new_results, resolve_source, AbstractTranscoder, ASSET_TYPE, FONTFAMILY, FONTNAME, FONTSTYLE, FONTWEIGHT, SCALE9BOTTOM, SCALE9LEFT, SCALE9RIGHT, SCALE9TOP};
use super::image_transcoder::scaling_grid_exception;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults, CORE_PACKAGE};
use crate::asc::flex2::flash::swf::dictionary::Dictionary;
use crate::asc::flex2::flash::swf::tag_decoder;
use crate::asc::flex2::flash::swf::tags::{codes, copy_sprite, new_place_object, new_sprite, tag_name, Body, DefineScalingGrid, Tag, TagRef};
use crate::asc::flex2::flash::swf::types::Rect;
use crate::asc::util::jhash::JHashMap;

pub const SYMBOL: &str = "symbol";

pub struct MovieTranscoder {
    base: AbstractTranscoder,
    dictionary_map: HashMap<String, std::rc::Rc<Dictionary>>,
    embed_props: JHashMap<String, String>,
}

impl Default for MovieTranscoder {
    fn default() -> Self {
        Self::new()
    }
}

/// `FontTranscoder.isBold`.
pub fn is_bold(value: Option<&str>) -> bool {
    let Some(v) = value else { return false };
    let b = v.trim().to_lowercase();
    b.starts_with("bold") || b.parse::<i32>().is_ok_and(|w| w >= 700)
}

/// `FontTranscoder.isItalic`.
pub fn is_italic(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        let i = v.trim().to_lowercase();
        i == "italic" || i == "oblique"
    })
}

fn path_line(args: &Args) -> (Option<String>, i32) {
    let path = args.get(transcoder::FILE).map(|p| if args.get(transcoder::PATHSEP).map(String::as_str) == Some("true") { p.replace('/', "\\") } else { p.clone() });
    let line = args.get(transcoder::LINE).and_then(|l| l.parse::<i32>().ok()).unwrap_or(-1);
    (path, line)
}

/// `defineScalingGrid(sprite, args)`.
pub fn define_scaling_grid(sprite: &TagRef, args: &Args) -> Result<(), TranscoderException> {
    let (Some(left), Some(right), Some(top), Some(bottom)) = (args.get(SCALE9LEFT), args.get(SCALE9RIGHT), args.get(SCALE9TOP), args.get(SCALE9BOTTOM)) else {
        return Err(scaling_grid_exception());
    };
    if sprite.borrow().sprite().unwrap().scaling_grid.is_some() {
        let (path, line) = path_line(args);
        toolkit::log(CompilerMessage::warning("SWF asset already has a scaling grid"), path.as_deref(), line, -1, None);
    }
    let parse = |s: &String| -> Result<i32, TranscoderException> {
        s.parse::<i32>().map_err(|_| TranscoderException { runtime: true, ..TranscoderException::new(format!("For input string: \"{s}\"")) })
    };
    let rect = Rect { x_min: parse(left)?.wrapping_mul(20), x_max: parse(right)?.wrapping_mul(20), y_min: parse(top)?.wrapping_mul(20), y_max: parse(bottom)?.wrapping_mul(20) };
    let grid = Tag::new_ref(codes::DEFINE_SCALING_GRID, Body::DefineScalingGrid(DefineScalingGrid { scaling_target: Some(std::rc::Rc::downgrade(sprite)), rect }));
    sprite.borrow_mut().sprite_mut().unwrap().scaling_grid = Some(grid);
    let range = || TranscoderException::new("scaling grid values must be greater than zero, the left value must be less than the right value, and the top value must be less than the bottom value");
    if rect.x_min < 0 || rect.x_max < 0 || rect.x_min >= rect.x_max {
        return Err(range());
    }
    if rect.y_min < 0 || rect.y_max < 0 || rect.y_min >= rect.y_max {
        return Err(range());
    }
    Ok(())
}

impl MovieTranscoder {
    pub fn new() -> MovieTranscoder {
        MovieTranscoder {
            base: AbstractTranscoder::new(&[super::mime::FLASH], None, false, "flex2.compiler.media.MovieTranscoder"),
            dictionary_map: HashMap::new(),
            embed_props: JHashMap::new(),
        }
    }

    fn is_supported_attribute(attr: &str) -> bool {
        attr == SYMBOL || attr == SCALE9TOP || attr == SCALE9LEFT || attr == SCALE9BOTTOM || attr == SCALE9RIGHT || attr == FONTNAME || attr == FONTFAMILY || attr == FONTWEIGHT || attr == FONTSTYLE
    }

    fn do_transcode(&mut self, context: &PathResolver, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let results = new_results(TranscodingResults::with_source(resolve_source(context, args)?));
        let symbol_name = args.get(SYMBOL).cloned();
        let font_family = args.get(FONTFAMILY).cloned();
        let font_weight = args.get(FONTWEIGHT).cloned();
        let font_style = args.get(FONTSTYLE).cloned();
        let font_name = args.get(FONTNAME).cloned().or(font_family);
        if let Some(font_name) = font_name {
            if symbol_name.is_some() {
                return Err(TranscoderException::incompatible_transcoder_parameters(SYMBOL, FONTNAME));
            }
            self.extract_define_font(&results, &font_name, is_bold(font_weight.as_deref()), is_italic(font_style.as_deref()))?;
            if generate_source {
                self.generate_source(&results, class_name, args)?;
            }
        } else if let Some(symbol_name) = symbol_name {
            if font_weight.is_some() {
                return Err(TranscoderException::incompatible_transcoder_parameters(SYMBOL, FONTWEIGHT));
            }
            if font_style.is_some() {
                return Err(TranscoderException::incompatible_transcoder_parameters(SYMBOL, FONTSTYLE));
            }
            self.extract_define_tag(&results, &symbol_name, args)?;
            if generate_source {
                self.generate_source(&results, class_name, args)?;
            }
        } else {
            results.borrow_mut().define_tag = Some(new_sprite(None));
            if generate_source {
                let size = self.get_swf_size(&results)?;
                generate_top_level_loader_source(&results, class_name, size);
            } else {
                let name = results.borrow().asset_source.as_ref().unwrap().get_name().to_string();
                return Err(TranscoderException::embed_requires_codegen(&name, class_name));
            }
        }
        if args.contains_key(SCALE9LEFT) || args.contains_key(SCALE9RIGHT) || args.contains_key(SCALE9TOP) || args.contains_key(SCALE9BOTTOM) {
            let tag = results.borrow().define_tag.clone().unwrap();
            if tag.borrow().sprite().is_none() {
                let code = tag.borrow().code;
                return Err(TranscoderException::new(format!("cannot apply scaling grid to resource type '{}'", tag_name(code))));
            }
            let copy = copy_sprite(&tag);
            define_scaling_grid(&copy, args)?;
            results.borrow_mut().define_tag = Some(copy);
        }
        Ok(results)
    }

    fn generate_source(&self, results: &ResultsRef, class_name: &str, args: &mut Args) -> Result<(), TranscoderException> {
        let base_class = self.base.get_associated_class(results.borrow().define_tag.as_ref());
        let props: Vec<(String, String)> = self.embed_props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        abstract_transcoder::generate_source(results, base_class, class_name, args, &props)
    }

    /// `getSwfSize(assetSource)`: the header's frame size.
    fn get_swf_size(&self, results: &ResultsRef) -> Result<Rect, TranscoderException> {
        let src = results.borrow().asset_source.clone().unwrap();
        let bytes = src.to_byte_array().map_err(|e| TranscoderException::exception_while_transcoding(&e.to_string()))?;
        tag_decoder::decode_header(&bytes).map(|h| h.size).map_err(|e| TranscoderException::exception_while_transcoding(&e))
    }

    /// `getDictionary(results, assetSource)`, decoded once per file.
    fn get_dictionary(&mut self, results: &ResultsRef) -> Result<std::rc::Rc<Dictionary>, TranscoderException> {
        let src = results.borrow().asset_source.clone().unwrap();
        if let Some(d) = self.dictionary_map.get(src.get_name()) {
            return Ok(d.clone());
        }
        let bytes = src.to_byte_array().map_err(|e| TranscoderException::exception_while_transcoding(&e.to_string()))?;
        let dict = std::rc::Rc::new(tag_decoder::decode_dictionary(&bytes).map_err(|e| TranscoderException::exception_while_transcoding(&e))?);
        self.dictionary_map.insert(src.get_name().to_string(), dict.clone());
        Ok(dict)
    }

    pub fn extract_define_font(&mut self, results: &ResultsRef, font_name: &str, bold: bool, italic: bool) -> Result<(), TranscoderException> {
        let dict = self.get_dictionary(results)?;
        match dict.get_font_face(font_name, bold, italic) {
            Some(font) => {
                results.borrow_mut().define_tag = Some(font);
                Ok(())
            }
            None => Err(TranscoderException::new(format!(
                "font '{font_name}' with {} weight and {} style not found",
                if bold { "bold" } else { "normal" },
                if italic { "italic" } else { "regular" }
            ))),
        }
    }

    pub fn extract_define_tag(&mut self, results: &ResultsRef, symbol_name: &str, args: &Args) -> Result<(), TranscoderException> {
        let dict = self.get_dictionary(results)?;
        let Some(definition) = dict.get_tag_by_name(symbol_name) else {
            return Err(TranscoderException::new(format!("definition for symbol '{symbol_name}' not found")));
        };
        if self.purge(&definition) {
            let (path, line) = path_line(args);
            toolkit::log(CompilerMessage::warning(format!("ignoring AS2 code within '{symbol_name}'; Flex 2 applications only support AS3")), path.as_deref(), line, -1, None);
        }
        results.borrow_mut().define_tag = Some(definition.clone());
        if matches!(definition.borrow().body, Body::DefineText(_)) {
            let s = new_sprite(None);
            let po = Tag::new_ref(codes::PLACE_OBJECT2, Body::PlaceObject(new_place_object(definition.clone(), 0)));
            s.borrow_mut().sprite_mut().unwrap().tag_list.push(po);
            results.borrow_mut().define_tag = Some(s);
        }
        Ok(())
    }

    /// `purge(tag)`: drops AS2 code from a sprite tree or button, noting named children as embed properties.
    fn purge(&mut self, tag: &TagRef) -> bool {
        let mut ret = false;
        let is_sprite = tag.borrow().sprite().is_some();
        if is_sprite {
            if tag.borrow().sprite().unwrap().init_action.is_some() {
                ret = true;
                tag.borrow_mut().sprite_mut().unwrap().init_action = None;
            }
            let children: Vec<TagRef> = tag.borrow().sprite().unwrap().tag_list.clone();
            let mut keep = Vec::with_capacity(children.len());
            for t in children {
                let is_action = matches!(t.borrow().body, Body::DoAction { .. } | Body::DoInitAction { .. });
                if is_action {
                    ret = true;
                } else {
                    keep.push(t.clone());
                }
                let place = match &t.borrow().body {
                    Body::PlaceObject(p) => Some((p.has_clip_action(), p.has_char_id(), p.ref_.clone(), p.name.clone())),
                    _ => None,
                };
                if let Some((clip, has_char, r, name)) = place {
                    if clip {
                        if let Body::PlaceObject(p) = &mut t.borrow_mut().body {
                            p.set_clip_actions(None);
                        }
                        ret = true;
                    }
                    if has_char {
                        if let Some(r) = r {
                            ret |= self.purge(&r);
                        }
                    }
                    if let Some(n) = name {
                        self.embed_props.insert(n, ASSET_TYPE.to_string());
                    }
                }
            }
            tag.borrow_mut().sprite_mut().unwrap().tag_list = keep;
        } else if let Body::DefineButton(b) = &mut tag.borrow_mut().body {
            if !b.cond_actions.is_empty() {
                ret = true;
                b.cond_actions.clear();
            }
        }
        ret
    }
}

/// `generateTopLevelLoaderSource(asset, fullClassName, embedMap, size)`.
fn generate_top_level_loader_source(asset: &ResultsRef, full_class_name: &str, size: Rect) {
    let (package_name, class_name) = match full_class_name.rfind('.') {
        Some(dot) => (&full_class_name[..dot], &full_class_name[dot + 1..]),
        None => ("", full_class_name),
    };
    let src = asset.borrow().asset_source.clone().unwrap();
    let resolved_source = src.get_name().replace('\\', "/");
    transcoder::add_resolved_path(&resolved_source, &src);
    let mut s = String::with_capacity(1024);
    s.push_str("package ");
    s.push_str(package_name);
    s.push_str(&format!("\n{{\nimport {CORE_PACKAGE}.MovieClipLoaderAsset;\n"));
    s.push_str("import flash.utils.ByteArray;\n\n");
    s.push_str("public class ");
    s.push_str(class_name);
    s.push_str(" extends MovieClipLoaderAsset\n{\n");
    s.push_str("\tpublic function ");
    s.push_str(class_name);
    s.push_str("()\n\t{\n\t\tsuper();\n");
    s.push_str("\t\tinitialWidth=");
    s.push_str(&size.get_width().to_string());
    s.push_str("/20;\n\t\tinitialHeight=");
    s.push_str(&size.get_height().to_string());
    s.push_str("/20;\n\t}\n");
    s.push_str("\tprivate static var bytes:ByteArray = null;\n\n");
    s.push_str("\toverride public function get movieClipData():ByteArray\n\t{\n");
    s.push_str("\t\tif (bytes == null)\n\t\t{\n\t\t\tbytes = ByteArray( new dataClass() );\n\t\t}\n");
    s.push_str("\t\treturn bytes;\n\t}\n\n");
    s.push_str(&format!("\t[Embed({}='", transcoder::RESOLVED_SOURCE));
    s.push_str(&resolved_source);
    s.push_str("', mimeType='application/octet-stream')]\n\tpublic var dataClass:Class;\n");
    s.push_str("}\n}\n");
    asset.borrow_mut().generated_code = Some(s);
}

impl Transcoder for MovieTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let mut base = std::mem::replace(&mut self.base, AbstractTranscoder::new(&[], None, false, ""));
        let r = base.transcode(args, &Self::is_supported_attribute, &mut |args| self.do_transcode(context, args, class_name, generate_source));
        self.base = base;
        r
    }

    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        self.base.get_associated_class(tag)
    }

    fn clear(&mut self) {
        self.base.clear();
        if !self.dictionary_map.is_empty() {
            self.dictionary_map = HashMap::new();
        }
        if !self.embed_props.is_empty() {
            self.embed_props = JHashMap::new();
        }
    }
}
