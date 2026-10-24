//! Port of `macromedia/asc/embedding/avmplus/GlobalBuilder.java`: builds global objects, whose prototype holds the
//! operator slots and the attribute names.

use crate::asc::emit::runtime_constants::*;
use crate::asc::semantics::builder::{Builder, BuilderKind};
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::semantics::slot::*;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::value::Value;
use crate::asc::util::context::Context;
use crate::asc::util::namespaces::Namespaces;

/// One shared global prototype instead of one per global object.
pub const USE_STATIC_BUILTINS: bool = true;

pub fn new_global_builder() -> Builder {
    Builder::from_kind(BuilderKind::Global { is_in_package: false })
}

pub(crate) fn build(b: &Builder, cx: &Context, ob: &ObjectValue) {
    b.bind(cx, ob);

    // The builtin types must exist, in this order, before global.abc is processed.
    cx.type_type();
    cx.no_type();
    cx.object_type();
    cx.array_type();
    cx.function_type();
    cx.uint_type();
    cx.string_type();
    cx.number_type();
    cx.double_type();
    if cx.statics.borrow().es4_numerics {
        cx.decimal_type();
    }
    cx.null_type();
    cx.void_type();
    cx.xml_type();
    cx.xml_list_type();
    cx.reg_exp_type();
    cx.vector_type();
    cx.vector_obj_type();

    if USE_STATIC_BUILTINS {
        let existing = cx.statics.borrow().global_prototype.clone();
        let proto = match existing {
            Some(p) => p,
            None => {
                let p = ObjectValue::new();
                cx.statics.borrow_mut().global_prototype = Some(p.clone());
                init_global_object(b, cx, &p);
                p
            }
        };
        ob.set_proto(Some(proto));
    } else {
        init_global_object(b, cx, ob);
    }

    // _cv holds the value the script ends with; the slot gives it a type to reason about.
    b.implicit_var(cx, ob, "_cv", &ObjectValue::internal_namespace(), Some(&cx.no_type()), SLOT_Global__cv, -1, -1);

    let mut d = b.data_mut();
    d.var_offset = 0;
    d.reg_offset = 1;
}

