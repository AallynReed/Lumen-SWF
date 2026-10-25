//! The Java objects the CSS templates (`StyleDef.vm`, `StyleModule.vm`, `StyleLibrary.vm`, `FontFaceRules.vm`) see,
//! as Velocity values: each class with the public getters Velocity can resolve, their results taken when the context
//! is built (templates only read them). Also `mxml.gen.VelocityUtil` and the `getTemplate`/`merge` calls with their
//! `VelocityException` reporting.

use std::borrow::Cow;
use std::rc::Rc;

use super::at_embed::{AtEmbed, AtEmbedKey, EmbedValue};
use super::import::Import;
use super::reference::Reference;
use super::style_def::StyleDefRef;
use super::style_module::StyleModule;
use super::styles_container::StyleDefList;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::flash::css::style_declaration::{PropertyValue, StyleDeclarationBlockRef, StyleDeclarationRef, StyleProperty};
use crate::asc::flex2::flash::css::style_selector::{MediaList, StyleCondition, StyleSelectorRef};
use crate::asc::flex2::velocity::{self, JType, JavaException, JavaObject, Method, Value};
use crate::asc::util::jhash::{JHashSet, JavaHash};

/// A Java object whose zero-argument getters were evaluated up front.
struct Bean {
    class_name: &'static str,
    methods: Vec<Method>,
    results: Vec<Value>,
    text: String,
    hash: Option<i32>,
}

fn method(name: &'static str, ret: JType) -> Method {
    Method { name: Cow::Borrowed(name), params: Cow::Borrowed(&[]), ret }
}

fn class(c: &'static str) -> JType {
    JType::Class(Cow::Borrowed(c))
}

impl Bean {
    fn new(class_name: &'static str, text: String) -> Bean {
        Bean { class_name, methods: Vec::new(), results: Vec::new(), text, hash: None }
    }

    fn get(mut self, name: &'static str, ret: JType, value: Value) -> Bean {
        self.methods.push(method(name, ret));
        self.results.push(value);
        self
    }

    fn value(self) -> Value {
        Value::Object(Rc::new(self))
    }
}

impl JavaObject for Bean {
    fn class_name(&self) -> &str {
        self.class_name
    }

    fn methods(&self) -> &[Method] {
        &self.methods
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        let i = self.methods.iter().position(|m| m == method).ok_or_else(|| JavaException::new("java.lang.NoSuchMethodException", Some(&method.name)))?;
        Ok(self.results[i].clone())
    }

    fn to_java_string(&self) -> String {
        self.text.clone()
    }

    fn java_equals(&self, other: &Value) -> bool {
        match (self.hash, other) {
            (Some(_), Value::Object(o)) => o.class_name() == self.class_name && o.to_java_string() == self.text,
            (None, Value::Object(o)) => std::ptr::addr_eq(Rc::as_ptr(o), self as *const Bean),
            _ => false,
        }
    }

    fn java_hash_code(&self) -> i32 {
        self.hash.unwrap_or(self as *const Bean as usize as i32)
    }
}

pub fn string(s: &str) -> Value {
    Value::str(s)
}

fn boolean(b: bool) -> Value {
    Value::Boolean(b)
}

fn strings(v: &[String]) -> Value {
    Value::list(v.iter().map(|s| Value::str(s)).collect())
}

fn identity(prefix: &str, p: *const u8) -> String {
    format!("{prefix}@{:x}", p as usize)
}

fn import(i: &Import) -> Value {
    let mut b = Bean::new("flex2.compiler.css.Import", identity("flex2.compiler.css.Import", i as *const Import as *const u8))
        .get("getLineNumber", JType::Int, Value::Integer(i.get_line_number()))
        .get("getValue", JType::String, string(i.get_value()));
    b.hash = Some(i.java_hash());
    b.value()
}

fn imports(set: &JHashSet<Import>) -> Value {
    Value::list_of("java.util.HashSet", set.iter().map(import).collect())
}

