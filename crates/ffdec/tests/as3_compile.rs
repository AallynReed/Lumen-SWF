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

/// JPEXS's own recompile of every script pack, from `FFDEC_AS3_RECOMPILE` (the As3Recompile
/// driver's output directory), compared byte for byte.
#[test]
fn recompiles_like_ffdec() {
    use ffdec::abc::{tag_abc, Abc};
    let Ok(root) = std::env::var("FFDEC_AS3_RECOMPILE") else { return };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let (mut same, mut differ, mut failed, mut ffdec_failed, mut skipped) = (0, 0, 0, 0, 0);
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
            let (Some(i), Some(path), Some(_simple)) = (f.next(), f.next(), f.next()) else { continue };
            let Ok(text) = std::fs::read_to_string(d.join(format!("{i}.as"))) else { continue };
            let expected = std::fs::read(d.join(format!("{i}.abc")));
            let tag: Option<usize> = std::fs::read_to_string(d.join(format!("{i}.tag"))).ok().and_then(|s| s.trim().parse().ok());
            let Ok(expected) = expected else {
                ffdec_failed += 1;
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
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ffdec::abc::compile::replace_script(&abcs, sel, script, trait_index, &text, doc.as_deref()))) {
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
                            let _ = std::fs::write(Path::new(&dir).join(format!("{name}.ffdec.txt")), theirs);
                        }
                    }
                }
                Ok(Err(e)) => {
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
    eprintln!("same {same}, differ {differ}, failed {failed}, ffdec failed {ffdec_failed}, skipped {skipped}");
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
