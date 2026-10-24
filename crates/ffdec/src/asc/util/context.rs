//! Start of a port of `macromedia/asc/util/Context.java`, with the parts of `ContextStatics` and `embedding/CompilerHandler` it needs so far.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::asc::parser::input::InputBuffer;
use crate::asc::util::errors::*;

pub trait CompilerHandler {
    fn warning(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str, _code: i32) {
        self.warning_no_code(filename, ln, col, msg, source);
    }

    fn error(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str, _code: i32) {
        self.error_no_code(filename, ln, col, msg, source);
    }

    fn error_no_code(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str) {
        let source16: Vec<u16> = source.encode_utf16().collect();
        let size: i32 = if col > 70 {
            col + 10
        } else if source16.len() > 70 {
            70
        } else {
            source16.len() as i32
        };

        let mut source = source.to_string();
        if size < source16.len() as i32 {
            source = String::from_utf16_lossy(&source16[..size as usize]) + "...";
        }

        eprintln!();
        eprintln!("{}", msg);

        if ln >= 0 {
            eprintln!("   {}, Ln {}, Col {}: ", filename, ln, col);
        }

        if !source.is_empty() {
            eprintln!("   {}", source);
            eprintln!("   {}", InputBuffer::get_line_pointer(col));
        }
    }

    fn warning_no_code(&mut self, filename: &str, ln: i32, col: i32, msg: &str, source: &str) {
        self.error_no_code(filename, ln, col, msg, source);
    }
}

/// `new CompilerHandler()`: the base class with no overrides.
pub struct DefaultCompilerHandler;

impl CompilerHandler for DefaultCompilerHandler {}

pub static USE_VERBOSE_ERRORS: AtomicBool = AtomicBool::new(true);
pub static USE_SANITY_STYLE_ERRORS: AtomicBool = AtomicBool::new(false);

pub struct ContextStatics {
    pub handler: Option<Rc<RefCell<dyn CompilerHandler>>>,
    pub error_code_map: HashMap<i32, String>,
    pub language_id: i32,
    pub es4_numerics: bool,
    pub err_count: i32,
}

impl ContextStatics {
    pub const LANG_EN: i32 = 0;
    pub const LANG_CN: i32 = 1;
    pub const LANG_CS: i32 = 2;
    pub const LANG_DA: i32 = 3;
    pub const LANG_DE: i32 = 4;
    pub const LANG_ES: i32 = 5;
    pub const LANG_FI: i32 = 6;
    pub const LANG_FR: i32 = 7;
    pub const LANG_IT: i32 = 8;
    pub const LANG_JP: i32 = 9;
    pub const LANG_KR: i32 = 10;
    pub const LANG_NB: i32 = 11;
    pub const LANG_NL: i32 = 12;
    pub const LANG_PL: i32 = 13;
    pub const LANG_PT: i32 = 14;
    pub const LANG_RU: i32 = 15;
    pub const LANG_SV: i32 = 16;
    pub const LANG_TR: i32 = 17;
    pub const LANG_TW: i32 = 18;

    pub fn new() -> ContextStatics {
        ContextStatics {
            handler: None,
            error_code_map: HashMap::new(),
            language_id: 0,
            es4_numerics: false,
            err_count: 0,
        }
    }
}

impl Default for ContextStatics {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Context {
    parser_scanner_input_origin: Option<String>,
    qualified_origin: Option<String>,
    pub input: Option<Rc<RefCell<InputBuffer>>>,

    pub statics: Rc<RefCell<ContextStatics>>,
    pub handler: Option<Rc<RefCell<dyn CompilerHandler>>>,
}

impl Context {
    pub fn new(statics: Rc<RefCell<ContextStatics>>) -> Context {
        Context {
            parser_scanner_input_origin: None,
            qualified_origin: Some(String::new()),
            input: None,
            statics,
            handler: None,
        }
    }

    /// `None` where Java returns null (string concatenation then shows "null").
    pub fn error_string(&self, error: i32) -> Option<String> {
        let mut statics = self.statics.borrow_mut();
        if !statics.error_code_map.contains_key(&error) {
            let error_consts = ALL_ERROR_CONSTANTS[statics.language_id as usize];
            for x in 0..kNumErrorConstants as usize {
                statics.error_code_map.insert(error_consts[x].code, error_consts[x].error_msg.to_string());
            }
        }

        statics.error_code_map.get(&error).cloned()
    }

    pub fn replace_string_arg(out: &mut String, template_str: &str, start_loc: i32, arg: &str) -> i32 {
        if start_loc == -1 {
            return -1;
        }

        let template: Vec<u16> = template_str.encode_utf16().collect();
        let mut next_loc = -1;
        let template_len = template.len() as i32;
        let subst_loc = index_of(&template, b'%' as u16, start_loc);

        if subst_loc != -1 {
            if subst_loc > start_loc {
                out.push_str(&substring_utf16(&template, start_loc, subst_loc));
            }
            out.push_str(arg);
            next_loc = subst_loc + 2;
        } else {
            out.push_str(&substring_utf16(&template, start_loc, template_len));
        }

        next_loc
    }

