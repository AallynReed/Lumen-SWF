use std::path::Path;

use ffdec::abc::asm::assemble_method;
use ffdec::abc::pcode::method_pcode;
use ffdec::abc::{tag_abc, Abc};

fn sample() -> Option<Abc> {
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib/testdata/as3_new/bin/as3_new.flex.swf");
    let swf = ffdec::Swf::parse(&std::fs::read(f).ok()?).ok()?;
    let mut abc = None;
    swf.walk(|_, t| {
        if abc.is_none() {
            abc = tag_abc(t).and_then(|(d, _)| Abc::parse(d).ok());
        }
    });
    abc
}

fn with_code(abc: &Abc, m: usize, code: &str) -> String {
    let text = method_pcode(abc, m);
    let start = text.find("      code\n").unwrap() + "      code\n".len();
    let end = text.find("      end ; code").unwrap();
    format!("{}{code}\n{}", &text[..start], &text[end..])
}

#[test]
fn new_constants_are_added() {
    let Some(mut abc) = sample() else { return };
    let m = abc.bodies[0].method;
    let strings = abc.constants.strings.len();
    let text = with_code(&abc, m, "pushstring \"brand new\"\npop\nreturnvoid");
    assemble_method(&mut abc, m, None, &text).unwrap();
    assert_eq!(abc.constants.strings.len(), strings + 1);
    assert!(method_pcode(&abc, m).contains("pushstring \"brand new\""));
}

#[test]
fn errors_leave_the_abc_alone() {
    let Some(mut abc) = sample() else { return };
    let m = abc.bodies[0].method;
    let before = abc.to_bytes();
    let text = with_code(&abc, m, "returnvoid\nfrobnicate");
    let e = assemble_method(&mut abc, m, None, &text).unwrap_err();
    assert_eq!(text.lines().nth(e.line - 1).map(str::trim), Some("frobnicate"));
    assert_eq!(abc.to_bytes(), before);
}
