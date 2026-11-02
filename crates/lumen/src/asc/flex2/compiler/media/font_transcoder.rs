//! Port of `flex2.compiler.media.FontTranscoder` and `FontsConfiguration.getTopLevelManager`: fonts as
//! DefineFont3/DefineFont4 tags behind a `FontAsset`.

use std::rc::Rc;

use super::abstract_transcoder::{self, new_results, resolve_source, AbstractTranscoder, FONTFAMILY, FONTNAME, FONTSTYLE, FONTWEIGHT};
use super::mime;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::fonts_configuration::{compatibility_version, compatibility_version_string, FontsConfiguration, VERSION_4_0};
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, Transcoder, TranscoderException, TranscodingResults};
use crate::asc::flex2::flash::swf::tags::{codes, TagRef};
use crate::asc::flex2::fonts::font_description::{FontDescription, FontSource, JUrl};
use crate::asc::flex2::fonts::font_face::{BOLD, ITALIC, PLAIN};
use crate::asc::flex2::fonts::font_manager::{self, FontManager, InitMap};

pub const UNICODERANGE: &str = "unicodeRange";
pub const SYSTEMFONT: &str = "systemFont";
pub const SOURCELIST: &str = "sourceList";
pub const FLASHTYPE: &str = "flashType";
pub const ADVANTIALIASING: &str = "advancedAntiAliasing";
pub const EMBEDASCFF: &str = "embedAsCFF";

const MIME_TYPES: &[&str] = &[mime::TTF, mime::OTF, mime::FONT, mime::TTC, mime::DFONT];

pub struct FontTranscoder {
    base: AbstractTranscoder,
    inner: Inner,
}

struct Inner {
    fonts_config: FontsConfiguration,
    compatibility_version: i32,
    compatibility_version_string: Option<String>,
    show_shadowed_device_font_warnings: bool,
    /// `FontsConfiguration.topLevelManager`, made on first use.
    top_level_manager: Option<Option<Rc<dyn FontManager>>>,
}

/// `new FontTranscoder(cfg)`, for `media::FONT_TRANSCODER`.
pub fn factory(cfg: &Configuration) -> Box<dyn Transcoder> {
    Box::new(FontTranscoder::new(cfg))
}

/// Registers the transcoder in `WebTierAPI.getTranscoders`.
pub fn register() {
    let _ = super::FONT_TRANSCODER.set(factory);
}

impl FontTranscoder {
    pub fn new(cfg: &Configuration) -> FontTranscoder {
        let cv = cfg.css.compatibility_version;
        FontTranscoder {
            base: AbstractTranscoder::new(MIME_TYPES, None, true, "flex2.compiler.media.FontTranscoder"),
            inner: Inner {
                fonts_config: cfg.flex.fonts.clone(),
                compatibility_version: compatibility_version(cv),
                compatibility_version_string: compatibility_version_string(cv),
                show_shadowed_device_font_warnings: cfg.flex.show_shadowed_device_font_warnings,
                top_level_manager: None,
            },
        }
    }
}

pub fn is_supported_attribute(attr: &str) -> bool {
    [FONTNAME, FONTSTYLE, FONTWEIGHT, FONTFAMILY, UNICODERANGE, FLASHTYPE, ADVANTIALIASING, SYSTEMFONT, SOURCELIST, EMBEDASCFF].contains(&attr)
}

fn bad_parameters() -> TranscoderException {
    TranscoderException::new("font transcoding requires you to specify 'fontName' and one of 'source', 'systemFont', or 'sourceList'")
}

/// The path `FontTranscoder` reports its own messages at: `_file`, with `\` when `_pathsep` is set.
fn report_path(args: &Args) -> Option<String> {
    let path = args.get(transcoder::FILE).cloned();
    if args.get(transcoder::PATHSEP).map(String::as_str) == Some("true") {
        path.map(|p| p.replace('/', "\\"))
    } else {
        path
    }
}

/// `Integer.parseInt(args.get(_line))`; a bad number is a Java runtime exception.
fn report_line(args: &Args) -> Result<i32, TranscoderException> {
    match args.get(transcoder::LINE) {
        None => Ok(-1),
        Some(l) => font_manager::java_parse_int(l, 10).ok_or_else(|| TranscoderException { runtime: true, ..TranscoderException::new(format!("For input string: \"{l}\"")) }),
    }
}

