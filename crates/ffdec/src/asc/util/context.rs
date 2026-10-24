//! Port of `macromedia/asc/util/Context.java` and `ContextStatics.java`, with the parts of `embedding/CompilerHandler`
//! they need.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::asc::parser::factory::NodeFactory;
use crate::asc::parser::input::InputBuffer;
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::parser::ParserRef;
use crate::asc::util::errors::*;

/// The Java `Context` reference shared by the parser, the node factory, nodes and evaluators.
pub type ContextRef = Rc<RefCell<Context>>;

/// `CompilerHandler.FileInclude`.
pub struct FileInclude {
    pub in_: Option<Vec<u8>>,
    pub text: Option<String>,
    pub fixed_filespec: String,
    pub parent_path: String,
}

/// `embedding/ConfigVar.java`.
#[derive(Clone)]
pub struct ConfigVar {
    pub ns: String,
    pub name: String,
    pub value: String,
}

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

    fn find_file_include(&mut self, _parent_path: Option<&str>, _filespec: Option<&str>) -> Option<FileInclude> {
        None
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
    pub node_factory: Option<Rc<RefCell<NodeFactory>>>,
    pub pathspec: Option<String>,
    pub dialect: i32,
    pub use_namespaces: Vec<String>,
    pub es4_nullability: bool,
    pub es4_vectors: bool,
    pub include_paths: Vec<String>,
    pub sem: SemanticStatics,
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
            node_factory: None,
            pathspec: None,
            dialect: 9,
            use_namespaces: Vec::new(),
            es4_nullability: false,
            es4_vectors: true,
            include_paths: Vec::new(),
            sem: SemanticStatics::default(),
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
    pub parser: Option<Rc<ParserRef>>,
    pub input: Option<Rc<RefCell<InputBuffer>>>,

    pub statics: Rc<RefCell<ContextStatics>>,
    pub handler: Option<Rc<RefCell<dyn CompilerHandler>>>,

    pub config_vars: Vec<ConfigVar>,
    pub comments: Vec<NodeRef>,
    pub script_assist_parsing: bool,
    pub sem: SemanticContext,
}