fn embed_value(v: &EmbedValue) -> Value {
    match v {
        EmbedValue::Str(s) => string(s),
        EmbedValue::Sources(_) => Value::list(Vec::new()),
    }
}

fn at_embed(e: &Rc<AtEmbed>) -> Value {
    let attrs: Vec<(Value, Value)> = e.get_attributes().iter().map(|(k, v)| (string(k), embed_value(v))).collect();
    let mut b = Bean::new("flex2.compiler.mxml.rep.AtEmbed", e.get_prop_name().to_string())
        .get("getAttributes", class("java.util.Map"), Value::map_of("java.util.HashMap", attrs))
        .get("getPropName", JType::String, string(e.get_prop_name()))
        .get("getType", JType::String, string(e.get_type()))
        .get("getXmlLineNumber", JType::Int, Value::Integer(e.get_xml_line_number()));
    b.hash = Some(e.java_hash());
    b.value()
}

fn at_embeds(set: &JHashSet<AtEmbedKey>) -> Value {
    Value::list_of("java.util.HashSet", set.iter().map(|k| at_embed(&k.0)).collect())
}

/// `StylesContainer.atEmbeds`, a `LinkedHashMap<String, AtEmbed>`, as `FontFaceRules.vm` gets it.
pub fn at_embed_map(entries: &[(String, Rc<AtEmbed>)]) -> Value {
    Value::map_of("java.util.LinkedHashMap", entries.iter().map(|(k, e)| (string(k), at_embed(e))).collect())
}

fn reference(r: &Rc<Reference>) -> Value {
    Bean::new("flex2.compiler.css.Reference", r.get_value().to_string())
        .get("getLineNumber", JType::Int, Value::Integer(r.get_line_number()))
        .get("getPath", JType::String, string(r.get_path()))
        .get("getValue", JType::String, string(r.get_value()))
        .get("isClassReference", JType::Boolean, boolean(r.is_class_reference()))
        .value()
}

fn style_property(p: &Rc<StyleProperty>) -> Value {
    let value = match p.get_value() {
        PropertyValue::Str(s) => string(s),
        PropertyValue::AtEmbed(e) => at_embed(e),
        PropertyValue::Reference(r) => reference(r),
    };
    Bean::new("flash.css.StyleProperty", p.to_string())
        .get("getLineNumber", JType::Int, Value::Integer(p.get_line_number()))
        .get("getName", JType::String, string(p.get_name()))
        .get("getPath", JType::String, string(p.get_path()))
        .get("getValue", JType::Object, value)
        .value()
}

fn media_list(m: &MediaList) -> Value {
    Bean::new("flash.css.MediaList", m.to_string())
        .get("getQueries", class("java.util.List"), strings(m.get_queries()))
        .get("toEscapedString", JType::String, string(&m.to_escaped_string()))
        .value()
}

fn block(b: &StyleDeclarationBlockRef) -> Value {
    let b = b.borrow();
    let props: Vec<(Value, Value)> = b.get_properties().iter().map(|(k, v)| (string(k), style_property(v))).collect();
    Bean::new("flash.css.StyleDeclarationBlock", identity("flash.css.StyleDeclarationBlock", &*b as *const _ as *const u8))
        .get("getEffectStyles", class("java.util.List"), strings(b.get_effect_styles()))
        .get("getMediaList", class("flash.css.MediaList"), b.get_media_list().map_or(Value::Null, |m| media_list(m)))
        .get("getProperties", class("java.util.Map"), Value::map_of("java.util.HashMap", props))
        .get("hasMediaList", JType::Boolean, boolean(b.has_media_list()))
        .value()
}

fn condition(c: &StyleCondition) -> Value {
    Bean::new("flash.css.StyleCondition", c.to_string())
        .get("getKind", JType::String, string(c.get_kind()))
        .get("getValue", JType::String, c.get_value().map_or(Value::Null, string))
        .value()
}

