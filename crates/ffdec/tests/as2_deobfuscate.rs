//! JPEXS's `ActionScript2DeobfuscatorTest`, with automatic deobfuscation on.

use ffdec::action::compile::{compile, Target};
use ffdec::action::decomp::actions_to_source;
use ffdec::action::{read_action_list, ActionList};

const VERSION: u8 = 10;

fn source(list: &ActionList) -> String {
    let bytes = list.to_bytes();
    ffdec::set_auto_deobfuscate(true);
    let read = read_action_list(&bytes, 0, bytes.len(), list.version);
    actions_to_source(&read, false, None, "", &Default::default()).replace("\r\n", "\n")
}

fn recompile(src: &str) -> String {
    source(&compile(src, VERSION, Target::Actions, &[], None).unwrap().actions)
}

fn decompile_pcode(pcode: &str) -> String {
    source(&ffdec::action::asm::parse_pcode(pcode, VERSION).unwrap()).trim().to_string()
}

#[test]
fn constant_conditions_are_removed() {
    for (expr, holds) in [("1!=5", true), ("5==5", true), ("1<4", true), ("5>4", true), ("5*6==30", true), ("1==5", false), ("5!=5", false), ("1>4", false), ("5<4", false), ("5*7==12", false)] {
        let (on_true, on_false) = if holds { ("OK", "FAIL") } else { ("FAIL", "OK") };
        let res = recompile(&format!("if({expr}){{trace(\"{on_true}\");}} else {{trace(\"{on_false}\");}}"));
        assert!(res.contains("\"OK\"") && !res.contains("\"FAIL\""), "{expr}:\n{res}");
    }
}

#[test]
fn eval_expression_after_while() {
    let res = recompile("var a = 5;while(true){if(a==73){a = 15;}if(a==1){trace(\"FAIL1\");}if(a==5){a=50;}if(a == 201){break;}a++;if(a == 53){a = a + 20;}if(a>500){trace(\"FAIL2\");}if(a==16){a = 200;}}if(a == 201){trace(\"OK\");}else{trace(\"FAIL3\");}");
    assert!(!res.contains("\"FAIL1\"") && !res.contains("\"FAIL2\"") && !res.contains("\"FAIL3\"") && res.contains("\"OK\""), "{res}");
}

#[test]
fn jumps_to_the_next_action_are_removed() {
    let res = decompile_pcode(
        "ConstantPool \"a\" \"b\" \"c\"\nPush false register1\nStoreRegister 2\nPop\nPush register2\nStoreRegister 0\nPush \"a\"\nStrictEquals\nIf loc005a\nPush register0 \"b\"\nStrictEquals\nIf loc0068\nJump loc0048;\nloc0048:Push register0 \"c\"\nStrictEquals\nIf loc0076\nJump loc0084\nloc005a:Push 1\nTrace\nJump loc0084\nloc0068:Push 2\nTrace\nJump loc0084\nloc0076:Push 3\nTrace\nJump loc0084\nloc0084:",
    );
    assert!(res.contains("case \"c\":"), "{res}");
}

#[test]
fn get_time_conditions_are_removed() {
    for pcode in [
        "ConstantPool \"a\"\nGetTime\nIf loc1\nPush \"FAIL\"\nTrace\nloc1:Push \"OK\"\nTrace\nloc2:",
        "ConstantPool \"a\"\nJump loc3\nloc4:Push 1\nTrace\nJump loc5\nloc3:GetTime\nIf loc1\nPush \"FAIL\"\nTrace\nloc1:Push \"OK\"\nTrace\nloc2:Jump loc4\nloc5:",
        "ConstantPool \"a\"\nGetTime\nIncrement\nIf loc1\nPush \"FAIL\"\nTrace\nloc1:Push \"OK\"\nTrace\nloc2:",
        "ConstantPool \"a\"\nJump loc3\nloc4: Push 1\nTrace\nJump loc5\nloc3:GetTime\nIncrement\nIf loc1\nPush \"FAIL\"\nTrace\nloc1:Push \"OK\"\nTrace\nloc2:Jump loc4\nloc5:\n",
    ] {
        let res = decompile_pcode(pcode);
        assert!(!res.contains("FAIL") && res.contains("OK"), "{pcode}\n{res}");
    }
}

#[test]
fn jump_after_function_end() {
    let res = decompile_pcode(
        "ConstantPool \"a\", \"b\", \"c\", \"d\"\nJump loc0026\nloc0012:Push \"once\"\nTrace\nJump loc0058\nloc0021:Jump loc0053\nloc0026:DefineFunction \"f\", 0 {\nPush \"a\"\nTrace\nPush register1\nPush 2\nEquals\nIf loc004d\nJump loc0021\nloc004d:Push \"b\"\nTrace\n}\nloc0053:Jump loc0012\nloc0058:Push \"c\"\nTrace",
    );
    assert_eq!(res.matches("\"once\"").count(), 1, "{res}");
}

#[test]
fn for_in_returns_with_jumps_end() {
    let res = decompile_pcode("Jump locA\nlocC: Not\nJump locD\nlocB: Equals2\nJump locC\nlocA: Push null\nJump locB\nlocD: If locA\nPush 1\nTrace\n");
    assert_eq!(res, "trace(1);");
}

#[test]
fn and_or_are_evaluated() {
    let and = "ConstantPool \"v\"\nPush \"v\"\nPush 1\nPush 2\nLess2\nPushDuplicate\nNot\nIf loc0036\nPop\nPush 2\nPush 8\nLess2\nloc0036:DefineLocal\n";
    let or = "ConstantPool \"v\"\nPush \"v\"\nPush 1\nPush 2\nLess2\nPushDuplicate\nIf loc0035\nPop\nPush 2\nPush 8\nLess2\nloc0035:DefineLocal\n";
    assert_eq!(decompile_pcode(and), "var v = true;");
    assert_eq!(decompile_pcode(or), "var v = true;");
}

#[test]
fn pop_must_stay_intact() {
    let expected = "test < 100 ? (test > -100 ? 0 : f()) : g();";
    assert_eq!(recompile(expected).trim(), expected);
}

#[test]
fn broken_scripts_are_detected() {
    assert!(ffdec::action::deobf::code_is_broken("\u{a7}\u{a7}push(1);"));
    assert!(!ffdec::action::deobf::code_is_broken("trace(1);"));
}
