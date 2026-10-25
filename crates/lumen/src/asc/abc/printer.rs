//! Port of `macromedia.abc.Printer`, the `abcdump` command-line tool.

use super::bytecode_buffer::BytecodeBuffer;
use super::constant_pool::PoolValue;
use super::decoder::Decoder;
use super::decoder_exception::DecoderException;
use super::default_visitor::DefaultVisitor;
use super::multi_name::MultiName;
use super::opcodes::OP_NAMES;
use super::q_name::QName;
use super::visitor::Visitor;
use crate::asc::emit::constants::*;

const LINE_SEPARATOR: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Java's `String.valueOf(Object)` for the values `ConstantPool` hands back.
pub fn pool_value_string(v: Option<&PoolValue>) -> String {
    let opt = |s: &Option<String>| s.clone().unwrap_or_else(|| "null".to_string());
    match v {
        None => "null".to_string(),
        Some(PoolValue::Str(s)) => s.clone(),
        Some(PoolValue::Int(i)) => i.to_string(),
        Some(PoolValue::Long(l)) => l.to_string(),
        Some(PoolValue::Double(d)) => crate::xfl::num::java_double(*d),
        Some(PoolValue::Decimal(d)) => d.to_string(),
        Some(PoolValue::QName(q)) => q.to_string(),
        Some(PoolValue::MultiName(m)) => m.to_string(),
        Some(PoolValue::Bool(b)) => b.to_string(),
        // identity-hashed Object/String[] in Java
        Some(PoolValue::Null) => "java.lang.Object".to_string(),
        Some(PoolValue::NamespaceSet(_)) => "[Ljava.lang.String;".to_string(),
        Some(PoolValue::List(l)) => format!("[{}]", l.iter().map(opt).collect::<Vec<_>>().join(", ")),
    }
}

fn qname_string(q: &Option<QName>) -> String {
    q.as_ref().map_or_else(|| "null".to_string(), |q| q.to_string())
}

#[derive(Default)]
struct State {
    show_opcodes: bool,
    class_name: Option<String>,
    method_name: Option<String>,
    start_line: i32,
    end_line: i32,
    current_class: Option<String>,
    current_method: Option<String>,
    current_line: i32,
}

impl State {
    fn print(&self, s: &str) {
        if self.current_class == self.class_name && self.current_method == self.method_name {
            print!("{}", s);
        }
    }

    fn println(&self, s: &str) {
        if self.current_class == self.class_name && self.current_method == self.method_name {
            println!("{}", s);
        }
    }

    fn print_opcode(&self, s: &str) {
        if self.current_class == self.class_name
            && self.current_method == self.method_name
            && ((self.start_line == 0 && self.end_line == 0) || (self.current_line >= self.start_line && self.current_line < self.end_line))
            && self.show_opcodes
        {
            println!("{}", s);
        }
    }
}

/// Usage: `abcdump [-b] [-c mx.core:UIObject] [-m setColor] [-l 10-15] foo.abc`.
pub fn main(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        eprintln!("Usage: abcdump [-b] [-c mx.core:UIObject] [-m setColor] [-l 10-15] foo.abc");
        eprintln!();
        eprintln!("  -b:                     include opcodes in the output");
        eprintln!("  -c className:           restrict output to the specified class");
        eprintln!("  -m methodName:          restrict output to the specified method");
        eprintln!("  -l startLine[-endLine]: restrict output to the specified line(s)");
        return Ok(());
    }

    let mut state = State::default();
    let mut file_name = None;
    let argc = args.len();
    let mut i = 0;
    while i < argc {
        if args[i] == "-c" {
            state.class_name = Some(args[i + 1].clone());
            i += 1;
        } else if args[i] == "-m" {
            state.method_name = Some(args[i + 1].clone());
            i += 1;
        } else if args[i] == "-l" {
            let line_range = &args[i + 1];
            let parse = |s: &str| s.parse::<i32>().map_err(|e| e.to_string());
            if let Some(index) = line_range.find('-') {
                state.start_line = parse(&line_range[..index])?;
                state.end_line = parse(&line_range[index + 1..])?;
            } else {
                state.start_line = parse(line_range)?;
                state.end_line = state.start_line + 1;
            }
            i += 1;
        } else if args[i] == "-b" {
            state.show_opcodes = true;
        } else if i == argc - 1 {
            file_name = Some(args[i].clone());
        }
        i += 1;
    }

    let file_name = file_name.ok_or_else(|| "no file name".to_string())?;
    let bytecodes = std::fs::read(&file_name).map_err(|e| e.to_string())?;
    let decoder = Decoder::new(BytecodeBuffer::from_bytes(bytecodes)).map_err(|e| e.to_string())?;
    process_scripts(&decoder, state).map_err(|e| e.to_string())
}

