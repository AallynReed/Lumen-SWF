//! JPEXS's `ActionScript3DeobfuscatorTest` p-code cases: remove traps, then decompile.

use std::collections::HashSet;
use std::path::Path;

use ffdec::abc::asm::assemble_method;
use ffdec::abc::compile::pool;
use ffdec::abc::decomp::ctx::{Config, Ctx};
use ffdec::abc::decomp::deobf;
use ffdec::abc::decomp::index::AbcIndex;
use ffdec::abc::decomp::method::{self, ConvertData, MethodReq};
use ffdec::abc::decomp::writer::Writer;
use ffdec::abc::pcode::method_pcode;
use ffdec::abc::{tag_abc, Abc, Class, Instance, MethodBody, MethodInfo, Multiname, Script};

fn clean(s: &str) -> String {
    s.replace("\r\n", "\n").lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).map(str::trim).collect::<Vec<_>>().join("\n")
}

/// The test's fresh ABC: method 0 with one parameter, used by a class `Test` and a script.
fn test_abc(pcode: &str) -> Abc {
    let mut abc = Abc::parse(&Abc::empty().to_bytes()).unwrap();
    abc.methods.push(MethodInfo { param_types: vec![0], return_type: 0, name: 0, flags: 0, optional: vec![], param_names: vec![] });
    abc.bodies.push(MethodBody { method: 0, max_stack: 1, max_regs: 2, init_scope_depth: 0, max_scope_depth: 1, code: vec![0x47], exceptions: vec![], traits: vec![] });
    abc.body_of = vec![Some(0)];
    let name = pool::str_id(&mut abc, "Test");
    let ns = pool::ns(&mut abc, ffdec::abc::ns_kind::PACKAGE, "");
    let qn = pool::mn(&mut abc, Multiname::QName { attr: false, ns, name });
    abc.instances.push(Instance { name: qn, super_name: 0, flags: 0, protected_ns: 0, interfaces: vec![], iinit: 0, traits: vec![] });
    abc.classes.push(Class { cinit: 0, traits: vec![] });
    abc.scripts.push(Script { init: 0, traits: vec![] });
    let text = method_pcode(&abc, 0);
    let start = text.find("      code\n").unwrap() + "      code\n".len();
    let end = text.find("      end ; code").unwrap();
    let text = format!("{}getlocal0\npushscope\n{pcode}returnvoid\n{}", &text[..start], &text[end..]);
    assemble_method(&mut abc, 0, None, &text).unwrap();
    abc
}

#[test]
fn as3_deobfuscator_cases() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib");
    let Ok(data) = std::fs::read(root.join("testdata/as3/as3.swf")) else {
        eprintln!("JPEXS test corpus not found; skipping");
        return;
    };
    let swf = ffdec::Swf::parse(&data).unwrap();
    let mut abcs = Vec::new();
    swf.walk(|_, t| {
        if let Some((d, _)) = tag_abc(t) {
            abcs.push(Abc::parse(d).unwrap());
        }
    });
    let index = AbcIndex::for_abcs(&abcs.iter().collect::<Vec<_>>());
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/as3_deobfuscate.json");
    let cases: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut failed = Vec::new();
    for c in cases.as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut abc = test_abc(c["pcode"].as_str().unwrap());
        assert!(deobf::remove_traps(&mut abc, 0, true, 0, -1), "{name}: remove traps failed");
        deobf::remove_labels_and_debug_line(&mut abc, 0);
        let mut ctx = Ctx::new(&abc, Config { deobfuscate: true, ..Config::default() }).with_index(index.clone());
        ctx.swf_version = 10;
        let mut req = MethodReq::new(0);
        req.script_index = 0;
        req.class_index = 0;
        req.init_traits = Some(&[]);
        let mut cd = ConvertData { deobfuscate: Some(false), ..ConvertData::default() };
        let seen = HashSet::new();
        let conv = method::convert(&ctx, &req, &mut cd, &seen).unwrap();
        let mut w = Writer::new();
        method::to_source(&ctx, &req, &conv, &seen, -1, &mut w);
        let actual = w.finish();
        let expected = c["expected"].as_str().unwrap();
        if clean(&actual) != clean(expected) {
            failed.push(format!("{name}\n--- expected\n{}\n--- actual\n{}", clean(expected), clean(&actual)));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
}

