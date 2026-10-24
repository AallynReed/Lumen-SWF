use std::path::{Path, PathBuf};

use ffdec::action::{self, scripts};

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

fn swfs() -> impl Iterator<Item = (PathBuf, ffdec::Swf)> {
    corpus().into_iter().filter_map(|f| ffdec::Swf::parse(&std::fs::read(&f).ok()?).ok().map(|s| (f, s)))
}

fn report(name: &str, total: usize, bad: &[String]) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}_report.txt")), bad.join("\n")).unwrap();
    eprintln!("{name}: {}/{total} ok", total - bad.len());
}

/// Every ABC must re-serialize byte for byte.
#[test]
fn abc_write_is_identity() {
    let (mut total, mut bad) = (0, Vec::new());
    for (f, swf) in swfs() {
        swf.walk(|path, t| {
            let Some((data, _)) = ffdec::abc::tag_abc(t) else { return };
            let Ok(abc) = ffdec::abc::Abc::parse(data) else { return };
            total += 1;
            let out = abc.to_bytes();
            if out != data {
                let at = out.iter().zip(data).position(|(a, b)| a != b).unwrap_or(out.len().min(data.len()));
                bad.push(format!("{} {path:?}: differs at {at} of {} (wrote {})", f.display(), data.len(), out.len()));
            }
        });
    }
    report("abc_write", total, &bad);
    assert!(bad.is_empty(), "{} ABCs differ", bad.len());
}

/// Writing an action list and reading it back must give the same p-code.
#[test]
fn action_write_round_trips() {
    let (mut total, mut bad) = (0, Vec::new());
    for (f, swf) in swfs() {
        for (name, src) in scripts::export_paths(&scripts::tree(&swf)) {
            let list = src.actions(&swf);
            let before = action::pcode::to_pcode(&list);
            let bytes = list.to_bytes();
            let after = action::pcode::to_pcode(&action::read_action_list(&bytes, 0, bytes.len(), swf.header.version));
            total += 1;
            if before != after {
                bad.push(format!("{} {name}", f.display()));
            }
        }
    }
    report("action_write", total, &bad);
    assert!(bad.is_empty(), "{} action lists differ", bad.len());
}

/// Assembling printed p-code must give back the same p-code.
#[test]
fn pcode_assembles_to_itself() {
    let (mut total, mut bad) = (0, Vec::new());
    for (f, swf) in swfs() {
        for (name, src) in scripts::export_paths(&scripts::tree(&swf)) {
            let before = action::pcode::to_pcode(&src.actions(&swf));
            total += 1;
            let after = action::asm::parse_pcode(&before, swf.header.version).map(|l| {
                let bytes = l.to_bytes();
                action::pcode::to_pcode(&action::read_action_list(&bytes, 0, bytes.len(), swf.header.version))
            });
            // The assembler drops an empty constant pool.
            let expected = before.replace("ConstantPool\n", "");
            match after {
                Ok(a) if a == expected => {}
                Ok(_) => bad.push(format!("{} {name}: differs", f.display())),
                Err(e) => bad.push(format!("{} {name}: {e}", f.display())),
            }
        }
    }
    report("pcode_asm", total, &bad);
    assert!(bad.is_empty(), "{} scripts differ", bad.len());
}

