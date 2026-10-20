use ffdec::abc::asm::assemble_instructions;
use ffdec::abc::opcodes::{disassemble, Dialect};
use ffdec::abc::{tag_abc, Abc};
use ffdec::tags::code;
use ffdec::Swf;

#[test]
fn test_as3_assemble_simple_instructions() {
    let abc = Abc::empty();
    let pcode = r#"
getlocal0
pushscope
returnvoid
"#;
    let code = assemble_instructions(pcode, &abc).unwrap();
    assert_eq!(code, vec![0xd0, 0x30, 0x47]);

    let disassembled = disassemble(&code, &[], Dialect::default());
    assert_eq!(disassembled.len(), 3);
    assert_eq!(disassembled[0].name, "getlocal0");
    assert_eq!(disassembled[1].name, "pushscope");
    assert_eq!(disassembled[2].name, "returnvoid");
}

#[test]
fn test_as3_assemble_jump_and_labels() {
    let abc = Abc::empty();
    let pcode = r#"
ofs0000:
getlocal0
pushscope
pushtrue
iffalse ofs000a
returnvoid
ofs000a:
returnvoid
"#;
    let code = assemble_instructions(pcode, &abc).unwrap();
    let disassembled = disassemble(&code, &[], Dialect::default());
    assert_eq!(disassembled.len(), 6);
    assert_eq!(disassembled[3].name, "iffalse");
    let iffalse_target = (disassembled[3].offset + disassembled[3].len) as i64 + disassembled[3].operands[0];
    assert_eq!(iffalse_target, disassembled[5].offset as i64);
}

#[test]
fn test_abc_write_roundtrip() {
    let playerglobal = include_bytes!("../resources/playerglobal.swf");
    let swf = Swf::parse(playerglobal).unwrap();
    let abc_tag = swf.timeline.tags.iter().find(|t| t.code == code::DO_ABC || t.code == code::DO_ABC2).unwrap();
    let (abc_bytes, _) = tag_abc(abc_tag).unwrap();

    let abc = Abc::parse(abc_bytes).unwrap();
    let written = abc.to_bytes();

    let reparsed = Abc::parse(&written).unwrap();
    assert_eq!(reparsed.major, abc.major);
    assert_eq!(reparsed.minor, abc.minor);
    assert_eq!(reparsed.constants.strings.len(), abc.constants.strings.len());
    assert_eq!(reparsed.methods.len(), abc.methods.len());
    assert_eq!(reparsed.classes.len(), abc.classes.len());
    assert_eq!(reparsed.scripts.len(), abc.scripts.len());
    assert_eq!(reparsed.bodies.len(), abc.bodies.len());
}
