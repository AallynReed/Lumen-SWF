//! Rendering oracle: the Java harness renders the same spec through mxmlc.jar's VelocityManager (`VELOCITY_ORACLE/out`).

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::Value as J;

use crate::asc::flex2::compiler::toolkit;
use crate::asc::flex2::velocity::introspection::{JType, JavaException, JavaObject, Method, Value};
use crate::asc::flex2::velocity::{
    get_code_gen_context, get_template, get_template_with_lib, register_template_source, Util,
};

thread_local! {
    static CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static INSTANCES: RefCell<HashMap<String, Value>> = RefCell::new(HashMap::new());
    static OBJECTS: RefCell<serde_json::Map<String, J>> = RefCell::new(serde_json::Map::new());
    static CLASS_SIGS: RefCell<HashMap<String, Vec<(String, String)>>> = RefCell::new(HashMap::new());
}

fn jtype(t: &str) -> JType {
    match t {
        "void" => JType::Void,
        "int" => JType::Int,
        "long" => JType::Long,
        "boolean" => JType::Boolean,
        "java.lang.String" => JType::String,
        "java.lang.Object" => JType::Object,
        other => JType::Class(Cow::Owned(other.to_string())),
    }
}

fn method_of(sig: &str, ret: &str) -> Method {
    let open = sig.find('(').unwrap();
    let ps = &sig[open + 1..sig.len() - 1];
    let params: Vec<JType> = if ps.is_empty() { Vec::new() } else { ps.split(',').map(jtype).collect() };
    Method { name: Cow::Owned(sig[..open].to_string()), params: Cow::Owned(params), ret: jtype(ret) }
}

/// A generated harness class (`velo.Base` subclass): members by signature, calls with arguments recorded.
struct SpecObject {
    class_name: String,
    members: serde_json::Map<String, J>,
    sigs: Vec<String>,
    methods: Vec<Method>,
}

impl JavaObject for SpecObject {
    fn class_name(&self) -> &str {
        &self.class_name
    }

    fn methods(&self) -> &[Method] {
        &self.methods
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        let sig = &self.sigs[self.methods.iter().position(|m| m == method).unwrap()];
        let mut call = String::new();
        if !args.is_empty() {
            call = format!("{}(", method.name);
            for (k, a) in args.iter().enumerate() {
                if k > 0 {
                    call.push(',');
                }
                call.push_str(&a.to_java_string());
            }
            call.push(')');
            CALLS.with(|c| c.borrow_mut().push(call.clone()));
        }
        let Some(m) = self.members.get(sig) else {
            return Err(JavaException::new("java.lang.NullPointerException", None));
        };
        if let Some(t) = m.get("throw") {
            return Err(JavaException::new("java.lang.RuntimeException", t.as_str()));
        }
        let v = m.get("v").cloned().unwrap_or(J::Null);
        if !args.is_empty() && v.as_str() == Some("$ECHO") {
            return Ok(Value::str(&call));
        }
        Ok(value(&v))
    }

    fn to_java_string(&self) -> String {
        match self.members.get("toString()") {
            Some(m) => m["v"].as_str().unwrap().to_string(),
            None => format!("Spec@{}", self.class_name),
        }
    }
}

fn value(j: &J) -> Value {
    match j {
        J::Null => Value::Null,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => Value::Integer(n.as_i64().unwrap() as i32),
        J::String(s) => Value::str(s),
        J::Array(_) => panic!("bare array"),
        J::Object(m) => {
            if let Some(l) = m.get("long") {
                Value::Long(l.as_i64().unwrap())
            } else if let Some(l) = m.get("list") {
                Value::list(l.as_array().unwrap().iter().map(value).collect())
            } else if let Some(l) = m.get("iter") {
                Value::iterator(l.as_array().unwrap().iter().map(value).collect())
            } else if let Some(l) = m.get("map") {
                let entries = l.as_array().unwrap().iter().map(|kv| (value(&kv[0]), value(&kv[1]))).collect();
                Value::map_of("java.util.LinkedHashMap", entries)
            } else if let Some(id) = m.get("obj") {
                let id = id.as_str().unwrap().to_string();
                if let Some(v) = INSTANCES.with(|i| i.borrow().get(&id).cloned()) {
                    return v;
                }
                let def = OBJECTS.with(|o| o.borrow()[&id].clone());
                let class_name = def["class"].as_str().unwrap().to_string();
                let all = CLASS_SIGS.with(|c| c.borrow()[&class_name].clone());
                let (sigs, methods) =
                    all.iter().filter(|(s, _)| s != "toString()").map(|(s, r)| (s.clone(), method_of(s, r))).unzip();
                let o = SpecObject { class_name, members: def["members"].as_object().unwrap().clone(), sigs, methods };
                let v = Value::Object(Rc::new(o));
                INSTANCES.with(|i| i.borrow_mut().insert(id, v.clone()));
                v
            } else {
                panic!("bad spec value {j}")
            }
        }
    }
}

/// mxml.gen's `VelocityUtil` without a line-number map, as the harness constructs it.
struct VelocityUtilStub {
    path: String,
    debug: bool,
}

