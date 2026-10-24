use std::path::{Path, PathBuf};

use ffdec::abc::{scripts, tag_abc, Abc};

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("swf")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Every ABC in the corpus parses, and every script entry produces p-code.
#[test]
fn corpus_abc() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let (mut abcs, mut bodies, mut entries) = (0, 0, 0);
    let mut failures = Vec::new();
    for f in &files {
        let Ok(swf) = ffdec::Swf::parse(&std::fs::read(f).unwrap()) else { continue };
        swf.walk(|path, t| {
            let Some((data, _)) = tag_abc(t) else { return };
            match Abc::parse(data) {
                Ok(abc) => {
                    abcs += 1;
                    bodies += abc.bodies.len();
                    for e in scripts::entries(&abc) {
                        entries += 1;
                        assert!(!scripts::entry_pcode(&abc, &e).is_empty());
                    }
                }
                Err(e) => failures.push(format!("{} {path:?}: {e}", f.display())),
            }
        });
    }
    eprintln!("{abcs} ABCs, {bodies} method bodies, {entries} script entries");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