impl Context {
    pub fn new(statics: Rc<RefCell<ContextStatics>>) -> Context {
        Context {
            parser_scanner_input_origin: None,
            qualified_origin: Some(String::new()),
            parser: None,
            input: None,
            statics,
            handler: None,
            config_vars: Vec::new(),
            comments: Vec::new(),
            script_assist_parsing: false,
            sem: SemanticContext::new(),
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

    pub fn set_path(&self, pathspec: Option<String>) {
        self.statics.borrow_mut().pathspec = pathspec;
    }

    pub fn path(&self) -> Option<String> {
        self.statics.borrow().pathspec.clone()
    }

    /// `cx.getNodeFactory()`.
    pub fn get_node_factory(cx: &ContextRef) -> Rc<RefCell<NodeFactory>> {
        let statics = cx.borrow().statics.clone();
        let existing = statics.borrow().node_factory.clone();
        match existing {
            None => {
                let nf = Rc::new(RefCell::new(NodeFactory::new(cx)));
                statics.borrow_mut().node_factory = Some(nf.clone());
                nf
            }
            Some(nf) => {
                nf.borrow_mut().set_context(cx);
                nf
            }
        }
    }

    pub fn dialect(&self, n: i32) -> bool {
        self.statics.borrow().dialect == n
    }

    pub fn get_comments(&self) -> &Vec<NodeRef> {
        &self.comments
    }

    pub fn get_config_var_code(&self) -> Option<String> {
        let mut code = None;
        if !self.config_vars.is_empty() {
            let mut namespaces = HashSet::new();
            let mut code_buffer = String::with_capacity(self.config_vars.len() * 10);
            for cv in &self.config_vars {
                if !namespaces.contains(&cv.ns) {
                    namespaces.insert(cv.ns.clone());
                    code_buffer.push_str(&format!("config namespace {};
", cv.ns));
                }
                code_buffer.push_str(&cv.ns);
                code_buffer.push_str(" const ");
                code_buffer.push_str(&format!("{}={};
", cv.name, cv.value));
            }
            code = Some(code_buffer);
        }
        code
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

// The rest of `Context.java` and `ContextStatics.java`: the semantic model's state.

use std::cell::Cell;

use crate::asc::emit::bytecode::ByteCodeFactory;
use crate::asc::emit::class_builder::new_class_builder;
use crate::asc::emit::features::{TARGET_AVM1, TARGET_AVM2};
use crate::asc::emit::instance_builder::new_instance_builder;
use crate::asc::emit::runtime_constants::*;
use crate::asc::parser::tokens::{GET_TOKEN, SET_TOKEN};
use crate::asc::semantics::namespace_value::new_namespace_value;
use crate::asc::semantics::object_value::{object_value_compare, ObjectValue};
use crate::asc::semantics::q_name::QName;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::type_info::TypeInfo;
use crate::asc::semantics::type_value::TypeValue;
use crate::asc::semantics::unresolved_namespace::new_unresolved_namespace;
use crate::asc::semantics::value::Value;
use crate::asc::util::bit_set::BitSet;
use crate::asc::util::block::Block;
use crate::asc::util::decimal128_context::Decimal128Context;
use crate::asc::util::namespaces::Namespaces;
use crate::asc::util::namespaces_table::NamespacesTable;

pub const NS_PUBLIC: u8 = 0x00;
pub const NS_INTERNAL: u8 = 0x01;
pub const NS_PRIVATE: u8 = 0x02;
pub const NS_PROTECTED: u8 = 0x03;
pub const NS_EXPLICIT: u8 = 0x04;
pub const NS_STATIC_PROTECTED: u8 = 0x05;

pub const NS_INTERNAL_SUFFIX: &str = "$internal";
pub const NS_PRIVATE_SUFFIX: &str = "$private";
pub const NS_PROTECTED_SUFFIX: &str = "$protected";
pub const NS_STATIC_PROTECTED_SUFFIX: &str = "$staticprotected";

pub const MIN_API_MARK: i32 = 0xE000;
pub const MAX_API_MARK: i32 = 0xF8FF;

/// `ContextStatics.useSimpleLogWarnings`.
pub static USE_SIMPLE_LOG_WARNINGS: AtomicBool = AtomicBool::new(false);
/// `ContextStatics.omitTrace`.
pub static OMIT_TRACE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// `Context.contextIds`.
    static CONTEXT_IDS: Cell<i32> = const { Cell::new(0) };
}

/// `Emitter.GetMethodId`, which the base `Builder.Method` asks the context's current emitter for.
pub trait EmitterMethodIds {
    fn get_method_id(&mut self, name: &str, namespaces: &Namespaces) -> i32;
}

/// The lazily created builtin types and singletons (`ContextStatics._noType` and friends).
#[derive(Default)]
pub struct BuiltinCache {
    pub public_namespace: Option<ObjectValue>,
    pub as3_namespace: Option<ObjectValue>,
    pub any_namespace: Option<ObjectValue>,
    pub no_type: Option<TypeValue>,
    pub object_type: Option<TypeValue>,
    pub array_type: Option<TypeValue>,
    pub void_type: Option<TypeValue>,
    pub null_type: Option<TypeValue>,
    pub boolean_type: Option<TypeValue>,
    pub string_type: Option<TypeValue>,
    pub type_type: Option<TypeValue>,
    pub function_type: Option<TypeValue>,
    pub int_type: Option<TypeValue>,
    pub uint_type: Option<TypeValue>,
    pub double_type: Option<TypeValue>,
    pub number_type: Option<TypeValue>,
    pub decimal_type: Option<TypeValue>,
    pub xml_type: Option<TypeValue>,
    pub xml_list_type: Option<TypeValue>,
    pub reg_exp_type: Option<TypeValue>,
    pub vector_type: Option<TypeValue>,
    pub vector_obj_type: Option<TypeValue>,
    pub boolean_true: Option<ObjectValue>,
    pub boolean_false: Option<ObjectValue>,
}

/// The semantic-model fields of `ContextStatics`.
pub struct SemanticStatics {
    pub emitter: Option<Rc<RefCell<dyn EmitterMethodIds>>>,
    pub bytecode_factory: Option<Rc<RefCell<ByteCodeFactory>>>,
    pub scopes: Vec<ObjectValue>,
    pub versions: Vec<i32>,
    pub with_depth: i32,
    pub intern_namespaces: NamespacesTable,
    pub scriptname: Option<String>,
    pub global: Option<ObjectValue>,
    pub builtins: HashMap<String, TypeValue>,
    pub user_defined: HashMap<String, TypeValue>,
    pub namespaces: HashMap<String, ObjectValue>,
    pub internal_namespaces: HashMap<String, ObjectValue>,
    pub protected_namespaces: HashMap<String, ObjectValue>,
    pub static_protected_namespaces: HashMap<String, ObjectValue>,
    pub private_namespaces: HashMap<String, ObjectValue>,
    pub valid_imports: HashSet<String>,
    pub global_prototype: Option<ObjectValue>,
    next_slot_id: i32,
    expected_slot_id: i32,
    pub unresolved_ns_count: i32,
    pub ticket_count: i32,
    pub use_static_semantics: bool,
    pub check_version: bool,
    pub abc_version: i32,
    pub cache: BuiltinCache,
}

impl Default for SemanticStatics {
    fn default() -> Self {
        SemanticStatics {
            emitter: None,
            bytecode_factory: None,
            scopes: Vec::new(),
            versions: Vec::new(),
            with_depth: -1,
            intern_namespaces: NamespacesTable::new(),
            scriptname: None,
            global: None,
            builtins: HashMap::new(),
            user_defined: HashMap::new(),
            namespaces: HashMap::new(),
            internal_namespaces: HashMap::new(),
            protected_namespaces: HashMap::new(),
            static_protected_namespaces: HashMap::new(),
            private_namespaces: HashMap::new(),
            valid_imports: HashSet::new(),
            global_prototype: None,
            next_slot_id: 1,
            expected_slot_id: -1,
            unresolved_ns_count: 0,
            ticket_count: 0,
            use_static_semantics: false,
            check_version: false,
            abc_version: 1,
            cache: BuiltinCache::default(),
        }
    }
}

/// The semantic-model fields of `Context`.
pub struct SemanticContext {
    pub(crate) context_id: i32,
    pub decimal_ctx: Option<Decimal128Context>,
    pub decimal_params: i32,
    /// `TreeMap<UnresolvedNamespace, scopes>` in `ObjectValueCompare` order.
    unresolved_namespaces: RefCell<Option<Vec<(ObjectValue, Vec<ObjectValue>)>>>,
    /// Shared with the contexts copied from this one, as in Java.
    pub def_types: Rc<RefCell<Vec<Option<TypeInfo>>>>,
    pub space_operators: bool,
}

impl SemanticContext {
    pub fn new() -> SemanticContext {
        SemanticContext {
            context_id: CONTEXT_IDS.with(|c| {
                let id = c.get();
                c.set(id + 1);
                id
            }),
            decimal_ctx: None,
            decimal_params: 0,
            unresolved_namespaces: RefCell::new(None),
            def_types: Rc::new(RefCell::new(Vec::new())),
            space_operators: false,
        }
    }
}

impl Default for SemanticContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for ContextStatics {
    type Target = SemanticStatics;
    fn deref(&self) -> &SemanticStatics {
        &self.sem
    }
}

impl std::ops::DerefMut for ContextStatics {
    fn deref_mut(&mut self) -> &mut SemanticStatics {
        &mut self.sem
    }
}

impl ContextStatics {
    pub fn get_next_slot_id(&mut self) -> i32 {
        if self.sem.expected_slot_id != -1 {
            let id = self.sem.expected_slot_id;
            self.sem.expected_slot_id = -1;
            id
        } else {
            let id = self.sem.next_slot_id;
            self.sem.next_slot_id += 1;
            id
        }
    }

    pub fn push_expected_slot_id(&mut self, id: i32) {
        self.sem.expected_slot_id = id;
    }

    /// The namespaces to open automatically for a target player; none since `flash10` was dropped.
    pub fn get_required_use_namespaces(_target_player_major_version: i32) -> Vec<String> {
        Vec::new()
    }

    pub fn get_target_avm(swf_version: i32) -> i32 {
        if swf_version > 9 {
            TARGET_AVM2
        } else {
            TARGET_AVM1
        }
    }

    pub fn set_abc_version(&mut self, target_avm: i32) {
        match target_avm {
            TARGET_AVM1 => {
                self.es4_numerics = false;
                self.es4_nullability = false;
                self.es4_vectors = false;
            }
            TARGET_AVM2 => {
                self.es4_numerics = false;
                self.es4_nullability = false;
                self.es4_vectors = true;
            }
            _ => {}
        }
        self.sem.abc_version = target_avm;
    }

    pub fn clear(&mut self) {
        self.sem.builtins.clear();
        self.error_code_map.clear();
        let as3 = self.sem.cache.as3_namespace.take();
        self.sem.cache = BuiltinCache { as3_namespace: as3, ..BuiltinCache::default() };
        self.sem.namespaces.clear();
        self.sem.internal_namespaces.clear();
        self.sem.protected_namespaces.clear();
        self.sem.static_protected_namespaces.clear();
        self.sem.private_namespaces.clear();
        if let Some(b) = &self.sem.bytecode_factory {
            b.borrow_mut().clear();
        }
        self.err_count = 0;
        self.handler = None;
        self.sem.unresolved_ns_count = 0;
        self.sem.ticket_count = 0;
        self.sem.valid_imports.clear();
    }

    pub fn reuse(&mut self) {
        self.sem.emitter = None;
        self.node_factory = None;
        if let Some(b) = self.sem.bytecode_factory.take() {
            b.borrow_mut().clear();
        }
        self.sem.scopes.clear();
        self.sem.intern_namespaces.clear();
        self.handler = None;
        self.pathspec = None;
        self.sem.scriptname = None;
        self.sem.global = None;
        self.sem.global_prototype = None;
        self.err_count = 0;
        self.sem.unresolved_ns_count = 0;
        self.sem.ticket_count = 0;
        self.include_paths.clear();
        Self::clean_slots(&self.sem.builtins);
        Self::clean_slots(&self.sem.user_defined);
        self.sem.valid_imports.clear();
    }

    fn clean_slots(types: &HashMap<String, TypeValue>) {
        for value in types.values() {
            for s in value.slots().unwrap_or_default() {
                Self::clean_slot(Some(&s));
            }
            if let Some(ov) = value.prototype() {
                for s in ov.slots().unwrap_or_default() {
                    Self::clean_slot(Some(&s));
                }
            }
        }
    }

    pub fn clean_slot(slot: Option<&crate::asc::semantics::slot::Slot>) {
        if let Some(slot) = slot {
            slot.set_impl_node(None);
        }
    }

    /// Drops a package (`mx.controls`) or class (`mx.controls:Button`) name's namespaces.
    pub fn remove_namespace(&mut self, name: &str) {
        self.sem.internal_namespaces.remove(name);
        self.sem.private_namespaces.remove(name);
        self.sem.protected_namespaces.remove(name);
        self.sem.static_protected_namespaces.remove(name);
        self.sem.namespaces.remove(name);
    }

    /// The one namespace value per name and kind.
    pub fn get_namespace(&mut self, name: &str, ns_kind: u8) -> ObjectValue {
        let name = Context::strip_version(name);
        let namespace_map = match ns_kind {
            NS_INTERNAL => &mut self.sem.internal_namespaces,
            NS_PRIVATE => &mut self.sem.private_namespaces,
            NS_PROTECTED => &mut self.sem.protected_namespaces,
            NS_STATIC_PROTECTED => &mut self.sem.static_protected_namespaces,
            _ => &mut self.sem.namespaces,
        };
        if let Some(v) = namespace_map.get(&name) {
            return v.clone();
        }
        let val = new_namespace_value(ns_kind);
        val.set_value(&name);
        val.set_name(&name);
        namespace_map.insert(name, val.clone());
        val
    }
}

impl Context {
    /// `makeCopyOf()`: a shallow copy for reporting errors later against the current file.
    pub fn make_copy_of(&self) -> Context {
        let mut c = Context::new(self.statics.clone());
        // The copy constructor does not take a new id.
        CONTEXT_IDS.with(|n| n.set(n.get() - 1));
        c.handler = self.handler.clone();
        c.sem.context_id = self.sem.context_id;
        c.sem.def_types = self.sem.def_types.clone();
        c.input = self.input.clone();
        c.parser = self.parser.clone();
        c.parser_scanner_input_origin = self.parser_scanner_input_origin.clone();
        c.qualified_origin = self.qualified_origin.clone();
        c.sem.decimal_ctx = self.sem.decimal_ctx.clone();
        c.sem.decimal_params = self.sem.decimal_params;
        c
    }

    pub fn get_id(&self) -> i32 {
        self.sem.context_id
    }

    /// `switchToContext`: this context takes over another's file state, keeping both sets of unresolved namespaces.
    pub fn switch_to_context(&mut self, cx: &Context) {
        self.parser_scanner_input_origin = cx.parser_scanner_input_origin.clone();
        self.qualified_origin = cx.qualified_origin.clone();
        self.parser = cx.parser.clone();
        self.input = cx.input.clone();
        self.statics = cx.statics.clone();
        self.handler = cx.handler.clone();
        let theirs = cx.sem.unresolved_namespaces.borrow().clone();
        if let Some(theirs) = theirs {
            let mut mine = self.sem.unresolved_namespaces.borrow_mut();
            match mine.as_mut() {
                None => *mine = Some(theirs),
                Some(m) => {
                    for (ns, scopes) in theirs {
                        if let Err(i) = m.binary_search_by(|(k, _)| object_value_compare(k, &ns).cmp(&0)) {
                            m.insert(i, (ns, scopes));
                        }
                    }
                }
            }
        }
        self.sem.def_types = cx.sem.def_types.clone();
    }

    pub fn set_compound_names(cx: &ContextRef, compound_names: &[String]) {
        Context::get_node_factory(cx).borrow_mut().init(compound_names);
    }

    pub fn get_unique_namespace_name(&self, base: &str, cx: &Context) -> String {
        let mut s = cx.statics.borrow_mut();
        let name = format!("{}${}", base, s.sem.ticket_count);
        s.sem.ticket_count += 1;
        name
    }

    pub fn get_file_internal_namespace_name(&self) -> String {
        let mut base = self.get_error_origin();
        if let Some(idx) = base.rfind(std::path::MAIN_SEPARATOR) {
            base = base[idx + 1..].to_string();
        }
        self.get_unique_namespace_name(&base, self)
    }

    pub fn exit(&self, _exit_code: i32) {}

    /// Records the type of the definition numbered by the first bit of `def`.
    pub fn set_def_type(&self, def: Option<BitSet>, type_: Option<TypeInfo>) {
        if BitSet::is_empty(&def) {
            return;
        }
        let i = BitSet::next_set_bit(&def, 0) as usize;
        let mut def_types = self.sem.def_types.borrow_mut();
        if i >= def_types.len() {
            def_types.resize(i + 1, None);
            def_types[i] = type_;
        } else if def_types[i].is_none() {
            def_types[i] = type_;
        } else if !type_.as_ref().map_or(false, |t| def_types[i].as_ref().map_or(false, |d| d.ptr_eq(t))) {
            def_types[i] = type_;
        }
    }

    /// The combined type of all reaching definitions; `*` when they disagree.
    pub fn get_def_type(&self, def: Option<BitSet>) -> Option<TypeInfo> {
        let mut type_ = Some(self.void_type().get_default_type_info());
        if BitSet::is_empty(&def) {
            return Some(self.no_type().get_default_type_info());
        }
        let void = self.void_type();
        let mut i = BitSet::next_set_bit(&def, 0);
        let def_types = self.sem.def_types.borrow().clone();
        while i >= 0 {
            let idx = i as usize;
            if idx >= def_types.len() {
                return Some(self.no_type().get_default_type_info());
            } else if type_.as_ref().map_or(false, |t| t.get_type_value().map_or(false, |tv| tv.ptr_eq(&void))) {
                type_ = def_types[idx].clone();
            } else if def_types[idx].is_none() {
            } else if let Some(t) = &type_ {
                if (t.get_type_id() & def_types[idx].as_ref().unwrap().get_type_id()) == 0 {
                    type_ = Some(self.no_type().get_default_type_info());
                }
            }
            i = BitSet::next_set_bit(&def, i + 1);
        }
        type_
    }

    fn push_static_class_scopes_helper(&self, cframe: &TypeValue) {
        if let Some(b) = cframe.baseclass() {
            self.push_static_class_scopes_helper(&b);
        }
        self.push_scope(cframe.as_object().clone());
    }

    /// `pushStaticClassScopes(node)`, given `node.cframe`.
    pub fn push_static_class_scopes(&self, cframe: &TypeValue) {
        self.push_static_class_scopes_helper(cframe);
    }

    /// `popStaticClassScopes(node)`, given `node.cframe`.
    pub fn pop_static_class_scopes(&self, cframe: Option<&TypeValue>) {
        let mut cframe = cframe.cloned();
        while let Some(c) = cframe {
            self.pop_scope();
            cframe = c.baseclass();
        }
    }

    pub fn push_scope(&self, scope: ObjectValue) {
        let mut s = self.statics.borrow_mut();
        if s.sem.scopes.is_empty() {
            s.sem.global = Some(scope.clone());
        }
        s.sem.scopes.push(scope);
    }

    pub fn pop_scope(&self) {
        let mut s = self.statics.borrow_mut();
        s.sem.scopes.pop().expect("NoSuchElementException");
        if s.sem.scopes.is_empty() {
            s.sem.global = None;
        }
    }

    /// A snapshot of the scope chain, outermost first.
    pub fn get_scopes(&self) -> Vec<ObjectValue> {
        self.statics.borrow().sem.scopes.clone()
    }

    pub fn swap_scope_chain(&self, new_scopes: Vec<ObjectValue>) -> Vec<ObjectValue> {
        std::mem::replace(&mut self.statics.borrow_mut().sem.scopes, new_scopes)
    }

    /// The innermost scope, or the global object when the chain is empty.
    pub fn scope(&self) -> Option<ObjectValue> {
        let s = self.statics.borrow();
        match s.sem.scopes.last() {
            Some(sc) => Some(sc.clone()),
            None => s.sem.global.clone(),
        }
    }

    pub fn get_scope_depth(&self) -> usize {
        self.statics.borrow().sem.scopes.len()
    }

    /// `scope(n)`; out of range gives the global object.
    pub fn scope_at(&self, n: i32) -> ObjectValue {
        let s = self.statics.borrow();
        if n >= 0 && (n as usize) < s.sem.scopes.len() {
            s.sem.scopes[n as usize].clone()
        } else {
            s.sem.global.clone().expect("NullPointerException")
        }
    }

    pub fn global_scope(&self) -> Option<ObjectValue> {
        self.statics.borrow().sem.global.clone()
    }

    pub fn builtin_scope(&self) -> Option<ObjectValue> {
        if crate::asc::emit::global_builder::USE_STATIC_BUILTINS {
            self.statics.borrow().sem.global_prototype.clone()
        } else {
            self.global_scope()
        }
    }

    pub fn set_emitter(&self, emitter: Option<Rc<RefCell<dyn EmitterMethodIds>>>) {
        self.statics.borrow_mut().sem.emitter = emitter;
    }

    pub fn get_emitter(&self) -> Option<Rc<RefCell<dyn EmitterMethodIds>>> {
        self.statics.borrow().sem.emitter.clone()
    }

    pub fn set_script_name(&self, scriptname: Option<String>) {
        self.statics.borrow_mut().sem.scriptname = scriptname;
    }

    pub fn script_name(&self) -> Option<String> {
        self.statics.borrow().sem.scriptname.clone()
    }

    /// The node factory `getNodeFactory()` would return, for model code that has only `&Context`; it must exist.
    pub fn node_factory(&self) -> Rc<RefCell<NodeFactory>> {
        self.statics.borrow().node_factory.clone().expect("no node factory yet: call Context::get_node_factory first")
    }

    pub fn get_byte_code_factory(&self) -> Rc<RefCell<ByteCodeFactory>> {
        let mut s = self.statics.borrow_mut();
        s.sem.bytecode_factory.get_or_insert_with(|| Rc::new(RefCell::new(ByteCodeFactory::new()))).clone()
    }

    pub fn new_block(&self) -> Block {
        Block::new()
    }

    /// `cx.scriptAssistParsing`.
    pub fn script_assist_parsing(&self) -> bool {
        self.script_assist_parsing
    }

    /// `cx.parser.error(code)`.
    pub fn parser_error(&self, code: i32) {
        self.parser.as_ref().expect("NullPointerException").error(code);
    }

    pub fn public_namespace(&self) -> ObjectValue {
        if let Some(n) = self.statics.borrow().sem.cache.public_namespace.clone() {
            return n;
        }
        let n = self.get_namespace("");
        self.statics.borrow_mut().sem.cache.public_namespace = Some(n.clone());
        n
    }

    pub fn as3_namespace(&self) -> ObjectValue {
        if let Some(n) = self.statics.borrow().sem.cache.as3_namespace.clone() {
            return n;
        }
        let n = self.get_namespace("");
        self.statics.borrow_mut().sem.cache.as3_namespace = Some(n.clone());
        n
    }

    pub fn any_namespace(&self) -> ObjectValue {
        if let Some(n) = self.statics.borrow().sem.cache.any_namespace.clone() {
            return n;
        }
        let n = self.get_namespace("*");
        self.statics.borrow_mut().sem.cache.any_namespace = Some(n.clone());
        n
    }

    /// Creates a builtin class: the type is cached before its prototype is built, then registered, as in Java.
    fn make_builtin(
        &self,
        ns: ObjectValue,
        name: &str,
        type_id: i32,
        store: fn(&mut BuiltinCache) -> &mut Option<TypeValue>,
        setup: impl FnOnce(&TypeValue),
        builtin_key: &str,
    ) -> TypeValue {
        let qname = QName::new(Some(ns), name);
        let protected_namespace = self.get_namespace_kind(&qname.to_string(), NS_PROTECTED);
        let static_protected_namespace = self.get_namespace_kind(&qname.to_string(), NS_STATIC_PROTECTED);
        let t = TypeValue::new(self, new_class_builder(qname.clone(), Some(protected_namespace), Some(static_protected_namespace)), &qname, type_id);
        *store(&mut self.statics.borrow_mut().sem.cache) = Some(t.clone());
        setup(&t);
        let proto = ObjectValue::with_builder(self, new_instance_builder(qname), Some(&t));
        t.set_prototype(Some(proto));
        self.statics.borrow_mut().sem.builtins.insert(builtin_key.to_string(), t.clone());
        t
    }

    fn cached_type(&self, get: fn(&BuiltinCache) -> &Option<TypeValue>) -> Option<TypeValue> {
        get(&self.statics.borrow().sem.cache).clone()
    }

    pub fn no_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.no_type) {
            return t;
        }
        let t = self.make_builtin(self.public_namespace(), "*", TYPE_none, |c| &mut c.no_type, |_| {}, "*");
        // Done here since there is no class definition.
        t.prototype().unwrap().builder().unwrap().set_is_dynamic(true);
        t
    }

    pub fn object_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.object_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "Object", TYPE_object, |c| &mut c.object_type, |_| {}, "Object")
    }

    pub fn array_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.array_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "Array", TYPE_array, |c| &mut c.array_type, |_| {}, "Array")
    }

    pub fn void_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.void_type) {
            return t;
        }
        let t = self.make_builtin(self.public_namespace(), "void", TYPE_void, |c| &mut c.void_type, |_| {}, "void");
        t.prototype().unwrap().set_value("undefined");
        t
    }

    pub fn null_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.null_type) {
            return t;
        }
        let t = self.make_builtin(self.public_namespace(), "Null", TYPE_null, |c| &mut c.null_type, |_| {}, "Null");
        t.prototype().unwrap().set_value("null");
        t
    }

    pub fn boolean_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.boolean_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "Boolean", TYPE_boolean, |c| &mut c.boolean_type, |_| {}, "Boolean")
    }

    pub fn string_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.string_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "String", TYPE_string, |c| &mut c.string_type, |_| {}, "String")
    }

    pub fn type_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.type_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "Class", TYPE_type, |c| &mut c.type_type, |_| {}, "Class")
    }

    pub fn function_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.function_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "Function", TYPE_function, |c| &mut c.function_type, |_| {}, "Function")
    }

    pub fn int_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.int_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "int", TYPE_int, |c| &mut c.int_type, |_| {}, "int")
    }

    pub fn uint_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.uint_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "uint", TYPE_uint, |c| &mut c.uint_type, |_| {}, "uint")
    }

    /// `Number`, which is also `double` unless ES4 numerics are on.
    pub fn number_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.number_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "Number", TYPE_double, |c| &mut c.number_type, |_| {}, "Number")
    }

    pub fn double_type(&self) -> TypeValue {
        if !self.statics.borrow().es4_numerics {
            return self.number_type();
        }
        if let Some(t) = self.cached_type(|c| &c.double_type) {
            return t;
        }
        let object_type = self.object_type();
        // double is not in older Global.abc files, so its base class is set here.
        self.make_builtin(self.public_namespace(), "double", TYPE_double, |c| &mut c.double_type, move |t| t.set_baseclass(Some(object_type)), "double")
    }

    /// Null unless ES4 numerics are on.
    pub fn decimal_type(&self) -> Option<TypeValue> {
        if self.cached_type(|c| &c.decimal_type).is_none() && self.statics.borrow().es4_numerics {
            self.make_builtin(self.public_namespace(), "decimal", TYPE_decimal, |c| &mut c.decimal_type, |_| {}, "decimal");
        }
        self.cached_type(|c| &c.decimal_type)
    }

    pub fn xml_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.xml_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "XML", TYPE_xml, |c| &mut c.xml_type, |_| {}, "XML")
    }

    pub fn reg_exp_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.reg_exp_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "RegExp", TYPE_object, |c| &mut c.reg_exp_type, |_| {}, "RegExp")
    }

    pub fn xml_list_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.xml_list_type) {
            return t;
        }
        self.make_builtin(self.public_namespace(), "XMLList", TYPE_xml, |c| &mut c.xml_list_type, |_| {}, "XMLList")
    }

    pub fn vector_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.vector_type) {
            return t;
        }
        let ns = self.get_namespace("__AS3__.vec");
        let key = QName::new(Some(ns.clone()), "Vector").to_string();
        let t = self.make_builtin(ns, "Vector", TYPE_object, |c| &mut c.vector_type, |_| {}, &key);
        t.set_is_parameterized(true);
        t
    }

    pub fn vector_obj_type(&self) -> TypeValue {
        if let Some(t) = self.cached_type(|c| &c.vector_obj_type) {
            return t;
        }
        let ns = self.get_namespace_kind("__AS3__.vec", NS_INTERNAL);
        let key = QName::new(Some(ns.clone()), "Vector$object").to_string();
        self.make_builtin(ns, "Vector$object", TYPE_object, |c| &mut c.vector_obj_type, |_| {}, &key)
    }

    pub fn is_builtin(&self, name: &str) -> bool {
        self.statics.borrow().sem.builtins.contains_key(name)
    }

    pub fn builtin(&self, name: &str) -> Option<TypeValue> {
        self.statics.borrow().sem.builtins.get(name).cloned()
    }

    pub fn user_defined(&self, name: &str) -> Option<TypeValue> {
        self.statics.borrow().sem.user_defined.get(name).cloned()
    }

    /// Stores the `TypeValue` for a qualified name, so ABC imports and recompiles reuse it.
    pub fn set_user_defined(&self, name: &str, value: TypeValue) {
        self.statics.borrow_mut().sem.user_defined.insert(name.to_string(), value);
    }

    pub fn remove_user_defined(&self, name: &str) -> Option<TypeValue> {
        self.statics.borrow_mut().sem.user_defined.remove(name)
    }

    pub fn boolean_true(&self) -> ObjectValue {
        if let Some(v) = self.statics.borrow().sem.cache.boolean_true.clone() {
            return v;
        }
        let v = ObjectValue::with_value("true", Some(&self.boolean_type()));
        self.statics.borrow_mut().sem.cache.boolean_true = Some(v.clone());
        v
    }

    pub fn boolean_false(&self) -> ObjectValue {
        if let Some(v) = self.statics.borrow().sem.cache.boolean_false.clone() {
            return v;
        }
        let v = ObjectValue::with_value("false", Some(&self.boolean_type()));
        self.statics.borrow_mut().sem.cache.boolean_false = Some(v.clone());
        v
    }

    /// The API version a namespace URI carries in its last character, or -1.
    pub fn get_version(uri: &str) -> i32 {
        match uri.encode_utf16().last() {
            None => -1,
            Some(last) => {
                let last = last as i32;
                if (MIN_API_MARK..=MAX_API_MARK).contains(&last) {
                    last - MIN_API_MARK
                } else {
                    -1
                }
            }
        }
    }

    pub fn strip_version(uri: &str) -> String {
        if Self::get_version(uri) >= 0 {
            let units: Vec<u16> = uri.encode_utf16().collect();
            return String::from_utf16_lossy(&units[..units.len() - 1]);
        }
        uri.to_string()
    }

    pub fn get_namespace(&self, name: &str) -> ObjectValue {
        self.get_namespace_kind(name, NS_PUBLIC)
    }

    /// `getNamespace(name, ns_kind)`.
    pub fn get_namespace_kind(&self, name: &str, ns_kind: u8) -> ObjectValue {
        self.statics.borrow_mut().get_namespace(name, ns_kind)
    }

    pub fn get_opaque_namespace(&self, name: &str) -> ObjectValue {
        self.get_namespace(name)
    }

    /// Whether a definition name (not a package name) was imported.
    pub fn is_valid_import(&self, name: &str) -> bool {
        self.statics.borrow().sem.valid_imports.contains(name)
    }

    pub fn add_valid_import(&self, name: &str) {
        self.statics.borrow_mut().sem.valid_imports.insert(name.to_string());
    }

    pub fn use_static_semantics(&self) -> bool {
        self.statics.borrow().sem.use_static_semantics
    }

    pub fn check_version(&self) -> bool {
        self.statics.borrow().sem.check_version
    }

    pub fn push_version(&self, v: i32) {
        if self.check_version() {
            self.statics.borrow_mut().sem.versions.push(v);
        }
    }

    pub fn pop_version(&self) -> i32 {
        if self.check_version() {
            return self.statics.borrow_mut().sem.versions.pop().expect("IndexOutOfBoundsException");
        }
        0
    }

    pub fn version(&self) -> i32 {
        self.statics.borrow().sem.versions.last().copied().unwrap_or(0)
    }

    /// Whether the target VM version is at least `n`.
    pub fn abc_version(&self, n: i32) -> bool {
        self.statics.borrow().sem.abc_version >= n
    }

    /// `isNamespace(String ns_name)`.
    pub fn is_namespace_name(&self, ns_name: &str) -> bool {
        self.statics.borrow().sem.namespaces.contains_key(ns_name)
    }

    /// `isNamespace(ObjectValue obj)`.
    pub fn is_namespace(&self, obj: Option<&ObjectValue>) -> bool {
        let obj = match obj {
            Some(o) => o,
            None => return false,
        };
        let s = self.statics.borrow();
        let name = obj.name();
        if obj.is_private() && s.sem.private_namespaces.contains_key(&name) {
            true
        } else if obj.is_protected() && s.sem.protected_namespaces.contains_key(&name) {
            true
        } else if obj.is_internal() && s.sem.internal_namespaces.contains_key(&name) {
            true
        } else {
            s.sem.namespaces.contains_key(&name)
        }
    }

    /// A placeholder namespace for a reference that cannot be resolved yet; one per name and namespace names.
    pub fn get_unresolved_namespace(&self, cx: &Context, node: Option<NodeRef>, ref_: &ReferenceValue) -> ObjectValue {
        {
            let mut un = self.sem.unresolved_namespaces.borrow_mut();
            let un = un.get_or_insert_with(Vec::new);
            for (ns, _) in un.iter() {
                let val = ns.with_unresolved(|u| u.ref_.clone()).expect("NullPointerException");
                let vn = val.get_immutable_namespaces().to_vec();
                let rn = ref_.get_immutable_namespaces().to_vec();
                if val.name() == ref_.name() && vn.len() == rn.len() {
                    if vn.iter().zip(rn.iter()).all(|(a, b)| *a.name_ref() == *b.name_ref()) {
                        return ns.clone();
                    }
                }
            }
        }
        let ns = new_unresolved_namespace(cx, node, ref_.clone());
        let count = {
            let mut s = self.statics.borrow_mut();
            let c = s.sem.unresolved_ns_count;
            s.sem.unresolved_ns_count += 1;
            c
        };
        ns.set_name(&format!("__unresolved__ns__{}", count));
        let scopes = self.get_scopes();
        let mut un = self.sem.unresolved_namespaces.borrow_mut();
        let un = un.as_mut().unwrap();
        match un.binary_search_by(|(k, _)| object_value_compare(k, &ns).cmp(&0)) {
            Ok(i) => un[i].1 = scopes,
            Err(i) => un.insert(i, (ns.clone(), scopes)),
        }
        ns
    }

    /// Resolves the queued namespaces against the scopes they were seen in, reporting the ones that are unknown.
    pub fn process_unresolved_namespaces(&self) {
        let ns_list: Vec<ObjectValue> = match self.sem.unresolved_namespaces.borrow().as_ref() {
            None => return,
            Some(un) => un.iter().map(|(k, _)| k.clone()).collect(),
        };
        for ns in &ns_list {
            let scopes = {
                let un = self.sem.unresolved_namespaces.borrow();
                let un = un.as_ref().unwrap();
                un.binary_search_by(|(k, _)| object_value_compare(k, ns).cmp(&0)).ok().map(|i| un[i].1.clone()).unwrap_or_default()
            };
            let temp = self.swap_scope_chain(scopes);
            let r = ns.with_unresolved(|u| u.ref_.clone()).expect("NullPointerException");
            let slot = r.get_slot(self);
            self.swap_scope_chain(temp);
            match slot {
                Some(slot) => {
                    if let Some(Value::Object(real_namespace)) = slot.get_value() {
                        // Remove before renaming, which would change the entry's place in the ordering.
                        {
                            let mut un = self.sem.unresolved_namespaces.borrow_mut();
                            let un = un.as_mut().unwrap();
                            if let Ok(i) = un.binary_search_by(|(k, _)| object_value_compare(k, ns).cmp(&0)) {
                                un.remove(i);
                            }
                        }
                        ns.set_name(&real_namespace.name());
                        ns.set_ns_kind(real_namespace.get_namespace_kind());
                        ns.with_unresolved(|u| u.resolved = true);
                    }
                }
                None => {
                    // The node may come from an included file, so report against the context it was seen in.
                    let (ucx, pos) = ns.with_unresolved(|u| (u.cx.clone(), u.node.as_ref().map(|n| n.borrow().pos())));
                    ucx.expect("NullPointerException").borrow().error(pos.expect("NullPointerException"), kError_Unknown_Namespace);
                }
            }
            ns.with_unresolved(|u| {
                u.cx = None;
                u.node = None;
                u.ref_ = None;
            });
        }
        *self.sem.unresolved_namespaces.borrow_mut() = None;
    }

    pub fn debug_name(&self, region_part: &str, name: &str, namespace_ids: Option<&[String]>, kind: i32) -> String {
        let kind_part = if kind == GET_TOKEN {
            "/get"
        } else if kind == SET_TOKEN {
            "/set"
        } else {
            ""
        };
        let mut namespace_part: Vec<u16> = region_part.encode_utf16().collect();
        let mut region_part_length = namespace_part.len();
        if region_part_length > 0 {
            namespace_part.push(b'/' as u16);
            region_part_length += 1;
        }
        if let Some(ids) = namespace_ids {
            let last = ids.len() as i32 - 1;
            let mut n = last;
            while n >= 0 {
                let id = &ids[n as usize];
                if id.is_empty() || n == last {
                } else {
                    namespace_part.push(b'|' as u16);
                }
                namespace_part.extend(id.encode_utf16());
                n -= 1;
            }
        }
        if namespace_part.len() > region_part_length {
            namespace_part.push(b':' as u16);
        }
        let mut s = String::from_utf16_lossy(&namespace_part);
        s.push_str(name);
        s.push_str(kind_part);
        s
    }

    pub fn compute_qualified_name(&self, region_part: &str, name: &str, qualifier: &ObjectValue, kind: i32) -> QName {
        let kind_part = if kind == GET_TOKEN {
            "/get"
        } else if kind == SET_TOKEN {
            "/set"
        } else {
            ""
        };
        let namespace_part = qualifier.name();
        let ns_kind = qualifier.get_namespace_kind();
        let ns_name = if !region_part.is_empty() { format!("{}{}", region_part, namespace_part) } else { namespace_part };
        let n = if !kind_part.is_empty() { format!("{}{}", name, kind_part) } else { name.to_string() };
        QName::new(Some(self.get_namespace_kind(&ns_name, ns_kind)), &n)
    }
}

