//! Port of `flex2.compiler.util.VelocityManager`: one engine per macro library, a template cache, `getCodeGenContext`, the
//! `Util` context object with its event handlers, `StrictUberspectImpl`'s reporting and the `Logger` log system.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::context::{EventCartridge, MethodExceptionEventHandler, ReferenceInsertionEventHandler, VelocityContext};
use super::introspection::{JType, JavaException, JavaObject, Method, Value};
use super::runtime::{RuntimeInstance, UberspectMiss, ERROR_ID};
use super::template::Template;
use super::VelocityError;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::util::jstr;

/// The `.vm` sources mxmlc.jar ships pre-parsed as `.vms`, by classpath resource path.
const TEMPLATES: &[(&str, &str)] = &[
    ("flex2/compiler/as3/EmbedClass.vm", include_str!("templates/EmbedClass.vm")),
    ("flex2/compiler/as3/binding/BindableProperty.vm", include_str!("templates/BindableProperty.vm")),
    ("flex2/compiler/as3/binding/WatcherSetupUtil.vm", include_str!("templates/WatcherSetupUtil.vm")),
    ("flex2/compiler/as3/managed/ManagedProperty.vm", include_str!("templates/ManagedProperty.vm")),
    ("flex2/compiler/css/FontFaceRules.vm", include_str!("templates/FontFaceRules.vm")),
    ("flex2/compiler/css/StyleDef.vm", include_str!("templates/StyleDef.vm")),
    ("flex2/compiler/css/StyleLibrary.vm", include_str!("templates/StyleLibrary.vm")),
    ("flex2/compiler/css/StyleModule.vm", include_str!("templates/StyleModule.vm")),
    ("flex2/compiler/media/SkinClass.vm", include_str!("templates/SkinClass.vm")),
    ("flex2/compiler/mxml/gen/ClassDef.vm", include_str!("templates/ClassDef.vm")),
    ("flex2/compiler/mxml/gen/ClassDefLib.vm", include_str!("templates/ClassDefLib.vm")),
    ("flex2/compiler/mxml/gen/InterfaceDef.vm", include_str!("templates/InterfaceDef.vm")),
];

const UTIL_KEY: &str = "util";

struct Manager {
    engines: HashMap<String, Rc<RuntimeInstance>>,
    templates: HashMap<String, Rc<Template>>,
    /// Each engine's macro library, which only its macros reference in Java.
    libs: HashMap<String, Rc<Template>>,
    /// The runtime `SerializedTemplateFactory.main` parsed every template with.
    build: Rc<RuntimeInstance>,
    extra_sources: HashMap<String, String>,
}

thread_local! {
    static MANAGER: RefCell<Manager> = RefCell::new(Manager {
        engines: HashMap::new(),
        templates: HashMap::new(),
        libs: HashMap::new(),
        build: RuntimeInstance::new(None, None),
        extra_sources: HashMap::new(),
    });
}

/// Adds a template resource beside the bundled ones (a `.vm` that would ship as `path + "s"`).
pub fn register_template_source(path: &str, source: &str) {
    MANAGER.with(|m| m.borrow_mut().extra_sources.insert(path.to_string(), source.to_string()));
}

fn source_for(path: &str) -> Option<String> {
    let extra = MANAGER.with(|m| m.borrow().extra_sources.get(path).cloned());
    extra.or_else(|| TEMPLATES.iter().find(|(p, _)| *p == path).map(|(_, s)| s.replace("\r\n", "\n")))
}

#[cfg(test)]
pub(super) fn tests_source(path: &str) -> String {
    source_for(path).unwrap()
}

#[cfg(test)]
pub(super) fn tests_lib_template(lib: &str) -> Rc<Template> {
    MANAGER.with(|m| m.borrow().libs[lib].clone())
}

fn logger() -> Box<dyn Fn(i32, &str)> {
    Box::new(|level, message| {
        if level == ERROR_ID
            && message != "VM #writeWatcher: error : too few arguments to macro. Wanted 1 got 0"
            && message != "VM #writeEvaluationWatcherPart: error : too few arguments to macro. Wanted 2 got 0"
            && message != "VM #writeWatcherBottom: error : too few arguments to macro. Wanted 1 got 0"
        {
            toolkit::log_plain(Level::Warning, message);
        }
    })
}

