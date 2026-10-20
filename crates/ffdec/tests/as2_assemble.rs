use ffdec::action::asm::parse_pcode;
use ffdec::action::{Op, Value};

#[test]
fn pool_strings_become_constants() {
    let list = parse_pcode("ConstantPool \"a\", \"b\"\nPush \"b\", \"c\" ; comment\nTrace\n", 8).unwrap();
    let Op::Push { values, .. } = &list.actions[1].op else { panic!("push expected") };
    assert_eq!(values, &[Value::Constant(1), Value::Str("c".into())]);
    assert!(matches!(list.actions.last().unwrap().op, Op::End));
}

#[test]
fn errors_name_their_line() {
    let e = parse_pcode("Push 1\nIf nowhere\n", 8).unwrap_err();
    assert_eq!((e.line, e.message.as_str()), (2, "Target not found: nowhere"));
    let e = parse_pcode("Stop\nFrobnicate\n", 8).unwrap_err();
    assert_eq!(e.line, 2);
}