impl Context {
    /// `coerce(expr, actual, expected, isExplicit, force)`: wraps `expr` in a coercion to `expected` when needed,
    /// reporting static type errors. `actual` is Java's one-element in/out array.
    pub fn coerce(&self, expr: NodeRef, actual: &mut Option<TypeInfo>, expected: Option<TypeInfo>, is_explicit: bool, force: bool) -> NodeRef {
        use crate::asc::parser::tokens::{LOGICALAND_TOKEN, LOGICALOR_TOKEN};
        let void = self.void_type();
        let is_list = expr.borrow().is_list();
        if is_list {
            // A compound expression: only its last item is coerced.
            let last = expr.borrow().list().unwrap().items.last().cloned().flatten().expect("NullPointerException");
            let last = self.coerce(last, actual, expected, is_explicit, force);
            let mut e = expr.borrow_mut();
            let items = &mut e.list_mut().unwrap().items;
            items.pop();
            items.push(Some(last));
            drop(e);
            return expr;
        }
        let cond = expr.borrow().as_conditional_expression().map(|c| (c.thenvalue.clone(), c.thenexpr.clone(), c.elsevalue.clone(), c.elseexpr.clone()));
        if let Some((thenvalue, thenexpr, elsevalue, elseexpr)) = cond {
            // Coerce each branch instead of the result, forcing it since the join node cannot compensate.
            *actual = thenvalue.and_then(|v| v.get_type(self));
            let thenexpr = self.coerce(thenexpr.expect("NullPointerException"), actual, expected.clone(), is_explicit, true);
            expr.borrow_mut().as_conditional_expression_mut().unwrap().thenexpr = Some(thenexpr);
            *actual = elsevalue.and_then(|v| v.get_type(self));
            let elseexpr = self.coerce(elseexpr.expect("NullPointerException"), actual, expected, is_explicit, true);
            expr.borrow_mut().as_conditional_expression_mut().unwrap().elseexpr = Some(elseexpr);
            return expr;
        }
        let binary = expr.borrow().as_binary_expression().map(|b| (b.op, b.lhs.clone(), b.rhs.clone(), b.lhstype.clone(), b.rhstype.clone()));
        if let Some((op, lhs, rhs, lhstype, rhstype)) = binary {
            let expected_not_void = expected.as_ref().map_or(false, |e| !e.get_type_value().map_or(false, |t| t.ptr_eq(&void)));
            let types_differ = match (&lhstype, &rhstype) {
                (Some(a), Some(b)) => !a.ptr_eq(b),
                (None, None) => false,
                _ => true,
            };
            if (op == LOGICALAND_TOKEN || op == LOGICALOR_TOKEN) && (expected_not_void || types_differ) {
                // Short-circuit operators: push the coercion into both operands so their types agree at the join; a
                // void expectation becomes `*` so that the operands still leave a value.
                let expected2 = expected.as_ref().map(|e| if e.get_type_value().map_or(false, |t| t.ptr_eq(&void)) { self.no_type().get_default_type_info() } else { e.clone() });
                *actual = lhstype;
                let lhs = self.coerce(lhs.expect("NullPointerException"), actual, expected2.clone(), is_explicit, true);
                {
                    let mut e = expr.borrow_mut();
                    let b = e.as_binary_expression_mut().unwrap();
                    b.lhs = Some(lhs);
                    b.lhstype = actual.clone();
                }
                *actual = rhstype;
                let rhs = self.coerce(rhs.expect("NullPointerException"), actual, expected2, is_explicit, true);
                {
                    let mut e = expr.borrow_mut();
                    let b = e.as_binary_expression_mut().unwrap();
                    b.rhs = Some(rhs);
                    b.rhstype = actual.clone();
                }
                return expr;
            }
        }

        let nf = self.node_factory();
        let expected = match expected {
            // A type-sniffing operator keeps the actual type, unless it feeds a join node, which needs `*`.
            None => {
                if force {
                    let star = self.no_type().get_default_type_info();
                    *actual = Some(star.clone());
                    return nf.borrow_mut().coerce(Some(expr), None, Some(star), is_explicit);
                }
                return expr;
            }
            Some(e) => e,
        };

        if expected.get_type_value().map_or(false, |t| t.ptr_eq(&void)) {
            *actual = Some(expected);
            expr.borrow_mut().void_result();
            return expr;
        } else if actual.is_none() {
            *actual = Some(expected.clone());
            return nf.borrow_mut().coerce(Some(expr), None, Some(expected), is_explicit);
        } else if actual.as_ref().map_or(false, |a| a.ptr_eq(&expected)) || expected.includes(self, actual.as_ref().unwrap()) {
            return if force { nf.borrow_mut().coerce(Some(expr), None, Some(expected), is_explicit) } else { expr };
        } else if self.use_static_semantics() {
            let a = actual.clone().unwrap();
            let atv = a.type_value();
            let etv = expected.type_value();
            // null is always valid and everything converts to Boolean.
            if !atv.ptr_eq(&self.null_type()) && !etv.ptr_eq(&self.boolean_type()) {
                let pos = expr.borrow().pos();
                if atv.get_type_info(true).includes(self, &expected) {
                    // A widening cast.
                    if atv.ptr_eq(&self.vector_obj_type()) && etv.baseclass().map_or(false, |b| b.ptr_eq(&self.vector_obj_type())) {
                        // Vector.<*> coerces to Vector.<T>.
                    } else if !is_explicit && !atv.ptr_eq(&self.no_type()) {
                        // Coercion from Object is always allowed; flagging it would be too noisy.
                        self.error2(pos, kError_ImplicitCoercionToSubtype, &a.get_name_cx(self).to_string(), &expected.get_name_cx(self).to_string());
                    }
                } else if etv.ptr_eq(&self.string_type()) && (atv.ptr_eq(&self.xml_type()) || atv.ptr_eq(&self.xml_list_type())) {
                    // E4X values are strings without an explicit toString().
                } else if !(expected.is_numeric(self) && a.is_numeric(self)) {
                    self.error2(pos, kError_ImplicitCoercisionOfUnrelatedType, &a.get_name_cx(self).to_string(), &expected.get_name_cx(self).to_string());
                }
            }
        }

        if force || is_explicit {
            nf.borrow_mut().coerce(Some(expr), actual.clone(), Some(expected), is_explicit)
        } else {
            expr
        }
    }
}