fn strict_uberspect() -> Box<dyn Fn(&UberspectMiss)> {
    Box::new(|m| {
        // CompilerMessage's own public line and column (-1) override the message's fields when it is localized.
        log_error(
            if m.is_set { "SetMethodNotFound" } else { "GetMethodNotFound" },
            if m.is_set {
                "${template} line ${line}, column ${column}: Set method for '${identifier}' not found on ${className}"
            } else {
                "${template} line ${line}, column ${column}: Get method for '${identifier}' not found on ${className}"
            },
            &[
                ("template", &m.info.template_name),
                ("line", "-1"),
                ("column", "-1"),
                ("identifier", m.identifier.unwrap_or("")),
                ("className", m.class_name),
            ],
        );
    })
}

fn create_engine(lib: Option<&str>) -> Result<Rc<RuntimeInstance>, VelocityError> {
    let ve = RuntimeInstance::new(Some(strict_uberspect()), Some(logger()));
    if let Some(lib) = lib {
        let t = Rc::new(load_template(lib, &ve)?);
        MANAGER.with(|m| m.borrow_mut().libs.insert(lib.to_string(), t));
    }
    Ok(ve)
}

fn get_engine(lib: Option<&str>) -> Result<Rc<RuntimeInstance>, VelocityError> {
    let lib_key = lib.unwrap_or("").to_string();
    if let Some(ve) = MANAGER.with(|m| m.borrow().engines.get(&lib_key).cloned()) {
        return Ok(ve);
    }
    let ve = create_engine(lib)?;
    MANAGER.with(|m| m.borrow_mut().engines.insert(lib_key, ve.clone()));
    Ok(ve)
}

/// `getTemplate(path)`.
pub fn get_template(path: &str) -> Result<Rc<Template>, VelocityError> {
    get_template_with_lib(path, None)
}

/// `getTemplate(path, lib)`: the template initialized in the engine that holds `lib`'s macros.
pub fn get_template_with_lib(path: &str, lib: Option<&str>) -> Result<Rc<Template>, VelocityError> {
    let ve = get_engine(lib)?;
    let key = format!("{}{}", path, lib.unwrap_or(""));
    if let Some(t) = MANAGER.with(|m| m.borrow().templates.get(&key).cloned()) {
        return Ok(t);
    }
    let t = Rc::new(load_template(path, &ve)?);
    MANAGER.with(|m| m.borrow_mut().templates.insert(key, t.clone()));
    Ok(t)
}

/// The private `getTemplate(path, ve)`: any failure is rethrown as a RuntimeException.
fn load_template(path: &str, ve: &Rc<RuntimeInstance>) -> Result<Template, VelocityError> {
    let load = || -> Result<Template, VelocityError> {
        let source =
            source_for(path).ok_or_else(|| VelocityError::Runtime("java.io.IOException: Stream closed".into()))?;
        let build = MANAGER.with(|m| m.borrow().build.clone());
        let t = Template::build(&source, path, &build)?;
        t.set_runtime_services(ve);
        t.init_document()?;
        Ok(t)
    };
    load().map_err(|e| VelocityError::Runtime(format!("java.lang.RuntimeException: {e}")))
}

/// `LocalizationManager.replaceInlineReferences`, which formats the compiler messages below.
fn replace_inline_references(text: &str, parameters: &[(&str, &str)]) -> Option<String> {
    let mut text = text.to_string();
    let mut depth = 100;
    while depth > 0 {
        depth -= 1;
        let mut o = jstr::index_of(&text, "${", 0);
        if o == -1 {
            break;
        }
        if o >= 1 && jstr::char_at(&text, o - 1) == b'$' as u16 {
            o = jstr::index_of(&text, "${", o + 2);
            if o == -1 {
                break;
            }
        }
        let c = jstr::index_of(&text, "}", o);
        if c == -1 {
            return None;
        }
        let name = jstr::substring(&text, o + 2, c);
        let value = parameters.iter().find(|(k, _)| *k == name).map(|(_, v)| *v).unwrap_or("");
        text = format!("{}{}{}", jstr::substring(&text, 0, o), value, jstr::substring_from(&text, c + 1));
    }
    Some(text.replace("$${", "${"))
}

