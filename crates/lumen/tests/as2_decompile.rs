//! AS1/2 decompiler expectations (`ActionScript2Test`, `ActionScript2ClassesTest`).

use std::collections::HashMap;
use std::path::Path;

use lumen::action::decomp::uninit::Uninitialized;
use lumen::action::{decomp, read_action_list, tag_actions};
use lumen::tags::code::{DO_ACTION, DO_INIT_ACTION, SHOW_FRAME};

/// `ActionScriptTestBase.cleanPCode`.
fn clean(s: &str) -> String {
    let s = s.replace('\t', "   ");
    let mut lines: Vec<&str> = Vec::new();
    for l in s.trim().split(['\r', '\n']) {
        let t = l.trim_matches(' ');
        if !t.is_empty() {
            lines.push(t);
        }
    }
    let joined = lines.join("\n");
    let mut out = String::new();
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
    out.trim().to_string()
}

#[test]
fn as2_decompile_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let testdata = root.join("../../corpus/libsrc/ffdec_lib/testdata");
    if !testdata.is_dir() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let mut swfs: HashMap<String, (lumen::Swf, lumen::names::Names, Uninitialized)> = HashMap::new();
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("tests/fixtures/as2_decompile.json")).unwrap()).unwrap();
    let only = std::env::var("AS2_CASE").ok();
    let (mut pass, mut total) = (0, 0);
    let mut report = String::new();
    for c in v.as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let file = c["swf"].as_str().unwrap();
        let (swf, names, uninit) = swfs.entry(file.to_string()).or_insert_with(|| {
            let swf = lumen::Swf::parse(&std::fs::read(testdata.join(file)).unwrap()).unwrap();
            let names = lumen::names::Names::new(&swf);
            let uninit = decomp::swf_uninitialized(&swf);
            (swf, names, uninit)
        });
        let expected = c["expected"].as_str().unwrap();
        let tag = if let Some(frame) = c["frame"].as_u64() {
            let mut f = 0;
            let mut last = None;
            let mut found = None;
            for t in &swf.timeline.tags {
                if t.code == DO_ACTION {
                    last = Some(t);
                }
                if t.code == SHOW_FRAME {
                    f += 1;
                    if f == frame {
                        found = last;
                        break;
                    }
                    last = None;
                }
            }
            found
        } else {
            let cls = format!("__Packages.{}", c["class"].as_str().unwrap());
            swf.timeline.tags.iter().find(|t| t.code == DO_INIT_ACTION && t.character_id().and_then(|id| names.export.get(&id)) == Some(&cls))
        };
        total += 1;
        let Some(tag) = tag else {
            report.push_str(&format!("== {name}: source not found\n"));
            continue;
        };
        let body = tag_actions(tag.code, &tag.body).unwrap();
        let list = read_action_list(body, 0, body.len(), swf.header.version);
        let empty = Default::default();
        let u = if tag.code == DO_INIT_ACTION { &*uninit } else { &empty };
        let actual = decomp::actions_to_source(&list, tag.code == DO_INIT_ACTION, None, "", u);
        if clean(&actual) == clean(expected) {
            pass += 1;
        } else {
            report.push_str(&format!("== {name}\n-- expected\n{}\n-- actual\n{}\n", clean(expected), clean(&actual)));
        }
    }
    let out = root.join("../../target/tmp");
    std::fs::create_dir_all(&out).ok();
    let rp = out.join("as2_decompile_report.txt");
    std::fs::write(&rp, &report).unwrap();
    println!("AS2 decompile fixtures: {pass}/{total} pass (report: {})", rp.display());
}
