use std::path::{Path, PathBuf};

use lumen::action::compile::{compile, swf_classes, Target};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles decompiled corpus scripts and compares the bytes with the reference compiler's
/// output for the same text, read from `LUMEN_AS2_COMPILE_REF` (the As2Compile driver's
/// output: one folder per SWF with swf.txt, index.txt, i.as and i.bin).
#[test]
fn compiles_like_reference() {
    let Ok(dir) = std::env::var("LUMEN_AS2_COMPILE_REF") else { return };
    let (mut total, mut bad) = (0, Vec::new());
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
    dirs.sort();
    for d in dirs {
        let Ok(swf_path) = std::fs::read_to_string(d.join("swf.txt")) else { continue };
        let index = std::fs::read_to_string(d.join("index.txt")).unwrap_or_default();
        if index.trim().is_empty() {
            continue;
        }
        let swf = lumen::Swf::parse(&std::fs::read(root().join(swf_path.trim())).unwrap()).unwrap();
        let classes = swf_classes(&swf);
        for line in index.lines() {
            let mut parts = line.splitn(3, '\t');
            let (i, kind, key) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap_or(""));
            let Ok(text) = std::fs::read_to_string(d.join(format!("{i}.as"))) else { continue };
            let Ok(expected) = std::fs::read(d.join(format!("{i}.bin"))) else { continue };
            let target = match kind {
                "button" => Target::ButtonCond,
                "clip" => Target::ClipAction,
                _ => Target::Actions,
            };
            total += 1;
            let label = format!("{} {key}", swf_path.trim());
            match compile(&text, swf.header.version, target, &classes, Some(&swf)) {
                Ok(c) => {
                    let got = c.actions.to_bytes();
                    if got != expected {
                        let at = got.iter().zip(&expected).position(|(a, b)| a != b).unwrap_or(got.len().min(expected.len()));
                        bad.push(format!("{label}: differs at {at} of {} (wrote {})", expected.len(), got.len()));
                    }
                }
                Err(e) => bad.push(format!("{label}: {e}")),
            }
        }
    }
    let out = root().join("target/tmp");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("as2_compile_report.txt"), bad.join("\n")).unwrap();
    eprintln!("as2_compile: {}/{total} ok", total - bad.len());
    assert!(bad.is_empty(), "{} scripts differ", bad.len());
}

fn pcode(src: &str) -> String {
    let c = compile(src, 10, Target::Actions, &[], None).unwrap();
    let bytes = c.actions.to_bytes();
    let text = lumen::action::pcode::to_pcode(&lumen::action::read_action_list(&bytes, 0, bytes.len(), 10));
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// ActionScript2CompilerTest.
#[test]
fn compiler_expectations() {
    assert_eq!(
        pcode("function outfunc() {\n\tvar v1 = function (){\n\t\tvar a = 1;\n\t\tvar v3 = 2;\n\t\t\n\t\tvar v4 = function (){\n\t\t\tvar v5 = a + 3;\n\t\t}\n\t}\n}"),
        "ConstantPool \"v1\", \"a\", \"v3\", \"v4\", \"v5\"\nDefineFunction \"outfunc\", 0 {\nPush \"v1\"\nDefineFunction2 \"\", 0, 1, false, false, true, false, true, false, true, false, false {\nPush \"a\"\nPush 1\nDefineLocal\nPush \"v3\"\nPush 2\nDefineLocal\nPush \"v4\"\nDefineFunction2 \"\", 0, 1, false, false, true, false, true, false, true, false, false {\nPush \"v5\"\nPush \"a\"\nGetVariable\nPush 3\nAdd2\nDefineLocal\n}\nDefineLocal\n}\nDefineLocal\n}"
    );
    assert_eq!(
        pcode("function outfunc() {\n\tvar g = function (a,p2){\n\t\tvar v1 = a + 1 + p2;\n\t\t\n\t\tvar f = function (){\n\t\t\tvar v2 = a + 2;\n\t\t}\n\t}\n}"),
        "ConstantPool \"g\", \"v1\", \"a\", \"f\", \"v2\"\nDefineFunction \"outfunc\", 0 {\nPush \"g\"\nDefineFunction2 \"\", 2, 2, false, false, true, false, true, false, true, false, false, 0, \"a\", 1, \"p2\" {\nPush \"v1\"\nPush \"a\"\nGetVariable\nPush 1\nAdd2\nPush register1\nAdd2\nDefineLocal\nPush \"f\"\nDefineFunction2 \"\", 0, 1, false, false, true, false, true, false, true, false, false {\nPush \"v2\"\nPush \"a\"\nGetVariable\nPush 2\nAdd2\nDefineLocal\n}\nDefineLocal\n}\nDefineLocal\n}"
    );
    assert_eq!(pcode("trace(stop());"), "Stop\nPush undefined, undefined\nTrace");
}

#[test]
fn errors_name_their_line() {
    let e = compile("var a = 1;\nvar b = ;\n", 10, Target::Actions, &[], None).unwrap_err();
    assert_eq!(e.line, 2);
}