fn log_at(m: CompilerMessage, args: &Args) -> Result<(), TranscoderException> {
    let line = report_line(args)?;
    let path = report_path(args);
    toolkit::log(m, path.as_deref(), line, -1, None);
    Ok(())
}

impl Inner {
    fn get_top_level_manager(&mut self) -> Option<Rc<dyn FontManager>> {
        if self.top_level_manager.is_none() {
            let c = &self.fonts_config;
            let map = InitMap {
                max_cached_fonts: c.max_cached_fonts.clone(),
                max_glyphs_per_face: c.max_glyphs_per_face.clone(),
                compatibility_version: self.compatibility_version_string.clone(),
                local_fonts_snapshot: c.local_fonts_snapshot.clone(),
                local_font_paths: c.local_font_paths.clone(),
            };
            let languages = Rc::new(c.languages.clone());
            self.top_level_manager = Some(font_manager::create(c.managers.as_deref(), Some(Rc::new(map)), Some(languages)));
        }
        self.top_level_manager.clone().unwrap()
    }

    fn do_transcode(&mut self, context: &PathResolver, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let mut results = TranscodingResults::default();
        let mut system_font = None;
        let locations: Vec<FontSource>;
        if args.contains_key(transcoder::SOURCE) {
            if args.contains_key(SYSTEMFONT) || args.contains_key(SOURCELIST) {
                return Err(bad_parameters());
            }
            let src = resolve_source(context, args)?;
            results.modified = src.get_last_modified();
            locations = vec![FontSource::Url(get_url(&src)?)];
            results.asset_source = Some(src);
        } else if args.contains_key(SYSTEMFONT) {
            if args.contains_key(transcoder::SOURCE) || args.contains_key(SOURCELIST) {
                return Err(bad_parameters());
            }
            let s = args.get(SYSTEMFONT).cloned().unwrap();
            system_font = Some(s.clone());
            locations = vec![FontSource::Name(s)];
        } else if args.contains_key(SOURCELIST) {
            return Err(TranscoderException {
                runtime: true,
                ..TranscoderException::new("class java.lang.String cannot be cast to class java.util.List (java.lang.String and java.util.List are in module java.base of loader 'bootstrap')")
            });
        } else {
            return Err(bad_parameters());
        }
        let family = args.get(FONTFAMILY).cloned();
        let mut alias = args.get(FONTNAME).cloned();
        if alias.is_none() {
            alias = system_font.clone();
        }
        if alias.is_none() {
            alias = family;
        }
        let Some(alias) = alias else { return Err(bad_parameters()) };
        if system_font.as_deref() == Some(alias.as_str()) && self.show_shadowed_device_font_warnings {
            log_at(CompilerMessage::warning(format!("the embedded font '{alias}' may shadow a device font of the same name.  Use fontName to alias the font to a different name")), args)?;
        }
        let mut font_desc = FontDescription {
            alias: Some(alias),
            source: None,
            style: get_font_style(args),
            unicode_ranges: args.get(UNICODERANGE).cloned(),
            advanced_anti_aliasing: self.use_advanced_anti_aliasing(args)?,
            compact_font_format: use_compact_font_format(args, self.compatibility_version),
        };
        let define_font = self.get_define_font(&mut font_desc, locations, args)?;
        results.define_tag = Some(define_font);
        let results = new_results(results);
        if generate_source {
            let base = abstract_transcoder::associated_class(None, results.borrow().define_tag.as_ref());
            abstract_transcoder::generate_source(&results, base, class_name, args, &[])?;
        }
        Ok(results)
    }

    fn get_define_font(&mut self, font_desc: &mut FontDescription, locations: Vec<FontSource>, args: &Args) -> Result<TagRef, TranscoderException> {
        let font_manager = self.get_top_level_manager();
        let mut define_font = None;
        for source in locations {
            let r = match &font_manager {
                None => Err(font_manager::JavaException::new("flex2.compiler.media.FontTranscoder$NoFontManagerException", Some("No FontManager provided. Cannot build font.".into()))),
                Some(fm) => {
                    font_desc.source = Some(source);
                    fm.create_define_font(codes::DEFINE_FONT3, font_desc)
                }
            };
            match r {
                Ok(t) => define_font = t,
                Err(e) if e.is_invalid_unicode_range() => return Err(TranscoderException::new(format!("invalid Unicode range '{}'", e.range.as_deref().unwrap_or("null")))),
                Err(e) => log_at(CompilerMessage::error(format!("exception during transcoding: {}", e.get_message().unwrap_or("null"))), args)?,
            }
            if let Some(t) = define_font {
                return Ok(t);
            }
        }
        Err(TranscoderException::new(format!("unable to build font '{}'", font_desc.alias.as_deref().unwrap_or("null"))))
    }

