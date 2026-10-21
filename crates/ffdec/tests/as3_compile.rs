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
