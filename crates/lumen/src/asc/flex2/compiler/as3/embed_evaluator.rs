//! Port of `flex2.compiler.as3.EmbedEvaluator`: transcodes class-level `[Embed]`s into the unit's assets, and turns
//! variable-level ones into generated asset classes the variables are initialized with.

use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use super::embed_util::{self, Uncaught};
use super::evaluator_adapter::{walk, Adapter};
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::source::{Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::compiler::transcoder::{self, Args, ResultsRef, Transcoders};
use crate::asc::flex2::compiler::util::{mime_mappings, name_formatter, MultiName, Name, QName};
use crate::asc::parser::node::{GetExpressionNode, IdentifierNode, Kind, LiteralStringNode, MemberExpressionNode, NodeRef};
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::JHashMap;

thread_local! {
    /// Classes whose class-level `[Embed]` is left as the target SWF already binds it, instead of being transcoded.
    pub static KEEP_EMBEDDED: std::cell::RefCell<HashSet<String>> = std::cell::RefCell::new(HashSet::new());
}

/// `EmbedData`: the variable-level embeds of one class.
struct EmbedData {
    reference_class_name: String,
    class2params: JHashMap<String, Args>,
    in_use: bool,
}

impl EmbedData {
    fn new() -> EmbedData {
        EmbedData { reference_class_name: String::new(), class2params: JHashMap::new(), in_use: false }
    }

    fn has_data(&self) -> bool {
        !self.class2params.is_empty()
    }
}

/// `cx.localizedError2(...)`: logged at the message's own level, and always counted as an error by ASC.
fn error2_at(cx: &ContextRef, level: Level, path: &str, ln: i32, col: i32, text: String, source: Option<&str>) {
    toolkit::log(CompilerMessage { level, text, path: None, line: -1, col: -1 }, Some(path), ln, col, source);
    let statics = cx.borrow().statics.clone();
    statics.borrow_mut().err_count += 1;
}

fn error2(cx: &ContextRef, level: Level, pos: i32, text: String) {
    let pos = pos.max(0);
    let (ln, col, source, origin) = {
        let c = cx.borrow();
        (c.get_input_line(pos), c.get_input_col(pos), c.get_error_line_text(pos), c.get_error_origin())
    };
    error2_at(cx, level, &origin, ln, col, text, Some(&source));
}

const EMBED_ONLY_ON_CLASSES_AND_VARS: &str = "Embed is only supported on classes and member variables.";

pub struct EmbedEvaluator<'a> {
    unit: UnitRef,
    generated_output_dir: Option<String>,
    check_deprecation: bool,
    transcoders: Transcoders,
    embed_data_stack: Vec<EmbedData>,
    evaluated_classes: HashSet<*const std::cell::RefCell<crate::asc::parser::node::Node>>,
    symbol_table: &'a SymbolTable,
}

impl<'a> EmbedEvaluator<'a> {
    pub fn new(unit: UnitRef, symbol_table: &'a SymbolTable, transcoders: Transcoders, generated_output_dir: Option<String>, check_deprecation: bool) -> EmbedEvaluator<'a> {
        EmbedEvaluator { unit, symbol_table, generated_output_dir, transcoders, check_deprecation, embed_data_stack: Vec::new(), evaluated_classes: HashSet::new() }
    }

    fn class_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        if !self.evaluated_classes.insert(Rc::as_ptr(node)) {
            return;
        }
        let (package_name, class_name, statements) = {
            let b = node.borrow();
            let c = b.class_definition().unwrap();
            let pkg = get_package_name(&c.def.pkgdef);
            let name = c.name.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default();
            (pkg, name, c.statements.clone())
        };
        if self.embed_data_stack.last().is_none_or(|d| d.in_use) {
            self.embed_data_stack.push(EmbedData::new());
        }
        {
            let d = self.embed_data_stack.last_mut().unwrap();
            d.in_use = true;
            d.reference_class_name = class_name;
        }
        let depth = self.embed_data_stack.len();
        if let Some(s) = statements {
            super::evaluator_adapter::evaluate(self, cx, &s);
        }
        let has = self.embed_data_stack.get(depth - 1).is_some_and(|d| d.has_data());
        if has {
            let sources = self.generate_sources(&package_name, cx);
            self.unit.borrow_mut().add_generated_sources(sources);
            let d = &mut self.embed_data_stack[depth - 1];
            d.class2params = JHashMap::new();
        }
        let popped = self.embed_data_stack.pop();
        if let Some(mut d) = popped {
            d.in_use = false;
        }
    }

