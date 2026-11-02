//! `LOG_SPEC=<spec> LOG_OUT=<dump>`: the scenarios of the scratchpad's `LogDump` harness, through `LocalLogger` and
//! `MxmlLogAdapter`.

use std::rc::Rc;

use crate::asc::flex2::compiler::mxml::gen::line_number_map::{DualModeLineNumberMap, LineNumberMap};
use crate::asc::flex2::compiler::mxml::mxml_log_adapter::{LineMap, MxmlLogAdapter};
use crate::asc::flex2::compiler::source::{canonical_path, Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level, LocalLogger, LogAdapter, LogCall};
use crate::asc::util::jhash::JHashMap;

fn esc(s: &str) -> String {
    let mut b = String::new();
    for c in s.encode_utf16() {
        match c {
            10 => b.push_str("\\n"),
            13 => b.push_str("\\r"),
            9 => b.push_str("\\t"),
            92 => b.push_str("\\\\"),
            c if !(0x20..=0x7e).contains(&c) => b.push_str(&format!("\\u{:04x}", c)),
            c => b.push(c as u8 as char),
        }
    }
    b
}

fn unescape(s: &str) -> String {
    let mut b = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => b.push('\n'),
                Some('t') => b.push('\t'),
                Some('r') => b.push('\r'),
                Some(n) => b.push(n),
                None => b.push(c),
            }
        } else {
            b.push(c);
        }
    }
    b
}

#[derive(Default)]
struct Cfg {
    parent: Option<String>,
    local: Option<Rc<LocalLogger>>,
    source: Option<SourceRef>,
    map: Option<Rc<DualModeLineNumberMap>>,
    extras: Vec<LineNumberMap>,
    renames: Option<JHashMap<String, String>>,
    built: Option<Rc<dyn LogAdapter>>,
}

fn bottom(cfgs: &std::collections::HashMap<String, Cfg>, id: &str) -> String {
    match &cfgs[id].parent {
        Some(p) => bottom(cfgs, p),
        None => id.to_string(),
    }
}

/// The adapter chain for `id`, and the `LocalLogger` at its bottom.
fn build(cfgs: &mut std::collections::HashMap<String, Cfg>, id: &str) -> Rc<dyn LogAdapter> {
    if let Some(b) = &cfgs[id].built {
        return b.clone();
    }
    let original = cfgs[id].parent.clone().map(|p| build(cfgs, &p));
    let c = cfgs.get_mut(id).unwrap();
    let a = MxmlLogAdapter::new(original, c.map.clone().map(LineMap::Dual));
    for x in std::mem::take(&mut c.extras) {
        a.add_line_number_map(LineMap::Plain(Rc::new(std::cell::RefCell::new(x))));
    }
    a.set_renamed_variable_map(c.renames.take());
    let a: Rc<dyn LogAdapter> = Rc::new(a);
    c.built = Some(a.clone());
    a
}

