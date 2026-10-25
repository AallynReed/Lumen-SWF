//! Port of the parts of `RuntimeInstance`, `VelocimacroFactory` and `VelocimacroManager` the flex2 setup uses: the default
//! directives, Velocity's default properties (with flex's overrides), global-namespace Velocimacros and the log system.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::directive::VelocimacroProxy;
use super::introspection::Info;
use super::node::Node;
use super::parser::{BLOCK, LINE};

pub const DEBUG_ID: i32 = 0;
pub const INFO_ID: i32 = 1;
pub const WARN_ID: i32 = 2;
pub const ERROR_ID: i32 = 3;

/// `directive.foreach.counter.name` / `.initial.value`.
pub const COUNTER_NAME: &str = "velocityCount";
pub const COUNTER_INITIAL_VALUE: i32 = 1;

pub type LogSystem = Box<dyn Fn(i32, &str)>;

/// `VelocimacroManager.MacroEntry`.
pub struct MacroEntry {
    pub macroname: String,
    pub argarray: Vec<String>,
    /// `None` for the parse-time registration (only its name is used later).
    pub node_tree: Option<Rc<Node>>,
    pub sourcetemplate: String,
}

/// What `StrictUberspectImpl` reports when a property getter or setter is not found.
pub struct UberspectMiss<'a> {
    pub is_set: bool,
    pub info: &'a Info,
    pub identifier: Option<&'a str>,
    pub class_name: &'a str,
}

pub type StrictUberspect = Box<dyn Fn(&UberspectMiss)>;

pub struct RuntimeInstance {
    macros: RefCell<HashMap<String, MacroEntry>>,
    /// `StrictUberspectImpl`'s reporting; `None` is plain `UberspectImpl`.
    strict_uberspect: Option<StrictUberspect>,
    log_system: Option<LogSystem>,
}

impl RuntimeInstance {
    pub fn new(strict_uberspect: Option<StrictUberspect>, log_system: Option<LogSystem>) -> Rc<RuntimeInstance> {
        Rc::new(RuntimeInstance { macros: RefCell::new(HashMap::new()), strict_uberspect, log_system })
    }

    pub(crate) fn strict_get_miss(&self, info: &Info, identifier: Option<&str>, class_name: &str) {
        if let Some(s) = &self.strict_uberspect {
            s(&UberspectMiss { is_set: false, info, identifier, class_name });
        }
    }

    pub(crate) fn strict_set_miss(&self, info: &Info, identifier: Option<&str>, class_name: &str) {
        if let Some(s) = &self.strict_uberspect {
            s(&UberspectMiss { is_set: true, info, identifier, class_name });
        }
    }

    /// The type of a directive from `directive.properties`.
    pub fn directive_type(&self, name: &str) -> Option<i32> {
        match name {
            "foreach" | "macro" | "literal" => Some(BLOCK),
            "include" | "parse" => Some(LINE),
            _ => None,
        }
    }

    fn log(&self, level: i32, message: &str) {
        if let Some(l) = &self.log_system {
            l(level, message);
        }
    }

    pub fn warn(&self, message: &str) {
        self.log(WARN_ID, message);
    }

    pub fn info(&self, message: &str) {
        self.log(INFO_ID, message);
    }

    pub fn error(&self, message: &str) {
        self.log(ERROR_ID, message);
    }

    pub fn debug(&self, message: &str) {
        self.log(DEBUG_ID, message);
    }

    /// `VelocimacroFactory.addVelocimacro`; inline macros may not replace an existing one.
    pub fn add_velocimacro(
        &self,
        name: &str,
        body: Option<Rc<Node>>,
        args: Vec<String>,
        source_template: &str,
    ) -> bool {
        if self.is_velocimacro(name, source_template) {
            self.warn(&format!(
                "Velocimacro : VM addition rejected : {name} : inline not allowed to replace existing VM"
            ));
            return false;
        }
        let mut s = format!("#{name}(");
        for a in &args {
            s.push(' ');
            s.push_str(a);
        }
        s.push_str(&format!(" ) : source = {source_template}"));
        self.macros.borrow_mut().insert(
            name.to_string(),
            MacroEntry {
                macroname: name.to_string(),
                argarray: args,
                node_tree: body,
                sourcetemplate: source_template.to_string(),
            },
        );
        self.info(&format!("Velocimacro : added new VM : {s}"));
        true
    }

    pub fn is_velocimacro(&self, name: &str, _template_name: &str) -> bool {
        self.macros.borrow().contains_key(name)
    }

    /// `VelocimacroFactory.getVelocimacro`: a new proxy per call site.
    pub fn get_velocimacro(&self, name: &str, template_name: &str) -> Option<VelocimacroProxy> {
        self.macros.borrow().get(name).map(|me| VelocimacroProxy::new(me, template_name))
    }
}