fn process_scripts(decoder: &Decoder, state: State) -> Result<(), DecoderException> {
    let script_info = &decoder.script_info;
    let mut v = AbcVisitor { decoder, s: state };
    for i in 0..script_info.size() {
        script_info.decode(i, &mut v)?;
    }
    Ok(())
}

struct AbcVisitor<'a> {
    decoder: &'a Decoder,
    s: State,
}

impl AbcVisitor<'_> {
    fn print(&self, s: &str) {
        self.s.print(s);
    }

    fn println(&self, s: &str) {
        self.s.println(s);
    }

    fn print_opcode(&self, s: &str) {
        self.s.print_opcode(s);
    }

    fn constant_string_value(&self, index: i32) -> String {
        match self.decoder.constant_pool.get_string(index) {
            Ok(s) => s.unwrap_or_else(|| "null".to_string()),
            Err(_) => "decoder exception...".to_string(),
        }
    }

    fn constant_namespace_value(&self, index: i32) -> String {
        match self.decoder.constant_pool.get_namespace_name(index) {
            Ok(s) => s.unwrap_or_else(|| "null".to_string()),
            Err(_) => "decoder exception...".to_string(),
        }
    }

    fn constant_int_value(&self, index: i32) -> String {
        self.decoder.constant_pool.get_int(index).to_string()
    }

    fn constant_uint_value(&self, index: i32) -> String {
        self.decoder.constant_pool.get_long(index).to_string()
    }

    fn constant_multiname_value(&self, index: i32) -> String {
        match self.decoder.constant_pool.get_general_multiname(index) {
            Ok(v) => pool_value_string(v.as_ref()),
            Err(_) => "decoder exception...".to_string(),
        }
    }
}