fn init_global_object(b: &Builder, cx: &Context, ob: &ObjectValue) {
    let es4 = cx.statics.borrow().es4_numerics;
    let t_boolean = cx.boolean_type();
    let t_none = cx.no_type();
    let t_string = cx.string_type();
    let t_int = cx.int_type();
    let t_uint = cx.uint_type();
    let t_double = cx.double_type();
    let t_void = cx.void_type();
    let t_decimal = || cx.decimal_type().expect("NullPointerException");
    let (bo, no, st, it, ui, db) = (Some(&t_boolean), Some(&t_none), Some(&t_string), Some(&t_int), Some(&t_uint), Some(&t_double));

    // delete
    b.unary_operator(cx, ob, bo, CALL_ThisUnary, SLOT_Global_DeleteOp);

    // void
    b.unary_operator(cx, ob, no, CALL_Unary, SLOT_Global_VoidOp);

    // typeof
    b.unary_operator(cx, ob, st, CALL_Unary, SLOT_Global_TypeofOp);
    b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_boolean, CALL_Empty, SLOT_Global_TypeofOp_B);
    b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_int, CALL_Empty, SLOT_Global_TypeofOp_I);
    b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_uint, CALL_Empty, SLOT_Global_TypeofOp_U);
    b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_double, CALL_Empty, SLOT_Global_TypeofOp_D);
    if es4 {
        b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_decimal(), CALL_Empty, SLOT_Global_TypeofOp_M);
    }
    b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_string, CALL_Empty, SLOT_Global_TypeofOp_S);
    b.unary_overload(cx, ob, SLOT_Global_TypeofOp, st, &t_void, CALL_Empty, SLOT_Global_TypeofOp_U);

    // ++
    b.unary_operator(cx, ob, db, CALL_Unary, SLOT_Global_IncrementOp);
    b.unary_overload(cx, ob, SLOT_Global_IncrementOp, it, &t_int, CALL_Empty, SLOT_Global_IncrementOp_I);
    if es4 {
        b.unary_overload(cx, ob, SLOT_Global_IncrementOp, Some(&t_decimal()), &t_decimal(), CALL_Empty, SLOT_Global_IncrementOp);
    }
    b.unary_operator(cx, ob, db, CALL_Unary, SLOT_Global_IncrementLocalOp);
    b.unary_overload(cx, ob, SLOT_Global_IncrementLocalOp, it, &t_int, CALL_Empty, SLOT_Global_IncrementLocalOp_I);
    if es4 {
        b.unary_overload(cx, ob, SLOT_Global_IncrementLocalOp, Some(&t_decimal()), &t_decimal(), CALL_Empty, SLOT_Global_IncrementLocalOp);
    }

    // --
    b.unary_operator(cx, ob, db, CALL_Unary, SLOT_Global_DecrementOp);
    if es4 {
        b.unary_overload(cx, ob, SLOT_Global_DecrementOp, Some(&t_decimal()), &t_decimal(), CALL_Empty, SLOT_Global_DecrementOp);
    }
    b.unary_overload(cx, ob, SLOT_Global_DecrementOp, it, &t_int, CALL_Empty, SLOT_Global_DecrementOp_I);
    b.unary_operator(cx, ob, db, CALL_Unary, SLOT_Global_DecrementLocalOp);
    b.unary_overload(cx, ob, SLOT_Global_DecrementLocalOp, it, &t_int, CALL_Empty, SLOT_Global_DecrementLocalOp_I);

    // unary +
    b.unary_operator(cx, ob, db, CALL_Unary, SLOT_Global_UnaryPlusOp);
    if es4 {
        b.unary_overload(cx, ob, SLOT_Global_UnaryPlusOp, Some(&t_decimal()), &t_decimal(), CALL_Unary, SLOT_Global_UnaryPlusOp_M);
    }
    b.unary_overload(cx, ob, SLOT_Global_UnaryPlusOp, it, &t_int, CALL_Unary, SLOT_Global_UnaryPlusOp_I);

    // binary +; the int overload is off since the result can exceed MAX_INT
    b.binary_operator(cx, ob, no, SLOT_Global_BinaryPlusOp);

    // unary -
    b.unary_operator(cx, ob, db, CALL_Unary, SLOT_Global_UnaryMinusOp);
    if es4 {
        b.unary_overload(cx, ob, SLOT_Global_UnaryMinusOp, Some(&t_decimal()), &t_decimal(), CALL_Unary, SLOT_Global_UnaryMinusOp);
    }
    b.unary_overload(cx, ob, SLOT_Global_UnaryMinusOp, it, &t_int, CALL_Unary, SLOT_Global_UnaryMinusOp_I);

    // binary -
    b.binary_operator(cx, ob, db, SLOT_Global_BinaryMinusOp);
    if es4 {
        b.binary_overload(cx, ob, SLOT_Global_BinaryMinusOp, Some(&t_decimal()), &t_decimal(), &t_decimal(), SLOT_Global_BinaryMinusOp);
    }

    // ~
    b.unary_operator(cx, ob, it, CALL_Unary, SLOT_Global_BitwiseNotOp);
    b.unary_overload(cx, ob, SLOT_Global_BitwiseNotOp, it, &t_int, CALL_Unary, SLOT_Global_BitwiseNotOp_I);

    // !
    b.unary_operator(cx, ob, bo, CALL_Unary, SLOT_Global_LogicalNotOp);
    b.unary_overload(cx, ob, SLOT_Global_LogicalNotOp, bo, &t_boolean, CALL_Unary, SLOT_Global_LogicalNotOp_B);
    b.unary_overload(cx, ob, SLOT_Global_LogicalNotOp, bo, &t_int, CALL_Unary, SLOT_Global_LogicalNotOp_I);

    // *
    b.binary_operator(cx, ob, db, SLOT_Global_MultiplyOp);
    if es4 {
        b.binary_overload(cx, ob, SLOT_Global_MultiplyOp, Some(&t_decimal()), &t_decimal(), &t_decimal(), SLOT_Global_MultiplyOp);
    }

    // /
    b.binary_operator(cx, ob, db, SLOT_Global_DivideOp);
    if es4 {
        b.binary_overload(cx, ob, SLOT_Global_DivideOp, Some(&t_decimal()), &t_decimal(), &t_decimal(), SLOT_Global_DivideOp);
    }

    // %
    b.binary_operator(cx, ob, db, SLOT_Global_ModulusOp);
    if es4 {
        b.binary_overload(cx, ob, SLOT_Global_ModulusOp, Some(&t_decimal()), &t_decimal(), &t_decimal(), SLOT_Global_ModulusOp);
    }

    // <<
    b.binary_operator(cx, ob, it, SLOT_Global_LeftShiftOp);
    b.binary_overload(cx, ob, SLOT_Global_LeftShiftOp, it, &t_int, &t_int, SLOT_Global_LeftShiftOp_II);

    // >>
    b.binary_operator(cx, ob, it, SLOT_Global_RightShiftOp);
    b.binary_overload(cx, ob, SLOT_Global_RightShiftOp, it, &t_int, &t_int, SLOT_Global_RightShiftOp_II);

    // >>>
    b.binary_operator(cx, ob, ui, SLOT_Global_UnsignedRightShiftOp);
    b.binary_overload(cx, ob, SLOT_Global_UnsignedRightShiftOp, it, &t_int, &t_int, SLOT_Global_UnsignedRightShiftOp_II);

    // < > <= >= instanceof in is
    b.binary_operator(cx, ob, bo, SLOT_Global_LessThanOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_GreaterThanOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_LessThanOrEqualOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_GreaterThanOrEqualOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_InstanceofOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_InOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_IsOp);
    b.binary_operator(cx, ob, bo, SLOT_Global_IsLateOp);

    // == != === !==
    b.binary_operator(cx, ob, bo, SLOT_Global_EqualsOp);
    b.binary_overload(cx, ob, SLOT_Global_EqualsOp, bo, &t_int, &t_int, SLOT_Global_EqualsOp_II);
    b.binary_operator(cx, ob, bo, SLOT_Global_NotEqualsOp);
    b.binary_overload(cx, ob, SLOT_Global_NotEqualsOp, bo, &t_int, &t_int, SLOT_Global_NotEqualsOp_II);
    b.binary_operator(cx, ob, bo, SLOT_Global_StrictEqualsOp);
    b.binary_overload(cx, ob, SLOT_Global_StrictEqualsOp, bo, &t_int, &t_int, SLOT_Global_StrictEqualsOp_II);
    b.binary_operator(cx, ob, bo, SLOT_Global_StrictNotEqualsOp);
    b.binary_overload(cx, ob, SLOT_Global_StrictNotEqualsOp, bo, &t_int, &t_int, SLOT_Global_StrictNotEqualsOp_II);

    // & ^ |
    b.binary_operator(cx, ob, it, SLOT_Global_BitwiseAndOp);
    b.binary_overload(cx, ob, SLOT_Global_BitwiseAndOp, it, &t_int, &t_int, SLOT_Global_BitwiseAndOp_II);
    b.binary_operator(cx, ob, it, SLOT_Global_BitwiseXorOp);
    b.binary_overload(cx, ob, SLOT_Global_BitwiseXorOp, it, &t_int, &t_int, SLOT_Global_BitwiseXorOp_II);
    b.binary_operator(cx, ob, it, SLOT_Global_BitwiseOrOp);
    b.binary_overload(cx, ob, SLOT_Global_BitwiseOrOp, it, &t_int, &t_int, SLOT_Global_BitwiseOrOp_II);

    // && || (the result is one of the operand types)
    b.binary_operator(cx, ob, no, SLOT_Global_LogicalAndOp);
    b.binary_overload(cx, ob, SLOT_Global_LogicalAndOp, it, &t_int, &t_int, SLOT_Global_LogicalAndOp_II);
    b.binary_operator(cx, ob, no, SLOT_Global_LogicalOrOp);
    b.binary_overload(cx, ob, SLOT_Global_LogicalOrOp, it, &t_int, &t_int, SLOT_Global_LogicalOrOp_II);

    // as (the result is the rhs type)
    b.binary_operator(cx, ob, no, SLOT_Global_AsOp);
    b.binary_operator(cx, ob, no, SLOT_Global_AsLateOp);

    let attribute = |name: &str, value: ObjectValue| {
        let slot_id = b.explicit_get(cx, ob, name, &cx.public_namespace(), no, true, false, -1, -1, -1);
        ob.get_slot(cx, slot_id).expect("NullPointerException").set_value(Some(Value::Object(value)));
    };
    attribute("public", cx.public_namespace());
    attribute("intrinsic", ObjectValue::intrinsic_attribute());
    attribute("native", ObjectValue::native_attribute());
    attribute("static", ObjectValue::static_attribute());
    attribute("dynamic", ObjectValue::dynamic_attribute());
    attribute("final", ObjectValue::final_attribute());
    attribute("virtual", ObjectValue::virtual_attribute());
    attribute("override", ObjectValue::override_attribute());
    // The builtin type void has no class.
    attribute("void", t_void.as_object().clone());
}