/// Logs a `VelocityManager` CompilerError; ConsoleLogger falls back to the class name when the text cannot be formatted.
fn log_error(class: &str, text: &str, parameters: &[(&str, &str)]) {
    let text = replace_inline_references(text, parameters)
        .unwrap_or_else(|| format!("flex2.compiler.util.VelocityManager${class}"));
    toolkit::log(CompilerMessage::error(text), None, -1, -1, None);
}

/// `getCodeGenContext(util)`: a context holding `$util`, with the util's event handlers attached.
pub fn get_code_gen_context(util: Option<Rc<Util>>) -> VelocityContext {
    let cx = VelocityContext::new();
    let util = util.unwrap_or_else(|| Rc::new(Util::new()));
    let ec = EventCartridge {
        rieh: Some(util.clone() as Rc<dyn ReferenceInsertionEventHandler>),
        nseh: None,
        meeh: Some(util.clone() as Rc<dyn MethodExceptionEventHandler>),
    };
    cx.attach_event_cartridge(Rc::new(ec));
    cx.put(UTIL_KEY, Value::Object(util));
    cx
}

static UTIL_METHODS: &[Method] = &[
    Method::new("Now", &[], JType::Long),
    Method::new("getTimeStamp", &[], JType::String),
    Method::new("referenceInsert", &[JType::String, JType::Object], JType::Object),
];

/// `VelocityManager.Util`, optionally extended by a subclass's own members (such as mxml's `VelocityUtil`), which come first.
pub struct Util {
    extension: Option<Rc<dyn JavaObject>>,
    methods: Vec<Method>,
    /// `getTimeStamp()`'s text; `None` formats the current time in UTC.
    pub time_stamp: Option<String>,
}

impl Default for Util {
    fn default() -> Self {
        Util::new()
    }
}

impl Util {
    pub fn new() -> Util {
        Util { extension: None, methods: UTIL_METHODS.to_vec(), time_stamp: None }
    }

    pub fn with_extension(extension: Rc<dyn JavaObject>) -> Util {
        let mut methods = extension.methods().to_vec();
        methods.extend(UTIL_METHODS.iter().cloned());
        Util { extension: Some(extension), methods, time_stamp: None }
    }

    fn now() -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
    }

    /// `SimpleDateFormat("yyyy.MM.dd HH:mm:ss z")` of `Now()`, in UTC.
    fn time_stamp_text(&self) -> String {
        if let Some(t) = &self.time_stamp {
            return t.clone();
        }
        let secs = Self::now().div_euclid(1000);
        let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        format!("{:04}.{:02}.{:02} {:02}:{:02}:{:02} UTC", y, m, d, rem / 3600, rem % 3600 / 60, rem % 60)
    }
}

impl JavaObject for Util {
    fn class_name(&self) -> &str {
        match &self.extension {
            Some(e) => e.class_name(),
            None => "flex2.compiler.util.VelocityManager$Util",
        }
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == "flex2.compiler.util.VelocityManager$Util"
            || self.extension.as_ref().is_some_and(|e| e.instance_of(class_name))
    }

    fn methods(&self) -> &[Method] {
        &self.methods
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        if let Some(e) = &self.extension {
            if e.methods().contains(method) {
                return e.invoke(method, args);
            }
        }
        Ok(match &*method.name {
            "Now" => Value::Long(Self::now()),
            "getTimeStamp" => Value::str(&self.time_stamp_text()),
            "referenceInsert" => self.reference_insert(&args[0].to_java_string(), args[1].clone()),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        match &self.extension {
            Some(e) => e.to_java_string(),
            None => format!("{}@{:x}", self.class_name(), self as *const Self as usize),
        }
    }
}

impl MethodExceptionEventHandler for Util {
    fn method_exception(&self, class_name: &str, method: &str, e: &JavaException) -> Result<Value, JavaException> {
        log_error(
            "InvocationError",
            "Method '${className}.${methodName}': invocation exception during codegen: ${message}",
            &[("className", class_name), ("methodName", method), ("message", e.message.as_deref().unwrap_or(""))],
        );
        Ok(Value::Null)
    }
}

impl ReferenceInsertionEventHandler for Util {
    fn reference_insert(&self, reference: &str, value: Value) -> Value {
        if value.is_null() {
            log_error("TemplateReferenceIsNull", "Template reference '${s}' yields null.", &[("s", reference)]);
        }
        value
    }
}
