//! Script editing by tree id: `as2:<index>` for AS1/2 sources, `<script>:<trait>` for AS3
//! entries, whose methods are edited one at a time. Texts are edited by tag path.

use crate::abc::scripts::{entries, methods, parse_id, MethodRef};
use crate::abc::Abc;
use crate::action::asm::ParseError;
use crate::Swf;

fn fail(message: impl Into<String>) -> ParseError {
    ParseError { line: 0, message: message.into() }
}

fn as2_source(swf: &Swf, path: &[usize], script: &str) -> Option<Result<crate::action::scripts::Source, ParseError>> {
    let index = script.strip_prefix("as2:")?;
    let index = index.parse().map_err(|_| fail("bad script id"));
    Some(index.and_then(|i| crate::action::scripts::find(swf, path, i).ok_or_else(|| fail("script not found"))))
}

fn as3_entry(swf: &Swf, path: &[usize], script: &str) -> Result<(Abc, Vec<MethodRef>), ParseError> {
    let tag = swf.tag(path).ok_or_else(|| fail("tag not found"))?;
    let (data, _) = crate::abc::tag_abc(tag).ok_or_else(|| fail("not an ABC tag"))?;
    let abc = Abc::parse(data).map_err(|e| fail(e.to_string()))?;
    let (s, t) = parse_id(script).ok_or_else(|| fail("bad script id"))?;
    let e = entries(&abc).into_iter().find(|e| e.script == s && e.trait_index == t).ok_or_else(|| fail("script not found"))?;
    let m = methods(&abc, &e);
    Ok((abc, m))
}

/// The methods an AS3 script is edited by; empty for AS1/2, which is edited whole.
pub fn script_methods(swf: &Swf, path: &[usize], script: &str) -> Result<Vec<String>, ParseError> {
    if as2_source(swf, path, script).is_some() {
        return Ok(Vec::new());
    }
    Ok(as3_entry(swf, path, script)?.1.into_iter().map(|m| m.label).collect())
}

/// The editable p-code of an AS1/2 script, or of one AS3 method.
pub fn script_pcode(swf: &Swf, path: &[usize], script: &str, method: Option<&str>) -> Result<String, ParseError> {
    if let Some(src) = as2_source(swf, path, script) {
        return Ok(crate::action::pcode::to_pcode(&src?.actions(swf)));
    }
    let (abc, all) = as3_entry(swf, path, script)?;
    let m = all.iter().find(|m| Some(m.label.as_str()) == method).ok_or_else(|| fail("method not found"))?;
    Ok(m.pcode(&abc))
}

/// Assembles `text` in place of what `script_pcode` returned.
pub fn set_script_pcode(swf: &mut Swf, path: &[usize], script: &str, method: Option<&str>, text: &str) -> Result<(), ParseError> {
    if let Some(src) = as2_source(swf, path, script) {
        let src = src?;
        let list = crate::action::asm::parse_pcode(text, swf.header.version)?;
        return src.set_actions(swf, &list).map_err(|e| fail(e.to_string()));
    }
    let (mut abc, all) = as3_entry(swf, path, script)?;
    let m = all.into_iter().find(|m| Some(m.label.as_str()) == method).ok_or_else(|| fail("method not found"))?;
    crate::abc::asm::assemble_method(&mut abc, m.method, m.owner, text)?;
    swf.set_abc(path, &abc).map_err(|e| fail(e.to_string()))
}

/// Compiles ActionScript in place of a script, as FFDec's script editor does.
pub fn set_script_source(swf: &mut Swf, path: &[usize], script: &str, text: &str) -> Result<(), ParseError> {
    use crate::action::compile::{compile, swf_classes, Target};
    use crate::action::scripts::Kind;
    let Some(src) = as2_source(swf, path, script) else { return Err(fail("compiling ActionScript 3 is not supported yet; edit its p-code")) };
    let src = src?;
    let target = match src.kind {
        Kind::ButtonCond => Target::ButtonCond,
        Kind::ClipAction => Target::ClipAction,
        _ => Target::Actions,
    };
    let c = compile(text, swf.header.version, target, &swf_classes(swf), Some(swf)).map_err(|e| ParseError { line: e.line, message: e.message })?;
    src.set_compiled(swf, &c).map_err(|e| fail(e.to_string()))
}

/// The editable text of a text tag: FFDec's formatted text for DefineText/2, the initial text
/// (HTML when the field is HTML) for DefineEditText. None for other tags.
pub fn text_source(swf: &Swf, path: &[usize]) -> Result<Option<String>, ParseError> {
    use crate::tags::code::*;
    let tag = swf.tag(path).ok_or_else(|| fail("tag not found"))?;
    match tag.code {
        DEFINE_TEXT | DEFINE_TEXT2 => {
            let t = crate::text::StaticText::read(tag).map_err(|e| fail(e.to_string()))?;
            Ok(Some(crate::text::formatted(swf, &t, false)))
        }
        DEFINE_EDIT_TEXT => Ok(Some(crate::export::text::edit_text_raw(tag).map_err(|e| fail(e.to_string()))?.0)),
        _ => Ok(None),
    }
}

/// Stores edited text in place of what `text_source` returned.
pub fn set_text_source(swf: &mut Swf, path: &[usize], text: &str) -> Result<(), ParseError> {
    use crate::tags::code::*;
    match swf.tag(path).map(|t| t.code) {
        Some(DEFINE_TEXT | DEFINE_TEXT2) => crate::text::import(swf, path, text),
        Some(DEFINE_EDIT_TEXT) => crate::importers::edit_text(swf, path, text).map_err(|e| fail(e.to_string())),
        _ => Err(fail("not a text tag")),
    }
}