/// `GlobalBuilder.ExplicitCall`: in a package this is `PackageBuilder`'s; at script level a function is a variable.
#[allow(clippy::too_many_arguments)]
pub(crate) fn explicit_call_nss(b: &Builder, cx: &Context, ob: &ObjectValue, name: &str, namespaces: &Namespaces, type_: Option<&TypeValue>, is_final: bool, is_override: bool, expected_id: i32, method_id: i32, var_id: i32) -> i32 {
    if b.is_in_package() {
        return crate::asc::emit::package_builder::explicit_call_nss(b, cx, ob, name, namespaces, type_, is_final, is_override, expected_id, method_id, var_id);
    }
    let mut var_id = var_id;
    if method_id >= 0 || var_id >= 0 {
        var_id = b.variable(cx, ob);
    }
    let slot_id = b.explicit_var_nss(cx, ob, name, namespaces, type_, expected_id, -1, var_id);
    if cx.use_static_semantics() {
        b.implicit_call(cx, ob, slot_id, type_, CALL_Method, -1, -1);
        b.implicit_construct(cx, ob, slot_id, Some(&cx.no_type()), CALL_Method, -1, -1);
        let slot = ob.get_slot(cx, slot_id).expect("NullPointerException");
        slot.set_const(true);
        slot.set_type(Some(cx.function_type().get_default_type_info()));
    }
    slot_id
}