    pub fn shell_error_string(&self, err_code: i32) -> String {
        let prefix = if USE_VERBOSE_ERRORS.load(Ordering::Relaxed) {
            format!("[Compiler] Error #{}: ", err_code)
        } else {
            String::new()
        };
        prefix + &self.error_string(err_code).unwrap_or_else(|| "null".to_string())
    }

    pub fn error(&self, pos: i32, error: i32) {
        self.error3(pos, error, "", "", "");
    }

    pub fn error1(&self, pos: i32, error: i32, arg1: &str) {
        self.error3(pos, error, arg1, "", "");
    }

    pub fn error2(&self, pos: i32, error: i32, arg1: &str, arg2: &str) {
        self.error3(pos, error, arg1, arg2, "");
    }

    pub fn error3(&self, pos: i32, error: i32, arg1: &str, arg2: &str, arg3: &str) {
        let mut out = String::new();

        if USE_SANITY_STYLE_ERRORS.load(Ordering::Relaxed) {
            out.push_str(&format!("code={}; arg1={}; arg2={}; arg3={}", error, arg1, arg2, arg3));
        } else {
            let template_str = self.shell_error_string(error);
            let mut next_loc = Self::replace_string_arg(&mut out, &template_str, 0, arg1);
            next_loc = Self::replace_string_arg(&mut out, &template_str, next_loc, arg2);
            next_loc = Self::replace_string_arg(&mut out, &template_str, next_loc, arg3);
            if next_loc != -1 {
                out.push_str(&java_substring(&template_str, next_loc, java_length(&template_str)));
            }
        }

        self.localized_error_file_code(&self.get_error_origin(), pos, &out, error);
    }

    pub fn localized_error(&self, pos: i32, msg: &str) {
        self.localized_error_code(pos, msg, -1);
    }

    pub fn localized_error_code(&self, pos: i32, msg: &str, code: i32) {
        let pos = if pos < 0 { 0 } else { pos };

        let ln = self.get_input_line(pos);
        let col = self.get_input_col(pos);

        self.localized_error_ln_col_code(&self.get_error_origin(), ln, col, msg, &self.get_error_line_text(pos), code);
    }

    pub fn localized_warning(&self, pos: i32, msg: &str) {
        self.localized_warning_code(pos, msg, -1);
    }

    pub fn localized_warning_code(&self, pos: i32, msg: &str, code: i32) {
        let pos = if pos < 0 { 0 } else { pos };

        let ln = self.get_input_line(pos);
        let col = self.get_input_col(pos);

        self.localized_warning_ln_col_code(&self.get_error_origin(), ln, col, msg, &self.get_error_line_text(pos), code);
    }

    pub fn localized_error_file(&self, filename: &str, pos: i32, msg: &str) {
        self.localized_error_file_code(filename, pos, msg, -1);
    }

    pub fn localized_error_file_code(&self, filename: &str, pos: i32, msg: &str, code: i32) {
        let pos = if pos < 0 { 0 } else { pos };

        let ln = self.get_input_line(pos);
        let col = self.get_input_col(pos);

        self.localized_error_ln_col_code(filename, ln, col, msg, &self.get_error_line_text(pos), code);
    }

    pub fn localized_warning_file_code(&self, filename: &str, pos: i32, msg: &str, code: i32) {
        let pos = if pos < 0 { 0 } else { pos };

        let ln = self.get_input_line(pos);
        let col = self.get_input_col(pos);

        self.localized_warning_ln_col_code(filename, ln, col, msg, &self.get_error_line_text(pos), code);
    }

    fn missing_handler(&self, filename: &str, ln: i32, col: i32, msg: &str, source: &str) {
        eprintln!("{}", msg);
        eprintln!("   {}, Ln {}, Col {}: ", filename, ln, col);

        if !source.is_empty() {
            eprintln!("   {}", source);
            eprintln!("   {}", InputBuffer::get_line_pointer(col));
        }
        eprintln!();
    }

    pub fn get_input_line(&self, pos: i32) -> i32 {
        let mut ret = -1;
        if let Some(input) = &self.input {
            ret = input.borrow_mut().get_ln_num(pos);
        }
        ret
    }

    pub fn get_input_col(&self, pos: i32) -> i32 {
        let mut ret = -1;
        if let Some(input) = &self.input {
            ret = input.borrow_mut().get_col_pos(pos);
        }
        ret
    }

    pub fn get_error_line_text(&self, pos: i32) -> String {
        let mut ret = String::new();
        if let Some(input) = &self.input {
            ret = input.borrow_mut().get_line_text(pos);
        }
        ret
    }

