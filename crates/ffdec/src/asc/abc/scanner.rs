//! Port of `macromedia.abc.Scanner`.

use super::bytecode_buffer::BytecodeBuffer;
use super::decoder_exception::DecoderException;
use crate::asc::emit::constants::*;

pub fn scan_minor_version(in_: &BytecodeBuffer) -> i32 {
    let position = in_.pos();
    in_.skip(2);
    position
}

pub fn scan_major_version(in_: &BytecodeBuffer) -> i32 {
    let position = in_.pos();
    in_.skip(2);
    position
}

fn positions(in_: &BytecodeBuffer, mut skip: impl FnMut(&BytecodeBuffer)) -> Vec<i32> {
    let size = in_.read_u32();
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut().skip(1) {
        *p = in_.pos();
        skip(in_);
    }
    positions
}

pub fn scan_int_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        b.read_u32();
    })
}

pub fn scan_uint_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        b.read_u32();
    })
}

pub fn scan_double_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        b.read_double();
    })
}

pub fn scan_decimal_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        b.read_bytes(16);
    })
}

pub fn scan_str_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        let length = b.read_u32() as i64;
        b.skip(length);
    })
}

pub fn scan_ns_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        b.read_u8();
        b.read_u32();
    })
}

pub fn scan_ns_set_constants(in_: &BytecodeBuffer) -> Vec<i32> {
    positions(in_, |b| {
        let count = b.read_u32() as i64;
        b.skip_entries(count);
    })
}

pub fn scan_multiname_constants(in_: &BytecodeBuffer) -> Result<Vec<i32>, DecoderException> {
    let size = in_.read_u32();
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut().skip(1) {
        *p = in_.pos();
        let kind = in_.read_u8();
        match kind {
            CONSTANT_Qname | CONSTANT_QnameA => {
                in_.read_u32();
                in_.read_u32();
            }
            CONSTANT_RTQname | CONSTANT_RTQnameA => {
                in_.read_u32();
            }
            CONSTANT_Multiname | CONSTANT_MultinameA => {
                in_.read_u32();
                in_.read_u32();
            }
            CONSTANT_RTQnameL | CONSTANT_RTQnameLA => {}
            CONSTANT_MultinameL | CONSTANT_MultinameLA => {
                in_.read_u32();
            }
            CONSTANT_TypeName => {
                in_.read_u32();
                let count = in_.read_u32() as i64;
                in_.skip_entries(count);
            }
            _ => return Err(DecoderException::new(format!("Invalid constant type: {}", kind))),
        }
    }
    Ok(positions)
}

pub fn scan_methods(in_: &BytecodeBuffer) -> Vec<i32> {
    let size = in_.read_u32();
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut() {
        *p = in_.pos();
        let param_count = in_.read_u32() as i64;
        in_.read_u32();
        in_.skip_entries(param_count);
        in_.read_u32();
        let flags = in_.read_u8();
        let optional_count = if (flags & METHOD_HasOptional) != 0 { in_.read_u32() as i64 } else { 0 };
        for _ in 0..optional_count {
            in_.read_u32();
            in_.read_u8();
        }
        let param_name_count = if (flags & METHOD_HasParamNames) != 0 { param_count } else { 0 };
        for _ in 0..param_name_count {
            in_.read_u32();
        }
    }
    positions
}

pub fn scan_metadata(in_: &BytecodeBuffer) -> Vec<i32> {
    let size = in_.read_u32();
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut() {
        *p = in_.pos();
        in_.read_u32();
        let value_count = in_.read_u32() as i64;
        in_.skip_entries(value_count * 2);
    }
    positions
}

pub fn scan_instances(in_: &BytecodeBuffer, size: i32) -> Vec<i32> {
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut() {
        *p = in_.pos();
        in_.skip_entries(2);
        let flags = in_.read_u8();
        if (flags & CLASS_FLAG_protected) != 0 {
            in_.read_u32();
        }
        let interface_count = in_.read_u32() as i64;
        in_.skip_entries(interface_count);
        in_.read_u32();
        scan_traits(in_);
    }
    positions
}

pub fn scan_classes(in_: &BytecodeBuffer, size: i32) -> Vec<i32> {
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut() {
        *p = in_.pos();
        in_.read_u32();
        scan_traits(in_);
    }
    positions
}

pub fn scan_scripts(in_: &BytecodeBuffer) -> Vec<i32> {
    let size = in_.read_u32();
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut() {
        *p = in_.pos();
        in_.read_u32();
        scan_traits(in_);
    }
    positions
}

pub fn scan_method_bodies(in_: &BytecodeBuffer) -> Vec<i32> {
    let size = in_.read_u32();
    let mut positions = vec![0; size.max(0) as usize];
    for p in positions.iter_mut() {
        *p = in_.pos();
        in_.skip_entries(5);
        let code_length = in_.read_u32() as i64;
        in_.skip(code_length as i32 as i64);
        scan_exceptions(in_);
        scan_traits(in_);
    }
    positions
}

fn scan_exceptions(in_: &BytecodeBuffer) {
    let count = in_.read_u32() as i64;
    if in_.minor_version() == 15 {
        in_.skip_entries(count * 4);
    } else {
        in_.skip_entries(count * 5);
    }
}

fn scan_traits(in_: &BytecodeBuffer) {
    let count = in_.read_u32() as i64;
    for _ in 0..count {
        in_.read_u32();
        let kind = in_.read_u8();
        let tag = kind & 0x0f;
        match tag {
            TRAIT_Var | TRAIT_Const => {
                in_.skip_entries(2);
                let value_id = in_.read_u32();
                if value_id > 0 {
                    in_.read_u8();
                }
            }
            TRAIT_Method | TRAIT_Getter | TRAIT_Setter => in_.skip_entries(2),
            TRAIT_Class | TRAIT_Function => in_.skip_entries(2),
            _ => eprintln!("invalid trait type: {}", tag),
        }
        if ((kind >> 4) & TRAIT_FLAG_metadata) != 0 {
            let metadata = in_.read_u32() as i64;
            in_.skip_entries(metadata);
        }
    }
}
