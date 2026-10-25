//! Port of the parts of `flex2.compiler.mxml.rep.AtEmbed` (and `flex2.compiler.as3.MetaDataParser`) that CSS uses:
//! `Embed(...)` style values and `@font-face` rules, resolved the way `ThreadLocalToolkit`'s path resolver does while
//! a style sheet is processed. MXML's `@Embed` constructors are left to the MXML port.

use std::cell::RefCell;
use std::rc::Rc;

use super::css_file::{self, CssFile};
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::parser::evaluator::evaluate;
use crate::asc::parser::meta_data_evaluator::MetaDataEvaluator;
use crate::asc::parser::parser::Parser;
use crate::asc::semantics::value::Value;
use crate::asc::util::context::{CompilerHandler, Context, ContextStatics};
use crate::asc::util::jhash::{JHashMap, JavaHash};
use crate::asc::util::jstr;

/// A `@font-face` `src` entry: `flash.css.LocalSource` or `flash.css.URLSource`.
#[derive(Clone, Debug)]
pub enum FontSource {
    Local(String),
    Url(String),
}

/// A value of an Embed parameter map (`Map<String, Object>`).
#[derive(Clone, Debug)]
pub enum EmbedValue {
    Str(String),
    /// The `sourceList` of a multi-source `@font-face`; Java prints and hashes its elements by identity.
    Sources(Vec<FontSource>),
}

impl EmbedValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            EmbedValue::Str(s) => Some(s),
            EmbedValue::Sources(_) => None,
        }
    }
}

impl std::fmt::Display for EmbedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbedValue::Str(s) => f.write_str(s),
            EmbedValue::Sources(v) => {
                let parts: Vec<&str> = v.iter().map(|s| if matches!(s, FontSource::Local(_)) { "flash.css.LocalSource@0" } else { "flash.css.URLSource@0" }).collect();
                write!(f, "[{}]", parts.join(", "))
            }
        }
    }
}

impl JavaHash for EmbedValue {
    fn java_hash(&self) -> i32 {
        match self {
            EmbedValue::Str(s) => s.java_hash(),
            EmbedValue::Sources(v) => v.iter().fold(1i32, |h, _| h.wrapping_mul(31)),
        }
    }
}

pub type EmbedParams = JHashMap<String, EmbedValue>;

/// `AbstractMap.hashCode()`.
pub fn map_hash_code(m: &EmbedParams) -> i32 {
    m.iter().fold(0i32, |h, (k, v)| h.wrapping_add(k.java_hash() ^ v.java_hash()))
}

pub struct AtEmbed {
    prop_name: String,
    line_number: i32,
    attributes: RefCell<EmbedParams>,
    str_type: bool,
}

impl PartialEq for AtEmbed {
    fn eq(&self, other: &AtEmbed) -> bool {
        self.prop_name == other.prop_name
    }
}

impl Eq for AtEmbed {}

impl JavaHash for AtEmbed {
    fn java_hash(&self) -> i32 {
        self.prop_name.java_hash()
    }
}

/// An `AtEmbed` in a `HashSet<AtEmbed>`.
#[derive(Clone)]
pub struct AtEmbedKey(pub Rc<AtEmbed>);

impl PartialEq for AtEmbedKey {
    fn eq(&self, other: &AtEmbedKey) -> bool {
        self.0 == other.0
    }
}

impl Eq for AtEmbedKey {}

impl JavaHash for AtEmbedKey {
    fn java_hash(&self) -> i32 {
        self.0.java_hash()
    }
}

fn invalid_embed(source: &str, path: &str, line: i32) {
    toolkit::log(CompilerMessage::error(format!("Invalid Embed directive in stylesheet - can't resolve source '{source}'.")), Some(path), line, -1, None);
}

fn skin_class_with_source_not_supported(_source: &str, path: &str, line: i32) {
    toolkit::log(CompilerMessage::error("Source attribute not supported with skinClass."), Some(path), line, -1, None);
}