    pub fn get_error_origin(&self) -> String {
        match &self.parser_scanner_input_origin {
            Some(o) => o.clone(),
            None => String::new(),
        }
    }

    pub fn get_qualified_error_origin(&self) -> String {
        if let Some(q) = &self.qualified_origin {
            if !q.is_empty() {
                return q.clone();
            }
        }

        self.get_error_origin()
    }

    pub fn localized_error_ln_col(&self, filename: &str, ln: i32, col: i32, msg: &str, source: &str) {
        self.localized_error_ln_col_code(filename, ln, col, msg, source, -1);
    }

    pub fn localized_error_ln_col_code(&self, filename: &str, ln: i32, col: i32, msg: &str, source: &str, code: i32) {
        let statics_handler = self.statics.borrow().handler.clone();
        if let Some(handler) = &self.handler {
            handler.borrow_mut().error(filename, ln, col, msg, source, code);
        } else if let Some(handler) = statics_handler {
            handler.borrow_mut().error(filename, ln, col, msg, source, code);
        } else {
            self.missing_handler(filename, ln, col, msg, source);
        }
        self.statics.borrow_mut().err_count += 1;
    }

    fn localized_warning_ln_col_code(&self, filename: &str, ln: i32, col: i32, msg: &str, source: &str, code: i32) {
        let statics_handler = self.statics.borrow().handler.clone();
        if let Some(handler) = &self.handler {
            handler.borrow_mut().warning(filename, ln, col, msg, source, code);
        } else if let Some(handler) = statics_handler {
            handler.borrow_mut().warning(filename, ln, col, msg, source, code);
        } else {
            self.missing_handler(filename, ln, col, msg, source);
        }
    }

    pub fn internal_error_at(&self, pos: i32, msg: &str) {
        self.localized_error_file_code(&self.get_error_origin(), pos, msg, kError_InternalError);
    }

    pub fn internal_error(&self, msg: &str) {
        self.internal_error_at(-1, msg);
    }

    pub fn error_count(&self) -> i32 {
        self.statics.borrow().err_count
    }

    pub fn set_handler(&mut self, handler: Rc<RefCell<dyn CompilerHandler>>) {
        self.handler = Some(handler.clone());

        let mut statics = self.statics.borrow_mut();
        if statics.handler.is_none() {
            statics.handler = Some(handler);
        }
    }

    pub fn set_language(&self, language: &str) {
        self.statics.borrow_mut().language_id = Self::get_language_id(language);
    }

    pub fn get_language_id(language: &str) -> i32 {
        match language {
            "EN" => ContextStatics::LANG_EN,
            "CN" => ContextStatics::LANG_CN,
            "CS" => ContextStatics::LANG_CS,
            "DK" => ContextStatics::LANG_DA,
            "DE" => ContextStatics::LANG_DE,
            "ES" => ContextStatics::LANG_ES,
            "FI" => ContextStatics::LANG_FI,
            "FR" => ContextStatics::LANG_FR,
            "IT" => ContextStatics::LANG_IT,
            "JP" => ContextStatics::LANG_JP,
            "KR" => ContextStatics::LANG_KR,
            "NO" => ContextStatics::LANG_NB,
            "NL" => ContextStatics::LANG_NL,
            "PL" => ContextStatics::LANG_PL,
            "BR" => ContextStatics::LANG_PT,
            "RU" => ContextStatics::LANG_RU,
            "SE" => ContextStatics::LANG_SV,
            "TR" => ContextStatics::LANG_TR,
            "TW" => ContextStatics::LANG_TW,
            _ => ContextStatics::LANG_EN,
        }
    }

    pub fn get_handler(&self) -> Option<Rc<RefCell<dyn CompilerHandler>>> {
        self.handler.clone()
    }

    pub fn set_origin(&mut self, origin: &str) {
        self.parser_scanner_input_origin = Some(origin.to_string());
    }

    pub fn set_qualified_origin(&mut self, qualified_origin: Option<String>) {
        self.qualified_origin = qualified_origin;
    }
}

/// Java `String.length()`.
pub fn java_length(s: &str) -> i32 {
    s.encode_utf16().count() as i32
}

/// Java `String.substring(begin, end)`; out-of-range bounds panic like Java throws.
pub fn java_substring(s: &str, begin: i32, end: i32) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    substring_utf16(&units, begin, end)
}

fn substring_utf16(units: &[u16], begin: i32, end: i32) -> String {
    if begin < 0 || end > units.len() as i32 || begin > end {
        panic!("String index out of range: begin {}, end {}, length {}", begin, end, units.len());
    }
    String::from_utf16_lossy(&units[begin as usize..end as usize])
}

fn index_of(units: &[u16], ch: u16, from: i32) -> i32 {
    let from = from.max(0) as usize;
    for (i, &u) in units.iter().enumerate().skip(from) {
        if u == ch {
            return i as i32;
        }
    }
    -1
}
