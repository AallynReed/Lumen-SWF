//! Port of `macromedia.asc.embedding.avmplus.ActionBlockConstants`.

pub const MINORwithDECIMAL: i32 = 17;

pub const CONSTANT_Void: i32 = 0x00;
pub const CONSTANT_Utf8: i32 = 0x01;
pub const CONSTANT_Decimal: i32 = 0x02;
pub const CONSTANT_Integer: i32 = 0x03;
pub const CONSTANT_UInteger: i32 = 0x04;
pub const CONSTANT_PrivateNamespace: i32 = 0x05;
pub const CONSTANT_Double: i32 = 0x06;
pub const CONSTANT_Qname: i32 = 0x07;
pub const CONSTANT_Namespace: i32 = 0x08;
pub const CONSTANT_Multiname: i32 = 0x09;
pub const CONSTANT_False: i32 = 0x0A;
pub const CONSTANT_True: i32 = 0x0B;
pub const CONSTANT_Null: i32 = 0x0C;
pub const CONSTANT_QnameA: i32 = 0x0D;
pub const CONSTANT_MultinameA: i32 = 0x0E;
pub const CONSTANT_RTQname: i32 = 0x0F;
pub const CONSTANT_RTQnameA: i32 = 0x10;
pub const CONSTANT_RTQnameL: i32 = 0x11;
pub const CONSTANT_RTQnameLA: i32 = 0x12;
pub const CONSTANT_Namespace_Set: i32 = 0x15;
pub const CONSTANT_PackageNamespace: i32 = 0x16;
pub const CONSTANT_PackageInternalNs: i32 = 0x17;
pub const CONSTANT_ProtectedNamespace: i32 = 0x18;
pub const CONSTANT_ExplicitNamespace: i32 = 0x19;
pub const CONSTANT_StaticProtectedNs: i32 = 0x1A;
pub const CONSTANT_MultinameL: i32 = 0x1B;
pub const CONSTANT_MultinameLA: i32 = 0x1C;
pub const CONSTANT_TypeName: i32 = 0x1D;

pub const TRAIT_Var: i32 = 0x00;
pub const TRAIT_Method: i32 = 0x01;
pub const TRAIT_Getter: i32 = 0x02;
pub const TRAIT_Setter: i32 = 0x03;
pub const TRAIT_Class: i32 = 0x04;
pub const TRAIT_Function: i32 = 0x05;
pub const TRAIT_Const: i32 = 0x06;

pub const TRAIT_FLAG_final: i32 = 0x01;
pub const TRAIT_FLAG_override: i32 = 0x02;
pub const TRAIT_FLAG_metadata: i32 = 0x04;

pub const CLASS_FLAG_sealed: i32 = 0x01;
pub const CLASS_FLAG_final: i32 = 0x02;
pub const CLASS_FLAG_interface: i32 = 0x04;
pub const CLASS_FLAG_protected: i32 = 0x08;
pub const CLASS_FLAG_non_nullable: i32 = 0x10;

pub const METHOD_Arguments: i32 = 0x1;
pub const METHOD_Activation: i32 = 0x2;
pub const METHOD_Needrest: i32 = 0x4;
pub const METHOD_HasOptional: i32 = 0x8;
pub const METHOD_IgnoreRest: i32 = 0x10;
pub const METHOD_Native: i32 = 0x20;
pub const METHOD_Setsdxns: i32 = 0x40;
pub const METHOD_HasParamNames: i32 = 0x80;

pub const OP_bkpt: i32 = 0x01;
pub const OP_nop: i32 = 0x02;
pub const OP_throw: i32 = 0x03;
pub const OP_getsuper: i32 = 0x04;
pub const OP_setsuper: i32 = 0x05;
pub const OP_dxns: i32 = 0x06;
pub const OP_dxnslate: i32 = 0x07;
pub const OP_kill: i32 = 0x08;
pub const OP_label: i32 = 0x09;

