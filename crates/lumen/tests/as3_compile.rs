use std::path::Path;

use lumen::abc::decomp::{entry_renames, entry_source, index::AbcIndex, SwfContext};
use lumen::abc::{scripts::entries, tag_abc, Abc};

/// Compiling what the decompiler printed for compiled code gives the same code back: for
/// every class of the AS3 test movie, the second compile decompiles like the first.
#[test]
fn recompiling_is_stable() {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata/as3_new/bin/as3_new.flex.swf");
    let Ok(data) = std::fs::read(f) else { return };
    let swf = lumen::Swf::parse(&data).unwrap();
    let mut abcs = Vec::new();
    swf.walk(|_, t| {
        if let Some((d, _)) = tag_abc(t) {
            abcs.push(Abc::parse(d).unwrap());
        }
    });
    let ctx = SwfContext::of(&swf);
    let compile_once = |abcs: &[Abc], sel: usize, name: &str, text: &str| -> Result<(Vec<Abc>, String), String> {
        let index = AbcIndex::for_abcs(&abcs.iter().collect::<Vec<_>>());
        let e = entries(&abcs[sel]).into_iter().find(|x| x.full_name() == name).ok_or("missing")?;
        let renames = entry_renames(&index, &abcs[sel], &e, &ctx);
        let out = lumen::abc::decomp::with_big_stack(|| lumen::as3_replace::replace_script(abcs, sel, e.script, e.trait_index, text, &renames)).map_err(|e| format!("line {}: {}", e.line, e.message))?;
        let mut all = abcs.to_vec();
        all[sel] = Abc::parse(&out).map_err(|e| e.to_string())?;
        let index = AbcIndex::for_abcs(&all.iter().collect::<Vec<_>>());
        let e2 = entries(&all[sel]).into_iter().find(|x| x.full_name() == name).ok_or("missing after compiling")?;
        let text = entry_source(&index, &all[sel], &e2, &ctx);
        Ok((all, text))
    };
    let (mut ok, mut bad) = (0, Vec::new());
    for sel in 0..abcs.len() {
        let index = AbcIndex::for_abcs(&abcs.iter().collect::<Vec<_>>());
        for e in entries(&abcs[sel]) {
            if e.trait_index.is_none() {
                continue;
            }
            let name = e.full_name();
            let source = entry_source(&index, &abcs[sel], &e, &ctx);
            let r = compile_once(&abcs, sel, &name, &source).and_then(|(all, first)| compile_once(&all, sel, &name, &first).map(|(_, second)| (first, second)));
            match r {
                Ok((first, second)) if first == second => ok += 1,
                Ok((first, second)) => {
                    if let Ok(dir) = std::env::var("LUMEN_AS3_DUMP") {
                        let _ = std::fs::write(Path::new(&dir).join(format!("{name}.first.as")), &first);
                        let _ = std::fs::write(Path::new(&dir).join(format!("{name}.second.as")), &second);
                    }
                    bad.push(format!("{name}: decompiles differently after a second compile"))
                }
                Err(err) => bad.push(format!("{name}: {err}")),
            }
        }
    }
    eprintln!("stable {ok}/{}\n{}", ok + bad.len(), bad.join("\n"));
    let failed: Vec<&str> = bad.iter().map(|b| b.split(':').next().unwrap_or("")).collect();
    let known: Vec<&str> = KNOWN.iter().map(|(n, _)| *n).collect();
    assert_eq!(failed, known, "{}", bad.join("\n"));
}

/// Classes mxmlc.jar and JPEXS themselves do not round-trip, in decompile order.
const KNOWN: &[(&str, &str)] = &[
    ("tests.TestXml", "ASC's scanner puts the whitespace before an XML comment or CDATA literal into it"),
    ("tests.TestTryReturn2", "ASC stores a needs_init slot's default at entry and JPEXS prints that as the declaration"),
    ("tests.TestForEachReturn", "JPEXS keeps a for-each's registers as locals when the body returns"),
    ("tests.TestForInReturn", "JPEXS keeps a for-in's registers as locals when the body returns"),
    ("tests.TestVarFqn", "ASC stores a needs_init slot's default at entry and JPEXS prints that as the declaration"),
    ("tests.TestUndefined", "ASC stores a needs_init slot's default at entry and JPEXS prints that as the declaration"),
    ("tests.TestConvert", "ASC hoists the default of the bare `var` JPEXS prints beside a chained assignment"),
    ("tests.TestInlineFunctions", "ASC stores a needs_init slot's default at entry and JPEXS prints that as the declaration"),
];
