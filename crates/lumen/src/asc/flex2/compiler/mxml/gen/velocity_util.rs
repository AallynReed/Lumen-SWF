//! Port of `flex2.compiler.mxml.gen.VelocityUtil`, the `$util` of the MXML templates. Java maps lines against the
//! `SourceCodeBuffer` while the template writes into it; here each `mapLines` call leaves a marker in the output, and
//! [`merge`] turns the markers into the same mappings once the text is complete.

use std::cell::RefCell;
use std::rc::Rc;

use super::line_number_map::DualModeLineNumberMap;
use crate::asc::flex2::velocity::{self, JType, JavaException, JavaObject, Method, Template, Value, VelocityError};

const MARK_START: char = '\u{FDD0}';
const MARK_END: char = '\u{FDD1}';

pub struct VelocityUtil {
    path: String,
    debug: bool,
    line_number_map: Option<Rc<DualModeLineNumberMap>>,
    /// `(origLine, extent, compileOnly)` of each `mapLines` call, by marker index.
    pending: RefCell<Vec<(i32, i32, bool)>>,
}

static METHODS: &[Method] = &[
    Method::new("getTemplatePath", &[], JType::String),
    Method::new("getDebug", &[], JType::Boolean),
    Method::new("getLineMappingEnabled", &[], JType::Boolean),
    // void in Java; the marker they return is removed by `merge`.
    Method::new("mapLines", &[JType::Int, JType::String], JType::String),
    Method::new("mapCompileErrorLines", &[JType::Int, JType::String], JType::String),
    Method::new("mapLines", &[JType::Int, JType::String, JType::Boolean], JType::String),
];

/// `StringUtils.countLines`.
pub fn count_lines(buffer: &str) -> i32 {
    buffer.matches('\n').count() as i32
}

impl VelocityUtil {
    /// `new VelocityUtil(path, debug, sourceCodeBuffer, lineNumberMap)`; mapping is enabled when a map is given.
    pub fn new(path: &str, debug: bool, line_number_map: Option<Rc<DualModeLineNumberMap>>) -> VelocityUtil {
        VelocityUtil { path: path.to_string(), debug, line_number_map, pending: RefCell::new(Vec::new()) }
    }

    pub fn get_template_path(&self) -> &str {
        &self.path
    }

    pub fn get_debug(&self) -> bool {
        self.debug
    }

    pub fn get_line_mapping_enabled(&self) -> bool {
        self.line_number_map.is_some()
    }

    fn map_lines(&self, orig_line: i32, text: &str, compile_only: bool) -> String {
        if self.get_line_mapping_enabled() && orig_line > 0 {
            let mut p = self.pending.borrow_mut();
            p.push((orig_line, count_lines(text) + 1, compile_only));
            return format!("{MARK_START}{}{MARK_END}", p.len() - 1);
        }
        String::new()
    }

    /// Strips the markers from `out`, recording each mapping at the line the marker was written on.
    fn resolve(&self, out: &str) -> String {
        let pending = self.pending.borrow();
        let mut result = String::with_capacity(out.len());
        let mut current_line = 1;
        let mut rest = out;
        while let Some(i) = rest.find(MARK_START) {
            let before = &rest[..i];
            current_line += count_lines(before);
            result.push_str(before);
            let after = &rest[i + MARK_START.len_utf8()..];
            let end = after.find(MARK_END).unwrap_or(after.len());
            if let (Ok(n), Some(map)) = (after[..end].parse::<usize>(), &self.line_number_map) {
                let (orig_line, extent, compile_only) = pending[n];
                map.put(orig_line, current_line, extent, compile_only);
            }
            rest = &after[(end + MARK_END.len_utf8()).min(after.len())..];
        }
        result.push_str(rest);
        result
    }
}

impl JavaObject for VelocityUtil {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.gen.VelocityUtil"
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == "flex2.compiler.mxml.gen.VelocityUtil" || class_name == "flex2.compiler.util.VelocityManager$Util"
    }

    fn methods(&self) -> &[Method] {
        METHODS
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        let int = |v: &Value| match v {
            Value::Integer(i) => *i,
            Value::Long(l) => *l as i32,
            _ => 0,
        };
        Ok(match (&*method.name, args.len()) {
            ("getTemplatePath", _) => Value::str(&self.path),
            ("getDebug", _) => Value::Boolean(self.debug),
            ("getLineMappingEnabled", _) => Value::Boolean(self.get_line_mapping_enabled()),
            ("mapLines", 2) => Value::str(&self.map_lines(int(&args[0]), &args[1].to_java_string(), false)),
            ("mapCompileErrorLines", _) => Value::str(&self.map_lines(int(&args[0]), &args[1].to_java_string(), true)),
            ("mapLines", _) => {
                let compile_only = matches!(args.get(2), Some(Value::Boolean(true)));
                Value::str(&self.map_lines(int(&args[0]), &args[1].to_java_string(), compile_only))
            }
            _ => Value::Null,
        })
    }
}

/// `VelocityManager.getCodeGenContext(util)` for a `VelocityUtil`: the context, and the util to pass to [`merge`].
/// `time_stamp` replaces `$util.timeStamp`'s clock reading when given.
pub fn get_code_gen_context(util: VelocityUtil, time_stamp: Option<String>) -> (velocity::VelocityContext, Rc<VelocityUtil>) {
    let util = Rc::new(util);
    let mut base = velocity::Util::with_extension(util.clone());
    base.time_stamp = time_stamp;
    let cx = velocity::get_code_gen_context(Some(Rc::new(base)));
    (cx, util)
}

/// `template.merge(context, sourceCodeBuffer)`: the generated text, with the util's line mappings recorded.
pub fn merge(template: &Template, context: &velocity::VelocityContext, util: &VelocityUtil) -> Result<String, VelocityError> {
    let out = template.merge(context)?;
    Ok(util.resolve(&out))
}