pub const OP_ifnlt: i32 = 0x0c;
pub const OP_ifnle: i32 = 0x0d;
pub const OP_ifngt: i32 = 0x0e;
pub const OP_ifnge: i32 = 0x0f;
pub const OP_jump: i32 = 0x10;
pub const OP_iftrue: i32 = 0x11;
pub const OP_iffalse: i32 = 0x12;
pub const OP_ifeq: i32 = 0x13;
pub const OP_ifne: i32 = 0x14;
pub const OP_iflt: i32 = 0x15;
pub const OP_ifle: i32 = 0x16;
pub const OP_ifgt: i32 = 0x17;
pub const OP_ifge: i32 = 0x18;
pub const OP_ifstricteq: i32 = 0x19;
pub const OP_ifstrictne: i32 = 0x1a;
pub const OP_lookupswitch: i32 = 0x1b;
pub const OP_pushwith: i32 = 0x1c;
pub const OP_popscope: i32 = 0x1d;
pub const OP_nextname: i32 = 0x1e;
pub const OP_hasnext: i32 = 0x1f;

pub const OP_pushnull: i32 = 0x20;
pub const OP_pushundefined: i32 = 0x21;
pub const OP_pushuninitialized: i32 = 0x22;
pub const OP_nextvalue: i32 = 0x23;
pub const OP_pushbyte: i32 = 0x24;
pub const OP_pushshort: i32 = 0x25;
pub const OP_pushtrue: i32 = 0x26;
pub const OP_pushfalse: i32 = 0x27;
pub const OP_pushnan: i32 = 0x28;
pub const OP_pop: i32 = 0x29;
pub const OP_dup: i32 = 0x2a;
pub const OP_swap: i32 = 0x2b;
pub const OP_pushstring: i32 = 0x2c;
pub const OP_pushint: i32 = 0x2d;
pub const OP_pushuint: i32 = 0x2e;
pub const OP_pushdouble: i32 = 0x2f;
pub const OP_pushscope: i32 = 0x30;
pub const OP_pushnamespace: i32 = 0x31;
pub const OP_hasnext2: i32 = 0x32;
pub const OP_pushdecimal: i32 = 0x33;
pub const OP_pushdnan: i32 = 0x34;

pub const OP_li8: i32 = 0x35;
pub const OP_li16: i32 = 0x36;
pub const OP_li32: i32 = 0x37;
pub const OP_lf32: i32 = 0x38;
pub const OP_lf64: i32 = 0x39;
pub const OP_si8: i32 = 0x3A;
pub const OP_si16: i32 = 0x3B;
pub const OP_si32: i32 = 0x3C;
pub const OP_sf32: i32 = 0x3D;
pub const OP_sf64: i32 = 0x3E;

pub const OP_newfunction: i32 = 0x40;
pub const OP_call: i32 = 0x41;
pub const OP_construct: i32 = 0x42;
pub const OP_callmethod: i32 = 0x43;
pub const OP_callstatic: i32 = 0x44;
pub const OP_callsuper: i32 = 0x45;
pub const OP_callproperty: i32 = 0x46;
pub const OP_returnvoid: i32 = 0x47;
pub const OP_returnvalue: i32 = 0x48;
pub const OP_constructsuper: i32 = 0x49;
pub const OP_constructprop: i32 = 0x4A;
pub const OP_callproplex: i32 = 0x4C;
pub const OP_callsupervoid: i32 = 0x4E;
pub const OP_callpropvoid: i32 = 0x4F;
pub const OP_sxi1: i32 = 0x50;
pub const OP_sxi8: i32 = 0x51;
pub const OP_sxi16: i32 = 0x52;
pub const OP_applytype: i32 = 0x53;

pub const OP_newobject: i32 = 0x55;
pub const OP_newarray: i32 = 0x56;
pub const OP_newactivation: i32 = 0x57;

pub const OP_newclass: i32 = 0x58;
pub const OP_getdescendants: i32 = 0x59;
pub const OP_newcatch: i32 = 0x5a;
pub const OP_deldescendants: i32 = 0x5b;

pub const OP_findpropstrict: i32 = 0x5d;
pub const OP_findproperty: i32 = 0x5e;
pub const OP_finddef: i32 = 0x5f;
pub const OP_getlex: i32 = 0x60;

pub const OP_setproperty: i32 = 0x61;
pub const OP_getlocal: i32 = 0x62;
pub const OP_setlocal: i32 = 0x63;

pub const OP_getglobalscope: i32 = 0x64;
pub const OP_getscopeobject: i32 = 0x65;
pub const OP_getproperty: i32 = 0x66;
pub const OP_initproperty: i32 = 0x68;
pub const OP_deleteproperty: i32 = 0x6a;
pub const OP_getslot: i32 = 0x6c;
pub const OP_setslot: i32 = 0x6d;

pub const OP_getglobalslot: i32 = 0x6e;

pub const OP_setglobalslot: i32 = 0x6f;

