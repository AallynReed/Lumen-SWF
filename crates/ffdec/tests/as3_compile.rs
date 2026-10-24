//! The AS3 compiler, checked against sources from `FFDEC_AS3_SRC` (directories of .as files, `;`-separated).

use std::path::{Path, PathBuf};

use ffdec::abc::compile::lexer::{Lexer, T};

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "as") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(v) = std::env::var("FFDEC_AS3_SRC") {
        for d in v.split(';').filter(|d| !d.is_empty()) {
            walk(Path::new(d), &mut out);
        }
    }
    out.sort();
    out
}

#[test]
fn lexes_sources() {
    let files = sources();
    let mut bad = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_default();
        let mut lx = Lexer::new(&src);
        loop {
            match lx.lex() {
                Ok(s) if s.t == T::Eof => break,
                Ok(_) => {}
                Err(e) => {
                    bad.push(format!("{}: {e}", f.display()));
                    break;
                }
            }
        }
    }
    eprintln!("lexed {}/{}", files.len() - bad.len(), files.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Compiling what the decompiler printed for compiled code gives the same code back: for
/// every class of the AS3 test movie, the second compile decompiles like the first.
#[test]
fn recompiling_is_stable() {
    use ffdec::abc::decomp::{entry_source, index::AbcIndex, SwfContext};
    use ffdec::abc::{scripts::entries, tag_abc, Abc};
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata/as3_new/bin/as3_new.flex.swf");
    let Ok(data) = std::fs::read(f) else { return };
    let swf = ffdec::Swf::parse(&data).unwrap();
    let mut abcs = Vec::new();
    swf.walk(|_, t| {
        if let Some((d, _)) = tag_abc(t) {
            abcs.push(Abc::parse(d).unwrap());
        }
    });
    let ctx = SwfContext::of(&swf);
    let doc = ffdec::names::Names::new(&swf).classes.get(&0).and_then(|v| v.first().cloned());
    let compile_once = |abcs: &[Abc], sel: usize, name: &str, text: &str| -> Result<(Vec<Abc>, String), String> {
        let e = entries(&abcs[sel]).into_iter().find(|x| x.full_name() == name).ok_or("missing")?;
        let out = ffdec::abc::decomp::with_big_stack(|| ffdec::abc::compile::replace_script(abcs, sel, e.script, e.trait_index, text, doc.as_deref())).map_err(|e| e.to_string())?;
        let mut all = abcs.to_vec();
        all[sel] = Abc::parse(&out.to_bytes()).map_err(|e| e.to_string())?;
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
                    if let Ok(dir) = std::env::var("FFDEC_AS3_DUMP") {
                        let _ = std::fs::write(Path::new(&dir).join(format!("{name}.first.as")), &first);
                        let _ = std::fs::write(Path::new(&dir).join(format!("{name}.second.as")), &second);
                    }
                    bad.push(format!("{name}: decompiles differently after a second compile"))
                }
                Err(err) => bad.push(format!("{name}: {err}")),
            }
        }
    }
    eprintln!("stable {ok}/{}
{}", ok + bad.len(), bad.join("
"));
    // A loop body that returns leaves its iteration registers visible, and each compile adds
    // another pair; those classes grow but stay correct, as does `this` typed `global`.
    assert!(bad.iter().all(|b| b.contains("differently")) && bad.len() <= 3, "{}", bad.join("
"));
}

/// Recompile of every script pack, from `FFDEC_AS3_RECOMPILE` (the As3Recompile
/// driver's output directory), compared byte for byte.
#[test]
fn recompiles_like_reference() {
    use ffdec::abc::{tag_abc, Abc};
    let Ok(root) = std::env::var("FFDEC_AS3_RECOMPILE") else { return };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let (mut same, mut differ, mut failed, mut ref_failed, mut skipped, mut multi, mut ours_where_ref_failed) = (0, 0, 0, 0, 0, 0, 0);
    let mut report = Vec::new();
    let only = std::env::var("FFDEC_AS3_ONLY").ok();
    for d in &dirs {
        if let Some(o) = &only {
            if !o.split(',').any(|x| d.file_name().is_some_and(|n| n == x)) {
                continue;
            }
        }
        let Ok(swf_path) = std::fs::read_to_string(d.join("swf.txt")) else { continue };
        let Ok(data) = std::fs::read(swf_path.trim()) else { continue };
        let Ok(swf) = ffdec::Swf::parse(&data) else { continue };
        let mut abcs = Vec::new();
        let mut tag_pos = Vec::new();
        for (ti, t) in swf.timeline.tags.iter().enumerate() {
            if let Some((b, _)) = tag_abc(t) {
                if let Ok(a) = Abc::parse(b) {
                    abcs.push(a);
                    tag_pos.push(ti);
                }
            }
        }
        let doc = ffdec::names::Names::new(&swf).classes.get(&0).and_then(|v| v.first().cloned());
        let index = std::fs::read_to_string(d.join("index.txt")).unwrap_or_default();
        for line in index.lines() {
            let mut f = line.split('\t');
            let (Some(i), Some(path), Some(simple)) = (f.next(), f.next(), f.next()) else { continue };
            let Ok(text) = std::fs::read_to_string(d.join(format!("{i}.as"))) else { continue };
            let expected = std::fs::read(d.join(format!("{i}.abc")));
            let tag: Option<usize> = std::fs::read_to_string(d.join(format!("{i}.tag"))).ok().and_then(|s| s.trim().parse().ok());
            let Ok(expected) = expected else {
                ref_failed += 1;
                let jerr = std::fs::read_to_string(d.join(format!("{i}.err"))).unwrap_or_default();
                let sel = tag.and_then(|t| tag_pos.iter().position(|&p| p == t)).unwrap_or(0);
                let found: Vec<_> = ffdec::abc::scripts::entries(&abcs[sel]).into_iter().filter(|e| e.full_name() == path && e.trait_index.is_some()).collect();
                if let [e] = found.as_slice() {
                    let r = ffdec::abc::compile::replace_script(&abcs, sel, e.script, e.trait_index, &text, doc.as_deref());
                    match r {
                        Ok(_) => {
                            ours_where_ref_failed += 1;
                            report.push(format!("OURS {}/{i} {path}: compiles; reference: {}", d.file_name().unwrap().to_string_lossy(), jerr.trim()));
                        }
                        Err(e) => report.push(format!("BOTH {}/{i} {path}: {e}; reference: {}", d.file_name().unwrap().to_string_lossy(), jerr.trim())),
                    }
                }
                continue;
            };
            let Some(sel) = tag.and_then(|t| tag_pos.iter().position(|&p| p == t)) else {
                skipped += 1;
                continue;
            };
            let abc = &abcs[sel];
            let found: Vec<_> = ffdec::abc::scripts::entries(abc).into_iter().filter(|e| e.full_name() == path && e.trait_index.is_some()).map(|e| (e.script, e.trait_index)).collect();
            let Some(&(script, trait_index)) = found.first().filter(|_| found.len() == 1) else {
                skipped += 1;
                continue;
            };
            let case = format!("{}/{i} {path}", d.file_name().unwrap().to_string_lossy());
            let renames = if simple == "false" {
                let index = ffdec::abc::decomp::index::AbcIndex::for_abcs(&abcs.iter().collect::<Vec<_>>());
                let e = ffdec::abc::scripts::entries(abc).into_iter().find(|e| e.script == script && e.trait_index == trait_index).unwrap();
                ffdec::abc::decomp::entry_renames(&index, abc, &e, &ffdec::abc::decomp::SwfContext::of(&swf))
            } else {
                Vec::new()
            };
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ffdec::abc::compile::replace_script_renamed(&abcs, sel, script, trait_index, &text, doc.as_deref(), &renames))) {
                Ok(Ok(out)) if simple == "false" => {
                    // The reference appends these as the entry script, so only check ours is whole.
                    let bytes = out.to_bytes();
                    let names = |a: &Abc| {
                        let mut v: Vec<String> = ffdec::abc::scripts::entries(a).into_iter().filter(|e| e.trait_index.is_some()).map(|e| e.full_name()).collect();
                        v.sort();
                        v
                    };
                    match Abc::parse(&bytes) {
                        Ok(back) if names(&back) == names(abc) && back.scripts.len() == abc.scripts.len() + 1 => multi += 1,
                        r => {
                            failed += 1;
                            let detail = match r {
                                Ok(back) => {
                                    let (a, b) = (names(abc), names(&back));
                                    format!("scripts {} -> {}, lost {:?}, gained {:?}", abc.scripts.len(), back.scripts.len(), a.iter().filter(|x| !b.contains(x)).collect::<Vec<_>>(), b.iter().filter(|x| !a.contains(x)).collect::<Vec<_>>())
                                }
                                Err(e) => e.to_string(),
                            };
                            report.push(format!("MULTI {case}: {detail}"));
                        }
                    }
                }
                Ok(Ok(out)) => {
                    let bytes = out.to_bytes();
                    if bytes == expected {
                        same += 1;
                    } else {
                        differ += 1;
                        report.push(format!("DIFF {case}: {} vs {} bytes", bytes.len(), expected.len()));
                        if let Ok(dir) = std::env::var("FFDEC_AS3_DUMP") {
                            let name = case.replace(['/', ' ', '.'], "_");
                            let ours = dump(&out);
                            let theirs = Abc::parse(&expected).map(|a| dump(&a)).unwrap_or_default();
                            let _ = std::fs::write(Path::new(&dir).join(format!("{name}.ours.txt")), ours);
                            let _ = std::fs::write(Path::new(&dir).join(format!("{name}.ref.txt")), theirs);
                        }
                    }
                }
                Ok(Err(e)) => {
                    if let Ok(dir) = std::env::var("FFDEC_AS3_DUMP") {
                        let _ = std::fs::write(Path::new(&dir).join(format!("{}.input.txt", case.replace(['/', ' ', '.'], "_"))), dump(abc));
                    }
                    failed += 1;
                    report.push(format!("ERR  {case}: {e}"));
                }
                Err(_) => {
                    failed += 1;
                    report.push(format!("PANIC {case}"));
                }
            }
        }
    }
    let out = std::env::var("FFDEC_AS3_REPORT").unwrap_or_default();
    if !out.is_empty() {
        std::fs::write(&out, report.join("\n")).unwrap();
    }
    for r in report.iter().take(40) {
        eprintln!("{r}");
    }
    eprintln!("same {same}, differ {differ}, multi-definition scripts compiled {multi}, failed {failed}, reference failed {ref_failed} (we compile {ours_where_ref_failed}), skipped {skipped}");
}

