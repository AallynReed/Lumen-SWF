//! Port of `flex2.compiler.mxml.lang.TypeCompatibility`.

use crate::asc::flex2::compiler::mxml::lang::standard_defs::StandardDefs;
use crate::asc::flex2::compiler::mxml::reflect::type_table::{type_equals, TypeRef, TypeTable, VECTOR};

pub const OK: i32 = 0;
pub const OK_COERCE_TO_ARRAY: i32 = 1;
pub const OK_COERCE_TO_VECTOR: i32 = 2;
pub const ERR_RTYPE_NOT_ASSIGNABLE_TO_LTYPE: i32 = 3;
pub const ERR_LTYPE_NOT_MULTIPLE: i32 = 4;
pub const ERR_SINGLE_RVALUE_NOT_ARRAY_OR_ARRAY_ELEM: i32 = 5;
pub const ERR_MULTI_RVALUE_NOT_ARRAY_ELEM: i32 = 6;

/// `check(lvalueType, lvalueElementType, rvalueType, rvalueIsSingleton, standardDefs)`.
pub fn check(lvalue_type: &TypeRef, lvalue_element_type: Option<&TypeRef>, rvalue_type: Option<&TypeRef>, rvalue_is_singleton: bool, standard_defs: &StandardDefs) -> i32 {
    let type_table = lvalue_type.get_type_table();
    let array_type = type_table.array_type();
    if *array_type == **lvalue_type {
        if rvalue_is_singleton {
            if rvalue_type.is_some_and(|r| r.is_assignable_to(Some(&*array_type))) {
                OK
            } else if check_singleton(&type_table, lvalue_element_type, rvalue_type, standard_defs) {
                OK_COERCE_TO_ARRAY
            } else {
                ERR_SINGLE_RVALUE_NOT_ARRAY_OR_ARRAY_ELEM
            }
        } else if check_singleton(&type_table, lvalue_element_type, rvalue_type, standard_defs) {
            OK_COERCE_TO_ARRAY
        } else {
            ERR_MULTI_RVALUE_NOT_ARRAY_ELEM
        }
    } else if lvalue_type.get_name().starts_with(VECTOR) {
        if rvalue_is_singleton {
            if type_equals(rvalue_type, Some(lvalue_type)) {
                OK
            } else if check_singleton(&type_table, lvalue_element_type, rvalue_type, standard_defs) {
                OK_COERCE_TO_VECTOR
            } else {
                ERR_SINGLE_RVALUE_NOT_ARRAY_OR_ARRAY_ELEM
            }
        } else if check_singleton(&type_table, lvalue_element_type, rvalue_type, standard_defs) {
            OK_COERCE_TO_VECTOR
        } else {
            ERR_MULTI_RVALUE_NOT_ARRAY_ELEM
        }
    } else if array_type.is_assignable_to(Some(&**lvalue_type)) {
        if rvalue_is_singleton {
            if check_singleton(&type_table, Some(lvalue_type), rvalue_type, standard_defs) {
                OK
            } else {
                ERR_RTYPE_NOT_ASSIGNABLE_TO_LTYPE
            }
        } else if check_singleton(&type_table, lvalue_element_type, rvalue_type, standard_defs) {
            OK_COERCE_TO_ARRAY
        } else {
            ERR_MULTI_RVALUE_NOT_ARRAY_ELEM
        }
    } else if !rvalue_is_singleton {
        ERR_LTYPE_NOT_MULTIPLE
    } else if check_singleton(&type_table, Some(lvalue_type), rvalue_type, standard_defs) {
        OK
    } else {
        ERR_RTYPE_NOT_ASSIGNABLE_TO_LTYPE
    }
}

fn check_singleton(type_table: &TypeTable, ltype: Option<&TypeRef>, rtype: Option<&TypeRef>, standard_defs: &StandardDefs) -> bool {
    let Some(rtype) = rtype else { return false };
    rtype.is_assignable_to(ltype.map(|l| &**l)) || (standard_defs.is_ifactory(ltype.expect("NullPointerException")) && **rtype == *type_table.class_type())
}