const VU_METHODS: &[Method] = &[
    Method::new("getTemplatePath", &[], JType::String),
    Method::new("getDebug", &[], JType::Boolean),
    Method::new("getLineMappingEnabled", &[], JType::Boolean),
    Method::new("mapLines", &[JType::Int, JType::String], JType::Void),
    Method::new("mapCompileErrorLines", &[JType::Int, JType::String], JType::Void),
    Method::new("mapLines", &[JType::Int, JType::String, JType::Boolean], JType::Void),
];

impl JavaObject for VelocityUtilStub {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.gen.VelocityUtil"
    }

    fn methods(&self) -> &[Method] {
        VU_METHODS
    }

    fn invoke(&self, method: &Method, _: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getTemplatePath" => Value::str(&self.path),
            "getDebug" => Value::Boolean(self.debug),
            "getLineMappingEnabled" => Value::Boolean(false),
            _ => Value::Null,
        })
    }
}

/// Masks what cannot match: the clock in `$util.timeStamp`, and the Java stack trace VelocimacroProxy logs.
fn mask_time(s: &str) -> String {
    let re = regex::Regex::new(r"\d{4}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2} \S+").unwrap();
    let s = re.replace_all(s, "<TIME>");
    let trace = regex::Regex::new(r"(exception VM = #\w+\(\) : )[\s\S]*?\n\n+").unwrap();
    trace.replace_all(&s, "$1<trace>\n\n").to_string()
}

fn exception_class(s: &str) -> &str {
    s.split(':').next().unwrap_or("")
}

#[test]
fn renders_like_java() {
    let Ok(dir) = std::env::var("VELOCITY_ORACLE") else { return };
    let spec: J = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/spec.json")).unwrap()).unwrap();
    OBJECTS.with(|o| *o.borrow_mut() = spec["objects"].as_object().unwrap().clone());
    let mut sigs: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for def in spec["objects"].as_object().unwrap().values() {
        let e = sigs.entry(def["class"].as_str().unwrap().to_string()).or_default();
        for (s, m) in def["members"].as_object().unwrap() {
            if !e.iter().any(|(x, _)| x == s) {
                e.push((s.clone(), m["ret"].as_str().unwrap().to_string()));
            }
        }
    }
    CLASS_SIGS.with(|c| *c.borrow_mut() = sigs);
    for src in spec["sources"].as_array().unwrap() {
        let name = src.as_str().unwrap();
        // ISO-8859-1, the build's input.encoding.
        let text: String = std::fs::read(format!("{dir}/{name}")).unwrap().iter().map(|&b| b as char).collect();
        register_template_source(name, &text);
    }
    let mut failed = Vec::new();
    let mut passed = 0;
    for case in spec["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        toolkit::reset(true);
        CALLS.with(|c| c.borrow_mut().clear());
        let mut w = String::new();
        let mut exception = String::new();
        let path = case["template"].as_str().unwrap();
        let t = match case["lib"].as_str() {
            None => get_template(path),
            Some(lib) => get_template_with_lib(path, Some(lib)),
        };
        match t {
            Err(e) => exception = e.to_string(),
            Ok(t) => {
                let util = case.get("util").filter(|u| !u.is_null()).map(|u| {
                    Rc::new(Util::with_extension(Rc::new(VelocityUtilStub {
                        path: u["path"].as_str().unwrap().to_string(),
                        debug: u["debug"].as_bool().unwrap(),
                    })))
                });
                let vc = get_code_gen_context(util);
                if let Some(ctx) = case.get("context").and_then(|c| c.as_object()) {
                    for (k, v) in ctx {
                        vc.put(k, value(v));
                    }
                }
                if let Err(e) = t.merge_into(&vc, &mut w) {
                    exception = e.to_string();
                }
            }
        }
        let mut out = w;
        out.push_str("\n----CALLS----\n");
        for c in CALLS.with(|c| c.borrow().clone()) {
            out.push_str(&c);
            out.push('\n');
        }
        out.push_str("----LOG----\n");
        for d in toolkit::take_diagnostics() {
            out.push_str(&d.text);
            out.push_str("\n\n");
        }
        let expected = std::fs::read_to_string(format!("{dir}/out/{name}.out")).unwrap();
        let (exp_body, exp_exc) = expected.split_once("----EXCEPTION----\n").unwrap();
        let ok = mask_time(exp_body) == mask_time(&out)
            && exception_class(exp_exc.trim_end()) == exception_class(&exception);
        out.push_str("----EXCEPTION----\n");
        out.push_str(&exception);
        out.push('\n');
        std::fs::write(format!("{dir}/out/{name}.rust"), &out).unwrap();
        if ok {
            passed += 1;
        } else {
            failed.push(name.to_string());
        }
    }
    eprintln!("velocity oracle: {passed} cases identical, {} differ", failed.len());
    for path in super::FLEX_TEMPLATES {
        let src = crate::asc::flex2::velocity::velocity_manager::tests_source(path);
        let lib = if path.ends_with("ClassDef.vm") { Some("flex2/compiler/mxml/gen/ClassDefLib.vm") } else { None };
        let t = if path.ends_with("ClassDefLib.vm") {
            crate::asc::flex2::velocity::velocity_manager::tests_lib_template(path)
        } else {
            get_template_with_lib(path, lib).unwrap()
        };
        let _ = src;
        let mut lines = Vec::new();
        super::unrendered_blocks(t.data(), &mut lines);
        eprintln!("unrendered blocks in {path}: {lines:?}");
    }
    assert!(failed.is_empty(), "differ: {failed:?}");
}