fn selector(s: &StyleSelectorRef) -> Value {
    let s = s.borrow();
    Bean::new("flash.css.StyleSelector", s.to_string())
        .get("getAncestor", class("flash.css.StyleSelector"), s.get_ancestor().map_or(Value::Null, |a| selector(&a)))
        .get("getConditions", class("java.util.List"), s.get_conditions().map_or(Value::Null, |c| Value::list(c.iter().map(condition).collect())))
        .get("getValue", JType::String, s.get_value().map_or(Value::Null, string))
        .value()
}

fn declaration(d: &StyleDeclarationRef) -> Value {
    let d = d.borrow();
    Bean::new("flash.css.StyleDeclaration", identity("flash.css.StyleDeclaration", &*d as *const _ as *const u8))
        .get("getDeclarationBlocks", class("java.util.List"), Value::list(d.get_declaration_blocks().iter().map(block).collect()))
        .get("getLineNumber", JType::Int, Value::Integer(d.get_line_number()))
        .get("getPath", JType::String, string(d.get_path()))
        .get("getSelector", class("flash.css.StyleSelector"), d.get_selector().map_or(Value::Null, |s| selector(&s)))
        .get("getSubject", JType::String, d.get_subject().map_or(Value::Null, string))
        .get("hasEffectStyles", JType::Boolean, boolean(d.has_effect_styles()))
        .get("hasProperties", JType::Boolean, boolean(d.has_properties()))
        .value()
}

fn style_def(sd: &StyleDefRef) -> Value {
    let d = sd.borrow();
    let declarations = d.get_declarations().map_or(Value::Null, |ds| Value::map_of("java.util.LinkedHashMap", ds.iter().map(|(k, v)| (string(k), declaration(v))).collect()));
    let styles: Vec<(Value, Value)> = d.get_styles().iter().map(|(k, v)| (string(k), style_property(v))).collect();
    Bean::new("flex2.compiler.css.StyleDef", identity("flex2.compiler.css.StyleDef", &*d as *const _ as *const u8))
        .get("getAllowDuplicateDefaultStyleDeclarations", JType::Boolean, boolean(d.get_allow_duplicate_default_style_declarations()))
        .get("getAtEmbeds", class("java.util.Set"), at_embeds(&d.get_at_embeds()))
        .get("getDeclarations", class("java.util.Map"), declarations)
        .get("getEffectStyles", class("java.util.List"), strings(d.get_effect_styles()))
        .get("getImports", class("java.util.Set"), imports(&d.get_imports()))
        .get("getLineNumber", JType::Int, Value::Integer(d.get_line_number()))
        .get("getStyles", class("java.util.Map"), Value::map_of("java.util.HashMap", styles))
        .get("getSubject", JType::String, string(d.get_subject()))
        .get("getTypeName", JType::String, string(&d.get_type_name()))
        .get("isAdvanced", JType::Boolean, boolean(d.is_advanced()))
        .get("isTypeSelector", JType::Boolean, boolean(d.is_type_selector()))
        .value()
}

/// `StylesContainer.StyleDefList`, as `StyleDef.vm`'s `$styleDefList`.
pub fn style_def_list(list: &StyleDefList) -> Value {
    Bean::new("flex2.compiler.css.StylesContainer$StyleDefList", identity("flex2.compiler.css.StylesContainer$StyleDefList", list as *const _ as *const u8))
        .get("getAllowDuplicateDefaultStyleDeclarations", JType::Boolean, boolean(list.get_allow_duplicate_default_style_declarations()))
        .get("getAtEmbeds", class("java.util.Set"), at_embeds(&list.get_at_embeds()))
        .get("getImports", class("java.util.Set"), imports(&list.get_imports()))
        .get("getStyleDefs", class("java.util.List"), Value::list(list.style_defs.iter().map(style_def).collect()))
        .get("isAdvanced", JType::Boolean, boolean(list.is_advanced()))
        .get("size", JType::Int, Value::Integer(list.style_defs.len() as i32))
        .value()
}

