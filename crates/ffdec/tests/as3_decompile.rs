//! JPEXS's AS3 decompiler expectations, scored against our port.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ffdec::abc::decomp::ctx::{Config, Ctx};
use ffdec::abc::decomp::method::{self, ConvertData, MethodReq};
use ffdec::abc::decomp::writer::Writer;
use ffdec::abc::{scripts, tag_abc, Abc, TraitData};

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

fn decompile(abcs: &[Abc], version: u8, method_name: &str, is_static: bool) -> Option<String> {
    let class_name = format!("{}{}", method_name[..1].to_uppercase(), &method_name[1..]);
    for abc in abcs {
        for e in scripts::entries(abc) {
            if e.package != "tests" || e.name != class_name {
                continue;
            }
            let ci = scripts::class_of(abc, &e)?;
            let inst = &abc.instances[ci];
            let mut ctx = Ctx::new(abc, Config::testing());
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib");
    if !root.exists() {
        eprintln!("JPEXS test corpus not found; skipping");
        return;
    }
    if std::env::var("AS3_DEBUG").is_err() {
        std::panic::set_hook(Box::new(|_| {}));
    }
    let filter = std::env::var("AS3_CASE").ok();
    let mut swfs: HashMap<String, (u8, Vec<Abc>)> = HashMap::new();
    let (mut pass, mut total) = (0, 0);
    let mut report = String::new();
    for c in cases() {
        if let Some(f) = &filter {
            if !c.method.contains(f.as_str()) {
                continue;
            }
        }
        let entry = swfs.entry(c.swf.clone()).or_insert_with(|| {
            let swf = ffdec::Swf::parse(&std::fs::read(root.join(&c.swf)).unwrap()).unwrap();
            let mut abcs = Vec::new();
            swf.walk(|_, t| {
                if let Some((data, _)) = tag_abc(t) {
                    abcs.push(Abc::parse(data).unwrap());
                }
            });
            (swf.header.version, abcs)
        });
        total += 1;
        let actual = decompile(&entry.1, entry.0, &c.method, c.is_static).unwrap_or_else(|| "NOT FOUND".into());
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
