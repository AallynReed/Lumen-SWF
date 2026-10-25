mod common;

use lumen::abc::{scripts, tag_abc, Abc};

/// Every ABC in the corpus parses, and every script entry produces p-code.
#[test]
fn corpus_abc() {
    let files = common::swf_files();
    if files.is_empty() {
        eprintln!("test corpus not found; skipping");
        return;
    }
    let per = common::par_map(&files, |f| {
        let (mut abcs, mut bodies, mut entries) = (0, 0, 0);
        let mut failures = Vec::new();
        let Ok(swf) = lumen::Swf::parse(&std::fs::read(f).unwrap()) else { return (0, 0, 0, failures) };
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
        (abcs, bodies, entries, failures)
    });
    let (mut abcs, mut bodies, mut entries, mut failures) = (0, 0, 0, Vec::new());
    for (a, b, e, f) in per {
        abcs += a;
        bodies += b;
        entries += e;
        failures.extend(f);
    }
    eprintln!("{abcs} ABCs, {bodies} method bodies, {entries} script entries");
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