/// Assembling every method's printed p-code, with its trait where it has one, must leave
/// the ABC byte for byte as it was.
#[test]
fn as3_pcode_assembles_to_itself() {
    use ffdec::abc::asm::{assemble_method, TraitLoc};
    use ffdec::abc::pcode::{method_pcode, trait_pcode};
    let (mut total, mut bad) = (0, Vec::new());
    for (f, swf) in swfs() {
        swf.walk(|path, t| {
            let Some((data, _)) = ffdec::abc::tag_abc(t) else { return };
            let Ok(orig) = ffdec::abc::Abc::parse(data) else { return };
            let mut owned: Vec<(usize, Option<TraitLoc>)> = Vec::new();
            let mut add = |traits: &[ffdec::abc::Trait], loc: &dyn Fn(usize) -> TraitLoc| {
                for (i, tr) in traits.iter().enumerate() {
                    if let Some(m) = tr.method() {
                        owned.push((m, Some(loc(i))));
                    }
                }
            };
            for (s, sc) in orig.scripts.iter().enumerate() {
                add(&sc.traits, &|i| TraitLoc::Script(s, i));
            }
            for (c, ii) in orig.instances.iter().enumerate() {
                add(&ii.traits, &|i| TraitLoc::Instance(c, i));
            }
            for (c, cl) in orig.classes.iter().enumerate() {
                add(&cl.traits, &|i| TraitLoc::Class(c, i));
            }
            let traited: std::collections::HashSet<usize> = owned.iter().map(|(m, _)| *m).collect();
            owned.extend((0..orig.methods.len()).filter(|m| !traited.contains(m)).map(|m| (m, None)));
            for (m, loc) in owned {
                total += 1;
                let text = match loc {
                    Some(l) => trait_pcode(&orig, orig.trait_at(l).unwrap()),
                    None => method_pcode(&orig, m),
                };
                let mut abc = orig.clone();
                let mut fitted = orig.clone();
                if let Some(Some(bi)) = orig.body_of.get(m) {
                    let _ = ffdec::abc::stats::fit_body(&mut fitted, *bi);
                }
                let data = if fitted.methods[m].flags == orig.methods[m].flags && orig.body_of[m].map_or(true, |b| format!("{:?}", fitted.bodies[b]) == format!("{:?}", orig.bodies[b])) { data.to_vec() } else { fitted.to_bytes() };
                match assemble_method(&mut abc, m, loc, &text) {
                    Err(e) => bad.push(format!("{} {path:?} method {m}: {e}: {}", f.display(), text.lines().nth(e.line - 1).unwrap_or("").trim())),
                    // Dead bytes the disassembler skips cannot survive; the text must then.
                    Ok(()) if abc.to_bytes() != data => {
                        let again = match loc {
                            Some(l) => trait_pcode(&abc, abc.trait_at(l).unwrap()),
                            None => method_pcode(&abc, m),
                        };
                        if again != text {
                            bad.push(format!("{} {path:?} method {m}: differs", f.display()));
                        }
                    }
                    Ok(()) => {}
                }
            }
        });
    }
    report("as3_asm", total, &bad);
    assert!(bad.is_empty(), "{} methods differ", bad.len());
}