/// A `StyleModule`, as `StyleModule.vm` / `StyleLibrary.vm`'s `$styleModule`.
pub fn style_module(m: &StyleModule) -> Value {
    Bean::new("flex2.compiler.css.StyleModule", identity("flex2.compiler.css.StyleModule", m as *const _ as *const u8))
        .get("getAtEmbeds", class("java.util.Set"), at_embeds(&m.get_at_embeds()))
        .get("getImports", class("java.util.Set"), imports(&m.get_imports()))
        .get("getName", JType::String, m.get_name().map_or(Value::Null, string))
        .get("getQualifiedTypeSelectors", JType::Boolean, boolean(m.get_qualified_type_selectors()))
        .get("getStyleDefs", class("java.util.Collection"), Value::list_of("java.util.LinkedHashMap$LinkedValues", m.get_style_defs().iter().map(style_def).collect()))
        .get("isAdvanced", JType::Boolean, boolean(m.is_advanced()))
        .value()
}

/// `flex2.compiler.mxml.gen.VelocityUtil`. Its line mappings only feed error reporting and debug line numbers,
/// which these sources do not get, so `mapLines` records nothing.
struct VelocityUtil {
    path: String,
    debug: bool,
    line_mapping: bool,
}

static VELOCITY_UTIL_METHODS: &[Method] = &[
    Method::new("getTemplatePath", &[], JType::String),
    Method::new("getDebug", &[], JType::Boolean),
    Method::new("getLineMappingEnabled", &[], JType::Boolean),
    Method::new("mapLines", &[JType::Int, JType::String], JType::Void),
    Method::new("mapCompileErrorLines", &[JType::Int, JType::String], JType::Void),
    Method::new("mapLines", &[JType::Int, JType::String, JType::Boolean], JType::Void),
];

impl JavaObject for VelocityUtil {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.gen.VelocityUtil"
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == "flex2.compiler.mxml.gen.VelocityUtil" || class_name == "flex2.compiler.util.VelocityManager$Util"
    }

    fn methods(&self) -> &[Method] {
        VELOCITY_UTIL_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getTemplatePath" => string(&self.path),
            "getDebug" => boolean(self.debug),
            "getLineMappingEnabled" => boolean(self.line_mapping),
            _ => Value::Null,
        })
    }
}

/// `VelocityManager.getTemplate(template)` and `template.merge(context, out)` with `VelocityUtil(TEMPLATE_PATH, debug,
/// out, lineNumberMap)`; `None` after logging `TemplateNotFound` or `GenerateException` (the latter naming
/// `generating`).
pub fn merge(template_name: &str, debug: bool, values: Vec<(&str, Value)>, generating: &str) -> Option<String> {
    merge_mapped(template_name, debug, false, values, generating)
}

pub fn merge_mapped(template_name: &str, debug: bool, line_mapping: bool, values: Vec<(&str, Value)>, generating: &str) -> Option<String> {
    let template = match velocity::get_template(template_name) {
        Ok(t) => t,
        Err(_) => {
            toolkit::log(CompilerMessage::error(format!("Unable to load template '{template_name}'.")), None, -1, -1, None);
            return None;
        }
    };
    let util = velocity::Util::with_extension(Rc::new(VelocityUtil { path: "flex2/compiler/css/".to_string(), debug, line_mapping }));
    let vc = velocity::get_code_gen_context(Some(Rc::new(util)));
    for (k, v) in values {
        vc.put(k, v);
    }
    match template.merge(&vc) {
        Ok(s) => Some(s),
        Err(e) => {
            toolkit::log(CompilerMessage::error(format!("Exception while generating from {generating}: {e}.")), None, -1, -1, None);
            None
        }
    }
}