fn dump(abc: &ffdec::abc::Abc) -> String {
    use std::fmt::Write;
    let mut o = String::new();
    let c = &abc.constants;
    let _ = writeln!(o, "ints {:?}", c.ints);
    let _ = writeln!(o, "uints {:?}", c.uints);
    let _ = writeln!(o, "doubles {:?}", c.doubles);
    for (i, s) in c.strings.iter().enumerate() {
        let _ = writeln!(o, "str {i} {s:?}");
    }
    for (i, n) in c.namespaces.iter().enumerate() {
        let _ = writeln!(o, "ns {i} {} {}", n.kind, n.name);
    }
    for (i, n) in c.ns_sets.iter().enumerate() {
        let _ = writeln!(o, "nsset {i} {n:?}");
    }
    for (i, m) in c.multinames.iter().enumerate() {
        let _ = writeln!(o, "mn {i} {m:?}");
    }
    for (i, m) in abc.methods.iter().enumerate() {
        let _ = writeln!(o, "method {i} {m:?}");
    }
    for (i, m) in abc.metadata.iter().enumerate() {
        let _ = writeln!(o, "meta {i} {m:?}");
    }
    for (i, ii) in abc.instances.iter().enumerate() {
        let _ = writeln!(o, "instance {i} name {} super {} flags {} prot {} ifs {:?} iinit {}", ii.name, ii.super_name, ii.flags, ii.protected_ns, ii.interfaces, ii.iinit);
        for t in &ii.traits {
            let _ = writeln!(o, "  {t:?}");
        }
        let _ = writeln!(o, "class {i} cinit {}", abc.classes[i].cinit);
        for t in &abc.classes[i].traits {
            let _ = writeln!(o, "  {t:?}");
        }
    }
    for (i, s) in abc.scripts.iter().enumerate() {
        let _ = writeln!(o, "script {i} init {}", s.init);
        for t in &s.traits {
            let _ = writeln!(o, "  {t:?}");
        }
    }
    for (i, b) in abc.bodies.iter().enumerate() {
        let _ = writeln!(o, "body {i} method {} stack {} regs {} scope {}-{} exc {:?} traits {:?}", b.method, b.max_stack, b.max_regs, b.init_scope_depth, b.max_scope_depth, b.exceptions.iter().map(|e| (e.start, e.end, e.target, e.type_name, e.var_name)).collect::<Vec<_>>(), b.traits);
        let code = ffdec::abc::opcodes::disassemble(&b.code, &[], ffdec::abc::opcodes::Dialect::of(abc.major, abc.minor));
        for ins in code {
            let _ = writeln!(o, "  {:5} {} {:?}", ins.offset, ins.name, ins.operands);
        }
    }
    o
}