    fn meta_data(&mut self, cx: &ContextRef, node: &NodeRef) {
        let (id, def, pos) = {
            let b = node.borrow();
            let m = b.as_meta_data().unwrap();
            (m.get_id(), m.def.clone(), b.pos())
        };
        if id.as_deref() != Some("Embed") {
            return;
        }
        let input = cx.borrow().input.clone();
        let (file, line, col) = match &input {
            Some(i) => {
                let mut i = i.borrow_mut();
                (i.origin.clone().unwrap_or_default(), i.get_ln_num(pos), i.get_col_pos(pos))
            }
            None => (cx.borrow().script_name().unwrap_or_default(), -1, -1),
        };
        let has_input = input.is_some();
        let put_file = |values: &mut Args| {
            if !values.contains_key(transcoder::FILE) {
                if file.contains('\\') {
                    values.insert(transcoder::FILE.to_string(), file.replace('\\', "/"));
                    values.insert(transcoder::PATHSEP.to_string(), "true".to_string());
                } else {
                    values.insert(transcoder::FILE.to_string(), file.clone());
                }
                if has_input {
                    values.insert(transcoder::LINE.to_string(), line.to_string());
                    values.insert(transcoder::COLUMN.to_string(), col.to_string());
                }
            }
        };
        let Some(def) = def else {
            error2(cx, Level::Error, pos, "The metadata requires a definition.".to_string());
            return;
        };
        let kind = match &def.borrow().kind {
            Kind::VariableDefinition(_) => 1,
            Kind::ClassDefinition(_) => 2,
            _ => 0,
        };
        match kind {
            1 => {
                let item = {
                    let b = def.borrow();
                    let v = b.as_variable_definition().unwrap();
                    v.list.as_ref().and_then(|l| l.borrow().list().and_then(|l| l.items.first().cloned().flatten()))
                };
                let Some(item) = item else { return };
                if item.borrow().as_variable_binding().is_none() {
                    return;
                }
                let has_initializer = item.borrow().as_variable_binding().unwrap().initializer.is_some();
                if has_initializer {
                    error2(cx, Level::Error, pos, "An Embed variable must not have an existing value.".to_string());
                    return;
                }
                let Some(reference) = self.embed_data_stack.last().map(|d| d.reference_class_name.clone()) else {
                    error2(cx, Level::Error, pos, EMBED_ONLY_ON_CLASSES_AND_VARS.to_string());
                    return;
                };
                let name = {
                    let b = item.borrow();
                    let vb = b.as_variable_binding().unwrap();
                    let var = vb.variable.as_ref().unwrap().borrow();
                    let t = var.as_typed_identifier().unwrap();
                    let id = t.identifier.as_ref().unwrap().borrow();
                    id.identifier().map(|i| i.name.clone()).unwrap_or_default()
                };
                let class_name = if name.starts_with("_embed_css_") { format!("_class{name}") } else { format!("{reference}_{name}") };
                self.unit.borrow_mut().expressions.insert(Name::M(MultiName::parse(&name_formatter::to_colon(&class_name))));
                let mut values = self.get_meta_data_values(node, cx);
                put_file(&mut values);
                self.embed_data_stack.last_mut().unwrap().class2params.insert(class_name.clone(), values);
                let type_ = lookup_type(&item);
                let mut correct_type = false;
                match type_.as_deref() {
                    Some("Class") => {
                        correct_type = true;
                        item.borrow_mut().as_variable_binding_mut().unwrap().initializer = Some(generate_class_initializer(&class_name));
                    }
                    Some("String") => {
                        correct_type = true;
                        item.borrow_mut().as_variable_binding_mut().unwrap().initializer = Some(generate_string_initializer(&class_name));
                    }
                    _ => {}
                }
                if !correct_type {
                    error2(cx, Level::Error, pos, "Embed only supported with variables of type Class or String.".to_string());
                }
            }
            2 => {
                let cls = {
                    let b = def.borrow();
                    let c = b.class_definition().unwrap();
                    let pkg = get_package_name(&c.def.pkgdef);
                    let name = c.name.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default();
                    if pkg.is_empty() { name } else { format!("{pkg}.{name}") }
                };
                if KEEP_EMBEDDED.with(|k| k.borrow().contains(&cls)) {
                    return;
                }
                let mut values = self.get_meta_data_values(node, cx);
                put_file(&mut values);
                let asset = embed_util::transcode(&self.transcoders, &self.unit, Some(self.symbol_table), &cls, &mut values, line, col, false);
                match asset {
                    Ok(None) if values.contains_key(transcoder::SOURCE) => {
                        error2(cx, Level::Error, pos, format!("Unable to transcode {}.", values.get(transcoder::SOURCE).unwrap()));
                    }
                    Err(Uncaught(_)) => error2(cx, Level::Error, pos, format!("Unable to create source {cls}.")),
                    _ => {}
                }
            }
            _ => error2(cx, Level::Error, pos, EMBED_ONLY_ON_CLASSES_AND_VARS.to_string()),
        }
    }