/// `recompile`: compiles a `Test.test()` body next to as3.swf's classes and decompiles it with
/// automatic deobfuscation.
fn recompile(body: &str) -> Option<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../jpexs-decompiler/libsrc/ffdec_lib");
    let swf = ffdec::Swf::parse(&std::fs::read(root.join("testdata/as3/as3.swf")).ok()?).unwrap();
    let mut abcs = Vec::new();
    swf.walk(|_, t| {
        if let Some((d, _)) = tag_abc(t) {
            abcs.push(Abc::parse(d).unwrap());
        }
    });
    let text = format!("package {{ public class Test {{  public static function trace(s){{ }} public static function test(){{ {body} }}   }}  }}");
    let e = ffdec::abc::scripts::entries(&abcs[0]).into_iter().find(|e| e.trait_index.is_some()).unwrap();
    let abc = ffdec::abc::compile::replace_script(&abcs, 0, e.script, e.trait_index, &text, None).unwrap();
    let mut all = abcs.clone();
    all[0] = abc;
    let index = AbcIndex::for_abcs(&all.iter().collect::<Vec<_>>());
    let e = ffdec::abc::scripts::entries(&all[0]).into_iter().find(|e| e.full_name() == "Test").unwrap();
    Some(ffdec::abc::decomp::entry_source_with(&index, &all[0], &e, &ffdec::abc::decomp::SwfContext::of(&swf), Config { deobfuscate: true, ..Config::default() }))
}

#[test]
fn constant_conditions_are_removed() {
    for (expr, holds) in [("1!=5", true), ("5==5", true), ("1<4", true), ("5>4", true), ("5*6==30", true), ("1==5", false), ("5!=5", false), ("1>4", false), ("5<4", false), ("5*7==12", false)] {
        let (on_true, on_false) = if holds { ("OK", "FAIL") } else { ("FAIL", "OK") };
        let Some(res) = recompile(&format!("if({expr}){{trace(\"{on_true}\");}} else {{trace(\"{on_false}\");}}")) else { return };
        assert!(res.contains("\"OK\"") && !res.contains("\"FAIL\""), "{expr}:\n{res}");
    }
}

#[test]
fn known_variables_are_removed() {
    let Some(res) = recompile("var a = true; var b = false;if(a){trace(\"OK1\");}else{trace(\"FAIL1\");}if(b){trace(\"FAIL2\");}else{trace(\"OK2\");}") else { return };
    for (s, want) in [("\"OK1\"", true), ("\"OK2\"", true), ("\"FAIL1\"", false), ("\"FAIL2\"", false), ("var ", false), ("if", false)] {
        assert_eq!(res.contains(s), want, "{s}:\n{res}");
    }
}

/// `recompilePCode`: the code section after one deobfuscator, one instruction per line.
fn recompile_pcode(pcode: &str, pass: fn(&mut Abc, usize)) -> String {
    let mut abc = test_abc(pcode);
    pass(&mut abc, 0);
    let text = method_pcode(&abc, 0);
    let start = text.find("      code\n").unwrap() + "      code\n".len();
    let end = text.find("      end ; code").unwrap();
    text[start..end].lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| format!("{l}\n")).collect()
}

#[test]
fn jumps_to_jumps_are_redirected() {
    let res = recompile_pcode("pushbyte 3\npushbyte 4\nifeq a\njump b\na:jump c\nc:pushbyte 4\nb:pushbyte 3\n", deobf::jumps_in_place);
    assert_eq!(res, "getlocal0\npushscope\npushbyte 3\npushbyte 4\nifeq ofs000e\njump ofs0010\nofs000e:\npushbyte 4\nofs0010:\npushbyte 3\nreturnvoid\n");
}

#[test]
fn parts_are_grouped() {
    let group = |abc: &mut Abc, m: usize| {
        assert!(deobf::group_parts_in_place(abc, m));
    };
    let res = recompile_pcode("pushbyte 1\npop\njump A\nB:pushbyte 3\npop\njump C\nA:pushbyte 2\npop\njump B\nC:pushbyte 4\npop\n", group);
    assert_eq!(res, "getlocal0\npushscope\npushbyte 1\npop\npushbyte 2\npop\npushbyte 3\npop\npushbyte 4\npop\nreturnvoid\n");
}