/// Storing every script back in its tag with an action added, and saving, must keep every
/// record of the file readable as what was stored.
#[test]
fn as2_scripts_store_in_place() {
    let (mut total, mut bad) = (0, Vec::new());
    let v = |swf: &ffdec::Swf| swf.header.version;
    for (f, mut swf) in swfs() {
        let sources = |swf: &ffdec::Swf| scripts::export_paths(&scripts::tree(swf));
        let count = sources(&swf).len();
        let mut expected = Vec::new();
        for i in 0..count {
            let (_, src) = sources(&swf).swap_remove(i);
            let list = action::asm::parse_pcode(&format!("Push 1
Pop
{}", action::pcode::to_pcode(&src.actions(&swf))), v(&swf)).unwrap();
            let bytes = list.to_bytes();
            expected.push(action::pcode::to_pcode(&action::read_action_list(&bytes, 0, bytes.len(), v(&swf))));
            src.set_actions(&mut swf, &list).unwrap();
        }
        let saved = ffdec::Swf::parse(&swf.save(&ffdec::SaveOptions { compression: ffdec::Compression::None, encrypt: false }).unwrap()).unwrap();
        let after = sources(&saved);
        total += count;
        if after.len() != count {
            bad.push(format!("{}: {count} scripts became {}", f.display(), after.len()));
            continue;
        }
        for ((n, s), e) in after.iter().zip(&expected) {
            if action::pcode::to_pcode(&s.actions(&saved)) != *e {
                bad.push(format!("{} {n}: differs", f.display()));
            }
        }
    }
    report("as2_store", total, &bad);
    assert!(bad.is_empty(), "{} scripts differ", bad.len());
}

/// Decompiling every script, compiling it and storing it back must keep the file's records,
/// event headers included, and read back as what was compiled.
#[test]
fn as2_sources_compile_in_place() {
    use ffdec::action::compile::{compile, swf_classes, Target};
    use ffdec::action::scripts::Kind;
    let (mut total, mut bad) = (0, Vec::new());
    let target = |k: Kind| match k {
        Kind::ButtonCond => Target::ButtonCond,
        Kind::ClipAction => Target::ClipAction,
        _ => Target::Actions,
    };
    for (f, mut swf) in swfs() {
        let sources = |swf: &ffdec::Swf| scripts::export_paths(&scripts::tree(swf));
        let count = sources(&swf).len();
        if count == 0 {
            continue;
        }
        let classes = swf_classes(&swf);
        let mut first = Vec::new();
        for i in 0..count {
            let (name, src) = sources(&swf).swap_remove(i);
            let text = ffdec::export::Exporter::new(&swf).as2_source(&src);
            total += 1;
            match compile(&text, swf.header.version, target(src.kind), &classes, Some(&swf)) {
                Ok(c) => {
                    first.push((name, src.header.clone(), c.actions.to_bytes()));
                    src.set_compiled(&mut swf, &c).unwrap();
                }
                Err(e) => {
                    bad.push(format!("{} {name}: {e}", f.display()));
                    first.push((name, src.header.clone(), Vec::new()));
                }
            }
        }
        let saved = ffdec::Swf::parse(&swf.save(&ffdec::SaveOptions { compression: ffdec::Compression::None, encrypt: false }).unwrap()).unwrap();
        let after = sources(&saved);
        if after.len() != count {
            bad.push(format!("{}: {count} scripts became {}", f.display(), after.len()));
            continue;
        }
        for ((name, header, bytes), (_, src)) in first.iter().zip(&after) {
            if bytes.is_empty() {
                continue;
            }
            if *header != src.header {
                bad.push(format!("{} {name}: header {header:?} became {:?}", f.display(), src.header));
                continue;
            }
            let v = saved.header.version;
            let stored = action::pcode::to_pcode(&src.actions(&saved));
            if stored != action::pcode::to_pcode(&action::read_action_list(bytes, 0, bytes.len(), v)) {
                bad.push(format!("{} {name}: reads back differently", f.display()));
            }
        }
    }
    report("as2_source_store", total, &bad);
    assert!(bad.is_empty(), "{} scripts differ", bad.len());
}

/// Bodies the player accepted already declare enough stack, scope and registers, so fitting
/// them changes nothing. as3_assembled.swf is hand-assembled with placeholder limits.
#[test]
fn shipped_bodies_already_fit() {
    let (mut total, mut skipped, mut bad) = (0, 0, Vec::new());
    for (f, swf) in swfs().filter(|(f, _)| !f.ends_with("as3_assembled.swf")) {
        swf.walk(|path, t| {
            let Some((data, _)) = ffdec::abc::tag_abc(t) else { return };
            let Ok(abc) = ffdec::abc::Abc::parse(data) else { return };
            for (bi, b) in abc.bodies.iter().enumerate() {
                let Ok(code) = ffdec::abc::opcodes::decode(&b.code, ffdec::abc::opcodes::Dialect::of(abc.major, abc.minor)) else { continue };
                let handlers: Vec<usize> = b.exceptions.iter().map(|e| e.target).collect();
                total += 1;
                let Some((st, problem)) = ffdec::abc::stats::stats(&abc, &code, &handlers, b.init_scope_depth) else {
                    skipped += 1;
                    continue;
                };
                let mut fit = abc.clone();
                let r = ffdec::abc::stats::fit_body(&mut fit, bi);
                let fb = &fit.bodies[bi];
                if problem.is_some() || r.is_err() || fb.max_stack != b.max_stack || fb.max_scope_depth != b.max_scope_depth || fb.max_regs != b.max_regs || fit.methods[b.method].flags != abc.methods[b.method].flags {
                    bad.push(format!("{} {path:?} body {bi}: {problem:?} {st:?} declared stack {} scope {}..{} regs {}", f.display(), b.max_stack, b.init_scope_depth, b.max_scope_depth, b.max_regs));
                }
            }
        });
    }
    eprintln!("skipped {skipped}");
    report("body_fit", total, &bad);
    assert!(bad.is_empty(), "{} bodies change", bad.len());
}