    /// `getMetaDataValues(node, context)`.
    fn get_meta_data_values(&self, node: &NodeRef, cx: &ContextRef) -> Args {
        let (pairs, pos) = {
            let b = node.borrow();
            let m = b.as_meta_data().unwrap();
            let n = m.count();
            let md = m.get_metadata();
            let mut pairs = Vec::new();
            for i in 0..n {
                let v = md.as_ref().and_then(|md| md.values()).and_then(|v| v.get(i as usize).cloned());
                let (key, value) = match v {
                    Some(crate::asc::semantics::value::Value::KeyValuePair(kv)) => (Some(kv.key.clone()), kv.obj.clone()),
                    Some(crate::asc::semantics::value::Value::KeylessValue(k)) => (None, k.obj.clone()),
                    _ => (None, String::new()),
                };
                pairs.push((key, value));
            }
            (pairs, b.pos())
        };
        let mut values = Args::new();
        for (key, value) in pairs {
            if key.as_deref().is_none_or(|k| k == transcoder::SOURCE) {
                match value.find('#') {
                    Some(octothorpe) => {
                        let src = self.unit.borrow().get_source();
                        if source_resolve(&src, &value).is_some() {
                            values.insert(transcoder::SOURCE.to_string(), value);
                        } else {
                            values.insert(transcoder::SOURCE.to_string(), value[..octothorpe].to_string());
                            values.insert(transcoder::SYMBOL.to_string(), value[octothorpe + 1..].to_string());
                        }
                    }
                    None => {
                        values.insert(transcoder::SOURCE.to_string(), value);
                    }
                }
            } else {
                values.insert(key.unwrap(), value);
            }
        }
        if self.check_deprecation && (values.contains_key("flashType") || values.contains_key("flash-type")) {
            let (deprecated, replacement) = if values.contains_key("flashType") { ("flashType", "advancedAntiAliasing") } else { ("flash-type", "advanced-anti-aliasing") };
            error2(cx, Level::Warning, pos, format!("{deprecated} has been deprecated since 3.0. Please use {replacement}."));
        }
        values
    }

    /// `generateSources(packageName, cx, node)`.
    fn generate_sources(&mut self, package_name: &str, cx: &ContextRef) -> JHashMap<QName, SourceRef> {
        let mut sources: JHashMap<QName, SourceRef> = JHashMap::new();
        let entries: Vec<String> = self.embed_data_stack.last().unwrap().class2params.keys().cloned().collect();
        for class_name in entries {
            let mut params = self.embed_data_stack.last_mut().unwrap().class2params.remove(&class_name).unwrap();
            self.generate_sources_for(&mut sources, package_name, &class_name, &mut params, cx);
            self.embed_data_stack.last_mut().unwrap().class2params.insert(class_name, params);
        }
        sources
    }

    fn generate_sources_for(&mut self, sources: &mut JHashMap<QName, SourceRef>, package_name: &str, class_name: &str, embed_map: &mut Args, cx: &ContextRef) {
        let line = embed_map.get(transcoder::LINE).and_then(|l| l.parse::<i32>().ok()).unwrap_or(-1);
        let col = embed_map.get(transcoder::COLUMN).and_then(|l| l.parse::<i32>().ok()).unwrap_or(-1);
        let mut path = embed_map.get(transcoder::FILE).cloned().unwrap_or_default();
        if embed_map.get(transcoder::PATHSEP).map(String::as_str) == Some("true") {
            path = path.replace('/', "\\");
        }
        let package_prefix = if package_name.is_empty() { String::new() } else { format!("{package_name}.") };
        let name_for_reporting = path.clone();
        let full = format!("{package_prefix}{class_name}");
        match embed_util::transcode(&self.transcoders, &self.unit, Some(self.symbol_table), &full, embed_map, line, col, true) {
            Ok(Some(asset)) => self.generate_source(sources, &asset, package_name, class_name),
            Ok(None) => {
                if let Some(what) = embed_map.get(transcoder::SOURCE) {
                    error2_at(cx, Level::Error, &name_for_reporting, line, col, format!("Unable to transcode {what}."), Some(""));
                }
            }
            Err(Uncaught(_)) => error2_at(cx, Level::Error, &name_for_reporting, line, col, format!("Unable to create source {full}."), None),
        }
    }