/// `LOG_SPEC=<script> LOG_OUT=<dump>`: replays the scratchpad's `LogDump` scripts.
#[test]
fn log_dump() {
    let (Ok(spec), Ok(out)) = (std::env::var("LOG_SPEC"), std::env::var("LOG_OUT")) else { return };
    let mut all = String::new();
    let mut cfgs: std::collections::HashMap<String, Cfg> = std::collections::HashMap::new();
    let mut last = String::new();
    toolkit::reset(true);
    for line in std::fs::read_to_string(spec).unwrap().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let w: Vec<&str> = f[0].split(' ').collect();
        match w[0] {
            "SCENARIO" => {
                last = w[1].to_string();
                cfgs.insert(last.clone(), Cfg::default());
                toolkit::set_log_adapter(None);
            }
            "PARENT" => cfgs.get_mut(&last).unwrap().parent = Some(f[1].to_string()),
            "SOURCE" => {
                let s = Source::new(VirtualFile::local(std::path::Path::new(f[1])), None, "", "App", Owner::None, false, true, true);
                let l = LocalLogger::new();
                l.set_source(&s);
                let c = cfgs.get_mut(&last).unwrap();
                c.local = Some(l);
                c.source = Some(s);
            }
            "INCLUDE" => cfgs[&last].source.as_ref().unwrap().borrow_mut().add_file_include(canonical_path(std::path::Path::new(f[1]))),
            "MAP" => cfgs.get_mut(&last).unwrap().map = Some(Rc::new(DualModeLineNumberMap::new(Some(f[1]), Some(f[2])))),
            "PUT" => cfgs[&last].map.as_ref().unwrap().put(f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap(), f[4] == "1"),
            "EXTRA" => cfgs.get_mut(&last).unwrap().extras.push(LineNumberMap::new(Some(f[1]), Some(f[2]))),
            "XPUT" => cfgs.get_mut(&last).unwrap().extras.last_mut().unwrap().put(f[1].parse().unwrap(), f[2].parse().unwrap(), f[3].parse().unwrap()),
            "RENAME" => {
                cfgs.get_mut(&last).unwrap().renames.get_or_insert_with(JHashMap::new).insert(f[1].to_string(), f[2].to_string());
            }
            "USE" => {
                let (adapter, base) = if f[1] == "none" { (None, bottom(&cfgs, &last)) } else { (Some(build(&mut cfgs, f[1])), bottom(&cfgs, f[1])) };
                toolkit::set_logger(cfgs[&base].local.clone());
                toolkit::set_log_adapter(adapter);
            }
            "CALL" => {
                let level = if f[2] == "error" { Level::Error } else { Level::Warning };
                let (path, msg, source) = (f[3].to_string(), unescape(f[7]), unescape(f[8]));
                let (line, col, code): (i32, i32, i32) = (f[4].parse().unwrap(), f[5].parse().unwrap(), f[6].parse().unwrap());
                match f[1] {
                    "text" => toolkit::log_call(LogCall::Text { level, msg }),
                    "path" => toolkit::log_call(LogCall::Path { level, path, msg, code: None }),
                    "pathcode" => toolkit::log_call(LogCall::Path { level, path, msg, code: Some(code) }),
                    "pathline" => toolkit::log_call(LogCall::PathLine { level, path, line, msg, code: None }),
                    "pathlinecode" => toolkit::log_call(LogCall::PathLine { level, path, line, msg, code: Some(code) }),
                    "pathlinecol" => toolkit::log_call(LogCall::PathLineCol { level, path, line, col, msg }),
                    "src" => toolkit::log_call(LogCall::PathLineColSource { level, path, line, col, msg, source, code: None }),
                    "srccode" => toolkit::log_call(LogCall::PathLineColSource { level, path, line, col, msg, source, code: Some(code) }),
                    kind => {
                        let m = if level == Level::Error {
                            CompilerMessage::error(format!("'{msg}' is not a valid identifier."))
                        } else {
                            CompilerMessage::warning(format!("Unexpected processing instruction encountered and will be ignored. Found: {msg}"))
                        };
                        match kind {
                            "msgnopath" => toolkit::log(m, None, -1, -1, None),
                            "msgsrc" => toolkit::log(m, Some(&path), line, col, Some(&source)),
                            _ => toolkit::log(m, Some(&path), line, col, None),
                        }
                    }
                }
            }
            "END" => {
                let text: String = toolkit::take_diagnostics().iter().map(|d| format!("{}\n\n", d.text.replace("\r\n", "\n"))).collect();
                all += &format!("=== {}\nSTDERR {}\n", w.get(1).copied().unwrap_or(""), esc(&text));
                toolkit::set_log_adapter(None);
                toolkit::set_logger(None);
                cfgs.clear();
                toolkit::reset(true);
            }
            _ => {}
        }
    }
    std::fs::write(out, all).unwrap();
}