/// `StringUtils.replaceAll(name + '_' + Math.abs(hash), "[^A-Za-z0-9]", "_")`.
pub fn create_mangled_name(name: &str, hash: i32) -> String {
    format!("{name}_{}", hash.wrapping_abs()).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

/// The keyed and keyless values of a parsed `[Embed(...)]`, as `flex2.compiler.as3.reflect.MetaData` gives them.
pub struct ParsedMetaData {
    values: Vec<(Option<String>, String)>,
}

impl ParsedMetaData {
    pub fn count(&self) -> i32 {
        self.values.len() as i32
    }

    pub fn get_key(&self, index: i32) -> Option<&str> {
        self.values[index as usize].0.as_deref()
    }

    pub fn get_value(&self, index: i32) -> &str {
        &self.values[index as usize].1
    }

    pub fn get_value_key(&self, key: &str) -> Option<&str> {
        self.values.iter().find(|(k, _)| k.as_deref() == Some(key)).map(|(_, v)| v.as_str())
    }

    /// `getValueMap()`: a `HashMap` of the keyed values.
    pub fn get_value_map(&self) -> JHashMap<String, String> {
        let mut m = JHashMap::new();
        for (k, v) in &self.values {
            if let Some(k) = k {
                m.insert(k.clone(), v.clone());
            }
        }
        m
    }
}

struct MetaDataHandler {
    begin_line: i32,
}

impl CompilerHandler for MetaDataHandler {
    fn error(&mut self, filename: &str, _ln: i32, _col: i32, _msg: &str, _source: &str, _code: i32) {
        toolkit::log(CompilerMessage::error("Invalid metadata format."), Some(filename), self.begin_line, -1, None);
    }
    fn warning(&mut self, filename: &str, _ln: i32, _col: i32, _msg: &str, _source: &str, _code: i32) {
        toolkit::log(CompilerMessage::warning("Invalid metadata format."), Some(filename), self.begin_line, -1, None);
    }
    fn error_no_code(&mut self, filename: &str, _ln: i32, _col: i32, _msg: &str, _source: &str) {
        toolkit::log(CompilerMessage::error("Invalid metadata format."), Some(filename), self.begin_line, -1, None);
    }
    fn warning_no_code(&mut self, filename: &str, _ln: i32, _col: i32, _msg: &str, _source: &str) {
        toolkit::log(CompilerMessage::warning("Invalid metadata format."), Some(filename), self.begin_line, -1, None);
    }
}

/// `MetaDataParser.parse(perCompileData, file, beginLine, md)`.
pub fn parse_meta_data(per_compile_data: &Rc<RefCell<ContextStatics>>, file: &SourceRef, begin_line: i32, md: &str) -> Option<ParsedMetaData> {
    let text = format!("[{md}]");
    let cx = Rc::new(RefCell::new(Context::new(per_compile_data.clone())));
    let (parent, name) = {
        let f = file.borrow();
        (f.get_parent(), f.get_name().to_string())
    };
    {
        let c = cx.borrow();
        c.set_path(parent);
        c.set_script_name(Some(name.clone()));
    }
    let handler: Rc<RefCell<dyn CompilerHandler>> = Rc::new(RefCell::new(MetaDataHandler { begin_line }));
    cx.borrow_mut().set_handler(handler);
    per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
    let node = Parser::new(&cx, &text, &name).parse_meta_data();
    evaluate(&node, &cx, &mut MetaDataEvaluator::new());
    if cx.borrow().error_count() > 0 {
        return None;
    }
    let n = node.borrow();
    let md = n.as_meta_data()?;
    let values = md
        .get_values()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| match v {
            Value::KeylessValue(k) => Some((None, k.obj.clone())),
            Value::KeyValuePair(k) => Some((Some(k.key.clone()), k.obj.clone())),
            _ => None,
        })
        .collect();
    Some(ParsedMetaData { values })
}

impl AtEmbed {
    pub fn new(prop_name: String, line_number: i32, attrs: EmbedParams, str_type: bool) -> AtEmbed {
        AtEmbed { prop_name, line_number, attributes: RefCell::new(attrs), str_type }
    }

