//! Java-checked behaviour of rep's helpers (expected values printed by JDK 17 and Flex 4.8's `swfutils.jar`).

use super::binding_expression::integer_hash_map_order;
use super::init::value_initializer::format_string;
use super::model::java_double_to_string;
use crate::asc::flex2::compiler::mxml::gen::line_number_map::{DualModeLineNumberMap, LineNumberMap};

#[test]
fn double_to_string_matches_java() {
    let cases: &[(f64, &str)] = &[
        (1.5, "1.5"),
        (0.1, "0.1"),
        (100.0, "100.0"),
        (1e7, "1.0E7"),
        (1.234e-5, "1.234E-5"),
        (123456789.0, "1.23456789E8"),
        (0.001, "0.001"),
        (9999999.0, "9999999.0"),
        (1e21, "1.0E21"),
        (2e-3, "0.002"),
        (3.0e10, "3.0E10"),
        (-0.5, "-0.5"),
        (1.0 / 3.0, "0.3333333333333333"),
        (0.0001, "1.0E-4"),
    ];
    for (d, s) in cases {
        assert_eq!(java_double_to_string(*d), *s);
    }
}

#[test]
fn integer_hash_map_order_matches_java() {
    let keys = [0, 17, 1, 33, 16, 2, 5, 40, 3, 100, 12, 13, 14, 15];
    let entries: Vec<(i32, String)> = keys.iter().map(|k| (*k, format!("u{k}"))).collect();
    let order: Vec<i32> = integer_hash_map_order(&entries).into_iter().map(|(k, _)| k).collect();
    assert_eq!(order, [0, 1, 33, 2, 3, 100, 5, 40, 12, 13, 14, 15, 16, 17]);
}

#[test]
fn format_string_matches_java() {
    assert_eq!(format_string("a\"b\\c\\u0041\t\n\u{1}z"), "\"a\\\"b\\\\c\\u0041\\t\\n\\x1Xz\"");
}

#[test]
fn line_number_map_lookups() {
    let mut m = LineNumberMap::new(Some("a"), Some("b"));
    m.put(10, 1, 3);
    m.put(20, 5, 1);
    assert_eq!(m.get(2), 11);
    assert_eq!(m.get(4), 0);
    assert_eq!(m.get(5), 20);
    let d = DualModeLineNumberMap::new(None, None);
    d.put(7, 3, 1, true);
    d.put(8, 3, 1, false);
    assert_eq!(d.get(3), 7);
    d.flush_temp();
    assert_eq!(d.get(3), 8);
}