    /// `generateSource(sources, asset, packageName, className)`, recursing into the additional assets.
    fn generate_source(&mut self, sources: &mut JHashMap<QName, SourceRef>, asset: &ResultsRef, package_name: &str, class_name: &str) {
        let package_prefix = if package_name.is_empty() { String::new() } else { format!("{package_name}.") };
        let generated_name = format!("{package_prefix}{class_name}").replace('.', std::path::MAIN_SEPARATOR_STR) + ".as";
        let (code, modified) = {
            let a = asset.borrow();
            (a.generated_code.clone().unwrap_or_default(), a.asset_source.as_ref().map_or(-1, |s| s.get_last_modified()))
        };
        if let Some(dir) = &self.generated_output_dir {
            write_class_to_file(dir, &package_prefix, &format!("{class_name}.as"), &code);
        }
        let relative_path = package_name.replace('.', "/");
        let file = VirtualFile::InMemory { bytes: Rc::new(code.into_bytes()), name: generated_name, mime_type: mime_mappings::AS, last_modified: modified };
        let source = Source::new(file, None, &relative_path, class_name, Owner::None, false, false, false);
        sources.insert(QName::new(package_name, class_name), source);
        let children: Vec<ResultsRef> = asset.borrow().additional_assets.clone().unwrap_or_default();
        for child in children {
            let q = child.borrow().class_name.clone();
            if let Some(q) = q {
                let (pkg, cls) = match q.rfind('.') {
                    None => (String::new(), q.clone()),
                    Some(dot) => (q[..dot].to_string(), q[dot + 1..].to_string()),
                };
                self.generate_source(sources, &child, &pkg, &cls);
            }
        }
    }
}

/// `NodeMagic.getPackageName(pkgdef)`.
fn get_package_name(pkgdef: &Option<NodeRef>) -> String {
    let Some(p) = pkgdef else { return String::new() };
    let p = p.borrow();
    let Some(name) = p.as_package_definition().and_then(|p| p.name.clone()) else { return String::new() };
    let name = name.borrow();
    let Some(id) = name.as_package_name().and_then(|n| n.id.clone()) else { return String::new() };
    let mut id = id.borrow_mut();
    id.to_identifier_string().unwrap_or_default()
}

/// `FileUtils.writeClassToFile(baseDir, packageName, className, str)`.
fn write_class_to_file(base_dir: &str, package_prefix: &str, file_name: &str, code: &str) {
    let mut dir = Path::new(base_dir).to_path_buf();
    for part in package_prefix.split('.').filter(|p| !p.is_empty()) {
        dir.push(part);
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(file_name), code);
}

/// `Source.resolve(include)` through `Source.Resolver` and the toolkit resolver.
fn source_resolve(src: &SourceRef, relative: &str) -> Option<VirtualFile> {
    use crate::asc::flex2::compiler::transcoder::SinglePathResolver;
    let s = src.borrow();
    if !relative.starts_with('/') {
        if let VirtualFile::Local { name, .. } = s.get_backing_file() {
            if let Some(f) = SinglePathResolver::LocalFile(name.clone()).resolve(relative) {
                return Some(f);
            }
        }
    }
    if let Some(root) = s.get_path_root() {
        if let Some(f) = SinglePathResolver::LocalFile(root.to_string()).resolve(relative) {
            return Some(f);
        }
    }
    SinglePathResolver::Toolkit.resolve(relative)
}

/// `NodeMagic.lookupType(variableBinding)`.
fn lookup_type(binding: &NodeRef) -> Option<String> {
    let b = binding.borrow();
    let vb = b.as_variable_binding()?;
    let var = vb.variable.as_ref()?.borrow();
    let t = var.as_typed_identifier()?.type_.clone()?;
    let member = {
        let tb = t.borrow();
        if let Some(te) = tb.as_type_expression() {
            te.expr.clone()?
        } else if tb.as_member_expression().is_some() {
            t.clone()
        } else {
            return None;
        }
    };
    let mb = member.borrow();
    let sel = mb.as_member_expression()?.selector.clone()?;
    let sb = sel.borrow();
    let get = sb.as_get_expression()?;
    let id = get.sel.get_identifier()?;
    let name = id.borrow().identifier().map(|i| i.name.clone());
    name
}

/// `generateClassInitializer(name)`.
fn generate_class_initializer(name: &str) -> NodeRef {
    let identifier = IdentifierNode::new(name, 0);
    let get = GetExpressionNode::new(Some(identifier));
    get.borrow_mut().set_pos(0);
    MemberExpressionNode::new(None, Some(get), 0)
}

/// `generateStringInitializer(name)`.
fn generate_string_initializer(name: &str) -> NodeRef {
    let s = LiteralStringNode::new(name);
    s.borrow_mut().set_pos(0);
    s
}

impl Adapter for EmbedEvaluator<'_> {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let k = match &node.borrow().kind {
            Kind::ClassDefinition(_) => 1,
            Kind::MetaData(_) => 2,
            Kind::Program(_) => 3,
            _ => 0,
        };
        match k {
            1 => self.class_definition(cx, node),
            2 => self.meta_data(cx, node),
            3 => {
                self.embed_data_stack = Vec::new();
                walk(self, cx, node);
            }
            _ => return false,
        }
        true
    }
}