pub const OP_convert_s: i32 = 0x70;
pub const OP_esc_xelem: i32 = 0x71;
pub const OP_esc_xattr: i32 = 0x72;
pub const OP_convert_i: i32 = 0x73;
pub const OP_convert_u: i32 = 0x74;
pub const OP_convert_d: i32 = 0x75;
pub const OP_convert_b: i32 = 0x76;
pub const OP_convert_o: i32 = 0x77;
pub const OP_checkfilter: i32 = 0x78;
pub const OP_convert_m: i32 = 0x79;
pub const OP_convert_m_p: i32 = 0x7a;

pub const OP_coerce: i32 = 0x80;

pub const OP_coerce_b: i32 = 0x81;
pub const OP_coerce_a: i32 = 0x82;
pub const OP_coerce_i: i32 = 0x83;
pub const OP_coerce_d: i32 = 0x84;
pub const OP_coerce_s: i32 = 0x85;
pub const OP_astype: i32 = 0x86;
pub const OP_astypelate: i32 = 0x87;
pub const OP_coerce_u: i32 = 0x88;
pub const OP_coerce_o: i32 = 0x89;

pub const OP_negate_p: i32 = 0x8f;
pub const OP_negate: i32 = 0x90;
pub const OP_increment: i32 = 0x91;
pub const OP_inclocal: i32 = 0x92;
pub const OP_decrement: i32 = 0x93;
pub const OP_declocal: i32 = 0x94;
pub const OP_typeof: i32 = 0x95;
pub const OP_not: i32 = 0x96;
pub const OP_bitnot: i32 = 0x97;

pub const OP_increment_p: i32 = 0x9c;
pub const OP_inclocal_p: i32 = 0x9d;
pub const OP_decrement_p: i32 = 0x9e;
pub const OP_declocal_p: i32 = 0x9f;

pub const OP_add: i32 = 0xa0;
pub const OP_subtract: i32 = 0xa1;
pub const OP_multiply: i32 = 0xa2;
pub const OP_divide: i32 = 0xa3;
pub const OP_modulo: i32 = 0xa4;
pub const OP_lshift: i32 = 0xa5;
pub const OP_rshift: i32 = 0xa6;
pub const OP_urshift: i32 = 0xa7;
pub const OP_bitand: i32 = 0xa8;
pub const OP_bitor: i32 = 0xa9;
pub const OP_bitxor: i32 = 0xaa;
pub const OP_equals: i32 = 0xab;
pub const OP_strictequals: i32 = 0xac;
pub const OP_lessthan: i32 = 0xad;
pub const OP_lessequals: i32 = 0xae;
pub const OP_greaterthan: i32 = 0xaf;

pub const OP_greaterequals: i32 = 0xb0;
pub const OP_instanceof: i32 = 0xb1;
pub const OP_istype: i32 = 0xb2;
pub const OP_istypelate: i32 = 0xb3;
pub const OP_in: i32 = 0xb4;
pub const OP_add_p: i32 = 0xb5;
pub const OP_subtract_p: i32 = 0xb6;
pub const OP_multiply_p: i32 = 0xb7;
pub const OP_divide_p: i32 = 0xb8;
pub const OP_modulo_p: i32 = 0xb9;

pub const OP_increment_i: i32 = 0xc0;
pub const OP_decrement_i: i32 = 0xc1;
pub const OP_inclocal_i: i32 = 0xc2;
pub const OP_declocal_i: i32 = 0xc3;
pub const OP_negate_i: i32 = 0xc4;
pub const OP_add_i: i32 = 0xc5;
pub const OP_subtract_i: i32 = 0xc6;
pub const OP_multiply_i: i32 = 0xc7;

pub const OP_getlocal0: i32 = 0xd0;
pub const OP_getlocal1: i32 = 0xd1;
pub const OP_getlocal2: i32 = 0xd2;
pub const OP_getlocal3: i32 = 0xd3;
pub const OP_setlocal0: i32 = 0xd4;
pub const OP_setlocal1: i32 = 0xd5;
pub const OP_setlocal2: i32 = 0xd6;
pub const OP_setlocal3: i32 = 0xd7;

pub const OP_debug: i32 = 0xef;

pub const OP_debugline: i32 = 0xf0;
pub const OP_debugfile: i32 = 0xf1;
pub const OP_bkptline: i32 = 0xf2;
pub const OP_timestamp: i32 = 0xf3;