    fn add_file_and_line(values: &mut EmbedParams, file: &str, line: i32) {
        if file.contains('\\') {
            values.insert("_file".into(), EmbedValue::Str(file.replace('\\', "/")));
            values.insert("_pathsep".into(), EmbedValue::Str("true".into()));
        } else {
            values.insert("_file".into(), EmbedValue::Str(file.to_string()));
        }
        values.insert("_line".into(), EmbedValue::Str(line.to_string()));
    }

    pub fn get_prop_name(&self) -> &str {
        &self.prop_name
    }

    pub fn get_attributes(&self) -> std::cell::Ref<'_, EmbedParams> {
        self.attributes.borrow()
    }

    pub fn get_xml_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_type(&self) -> &'static str {
        if self.str_type {
            "String"
        } else {
            "Class"
        }
    }

    fn create_resolved(per_compile_data: &Rc<RefCell<ContextStatics>>, source: &SourceRef, value: &str, path: &str, line: i32, prefix: &str, log_invalid_embed_on_missing_file: bool) -> Option<AtEmbed> {
        let meta_data = parse_meta_data(per_compile_data, source, line, value)?;
        let mut source_value: Option<String> = None;
        let mut values = EmbedParams::new();
        if meta_data.count() > 1 && meta_data.get_key(0).is_none() && meta_data.get_key(1).is_none() {
            source_value = Some(meta_data.get_value(0).to_string());
            let symbol_or_mime_type = meta_data.get_value(1).to_string();
            if is_mime_type(&symbol_or_mime_type) {
                values.insert("mimeType".into(), EmbedValue::Str(symbol_or_mime_type));
            } else {
                values.insert("symbol".into(), EmbedValue::Str(symbol_or_mime_type));
            }
        } else if meta_data.count() > 0 && meta_data.get_key(0).is_none() {
            source_value = Some(meta_data.get_value(0).to_string());
        } else if meta_data.get_value_key("skinClass").is_some() && meta_data.get_value_key("source").is_some() {
            skin_class_with_source_not_supported(value, path, line);
        } else if meta_data.get_value_key("skinClass").is_some() {
        } else {
            source_value = meta_data.get_value_key("source").map(str::to_string);
        }
        let resolved = match &source_value {
            Some(sv) => tokenize_and_resolve_source(sv, &mut values, source, value, path, line, log_invalid_embed_on_missing_file),
            None => false,
        };
        if resolved || meta_data.get_value_key("skinClass").is_some() {
            for (key, val) in meta_data.get_value_map().iter() {
                if !values.contains_key(key) {
                    values.insert(key.clone(), EmbedValue::Str(val.replace('\\', "/")));
                }
            }
            if prefix == "_embed_css_" {
                if let Some(EmbedValue::Str(ts)) = values.get("_resolvedSource") {
                    let h = ts.java_hash();
                    source_value = Some(format!("{}_{h}", source_value.as_deref().unwrap_or("null")));
                }
            }
            let prop_name = create_mangled_name_for(source, source_value.as_deref(), false, &values, Some(prefix));
            Self::add_file_and_line(&mut values, path, line);
            return Some(AtEmbed::new(prop_name, line, values, false));
        }
        None
    }

    /// `AtEmbed.create(perCompileData, source, value, path, line, prefix)`: an `Embed(...)` in a style sheet.
    pub fn create(per_compile_data: &Rc<RefCell<ContextStatics>>, source: &SourceRef, value: &str, path: &str, line: i32, prefix: &str) -> Option<Rc<AtEmbed>> {
        let result = Self::create_resolved(per_compile_data, source, value, path, line, prefix, true);
        if result.is_none() {
            invalid_embed(value, path, line);
        }
        result.map(Rc::new)
    }

    /// `AtEmbed.create(baseName, source, path, beginLine, attrs, strType)`: a `@font-face` rule.
    pub fn create_named(base_name: &str, source: &SourceRef, path: &str, begin_line: i32, mut attrs: EmbedParams, str_type: bool) -> Rc<AtEmbed> {
        let embed_name = create_mangled_name(base_name, map_hash_code(&attrs));
        let source_value = attrs.get("source").and_then(|v| v.as_str()).map(str::to_string);
        if let Some(sv) = source_value {
            if !tokenize_and_resolve_source(&sv, &mut attrs, source, &sv, path, begin_line, true) {
                invalid_embed(&sv, path, begin_line);
            }
        }
        Rc::new(AtEmbed::new(embed_name, begin_line, attrs, str_type))
    }
}

