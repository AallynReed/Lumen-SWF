//! AS3 decompiler expectations, scored against our port.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lumen::abc::decomp::ctx::{Config, Ctx};
use lumen::abc::decomp::index::AbcIndex;
use lumen::abc::decomp::method::{self, ConvertData, MethodReq};
use lumen::abc::decomp::writer::Writer;
use lumen::abc::{scripts, tag_abc, Abc, TraitData};

/// `ActionScriptTestBase.cleanPCode`.
fn clean(s: &str) -> String {
    let s = s.replace('\t', "   ");
    let s = s.trim();
    let mut out = String::new();
    let mut lines: Vec<&str> = Vec::new();
    for l in s.split(['\r', '\n']) {
        let t = l.trim_matches(' ');
        if !t.is_empty() || lines.is_empty() {
            lines.push(t);
        }
    }
    let joined = lines.join("\n");
    let mut prev_space = false;
    for c in joined.trim().chars() {
        if c == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        out.push(c);
    }
    let mut res = String::new();
    let cs: Vec<char> = out.trim().chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '#' && i + 1 < cs.len() && cs[i + 1].is_ascii_digit() {
            res.push_str("#_");
            i += 1;
            while i < cs.len() && cs[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        res.push(cs[i]);
        i += 1;
    }
    res
}

struct Case {
    suite: String,
    swf: String,
    method: String,
    is_static: bool,
    expected: String,
}

fn cases() -> Vec<Case> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/as3_decompile.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| Case {
            suite: c["suite"].as_str().unwrap().into(),
            swf: c["swf"].as_str().unwrap().into(),
            method: c["method"].as_str().unwrap().into(),
            is_static: c["static"].as_bool().unwrap(),
            expected: c["expected"].as_str().unwrap().into(),
        })
        .collect()
}

fn decompile(abcs: &[Abc], index: &std::sync::Arc<AbcIndex>, version: u8, method_name: &str, is_static: bool) -> Option<String> {
    let class_name = format!("{}{}", method_name[..1].to_uppercase(), &method_name[1..]);
    for abc in abcs {
        for e in scripts::entries(abc) {
            if e.package != "tests" || e.name != class_name {
                continue;
            }
            let ci = scripts::class_of(abc, &e)?;
            let inst = &abc.instances[ci];
            let cfg = Config { deobfuscate: method_name.to_lowercase().contains("obfus"), ..Config::testing() };
            let mut ctx = Ctx::new(abc, cfg).with_index(index.clone());
            ctx.swf_version = version;
            let run = inst.traits.iter().find_map(|t| match t.data {
                TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } if ctx.names().name(t.name, &[], true, true) == "run" => Some(method),
                _ => None,
            })?;
            let mut req = MethodReq::new(run);
            req.is_static = is_static;
            req.script_index = e.script as i32;
            req.class_index = ci as i32;
            req.init_traits = Some(&inst.traits);
            req.path = "run".into();
            let mut cd = ConvertData::default();
            let seen = HashSet::new();
            let conv = match method::convert(&ctx, &req, &mut cd, &seen) {
                Ok(c) => c,
                Err(e) => return Some(format!("ERROR: {e}")),
            };
            let mut w = Writer::new();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| method::to_source(&ctx, &req, &conv, &seen, -1, &mut w)));
            if r.is_err() {
                return Some("ERROR: print panic".into());
            }
            return Some(w.finish());
        }
    }
    None
}

#[test]
fn as3_decompile_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib");
    if !root.exists() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    if std::env::var("AS3_DEBUG").is_err() {
        std::panic::set_hook(Box::new(|_| {}));
    }
    let filter = std::env::var("AS3_CASE").ok();
    let mut swfs: HashMap<String, (u8, Vec<Abc>)> = HashMap::new();
    let mut indexes: HashMap<String, std::sync::Arc<AbcIndex>> = HashMap::new();
    let (mut pass, mut total) = (0, 0);
    let mut report = String::new();
    for c in cases() {
        if let Some(f) = &filter {
            if !c.method.contains(f.as_str()) {
                continue;
            }
        }
        let entry = swfs.entry(c.swf.clone()).or_insert_with(|| {
            let swf = lumen::Swf::parse(&std::fs::read(root.join(&c.swf)).unwrap()).unwrap();
            let mut abcs = Vec::new();
            swf.walk(|_, t| {
                if let Some((data, _)) = tag_abc(t) {
                    abcs.push(Abc::parse(data).unwrap());
                }
            });
            (swf.header.version, abcs)
        });
        let index = indexes.entry(c.swf.clone()).or_insert_with(|| AbcIndex::for_abcs(&entry.1.iter().collect::<Vec<_>>())).clone();
        total += 1;
        let actual = decompile(&entry.1, &index, entry.0, &c.method, c.is_static).unwrap_or_else(|| "NOT FOUND".into());
        let ok = clean(&actual) == clean(&c.expected);
        if ok {
            pass += 1;
        }
        report.push_str(&format!("=== {} {} {}\n", if ok { "PASS" } else { "FAIL" }, c.suite, c.method));
        if !ok {
            report.push_str("--- expected\n");
            report.push_str(&clean(&c.expected));
            report.push_str("\n--- actual\n");
            report.push_str(&clean(&actual));
            report.push('\n');
        }
    }
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("as3_decompile_report.txt");
    std::fs::write(&out, &report).unwrap();
    eprintln!("AS3 decompile fixtures: {pass}/{total} pass (report: {})", out.display());
}

/// Whole-class expectations (`ActionScript3ClassTest`).
#[test]
fn as3_class_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib");
    if !root.exists() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/as3_classes.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let filter = std::env::var("AS3_CASE").ok();
    let (mut pass, mut total) = (0, 0);
    let mut report = String::new();
    for c in v.as_array().unwrap() {
        let (swf_path, path, expected) = (c["swf"].as_str().unwrap(), c["path"].as_str().unwrap(), c["expected"].as_str().unwrap());
        if filter.as_ref().is_some_and(|f| !path.contains(f.as_str())) {
            continue;
        }
        let Ok(data) = std::fs::read(root.join(swf_path)) else { continue };
        let swf = lumen::Swf::parse(&data).unwrap();
        let mut abcs = Vec::new();
        swf.walk(|_, t| {
            if let Some((d, _)) = tag_abc(t) {
                abcs.push(Abc::parse(d).unwrap());
            }
        });
        let index = AbcIndex::for_abcs(&abcs.iter().collect::<Vec<_>>());
        let mut actual = "NOT FOUND".to_string();
        for abc in &abcs {
            if let Some(e) = scripts::entries(abc).into_iter().find(|e| e.full_name() == path) {
                actual = lumen::abc::decomp::entry_source_with(&index, abc, &e, &lumen::abc::decomp::SwfContext::of(&swf), Config::testing());
                break;
            }
        }
        total += 1;
        let ok = clean(&actual) == clean(expected);
        if ok {
            pass += 1;
        }
        report.push_str(&format!("=== {} {}\n", if ok { "PASS" } else { "FAIL" }, path));
        if !ok {
            report.push_str(&format!("--- expected\n{}\n--- actual\n{}\n", clean(expected), clean(&actual)));
        }
    }
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("as3_class_report.txt");
    std::fs::write(&out, &report).unwrap();
    eprintln!("AS3 class fixtures: {pass}/{total} pass (report: {})", out.display());
}
