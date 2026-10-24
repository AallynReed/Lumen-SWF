//! Searching scripts and texts: a plain or
//! regular-expression pattern over decompiled ActionScript (or p-code) and formatted texts.

use serde::Serialize;

use crate::error::{Error, Result};
use crate::export::Exporter;
use crate::tags::code::*;
use crate::tree::{self, Node};

pub struct Query {
    pub text: String,
    pub ignore_case: bool,
    pub regex: bool,
    /// Search p-code instead of ActionScript.
    pub pcode: bool,
    /// Search texts instead of scripts.
    pub texts: bool,
}

/// A matching tree node.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub label: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
}

pub fn search(ex: &Exporter, q: &Query) -> Result<Vec<Hit>> {
    let pattern = if q.regex { q.text.clone() } else { regex::escape(&q.text) };
    let re = regex::RegexBuilder::new(&pattern).case_insensitive(q.ignore_case).build().map_err(|e| Error::Other(e.to_string()))?;
    let mut out = Vec::new();
    if q.text.is_empty() {
        return Ok(out);
    }
    let swf = ex.swf();
    let mut visit = |n: &Node| -> Result<()> {
        let Some(path) = &n.path else { return Ok(()) };
        let p = tree::parse_path(path).unwrap_or_default();
        let text = match (&n.script, q.texts) {
            (Some(s), false) => {
                let pv = ex.script_preview(&p, s)?;
                if q.pcode { pv.alt_text } else { pv.text }
            }
            (None, true) if n.kind == "tag" && swf.tag(&p).is_some_and(|t| matches!(t.code, DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT)) => crate::edit::text_source(swf, &p).ok().flatten(),
            _ => None,
        };
        if text.is_some_and(|t| re.is_match(&t)) {
            out.push(Hit { label: n.label.clone(), path: path.clone(), script: n.script.clone() });
        }
        Ok(())
    };
    fn walk(nodes: &[Node], f: &mut dyn FnMut(&Node) -> Result<()>) -> Result<()> {
        for n in nodes {
            f(n)?;
            walk(&n.children, f)?;
        }
        Ok(())
    }
    walk(&tree::resource_tree(swf), &mut visit)?;
    Ok(out)
}
