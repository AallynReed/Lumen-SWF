use ffdec::action::asm::parse_pcode;
use ffdec::action::pcode::action_source;
use ffdec::action::read_action_list;

#[test]
fn test_as2_assemble_simple() {
    let pcode = r#"
Push "Hello", 42
Trace
Stop
"#;
    let list = parse_pcode(pcode, 8).unwrap();
    assert_eq!(list.actions.len(), 3);
    assert_eq!(list.actions[0].code, 0x96);
    assert_eq!(list.actions[1].code, 0x26); // Trace
    assert_eq!(list.actions[2].code, 0x07); // Stop

    let bytes = list.to_bytes();
    let read_back = read_action_list(&bytes, 0, bytes.len(), 8);
    for a in &read_back.actions {
        println!("{}: {}", a.address, action_source(a));
    }
    assert_eq!(action_source(&read_back.actions[1]), "Trace");
    assert_eq!(action_source(&read_back.actions[2]), "Stop");
}

#[test]
fn test_as2_assemble_jump_and_labels() {
    let pcode = r#"
loc0000:
Push 1
If loc0010
Push "no"
Trace
Jump loc0015
loc0010:
Push "yes"
Trace
loc0015:
Stop
"#;
    let list = parse_pcode(pcode, 8).unwrap();
    let bytes = list.to_bytes();
    let read_back = read_action_list(&bytes, 0, bytes.len(), 8);
    for a in &read_back.actions {
        println!("{}: {}", a.address, action_source(a));
    }

    assert_eq!(read_back.actions.len(), 9);

    // If should jump forward to the Push "yes" instruction
    let if_target = read_back.actions[1].target().unwrap();
    let yes_push_addr = read_back.actions[5].address;
    assert_eq!(if_target, yes_push_addr);

    // Jump should jump to Stop
    let jump_target = read_back.actions[4].target().unwrap();
    let stop_addr = read_back.actions[7].address;
    assert_eq!(jump_target, stop_addr);
}