/// `MimeMappings.getExtension(mimeType) != null`.
fn is_mime_type(s: &str) -> bool {
    matches!(
        s,
        "text/mxml" | "text/as" | "text/fxg" | "application/x-actionscript-bytecode" | "text/css" | "text/properties" | "image/jpeg" | "image/jpg" | "image/png" | "image/gif" | "image/svg" | "audio/mpeg" | "application/x-shockwave-flash" | "text/xml" | "application/x-font-truetype" | "application/x-font-truetype-collection" | "application/x-font-opentype" | "application/x-dfont" | "application/x-pbj" | "image/tiff"
    )
}

/// `createMangledName(source, path, strType, attrs, prefix)`.
pub fn create_mangled_name_for(source: &SourceRef, path: Option<&str>, str_type: bool, attrs: &EmbedParams, prefix: Option<&str>) -> String {
    let mut buf = String::with_capacity(128);
    if let Some(p) = prefix {
        buf.push_str(p);
    }
    let mut hash = map_hash_code(attrs);
    let mime_type = attrs.get("mimeType").and_then(|v| v.as_str());
    if let Some(p) = path {
        buf.push_str(p);
        if let Some(f) = css_file::source_resolve(source, p) {
            hash ^= f.get_name().java_hash();
        }
    } else if let Some(m) = mime_type {
        buf.push_str(m);
    }
    if let Some(EmbedValue::Str(symbol)) = attrs.get("symbol") {
        buf.push('_');
        buf.push_str(symbol);
    }
    if str_type {
        buf.push_str("_s");
    }
    create_mangled_name(&buf, hash)
}

/// `resolveSource(owner, source, file, sourcePath)`.
fn resolve_source(source: &SourceRef, file: Option<&CssFile>, source_path: &str) -> Option<CssFile> {
    let mut context: Vec<CssFile> = Vec::new();
    if jstr::units(source_path).next() == Some(b'/' as u16) {
        if let Some(root) = source.borrow().get_path_root() {
            context.push(CssFile::Local(root.to_string()));
        }
    } else if let Some(f) = file {
        context.push(f.clone());
    }
    context.iter().find_map(|r| r.resolve(source_path)).or_else(|| css_file::toolkit_resolve(source_path)).or_else(|| css_file::source_resolve(source, source_path))
}

/// `tokenizeAndResolveSource(sourceValue, values, source, value, path, line, logInvalidEmbedOnMissingFile)`.
fn tokenize_and_resolve_source(source_value: &str, values: &mut EmbedParams, source: &SourceRef, value: &str, path: &str, line: i32, log_invalid_embed_on_missing_file: bool) -> bool {
    let mut source_value = source_value.to_string();
    let octothorpe = jstr::index_of_unit(&source_value, b'#' as u16, 0);
    if octothorpe != -1 {
        values.insert("symbol".into(), EmbedValue::Str(jstr::substring_from(&source_value, octothorpe + 1)));
        source_value = jstr::substring(&source_value, 0, octothorpe);
    }
    let file = css_file::toolkit_resolve(path);
    match resolve_source(source, file.as_ref(), &source_value) {
        Some(resolved_file) => {
            let resolved_name = resolved_file.get_name().replace('\\', "/");
            values.insert("original".into(), EmbedValue::Str(source_value));
            values.insert("_resolvedSource".into(), EmbedValue::Str(resolved_name.clone()));
            css_file::add_resolved_path(&resolved_name, &resolved_file);
            values.insert("source".into(), EmbedValue::Str(resolved_name));
            true
        }
        None => {
            if log_invalid_embed_on_missing_file {
                invalid_embed(value, path, line);
            }
            false
        }
    }
}

impl std::fmt::Display for AtEmbed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.prop_name)
    }
}