    /// `useAdvancedAntiAliasing(args)`.
    fn use_advanced_anti_aliasing(&self, args: &Args) -> Result<bool, TranscoderException> {
        let mut flash_type_as_name = true;
        let mut s = args.get(ADVANTIALIASING).cloned();
        if s.is_none() {
            s = args.get(FLASHTYPE).cloned();
        } else {
            flash_type_as_name = false;
        }
        match s {
            Some(v) if v.eq_ignore_ascii_case("true") => Ok(true),
            Some(v) if v.eq_ignore_ascii_case("false") => Ok(false),
            Some(_) if flash_type_as_name => Err(TranscoderException::new("flashType must be true or false.")),
            Some(_) => Err(TranscoderException::new("advancedAntiAliasing must be true or false.")),
            None => Ok(self.fonts_config.flash_type),
        }
    }
}

/// `getURL(virtualFile)`: a local file's `File.toURL()`, or a temp copy of any other file.
fn get_url(f: &VirtualFile) -> Result<JUrl, TranscoderException> {
    match f {
        VirtualFile::Local { name, .. } => {
            let p = transcoder::raw_path(name).unwrap_or_else(|| std::path::PathBuf::from(name));
            Ok(JUrl::from_path(&p))
        }
        _ => {
            let name = f.get_name();
            let path = &name[name.find('$').map_or(0, |i| i + 1)..];
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
            let tmp = std::env::temp_dir().join(format!("{}{}.tmp", path.replace('/', "_"), nonce));
            let bytes = f.to_byte_array().map_err(|_| TranscoderException::new(format!("unable to extract font '{name}'")))?;
            std::fs::write(&tmp, bytes).map_err(|_| TranscoderException::new(format!("unable to extract font '{name}'")))?;
            Ok(JUrl::from_path(&tmp))
        }
    }
}

/// `useCompactFontFormat(args, compatibilityVersion)`.
fn use_compact_font_format(args: &Args, compatibility_version: i32) -> bool {
    let mut use_cff = compatibility_version >= VERSION_4_0;
    if let Some(v) = args.get(EMBEDASCFF) {
        use_cff = font_manager::java_trim(v).eq_ignore_ascii_case("true");
    }
    use_cff
}

pub fn get_font_style(args: &Args) -> i32 {
    let mut s = PLAIN;
    let style = args.get(FONTSTYLE).map_or("normal", String::as_str);
    let weight = args.get(FONTWEIGHT).map_or("normal", String::as_str);
    if is_bold(Some(weight)) {
        s += BOLD;
    }
    if is_italic(Some(style)) {
        s += ITALIC;
    }
    s
}

pub fn is_bold(value: Option<&str>) -> bool {
    let Some(v) = value else { return false };
    let b = font_manager::java_trim(v).to_lowercase();
    if b.starts_with("bold") {
        return true;
    }
    font_manager::java_parse_int(&b, 10).is_some_and(|w| w >= 700)
}

pub fn is_italic(value: Option<&str>) -> bool {
    let Some(v) = value else { return false };
    let i = font_manager::java_trim(v).to_lowercase();
    i == "italic" || i == "oblique"
}

impl Transcoder for FontTranscoder {
    fn is_supported(&self, mime_type: &str) -> bool {
        self.base.is_supported(mime_type)
    }

    fn transcode(&mut self, context: &PathResolver, _symbol_table: Option<&SymbolTable>, args: &mut Args, class_name: &str, generate_source: bool) -> Result<ResultsRef, TranscoderException> {
        let inner = &mut self.inner;
        self.base.transcode(args, &is_supported_attribute, &mut |args| inner.do_transcode(context, args, class_name, generate_source))
    }

    fn get_associated_class(&self, tag: Option<&TagRef>) -> Option<String> {
        abstract_transcoder::associated_class(None, tag)
    }

    fn clear(&mut self) {
        self.base.clear();
    }
}