impl<'a> DefaultVisitor<'a> for AbcVisitor<'a> {
    fn decoder(&self) -> &'a Decoder {
        self.decoder
    }

    fn method_info_resolved(
        &mut self,
        return_type: Option<QName>,
        param_types: Option<Vec<Option<QName>>>,
        native_name: Option<String>,
        _flags: i32,
        _values: Option<Vec<Option<PoolValue>>>,
        param_names: Option<Vec<Option<String>>>,
    ) {
        self.print(&format!("<info name='{}' paramTypes='", native_name.as_deref().unwrap_or("null")));
        for p in param_types.iter().flatten() {
            self.print(&format!("{} ", qname_string(p)));
        }
        self.print("'");
        if let Some(r) = &return_type {
            self.print(&format!(" returnType='{}'", r));
        }
        self.print(" paramNames='");
        for p in param_names.iter().flatten() {
            self.print(&format!("{} ", p.as_deref().unwrap_or("null")));
        }
        self.print("'");

        self.println("/>");
    }

    fn metadata(&mut self, _name: Option<String>, _keys: Option<Vec<Option<String>>>, _values: Option<Vec<Option<String>>>) {
        self.println("<metadata/>");
    }

    fn begin_var(&mut self, name: Option<QName>, _slot_id: i32, type_: Option<QName>, value: Option<PoolValue>) {
        self.print(&format!("<var name='{}'", qname_string(&name)));
        if let Some(t) = &type_ {
            self.print(&format!(" type='{}'", t));
        }
        self.println(">");
        if value.is_some() {
            self.println(&format!("<value>{}</value>", pool_value_string(value.as_ref())));
        }
    }

    fn end_var(&mut self, _name: Option<QName>) {
        self.println("</var>");
    }

    fn begin_const(&mut self, name: Option<QName>, _slot_id: i32, type_: Option<QName>, value: Option<PoolValue>) {
        self.print(&format!("<const name='{}'", qname_string(&name)));
        if let Some(t) = &type_ {
            self.print(&format!(" type='{}'", t));
        }
        self.println(">");
        if value.is_some() {
            self.println(&format!("<value>{}</value>", pool_value_string(value.as_ref())));
        }
    }

    fn end_const(&mut self, _name: Option<QName>) {
        self.println("</const>");
    }

    fn begin_getter(&mut self, _method_info: i32, name: Option<QName>, disp_id: i32, attr: i32) {
        if self.s.method_name.is_some() {
            self.s.current_method = Some(qname_string(&name));
        }
        self.println(&format!("<getter name='{}' dispID='{}' attr='{}'>", qname_string(&name), disp_id, attr));
    }

    fn end_getter(&mut self, _name: Option<QName>) {
        self.println("</getter>");
        if self.s.method_name.is_some() {
            self.s.current_method = None;
        }
    }

    fn begin_setter(&mut self, _method_info: i32, name: Option<QName>, disp_id: i32, attr: i32) {
        if self.s.method_name.is_some() {
            self.s.current_method = Some(qname_string(&name));
        }
        self.println(&format!("<setter name='{}' dispID='{}' attr='{}'>", qname_string(&name), disp_id, attr));
    }

    fn end_setter(&mut self, _name: Option<QName>) {
        self.println("</setter>");
        if self.s.method_name.is_some() {
            self.s.current_method = None;
        }
    }

    fn begin_method(&mut self, _method_info: i32, name: Option<QName>, disp_id: i32, attr: i32) {
        if self.s.method_name.is_some() {
            self.s.current_method = Some(qname_string(&name));
        }
        self.println(&format!("<method name='{}' dispID='{}' attr='{}'>", qname_string(&name), disp_id, attr));
    }

    fn end_method(&mut self, _name: Option<QName>) {
        self.println("</method>");
        if self.s.method_name.is_some() {
            self.s.current_method = None;
        }
    }

    fn begin_class(&mut self, name: Option<QName>, slot_id: i32) {
        if self.s.class_name.is_some() {
            self.s.current_class = Some(qname_string(&name));
        }
        self.println(&format!("<class name='{}' slot='{}'>", qname_string(&name), slot_id));
    }

    fn end_class_named(&mut self, _name: Option<QName>) {
        self.println("</class>");
        if self.s.class_name.is_some() {
            self.s.current_class = None;
        }
    }

    fn begin_i_init(&mut self, _method_info: i32) {
        self.println("<iinit>");
    }

    fn end_i_init(&mut self) {
        self.println("</iinit>");
    }

    fn begin_c_init(&mut self, _method_info: i32) {
        self.println("<cinit>");
    }

    fn end_c_init(&mut self) {
        self.println("</cinit>");
    }

    fn begin_function(&mut self, _method_info: i32, name: Option<QName>, _slot_id: i32) {
        self.println(&format!("<function name='{}'>", qname_string(&name)));
    }

    fn end_function(&mut self, _name: Option<QName>) {
        self.println("</function>");
    }

    fn instance_info(&mut self, name: Option<QName>, super_name: Option<QName>, _interfaces: Option<Vec<Option<MultiName>>>) {
        self.print(&format!("<info type='{}'", qname_string(&name)));
        if let Some(s) = &super_name {
            self.print(&format!(" superType='{}'", s));
        }
        self.println("/>");
    }

    fn class_info(&mut self, _name: Option<QName>) {}

    fn begin_abc(&mut self) {
        self.println("<abc>");
    }

    fn end_abc(&mut self) {
        self.println("</abc>");
    }

    fn begin_body(&mut self, _method_id: i32, _code_start: i32, _code_length: i64) {
        self.println("<![CDATA[");
    }

    fn end_body(&mut self) {
        self.println("]]>");
    }
}

impl Visitor for AbcVisitor<'_> {
    crate::default_visitor_methods!();

    fn target(&mut self, pos: i32) {
        println!("target {}", pos);
    }

    fn op_returnvoid(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_returnvoid as usize]));
    }

    fn op_returnvalue(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_returnvalue as usize]));
    }

    fn op_nop(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_nop as usize]));
    }

    fn op_bkpt(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_bkpt as usize]));
    }

    fn op_timestamp(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_timestamp as usize]));
    }

    fn op_debugline(&mut self, linenum: i32) {
        if self.s.start_line != 0 && self.s.end_line != 0 {
            self.s.current_line = linenum;
        }
        self.print_opcode(&format!("{}{} linenum={}", LINE_SEPARATOR, OP_NAMES[OP_debugline as usize], linenum));
    }

    fn op_bkptline(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_bkptline as usize]));
    }

    fn op_debug(&mut self, di_local: i32, index: i32, slot: i32, linenum: i32) {
        self.print_opcode(&format!(
            "{} di_local={} index={} slot={} linenum={}",
            OP_NAMES[OP_debug as usize], di_local, index, slot, linenum
        ));
    }

    fn op_debugfile(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_debugfile as usize], index, self.constant_string_value(index)));
    }

    fn op_jump(&mut self, jump: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_jump as usize], jump));
    }

    fn op_pushnull(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushnull as usize]));
    }

    fn op_pushundefined(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushundefined as usize]));
    }

    fn op_pushstring(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool_string[{}] = {}", OP_NAMES[OP_pushstring as usize], index, self.constant_string_value(index)));
    }

    fn op_pushnamespace(&mut self, index: i32) {
        self.print_opcode(&format!(
            "{} cpool_namespace[{}] = {}",
            OP_NAMES[OP_pushnamespace as usize],
            index,
            self.constant_namespace_value(index)
        ));
    }

    fn op_pushint(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool_int[{}] = {}", OP_NAMES[OP_pushint as usize], index, self.constant_int_value(index)));
    }

    fn op_pushuint(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool_uint[{}] = {}", OP_NAMES[OP_pushuint as usize], index, self.constant_uint_value(index)));
    }

    fn op_pushdouble(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool_double[{}]", OP_NAMES[OP_pushdouble as usize], index));
    }

    fn op_pushdecimal(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool_decimal[{}]", OP_NAMES[OP_pushdouble as usize], index));
    }

    fn op_getlocal(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_getlocal as usize], index));
    }

    fn op_pushtrue(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushtrue as usize]));
    }

    fn op_pushfalse(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushfalse as usize]));
    }

    fn op_pushnan(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushnan as usize]));
    }

    fn op_pushdnan(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushdnan as usize]));
    }

    fn op_pop(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pop as usize]));
    }

    fn op_dup(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_dup as usize]));
    }

    fn op_swap(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_swap as usize]));
    }

    fn op_convert_s(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_s as usize]));
    }

    fn op_esc_xelem(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_esc_xelem as usize]));
    }

    fn op_esc_xattr(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_esc_xattr as usize]));
    }

    fn op_checkfilter(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_checkfilter as usize]));
    }

    fn op_convert_d(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_d as usize]));
    }

    fn op_convert_b(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_b as usize]));
    }

    fn op_convert_o(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_o as usize]));
    }

    fn op_convert_m(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_m as usize]));
    }

    fn op_convert_m_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_convert_m_p as usize], param));
    }

    fn op_negate(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_negate as usize]));
    }

    fn op_negate_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_negate_p as usize], param));
    }

    fn op_negate_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_negate_i as usize]));
    }

    fn op_increment(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_increment as usize]));
    }

    fn op_increment_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_increment_p as usize], param));
    }

    fn op_increment_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_increment_i as usize]));
    }

    fn op_inclocal(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_inclocal as usize], index));
    }

    fn op_inclocal_p(&mut self, param: i32, index: i32) {
        self.print_opcode(&format!("{} p{} r{}", OP_NAMES[OP_inclocal as usize], param, index));
    }

    fn op_kill(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_kill as usize], index));
    }

    fn op_inclocal_i(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_inclocal_i as usize], index));
    }

    fn op_decrement(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_decrement as usize]));
    }

    fn op_decrement_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_decrement_p as usize], param));
    }

    fn op_decrement_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_decrement_i as usize]));
    }

    fn op_declocal(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_declocal as usize], index));
    }

    fn op_declocal_p(&mut self, param: i32, index: i32) {
        self.print_opcode(&format!("{} p{} r{}", OP_NAMES[OP_declocal_p as usize], param, index));
    }

    fn op_declocal_i(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_declocal_i as usize], index));
    }

    fn op_typeof(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_typeof as usize]));
    }

    fn op_not(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_not as usize]));
    }

    fn op_bitnot(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_bitnot as usize]));
    }

    fn op_setlocal(&mut self, index: i32) {
        self.print_opcode(&format!("{} r{}", OP_NAMES[OP_setlocal as usize], index));
    }

    fn op_add(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_add as usize]));
    }

    fn op_add_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_add as usize], param));
    }

    fn op_add_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_add_i as usize]));
    }

    fn op_subtract(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_subtract as usize]));
    }

    fn op_subtract_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_subtract_p as usize], param));
    }

    fn op_subtract_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_subtract_i as usize]));
    }

    fn op_multiply(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_multiply as usize]));
    }

    fn op_multiply_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_multiply_p as usize], param));
    }

    fn op_multiply_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_multiply_i as usize]));
    }

    fn op_divide(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_divide as usize]));
    }

    fn op_divide_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_divide_p as usize], param));
    }

    fn op_modulo(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_modulo as usize]));
    }

    fn op_modulo_p(&mut self, param: i32) {
        self.print_opcode(&format!("{} p{}", OP_NAMES[OP_modulo_p as usize], param));
    }

    fn op_lshift(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_lshift as usize]));
    }

    fn op_rshift(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_rshift as usize]));
    }

    fn op_urshift(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_urshift as usize]));
    }

    fn op_bitand(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_bitand as usize]));
    }

    fn op_bitor(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_bitor as usize]));
    }

    fn op_bitxor(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_bitxor as usize]));
    }

    fn op_equals(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_equals as usize]));
    }

    fn op_strictequals(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_strictequals as usize]));
    }

    fn op_lookupswitch(&mut self, default_pos: i32, case_pos: &[i32], _pos: i32, _p2: i32) {
        let case_list: String = case_pos.iter().map(|c| format!("{},", c)).collect();
        self.print_opcode(&format!("{} defaultPos={} casePos={}", OP_NAMES[OP_lookupswitch as usize], default_pos, case_list));
    }

    fn op_iftrue(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_iftrue as usize], offset));
    }

    fn op_iffalse(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_iffalse as usize], offset));
    }

    fn op_ifeq(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifeq as usize], offset));
    }

    fn op_ifne(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifne as usize], offset));
    }

    fn op_ifstricteq(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifstricteq as usize], offset));
    }

    fn op_ifstrictne(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifstrictne as usize], offset));
    }

    fn op_iflt(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_iflt as usize], offset));
    }

    fn op_ifle(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifle as usize], offset));
    }

    fn op_ifgt(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifgt as usize], offset));
    }

    fn op_ifge(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifge as usize], offset));
    }

    fn op_lessthan(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_lessthan as usize]));
    }

    fn op_lessequals(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_lessequals as usize]));
    }

    fn op_greaterthan(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_greaterthan as usize]));
    }

    fn op_greaterequals(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_greaterequals as usize]));
    }

    fn op_newobject(&mut self, size: i32) {
        self.print_opcode(&format!("{} size={}", OP_NAMES[OP_newobject as usize], size));
    }

    fn op_newarray(&mut self, size: i32) {
        self.print_opcode(&format!("{} size={}", OP_NAMES[OP_newarray as usize], size));
    }

    fn op_getproperty(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_getproperty as usize], index, self.constant_multiname_value(index)));
    }

    fn op_setproperty(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_setproperty as usize], index, self.constant_multiname_value(index)));
    }

    fn op_initproperty(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_initproperty as usize], index, self.constant_multiname_value(index)));
    }

    fn op_getdescendants(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_getdescendants as usize], index, self.constant_multiname_value(index)));
    }

    fn op_findpropstrict(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_findpropstrict as usize], index, self.constant_multiname_value(index)));
    }

    fn op_findproperty(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_findproperty as usize], index, self.constant_multiname_value(index)));
    }

    fn op_finddef(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_finddef as usize], index, self.constant_multiname_value(index)));
    }

    fn op_getlex(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_getlex as usize], index, self.constant_multiname_value(index)));
    }

    fn op_nextname(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_nextname as usize]));
    }

    fn op_nextvalue(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_nextvalue as usize]));
    }

    fn op_hasnext(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_hasnext as usize]));
    }

    fn op_hasnext2(&mut self, object_register: i32, index_register: i32) {
        self.print_opcode(&format!("{} {} {}", OP_NAMES[OP_hasnext2 as usize], object_register, index_register));
    }

    fn op_deleteproperty(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_deleteproperty as usize], index, self.constant_multiname_value(index)));
    }

    fn op_setslot(&mut self, index: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_setslot as usize], index));
    }

    fn op_getslot(&mut self, index: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_getslot as usize], index));
    }

    fn op_setglobalslot(&mut self, index: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_setglobalslot as usize], index));
    }

    fn op_getglobalslot(&mut self, index: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_getglobalslot as usize], index));
    }

    fn op_call(&mut self, size: i32) {
        self.print_opcode(&format!("{} size={}", OP_NAMES[OP_call as usize], size));
    }

    fn op_construct(&mut self, size: i32) {
        self.print_opcode(&format!("{} size={}", OP_NAMES[OP_construct as usize], size));
    }

    fn op_applytype(&mut self, size: i32) {
        self.print_opcode(&format!("{} size={}", OP_NAMES[OP_applytype as usize], size));
    }

    fn op_newfunction(&mut self, id: i32) {
        self.print_opcode(&format!("{} function={}", OP_NAMES[OP_newfunction as usize], id));
    }

    fn op_newclass(&mut self, id: i32) {
        self.print_opcode(&format!("{} class={}", OP_NAMES[OP_newclass as usize], id));
    }

    fn op_callstatic(&mut self, id: i32, argc: i32) {
        self.print_opcode(&format!("{} method={} argc={}", OP_NAMES[OP_callstatic as usize], id, argc));
    }

    fn op_callmethod(&mut self, id: i32, argc: i32) {
        self.print_opcode(&format!("{} dispid={} argc={}", OP_NAMES[OP_callmethod as usize], id, argc));
    }

    fn op_callproperty(&mut self, index: i32, argc: i32) {
        self.print_opcode(&format!(
            "{} cpool[{}] = {} argc={}",
            OP_NAMES[OP_callproperty as usize],
            index,
            self.constant_multiname_value(index),
            argc
        ));
    }

    fn op_callproplex(&mut self, index: i32, argc: i32) {
        self.print_opcode(&format!(
            "{} cpool[{}] = {} argc={}",
            OP_NAMES[OP_callproplex as usize],
            index,
            self.constant_multiname_value(index),
            argc
        ));
    }

    fn op_constructprop(&mut self, index: i32, argc: i32) {
        self.print_opcode(&format!(
            "{} cpool[{}] = {} argc={}",
            OP_NAMES[OP_constructprop as usize],
            index,
            self.constant_multiname_value(index),
            argc
        ));
    }

    fn op_callsuper(&mut self, index: i32, argc: i32) {
        self.print_opcode(&format!(
            "{} cpool[{}] = {} argc={}",
            OP_NAMES[OP_callsuper as usize],
            index,
            self.constant_multiname_value(index),
            argc
        ));
    }

    fn op_getsuper(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_getsuper as usize], index, self.constant_multiname_value(index)));
    }

    fn op_setsuper(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_setsuper as usize], index, self.constant_multiname_value(index)));
    }

    fn op_constructsuper(&mut self, argc: i32) {
        self.print_opcode(&format!("{} argc={}", OP_NAMES[OP_constructsuper as usize], argc));
    }

    fn op_pushshort(&mut self, n: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_pushshort as usize], n));
    }

    fn op_astype(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_astype as usize], index, self.constant_multiname_value(index)));
    }

    fn op_astypelate(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_astypelate as usize]));
    }

    fn op_coerce(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_coerce as usize], index, self.constant_multiname_value(index)));
    }

    fn op_coerce_b(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_b as usize]));
    }

    fn op_coerce_o(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_o as usize]));
    }

    fn op_coerce_a(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_a as usize]));
    }

    fn op_coerce_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_i as usize]));
    }

    fn op_coerce_u(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_u as usize]));
    }

    fn op_coerce_d(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_d as usize]));
    }

    fn op_coerce_s(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_coerce_s as usize]));
    }

    fn op_istype(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_istype as usize], index, self.constant_multiname_value(index)));
    }

    fn op_istypelate(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_istypelate as usize]));
    }

    fn op_pushbyte(&mut self, n: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_pushbyte as usize], n));
    }

    fn op_getscopeobject(&mut self, index: i32) {
        self.print_opcode(&format!("{} index={}", OP_NAMES[OP_getscopeobject as usize], index));
    }

    fn op_pushscope(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushscope as usize]));
    }

    fn op_popscope(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_popscope as usize]));
    }

    fn op_convert_i(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_i as usize]));
    }

    fn op_convert_u(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_convert_u as usize]));
    }

    fn op_throw(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_throw as usize]));
    }

    fn op_instanceof(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_instanceof as usize]));
    }

    fn op_in(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_in as usize]));
    }

    fn op_dxns(&mut self, index: i32) {
        self.print_opcode(&format!("{} cpool[{}] = {}", OP_NAMES[OP_dxns as usize], index, self.constant_multiname_value(index)));
    }

    fn op_dxnslate(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_dxnslate as usize]));
    }

    fn op_ifnlt(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifnlt as usize], offset));
    }

    fn op_ifnle(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifnle as usize], offset));
    }

    fn op_ifngt(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifngt as usize], offset));
    }

    fn op_ifnge(&mut self, offset: i32, _pos: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_ifnge as usize], offset));
    }

    fn op_pushwith(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_pushwith as usize]));
    }

    fn op_newactivation(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_newactivation as usize]));
    }

    fn op_newcatch(&mut self, index: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_newcatch as usize], index));
    }

    fn op_deldescendants(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_deldescendants as usize]));
    }

    fn op_getglobalscope(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_getglobalscope as usize]));
    }

    fn op_getlocal0(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_getlocal0 as usize]));
    }

    fn op_getlocal1(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_getlocal1 as usize]));
    }

    fn op_getlocal2(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_getlocal2 as usize]));
    }

    fn op_getlocal3(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_getlocal3 as usize]));
    }

    fn op_setlocal0(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_setlocal0 as usize]));
    }

    fn op_setlocal1(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_setlocal1 as usize]));
    }

    fn op_setlocal2(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_setlocal2 as usize]));
    }

    fn op_setlocal3(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_setlocal3 as usize]));
    }

    fn op_label(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_label as usize]));
    }

    fn op_pushconstant(&mut self, id: i32) {
        self.print_opcode(&format!("{} {}", OP_NAMES[OP_pushuninitialized as usize], id));
    }

    fn op_callsupervoid(&mut self, index: i32, argc: i32) {
        self.print_opcode(&format!(
            "{} cpool[{}] = {} argc={}",
            OP_NAMES[OP_callsupervoid as usize],
            index,
            self.constant_multiname_value(index),
            argc
        ));
    }

    fn op_callpropvoid(&mut self, index: i32, argc: i32) {
        self.print_opcode(&format!(
            "{} cpool[{}] = {} argc={}",
            OP_NAMES[OP_callpropvoid as usize],
            index,
            self.constant_multiname_value(index),
            argc
        ));
    }

    fn op_li8(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_li8 as usize]));
    }

    fn op_li16(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_li16 as usize]));
    }

    fn op_li32(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_li32 as usize]));
    }

    fn op_lf32(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_lf32 as usize]));
    }

    fn op_lf64(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_lf64 as usize]));
    }

    fn op_si8(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_si8 as usize]));
    }

    fn op_si16(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_si16 as usize]));
    }

    fn op_si32(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_si32 as usize]));
    }

    fn op_sf32(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_sf32 as usize]));
    }

    fn op_sf64(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_sf64 as usize]));
    }

    fn op_sxi1(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_sxi1 as usize]));
    }

    fn op_sxi8(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_sxi8 as usize]));
    }

    fn op_sxi16(&mut self) {
        self.print_opcode(&format!("{}", OP_NAMES[OP_sxi16 as usize]));
    }
}

#[cfg(test)]
mod tests {
    /// Runs `abcdump` on `ASC_PRINT_FILE` (with `ASC_PRINT_ARGS` before it), for comparison with the Java tool.
    #[test]
    #[ignore]
    fn print_for_java_comparison() {
        let Ok(file) = std::env::var("ASC_PRINT_FILE") else {
            eprintln!("skipped: set ASC_PRINT_FILE");
            return;
        };
        let mut args: Vec<String> = std::env::var("ASC_PRINT_ARGS").unwrap_or_default().split_whitespace().map(str::to_string).collect();
        args.push(file);
        println!("=====BEGIN=====");
        super::main(&args).unwrap();
        println!("=====END=====");
    }
}
